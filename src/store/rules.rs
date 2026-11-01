//! Rules: if-this-then-that over the action history. See `docs/rules.md`.
//!
//! A rule names the actions it reacts to (`on`: action kinds from the history, `*` and
//! `prefix.*` allowed), conditions on the action's fields (`if`), and what to do (`then`).
//! [`crate::rules`] evaluates every enabled rule each time an action is recorded.
//!
//! Where a rule lives: on its own, in a journal or a live search (`owner_kind`/`owner_id`),
//! and in any number of projects (project items of kind `rule_set`). The owner gives
//! then-actions their default target (the journal to write in, the search to run, the
//! project to add to) and goes with it into project archives.
//!
//! The spec is versioned JSON so later phases (the Model Designer) can add conditions and
//! actions without a schema change.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};

pub const MIGRATION: &str = r#"
CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    owner_kind TEXT NOT NULL DEFAULT 'standalone',
    owner_id TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    spec TEXT NOT NULL,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    runs INTEGER NOT NULL DEFAULT 0,
    last_run_at INTEGER,
    last_status TEXT
);
CREATE INDEX rules_owner ON rules(owner_kind, owner_id);
"#;

/// Spec version written by this Muninn.
pub const SPEC_VERSION: u32 = 1;

/// What a rule belongs to (projects hold rules as items, see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OwnerKind {
    #[default]
    Standalone,
    Journal,
    LiveSearch,
    /// A designed model (`docs/models.md`).
    Model,
}

impl OwnerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OwnerKind::Standalone => "standalone",
            OwnerKind::Journal => "journal",
            OwnerKind::LiveSearch => "live_search",
            OwnerKind::Model => "model",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "journal" => OwnerKind::Journal,
            "live_search" => OwnerKind::LiveSearch,
            "model" => OwnerKind::Model,
            _ => OwnerKind::Standalone,
        }
    }
}

/// How a condition compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    #[serde(rename = "=")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
    #[serde(rename = "~")]
    Contains,
    #[serde(rename = "!~")]
    NotContains,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = "?")]
    Exists,
    #[serde(rename = "!?")]
    Missing,
}

impl Op {
    /// Longest first, so `>=` is not read as `>`.
    pub const ALL: [Op; 10] = [Op::Ne, Op::NotContains, Op::Ge, Op::Le, Op::Missing, Op::Eq, Op::Contains, Op::Gt, Op::Lt, Op::Exists];
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "!=",
            Op::Contains => "~",
            Op::NotContains => "!~",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Exists => "?",
            Op::Missing => "!?",
        }
    }
    pub fn unary(self) -> bool {
        matches!(self, Op::Exists | Op::Missing)
    }
}

/// `field op value`, e.g. `inserted > 0`, `format = pdf`, `summary ~ btc`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    #[serde(default)]
    pub value: String,
}

/// What a then-action adds to a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ItemRef {
    /// The live search, journal or report the action was about.
    Subject,
    LiveSearch(String),
    Journal(String),
    Report(String),
    Model(String),
}

/// One then-action. Targets are ids; `None` means "the rule's own" (its journal, live
/// search or project). Texts and paths are templates (`{summary}`, `{date}`, …).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Then {
    /// A message: the TUI status line, stderr for the CLI.
    Status { text: String },
    /// Run a live search on the current data (its match count is reported).
    RunSearch {
        #[serde(default)]
        search: Option<String>,
    },
    ExportReport {
        report: String,
        to: String,
        #[serde(default)]
        format: Option<String>,
    },
    ExportJournal {
        #[serde(default)]
        journal: Option<String>,
        to: String,
        #[serde(default)]
        format: Option<String>,
    },
    AddToProject {
        #[serde(default)]
        project: Option<String>,
        item: ItemRef,
    },
    WriteEntry {
        #[serde(default)]
        journal: Option<String>,
        #[serde(default)]
        title: String,
        text: String,
    },
    AddNote {
        #[serde(default)]
        journal: Option<String>,
        text: String,
    },
    /// Create a model (fields in the designer syntax, applied unless `draft`); nothing
    /// happens if one with the name exists. The name is a template.
    ModelCreate {
        name: String,
        #[serde(default)]
        fields: String,
        #[serde(default)]
        about: String,
        #[serde(default)]
        draft: bool,
    },
    /// Change a model's design: the changes go into its draft, applied at once with `apply`.
    ModelEdit {
        #[serde(default)]
        model: Option<String>,
        ops: Vec<super::FieldOp>,
        #[serde(default)]
        apply: bool,
        #[serde(default)]
        destructive: bool,
    },
    /// Apply a model's draft (a migration); `destructive` allows losing values.
    ModelApply {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        destructive: bool,
    },
    /// Remove a model; `destructive` allows it when it has records.
    ModelRemove {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        destructive: bool,
    },
    /// Create a record; values are templates (`price={close}`).
    RecordCreate {
        #[serde(default)]
        model: Option<String>,
        values: serde_json::Map<String, serde_json::Value>,
    },
    /// Change one record (`id`) or every record meeting `where`.
    RecordUpdate {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        id: Option<String>,
        #[serde(default, rename = "where")]
        filter: Vec<Condition>,
        values: serde_json::Map<String, serde_json::Value>,
    },
    /// Delete one record (`id`) or every record meeting `where`.
    RecordDelete {
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        id: Option<String>,
        #[serde(default, rename = "where")]
        filter: Vec<Condition>,
    },
    /// Run a code space (only once it is approved); its result becomes its last run (2.5+).
    RunCode { space: String },
    /// Check org certificates (2.16, the built-in `cert-expiry-watch`): ACME ones with fewer
    /// than `renew_days` left are renewed by the host; your own notify at `notify_days`
    /// and on expiry. See [`crate::org::tls::watch`].
    CertWatch {
        #[serde(default = "renew_days")]
        renew_days: i64,
        #[serde(default = "notify_days")]
        notify_days: Vec<i64>,
    },
}

fn renew_days() -> i64 {
    30
}

fn notify_days() -> Vec<i64> {
    vec![30, 14, 3]
}

impl Then {
    /// The verb used on the command line (`status`, `search`, `export-report`, …).
    pub fn verb(&self) -> &'static str {
        match self {
            Then::Status { .. } => "status",
            Then::RunSearch { .. } => "search",
            Then::ExportReport { .. } => "export-report",
            Then::ExportJournal { .. } => "export-journal",
            Then::AddToProject { .. } => "add-to-project",
            Then::WriteEntry { .. } => "entry",
            Then::AddNote { .. } => "note",
            Then::ModelCreate { .. } => "model-create",
            Then::ModelEdit { .. } => "model-edit",
            Then::ModelApply { .. } => "model-apply",
            Then::ModelRemove { .. } => "model-remove",
            Then::RecordCreate { .. } => "record-create",
            Then::RecordUpdate { .. } => "record-update",
            Then::RecordDelete { .. } => "record-delete",
            Then::RunCode { .. } => "run-code",
            Then::CertWatch { .. } => "cert-watch",
        }
    }
}

fn spec_version() -> u32 {
    SPEC_VERSION
}

/// When a rule runs and what it does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSpec {
    #[serde(default = "spec_version")]
    pub version: u32,
    /// Action kinds (`import`, `journal.*`, `*`); any one matching is enough.
    pub on: Vec<String>,
    /// All must hold.
    #[serde(default, rename = "if")]
    pub when: Vec<Condition>,
    pub then: Vec<Then>,
    /// Only for actions about where the rule lives (its journal, live search or projects).
    #[serde(default)]
    pub related: bool,
    /// Run on a schedule as well (2.16): the node runs it with a `schedule` action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
}

impl Default for RuleSpec {
    fn default() -> Self {
        Self { version: SPEC_VERSION, on: vec![], when: vec![], then: vec![], related: false, schedule: None }
    }
}

/// When a scheduled rule runs: every so often (`--every 6h`) or at cron times
/// (`--cron "0 3 * * *"`, local time). See [`crate::rules::next_run`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    /// Milliseconds between runs.
    Every(i64),
    Cron(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub owner_kind: OwnerKind,
    #[serde(default)]
    pub owner_id: Option<String>,
    pub enabled: bool,
    pub spec: RuleSpec,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub runs: i64,
    #[serde(default)]
    pub last_run_at: Option<i64>,
    /// `ok`, `failed` or `blocked` (loop protection) after the last run.
    #[serde(default)]
    pub last_status: Option<String>,
}

impl Rule {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
}

/// A rule to create.
#[derive(Debug, Clone, Default)]
pub struct NewRule {
    pub name: String,
    pub description: String,
    pub owner_kind: OwnerKind,
    pub owner_id: Option<String>,
    pub spec: RuleSpec,
    pub enabled: bool,
}

/// Changes to a rule (`None`: keep).
#[derive(Debug, Clone, Default)]
pub struct RuleChange {
    pub name: Option<String>,
    pub description: Option<String>,
    pub owner: Option<(OwnerKind, Option<String>)>,
    pub spec: Option<RuleSpec>,
}

/// Per-connection state of the rule engine: the rules running now (loop protection), the
/// run budget of the current outermost action, and whether messages go to stderr.
#[derive(Debug, Default)]
pub struct RuleRuntime {
    pub(crate) chain: RefCell<Vec<String>>,
    pub(crate) budget: Cell<u32>,
    pub(crate) echo: Cell<bool>,
    pub(crate) off: Cell<bool>,
    /// An org host serves from this node (scheduled runs say so; see [`crate::org::tls::watch`]).
    pub(crate) host: Cell<bool>,
}

const COLS: &str = "id, name, description, owner_kind, owner_id, enabled, spec, author_id, created_at, updated_at, runs, last_run_at, last_status";

fn check_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(Error::invalid("rule name", "must not be empty"));
    }
    Ok(n.to_string())
}

fn check_spec(s: &RuleSpec) -> Result<()> {
    if s.schedule.is_none() && s.on.iter().all(|o| o.trim().is_empty()) {
        return Err(Error::invalid("rule", "needs at least one --on action kind (e.g. import, journal.*, *) or a schedule (--every, --cron)"));
    }
    if s.then.is_empty() {
        return Err(Error::invalid("rule", "needs at least one --then action"));
    }
    Ok(())
}

impl Store {
    fn row_rule(r: &rusqlite::Row) -> rusqlite::Result<Rule> {
        let kind: String = r.get(3)?;
        let spec: String = r.get(6)?;
        Ok(Rule {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            owner_kind: OwnerKind::parse(&kind),
            owner_id: r.get(4)?,
            enabled: r.get::<_, i64>(5)? != 0,
            spec: serde_json::from_str(&spec).unwrap_or_default(),
            author_id: r.get(7)?,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
            runs: r.get(10)?,
            last_run_at: r.get(11)?,
            last_status: r.get(12)?,
        })
    }

    /// Every rule, by name.
    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM rules ORDER BY name COLLATE NOCASE, created_at"))?;
        let v = st.query_map([], Self::row_rule)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Enabled rules, oldest first (the order they run in).
    pub fn enabled_rules(&self) -> Result<Vec<Rule>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM rules WHERE enabled = 1 ORDER BY created_at, id"))?;
        let v = st.query_map([], Self::row_rule)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn rule(&self, id: &str) -> Result<Option<Rule>> {
        Ok(self.conn.query_row(&format!("SELECT {COLS} FROM rules WHERE id = ?1"), [id], Self::row_rule).optional()?)
    }

    /// By id, id prefix or name.
    pub fn find_rule(&self, key: &str) -> Result<Rule> {
        super::pick(self.rules()?, key, |r| &r.id, |r| &r.name, "rule", "mnnn rule list")
    }

    /// Rules owned by a journal or live search.
    pub fn owned_rules(&self, kind: OwnerKind, id: &str) -> Result<Vec<Rule>> {
        let mut st = self.conn.prepare(&format!("SELECT {COLS} FROM rules WHERE owner_kind = ?1 AND owner_id = ?2 ORDER BY name COLLATE NOCASE"))?;
        let v = st.query_map(params![kind.as_str(), id], Self::row_rule)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Rules that are items of a project.
    pub fn project_rules(&self, project: &str) -> Result<Vec<Rule>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {} FROM rules r JOIN project_items i ON i.ref_id = r.id AND i.kind = 'rule_set' WHERE i.project_id = ?1 ORDER BY r.name COLLATE NOCASE",
            COLS.split(", ").map(|c| format!("r.{c}")).collect::<Vec<_>>().join(", ")
        ))?;
        let v = st.query_map([project], Self::row_rule)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    fn check_owner(&self, kind: OwnerKind, id: Option<&str>) -> Result<Option<String>> {
        match (kind, id) {
            (OwnerKind::Standalone, _) => Ok(None),
            (OwnerKind::Journal, Some(i)) if self.journal(i)?.is_some() => Ok(Some(i.to_string())),
            (OwnerKind::LiveSearch, Some(i)) if self.live_search(i)?.is_some() => Ok(Some(i.to_string())),
            (OwnerKind::Model, Some(i)) if self.model(i)?.is_some() => Ok(Some(i.to_string())),
            (k, i) => Err(Error::invalid("rule owner", format!("{} `{}` not found", k.as_str(), i.unwrap_or("")))),
        }
    }

    pub fn create_rule(&self, n: &NewRule) -> Result<Rule> {
        check_spec(&n.spec)?;
        let now = now_ms();
        let r = Rule {
            id: super::new_id(),
            name: check_name(&n.name)?,
            description: n.description.trim().to_string(),
            owner_kind: n.owner_kind,
            owner_id: self.check_owner(n.owner_kind, n.owner_id.as_deref())?,
            enabled: n.enabled,
            spec: RuleSpec { version: SPEC_VERSION, ..n.spec.clone() },
            author_id: self.current_author_id()?,
            created_at: now,
            updated_at: now,
            runs: 0,
            last_run_at: None,
            last_status: None,
        };
        self.insert_rule(&r)?;
        Ok(r)
    }

    /// Insert a rule as is (archive restore).
    pub(crate) fn insert_rule(&self, r: &Rule) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO rules({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"),
            params![
                r.id,
                r.name,
                r.description,
                r.owner_kind.as_str(),
                r.owner_id,
                r.enabled as i64,
                serde_json::to_string(&r.spec)?,
                r.author_id,
                r.created_at,
                r.updated_at,
                r.runs,
                r.last_run_at,
                r.last_status
            ],
        )?;
        Ok(())
    }

    pub fn update_rule(&self, id: &str, c: &RuleChange) -> Result<Rule> {
        let mut r = self.rule(id)?.ok_or_else(|| Error::invalid("rule", format!("`{id}` not found")))?;
        if let Some(n) = &c.name {
            r.name = check_name(n)?;
        }
        if let Some(d) = &c.description {
            r.description = d.trim().to_string();
        }
        if let Some((k, i)) = &c.owner {
            r.owner_id = self.check_owner(*k, i.as_deref())?;
            r.owner_kind = *k;
        }
        if let Some(s) = &c.spec {
            check_spec(s)?;
            r.spec = RuleSpec { version: SPEC_VERSION, ..s.clone() };
        }
        r.updated_at = now_ms();
        self.conn.execute(
            "UPDATE rules SET name = ?2, description = ?3, owner_kind = ?4, owner_id = ?5, spec = ?6, updated_at = ?7 WHERE id = ?1",
            params![r.id, r.name, r.description, r.owner_kind.as_str(), r.owner_id, serde_json::to_string(&r.spec)?, r.updated_at],
        )?;
        Ok(r)
    }

    pub fn set_rule_enabled(&self, id: &str, on: bool) -> Result<Rule> {
        let n = self.conn.execute("UPDATE rules SET enabled = ?2, updated_at = ?3 WHERE id = ?1", params![id, on as i64, now_ms()])?;
        if n == 0 {
            return Err(Error::invalid("rule", format!("`{id}` not found")));
        }
        Ok(self.rule(id)?.expect("rule just updated"))
    }

    /// Delete a rule and take it out of every project.
    pub fn remove_rule(&self, id: &str) -> Result<Rule> {
        let r = self.rule(id)?.ok_or_else(|| Error::invalid("rule", format!("`{id}` not found")))?;
        self.conn.execute("DELETE FROM project_items WHERE kind = 'rule_set' AND ref_id = ?1", [id])?;
        self.conn.execute("DELETE FROM rules WHERE id = ?1", [id])?;
        Ok(r)
    }

    /// When a journal or live search is removed its rules stay, standalone and disabled.
    pub(crate) fn orphan_rules(&self, kind: OwnerKind, id: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE rules SET owner_kind = 'standalone', owner_id = NULL, enabled = 0, updated_at = ?3 WHERE owner_kind = ?1 AND owner_id = ?2",
            params![kind.as_str(), id, now_ms()],
        )?)
    }

    /// Count a run (`ok`, `failed`, `blocked`).
    pub(crate) fn note_rule_run(&self, id: &str, status: &str) -> Result<()> {
        self.conn.execute("UPDATE rules SET runs = runs + 1, last_run_at = ?2, last_status = ?3 WHERE id = ?1", params![id, now_ms(), status])?;
        Ok(())
    }

    /// A scheduled run starts now (its next one counts from here, even if it fails).
    pub(crate) fn note_rule_scheduled(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE rules SET last_run_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    /// Recent runs of a rule from the history (`rule.run` actions), newest first.
    pub fn rule_runs(&self, id: &str, limit: usize) -> Result<Vec<super::ActionRecord>> {
        let mut v: Vec<_> = self.actions_of_kind("rule.run")?.into_iter().filter(|a| a.params["rule"].as_str() == Some(id)).collect();
        v.reverse();
        v.truncate(limit);
        Ok(v)
    }

    /// Write messages from rules to stderr (the CLI turns this on).
    pub fn set_rule_echo(&self, on: bool) {
        self.rules_rt.echo.set(on);
    }

    /// Tell rules an org host is serving from this node (the cert watcher asks it to renew).
    pub fn set_host_running(&self, on: bool) {
        self.rules_rt.host.set(on);
    }

    pub fn host_running(&self) -> bool {
        self.rules_rt.host.get()
    }

    /// Stop evaluating rules on this connection (tests, bulk tools).
    pub fn set_rules_off(&self, off: bool) {
        self.rules_rt.off.set(off);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EntryFormat;

    fn spec() -> RuleSpec {
        RuleSpec { on: vec!["import".into()], then: vec![Then::Status { text: "hi".into() }], ..Default::default() }
    }

    #[test]
    fn crud_owner_and_projects() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.create_rule(&NewRule { name: "x".into(), ..Default::default() }).is_err(), "needs on and then");
        let r = s.create_rule(&NewRule { name: "Imported".into(), spec: spec(), enabled: true, ..Default::default() }).unwrap();
        assert_eq!(s.find_rule("imported").unwrap().id, r.id);
        assert_eq!(s.enabled_rules().unwrap().len(), 1);
        let j = s.create_journal("J", None, EntryFormat::Markdown).unwrap();
        let r = s.update_rule(&r.id, &RuleChange { owner: Some((OwnerKind::Journal, Some(j.id.clone()))), ..Default::default() }).unwrap();
        assert_eq!(s.owned_rules(OwnerKind::Journal, &j.id).unwrap()[0].id, r.id);
        assert!(s.update_rule(&r.id, &RuleChange { owner: Some((OwnerKind::LiveSearch, Some("nope".into()))), ..Default::default() }).is_err());
        let p = s.create_project("P", "").unwrap();
        s.add_project_item(&p.id, crate::store::ItemKind::RuleSet, &r.id, serde_json::json!({})).unwrap();
        assert_eq!(s.project_rules(&p.id).unwrap()[0].id, r.id);
        // the journal goes: the rule stays, standalone and off
        s.remove_journal(&j.id).unwrap();
        let r = s.rule(&r.id).unwrap().unwrap();
        assert_eq!((r.owner_kind, r.owner_id.clone(), r.enabled), (OwnerKind::Standalone, None, false));
        assert!(!s.set_rule_enabled(&r.id, true).unwrap().spec.on.is_empty());
        s.remove_rule(&r.id).unwrap();
        assert!(s.project_items(&p.id).unwrap().is_empty() && s.rules().unwrap().is_empty());
    }

    #[test]
    fn spec_json_is_stable() {
        let mut sp = spec();
        sp.when.push(Condition { field: "inserted".into(), op: Op::Gt, value: "0".into() });
        sp.then.push(Then::AddToProject { project: None, item: ItemRef::Subject });
        let v = serde_json::to_value(&sp).unwrap();
        assert_eq!(v["if"][0]["op"], ">");
        assert_eq!(v["then"][0]["do"], "status");
        assert_eq!(v["then"][1]["item"]["kind"], "subject");
        assert_eq!(serde_json::from_value::<RuleSpec>(v).unwrap(), sp);
        // older/newer specs without optional fields still read
        let r: RuleSpec = serde_json::from_str(r#"{"on":["*"],"then":[{"do":"run_search"}]}"#).unwrap();
        assert_eq!((r.version, r.then[0].clone()), (1, Then::RunSearch { search: None }));
    }
}
