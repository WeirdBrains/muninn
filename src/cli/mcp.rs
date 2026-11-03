//! `mnnn mcp` (2.20).
use super::{Ctx, McpClientCommand, McpCommand};
use crate::ingest::time::fmt_ts;
use crate::mcp;
use crate::store::{McpRole, NewAction, Secrets, Store};
use serde_json::json;

fn note(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "cli", summary).params(params));
}

/// `KEY=VALUE`, or `KEY` asked for (hidden).
fn pair(ctx: &Ctx, s: &str) -> anyhow::Result<(String, String)> {
    match s.split_once('=') {
        Some((k, v)) if !k.trim().is_empty() => Ok((k.trim().into(), v.into())),
        None if !s.trim().is_empty() => Ok((s.trim().into(), ctx.io.password(&format!("value for {}", s.trim()), "give KEY=VALUE")?)),
        _ => anyhow::bail!("`{s}`: KEY=VALUE"),
    }
}

fn secrets(ctx: &Ctx, into: &mut Secrets, env: &[String], header: &[String], token: Option<String>) -> anyhow::Result<()> {
    for e in env {
        let (k, v) = pair(ctx, e)?;
        into.env.insert(k, v);
    }
    for h in header {
        let (k, v) = pair(ctx, h)?;
        into.headers.insert(k, v);
    }
    if let Some(t) = token {
        let t = if t == "-" { ctx.io.password("bearer token", "give --token TOKEN")? } else { t };
        into.headers.insert("Authorization".into(), format!("Bearer {}", t.trim()));
    }
    Ok(())
}

pub fn run(ctx: &Ctx, cmd: McpCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        McpCommand::Serve {
            client,
            http,
            tls_domain,
            tls_email,
            tls_cert,
            tls_key,
            acme_staging,
            acme_directory,
            acme_challenge,
            acme_http_port,
        } => {
            let wants_http = http.is_some() || tls_domain.is_some() || tls_cert.is_some();
            if wants_http {
                let prefs = ctx.prefs()?;
                let tls = serve_tls(
                    &prefs,
                    ServeTls { tls_domain, tls_email, tls_cert, tls_key, acme_staging, acme_directory, acme_challenge, acme_http_port },
                )?;
                let bind = match http.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
                    Some(a) => a.to_string(),
                    None if !prefs.mcp.bind.trim().is_empty() => prefs.mcp.bind.trim().to_string(),
                    None if tls.is_some() => mcp::DEFAULT_HTTPS.to_string(),
                    None => mcp::DEFAULT_HTTP.to_string(),
                };
                let has_tls = tls.is_some();
                let mut h = mcp::serve::Http::start_with(store, &bind, tls, None)?;
                if !has_tls && !h.addr.ip().is_loopback() {
                    ctx.io.warn(
                        "listening beyond this machine without TLS: use --tls-domain, or put it behind an HTTPS proxy, and keep client tokens secret",
                    );
                }
                if let Some(c) = h.cert() {
                    ctx.io.note(format!("certificate: {} ({}, until {})", c.names.join(", "), c.source, crate::ingest::time::fmt_ts(c.not_after)));
                    if c.source == "self-signed" {
                        ctx.io.note("asking the CA for a certificate now; it is served as soon as it arrives");
                    }
                }
                if let Some(a) = h.http_addr {
                    ctx.io.note(format!("answering ACME HTTP-01 challenges on {a}"));
                }
                ctx.io.note(format!("MCP server on {} (Ctrl-C stops)", h.url()));
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = ctrlc::set_handler(move || {
                    let _ = tx.send(());
                });
                let _ = rx.recv();
                h.halt();
            } else {
                let c = mcp::serve::stdio_client(&store, client.as_deref().unwrap_or_default())?;
                let stdin = std::io::stdin();
                mcp::serve::stdio(&store, &c, stdin.lock(), std::io::stdout())?;
            }
        }
        McpCommand::Client(c) => client(ctx, &store, c)?,
        McpCommand::Add { name, url, env, header, token, command } => {
            let mut sec = Secrets::default();
            secrets(ctx, &mut sec, &env, &header, token)?;
            let s = match url {
                Some(u) => store.add_mcp_server(&name, "http", "", &[], &u, &sec)?,
                None => {
                    let Some((cmd, args)) = command.split_first() else { anyhow::bail!("give --url URL, or the server's command after --") };
                    store.add_mcp_server(&name, "stdio", cmd, args, "", &sec)?
                }
            };
            note(
                &store,
                "mcp.server.add",
                format!("add MCP server {} ({})", s.name, s.transport),
                json!({ "server": s.id, "name": s.name, "transport": s.transport, "credentials": s.secrets.names() }),
            );
            ctx.io.info(format!("added MCP server {} ({}); see its tools: mnnn mcp tools {}", s.name, s.transport, s.name));
        }
        McpCommand::List { json } => {
            let v = store.mcp_servers()?;
            if json {
                let out: Vec<_> = v.iter().map(|s| server_json(&store, s)).collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else if v.is_empty() {
                ctx.io.info("no MCP servers (`mnnn mcp add NAME --url URL` or `-- COMMAND`)");
            } else {
                for s in &v {
                    let approved = store.mcp_approvals(&s.id)?.len();
                    println!("{}\t{}\t{}\t{approved} approved", s.name, s.transport, reach(s));
                }
            }
        }
        McpCommand::Show { name, json } => {
            let s = store.find_mcp_server(&name)?;
            let v = server_json(&store, &s);
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("{} ({})\n{}", s.name, s.transport, reach(&s));
                let names = s.secrets.names();
                println!("credentials: {}", if names.is_empty() { "none".into() } else { names.join(", ") });
                let a = store.mcp_approvals(&s.id)?;
                println!("approved tools: {}", if a.is_empty() { "none".into() } else { a.join(", ") });
            }
        }
        McpCommand::Edit { name, env, header, token, clear } => {
            let s = store.find_mcp_server(&name)?;
            let mut sec = if clear { Secrets::default() } else { s.secrets.clone() };
            secrets(ctx, &mut sec, &env, &header, token)?;
            store.set_mcp_secrets(&s.id, &sec)?;
            note(
                &store,
                "mcp.server.edit",
                format!("change credentials of MCP server {}", s.name),
                json!({ "server": s.id, "name": s.name, "credentials": sec.names() }),
            );
            ctx.io.info(format!("{}: credentials {}", s.name, if sec.names().is_empty() { "none".into() } else { sec.names().join(", ") }));
        }
        McpCommand::Remove { name } => {
            let s = store.find_mcp_server(&name)?;
            if !ctx.io.confirm(&format!("Remove MCP server {}?", s.name))? {
                return Ok(());
            }
            store.remove_mcp_server(&s.id)?;
            note(&store, "mcp.server.remove", format!("remove MCP server {}", s.name), json!({ "server": s.id, "name": s.name }));
            ctx.io.info(format!("removed {}", s.name));
        }
        McpCommand::Tools { name, json } => {
            let s = store.find_mcp_server(&name)?;
            let tools = mcp::client::Conn::open(&s)?.tools()?;
            let approved = store.mcp_approvals(&s.id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&tools)?);
            } else {
                for t in &tools {
                    let n = t["name"].as_str().unwrap_or_default();
                    let mark = if approved.iter().any(|a| a == n) { "✓" } else { " " };
                    println!("{mark} {n}\t{}", t["description"].as_str().unwrap_or_default().lines().next().unwrap_or_default());
                }
            }
        }
        McpCommand::Call { name, tool, args, json } => {
            let s = store.find_mcp_server(&name)?;
            let pairs = args
                .iter()
                .map(|a| a.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())).ok_or_else(|| anyhow::anyhow!("`{a}`: KEY=VALUE")))
                .collect::<anyhow::Result<Vec<_>>>()?;
            let io = ctx.io;
            let mut ask = |q: &str| io.confirm(q).unwrap_or(false);
            let r = mcp::client::call(&store, &s, &tool, mcp::client::args_from_pairs(&pairs), "cli", &mut ask)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                println!("{}", mcp::result_text(&r));
            }
        }
        McpCommand::Approve { name, tool } => {
            let s = store.find_mcp_server(&name)?;
            mcp::client::approve(&store, &s, &tool, "cli", true)?;
            ctx.io.info(format!("approved {}/{tool}", s.name));
        }
        McpCommand::Unapprove { name, tool } => {
            let s = store.find_mcp_server(&name)?;
            mcp::client::approve(&store, &s, &tool, "cli", false)?;
            ctx.io.info(format!("{}/{tool} needs approval again", s.name));
        }
    }
    Ok(())
}

fn reach(s: &crate::store::McpServer) -> String {
    if s.transport == "http" { s.url.clone() } else { std::iter::once(s.command.clone()).chain(s.args.iter().cloned()).collect::<Vec<_>>().join(" ") }
}

fn server_json(store: &Store, s: &crate::store::McpServer) -> serde_json::Value {
    let mut v = serde_json::to_value(s).unwrap_or_default();
    v["credentials"] = json!(s.secrets.names());
    v["approved"] = json!(store.mcp_approvals(&s.id).unwrap_or_default());
    v
}

fn client(ctx: &Ctx, store: &Store, cmd: McpClientCommand) -> anyhow::Result<()> {
    match cmd {
        McpClientCommand::Add { name, role } => {
            let (c, token) = store.add_mcp_client(&name, McpRole::parse(&role)?)?;
            note(
                store,
                "mcp.client.add",
                format!("add MCP client {} ({})", c.name, c.role.as_str()),
                json!({ "client": c.name, "role": c.role.as_str() }),
            );
            ctx.io.note(format!("added MCP client {} with role {}; its token (shown only now):", c.name, c.role.as_str()));
            println!("{token}");
        }
        McpClientCommand::List { json } => {
            let v = store.mcp_clients()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if v.is_empty() {
                ctx.io.info("no MCP clients (`mnnn mcp client add NAME`)");
            } else {
                for c in &v {
                    println!(
                        "{}\t{}\t{}\t{}",
                        c.name,
                        c.role.as_str(),
                        if c.revoked_at.is_some() { "revoked" } else { "active" },
                        c.last_seen_at.map(|t| format!("last seen {}", fmt_ts(t))).unwrap_or_else(|| "never seen".into())
                    );
                }
            }
        }
        McpClientCommand::Role { name, role } => {
            let c = store.find_mcp_client(&name)?;
            let r = McpRole::parse(&role)?;
            store.set_mcp_client_role(&c.id, r)?;
            note(store, "mcp.client.role", format!("MCP client {} is now {}", c.name, r.as_str()), json!({ "client": c.name, "role": r.as_str() }));
            ctx.io.info(format!("{} is now {}", c.name, r.as_str()));
        }
        McpClientCommand::Revoke { name } => {
            let c = store.find_mcp_client(&name)?;
            store.revoke_mcp_client(&c.id, true)?;
            note(store, "mcp.client.revoke", format!("revoke MCP client {}", c.name), json!({ "client": c.name }));
            ctx.io.info(format!("revoked {}", c.name));
        }
        McpClientCommand::Restore { name } => {
            let c = store.find_mcp_client(&name)?;
            store.revoke_mcp_client(&c.id, false)?;
            note(store, "mcp.client.restore", format!("restore MCP client {}", c.name), json!({ "client": c.name }));
            ctx.io.info(format!("restored {}", c.name));
        }
        McpClientCommand::Remove { name } => {
            let c = store.find_mcp_client(&name)?;
            store.remove_mcp_client(&c.id)?;
            note(store, "mcp.client.remove", format!("remove MCP client {}", c.name), json!({ "client": c.name }));
            ctx.io.info(format!("removed {}", c.name));
        }
    }
    Ok(())
}

struct ServeTls {
    tls_domain: Option<String>,
    tls_email: Option<String>,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
    acme_staging: bool,
    acme_directory: Option<String>,
    acme_challenge: Option<String>,
    acme_http_port: Option<u16>,
}

/// TLS for `mcp serve --http`: flags over `[mcp]`, the contact address falling back to `[hosting.acme]`.
fn serve_tls(prefs: &crate::prefs::Preferences, f: ServeTls) -> anyhow::Result<Option<mcp::serve::TlsSetup>> {
    let mut a = prefs.mcp.acme.clone();
    if let Some(d) = f.tls_domain {
        a.domain = d;
        a.enabled = true;
    }
    if let Some(e) = f.tls_email {
        a.email = e;
    }
    if a.email.trim().is_empty() {
        a.email = prefs.hosting.acme.email.clone();
    }
    if f.acme_staging {
        a.directory = "letsencrypt-staging".into();
    }
    if let Some(d) = f.acme_directory {
        a.directory = d;
    }
    if let Some(c) = f.acme_challenge {
        a.challenge = c;
    }
    if let Some(p) = f.acme_http_port {
        a.http_port = p;
    }
    let cert = f.tls_cert.or_else(|| Some(prefs.mcp.tls_cert.trim()).filter(|p| !p.is_empty()).map(Into::into));
    let key = f.tls_key.or_else(|| Some(prefs.mcp.tls_key.trim()).filter(|p| !p.is_empty()).map(Into::into));
    let given = match (cert, key) {
        (Some(c), Some(k)) => Some(crate::org::host::read_pem_pair(&c, &k)?),
        (None, None) => None,
        _ => anyhow::bail!("give both a certificate and its key (--tls-cert FILE --tls-key FILE)"),
    };
    if given.is_some() {
        return Ok(Some(mcp::serve::TlsSetup { acme: None, given, http_port: a.http_port, auto_renew: false }));
    }
    if !a.enabled {
        return Ok(None);
    }
    if !a.email.contains('@') {
        anyhow::bail!("Let's Encrypt needs a contact address: --tls-email you@example.org (or `[mcp.acme] email`)");
    }
    let acme = crate::org::host::Acme::from_prefs(&a)?;
    Ok(acme.map(|acme| mcp::serve::TlsSetup { acme: Some(acme), given: None, http_port: a.http_port, auto_renew: true }))
}
