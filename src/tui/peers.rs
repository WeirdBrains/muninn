//! Peers, messages and live sessions (`W`, or Net ▸ Peers, messages & live…). Peers lists
//! your connections and, on Enter, what one offers you (shares, live sessions, executables,
//! sites); Messages is the conversation with the selected peer; Live is the sessions you
//! host or joined, with the text, who is there and where their cursors are. Network work runs
//! on a worker thread so the window stays usable.
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, row_style};
use super::{App, Hit};
use crate::collab::{msg as cmsg, peers as cpeers, run as crun, session, wire};
use crate::ingest::time::fmt_ts;
use crate::net::{Node, identity, local};
use crate::store::{LiveSession, Message, Store};
use libp2p::identity::Keypair;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

const ORIGIN: &str = "tui";

/// A live session on view: (session, docs, who is here and their cursor).
pub type LiveView = (String, BTreeMap<String, String>, Vec<(String, Option<i64>)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Peers,
    Messages,
    Live,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Peers, Tab::Messages, Tab::Live];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Peers => "Peers",
            Tab::Messages => "Messages",
            Tab::Live => "Live",
        }
    }
    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PAsk {
    Message,
    Request,
    Run,
    Join,
    Share,
    Edit(String, String),
}

/// What a worker sends back: lines to show, and (for a peer) its live offers.
pub struct Done {
    pub lines: Vec<String>,
    pub offers: Option<Vec<(String, String)>>,
    /// (session, docs, here) for a live session view.
    pub view: Option<LiveView>,
}

impl Done {
    fn lines(l: Vec<String>) -> Self {
        Done { lines: l, offers: None, view: None }
    }
}

pub struct Job {
    pub what: String,
    rx: mpsc::Receiver<Result<Done, String>>,
}

#[derive(Default)]
pub struct PeersState {
    pub tab: Tab,
    pub sel: usize,
    /// The selected peer (its peer id).
    pub peer: Option<String>,
    pub peers: Vec<cpeers::Peer>,
    pub messages: Vec<Message>,
    pub sessions: Vec<LiveSession>,
    /// The selected peer's live offers: (session, title).
    pub offers: Vec<(String, String)>,
    /// The live session on view: (session, docs, who is here and their cursor).
    pub view: Option<LiveView>,
    pub ask: Option<Ask<PAsk>>,
    pub msg: Option<(String, bool)>,
    pub detail: Vec<String>,
    pub job: Option<Job>,
}

impl PeersState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    fn current(&self) -> Option<&cpeers::Peer> {
        let p = self.peer.as_deref()?;
        self.peers.iter().find(|x| x.peer == p)
    }
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.peers.msg = Some((m.into(), false));
}

fn err(app: &mut App, m: impl Into<String>) {
    app.peers.msg = Some((m.into(), true));
}

pub fn open(app: &mut App, tab: Tab) {
    let st = &mut app.peers;
    st.ask = None;
    st.msg = None;
    if st.tab != tab {
        st.sel = 0;
        st.detail.clear();
    }
    st.tab = tab;
    reload(app);
    app.modal = Some(super::modal::Modal::Peers);
}

fn reload(app: &mut App) {
    let s = &app.store;
    let st = &mut app.peers;
    st.peers = cpeers::list(s).unwrap_or_default();
    if st.current().is_none() {
        st.peer = st.peers.first().map(|p| p.peer.clone());
    }
    st.messages = s.messages(st.peer.as_deref(), None).unwrap_or_default();
    st.sessions = s.live_sessions().unwrap_or_default().into_iter().filter(|l| l.kind != "run").collect();
    let n = rows(app);
    app.peers.sel = app.peers.sel.min(n.saturating_sub(1));
}

fn rows(app: &App) -> usize {
    let st = &app.peers;
    match st.tab {
        Tab::Peers => st.peers.len(),
        Tab::Messages => st.messages.len(),
        Tab::Live => st.sessions.len(),
    }
}

fn label(app: &App) -> String {
    local::my_label(&app.store, &app.prefs.net)
}

/// Network work on a worker thread (with the TUI's node if it runs).
fn spawn(app: &mut App, what: &str, f: impl FnOnce(&Store, &Node, &Keypair) -> crate::Result<Done> + Send + 'static) {
    if app.peers.job.is_some() {
        return err(app, "still working on the last request");
    }
    let kp = match identity::load_or_create(&app.store) {
        Ok(k) => k,
        Err(e) => return err(app, e.to_string()),
    };
    let node = app.network.hosted.as_ref().filter(|h| h.node.running()).map(|h| h.node.clone());
    let (db, dek, label, prefs) = (app.paths.db.clone(), app.dek.clone(), label(app), app.prefs.net.clone());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = (|| -> crate::Result<Done> {
            let s = Store::open(&db, Some(&dek.0))?;
            let node = match node {
                Some(n) => n,
                None => Arc::new(local::ephemeral(&kp, Store::open(&db, Some(&dek.0))?, &label, &prefs)?),
            };
            f(&s, &node, &kp)
        })();
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    app.peers.job = Some(Job { what: what.to_string(), rx });
    ok(app, format!("{what}… (the window stays usable)"));
}

/// Between frames: take finished work.
pub fn poll(app: &mut App) {
    let Some(job) = app.peers.job.as_ref() else { return };
    let r = match job.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Err("the task stopped".into()),
    };
    let what = app.peers.job.take().map(|j| j.what).unwrap_or_default();
    app.refresh_history(true);
    reload(app);
    match r {
        Err(e) => err(app, format!("{what}: {e}")),
        Ok(d) => {
            ok(app, d.lines.first().cloned().unwrap_or_else(|| format!("{what}: done")));
            app.peers.detail = d.lines.into_iter().skip(1).collect();
            if let Some(o) = d.offers {
                app.peers.offers = o;
            }
            if d.view.is_some() {
                app.peers.view = d.view;
            }
        }
    }
}

fn me(kp: &Keypair) -> String {
    kp.public().to_peer_id().to_string()
}

fn ask(app: &mut App, kind: PAsk, title: &str, v: &str) {
    app.peers.ask = Some(Ask::new(kind, title, v));
}

/// A guest's round with its host, on the worker.
fn guest_round(s: &Store, node: &Node, relays: &[String], ls: &LiveSession, frames: Vec<crate::collab::proto::Frame>) -> crate::Result<Done> {
    let xid = ls.exchange_id.clone().unwrap_or_default();
    let x = s.exchange(&xid)?.ok_or_else(|| crate::Error::Other("the connection to the host is gone".into()))?;
    let t = wire::Libp2p { store: s, node, exchange: &x, relays };
    let (st, here) = session::sync_with(s, ls, frames, |b| wire::Transport::live(&t, b))?;
    let closed = s.live_session(&ls.id)?.is_some_and(|l| l.status == "closed");
    Ok(Done {
        lines: vec![format!("{} {}{}", ls.kind, ls.title, if closed { " (closed by the host)" } else { "" })],
        offers: None,
        view: Some((ls.id.clone(), st.docs(), here.into_iter().map(|c| (c.label, c.offset)).collect())),
    })
}

/// Show a session: hosts read their own log, guests sync with the host.
fn view(app: &mut App, ls: LiveSession) {
    if ls.role == "guest" && ls.status == "open" {
        let relays = app.prefs.net.relays.clone();
        spawn(app, "syncing", move |s, n, _| guest_round(s, n, &relays, &ls, vec![]));
        return;
    }
    let kp = identity::load_or_create(&app.store).ok();
    if let Some(k) = &kp {
        let _ = session::absorb(&app.store, &ls, &me(k));
    }
    let st = session::state(&app.store, &ls.id).unwrap_or_default();
    let here = app.store.presence(&ls.id, session::PRESENCE_MS).unwrap_or_default().into_iter().map(|p| (p.label, p.cursor)).collect();
    app.peers.view = Some((ls.id.clone(), st.docs(), here));
    ok(app, format!("{} {} ({})", ls.kind, ls.title, ls.status));
}

fn finish_ask(app: &mut App, kind: PAsk, v: String) {
    let relays = app.prefs.net.relays.clone();
    let my = label(app);
    if let PAsk::Edit(id, doc) = kind {
        return edit(app, &id, &doc, v);
    }
    let Some(peer) = app.peers.peer.clone() else {
        return err(app, "no peer selected");
    };
    match kind {
        PAsk::Message | PAsk::Request => {
            if v.is_empty() {
                return;
            }
            let about = (kind == PAsk::Request).then(|| "request:peers".to_string());
            spawn(app, "sending", move |s, n, kp| {
                let x = wire::exchange_to(s, &peer, None)?;
                let t = wire::Libp2p { store: s, node: n, exchange: &x, relays: &relays };
                let m = cmsg::send(s, &t, &x, &me(kp), &my, about.as_deref(), &v, ORIGIN)?;
                Ok(Done::lines(vec![if m.delivered_at.is_some() {
                    format!("delivered to {}", x.title())
                } else {
                    format!("kept: {} is not reachable now; the running node keeps trying", x.title())
                }]))
            });
        }
        PAsk::Run => {
            let mut w = v.splitn(3, ' ');
            let (org, endpoint, input) =
                (w.next().unwrap_or("").to_string(), w.next().unwrap_or("").to_string(), w.next().unwrap_or("{}").to_string());
            let input: serde_json::Value = match serde_json::from_str(&input) {
                Ok(i) => i,
                Err(e) => return err(app, format!("input: {e}")),
            };
            if endpoint.is_empty() {
                return err(app, "give ORG ENDPOINT [JSON]");
            }
            spawn(app, "running", move |s, n, _| {
                let x = wire::exchange_to(s, &peer, Some(&org))?;
                let t = wire::Libp2p { store: s, node: n, exchange: &x, relays: &relays };
                let out = crun::run(s, &t, &peer, &x.id, &org, &endpoint, input, |_| {})?;
                let mut l = vec![format!("{endpoint} of {org} ran on {}", x.title())];
                l.extend(serde_json::to_string_pretty(&out).unwrap_or_default().lines().take(200).map(String::from));
                Ok(Done::lines(l))
            });
        }
        PAsk::Join => {
            let id = app.peers.offers.iter().find(|(s, _)| s.starts_with(v.trim())).map(|o| o.0.clone()).unwrap_or(v.trim().to_string());
            spawn(app, "joining", move |s, n, _| {
                let x = wire::exchange_to(s, &peer, None)?;
                let t = wire::Libp2p { store: s, node: n, exchange: &x, relays: &relays };
                let offers = cpeers::offers(&t)?;
                if !offers["live"].as_array().into_iter().flatten().any(|o| o["session"] == id.as_str()) {
                    return Err(crate::Error::Other(format!("{} has not invited you to {id}", x.title())));
                }
                let ls = session::joined(s, &id, &peer, &x.id, &serde_json::json!({}))?;
                guest_round(s, n, &relays, &ls, vec![])
            });
            app.peers.tab = Tab::Live;
        }
        PAsk::Share => {
            let (k, key) = v.trim().split_once(' ').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or_default();
            let r = identity::load_or_create(&app.store)
                .and_then(|kp| session::host(&app.store, ORIGIN, &me(&kp), &k, &key, std::slice::from_ref(&peer)));
            match r {
                Ok(ls) => {
                    reload(app);
                    app.peers.tab = Tab::Live;
                    app.peers.sel = app.peers.sessions.iter().position(|l| l.id == ls.id).unwrap_or(0);
                    ok(app, format!("hosting {} {} live (the running node answers guests)", ls.kind, ls.title));
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        PAsk::Edit(..) => {}
    }
}

fn edit(app: &mut App, id: &str, doc: &str, v: String) {
    let relays = app.prefs.net.relays.clone();
    let my = label(app);
    let Ok(Some(ls)) = app.store.live_session(id) else { return };
    let changes = BTreeMap::from([(doc.to_string(), v)]);
    if ls.role == "host" {
        let r = identity::load_or_create(&app.store).and_then(|kp| session::host_edit(&app.store, &ls, &me(&kp), &changes));
        match r {
            Ok(_) => view(app, ls),
            Err(e) => err(app, e.to_string()),
        }
    } else {
        spawn(app, "editing", move |s, n, kp| {
            let st = session::state(s, &ls.id)?;
            let cursor = changes.values().next().map(|t| t.chars().count() as i64);
            let frames = session::edits(&st, &me(kp), &my, &ls.kind, &changes, cursor)?;
            guest_round(s, n, &relays, &ls, frames)
        });
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.peers.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.peers.ask = None,
            Typed::Done(v) => {
                let kind = a.kind.clone();
                app.peers.ask = None;
                finish_ask(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    let n = rows(app);
    let sel = app.peers.sel;
    match k.code {
        KeyCode::Esc => {
            if app.peers.view.is_some() || !app.peers.detail.is_empty() {
                app.peers.view = None;
                app.peers.detail.clear();
            } else {
                app.modal = None;
            }
            return;
        }
        KeyCode::Tab | KeyCode::Right => return open(app, Tab::ALL[(app.peers.tab.index() + 1) % 3]),
        KeyCode::BackTab | KeyCode::Left => return open(app, Tab::ALL[(app.peers.tab.index() + 2) % 3]),
        KeyCode::Char(c @ '1'..='3') => return open(app, Tab::ALL[c as usize - '1' as usize]),
        KeyCode::Up | KeyCode::Char('k') => app.peers.sel = sel.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => app.peers.sel = (sel + 1).min(n.saturating_sub(1)),
        KeyCode::Char('m') => return ask(app, PAsk::Message, "Message", ""),
        _ => {}
    }
    match (app.peers.tab, k.code) {
        (Tab::Peers, KeyCode::Up | KeyCode::Down | KeyCode::Char('k') | KeyCode::Char('j')) => {
            app.peers.peer = app.peers.peers.get(app.peers.sel).map(|p| p.peer.clone());
            app.peers.offers.clear();
            app.peers.detail.clear();
        }
        (Tab::Peers, KeyCode::Enter) => {
            let Some(p) = app.peers.peers.get(sel).cloned() else { return };
            app.peers.peer = Some(p.peer.clone());
            let relays = app.prefs.net.relays.clone();
            spawn(app, "asking what it offers", move |s, n, _| {
                let x = wire::exchange_to(s, &p.peer, None)?;
                let shares = match crate::net::client::send(s, n, &x, crate::net::proto::Req::List, &relays) {
                    Ok(crate::net::proto::Resp::Shared { items }) => items,
                    _ => vec![],
                };
                let t = wire::Libp2p { store: s, node: n, exchange: &x, relays: &relays };
                let o = cpeers::offers(&t)?;
                let st = |i: &serde_json::Value, k: &str| i.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let mut l = vec![format!("{} offers you:", p.label)];
                l.extend(shares.iter().map(|i| format!("shared  {} {} (open it on Net ▸ Shared data)", i.kind.label(), i.name)));
                let live: Vec<_> = o["live"].as_array().cloned().unwrap_or_default();
                l.extend(live.iter().map(|i| format!("live    {} {} (J joins)", st(i, "kind"), st(i, "title"))));
                for i in o["executables"].as_array().into_iter().flatten() {
                    l.push(format!("run     {} {} · {} (x runs)", st(i, "org"), st(i, "endpoint"), st(i, "kind")));
                }
                for site in &p.sites {
                    l.push(format!(
                        "site    {} {}",
                        st(site, "name"),
                        site["urls"].as_array().and_then(|u| u.first()).and_then(|u| u.as_str()).unwrap_or("")
                    ));
                }
                if l.len() == 1 {
                    l.push("nothing yet".into());
                }
                Ok(Done { lines: l, offers: Some(live.iter().map(|i| (st(i, "session"), st(i, "title"))).collect()), view: None })
            });
        }
        (Tab::Peers, KeyCode::Char('a')) => ask(app, PAsk::Request, "Ask for (a share, an invitation, access)", ""),
        (Tab::Peers, KeyCode::Char('x')) => ask(app, PAsk::Run, "Run ORG ENDPOINT [JSON input]", ""),
        (Tab::Peers, KeyCode::Char('J')) => {
            let first = app.peers.offers.first().map(|o| o.0.clone()).unwrap_or_default();
            ask(app, PAsk::Join, "Join live session (id)", &first)
        }
        (Tab::Peers, KeyCode::Char('s')) => ask(app, PAsk::Share, "Edit live with this peer: entry|journal|code|record KEY", "entry "),
        (Tab::Messages, KeyCode::Char('r')) => {
            let k = app.store.mark_messages_read(app.peers.peer.as_deref()).unwrap_or(0);
            ok(app, format!("{k} marked read"));
        }
        (Tab::Live, KeyCode::Enter) => {
            if let Some(ls) = app.peers.sessions.get(sel).cloned() {
                view(app, ls);
            }
        }
        (Tab::Live, KeyCode::Char('e')) => {
            let Some(ls) = app.peers.sessions.get(sel).cloned() else { return };
            if ls.status != "open" {
                return err(app, "this session is closed");
            }
            let docs = app.peers.view.as_ref().filter(|v| v.0 == ls.id).map(|v| v.1.clone()).unwrap_or_default();
            let Some(doc) = session::default_doc(&ls.kind, &docs).or_else(|| docs.keys().next().cloned()) else {
                return err(app, "Enter shows the session first");
            };
            let now = docs.get(&doc).cloned().unwrap_or_default();
            ask(app, PAsk::Edit(ls.id.clone(), doc.clone()), &format!("Edit {doc} (for long text: mnnn live edit)"), &now);
        }
        (Tab::Live, KeyCode::Char('x')) => {
            let Some(ls) = app.peers.sessions.get(sel).cloned() else { return };
            let r = if ls.role == "host" {
                identity::load_or_create(&app.store).and_then(|kp| session::stop(&app.store, ORIGIN, &ls, &me(&kp)))
            } else {
                app.store.set_live_session(&ls.id, Some("closed"), None)
            };
            match r {
                Ok(()) => ok(app, format!("{} {}", if ls.role == "host" { "closed" } else { "left" }, ls.title)),
                Err(e) => err(app, e.to_string()),
            }
        }
        _ => {}
    }
    if matches!(app.modal, Some(super::modal::Modal::Peers)) {
        reload(app);
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.peers.ask.is_some() {
        return;
    }
    if app.peers.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < rows(app) {
        app.peers.sel = i;
        if app.peers.tab == Tab::Peers {
            app.peers.peer = app.peers.peers.get(i).map(|p| p.peer.clone());
            app.peers.offers.clear();
            app.peers.detail.clear();
        }
    }
}

/// A document with the others' cursors marked (`▏name`).
pub fn with_cursors(text: &str, here: &[(String, Option<i64>)]) -> String {
    let mut marks: Vec<(usize, &str)> = here.iter().filter_map(|(l, c)| c.map(|c| (c.max(0) as usize, l.as_str()))).collect();
    marks.sort();
    let mut out = String::new();
    let mut m = marks.iter().peekable();
    for (i, ch) in text.chars().enumerate() {
        while let Some((_, l)) = m.next_if(|(c, _)| *c <= i) {
            out.push_str(&format!("▏{l}▕"));
        }
        out.push(ch);
    }
    for (_, l) in m {
        out.push_str(&format!("▏{l}▕"));
    }
    out
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.peers;
    let tabs: Vec<String> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, x)| if *x == st.tab { format!("[{} {}]", i + 1, x.title()) } else { format!(" {} {} ", i + 1, x.title()) })
        .collect();
    let busy = st.job.as_ref().map(|j| format!(" · {}…", j.what)).unwrap_or_default();
    let unread = app.store.unread_messages().unwrap_or(0);
    let mut v = View::new(format!("Peers · {}{}{busy}", st.tab.title(), if unread > 0 { format!(" · {unread} unread") } else { String::new() }));
    v.head.push(Line::styled(tabs.join(" "), Style::new().fg(t.accent)));
    let fg = Style::new().fg(t.fg);
    let mt = Style::new().fg(t.muted);
    if let Some(p) = st.current() {
        v.head.push(muted(&t, format!(" with {} (…{})", p.label, crate::store::net::short_peer(&p.peer))));
    }
    v.head.push(Line::raw(""));
    match st.tab {
        Tab::Peers => {
            if st.peers.is_empty() {
                v.head.push(muted(&t, " no peers yet: connect on Net ▸ Invite or connect…"));
            }
            for (i, p) in st.peers.iter().enumerate() {
                let mark = if st.peer.as_deref() == Some(p.peer.as_str()) { "▸" } else { " " };
                let extra =
                    [(p.unread > 0).then(|| format!("{} unread", p.unread)), (!p.sites.is_empty()).then(|| format!("{} site(s)", p.sites.len()))]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" · ");
                v.rows.push(Line::from(vec![
                    Span::styled(format!("{mark}{:<28} {:<20}", p.label, p.scopes.join(",")), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {extra}"), mt),
                ]));
            }
        }
        Tab::Messages => {
            if st.messages.is_empty() {
                v.head.push(muted(&t, " no messages yet: m writes one"));
            }
            for (i, m) in st.messages.iter().enumerate() {
                let arrow = if m.direction == "in" { "←" } else { "→" };
                let mark = if m.direction == "in" && m.read_at.is_none() { "●" } else { " " };
                let wait = if m.direction == "out" && m.delivered_at.is_none() { " (waiting)" } else { "" };
                let about = m.about.as_deref().map(|a| format!(" [{a}]")).unwrap_or_default();
                v.rows.push(Line::from(vec![
                    Span::styled(format!("{mark}{arrow} {:<14} {}", m.peer_label, m.text), row_style(&t, i == st.sel)),
                    Span::styled(format!("{about} {}{wait}", fmt_ts(m.created_at)), mt),
                ]));
            }
        }
        Tab::Live => {
            if st.sessions.is_empty() {
                v.head.push(muted(&t, " none yet: s on Peers shares an item live; J joins one you were invited to"));
            }
            for (i, l) in st.sessions.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<28} {:<8} {:<6} {:<6}", l.title, l.kind, l.role, l.status), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {}", if l.role == "host" { format!("{} invited", l.peers.len()) } else { "joined".into() }), mt),
                ]));
            }
            if let Some((_, docs, here)) = &st.view {
                let who: Vec<String> = here.iter().map(|(l, c)| format!("{l}{}", c.map(|c| format!(" at {c}")).unwrap_or_default())).collect();
                v.head.push(muted(&t, format!(" here: {}", if who.is_empty() { "nobody else".into() } else { who.join(", ") })));
                for (doc, text) in docs {
                    v.head.push(Line::styled(format!(" ── {doc}"), Style::new().fg(t.accent)));
                    for line in with_cursors(text, here).lines().take(40) {
                        v.head.push(Line::styled(format!(" {line}"), fg));
                    }
                }
            }
        }
    }
    if !st.detail.is_empty() {
        v.head.push(Line::raw(""));
        for d in &st.detail {
            v.head.push(Line::styled(format!(" {d}"), fg));
        }
    }
    v.sel = (!v.rows.is_empty()).then_some(st.sel);
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    v.keys = match st.tab {
        Tab::Peers => vec![
            ("Enter offers", KeyCode::Enter),
            ("m message", KeyCode::Char('m')),
            ("a ask for", KeyCode::Char('a')),
            ("J join", KeyCode::Char('J')),
            ("s share live", KeyCode::Char('s')),
            ("x run", KeyCode::Char('x')),
            ("Esc", KeyCode::Esc),
        ],
        Tab::Messages => vec![("m write", KeyCode::Char('m')), ("r all read", KeyCode::Char('r')), ("Esc", KeyCode::Esc)],
        Tab::Live => vec![("Enter show", KeyCode::Enter), ("e edit", KeyCode::Char('e')), ("x stop", KeyCode::Char('x')), ("Esc", KeyCode::Esc)],
    };
    draw_view(f, r, v, &t, hits);
}
