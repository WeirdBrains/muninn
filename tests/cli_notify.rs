//! `mnnn notify` end to end (2.18): send, list, read, mute and a rule that notifies.
mod common;
use common::*;

#[test]
fn notify_list_read_mute_and_rules() {
    let e = Env::new();
    assert!(e.run(&["notify", "list"]).status.success());
    e.ok(&["notify", "send", "Backup done", "--body", "all good", "--level", "warn"]);
    e.ok(&["journal", "new", "Log"]);
    e.ok(&["rule", "create", "Tell", "--on", "journal.create", "--then", "notify \"new journal {name}\" level=error"]);
    e.ok(&["journal", "new", "Field"]);
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["notify", "list", "--unread", "--json"])).unwrap();
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), 2, "{v}");
    assert!(a.iter().any(|n| n["level"] == "error" && n["kind"] == "rule" && n["title"].as_str().unwrap().starts_with("new journal")), "{v}");
    let id = a.iter().find(|n| n["title"] == "Backup done").unwrap()["id"].as_str().unwrap().to_string();
    let show = e.ok(&["notify", "show", &id[..8]]);
    assert!(show.contains("Backup done") && show.contains("all good"), "{show}");
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["notify", "list", "-u", "--json"])).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1, "show marks read");
    e.ok(&["notify", "unread", &id]);
    e.ok(&["notify", "read", "--all"]);
    assert!(e.ok(&["notify", "list", "-u", "--json"]).trim() == "[]");
    e.ok(&["notify", "mute", "review.comment"]);
    assert!(e.ok(&["notify", "kinds"]).contains("review.comment (muted)"));
    assert!(!e.run(&["notify", "mute", "nope"]).status.success());
    e.ok(&["notify", "unmute", "review.comment"]);
    assert!(!e.ok(&["notify", "kinds"]).contains("(muted)"));
}
