//! `mnnn share` end to end.
mod common;
use common::*;

fn has_cargo() -> bool {
    std::process::Command::new("cargo").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

fn nums(v: &serde_json::Value) -> Vec<f64> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default()
}

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

#[test]
fn standard_input_is_a_pipe_source_in_one_go_or_in_chunks() {
    use std::io::Write;
    let e = Env::new();
    e.ok(&["journal", "new", "Feed"]);
    e.ok(&["model", "create", "Ticks", "--fields", "sym:text; px:real"]);
    let feed = |args: &[&str], input: &str| {
        let mut c = e
            .cmd()
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("MUNINN_CODE_TARGET", std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("code-target"))
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        c.wait_with_output().unwrap()
    };
    // raw: no parser, the text arrives as it is, one `text` column
    let o = feed(&["pipe", "-", "--json"], "sym,px\nBTC,1\n");
    assert!(o.status.success(), "{}", err(&o));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(j["output"]["columns"]["text"], serde_json::json!(["sym,px\nBTC,1\n"]), "{j}");
    // raw lines in chunks of 2: three pipe runs, each recorded with its hops
    let o = feed(&["pipe", "-", "--to", "journal:Feed", "--chunk", "2"], "a\nb\nc\nd\ne\n");
    assert!(o.status.success(), "{}", err(&o));
    let h = e.ok(&["history", "list", "--json"]);
    let hist: serde_json::Value = serde_json::from_str(&h).unwrap();
    let kinds: Vec<&str> = hist.as_array().unwrap().iter().filter_map(|a| a["kind"].as_str()).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "pipe.run").count(), 4, "{kinds:?}");
    assert!(kinds.contains(&"pipe.hop") && kinds.contains(&"pipe.start"));
    assert!(!feed(&["pipe", "journal:Feed", "--format", "csv"], "").status.success(), "--format is for stdin");
    assert!(!feed(&["pipe", "-", "--format", "xml"], "").status.success());
    if !has_cargo() {
        eprintln!("skipped the parser half: no cargo");
        return;
    }
    // --format csv: the built-in parser installed and shown as the first step
    let o =
        feed(&["-y", "pipe", "-", "journal:Feed", "--to", "model:Ticks", "--json", "--format", "csv"], "sym,px\n\"BTC, spot\",42000.5\nETH,3000\n");
    assert!(o.status.success(), "{}", err(&o));
    assert!(err(&o).contains("code:parse-csv"), "{}", err(&o));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(nums(&j["output"]["columns"]["px"]), vec![42000.5, 3000.0], "{j}");
    assert_eq!(j["output"]["columns"]["sym"][0], "BTC, spot", "{j}");
    let recs: serde_json::Value = serde_json::from_str(&e.ok(&["model", "record", "list", "Ticks", "--json"])).unwrap();
    assert_eq!(recs.as_array().unwrap().len(), 2, "{recs}");
    // the other built-ins, and to-bars after a parser
    for (fmt, input) in [("ndjson", "{\"x\":1}\n{\"x\":2}\n"), ("json", "[{\"x\":1},{\"x\":2}]")] {
        let o = feed(&["-y", "pipe", "-", "--json", "--format", fmt], input);
        assert!(o.status.success(), "{fmt}: {}", err(&o));
        let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(nums(&j["output"]["columns"]["x"]), vec![1.0, 2.0], "{fmt}: {j}");
    }
    assert!(e.ok(&["code", "parsers"]).contains("not installed (--install)"));
    e.ok(&["code", "parsers", "--install"]);
    let o = feed(&["-y", "pipe", "-", "code:to-bars", "--json", "--format", "csv"], "time,open,high,low,close,vol\n1700000000000,1,2,0.5,1.5,10\n");
    assert!(o.status.success(), "{}", err(&o));
    let j: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(nums(&j["output"]["columns"]["close"]), vec![1.5], "{j}");
    // the templates are ordinary code spaces: shown, copied
    assert!(e.ok(&["code", "parsers"]).contains("installed"));
    assert!(e.ok(&["code", "parsers", "--show", "to-bars"]).contains("pub fn run"));
    e.ok(&["code", "create", "my-csv", "--from", "parse-csv"]);
    assert!(e.ok(&["code", "show", "my-csv"]).contains("pub fn run"));
}
