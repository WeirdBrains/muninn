//! Projects ▸ Live searches…: saved searches that always run on the current data.
use super::workspace::{Ask, Back, Purpose, Typed, View, ask_key, draw_view, muted, pick_project, record, row_style};
use super::{App, Hit};
use crate::store::{ItemKind, LiveSearch, SearchSpec, SearchTarget};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SAsk {
    /// Save the current view as a new live search.
    Save,
    Rename,
    Fork,
}

#[derive(Debug, Default)]
pub struct SearchesState {
    /// Each live search with its match count now.
    pub list: Vec<(LiveSearch, usize)>,
    pub sel: usize,
    pub ask: Option<Ask<SAsk>>,
    pub confirm_delete: bool,
    pub msg: Option<(String, bool)>,
    /// Save into this project too (Projects ▸ project ▸ s).
    pub into_project: Option<String>,
    /// Open the share picker on the search once it is saved (the main view's Share button).
    pub then_share: bool,
}

impl SearchesState {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    fn selected(&self) -> Option<&LiveSearch> {
        self.list.get(self.sel).map(|x| &x.0)
    }
}

/// The search the sidebar shows now: its text, filters and sort.
pub fn current_spec(app: &App) -> SearchSpec {
    let p = &app.prefs.tui.sidebar;
    SearchSpec {
        target: SearchTarget::Series,
        query: app.sidebar.query.trim().to_string(),
        filters: p.filters.clone(),
        sort: p.sort,
        desc: p.desc,
        ..Default::default()
    }
}

fn params(s: &LiveSearch) -> serde_json::Value {
    serde_json::json!({ "search": s.id, "name": s.name, "spec": s.spec })
}

pub fn open(app: &mut App) {
    app.searches.ask = None;
    app.searches.confirm_delete = false;
    app.searches.msg = None;
    app.searches.into_project = None;
    app.searches.then_share = false;
    reopen(app);
}

/// Show the popup again (after the project picker), keeping the selection.
pub fn reopen(app: &mut App) {
    reload(app);
    app.modal = Some(super::modal::Modal::Searches);
}

/// Projects ▸ Save search as live search… (B): ask for a name for the current view.
pub fn open_save(app: &mut App, project: Option<String>) {
    open(app);
    app.searches.into_project = project;
    let q = app.sidebar.query.trim().to_string();
    app.searches.ask = Some(Ask::new(SAsk::Save, "Save current search as", q));
}

/// The main view's Share button: name the view (saved as a live search), then pick where it goes.
pub fn open_save_then_share(app: &mut App) {
    open_save(app, None);
    app.searches.then_share = true;
    if let Some(a) = app.searches.ask.as_mut() {
        a.title = "Share this view: save it as".into();
    }
}

fn reload(app: &mut App) {
    let list: Vec<LiveSearch> = app.store.live_searches().unwrap_or_default();
    let st = &mut app.searches;
    st.list = list.into_iter().map(|s| (crate::live::count(&app.store, &s.spec), s)).map(|(n, s)| (s, n)).collect();
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
}

fn select_id(app: &mut App, id: &str) {
    app.searches.sel = app.searches.list.iter().position(|x| x.0.id == id).unwrap_or(0);
}

/// Show a live search's results: series searches set the sidebar, journal searches list entries.
pub fn apply(app: &mut App, s: &LiveSearch) {
    match s.spec.target {
        SearchTarget::Series => {
            app.sidebar.query = s.spec.query.clone();
            app.sidebar.sel = 0;
            let p = &mut app.prefs.tui.sidebar;
            p.filters = s.spec.filters.clone();
            p.sort = s.spec.sort;
            p.desc = s.spec.desc;
            app.prefs.tui.sidebar_collapsed = false;
            app.sidebar.focused = true;
            app.sidebar.searching = false;
            app.history.focused = false;
            app.modal = None;
            app.filters_changed();
            let n = app.sidebar.rows.len();
            app.set_status(format!("live search {}: {n} series ({})", s.name, s.spec.describe()));
        }
        SearchTarget::Journals => super::journals::open_found(app, s),
        SearchTarget::Records => super::models::open_found(app, s),
    }
}

fn save(app: &mut App, name: &str) {
    let spec = current_spec(app);
    match app.store.save_live_search(name, &spec, None) {
        Ok(s) => {
            record(app, "search.save", format!("save live search {}", s.name), params(&s));
            let mut m = format!("saved live search {}: {}", s.name, s.spec.describe());
            if let Some(p) = app.searches.into_project.take()
                && let Ok((it, _)) = app.store.add_project_item(&p, ItemKind::LiveSearch, &s.id, serde_json::json!({}))
            {
                let pname = app.store.project(&p).ok().flatten().map(|p| p.name).unwrap_or_default();
                record(
                    app,
                    "project.add",
                    format!("add live search {} to project {pname}", s.name),
                    serde_json::json!({ "project": p, "item": it.id, "kind": "live_search", "ref": s.id }),
                );
                m.push_str(&format!(" (in project {pname})"));
            }
            reload(app);
            select_id(app, &s.id);
            app.set_status(m.clone());
            app.searches.msg = Some((m, false));
            if std::mem::take(&mut app.searches.then_share) {
                super::share::open(app);
            }
        }
        Err(e) => app.searches.msg = Some((e.to_string(), true)),
    }
}

fn answer(app: &mut App, kind: SAsk, value: String) {
    let sel = app.searches.selected().cloned();
    let res = match (kind, sel) {
        (SAsk::Save, _) => return save(app, &value),
        (SAsk::Rename, Some(s)) => {
            app.store.update_live_search(&s.id, Some(&value), None).map(|n| ("search.edit", format!("rename live search {} → {}", s.name, n.name), n))
        }
        (SAsk::Fork, Some(s)) => {
            let name = (!value.is_empty()).then_some(value.as_str());
            app.store.fork_live_search(&s.id, name, None).map(|n| ("search.fork", format!("fork live search {} → {}", s.name, n.name), n))
        }
        (_, None) => return,
    };
    match res {
        Ok((kind, summary, s)) => {
            record(app, kind, summary.clone(), params(&s));
            reload(app);
            select_id(app, &s.id);
            app.searches.msg = Some((summary, false));
        }
        Err(e) => app.searches.msg = Some((e.to_string(), true)),
    }
}

/// w: overwrite the selected live search with the current view (refine / optimize it).
fn overwrite(app: &mut App) {
    let Some(s) = app.searches.selected().cloned() else { return };
    let spec = current_spec(app);
    match app.store.update_live_search(&s.id, None, Some(&spec)) {
        Ok(n) => {
            let m = format!("live search {} now: {}", n.name, n.spec.describe());
            record(app, "search.edit", format!("refine live search {}", n.name), params(&n));
            reload(app);
            app.searches.msg = Some((m, false));
        }
        Err(e) => app.searches.msg = Some((e.to_string(), true)),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.searches.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.searches.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.searches.ask = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    let st = &mut app.searches;
    if st.confirm_delete {
        st.confirm_delete = false;
        if let (KeyCode::Char('y' | 'Y'), Some(s)) = (k.code, st.selected().cloned()) {
            match app.store.remove_live_search(&s.id) {
                Ok(_) => {
                    record(app, "search.remove", format!("remove live search {}", s.name), params(&s));
                    reload(app);
                    app.searches.msg = Some((format!("removed live search {}", s.name), false));
                }
                Err(e) => app.searches.msg = Some((e.to_string(), true)),
            }
        } else {
            st.msg = Some(("delete cancelled".into(), false));
        }
        return;
    }
    let n = st.list.len();
    st.msg = None;
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Char('n') | KeyCode::Char('a') => {
            let q = app.sidebar.query.trim().to_string();
            app.searches.ask = Some(Ask::new(SAsk::Save, "Save current search as", q));
        }
        KeyCode::Enter => {
            if let Some(s) = st.selected().cloned() {
                apply(app, &s);
            }
        }
        KeyCode::Char('f') => {
            if let Some(s) = st.selected() {
                let v = format!("{} (fork)", s.name);
                st.ask = Some(Ask::new(SAsk::Fork, "Fork as", v));
            }
        }
        KeyCode::Char('r') => {
            if let Some(s) = st.selected() {
                let v = s.name.clone();
                st.ask = Some(Ask::new(SAsk::Rename, "Rename to", v));
            }
        }
        KeyCode::Char('w') => overwrite(app),
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(s) = st.selected() {
                st.msg = Some((format!("delete live search {}? y confirms, any other key cancels", s.name), false));
                st.confirm_delete = true;
            }
        }
        KeyCode::Char('p') => {
            if let Some(s) = st.selected().cloned() {
                pick_project(app, Purpose::Add(ItemKind::LiveSearch, s.id.clone(), serde_json::json!({}), s.name.clone()), Back::Searches);
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.searches.ask.is_some() {
        return;
    }
    if app.searches.sel == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < app.searches.list.len() {
        app.searches.sel = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let now = current_spec(app);
    let st = &app.searches;
    let mut v = View::new("Live searches · always run on the current data");
    v.head.push(muted(&t, format!(" current view: {}", now.describe())));
    v.head.push(Line::raw(""));
    let w = st.list.iter().map(|(s, _)| s.name.chars().count()).max().unwrap_or(4).min(30);
    for (i, (s, n)) in st.list.iter().enumerate() {
        let on = i == st.sel;
        let fork = if s.forked_from.is_some() { " ⑂" } else { "" };
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {:<w$}{fork} ", s.name), row_style(&t, on)),
            Span::styled(
                format!(
                    " {n:>5} {} ",
                    match s.spec.target {
                        SearchTarget::Journals => "entries",
                        SearchTarget::Records => "records",
                        SearchTarget::Series => "series ",
                    }
                ),
                Style::new().fg(t.accent),
            ),
            Span::styled(format!(" {}", s.spec.describe()), Style::new().fg(t.muted)),
        ]));
    }
    if st.list.is_empty() {
        v.head.push(muted(&t, " no live searches yet: n saves the current sidebar search (text, filters, sort)"));
    }
    v.sel = (!st.list.is_empty()).then_some(st.sel);
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    v.keys = vec![
        ("Enter show", KeyCode::Enter),
        ("n save view", KeyCode::Char('n')),
        ("f fork", KeyCode::Char('f')),
        ("w use view", KeyCode::Char('w')),
        ("r rename", KeyCode::Char('r')),
        ("d delete", KeyCode::Char('d')),
        ("p project", KeyCode::Char('p')),
        ("Esc", KeyCode::Esc),
    ];
    super::share::add_key(app, &mut v.keys);
    draw_view(f, r, v, &t, hits);
}
