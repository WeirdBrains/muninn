//! `mnnn share` end to end.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn share_goes_to_projects_journals_and_code_spaces() {
    let e = Env::new();
    e.ok(&["project", "create", "Alpha"]);
    e.ok(&["journal", "new", "Notes"]);
    e.ok(&["search", "save", "usdt", "--query", "usdt"]);
    let t = e.ok(&["share", "search", "--targets"]);
    assert!(t.contains("project") && t.contains("journal") && t.contains("code"), "{t}");
    e.ok(&["share", "search", "usdt", "project", "Alpha"]);
    assert!(e.ok(&["project", "show", "Alpha"]).contains("usdt"));
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["share", "search", "usdt", "journal", "Notes", "--note", "see this", "--json"])).unwrap();
    assert_eq!(v["target"], "journal");
    let o = e.run(&["share", "project", "Alpha", "project", "Alpha"]);
    assert!(!o.status.success());
}

#[test]
fn data_shared_to_a_code_space_becomes_its_input_and_can_run() {
    let e = Env::new();
    e.ok(&["model", "create", "Trades", "--fields", "price:real; side:text"]);
    e.ok(&["code", "create", "calc", "--lang", "math", "--model", "bars", "--limit", "9", "--source", "x = price * 2\n"]);
    e.ok(&["search", "save", "big", "--model", "Trades", "--where", "price > 3", "--query", "buy"]);
    e.ok(&["share", "search", "big", "code", "calc"]);
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["code", "show", "calc", "--json"])).unwrap();
    assert_eq!(j["input"]["model"], "trades", "{j}");
    assert_eq!(j["input"]["where"][0], "price > 3", "{j}");
    assert_eq!(j["input"]["text"], "buy", "{j}");
    assert!(j["input"]["limit"].is_null(), "the search replaces the whole input: {j}");
    // a series search is not one model's records
    e.ok(&["search", "save", "usdt", "--query", "usdt"]);
    assert!(!e.run(&["share", "search", "usdt", "code", "calc"]).status.success());
    // --run runs it after; unapproved code doesn't run without a yes
    let o = e.run(&["share", "model", "Trades", "code", "calc", "--run"]);
    assert!(!o.status.success() && err(&o).contains("not approved"), "{}", err(&o));
    // --run is only for code spaces
    e.ok(&["journal", "new", "Notes"]);
    assert!(!e.run(&["share", "model", "Trades", "journal", "Notes", "--run"]).status.success());
}
