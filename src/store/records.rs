//! Records of designed models: create, update by id or by filter, delete, and load with
//! filters, grouping and sorting. The CLI, the TUI, rules, live searches, reports and
//! (later) code spaces all come through here. See `docs/models.md`.
//!
//! Filters are rule conditions (`price > 3`, `side = buy`, `note ~ halving`) with the same
//! meaning as in rules; timestamp fields take dates (`opened >= 2024-01-01`). Besides the
//! fields, `id`, `_version`, `_created_at`, `_updated_at` and `author` can be filtered and
//! sorted on.
//!
//! The built-in `bars` model lists the bars with the fields its design gives them: the
//! built-in ones (source, symbol, period, timestamp, prices, … as renamed, retyped or
//! dropped) and the added ones. Bars are created, changed (a key field re-keys the bar)
//! and deleted here like any record.
use super::models::{Field, FieldType, Model, bar_key, bar_sql, bar_value, coerce, from_sql, q, to_sql};
use super::{Condition, Op, Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::params_from_iter;
use rusqlite::types::Value as Sql;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cmp::Ordering;

/// A row as read: id, version, created, updated-by, values.
type RawRecord = (i64, i64, i64, Option<String>, Map<String, Value>);

/// One record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    /// The design version the record was last written or migrated under.
    #[serde(rename = "_version")]
    pub version: i64,
    #[serde(rename = "_created_at")]
    pub created_at: i64,
    #[serde(rename = "_updated_at")]
    pub updated_at: i64,
    #[serde(rename = "_author")]
    pub author_id: Option<String>,
    #[serde(flatten)]
    pub values: Map<String, Value>,
}

impl Record {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// A field or `id`, `_version`, `_created_at`, `_updated_at`, `_author`.
    pub fn get(&self, field: &str) -> Option<Value> {
        let v = match field {
            "id" => Value::from(self.id.clone()),
            "_version" => Value::from(self.version),
            "_created_at" => Value::from(self.created_at),
            "_updated_at" => Value::from(self.updated_at),
            "_author" => self.author_id.clone().map(Value::from).unwrap_or(Value::Null),
            f => self.values.get(f).cloned().unwrap_or(Value::Null),
        };
        (!v.is_null()).then_some(v)
    }
}

/// What to load.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordQuery {
    /// All must hold.
    #[serde(default, rename = "where")]
    pub filter: Vec<Condition>,
    /// Fuzzy text over the text fields (like the sidebar search).
    #[serde(default)]
    pub text: String,
    /// (field, descending), first is the main order.
    #[serde(default)]
    pub sort: Vec<(String, bool)>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
}

/// Records sharing the values of the group fields.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub key: Map<String, Value>,
    pub count: usize,
    /// `sum:price` → value, …
    pub aggs: Map<String, Value>,
    pub records: Vec<Record>,
}

/// Fields every record has (for filters, sorting and listings).
pub const META_FIELDS: &[&str] = &["id", "_version", "_created_at", "_updated_at", "author"];

/// Field names of a model, in order.
pub fn field_names(m: &Model) -> Vec<String> {
    m.design.fields.iter().map(|f| f.name.clone()).collect()
}

/// The bars column a bars condition or sort names: a field reading one, or `bar.KEY` (the
/// value as imported, whatever the model calls or keeps of it).
fn bar_column(m: &Model, name: &str) -> Option<&'static str> {
    if !m.is_bars() {
        return None;
    }
    if let Some(k) = name.strip_prefix(super::models::BAR_ID_PREFIX) {
        return super::models::BAR_FIELDS.iter().find(|(n, _)| *n == k).map(|(n, _)| *n);
    }
    m.design.field(name).and_then(bar_key)
}

/// The type of a field (meta fields too).
pub fn field_type(m: &Model, name: &str) -> Option<FieldType> {
    if let Some(f) = m.design.field(name) {
        return Some(f.ty.clone());
    }
    if let Some(k) = bar_column(m, name) {
        return super::models::BAR_FIELDS.iter().find(|(n, _)| *n == k).and_then(|(_, t)| FieldType::basic(t));
    }
    match name {
        "_version" => Some(FieldType::Integer),
        "_created_at" | "_updated_at" => Some(FieldType::Timestamp),
        "id" | "author" | "_author" => Some(FieldType::Text),
        _ => None,
    }
}

fn cmp_values(a: Option<&Value>, b: Option<&Value>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(p), Some(q)) => p.partial_cmp(&q).unwrap_or(Ordering::Equal),
            _ => {
                let s = |v: &Value| match v {
                    Value::String(s) => s.to_lowercase(),
                    o => o.to_string(),
                };
                s(x).cmp(&s(y))
            }
        },
    }
}

fn friendly(e: rusqlite::Error, m: &Model) -> Error {
    let s = e.to_string();
    if let Some(rest) = s.split("UNIQUE constraint failed: ").nth(1) {
        let field = rest.rsplit('.').next().unwrap_or(rest);
        return Error::invalid("value", format!("{field}: another {} record already has this value (it is unique)", m.name));
    }
    if let Some(rest) = s.split("NOT NULL constraint failed: ").nth(1) {
        let field = rest.rsplit('.').next().unwrap_or(rest);
        return Error::invalid("value", format!("{field} is required"));
    }
    Error::Sqlite(e)
}

impl Store {
    fn need_table(&self, m: &Model) -> Result<()> {
        if m.is_bars() {
            return Ok(());
        }
        if m.version == 0 {
            return Err(Error::invalid(
                "model",
                if m.is_bars() {
                    "no fields have been added to bars yet (`mnnn model edit bars --add NAME:TYPE --apply`)".to_string()
                } else {
                    format!("{} is not applied yet (`mnnn model apply {}`)", m.name, m.name)
                },
            ));
        }
        Ok(())
    }

    /// Records in the model now.
    pub fn count_records(&self, m: &Model) -> Result<usize> {
        if m.is_bars() {
            return Ok(self.conn.query_row("SELECT COALESCE(SUM(count), 0) FROM series_stats", [], |r| r.get::<_, i64>(0))? as usize);
        }
        if m.version == 0 || !self.table_exists(&m.table())? {
            return Ok(0);
        }
        Ok(self.conn.query_row(&format!("SELECT COUNT(*) FROM {}", q(&m.table())), [], |r| r.get::<_, i64>(0))? as usize)
    }

    fn read_record(m: &Model, r: &rusqlite::Row, at: usize) -> rusqlite::Result<RawRecord> {
        let mut values = Map::new();
        for (i, f) in m.design.stored().enumerate() {
            values.insert(f.name.clone(), from_sql(r.get(at + 4 + i)?, &f.ty));
        }
        Ok((
            r.get::<_, Option<i64>>(at)?.unwrap_or(m.version),
            r.get::<_, Option<i64>>(at + 1)?.unwrap_or(0),
            r.get::<_, Option<i64>>(at + 2)?.unwrap_or(0),
            r.get(at + 3)?,
            values,
        ))
    }

    fn ext_cols(m: &Model, alias: &str) -> String {
        let mut c = vec![format!("{alias}._version"), format!("{alias}._created_at"), format!("{alias}._updated_at"), format!("{alias}._author")];
        c.extend(m.design.stored().map(|f| format!("{alias}.{}", q(&f.name))));
        c.join(", ")
    }

    /// Every record of a custom model, oldest first.
    fn all_records(&self, m: &Model) -> Result<Vec<Record>> {
        if m.version == 0 || !self.table_exists(&m.table())? {
            return Ok(vec![]);
        }
        let mut st = self.conn.prepare(&format!("SELECT r.id, {} FROM {} r ORDER BY r._created_at, r.id", Self::ext_cols(m, "r"), q(&m.table())))?;
        let v = st
            .query_map([], |r| {
                let (version, created_at, updated_at, author_id, values) = Self::read_record(m, r, 1)?;
                Ok(Record { id: r.get(0)?, version, created_at, updated_at, author_id, values })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Bars as records, with conditions on their own columns done in SQL.
    fn bar_records(&self, m: &Model, qy: &RecordQuery, all_sql: bool) -> Result<Vec<Record>> {
        let mut wh: Vec<String> = vec![];
        let mut args: Vec<Sql> = vec![];
        for c in &qy.filter {
            if let Some((sql, arg)) = bar_condition(m, c) {
                wh.push(sql);
                args.push(arg);
            }
        }
        let ext = m.design.stored().next().is_some() && self.table_exists(&m.table())?;
        let join = if ext { format!(" LEFT JOIN {} x ON x.id = b.id", q(&m.table())) } else { String::new() };
        let cols = if ext { format!(", {}", Self::ext_cols(m, "x")) } else { String::new() };
        let mut sql = format!(
            "SELECT b.id, s.name, b.base, b.quote, b.duration_s, b.timestamp, b.open, b.high, b.low, b.close, b.volume, b.flags{cols} FROM bars b JOIN sources s ON s.id = b.source_id{join}"
        );
        if !wh.is_empty() {
            sql.push_str(&format!(" WHERE {}", wh.join(" AND ")));
        }
        let order: Vec<String> =
            qy.sort.iter().filter_map(|(f, d)| bar_column(m, f).map(|k| format!("{}{}", bar_sql(k), if *d { " DESC" } else { "" }))).collect();
        if all_sql && order.len() == qy.sort.len() {
            let mut o = order;
            o.extend(["b.timestamp", "s.name", "b.base", "b.quote", "b.duration_s"].map(String::from));
            sql.push_str(&format!(" ORDER BY {}", o.join(", ")));
            if let Some(l) = qy.limit {
                sql.push_str(&format!(" LIMIT {l} OFFSET {}", qy.offset));
            }
        }
        let mut st = self.conn.prepare(&sql)?;
        let v = st
            .query_map(params_from_iter(args), |r| {
                let mut phys = Map::new();
                let (base, quote): (String, String) = (r.get(2)?, r.get(3)?);
                phys.insert("source".into(), Value::String(r.get(1)?));
                phys.insert("symbol".into(), Value::String(format!("{base}/{quote}")));
                phys.insert("base".into(), Value::String(base));
                phys.insert("quote".into(), Value::String(quote));
                phys.insert("period".into(), bar_value("period", r.get(4)?));
                for (i, k) in ["timestamp", "open", "high", "low", "close", "volume", "flags"].iter().enumerate() {
                    phys.insert(k.to_string(), bar_value(k, r.get(5 + i)?));
                }
                let (mut version, mut created_at, mut updated_at, mut author_id) = (m.version, 0, 0, None);
                let mut more = Map::new();
                if ext {
                    let (v, c, u, a, stored) = Self::read_record(m, r, 12)?;
                    (version, created_at, updated_at, author_id) = (v, c, u, a);
                    more = stored;
                }
                // in the design's order and names: built-in fields read the bars, the rest mx_bars
                let mut values = Map::new();
                for f in &m.design.fields {
                    let v = match bar_key(f) {
                        Some(k) => phys.get(k).cloned().unwrap_or(Value::Null),
                        None => match more.remove(&f.name) {
                            Some(v) if !v.is_null() => v,
                            _ => f.default.clone().unwrap_or(Value::Null),
                        },
                    };
                    values.insert(f.name.clone(), v);
                }
                Ok(Record { id: hex::encode(r.get::<_, Vec<u8>>(0)?), version, created_at, updated_at, author_id, values })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Does a record satisfy the conditions? (Timestamps take dates, `author` is a name.)
    pub fn record_matches(&self, m: &Model, r: &Record, conds: &[Condition]) -> bool {
        conds.iter().all(|c| {
            let actual = if c.field == "author" && m.design.field("author").is_none() {
                Some(Value::from(crate::history::author_label(self, r.author_id.as_deref())))
            } else {
                r.get(&c.field)
            };
            let want = match field_type(m, &c.field) {
                Some(FieldType::Timestamp) if !c.op.unary() && c.value.trim().parse::<i64>().is_err() => {
                    crate::ingest::time::parse_ts(&c.value).map(|t| t.to_string()).unwrap_or_else(|| c.value.clone())
                }
                _ => c.value.clone(),
            };
            crate::rules::compare(actual.as_ref(), c.op, &want)
        })
    }

    /// Check filter and sort fields against the model.
    fn check_query(&self, m: &Model, qy: &RecordQuery) -> Result<()> {
        let names = field_names(m);
        for f in qy.filter.iter().map(|c| &c.field).chain(qy.sort.iter().map(|(f, _)| f)) {
            if !names.contains(f) && !META_FIELDS.contains(&f.as_str()) && f != "_author" && bar_column(m, f).is_none() {
                return Err(Error::invalid("field", format!("{} has no field `{f}`{} (fields: {})", m.name, bar_hint(m, f), names.join(", "))));
            }
        }
        Ok(())
    }

    /// Load records: filter, fuzzy text, sort, then offset and limit.
    pub fn records(&self, m: &Model, qy: &RecordQuery) -> Result<Vec<Record>> {
        self.check_query(m, qy)?;
        let rest: Vec<Condition> =
            if m.is_bars() { qy.filter.iter().filter(|c| bar_condition(m, c).is_none()).cloned().collect() } else { qy.filter.clone() };
        let all_sql = m.is_bars() && rest.is_empty() && qy.text.trim().is_empty();
        let mut v = if m.is_bars() { self.bar_records(m, qy, all_sql)? } else { self.all_records(m)? };
        if all_sql && qy.sort.iter().all(|(f, _)| bar_column(m, f).is_some()) {
            return Ok(v);
        }
        v.retain(|r| self.record_matches(m, r, &rest));
        if !qy.text.trim().is_empty() {
            let mut scored: Vec<(i64, Record)> = v
                .into_iter()
                .filter_map(|r| {
                    let hay: Vec<String> = r.values.values().filter_map(|x| x.as_str().map(str::to_string)).collect();
                    crate::tui::sidebar::fuzzy(&qy.text, &hay.join(" ")).map(|s| (s, r))
                })
                .collect();
            if qy.sort.is_empty() {
                scored.sort_by_key(|x| std::cmp::Reverse(x.0));
            }
            v = scored.into_iter().map(|x| x.1).collect();
        }
        if !qy.sort.is_empty() {
            v.sort_by(|a, b| {
                for (f, desc) in &qy.sort {
                    let o = cmp_values(a.get(f).as_ref(), b.get(f).as_ref());
                    let o = if *desc { o.reverse() } else { o };
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                Ordering::Equal
            });
        }
        Ok(v.into_iter().skip(qy.offset).take(qy.limit.unwrap_or(usize::MAX)).collect())
    }

    /// Load records in groups of equal values of `by` (ordered by those values), with
    /// `count` and aggregates like `sum:price`, `avg:price`, `min:price`, `max:price`.
    pub fn group_records(&self, m: &Model, qy: &RecordQuery, by: &[String], aggs: &[String]) -> Result<Vec<Group>> {
        let names = field_names(m);
        for f in by {
            if !names.contains(f) && !META_FIELDS.contains(&f.as_str()) {
                return Err(Error::invalid("--group", format!("{} has no field `{f}`", m.name)));
            }
        }
        let mut parsed = vec![];
        for a in aggs {
            let (k, f) =
                a.split_once(':').ok_or_else(|| Error::invalid("--agg", format!("`{a}`: use sum:FIELD, avg:FIELD, min:FIELD or max:FIELD")))?;
            if !["sum", "avg", "min", "max"].contains(&k) || !names.contains(&f.to_string()) {
                return Err(Error::invalid("--agg", format!("`{a}`: use sum, avg, min or max of a field of {}", m.name)));
            }
            parsed.push((a.clone(), k.to_string(), f.to_string()));
        }
        let all = self.records(m, &RecordQuery { limit: None, offset: 0, ..qy.clone() })?;
        let mut groups: Vec<Group> = vec![];
        for r in all {
            let key: Map<String, Value> = by.iter().map(|f| (f.clone(), r.get(f).unwrap_or(Value::Null))).collect();
            match groups.iter_mut().find(|g| g.key == key) {
                Some(g) => g.records.push(r),
                None => groups.push(Group { key, count: 0, aggs: Map::new(), records: vec![r] }),
            }
        }
        groups.sort_by(|a, b| {
            for f in by {
                let o = cmp_values(a.key.get(f).filter(|v| !v.is_null()), b.key.get(f).filter(|v| !v.is_null()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        });
        for g in &mut groups {
            g.count = g.records.len();
            for (name, k, f) in &parsed {
                let xs: Vec<f64> = g.records.iter().filter_map(|r| r.get(f).and_then(|v| v.as_f64())).collect();
                let v = if xs.is_empty() {
                    Value::Null
                } else {
                    match k.as_str() {
                        "sum" => Value::from(xs.iter().sum::<f64>()),
                        "avg" => Value::from(xs.iter().sum::<f64>() / xs.len() as f64),
                        "min" => Value::from(xs.iter().cloned().fold(f64::INFINITY, f64::min)),
                        _ => Value::from(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)),
                    }
                };
                g.aggs.insert(name.clone(), v);
            }
        }
        Ok(groups.into_iter().skip(qy.offset).take(qy.limit.unwrap_or(usize::MAX)).collect())
    }

    /// One record by id or unique id prefix.
    pub fn find_record(&self, m: &Model, key: &str) -> Result<Record> {
        let k = key.trim().to_lowercase();
        if k.is_empty() {
            return Err(Error::invalid("record", "give a record id"));
        }
        let found: Vec<Record> = if m.is_bars() {
            if k.len() != 64 {
                return Err(Error::invalid("record", "a bar is named by its full id (64 hex characters)"));
            }
            self.bar_records(m, &RecordQuery::default(), false)?.into_iter().filter(|r| r.id == k).collect()
        } else {
            let all = self.all_records(m)?;
            match all.iter().find(|r| r.id == k) {
                Some(r) => vec![r.clone()],
                None => all.into_iter().filter(|r| r.id.starts_with(&k)).collect(),
            }
        };
        match found.len() {
            1 => Ok(found.into_iter().next().unwrap()),
            0 => Err(Error::invalid("record", format!("`{key}` not found in {}", m.name))),
            _ => Err(Error::invalid("record", format!("`{key}` matches more than one record; use more of the id"))),
        }
    }

    /// Does a record exist (for refs)?
    pub fn record_exists(&self, model_id: &str, id: &str) -> Result<bool> {
        if model_id == super::models::BARS {
            let Ok(b) = hex::decode(id) else { return Ok(false) };
            return Ok(self.conn.query_row("SELECT COUNT(*) FROM bars WHERE id = ?1", [b], |r| r.get::<_, i64>(0))? > 0);
        }
        let Some(m) = self.model(model_id)? else { return Ok(false) };
        if m.version == 0 || !self.table_exists(&m.table())? {
            return Ok(false);
        }
        Ok(self.conn.query_row(&format!("SELECT COUNT(*) FROM {} WHERE id = ?1", q(&m.table())), [id], |r| r.get::<_, i64>(0))? > 0)
    }

    /// Values for the fields given: known fields only, converted, refs checked.
    fn values_for(&self, m: &Model, values: &Map<String, Value>) -> Result<Vec<(Field, Value)>> {
        let mut out = vec![];
        for (k, v) in values {
            let Some(f) = m.design.field(k).filter(|f| bar_key(f).is_none()) else {
                let names: Vec<&str> = m.design.fields.iter().map(|f| f.name.as_str()).collect();
                return Err(Error::invalid("field", format!("{} has no field `{k}`{} (fields: {})", m.name, bar_hint(m, k), names.join(", "))));
            };
            let mut v = coerce(f, v.clone())?;
            if let (FieldType::Ref(t), Value::String(id)) = (&f.ty, &v) {
                let id = if t == super::models::BARS { id.to_lowercase() } else { self.resolve_ref(t, id)? };
                if !self.record_exists(t, &id)? {
                    let tn = self.model(t)?.map(|x| x.name).unwrap_or_else(|| t.clone());
                    return Err(Error::invalid("value", format!("{}: no {tn} record `{id}`", f.name)));
                }
                v = Value::String(id);
            }
            if f.required && v.is_null() {
                return Err(Error::invalid("value", format!("{} is required", f.name)));
            }
            out.push((f.clone(), v));
        }
        Ok(out)
    }

    /// A ref given as an id prefix → the full id (if exactly one record has it).
    fn resolve_ref(&self, model_id: &str, id: &str) -> Result<String> {
        let Some(t) = self.model(model_id)? else { return Ok(id.to_string()) };
        Ok(self.find_record(&t, id).map(|r| r.id).unwrap_or_else(|_| id.to_string()))
    }

    /// Create a record; missing fields get their defaults.
    pub fn create_record(&self, m: &Model, values: &Map<String, Value>) -> Result<Record> {
        self.need_table(m)?;
        if m.is_bars() {
            let id = crate::code::bars::create_record(self, m, values)?;
            return self.find_record(m, &id);
        }
        let given = self.values_for(m, values)?;
        let mut cols = vec!["id".to_string(), "_version".into(), "_created_at".into(), "_updated_at".into(), "_author".into()];
        let now = now_ms();
        let author = self.current_author_id()?;
        let id = new_id();
        let mut args: Vec<Sql> =
            vec![id.clone().into(), m.version.into(), now.into(), now.into(), author.clone().map(Sql::from).unwrap_or(Sql::Null)];
        for f in &m.design.fields {
            let v = match given.iter().find(|(g, _)| g.id == f.id) {
                Some((_, v)) => v.clone(),
                None => match &f.default {
                    Some(d) => coerce(f, d.clone())?,
                    None if f.required => return Err(Error::invalid("value", format!("{} is required", f.name))),
                    None => Value::Null,
                },
            };
            cols.push(q(&f.name));
            args.push(to_sql(&v, &f.ty));
        }
        let marks: Vec<String> = (1..=cols.len()).map(|i| format!("?{i}")).collect();
        self.conn
            .execute(&format!("INSERT INTO {}({}) VALUES ({})", q(&m.table()), cols.join(", "), marks.join(", ")), params_from_iter(args))
            .map_err(|e| friendly(e, m))?;
        self.find_record(m, &id)
    }

    /// Change fields of one record (null clears a field that is not required).
    pub fn update_record(&self, m: &Model, id: &str, values: &Map<String, Value>) -> Result<Record> {
        self.need_table(m)?;
        let r = self.find_record(m, id)?;
        if m.is_bars() {
            let (id, _) = crate::code::bars::update_record(self, m, &r.id, values)?;
            return self.find_record(m, &id);
        }
        self.update_ids(m, std::slice::from_ref(&r.id), values)?;
        self.find_record(m, &r.id)
    }

    /// Change fields of every record matching the filter; returns how many.
    pub fn update_records(&self, m: &Model, filter: &[Condition], values: &Map<String, Value>) -> Result<usize> {
        self.need_table(m)?;
        let ids: Vec<String> = self.records(m, &RecordQuery { filter: filter.to_vec(), ..Default::default() })?.into_iter().map(|r| r.id).collect();
        self.update_ids(m, &ids, values)
    }

    /// Change fields of records known to exist (full ids). On bars, built-in fields change
    /// the bars (a key field re-keys the bar) and the rest go to `mx_bars`.
    pub(crate) fn update_ids(&self, m: &Model, ids: &[String], values: &Map<String, Value>) -> Result<usize> {
        if m.is_bars() {
            let (core, _) = crate::code::bars::split(m, values)?;
            if !core.is_empty() {
                self.conn.execute_batch("SAVEPOINT bars_update")?;
                let r = ids.iter().try_for_each(|id| crate::code::bars::update_record(self, m, id, values).map(|_| ()));
                match r {
                    Ok(()) => self.conn.execute_batch("RELEASE bars_update")?,
                    Err(e) => {
                        let _ = self.conn.execute_batch("ROLLBACK TO bars_update; RELEASE bars_update");
                        return Err(e);
                    }
                }
                return Ok(ids.len());
            }
        }
        self.update_stored(m, ids, values)
    }

    /// Change stored fields (not the bars' own columns) of records known to exist.
    pub(crate) fn update_stored(&self, m: &Model, ids: &[String], values: &Map<String, Value>) -> Result<usize> {
        if m.is_bars() && !self.table_exists(&m.table())? {
            return Err(Error::invalid(
                "model",
                format!("no fields have been added to {} yet (`mnnn model edit {} --add NAME:TYPE --apply`)", m.name, m.name),
            ));
        }
        let given = self.values_for(m, values)?;
        if given.is_empty() {
            return Err(Error::invalid("record", "give at least one FIELD=VALUE to set"));
        }
        let now = now_ms();
        let author: Sql = self.current_author_id()?.map(Sql::from).unwrap_or(Sql::Null);
        let t = q(&m.table());
        let sql = if m.is_bars() {
            let cols: Vec<String> = given.iter().map(|(f, _)| q(&f.name)).collect();
            let marks: Vec<String> = (0..given.len()).map(|i| format!("?{}", i + 5)).collect();
            let set: Vec<String> = cols.iter().map(|c| format!("{c} = excluded.{c}")).collect();
            format!(
                "INSERT INTO {t}(id, _version, _created_at, _updated_at, _author, {}) VALUES (?1, ?2, ?3, ?3, ?4, {}) ON CONFLICT(id) DO UPDATE SET _version = excluded._version, _updated_at = excluded._updated_at, _author = excluded._author, {}",
                cols.join(", "),
                marks.join(", "),
                set.join(", ")
            )
        } else {
            let set: Vec<String> = given.iter().enumerate().map(|(i, (f, _))| format!("{} = ?{}", q(&f.name), i + 5)).collect();
            format!("UPDATE {t} SET _version = ?2, _updated_at = ?3, _author = ?4, {} WHERE id = ?1", set.join(", "))
        };
        self.conn.execute_batch("SAVEPOINT record_update")?;
        let r = (|| -> Result<usize> {
            let mut st = self.conn.prepare(&sql)?;
            let mut n = 0;
            for id in ids {
                let key: Sql = if m.is_bars() { Sql::Blob(hex::decode(id).unwrap_or_default()) } else { id.clone().into() };
                let mut args: Vec<Sql> = vec![key, m.version.into(), now.into(), author.clone()];
                args.extend(given.iter().map(|(f, v)| to_sql(v, &f.ty)));
                n += st.execute(params_from_iter(args)).map_err(|e| friendly(e, m))?;
            }
            Ok(n)
        })();
        match r {
            Ok(n) => {
                self.conn.execute_batch("RELEASE record_update")?;
                Ok(n)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK TO record_update; RELEASE record_update");
                Err(e)
            }
        }
    }

    /// Records elsewhere referring to these: (model name, count).
    fn referrers(&self, m: &Model, ids: &[String]) -> Result<Vec<(String, usize)>> {
        let mut out = vec![];
        for o in self.models()? {
            for f in o.design.fields.iter().filter(|f| f.ty == FieldType::Ref(m.id.clone())) {
                if o.version == 0 || !self.table_exists(&o.table())? {
                    continue;
                }
                let mut n = 0;
                let mut st = self.conn.prepare(&format!("SELECT COUNT(*) FROM {} WHERE {} = ?1", q(&o.table()), q(&f.name)))?;
                for id in ids {
                    n += st.query_row([id], |r| r.get::<_, i64>(0))? as usize;
                }
                if n > 0 {
                    out.push((format!("{}.{}", o.name, f.name), n));
                }
            }
        }
        Ok(out)
    }

    /// Delete records by id; refused while other records refer to them.
    pub fn delete_records_by_id(&self, m: &Model, ids: &[String]) -> Result<usize> {
        self.need_table(m)?;
        let refs = self.referrers(m, ids)?;
        if !refs.is_empty() {
            let r: Vec<String> = refs.into_iter().map(|(f, n)| format!("{n} in {f}")).collect();
            return Err(Error::invalid("record", format!("still referred to ({}); change those first", r.join(", "))));
        }
        if m.is_bars() {
            return crate::code::bars::delete(self, ids);
        }
        let mut st = self.conn.prepare(&format!("DELETE FROM {} WHERE id = ?1", q(&m.table())))?;
        let mut n = 0;
        for id in ids {
            n += st.execute([id])?;
        }
        Ok(n)
    }

    pub fn delete_record(&self, m: &Model, id: &str) -> Result<Record> {
        let r = self.find_record(m, id)?;
        self.delete_records_by_id(m, std::slice::from_ref(&r.id))?;
        Ok(r)
    }

    /// Delete every record matching the filter.
    pub fn delete_records(&self, m: &Model, filter: &[Condition]) -> Result<usize> {
        let ids: Vec<String> = self.records(m, &RecordQuery { filter: filter.to_vec(), ..Default::default() })?.into_iter().map(|r| r.id).collect();
        self.delete_records_by_id(m, &ids)
    }

    /// Insert records as they are (archive import): ids, stamps and values kept.
    pub(crate) fn insert_raw_records(&self, m: &Model, rows: &[Record]) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        self.need_table(m)?;
        let t = q(&m.table());
        let names: Vec<String> = m.design.stored().map(|f| q(&f.name)).collect();
        let marks: Vec<String> = (1..=5 + names.len()).map(|i| format!("?{i}")).collect();
        let extra = if names.is_empty() { String::new() } else { format!(", {}", names.join(", ")) };
        let mut st = self
            .conn
            .prepare(&format!("INSERT OR REPLACE INTO {t}(id, _version, _created_at, _updated_at, _author{extra}) VALUES ({})", marks.join(", ")))?;
        let mut n = 0;
        for r in rows {
            if m.is_bars() && !self.record_exists(super::models::BARS, &r.id)? {
                continue;
            }
            let key: Sql = if m.is_bars() { Sql::Blob(hex::decode(&r.id).unwrap_or_default()) } else { r.id.clone().into() };
            let mut args: Vec<Sql> =
                vec![key, m.version.into(), r.created_at.into(), r.updated_at.into(), r.author_id.clone().map(Sql::from).unwrap_or(Sql::Null)];
            for f in m.design.stored() {
                let v = r.values.get(&f.name).cloned().unwrap_or(Value::Null);
                args.push(to_sql(&super::models::convert(&v, &f.ty).unwrap_or(Value::Null), &f.ty));
            }
            n += st.execute(params_from_iter(args)).map_err(|e| friendly(e, m))?;
        }
        Ok(n)
    }
}

/// Why a bars field name is not there, when it was a built-in one: renamed, retyped or dropped.
pub fn bar_hint(m: &Model, name: &str) -> String {
    if !m.is_bars() || !super::models::BAR_FIELDS.iter().any(|(n, _)| *n == name) {
        return String::new();
    }
    let id = format!("{}{name}", super::models::BAR_ID_PREFIX);
    match m.design.by_id(&id) {
        Some(f) => format!(" (the built-in {name} is called `{}` now)", f.name),
        None => {
            format!(" (the built-in {name} was dropped from {}; `mnnn model history {}` shows when, and rolling back brings it back)", m.name, m.name)
        }
    }
}

/// A bars condition that SQL can do exactly (numbers and times, exact names), or `None`.
fn bar_condition(m: &Model, c: &Condition) -> Option<(String, Sql)> {
    let key = bar_column(m, &c.field)?;
    let op = match c.op {
        Op::Gt => ">",
        Op::Ge => ">=",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Eq => "=",
        _ => return None,
    };
    let v = c.value.trim();
    match key {
        "timestamp" => {
            let t = v.parse::<i64>().ok().or_else(|| crate::ingest::time::parse_ts(v))?;
            Some((format!("b.timestamp {op} ?"), Sql::Integer(t)))
        }
        "open" | "high" | "low" | "close" | "volume" | "flags" => {
            let x: f64 = v.parse().ok()?;
            Some((format!("b.{key} {op} ?"), Sql::Real(x)))
        }
        "period" if c.op == Op::Eq => Some(("b.duration_s = ?".into(), Sql::Integer(crate::model::Duration::parse(v).ok()?.0))),
        "source" if c.op == Op::Eq && !v.contains('*') => Some(("s.name = ? COLLATE NOCASE".into(), Sql::Text(v.to_string()))),
        "base" | "quote" if c.op == Op::Eq && !v.contains('*') => Some((format!("b.{key} = ?"), Sql::Text(v.to_uppercase()))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::models::{Design, Field, FieldType};
    use super::*;
    use crate::model::{Bar, Duration, Symbol};
    use serde_json::json;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn signals(s: &Store) -> Model {
        let mut price = Field::new("price", FieldType::Real);
        price.required = true;
        let mut side = Field::new("side", FieldType::Text);
        side.choices = vec!["buy".into(), "sell".into()];
        side.default = Some(json!("buy"));
        let mut code = Field::new("code", FieldType::Text);
        code.unique = true;
        let at = Field::new("at", FieldType::Timestamp);
        s.create_model("signals", "", Design { fields: vec![price, side, code, at], ..Default::default() }, true).unwrap().0
    }

    #[test]
    fn crud_filters_groups_and_sorts() {
        let s = Store::open_in_memory().unwrap();
        let m = signals(&s);
        let a = s.create_record(&m, &map(json!({"price": "1.5", "code": "a", "at": "2024-01-01"}))).unwrap();
        assert_eq!(a.values["side"], json!("buy"), "default");
        assert_eq!(a.values["at"], json!(1704067200000i64));
        assert_eq!(a.version, 1);
        s.create_record(&m, &map(json!({"price": 3, "side": "SELL", "code": "b", "at": "2024-02-01"}))).unwrap();
        s.create_record(&m, &map(json!({"price": 2, "side": "sell", "code": "c"}))).unwrap();
        assert!(s.create_record(&m, &map(json!({"side": "buy"}))).unwrap_err().to_string().contains("price is required"));
        assert!(s.create_record(&m, &map(json!({"price": 1, "code": "a"}))).unwrap_err().to_string().contains("unique"));
        assert!(s.create_record(&m, &map(json!({"price": 1, "nope": 1}))).unwrap_err().to_string().contains("no field `nope`"));
        assert!(s.create_record(&m, &map(json!({"price": "x"}))).unwrap_err().to_string().contains("is not real"));

        let cond = |c: &str| crate::rules::parse_condition(c).unwrap();
        let sells =
            s.records(&m, &RecordQuery { filter: vec![cond("side = sell")], sort: vec![("price".into(), true)], ..Default::default() }).unwrap();
        assert_eq!(sells.iter().map(|r| r.values["code"].clone()).collect::<Vec<_>>(), vec![json!("b"), json!("c")]);
        let after = s.records(&m, &RecordQuery { filter: vec![cond("at >= 2024-01-15")], ..Default::default() }).unwrap();
        assert_eq!(after.len(), 1, "timestamps take dates");
        let g = s.group_records(&m, &RecordQuery::default(), &["side".into()], &["sum:price".into(), "max:price".into()]).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!((g[0].key["side"].clone(), g[0].count, g[1].aggs["sum:price"].clone()), (json!("buy"), 1, json!(5.0)));
        assert!(s.group_records(&m, &RecordQuery::default(), &[], &["mean:price".into()]).is_err());

        let up = s.update_record(&m, a.short_id(), &map(json!({"price": 9}))).unwrap();
        assert_eq!(up.values["price"], json!(9.0));
        assert!(s.update_record(&m, &a.id, &map(json!({"price": null}))).is_err(), "required");
        assert_eq!(s.update_records(&m, &[cond("side = sell")], &map(json!({"side": "buy"}))).unwrap(), 2);
        assert_eq!(s.records(&m, &RecordQuery { filter: vec![cond("side = buy")], ..Default::default() }).unwrap().len(), 3);
        assert_eq!(s.records(&m, &RecordQuery { limit: Some(2), offset: 2, ..Default::default() }).unwrap().len(), 1);
        assert_eq!(s.records(&m, &RecordQuery { text: "c".into(), ..Default::default() }).unwrap()[0].values["code"], json!("c"));
        assert!(s.records(&m, &RecordQuery { filter: vec![cond("zip = 1")], ..Default::default() }).is_err());

        // refs: a record that is referred to cannot be deleted
        let r = s
            .create_model("notes", "", Design { fields: vec![Field::new("signal", FieldType::Ref(m.id.clone()))], ..Default::default() }, true)
            .unwrap()
            .0;
        assert!(s.create_record(&r, &map(json!({"signal": "nope"}))).is_err());
        let n = s.create_record(&r, &map(json!({"signal": a.short_id()}))).unwrap();
        assert_eq!(n.values["signal"], json!(a.id), "prefix resolved");
        assert!(s.delete_record(&m, &a.id).unwrap_err().to_string().contains("referred to"));
        s.delete_record(&r, &n.id).unwrap();
        assert_eq!(s.delete_records(&m, &[cond("price < 5")]).unwrap(), 2);
        assert_eq!(s.count_records(&m).unwrap(), 1);
    }

    #[test]
    fn bars_are_records_with_added_fields() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("bn", "csv", None).unwrap();
        let sym = Symbol { base: "BTC".into(), quote: "USDT".into() };
        let bars: Vec<Bar> = (0..4)
            .map(|i| Bar { ts: 1_704_067_200_000 + i * 300_000, open: 1.0 + i as f64, high: 2.0, low: 0.5, close: 1.5, volume: 10.0, flags: 0 })
            .collect();
        s.upsert_bars(&src, &sym, Duration(300), &bars).unwrap();
        let m = s.find_model("bars").unwrap();
        let all = s.records(&m, &RecordQuery { filter: vec![crate::rules::parse_condition("open >= 3").unwrap()], ..Default::default() }).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].values["symbol"].clone(), all[0].values["period"].clone()), (json!("BTC/USDT"), json!("5m")));
        assert!(s.update_records(&m, &[], &map(json!({"score": 1}))).is_err(), "no fields added yet");
        let mut score = Field::new("score", FieldType::Real);
        score.default = Some(json!(0));
        s.set_model_draft("bars", Some(Design { fields: vec![score], ..Default::default() })).unwrap();
        let (m, _) = s.apply_model("bars", &Default::default()).unwrap();
        let n = s.update_records(&m, &[crate::rules::parse_condition("open > 2").unwrap()], &map(json!({"score": 5}))).unwrap();
        assert_eq!(n, 2);
        let v = s.records(&m, &RecordQuery { sort: vec![("score".into(), true), ("open".into(), false)], ..Default::default() }).unwrap();
        assert_eq!(v.iter().map(|r| r.values["score"].clone()).collect::<Vec<_>>(), vec![json!(5.0), json!(5.0), json!(0), json!(0)]);
        // built-in fields change like any other (the bars change)
        assert_eq!(s.update_records(&m, &[crate::rules::parse_condition("open = 1").unwrap()], &map(json!({"close": 1.25}))).unwrap(), 1);
        assert_eq!(
            s.records(&m, &RecordQuery { filter: vec![crate::rules::parse_condition("close < 1.5").unwrap()], ..Default::default() }).unwrap().len(),
            1
        );
        let e = s.create_record(&m, &map(json!({"score": 1}))).unwrap_err().to_string();
        assert!(e.contains("needs source"), "{e}");
        s.update_records(&m, &[crate::rules::parse_condition("open = 1").unwrap()], &map(json!({"close": 1.5}))).unwrap();
        let first = &v[0];
        // added values follow a source rename (the bar id changes) and go with removals
        s.rename_source("bn", "binance").unwrap();
        let v = s.records(&m, &RecordQuery { filter: vec![crate::rules::parse_condition("score = 5").unwrap()], ..Default::default() }).unwrap();
        assert_eq!(v.len(), 2);
        assert_ne!(v[0].id, first.id);
        let limited = s.records(&m, &RecordQuery { limit: Some(1), sort: vec![("open".into(), true)], ..Default::default() }).unwrap();
        assert_eq!(limited[0].values["open"], json!(4.0));
        s.remove_source("binance").unwrap();
        assert_eq!(s.conn.query_row("SELECT COUNT(*) FROM mx_bars", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    }
}
