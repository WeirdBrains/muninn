//! `mnnn rule`: rules (if this action happens, then do that). Thin over [`crate::rules`].
use super::searches::record;
use super::{Ctx, RuleCommand, RuleSpecArgs};
use crate::ingest::time::fmt_ts;
use crate::rules;
use crate::store::{ActionRecord, ItemKind, NewRule, OwnerKind, Rule, RuleChange, RuleSpec, Store};
use anyhow::anyhow;
use serde_json::json;

fn params(r: &Rule) -> serde_json::Value {
    json!({ "rule": r.id, "name": r.name })
}

/// The `--on`, `--if` and `--then` given (`None`: not given).
type Parts = (Option<Vec<String>>, Option<Vec<crate::store::Condition>>, Option<Vec<crate::store::Then>>, Option<crate::store::Schedule>);

impl RuleSpecArgs {
    /// Fill a spec from the flags given (`None` for parts not given).
    fn parts(&self, store: &Store) -> anyhow::Result<Parts> {
        let on = (!self.on.is_empty()).then(|| rules::parse_on(&self.on)).transpose()?;
        let when =
            (!self.when.is_empty()).then(|| self.when.iter().map(|c| rules::parse_condition(c)).collect::<crate::Result<Vec<_>>>()).transpose()?;
        let then =
            (!self.then.is_empty()).then(|| self.then.iter().map(|t| rules::parse_then(store, t)).collect::<crate::Result<Vec<_>>>()).transpose()?;
        let schedule = rules::parse_schedule(self.every.as_deref(), self.cron.as_deref())?;
        Ok((on, when, then, schedule))
    }
}

fn owner(
    store: &Store,
    journal: &Option<String>,
    search: &Option<String>,
    model: &Option<String>,
) -> anyhow::Result<Option<(OwnerKind, Option<String>)>> {
    Ok(if let Some(m) = model {
        Some((OwnerKind::Model, Some(store.find_model(m)?.id)))
    } else if let Some(j) = journal {
        Some((OwnerKind::Journal, Some(store.find_journal(j)?.id)))
    } else if let Some(s) = search {
        Some((OwnerKind::LiveSearch, Some(store.find_live_search(s)?.id)))
    } else {
        None
    })
}

/// The action a rule is checked against: the one given; for a scheduled rule, its
/// `schedule` action; otherwise the newest.
fn action_for(store: &Store, r: &Rule, id: Option<i64>) -> anyhow::Result<ActionRecord> {
    if id.is_none() && r.spec.schedule.is_some() {
        return Ok(rules::schedule_action(r));
    }
    the_action(store, id)
}

fn the_action(store: &Store, id: Option<i64>) -> anyhow::Result<ActionRecord> {
    match id {
        Some(i) => store.action(i)?.ok_or_else(|| anyhow!("no action #{i} (see `mnnn history list`)")),
        None => crate::rules::last_trigger(store)?.ok_or_else(|| anyhow!("nothing in the history yet; give --action")),
    }
}

fn picked(store: &Store, rule: &Option<String>) -> anyhow::Result<Vec<Rule>> {
    Ok(match rule {
        Some(r) => vec![store.find_rule(r)?],
        None => store.rules()?,
    })
}

fn print_check(store: &Store, r: &Rule, c: &rules::Check) {
    println!("rule {}{} vs #{} {}:", r.name, if r.enabled { "" } else { " (off)" }, c.action, c.kind);
    match &c.on {
        Some(p) => println!("  ✓ on {p}"),
        None if c.kind.starts_with("rule.") => println!("  ✗ on {} (rule actions never trigger rules)", r.spec.on.join(", ")),
        None => println!("  ✗ on {}", r.spec.on.join(", ")),
    }
    if let Some(rel) = c.related {
        println!("  {} related to {}", if rel { "✓" } else { "✗" }, rules::home(store, r));
    }
    for k in &c.conditions {
        let actual = k.actual.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "missing".into());
        println!("  {} {}   ({actual})", if k.ok { "✓" } else { "✗" }, k.condition);
    }
    println!("  → {}", if c.matched { "matches" } else { "no match" });
}

pub fn run(ctx: &Ctx, cmd: RuleCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        RuleCommand::Create { name, spec, project, journal, search, model, related, disabled, description } => {
            let (on, when, then, schedule) = spec.parts(&store)?;
            let (kind, id) = owner(&store, &journal, &search, &model)?.unwrap_or((OwnerKind::Standalone, None));
            let project = project.map(|p| store.find_project(&p)).transpose()?;
            let spec = RuleSpec {
                on: on.unwrap_or_default(),
                when: when.unwrap_or_default(),
                then: then.unwrap_or_default(),
                related,
                schedule,
                ..Default::default()
            };
            let r = store.create_rule(&NewRule { name, description, owner_kind: kind, owner_id: id, spec, enabled: !disabled })?;
            record(&store, "rule.create", format!("create rule {}", r.name), params(&r));
            if let Some(p) = project {
                let (it, _) = store.add_project_item(&p.id, ItemKind::RuleSet, &r.id, json!({}))?;
                record(
                    &store,
                    "project.add",
                    format!("add rule {} to project {}", r.name, p.name),
                    json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                );
            }
            ctx.io.info(format!("created rule {} ({}), {}, in {}", r.name, r.short_id(), rules::brief(&r), rules::home(&store, &r)));
        }
        RuleCommand::Edit { rule, name, description, spec, clear_if, journal, search, model, standalone, related, unrelated, no_schedule } => {
            let r = store.find_rule(&rule)?;
            let (on, when, then, schedule) = spec.parts(&store)?;
            let mut sp = r.spec.clone();
            let mut spec_changed = false;
            if let Some(o) = on {
                sp.on = o;
                spec_changed = true;
            }
            if let Some(w) = when {
                sp.when = w;
                spec_changed = true;
            }
            if clear_if {
                sp.when.clear();
                spec_changed = true;
            }
            if let Some(t) = then {
                sp.then = t;
                spec_changed = true;
            }
            if schedule.is_some() || no_schedule {
                sp.schedule = schedule;
                spec_changed = true;
            }
            if related || unrelated {
                sp.related = related;
                spec_changed = true;
            }
            let owner = if standalone { Some((OwnerKind::Standalone, None)) } else { owner(&store, &journal, &search, &model)? };
            let c = RuleChange { name, description, owner, spec: spec_changed.then_some(sp) };
            let n = store.update_rule(&r.id, &c)?;
            record(&store, "rule.edit", format!("edit rule {}", n.name), params(&n));
            ctx.io.info(format!("updated rule {}, {}, in {}", n.name, rules::brief(&n), rules::home(&store, &n)));
        }
        RuleCommand::List { project, journal, search, model, json } => {
            let all = if let Some(p) = project {
                store.project_rules(&store.find_project(&p)?.id)?
            } else if let Some(j) = journal {
                store.owned_rules(OwnerKind::Journal, &store.find_journal(&j)?.id)?
            } else if let Some(s) = search {
                store.owned_rules(OwnerKind::LiveSearch, &store.find_live_search(&s)?.id)?
            } else if let Some(m) = model {
                store.owned_rules(OwnerKind::Model, &store.find_model(&m)?.id)?
            } else {
                store.rules()?
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
            } else {
                for r in &all {
                    println!(
                        "{}\t{}\t{}\t{}\t{}\t{} run{}",
                        r.short_id(),
                        if r.enabled { "on" } else { "off" },
                        r.name,
                        rules::brief(r),
                        rules::home(&store, r),
                        r.runs,
                        if r.runs == 1 { "" } else { "s" }
                    );
                }
                if all.is_empty() {
                    ctx.io.info("no rules; create one with `mnnn rule create NAME --on KIND --then ACTION` (or R in the TUI)");
                }
            }
        }
        RuleCommand::Show { rule, json } => {
            let r = store.find_rule(&rule)?;
            let runs = store.rule_runs(&r.id, 10)?;
            if json {
                let runs: Vec<_> =
                    runs.iter().map(|a| json!({ "action": a.id, "at": a.created_at, "summary": a.summary, "status": a.params["status"] })).collect();
                let then: Vec<_> = r.spec.then.iter().map(|t| rules::then_text(&store, t)).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({ "rule": r, "lives_in": rules::home(&store, &r), "then_text": then, "next_run": rules::next_run(&r), "runs": runs })
                    )?
                );
            } else {
                println!("id:       {}", r.id);
                println!("name:     {}", r.name);
                if !r.description.is_empty() {
                    println!("about:    {}", r.description);
                }
                println!("state:    {}", if r.enabled { "on" } else { "off" });
                println!("lives in: {}", rules::home(&store, &r));
                if let Some(s) = &r.spec.schedule {
                    let next = rules::next_run(&r).map(|t| fmt_ts(t.max(crate::store::now_ms()))).unwrap_or_else(|| "never".into());
                    println!("runs:     {} (next {next}{})", rules::schedule_text(s), if r.enabled { "" } else { ", when on" });
                }
                if !r.spec.on.is_empty() || r.spec.schedule.is_none() {
                    println!("on:       {}{}", r.spec.on.join(", "), if r.spec.related { " (related only)" } else { "" });
                }
                for c in &r.spec.when {
                    println!("if:       {}", rules::condition_text(c));
                }
                for t in &r.spec.then {
                    println!("then:     {}", rules::then_text(&store, t));
                }
                println!("author:   {}", crate::history::author_label(&store, r.author_id.as_deref()));
                println!("created:  {}", fmt_ts(r.created_at));
                println!("runs:     {}{}", r.runs, r.last_status.as_ref().map(|s| format!(" (last: {s})")).unwrap_or_default());
                for a in &runs {
                    println!("  #{} {}  {}", a.id, fmt_ts(a.created_at), a.summary);
                }
            }
        }
        RuleCommand::Remove { rule } => {
            let r = store.find_rule(&rule)?;
            if !ctx.io.confirm(&format!("Remove rule {}?", r.name))? {
                return Ok(());
            }
            let r = store.remove_rule(&r.id)?;
            record(&store, "rule.remove", format!("remove rule {}", r.name), params(&r));
            ctx.io.info(format!("removed rule {}", r.name));
        }
        RuleCommand::Enable { rule } => toggle(ctx, &store, &rule, true)?,
        RuleCommand::Disable { rule } => toggle(ctx, &store, &rule, false)?,
        RuleCommand::Test { rule, action, json } => {
            let mut checks = vec![];
            for r in picked(&store, &rule)? {
                let a = action_for(&store, &r, action)?;
                let c = rules::check(&store, &r, &a);
                checks.push((r, c));
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&checks.iter().map(|(_, c)| c).collect::<Vec<_>>())?);
            } else {
                for (r, c) in &checks {
                    print_check(&store, r, c);
                }
                if checks.is_empty() {
                    ctx.io.info("no rules to test");
                }
            }
        }
        RuleCommand::DryRun { rule, action, json } => {
            store.set_host_running(crate::org::host::running(&ctx.paths.home).is_some());
            let mut out = vec![];
            for r in picked(&store, &rule)? {
                let a = action_for(&store, &r, action)?;
                let c = rules::check(&store, &r, &a);
                let plan: Vec<_> = if c.matched {
                    r.spec.then.iter().map(|t| rules::plan(&store, &r, &a, t).map_err(|e| format!("{} would fail: {e}", t.verb()))).collect()
                } else {
                    vec![]
                };
                out.push((r, c, plan));
            }
            if json {
                let v: Vec<_> = out
                    .iter()
                    .map(|(r, c, p)| {
                        let p: Vec<_> = p
                            .iter()
                            .map(|x| match x {
                                Ok(m) => json!({ "ok": true, "would": m }),
                                Err(e) => json!({ "ok": false, "error": e }),
                            })
                            .collect();
                        json!({ "rule": r.id, "name": r.name, "matched": c.matched, "enabled": r.enabled, "plan": p })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for (r, c, p) in &out {
                    print_check(&store, r, c);
                    if c.matched && !r.enabled {
                        println!("  (off: it would not run now)");
                    }
                    for x in p {
                        match x {
                            Ok(m) => println!("  would {m}"),
                            Err(e) => println!("  ! {e}"),
                        }
                    }
                }
            }
        }
        RuleCommand::Tick { json } => {
            store.set_host_running(crate::org::host::running(&ctx.paths.home).is_some());
            let ran = rules::tick(&store, crate::store::now_ms())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&json!({ "ran": ran }))?);
            } else if ran.is_empty() {
                ctx.io.info("no scheduled rules are due");
            } else {
                ctx.io.info(format!("ran {}", ran.join(", ")));
            }
        }
        RuleCommand::Events { kind, json } => {
            let ev: Vec<_> = rules::EVENTS
                .iter()
                .filter(|e| kind.as_deref().is_none_or(|k| k == "*" || e.kind == k || k.strip_suffix(".*").is_some_and(|p| e.kind.starts_with(p))))
                .collect();
            if json {
                let v: Vec<_> = ev.iter().map(|e| json!({ "kind": e.kind, "about": e.about, "fields": e.fields })).collect();
                println!("{}", serde_json::to_string_pretty(&json!({ "common": rules::COMMON_FIELDS, "events": v }))?);
            } else {
                println!("every action: {}", rules::COMMON_FIELDS.iter().map(|f| f.split(':').next().unwrap_or(f)).collect::<Vec<_>>().join(", "));
                for e in ev {
                    println!("{}\t{}\t{}", e.kind, e.about, e.fields.iter().map(|f| f.split(':').next().unwrap_or(f)).collect::<Vec<_>>().join(", "));
                }
            }
        }
    }
    Ok(())
}

fn toggle(ctx: &Ctx, store: &Store, rule: &str, on: bool) -> anyhow::Result<()> {
    let r = store.find_rule(rule)?;
    let r = store.set_rule_enabled(&r.id, on)?;
    let (kind, verb) = if on { ("rule.enable", "on") } else { ("rule.disable", "off") };
    record(store, kind, format!("switch rule {} {verb}", r.name), params(&r));
    ctx.io.info(format!("rule {} is {verb}", r.name));
    Ok(())
}
