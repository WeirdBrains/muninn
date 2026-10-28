//! Import popup: file browser, per-file guesses, background ingest with live progress.
use super::{block, App};
use crate::ingest::{self, IngestOptions, Progress};
use crate::theme::Theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, Paragraph};
use ratatui::Frame;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};

/// File extensions the importer reads.
pub const EXTS: &[&str] = &["csv", "tsv", "txt", "json", "ndjson", "jsonl"];
const RECENT_MAX: usize = 10;

pub fn is_data_file(p: &Path) -> bool {
    p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).is_some_and(|e| EXTS.contains(&e.as_str()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
}

/// Directory listing: `..`, then folders, then data files; hidden entries only when asked;
/// `filter` is a case-insensitive substring.
pub fn list_dir(dir: &Path, hidden: bool, filter: &str) -> Vec<Entry> {
    let f = filter.to_lowercase();
    let mut dirs = vec![];
    let mut files = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if (!hidden && name.starts_with('.')) || (!f.is_empty() && !name.to_lowercase().contains(&f)) {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                dirs.push(Entry { name, dir: true });
            } else if is_data_file(&p) {
                files.push(Entry { name, dir: false });
            }
        }
    }
    dirs.sort_by(|a, b| a.name.cmp(&b.name));
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let mut v = vec![];
    if dir.parent().is_some() {
        v.push(Entry { name: "..".into(), dir: true });
    }
    v.extend(dirs);
    v.extend(files);
    v
}

/// Shell-style Tab completion of a typed path. Returns the completed text and, when
/// ambiguous, the candidates. `~` is expanded only for lookup; the text keeps it.
pub fn complete(input: &str, hidden: bool) -> (String, Vec<String>) {
    let (head, prefix) = match input.rfind('/') {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => ("", input),
    };
    let dir = if head.is_empty() { PathBuf::from(".") } else { super::forms::expand(head) };
    let mut cands: Vec<(String, bool)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    let show = n.starts_with(prefix) && (hidden || prefix.starts_with('.') || !n.starts_with('.'));
                    show.then(|| (n, e.path().is_dir()))
                })
                .collect()
        })
        .unwrap_or_default();
    cands.sort();
    match cands.len() {
        0 => (input.to_string(), vec![]),
        1 => {
            let (n, d) = &cands[0];
            (format!("{head}{n}{}", if *d { "/" } else { "" }), vec![])
        }
        _ => {
            let first = &cands[0].0;
            let mut l = first.len();
            for (c, _) in &cands[1..] {
                l = l.min(first.bytes().zip(c.bytes()).take_while(|(a, b)| a == b).count());
            }
            while !first.is_char_boundary(l) {
                l -= 1;
            }
            (format!("{head}{}", &first[..l]), cands.into_iter().map(|(n, d)| if d { n + "/" } else { n }).collect())
        }
    }
}

/// First lines of a file, read without parsing the whole thing.
pub fn head(path: &Path, n: usize) -> Vec<String> {
    use std::io::{BufRead, BufReader, Read};
    let Ok(f) = std::fs::File::open(path) else { return vec!["(unreadable)".into()] };
    BufReader::new(f.take(64 * 1024)).lines().take(n).map(|l| l.unwrap_or_default().chars().take(200).collect()).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Files,
    Path,
    Filter,
    Field(usize),
}

pub enum Msg {
    File(usize),
    Report(String, Vec<String>),
    Failed(String, String),
    Done,
}

pub struct Running {
    pub progress: Arc<Progress>,
    pub rx: mpsc::Receiver<Msg>,
    pub files: Vec<PathBuf>,
    pub current: usize,
    pub log: Vec<String>,
}

pub enum Phase {
    Browse,
    Running(Running),
    Done { lines: Vec<String>, cancelled: bool },
}

/// Import popup state.
pub struct ImportState {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub sel: usize,
    pub top: usize,
    pub chosen: BTreeSet<PathBuf>,
    pub hidden: bool,
    pub filter: String,
    pub path: String,
    pub completions: Vec<String>,
    pub pane: Pane,
    /// source, symbol, duration (empty = derive).
    pub fields: [String; 3],
    pub phase: Phase,
    pub preview: Option<(PathBuf, Vec<String>)>,
    pub message: Option<String>,
}

pub const FIELDS: [&str; 3] = ["source", "symbol", "duration"];

impl Default for ImportState {
    fn default() -> Self {
        Self::at(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
    }
}

impl ImportState {
    pub fn at(dir: PathBuf) -> Self {
        let mut s = Self {
            dir,
            entries: vec![],
            sel: 0,
            top: 0,
            chosen: BTreeSet::new(),
            hidden: false,
            filter: String::new(),
            path: String::new(),
            completions: vec![],
            pane: Pane::Files,
            fields: Default::default(),
            phase: Phase::Browse,
            preview: None,
            message: None,
        };
        s.reload();
        s
    }

    pub fn typing(&self) -> bool {
        matches!(self.phase, Phase::Browse) && self.pane != Pane::Files
    }

    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Running(_))
    }

    pub fn reload(&mut self) {
        self.entries = list_dir(&self.dir, self.hidden, &self.filter);
        self.sel = self.sel.min(self.entries.len().saturating_sub(1));
        self.path = format!("{}/", self.dir.display()).replace("//", "/");
    }

    pub fn cd(&mut self, dir: PathBuf) {
        self.dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        self.filter.clear();
        self.sel = 0;
        self.top = 0;
        self.reload();
    }

    pub fn current(&self) -> Option<PathBuf> {
        let e = self.entries.get(self.sel)?;
        Some(if e.name == ".." { self.dir.parent()?.to_path_buf() } else { self.dir.join(&e.name) })
    }

    /// Files to import: the multi-selection, else the highlighted file.
    pub fn targets(&self) -> Vec<PathBuf> {
        if !self.chosen.is_empty() {
            return self.chosen.iter().cloned().collect();
        }
        match (self.current(), self.entries.get(self.sel)) {
            (Some(p), Some(e)) if !e.dir => vec![p],
            _ => vec![],
        }
    }

    fn toggle(&mut self, p: PathBuf) {
        if !self.chosen.remove(&p) {
            self.chosen.insert(p);
        }
    }

    /// Select (or, if all are selected, unselect) every data file in the folder.
    pub fn toggle_all(&mut self) {
        let files: Vec<PathBuf> = self.entries.iter().filter(|e| !e.dir).map(|e| self.dir.join(&e.name)).collect();
        if files.iter().all(|f| self.chosen.contains(f)) {
            for f in files {
                self.chosen.remove(&f);
            }
        } else {
            self.chosen.extend(files);
        }
    }

    /// Guessed symbol and duration for a file name, shown before importing.
    pub fn guess(p: &Path) -> (String, String) {
        let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        (
            ingest::symbol_from_name_static(&n).map(|s| s.display()).unwrap_or_else(|| "?".into()),
            ingest::duration_from_name(&n).map(|d| d.to_string()).unwrap_or_else(|| "from data".into()),
        )
    }
}

/// Bookmarks: home, the raw data folder (MUNINN_RAW_DATA or ~/Projects/kommandosaldat/raw/data), cwd.
pub fn bookmarks() -> Vec<(String, PathBuf)> {
    let mut v = vec![];
    if let Some(h) = dirs::home_dir() {
        v.push(("home".into(), h.clone()));
        let raw = std::env::var_os("MUNINN_RAW_DATA").map(PathBuf::from).unwrap_or_else(|| h.join("Projects/kommandosaldat/raw/data"));
        if raw.is_dir() {
            v.push(("raw/data".into(), raw));
        }
    }
    if let Ok(c) = std::env::current_dir() {
        v.push(("cwd".into(), c));
    }
    v
}

fn places(app: &App) -> Vec<(String, PathBuf)> {
    let mut v = bookmarks();
    for r in &app.prefs.tui.import_recent {
        v.push((format!("recent: {}", Path::new(r).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), PathBuf::from(r)));
    }
    v
}

pub fn open(app: &mut App) {
    if !app.import.running() {
        let dir = app.prefs.tui.import_recent.first().map(PathBuf::from).filter(|p| p.is_dir());
        app.import = match dir {
            Some(d) => ImportState::at(d),
            None => ImportState::default(),
        };
    }
    app.modal = Some(super::modal::Modal::Import);
}

/// Start the background import of `files`.
pub fn start(app: &mut App, files: Vec<PathBuf>) {
    if files.is_empty() {
        app.import.message = Some("nothing selected: Space selects files, a selects the folder".into());
        return;
    }
    let progress = Arc::new(Progress::default());
    let f = &app.import.fields;
    let opt = |s: &String| (!s.trim().is_empty()).then(|| s.trim().to_string());
    let opts = IngestOptions {
        source: opt(&f[0]).or_else(|| Some("local".into())),
        symbol: opt(&f[1]),
        duration: opt(&f[2]),
        drop_zero_volume: app.prefs.ingest.drop_zero_volume,
        progress: Some(progress.clone()),
        ..Default::default()
    };
    let (tx, rx) = mpsc::channel();
    let db = app.paths.db.clone();
    let key = app.dek.0;
    let list = files.clone();
    std::thread::spawn(move || {
        let mut store = match crate::store::Store::open(&db, Some(&key)) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Msg::Failed("open store".into(), e.to_string()));
                let _ = tx.send(Msg::Done);
                return;
            }
        };
        for (i, f) in list.iter().enumerate() {
            if opts.progress.as_ref().is_some_and(|p| p.cancelled()) {
                break;
            }
            let _ = tx.send(Msg::File(i));
            let io = crate::interact::Interact::batch(true);
            let r = if super::forms::is_json(f) {
                ingest::json::ingest_json(&mut store, io, opts.clone(), f, None)
            } else {
                ingest::ingest_csv(&mut store, io, opts.clone(), f, &Default::default())
            };
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let _ = match r {
                Ok(reps) => tx.send(Msg::Report(name, reps.iter().map(line_of).collect())),
                Err(e) => tx.send(Msg::Failed(name, e.to_string())),
            };
        }
        let _ = tx.send(Msg::Done);
    });
    // remember the folder
    let d = app.import.dir.display().to_string();
    let r = &mut app.prefs.tui.import_recent;
    r.retain(|x| *x != d);
    r.insert(0, d);
    r.truncate(RECENT_MAX);
    app.save_prefs();
    app.import.phase = Phase::Running(Running { progress, rx, files, current: 0, log: vec![] });
}

fn line_of(r: &ingest::IngestReport) -> String {
    if r.skipped {
        return format!("{} {} {}: skipped (already imported)", r.source, r.symbol, r.duration);
    }
    let w = if r.warnings.is_empty() { String::new() } else { format!(" · {}", r.warnings.join("; ")) };
    format!("{} {} {}: {} rows, {} new, {} dup, {} rejected, {} gaps{w}", r.source, r.symbol, r.duration, r.rows, r.inserted, r.duplicate, r.rejected, r.gaps)
}

/// Drain messages from the background import. Returns true when it finished.
pub fn poll(app: &mut App) -> bool {
    let Phase::Running(run) = &mut app.import.phase else { return false };
    let mut done = false;
    while let Ok(m) = run.rx.try_recv() {
        match m {
            Msg::File(i) => run.current = i,
            Msg::Report(n, lines) => {
                run.log.push(format!("✓ {n}"));
                run.log.extend(lines.into_iter().map(|l| format!("    {l}")));
            }
            Msg::Failed(n, e) => run.log.push(format!("✗ {n}: {e}")),
            Msg::Done => done = true,
        }
    }
    if done {
        let cancelled = run.progress.cancelled();
        let (d, _) = run.progress.get();
        let mut lines = std::mem::take(&mut run.log);
        lines.push(String::new());
        lines.push(format!("{} file(s), {d} bars stored{}", run.files.len(), if cancelled { " · cancelled" } else { "" }));
        app.import.phase = Phase::Done { lines, cancelled };
        app.dispatch(super::action::Action::Refresh);
    }
    done
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    match &app.import.phase {
        Phase::Running(r) => {
            if matches!(k.code, KeyCode::Esc) || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL)) {
                r.progress.cancel.store(true, Ordering::Relaxed);
                app.import.message = Some("cancelling after the current chunk…".into());
            }
            return;
        }
        Phase::Done { .. } => {
            if matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                app.import.phase = Phase::Browse;
                app.import.chosen.clear();
                app.modal = None;
            }
            return;
        }
        Phase::Browse => {}
    }
    app.import.message = None;
    let st = &mut app.import;
    match st.pane {
        Pane::Path => match k.code {
            KeyCode::Tab => {
                let (t, c) = complete(&st.path, st.hidden);
                st.path = t;
                st.completions = c;
            }
            KeyCode::Enter => {
                let p = super::forms::expand(&st.path);
                st.completions.clear();
                if p.is_dir() {
                    st.cd(p);
                    st.pane = Pane::Files;
                } else if p.is_file() {
                    let files = vec![p.clone()];
                    if let Some(d) = p.parent() {
                        st.cd(d.to_path_buf());
                    }
                    st.pane = Pane::Files;
                    start(app, files);
                } else {
                    st.message = Some(format!("{} does not exist", p.display()));
                }
            }
            KeyCode::Esc => {
                st.completions.clear();
                st.reload();
                st.pane = Pane::Files;
            }
            KeyCode::Backspace => {
                st.path.pop();
            }
            KeyCode::Char(c) => st.path.push(c),
            _ => {}
        },
        Pane::Filter => match k.code {
            KeyCode::Enter | KeyCode::Esc | KeyCode::Down => st.pane = Pane::Files,
            KeyCode::Backspace => {
                st.filter.pop();
                st.reload();
            }
            KeyCode::Char(c) => {
                st.filter.push(c);
                st.sel = 0;
                st.reload();
            }
            _ => {}
        },
        Pane::Field(i) => match k.code {
            KeyCode::Enter | KeyCode::Esc => st.pane = Pane::Files,
            KeyCode::Tab | KeyCode::Down => st.pane = Pane::Field((i + 1) % 3),
            KeyCode::Up | KeyCode::BackTab => st.pane = Pane::Field((i + 2) % 3),
            KeyCode::Backspace => {
                st.fields[i].pop();
            }
            KeyCode::Char(c) => st.fields[i].push(c),
            _ => {}
        },
        Pane::Files => {
            let n = st.entries.len();
            match k.code {
                KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
                KeyCode::Down | KeyCode::Char('j') => st.sel = (st.sel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => st.sel = st.sel.saturating_sub(1),
                KeyCode::PageDown => st.sel = (st.sel + 10).min(n.saturating_sub(1)),
                KeyCode::PageUp => st.sel = st.sel.saturating_sub(10),
                KeyCode::Home => st.sel = 0,
                KeyCode::End => st.sel = n.saturating_sub(1),
                KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                    if let Some(p) = st.dir.parent().map(Path::to_path_buf) {
                        let from = st.dir.file_name().map(|n| n.to_string_lossy().to_string());
                        st.cd(p);
                        if let Some(f) = from {
                            st.sel = st.entries.iter().position(|e| e.name == f).unwrap_or(0);
                        }
                    }
                }
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    let is_dir = st.entries.get(st.sel).is_some_and(|e| e.dir);
                    if is_dir {
                        if let Some(p) = st.current() {
                            st.cd(p);
                        }
                    } else if k.code == KeyCode::Enter {
                        let t = st.targets();
                        start(app, t);
                    }
                }
                KeyCode::Char(' ') => {
                    if let (Some(p), Some(e)) = (st.current(), st.entries.get(st.sel))
                        && !e.dir
                    {
                        st.toggle(p);
                        st.sel = (st.sel + 1).min(n.saturating_sub(1));
                    }
                }
                KeyCode::Char('a') => st.toggle_all(),
                KeyCode::Char('.') => {
                    st.hidden = !st.hidden;
                    st.reload();
                }
                KeyCode::Char('/') => st.pane = Pane::Filter,
                KeyCode::Char('g') | KeyCode::Tab => {
                    st.pane = Pane::Path;
                    st.completions.clear();
                }
                KeyCode::Char('s') => st.pane = Pane::Field(0),
                KeyCode::Char('y') => st.pane = Pane::Field(1),
                KeyCode::Char('d') => st.pane = Pane::Field(2),
                KeyCode::Char('i') => {
                    let t = st.targets();
                    start(app, t);
                }
                KeyCode::Char(c @ '1'..='9') => {
                    let i = c as usize - '1' as usize;
                    if let Some((_, p)) = places(app).get(i).cloned() {
                        app.import.cd(p);
                    }
                }
                _ => {}
            }
        }
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect) {
    let t = app.theme.clone();
    let places = places(app);
    let st = &mut app.import;
    let title = match &st.phase {
        Phase::Browse => "Import",
        Phase::Running(_) => "Importing…",
        Phase::Done { cancelled: true, .. } => "Import cancelled",
        Phase::Done { .. } => "Import finished",
    };
    let outer = block(title, true, &t);
    let inner = outer.inner(r);
    f.render_widget(outer, r);
    match &st.phase {
        Phase::Running(run) => return draw_running(run, st.message.as_deref(), f, inner, &t),
        Phase::Done { lines, .. } => {
            let mut v: Vec<Line> = lines.iter().map(|l| Line::styled(l.clone(), style_of(l, &t))).collect();
            v.push(Line::styled("Enter / Esc closes", Style::new().fg(t.muted)));
            let skip = v.len().saturating_sub(inner.height as usize);
            f.render_widget(Paragraph::new(v).scroll((skip as u16, 0)), inner);
            return;
        }
        Phase::Browse => {}
    }
    let [path_r, body, fields_r, help_r] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(4), Constraint::Length(4), Constraint::Length(1)]).areas(inner);
    // path line + completions
    let focus = |p: Pane| if st.pane == p { Style::new().fg(t.accent).add_modifier(Modifier::BOLD) } else { Style::new().fg(t.muted) };
    let caret = |p: Pane| if st.pane == p { "▏" } else { "" };
    let mut pl = vec![Line::from(vec![Span::styled(" path ", focus(Pane::Path)), Span::styled(format!("{}{}", st.path, caret(Pane::Path)), Style::new().fg(t.fg))])];
    pl.push(if !st.completions.is_empty() {
        Line::styled(format!("      {}", st.completions.join("  ")), Style::new().fg(t.info))
    } else {
        Line::from(vec![
            Span::styled(" filter ", focus(Pane::Filter)),
            Span::styled(format!("{}{}", st.filter, caret(Pane::Filter)), Style::new().fg(t.fg)),
            Span::styled(format!("   hidden {}   {} selected", if st.hidden { "shown" } else { "off" }, st.chosen.len()), Style::new().fg(t.muted)),
        ])
    });
    f.render_widget(Paragraph::new(pl), path_r);
    let [places_r, files_r, prev_r] =
        Layout::horizontal([Constraint::Length(22), Constraint::Percentage(45), Constraint::Min(20)]).areas(body);
    // places
    let pv: Vec<Line> = places
        .iter()
        .enumerate()
        .map(|(i, (n, _))| Line::styled(format!("{} {}", i + 1, n).chars().take(21).collect::<String>(), Style::new().fg(if i < 9 { t.fg } else { t.muted })))
        .collect();
    f.render_widget(Paragraph::new(pv).block(block("Places", false, &t)), places_r);
    // files
    let fb = block("Files", st.pane == Pane::Files, &t);
    let fi = fb.inner(files_r);
    f.render_widget(fb, files_r);
    let h = fi.height as usize;
    if st.sel < st.top {
        st.top = st.sel;
    } else if h > 0 && st.sel >= st.top + h {
        st.top = st.sel + 1 - h;
    }
    let mut lines = vec![];
    for (i, e) in st.entries.iter().enumerate().skip(st.top).take(h) {
        let p = st.dir.join(&e.name);
        let mark = if e.dir { "▸" } else if st.chosen.contains(&p) { "●" } else { "○" };
        let name = if e.dir && e.name != ".." { format!("{}/", e.name) } else { e.name.clone() };
        let mut s = Style::new().fg(if e.dir { t.accent } else { t.fg });
        if i == st.sel {
            s = Style::new().bg(t.selection_bg).fg(t.selection_fg);
        }
        lines.push(Line::styled(format!("{mark} {name}"), s));
    }
    if st.entries.is_empty() {
        lines.push(Line::styled("(no folders or data files)", Style::new().fg(t.muted)));
    }
    f.render_widget(Paragraph::new(lines), fi);
    // preview of the highlighted file
    let cur = st.current().filter(|p| p.is_file());
    let mut pv = vec![];
    if let Some(p) = cur {
        if st.preview.as_ref().is_none_or(|(q, _)| *q != p) {
            st.preview = Some((p.clone(), head(&p, 8)));
        }
        let (sym, dur) = ImportState::guess(&p);
        pv.push(Line::from(vec![Span::styled("symbol ", Style::new().fg(t.muted)), Span::styled(sym, Style::new().fg(t.fg))]));
        pv.push(Line::from(vec![Span::styled("period ", Style::new().fg(t.muted)), Span::styled(dur, Style::new().fg(t.fg))]));
        if let Ok(m) = p.metadata() {
            pv.push(Line::styled(format!("size   {:.1} MB", m.len() as f64 / 1e6), Style::new().fg(t.muted)));
        }
        pv.push(Line::raw(""));
        for l in &st.preview.as_ref().unwrap().1 {
            pv.push(Line::styled(l.clone(), Style::new().fg(t.fg)));
        }
    } else {
        pv.push(Line::styled("highlight a file to preview it", Style::new().fg(t.muted)));
    }
    f.render_widget(Paragraph::new(pv).block(block("Preview", false, &t)), prev_r);
    // inline resolution fields
    let mut fl = vec![];
    for (i, name) in FIELDS.iter().enumerate() {
        let v = &st.fields[i];
        let shown = if v.is_empty() && st.pane != Pane::Field(i) {
            Span::styled(if i == 0 { "local (default)" } else { "derive from name / data" }, Style::new().fg(t.muted))
        } else {
            Span::styled(format!("{v}{}", caret(Pane::Field(i))), Style::new().fg(t.fg))
        };
        fl.push(Line::from(vec![Span::styled(format!(" {:<9}", name), focus(Pane::Field(i))), shown]));
    }
    if let Some(m) = &st.message {
        fl.push(Line::styled(format!(" {m}"), Style::new().fg(t.warning)));
    }
    f.render_widget(Paragraph::new(fl), fields_r);
    let help = match st.pane {
        Pane::Path => "Tab complete · Enter go/import · Esc back",
        Pane::Filter | Pane::Field(_) => "type · Enter done · Esc back",
        Pane::Files => "Enter open/import · Space select · a folder · . hidden · / filter · Tab path · 1-9 places · s/y/d fields · i import · Esc",
    };
    f.render_widget(Paragraph::new(Line::styled(help, Style::new().fg(t.muted))), help_r);
}

fn style_of(l: &str, t: &Theme) -> Style {
    if l.starts_with('✗') {
        Style::new().fg(t.error)
    } else if l.starts_with('✓') {
        Style::new().fg(t.success)
    } else {
        Style::new().fg(t.fg)
    }
}

fn draw_running(run: &Running, msg: Option<&str>, f: &mut Frame, r: Rect, t: &Theme) {
    let [info, gauge, log, help] =
        Layout::vertical([Constraint::Length(2), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]).areas(r);
    let name = run.files.get(run.current).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(format!(" file {}/{}: {name}", run.current + 1, run.files.len()), Style::new().fg(t.fg)),
            Line::styled(msg.unwrap_or("").to_string(), Style::new().fg(t.warning)),
        ]),
        info,
    );
    let (d, tot) = run.progress.get();
    let ratio = if tot == 0 { 0.0 } else { (d as f64 / tot as f64).min(1.0) };
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::new().fg(t.accent).bg(t.bg))
            .ratio(ratio)
            .label(format!("{d} / {tot} bars ({:.0}%)", ratio * 100.0)),
        gauge,
    );
    let v: Vec<Line> = run.log.iter().map(|l| Line::styled(l.clone(), style_of(l, t))).collect();
    let skip = v.len().saturating_sub(log.height as usize);
    f.render_widget(Paragraph::new(v).scroll((skip as u16, 0)), log);
    f.render_widget(Paragraph::new(Line::styled("Esc cancels (stops after the current chunk; stored bars stay)", Style::new().fg(t.muted))), help);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let p = t.path();
        std::fs::create_dir(p.join("data")).unwrap();
        std::fs::create_dir(p.join("docs")).unwrap();
        std::fs::create_dir(p.join(".hidden")).unwrap();
        std::fs::write(p.join("BTCUSDT_5m.csv"), "a\n").unwrap();
        std::fs::write(p.join("ETHUSDT_1h.json"), "[]").unwrap();
        std::fs::write(p.join("notes.md"), "x").unwrap();
        t
    }

    #[test]
    fn completion_extends_common_prefix_and_lists() {
        let t = tree();
        let base = format!("{}/", t.path().display());
        assert_eq!(complete(&format!("{base}da"), false), (format!("{base}data/"), vec![]));
        let (s, c) = complete(&format!("{base}d"), false);
        assert_eq!(s, format!("{base}d"));
        assert_eq!(c, vec!["data/".to_string(), "docs/".to_string()]);
        assert_eq!(complete(&format!("{base}B"), false).0, format!("{base}BTCUSDT_5m.csv"));
        // hidden only when asked or typed
        assert!(complete(&base, false).1.iter().all(|c| !c.starts_with('.')));
        assert_eq!(complete(&format!("{base}.h"), false).0, format!("{base}.hidden/"));
        assert_eq!(complete(&format!("{base}zzz"), false), (format!("{base}zzz"), vec![]));
    }

    #[test]
    fn listing_filters_and_orders() {
        let t = tree();
        let names = |v: Vec<Entry>| v.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(list_dir(t.path(), false, "")), ["..", "data", "docs", "BTCUSDT_5m.csv", "ETHUSDT_1h.json"]);
        assert!(names(list_dir(t.path(), true, "")).contains(&".hidden".to_string()));
        assert_eq!(names(list_dir(t.path(), false, "eth")), ["..", "ETHUSDT_1h.json"]);
    }

    #[test]
    fn selection_targets_and_guesses() {
        let t = tree();
        let mut s = ImportState::at(t.path().to_path_buf());
        assert!(s.targets().is_empty()); // ".." highlighted
        s.sel = 3;
        assert_eq!(s.targets().len(), 1);
        s.toggle_all();
        assert_eq!(s.chosen.len(), 2);
        s.toggle_all();
        assert!(s.chosen.is_empty());
        assert_eq!(ImportState::guess(Path::new("BTCUSDT_5m.csv")), ("BTC/USDT".into(), "5m".into()));
    }
}
