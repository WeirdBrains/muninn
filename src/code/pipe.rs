//! Pipes: code spaces are stateless transforms (a frame in, a result out) that work on
//! data, not own it. A pipe binds a source to one or more steps and, optionally, a target:
//! `search:usdt | code:fast | tee journal:Raw | code:smooth > journal:Notes`: saves are
//! side branches (the data flows on), at any step and as many as wanted. See `docs/pipes.md`.
//!
//! Sources: none, a model query (`bars where … limit N`), any live search (records, series
//! or journal entries, evaluated when the pipe runs), a journal's entries, another code
//! space's output, or a frame given as is (shared data). Steps: code spaces (each gets the
//! frame before it), or `search:NAME` filtering the frame by that search's text and
//! conditions. A run's result is the pipe's output; it is kept only where it is piped to.
use super::view::ResultView;
use super::{CodeEnv, host};
use crate::error::{Error, Result};
use crate::store::{BarFilter, CodeInput, CodeSpace, NewAction, RunInfo, SearchSpec, SearchTarget, Store};
use serde_json::{Map, Value, json};

/// Where a pipe's data comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    None,
    /// A model query (the classic code space input).
    Model(CodeInput),
    /// A live search by name or id, of any kind.
    Search(String),
    /// A journal's entries.
    Journal(String),
    /// Another code space's output (run on its own default input).
    Code(String),
    /// Data given as is: a frame (`{model, fields, ids, columns, groups}`).
    Frame(Value),
}

impl Source {
    /// Read `none`, `search:NAME`, `journal:NAME`, `code:SPACE`, `model:QUERY` or a bare
    /// model query (`bars where … limit N`).
    pub fn parse(s: &str) -> Result<Source> {
        let s = s.trim();
        let (k, rest) = s.split_once(':').map(|(k, r)| (k.trim().to_ascii_lowercase(), r.trim())).unwrap_or_default();
        let need = |what: &str| -> Result<String> {
            if rest.is_empty() { Err(Error::invalid("input", format!("`{what}:` needs a name"))) } else { Ok(rest.to_string()) }
        };
        Ok(match k.as_str() {
            "search" => Source::Search(need("search")?),
            "journal" => Source::Journal(need("journal")?),
            "code" => Source::Code(need("code")?),
            "model" => Source::of(super::parse_input(rest)?),
            _ => Source::of(super::parse_input(s)?),
        })
    }

    /// A stored default input as a source.
    pub fn of(i: CodeInput) -> Source {
        if !i.from.is_empty() {
            return Source::parse(&i.from).unwrap_or(Source::None);
        }
        if i.model.is_empty() { Source::None } else { Source::Model(i) }
    }

    /// As a stored default input (frames are not stored).
    pub fn input(&self) -> CodeInput {
        match self {
            Source::Model(i) => i.clone(),
            Source::None | Source::Frame(_) => CodeInput::default(),
            o => CodeInput { from: o.text(), ..Default::default() },
        }
    }

    pub fn text(&self) -> String {
        match self {
            Source::None => "none".into(),
            Source::Model(i) => i.text(),
            Source::Search(n) => format!("search:{n}"),
            Source::Journal(n) => format!("journal:{n}"),
            Source::Code(n) => format!("code:{n}"),
            Source::Frame(f) => format!("data:{}", f.get("model").and_then(Value::as_str).unwrap_or("shared")),
        }
    }
}

thread_local! {
    /// Code spaces whose output is being worked out (a cycle guard).
    static RESOLVING: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(vec![]) };
}

/// The frame a source stands for now (`None`: no input).
pub fn frame(store: &Store, env: &CodeEnv, origin: &str, src: &Source, log: &mut dyn FnMut(&str)) -> Result<Option<Value>> {
    Ok(Some(match src {
        Source::None => return Ok(None),
        Source::Frame(f) => f.clone(),
        Source::Model(i) => host::load(store, &serde_json::to_value(i)?)?,
        Source::Search(n) => {
            let ls = store.find_live_search(n)?;
            search_frame(store, &ls.spec, &format!("search:{}", ls.name))?
        }
        Source::Journal(n) => {
            let j = store.find_journal(n)?;
            entries_frame(&format!("journal:{}", j.name), store.entries(&j.id)?.into_iter().map(|e| (j.name.clone(), e)).collect())
        }
        Source::Code(n) => {
            let c = store.find_code_space(n)?;
            if RESOLVING.with(|r| r.borrow().contains(&c.id)) {
                let chain = RESOLVING.with(|r| r.borrow().len());
                return Err(Error::invalid("pipe", format!("code space {} reads its own output ({chain} hops back): a cycle", c.name)));
            }
            RESOLVING.with(|r| r.borrow_mut().push(c.id.clone()));
            let res = (|| {
                let input = frame(store, env, origin, &Source::of(c.input.clone()), log)?;
                let info = super::run_on(store, env, origin, &c, input, &format!("{} (its default input)", c.input.text()), log)?;
                output(&c, &info)
            })();
            RESOLVING.with(|r| r.borrow_mut().retain(|x| *x != c.id));
            res?
        }
    }))
}

fn col_frame(name: &str, cols: Vec<(&str, &str, Vec<Value>)>, ids: Vec<String>, groups: Vec<Value>) -> Value {
    let fields: Vec<Value> = cols.iter().map(|(n, t, _)| json!({ "name": n, "type": t })).collect();
    let mut columns = Map::new();
    for (n, _, v) in cols {
        columns.insert(n.to_string(), Value::Array(v));
    }
    json!({ "model": name, "fields": fields, "ids": ids, "columns": columns, "groups": groups })
}

fn entries_frame(name: &str, es: Vec<(String, crate::store::Entry)>) -> Value {
    let c = |f: &dyn Fn(&(String, crate::store::Entry)) -> Value| es.iter().map(f).collect::<Vec<_>>();
    col_frame(
        name,
        vec![
            ("journal", "text", c(&|(j, _)| json!(j))),
            ("title", "text", c(&|(_, e)| json!(e.heading()))),
            ("body", "text", c(&|(_, e)| json!(e.body))),
            ("words", "integer", c(&|(_, e)| json!(e.body.split_whitespace().count()))),
            ("_created_at", "timestamp", c(&|(_, e)| json!(e.created_at))),
            ("_updated_at", "timestamp", c(&|(_, e)| json!(e.updated_at))),
        ],
        es.iter().map(|(_, e)| e.id.clone()).collect(),
        vec![],
    )
}

/// A live search's matches as a frame: records as their model's fields (several models:
/// a `model` column and every field), series as their bars (one group per series, in the
/// time range the search scopes), journal entries as journal, title, body, words and times.
pub fn search_frame(store: &Store, spec: &SearchSpec, name: &str) -> Result<Value> {
    match spec.target {
        SearchTarget::Records => {
            if let Some(m) = &spec.model {
                let mut f = host::load(
                    store,
                    &json!({ "model": m, "where": spec.conditions.iter().map(crate::rules::condition_text).collect::<Vec<_>>(), "text": spec.query }),
                )?;
                f["source"] = json!(name);
                return Ok(f);
            }
            let rs = crate::live::record_matches(store, spec)?;
            let mut names: Vec<String> = vec![];
            for (_, r) in &rs {
                for k in r.values.keys() {
                    if !names.contains(k) {
                        names.push(k.clone());
                    }
                }
            }
            let mut cols = vec![("model", "text", rs.iter().map(|(m, _)| json!(m.name)).collect())];
            for n in &names {
                cols.push((n.as_str(), "json", rs.iter().map(|(_, r)| r.values.get(n).cloned().unwrap_or(Value::Null)).collect()));
            }
            cols.push(("_created_at", "timestamp", rs.iter().map(|(_, r)| json!(r.created_at)).collect()));
            Ok(col_frame(name, cols, rs.iter().map(|(_, r)| r.id.clone()).collect(), vec![]))
        }
        SearchTarget::Journals => Ok(entries_frame(name, crate::live::entry_matches(store, spec)?.into_iter().map(|(j, e)| (j.name, e)).collect())),
        SearchTarget::Series => {
            let scope = spec.filters.scope();
            let mut bars = vec![];
            let mut groups = vec![];
            for s in crate::live::series_matches(store, spec)? {
                let f = BarFilter {
                    source: Some(s.source.clone()),
                    symbol: Some(s.symbol.clone()),
                    duration: Some(s.duration),
                    from: scope.map(|x| x.0),
                    to: scope.map(|x| x.1),
                    ..Default::default()
                };
                let got = store.query_bars(&f)?;
                if got.is_empty() {
                    continue;
                }
                groups.push(json!({ "key": [s.source, s.symbol.display(), s.duration.to_string()], "start": bars.len(), "len": got.len() }));
                bars.extend(got);
            }
            let c = |f: &dyn Fn(&crate::model::StoredBar) -> Value| bars.iter().map(f).collect::<Vec<_>>();
            let ids = bars.iter().map(|b| crate::model::BarId::compute(b.bar.ts, &b.source, &b.symbol, b.duration).hex()).collect();
            Ok(col_frame(
                name,
                vec![
                    ("source", "text", c(&|b| json!(b.source))),
                    ("symbol", "text", c(&|b| json!(b.symbol.display()))),
                    ("base", "text", c(&|b| json!(b.symbol.base))),
                    ("quote", "text", c(&|b| json!(b.symbol.quote))),
                    ("period", "text", c(&|b| json!(b.duration.to_string()))),
                    ("timestamp", "timestamp", c(&|b| json!(b.bar.ts))),
                    ("open", "real", c(&|b| json!(b.bar.open))),
                    ("high", "real", c(&|b| json!(b.bar.high))),
                    ("low", "real", c(&|b| json!(b.bar.low))),
                    ("close", "real", c(&|b| json!(b.bar.close))),
                    ("volume", "real", c(&|b| json!(b.bar.volume))),
                    ("flags", "integer", c(&|b| json!(b.bar.flags))),
                ],
                ids,
                groups,
            ))
        }
    }
}

/// A run's result as a frame for the next step: a table's columns, a chart's x and lines,
/// text as one `text` column; tiles give their first table or chart.
pub fn view_frame(v: &ResultView, name: &str) -> Value {
    match v {
        ResultView::Table { columns, rows, .. } => {
            let cols = columns
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let vals: Vec<Value> = rows.iter().map(|r| r.get(i).cloned().unwrap_or(Value::Null)).collect();
                    let t = if vals.iter().all(|v| v.is_number() || v.is_null()) { "real" } else { "text" };
                    (c.as_str(), t, vals)
                })
                .collect();
            col_frame(name, cols, (0..rows.len()).map(|i| i.to_string()).collect(), vec![])
        }
        ResultView::Chart { x, series, .. } => {
            let xn = if x.name.is_empty() { "x" } else { x.name.as_str() };
            let mut cols = vec![(xn, if x.time { "timestamp" } else { "real" }, x.values.iter().map(|v| json!(v)).collect())];
            for s in series {
                cols.push((s.name.as_str(), "real", s.values.iter().map(|v| json!(v)).collect()));
            }
            col_frame(name, cols, (0..x.values.len()).map(|i| i.to_string()).collect(), vec![])
        }
        ResultView::Text { text, .. } => {
            let lines: Vec<Value> = text.lines().map(|l| json!(l)).collect();
            col_frame(name, vec![("text", "text", lines.clone())], (0..lines.len()).map(|i| i.to_string()).collect(), vec![])
        }
        ResultView::Tiles { tiles, .. } => tiles
            .iter()
            .flat_map(|t| t.leaves())
            .find(|t| !matches!(t, ResultView::Text { .. }))
            .or_else(|| tiles.first())
            .map(|t| view_frame(t, name))
            .unwrap_or_else(|| col_frame(name, vec![], vec![], vec![])),
    }
}

/// The output of a run as a frame (an error run has none).
pub fn output(c: &CodeSpace, info: &RunInfo) -> Result<Value> {
    if let Some(e) = &info.error {
        return Err(Error::invalid("pipe", format!("code space {} failed: {e}", c.name)));
    }
    let v = super::run_view(info).ok_or_else(|| Error::invalid("pipe", format!("code space {} returned no result", c.name)))?;
    Ok(view_frame(&v, &format!("code:{}", c.name)))
}

/// Rows of a frame, kept where `keep(row)` holds.
pub fn filter_rows(f: &Value, keep: &dyn Fn(&Map<String, Value>) -> bool) -> Value {
    let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
    let n = f.get("ids").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    let rows: Vec<usize> =
        (0..n).filter(|i| keep(&cols.iter().map(|(k, v)| (k.clone(), v.get(*i).cloned().unwrap_or(Value::Null))).collect())).collect();
    let mut out = f.clone();
    out["ids"] = Value::Array(rows.iter().map(|i| f["ids"][*i].clone()).collect());
    out["columns"] = Value::Object(cols.iter().map(|(k, v)| (k.clone(), Value::Array(rows.iter().map(|i| v[*i].clone()).collect()))).collect());
    out["groups"] = json!([]);
    out
}

/// A frame filtered by a live search: its text (fuzzy, over the row's values) and the
/// conditions of a records search.
pub fn search_filter(f: &Value, spec: &SearchSpec) -> Value {
    let q = spec.query.trim().to_string();
    let conds = if spec.target == SearchTarget::Records { spec.conditions.clone() } else { vec![] };
    filter_rows(f, &|row| {
        let text = row.values().map(crate::designer::value_text).collect::<Vec<_>>().join(" ");
        (q.is_empty() || crate::tui::sidebar::fuzzy(&q, &text).is_some())
            && conds.iter().all(|c| crate::rules::compare(row.get(&c.field), c.op, &c.value))
    })
}

/// One step of a pipe.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Code(String),
    Search(String),
    /// Save the data here and pass it on unchanged (a side branch, like `tee`).
    Save(Sink),
}

impl Step {
    pub fn parse(s: &str) -> Result<Step> {
        match s.trim().split_once(':') {
            Some((k, n)) if k.eq_ignore_ascii_case("search") && !n.trim().is_empty() => Ok(Step::Search(n.trim().into())),
            Some((k, n)) if k.eq_ignore_ascii_case("code") && !n.trim().is_empty() => Ok(Step::Code(n.trim().into())),
            Some((k, _)) if ["journal", "model"].contains(&k.trim().to_ascii_lowercase().as_str()) => Ok(Step::Save(Sink::parse(s)?)),
            None if !s.trim().is_empty() => Ok(Step::Code(s.trim().into())),
            _ => Err(Error::invalid("pipe", format!("`{s}`: a step is code:SPACE (or a space name), search:NAME, journal:NAME or model:NAME"))),
        }
    }
    pub fn text(&self) -> String {
        match self {
            Step::Code(n) => format!("code:{n}"),
            Step::Search(n) => format!("search:{n}"),
            Step::Save(k) => format!("tee {}", k.text()),
        }
    }
}

/// Where a pipe's output goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Sink {
    /// A new journal entry with the result.
    Journal(String),
    /// New records of a model (columns named like its fields).
    Model(String),
}

impl Sink {
    pub fn parse(s: &str) -> Result<Sink> {
        match s.trim().split_once(':') {
            Some((k, n)) if k.eq_ignore_ascii_case("journal") && !n.trim().is_empty() => Ok(Sink::Journal(n.trim().into())),
            Some((k, n)) if k.eq_ignore_ascii_case("model") && !n.trim().is_empty() => Ok(Sink::Model(n.trim().into())),
            _ => Err(Error::invalid("pipe", format!("`{s}`: a target is journal:NAME or model:NAME"))),
        }
    }
    pub fn text(&self) -> String {
        match self {
            Sink::Journal(n) => format!("journal:{n}"),
            Sink::Model(n) => format!("model:{n}"),
        }
    }
}

/// What a pipe did.
#[derive(Debug, Clone, Default)]
pub struct Piped {
    /// Each code step's run, in order.
    pub runs: Vec<(CodeSpace, RunInfo)>,
    /// The last frame (the output of the last step).
    pub frame: Option<Value>,
    /// What each save made (`journal:Notes → entry id`, `model:x → 3 records`), in order.
    pub saved: Vec<String>,
    pub hops: Vec<String>,
}

/// Run `src | steps… > sink`. Each code run is recorded (`code.run`, with its input named),
/// and the pipe as a whole (`pipe.run`, with every hop). Code steps must be approved.
pub fn run(store: &Store, env: &CodeEnv, origin: &str, src: &Source, steps: &[Step], to: &[Sink], log: &mut dyn FnMut(&str)) -> Result<Piped> {
    let mut seen: Vec<String> = vec![];
    if let Source::Code(n) = src {
        seen.push(store.find_code_space(n)?.id);
    }
    let mut out = Piped { hops: vec![src.text()], ..Default::default() };
    let mut cur = frame(store, env, origin, src, log)?;
    let mut last: Option<(CodeSpace, RunInfo)> = None;
    for st in steps {
        out.hops.push(st.text());
        match st {
            Step::Search(n) => {
                let ls = store.find_live_search(n)?;
                let f = cur.take().ok_or_else(|| Error::invalid("pipe", format!("search:{} needs data before it", ls.name)))?;
                cur = Some(search_filter(&f, &ls.spec));
                last = None;
            }
            Step::Save(k) => {
                let f = cur.as_ref().ok_or_else(|| Error::invalid("pipe", format!("{} has no data to save", k.text())))?;
                let made = deliver(store, origin, k, f, last.as_ref(), &out.hops.join(" | "))?;
                out.saved.push(format!("{} → {made}", k.text()));
            }
            Step::Code(n) => {
                let c = store.find_code_space(n)?;
                if seen.contains(&c.id) {
                    return Err(Error::invalid("pipe", format!("code space {} is in this pipe twice: a cycle", c.name)));
                }
                seen.push(c.id.clone());
                let from = out.hops[out.hops.len() - 2].clone();
                let info = super::run_on(store, env, origin, &c, cur.take(), &from, log)?;
                cur = Some(output(&c, &info)?);
                out.runs.push((c.clone(), info.clone()));
                last = Some((c, info));
            }
        }
    }
    for k in to {
        out.hops.push(k.text());
        let f = cur.clone().ok_or_else(|| Error::invalid("pipe", "nothing to save: the pipe has no data"))?;
        let made = deliver(store, origin, k, &f, last.as_ref(), &out.hops.join(" | "))?;
        out.saved.push(format!("{} → {made}", k.text()));
    }
    out.frame = cur;
    store.note_action(
        &NewAction::new("pipe.run", origin, format!("pipe {}", out.hops.join(" | "))).params(json!({ "hops": out.hops, "saved": out.saved })),
    );
    Ok(out)
}

/// The frame as a Markdown table (at most `max` rows).
pub fn markdown(f: &Value, max: usize) -> String {
    let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
    let names: Vec<&String> = cols.keys().filter(|k| !k.starts_with('_')).collect();
    let n = f.get("ids").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    if names.is_empty() {
        return "(no columns)\n".into();
    }
    let mut s = format!("| {} |\n|{}\n", names.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(" | "), "---|".repeat(names.len()));
    for i in 0..n.min(max) {
        let cells: Vec<String> = names.iter().map(|k| crate::designer::value_text(&cols[*k][i]).replace('|', "\\|")).collect();
        s += &format!("| {} |\n", cells.join(" | "));
    }
    if n > max {
        s += &format!("\n… {} more rows\n", n - max);
    }
    s
}

/// Save a frame (a side branch of a pipe): a journal entry or new records.
pub fn save(store: &Store, origin: &str, sink: &Sink, f: &Value, last: Option<(&CodeSpace, &RunInfo)>, hops: &str) -> Result<String> {
    let last = last.map(|(c, i)| (c.clone(), i.clone()));
    deliver(store, origin, sink, f, last.as_ref(), hops)
}

fn deliver(store: &Store, origin: &str, sink: &Sink, f: &Value, last: Option<&(CodeSpace, RunInfo)>, hops: &str) -> Result<String> {
    match sink {
        Sink::Journal(n) => {
            let j = store.find_journal(n)?;
            let title = match last {
                Some((c, _)) => format!("Result: {}", c.name),
                None => format!("Data: {}", f.get("model").and_then(Value::as_str).unwrap_or("pipe")),
            };
            let mut body = format!("Piped: `{hops}`\n\n");
            if let Some(v) = last.and_then(|(_, i)| super::run_view(i))
                && !v.title().is_empty()
            {
                body += &format!("**{}**\n\n", v.title());
            }
            body += &markdown(f, 200);
            let e = store.add_entry(&j.id, &title, &body, Some(crate::store::EntryFormat::Markdown))?;
            store.note_action(
                &NewAction::new("journal.entry.create", origin, format!("pipe result into journal {}", j.name))
                    .params(json!({ "journal": j.id, "entry": e.id, "pipe": hops })),
            );
            Ok(e.id)
        }
        Sink::Model(n) => {
            let m = store.find_model(n)?;
            let fields = crate::store::records::field_names(&m);
            let cols = f.get("columns").and_then(Value::as_object).cloned().unwrap_or_default();
            let rows = f.get("ids").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
            let made = store.code_atomic(|| {
                let mut k = 0;
                for i in 0..rows {
                    let v: Map<String, Value> =
                        fields.iter().filter_map(|fl| cols.get(fl).map(|c| (fl.clone(), c[i].clone()))).filter(|(_, v)| !v.is_null()).collect();
                    store.create_record(&m, &v)?;
                    k += 1;
                }
                Ok(k)
            })?;
            store.note_action(
                &NewAction::new("record.create", origin, format!("pipe {made} records into {}", m.name))
                    .params(json!({ "model": m.id, "count": made, "pipe": hops })),
            );
            Ok(format!("{made} records"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, Duration, Symbol};

    #[test]
    fn series_searches_pipe_their_bars_and_views_become_frames() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("binance", "file", None).unwrap();
        let bars: Vec<Bar> =
            (0..4).map(|i| Bar { ts: i * 300_000, open: 1.0, high: 2.0, low: 0.5, close: i as f64, volume: 3.0, flags: 0 }).collect();
        s.upsert_bars(&src, &Symbol::new("BTC", "USDT"), Duration(300), &bars).unwrap();
        s.upsert_bars(&src, &Symbol::new("ETH", "USDT"), Duration(300), &bars[..2]).unwrap();
        let f = search_frame(&s, &SearchSpec::default(), "search:all").unwrap();
        assert_eq!(f["ids"].as_array().unwrap().len(), 6);
        assert_eq!(f["groups"].as_array().unwrap().len(), 2, "one group per series");
        assert_eq!(f["columns"]["close"][3], json!(3.0));
        assert!(f["fields"].as_array().unwrap().iter().any(|x| x["name"] == "timestamp" && x["type"] == "timestamp"));
        let btc = search_frame(&s, &SearchSpec { query: "btc".into(), ..Default::default() }, "b").unwrap();
        assert_eq!(btc["ids"].as_array().unwrap().len(), 4);
        // a filter keeps matching rows
        let kept = filter_rows(&f, &|r| r["close"].as_f64().unwrap() >= 2.0);
        assert_eq!(kept["columns"]["symbol"], json!(["BTC/USDT", "BTC/USDT"]));
        // results as frames
        let t = ResultView::Table { title: "t".into(), columns: vec!["a".into(), "b".into()], rows: vec![vec![json!(1), json!("x")]] };
        let tf = view_frame(&t, "code:t");
        assert_eq!((tf["columns"]["a"][0].clone(), tf["fields"][1]["type"].clone()), (json!(1), json!("text")));
        assert!(markdown(&tf, 10).contains("| 1 | x |"));
        // sources read back
        for t in ["search:all", "journal:Notes", "code:calc", "none", "bars where close > 1 limit 5"] {
            assert_eq!(Source::parse(t).unwrap().text(), t);
        }
        assert_eq!(Source::of(Source::parse("code:x").unwrap().input()), Source::Code("x".into()));
        assert!(Step::parse("tee:x").is_err() && Sink::parse("org:x").is_err());
        assert_eq!(Step::parse("journal:Out").unwrap(), Step::Save(Sink::Journal("Out".into())));
    }
}
