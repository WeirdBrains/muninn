//! In-memory series data for the TUI.
//!
//! A whole series is loaded once (in a background thread, on its own connection) and
//! then lists sort, charts aggregate and the sidebar filters in memory. Reading a
//! million encrypted rows from SQLite costs seconds; sorting them in memory costs tens
//! of milliseconds.
use super::layout::SeriesKey;
use crate::model::{Bar, PriceMetric};
use crate::store::{BarSort, Store};
use std::collections::HashMap;
use std::sync::{Arc, mpsc};

/// All bars of one series in time order.
#[derive(Debug, Default)]
pub struct SeriesData {
    pub bars: Vec<Bar>,
}

/// Sortable key for an f64 (total order; NaN first like SQL NULL).
fn ord_key(v: f64) -> u64 {
    if v.is_nan() {
        return 0;
    }
    let b = v.to_bits();
    if b >> 63 == 1 { !b } else { b | (1 << 63) }
}

impl SeriesData {
    pub fn new(mut bars: Vec<Bar>) -> Self {
        if !bars.is_sorted_by_key(|b| b.ts) {
            bars.sort_by_key(|b| b.ts);
        }
        Self { bars }
    }

    /// Index range of the bars inside `scope` (inclusive times).
    pub fn range(&self, scope: Option<(i64, i64)>) -> std::ops::Range<usize> {
        match scope {
            None => 0..self.bars.len(),
            Some((a, b)) => {
                let lo = self.bars.partition_point(|x| x.ts < a);
                let hi = self.bars.partition_point(|x| x.ts <= b);
                lo..hi.max(lo)
            }
        }
    }

    /// Bar indices within `r` ordered by `sort` ascending, ties by time.
    pub fn sorted(&self, r: std::ops::Range<usize>, sort: BarSort) -> Vec<u32> {
        let mut keys: Vec<(u64, u32)> = r.map(|i| (ord_key(sort.value(&self.bars[i])), i as u32)).collect();
        keys.sort_unstable();
        keys.into_iter().map(|(_, i)| i).collect()
    }

    /// Bars aggregated into `bucket_ms` buckets over `[from, to]` (raw bars when
    /// `bucket_ms <= dur_ms`). Same rules as `Store::bucketed`.
    pub fn bucketed(&self, from: i64, to: i64, bucket_ms: i64, dur_ms: i64) -> Vec<Bar> {
        let r = self.range(Some((from, to)));
        let s = &self.bars[r];
        if bucket_ms <= dur_ms {
            return s.to_vec();
        }
        let mut out: Vec<Bar> = Vec::with_capacity(s.len() / (bucket_ms / dur_ms.max(1)).max(1) as usize + 2);
        let mut cur: Option<(i64, Bar)> = None;
        for b in s {
            let k = b.ts.div_euclid(bucket_ms);
            match &mut cur {
                Some((ck, a)) if *ck == k => {
                    a.high = a.high.max(b.high);
                    a.low = a.low.min(b.low);
                    a.close = b.close;
                    a.volume += b.volume;
                    a.flags = a.flags.max(b.flags);
                }
                _ => {
                    if let Some((_, a)) = cur.take() {
                        out.push(a);
                    }
                    cur = Some((k, Bar { ts: k * bucket_ms, ..*b }));
                }
            }
        }
        if let Some((_, a)) = cur {
            out.push(a);
        }
        out
    }

    /// Min and max of a price metric over `r` (VWAP: one volume-weighted value).
    pub fn metric_range(&self, r: std::ops::Range<usize>, m: PriceMetric) -> Option<(f64, f64)> {
        let s = &self.bars[r];
        if s.is_empty() {
            return None;
        }
        if m == PriceMetric::Vwap {
            let (pv, v) = s.iter().fold((0.0, 0.0), |(pv, v), b| (pv + PriceMetric::Typical.value(b) * b.volume, v + b.volume));
            return (v > 0.0).then(|| (pv / v, pv / v));
        }
        Some(s.iter().map(|b| m.value(b)).fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| (lo.min(x), hi.max(x))))
    }
}

/// What the loader thread reports.
enum Msg {
    /// Bars read so far, and the total once counted.
    Progress(SeriesKey, u64, Option<u64>),
    Done(SeriesKey, Result<Vec<Bar>, String>),
}

/// Bars per read; the loader reports progress after each chunk.
pub const CHUNK: i64 = 50_000;

/// A series that is not ready: still loading (bars read, total if known) or failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Loading { done: u64, total: Option<u64> },
    Failed(String),
}

pub enum Slot {
    Loading { done: u64, total: Option<u64> },
    Ready(Arc<SeriesData>),
    Failed(String),
}

/// Loaded series plus a background loader.
pub struct DataCache {
    slots: HashMap<SeriesKey, Slot>,
    /// Bumped on every arrival/invalidate; caches key on it.
    pub generation: u64,
    /// Load inline instead of in a thread (tests).
    pub sync: bool,
    db: std::path::PathBuf,
    key: [u8; 32],
    req: Option<mpsc::Sender<SeriesKey>>,
    rx: Option<mpsc::Receiver<Msg>>,
}

fn load(store: &Store, k: &SeriesKey) -> Result<Vec<Bar>, String> {
    load_chunked(store, k, &mut |_, _| {})
}

/// Read a series in time order, CHUNK bars at a time (keyset paging), calling
/// `progress(done, total)` once the total is counted and after every chunk.
pub fn load_chunked(store: &Store, k: &SeriesKey, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<Vec<Bar>, String> {
    let mut f = k.filter();
    let total = store.count_filtered(&f).ok().map(|n| n.max(0) as u64);
    progress(0, total);
    let mut bars: Vec<Bar> = Vec::with_capacity(total.unwrap_or(0) as usize);
    loop {
        let page = store.page_bars(&f, BarSort::Time, false, CHUNK, 0).map_err(|e| e.to_string())?;
        let n = page.len() as i64;
        if let Some(last) = page.last() {
            f.from = Some(last.ts + 1);
        }
        bars.extend(page);
        progress(bars.len() as u64, total);
        if n < CHUNK {
            return Ok(bars);
        }
    }
}

impl DataCache {
    pub fn new(db: std::path::PathBuf, key: [u8; 32]) -> Self {
        Self { slots: HashMap::new(), generation: 1, sync: false, db, key, req: None, rx: None }
    }

    fn worker(&mut self) -> &mpsc::Sender<SeriesKey> {
        if self.req.is_none() {
            let (qtx, qrx) = mpsc::channel::<SeriesKey>();
            let (rtx, rrx) = mpsc::channel();
            let (db, key) = (self.db.clone(), self.key);
            std::thread::spawn(move || {
                let store = match Store::open(&db, Some(&key)) {
                    Ok(s) => s,
                    Err(e) => {
                        while let Ok(k) = qrx.recv() {
                            let _ = rtx.send(Msg::Done(k, Err(e.to_string())));
                        }
                        return;
                    }
                };
                store.tune_for_reads();
                while let Ok(k) = qrx.recv() {
                    let tx = rtx.clone();
                    let key = k.clone();
                    let r = load_chunked(&store, &k, &mut |done, total| {
                        let _ = tx.send(Msg::Progress(key.clone(), done, total));
                    });
                    if rtx.send(Msg::Done(k, r)).is_err() {
                        return;
                    }
                }
            });
            self.req = Some(qtx);
            self.rx = Some(rrx);
        }
        self.req.as_ref().unwrap()
    }

    /// The series if loaded; otherwise starts loading it (inline when `sync`).
    pub fn get(&mut self, k: &SeriesKey, store: &Store) -> Option<Arc<SeriesData>> {
        match self.slots.get(k) {
            Some(Slot::Ready(d)) => return Some(d.clone()),
            Some(_) => return None,
            None => {}
        }
        if self.sync {
            let s = match load(store, k) {
                Ok(b) => Slot::Ready(Arc::new(SeriesData::new(b))),
                Err(e) => Slot::Failed(e),
            };
            self.slots.insert(k.clone(), s);
            self.generation += 1;
            return self.get(k, store);
        }
        self.slots.insert(k.clone(), Slot::Loading { done: 0, total: None });
        let _ = self.worker().send(k.clone());
        None
    }

    pub fn state(&self, k: &SeriesKey) -> Option<&Slot> {
        self.slots.get(k)
    }

    pub fn loading(&self) -> usize {
        self.slots.values().filter(|s| matches!(s, Slot::Loading { .. })).count()
    }

    /// Why a series has no data yet (None when ready or never requested).
    pub fn pending(&self, k: &SeriesKey) -> Option<Pending> {
        match self.slots.get(k)? {
            Slot::Loading { done, total } => Some(Pending::Loading { done: *done, total: *total }),
            Slot::Failed(e) => Some(Pending::Failed(e.clone())),
            Slot::Ready(_) => None,
        }
    }

    /// Take finished loads. Returns true when something arrived.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut got = false;
        while let Ok(m) = rx.try_recv() {
            match m {
                Msg::Progress(k, done, total) => {
                    // ignore stale progress for a series that was cleared meanwhile
                    if let Some(Slot::Loading { done: d, total: t }) = self.slots.get_mut(&k) {
                        *d = done;
                        *t = total;
                    }
                }
                Msg::Done(k, r) => {
                    let s = match r {
                        Ok(b) => Slot::Ready(Arc::new(SeriesData::new(b))),
                        Err(e) => Slot::Failed(e),
                    };
                    self.slots.insert(k, s);
                    got = true;
                }
            }
        }
        if got {
            self.generation += 1;
        }
        got
    }

    /// Drop everything (after an import or refresh); reloads on demand.
    pub fn clear(&mut self) {
        self.slots.retain(|_, s| matches!(s, Slot::Loading { .. }));
        self.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SeriesData {
        SeriesData::new(
            (0..10)
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
                .collect(),
        )
    }

    #[test]
    fn ord_key_orders_floats() {
        let mut v = [3.5, -1.0, 0.0, -0.0, f64::NAN, 1e9, -1e9, 2.0];
        v.sort_by_key(|x| ord_key(*x));
        assert!(v[0].is_nan());
        assert_eq!(&v[1..3], &[-1e9, -1.0]);
        assert_eq!(v[7], 1e9);
    }

    #[test]
    fn matches_the_sql_store_for_sorts_buckets_and_ranges() {
        let d = data();
        let mut s = Store::open_in_memory().unwrap();
        let src = s.get_or_create_source("t", "file", None).unwrap();
        s.upsert_bars(&src, &crate::model::Symbol::new("BTC", "USDT"), crate::model::Duration(300), &d.bars).unwrap();
        let k = SeriesKey { source: "t".into(), symbol: "BTC/USDT".into(), duration: "5m".into() };
        let f = k.filter();
        for sort in BarSort::ALL {
            let mem: Vec<i64> = d.sorted(d.range(None), sort).iter().map(|i| d.bars[*i as usize].ts).collect();
            let sql: Vec<i64> = s.page_bars(&f, sort, false, 100, 0).unwrap().iter().map(|b| b.ts).collect();
            assert_eq!(mem, sql, "{sort:?}");
        }
        for bucket in [300_000, 900_000, 3_600_000] {
            let mut f2 = f.clone();
            f2.from = Some(300_000);
            f2.to = Some(2_400_000);
            assert_eq!(d.bucketed(300_000, 2_400_000, bucket, 300_000), s.bucketed(&f2, bucket).unwrap(), "{bucket}");
        }
        for m in PriceMetric::ALL {
            let a = d.metric_range(d.range(Some((0, 1_500_000))), m).unwrap();
            let mut f3 = f.clone();
            f3.to = Some(1_500_000);
            let b = s.metric_range(&f3, m).unwrap().unwrap();
            assert!((a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9, "{m:?} {a:?} {b:?}");
        }
        assert_eq!(d.range(Some((300_000, 900_000))), 1..4);
    }

    #[test]
    fn chunked_load_reports_progress_and_matches_a_single_read() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.get_or_create_source("t", "file", None).unwrap();
        let n = CHUNK as usize * 2 + 7;
        let bars: Vec<Bar> =
            (0..n as i64).map(|i| Bar { ts: i * 60_000, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: i as f64, flags: 0 }).collect();
        s.upsert_bars(&src, &crate::model::Symbol::new("BTC", "USDT"), crate::model::Duration(60), &bars).unwrap();
        let k = SeriesKey { source: "t".into(), symbol: "BTC/USDT".into(), duration: "1m".into() };
        let mut seen = vec![];
        let got = load_chunked(&s, &k, &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(got, s.bucketed(&k.filter(), 0).unwrap());
        let t = Some(n as u64);
        assert_eq!(seen, vec![(0, t), (CHUNK as u64, t), (2 * CHUNK as u64, t), (n as u64, t)]);
    }

    #[test]
    fn background_load_goes_through_loading_to_ready() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("m.db");
        let key = [7u8; 32];
        {
            let mut s = Store::open(&db, Some(&key)).unwrap();
            let src = s.get_or_create_source("t", "file", None).unwrap();
            s.upsert_bars(&src, &crate::model::Symbol::new("BTC", "USDT"), crate::model::Duration(300), &data().bars).unwrap();
        }
        let s = Store::open(&db, Some(&key)).unwrap();
        let mut c = DataCache::new(db, key);
        let k = SeriesKey { source: "t".into(), symbol: "BTC/USDT".into(), duration: "5m".into() };
        assert!(c.get(&k, &s).is_none());
        assert!(matches!(c.pending(&k), Some(Pending::Loading { .. })));
        let t0 = std::time::Instant::now();
        while c.get(&k, &s).is_none() && t0.elapsed().as_secs() < 30 {
            c.poll();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(c.get(&k, &s).unwrap().bars.len(), 10);
        assert_eq!(c.pending(&k), None);
    }

    #[test]
    fn sync_cache_loads_and_clears() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.get_or_create_source("t", "file", None).unwrap();
        s.upsert_bars(&src, &crate::model::Symbol::new("BTC", "USDT"), crate::model::Duration(300), &data().bars).unwrap();
        let mut c = DataCache::new("unused".into(), [0; 32]);
        c.sync = true;
        let k = SeriesKey { source: "t".into(), symbol: "BTC/USDT".into(), duration: "5m".into() };
        let g = c.generation;
        assert_eq!(c.get(&k, &s).unwrap().bars.len(), 10);
        assert!(c.generation > g);
        c.clear();
        assert!(c.state(&k).is_none());
    }
}
