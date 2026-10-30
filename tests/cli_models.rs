//! `mnnn model` end to end: design, migrate, roll back, records, rules, archives and backups.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn json(e: &Env, args: &[&str]) -> serde_json::Value {
    serde_json::from_str(&e.ok(args)).unwrap()
}

#[test]
fn models_are_designed_migrated_and_rolled_back() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    e.ok(&["model", "create", "Signals", "--fields", "symbol:text!; price:real!; side:text choices=buy|sell", "--description", "trade ideas"]);
    let list = e.ok(&["model", "list"]);
    assert!(list.contains("bars") && list.contains("signals") && list.contains("v1 applied"), "{list}");
    // bad specs are refused with a reason
    let o = e.run(&["model", "create", "x", "--fields", "a"]);
    assert!(!o.status.success() && err(&o).contains("needs a type"), "{}", err(&o));
    assert!(!e.ok(&["model", "list"]).contains(" x "), "a failed create leaves nothing behind");

    e.ok(&["model", "record", "create", "signals", "symbol=BTC/USDT", "price=42000", "side=buy"]);
    e.ok(&["model", "rec", "add", "signals", "symbol=ETH/USDT", "price=2500", "side=sell"]);
    let o = e.run(&["model", "record", "create", "signals", "symbol=X", "price=1", "side=hold"]);
    assert!(!o.status.success() && err(&o).contains("buy"), "{}", err(&o));
    let o = e.run(&["model", "record", "create", "signals", "symbol=X"]);
    assert!(!o.status.success(), "price is required");

    // edits go to a draft with a plan
    let out = e.ok(&["model", "edit", "signals", "--add", "note:text default=new", "--rename", "side:direction"]);
    assert!(out.contains("+") && out.contains("note") && out.contains("direction"), "{out}");
    let then = e.ok(&["model", "plan", "signals", "--then"]);
    assert!(then.starts_with("model-edit signals ") && then.contains("rename=") && then.contains("add="), "{then}");
    e.ok(&["model", "apply", "signals"]);
    let m = json(&e, &["model", "show", "signals", "--json"]);
    assert_eq!(m["model"]["version"], 2);
    assert_eq!(m["records"], 2);
    let rs = json(&e, &["model", "record", "list", "signals", "--json", "--sort", "-price"]);
    assert_eq!(rs[0]["direction"], "buy", "a rename keeps values: {rs}");
    assert_eq!(rs[0]["note"], "new", "defaults fill new fields: {rs}");
    assert_eq!(rs[0]["_version"], 2, "records are migrated to the new version: {rs}");

    // dropping a field with values asks first
    e.ok(&["model", "edit", "signals", "--drop", "note"]);
    let o = e.run(&["model", "apply", "signals"]);
    assert!(!o.status.success() && err(&o).contains("throws away 2"), "{}", err(&o));
    e.ok(&["-y", "model", "apply", "signals"]);
    let hist = e.ok(&["model", "history", "signals"]);
    assert!(hist.contains("v1") && hist.contains("v3*") && hist.contains("Ada"), "{hist}");
    let dry = e.ok(&["model", "rollback", "signals", "v2", "--dry-run"]);
    assert!(dry.contains("note"), "{dry}");
    e.ok(&["model", "rollback", "signals", "2"]);
    let m = json(&e, &["model", "show", "signals", "--json"]);
    assert_eq!(m["model"]["version"], 4);
    assert!(m["model"]["design"]["fields"].to_string().contains("note"));

    // update by id and by filter, list with filter, group and sort
    let rs = json(&e, &["model", "record", "list", "signals", "--json", "--where", "price > 10000"]);
    assert_eq!(rs.as_array().unwrap().len(), 1);
    let id = rs[0]["id"].as_str().unwrap().to_string();
    e.ok(&["model", "record", "update", "signals", "--id", &id[..8], "note=checked"]);
    assert_eq!(json(&e, &["model", "record", "show", "signals", &id, "--json"])["note"], "checked");
    let o = e.run(&["model", "record", "update", "signals", "--where", "price > 0", "note=all"]);
    assert!(!o.status.success(), "many changes ask first");
    e.ok(&["-y", "model", "record", "update", "signals", "--where", "price > 0", "note=all"]);
    let g = e.ok(&["model", "record", "list", "signals", "--group", "note", "--agg", "sum:price"]);
    assert!(g.contains("all") && g.contains("44500"), "{g}");
    let t = e.ok(&["model", "record", "list", "signals", "--text", "eth"]);
    assert!(t.contains("ETH/USDT") && !t.contains("BTC/USDT"), "{t}");

    // the history has it all, with the author
    let h = e.ok(&["history", "list", "--json"]);
    for k in ["model.create", "model.edit", "model.apply", "model.rollback", "record.create", "record.update"] {
        assert!(h.contains(k), "{k} missing");
    }

    // built-in bars are protected but take new fields
    let o = e.run(&["-y", "model", "remove", "bars"]);
    assert!(!o.status.success() && err(&o).contains("built in"), "{}", err(&o));
    let o = e.run(&["model", "edit", "bars", "--drop", "close"]);
    assert!(!o.status.success(), "bars fields stay");
    e.ok(&["model", "edit", "bars", "--add", "flag:text", "--apply"]);

    // delete by filter, then remove the model
    e.ok(&["-y", "model", "record", "delete", "signals", "--where", "symbol ~ eth"]);
    assert_eq!(json(&e, &["model", "show", "signals", "--json"])["records"], 1);
    let o = e.run(&["model", "remove", "signals"]);
    assert!(!o.status.success());
    e.ok(&["-y", "model", "remove", "signals"]);
    assert!(e.run(&["model", "show", "signals"]).status.code() != Some(0));
    assert!(e.ok(&["history", "list", "--json"]).contains("model.remove"));
}

#[test]
fn rules_feed_models_and_archives_carry_them() {
    let e = Env::new();
    e.ok(&["model", "create", "notes", "--fields", "text:text!; kind:text"]);
    e.ok(&["journal", "new", "Log"]);
    // a rule living in the model: every journal entry becomes a record
    e.ok(&["rule", "create", "Feed", "--model", "notes", "--on", "entry.add", "--then", "record-create text={summary} kind=entry"]);
    assert!(e.ok(&["rule", "list", "--model", "notes"]).contains("Feed"));
    e.ok(&["journal", "write", "Log", "--title", "first", "--text", "hello"]);
    let rs = json(&e, &["model", "record", "list", "notes", "--json"]);
    assert_eq!(rs.as_array().unwrap().len(), 1, "{rs}");
    assert_eq!(rs[0]["kind"], "entry");
    // rules design models too
    e.ok(&[
        "rule",
        "create",
        "Grow",
        "--on",
        "record.create",
        "--if",
        "name = notes",
        "--then",
        "model-edit notes add=\"seen:bool default=false\" apply=yes",
    ]);
    e.ok(&["journal", "write", "Log", "--title", "second", "--text", "again"]);
    let m = json(&e, &["model", "show", "notes", "--json"]);
    assert!(m["model"]["design"]["fields"].to_string().contains("seen"), "{m}");
    assert!(e.ok(&["model", "history", "notes"]).contains("rule Grow"));
    let o = e.run(&["rule", "create", "Bad", "--on", "import", "--then", "record-update notes kind=x"]);
    assert!(!o.status.success() && err(&o).contains("id="), "{}", err(&o));

    // .mnnp carries the model, its records and its rules
    e.ok(&["project", "create", "P"]);
    e.ok(&["project", "add", "P", "--model", "notes"]);
    let pack = e.home().join("p.mnnp");
    e.ok(&["project", "export", "P", "--out", pack.to_str().unwrap()]);
    let e2 = Env::new();
    let o = e2.run(&["project", "import", pack.to_str().unwrap()]);
    assert!(o.status.success(), "{}", err(&o));
    let rs = json(&e2, &["model", "record", "list", "notes", "--json"]);
    assert_eq!(rs.as_array().unwrap().len(), 2, "{rs}");
    assert!(e2.ok(&["model", "history", "notes"]).contains("v2"));
    let rules = e2.ok(&["rule", "list", "--model", "notes"]);
    assert!(rules.contains("Feed") && rules.contains("off"), "{rules}");
    let all = e2.ok(&["rule", "list"]);
    assert!(all.contains("Grow"), "the rule that migrated the model travels with it: {all}");
    // importing again gets a new name
    e2.ok(&["project", "import", pack.to_str().unwrap(), "--name", "P2"]);
    assert!(e2.ok(&["model", "list"]).contains("notes_imported"));

    // backups carry models
    let bk = e.dir.path().join("bk");
    e.ok(&["backup", "create", "--out", bk.to_str().unwrap()]);
    let e3 = Env::bare();
    e3.ok(&["backup", "restore", "--from", bk.to_str().unwrap()]);
    assert_eq!(json(&e3, &["model", "show", "notes", "--json"])["records"], 2);
}

#[test]
fn live_searches_and_reports_show_records() {
    let e = Env::new();
    e.ok(&["model", "create", "trades", "--fields", "symbol:text!; price:real"]);
    e.ok(&["model", "record", "create", "trades", "symbol=BTC/USDT", "price=42000"]);
    e.ok(&["model", "record", "create", "trades", "symbol=ETH/USDT", "price=2500"]);
    e.ok(&["search", "save", "big", "--model", "trades", "--where", "price > 3000"]);
    let run = e.ok(&["search", "run", "big"]);
    assert!(run.contains("BTC/USDT") && !run.contains("ETH/USDT"), "{run}");
    let run = e.ok(&["search", "find", "--records", "--query", "eth"]);
    assert!(run.contains("ETH/USDT") && !run.contains("BTC/USDT"), "{run}");
    let o = e.run(&["search", "find", "--model", "nope"]);
    assert!(!o.status.success(), "an unknown model is refused");

    e.ok(&["report", "create", "Book"]);
    e.ok(&["report", "add", "Book", "--model", "trades", "--where", "price < 3000"]);
    let p = e.ok(&["report", "preview", "Book"]);
    assert!(p.contains("ETH/USDT") && !p.contains("BTC/USDT"), "{p}");
}
