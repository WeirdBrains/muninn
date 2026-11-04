//! Projects ▸ Reports… (O): build reports from sections (text, records, series, journals,
//! projects, authors, code), preview them and export them.
use super::docexport::From;
use super::workspace::{Ask, Back, Purpose, Typed, View, ask_key, draw_view, muted, pick_project, record, row_style};
use super::{App, Hit};
use crate::export::docs::section_summary;
use crate::export::files::Target;
use crate::store::reports::SECTION_KINDS;
use crate::store::{EntryFormat, ItemKind, NewSection, Report, Section, SectionChange, SectionKind};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RScreen {
    #[default]
    List,
    /// One report (its id).
    Report(String),
    /// Choosing what a new section shows.
    Kinds(String),
    /// Choosing the journal, project or live search a new section shows.
    Choose(String, SectionKind),
    /// Writing a text section.
    Edit,
    /// The report as text.
    Preview(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsk {
    New,
    Rename,
    Describe,
    Title,
    Rows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RConfirm {
    Report(String),
    Section(String),
}

#[derive(Debug, Clone)]
pub struct REditor {
    pub report: String,
    /// None: a new section.
    pub section: Option<String>,
    pub title: String,
    pub body: String,
    pub format: EntryFormat,
    /// 0 title, 1 text.
    pub field: usize,
}

#[derive(Debug, Default)]
pub struct ReportsState {
    pub screen: RScreen,
    pub list: Vec<Report>,
    pub sel: usize,
    pub sections: Vec<Section>,
    pub ssel: usize,
    pub ksel: usize,
    /// (label, id; None = every series)
    pub choices: Vec<(String, Option<String>)>,
    pub csel: usize,
    pub editor: Option<REditor>,
    /// (width, lines) of the preview.
    pub preview: Option<(usize, Vec<String>)>,
    pub pscroll: usize,
    /// The preview was opened from the list (Esc goes back there).
    pub preview_from_list: bool,
    pub ask: Option<Ask<RAsk>>,
    pub confirm: Option<RConfirm>,
    pub msg: Option<(String, bool)>,
}

impl ReportsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some() || self.screen == RScreen::Edit
    }
    /// The report being looked at: the open one, else the selected one.
    fn report_id(&self) -> Option<String> {
        match &self.screen {
            RScreen::Report(id) | RScreen::Kinds(id) | RScreen::Choose(id, _) | RScreen::Preview(id) => Some(id.clone()),
            RScreen::Edit => self.editor.as_ref().map(|e| e.report.clone()),
            RScreen::List => self.list.get(self.sel).map(|r| r.id.clone()),
        }
    }
}

pub fn open(app: &mut App) {
    let st = &mut app.reports;
    st.screen = RScreen::List;
    st.ask = None;
    st.confirm = None;
    st.editor = None;
    st.msg = None;
    reload(app);
    app.modal = Some(super::modal::Modal::Reports);
}

/// Projects ▸ New report…
pub fn open_new(app: &mut App) {
    open(app);
    app.reports.ask = Some(Ask::new(RAsk::New, "New report name", ""));
}

/// Open the window on one report (Enter on a report in a project).
pub fn open_report(app: &mut App, id: &str) {
    open(app);
    if app.store.report(id).ok().flatten().is_none() {
        app.reports.msg = Some(("that report no longer exists".into(), true));
        return;
    }
    app.reports.screen = RScreen::Report(id.to_string());
    app.reports.ssel = 0;
    reload(app);
}

/// Come back after the project picker or the export window.
pub fn reopen(app: &mut App) {
    reload(app);
    app.modal = Some(super::modal::Modal::Reports);
}

fn reload(app: &mut App) {
    app.reports.list = app.store.reports().unwrap_or_default();
    let st = &mut app.reports;
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
    st.preview = None;
    if let RScreen::Report(id) = &st.screen {
        let id = id.clone();
        app.reports.sections = app.store.sections(&id).unwrap_or_default();
        app.reports.ssel = app.reports.ssel.min(app.reports.sections.len().saturating_sub(1));
    }
}

fn params(r: &Report) -> serde_json::Value {
    serde_json::json!({ "report": r.id, "name": r.name })
}

fn sparams(r: &Report, s: &Section) -> serde_json::Value {
    serde_json::json!({ "report": r.id, "section": s.id, "kind": s.kind.as_str(), "position": s.position })
}

fn err(app: &mut App, e: impl std::fmt::Display) {
    app.reports.msg = Some((e.to_string(), true));
}

fn ok(app: &mut App, m: impl Into<String>) {
    app.reports.msg = Some((m.into(), false));
}

fn current(app: &App) -> Option<Report> {
    app.reports.report_id().and_then(|id| app.store.report(&id).ok().flatten())
}

fn answer(app: &mut App, kind: RAsk, v: String) {
    let cur = current(app);
    match (kind, cur) {
        (RAsk::New, _) => match app.store.create_report(&v, "") {
            Ok(r) => {
                record(app, "report.create", format!("create report {}", r.name), params(&r));
                app.reports.screen = RScreen::Report(r.id.clone());
                app.reports.ssel = 0;
                reload(app);
                ok(app, format!("created report {}: a adds a section", r.name));
            }
            Err(e) => err(app, e),
        },
        (RAsk::Rename | RAsk::Describe, Some(r)) => {
            let res =
                if kind == RAsk::Rename { app.store.update_report(&r.id, Some(&v), None) } else { app.store.update_report(&r.id, None, Some(&v)) };
            match res {
                Ok(n) => {
                    let what =
                        if kind == RAsk::Rename { format!("rename report {} → {}", r.name, n.name) } else { format!("describe report {}", n.name) };
                    record(app, "report.edit", what.clone(), params(&n));
                    reload(app);
                    ok(app, what);
                }
                Err(e) => err(app, e),
            }
        }
        (RAsk::Title | RAsk::Rows, Some(r)) => {
            let Some(s) = app.reports.sections.get(app.reports.ssel).cloned() else { return };
            let change = if kind == RAsk::Title {
                SectionChange { title: Some(v), ..Default::default() }
            } else {
                let Ok(n) = v.parse::<usize>() else { return err(app, "rows is a number (0 = every bar)") };
                let mut p = s.params.clone();
                p.rows = Some(n);
                SectionChange { params: Some(p), ..Default::default() }
            };
            match app.store.update_section(&s.id, &change) {
                Ok(n) => {
                    record(app, "report.section.edit", format!("edit section {} of report {}", n.heading(), r.name), sparams(&r, &n));
                    reload(app);
                    ok(app, format!("updated {}", n.heading()));
                }
                Err(e) => err(app, e),
            }
        }
        (_, None) => {}
    }
}

/// Add a section and show it.
fn add(app: &mut App, r: &Report, s: NewSection) {
    match app.store.add_section(&r.id, &s) {
        Ok(s) => {
            record(app, "report.section.add", format!("add {} section {} to report {}", s.kind.as_str(), s.heading(), r.name), sparams(r, &s));
            app.reports.screen = RScreen::Report(r.id.clone());
            reload(app);
            app.reports.ssel = app.reports.sections.iter().position(|x| x.id == s.id).unwrap_or(0);
            ok(app, format!("added {}", s.heading()));
        }
        Err(e) => {
            app.reports.screen = RScreen::Report(r.id.clone());
            reload(app);
            err(app, e)
        }
    }
}

/// A kind was chosen for a new section.
fn kind_chosen(app: &mut App, r: &Report, kind: SectionKind) {
    let st = &mut app.reports;
    match kind {
        SectionKind::Text => {
            st.editor = Some(REditor {
                report: r.id.clone(),
                section: None,
                title: String::new(),
                body: String::new(),
                format: EntryFormat::Markdown,
                field: 0,
            });
            st.screen = RScreen::Edit;
        }
        SectionKind::Records => match super::projects::selected_series(app) {
            Some(series) => {
                let mut s = NewSection::new(SectionKind::Records);
                s.params.series = Some(series);
                add(app, r, s);
            }
            None => {
                app.reports.screen = RScreen::Report(r.id.clone());
                err(app, "select a series in the sidebar first (its time range is used too)");
            }
        },
        SectionKind::Authors => add(app, r, NewSection::new(SectionKind::Authors)),
        SectionKind::Series | SectionKind::Journal | SectionKind::Project | SectionKind::Model | SectionKind::Code => {
            let choices: Vec<(String, Option<String>)> = match kind {
                SectionKind::Series => std::iter::once(("(every series)".to_string(), None))
                    .chain(app.store.live_searches().unwrap_or_default().into_iter().map(|s| (format!("live search {}", s.name), Some(s.id))))
                    .collect(),
                SectionKind::Journal => app.store.journals().unwrap_or_default().into_iter().map(|j| (j.name, Some(j.id))).collect(),
                SectionKind::Model => app
                    .store
                    .models()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|m| m.version > 0)
                    .map(|m| (format!("model {}", m.name), Some(m.id)))
                    .collect(),
                SectionKind::Code => app
                    .store
                    .code_spaces()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| (format!("{} ({})", c.name, c.lang.as_str()), Some(c.id)))
                    .collect(),
                _ => app.store.projects().unwrap_or_default().into_iter().map(|p| (p.name, Some(p.id))).collect(),
            };
            if choices.is_empty() {
                app.reports.screen = RScreen::Report(r.id.clone());
                return err(app, format!("no {}s yet", kind.as_str()));
            }
            let st = &mut app.reports;
            st.choices = choices;
            st.csel = 0;
            st.screen = RScreen::Choose(r.id.clone(), kind);
        }
    }
}

fn choice_made(app: &mut App, r: &Report, kind: SectionKind) {
    let Some((_, id)) = app.reports.choices.get(app.reports.csel).cloned() else { return };
    let mut s = NewSection::new(kind);
    match kind {
        SectionKind::Series => s.params.search = id,
        SectionKind::Journal => s.params.journal = id,
        SectionKind::Model => s.params.model = id,
        SectionKind::Code => s.params.space = id,
        _ => s.params.project = id,
    }
    add(app, r, s);
}

fn save_editor(app: &mut App) {
    let Some(ed) = app.reports.editor.clone() else { return };
    let Some(r) = app.store.report(&ed.report).ok().flatten() else { return };
    match &ed.section {
        None => {
            app.reports.editor = None;
            add(app, &r, NewSection::text(&ed.title, &ed.body, ed.format));
        }
        Some(id) => {
            let change = SectionChange { title: Some(ed.title.clone()), body: Some(ed.body.clone()), format: Some(ed.format), params: None };
            match app.store.update_section(id, &change) {
                Ok(s) => {
                    record(app, "report.section.edit", format!("edit section {} of report {}", s.heading(), r.name), sparams(&r, &s));
                    app.reports.editor = None;
                    app.reports.screen = RScreen::Report(r.id.clone());
                    reload(app);
                    ok(app, format!("saved {}", s.heading()));
                }
                Err(e) => err(app, e),
            }
        }
    }
}

fn editor_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let Some(ed) = app.reports.editor.as_mut() else { return };
    match k.code {
        KeyCode::Char('s') if ctrl => save_editor(app),
        KeyCode::F(30) => save_editor(app),
        KeyCode::Char('t') if ctrl => ed.format = ed.format.toggle(),
        KeyCode::F(31) => ed.format = ed.format.toggle(),
        KeyCode::Esc => {
            let id = ed.report.clone();
            app.reports.editor = None;
            app.reports.screen = RScreen::Report(id);
            reload(app);
            ok(app, "not saved");
        }
        KeyCode::Tab | KeyCode::BackTab => ed.field = 1 - ed.field,
        KeyCode::Enter if ed.field == 0 => ed.field = 1,
        KeyCode::Enter => ed.body.push('\n'),
        KeyCode::Backspace => {
            if ed.field == 0 {
                ed.title.pop();
            } else {
                ed.body.pop();
            }
        }
        KeyCode::Char(c) if !ctrl => {
            if ed.field == 0 {
                ed.title.push(c);
            } else {
                ed.body.push(c);
            }
        }
        _ => {}
    }
}

fn confirmed(app: &mut App, c: RConfirm) {
    match c {
        RConfirm::Report(id) => {
            let Some(r) = app.store.report(&id).ok().flatten() else { return };
            match app.store.remove_report(&id) {
                Ok(_) => {
                    record(app, "report.remove", format!("remove report {}", r.name), params(&r));
                    app.reports.screen = RScreen::List;
                    reload(app);
                    ok(app, format!("removed report {}", r.name));
                }
                Err(e) => err(app, e),
            }
        }
        RConfirm::Section(id) => {
            let (Some(r), Some(s)) = (current(app), app.store.section(&id).ok().flatten()) else { return };
            match app.store.remove_section(&id) {
                Ok(_) => {
                    record(app, "report.section.remove", format!("remove section {} from report {}", s.heading(), r.name), sparams(&r, &s));
                    reload(app);
                    ok(app, format!("removed {}", s.heading()));
                }
                Err(e) => err(app, e),
            }
        }
    }
}

fn move_by(app: &mut App, r: &Report, down: bool) {
    let st = &app.reports;
    let Some(s) = st.sections.get(st.ssel).cloned() else { return };
    let to = if down { s.position + 1 } else { s.position - 1 };
    if to < 1 || to as usize > st.sections.len() {
        return;
    }
    match app.store.move_section(&s.id, to as usize) {
        Ok(n) => {
            let mut p = sparams(r, &n);
            p["from"] = s.position.into();
            record(app, "report.section.move", format!("move section {} of report {} from {} to {}", n.heading(), r.name, s.position, n.position), p);
            reload(app);
            app.reports.ssel = app.reports.sections.iter().position(|x| x.id == n.id).unwrap_or(0);
        }
        Err(e) => err(app, e),
    }
}

/// Keys for a report, from the list or from inside it.
fn report_common(app: &mut App, k: KeyEvent, r: &Report) {
    match k.code {
        KeyCode::Char('r') => app.reports.ask = Some(Ask::new(RAsk::Rename, "Rename report to", r.name.clone())),
        KeyCode::Char('e') if app.reports.screen == RScreen::List => {
            app.reports.ask = Some(Ask::new(RAsk::Describe, "Description", r.description.clone()))
        }
        KeyCode::Char('D') => {
            app.reports.confirm = Some(RConfirm::Report(r.id.clone()));
            ok(app, format!("delete report {}? y confirms", r.name));
        }
        KeyCode::Char('p') => pick_project(app, Purpose::Add(ItemKind::Report, r.id.clone(), serde_json::json!({}), r.name.clone()), Back::Reports),
        KeyCode::Char('E') => super::docexport::open(app, Target::Report(r.id.clone()), format!("report {}", r.name), From::Reports),
        KeyCode::Char('v') => {
            app.reports.preview_from_list = app.reports.screen == RScreen::List;
            app.reports.screen = RScreen::Preview(r.id.clone());
            app.reports.preview = None;
            app.reports.pscroll = 0;
        }
        _ => {}
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.reports.screen == RScreen::Edit {
        return editor_key(app, k);
    }
    if let Some(a) = app.reports.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.reports.ask = None,
            Typed::Done(v) => {
                let kind = a.kind;
                app.reports.ask = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.reports.confirm.take() {
        if matches!(k.code, KeyCode::Char('y' | 'Y')) {
            confirmed(app, c);
        } else {
            ok(app, "delete cancelled");
        }
        return;
    }
    app.reports.msg = None;
    match app.reports.screen.clone() {
        RScreen::List => {
            let st = &mut app.reports;
            let n = st.list.len();
            let sel = st.list.get(st.sel).cloned();
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
                KeyCode::Char('n') | KeyCode::Char('a') => st.ask = Some(Ask::new(RAsk::New, "New report name", "")),
                KeyCode::Char('d') | KeyCode::Delete => {
                    if let Some(r) = sel {
                        report_common(app, KeyEvent::from(KeyCode::Char('D')), &r);
                    }
                }
                KeyCode::Enter | KeyCode::Right => {
                    if let Some(r) = sel {
                        st.screen = RScreen::Report(r.id);
                        st.ssel = 0;
                        reload(app);
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
                _ => {
                    if let Some(r) = sel {
                        report_common(app, k, &r);
                    }
                }
            }
        }
        RScreen::Report(id) => {
            let Some(r) = app.store.report(&id).ok().flatten() else { return open(app) };
            let st = &mut app.reports;
            let n = st.sections.len();
            let cur = st.sections.get(st.ssel).cloned();
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => st.ssel = (st.ssel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => st.ssel = st.ssel.saturating_sub(1),
                KeyCode::Char('a') | KeyCode::Char('n') => {
                    st.ksel = 0;
                    st.screen = RScreen::Kinds(id);
                }
                KeyCode::Char('J') => move_by(app, &r, true),
                KeyCode::Char('K') => move_by(app, &r, false),
                KeyCode::Char('t') => {
                    if let Some(s) = cur {
                        st.ask = Some(Ask::new(RAsk::Title, "Section title (empty: from what it shows)", s.title));
                    }
                }
                KeyCode::Enter | KeyCode::Char('e') => match cur {
                    Some(s) if s.kind == SectionKind::Text => {
                        st.editor = Some(REditor { report: id, section: Some(s.id), title: s.title, body: s.body, format: s.format, field: 1 });
                        st.screen = RScreen::Edit;
                    }
                    Some(s) if matches!(s.kind, SectionKind::Records | SectionKind::Model) => {
                        let v = s.params.rows.unwrap_or(crate::store::reports::DEFAULT_ROWS).to_string();
                        let what = if s.kind == SectionKind::Model { "Records to list (0 = all)" } else { "Latest bars to list (0 = every bar)" };
                        st.ask = Some(Ask::new(RAsk::Rows, what, v));
                    }
                    Some(s) => st.ask = Some(Ask::new(RAsk::Title, "Section title (empty: from what it shows)", s.title)),
                    None => {}
                },
                KeyCode::Char('d') | KeyCode::Delete => {
                    if let Some(s) = cur {
                        st.confirm = Some(RConfirm::Section(s.id.clone()));
                        ok(app, format!("remove section {}? y confirms", s.heading()));
                    }
                }
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
                    st.screen = RScreen::List;
                    st.sel = st.list.iter().position(|x| x.id == id).unwrap_or(0);
                    reload(app);
                }
                KeyCode::Char('q') => app.modal = None,
                _ => report_common(app, k, &r),
            }
        }
        RScreen::Kinds(id) => {
            let st = &mut app.reports;
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => st.ksel = (st.ksel + 1).min(SECTION_KINDS.len() - 1),
                KeyCode::Up | KeyCode::Char('k') => st.ksel = st.ksel.saturating_sub(1),
                KeyCode::Enter => {
                    let kind = SECTION_KINDS[st.ksel];
                    if let Some(r) = app.store.report(&id).ok().flatten() {
                        kind_chosen(app, &r, kind);
                    }
                }
                KeyCode::Esc | KeyCode::Backspace => st.screen = RScreen::Report(id),
                _ => {}
            }
        }
        RScreen::Choose(id, kind) => {
            let st = &mut app.reports;
            let n = st.choices.len();
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => st.csel = (st.csel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => st.csel = st.csel.saturating_sub(1),
                KeyCode::Enter => {
                    if let Some(r) = app.store.report(&id).ok().flatten() {
                        choice_made(app, &r, kind);
                    }
                }
                KeyCode::Esc | KeyCode::Backspace => st.screen = RScreen::Kinds(id),
                _ => {}
            }
        }
        RScreen::Preview(id) => {
            let st = &mut app.reports;
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => st.pscroll += 1,
                KeyCode::Up | KeyCode::Char('k') => st.pscroll = st.pscroll.saturating_sub(1),
                KeyCode::PageDown | KeyCode::Char(' ') => st.pscroll += 15,
                KeyCode::PageUp => st.pscroll = st.pscroll.saturating_sub(15),
                KeyCode::Home => st.pscroll = 0,
                KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('v') => {
                    st.screen = if st.preview_from_list { RScreen::List } else { RScreen::Report(id) };
                    reload(app);
                }
                KeyCode::Char('E') => {
                    if let Some(r) = app.store.report(&id).ok().flatten() {
                        report_common(app, k, &r);
                    }
                }
                KeyCode::Char('q') => app.modal = None,
                _ => {}
            }
        }
        RScreen::Edit => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.reports.typing() {
        return;
    }
    let st = &mut app.reports;
    let (cur, n) = match st.screen {
        RScreen::List => (&mut st.sel, st.list.len()),
        RScreen::Report(_) => (&mut st.ssel, st.sections.len()),
        RScreen::Kinds(_) => (&mut st.ksel, SECTION_KINDS.len()),
        RScreen::Choose(..) => (&mut st.csel, st.choices.len()),
        _ => return,
    };
    if *cur == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < n {
        *cur = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    if let RScreen::Preview(id) = &app.reports.screen {
        let w = (r.width as usize).saturating_sub(4).max(40);
        if app.reports.preview.as_ref().map(|p| p.0) != Some(w) {
            let text = match crate::export::docs::report(&app.store, id) {
                Ok(d) => crate::render::text::write(&d, w),
                Err(e) => format!("cannot build the report: {e}"),
            };
            app.reports.preview = Some((w, text.lines().map(str::to_string).collect()));
        }
    }
    let st = &app.reports;
    let mut v = match &st.screen {
        RScreen::List => {
            let mut v = View::new("Reports");
            for (i, rep) in st.list.iter().enumerate() {
                let n = app.store.sections(&rep.id).map(|v| v.len()).unwrap_or(0);
                let desc = if rep.description.is_empty() { String::new() } else { format!("  {}", rep.description) };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {} ", rep.name), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {n} sections"), Style::new().fg(t.accent)),
                    Span::styled(desc, Style::new().fg(t.muted)),
                ]));
            }
            if st.list.is_empty() {
                v.head.push(muted(&t, " no reports yet: n creates one"));
            }
            v.sel = (!st.list.is_empty()).then_some(st.sel);
            v.keys = vec![
                ("Enter open", KeyCode::Enter),
                ("n new", KeyCode::Char('n')),
                ("v preview", KeyCode::Char('v')),
                ("E export", KeyCode::Char('E')),
                ("p to project", KeyCode::Char('p')),
                ("r rename", KeyCode::Char('r')),
                ("d delete", KeyCode::Char('d')),
                ("Esc", KeyCode::Esc),
            ];
            v
        }
        RScreen::Report(id) => {
            let rep = app.store.report(id).ok().flatten();
            let mut v = View::new(format!("Report {}", rep.as_ref().map(|r| r.name.clone()).unwrap_or_default()));
            if let Some(rep) = &rep
                && !rep.description.is_empty()
            {
                v.head.push(muted(&t, format!(" {}", rep.description)));
            }
            v.head.push(muted(&t, " sections read the data, journals and projects as they are when exported"));
            v.head.push(Line::raw(""));
            for (i, s) in st.sections.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:>2}. {} ", s.position, s.heading()), row_style(&t, i == st.ssel)),
                    Span::styled(format!("  {}", section_summary(&app.store, s)), Style::new().fg(t.muted)),
                ]));
            }
            if st.sections.is_empty() {
                v.head.push(muted(&t, " empty: a adds a section"));
            }
            v.sel = (!st.sections.is_empty()).then_some(st.ssel);
            v.keys = vec![
                ("a add", KeyCode::Char('a')),
                ("Enter edit", KeyCode::Enter),
                ("t title", KeyCode::Char('t')),
                ("J/K move", KeyCode::Char('J')),
                ("d remove", KeyCode::Char('d')),
                ("v preview", KeyCode::Char('v')),
                ("E export", KeyCode::Char('E')),
                ("p to project", KeyCode::Char('p')),
                ("Esc back", KeyCode::Esc),
            ];
            v
        }
        RScreen::Kinds(_) => {
            let mut v = View::new("Add a section");
            for (i, k) in SECTION_KINDS.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {:<8} ", k.as_str()), row_style(&t, i == st.ksel)),
                    Span::styled(format!("  {}", k.describe()), Style::new().fg(t.muted)),
                ]));
            }
            v.head.push(muted(&t, " records use the series selected in the sidebar and the current time range"));
            v.sel = Some(st.ksel);
            v.keys = vec![("Enter choose", KeyCode::Enter), ("Esc back", KeyCode::Esc)];
            v
        }
        RScreen::Choose(_, kind) => {
            let mut v = View::new(format!("Add a {} section", kind.as_str()));
            for (i, (label, _)) in st.choices.iter().enumerate() {
                v.rows.push(Line::styled(format!(" {label}"), row_style(&t, i == st.csel)));
            }
            v.sel = Some(st.csel);
            v.keys = vec![("Enter choose", KeyCode::Enter), ("Esc back", KeyCode::Esc)];
            v
        }
        RScreen::Edit => {
            let Some(ed) = st.editor.clone() else { return };
            let mut v = View::new(if ed.section.is_some() { "Edit text section" } else { "New text section" });
            let field = |on: bool| if on { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new().fg(t.fg) };
            v.head.push(Line::from(vec![
                Span::styled(" Title  ", Style::new().fg(t.muted)),
                Span::styled(format!("{}{}", ed.title, if ed.field == 0 { "▏" } else { "" }), field(ed.field == 0)),
            ]));
            v.head.push(muted(&t, format!(" Format {} (Ctrl-T switches)", ed.format.as_str())));
            v.head.push(Line::raw(""));
            let n = ed.body.split('\n').count();
            for (i, l) in ed.body.split('\n').enumerate() {
                let cursor = if ed.field == 1 && i + 1 == n { "▏" } else { "" };
                v.head.push(Line::styled(format!(" {l}{cursor}"), field(ed.field == 1)));
            }
            let avail = r.height.saturating_sub(4) as usize;
            v.head_scroll = v.head.len().saturating_sub(avail);
            v.keys = vec![
                ("Ctrl-S save", KeyCode::F(30)),
                ("Tab title/text", KeyCode::Tab),
                ("Ctrl-T format", KeyCode::F(31)),
                ("Esc cancel", KeyCode::Esc),
            ];
            v
        }
        RScreen::Preview(id) => {
            let name = app.store.report(id).ok().flatten().map(|r| r.name).unwrap_or_default();
            let mut v = View::new(format!("Preview {name}"));
            let lines = st.preview.as_ref().map(|p| p.1.clone()).unwrap_or_default();
            let avail = r.height.saturating_sub(4) as usize;
            let scroll = st.pscroll.min(lines.len().saturating_sub(avail));
            v.head = lines.into_iter().map(|l| Line::raw(format!(" {l}"))).collect();
            v.head_scroll = scroll;
            v.keys = vec![("j/k scroll", KeyCode::Down), ("E export", KeyCode::Char('E')), ("Esc back", KeyCode::Esc)];
            v
        }
    };
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    super::share::add_key(app, &mut v.keys);
    draw_view(f, r, v, &t, hits);
}
