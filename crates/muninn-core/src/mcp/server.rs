//! The MCP server: answers JSON-RPC messages for one client. Transport-free (see
//! [`super::serve`]); every call re-reads the client so a revoke or role change takes
//! effect at once.
use super::{INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, ORIGIN, PROTOCOL, PROTOCOLS, UNAUTHORIZED, err, ok};
use crate::error::{Error, Result};
use crate::store::{McpClient, NewAction, RecordQuery, Store};
use serde_json::{Map, Value, json};

/// What a tool needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Read,
    Write,
    Run,
}

impl Need {
    pub fn as_str(self) -> &'static str {
        match self {
            Need::Read => "read",
            Need::Write => "write",
            Need::Run => "run",
        }
    }
}

pub struct Tool {
    pub name: &'static str,
    pub about: &'static str,
    pub need: Need,
    pub schema: fn() -> Value,
}

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

const LIST_WHAT: &[&str] =
    &["journals", "entries", "projects", "models", "records", "reports", "searches", "org_resources", "executables", "views", "history"];

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "list",
        about: "List items: journals, entries (of a journal), projects, models, records (of a model), reports, searches (live searches), org_resources and executables (of an org), views (code space results) or history (recent actions).",
        need: Need::Read,
        schema: || {
            obj(
                json!({
                    "what": { "type": "string", "enum": LIST_WHAT },
                    "of": { "type": "string", "description": "The journal, model or org (name or id) for entries, records, org_resources and executables" },
                    "limit": { "type": "integer", "minimum": 1, "description": "At most this many (default 100)" }
                }),
                &["what"],
            )
        },
    },
    Tool {
        name: "read",
        about: "Read one resource by its muninn:// URI (see resources/list), e.g. muninn://journal/ID.",
        need: Need::Read,
        schema: || obj(json!({ "uri": { "type": "string" } }), &["uri"]),
    },
    Tool {
        name: "write_entry",
        about: "Write a new entry in a journal.",
        need: Need::Write,
        schema: || obj(json!({ "journal": { "type": "string" }, "title": { "type": "string" }, "text": { "type": "string" } }), &["journal", "text"]),
    },
    Tool {
        name: "add_note",
        about: "Add a note to a journal.",
        need: Need::Write,
        schema: || obj(json!({ "journal": { "type": "string" }, "text": { "type": "string" } }), &["journal", "text"]),
    },
    Tool {
        name: "create_record",
        about: "Create a record in a model (values by field name).",
        need: Need::Write,
        schema: || obj(json!({ "model": { "type": "string" }, "values": { "type": "object" } }), &["model", "values"]),
    },
    Tool {
        name: "update_record",
        about: "Change fields of one record.",
        need: Need::Write,
        schema: || {
            obj(json!({ "model": { "type": "string" }, "id": { "type": "string" }, "values": { "type": "object" } }), &["model", "id", "values"])
        },
    },
    Tool {
        name: "delete_record",
        about: "Delete one record.",
        need: Need::Write,
        schema: || obj(json!({ "model": { "type": "string" }, "id": { "type": "string" } }), &["model", "id"]),
    },
    Tool {
        name: "run_search",
        about: "Run a live search on the data stored now and return its matches.",
        need: Need::Run,
        schema: || obj(json!({ "search": { "type": "string" }, "limit": { "type": "integer", "minimum": 1 } }), &["search"]),
    },
    Tool {
        name: "run_code",
        about: "Run an approved code space and return its result view.",
        need: Need::Run,
        schema: || obj(json!({ "space": { "type": "string" } }), &["space"]),
    },
    Tool {
        name: "run_executable",
        about: "Run an executable this node serves for an org (input: where, text, sort, group, limit).",
        need: Need::Run,
        schema: || {
            obj(json!({ "org": { "type": "string" }, "executable": { "type": "string" }, "input": { "type": "object" } }), &["org", "executable"])
        },
    },
];

pub fn allowed(c: &McpClient, need: Need) -> bool {
    match need {
        Need::Read => true,
        Need::Write => c.role.can_write(),
        Need::Run => c.role.can_run(),
    }
}

/// Answer one message (or a batch). `None` for notifications.
pub fn handle(store: &Store, client_id: &str, msg: &Value) -> Option<Value> {
    if let Some(batch) = msg.as_array() {
        let v: Vec<Value> = batch.iter().filter_map(|m| handle(store, client_id, m)).collect();
        return (!v.is_empty()).then(|| Value::Array(v));
    }
    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // a response to us (we send no requests) or junk
        return id.filter(|_| msg.get("result").is_none() && msg.get("error").is_none()).map(|i| err(&i, INVALID_REQUEST, "no method"));
    };
    let id = id?;
    let client = match store.mcp_clients().ok().and_then(|v| v.into_iter().find(|c| c.id == client_id)) {
        Some(c) if c.revoked_at.is_none() => c,
        _ => return Some(err(&id, UNAUTHORIZED, "this MCP client was revoked or removed")),
    };
    let _ = store.touch_mcp_client(&client.id);
    let p = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match answer(store, &client, method, &p) {
        Ok(v) => ok(&id, v),
        Err((code, m)) => err(&id, code, m),
    })
}

type Answer = std::result::Result<Value, (i64, String)>;

fn answer(store: &Store, c: &McpClient, method: &str, p: &Value) -> Answer {
    match method {
        "initialize" => {
            let want = p["protocolVersion"].as_str().unwrap_or(PROTOCOL);
            let v = if PROTOCOLS.contains(&want) { want } else { PROTOCOL };
            Ok(json!({
                "protocolVersion": v,
                "capabilities": { "tools": { "listChanged": false }, "resources": { "listChanged": false, "subscribe": false } },
                "serverInfo": { "name": "muninn", "title": "Muninn", "version": env!("CARGO_PKG_VERSION") },
                "instructions": format!(
                    "Muninn: journals, projects, models and records, reports, live searches and org executables. You are MCP client `{}` with role `{}`.",
                    c.name,
                    c.role.as_str()
                )
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": TOOLS.iter().filter(|t| allowed(c, t.need)).map(|t| json!({
                "name": t.name,
                "description": t.about,
                "inputSchema": (t.schema)(),
                "annotations": { "readOnlyHint": t.need == Need::Read, "destructiveHint": t.name.starts_with("delete") }
            })).collect::<Vec<_>>()
        })),
        "tools/call" => {
            let name = p["name"].as_str().ok_or((INVALID_PARAMS, "tools/call needs a name".to_string()))?;
            let args = p.get("arguments").cloned().unwrap_or(json!({}));
            let Some(t) = TOOLS.iter().find(|t| t.name == name) else { return Err((INVALID_PARAMS, format!("unknown tool `{name}`"))) };
            if !allowed(c, t.need) {
                denied(store, c, name);
                return Ok(tool_error(format!(
                    "client {} (role {}) may not {} (`{}` needs {})",
                    c.name,
                    c.role.as_str(),
                    t.about_verb(),
                    name,
                    t.need.as_str()
                )));
            }
            Ok(match call(store, c, name, &args) {
                Ok(v) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }], "structuredContent": wrap(v), "isError": false })
                }
                Err(e) => tool_error(e.to_string()),
            })
        }
        "resources/list" => Ok(json!({ "resources": resources(store).map_err(internal)? })),
        "resources/templates/list" => Ok(
            json!({ "resourceTemplates": TEMPLATES.iter().map(|(u, n, d)| json!({ "uriTemplate": u, "name": n, "description": d, "mimeType": "application/json" })).collect::<Vec<_>>() }),
        ),
        "resources/read" => {
            let uri = p["uri"].as_str().ok_or((INVALID_PARAMS, "resources/read needs a uri".to_string()))?;
            let v = read(store, uri).map_err(|e| (INVALID_PARAMS, e.to_string()))?;
            Ok(json!({ "contents": [{ "uri": uri, "mimeType": "application/json", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }] }))
        }
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "logging/setLevel" => Ok(json!({})),
        m => Err((METHOD_NOT_FOUND, format!("`{m}` is not supported"))),
    }
}

impl Tool {
    fn about_verb(&self) -> &'static str {
        match self.need {
            Need::Read => "read",
            Need::Write => "make changes",
            Need::Run => "run things",
        }
    }
}

fn internal(e: Error) -> (i64, String) {
    (-32603, e.to_string())
}

/// structuredContent must be an object.
fn wrap(v: Value) -> Value {
    if v.is_object() { v } else { json!({ "result": v }) }
}

fn tool_error(m: String) -> Value {
    json!({ "content": [{ "type": "text", "text": m }], "isError": true })
}

fn denied(store: &Store, c: &McpClient, tool: &str) {
    store.note_action(
        &NewAction::new("mcp.denied", ORIGIN, format!("MCP client {} refused {tool} (role {})", c.name, c.role.as_str()))
            .params(json!({ "client": c.name, "tool": tool, "role": c.role.as_str() })),
    );
}

fn record(store: &Store, c: &McpClient, kind: &str, summary: String, mut params: Value) {
    params["client"] = json!(c.name);
    store.note_action(&NewAction::new(kind, ORIGIN, format!("{summary} (MCP client {})", c.name)).params(params));
}

fn s<'a>(a: &'a Value, k: &str) -> Result<&'a str> {
    a.get(k).and_then(Value::as_str).filter(|v| !v.trim().is_empty()).ok_or_else(|| Error::invalid("arguments", format!("`{k}` is required")))
}

fn values(a: &Value) -> Result<Map<String, Value>> {
    a.get("values").and_then(Value::as_object).cloned().ok_or_else(|| Error::invalid("arguments", "`values`: an object of field values"))
}

fn limit(a: &Value) -> usize {
    a.get("limit").and_then(Value::as_u64).map(|n| n.max(1) as usize).unwrap_or(100)
}

fn call(store: &Store, c: &McpClient, name: &str, a: &Value) -> Result<Value> {
    match name {
        "list" => list(store, s(a, "what")?, a.get("of").and_then(Value::as_str), limit(a)),
        "read" => read(store, s(a, "uri")?),
        "write_entry" => {
            let j = store.find_journal(s(a, "journal")?)?;
            let e = store.add_entry(&j.id, a["title"].as_str().unwrap_or_default(), s(a, "text")?, None)?;
            record(
                store,
                c,
                "entry.add",
                format!("write entry {} in {}", e.heading(), j.name),
                json!({ "journal": j.id, "entry": e.id, "title": e.heading() }),
            );
            Ok(json!({ "entry": e.id, "journal": j.id, "uri": format!("muninn://journal/{}", j.id) }))
        }
        "add_note" => {
            let j = store.find_journal(s(a, "journal")?)?;
            let n = store.add_note(&j.id, None, None, s(a, "text")?)?;
            record(store, c, "note.add", format!("add note to {}", j.name), json!({ "journal": j.id, "entry": null, "note": n.id }));
            Ok(json!({ "note": n.id, "journal": j.id }))
        }
        "create_record" => {
            let m = store.find_model(s(a, "model")?)?;
            let r = store.create_record(&m, &values(a)?)?;
            let act = crate::designer::act_record_create(&m, &r);
            record(store, c, act.kind, act.summary, act.params);
            Ok(serde_json::to_value(&r)?)
        }
        "update_record" => {
            let m = store.find_model(s(a, "model")?)?;
            let v = values(a)?;
            let r = store.update_record(&m, s(a, "id")?, &v)?;
            let act = crate::designer::act_record_update(&m, Some(&r), 1, &[], &v);
            record(store, c, act.kind, act.summary, act.params);
            Ok(serde_json::to_value(&r)?)
        }
        "delete_record" => {
            let m = store.find_model(s(a, "model")?)?;
            let r = store.delete_record(&m, s(a, "id")?)?;
            let act = crate::designer::act_record_delete(&m, Some(&r), 1, &[]);
            record(store, c, act.kind, act.summary, act.params);
            Ok(json!({ "deleted": r.id }))
        }
        "run_search" => {
            let ls = store.find_live_search(s(a, "search")?)?;
            let found = crate::live::run(store, &ls.spec)?;
            record(
                store,
                c,
                "mcp.run",
                format!("run live search {}: {}", ls.name, crate::live::matches(found.len())),
                json!({ "search": ls.id, "matches": found.len() }),
            );
            let n = found.len();
            Ok(json!({ "search": ls.name, "matches": n, "found": found.into_iter().take(limit(a)).collect::<Vec<_>>() }))
        }
        "run_code" => {
            let sp = store.find_code_space(s(a, "space")?)?;
            if !crate::code::approved(&sp) {
                return Err(Error::invalid("run_code", format!("code space {} is not approved to run (`mnnn code approve {}`)", sp.name, sp.name)));
            }
            let env = crate::code::CodeEnv::for_store(store)?;
            let info = crate::code::run(store, &env, ORIGIN, &sp, &mut |_| {})?;
            record(store, c, "mcp.run", format!("run code space {}", sp.name), json!({ "code": sp.id, "status": info.status }));
            Ok(crate::code::run_json(&info))
        }
        "run_executable" => {
            let org = store.find_org(s(a, "org")?)?;
            let input = crate::org::exec::Input::from_json(a.get("input").unwrap_or(&json!({})))?;
            let who = crate::org::Actor::Owner;
            let v = crate::org::exec::run(store, ORIGIN, &who, &org, s(a, "executable")?, &input)?;
            record(
                store,
                c,
                "mcp.run",
                format!("run executable {} of {}", s(a, "executable")?, org.name),
                json!({ "org": org.id, "executable": s(a, "executable")? }),
            );
            Ok(v)
        }
        t => Err(Error::invalid("tool", format!("unknown tool `{t}`"))),
    }
}

fn take<T: serde::Serialize>(v: Vec<T>, n: usize) -> Result<Value> {
    Ok(serde_json::to_value(v.into_iter().take(n).collect::<Vec<_>>())?)
}

/// `list`: items of one kind.
pub fn list(store: &Store, what: &str, of: Option<&str>, n: usize) -> Result<Value> {
    let need = |w: &str| of.filter(|o| !o.trim().is_empty()).ok_or_else(|| Error::invalid("arguments", format!("`of`: the {w} to list from")));
    Ok(match what {
        "journals" => take(store.journals()?, n)?,
        "entries" => take(store.entries(&store.find_journal(need("journal")?)?.id)?, n)?,
        "projects" => take(store.projects()?, n)?,
        "models" => take(store.models()?, n)?,
        "records" => {
            let m = store.find_model(need("model")?)?;
            take(store.records(&m, &RecordQuery { limit: Some(n), ..Default::default() })?, n)?
        }
        "reports" => take(store.reports()?, n)?,
        "searches" => take(store.live_searches()?, n)?,
        "org_resources" => take(store.resources(&store.find_org(need("org")?)?.id)?, n)?,
        "executables" => {
            let org = store.find_org(need("org")?)?;
            take(crate::org::exec::executables(store, &org, &crate::org::Actor::Owner)?, n)?
        }
        "views" => take(views(store)?, n)?,
        "history" => {
            let mut v = store.actions()?;
            v.reverse();
            take(
                v.into_iter().map(|a| json!({ "id": a.id, "kind": a.kind, "summary": a.summary, "origin": a.origin, "at": a.created_at })).collect(),
                n,
            )?
        }
        w => return Err(Error::invalid("what", format!("`{w}`: {}", LIST_WHAT.join(", ")))),
    })
}

/// Code spaces with a result view (their last run).
fn views(store: &Store) -> Result<Vec<Value>> {
    Ok(store
        .code_spaces()?
        .into_iter()
        .filter_map(|c| {
            let r = c.last_run.as_ref()?;
            let title = crate::code::run_view(r).map(|v| v.title().to_string()).unwrap_or_default();
            Some(json!({ "code": c.id, "name": c.name, "title": title, "status": r.status, "at": r.at, "uri": format!("muninn://view/{}", c.id) }))
        })
        .collect())
}

const TEMPLATES: &[(&str, &str, &str)] = &[
    ("muninn://journal/{id}", "journal", "A journal with its entries"),
    ("muninn://project/{id}", "project", "A project with its items"),
    ("muninn://model/{id}", "model", "A model's design"),
    ("muninn://model/{id}/records", "records", "A model's records (first 500)"),
    ("muninn://report/{id}", "report", "A report with its sections"),
    ("muninn://search/{id}", "live search", "A live search's definition"),
    ("muninn://view/{id}", "view", "A code space's last result"),
    ("muninn://org/{id}/resources", "org resources", "What an org holds"),
    ("muninn://org/{id}/executables", "executables", "What this node serves for an org"),
];

fn res(uri: String, name: &str, kind: &str) -> Value {
    json!({ "uri": uri, "name": name, "title": format!("{kind}: {name}"), "mimeType": "application/json" })
}

/// Every concrete resource (journals, projects, models, reports, searches, views, orgs).
pub fn resources(store: &Store) -> Result<Vec<Value>> {
    let mut v = vec![];
    v.extend(store.journals()?.iter().map(|j| res(format!("muninn://journal/{}", j.id), &j.name, "journal")));
    v.extend(store.projects()?.iter().map(|p| res(format!("muninn://project/{}", p.id), &p.name, "project")));
    for m in store.models()? {
        v.push(res(format!("muninn://model/{}", m.id), &m.name, "model"));
        v.push(res(format!("muninn://model/{}/records", m.id), &m.name, "records"));
    }
    v.extend(store.reports()?.iter().map(|r| res(format!("muninn://report/{}", r.id), &r.name, "report")));
    v.extend(store.live_searches()?.iter().map(|s| res(format!("muninn://search/{}", s.id), &s.name, "live search")));
    v.extend(views(store)?.iter().map(|x| res(x["uri"].as_str().unwrap_or_default().into(), x["name"].as_str().unwrap_or_default(), "view")));
    for o in store.orgs()? {
        v.push(res(format!("muninn://org/{}/resources", o.id), &o.name, "org resources"));
        v.push(res(format!("muninn://org/{}/executables", o.id), &o.name, "executables"));
    }
    Ok(v)
}

/// One resource by URI.
pub fn read(store: &Store, uri: &str) -> Result<Value> {
    let rest = uri.strip_prefix("muninn://").ok_or_else(|| Error::invalid("uri", format!("`{uri}`: muninn://KIND/ID")))?;
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    let bad = || Error::invalid("uri", format!("`{uri}`: see resources/templates/list"));
    Ok(match parts.as_slice() {
        ["journals"] | ["projects"] | ["models"] | ["reports"] | ["searches"] | ["views"] => list(store, parts[0], None, 500)?,
        ["journal", k] => {
            let j = store.find_journal(k)?;
            json!({ "journal": j, "entries": store.entries(&j.id)? })
        }
        ["project", k] => {
            let p = store.find_project(k)?;
            json!({ "project": p, "items": store.project_items(&p.id)? })
        }
        ["model", k] => serde_json::to_value(store.find_model(k)?)?,
        ["model", k, "records"] => list(store, "records", Some(k), 500)?,
        ["report", k] => {
            let r = store.find_report(k)?;
            json!({ "report": r, "sections": store.sections(&r.id)? })
        }
        ["search", k] => serde_json::to_value(store.find_live_search(k)?)?,
        ["view", k] => {
            let c = store.find_code_space(k)?;
            match &c.last_run {
                Some(r) => json!({ "code": c.id, "name": c.name, "run": r }),
                None => return Err(Error::invalid("view", format!("{} has not run yet", c.name))),
            }
        }
        ["org", k, "resources"] => list(store, "org_resources", Some(k), 500)?,
        ["org", k, "executables"] => list(store, "executables", Some(k), 500)?,
        _ => return Err(bad()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{EntryFormat, McpRole};

    fn rpc(s: &Store, c: &str, method: &str, params: Value) -> Value {
        handle(s, c, &json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })).unwrap()
    }

    #[test]
    fn roles_gate_tools_and_everything_is_recorded() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("Lab", None, EntryFormat::Markdown).unwrap();
        let (reader, _) = s.add_mcp_client("reader", McpRole::Read).unwrap();
        let (writer, _) = s.add_mcp_client("writer", McpRole::Write).unwrap();
        let init = rpc(&s, &reader.id, "initialize", json!({ "protocolVersion": "2025-03-26" }));
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert!(handle(&s, &reader.id, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let tools = rpc(&s, &reader.id, "tools/list", json!({}));
        let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["list", "read"]);
        // the reader may list but not write; the refusal is recorded
        let l = rpc(&s, &reader.id, "tools/call", json!({ "name": "list", "arguments": { "what": "journals" } }));
        assert_eq!(l["result"]["isError"], false);
        assert!(super::super::result_text(&l["result"]).contains("Lab"));
        let w = rpc(&s, &reader.id, "tools/call", json!({ "name": "write_entry", "arguments": { "journal": "Lab", "text": "hi" } }));
        assert_eq!(w["result"]["isError"], true);
        assert_eq!(s.actions_of_kind("mcp.denied").unwrap().len(), 1);
        assert!(s.entries(&j.id).unwrap().is_empty());
        // the writer may, and the history says which client did it
        let w =
            rpc(&s, &writer.id, "tools/call", json!({ "name": "write_entry", "arguments": { "journal": "Lab", "title": "Day 1", "text": "hi" } }));
        assert_eq!(w["result"]["isError"], false, "{w}");
        let a = s.actions_of_kind("entry.add").unwrap();
        assert_eq!(a[0].origin, "mcp");
        assert_eq!(a[0].params["client"], "writer");
        // but may not run
        let r = rpc(&s, &writer.id, "tools/call", json!({ "name": "run_search", "arguments": { "search": "x" } }));
        assert_eq!(r["result"]["isError"], true);
        // resources
        let res = rpc(&s, &reader.id, "resources/list", json!({}));
        let uri = res["result"]["resources"][0]["uri"].as_str().unwrap().to_string();
        assert_eq!(uri, format!("muninn://journal/{}", j.id));
        let back = rpc(&s, &reader.id, "resources/read", json!({ "uri": uri }));
        assert!(back["result"]["contents"][0]["text"].as_str().unwrap().contains("Day 1"));
        assert_eq!(rpc(&s, &reader.id, "resources/read", json!({ "uri": "muninn://nope/1" }))["error"]["code"], INVALID_PARAMS);
        assert_eq!(rpc(&s, &reader.id, "nope", json!({}))["error"]["code"], METHOD_NOT_FOUND);
        // revoked: refused at once
        s.revoke_mcp_client(&writer.id, true).unwrap();
        assert_eq!(rpc(&s, &writer.id, "tools/list", json!({}))["error"]["code"], UNAUTHORIZED);
    }
}
