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
fn code_spaces_take_any_data_piped_in_and_keep_none_of_it() {
    let e = Env::new();
    e.ok(&["journal", "new", "Notes"]);
    e.ok(&["journal", "new", "Out"]);
    e.ok(&["code", "create", "calc", "--lang", "math", "--source", "@view table\nw2 = words * 2\n"]);
    // a journal search is data like any other; sharing runs over it and asks to approve first
    e.ok(&["search", "save", "notes", "--journals", "--query", "x"]);
    let o = e.run(&["share", "search", "notes", "code", "calc"]);
    assert!(!o.status.success() && err(&o).contains("not approved"), "{}", err(&o));
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["code", "show", "calc", "--json"])).unwrap();
    assert!(j["input"].as_object().is_none_or(|o| o.is_empty()), "the space keeps no input: {j}");
    // a default input can name any source (a convenience)
    e.ok(&["code", "edit", "calc", "--input", "search:notes"]);
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["code", "show", "calc", "--json"])).unwrap();
    assert_eq!(j["input"]["from"], "search:notes", "{j}");
    e.ok(&["code", "edit", "calc", "--no-input"]);
    // a pipe without code: a search, saved (tee) to a journal, then filtered and saved again
    e.ok(&["journal", "write", "Notes", "--title", "alpha", "--text", "alpha beta"]);
    e.ok(&["journal", "write", "Notes", "--title", "zed", "--text", "zed"]);
    e.ok(&["search", "save", "all", "--journals"]);
    e.ok(&["search", "save", "alphas", "--journals", "--query", "alpha"]);
    let j: serde_json::Value =
        serde_json::from_str(&e.ok(&["pipe", "search:all", "journal:Out", "search:alphas", "--to", "journal:Out", "--json"])).unwrap();
    assert_eq!(j["saved"].as_array().unwrap().len(), 2, "{j}");
    assert!(j["output"]["columns"]["title"].as_array().unwrap().iter().all(|t| t.as_str().unwrap().contains("alpha")), "{j}");
    // refusals: an unknown step, a cycle of default inputs
    assert!(!e.run(&["pipe", "search:all", "nope:x"]).status.success());
    e.ok(&["code", "create", "a", "--lang", "math", "--input", "code:b", "--source", "x = 1"]);
    e.ok(&["code", "create", "b", "--lang", "math", "--input", "code:a", "--source", "x = 1"]);
    let o = e.run(&["-y", "pipe", "code:a"]);
    assert!(!o.status.success() && err(&o).contains("cycle"), "{}", err(&o));
}
