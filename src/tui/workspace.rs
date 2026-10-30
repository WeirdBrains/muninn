//! Shared pieces of the Projects menu popups (live searches, projects, journals): one-line
//! prompts, the "add to project" picker, history recording and list drawing.
use super::{App, Hit, block};
use crate::store::{ItemKind, NewAction, Project};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// A one-line text prompt inside a popup.
#[derive(Debug, Clone)]
pub struct Ask<K> {
    pub kind: K,
    pub title: String,
    pub value: String,
}

impl<K> Ask<K> {
    pub fn new(kind: K, title: impl Into<String>, value: impl Into<String>) -> Self {
        Ask { kind, title: title.into(), value: value.into() }
    }
}

pub enum Typed {
    Cancel,
    Done(String),
    Editing,
}

/// Edit a prompt with a key: Enter finishes, Esc cancels.
pub fn ask_key<K>(a: &mut Ask<K>, k: KeyEvent) -> Typed {
    match k.code {
        KeyCode::Esc => Typed::Cancel,
        KeyCode::Enter => Typed::Done(a.value.trim().to_string()),
        KeyCode::Backspace => {
            a.value.pop();
            Typed::Editing
        }
        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
            a.value.push(c);
            Typed::Editing
        }
        _ => Typed::Editing,
    }
}

/// Record a workspace action from the TUI and refresh the history bar.
pub fn record(app: &mut App, kind: &str, summary: String, params: serde_json::Value) {
    app.store.note_action(&NewAction::new(kind, "tui", summary).params(params));
    app.refresh_history(true);
}

pub fn row_style(t: &Theme, on: bool) -> Style {
    if on { Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD) } else { Style::new().fg(t.fg) }
}

pub fn muted<'a>(t: &Theme, s: impl Into<String>) -> Line<'a> {
    Line::styled(s.into(), Style::new().fg(t.muted))
}

/// What a popup screen shows: fixed lines on top, a selectable list, and a key bar.
pub struct View<'a> {
    pub title: String,
    pub head: Vec<Line<'a>>,
    /// First head line shown (long entries scroll).
    pub head_scroll: usize,
    pub rows: Vec<Line<'a>>,
    pub sel: Option<usize>,
    pub keys: Vec<(&'static str, KeyCode)>,
    /// (prompt title, value being typed)
    pub ask: Option<(String, String)>,
    /// One line of feedback: (text, is error).
    pub msg: Option<(String, bool)>,
}

impl<'a> View<'a> {
    pub fn new(title: impl Into<String>) -> Self {
        View { title: title.into(), head: vec![], head_scroll: 0, rows: vec![], sel: None, keys: vec![], ask: None, msg: None }
    }
}

/// Draw a [`View`]; list rows are clickable as `Hit::Row(index)`.
pub fn draw_view(f: &mut Frame, r: Rect, v: View, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
    f.render_widget(Paragraph::new("").block(block(&v.title, true, t)), r);
    let inner = Rect { x: r.x + 1, y: r.y + 1, width: r.width.saturating_sub(2), height: r.height.saturating_sub(2) };
    if inner.height < 3 {
        return;
    }
    let bottom = (if v.ask.is_some() || v.msg.is_some() { 2 } else { 1 }) as u16;
    let avail = inner.height.saturating_sub(bottom) as usize;
    let min_rows = v.rows.len().min(4);
    let head_n = v.head.len().saturating_sub(v.head_scroll).min(avail.saturating_sub(min_rows));
    let head: Vec<Line> = v.head.into_iter().skip(v.head_scroll).take(head_n).collect();
    f.render_widget(Paragraph::new(head), Rect { height: head_n as u16, ..inner });
    let space = avail - head_n;
    let off = match v.sel {
        Some(s) if s >= space => s + 1 - space,
        _ => 0,
    };
    for (i, line) in v.rows.into_iter().enumerate().skip(off).take(space) {
        let y = inner.y + (head_n + i - off) as u16;
        let row = Rect { x: inner.x, y, width: inner.width, height: 1 };
        f.render_widget(Paragraph::new(line), row);
        hits.push((row, Hit::Row(i)));
    }
    let line_y = inner.bottom().saturating_sub(2);
    if let Some((title, value)) = &v.ask {
        let l = Line::from(vec![
            Span::styled(format!(" {title}: "), Style::new().fg(t.accent).add_modifier(Modifier::BOLD)),
            Span::styled(value.clone(), Style::new().bg(t.selection_bg).fg(t.selection_fg)),
            Span::styled("▏", Style::new().fg(t.accent)),
            Span::styled("  Enter ok · Esc cancel", Style::new().fg(t.muted)),
        ]);
        f.render_widget(Paragraph::new(l), Rect { y: line_y, height: 1, ..inner });
    } else if let Some((m, err)) = &v.msg {
        let st = Style::new().fg(if *err { t.error } else { t.warning });
        f.render_widget(Paragraph::new(Line::styled(format!(" {m}"), st)), Rect { y: line_y, height: 1, ..inner });
    }
    let bar = Rect { x: inner.x + 1, y: inner.bottom().saturating_sub(1), width: inner.width.saturating_sub(2), height: 1 };
    super::widgets::key_bar(f, bar, &v.keys, t, hits);
}

/// What the project picker does with the chosen project.
#[derive(Debug, Clone)]
pub enum Purpose {
    /// Add an item (kind, ref id, payload, label for messages).
    Add(ItemKind, String, serde_json::Value, String),
    /// Fork this journal into the chosen project (or standalone).
    ForkJournal(String, String),
}

/// Where to go when the picker closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Back {
    #[default]
    Close,
    Searches,
    Journals,
    Reports,
    Rules,
}

#[derive(Debug, Default)]
pub struct PickProject {
    pub purpose: Option<Purpose>,
    pub back: Back,
    pub list: Vec<Project>,
    pub sel: usize,
    pub ask: Option<Ask<()>>,
    pub error: Option<String>,
}

impl PickProject {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
    /// Rows: optionally "standalone", the projects, then "+ New project…".
    fn standalone(&self) -> bool {
        matches!(self.purpose, Some(Purpose::ForkJournal(..)))
    }
    fn len(&self) -> usize {
        self.list.len() + 1 + usize::from(self.standalone())
    }
}

/// Open the project picker.
pub fn pick_project(app: &mut App, purpose: Purpose, back: Back) {
    app.pick_project = PickProject { purpose: Some(purpose), back, list: app.store.projects().unwrap_or_default(), ..Default::default() };
    app.modal = Some(super::modal::Modal::PickProject);
}

fn go_back(app: &mut App) {
    match app.pick_project.back {
        Back::Close => app.modal = None,
        Back::Searches => super::searches::reopen(app),
        Back::Journals => super::journals::reopen(app),
        Back::Reports => super::reports::reopen(app),
        Back::Rules => super::rules::reopen(app),
    }
}

fn finish(app: &mut App, project: Option<Project>) {
    let Some(purpose) = app.pick_project.purpose.clone() else { return go_back(app) };
    match purpose {
        Purpose::Add(kind, id, payload, label) => {
            let Some(p) = project else { return };
            match app.store.add_project_item(&p.id, kind, &id, payload) {
                Ok((it, true)) => {
                    record(
                        app,
                        "project.add",
                        format!("add {} {label} to project {}", kind.label(), p.name),
                        serde_json::json!({ "project": p.id, "item": it.id, "kind": kind, "ref": it.ref_id }),
                    );
                    app.set_status(format!("added {} {label} to project {}", kind.label(), p.name));
                }
                Ok((_, false)) => app.set_status(format!("{label} is already in project {}", p.name)),
                Err(e) => return app.pick_project.error = Some(e.to_string()),
            }
        }
        Purpose::ForkJournal(id, name) => match app.store.fork_journal(&id, project.as_ref().map(|p| p.id.as_str()), None) {
            Ok(j) => {
                let into = project.as_ref().map(|p| format!(" into project {}", p.name)).unwrap_or_default();
                record(
                    app,
                    "journal.fork",
                    format!("fork journal {name} → {}{into}", j.name),
                    serde_json::json!({ "journal": j.id, "name": j.name }),
                );
                app.set_status(format!("forked {name} → {}{into}", j.name));
                app.journals.focus_journal = Some(j.id);
            }
            Err(e) => return app.pick_project.error = Some(e.to_string()),
        },
    }
    go_back(app);
}

fn choose(app: &mut App, i: usize) {
    let pp = &mut app.pick_project;
    let off = usize::from(pp.standalone());
    if pp.standalone() && i == 0 {
        return finish(app, None);
    }
    match pp.list.get(i - off).cloned() {
        Some(p) => finish(app, Some(p)),
        None => pp.ask = Some(Ask::new((), "New project name", "")),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.pick_project.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.pick_project.ask = None,
            Typed::Done(name) => match app.store.create_project(&name, "") {
                Ok(p) => {
                    app.pick_project.ask = None;
                    record(app, "project.create", format!("create project {}", p.name), serde_json::json!({ "project": p.id, "name": p.name }));
                    finish(app, Some(p));
                }
                Err(e) => app.pick_project.error = Some(e.to_string()),
            },
            Typed::Editing => {}
        }
        return;
    }
    let pp = &mut app.pick_project;
    let n = pp.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => pp.sel = (pp.sel + 1).min(n - 1),
        KeyCode::Up | KeyCode::Char('k') => pp.sel = pp.sel.saturating_sub(1),
        KeyCode::Char('n') => pp.ask = Some(Ask::new((), "New project name", "")),
        KeyCode::Enter => {
            let i = pp.sel;
            choose(app, i)
        }
        KeyCode::Esc | KeyCode::Char('q') => go_back(app),
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.pick_project.ask.is_some() {
        return;
    }
    if app.pick_project.sel == i {
        choose(app, i);
    } else if i < app.pick_project.len() {
        app.pick_project.sel = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let pp = &app.pick_project;
    let what = match &pp.purpose {
        Some(Purpose::Add(kind, _, _, label)) => format!("Add {} {label} to project", kind.label()),
        Some(Purpose::ForkJournal(_, name)) => format!("Fork journal {name} into"),
        None => "Project".into(),
    };
    let mut v = View::new(what);
    let mut i = 0;
    if pp.standalone() {
        v.rows.push(Line::styled(" (no project: a standalone fork)", row_style(&t, pp.sel == 0)));
        i = 1;
    }
    for p in &pp.list {
        let desc = if p.description.is_empty() { String::new() } else { format!("  · {}", p.description) };
        v.rows.push(Line::styled(format!(" {}{desc}", p.name), row_style(&t, pp.sel == i)));
        i += 1;
    }
    v.rows.push(Line::styled(" + New project…", row_style(&t, pp.sel == i).fg(t.accent)));
    v.sel = Some(pp.sel);
    v.ask = pp.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = pp.error.clone().map(|e| (e, true));
    v.keys = vec![("Enter choose", KeyCode::Enter), ("n new project", KeyCode::Char('n')), ("Esc back", KeyCode::Esc)];
    draw_view(f, r, v, &t, hits);
}
