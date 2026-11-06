//! Network status, peers, and writes (`mnnn net`): start/stop, connect, invites,
//! relays and sharing. Uses the unlocked key in Rust; the node identity never
//! leaves this crate. Invite links hold no secret.
use super::data::{open, open_write};
use super::info::paths;
use muninn_core::net::{self, Node, auto, client, identity, local};
use muninn_core::prefs::Preferences;
use muninn_core::store::{ExchangeStatus, ShareKind, Store};
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
}
