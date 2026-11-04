//! File ▸ Authors…: list, add, edit, delete and choose the current author.
use super::widgets::Input;
use super::{App, Hit, block};
use crate::store::authors::parse_details;
use crate::store::{Author, AuthorChange, NewAction};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// The add/edit form: Name, Email, Details (`key=value; key=value`).
#[derive(Debug, Clone)]
pub struct AuthorForm {
    /// None: adding a new author.
    pub id: Option<String>,
    pub fields: [Input; 3],
    pub sel: usize,
}

#[derive(Debug, Default)]
pub struct AuthorsState {
    pub list: Vec<Author>,
    pub current: Option<String>,
    pub sel: usize,
    pub form: Option<AuthorForm>,
    /// Waiting for `y` to delete the selected author.
    pub confirm_delete: bool,
    pub error: Option<String>,
}

impl AuthorsState {
    pub fn typing(&self) -> bool {
        self.form.is_some()
    }
    fn selected(&self) -> Option<&Author> {
        self.list.get(self.sel)
    }
}

fn details_text(a: &Author) -> String {
    a.details.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ")
}

fn form(a: Option<&Author>) -> AuthorForm {
    let f = |l, v: &str| Input::new(l).with(v);
    AuthorForm {
        id: a.map(|a| a.id.clone()),
        fields: [
            f("Name", a.map(|a| a.name.as_str()).unwrap_or("")),
            f("Email", a.and_then(|a| a.email.as_deref()).unwrap_or("")),
            f("Details", &a.map(details_text).unwrap_or_default()),
        ],
        sel: 0,
    }
}

/// Open the popup (reloads the list).
pub fn open(app: &mut App) {
    reload(app);
    app.authors.form = None;
    app.authors.confirm_delete = false;
    app.authors.error = None;
    app.modal = Some(super::modal::Modal::Authors);
}

fn reload(app: &mut App) {
    let st = &mut app.authors;
    st.list = app.store.authors().unwrap_or_default();
    st.current = app.store.current_author().ok().flatten().map(|a| a.id);
    st.sel = st.sel.min(st.list.len().saturating_sub(1));
}

fn record(app: &mut App, kind: &str, summary: String, a: &Author) {
    app.store.note_action(&NewAction::new(kind, "tui", summary).params(serde_json::json!({ "author": a.id, "name": a.name })));
    app.refresh_history(true);
}

fn save(app: &mut App) {
    let Some(fm) = app.authors.form.clone() else { return };
    let items: Vec<String> = fm.fields[2].value.split(';').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
    let res = parse_details(&items).and_then(|details| {
        let name = fm.fields[0].value.trim().to_string();
        let email = fm.fields[1].value.trim().to_string();
        match &fm.id {
            None => app.store.add_author(&name, (!email.is_empty()).then_some(email.as_str()), &details).map(|a| (a, true)),
            Some(id) => app
                .store
                .update_author(id, &AuthorChange { name: Some(name), email: Some(email), details, replace_details: true })
                .map(|a| (a, false)),
        }
    });
    match res {
        Ok((a, added)) => {
            let (kind, verb) = if added { ("author.add", "add") } else { ("author.edit", "edit") };
            record(app, kind, format!("{verb} author {}", a.name), &a);
            app.authors.form = None;
            app.authors.error = None;
            reload(app);
            app.authors.sel = app.authors.list.iter().position(|x| x.id == a.id).unwrap_or(0);
            app.set_status(format!("{} author {}", if added { "added" } else { "saved" }, a.display()));
        }
        Err(e) => app.authors.error = Some(e.to_string()),
    }
}

fn make_current(app: &mut App) {
    let Some(a) = app.authors.selected().cloned() else { return };
    match app.store.set_current_author(Some(&a.id)) {
        Ok(()) => {
            record(app, "author.use", format!("current author {}", a.name), &a);
            reload(app);
            app.set_status(format!("current author: {}", a.display()));
        }
        Err(e) => app.authors.error = Some(e.to_string()),
    }
}

fn delete(app: &mut App) {
    let Some(a) = app.authors.selected().cloned() else { return };
    record(app, "author.remove", format!("remove author {}", a.name), &a);
    match app.store.remove_author(&a.id) {
        Ok(_) => {
            reload(app);
            app.set_status(format!("removed author {}", a.display()));
        }
        Err(e) => app.authors.error = Some(e.to_string()),
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(fm) = app.authors.form.as_mut() {
        match k.code {
            KeyCode::Esc => {
                app.authors.form = None;
                app.authors.error = None;
            }
            KeyCode::Tab | KeyCode::Down => fm.sel = (fm.sel + 1) % 3,
            KeyCode::BackTab | KeyCode::Up => fm.sel = (fm.sel + 2) % 3,
            KeyCode::Enter => save(app),
            c => {
                fm.fields[fm.sel].edit(c);
            }
        }
        return;
    }
    let st = &mut app.authors;
    if st.confirm_delete {
        st.confirm_delete = false;
        if matches!(k.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
            delete(app);
        } else {
            app.set_status("delete cancelled");
        }
        return;
    }
    let n = st.list.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
        KeyCode::Char('a') | KeyCode::Char('n') => {
            st.form = Some(form(None));
            st.error = None;
        }
        KeyCode::Char('e') => {
            if let Some(a) = st.selected() {
                st.form = Some(form(Some(a)));
                st.error = None;
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(name) = st.selected().map(Author::display) {
                st.confirm_delete = true;
                app.set_status(format!("delete author {name}? y to confirm"));
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => make_current(app),
        KeyCode::Char('x') => {
            if app.store.set_current_author(None).is_ok() {
                reload(app);
                app.set_status("no current author");
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
        _ => {}
    }
}

/// A click on row `i`: a list row selects (twice: makes current); a form row focuses that field.
pub fn on_click(app: &mut App, i: usize) {
    if let Some(fm) = app.authors.form.as_mut() {
        fm.sel = i.min(2);
        return;
    }
    if app.authors.sel == i {
        make_current(app);
    } else if i < app.authors.list.len() {
        app.authors.sel = i;
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect, hits: &mut Vec<(Rect, Hit)>) {
    let t = app.theme.clone();
    let st = &app.authors;
    let mut lines = vec![];
    let row_hit = |hits: &mut Vec<(Rect, Hit)>, n: usize| {
        for i in 0..n {
            let y = r.y + 1 + i as u16;
            if y + 3 < r.bottom() {
                hits.push((Rect { x: r.x + 1, y, width: r.width.saturating_sub(2), height: 1 }, Hit::Row(i)));
            }
        }
    };
    let keys: Vec<(&str, KeyCode)>;
    if let Some(fm) = &st.form {
        for (i, inp) in fm.fields.iter().enumerate() {
            let on = i == fm.sel;
            let st = if on { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new().fg(t.fg) };
            let cursor = if on { "▏" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!(" {:<8} ", inp.label), Style::new().fg(t.muted)),
                Span::styled(format!("{}{cursor}", inp.value), st),
            ]));
        }
        lines.push(Line::raw(""));
        lines.push(Line::styled(" Details: key=value; key=value", Style::new().fg(t.muted)));
        row_hit(hits, 3);
        keys = vec![("Enter save", KeyCode::Enter), ("Tab next", KeyCode::Tab), ("Esc cancel", KeyCode::Esc)];
    } else {
        for (i, a) in st.list.iter().enumerate() {
            let cur = st.current.as_deref() == Some(a.id.as_str());
            let s = if i == st.sel { Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD) } else { Style::new().fg(t.fg) };
            let email = a.email.as_deref().map(|e| format!(" <{e}>")).unwrap_or_default();
            let extra = if a.details.is_empty() { String::new() } else { format!("  {}", details_text(a)) };
            lines.push(Line::styled(format!(" {} {}{email}{extra} ", if cur { "★" } else { " " }, a.name), s));
        }
        if st.list.is_empty() {
            lines.push(Line::styled(" no authors yet: a adds one (the first becomes current)", Style::new().fg(t.muted)));
        }
        row_hit(hits, st.list.len());
        keys = vec![
            ("a add", KeyCode::Char('a')),
            ("e edit", KeyCode::Char('e')),
            ("d delete", KeyCode::Char('d')),
            ("Enter current", KeyCode::Enter),
            ("Esc", KeyCode::Esc),
        ];
    }
    if let Some(e) = &st.error {
        lines.push(Line::styled(format!(" {e}"), Style::new().fg(t.error)));
    } else if st.confirm_delete {
        lines.push(Line::styled(" delete the selected author? y confirms, any other key cancels", Style::new().fg(t.warning)));
    }
    let title = match &st.form {
        Some(fm) if fm.id.is_some() => "Edit author",
        Some(_) => "New author",
        None => "Authors · ★ current",
    };
    f.render_widget(Paragraph::new(lines).block(block(title, true, &t)), r);
    let bar = Rect { x: r.x + 2, y: r.bottom().saturating_sub(2), width: r.width.saturating_sub(4), height: 1 };
    super::widgets::key_bar(f, bar, &keys, &t, hits);
}
