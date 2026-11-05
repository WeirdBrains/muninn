//! Ingestion: data goes into its source **as it arrives**. A file, an HTTP page or a
//! query result is stored byte for byte in `source_data`; nothing is parsed, no columns,
//! symbols, bar lengths or rows are assumed. What the data is, and what to make of it
//! (rows, bars, anything), is decided later by pipelines and their code spaces
//! (`source:NAME | code:parse-csv | code:to-bars | model:bars`). See `docs/ingest-csv.md`.
pub mod csv;
pub mod frame;
pub mod json;
pub mod progress;
pub mod service;
pub mod time;

use crate::error::{Error, Result};
use crate::interact::Interact;
use crate::model::Source;
use crate::store::Store;
use std::path::{Path, PathBuf};

/// Ingest options (flags or TUI fields). Only where data goes and how it is tracked:
/// nothing about what the data is.
#[derive(Debug, Clone, Default)]
pub struct IngestOptions {
    pub source: Option<String>,
    /// Store again even if these exact bytes are already in the source.
    pub force: bool,
    /// Live progress and cancellation; checked between reads.
    pub progress: Option<std::sync::Arc<Progress>>,
    /// Import run whose artifact ledger this ingest writes to.
    pub run_id: Option<i64>,
    /// Only the top level of a directory (default: recurse into subdirectories).
    pub no_recurse: bool,
    /// `ingest_csv`/`ingest_json` mark file starts and ends in `progress` themselves
    /// (the CLI). Off when the caller does that per file (the TUI import).
    pub track_files: bool,
    /// Source used when none is given and the locator is not a known source's.
    pub default_source: Option<String>,
}

pub use progress::{CountingReader, Estimate, Progress};

/// One unit of input (a file, a response, a query result), as it arrived.
pub struct Input {
    pub locator: String,
    /// A short name: the file name, the URL's last segment, the key.
    pub name: String,
    /// What it looks like (`csv`, `json`, `ndjson`, `text`, …): a hint, never enforced.
    pub format: String,
    pub content_hash: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IngestReport {
    pub locator: String,
    pub source: String,
    pub format: String,
    pub bytes: u64,
    /// The stored item's id (the existing one when skipped).
    pub id: String,
    pub skipped: bool,
    pub warnings: Vec<String>,
    pub elapsed_ms: u128,
}

impl IngestReport {
    pub fn summary(&self) -> String {
        let id = &self.id[..8.min(self.id.len())];
        if self.skipped {
            return format!("{}: skipped (already in source {} as {id}; use --force to store it again)", self.locator, self.source);
        }
        let fmt = if self.format.is_empty() { String::new() } else { format!(" ({})", self.format) };
        format!("{}\n  stored     {} bytes{fmt} in source {} as {id}\n  time       {} ms", self.locator, self.bytes, self.source, self.elapsed_ms)
    }
}

/// Hash a file's contents (BLAKE3, hex).
pub fn hash_file(path: &Path) -> Result<String> {
    let mut h = blake3::Hasher::new();
    let mut f = std::fs::File::open(path)?;
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().to_hex().to_string())
}

pub fn hash_bytes(b: &[u8]) -> String {
    blake3::hash(b).to_hex().to_string()
}

/// The format hint of a file name, by its extension (`csv`, `tsv`, `json`, `ndjson`,
/// `text`); empty when it says nothing.
pub fn format_of(name: &str) -> String {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "csv" | "tsv" | "json" | "ndjson" => ext,
        "jsonl" => "ndjson".into(),
        "txt" => "text".into(),
        _ => String::new(),
    }
}

/// The default source name for a path: the folder's name (a file's folder for a file).
pub fn source_name_for(path: &Path) -> Option<String> {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let dir = if abs.is_dir() { abs.as_path() } else { abs.parent()? };
    dir.file_name().map(|n| n.to_string_lossy().trim().to_string()).filter(|n| !n.is_empty() && !n.contains('|'))
}

/// An ingest session: one source for a file/directory run (or a service read).
pub struct Session<'a> {
    pub store: &'a mut Store,
    pub io: Interact,
    pub opts: IngestOptions,
    source_kind: String,
    source_locator: String,
    source: Option<Source>,
    /// Source name used when none is given and the locator is unknown (services, the
    /// folder name for files).
    pub default_source: Option<String>,
}

impl<'a> Session<'a> {
    pub fn new(store: &'a mut Store, io: Interact, opts: IngestOptions, source_kind: &str, source_locator: &str) -> Self {
        Self { store, io, opts, source_kind: source_kind.to_string(), source_locator: source_locator.to_string(), source: None, default_source: None }
    }

    /// `--source`, else the source already holding this locator, else the default (the
    /// folder name, the service host); a prompt only when there is none of these.
    fn resolve_source(&mut self) -> Result<(Source, bool)> {
        if let Some(s) = &self.source {
            return Ok((s.clone(), false));
        }
        let name = if let Some(n) = self.opts.source.clone() {
            Some(n)
        } else if let Some(s) = self.store.source_by_locator(&self.source_locator)? {
            self.source = Some(s.clone());
            return Ok((s, false));
        } else {
            self.default_source.clone().or_else(|| self.opts.default_source.clone())
        };
        let name = match name {
            Some(n) => n,
            None => self.prompt("source", &format!("Source name for {}", self.source_locator), "pass --source <NAME>")?,
        };
        let created = self.store.source_by_name(&name)?.is_none();
        let s = self.store.get_or_create_source(&name, &self.source_kind, Some(&self.source_locator))?;
        self.source = Some(s.clone());
        Ok((s, created))
    }

    /// Prompt with progress output paused.
    fn prompt(&self, what: &'static str, question: &str, hint: &str) -> Result<String> {
        if !self.io.interactive {
            return self.io.prompt(what, question, None, hint);
        }
        if let Some(p) = &self.opts.progress {
            p.pause();
        }
        let r = self.io.prompt(what, question, None, hint);
        if let Some(p) = &self.opts.progress {
            p.resume();
        }
        r
    }

    /// Store one input as it is, all or nothing.
    pub fn ingest(&mut self, input: Input) -> Result<IngestReport> {
        let started = std::time::Instant::now();
        self.store.begin_atomic()?;
        let file = input.locator.clone();
        let r = (|| {
            let (source, created) = self.resolve_source()?;
            let mut rep = IngestReport {
                locator: input.locator.clone(),
                source: source.name.clone(),
                format: input.format.clone(),
                bytes: input.bytes.len() as u64,
                ..Default::default()
            };
            if let Some(p) = &self.opts.progress {
                p.set_phase(progress::Phase::Store);
                p.parsed(1);
                if p.cancelled() {
                    return Err(Error::Aborted);
                }
            }
            let mut notes = vec![];
            if created {
                notes.push(("source", source.id.clone()));
            }
            match self.store.find_source_data(&source.id, &input.content_hash)? {
                Some(d) if !self.opts.force => {
                    rep.skipped = true;
                    rep.id = d.id;
                }
                _ => {
                    rep.id = self.store.add_source_data(
                        &source.id,
                        &crate::store::NewSourceData {
                            locator: &input.locator,
                            name: &input.name,
                            format: &input.format,
                            content_hash: &input.content_hash,
                            data: &input.bytes,
                            run_id: self.opts.run_id,
                        },
                    )?;
                    notes.push(("source_data", rep.id.clone()));
                }
            }
            if let Some(p) = &self.opts.progress {
                p.stored(1);
            }
            rep.elapsed_ms = started.elapsed().as_millis();
            Ok((rep, notes))
        })();
        match r {
            Ok((rep, notes)) => {
                if let Some(run) = self.opts.run_id {
                    for (kind, reference) in notes {
                        self.store
                            .add_artifact(run, &crate::store::Artifact { kind: kind.into(), reference, file: file.clone(), state: "kept".into() })?;
                    }
                }
                self.store.commit_atomic()?;
                Ok(rep)
            }
            Err(e) => {
                let _ = self.store.rollback_atomic();
                // A source created inside the rolled-back unit no longer exists.
                self.source = None;
                if let Some(run) = self.opts.run_id
                    && let Some(p) = self.opts.progress.as_ref().filter(|p| p.cancelled())
                {
                    let _ = self.store.record_cancel(
                        run,
                        p.cancel_at.load(std::sync::atomic::Ordering::Relaxed),
                        p.cancel_by_name(),
                        &file,
                        p.phase().name(),
                    );
                }
                Err(e)
            }
        }
    }
}

/// Extensions of every supported data file (CSV-like, then JSON-like).
pub const CSV_EXTS: &[&str] = &["csv", "tsv", "txt"];
pub const JSON_EXTS: &[&str] = &["json", "ndjson", "jsonl"];
pub const DATA_EXTS: &[&str] = &["csv", "tsv", "txt", "json", "ndjson", "jsonl"];

fn has_ext(p: &Path, exts: &[&str]) -> bool {
    p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).is_some_and(|e| exts.contains(&e.as_str()))
}

/// Is this a JSON-family data file (by extension)?
pub fn is_json_path(p: &Path) -> bool {
    has_ext(p, JSON_EXTS)
}

/// Files under `path` with one of `exts`, sorted by path. A file is returned as is.
/// Directories are searched recursively unless `recursive` is false; hidden entries
/// (names starting with `.`) and symlinked directories are skipped.
pub fn find_files(path: &Path, exts: &[&str], recursive: bool) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(Error::invalid("--path", format!("{} does not exist", path.display())));
    }
    fn walk(dir: &Path, exts: &[&str], recursive: bool, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for e in std::fs::read_dir(dir)?.flatten() {
            let p = e.path();
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let ft = e.file_type()?;
            if ft.is_dir() {
                if recursive {
                    walk(&p, exts, recursive, out)?;
                }
            } else if p.is_file() && has_ext(&p, exts) {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut v = vec![];
    walk(path, exts, recursive, &mut v)?;
    v.sort();
    if v.is_empty() {
        return Err(Error::invalid("--path", format!("no {} files in {}", exts.join("/"), path.display())));
    }
    Ok(v)
}

/// Expand a path into files with one of `exts` (recursively), sorted.
pub fn expand_path(path: &Path, exts: &[&str]) -> Result<Vec<PathBuf>> {
    find_files(path, exts, true)
}

pub(crate) fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

/// Ingest CSV file(s): stored as they are.
pub fn ingest_csv(store: &mut Store, io: Interact, opts: IngestOptions, path: &Path) -> Result<Vec<IngestReport>> {
    let files = find_files(path, CSV_EXTS, !opts.no_recurse)?;
    ingest_files(store, io, opts, path, &files)
}

/// Ingest `files` (found under `path`) one at a time, each stored as is and all or
/// nothing, reporting per-file and overall progress through `opts.progress`.
pub fn ingest_files(store: &mut Store, io: Interact, opts: IngestOptions, path: &Path, files: &[PathBuf]) -> Result<Vec<IngestReport>> {
    let kind = if path.is_dir() { "directory" } else { "file" };
    let prog = opts.progress.clone().filter(|_| opts.track_files);
    let sizes: Vec<u64> = files.iter().map(|f| f.metadata().map(|m| m.len()).unwrap_or(0)).collect();
    if let Some(p) = &prog {
        p.bytes_total.store(sizes.iter().sum(), std::sync::atomic::Ordering::Relaxed);
    }
    let mut s = Session::new(store, io, opts, kind, &canonical(path));
    s.default_source = source_name_for(path);
    let mut out = Vec::new();
    for (i, f) in files.iter().enumerate() {
        // a cancel shows up as the next read failing (recorded in the read phase)
        if let Some(p) = &prog {
            p.begin_file(i as u64, sizes[i]);
        }
        let r = (|| {
            let bytes = match progress::read_counted(f, s.opts.progress.clone()) {
                Ok(b) => b,
                Err(e) => {
                    let Some(p) = s.opts.progress.as_ref().filter(|p| p.cancelled()) else { return Err(e.into()) };
                    // cancelled while reading: nothing of this file was stored
                    if let Some(run) = s.opts.run_id {
                        let at = p.cancel_at.load(std::sync::atomic::Ordering::Relaxed);
                        let _ = s.store.record_cancel(run, at, p.cancel_by_name(), &canonical(f), "read");
                    }
                    return Err(Error::Aborted);
                }
            };
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let input = Input { locator: canonical(f), format: format_of(&name), name, content_hash: hash_bytes(&bytes), bytes };
            s.ingest(input)
        })();
        if let Some(p) = &prog {
            p.end_file();
        }
        out.push(r?);
    }
    Ok(out)
}

/// Ingest every data file under `path` (CSV- and JSON-like by extension), each stored
/// as is; `on_file(index, total, file)` is called as each starts.
pub fn ingest_path(
    store: &mut Store,
    io: Interact,
    opts: IngestOptions,
    path: &Path,
    mut on_file: impl FnMut(usize, usize, &Path),
) -> Result<Vec<IngestReport>> {
    let files = find_files(path, DATA_EXTS, !opts.no_recurse)?;
    let mut opts = opts;
    opts.track_files = true;
    let n = files.len();
    for (i, f) in files.iter().enumerate() {
        on_file(i, n, f);
    }
    ingest_files(store, io, opts, path, &files)
}

/// Run `f` as one tracked import run: an `import_runs` row (origin `tui`/`cli`), the
/// artifact ledger, cleanup of empty leftovers, and the final status (`completed`,
/// `cancelled` or `failed`).
pub fn tracked<F>(store: &mut Store, origin: &str, files_total: usize, mut opts: IngestOptions, f: F) -> Result<Vec<IngestReport>>
where
    F: FnOnce(&mut Store, IngestOptions) -> Result<Vec<IngestReport>>,
{
    let run = store.begin_import_run(origin, files_total)?;
    opts.run_id = Some(run);
    let prog = opts.progress.clone();
    let mut r = f(store, opts);
    if let Some(p) = prog.as_ref().filter(|p| p.cancelled()) {
        // Cancelled outside a file's transaction (while reading, or between files).
        if store.import_run(run)?.is_some_and(|x| x.cancel_requested_at.is_none()) {
            store.record_cancel(run, p.cancel_at.load(std::sync::atomic::Ordering::Relaxed), p.cancel_by_name(), "", p.phase().name())?;
        }
        if r.is_err() {
            r = Err(Error::Aborted);
        }
    }
    let removed = store.cleanup_run(run).unwrap_or(0);
    let kept = store.import_artifacts(run).map(|a| a.iter().filter(|a| a.kind == "source_data" && a.state == "kept").count()).unwrap_or(0);
    let (status, note) = match &r {
        Ok(_) => ("completed", String::new()),
        Err(Error::Aborted) => ("cancelled", format!("current file rolled back; {removed} empty leftover(s) removed")),
        Err(e) => ("failed", e.to_string()),
    };
    store.finish_import_run(run, status, kept, &note)?;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::open_in_memory().unwrap()
    }

    #[test]
    fn files_are_stored_byte_for_byte_without_prompts() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("math");
        std::fs::create_dir(&dir).unwrap();
        // nothing here looks like bars, and the names say nothing about symbols
        std::fs::write(dir.join("BTC_5m_combined_part1.csv"), "a;b\n1;x\n").unwrap();
        std::fs::write(dir.join("notes.json"), "{\"not\": [\"an\", \"array of bars\"]}").unwrap();
        let mut s = store();
        // a batch session errors on any prompt: none may be asked
        let r = ingest_path(&mut s, Interact::batch(false), IngestOptions::default(), &dir, |_, _, _| {}).unwrap();
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|r| r.source == "math" && !r.skipped), "{r:?}");
        let src = s.source_by_name("math").unwrap().unwrap();
        let items = s.source_data(&src.id).unwrap();
        assert_eq!(items.iter().map(|i| i.format.as_str()).collect::<Vec<_>>(), ["csv", "json"]);
        assert_eq!(s.source_data_bytes(&items[0].id).unwrap(), b"a;b\n1;x\n");
        // no bars, series or symbols were made
        let n = |sql: &str| s.conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
        assert_eq!((n("SELECT COUNT(*) FROM m_bars"), n("SELECT COUNT(*) FROM series_stats"), n("SELECT COUNT(*) FROM ingest_runs")), (0, 0, 0));
        // the same bytes again are skipped, unless --force
        let again = ingest_csv(&mut s, Interact::batch(false), IngestOptions::default(), &dir).unwrap();
        assert!(again[0].skipped && again[0].summary().contains("skipped"));
        let o = IngestOptions { force: true, ..Default::default() };
        assert!(!ingest_csv(&mut s, Interact::batch(false), o, &dir).unwrap()[0].skipped);
        assert_eq!(s.source_data(&src.id).unwrap().len(), 3);
    }

    #[test]
    fn source_defaults_to_the_folder_and_only_a_rootless_path_would_ask() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("prices");
        std::fs::create_dir(&f).unwrap();
        std::fs::write(f.join("x.csv"), "1\n").unwrap();
        assert_eq!(source_name_for(&f.join("x.csv")).as_deref(), Some("prices"));
        assert_eq!(source_name_for(&f).as_deref(), Some("prices"));
        assert_eq!(source_name_for(Path::new("/")), None);
        let mut s = store();
        let o = IngestOptions { source: Some("given".into()), ..Default::default() };
        assert_eq!(ingest_csv(&mut s, Interact::batch(false), o, &f).unwrap()[0].source, "given");
        // a service with no default and no --source asks for the source name, and nothing else
        let mut sess = Session::new(&mut s, Interact::batch(false), IngestOptions::default(), "service", "redis://h/k");
        let e = sess
            .ingest(Input {
                locator: "redis://h/k".into(),
                name: "k".into(),
                format: "ndjson".into(),
                content_hash: "x".into(),
                bytes: b"{}".to_vec(),
            })
            .unwrap_err();
        assert!(e.to_string().contains("--source"), "{e}");
    }

    #[test]
    fn formats_are_hints_from_names() {
        assert_eq!(format_of("a.CSV"), "csv");
        assert_eq!(format_of("a.jsonl"), "ndjson");
        assert_eq!(format_of("a.txt"), "text");
        assert_eq!(format_of("a.bin"), "");
    }
}
