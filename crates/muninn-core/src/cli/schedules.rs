//! `mnnn ruleset`, `mnnn schedule`, `mnnn scheduler` (2.25). Thin over [`crate::schedule`].
use super::searches::record;
use super::{Ctx, RulesetCommand, RunPolicyArgs, ScheduleCommand, SchedulerCommand, TimingArgs};
use crate::schedule::{self, Past, Timing, show_time};
use crate::store::{ScheduleDef, Store, now_ms};
use anyhow::bail;
use serde_json::json;

fn set_json(store: &Store, r: &crate::store::Ruleset) -> anyhow::Result<serde_json::Value> {
    Ok(json!({
        "id": r.id, "name": r.name, "description": r.description, "owner_kind": r.owner_kind, "owner_id": r.owner_id, "enabled": r.enabled,
        "rules": store.ruleset_rules(&r.id)?.iter().map(|x| json!({ "id": x.id, "name": x.name, "enabled": x.enabled })).collect::<Vec<_>>(),
        "schedules": store.schedules_using("ruleset_id", &r.id)?.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
    }))
}

pub fn ruleset(ctx: &Ctx, cmd: RulesetCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        RulesetCommand::List { json } => {
            let sets = store.rulesets()?;
            if json {
                let v = sets.iter().map(|r| set_json(&store, r)).collect::<anyhow::Result<Vec<_>>>()?;
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if sets.is_empty() {
                ctx.io.info("no rule sets (mnnn ruleset create NAME --rule RULE)");
            } else {
                for r in sets {
                    let n = store.ruleset_rules(&r.id)?.len();
                    let s = store.schedules_using("ruleset_id", &r.id)?.len();
                    println!("{}  {}{}  {n} rule(s), {s} schedule(s){}", crate::store::short(&r.id), r.name, if r.enabled { "" } else { " (off)" }, if r.owner_kind == "standalone" { String::new() } else { format!(", {}", r.owner_kind) });
                }
            }
        }
        RulesetCommand::Show { ruleset, json } => {
            let r = store.find_ruleset(&ruleset)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&set_json(&store, &r)?)?);
            } else {
                println!("rule set {}{} ({})", r.name, if r.enabled { "" } else { " (off)" }, r.owner_kind);
                if !r.description.is_empty() {
                    println!("  {}", r.description);
                }
                for (i, x) in store.ruleset_rules(&r.id)?.iter().enumerate() {
                    println!("  {}. {}{}  {}", i + 1, x.name, if x.enabled { "" } else { " (off)" }, crate::rules::brief(x));
                }
                for s in store.schedules_using("ruleset_id", &r.id)? {
                    println!("  run by schedule {}: {}", s.name, schedule::describe(&s).join("; "));
                }
            }
        }
        RulesetCommand::Create { name, description, rules } => {
            let found = rules.iter().map(|k| store.find_rule(k)).collect::<crate::Result<Vec<_>>>()?;
            let r = store.create_ruleset(&name, &description, "standalone", None)?;
            for x in &found {
                store.add_ruleset_rule(&r.id, &x.id, None)?;
            }
            record(&store, "ruleset.create", format!("create rule set {}", r.name), json!({ "ruleset": r.id, "name": r.name, "rules": found.iter().map(|x| &x.id).collect::<Vec<_>>() }));
            ctx.io.info(format!("created rule set {} ({}) with {} rule(s)", r.name, crate::store::short(&r.id), found.len()));
        }
        RulesetCommand::Edit { ruleset, name, description, enable, disable } => {
            let r = store.find_ruleset(&ruleset)?;
            let mut r2 = store.edit_ruleset(&r.id, name.as_deref(), description.as_deref())?;
            if enable || disable {
                store.set_ruleset_enabled(&r.id, enable)?;
                r2.enabled = enable;
            }
            record(&store, "ruleset.edit", format!("edit rule set {}", r2.name), json!({ "ruleset": r.id, "name": r2.name, "enabled": r2.enabled }));
            ctx.io.info(format!("rule set {}{}", r2.name, if r2.enabled { "" } else { " (off)" }));
        }
        RulesetCommand::Remove { ruleset, force } => {
            let r = store.find_ruleset(&ruleset)?;
            let using = store.schedules_using("ruleset_id", &r.id)?;
            if !using.is_empty() && !force {
                bail!(
                    "schedules run rule set {}: {} (remove them, or give --force to remove them too)",
                    r.name,
                    using.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                );
            }
            if !ctx.io.confirm(&format!("remove rule set {}{}?", r.name, if using.is_empty() { String::new() } else { format!(" and {} schedule(s)", using.len()) }))? {
                bail!("not removed");
            }
            for s in &using {
                store.remove_schedule(&s.id)?;
            }
            store.remove_ruleset(&r.id)?;
            record(&store, "ruleset.remove", format!("remove rule set {}", r.name), json!({ "ruleset": r.id, "name": r.name, "schedules": using.iter().map(|s| &s.id).collect::<Vec<_>>() }));
            ctx.io.info(format!("removed rule set {} (its rules stay)", r.name));
        }
        RulesetCommand::Add { ruleset, rule, position } => {
            let r = store.find_ruleset(&ruleset)?;
            let x = store.find_rule(&rule)?;
            store.add_ruleset_rule(&r.id, &x.id, position)?;
            record(&store, "ruleset.add", format!("add rule {} to rule set {}", x.name, r.name), json!({ "ruleset": r.id, "rule": x.id }));
            ctx.io.info(format!("rule set {}: {}", r.name, store.ruleset_rules(&r.id)?.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")));
        }
        RulesetCommand::Drop { ruleset, rule } => {
            let r = store.find_ruleset(&ruleset)?;
            let x = store.find_rule(&rule)?;
            if !store.drop_ruleset_rule(&r.id, &x.id)? {
                bail!("rule {} is not in rule set {}", x.name, r.name);
            }
            record(&store, "ruleset.drop", format!("drop rule {} from rule set {}", x.name, r.name), json!({ "ruleset": r.id, "rule": x.id }));
            ctx.io.info(format!("dropped {} from {}", x.name, r.name));
        }
        RulesetCommand::Run { ruleset, json } => {
            let r = store.find_ruleset(&ruleset)?;
            let aid = store.record_action(
                &crate::store::NewAction::new(crate::rules::SCHEDULE, "cli", format!("run rule set {}", r.name)).params(json!({ "ruleset": r.id, "name": r.name, "manual": true })),
            )?;
            let a = store.action(aid)?.ok_or_else(|| anyhow::anyhow!("action #{aid} is gone"))?;
            let res = schedule::run_ruleset(&store, &r.id, &a)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&res)?);
            } else {
                for m in &res.messages {
                    ctx.io.info(m);
                }
                ctx.io.info(format!("ran {} rule(s){}", res.ran.len(), if res.failed.is_empty() { String::new() } else { format!(", failed: {}", res.failed.join(", ")) }));
            }
            if !res.failed.is_empty() {
                bail!("{} rule(s) failed", res.failed.len());
            }
        }
    }
    Ok(())
}

fn timing(t: &TimingArgs, run_every: Option<String>) -> Timing {
    Timing {
        run_every,
        cron: t.cron.clone(),
        start_at: t.start_at.clone(),
        ends_at: t.ends_at.clone(),
        for_: t.for_.clone(),
        tz: t.tz.clone(),
        past_next: t.past.as_deref() == Some("next-interval"),
    }
}

fn policy(s: &mut ScheduleDef, p: &RunPolicyArgs) -> anyhow::Result<()> {
    if let Some(o) = &p.overlap {
        s.overlap = o.clone();
    }
    if let Some(c) = &p.catch_up {
        s.catch_up = c.clone();
    }
    if let Some(m) = p.max_attempts {
        if !(1..=20).contains(&m) {
            bail!("--max-attempts: 1 to 20");
        }
        s.max_attempts = m;
    }
    Ok(())
}

/// Apply timing; a past start_at asks (on a terminal): the next interval, or a new start.
fn apply(ctx: &Ctx, s: &mut ScheduleDef, mut t: Timing, now: i64) -> anyhow::Result<()> {
    let base = s.clone();
    loop {
        let mut probe = base.clone();
        // check the past case first, so the question can name the next slot
        let past = match schedule::apply_timing(&mut probe, &Timing { past_next: false, ..t.clone() }, now) {
            Err(e) if e.to_string().contains("in the past") && probe.start_at.is_some() && !t.past_next => schedule::past_of(&probe, now),
            _ => Past::No,
        };
        if let Past::Yes { next } = past
            && ctx.io.interactive
        {
            let when = show_time(probe.start_at.unwrap_or(now), probe.tz.as_deref());
            let ans = match next {
                Some(n) => ctx.io.prompt(
                    "start_at",
                    &format!("start_at {when} is in the past: [n]ext interval ({}) or [c]orrect it", show_time(n, probe.tz.as_deref())),
                    Some("n"),
                    "--past next-interval",
                )?,
                None => "c".into(),
            };
            if ans.starts_with('n') && next.is_some() {
                t.past_next = true;
            } else {
                let fresh = ctx.io.prompt("start_at", &format!("start_at {when} is in the past; a new start"), None, "--start-at DATE")?;
                t.start_at = Some(fresh);
            }
            continue;
        }
        *s = base.clone();
        schedule::apply_timing(s, &t, now)?;
        return Ok(());
    }
}

fn print_schedule(store: &Store, s: &ScheduleDef) -> anyhow::Result<()> {
    let target = match s.target_kind.as_str() {
        "ruleset" => format!("rule set {}", s.ruleset_id.as_deref().and_then(|id| store.ruleset(id).ok().flatten()).map_or("(removed)".into(), |r| r.name)),
        "connector" => format!(
            "connector {} → source {}",
            s.plugin_id.as_deref().and_then(|id| store.space(id).ok().flatten()).map_or("(removed)".into(), |x| x.name),
            s.source_id.as_deref().and_then(|id| store.source_by_id(id).ok().flatten()).map_or("(removed)".into(), |x| x.name)
        ),
        k => k.to_string(),
    };
    println!("schedule {}{} → {target}", s.name, if s.enabled { "" } else { " (off)" });
    for l in schedule::describe(s) {
        println!("  {l}");
    }
    println!("  overlap {}, catch up {}, {} attempt(s)", s.overlap, s.catch_up, s.max_attempts);
    Ok(())
}

fn run_line(r: &crate::store::ScheduleRun, tz: Option<&str>) -> String {
    format!(
        "#{} {} due {}{}{}",
        r.id,
        r.status,
        show_time(r.timestamp, tz),
        if r.attempt > 1 { format!(" (attempt {})", r.attempt) } else { String::new() },
        r.error.as_ref().map(|e| format!(": {e}")).unwrap_or_default()
    )
}

pub fn schedule(ctx: &Ctx, cmd: ScheduleCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let now = now_ms();
    match cmd {
        ScheduleCommand::Add { name, run_every, ruleset, plugin, source, pipeline, connector_ruleset, options, timing: t, policy: p, disabled } => {
            let mut s = ScheduleDef {
                name,
                overlap: "skip".into(),
                catch_up: "once".into(),
                max_attempts: 3,
                enabled: !disabled,
                created_at: now,
                ..Default::default()
            };
            match (ruleset, plugin) {
                (Some(rs), None) => {
                    let set = store.find_ruleset(&rs)?;
                    s.target_kind = "ruleset".into();
                    s.ruleset_id = Some(set.id.clone());
                    policy(&mut s, &p)?;
                    apply(ctx, &mut s, timing(&t, run_every), now)?;
                    let s = store.save_schedule(&s)?;
                    record(&store, "schedule.add", format!("add schedule {} for rule set {}", s.name, set.name), json!({ "schedule": s.id, "name": s.name, "ruleset": set.id }));
                    print_schedule(&store, &s)?;
                }
                (None, Some(plug)) => {
                    let space = store.find_space(&plug)?;
                    if space.kind != crate::store::SpaceKind::IngestConnector {
                        bail!("{} is a {} space; connector schedules need kind ingest-connector", space.name, space.kind);
                    }
                    let src_name = source.ok_or_else(|| anyhow::anyhow!("--plugin needs --source"))?;
                    let src = store.source_by_name(&src_name)?.ok_or_else(|| anyhow::anyhow!("no source `{src_name}` (`mnnn sources list`)"))?;
                    let pipe = pipeline.map(|n| store.find_pipeline(&n)).transpose()?;
                    let rset = connector_ruleset.map(|n| store.find_ruleset(&n)).transpose()?;
                    let mut opts = serde_json::Map::new();
                    for o in options {
                        let (k, v) = o.split_once('=').ok_or_else(|| anyhow::anyhow!("--option KEY=VALUE"))?;
                        opts.insert(k.trim().into(), json!(v));
                    }
                    s.target_kind = "connector".into();
                    s.plugin_id = Some(space.id.clone());
                    s.source_id = Some(src.id.clone());
                    s.pipeline_id = pipe.as_ref().map(|p| p.id.clone());
                    s.ruleset_id = rset.as_ref().map(|r| r.id.clone());
                    s.options = opts;
                    policy(&mut s, &p)?;
                    apply(ctx, &mut s, timing(&t, run_every), now)?;
                    let s = store.save_schedule(&s)?;
                    record(
                        &store,
                        "schedule.add",
                        format!("add schedule {} for connector {}", s.name, space.name),
                        json!({ "schedule": s.id, "name": s.name, "plugin": space.id, "source": src.id }),
                    );
                    print_schedule(&store, &s)?;
                }
                _ => bail!("give --ruleset SET or --plugin CONNECTOR --source SRC"),
            }
        }
        ScheduleCommand::Parse { run_every, timing: t, json } => {
            let mut s = ScheduleDef { name: "parse".into(), target_kind: "ruleset".into(), enabled: true, created_at: now, ..Default::default() };
            apply(ctx, &mut s, timing(&t, run_every), now)?;
            if json {
                let next: Vec<i64> = std::iter::successors(Some(s.next_run_at), |x| schedule::step(&s, *x)).take_while(|x| s.ends_at.is_none_or(|e| *x < e)).take(5).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({ "run_every": s.run_every, "every_ms": s.every_ms, "every_months": s.every_months, "cron": s.cron, "tz": s.tz, "start_at": s.start_at, "ends_at": s.ends_at, "next": next }))?
                );
            } else {
                for l in schedule::describe(&s) {
                    println!("{l}");
                }
                let mut t = s.next_run_at;
                for _ in 0..3 {
                    let Some(n) = schedule::step(&s, t).filter(|n| s.ends_at.is_none_or(|e| *n < e)) else { break };
                    println!("then   {}", show_time(n, s.tz.as_deref()));
                    t = n;
                }
            }
        }
        ScheduleCommand::List { json } => {
            let all = store.schedules()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
            } else if all.is_empty() {
                ctx.io.info("no schedules (mnnn schedule add NAME 1h --ruleset SET)");
            } else {
                for s in all {
                    let when = s.cron.clone().map(|c| format!("cron {c}")).or(s.run_every.clone().map(|e| if e == "once" { e } else { format!("every {e}") })).unwrap_or_default();
                    println!(
                        "{}  {}{}  {when}  next {}",
                        crate::store::short(&s.id),
                        s.name,
                        if s.enabled { "" } else { " (off)" },
                        if s.enabled { show_time(s.next_run_at, s.tz.as_deref()) } else { "-".into() }
                    );
                }
            }
        }
        ScheduleCommand::Show { schedule: key, json } => {
            let s = store.find_schedule(&key)?;
            let runs = store.schedule_runs(&s.id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&json!({ "schedule": s, "runs": runs.iter().rev().take(10).collect::<Vec<_>>() }))?);
            } else {
                print_schedule(&store, &s)?;
                for r in runs.iter().rev().take(5) {
                    println!("  {}", run_line(r, s.tz.as_deref()));
                }
            }
        }
        ScheduleCommand::Edit { schedule: key, name, every, ruleset, timing: t, policy: p, no_end } => {
            let mut s = store.find_schedule(&key)?;
            if let Some(n) = name {
                s.name = n;
            }
            if let Some(r) = ruleset {
                s.ruleset_id = Some(store.find_ruleset(&r)?.id);
            }
            policy(&mut s, &p)?;
            if no_end {
                s.ends_at = None;
                s.ends_at_input = None;
            }
            let changes_timing = every.is_some() || t.cron.is_some() || t.start_at.is_some() || t.ends_at.is_some() || t.for_.is_some() || t.tz.is_some() || no_end;
            if changes_timing {
                let mut tm = timing(&t, every);
                if tm.run_every.is_none() && tm.cron.is_none() {
                    tm.run_every = s.run_every.clone().filter(|_| s.cron.is_none());
                    tm.cron = s.cron.clone();
                }
                // keep the old start unless a new one is given (a past one moves on)
                if tm.start_at.is_none() && s.start_at.is_some_and(|x| x < now) && !s.is_once() {
                    tm.past_next = true;
                }
                if tm.start_at.is_none() {
                    tm.start_at = s.start_at.map(|x| format!("@{}.{:03}", x.div_euclid(1000), x.rem_euclid(1000)));
                }
                let input = s.start_at_input.clone();
                apply(ctx, &mut s, tm, now)?;
                if t.start_at.is_none() {
                    s.start_at_input = input;
                }
            }
            let s = store.save_schedule(&s)?;
            record(&store, "schedule.edit", format!("edit schedule {}", s.name), json!({ "schedule": s.id, "name": s.name }));
            print_schedule(&store, &s)?;
        }
        ScheduleCommand::Enable { schedule: key } => {
            let mut s = store.find_schedule(&key)?;
            schedule::set_enabled(&store, &mut s, true, now)?;
            record(&store, "schedule.enable", format!("enable schedule {}", s.name), json!({ "schedule": s.id }));
            ctx.io.info(format!("schedule {} on, next {}", s.name, show_time(s.next_run_at, s.tz.as_deref())));
        }
        ScheduleCommand::Disable { schedule: key } => {
            let mut s = store.find_schedule(&key)?;
            schedule::set_enabled(&store, &mut s, false, now)?;
            record(&store, "schedule.disable", format!("disable schedule {}", s.name), json!({ "schedule": s.id }));
            ctx.io.info(format!("schedule {} off (its history stays)", s.name));
        }
        ScheduleCommand::Remove { schedule: key } => {
            let s = store.find_schedule(&key)?;
            if !ctx.io.confirm(&format!("remove schedule {} and its {} run(s)?", s.name, store.schedule_runs(&s.id)?.len()))? {
                bail!("not removed");
            }
            store.remove_schedule(&s.id)?;
            record(&store, "schedule.remove", format!("remove schedule {}", s.name), json!({ "schedule": s.id, "name": s.name }));
            ctx.io.info(format!("removed schedule {}", s.name));
        }
        ScheduleCommand::RunNow { schedule: key, json } => {
            let s = store.find_schedule(&key)?;
            let r = schedule::run_now(&store, &s, &schedule::holder("cli"), now)?;
            record(&store, "schedule.run", format!("run schedule {} now", s.name), json!({ "schedule": s.id, "run": r.id, "status": r.status }));
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                ctx.io.info(run_line(&r, s.tz.as_deref()));
            }
            if r.status != "success" {
                bail!("run #{} failed", r.id);
            }
        }
        ScheduleCommand::Runs { schedule: key, limit, json } => {
            let s = store.find_schedule(&key)?;
            let runs: Vec<_> = store.schedule_runs(&s.id)?.into_iter().rev().take(limit).collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else if runs.is_empty() {
                ctx.io.info(format!("schedule {} has not run yet", s.name));
            } else {
                for r in runs {
                    println!("{}", run_line(&r, s.tz.as_deref()));
                }
            }
        }
    }
    Ok(())
}

pub fn scheduler(ctx: &Ctx, cmd: SchedulerCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let holder = schedule::holder("cli");
    let show = |rep: &schedule::TickReport| {
        if rep.standby {
            ctx.io.info("another scheduler holds the lease (standby)");
            return;
        }
        for n in &rep.missed {
            ctx.io.warn(format!("{n}: missed its one run; disabled"));
        }
        if !rep.started.is_empty() {
            ctx.io.info(format!("{} run(s): {} ok, {} failed", rep.started.len(), rep.succeeded.len(), rep.failed.len()));
        }
        if !rep.legacy.is_empty() {
            ctx.io.info(format!("ran {}", rep.legacy.join(", ")));
        }
    };
    match cmd {
        SchedulerCommand::Tick { json } => {
            store.set_host_running(crate::org::host::running(&ctx.paths.home).is_some());
            let rep = schedule::tick(&store, &holder, now_ms())?;
            store.release_lease(&holder)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rep)?);
            } else if rep.started.is_empty() && rep.missed.is_empty() && rep.legacy.is_empty() && !rep.standby {
                ctx.io.info("nothing is due");
            } else {
                show(&rep);
            }
        }
        SchedulerCommand::Run { every } => {
            let every = every.clamp(1, 60);
            ctx.io.info(format!("scheduler {holder}: a pass every {every} s (Ctrl-C stops)"));
            loop {
                store.set_host_running(crate::org::host::running(&ctx.paths.home).is_some());
                match schedule::tick(&store, &holder, now_ms()) {
                    Ok(rep) => show(&rep),
                    Err(e) => ctx.io.warn(format!("scheduler: {e}")),
                }
                std::thread::sleep(std::time::Duration::from_secs(every));
            }
        }
        SchedulerCommand::Status { json } => {
            let v = schedule::status(&store, now_ms())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                match v["lease"]["holder"].as_str() {
                    Some(h) => println!("running: {h}"),
                    None => println!("not running (the TUI, an org host or `mnnn scheduler run` runs it)"),
                }
                println!("open runs: {}", v["open"]);
                if let (Some(n), Some(at)) = (v["next"]["schedule"].as_str(), v["next"]["at"].as_i64()) {
                    println!("next: {n} at {}", show_time(at, None));
                }
            }
        }
    }
    Ok(())
}
