//! Linked-node sync (2.11): every node of a user keeps all of the user's data.
//!
//! Triggers on each synced table write the key of every changed row to `sync_log`
//! (one entry per row, the latest change last). A linked node pulls the log after its
//! cursor and gets each row as it is now, or a tombstone. Per peer it remembers the
//! hash both sides last agreed on (`sync_base`), so it can tell who changed a row:
//! only them (take theirs), only us (keep ours; they pull it from us) or both (a
//! conflict). Conflicts keep the newer row and save the other in `sync_conflicts`;
//! nothing is dropped silently. An edit always wins over a delete.
//!
//! Node-local things never sync: identities, exchanges, peers, shards, settings,
//! import runs, and a few columns (exchange ids, the user's key, run counters).
//! Bars sync on request only (`mnnn sync bars`). See `docs/sync.md`.
//!
//! The log, row codec, agreed hashes, conflicts and the three-way decision are the
//! `weirdnet-storage` change log; the table registry, model tables, history import, epoch
//! and the order changes travel in are Muninn's.
use super::models::{BARS, Model, q};
use super::{Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use weirdnet_storage::sync::{self as log, Backfill, Table, Verdict, alike, row_time};

pub use weirdnet_storage::sync::{Applied, Change, Conflict, Page, Row, row_hash};

pub const MIGRATION: &str = r#"
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
CREATE TABLE sync_imported (
    peer TEXT NOT NULL,
    remote_id INTEGER NOT NULL,
    local_id INTEGER NOT NULL,
    PRIMARY KEY (peer, remote_id)
);
"#;

/// Synced tables and their node-local columns (never sent, never overwritten).
pub const TABLES: &[(&str, &[&str])] = &[
    ("authors", &[]),
    ("users", &["keypair"]),
    ("user_nodes", &[]),
    ("connections", &[]),
    ("assets", &[]),
    ("asset_aliases", &[]),
    ("models", &[]),
    ("model_versions", &[]),
    ("journals", &[]),
    ("journal_entries", &[]),
    ("journal_annotations", &[]),
    ("journal_attachments", &[]),
    ("live_searches", &[]),
    ("projects", &[]),
    ("project_items", &[]),
    ("reports", &[]),
    ("report_sections", &[]),
    ("rules", &["runs", "last_run_at", "last_status"]),
    ("code_spaces", &["approved_hash", "last_run"]),
    ("reviews", &[]),
    ("review_items", &[]),
    ("review_reviewers", &["exchange_id"]),
    ("revisions", &[]),
    ("orgs", &["hosted"]),
    ("org_roles", &[]),
    ("org_members", &["exchange_id"]),
    ("org_nodes", &[]),
    ("org_endpoints", &[]),
    ("org_projects", &[]),
    ("org_project_members", &[]),
    ("org_resources", &[]),
    ("org_rules", &[]),
    ("org_templates", &[]),
    ("org_template_assets", &[]),
    ("actions", &[]),
    ("notifications", &[]),
    ("messages", &[]),
];

/// The log key of a model's records.
pub fn model_key(id: &str) -> String {
    format!("model:{id}")
}

const EPOCH: &str = "sync.epoch";

impl Store {
    fn sync_ready(&self) -> Result<bool> {
        self.table_exists("sync_log")
    }

    /// The table a log key stands for here, if this node has it.
    fn target(&self, key: &str) -> Result<Option<Table>> {
        let (table, local): (String, &'static [&'static str]) = if let Some(id) = key.strip_prefix("model:") {
            if id == BARS {
                return Ok(None);
            }
            match self.model(id)? {
                Some(m) if m.version > 0 => (m.table(), &[]),
                _ => return Ok(None),
            }
        } else {
            match TABLES.iter().find(|(t, _)| *t == key) {
                Some((t, l)) => (t.to_string(), l),
                None => return Ok(None),
            }
        };
        Ok(Table::resolve(&self.conn, &table, local)?)
    }

    /// Make sure every synced table logs its changes (new model tables included); a
    /// table that did not log yet has all its rows logged once.
    pub fn sync_ensure(&self) -> Result<()> {
        if !self.sync_ready()? {
            return Ok(());
        }
        let mut keys: Vec<String> = TABLES.iter().map(|(t, _)| t.to_string()).collect();
        for m in self.models()? {
            if !m.is_bars() && m.version > 0 {
                keys.push(model_key(&m.id));
            }
        }
        let skip_bars = format!("id <> '{BARS}'");
        for key in keys {
            let Some(t) = self.target(&key)? else { continue };
            let has = log::trigger_count(&self.conn, &t.name)?;
            if has >= 3 {
                continue;
            }
            let backfill = if key == "models" {
                Backfill::Where(&skip_bars)
            } else if key.starts_with("model:") || has == 0 {
                Backfill::All
            } else {
                Backfill::None
            };
            log::log_table(&self.conn, &key, &t, backfill)?;
        }
        Ok(())
    }

    /// This database's sync epoch (made on first use).
    pub fn sync_epoch(&self) -> Result<String> {
        if let Some(e) = self.setting(EPOCH)? {
            return Ok(e);
        }
        let e = new_id();
        self.set_setting(EPOCH, Some(&e))?;
        Ok(e)
    }

    fn read_row(&self, t: &Table, pk: &serde_json::Value) -> Result<Option<Row>> {
        Ok(log::read_row(&self.conn, t, pk)?)
    }

    fn write_row(&self, t: &Table, pk: &serde_json::Value, row: Option<&Row>) -> Result<()> {
        Ok(log::write_row(&self.conn, t, pk, row)?)
    }

    /// The changes after `cursor` (at most `limit`, about `max_bytes`), as they are now, for `asker`.
    pub fn sync_changes(&self, asker: &str, cursor: i64, limit: usize, max_bytes: usize) -> Result<Page> {
        self.sync_ensure()?;
        let epoch = self.sync_epoch()?;
        let rows: Vec<(i64, String, String, i64, i64)> =
            log::log_after(&self.conn, cursor, limit + 1)?.into_iter().map(|e| (e.seq, e.tbl, e.pk, e.deleted as i64, e.at)).collect();
        let mut more = rows.len() > limit;
        let mut page = Page { epoch, next: cursor, ..Default::default() };
        let mut bytes = 0usize;
        let mut sent_models: Vec<String> = vec![];
        for (seq, tbl, pk, deleted, at) in rows.into_iter().take(limit) {
            let pkv: serde_json::Value = serde_json::from_str(&pk).unwrap_or(serde_json::Value::Null);
            // the bars model row stays local (bars sync on request)
            if tbl == "models" && pkv.get(0).and_then(|v| v.as_str()) == Some(BARS) {
                page.next = seq;
                continue;
            }
            // records travel after their model: send the model along if it is newer
            if let Some(id) = tbl.strip_prefix("model:")
                && !sent_models.iter().any(|m| m == id)
            {
                sent_models.push(id.to_string());
                let mpk = serde_json::json!([id]);
                let mseq = log::log_seq(&self.conn, "models", &mpk.to_string())?;
                if mseq.is_some_and(|m| m > seq)
                    && let Some(t) = self.target("models")?
                {
                    let row = self.read_row(&t, &mpk)?;
                    let seen = self.base(asker, "models", &mpk.to_string())?;
                    page.changes.push(Change { seq, tbl: "models".into(), pk: mpk, row, at, seen });
                }
            }
            let row = if deleted == 1 {
                None
            } else {
                match self.target(&tbl)? {
                    Some(t) => self.read_row(&t, &pkv)?,
                    None => {
                        page.next = seq;
                        continue;
                    }
                }
            };
            bytes += row.as_ref().map(|r| serde_json::to_vec(r).map(|b| b.len()).unwrap_or(0)).unwrap_or(0) + pk.len() + tbl.len();
            if tbl == "models"
                && let Some(id) = pkv.get(0).and_then(|v| v.as_str())
            {
                sent_models.push(id.to_string());
            }
            let seen = self.base(asker, &tbl, &pk)?;
            page.changes.push(Change { seq, tbl, pk: pkv, row, at, seen });
            page.next = seq;
            if bytes >= max_bytes {
                more = true;
                break;
            }
        }
        page.more = more;
        Ok(page)
    }

    fn base(&self, peer: &str, tbl: &str, pk: &str) -> Result<Option<Option<String>>> {
        Ok(log::base(&self.conn, peer, tbl, pk)?)
    }

    fn set_base(&self, peer: &str, tbl: &str, pk: &str, hash: Option<&str>) -> Result<()> {
        Ok(log::set_base(&self.conn, peer, tbl, pk, hash)?)
    }

    #[allow(clippy::too_many_arguments)]
    fn note_conflict(&self, tbl: &str, pk: &str, peer: &str, kept: &str, mine: Option<&Row>, theirs: Option<&Row>, reason: &str) -> Result<()> {
        let id = new_id();
        Ok(log::note_conflict(&self.conn, log::NewConflict { id: &id, tbl, pk, peer, kept, mine, theirs, reason, at: now_ms() })?)
    }

    /// Add (or update) an action of `peer` to this node's history under a local id.
    fn import_action(&self, peer: &str, c: &Change) -> Result<bool> {
        let Some(row) = c.row.as_ref() else { return Ok(false) };
        let Some(rid) = c.pk.get(0).and_then(|v| v.as_i64()) else { return Ok(false) };
        let local = |id: i64| -> Result<Option<i64>> {
            Ok(self
                .conn
                .query_row("SELECT local_id FROM sync_imported WHERE peer = ?1 AND remote_id = ?2", params![peer, id], |r| r.get(0))
                .optional()?)
        };
        let st = |k: &str| row.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let parent = match row.get("parent_id").and_then(|v| v.as_i64()) {
            Some(p) => local(p)?,
            None => None,
        };
        let vals = params![
            parent,
            st("kind").unwrap_or_default(),
            st("summary").unwrap_or_default(),
            st("params").unwrap_or_else(|| "{}".into()),
            st("origin").unwrap_or_default(),
            st("author_id"),
            row.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0),
            st("state_before"),
            st("state_after"),
        ];
        let known = local(rid)?;
        let id = match known {
            Some(id) => {
                self.conn.execute(
                    "UPDATE actions SET parent_id = ?1, kind = ?2, summary = ?3, params = ?4, origin = ?5, author_id = ?6, \
                     created_at = ?7, state_before = ?8, state_after = ?9 WHERE id = ?10",
                    rusqlite::params_from_iter(vals.iter().copied().chain(std::iter::once(&id as &dyn rusqlite::ToSql))),
                )?;
                id
            }
            None => {
                self.conn.execute(
                    "INSERT INTO actions(parent_id, kind, summary, params, origin, author_id, created_at, state_before, state_after) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    vals,
                )?;
                let id = self.conn.last_insert_rowid();
                self.conn.execute("INSERT INTO sync_imported(peer, remote_id, local_id) VALUES (?1, ?2, ?3)", params![peer, rid, id])?;
                id
            }
        };
        // not this node's own: never sent on
        self.conn.execute("DELETE FROM sync_log WHERE tbl = 'actions' AND pk = json_array(?1)", [id])?;
        Ok(known.is_none())
    }

    /// Write a row taken from a linked node; models also get their table.
    fn take(&self, t: &Table, c: &Change) -> Result<()> {
        if c.tbl != "models" {
            return self.write_row(t, &c.pk, c.row.as_ref());
        }
        let Some(row) = c.row.as_ref() else { return Ok(()) };
        let id = c.pk.get(0).and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let before = self.model(&id)?;
        self.write_row(t, &c.pk, Some(row))?;
        let Some(m) = self.model(&id)? else { return Ok(()) };
        self.adopt_model(before.as_ref(), &m)
    }

    /// Bring a model's table in line with a design that came from a linked node.
    fn adopt_model(&self, before: Option<&Model>, m: &Model) -> Result<()> {
        if m.is_bars() || m.version == 0 {
            return Ok(());
        }
        let mut old = before.cloned();
        if let Some(o) = old.as_mut()
            && o.name != m.name
        {
            if o.version > 0 && self.table_exists(&o.table())? && !self.table_exists(&m.table())? {
                self.conn.execute_batch(&format!("ALTER TABLE {} RENAME TO {}", q(&o.table()), q(&m.table())))?;
            }
            o.name = m.name.clone();
        }
        let same = old.as_ref().is_some_and(|o| {
            o.version == m.version
                && serde_json::to_string(&o.design).ok() == serde_json::to_string(&m.design).ok()
                && self.table_exists(&m.table()).unwrap_or(false)
        });
        if same {
            return Ok(());
        }
        let rows = match &old {
            Some(o) if o.version > 0 => self.raw_rows(o)?,
            _ => vec![],
        };
        let shape = old.unwrap_or_else(|| Model { version: 0, ..m.clone() });
        self.rebuild(&shape, &m.design, m.version, &rows)?;
        self.sync_ensure()
    }

    /// Apply a page pulled from `peer` (a node of this user).
    pub fn sync_apply(&self, peer: &str, page: &Page) -> Result<Applied> {
        self.sync_ensure()?;
        let mut out = Applied::default();
        // parents may come after their children: keys are checked by the other node
        self.conn.execute_batch("PRAGMA foreign_keys = OFF")?;
        let r = self.atomic("sync_apply", || -> Result<()> {
            let mut order: Vec<&Change> = page.changes.iter().filter(|c| c.tbl == "models").collect();
            order.extend(page.changes.iter().filter(|c| c.tbl != "models"));
            for c in order {
                // history is per node (local ids): another node's actions are added to it
                if c.tbl == "actions" {
                    let added = std::cell::Cell::new(false);
                    if self.sp("sync_row", || self.import_action(peer, c).map(|n| added.set(n))).is_ok() && added.get() {
                        out.history += 1;
                    }
                    continue;
                }
                let Some(t) = self.target(&c.tbl)? else { continue };
                let pk = c.pk.to_string();
                let mine = self.read_row(&t, &c.pk)?;
                let lh = mine.as_ref().map(row_hash);
                let rh = c.row.as_ref().map(row_hash);
                if lh == rh {
                    out.same += 1;
                    self.set_base(peer, &c.tbl, &pk, rh.as_deref())?;
                    continue;
                }
                let base = self.base(peer, &c.tbl, &pk)?.flatten();
                // they already had my version when they changed it: theirs is newer
                let step = match log::decide(lh.as_deref(), rh.as_deref(), base.as_deref(), c.seen.as_ref().map(|h| h.as_deref())) {
                    Verdict::Take => "take",
                    Verdict::Keep => "keep",
                    Verdict::Same | Verdict::Conflict => "conflict",
                };
                match step {
                    "take" => {
                        self.sp("sync_row", || self.take(&t, c)).or_else(|e| {
                            self.note_conflict(&c.tbl, &pk, peer, "mine", mine.as_ref(), c.row.as_ref(), &format!("could not apply: {e}"))?;
                            out.conflicts += 1;
                            Ok::<(), Error>(())
                        })?;
                        if c.tbl == "actions" {
                            out.history += 1;
                        } else if c.row.is_some() {
                            out.taken += 1;
                        } else {
                            out.deleted += 1;
                        }
                    }
                    "keep" => {
                        out.kept += 1;
                        continue;
                    }
                    _ if base.is_none() && mine.as_ref().zip(c.row.as_ref()).is_some_and(|(a, b)| alike(a, b)) => {
                        // made on both before they ever met, alike but for times and blanks: no conflict
                        let (a, b) = (mine.as_ref().expect("mine"), c.row.as_ref().expect("theirs"));
                        if row_time(b) > row_time(a) {
                            self.sp("sync_row", || self.take(&t, c))?;
                            out.taken += 1;
                        } else {
                            out.same += 1;
                        }
                    }
                    _ => {
                        let theirs_wins = log::theirs_win(mine.as_ref(), c.row.as_ref());
                        let reason = log::conflict_reason(mine.as_ref(), c.row.as_ref());
                        if theirs_wins {
                            if let Err(e) = self.sp("sync_row", || self.take(&t, c)) {
                                self.note_conflict(&c.tbl, &pk, peer, "mine", mine.as_ref(), c.row.as_ref(), &format!("could not apply: {e}"))?;
                            } else {
                                self.note_conflict(&c.tbl, &pk, peer, "theirs", mine.as_ref(), c.row.as_ref(), reason)?;
                                out.taken += 1;
                            }
                        } else {
                            self.note_conflict(&c.tbl, &pk, peer, "mine", mine.as_ref(), c.row.as_ref(), reason)?;
                            out.kept += 1;
                        }
                        out.conflicts += 1;
                    }
                }
                self.set_base(peer, &c.tbl, &pk, rh.as_deref())?;
            }
            Ok(())
        });
        self.conn.execute_batch("PRAGMA foreign_keys = ON")?;
        r?;
        Ok(out)
    }

    fn sp(&self, name: &str, f: impl FnOnce() -> Result<()>) -> Result<()> {
        self.atomic(name, f)
    }

    pub fn sync_conflicts(&self, all: bool) -> Result<Vec<Conflict>> {
        Ok(log::conflicts(&self.conn, all)?)
    }

    pub fn sync_conflict(&self, key: &str) -> Result<Conflict> {
        let all = self.sync_conflicts(true)?;
        let m: Vec<&Conflict> = all.iter().filter(|c| c.id == key || (key.len() >= 4 && c.id.starts_with(key))).collect();
        match m.as_slice() {
            [c] => Ok((*c).clone()),
            [] => Err(Error::invalid("conflict", format!("`{key}` not found"))),
            _ => Err(Error::invalid("conflict", format!("`{key}` matches more than one; give more of the id"))),
        }
    }

    /// Settle a conflict: keep what is in the table, or put the other row back
    /// (`use_other`); the change then syncs to the other nodes like any edit.
    pub fn sync_resolve(&self, key: &str, use_other: bool) -> Result<Conflict> {
        let c = self.sync_conflict(key)?;
        if c.resolved_at.is_some() {
            return Err(Error::invalid("conflict", "already resolved"));
        }
        let mut resolution = format!("kept {}", c.kept);
        if use_other {
            let other = if c.kept == "mine" { &c.theirs } else { &c.mine };
            let row: Option<Row> = other.as_deref().map(serde_json::from_str).transpose()?;
            let t = self.target(&c.tbl)?.ok_or_else(|| Error::invalid("conflict", format!("{} is not here any more", c.tbl)))?;
            let pk: serde_json::Value = serde_json::from_str(&c.pk)?;
            self.sp("sync_resolve", || {
                self.take(&t, &Change { seq: 0, tbl: c.tbl.clone(), pk: pk.clone(), row: row.clone(), at: now_ms(), seen: None })
            })?;
            resolution = format!("took {}", if c.kept == "mine" { "theirs" } else { "mine" });
        }
        log::mark_resolved(&self.conn, &c.id, now_ms(), &resolution)?;
        self.sync_conflict(&c.id)
    }

    /// How many changes are waiting in the log after a cursor (for status).
    pub fn sync_pending(&self, cursor: i64) -> Result<i64> {
        Ok(log::pending(&self.conn, cursor)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pull(from: &Store, to: &Store, peer: &str) -> Applied {
        let mut cursor = 0;
        let mut total = Applied::default();
        loop {
            let asker = if peer == "A" { "B" } else { "A" };
            let p = from.sync_changes(asker, cursor, 50, 1 << 20).unwrap();
            total.add(&to.sync_apply(peer, &p).unwrap());
            cursor = p.next;
            if !p.more {
                break;
            }
        }
        total
    }

    fn journal(s: &Store, name: &str) -> String {
        s.conn
            .execute(
                "INSERT INTO journals(id, name, format, created_at, updated_at) VALUES (?1, ?2, 'markdown', 1, 1)",
                params![format!("j-{name}"), name],
            )
            .unwrap();
        format!("j-{name}")
    }

    fn name(s: &Store, id: &str) -> Option<String> {
        s.conn.query_row("SELECT name FROM journals WHERE id = ?1", [id], |r| r.get(0)).optional().unwrap()
    }

    #[test]
    fn the_change_log_is_the_storage_engines_byte_for_byte() {
        let s = Store::open_in_memory().unwrap();
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(weirdnet_storage::sync::SCHEMA).unwrap();
        let sql = |conn: &rusqlite::Connection, kind: &str, name: &str| -> String {
            conn.query_row("SELECT sql FROM sqlite_master WHERE type = ?1 AND name = ?2", [kind, name], |r| r.get(0)).unwrap()
        };
        for t in ["sync_log", "sync_base", "sync_conflicts"] {
            assert_eq!(sql(&s.conn, "table", t), sql(&c, "table", t));
        }
        // trigger names and bodies as Muninn has always made them (existing databases keep theirs)
        assert_eq!(
            sql(&s.conn, "trigger", "sync_journals_ai"),
            "CREATE TRIGGER \"sync_journals_ai\" AFTER INSERT ON \"journals\" BEGIN DELETE FROM sync_log WHERE tbl = 'journals' AND pk = json_array(NEW.\"id\"); \
             INSERT INTO sync_log(tbl, pk, deleted, at) VALUES ('journals', json_array(NEW.\"id\"), 0, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)); END"
        );
        assert_eq!(
            sql(&s.conn, "trigger", "sync_journals_ad"),
            "CREATE TRIGGER \"sync_journals_ad\" AFTER DELETE ON \"journals\" BEGIN DELETE FROM sync_log WHERE tbl = 'journals' AND pk = json_array(OLD.\"id\"); \
             INSERT INTO sync_log(tbl, pk, deleted, at) VALUES ('journals', json_array(OLD.\"id\"), 1, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)); END"
        );
    }

    #[test]
    fn rows_travel_both_ways_and_echoes_are_quiet() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let j = journal(&a, "notes");
        let got = pull(&a, &b, "A");
        assert!(got.taken >= 1, "{got:?}");
        assert_eq!(name(&b, &j).as_deref(), Some("notes"));
        // B pulling back changes nothing on A
        let back = pull(&b, &a, "B");
        assert_eq!(back.changed(), 0, "{back:?} {:?}", a.sync_conflicts(true).unwrap());
        assert_eq!(back.conflicts, 0);
        // an edit on B reaches A
        b.conn.execute("UPDATE journals SET name = 'ideas', updated_at = 5 WHERE id = ?1", [&j]).unwrap();
        let r = pull(&b, &a, "B");
        assert_eq!(r.conflicts, 0, "{r:?}");
        assert_eq!(name(&a, &j).as_deref(), Some("ideas"));
        // a delete on A reaches B
        a.conn.execute("DELETE FROM journals WHERE id = ?1", [&j]).unwrap();
        let r = pull(&a, &b, "A");
        assert_eq!(r.deleted, 1, "{r:?}");
        assert_eq!(name(&b, &j), None);
    }

    #[test]
    fn edits_on_both_nodes_keep_the_newer_and_save_the_other() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let j = journal(&a, "notes");
        pull(&a, &b, "A");
        pull(&b, &a, "B");
        a.conn.execute("UPDATE journals SET name = 'from a', updated_at = 10 WHERE id = ?1", [&j]).unwrap();
        b.conn.execute("UPDATE journals SET name = 'from b', updated_at = 20 WHERE id = ?1", [&j]).unwrap();
        let r = pull(&b, &a, "B");
        assert_eq!(r.conflicts, 1, "{r:?}");
        assert_eq!(name(&a, &j).as_deref(), Some("from b"));
        let cs = a.sync_conflicts(false).unwrap();
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].kept, "theirs");
        assert!(cs[0].mine.as_deref().unwrap().contains("from a"));
        // B agrees without losing anything either
        let r = pull(&a, &b, "A");
        assert_eq!(name(&b, &j).as_deref(), Some("from b"));
        assert_eq!(r.changed(), 0, "{r:?}");
        // put the older one back
        a.sync_resolve(&cs[0].id, true).unwrap();
        assert_eq!(name(&a, &j).as_deref(), Some("from a"));
        assert!(a.sync_conflicts(false).unwrap().is_empty());
        pull(&a, &b, "A");
        assert_eq!(name(&b, &j).as_deref(), Some("from a"));
    }

    #[test]
    fn an_edit_wins_over_a_delete() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        let j = journal(&a, "notes");
        pull(&a, &b, "A");
        pull(&b, &a, "B");
        a.conn.execute("DELETE FROM journals WHERE id = ?1", [&j]).unwrap();
        b.conn.execute("UPDATE journals SET name = 'kept', updated_at = 9 WHERE id = ?1", [&j]).unwrap();
        let r = pull(&a, &b, "A");
        assert_eq!(r.conflicts, 1, "{r:?}");
        assert_eq!(name(&b, &j).as_deref(), Some("kept"));
        pull(&b, &a, "B");
        assert_eq!(name(&a, &j).as_deref(), Some("kept"));
    }

    #[test]
    fn local_columns_stay_local() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        a.conn
            .execute("INSERT INTO users(id, label, public_key, keypair, created_at, updated_at) VALUES ('u1', 'ada', x'01', x'99', 1, 1)", [])
            .unwrap();
        pull(&a, &b, "A");
        let (label, kp): (String, Option<Vec<u8>>) =
            b.conn.query_row("SELECT label, keypair FROM users WHERE id = 'u1'", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(label, "ada");
        assert_eq!(kp, None);
        let p = a.sync_changes("B", 0, 1000, 1 << 20).unwrap();
        assert!(!serde_json::to_string(&p).unwrap().contains("keypair"));
    }

    #[test]
    fn history_is_added_under_local_ids_and_not_sent_back() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        a.note_action(&crate::store::NewAction::new("test.a", "cli", "on a"));
        b.note_action(&crate::store::NewAction::new("test.b", "cli", "on b"));
        let r = pull(&a, &b, "A");
        assert_eq!(r.conflicts, 0, "{r:?}");
        let kinds = |s: &Store| -> Vec<String> {
            let mut st = s.conn.prepare("SELECT kind FROM actions ORDER BY id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().map(|k| k.unwrap()).collect()
        };
        assert_eq!(kinds(&b), vec!["test.b", "test.a"]);
        let back = pull(&b, &a, "B");
        assert_eq!(back.conflicts, 0, "{back:?}");
        assert_eq!(kinds(&a), vec!["test.a", "test.b"]);
        // nothing more travels, and again is the same
        assert_eq!(pull(&a, &b, "A").history, 0);
        assert_eq!(kinds(&b).len(), 2);
    }

    #[test]
    fn rows_made_on_both_alike_but_for_times_are_no_conflict() {
        let a = Store::open_in_memory().unwrap();
        let b = Store::open_in_memory().unwrap();
        a.conn
            .execute("INSERT INTO users(id, label, public_key, author_id, created_at, updated_at) VALUES ('u1', 'ada', x'01', NULL, 1, 1)", [])
            .unwrap();
        b.conn
            .execute("INSERT INTO users(id, label, public_key, author_id, created_at, updated_at) VALUES ('u1', 'ada', x'01', NULL, 2, 2)", [])
            .unwrap();
        let r = pull(&a, &b, "A");
        assert_eq!(r.conflicts, 0, "{r:?}");
    }
}
