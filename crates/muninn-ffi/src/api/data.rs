//! Views of the store: projects, datasets (series) and downsampled bars.
//! Unlocking needs the Muninn password; the key stays in Rust. Dashboard reads open
//! the store read-only (no migrations). Import and export open it for writes.
use super::info::paths;
use muninn_core::crypto::Key32;
use muninn_core::model::{Duration, Symbol};
use muninn_core::store::{BarFilter, NewAction, Store};
use serde_json::json;
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

pub(crate) fn open() -> anyhow::Result<Store> {
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

/// One ingest source (`mnnn sources list` / TUI Data ▸ Sources…).
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub locator: Option<String>,
    pub created_at: i64,
    pub items: i64,
    pub bytes: i64,
}

/// One stored item under a source (`mnnn sources data`), without its bytes.
pub struct SourceDataItem {
    pub id: String,
    pub name: String,
    pub format: String,
    pub locator: String,
    pub size: i64,
    pub content_hash: String,
    pub received_at: i64,
}

/// A known trading pair (`mnnn symbols list` / TUI Data ▸ Symbols…).
pub struct SymbolInfo {
    pub symbol: String,
    pub base: String,
    pub quote: String,
}

/// An asset alias (`alias XBT -> BTC` on `mnnn symbols list`).
pub struct AliasInfo {
    pub alias: String,
    pub canonical: String,
}

fn source_info(s: &Store, src: muninn_core::model::Source) -> anyhow::Result<SourceInfo> {
    let (items, bytes) = s.source_data_totals(&src.id)?;
    Ok(SourceInfo {
        id: src.id,
        name: src.name,
        kind: src.kind,
        locator: src.locator,
        created_at: src.created_at,
        items: items as i64,
        bytes: bytes as i64,
    })
}

pub(crate) fn list_sources_of(s: &Store) -> anyhow::Result<Vec<SourceInfo>> {
    s.sources()?.into_iter().map(|src| source_info(s, src)).collect()
}

/// `mnnn sources list`. The store is opened read-only.
pub fn list_sources() -> anyhow::Result<Vec<SourceInfo>> {
    list_sources_of(&open()?)
}

pub(crate) fn add_source_of(s: &Store, name: &str, kind: &str, locator: Option<&str>) -> anyhow::Result<SourceInfo> {
    let kind = if kind.trim().is_empty() { "file" } else { kind.trim() };
    let src = s.add_source(name, kind, locator)?;
    source_info(s, src)
}

/// `mnnn sources add`. Opens the store for writes.
pub fn add_source(name: String, kind: String, locator: Option<String>) -> anyhow::Result<SourceInfo> {
    let s = open_write()?;
    let info = add_source_of(&s, &name, &kind, locator.as_deref())?;
    s.note_action(
        &NewAction::new("source.add", "app", format!("add source {}", info.name)).params(json!({ "source": info.name, "kind": info.kind })),
    );
    Ok(info)
}

pub(crate) fn rename_source_of(s: &mut Store, from: &str, to: &str) -> anyhow::Result<i64> {
    Ok(s.rename_source(from, to)? as i64)
}

/// `mnnn sources rename`. Recomputes bar ids; returns how many bars changed.
pub fn rename_source(from: String, to: String) -> anyhow::Result<i64> {
    let mut s = open_write()?;
    let n = rename_source_of(&mut s, &from, &to)?;
    s.note_action(
        &NewAction::new("source.rename", "app", format!("rename source {from} to {to}")).params(json!({ "from": from, "to": to, "bars": n })),
    );
    Ok(n)
}

pub(crate) fn remove_source_of(s: &Store, name: &str) -> anyhow::Result<i64> {
    Ok(s.remove_source(name)? as i64)
}

/// `mnnn sources remove`. Deletes the source, its bars and its stored items.
pub fn remove_source(name: String) -> anyhow::Result<i64> {
    let s = open_write()?;
    let n = remove_source_of(&s, &name)?;
    s.note_action(&NewAction::new("source.remove", "app", format!("remove source {name} ({n} bars)")).params(json!({ "source": name, "bars": n })));
    Ok(n)
}

fn data_item(i: muninn_core::store::SourceData) -> SourceDataItem {
    SourceDataItem {
        id: i.id,
        name: i.name,
        format: i.format,
        locator: i.locator,
        size: i.size.min(i64::MAX as u64) as i64,
        content_hash: i.content_hash,
        received_at: i.received_at,
    }
}

pub(crate) fn list_source_data_of(s: &Store, name: &str) -> anyhow::Result<Vec<SourceDataItem>> {
    let src = s.source_by_name(name)?.ok_or_else(|| anyhow::anyhow!("no source `{name}`"))?;
    Ok(s.source_data(&src.id)?.into_iter().map(data_item).collect())
}

/// `mnnn sources data NAME`. Metadata only; bytes stay in Rust.
pub fn list_source_data(name: String) -> anyhow::Result<Vec<SourceDataItem>> {
    list_source_data_of(&open()?, &name)
}

pub(crate) fn export_source_data_of(s: &Store, name: &str, item: &str, path: &str) -> anyhow::Result<i64> {
    let src = s.source_by_name(name)?.ok_or_else(|| anyhow::anyhow!("no source `{name}`"))?;
    let want = item.trim();
    let found: Vec<_> = s.source_data(&src.id)?.into_iter().filter(|i| i.id.starts_with(want)).collect();
    let i = match found.as_slice() {
        [one] => one,
        [] => anyhow::bail!("no item `{item}` in {name} (`mnnn sources data {name}`)"),
        _ => anyhow::bail!("`{item}` matches {} items; give more of the id", found.len()),
    };
    let bytes = s.source_data_bytes(&i.id)?;
    let n = bytes.len() as i64;
    std::fs::write(path, bytes)?;
    Ok(n)
}

/// `mnnn sources cat NAME ITEM`: write the stored bytes to [path]. The blob never enters Dart.
pub fn export_source_data(name: String, item: String, path: String) -> anyhow::Result<i64> {
    export_source_data_of(&open()?, &name, &item, &path)
}

pub(crate) fn list_symbols_of(s: &Store) -> anyhow::Result<Vec<SymbolInfo>> {
    Ok(s.symbols()?.into_iter().map(|sym| SymbolInfo { symbol: sym.display(), base: sym.base, quote: sym.quote }).collect())
}

/// `mnnn symbols list` (the pairs). Aliases are [`list_aliases`].
pub fn list_symbols() -> anyhow::Result<Vec<SymbolInfo>> {
    list_symbols_of(&open()?)
}

pub(crate) fn list_aliases_of(s: &Store) -> anyhow::Result<Vec<AliasInfo>> {
    Ok(s.aliases()?.into_iter().map(|(alias, canonical)| AliasInfo { alias, canonical }).collect())
}

/// `alias A -> C` lines from `mnnn symbols list`.
pub fn list_aliases() -> anyhow::Result<Vec<AliasInfo>> {
    list_aliases_of(&open()?)
}

pub(crate) fn add_symbol_of(s: &Store, symbol: &str, aliases: &[String]) -> anyhow::Result<SymbolInfo> {
    let sym = Symbol::parse(symbol)?;
    s.add_symbol(&sym, aliases)?;
    Ok(SymbolInfo { symbol: sym.display(), base: sym.base, quote: sym.quote })
}

/// `mnnn symbols add`. [aliases] apply to the base asset (e.g. XBT for BTC).
pub fn add_symbol(symbol: String, aliases: Vec<String>) -> anyhow::Result<SymbolInfo> {
    let s = open_write()?;
    let info = add_symbol_of(&s, &symbol, &aliases)?;
    s.note_action(
        &NewAction::new("symbol.add", "app", format!("add symbol {}", info.symbol)).params(json!({ "symbol": info.symbol, "aliases": aliases })),
    );
    Ok(info)
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

    #[test]
    fn sources_and_symbols_match_cli() {
        use muninn_core::store::NewSourceData;
        let mut s = Store::open_in_memory().unwrap();
        let added = add_source_of(&s, "local", "directory", Some("/d")).unwrap();
        assert_eq!((added.name.as_str(), added.kind.as_str(), added.locator.as_deref(), added.items), ("local", "directory", Some("/d"), 0));
        assert_eq!(list_sources_of(&s).unwrap().len(), 1);

        let bytes = b"hello,bars\n";
        let n = NewSourceData { locator: "/d/x.csv", name: "x.csv", format: "csv", content_hash: "h1", data: bytes, run_id: None };
        s.add_source_data(&added.id, &n).unwrap();
        let items = list_source_data_of(&s, "local").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].name.as_str(), items[0].size, items[0].format.as_str()), ("x.csv", bytes.len() as i64, "csv"));
        assert_eq!(list_sources_of(&s).unwrap()[0].items, 1);

        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.csv");
        let wrote = export_source_data_of(&s, "local", &items[0].id[..8], dest.to_str().unwrap()).unwrap();
        assert_eq!(wrote, bytes.len() as i64);
        assert_eq!(std::fs::read(&dest).unwrap(), bytes);

        let src = s.add_source("coinbase", "csv", None).unwrap();
        let t0 = 1_704_067_200_000i64;
        let bars = vec![muninn_core::model::Bar { ts: t0, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 1.0, flags: 0 }];
        s.upsert_bars(&src, &Symbol::new("BTC", "USD"), Duration(300), &bars).unwrap();
        let n = rename_source_of(&mut s, "coinbase", "cb").unwrap();
        assert_eq!(n, 1);
        assert!(s.source_by_name("coinbase").unwrap().is_none());
        assert_eq!(s.source_by_name("cb").unwrap().unwrap().name, "cb");

        let sym = add_symbol_of(&s, "ETH/USDT", &["ETHER".into()]).unwrap();
        assert_eq!((sym.symbol.as_str(), sym.base.as_str(), sym.quote.as_str()), ("ETH/USDT", "ETH", "USDT"));
        let symbols = list_symbols_of(&s).unwrap();
        assert!(symbols.iter().any(|x| x.symbol == "BTC/USD"));
        assert!(symbols.iter().any(|x| x.symbol == "ETH/USDT"));
        let aliases = list_aliases_of(&s).unwrap();
        assert!(aliases.iter().any(|a| a.alias == "ETHER" && a.canonical == "ETH"));

        let removed = remove_source_of(&s, "local").unwrap();
        assert_eq!(removed, 0);
        assert!(list_sources_of(&s).unwrap().iter().all(|x| x.name != "local"));
        assert!(list_source_data_of(&s, "local").is_err());
    }
}
