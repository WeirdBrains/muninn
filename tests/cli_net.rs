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
    e.ok(&["net", "config", "--label", label, "--mdns", "off", "--collab-bind", "127.0.0.1", "--collab-port", "0"]);
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

#[test]
fn broadcasts_reach_connections_as_notifications() {
    let a = Env::new();
    let b = Env::new();
    let (ca, _) = start(&a, "anna");
    let (cb, addr_b) = start(&b, "ben");
    // nobody to tell yet
    let o = a.run(&["net", "broadcast", "deploy", "--to", "connections"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("no one to tell"));
    a.ok(&["net", "connect", &addr_b, "--label", "ben"]);
    b.ok(&["net", "accept", "anna"]);
    let t = Instant::now();
    while !a.ok(&["net", "peers"]).lines().any(|l| l.contains("ben") && l.contains("connected")) {
        assert!(t.elapsed() < Duration::from_secs(15), "{}", a.ok(&["net", "peers"]));
        std::thread::sleep(Duration::from_millis(100));
    }
    // a rule set on ben's side reacts to broadcasts too
    b.ok(&["rule", "create", "Heard", "--on", "net.broadcast", "--if", "topic = deploy", "--then", "notify \"deploy by {from}\" level=warn"]);
    let r = json(&a.ok(&["net", "broadcast", "deploy", "v2 is out", "--to", "connections", "--json"]));
    assert_eq!(r["sent"].as_array().unwrap().len(), 1, "{r}");
    let v = json(&b.ok(&["notify", "list", "--json"]));
    let titles: Vec<&str> = v.as_array().unwrap().iter().filter_map(|n| n["title"].as_str()).collect();
    assert!(titles.contains(&"anna: deploy") && titles.contains(&"deploy by anna"), "{v}");
    assert!(v.as_array().unwrap().iter().any(|n| n["body"] == "v2 is out"), "{v}");
    stop(&a, ca);
    stop(&b, cb);
}

fn connected(a: &Env, b: &Env, addr_b: &str) {
    a.ok(&["net", "connect", addr_b, "--label", "ben"]);
    b.ok(&["net", "accept", "anna"]);
    let t = Instant::now();
    while !a.ok(&["net", "peers"]).lines().any(|l| l.contains("ben") && l.contains("connected")) {
        assert!(t.elapsed() < Duration::from_secs(15), "{}", a.ok(&["net", "peers"]));
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The two-node collaboration scenario, with anna (the guest) and ben (the host) on the
/// given transports; `via` is what anna should end up using.
fn collaborate(anna: &str, ben: &str, via: &str) {
    let a = Env::new();
    let b = Env::new();
    a.ok(&["net", "config", "--collab-transport", anna]);
    b.ok(&["net", "config", "--collab-transport", ben]);
    let (ca, _) = start(&a, "anna");
    let (cb, addr_b) = start(&b, "ben");
    connected(&a, &b, &addr_b);
    // a message, kept on both sides, notifying ben
    assert!(a.ok(&["msg", "send", "ben", "the comet is back", "--about", "journal:field"]).contains("delivered"));
    let v = json(&b.ok(&["msg", "list", "--unread", "--json"]));
    assert_eq!(v[0]["text"], "the comet is back", "{v}");
    assert_eq!(v[0]["about"], "journal:field");
    assert!(json(&b.ok(&["notify", "list", "--json"])).as_array().unwrap().iter().any(|n| n["kind"] == "net.message" && n["title"] == "anna"));
    b.ok(&["msg", "read"]);
    assert!(json(&b.ok(&["msg", "list", "--unread", "--json"])).as_array().unwrap().is_empty());
    assert!(a.ok(&["msg", "list", "ben"]).contains("→ ben"));
    // peers: anna is ben's peer
    let p = json(&b.ok(&["peers", "--json"]));
    assert_eq!(p[0]["label"], "anna", "{p}");
    // ben hosts an entry live; anna sees the offer, joins and edits it
    b.ok(&["journal", "new", "field notes"]);
    b.ok(&["journal", "write", "field notes", "--title", "Comet", "--text", "a comet over the hill"]);
    let j = json(&b.ok(&["journal", "show", "field notes", "--json"]));
    let entry = j["entries"][0]["entry"]["id"].as_str().unwrap().to_string();
    let session = b.ok(&["live", "share", "entry", &entry, "--with", "anna"]).lines().last().unwrap().trim().to_string();
    let offers = json(&a.ok(&["peers", "show", "ben", "--json"]));
    assert_eq!(offers["live"][0]["session"], session.as_str(), "{offers}");
    assert!(a.ok(&["live", "join", "ben", &session]).contains("joined"));
    let show = json(&a.ok(&["live", "show", &session[..8], "--json"]));
    assert_eq!(show["docs"]["body"], "a comet over the hill", "{show}");
    assert_eq!(show["transport"], via, "{show}");
    let fell_back = a.ok(&["history", "list"]).contains("net.collab.fallback");
    assert_eq!(fell_back, anna == "stream" && ben == "poll", "a stream node falls back (and says so) only for a poll peer");
    // anna follows the session: ben's next change reaches her without asking (pushed over
    // a stream, polled otherwise)
    let mut watch = Daemon(a.cmd().args(["live", "watch", &session[..8], "--json"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap());
    let out = watch.0.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for l in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            let _ = tx.send(l);
        }
    });
    let first = json(&rx.recv_timeout(Duration::from_secs(20)).expect("a first state"));
    assert_eq!(first["transport"], via);
    // ben edits through the session, then anna (who sees ben's edit first)
    b.ok(&["live", "edit", &session[..8], "--value", "a bright comet over the hill"]);
    let t = Instant::now();
    loop {
        let l = rx.recv_timeout(Duration::from_secs(20)).expect("the change arrives");
        if json(&l)["docs"]["body"] == "a bright comet over the hill" {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(20));
    }
    drop(watch);
    let show = json(&a.ok(&["live", "show", &session[..8], "--json"]));
    assert_eq!(show["docs"]["body"], "a bright comet over the hill", "{show}");
    a.ok(&["live", "edit", &session[..8], "--value", "a bright comet over the hill tonight", "--cursor", "29"]);
    let j = json(&b.ok(&["journal", "show", "field notes", "--json"]));
    assert_eq!(j["entries"][0]["entry"]["body"], "a bright comet over the hill tonight", "written back: {j}");
    let host = json(&b.ok(&["live", "show", &session[..8], "--json"]));
    assert!(host["present"].as_array().unwrap().iter().any(|c| c["label"] == "anna" && c["cursor"] == 29), "{host}");
    // a stranger session id is refused; ben closes it
    assert!(!a.run(&["live", "join", "ben", "nope"]).status.success());
    b.ok(&["live", "stop", &session[..8]]);
    let show = json(&a.ok(&["live", "show", &session[..8], "--json"]));
    assert_eq!(show["session"]["status"], "closed");
    stop(&a, ca);
    stop(&b, cb);
}

#[test]
fn collaboration_over_streams() {
    collaborate("stream", "stream", "stream");
}

#[test]
fn collaboration_over_polling() {
    collaborate("poll", "poll", "poll");
}

#[test]
fn a_stream_node_polls_a_poll_node_and_back() {
    collaborate("stream", "poll", "poll");
    collaborate("poll", "stream", "poll");
}

#[test]
fn an_unreachable_stream_listener_falls_back_to_polling() {
    let a = Env::new();
    let b = Env::new();
    // ben says his listener is somewhere nobody listens (a firewall, NAT, a wrong forward)
    b.ok(&["net", "config", "--collab-advertise", "127.0.0.1:1"]);
    let (ca, _) = start(&a, "anna");
    let (cb, addr_b) = start(&b, "ben");
    connected(&a, &b, &addr_b);
    assert!(a.ok(&["msg", "send", "ben", "still there?"]).contains("delivered"), "polling still delivers");
    let h = a.ok(&["history", "list"]);
    assert!(h.contains("net.collab.fallback") && h.contains("did not answer"), "{h}");
    b.ok(&["journal", "new", "notes"]);
    b.ok(&["journal", "write", "notes", "--title", "T", "--text", "x"]);
    let j = json(&b.ok(&["journal", "show", "notes", "--json"]));
    let entry = j["entries"][0]["entry"]["id"].as_str().unwrap().to_string();
    let session = b.ok(&["live", "share", "entry", &entry, "--with", "anna"]).lines().last().unwrap().trim().to_string();
    a.ok(&["live", "join", "ben", &session]);
    let show = json(&a.ok(&["live", "show", &session[..8], "--json"]));
    assert_eq!(show["transport"], "poll", "{show}");
    // it is not tried again for a while (one fallback record)
    assert_eq!(a.ok(&["history", "list"]).matches("net.collab.fallback").count(), 1);
    // the flag overrides the setting
    assert!(a.ok(&["--transport", "poll", "net", "config"]).contains("now poll"));
    stop(&a, ca);
    stop(&b, cb);
}
