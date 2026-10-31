//! A 2.14 database upgrades with every bar intact: ids, sources, symbols, periods,
//! timestamps, prices, flags, the fields added to bars and their stamps, versions,
//! refs to bars, projects and history. The fixture and what 2.14 reported about it come
//! from `scripts/make-bars-2.14-fixture.sh`.
mod common;
use common::*;
use serde_json::Value;
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() { copy_dir(&e.path(), &p) } else { std::fs::copy(e.path(), &p).map(|_| ()).unwrap() }
    }
}

fn expected(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(fixture("bars-2.14").join(name)).unwrap()).unwrap()
}

#[test]
fn a_2_14_database_keeps_every_bar() {
    let e = Env::bare();
    copy_dir(&fixture("bars-2.14/home"), &e.home());
    let got = |args: &[&str]| -> Value { serde_json::from_str(&e.ok(args)).unwrap() };
    let bars = got(&["model", "record", "list", "bars", "--limit", "0", "--json"]);
    let want = expected("bars.json");
    assert_eq!(bars.as_array().unwrap().len(), want.as_array().unwrap().len());
    for (g, w) in bars.as_array().unwrap().iter().zip(want.as_array().unwrap()) {
        assert_eq!(g, w, "bar {}", w["id"]);
    }
    assert_eq!(got(&["model", "show", "bars", "--json"]), expected("model.json"));
    assert_eq!(got(&["model", "history", "bars", "--json"]), expected("versions.json"));
    assert_eq!(got(&["model", "record", "list", "picks", "--json"]), expected("picks.json"));
    assert_eq!(got(&["project", "show", "Alpha", "--json"]), expected("project.json"));
    let (h, wh) = (got(&["history", "list", "--json"]), expected("history.json"));
    assert_eq!(h.as_array().unwrap().len(), wh.as_array().unwrap().len());
    for (g, w) in h.as_array().unwrap().iter().zip(wh.as_array().unwrap()) {
        assert_eq!(g, w);
    }
    let csv = e.home().join("bars.csv");
    e.ok(&["-y", "export", "--format", "csv", "--out", csv.to_str().unwrap()]);
    assert_eq!(std::fs::read_to_string(&csv).unwrap(), std::fs::read_to_string(fixture("bars-2.14/bars.csv")).unwrap());
    // and it keeps working: a new import and a stored value on an upgraded bar
    e.ok(&["-y", "ingest", "csv", "--path", fixture("ETHUSDT_5m.csv").to_str().unwrap(), "--source", "bitstamp"]);
    let id = want[0]["id"].as_str().unwrap();
    e.ok(&["model", "record", "update", "bars", "--id", id, "note=after"]);
    let one = got(&["model", "record", "show", "bars", id, "--json"]);
    assert_eq!((one["note"].as_str(), one["close"].clone()), (Some("after"), want[0]["close"].clone()));
}
