//! The host side of the plugin ABI: the table a plugin gets (`MnHost`) and the methods it
//! can call. Requests and replies are JSON; see `docs/code-spaces.md` for each method.
//!
//! Writes a plugin makes go through the store like any other front end (fields are checked
//! and converted, refs resolved) and are recorded in the history with origin `code`, after
//! the run's own `code.run` action. Bulk writes (`update_each`, `create_many`) are
//! all-or-nothing and one history entry each.
use super::abi::{CALL_ERROR, CALL_OK, MnBuf, MnHost, MnStr};
use super::view::ResultView;
use crate::designer::{self as d, Act};
use crate::error::{Error, Result};
use crate::store::{CodeSpace, Model, NewAction, RecordQuery, Store};
use serde_json::{Map, Value, json};
use std::ffi::c_void;

/// History origin of what a plugin does.
pub const ORIGIN: &str = "code";
/// Bars loads without a limit get this many.
pub const DEFAULT_BAR_LIMIT: usize = 100_000;
/// Log lines kept per run (each cut to 2000 characters).
pub const MAX_LOGS: usize = 1_000;
/// Previous built-in bar values kept in a history entry.
pub const BEFORE_KEPT: usize = 10_000;

/// The methods of this host (ABI v1).
pub const METHODS: &[&str] = &[
    "methods",
    "input",
    "input_request",
    "models",
    "load",
    "update",
    "update_where",
    "update_each",
    "create",
    "create_many",
    "model_create",
    "log",
    "result",
    "error",
];

/// The state of one run, behind the `ctx` pointer.
pub struct Run<'a> {
    pub store: &'a Store,
    pub space: &'a CodeSpace,
    pub logs: Vec<String>,
    pub result: Option<ResultView>,
    pub notes: Vec<String>,
    pub error: Option<String>,
    pub writes: usize,
    /// The frame piped in (else the space's default model query).
    pub input: Option<Value>,
}

impl<'a> Run<'a> {
    pub fn new(store: &'a Store, space: &'a CodeSpace) -> Self {
        Run { store, space, logs: vec![], result: None, notes: vec![], error: None, writes: 0, input: None }
    }

    /// The table to hand to `muninn_plugin_run` (valid while `self` is).
    pub fn table(&mut self) -> MnHost {
        MnHost { abi_version: super::abi::ABI_VERSION, ctx: self as *mut Run as *mut c_void, call: host_call, free: host_free }
    }

    fn note(&mut self, a: Act) {
        self.store.note_action(&NewAction::new(a.kind, ORIGIN, format!("{} (code space {})", a.summary, self.space.name)).params(a.params));
    }

    /// History of a bars update, with the built-in values before (for going back by hand).
    fn note_bars(&mut self, m: &Model, ids: &[String], v: &Map<String, Value>, before: &[Value]) {
        let n = ids.len();
        let mut params = json!({ "model": m.id, "name": m.name, "count": n, "records": ids.iter().take(100).collect::<Vec<_>>(), "values": v });
        if before.iter().any(|b| !b.is_null()) {
            params["before"] = json!(before.iter().take(BEFORE_KEPT).collect::<Vec<_>>());
        }
        let summary = if n == 1 { format!("update bars record {}", &ids[0][..8.min(ids[0].len())]) } else { format!("update {n} bars records") };
        self.note(Act { kind: "record.update", summary, params });
    }

    /// Handle one call.
    pub fn call(&mut self, method: &str, req: Value) -> Result<Value> {
        let s = self.store;
        match method {
            "methods" => Ok(json!(METHODS)),
            "input_request" => Ok(serde_json::to_value(&self.space.input)?),
            "input" => {
                if let Some(f) = &self.input {
                    return Ok(f.clone());
                }
                let i = &self.space.input;
                if i.is_empty() {
                    return Err(Error::invalid(
                        "input",
                        format!(
                            "code space {} has no input; pipe data in (`mnnn pipe SOURCE {}`) or give it a default with `mnnn code edit {} --input SOURCE`",
                            self.space.name, self.space.name, self.space.name
                        ),
                    ));
                }
                load(s, &serde_json::to_value(i)?)
            }
            "load" => load(s, &req),
            "models" => {
                let mut out = vec![];
                for m in s.models()? {
                    let fields: Vec<Value> =
                        crate::store::records::field_names(&m).iter().map(|f| json!({ "name": f, "type": type_name(s, &m, f) })).collect();
                    out.push(json!({ "name": m.name, "version": m.version, "builtin": m.builtin, "fields": fields }));
                }
                Ok(Value::Array(out))
            }
            "update" => {
                let m = model(s, &req)?;
                let id = str_of(&req, "id")?;
                let v = values(&req, "values")?;
                if m.is_bars() {
                    let (id, before) = super::bars::update_record(s, &m, &id, &v)?;
                    self.writes += 1;
                    self.note_bars(&m, std::slice::from_ref(&id), &v, &[before]);
                    return Ok(json!({ "id": id }));
                }
                let r = s.update_record(&m, &id, &v)?;
                self.writes += 1;
                self.note(d::act_record_update(&m, Some(&r), 1, &[], &v));
                Ok(json!({ "id": r.id }))
            }
            "update_where" => {
                let m = model(s, &req)?;
                let w = d::parse_where(&strings(&req, "where"))?;
                if w.is_empty() {
                    return Err(Error::invalid("update_where", "give at least one condition (use update_each to change records by id)"));
                }
                let v = values(&req, "values")?;
                if m.is_bars() {
                    let ids: Vec<String> =
                        s.records(&m, &RecordQuery { filter: w.clone(), ..Default::default() })?.into_iter().map(|r| r.id).collect();
                    let (ids, before) = s.code_atomic(|| {
                        let mut out = (vec![], vec![]);
                        for id in &ids {
                            let (id, b) = super::bars::update_record(s, &m, id, &v)?;
                            out.0.push(id);
                            out.1.push(b);
                        }
                        Ok(out)
                    })?;
                    self.writes += ids.len();
                    self.note_bars(&m, &ids, &v, &before);
                    return Ok(json!({ "count": ids.len() }));
                }
                let n = s.update_records(&m, &w, &v)?;
                self.writes += n;
                self.note(d::act_record_update(&m, None, n, &w, &v));
                Ok(json!({ "count": n }))
            }
            "update_each" => {
                let m = model(s, &req)?;
                let ups = req.get("updates").and_then(Value::as_array).cloned().unwrap_or_default();
                let mut before = vec![];
                let mut ids = vec![];
                let known: std::collections::HashSet<String> =
                    if m.is_bars() { Default::default() } else { s.records(&m, &RecordQuery::default())?.into_iter().map(|r| r.id).collect() };
                let n = s.code_atomic(|| {
                    for (i, u) in ups.iter().enumerate() {
                        let id = str_of(u, "id").map_err(|e| Error::invalid("update_each", format!("update {}: {e}", i + 1)))?;
                        let v = values(u, "values")?;
                        if m.is_bars() {
                            let (id, b) =
                                super::bars::update_record(s, &m, &id, &v).map_err(|e| Error::invalid("update_each", format!("bar {id}: {e}")))?;
                            ids.push(id);
                            before.push(b);
                        } else {
                            let id = if known.contains(&id) { id } else { s.find_record(&m, &id)?.id };
                            s.update_ids(&m, std::slice::from_ref(&id), &v)
                                .map_err(|e| Error::invalid("update_each", format!("record {id}: {e}")))?;
                            ids.push(id);
                        }
                    }
                    Ok(ups.len())
                })?;
                self.writes += n;
                let mut params = json!({ "model": m.id, "name": m.name, "count": n, "records": ids.iter().take(100).collect::<Vec<_>>() });
                if before.iter().any(|b| !b.is_null()) {
                    params["before"] = json!(before.iter().take(BEFORE_KEPT).collect::<Vec<_>>());
                }
                self.note(Act { kind: "record.update", summary: format!("update {n} {} record{}", m.name, if n == 1 { "" } else { "s" }), params });
                Ok(json!({ "count": n }))
            }
            "create" => {
                let m = model(s, &req)?;
                if m.is_bars() {
                    let id = super::bars::create_record(s, &m, &values(&req, "values")?)?;
                    self.writes += 1;
                    self.note(Act {
                        kind: "record.create",
                        summary: format!("create bars record {}", &id[..8]),
                        params: json!({ "model": m.id, "name": m.name, "record": id }),
                    });
                    return Ok(json!({ "id": id }));
                }
                let r = s.create_record(&m, &values(&req, "values")?)?;
                self.writes += 1;
                self.note(d::act_record_create(&m, &r));
                Ok(json!({ "id": r.id }))
            }
            "create_many" => {
                let m = model(s, &req)?;
                let recs = req.get("records").and_then(Value::as_array).cloned().unwrap_or_default();
                let ids = s.code_atomic(|| {
                    let mut ids = vec![];
                    for (i, r) in recs.iter().enumerate() {
                        let v = r.as_object().cloned().ok_or_else(|| Error::invalid("create_many", format!("record {} is not an object", i + 1)))?;
                        let id = if m.is_bars() { super::bars::create_record(s, &m, &v) } else { s.create_record(&m, &v).map(|r| r.id) };
                        ids.push(id.map_err(|e| Error::invalid("create_many", format!("record {}: {e}", i + 1)))?);
                    }
                    Ok(ids)
                })?;
                self.writes += ids.len();
                let n = ids.len();
                self.note(Act {
                    kind: "record.create",
                    summary: format!("create {n} {} record{}", m.name, if n == 1 { "" } else { "s" }),
                    params: json!({ "model": m.id, "name": m.name, "count": n, "records": ids.iter().take(100).collect::<Vec<_>>() }),
                });
                Ok(json!({ "ids": ids }))
            }
            "model_create" => {
                let name = str_of(&req, "name")?;
                if s.models()?.iter().any(|m| m.name == crate::store::models::slug(&name)) {
                    return Ok(json!({ "created": false }));
                }
                let fields = req.get("fields").and_then(Value::as_str).unwrap_or_default();
                let about = req.get("about").and_then(Value::as_str).unwrap_or_default();
                let (m, _) = s.create_model(&name, about, crate::store::Design::default(), false)?;
                let m = match d::parse_fields(s, &m, fields)
                    .and_then(|f| s.set_model_draft(&m.id, Some(crate::store::Design { fields: f, ..Default::default() })))
                {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = s.remove_model(&m.id, true);
                        return Err(e);
                    }
                };
                self.note(d::act_model_create(&m));
                let opts = crate::store::ApplyOpts { note: format!("code space {}", self.space.name), ..Default::default() };
                let (m, p) = s.apply_model(&m.id, &opts)?;
                self.note(d::act_model_apply(s, &m, &p, false));
                Ok(json!({ "created": true, "id": m.id }))
            }
            "log" => {
                let t = req.get("text").and_then(Value::as_str).unwrap_or_default();
                if self.logs.len() < MAX_LOGS {
                    self.logs.push(t.chars().take(2000).collect());
                }
                Ok(Value::Null)
            }
            "result" => {
                let (v, notes) = ResultView::from_plugin(req)?;
                self.result = Some(v);
                self.notes = notes;
                Ok(Value::Null)
            }
            "error" => {
                self.error = Some(req.get("message").and_then(Value::as_str).unwrap_or("error").chars().take(4000).collect());
                Ok(Value::Null)
            }
            m => Err(Error::invalid("host method", format!("`{m}` is not a method of this Muninn (methods: {})", METHODS.join(", ")))),
        }
    }
}

fn type_name(s: &Store, m: &Model, f: &str) -> String {
    match crate::store::records::field_type(m, f) {
        Some(t) => d::type_text(s, &t),
        None => "json".into(),
    }
}

fn str_of(req: &Value, k: &str) -> Result<String> {
    match req.get(k) {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.clone()),
        _ => Err(Error::invalid("request", format!("`{k}` is missing"))),
    }
}

fn strings(req: &Value, k: &str) -> Vec<String> {
    match req.get(k) {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => vec![],
    }
}

fn values(req: &Value, k: &str) -> Result<Map<String, Value>> {
    req.get(k).and_then(Value::as_object).cloned().ok_or_else(|| Error::invalid("request", format!("`{k}` must be an object of FIELD: VALUE")))
}

fn model(s: &Store, req: &Value) -> Result<Model> {
    s.find_model(&str_of(req, "model")?)
}

/// `load`: records as a frame `{model, fields, ids, columns, groups}`.
pub fn load(s: &Store, req: &Value) -> Result<Value> {
    let m = model(s, req)?;
    let filter = d::parse_where(&strings(req, "where"))?;
    let sort = d::parse_sort(&strings(req, "sort"));
    let group = strings(req, "group");
    let limit = req.get("limit").and_then(Value::as_u64).map(|n| n as usize).or(if m.is_bars() { Some(DEFAULT_BAR_LIMIT) } else { None });
    let text = req.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
    let qy = RecordQuery { filter, text, sort, limit, offset: 0 };
    let (records, groups) = if group.is_empty() {
        (s.records(&m, &qy)?, vec![])
    } else {
        let gs = s.group_records(&m, &RecordQuery { limit: None, ..qy.clone() }, &group, &[])?;
        let mut recs = vec![];
        let mut out = vec![];
        for g in gs {
            let room = limit.unwrap_or(usize::MAX).saturating_sub(recs.len());
            if room == 0 {
                break;
            }
            let n = g.records.len().min(room);
            out.push(json!({ "key": g.key, "start": recs.len(), "len": n }));
            recs.extend(g.records.into_iter().take(n));
        }
        (recs, out)
    };
    let names = crate::store::records::field_names(&m);
    let fields: Vec<Value> = names.iter().map(|f| json!({ "name": f, "type": type_name(s, &m, f) })).collect();
    let mut columns = Map::new();
    for f in &names {
        columns.insert(f.clone(), Value::Array(records.iter().map(|r| r.values.get(f).cloned().unwrap_or(Value::Null)).collect()));
    }
    for meta in ["_created_at", "_updated_at"] {
        columns.insert(meta.into(), Value::Array(records.iter().map(|r| r.get(meta).unwrap_or(Value::Null)).collect()));
    }
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    Ok(json!({ "model": m.name, "fields": fields, "ids": ids, "columns": columns, "groups": groups }))
}

fn reply(out: *mut MnBuf, s: String) {
    let b = s.into_bytes().into_boxed_slice();
    let len = b.len();
    let ptr = Box::into_raw(b) as *mut u8;
    // SAFETY: `out` is the plugin's buffer for this call.
    unsafe { *out = MnBuf { ptr, len, cap: len } };
}

/// `MnHost::call`.
unsafe extern "C" fn host_call(ctx: *mut c_void, method: MnStr, request: MnStr, out: *mut MnBuf) -> i32 {
    if ctx.is_null() || out.is_null() {
        return CALL_ERROR;
    }
    // SAFETY: the plugin passes back the ctx we gave it, and lends the strings for this call.
    let (run, method, request) = unsafe { (&mut *(ctx as *mut Run), method.as_bytes(), request.as_bytes()) };
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<String> {
        let method = std::str::from_utf8(method).map_err(|_| Error::invalid("host method", "not UTF-8"))?;
        let text = std::str::from_utf8(request).map_err(|_| Error::invalid("request", "not UTF-8"))?;
        let req: Value =
            if text.trim().is_empty() { Value::Null } else { serde_json::from_str(text).map_err(|e| Error::invalid("request", e.to_string()))? };
        Ok(run.call(method, req)?.to_string())
    }));
    match res {
        Ok(Ok(s)) => {
            reply(out, s);
            CALL_OK
        }
        Ok(Err(e)) => {
            reply(out, e.to_string());
            CALL_ERROR
        }
        Err(_) => {
            reply(out, "the host failed on this call".into());
            CALL_ERROR
        }
    }
}

/// `MnHost::free`.
unsafe extern "C" fn host_free(_ctx: *mut c_void, buf: MnBuf) {
    if !buf.ptr.is_null() {
        // SAFETY: made by `reply` from a boxed slice of exactly `len` bytes.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(buf.ptr, buf.len)) });
    }
}
