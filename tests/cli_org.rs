//! `mnnn org` end to end: an org on one home, members from others over loopback.
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

fn wait(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(30), "{what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Start a node in the background and return its address.
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
    let mut addr = String::new();
    wait("node did not start", || {
        let v = json(&e.ok(&["net", "status", "--json"]));
        if let Some(a) = v["status"]["listening"].as_array().and_then(|a| a.first()).and_then(|a| a.as_str()) {
            addr = format!("{a}/p2p/{}", v["status"]["peer_id"].as_str().unwrap());
            return true;
        }
        false
    });
    (child, addr)
}

#[test]
fn an_org_with_roles_resources_endpoints_templates_and_a_site() {
    let e = Env::new();
    assert_eq!(json(&e.ok(&["org", "list", "--json"]))["mine"], serde_json::json!([]));
    e.ok(&["org", "create", "Comet Lab", "--description", "we watch comets"]);
    let o = json(&e.ok(&["org", "show", "comet-lab", "--json"]));
    assert_eq!(o["org"]["visibility"], "private");
    assert_eq!(o["org"]["access"], "members");
    let roles: Vec<&str> = o["roles"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(roles, ["owner", "admin", "user", "viewer"]);
    assert_eq!(o["apply_role"], "user");

    e.ok(&["org", "edit", "comet-lab", "--visibility", "public", "--access", "open", "--hosted", "on"]);
    assert!(!e.run(&["org", "edit", "comet-lab", "--visibility", "secret"]).status.success());
    e.ok(&["org", "role", "create", "comet-lab", "observer", "--rank", "20", "--perm", "read", "--perm", "call"]);
    assert!(!e.run(&["org", "role", "create", "comet-lab", "bad", "--perm", "fly"]).status.success());
    e.ok(&["org", "role", "edit", "comet-lab", "observer", "--rank", "25"]);
    assert!(e.ok(&["org", "role", "list", "comet-lab"]).contains("rank  25"));
    assert!(!e.run(&["org", "role", "remove", "comet-lab", "owner"]).status.success(), "the owner role stays");
    e.ok(&["org", "member", "defaults", "comet-lab", "--apply", "observer"]);
    assert_eq!(json(&e.ok(&["org", "show", "comet-lab", "--json"]))["apply_role"], "observer");
    assert!(e.ok(&["org", "role", "perms"]).contains("members.approve"));

    // resources: a journal given to the org, published
    e.ok(&["journal", "new", "field notes"]);
    e.ok(&["journal", "write", "field notes", "--title", "Comet", "--text", "a comet over the hill"]);
    e.ok(&["org", "resource", "add", "comet-lab", "journal", "field notes"]);
    e.ok(&["org", "publish", "comet-lab", "field notes"]);
    let rs = json(&e.ok(&["org", "resource", "list", "comet-lab", "--json"]));
    assert_eq!(rs[0]["published"], "public");
    e.ok(&["org", "resource", "archive", "comet-lab", "field notes"]);
    assert!(e.ok(&["org", "resource", "list", "comet-lab"]).contains("archived"));
    e.ok(&["org", "resource", "restore", "comet-lab", "field notes"]);
    e.ok(&["org", "unpublish", "comet-lab", "field notes"]);
    assert_eq!(json(&e.ok(&["org", "resource", "list", "comet-lab", "--json"]))[0]["published"], "no");

    // projects
    e.ok(&["project", "new", "Tails"]);
    e.ok(&["org", "project", "add", "comet-lab", "Tails"]);
    assert!(e.ok(&["org", "project", "list", "comet-lab"]).contains("Tails"));

    // a template with an asset
    let d = e.dir.path();
    std::fs::write(d.join("t.html"), "<html><head><title>{{title}}</title></head><body><h1>{{org.name}}</h1>{{content}}</body></html>").unwrap();
    std::fs::write(d.join("logo.svg"), "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    e.ok(&["org", "template", "save", "comet-lab", "plain", "--html", d.join("t.html").to_str().unwrap(), "--use"]);
    e.ok(&["org", "template", "asset", "comet-lab", "plain", d.join("logo.svg").to_str().unwrap()]);
    let tl = e.ok(&["org", "template", "list", "comet-lab"]);
    assert!(tl.contains("(in use)") && tl.contains("logo.svg"), "{tl}");

    // a records endpoint answers calls
    e.ok(&["model", "create", "Sightings", "--fields", "name:text!; mag:real"]);
    e.ok(&["model", "record", "create", "sightings", "name=Halley", "mag=4.5"]);
    e.ok(&["org", "endpoint", "add", "comet-lab", "sightings", "--kind", "records", "--target", "Sightings", "--anyone"]);
    assert!(e.ok(&["org", "endpoint", "list", "comet-lab"]).contains("records.read"));
    let r = e.ok(&["org", "call", "comet-lab", "sightings", "--input", r#"{"op":"query"}"#]);
    assert!(r.contains("Halley"), "{r}");
    assert!(!e.run(&["org", "endpoint", "add", "comet-lab", "x", "--kind", "spell", "--target", "y"]).status.success());
    assert!(!e.run(&["org", "call", "comet-lab", "nothing"]).status.success());

    // the site: served on localhost, status for other processes, stopped by request
    let mut c =
        Daemon(e.cmd().args(["org", "host", "start", "--port", "0", "--for", "60"]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let mut st = Value::Null;
    wait("host did not start", || {
        st = json(&e.ok(&["org", "host", "status", "--json"]));
        st["running"] == true
    });
    let url = st["host"]["url"].as_str().unwrap();
    assert!(url.starts_with("https://127.0.0.1:"), "{url}");
    assert_eq!(st["host"]["orgs"][0], "comet-lab");
    assert_eq!(st["host"]["cert"]["source"], "self-signed");
    assert!(e.ok(&["org", "cert", "status"]).contains("self-signed"));
    e.ok(&["org", "host", "stop"]);
    wait("host did not stop", || c.0.try_wait().unwrap().is_some());
    assert_eq!(json(&e.ok(&["org", "host", "status", "--json"]))["running"], false);
    let h = e.ok(&["history", "list"]);
    assert!(h.contains("org.host.start") && h.contains("org.host.stop") && h.contains("org.create"), "{h}");

    // ACME is opt-in and needs a domain and an email
    assert!(!e.run(&["org", "cert", "acme", "--email", "me@example.org"]).status.success());
    e.ok(&["org", "cert", "acme", "--domain", "lab.example.org", "--email", "me@example.org", "--directory", "letsencrypt-staging"]);
    let cs = json(&e.ok(&["org", "cert", "status", "--json"]));
    assert_eq!(cs["acme"]["enabled"], true);
    assert!(cs["acme"]["directory"].as_str().unwrap().contains("staging"));
    e.ok(&["org", "cert", "acme", "--off"]);
    assert_eq!(json(&e.ok(&["org", "cert", "status", "--json"]))["acme"]["enabled"], false);

    e.ok(&["-y", "org", "remove", "comet-lab"]);
    assert!(e.ok(&["journal", "list"]).contains("field notes"), "resources stay");
    assert_eq!(json(&e.ok(&["org", "list", "--json"]))["mine"], serde_json::json!([]));
}

#[test]
fn members_apply_join_by_invitation_give_fetch_and_get_kicked() {
    let a = Env::new();
    let b = Env::new();
    let c = Env::new();
    let (_na, addr) = start(&a, "anna");
    b.ok(&["net", "config", "--label", "ben", "--mdns", "off"]);
    c.ok(&["net", "config", "--label", "cleo", "--mdns", "off"]);
    a.ok(&["org", "create", "Comet Lab", "--public"]);
    a.ok(&["org", "create", "Hidden"]);

    let l = json(&b.ok(&["org", "browse", &addr, "--json"]));
    let names: Vec<&str> = l.as_array().unwrap().iter().map(|o| o["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Comet Lab"], "private orgs are not listed");

    // ben applies, anna approves, ben sees it
    b.ok(&["org", "apply", &addr, "comet-lab", "--note", "I like comets"]);
    let m = json(&a.ok(&["org", "member", "list", "comet-lab", "--json"]));
    let ben = m.as_array().unwrap().iter().find(|m| m["member"]["status"] == "applied").expect("an application");
    assert_eq!(ben["member"]["note"], "I like comets");
    a.ok(&["org", "member", "approve", "comet-lab", "ben", "--role", "admin"]);
    b.ok(&["org", "sync", "comet-lab"]);
    let j = json(&b.ok(&["org", "show", "comet-lab", "--json"]));
    assert_eq!(j["status"], "member");
    assert_eq!(j["role"], "admin");

    // cleo is invited and joins with the token
    let out = a.ok(&["org", "member", "invite", "comet-lab", "--label", "cleo", "--role", "viewer"]);
    let token = out.lines().find(|l| l.starts_with("mnnn-x1-")).expect("a token").to_string();
    c.ok(&["org", "join", &token]);
    wait("cleo did not become a member", || {
        let m = json(&a.ok(&["org", "member", "list", "comet-lab", "--json"]));
        m.as_array().unwrap().iter().any(|m| m["member"]["status"] == "member" && m["role"] == "viewer")
    });
    c.ok(&["org", "sync", "comet-lab"]);
    assert_eq!(json(&c.ok(&["org", "show", "comet-lab", "--json"]))["role"], "viewer");

    // ben gives a journal (a copy); members see it; cleo fetches it
    b.ok(&["journal", "new", "field notes"]);
    b.ok(&["journal", "write", "field notes", "--title", "Comet", "--text", "a comet over the hill"]);
    b.ok(&["org", "give", "comet-lab", "journal", "field notes"]);
    assert!(a.ok(&["org", "resource", "list", "comet-lab"]).contains("field notes"));
    assert!(c.ok(&["org", "resource", "list", "comet-lab"]).contains("field notes"));
    assert!(c.ok(&["org", "fetch", "comet-lab", "field notes", "--format", "text"]).contains("comet over the hill"));
    assert!(!c.run(&["org", "give", "comet-lab", "journal", "anything"]).status.success(), "viewers cannot give");

    // ben (admin) manages through anna's node: publishes and changes cleo's role
    b.ok(&["org", "publish", "comet-lab", "field notes", "--to", "members"]);
    assert_eq!(json(&a.ok(&["org", "resource", "list", "comet-lab", "--json"]))[0]["published"], "members");
    b.ok(&["org", "member", "role", "comet-lab", "cleo", "user"]);
    assert!(b.ok(&["org", "member", "list", "comet-lab"]).contains("cleo"));
    assert!(!c.run(&["org", "member", "kick", "comet-lab", "ben"]).status.success(), "a user cannot kick an admin");

    // a member's signed web request header
    let h = b.ok(&["org", "sign", "comet-lab", "GET", "/o/comet-lab/api/artifacts"]);
    assert!(h.starts_with("Authorization: Muninn exchange="), "{h}");

    // ben is kicked; what he gave stays with the org, archived
    a.ok(&["-y", "org", "member", "kick", "comet-lab", "ben", "--resources", "archive"]);
    let rs = json(&a.ok(&["org", "resource", "list", "comet-lab", "--json"]));
    assert_eq!(rs[0]["state"], "archived");
    assert!(!b.run(&["org", "resource", "list", "comet-lab"]).status.success());

    // cleo leaves
    c.ok(&["-y", "org", "leave", "comet-lab"]);
    wait("anna did not see cleo leave", || {
        let m = json(&a.ok(&["org", "member", "list", "comet-lab", "--json"]));
        m.as_array().unwrap().iter().any(|m| m["member"]["status"] == "left")
    });
    let h = a.ok(&["history", "list"]);
    assert!(h.contains("org.member.approve") && h.contains("org.member.kick"), "{h}");
    a.ok(&["net", "stop"]);
}
