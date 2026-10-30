//! Projects ▸ Projects…: list, create, rename and delete projects; open one to see and use
//! its live searches, results and journals.
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, record, row_style};
use super::{App, Hit};
use crate::store::{ItemKind, Project, ProjectItem, SeriesRef};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PAsk {
    New,
    Rename,
    Describe,
    NewJournal,
    Import,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PScreen {
    #[default]
    List,
    /// One project (its id).
    Project(String),
}

#[derive(Debug, Default)]
pub struct ProjectsState {
    pub screen: PScreen,
    pub list: Vec<Project>,
    pub sel: usize,
    /// Items of the open project: (item, name, what it holds now).
    pub items: Vec<(ProjectItem, String, String)>,
    pub isel: usize,
    pub ask: Option<Ask<PAsk>>,
    pub confirm_delete: bool,
    pub msg: Option<(String, bool)>,
}

impl ProjectsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    fn selected(&self) -> Option<&Project> {
        self.list.get(self.sel)
    }
    fn open_project(&self) -> Option<&str> {
        match &self.screen {
            PScreen::Project(id) => Some(id),
            PScreen::List => None,
        }
    }
}

pub fn open(app: &mut App) {
    let st = &mut app.projects;
    st.screen = PScreen::List;
    st.ask = None;
    st.confirm_delete = false;
    st.msg = None;
    reload(app);
    app.modal = Some(super::modal::Modal::Projects);
}

/// Projects ▸ New project…
pub fn open_new(app: &mut App) {
    open(app);
    app.projects.ask = Some(Ask::new(PAsk::New, "New project name", ""));
}

/// Open the popup on one project.
pub fn open_project(app: &mut App, id: &str) {
    open(app);
    app.projects.screen = PScreen::Project(id.to_string());
    app.projects.isel = 0;
    reload(app);
}

fn reload(app: &mut App) {
    app.projects.list = app.store.projects().unwrap_or_default();
    let st = &mut app.projects;
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
    if let Some(id) = st.open_project().map(str::to_string) {
        let items = app.store.project_items(&id).unwrap_or_default();
        app.projects.items = items
            .into_iter()
            .map(|i| {
                let (name, now) = crate::live::describe_item(&app.store, &i);
                (i, name, now)
            })
            .collect();
        app.projects.isel = app.projects.isel.min(app.projects.items.len().saturating_sub(1));
    }
}

fn params(p: &Project) -> serde_json::Value {
    serde_json::json!({ "project": p.id, "name": p.name })
}

fn answer(app: &mut App, kind: PAsk, value: String) {
    let cur = match app.projects.open_project() {
        Some(id) => app.store.project(id).ok().flatten(),
        None => app.projects.selected().cloned(),
    };
    if kind == PAsk::Import {
        import(app, &value);
        return;
    }
    let res = match (kind, cur) {
        (PAsk::Import, _) => return,
        (PAsk::New, _) => app.store.create_project(&value, "").map(|p| ("project.create", format!("create project {}", p.name), p)),
        (PAsk::Rename, Some(p)) => {
            app.store.update_project(&p.id, Some(&value), None).map(|n| ("project.edit", format!("rename project {} → {}", p.name, n.name), n))
        }
        (PAsk::Describe, Some(p)) => {
            app.store.update_project(&p.id, None, Some(&value)).map(|n| ("project.edit", format!("describe project {}", n.name), n))
        }
        (PAsk::NewJournal, Some(p)) => match app.store.create_journal(&value, Some(&p.id), Default::default()) {
            Ok(j) => {
                record(
                    app,
                    "journal.create",
                    format!("create journal {} in project {}", j.name, p.name),
                    serde_json::json!({ "journal": j.id, "name": j.name }),
                );
                reload(app);
                app.projects.msg = Some((format!("created journal {} (Enter opens it)", j.name), false));
                app.projects.isel = app.projects.items.iter().position(|x| x.0.ref_id == j.id).unwrap_or(0);
                return;
            }
            Err(e) => Err(e),
        },
        (_, None) => return,
    };
    match res {
        Ok((kind, summary, p)) => {
            record(app, kind, summary.clone(), params(&p));
            reload(app);
            app.projects.sel = app.projects.list.iter().position(|x| x.id == p.id).unwrap_or(0);
            app.projects.msg = Some((summary, false));
        }
        Err(e) => app.projects.msg = Some((e.to_string(), true)),
    }
}

/// Enter on an item: a live search is shown, a result opens its series, a journal opens.
fn open_item(app: &mut App) {
    let Some((it, name, _)) = app.projects.items.get(app.projects.isel).cloned() else { return };
    match it.kind {
        ItemKind::LiveSearch => match app.store.live_search(&it.ref_id) {
            Ok(Some(s)) => super::searches::apply(app, &s),
            _ => app.projects.msg = Some((format!("{name} no longer exists"), true)),
        },
        ItemKind::Result => match it.series() {
            Some(r) => open_result(app, &r),
            None => app.projects.msg = Some(("this result cannot be read".into(), true)),
        },
        ItemKind::Journal => super::journals::open_journal(app, &it.ref_id),
        ItemKind::Report => super::reports::open_report(app, &it.ref_id),
        ItemKind::RuleSet => super::rules::open_rule(app, &it.ref_id),
        ItemKind::Model => super::models::open_model(app, &it.ref_id),
        ItemKind::Code => super::code::open_space(app, &it.ref_id),
    }
}

/// Show a result: its series in the focused tile; its time range (if any) becomes the time range.
pub fn open_result(app: &mut App, r: &SeriesRef) {
    let key = super::layout::SeriesKey { source: r.source.clone(), symbol: r.symbol.display(), duration: r.duration.to_string() };
    if !app.series.iter().any(|s| super::layout::SeriesKey::of(s) == key) {
        app.projects.msg = Some((format!("no data for {} now", r.label()), true));
        return;
    }
    app.modal = None;
    if r.from.is_some() || r.to.is_some() {
        app.prefs.tui.sidebar.filters.from = r.from;
        app.prefs.tui.sidebar.filters.to = r.to;
        app.filters_changed();
    }
    app.set_series(key);
    app.set_status(format!("result {}", r.label()));
}

/// E: export the project (an archive by default) or render it as a document.
/// Restore a project archive (`.mnnp`) or JSON pack as a new project.
fn import(app: &mut App, value: &str) {
    let path = std::path::PathBuf::from(super::journals::shellexpand(value.trim()));
    match crate::export::archive::import_project(&mut app.store, &path, None) {
        Ok(r) => {
            let Some(p) = r.project.clone() else { return };
            record(
                app,
                "project.import",
                format!("import project {} from {}", p.name, path.display()),
                serde_json::json!({ "project": p.id, "name": p.name, "file": path.display().to_string(), "restored": r }),
            );
            reload(app);
            app.projects.sel = app.projects.list.iter().position(|x| x.id == p.id).unwrap_or(0);
            app.projects.msg = Some((format!("imported project {}: {}", p.name, r.counts(true)), false));
        }
        Err(e) => app.projects.msg = Some((e.to_string(), true)),
    }
}

fn export(app: &mut App, id: &str) {
    let name = app.store.project(id).ok().flatten().map(|p| p.name).unwrap_or_default();
    let t = crate::export::files::Target::Project { id: id.to_string(), rows: crate::store::reports::DEFAULT_ROWS, data: true };
    super::docexport::open(app, t, format!("project {name}"), super::docexport::From::Projects);
}

fn drop_item(app: &mut App) {
    let Some((it, name, _)) = app.projects.items.get(app.projects.isel).cloned() else { return };
    let pname = app.store.project(&it.project_id).ok().flatten().map(|p| p.name).unwrap_or_default();
    match app.store.remove_project_item(it.id) {
        Ok(_) => {
            record(
                app,
                "project.drop",
                format!("drop {} {name} from project {pname}", it.kind.label()),
                serde_json::json!({ "project": it.project_id, "kind": it.kind, "ref": it.ref_id }),
            );
            reload(app);
            app.projects.msg = Some((format!("dropped {name} from {pname} (it still exists)"), false));
        }
        Err(e) => app.projects.msg = Some((e.to_string(), true)),
    }
}

/// a: add the series selected in the sidebar (with the current time range) as a result.
fn add_selected_series(app: &mut App, pid: &str) {
    let Some(r) = selected_series(app) else {
        app.projects.msg = Some(("select a series in the sidebar first".into(), true));
        return;
    };
    match app.store.add_project_result(pid, &r) {
        Ok((it, new)) => {
            if new {
                record(
                    app,
                    "project.add",
                    format!("add result {} to project", r.label()),
                    serde_json::json!({ "project": pid, "item": it.id, "kind": "result", "ref": it.ref_id }),
                );
            }
            reload(app);
            app.projects.msg = Some((format!("{} {}", r.label(), if new { "added" } else { "is already here" }), false));
        }
        Err(e) => app.projects.msg = Some((e.to_string(), true)),
    }
}

/// The series selected in the sidebar as a result, with the current time range.
pub fn selected_series(app: &App) -> Option<SeriesRef> {
    let i = *app.sidebar.rows.get(app.sidebar.sel)?;
    let s = app.series.get(i)?;
    let f = &app.prefs.tui.sidebar.filters;
    Some(SeriesRef { source: s.source.clone(), symbol: s.symbol.clone(), duration: s.duration, from: f.from, to: f.to })
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.projects.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.projects.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.projects.ask = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if app.projects.confirm_delete {
        app.projects.confirm_delete = false;
        if let (KeyCode::Char('y' | 'Y'), Some(p)) = (k.code, app.projects.selected().cloned()) {
            match app.store.remove_project(&p.id) {
                Ok(_) => {
                    record(app, "project.remove", format!("remove project {}", p.name), params(&p));
                    reload(app);
                    app.projects.msg = Some((format!("removed project {} (its journals are standalone now)", p.name), false));
                }
                Err(e) => app.projects.msg = Some((e.to_string(), true)),
            }
        } else {
            app.projects.msg = Some(("delete cancelled".into(), false));
        }
        return;
    }
    app.projects.msg = None;
    if let Some(pid) = app.projects.open_project().map(str::to_string) {
        let n = app.projects.items.len();
        let st = &mut app.projects;
        match k.code {
            KeyCode::Down | KeyCode::Char('j') => st.isel = (st.isel + 1).min(n.saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => st.isel = st.isel.saturating_sub(1),
            KeyCode::Enter => open_item(app),
            KeyCode::Char('d') | KeyCode::Delete => drop_item(app),
            KeyCode::Char('n') => st.ask = Some(Ask::new(PAsk::NewJournal, "New journal in this project", "")),
            KeyCode::Char('s') => super::searches::open_save(app, Some(pid)),
            KeyCode::Char('a') => add_selected_series(app, &pid),
            KeyCode::Char('E') => export(app, &pid),
            KeyCode::Char('r') => {
                let v = app.store.project(&pid).ok().flatten().map(|p| p.name).unwrap_or_default();
                app.projects.ask = Some(Ask::new(PAsk::Rename, "Rename project to", v));
            }
            KeyCode::Char('e') => {
                let v = app.store.project(&pid).ok().flatten().map(|p| p.description).unwrap_or_default();
                app.projects.ask = Some(Ask::new(PAsk::Describe, "Description", v));
            }
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
                st.screen = PScreen::List;
                reload(app);
            }
            KeyCode::Char('q') => app.modal = None,
            _ => {}
        }
        return;
    }
    let st = &mut app.projects;
    let n = st.list.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Char('n') | KeyCode::Char('a') => st.ask = Some(Ask::new(PAsk::New, "New project name", "")),
        KeyCode::Char('I') => st.ask = Some(Ask::new(PAsk::Import, "Import project from (.mnnp or .json)", "")),
        KeyCode::Char('r') => {
            if let Some(p) = st.selected() {
                let v = p.name.clone();
                st.ask = Some(Ask::new(PAsk::Rename, "Rename project to", v));
            }
        }
        KeyCode::Char('e') => {
            if let Some(p) = st.selected() {
                let v = p.description.clone();
                st.ask = Some(Ask::new(PAsk::Describe, "Description", v));
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(p) = st.selected() {
                st.msg = Some((format!("delete project {}? Its journals stay. y confirms", p.name), false));
                st.confirm_delete = true;
            }
        }
        KeyCode::Char('E') => {
            if let Some(p) = st.selected() {
                let id = p.id.clone();
                export(app, &id);
            }
        }
        KeyCode::Enter | KeyCode::Right => {
            if let Some(p) = st.selected() {
                st.screen = PScreen::Project(p.id.clone());
                st.isel = 0;
                reload(app);
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.projects.ask.is_some() {
        return;
    }
    let st = &mut app.projects;
    let (cur, n) = if st.open_project().is_some() { (&mut st.isel, st.items.len()) } else { (&mut st.sel, st.list.len()) };
    if *cur == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < n {
        *cur = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.projects;
    let mut v = match st.open_project() {
        None => {
            let mut v = View::new("Projects");
            for (i, p) in st.list.iter().enumerate() {
                let n = app.store.project_items(&p.id).map(|v| v.len()).unwrap_or(0);
                let desc = if p.description.is_empty() { String::new() } else { format!("  {}", p.description) };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {} ", p.name), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {n} items"), Style::new().fg(t.accent)),
                    Span::styled(desc, Style::new().fg(t.muted)),
                ]));
            }
            if st.list.is_empty() {
                v.head.push(muted(&t, " no projects yet: n creates one"));
            }
            v.sel = (!st.list.is_empty()).then_some(st.sel);
            v.keys = vec![
                ("Enter open", KeyCode::Enter),
                ("n new", KeyCode::Char('n')),
                ("r rename", KeyCode::Char('r')),
                ("e describe", KeyCode::Char('e')),
                ("E export", KeyCode::Char('E')),
                ("I import", KeyCode::Char('I')),
                ("d delete", KeyCode::Char('d')),
                ("Esc", KeyCode::Esc),
            ];
            v
        }
        Some(id) => {
            let p = app.store.project(id).ok().flatten();
            let name = p.as_ref().map(|p| p.name.clone()).unwrap_or_default();
            let mut v = View::new(format!("Project {name}"));
            if let Some(p) = &p
                && !p.description.is_empty()
            {
                v.head.push(muted(&t, format!(" {}", p.description)));
            }
            v.head.push(muted(&t, " everything below shows what the data holds now"));
            v.head.push(Line::raw(""));
            for (i, (it, name, now)) in st.items.iter().enumerate() {
                let icon = match it.kind {
                    ItemKind::LiveSearch => "⌕",
                    ItemKind::Result => "▤",
                    ItemKind::Journal => "✎",
                    ItemKind::RuleSet => "§",
                    ItemKind::Report => "▦",
                    ItemKind::Model => "◇",
                    ItemKind::Code => "λ",
                };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {icon} {name} "), row_style(&t, i == st.isel)),
                    Span::styled(format!(" {}", it.kind.label()), Style::new().fg(t.accent)),
                    Span::styled(format!("  {now}"), Style::new().fg(t.muted)),
                ]));
            }
            if st.items.is_empty() {
                v.head.push(muted(&t, " empty: s saves the current search here, a adds the sidebar's series, n starts a journal"));
                v.head.push(muted(&t, " (p in Live searches and Journals, a in the sidebar, also add to a project)"));
            }
            v.sel = (!st.items.is_empty()).then_some(st.isel);
            v.keys = vec![
                ("Enter open", KeyCode::Enter),
                ("s save search", KeyCode::Char('s')),
                ("a add series", KeyCode::Char('a')),
                ("n new journal", KeyCode::Char('n')),
                ("d drop", KeyCode::Char('d')),
                ("E export", KeyCode::Char('E')),
                ("r rename", KeyCode::Char('r')),
                ("Esc back", KeyCode::Esc),
            ];
            v
        }
    };
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    draw_view(f, r, v, &t, hits);
}
