//! Views of the store: projects, datasets (series) and downsampled bars.
//! Unlocking needs the Muninn password; the key stays in Rust. Dashboard reads open
//! the store read-only (no migrations). Import and export open it for writes.
use super::info::paths;
use muninn_core::crypto::Key32;
use muninn_core::model::{Duration, Symbol};
use muninn_core::store::{BarFilter, Store};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct ProjectSummary {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub description: String,
    pub updated_at: i64,
}

/// One stored series: source, symbol and bar duration, with its extent.
pub struct DatasetSummary {
    pub source: String,
    pub base: String,
    pub quote: String,
    pub duration: String,
    pub duration_s: i64,
    pub count: i64,
    pub first: i64,
    pub last: i64,
}

pub struct BarPoint {
    pub ts: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Everything the dashboard shows.
pub struct Overview {
    pub projects: Vec<ProjectSummary>,
    pub datasets: Vec<DatasetSummary>,
}

/// The unlocked data key, kept in Rust memory only (wiped on [`lock`] and on drop).
static SESSION: Mutex<Option<(PathBuf, Key32)>> = Mutex::new(None);

/// Unlock with the Muninn password. The key stays in Rust; Dart never sees it.
pub fn unlock(home: Option<String>, password: String) -> anyhow::Result<()> {
    let p = paths(home)?;
    let pw = secrecy::SecretString::from(password);
    let dek = muninn_core::crypto::keyring::unlock_with_password(&p, &pw)?;
    Store::open_readonly(&p.db, Some(&dek.0))?; // check the key against the database
    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = Some((p.db, dek));
    Ok(())
}

/// Forget the key.
#[flutter_rust_bridge::frb(sync)]
pub fn lock() {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).take();
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_unlocked() -> bool {
    SESSION.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

fn open() -> anyhow::Result<Store> {
    let g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let (db, key) = g.as_ref().ok_or_else(|| anyhow::anyhow!("locked: unlock with the Muninn password first"))?;
    Ok(Store::open_readonly(db, Some(&key.0))?)
}

/// Open the unlocked store for writes (migrations may run). The key never leaves Rust.
pub(crate) fn open_write() -> anyhow::Result<Store> {
    let g = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    let (db, key) = g.as_ref().ok_or_else(|| anyhow::anyhow!("locked: unlock with the Muninn password first"))?;
    Ok(Store::open(db, Some(&key.0))?)
}

/// Projects and datasets, read-only (after [`unlock`]).
pub fn overview() -> anyhow::Result<Overview> {
    overview_of(&open()?)
}

/// A dataset a project points at, with the project's saved range (if any).
pub struct ProjectSeries {
    /// The project item id (`mnnn project drop PROJECT ITEM`).
    pub item_id: i64,
    pub source: String,
    pub base: String,
    pub quote: String,
    pub duration_s: i64,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// The series results saved in a project (`mnnn project add … --series/--search`).
pub fn project_series(project_id: String) -> anyhow::Result<Vec<ProjectSeries>> {
    project_series_of(&open()?, &project_id)
}

pub(crate) fn project_series_of(s: &Store, project_id: &str) -> anyhow::Result<Vec<ProjectSeries>> {
    Ok(s.project_items(project_id)?
        .iter()
        .filter_map(|i| {
            i.series().map(|r| ProjectSeries {
                item_id: i.id,
                source: r.source,
                base: r.symbol.base,
                quote: r.symbol.quote,
                duration_s: r.duration.0,
                from: r.from,
                to: r.to,
            })
        })
        .collect())
}

pub(crate) fn overview_of(s: &Store) -> anyhow::Result<Overview> {
    let projects = s
        .projects()?
        .into_iter()
        .map(|p| ProjectSummary { short_id: p.short_id().to_string(), id: p.id, name: p.name, description: p.description, updated_at: p.updated_at })
        .collect();
    let datasets = s
        .series()?
        .into_iter()
        .map(|d| DatasetSummary {
            source: d.source,
            base: d.symbol.base,
            quote: d.symbol.quote,
            duration: d.duration.to_string(),
            duration_s: d.duration.0,
            count: d.count,
            first: d.first,
            last: d.last,
        })
        .collect();
    Ok(Overview { projects, datasets })
}

/// Bars of one series in `[from, to]` (ms, optional), downsampled in Rust to at most
/// `max_points` buckets so charts stay fast on any device.
#[allow(clippy::too_many_arguments)]
pub fn query_bars(
    source: String,
    base: String,
    quote: String,
    duration_s: i64,
    from: Option<i64>,
    to: Option<i64>,
    max_points: u32,
) -> anyhow::Result<Vec<BarPoint>> {
    let s = open()?;
    query_of(&s, source, base, quote, duration_s, from, to, max_points)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn query_of(
    s: &Store,
    source: String,
    base: String,
    quote: String,
    duration_s: i64,
    from: Option<i64>,
    to: Option<i64>,
    max_points: u32,
) -> anyhow::Result<Vec<BarPoint>> {
    let f = BarFilter {
        source: Some(source),
        symbol: Some(Symbol { base, quote }),
        duration: Some(Duration(duration_s)),
        from,
        to,
        limit: None,
        offset: None,
    };
    let (lo, hi) = match (from, to) {
        (Some(a), Some(b)) => (a, b),
        _ => s
            .series()?
            .into_iter()
            .find(|d| Some(&d.source) == f.source.as_ref() && Some(&d.symbol) == f.symbol.as_ref() && d.duration.0 == duration_s)
            .map(|d| (from.unwrap_or(d.first), to.unwrap_or(d.last)))
            .unwrap_or((0, 0)),
    };
    let span = (hi - lo).max(0) + duration_s * 1000;
    let bucket = (span / i64::from(max_points.max(1))).max(duration_s * 1000);
    Ok(s.bucketed(&f, bucket)?
        .into_iter()
        .map(|b| BarPoint { ts: b.ts, open: b.open, high: b.high, low: b.low, close: b.close, volume: b.volume })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_store_overview_and_query() {
        let s = Store::open_in_memory().unwrap();
        let o = overview_of(&s).unwrap();
        assert!(o.projects.is_empty() && o.datasets.is_empty());
        assert!(query_of(&s, "x".into(), "BTC".into(), "USD".into(), 300, None, None, 500).unwrap().is_empty());
    }

    #[test]
    fn locked_until_unlocked() {
        let home = tempfile::tempdir().unwrap();
        assert!(unlock(Some(home.path().display().to_string()), "pw".into()).is_err());
        assert!(!is_unlocked() && overview().is_err());
        assert!(!home.path().join("storage").exists(), "a failed unlock must not create anything");
    }

    #[test]
    fn bars_downsample_and_projects_list_their_series() {
        use muninn_core::model::Bar;
        use muninn_core::store::SeriesRef;
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("coinbase", "csv", None).unwrap();
        let t0 = 1_704_067_200_000i64;
        let bars: Vec<Bar> =
            (0..1000).map(|i| Bar { ts: t0 + i * 300_000, open: 1.0, high: 2.0 + i as f64, low: 0.5, close: 1.5, volume: 1.0, flags: 0 }).collect();
        s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(300), &bars).unwrap();
        let all = query_of(&s, "coinbase".into(), "BTC".into(), "USD".into(), 300, None, None, 10_000).unwrap();
        assert_eq!(all.len(), 1000);
        let few = query_of(&s, "coinbase".into(), "BTC".into(), "USD".into(), 300, None, None, 100).unwrap();
        assert!(few.len() <= 101 && few.len() >= 50, "{}", few.len());
        assert_eq!(few.iter().map(|b| b.volume).sum::<f64>(), 1000.0, "buckets keep the volume");
        let ranged = query_of(&s, "coinbase".into(), "BTC".into(), "USD".into(), 300, Some(t0), Some(t0 + 99 * 300_000), 10_000).unwrap();
        assert_eq!(ranged.len(), 100);
        let p = s.create_project("Alpha", "").unwrap();
        let r = SeriesRef { source: "coinbase".into(), symbol: Symbol::new("BTC", "USD"), duration: Duration(300), from: Some(t0), to: None };
        s.add_project_result(&p.id, &r).unwrap();
        let ps = project_series_of(&s, &p.id).unwrap();
        assert_eq!(ps.len(), 1);
        assert!(ps[0].item_id > 0);
        assert_eq!((ps[0].base.as_str(), ps[0].from, ps[0].to), ("BTC", Some(t0), None));
        assert_eq!(overview_of(&s).unwrap().datasets[0].count, 1000);
    }
}
