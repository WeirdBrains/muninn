//! Shared helpers for CLI tests.
#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

pub struct Env {
    pub dir: tempfile::TempDir,
}

impl Env {
    /// A fresh home with keys initialized.
    pub fn new() -> Self {
        let e = Self::bare();
        e.ok(&["keys", "init", "--json"]);
        e
    }
    /// A fresh, uninitialized home.
    pub fn bare() -> Self {
        Env { dir: tempfile::tempdir().unwrap() }
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
            // code spaces (the built-in parsers) build into one shared, cached target
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("MUNINN_CODE_TARGET", Path::new(env!("CARGO_TARGET_TMPDIR")).join("code-target"))
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
    /// Bars from a fixture CSV the way a user makes them: `mnnn ingest` stores the file as
    /// it is (in a source of its own, `<source>-<file>`), then a pipe through the built-in
    /// parsers writes them to the bars model under `source`. The symbol comes from the
    /// fixture's name (ETHUSDT_5m.csv: ETH/USDT, 5m).
    pub fn bars(&self, file: &str, source: &str) {
        let f = fixture(file);
        let stem = file.trim_end_matches(".csv");
        let raw = format!("{source}-{stem}");
        self.ok(&["-y", "ingest", "csv", "--path", f.to_str().unwrap(), "--source", &raw]);
        let (pair, dur) = stem.rsplit_once('_').unwrap_or((stem, "5m"));
        let sym = format!("{}/USDT", pair.trim_end_matches("USDT"));
        let to_bars = format!("space:to-bars?symbol={sym}&duration={dur}&source={source}");
        self.ok(&["-y", "code", "parsers", "--install"]);
        self.ok(&["-y", "pipe", &format!("source:{raw}"), "space:parse-csv", &to_bars, "--to", "model:bars", "--rows", "1"]);
    }
}
