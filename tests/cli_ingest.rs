//! End-to-end CLI tests. Every test uses its own temporary MUNINN_HOME.
//!
//! Ingest stores what arrives, byte for byte, in a source; it never asks about symbols
//! and never makes bars. Pipelines (`source:NAME > code:parse-csv > code:to-bars`) do.
mod common;
use common::*;
use std::path::Path;

fn err(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn bars(e: &Env) -> usize {
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["model", "record", "list", "bars", "--json", "--limit", "100000"])).unwrap();
    v.as_array().unwrap().len()
}

#[test]
fn csv_ingest_stores_the_file_as_it_is_and_is_idempotent() {
    let e = Env::new();
    let f = fixture("BTCUSDT_5m.csv");
    let size = std::fs::metadata(&f).unwrap().len();
    let out = e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(out.contains(&format!("stored     {size} bytes (csv) in source binance")), "{out}");
    assert_eq!(bars(&e), 0, "no bars without a pipeline");
    let items = e.ok(&["sources", "data", "binance"]);
    assert!(items.contains("BTCUSDT_5m.csv") && items.contains(&format!("{size} bytes")), "{items}");
    let id = items.split('\t').next().unwrap();
    let o = e.run(&["sources", "cat", "binance", id]);
    assert_eq!(o.stdout, std::fs::read(&f).unwrap(), "byte for byte");
    // identical bytes -> skipped; --force stores again
    let out = e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(out.contains("skipped"), "{out}");
    e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance", "--force"]);
    assert_eq!(e.ok(&["sources", "data", "binance"]).lines().count(), 2);
}

#[test]
fn ingest_never_asks_about_what_the_data_is() {
    // non-interactive: any question would be an error; none is needed
    let e = Env::new();
    let dir = e.dir.path().join("kraken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture("unnamed.csv"), dir.join("unnamed.csv")).unwrap();
    std::fs::write(dir.join("weird.json"), r#"{"not":"bars"}"#).unwrap();
    let o = e.run(&["ingest", "path", dir.to_str().unwrap()]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(!err(&o).to_lowercase().contains("symbol"), "{}", err(&o));
    let list = e.ok(&["sources", "list"]);
    assert!(list.contains("kraken\t") && list.contains("2 item(s)"), "source is the folder's name: {list}");
    // even interactively, with nothing on stdin: no prompt is read, nothing hangs
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_mnnn"));
    c.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("MUNINN_HOME", e.home())
        .env("MUNINN_PASSWORD", "correct horse battery staple")
        .args(["ingest", "csv", "--force", "--path"])
        .arg(dir.join("unnamed.csv"))
        .stdin(std::process::Stdio::null());
    let o = c.output().unwrap();
    assert!(o.status.success(), "{}", err(&o));
}

#[test]
fn old_flags_warn_and_are_ignored() {
    let e = Env::new();
    let f = fixture("unnamed.csv");
    let rej = e.dir.path().join("rejects.csv");
    let o = e.run(&[
        "ingest",
        "csv",
        "--path",
        f.to_str().unwrap(),
        "--source",
        "x",
        "--symbol",
        "btc-usdt",
        "--duration",
        "5m",
        "--map",
        "a=open",
        "--rejects",
        rej.to_str().unwrap(),
        "--delimiter",
        ";",
        "--no-header",
    ]);
    assert!(o.status.success(), "{}", err(&o));
    let w = err(&o);
    for (flag, space) in [
        ("--symbol", "to-bars"),
        ("--duration", "to-bars"),
        ("--map", "to-bars"),
        ("--rejects", "to-bars"),
        ("--delimiter", "parse-csv"),
        ("--no-header", "parse-csv"),
    ] {
        assert!(w.lines().any(|l| l.contains(flag) && l.contains("ignored") && l.contains(space)), "{flag}: {w}");
    }
    assert!(!rej.exists());
    assert_eq!(bars(&e), 0);
    let o = e.run(&["ingest", "json", "--path", f.to_str().unwrap(), "--source", "y", "--records", "data"]);
    assert!(o.status.success() && err(&o).contains("--records is ignored") && err(&o).contains("parse-json"), "{}", err(&o));
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
    let out = e.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", "binance"]);
    assert!(out.contains("skipped"));
}

#[test]
fn a_pipeline_turns_an_ingested_csv_into_bars_and_export_round_trips() {
    let e = Env::new();
    e.bars("ETHUSDT_5m.csv", "binance");
    assert_eq!(bars(&e), 300);
    // running the same pipe again adds nothing
    let o =
        e.run(&["-y", "pipe", "source:binance-ETHUSDT_5m", "code:parse-csv", "code:to-bars?symbol=ETH/USDT&source=binance", "--to", "model:bars"]);
    let all = format!("{}{}", String::from_utf8_lossy(&o.stdout), err(&o));
    assert!(o.status.success() && all.contains("0 new bars, 300 already there"), "{all}");
    let csv = e.ok(&["export", "--format", "csv", "--out", "-", "--symbol", "ETH/USDT"]);
    assert_eq!(csv.lines().count(), 301);
    assert!(csv.starts_with("timestamp,datetime_utc,source,symbol"));
    let json = e.ok(&["export", "--format", "json", "--out", "-", "--from", "2017-08-17 04:00:00", "--to", "2017-08-17 04:10:00"]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 3);
    let pq = e.dir.path().join("out/eth.parquet");
    e.ok(&["export", "--format", "parquet", "--out", pq.to_str().unwrap()]);
    assert!(std::fs::metadata(&pq).unwrap().len() > 100);
    // bad rows go to the output, not into bars; rejects=fail stops instead
    let bad = e.dir.path().join("bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("b.csv"), "time,open,high,low,close,volume\n2020-01-01 00:00:00,1,2,0.5,1,1\n2020-01-01 00:05:00,1,0.1,0.5,1,1\n")
        .unwrap();
    e.ok(&["ingest", "csv", "--path", bad.join("b.csv").to_str().unwrap(), "--source", "bad"]);
    let o = e.run(&["-y", "pipe", "source:bad", "code:parse-csv", "code:to-bars?symbol=SOL/USDT&duration=5m", "--to", "model:bars"]);
    let all = format!("{}{}", String::from_utf8_lossy(&o.stdout), err(&o));
    assert!(o.status.success() && all.contains("Rejected") && all.contains("1 new bars"), "{all}");
    let o = e.run(&["-y", "pipe", "source:bad", "code:parse-csv", "code:to-bars?symbol=SOL/USDT&rejects=fail", "--to", "model:bars"]);
    assert!(!o.status.success(), "{}", err(&o));
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
    std::fs::write(root.join("a/b/XRPUSDT_5m.json"), r#"[{"t":"2024-01-01 00:00:00","o":1}]"#).unwrap();
    std::fs::write(root.join("a/notes.md"), "not data").unwrap();
    let p = root.to_str().unwrap();
    // top level only
    let out = e.ok(&["-y", "ingest", "path", p, "--source", "mix", "--no-recursive"]);
    assert!(out.contains("BTCUSDT_5m.csv") && !out.contains("ETHUSDT_5m.csv"), "{out}");
    // whole tree: csv + json, hidden folder skipped, per-file progress on stderr
    let o = e.run(&["-y", "ingest", "path", "--path", p, "--source", "mix", "--progress"]);
    assert!(o.status.success(), "{}", err(&o));
    let (out, err) = (String::from_utf8_lossy(&o.stdout), err(&o));
    assert!(out.contains("ETHUSDT_5m.csv") && out.contains("(json)") && !out.contains("SOLUSDT"), "{out}");
    assert!(err.contains("[1/3] BTCUSDT_5m.csv") && err.contains("[3/3] a/b/XRPUSDT_5m.json"), "{err}");
    let runs = e.ok(&["ingest", "runs"]);
    assert!(runs.contains("completed · 2/3"), "{runs}"); // BTC was already stored
    assert_eq!(e.ok(&["sources", "data", "mix"]).lines().count(), 3, "BTC was already there");
}

#[test]
fn source_comes_from_the_flag_else_the_folder() {
    let e = Env::new();
    let dir = e.dir.path().join("coinbase");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture("BTCUSDT_5m.csv"), dir.join("binance_BTCUSDT_5m.csv")).unwrap();
    // no --source: the folder's name, not a name inside the file name
    let out = e.ok(&["ingest", "path", dir.to_str().unwrap()]);
    assert!(out.contains("in source coinbase"), "{out}");
    let out = e.ok(&["ingest", "csv", "--path", dir.join("binance_BTCUSDT_5m.csv").to_str().unwrap(), "--source", "kraken"]);
    assert!(out.contains("in source kraken"), "{out}");
    let out = e.ok(&["sources", "list"]);
    let names: Vec<&str> = out.lines().map(|l| l.split('\t').next().unwrap()).collect();
    assert_eq!(names, ["coinbase", "kraken"], "{out}");
}

#[test]
fn a_stored_pipeline_reads_a_source_and_makes_bars_with_to_bars_options() {
    let e = Env::new();
    let f = fixture("unnamed.csv");
    e.ok(&["ingest", "csv", "--path", f.to_str().unwrap(), "--source", "drop"]);
    e.ok(&["-y", "code", "parsers", "--install"]);
    e.ok(&[
        "pipeline",
        "create",
        "btc",
        "--node",
        "src=source:drop",
        "--node",
        "p=code:parse-csv",
        "--node",
        "b=code:to-bars?symbol=BTC/USDT&duration=5m&source=bitstamp",
        "--node",
        "m=model:bars",
        "--edge",
        "src>p>b>m",
    ]);
    let show = e.ok(&["pipeline", "show", "btc"]);
    assert!(show.contains("source:drop") && show.contains("symbol=BTC/USDT") && !show.contains('!'), "{show}");
    let o = e.run(&["-y", "pipeline", "run", "btc"]);
    assert!(o.status.success(), "{}", err(&o));
    let v: serde_json::Value = serde_json::from_str(&e.ok(&["model", "record", "list", "bars", "--json", "--limit", "1000"])).unwrap();
    let rows = v.as_array().unwrap();
    assert_eq!(rows.len(), 60, "{v}");
    assert!(rows.iter().all(|r| r.to_string().contains("BTC")), "{}", rows[0]);
    assert!(e.ok(&["sources", "list"]).lines().any(|l| l.starts_with("bitstamp\t")));
    // an unknown source is refused when the pipeline is checked
    let o = e.run(&["pipeline", "create", "nope", "--node", "s=source:missing", "--node", "m=model:bars", "--edge", "s>m"]);
    let all = format!("{}{}", String::from_utf8_lossy(&o.stdout), err(&o));
    assert!(all.contains("missing"), "{all}");
}
