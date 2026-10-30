//! Organizations ▸ … (G): my orgs, their members, roles and resources, the orgs I joined
//! and the site. Thin over [`crate::org`]; anything that talks to other nodes runs on a
//! worker thread so the window stays usable.
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, record, row_style};
use super::{App, Hit};
use crate::net::{Node, client, identity, local};
use crate::org::host::{self, Acme, Host, HostOptions};
use crate::org::net as onet;
use crate::org::{self, Actor, Leftovers};
use crate::store::{
    Access, Joined, Member, MemberStatus, Mirror, Org, OrgChange, Published, Resource, ResourceState, Role, ShareKind, Store, Visibility,
};
use libp2p::identity::Keypair;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const ORIGIN: &str = "tui";
const OWNER: Actor = Actor::Owner;

/// The window's pages (Tab / ← → / 1-6 switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Orgs,
    Members,
    Roles,
    Resources,
    Joined,
    Site,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Orgs, Tab::Members, Tab::Roles, Tab::Resources, Tab::Joined, Tab::Site];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Orgs => "Organizations",
            Tab::Members => "Members",
            Tab::Roles => "Roles",
            Tab::Resources => "Resources",
            Tab::Joined => "Joined",
            Tab::Site => "Site",
        }
    }
    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// What a prompt is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAsk {
    NewOrg,
    Rename,
    Invite,
    MemberRole,
    Kick,
    NewRole,
    RolePerms,
    AddResource,
    Browse,
    Apply,
    Join,
    Acme,
}

/// Something that needs `y` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    RemoveOrg(String),
    RemoveRole(String),
    Release(String),
    Leave(String),
}

pub struct Job {
    pub what: String,
    rx: mpsc::Receiver<Result<Vec<String>, String>>,
}

#[derive(Default)]
pub struct OrgsState {
    pub tab: Tab,
    pub sel: usize,
    /// The org the Members, Roles and Resources pages show.
    pub org: Option<String>,
    pub ask: Option<Ask<OAsk>>,
    pub confirm: Option<Confirm>,
    pub msg: Option<(String, bool)>,
    pub detail: Vec<String>,
    pub job: Option<Job>,
    pub orgs: Vec<Org>,
    pub members: Vec<Member>,
    pub roles: Vec<Role>,
    pub resources: Vec<Resource>,
    pub joined: Vec<Joined>,
    pub mirrors: Vec<Mirror>,
    /// The site this TUI serves.
    pub host: Option<Host>,
    last_status: Option<Instant>,
}

impl OrgsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    pub fn current(&self) -> Option<&Org> {
        let id = self.org.as_deref()?;
        self.orgs.iter().find(|o| o.id == id)
    }
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.orgs.msg = Some((m.into(), false));
}

fn err(app: &mut App, m: impl Into<String>) {
    app.orgs.msg = Some((m.into(), true));
}

pub fn open(app: &mut App, tab: Tab) {
    let st = &mut app.orgs;
    st.ask = None;
    st.confirm = None;
    st.msg = None;
    st.detail.clear();
    if st.tab != tab {
        st.sel = 0;
    }
    st.tab = tab;
    reload(app);
    app.modal = Some(super::modal::Modal::Orgs);
}

/// Open on the Organizations page with the new-org prompt.
pub fn open_new(app: &mut App) {
    open(app, Tab::Orgs);
    app.orgs.ask = Some(Ask::new(OAsk::NewOrg, "New organization name", ""));
}

fn reload(app: &mut App) {
    let s = &app.store;
    let st = &mut app.orgs;
    st.orgs = s.orgs().unwrap_or_default();
    if st.current().is_none() {
        st.org = st.orgs.first().map(|o| o.id.clone());
    }
    let id = st.org.clone().unwrap_or_default();
    st.members = s.members(&id).unwrap_or_default();
    st.roles = s.roles(&id).unwrap_or_default();
    st.resources = s.resources(&id).unwrap_or_default();
    st.joined = s.joined().unwrap_or_default();
    st.mirrors = s.mirrors().unwrap_or_default();
    let n = rows(app);
    app.orgs.sel = app.orgs.sel.min(n.saturating_sub(1));
}

fn rows(app: &App) -> usize {
    let st = &app.orgs;
    match st.tab {
        Tab::Orgs => st.orgs.len(),
        Tab::Members => st.members.len(),
        Tab::Roles => st.roles.len(),
        Tab::Resources => st.resources.len(),
        Tab::Joined => st.joined.len(),
        Tab::Site => 0,
    }
}

fn label(app: &App) -> String {
    local::my_label(&app.store, &app.prefs.net)
}

/// Run work that talks to other nodes on a worker thread (with the TUI's node if it runs).
fn spawn(app: &mut App, what: &str, f: impl FnOnce(&Store, &Node, &Keypair) -> crate::Result<Vec<String>> + Send + 'static) {
    if app.orgs.job.is_some() {
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
        let r = (|| -> crate::Result<Vec<String>> {
            let s = Store::open(&db, Some(&dek.0))?;
            let node = match node {
                Some(n) => n,
                None => Arc::new(local::ephemeral(&kp, Store::open(&db, Some(&dek.0))?, &label, &prefs)?),
            };
            f(&s, &node, &kp)
        })();
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    app.orgs.job = Some(Job { what: what.to_string(), rx });
    ok(app, format!("{what}… (the window stays usable)"));
}

/// Between frames: take finished work, keep the site's status fresh.
pub fn poll(app: &mut App) {
    if app.orgs.host.is_some() {
        if host::stop_requested(&app.paths.home) {
            stop_host(app);
        } else if app.orgs.last_status.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
            if let Some(h) = &app.orgs.host {
                let _ = host::write_status(&app.paths.home, &h.status());
            }
            app.orgs.last_status = Some(Instant::now());
        }
    }
    let Some(job) = app.orgs.job.as_ref() else { return };
    let r = match job.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Err("the task stopped".into()),
    };
    let what = app.orgs.job.take().map(|j| j.what).unwrap_or_default();
    app.refresh_history(true);
    reload(app);
    match r {
        Err(e) => {
            app.set_status(format!("{what}: {e}"));
            err(app, format!("{what}: {e}"));
        }
        Ok(lines) => {
            let first = lines.first().cloned().unwrap_or_else(|| format!("{what}: done"));
            app.set_status(first.clone());
            ok(app, first);
            app.orgs.detail = lines.into_iter().skip(1).collect();
        }
    }
}

/// Serve the hosted orgs (Organizations ▸ Start site).
pub fn start_host(app: &mut App) {
    if app.orgs.host.is_some() {
        return ok(app, "the site is already being served");
    }
    if let Some(f) = host::running(&app.paths.home) {
        return err(app, format!("already served at {} by process {}", f.url, f.pid));
    }
    let r = (|| -> crate::Result<Host> {
        let opts = HostOptions::from_prefs(&app.prefs.hosting)?;
        Host::start(Store::open(&app.paths.db, Some(&app.dek.0))?, opts)
    })();
    match r {
        Ok(h) => {
            let st = h.status();
            let _ = host::write_status(&app.paths.home, &st);
            app.orgs.last_status = Some(Instant::now());
            record(
                app,
                "org.host.start",
                format!("site served at {}", st.url),
                serde_json::json!({ "addr": st.addr, "orgs": st.orgs, "mirrors": st.mirrors, "cert": st.cert.source }),
            );
            app.set_status(format!("site at {}", st.url));
            ok(app, format!("serving at {}", st.url));
            app.orgs.host = Some(h);
        }
        Err(e) => err(app, format!("site: {e}")),
    }
}

/// Stop the site this TUI serves, or ask the one another process serves to stop.
pub fn stop_host(app: &mut App) {
    if let Some(h) = app.orgs.host.take() {
        h.stop();
        host::clear(&app.paths.home);
        record(app, "org.host.stop", "site stopped".into(), serde_json::json!({}));
        app.set_status("site stopped");
        ok(app, "site stopped");
    } else if let Some(f) = host::running(&app.paths.home) {
        match host::request_stop(&app.paths.home) {
            Ok(()) => ok(app, format!("asked the site at {} to stop", f.url)),
            Err(e) => err(app, e.to_string()),
        }
    } else {
        ok(app, "the site is not being served");
    }
}

/// Stop our site when the TUI closes.
pub fn shutdown(app: &mut App) {
    if let Some(h) = app.orgs.host.take() {
        h.stop();
        host::clear(&app.paths.home);
    }
}

fn done<T>(app: &mut App, r: crate::Result<T>, msg: impl FnOnce(&T) -> String) {
    match r {
        Ok(v) => {
            let m = msg(&v);
            reload(app);
            app.refresh_history(true);
            app.set_status(m.clone());
            ok(app, m);
        }
        Err(e) => err(app, e.to_string()),
    }
}

fn edit(app: &mut App, c: OrgChange) {
    let Some(o) = sel_org(app) else { return };
    let r = org::edit(&app.store, ORIGIN, &OWNER, &o, &c);
    done(app, r, |o| format!("{}: {}, {}{}", o.name, o.visibility.as_str(), o.access.as_str(), if o.hosted { ", hosted" } else { "" }));
}

fn sel_org(app: &App) -> Option<Org> {
    match app.orgs.tab {
        Tab::Orgs => app.orgs.orgs.get(app.orgs.sel).cloned(),
        _ => app.orgs.current().cloned(),
    }
}

fn sel_member(app: &App) -> Option<(Org, Member)> {
    Some((app.orgs.current()?.clone(), app.orgs.members.get(app.orgs.sel)?.clone()))
}

fn sel_role(app: &App) -> Option<(Org, Role)> {
    Some((app.orgs.current()?.clone(), app.orgs.roles.get(app.orgs.sel)?.clone()))
}

fn sel_resource(app: &App) -> Option<(Org, Resource)> {
    Some((app.orgs.current()?.clone(), app.orgs.resources.get(app.orgs.sel)?.clone()))
}

fn words(v: &str) -> Vec<String> {
    v.split([' ', ',']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn finish_ask(app: &mut App, kind: OAsk, v: String) {
    let relays = app.prefs.net.relays.clone();
    match kind {
        OAsk::NewOrg => {
            if v.is_empty() {
                return;
            }
            let r = org::create(&app.store, ORIGIN, &v, None, "");
            if let Ok(o) = &r {
                app.orgs.org = Some(o.id.clone());
            }
            done(app, r, |o| format!("created {} ({}): private, members only", o.name, o.slug));
            app.orgs.sel = app.orgs.orgs.iter().position(|o| Some(&o.id) == app.orgs.org.as_ref()).unwrap_or(0);
        }
        OAsk::Rename => {
            if !v.is_empty() {
                edit(app, OrgChange { name: Some(v), ..Default::default() })
            }
        }
        OAsk::Invite => {
            let Some(o) = app.orgs.current().cloned() else { return };
            let my = label(app);
            spawn(app, "invite", move |s, n, kp| {
                let to = (!v.is_empty()).then_some(v.as_str());
                let (m, c) = onet::invite(s, to.map(|_| n), kp, &OWNER, &o, None, to, "", &relays, ORIGIN)?;
                let mut l = vec![format!("invited {} to {} as {}", m.title(), o.name, org::role_name(s, &m))];
                if let Some(d) = client::describe_delivery(&c.delivered) {
                    l.push(d);
                }
                l.push(format!("Invitation token from {my} (they join with `mnnn org join TOKEN` or j on Joined):"));
                l.push(c.token);
                Ok(l)
            });
        }
        OAsk::MemberRole => {
            let Some((o, m)) = sel_member(app) else { return };
            let r = app.store.find_role(&o.id, &v).and_then(|r| org::set_role(&app.store, ORIGIN, &OWNER, &o, &m, &r));
            done(app, r, |m| format!("{} is now {v}", m.title()));
        }
        OAsk::Kick => {
            let Some((o, m)) = sel_member(app) else { return };
            let left = match Leftovers::parse(if v.is_empty() { "keep" } else { &v }) {
                Ok(l) => l,
                Err(e) => return err(app, e.to_string()),
            };
            spawn(app, "kick", move |s, n, _| {
                let m = onet::kick(s, Some(n), &OWNER, &o, &m, left, &relays, ORIGIN)?;
                Ok(vec![format!("removed {}; what they gave stays with {}", m.title(), o.name)])
            });
        }
        OAsk::NewRole => {
            let Some(o) = app.orgs.current().cloned() else { return };
            let w = words(&v);
            let Some(name) = w.first() else { return };
            let (rank, perms) = match w.get(1).and_then(|r| r.parse::<i64>().ok()) {
                Some(r) => (r, w[2..].to_vec()),
                None => (30, w[1..].to_vec()),
            };
            let r = org::create_role(&app.store, ORIGIN, &OWNER, &o, name, rank, &perms);
            done(app, r, |r| format!("role {} (rank {}): {}", r.name, r.rank, r.perms.join(" ")));
        }
        OAsk::RolePerms => {
            let Some((o, role)) = sel_role(app) else { return };
            let perms = words(&v);
            let r = org::edit_role(&app.store, ORIGIN, &OWNER, &o, &role, None, None, Some(&perms));
            done(app, r, |r| format!("role {}: {}", r.name, r.perms.join(" ")));
        }
        OAsk::AddResource => {
            let Some(o) = app.orgs.current().cloned() else { return };
            let (k, name) = v.split_once(' ').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((v.as_str(), ""));
            let Some(kind) = ShareKind::parse(k) else {
                return err(app, format!("what kind is {k:?}? model, search, journal, project, report or code"));
            };
            let r = org::transfer(&app.store, ORIGIN, &OWNER, &o, kind, name, None);
            done(app, r, |(r, _)| format!("{} {} belongs to {}", kind.label(), r.name, o.name));
        }
        OAsk::Browse => {
            if v.is_empty() {
                return;
            }
            spawn(app, "browse", move |s, n, _| {
                let cards = onet::browse(s, n, &v, None, &relays)?;
                let mut l = vec![format!("{} lists {} organization(s); A applies to one", v, cards.len())];
                l.extend(cards.iter().map(|c| {
                    format!(
                        "{:<20} {:<14} {} · {} · {} members  {}",
                        c.name,
                        c.slug,
                        c.visibility.as_str(),
                        c.access.as_str(),
                        c.members,
                        c.description
                    )
                }));
                Ok(l)
            });
        }
        OAsk::Apply => {
            let w: Vec<&str> = v.splitn(3, ' ').collect();
            if w.len() < 2 {
                return err(app, "give the node and the org (and a note), e.g. …/p2p/ID comet-lab hello");
            }
            let (target, slug, note) = (w[0].to_string(), w[1].to_string(), w.get(2).unwrap_or(&"").to_string());
            let my = label(app);
            spawn(app, "apply", move |s, n, kp| {
                let j = onet::apply(s, n, kp, &target, &slug, &note, &my, &relays, ORIGIN)?;
                Ok(vec![format!("applied to {}: s checks for an answer", j.name)])
            });
        }
        OAsk::Join => {
            if v.is_empty() {
                return;
            }
            let my = label(app);
            let node = app.network.hosted.as_ref().map(|h| h.node.clone());
            let addrs = local::offer_addrs(&app.paths.home, &app.store, node.as_deref());
            spawn(app, "join", move |s, n, kp| {
                let a = client::accept(s, Some(n), kp, &v, "", &my, addrs, ORIGIN)?;
                let mut l = vec![match s.joined_by_exchange(&a.exchange.id)? {
                    Some(j) => format!("joined {} as {}", j.name, j.role),
                    None => "the key exchange is done, but it was not an invitation to an organization".into(),
                }];
                if let Some(t) = a.answer
                    && !a.delivered.as_ref().is_some_and(|d| d.is_ok())
                {
                    l.push("Their node was not reachable; give them this answer token:".into());
                    l.push(t);
                }
                Ok(l)
            });
        }
        OAsk::Acme => {
            let a = &mut app.prefs.hosting.acme;
            if v.eq_ignore_ascii_case("off") || v.is_empty() {
                a.enabled = false;
            } else {
                let w = words(&v);
                if w.len() < 2 {
                    return err(app, "give the domain and an email, or off");
                }
                a.domain = w[0].clone();
                a.email = w[1].clone();
                a.enabled = true;
            }
            if let Err(e) = Acme::from_prefs(a) {
                a.enabled = false;
                return err(app, e.to_string());
            }
            let (on, domain, dir) = (a.enabled, a.domain.clone(), a.directory_url());
            app.save_prefs();
            let summary = if on { format!("ACME on for {domain} ({dir})") } else { "ACME off".to_string() };
            record(app, "org.host.acme", summary.clone(), serde_json::json!({ "enabled": on, "domain": domain, "directory": dir }));
            ok(app, format!("{summary}; applies when the site starts"));
        }
    }
}

fn confirmed(app: &mut App, c: Confirm) {
    let relays = app.prefs.net.relays.clone();
    match c {
        Confirm::RemoveOrg(id) => {
            let Some(o) = app.orgs.orgs.iter().find(|o| o.id == id).cloned() else { return };
            let r = org::remove(&app.store, ORIGIN, &OWNER, &o);
            done(app, r, |o| format!("removed {} (its resources stay here)", o.name));
        }
        Confirm::RemoveRole(id) => {
            let Some((o, role)) = sel_role(app).filter(|(_, r)| r.id == id) else { return };
            let r = org::remove_role(&app.store, ORIGIN, &OWNER, &o, &role);
            done(app, r, |r| format!("removed role {}", r.name));
        }
        Confirm::Release(id) => {
            let Some((o, res)) = sel_resource(app).filter(|(_, r)| r.id == id) else { return };
            let r = org::release(&app.store, ORIGIN, &OWNER, &o, &res);
            done(app, r, |r| format!("{} is no longer {}'s (it stays here)", r.name, o.name));
        }
        Confirm::Leave(id) => {
            let Some(j) = app.orgs.joined.iter().find(|j| j.org_id == id).cloned() else { return };
            spawn(app, "leave", move |s, n, _| {
                let j = onet::leave(s, Some(n), &j, &relays, ORIGIN)?;
                Ok(vec![format!("left {}", j.name)])
            });
        }
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.orgs.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.orgs.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.orgs.ask = None;
                finish_ask(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.orgs.confirm.take() {
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
            if !app.orgs.detail.is_empty() {
                app.orgs.detail.clear();
            } else {
                app.modal = None;
            }
            return;
        }
        KeyCode::Tab | KeyCode::Right => return switch(app, (app.orgs.tab.index() + 1) % Tab::ALL.len()),
        KeyCode::BackTab | KeyCode::Left => return switch(app, (app.orgs.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()),
        KeyCode::Char(c @ '1'..='6') => return switch(app, c as usize - '1' as usize),
        KeyCode::Down | KeyCode::Char('j') if app.orgs.tab != Tab::Joined || k.code == KeyCode::Down => {
            app.orgs.sel = (app.orgs.sel + 1).min(n.saturating_sub(1));
            return;
        }
        KeyCode::Up | KeyCode::Char('k') if app.orgs.tab != Tab::Members || k.code == KeyCode::Up => {
            app.orgs.sel = app.orgs.sel.saturating_sub(1);
            return;
        }
        KeyCode::Char('g') => {
            reload(app);
            return ok(app, "refreshed");
        }
        _ => {}
    }
    let ask = |app: &mut App, kind: OAsk, title: &str, v: &str| app.orgs.ask = Some(Ask::new(kind, title, v));
    let need_org = matches!(app.orgs.tab, Tab::Members | Tab::Roles | Tab::Resources) && app.orgs.current().is_none();
    if need_org {
        return ok(app, "no organization yet: n on the first page makes one");
    }
    match (app.orgs.tab, k.code) {
        (Tab::Orgs, KeyCode::Char('n')) => ask(app, OAsk::NewOrg, "New organization name", ""),
        (Tab::Orgs, KeyCode::Enter) => {
            if let Some(o) = app.orgs.orgs.get(app.orgs.sel) {
                app.orgs.org = Some(o.id.clone());
                switch(app, Tab::Members.index());
            }
        }
        (Tab::Orgs, KeyCode::Char('r')) => {
            if let Some(o) = sel_org(app) {
                ask(app, OAsk::Rename, "Name", &o.name)
            }
        }
        (Tab::Orgs, KeyCode::Char('v')) => {
            if let Some(o) = sel_org(app) {
                let to = if o.visibility == Visibility::Public { Visibility::Private } else { Visibility::Public };
                edit(app, OrgChange { visibility: Some(to), ..Default::default() })
            }
        }
        (Tab::Orgs, KeyCode::Char('o')) => {
            if let Some(o) = sel_org(app) {
                let to = if o.access == Access::Open { Access::Members } else { Access::Open };
                edit(app, OrgChange { access: Some(to), ..Default::default() })
            }
        }
        (Tab::Orgs, KeyCode::Char('h')) => {
            if let Some(o) = sel_org(app) {
                edit(app, OrgChange { hosted: Some(!o.hosted), ..Default::default() })
            }
        }
        (Tab::Orgs, KeyCode::Char('m')) => {
            if let Some(o) = sel_org(app) {
                edit(app, OrgChange { member_auth: Some(!o.member_auth), ..Default::default() })
            }
        }
        (Tab::Orgs, KeyCode::Char('d')) => {
            if let Some(o) = sel_org(app) {
                app.orgs.confirm = Some(Confirm::RemoveOrg(o.id.clone()));
                ok(app, format!("delete {} and its memberships? (its resources stay here) y/n", o.name));
            }
        }
        (Tab::Members, KeyCode::Char('i')) => ask(app, OAsk::Invite, "Invite (their node …/p2p/ID or peer; empty: just a token)", ""),
        (Tab::Members, KeyCode::Enter) => {
            if let Some((_, m)) = sel_member(app) {
                app.orgs.detail = vec![
                    format!("{} · {} · {}", m.title(), org::role_name(&app.store, &m), m.status.as_str()),
                    format!("peer    {}", m.peer_id),
                    format!("since   {}", crate::ingest::time::fmt_ts(m.created_at)),
                ];
                if !m.note.is_empty() {
                    app.orgs.detail.push(format!("note    {}", m.note));
                }
            }
        }
        (Tab::Members, KeyCode::Char('a')) => {
            let Some((o, m)) = sel_member(app) else { return };
            if m.status != MemberStatus::Applied {
                return err(app, format!("{} has not applied", m.title()));
            }
            spawn(app, "approve", move |s, n, kp| {
                let (m, told) = onet::approve(s, Some(n), kp, &OWNER, &o, &m, None, ORIGIN)?;
                let t = if matches!(told, Some(Ok(_))) { "they were told" } else { "they see it when they sync" };
                Ok(vec![format!("{} is a member of {} as {}; {t}", m.title(), o.name, org::role_name(s, &m))])
            });
        }
        (Tab::Members, KeyCode::Char('x')) => {
            let Some((o, m)) = sel_member(app) else { return };
            let r = onet::reject(&app.store, ORIGIN, &OWNER, &o, &m);
            done(app, r, |m| format!("rejected {}", m.title()));
        }
        (Tab::Members, KeyCode::Char('o')) => {
            if let Some((_, m)) = sel_member(app) {
                let r = org::role_name(&app.store, &m);
                ask(app, OAsk::MemberRole, &format!("Role for {}", m.title()), &r)
            }
        }
        (Tab::Members, KeyCode::Char('k')) => {
            if let Some((_, m)) = sel_member(app) {
                ask(app, OAsk::Kick, &format!("Remove {}: what they gave is kept, archived or published", m.title()), "keep")
            }
        }
        (Tab::Roles, KeyCode::Char('n')) => ask(app, OAsk::NewRole, "New role: name [rank] perms… (P lists permissions)", ""),
        (Tab::Roles, KeyCode::Char('e') | KeyCode::Enter) => {
            if let Some((_, r)) = sel_role(app) {
                ask(app, OAsk::RolePerms, &format!("Permissions of {}", r.name), &r.perms.join(" "))
            }
        }
        (Tab::Roles, KeyCode::Char('P')) => {
            app.orgs.detail = org::PERMS.iter().map(|(p, w)| format!("{p:<20} {w}")).collect();
        }
        (Tab::Roles, KeyCode::Char('a') | KeyCode::Char('i')) => {
            let Some((o, r)) = sel_role(app) else { return };
            let apply = k.code == KeyCode::Char('a');
            let c = OrgChange { apply_role: apply.then(|| r.id.clone()), invite_role: (!apply).then(|| r.id.clone()), ..Default::default() };
            let res = onet::defaults(&app.store, ORIGIN, &OWNER, &o, &c);
            done(app, res, |_| format!("new members {} get {}", if apply { "who apply" } else { "invited" }, r.name));
        }
        (Tab::Roles, KeyCode::Char('d')) => {
            if let Some((_, r)) = sel_role(app) {
                app.orgs.confirm = Some(Confirm::RemoveRole(r.id.clone()));
                ok(app, format!("remove role {}? (its members get the lowest remaining role) y/n", r.name));
            }
        }
        (Tab::Resources, KeyCode::Char('a')) => ask(app, OAsk::AddResource, "Give the org: kind name (e.g. journal Field notes)", ""),
        (Tab::Resources, KeyCode::Char('p') | KeyCode::Enter) => {
            let Some((o, r)) = sel_resource(app) else { return };
            let to = match r.published {
                Published::No => Published::Members,
                Published::Members => Published::Public,
                Published::Public => Published::No,
            };
            let res = org::set_published(&app.store, ORIGIN, &OWNER, &o, &r, to);
            done(app, res, |r| format!("{}: published {}", r.name, r.published.as_str()));
        }
        (Tab::Resources, KeyCode::Char('z')) => {
            let Some((o, r)) = sel_resource(app) else { return };
            let to = if r.state == ResourceState::Archived { ResourceState::Active } else { ResourceState::Archived };
            let res = org::set_state(&app.store, ORIGIN, &OWNER, &o, &r, to);
            done(app, res, |r| format!("{}: {}", r.name, r.state.as_str()));
        }
        (Tab::Resources, KeyCode::Char('d')) => {
            if let Some((o, r)) = sel_resource(app) {
                app.orgs.confirm = Some(Confirm::Release(r.id.clone()));
                ok(app, format!("{} is no longer {}'s? (it stays here) y/n", r.name, o.name));
            }
        }
        (Tab::Joined, KeyCode::Char('b')) => ask(app, OAsk::Browse, "Browse the organizations of (…/p2p/ID or a peer)", ""),
        (Tab::Joined, KeyCode::Char('A')) => ask(app, OAsk::Apply, "Apply: node org [note]", ""),
        (Tab::Joined, KeyCode::Char('j')) => ask(app, OAsk::Join, "Invitation token", ""),
        (Tab::Joined, KeyCode::Enter) => {
            if let Some(j) = app.orgs.joined.get(app.orgs.sel) {
                app.orgs.detail = vec![
                    format!("{} ({}) · {} as {}", j.name, j.slug, j.status.as_str(), j.role),
                    format!("node    {}", j.node_peer),
                    format!("can     {}", if j.perms.is_empty() { "-".into() } else { j.perms.join(" ") }),
                ];
            }
        }
        (Tab::Joined, KeyCode::Char('s')) => {
            let Some(j) = app.orgs.joined.get(app.orgs.sel).cloned() else { return };
            let relays = app.prefs.net.relays.clone();
            spawn(app, "sync", move |s, n, _| {
                let j = onet::sync(s, n, &j, &relays, ORIGIN)?;
                Ok(vec![format!("{}: {} as {}", j.name, j.status.as_str(), j.role)])
            });
        }
        (Tab::Joined, KeyCode::Char('m')) => {
            let Some(j) = app.orgs.joined.get(app.orgs.sel).cloned() else { return };
            let relays = app.prefs.net.relays.clone();
            spawn(app, "mirror", move |s, n, _| {
                let m = onet::mirror(s, n, &j, &relays, ORIGIN)?;
                Ok(vec![format!("mirrored {} ({} bytes): the site serves it at /o/{}/", j.name, m.snapshot.len(), m.slug)])
            });
        }
        (Tab::Joined, KeyCode::Char('l')) => {
            if let Some(j) = app.orgs.joined.get(app.orgs.sel) {
                let (id, name) = (j.org_id.clone(), j.name.clone());
                app.orgs.confirm = Some(Confirm::Leave(id));
                ok(app, format!("leave {name}? y/n"));
            }
        }
        (Tab::Site, KeyCode::Char('S') | KeyCode::Enter) => {
            if app.orgs.host.is_some() {
                stop_host(app)
            } else {
                start_host(app)
            }
        }
        (Tab::Site, KeyCode::Char('X')) => stop_host(app),
        (Tab::Site, KeyCode::Char('e')) => {
            let a = &app.prefs.hosting.acme;
            let v = if a.enabled { format!("{} {}", a.domain, a.email) } else { String::new() };
            ask(app, OAsk::Acme, "ACME (Let's Encrypt): domain email, or off", &v)
        }
        _ => {}
    }
}

fn switch(app: &mut App, i: usize) {
    open(app, Tab::ALL[i.min(Tab::ALL.len() - 1)]);
}

pub fn on_click(app: &mut App, i: usize) {
    if app.orgs.ask.is_some() {
        return;
    }
    if app.orgs.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < rows(app) {
        app.orgs.sel = i;
        app.orgs.detail.clear();
    }
}

fn site_lines(app: &App) -> Vec<String> {
    let mut v = vec![];
    let hp = &app.prefs.hosting;
    match (&app.orgs.host, host::running(&app.paths.home)) {
        (Some(h), _) => {
            let st = h.status();
            v.push(format!("● serving here at {}", st.url));
            v.extend(st.orgs.iter().map(|o| format!("  {}o/{o}/", st.url)));
            v.extend(st.mirrors.iter().map(|o| format!("  {}o/{o}/  (mirror)", st.url)));
            v.push(cert_line(&st.cert));
        }
        (None, Some(f)) => {
            v.push(format!("● served by process {} at {}", f.pid, f.url));
            v.push(cert_line(&f.cert));
        }
        (None, None) => v.push(format!("○ not served: S serves it on https://{}:{}/ (localhost unless you change hosting.bind)", hp.bind, hp.port)),
    }
    let hosted: Vec<&str> = app.orgs.orgs.iter().filter(|o| o.hosted).map(|o| o.slug.as_str()).collect();
    v.push(format!("hosted orgs  {}", if hosted.is_empty() { "none (h on an organization turns it on)".into() } else { hosted.join(", ") }));
    v.push(format!("mirrors      {}", app.orgs.mirrors.len()));
    if hp.acme.enabled {
        v.push(format!("ACME         {} for {} ({}), profile {}", hp.acme.directory_url(), hp.acme.domain, hp.acme.email, hp.acme.profile));
    } else {
        v.push(format!("certificate  self-signed, {} days, made when the site starts (e sets up Let's Encrypt)", hp.cert_days));
    }
    v
}

fn cert_line(c: &host::CertInfo) -> String {
    format!("certificate  {} for {} until {}", c.source, c.names.join(", "), crate::ingest::time::fmt_ts(c.not_after))
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.orgs;
    let tabs: Vec<String> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, x)| if *x == st.tab { format!("[{} {}]", i + 1, x.title()) } else { format!(" {} {} ", i + 1, x.title()) })
        .collect();
    let busy = st.job.as_ref().map(|j| format!(" · {}…", j.what)).unwrap_or_default();
    let mut v = View::new(format!("Organizations · {}{busy}", st.tab.title()));
    v.head.push(Line::styled(tabs.join(" "), Style::new().fg(t.accent)));
    let fg = Style::new().fg(t.fg);
    let mt = Style::new().fg(t.muted);
    if matches!(st.tab, Tab::Members | Tab::Roles | Tab::Resources) {
        match st.current() {
            Some(o) => v.head.push(Line::styled(format!(" {} ({}) · Enter on Organizations picks another", o.name, o.slug), fg)),
            None => v.head.push(muted(&t, " no organization yet: n on the Organizations page makes one")),
        }
    }
    v.head.push(Line::raw(""));
    let role_of = |id: &Option<String>| id.as_deref().and_then(|r| st.roles.iter().find(|x| x.id == r)).map(|r| r.name.as_str()).unwrap_or("-");
    match st.tab {
        Tab::Orgs => {
            if st.orgs.is_empty() {
                v.head.push(muted(&t, " no organizations yet: n makes one (private, members only, until you say otherwise)"));
            }
            for (i, o) in st.orgs.iter().enumerate() {
                let mark = if st.org.as_deref() == Some(o.id.as_str()) { "▸" } else { " " };
                v.rows.push(Line::from(vec![
                    Span::styled(format!("{mark}{:<22} {:<16}", o.name, o.slug), row_style(&t, i == st.sel)),
                    Span::styled(
                        format!(
                            " {:<7} {:<7}{}{}",
                            o.visibility.as_str(),
                            o.access.as_str(),
                            if o.hosted { " hosted" } else { "" },
                            if o.member_auth { "" } else { " (no member sign-in)" }
                        ),
                        mt,
                    ),
                ]));
            }
        }
        Tab::Members => {
            for (i, m) in st.members.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<20} {:<10} {:<9}", m.title(), role_of(&m.role_id), m.status.as_str()), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {}", m.note), mt),
                ]));
            }
        }
        Tab::Roles => {
            if let Some(o) = st.current() {
                v.head.push(muted(&t, format!(" new members get {} (apply) and {} (invite)", role_of(&o.apply_role), role_of(&o.invite_role))));
            }
            for (i, r) in st.roles.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<12} rank {:>3}", r.name, r.rank), row_style(&t, i == st.sel)),
                    Span::styled(format!("  {}", r.perms.join(" ")), mt),
                ]));
            }
        }
        Tab::Resources => {
            if st.resources.is_empty() {
                v.head.push(muted(&t, " nothing yet: a gives the org one of your things (members give theirs from their node)"));
            }
            for (i, r) in st.resources.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<8} {:<24}", r.kind.label(), r.name), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {:<8} published: {}", r.state.as_str(), r.published.as_str()), mt),
                ]));
            }
        }
        Tab::Joined => {
            if st.joined.is_empty() {
                v.head.push(muted(&t, " none yet: b browses a node's organizations, A applies, j joins with an invitation"));
            }
            for (i, j) in st.joined.iter().enumerate() {
                let node = &j.node_peer[j.node_peer.len().saturating_sub(8)..];
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<22} {:<9} {:<9}", j.name, j.status.as_str(), j.role), row_style(&t, i == st.sel)),
                    Span::styled(format!(" at …{node}"), mt),
                ]));
            }
        }
        Tab::Site => {
            for l in site_lines(app) {
                v.head.push(Line::styled(format!(" {l}"), fg));
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
    let mut keys: Vec<(&'static str, KeyCode)> = vec![("Tab page", KeyCode::Tab)];
    keys.extend(match st.tab {
        Tab::Orgs => vec![
            ("n new", KeyCode::Char('n')),
            ("Enter pick", KeyCode::Enter),
            ("r rename", KeyCode::Char('r')),
            ("v public", KeyCode::Char('v')),
            ("o open", KeyCode::Char('o')),
            ("h host", KeyCode::Char('h')),
            ("m sign-in", KeyCode::Char('m')),
            ("d delete", KeyCode::Char('d')),
        ],
        Tab::Members => vec![
            ("i invite", KeyCode::Char('i')),
            ("a approve", KeyCode::Char('a')),
            ("x reject", KeyCode::Char('x')),
            ("o role", KeyCode::Char('o')),
            ("k kick", KeyCode::Char('k')),
        ],
        Tab::Roles => vec![
            ("n new", KeyCode::Char('n')),
            ("e perms", KeyCode::Char('e')),
            ("a/i default", KeyCode::Char('a')),
            ("P list", KeyCode::Char('P')),
            ("d remove", KeyCode::Char('d')),
        ],
        Tab::Resources => {
            vec![
                ("a add", KeyCode::Char('a')),
                ("p publish", KeyCode::Char('p')),
                ("z archive", KeyCode::Char('z')),
                ("d release", KeyCode::Char('d')),
            ]
        }
        Tab::Joined => vec![
            ("b browse", KeyCode::Char('b')),
            ("A apply", KeyCode::Char('A')),
            ("j join", KeyCode::Char('j')),
            ("s sync", KeyCode::Char('s')),
            ("m mirror", KeyCode::Char('m')),
            ("l leave", KeyCode::Char('l')),
        ],
        Tab::Site => vec![("S start/stop", KeyCode::Char('S')), ("e ACME", KeyCode::Char('e'))],
    });
    keys.push(("Esc", KeyCode::Esc));
    v.keys = keys;
    draw_view(f, r, v, &t, hits);
}
