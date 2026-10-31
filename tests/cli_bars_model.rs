//! Bars live in the bars model's table: ingest, exports, sources, backups and project
//! packs all work on it, and values stored in fields added to bars stay with their bars.
mod common;
use common::*;
use serde_json::Value;

fn json(e: &Env, args: &[&str]) -> Value {
    serde_json::from_str(&e.ok(args)).unwrap()
}

fn ingest(e: &Env, file: &str, source: &str) {
    e.ok(&["-y", "ingest", "csv", "--path", fixture(file).to_str().unwrap(), "--source", source]);
}

/// The bars with their ids, after a stored value on the first one.
fn with_a_note(e: &Env) -> (String, Value) {
    ingest(e, "BTCUSDT_5m.csv", "binance");
    e.ok(&["model", "edit", "bars", "--add", "note:text", "--apply"]);
    let all = json(e, &["model", "record", "list", "bars", "--limit", "0", "--json"]);
    let id = all[0]["id"].as_str().unwrap().to_string();
    e.ok(&["model", "record", "update", "bars", "--id", &id, "note=keep me"]);
    (id, all)
}

#[test]
fn ingest_and_exports_work_on_the_model_table() {
    let e = Env::new();
    let (id, before) = with_a_note(&e);
    let n = before.as_array().unwrap().len();
    // importing the same bars again adds nothing and keeps the stored value
    ingest(&e, "BTCUSDT_5m.csv", "binance");
    let after = json(&e, &["model", "record", "list", "bars", "--limit", "0", "--json"]);
    assert_eq!(after.as_array().unwrap().len(), n);
    let one = json(&e, &["model", "record", "show", "bars", &id, "--json"]);
    assert_eq!(one["note"], "keep me");
    assert_eq!(one["close"], before[0]["close"]);
    // a second source and the series listing over both
    ingest(&e, "ETHUSDT_5m.csv", "bitstamp");
    let total = json(&e, &["model", "show", "bars", "--json"])["records"].as_u64().unwrap() as usize;
    assert!(total > n);
    let csv = e.ok(&["export", "--format", "csv", "--out", "-"]);
    assert_eq!(csv.lines().count(), total + 1);
    // records filtered on a stored field and on a bar column
    let rs = json(&e, &["model", "record", "list", "bars", "--where", "note = keep*", "--json"]);
    assert_eq!(rs.as_array().unwrap().len(), 1);
    // removing a source removes its bars (and their stored values)
    e.ok(&["-y", "sources", "remove", "binance"]);
    assert_eq!(json(&e, &["model", "show", "bars", "--json"])["records"].as_u64().unwrap() as usize, total - n);
    assert!(!e.run(&["model", "record", "show", "bars", &id]).status.success());
}

#[test]
fn backups_and_project_packs_carry_the_bars_and_their_values() {
    let e = Env::new();
    let (id, before) = with_a_note(&e);
    let n = before.as_array().unwrap().len();
    // a backup restores every bar with its stored value
    let bk = e.dir.path().join("bk");
    e.ok(&["backup", "create", "--out", bk.to_str().unwrap()]);
    let e2 = Env::bare();
    e2.ok(&["backup", "restore", "--from", bk.to_str().unwrap()]);
    let restored = json(&e2, &["model", "record", "list", "bars", "--limit", "0", "--json"]);
    assert_eq!(restored, json(&e, &["model", "record", "list", "bars", "--limit", "0", "--json"]));
    assert_eq!(json(&e2, &["model", "record", "show", "bars", &id, "--json"])["note"], "keep me");
    // a project pack with a result brings its bars into another database, same ids
    e.ok(&["project", "create", "Lab"]);
    e.ok(&["project", "add", "Lab", "--result", "binance:BTC/USDT:5m"]);
    let pack = e.dir.path().join("lab.mnnp");
    e.ok(&["project", "export", "Lab", "--out", pack.to_str().unwrap()]);
    let e3 = Env::new();
    e3.ok(&["project", "import", pack.to_str().unwrap()]);
    let got = json(&e3, &["model", "record", "list", "bars", "--limit", "0", "--json"]);
    assert_eq!(got.as_array().unwrap().len(), n);
    let ids = |v: &Value| v.as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(ids(&got), ids(&before));
    assert_eq!(got[0]["close"], before[0]["close"]);
}
