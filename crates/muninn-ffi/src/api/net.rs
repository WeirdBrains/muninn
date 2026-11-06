//! Network status, peers, and writes (`mnnn net`): start/stop, connect, invites,
//! relays, sharing, settings, connections, exchanges, search, broadcast and dark shards.
//! Uses the unlocked key in Rust; the node identity, exchange tokens and shard blobs
//! never leave this crate. Invite links hold no secret.
use super::data::{open, open_write};
use super::info::paths;
use muninn_core::net::{self, Node, auto, client, identity, local};
use muninn_core::prefs::Preferences;
use muninn_core::store::{Exchange, ExchangeStatus, ShareKind, Store};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

/// What `mnnn net status` shows: the node and counts, no secrets.
pub struct NetStatus {
    pub label: String,
    pub peer_id: Option<String>,
    pub fingerprint: Option<String>,
    pub running: bool,
    pub pid: Option<i64>,
    pub started_at: Option<i64>,
    pub nat: String,
    pub mdns: bool,
    pub relay_server: bool,
    pub addresses: Vec<String>,
    pub reservations: Vec<String>,
    pub connected: i64,
    pub exchanges_active: i64,
    pub incoming: i64,
    pub shared: i64,
}

/// One known peer (`mnnn net peers`): public facts only.
pub struct NetPeerInfo {
    pub peer_id: String,
    pub short_id: String,
    pub label: String,
    pub user: String,
    pub caps: String,
    pub link: String,
    pub source: String,
    pub last_seen: i64,
}

pub struct NetOverview {
    pub status: NetStatus,
    pub peers: Vec<NetPeerInfo>,
}

fn status_of(store: &Store, home: &Path) -> anyhow::Result<NetStatus> {
    let prefs: Preferences = std::fs::read_to_string(home.join("preferences.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
    let label = local::my_label(store, &prefs.net);
    let running = local::running(home);
    let peer_id = match identity::peer_id(store)? {
        Some(p) => Some(p.to_string()),
        None => running.as_ref().map(|f| f.status.peer_id.clone()).filter(|s| !s.is_empty()),
    };
    let fingerprint = identity::fingerprint_of(store)?;
    let xs = store.exchanges()?;
    let exchanges_active = xs.iter().filter(|x| x.status.active()).count() as i64;
    let incoming = xs.iter().filter(|x| x.status == ExchangeStatus::Incoming).count() as i64;
    let shared = store.shares()?.len() as i64;
    let (nat, mdns, relay_server, addresses, reservations, connected, pid, started_at) = match &running {
        Some(f) => {
            let s = &f.status;
            (
                s.nat.clone(),
                s.mdns,
                s.relay_server,
                local::reachable(s),
                s.reservations.clone(),
                s.connected.len() as i64,
                Some(f.pid as i64),
                (s.started_at != 0).then_some(s.started_at),
            )
        }
        None => (String::new(), false, false, Vec::new(), Vec::new(), 0, None, None),
    };
    Ok(NetStatus {
        label,
        peer_id,
        fingerprint,
        running: running.is_some(),
        pid,
        started_at,
        nat,
        mdns,
        relay_server,
        addresses,
        reservations,
        connected,
        exchanges_active,
        incoming,
        shared,
    })
}

fn peers_of(store: &Store) -> anyhow::Result<Vec<NetPeerInfo>> {
    let mut out = Vec::new();
    for p in store.net_peers()? {
        let link = auto::node_link(store, &p.peer_id)?.map(|x| auto::status_of(&x).as_str().to_string()).unwrap_or_else(|| "-".into());
        let user = muninn_core::net::user::user_of_peer(store, &p.peer_id)?.map(|u| u.label).filter(|l| !l.is_empty()).unwrap_or_default();
        out.push(NetPeerInfo {
            short_id: p.short_id().to_string(),
            peer_id: p.peer_id,
            label: p.label,
            user,
            caps: client::caps_text(&p.caps),
            link,
            source: p.source,
            last_seen: p.last_seen,
        });
    }
    Ok(out)
}

pub(crate) fn net_overview_of(store: &Store, home: &Path) -> anyhow::Result<NetOverview> {
    Ok(NetOverview { status: status_of(store, home)?, peers: peers_of(store)? })
}

/// `mnnn net status` and `mnnn net peers`. The store is opened read-only.
pub fn net_overview(home: Option<String>) -> anyhow::Result<NetOverview> {
    net_overview_of(&open()?, &paths(home)?.home)
}


const ORIGIN: &str = "app";

/// The node this process started (TUI `Start node` / `mnnn net start`).
struct HostedDriver {
    node: Arc<Node>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

static HOSTED: LazyLock<Mutex<Option<HostedDriver>>> = LazyLock::new(|| Mutex::new(None));

fn prefs_of(home: &Path) -> Preferences {
    std::fs::read_to_string(home.join("preferences.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
}

fn edit_prefs(home: Option<String>, f: impl FnOnce(&mut Preferences)) -> anyhow::Result<()> {
    let p = paths(home)?;
    let mut prefs = Preferences::load_or_create(&p)?;
    f(&mut prefs);
    prefs.save(&p)?;
    Ok(())
}

fn reap_hosted() {
    let mut g = HOSTED.lock().unwrap_or_else(|e| e.into_inner());
    let dead = g.as_ref().is_some_and(|d| !d.node.running());
    if !dead {
        return;
    }
    if let Some(mut d) = g.take() {
        d.stop.store(true, Ordering::SeqCst);
        if let Some(t) = d.thread.take() {
            let _ = t.join();
        }
    }
}

pub(crate) fn hosted_node() -> Option<Arc<Node>> {
    reap_hosted();
    HOSTED.lock().ok()?.as_ref().filter(|d| d.node.running()).map(|d| d.node.clone())
}

fn link_of(l: &auto::Link) -> NetLinkResult {
    NetLinkResult {
        status: l.status.as_str().into(),
        peer: l.peer.clone(),
        peer_id: l.peer_id.clone(),
        detail: l.detail.clone(),
        warning: l.warning.clone(),
    }
}

fn parse_kind(k: &str) -> anyhow::Result<ShareKind> {
    ShareKind::parse(k).ok_or_else(|| anyhow::anyhow!("`{k}`: share a model, search, journal, project, report, code or pipeline"))
}

fn check_relay_addr(addr: &str) -> anyhow::Result<()> {
    if !addr.contains("/p2p/") {
        return Err(anyhow::anyhow!("a relay address ends in /p2p/PEER_ID"));
    }
    let p = muninn_core::prefs::NetPrefs { relays: vec![addr.to_string()], ..Default::default() };
    p.node_config("check")?;
    Ok(())
}

/// Result of connect / accept / hello (no keys).
pub struct NetLinkResult {
    pub status: String,
    pub peer: String,
    pub peer_id: Option<String>,
    pub detail: String,
    pub warning: Option<String>,
}

/// An invite this node made (`mnnn net invite`). The link holds no secret.
pub struct NetInviteInfo {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub purpose: String,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub link: String,
}

/// A request waiting for this node (`mnnn net requests`).
pub struct NetRequestInfo {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub purpose: String,
    pub peer_id: Option<String>,
    pub created_at: i64,
}

pub struct NetInvites {
    pub invites: Vec<NetInviteInfo>,
    pub requests: Vec<NetRequestInfo>,
}

pub struct NetRelayInfo {
    pub configured: Vec<String>,
    pub seen: Vec<NetPeerInfo>,
    pub serving: bool,
}

/// One item this node shares (`mnnn net shared`).
pub struct NetShareInfo {
    pub id: String,
    pub short_id: String,
    pub kind: String,
    pub name: String,
    pub with: String,
}

fn invite_info(x: &muninn_core::store::Exchange) -> NetInviteInfo {
    NetInviteInfo {
        id: x.id.clone(),
        short_id: x.short_id().to_string(),
        title: x.title(),
        purpose: auto::purpose(x),
        expires_at: x.expires_at,
        created_at: x.created_at,
        link: net::exchange::link(&x.offer),
    }
}

fn request_info(x: &muninn_core::store::Exchange) -> NetRequestInfo {
    NetRequestInfo {
        id: x.id.clone(),
        short_id: x.short_id().to_string(),
        title: x.title(),
        purpose: auto::purpose(x),
        peer_id: x.peer_id.clone(),
        created_at: x.created_at,
    }
}

pub(crate) fn invites_of(store: &Store) -> anyhow::Result<NetInvites> {
    let now = muninn_core::store::now_ms();
    let xs = store.exchanges()?;
    let invites = xs
        .iter()
        .filter(|x| x.role == "offerer" && x.status == ExchangeStatus::Offered && x.expires_at.is_none_or(|e| e > now))
        .map(invite_info)
        .collect();
    let requests = auto::requests(store)?.iter().map(request_info).collect();
    Ok(NetInvites { invites, requests })
}

pub(crate) fn shared_of(store: &Store) -> anyhow::Result<Vec<NetShareInfo>> {
    let mut out = Vec::new();
    for s in store.shares()? {
        let with = match &s.exchange_id {
            None => "everyone with an exchange".to_string(),
            Some(x) => store.exchange(x)?.map(|x| x.title()).unwrap_or_else(|| x.clone()),
        };
        out.push(NetShareInfo {
            short_id: s.short_id().to_string(),
            id: s.id,
            kind: s.kind.as_str().to_string(),
            name: s.name,
            with,
        });
    }
    Ok(out)
}

pub(crate) fn relays_of(store: &Store, home: &Path) -> anyhow::Result<NetRelayInfo> {
    let prefs = prefs_of(home);
    let mut seen = Vec::new();
    for p in store.net_peers()? {
        if !p.is_relay() {
            continue;
        }
        let link = auto::node_link(store, &p.peer_id)?.map(|x| auto::status_of(&x).as_str().to_string()).unwrap_or_else(|| "-".into());
        let user = muninn_core::net::user::user_of_peer(store, &p.peer_id)?.map(|u| u.label).filter(|l| !l.is_empty()).unwrap_or_default();
        seen.push(NetPeerInfo {
            short_id: p.short_id().to_string(),
            peer_id: p.peer_id,
            label: p.label,
            user,
            caps: client::caps_text(&p.caps),
            link,
            source: p.source,
            last_seen: p.last_seen,
        });
    }
    Ok(NetRelayInfo { configured: prefs.net.relays, seen, serving: prefs.net.relay_server })
}

pub(crate) fn invite_of(store: &Store, home: &Path, label: Option<&str>) -> anyhow::Result<String> {
    let prefs = prefs_of(home);
    let kp = identity::load_or_create(store)?;
    let my = local::my_label(store, &prefs.net);
    let node = hosted_node();
    let addrs = local::offer_addrs(home, store, node.as_deref());
    let c = client::create(store, node.as_deref(), &kp, label.unwrap_or(""), &my, addrs, None, client::DEFAULT_TTL_MS, &prefs.net.relays, ORIGIN)?;
    Ok(net::exchange::link(&c.token))
}

pub(crate) fn share_of(store: &Store, kind: &str, item: &str, with: &[String]) -> anyhow::Result<String> {
    let k = parse_kind(kind)?;
    if item.trim().is_empty() {
        return Err(anyhow::anyhow!("name the item after its kind, e.g. project Notes"));
    }
    let made = client::share(store, k, item, with, ORIGIN)?;
    let fresh = made.iter().filter(|(_, new)| *new).count();
    let s = &made[0].0;
    let who = if with.is_empty() { "every key exchange".to_string() } else { with.join(", ") };
    Ok(format!(
        "sharing {} {} with {who}{}",
        s.kind.label(),
        s.name,
        if fresh == 0 { " (already)" } else { "" }
    ))
}

pub(crate) fn unshare_of(store: &Store, kind: &str, item: &str, with: &[String]) -> anyhow::Result<String> {
    let gone = client::unshare(store, parse_kind(kind)?, item, with, ORIGIN)?;
    Ok(format!("stopped sharing {} {}", gone[0].kind.label(), gone[0].name))
}

fn stop_of(home: &Path) -> anyhow::Result<String> {
    reap_hosted();
    let mut g = HOSTED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut d) = g.take() {
        d.stop.store(true, Ordering::SeqCst);
        if let Some(t) = d.thread.take() {
            drop(g);
            let _ = t.join();
        }
        return Ok("node stopped".into());
    }
    drop(g);
    if local::running(home).is_some() {
        local::request_stop(home)?;
        return Ok("asked the running node to stop".into());
    }
    Ok("no node is running".into())
}

/// `mnnn net start`: run the node in this process until `net_stop` (or `mnnn net stop`).
/// The node identity stays in Rust. Does not start the org site (that stays an Orgs action).
pub fn net_start(home: Option<String>) -> anyhow::Result<String> {
    reap_hosted();
    {
        let g = HOSTED.lock().unwrap_or_else(|e| e.into_inner());
        if g.as_ref().is_some_and(|d| d.node.running()) {
            return Ok("the node is already running".into());
        }
    }
    let p = paths(home.clone())?;
    let store = open_write()?;
    let service = open_write()?;
    let listener = open_write()?;
    let spare = open_write()?;
    let tick_store = open_write()?;
    let prefs = prefs_of(&p.home);
    let kp = identity::load_or_create(&store)?;
    let label = local::my_label(&store, &prefs.net);
    let mut hosted = local::Hosted::start(&p.home, &store, service, &kp, &label, &prefs.net, ORIGIN)?
        .with_stream(&store, listener, &kp, &prefs.net, ORIGIN)
        .settle_with(spare, prefs.net.relays.clone());
    let node = hosted.node.clone();
    let peer = node.peer_id().to_string();
    let mut msg = format!("node {label} running as {peer}");
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let thread = thread::spawn(move || {
        while !stop2.load(Ordering::SeqCst) && hosted.tick(&tick_store) {
            thread::sleep(Duration::from_millis(250));
        }
        hosted.stop(&tick_store, ORIGIN);
    });
    *HOSTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(HostedDriver { node, stop, thread: Some(thread) });
    // Same as the TUI / `mnnn net start`: the site comes up with the node when hosting.autostart is on.
    if prefs.hosting.autostart {
        match super::orgs::org_host_start(home) {
            Ok(st) if st.running && !st.url.is_empty() => {
                msg = format!("{msg}; site at {}", st.url);
            }
            Ok(_) => {}
            Err(e) => {
                msg = format!("{msg}; site: {e}");
            }
        }
    }
    Ok(msg)
}

/// `mnnn net stop`: stop the node this app started, or ask another process to stop.
pub fn net_stop(home: Option<String>) -> anyhow::Result<String> {
    stop_of(&paths(home)?.home)
}

/// `mnnn net connect` (or an invite link). `hello` only says hello and remembers the peer.
pub fn net_connect(home: Option<String>, target: String, label: Option<String>, hello: bool) -> anyhow::Result<NetLinkResult> {
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(anyhow::anyhow!("connect to an address, peer id, name or invite link"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let relays = prefs.net.relays.clone();
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    if hello {
        let peer = client::hello(&store, node, &target, label.as_deref(), &relays)?;
        let short = peer.short_id().to_string();
        let name = if peer.label.is_empty() { short.clone() } else { peer.label };
        return Ok(NetLinkResult {
            status: "hello".into(),
            peer: name,
            peer_id: Some(peer.peer_id),
            detail: format!("{short} says hello: {}", peer.agent),
            warning: None,
        });
    }
    let addrs = local::offer_addrs(&p.home, &store, Some(node));
    if net::exchange::is_token(&net::exchange::unlink(&target)) {
        return Ok(link_of(&auto::use_link(&store, Some(node), &kp, &target, label.as_deref().unwrap_or(""), &my, addrs, ORIGIN)?));
    }
    let l = auto::connect(&store, node, &kp, &target, label.as_deref(), &my, addrs, &relays, ORIGIN)?;
    let out = link_of(&l);
    if l.status == auto::LinkStatus::Failed {
        return Err(anyhow::anyhow!("{}", l.detail));
    }
    Ok(out)
}

/// `mnnn net invite`: a public link the other node can paste. It holds no secret.
pub fn net_invite(home: Option<String>, label: Option<String>) -> anyhow::Result<String> {
    invite_of(&open_write()?, &paths(home)?.home, label.as_deref())
}

/// Open invites this node made, and requests waiting for it.
pub fn net_invites(home: Option<String>) -> anyhow::Result<NetInvites> {
    let _ = home;
    invites_of(&open()?)
}

/// `mnnn net accept` (a request id or an invite link).
pub fn net_accept(home: Option<String>, request: String) -> anyhow::Result<NetLinkResult> {
    let request = request.trim().to_string();
    if request.is_empty() {
        return Err(anyhow::anyhow!("accept a request id or paste an invite link"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let addrs = local::offer_addrs(&p.home, &store, Some(node));
    let l = if net::exchange::is_token(&net::exchange::unlink(&request)) {
        auto::use_link(&store, Some(node), &kp, &request, "", &my, addrs, ORIGIN)?
    } else {
        auto::accept(&store, Some(node), &kp, &request, &my, addrs, ORIGIN)?
    };
    Ok(link_of(&l))
}

/// `mnnn net decline`.
pub fn net_decline(home: Option<String>, request: String) -> anyhow::Result<String> {
    let _ = home;
    let x = auto::decline(&open_write()?, request.trim(), ORIGIN)?;
    Ok(format!("declined {} ({})", x.title(), auto::purpose(&x)))
}

/// Withdraw an invite this node made (or end a connection): `mnnn net disconnect` / TUI revoke.
pub fn net_disconnect(home: Option<String>, peer: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let key = match store.find_net_peer(&peer).ok().and_then(|peer| auto::node_link(&store, &peer.peer_id).ok().flatten()) {
        Some(x) => x.id,
        None => peer,
    };
    let owned = hosted_node();
    let (x, told) = client::revoke(&store, owned.as_deref(), &key, &prefs.net.relays, ORIGIN)?;
    let mut msg = format!("disconnected from {}", x.title());
    if let Some(Err(e)) = told {
        msg.push_str(&format!("; {} was not reachable ({})", x.title(), auto::plain(&e)));
    }
    Ok(msg)
}

/// `mnnn net forget`: drop a known peer (exchanges stay).
pub fn net_forget(home: Option<String>, peer: String) -> anyhow::Result<String> {
    let _ = home;
    let store = open_write()?;
    let p = store.find_net_peer(&peer)?;
    store.remove_net_peer(&p.peer_id)?;
    Ok(format!("forgot {}", if p.label.is_empty() { p.short_id().to_string() } else { p.label }))
}

/// Configured relays, seen relay peers, and whether this node will serve.
pub fn net_relays(home: Option<String>) -> anyhow::Result<NetRelayInfo> {
    relays_of(&open()?, &paths(home)?.home)
}

/// `mnnn net relay add`. Saved in preferences.toml (picked up the next time the node starts).
pub fn net_relay_add(home: Option<String>, addr: String) -> anyhow::Result<String> {
    let addr = addr.trim().to_string();
    check_relay_addr(&addr)?;
    edit_prefs(home, |prefs| {
        if !prefs.net.relays.contains(&addr) {
            prefs.net.relays.push(addr.clone());
        }
    })?;
    Ok(format!("using relay {addr} (from the next node start)"))
}

/// `mnnn net relay remove`.
pub fn net_relay_remove(home: Option<String>, relay: String) -> anyhow::Result<String> {
    let relay = relay.trim().to_string();
    let mut found = false;
    edit_prefs(home, |prefs| {
        let before = prefs.net.relays.len();
        prefs.net.relays.retain(|r| r != &relay && !r.ends_with(&relay) && !r.contains(&format!("/p2p/{relay}")));
        found = prefs.net.relays.len() != before;
    })?;
    if !found {
        return Err(anyhow::anyhow!("no configured relay matches `{relay}`"));
    }
    Ok("relay removed".into())
}

/// `mnnn net relay serve on|off`.
pub fn net_relay_serve(home: Option<String>, on: bool) -> anyhow::Result<String> {
    edit_prefs(home, |prefs| prefs.net.relay_server = on)?;
    Ok(if on { "this node will relay for others (from the next start)".into() } else { "this node will not relay".into() })
}

/// `mnnn net shared`.
pub fn net_shared(home: Option<String>) -> anyhow::Result<Vec<NetShareInfo>> {
    let _ = home;
    shared_of(&open()?)
}

/// `mnnn net share KIND ITEM [--with EXCHANGE…]`.
pub fn net_share(home: Option<String>, kind: String, item: String, with: Vec<String>) -> anyhow::Result<String> {
    let _ = home;
    share_of(&open_write()?, &kind, &item, &with)
}

/// `mnnn net unshare KIND ITEM [--with EXCHANGE…]`.
pub fn net_unshare(home: Option<String>, kind: String, item: String, with: Vec<String>) -> anyhow::Result<String> {
    let _ = home;
    unshare_of(&open_write()?, &kind, &item, &with)
}

/// `mnnn net config` / TUI Net ▸ Settings… (plus CLI extras the TUI page does not list).
pub struct NetConfig {
    pub label: String,
    pub port: u16,
    pub listen: Vec<String>,
    pub mdns: bool,
    pub relay_server: bool,
    pub autostart: bool,
    pub host_autostart: bool,
    pub accept: String,
    pub bootstrap: Vec<String>,
    pub external_addresses: Vec<String>,
    pub collab_transport: String,
    pub collab_bind: String,
    pub collab_port: u16,
    pub collab_advertise: Vec<String>,
}

fn config_of(home: &Path) -> NetConfig {
    let p = prefs_of(home);
    let n = &p.net;
    NetConfig {
        label: n.label.clone(),
        port: n.port,
        listen: n.listen.clone(),
        mdns: n.mdns,
        relay_server: n.relay_server,
        autostart: n.autostart,
        host_autostart: p.hosting.autostart,
        accept: if n.accept_anyone() { "anyone".into() } else { "known".into() },
        bootstrap: n.bootstrap.clone(),
        external_addresses: n.external_addresses.clone(),
        collab_transport: n.collab_transport.clone(),
        collab_bind: n.collab_bind.clone(),
        collab_port: n.collab_port,
        collab_advertise: n.collab_advertise.clone(),
    }
}

/// Read network settings from preferences.toml (does not create the file).
pub fn net_config(home: Option<String>) -> anyhow::Result<NetConfig> {
    Ok(config_of(&paths(home)?.home))
}

/// Save network settings (`mnnn net config`). Relays are left to the Relays page.
pub fn net_config_set(home: Option<String>, cfg: NetConfig) -> anyhow::Result<String> {
    let p = paths(home)?;
    let mut prefs = Preferences::load_or_create(&p)?;
    let n = &mut prefs.net;
    n.label = cfg.label.trim().to_string();
    n.port = cfg.port;
    n.listen = cfg.listen.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "default").collect();
    n.mdns = cfg.mdns;
    n.relay_server = cfg.relay_server;
    n.autostart = cfg.autostart;
    let accept = cfg.accept.trim().to_ascii_lowercase();
    if accept != "known" && accept != "anyone" {
        return Err(anyhow::anyhow!("--accept: `known` or `anyone`"));
    }
    n.accept = accept;
    n.bootstrap = cfg.bootstrap.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    n.external_addresses = cfg.external_addresses.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    n.collab_transport = muninn_core::collab::stream::Mode::parse(&cfg.collab_transport)?.as_str().into();
    let bind = cfg.collab_bind.trim().to_string();
    if bind.parse::<std::net::IpAddr>().is_err() {
        return Err(anyhow::anyhow!("collab bind: an IP address (like 0.0.0.0 or 127.0.0.1)"));
    }
    n.collab_bind = bind;
    n.collab_port = cfg.collab_port;
    n.collab_advertise = cfg.collab_advertise.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    n.node_config("check")?;
    prefs.hosting.autostart = cfg.host_autostart;
    prefs.save(&p)?;
    if let Ok(s) = open_write() {
        s.note_action(&muninn_core::store::NewAction::new("net.config", ORIGIN, "network settings changed").params(serde_json::to_value(&prefs.net)?));
    }
    let later = if hosted_node().is_some() { " (applies when the node starts again)" } else { "" };
    Ok(format!("saved network settings{later}"))
}

pub struct NetMeInfo {
    pub user_id: String,
    pub short_id: String,
    pub name: String,
    pub nodes: i64,
}

pub struct NetConnNode {
    pub short_id: String,
    pub label: String,
    pub active: bool,
    pub state: String,
}

pub struct NetConnectionInfo {
    pub user_id: String,
    pub short_id: String,
    pub name: String,
    pub trusted: bool,
    pub nodes: i64,
    pub since: i64,
    pub node_list: Vec<NetConnNode>,
}

pub struct NetConnections {
    pub me: Option<NetMeInfo>,
    pub connections: Vec<NetConnectionInfo>,
}

fn connections_of(s: &Store) -> anyhow::Result<NetConnections> {
    let me = s.users()?.into_iter().find(|u| u.mine).map(|u| {
        let nodes = s.user_nodes(&u.id).ok().map(|n| n.iter().filter(|x| x.active()).count() as i64).unwrap_or(0);
        NetMeInfo { short_id: u.short_id().to_string(), name: u.name(), user_id: u.id, nodes }
    });
    let mut connections = Vec::new();
    for c in s.connections()? {
        let Some(u) = s.user(&c.user_id)? else { continue };
        let node_list = s
            .user_nodes(&u.id)?
            .into_iter()
            .map(|n| {
                let active = n.active();
                let state = match n.revoked_at {
                    Some(t) => format!("revoked {}", muninn_core::ingest::time::fmt_ts(t)),
                    None => format!("linked {}", muninn_core::ingest::time::fmt_ts(n.issued_at)),
                };
                NetConnNode { short_id: muninn_core::store::net::short_peer(&n.peer_id).to_string(), label: n.label, active, state }
            })
            .collect::<Vec<_>>();
        let nodes = node_list.iter().filter(|n| n.active).count() as i64;
        connections.push(NetConnectionInfo {
            short_id: u.short_id().to_string(),
            name: u.name(),
            user_id: u.id,
            trusted: c.trusted,
            nodes,
            since: c.created_at,
            node_list,
        });
    }
    Ok(NetConnections { me, connections })
}

/// `mnnn connection list` / TUI Net ▸ Connections…. No user keys leave Rust.
pub fn net_connections(home: Option<String>) -> anyhow::Result<NetConnections> {
    let _ = home;
    connections_of(&open()?)
}

/// `mnnn connection trust` / TUI `t`.
pub fn net_connection_trust(home: Option<String>, user: String, trusted: bool) -> anyhow::Result<String> {
    let _ = home;
    let (u, changed) = muninn_core::net::user::set_trust(&open_write()?, user.trim(), trusted, ORIGIN)?;
    let word = if trusted { "trusted" } else { "untrusted" };
    Ok(if changed { format!("{} is now {word}", u.name()) } else { format!("{} was already {word}", u.name()) })
}

/// `mnnn connection remove` / TUI `d`.
pub fn net_connection_remove(home: Option<String>, user: String) -> anyhow::Result<String> {
    let _ = home;
    let u = muninn_core::net::user::remove_connection(&open_write()?, user.trim(), ORIGIN)?;
    Ok(format!("connection with {} removed", u.name()))
}

pub struct NetExchangeInfo {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub label: String,
    pub status: String,
    pub role: String,
    pub peer_id: String,
    pub peer_label: String,
    pub short_code: Option<String>,
    pub fingerprint: Option<String>,
    pub expires_at: Option<i64>,
    pub verified_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
    pub active: bool,
    pub incoming: bool,
}

fn exchange_info(x: &Exchange) -> NetExchangeInfo {
    NetExchangeInfo {
        short_id: x.short_id().to_string(),
        title: x.title(),
        label: x.label.clone(),
        status: x.status.as_str().into(),
        role: x.role.clone(),
        peer_id: x.peer_id.as_deref().map(muninn_core::store::net::short_peer).unwrap_or_default().to_string(),
        peer_label: x.peer_label.clone(),
        short_code: x.short_code.clone(),
        fingerprint: x.fingerprint.clone(),
        expires_at: x.expires_at,
        verified_at: x.verified_at,
        revoked_at: x.revoked_at,
        created_at: x.created_at,
        active: x.status.active(),
        incoming: x.status == ExchangeStatus::Incoming,
        id: x.id.clone(),
    }
}

fn exchanges_of(s: &Store) -> anyhow::Result<Vec<NetExchangeInfo>> {
    Ok(s.exchanges()?.iter().map(exchange_info).collect())
}

/// `mnnn net exchange list` / TUI Net ▸ Nodes…. Tokens stay in Rust; short codes are meant to be read aloud.
pub fn net_exchanges(home: Option<String>) -> anyhow::Result<Vec<NetExchangeInfo>> {
    let _ = home;
    exchanges_of(&open()?)
}

/// `mnnn net exchange label`.
pub fn net_exchange_label(home: Option<String>, exchange: String, label: String) -> anyhow::Result<String> {
    let _ = home;
    let s = open_write()?;
    let x = client::exchange_for(&s, exchange.trim())?;
    s.label_exchange(&x.id, &label)?;
    Ok(format!("exchange {} is now {label}", x.short_id()))
}

/// `mnnn net exchange verify`.
pub fn net_exchange_verify(home: Option<String>, exchange: String, code: String) -> anyhow::Result<String> {
    let _ = home;
    let s = open_write()?;
    let x = client::exchange_for(&s, exchange.trim())?;
    let x = client::verify(&s, &x.id, code.trim(), ORIGIN)?;
    Ok(format!("key exchange with {} verified", x.title()))
}

/// `mnnn net exchange revoke` / TUI disconnect on a node.
pub fn net_exchange_revoke(home: Option<String>, exchange: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let x = client::exchange_for(&store, exchange.trim())?;
    let owned = hosted_node();
    let (x, told) = client::revoke(&store, owned.as_deref(), &x.id, &prefs.net.relays, ORIGIN)?;
    let mut msg = format!("disconnected from {}", x.title());
    if let Some(Err(e)) = told {
        msg.push_str(&format!("; {} was not reachable ({})", x.title(), auto::plain(&e)));
    }
    Ok(msg)
}

pub struct NetRemoteItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub about: String,
}

/// `mnnn net list` / TUI Nodes ▸ L. Needs a running or ephemeral node.
pub fn net_remote_list(home: Option<String>, exchange: String) -> anyhow::Result<Vec<NetRemoteItem>> {
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let x = client::exchange_for(&store, exchange.trim())?;
    if !x.status.active() {
        return Err(anyhow::anyhow!("{} is {}", x.title(), x.status.as_str()));
    }
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let items = client::remote_list(&store, node, &x, &prefs.net.relays)?;
    Ok(items
        .into_iter()
        .map(|i| NetRemoteItem { id: i.id, kind: i.kind.label().into(), name: i.name, about: i.about })
        .collect())
}

/// `mnnn net fetch` / TUI Enter on a remote item. Bytes stay in Rust (import or write [path]).
pub fn net_fetch(home: Option<String>, exchange: String, item: String, format: Option<String>, path: Option<String>, import: bool) -> anyhow::Result<String> {
    let p = paths(home)?;
    let mut store = open_write()?;
    let prefs = prefs_of(&p.home);
    let x = client::exchange_for(&store, exchange.trim())?;
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let it = client::fetch(&store, node, &x, item.trim(), format.as_deref(), &prefs.net.relays, ORIGIN)?;
    let mut parts = Vec::new();
    if import {
        let made = client::import_item(&mut store, &it, &x.title(), ORIGIN)?;
        parts.push(format!("imported {made}"));
    }
    if let Some(path) = path.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        let n = it.data.len();
        std::fs::write(&path, &it.data)?;
        parts.push(format!("wrote {path} ({n} bytes)"));
    }
    if parts.is_empty() {
        return Err(anyhow::anyhow!("choose Import or a file to write (the item stays in Rust)"));
    }
    Ok(parts.join("; "))
}

pub struct NetEntrustedInfo {
    pub set_id: String,
    pub index: u32,
    pub threshold: u32,
    pub total: u32,
    pub exchange: String,
    pub status: String,
}

pub struct NetHeldInfo {
    pub id: String,
    pub short_id: String,
    pub set_id: String,
    pub index: u32,
    pub threshold: u32,
    pub total: u32,
    pub owner: String,
    pub exchange: String,
}

pub struct NetShards {
    pub entrusted: Vec<NetEntrustedInfo>,
    pub held: Vec<NetHeldInfo>,
}

fn shards_of(s: &Store) -> anyhow::Result<NetShards> {
    let mut entrusted = Vec::new();
    for e in s.entrusted()? {
        let (exchange, status) = match s.exchange(&e.exchange_id)? {
            Some(x) => (x.title(), x.status.as_str().to_string()),
            None => (String::new(), String::new()),
        };
        entrusted.push(NetEntrustedInfo { set_id: e.set_id, index: e.index, threshold: e.threshold, total: e.total, exchange, status });
    }
    let mut held = Vec::new();
    for h in s.held_shards()? {
        let owner = if h.owner_label.is_empty() { muninn_core::store::net::short_peer(&h.owner_peer).to_string() } else { h.owner_label };
        held.push(NetHeldInfo {
            short_id: muninn_core::store::projects::short_of(&h.id).to_string(),
            set_id: h.set_id,
            index: h.index,
            threshold: h.threshold,
            total: h.total,
            owner,
            exchange: h.exchange_id,
            id: h.id,
        });
    }
    Ok(NetShards { entrusted, held })
}

/// `mnnn net shard list` / TUI Dark shards. Sealed blobs stay in Rust.
pub fn net_shards(home: Option<String>) -> anyhow::Result<NetShards> {
    let _ = home;
    shards_of(&open()?)
}

fn master_from_key_file(home: &std::path::Path, file: &str, pass: Option<String>) -> anyhow::Result<muninn_core::crypto::Key32> {
    let paths = muninn_core::paths::Paths::resolve(Some(home.to_path_buf()), None)?;
    let text = muninn_core::crypto::keyring::read_maybe_age(std::path::Path::new(file), || {
        pass.clone().filter(|s| !s.is_empty()).map(secrecy::SecretString::from).ok_or_else(|| {
            muninn_core::Error::missing("file password", "this key file is encrypted: type its password")
        })
    })?;
    let t = text.trim();
    let m = if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        muninn_core::crypto::Key32::from_hex(t)?
    } else {
        muninn_core::crypto::from_words(t)?
    };
    muninn_core::crypto::keyring::verify_master(&paths, &m)?;
    Ok(m)
}

/// `mnnn net shard entrust` / TUI Entrust…. Master comes from a key file; shard password is not stored.
pub fn net_shard_entrust(
    home: Option<String>,
    to: Vec<String>,
    threshold: u32,
    key_file: String,
    file_password: Option<String>,
    shard_password: String,
) -> anyhow::Result<String> {
    if to.len() < 2 {
        return Err(anyhow::anyhow!("pick at least two connected nodes: each holds one shard"));
    }
    if shard_password.is_empty() {
        return Err(anyhow::anyhow!("type a shard password: you need it again to restore"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let master = master_from_key_file(&p.home, key_file.trim(), file_password)?;
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let e = client::entrust(&store, node, &master, &secrecy::SecretString::from(shard_password), &to, threshold as usize, &prefs.net.relays, ORIGIN)?;
    let mut msg = format!("entrusted: any {} of {} shards restore your key (set {})", e.threshold, e.placed.len(), muninn_core::store::projects::short_of(&e.set));
    for pl in &e.placed {
        let who = store.exchange(&pl.exchange_id)?.map(|x| x.title()).unwrap_or_default();
        msg.push_str(&format!("; {who} holds shard {}/{}", pl.index, pl.total));
    }
    Ok(msg)
}

/// TUI “Check restore”: gather shards and verify they rebuild this database's key. The key never leaves Rust.
pub fn net_shard_check(home: Option<String>, shard_password: String) -> anyhow::Result<String> {
    if shard_password.is_empty() {
        return Err(anyhow::anyhow!("type your shard password to check the shards restore your key"));
    }
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let (key, g) = client::gather(&store, node, &secrecy::SecretString::from(shard_password), None, &[], &prefs.net.relays, ORIGIN)?;
    muninn_core::crypto::keyring::verify_master(&p, &key)?;
    let mut msg = format!("your shards restore this database's key ({} answered, {} needed: {})", g.got, g.threshold, g.from.join(", "));
    for e in &g.errors {
        msg.push_str(&format!("; {e}"));
    }
    Ok(msg)
}

/// `mnnn net shard drop` / TUI `d` on a held shard.
pub fn net_shard_drop(home: Option<String>, id: String) -> anyhow::Result<String> {
    let _ = home;
    let s = open_write()?;
    let held = s.held_shards()?;
    let want = id.trim();
    let h = held
        .iter()
        .find(|h| h.id == want || h.id.starts_with(want))
        .ok_or_else(|| anyhow::anyhow!("no held shard `{want}`"))?;
    let owner = if h.owner_label.is_empty() { h.owner_peer.clone() } else { h.owner_label.clone() };
    let set = h.set_id.clone();
    s.drop_held_shard(&h.id)?;
    s.note_action(
        &muninn_core::store::NewAction::new("net.shard.drop", ORIGIN, "a held dark shard dropped").params(serde_json::json!({ "peer": owner, "set": set })),
    );
    Ok("dropped".into())
}

pub struct NetSearchOrigin {
    pub origin: String,
    pub peer: Option<String>,
    pub exchange: Option<String>,
    pub error: Option<String>,
    pub truncated: bool,
    pub lines: Vec<String>,
}

/// `mnnn net search` / TUI Network search… (runs a saved live search here and on peers).
pub fn net_search(home: Option<String>, saved: String, only: Vec<String>, remote_only: bool, limit: u32) -> anyhow::Result<Vec<NetSearchOrigin>> {
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let name = saved.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("pick a saved live search to run across your peers"));
    }
    let spec = store.find_live_search(name)?.spec;
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let cap = if limit == 0 { 200 } else { limit as usize };
    let res = client::search(&store, Some(node), &spec, &only, !remote_only, cap, &prefs.net.relays, ORIGIN)?;
    Ok(res
        .into_iter()
        .map(|o| NetSearchOrigin {
            origin: o.origin,
            peer: o.peer,
            exchange: o.exchange,
            error: o.error,
            truncated: o.truncated,
            lines: o.found.iter().map(|f| f.line()).collect(),
        })
        .collect())
}

/// `mnnn net broadcast`.
pub fn net_broadcast(home: Option<String>, topic: String, text: String, to: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let store = open_write()?;
    let prefs = prefs_of(&p.home);
    let dest = muninn_core::net::broadcast::To::parse(&store, to.trim())?;
    if muninn_core::net::broadcast::targets(&store, &dest)?.is_empty() {
        return Err(anyhow::anyhow!("no one to tell: no active {} exchanges", dest.text(&store)));
    }
    let kp = identity::load_or_create(&store)?;
    let my = local::my_label(&store, &prefs.net);
    let owned = hosted_node();
    let ephemeral;
    let node: &Node = if let Some(n) = owned.as_ref() {
        n
    } else {
        ephemeral = local::ephemeral(&kp, open_write()?, &my, &prefs.net)?;
        &ephemeral
    };
    let by = muninn_core::net::broadcast::From { node, label: &my, relays: &prefs.net.relays };
    let r = muninn_core::net::broadcast::send(&store, &by, &dest, topic.trim(), &text, ORIGIN)?;
    let mut msg = format!("sent to {} node(s)", r.sent.len());
    for (who, e) in &r.failed {
        msg.push_str(&format!("; {who}: {e}"));
    }
    Ok(msg)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::data::lock;
    use muninn_core::net::identity;
    use muninn_core::net::node::Status;
    use muninn_core::store::Store;

    #[test]
    fn empty_store_is_stopped_with_no_identity() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let o = net_overview_of(&s, home.path()).unwrap();
        assert!(!o.status.running);
        assert!(o.status.peer_id.is_none() && o.status.fingerprint.is_none());
        assert_eq!(o.status.label, "muninn");
        assert_eq!((o.status.exchanges_active, o.status.shared, o.peers.len()), (0, 0, 0));
        assert!(!home.path().join("preferences.toml").exists(), "status must not create preferences");
    }

    #[test]
    fn identity_is_read_not_created() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        assert!(net_overview_of(&s, home.path()).unwrap().status.peer_id.is_none());
        let kp = identity::load_or_create(&s).unwrap();
        let o = net_overview_of(&s, home.path()).unwrap();
        assert_eq!(o.status.peer_id.as_deref(), Some(kp.public().to_peer_id().to_string().as_str()));
        assert_eq!(o.status.fingerprint.as_deref(), Some(identity::fingerprint(&kp.public()).as_str()));
    }

    #[test]
    fn running_node_file_and_known_peers() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let st = Status {
            peer_id: "12D3KooWabc".into(),
            listening: vec!["/ip4/1.2.3.4/tcp/4741".into()],
            connected: vec!["12D3KooWxyz".into()],
            nat: "public".into(),
            mdns: true,
            relay_server: false,
            started_at: 1_704_067_200_000,
            ..Default::default()
        };
        local::write(home.path(), "desk", &st).unwrap();
        s.upsert_net_peer("12D3KooWpeerXXXX", Some("lab"), &[], Some(&["muninn".into()]), None, "mdns").unwrap();
        let o = net_overview_of(&s, home.path()).unwrap();
        assert!(o.status.running);
        assert!(o.status.pid.is_some_and(|p| p > 0));
        assert_eq!(o.status.nat, "public");
        assert!(o.status.mdns && !o.status.relay_server);
        assert_eq!(o.status.connected, 1);
        assert!(o.status.addresses.iter().any(|a| a.contains("1.2.3.4")), "{:?}", o.status.addresses);
        assert_eq!(o.status.started_at, Some(1_704_067_200_000));
        assert_eq!(o.peers.len(), 1);
        assert_eq!((o.peers[0].label.as_str(), o.peers[0].source.as_str(), o.peers[0].link.as_str()), ("lab", "mdns", "-"));
        assert_eq!(o.peers[0].caps, "muninn");
        assert!(o.peers[0].short_id.len() <= 8);
    }

    #[test]
    fn overview_needs_unlock() {
        lock();
        assert!(net_overview(None).is_err());
    }

    #[test]
    fn writes_need_unlock() {
        lock();
        assert!(net_start(None).is_err());
        assert!(net_invite(None, None).is_err());
        assert!(net_share(None, "project".into(), "X".into(), vec![]).is_err());
        assert!(net_connect(None, "x".into(), None, false).is_err());
    }

    #[test]
    fn invite_link_is_public_and_listed() {
        let s = Store::open_in_memory().unwrap();
        let home = tempfile::tempdir().unwrap();
        let link = invite_of(&s, home.path(), Some("ben")).unwrap();
        assert!(link.starts_with("muninn://join/"), "{link}");
        assert!(muninn_core::net::exchange::is_token(&muninn_core::net::exchange::unlink(&link)));
        assert!(!link.to_lowercase().contains("secret") && !link.contains("private"), "{link}");
        let inv = invites_of(&s).unwrap();
        assert_eq!(inv.invites.len(), 1);
        assert_eq!(inv.invites[0].link, link);
        assert!(inv.requests.is_empty());
        assert!(identity::peer_id(&s).unwrap().is_some(), "invite may create the node identity in the store");
    }

    #[test]
    fn share_and_unshare_a_project() {
        let s = Store::open_in_memory().unwrap();
        s.create_project("Alpha", "").unwrap();
        let msg = share_of(&s, "project", "Alpha", &[]).unwrap();
        assert!(msg.contains("Alpha") && msg.contains("project"), "{msg}");
        let shared = shared_of(&s).unwrap();
        assert_eq!(shared.len(), 1);
        assert_eq!((shared[0].kind.as_str(), shared[0].name.as_str()), ("project", "Alpha"));
        assert!(shared[0].with.contains("everyone"));
        assert!(share_of(&s, "nope", "Alpha", &[]).is_err());
        assert!(share_of(&s, "project", "   ", &[]).is_err());
        let gone = unshare_of(&s, "project", "Alpha", &[]).unwrap();
        assert!(gone.contains("Alpha"));
        assert!(shared_of(&s).unwrap().is_empty());
    }

    #[test]
    fn relay_add_requires_peer_id_and_saves() {
        let s = Store::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().display().to_string();
        assert!(check_relay_addr("/ip4/1.2.3.4/tcp/4741").is_err());
        assert!(net_relay_add(Some(home.clone()), "/ip4/1.2.3.4/tcp/4741".into()).is_err());
        assert!(check_relay_addr("/ip4/1.2.3.4/tcp/4741/p2p/12D3KooWC21S2Wxq3UCGcZYTsipGKjRitEar4nFARuvzpiUv2HH2").is_ok());
        // write via edit_prefs (creates preferences.toml)
        assert!(net_relay_add(Some(home.clone()), "/ip4/1.2.3.4/tcp/4741/p2p/12D3KooWC21S2Wxq3UCGcZYTsipGKjRitEar4nFARuvzpiUv2HH2".into()).is_ok());
        let r = relays_of(&s, dir.path()).unwrap();
        assert_eq!(r.configured.len(), 1);
        assert!(!r.serving);
        assert!(net_relay_serve(Some(home.clone()), true).is_ok());
        assert!(relays_of(&s, dir.path()).unwrap().serving);
        assert!(net_relay_remove(Some(home.clone()), "12D3KooWC21S2Wxq3UCGcZYTsipGKjRitEar4nFARuvzpiUv2HH2".into()).is_ok());
        assert!(relays_of(&s, dir.path()).unwrap().configured.is_empty());
        assert!(net_relay_remove(Some(home), "missing".into()).is_err());
    }

    #[test]
    fn stop_idle_or_ask_the_other_process() {
        let home = tempfile::tempdir().unwrap();
        assert!(stop_of(home.path()).unwrap().contains("no node"));
        let st = muninn_core::net::node::Status {
            peer_id: "12D3KooWabc".into(),
            started_at: 1_704_067_200_000,
            ..Default::default()
        };
        local::write(home.path(), "desk", &st).unwrap();
        assert!(local::running(home.path()).is_some());
        let msg = stop_of(home.path()).unwrap();
        assert!(msg.contains("asked"), "{msg}");
        assert!(local::stop_requested(home.path()));
    }

    #[test]
    fn start_refused_when_another_process_is_running() {
        lock();
        assert!(net_start(None).is_err());
    }

    #[test]
    fn config_round_trip_does_not_create_prefs_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().display().to_string();
        let c = config_of(dir.path());
        assert!(c.mdns && c.autostart && c.accept == "known");
        assert!(!dir.path().join("preferences.toml").exists(), "reading config must not create preferences");
        let mut n = c;
        n.label = "desk".into();
        n.mdns = false;
        n.accept = "anyone".into();
        n.external_addresses = vec!["example.com:4741".into()];
        assert!(net_config_set(Some(home.clone()), n).is_ok());
        let got = config_of(dir.path());
        assert_eq!(got.label, "desk");
        assert!(!got.mdns);
        assert_eq!(got.accept, "anyone");
        assert_eq!(got.external_addresses, vec!["example.com:4741"]);
    }

    #[test]
    fn connections_exchanges_and_shards_list_without_secrets() {
        let s = Store::open_in_memory().unwrap();
        assert!(connections_of(&s).unwrap().connections.is_empty());
        assert!(exchanges_of(&s).unwrap().is_empty());
        let sh = shards_of(&s).unwrap();
        assert!(sh.entrusted.is_empty() && sh.held.is_empty());
    }

    #[test]
    fn leftover_writes_need_unlock() {
        lock();
        assert!(net_connection_trust(None, "x".into(), true).is_err());
        assert!(net_exchanges(None).is_err());
        assert!(net_shards(None).is_err());
        assert!(net_broadcast(None, "t".into(), "".into(), "nodes".into()).is_err());
        assert!(net_search(None, "x".into(), vec![], false, 200).is_err());
    }
}
