//! `mnnn spaces`, `mnnn migrate` and `mnnn status` end to end: line-ups, approval, pins,
//! project items, pipeline files carrying spaces, and migrate's dry run, apply, rerun,
//! `-migration` naming and skips. Nothing here builds code.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

fn math(e: &Env, name: &str, src: &str) {
    e.ok(&["code", "create", name, "--lang", "math", "--source", src]);
}

#[test]
fn spaces_are_managed_approved_pinned_and_kept_in_projects() {
    let e = Env::new();
    math(&e, "mainc", "// @imports dbl\nx = 1\n");
    math(&e, "dbl", "y = 2\n");
    math(&e, "other", "z = 3\n");
    e.ok(&["spaces", "create", "both", "--kind", "analysis", "--main", "mainc", "--add", "dbl"]);
    // names are unique, and colons are refused
    assert!(!e.run(&["spaces", "create", "BOTH", "--kind", "analysis", "--main", "other"]).status.success());
    assert!(!e.run(&["spaces", "create", "a:b", "--kind", "analysis", "--main", "other"]).status.success());
    let list = e.ok(&["spaces", "list"]);
    assert!(list.contains("both") && list.contains("not approved"), "{list}");
    // the aliases reach the same command
    assert_eq!(e.ok(&["space", "list"]), list);
    assert_eq!(e.ok(&["code-spaces", "list"]), list);

    // without -y nothing is approved; with it the whole line-up is
    assert!(!e.run(&["spaces", "approve", "both"]).status.success());
    e.ok(&["-y", "spaces", "approve", "both"]);
    let s = json(&e.ok(&["spaces", "show", "both", "--json"]));
    assert_eq!(s["approved"], true, "{s}");
    assert_eq!(s["main"], "mainc");
    assert_eq!(s["members"].as_array().unwrap().len(), 2);
    // adding a member changes the line-up: approval is asked again
    e.ok(&["spaces", "add", "both", "other", "--as", "oth"]);
    assert_eq!(json(&e.ok(&["spaces", "show", "both", "--json"]))["approved"], false);
    e.ok(&["spaces", "remove-code", "both", "oth"]);
    assert!(!e.run(&["spaces", "remove-code", "both", "mainc"]).status.success(), "main can't be removed");
    e.ok(&["-y", "spaces", "approve", "both"]);
    // an edit runs at once: still approved, the member flagged unreviewed
    e.ok(&["code", "edit", "dbl", "--source", "y = 3\n"]);
    let s = json(&e.ok(&["spaces", "show", "both", "--json"]));
    assert_eq!(s["approved"], true, "{s}");
    assert!(s["members"].as_array().unwrap().iter().any(|m| m["code"] == "dbl" && m["unreviewed"] == true), "{s}");
    // a pin breaks once its code changes
    e.ok(&["spaces", "pin", "both", "dbl"]);
    e.ok(&["code", "edit", "dbl", "--source", "y = 4\n"]);
    let s = json(&e.ok(&["spaces", "show", "both", "--json"]));
    assert!(s["members"].as_array().unwrap().iter().any(|m| m["code"] == "dbl" && m["pin_broken"] == true), "{s}");
    e.ok(&["spaces", "unpin", "both", "dbl"]);
    // code in a space stays
    let o = e.run(&["-y", "code", "remove", "dbl"]);
    assert!(!o.status.success() && err(&o).contains("both"), "{}", err(&o));
    // kind, rename, on/off
    e.ok(&["spaces", "kind", "both", "transform"]);
    e.ok(&["spaces", "rename", "both", "pair"]);
    e.ok(&["spaces", "disable", "pair"]);
    e.ok(&["spaces", "enable", "pair"]);
    let s = json(&e.ok(&["spaces", "show", "pair", "--json"]));
    assert_eq!(s["kind"], "transform");
    assert_eq!(s["enabled"], true);

    // a project item
    e.ok(&["project", "create", "Lab"]);
    e.ok(&["project", "add", "Lab", "--space", "pair"]);
    let p = e.ok(&["project", "show", "Lab"]);
    assert!(p.contains("pair") && p.contains("code space"), "{p}");

    // status: `other` is in no space (to migrate); the space is unapproved
    let st = json(&e.ok(&["status", "--json"]));
    assert_eq!(st["unmigrated_code"], 1, "{st}");
    assert_eq!(st["unapproved_spaces"], 1, "{st}");
    assert_eq!(st["spaces"], 1);

    e.ok(&["-y", "spaces", "remove", "pair"]);
    assert!(!e.ok(&["spaces", "list"]).contains("pair"));
    e.ok(&["-y", "code", "remove", "dbl"]);
}

#[test]
fn migrate_dry_runs_applies_names_skips_and_reruns_safely() {
    let e = Env::new();
    math(&e, "a", "x = 1\n");
    math(&e, "b", "x = 2\n");
    math(&e, "x", "x = 3\n");
    math(&e, "holder", "x = 4\n");
    // `b` is taken as a space name: b-migration. `x` and `x-migration` both: skipped.
    e.ok(&["spaces", "create", "b", "--kind", "analysis", "--main", "holder"]);
    e.ok(&["spaces", "create", "x", "--kind", "analysis", "--main", "holder"]);
    e.ok(&["spaces", "create", "x-migration", "--kind", "analysis", "--main", "holder"]);

    let st = e.ok(&["status"]);
    assert!(st.contains("3 code not in any code space") && st.contains("mnnn migrate"), "{st}");
    assert_eq!(json(&e.ok(&["status", "--json"]))["unmigrated_code"], 3);

    // a dry run reports and changes nothing
    let r = json(&e.ok(&["migrate", "--dry-run", "--kind", "analysis", "--json"]));
    assert_eq!(r["dry_run"], true);
    let created = r["created"].as_array().unwrap();
    assert!(created.iter().any(|c| c["code"] == "a" && c["space"] == "a" && c["renamed"] == false), "{r}");
    assert!(created.iter().any(|c| c["code"] == "b" && c["space"] == "b-migration" && c["renamed"] == true), "{r}");
    assert!(r["skipped"].as_array().unwrap().iter().any(|s| s["code"] == "x"), "{r}");
    assert_eq!(json(&e.ok(&["spaces", "list", "--json"])).as_array().unwrap().len(), 3, "a dry run creates nothing");
    assert!(!e.home().join("backups").exists() || std::fs::read_dir(e.home().join("backups")).unwrap().next().is_none());

    // unclear kinds without --kind (and no terminal) are skipped, not guessed
    let r = json(&e.ok(&["migrate", "--dry-run", "--json"]));
    assert!(r["skipped"].as_array().unwrap().len() >= 1, "{r}");

    // apply: a backup, the spaces, the report
    let out = e.ok(&["-y", "migrate", "--kind", "analysis"]);
    assert!(out.contains("b-migration") && out.contains("x"), "{out}");
    assert!(std::fs::read_dir(e.home().join("backups")).unwrap().any(|f| f.unwrap().file_name().to_string_lossy().starts_with("migrate-")));
    let names: Vec<String> =
        json(&e.ok(&["spaces", "list", "--json"])).as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect();
    for n in ["a", "b", "b-migration", "x", "x-migration"] {
        assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
    }
    assert_eq!(names.len(), 5);

    // again: only the skipped one is left, nothing new is made
    let r = json(&e.ok(&["-y", "migrate", "--kind", "analysis", "--json"]));
    assert!(r["created"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!(json(&e.ok(&["spaces", "list", "--json"])).as_array().unwrap().len(), 5);
    assert_eq!(json(&e.ok(&["status", "--json"]))["unmigrated_code"], 1, "x is still to do");
}

#[test]
fn a_pipeline_file_carries_its_spaces_and_their_members() {
    let a = Env::new();
    a.ok(&["journal", "new", "Notes"]);
    math(&a, "mainc", "// @imports dbl\n@view table\nw2 = words * 2\n");
    math(&a, "dbl", "y = 2\n");
    a.ok(&["spaces", "create", "both", "--kind", "transform", "--main", "mainc", "--add", "dbl"]);
    a.ok(&["spaces", "pin", "both", "dbl"]);
    a.ok(&["pipeline", "create", "flow", "--node", "src=journal:Notes", "--node", "d=space:both", "--node", "out=model:bars", "--edge", "src>d>out"]);
    let file = a.home().join("flow.json");
    let f = file.to_str().unwrap();
    a.ok(&["pipeline", "export", "flow", "--out", f]);
    let x = json(&std::fs::read_to_string(&file).unwrap());
    assert_eq!(x["spaces"][0]["name"], "both", "{x}");
    assert_eq!(x["spaces"][0]["main"], "mainc");
    let code: Vec<&str> = x["code"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert!(code.contains(&"mainc") && code.contains(&"dbl"), "{code:?}");

    let b = Env::new();
    b.ok(&["journal", "new", "Notes"]);
    let plan = b.ok(&["pipeline", "import", f, "--dry-run"]);
    assert!(plan.contains("space both"), "{plan}");
    b.ok(&["-y", "pipeline", "import", f]);
    let s = json(&b.ok(&["spaces", "show", "both", "--json"]));
    assert_eq!(s["approved"], false, "imported spaces wait for approval: {s}");
    assert_eq!(s["main"], "mainc");
    assert!(s["members"].as_array().unwrap().iter().any(|m| m["code"] == "dbl" && !m["pin"].is_null()), "the pin comes along: {s}");
    // a different space of that name: created alongside, the pipeline pointed at it
    let c = Env::new();
    c.ok(&["journal", "new", "Notes"]);
    math(&c, "z", "z = 1\n");
    c.ok(&["spaces", "create", "both", "--kind", "analysis", "--main", "z"]);
    c.ok(&["-y", "pipeline", "import", f]);
    assert!(c.ok(&["spaces", "list"]).contains("both-imported"));
    assert!(c.ok(&["pipeline", "show", "flow"]).contains("space:both-imported"));
}
