//! Import (ingest files as they are) and export (bars as CSV, Parquet or JSON).
//! Thin wrappers over `muninn-core`; the unlocked key stays in Rust.
use super::data::open_write;
use muninn_core::error::Error;
use muninn_core::export::{self, Format};
use muninn_core::ingest::{self, IngestOptions, IngestReport, Progress};
use muninn_core::interact::Interact;
use muninn_core::store::Store;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One stored file from an import.
#[derive(Debug)]
pub struct IngestFileReport {
    pub locator: String,
    pub source: String,
    pub format: String,
    pub bytes: i64,
    pub id: String,
    pub skipped: bool,
    pub elapsed_ms: i64,
}

impl From<IngestReport> for IngestFileReport {
    fn from(r: IngestReport) -> Self {
        Self {
            locator: r.locator,
            source: r.source,
            format: r.format,
            bytes: r.bytes as i64,
            id: r.id,
            skipped: r.skipped,
            elapsed_ms: r.elapsed_ms as i64,
        }
    }
}

#[derive(Debug)]
pub struct IngestOutcome {
    pub files: Vec<IngestFileReport>,
    pub cancelled: bool,
}

/// Live import progress (byte-based), for the app to poll.
#[derive(Debug)]
pub struct IngestProgressSnap {
    pub file_index: i64,
    pub file: f64,
    pub overall: f64,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub cancelled: bool,
}

#[derive(Debug)]
pub struct ExportOutcome {
    pub path: String,
    pub format: String,
    pub rows: i64,
}

static INGEST: Mutex<Option<Arc<Progress>>> = Mutex::new(None);

fn take_progress() {
    INGEST.lock().unwrap_or_else(|e| e.into_inner()).take();
}

/// Import every data file at `path` (a file or a folder). Files are stored byte for
/// byte in a source; no bars are made. `source` overrides the folder name (else `local`).
pub fn ingest_path(path: String, source: Option<String>) -> anyhow::Result<IngestOutcome> {
    let mut store = open_write()?;
    let progress = ingest_progress_handle()?;
    let outcome = ingest_path_of(&mut store, Path::new(&path), source, Some(progress));
    take_progress();
    outcome
}

fn ingest_progress_handle() -> anyhow::Result<Arc<Progress>> {
    let mut g = INGEST.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        anyhow::bail!("an import is already running");
    }
    let progress = Arc::new(Progress::default());
    *g = Some(progress.clone());
    Ok(progress)
}

pub(crate) fn ingest_path_of(
    store: &mut Store,
    path: &Path,
    source: Option<String>,
    progress: Option<Arc<Progress>>,
) -> anyhow::Result<IngestOutcome> {
    let files = ingest::find_files(path, ingest::DATA_EXTS, true)?;
    let opts = IngestOptions { source, progress, track_files: true, default_source: Some("local".into()), ..Default::default() };
    match ingest::tracked(store, "app", files.len(), opts, |st, o| ingest::ingest_files(st, Interact::batch(true), o, path, &files)) {
        Ok(reports) => Ok(IngestOutcome { files: reports.into_iter().map(Into::into).collect(), cancelled: false }),
        Err(Error::Aborted) => Ok(IngestOutcome { files: vec![], cancelled: true }),
        Err(e) => Err(e.into()),
    }
}

/// Ask a running import to stop (the current file is rolled back; finished files stay).
#[flutter_rust_bridge::frb(sync)]
pub fn cancel_ingest() {
    if let Some(p) = INGEST.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        p.request_cancel("app");
    }
}

/// Snapshot of the running import, if any.
#[flutter_rust_bridge::frb(sync)]
pub fn ingest_progress() -> Option<IngestProgressSnap> {
    INGEST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|p| {
        let e = p.estimate();
        IngestProgressSnap {
            file_index: e.file_index as i64,
            file: e.file,
            overall: e.overall,
            bytes_done: e.bytes_done as i64,
            bytes_total: e.bytes_total as i64,
            cancelled: p.cancelled(),
        }
    })
}

fn parse_format(format: &str) -> anyhow::Result<Format> {
    match format.to_ascii_lowercase().as_str() {
        "csv" => Ok(Format::Csv),
        "parquet" => Ok(Format::Parquet),
        "json" => Ok(Format::Json),
        other => anyhow::bail!("unsupported export format `{other}` (use csv, parquet or json)"),
    }
}

/// Write matching bars to `path` as CSV, Parquet or JSON (`mnnn export`).
pub fn export_bars(
    path: String,
    format: String,
    source: Option<String>,
    symbol: Option<String>,
    duration: Option<String>,
    from: Option<String>,
    to: Option<String>,
) -> anyhow::Result<ExportOutcome> {
    let store = open_write()?;
    export_of(&store, Path::new(&path), &format, source.as_deref(), symbol.as_deref(), duration.as_deref(), from.as_deref(), to.as_deref())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn export_of(
    store: &Store,
    path: &Path,
    format: &str,
    source: Option<&str>,
    symbol: Option<&str>,
    duration: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> anyhow::Result<ExportOutcome> {
    let fmt = parse_format(format)?;
    let f = export::filter(store, source, symbol, duration, from, to)?;
    let rows = export::export_recorded(store, &f, fmt, path, "app")?;
    Ok(ExportOutcome { path: path.display().to_string(), format: format.to_ascii_lowercase(), rows: rows as i64 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::model::{Bar, Duration, Symbol};
    use muninn_core::store::Store;
    use std::path::PathBuf;

    fn csv_dir() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::TempDir::new().unwrap();
        let dir = d.path().join("math");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("notes.csv"), "a;b\n1;x\n").unwrap();
        std::fs::write(dir.join("meta.json"), "{\"ok\":true}\n").unwrap();
        (d, dir)
    }

    #[test]
    fn ingest_stores_files_as_they_are() {
        let (_tmp, dir) = csv_dir();
        let mut s = Store::open_in_memory().unwrap();
        let out = ingest_path_of(&mut s, &dir, None, None).unwrap();
        assert!(!out.cancelled && out.files.len() == 2, "{:?}", out.files.len());
        assert!(out.files.iter().all(|f| f.source == "math" && !f.skipped));
        let src = s.source_by_name("math").unwrap().unwrap();
        let items = s.source_data(&src.id).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(s.source_data_bytes(&items.iter().find(|i| i.format == "csv").unwrap().id).unwrap(), b"a;b\n1;x\n");
        assert!(s.series().unwrap().is_empty(), "ingest must not make bars");
    }

    #[test]
    fn ingest_source_override_and_skip_duplicates() {
        let (_tmp, dir) = csv_dir();
        let mut s = Store::open_in_memory().unwrap();
        let first = ingest_path_of(&mut s, &dir, Some("given".into()), None).unwrap();
        assert!(first.files.iter().all(|f| f.source == "given"));
        let again = ingest_path_of(&mut s, &dir, Some("given".into()), None).unwrap();
        assert!(again.files.iter().all(|f| f.skipped));
    }

    #[test]
    fn cancel_before_start_stores_nothing() {
        let (_tmp, dir) = csv_dir();
        let mut s = Store::open_in_memory().unwrap();
        let progress = Arc::new(Progress::default());
        progress.request_cancel("app");
        let out = ingest_path_of(&mut s, &dir, None, Some(progress)).unwrap();
        assert!(out.cancelled && out.files.is_empty());
        assert!(
            s.sources().unwrap().is_empty() || s.source_by_name("math").unwrap().is_none() || {
                let src = s.source_by_name("math").unwrap().unwrap();
                s.source_data(&src.id).unwrap().is_empty()
            }
        );
    }

    #[test]
    fn export_writes_csv_and_json_of_bars() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("coinbase", "csv", None).unwrap();
        let t0 = 1_704_067_200_000i64;
        let bars = vec![Bar { ts: t0, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 3.0, flags: 0 }];
        s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(300), &bars).unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let csv = dir.path().join("out.csv");
        let json = dir.path().join("out.json");
        let c = export_of(&s, &csv, "csv", Some("coinbase"), Some("BTC/USD"), Some("5m"), None, None).unwrap();
        assert_eq!((c.rows, c.format.as_str()), (1, "csv"));
        let text = std::fs::read_to_string(&csv).unwrap();
        assert!(text.contains("timestamp") && text.contains("coinbase") && text.contains("BTC"), "{text}");
        let j = export_of(&s, &json, "json", None, None, None, Some(&t0.to_string()), None).unwrap();
        assert_eq!(j.rows, 1);
        assert!(std::fs::read_to_string(&json).unwrap().contains("BTC"));
    }

    #[test]
    fn export_rejects_unknown_format() {
        let s = Store::open_in_memory().unwrap();
        let err = export_of(&s, Path::new("/tmp/x.bin"), "xlsx", None, None, None, None, None).unwrap_err();
        assert!(err.to_string().contains("unsupported export format"), "{err}");
    }

    #[test]
    fn ingest_and_export_need_unlock() {
        assert!(ingest_path("/no/such".into(), None).is_err());
        assert!(export_bars("/tmp/x.csv".into(), "csv".into(), None, None, None, None, None).is_err());
        assert!(ingest_progress().is_none());
        cancel_ingest(); // nothing running: fine
    }
}
