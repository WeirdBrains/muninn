//! Export a journal, entry, report or project as a document (E in Journals, Reports and
//! Projects): choose a format, then where to write it.
use super::workspace::{Ask, Typed, View, ask_key, draw_view, muted, row_style};
use super::{App, Hit};
use crate::export::files::{OutFormat, Target, write_recorded};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// The window to return to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum From {
    #[default]
    Close,
    Journals,
    Reports,
    Projects,
}

#[derive(Debug, Default)]
pub struct DocExport {
    pub target: Option<Target>,
    pub title: String,
    pub formats: Vec<OutFormat>,
    pub sel: usize,
    pub ask: Option<Ask<OutFormat>>,
    pub msg: Option<(String, bool)>,
    pub from: From,
}

impl DocExport {
    pub fn typing(&self) -> bool {
        self.ask.is_some()
    }
}

/// Open the export window for `target`.
pub fn open(app: &mut App, target: Target, title: impl Into<String>, from: From) {
    let mut formats = target.formats();
    // A project is exported whole first of all.
    if let Some(i) = formats.iter().position(|f| *f == OutFormat::Archive) {
        let a = formats.remove(i);
        formats.insert(0, a);
    }
    app.doc_export = DocExport { formats, target: Some(target), title: title.into(), from, ..Default::default() };
    app.modal = Some(super::modal::Modal::DocExport);
}

fn back(app: &mut App) {
    match app.doc_export.from {
        From::Close => app.modal = None,
        From::Journals => super::journals::reopen(app),
        From::Reports => super::reports::reopen(app),
        From::Projects => app.modal = Some(super::modal::Modal::Projects),
    }
}

/// The suggested path: `<cwd>/<name>.<ext>`.
fn suggest(app: &App, f: OutFormat) -> String {
    let Some(t) = &app.doc_export.target else { return String::new() };
    let dir = std::env::current_dir().unwrap_or_default();
    dir.join(t.file_name(&app.store, f)).display().to_string()
}

fn choose(app: &mut App) {
    let Some(&f) = app.doc_export.formats.get(app.doc_export.sel) else { return };
    let v = suggest(app, f);
    app.doc_export.ask = Some(Ask::new(f, format!("Write {} to", f.as_str()), v));
}

fn write(app: &mut App, f: OutFormat, path: String) {
    let Some(t) = app.doc_export.target.clone() else { return };
    if path.is_empty() {
        app.doc_export.msg = Some(("give a file name".into(), true));
        return;
    }
    let path = std::path::PathBuf::from(super::journals::shellexpand(&path));
    match write_recorded(&app.store, &t, f, &path, "tui") {
        Ok(files) => {
            app.refresh_history(true);
            let extra = if files.len() > 1 { format!(" (+{} images)", files.len() - 1) } else { String::new() };
            let m = format!("wrote {}{extra}", path.display());
            app.set_status(m.clone());
            app.doc_export.msg = Some((m, false));
        }
        Err(e) => app.doc_export.msg = Some((e.to_string(), true)),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(a) = app.doc_export.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => app.doc_export.ask = None,
            Typed::Done(v) => {
                let f = a.kind;
                app.doc_export.ask = None;
                write(app, f, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    let st = &mut app.doc_export;
    let n = st.formats.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Enter => choose(app),
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('q') => back(app),
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    if app.doc_export.ask.is_some() {
        return;
    }
    if app.doc_export.sel == i {
        choose(app);
    } else if i < app.doc_export.formats.len() {
        app.doc_export.sel = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.doc_export;
    let mut v = View::new(format!("Export {}", st.title));
    v.head.push(muted(&t, " made inside Muninn: no TeX or other tools needed"));
    v.head.push(Line::raw(""));
    for (i, fm) in st.formats.iter().enumerate() {
        v.rows.push(Line::from(vec![
            Span::styled(format!(" {} ", fm.label()), row_style(&t, i == st.sel)),
            Span::styled(format!("  .{}", fm.ext()), Style::new().fg(t.muted)),
        ]));
    }
    v.sel = Some(st.sel);
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    v.keys = vec![("Enter choose", KeyCode::Enter), ("Esc back", KeyCode::Esc)];
    draw_view(f, r, v, &t, hits);
}
