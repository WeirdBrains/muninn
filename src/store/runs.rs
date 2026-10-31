//! Import runs and their artifact ledger.
//!
//! Every import (TUI or CLI) gets an `import_runs` row. Each thing a file creates is
//! noted in `import_artifacts` (a new source, a new series, bars, the ingest record).
//! A file that finishes commits its artifacts as `kept`; a cancelled or failed file is
//! rolled back and its artifacts are written as `rolled_back`, so the ledger shows what
//! was discarded. Afterwards `cleanup_run` removes anything left that the run created
//! but that holds no data (state `removed`).
use super::{Store, now_ms};
use crate::error::Result;
use rusqlite::params;

/// One artifact of an import.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    /// `source`, `series`, `bars`, `ingest_record`, `recent`, ...
    pub kind: String,
    /// What it refers to (source id, `source|BASE/QUOTE|5m`, row id, path, ...).
    pub reference: String,
    pub file: String,
    /// `kept`, `rolled_back` or `removed`.
    pub state: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportRun {
    pub id: i64,
    pub origin: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub files_total: i64,
    pub files_done: i64,
    pub cancel_requested_at: Option<i64>,
    pub cancel_by: Option<String>,
    pub cancel_file: Option<String>,
    pub cancel_phase: Option<String>,
    pub note: Option<String>,
}

pub const MIGRATION: &str = r#"
CREATE TABLE import_runs (
    id INTEGER PRIMARY KEY,
    origin TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    files_total INTEGER NOT NULL DEFAULT 0,
    files_done INTEGER NOT NULL DEFAULT 0,
    cancel_requested_at INTEGER,
    cancel_by TEXT,
    cancel_file TEXT,
    cancel_phase TEXT,
    note TEXT
);
CREATE TABLE import_artifacts (
    run_id INTEGER NOT NULL REFERENCES import_runs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    reference TEXT NOT NULL,
    file TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX import_artifacts_run ON import_artifacts(run_id);
"#;

impl Store {
    /// Start a run (`origin` = `tui` or `cli`), status `running`.
    pub fn begin_import_run(&self, origin: &str, files_total: usize) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO import_runs(origin, status, started_at, files_total) VALUES (?1, 'running', ?2, ?3)",
            params![origin, now_ms(), files_total as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_artifact(&self, run: i64, a: &Artifact) -> Result<()> {
        self.conn.execute(
            "INSERT INTO import_artifacts(run_id, kind, reference, file, state, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![run, a.kind, a.reference, a.file, a.state, now_ms()],
        )?;
        Ok(())
    }

    /// Note a cancel: when it was asked for, by whom, during which file and phase.
    pub fn record_cancel(&self, run: i64, requested_at: i64, by: &str, file: &str, phase: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE import_runs SET status = 'cancelled', cancel_requested_at = ?2, cancel_by = ?3, cancel_file = ?4, cancel_phase = ?5 WHERE id = ?1",
            params![run, requested_at, by, file, phase],
        )?;
        Ok(())
    }

    /// Finish a run. A cancelled run stays `cancelled` whatever `status` says.
    /// The run is also recorded in the action history (kind `import`).
    pub fn finish_import_run(&self, run: i64, status: &str, files_done: usize, note: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE import_runs SET status = CASE WHEN status = 'cancelled' THEN status ELSE ?2 END,
                    finished_at = ?3, files_done = ?4, note = ?5 WHERE id = ?1",
            params![run, status, now_ms(), files_done as i64, note],
        )?;
        if let Some(r) = self.import_run(run)? {
            let bars: i64 = self.conn.query_row(
                "SELECT COALESCE(SUM(i.inserted), 0) FROM import_artifacts a JOIN ingest_runs i ON i.id = CAST(a.reference AS INTEGER)
                 WHERE a.run_id = ?1 AND a.kind = 'ingest_record' AND a.state = 'kept'",
                [run],
                |r| r.get(0),
            )?;
            let files: Vec<String> = {
                let mut st =
                    self.conn.prepare("SELECT DISTINCT file FROM import_artifacts WHERE run_id = ?1 AND kind = 'ingest_record' ORDER BY rowid")?;
                st.query_map([run], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let what = match files.as_slice() {
                [one] => std::path::Path::new(one).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| one.clone()),
                _ => format!("{}/{} file(s)", r.files_done, r.files_total),
            };
            let a = super::NewAction::new("import", &r.origin, format!("import {what} · {bars} bars · {} · run #{run}", r.status)).params(
                serde_json::json!({
                    "run": run, "status": r.status, "files_done": r.files_done, "files_total": r.files_total, "files": files, "inserted": bars,
                }),
            );
            self.note_action(&a);
        }
        Ok(())
    }

    pub fn import_run(&self, id: i64) -> Result<Option<ImportRun>> {
        let mut st = self.conn.prepare(
            "SELECT id, origin, status, started_at, finished_at, files_total, files_done, cancel_requested_at, cancel_by, cancel_file, cancel_phase, note
             FROM import_runs WHERE id = ?1",
        )?;
        let mut rows = st.query_map([id], |r| {
            Ok(ImportRun {
                id: r.get(0)?,
                origin: r.get(1)?,
                status: r.get(2)?,
                started_at: r.get(3)?,
                finished_at: r.get(4)?,
                files_total: r.get(5)?,
                files_done: r.get(6)?,
                cancel_requested_at: r.get(7)?,
                cancel_by: r.get(8)?,
                cancel_file: r.get(9)?,
                cancel_phase: r.get(10)?,
                note: r.get(11)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn last_import_run(&self) -> Result<Option<ImportRun>> {
        let id: Option<i64> = self.conn.query_row("SELECT MAX(id) FROM import_runs", [], |r| r.get(0))?;
        match id {
            Some(i) => self.import_run(i),
            None => Ok(None),
        }
    }

    pub fn import_artifacts(&self, run: i64) -> Result<Vec<Artifact>> {
        let mut st = self.conn.prepare("SELECT kind, reference, file, state FROM import_artifacts WHERE run_id = ?1 ORDER BY rowid")?;
        let v = st
            .query_map([run], |r| Ok(Artifact { kind: r.get(0)?, reference: r.get(1)?, file: r.get(2)?, state: r.get(3)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Remove what the run created but left empty (a new source with no bars and no
    /// series). Returns how many artifacts were removed.
    pub fn cleanup_run(&self, run: i64) -> Result<u64> {
        let mut n = 0;
        let created: Vec<(String, String)> = {
            let mut st = self.conn.prepare("SELECT DISTINCT reference, file FROM import_artifacts WHERE run_id = ?1 AND kind = 'source'")?;
            st.query_map([run], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, file) in created {
            let used: i64 = self.conn.query_row(
                "SELECT (SELECT COUNT(*) FROM series_stats WHERE source_id = ?1) + EXISTS(SELECT 1 FROM m_bars WHERE _source_id = ?1)",
                [&id],
                |r| r.get(0),
            )?;
            let exists: i64 = self.conn.query_row("SELECT COUNT(*) FROM sources WHERE id = ?1", [&id], |r| r.get(0))?;
            if used == 0 && exists > 0 {
                self.conn.execute("DELETE FROM ingest_runs WHERE source_id = ?1", [&id])?;
                self.conn.execute("DELETE FROM sources WHERE id = ?1", [&id])?;
                self.add_artifact(run, &Artifact { kind: "source".into(), reference: id, file, state: "removed".into() })?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Whether a series row exists (to note a new series as an artifact).
    pub fn has_series(&self, source_id: &str, base: &str, quote: &str, duration_s: i64) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM series_stats WHERE source_id = ?1 AND base = ?2 AND quote = ?3 AND duration_s = ?4)",
            params![source_id, base, quote, duration_s],
            |r| r.get(0),
        )?)
    }

    pub fn last_rowid(&self) -> i64 {
        self.conn.last_insert_rowid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_lifecycle_and_cleanup_of_empty_source() {
        let s = Store::open_in_memory().unwrap();
        let run = s.begin_import_run("cli", 2).unwrap();
        let src = s.add_source("empty", "file", None).unwrap();
        s.add_artifact(run, &Artifact { kind: "source".into(), reference: src.id.clone(), file: "a.csv".into(), state: "kept".into() }).unwrap();
        s.record_cancel(run, 123, "cli", "a.csv", "store").unwrap();
        assert_eq!(s.cleanup_run(run).unwrap(), 1);
        assert!(s.source_by_name("empty").unwrap().is_none());
        s.finish_import_run(run, "completed", 0, "x").unwrap();
        let r = s.import_run(run).unwrap().unwrap();
        assert_eq!(
            (r.status.as_str(), r.cancel_by.as_deref(), r.cancel_phase.as_deref(), r.cancel_requested_at),
            ("cancelled", Some("cli"), Some("store"), Some(123))
        );
        let a = s.import_artifacts(run).unwrap();
        assert_eq!(a.last().unwrap().state, "removed");
    }
}
