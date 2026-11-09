//! Rule sets and schedules (`mnnn ruleset`, `mnnn schedule`, `mnnn scheduler`; TUI
//! Rules ▸ Rule sets / Schedules).
use super::data::{open, open_write};
use muninn_core::schedule;
use muninn_core::store::{NewAction, ScheduleDef, ScheduleRun, Store};
use serde_json::json;

const ORIGIN: &str = "app";

pub struct RulesetInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_kind: String,
    pub enabled: bool,
    pub rules: Vec<String>,
    pub schedules: Vec<String>,
}

pub struct ScheduleInfo {
    pub id: String,
    pub name: String,
    pub target_kind: String,
    pub target: String,
    pub run_every: String,
    pub cron: String,
    pub tz: String,
    pub start_at: i64,
    pub ends_at: i64,
    pub enabled: bool,
    pub next_run_at: i64,
    pub overlap: String,
    pub catch_up: String,
    pub max_attempts: i64,
    pub describe: Vec<String>,
}

pub struct ScheduleRunInfo {
    pub id: i64,
    pub status: String,
    pub timestamp: i64,
    pub attempt: i64,
    pub started_at: i64,
    pub finished_at: i64,
    pub ruleset_ran: i64,
    pub stored: i64,
    pub error: String,
}

fn record(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, ORIGIN, summary).params(params));
}

fn info(store: &Store, s: &ScheduleDef) -> ScheduleInfo {
    let target = match s.target_kind.as_str() {
        "ruleset" => s.ruleset_id.as_deref().and_then(|id| store.ruleset(id).ok().flatten()).map(|r| r.name).unwrap_or_else(|| "(removed)".into()),
        _ => s.plugin_id.as_deref().and_then(|id| store.code_space(id).ok().flatten()).map(|c| c.name).unwrap_or_else(|| "(removed)".into()),
    };
    ScheduleInfo {
        id: s.id.clone(),
        name: s.name.clone(),
        target_kind: s.target_kind.clone(),
        target,
        run_every: s.run_every.clone().unwrap_or_default(),
        cron: s.cron.clone().unwrap_or_default(),
        tz: s.tz.clone().unwrap_or_default(),
        start_at: s.start_at.unwrap_or(0),
        ends_at: s.ends_at.unwrap_or(0),
        enabled: s.enabled,
        next_run_at: s.next_run_at,
        overlap: s.overlap.clone(),
        catch_up: s.catch_up.clone(),
        max_attempts: s.max_attempts,
        describe: schedule::describe(s),
    }
}

fn run_info(r: &ScheduleRun) -> ScheduleRunInfo {
    ScheduleRunInfo {
        id: r.id,
        status: r.status.clone(),
        timestamp: r.timestamp,
        attempt: r.attempt,
        started_at: r.started_at.unwrap_or(0),
        finished_at: r.finished_at.unwrap_or(0),
        ruleset_ran: r.ruleset_ran,
        stored: r.stored,
        error: r.error.clone().unwrap_or_default(),
    }
}

pub(crate) fn tick_text(rep: &schedule::TickReport) -> String {
    if rep.standby {
        return "another scheduler is running (standby)".into();
    }
    let mut parts = vec![];
    if !rep.started.is_empty() {
        parts.push(format!("{} run(s): {} ok, {} failed", rep.started.len(), rep.succeeded.len(), rep.failed.len()));
    }
    if !rep.missed.is_empty() {
        parts.push(format!("missed {}", rep.missed.join(", ")));
    }
    if !rep.legacy.is_empty() {
        parts.push(format!("ran {}", rep.legacy.join(", ")));
    }
    if parts.is_empty() { "nothing is due".into() } else { parts.join("; ") }
}

/// `mnnn ruleset list`.
pub fn rulesets_list() -> anyhow::Result<Vec<RulesetInfo>> {
    let store = open()?;
    store
        .rulesets()?
        .into_iter()
        .map(|r| {
            Ok(RulesetInfo {
                rules: store.ruleset_rules(&r.id)?.into_iter().map(|x| x.name).collect(),
                schedules: store.schedules_using("ruleset_id", &r.id)?.into_iter().map(|s| s.name).collect(),
                id: r.id,
                name: r.name,
                description: r.description,
                owner_kind: r.owner_kind,
                enabled: r.enabled,
            })
        })
        .collect()
}

/// `mnnn schedule list`.
pub fn schedules_list() -> anyhow::Result<Vec<ScheduleInfo>> {
    let store = open()?;
    Ok(store.schedules()?.iter().map(|s| info(&store, s)).collect())
}

/// `mnnn schedule runs KEY`.
pub fn schedule_runs(key: String) -> anyhow::Result<Vec<ScheduleRunInfo>> {
    let store = open()?;
    let s = store.find_schedule(&key)?;
    Ok(store.schedule_runs(&s.id)?.iter().rev().map(run_info).collect())
}

/// `mnnn schedule run-now KEY`.
pub fn schedule_run_now(key: String) -> anyhow::Result<ScheduleRunInfo> {
    let store = open_write()?;
    let s = store.find_schedule(&key)?;
    let r = schedule::run_now(&store, &s, &schedule::holder("app"), muninn_core::store::now_ms())?;
    record(&store, "schedule.run", format!("run schedule {} now", s.name), json!({ "schedule": s.id, "run": r.id }));
    Ok(run_info(&r))
}

/// `mnnn schedule enable|disable KEY`.
pub fn schedule_set_enabled(key: String, enabled: bool) -> anyhow::Result<ScheduleInfo> {
    let store = open_write()?;
    let mut s = store.find_schedule(&key)?;
    schedule::set_enabled(&store, &mut s, enabled, muninn_core::store::now_ms())?;
    record(&store, if enabled { "schedule.enable" } else { "schedule.disable" }, format!("{} schedule {}", if enabled { "enable" } else { "disable" }, s.name), json!({ "schedule": s.id }));
    Ok(info(&store, &s))
}

/// `mnnn scheduler tick`.
pub fn scheduler_tick() -> anyhow::Result<String> {
    let store = open_write()?;
    let rep = schedule::tick(&store, &schedule::holder("app"), muninn_core::store::now_ms())?;
    Ok(tick_text(&rep))
}
