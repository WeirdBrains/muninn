//! MCP transports for `mnnn mcp serve`: stdio (newline-delimited JSON-RPC, one client named
//! at start) and HTTP (Streamable HTTP with JSON responses; each request carries a client
//! token as `Authorization: Bearer …`). Off until started.
use super::{PARSE_ERROR, UNAUTHORIZED, err, server};
use crate::error::{Error, Result};
use crate::store::{McpClient, NewAction, Store};
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::io::{BufRead, Write};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

/// Largest request body taken over HTTP.
const MAX_BODY: usize = 4 << 20;

fn started(store: &Store, c: &McpClient, how: &str) {
    store.note_action(
        &NewAction::new("mcp.serve", super::ORIGIN, format!("MCP server started for client {} over {how}", c.name))
            .params(json!({ "client": c.name, "transport": how })),
    );
}

/// The client to serve over stdio: it must exist and not be revoked.
pub fn stdio_client(store: &Store, name: &str) -> Result<McpClient> {
    let c = store.find_mcp_client(name)?;
    if c.revoked_at.is_some() {
        return Err(Error::invalid("MCP client", format!("{} is revoked (`mnnn mcp client restore {}`)", c.name, c.name)));
    }
    Ok(c)
}

/// Serve one client over a line stream until it closes.
pub fn stdio(store: &Store, c: &McpClient, input: impl BufRead, mut out: impl Write) -> Result<()> {
    started(store, c, "stdio");
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(m) => server::handle(store, &c.id, &m),
            Err(e) => Some(err(&Value::Null, PARSE_ERROR, format!("not JSON: {e}"))),
        };
        if let Some(r) = reply {
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
            out.flush()?;
        }
    }
    Ok(())
}

/// TLS for the HTTP server: a certificate from ACME (Let's Encrypt by default), or given.
#[derive(Clone)]
pub struct TlsSetup {
    /// Get and renew a certificate for `acme.domain`.
    pub acme: Option<crate::org::host::Acme>,
    /// A certificate chain and key of your own (no renewal).
    pub given: Option<crate::org::host::CertPem>,
    /// Where ACME HTTP-01 validations are answered (80; 0 picks one).
    pub http_port: u16,
    /// Check the certificate every minute and renew it when due.
    pub auto_renew: bool,
}

/// Where an ACME certificate for the MCP server is kept (separate from the org site's).
pub const CERT_SETTING: &str = "mcp.acme.cert";

/// A running HTTP MCP server.
pub struct Http {
    pub addr: SocketAddr,
    /// Where ACME HTTP-01 validations are answered (only with ACME over HTTP-01).
    pub http_addr: Option<SocketAddr>,
    tls: Option<Arc<crate::org::host::NodeTls>>,
    store: Arc<Mutex<Store>>,
    stop: watch::Sender<bool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Http {
    /// Listen on `bind` (`127.0.0.1:4743`; port 0 picks one), plain HTTP.
    pub fn start(store: Store, bind: &str) -> Result<Http> {
        Http::start_with(store, bind, None, None)
    }

    /// Listen on `bind`, over HTTPS when `tls` is given (`acme_http` is the ACME client; tests use a mock CA).
    pub fn start_with(store: Store, bind: &str, tls: Option<TlsSetup>, acme_http: Option<crate::org::host::AcmeHttp>) -> Result<Http> {
        let addr: SocketAddr = bind.parse().map_err(|_| Error::invalid("bind", format!("`{bind}`: HOST:PORT, e.g. 127.0.0.1:4743")))?;
        let node_tls = match &tls {
            Some(t) => Some(Arc::new(crate::org::host::NodeTls::new(
                &store,
                CERT_SETTING,
                "mcp.cert",
                t.acme.clone(),
                t.given.clone(),
                &["localhost".to_string(), "127.0.0.1".to_string()],
                acme_http,
            )?)),
            None => None,
        };
        let acceptor = node_tls.as_ref().map(|t| t.acceptor()).transpose()?;
        let std_listener = std::net::TcpListener::bind(addr)?;
        std_listener.set_nonblocking(true)?;
        let addr = std_listener.local_addr()?;
        let http_listener = match (&tls, &node_tls) {
            (Some(t), Some(n)) if n.wants_http01() => {
                let l = std::net::TcpListener::bind(SocketAddr::new(addr.ip(), t.http_port))?;
                l.set_nonblocking(true)?;
                Some(l)
            }
            _ => None,
        };
        let http_addr = http_listener.as_ref().map(|l| l.local_addr()).transpose()?;
        let scheme = if node_tls.is_some() { "https" } else { "http" };
        store.note_action(
            &NewAction::new("mcp.serve", super::ORIGIN, format!("MCP server listening on {scheme}://{addr}/mcp"))
                .params(json!({ "transport": scheme, "addr": addr.to_string() })),
        );
        // a browser page may come from the server's own public name
        let own: Arc<Vec<String>> = Arc::new(node_tls.as_ref().and_then(|t| t.acme().map(|a| a.domain.clone())).into_iter().collect());
        let (stop, mut rx) = watch::channel(false);
        let store = Arc::new(Mutex::new(store));
        let (st, nt, auto) = (store.clone(), node_tls.clone(), tls.as_ref().is_some_and(|t| t.auto_renew));
        let thread = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() else { return };
            let store = st;
            rt.block_on(async move {
                let Ok(listener) = tokio::net::TcpListener::from_std(std_listener) else { return };
                if let (Some(l), Some(t)) = (http_listener, &nt)
                    && let Ok(l) = tokio::net::TcpListener::from_std(l)
                {
                    tokio::spawn(crate::org::host::serve_challenges(l, t.challenges(), rx.clone()));
                }
                if let Some(t) = nt.clone().filter(|_| auto) {
                    tokio::spawn(renew_loop(t, store.clone(), rx.clone()));
                }
                loop {
                    tokio::select! {
                        _ = rx.changed() => break,
                        a = listener.accept() => {
                            let Ok((tcp, _)) = a else { continue };
                            let (store, own, acceptor) = (store.clone(), own.clone(), acceptor.clone());
                            tokio::spawn(async move {
                                let svc = hyper::service::service_fn(move |req| answer(store.clone(), own.clone(), req));
                                match acceptor {
                                    None => {
                                        let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tcp), svc).await;
                                    }
                                    Some(acc) => {
                                        let Ok(Ok(tls)) = tokio::time::timeout(std::time::Duration::from_secs(10), acc.accept(tcp)).await else { return };
                                        if crate::org::host::is_acme_validation(&tls) {
                                            return;
                                        }
                                        let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tls), svc).await;
                                    }
                                }
                            });
                        }
                    }
                }
            });
        });
        Ok(Http { addr, http_addr, tls: node_tls, store, stop, thread: Some(thread) })
    }

    pub fn url(&self) -> String {
        match self.tls.as_ref().and_then(|t| t.acme().map(|a| a.domain.clone())) {
            Some(d) if self.addr.port() == 443 => format!("https://{d}/mcp"),
            Some(d) => format!("https://{d}:{}/mcp", self.addr.port()),
            None if self.tls.is_some() => format!("https://{}/mcp", self.addr),
            None => format!("http://{}/mcp", self.addr),
        }
    }

    /// The served certificate (with TLS).
    pub fn cert(&self) -> Option<crate::org::host::CertInfo> {
        self.tls.as_ref().map(|t| t.cert())
    }

    /// Get a fresh ACME certificate now (with ACME).
    pub fn renew(&self) -> Result<Option<crate::org::host::CertInfo>> {
        let Some(t) = &self.tls else { return Ok(None) };
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        rt.block_on(t.renew_if_due(&self.store, true))
    }

    pub fn halt(&mut self) {
        let _ = self.stop.send(true);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Http {
    fn drop(&mut self) {
        self.halt();
    }
}

/// Renew the certificate when due, checking every minute (backing off to an hour after failures).
async fn renew_loop(tls: Arc<crate::org::host::NodeTls>, store: Arc<Mutex<Store>>, mut stop: watch::Receiver<bool>) {
    let mut wait = 60;
    loop {
        match tls.renew_if_due(&store, false).await {
            Ok(Some(c)) => {
                tracing::info!("MCP certificate renewed, valid until {}", c.not_after);
                wait = 60;
            }
            Ok(None) => wait = 60,
            Err(e) => {
                tracing::warn!("MCP certificate renewal failed: {e}");
                wait = (wait * 2).min(3600);
            }
        }
        tokio::select! {
            _ = stop.changed() => return,
            _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => {}
        }
    }
}

fn reply(status: u16, body: Option<Value>) -> Response<Full<Bytes>> {
    let b = Response::builder().status(status);
    match body {
        Some(v) => b.header("content-type", "application/json").body(Full::new(Bytes::from(serde_json::to_vec(&v).unwrap_or_default()))),
        None => b.body(Full::new(Bytes::new())),
    }
    .unwrap_or_default()
}

/// A browser page on another site must not reach a local server (DNS rebinding).
/// Local, or the server's own public name.
fn allowed_origin(o: &str, own: &[String]) -> bool {
    let rest = o.split_once("://").map_or(o, |(_, r)| r);
    let host = rest.split('/').next().unwrap_or_default();
    let host =
        if host.starts_with('[') { host.split(']').next().map(|h| &h[1..]).unwrap_or_default() } else { host.split(':').next().unwrap_or_default() };
    matches!(host, "localhost" | "127.0.0.1" | "::1") || own.iter().any(|d| d.eq_ignore_ascii_case(host))
}

async fn answer(store: Arc<Mutex<Store>>, own: Arc<Vec<String>>, req: Request<Incoming>) -> std::result::Result<Response<Full<Bytes>>, Infallible> {
    if req.uri().path() != "/mcp" {
        return Ok(reply(404, Some(json!({ "error": "not found: the MCP endpoint is /mcp" }))));
    }
    if let Some(o) = req.headers().get("origin").and_then(|v| v.to_str().ok())
        && !allowed_origin(o, &own)
    {
        return Ok(reply(403, Some(json!({ "error": "cross-origin requests are refused" }))));
    }
    match *req.method() {
        hyper::Method::POST => {}
        // no server-initiated stream; sessions are not kept
        hyper::Method::GET | hyper::Method::DELETE => return Ok(reply(405, None)),
        _ => return Ok(reply(405, None)),
    }
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
        .map(|t| t.trim().to_string());
    let body = match Limited::new(req.into_body(), MAX_BODY).collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => return Ok(reply(413, Some(err(&Value::Null, super::INVALID_REQUEST, "request too large")))),
    };
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return Ok(reply(400, Some(err(&Value::Null, PARSE_ERROR, format!("not JSON: {e}"))))),
    };
    let out = tokio::task::spawn_blocking(move || {
        let Ok(s) = store.lock() else { return (500, None) };
        let client = token.as_deref().and_then(|t| s.mcp_client_by_token(t).ok().flatten());
        let Some(c) = client.filter(|c| c.revoked_at.is_none()) else {
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            return (401, Some(err(&id, UNAUTHORIZED, "send a client token: Authorization: Bearer mnnn_mcp_… (`mnnn mcp client add`)")));
        };
        match server::handle(&s, &c.id, &msg) {
            Some(v) => (200, Some(v)),
            None => (202, None),
        }
    })
    .await
    .unwrap_or((500, None));
    Ok(reply(out.0, out.1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::McpRole;

    #[test]
    fn stdio_answers_lines() {
        let s = Store::open_in_memory().unwrap();
        let (c, _) = s.add_mcp_client("cli", McpRole::Read).unwrap();
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "not json\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n"
        );
        let mut out = vec![];
        stdio(&s, &stdio_client(&s, "cli").unwrap(), input.as_bytes(), &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["result"]["serverInfo"]["name"], "muninn");
        assert_eq!(lines[1]["error"]["code"], PARSE_ERROR);
        assert!(lines[2]["result"]["tools"].is_array());
        s.revoke_mcp_client(&c.id, true).unwrap();
        assert!(stdio_client(&s, "cli").is_err());
    }

    #[test]
    fn http_needs_a_live_token() {
        let s = Store::open_in_memory().unwrap();
        let (c, token) = s.add_mcp_client("web", McpRole::Read).unwrap();
        let mut h = Http::start(s, "127.0.0.1:0").unwrap();
        let http = reqwest::blocking::Client::new();
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let r = http.post(h.url()).json(&body).send().unwrap();
        assert_eq!(r.status(), 401);
        let r = http.post(h.url()).bearer_auth(&token).json(&body).send().unwrap();
        assert_eq!(r.status(), 200);
        assert!(r.json::<Value>().unwrap()["result"]["tools"].is_array());
        let r = http.post(h.url()).bearer_auth(&token).json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).send().unwrap();
        assert_eq!(r.status(), 202);
        let r = http.post(h.url()).bearer_auth(&token).header("origin", "https://evil.example").json(&body).send().unwrap();
        assert_eq!(r.status(), 403);
        assert_eq!(http.get(h.url()).send().unwrap().status(), 405);
        let _ = c;
        h.halt();
    }
}
