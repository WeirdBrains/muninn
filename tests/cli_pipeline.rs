//! `mnnn pipeline` end to end.
mod common;
use common::*;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn feed(e: &Env, args: &[&str], input: &str) -> std::process::Output {
    use std::io::Write;
    let mut c = e
        .cmd()
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

#[test]
fn pipelines_are_validated_stored_run_and_removed() {
    let e = Env::new();
    e.ok(&["model", "create", "Ticks", "--fields", "sym:text; px:real"]);
    e.ok(&["model", "create", "Prices", "--fields", "px:real"]);
    e.ok(&["journal", "new", "Raw"]);
    // a leaf that is not a model table is refused at run time and shown
    e.ok(&["pipeline", "create", "feed", "--node", "in=-+journal:Raw", "--node", "v=view:table:px", "--edge", "in>v"]);
    assert!(e.ok(&["pipeline", "show", "feed"]).contains("every leaf is a model table"));
    let o = feed(&e, &["pipeline", "run", "feed", "-"], "sym,px\nBTC,1\n");
    assert!(!o.status.success() && err(&o).contains("not valid"), "{}", err(&o));
    // fan-out: one input, computed once, into two tables
    e.ok(&["pipeline", "edit", "feed", "--node", "t=model:Ticks", "--node", "p=model:Prices", "--edge", "in>t", "--edge", "v>p"]);
    assert!(!e.ok(&["pipeline", "show", "feed"]).contains('!'));
    let o = feed(&e, &["pipeline", "run", "feed", "-", "--chunk", "2", "--json"], "sym,px\nBTC,1\nETH,2\nSOL,3\n");
    assert!(o.status.success(), "{}", err(&o));
    let n = |m: &str| serde_json::from_str::<serde_json::Value>(&e.ok(&["model", "record", "list", m, "--json"])).unwrap().as_array().unwrap().len();
    assert_eq!((n("Ticks"), n("Prices")), (3, 3));
    assert!(e.ok(&["pipeline", "list"]).contains("feed  v2"));
    let h = e.ok(&["history", "list", "--json"]);
    assert!(h.contains("pipeline.create") && h.contains("pipeline.edit") && h.contains("pipe.hop"), "{h}");
    // a cycle
    let o = e.run(&[
        "pipeline",
        "create",
        "loop",
        "--node",
        "s=-",
        "--node",
        "a=code:x",
        "--node",
        "b=code:y",
        "--node",
        "m=model:Ticks",
        "--edge",
        "s>a>b>a",
        "--edge",
        "b>m",
    ]);
    assert!(o.status.success());
    assert!(e.ok(&["pipeline", "show", "loop"]).contains("cycle"));
    e.ok(&["-y", "pipeline", "remove", "loop"]);
    assert!(!e.ok(&["pipeline", "list"]).contains("loop"));
}

#[test]
fn pipelines_export_and_import_with_a_plan() {
    let a = Env::new();
    a.ok(&["model", "create", "Doubled", "--fields", "w2:real"]);
    a.ok(&["journal", "new", "Notes"]);
    a.ok(&["code", "create", "double", "--lang", "math", "--source", "@view table\nw2 = words * 2\n"]);
    a.ok(&[
        "pipeline",
        "create",
        "flow",
        "--node",
        "src=journal:Notes",
        "--node",
        "d=code:double",
        "--node",
        "out=model:Doubled",
        "--edge",
        "src>d>out",
    ]);
    let file = a.home().join("flow.json");
    let f = file.to_str().unwrap();
    a.ok(&["pipeline", "export", "flow", "--out", f]);
    let b = Env::new();
    b.ok(&["journal", "new", "Notes"]);
    let plan = b.ok(&["pipeline", "import", f, "--dry-run"]);
    assert!(plan.contains("needs approval") && plan.contains("embedded template"), "{plan}");
    assert!(b.run(&["pipeline", "show", "flow"]).status.code() != Some(0), "a dry run imports nothing");
    b.ok(&["-y", "pipeline", "import", f]);
    assert!(!b.ok(&["pipeline", "show", "flow"]).contains('!'));
    let c: serde_json::Value = serde_json::from_str(&b.ok(&["code", "show", "double", "--json"])).unwrap();
    assert!(c["approved_hash"].is_null(), "imported code is not approved: {c}");
    // a clash: refused, then renamed, then merged
    assert!(!b.run(&["-y", "pipeline", "import", f]).status.success());
    b.ok(&["-y", "pipeline", "import", f, "--name", "flow2"]);
    b.ok(&["-y", "pipeline", "import", f, "--merge"]);
    let l = b.ok(&["pipeline", "list"]);
    assert!(l.contains("flow  v1") && l.contains("flow2  v1"), "the same graph again is no new version: {l}");
    assert!(b.ok(&["history", "list"]).contains("imported"));
}
