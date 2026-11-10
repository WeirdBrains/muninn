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
    record(
        &store,
        if enabled { "schedule.enable" } else { "schedule.disable" },
        format!("{} schedule {}", if enabled { "enable" } else { "disable" }, s.name),
        json!({ "schedule": s.id }),
    );
    Ok(info(&store, &s))
}

/// `mnnn scheduler tick`.
pub fn scheduler_tick() -> anyhow::Result<String> {
    let store = open_write()?;
    let rep = schedule::tick(&store, &schedule::holder("app"), muninn_core::store::now_ms())?;
    Ok(tick_text(&rep))
}

/// Whether start_at would be in the past (`Past::Yes` → offer next-interval).
pub fn schedule_past_start(
    run_every: Option<String>,
    cron: Option<String>,
    start_at: Option<String>,
    ends_at: Option<String>,
    for_: Option<String>,
    tz: Option<String>,
) -> anyhow::Result<String> {
    let now = muninn_core::store::now_ms();
    let mut s =
        muninn_core::store::ScheduleDef { name: "check".into(), target_kind: "ruleset".into(), enabled: true, created_at: now, ..Default::default() };
    let timing = schedule::Timing {
        run_every,
        cron,
        start_at,
        ends_at,
        for_,
        tz,
        past_next: true, // probe with next so apply_timing can fill; we read past_of before
    };
    // Apply with past_next so we get next_run; but first detect past without it.
    let probe = schedule::Timing { past_next: false, ..timing.clone() };
    match schedule::apply_timing(&mut s, &probe, now) {
        Ok(()) => Ok("ok".into()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("past") {
                // Recompute next slot.
                let mut s2 = muninn_core::store::ScheduleDef {
                    name: "check".into(),
                    target_kind: "ruleset".into(),
                    enabled: true,
                    created_at: now,
                    ..Default::default()
                };
                schedule::apply_timing(&mut s2, &timing, now)?;
                Ok(format!("past:next:{}", s2.start_at.unwrap_or(s2.next_run_at)))
            } else {
                Err(e.into())
            }
        }
    }
}

/// `mnnn ruleset create`.
pub fn ruleset_create(name: String, description: String, rules: Vec<String>) -> anyhow::Result<RulesetInfo> {
    let store = open_write()?;
    let found = rules.iter().map(|k| store.find_rule(k)).collect::<muninn_core::error::Result<Vec<_>>>()?;
    let r = store.create_ruleset(&name, &description, "standalone", None)?;
    for x in &found {
        store.add_ruleset_rule(&r.id, &x.id, None)?;
    }
    record(&store, "ruleset.create", format!("create rule set {}", r.name), json!({ "ruleset": r.id, "name": r.name }));
    Ok(RulesetInfo {
        rules: store.ruleset_rules(&r.id)?.into_iter().map(|x| x.name).collect(),
        schedules: vec![],
        id: r.id,
        name: r.name,
        description: r.description,
        owner_kind: r.owner_kind,
        enabled: r.enabled,
    })
}

/// `mnnn ruleset edit`.
pub fn ruleset_edit(ruleset: String, name: Option<String>, description: Option<String>, enable: Option<bool>) -> anyhow::Result<RulesetInfo> {
    let store = open_write()?;
    let r = store.find_ruleset(&ruleset)?;
    let mut r2 = store.edit_ruleset(&r.id, name.as_deref(), description.as_deref())?;
    if let Some(on) = enable {
        store.set_ruleset_enabled(&r.id, on)?;
        r2.enabled = on;
    }
    record(&store, "ruleset.edit", format!("edit rule set {}", r2.name), json!({ "ruleset": r.id }));
    Ok(RulesetInfo {
        rules: store.ruleset_rules(&r2.id)?.into_iter().map(|x| x.name).collect(),
        schedules: store.schedules_using("ruleset_id", &r2.id)?.into_iter().map(|s| s.name).collect(),
        id: r2.id,
        name: r2.name,
        description: r2.description,
        owner_kind: r2.owner_kind,
        enabled: r2.enabled,
    })
}

/// `mnnn ruleset remove` (force drops schedules that use it).
pub fn ruleset_remove(ruleset: String, force: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = store.find_ruleset(&ruleset)?;
    let using = store.schedules_using("ruleset_id", &r.id)?;
    if !using.is_empty() && !force {
        return Err(anyhow::anyhow!(
            "schedules run rule set {}: {} (remove them, or force)",
            r.name,
            using.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    for s in &using {
        store.remove_schedule(&s.id)?;
    }
    store.remove_ruleset(&r.id)?;
    record(&store, "ruleset.remove", format!("remove rule set {}", r.name), json!({ "ruleset": r.id }));
    Ok(format!("removed rule set {}", r.name))
}

/// `mnnn ruleset add SET RULE`.
pub fn ruleset_add_rule(ruleset: String, rule: String, position: Option<u32>) -> anyhow::Result<RulesetInfo> {
    let store = open_write()?;
    let r = store.find_ruleset(&ruleset)?;
    let x = store.find_rule(&rule)?;
    store.add_ruleset_rule(&r.id, &x.id, position.map(|p| p as usize))?;
    record(&store, "ruleset.add", format!("add rule {} to rule set {}", x.name, r.name), json!({ "ruleset": r.id, "rule": x.id }));
    ruleset_edit(r.id, None, None, None)
}

/// `mnnn ruleset drop SET RULE`.
pub fn ruleset_drop_rule(ruleset: String, rule: String) -> anyhow::Result<RulesetInfo> {
    let store = open_write()?;
    let r = store.find_ruleset(&ruleset)?;
    let x = store.find_rule(&rule)?;
    if !store.drop_ruleset_rule(&r.id, &x.id)? {
        return Err(anyhow::anyhow!("rule {} is not in rule set {}", x.name, r.name));
    }
    record(&store, "ruleset.drop", format!("drop rule {} from rule set {}", x.name, r.name), json!({ "ruleset": r.id, "rule": x.id }));
    ruleset_edit(r.id, None, None, None)
}

fn timing_of(
    run_every: Option<String>,
    cron: Option<String>,
    start_at: Option<String>,
    ends_at: Option<String>,
    for_: Option<String>,
    tz: Option<String>,
    past_next: bool,
) -> schedule::Timing {
    schedule::Timing { run_every, cron, start_at, ends_at, for_, tz, past_next }
}

/// `mnnn schedule add NAME … --ruleset SET` or `--plugin CONNECTOR --source SRC`.
#[allow(clippy::too_many_arguments)]
pub fn schedule_add(
    name: String,
    run_every: Option<String>,
    ruleset: Option<String>,
    plugin: Option<String>,
    source: Option<String>,
    pipeline: Option<String>,
    connector_ruleset: Option<String>,
    options: Vec<String>,
    cron: Option<String>,
    start_at: Option<String>,
    ends_at: Option<String>,
    for_: Option<String>,
    tz: Option<String>,
    past_next: bool,
    overlap: Option<String>,
    catch_up: Option<String>,
    max_attempts: Option<i64>,
    disabled: bool,
) -> anyhow::Result<ScheduleInfo> {
    let store = open_write()?;
    let now = muninn_core::store::now_ms();
    let mut s = muninn_core::store::ScheduleDef {
        name,
        overlap: overlap.unwrap_or_else(|| "skip".into()),
        catch_up: catch_up.unwrap_or_else(|| "once".into()),
        max_attempts: max_attempts.unwrap_or(3),
        enabled: !disabled,
        created_at: now,
        ..Default::default()
    };
    match (ruleset, plugin) {
        (Some(rs), None) => {
            let set = store.find_ruleset(&rs)?;
            s.target_kind = "ruleset".into();
            s.ruleset_id = Some(set.id.clone());
            schedule::apply_timing(&mut s, &timing_of(run_every, cron, start_at, ends_at, for_, tz, past_next), now)?;
            let s = store.save_schedule(&s)?;
            record(
                &store,
                "schedule.add",
                format!("add schedule {} for rule set {}", s.name, set.name),
                json!({ "schedule": s.id, "ruleset": set.id }),
            );
            Ok(info(&store, &s))
        }
        (None, Some(plug)) => {
            let space = store.find_space(&plug)?;
            if space.kind != muninn_core::store::SpaceKind::IngestConnector {
                return Err(anyhow::anyhow!("{} is a {} space; connector schedules need kind ingest-connector", space.name, space.kind));
            }
            let src_name = source.ok_or_else(|| anyhow::anyhow!("plugin needs a source"))?;
            let src = store.source_by_name(&src_name)?.ok_or_else(|| anyhow::anyhow!("no source `{src_name}`"))?;
            let pipe = pipeline.map(|n| store.find_pipeline(&n)).transpose()?;
            let rset = connector_ruleset.map(|n| store.find_ruleset(&n)).transpose()?;
            let mut opts = serde_json::Map::new();
            for o in options {
                let (k, v) = o.split_once('=').ok_or_else(|| anyhow::anyhow!("option KEY=VALUE"))?;
                opts.insert(k.trim().into(), json!(v));
            }
            s.target_kind = "connector".into();
            s.plugin_id = Some(space.id.clone());
            s.source_id = Some(src.id.clone());
            s.pipeline_id = pipe.as_ref().map(|p| p.id.clone());
            s.ruleset_id = rset.as_ref().map(|r| r.id.clone());
            s.options = opts;
            schedule::apply_timing(&mut s, &timing_of(run_every, cron, start_at, ends_at, for_, tz, past_next), now)?;
            let s = store.save_schedule(&s)?;
            record(
                &store,
                "schedule.add",
                format!("add schedule {} for connector {}", s.name, space.name),
                json!({ "schedule": s.id, "plugin": space.id }),
            );
            Ok(info(&store, &s))
        }
        _ => Err(anyhow::anyhow!("give a ruleset or a plugin+source")),
    }
}

/// `mnnn schedule edit KEY` — empty timing fields keep the old value when [retarget] is false.
#[allow(clippy::too_many_arguments)]
pub fn schedule_edit(
    key: String,
    name: Option<String>,
    run_every: Option<String>,
    cron: Option<String>,
    start_at: Option<String>,
    ends_at: Option<String>,
    for_: Option<String>,
    tz: Option<String>,
    past_next: bool,
    overlap: Option<String>,
    catch_up: Option<String>,
    max_attempts: Option<i64>,
    clear_ends: bool,
) -> anyhow::Result<ScheduleInfo> {
    let store = open_write()?;
    let now = muninn_core::store::now_ms();
    let mut s = store.find_schedule(&key)?;
    if let Some(n) = name.map(|x| x.trim().to_string()).filter(|x| !x.is_empty()) {
        s.name = n;
    }
    if let Some(o) = overlap {
        s.overlap = o;
    }
    if let Some(c) = catch_up {
        s.catch_up = c;
    }
    if let Some(m) = max_attempts {
        s.max_attempts = m;
    }
    let has_timing = run_every.is_some() || cron.is_some() || start_at.is_some() || ends_at.is_some() || for_.is_some() || tz.is_some() || clear_ends;
    if has_timing {
        if clear_ends {
            s.ends_at = None;
            s.ends_at_input = None;
        }
        let t = timing_of(
            run_every.or_else(|| s.run_every.clone()),
            cron.or_else(|| s.cron.clone()),
            start_at,
            if clear_ends { None } else { ends_at },
            for_,
            tz.or_else(|| s.tz.clone()),
            past_next,
        );
        schedule::apply_timing(&mut s, &t, now)?;
    }
    let s = store.save_schedule(&s)?;
    record(&store, "schedule.edit", format!("edit schedule {}", s.name), json!({ "schedule": s.id }));
    Ok(info(&store, &s))
}

pub struct SchedulePreview {
    pub lines: Vec<String>,
    pub runs_remaining: String,
}

/// `mnnn schedule parse`: what the timing means, without saving. [runs_remaining]
/// is `no end`, a count, or `10000+` when a cron end is far away.
#[allow(clippy::too_many_arguments)]
pub fn schedule_parse(
    run_every: Option<String>,
    cron: Option<String>,
    start_at: Option<String>,
    ends_at: Option<String>,
    for_: Option<String>,
    tz: Option<String>,
    past_next: bool,
) -> anyhow::Result<SchedulePreview> {
    let now = muninn_core::store::now_ms();
    let mut s =
        muninn_core::store::ScheduleDef { name: "parse".into(), target_kind: "ruleset".into(), enabled: true, created_at: now, ..Default::default() };
    schedule::apply_timing(&mut s, &timing_of(run_every, cron, start_at, ends_at, for_, tz, past_next), now)?;
    let mut lines = schedule::describe(&s);
    let mut t = s.next_run_at;
    for _ in 0..3 {
        let Some(n) = schedule::step(&s, t).filter(|n| s.ends_at.is_none_or(|e| *n < e)) else { break };
        lines.push(format!("then   {}", schedule::show_time(n, s.tz.as_deref())));
        t = n;
    }
    let runs_remaining = match s.ends_at {
        None => "no end".into(),
        Some(en) => {
            let mut n = 0i64;
            let mut cur = Some(s.next_run_at);
            let mut capped = false;
            while let Some(t) = cur.filter(|t| *t < en) {
                n += 1;
                if n >= 10_000 {
                    capped = true;
                    break;
                }
                cur = schedule::step(&s, t);
            }
            if capped { "10000+".into() } else { n.to_string() }
        }
    };
    if s.is_once() {
        lines.push("ends   auto (once)".into());
    } else if s.ends_at.is_some() {
        lines.push(format!("runs remaining: {runs_remaining}"));
    }
    Ok(SchedulePreview { lines, runs_remaining })
}

/// `mnnn ruleset run NAME` (records a manual schedule action, then runs the set).
pub fn ruleset_run(ruleset: String) -> anyhow::Result<Vec<String>> {
    let store = open_write()?;
    let r = store.find_ruleset(&ruleset)?;
    let aid = store.record_action(
        &NewAction::new(muninn_core::rules::SCHEDULE, ORIGIN, format!("run rule set {}", r.name))
            .params(json!({ "ruleset": r.id, "name": r.name, "manual": true })),
    )?;
    let a = store.action(aid)?.ok_or_else(|| anyhow::anyhow!("action #{aid} is gone"))?;
    let res = schedule::run_ruleset(&store, &r.id, &a)?;
    let mut lines = res.messages;
    lines.push(format!(
        "ran {} rule(s){}",
        res.ran.len(),
        if res.failed.is_empty() { String::new() } else { format!(", failed: {}", res.failed.join(", ")) }
    ));
    if !res.failed.is_empty() {
        anyhow::bail!(lines.join("\n"));
    }
    Ok(lines)
}

/// `mnnn schedule remove KEY`.
pub fn schedule_remove(key: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_schedule(&key)?;
    store.remove_schedule(&s.id)?;
    record(&store, "schedule.remove", format!("remove schedule {}", s.name), json!({ "schedule": s.id }));
    Ok(format!("removed schedule {}", s.name))
}
