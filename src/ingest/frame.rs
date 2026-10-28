//! Column mapping, timestamp normalization, duration derivation and validation over a Polars frame.
use super::time::{epoch_to_ms, parse_ts};
use crate::error::{Error, Result};
use crate::model::{Bar, Duration, FLAG_ZERO_VOLUME};
use polars::prelude::*;
use std::collections::HashMap;

pub const FIELDS: [&str; 6] = ["timestamp", "open", "high", "low", "close", "volume"];

fn aliases(field: &str) -> &'static [&'static str] {
    match field {
        "timestamp" => &["timestamp_utc", "timestamp", "open_time", "opentime", "time", "date", "datetime", "ts", "t", "start", "time_period_start"],
        "open" => &["open", "o", "open_price", "price_open"],
        "high" => &["high", "h", "high_price", "price_high"],
        "low" => &["low", "l", "low_price", "price_low"],
        "close" => &["close", "c", "close_price", "price_close"],
        "volume" => &["volume", "vol", "v", "base_volume", "volume_base", "volume_traded"],
        _ => &[],
    }
}

/// Parse `--map col=field` values.
pub fn parse_map(items: &[String]) -> Result<HashMap<String, String>> {
    let mut m = HashMap::new();
    for it in items {
        let (col, field) = it
            .split_once('=')
            .ok_or_else(|| Error::invalid("--map", format!("`{it}` (expected COL=FIELD)")))?;
        let field = field.trim().to_ascii_lowercase();
        if !FIELDS.contains(&field.as_str()) {
            return Err(Error::invalid("--map", format!("unknown field `{field}` (fields: {})", FIELDS.join(", "))));
        }
        m.insert(field, col.trim().to_string());
    }
    Ok(m)
}

/// Resolve the source column for every field.
pub fn map_columns(df: &DataFrame, overrides: &HashMap<String, String>) -> Result<[String; 6]> {
    let names: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    let positional = names.iter().all(|n| n.starts_with("column_")) && names.len() >= 6;
    let mut out: Vec<String> = Vec::with_capacity(6);
    for (i, f) in FIELDS.iter().enumerate() {
        if let Some(c) = overrides.get(*f) {
            if !names.contains(c) {
                return Err(Error::invalid("--map", format!("column `{c}` not found (have: {})", names.join(", "))));
            }
            out.push(c.clone());
            continue;
        }
        if positional {
            out.push(names[i].clone());
            continue;
        }
        let found = aliases(f)
            .iter()
            .find_map(|a| names.iter().find(|n| n.trim().eq_ignore_ascii_case(a)))
            .cloned();
        match found {
            Some(c) => out.push(c),
            None => {
                return Err(Error::missing(
                    "column",
                    format!("no `{f}` column (have: {}); map one with --map <col>={f}", names.join(", ")),
                ))
            }
        }
    }
    Ok([out[0].clone(), out[1].clone(), out[2].clone(), out[3].clone(), out[4].clone(), out[5].clone()])
}

/// Timestamps as epoch ms (None when unparseable).
pub fn timestamps(df: &DataFrame, col: &str) -> Result<Vec<Option<i64>>> {
    let s = df.column(col)?.as_materialized_series().clone();
    Ok(match s.dtype() {
        DataType::String => s.str()?.iter().map(|v| v.and_then(parse_ts)).collect(),
        DataType::Datetime(tu, _) => {
            let tu = *tu;
            let i = s.cast(&DataType::Int64)?;
            i.i64()?
                .iter()
                .map(|v| {
                    v.map(|x| match tu {
                        TimeUnit::Milliseconds => x,
                        TimeUnit::Microseconds => x / 1000,
                        TimeUnit::Nanoseconds => x / 1_000_000,
                    })
                })
                .collect()
        }
        DataType::Float32 | DataType::Float64 => s
            .cast(&DataType::Float64)?
            .f64()?
            .iter()
            .map(|v| v.filter(|x| x.is_finite()).map(|x| if x.abs() < 1e11 { (x * 1000.0).round() as i64 } else { epoch_to_ms(x as i64) }))
            .collect(),
        dt if dt.is_integer() => s.cast(&DataType::Int64)?.i64()?.iter().map(|v| v.map(epoch_to_ms)).collect(),
        _ => s.cast(&DataType::String)?.str()?.iter().map(|v| v.and_then(parse_ts)).collect(),
    })
}

pub fn floats(df: &DataFrame, col: &str) -> Result<Vec<Option<f64>>> {
    let s = df.column(col)?.as_materialized_series().clone();
    let s = if s.dtype() == &DataType::String {
        // trim then cast; invalid -> null
        let trimmed: StringChunked = s.str()?.apply_values(|v| std::borrow::Cow::Owned(v.trim().to_string()));
        trimmed.into_series().cast(&DataType::Float64)?
    } else {
        s.cast(&DataType::Float64)?
    };
    Ok(s.f64()?.iter().collect())
}

/// Duration derivation result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerivedDuration {
    pub duration: Duration,
    /// Share of consecutive diffs equal to the mode.
    pub share: f64,
}

/// Mode of consecutive differences across the whole (sorted, de-duplicated) series.
pub fn derive_duration(ts: &[i64]) -> Option<DerivedDuration> {
    let mut v: Vec<i64> = ts.to_vec();
    v.sort_unstable();
    v.dedup();
    if v.len() < 2 {
        return None;
    }
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for w in v.windows(2) {
        *counts.entry(w[1] - w[0]).or_default() += 1;
    }
    let total: usize = counts.values().sum();
    let (diff, n) = counts.into_iter().max_by_key(|(d, n)| (*n, std::cmp::Reverse(*d)))?;
    if diff <= 0 || diff % 1000 != 0 {
        return None;
    }
    Some(DerivedDuration { duration: Duration(diff / 1000), share: n as f64 / total as f64 })
}

/// A row rejected during validation.
#[derive(Debug, Clone, PartialEq)]
pub struct Reject {
    /// 1-based data row number.
    pub row: usize,
    pub timestamp: Option<i64>,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct Validated {
    pub bars: Vec<Bar>,
    pub rejects: Vec<Reject>,
    pub zero_volume: u64,
    pub dropped_zero_volume: u64,
}

/// Columns extracted from a frame.
pub struct Columns {
    pub ts: Vec<Option<i64>>,
    pub open: Vec<Option<f64>>,
    pub high: Vec<Option<f64>>,
    pub low: Vec<Option<f64>>,
    pub close: Vec<Option<f64>>,
    pub volume: Vec<Option<f64>>,
}

impl Columns {
    pub fn from_frame(df: &DataFrame, overrides: &HashMap<String, String>) -> Result<Self> {
        let m = map_columns(df, overrides)?;
        Ok(Self {
            ts: timestamps(df, &m[0])?,
            open: floats(df, &m[1])?,
            high: floats(df, &m[2])?,
            low: floats(df, &m[3])?,
            close: floats(df, &m[4])?,
            volume: floats(df, &m[5])?,
        })
    }

    pub fn len(&self) -> usize {
        self.ts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ts.is_empty()
    }

    pub fn valid_ts(&self) -> Vec<i64> {
        self.ts.iter().flatten().copied().collect()
    }

    /// Validate every row. `high >= max(o,c)`, `low <= min(o,c)`, `volume >= 0`, no NaN, aligned.
    pub fn validate(&self, duration: Duration, drop_zero_volume: bool) -> Validated {
        let mut out = Validated { bars: Vec::with_capacity(self.len()), ..Default::default() };
        let dms = duration.millis();
        let check_align = duration.secs() <= 86_400;
        for i in 0..self.len() {
            let row = i + 1;
            let ts = self.ts[i];
            let rej = |reason: &str| Reject { row, timestamp: ts, reason: reason.to_string() };
            let Some(t) = ts else {
                out.rejects.push(rej("unparseable timestamp"));
                continue;
            };
            let vals = [self.open[i], self.high[i], self.low[i], self.close[i], self.volume[i]];
            if vals.iter().any(|v| v.is_none_or(|x| !x.is_finite())) {
                out.rejects.push(rej("missing or non-numeric value"));
                continue;
            }
            let [o, h, l, c, v] = vals.map(|x| x.unwrap_or_default());
            if h < o.max(c) || h < l {
                out.rejects.push(rej("high below open/close/low"));
                continue;
            }
            if l > o.min(c) {
                out.rejects.push(rej("low above open/close"));
                continue;
            }
            if v < 0.0 {
                out.rejects.push(rej("negative volume"));
                continue;
            }
            if check_align && t.rem_euclid(dms) != 0 {
                out.rejects.push(rej("timestamp not aligned to duration"));
                continue;
            }
            let mut flags = 0;
            if v == 0.0 {
                out.zero_volume += 1;
                if drop_zero_volume {
                    out.dropped_zero_volume += 1;
                    continue;
                }
                flags |= FLAG_ZERO_VOLUME;
            }
            out.bars.push(Bar { ts: t, open: o, high: h, low: l, close: c, volume: v, flags });
        }
        out
    }
}

/// Gap statistics over sorted unique timestamps.
pub fn gaps(ts: &[i64], duration: Duration) -> (u64, u64) {
    let mut v = ts.to_vec();
    v.sort_unstable();
    v.dedup();
    let d = duration.millis();
    let mut gaps = 0;
    let mut missing = 0;
    for w in v.windows(2) {
        let diff = w[1] - w[0];
        if diff > d {
            gaps += 1;
            missing += (diff / d - 1).max(0) as u64;
        }
    }
    (gaps, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_ignores_gaps() {
        let mut ts: Vec<i64> = (0..100).map(|i| i * 300_000).collect();
        ts.extend((200..210).map(|i| i * 300_000)); // a big gap
        let d = derive_duration(&ts).unwrap();
        assert_eq!(d.duration, Duration(300));
        assert!(d.share > 0.9);
        assert_eq!(gaps(&ts, Duration(300)), (1, 100));
    }

    #[test]
    fn map_parsing() {
        let m = parse_map(&["ts=timestamp".into()]).unwrap();
        assert_eq!(m["timestamp"], "ts");
        assert!(parse_map(&["x=bogus".into()]).is_err());
        assert!(parse_map(&["nope".into()]).is_err());
    }

    #[test]
    fn validation_rules() {
        let c = Columns {
            ts: vec![Some(0), Some(300_000), None, Some(600_000), Some(900_001)],
            open: vec![Some(1.0), Some(1.0), Some(1.0), Some(1.0), Some(1.0)],
            high: vec![Some(2.0), Some(0.5), Some(2.0), Some(2.0), Some(2.0)],
            low: vec![Some(0.5), Some(0.4), Some(0.5), Some(0.5), Some(0.5)],
            close: vec![Some(1.5), Some(1.0), Some(1.0), Some(1.0), Some(1.0)],
            volume: vec![Some(0.0), Some(1.0), Some(1.0), Some(1.0), Some(1.0)],
        };
        let v = c.validate(Duration(300), false);
        assert_eq!(v.bars.len(), 2);
        assert_eq!(v.rejects.len(), 3);
        assert_eq!(v.zero_volume, 1);
        assert_eq!(v.bars[0].flags, FLAG_ZERO_VOLUME);
        let v = c.validate(Duration(300), true);
        assert_eq!(v.bars.len(), 1);
        assert_eq!(v.dropped_zero_volume, 1);
    }
}
