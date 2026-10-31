//! HTTP source with default User-Agent, retries, rate limiting and pagination.
use crate::error::{Error, Result};
use crate::ingest::frame::Columns;
use crate::ingest::json::{parse_records, records_to_frame, select};
use crate::ingest::time::parse_ts;
use crate::ingest::{Input, hash_bytes};
use polars::prelude::*;
use serde_json::Value;
use std::time::Duration as StdDuration;

pub fn user_agent() -> String {
    format!("muninn/{} (+https://github.com/WeirdBrains/muninn)", env!("CARGO_PKG_VERSION"))
}

pub const ACCEPT: &str = "application/json, text/csv;q=0.9";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Csv,
    Ndjson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paging {
    None,
    Cursor,
    Time,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub url: String,
    pub method: String,
    pub body: Option<String>,
    pub params: Vec<String>,
    pub headers: Vec<String>,
    pub format: Option<Format>,
    pub records: Option<String>,
    pub paging: Paging,
    pub cursor_path: String,
    pub cursor_param: String,
    pub start_param: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub max_pages: usize,
    pub rate_limit: Option<f64>,
    pub retries: u32,
    pub insecure: bool,
    pub timeout: u64,
}

pub fn method_allows_body(m: &str) -> bool {
    matches!(m.to_ascii_uppercase().as_str(), "POST" | "PUT" | "PATCH")
}

/// Parse `KEY=VALUE` params.
pub fn parse_params(items: &[String]) -> Result<Vec<(String, String)>> {
    items
        .iter()
        .map(|p| {
            p.split_once('=')
                .map(|(k, v)| (k.trim().to_string(), v.to_string()))
                .filter(|(k, _)| !k.is_empty())
                .ok_or_else(|| Error::invalid("--param", format!("`{p}` (expected KEY=VALUE)")))
        })
        .collect()
}

/// Default headers overridden (case-insensitively) by `KEY: VALUE` items.
pub fn build_headers(items: &[String]) -> Result<Vec<(String, String)>> {
    let mut h: Vec<(String, String)> = vec![("User-Agent".into(), user_agent()), ("Accept".into(), ACCEPT.into())];
    for it in items {
        let (k, v) = it
            .split_once(':')
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .filter(|(k, _)| !k.is_empty())
            .ok_or_else(|| Error::invalid("--header", format!("`{it}` (expected \"KEY: VALUE\")")))?;
        h.retain(|(e, _)| !e.eq_ignore_ascii_case(&k));
        h.push((k, v));
    }
    Ok(h)
}

pub fn build_url(base: &str, params: &[(String, String)]) -> Result<url::Url> {
    let mut u = url::Url::parse(base).map_err(|e| Error::invalid("--url", format!("`{base}`: {e}")))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(Error::invalid("--url", "only http and https are supported"));
    }
    {
        let mut q = u.query_pairs_mut();
        for (k, v) in params {
            q.append_pair(k, v);
        }
    }
    if u.query() == Some("") {
        u.set_query(None);
    }
    Ok(u)
}

impl HttpRequest {
    pub fn validate(&self) -> Result<()> {
        if self.body.is_some() && !method_allows_body(&self.method) {
            return Err(Error::invalid("--body", format!("not allowed with {} (use POST, PUT or PATCH)", self.method)));
        }
        parse_params(&self.params)?;
        build_headers(&self.headers)?;
        build_url(&self.url, &[])?;
        if self.paging == Paging::Time && self.from.is_none() {
            return Err(Error::missing("--from", "time pagination needs --from"));
        }
        Ok(())
    }

    fn body_bytes(&self) -> Result<Option<Vec<u8>>> {
        match &self.body {
            None => Ok(None),
            Some(b) => match b.strip_prefix('@') {
                Some(p) => Ok(Some(std::fs::read(p)?)),
                None => Ok(Some(b.clone().into_bytes())),
            },
        }
    }

    fn client(&self) -> Result<reqwest::blocking::Client> {
        if self.insecure {
            tracing::warn!("TLS verification disabled (--insecure)");
            eprintln!("warning: TLS certificate verification is disabled (--insecure)");
        }
        reqwest::blocking::Client::builder()
            .timeout(StdDuration::from_secs(self.timeout))
            .danger_accept_invalid_certs(self.insecure)
            .build()
            .map_err(|e| Error::Other(format!("http client: {e}")))
    }

    fn send(&self, client: &reqwest::blocking::Client, url: &url::Url) -> Result<(Option<String>, Vec<u8>)> {
        let method =
            reqwest::Method::from_bytes(self.method.to_ascii_uppercase().as_bytes()).map_err(|_| Error::invalid("--method", self.method.clone()))?;
        let headers = build_headers(&self.headers)?;
        let body = self.body_bytes()?;
        let mut attempt = 0;
        loop {
            let mut rb = client.request(method.clone(), url.clone());
            for (k, v) in &headers {
                rb = rb.header(k, v);
            }
            if let Some(b) = &body {
                rb = rb.body(b.clone());
            }
            let res = rb.send();
            let retry = match &res {
                Ok(r) => r.status().as_u16() == 429 || r.status().is_server_error(),
                Err(e) => e.is_timeout() || e.is_connect(),
            };
            if retry && attempt < self.retries {
                attempt += 1;
                std::thread::sleep(StdDuration::from_millis(200 * (1 << attempt.min(6))));
                continue;
            }
            let r = res.map_err(|e| Error::Other(format!("GET {url}: {e}")))?;
            let status = r.status();
            let ct = r.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
            let bytes = r.bytes().map_err(|e| Error::Other(format!("{url}: {e}")))?.to_vec();
            if !status.is_success() {
                let snippet = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_string();
                return Err(Error::Other(format!("{} {url} -> {status}: {snippet}", self.method)));
            }
            return Ok((ct, bytes));
        }
    }

    fn format_for(&self, ct: Option<&str>) -> Format {
        if let Some(f) = self.format {
            return f;
        }
        let ct = ct.unwrap_or("").to_ascii_lowercase();
        if ct.contains("csv") {
            Format::Csv
        } else if ct.contains("ndjson") || ct.contains("jsonl") {
            Format::Ndjson
        } else {
            Format::Json
        }
    }

    /// Fetch all pages into one Input.
    pub fn fetch(&self) -> Result<Input> {
        self.validate()?;
        let client = self.client()?;
        let base_params = parse_params(&self.params)?;
        let mut json_records: Vec<Value> = vec![];
        let mut csv_frame: Option<DataFrame> = None;
        let mut all_bytes: Vec<u8> = vec![];
        let mut cursor: Option<String> = None;
        let mut start = self.from.as_deref().map(|s| parse_ts(s).ok_or_else(|| Error::invalid("--from", s.to_string()))).transpose()?;
        let to = self.to.as_deref().map(|s| parse_ts(s).ok_or_else(|| Error::invalid("--to", s.to_string()))).transpose()?;
        let interval = self.rate_limit.filter(|r| *r > 0.0).map(|r| StdDuration::from_secs_f64(1.0 / r));
        let mut first_url = None;
        for page in 0..self.max_pages.max(1) {
            let mut params = base_params.clone();
            if let Some(c) = &cursor {
                params.push((self.cursor_param.clone(), c.clone()));
            }
            if self.paging == Paging::Time
                && let Some(s) = start
            {
                params.retain(|(k, _)| k != &self.start_param);
                params.push((self.start_param.clone(), s.to_string()));
            }
            let url = build_url(&self.url, &params)?;
            if first_url.is_none() {
                first_url = Some(build_url(&self.url, &base_params)?);
            }
            if page > 0
                && let Some(iv) = interval
            {
                std::thread::sleep(iv);
            }
            let (ct, bytes) = self.send(&client, &url)?;
            all_bytes.extend_from_slice(&bytes);
            let fmt = self.format_for(ct.as_deref());
            let (n, page_frame) = match fmt {
                Format::Csv => {
                    let df = CsvReadOptions::default()
                        .with_has_header(true)
                        .with_infer_schema_length(Some(0))
                        .into_reader_with_file_handle(std::io::Cursor::new(bytes.clone()))
                        .finish()?;
                    let n = df.height();
                    match &mut csv_frame {
                        Some(acc) => {
                            acc.vstack_mut(&df)?;
                        }
                        None => csv_frame = Some(df.clone()),
                    }
                    (n, Some(df))
                }
                Format::Json | Format::Ndjson => {
                    let recs = parse_records(&bytes, fmt == Format::Ndjson, self.records.as_deref())?;
                    let n = recs.len();
                    let df = if n > 0 { Some(records_to_frame(&recs)?) } else { None };
                    json_records.extend(recs);
                    if self.paging == Paging::Cursor {
                        let doc: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                        cursor = select(&doc, Some(&self.cursor_path)).ok().and_then(|v| match v {
                            Value::String(s) if !s.is_empty() => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        });
                    }
                    (n, df)
                }
            };
            match self.paging {
                Paging::None => break,
                Paging::Cursor => {
                    if cursor.is_none() || n == 0 {
                        break;
                    }
                }
                Paging::Time => {
                    let Some(df) = page_frame else { break };
                    let cols = Columns::from_frame(&df, &Default::default())?;
                    let Some(max) = cols.valid_ts().into_iter().max() else { break };
                    if start.is_some_and(|s| max < s) || to.is_some_and(|t| max >= t) {
                        break;
                    }
                    start = Some(max + 1);
                }
            }
        }
        let mut frame = match csv_frame {
            Some(f) => f,
            None => records_to_frame(&json_records)?,
        };
        if let Some(t) = to {
            // Trim bars past --to when paging by time.
            if self.paging == Paging::Time {
                let cols = Columns::from_frame(&frame, &Default::default())?;
                let mask: BooleanChunked = cols.ts.iter().map(|v| Some(v.is_none_or(|x| x <= t))).collect();
                frame = frame.filter(&mask)?;
            }
        }
        let u = first_url.unwrap_or(build_url(&self.url, &[])?);
        let name_hint = u.path_segments().and_then(|mut s| s.next_back().map(str::to_string)).filter(|s| !s.is_empty());
        Ok(Input { locator: redact(&u), name_hint, content_hash: hash_bytes(&[u.as_str().as_bytes(), &all_bytes].concat()), frame })
    }
}

/// URL without credentials.
pub fn redact(u: &url::Url) -> String {
    let mut u = u.clone();
    let _ = u.set_password(None);
    let _ = u.set_username("");
    u.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_and_params() {
        let h = build_headers(&[]).unwrap();
        assert!(h.iter().any(|(k, v)| k == "User-Agent" && v.starts_with("muninn/")));
        let h = build_headers(&["user-agent: me".into(), "X-Key: a:b".into()]).unwrap();
        assert!(h.iter().any(|(k, v)| k == "user-agent" && v == "me"));
        assert!(!h.iter().any(|(k, _)| k == "User-Agent"));
        assert!(h.iter().any(|(k, v)| k == "X-Key" && v == "a:b"));
        assert!(build_headers(&["nocolon".into()]).is_err());
        let p = parse_params(&["symbol=BTC USDT".into(), "a=b=c".into()]).unwrap();
        let u = build_url("https://x.test/api?x=1", &p).unwrap();
        assert_eq!(u.as_str(), "https://x.test/api?x=1&symbol=BTC+USDT&a=b%3Dc");
        assert!(parse_params(&["=v".into()]).is_err());
        assert!(build_url("ftp://x", &[]).is_err());
        assert_eq!(redact(&url::Url::parse("https://u:p@h/x").unwrap()), "https://h/x");
    }

    #[test]
    fn body_only_with_body_methods() {
        let mut r = HttpRequest {
            url: "http://localhost/x".into(),
            method: "GET".into(),
            body: Some("{}".into()),
            params: vec![],
            headers: vec![],
            format: None,
            records: None,
            paging: Paging::None,
            cursor_path: "next".into(),
            cursor_param: "cursor".into(),
            start_param: "startTime".into(),
            from: None,
            to: None,
            max_pages: 10,
            rate_limit: None,
            retries: 0,
            insecure: false,
            timeout: 5,
        };
        assert!(r.validate().is_err());
        r.method = "POST".into();
        assert!(r.validate().is_ok());
        r.paging = Paging::Time;
        assert!(r.validate().is_err());
    }
}
