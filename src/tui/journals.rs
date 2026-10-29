//! Projects ▸ Journals…: journals, entries (Markdown or LaTeX) with images, notes on the
//! journal, an entry or selected words, and highlights.
use super::workspace::{Ask, Back, Purpose, Typed, View, ask_key, draw_view, muted, pick_project, record, row_style};
use super::{App, Hit};
use crate::history::author_label;
use crate::ingest::time::fmt_ts;
use crate::store::{Annotation, AnnotationKind, Attachment, Entry, EntryFormat, ItemKind, Journal, LiveSearch, Span as Sel};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum JScreen {
    #[default]
    List,
    /// A journal's entries.
    Journal(String),
    /// One entry: text, highlights, notes, images.
    Entry(String),
    /// Writing or editing an entry.
    Edit,
    /// Entries a journal live search found.
    Found,
}

#[derive(Debug, Clone)]
pub struct Editor {
    /// None: a new entry.
    pub entry: Option<String>,
    pub journal: String,
    pub title: String,
    pub body: String,
    pub format: EntryFormat,
    /// 0 title, 1 text.
    pub field: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JAsk {
    NewJournal,
    Rename,
    JournalNote,
    Note,
    EditNote,
    Attach,
    CopyNew,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    Journal(String),
    Entry(String),
    Annotation(String),
}

/// Choose the journal selected entries are added to.
#[derive(Debug, Clone, Default)]
pub struct CopyPick {
    /// (label, journal id; None = new journal)
    pub items: Vec<(String, Option<String>)>,
    pub sel: usize,
    pub ids: Vec<String>,
}

#[derive(Debug, Default)]
pub struct JournalsState {
    pub screen: JScreen,
    pub list: Vec<Journal>,
    pub sel: usize,
    pub entries: Vec<Entry>,
    pub esel: usize,
    /// Marked entries (Space) for copying.
    pub marks: BTreeSet<String>,
    /// Notes on the open journal.
    pub notes: Vec<Annotation>,
    pub entry: Option<Entry>,
    pub anns: Vec<Annotation>,
    pub atts: Vec<Attachment>,
    /// Word under the cursor in the entry view.
    pub cursor: usize,
    /// Selection start (word index); the selection runs to the cursor.
    pub anchor: Option<usize>,
    pub asel: usize,
    pub scroll: usize,
    pub editor: Option<Editor>,
    pub found: Vec<(Journal, Entry)>,
    pub found_title: String,
    pub fsel: usize,
    pub entry_from_found: bool,
    pub ask: Option<Ask<JAsk>>,
    pub pick: Option<CopyPick>,
    pub confirm: Option<Confirm>,
    pub msg: Option<(String, bool)>,
    /// Open this journal when the popup comes back (after a fork).
    pub focus_journal: Option<String>,
}

impl JournalsState {
    pub fn typing(&self) -> bool {
        self.ask.is_some() || self.screen == JScreen::Edit
    }
    fn journal_id(&self) -> Option<String> {
        match &self.screen {
            JScreen::Journal(id) => Some(id.clone()),
            JScreen::Entry(_) | JScreen::Edit => {
                self.entry.as_ref().map(|e| e.journal_id.clone()).or_else(|| self.editor.as_ref().map(|e| e.journal.clone()))
            }
            _ => self.list.get(self.sel).map(|j| j.id.clone()),
        }
    }
    /// The selection as a character span of the entry text.
    pub fn selection(&self) -> Option<Sel> {
        let e = self.entry.as_ref()?;
        let w = words(&e.body);
        if w.is_empty() {
            return None;
        }
        let c = self.cursor.min(w.len() - 1);
        let a = self.anchor.unwrap_or(c).min(w.len() - 1);
        Some(Sel { start: w[a.min(c)].0, end: w[a.max(c)].1 })
    }
}

/// Character spans of the words (runs of non-space) of a text.
pub fn words(body: &str) -> Vec<(usize, usize)> {
    let mut v = vec![];
    let mut start = None;
    for (i, c) in body.chars().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                v.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        v.push((s, body.chars().count()));
    }
    v
}

pub fn open(app: &mut App) {
    let st = &mut app.journals;
    st.screen = JScreen::List;
    st.ask = None;
    st.pick = None;
    st.confirm = None;
    st.msg = None;
    st.editor = None;
    reload(app);
    app.modal = Some(super::modal::Modal::Journals);
}

/// Projects ▸ New journal…
pub fn open_new(app: &mut App) {
    open(app);
    app.journals.ask = Some(Ask::new(JAsk::NewJournal, "New journal name", ""));
}

pub fn open_journal(app: &mut App, id: &str) {
    open(app);
    app.journals.screen = JScreen::Journal(id.to_string());
    app.journals.esel = 0;
    app.journals.marks.clear();
    reload(app);
}

/// Show what a journal live search finds now.
pub fn open_found(app: &mut App, s: &LiveSearch) {
    open(app);
    let st = &mut app.journals;
    st.found = crate::live::entry_matches(&app.store, &s.spec).unwrap_or_default();
    st.found_title = format!("Live search {} · {}", s.name, s.spec.describe());
    st.fsel = 0;
    st.screen = JScreen::Found;
    let n = st.found.len();
    app.set_status(format!("live search {}: {n} entries", s.name));
}

/// Back from the project picker.
pub fn reopen(app: &mut App) {
    if let Some(id) = app.journals.focus_journal.take() {
        let msg = app.journals.msg.take();
        open_journal(app, &id);
        app.journals.msg = msg;
        return;
    }
    reload(app);
    app.modal = Some(super::modal::Modal::Journals);
}

fn reload(app: &mut App) {
    let s = &app.store;
    let st = &mut app.journals;
    st.list = s.journals().unwrap_or_default();
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
    match st.screen.clone() {
        JScreen::Journal(id) => {
            st.entries = s.entries(&id).unwrap_or_default();
            st.notes = s.annotations(&id).unwrap_or_default().into_iter().filter(|a| a.entry_id.is_none()).collect();
            st.esel = st.esel.min(st.entries.len().saturating_sub(1));
            let ids: BTreeSet<String> = st.entries.iter().map(|e| e.id.clone()).collect();
            st.marks.retain(|m| ids.contains(m));
        }
        JScreen::Entry(id) => {
            st.entry = s.entry(&id).ok().flatten();
            st.anns = s.entry_annotations(&id).unwrap_or_default();
            st.atts = s.attachments(&id).unwrap_or_default();
            st.asel = st.asel.min(st.anns.len().saturating_sub(1));
            let n = st.entry.as_ref().map(|e| words(&e.body).len()).unwrap_or(0);
            st.cursor = st.cursor.min(n.saturating_sub(1));
        }
        _ => {}
    }
}

fn jparams(j: &Journal) -> serde_json::Value {
    serde_json::json!({ "journal": j.id, "name": j.name })
}

fn eparams(e: &Entry) -> serde_json::Value {
    serde_json::json!({ "journal": e.journal_id, "entry": e.id, "title": e.heading() })
}

fn err(app: &mut App, e: impl std::fmt::Display) {
    app.journals.msg = Some((e.to_string(), true));
}

fn ok(app: &mut App, m: impl Into<String>) {
    let m = m.into();
    app.set_status(m.clone());
    app.journals.msg = Some((m, false));
}

fn open_entry(app: &mut App, id: &str, from_found: bool) {
    let st = &mut app.journals;
    st.screen = JScreen::Entry(id.to_string());
    st.entry_from_found = from_found;
    st.cursor = 0;
    st.anchor = None;
    st.asel = 0;
    st.scroll = 0;
    reload(app);
}

fn edit_entry(app: &mut App, e: Option<Entry>) {
    let st = &mut app.journals;
    let Some(jid) = st.journal_id() else { return };
    let jformat = st.list.iter().find(|j| j.id == jid).map(|j| j.format).unwrap_or_default();
    st.editor = Some(match e {
        Some(e) => Editor { entry: Some(e.id.clone()), journal: jid, title: e.title.clone(), body: e.body.clone(), format: e.format, field: 1 },
        None => Editor { entry: None, journal: jid, title: String::new(), body: String::new(), format: jformat, field: 0 },
    });
    st.screen = JScreen::Edit;
}

fn save_editor(app: &mut App) {
    let Some(ed) = app.journals.editor.clone() else { return };
    let res = match &ed.entry {
        None => app.store.add_entry(&ed.journal, &ed.title, &ed.body, Some(ed.format)).map(|e| (e, true)),
        Some(id) => app.store.update_entry(id, Some(&ed.title), Some(&ed.body), Some(ed.format)).map(|e| (e, false)),
    };
    match res {
        Ok((e, new)) => {
            let jname = app.journals.list.iter().find(|j| j.id == e.journal_id).map(|j| j.name.clone()).unwrap_or_default();
            if new {
                record(app, "entry.add", format!("write entry {} in {jname}", e.heading()), eparams(&e));
            } else {
                record(app, "entry.edit", format!("edit entry {}", e.heading()), eparams(&e));
            }
            app.journals.editor = None;
            if new {
                app.journals.screen = JScreen::Journal(e.journal_id.clone());
                reload(app);
                app.journals.esel = app.journals.entries.iter().position(|x| x.id == e.id).unwrap_or(0);
                ok(app, format!("wrote entry {}", e.heading()));
            } else {
                open_entry(app, &e.id, app.journals.entry_from_found);
                let lost = app.journals.anns.iter().filter(|a| a.detached()).count();
                if lost > 0 {
                    ok(app, format!("saved; {lost} note(s)/highlight(s) lost their words and are kept detached"));
                } else {
                    ok(app, format!("saved entry {}", e.heading()));
                }
            }
        }
        Err(e) => err(app, e),
    }
}

fn editor_key(app: &mut App, k: KeyEvent) {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let Some(ed) = app.journals.editor.as_mut() else { return };
    match k.code {
        KeyCode::Char('s') if ctrl => save_editor(app),
        KeyCode::F(30) => save_editor(app),
        KeyCode::Char('t') if ctrl => ed.format = ed.format.toggle(),
        KeyCode::F(31) => ed.format = ed.format.toggle(),
        KeyCode::Esc => {
            let back = match &ed.entry {
                Some(id) => JScreen::Entry(id.clone()),
                None => JScreen::Journal(ed.journal.clone()),
            };
            app.journals.editor = None;
            app.journals.screen = back;
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

fn answer(app: &mut App, kind: JAsk, v: String) {
    let st = &app.journals;
    match kind {
        JAsk::NewJournal => match app.store.create_journal(&v, None, Default::default()) {
            Ok(j) => {
                record(app, "journal.create", format!("create journal {}", j.name), jparams(&j));
                open_journal(app, &j.id);
                ok(app, format!("created journal {}: w writes the first entry", j.name));
            }
            Err(e) => err(app, e),
        },
        JAsk::Rename => {
            let Some(id) = st.journal_id() else { return };
            match app.store.update_journal(&id, Some(&v), None) {
                Ok(j) => {
                    record(app, "journal.edit", format!("rename journal → {}", j.name), jparams(&j));
                    reload(app);
                    ok(app, format!("renamed to {}", j.name));
                }
                Err(e) => err(app, e),
            }
        }
        JAsk::JournalNote | JAsk::Note => {
            let Some(jid) = st.journal_id() else { return };
            let (entry, span) = match (kind, &st.entry) {
                (JAsk::Note, Some(e)) => (Some(e.id.clone()), if st.anchor.is_some() { st.selection() } else { None }),
                _ => (None, None),
            };
            match app.store.add_note(&jid, entry.as_deref(), span, &v) {
                Ok(a) => {
                    let what = note_label(&a);
                    record(app, "note.add", format!("add {what}"), serde_json::json!({ "journal": a.journal_id, "entry": a.entry_id, "note": a.id }));
                    app.journals.anchor = None;
                    reload(app);
                    ok(app, format!("added {what}"));
                }
                Err(e) => err(app, e),
            }
        }
        JAsk::EditNote => {
            let Some(a) = st.anns.get(st.asel).cloned() else { return };
            match app.store.update_annotation(&a.id, &v) {
                Ok(a) => {
                    record(app, "note.edit", format!("edit {}", note_label(&a)), serde_json::json!({ "journal": a.journal_id, "note": a.id }));
                    reload(app);
                    ok(app, "saved");
                }
                Err(e) => err(app, e),
            }
        }
        JAsk::Attach => {
            let Some(e) = st.entry.clone() else { return };
            let path = std::path::PathBuf::from(shellexpand(&v));
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
            match std::fs::read(&path)
                .map_err(|x| format!("{}: {x}", path.display()))
                .and_then(|d| app.store.attach_image(&e.id, &name, &d).map_err(|x| x.to_string()))
            {
                Ok(a) => {
                    record(
                        app,
                        "image.add",
                        format!("attach image {} to {}", a.name, e.heading()),
                        serde_json::json!({ "entry": e.id, "image": a.id }),
                    );
                    reload(app);
                    ok(app, format!("attached {} ({} bytes); referenced as {}", a.name, a.size, a.reference(e.format)));
                }
                Err(x) => err(app, x),
            }
        }
        JAsk::CopyNew => {
            let project = st.journal_id().and_then(|id| st.list.iter().find(|j| j.id == id).and_then(|j| j.project_id.clone()));
            let format = st.journal_id().and_then(|id| st.list.iter().find(|j| j.id == id).map(|j| j.format)).unwrap_or_default();
            let Some(pick) = app.journals.pick.take() else { return };
            match app.store.create_journal(&v, project.as_deref(), format) {
                Ok(j) => {
                    record(app, "journal.create", format!("create journal {}", j.name), jparams(&j));
                    copy_to(app, &pick.ids, &j);
                }
                Err(e) => err(app, e),
            }
        }
    }
}

/// `~/` at the start of a typed path means the home directory.
fn shellexpand(p: &str) -> String {
    match (p.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(h)) => format!("{h}/{rest}"),
        _ => p.to_string(),
    }
}

fn note_label(a: &Annotation) -> String {
    let what = if a.kind == AnnotationKind::Highlight { "highlight" } else { "note" };
    match &a.quote {
        Some(q) => format!("{what} on \"{}\"", q.chars().take(40).collect::<String>()),
        None if a.entry_id.is_some() => format!("{what} on the entry"),
        None => format!("{what} on the journal"),
    }
}

fn copy_to(app: &mut App, ids: &[String], j: &Journal) {
    match app.store.copy_entries(ids, &j.id) {
        Ok(v) => {
            record(
                app,
                "entry.copy",
                format!("add {} entries to journal {}", v.len(), j.name),
                serde_json::json!({ "journal": j.id, "entries": v.iter().map(|e| &e.id).collect::<Vec<_>>(), "from": ids }),
            );
            app.journals.marks.clear();
            reload(app);
            ok(app, format!("added {} entries to {}", v.len(), j.name));
        }
        Err(e) => err(app, e),
    }
}

fn start_copy(app: &mut App) {
    let st = &mut app.journals;
    let ids: Vec<String> = if st.marks.is_empty() {
        st.entries.get(st.esel).map(|e| vec![e.id.clone()]).unwrap_or_default()
    } else {
        st.entries.iter().filter(|e| st.marks.contains(&e.id)).map(|e| e.id.clone()).collect()
    };
    if ids.is_empty() {
        st.msg = Some(("no entries to copy".into(), true));
        return;
    }
    let cur = st.journal_id();
    let mut items: Vec<(String, Option<String>)> =
        st.list.iter().filter(|j| Some(&j.id) != cur.as_ref()).map(|j| (j.name.clone(), Some(j.id.clone()))).collect();
    items.push(("+ New journal…".into(), None));
    st.pick = Some(CopyPick { items, sel: 0, ids });
}

fn pick_key(app: &mut App, k: KeyEvent) {
    let Some(p) = app.journals.pick.as_mut() else { return };
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => p.sel = (p.sel + 1).min(p.items.len() - 1),
        KeyCode::Up | KeyCode::Char('k') => p.sel = p.sel.saturating_sub(1),
        KeyCode::Esc => app.journals.pick = None,
        KeyCode::Enter => match p.items[p.sel].1.clone() {
            Some(id) => {
                let ids = p.ids.clone();
                app.journals.pick = None;
                if let Ok(Some(j)) = app.store.journal(&id) {
                    copy_to(app, &ids, &j);
                }
            }
            None => app.journals.ask = Some(Ask::new(JAsk::CopyNew, "New journal name", "")),
        },
        _ => {}
    }
}

fn confirmed(app: &mut App, c: Confirm) {
    match c {
        Confirm::Journal(id) => match app.store.remove_journal(&id) {
            Ok(j) => {
                record(app, "journal.remove", format!("remove journal {}", j.name), jparams(&j));
                app.journals.screen = JScreen::List;
                reload(app);
                ok(app, format!("removed journal {}", j.name));
            }
            Err(e) => err(app, e),
        },
        Confirm::Entry(id) => match app.store.remove_entry(&id) {
            Ok(e) => {
                record(app, "entry.remove", format!("remove entry {}", e.heading()), eparams(&e));
                app.journals.screen = JScreen::Journal(e.journal_id.clone());
                reload(app);
                ok(app, format!("removed entry {}", e.heading()));
            }
            Err(e) => err(app, e),
        },
        Confirm::Annotation(id) => match app.store.remove_annotation(&id) {
            Ok(a) => {
                let kind = if a.kind == AnnotationKind::Highlight { "highlight.remove" } else { "note.remove" };
                record(app, kind, format!("remove {}", note_label(&a)), serde_json::json!({ "journal": a.journal_id, "id": a.id }));
                reload(app);
                ok(app, format!("removed {}", note_label(&a)));
            }
            Err(e) => err(app, e),
        },
    }
}

fn ask_confirm(app: &mut App, c: Confirm, what: String) {
    app.journals.msg = Some((format!("delete {what}? y confirms, any other key cancels"), false));
    app.journals.confirm = Some(c);
}

fn highlight(app: &mut App) {
    let st = &app.journals;
    let (Some(e), Some(span)) = (st.entry.clone(), st.selection()) else { return err(app, "nothing to highlight") };
    match app.store.add_highlight(&e.id, span, Some("yellow"), "") {
        Ok(a) => {
            let what = note_label(&a);
            record(
                app,
                "highlight.add",
                format!("add {what}"),
                serde_json::json!({ "journal": a.journal_id, "entry": a.entry_id, "highlight": a.id }),
            );
            app.journals.anchor = None;
            reload(app);
            ok(app, format!("added {what}"));
        }
        Err(x) => err(app, x),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.journals.screen == JScreen::Edit {
        return editor_key(app, k);
    }
    if let Some(a) = app.journals.ask.as_mut() {
        match ask_key(a, k) {
            Typed::Cancel => {
                app.journals.ask = None;
                app.journals.pick = None;
            }
            Typed::Done(v) => {
                let kind = a.kind;
                app.journals.ask = None;
                answer(app, kind, v);
            }
            Typed::Editing => {}
        }
        return;
    }
    if let Some(c) = app.journals.confirm.take() {
        if matches!(k.code, KeyCode::Char('y' | 'Y')) {
            confirmed(app, c);
        } else {
            app.journals.msg = Some(("delete cancelled".into(), false));
        }
        return;
    }
    if app.journals.pick.is_some() {
        return pick_key(app, k);
    }
    app.journals.msg = None;
    match app.journals.screen.clone() {
        JScreen::List => list_key(app, k),
        JScreen::Journal(id) => journal_key(app, k, &id),
        JScreen::Entry(_) => entry_key(app, k),
        JScreen::Found => found_key(app, k),
        JScreen::Edit => {}
    }
}

fn list_key(app: &mut App, k: KeyEvent) {
    let st = &mut app.journals;
    let n = st.list.len();
    let sel = st.list.get(st.sel).cloned();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Char('n') | KeyCode::Char('a') => st.ask = Some(Ask::new(JAsk::NewJournal, "New journal name", "")),
        KeyCode::Enter | KeyCode::Right => {
            if let Some(j) = sel {
                open_journal(app, &j.id);
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {
            if let Some(j) = sel {
                journal_common(app, k, &j);
            }
        }
    }
}

/// Keys that act on a journal from the list or from inside it.
fn journal_common(app: &mut App, k: KeyEvent, j: &Journal) {
    match k.code {
        KeyCode::Char('r') => app.journals.ask = Some(Ask::new(JAsk::Rename, "Rename journal to", j.name.clone())),
        KeyCode::Char('D') | KeyCode::Delete => {
            let n = app.store.entries(&j.id).map(|v| v.len()).unwrap_or(0);
            ask_confirm(app, Confirm::Journal(j.id.clone()), format!("journal {} with its {n} entries", j.name));
        }
        KeyCode::Char('f') => pick_project(app, Purpose::ForkJournal(j.id.clone(), j.name.clone()), Back::Journals),
        KeyCode::Char('p') => pick_project(app, Purpose::Add(ItemKind::Journal, j.id.clone(), serde_json::json!({}), j.name.clone()), Back::Journals),
        KeyCode::Char('N') => app.journals.ask = Some(Ask::new(JAsk::JournalNote, format!("Note on journal {}", j.name), "")),
        KeyCode::Char('t') => match app.store.update_journal(&j.id, None, Some(j.format.toggle())) {
            Ok(n) => {
                record(app, "journal.edit", format!("journal {} writes {} now", n.name, n.format.as_str()), jparams(&n));
                reload(app);
                ok(app, format!("new entries in {} are {}", n.name, n.format.as_str()));
            }
            Err(e) => err(app, e),
        },
        _ => {}
    }
}

fn journal_key(app: &mut App, k: KeyEvent, id: &str) {
    let st = &mut app.journals;
    let n = st.entries.len();
    let cur = st.entries.get(st.esel).cloned();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.esel = (st.esel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.esel = st.esel.saturating_sub(1),
        KeyCode::Char(' ') => {
            if let Some(e) = cur {
                if !st.marks.remove(&e.id) {
                    st.marks.insert(e.id);
                }
                st.esel = (st.esel + 1).min(n.saturating_sub(1));
            }
        }
        KeyCode::Enter | KeyCode::Right => {
            if let Some(e) = cur {
                open_entry(app, &e.id, false);
            }
        }
        KeyCode::Char('w') | KeyCode::Char('n') => edit_entry(app, None),
        KeyCode::Char('e') => {
            if let Some(e) = cur {
                edit_entry(app, Some(e));
            }
        }
        KeyCode::Char('d') => {
            if let Some(e) = cur {
                ask_confirm(app, Confirm::Entry(e.id.clone()), format!("entry {}", e.heading()));
            }
        }
        KeyCode::Char('c') => start_copy(app),
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
            st.screen = JScreen::List;
            st.sel = st.list.iter().position(|j| j.id == id).unwrap_or(0);
            reload(app);
        }
        KeyCode::Char('q') => app.modal = None,
        _ => {
            if let Ok(Some(j)) = app.store.journal(id) {
                journal_common(app, k, &j);
            }
        }
    }
}

fn entry_key(app: &mut App, k: KeyEvent) {
    let st = &mut app.journals;
    let Some(e) = st.entry.clone() else { return };
    let nw = words(&e.body).len();
    match k.code {
        KeyCode::Right | KeyCode::Char('l') => st.cursor = (st.cursor + 1).min(nw.saturating_sub(1)),
        KeyCode::Left | KeyCode::Char('h') => st.cursor = st.cursor.saturating_sub(1),
        KeyCode::Char('w') => st.cursor = (st.cursor + 5).min(nw.saturating_sub(1)),
        KeyCode::Char('b') => st.cursor = st.cursor.saturating_sub(5),
        KeyCode::Home => st.cursor = 0,
        KeyCode::End => st.cursor = nw.saturating_sub(1),
        KeyCode::Char('v') => st.anchor = if st.anchor.is_some() { None } else { Some(st.cursor) },
        KeyCode::Down | KeyCode::Char('j') => st.asel = (st.asel + 1).min(st.anns.len().saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.asel = st.asel.saturating_sub(1),
        KeyCode::PageDown => st.scroll += 5,
        KeyCode::PageUp => st.scroll = st.scroll.saturating_sub(5),
        KeyCode::Char('m') => highlight(app),
        KeyCode::Char('n') => {
            let title = match st.selection().filter(|_| st.anchor.is_some()) {
                Some(s) => {
                    format!("Note on \"{}\"", crate::store::journals::chars_slice(&e.body, s.start, s.end).chars().take(30).collect::<String>())
                }
                None => format!("Note on entry {}", e.heading()),
            };
            st.ask = Some(Ask::new(JAsk::Note, title, ""));
        }
        KeyCode::Char('N') => st.ask = Some(Ask::new(JAsk::JournalNote, "Note on the journal", "")),
        KeyCode::Enter => {
            if let Some(a) = st.anns.get(st.asel) {
                let v = a.body.clone();
                st.ask = Some(Ask::new(JAsk::EditNote, if a.kind == AnnotationKind::Highlight { "Highlight comment" } else { "Note" }, v));
            }
        }
        KeyCode::Char('x') | KeyCode::Delete => {
            if let Some(a) = st.anns.get(st.asel).cloned() {
                ask_confirm(app, Confirm::Annotation(a.id.clone()), note_label(&a));
            }
        }
        KeyCode::Char('i') => st.ask = Some(Ask::new(JAsk::Attach, "Attach image (path)", "")),
        KeyCode::Char('e') => edit_entry(app, Some(e)),
        KeyCode::Char('d') => ask_confirm(app, Confirm::Entry(e.id.clone()), format!("entry {}", e.heading())),
        KeyCode::Esc if st.anchor.is_some() => st.anchor = None,
        KeyCode::Esc | KeyCode::Backspace => {
            st.screen = if st.entry_from_found { JScreen::Found } else { JScreen::Journal(e.journal_id.clone()) };
            st.entry = None;
            reload(app);
            app.journals.esel = app.journals.entries.iter().position(|x| x.id == e.id).unwrap_or(0);
        }
        KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

fn found_key(app: &mut App, k: KeyEvent) {
    let st = &mut app.journals;
    let n = st.found.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.fsel = (st.fsel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.fsel = st.fsel.saturating_sub(1),
        KeyCode::Enter => {
            if let Some((_, e)) = st.found.get(st.fsel).cloned() {
                open_entry(app, &e.id, true);
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

pub fn on_click(app: &mut App, i: usize) {
    let st = &mut app.journals;
    if st.ask.is_some() || st.screen == JScreen::Edit {
        return;
    }
    if let Some(p) = st.pick.as_mut() {
        if p.sel == i {
            pick_key(app, KeyEvent::from(KeyCode::Enter));
        } else if i < p.items.len() {
            p.sel = i;
        }
        return;
    }
    let (cur, n) = match st.screen {
        JScreen::List => (&mut st.sel, st.list.len()),
        JScreen::Journal(_) => (&mut st.esel, st.entries.len()),
        JScreen::Entry(_) => (&mut st.asel, st.anns.len()),
        JScreen::Found => (&mut st.fsel, st.found.len()),
        JScreen::Edit => return,
    };
    if *cur == i {
        on_key(app, KeyEvent::from(KeyCode::Enter));
    } else if i < n {
        *cur = i;
    }
}

fn ann_line<'a>(app: &App, a: &Annotation, on: bool) -> Line<'a> {
    let t = &app.theme;
    let icon = if a.kind == AnnotationKind::Highlight { "◆" } else { "✎" };
    let mut spans = vec![Span::styled(format!(" {icon} {} ", note_label(a)), row_style(t, on))];
    if !a.body.is_empty() {
        spans.push(Span::styled(format!(" {}", a.body), Style::new().fg(t.fg)));
    }
    if a.detached() {
        spans.push(Span::styled("  (words no longer in the entry)", Style::new().fg(t.warning)));
    }
    spans.push(Span::styled(format!("  · {}", author_label(&app.store, a.author_id.as_deref())), Style::new().fg(t.muted)));
    Line::from(spans)
}

/// The entry text with highlights, noted words, the selection and the cursor word marked,
/// wrapped to `width` characters.
fn body_lines<'a>(app: &App, e: &Entry, width: usize) -> Vec<Line<'a>> {
    let t = &app.theme;
    let st = &app.journals;
    let chars: Vec<char> = e.body.chars().collect();
    let mut styles = vec![Style::new().fg(t.fg); chars.len()];
    for a in &st.anns {
        let Some(s) = a.span() else { continue };
        let add = if a.kind == AnnotationKind::Highlight {
            Style::new().bg(t.warning).fg(t.bg)
        } else {
            Style::new().fg(t.info).add_modifier(Modifier::UNDERLINED)
        };
        for st in styles.iter_mut().take(s.1.min(chars.len())).skip(s.0) {
            *st = st.patch(add);
        }
    }
    let w = words(&e.body);
    if let Some(sel) = st.selection().filter(|_| st.anchor.is_some()) {
        for s in styles.iter_mut().take(sel.end.min(chars.len())).skip(sel.start) {
            *s = s.patch(Style::new().bg(t.selection_bg).fg(t.selection_fg));
        }
    }
    if let Some(&(a, b)) = w.get(st.cursor.min(w.len().saturating_sub(1))) {
        for s in styles.iter_mut().take(b).skip(a) {
            *s = s.patch(Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED));
        }
    }
    let width = width.max(10);
    let mut lines = vec![];
    let mut cur: Vec<Span> = vec![];
    let mut col = 0;
    for (c, s) in chars.iter().zip(styles) {
        if *c == '\n' || col >= width {
            lines.push(Line::from(std::mem::take(&mut cur)));
            col = 0;
            if *c == '\n' {
                continue;
            }
        }
        cur.push(Span::styled(c.to_string(), s));
        col += 1;
    }
    lines.push(Line::from(cur));
    lines
        .into_iter()
        .map(|mut l| {
            l.spans.insert(0, Span::raw(" "));
            l
        })
        .collect()
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.journals;
    let proj = |id: &Option<String>| id.as_ref().and_then(|p| app.store.project(p).ok().flatten()).map(|p| p.name);
    let mut v = match &st.screen {
        JScreen::List => {
            let mut v = View::new("Journals");
            for (i, j) in st.list.iter().enumerate() {
                let n = app.store.entries(&j.id).map(|v| v.len()).unwrap_or(0);
                let p = proj(&j.project_id).map(|p| format!("  project {p}")).unwrap_or_else(|| "  standalone".into());
                let fork = if j.forked_from.is_some() { " ⑂" } else { "" };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {}{fork} ", j.name), row_style(&t, i == st.sel)),
                    Span::styled(format!(" {n} entries"), Style::new().fg(t.accent)),
                    Span::styled(format!("{p} · {}", j.format.as_str()), Style::new().fg(t.muted)),
                ]));
            }
            if st.list.is_empty() {
                v.head.push(muted(&t, " no journals yet: n creates one"));
            }
            v.sel = (!st.list.is_empty()).then_some(st.sel);
            v.keys = vec![
                ("Enter open", KeyCode::Enter),
                ("n new", KeyCode::Char('n')),
                ("r rename", KeyCode::Char('r')),
                ("f fork", KeyCode::Char('f')),
                ("p project", KeyCode::Char('p')),
                ("D delete", KeyCode::Char('D')),
                ("Esc", KeyCode::Esc),
            ];
            v
        }
        JScreen::Journal(id) => {
            let j = st.list.iter().find(|j| &j.id == id).cloned();
            let name = j.as_ref().map(|j| j.name.clone()).unwrap_or_default();
            let mut v = View::new(format!("Journal {name}"));
            if let Some(j) = &j {
                let p = proj(&j.project_id).map(|p| format!("project {p}")).unwrap_or_else(|| "standalone".into());
                v.head
                    .push(muted(&t, format!(" {p} · new entries: {} · by {}", j.format.as_str(), author_label(&app.store, j.author_id.as_deref()))));
            }
            for n in &st.notes {
                v.head.push(Line::styled(format!(" ✎ {} · {}", n.body, author_label(&app.store, n.author_id.as_deref())), Style::new().fg(t.info)));
            }
            v.head.push(Line::raw(""));
            for (i, e) in st.entries.iter().enumerate() {
                let mark = if st.marks.contains(&e.id) { "[x]" } else { "[ ]" };
                let n = app.store.entry_annotations(&e.id).map(|v| v.len()).unwrap_or(0);
                let extra = if n > 0 { format!(" · {n} notes") } else { String::new() };
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {mark} {} ", e.heading()), row_style(&t, i == st.esel)),
                    Span::styled(
                        format!(
                            " {} · {} · {}{extra}",
                            author_label(&app.store, e.author_id.as_deref()),
                            &fmt_ts(e.created_at)[..16],
                            e.format.as_str()
                        ),
                        Style::new().fg(t.muted),
                    ),
                ]));
            }
            if st.entries.is_empty() {
                v.head.push(muted(&t, " no entries yet: w writes one"));
            }
            v.sel = (!st.entries.is_empty()).then_some(st.esel);
            v.keys = vec![
                ("Enter read", KeyCode::Enter),
                ("w write", KeyCode::Char('w')),
                ("e edit", KeyCode::Char('e')),
                ("Space mark", KeyCode::Char(' ')),
                ("c copy to", KeyCode::Char('c')),
                ("N note", KeyCode::Char('N')),
                ("f fork", KeyCode::Char('f')),
                ("p project", KeyCode::Char('p')),
                ("d delete", KeyCode::Char('d')),
                ("Esc back", KeyCode::Esc),
            ];
            v
        }
        JScreen::Entry(_) => {
            let Some(e) = st.entry.clone() else { return };
            let mut v = View::new(format!("Entry {}", e.heading()));
            v.head.push(muted(
                &t,
                format!(
                    " {} · {} · {}{}",
                    author_label(&app.store, e.author_id.as_deref()),
                    fmt_ts(e.created_at),
                    e.format.as_str(),
                    if e.forked_from.is_some() { " · copied" } else { "" }
                ),
            ));
            v.head.push(Line::raw(""));
            v.head.extend(body_lines(app, &e, r.width.saturating_sub(4) as usize));
            v.head.push(Line::raw(""));
            for a in &st.atts {
                v.head.push(Line::styled(
                    format!(" ▣ image {} ({}, {} bytes) · {}", a.name, a.mime, a.size, a.reference(e.format)),
                    Style::new().fg(t.accent),
                ));
            }
            v.head_scroll = st.scroll.min(v.head.len().saturating_sub(1));
            for (i, a) in st.anns.iter().enumerate() {
                v.rows.push(ann_line(app, a, i == st.asel));
            }
            v.sel = (!st.anns.is_empty()).then_some(st.asel);
            let sel_on = st.anchor.is_some();
            v.keys = vec![
                ("←→ word", KeyCode::Right),
                (if sel_on { "v end select" } else { "v select" }, KeyCode::Char('v')),
                ("m highlight", KeyCode::Char('m')),
                (if sel_on { "n note words" } else { "n note entry" }, KeyCode::Char('n')),
                ("i image", KeyCode::Char('i')),
                ("e edit", KeyCode::Char('e')),
                ("x remove note", KeyCode::Char('x')),
                ("Esc back", KeyCode::Esc),
            ];
            v
        }
        JScreen::Edit => {
            let Some(ed) = st.editor.clone() else { return };
            let mut v = View::new(if ed.entry.is_some() { "Edit entry" } else { "New entry" });
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
        JScreen::Found => {
            let mut v = View::new(st.found_title.clone());
            for (i, (j, e)) in st.found.iter().enumerate() {
                v.rows.push(Line::from(vec![
                    Span::styled(format!(" {} ", e.heading()), row_style(&t, i == st.fsel)),
                    Span::styled(format!(" {} · {}", j.name, &fmt_ts(e.created_at)[..16]), Style::new().fg(t.muted)),
                ]));
            }
            if st.found.is_empty() {
                v.head.push(muted(&t, " no entries match now"));
            }
            v.sel = (!st.found.is_empty()).then_some(st.fsel);
            v.keys = vec![("Enter read", KeyCode::Enter), ("Esc close", KeyCode::Esc)];
            v
        }
    };
    if let Some(p) = &st.pick {
        v = View::new(format!("Add {} entries to journal", p.ids.len()));
        for (i, (label, _)) in p.items.iter().enumerate() {
            v.rows.push(Line::styled(format!(" {label}"), row_style(&t, i == p.sel)));
        }
        v.sel = Some(p.sel);
        v.keys = vec![("Enter choose", KeyCode::Enter), ("Esc cancel", KeyCode::Esc)];
    }
    v.ask = st.ask.as_ref().map(|a| (a.title.clone(), a.value.clone()));
    v.msg = st.msg.clone();
    draw_view(f, r, v, &t, hits);
}
