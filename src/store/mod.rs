//! SQLite / SQLCipher storage: schema, migrations, queries, upsert.
use crate::error::{Error, Result};
use crate::model::{Bar, BarId, Duration, Source, StoredBar, Symbol, KNOWN_ALIASES, KNOWN_BASES, KNOWN_QUOTES};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::path::Path;

mod query;
pub use query::BarSort;

pub const SCHEMA_VERSION: i64 = 1;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    locator TEXT,
    created_at INTEGER NOT NULL
);
CREATE TABLE bars (
    id BLOB PRIMARY KEY,
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    open REAL NOT NULL,
    high REAL NOT NULL,
    low REAL NOT NULL,
    close REAL NOT NULL,
    volume REAL NOT NULL,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    duration_s INTEGER NOT NULL,
    flags INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE INDEX bars_series ON bars(base, quote, source_id, duration_s, timestamp);
CREATE TABLE symbols (
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    PRIMARY KEY (base, quote)
);
CREATE TABLE assets (
    name TEXT PRIMARY KEY,
    is_quote INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE asset_aliases (
    alias TEXT PRIMARY KEY,
    canonical TEXT NOT NULL
);
CREATE TABLE ingest_runs (
    id INTEGER PRIMARY KEY,
    source_id TEXT NOT NULL,
    locator TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    duration_s INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    finished_at INTEGER NOT NULL,
    inserted INTEGER NOT NULL,
    duplicate INTEGER NOT NULL,
    rejected INTEGER NOT NULL
);
CREATE INDEX ingest_runs_hash ON ingest_runs(content_hash, source_id, base, quote, duration_s);
"#, r#"
-- Per-series bar count and time range, kept up to date by upsert_bars (listing
-- series by GROUP BY over millions of encrypted rows takes seconds).
CREATE TABLE series_stats (
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    duration_s INTEGER NOT NULL,
    count INTEGER NOT NULL,
    first INTEGER NOT NULL,
    last INTEGER NOT NULL,
    PRIMARY KEY (source_id, base, quote, duration_s)
);
INSERT INTO series_stats SELECT source_id, base, quote, duration_s, COUNT(*), MIN(timestamp), MAX(timestamp)
    FROM bars GROUP BY source_id, base, quote, duration_s;
"#];

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Summary of one series for listings and the navigator.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesInfo {
    pub source: String,
    pub symbol: Symbol,
    pub duration: Duration,
    pub count: i64,
    pub first: i64,
    pub last: i64,
}

impl SeriesInfo {
    /// Bars missing between first and last.
    pub fn missing(&self) -> i64 {
        let expected = (self.last - self.first) / self.duration.millis() + 1;
        (expected - self.count).max(0)
    }
}

/// Filter for queries/exports.
#[derive(Debug, Clone, Default)]
pub struct BarFilter {
    pub source: Option<String>,
    pub symbol: Option<Symbol>,
    pub duration: Option<Duration>,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub struct Store {
    pub conn: Connection,
}

/// Outcome of upserting a batch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpsertCounts {
    pub inserted: u64,
    pub duplicate: u64,
}

impl Store {
    /// Open (creating if needed) with an optional raw 256-bit SQLCipher key.
    pub fn open(path: &Path, key: Option<&[u8; 32]>) -> Result<Self> {
        if let Some(parent) = path.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let existed = path.exists();
        let conn = Connection::open(path)?;
        Self::apply_key(&conn, key)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // Verify the key works.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
            .map_err(|e| Error::Unlock(format!("wrong key or not a muninn database ({e})")))?;
        if !existed {
            crate::paths::set_mode(path, 0o600)?;
        }
        let mut s = Self { conn };
        s.migrate()?;
        Ok(s)
    }

    /// Bigger page cache for bulk reads (TUI loader connection).
    pub fn tune_for_reads(&self) {
        let _ = self.conn.execute_batch("PRAGMA cache_size = -262144; PRAGMA temp_store = MEMORY;");
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut s = Self { conn: Connection::open_in_memory()? };
        s.migrate()?;
        Ok(s)
    }

    /// Open an arbitrary SQLite file read-only (service ingest).
    pub fn open_readonly_raw(path: &Path, key: Option<&str>) -> Result<Connection> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI)?;
        if let Some(k) = key {
            conn.pragma_update(None, "key", k)?;
        }
        Ok(conn)
    }

    pub fn apply_key(conn: &Connection, key: Option<&[u8; 32]>) -> Result<()> {
        // Keep SQLCipher's own logging off stderr (wrong-key attempts are reported as errors by us).
        let _ = conn.query_row("PRAGMA cipher_log_level = NONE", [], |_| Ok(()));
        if let Some(k) = key {
            let mut lit = format!("x'{}'", hex::encode(k));
            let r = conn.pragma_update(None, "key", &lit);
            zeroize::Zeroize::zeroize(&mut lit);
            r?;
        }
        Ok(())
    }

    /// Re-encrypt the database with a new key (`PRAGMA rekey`).
    pub fn rekey(&self, new_key: &[u8; 32]) -> Result<()> {
        let mut lit = format!("x'{}'", hex::encode(new_key));
        let r = self.conn.pragma_update(None, "rekey", &lit);
        zeroize::Zeroize::zeroize(&mut lit);
        r?;
        Ok(())
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;
        let v: i64 = self
            .conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))?;
        let tx = self.conn.transaction()?;
        for (i, m) in MIGRATIONS.iter().enumerate() {
            let target = i as i64 + 1;
            if v < target {
                tx.execute_batch(m)?;
                tx.execute("INSERT INTO schema_version(version) VALUES (?1)", [target])?;
                if target == 1 {
                    for b in KNOWN_BASES {
                        tx.execute("INSERT OR IGNORE INTO assets(name, is_quote) VALUES (?1, 0)", [b])?;
                    }
                    for q in KNOWN_QUOTES {
                        tx.execute(
                            "INSERT INTO assets(name, is_quote) VALUES (?1, 1) ON CONFLICT(name) DO UPDATE SET is_quote = 1",
                            [q],
                        )?;
                    }
                    for (a, c) in KNOWN_ALIASES {
                        tx.execute("INSERT OR IGNORE INTO asset_aliases(alias, canonical) VALUES (?1, ?2)", [a, c])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?)
    }

    // ---- sources ----

    fn row_source(r: &rusqlite::Row) -> rusqlite::Result<Source> {
        Ok(Source { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, locator: r.get(3)?, created_at: r.get(4)? })
    }

    pub fn sources(&self) -> Result<Vec<Source>> {
        let mut st = self.conn.prepare("SELECT id, name, kind, locator, created_at FROM sources ORDER BY name")?;
        let v = st.query_map([], Self::row_source)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn source_by_name(&self, name: &str) -> Result<Option<Source>> {
        Ok(self
            .conn
            .query_row("SELECT id, name, kind, locator, created_at FROM sources WHERE name = ?1", [name], Self::row_source)
            .optional()?)
    }

    pub fn source_by_locator(&self, locator: &str) -> Result<Option<Source>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, name, kind, locator, created_at FROM sources WHERE locator = ?1 ORDER BY created_at LIMIT 1",
                [locator],
                Self::row_source,
            )
            .optional()?)
    }

    pub fn add_source(&self, name: &str, kind: &str, locator: Option<&str>) -> Result<Source> {
        let name = name.trim();
        if name.is_empty() || name.contains('|') {
            return Err(Error::invalid("source name", format!("`{name}` (must be non-empty, no `|`)")));
        }
        if self.source_by_name(name)?.is_some() {
            return Err(Error::invalid("source name", format!("`{name}` already exists")));
        }
        let s = Source {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            locator: locator.map(str::to_string),
            created_at: now_ms(),
        };
        self.conn.execute(
            "INSERT INTO sources(id, name, kind, locator, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![s.id, s.name, s.kind, s.locator, s.created_at],
        )?;
        Ok(s)
    }

    pub fn get_or_create_source(&self, name: &str, kind: &str, locator: Option<&str>) -> Result<Source> {
        match self.source_by_name(name)? {
            Some(s) => Ok(s),
            None => self.add_source(name, kind, locator),
        }
    }

    /// Rename a source and recompute its bars' ids in one transaction.
    pub fn rename_source(&mut self, from: &str, to: &str) -> Result<u64> {
        let src = self.source_by_name(from)?.ok_or_else(|| Error::invalid("source", format!("`{from}` not found")))?;
        if to.trim().is_empty() || to.contains('|') {
            return Err(Error::invalid("source name", to.to_string()));
        }
        if self.source_by_name(to)?.is_some() {
            return Err(Error::invalid("source name", format!("`{to}` already exists")));
        }
        let tx = self.conn.transaction()?;
        tx.execute("UPDATE sources SET name = ?1 WHERE id = ?2", params![to, src.id])?;
        let rows: Vec<(Vec<u8>, String, String, i64, i64)> = {
            let mut st = tx.prepare("SELECT id, base, quote, timestamp, duration_s FROM bars WHERE source_id = ?1")?;
            st.query_map([&src.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut n = 0;
        {
            let mut up = tx.prepare("UPDATE bars SET id = ?1 WHERE id = ?2")?;
            for (old, base, quote, ts, d) in rows {
                let id = BarId::compute(ts, to, &Symbol { base, quote }, Duration(d));
                up.execute(params![&id.0[..], old])?;
                n += 1;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    pub fn remove_source(&self, name: &str) -> Result<u64> {
        let src = self.source_by_name(name)?.ok_or_else(|| Error::invalid("source", format!("`{name}` not found")))?;
        let n = self.conn.execute("DELETE FROM bars WHERE source_id = ?1", [&src.id])?;
        self.conn.execute("DELETE FROM ingest_runs WHERE source_id = ?1", [&src.id])?;
        self.conn.execute("DELETE FROM series_stats WHERE source_id = ?1", [&src.id])?;
        self.conn.execute("DELETE FROM sources WHERE id = ?1", [&src.id])?;
        Ok(n as u64)
    }

    // ---- symbols / assets ----

    pub fn known_quotes(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT name FROM assets WHERE is_quote = 1")?;
        let v = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(v)
    }

    pub fn is_known_asset(&self, name: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM assets WHERE name = ?1", [name], |_| Ok(()))
            .optional()?
            .is_some())
    }

    pub fn canonical_asset(&self, name: &str) -> Result<String> {
        let up = name.to_ascii_uppercase();
        Ok(self
            .conn
            .query_row("SELECT canonical FROM asset_aliases WHERE alias = ?1", [&up], |r| r.get(0))
            .optional()?
            .unwrap_or(up))
    }

    pub fn canonical_symbol(&self, s: &Symbol) -> Result<Symbol> {
        Ok(Symbol { base: self.canonical_asset(&s.base)?, quote: self.canonical_asset(&s.quote)? })
    }

    pub fn add_symbol(&self, sym: &Symbol, aliases: &[String]) -> Result<()> {
        self.conn.execute("INSERT OR IGNORE INTO symbols(base, quote) VALUES (?1, ?2)", params![sym.base, sym.quote])?;
        self.conn.execute("INSERT OR IGNORE INTO assets(name, is_quote) VALUES (?1, 0)", [&sym.base])?;
        self.conn.execute(
            "INSERT INTO assets(name, is_quote) VALUES (?1, 1) ON CONFLICT(name) DO UPDATE SET is_quote = 1",
            [&sym.quote],
        )?;
        for a in aliases {
            self.conn.execute(
                "INSERT OR REPLACE INTO asset_aliases(alias, canonical) VALUES (?1, ?2)",
                params![a.to_ascii_uppercase(), sym.base],
            )?;
        }
        Ok(())
    }

    /// Known symbols: explicit ones plus every stored series.
    pub fn symbols(&self) -> Result<Vec<Symbol>> {
        let mut st = self.conn.prepare(
            "SELECT base, quote FROM symbols UNION SELECT DISTINCT base, quote FROM bars ORDER BY 1, 2",
        )?;
        let v = st
            .query_map([], |r| Ok(Symbol { base: r.get(0)?, quote: r.get(1)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn aliases(&self) -> Result<Vec<(String, String)>> {
        let mut st = self.conn.prepare("SELECT alias, canonical FROM asset_aliases ORDER BY alias")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    // ---- bars ----

    /// Insert bars, ignoring ids that already exist. Batched in one transaction.
    pub fn upsert_bars(&mut self, source: &Source, symbol: &Symbol, duration: Duration, bars: &[Bar]) -> Result<UpsertCounts> {
        self.upsert_bars_with(source, symbol, duration, bars, &mut |_| true)
    }

    /// Start an all-or-nothing unit (one input file). Nests with `upsert_bars`.
    pub fn begin_atomic(&self) -> Result<()> {
        Ok(self.conn.execute_batch("SAVEPOINT muninn_file")?)
    }
    pub fn commit_atomic(&self) -> Result<()> {
        Ok(self.conn.execute_batch("RELEASE muninn_file")?)
    }
    /// Undo everything since `begin_atomic` (bars, series_stats, ingest run).
    pub fn rollback_atomic(&self) -> Result<()> {
        Ok(self.conn.execute_batch("ROLLBACK TO muninn_file; RELEASE muninn_file")?)
    }

    /// Like `upsert_bars`; calls `on_rows(n)` every 4096 rows written (progress).
    /// `on_rows` returning false aborts: the batch is rolled back and `Error::Aborted` returned.
    pub fn upsert_bars_with(
        &mut self,
        source: &Source,
        symbol: &Symbol,
        duration: Duration,
        bars: &[Bar],
        on_rows: &mut dyn FnMut(u64) -> bool,
    ) -> Result<UpsertCounts> {
        // A savepoint: a transaction on its own, nested inside `begin_atomic` when used.
        let tx = self.conn.savepoint()?;
        let mut c = UpsertCounts::default();
        {
            let mut st = tx.prepare_cached(
                "INSERT INTO bars(id, base, quote, timestamp, open, high, low, close, volume, source_id, duration_s, flags)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) ON CONFLICT(id) DO NOTHING",
            )?;
            for (i, b) in bars.iter().enumerate() {
                if i % 4096 == 4095 && !on_rows(4096) {
                    return Err(Error::Aborted);
                }
                let id = BarId::compute(b.ts, &source.name, symbol, duration);
                let n = st.execute(params![
                    &id.0[..],
                    symbol.base,
                    symbol.quote,
                    b.ts,
                    b.open,
                    b.high,
                    b.low,
                    b.close,
                    b.volume,
                    source.id,
                    duration.0,
                    b.flags
                ])?;
                if n == 1 {
                    c.inserted += 1;
                } else {
                    c.duplicate += 1;
                }
            }
            if c.inserted > 0 {
                let lo = bars.iter().map(|b| b.ts).min().unwrap_or(0);
                let hi = bars.iter().map(|b| b.ts).max().unwrap_or(0);
                tx.execute(
                    "INSERT INTO series_stats(source_id, base, quote, duration_s, count, first, last) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT DO UPDATE SET count = count + excluded.count, first = min(first, excluded.first), last = max(last, excluded.last)",
                    params![source.id, symbol.base, symbol.quote, duration.0, c.inserted as i64, lo, hi],
                )?;
            }
        }
        tx.commit()?;
        Ok(c)
    }

    pub fn series(&self) -> Result<Vec<SeriesInfo>> {
        let mut st = self.conn.prepare(
            "SELECT s.name, b.base, b.quote, b.duration_s, b.count, b.first, b.last
             FROM series_stats b JOIN sources s ON s.id = b.source_id
             WHERE b.count > 0 ORDER BY s.name, b.base, b.quote, b.duration_s",
        )?;
        let v = st
            .query_map([], |r| {
                Ok(SeriesInfo {
                    source: r.get(0)?,
                    symbol: Symbol { base: r.get(1)?, quote: r.get(2)? },
                    duration: Duration(r.get(3)?),
                    count: r.get(4)?,
                    first: r.get(5)?,
                    last: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn count_bars(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM bars", [], |r| r.get(0))?)
    }

    pub fn query_bars(&self, f: &BarFilter) -> Result<Vec<StoredBar>> {
        let mut sql = String::from(
            "SELECT s.name, b.base, b.quote, b.duration_s, b.timestamp, b.open, b.high, b.low, b.close, b.volume, b.flags
             FROM bars b JOIN sources s ON s.id = b.source_id WHERE 1=1",
        );
        let mut p: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(s) = &f.source {
            sql.push_str(" AND s.name = ?");
            p.push(s.clone().into());
        }
        if let Some(sym) = &f.symbol {
            sql.push_str(" AND b.base = ? AND b.quote = ?");
            p.push(sym.base.clone().into());
            p.push(sym.quote.clone().into());
        }
        if let Some(d) = f.duration {
            sql.push_str(" AND b.duration_s = ?");
            p.push(d.0.into());
        }
        if let Some(t) = f.from {
            sql.push_str(" AND b.timestamp >= ?");
            p.push(t.into());
        }
        if let Some(t) = f.to {
            sql.push_str(" AND b.timestamp <= ?");
            p.push(t.into());
        }
        sql.push_str(" ORDER BY s.name, b.base, b.quote, b.duration_s, b.timestamp");
        if let Some(l) = f.limit {
            sql.push_str(&format!(" LIMIT {l}"));
            if let Some(o) = f.offset {
                sql.push_str(&format!(" OFFSET {o}"));
            }
        }
        let mut st = self.conn.prepare(&sql)?;
        let v = st
            .query_map(rusqlite::params_from_iter(p), |r| {
                Ok(StoredBar {
                    source: r.get(0)?,
                    symbol: Symbol { base: r.get(1)?, quote: r.get(2)? },
                    duration: Duration(r.get(3)?),
                    bar: Bar {
                        ts: r.get(4)?,
                        open: r.get(5)?,
                        high: r.get(6)?,
                        low: r.get(7)?,
                        close: r.get(8)?,
                        volume: r.get(9)?,
                        flags: r.get(10)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    // ---- ingest runs ----

    pub fn find_run(&self, hash: &str, source: &Source, symbol: &Symbol, duration: Duration) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM ingest_runs WHERE content_hash = ?1 AND source_id = ?2 AND base = ?3 AND quote = ?4 AND duration_s = ?5 LIMIT 1",
                params![hash, source.id, symbol.base, symbol.quote, duration.0],
                |r| r.get(0),
            )
            .optional()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_run(
        &self,
        source: &Source,
        locator: &str,
        hash: &str,
        symbol: &Symbol,
        duration: Duration,
        started: i64,
        counts: UpsertCounts,
        rejected: u64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ingest_runs(source_id, locator, content_hash, base, quote, duration_s, started_at, finished_at, inserted, duplicate, rejected)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                source.id,
                locator,
                hash,
                symbol.base,
                symbol.quote,
                duration.0,
                started,
                now_ms(),
                counts.inserted as i64,
                counts.duplicate as i64,
                rejected as i64
            ],
        )?;
        Ok(())
    }

    /// Online backup to `dest`, keyed with the same key (stays encrypted).
    pub fn backup_to(&self, dest: &Path, key: Option<&[u8; 32]>) -> Result<()> {
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }
        let mut dst = Connection::open(dest)?;
        Self::apply_key(&dst, key)?;
        {
            let b = rusqlite::backup::Backup::new(&self.conn, &mut dst)?;
            b.run_to_completion(1024, std::time::Duration::from_millis(0), None)?;
        }
        drop(dst);
        crate::paths::set_mode(dest, 0o600)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts: i64) -> Bar {
        Bar { ts, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 3.0, flags: 0 }
    }

    #[test]
    fn upsert_is_idempotent_and_durations_dont_collide() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("binance", "file", Some("/x")).unwrap();
        let sym = Symbol::new("BTC", "USDT");
        let bars: Vec<Bar> = (0..10).map(|i| bar(i * 300_000)).collect();
        let c = s.upsert_bars(&src, &sym, Duration(300), &bars).unwrap();
        assert_eq!(c, UpsertCounts { inserted: 10, duplicate: 0 });
        let c = s.upsert_bars(&src, &sym, Duration(300), &bars).unwrap();
        assert_eq!(c, UpsertCounts { inserted: 0, duplicate: 10 });
        let c = s.upsert_bars(&src, &sym, Duration(3600), &bars[..1]).unwrap();
        assert_eq!(c.inserted, 1);
        assert_eq!(s.count_bars().unwrap(), 11);
        let series = s.series().unwrap();
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].missing(), 0);
    }

    #[test]
    fn series_stats_track_upserts_and_removal() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("binance", "file", Some("/x")).unwrap();
        let sym = Symbol::new("BTC", "USDT");
        let bars: Vec<Bar> = (0..10).map(|i| bar(i * 300_000)).collect();
        s.upsert_bars(&src, &sym, Duration(300), &bars[3..8]).unwrap();
        s.upsert_bars(&src, &sym, Duration(300), &bars).unwrap();
        s.upsert_bars(&src, &sym, Duration(300), &bars[..2]).unwrap();
        let group: (i64, i64, i64) = s
            .conn
            .query_row("SELECT COUNT(*), MIN(timestamp), MAX(timestamp) FROM bars", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        let v = s.series().unwrap();
        assert_eq!((v[0].count, v[0].first, v[0].last), group);
        s.remove_source("binance").unwrap();
        assert!(s.series().unwrap().is_empty());
    }

    #[test]
    fn rename_recomputes_ids() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("a", "file", None).unwrap();
        let sym = Symbol::new("ETH", "USD");
        s.upsert_bars(&src, &sym, Duration(60), &[bar(0)]).unwrap();
        assert_eq!(s.rename_source("a", "b").unwrap(), 1);
        let id: Vec<u8> = s.conn.query_row("SELECT id FROM bars", [], |r| r.get(0)).unwrap();
        assert_eq!(id, BarId::compute(0, "b", &sym, Duration(60)).0.to_vec());
        assert!(s.source_by_name("a").unwrap().is_none());
    }

    #[test]
    fn aliases_and_symbols() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.canonical_asset("xbt").unwrap(), "BTC");
        assert!(s.known_quotes().unwrap().contains(&"USDT".to_string()));
        s.add_symbol(&Symbol::new("FOO", "USD"), &["FO".into()]).unwrap();
        assert_eq!(s.canonical_asset("FO").unwrap(), "FOO");
        assert!(s.symbols().unwrap().contains(&Symbol::new("FOO", "USD")));
    }

    #[test]
    fn query_filters() {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("a", "file", None).unwrap();
        let sym = Symbol::new("ETH", "USD");
        let bars: Vec<Bar> = (0..5).map(|i| bar(i * 60_000)).collect();
        s.upsert_bars(&src, &sym, Duration(60), &bars).unwrap();
        let f = BarFilter { from: Some(60_000), to: Some(180_000), ..Default::default() };
        assert_eq!(s.query_bars(&f).unwrap().len(), 3);
    }
}
