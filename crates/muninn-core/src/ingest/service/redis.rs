//! Redis source: streams, lists, sorted sets, hashes and strings.
use crate::error::{Error, Result};
use crate::ingest::frame::is_timestamp_alias;
use crate::ingest::json::{parse_records, records_to_frame};
use crate::ingest::{Input, hash_bytes};
use redis::Value as RV;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Auto,
    Stream,
    List,
    Zset,
    Hash,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueFormat {
    Json,
    Csv,
    Fields,
}

#[derive(Debug, Clone)]
pub struct RedisSource {
    pub uri: String,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub db: i64,
    pub tls: bool,
    pub key: Option<String>,
    pub pattern: Option<String>,
    pub kind: Kind,
    pub value_format: ValueFormat,
    pub from: Option<String>,
    pub to: Option<String>,
    pub batch: usize,
    pub timeout: u64,
}

impl RedisSource {
    /// Connection URL (contains the password: never log it; use `display_url`).
    pub fn url(&self) -> Result<url::Url> {
        let raw = if self.uri.contains("://") { self.uri.clone() } else { format!("redis://{}", self.uri) };
        let mut u = url::Url::parse(&raw).map_err(|e| Error::invalid("--uri", format!("{e}")))?;
        if !matches!(u.scheme(), "redis" | "rediss") {
            return Err(Error::invalid("--uri", "scheme must be redis:// or rediss://"));
        }
        if self.tls && u.scheme() == "redis" {
            let s = u.as_str().replacen("redis://", "rediss://", 1);
            u = url::Url::parse(&s).map_err(|e| Error::invalid("--uri", format!("{e}")))?;
        }
        if u.port().is_none() {
            let _ = u.set_port(Some(self.port.unwrap_or(6379)));
        }
        if let Some(user) = &self.username {
            let _ = u.set_username(user);
        }
        if let Some(p) = &self.password {
            let _ = u.set_password(Some(p));
        }
        if self.db != 0 || u.path().is_empty() || u.path() == "/" {
            u.set_path(&format!("/{}", self.db));
        }
        Ok(u)
    }

    pub fn display_url(&self) -> String {
        self.url().map(|u| crate::ingest::service::http::redact(&u)).unwrap_or_else(|_| self.uri.clone())
    }

    pub fn host(&self) -> String {
        self.url().ok().and_then(|u| u.host_str().map(str::to_string)).unwrap_or_else(|| "redis".into())
    }

    pub fn read(&self) -> Result<Vec<Input>> {
        let client = redis::Client::open(self.url()?.as_str()).map_err(|e| Error::Other(format!("redis {}: {e}", self.display_url())))?;
        let mut con = client
            .get_connection_with_timeout(std::time::Duration::from_secs(self.timeout))
            .map_err(|e| Error::Other(format!("redis {}: {e}", self.display_url())))?;
        let keys = match (&self.key, &self.pattern) {
            (Some(k), None) => vec![k.clone()],
            (None, Some(p)) => scan(&mut con, p, self.batch)?,
            _ => return Err(Error::invalid("redis", "give exactly one of --key / --pattern")),
        };
        let mut out = vec![];
        for key in keys {
            let kind = match self.kind {
                Kind::Auto => {
                    let t: String = q(&mut con, redis::cmd("TYPE").arg(&key))?;
                    parse_kind(&t).ok_or_else(|| Error::invalid("redis", format!("key `{key}` has unsupported type `{t}`")))?
                }
                k => k,
            };
            let records = self.records(&mut con, &key, kind)?;
            if records.is_empty() {
                continue;
            }
            let hash = hash_bytes(serde_json::to_string(&records)?.as_bytes());
            out.push(Input {
                locator: format!("{}#{key}", self.display_url()),
                name_hint: Some(key.clone()),
                content_hash: hash,
                frame: records_to_frame(&records)?,
            });
        }
        if out.is_empty() {
            return Err(Error::invalid("redis", "no records found"));
        }
        Ok(out)
    }

    fn records(&self, con: &mut redis::Connection, key: &str, kind: Kind) -> Result<Vec<Value>> {
        let vf = self.value_format;
        Ok(match kind {
            Kind::Stream => {
                let mut start = self.from.clone().unwrap_or_else(|| "-".into());
                let end = self.to.clone().unwrap_or_else(|| "+".into());
                let mut v = vec![];
                loop {
                    let reply: RV = q(con, redis::cmd("XRANGE").arg(key).arg(&start).arg(&end).arg("COUNT").arg(self.batch))?;
                    let entries = stream_entries(&reply);
                    let n = entries.len();
                    let last = entries.last().map(|(id, _)| id.clone());
                    for (id, fields) in entries {
                        v.push(stream_record(&id, &fields, vf)?);
                    }
                    match last {
                        Some(id) if n >= self.batch => start = format!("({id}"),
                        _ => break,
                    }
                }
                v
            }
            Kind::List => {
                let items: Vec<String> = q(con, redis::cmd("LRANGE").arg(key).arg(0).arg(-1))?;
                items.iter().map(|s| parse_value(s, vf)).collect::<Result<_>>()?
            }
            Kind::Zset => {
                let from = self.from.clone().unwrap_or_else(|| "-inf".into());
                let to = self.to.clone().unwrap_or_else(|| "+inf".into());
                let items: Vec<(String, f64)> = q(con, redis::cmd("ZRANGEBYSCORE").arg(key).arg(from).arg(to).arg("WITHSCORES"))?;
                items.iter().map(|(m, s)| Ok(with_ts(parse_value(m, vf)?, &format!("{}", *s as i64)))).collect::<Result<_>>()?
            }
            Kind::Hash => {
                let items: Vec<(String, String)> = q(con, redis::cmd("HGETALL").arg(key))?;
                items.iter().map(|(f, v)| Ok(with_ts(parse_value(v, vf)?, f))).collect::<Result<_>>()?
            }
            Kind::String => {
                let s: Option<Vec<u8>> = q(con, redis::cmd("GET").arg(key))?;
                let b = s.unwrap_or_default();
                parse_document(&b)?
            }
            Kind::Auto => unreachable!("resolved before"),
        })
    }
}

fn q<T: redis::FromRedisValue>(con: &mut redis::Connection, cmd: &mut redis::Cmd) -> Result<T> {
    cmd.query(con).map_err(|e| Error::Other(format!("redis: {e}")))
}

fn scan(con: &mut redis::Connection, pattern: &str, count: usize) -> Result<Vec<String>> {
    let mut cursor: u64 = 0;
    let mut keys = vec![];
    loop {
        let (next, batch): (u64, Vec<String>) = q(con, redis::cmd("SCAN").arg(cursor).arg("MATCH").arg(pattern).arg("COUNT").arg(count))?;
        keys.extend(batch);
        if next == 0 {
            break;
        }
        cursor = next;
    }
    keys.sort();
    keys.dedup();
    Ok(keys)
}

pub fn parse_kind(t: &str) -> Option<Kind> {
    match t {
        "stream" => Some(Kind::Stream),
        "list" => Some(Kind::List),
        "zset" => Some(Kind::Zset),
        "hash" => Some(Kind::Hash),
        "string" => Some(Kind::String),
        _ => None,
    }
}

fn rv_string(v: &RV) -> Option<String> {
    match v {
        RV::BulkString(b) => Some(String::from_utf8_lossy(b).to_string()),
        RV::SimpleString(s) => Some(s.clone()),
        RV::Int(i) => Some(i.to_string()),
        RV::Double(d) => Some(d.to_string()),
        RV::Okay => Some("OK".into()),
        _ => None,
    }
}

/// Parse an XRANGE reply into (id, [(field, value)]).
pub fn stream_entries(reply: &RV) -> Vec<(String, Vec<(String, String)>)> {
    let mut out = vec![];
    if let RV::Array(entries) = reply {
        for e in entries {
            if let RV::Array(parts) = e
                && parts.len() == 2
            {
                let id = rv_string(&parts[0]).unwrap_or_default();
                let mut fields = vec![];
                if let RV::Array(kv) = &parts[1] {
                    for pair in kv.chunks(2) {
                        if let [k, v] = pair {
                            fields.push((rv_string(k).unwrap_or_default(), rv_string(v).unwrap_or_default()));
                        }
                    }
                }
                out.push((id, fields));
            }
        }
    }
    out
}

/// Add a `timestamp` key when an object record has none.
fn with_ts(v: Value, ts: &str) -> Value {
    match v {
        Value::Object(mut m) => {
            if !m.keys().any(|k| is_timestamp_alias(k)) {
                m.insert("timestamp".into(), Value::String(ts.to_string()));
            }
            Value::Object(m)
        }
        other => other,
    }
}

/// One stream entry -> record.
pub fn stream_record(id: &str, fields: &[(String, String)], vf: ValueFormat) -> Result<Value> {
    let id_ms = id.split('-').next().unwrap_or(id).to_string();
    let v = match vf {
        ValueFormat::Fields => {
            let mut m = Map::new();
            for (k, v) in fields {
                m.insert(k.clone(), Value::String(v.clone()));
            }
            Value::Object(m)
        }
        _ => {
            let raw = fields
                .iter()
                .find(|(k, _)| matches!(k.as_str(), "json" | "csv" | "data" | "value" | "bar"))
                .or_else(|| fields.first())
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            parse_value(&raw, vf)?
        }
    };
    Ok(with_ts(v, &id_ms))
}

/// Decode one entry string.
pub fn parse_value(s: &str, vf: ValueFormat) -> Result<Value> {
    match vf {
        ValueFormat::Json | ValueFormat::Fields => serde_json::from_str(s).map_err(|e| Error::invalid("redis value", format!("`{s}`: {e}"))),
        ValueFormat::Csv => Ok(Value::Array(s.split(',').map(|x| Value::String(x.trim().to_string())).collect())),
    }
}

/// A whole document in a string key: JSON array, NDJSON, or CSV.
pub fn parse_document(b: &[u8]) -> Result<Vec<Value>> {
    if let Ok(v) = parse_records(b, false, None) {
        return Ok(v);
    }
    if let Ok(v) = parse_records(b, true, None) {
        return Ok(v);
    }
    let text = String::from_utf8_lossy(b);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<String> = lines.next().map(|h| h.split(',').map(|s| s.trim().to_string()).collect()).unwrap_or_default();
    Ok(lines
        .map(|l| {
            let mut m = Map::new();
            for (k, v) in header.iter().zip(l.split(',')) {
                m.insert(k.clone(), Value::String(v.trim().to_string()));
            }
            Value::Object(m)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src() -> RedisSource {
        RedisSource {
            uri: "localhost".into(),
            port: None,
            username: None,
            password: None,
            db: 0,
            tls: false,
            key: Some("k".into()),
            pattern: None,
            kind: Kind::Auto,
            value_format: ValueFormat::Json,
            from: None,
            to: None,
            batch: 1000,
            timeout: 5,
        }
    }

    #[test]
    fn urls() {
        assert_eq!(src().url().unwrap().as_str(), "redis://localhost:6379/0");
        let mut s = src();
        s.uri = "redis://h:7000/2".into();
        s.port = Some(1);
        assert_eq!(s.url().unwrap().as_str(), "redis://h:7000/2");
        let mut s = src();
        s.tls = true;
        s.username = Some("u".into());
        s.password = Some("p@ss".into());
        s.db = 3;
        let u = s.url().unwrap();
        assert_eq!(u.scheme(), "rediss");
        assert_eq!(u.path(), "/3");
        assert!(!s.display_url().contains("p%40ss"));
        let mut s = src();
        s.uri = "http://x".into();
        assert!(s.url().is_err());
    }

    #[test]
    fn stream_parsing() {
        let reply = RV::Array(vec![RV::Array(vec![
            RV::BulkString(b"1502942400000-0".to_vec()),
            RV::Array(vec![
                RV::BulkString(b"o".to_vec()),
                RV::BulkString(b"1".to_vec()),
                RV::BulkString(b"c".to_vec()),
                RV::BulkString(b"2".to_vec()),
            ]),
        ])]);
        let e = stream_entries(&reply);
        assert_eq!(e.len(), 1);
        let r = stream_record(&e[0].0, &e[0].1, ValueFormat::Fields).unwrap();
        assert_eq!(r["timestamp"], "1502942400000");
        assert_eq!(r["o"], "1");
        let r = stream_record("5-0", &[("json".into(), r#"{"time":1,"o":1}"#.into())], ValueFormat::Json).unwrap();
        assert!(r.get("timestamp").is_none());
        let r = stream_record("5-0", &[("csv".into(), "1,2,3,1,2,9".into())], ValueFormat::Csv).unwrap();
        assert_eq!(r.as_array().unwrap().len(), 6);
    }

    #[test]
    fn documents() {
        assert_eq!(parse_document(br#"[{"t":1}]"#).unwrap().len(), 1);
        assert_eq!(parse_document(b"{\"t\":1}\n{\"t\":2}").unwrap().len(), 2);
        let v = parse_document(b"time,open\n1,2\n3,4\n").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[1]["open"], "4");
        assert_eq!(parse_kind("zset"), Some(Kind::Zset));
        assert_eq!(parse_kind("set"), None);
    }

    /// Live test: `MUNINN_TEST_REDIS=redis://localhost:6379 cargo test -- --ignored redis_live`.
    #[test]
    #[ignore]
    fn redis_live() {
        let url = std::env::var("MUNINN_TEST_REDIS").expect("MUNINN_TEST_REDIS");
        let client = redis::Client::open(url.as_str()).unwrap();
        let mut con = client.get_connection().unwrap();
        let _: () = redis::cmd("DEL").arg("muninn:test:BTCUSDT_5m").query(&mut con).unwrap();
        for i in 0..5i64 {
            let _: String = redis::cmd("XADD")
                .arg("muninn:test:BTCUSDT_5m")
                .arg(format!("{}-0", 1502942400000 + i * 300_000))
                .arg("open")
                .arg("1")
                .arg("high")
                .arg("2")
                .arg("low")
                .arg("0.5")
                .arg("close")
                .arg("1.5")
                .arg("volume")
                .arg("3")
                .query(&mut con)
                .unwrap();
        }
        let mut s = src();
        s.uri = url;
        s.key = Some("muninn:test:BTCUSDT_5m".into());
        s.value_format = ValueFormat::Fields;
        let inputs = s.read().unwrap();
        assert_eq!(inputs[0].frame.height(), 5);
    }
}
