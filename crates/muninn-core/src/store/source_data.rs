//! Data as it arrived: every ingested file, HTTP page or query result is kept in
//! `source_data` byte for byte, under its source. Muninn does not interpret it here;
//! pipelines read it with a `source:NAME` node and code spaces (`parse-csv`, `to-bars`, …)
//! decide what it is.
use super::{Store, now_ms};
use crate::error::Result;
use rusqlite::{OptionalExtension, params};

pub const MIGRATION: &str = r#"
CREATE TABLE source_data (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    locator TEXT NOT NULL,
    name TEXT NOT NULL,
    format TEXT NOT NULL DEFAULT '',
    content_hash TEXT NOT NULL,
    size INTEGER NOT NULL,
    data BLOB NOT NULL,
    received_at INTEGER NOT NULL,
    run_id INTEGER
);
CREATE INDEX source_data_source ON source_data(source_id, received_at);
CREATE INDEX source_data_hash ON source_data(source_id, content_hash);
"#;

/// One stored item, without its bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceData {
    pub id: String,
    pub source_id: String,
    /// Where it came from (a path, a redacted URL, `redis://host/key`, …).
    pub locator: String,
    /// A short name (the file name, the URL's last segment, the key).
    pub name: String,
    /// What it looks like by its name or content type (`csv`, `json`, `ndjson`, `text`,
    /// …): a hint for code spaces, never enforced.
    pub format: String,
    pub content_hash: String,
    pub size: u64,
    pub received_at: i64,
    pub run_id: Option<i64>,
}

/// What to store.
pub struct NewSourceData<'a> {
    pub locator: &'a str,
    pub name: &'a str,
    pub format: &'a str,
    pub content_hash: &'a str,
    pub data: &'a [u8],
    pub run_id: Option<i64>,
}

const COLS: &str = "id, source_id, locator, name, format, content_hash, size, received_at, run_id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<SourceData> {
    Ok(SourceData {
        id: r.get(0)?,
        source_id: r.get(1)?,
        locator: r.get(2)?,
        name: r.get(3)?,
        format: r.get(4)?,
        content_hash: r.get(5)?,
        size: r.get::<_, i64>(6)? as u64,
        received_at: r.get(7)?,
        run_id: r.get(8)?,
    })
}

impl Store {
    /// Keep `d` under `source_id` as it is; returns its id.
    pub fn add_source_data(&self, source_id: &str, d: &NewSourceData) -> Result<String> {
        let id = super::new_id();
        self.conn.execute(
            "INSERT INTO source_data(id, source_id, locator, name, format, content_hash, size, data, received_at, run_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, source_id, d.locator, d.name, d.format, d.content_hash, d.data.len() as i64, d.data, now_ms(), d.run_id],
        )?;
        Ok(id)
    }

    /// The item of this source with these exact bytes, if one is stored.
    pub fn find_source_data(&self, source_id: &str, content_hash: &str) -> Result<Option<SourceData>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM source_data WHERE source_id = ?1 AND content_hash = ?2 ORDER BY received_at LIMIT 1"),
                params![source_id, content_hash],
                row,
            )
            .optional()?)
    }

    /// A source's items, oldest first (no bytes).
    pub fn source_data(&self, source_id: &str) -> Result<Vec<SourceData>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM source_data WHERE source_id = ?1 ORDER BY received_at, rowid"))?;
        Ok(st.query_map([source_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// One item's bytes.
    pub fn source_data_bytes(&self, id: &str) -> Result<Vec<u8>> {
        Ok(self.conn.query_row("SELECT data FROM source_data WHERE id = ?1", [id], |r| r.get(0))?)
    }

    /// Remove one item; true when it existed.
    pub fn remove_source_data(&self, id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM source_data WHERE id = ?1", [id])? > 0)
    }

    /// (items, bytes) stored for a source.
    pub fn source_data_totals(&self, source_id: &str) -> Result<(u64, u64)> {
        Ok(self.conn.query_row("SELECT COUNT(*), COALESCE(SUM(size), 0) FROM source_data WHERE source_id = ?1", [source_id], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64))
        })?)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::Store;

    #[test]
    fn stores_bytes_as_they_are() {
        let s = Store::open_in_memory().unwrap();
        let src = s.add_source("local", "directory", Some("/d")).unwrap();
        let bytes = b"anything,at all\n\x00\xffnot even text";
        let n = super::NewSourceData { locator: "/d/x.bin", name: "x.bin", format: "", content_hash: "h1", data: bytes, run_id: None };
        let id = s.add_source_data(&src.id, &n).unwrap();
        assert_eq!(s.source_data_bytes(&id).unwrap(), bytes);
        let items = s.source_data(&src.id).unwrap();
        assert_eq!((items.len(), items[0].size, items[0].name.as_str()), (1, bytes.len() as u64, "x.bin"));
        assert_eq!(s.find_source_data(&src.id, "h1").unwrap().unwrap().id, id);
        assert!(s.find_source_data(&src.id, "h2").unwrap().is_none());
        assert_eq!(s.source_data_totals(&src.id).unwrap(), (1, bytes.len() as u64));
        assert!(s.remove_source_data(&id).unwrap());
        assert_eq!(s.source_data_totals(&src.id).unwrap(), (0, 0));
    }
}
