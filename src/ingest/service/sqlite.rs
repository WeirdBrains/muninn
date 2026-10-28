//! SQLite source (read-only).
use super::{ident, sql_or_file};
use crate::error::{Error, Result};
use crate::ingest::{hash_bytes, Input};
use polars::prelude::*;
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
        let mut cols: Vec<Vec<Option<String>>> = vec![vec![]; names.len()];
        let mut rows = st.query([])?;
        let mut hasher = blake3::Hasher::new();
        while let Some(r) = rows.next()? {
            for (i, c) in cols.iter_mut().enumerate() {
                let v = match r.get_ref(i)? {
                    ValueRef::Null => None,
                    ValueRef::Integer(x) => Some(x.to_string()),
                    ValueRef::Real(x) => Some(x.to_string()),
                    ValueRef::Text(t) => Some(String::from_utf8_lossy(t).to_string()),
                    ValueRef::Blob(b) => Some(hex::encode(b)),
                };
                if let Some(s) = &v {
                    hasher.update(s.as_bytes());
                }
                hasher.update(b"\x1f");
                c.push(v);
            }
        }
        let columns: Vec<Column> = names.iter().zip(cols).map(|(n, c)| Column::new(n.as_str().into(), c)).collect();
        let frame = DataFrame::new_infer_height(columns)?;
        let canon = std::fs::canonicalize(self.path).unwrap_or_else(|_| self.path.to_path_buf());
        Ok(Input {
            locator: format!("sqlite://{}#{}", canon.display(), self.table.unwrap_or("query")),
            name_hint: Some(self.table.map(str::to_string).unwrap_or_else(|| self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
            content_hash: hash_bytes(format!("{}|{}|{}", canon.display(), sql, hasher.finalize().to_hex()).as_bytes()),
            frame,
        })
    }
}
