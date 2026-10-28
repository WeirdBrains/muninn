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

