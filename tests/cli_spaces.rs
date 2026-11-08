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
    assert!(!r["skipped"].as_array().unwrap().is_empty(), "{r}");

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

/// The deprecated path (2.23 keeps it for one release): `code:NAME` steps still run, with
/// a note pointing at `space:` and `mnnn migrate`. Builds code (skipped without cargo).
#[test]
fn deprecated_code_steps_still_run_with_a_note() {
    let e = Env::new();
    if std::process::Command::new("cargo").arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("skipped: no cargo");
        return;
    }
    let f = fixture("ETHUSDT_5m.csv");
    e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "old-ETHUSDT_5m"]);
    e.ok(&["-y", "code", "parsers", "--install"]);
    let o = e.run(&[
        "-y",
        "pipe",
        "source:old-ETHUSDT_5m",
        "code:parse-csv",
        "code:to-bars?symbol=ETH/USDT&duration=5m&source=old",
        "--to",
        "model:bars",
        "--rows",
        "1",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let note = err(&o);
    assert!(note.contains("code:parse-csv") && note.matches("is deprecated").count() >= 2 && note.contains("mnnn migrate"), "{note}");
    assert!(json(&e.ok(&["status", "--json"]))["bars"].as_i64().unwrap() > 0);
}

#[test]
fn space_files_and_the_share_command_carry_spaces() {
    let a = Env::new();
    math(&a, "mainc", "// @imports dbl\nx = 1\n");
    math(&a, "dbl", "y = 2\n");
    a.ok(&["spaces", "create", "both", "--kind", "analysis", "--main", "mainc", "--add", "dbl"]);
    a.ok(&["spaces", "pin", "both", "dbl"]);
    let file = a.home().join("both.json");
    let f = file.to_str().unwrap();
    a.ok(&["spaces", "export", "both", "--out", f]);
    let x = json(&std::fs::read_to_string(&file).unwrap());
    assert_eq!(x["format"], "muninn-space");
    assert_eq!(x["spaces"][0]["name"], "both");
    // the share command: projects, journals, and where a space can go
    a.ok(&["project", "create", "Lab"]);
    a.ok(&["journal", "new", "Notes"]);
    a.ok(&["share", "space", "both", "project", "Lab"]);
    assert!(a.ok(&["project", "show", "Lab"]).contains("both"));
    a.ok(&["share", "space", "both", "journal", "Notes", "--note", "see"]);
    let t = a.ok(&["share", "space", "--targets"]);
    assert!(t.contains("project") && t.contains("journal") && t.contains("space") && t.contains("net"), "{t}");
    assert!(a.ok(&["share", "journal", "--targets"]).contains("space"), "data goes into spaces");
    // an org takes it as a resource, and can serve it from an endpoint
    a.ok(&["org", "create", "Guild"]);
    a.ok(&["org", "resource", "add", "Guild", "space", "both"]);
    assert!(a.ok(&["org", "resource", "list", "Guild"]).contains("both"));
    a.ok(&["org", "endpoint", "add", "Guild", "both", "--kind", "space", "--target", "both"]);
    assert!(a.ok(&["org", "code", "list", "Guild"]).contains("both\tcode space"));
    a.ok(&["org", "code", "stop", "Guild", "both", "--space"]);
    a.ok(&["org", "code", "serve", "Guild", "both", "--space"]);

    let b = Env::new();
    let plan = b.ok(&["spaces", "import", f, "--dry-run"]);
    assert!(plan.contains("space both") && plan.contains("needs approval"), "{plan}");
    assert!(!b.ok(&["spaces", "list"]).contains("both"), "a dry run imports nothing");
    b.ok(&["-y", "spaces", "import", f]);
    let s = json(&b.ok(&["spaces", "show", "both", "--json"]));
    assert_eq!(s["approved"], false, "{s}");
    assert!(s["members"].as_array().unwrap().iter().any(|m| m["code"] == "dbl" && !m["pin"].is_null()), "{s}");
    // a different space of that name: NAME-imported
    let c = Env::new();
    math(&c, "z", "z = 1\n");
    c.ok(&["spaces", "create", "both", "--kind", "analysis", "--main", "z"]);
    c.ok(&["-y", "spaces", "import", f]);
    assert!(c.ok(&["spaces", "list"]).contains("both-imported"));
    // --name lands it under another name
    c.ok(&["-y", "spaces", "import", f, "--name", "copy"]);
    assert!(c.ok(&["spaces", "list"]).contains("copy"));
}

#[test]
fn rules_run_a_code_space() {
    let e = Env::new();
    math(&e, "mainc", "x = 1\n");
    e.ok(&["spaces", "create", "pair", "--kind", "analysis", "--main", "mainc"]);
    e.ok(&["rule", "create", "Crunch", "--on", "import", "--then", "run-space pair"]);
    let show = e.ok(&["rule", "show", "Crunch"]);
    assert!(show.contains("run-space pair"), "{show}");
    let o = e.run(&["rule", "create", "Bad", "--on", "import", "--then", "run-space nope"]);
    assert!(!o.status.success() && err(&o).contains("nope"), "{}", err(&o));
    let r = json(&e.ok(&["rule", "show", "Crunch", "--json"]));
    assert!(r.to_string().contains("run-space"), "{r}");
}

#[test]
fn connector_template_and_dry_run_flags() {
    let e = Env::new();
    e.ok(&["code", "create", "feed", "--template", "ingest-connector"]);
    let src = e.ok(&["code", "show", "feed"]);
    assert!(src.contains("store_raw") && src.contains("run_ruleset"), "{src}");
    e.ok(&["spaces", "create", "feedsp", "--kind", "analysis", "--main", "feed"]);
    let o = e.run(&["spaces", "run", "feedsp", "--connector-dry-run"]);
    assert!(!o.status.success() && err(&o).contains("ingest-connector"), "{}", err(&o));
    // the context flags need --connector-dry-run
    assert!(!e.run(&["spaces", "run", "feedsp", "--pipeline", "p"]).status.success());
}

/// `top`'s main imports space `inner`; where `inner` already exists (a different one), it
/// lands as `inner-imported` and `top`'s main follows it.
fn top_and_inner(e: &Env) {
    math(e, "innerc", "y = 2\n");
    math(e, "topc", "// @imports space:inner\nx = 1\n");
    e.ok(&["spaces", "create", "inner", "--kind", "analysis", "--main", "innerc"]);
    e.ok(&["spaces", "create", "top", "--kind", "analysis", "--main", "topc"]);
    e.ok(&["spaces", "pin", "top", "topc"]);
}

fn clashing() -> Env {
    let c = Env::new();
    math(&c, "z", "z = 1\n");
    c.ok(&["spaces", "create", "inner", "--kind", "analysis", "--main", "z"]);
    c
}

#[test]
fn renamed_imports_are_rewritten_in_the_mains_that_import_them() {
    let a = Env::new();
    top_and_inner(&a);
    let file = a.home().join("top.json");
    let f = file.to_str().unwrap();
    a.ok(&["spaces", "export", "top", "--out", f]);

    let c = clashing();
    let plan = c.ok(&["spaces", "import", f, "--dry-run"]);
    assert!(plan.contains("inner → inner-imported") && plan.contains("@imports rewritten (space:inner → space:inner-imported as inner)"), "{plan}");
    c.ok(&["-y", "spaces", "import", f]);
    let top = json(&c.ok(&["spaces", "show", "top", "--json"]));
    let main = top["main"].as_str().unwrap().to_string();
    let src = c.ok(&["code", "show", &main]);
    assert!(src.contains("@imports space:inner-imported as inner"), "{src}");
    // the pin follows the rewritten main, and the existing inner is untouched
    assert!(top["members"].as_array().unwrap().iter().any(|m| m["code"] == main.as_str() && !m["pin"].is_null()), "{top}");
    let s = c.ok(&["spaces", "show", "top"]);
    assert!(!s.contains("changed since pinned"), "{s}");
    assert_eq!(json(&c.ok(&["spaces", "show", "inner", "--json"]))["main"], "z");

    // without a clash nothing is rewritten
    let d = Env::new();
    let plan = d.ok(&["spaces", "import", f, "--dry-run"]);
    assert!(!plan.contains("rewritten"), "{plan}");
}

#[test]
fn project_packs_carry_a_rules_space_and_rewrite_renamed_imports() {
    let a = Env::new();
    top_and_inner(&a);
    a.ok(&["project", "create", "Lab"]);
    a.ok(&["rule", "create", "Crunch", "--on", "import", "--then", "run-space top input=action", "--project", "Lab"]);
    let pack = a.home().join("lab.mnnnp");
    a.ok(&["project", "export", "Lab", "--out", pack.to_str().unwrap()]);

    // round trip: the rule's space (and the space it imports) come with the pack
    let b = Env::new();
    let msg = b.ok(&["project", "import", pack.to_str().unwrap()]);
    assert!(msg.contains("code spaces (to approve)"), "{msg}");
    let list = b.ok(&["spaces", "list"]);
    assert!(list.contains("top") && list.contains("inner"), "{list}");
    let r = b.ok(&["rule", "show", "Crunch"]);
    assert!(r.contains("run-space top input=action"), "{r}");

    // a clash: inner lands as inner-imported and top's main follows it
    let c = clashing();
    let msg = c.ok(&["project", "import", pack.to_str().unwrap()]);
    assert!(msg.contains("@imports rewritten"), "{msg}");
    let top = json(&c.ok(&["spaces", "show", "top", "--json"]));
    let src = c.ok(&["code", "show", top["main"].as_str().unwrap()]);
    assert!(src.contains("@imports space:inner-imported as inner"), "{src}");
    assert!(!c.ok(&["spaces", "show", "top"]).contains("changed since pinned"));
}
