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
    Record { model: String, model_name: String, record: crate::store::Record },
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
            Found::Record { model_name, record, .. } => format!("{}\t{model_name}\t{}", record.short_id(), record_brief(record)),
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

/// A record's values on one line: `price=1.5 side=buy`.
pub fn record_brief(r: &crate::store::Record) -> String {
    r.values
        .iter()
        .filter(|(_, v)| !v.is_null())
        .take(6)
        .map(|(k, v)| {
            let s = crate::designer::value_text(v);
            let s: String = if s.chars().count() > 40 { format!("{}…", s.chars().take(39).collect::<String>()) } else { s };
            format!("{k}={s}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Records matching a spec: one model (or every model but bars), newest first (`desc`
/// flips), best matches first with a query. Models lacking a field the conditions name
/// are skipped.
pub fn record_matches(store: &Store, spec: &SearchSpec) -> crate::Result<Vec<(crate::store::Model, crate::store::Record)>> {
    let models = match &spec.model {
        Some(id) => vec![store.model(id)?.ok_or_else(|| crate::Error::invalid("model", format!("`{id}` no longer exists")))?],
        None => store.models()?.into_iter().filter(|m| !m.is_bars()).collect(),
    };
    let mut filter = spec.conditions.clone();
    if let Some(f) = spec.filters.from {
        filter.push(crate::store::Condition { field: "_created_at".into(), op: crate::store::Op::Ge, value: f.to_string() });
    }
    if let Some(t) = spec.filters.to {
        filter.push(crate::store::Condition { field: "_created_at".into(), op: crate::store::Op::Le, value: t.to_string() });
    }
    let sort = if spec.query.trim().is_empty() { vec![("_created_at".to_string(), !spec.desc)] } else { vec![] };
    let mut out = vec![];
    for m in models.into_iter().filter(|m| m.version > 0) {
        let q = crate::store::RecordQuery { filter: filter.clone(), text: spec.query.clone(), sort: sort.clone(), ..Default::default() };
        match store.records(&m, &q) {
            Ok(v) => out.extend(v.into_iter().map(|r| (m.clone(), r))),
            Err(e) if spec.model.is_none() => tracing::debug!("live search skips model {}: {e}", m.name),
            Err(e) => return Err(e),
        }
    }
    if spec.query.trim().is_empty() && spec.model.is_none() {
        out.sort_by_key(|(_, r)| std::cmp::Reverse(r.created_at));
        if spec.desc {
            out.reverse();
        }
    }
    Ok(out)
}

/// Run a spec on the current data.
pub fn run(store: &Store, spec: &SearchSpec) -> crate::Result<Vec<Found>> {
    Ok(match spec.target {
        SearchTarget::Series => series_matches(store, spec)?.iter().map(|s| Found::Series { series: s.into() }).collect(),
        SearchTarget::Journals => {
            entry_matches(store, spec)?.into_iter().map(|(j, e)| Found::Entry { journal: j.id, journal_name: j.name, entry: e }).collect()
        }
        SearchTarget::Records => {
            record_matches(store, spec)?.into_iter().map(|(m, r)| Found::Record { model: m.id, model_name: m.name, record: r }).collect()
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
        crate::store::ItemKind::Report => match store.report(&it.ref_id).ok().flatten() {
            Some(r) => {
                let n = store.sections(&r.id).map(|v| v.len()).unwrap_or(0);
                (r.name, format!("{n} section{}", if n == 1 { "" } else { "s" }))
            }
            None => ("(removed report)".into(), String::new()),
        },
        crate::store::ItemKind::RuleSet => match store.rule(&it.ref_id).ok().flatten() {
            Some(r) => (r.name.clone(), crate::rules::brief(&r)),
            None => ("(removed rule)".into(), String::new()),
        },
        crate::store::ItemKind::Model => match store.model(&it.ref_id).ok().flatten() {
            Some(m) => {
                let n = store.count_records(&m).unwrap_or(0);
                (
                    m.name.clone(),
                    format!(
                        "v{} · {} field{} · {n} record{}",
                        m.version,
                        m.design.fields.len(),
                        if m.design.fields.len() == 1 { "" } else { "s" },
                        if n == 1 { "" } else { "s" }
                    ),
                )
            }
            None => ("(removed model)".into(), String::new()),
        },
        crate::store::ItemKind::Code => match store.code_space(&it.ref_id).ok().flatten() {
            Some(c) => {
                let run = match &c.last_run {
                    Some(r) => format!("{} {}", r.status, crate::ingest::time::fmt_ts(r.at)),
                    None => "not run yet".into(),
                };
                (c.name.clone(), format!("{} · {run}", c.lang.as_str()))
            }
            None => ("(removed code space)".into(), String::new()),
        },
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

    #[test]
    fn record_searches() {
        use crate::store::{Design, FieldOp};
        let s = Store::open_in_memory().unwrap();
        let (m, _) = s.create_model("signals", "", Design::default(), false).unwrap();
        s.edit_model(&m.id, &[FieldOp::Add { spec: "symbol:text!".into() }, FieldOp::Add { spec: "price:real".into() }]).unwrap();
        let (m, _) = s.apply_model(&m.id, &Default::default()).unwrap();
        let (n, _) = s.create_model("notes", "", Design::default(), false).unwrap();
        s.edit_model(&n.id, &[FieldOp::Add { spec: "text:text".into() }]).unwrap();
        let (n, _) = s.apply_model(&n.id, &Default::default()).unwrap();
        for (sym, p) in [("BTC/USDT", "42000"), ("ETH/USDT", "2500")] {
            s.create_record(&m, &serde_json::json!({ "symbol": sym, "price": p }).as_object().unwrap().clone()).unwrap();
        }
        s.create_record(&n, &serde_json::json!({ "text": "btc looks strong" }).as_object().unwrap().clone()).unwrap();
        // text over every model (not bars)
        let spec = SearchSpec { target: SearchTarget::Records, query: "btc".into(), ..Default::default() };
        let v = run(&s, &spec).unwrap();
        assert_eq!(v.len(), 2, "{:?}", v.iter().map(Found::line).collect::<Vec<_>>());
        // one model with conditions; models without the field are skipped when searching all
        let cond = vec![crate::rules::parse_condition("price > 10000").unwrap()];
        let one = SearchSpec { target: SearchTarget::Records, model: Some(m.id.clone()), conditions: cond.clone(), ..Default::default() };
        let v = run(&s, &one).unwrap();
        assert!(v.len() == 1 && v[0].line().contains("BTC/USDT"));
        let all = SearchSpec { target: SearchTarget::Records, conditions: cond, ..Default::default() };
        assert_eq!(count(&s, &all), 1);
        // results follow the data
        s.create_record(&m, &serde_json::json!({ "symbol": "SOL/USDT", "price": 20000 }).as_object().unwrap().clone()).unwrap();
        assert_eq!(count(&s, &all), 2);
    }
}
