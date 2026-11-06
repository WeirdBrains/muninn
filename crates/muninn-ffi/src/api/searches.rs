//! Live searches (`mnnn search` / TUI Live searches): save, rename, refine, fork and remove.
use super::data::open_write;
use muninn_core::model::{Duration, PriceMetric, Symbol};
use muninn_core::store::{LiveSearch, NewAction, SearchSpec, SearchTarget, Store};
use muninn_core::tui::sidebar::SortBy;

pub struct SearchInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub target: String,
    pub query: String,
    pub describe: String,
    pub matches: i64,
    pub forked_from: Option<String>,
    pub sources: Vec<String>,
    pub symbols: Vec<String>,
    pub periods: Vec<String>,
    pub years: Vec<i64>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub price_metric: String,
    pub price_min: Option<f64>,
    pub price_max: Option<f64>,
    pub sort: String,
    pub desc: bool,
    pub model: Option<String>,
    pub conditions: Vec<String>,
}

fn info(store: &Store, s: LiveSearch) -> SearchInfo {
    let f = &s.spec.filters;
    SearchInfo {
        short_id: s.short_id().to_string(),
        describe: s.spec.describe(),
        target: s.spec.target.label().into(),
        query: s.spec.query.clone(),
        matches: muninn_core::live::count(store, &s.spec) as i64,
        sources: f.sources.iter().cloned().collect(),
        symbols: f.symbols.iter().cloned().collect(),
        periods: f.durations.iter().cloned().collect(),
        years: f.years.iter().map(|&y| y as i64).collect(),
        from: f.from,
        to: f.to,
        price_metric: f.price.metric.label().into(),
        price_min: f.price.min,
        price_max: f.price.max,
        sort: match s.spec.sort {
            SortBy::Name => "name".into(),
            SortBy::Year => "year".into(),
        },
        desc: s.spec.desc,
        model: s.spec.model.clone(),
        conditions: s.spec.conditions.iter().map(muninn_core::rules::condition_text).collect(),
        id: s.id,
        name: s.name,
        forked_from: s.forked_from,
    }
}

fn note(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "app", summary).params(params));
}

fn parse_target(t: &str) -> anyhow::Result<SearchTarget> {
    match t.trim().to_ascii_lowercase().as_str() {
        "" | "series" => Ok(SearchTarget::Series),
        "journals" | "journal" => Ok(SearchTarget::Journals),
        "records" | "record" => Ok(SearchTarget::Records),
        other => Err(anyhow::anyhow!("`{other}`: search series, journals or records")),
    }
}

fn parse_price(metric: &str) -> anyhow::Result<PriceMetric> {
    let m = metric.trim().to_lowercase();
    PriceMetric::ALL
        .into_iter()
        .find(|x| x.label() == m || x.label().replace(' ', "") == m || (m == "ohlc" && *x == PriceMetric::OhlcAvg))
        .ok_or_else(|| anyhow::anyhow!("invalid --price: `{metric}` (open, high, low, close, typical, median, ohlc, vwap)"))
}

fn parse_sort(s: &str) -> SortBy {
    if s.trim().eq_ignore_ascii_case("name") { SortBy::Name } else { SortBy::Year }
}

fn nonempty_parts(items: &[String]) -> Vec<String> {
    items.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Sidebar filters plus query/target/sort, matching `mnnn search save|edit` / TUI `w`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spec_from(
    store: &Store,
    query: &str,
    target: &str,
    sources: &[String],
    symbols: &[String],
    periods: &[String],
    years: &[i64],
    from: Option<i64>,
    to: Option<i64>,
    price: Option<&str>,
    min: Option<f64>,
    max: Option<f64>,
    sort: Option<&str>,
    desc: bool,
    model: Option<&str>,
    conditions: &[String],
) -> anyhow::Result<SearchSpec> {
    let mut spec = SearchSpec { target: parse_target(target)?, query: query.to_string(), desc, ..Default::default() };
    if let Some(s) = sort {
        spec.sort = parse_sort(s);
    }
    if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
        spec.model = if m.eq_ignore_ascii_case("all") { None } else { Some(store.find_model(m)?.id) };
    }
    if !conditions.is_empty() {
        spec.conditions = muninn_core::designer::parse_where(conditions)?;
    }
    let f = &mut spec.filters;
    f.sources = nonempty_parts(sources).into_iter().collect();
    f.symbols =
        nonempty_parts(symbols).into_iter().map(|s| Ok(store.canonical_symbol(&Symbol::parse(&s)?)?.display())).collect::<anyhow::Result<_>>()?;
    f.durations = nonempty_parts(periods).into_iter().map(|d| Ok(Duration::parse(&d)?.to_string())).collect::<anyhow::Result<_>>()?;
    f.years = years.iter().map(|&y| y as i32).collect();
    f.from = from;
    f.to = to;
    if let Some(m) = price.map(str::trim).filter(|s| !s.is_empty()) {
        f.price.metric = parse_price(m)?;
    }
    f.price.min = min;
    f.price.max = max;
    Ok(spec)
}

pub(crate) fn searches_of(store: &Store) -> anyhow::Result<Vec<SearchInfo>> {
    Ok(store.live_searches()?.into_iter().map(|s| info(store, s)).collect())
}

/// `mnnn search list`.
pub fn searches() -> anyhow::Result<Vec<SearchInfo>> {
    searches_of(&super::data::open()?)
}

pub(crate) fn save_search_of(store: &Store, name: &str, query: &str, target: &str) -> anyhow::Result<SearchInfo> {
    save_search_spec_of(store, name, query, target, &[], &[], &[], &[], None, None, None, None, None, None, false, None, &[])
}

/// `mnnn search save NAME --query …` (`--journals` / `--records` via `target`).
pub fn save_search(name: String, query: String, target: String) -> anyhow::Result<SearchInfo> {
    save_search_of(&open_write()?, &name, &query, &target)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn save_search_spec_of(
    store: &Store,
    name: &str,
    query: &str,
    target: &str,
    sources: &[String],
    symbols: &[String],
    periods: &[String],
    years: &[i64],
    from: Option<i64>,
    to: Option<i64>,
    price: Option<&str>,
    min: Option<f64>,
    max: Option<f64>,
    sort: Option<&str>,
    desc: bool,
    model: Option<&str>,
    conditions: &[String],
) -> anyhow::Result<SearchInfo> {
    let spec = spec_from(store, query, target, sources, symbols, periods, years, from, to, price, min, max, sort, desc, model, conditions)?;
    let s = store.save_live_search(name, &spec, None)?;
    note(store, "search.save", format!("save live search {}", s.name), serde_json::json!({ "search": s.id, "name": s.name }));
    Ok(info(store, s))
}

/// `mnnn search save` with the full sidebar filters (sources, symbols, periods, years, range, price, sort).
#[allow(clippy::too_many_arguments)]
pub fn save_search_spec(
    name: String,
    query: String,
    target: String,
    sources: Vec<String>,
    symbols: Vec<String>,
    periods: Vec<String>,
    years: Vec<i64>,
    from: Option<i64>,
    to: Option<i64>,
    price: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
    sort: Option<String>,
    desc: bool,
    model: Option<String>,
    conditions: Vec<String>,
) -> anyhow::Result<SearchInfo> {
    save_search_spec_of(
        &open_write()?,
        &name,
        &query,
        &target,
        &sources,
        &symbols,
        &periods,
        &years,
        from,
        to,
        price.as_deref(),
        min,
        max,
        sort.as_deref(),
        desc,
        model.as_deref(),
        &conditions,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn edit_search_of(
    store: &Store,
    id: &str,
    name: Option<&str>,
    query: &str,
    target: &str,
    sources: &[String],
    symbols: &[String],
    periods: &[String],
    years: &[i64],
    from: Option<i64>,
    to: Option<i64>,
    price: Option<&str>,
    min: Option<f64>,
    max: Option<f64>,
    sort: Option<&str>,
    desc: bool,
    model: Option<&str>,
    conditions: &[String],
) -> anyhow::Result<SearchInfo> {
    let found = store.find_live_search(id)?;
    let spec = spec_from(store, query, target, sources, symbols, periods, years, from, to, price, min, max, sort, desc, model, conditions)?;
    let n = store.update_live_search(&found.id, name.map(str::trim).filter(|s| !s.is_empty()), Some(&spec))?;
    note(store, "search.edit", format!("refine live search {}", n.name), serde_json::json!({ "search": n.id, "name": n.name }));
    Ok(info(store, n))
}

/// Refine or overwrite a live search (`mnnn search edit` / TUI `w`).
#[allow(clippy::too_many_arguments)]
pub fn edit_search(
    id: String,
    name: Option<String>,
    query: String,
    target: String,
    sources: Vec<String>,
    symbols: Vec<String>,
    periods: Vec<String>,
    years: Vec<i64>,
    from: Option<i64>,
    to: Option<i64>,
    price: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
    sort: Option<String>,
    desc: bool,
    model: Option<String>,
    conditions: Vec<String>,
) -> anyhow::Result<SearchInfo> {
    edit_search_of(
        &open_write()?,
        &id,
        name.as_deref(),
        &query,
        &target,
        &sources,
        &symbols,
        &periods,
        &years,
        from,
        to,
        price.as_deref(),
        min,
        max,
        sort.as_deref(),
        desc,
        model.as_deref(),
        &conditions,
    )
}

pub(crate) fn rename_search_of(store: &Store, id: &str, name: &str) -> anyhow::Result<SearchInfo> {
    let s = store.find_live_search(id)?;
    let n = store.update_live_search(&s.id, Some(name), None)?;
    note(store, "search.edit", format!("rename live search {} → {}", s.name, n.name), serde_json::json!({ "search": n.id, "name": n.name }));
    Ok(info(store, n))
}

pub fn rename_search(id: String, name: String) -> anyhow::Result<SearchInfo> {
    rename_search_of(&open_write()?, &id, &name)
}

pub(crate) fn fork_search_of(store: &Store, id: &str, name: Option<&str>) -> anyhow::Result<SearchInfo> {
    let s = store.find_live_search(id)?;
    let n = store.fork_live_search(&s.id, name.map(str::trim).filter(|x| !x.is_empty()), None)?;
    note(store, "search.fork", format!("fork live search {} → {}", s.name, n.name), serde_json::json!({ "search": n.id, "name": n.name }));
    Ok(info(store, n))
}

pub fn fork_search(id: String, name: Option<String>) -> anyhow::Result<SearchInfo> {
    fork_search_of(&open_write()?, &id, name.as_deref())
}

pub(crate) fn remove_search_of(store: &Store, id: &str) -> anyhow::Result<String> {
    let s = store.find_live_search(id)?;
    let name = s.name.clone();
    store.remove_live_search(&s.id)?;
    note(store, "search.remove", format!("remove live search {name}"), serde_json::json!({ "search": s.id, "name": name }));
    Ok(format!("removed live search {name}"))
}

pub fn remove_search(id: String) -> anyhow::Result<String> {
    remove_search_of(&open_write()?, &id)
}

pub struct FoundEntry {
    pub journal_id: String,
    pub journal_name: String,
    pub entry_id: String,
    pub heading: String,
    pub title: String,
    pub created_at: i64,
}

pub struct FoundRecord {
    pub model_id: String,
    pub model_name: String,
    pub record_id: String,
    pub brief: String,
    pub created_at: i64,
}

fn found_spec(query: &str, target: SearchTarget, from: Option<i64>, to: Option<i64>) -> SearchSpec {
    let mut spec = SearchSpec { target, query: query.to_string(), ..Default::default() };
    spec.filters.from = from;
    spec.filters.to = to;
    spec
}

pub(crate) fn search_found_entries_of(store: &Store, query: &str, from: Option<i64>, to: Option<i64>) -> anyhow::Result<Vec<FoundEntry>> {
    Ok(muninn_core::live::entry_matches(store, &found_spec(query, SearchTarget::Journals, from, to))?
        .into_iter()
        .map(|(j, e)| FoundEntry {
            journal_id: j.id,
            journal_name: j.name,
            heading: e.heading(),
            title: e.title,
            created_at: e.created_at,
            entry_id: e.id,
        })
        .collect())
}

/// Journal entries matching a live search (TUI apply → found-views).
pub fn search_found_entries(query: String, from: Option<i64>, to: Option<i64>) -> anyhow::Result<Vec<FoundEntry>> {
    search_found_entries_of(&super::data::open()?, &query, from, to)
}

pub(crate) fn search_found_records_of(store: &Store, query: &str, from: Option<i64>, to: Option<i64>) -> anyhow::Result<Vec<FoundRecord>> {
    Ok(muninn_core::live::record_matches(store, &found_spec(query, SearchTarget::Records, from, to))?
        .into_iter()
        .map(|(m, r)| FoundRecord {
            model_id: m.id,
            model_name: m.name,
            brief: muninn_core::live::record_brief(&r),
            created_at: r.created_at,
            record_id: r.id,
        })
        .collect())
}

/// Records matching a live search (TUI apply → found-views).
pub fn search_found_records(query: String, from: Option<i64>, to: Option<i64>) -> anyhow::Result<Vec<FoundRecord>> {
    search_found_records_of(&super::data::open()?, &query, from, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn save_rename_fork_remove() {
        let s = Store::open_in_memory().unwrap();
        let a = save_search_of(&s, "BTC", "btc", "series").unwrap();
        assert_eq!((a.name.as_str(), a.target.as_str(), a.query.as_str()), ("BTC", "series", "btc"));
        assert!(a.describe.contains("btc"));
        assert!(a.sources.is_empty());
        let a = rename_search_of(&s, &a.id, "Bitcoin").unwrap();
        assert_eq!(a.name, "Bitcoin");
        let f = fork_search_of(&s, &a.id, None).unwrap();
        assert!(f.name.contains("fork"));
        assert_eq!(searches_of(&s).unwrap().len(), 2);
        assert!(remove_search_of(&s, &a.id).unwrap().contains("Bitcoin"));
        assert_eq!(searches_of(&s).unwrap().len(), 1);
    }

    #[test]
    fn save_and_refine_with_sidebar_filters() {
        let s = Store::open_in_memory().unwrap();
        let from = 1_704_067_200_000i64;
        let to = 1_704_153_600_000i64;
        let a = save_search_spec_of(
            &s,
            "Desk",
            "eth",
            "series",
            &["binance".into()],
            &["ETH/USDT".into()],
            &["1h".into()],
            &[2024],
            Some(from),
            Some(to),
            Some("close"),
            Some(10.0),
            Some(99.0),
            Some("name"),
            true,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(a.name, "Desk");
        assert_eq!(a.query, "eth");
        assert_eq!(a.sources, vec!["binance".to_string()]);
        assert_eq!(a.symbols, vec!["ETH/USDT".to_string()]);
        assert_eq!(a.periods, vec!["1h".to_string()]);
        assert_eq!(a.years, vec![2024]);
        assert_eq!(a.from, Some(from));
        assert_eq!(a.to, Some(to));
        assert_eq!(a.price_metric, "close");
        assert_eq!(a.price_min, Some(10.0));
        assert_eq!(a.price_max, Some(99.0));
        assert_eq!(a.sort, "name");
        assert!(a.desc);
        assert!(a.describe.contains("binance") || a.describe.contains("2024") || a.describe.contains("eth"));

        let a = edit_search_of(
            &s,
            &a.id,
            Some("Desk 2"),
            "btc",
            "series",
            &["coinbase".into()],
            &[],
            &["5m".into()],
            &[],
            Some(from),
            None,
            Some("typical"),
            None,
            None,
            Some("year"),
            false,
            None,
            &[],
        )
        .unwrap();
        assert_eq!((a.name.as_str(), a.query.as_str()), ("Desk 2", "btc"));
        assert_eq!(a.sources, vec!["coinbase".to_string()]);
        assert!(a.symbols.is_empty());
        assert_eq!(a.periods, vec!["5m".to_string()]);
        assert_eq!(a.from, Some(from));
        assert_eq!(a.to, None);
        assert_eq!(a.price_metric, "typical");
        assert_eq!(a.sort, "year");
        assert!(!a.desc);
    }

    #[test]
    fn found_views_match_journal_entries_and_records() {
        let s = Store::open_in_memory().unwrap();
        let j = crate::api::journals::create_journal_of(&s, "Desk", None, "markdown").unwrap();
        crate::api::journals::add_entry_of(&s, &j.id, "BTC note", "watched bitcoin break out", None).unwrap();
        crate::api::journals::add_entry_of(&s, &j.id, "ETH note", "ether stayed quiet", None).unwrap();
        let hits = search_found_entries_of(&s, "bitcoin", None, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].journal_name, "Desk");
        assert!(hits[0].heading.contains("BTC"));
        assert!(search_found_entries_of(&s, "quiet", None, None).unwrap().iter().any(|e| e.title.contains("ETH")));

        let (m, _) = s
            .create_model(
                "signals",
                "",
                muninn_core::store::Design {
                    fields: vec![muninn_core::store::Field::new("memo", muninn_core::store::FieldType::Text)],
                    ..Default::default()
                },
                true,
            )
            .unwrap();
        let mut values = serde_json::Map::new();
        values.insert("memo".into(), serde_json::json!("alpha signal"));
        s.create_record(&m, &values).unwrap();
        let recs = search_found_records_of(&s, "alpha", None, None).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].model_name, "signals");
        assert!(recs[0].brief.contains("alpha"), "{}", recs[0].brief);
        assert!(search_found_records_of(&s, "no-such", None, None).unwrap().is_empty());
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(save_search("X".into(), "q".into(), "series".into()).is_err());
        assert!(
            save_search_spec(
                "X".into(),
                "q".into(),
                "series".into(),
                vec![],
                vec![],
                vec![],
                vec![],
                None,
                None,
                None,
                None,
                None,
                None,
                false,
                None,
                vec![],
            )
            .is_err()
        );
        assert!(
            edit_search(
                "x".into(),
                None,
                "q".into(),
                "series".into(),
                vec![],
                vec![],
                vec![],
                vec![],
                None,
                None,
                None,
                None,
                None,
                None,
                false,
                None,
                vec![],
            )
            .is_err()
        );
        assert!(searches().is_err());
    }
}
