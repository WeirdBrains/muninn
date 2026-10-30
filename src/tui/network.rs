//! Net ▸ … (K): the node, peers, relays, connections, shared data, dark shards,
//! network searches and settings. Thin over [`crate::net`]; anything that talks to
//! other nodes runs on a worker thread so the window stays usable.
use super::forms::{Form, FormEvent, expand};
use super::widgets::Input;
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, record, row_style};
use super::{App, Hit};
use crate::net::proto::{Item, SharedItem};
use crate::net::{Node, auto, client, exchange, identity, local, user};
use crate::store::users::User;
use crate::store::{Entrusted, Exchange, ExchangeStatus, Held, LiveSearch, NetPeer, Share, ShareKind, Store};
use libp2p::identity::Keypair;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::sync::{Arc, mpsc};

const ORIGIN: &str = "tui";

/// The window's pages, in order (Tab / ← → / 1-8 switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Node,
    Peers,
    Relays,
    Exchanges,
    Shared,
    Shards,
    Search,
    Settings,
    /// The users your nodes know (2.10), trusted or not.
    Users,
    /// Invite links you made and requests waiting for you.
    Invites,
}

impl Tab {
    pub const ALL: [Tab; 10] =
        [Tab::Node, Tab::Peers, Tab::Relays, Tab::Exchanges, Tab::Shared, Tab::Shards, Tab::Search, Tab::Settings, Tab::Users, Tab::Invites];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Node => "Node",
            Tab::Peers => "Peers",
            Tab::Relays => "Relays",
            Tab::Exchanges => "Nodes",
            Tab::Shared => "Shared",
            Tab::Shards => "Dark shards",
            Tab::Search => "Search",
            Tab::Settings => "Settings",
            Tab::Users => "Connections",
            Tab::Invites => "Invites",
        }
    }
    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// What a prompt is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NAsk {
    Connect,
    PeerLabel,
    RelayAdd,
    Offer,
    Accept,
    Verify,
    ExchangeLabel,
    /// Share with every exchange (false) or the selected one (true).
    Share(bool),
    Setting(Setting),
}

/// Something that needs `y` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Forget(String),
    Revoke(String),
    DropHeld(String),
    /// Forget a connection (user id).
    RemoveConnection(String),
    /// Let a node that asks to link act as you (exchange id).
    ApproveLink(String),
    /// Withdraw an invite link you made (exchange id).
    RevokeInvite(String),
}

/// The editable settings, in the order shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Label,
    Port,
    Listen,
    Mdns,
    RelayServer,
    Autostart,
    HostAutostart,
    Accept,
    Bootstrap,
}

const SETTINGS: [Setting; 9] = [
    Setting::Label,
    Setting::Port,
    Setting::Listen,
    Setting::Mdns,
    Setting::RelayServer,
    Setting::Autostart,
    Setting::HostAutostart,
    Setting::Accept,
    Setting::Bootstrap,
];

/// A worker's result.
pub enum Done {
    Lines(Vec<String>),
    Remote { exchange: String, title: String, items: Vec<SharedItem> },
    Fetched { from: String, item: Item },
    Searched { name: String, results: Vec<client::Origin> },
}

pub struct Job {
    pub what: String,
    rx: mpsc::Receiver<Result<Done, String>>,
}

/// What another node shares with us (from `L` on an exchange).
pub struct Remote {
    pub exchange: String,
    pub title: String,
    pub items: Vec<SharedItem>,
    pub sel: usize,
}

#[derive(Default)]
pub struct NetState {
    pub tab: Tab,
    pub sel: usize,
    /// The node this TUI runs (Start node).
    pub hosted: Option<local::Hosted>,
    kp: Option<Keypair>,
    pub ask: Option<Ask<NAsk>>,
    pub confirm: Option<Confirm>,
    pub msg: Option<(String, bool)>,
    /// Lines above the list: a token, details, search results.
    pub detail: Vec<String>,
    pub job: Option<Job>,
    pub remote: Option<Remote>,
    pub peers: Vec<NetPeer>,
    pub exchanges: Vec<Exchange>,
    pub shares: Vec<Share>,
    pub entrusted: Vec<Entrusted>,
    pub held: Vec<Held>,
    pub searches: Vec<LiveSearch>,
    /// You, then your connections: (user, trusted, linked nodes).
    pub me: Option<User>,
    pub users: Vec<(User, bool, usize)>,
    /// Open invites you made, then requests waiting for you.
    pub invites: Vec<Exchange>,
    pub requests: Vec<Exchange>,
    /// Dark shards ▸ entrust (e): open while picking nodes and typing passwords.
    pub entrust: Option<EntrustForm>,
}

impl NetState {
    pub fn typing(&self) -> bool {
        self.ask.is_some() || self.entrust.is_some()
    }
    pub fn node_running(&self) -> bool {
        self.hosted.as_ref().is_some_and(|h| h.node.running())
    }
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.network.msg = Some((m.into(), false));
}

fn err(app: &mut App, m: impl Into<String>) {
    app.network.msg = Some((m.into(), true));
}

/// Open the window on a page.
pub fn open(app: &mut App, tab: Tab) {
    let st = &mut app.network;
    st.ask = None;
    st.confirm = None;
    st.msg = None;
    st.detail.clear();
    st.remote = None;
    st.entrust = None;
    if st.tab != tab {
        st.sel = 0;
    }
    st.tab = tab;
    reload(app);
    app.modal = Some(super::modal::Modal::Network);
}

/// Open Connections with the invite-or-connect prompt (menu: Net ▸ Invite or connect…).
pub fn open_invite(app: &mut App) {
    open(app, Tab::Invites);
    app.network.ask = Some(Ask::new(NAsk::Offer, "Invite link (Enter) or connect to (address, peer id)", ""));
}

fn reload(app: &mut App) {
    let s = &app.store;
    let st = &mut app.network;
    st.peers = s.net_peers().unwrap_or_default();
    st.exchanges = s.exchanges().unwrap_or_default();
    st.shares = s.shares().unwrap_or_default();
    st.entrusted = s.entrusted().unwrap_or_default();
    st.held = s.held_shards().unwrap_or_default();
    st.searches = s.live_searches().unwrap_or_default();
    st.me = user::load_or_create(s).ok().map(|(u, _)| u);
    st.users = s
        .connections()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let u = s.user(&c.user_id).ok().flatten()?;
            let n = s.user_nodes(&u.id).ok()?.iter().filter(|x| x.active()).count();
            Some((u, c.trusted, n))
        })
        .collect();
    let now = crate::store::now_ms();
    st.invites = st
        .exchanges
        .iter()
        .filter(|x| x.role == "offerer" && x.status == ExchangeStatus::Offered && x.expires_at.is_none_or(|e| e > now))
        .cloned()
        .collect();
    st.requests = auto::requests(s).unwrap_or_default();
    let n = rows(app);
    app.network.sel = app.network.sel.min(n.saturating_sub(1));
}

fn rows(app: &App) -> usize {
    let st = &app.network;
    match st.tab {
        Tab::Node => 0,
        Tab::Peers => st.peers.len(),
        Tab::Relays => app.prefs.net.relays.len(),
        Tab::Exchanges => match &st.remote {
            Some(r) => r.items.len(),
            None => st.exchanges.len(),
        },
        Tab::Shared => st.shares.len(),
        Tab::Shards => st.entrusted.len() + st.held.len(),
        Tab::Search => st.searches.len(),
        Tab::Settings => SETTINGS.len(),
        Tab::Users => st.users.len(),
        Tab::Invites => st.invites.len() + st.requests.len(),
    }
}

/// The selected row on the Invites page: one of yours (false) or a request (true).
fn selected_invite(app: &App) -> Option<(Exchange, bool)> {
    let st = &app.network;
    let i = st.sel;
    match st.invites.get(i) {
        Some(x) => Some((x.clone(), false)),
        None => st.requests.get(i - st.invites.len()).map(|x| (x.clone(), true)),
    }
}

fn keypair(app: &mut App) -> crate::Result<Keypair> {
    if let Some(k) = &app.network.kp {
        return Ok(k.clone());
    }
    let k = identity::load_or_create(&app.store)?;
    app.network.kp = Some(k.clone());
    Ok(k)
}

fn label(app: &App) -> String {
    local::my_label(&app.store, &app.prefs.net)
}

/// Start the node with the saved settings (Net ▸ Node ▸ Start node).
pub fn start(app: &mut App) {
    if app.network.node_running() {
        ok(app, "the node is already running");
        return;
    }
    let r = (|| -> crate::Result<local::Hosted> {
        let kp = keypair(app)?;
        let service = Store::open(&app.paths.db, Some(&app.dek.0))?;
        let label = label(app);
        let spare = Store::open(&app.paths.db, Some(&app.dek.0))?;
        Ok(local::Hosted::start(&app.paths.home, &app.store, service, &kp, &label, &app.prefs.net, ORIGIN)?
            .settle_with(spare, app.prefs.net.relays.clone()))
    })();
    match r {
        Ok(h) => {
            let m = format!("node {} running as {}", h.label, h.node.peer_id());
            app.network.hosted = Some(h);
            app.refresh_history(true);
            app.set_status(m.clone());
            ok(app, m);
            // the web server comes up with the node when asked to
            if app.prefs.hosting.autostart && app.orgs.host.is_none() && crate::org::host::running(&app.paths.home).is_none() {
                super::orgs::start_host(app);
            }
        }
        Err(e) => {
            app.set_status(format!("node: {e}"));
            err(app, e.to_string());
        }
    }
}

/// Stop the node this TUI runs, or ask the one another process runs to stop.
pub fn stop(app: &mut App) {
    if let Some(h) = app.network.hosted.take() {
        h.stop(&app.store, ORIGIN);
        app.refresh_history(true);
        app.set_status("node stopped");
        ok(app, "node stopped");
    } else if local::running(&app.paths.home).is_some() {
        match local::request_stop(&app.paths.home) {
            Ok(()) => ok(app, "asked the running node to stop"),
            Err(e) => err(app, e.to_string()),
        }
    } else {
        ok(app, "no node is running");
    }
}

/// Between frames: keep the node's file fresh and take finished work.
pub fn poll(app: &mut App) {
    if let Some(h) = app.network.hosted.as_mut()
        && !h.tick(&app.store)
    {
        let h = app.network.hosted.take().expect("hosted");
        h.stop(&app.store, ORIGIN);
        app.set_status("node stopped");
        app.refresh_history(true);
    }
    let Some(job) = app.network.job.as_ref() else { return };
    let r = match job.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Err("the network task stopped".into()),
    };
    let what = app.network.job.take().map(|j| j.what).unwrap_or_default();
    app.refresh_history(true);
    reload(app);
    match r {
        Err(e) => {
            app.set_status(format!("{what}: {e}"));
            if let Some(f) = app.network.entrust.as_mut() {
                f.status = vec![format!("✗ {e}")];
            }
            err(app, format!("{what}: {e}"));
        }
        Ok(Done::Lines(lines)) => {
            let first = lines.first().cloned().unwrap_or_else(|| format!("{what}: done"));
            app.set_status(first.clone());
            ok(app, first);
            if let Some(e) = app.network.entrust.as_mut() {
                e.status = lines;
            } else {
                app.network.detail = lines;
            }
        }
        Ok(Done::Remote { exchange, title, items }) => {
            let n = items.len();
            app.network.remote = Some(Remote { exchange, title: title.clone(), items, sel: 0 });
            app.network.detail.clear();
            ok(app, format!("{title} shares {n} item(s) with you: Enter imports one, Esc goes back"));
        }
        Ok(Done::Fetched { from, item }) => match client::import_item(&mut app.store, &item, &from, ORIGIN) {
            Ok(m) => {
                app.refresh_history(true);
                app.set_status(m.clone());
                ok(app, m);
            }
            Err(e) => err(app, format!("import: {e}")),
        },
        Ok(Done::Searched { name, results }) => {
            let total: usize = results.iter().map(|o| o.found.len()).sum();
            let nodes = results.iter().filter(|o| o.error.is_none()).count();
            let mut lines = vec![];
            for o in &results {
                if let Some(e) = &o.error {
                    lines.push(format!("{}: {e}", o.origin));
                }
                for f in &o.found {
                    lines.push(format!("{:<12} {}", o.origin, f.line()));
                }
                if o.truncated {
                    lines.push(format!("{}: more results than the limit", o.origin));
                }
            }
            app.network.detail = lines;
            ok(app, format!("{name}: {total} match(es) from {nodes} node(s)"));
        }
    }
}

/// Run network work on a worker thread with its own store connection and the TUI's
/// node (else a short-lived one with the same identity).
fn spawn(app: &mut App, what: &str, f: impl FnOnce(&mut Store, &Node) -> crate::Result<Done> + Send + 'static) {
    if app.network.job.is_some() {
        err(app, "still working on the last request");
        return;
    }
    let kp = match keypair(app) {
        Ok(k) => k,
        Err(e) => return err(app, e.to_string()),
    };
    let node = app.network.hosted.as_ref().filter(|h| h.node.running()).map(|h| h.node.clone());
    let (db, dek, label, prefs) = (app.paths.db.clone(), app.dek.clone(), label(app), app.prefs.net.clone());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = (|| -> crate::Result<Done> {
            let mut s = Store::open(&db, Some(&dek.0))?;
            let node = match node {
                Some(n) => n,
                None => Arc::new(local::ephemeral(&kp, Store::open(&db, Some(&dek.0))?, &label, &prefs)?),
            };
            f(&mut s, &node)
        })();
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    app.network.job = Some(Job { what: what.to_string(), rx });
    ok(app, format!("{what}… (the window stays usable)"));
}

fn selected_exchange(app: &App) -> Option<Exchange> {
    (app.network.tab == Tab::Exchanges && app.network.remote.is_none()).then(|| app.network.exchanges.get(app.network.sel).cloned()).flatten()
}

fn exchange_lines(x: &Exchange, shares: &[Share]) -> Vec<String> {
    let mut v = vec![
        format!("{} · {} · {}", x.title(), auto::status_of(x).as_str(), auto::purpose(x)),
        format!("node    {}", x.peer_id.as_deref().unwrap_or("(not known yet)")),
    ];
    if let Some(f) = &x.fingerprint {
        v.push(format!("key     {f}  (pinned the first time; a changed key is refused)"));
    }
    let shared: Vec<String> = shares.iter().filter(|s| s.applies_to(&x.id)).map(|s| format!("{} {}", s.kind.label(), s.name)).collect();
    v.push(format!("shared  {}", if shared.is_empty() { "nothing".into() } else { shared.join(", ") }));
    if x.status == ExchangeStatus::Offered && x.peer_id.is_none() {
        v.push(String::new());
        v.push("Invite link (send it to them; it holds no secret):".into());
        v.push(exchange::link(&x.offer));
    }
    v
}

fn setting_value(app: &App, s: Setting) -> String {
    let p = &app.prefs.net;
    let onoff = |b: bool| if b { "on" } else { "off" }.to_string();
    match s {
        Setting::Label => {
            if p.label.is_empty() {
                format!("(author: {})", label(app))
            } else {
                p.label.clone()
            }
        }
        Setting::Port => p.port.to_string(),
        Setting::Listen => {
            if p.listen.is_empty() {
                "(all interfaces, TCP + QUIC)".into()
            } else {
                p.listen.join(" ")
            }
        }
        Setting::Mdns => onoff(p.mdns),
        Setting::RelayServer => onoff(p.relay_server),
        Setting::Autostart => onoff(p.autostart),
        Setting::HostAutostart => onoff(app.prefs.hosting.autostart),
        Setting::Accept => if p.accept_anyone() { "anyone (no request)" } else { "known nodes (others ask first)" }.into(),
        Setting::Bootstrap => {
            if p.bootstrap.is_empty() {
                "(none)".into()
            } else {
                p.bootstrap.join(" ")
            }
        }
    }
}

fn setting_name(s: Setting) -> &'static str {
    match s {
        Setting::Label => "label",
        Setting::Port => "port",
        Setting::Listen => "listen",
        Setting::Mdns => "local discovery (mDNS)",
        Setting::RelayServer => "relay for others",
        Setting::Autostart => "start with the TUI",
        Setting::HostAutostart => "web server with the node",
        Setting::Accept => "connect at once",
        Setting::Bootstrap => "bootstrap nodes",
    }
}

fn save_net(app: &mut App, what: &str) {
    app.save_prefs();
    let p = serde_json::to_value(&app.prefs.net).unwrap_or_default();
    record(app, "net.config", format!("network settings: {what}"), p);
    let later = if app.network.node_running() { " (applies when the node starts again)" } else { "" };
    ok(app, format!("saved {what}{later}"));
}

fn split_list(v: &str) -> Vec<String> {
    v.split([' ', ',']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn finish_ask(app: &mut App, kind: NAsk, v: String) {
    let relays = app.prefs.net.relays.clone();
    match kind {
        NAsk::Connect => {
            if v.is_empty() {
                return;
            }
            connect(app, v);
        }
        NAsk::PeerLabel => {
            let Some(p) = app.network.peers.get(app.network.sel).cloned() else { return };
            match app.store.label_net_peer(&p.peer_id, &v) {
                Ok(()) => {
                    reload(app);
                    ok(app, format!("peer {} is now {v}", p.short_id()));
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        NAsk::RelayAdd => {
            if v.is_empty() {
                return;
            }
            if let Err(e) = v.parse::<libp2p::Multiaddr>() {
                return err(app, format!("not an address: {e}"));
            }
            if crate::net::node::addr_peer(&v.parse().expect("parsed")).is_none() {
                return err(app, "a relay address ends in /p2p/PEER_ID");
            }
            if !app.prefs.net.relays.contains(&v) {
                app.prefs.net.relays.push(v.clone());
            }
            if let Some(h) = &app.network.hosted {
                let _ = h.node.add_relay(v.parse().expect("parsed"));
            }
            save_net(app, "relays");
        }
        NAsk::Offer => {
            if !v.is_empty() {
                return connect(app, v);
            }
            let kp = match keypair(app) {
                Ok(k) => k,
                Err(e) => return err(app, e.to_string()),
            };
            let my = label(app);
            let node = app.network.hosted.as_ref().map(|h| h.node.clone());
            let addrs = local::offer_addrs(&app.paths.home, &app.store, node.as_deref());
            match client::create(&app.store, None, &kp, "", &my, addrs, None, client::DEFAULT_TTL_MS, &relays, ORIGIN) {
                Ok(c) => {
                    reload(app);
                    app.refresh_history(true);
                    app.network.sel = app.network.exchanges.iter().position(|x| x.id == c.exchange.id).unwrap_or(0);
                    app.network.detail = vec!["Invite link (send it to them; it holds no secret):".into(), exchange::link(&c.token)];
                    ok(app, "invite link made: when they use it, the nodes connect on their own");
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        NAsk::Accept => {
            if v.is_empty() {
                return;
            }
            accept(app, v);
        }
        NAsk::Verify => {
            let Some(x) = selected_exchange(app) else { return };
            match client::verify(&app.store, &x.id, &v, ORIGIN) {
                Ok(x) => {
                    reload(app);
                    app.refresh_history(true);
                    ok(app, format!("{} verified", x.title()));
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        NAsk::ExchangeLabel => {
            let Some(x) = selected_exchange(app) else { return };
            match app.store.label_exchange(&x.id, &v) {
                Ok(()) => {
                    reload(app);
                    ok(app, "label saved");
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        NAsk::Share(one) => {
            let (k, name) = v.split_once(' ').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((v.as_str(), ""));
            let Some(kind) = ShareKind::parse(k) else {
                return err(app, format!("what kind is {k:?}? model, search, journal, project, report or code"));
            };
            if name.is_empty() {
                return err(app, "name the item after its kind, e.g. journal Field notes");
            }
            let with: Vec<String> = if one { selected_exchange(app).map(|x| vec![x.id]).unwrap_or_default() } else { vec![] };
            match client::share(&app.store, kind, name, &with, ORIGIN) {
                Ok(v) => {
                    reload(app);
                    app.refresh_history(true);
                    let new = v.iter().filter(|(_, n)| *n).count();
                    let to = if one { "this exchange" } else { "every exchange" };
                    ok(app, if new > 0 { format!("shared {} {name} with {to}", kind.label()) } else { "already shared".into() });
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        NAsk::Setting(s) => {
            let p = &mut app.prefs.net;
            match s {
                Setting::Label => p.label = v,
                Setting::Port => match v.parse() {
                    Ok(n) => p.port = n,
                    Err(_) => return err(app, "a port is a number from 0 to 65535 (0: any)"),
                },
                Setting::Listen | Setting::Bootstrap => {
                    let list = split_list(&v);
                    if let Some(bad) = list.iter().find(|a| a.parse::<libp2p::Multiaddr>().is_err()) {
                        return err(app, format!("not an address: {bad}"));
                    }
                    if s == Setting::Listen {
                        p.listen = list;
                    } else {
                        p.bootstrap = list;
                    }
                }
                _ => return,
            }
            save_net(app, setting_name(s));
        }
    }
}

/// Connect to a node by address, peer id or name: the keys are exchanged on their own.
fn connect(app: &mut App, target: String) {
    let kp = match keypair(app) {
        Ok(k) => k,
        Err(e) => return err(app, e.to_string()),
    };
    let my = label(app);
    let relays = app.prefs.net.relays.clone();
    let node = app.network.hosted.as_ref().map(|h| h.node.clone());
    let addrs = local::offer_addrs(&app.paths.home, &app.store, node.as_deref());
    spawn(app, "connect", move |s, n| {
        let l = auto::connect(s, n, &kp, &target, None, &my, addrs, &relays, ORIGIN)?;
        Ok(Done::Lines(l.lines()))
    });
}

/// Accept a request (selected) or use a pasted invite link.
fn accept(app: &mut App, input: String) {
    let kp = match keypair(app) {
        Ok(k) => k,
        Err(e) => return err(app, e.to_string()),
    };
    let my = label(app);
    let node = app.network.hosted.as_ref().map(|h| h.node.clone());
    let addrs = local::offer_addrs(&app.paths.home, &app.store, node.as_deref());
    let pasted = exchange::is_token(&exchange::unlink(&input));
    spawn(app, "accept", move |s, n| {
        if !pasted {
            return Ok(Done::Lines(auto::accept(s, Some(n), &kp, &input, &my, addrs, ORIGIN)?.lines()));
        }
        Ok(Done::Lines(auto::use_link(s, Some(n), &kp, &input, "", &my, addrs, ORIGIN)?.lines()))
    });
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.network.entrust.is_some() {
        return entrust_key(app, k);
    }
    if let Some(a) = app.network.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.network.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.network.ask = None;
                finish_ask(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.network.confirm.take() {
        if matches!(k.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            confirmed(app, c);
        } else {
            ok(app, "left as it was");
        }
        return;
    }
    let n = rows(app);
    match k.code {
        KeyCode::Esc => {
            if app.network.remote.is_some() {
                app.network.remote = None;
                app.network.msg = None;
            } else if !app.network.detail.is_empty() {
                app.network.detail.clear();
            } else {
                app.modal = None;
            }
            return;
        }
        KeyCode::Tab | KeyCode::Right => return switch(app, (app.network.tab.index() + 1) % Tab::ALL.len()),
        KeyCode::BackTab | KeyCode::Left => return switch(app, (app.network.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()),
        KeyCode::Char(c @ '1'..='9') => return switch(app, c as usize - '1' as usize),
        KeyCode::Char('0') => return switch(app, 9),
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(r) = app.network.remote.as_mut() {
                r.sel = (r.sel + 1).min(r.items.len().saturating_sub(1));
            } else {
                app.network.sel = (app.network.sel + 1).min(n.saturating_sub(1));
            }
            return;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(r) = app.network.remote.as_mut() {
                r.sel = r.sel.saturating_sub(1);
            } else {
                app.network.sel = app.network.sel.saturating_sub(1);
            }
            return;
        }
        KeyCode::Char('S') => return start(app),
        KeyCode::Char('X') => return stop(app),
        KeyCode::Char('g') => {
            reload(app);
            return ok(app, "refreshed");
        }
        _ => {}
    }
    let ask = |app: &mut App, kind: NAsk, title: &str, v: &str| app.network.ask = Some(Ask::new(kind, title, v));
    match (app.network.tab, k.code) {
        (Tab::Node, KeyCode::Char('s') | KeyCode::Enter) => {
            if app.network.node_running() {
                stop(app)
            } else {
                start(app)
            }
        }
        (Tab::Node, KeyCode::Char('c')) | (Tab::Exchanges, KeyCode::Char('c')) => {
            ask(app, NAsk::Connect, "Connect to (address …/p2p/ID, peer id or name)", "")
        }
        (Tab::Peers, KeyCode::Char('c')) => {
            let v = app.network.peers.get(app.network.sel).map(|p| p.peer_id.clone()).unwrap_or_default();
            ask(app, NAsk::Connect, "Connect to (address …/p2p/ID, peer id or name)", &v)
        }
        (Tab::Peers, KeyCode::Enter) => {
            if let Some(p) = app.network.peers.get(app.network.sel) {
                app.network.detail = vec![
                    format!("{} · {}", if p.label.is_empty() { p.short_id() } else { &p.label }, p.peer_id),
                    format!("found by {} · {} · offers {}", p.source, p.agent, client::caps_text(&p.caps)),
                    format!("first seen {} · last seen {}", fmt_ms(p.first_seen), fmt_ms(p.last_seen)),
                ];
                app.network.detail.extend(p.addrs.iter().map(|a| format!("  {a}")));
            }
        }
        (Tab::Peers, KeyCode::Char('l')) => {
            if let Some(p) = app.network.peers.get(app.network.sel) {
                let v = p.label.clone();
                ask(app, NAsk::PeerLabel, "Peer label", &v)
            }
        }
        (Tab::Peers, KeyCode::Char('d')) => {
            if let Some(p) = app.network.peers.get(app.network.sel) {
                let (id, short) = (p.peer_id.clone(), p.short_id().to_string());
                app.network.confirm = Some(Confirm::Forget(id));
                ok(app, format!("forget peer {short}? (exchanges stay) y/n"));
            }
        }
        (Tab::Relays, KeyCode::Char('a')) => ask(app, NAsk::RelayAdd, "Relay address (…/p2p/ID)", ""),
        (Tab::Relays, KeyCode::Char('d')) => {
            let i = app.network.sel;
            if i < app.prefs.net.relays.len() {
                app.prefs.net.relays.remove(i);
                save_net(app, "relays");
                app.network.sel = i.min(app.prefs.net.relays.len().saturating_sub(1));
            }
        }
        (Tab::Relays, KeyCode::Char('r')) => {
            app.prefs.net.relay_server = !app.prefs.net.relay_server;
            save_net(app, "relay for others");
        }
        (Tab::Exchanges, _) if app.network.remote.is_some() => {
            if k.code == KeyCode::Enter {
                let r = app.network.remote.as_ref().expect("remote");
                let Some(it) = r.items.get(r.sel).cloned() else { return };
                let Some(x) = app.network.exchanges.iter().find(|x| x.id == r.exchange).cloned() else { return };
                let relays = app.prefs.net.relays.clone();
                spawn(app, "fetch", move |s, n| {
                    let item = client::fetch(s, n, &x, &it.id, None, &relays, ORIGIN)?;
                    Ok(Done::Fetched { from: x.title(), item })
                });
            }
        }
        (Tab::Exchanges, KeyCode::Char('n')) => ask(app, NAsk::Offer, "Invite link (Enter) or connect to (address, peer id)", ""),
        (Tab::Exchanges, KeyCode::Char('a')) => match selected_exchange(app) {
            Some(x) if x.status == ExchangeStatus::Incoming => accept(app, x.id),
            _ => err(app, "select a request to accept (p pastes an invite link)"),
        },
        (Tab::Exchanges, KeyCode::Char('p')) => ask(app, NAsk::Accept, "Paste an invite link (muninn://…)", ""),
        (Tab::Exchanges, KeyCode::Char('d')) => match selected_exchange(app) {
            Some(x) if x.status == ExchangeStatus::Incoming => match auto::decline(&app.store, &x.id, ORIGIN) {
                Ok(x) => {
                    reload(app);
                    app.refresh_history(true);
                    ok(app, format!("declined {}", x.title()));
                }
                Err(e) => err(app, e.to_string()),
            },
            _ => err(app, "only a request can be declined (r disconnects)"),
        },
        (Tab::Exchanges, KeyCode::Enter) => {
            if let Some(x) = selected_exchange(app) {
                app.network.detail = exchange_lines(&x, &app.network.shares);
            }
        }
        (Tab::Exchanges, KeyCode::Char('v')) => {
            if let Some(x) = selected_exchange(app) {
                if !x.status.active() {
                    return err(app, format!("{} is {}: nothing to verify yet", x.title(), x.status.as_str()));
                }
                ask(app, NAsk::Verify, "Short code the other person reads you", "")
            }
        }
        (Tab::Exchanges, KeyCode::Char('l')) => {
            if let Some(x) = selected_exchange(app) {
                ask(app, NAsk::ExchangeLabel, "Connection label", &x.label)
            }
        }
        (Tab::Exchanges, KeyCode::Char('r')) => {
            if let Some(x) = selected_exchange(app) {
                let t = x.title();
                app.network.confirm = Some(Confirm::Revoke(x.id));
                ok(app, format!("disconnect from {t}? Nothing more is shared with them. y/n"));
            }
        }
        (Tab::Exchanges, KeyCode::Char('s')) => {
            if selected_exchange(app).is_some_and(|x| x.status.active()) {
                ask(app, NAsk::Share(true), "Share with this connection (kind name: journal Field notes)", "")
            } else {
                err(app, "only a connected node can be shared with")
            }
        }
        (Tab::Exchanges, KeyCode::Char('L')) => {
            let Some(x) = selected_exchange(app) else { return };
            if !x.status.active() {
                return err(app, format!("{} is {}", x.title(), x.status.as_str()));
            }
            let relays = app.prefs.net.relays.clone();
            spawn(app, "list", move |s, n| {
                let items = client::remote_list(s, n, &x, &relays)?;
                Ok(Done::Remote { exchange: x.id.clone(), title: x.title(), items })
            });
        }
        (Tab::Exchanges, KeyCode::Char('e')) => {
            app.network.detail = super::modal::cli_hint(
                "net exchange export --out FILE",
                "Exchange backups are encrypted with your export password, which the TUI never asks for.",
            );
        }
        (Tab::Shared, KeyCode::Char('a')) => ask(app, NAsk::Share(false), "Share with every exchange (kind name: journal Field notes)", ""),
        (Tab::Shared, KeyCode::Char('d')) => {
            let Some(sh) = app.network.shares.get(app.network.sel).cloned() else { return };
            let with: Vec<String> = sh.exchange_id.clone().into_iter().collect();
            match client::unshare(&app.store, sh.kind, &sh.ref_id, &with, ORIGIN) {
                Ok(_) => {
                    reload(app);
                    app.refresh_history(true);
                    ok(app, format!("stopped sharing {} {}", sh.kind.label(), sh.name));
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        (Tab::Shards, KeyCode::Char('e') | KeyCode::Char('r')) => open_entrust(app),
        (Tab::Shards, KeyCode::Char('d')) => {
            let i = app.network.sel.checked_sub(app.network.entrusted.len());
            if let Some(h) = i.and_then(|i| app.network.held.get(i)) {
                let (id, who) = (h.id.clone(), h.owner_label.clone());
                app.network.confirm = Some(Confirm::DropHeld(id));
                ok(app, format!("drop the shard you hold for {who}? They may need it to recover. y/n"));
            } else {
                err(app, "only shards you hold for others can be dropped here");
            }
        }
        (Tab::Search, KeyCode::Enter) => {
            let Some(ls) = app.network.searches.get(app.network.sel).cloned() else { return };
            let relays = app.prefs.net.relays.clone();
            spawn(app, "search", move |s, n| {
                let results = client::search(s, Some(n), &ls.spec, &[], true, 200, &relays, ORIGIN)?;
                Ok(Done::Searched { name: ls.name, results })
            });
        }
        (Tab::Users, KeyCode::Enter) => {
            if let Some((u, trusted, _)) = app.network.users.get(app.network.sel).cloned() {
                let mut d = vec![format!("{} · user {} · {}", u.name(), u.id, if trusted { "trusted" } else { "untrusted" })];
                for x in app.store.user_nodes(&u.id).unwrap_or_default() {
                    let state = match x.revoked_at {
                        Some(t) => format!("revoked {}", fmt_ms(t)),
                        None => format!("linked {}", fmt_ms(x.issued_at)),
                    };
                    d.push(format!("  node {} {} · {state}", crate::store::net::short_peer(&x.peer_id), x.label));
                }
                app.network.detail = d;
            }
        }
        (Tab::Users, KeyCode::Char('t')) => {
            let Some((u, trusted, _)) = app.network.users.get(app.network.sel).cloned() else { return };
            match user::set_trust(&app.store, &u.id, !trusted, ORIGIN) {
                Ok((u, _)) => {
                    reload(app);
                    app.refresh_history(true);
                    ok(app, format!("{} is now {}", u.name(), if trusted { "untrusted" } else { "trusted" }));
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        (Tab::Users, KeyCode::Char('d')) => {
            if let Some((u, _, _)) = app.network.users.get(app.network.sel).cloned() {
                app.network.confirm = Some(Confirm::RemoveConnection(u.id.clone()));
                ok(app, format!("forget the connection with {}? (their nodes stay connected; trust is reset) y/n", u.name()));
            }
        }
        (Tab::Invites, KeyCode::Char('n')) => ask(app, NAsk::Offer, "Invite link (Enter) or connect to (address, peer id)", ""),
        (Tab::Invites, KeyCode::Char('p')) => ask(app, NAsk::Accept, "Paste an invite link (muninn://…)", ""),
        (Tab::Invites, KeyCode::Char('a')) => match selected_invite(app) {
            Some((x, true)) if exchange::scope_kind(&x.scope) == "link" => {
                let t = x.title();
                app.network.confirm = Some(Confirm::ApproveLink(x.id));
                ok(app, format!("let {t} act as you? It gets your user key. y/n"));
            }
            Some((x, true)) => accept(app, x.id),
            _ => err(app, "select a request to accept (p pastes an invite link)"),
        },
        (Tab::Invites, KeyCode::Char('d')) => match selected_invite(app) {
            Some((x, true)) => match auto::decline(&app.store, &x.id, ORIGIN) {
                Ok(x) => {
                    reload(app);
                    app.refresh_history(true);
                    ok(app, format!("declined {}", x.title()));
                }
                Err(e) => err(app, e.to_string()),
            },
            _ => err(app, "only a request can be declined (r withdraws an invite)"),
        },
        (Tab::Invites, KeyCode::Char('r')) => match selected_invite(app) {
            Some((x, false)) => {
                app.network.confirm = Some(Confirm::RevokeInvite(x.id.clone()));
                ok(app, format!("withdraw invite {}? The link stops working. y/n", x.short_id()));
            }
            _ => err(app, "select one of your invites (d declines a request)"),
        },
        (Tab::Invites, KeyCode::Enter) => {
            if let Some((x, req)) = selected_invite(app) {
                let mut d = vec![format!(
                    "{} {} · for {} · {}",
                    if req { "request from" } else { "invite" },
                    if req { x.title() } else { x.short_id().to_string() },
                    auto::purpose(&x),
                    x.expires_at.map(|e| format!("expires {}", fmt_ms(e))).unwrap_or_default()
                )];
                if !req {
                    d.push(exchange::link(&x.offer));
                }
                app.network.detail = d;
            }
        }
        (Tab::Settings, KeyCode::Enter | KeyCode::Char(' ')) => {
            let s = SETTINGS[app.network.sel.min(SETTINGS.len() - 1)];
            let p = &mut app.prefs.net;
            match s {
                Setting::Mdns => p.mdns = !p.mdns,
                Setting::RelayServer => p.relay_server = !p.relay_server,
                Setting::Autostart => p.autostart = !p.autostart,
                Setting::Accept => p.accept = if p.accept_anyone() { "known".into() } else { "anyone".into() },
                Setting::HostAutostart => {
                    app.prefs.hosting.autostart = !app.prefs.hosting.autostart;
                    app.save_prefs();
                    let on = app.prefs.hosting.autostart;
                    record(
                        app,
                        "org.host.config",
                        format!("web server with the node: {}", if on { "on" } else { "off" }),
                        serde_json::json!({ "autostart": on }),
                    );
                    return ok(app, "saved web server with the node");
                }
                _ => {
                    let v = match s {
                        Setting::Label => p.label.clone(),
                        Setting::Port => p.port.to_string(),
                        Setting::Listen => p.listen.join(" "),
                        _ => p.bootstrap.join(" "),
                    };
                    let title = format!("{} (empty: default)", setting_name(s));
                    return ask(app, NAsk::Setting(s), &title, &v);
                }
            }
            save_net(app, setting_name(s));
        }
        _ => {}
    }
}

fn confirmed(app: &mut App, c: Confirm) {
    match c {
        Confirm::Forget(id) => match app.store.remove_net_peer(&id) {
            Ok(_) => {
                reload(app);
                ok(app, "peer forgotten");
            }
            Err(e) => err(app, e.to_string()),
        },
        Confirm::Revoke(id) => {
            let relays = app.prefs.net.relays.clone();
            spawn(app, "revoke", move |s, n| {
                let (x, told) = client::revoke(s, Some(n), &id, &relays, ORIGIN)?;
                let t = match told {
                    Some(Ok(())) => "they were told".to_string(),
                    Some(Err(e)) => format!("they could not be told ({e})"),
                    None => "nothing to tell".into(),
                };
                Ok(Done::Lines(vec![format!("disconnected from {}; {t}", x.title())]))
            });
        }
        Confirm::RemoveConnection(id) => match user::remove_connection(&app.store, &id, ORIGIN) {
            Ok(u) => {
                reload(app);
                app.refresh_history(true);
                ok(app, format!("connection with {} removed", u.name()));
            }
            Err(e) => err(app, e.to_string()),
        },
        Confirm::ApproveLink(id) => accept(app, id),
        Confirm::RevokeInvite(id) => match client::revoke(&app.store, None, &id, &[], ORIGIN) {
            Ok((x, _)) => {
                reload(app);
                app.refresh_history(true);
                ok(app, format!("invite {} withdrawn", x.short_id()));
            }
            Err(e) => err(app, e.to_string()),
        },
        Confirm::DropHeld(id) => match app.store.drop_held_shard(&id) {
            Ok(_) => {
                record(app, "net.shard.drop", "dropped a held shard".into(), serde_json::json!({ "shard": id }));
                reload(app);
                ok(app, "shard dropped");
            }
            Err(e) => err(app, e.to_string()),
        },
    }
}

fn switch(app: &mut App, i: usize) {
    let tab = Tab::ALL[i.min(Tab::ALL.len() - 1)];
    open(app, tab);
}

pub fn on_click(app: &mut App, i: usize) {
    if app.network.ask.is_some() {
        return;
    }
    if let Some(r) = app.network.remote.as_mut() {
        if r.sel == i {
            on_key(app, KeyEvent::from(KeyCode::Enter));
        } else if i < r.items.len() {
            r.sel = i;
        }
        return;
    }
    if app.network.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < rows(app) {
        app.network.sel = i;
        app.network.detail.clear();
    }
}

fn fmt_ms(ms: i64) -> String {
    crate::ingest::time::fmt_local(ms)
}

fn node_lines(app: &App) -> Vec<String> {
    let mut v = vec![];
    let other = local::running(&app.paths.home).filter(|f| f.pid != std::process::id());
    let st = match (&app.network.hosted, &other) {
        (Some(h), _) => {
            v.push(format!("● running here as {}", h.label));
            Some(h.node.status())
        }
        (None, Some(f)) => {
            v.push(format!("● running in another process (pid {}) as {}", f.pid, f.label));
            Some(f.status.clone())
        }
        (None, None) => {
            v.push("○ stopped: S starts it (nothing is shared until you share it)".into());
            None
        }
    };
    let id = identity::peer_id(&app.store).ok().flatten();
    match &id {
        Some(p) => v.push(format!("peer id   {p}")),
        None => v.push("peer id   (made when the node first starts)".into()),
    }
    if let Some(s) = st {
        v.push(format!("NAT       {}", if s.nat.is_empty() { "unknown" } else { &s.nat }));
        v.push(format!("peers     {} connected", s.connected.len()));
        v.push(format!(
            "relay     {}{}",
            if s.relay_server { "serving others" } else { "not serving" },
            if s.reservations.is_empty() { String::new() } else { format!(" · reserved on {}", s.reservations.len()) }
        ));
        v.push(format!("mDNS      {}", if s.mdns { "on" } else { "off" }));
        v.push("reachable at:".into());
        let addrs = local::reachable(&s);
        if addrs.is_empty() {
            v.push("  (no address others can dial yet)".into());
        }
        v.extend(addrs.into_iter().map(|a| format!("  {a}")));
    }
    let active = app.network.exchanges.iter().filter(|x| x.status.active()).count();
    v.push(format!("exchanges {active} active · shared items {} · shards held for others {}", app.network.shares.len(), app.network.held.len()));
    v
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.network;
    let tabs: Vec<String> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let n = (i + 1) % 10;
            if *x == st.tab { format!("[{n} {}]", x.title()) } else { format!(" {n} {} ", x.title()) }
        })
        .collect();
    let busy = st.job.as_ref().map(|j| format!(" · {}…", j.what)).unwrap_or_default();
    let mut v = View::new(format!("Network · {}{busy}", st.tab.title()));
    // the page names wrap onto a second line in narrow terminals
    let room = (r.width as usize).saturating_sub(4).max(20);
    let mut line = String::new();
    for tb in &tabs {
        if !line.is_empty() && line.chars().count() + 1 + tb.chars().count() > room {
            v.head.push(Line::styled(std::mem::take(&mut line), Style::new().fg(t.accent)));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(tb);
    }
    v.head.push(Line::styled(line, Style::new().fg(t.accent)));
    v.head.push(Line::raw(""));
    let fg = Style::new().fg(t.fg);
    if let Some(e) = &st.entrust {
        entrust_lines(e, &t, &mut v.head);
        v.msg = st.msg.clone();
        v.keys = vec![
            ("Space pick", KeyCode::Char(' ')),
            ("Tab nodes/form", KeyCode::Tab),
            ("Enter edit/run", KeyCode::Enter),
            ("Esc close", KeyCode::Esc),
        ];
        return draw_view(f, r, v, &t, hits);
    }
    match st.tab {
        Tab::Node => {
            for l in node_lines(app) {
                v.head.push(Line::styled(format!(" {l}"), fg));
            }
        }
        Tab::Peers => {
            if st.peers.is_empty() {
                v.head.push(muted(&t, " no peers yet: start the node (S) to find some nearby, or c connects to an address"));
            }
            for (i, p) in st.peers.iter().enumerate() {
                let name = if p.label.is_empty() { "-" } else { &p.label };
                let link = st
                    .exchanges
                    .iter()
                    .filter(|x| x.peer_id.as_deref() == Some(p.peer_id.as_str()))
                    .map(auto::status_of)
                    .min_by_key(|s| match s {
                        auto::LinkStatus::Connected => 0,
                        auto::LinkStatus::Requested => 1,
                        auto::LinkStatus::Pending => 2,
                        _ => 3,
                    })
                    .map(|s| s.as_str())
                    .unwrap_or("-");
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<18} {:<9} {:<10}", name, p.short_id(), link), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {:<9} {}", p.source, client::caps_text(&p.caps)), Style::new().fg(t.muted)),
                ]));
            }
        }
        Tab::Relays => {
            v.head.push(Line::styled(
                format!(" relay for others: {} (r toggles; applies at the next start)", if app.prefs.net.relay_server { "on" } else { "off" }),
                fg,
            ));
            let known: Vec<&NetPeer> = st.peers.iter().filter(|p| p.is_relay()).collect();
            if !known.is_empty() {
                v.head.push(muted(&t, format!(" known relays: {}", known.iter().map(|p| p.short_id()).collect::<Vec<_>>().join(", "))));
            }
            if app.prefs.net.relays.is_empty() {
                v.head.push(muted(&t, " no relays configured: a adds one (…/p2p/ID) so nodes behind NAT can reach you"));
            } else {
                v.head.push(muted(&t, " relays used for reservations:"));
            }
            for (i, a) in app.prefs.net.relays.iter().enumerate() {
                v.rows.push(Line::styled(format!(" {a}"), row_style(&t, i == st.sel)));
            }
        }
        Tab::Exchanges => {
            if let Some(rm) = &st.remote {
                v.head.push(Line::styled(format!(" shared by {}:", rm.title), fg));
                if rm.items.is_empty() {
                    v.head.push(muted(&t, " nothing shared with you yet"));
                }
                for (i, it) in rm.items.iter().enumerate() {
                    v.rows.push(Line::from(vec![
                        Span::styled(format!(" {:<11} {}", it.kind.label(), it.name), row_style(&t, i == rm.sel)),
                        Span::styled(format!("  {}", it.about), Style::new().fg(t.muted)),
                    ]));
                }
            } else {
                if st.exchanges.is_empty() {
                    v.head.push(muted(&t, " no connections yet: c connects to a node, n makes an invite link, p uses one"));
                } else {
                    v.head.push(muted(&t, " keys are exchanged by the nodes on their own; requests from others wait for a (accept) or d (decline)"));
                }
                for (i, x) in st.exchanges.iter().enumerate() {
                    let ls = auto::status_of(x);
                    let mark = match ls {
                        auto::LinkStatus::Connected => "●",
                        auto::LinkStatus::Requested => "?",
                        auto::LinkStatus::Pending => "○",
                        _ => "✗",
                    };
                    v.rows.push(Line::from(vec![
                        Span::styled(format!(" {mark} "), Style::new().fg(if x.status.active() { t.accent } else { t.muted })),
                        Span::styled(format!("{:<20} {:<10}", x.title(), ls.as_str()), row_style(&t, i == st.sel)),
                        Span::styled(format!(" {}", auto::purpose(x)), Style::new().fg(t.muted)),
                    ]));
                }
            }
        }
        Tab::Shared => {
            if st.shares.is_empty() {
                v.head.push(muted(&t, " nothing is shared: a shares an item with every connection (s on a connection: just that one)"));
            }
            for (i, s) in st.shares.iter().enumerate() {
                let with = match &s.exchange_id {
                    None => "every connection".to_string(),
                    Some(e) => st.exchanges.iter().find(|x| &x.id == e).map(|x| x.title()).unwrap_or_else(|| e.clone()),
                };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<11} {:<24}", s.kind.label(), s.name), row_style(&t, i == st.sel)),
                    Span::styled(format!(" with {with}"), Style::new().fg(t.muted)),
                ]));
            }
        }
        Tab::Shards => {
            v.head.push(muted(&t, " Shards of your master key, sealed with your shard password; peers never see them in plain."));
            if st.entrusted.is_empty() && st.held.is_empty() {
                v.head.push(muted(&t, " none yet: e entrusts shards to nodes you are connected to"));
            }
            let mut i = 0;
            for e in &st.entrusted {
                let to = st.exchanges.iter().find(|x| x.id == e.exchange_id).map(|x| x.title()).unwrap_or_else(|| e.peer_id.clone());
                v.rows.push(Line::from(vec![
                    Span::styled(
                        format!(" entrusted  {}/{} of set {} ", e.index, e.total, crate::store::short(&e.set_id)),
                        row_style(&t, i == st.sel),
                    ),
                    Span::styled(format!(" to {to} · any {} restore", e.threshold), Style::new().fg(t.muted)),
                ]));
                i += 1;
            }
            for h in &st.held {
                v.rows.push(Line::from(vec![
                    Span::styled(
                        format!(" holding    {}/{} of set {} ", h.index, h.total, crate::store::short(&h.set_id)),
                        row_style(&t, i == st.sel),
                    ),
                    Span::styled(format!(" for {} · since {}", h.owner_label, fmt_ms(h.stored_at)), Style::new().fg(t.muted)),
                ]));
                i += 1;
            }
        }
        Tab::Search => {
            if st.searches.is_empty() {
                v.head.push(muted(&t, " no saved live searches: save one (B) and run it here across your peers"));
            } else {
                v.head.push(muted(&t, " Enter runs a saved search here and on every peer; each peer searches only what it shares with you"));
            }
            for (i, s) in st.searches.iter().enumerate() {
                v.rows.push(Line::styled(format!(" {}", s.name), row_style(&t, i == st.sel)));
            }
        }
        Tab::Users => {
            if let Some(me) = &st.me {
                let mine = app.store.user_nodes(&me.id).map(|n| n.iter().filter(|x| x.active()).count()).unwrap_or(1);
                v.head.push(Line::styled(format!(" you: {} ({}) · {mine} node(s)", me.name(), me.short_id()), fg));
            }
            if st.users.is_empty() {
                v.head.push(muted(&t, " no connections yet: people you connect with appear here, untrusted until t trusts them"));
            } else {
                v.head.push(muted(&t, " untrusted until you trust them (t); only trusted ones are offered for dark shards"));
            }
            for (i, (u, trusted, n)) in st.users.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {} ", if *trusted { "★" } else { "·" }), Style::new().fg(if *trusted { t.accent } else { t.muted })),
                    Span::styled(format!("{:<20} {:<10}", u.name(), if *trusted { "trusted" } else { "untrusted" }), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {} · {n} node(s)", u.short_id()), Style::new().fg(t.muted)),
                ]));
            }
        }
        Tab::Invites => {
            if st.invites.is_empty() && st.requests.is_empty() {
                v.head.push(muted(&t, " no open invites or requests: n makes an invite link, p uses one"));
            }
            let mut i = 0;
            for x in &st.invites {
                let to = if x.peer_id.is_some() || !x.label.is_empty() { x.title() } else { "anyone with the link".into() };
                let exp = x.expires_at.map(|e| format!(" · expires {}", fmt_ms(e))).unwrap_or_default();
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" invite   {:<10} {:<20}", x.short_id(), to), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {}{exp}", auto::purpose(x)), Style::new().fg(t.muted)),
                ]));
                i += 1;
            }
            for x in &st.requests {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" request  {:<10} {:<20}", x.short_id(), x.title()), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {} · since {}", auto::purpose(x), fmt_ms(x.created_at)), Style::new().fg(t.muted)),
                ]));
                i += 1;
            }
        }
        Tab::Settings => {
            v.head.push(muted(&t, " Enter edits or toggles · saved to preferences · the node uses them when it starts"));
            for (i, s) in SETTINGS.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<24}", setting_name(*s)), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {}", setting_value(app, *s)), Style::new().fg(t.muted)),
                ]));
            }
        }
    }
    if !st.detail.is_empty() {
        v.head.push(Line::raw(""));
        for d in &st.detail {
            v.head.push(Line::styled(format!(" {d}"), fg));
        }
    }
    v.sel = match &st.remote {
        Some(r) if st.tab == Tab::Exchanges => (!r.items.is_empty()).then_some(r.sel),
        _ => (!v.rows.is_empty()).then_some(st.sel),
    };
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    let mut keys: Vec<(&'static str, KeyCode)> = vec![("Tab page", KeyCode::Tab)];
    keys.extend(match st.tab {
        Tab::Node => vec![("S start", KeyCode::Char('S')), ("X stop", KeyCode::Char('X')), ("c connect", KeyCode::Char('c'))],
        Tab::Peers => {
            vec![("Enter show", KeyCode::Enter), ("c connect", KeyCode::Char('c')), ("l label", KeyCode::Char('l')), ("d forget", KeyCode::Char('d'))]
        }
        Tab::Relays => vec![("a add", KeyCode::Char('a')), ("d remove", KeyCode::Char('d')), ("r serve", KeyCode::Char('r'))],
        Tab::Exchanges if st.remote.is_some() => vec![("Enter import", KeyCode::Enter)],
        Tab::Exchanges => vec![
            ("Enter show", KeyCode::Enter),
            ("c connect", KeyCode::Char('c')),
            ("a accept", KeyCode::Char('a')),
            ("d decline", KeyCode::Char('d')),
            ("n invite link", KeyCode::Char('n')),
            ("p paste link", KeyCode::Char('p')),
            ("s share", KeyCode::Char('s')),
            ("L their items", KeyCode::Char('L')),
            ("l label", KeyCode::Char('l')),
            ("r disconnect", KeyCode::Char('r')),
        ],
        Tab::Shared => vec![("a share", KeyCode::Char('a')), ("d unshare", KeyCode::Char('d'))],
        Tab::Shards => vec![("e entrust / check", KeyCode::Char('e')), ("d drop held", KeyCode::Char('d'))],
        Tab::Search => vec![("Enter run", KeyCode::Enter)],
        Tab::Settings => vec![("Enter edit", KeyCode::Enter)],
        Tab::Users => vec![("Enter show", KeyCode::Enter), ("t trust", KeyCode::Char('t')), ("d forget", KeyCode::Char('d'))],
        Tab::Invites => vec![
            ("Enter show", KeyCode::Enter),
            ("n invite link", KeyCode::Char('n')),
            ("p paste link", KeyCode::Char('p')),
            ("a accept", KeyCode::Char('a')),
            ("d decline", KeyCode::Char('d')),
            ("r withdraw", KeyCode::Char('r')),
        ],
    });
    keys.push(("Esc", KeyCode::Esc));
    v.keys = keys;
    draw_view(f, r, v, &t, hits);
}

/// Start the node when the TUI opens, if asked to in the settings.
pub fn autostart(app: &mut App) {
    if app.prefs.net.autostart && local::running(&app.paths.home).is_none() {
        start(app);
    }
}

/// Stop our node when the TUI closes.
pub fn shutdown(app: &mut App) {
    if let Some(h) = app.network.hosted.take() {
        h.stop(&app.store, ORIGIN);
    }
}

// ---------------- Dark shards: entrust and check ----------------

/// Net ▸ Dark shards ▸ Held & entrusted, e: pick connected nodes, give the key file and a shard password.
/// The master key is never stored, so it is read from a key file (`mnnn keys export` or
/// the recovery words saved to a file, plain or .age). Passwords are masked, cleared after
/// use and never logged.
pub struct EntrustForm {
    /// Connected nodes and whether each gets a shard.
    pub peers: Vec<(Exchange, bool)>,
    pub on_peers: bool,
    pub psel: usize,
    pub form: Form,
    threshold_edited: bool,
    /// Per-node outcome of the last run.
    pub status: Vec<String>,
}

const F_FILE: &str = "key file";
const F_FILEPW: &str = "file pass";
const F_PW: &str = "shard pass";
const F_REPEAT: &str = "repeat";
const F_K: &str = "threshold";

/// Open the entrust form (Net ▸ Dark shards ▸ Entrust…).
pub fn open_entrust(app: &mut App) {
    if app.network.tab != Tab::Shards || app.modal.is_none() {
        open(app, Tab::Shards);
    }
    reload(app);
    let mut peers: Vec<(Exchange, bool)> = vec![];
    let mut untrusted = 0;
    for x in app.network.exchanges.iter().filter(|x| x.status.active() && x.peer_id.is_some()) {
        if peers.iter().any(|(y, _)| y.peer_id == x.peer_id) {
            continue;
        }
        // only nodes of trusted connections (or your own) are offered
        if user::trusted_peer(&app.store, x.peer_id.as_deref().unwrap_or_default()).unwrap_or(false) {
            peers.push((x.clone(), false));
        } else {
            untrusted += 1;
        }
    }
    let form = Form::new(
        vec![Input::new(F_FILE), Input::masked(F_FILEPW), Input::masked(F_PW), Input::masked(F_REPEAT), Input::new(F_K)],
        vec!["Entrust", "Check restore", "Close"],
    );
    let none = peers.is_empty();
    app.network.entrust = Some(EntrustForm { peers, on_peers: !none, psel: 0, form, threshold_edited: false, status: vec![] });
    if none && untrusted > 0 {
        ok(app, format!("no trusted nodes: trust a connection first (Connections ▸ t); {untrusted} untrusted node(s) hidden"));
    } else if none {
        ok(app, "no connected nodes yet: connect to two or more (Nodes ▸ c) to entrust shards; Check restore still works");
    } else {
        ok(app, "Space picks nodes; then the key file and a shard password");
    }
}

fn suggest_threshold(e: &mut EntrustForm) {
    if e.threshold_edited {
        return;
    }
    let n = e.peers.iter().filter(|(_, on)| *on).count();
    let k = if n >= 2 { n.div_ceil(2).max(2).min(n).to_string() } else { String::new() };
    if let Some(f) = e.form.fields.iter_mut().find(|f| f.label == F_K) {
        f.value = k;
    }
}

fn entrust_key(app: &mut App, k: KeyEvent) {
    let Some(e) = app.network.entrust.as_mut() else { return };
    if e.on_peers {
        match k.code {
            KeyCode::Esc => app.network.entrust = None,
            KeyCode::Up | KeyCode::Char('k') => e.psel = e.psel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab | KeyCode::BackTab => {
                if k.code == KeyCode::Down && e.psel + 1 < e.peers.len() {
                    e.psel += 1;
                } else {
                    e.on_peers = false;
                    e.form.sel = 0;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(p) = e.peers.get_mut(e.psel) {
                    p.1 = !p.1;
                }
                suggest_threshold(e);
            }
            _ => {}
        }
        return;
    }
    if !e.form.editing {
        match k.code {
            KeyCode::Esc => return app.network.entrust = None,
            KeyCode::Tab | KeyCode::BackTab if !e.peers.is_empty() => return e.on_peers = true,
            KeyCode::Up | KeyCode::Char('k') if e.form.sel == 0 && !e.peers.is_empty() => {
                e.on_peers = true;
                e.psel = e.peers.len() - 1;
                return;
            }
            _ => {}
        }
    }
    match e.form.key(k) {
        FormEvent::Changed => {
            if e.form.fields.get(e.form.sel).is_some_and(|f| f.label == F_K) {
                e.threshold_edited = true;
            }
        }
        FormEvent::Action(0) => entrust_run(app),
        FormEvent::Action(1) => check_run(app),
        FormEvent::Action(_) => app.network.entrust = None,
        FormEvent::None => {}
    }
}

/// Take a masked field's value and clear it.
fn take(e: &mut EntrustForm, label: &str) -> Option<String> {
    let f = e.form.fields.iter_mut().find(|f| f.label == label)?;
    let v = f.opt();
    f.value.clear();
    v
}

fn missing(app: &mut App, field: &str, m: &str) {
    if let Some(e) = app.network.entrust.as_mut() {
        e.form.focus(field);
    }
    err(app, m.to_string());
}

/// The master key from a key file: 64 hex digits (`mnnn keys export`) or the 24 words,
/// plain or age-encrypted; checked against this database.
fn master_from_file(paths: &crate::paths::Paths, file: &str, pass: Option<String>) -> crate::Result<crate::crypto::Key32> {
    let text = crate::crypto::keyring::read_maybe_age(&expand(file), || {
        pass.map(Into::into).ok_or_else(|| crate::Error::missing(F_FILEPW, "this key file is encrypted: type its password"))
    })?;
    let t = text.trim();
    let m =
        if t.len() == 64 && t.chars().all(|c| c.is_ascii_hexdigit()) { crate::crypto::Key32::from_hex(t)? } else { crate::crypto::from_words(t)? };
    crate::crypto::keyring::verify_master(paths, &m)?;
    Ok(m)
}

fn entrust_run(app: &mut App) {
    let Some(e) = app.network.entrust.as_mut() else { return };
    let to: Vec<Exchange> = e.peers.iter().filter(|(_, on)| *on).map(|(x, _)| x.clone()).collect();
    let (file, filepw, pw, repeat) = (e.form.get(F_FILE), take(e, F_FILEPW), take(e, F_PW), take(e, F_REPEAT));
    let k = e.form.get(F_K).unwrap_or_default();
    if to.len() < 2 {
        return err(app, "pick at least two connected nodes (Space): each holds one shard");
    }
    let Some(file) = file else { return missing(app, F_FILE, "the key file: `mnnn keys export` or your 24 words saved to a file") };
    let Some(pw) = pw else { return missing(app, F_PW, "type a shard password: you need it again to restore") };
    if repeat.as_deref() != Some(pw.as_str()) {
        return missing(app, F_PW, "the two shard passwords differ: type both again");
    }
    let n = to.len();
    let threshold = match k.trim() {
        "" => 0,
        v => match v.parse::<usize>() {
            Ok(t) if (2..=n).contains(&t) => t,
            _ => return missing(app, F_K, &format!("the threshold is how many of the {n} shards restore the key: 2 to {n}")),
        },
    };
    let master = match master_from_file(&app.paths, &file, filepw) {
        Ok(m) => m,
        Err(crate::Error::Missing { what, hint }) => return missing(app, what, &hint),
        Err(e) => return err(app, format!("key file: {e}")),
    };
    if let Some(e) = app.network.entrust.as_mut() {
        e.status = to.iter().map(|x| format!("… {}: sending", x.title())).collect();
    }
    let relays = app.prefs.net.relays.clone();
    spawn(app, "entrust", move |s, node| {
        // every node must answer before any shard leaves, so a set is never half placed
        let mut lines = vec![];
        let mut down = false;
        for x in &to {
            let p = x.peer_id.clone().unwrap_or_default();
            match client::hello(s, node, &p, None, &relays) {
                Ok(_) => lines.push(format!("✓ {}: reachable", x.title())),
                Err(e) => {
                    down = true;
                    lines.push(format!("✗ {}: {e}", x.title()));
                }
            }
        }
        if down {
            lines.insert(0, "nothing was sent: every picked node has to be online".into());
            return Ok(Done::Lines(lines));
        }
        let ids: Vec<String> = to.iter().map(|x| x.id.clone()).collect();
        let r = client::entrust(s, node, &master, &pw.into(), &ids, threshold, &relays, ORIGIN)?;
        let mut l =
            vec![format!("entrusted: any {} of {} shards restore your key (set {})", r.threshold, r.placed.len(), crate::store::short(&r.set))];
        for p in &r.placed {
            let who = to.iter().find(|x| x.id == p.exchange_id).map(|x| x.title()).unwrap_or_default();
            l.push(format!("✓ {who}: holds shard {}/{}", p.index, p.total));
        }
        Ok(Done::Lines(l))
    });
}

fn check_run(app: &mut App) {
    let Some(e) = app.network.entrust.as_mut() else { return };
    take(e, F_REPEAT);
    take(e, F_FILEPW);
    let Some(pw) = take(e, F_PW) else { return missing(app, F_PW, "type your shard password to check the shards restore your key") };
    e.status = vec!["… asking your connections for their shards".into()];
    let (paths, relays) = (app.paths.clone(), app.prefs.net.relays.clone());
    spawn(app, "restore check", move |s, node| {
        let (key, g) = client::gather(s, node, &pw.into(), None, &[], &relays, ORIGIN)?;
        crate::crypto::keyring::verify_master(&paths, &key)?;
        let mut l = vec![format!("✓ your shards restore this database's key ({} answered, {} needed: {})", g.got, g.threshold, g.from.join(", "))];
        l.extend(g.errors.iter().map(|e| format!("✗ {e}")));
        Ok(Done::Lines(l))
    });
}

fn entrust_lines(e: &EntrustForm, t: &crate::theme::Theme, out: &mut Vec<Line<'static>>) {
    let fg = Style::new().fg(t.fg);
    out.push(muted(t, " Split your master key into shards, each sealed with the shard password, one per node."));
    out.push(muted(t, " Nodes of trusted connections are listed; nothing about the key is stored or logged here."));
    out.push(Line::raw(""));
    if e.peers.is_empty() {
        out.push(muted(t, " (no connected nodes of trusted connections)"));
    }
    for (i, (x, on)) in e.peers.iter().enumerate() {
        let sel = e.on_peers && i == e.psel;
        out.push(Line::from(vec![
            Span::styled(format!(" [{}] {:<20}", if *on { "x" } else { " " }, x.title()), row_style(t, sel)),
            Span::styled(format!(" {}", x.peer_id.as_deref().map(crate::store::short).unwrap_or_default()), Style::new().fg(t.muted)),
        ]));
    }
    let n = e.peers.iter().filter(|(_, on)| *on).count();
    out.push(muted(t, format!(" {n} picked · empty threshold: suggested · file pass only for .age files")));
    out.push(Line::raw(""));
    out.extend(e.form.lines(t));
    if !e.status.is_empty() {
        out.push(Line::raw(""));
        for s in &e.status {
            out.push(Line::styled(format!(" {s}"), fg));
        }
    }
}
