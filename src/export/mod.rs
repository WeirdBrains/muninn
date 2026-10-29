//! Export bars to CSV, Parquet or JSON via Polars; documents (journals, reports, projects) in
//! [`docs`], archives of whole projects in [`archive`].
pub mod archive;
pub mod docs;
pub mod files;
use crate::error::{Error, Result};
use crate::ingest::time::{fmt_ts, parse_ts};
use crate::model::{Duration, FLAG_ZERO_VOLUME, StoredBar, Symbol};
use crate::store::{BarFilter, Store};
use polars::prelude::*;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Parquet,
    Json,
}

/// Build a filter from user strings.
pub fn filter(
    store: &Store,
    source: Option<&str>,
    symbol: Option<&str>,
    duration: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> Result<BarFilter> {
    let ts = |s: &str, what: &'static str| parse_ts(s).ok_or_else(|| Error::invalid(what, format!("`{s}` is not a timestamp")));
    Ok(BarFilter {
        source: source.map(str::to_string),
        symbol: symbol.map(Symbol::parse).transpose()?.map(|s| store.canonical_symbol(&s)).transpose()?,
        duration: duration.map(Duration::parse).transpose()?,
        from: from.map(|s| ts(s, "--from")).transpose()?,
        to: to.map(|s| ts(s, "--to")).transpose()?,
        ..Default::default()
    })
}

pub fn to_frame(bars: &[StoredBar]) -> Result<DataFrame> {
    let df = df!(
        "timestamp" => bars.iter().map(|b| b.bar.ts).collect::<Vec<_>>(),
        "datetime_utc" => bars.iter().map(|b| fmt_ts(b.bar.ts)).collect::<Vec<_>>(),
        "source" => bars.iter().map(|b| b.source.clone()).collect::<Vec<_>>(),
        "symbol" => bars.iter().map(|b| b.symbol.display()).collect::<Vec<_>>(),
        "base" => bars.iter().map(|b| b.symbol.base.clone()).collect::<Vec<_>>(),
        "quote" => bars.iter().map(|b| b.symbol.quote.clone()).collect::<Vec<_>>(),
        "duration" => bars.iter().map(|b| b.duration.to_string()).collect::<Vec<_>>(),
        "duration_s" => bars.iter().map(|b| b.duration.0).collect::<Vec<_>>(),
        "open" => bars.iter().map(|b| b.bar.open).collect::<Vec<_>>(),
        "high" => bars.iter().map(|b| b.bar.high).collect::<Vec<_>>(),
        "low" => bars.iter().map(|b| b.bar.low).collect::<Vec<_>>(),
        "close" => bars.iter().map(|b| b.bar.close).collect::<Vec<_>>(),
        "volume" => bars.iter().map(|b| b.bar.volume).collect::<Vec<_>>(),
        "zero_volume" => bars.iter().map(|b| b.bar.flags & FLAG_ZERO_VOLUME != 0).collect::<Vec<_>>(),
    )?;
    Ok(df)
}

/// Write bars; `out` of `-` means stdout (csv/json only). Returns rows written.
pub fn export(store: &Store, f: &BarFilter, format: Format, out: &Path) -> Result<usize> {
    let bars = store.query_bars(f)?;
    let mut df = to_frame(&bars)?;
    let stdout = out.as_os_str() == "-";
    if stdout && format == Format::Parquet {
        return Err(Error::invalid("--out", "parquet cannot be written to stdout"));
    }
    let mut w: Box<dyn std::io::Write> = if stdout {
        Box::new(std::io::stdout().lock())
    } else {
        if let Some(p) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(p)?;
        }
        Box::new(std::fs::File::create(out)?)
    };
    match format {
        Format::Csv => {
            CsvWriter::new(&mut w).include_header(true).finish(&mut df)?;
        }
        Format::Json => {
            JsonWriter::new(&mut w).with_json_format(JsonFormat::Json).finish(&mut df)?;
        }
        Format::Parquet => {
            ParquetWriter::new(&mut w).finish(&mut df)?;
        }
    }
    w.flush()?;
    Ok(bars.len())
}

/// [`export`], then record it in the action history (`origin` is `cli` or `tui`).
pub fn export_recorded(store: &Store, f: &BarFilter, format: Format, out: &Path, origin: &str) -> Result<usize> {
    let n = export(store, f, format, out)?;
    let fmt = format!("{format:?}").to_lowercase();
    let dest = if out.as_os_str() == "-" { "stdout".to_string() } else { out.display().to_string() };
    let mut scope = vec![];
    if let Some(s) = &f.source {
        scope.push(format!("source {s}"));
    }
    if let Some(s) = &f.symbol {
        scope.push(s.display());
    }
    if let Some(d) = f.duration {
        scope.push(d.to_string());
    }
    let a = crate::store::NewAction::new("export", origin, format!("export {n} bars to {dest} ({fmt})")).params(serde_json::json!({
        "out": dest, "format": fmt, "bars": n, "scope": scope, "from": f.from, "to": f.to,
    }));
    store.note_action(&a);
    Ok(n)
}
