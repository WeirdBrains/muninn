//! Orgs ▸ … (G): my orgs, their members, roles and resources, the orgs I joined,
//! the site (with the sites other nodes announced) and the site templates. Thin over [`crate::org`]; anything that talks to other nodes runs on a
//! worker thread so the window stays usable.
use super::code::{Editor, byte_at};
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, record, row_style};
use super::{App, Hit};
use crate::net::{Node, client, identity, local};
use crate::org::host::{self, Acme, Host, HostOptions};
use crate::org::net as onet;
use crate::org::{self, Actor, Leftovers};
use crate::store::{
    Access, Joined, Member, MemberStatus, Mirror, Org, OrgChange, Published, Resource, ResourceState, Role, ShareKind, Store, Template,
    TemplateAsset, Visibility,
};
use libp2p::identity::Keypair;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const ORIGIN: &str = "tui";
const OWNER: Actor = Actor::Owner;

/// The window's pages (Tab / ← → / 1-7 switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Orgs,
    Members,
    Roles,
    Resources,
    Joined,
    Site,
    Templates,
}

impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Orgs, Tab::Members, Tab::Roles, Tab::Resources, Tab::Joined, Tab::Site, Tab::Templates];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Orgs => "Organizations",
            Tab::Members => "Members",
            Tab::Roles => "Roles",
            Tab::Resources => "Resources",
            Tab::Joined => "Joined",
            Tab::Site => "Site",
            Tab::Templates => "Templates",
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
    /// An org's domains (`add D`, `remove D`, or just `D`).
    Domain,
    /// Your certificate for an org (`CERT.pem KEY.pem`, or `clear`).
    Cert,
    /// Let's Encrypt for an org (`on [email]`, `off`).
    OrgAcme,
    /// 2.17: a new template's name.
    NewTemplate,
    /// A file for the selected template (`PATH [NAME]`).
    AddAsset,
    /// Remove one of the selected template's files (its name).
    RemoveAsset,
    /// The selected template's build step (`FOLDER | COMMAND [| OUTPUT]`, or `clear`).
    BuildStep,
}

/// Something that needs `y` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    RemoveOrg(String),
    RemoveRole(String),
    Release(String),
    Leave(String),
    /// 2.17: a template.
    RemoveTemplate(String),
    /// Run a template's new or changed build command.
    Build(String),
}

/// A template being edited in the window (HTML or CSS).
pub struct TemplateEdit {
    pub template: String,
    pub name: String,
    pub css: bool,
    pub ed: Editor,
    pub saved: String,
    /// Esc once more leaves without saving.
    pub leaving: bool,
}

impl TemplateEdit {
    pub fn dirty(&self) -> bool {
        self.ed.text().trim_end() != self.saved.trim_end()
    }
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
    /// 2.17: the current org's templates (and the selected one's files), and the sites
    /// other nodes announced.
    pub templates: Vec<Template>,
    pub assets: Vec<TemplateAsset>,
    pub announced: Vec<(crate::store::SiteAnnouncement, org::announce::SiteCard)>,
    pub tedit: Option<TemplateEdit>,
    /// The site this TUI serves.
    pub host: Option<Host>,
    last_status: Option<Instant>,
}

impl OrgsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some() || self.tedit.is_some()
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
    st.templates = s.templates(&id).unwrap_or_default();
    st.announced = org::announce::received(s).unwrap_or_default();
    let n = rows(app);
    app.orgs.sel = app.orgs.sel.min(n.saturating_sub(1));
    let st = &mut app.orgs;
    st.assets = match (st.tab, st.templates.get(st.sel)) {
        (Tab::Templates, Some(t)) => app.store.template_assets(&t.id).unwrap_or_default(),
        _ => vec![],
    };
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
        Tab::Templates => st.templates.len(),
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

/// Run local work (a template build) on a worker thread with its own store connection.
fn spawn_local(app: &mut App, what: &str, f: impl FnOnce(&Store) -> crate::Result<Vec<String>> + Send + 'static) {
    if app.orgs.job.is_some() {
        return err(app, "still working on the last request");
    }
    let (db, dek) = (app.paths.db.clone(), app.dek.clone());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = Store::open(&db, Some(&dek.0)).and_then(|s| f(&s));
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    app.orgs.job = Some(Job { what: what.to_string(), rx });
    ok(app, format!("{what}… (the window stays usable)"));
}

/// Tell the org's nodes, linked nodes and (public orgs) connections that the hosted sites
/// opened or closed (2.17), on a worker thread.
fn announce_sites(app: &mut App, addr: String, open: bool) {
    if app.orgs.job.is_some() {
        return;
    }
    let (hosting, external, relays) = (app.prefs.hosting.clone(), app.prefs.net.external_addresses.clone(), app.prefs.net.relays.clone());
    if !org::announce::worth_it(&app.store, &hosting, &external, &addr, open).unwrap_or(false) {
        app.orgs.detail = vec!["not announced: no hosted org is reachable from elsewhere with anyone to tell (see Help ▸ org hosting)".into()];
        return;
    }
    spawn(app, if open { "announcing the site" } else { "announcing the site closed" }, move |s, n, _| {
        let mut l = vec![];
        for (o, r) in org::announce::announce_hosted(s, n, &hosting, &external, &addr, open, &relays)? {
            l.push(match (&r.skipped, r.sent.len()) {
                (Some(why), _) => format!("{} not announced: {why}", o.name),
                (None, n) => format!(
                    "{} announced {} to {n} node(s){}",
                    o.name,
                    if open { "open" } else { "closed" },
                    if r.failed.is_empty() { String::new() } else { format!(", {} unreachable", r.failed.len()) }
                ),
            });
        }
        if l.is_empty() {
            l.push("no hosted org to announce".into());
        }
        Ok(l)
    });
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

/// Serve the hosted orgs (Orgs ▸ Start site).
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
            announce_sites(app, st.addr, true);
        }
        Err(e) => err(app, format!("site: {e}")),
    }
}

/// Stop the site this TUI serves, or ask the one another process serves to stop.
pub fn stop_host(app: &mut App) {
    if let Some(h) = app.orgs.host.take() {
        let addr = h.status().addr;
        h.stop();
        host::clear(&app.paths.home);
        record(app, "org.host.stop", "site stopped".into(), serde_json::json!({}));
        app.set_status("site stopped");
        ok(app, "site stopped");
        announce_sites(app, addr, false);
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

fn sel_template(app: &App) -> Option<(Org, Template)> {
    Some((app.orgs.current()?.clone(), app.orgs.templates.get(app.orgs.sel)?.clone()))
}

/// Open the in-window editor on a template's HTML or CSS.
fn edit_template(app: &mut App, t: &Template, css: bool) {
    let text = if css { &t.css } else { &t.html };
    app.orgs.tedit =
        Some(TemplateEdit { template: t.id.clone(), name: t.name.clone(), css, ed: Editor::new(text), saved: text.clone(), leaving: false });
    ok(app, format!("editing {} of {}: Ctrl-S saves, Ctrl-P previews, Esc leaves", if css { "the CSS" } else { "the HTML" }, t.name));
}

fn save_edit(app: &mut App) {
    let Some(o) = app.orgs.current().cloned() else { return };
    let Some(te) = app.orgs.tedit.as_ref() else { return };
    let Ok(Some(t)) = app.store.template(&te.template) else { return err(app, "that template is gone") };
    let text = te.ed.text();
    let (html, css) = if te.css { (t.html.as_str(), text.as_str()) } else { (text.as_str(), t.css.as_str()) };
    match org::save_template(&app.store, ORIGIN, &OWNER, &o, &t.name, html, css) {
        Ok(t) => {
            if let Some(te) = app.orgs.tedit.as_mut() {
                te.saved = text.clone();
                te.leaving = false;
            }
            app.refresh_history(true);
            app.orgs.templates = app.store.templates(&o.id).unwrap_or_default();
            ok(app, format!("saved {} (a preview shows it on its next load)", t.name));
        }
        Err(e) => err(app, e.to_string()),
    }
}

/// A preview link for a template: through the site if it runs, else the path to open once it does.
fn preview(app: &mut App, o: &Org, t: &Template) {
    match org::preview(&app.store, ORIGIN, &OWNER, o, t) {
        Ok(token) => {
            let path = org::serve::preview_path(&o.slug, &token);
            let served = app.orgs.host.as_ref().map(|h| h.status().url).or_else(|| host::running(&app.paths.home).map(|f| f.url));
            let line = match served {
                Some(u) => format!("preview of {} (an hour, live): {u}{}", t.name, path.trim_start_matches('/')),
                None => format!("preview of {} at {path} once the site runs (S on Site starts it)", t.name),
            };
            app.refresh_history(true);
            app.orgs.detail = vec![line.clone()];
            ok(app, line);
        }
        Err(e) => err(app, e.to_string()),
    }
}

fn run_build(app: &mut App, o: Org, t: Template, approve: bool) {
    spawn_local(app, "building", move |s| {
        let b = org::build::build(s, ORIGIN, &OWNER, &o, &t, approve)?;
        let mut l = vec![format!("built {}: {} file(s) besides index.html", t.name, b.assets.len())];
        l.extend(b.notes);
        l.extend(b.log.lines().map(str::to_string));
        Ok(l)
    });
}

fn template_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Char('s') if ctrl => save_edit(app),
        KeyCode::Char('p') if ctrl => {
            let Some(o) = app.orgs.current().cloned() else { return };
            let Some(id) = app.orgs.tedit.as_ref().map(|t| t.template.clone()) else { return };
            if let Ok(Some(t)) = app.store.template(&id) {
                preview(app, &o, &t);
            }
        }
        KeyCode::Esc => {
            let Some(te) = app.orgs.tedit.as_mut() else { return };
            if te.dirty() && !te.leaving {
                te.leaving = true;
                return ok(app, "not saved: Ctrl-S saves, Esc again leaves without saving");
            }
            app.orgs.tedit = None;
            reload(app);
            ok(app, "left the editor");
        }
        _ => {
            if let Some(te) = app.orgs.tedit.as_mut() {
                te.leaving = false;
                te.ed.key(k, 10);
            }
        }
    }
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
        OAsk::Domain | OAsk::Cert | OAsk::OrgAcme => {
            let Some(o) = sel_org(app) else { return };
            if v.trim().is_empty() {
                return;
            }
            let r = org_tls(app, kind, &o, v.trim());
            done(app, r, |m| m.clone());
        }
        OAsk::NewTemplate => {
            let Some(o) = app.orgs.current().cloned() else { return };
            if v.trim().is_empty() {
                return;
            }
            match org::save_template(&app.store, ORIGIN, &OWNER, &o, v.trim(), org::serve::DEFAULT_HTML, org::serve::DEFAULT_CSS) {
                Ok(t) => {
                    reload(app);
                    app.orgs.sel = app.orgs.templates.iter().position(|x| x.id == t.id).unwrap_or(0);
                    app.refresh_history(true);
                    edit_template(app, &t, false);
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        OAsk::AddAsset => {
            let Some((o, t)) = sel_template(app) else { return };
            let w = words(&v);
            let Some(p) = w.first() else { return };
            let path = path_of(p);
            let name = w.get(1).cloned().unwrap_or_else(|| path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
            let r = std::fs::read(&path)
                .map_err(|e| crate::Error::invalid("asset", format!("{p}: {e}")))
                .and_then(|d| org::put_asset(&app.store, ORIGIN, &OWNER, &o, &t, &name, &d));
            done(app, r, |at| format!("{name} added to {} at {at}", t.name));
        }
        OAsk::RemoveAsset => {
            let Some((o, t)) = sel_template(app) else { return };
            if v.trim().is_empty() {
                return;
            }
            let r = org::remove_asset(&app.store, ORIGIN, &OWNER, &o, &t, v.trim());
            done(app, r, |_| format!("{} removed from {}", v.trim(), t.name));
        }
        OAsk::BuildStep => {
            let Some((o, t)) = sel_template(app) else { return };
            let v = v.trim();
            if v.is_empty() {
                return;
            }
            let r = if v.eq_ignore_ascii_case("clear") {
                org::build::configure(&app.store, ORIGIN, &OWNER, &o, &t, None)
            } else {
                let parts: Vec<&str> = v.split('|').map(str::trim).collect();
                if parts.len() < 2 {
                    return err(app, "give the folder and the command: ~/site | npm run build [| dist]");
                }
                let src = path_of(parts[0]);
                let src = std::fs::canonicalize(&src).unwrap_or(src).to_string_lossy().into_owned();
                let out = parts.get(2).copied().filter(|x| !x.is_empty());
                org::build::configure(&app.store, ORIGIN, &OWNER, &o, &t, Some(org::build::Step { cmd: parts[1], src: &src, out }))
            };
            done(app, r, |t| match &t.build_cmd {
                Some(c) => format!("{}: builds with `{c}` (B runs it)", t.name),
                None => format!("{}: no build step", t.name),
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
        Confirm::RemoveTemplate(id) => {
            let Some((o, t)) = sel_template(app).filter(|(_, t)| t.id == id) else { return };
            let r = org::remove_template(&app.store, ORIGIN, &OWNER, &o, &t);
            done(app, r, |_| format!("removed template {}", t.name));
        }
        Confirm::Build(id) => {
            let Some((o, t)) = sel_template(app).filter(|(_, t)| t.id == id) else { return };
            run_build(app, o, t, true);
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
    if app.orgs.tedit.is_some() && app.orgs.ask.is_none() {
        return template_key(app, k);
    }
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
        KeyCode::Char(c @ '1'..='7') => return switch(app, c as usize - '1' as usize),
        KeyCode::Down | KeyCode::Char('j') if app.orgs.tab != Tab::Joined || k.code == KeyCode::Down => {
            app.orgs.sel = (app.orgs.sel + 1).min(n.saturating_sub(1));
            if app.orgs.tab == Tab::Templates {
                reload(app);
            }
            return;
        }
        KeyCode::Up | KeyCode::Char('k') if app.orgs.tab != Tab::Members || k.code == KeyCode::Up => {
            app.orgs.sel = app.orgs.sel.saturating_sub(1);
            if app.orgs.tab == Tab::Templates {
                reload(app);
            }
            return;
        }
        KeyCode::Char('g') => {
            reload(app);
            return ok(app, "refreshed");
        }
        _ => {}
    }
    let ask = |app: &mut App, kind: OAsk, title: &str, v: &str| app.orgs.ask = Some(Ask::new(kind, title, v));
    let need_org = matches!(app.orgs.tab, Tab::Members | Tab::Roles | Tab::Resources | Tab::Templates) && app.orgs.current().is_none();
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
        (Tab::Orgs, KeyCode::Char('w')) => {
            if let Some(o) = sel_org(app) {
                let have = crate::org::tls::domains(&app.store, &o).unwrap_or_default();
                let title = if have.is_empty() {
                    format!("Domain for {} (lab.example.org; remove D takes one off)", o.name)
                } else {
                    format!("Domains of {}: {} · add D / remove D", o.name, have.join(", "))
                };
                ask(app, OAsk::Domain, &title, "")
            }
        }
        (Tab::Orgs, KeyCode::Char('c')) => {
            if let Some(o) = sel_org(app) {
                ask(app, OAsk::Cert, &format!("Your certificate for {}: CERT.pem KEY.pem (replaces one set before), or clear", o.name), "")
            }
        }
        (Tab::Orgs, KeyCode::Char('L')) => {
            if let Some(o) = sel_org(app) {
                let a = crate::org::tls::acme(&app.store, &o).unwrap_or_default();
                let v = if a.enabled { format!("on {}", a.email) } else { String::new() };
                ask(app, OAsk::OrgAcme, &format!("Let's Encrypt for {}: on [email], or off", o.name), v.trim())
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
        (Tab::Site, KeyCode::Char('A')) => match (&app.orgs.host, host::running(&app.paths.home)) {
            (Some(h), _) => {
                let addr = h.status().addr;
                announce_sites(app, addr, true)
            }
            (None, Some(f)) => announce_sites(app, f.addr, true),
            (None, None) => ok(app, "the site is not running: S starts it (and announces it)"),
        },
        (Tab::Templates, KeyCode::Char('n')) => ask(app, OAsk::NewTemplate, "New template name (starts from the built-in look)", ""),
        (Tab::Templates, KeyCode::Char('e') | KeyCode::Enter | KeyCode::Char('c')) => {
            if let Some((_, t)) = sel_template(app) {
                edit_template(app, &t, k.code == KeyCode::Char('c'))
            }
        }
        (Tab::Templates, KeyCode::Char('a')) => {
            if sel_template(app).is_some() {
                ask(app, OAsk::AddAsset, "Add a file: path [name under assets/]", "")
            }
        }
        (Tab::Templates, KeyCode::Char('x')) => {
            if sel_template(app).is_some() {
                ask(app, OAsk::RemoveAsset, "Remove the file named", "")
            }
        }
        (Tab::Templates, KeyCode::Char('b')) => {
            if let Some((_, t)) = sel_template(app) {
                let v = match &t.build_cmd {
                    Some(c) => format!(
                        "{} | {c}{}",
                        t.build_src.as_deref().unwrap_or(""),
                        t.build_out.as_deref().map(|o| format!(" | {o}")).unwrap_or_default()
                    ),
                    None => String::new(),
                };
                ask(app, OAsk::BuildStep, "Build step: folder | command [| output folder, default build], or clear", &v)
            }
        }
        (Tab::Templates, KeyCode::Char('B')) => {
            let Some((o, t)) = sel_template(app) else { return };
            if t.build_cmd.is_none() {
                return ok(app, format!("{} has no build step: b sets one", t.name));
            }
            match org::build::approved(&app.store, &t) {
                Ok(true) => run_build(app, o, t, false),
                Ok(false) => {
                    let q = org::build::question(&t);
                    app.orgs.confirm = Some(Confirm::Build(t.id.clone()));
                    ok(app, format!("{q} y/n"));
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        (Tab::Templates, KeyCode::Char('p')) => {
            if let Some((o, t)) = sel_template(app) {
                preview(app, &o, &t)
            }
        }
        (Tab::Templates, KeyCode::Char('u')) => {
            let Some((o, t)) = sel_template(app) else { return };
            let using = o.template_id.as_deref() == Some(t.id.as_str());
            let r = org::edit(&app.store, ORIGIN, &OWNER, &o, &OrgChange { template_id: Some((!using).then(|| t.id.clone())), ..Default::default() });
            done(app, r, |o| if using { format!("{} uses the built-in look", o.name) } else { format!("{} uses {}", o.name, t.name) });
        }
        (Tab::Templates, KeyCode::Char('d')) => {
            if let Some((_, t)) = sel_template(app) {
                app.orgs.confirm = Some(Confirm::RemoveTemplate(t.id.clone()));
                ok(app, format!("remove template {} and its files? y/n", t.name));
            }
        }
        (Tab::Site, KeyCode::Char('e')) => {
            let a = &app.prefs.hosting.acme;
            let v = if a.enabled { format!("{} {}", a.domain, a.email) } else { String::new() };
            ask(app, OAsk::Acme, "ACME (Let's Encrypt): domain email, or off", &v)
        }
        _ => {}
    }
}

fn path_of(p: &str) -> std::path::PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join(rest),
        None => p.into(),
    }
}

/// The org TLS settings from the Organizations page: domains, your certificate, ACME.
fn org_tls(app: &mut App, kind: OAsk, o: &Org, v: &str) -> crate::Result<String> {
    use crate::org::tls;
    let s = &app.store;
    let note = |kind: &str, summary: String, p: serde_json::Value| s.note_action(&crate::store::NewAction::new(kind, ORIGIN, summary).params(p));
    let msg = match kind {
        OAsk::Domain => {
            let (verb, d) = match v.split_once(' ') {
                Some((w, d)) if matches!(w, "add" | "remove" | "rm") => (w, d.trim()),
                _ => ("add", v),
            };
            if verb == "add" {
                let d = tls::check_domain(d)?;
                if !tls::add_domain(s, o, &d)? {
                    return Ok(format!("{} is served at {d} already", o.name));
                }
                note("org.domain.add", format!("{} served at {d}", o.name), serde_json::json!({ "org": o.id, "domain": d }));
                format!("{} is served at {d}; point its DNS here (c sets your certificate, L Let's Encrypt)", o.name)
            } else {
                if !tls::remove_domain(s, o, d)? {
                    return Err(crate::Error::invalid("domain", format!("{} is not served at {d}", o.name)));
                }
                note("org.domain.remove", format!("{} no longer served at {d}", o.name), serde_json::json!({ "org": o.id, "domain": d }));
                format!("{} is no longer served at {d}", o.name)
            }
        }
        OAsk::Cert if v.eq_ignore_ascii_case("clear") => {
            if !tls::clear_user_cert(s, o)? {
                return Err(crate::Error::invalid("certificate", format!("{} has no certificate of yours", o.name)));
            }
            note("org.cert.clear", format!("your certificate removed from {}", o.name), serde_json::json!({ "org": o.id }));
            format!("{}: back to {}", o.name, if tls::acme(s, o)?.enabled { "ACME" } else { "a self-signed certificate" })
        }
        OAsk::Cert => {
            let w = words(v);
            let [c, k] = w.as_slice() else { return Err(crate::Error::invalid("certificate", "give the certificate file and the key file")) };
            let read = |p: &str| std::fs::read_to_string(path_of(p)).map_err(|e| crate::Error::invalid("certificate", format!("{p}: {e}")));
            let pem = crate::org::host::CertPem { chain: read(c)?, key: read(k)? };
            let info = tls::set_user_cert(s, o, &pem)?;
            note(
                "org.cert",
                format!("your certificate set for {}", o.name),
                serde_json::json!({ "org": o.id, "source": tls::USER, "names": info.names, "not_after": info.not_after }),
            );
            format!("{}: your certificate for {} until {}", o.name, info.names.join(", "), crate::ingest::time::fmt_ts(info.not_after))
        }
        _ => {
            let w = words(v);
            let mut a = tls::acme(s, o)?;
            match w.first().map(|x| x.to_ascii_lowercase()).as_deref() {
                Some("off") => a.enabled = false,
                Some("on") => {
                    a.enabled = true;
                    if let Some(e) = w.get(1) {
                        a.email = e.clone();
                    }
                }
                _ => return Err(crate::Error::invalid("acme", "on [email] or off")),
            }
            if a.enabled && tls::domains(s, o)?.is_empty() {
                return Err(crate::Error::invalid("acme", format!("{} has no domains yet (w adds one)", o.name)));
            }
            if a.enabled && !a.email.contains('@') && !app.prefs.hosting.acme.email.contains('@') {
                return Err(crate::Error::invalid("acme", "give a contact address: on you@example.org"));
            }
            tls::set_acme(s, o, &a)?;
            let m = format!("ACME {} for {}", if a.enabled { "on" } else { "off" }, o.name);
            note("org.cert.acme", m.clone(), serde_json::json!({ "org": o.id, "enabled": a.enabled }));
            m
        }
    };
    if let Some(h) = &app.orgs.host {
        let _ = h.reload_orgs();
    }
    Ok(msg)
}

fn org_cert_line(c: &crate::org::tls::OrgCert) -> String {
    let now = crate::store::now_ms();
    let when = match &c.cert {
        Some(i) => format!("until {} ({} days)", crate::ingest::time::fmt_ts(i.not_after), c.days_left(now).unwrap_or(0)),
        None => "made when the site starts".into(),
    };
    format!(
        "  {}  {} · {} {when}{}",
        c.slug,
        if c.domains.is_empty() { "no domains".to_string() } else { c.domains.join(", ") },
        c.source,
        if c.acme.enabled { " · ACME on" } else { "" }
    )
}

fn switch(app: &mut App, i: usize) {
    open(app, Tab::ALL[i.min(Tab::ALL.len() - 1)]);
}

pub fn on_click(app: &mut App, i: usize) {
    if app.orgs.ask.is_some() {
        return;
    }
    if let Some(te) = app.orgs.tedit.as_mut() {
        te.ed.row = i.min(te.ed.lines.len().saturating_sub(1));
        te.ed.col = te.ed.col.min(te.ed.lines[te.ed.row].chars().count());
        return;
    }
    if app.orgs.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < rows(app) {
        app.orgs.sel = i;
        app.orgs.detail.clear();
        if app.orgs.tab == Tab::Templates {
            reload(app);
        }
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
    let certs = crate::org::tls::all(&app.store).unwrap_or_default();
    if !certs.is_empty() {
        v.push("org domains and certificates (w, c and L on Organizations change them)".into());
        v.extend(certs.iter().map(org_cert_line));
    }
    if hp.acme.enabled {
        v.push(format!("ACME         {} for {} ({}), profile {}", hp.acme.directory_url(), hp.acme.domain, hp.acme.email, hp.acme.profile));
    } else {
        v.push(format!("certificate  self-signed, {} days, made when the site starts (e sets up Let's Encrypt)", hp.cert_days));
    }
    let open: Vec<_> = app.orgs.announced.iter().filter(|(a, _)| a.open).collect();
    v.push(String::new());
    v.push(format!("announced to this node  {} open site(s) (mnnn org sites; A announces yours)", open.len()));
    for (a, c) in open {
        let peer = &a.peer[a.peer.len().saturating_sub(8)..];
        v.push(format!("  {} ({}) from …{peer}  {}", c.name, c.visibility.as_str(), c.urls.first().map(String::as_str).unwrap_or("-")));
        if !c.serves.is_empty() {
            v.push(format!("    serves {}", c.serves.join(", ")));
        }
    }
    v
}

fn cert_line(c: &host::CertInfo) -> String {
    format!("certificate  {} for {} until {}", c.source, c.names.join(", "), crate::ingest::time::fmt_ts(c.not_after))
}

/// The template editor in place of the Templates list.
fn draw_edit(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let Some(te) = app.orgs.tedit.as_mut() else { return };
    let mut v = View::new(format!(
        "Organizations · Templates · {} · {}{}",
        te.name,
        if te.css { "CSS" } else { "HTML" },
        if te.dirty() { " · modified" } else { "" }
    ));
    v.head.push(muted(&t, " placeholders: {{title}} {{org.name}} {{org.description}} {{org.slug}} {{base}} {{assets}} {{css}} {{content}}"));
    v.head.push(Line::raw(""));
    let gutter = format!("{}", te.ed.lines.len()).len().max(2);
    for (i, l) in te.ed.lines.iter().enumerate() {
        let mut spans = vec![Span::styled(format!("{:>gutter$} ", i + 1), Style::new().fg(t.muted))];
        if i == te.ed.row {
            let at = byte_at(l, te.ed.col);
            let (a, b) = l.split_at(at);
            let mut rest = b.chars();
            let cur = rest.next().map(String::from).unwrap_or_else(|| " ".into());
            spans.push(Span::styled(a.to_string(), Style::new().fg(t.fg)));
            spans.push(Span::styled(cur, Style::new().bg(t.selection_bg).fg(t.selection_fg)));
            spans.push(Span::styled(rest.collect::<String>(), Style::new().fg(t.fg)));
        } else {
            spans.push(Span::styled(l.clone(), Style::new().fg(t.fg)));
        }
        v.rows.push(Line::from(spans));
    }
    v.sel = Some(te.ed.row);
    v.ask = app.orgs.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = app.orgs.msg.clone();
    v.keys = vec![("^S save", KeyCode::Char('s')), ("^P preview", KeyCode::Char('p')), ("Esc leave", KeyCode::Esc)];
    draw_view(f, r, v, &t, hits);
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    if app.orgs.tedit.is_some() {
        return draw_edit(app, f, r, hits);
    }
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
    if matches!(st.tab, Tab::Members | Tab::Roles | Tab::Resources | Tab::Templates) {
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
        Tab::Templates => {
            if st.templates.is_empty() {
                v.head.push(muted(&t, " no templates yet: n makes one from the built-in look (the site uses that look until u picks one)"));
            }
            let in_use = st.current().and_then(|o| o.template_id.clone());
            if let Some(tp) = st.templates.get(st.sel) {
                let files =
                    if st.assets.is_empty() { "-".to_string() } else { st.assets.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ") };
                v.head.push(muted(&t, format!(" files: {files}")));
                if let Some(c) = &tp.build_cmd {
                    let ok = org::build::approved(&app.store, tp).unwrap_or(false);
                    v.head.push(muted(
                        &t,
                        format!(
                            " build: `{c}` in a copy of {} → {}/ ({})",
                            tp.build_src.as_deref().unwrap_or("?"),
                            tp.build_out.as_deref().unwrap_or(org::build::DEFAULT_OUT),
                            if ok { "approved here" } else { "asks before running" }
                        ),
                    ));
                }
                v.head.push(Line::raw(""));
            }
            for (i, tp) in st.templates.iter().enumerate() {
                let used = if in_use.as_deref() == Some(tp.id.as_str()) { " (in use)" } else { "" };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<24}{used:<9}", tp.name), row_style(&t, i == st.sel)),
                    Span::styled(
                        format!(
                            " html {} B · css {} B{} · updated {}",
                            tp.html.len(),
                            tp.css.len(),
                            if tp.build_cmd.is_some() { " · build step" } else { "" },
                            crate::ingest::time::fmt_ts(tp.updated_at)
                        ),
                        mt,
                    ),
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
            ("w domains", KeyCode::Char('w')),
            ("c cert", KeyCode::Char('c')),
            ("L ACME", KeyCode::Char('L')),
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
        Tab::Site => vec![("S start/stop", KeyCode::Char('S')), ("A announce", KeyCode::Char('A')), ("e ACME", KeyCode::Char('e'))],
        Tab::Templates => vec![
            ("n new", KeyCode::Char('n')),
            ("e html", KeyCode::Char('e')),
            ("c css", KeyCode::Char('c')),
            ("a file", KeyCode::Char('a')),
            ("x rm file", KeyCode::Char('x')),
            ("b build step", KeyCode::Char('b')),
            ("B build", KeyCode::Char('B')),
            ("p preview", KeyCode::Char('p')),
            ("u use", KeyCode::Char('u')),
            ("d delete", KeyCode::Char('d')),
        ],
    });
    keys.push(("Esc", KeyCode::Esc));
    v.keys = keys;
    draw_view(f, r, v, &t, hits);
}
