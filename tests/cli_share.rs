//! `mnnn share` end to end.
mod common;
use common::*;

#[test]
fn share_goes_to_projects_journals_and_code_spaces() {
    let e = Env::new();
    e.ok(&["project", "create", "Alpha"]);
    e.ok(&["journal", "new", "Notes"]);
    e.ok(&["search", "save", "usdt", "--query", "usdt"]);
    let t = e.ok(&["share", "search", "--targets"]);
    assert!(t.contains("project") && t.contains("journal") && !t.contains("code "), "{t}");
    e.ok(&["share", "search", "usdt", "project", "Alpha"]);
    assert!(e.ok(&["project", "show", "Alpha"]).contains("usdt"));
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["share", "search", "usdt", "journal", "Notes", "--note", "see this", "--json"])).unwrap();
    assert_eq!(v["target"], "journal");
    let o = e.run(&["share", "project", "Alpha", "project", "Alpha"]);
    assert!(!o.status.success());
}
