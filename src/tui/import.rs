//! Import popup: file browser, per-file guesses, background ingest with live progress.
use super::{block, App};
use crate::ingest::{self, IngestOptions, Progress};
use crate::theme::Theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
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

/// Lines shown by the preview.
pub const PREVIEW_LINES: usize = 10;

/// One preview line: parsed bar text, or the raw line with why it is invalid.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewRow {
    pub text: String,
    pub ok: bool,
}

fn fmt_px(v: f64) -> String {
    super::tiles::fmt_num(v)
}

/// Parse the first [`PREVIEW_LINES`] data lines of a file with the ingest parser
/// (column mapping, timestamp parsing, validation). Reads at most 64 KiB.
pub fn preview_bars(path: &Path) -> Vec<PreviewRow> {
    use crate::ingest::frame::{derive_duration, Columns};
    let json = super::forms::is_json(path);
    let ndjson = crate::ingest::json::is_ndjson(path);
    let header = usize::from(!json);
    let lines = head(path, PREVIEW_LINES + header);
    let data_lines: Vec<String> = lines.iter().skip(header).cloned().collect();
    let raw = |why: &str| {
        let mut v = vec![PreviewRow { text: why.to_string(), ok: false }];
        v.extend(data_lines.iter().map(|l| PreviewRow { text: l.clone(), ok: false }));
        v
    };
    let mut text = lines.join("\n");
    text.push('\n');
    let df = if json && !ndjson {
        // A JSON array only parses whole; small files fit in the head read.
        let whole = path.metadata().map(|m| m.len() <= 64 * 1024).unwrap_or(false);
        if !whole {
            return raw("JSON array: too large to parse a head preview (ndjson previews parse)");
        }
        std::fs::read(path).map_err(crate::Error::from).and_then(|b| crate::ingest::json::read_json_bytes(&b, false))
    } else if json {
        crate::ingest::json::read_json_bytes(text.as_bytes(), true)
    } else {
        crate::ingest::csv::read_csv_bytes(text.into_bytes(), &Default::default())
    };
    let df = match df {
        Ok(d) => d.head(Some(PREVIEW_LINES)),
        Err(e) => return raw(&format!("✗ cannot parse: {e}")),
    };
    let cols = match Columns::from_frame(&df, &Default::default()) {
        Ok(c) => c,
        Err(e) => return raw(&format!("✗ {e}")),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let sym = crate::ingest::symbol_from_name_static(&name).map(|s| s.display()).unwrap_or_else(|| "?/?".into());
    let dur = crate::ingest::duration_from_name(&name)
        .or_else(|| derive_duration(&cols.valid_ts()).map(|d| d.duration))
        // unknown duration: skip the alignment check
        .unwrap_or(crate::model::Duration(400 * 86_400));
    let v = cols.validate(dur, false);
    let mut rejects: std::collections::HashMap<usize, String> = v.rejects.into_iter().map(|r| (r.row, r.reason)).collect();
    let mut bars = v.bars.into_iter();
    let mut out = vec![];
    for i in 0..cols.len() {
        let row = i + 1;
        let line = data_lines.get(i).cloned().unwrap_or_default();
        if let Some(why) = rejects.remove(&row) {
            out.push(PreviewRow { text: format!("✗ {why}: {line}"), ok: false });
        } else if let Some(b) = bars.next() {
            let flags = if b.flags & crate::model::FLAG_ZERO_VOLUME != 0 { " zero-vol" } else { "" };
            out.push(PreviewRow {
                text: format!(
                    "{} {sym} o {} h {} l {} c {} v {}{flags}",
                    crate::ingest::time::fmt_ts(b.ts),
                    fmt_px(b.open),
                    fmt_px(b.high),
                    fmt_px(b.low),
                    fmt_px(b.close),
                    fmt_px(b.volume)
                ),
                ok: true,
            });
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Files,
    Path,
    Filter,
    Field(usize),
    Places,
}

pub enum Msg {
    File(usize),
    Report(String, Vec<String>),
    Failed(String, String),
    /// Cancelled while importing this file; it was rolled back.
    Cancelled(String),
    /// The import run id (ledger).
    Run(i64),
    Done,
}

pub struct Running {
    pub progress: Arc<Progress>,
    pub rx: mpsc::Receiver<Msg>,
    pub files: Vec<PathBuf>,
    pub current: usize,
    pub log: Vec<String>,
    pub started: std::time::Instant,
    /// Asking "cancel the import?".
    pub confirm: bool,
    /// Files fully imported (kept).
    pub kept: usize,
    pub run_id: Option<i64>,
    /// The folder was added to Recent by this run (undone if nothing is kept).
    pub recent_added: bool,
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
    pub preview: Option<(PathBuf, Vec<PreviewRow>)>,
    pub message: Option<String>,
    /// Highlighted entry in the Places column.
    pub place_sel: usize,
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
            place_sel: 0,
        };
        s.reload();
        s
    }

    pub fn typing(&self) -> bool {
        matches!(self.phase, Phase::Browse) && !matches!(self.pane, Pane::Files | Pane::Places)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceKind {
    Builtin,
    Bookmark,
    Recent,
}

/// One entry of the Places column.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
    pub kind: PlaceKind,
}

/// Built-in places: home, ~/Projects, the raw data folder (MUNINN_RAW_DATA or
/// ~/Projects/kommandosaldat/raw/data), the working folder. Missing folders are left out.
pub fn builtin_places() -> Vec<Place> {
    let mut v = vec![];
    let mut add = |label: &str, path: PathBuf| {
        if path.is_dir() && !v.iter().any(|p: &Place| p.path == path) {
            v.push(Place { label: label.into(), path, kind: PlaceKind::Builtin });
        }
    };
    if let Some(h) = dirs::home_dir() {
        add("Home", h.clone());
        add("Projects", h.join("Projects"));
        add("raw/data", std::env::var_os("MUNINN_RAW_DATA").map(PathBuf::from).unwrap_or_else(|| h.join("Projects/kommandosaldat/raw/data")));
    }
    if let Ok(c) = std::env::current_dir() {
        add("Working folder", c);
    }
    v
}

fn short(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string())
}

/// Places in display order: built-ins, then bookmarks, then recent folders.
pub fn places(app: &App) -> Vec<Place> {
    let mut v = builtin_places();
    for b in &app.prefs.tui.import_bookmarks {
        v.push(Place { label: short(b), path: PathBuf::from(b), kind: PlaceKind::Bookmark });
    }
    for r in &app.prefs.tui.import_recent {
        v.push(Place { label: short(r), path: PathBuf::from(r), kind: PlaceKind::Recent });
    }
    v
}

/// Bookmark the current folder (toggle: removes it when already bookmarked).
pub fn toggle_bookmark(app: &mut App) {
    let d = app.import.dir.display().to_string();
    let b = &mut app.prefs.tui.import_bookmarks;
    if let Some(i) = b.iter().position(|x| *x == d) {
        b.remove(i);
        app.import.message = Some(format!("removed bookmark {d}"));
    } else {
        b.push(d.clone());
        app.import.message = Some(format!("bookmarked {d}"));
    }
    app.save_prefs();
}

/// Remove a bookmark or recent entry; built-ins cannot be removed.
pub fn remove_place(app: &mut App, i: usize) {
    let Some(p) = places(app).get(i).cloned() else { return };
    let s = p.path.display().to_string();
    match p.kind {
        PlaceKind::Builtin => {
            app.import.message = Some("built-in places can't be removed".into());
            return;
        }
        PlaceKind::Bookmark => app.prefs.tui.import_bookmarks.retain(|x| *x != s),
        PlaceKind::Recent => app.prefs.tui.import_recent.retain(|x| *x != s),
    }
    app.import.message = Some(format!("removed {}", p.label));
    app.save_prefs();
}

pub fn go_place(app: &mut App, i: usize) {
    if let Some(p) = places(app).get(i).cloned() {
        if p.path.is_dir() {
            app.import.cd(p.path);
            app.import.place_sel = i;
        } else {
            app.import.message = Some(format!("{} no longer exists (x removes it)", p.path.display()));
        }
    }
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
    let sizes: Vec<u64> = files.iter().map(|f| f.metadata().map(|m| m.len()).unwrap_or(0)).collect();
    progress.bytes_total.store(sizes.iter().sum(), Ordering::Relaxed);
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
        let mut opts = opts;
        let run = store.begin_import_run("tui", list.len()).ok();
        opts.run_id = run;
        let mut kept = 0usize;
        let mut failed = false;
        for (i, f) in list.iter().enumerate() {
            if opts.progress.as_ref().is_some_and(|p| p.cancelled()) {
                break;
            }
            let _ = tx.send(Msg::File(i));
            if let Some(p) = &opts.progress {
                p.begin_file(i as u64, sizes[i]);
            }
            let io = crate::interact::Interact::batch(true);
            let r = if super::forms::is_json(f) {
                ingest::json::ingest_json(&mut store, io, opts.clone(), f, None)
            } else {
                ingest::ingest_csv(&mut store, io, opts.clone(), f, &Default::default())
            };
            if let Some(p) = &opts.progress {
                p.end_file();
            }
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let _ = match r {
                Ok(reps) => {
                    kept += 1;
                    tx.send(Msg::Report(name, reps.iter().map(line_of).collect()))
                }
                Err(_) if opts.progress.as_ref().is_some_and(|p| p.cancelled()) => tx.send(Msg::Cancelled(name)),
                Err(e) => {
                    failed = true;
                    tx.send(Msg::Failed(name, e.to_string()))
                }
            };
        }
        if let Some(run) = run {
            let cancelled = opts.progress.as_ref().is_some_and(|p| p.cancelled());
            if cancelled {
                // A cancel that landed between files has no file to name.
                if let Some(p) = &opts.progress
                    && store.import_run(run).ok().flatten().is_some_and(|r| r.cancel_requested_at.is_none())
                {
                    let _ = store.record_cancel(run, p.cancel_at.load(Ordering::Relaxed), p.cancel_by_name(), "", p.phase().name());
                }
            }
            let removed = store.cleanup_run(run).unwrap_or(0);
            let status = if cancelled { "cancelled" } else if failed { "failed" } else { "completed" };
            let _ = store.finish_import_run(run, status, kept, &format!("{removed} empty leftover(s) removed"));
            let _ = tx.send(Msg::Run(run));
        }
        let _ = tx.send(Msg::Done);
    });
    // remember the folder
    let d = app.import.dir.display().to_string();
    let r = &mut app.prefs.tui.import_recent;
    let recent_added = !r.contains(&d);
    r.retain(|x| *x != d);
    r.insert(0, d);
    r.truncate(RECENT_MAX);
    app.save_prefs();
    app.import.phase = Phase::Running(Running { progress, rx, files, current: 0, log: vec![], started: std::time::Instant::now(), confirm: false, kept: 0, run_id: None, recent_added });
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
                run.kept += 1;
                run.log.push(format!("✓ {n}"));
                run.log.extend(lines.into_iter().map(|l| format!("    {l}")));
            }
            Msg::Failed(n, e) => run.log.push(format!("✗ {n}: {e} (rolled back)")),
            Msg::Cancelled(n) => run.log.push(format!("✗ {n}: cancelled, rolled back (nothing of it stored)")),
            Msg::Run(id) => run.run_id = Some(id),
            Msg::Done => done = true,
        }
    }
    if done {
        let cancelled = run.progress.cancelled();
        let (d, _) = run.progress.get();
        let mb = run.progress.bytes_before.load(Ordering::Relaxed) as f64 / 1e6;
        let d = if cancelled { d.min(run.progress.done.load(Ordering::Relaxed)) } else { d };
        let secs = run.started.elapsed().as_secs_f64();
        let mut lines = std::mem::take(&mut run.log);
        lines.push(String::new());
        let mut undo_recent = None;
        if cancelled && run.kept == 0 && run.recent_added {
            undo_recent = Some(app.import.dir.display().to_string());
        }
        if let Some(id) = run.run_id {
            lines.push(format!("import run #{id} · ledger: mnnn ingest runs --show {id}"));
        }
        if cancelled {
            let started = (run.current + 1).min(run.files.len());
            let discarded = started - run.kept.min(started);
            lines.push(format!(
                "Cancelled: kept {} complete file(s), discarded {discarded} partly imported, {} not started.",
                run.kept,
                run.files.len() - started
            ));
        }
        lines.push(format!(
            "{} file(s), {mb:.1} MB, {d} bars stored in {secs:.1} s{}",
            run.files.len(),
            if cancelled { " · cancelled" } else { "" }
        ));
        let run_id = run.run_id;
        app.import.phase = Phase::Done { lines, cancelled };
        if let Some(d) = undo_recent {
            // Nothing was imported: the Recent entry this run added goes too.
            app.prefs.tui.import_recent.retain(|x| *x != d);
            app.save_prefs();
            if let Some(id) = run_id {
                let _ = app.store.add_artifact(
                    id,
                    &crate::store::Artifact { kind: "recent".into(), reference: d, file: String::new(), state: "removed".into() },
                );
            }
        }
        app.dispatch(super::action::Action::Refresh);
    }
    done
}

/// Ask to cancel the running import (Esc, c, [Cancel], [X]).
pub fn request_cancel(app: &mut App) {
    if let Phase::Running(r) = &mut app.import.phase
        && !r.progress.cancelled()
    {
        r.confirm = true;
    }
}

/// Answer the cancel question. Yes signals the background thread, which stops at the
/// next read or row batch and rolls back the current file.
pub fn confirm_cancel(app: &mut App, yes: bool) {
    if let Phase::Running(r) = &mut app.import.phase {
        r.confirm = false;
        if yes {
            r.progress.request_cancel("tui");
            app.import.message = Some("Cancelling… rolling back the current file".into());
        }
    }
}

/// Close the popup ([X] or Esc); during an import this asks to cancel instead.
pub fn close(app: &mut App) {
    match app.import.phase {
        Phase::Running(_) => request_cancel(app),
        Phase::Done { .. } => {
            app.import.phase = Phase::Browse;
            app.import.chosen.clear();
            app.modal = None;
        }
        Phase::Browse => app.modal = None,
    }
}

pub fn on_click(app: &mut App, h: super::Hit) {
    use super::Hit;
    match h {
        Hit::ImportCancel => request_cancel(app),
        Hit::ImportConfirm(yes) => confirm_cancel(app, yes),
        Hit::ImportPlace(i) => {
            app.import.pane = Pane::Places;
            go_place(app, i);
        }
        _ => {}
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    match &app.import.phase {
        Phase::Running(r) => {
            if r.confirm {
                match k.code {
                    KeyCode::Char('y') | KeyCode::Enter => confirm_cancel(app, true),
                    KeyCode::Char('n') | KeyCode::Esc => confirm_cancel(app, false),
                    _ => {}
                }
            } else if matches!(k.code, KeyCode::Esc | KeyCode::Char('c')) {
                request_cancel(app);
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
        Pane::Places => {
            let n = places(app).len();
            let st = &mut app.import;
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => st.place_sel = (st.place_sel + 1).min(n.saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => st.place_sel = st.place_sel.saturating_sub(1),
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                    let i = st.place_sel;
                    go_place(app, i);
                    app.import.pane = Pane::Files;
                }
                KeyCode::Char('x') | KeyCode::Delete => {
                    let i = st.place_sel;
                    remove_place(app, i);
                    app.import.place_sel = i.min(places(app).len().saturating_sub(1));
                }
                KeyCode::Char('b') => toggle_bookmark(app),
                KeyCode::Esc | KeyCode::Tab | KeyCode::Char('p') => st.pane = Pane::Files,
                KeyCode::Char(c @ '1'..='9') => go_place(app, c as usize - '1' as usize),
                _ => {}
            }
        }
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
                KeyCode::Char(c @ '1'..='9') => go_place(app, c as usize - '1' as usize),
                KeyCode::Char('p') => st.pane = Pane::Places,
                KeyCode::Char('b') => toggle_bookmark(app),
                _ => {}
            }
        }
    }
}

pub fn draw(app: &mut App, f: &mut Frame, r: Rect) {
    let mut hits = std::mem::take(&mut app.modal_hits);
    draw_body(app, f, r, &mut hits);
    app.modal_hits = hits;
}

fn draw_body(app: &mut App, f: &mut Frame, r: Rect, app_hits: &mut Vec<(Rect, super::Hit)>) {
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
        Phase::Running(run) => return draw_running(run, st.message.as_deref(), f, inner, &t, app_hits),
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
    let pb = block("Places (p)", st.pane == Pane::Places, &t);
    let pi = pb.inner(places_r);
    f.render_widget(pb, places_r);
    let mut pv: Vec<Line> = vec![];
    let mut y = pi.y;
    let mut last: Option<PlaceKind> = None;
    let w = pi.width as usize;
    for (i, p) in places.iter().enumerate() {
        if last != Some(p.kind) {
            if last.is_some() {
                pv.push(Line::raw(""));
                y += 1;
            }
            let head = match p.kind {
                PlaceKind::Builtin => "Places",
                PlaceKind::Bookmark => "Bookmarks",
                PlaceKind::Recent => "Recent",
            };
            pv.push(Line::styled(head, Style::new().fg(t.header).add_modifier(Modifier::BOLD)));
            y += 1;
            last = Some(p.kind);
        }
        let key = if i < 9 { format!("{} ", i + 1) } else { "  ".into() };
        let sel = st.pane == Pane::Places && i == st.place_sel;
        let here = p.path == st.dir;
        let style = if sel {
            Style::new().bg(t.selection_bg).fg(t.selection_fg)
        } else if here {
            Style::new().fg(t.accent)
        } else {
            Style::new().fg(t.fg)
        };
        pv.push(Line::styled(format!("{key}{}", p.label).chars().take(w).collect::<String>(), style));
        if y < pi.bottom() {
            app_hits.push((Rect { x: pi.x, y, width: pi.width, height: 1 }, super::Hit::ImportPlace(i)));
        }
        y += 1;
    }
    if !places.iter().any(|p| p.kind == PlaceKind::Bookmark) {
        pv.push(Line::raw(""));
        pv.push(Line::styled("b bookmarks this", Style::new().fg(t.muted)));
        pv.push(Line::styled("folder", Style::new().fg(t.muted)));
    }
    f.render_widget(Paragraph::new(pv), pi);
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
            st.preview = Some((p.clone(), preview_bars(&p)));
        }
        let (sym, dur) = ImportState::guess(&p);
        pv.push(Line::from(vec![Span::styled("symbol ", Style::new().fg(t.muted)), Span::styled(sym, Style::new().fg(t.fg))]));
        pv.push(Line::from(vec![Span::styled("period ", Style::new().fg(t.muted)), Span::styled(dur, Style::new().fg(t.fg))]));
        if let Ok(m) = p.metadata() {
            pv.push(Line::styled(format!("size   {:.1} MB", m.len() as f64 / 1e6), Style::new().fg(t.muted)));
        }
        pv.push(Line::raw(""));
        for r in &st.preview.as_ref().unwrap().1 {
            pv.push(Line::styled(r.text.clone(), Style::new().fg(if r.ok { t.fg } else { t.error })));
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
        Pane::Files => "Enter open/import · Space select · a folder · . hidden · / filter · Tab path · p places · 1-9 jump · b bookmark · s/y/d fields · i import",
        Pane::Places => "j/k move · Enter go · x remove bookmark/recent · b bookmark current folder · 1-9 jump · Esc/p back",
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

/// "1.2 MB" style.
pub fn fmt_bytes(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{b} B")
    }
}

/// Seconds left from the rate so far; None until there is something to go on.
pub fn eta(done: u64, total: u64, elapsed: f64) -> Option<f64> {
    if done == 0 || elapsed < 0.5 || done >= total {
        return None;
    }
    let rate = done as f64 / elapsed;
    Some((total - done) as f64 / rate)
}

fn fmt_secs(s: f64) -> String {
    let s = s.round() as u64;
    if s >= 3600 { format!("{}h{:02}m", s / 3600, s / 60 % 60) } else if s >= 60 { format!("{}m{:02}s", s / 60, s % 60) } else { format!("{s}s") }
}

fn draw_running(run: &Running, msg: Option<&str>, f: &mut Frame, r: Rect, t: &Theme, hits: &mut Vec<(Rect, super::Hit)>) {
    let [info, file_g, all_g, stats, log, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(r);
    let e = run.progress.estimate();
    let idx = (e.file_index as usize).min(run.files.len().saturating_sub(1));
    let name = run.files.get(idx).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    f.render_widget(Paragraph::new(Line::styled(format!(" file {}/{}: {name}", idx + 1, run.files.len()), Style::new().fg(t.fg))), info);
    let gauge = |ratio: f64, label: String| Gauge::default().gauge_style(Style::new().fg(t.accent).bg(t.bg)).ratio(ratio.clamp(0.0, 1.0)).label(label);
    f.render_widget(gauge(e.file, format!("file {:.0}%", e.file * 100.0)), file_g);
    let elapsed = run.started.elapsed().as_secs_f64();
    let left = eta(e.bytes_done, e.bytes_total, elapsed).map(|s| format!(" · ETA {}", fmt_secs(s))).unwrap_or_default();
    f.render_widget(
        gauge(e.overall, format!("all {:.0}% · {} / {}{left}", e.overall * 100.0, fmt_bytes(e.bytes_done), fmt_bytes(e.bytes_total))),
        all_g,
    );
    let (d, tot) = run.progress.get();
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(format!(" {d} / {tot} bars stored · {}", fmt_secs(elapsed)), Style::new().fg(t.muted)),
            Line::styled(msg.unwrap_or("").to_string(), Style::new().fg(t.warning)),
        ]),
        stats,
    );
    let v: Vec<Line> = run.log.iter().map(|l| Line::styled(l.clone(), style_of(l, t))).collect();
    let skip = v.len().saturating_sub(log.height as usize);
    f.render_widget(Paragraph::new(v).scroll((skip as u16, 0)), log);
    // [Cancel] button, or the confirm question
    let btn = |label: &str, fg, bg| Span::styled(format!(" {label} "), Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD));
    let mut x = help.x;
    let mut put = |spans: &mut Vec<Span<'static>>, s: Span<'static>, hit: Option<super::Hit>| {
        let w = s.content.chars().count() as u16;
        if let Some(h) = hit {
            hits.push((Rect { x, y: help.y, width: w, height: 1 }, h));
        }
        x += w;
        spans.push(s);
    };
    let mut spans: Vec<Span<'static>> = vec![];
    if run.progress.cancelled() {
        put(&mut spans, Span::styled("Cancelling… the current file is being rolled back", Style::new().fg(t.warning)), None);
    } else if run.confirm {
        put(&mut spans, Span::styled("Cancel the import? Completed files stay; the current file is rolled back.  ", Style::new().fg(t.warning)), None);
        put(&mut spans, btn("[Yes, cancel]", t.bg, t.error), Some(super::Hit::ImportConfirm(true)));
        put(&mut spans, Span::raw("  "), None);
        put(&mut spans, btn("[No, continue]", t.bg, t.accent), Some(super::Hit::ImportConfirm(false)));
        put(&mut spans, Span::styled("  y / n", Style::new().fg(t.muted)), None);
    } else {
        put(&mut spans, btn("[Cancel]", t.bg, t.warning), Some(super::Hit::ImportCancel));
        put(&mut spans, Span::styled("  Esc or c cancels (asks first)", Style::new().fg(t.muted)), None);
    }
    f.render_widget(Paragraph::new(Line::from(spans)), help);
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
    fn preview_parses_head_and_marks_invalid_lines() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("BTCUSDT_5m.csv");
        let mut csv = String::from("timestamp,open,high,low,close,volume\n");
        csv += "2020-01-01 00:00:00,10,12,9,11,5\n";
        csv += "2020-01-01 00:05:00,10,9,9,11,5\n"; // high below close
        csv += "garbage,1,2,0.5,1,1\n";
        csv += "2020-01-01 00:10:00,11,11,11,11,0\n";
        for i in 0..50_000 {
            csv += &format!("2020-02-01 00:{:02}:00,1,1,1,1,1\n", i % 60);
        }
        std::fs::write(&f, csv).unwrap();
        let t = std::time::Instant::now();
        let v = preview_bars(&f);
        assert!(t.elapsed().as_millis() < 200);
        assert_eq!(v.len(), PREVIEW_LINES);
        assert!(v[0].ok && v[0].text.contains("2020-01-01 00:00:00 BTC/USDT o 10"), "{:?}", v[0]);
        assert!(!v[1].ok && v[1].text.contains("high below") && v[1].text.contains("00:05:00,10,9"), "{:?}", v[1]);
        assert!(!v[2].ok && v[2].text.contains("timestamp"), "{:?}", v[2]);
        assert!(v[3].ok && v[3].text.ends_with("zero-vol"), "{:?}", v[3]);
        let n = d.path().join("x.ndjson");
        std::fs::write(&n, "{\"t\":1577836800000,\"o\":1,\"h\":2,\"l\":0.5,\"c\":1.5,\"v\":3}\n").unwrap();
        let v = preview_bars(&n);
        assert!(v.len() == 1, "{v:?}");
    }

    #[test]
    fn bytes_and_eta_format() {
        assert_eq!(fmt_bytes(61_855_259), "61.9 MB");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(eta(0, 100, 10.0), None);
        assert_eq!(eta(50, 100, 10.0), Some(10.0));
        assert_eq!(eta(100, 100, 10.0), None);
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
