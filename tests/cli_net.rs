//! `mnnn net` end to end: two homes, two nodes on loopback TCP.
mod common;
use common::Env;
use serde_json::Value;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

/// A background node, killed if a test fails before stopping it.
struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Start a node in the background and wait for its address.
fn start(e: &Env, label: &str) -> (Daemon, String) {
    e.ok(&["net", "config", "--label", label, "--mdns", "off"]);
    let child = Daemon(
        e.cmd()
            .args(["net", "start", "--listen", "/ip4/127.0.0.1/tcp/0", "--for", "120"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let t = Instant::now();
    loop {
        let v = json(&e.ok(&["net", "status", "--json"]));
        if !v["running"].is_null()
            && let Some(a) = v["status"]["listening"].as_array().and_then(|a| a.first()).and_then(|a| a.as_str())
        {
            let peer = v["status"]["peer_id"].as_str().unwrap();
            return (child, format!("{a}/p2p/{peer}"));
        }
        assert!(t.elapsed() < Duration::from_secs(30), "node did not start");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn stop(e: &Env, mut c: Daemon) {
    e.ok(&["net", "stop"]);
    let t = Instant::now();
    while c.0.try_wait().unwrap().is_none() {
        assert!(t.elapsed() < Duration::from_secs(20), "node did not stop");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(json(&e.ok(&["net", "status", "--json"]))["running"].is_null());
}

#[test]
fn nothing_is_shared_by_default() {
    let e = Env::new();
    let v = json(&e.ok(&["net", "status", "--json"]));
    assert!(v["running"].is_null());
    assert_eq!(v["shared"], 0);
    // the node starts with the TUI by default; strangers ask first
    assert_eq!(v["settings"]["autostart"], true);
    assert_eq!(v["settings"]["accept"], "known");
    assert!(e.ok(&["net", "requests"]).contains("no requests"));
    assert_eq!(e.ok(&["net", "shared", "--json"]).trim(), "[]");
    assert!(e.ok(&["net", "exchange", "list", "--json"]).trim() == "[]");
    // an offer can be made offline: a token to pass on
    let t = e.ok(&["net", "exchange", "create", "--label", "later"]);
    assert!(t.trim().starts_with("mnnn-x1-offer:"), "{t}");
    // the events are known to rules
    let ev = e.ok(&["rule", "events"]);
    assert!(ev.contains("net.exchange.complete") && ev.contains("net.share"));
}

#[test]
fn connecting_asks_once_and_then_just_works() {
    let a = Env::new();
    let b = Env::new();
    let (ca, addr_a) = start(&a, "anna");
    let (cb, addr_b) = start(&b, "ben");
    let out = a.ok(&["net", "connect", &addr_b, "--label", "ben"]);
    assert!(out.contains("pending"), "{out}");
    assert!(!out.contains("mnnn-x1-") && !out.contains("code"), "no tokens or codes: {out}");
    let reqs = b.ok(&["net", "requests"]);
    assert!(reqs.contains("anna") && reqs.contains("node"), "{reqs}");
    let acc = b.ok(&["net", "accept", "anna"]);
    assert!(acc.contains("connected"), "{acc}");
    let t = Instant::now();
    while !a.ok(&["net", "peers"]).lines().any(|l| l.contains("ben") && l.contains("connected")) {
        assert!(t.elapsed() < Duration::from_secs(15), "{}", a.ok(&["net", "peers"]));
        std::thread::sleep(Duration::from_millis(100));
    }
    // a known node connects at once from the other side too
    assert!(b.ok(&["net", "connect", &addr_a]).contains("connected"));
    // sharing with a name works without any exchange commands
    b.ok(&["journal", "new", "field notes"]);
    b.ok(&["net", "share", "journal", "field notes", "--with", "anna"]);
    assert!(a.ok(&["net", "list", "ben"]).contains("field notes"));
    // an invite link: whoever uses it connects, no request needed
    let c = Env::new();
    c.ok(&["net", "config", "--label", "cleo", "--mdns", "off"]);
    let link = a.ok(&["net", "invite"]);
    assert!(link.trim().starts_with("muninn://join/"), "{link}");
    let used = c.ok(&["net", "connect", link.trim()]);
    assert!(used.contains("connected"), "{used}");
    let t = Instant::now();
    while !a.ok(&["net", "peers"]).lines().any(|l| l.contains("cleo") && l.contains("connected")) {
        assert!(t.elapsed() < Duration::from_secs(15), "{}", a.ok(&["net", "peers"]));
        std::thread::sleep(Duration::from_millis(100));
    }
    // disconnect ends it on both sides
    a.ok(&["net", "disconnect", "ben"]);
    let t = Instant::now();
    while b.ok(&["net", "peers"]).lines().any(|l| l.contains("anna") && l.contains("connected")) {
        assert!(t.elapsed() < Duration::from_secs(15));
        std::thread::sleep(Duration::from_millis(100));
    }
    stop(&a, ca);
    stop(&b, cb);
}

#[test]
fn two_nodes_exchange_share_search_fetch_and_revoke() {
    let a = Env::new();
    let b = Env::new();
    let (ca, _addr_a) = start(&a, "anna");
    let (cb, addr_b) = start(&b, "ben");

    // A offers straight to B; B accepts the incoming offer; the answer flows back
    let x = json(&a.ok(&["net", "exchange", "create", "--label", "ben", "--peer", &addr_b, "--json"]));
    assert!(x["delivered"]["Ok"].is_string(), "{x}");
    let id = x["exchange"]["id"].as_str().unwrap().to_string();
    let list = json(&b.ok(&["net", "exchange", "list", "--json"]));
    assert_eq!(list[0]["status"], "incoming");
    let acc = json(&b.ok(&["net", "exchange", "accept", &id[..8], "--label", "anna", "--json"]));
    assert!(acc["delivered"]["Ok"].is_string(), "{acc}");
    let code = acc["exchange"]["short_code"].as_str().unwrap().to_string();
    let t = Instant::now();
    let mine = loop {
        let v = json(&a.ok(&["net", "exchange", "show", "ben", "--json"]));
        if v["status"] == "complete" {
            break v;
        }
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(mine["short_code"], code.as_str());
    assert!(!a.run(&["net", "exchange", "verify", "ben", "00000-00000"]).status.success());
    a.ok(&["net", "exchange", "verify", "ben", &code]);
    b.ok(&["net", "exchange", "verify", "anna", &code]);

    // B shares one journal, keeps another
    b.ok(&["journal", "new", "field notes"]);
    b.ok(&["journal", "write", "field notes", "--title", "Comet", "--text", "a comet over the hill"]);
    b.ok(&["journal", "new", "diary"]);
    b.ok(&["journal", "write", "diary", "--title", "Comet secret", "--text", "a private comet"]);
    b.ok(&["net", "share", "journal", "field notes"]);
    assert!(b.ok(&["net", "shared"]).contains("field notes"));

    let r = json(&a.ok(&["net", "search", "--journals", "--query", "comet", "--json"]));
    let origins: Vec<&str> = r.as_array().unwrap().iter().map(|o| o["origin"].as_str().unwrap()).collect();
    assert_eq!(origins, ["local", "ben"]);
    assert_eq!(r[1]["found"].as_array().unwrap().len(), 1, "{r}");
    let text = a.ok(&["net", "search", "--journals", "--query", "comet"]);
    assert!(text.lines().any(|l| l.starts_with("ben\t") && l.contains("field notes")), "{text}");
    assert!(!text.contains("diary"));

    let l = a.ok(&["net", "list", "ben"]);
    assert!(l.contains("field notes") && !l.contains("diary"), "{l}");
    a.ok(&["net", "fetch", "ben", "field notes", "--import"]);
    assert!(a.ok(&["journal", "list"]).contains("field notes"));
    assert!(!a.run(&["net", "fetch", "ben", "diary"]).status.success());

    // backups of the exchanges restore the identity elsewhere
    let f = a.dir.path().join("net.age");
    a.ok(&["net", "exchange", "export", "--out", f.to_str().unwrap()]);
    let c = Env::new();
    c.ok(&["-y", "net", "exchange", "restore", f.to_str().unwrap()]);
    let pa = json(&a.ok(&["net", "status", "--json"]))["status"]["peer_id"].clone();
    assert_eq!(json(&c.ok(&["net", "status", "--json"]))["status"]["peer_id"], pa);
    assert_eq!(json(&c.ok(&["net", "exchange", "list", "--json"]))[0]["status"], "verified");

    // revoke: both sides end it
    a.ok(&["-y", "net", "exchange", "revoke", "ben"]);
    let t = Instant::now();
    loop {
        if json(&b.ok(&["net", "exchange", "show", "anna", "--json"]))["status"] == "revoked" {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(a.ok(&["history", "list"]).contains("revoked"));
    stop(&a, ca);
    stop(&b, cb);
}
