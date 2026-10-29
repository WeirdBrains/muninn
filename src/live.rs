//! Running live searches against the data stored now. See `docs/live-searches.md`.
use crate::store::{BarFilter, Entry, Journal, SearchSpec, SearchTarget, SeriesInfo, Store};
use crate::tui::sidebar::{self, SidebarPrefs};
use serde::Serialize;
use std::collections::HashMap;

/// One match.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Found {
    Series { series: SeriesInfoOut },
    Entry { journal: String, journal_name: String, entry: Entry },
}

/// A series match (serializable copy of [`SeriesInfo`]).
#[derive(Debug, Clone, Serialize)]
pub struct SeriesInfoOut {
    pub source: String,
    pub symbol: String,
    pub duration: String,
    pub bars: i64,
    pub first: String,
    pub last: String,
}

impl From<&SeriesInfo> for SeriesInfoOut {
    fn from(s: &SeriesInfo) -> Self {
        let t = crate::ingest::time::fmt_ts;
        Self {
            source: s.source.clone(),
            symbol: s.symbol.display(),
            duration: s.duration.to_string(),
            bars: s.count,
            first: t(s.first),
            last: t(s.last),
        }
    }
}

impl Found {
    /// One line for listings.
    pub fn line(&self) -> String {
        match self {
            Found::Series { series: s } => {
                format!("{}\t{}\t{}\t{} bars\t{} → {}", s.symbol, s.source, s.duration, s.bars, &s.first[..10], &s.last[..10])
            }
            Found::Entry { journal_name, entry, .. } => format!("{}\t{journal_name}\t{}", entry.short_id(), entry.heading()),
        }
    }
}

/// Series matching a spec, in the order the sidebar would show them.
pub fn series_matches(store: &Store, spec: &SearchSpec) -> crate::Result<Vec<SeriesInfo>> {
    let all = store.series()?;
    let p = SidebarPrefs { sort: spec.sort, desc: spec.desc, filters: spec.filters.clone() };
    let scope = spec.filters.scope();
    let metric = spec.filters.price.metric;
    let mut ranges: HashMap<usize, Option<(f64, f64)>> = HashMap::new();
    if spec.filters.price.active() {
        for (i, s) in all.iter().enumerate() {
            let f = BarFilter {
                source: Some(s.source.clone()),
                symbol: Some(s.symbol.clone()),
                duration: Some(s.duration),
                from: scope.map(|x| x.0),
                to: scope.map(|x| x.1),
                ..Default::default()
            };
            ranges.insert(i, store.metric_range(&f, metric)?);
        }
    }
    let idx = |s: &SeriesInfo| all.iter().position(|x| std::ptr::eq(x, s));
    let price = |s: &SeriesInfo| idx(s).and_then(|i| ranges.get(&i).copied().flatten());
    Ok(sidebar::rows(&all, &spec.query, &p, &price).into_iter().map(|i| all[i].clone()).collect())
}

/// Journal entries matching a spec: the text (title and body), and the time range on when
/// they were written. Best matches first with a query, else newest first (`desc` flips).
pub fn entry_matches(store: &Store, spec: &SearchSpec) -> crate::Result<Vec<(Journal, Entry)>> {
    let journals: HashMap<String, Journal> = store.journals()?.into_iter().map(|j| (j.id.clone(), j)).collect();
    let (from, to) = (spec.filters.from.unwrap_or(i64::MIN), spec.filters.to.unwrap_or(i64::MAX));
    let mut v: Vec<(i64, Entry)> = store
        .all_entries()?
        .into_iter()
        .filter(|e| e.created_at >= from && e.created_at <= to)
        .filter_map(|e| sidebar::fuzzy(&spec.query, &format!("{} {}", e.title, e.body)).map(|sc| (sc, e)))
        .collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.1.created_at));
    if spec.desc {
        v.reverse();
    }
    if !spec.query.trim().is_empty() {
        v.sort_by_key(|x| std::cmp::Reverse(x.0));
    }
    Ok(v.into_iter().filter_map(|(_, e)| journals.get(&e.journal_id).cloned().map(|j| (j, e))).collect())
}

/// Run a spec on the current data.
pub fn run(store: &Store, spec: &SearchSpec) -> crate::Result<Vec<Found>> {
    Ok(match spec.target {
        SearchTarget::Series => series_matches(store, spec)?.iter().map(|s| Found::Series { series: s.into() }).collect(),
        SearchTarget::Journals => {
            entry_matches(store, spec)?.into_iter().map(|(j, e)| Found::Entry { journal: j.id, journal_name: j.name, entry: e }).collect()
        }
    })
}

/// `1 entry`, `2 entries`.
pub fn entries(n: usize) -> String {
    if n == 1 { "1 entry".into() } else { format!("{n} entries") }
}

/// `1 match`, `2 matches`.
pub fn matches(n: usize) -> String {
    if n == 1 { "1 match".into() } else { format!("{n} matches") }
}

/// Number of matches now (for listings).
pub fn count(store: &Store, spec: &SearchSpec) -> usize {
    run(store, spec).map(|v| v.len()).unwrap_or(0)
}

/// What an item is and what it holds now.
pub fn describe_item(store: &Store, it: &crate::store::ProjectItem) -> (String, String) {
    match it.kind {
        crate::store::ItemKind::LiveSearch => match store.live_search(&it.ref_id).ok().flatten() {
            Some(s) => (s.name.clone(), format!("{} now · {}", matches(count(store, &s.spec)), s.spec.describe())),
            None => ("(removed live search)".into(), String::new()),
        },
        crate::store::ItemKind::Result => match it.series() {
            Some(r) => (r.label(), format!("{} bars now", store.count_filtered(&r.filter()).unwrap_or(0))),
            None => ("(unreadable result)".into(), String::new()),
        },
        crate::store::ItemKind::Journal => match store.journal(&it.ref_id).ok().flatten() {
            Some(j) => {
                let n = store.entries(&j.id).map(|v| v.len()).unwrap_or(0);
                let own = if j.project_id.as_deref() == Some(it.project_id.as_str()) { "project journal" } else { "linked" };
                (j.name, format!("{} · {own}", entries(n)))
            }
            None => ("(removed journal)".into(), String::new()),
        },
        crate::store::ItemKind::RuleSet => (it.ref_id.clone(), "rule sets arrive in a later release".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, Duration, Symbol};
    use crate::store::EntryFormat;

    #[test]
    fn counts_read_as_words() {
        assert_eq!((matches(1), matches(0), matches(2)), ("1 match".into(), "0 matches".into(), "2 matches".into()));
        assert_eq!((entries(1), entries(3)), ("1 entry".into(), "3 entries".into()));
        assert_eq!(crate::store::ItemKind::LiveSearch.plural(), "live searches");
    }

    fn bars(n: i64, px: f64) -> Vec<Bar> {
        (0..n).map(|i| Bar { ts: i * 300_000, open: px, high: px + 1.0, low: px - 1.0, close: px, volume: 1.0, flags: 0 }).collect()
    }

    #[test]
    fn results_follow_the_data() {
        let mut s = Store::open_in_memory().unwrap();
        let mut spec = SearchSpec { query: "btc".into(), ..Default::default() };
        spec.filters.sources.insert("binance".into());
        let l = s.save_live_search("btc", &spec, None).unwrap();
        assert_eq!(count(&s, &l.spec), 0);
        let b = s.add_source("binance", "file", None).unwrap();
        let k = s.add_source("kraken", "file", None).unwrap();
        s.upsert_bars(&b, &Symbol::new("BTC", "USDT"), Duration(300), &bars(5, 100.0)).unwrap();
        s.upsert_bars(&k, &Symbol::new("BTC", "USDT"), Duration(300), &bars(5, 100.0)).unwrap();
        s.upsert_bars(&b, &Symbol::new("ETH", "USDT"), Duration(300), &bars(5, 10.0)).unwrap();
        let v = run(&s, &l.spec).unwrap();
        assert_eq!(v.len(), 1, "only BTC on binance");
        s.upsert_bars(&b, &Symbol::new("BTC", "USDT"), Duration(3600), &bars(3, 100.0)).unwrap();
        assert_eq!(count(&s, &l.spec), 2, "new data shows up without changing the search");
        // price filter uses the stored bars
        let mut p = SearchSpec::default();
        p.filters.price.min = Some(50.0);
        assert_eq!(series_matches(&s, &p).unwrap().iter().filter(|x| x.symbol.base == "ETH").count(), 0);
        s.remove_source("binance").unwrap();
        assert_eq!(count(&s, &l.spec), 0);
    }

    #[test]
    fn journal_searches() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("log", None, EntryFormat::Markdown).unwrap();
        s.add_entry(&j.id, "Breakout", "BTC broke out", None).unwrap();
        s.add_entry(&j.id, "Quiet", "nothing happened", None).unwrap();
        let spec = SearchSpec { target: SearchTarget::Journals, query: "broke".into(), ..Default::default() };
        let v = run(&s, &spec).unwrap();
        assert_eq!(v.len(), 1);
        assert!(v[0].line().contains("Breakout"));
        let all = SearchSpec { target: SearchTarget::Journals, ..Default::default() };
        assert_eq!(count(&s, &all), 2);
    }
}
