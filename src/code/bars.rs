//! Bars from code spaces. Muninn is a general research store: a code space may change any
//! bars field, including the built-in ones (`open` … `flags`, and the key fields `source`,
//! `symbol`, `period`, `timestamp`), and may create bars. Nothing is deleted or replaced:
//! changing a key field re-keys the bar, and is refused when another bar already has that
//! key. Previous values go into the history entry. Fields added with the model designer
//! are written through the store as for any model.
use crate::error::{Error, Result};
use crate::model::{BarId, Duration, Symbol};
use crate::store::{Model, Store, models::FieldType};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value, json};

/// Built-in bars fields a code space can write.
pub const FIELDS: &[&str] = &["source", "symbol", "base", "quote", "period", "timestamp", "open", "high", "low", "close", "volume", "flags"];

/// Split values into built-in bars fields and the rest (added fields).
pub fn split(values: &Map<String, Value>) -> (Map<String, Value>, Map<String, Value>) {
    let mut core = Map::new();
    let mut rest = Map::new();
    for (k, v) in values {
        if FIELDS.contains(&k.as_str()) {
            core.insert(k.clone(), v.clone());
        } else {
            rest.insert(k.clone(), v.clone());
        }
    }
    (core, rest)
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
             FROM bars b JOIN sources s ON s.id = b.source_id WHERE b.id = ?1",
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
        "INSERT INTO series_stats SELECT source_id, base, quote, duration_s, COUNT(*), MIN(timestamp), MAX(timestamp) FROM bars
         WHERE source_id = ?1 AND base = ?2 AND quote = ?3 AND duration_s = ?4 GROUP BY source_id, base, quote, duration_s",
        params![source_id, sym.base, sym.quote, dur.0],
    )?;
    Ok(())
}

fn exists(s: &Store, id: &str) -> Result<bool> {
    Ok(s.conn.query_row("SELECT COUNT(*) FROM bars WHERE id = ?1", [hex::decode(id).unwrap_or_default()], |r| r.get::<_, i64>(0))? > 0)
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
        "UPDATE bars SET id = ?1, base = ?2, quote = ?3, timestamp = ?4, open = ?5, high = ?6, low = ?7, close = ?8, volume = ?9,
         source_id = ?10, duration_s = ?11, flags = ?12 WHERE id = ?13",
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
/// `open`, `high`, `low`, `close` (and optionally `volume`, `flags`); returns its id.
pub fn create(s: &Store, core: &Map<String, Value>) -> Result<String> {
    for need in ["source", "period", "timestamp", "open", "high", "low", "close"] {
        if !core.contains_key(need) {
            return Err(Error::invalid("bar", format!("a new bar needs {need} (source, symbol, period, timestamp, open, high, low, close)")));
        }
    }
    if !core.contains_key("symbol") && !(core.contains_key("base") && core.contains_key("quote")) {
        return Err(Error::invalid("bar", "a new bar needs symbol (BASE/QUOTE) or base and quote"));
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
        "INSERT INTO bars(id, base, quote, timestamp, open, high, low, close, volume, source_id, duration_s, flags) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![hex::decode(&id).unwrap_or_default(), row.sym.base, row.sym.quote, row.ts, o[0], o[1], o[2], o[3], o[4], row.source_id, row.dur.0, row.flags],
    )?;
    restat(s, &row.source_id, &row.sym, row.dur)?;
    Ok(id)
}

/// Update one bar: built-in fields here, added fields through the store. Returns the id and
/// the built-in values before (null when none changed).
pub fn update_record(s: &Store, m: &Model, id: &str, values: &Map<String, Value>) -> Result<(String, Value)> {
    let id = id.trim().to_lowercase();
    if id.len() != 64 || !exists(s, &id)? {
        return Err(Error::invalid("bar", format!("no bar `{id}` (bars are named by their full id)")));
    }
    let (core, rest) = split(values);
    let (id, before) = if core.is_empty() { (id, Value::Null) } else { update(s, &id, &core)? };
    if !rest.is_empty() {
        s.update_ids(m, std::slice::from_ref(&id), &rest)?;
    }
    Ok((id, before))
}

/// Create a bar with added fields too.
pub fn create_record(s: &Store, m: &Model, values: &Map<String, Value>) -> Result<String> {
    let (core, rest) = split(values);
    let id = create(s, &core)?;
    if !rest.is_empty() {
        s.update_ids(m, std::slice::from_ref(&id), &rest)?;
    }
    Ok(id)
}
