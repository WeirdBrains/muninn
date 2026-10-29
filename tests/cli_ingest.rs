//! End-to-end CLI tests. Every test uses its own temporary MUNINN_HOME.
mod common;
use common::*;
use std::path::Path;

fn field(out: &str, name: &str) -> i64 {
    out.lines()
        .find_map(|l| l.trim().strip_prefix(name).map(|r| r.split_whitespace().next().unwrap().parse().unwrap()))
        .unwrap_or_else(|| panic!("{name} not in {out}"))
}

#[test]
fn csv_ingest_derives_symbol_and_duration_and_is_idempotent() {
    let e = Env::new();
    let f = fixture("BTCUSDT_5m.csv");
    let out = e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(out.contains("binance BTC/USDT 5m"), "{out}");
    assert_eq!(field(&out, "rows"), 402);
    assert_eq!(field(&out, "inserted"), 401);
    assert_eq!(field(&out, "rejected"), 1);
    assert_eq!(field(&out, "gaps"), 2); // the cut between slices + the real 2017-09-06 gap
    assert!(field(&out, "zero-vol") > 0);
    // identical file -> skipped
    let out = e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(out.contains("skipped"), "{out}");
    // forced -> all duplicates
    let out = e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance", "--force"]);
    assert_eq!(field(&out, "inserted"), 0);
    assert_eq!(field(&out, "duplicate"), 401);
}

#[test]
fn rejected_rows_need_confirmation_without_tty() {
    let e = Env::new();
    let f = fixture("BTCUSDT_5m.csv");
    let o = e.run(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--yes"));
}

#[test]
fn missing_source_and_symbol_error_non_interactive() {
    let e = Env::new();
    let f = fixture("unnamed.csv");
    let o = e.run(&["ingest", "csv", "--path", f.to_str().unwrap()]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--source"));
    let o = e.run(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "x"]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("--symbol"));
    let out = e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "x", "--symbol", "btc-usdt"]);
    assert!(out.contains("x BTC/USDT 5m"), "{out}");
    assert_eq!(field(&out, "inserted"), 60);
}

#[test]
fn directory_ingest_and_rejects_file() {
    let e = Env::new();
    let dir = e.dir.path().join("in");
    std::fs::create_dir_all(&dir).unwrap();
    for f in ["BTCUSDT_5m.csv", "ETHUSDT_5m.csv"] {
        std::fs::copy(fixture(f), dir.join(f)).unwrap();
    }
    let rej = e.dir.path().join("rejects.csv");
    let out = e.ok(&["-y", "ingest", "csv", "--path", dir.to_str().unwrap(), "--source", "binance", "--rejects", rej.to_str().unwrap()]);
    assert!(out.contains("ETH/USDT"));
    let r = std::fs::read_to_string(rej).unwrap();
    assert_eq!(r.lines().count(), 2, "{r}");
    let out = e.ok(&["sources", "list"]);
    assert!(out.starts_with("binance\tdirectory"), "{out}");
}

#[test]
fn quiet_prints_nothing() {
    let e = Env::new();
    let f = fixture("ETHUSDT_5m.csv");
    let out = e.ok(&["-q", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert_eq!(out, "");
}

/// Full-file test against the real data: `MUNINN_RAW_DATA=~/Projects/kommandosaldat/raw/data cargo test --release -- --ignored`.
#[test]
#[ignore]
fn full_file_btc_ingest() {
    let raw = std::env::var("MUNINN_RAW_DATA").expect("set MUNINN_RAW_DATA");
    let f = Path::new(&raw).join("binance_BTCUSDT_5m.csv");
    let e = Env::new();
    let t = std::time::Instant::now();
    let out = e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    println!("{out}\nwall: {:?}", t.elapsed());
    assert!(field(&out, "inserted") > 900_000);
    let out = e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(out.contains("skipped"));
}

#[test]
fn export_formats_round_trip() {
    let e = Env::new();
    let f = fixture("ETHUSDT_5m.csv");
    e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    let csv = e.ok(&["export", "--format", "csv", "--out", "-", "--symbol", "ETH/USDT"]);
    assert_eq!(csv.lines().count(), 301);
    assert!(csv.starts_with("timestamp,datetime_utc,source,symbol"));
    let json = e.ok(&["export", "--format", "json", "--out", "-", "--from", "2017-08-17 04:00:00", "--to", "2017-08-17 04:10:00"]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 3);
    let pq = e.dir.path().join("out/eth.parquet");
    e.ok(&["export", "--format", "parquet", "--out", pq.to_str().unwrap()]);
    assert!(std::fs::metadata(&pq).unwrap().len() > 100);
    assert!(!e.run(&["export", "--format", "parquet", "--out", "-"]).status.success());
    // re-ingest the exported CSV under another source: same bars, new ids
    let back = e.dir.path().join("ETHUSDT_5m_export.csv");
    e.ok(&["export", "--format", "csv", "--out", back.to_str().unwrap()]);
    let out = e.ok(&["ingest", "csv", "--path", back.to_str().unwrap(), "--source", "roundtrip"]);
    assert!(out.contains("inserted   300"), "{out}");
}

#[test]
fn ingest_path_recurses_mixed_formats_with_progress() {
    let e = Env::new();
    let root = e.dir.path().join("tree");
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::copy(fixture("BTCUSDT_5m.csv"), root.join("BTCUSDT_5m.csv")).unwrap();
    std::fs::copy(fixture("ETHUSDT_5m.csv"), root.join("a/ETHUSDT_5m.csv")).unwrap();
    std::fs::copy(fixture("ETHUSDT_5m.csv"), root.join(".hidden/SOLUSDT_5m.csv")).unwrap();
    std::fs::write(
        root.join("a/b/XRPUSDT_5m.json"),
        r#"[{"t":"2024-01-01 00:00:00","o":1,"h":2,"l":0.5,"c":1.5,"v":10},{"t":"2024-01-01 00:05:00","o":1.5,"h":2,"l":1,"c":1.2,"v":5}]"#,
    )
    .unwrap();
    std::fs::write(root.join("a/notes.md"), "not data").unwrap();
    let p = root.to_str().unwrap();
    // top level only
    let out = e.ok(&["-y", "ingest", "path", p, "--source", "mix", "--no-recursive"]);
    assert!(out.contains("BTC/USDT") && !out.contains("ETH/USDT"), "{out}");
    // whole tree: csv + json, hidden folder skipped, per-file progress on stderr
    let o = e.run(&["-y", "ingest", "path", "--path", p, "--source", "mix"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let (out, err) = (String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(out.contains("ETH/USDT") && out.contains("XRP/USDT") && !out.contains("SOL/USDT"), "{out}");
    assert!(err.contains("[1/3] BTCUSDT_5m.csv") && err.contains("[3/3] a/b/XRPUSDT_5m.json"), "{err}");
    // one tracked run (the BTC file was already imported above)
    let runs = e.ok(&["ingest", "runs"]);
    assert!(runs.contains("completed · 2/3"), "{runs}"); // BTC was skipped: already imported
    // csv subcommand recurses too
    let out = e.ok(&["-y", "ingest", "csv", "--path", p, "--source", "mix2"]);
    assert!(out.contains("mix2 ETH/USDT"), "{out}");
}

#[test]
fn source_comes_from_the_flag_else_the_exchange_in_the_file_name() {
    let e = Env::new();
    let dir = e.dir.path().join("in");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture("BTCUSDT_5m.csv"), dir.join("binance_BTCUSDT_5m.csv")).unwrap();
    std::fs::copy(fixture("ETHUSDT_5m.csv"), dir.join("Coinbase-ETHUSDT_5m.csv")).unwrap();
    // no --source: each file's exchange is taken from its name
    let out = e.ok(&["-y", "ingest", "path", dir.to_str().unwrap()]);
    assert!(out.contains("binance BTC/USDT 5m") && out.contains("coinbase ETH/USDT 5m"), "{out}");
    // --source wins over the name (records where the data came from, e.g. an upload)
    let out = e.ok(&["-y", "ingest", "csv", "--path", dir.join("binance_BTCUSDT_5m.csv").to_str().unwrap(), "--source", "kraken", "--force"]);
    assert!(out.contains("kraken BTC/USDT 5m"), "{out}");
    let out = e.ok(&["sources", "list"]);
    for s in ["binance", "coinbase", "kraken"] {
        assert!(out.contains(s), "{out}");
    }
}
