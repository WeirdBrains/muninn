//! Code ▸ Code spaces…: collections of code. The list (kind, main, members, approval,
//! flags), a space's members (★ main, pins, unreviewed edits), and Code ▸ Migrate…
//! (`mnnn migrate`: a dry-run report, then y to migrate). Thin over [`crate::code::spaces`]
//! and [`crate::code::migrate`].
use super::workspace::{View, draw_view, muted, row_style};
use super::{App, Hit};
use crate::code::{self, CodeEnv, migrate, spaces};
use crate::store::Space;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    List,
    Members,
    /// The migrate report (dry run): y migrates.
    Migrate,
}

#[derive(Debug, Default)]
pub struct SpacesState {
    pub screen: Screen,
    pub list: Vec<Space>,
    pub sel: usize,
    /// The space whose members show.
    pub open: Option<Space>,
    pub msel: usize,
    /// Approve asked (y/n).
    pub confirm: bool,
    pub msg: Option<(String, bool)>,
    /// The last run's text lines.
    pub result: Vec<String>,
    pub report: Option<migrate::Report>,
    /// Plugins ▸ KIND: only spaces of this kind; `plugins` is set for any Plugins entry.
    pub kind: Option<crate::store::SpaceKind>,
    pub plugins: bool,
}

fn reload(app: &mut App) {
    let want = app.spaces.kind;
    app.spaces.list = app.store.spaces().unwrap_or_default().into_iter().filter(|s| want.is_none_or(|k| s.kind == k)).collect();
    app.spaces.sel = app.spaces.sel.min(app.spaces.list.len().saturating_sub(1));
    if let Some(o) = &app.spaces.open {
        app.spaces.open = app.store.space(&o.id).ok().flatten();
    }
}

pub fn open(app: &mut App) {
    app.spaces.kind = None;
    app.spaces.plugins = false;
    show(app);
}

/// Plugins ▸ Parsers… (a kind) or All plugins… (None).
pub fn open_kind(app: &mut App, kind: Option<crate::store::SpaceKind>) {
    app.spaces.kind = kind;
    app.spaces.plugins = true;
    app.spaces.sel = 0;
    show(app);
}

fn show(app: &mut App) {
    let st = &mut app.spaces;
    st.screen = Screen::List;
    st.confirm = false;
    st.msg = None;
    reload(app);
    app.modal = Some(super::modal::Modal::Spaces);
}

/// Back from picking a project: the window as it was.
pub fn reopen(app: &mut App) {
    reload(app);
    app.modal = Some(super::modal::Modal::Spaces);
}

fn add_to_project(app: &mut App, s: &Space) {
    use super::workspace::{Back, Purpose, pick_project};
    pick_project(app, Purpose::Add(crate::store::ItemKind::Space, s.id.clone(), serde_json::json!({}), s.name.clone()), Back::Spaces);
}

/// `g`: give the space to one of your organizations (it then syncs to the org's nodes).
fn give(app: &mut App, s: &Space) {
    let orgs: Vec<String> = app.store.orgs().map(|l| l.into_iter().map(|o| o.name).collect()).unwrap_or_default();
    if orgs.is_empty() {
        app.spaces.msg = Some(("you have no organizations yet (Orgs ▸ New organization…)".into(), true));
        return;
    }
    app.share_back = app.modal.take().map(Box::new);
    app.share_item = Some((crate::store::ShareKind::Space, s.id.clone()));
    app.modal = Some(super::modal::Modal::Pick {
        title: format!("Give code space {} to", s.name),
        items: orgs,
        sel: 0,
        kind: super::modal::PickKind::GiveSpace,
    });
}

/// The organization was picked.
pub fn give_picked(app: &mut App, org: &str) {
    let back = app.share_back.take().map(|b| *b);
    let Some((_, id)) = app.share_item.take() else { return };
    app.modal = back;
    let r = app.store.find_org(org).and_then(|o| {
        crate::org::transfer(&app.store, "tui", &crate::org::Actor::Owner, &o, crate::store::ShareKind::Space, &id, None).map(|_| o.name)
    });
    app.spaces.msg = Some(match r {
        Ok(o) => (format!("gave it to {o} · members sync it with `mnnn org sync-code`"), false),
        Err(e) => (e.to_string(), true),
    });
    app.refresh_history(true);
}

/// One space's members (from a project item).
pub fn open_space(app: &mut App, id: &str) {
    open(app);
    if let Some(i) = app.spaces.list.iter().position(|s| s.id == id) {
        app.spaces.sel = i;
        app.spaces.open = app.spaces.list.get(i).cloned();
        app.spaces.msel = 0;
        app.spaces.screen = Screen::Members;
    }
}

/// Code ▸ Migrate…: the dry-run report first.
pub fn open_migrate(app: &mut App) {
    open(app);
    let opts = migrate::Options { dry_run: true, ..Default::default() };
    match migrate::migrate(&app.store, &app.paths.home, "tui", &opts, &mut |_, _| Ok(None)) {
        Ok(r) => {
            app.spaces.report = Some(r);
            app.spaces.screen = Screen::Migrate;
        }
        Err(e) => app.spaces.msg = Some((e.to_string(), true)),
    }
}

/// The one-time offer at start (never automatic).
pub fn offer(app: &mut App) {
    if let Some(m) = migrate::offer_once(&app.store, &app.paths.home) {
        app.set_status(format!("{m} · Code ▸ Migrate…"));
    }
}

fn selected(app: &App) -> Option<Space> {
    app.spaces.list.get(app.spaces.sel).cloned()
}

fn run_space(app: &mut App, s: &Space) {
    run_space_on(app, s, None);
}

/// Shared data into a space (Share ▸ space · NAME): open its members and run it over the
/// data once its line-up is approved (the space is not changed).
pub fn run_piped(app: &mut App, space_id: &str, src: code::pipe::Source, what: &str) {
    open_space(app, space_id);
    let Some(s) = app.spaces.open.clone() else { return };
    if !spaces::approved(&app.store, &s) {
        app.spaces.msg = Some((format!("{what}; approve {} first (a in the list), then share again", s.name), true));
        return;
    }
    run_space_on(app, &s, Some(src));
}

fn run_space_on(app: &mut App, s: &Space, src: Option<code::pipe::Source>) {
    let env = CodeEnv::new(&app.paths.home);
    let mut log = vec![];
    let main = app.store.code_space(&s.main_code_id).ok().flatten();
    let src = src.or_else(|| main.as_ref().map(|m| code::pipe::Source::of(m.input.clone())));
    let input = src.as_ref().and_then(|src| code::pipe::frame(&app.store, &env, "tui", src, &mut |l| log.push(l.to_string())).ok().flatten());
    match spaces::run(&app.store, &env, "tui", s, input, "the TUI", &mut |l| log.push(l.to_string())) {
        Ok(info) => {
            let mut lines: Vec<String> = log.into_iter().filter(|l| l.starts_with("note:")).collect();
            if let Some(e) = &info.error {
                lines.push(format!("error: {e}"));
            }
            if let Some(v) = code::run_view(&info) {
                lines.extend(v.text_lines(12));
            }
            app.spaces.result = lines;
            app.spaces.msg = Some((format!("ran {} ({})", s.name, info.status), info.error.is_some()));
        }
        Err(e) => app.spaces.msg = Some((e.to_string(), true)),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.spaces.confirm {
        app.spaces.confirm = false;
        if k.code == KeyCode::Char('y')
            && let Some(s) = selected(app)
        {
            app.spaces.msg = Some(match spaces::approve(&app.store, "tui", &s, true) {
                Ok(_) => (format!("approved {} (its line-up and its unreviewed code)", s.name), false),
                Err(e) => (e.to_string(), true),
            });
            reload(app);
        }
        return;
    }
    match app.spaces.screen {
        Screen::Migrate => match k.code {
            KeyCode::Char('y') => {
                let r = migrate::migrate(&app.store, &app.paths.home, "tui", &migrate::Options::default(), &mut |_, _| Ok(None));
                app.spaces.msg = Some(match r {
                    Ok(r) => (
                        format!("migrated: {} created, {} skipped, {} pipeline(s) rewritten", r.created.len(), r.skipped.len(), r.pipelines.len()),
                        false,
                    ),
                    Err(e) => (e.to_string(), true),
                });
                app.spaces.report = None;
                app.spaces.screen = Screen::List;
                reload(app);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => {
                app.spaces.report = None;
                app.spaces.screen = Screen::List;
            }
            _ => {}
        },
        Screen::List => {
            let n = app.spaces.list.len();
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => app.spaces.sel = (app.spaces.sel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => app.spaces.sel = app.spaces.sel.saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(s) = selected(app) {
                        app.spaces.open = Some(s);
                        app.spaces.msel = 0;
                        app.spaces.screen = Screen::Members;
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(s) = selected(app) {
                        run_space(app, &s);
                    }
                }
                KeyCode::Char('a') => {
                    if let Some(s) = selected(app) {
                        if spaces::approved(&app.store, &s) {
                            app.spaces.msg = Some((format!("{} is approved already", s.name), false));
                        } else {
                            app.spaces.confirm = true;
                        }
                    }
                }
                KeyCode::Char(' ') => {
                    if let Some(s) = selected(app) {
                        let _ = app.store.set_space_enabled(&s.id, !s.enabled);
                        spaces::note(
                            &app.store,
                            "tui",
                            &s,
                            if s.enabled { "disabled" } else { "enabled" },
                            serde_json::json!({ "enabled": !s.enabled }),
                        );
                        reload(app);
                    }
                }
                KeyCode::Char('M') => open_migrate(app),
                KeyCode::Char('p') => {
                    if let Some(s) = selected(app) {
                        add_to_project(app, &s);
                    }
                }
                KeyCode::Char('g') => {
                    if let Some(s) = selected(app) {
                        give(app, &s);
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
                _ => {}
            }
        }
        Screen::Members => {
            let Some(s) = app.spaces.open.clone() else {
                app.spaces.screen = Screen::List;
                return;
            };
            let ms = spaces::members(&app.store, &s).unwrap_or_default();
            let n = ms.len();
            let cur = ms.get(app.spaces.msel).cloned();
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => app.spaces.msel = (app.spaces.msel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => app.spaces.msel = app.spaces.msel.saturating_sub(1),
                KeyCode::Char('p') => add_to_project(app, &s),
                KeyCode::Char('P') => {
                    if let Some(m) = cur {
                        let pin = match &m.member.pin_hash {
                            Some(_) => None,
                            None => code::trust_hash(&m.code).ok(),
                        };
                        let r = app.store.set_member_pin(&s.id, &m.code.id, pin.as_deref());
                        spaces::note(
                            &app.store,
                            "tui",
                            &s,
                            if pin.is_some() { "pin" } else { "unpin" },
                            serde_json::json!({ "code": m.code.id, "pin": pin }),
                        );
                        app.spaces.msg = Some(match r {
                            Ok(()) => (
                                format!("{} {} (the space needs approving again)", if pin.is_some() { "pinned" } else { "unpinned" }, m.code.name),
                                false,
                            ),
                            Err(e) => (e.to_string(), true),
                        });
                    }
                }
                KeyCode::Char('m') => {
                    if let Some(m) = cur {
                        let r = app.store.set_space_main(&s.id, &m.code);
                        spaces::note(&app.store, "tui", &s, &format!("main is now {}", m.code.name), serde_json::json!({ "main": m.code.id }));
                        app.spaces.msg = Some(match r {
                            Ok(()) => (format!("{} is the main now (the space needs approving again)", m.code.name), false),
                            Err(e) => (e.to_string(), true),
                        });
                        app.spaces.msel = 0;
                    }
                }
                KeyCode::Char('e') => {
                    if let Some(m) = cur {
                        super::code::open_space(app, &m.code.id);
                        return;
                    }
                }
                KeyCode::Char('r') => run_space(app, &s),
                KeyCode::Esc | KeyCode::Char('q') => app.spaces.screen = Screen::List,
                _ => {}
            }
            reload(app);
        }
    }
}

pub fn on_click(app: &mut App, i: usize) {
    match app.spaces.screen {
        Screen::List => app.spaces.sel = i,
        Screen::Members => app.spaces.msel = i,
        Screen::Migrate => {}
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.spaces;
    let mut v;
    match st.screen {
        Screen::Migrate => {
            v = View::new("Migrate code to code spaces (mnnn migrate) · dry run");
            if let Some(r) = &st.report {
                if r.is_empty() {
                    v.head.push(muted(&t, " nothing to migrate: every code is in a code space"));
                }
                for c in &r.created {
                    let what = if c.renamed {
                        format!(" renamed  {} → {} ({})", c.code, c.space, c.kind)
                    } else {
                        format!(" created  {} ({})", c.space, c.kind)
                    };
                    v.rows.push(Line::from(vec![
                        Span::styled(what, Style::new().fg(t.accent)),
                        Span::styled(format!("  {}", c.why), Style::new().fg(t.muted)),
                    ]));
                }
                for k in &r.skipped {
                    v.rows.push(Line::from(vec![
                        Span::styled(format!(" skipped  {}", k.code), Style::new().fg(t.warning)),
                        Span::styled(format!("  {}", k.why), Style::new().fg(t.muted)),
                    ]));
                }
                for (p, ver) in &r.pipelines {
                    v.rows.push(Line::styled(format!(" pipeline {p}: code: steps → space: (version {ver})"), Style::new().fg(t.fg)));
                }
                v.head.push(muted(&t, " a backup is written first; running it again changes nothing · unclear kinds are skipped here (give them with `mnnn migrate --kind`)"));
            }
            v.keys = vec![("y migrate", KeyCode::Char('y')), ("Esc cancel", KeyCode::Esc)];
        }
        Screen::List => {
            v = View::new(&match (st.plugins, st.kind) {
                (true, Some(k)) => format!("Plugins ▸ {} · {k} code spaces", kind_label(k)),
                (true, None) => "Plugins ▸ All plugins · every code space, by kind".to_string(),
                _ => "Code spaces · collections of code; the main runs".to_string(),
            });
            if st.list.is_empty() && st.plugins {
                v.head.push(muted(
                    &t,
                    format!(" none yet: `mnnn plugins install PATH|CODE --kind {}`", st.kind.map(|k| k.as_str()).unwrap_or("KIND")),
                ));
            }
            for (i, s) in st.list.iter().enumerate() {
                let ok = spaces::approved(&app.store, s);
                let ms = spaces::members(&app.store, s).unwrap_or_default();
                let main = ms.iter().find(|m| m.main).map(|m| m.code.name.clone()).unwrap_or_default();
                let un = ms.iter().filter(|m| m.unreviewed).count();
                let pb = ms.iter().filter(|m| m.pin_broken).count();
                let mut flags = vec![];
                if !s.enabled {
                    flags.push("disabled".to_string());
                }
                if un > 0 {
                    flags.push(format!("{un} unreviewed"));
                }
                if pb > 0 {
                    flags.push(format!("{pb} pin changed"));
                }
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {} ", if ok { "●" } else { "○" }), Style::new().fg(if ok { t.accent } else { t.muted })),
                    Span::styled(format!("{:<24} ", s.name), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {:<16}", s.kind.as_str()), Style::new().fg(t.accent)),
                    Span::styled(format!(" main {main} · {} member(s)", ms.len()), Style::new().fg(t.muted)),
                    Span::styled(if flags.is_empty() { String::new() } else { format!(" · {}", flags.join(", ")) }, Style::new().fg(t.warning)),
                ]));
            }
            if st.list.is_empty() {
                v.head.push(muted(&t, " no code spaces yet: M migrates your code into spaces, or `mnnn spaces create NAME --kind K --main CODE`"));
            }
            for l in &st.result {
                v.head.push(muted(&t, format!(" {l}")));
            }
            v.sel = (!st.list.is_empty()).then_some(st.sel);
            v.keys = vec![
                ("Enter members", KeyCode::Enter),
                ("r run", KeyCode::Char('r')),
                ("a approve", KeyCode::Char('a')),
                ("Space on/off", KeyCode::Char(' ')),
                ("M migrate", KeyCode::Char('M')),
                ("p add to project", KeyCode::Char('p')),
                ("g give to org", KeyCode::Char('g')),
                ("Esc", KeyCode::Esc),
            ];
        }
        Screen::Members => {
            let s = st.open.clone();
            v = View::new(format!("Code space {} · members", s.as_ref().map(|s| s.name.as_str()).unwrap_or("?")));
            if let Some(s) = &s {
                for (i, m) in spaces::members(&app.store, s).unwrap_or_default().iter().enumerate() {
                    v.rows.push(Line::from(vec![
                        Span::styled(format!(" {} ", if m.main { "★" } else { " " }), Style::new().fg(t.accent)),
                        Span::styled(format!("{:<18} ", m.member.alias), row_style(&t, i == st.msel)),
                        Span::styled(format!(" {} ({})", m.code.name, m.code.lang.as_str()), Style::new().fg(t.fg)),
                        Span::styled(
                            if m.member.pin_hash.is_some() { " · pinned".into() } else { " · follows".to_string() },
                            Style::new().fg(t.muted),
                        ),
                        Span::styled(if m.pin_broken { " · changed since pinned" } else { "" }, Style::new().fg(t.error)),
                        Span::styled(if m.unreviewed { " · edited, unreviewed" } else { "" }, Style::new().fg(t.warning)),
                    ]));
                }
                if let Ok(Some(main)) = app.store.code_space(&s.main_code_id) {
                    let imp = spaces::imports(&main.source);
                    if !imp.code.is_empty() || !imp.spaces.is_empty() {
                        v.head.push(muted(
                            &t,
                            format!(
                                " main imports {}",
                                imp.code.iter().cloned().chain(imp.spaces.iter().map(|x| format!("space:{x}"))).collect::<Vec<_>>().join(", ")
                            ),
                        ));
                    }
                }
                v.head.push(muted(
                    &t,
                    format!(
                        " {} · approval covers the line-up; edits run at once (flagged); pins freeze a member",
                        if spaces::approved(&app.store, s) { "approved" } else { "not approved" }
                    ),
                ));
            }
            v.sel = Some(st.msel);
            v.keys = vec![
                ("e edit code", KeyCode::Char('e')),
                ("m make main", KeyCode::Char('m')),
                ("P pin/unpin", KeyCode::Char('P')),
                ("r run", KeyCode::Char('r')),
                ("Esc back", KeyCode::Esc),
            ];
        }
    }
    if st.confirm {
        v.ask = Some(("approve this space's line-up and its unreviewed code? (y/n)".into(), String::new()));
    }
    v.msg = st.msg.clone();
    super::share::add_key(app, &mut v.keys);
    draw_view(f, r, v, &t, hits);
}

/// A kind as its Plugins menu entry names it.
pub fn kind_label(k: crate::store::SpaceKind) -> &'static str {
    use crate::store::SpaceKind::*;
    match k {
        Parser => "Parsers",
        Transform => "Transforms",
        Analysis => "Analysis",
        IngestConnector => "Ingest connectors",
    }
}
