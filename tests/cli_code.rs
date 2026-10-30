//! `mnnn code` end to end: create, edit, approve, run, reports, projects, archives and
//! rules. Builds need cargo (skipped without it).
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn code_spaces_are_managed_reported_and_carried_in_archives() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    let check = e.ok(&["code", "check"]);
    assert!(check.contains("cargo") && check.contains("python   built in") && check.contains("ABI      1"), "{check}");
    e.ok(&["project", "create", "Lab"]);
    e.ok(&[
        "code",
        "create",
        "fast",
        "--lang",
        "math",
        "--model",
        "bars",
        "--where",
        "symbol = BTC/USDT",
        "--sort",
        "timestamp",
        "--limit",
        "5",
        "--source",
        "@title Doubled\ndouble = close * 2\n",
        "--project",
        "Lab",
    ]);
    // bad input is refused with a reason
    let o = e.run(&["code", "create", "x", "--lang", "cobol"]);
    assert!(!o.status.success() && err(&o).contains("invalid value"), "{}", err(&o));
    let o = e.run(&["code", "create", "x", "--lang", "math", "--where", "a = 1"]);
    assert!(err(&o).contains("go with --model"), "{}", err(&o));
    let o = e.run(&["code", "create", "fast", "--lang", "math"]);
    assert!(!o.status.success(), "names are unique");

    let list = e.ok(&["code", "list"]);
    assert!(list.contains("fast\tmath\tuser\tnot approved\tnever run"), "{list}");
    let show = e.ok(&["code", "show", "fast"]);
    assert!(show.contains("input:       bars where symbol = BTC/USDT sort timestamp limit 5") && show.contains("author:      Ada"), "{show}");
    assert!(show.contains("double = close * 2"), "{show}");
    assert_eq!(e.ok(&["code", "show", "fast", "--source"]), "@title Doubled\ndouble = close * 2\n");
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["code", "show", "fast", "--json"])).unwrap();
    assert_eq!(j["approved"], false);
    assert_eq!(j["input"]["limit"], 5);

    // without -y (and no terminal) nothing unapproved runs
    let o = e.run(&["code", "run", "fast"]);
    assert!(!o.status.success() && err(&o).contains("not approved"), "{}", err(&o));
    e.ok(&["-y", "code", "approve", "fast"]);
    assert!(e.ok(&["code", "list"]).contains("\tapproved\t"));
    e.ok(&["code", "edit", "fast", "--source", "@title Doubled\ndouble = close * 2\nhalf = close / 2\n", "--description", "two columns"]);
    assert!(e.ok(&["code", "list"]).contains("changed since approved"), "a change needs approving again");
    e.ok(&["code", "edit", "fast", "--no-input"]);
    assert!(e.ok(&["code", "show", "fast"]).contains("input:       none"));
    e.ok(&["code", "edit", "fast", "--model", "bars", "--group", "symbol"]);

    // a missing prebuilt plugin explains itself
    e.ok(&["code", "create", "pre", "--lang", "plugin", "--source", "/nonexistent/libx.so"]);
    let o = e.run(&["-y", "code", "run", "pre"]);
    assert!(!o.status.success() && err(&o).contains("/nonexistent/libx.so"), "{}", err(&o));

    // rules, reports and projects point at it
    e.ok(&["rule", "create", "Recalc", "--on", "journal.create", "--then", "run-code fast"]);
    assert!(e.ok(&["rule", "show", "Recalc"]).contains("run-code fast"));
    let o = e.run(&["rule", "create", "Bad", "--on", "import", "--then", "run-code nope"]);
    assert!(err(&o).contains("nope"), "{}", err(&o));
    e.ok(&["report", "create", "Weekly", "--project", "Lab"]);
    e.ok(&["report", "add", "Weekly", "--code", "fast"]);
    let pre = e.ok(&["report", "preview", "Weekly"]);
    assert!(pre.contains("Code space fast") && pre.contains("Not run yet") && pre.contains("half = close / 2"), "{pre}");
    e.ok(&["project", "add", "Lab", "--code", "pre"]);
    let p = e.ok(&["project", "show", "Lab"]);
    assert!(p.contains("fast") && p.contains("pre"), "{p}");

    // archives carry the source only; imported spaces need approving again
    let pack = e.home().join("lab.mnnp");
    e.ok(&["project", "export", "Lab", "--out", pack.to_str().unwrap()]);
    let msg = e.ok(&["project", "import", pack.to_str().unwrap(), "--name", "Copy"]);
    assert!(msg.contains("2 code spaces (to approve)"), "{msg}");
    let list = e.ok(&["code", "list"]);
    assert!(list.contains("fast (imported)\tmath\tuser\tnot approved"), "{list}");
    let pre = e.ok(&["report", "preview", "Weekly (imported)"]);
    assert!(pre.contains("Code space fast (imported)") && pre.contains("Not run yet"), "the copy points at the copied space: {pre}");

    // the SDK for prebuilt plugins
    let sdk = e.home().join("sdk");
    e.ok(&["code", "sdk", "--out", sdk.to_str().unwrap()]);
    assert!(sdk.join("Cargo.toml").exists() && sdk.join("src/abi.rs").exists());

    e.ok(&["-y", "code", "remove", "pre"]);
    assert!(!e.ok(&["code", "list"]).contains("pre\t"));
    let h = e.ok(&["history", "list"]);
    assert!(h.contains("create code space fast") && h.contains("approve code space fast"), "{h}");
}

#[test]
fn a_math_code_space_builds_runs_and_runs_from_a_rule() {
    let e = Env::new();
    let mut probe = std::process::Command::new("cargo");
    if probe.arg("--version").output().map(|o| !o.status.success()).unwrap_or(true) {
        eprintln!("skipped: no cargo");
        return;
    }
    let run = |args: &[&str]| {
        e.cmd()
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("MUNINN_CODE_TARGET", std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("code-target"))
            .args(args)
            .output()
            .unwrap()
    };
    let f = fixture("BTCUSDT_5m.csv");
    e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    e.ok(&[
        "code",
        "create",
        "fast",
        "--lang",
        "math",
        "--model",
        "bars",
        "--sort",
        "timestamp",
        "--limit",
        "4",
        "--source",
        "@title Doubled\ndouble = close * 2\n",
    ]);
    let o = run(&["-y", "code", "run", "fast"]);
    assert!(o.status.success(), "{}", err(&o));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("== Doubled ==") && out.contains("double"), "{out}");
    let o = run(&["code", "run", "fast", "--json"]);
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(j["status"], "ok", "{j}");
    assert!(e.ok(&["code", "list"]).contains("\tapproved\tok "));
    e.ok(&["report", "create", "Weekly"]);
    e.ok(&["report", "add", "Weekly", "--code", "fast"]);
    let pre = e.ok(&["report", "preview", "Weekly"]);
    assert!(pre.contains("Last run") && pre.contains("Doubled"), "{pre}");
    // a rule runs it after an action
    e.ok(&["rule", "create", "Recalc", "--on", "journal.create", "--then", "run-code fast"]);
    let o = run(&["journal", "new", "Desk"]);
    assert!(o.status.success() && err(&o).contains("rule Recalc: code space fast:"), "{}", err(&o));
    let h = e.ok(&["history", "list"]);
    assert!(h.matches("run code space fast").count() >= 3, "{h}");
}

#[test]
fn code_spaces_have_a_scope() {
    let e = Env::new();
    e.ok(&["author", "add", "--name", "Ada"]);
    e.ok(&["code", "create", "mine", "--lang", "math", "--source", "x = close"]);
    e.ok(&["code", "create", "shared", "--lang", "math", "--source", "y = close"]);
    e.ok(&["org", "create", "Comet Lab"]);
    e.ok(&["org", "resource", "add", "comet-lab", "code", "shared"]);
    let all = e.ok(&["code", "list"]);
    assert!(all.lines().any(|l| l.contains("\tmine\t") && l.contains("\tuser\t")), "{all}");
    assert!(all.lines().any(|l| l.contains("\tshared\t") && l.contains("\torg:comet-lab\t")), "{all}");
    let user = e.ok(&["code", "list", "--scope", "user"]);
    assert!(user.contains("mine") && !user.contains("shared"), "{user}");
    let org = e.ok(&["code", "list", "--scope", "org:comet-lab"]);
    assert!(org.contains("shared") && !org.contains("mine"), "{org}");
    let j: serde_json::Value = serde_json::from_str(&e.ok(&["code", "list", "--scope", "org", "--json"])).unwrap();
    assert_eq!(j[0]["scope"]["kind"], "organization");
    assert!(e.ok(&["code", "list", "--scope", "global"]).contains("no code spaces"));
    assert!(!e.run(&["code", "list", "--scope", "galaxy"]).status.success());
}
