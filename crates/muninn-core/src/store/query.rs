//! Paged, sorted and bucketed bar queries for viewers (TUI list and chart).
use super::{BarFilter, Store};
use crate::error::Result;
use crate::model::Bar;
use serde::{Deserialize, Serialize};

/// Sort key for bar lists: raw columns plus derived per-bar values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BarSort {
    #[default]
    Time,
    Open,
    High,
    Low,
    Close,
    Volume,
    /// close - open
    Change,
    /// (close - open) / open * 100
    ChangePct,
    /// high - low
    Range,
    /// |close - open|
    Body,
    /// high - max(open, close)
    UpperWick,
    /// min(open, close) - low
    LowerWick,
    /// |close - open| / (high - low)
    BodyRange,
    Flags,
}

impl BarSort {
    pub const ALL: [BarSort; 14] = [
        BarSort::Time,
        BarSort::Open,
        BarSort::High,
        BarSort::Low,
        BarSort::Close,
        BarSort::Volume,
        BarSort::Change,
        BarSort::ChangePct,
        BarSort::Range,
        BarSort::Body,
        BarSort::UpperWick,
        BarSort::LowerWick,
        BarSort::BodyRange,
        BarSort::Flags,
    ];

    /// SQL expression over the `bars b` alias.
    pub fn sql(self) -> &'static str {
        match self {
            BarSort::Time => "b.timestamp",
            BarSort::Open => "b.open",
            BarSort::High => "b.high",
            BarSort::Low => "b.low",
            BarSort::Close => "b.close",
            BarSort::Volume => "b.volume",
            BarSort::Change => "(b.close - b.open)",
            BarSort::ChangePct => "((b.close - b.open) / NULLIF(b.open, 0))",
            BarSort::Range => "(b.high - b.low)",
            BarSort::Body => "abs(b.close - b.open)",
            BarSort::UpperWick => "(b.high - max(b.open, b.close))",
            BarSort::LowerWick => "(min(b.open, b.close) - b.low)",
            BarSort::BodyRange => "(abs(b.close - b.open) / NULLIF(b.high - b.low, 0))",
            BarSort::Flags => "b.flags",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BarSort::Time => "time",
            BarSort::Open => "open",
            BarSort::High => "high",
            BarSort::Low => "low",
            BarSort::Close => "close",
            BarSort::Volume => "volume",
            BarSort::Change => "chg",
            BarSort::ChangePct => "chg%",
            BarSort::Range => "range",
            BarSort::Body => "body",
            BarSort::UpperWick => "uwick",
            BarSort::LowerWick => "lwick",
            BarSort::BodyRange => "b/r",
            BarSort::Flags => "flags",
        }
    }

    /// The value for one bar (same formula as `sql`).
    pub fn value(self, b: &Bar) -> f64 {
        match self {
            BarSort::Time => b.ts as f64,
            BarSort::Open => b.open,
            BarSort::High => b.high,
            BarSort::Low => b.low,
            BarSort::Close => b.close,
            BarSort::Volume => b.volume,
            BarSort::Change => b.close - b.open,
            BarSort::ChangePct => {
                if b.open != 0.0 {
                    (b.close - b.open) / b.open * 100.0
                } else {
                    f64::NAN
                }
            }
            BarSort::Range => b.high - b.low,
            BarSort::Body => (b.close - b.open).abs(),
            BarSort::UpperWick => b.high - b.open.max(b.close),
            BarSort::LowerWick => b.open.min(b.close) - b.low,
            BarSort::BodyRange => {
                let r = b.high - b.low;
                if r != 0.0 { (b.close - b.open).abs() / r } else { f64::NAN }
            }
            BarSort::Flags => b.flags as f64,
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    pub fn prev(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

fn where_clause(f: &BarFilter) -> (String, Vec<rusqlite::types::Value>) {
    let mut sql = String::from(" WHERE 1=1");
    let mut p: Vec<rusqlite::types::Value> = Vec::new();
    if let Some(s) = &f.source {
        sql.push_str(" AND b.source_id = (SELECT id FROM sources WHERE name = ?)");
        p.push(s.clone().into());
    }
    if let Some(sym) = &f.symbol {
        sql.push_str(" AND b.base = ? AND b.quote = ?");
        p.push(sym.base.clone().into());
        p.push(sym.quote.clone().into());
    }
    if let Some(d) = f.duration {
        sql.push_str(" AND b.duration_s = ?");
        p.push(d.0.into());
    }
    if let Some(t) = f.from {
        sql.push_str(" AND b.timestamp >= ?");
        p.push(t.into());
    }
    if let Some(t) = f.to {
        sql.push_str(" AND b.timestamp <= ?");
        p.push(t.into());
    }
    (sql, p)
}

fn row_bar(r: &rusqlite::Row) -> rusqlite::Result<Bar> {
    Ok(Bar { ts: r.get(0)?, open: r.get(1)?, high: r.get(2)?, low: r.get(3)?, close: r.get(4)?, volume: r.get(5)?, flags: r.get(6)? })
}

impl Store {
    /// Number of bars matching a filter.
    pub fn count_filtered(&self, f: &BarFilter) -> Result<i64> {
        let (w, p) = where_clause(f);
        Ok(self.conn.query_row(&format!("SELECT COUNT(*) FROM bar_rows b{w}"), rusqlite::params_from_iter(p), |r| r.get(0))?)
    }

    /// One page of bars sorted by `sort` (ties broken by time).
    pub fn page_bars(&self, f: &BarFilter, sort: BarSort, desc: bool, limit: i64, offset: i64) -> Result<Vec<Bar>> {
        let (w, p) = where_clause(f);
        let dir = if desc { "DESC" } else { "ASC" };
        let sql = format!(
            "SELECT b.timestamp, b.open, b.high, b.low, b.close, b.volume, b.flags FROM bar_rows b{w}
             ORDER BY {} {dir}, b.timestamp {dir} LIMIT {limit} OFFSET {offset}",
            sort.sql()
        );
        let mut st = self.conn.prepare(&sql)?;
        let v = st.query_map(rusqlite::params_from_iter(p), row_bar)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Position of the first bar at or after `ts` in time order (for linked list scrolling).
    pub fn time_index(&self, f: &BarFilter, ts: i64, desc: bool) -> Result<i64> {
        let (w, mut p) = where_clause(f);
        p.push(ts.into());
        let op = if desc { ">" } else { "<" };
        Ok(self.conn.query_row(&format!("SELECT COUNT(*) FROM bar_rows b{w} AND b.timestamp {op} ?"), rusqlite::params_from_iter(p), |r| r.get(0))?)
    }

    /// Bars aggregated into buckets of `bucket_ms` (open = first, close = last, high/low = extremes,
    /// volume = sum, flags = OR). `bucket_ms` <= the bar duration returns raw bars.
    pub fn bucketed(&self, f: &BarFilter, bucket_ms: i64) -> Result<Vec<Bar>> {
        let (w, p) = where_clause(f);
        let raw = f.duration.map(|d| bucket_ms <= d.millis()).unwrap_or(bucket_ms <= 1);
        if raw {
            let sql = format!("SELECT b.timestamp, b.open, b.high, b.low, b.close, b.volume, b.flags FROM bar_rows b{w} ORDER BY b.timestamp");
            let mut st = self.conn.prepare(&sql)?;
            return Ok(st.query_map(rusqlite::params_from_iter(p), row_bar)?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        let bk = format!("(b.timestamp / {bucket_ms})");
        let sql = format!(
            "WITH g AS (SELECT {bk} AS k, MIN(b.timestamp) AS t0, MAX(b.timestamp) AS t1, MAX(b.high) AS h, MIN(b.low) AS l,
                               SUM(b.volume) AS v, MAX(b.flags) AS fl FROM bar_rows b{w} GROUP BY k)
             SELECT g.k * {bucket_ms},
                    (SELECT b.open FROM bar_rows b{w} AND b.timestamp = g.t0 LIMIT 1), g.h, g.l,
                    (SELECT b.close FROM bar_rows b{w} AND b.timestamp = g.t1 LIMIT 1), g.v, g.fl
             FROM g ORDER BY g.k"
        );
        let mut all = p.clone();
        all.extend(p.clone());
        all.extend(p);
        let mut st = self.conn.prepare(&sql)?;
        let v = st.query_map(rusqlite::params_from_iter(all), row_bar)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }
}

/// Totals over the bars of a filter (for report summaries).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarSummary {
    pub count: i64,
    pub first: i64,
    pub last: i64,
    /// Open of the first bar, close of the last.
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
}

impl BarSummary {
    /// Change from the first open to the last close, in percent.
    pub fn change_pct(&self) -> Option<f64> {
        (self.open != 0.0).then(|| (self.close - self.open) / self.open * 100.0)
    }
}

impl Store {
    /// Count, range, open/close, extremes and total volume of the filtered bars.
    pub fn bar_summary(&self, f: &BarFilter) -> Result<Option<BarSummary>> {
        let (w, p) = where_clause(f);
        type Row = (i64, Option<i64>, Option<i64>, Option<f64>, Option<f64>, Option<f64>);
        let row: Row = self.conn.query_row(
            &format!("SELECT COUNT(*), MIN(b.timestamp), MAX(b.timestamp), MAX(b.high), MIN(b.low), SUM(b.volume) FROM bar_rows b{w}"),
            rusqlite::params_from_iter(p.clone()),
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?;
        let (count, Some(first), Some(last), Some(high), Some(low), volume) = row else { return Ok(None) };
        if count == 0 {
            return Ok(None);
        }
        let px = |ts: i64, col: &str| -> Result<f64> {
            let mut q = p.clone();
            q.push(ts.into());
            Ok(self.conn.query_row(
                &format!("SELECT b.{col} FROM bar_rows b{w} AND b.timestamp = ? LIMIT 1"),
                rusqlite::params_from_iter(q),
                |r| r.get(0),
            )?)
        };
        Ok(Some(BarSummary { count, first, last, open: px(first, "open")?, close: px(last, "close")?, high, low, volume: volume.unwrap_or(0.0) }))
    }

    /// Min and max of a price metric over the filtered bars (VWAP: one volume-weighted value).
    pub fn metric_range(&self, f: &BarFilter, m: crate::model::PriceMetric) -> Result<Option<(f64, f64)>> {
        let (w, p) = where_clause(f);
        let sql = if m == crate::model::PriceMetric::Vwap {
            format!(
                "SELECT SUM({e} * b.volume) / NULLIF(SUM(b.volume), 0), SUM({e} * b.volume) / NULLIF(SUM(b.volume), 0) FROM bar_rows b{w}",
                e = m.sql()
            )
        } else {
            format!("SELECT MIN({e}), MAX({e}) FROM bar_rows b{w}", e = m.sql())
        };
        let r: (Option<f64>, Option<f64>) = self.conn.query_row(&sql, rusqlite::params_from_iter(p), |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(match r {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Symbol};

    fn store() -> Store {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.get_or_create_source("t", "file", None).unwrap();
        let sym = Symbol::new("BTC", "USDT");
        let bars: Vec<Bar> = (0..10)
            .map(|i| {
                let o = 100.0 + i as f64;
                let c = if i % 2 == 0 { o + 2.0 } else { o - 1.0 };
                Bar {
                    ts: i * 300_000,
                    open: o,
                    high: o.max(c) + i as f64,
                    low: o.min(c) - 1.0,
                    close: c,
                    volume: (10 - i) as f64,
                    flags: (i == 3) as i64,
                }
            })
            .collect();
        s.upsert_bars(&src, &sym, Duration(300), &bars).unwrap();
        s
    }

    fn f() -> BarFilter {
        BarFilter { symbol: Some(Symbol::new("BTC", "USDT")), duration: Some(Duration(300)), source: Some("t".into()), ..Default::default() }
    }

    #[test]
    fn sorts_by_every_key_matching_value() {
        let s = store();
        assert_eq!(s.count_filtered(&f()).unwrap(), 10);
        for k in BarSort::ALL {
            for desc in [false, true] {
                let v = s.page_bars(&f(), k, desc, 100, 0).unwrap();
                assert_eq!(v.len(), 10);
                for w in v.windows(2) {
                    let (a, b) = (k.value(&w[0]), k.value(&w[1]));
                    if desc { assert!(a >= b || a.is_nan(), "{k:?} desc") } else { assert!(a <= b || b.is_nan(), "{k:?} asc") }
                }
            }
        }
        let p = s.page_bars(&f(), BarSort::Time, false, 3, 4).unwrap();
        assert_eq!(p[0].ts, 4 * 300_000);
        assert_eq!(s.time_index(&f(), 5 * 300_000, false).unwrap(), 5);
    }

    #[test]
    fn summaries() {
        let s = store();
        let raw = s.bucketed(&f(), 300_000).unwrap();
        let m = s.bar_summary(&f()).unwrap().unwrap();
        assert_eq!((m.count, m.first, m.last), (10, raw[0].ts, raw[9].ts));
        assert_eq!((m.open, m.close), (raw[0].open, raw[9].close));
        assert_eq!(m.volume, raw.iter().map(|b| b.volume).sum::<f64>());
        let mut none = f();
        none.from = Some(i64::MAX - 1);
        assert_eq!(s.bar_summary(&none).unwrap(), None);
    }

    #[test]
    fn metric_ranges() {
        let s = store();
        let (lo, hi) = s.metric_range(&f(), crate::model::PriceMetric::Close).unwrap().unwrap();
        assert!(lo < hi);
        let (a, b) = s.metric_range(&f(), crate::model::PriceMetric::Vwap).unwrap().unwrap();
        assert_eq!(a, b);
        let mut none = f();
        none.from = Some(i64::MAX - 1);
        assert_eq!(s.metric_range(&none, crate::model::PriceMetric::Close).unwrap(), None);
    }

    #[test]
    fn buckets_aggregate_ohlcv() {
        let s = store();
        let raw = s.bucketed(&f(), 300_000).unwrap();
        assert_eq!(raw.len(), 10);
        let b = s.bucketed(&f(), 900_000).unwrap();
        assert_eq!(b.len(), 4);
        assert_eq!(b[0].open, raw[0].open);
        assert_eq!(b[0].close, raw[2].close);
        assert_eq!(b[0].volume, raw[0].volume + raw[1].volume + raw[2].volume);
        assert_eq!(b[1].flags, 1);
        assert_eq!(b[0].high, raw[..3].iter().map(|x| x.high).fold(f64::MIN, f64::max));
    }
}
