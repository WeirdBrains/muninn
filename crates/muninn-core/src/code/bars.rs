//! Writing bars (code spaces, model records, rules). Muninn is a general research store:
//! any bars field may change, including the built-in ones (`open` … `flags`, and the key
//! fields `source`, `symbol`, `period`, `timestamp`) under whatever names the bars model
//! gives them, and bars may be created and deleted. Changing a key field re-keys the bar,
//! and is refused when another bar already has that key. Previous values go into the
//! history entry. Fields added with the model designer (and built-in ones given another
//! type) are written through the store as for any model.
use crate::error::{Error, Result};
use crate::model::{BarId, Duration, Symbol};
use crate::store::{Model, Store, models::FieldType, models::bar_key};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value, json};

/// The bars columns (keys of the built-in fields as shipped).
pub const FIELDS: &[&str] = &["source", "symbol", "base", "quote", "period", "timestamp", "open", "high", "low", "close", "volume", "flags"];

/// Split values by the bars model's design: fields reading the bars columns (checked like
/// any field, keyed by column) and the rest (stored fields, checked by the store).
pub fn split(m: &Model, values: &Map<String, Value>) -> Result<(Map<String, Value>, Map<String, Value>)> {
    let mut core = Map::new();
    let mut rest = Map::new();
    for (k, v) in values {
        match m.design.field(k).and_then(|f| bar_key(f).map(|key| (f, key))) {
            Some((f, key)) => {
                let v = crate::store::models::coerce(f, v.clone())?;
                if v.is_null() {
                    return Err(Error::invalid("value", format!("{k}: give a value (every bar has one)")));
                }
                core.insert(key.to_string(), v);
            }
            None => {
                rest.insert(k.clone(), v.clone());
            }
        }
    }
    Ok((core, rest))
}

/// The name the bars model gives a column (the key itself when it has no field for it).
fn name(m: &Model, key: &str) -> String {
    m.design.bar_field(key).map(|f| f.name.clone()).unwrap_or_else(|| key.to_string())
}

/// Built-in fields made unique must stay so after a write.
fn check_unique(s: &Store, m: &Model, core: &Map<String, Value>) -> Result<()> {
    for key in core.keys() {
        let Some(f) = m.design.bar_field(key).filter(|f| f.unique) else { continue };
        let e = crate::store::models::bar_sql(key);
        let dups: i64 =
            s.conn.query_row(&format!("SELECT COUNT(*) - COUNT(DISTINCT {e}) FROM bar_rows b JOIN sources s ON s.id = b.source_id"), [], |r| {
                r.get(0)
            })?;
        if dups > 0 {
            return Err(Error::invalid("value", format!("{}: another bar already has this value (it is unique)", f.name)));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct Row {
    source_id: String,
    source: String,
    sym: Symbol,
    dur: Duration,
    ts: i64,
    ohlcv: [f64; 5],
    flags: i64,
}

impl Row {
    fn id(&self) -> String {
        BarId::compute(self.ts, &self.source, &self.sym, self.dur).hex()
    }
    fn json(&self) -> Value {
        json!({
            "source": self.source, "symbol": format!("{}/{}", self.sym.base, self.sym.quote), "period": self.dur.to_string(),
            "timestamp": self.ts, "open": self.ohlcv[0], "high": self.ohlcv[1], "low": self.ohlcv[2], "close": self.ohlcv[3],
            "volume": self.ohlcv[4], "flags": self.flags,
        })
    }
}

fn read(s: &Store, id: &str) -> Result<Row> {
    let key = hex::decode(id).map_err(|_| Error::invalid("bar", format!("`{id}` is not a bar id")))?;
    s.conn
        .query_row(
            "SELECT b.source_id, s.name, b.base, b.quote, b.duration_s, b.timestamp, b.open, b.high, b.low, b.close, b.volume, b.flags
             FROM bar_rows b JOIN sources s ON s.id = b.source_id WHERE b.id = ?1",
            [key],
            |r| {
                Ok(Row {
                    source_id: r.get(0)?,
                    source: r.get(1)?,
                    sym: Symbol { base: r.get(2)?, quote: r.get(3)? },
                    dur: Duration(r.get(4)?),
                    ts: r.get(5)?,
                    ohlcv: [r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?],
                    flags: r.get(11)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| Error::invalid("bar", format!("no bar `{id}`")))
}

fn num(k: &str, v: &Value) -> Result<f64> {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    };
    n.filter(|x| x.is_finite()).ok_or_else(|| Error::invalid("value", format!("{k}: `{v}` is not a number")))
}

fn text(k: &str, v: &Value) -> Result<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(Error::invalid("value", format!("{k}: give a value"))),
    }
}

/// Apply built-in field values to a row (a new source is created, kind `code`).
fn apply(s: &Store, row: &mut Row, core: &Map<String, Value>) -> Result<()> {
    for (k, v) in core {
        match k.as_str() {
            "open" => row.ohlcv[0] = num(k, v)?,
            "high" => row.ohlcv[1] = num(k, v)?,
            "low" => row.ohlcv[2] = num(k, v)?,
            "close" => row.ohlcv[3] = num(k, v)?,
            "volume" => row.ohlcv[4] = num(k, v)?,
            "flags" => row.flags = num(k, v)? as i64,
            "timestamp" => {
                row.ts = match v {
                    Value::String(t) => {
                        crate::ingest::time::parse_ts(t).ok_or_else(|| Error::invalid("value", format!("timestamp: `{t}` is not a time")))?
                    }
                    _ => num(k, v)? as i64,
                }
            }
            "period" => {
                row.dur = match v {
                    Value::Number(_) => Duration(num(k, v)? as i64).max_one()?,
                    _ => Duration::parse(&text(k, v)?)?,
                }
            }
            "symbol" => row.sym = Symbol::parse(&text(k, v)?)?,
            "base" => row.sym.base = text(k, v)?.to_ascii_uppercase(),
            "quote" => row.sym.quote = text(k, v)?.to_ascii_uppercase(),
            "source" => {
                let name = text(k, v)?;
                let src = match s.source_by_name(&name)? {
                    Some(x) => x,
                    None => s.add_source(&name, "code", None)?,
                };
                row.source_id = src.id;
                row.source = src.name;
            }
            _ => {}
        }
    }
    Ok(())
}

trait MaxOne: Sized {
    fn max_one(self) -> Result<Self>;
}
impl MaxOne for Duration {
    fn max_one(self) -> Result<Self> {
        if self.0 > 0 { Ok(self) } else { Err(Error::invalid("value", "period: give a positive number of seconds or e.g. 5m")) }
    }
}

fn restat(s: &Store, source_id: &str, sym: &Symbol, dur: Duration) -> Result<()> {
    s.conn.execute(
        "DELETE FROM series_stats WHERE source_id = ?1 AND base = ?2 AND quote = ?3 AND duration_s = ?4",
        params![source_id, sym.base, sym.quote, dur.0],
    )?;
    s.conn.execute(
        "INSERT INTO series_stats SELECT source_id, base, quote, duration_s, COUNT(*), MIN(timestamp), MAX(timestamp) FROM bar_rows
         WHERE source_id = ?1 AND base = ?2 AND quote = ?3 AND duration_s = ?4 GROUP BY source_id, base, quote, duration_s",
        params![source_id, sym.base, sym.quote, dur.0],
    )?;
    Ok(())
}

fn exists(s: &Store, id: &str) -> Result<bool> {
    Ok(s.conn.query_row("SELECT COUNT(*) FROM m_bars WHERE id = ?1", [hex::decode(id).unwrap_or_default()], |r| r.get::<_, i64>(0))? > 0)
}

fn known_symbol(s: &Store, sym: &Symbol) -> Result<()> {
    s.conn.execute("INSERT OR IGNORE INTO symbols(base, quote) VALUES (?1, ?2)", params![sym.base, sym.quote])?;
    s.conn.execute("INSERT OR IGNORE INTO assets(name, is_quote) VALUES (?1, 0)", [&sym.base])?;
    s.conn.execute("INSERT INTO assets(name, is_quote) VALUES (?1, 1) ON CONFLICT(name) DO UPDATE SET is_quote = 1", [&sym.quote])?;
    Ok(())
}

/// Change built-in fields of one bar; returns (its id afterwards, the values before).
pub fn update(s: &Store, id: &str, core: &Map<String, Value>) -> Result<(String, Value)> {
    let old = read(s, id)?;
    let mut row = old.clone();
    apply(s, &mut row, core)?;
    let new_id = row.id();
    if new_id != id && exists(s, &new_id)? {
        return Err(Error::invalid(
            "bar",
            format!("changing {id} to {} {} {} at {} would replace bar {new_id}; nothing was changed", row.source, row.sym, row.dur, row.ts),
        ));
    }
    if row == old {
        return Ok((new_id, old.json()));
    }
    let o = row.ohlcv;
    s.conn.execute(
        "UPDATE m_bars SET id = ?1, _base = ?2, _quote = ?3, _timestamp = ?4, _open = ?5, _high = ?6, _low = ?7, _close = ?8, _volume = ?9,
         _source_id = ?10, _duration_s = ?11, _flags = ?12 WHERE id = ?13",
        params![
            hex::decode(&new_id).unwrap_or_default(),
            row.sym.base,
            row.sym.quote,
            row.ts,
            o[0],
            o[1],
            o[2],
            o[3],
            o[4],
            row.source_id,
            row.dur.0,
            row.flags,
            hex::decode(id).unwrap_or_default()
        ],
    )?;
    if new_id != id {
        // the added-fields table follows by `ON UPDATE CASCADE`; refs in other models are text ids
        known_symbol(s, &row.sym)?;
        for m in s.models()? {
            if m.version == 0 {
                continue;
            }
            for f in m.design.fields.iter().filter(|f| f.ty == FieldType::Ref(crate::store::models::BARS.into())) {
                let t = crate::store::models::q(&m.table());
                if s.conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [m.table()], |r| r.get::<_, i64>(0))? > 0
                {
                    s.conn.execute(
                        &format!("UPDATE {t} SET {} = ?1 WHERE {} = ?2", crate::store::models::q(&f.name), crate::store::models::q(&f.name)),
                        params![new_id, id],
                    )?;
                }
            }
        }
        restat(s, &old.source_id, &old.sym, old.dur)?;
        restat(s, &row.source_id, &row.sym, row.dur)?;
    }
    Ok((new_id, old.json()))
}

/// Create a bar from `source`, `symbol` (or `base` and `quote`), `period`, `timestamp`,
/// `open`, `high`, `low`, `close` (and optionally `volume`, `flags`), keyed by column;
/// returns its id. `m` names the columns in errors.
pub fn create(s: &Store, m: &Model, core: &Map<String, Value>) -> Result<String> {
    let need_text = |key: &str| match m.design.bar_field(key) {
        Some(f) => format!("a new bar needs {} (the bars' {key})", f.name),
        None => format!(
            "a new bar needs its {key}, and {} has no field for it (it was dropped or given another type); roll back to a version with it (`mnnn model history {}`)",
            m.name, m.name
        ),
    };
    for need in ["source", "period", "timestamp", "open", "high", "low", "close"] {
        if !core.contains_key(need) {
            return Err(Error::invalid("bar", need_text(need)));
        }
    }
    if !core.contains_key("symbol") && !(core.contains_key("base") && core.contains_key("quote")) {
        return Err(Error::invalid("bar", format!("{} (BASE/QUOTE), or {} and {}", need_text("symbol"), name(m, "base"), name(m, "quote"))));
    }
    let mut row =
        Row { source_id: String::new(), source: String::new(), sym: Symbol::new("", ""), dur: Duration(60), ts: 0, ohlcv: [0.0; 5], flags: 0 };
    apply(s, &mut row, core)?;
    let id = row.id();
    if exists(s, &id)? {
        return Err(Error::invalid(
            "bar",
            format!("bar {id} already exists ({} {} {} at {}); update it instead", row.source, row.sym, row.dur, row.ts),
        ));
    }
    known_symbol(s, &row.sym)?;
    let o = row.ohlcv;
    s.conn.execute(
        "INSERT INTO m_bars(id, _base, _quote, _timestamp, _open, _high, _low, _close, _volume, _source_id, _duration_s, _flags) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![hex::decode(&id).unwrap_or_default(), row.sym.base, row.sym.quote, row.ts, o[0], o[1], o[2], o[3], o[4], row.source_id, row.dur.0, row.flags],
    )?;
    restat(s, &row.source_id, &row.sym, row.dur)?;
    Ok(id)
}

/// Rows arriving at a pipeline's bars leaf (`model:bars`), e.g. from `to-bars`: each row
/// gives `source`, `symbol` (or `base` and `quote`), `period`, `timestamp` (epoch ms or a
/// time), `open`, `high`, `low`, `close` and optionally `volume` and `flags`. Stored by
/// bar id, so running the pipeline again adds only bars not there yet. Returns (new,
/// already there). A row missing a value is an error naming the row.
pub fn upsert_frame(s: &Store, cols: &Map<String, Value>, rows: usize) -> Result<(u64, u64)> {
    let keys = ["source", "symbol", "base", "quote", "period", "timestamp", "open", "high", "low", "close", "volume", "flags"];
    for need in ["source", "period", "timestamp", "open", "high", "low", "close"] {
        if !cols.contains_key(need) {
            return Err(Error::invalid("bars", format!("rows for bars need a `{need}` column (to-bars makes them: `code:to-bars?symbol=BTC/USDT`)")));
        }
    }
    if !cols.contains_key("symbol") && !(cols.contains_key("base") && cols.contains_key("quote")) {
        return Err(Error::invalid("bars", "rows for bars need a `symbol` column (or `base` and `quote`): give to-bars `symbol=BASE/QUOTE`"));
    }
    let mut series: std::collections::BTreeSet<(String, String, String, i64)> = Default::default();
    let (mut new, mut dup) = (0u64, 0u64);
    let mut st = s.conn.prepare_cached(
        "INSERT OR IGNORE INTO m_bars(id, _base, _quote, _timestamp, _open, _high, _low, _close, _volume, _source_id, _duration_s, _flags) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    let mut sources: std::collections::HashMap<String, (String, String)> = Default::default();
    for i in 0..rows {
        let mut core = Map::new();
        for k in keys {
            if let Some(v) = cols.get(k).and_then(|c| c.get(i)).filter(|v| !v.is_null()) {
                core.insert(k.to_string(), v.clone());
            }
        }
        let mut row =
            Row { source_id: String::new(), source: String::new(), sym: Symbol::new("", ""), dur: Duration(60), ts: 0, ohlcv: [0.0; 5], flags: 0 };
        // the source once per name (apply would look it up for every row)
        let src_name = core.remove("source").map(|v| match v {
            Value::String(t) => t,
            o => o.to_string(),
        });
        apply(s, &mut row, &core).map_err(|e| Error::invalid("bars", format!("row {}: {e}", i + 1)))?;
        let Some(src_name) = src_name else { return Err(Error::invalid("bars", format!("row {}: no source", i + 1))) };
        let (sid, sname) = match sources.get(&src_name) {
            Some(x) => x.clone(),
            None => {
                let src = match s.source_by_name(&src_name)? {
                    Some(x) => x,
                    None => s.add_source(&src_name, "code", None)?,
                };
                let x = (src.id, src.name);
                sources.insert(src_name.clone(), x.clone());
                x
            }
        };
        row.source_id = sid;
        row.source = sname;
        if row.sym.base.is_empty() || row.sym.quote.is_empty() {
            return Err(Error::invalid("bars", format!("row {}: no symbol", i + 1)));
        }
        let key = (row.source_id.clone(), row.sym.base.clone(), row.sym.quote.clone(), row.dur.0);
        if series.insert(key) {
            known_symbol(s, &row.sym)?;
        }
        let o = row.ohlcv;
        let id = row.id();
        let n = st.execute(params![
            hex::decode(&id).unwrap_or_default(),
            row.sym.base,
            row.sym.quote,
            row.ts,
            o[0],
            o[1],
            o[2],
            o[3],
            o[4],
            row.source_id,
            row.dur.0,
            row.flags
        ])?;
        if n > 0 { new += 1 } else { dup += 1 }
    }
    drop(st);
    for (sid, base, quote, dur) in &series {
        restat(s, sid, &Symbol { base: base.clone(), quote: quote.clone() }, Duration(*dur))?;
    }
    Ok((new, dup))
}

/// Update one bar: built-in fields here, stored fields through the store. Returns the id and
/// the built-in values before (null when none changed).
pub fn update_record(s: &Store, m: &Model, id: &str, values: &Map<String, Value>) -> Result<(String, Value)> {
    let id = id.trim().to_lowercase();
    if id.len() != 64 || !exists(s, &id)? {
        return Err(Error::invalid("bar", format!("no bar `{id}` (bars are named by their full id)")));
    }
    let (core, rest) = split(m, values)?;
    let (id, before) = if core.is_empty() {
        (id, Value::Null)
    } else {
        let r = update(s, &id, &core)?;
        check_unique(s, m, &core)?;
        r
    };
    if !rest.is_empty() {
        s.update_stored(m, std::slice::from_ref(&id), &rest)?;
    }
    Ok((id, before))
}

/// Create a bar with stored fields too. Built-in fields left out get their defaults.
pub fn create_record(s: &Store, m: &Model, values: &Map<String, Value>) -> Result<String> {
    let (mut core, rest) = split(m, values)?;
    for f in &m.design.fields {
        if let (Some(key), Some(d)) = (bar_key(f), &f.default)
            && !core.contains_key(key)
        {
            core.insert(key.to_string(), crate::store::models::coerce(f, d.clone())?);
        }
    }
    s.atomic("bar_create", || -> Result<String> {
        let id = create(s, m, &core)?;
        check_unique(s, m, &core)?;
        if !rest.is_empty() {
            s.update_stored(m, std::slice::from_ref(&id), &rest)?;
        }
        Ok(id)
    })
}

/// Delete bars by id (their stored fields go with them); returns how many.
pub fn delete(s: &Store, ids: &[String]) -> Result<usize> {
    let mut series = std::collections::BTreeSet::new();
    let mut n = 0;
    for id in ids {
        let Ok(row) = read(s, id) else { continue };
        n += s.conn.execute("DELETE FROM m_bars WHERE id = ?1", [hex::decode(id).unwrap_or_default()])?;
        series.insert((row.source_id, row.sym.base, row.sym.quote, row.dur.0));
    }
    for (src, base, quote, d) in series {
        restat(s, &src, &Symbol { base, quote }, Duration(d))?;
    }
    Ok(n)
}

/// Bars straight from an OHLCV CSV into `m_bars`, the way `source:… > code:parse-csv >
/// code:to-bars?symbol=…&duration=… > model:bars` would store them, without compiling
/// the parser code spaces. For tests and fixtures only: `mnnn ingest` never makes bars.
#[doc(hidden)]
pub fn load_csv_bars(s: &mut Store, path: &std::path::Path, source: &str, symbol: &str, duration: &str) -> Result<(u64, u64)> {
    let df = crate::ingest::csv::read_csv(path, &Default::default())?;
    let names = crate::ingest::frame::map_columns(&df, &Default::default())?;
    let ts = crate::ingest::frame::timestamps(&df, &names[0])?;
    let vals: Vec<Vec<Option<f64>>> = names[1..].iter().map(|c| crate::ingest::frame::floats(&df, c)).collect::<Result<_>>()?;
    let period = Duration::parse(duration)?.0;
    let keep: Vec<usize> = (0..ts.len()).filter(|&i| ts[i].is_some() && vals[..4].iter().all(|v| v[i].is_some())).collect();
    let mut cols = Map::new();
    let n = keep.len();
    cols.insert("source".into(), Value::Array(vec![json!(source); n]));
    cols.insert("symbol".into(), Value::Array(vec![json!(symbol); n]));
    cols.insert("period".into(), Value::Array(vec![json!(period); n]));
    cols.insert("timestamp".into(), keep.iter().map(|&i| json!(ts[i])).collect());
    for (k, v) in ["open", "high", "low", "close", "volume"].iter().zip(&vals) {
        cols.insert(k.to_string(), keep.iter().map(|&i| json!(v[i].unwrap_or(0.0))).collect());
    }
    cols.insert("flags".into(), keep.iter().map(|&i| json!(i32::from(vals[4][i].unwrap_or(0.0) == 0.0))).collect());
    let tx = s.conn.unchecked_transaction()?;
    let r = upsert_frame(s, &cols, n)?;
    tx.commit()?;
    Ok(r)
}
