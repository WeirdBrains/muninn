//! 2.25: rule sets (`rulesets`, `ruleset_rules`) and the scheduler's tables
//! (`schedules`, `schedule_runs`, `scheduler_lease`). The logic is in
//! [`crate::schedule`]; see `docs/schedules.md`.
use super::{Rule, Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub const MIGRATION: &str = r#"
CREATE TABLE rulesets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'standalone'
        CHECK (owner_kind IN ('standalone','project','journal','search','model','org')),
    owner_id TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE ruleset_rules (
    ruleset_id TEXT NOT NULL REFERENCES rulesets(id) ON DELETE CASCADE,
    rule_id TEXT NOT NULL REFERENCES rules(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    PRIMARY KEY (ruleset_id, rule_id)
);
CREATE INDEX ruleset_rules_rule ON ruleset_rules(rule_id);
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('connector','ruleset')),
    plugin_id TEXT REFERENCES code_spaces(id) ON DELETE CASCADE,
    ruleset_id TEXT REFERENCES rulesets(id) ON DELETE SET NULL,
    source_id TEXT REFERENCES sources(id) ON DELETE CASCADE,
    pipeline_id TEXT REFERENCES pipelines(id) ON DELETE SET NULL,
    options TEXT NOT NULL DEFAULT '{}',
    start_at INTEGER,
    start_at_input TEXT,
    ends_at INTEGER,
    ends_at_input TEXT,
    tz TEXT,
    run_every TEXT,
    every_ms INTEGER,
    every_months INTEGER,
    cron TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    overlap TEXT NOT NULL DEFAULT 'skip' CHECK (overlap IN ('skip','queue')),
    catch_up TEXT NOT NULL DEFAULT 'once' CHECK (catch_up IN ('once','all','none')),
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_run_at INTEGER NOT NULL,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK ((run_every IS NULL) <> (cron IS NULL)),
    CHECK (run_every IS NOT 'once' OR (start_at IS NOT NULL AND cron IS NULL
           AND every_ms IS NULL AND every_months IS NULL
           AND ends_at IS NOT NULL)),
    CHECK (ends_at IS NULL OR start_at IS NULL OR ends_at > start_at),
    CHECK (target_kind <> 'connector' OR (plugin_id IS NOT NULL AND source_id IS NOT NULL)),
    CHECK (target_kind <> 'ruleset' OR ruleset_id IS NOT NULL)
);
CREATE INDEX schedules_due ON schedules(enabled, next_run_at);
CREATE TABLE schedule_runs (
    id INTEGER PRIMARY KEY,
    schedule_id TEXT NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending','running','success','failed')),
    timestamp INTEGER NOT NULL,
    attempt INTEGER NOT NULL DEFAULT 1,
    not_before INTEGER NOT NULL DEFAULT 0,
    claimed_by TEXT,
    heartbeat_at INTEGER,
    started_at INTEGER,
    finished_at INTEGER,
    stored INTEGER NOT NULL DEFAULT 0,
    bytes INTEGER NOT NULL DEFAULT 0,
    inserted INTEGER NOT NULL DEFAULT 0,
    piped INTEGER NOT NULL DEFAULT 0,
    ruleset_ran INTEGER NOT NULL DEFAULT 0,
    warnings TEXT NOT NULL DEFAULT '[]',
    error TEXT,
    action_id INTEGER REFERENCES actions(id)
);
CREATE INDEX schedule_runs_schedule ON schedule_runs(schedule_id, timestamp);
CREATE INDEX schedule_runs_open ON schedule_runs(status) WHERE status IN ('pending','running');
CREATE TABLE scheduler_lease (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    holder TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);
"#;

/// Setting: what the 2.25 migration moved (recorded as history on the next open).
pub const MIGRATED_NOTE: &str = "schedules.migrated";

/// A rule set: rules run together, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ruleset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_kind: String,
    pub owner_id: Option<String>,
    pub enabled: bool,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// One schedule (see `docs/schedules.md`): a target, when it first runs, how it repeats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScheduleDef {
    pub id: String,
    pub name: String,
    /// `connector` or `ruleset`.
    pub target_kind: String,
    pub plugin_id: Option<String>,
    pub ruleset_id: Option<String>,
    pub source_id: Option<String>,
    pub pipeline_id: Option<String>,
    pub options: Map<String, Value>,
    pub start_at: Option<i64>,
    pub start_at_input: Option<String>,
    pub ends_at: Option<i64>,
    pub ends_at_input: Option<String>,
    pub tz: Option<String>,
    /// As written (`2 hours 30 minutes`, `15m`) or `once`.
    pub run_every: Option<String>,
    pub every_ms: Option<i64>,
    pub every_months: Option<i64>,
    pub cron: Option<String>,
    pub enabled: bool,
    pub overlap: String,
    pub catch_up: String,
    pub max_attempts: i64,
    pub next_run_at: i64,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ScheduleDef {
    pub fn is_once(&self) -> bool {
        self.run_every.as_deref() == Some("once")
    }
}

/// One run of a schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScheduleRun {
    pub id: i64,
    pub schedule_id: String,
    pub status: String,
    /// When it was due (epoch ms, UTC).
    pub timestamp: i64,
    pub attempt: i64,
    /// A retry waits until then.
    pub not_before: i64,
    pub claimed_by: Option<String>,
    pub heartbeat_at: Option<i64>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub stored: i64,
    pub bytes: i64,
    pub inserted: i64,
    pub piped: i64,
    pub ruleset_ran: i64,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub action_id: Option<i64>,
}

const RS_COLS: &str = "id, name, description, owner_kind, owner_id, enabled, author_id, created_at, updated_at";
const S_COLS: &str = "id, name, target_kind, plugin_id, ruleset_id, source_id, pipeline_id, options, start_at, start_at_input, ends_at, ends_at_input, tz, run_every, every_ms, every_months, cron, enabled, overlap, catch_up, max_attempts, next_run_at, author_id, created_at, updated_at";
const R_COLS: &str = "id, schedule_id, status, timestamp, attempt, not_before, claimed_by, heartbeat_at, started_at, finished_at, stored, bytes, inserted, piped, ruleset_ran, warnings, error, action_id";

fn row_rs(r: &rusqlite::Row) -> rusqlite::Result<Ruleset> {
    Ok(Ruleset {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        owner_kind: r.get(3)?,
        owner_id: r.get(4)?,
        enabled: r.get::<_, i64>(5)? != 0,
        author_id: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

fn row_s(r: &rusqlite::Row) -> rusqlite::Result<ScheduleDef> {
    let o: String = r.get(7)?;
    Ok(ScheduleDef {
        id: r.get(0)?,
        name: r.get(1)?,
        target_kind: r.get(2)?,
        plugin_id: r.get(3)?,
        ruleset_id: r.get(4)?,
        source_id: r.get(5)?,
        pipeline_id: r.get(6)?,
        options: serde_json::from_str(&o).unwrap_or_default(),
        start_at: r.get(8)?,
        start_at_input: r.get(9)?,
        ends_at: r.get(10)?,
        ends_at_input: r.get(11)?,
        tz: r.get(12)?,
        run_every: r.get(13)?,
        every_ms: r.get(14)?,
        every_months: r.get(15)?,
        cron: r.get(16)?,
        enabled: r.get::<_, i64>(17)? != 0,
        overlap: r.get(18)?,
        catch_up: r.get(19)?,
        max_attempts: r.get(20)?,
        next_run_at: r.get(21)?,
        author_id: r.get(22)?,
        created_at: r.get(23)?,
        updated_at: r.get(24)?,
    })
}

fn row_r(r: &rusqlite::Row) -> rusqlite::Result<ScheduleRun> {
    let w: String = r.get(15)?;
    Ok(ScheduleRun {
        id: r.get(0)?,
        schedule_id: r.get(1)?,
        status: r.get(2)?,
        timestamp: r.get(3)?,
        attempt: r.get(4)?,
        not_before: r.get(5)?,
        claimed_by: r.get(6)?,
        heartbeat_at: r.get(7)?,
        started_at: r.get(8)?,
        finished_at: r.get(9)?,
        stored: r.get(10)?,
        bytes: r.get(11)?,
        inserted: r.get(12)?,
        piped: r.get(13)?,
        ruleset_ran: r.get(14)?,
        warnings: serde_json::from_str(&w).unwrap_or_default(),
        error: r.get(16)?,
        action_id: r.get(17)?,
    })
}

pub fn check_name(what: &'static str, name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 80 {
        return Err(Error::invalid(what, format!("`{n}`: give a name (up to 80 characters)")));
    }
    Ok(n.to_string())
}

pub const OWNER_KINDS: [&str; 6] = ["standalone", "project", "journal", "search", "model", "org"];

impl Store {
    // ---------------------------------------------------------------- rule sets

    pub fn rulesets(&self) -> Result<Vec<Ruleset>> {
        let mut st = self.conn.prepare(&format!("SELECT {RS_COLS} FROM rulesets ORDER BY name COLLATE NOCASE"))?;
        Ok(st.query_map([], row_rs)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn ruleset(&self, id: &str) -> Result<Option<Ruleset>> {
        Ok(self.conn.query_row(&format!("SELECT {RS_COLS} FROM rulesets WHERE id = ?1"), [id], row_rs).optional()?)
    }

    pub fn find_ruleset(&self, key: &str) -> Result<Ruleset> {
        super::pick(self.rulesets()?, key, |r| &r.id, |r| &r.name, "rule set", "mnnn ruleset list")
    }

    /// The rule set a project owns (its "Rules"), if any.
    pub fn ruleset_of(&self, owner_kind: &str, owner_id: &str) -> Result<Option<Ruleset>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {RS_COLS} FROM rulesets WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY created_at LIMIT 1"),
                [owner_kind, owner_id],
                row_rs,
            )
            .optional()?)
    }

    pub fn create_ruleset(&self, name: &str, description: &str, owner_kind: &str, owner_id: Option<&str>) -> Result<Ruleset> {
        let name = check_name("rule set", name)?;
        if !OWNER_KINDS.contains(&owner_kind) {
            return Err(Error::invalid("owner", format!("`{owner_kind}` ({})", OWNER_KINDS.join(", "))));
        }
        if self.rulesets()?.iter().any(|r| r.name.eq_ignore_ascii_case(&name)) {
            return Err(Error::invalid("rule set", format!("`{name}` exists already")));
        }
        let now = now_ms();
        let r = Ruleset {
            id: new_id(),
            name,
            description: description.trim().into(),
            owner_kind: owner_kind.into(),
            owner_id: owner_id.map(str::to_string),
            enabled: true,
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            &format!("INSERT INTO rulesets({RS_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8)"),
            params![r.id, r.name, r.description, r.owner_kind, r.owner_id, r.author_id, r.created_at, r.updated_at],
        )?;
        Ok(r)
    }

    pub fn edit_ruleset(&self, id: &str, name: Option<&str>, description: Option<&str>) -> Result<Ruleset> {
        let mut r = self.ruleset(id)?.ok_or_else(|| Error::invalid("rule set", "gone"))?;
        if let Some(n) = name {
            let n = check_name("rule set", n)?;
            if self.rulesets()?.iter().any(|x| x.id != r.id && x.name.eq_ignore_ascii_case(&n)) {
                return Err(Error::invalid("rule set", format!("`{n}` exists already")));
            }
            r.name = n;
        }
        if let Some(d) = description {
            r.description = d.trim().into();
        }
        r.updated_at = now_ms();
        self.conn.execute(
            "UPDATE rulesets SET name = ?2, description = ?3, updated_at = ?4 WHERE id = ?1",
            params![r.id, r.name, r.description, r.updated_at],
        )?;
        Ok(r)
    }

    pub fn set_ruleset_enabled(&self, id: &str, on: bool) -> Result<()> {
        self.conn.execute("UPDATE rulesets SET enabled = ?2, updated_at = ?3 WHERE id = ?1", params![id, on as i64, now_ms()])?;
        Ok(())
    }

    pub fn remove_ruleset(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM rulesets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// The set's rules in run order.
    pub fn ruleset_rules(&self, id: &str) -> Result<Vec<Rule>> {
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT rule_id FROM ruleset_rules WHERE ruleset_id = ?1 ORDER BY position, rule_id")?;
            st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut v = vec![];
        for i in ids {
            if let Some(r) = self.rule(&i)? {
                v.push(r);
            }
        }
        Ok(v)
    }

    /// The rule sets a rule is in.
    pub fn rulesets_of_rule(&self, rule_id: &str) -> Result<Vec<Ruleset>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {} FROM rulesets s JOIN ruleset_rules m ON m.ruleset_id = s.id WHERE m.rule_id = ?1 ORDER BY s.name",
            RS_COLS.split(", ").map(|c| format!("s.{c}")).collect::<Vec<_>>().join(", ")
        ))?;
        Ok(st.query_map([rule_id], row_rs)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Add a rule (at `position`, 1-based; default last). Already in: moved there.
    pub fn add_ruleset_rule(&self, set: &str, rule: &str, position: Option<usize>) -> Result<()> {
        let mut order: Vec<String> = self.ruleset_rules(set)?.into_iter().map(|r| r.id).filter(|i| i != rule).collect();
        let at = position.map(|p| p.saturating_sub(1).min(order.len())).unwrap_or(order.len());
        order.insert(at, rule.to_string());
        self.write_order(set, &order)
    }

    pub fn drop_ruleset_rule(&self, set: &str, rule: &str) -> Result<bool> {
        let n = self.conn.execute("DELETE FROM ruleset_rules WHERE ruleset_id = ?1 AND rule_id = ?2", [set, rule])?;
        let order: Vec<String> = self.ruleset_rules(set)?.into_iter().map(|r| r.id).collect();
        self.write_order(set, &order)?;
        Ok(n > 0)
    }

    fn write_order(&self, set: &str, order: &[String]) -> Result<()> {
        self.conn.execute("DELETE FROM ruleset_rules WHERE ruleset_id = ?1", [set])?;
        for (i, r) in order.iter().enumerate() {
            self.conn.execute("INSERT INTO ruleset_rules(ruleset_id, rule_id, position) VALUES (?1, ?2, ?3)", params![set, r, i as i64 + 1])?;
        }
        self.conn.execute("UPDATE rulesets SET updated_at = ?2 WHERE id = ?1", params![set, now_ms()])?;
        Ok(())
    }

    // ---------------------------------------------------------------- schedules

    pub fn schedules(&self) -> Result<Vec<ScheduleDef>> {
        let mut st = self.conn.prepare(&format!("SELECT {S_COLS} FROM schedules ORDER BY name COLLATE NOCASE"))?;
        Ok(st.query_map([], row_s)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn schedule(&self, id: &str) -> Result<Option<ScheduleDef>> {
        Ok(self.conn.query_row(&format!("SELECT {S_COLS} FROM schedules WHERE id = ?1"), [id], row_s).optional()?)
    }

    pub fn find_schedule(&self, key: &str) -> Result<ScheduleDef> {
        super::pick(self.schedules()?, key, |r| &r.id, |r| &r.name, "schedule", "mnnn schedule list")
    }

    /// Insert (a new id and times are filled when empty) or update a schedule (an upsert:
    /// a REPLACE would cascade-delete its runs).
    pub fn save_schedule(&self, s: &ScheduleDef) -> Result<ScheduleDef> {
        let mut s = s.clone();
        s.name = check_name("schedule", &s.name)?;
        let now = now_ms();
        if s.id.is_empty() {
            s.id = new_id();
            s.created_at = now;
            s.author_id = self.current_author_id()?;
        }
        if self.schedules()?.iter().any(|x| x.id != s.id && x.name.eq_ignore_ascii_case(&s.name)) {
            return Err(Error::invalid("schedule", format!("`{}` exists already", s.name)));
        }
        s.updated_at = now;
        self.conn.execute(
            &format!(
                "INSERT INTO schedules({S_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)\n             ON CONFLICT(id) DO UPDATE SET name = excluded.name, target_kind = excluded.target_kind, plugin_id = excluded.plugin_id, ruleset_id = excluded.ruleset_id, source_id = excluded.source_id, pipeline_id = excluded.pipeline_id, options = excluded.options, start_at = excluded.start_at, start_at_input = excluded.start_at_input, ends_at = excluded.ends_at, ends_at_input = excluded.ends_at_input, tz = excluded.tz, run_every = excluded.run_every, every_ms = excluded.every_ms, every_months = excluded.every_months, cron = excluded.cron, enabled = excluded.enabled, overlap = excluded.overlap, catch_up = excluded.catch_up, max_attempts = excluded.max_attempts, next_run_at = excluded.next_run_at, author_id = excluded.author_id, created_at = excluded.created_at, updated_at = excluded.updated_at"
            ),
            params![
                s.id,
                s.name,
                s.target_kind,
                s.plugin_id,
                s.ruleset_id,
                s.source_id,
                s.pipeline_id,
                serde_json::to_string(&s.options)?,
                s.start_at,
                s.start_at_input,
                s.ends_at,
                s.ends_at_input,
                s.tz,
                s.run_every,
                s.every_ms,
                s.every_months,
                s.cron,
                s.enabled as i64,
                s.overlap,
                s.catch_up,
                s.max_attempts,
                s.next_run_at,
                s.author_id,
                s.created_at,
                s.updated_at
            ],
        )?;
        Ok(s)
    }

    pub fn remove_schedule(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM schedules WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Schedules (enabled) using a code space or a rule set.
    pub fn schedules_using(&self, column: &str, id: &str) -> Result<Vec<ScheduleDef>> {
        if !["plugin_id", "ruleset_id", "source_id", "pipeline_id"].contains(&column) {
            return Err(Error::invalid("schedule", column.to_string()));
        }
        let mut st = self.conn.prepare(&format!("SELECT {S_COLS} FROM schedules WHERE {column} = ?1 ORDER BY name"))?;
        Ok(st.query_map([id], row_s)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    // ---------------------------------------------------------------- runs

    pub fn schedule_runs(&self, schedule_id: &str) -> Result<Vec<ScheduleRun>> {
        let mut st = self.conn.prepare(&format!("SELECT {R_COLS} FROM schedule_runs WHERE schedule_id = ?1 ORDER BY id"))?;
        Ok(st.query_map([schedule_id], row_r)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn schedule_run(&self, id: i64) -> Result<Option<ScheduleRun>> {
        Ok(self.conn.query_row(&format!("SELECT {R_COLS} FROM schedule_runs WHERE id = ?1"), [id], row_r).optional()?)
    }

    /// Runs not finished (pending or running), oldest first.
    pub fn open_runs(&self) -> Result<Vec<ScheduleRun>> {
        let mut st = self.conn.prepare(&format!("SELECT {R_COLS} FROM schedule_runs WHERE status IN ('pending','running') ORDER BY id"))?;
        Ok(st.query_map([], row_r)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn insert_run(&self, r: &ScheduleRun) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO schedule_runs(schedule_id, status, timestamp, attempt, not_before, started_at, finished_at, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![r.schedule_id, r.status, r.timestamp, r.attempt, r.not_before, r.started_at, r.finished_at, r.error],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_run(&self, r: &ScheduleRun) -> Result<()> {
        self.conn.execute(
            "UPDATE schedule_runs SET status = ?2, claimed_by = ?3, heartbeat_at = ?4, started_at = ?5, finished_at = ?6, stored = ?7,
             bytes = ?8, inserted = ?9, piped = ?10, ruleset_ran = ?11, warnings = ?12, error = ?13, action_id = ?14 WHERE id = ?1",
            params![
                r.id,
                r.status,
                r.claimed_by,
                r.heartbeat_at,
                r.started_at,
                r.finished_at,
                r.stored,
                r.bytes,
                r.inserted,
                r.piped,
                r.ruleset_ran,
                serde_json::to_string(&r.warnings)?,
                r.error,
                r.action_id
            ],
        )?;
        Ok(())
    }

    /// Move a pending run to running for `holder`, only if it is still pending (one claim).
    pub fn claim_run(&self, id: i64, holder: &str, now: i64) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE schedule_runs SET status = 'running', claimed_by = ?2, started_at = ?3, heartbeat_at = ?3 WHERE id = ?1 AND status = 'pending'",
            params![id, holder, now],
        )? == 1)
    }

    pub fn heartbeat_run(&self, id: i64, now: i64) -> Result<()> {
        self.conn.execute("UPDATE schedule_runs SET heartbeat_at = ?2 WHERE id = ?1", params![id, now])?;
        Ok(())
    }

    // ---------------------------------------------------------------- the lease

    /// Take or keep the scheduler lease until `until`; false when another holder has it.
    pub fn take_lease(&self, holder: &str, now: i64, until: i64) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT INTO scheduler_lease(id, holder, expires_at) VALUES (1, ?1, ?3)
             ON CONFLICT(id) DO UPDATE SET holder = ?1, expires_at = ?3 WHERE holder = ?1 OR expires_at <= ?2",
            params![holder, now, until],
        )?;
        Ok(n == 1)
    }

    pub fn lease(&self) -> Result<Option<(String, i64)>> {
        Ok(self.conn.query_row("SELECT holder, expires_at FROM scheduler_lease WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
    }

    pub fn release_lease(&self, holder: &str) -> Result<()> {
        self.conn.execute("DELETE FROM scheduler_lease WHERE holder = ?1", [holder])?;
        Ok(())
    }
}

/// The 2.25 move, inside the migration: each project's rules become a rule set owned by
/// the project; each scheduled rule becomes a rule set `NAME (scheduled)` plus a schedule
/// (same interval or cron, the node's zone, next run from its last run), and loses its
/// `schedule`. What moved is kept in [`MIGRATED_NOTE`] for the history.
pub fn upgrade_2_25(c: &rusqlite::Connection) -> rusqlite::Result<()> {
    let now = now_ms();
    let mut taken: Vec<String> = vec![];
    let mut uniq = |n: String| {
        let mut name = n.clone();
        let mut i = 2;
        while taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
            name = format!("{n} {i}");
            i += 1;
        }
        taken.push(name.clone());
        name
    };
    let mut moved_sets = 0;
    let projects: Vec<(String, String)> = {
        let mut st = c.prepare("SELECT id, name FROM projects ORDER BY created_at")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (pid, pname) in projects {
        let rules: Vec<String> = {
            let mut st = c.prepare(
                "SELECT i.ref_id FROM project_items i JOIN rules r ON r.id = i.ref_id WHERE i.project_id = ?1 AND i.kind = 'rule_set' ORDER BY i.added_at, i.ref_id",
            )?;
            st.query_map([&pid], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if rules.is_empty() {
            continue;
        }
        let id = new_id();
        c.execute(
            "INSERT INTO rulesets(id, name, description, owner_kind, owner_id, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, 'project', ?4, 1, ?5, ?5)",
            params![id, uniq(pname.clone()), format!("project {pname}'s rules"), pid, now],
        )?;
        for (i, r) in rules.iter().enumerate() {
            c.execute("INSERT INTO ruleset_rules(ruleset_id, rule_id, position) VALUES (?1, ?2, ?3)", params![id, r, i as i64 + 1])?;
        }
        moved_sets += 1;
    }
    let zone = crate::schedule::local_zone();
    let mut moved_rules = vec![];
    let rows: Vec<(String, String, String, i64, Option<i64>, i64)> = {
        let mut st = c.prepare("SELECT id, name, spec, created_at, last_run_at, enabled FROM rules")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, name, spec, created, last, enabled) in rows {
        let Ok(mut v) = serde_json::from_str::<Value>(&spec) else { continue };
        let Some(sched) = v.get("schedule").cloned().filter(|s| !s.is_null()) else { continue };
        let Ok(sched) = serde_json::from_value::<super::Schedule>(sched) else { continue };
        let set = new_id();
        c.execute(
            "INSERT INTO rulesets(id, name, description, owner_kind, enabled, created_at, updated_at) VALUES (?1, ?2, ?3, 'standalone', 1, ?4, ?4)",
            params![
                set,
                uniq(if name == crate::rules::CERT_WATCH { name.clone() } else { format!("{name} (scheduled)") }),
                format!("rule {name}, scheduled (moved in 2.25)"),
                now
            ],
        )?;
        c.execute("INSERT INTO ruleset_rules(ruleset_id, rule_id, position) VALUES (?1, ?2, 1)", params![set, id])?;
        let (run_every, every_ms, cron, next) = match &sched {
            super::Schedule::Every(ms) => (Some(crate::schedule::long_form(0, *ms)), Some(*ms), None, last.map_or(created.max(0), |l| l + ms)),
            super::Schedule::Cron(cr) => {
                let next = crate::rules::parse_cron(cr).ok().and_then(|p| p.next_after(last.unwrap_or(created))).unwrap_or(now);
                (None, None, Some(cr.clone()), next)
            }
        };
        let mut sname = name.clone();
        let mut i = 2;
        while c.query_row("SELECT count(*) FROM schedules WHERE name = ?1 COLLATE NOCASE", [&sname], |r| r.get::<_, i64>(0))? > 0 {
            sname = format!("{name} {i}");
            i += 1;
        }
        c.execute(
            "INSERT INTO schedules(id, name, target_kind, ruleset_id, options, tz, run_every, every_ms, cron, enabled, next_run_at, created_at, updated_at)
             VALUES (?1, ?2, 'ruleset', ?3, '{}', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![new_id(), sname, set, zone, run_every, every_ms, cron, enabled, next, now],
        )?;
        if let Some(o) = v.as_object_mut() {
            o.remove("schedule");
        }
        c.execute("UPDATE rules SET spec = ?2 WHERE id = ?1", params![id, v.to_string()])?;
        moved_rules.push(name);
    }
    if moved_sets > 0 || !moved_rules.is_empty() {
        c.execute(
            "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, ?2)",
            params![MIGRATED_NOTE, json!({ "project_rulesets": moved_sets, "scheduled_rules": moved_rules }).to_string()],
        )?;
    }
    Ok(())
}
