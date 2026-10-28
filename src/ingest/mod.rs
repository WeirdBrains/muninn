//! Ingestion pipeline: read -> source -> symbol -> timestamp -> duration -> validate -> upsert -> report.
pub mod csv;
pub mod frame;
pub mod json;
pub mod progress;
pub mod service;
pub mod time;

use crate::error::{Error, Result};
use crate::interact::Interact;
use crate::model::{Duration, Source, Symbol};
use crate::store::{now_ms, Store, UpsertCounts};
use frame::{derive_duration, gaps, parse_map, Columns, Reject};
use polars::prelude::DataFrame;
use std::io::Write;
use std::path::{Path, PathBuf};

/// User-supplied ingest options (flags or TUI fields).
#[derive(Debug, Clone, Default)]
pub struct IngestOptions {
    pub source: Option<String>,
    pub symbol: Option<String>,
    pub duration: Option<String>,
    pub map: Vec<String>,
    pub rejects: Option<PathBuf>,
    pub force: bool,
    pub drop_zero_volume: bool,
    /// Live progress and cancellation (TUI import); checked per stored chunk.
    pub progress: Option<std::sync::Arc<Progress>>,
}

pub use progress::{CountingReader, Estimate, Progress};

/// One unit of input (a file, a response, a query result).
pub struct Input {
    pub locator: String,
    /// File name (or key) used for symbol/duration derivation.
    pub name_hint: Option<String>,
    pub content_hash: String,
    pub frame: DataFrame,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IngestReport {
    pub locator: String,
    pub source: String,
    pub symbol: String,
    pub duration: String,
    pub rows: u64,
    pub inserted: u64,
    pub duplicate: u64,
    pub rejected: u64,
    pub zero_volume: u64,
    pub dropped_zero_volume: u64,
    pub gaps: u64,
    pub missing_bars: u64,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub skipped: bool,
    pub warnings: Vec<String>,
    pub elapsed_ms: u128,
}

impl IngestReport {
    pub fn summary(&self) -> String {
        if self.skipped {
            return format!(
                "{}: skipped (identical input already ingested as {} {} {}; use --force)",
                self.locator, self.source, self.symbol, self.duration
            );
        }
        let range = match (self.first, self.last) {
            (Some(a), Some(b)) => format!("{} .. {}", time::fmt_ts(a), time::fmt_ts(b)),
            _ => "-".into(),
        };
        format!(
            "{loc}\n  series     {src} {sym} {dur}\n  rows       {rows}\n  inserted   {ins}\n  duplicate  {dup}\n  rejected   {rej}\n  zero-vol   {zv} (dropped {dz})\n  gaps       {g} ({m} missing bars)\n  coverage   {range} UTC\n  time       {t} ms",
            loc = self.locator,
            src = self.source,
            sym = self.symbol,
            dur = self.duration,
            rows = self.rows,
            ins = self.inserted,
            dup = self.duplicate,
            rej = self.rejected,
            zv = self.zero_volume,
            dz = self.dropped_zero_volume,
            g = self.gaps,
            m = self.missing_bars,
            t = self.elapsed_ms,
        )
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

/// The last path segment without a known data-file extension (`a/b/BTCUSDT_5m.csv` -> `BTCUSDT_5m`).
fn name_stem(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let lower = last.to_ascii_lowercase();
    for ext in [".csv", ".tsv", ".txt", ".json", ".ndjson", ".jsonl", ".db", ".sqlite", ".sqlite3", ".gz"] {
        if lower.ends_with(ext) {
            return last[..last.len() - ext.len()].to_string();
        }
    }
    last.to_string()
}

/// Derive a symbol from a file name against the known symbol list.
pub fn symbol_from_name(store: &Store, name: &str) -> Result<Option<Symbol>> {
    let stem = name_stem(name);
    let quotes = store.known_quotes()?;
    let qrefs: Vec<&str> = quotes.iter().map(String::as_str).collect();
    let tokens: Vec<&str> = stem.split(['_', '.', ' ', ':', '/']).collect();
    // Try separator-joined pairs first (BTC-USD), then concatenated (BTCUSDT), across tokens and adjacent token pairs.
    let mut candidates: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
    for w in tokens.windows(2) {
        candidates.push(format!("{}/{}", w[0], w[1]));
    }
    for c in candidates {
        if let Some(sym) = Symbol::parse_with(&c, &qrefs) {
            let sym = store.canonical_symbol(&sym)?;
            if store.is_known_asset(&sym.base)? && store.is_known_asset(&sym.quote)? {
                return Ok(Some(sym));
            }
        }
    }
    Ok(None)
}

/// Derive a symbol from a file name using only the built-in symbol list (no store; for previews).
pub fn symbol_from_name_static(name: &str) -> Option<Symbol> {
    let stem = Path::new(name).file_stem()?.to_string_lossy().to_string();
    let tokens: Vec<&str> = stem.split(['_', '.', ' ']).collect();
    tokens.iter().find_map(|t| {
        Symbol::parse_with(t, crate::model::KNOWN_QUOTES).filter(|s| crate::model::KNOWN_BASES.contains(&s.base.as_str()))
    })
}

/// Derive a duration from a file name token such as `5m`, `1h`, `15min`.
pub fn duration_from_name(name: &str) -> Option<Duration> {
    let stem = name_stem(name);
    stem.split(['_', '.', '-', ' ', ':', '/'])
        .filter(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .find_map(|t| Duration::parse(t).ok())
}

/// An ingest session: answers to prompts are reused for the rest of a file/directory run.
pub struct Session<'a> {
    pub store: &'a mut Store,
    pub io: Interact,
    pub opts: IngestOptions,
    source_kind: String,
    source_locator: String,
    source: Option<Source>,
    prompted_symbol: Option<Symbol>,
    prompted_duration: Option<Duration>,
    rejects_started: bool,
    /// Source name used when none is given and the locator is unknown (services).
    pub default_source: Option<String>,
}

impl<'a> Session<'a> {
    pub fn new(store: &'a mut Store, io: Interact, opts: IngestOptions, source_kind: &str, source_locator: &str) -> Self {
        Self {
            store,
            io,
            opts,
            source_kind: source_kind.to_string(),
            source_locator: source_locator.to_string(),
            source: None,
            prompted_symbol: None,
            prompted_duration: None,
            rejects_started: false,
            default_source: None,
        }
    }

    fn resolve_source(&mut self) -> Result<Source> {
        if let Some(s) = &self.source {
            return Ok(s.clone());
        }
        let s = if let Some(name) = self.opts.source.clone() {
            self.store.get_or_create_source(&name, &self.source_kind, Some(&self.source_locator))?
        } else if let Some(s) = self.store.source_by_locator(&self.source_locator)? {
            s
        } else if let Some(name) = self.default_source.clone() {
            self.store.get_or_create_source(&name, &self.source_kind, Some(&self.source_locator))?
        } else {
            let name = self.io.prompt(
                "source",
                &format!("Source name for {}", self.source_locator),
                None,
                "pass --source <NAME>",
            )?;
            self.store.get_or_create_source(&name, &self.source_kind, Some(&self.source_locator))?
        };
        self.source = Some(s.clone());
        Ok(s)
    }

    fn resolve_symbol(&mut self, name_hint: Option<&str>) -> Result<Symbol> {
        if let Some(s) = &self.opts.symbol {
            let sym = Symbol::parse(s)?;
            return self.store.canonical_symbol(&sym);
        }
        if let Some(n) = name_hint
            && let Some(sym) = symbol_from_name(self.store, n)? {
                return Ok(sym);
            }
        if let Some(s) = &self.prompted_symbol {
            return Ok(s.clone());
        }
        let ans = self.io.prompt(
            "symbol",
            &format!("Symbol (BASE/QUOTE) for {}", name_hint.unwrap_or("input")),
            None,
            "could not derive the symbol from the name; pass --symbol BASE/QUOTE",
        )?;
        let sym = self.store.canonical_symbol(&Symbol::parse(&ans)?)?;
        self.store.add_symbol(&sym, &[])?;
        self.prompted_symbol = Some(sym.clone());
        Ok(sym)
    }

    fn resolve_duration(&mut self, name_hint: Option<&str>, ts: &[i64], warnings: &mut Vec<String>) -> Result<Duration> {
        if let Some(d) = &self.opts.duration {
            return Duration::parse(d);
        }
        if let Some(d) = name_hint.and_then(duration_from_name) {
            return Ok(d);
        }
        if let Some(d) = derive_duration(ts) {
            if d.share < 0.9 {
                warnings.push(format!(
                    "derived duration {} covers only {:.1}% of timestamp differences",
                    d.duration,
                    d.share * 100.0
                ));
            }
            return Ok(d.duration);
        }
        if let Some(d) = self.prompted_duration {
            return Ok(d);
        }
        let ans = self.io.prompt("duration", "Bar duration (e.g. 5m)", None, "pass --duration (e.g. 5m)")?;
        let d = Duration::parse(&ans)?;
        self.prompted_duration = Some(d);
        Ok(d)
    }

    fn write_rejects(&mut self, locator: &str, rejects: &[Reject]) -> Result<()> {
        let Some(p) = self.opts.rejects.clone() else { return Ok(()) };
        let mut f = if self.rejects_started {
            std::fs::OpenOptions::new().append(true).open(&p)?
        } else {
            let mut f = std::fs::File::create(&p)?;
            writeln!(f, "input,row,timestamp,reason")?;
            self.rejects_started = true;
            f
        };
        for r in rejects {
            writeln!(
                f,
                "\"{}\",{},{},{}",
                locator.replace('"', "\"\""),
                r.row,
                r.timestamp.map(time::fmt_ts).unwrap_or_default(),
                r.reason
            )?;
        }
        Ok(())
    }

    /// Normalize, validate and store one input.
    pub fn ingest(&mut self, input: Input) -> Result<IngestReport> {
        let started = std::time::Instant::now();
        let started_ms = now_ms();
        let mut rep = IngestReport { locator: input.locator.clone(), ..Default::default() };
        let overrides = parse_map(&self.opts.map)?;
        let cols = Columns::from_frame(&input.frame, &overrides)?;
        rep.rows = cols.len() as u64;
        let source = self.resolve_source()?;
        let symbol = self.resolve_symbol(input.name_hint.as_deref())?;
        let valid_ts = cols.valid_ts();
        let duration = self.resolve_duration(input.name_hint.as_deref(), &valid_ts, &mut rep.warnings)?;
        rep.source = source.name.clone();
        rep.symbol = symbol.display();
        rep.duration = duration.to_string();

        if !self.opts.force && self.store.find_run(&input.content_hash, &source, &symbol, duration)?.is_some() {
            rep.skipped = true;
            rep.elapsed_ms = started.elapsed().as_millis();
            return Ok(rep);
        }

        let v = cols.validate(duration, self.opts.drop_zero_volume);
        rep.rejected = v.rejects.len() as u64;
        rep.zero_volume = v.zero_volume;
        rep.dropped_zero_volume = v.dropped_zero_volume;
        if !v.rejects.is_empty() {
            self.write_rejects(&input.locator, &v.rejects)?;
            let sample: Vec<String> = v.rejects.iter().take(3).map(|r| format!("row {}: {}", r.row, r.reason)).collect();
            if !self.io.confirm(&format!(
                "{} rows of {} failed validation ({}); drop them and continue?",
                v.rejects.len(),
                input.locator,
                sample.join("; ")
            ))? {
                return Err(Error::Aborted);
            }
        }
        let ts: Vec<i64> = v.bars.iter().map(|b| b.ts).collect();
        let (g, m) = gaps(&ts, duration);
        rep.gaps = g;
        rep.missing_bars = m;
        rep.first = ts.iter().min().copied();
        rep.last = ts.iter().max().copied();

        let mut counts = UpsertCounts::default();
        let prog = self.opts.progress.clone();
        if let Some(p) = &prog {
            p.parsed(v.bars.len() as u64);
        }
        for chunk in v.bars.chunks(50_000) {
            if prog.as_ref().is_some_and(|p| p.cancelled()) {
                return Err(Error::Aborted);
            }
            // Progress inside the chunk's transaction (every 4096 rows), the rest at the end.
            let mut reported = 0u64;
            let c = match &prog {
                Some(p) => self.store.upsert_bars_with(&source, &symbol, duration, chunk, &mut |n| {
                    reported += n;
                    p.stored(n);
                })?,
                None => self.store.upsert_bars(&source, &symbol, duration, chunk)?,
            };
            if let Some(p) = &prog {
                p.stored(chunk.len() as u64 - reported);
            }
            counts.inserted += c.inserted;
            counts.duplicate += c.duplicate;
        }
        rep.inserted = counts.inserted;
        rep.duplicate = counts.duplicate;
        self.store
            .record_run(&source, &input.locator, &input.content_hash, &symbol, duration, started_ms, counts, rep.rejected)?;
        rep.elapsed_ms = started.elapsed().as_millis();
        Ok(rep)
    }
}

/// Expand a path into files with one of `exts`, sorted.
pub fn expand_path(path: &Path, exts: &[&str]) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(Error::invalid("--path", format!("{} does not exist", path.display())));
    }
    let mut v: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).is_some_and(|e| exts.contains(&e.as_str()))
        })
        .collect();
    v.sort();
    if v.is_empty() {
        return Err(Error::invalid("--path", format!("no {} files in {}", exts.join("/"), path.display())));
    }
    Ok(v)
}

pub(crate) fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

/// Ingest CSV file(s).
pub fn ingest_csv(store: &mut Store, io: Interact, opts: IngestOptions, path: &Path, csv: &csv::CsvOptions) -> Result<Vec<IngestReport>> {
    let files = expand_path(path, &["csv", "tsv", "txt"])?;
    let kind = if path.is_dir() { "directory" } else { "file" };
    let mut s = Session::new(store, io, opts, kind, &canonical(path));
    let mut out = Vec::new();
    let prog = s.opts.progress.clone();
    for f in files {
        let bytes = progress::read_counted(&f, prog.clone())?;
        let content_hash = hash_bytes(&bytes);
        let frame = csv::read_csv_bytes(bytes, csv)?;
        let input = Input { locator: canonical(&f), name_hint: f.file_name().map(|n| n.to_string_lossy().to_string()), content_hash, frame };
        out.push(s.ingest(input)?);
    }
    Ok(out)
}
