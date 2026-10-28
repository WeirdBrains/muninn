//! End-to-end CLI tests. Every test uses its own temporary MUNINN_HOME.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

pub struct Env {
    pub dir: tempfile::TempDir,
}

impl Env {
    pub fn new() -> Self {
        let e = Env { dir: tempfile::tempdir().unwrap() };
        let o = e.run(&["keys", "init", "--json"]);
        // Before milestone 5 `keys init` may not exist; ignore failures there.
        let _ = o;
        e
    }
    pub fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }
    pub fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_mnnn"));
        c.env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("MUNINN_HOME", self.home())
            .env("MUNINN_PASSWORD", "correct horse battery staple")
            .arg("--non-interactive");
        c
    }
    pub fn run(&self, args: &[&str]) -> Output {
        self.cmd().args(args).output().unwrap()
    }
    pub fn ok(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(
            o.status.success(),
            "mnnn {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).to_string()
    }
}

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
    let f = Path::new(&raw).join("BTCUSDT_5m.csv");
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
