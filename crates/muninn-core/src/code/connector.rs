//! The ingest-connector host methods (SDK 1.3; see `docs/connectors.md`): `context`,
//! `store_raw`, `pipe`, `run_ruleset`, `fetch`, `state_get` and `state_set`.
//!
//! They answer only inside a connector run. Until the v2.26 scheduler, the one connector
//! run is the dry run (`mnnn spaces run NAME --connector-dry-run`): an in-memory context
//! that records `store_raw`, `pipe` and `run_ruleset` calls instead of storing, piping or
//! running rules. `fetch` is real in a dry run (a connector can't be tried without its
//! data) and `state_*` keep an in-memory map, seeded with `--state`.
use crate::error::{Error, Result};
use base64::Engine as _;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::time::Duration;

/// The methods this module answers.
pub const METHODS: &[&str] = &["context", "store_raw", "pipe", "run_ruleset", "fetch", "state_get", "state_set"];

/// HTTP timeout of `fetch`.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
/// Largest body `fetch` returns.
pub const FETCH_MAX: usize = 64 << 20;

/// One recorded call of a dry run.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Call {
    pub method: String,
    pub params: Value,
}

/// A dry run's context and what the connector did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DryRun {
    pub context: Value,
    pub calls: Vec<Call>,
    pub state: Map<String, Value>,
    pub stored: usize,
    pub bytes: usize,
}

impl DryRun {
    /// A dry-run context: no schedule or run, `source` named after the space unless given.
    pub fn new(space: &str, source: Option<String>, pipeline: Option<String>, ruleset: Option<String>, options: Map<String, Value>) -> Self {
        DryRun {
            context: json!({
                "schedule_id": null,
                "run_id": "dry-run",
                "ruleset_id": ruleset,
                "source": source.unwrap_or_else(|| space.to_string()),
                "pipeline": pipeline,
                "last_success_at": null,
                "options": options,
                "dry_run": true,
            }),
            ..Default::default()
        }
    }

    /// One line per recorded call (for the run log and the CLI).
    pub fn summary(&self) -> Vec<String> {
        self.calls
            .iter()
            .map(|c| match c.method.as_str() {
                "store_raw" => format!(
                    "store_raw {} ({}, {} bytes) — not stored",
                    c.params["name"].as_str().unwrap_or(""),
                    c.params["format"].as_str().unwrap_or(""),
                    c.params["bytes"]
                ),
                "pipe" => format!("pipe {} — not run", c.params["pipeline"].as_str().unwrap_or("(the schedule's pipeline)")),
                "run_ruleset" => format!("run_ruleset {} — not run", c.params["ruleset_id"].as_str().unwrap_or("(the schedule's)")),
                "fetch" => {
                    format!("fetch {} {} → {}", c.params["method"].as_str().unwrap_or(""), c.params["url"].as_str().unwrap_or(""), c.params["status"])
                }
                "state_set" => format!("state_set {} = {}", c.params["key"].as_str().unwrap_or(""), c.params["value"]),
                m => m.to_string(),
            })
            .collect()
    }
}

thread_local! {
    static DRY: RefCell<Option<DryRun>> = const { RefCell::new(None) };
}

/// Run `f` as a connector dry run; gives its result and what the connector did.
pub fn dry_run<T>(d: DryRun, f: impl FnOnce() -> T) -> (T, DryRun) {
    let prev = DRY.with(|x| x.borrow_mut().replace(d));
    struct Back(Option<DryRun>);
    impl Drop for Back {
        fn drop(&mut self) {
            let p = self.0.take();
            DRY.with(|x| *x.borrow_mut() = p);
        }
    }
    let back = Back(prev);
    let out = f();
    let d = DRY.with(|x| x.borrow_mut().take()).unwrap_or_default();
    drop(back);
    (out, d)
}

/// The connector run's options (its schedule's), when one is on.
pub fn options() -> Option<Map<String, Value>> {
    DRY.with(|x| x.borrow().as_ref().map(|d| d.context["options"].as_object().cloned().unwrap_or_default()))
}

/// Is a connector run on this thread?
pub fn active() -> bool {
    DRY.with(|x| x.borrow().is_some())
}

fn not_here(method: &str) -> Error {
    Error::invalid(
        "host method",
        format!(
            "`{method}` is an ingest-connector method: it runs from the scheduler (available in v2.26) or in a dry run, `mnnn spaces run NAME --connector-dry-run`"
        ),
    )
}

fn str_of(req: &Value, k: &str) -> Result<String> {
    req.get(k).and_then(Value::as_str).map(str::to_string).ok_or_else(|| Error::invalid("request", format!("`{k}` is required")))
}

/// Answer one connector method.
pub fn call(method: &str, req: &Value) -> Result<Value> {
    if !active() {
        return Err(not_here(method));
    }
    // fetch outside the borrow: it may take a while, and nothing else runs meanwhile anyway
    if method == "fetch" {
        let reply = fetch(req)?;
        record(
            "fetch",
            json!({ "method": req.get("method").and_then(Value::as_str).unwrap_or("GET").to_ascii_uppercase(), "url": req.get("url"), "status": reply["status"] }),
        );
        return Ok(reply);
    }
    DRY.with(|x| {
        let mut b = x.borrow_mut();
        let d = b.as_mut().expect("active");
        match method {
            "context" => Ok(d.context.clone()),
            "store_raw" => {
                let name = str_of(req, "name")?;
                let format = req.get("format").and_then(Value::as_str).unwrap_or("bytes").to_string();
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(req.get("bytes_base64").and_then(Value::as_str).unwrap_or(""))
                    .map_err(|e| Error::invalid("store_raw", format!("bytes: {e}")))?;
                d.stored += 1;
                d.bytes += bytes.len();
                let id = format!("dry-{}", d.stored);
                let preview: String = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).into_owned();
                d.calls.push(Call {
                    method: "store_raw".into(),
                    params: json!({ "name": name, "format": format, "bytes": bytes.len(), "id": id, "preview": preview }),
                });
                Ok(json!({ "id": id }))
            }
            "pipe" => {
                let pipeline = d.context["pipeline"].clone();
                d.calls.push(Call { method: "pipe".into(), params: json!({ "pipeline": pipeline, "items": d.stored }) });
                Ok(json!({ "inserted": 0, "sinks": [], "dry_run": true }))
            }
            "run_ruleset" => {
                let id = req.get("ruleset_id").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| d.context["ruleset_id"].clone());
                d.calls.push(Call {
                    method: "run_ruleset".into(),
                    params: json!({ "ruleset_id": id, "action": { "kind": "connector.run", "stored": d.stored, "bytes": d.bytes, "inserted": 0 } }),
                });
                Ok(json!({ "rules_run": 0, "errors": [], "dry_run": true }))
            }
            "state_get" => Ok(json!({ "value": d.state.get(&str_of(req, "key")?).cloned().unwrap_or(Value::Null) })),
            "state_set" => {
                let k = str_of(req, "key")?;
                let v = req.get("value").cloned().unwrap_or(Value::Null);
                if v.is_null() {
                    d.state.remove(&k);
                } else {
                    d.state.insert(k.clone(), v.clone());
                }
                d.calls.push(Call { method: "state_set".into(), params: json!({ "key": k, "value": v }) });
                Ok(json!({}))
            }
            m => Err(Error::invalid("host method", format!("`{m}` is not a connector method"))),
        }
    })
}

fn record(method: &str, params: Value) {
    DRY.with(|x| {
        if let Some(d) = x.borrow_mut().as_mut() {
            d.calls.push(Call { method: method.into(), params });
        }
    })
}

fn fetch(req: &Value) -> Result<Value> {
    let url = str_of(req, "url")?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("GET").to_ascii_uppercase();
    let m = reqwest::Method::from_bytes(method.as_bytes()).map_err(|_| Error::invalid("fetch", format!("method `{method}`")))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent(crate::ingest::service::http::user_agent())
        .build()
        .map_err(|e| Error::Other(format!("http client: {e}")))?;
    let mut rb = client.request(m, &url);
    if let Some(h) = req.get("headers").and_then(Value::as_object) {
        for (k, v) in h {
            rb = rb.header(k, v.as_str().unwrap_or_default());
        }
    }
    if let Some(b) = req.get("body_base64").and_then(Value::as_str) {
        rb = rb.body(base64::engine::general_purpose::STANDARD.decode(b).map_err(|e| Error::invalid("fetch", format!("body: {e}")))?);
    }
    let r = rb.send().map_err(|e| Error::Other(format!("fetch {url}: {e}")))?;
    let status = r.status().as_u16();
    let headers: Map<String, Value> = r.headers().iter().map(|(k, v)| (k.to_string(), json!(v.to_str().unwrap_or_default()))).collect();
    let body = r.bytes().map_err(|e| Error::Other(format!("fetch {url}: {e}")))?;
    if body.len() > FETCH_MAX {
        return Err(Error::invalid("fetch", format!("{url}: body over {} MB", FETCH_MAX >> 20)));
    }
    Ok(json!({ "status": status, "headers": headers, "body_base64": base64::engine::general_purpose::STANDARD.encode(&body) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_refuse_outside_a_connector_run() {
        let e = call("store_raw", &json!({ "name": "x" })).unwrap_err().to_string();
        assert!(e.contains("v2.26") && e.contains("--connector-dry-run"), "{e}");
    }

    #[test]
    fn a_dry_run_records_store_pipe_and_ruleset_calls() {
        let d = DryRun::new("feed", None, Some("p1".into()), Some("rs".into()), Map::new());
        let ((), d) = dry_run(d, || {
            assert_eq!(call("context", &Value::Null).unwrap()["source"], "feed");
            let b = base64::engine::general_purpose::STANDARD.encode(b"a,b\n1,2\n");
            assert_eq!(call("store_raw", &json!({ "name": "page1", "format": "csv", "bytes_base64": b })).unwrap()["id"], "dry-1");
            assert_eq!(call("pipe", &Value::Null).unwrap()["inserted"], 0);
            assert_eq!(call("run_ruleset", &json!({})).unwrap()["rules_run"], 0);
            call("state_set", &json!({ "key": "cursor", "value": 42 })).unwrap();
            assert_eq!(call("state_get", &json!({ "key": "cursor" })).unwrap()["value"], 42);
        });
        assert!(!active());
        assert_eq!((d.stored, d.bytes), (1, 8));
        let m: Vec<&str> = d.calls.iter().map(|c| c.method.as_str()).collect();
        assert_eq!(m, ["store_raw", "pipe", "run_ruleset", "state_set"]);
        assert_eq!(d.calls[2].params["ruleset_id"], "rs");
        assert!(d.summary()[0].contains("page1 (csv, 8 bytes)"), "{:?}", d.summary());
    }
}
