//! Postgres source: server-side cursor, rows fetched as JSON.
use super::{ident, sql_or_file};
use crate::error::{Error, Result};
use crate::ingest::json::records_to_frame;
use crate::ingest::{Input, hash_bytes};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct PgSource {
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub secure: bool,
    pub ca_cert: Option<PathBuf>,
    pub schema: String,
    pub table: Option<String>,
    pub query: Option<String>,
    pub batch: usize,
    pub timeout: u64,
}

fn kv_escape(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

impl PgSource {
    /// libpq-style connection string (contains the password: never log it).
    pub fn conn_string(&self) -> Result<String> {
        if let Some(u) = &self.url {
            if !(u.starts_with("postgres://") || u.starts_with("postgresql://")) {
                return Err(Error::invalid("--url", "must start with postgres:// or postgresql://"));
            }
            return Ok(u.clone());
        }
        let user = self.username.as_deref().ok_or_else(|| Error::missing("--username", "required unless --url"))?;
        let host = self.host.as_deref().ok_or_else(|| Error::missing("--host", "required unless --url"))?;
        let mut s = format!(
            "host={} port={} user={} dbname={} sslmode={} connect_timeout={}",
            kv_escape(host),
            self.port.unwrap_or(5432),
            kv_escape(user),
            kv_escape(self.database.as_deref().unwrap_or("postgres")),
            if self.secure { "require" } else { "prefer" },
            self.timeout
        );
        if let Some(p) = &self.password {
            s.push_str(&format!(" password={}", kv_escape(p)));
        }
        Ok(s)
    }

    pub fn sql(&self) -> Result<String> {
        match (&self.table, &self.query) {
            (Some(t), None) => Ok(format!("SELECT * FROM {}.{}", ident(&self.schema), ident(t))),
            (None, Some(q)) => Ok(sql_or_file(q)?.trim().trim_end_matches(';').to_string()),
            _ => Err(Error::invalid("postgres", "give exactly one of --table / --query")),
        }
    }

    pub fn host_label(&self) -> String {
        if let Some(u) = &self.url
            && let Ok(p) = url::Url::parse(u)
        {
            return p.host_str().unwrap_or("postgres").to_string();
        }
        self.host.clone().unwrap_or_else(|| "postgres".into())
    }

    fn tls(&self) -> Result<postgres_native_tls::MakeTlsConnector> {
        let mut b = native_tls::TlsConnector::builder();
        match &self.ca_cert {
            Some(p) => {
                let pem = std::fs::read(p)?;
                let cert = native_tls::Certificate::from_pem(&pem).map_err(|e| Error::invalid("--ca-cert", e.to_string()))?;
                b.add_root_certificate(cert);
            }
            // sslmode=require/prefer: encrypt without verifying the server (libpq semantics).
            None => {
                b.danger_accept_invalid_certs(true).danger_accept_invalid_hostnames(true);
            }
        }
        let c = b.build().map_err(|e| Error::Other(format!("tls: {e}")))?;
        Ok(postgres_native_tls::MakeTlsConnector::new(c))
    }

    pub fn read(&self) -> Result<Input> {
        let conn = self.conn_string()?;
        let sql = self.sql()?;
        let mut client = postgres::Client::connect(&conn, self.tls()?).map_err(|e| Error::Other(format!("postgres {}: {e}", self.host_label())))?;
        let mut tx = client.transaction().map_err(|e| Error::Other(format!("postgres: {e}")))?;
        tx.batch_execute(&format!("DECLARE muninn_cur NO SCROLL CURSOR FOR SELECT row_to_json(t)::text FROM ({sql}) t"))
            .map_err(|e| Error::Other(format!("postgres: {e}")))?;
        let mut records: Vec<Value> = vec![];
        let mut hasher = blake3::Hasher::new();
        loop {
            let rows = tx.query(&format!("FETCH {} FROM muninn_cur", self.batch.max(1)), &[]).map_err(|e| Error::Other(format!("postgres: {e}")))?;
            if rows.is_empty() {
                break;
            }
            for r in rows {
                let s: String = r.get(0);
                hasher.update(s.as_bytes());
                records.push(serde_json::from_str(&s)?);
            }
        }
        tx.commit().map_err(|e| Error::Other(format!("postgres: {e}")))?;
        Ok(Input {
            locator: format!(
                "postgres://{}/{}#{}",
                self.host_label(),
                self.database.as_deref().unwrap_or("postgres"),
                self.table.as_deref().unwrap_or("query")
            ),
            name_hint: self.table.clone(),
            content_hash: hash_bytes(format!("{sql}|{}", hasher.finalize().to_hex()).as_bytes()),
            frame: records_to_frame(&records)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_strings() {
        let p = PgSource {
            username: Some("u".into()),
            host: Some("h".into()),
            schema: "public".into(),
            table: Some("t".into()),
            timeout: 30,
            ..Default::default()
        };
        let s = p.conn_string().unwrap();
        assert!(s.contains("host='h' port=5432 user='u' dbname='postgres' sslmode=prefer"), "{s}");
        assert!(!s.contains("password"));
        let p2 = PgSource { secure: true, password: Some("it's".into()), port: Some(6543), database: Some("m".into()), ..p.clone() };
        let s = p2.conn_string().unwrap();
        assert!(s.contains("sslmode=require") && s.contains("port=6543") && s.contains("dbname='m'") && s.contains("password='it\\'s'"), "{s}");
        assert_eq!(p.sql().unwrap(), "SELECT * FROM \"public\".\"t\"");
        let p3 = PgSource { url: Some("postgres://u:p@db.example/x".into()), query: Some("select 1;".into()), ..Default::default() };
        assert_eq!(p3.sql().unwrap(), "select 1");
        assert_eq!(p3.host_label(), "db.example");
        assert!(PgSource { url: Some("mysql://x".into()), ..Default::default() }.conn_string().is_err());
        assert!(PgSource::default().conn_string().is_err());
    }

    /// Live test: `MUNINN_TEST_POSTGRES=postgres://user:pass@localhost/db cargo test -- --ignored postgres_live`.
    #[test]
    #[ignore]
    fn postgres_live() {
        let url = std::env::var("MUNINN_TEST_POSTGRES").expect("MUNINN_TEST_POSTGRES");
        let p = PgSource {
            url: Some(url),
            schema: "public".into(),
            query: Some("SELECT to_timestamp(1502942400 + g * 300) AS time, 1.0 AS open, 2.0 AS high, 0.5 AS low, 1.5 AS close, 3.0 AS volume FROM generate_series(0, 9) g".into()),
            batch: 3,
            timeout: 10,
            ..Default::default()
        };
        assert_eq!(p.read().unwrap().frame.height(), 10);
    }
}
