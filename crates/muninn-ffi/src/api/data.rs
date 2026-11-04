//! Read-only views of the store: projects, datasets (series) and downsampled bars.
//! The store is opened read-only (no migrations, no writes); unlocking needs the Muninn password.
use super::info::paths;
use muninn_core::model::{Duration, Symbol};
use muninn_core::store::{BarFilter, Store};

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

fn open(home: Option<String>, password: String) -> anyhow::Result<Store> {
    let p = paths(home)?;
    let pw = secrecy::SecretString::from(password);
    let dek = muninn_core::crypto::keyring::unlock_with_password(&p, &pw)?;
    Ok(Store::open_readonly(&p.db, Some(&dek.0))?)
}

/// Projects and datasets, read-only.
pub fn overview(home: Option<String>, password: String) -> anyhow::Result<Overview> {
    let s = open(home, password)?;
    overview_of(&s)
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
    home: Option<String>,
    password: String,
    source: String,
    base: String,
    quote: String,
    duration_s: i64,
    from: Option<i64>,
    to: Option<i64>,
    max_points: u32,
) -> anyhow::Result<Vec<BarPoint>> {
    let s = open(home, password)?;
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
    fn overview_needs_initialized_keys() {
        let home = tempfile::tempdir().unwrap();
        assert!(overview(Some(home.path().display().to_string()), "pw".into()).is_err());
        assert!(!home.path().join("storage").exists(), "a failed unlock must not create anything");
    }
}
