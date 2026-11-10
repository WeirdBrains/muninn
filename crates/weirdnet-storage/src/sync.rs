//! A row change log for keeping copies of a database in step.
//!
//! Triggers on each registered table write the key of every changed row to `sync_log` (one
//! entry per row, the latest change last). A peer reads the log after its cursor and gets
//! each row as it is now, or a tombstone. Per peer the hash both sides last agreed on is
//! kept in `sync_base`, so [`decide`] can tell who changed a row: only them (take theirs),
//! only us (keep ours) or both (a conflict, recorded in `sync_conflicts`).
//!
//! The caller owns the registry (which tables, which columns stay local), the cursor
//! epoch, ordering rules between tables and what applying a row means for its schema.
use crate::error::{Result, StorageError};
use crate::quote_ident as q;
use base64::Engine;
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The log, agreed hashes and conflicts. Run it as one of your migrations.
pub const SCHEMA: &str = r#"
CREATE TABLE sync_log (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    tbl TEXT NOT NULL,
    pk TEXT NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0,
    at INTEGER NOT NULL,
    UNIQUE (tbl, pk)
);
CREATE TABLE sync_base (
    peer TEXT NOT NULL,
    tbl TEXT NOT NULL,
    pk TEXT NOT NULL,
    hash TEXT,
    PRIMARY KEY (peer, tbl, pk)
);
CREATE TABLE sync_conflicts (
    id TEXT PRIMARY KEY,
    tbl TEXT NOT NULL,
    pk TEXT NOT NULL,
    peer TEXT NOT NULL,
    kept TEXT NOT NULL,
    mine TEXT,
    theirs TEXT,
    reason TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    resolved_at INTEGER,
    resolution TEXT
);
"#;

/// A row as it travels: column → value (blobs as `{"$b": base64}`).
pub type Row = BTreeMap<String, serde_json::Value>;

/// One changed row: as it is now, or `None` when it was deleted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub seq: i64,
    pub tbl: String,
    pub pk: serde_json::Value,
    pub row: Option<Row>,
    pub at: i64,
    /// The hash the sender last agreed on with the asker for this row (outer `None`: never).
    /// Lets the asker see the sender already had its version, so no conflict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen: Option<Option<String>>,
}

/// A page of changes after a cursor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Page {
    /// Changes if this differs from what the cursor was taken under (start over).
    pub epoch: String,
    pub changes: Vec<Change>,
    /// The cursor to ask from next.
    pub next: i64,
    pub more: bool,
}

/// What applying a page did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    pub taken: usize,
    pub deleted: usize,
    pub kept: usize,
    pub same: usize,
    pub conflicts: usize,
    /// History entries taken (not counted as changes: syncing them makes more history).
    #[serde(default)]
    pub history: usize,
}

impl Applied {
    pub fn add(&mut self, o: &Applied) {
        self.taken += o.taken;
        self.deleted += o.deleted;
        self.kept += o.kept;
        self.same += o.same;
        self.conflicts += o.conflicts;
        self.history += o.history;
    }
    pub fn changed(&self) -> usize {
        self.taken + self.deleted
    }
}

/// A recorded conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    pub tbl: String,
    pub pk: String,
    pub peer: String,
    /// "mine" or "theirs": which row is in the table now.
    pub kept: String,
    pub mine: Option<String>,
    pub theirs: Option<String>,
    pub reason: String,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
    pub resolution: Option<String>,
}

/// A table resolved for reading and writing rows by primary key.
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub pk: Vec<String>,
    pub cols: Vec<String>,
    /// Columns that stay on this copy: never sent, never overwritten.
    pub local: Vec<String>,
}

impl Table {
    /// Read a table's columns and primary key. `None` if it does not exist or has no key.
    pub fn resolve(conn: &Connection, name: &str, local: &[&str]) -> Result<Option<Table>> {
        if !crate::table_exists(conn, name)? {
            return Ok(None);
        }
        let mut st = conn.prepare(&format!("PRAGMA table_info({})", q(name)))?;
        let mut cols = vec![];
        let mut pk: Vec<(i64, String)> = vec![];
        for r in st.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))? {
            let (c, k) = r?;
            if k > 0 {
                pk.push((k, c.clone()));
            }
            cols.push(c);
        }
        if pk.is_empty() {
            return Ok(None);
        }
        pk.sort();
        Ok(Some(Table {
            name: name.to_string(),
            pk: pk.into_iter().map(|(_, c)| c).collect(),
            cols,
            local: local.iter().map(|s| s.to_string()).collect(),
        }))
    }

    fn is_local(&self, c: &str) -> bool {
        self.local.iter().any(|l| l == c)
    }
}

/// Milliseconds since the epoch, in SQL (for triggers).
pub fn now_sql() -> &'static str {
    "CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)"
}

/// An SQL value as it travels.
pub fn value_json(v: ValueRef) -> serde_json::Value {
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => i.into(),
        ValueRef::Real(f) => serde_json::Number::from_f64(f).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
        ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
        ValueRef::Blob(b) => serde_json::json!({ "$b": base64::engine::general_purpose::STANDARD.encode(b) }),
    }
}

/// A travelling value back to SQL.
pub fn json_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Integer(*b as i64),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().unwrap_or(0.0)),
        },
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Object(o) => match o.get("$b").and_then(|b| b.as_str()) {
            Some(b) => Value::Blob(base64::engine::general_purpose::STANDARD.decode(b).unwrap_or_default()),
            None => Value::Text(v.to_string()),
        },
        serde_json::Value::Array(_) => Value::Text(v.to_string()),
    }
}

/// The hash a row is compared by (local columns are never part of a row).
pub fn row_hash(row: &Row) -> String {
    let b = serde_json::to_vec(row).unwrap_or_default();
    hex::encode(&Sha256::digest(&b)[..16])
}

/// The same row but for its times and blanks (one side empty, the other set).
pub fn alike(a: &Row, b: &Row) -> bool {
    let skip = |k: &str| matches!(k, "created_at" | "updated_at" | "_created_at" | "_updated_at");
    let blank = |v: Option<&serde_json::Value>| v.is_none_or(|v| v.is_null() || v.as_str() == Some(""));
    a.keys().chain(b.keys()).filter(|k| !skip(k)).all(|k| {
        let (x, y) = (a.get(k), b.get(k));
        x == y || blank(x) || blank(y)
    })
}

/// When a row was last changed, for picking the newer one in a conflict.
pub fn row_time(row: &Row) -> i64 {
    for c in ["updated_at", "_updated_at", "created_at", "_created_at", "added_at", "issued_at", "stored_at"] {
        if let Some(t) = row.get(c).and_then(|v| v.as_i64()) {
            return t;
        }
    }
    0
}

/// What to do with a row from a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Both sides have the same row.
    Same,
    /// Only they changed it (or they had seen ours): take theirs.
    Take,
    /// Only we changed it: keep ours (they pull it from us).
    Keep,
    /// Both changed it.
    Conflict,
}

/// Decide from our row's hash, theirs, the hash we last agreed on with this peer and the
/// hash the peer last agreed on with us (`seen`; outer `None`: never).
pub fn decide(mine: Option<&str>, theirs: Option<&str>, base: Option<&str>, seen: Option<Option<&str>>) -> Verdict {
    if mine == theirs {
        Verdict::Same
    } else if mine == base || seen.is_some_and(|h| h == mine) {
        Verdict::Take
    } else if theirs == base {
        Verdict::Keep
    } else {
        Verdict::Conflict
    }
}

/// In a conflict: does their row win? An edit beats a delete; otherwise the newer row, ties
/// broken by hash so both sides pick the same one.
pub fn theirs_win(mine: Option<&Row>, theirs: Option<&Row>) -> bool {
    match (mine, theirs) {
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(a), Some(b)) => (row_time(b), Some(row_hash(b))) > (row_time(a), Some(row_hash(a))),
        (None, None) => false,
    }
}

/// Why two rows conflict, in words.
pub fn conflict_reason(mine: Option<&Row>, theirs: Option<&Row>) -> &'static str {
    match (mine, theirs) {
        (None, _) => "deleted here, changed there",
        (_, None) => "changed here, deleted there",
        _ => "changed on both nodes",
    }
}

/// How many logging triggers (`sync_*`) a table has.
pub fn trigger_count(conn: &Connection, table: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'trigger' AND tbl_name = ?1 AND name LIKE 'sync\\_%' ESCAPE '\\'",
        [table],
        |r| r.get(0),
    )?)
}

/// Which existing rows to log when a table starts logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backfill<'a> {
    None,
    All,
    /// Rows matching an SQL condition (the caller's, trusted).
    Where(&'a str),
}

/// (Re)create the insert, update and delete triggers that log `t` under `key`
/// (`sync_<key>_ai|au|ad`), and log existing rows as `backfill` says.
pub fn log_table(conn: &Connection, key: &str, t: &Table, backfill: Backfill) -> Result<()> {
    let tag: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let lit = key.replace('\'', "''");
    let pk = |side: &str| t.pk.iter().map(|c| format!("{side}.{}", q(c))).collect::<Vec<_>>().join(", ");
    let tq = q(&t.name);
    let now = now_sql();
    let mut sql = String::new();
    for (n, ev, side, del) in [("ai", "INSERT", "NEW", 0), ("au", "UPDATE", "NEW", 0), ("ad", "DELETE", "OLD", 1)] {
        sql.push_str(&format!(
            "DROP TRIGGER IF EXISTS {name}; CREATE TRIGGER {name} AFTER {ev} ON {tq} BEGIN \
             DELETE FROM sync_log WHERE tbl = '{lit}' AND pk = json_array({p}); \
             INSERT INTO sync_log(tbl, pk, deleted, at) VALUES ('{lit}', json_array({p}), {del}, {now}); END;",
            name = q(&format!("sync_{tag}_{n}")),
            p = pk(side),
        ));
    }
    let filter = match backfill {
        Backfill::None => None,
        Backfill::All => Some(String::new()),
        Backfill::Where(w) => Some(format!(" WHERE {w}")),
    };
    if let Some(skip) = filter {
        let plain = t.pk.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
        sql.push_str(&format!("INSERT OR IGNORE INTO sync_log(tbl, pk, deleted, at) SELECT '{lit}', json_array({plain}), 0, {now} FROM {tq}{skip};"));
    }
    conn.execute_batch(&sql)?;
    Ok(())
}

fn pk_where(t: &Table, pk: &serde_json::Value) -> Result<(String, Vec<Value>)> {
    let vals = pk.as_array().ok_or_else(|| StorageError::invalid("sync", "bad key"))?;
    if vals.len() != t.pk.len() {
        return Err(StorageError::invalid("sync", "key does not fit the table"));
    }
    let w = t.pk.iter().enumerate().map(|(i, c)| format!("{} IS ?{}", q(c), i + 1)).collect::<Vec<_>>().join(" AND ");
    Ok((w, vals.iter().map(json_value).collect()))
}

/// A row as it is now (without local columns), by its key (a JSON array).
pub fn read_row(conn: &Connection, t: &Table, pk: &serde_json::Value) -> Result<Option<Row>> {
    let (w, vals) = pk_where(t, pk)?;
    let cols: Vec<&String> = t.cols.iter().filter(|c| !t.is_local(c)).collect();
    let sel = cols.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT {sel} FROM {} WHERE {w}", q(&t.name));
    Ok(conn
        .query_row(&sql, params_from_iter(vals), |r| {
            let mut row = Row::new();
            for (i, c) in cols.iter().enumerate() {
                row.insert((*c).clone(), value_json(r.get_ref(i)?));
            }
            Ok(row)
        })
        .optional()?)
}

/// Write a row (insert or update the columns it has; local columns untouched), or delete
/// it (`None`).
pub fn write_row(conn: &Connection, t: &Table, pk: &serde_json::Value, row: Option<&Row>) -> Result<()> {
    let (w, vals) = pk_where(t, pk)?;
    let Some(row) = row else {
        conn.execute(&format!("DELETE FROM {} WHERE {w}", q(&t.name)), params_from_iter(vals))?;
        return Ok(());
    };
    let cols: Vec<&String> = t.cols.iter().filter(|c| !t.is_local(c) && row.contains_key(*c)).collect();
    let exists: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {} WHERE {w}", q(&t.name)), params_from_iter(vals.clone()), |r| r.get(0))?;
    let args: Vec<Value> = cols.iter().map(|c| json_value(&row[*c])).collect();
    if exists > 0 {
        let set: Vec<&&String> = cols.iter().filter(|c| !t.pk.contains(c)).collect();
        if set.is_empty() {
            return Ok(());
        }
        let n = vals.len();
        let sets = set.iter().enumerate().map(|(i, c)| format!("{} = ?{}", q(c), n + i + 1)).collect::<Vec<_>>().join(", ");
        let mut all = vals;
        all.extend(set.iter().map(|c| json_value(&row[**c])));
        conn.execute(&format!("UPDATE {} SET {sets} WHERE {w}", q(&t.name)), params_from_iter(all))?;
    } else {
        let names = cols.iter().map(|c| q(c)).collect::<Vec<_>>().join(", ");
        let marks = (1..=cols.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
        conn.execute(&format!("INSERT INTO {}({names}) VALUES ({marks})", q(&t.name)), params_from_iter(args))?;
    }
    Ok(())
}

/// One log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub seq: i64,
    pub tbl: String,
    /// The key as stored: a JSON array.
    pub pk: String,
    pub deleted: bool,
    pub at: i64,
}

/// Log entries after `cursor`, oldest first, at most `limit`.
pub fn log_after(conn: &Connection, cursor: i64, limit: usize) -> Result<Vec<LogEntry>> {
    let mut st = conn.prepare("SELECT seq, tbl, pk, deleted, at FROM sync_log WHERE seq > ?1 ORDER BY seq LIMIT ?2")?;
    let v = st
        .query_map(params![cursor, limit as i64], |r| {
            Ok(LogEntry { seq: r.get(0)?, tbl: r.get(1)?, pk: r.get(2)?, deleted: r.get::<_, i64>(3)? == 1, at: r.get(4)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

/// Where a row is in the log, if it is.
pub fn log_seq(conn: &Connection, tbl: &str, pk: &str) -> Result<Option<i64>> {
    Ok(conn.query_row("SELECT seq FROM sync_log WHERE tbl = ?1 AND pk = ?2", [tbl, pk], |r| r.get(0)).optional()?)
}

/// How many entries are after a cursor.
pub fn pending(conn: &Connection, cursor: i64) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sync_log WHERE seq > ?1", [cursor], |r| r.get(0))?)
}

/// The hash last agreed on with `peer` for a row (outer `None`: never).
pub fn base(conn: &Connection, peer: &str, tbl: &str, pk: &str) -> Result<Option<Option<String>>> {
    Ok(conn
        .query_row("SELECT hash FROM sync_base WHERE peer = ?1 AND tbl = ?2 AND pk = ?3", params![peer, tbl, pk], |r| r.get::<_, Option<String>>(0))
        .optional()?)
}

/// Remember the hash agreed on with `peer` for a row (`None`: both have it deleted).
pub fn set_base(conn: &Connection, peer: &str, tbl: &str, pk: &str, hash: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_base(peer, tbl, pk, hash) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(peer, tbl, pk) DO UPDATE SET hash = excluded.hash",
        params![peer, tbl, pk, hash],
    )?;
    Ok(())
}

/// A conflict to record.
#[derive(Debug, Clone, Copy)]
pub struct NewConflict<'a> {
    pub id: &'a str,
    pub tbl: &'a str,
    pub pk: &'a str,
    pub peer: &'a str,
    pub kept: &'a str,
    pub mine: Option<&'a Row>,
    pub theirs: Option<&'a Row>,
    pub reason: &'a str,
    pub at: i64,
}

/// Record a conflict (both rows saved; nothing is dropped silently).
pub fn note_conflict(conn: &Connection, c: NewConflict) -> Result<()> {
    let j = |r: Option<&Row>| r.map(|r| serde_json::to_string(r).unwrap_or_default());
    conn.execute(
        "INSERT INTO sync_conflicts(id, tbl, pk, peer, kept, mine, theirs, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![c.id, c.tbl, c.pk, c.peer, c.kept, j(c.mine), j(c.theirs), c.reason, c.at],
    )?;
    Ok(())
}

/// Recorded conflicts, newest first (`all`: resolved ones too).
pub fn conflicts(conn: &Connection, all: bool) -> Result<Vec<Conflict>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, tbl, pk, peer, kept, mine, theirs, reason, created_at, resolved_at, resolution FROM sync_conflicts {} ORDER BY created_at DESC",
        if all { "" } else { "WHERE resolved_at IS NULL" }
    ))?;
    Ok(st
        .query_map([], |r| {
            Ok(Conflict {
                id: r.get(0)?,
                tbl: r.get(1)?,
                pk: r.get(2)?,
                peer: r.get(3)?,
                kept: r.get(4)?,
                mine: r.get(5)?,
                theirs: r.get(6)?,
                reason: r.get(7)?,
                created_at: r.get(8)?,
                resolved_at: r.get(9)?,
                resolution: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

/// Mark a conflict settled.
pub fn mark_resolved(conn: &Connection, id: &str, at: i64, resolution: &str) -> Result<()> {
    conn.execute("UPDATE sync_conflicts SET resolved_at = ?2, resolution = ?3 WHERE id = ?1", params![id, at, resolution])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(SCHEMA).unwrap();
        c.execute_batch("CREATE TABLE notes (id TEXT PRIMARY KEY, body TEXT, secret TEXT, pic BLOB, updated_at INTEGER)").unwrap();
        c.execute("INSERT INTO notes VALUES ('n0', 'old', 's', x'00ff', 1)", []).unwrap();
        let t = Table::resolve(&c, "notes", &["secret"]).unwrap().unwrap();
        log_table(&c, "notes", &t, Backfill::All).unwrap();
        c
    }

    #[test]
    fn triggers_log_every_change_once() {
        let c = db();
        assert_eq!(trigger_count(&c, "notes").unwrap(), 3);
        c.execute("INSERT INTO notes VALUES ('n1', 'a', 's', NULL, 2)", []).unwrap();
        c.execute("UPDATE notes SET body = 'b' WHERE id = 'n1'", []).unwrap();
        c.execute("DELETE FROM notes WHERE id = 'n0'", []).unwrap();
        let log = log_after(&c, 0, 10).unwrap();
        assert_eq!(log.iter().map(|e| (e.pk.as_str(), e.deleted)).collect::<Vec<_>>(), [(r#"["n1"]"#, false), (r#"["n0"]"#, true)]);
        assert_eq!(pending(&c, log[0].seq).unwrap(), 1);
        assert!(log_seq(&c, "notes", r#"["n1"]"#).unwrap().is_some());
        assert!(Table::resolve(&c, "missing", &[]).unwrap().is_none());
    }

    #[test]
    fn rows_round_trip_without_local_columns() {
        let (a, b) = (db(), db());
        let t = Table::resolve(&a, "notes", &["secret"]).unwrap().unwrap();
        let pk = serde_json::json!(["n0"]);
        a.execute("UPDATE notes SET body = 'new', secret = 'mine only', updated_at = 9 WHERE id = 'n0'", []).unwrap();
        let row = read_row(&a, &t, &pk).unwrap().unwrap();
        assert!(!row.contains_key("secret"));
        assert_eq!(row["pic"], serde_json::json!({"$b": "AP8="}));
        b.execute("UPDATE notes SET secret = 'b keeps this' WHERE id = 'n0'", []).unwrap();
        write_row(&b, &t, &pk, Some(&row)).unwrap();
        assert_eq!(read_row(&b, &t, &pk).unwrap().map(|r| row_hash(&r)), Some(row_hash(&row)));
        let s: String = b.query_row("SELECT secret FROM notes WHERE id = 'n0'", [], |r| r.get(0)).unwrap();
        assert_eq!(s, "b keeps this");
        write_row(&b, &t, &pk, None).unwrap();
        assert!(read_row(&b, &t, &pk).unwrap().is_none());
        assert!(read_row(&b, &t, &serde_json::json!(["x", "y"])).is_err());
        assert!(read_row(&b, &t, &serde_json::json!("n0")).is_err());
    }

    #[test]
    fn three_way_decisions() {
        let (x, y, z) = (Some("x"), Some("y"), Some("z"));
        assert_eq!(decide(x, x, None, None), Verdict::Same);
        assert_eq!(decide(x, y, x, None), Verdict::Take);
        assert_eq!(decide(x, y, y, None), Verdict::Keep);
        assert_eq!(decide(x, y, z, None), Verdict::Conflict);
        assert_eq!(decide(x, y, z, Some(x)), Verdict::Take);
        assert_eq!(decide(None, y, None, None), Verdict::Take);
        let mut a = Row::new();
        a.insert("updated_at".into(), 1.into());
        let mut b = a.clone();
        b.insert("updated_at".into(), 2.into());
        assert!(theirs_win(Some(&a), Some(&b)) && !theirs_win(Some(&b), Some(&a)));
        assert!(theirs_win(None, Some(&a)) && !theirs_win(Some(&a), None));
        assert!(alike(&a, &b));
        assert_eq!(conflict_reason(Some(&a), None), "changed here, deleted there");
    }

    #[test]
    fn conflicts_are_kept_and_resolved() {
        let c = db();
        let mut r = Row::new();
        r.insert("body".into(), "theirs".into());
        note_conflict(
            &c,
            NewConflict { id: "c1", tbl: "notes", pk: r#"["n0"]"#, peer: "p", kept: "mine", mine: None, theirs: Some(&r), reason: "test", at: 5 },
        )
        .unwrap();
        assert_eq!(conflicts(&c, false).unwrap().len(), 1);
        mark_resolved(&c, "c1", 6, "kept mine").unwrap();
        assert!(conflicts(&c, false).unwrap().is_empty());
        assert_eq!(conflicts(&c, true).unwrap()[0].resolution.as_deref(), Some("kept mine"));
        set_base(&c, "p", "notes", r#"["n0"]"#, Some("h")).unwrap();
        assert_eq!(base(&c, "p", "notes", r#"["n0"]"#).unwrap(), Some(Some("h".into())));
        assert_eq!(base(&c, "q", "notes", r#"["n0"]"#).unwrap(), None);
    }
}
