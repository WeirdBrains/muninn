//! Reviews ▸ … (V): reviews of my things and their threads, and the reviews I joined on
//! other nodes. Thin over [`crate::review`]; anything that talks to other nodes runs on a
//! worker thread so the window stays usable.
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, row_style};
use super::{App, Hit};
use crate::net::{Node, client, identity, local};
use crate::review::net as rnet;
use crate::review::{self, Who};
use crate::store::{Review, ReviewItem, ReviewItemKind, ReviewItemState, ReviewKind, ReviewStatus, Reviewer, Reviewing, ReviewingStatus, Store};
use libp2p::identity::Keypair;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::sync::{Arc, mpsc};

const ORIGIN: &str = "tui";

/// The window's pages (Tab / ← → / 1-3 switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Reviews,
    Thread,
    Joined,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Reviews, Tab::Thread, Tab::Joined];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Reviews => "My reviews",
            Tab::Thread => "Thread",
            Tab::Joined => "Reviewing",
        }
    }
    fn index(self) -> usize {
        Tab::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// What a prompt is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsk {
    Request,
    Invite,
    Join,
    /// Add to a review of mine (Thread page) or a joined one (Reviewing page).
    Item(ReviewItemKind),
    Reply,
}

/// Something that needs `y` first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Remove(String),
    Leave(String),
}

pub struct Job {
    pub what: String,
    rx: mpsc::Receiver<Result<Vec<String>, String>>,
}

#[derive(Default)]
pub struct ReviewsState {
    pub tab: Tab,
    pub sel: usize,
    /// The review the Thread page shows.
    pub review: Option<String>,
    pub ask: Option<Ask<RAsk>>,
    pub confirm: Option<Confirm>,
    pub msg: Option<(String, bool)>,
    pub detail: Vec<String>,
    pub job: Option<Job>,
    pub reviews: Vec<Review>,
    pub reviewers: Vec<Reviewer>,
    pub items: Vec<ReviewItem>,
    pub joined: Vec<Reviewing>,
}

impl ReviewsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    pub fn current(&self) -> Option<&Review> {
        let id = self.review.as_deref()?;
        self.reviews.iter().find(|r| r.id == id)
    }
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.reviews.msg = Some((m.into(), false));
}

fn err(app: &mut App, m: impl Into<String>) {
    app.reviews.msg = Some((m.into(), true));
}

pub fn open(app: &mut App, tab: Tab) {
    let st = &mut app.reviews;
    st.ask = None;
    st.confirm = None;
    st.msg = None;
    st.detail.clear();
    if st.tab != tab {
        st.sel = 0;
    }
    st.tab = tab;
    reload(app);
    app.modal = Some(super::modal::Modal::Reviews);
}

/// Open on My reviews with the request prompt.
pub fn open_new(app: &mut App) {
    open(app, Tab::Reviews);
    ask(app, RAsk::Request, "Review what? (entry|journal|project|search|record NAME; records MODEL/RECORD)", "journal ");
}

fn ask(app: &mut App, kind: RAsk, title: &str, v: &str) {
    app.reviews.ask = Some(Ask::new(kind, title, v));
}

fn reload(app: &mut App) {
    let s = &app.store;
    let st = &mut app.reviews;
    st.reviews = s.reviews().unwrap_or_default();
    if st.current().is_none() {
        st.review = st.reviews.first().map(|r| r.id.clone());
    }
    let id = st.review.clone().unwrap_or_default();
    st.reviewers = s.reviewers(&id).unwrap_or_default();
    st.items = s.review_items(&id).unwrap_or_default();
    st.joined = s.reviewing().unwrap_or_default();
    let n = rows(app);
    app.reviews.sel = app.reviews.sel.min(n.saturating_sub(1));
}

fn rows(app: &App) -> usize {
    let st = &app.reviews;
    match st.tab {
        Tab::Reviews => st.reviews.len(),
        Tab::Thread => st.items.len(),
        Tab::Joined => st.joined.len(),
    }
}

fn label(app: &App) -> String {
    local::my_label(&app.store, &app.prefs.net)
}

/// Run work that talks to other nodes on a worker thread (with the TUI's node if it runs).
fn spawn(app: &mut App, what: &str, f: impl FnOnce(&Store, &Node, &Keypair) -> crate::Result<Vec<String>> + Send + 'static) {
    if app.reviews.job.is_some() {
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
    app.reviews.job = Some(Job { what: what.to_string(), rx });
    ok(app, format!("{what}… (the window stays usable)"));
}

/// Between frames: take finished work.
pub fn poll(app: &mut App) {
    let Some(job) = app.reviews.job.as_ref() else { return };
    let r = match job.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Err("the task stopped".into()),
    };
    let what = app.reviews.job.take().map(|j| j.what).unwrap_or_default();
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
            app.reviews.detail = lines.into_iter().skip(1).collect();
        }
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

/// A prompt's text as a thread item: `field=F entry=E nth=N` first (any of them), then
/// `@quoted text :: the rest` to anchor on text, or just the rest.
pub fn parse_item(kind: ReviewItemKind, v: &str) -> review::Ask {
    let mut a = review::Ask { kind, nth: 1, ..Default::default() };
    let mut rest = v.trim();
    loop {
        let (w, more) = rest.split_once(' ').unwrap_or((rest, ""));
        match w.split_once('=') {
            Some(("field", f)) => a.field = f.to_string(),
            Some(("entry", e)) => a.target = e.to_string(),
            Some(("nth", n)) if n.parse::<usize>().is_ok() => a.nth = n.parse().unwrap_or(1),
            _ => break,
        }
        rest = more.trim_start();
    }
    // a highlight is always of some text: the leading @ is optional
    let at = rest.strip_prefix('@').or((kind == ReviewItemKind::Highlight && !rest.is_empty()).then_some(rest));
    let rest = if let Some(r) = at {
        let (at, body) = r.split_once(" :: ").unwrap_or((r, ""));
        a.at = Some(at.to_string());
        body.trim().to_string()
    } else {
        rest.to_string()
    };
    match kind {
        ReviewItemKind::Comment => a.body = rest,
        ReviewItemKind::Suggestion => a.value = Some(serde_json::Value::String(rest)),
        ReviewItemKind::Highlight => a.body = rest,
        ReviewItemKind::Vote => {
            let mut w = rest.split_whitespace();
            a.value = w.next().map(|v| serde_json::Value::String(v.to_string()));
            a.reply_to = w.next().map(str::to_string);
        }
    }
    a
}

fn hint(kind: ReviewItemKind) -> &'static str {
    match kind {
        ReviewItemKind::Comment => "Comment (`@text :: comment` anchors it; field=… entry=… first)",
        ReviewItemKind::Suggestion => "Suggest (`@old text :: new text`, or the whole field's new value; field=… entry=…)",
        ReviewItemKind::Highlight => "Highlight (`text :: note`; field=… entry=…)",
        ReviewItemKind::Vote => "Vote: approve, changes or reject (or `up ITEM` / `down ITEM`)",
    }
}

fn kind_of(k: &str) -> Option<ReviewKind> {
    ReviewKind::parse(match k {
        "live-search" | "searches" => "search",
        k => k,
    })
}

fn finish_ask(app: &mut App, kind: RAsk, v: String) {
    let relays = app.prefs.net.relays.clone();
    match kind {
        RAsk::Request => {
            let (k, name) = v.trim().split_once(' ').map(|(a, b)| (a.trim(), b.trim())).unwrap_or((v.trim(), ""));
            let Some(k) = kind_of(&k.to_ascii_lowercase()) else {
                return err(app, format!("what kind is {k:?}? entry, journal, project, search or record"));
            };
            let r = review::request(&app.store, ORIGIN, k, name, "", "", &[]);
            if let Ok(r) = &r {
                app.reviews.review = Some(r.id.clone());
            }
            done(app, r, |r| format!("review of {} {}: i makes a link for a reviewer", r.kind.label(), r.name));
            app.reviews.sel = app.reviews.reviews.iter().position(|r| Some(&r.id) == app.reviews.review.as_ref()).unwrap_or(0);
        }
        RAsk::Invite => {
            let Some(r) = sel_review(app) else { return };
            let my = label(app);
            spawn(app, "link", move |s, n, kp| {
                let to = (!v.is_empty()).then_some(v.as_str());
                let (_, c) = rnet::invite(s, to.map(|_| n), kp, &r, "", to, &relays, ORIGIN)?;
                let mut l = vec![format!("a link to {} (one reviewer, a week)", r.title())];
                if let Some(d) = client::describe_delivery(&c.delivered) {
                    l.push(d);
                }
                l.push(format!("Review link from {my} (they join with `mnnn review join TOKEN` or j on Reviewing):"));
                l.push(c.token);
                Ok(l)
            });
        }
        RAsk::Join => {
            if v.is_empty() {
                return;
            }
            spawn(app, "join", move |s, n, kp| {
                let (g, a) = rnet::join(s, Some(n), kp, v.trim(), "", ORIGIN)?;
                let mut l = vec![format!("reviewing {} ({} {}); you may {}", g.title(), g.kind, g.name, g.allow.join(", "))];
                if let Some(t) = a.answer
                    && !a.delivered.as_ref().is_some_and(|d| d.is_ok())
                {
                    l.push("Their node was not reachable; give them this answer token:".into());
                    l.push(t);
                }
                Ok(l)
            });
        }
        RAsk::Item(k) if k != ReviewItemKind::Vote && v.trim().is_empty() => {}
        RAsk::Reply if v.trim().is_empty() => {}
        RAsk::Item(k) => add(app, parse_item(k, &v)),
        RAsk::Reply => {
            let Some(it) = app.reviews.items.get(app.reviews.sel).cloned() else { return };
            let mut a = parse_item(ReviewItemKind::Comment, &v);
            a.reply_to = Some(it.id);
            add(app, a)
        }
    }
}

/// Add an item to the review in view: mine (Thread page) or a joined one (Reviewing page).
fn add(app: &mut App, a: review::Ask) {
    if app.reviews.tab == Tab::Joined {
        let Some(g) = app.reviews.joined.get(app.reviews.sel).cloned() else { return };
        let relays = app.prefs.net.relays.clone();
        return spawn(app, a.kind.as_str(), move |s, n, _| {
            Ok(vec![match rnet::send_or_queue(s, Some(n), &g, &a, &relays, ORIGIN)? {
                rnet::Sent::Item(it) => format!("sent to {}: {}", g.title(), review::line(&it)),
                rnet::Sent::Queued(_) => format!("kept here; sent when the node of {} is reachable", g.title()),
            }])
        });
    }
    let Some(r) = app.reviews.current().cloned() else { return };
    let res = review::add(&app.store, ORIGIN, &r, &Who::Author, &a);
    // The thread shows the new item; the message only confirms it (not the same line twice).
    done(app, res, |it| format!("added {} {}", it.kind.as_str(), it.short_id()));
    if app.reviews.tab == Tab::Thread {
        app.reviews.sel = app.reviews.items.len().saturating_sub(1);
    }
}

fn sel_review(app: &App) -> Option<Review> {
    match app.reviews.tab {
        Tab::Reviews => app.reviews.reviews.get(app.reviews.sel).cloned(),
        _ => app.reviews.current().cloned(),
    }
}

fn confirmed(app: &mut App, c: Confirm) {
    let relays = app.prefs.net.relays.clone();
    match c {
        Confirm::Remove(id) => {
            let Some(r) = app.reviews.reviews.iter().find(|r| r.id == id).cloned() else { return };
            let res = review::remove(&app.store, ORIGIN, &r);
            done(app, res, |_| format!("removed the review {} (revisions stay)", r.title()));
        }
        Confirm::Leave(id) => {
            let Some(g) = app.reviews.joined.iter().find(|g| g.review_id == id).cloned() else { return };
            spawn(app, "leave", move |s, n, _| {
                let g = rnet::leave(s, Some(n), &g, &relays, ORIGIN)?;
                Ok(vec![format!("left {}", g.title())])
            });
        }
    }
}

fn content_lines(v: &serde_json::Value) -> Vec<String> {
    let text = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    let mut l = vec![];
    if let Some(c) = v["content"].as_object() {
        l.extend(c.iter().map(|(k, v)| format!("{k}: {}", text(v))));
    }
    for e in v["entries"].as_array().into_iter().flatten() {
        let id = e["id"].as_str().unwrap_or_default();
        l.push(format!("entry {} · {}", crate::store::projects::short_of(id), e["title"].as_str().unwrap_or_default()));
        l.extend(e["body"].as_str().unwrap_or_default().lines().map(|x| format!("  {x}")));
    }
    l
}

fn merge(app: &mut App, force: bool) {
    let Some(r) = app.reviews.current().cloned() else { return };
    let res = review::merge(&app.store, ORIGIN, &r, force);
    if let Ok(m) = &res {
        app.reviews.detail = m.conflicts.iter().map(|(id, why)| format!("{} left accepted: {why}", crate::store::projects::short_of(id))).collect();
    }
    done(app, res, |m| match (m.merged.len(), m.conflicts.len()) {
        (0, 0) => "nothing to merge: accept suggestions first (a / A)".to_string(),
        (n, 0) => format!("merged {n} suggestion(s): {}", revs(m)),
        (n, c) => format!("merged {n}, {c} left (F merges them anyway): {}", revs(m)),
    });
}

fn revs(m: &review::Merged) -> String {
    m.revisions.iter().map(|v| format!("{} rev {}", v.kind.label(), v.rev)).collect::<Vec<_>>().join(", ")
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.reviews.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.reviews.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.reviews.ask = None;
                finish_ask(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.reviews.confirm.take() {
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
            if !app.reviews.detail.is_empty() {
                app.reviews.detail.clear();
            } else {
                app.modal = None;
            }
            return;
        }
        KeyCode::Tab | KeyCode::Right => return switch(app, (app.reviews.tab.index() + 1) % Tab::ALL.len()),
        KeyCode::BackTab | KeyCode::Left => return switch(app, (app.reviews.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()),
        KeyCode::Char(c @ '1'..='3') => return switch(app, c as usize - '1' as usize),
        KeyCode::Down | KeyCode::Char('j') if app.reviews.tab != Tab::Joined || k.code == KeyCode::Down => {
            app.reviews.sel = (app.reviews.sel + 1).min(n.saturating_sub(1));
            return;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.reviews.sel = app.reviews.sel.saturating_sub(1);
            return;
        }
        KeyCode::Char('g') => {
            reload(app);
            return ok(app, "refreshed");
        }
        _ => {}
    }
    if app.reviews.tab == Tab::Thread && app.reviews.current().is_none() {
        return ok(app, "no review yet: n on My reviews asks for one");
    }
    let item = |kind| RAsk::Item(kind);
    match (app.reviews.tab, k.code) {
        (Tab::Reviews, KeyCode::Char('n')) => open_new(app),
        (Tab::Reviews, KeyCode::Enter) => {
            if let Some(r) = app.reviews.reviews.get(app.reviews.sel) {
                app.reviews.review = Some(r.id.clone());
                switch(app, Tab::Thread.index());
            }
        }
        (Tab::Reviews | Tab::Thread, KeyCode::Char('i')) => {
            if sel_review(app).is_some() {
                ask(app, RAsk::Invite, "A link for a reviewer (their node …/p2p/ID or peer; empty: just the link)", "")
            }
        }
        (Tab::Reviews, KeyCode::Char('o')) => {
            let Some(r) = sel_review(app) else { return };
            if r.status == ReviewStatus::Open {
                let relays = app.prefs.net.relays.clone();
                spawn(app, "close", move |s, n, _| {
                    let r = rnet::close(s, Some(n), &r, &relays, ORIGIN)?;
                    Ok(vec![format!("closed {}; reviewers' links ended", r.title())])
                });
            } else {
                let res = review::reopen(&app.store, ORIGIN, &r);
                done(app, res, |r| format!("opened {} again: i makes new links", r.title()));
            }
        }
        (Tab::Reviews, KeyCode::Char('d')) => {
            if let Some(r) = sel_review(app) {
                app.reviews.confirm = Some(Confirm::Remove(r.id.clone()));
                ok(app, format!("delete the review {} and its thread? (revisions stay) y/n", r.title()));
            }
        }
        (Tab::Reviews | Tab::Thread, KeyCode::Char('p')) => {
            let Some(r) = sel_review(app) else { return };
            match review::view(&app.store, &r) {
                Ok(v) => app.reviews.detail = content_lines(&v),
                Err(e) => err(app, e.to_string()),
            }
        }
        (Tab::Reviews | Tab::Thread, KeyCode::Char('H')) => {
            let Some(r) = sel_review(app) else { return };
            let key =
                if r.kind == ReviewKind::Record { format!("{}/{}", r.model_id.clone().unwrap_or_default(), r.ref_id) } else { r.ref_id.clone() };
            match review::revisions(&app.store, r.kind, &key) {
                Ok(vs) if vs.is_empty() => ok(app, "no revisions yet: merging a review makes one"),
                Ok(vs) => {
                    app.reviews.detail = vs
                        .iter()
                        .map(|v| format!("rev {} · {} · {} suggestion(s)", v.rev, crate::ingest::time::fmt_ts(v.created_at), v.items.len()))
                        .collect()
                }
                Err(e) => err(app, e.to_string()),
            }
        }
        (Tab::Thread | Tab::Joined, KeyCode::Char('c')) => ask(app, item(ReviewItemKind::Comment), hint(ReviewItemKind::Comment), ""),
        (Tab::Thread | Tab::Joined, KeyCode::Char('e')) => ask(app, item(ReviewItemKind::Suggestion), hint(ReviewItemKind::Suggestion), ""),
        (Tab::Thread | Tab::Joined, KeyCode::Char('h')) => ask(app, item(ReviewItemKind::Highlight), hint(ReviewItemKind::Highlight), ""),
        (Tab::Thread | Tab::Joined, KeyCode::Char('v')) => ask(app, item(ReviewItemKind::Vote), hint(ReviewItemKind::Vote), ""),
        (Tab::Thread, KeyCode::Char('r')) => {
            if app.reviews.items.get(app.reviews.sel).is_some() {
                ask(app, RAsk::Reply, "Reply", "")
            }
        }
        (Tab::Thread, KeyCode::Char('a' | 'x')) => {
            let Some(r) = app.reviews.current().cloned() else { return };
            let Some(it) = app.reviews.items.get(app.reviews.sel).cloned() else { return };
            let accept = k.code == KeyCode::Char('a');
            let res = review::decide(&app.store, ORIGIN, &r, &[it.id], accept);
            done(app, res, |v| format!("{} {}", if accept { "accepted" } else { "rejected" }, v.first().map(review::line).unwrap_or_default()));
        }
        (Tab::Thread, KeyCode::Char('A' | 'X')) => {
            let Some(r) = app.reviews.current().cloned() else { return };
            let accept = k.code == KeyCode::Char('A');
            let res = review::decide(&app.store, ORIGIN, &r, &[], accept);
            done(app, res, |v| format!("{} {} open suggestion(s)", if accept { "accepted" } else { "rejected" }, v.len()));
        }
        (Tab::Thread, KeyCode::Char('m')) => merge(app, false),
        (Tab::Thread, KeyCode::Char('F')) => merge(app, true),
        (Tab::Thread, KeyCode::Char('w')) => {
            let Some(r) = app.reviews.current().cloned() else { return };
            let Some(it) = app.reviews.items.get(app.reviews.sel).cloned() else { return };
            let res = review::withdraw(&app.store, ORIGIN, &r, &Who::Author, &it.id);
            done(app, res, |i| format!("withdrew {}", i.short_id()));
        }
        (Tab::Thread, KeyCode::Char('D')) => {
            let Some(r) = app.reviews.current().cloned() else { return };
            let Some(v) = app.reviews.reviewers.iter().rev().find(|v| matches!(v.status, crate::store::ReviewerStatus::Joined)).cloned() else {
                return ok(app, "no joined reviewer");
            };
            let relays = app.prefs.net.relays.clone();
            spawn(app, "drop", move |s, n, _| {
                let v = rnet::drop_reviewer(s, Some(n), &r, &v, &relays, ORIGIN)?;
                Ok(vec![format!("took {} off {}", v.title(), r.title())])
            });
        }
        (Tab::Joined, KeyCode::Char('j')) => ask(app, RAsk::Join, "Review link (mnnn-x1-…)", ""),
        (Tab::Joined, KeyCode::Char('s')) => {
            let Some(g) = app.reviews.joined.get(app.reviews.sel).cloned() else { return };
            let relays = app.prefs.net.relays.clone();
            spawn(app, "sync", move |s, n, _| {
                let (g, r) = rnet::refresh(s, n, &g, &relays, ORIGIN)?;
                let mut l = vec![format!("{}: {}; you may {}; {} kept item(s) sent", g.title(), g.status.as_str(), g.allow.join(", "), r.sent)];
                l.extend(r.refused.iter().map(|e| format!("dropped (refused): {e}")));
                l.extend(r.model);
                Ok(l)
            });
        }
        (Tab::Joined, KeyCode::Enter) => {
            let Some(g) = app.reviews.joined.get(app.reviews.sel).cloned() else { return };
            if g.status != ReviewingStatus::Joined {
                app.reviews.detail = copy_lines(&g);
                return ok(app, format!("{} is {}: archived, read-only copy", g.title(), g.status.as_str()));
            }
            let relays = app.prefs.net.relays.clone();
            spawn(app, "view", move |s, n, _| {
                Ok(match rnet::refresh(s, n, &g, &relays, ORIGIN) {
                    Ok((g, _)) => copy_lines(&g),
                    Err(e) if !g.copy.is_null() => {
                        let mut l = vec![format!("{e}; the copy kept here:")];
                        l.extend(copy_lines(&g));
                        l
                    }
                    Err(e) => return Err(e),
                })
            });
        }
        (Tab::Joined, KeyCode::Char('l')) => {
            if let Some(g) = app.reviews.joined.get(app.reviews.sel) {
                let (id, t) = (g.review_id.clone(), g.title());
                app.reviews.confirm = Some(Confirm::Leave(id));
                ok(app, format!("leave the review {t}? y/n"));
            }
        }
        _ => {}
    }
}

/// A joined review's copy and thread, as lines.
fn copy_lines(g: &crate::store::Reviewing) -> Vec<String> {
    let items: Vec<crate::store::ReviewItem> = serde_json::from_value(g.thread.clone()).unwrap_or_default();
    let (a, c, r) = review::tally(&items);
    let mut l = vec![format!("{}: {} items, votes {a} approve · {c} changes · {r} reject", g.title(), items.len())];
    l.extend(content_lines(&g.copy));
    l.push(String::new());
    l.extend(items.iter().map(review::line));
    l
}

fn switch(app: &mut App, i: usize) {
    open(app, Tab::ALL[i.min(Tab::ALL.len() - 1)]);
}

pub fn on_click(app: &mut App, i: usize) {
    if app.reviews.ask.is_some() {
        return;
    }
    if app.reviews.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < rows(app) {
        app.reviews.sel = i;
        app.reviews.detail.clear();
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.reviews;
    let tabs: Vec<String> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, x)| if *x == st.tab { format!("[{} {}]", i + 1, x.title()) } else { format!(" {} {} ", i + 1, x.title()) })
        .collect();
    let busy = st.job.as_ref().map(|j| format!(" · {}…", j.what)).unwrap_or_default();
    let mut v = View::new(format!("Reviews · {}{busy}", st.tab.title()));
    v.head.push(Line::styled(tabs.join(" "), Style::new().fg(t.accent)));
    let fg = Style::new().fg(t.fg);
    let mt = Style::new().fg(t.muted);
    v.head.push(Line::raw(""));
    match st.tab {
        Tab::Reviews => {
            if st.reviews.is_empty() {
                v.head.push(muted(&t, " no reviews yet: n asks for one of an entry, journal, project, live search or record"));
            }
            for (i, r) in st.reviews.iter().enumerate() {
                let mark = if st.review.as_deref() == Some(r.id.as_str()) { "▸" } else { " " };
                v.rows.push(Line::from(vec![
                    Span::styled(format!("{mark}{:<28} {:<8} {:<6}", r.title(), r.kind.as_str(), r.status.as_str()), row_style(&t, i == st.sel)),
                    Span::styled(format!(" may {}", r.allow.join(" ")), mt),
                ]));
            }
        }
        Tab::Thread => match st.current() {
            Some(r) => {
                let (a, c, j) = review::tally(&st.items);
                // An untitled review's title is already "kind name": don't say it twice.
                let what = if r.title.is_empty() { r.title() } else { format!("{} ({} {})", r.title, r.kind.label(), r.name) };
                v.head.push(Line::styled(format!(" {what} · {}", r.status.as_str()), fg));
                let who: Vec<String> = st.reviewers.iter().map(|v| format!("{} ({})", v.title(), v.status.as_str())).collect();
                v.head.push(muted(&t, format!(" reviewers: {}", if who.is_empty() { "none yet (i makes a link)".into() } else { who.join(", ") })));
                v.head.push(muted(&t, format!(" votes: {a} approve · {c} changes · {j} reject")));
                if !r.note.is_empty() {
                    v.head.push(muted(&t, format!(" note: {}", r.note)));
                }
                if st.items.is_empty() {
                    v.head.push(muted(&t, " nothing in the thread yet"));
                }
                for (i, it) in st.items.iter().enumerate() {
                    let faded = matches!(it.state, ReviewItemState::Withdrawn | ReviewItemState::Rejected);
                    let style = if i == st.sel {
                        row_style(&t, true)
                    } else if faded {
                        mt
                    } else {
                        fg
                    };
                    v.rows.push(Line::styled(format!(" {}", review::line(it)), style));
                }
            }
            None => v.head.push(muted(&t, " no review yet: n on My reviews asks for one")),
        },
        Tab::Joined => {
            if st.joined.is_empty() {
                v.head.push(muted(&t, " none yet: j takes a review link someone gave you"));
            }
            for (i, g) in st.joined.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<28} {:<8} {:<7}", g.title(), g.kind, g.status.as_str()), row_style(&t, i == st.sel)),
                    Span::styled(format!(" at …{} · may {}", crate::store::net::short_peer(&g.node_peer), g.allow.join(" ")), mt),
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
        Tab::Reviews => vec![
            ("n new", KeyCode::Char('n')),
            ("Enter thread", KeyCode::Enter),
            ("i link", KeyCode::Char('i')),
            ("p view", KeyCode::Char('p')),
            ("H revisions", KeyCode::Char('H')),
            ("o close/open", KeyCode::Char('o')),
            ("d delete", KeyCode::Char('d')),
        ],
        Tab::Thread => vec![
            ("c comment", KeyCode::Char('c')),
            ("e suggest", KeyCode::Char('e')),
            ("h highlight", KeyCode::Char('h')),
            ("v vote", KeyCode::Char('v')),
            ("r reply", KeyCode::Char('r')),
            ("a/x accept/reject", KeyCode::Char('a')),
            ("A all", KeyCode::Char('A')),
            ("m merge", KeyCode::Char('m')),
        ],
        Tab::Joined => vec![
            ("j join", KeyCode::Char('j')),
            ("Enter view", KeyCode::Enter),
            ("s sync", KeyCode::Char('s')),
            ("c comment", KeyCode::Char('c')),
            ("e suggest", KeyCode::Char('e')),
            ("h highlight", KeyCode::Char('h')),
            ("v vote", KeyCode::Char('v')),
            ("l leave", KeyCode::Char('l')),
        ],
    });
    keys.push(("Esc", KeyCode::Esc));
    v.keys = keys;
    draw_view(f, r, v, &t, hits);
}
