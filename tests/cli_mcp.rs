//! `mnnn mcp` end to end (2.20): one Muninn serves another over stdio, as an external MCP
//! server with a credential, a client role, approvals, a rule set calling it, and a revoke.
mod common;
use common::*;

#[test]
fn one_muninn_calls_another_over_mcp() {
    let b = Env::new();
    b.ok(&["journal", "new", "Remote"]);
    let token = b.ok(&["mcp", "client", "add", "bot", "--role", "write"]);
    assert!(token.trim().starts_with("mnnn_mcp_"), "{token}");
    assert!(b.ok(&["mcp", "client", "list"]).contains("bot\twrite\tactive"));

    let a = Env::new();
    let home = format!("MUNINN_HOME={}", b.home().display());
    let bin = env!("CARGO_BIN_EXE_mnnn");
    a.ok(&["mcp", "add", "b", "--env", &home, "--", bin, "--non-interactive", "mcp", "serve", "--client", "bot"]);
    let show = a.ok(&["mcp", "show", "b"]);
    assert!(show.contains("credentials: env MUNINN_HOME") && !show.contains(&b.home().display().to_string()), "{show}");
    assert!(!a.ok(&["mcp", "list", "--json"]).contains(&b.home().display().to_string()));
    let tools = a.ok(&["mcp", "tools", "b"]);
    assert!(tools.contains("write_entry") && !tools.contains("run_code"), "{tools}");

    // not approved: refused without a yes; with -y it is approved and called
    assert!(!a.run(&["mcp", "call", "b", "list", "what=journals"]).status.success());
    let out = a.ok(&["-y", "mcp", "call", "b", "list", "what=journals"]);
    assert!(out.contains("Remote"), "{out}");
    assert!(a.ok(&["mcp", "show", "b"]).contains("approved tools: list"));

    // a rule set writes on b; it waits for approval first
    a.ok(&["rule", "create", "Mirror", "--on", "journal.create", "--then", "mcp b write_entry journal=Remote text=\"new journal {name}\""]);
    a.ok(&["journal", "new", "Field"]);
    assert!(!b.ok(&["journal", "show", "Remote"]).contains("new journal Field"));
    assert!(a.ok(&["notify", "list"]).contains("wants to call MCP tool b/write_entry"));
    a.ok(&["mcp", "approve", "b", "write_entry"]);
    a.ok(&["journal", "new", "Lab"]);
    assert!(b.ok(&["journal", "show", "Remote"]).contains("new journal Lab"));
    let hist = b.ok(&["history", "list"]);
    assert!(hist.contains("(MCP client bot)"), "{hist}");

    // revoked: refused at once
    b.ok(&["mcp", "client", "revoke", "bot"]);
    assert!(!a.run(&["mcp", "call", "b", "list", "what=journals"]).status.success());
    a.ok(&["mcp", "unapprove", "b", "list"]);
    a.ok(&["-y", "mcp", "remove", "b"]);
    assert!(a.ok(&["mcp", "list"]).contains("no MCP servers"));
}

#[test]
fn mcp_serve_tls_needs_a_contact_and_both_files() {
    let e = Env::new();
    let err = |args: &[&str]| {
        let o = e.run(args);
        assert!(!o.status.success(), "{args:?} should fail");
        String::from_utf8_lossy(&o.stderr).to_string()
    };
    let no_email = err(&["mcp", "serve", "--tls-domain", "mcp.example.test", "--http", "127.0.0.1:0"]);
    assert!(no_email.contains("--tls-email"), "{no_email}");
    assert!(err(&["mcp", "serve", "--tls-cert", "c.pem"]).contains("--tls-key"));
    assert!(err(&["mcp", "serve", "--client", "bot", "--tls-domain", "mcp.example.test"]).contains("cannot be used with"));
    let missing = e.home().join("none.pem");
    let m = missing.display().to_string();
    assert!(!err(&["mcp", "serve", "--tls-cert", &m, "--tls-key", &m, "--http", "127.0.0.1:0"]).is_empty());
}
