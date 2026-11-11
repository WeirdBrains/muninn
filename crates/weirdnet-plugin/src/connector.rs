//! The ingest-connector host methods (a code space of kind `ingest-connector`;
//! see `docs/connectors.md`). A connector gathers data, stores it raw
//! ([`Host::store_raw`]), pipes it into its model ([`Host::pipe`]) and runs its rule set
//! ([`Host::run_ruleset`]). Only main calls these, and only in a connector run: the
//! host's scheduler or a dry run, which records the calls instead of storing, piping or
//! running rules.
use crate::{Error, Host, Json, Result};

/// What this run is for (`host.context()`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Context {
    pub schedule_id: Option<String>,
    pub run_id: Option<String>,
    pub ruleset_id: Option<String>,
    /// The source raw items go into (its name).
    pub source: Option<String>,
    /// The pipeline [`Host::pipe`] runs (its name).
    pub pipeline: Option<String>,
    /// Epoch ms (UTC) of the schedule's last successful run.
    pub last_success_at: Option<i64>,
    /// The schedule's options, as text.
    pub options: Vec<(String, String)>,
    /// True in `--connector-dry-run`: nothing is stored, piped or run.
    pub dry_run: bool,
}

/// What [`Host::pipe`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Piped {
    pub inserted: usize,
    /// Each model sink: `{"model", "inserted"}`.
    pub sinks: Vec<Json>,
}

/// What [`Host::run_ruleset`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RulesetRun {
    pub rules_run: usize,
    pub errors: Vec<String>,
}

/// An HTTP request for [`Host::fetch`].
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub fn get(url: &str) -> Request {
        Request { method: "GET".into(), url: url.into(), headers: vec![], body: None }
    }
    pub fn post(url: &str, body: impl Into<Vec<u8>>) -> Request {
        Request { method: "POST".into(), url: url.into(), headers: vec![], body: Some(body.into()) }
    }
    pub fn header(mut self, k: &str, v: &str) -> Request {
        self.headers.push((k.into(), v.into()));
        self
    }
    fn to_json(&self) -> Json {
        let h = Json::Obj(self.headers.iter().map(|(k, v)| (k.clone(), Json::from(v.as_str()))).collect());
        let mut j = Json::obj().with("method", self.method.as_str()).with("url", self.url.as_str()).with("headers", h);
        if let Some(b) = &self.body {
            j = j.with("body_base64", b64_encode(b));
        }
        j
    }
}

/// A [`Host::fetch`] reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn json(&self) -> Result<Json> {
        Json::parse(&self.text()).map_err(|e| Error(format!("response is not JSON: {e}")))
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn s(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(|v| v.as_str().map(str::to_string))
}

fn n(j: &Json, k: &str) -> usize {
    j.get(k).and_then(Json::as_f64).unwrap_or(0.0) as usize
}

impl Host {
    /// The run's context: schedule, run, rule set, source, pipeline, last success, options.
    pub fn context(&self) -> Result<Context> {
        let j = self.call("context", &Json::Null)?;
        Ok(Context {
            schedule_id: s(&j, "schedule_id"),
            run_id: s(&j, "run_id"),
            ruleset_id: s(&j, "ruleset_id"),
            source: s(&j, "source"),
            pipeline: s(&j, "pipeline"),
            last_success_at: j.get("last_success_at").and_then(Json::as_i64),
            options: j
                .get("options")
                .and_then(Json::as_object)
                .unwrap_or(&[])
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
                .collect(),
            dry_run: j.get("dry_run").and_then(Json::as_bool).unwrap_or(false),
        })
    }

    /// The context as the host sent it.
    pub fn context_json(&self) -> Result<Json> {
        self.call("context", &Json::Null)
    }

    /// Store one raw item (kept byte for byte, never parsed) in the run's source; returns
    /// its id. `format` is a hint for the pipeline: `csv`, `json`, `ndjson`, `text`, `bytes`.
    pub fn store_raw(&self, name: &str, format: &str, bytes: &[u8]) -> Result<String> {
        let r = self.call("store_raw", &Json::obj().with("name", name).with("format", format).with("bytes_base64", b64_encode(bytes)))?;
        Ok(s(&r, "id").unwrap_or_default())
    }

    /// Run the schedule's pipeline over this run's raw items.
    pub fn pipe(&self) -> Result<Piped> {
        let r = self.call("pipe", &Json::Null)?;
        Ok(Piped { inserted: n(&r, "inserted"), sinks: r.get("sinks").and_then(Json::as_array).unwrap_or(&[]).to_vec() })
    }

    /// Record `connector.run` and run a rule set (`None`: the schedule's).
    pub fn run_ruleset(&self, ruleset_id: Option<&str>) -> Result<RulesetRun> {
        let req = match ruleset_id {
            Some(id) => Json::obj().with("ruleset_id", id),
            None => Json::obj(),
        };
        let r = self.call("run_ruleset", &req)?;
        Ok(RulesetRun {
            rules_run: n(&r, "rules_run"),
            errors: r
                .get("errors")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .map(|e| e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()))
                .collect(),
        })
    }

    /// HTTP through the host's client (its user agent, timeouts and retries).
    pub fn fetch(&self, req: &Request) -> Result<Response> {
        let r = self.call("fetch", &req.to_json())?;
        let body = match r.get("body_base64").and_then(Json::as_str) {
            Some(b) => b64_decode(b)?,
            None => Vec::new(),
        };
        Ok(Response {
            status: r.get("status").and_then(Json::as_f64).unwrap_or(0.0) as u16,
            headers: r
                .get("headers")
                .and_then(Json::as_object)
                .unwrap_or(&[])
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_default()))
                .collect(),
            body,
        })
    }

    /// A value of the schedule's small state (a cursor such as the last timestamp).
    pub fn state_get(&self, key: &str) -> Result<Option<Json>> {
        let r = self.call("state_get", &Json::obj().with("key", key))?;
        Ok(r.get("value").filter(|v| !v.is_null()).cloned())
    }

    /// Set (or with `Json::Null`, remove) a state value; kept only when the run succeeds.
    pub fn state_set(&self, key: &str, value: impl Into<Json>) -> Result<()> {
        self.call("state_set", &Json::obj().with("key", key).with("value", value.into())).map(|_| ())
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding (how bytes cross the JSON ABI).
pub fn b64_encode(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(B64[(v >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes [`b64_encode`]'s output (whitespace ignored).
pub fn b64_decode(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = B64.iter().position(|&x| x == c).ok_or_else(|| Error(format!("bad base64 byte `{}`", c as char)))? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_round_trips() {
        for s in ["", "f", "fo", "foo", "foob", "fooba", "foobar"] {
            assert_eq!(b64_decode(&b64_encode(s.as_bytes())).unwrap(), s.as_bytes());
        }
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(b64_decode(&b64_encode(&all)).unwrap(), all);
    }
}

#[cfg(test)]
mod macro_tests {
    use crate::prelude::*;
    fn run(_: &Host) -> Result<View> {
        Ok(View::text("t", "x"))
    }
    crate::plugin!(run, kind = "ingest-connector");

    #[test]
    fn info_names_the_kind_and_sdk() {
        let i = weirdnet_plugin_info();
        // SAFETY: the info string is static UTF-8
        let s = unsafe { std::str::from_utf8(std::slice::from_raw_parts(i.ptr, i.len)).unwrap() };
        assert_eq!(s, r#"{"sdk":"0.1.0","lang":"rust","kind":"ingest-connector"}"#);
        assert_eq!(crate::SDK_VERSION, "0.1.0");
    }
}
