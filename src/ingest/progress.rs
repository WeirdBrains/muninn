//! Byte-based ingest progress shared with a UI thread, and a counting reader.
//!
//! Each file contributes its size in bytes to the total. A file's own progress is
//! estimated from three phases: reading its bytes (counted as they are read), parsing
//! and validating, then storing bars. Storing dominates, so the read phase gets
//! [`READ_SHARE`] of the file, parsing [`PARSE_SHARE`] and storing the rest, scaled by
//! bars stored / bars to store. Overall progress = (bytes of finished files + the
//! current file's estimated bytes) / total bytes.
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, Ordering::Relaxed};
use std::sync::Arc;

pub const READ_SHARE: f64 = 0.10;
pub const PARSE_SHARE: f64 = 0.05;
/// The counting reader publishes at least every this many bytes.
pub const FLUSH_BYTES: u64 = 64 * 1024;

#[derive(Debug, Default)]
pub struct Progress {
    /// Bars stored / to store, all files (secondary stat).
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
    /// Sum of all file sizes in the run (set by the caller; 0 = unknown).
    pub bytes_total: AtomicU64,
    /// Bytes of files already finished.
    pub bytes_before: AtomicU64,
    /// Current file: index, size, bytes read, parsed flag, bars stored / to store.
    pub file_index: AtomicU64,
    pub file_size: AtomicU64,
    pub file_read: AtomicU64,
    pub file_parsed: AtomicBool,
    pub file_bars_done: AtomicU64,
    pub file_bars_total: AtomicU64,
    /// Current phase (see [`Phase`]).
    pub phase: AtomicU8,
    /// When a cancel was asked for (ms since epoch) and by whom (1 = TUI, 2 = CLI).
    pub cancel_at: AtomicI64,
    pub cancel_by: AtomicU8,
}

/// What the importer is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Idle = 0,
    Read = 1,
    Parse = 2,
    Store = 3,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Read => "read",
            Phase::Parse => "parse",
            Phase::Store => "store",
        }
    }
}

/// A snapshot for display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub file_index: u64,
    /// 0..=1 for the current file.
    pub file: f64,
    /// 0..=1 for the whole run.
    pub overall: f64,
    /// Estimated bytes done / total.
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl Progress {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Relaxed)
    }
    /// Ask the import to stop; `by` is `tui` or `cli`. The first request wins.
    pub fn request_cancel(&self, by: &str) {
        if !self.cancel.swap(true, Relaxed) {
            self.cancel_at.store(crate::store::now_ms(), Relaxed);
            self.cancel_by.store(if by == "cli" { 2 } else { 1 }, Relaxed);
        }
    }
    pub fn cancel_by_name(&self) -> &'static str {
        match self.cancel_by.load(Relaxed) {
            2 => "cli",
            1 => "tui",
            _ => "unknown",
        }
    }
    pub fn set_phase(&self, p: Phase) {
        self.phase.store(p as u8, Relaxed);
    }
    pub fn phase(&self) -> Phase {
        match self.phase.load(Relaxed) {
            1 => Phase::Read,
            2 => Phase::Parse,
            3 => Phase::Store,
            _ => Phase::Idle,
        }
    }
    /// Bars (done, total).
    pub fn get(&self) -> (u64, u64) {
        (self.done.load(Relaxed), self.total.load(Relaxed))
    }
    /// Start file `index` of `size` bytes.
    pub fn begin_file(&self, index: u64, size: u64) {
        self.file_index.store(index, Relaxed);
        self.file_read.store(0, Relaxed);
        self.file_parsed.store(false, Relaxed);
        self.file_bars_done.store(0, Relaxed);
        self.file_bars_total.store(0, Relaxed);
        self.file_size.store(size, Relaxed);
    }
    /// The current file is complete (or failed); its bytes count as done.
    pub fn end_file(&self) {
        self.bytes_before.fetch_add(self.file_size.swap(0, Relaxed), Relaxed);
        self.file_read.store(0, Relaxed);
    }
    pub fn add_read(&self, n: u64) {
        self.file_read.fetch_add(n, Relaxed);
    }
    pub fn parsed(&self, bars: u64) {
        self.file_parsed.store(true, Relaxed);
        self.file_bars_total.store(bars, Relaxed);
        self.total.fetch_add(bars, Relaxed);
    }
    pub fn stored(&self, bars: u64) {
        self.file_bars_done.fetch_add(bars, Relaxed);
        self.done.fetch_add(bars, Relaxed);
    }

    /// Current file fraction from its phase counters.
    pub fn file_fraction(&self) -> f64 {
        let size = self.file_size.load(Relaxed);
        let read = if size == 0 { 0.0 } else { (self.file_read.load(Relaxed) as f64 / size as f64).min(1.0) };
        let mut f = READ_SHARE * read;
        if self.file_parsed.load(Relaxed) {
            let t = self.file_bars_total.load(Relaxed);
            let store = if t == 0 { 1.0 } else { (self.file_bars_done.load(Relaxed) as f64 / t as f64).min(1.0) };
            f = READ_SHARE + PARSE_SHARE + (1.0 - READ_SHARE - PARSE_SHARE) * store;
        }
        f
    }

    pub fn estimate(&self) -> Estimate {
        let file = self.file_fraction();
        let size = self.file_size.load(Relaxed);
        let before = self.bytes_before.load(Relaxed);
        let total = self.bytes_total.load(Relaxed).max(before + size);
        let done = before + (file * size as f64) as u64;
        Estimate {
            file_index: self.file_index.load(Relaxed),
            file,
            overall: if total == 0 { 0.0 } else { (done as f64 / total as f64).min(1.0) },
            bytes_done: done,
            bytes_total: total,
        }
    }
}

/// Counts bytes read; publishes to a [`Progress`] every [`FLUSH_BYTES`] and on drop.
pub struct CountingReader<R> {
    inner: R,
    pub count: u64,
    pending: u64,
    progress: Option<Arc<Progress>>,
}

impl<R: Read> CountingReader<R> {
    pub fn new(inner: R, progress: Option<Arc<Progress>>) -> Self {
        Self { inner, count: 0, pending: 0, progress }
    }
    fn flush_progress(&mut self) {
        if let Some(p) = &self.progress
            && self.pending > 0
        {
            p.add_read(self.pending);
        }
        self.pending = 0;
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.progress.as_ref().is_some_and(|p| p.cancelled()) {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.inner.read(buf)?;
        self.count += n as u64;
        self.pending += n as u64;
        if self.pending >= FLUSH_BYTES || n == 0 {
            self.flush_progress();
        }
        Ok(n)
    }
}

impl<R> Drop for CountingReader<R> {
    fn drop(&mut self) {
        if let Some(p) = &self.progress
            && self.pending > 0
        {
            p.add_read(self.pending);
        }
    }
}

/// Read a whole file through a counting reader (64 KiB reads).
pub fn read_counted(path: &std::path::Path, progress: Option<Arc<Progress>>) -> std::io::Result<Vec<u8>> {
    let f = std::fs::File::open(path)?;
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut out = Vec::with_capacity(len as usize + 1);
    if let Some(p) = &progress {
        p.set_phase(Phase::Read);
    }
    let mut r = CountingReader::new(f, progress.clone());
    let mut buf = vec![0u8; FLUSH_BYTES as usize];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
    }
    drop(r);
    if let Some(p) = &progress {
        p.set_phase(Phase::Parse);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counting_reader_counts_and_throttles() {
        let data = vec![7u8; 200_000];
        let p = Arc::new(Progress::default());
        let mut r = CountingReader::new(&data[..], Some(p.clone()));
        let mut buf = [0u8; 1000];
        let mut seen = std::collections::BTreeSet::new();
        while r.read(&mut buf).unwrap() > 0 {
            seen.insert(p.file_read.load(Relaxed));
        }
        assert_eq!(r.count, 200_000);
        assert_eq!(p.file_read.load(Relaxed), 200_000);
        // published only at 64 KiB steps plus the final flush: 0, 65_000, 131_000, 197_000, 200_000
        assert!(seen.len() <= 6, "{seen:?}");
        assert!(seen.iter().all(|v| *v == 200_000 || *v % 1000 == 0));
    }

    #[test]
    fn reader_stops_when_cancelled() {
        let p = Arc::new(Progress::default());
        let mut r = CountingReader::new(&[1u8; 10][..], Some(p.clone()));
        let mut b = [0u8; 4];
        assert_eq!(r.read(&mut b).unwrap(), 4);
        p.cancel.store(true, Relaxed);
        assert!(r.read(&mut b).is_err());
    }

    #[test]
    fn drop_flushes_remainder() {
        let p = Arc::new(Progress::default());
        {
            let mut r = CountingReader::new(&b"hello"[..], Some(p.clone()));
            let mut b = [0u8; 3];
            r.read_exact(&mut b).unwrap();
        }
        assert_eq!(p.file_read.load(Relaxed), 3);
    }

    #[test]
    fn estimate_aggregates_files_by_bytes() {
        let p = Progress::default();
        p.bytes_total.store(1000, Relaxed);
        p.begin_file(0, 400);
        assert_eq!(p.estimate().overall, 0.0);
        p.add_read(200); // half read -> 5% of the file
        let e = p.estimate();
        assert!((e.file - 0.05).abs() < 1e-9 && e.bytes_done == 20, "{e:?}");
        p.add_read(200);
        p.parsed(100);
        assert!((p.file_fraction() - 0.15).abs() < 1e-9);
        p.stored(50);
        assert!((p.file_fraction() - (0.15 + 0.85 * 0.5)).abs() < 1e-9);
        p.stored(50);
        assert!((p.file_fraction() - 1.0).abs() < 1e-9);
        p.end_file();
        let e = p.estimate();
        assert_eq!((e.bytes_done, e.bytes_total), (400, 1000));
        p.begin_file(1, 600);
        p.add_read(600);
        let e = p.estimate();
        assert_eq!(e.file_index, 1);
        assert_eq!(e.bytes_done, 460);
        assert!((e.overall - 0.46).abs() < 1e-9);
        p.parsed(0); // nothing to store: file complete
        assert!((p.estimate().overall - 1.0).abs() < 1e-9);
        assert_eq!(p.get(), (100, 100));
    }

    #[test]
    fn read_counted_reads_whole_file() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("x.csv");
        std::fs::write(&f, vec![b'a'; 150_000]).unwrap();
        let p = Arc::new(Progress::default());
        assert_eq!(read_counted(&f, Some(p.clone())).unwrap().len(), 150_000);
        assert_eq!(p.file_read.load(Relaxed), 150_000);
    }
}
