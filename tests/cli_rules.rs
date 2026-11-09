//! `mnnn rule` end to end: rules fire on actions, test and dry-run explain, projects and
//! archives carry them.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn ingest(e: &Env, file: &str) -> std::process::Output {
    let f = fixture(file);
    let o = e.run(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(o.status.success(), "{}", err(&o));
    o
}

#[test]
fn rules_fire_on_actions_and_explain_themselves() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    e.ok(&["journal", "new", "Log"]);
    e.ok(&["report", "create", "Weekly"]);
    let out = e.home().join("out");
    let to = format!("{}/weekly-{{action}}.html", out.display());
    e.ok(&[
        "rule",
        "create",
        "After import",
        "--on",
        "import",
        "--if",
        "inserted > 0",
        "--then",
        &format!("export-report Weekly to=\"{to}\""),
        "--then",
        "entry Log \"imported {inserted} bars\" title=Import",
        "--then",
        "status Imported {inserted} bars",
    ]);
    // bad input is refused with a reason
    let o = e.run(&["rule", "create", "x", "--on", "imprt", "--then", "status hi"]);
    assert!(!o.status.success() && err(&o).contains("matches no action kind"), "{}", err(&o));
    let o = e.run(&["rule", "create", "x", "--on", "import", "--then", "explode"]);
    assert!(err(&o).contains("unknown action"), "{}", err(&o));
    let o = e.run(&["rule", "create", "x", "--on", "import", "--if", "inserted 3", "--then", "status hi"]);
    assert!(err(&o).contains("operator"), "{}", err(&o));

    let o = ingest(&e, "BTCUSDT_5m.csv");
    assert!(err(&o).contains("rule After import: Imported"), "{}", err(&o));
    let files: Vec<_> = std::fs::read_dir(&out).unwrap().collect();
    assert_eq!(files.len(), 1, "the report was exported");
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "show", "Log", "--json"])).unwrap();
    assert!(j.to_string().contains("imported") && j.to_string().contains("Import"), "{j}");

    let list = e.ok(&["rule", "list"]);
    assert!(list.contains("After import") && list.contains("on\t") && list.contains("1 run"), "{list}");
    let show = e.ok(&["rule", "show", "After import"]);
    assert!(
        show.contains("if:       inserted > 0")
            && show.contains("then:     entry Log")
            && show.contains("last: ok")
            && show.contains("author:   Ada"),
        "{show}"
    );

    // the history has the run, and the actions it took are marked as the rule's
    let h: serde_json::Value = serde_json::from_str(&e.ok(&["history", "list", "--json"])).unwrap();
    let h = h.to_string();
    assert!(h.contains("rule.run") && h.contains("rule_chain") && h.contains("\"origin\":\"rule\""), "{h}");

    // importing again inserts nothing: test says why it does not match
    ingest(&e, "BTCUSDT_5m.csv");
    let t = e.ok(&["rule", "test", "After import"]);
    assert!(t.contains("✓ on import") && t.contains("✗ inserted > 0") && t.contains("no match"), "{t}");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    // dry-run against the first import shows what it would do, and does nothing
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["history", "list", "--json"])).unwrap();
    let import_id = v.as_array().unwrap().iter().filter(|a| a["kind"] == "import").filter_map(|a| a["id"].as_i64()).min().unwrap().to_string();
    let d = e.ok(&["rule", "dry-run", "After import", "--action", &import_id]);
    assert!(d.contains("would export report Weekly") && d.contains("would write entry \"Import\" in Log"), "{d}");
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1, "dry-run changes nothing");

    // off rules do not run; --quiet hides messages
    e.ok(&["rule", "disable", "After import"]);
    assert!(e.ok(&["rule", "list"]).contains("off\t"));
    e.ok(&["rule", "edit", "After import", "--clear-if", "--then", "status again"]);
    let o = ingest(&e, "BTCUSDT_5m.csv");
    assert!(!err(&o).contains("again"));
    e.ok(&["rule", "enable", "After import"]);
    let o = ingest(&e, "BTCUSDT_5m.csv");
    assert!(err(&o).contains("rule After import: again"), "{}", err(&o));
    let f = fixture("BTCUSDT_5m.csv");
    let o = e.run(&["-q", "-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(!err(&o).contains("again"));

    let ev = e.ok(&["rule", "events", "journal.*"]);
    assert!(ev.contains("journal.create") && !ev.contains("import\t"), "{ev}");
    e.ok(&["-y", "rule", "remove", "After import"]);
    assert!(!e.ok(&["rule", "list"]).contains("After import"));
    let kinds = e.ok(&["history", "list"]);
    for k in ["rule.create", "rule.edit", "rule.disable", "rule.enable", "rule.remove"] {
        assert!(kinds.contains(k), "{k}: {kinds}");
    }
}

#[test]
fn rules_live_in_projects_journals_and_searches() {
    let e = Env::new();
    e.ok(&["project", "create", "Halving"]);
    e.ok(&["journal", "new", "Desk"]);
    e.ok(&["rule", "create", "Collect", "--project", "Halving", "--on", "search.save", "--then", "add-to-project"]);
    // a rule in a journal writes there by default, and only for its own journal when --related
    e.ok(&["rule", "create", "Echo", "--journal", "Desk", "--related", "--on", "journal.edit", "--then", "note \"renamed to {name}\""]);
    e.ok(&["search", "save", "btc", "--query", "btc"]);
    let show = e.ok(&["project", "show", "Halving"]);
    assert!(show.contains("Collect") && show.contains("btc"), "{show}");
    assert!(e.ok(&["rule", "list", "--project", "Halving"]).contains("Collect"));
    assert!(e.ok(&["rule", "list", "--journal", "Desk"]).contains("Echo"));
    e.ok(&["journal", "new", "Other"]);
    e.ok(&["journal", "edit", "Other", "--name", "Other2"]);
    e.ok(&["journal", "edit", "Desk", "--name", "Desk2"]);
    let j = e.ok(&["journal", "show", "Desk2", "--json"]);
    assert!(j.contains("renamed to Desk2") && !j.contains("Other2"), "{j}");
    // a loop: a rule that writes an entry on every entry is stopped after one round
    e.ok(&["rule", "create", "Loop", "--journal", "Desk2", "--on", "entry.add", "--then", "entry \"again\""]);
    e.ok(&["journal", "write", "Desk2", "--title", "first", "--text", "x"]);
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["journal", "show", "Desk2", "--json"])).unwrap();
    assert_eq!(j["entries"].as_array().unwrap().len(), 2, "{j}");
    assert!(e.ok(&["rule", "show", "Loop"]).contains("blocked"));
    // add an existing rule to a project; archives carry rules, which arrive switched off
    e.ok(&["project", "add", "Halving", "--rule", "Loop"]);
    let pack = e.home().join("h.mnnp");
    e.ok(&["project", "export", "Halving", "--out", pack.to_str().unwrap()]);
    let o = e.run(&["project", "import", pack.to_str().unwrap(), "--name", "Copy"]);
    assert!(o.status.success(), "{}", err(&o));
    let msg = String::from_utf8_lossy(&o.stdout);
    assert!(msg.contains("2 rules (off)"), "{msg}");
    let list = e.ok(&["rule", "list", "--project", "Copy"]);
    assert_eq!(list.lines().count(), 2, "{list}");
    assert!(list.lines().all(|l| l.contains("\toff\t")), "{list}");
    // removing the journal keeps its rules, standalone and off
    e.ok(&["-y", "journal", "remove", "Desk2"]);
    let echo = e.ok(&["rule", "show", "Echo"]);
    assert!(echo.contains("lives in: standalone") && echo.contains("state:    off"), "{echo}");
}

#[test]
fn scheduled_rules_become_rule_sets_with_schedules() {
    let e = Env::new();
    e.ok(&["journal", "new", "Log"]);
    // bad schedules are refused; no --on and no schedule is refused
    assert!(!e.run(&["rule", "create", "Bad", "--every", "5s", "--then", "status x"]).status.success());
    assert!(!e.run(&["rule", "create", "Bad", "--cron", "0 3 * *", "--then", "status x"]).status.success());
    assert!(!e.run(&["rule", "create", "Bad", "--then", "status x"]).status.success());
    // 2.25: --every / --cron make a rule set and a schedule (deprecated)
    let o = e.run(&["rule", "create", "Hourly", "--every", "1h", "--then", "note Log \"hourly {kind}\""]);
    assert!(o.status.success() && String::from_utf8_lossy(&o.stderr).contains("deprecated"), "{o:?}");
    e.ok(&["rule", "create", "Nightly", "--cron", "0 3 * * *", "--then", "note Log \"nightly {kind}\""]);
    assert!(e.ok(&["ruleset", "show", "Hourly (scheduled)"]).contains("1. Hourly"));
    let list = e.ok(&["schedule", "list"]);
    assert!(list.contains("Hourly") && list.contains("every 1 hour") && list.contains("cron 0 3 * * *"), "{list}");
    // the scheduler runs the hourly one now (run_every alone: right away), not again
    let t = e.ok(&["scheduler", "tick", "--json"]);
    assert!(t.contains("\"succeeded\": [\n    1"), "{t}");
    assert!(e.ok(&["scheduler", "tick", "--json"]).contains("\"started\": []"));
    // rule tick is deprecated and does the same
    let o = e.run(&["rule", "tick"]);
    assert!(o.status.success() && String::from_utf8_lossy(&o.stderr).contains("scheduler tick"));
    assert!(e.ok(&["history", "list"]).contains("rule Hourly on #"));
    assert!(e.ok(&["schedule", "runs", "Hourly"]).contains("success"));
    assert!(e.ok(&["rule", "events"]).contains("schedule"));
}

#[test]
fn rule_sets_and_schedules() {
    let e = Env::new();
    e.ok(&["journal", "new", "Log"]);
    e.ok(&["ruleset", "create", "Daily"]);
    // a rule only in a set reacts to `schedule`
    e.ok(&["rule", "create", "Note", "--ruleset", "Daily", "--then", "note Log \"daily {kind}\""]);
    e.ok(&["rule", "create", "Also", "--on", "import", "--then", "status hi"]);
    e.ok(&["ruleset", "add", "Daily", "Also", "--position", "1"]);
    assert!(e.ok(&["ruleset", "show", "Daily"]).contains("1. Also"));
    e.ok(&["ruleset", "drop", "Daily", "Also"]);
    e.ok(&["ruleset", "run", "Daily"]);
    assert!(e.ok(&["rulesets", "list"]).contains("Daily  1 rule(s), 0 schedule(s)"));
    // parse: what a timing means
    let p = e.ok(&["schedule", "parse", "1h30m", "--start-at", "2099-01-01 09:00", "--tz", "UTC"]);
    assert!(p.contains("every 1 hour 30 minutes") && p.contains("2099-01-01 09:00:00Z") && p.contains("then"), "{p}");
    let m = e.ok(&["schedule", "parse", "1 month", "--start-at", "2099-01-31 09:00", "--tz", "UTC"]);
    assert!(m.contains("2099-02-28 09:00:00Z") && m.contains("2099-03-31 09:00:00Z"), "{m}");
    let o = e.run(&["schedule", "parse", "tomorrow"]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("--start-at"));
    // a past start: refused without a terminal unless --past next-interval; a one-off can only be corrected
    let o = e.run(&["schedule", "add", "late", "1 day", "--ruleset", "Daily", "--start-at", "2020-01-01 09:00", "--tz", "UTC"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("--past next-interval"), "{o:?}");
    let o = e.run(&["schedule", "add", "late", "once", "--ruleset", "Daily", "--start-at", "2020-01-01 09:00"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("corrected"));
    let s = e.ok(&["schedule", "add", "late", "1 day", "--ruleset", "Daily", "--start-at", "2020-01-01 09:00", "--tz", "UTC", "--past", "next-interval"]);
    assert!(s.contains("09:00:00Z"), "{s}");
    e.ok(&["schedule", "add", "weekdays", "--ruleset", "Daily", "--cron", "0 9 * * 1-5", "--tz", "Europe/Oslo", "--for", "2 weeks", "--catch-up", "none"]);
    assert!(e.ok(&["schedule", "show", "weekdays"]).contains("cron `0 9 * * 1-5` in Europe/Oslo"));
    assert!(!e.run(&["schedule", "add", "bad", "1h", "--ruleset", "Daily", "--tz", "Nowhere/Land"]).status.success());
    e.ok(&["schedule", "add", "one", "once", "--ruleset", "Daily", "--start-at", "+1 hour"]);
    // run now, runs, disable / enable, edit
    assert!(e.ok(&["schedule", "run-now", "late"]).contains("success"));
    assert!(e.ok(&["history", "list"]).contains("rule Note on #"));
    e.ok(&["schedule", "disable", "late"]);
    assert!(e.ok(&["schedule", "list"]).contains("late (off)"));
    e.ok(&["schedule", "enable", "late"]);
    let ed = e.ok(&["schedule", "edit", "late", "--every", "2 days", "--overlap", "queue"]);
    assert!(ed.contains("every 2 days") && ed.contains("overlap queue"), "{ed}");
    assert!(e.ok(&["scheduler", "status"]).contains("next:"));
    // a rule set with schedules is not removed without --force
    assert!(!e.run(&["-y", "ruleset", "remove", "Daily"]).status.success());
    e.ok(&["-y", "ruleset", "remove", "Daily", "--force"]);
    assert!(e.ok(&["schedule", "list"]).contains("no schedules") || e.ok(&["schedule", "list"]).is_empty());
}
