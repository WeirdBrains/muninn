//! `mnnn review` end to end: local reviews, and a reviewer on another home over loopback.
mod common;
use common::Env;
use serde_json::Value;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
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

fn start(e: &Env, label: &str) -> Daemon {
    e.ok(&["net", "config", "--label", label, "--mdns", "off"]);
    let child = Daemon(
        e.cmd()
            .args(["net", "start", "--listen", "/ip4/127.0.0.1/tcp/0", "--for", "120"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait("node did not start", || json(&e.ok(&["net", "status", "--json"]))["status"]["listening"].as_array().is_some_and(|a| !a.is_empty()));
    child
}

fn token(out: &str) -> String {
    out.lines().find(|l| l.starts_with("mnnn-x1-")).expect("a token").to_string()
}

#[test]
fn an_author_reviews_comments_suggests_accepts_and_merges_locally() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    e.ok(&["journal", "new", "field notes"]);
    e.ok(&["journal", "write", "field notes", "--title", "Comet", "--text", "the cat over the hill"]);
    assert!(!e.run(&["review", "request", "galaxy", "x"]).status.success());
    let out = e.ok(&["review", "request", "journal", "field notes", "--note", "before Friday", "--allow", "comment,vote"]);
    token(&out);
    let l = json(&e.ok(&["review", "list", "--json"]));
    assert_eq!(l["mine"][0]["allow"], serde_json::json!(["view", "comment", "vote"]));

    let v = json(&e.ok(&["review", "view", "field notes", "--json"]));
    let entry = v["entries"][0]["id"].as_str().unwrap().to_string();
    assert!(e.ok(&["review", "view", "field notes"]).contains("the cat over the hill"));

    e.ok(&["review", "comment", "field notes", "a nice start", "--entry", &entry, "--at", "cat"]);
    e.ok(&["review", "highlight", "field notes", "--entry", &entry, "--at", "hill", "--color", "yellow"]);
    let o = e.run(&["review", "highlight", "field notes", "--entry", &entry, "--at", "moon"]);
    assert!(!o.status.success() && err(&o).contains("moon"), "{}", err(&o));
    e.ok(&["review", "suggest", "field notes", "--entry", &entry, "--at", "the cat", "--to", "the comet", "--why", "it is a comet"]);
    e.ok(&["review", "suggest", "field notes", "--entry", &entry, "--field", "title", "--to", "Comet!"]);
    e.ok(&["review", "vote", "field notes", "approve"]);
    e.ok(&["review", "vote", "field notes", "changes"]);
    let s = json(&e.ok(&["review", "show", "field notes", "--json"]));
    assert_eq!(s["votes"]["changes"], 1, "a vote is changed, not added");
    assert_eq!(s["votes"]["approve"], 0);
    let items = s["items"].as_array().unwrap();
    let comment = items.iter().find(|i| i["kind"] == "comment").unwrap()["id"].as_str().unwrap().to_string();
    let o = e.run(&["review", "accept", "field notes", &comment]);
    assert!(!o.status.success() && err(&o).contains("only suggestions"), "{}", err(&o));
    assert!(e.ok(&["review", "show", "field notes"]).contains("suggest"));

    e.ok(&["review", "accept", "field notes", "--all"]);
    e.ok(&["review", "merge", "field notes"]);
    let v = json(&e.ok(&["review", "view", "field notes", "--json"]));
    assert_eq!(v["entries"][0]["body"], "the comet over the hill");
    assert_eq!(v["entries"][0]["title"], "Comet!");
    let s = json(&e.ok(&["review", "show", "field notes", "--json"]));
    assert!(s["items"].as_array().unwrap().iter().filter(|i| i["kind"] == "suggestion").all(|i| i["state"] == "merged" && i["revision"] == 1));
    let revs = json(&e.ok(&["review", "revisions", "entry", &entry, "--json"]));
    assert_eq!(revs[0]["rev"], 1);
    assert_eq!(revs[0]["before"]["body"], "the cat over the hill");
    assert!(e.ok(&["review", "revisions", "entry", &entry]).contains("→ the comet over the hill"));

    // the edit and the review are in history under the author
    let h = e.ok(&["history", "list", "--limit", "40"]);
    for k in ["review.request", "review.suggest", "review.accept", "review.merge", "entry.edit"] {
        assert!(h.contains(k), "{k} in {h}");
    }

    // a conflict: the text changed after the suggestion
    e.ok(&["review", "suggest", "field notes", "--entry", &entry, "--at", "hill", "--to", "mountain"]);
    e.ok(&["review", "suggest", "field notes", "--entry", &entry, "--field", "body", "--to", "new body"]);
    e.ok(&["review", "accept", "field notes", "--all"]);
    e.ok(&["journal", "write", "field notes", "--title", "Other", "--text", "x"]);
    let s = json(&e.ok(&["review", "show", "field notes", "--json"]));
    let whole = s["items"].as_array().unwrap().iter().find(|i| i["value"] == "new body").unwrap()["id"].as_str().unwrap().to_string();
    e.ok(&["review", "reject", "field notes", &whole]);
    let o = e.run(&["review", "merge", "field notes"]);
    assert!(o.status.success());
    assert!(json(&e.ok(&["review", "view", "field notes", "--json"]))["entries"][0]["body"] == "the comet over the mountain");

    // records, projects and searches
    e.ok(&["model", "create", "Signals", "--fields", "symbol:text!; price:real!"]);
    e.ok(&["model", "record", "create", "signals", "symbol=BTC", "price=42000"]);
    let recs = json(&e.ok(&["model", "record", "list", "signals", "--json"]));
    let rid = recs[0]["id"].as_str().unwrap().to_string();
    e.ok(&["review", "request", "record", &format!("signals/{rid}"), "--title", "btc call", "--no-link"]);
    e.ok(&["review", "suggest", "btc call", "--field", "price", "--to", "43000", "--json"]);
    e.ok(&["review", "accept", "btc call", "--all"]);
    e.ok(&["review", "merge", "btc call"]);
    assert_eq!(json(&e.ok(&["review", "view", "btc call", "--json"]))["content"]["price"], 43000.0);
    e.ok(&["project", "new", "Comets", "--description", "find comets"]);
    e.ok(&["review", "request", "project", "Comets", "--no-link"]);
    e.ok(&["review", "suggest", "Comets", "--at", "find", "--to", "watch"]);
    e.ok(&["review", "accept", "Comets", "--all"]);
    e.ok(&["review", "merge", "Comets"]);
    assert!(e.ok(&["project", "show", "Comets"]).contains("watch comets"));

    // close, reopen, remove
    e.ok(&["review", "close", "Comets"]);
    assert!(!e.run(&["review", "comment", "Comets", "late"]).status.success(), "closed reviews take nothing");
    assert!(!e.run(&["review", "invite", "Comets"]).status.success());
    e.ok(&["review", "reopen", "Comets"]);
    e.ok(&["review", "invite", "Comets", "--label", "ben"]);
    e.ok(&["--yes", "review", "remove", "Comets"]);
    assert!(!e.ok(&["review", "list"]).contains("Comets"));
    assert!(json(&e.ok(&["review", "revisions", "project", "Comets", "--json"])).as_array().unwrap().len() == 1, "revisions stay");
}

#[test]
fn a_reviewer_joins_by_link_and_reviews_through_the_authors_node() {
    let a = Env::new();
    let b = Env::new();
    let c = Env::new();
    let _na = start(&a, "anna");
    b.ok(&["net", "config", "--label", "ben", "--mdns", "off"]);
    c.ok(&["net", "config", "--label", "cleo", "--mdns", "off"]);
    a.ok(&["journal", "new", "field notes"]);
    a.ok(&["journal", "write", "field notes", "--title", "Comet", "--text", "the cat over the hill"]);
    let out = a.ok(&["review", "request", "journal", "field notes", "--title", "notes review", "--label", "ben", "--allow", "comment,suggest,vote"]);
    let t = token(&out);

    b.ok(&["review", "join", &t]);
    wait("ben did not join", || json(&a.ok(&["review", "show", "notes review", "--json"]))["reviewers"][0]["status"] == "joined");
    let o = c.run(&["review", "join", &t]);
    assert!(!o.status.success(), "a link is for one reviewer");
    assert!(json(&c.ok(&["review", "list", "--json"]))["joined"].as_array().unwrap().is_empty());

    b.ok(&["review", "sync", "notes review"]);
    let v = json(&b.ok(&["review", "view", "notes review", "--json"]));
    let entry = v["entries"][0]["id"].as_str().unwrap().to_string();
    b.ok(&["review", "comment", "notes review", "lovely", "--entry", &entry]);
    b.ok(&["review", "suggest", "notes review", "--entry", &entry, "--at", "cat", "--to", "comet"]);
    let o = b.run(&["review", "highlight", "notes review", "--entry", &entry, "--at", "hill"]);
    assert!(!o.status.success() && err(&o).contains("may not highlight"), "{}", err(&o));
    b.ok(&["review", "vote", "notes review", "approve"]);
    let s = json(&b.ok(&["review", "show", "notes review", "--json"]));
    assert_eq!(s["items"].as_array().unwrap().len(), 3);
    assert!(!b.run(&["review", "merge", "notes review"]).status.success(), "only the author merges");

    a.ok(&["review", "accept", "notes review", "--all"]);
    a.ok(&["review", "merge", "notes review"]);
    let v = json(&b.ok(&["review", "view", "notes review", "--json"]));
    assert_eq!(v["entries"][0]["body"], "the comet over the hill");
    let s = json(&b.ok(&["review", "show", "notes review", "--json"]));
    assert!(s["items"].as_array().unwrap().iter().any(|i| i["state"] == "merged"));

    // a second link for cleo, then closing ends both
    let t2 = token(&a.ok(&["review", "invite", "notes review", "--label", "cleo"]));
    c.ok(&["review", "join", &t2]);
    c.ok(&["review", "leave", "notes review"]);
    wait("cleo did not leave", || {
        json(&a.ok(&["review", "show", "notes review", "--json"]))["reviewers"].as_array().unwrap().iter().any(|r| r["status"] == "left")
    });
    a.ok(&["review", "close", "notes review"]);
    assert!(!b.run(&["review", "comment", "notes review", "more"]).status.success(), "closed");
    assert!(b.ok(&["history", "list"]).contains("review.joined"));
}
