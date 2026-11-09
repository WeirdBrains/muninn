//! The ingest-connector host methods (SDK 1.3; see `docs/connectors.md`): `context`,
//! `store_raw`, `pipe`, `run_ruleset`, `fetch`, `state_get` and `state_set`.
//!
//! They answer only inside a connector run: a dry run (`mnnn spaces run NAME
//! --connector-dry-run`) or a live schedule run (v2.26). Main only (not members).
use crate::error::{Error, Result};
use crate::store::{NewAction, NewSourceData, Store};
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

/// Live schedule run state (counts go back onto `schedule_runs`).
#[derive(Debug, Clone)]
pub struct Live {
    pub schedule_id: String,
    pub schedule_name: String,
    pub run_id: i64,
    pub plugin_id: String,
    pub source_id: String,
    pub source_name: String,
    pub pipeline_id: Option<String>,
    pub ruleset_id: Option<String>,
    pub options: Map<String, Value>,
    pub last_success_at: Option<i64>,
    pub stored: i64,
    pub bytes: i64,
    pub inserted: i64,
    pub piped: i64,
    pub ruleset_ran: i64,
    pub warnings: Vec<String>,
    /// Per-schedule cursor (`state_*`), loaded at start and saved at end.
    pub state: Map<String, Value>,
}

impl Live {
    pub fn context_json(&self) -> Value {
        json!({
            "schedule_id": self.schedule_id,
            "run_id": self.run_id,
            "ruleset_id": self.ruleset_id,
            "source": self.source_name,
            "pipeline": self.pipeline_id,
            "last_success_at": self.last_success_at,
            "options": self.options,
            "dry_run": false,
        })
    }
}

thread_local! {
    static DRY: RefCell<Option<DryRun>> = const { RefCell::new(None) };
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
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

/// Run `f` as a live connector schedule run.
pub fn live_run<T>(live: Live, f: impl FnOnce() -> T) -> (T, Live) {
    let prev = LIVE.with(|x| x.borrow_mut().replace(live));
    struct Back(Option<Live>);
    impl Drop for Back {
        fn drop(&mut self) {
            let p = self.0.take();
            LIVE.with(|x| *x.borrow_mut() = p);
        }
    }
    let back = Back(prev);
    let out = f();
    let live = LIVE.with(|x| x.borrow_mut().take()).expect("live");
    drop(back);
    (out, live)
}

/// The connector run's options (its schedule's), when one is on.
pub fn options() -> Option<Map<String, Value>> {
    if let Some(o) = DRY.with(|x| x.borrow().as_ref().map(|d| d.context["options"].as_object().cloned().unwrap_or_default())) {
        return Some(o);
    }
    LIVE.with(|x| x.borrow().as_ref().map(|l| l.options.clone()))
}

/// Is a connector run on this thread?
pub fn active() -> bool {
    DRY.with(|x| x.borrow().is_some()) || LIVE.with(|x| x.borrow().is_some())
}

fn not_here(method: &str) -> Error {
    Error::invalid(
        "host method",
        format!("`{method}` is an ingest-connector method: it runs from a schedule or in a dry run, `mnnn spaces run NAME --connector-dry-run`"),
    )
}

fn require_main() -> Result<()> {
    use super::spaces;
    // members must not call connector methods (main-only)
    if spaces::connector_in_member() {
        return Err(Error::invalid("host method", "ingest-connector methods are for the space's main only".to_string()));
    }
    Ok(())
}

fn str_of(req: &Value, k: &str) -> Result<String> {
    req.get(k).and_then(Value::as_str).map(str::to_string).ok_or_else(|| Error::invalid("request", format!("`{k}` is required")))
}

/// Answer one connector method.
pub fn call(store: &Store, method: &str, req: &Value) -> Result<Value> {
    if !active() {
        return Err(not_here(method));
    }
    require_main()?;
    if method == "fetch" {
        let reply = fetch(req)?;
        record_fetch(req, &reply);
        return Ok(reply);
    }
    if DRY.with(|x| x.borrow().is_some()) {
        return dry_call(method, req);
    }
    live_call(store, method, req)
}

fn record_fetch(req: &Value, reply: &Value) {
    let params = json!({
        "method": req.get("method").and_then(Value::as_str).unwrap_or("GET").to_ascii_uppercase(),
        "url": req.get("url"),
        "status": reply["status"]
    });
    DRY.with(|x| {
        if let Some(d) = x.borrow_mut().as_mut() {
            d.calls.push(Call { method: "fetch".into(), params: params.clone() });
        }
    });
}

fn dry_call(method: &str, req: &Value) -> Result<Value> {
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

fn live_call(store: &Store, method: &str, req: &Value) -> Result<Value> {
    match method {
        "context" => LIVE.with(|x| Ok(x.borrow().as_ref().expect("live").context_json())),
        "store_raw" => {
            let name = str_of(req, "name")?;
            let format = req.get("format").and_then(Value::as_str).unwrap_or("bytes").to_string();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(req.get("bytes_base64").and_then(Value::as_str).unwrap_or(""))
                .map_err(|e| Error::invalid("store_raw", format!("bytes: {e}")))?;
            let (source_id, run_id) = LIVE.with(|x| {
                let l = x.borrow();
                let l = l.as_ref().expect("live");
                (l.source_id.clone(), l.run_id)
            });
            let hash = crate::ingest::hash_bytes(&bytes);
            let id = store.add_source_data(
                &source_id,
                &NewSourceData { locator: &format!("connector:{run_id}:{name}"), name: &name, format: &format, content_hash: &hash, data: &bytes, run_id: Some(run_id) },
            )?;
            LIVE.with(|x| {
                let mut b = x.borrow_mut();
                let l = b.as_mut().expect("live");
                l.stored += 1;
                l.bytes += bytes.len() as i64;
            });
            Ok(json!({ "id": id }))
        }
        "pipe" => live_pipe(store),
        "run_ruleset" => live_ruleset(store, req),
        "state_get" => LIVE.with(|x| {
            let l = x.borrow();
            let l = l.as_ref().expect("live");
            Ok(json!({ "value": l.state.get(&str_of(req, "key")?).cloned().unwrap_or(Value::Null) }))
        }),
        "state_set" => {
            let k = str_of(req, "key")?;
            let v = req.get("value").cloned().unwrap_or(Value::Null);
            LIVE.with(|x| {
                let mut b = x.borrow_mut();
                let l = b.as_mut().expect("live");
                if v.is_null() {
                    l.state.remove(&k);
                } else {
                    l.state.insert(k, v);
                }
            });
            Ok(json!({}))
        }
        m => Err(Error::invalid("host method", format!("`{m}` is not a connector method"))),
    }
}

fn live_pipe(store: &Store) -> Result<Value> {
    let (pipeline_id, source_name, run_id) = LIVE.with(|x| {
        let l = x.borrow();
        let l = l.as_ref().expect("live");
        (l.pipeline_id.clone(), l.source_name.clone(), l.run_id)
    });
    let Some(pid) = pipeline_id else {
        return Err(Error::invalid("pipe", "this schedule has no pipeline".to_string()));
    };
    let pipeline = store.pipeline(&pid)?.ok_or_else(|| Error::invalid("pipe", "its pipeline was removed".to_string()))?;
    let env = crate::code::CodeEnv::for_store(store)?;
    // only this run's raw items: build a frame and run the pipeline with a synthetic stdin
    let src = store.source_by_name(&source_name)?.ok_or_else(|| Error::invalid("pipe", format!("source `{source_name}` is gone")))?;
    let items = store.source_data_for_run(&src.id, run_id)?;
    let frame = crate::code::pipe::data_frame_items(store, &src.name, &items)?;
    let result = match crate::code::pipeline::run(store, &env, crate::schedule::ORIGIN, &pipeline, Some(frame), &mut |_| {}) {
        Ok(r) => {
            let inserted: usize = r.landed.iter().map(|(_, n)| *n).sum();
            let sinks: Vec<String> = r.landed.iter().map(|(m, n)| format!("{m}:{n}")).collect();
            if !r.ok() {
                let err = r.nodes.iter().find(|n| n.status != "ok").map(|n| n.error.clone().unwrap_or_else(|| n.status.clone())).unwrap_or_else(|| "pipeline failed".into());
                LIVE.with(|x| x.borrow_mut().as_mut().expect("live").warnings.push(format!("pipe: {err}")));
                return Err(Error::invalid("pipe", err));
            }
            LIVE.with(|x| {
                let mut b = x.borrow_mut();
                let l = b.as_mut().expect("live");
                l.piped += 1;
                l.inserted += inserted as i64;
            });
            json!({ "inserted": inserted, "sinks": sinks })
        }
        Err(e) => {
            LIVE.with(|x| x.borrow_mut().as_mut().expect("live").warnings.push(format!("pipe: {e}")));
            return Err(e);
        }
    };
    Ok(result)
}

fn live_ruleset(store: &Store, req: &Value) -> Result<Value> {
    let (ruleset_id, schedule_id, schedule_name, run_id, stored, bytes, inserted, plugin_id) = LIVE.with(|x| {
        let l = x.borrow();
        let l = l.as_ref().expect("live");
        (
            req.get("ruleset_id").and_then(Value::as_str).map(str::to_string).or_else(|| l.ruleset_id.clone()),
            l.schedule_id.clone(),
            l.schedule_name.clone(),
            l.run_id,
            l.stored,
            l.bytes,
            l.inserted,
            l.plugin_id.clone(),
        )
    });
    let Some(rid) = ruleset_id else {
        return Err(Error::invalid("run_ruleset", "this schedule has no rule set".to_string()));
    };
    let set = store.ruleset(&rid)?.ok_or_else(|| Error::invalid("run_ruleset", "its rule set was removed".to_string()))?;
    let params = json!({
        "schedule": schedule_id, "name": schedule_name, "run": run_id, "connector": plugin_id,
        "ruleset": set.id, "stored": stored, "bytes": bytes, "inserted": inserted
    });
    let aid = store.record_action(
        &NewAction::new("connector.run", crate::schedule::ORIGIN, format!("connector schedule {schedule_name} run #{run_id}")).params(params),
    )?;
    let a = store.action(aid)?.ok_or_else(|| Error::invalid("action", "gone".to_string()))?;
    let res = crate::schedule::run_ruleset(store, &set.id, &a)?;
    LIVE.with(|x| {
        let mut b = x.borrow_mut();
        let l = b.as_mut().expect("live");
        l.ruleset_ran += 1;
        if !res.failed.is_empty() {
            l.warnings.push(format!("run_ruleset failed: {}", res.failed.join(", ")));
        }
    });
    if !res.failed.is_empty() {
        return Err(Error::invalid("run_ruleset", format!("failed: {}", res.failed.join(", "))));
    }
    Ok(json!({ "rules_run": res.ran.len(), "errors": [] }))
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

/// Setting key for a schedule's connector state map.
pub fn state_key(schedule_id: &str) -> String {
    format!("schedule.state.{schedule_id}")
}

pub fn load_state(store: &Store, schedule_id: &str) -> Result<Map<String, Value>> {
    Ok(store
        .setting(&state_key(schedule_id))?
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default())
}

pub fn save_state(store: &Store, schedule_id: &str, state: &Map<String, Value>) -> Result<()> {
    store.set_setting(&state_key(schedule_id), Some(&serde_json::to_string(state)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_refuse_outside_a_connector_run() {
        let e = call(&Store::open_in_memory().unwrap(), "store_raw", &json!({ "name": "x" })).unwrap_err().to_string();
        assert!(e.contains("schedule") && e.contains("--connector-dry-run"), "{e}");
    }

    #[test]
    fn a_dry_run_records_store_pipe_and_ruleset_calls() {
        let d = DryRun::new("feed", None, Some("p1".into()), Some("rs".into()), Map::new());
        let ((), d) = dry_run(d, || {
            assert_eq!(call(&Store::open_in_memory().unwrap(), "context", &Value::Null).unwrap()["source"], "feed");
            let b = base64::engine::general_purpose::STANDARD.encode(b"a,b\n1,2\n");
            let st = Store::open_in_memory().unwrap();
            assert_eq!(call(&st, "store_raw", &json!({ "name": "page1", "format": "csv", "bytes_base64": b })).unwrap()["id"], "dry-1");
            assert_eq!(call(&st, "pipe", &Value::Null).unwrap()["inserted"], 0);
            assert_eq!(call(&st, "run_ruleset", &json!({})).unwrap()["rules_run"], 0);
            call(&st, "state_set", &json!({ "key": "cursor", "value": 42 })).unwrap();
            assert_eq!(call(&st, "state_get", &json!({ "key": "cursor" })).unwrap()["value"], 42);
        });
        assert!(!active());
        assert_eq!((d.stored, d.bytes), (1, 8));
        let m: Vec<&str> = d.calls.iter().map(|c| c.method.as_str()).collect();
        assert_eq!(m, ["store_raw", "pipe", "run_ruleset", "state_set"]);
        assert_eq!(d.calls[2].params["ruleset_id"], "rs");
        assert!(d.summary()[0].contains("page1 (csv, 8 bytes)"), "{:?}", d.summary());
    }
}
