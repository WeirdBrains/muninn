//! `mnnn user`, `mnnn connection` and `mnnn invite` end to end: nodes on loopback TCP.
mod common;
use common::Env;
use serde_json::Value;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

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
}

fn wait(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(20), "timed out: {what}");
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn me(e: &Env) -> String {
    json(&e.ok(&["user", "show", "--json"]))["id"].as_str().unwrap().to_string()
}

#[test]
fn every_home_has_a_user_with_this_node() {
    let e = Env::new();
    let v = json(&e.ok(&["user", "show", "--json"]));
    assert!(v["id"].as_str().unwrap().len() >= 16, "{v}");
    assert_eq!(v["nodes"], 1);
    e.ok(&["user", "label", "Anna"]);
    assert!(e.ok(&["user", "show"]).contains("Anna"));
    let nodes = e.ok(&["user", "nodes"]);
    assert!(nodes.contains("this node"), "{nodes}");
    assert!(e.ok(&["connection", "list", "--json"]).trim() == "[]");
    assert!(e.ok(&["invite", "list", "--json"]).contains("\"invites\": []"));
    // revoking this node is refused
    let here = json(&e.ok(&["net", "status", "--json"]))["peer_id"].as_str().map(str::to_string);
    if let Some(p) = here {
        assert!(!e.run(&["-y", "user", "revoke", &p]).status.success());
    }
}

#[test]
fn invites_make_untrusted_connections_that_can_be_trusted() {
    let a = Env::new();
    let b = Env::new();
    let (ca, _) = start(&a, "anna");
    let (cb, _) = start(&b, "ben");
    a.ok(&["user", "label", "Anna"]);
    b.ok(&["user", "label", "Ben"]);

    let link = a.ok(&["invite", "create", "--label", "ben"]);
    assert!(link.trim().starts_with("muninn://join/"), "{link}");
    let l = json(&a.ok(&["invite", "list", "--json"]));
    assert_eq!(l["invites"][0]["status"], "unused", "{l}");
    let used = b.ok(&["invite", "accept", link.trim()]);
    assert!(used.contains("connected"), "{used}");

    // the users are introduced on their own, as untrusted connections
    let ben = me(&b);
    let anna = me(&a);
    wait("anna knows ben", || a.ok(&["connection", "list"]).contains(&ben[..8]));
    wait("ben knows anna", || b.ok(&["connection", "list"]).contains(&anna[..8]));
    let c = json(&a.ok(&["connection", "list", "--json"]));
    assert_eq!(c[0]["trusted"], false, "{c}");
    assert_eq!(c[0]["label"], "Ben");
    assert!(a.ok(&["user", "list"]).contains("untrusted"));

    // trust is a decision, and it is in history
    a.ok(&["connection", "trust", "Ben"]);
    assert_eq!(json(&a.ok(&["connection", "list", "--json"]))[0]["trusted"], true);
    a.ok(&["connection", "trust", "Ben", "--off"]);
    assert_eq!(json(&a.ok(&["connection", "list", "--json"]))[0]["trusted"], false);
    let h = a.ok(&["history", "list"]);
    assert!(h.contains("Ben"), "{h}");

    // a used invite cannot be revoked; a fresh one can, and then it does not work
    let id = json(&a.ok(&["invite", "list", "--all", "--json"]))["invites"][0]["id"].as_str().unwrap().to_string();
    assert!(!a.run(&["invite", "revoke", &id[..8]]).status.success());
    let link2 = a.ok(&["invite", "create"]);
    let id2 = json(&a.ok(&["invite", "list", "--json"]))["invites"][0]["id"].as_str().unwrap().to_string();
    a.ok(&["invite", "revoke", &id2[..8]]);
    let c3 = Env::new();
    let (cc, _) = start(&c3, "cleo");
    let o = c3.run(&["invite", "accept", link2.trim()]);
    eprintln!("revoked invite: {} / {} / {}", o.status, String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(!o.status.success());
    stop(&c3, cc);

    a.ok(&["connection", "remove", "Ben"]);
    assert_eq!(a.ok(&["connection", "list", "--json"]).trim(), "[]");
    stop(&a, ca);
    stop(&b, cb);
}

#[test]
fn a_new_node_links_to_my_user_after_approval_and_can_be_revoked() {
    let a = Env::new();
    let b = Env::new();
    let (ca, addr_a) = start(&a, "desk");
    let (cb, _) = start(&b, "laptop");
    let anna = me(&a);
    assert_ne!(me(&b), anna);

    let out = b.ok(&["user", "link", &addr_a]);
    assert!(out.contains("pending") || out.contains("wait") || out.contains("approv"), "{out}");
    let l = json(&a.ok(&["invite", "list", "--json"]));
    assert_eq!(l["requests"][0]["for"], "link node", "{l}");
    // not a plain connection request
    a.ok(&["-y", "user", "approve"]);
    wait("laptop becomes anna", || me(&b) == anna);
    let nodes = json(&a.ok(&["user", "nodes", "--json"]));
    assert_eq!(nodes.as_array().unwrap().len(), 2, "{nodes}");

    a.ok(&["-y", "user", "revoke", "laptop"]);
    let nodes = a.ok(&["user", "nodes"]);
    assert!(nodes.contains("revoked"), "{nodes}");
    assert!(!a.run(&["-y", "user", "revoke", "laptop"]).status.success());
    stop(&a, ca);
    stop(&b, cb);
}
