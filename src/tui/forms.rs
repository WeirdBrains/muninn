//! Export and Keys & Backup dialogs, plus file preview helpers.
use super::{block, widgets::Input, App};
use crate::export::{self, Format};
use crate::ingest::{self, frame::Columns, time::fmt_ts};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use std::path::PathBuf;

/// Fields followed by action buttons. Up/Down select, Enter edits a field or runs an action, Esc stops editing.
#[derive(Debug, Clone, Default)]
pub struct Form {
    pub fields: Vec<Input>,
    pub actions: Vec<&'static str>,
    pub sel: usize,
    pub editing: bool,
}

pub enum FormEvent {
    None,
    Action(usize),
    Changed,
}

impl Form {
    pub fn new(fields: Vec<Input>, actions: Vec<&'static str>) -> Self {
        Self { fields, actions, sel: 0, editing: false }
    }
    fn len(&self) -> usize {
        self.fields.len() + self.actions.len()
    }
    pub fn key(&mut self, k: KeyEvent) -> FormEvent {
        if self.editing {
            match k.code {
                KeyCode::Esc | KeyCode::Enter => self.editing = false,
                KeyCode::Up => {
                    self.editing = false;
                    self.sel = self.sel.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.editing = false;
                    self.sel = (self.sel + 1).min(self.len() - 1);
                }
                c => {
                    if self.fields[self.sel].edit(c) {
                        return FormEvent::Changed;
                    }
                }
            }
            return FormEvent::None;
        }
        match k.code {
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1).min(self.len() - 1),
            KeyCode::Enter => {
                if self.sel < self.fields.len() {
                    self.editing = true;
                } else {
                    return FormEvent::Action(self.sel - self.fields.len());
                }
            }
            _ => {}
        }
        FormEvent::None
    }
    pub fn focus(&mut self, label: &str) {
        if let Some(i) = self.fields.iter().position(|f| f.label == label) {
            self.sel = i;
            self.editing = true;
        }
    }
    pub fn get(&self, label: &str) -> Option<String> {
        self.fields.iter().find(|f| f.label == label).and_then(|f| f.opt())
    }
    pub fn lines(&self, t: &crate::theme::Theme) -> Vec<Line<'static>> {
        let mut v = vec![];
        for (i, f) in self.fields.iter().enumerate() {
            let sel = i == self.sel;
            let cursor = if sel && self.editing { "▏" } else { "" };
            let st = if sel { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new().fg(t.fg) };
            v.push(Line::from(vec![
                Span::styled(format!("{:>12}: ", f.label), Style::new().fg(t.muted)),
                Span::styled(format!("{}{}", f.shown(), cursor), st),
            ]));
        }
        v.push(Line::from(""));
        let mut acts = vec![Span::raw("              ")];
        for (i, a) in self.actions.iter().enumerate() {
            let sel = self.fields.len() + i == self.sel;
            let st = if sel {
                Style::new().bg(t.accent).fg(t.bg).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.accent)
            };
            acts.push(Span::styled(format!("[ {a} ]"), st));
            acts.push(Span::raw("  "));
        }
        v.push(Line::from(acts));
        v
    }
}

// ---------------- Ingest ----------------

pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(h) = dirs::home_dir() {
            return h.join(rest);
        }
    PathBuf::from(p)
}

pub fn is_json(p: &std::path::Path) -> bool {
    p.extension().is_some_and(|e| matches!(e.to_string_lossy().to_ascii_lowercase().as_str(), "json" | "ndjson" | "jsonl"))
}

pub fn preview(path: &std::path::Path, n: usize) -> crate::Result<Vec<String>> {
    let file = ingest::expand_path(path, &["csv", "tsv", "txt", "json", "ndjson", "jsonl"])?.remove(0);
    let df = if is_json(&file) { ingest::json::read_json_file(&file, None)? } else { ingest::csv::read_csv(&file, &Default::default())? };
    let cols = Columns::from_frame(&df.head(Some(n)), &Default::default())?;
    let mut out = vec![format!("{} ({} columns)", file.display(), df.width())];
    for i in 0..cols.len() {
        let f = |v: Option<f64>| v.map(|x| format!("{x:.4}")).unwrap_or_else(|| "?".into());
        out.push(format!(
            "{:<20} o {} h {} l {} c {} v {}",
            cols.ts[i].map(fmt_ts).unwrap_or_else(|| "?".into()),
            f(cols.open[i]),
            f(cols.high[i]),
            f(cols.low[i]),
            f(cols.close[i]),
            f(cols.volume[i])
        ));
    }
    if let Some(n) = file.file_name() {
        let n = n.to_string_lossy();
        out.push(format!(
            "derived: symbol {} · duration {}",
            ingest::symbol_from_name_static(&n).map(|s| s.display()).unwrap_or_else(|| "?".into()),
            ingest::duration_from_name(&n).map(|d| d.to_string()).unwrap_or_else(|| "from data".into())
        ));
    }
    Ok(out)
}


// ---------------- Export ----------------

pub struct ExportForm {
    pub form: Form,
    pub format: Format,
}

impl Default for ExportForm {
    fn default() -> Self {
        Self {
            form: Form::new(
                vec![Input::new("out"), Input::new("symbol"), Input::new("source"), Input::new("duration"), Input::new("from"), Input::new("to")],
                vec!["Format", "Export"],
            ),
            format: Format::Csv,
        }
    }
}

impl ExportForm {
    pub fn typing(&self) -> bool {
        self.form.editing
    }
}

pub fn export_key(app: &mut App, k: KeyEvent) {
    match app.export.form.key(k) {
        FormEvent::Action(0) => {
            app.export.format = match app.export.format {
                Format::Csv => Format::Parquet,
                Format::Parquet => Format::Json,
                Format::Json => Format::Csv,
            };
        }
        FormEvent::Action(1) => {
            let fm = &app.export.form;
            let Some(out) = fm.get("out") else {
                app.export.form.focus("out");
                app.set_status("enter an output path");
                return;
            };
            let r = export::filter(
                &app.store,
                fm.get("source").as_deref(),
                fm.get("symbol").as_deref(),
                fm.get("duration").as_deref(),
                fm.get("from").as_deref(),
                fm.get("to").as_deref(),
            )
            .and_then(|flt| export::export(&app.store, &flt, app.export.format, &expand(&out)));
            match r {
                Ok(n) => app.set_status(format!("wrote {n} bars to {out}")),
                Err(e) => app.set_status(format!("export failed: {e}")),
            }
        }
        _ => {}
    }
}

pub fn draw_export(app: &mut App, f: &mut Frame, area: Rect) {
    let t = app.theme.clone();
    let mut lines = app.export.form.lines(&t);
    lines.insert(
        0,
        Line::from(vec![
            Span::styled(format!("{:>12}: ", "format"), Style::new().fg(t.muted)),
            Span::styled(format!("{:?}", app.export.format).to_lowercase(), Style::new().fg(t.accent)),
        ]),
    );
    lines.push(Line::from(""));
    lines.push(Line::styled("Times are UTC (epoch or YYYY-MM-DD HH:MM:SS). Symbol like BTC/USDT.", Style::new().fg(t.muted)));
    f.render_widget(Paragraph::new(lines).block(block("Export", true, &t)), area);
}

// ---------------- Keys & Backup ----------------

pub struct KeysForm {
    pub form: Form,
    pub secret_lines: Vec<String>,
    pub reveal: bool,
}

impl Default for KeysForm {
    fn default() -> Self {
        Self {
            form: Form::new(
                vec![Input::new("backup dir"), Input::masked("words"), Input::masked("password")],
                vec!["Create backup", "Rotate service key", "Re-issue shards", "Reveal/Hide"],
            ),
            secret_lines: vec![],
            reveal: false,
        }
    }
}

impl KeysForm {
    pub fn typing(&self) -> bool {
        self.form.editing
    }
}

fn master_from_form(app: &App) -> crate::Result<crate::crypto::Key32> {
    let w = app
        .keys
        .form
        .get("words")
        .ok_or_else(|| crate::Error::missing("words", "type the 24 recovery words"))?;
    let m = crate::crypto::from_words(&w)?;
    crate::crypto::keyring::verify_master(&app.paths, &m)?;
    Ok(m)
}

pub fn keys_key(app: &mut App, k: KeyEvent) {
    match app.keys.form.key(k) {
        FormEvent::Action(0) => {
            let Some(d) = app.keys.form.get("backup dir") else {
                app.keys.form.focus("backup dir");
                app.set_status("enter a backup directory");
                return;
            };
            match crate::backup::create(&app.paths, &app.store, &app.dek, &expand(&d), None) {
                Ok(p) => app.set_status(format!("wrote {}", p.display())),
                Err(e) => app.set_status(format!("backup failed: {e}")),
            }
        }
        FormEvent::Action(1) => {
            let r = master_from_form(app).and_then(|m| {
                let pw = app
                    .keys
                    .form
                    .get("password")
                    .ok_or_else(|| crate::Error::missing("password", "type the password that will wrap the new key"))?;
                crate::crypto::keyring::rotate(&app.paths, &m, &pw.into())
            });
            match r {
                Ok(v) => app.set_status(format!("service key rotated to v{v}")),
                Err(crate::Error::Missing { what, hint }) => {
                    app.keys.form.focus(what);
                    app.set_status(hint);
                }
                Err(e) => app.set_status(format!("rotate failed: {e}")),
            }
        }
        FormEvent::Action(2) => match master_from_form(app).and_then(|m| crate::crypto::split_shards(&m)) {
            Ok(s) => {
                app.keys.secret_lines = s.iter().map(|z| z.to_string()).collect();
                app.keys.reveal = false;
                app.set_status("new shards ready: select Reveal/Hide to show them once; old shards can't be mixed in");
            }
            Err(crate::Error::Missing { what, hint }) => {
                app.keys.form.focus(what);
                app.set_status(hint);
            }
            Err(e) => app.set_status(format!("failed: {e}")),
        },
        FormEvent::Action(3) => {
            app.keys.reveal = !app.keys.reveal;
            if !app.keys.reveal {
                app.keys.secret_lines.clear();
            }
        }
        _ => {}
    }
}

pub fn draw_keys(app: &mut App, f: &mut Frame, area: Rect) {
    let t = app.theme.clone();
    let [info, form, secrets] = Layout::vertical([Constraint::Length(7), Constraint::Length(8), Constraint::Min(0)]).areas(area);
    let mut lines = vec![];
    match crate::crypto::keyring::Envelope::load(&app.paths) {
        Ok(Some(e)) => {
            lines.push(Line::from(format!("database id   {}", e.db_id)));
            lines.push(Line::from(format!("database      {}", app.paths.db.display())));
            lines.push(Line::from(format!("service key   v{}", e.service_version)));
            lines.push(Line::styled(
                "Master key, words and shards are never stored. Backups contain the database (encrypted), keys/, preferences and themes.",
                Style::new().fg(t.muted),
            ));
        }
        _ => lines.push(Line::styled("keys not initialized", Style::new().fg(t.warning))),
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }).block(block("Keys", false, &t)), info);
    f.render_widget(Paragraph::new(app.keys.form.lines(&t)).block(block("Actions", true, &t)), form);
    let body: Vec<Line> = if app.keys.secret_lines.is_empty() {
        vec![Line::styled("nothing to show", Style::new().fg(t.muted))]
    } else if app.keys.reveal {
        app.keys.secret_lines.iter().map(|s| Line::styled(s.clone(), Style::new().fg(t.warning))).collect()
    } else {
        vec![Line::styled(format!("{} secret line(s) hidden — select Reveal/Hide", app.keys.secret_lines.len()), Style::new().fg(t.muted))]
    };
    f.render_widget(Paragraph::new(body).block(block("Secrets (shown once, never logged)", false, &t)), secrets);
}
