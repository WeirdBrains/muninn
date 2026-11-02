//! Calling external MCP servers (`mnnn mcp add`): over stdio (the command is started for the
//! call, its credentials passed as environment variables) or HTTP (credentials as headers).
//! Credentials are never logged or put in error messages; the server's stderr is discarded.
//!
//! Approval: before a tool's first call it must be allowed (`mnnn mcp approve SERVER TOOL`,
//! or a yes when asked); rule sets never approve on their own.
use super::PROTOCOL;
use crate::error::{Error, Result};
use crate::store::{McpServer, NewAction, NewNotification, Store};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// How long one request may take.
pub const TIMEOUT: Duration = Duration::from_secs(30);

enum Link {
    Stdio { child: Child, stdin: ChildStdin, lines: mpsc::Receiver<String> },
    Http { url: String, headers: Vec<(String, String)>, session: Option<String>, http: reqwest::blocking::Client },
}

/// An initialized connection to one external server.
pub struct Conn {
    name: String,
    link: Link,
    next: i64,
    pub server_info: Value,
}

fn fail(server: &str, m: impl std::fmt::Display) -> Error {
    Error::Other(format!("MCP server {server}: {m}"))
}

impl Conn {
    pub fn open(s: &McpServer) -> Result<Conn> {
        let link = match s.transport.as_str() {
            "stdio" => {
                let mut child = Command::new(&s.command)
                    .args(&s.args)
                    .envs(&s.secrets.env)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| fail(&s.name, format!("could not start `{}`: {e}", s.command)))?;
                let stdin = child.stdin.take().ok_or_else(|| fail(&s.name, "no stdin"))?;
                let stdout = child.stdout.take().ok_or_else(|| fail(&s.name, "no stdout"))?;
                let (tx, lines) = mpsc::channel();
                std::thread::spawn(move || {
                    for l in BufReader::new(stdout).lines() {
                        let Ok(l) = l else { break };
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                });
                Link::Stdio { child, stdin, lines }
            }
            "http" => Link::Http {
                url: s.url.clone(),
                headers: s.secrets.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                session: None,
                http: reqwest::blocking::Client::builder().timeout(TIMEOUT).build().map_err(|e| fail(&s.name, e))?,
            },
            t => return Err(fail(&s.name, format!("unknown transport `{t}`"))),
        };
        let mut c = Conn { name: s.name.clone(), link, next: 1, server_info: Value::Null };
        let init = c.request(
            "initialize",
            json!({ "protocolVersion": PROTOCOL, "capabilities": {}, "clientInfo": { "name": "muninn", "version": env!("CARGO_PKG_VERSION") } }),
        )?;
        c.server_info = init.get("serverInfo").cloned().unwrap_or(Value::Null);
        c.notify("notifications/initialized")?;
        Ok(c)
    }

    fn send_line(&mut self, v: &Value) -> Result<()> {
        if let Link::Stdio { stdin, .. } = &mut self.link {
            writeln!(stdin, "{}", serde_json::to_string(v)?).map_err(|e| fail(&self.name, format!("it closed: {e}")))?;
            stdin.flush()?;
        }
        Ok(())
    }

    fn notify(&mut self, method: &str) -> Result<()> {
        let msg = json!({ "jsonrpc": "2.0", "method": method });
        match &mut self.link {
            Link::Stdio { .. } => self.send_line(&msg),
            Link::Http { .. } => self.post(&msg, None).map(|_| ()),
        }
    }

    /// POST one message; the response (JSON or an event stream) with `want` id.
    fn post(&mut self, msg: &Value, want: Option<i64>) -> Result<Option<Value>> {
        let name = self.name.clone();
        let Link::Http { url, headers, session, http } = &mut self.link else { return Ok(None) };
        let mut rq =
            http.post(url.as_str()).header("accept", "application/json, text/event-stream").header("mcp-protocol-version", PROTOCOL).json(msg);
        for (k, v) in headers.iter() {
            rq = rq.header(k.as_str(), v.as_str());
        }
        if let Some(sid) = session.as_ref() {
            rq = rq.header("mcp-session-id", sid.as_str());
        }
        // reqwest errors can carry the URL but never the headers
        let r = rq.send().map_err(|e| fail(&name, format!("could not reach it: {}", e.without_url())))?;
        if let Some(sid) = r.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *session = Some(sid.to_string());
        }
        let status = r.status();
        if status == reqwest::StatusCode::ACCEPTED || want.is_none() {
            return Ok(None);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(fail(&name, format!("refused ({status}): check its credentials (`mnnn mcp edit {name} --header …`)")));
        }
        if !status.is_success() {
            return Err(fail(&name, format!("answered {status}")));
        }
        let sse = r.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|c| c.starts_with("text/event-stream"));
        let text = r.text().map_err(|e| fail(&name, e.without_url()))?;
        let msgs: Vec<Value> = if sse {
            text.lines().filter_map(|l| l.strip_prefix("data:")).filter_map(|d| serde_json::from_str(d.trim()).ok()).collect()
        } else {
            match serde_json::from_str::<Value>(&text).map_err(|e| fail(&name, format!("not JSON: {e}")))? {
                Value::Array(a) => a,
                v => vec![v],
            }
        };
        Ok(msgs.into_iter().find(|m| m.get("id").and_then(Value::as_i64) == want))
    }

    /// One request; its result, or the server's error.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let resp = match &self.link {
            Link::Http { .. } => self.post(&msg, Some(id))?.ok_or_else(|| fail(&self.name, format!("no answer to {method}")))?,
            Link::Stdio { .. } => {
                self.send_line(&msg)?;
                let deadline = std::time::Instant::now() + TIMEOUT;
                loop {
                    let Link::Stdio { lines, .. } = &self.link else { unreachable!() };
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    let line = lines
                        .recv_timeout(left)
                        .map_err(|_| fail(&self.name, format!("no answer to {method} (it stopped or took over {}s)", TIMEOUT.as_secs())))?;
                    let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                    if v.get("id").and_then(Value::as_i64) == Some(id) && v.get("method").is_none() {
                        break v;
                    }
                    // a request from the server (ping, sampling, …): answer pings, refuse the rest
                    if let (Some(rid), Some(m)) = (v.get("id").cloned(), v.get("method").and_then(Value::as_str)) {
                        let a = if m == "ping" {
                            super::ok(&rid, json!({}))
                        } else {
                            super::err(&rid, super::METHOD_NOT_FOUND, "not supported by Muninn")
                        };
                        self.send_line(&a)?;
                    }
                }
            }
        };
        if let Some(e) = resp.get("error") {
            return Err(fail(&self.name, e["message"].as_str().unwrap_or("an error")));
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Its tools (`tools/list`, all pages).
    pub fn tools(&mut self) -> Result<Vec<Value>> {
        let mut out = vec![];
        let mut cursor: Option<String> = None;
        loop {
            let p = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let r = self.request("tools/list", p)?;
            out.extend(r["tools"].as_array().cloned().unwrap_or_default());
            match r["nextCursor"].as_str() {
                Some(c) if out.len() < 10_000 => cursor = Some(c.to_string()),
                _ => return Ok(out),
            }
        }
    }

    /// Call a tool; its result (`content`, maybe `structuredContent`, `isError`).
    pub fn call(&mut self, tool: &str, args: Value) -> Result<Value> {
        self.request("tools/call", json!({ "name": tool, "arguments": args }))
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if let Link::Stdio { child, .. } = &mut self.link {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Is the tool allowed? If not, `ask` may allow it (recorded as `mcp.approve`).
pub fn ensure_approved(store: &Store, s: &McpServer, tool: &str, origin: &str, ask: &mut dyn FnMut(&str) -> bool) -> Result<()> {
    if store.mcp_approved(&s.id, tool)? {
        return Ok(());
    }
    if ask(&format!("Allow Muninn to call tool `{tool}` of MCP server {}?", s.name)) {
        approve(store, s, tool, origin, true)?;
        return Ok(());
    }
    Err(Error::invalid("MCP tool", format!("{}/{tool} is not approved yet (`mnnn mcp approve {} {tool}`)", s.name, s.name)))
}

pub fn approve(store: &Store, s: &McpServer, tool: &str, origin: &str, on: bool) -> Result<()> {
    store.set_mcp_approved(&s.id, tool, on)?;
    let (k, w) = if on { ("mcp.approve", "approved") } else { ("mcp.unapprove", "withdrew approval for") };
    store.note_action(
        &NewAction::new(k, origin, format!("{w} MCP tool {}/{tool}", s.name)).params(json!({ "server": s.id, "name": s.name, "tool": tool })),
    );
    Ok(())
}

/// Call an external tool (once approved) and record the call (the tool and argument names,
/// never values or credentials). Tool errors come back as `Err`.
pub fn call(store: &Store, s: &McpServer, tool: &str, args: Value, origin: &str, ask: &mut dyn FnMut(&str) -> bool) -> Result<Value> {
    ensure_approved(store, s, tool, origin, ask)?;
    let keys: Vec<String> = args.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    let r = Conn::open(s).and_then(|mut c| c.call(tool, args));
    let (ok, why) = match &r {
        Ok(v) if v["isError"].as_bool() == Some(true) => (false, super::result_text(v)),
        Ok(_) => (true, String::new()),
        Err(e) => (false, e.to_string()),
    };
    store.note_action(
        &NewAction::new("mcp.call", origin, format!("call MCP tool {}/{tool}{}", s.name, if ok { "" } else { " (failed)" }))
            .params(json!({ "server": s.id, "name": s.name, "tool": tool, "args": keys, "ok": ok })),
    );
    let v = r?;
    if !ok {
        return Err(fail(&s.name, format!("{tool}: {why}")));
    }
    Ok(v)
}

/// For rule sets: never asks; the first time a tool is wanted, a notification says how to allow it.
pub fn call_for_rule(store: &Store, s: &McpServer, tool: &str, args: Value, rule: &str) -> Result<Value> {
    let mut ask = |_: &str| {
        let key = format!("mcp.asked.{}.{tool}", s.id);
        if store.setting(&key).ok().flatten().is_none() {
            let _ = store.set_setting(&key, Some("1"));
            let _ = crate::notify::add(
                store,
                crate::rules::ORIGIN,
                &NewNotification {
                    kind: "mcp.approve".into(),
                    level: "warn".into(),
                    title: format!("Rule {rule} wants to call MCP tool {}/{tool}", s.name),
                    body: format!("Allow it with `mnnn mcp approve {} {tool}` (or in the TUI: Net ▸ MCP servers…).", s.name),
                    link: Some("mcp:".into()),
                    action_id: None,
                },
            );
        }
        false
    };
    call(store, s, tool, args, crate::rules::ORIGIN, &mut ask)
}

/// Make the server's arguments from `k=v` pairs: JSON values where they parse, else strings.
pub fn args_from_pairs(pairs: &[(String, String)]) -> Value {
    let mut m = serde_json::Map::new();
    for (k, v) in pairs {
        let val = serde_json::from_str::<Value>(v).ok().filter(|x| !x.is_string()).unwrap_or_else(|| Value::String(v.clone()));
        m.insert(k.clone(), val);
    }
    Value::Object(m)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::{McpRole, Secrets};

    /// A stdio MCP server for tests: this Muninn itself (`mnnn mcp serve --client …`) is not
    /// available in unit tests, so a tiny shell script answers initialize/tools/list/tools/call.
    pub fn fake_server(dir: &std::path::Path) -> std::path::PathBuf {
        let p = dir.join("fake-mcp.sh");
        std::fs::write(
            &p,
            r#"#!/bin/sh
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"initialize"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"fake","version":"1"}}}\n' "$id" ;;
    *'"tools/list"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"tools":[{"name":"price","description":"a price","inputSchema":{"type":"object"}}]}}\n' "$id" ;;
    *'"tools/call"'*) printf '{"jsonrpc":"2.0","id":%s,"result":{"content":[{"type":"text","text":"{\\"price\\": 42, \\"key\\": \\"%s\\"}"}]}}\n' "$id" "$FAKE_KEY" ;;
  esac
done
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    #[cfg(unix)]
    #[test]
    fn stdio_server_tools_approval_and_calls() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        let mut sec = Secrets::default();
        sec.env.insert("FAKE_KEY".into(), "sk-123".into());
        let srv = s.add_mcp_server("fake", "stdio", &fake_server(dir.path()).to_string_lossy(), &[], "", &sec).unwrap();
        let mut c = Conn::open(&srv).unwrap();
        assert_eq!(c.server_info["name"], "fake");
        assert_eq!(c.tools().unwrap()[0]["name"], "price");
        drop(c);
        // not approved: refused without asking yes
        let e = call(&s, &srv, "price", json!({}), "cli", &mut |_| false).unwrap_err().to_string();
        assert!(e.contains("mnnn mcp approve fake price"), "{e}");
        // asked and allowed: called, the credential reached the server, and nothing logs it
        let v = call(&s, &srv, "price", json!({ "symbol": "BTC" }), "cli", &mut |_| true).unwrap();
        assert_eq!(super::super::result_value(&v)["price"], 42);
        assert_eq!(super::super::result_value(&v)["key"], "sk-123");
        let calls = s.actions_of_kind("mcp.call").unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].params["args"], json!(["symbol"]));
        let all = serde_json::to_string(&s.actions().unwrap().iter().map(|a| (&a.summary, &a.params)).collect::<Vec<_>>()).unwrap();
        assert!(!all.contains("sk-123"));
        // rules never approve; they notify once
        s.set_mcp_approved(&srv.id, "price", false).unwrap();
        assert!(call_for_rule(&s, &srv, "price", json!({}), "r").is_err());
        assert!(call_for_rule(&s, &srv, "price", json!({}), "r").is_err());
        assert_eq!(s.notifications(false, 10).unwrap().iter().filter(|n| n.kind == "mcp.approve").count(), 1);
        let _ = McpRole::Read;
    }

    #[test]
    fn http_server_round_trip() {
        // Muninn's own HTTP server, called through an external-server entry
        let s = Store::open_in_memory().unwrap();
        let (_, token) = s.add_mcp_client("self", McpRole::Read).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("o.database");
        let other = Store::open(&db, None).unwrap();
        let (_, t2) = other.add_mcp_client("caller", McpRole::Read).unwrap();
        other.create_journal("Remote", None, crate::store::EntryFormat::Markdown).unwrap();
        let mut h = super::super::serve::Http::start(other, "127.0.0.1:0").unwrap();
        let mut sec = Secrets::default();
        sec.headers.insert("Authorization".into(), format!("Bearer {t2}"));
        let srv = s.add_mcp_server("other", "http", "", &[], &h.url(), &sec).unwrap();
        let mut c = Conn::open(&srv).unwrap();
        assert_eq!(c.server_info["name"], "muninn");
        assert!(c.tools().unwrap().iter().any(|t| t["name"] == "list"));
        let v = c.call("list", json!({ "what": "journals" })).unwrap();
        assert!(super::super::result_text(&v).contains("Remote"));
        // a wrong token is refused
        let mut bad = srv.clone();
        bad.secrets.headers.insert("Authorization".into(), format!("Bearer {token}x"));
        let e = Conn::open(&bad).err().unwrap().to_string();
        assert!(e.contains("refused") && !e.contains(&token), "{e}");
        h.halt();
    }
}
