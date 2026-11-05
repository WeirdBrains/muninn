//! SQLite source (read-only).
use super::{ident, sql_or_file};
use crate::error::{Error, Result};
use crate::ingest::{Input, hash_bytes};
use rusqlite::types::ValueRef;
use std::path::Path;

pub struct SqliteSource<'a> {
    pub path: &'a Path,
    pub table: Option<&'a str>,
    pub query: Option<&'a str>,
    pub key: Option<&'a str>,
}

impl SqliteSource<'_> {
    pub fn sql(&self) -> Result<String> {
        match (self.table, self.query) {
            (Some(t), None) => Ok(format!("SELECT * FROM {}", ident(t))),
            (None, Some(q)) => sql_or_file(q),
            _ => Err(Error::invalid("sqlite", "give exactly one of --table / --query")),
        }
    }

    pub fn read(&self) -> Result<Input> {
        if !self.path.is_file() {
            return Err(Error::invalid("--path", format!("{} does not exist", self.path.display())));
        }
        let conn = crate::store::Store::open_readonly_raw(self.path, self.key)?;
        let sql = self.sql()?;
        let mut st = conn.prepare(&sql)?;
        let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        let mut records = vec![];
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            // each row as an object, values as SQLite has them (blobs as hex)
            let mut o = serde_json::Map::new();
            for (i, n) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    ValueRef::Null => serde_json::Value::Null,
                    ValueRef::Integer(x) => x.into(),
                    ValueRef::Real(x) => serde_json::Number::from_f64(x).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned().into(),
                    ValueRef::Blob(b) => hex::encode(b).into(),
                };
                o.insert(n.clone(), v);
            }
            records.push(serde_json::Value::Object(o));
        }
        let bytes = super::ndjson(&records)?;
        let canon = std::fs::canonicalize(self.path).unwrap_or_else(|_| self.path.to_path_buf());
        Ok(Input {
            locator: format!("sqlite://{}#{}", canon.display(), self.table.unwrap_or("query")),
            name: self
                .table
                .map(str::to_string)
                .unwrap_or_else(|| self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
            format: "ndjson".into(),
            content_hash: hash_bytes(format!("{}|{}|{}", canon.display(), sql, hash_bytes(&bytes)).as_bytes()),
            bytes,
        })
    }
}
