//! Rules (`mnnn rule` / TUI Rules, `R`): act when something happens.
use super::data::{open, open_write};
use muninn_core::rules;
use muninn_core::store::{ItemKind, NewAction, NewRule, OwnerKind, Rule, RuleChange, RuleSpec, Store};
use serde_json::json;

const ORIGIN: &str = "app";

pub struct RuleInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub home: String,
    pub brief: String,
    pub on_text: String,
    pub when_text: String,
    pub then_text: String,
    pub related: bool,
    pub runs: i64,
    pub last_status: String,
    pub last_run_at: i64,
    pub next_run: i64,
    pub created_at: i64,
}

pub struct RuleEventInfo {
    pub kind: String,
    pub about: String,
    pub fields: String,
}

fn record(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, ORIGIN, summary).params(params));
}

fn params(r: &Rule) -> serde_json::Value {
    json!({ "rule": r.id, "name": r.name })
}

fn list_of(s: &str) -> Vec<String> {
    rules::split_list(&s.replace('\n', ";"))
}

fn rule_info(store: &Store, r: Rule) -> RuleInfo {
    RuleInfo {
        short_id: r.short_id().to_string(),
        home: rules::home(store, &r),
        brief: rules::brief(&r),
        on_text: rules::on_text(&r.spec),
        when_text: r.spec.when.iter().map(rules::condition_text).collect::<Vec<_>>().join("; "),
        then_text: r.spec.then.iter().map(|t| rules::then_text(store, t)).collect::<Vec<_>>().join("; "),
        related: r.spec.related,
        last_status: r.last_status.clone().unwrap_or_default(),
        last_run_at: r.last_run_at.unwrap_or(0),
        next_run: rules::next_run(&r).unwrap_or(0),
        id: r.id,
        name: r.name,
        description: r.description,
        enabled: r.enabled,
        runs: r.runs,
        created_at: r.created_at,
    }
}

pub(crate) fn rules_of(store: &Store) -> anyhow::Result<Vec<RuleInfo>> {
    Ok(store.rules()?.into_iter().map(|r| rule_info(store, r)).collect())
}

type RuleHome = (Option<(OwnerKind, Option<String>)>, Option<muninn_core::store::Project>);

fn home_of(store: &Store, h: &str) -> anyhow::Result<RuleHome> {
    let h = h.trim();
    if h.is_empty() {
        return Ok((Some((OwnerKind::Standalone, None)), None));
    }
    match h.split_once(':').map(|(k, v)| (k.trim(), v.trim())) {
        Some(("project", n)) => Ok((None, Some(store.find_project(n)?))),
        Some(("journal", n)) => Ok((Some((OwnerKind::Journal, Some(store.find_journal(n)?.id))), None)),
        Some(("search", n)) => Ok((Some((OwnerKind::LiveSearch, Some(store.find_live_search(n)?.id))), None)),
        Some(("model", n)) => Ok((Some((OwnerKind::Model, Some(store.find_model(n)?.id))), None)),
        _ => Err(anyhow::anyhow!("`{h}`: project:NAME, journal:NAME, search:NAME, model:NAME or empty")),
    }
}

fn add_to_project(store: &Store, r: &Rule, project: Option<muninn_core::store::Project>) -> anyhow::Result<()> {
    let Some(p) = project else { return Ok(()) };
    let (it, new) = store.add_project_item(&p.id, ItemKind::RuleSet, &r.id, json!({}))?;
    if new {
        record(
            store,
            "project.add",
            format!("add rule {} to project {}", r.name, p.name),
            json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
        );
    }
    Ok(())
}

fn format_check(store: &Store, r: &Rule, c: &rules::Check) -> Vec<String> {
    let mut lines = vec![format!("rule {}{} vs #{} {}:", r.name, if r.enabled { "" } else { " (off)" }, c.action, c.kind)];
    match &c.on {
        Some(p) => lines.push(format!("  ✓ on {p}")),
        None if c.kind.starts_with("rule.") => lines.push(format!("  ✗ on {} (rule actions never trigger rules)", r.spec.on.join(", "))),
        None => lines.push(format!("  ✗ on {}", r.spec.on.join(", "))),
    }
    if let Some(rel) = c.related {
        lines.push(format!("  {} related to {}", if rel { "✓" } else { "✗" }, rules::home(store, r)));
    }
    for k in &c.conditions {
        let actual = k.actual.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "missing".into());
        lines.push(format!("  {} {}   ({actual})", if k.ok { "✓" } else { "✗" }, k.condition));
    }
    lines.push(format!("  → {}", if c.matched { "matches" } else { "no match" }));
    lines
}

/// `mnnn rule list` / TUI Rules….
pub fn rules() -> anyhow::Result<Vec<RuleInfo>> {
    rules_of(&open()?)
}

/// `mnnn rule create` / TUI `n`. [on] is kinds or a schedule (`every 6h`, `cron "0 3 * * *"`).
/// [when] and [then] are `;` or newline separated. [home] is `project:NAME`, `journal:NAME`,
/// `search:NAME`, `model:NAME` or empty.
pub fn rule_create(
    name: String,
    description: String,
    on: String,
    when: String,
    then: String,
    home: String,
    related: bool,
) -> anyhow::Result<RuleInfo> {
    let store = open_write()?;
    let (on_kinds, schedule) = rules::parse_on_text(&on)?;
    if on_kinds.is_empty() && schedule.is_none() {
        return Err(anyhow::anyhow!("give at least one action kind or a schedule"));
    }
    let when = list_of(&when).iter().map(|c| rules::parse_condition(c)).collect::<muninn_core::error::Result<Vec<_>>>()?;
    let then_list = list_of(&then);
    if then_list.is_empty() {
        return Err(anyhow::anyhow!("give at least one then-action (status …, notify …, …)"));
    }
    let then = then_list.iter().map(|t| rules::parse_then(&store, t)).collect::<muninn_core::error::Result<Vec<_>>>()?;
    let (owner, project) = home_of(&store, &home)?;
    let (kind, oid) = owner.unwrap_or((OwnerKind::Standalone, None));
    let spec = RuleSpec { on: on_kinds, when, then, related, schedule, ..Default::default() };
    let r = store.create_rule(&NewRule { name, description, owner_kind: kind, owner_id: oid, spec, enabled: true })?;
    record(&store, "rule.create", format!("create rule {}", r.name), params(&r));
    add_to_project(&store, &r, project)?;
    Ok(rule_info(&store, r))
}

/// `mnnn rule edit` / TUI `e`. Empty [on]/[when]/[then]/[home] keep the old value.
#[allow(clippy::too_many_arguments)]
pub fn rule_edit(
    rule: String,
    name: Option<String>,
    description: Option<String>,
    on: Option<String>,
    when: Option<String>,
    then: Option<String>,
    home: Option<String>,
    related: Option<bool>,
    no_schedule: bool,
) -> anyhow::Result<RuleInfo> {
    let store = open_write()?;
    let r = store.find_rule(rule.trim())?;
    let mut sp = r.spec.clone();
    let mut spec_changed = false;
    if let Some(o) = on.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let (kinds, schedule) = rules::parse_on_text(o)?;
        sp.on = kinds;
        sp.schedule = schedule;
        spec_changed = true;
    }
    if no_schedule {
        sp.schedule = None;
        spec_changed = true;
    }
    if let Some(w) = when.as_deref() {
        sp.when = list_of(w).iter().map(|c| rules::parse_condition(c)).collect::<muninn_core::error::Result<Vec<_>>>()?;
        spec_changed = true;
    }
    if let Some(t) = then.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sp.then = list_of(t).iter().map(|x| rules::parse_then(&store, x)).collect::<muninn_core::error::Result<Vec<_>>>()?;
        spec_changed = true;
    }
    if let Some(rel) = related {
        sp.related = rel;
        spec_changed = true;
    }
    let (owner, project) = match home.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(h) => home_of(&store, h)?,
        None => (None, None),
    };
    let n = store.update_rule(
        &r.id,
        &RuleChange { name: name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()), description, owner, spec: spec_changed.then_some(sp) },
    )?;
    record(&store, "rule.edit", format!("edit rule {}", n.name), params(&n));
    add_to_project(&store, &n, project)?;
    Ok(rule_info(&store, n))
}

/// `mnnn rule remove` / TUI `d`.
pub fn rule_remove(rule: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = store.find_rule(rule.trim())?;
    let name = r.name.clone();
    let r = store.remove_rule(&r.id)?;
    record(&store, "rule.remove", format!("remove rule {}", r.name), params(&r));
    Ok(format!("removed rule {name}"))
}

/// `mnnn rule enable` / TUI Space.
pub fn rule_enable(rule: String) -> anyhow::Result<String> {
    toggle(rule, true)
}

/// `mnnn rule disable` / TUI Space.
pub fn rule_disable(rule: String) -> anyhow::Result<String> {
    toggle(rule, false)
}

fn toggle(rule: String, on: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = store.find_rule(rule.trim())?;
    let r = store.set_rule_enabled(&r.id, on)?;
    let (kind, verb) = if on { ("rule.enable", "on") } else { ("rule.disable", "off") };
    record(&store, kind, format!("switch rule {} {verb}", r.name), params(&r));
    Ok(format!("rule {} is {verb}", r.name))
}

/// Show a rule and its recent runs (`mnnn rule show`).
pub fn rule_show(rule: String) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let r = store.find_rule(rule.trim())?;
    let runs = store.rule_runs(&r.id, 10)?;
    let mut lines = vec![format!("id:       {}", r.id), format!("name:     {}", r.name)];
    if !r.description.is_empty() {
        lines.push(format!("about:    {}", r.description));
    }
    lines.push(format!("state:    {}", if r.enabled { "on" } else { "off" }));
    lines.push(format!("lives in: {}", rules::home(&store, &r)));
    if let Some(s) = &r.spec.schedule {
        let next =
            rules::next_run(&r).map(|t| muninn_core::ingest::time::fmt_ts(t.max(muninn_core::store::now_ms()))).unwrap_or_else(|| "never".into());
        lines.push(format!("runs:     {} (next {next}{})", rules::schedule_text(s), if r.enabled { "" } else { ", when on" }));
    }
    if !r.spec.on.is_empty() || r.spec.schedule.is_none() {
        lines.push(format!("on:       {}{}", r.spec.on.join(", "), if r.spec.related { " (related only)" } else { "" }));
    }
    for c in &r.spec.when {
        lines.push(format!("if:       {}", rules::condition_text(c)));
    }
    for t in &r.spec.then {
        lines.push(format!("then:     {}", rules::then_text(&store, t)));
    }
    lines.push(format!("runs:     {}{}", r.runs, r.last_status.as_ref().map(|s| format!(" (last: {s})")).unwrap_or_default()));
    for a in &runs {
        lines.push(format!("  #{} {}  {}", a.id, muninn_core::ingest::time::fmt_ts(a.created_at), a.summary));
    }
    Ok(lines)
}

fn action_for(store: &Store, r: &Rule, id: Option<i64>) -> anyhow::Result<muninn_core::store::ActionRecord> {
    if id.is_none() && (r.spec.schedule.is_some() || r.spec.on.iter().any(|k| k == rules::SCHEDULE)) {
        return Ok(rules::schedule_action(r));
    }
    match id {
        Some(i) => store.action(i)?.ok_or_else(|| anyhow::anyhow!("no action #{i}")),
        None => rules::last_trigger(store)?.ok_or_else(|| anyhow::anyhow!("nothing in the history yet to test against")),
    }
}

/// `mnnn rule test` / TUI `t`. Empty [rule] tests every rule.
pub fn rule_test(rule: Option<String>, action: Option<i64>) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let picked = match rule.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => vec![store.find_rule(k)?],
        None => store.rules()?,
    };
    let mut lines = Vec::new();
    for r in &picked {
        let a = action_for(&store, r, action)?;
        lines.extend(format_check(&store, r, &rules::check(&store, r, &a)));
    }
    if lines.is_empty() {
        lines.push("no rules to test".into());
    }
    Ok(lines)
}

/// `mnnn rule dry-run` / TUI `x`.
pub fn rule_dry_run(rule: Option<String>, action: Option<i64>) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    if let Ok(p) = super::info::paths(None) {
        store.set_host_running(muninn_core::org::host::running(&p.home).is_some());
    }
    let picked = match rule.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(k) => vec![store.find_rule(k)?],
        None => store.rules()?,
    };
    let mut lines = Vec::new();
    for r in &picked {
        let a = action_for(&store, r, action)?;
        let c = rules::check(&store, r, &a);
        lines.extend(format_check(&store, r, &c));
        if c.matched && !r.enabled {
            lines.push("  (off: it would not run now)".into());
        }
        if c.matched {
            for t in &r.spec.then {
                match rules::plan(&store, r, &a, t) {
                    Ok(m) => lines.push(format!("  would {m}")),
                    Err(e) => lines.push(format!("  ! {} would fail: {e}", t.verb())),
                }
            }
        }
    }
    Ok(lines)
}

/// `mnnn rule tick`.
pub fn rule_tick() -> anyhow::Result<String> {
    let store = open_write()?;
    if let Ok(p) = super::info::paths(None) {
        store.set_host_running(muninn_core::org::host::running(&p.home).is_some());
    }
    let rep = muninn_core::schedule::tick(&store, &muninn_core::schedule::holder("app"), muninn_core::store::now_ms())?;
    Ok(super::schedules::tick_text(&rep))
}

/// `mnnn rule events`.
pub fn rule_events(kind: Option<String>) -> anyhow::Result<Vec<RuleEventInfo>> {
    let want = kind.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(rules::EVENTS
        .iter()
        .filter(|e| want.is_none_or(|k| k == "*" || e.kind == k || k.strip_suffix(".*").is_some_and(|p| e.kind.starts_with(p))))
        .map(|e| RuleEventInfo {
            kind: e.kind.into(),
            about: e.about.into(),
            fields: e.fields.iter().map(|f| f.split(':').next().unwrap_or(f)).collect::<Vec<_>>().join(", "),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::{Store, Then};

    #[test]
    fn rules_create_list_remove_without_running() {
        let s = Store::open_in_memory().unwrap();
        assert!(rules_of(&s).unwrap().is_empty());
        let r = s
            .create_rule(&NewRule {
                name: "imported".into(),
                description: "say so".into(),
                spec: RuleSpec { on: vec!["import".into()], then: vec![Then::Status { text: "hi".into() }], ..Default::default() },
                enabled: true,
                ..Default::default()
            })
            .unwrap();
        let list = rules_of(&s).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "imported");
        assert!(list[0].enabled);
        assert!(list[0].on_text.contains("import"));
        assert!(list[0].then_text.contains("status"));
        s.remove_rule(&r.id).unwrap();
        assert!(rules_of(&s).unwrap().is_empty());
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(rules().is_err());
        assert!(rule_create("x".into(), "".into(), "import".into(), "".into(), "status hi".into(), "".into(), false).is_err());
    }
}
