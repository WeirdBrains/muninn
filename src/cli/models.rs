//! `mnnn model`: designed record tables and their records. Thin over
//! [`crate::store`]'s model and record API and [`crate::designer`].
use super::searches::record;
use super::{Ctx, ModelCommand, RecordCommand};
use crate::designer::{self as d, Act};
use crate::ingest::time::fmt_ts;
use crate::store::{ApplyOpts, FieldOp, FieldType, ItemKind, Model, OwnerKind, Plan, Record, RecordQuery, Store};
use anyhow::anyhow;
use serde_json::{Value, json};

fn note(store: &Store, a: Act) {
    record(store, a.kind, a.summary, a.params);
}

fn print_plan(store: &Store, p: &Plan) {
    for l in d::plan_lines(store, p) {
        println!("  {l}");
    }
}

/// Ask before a migration that loses values; refuse a blocked one.
fn check_plan(ctx: &Ctx, store: &Store, m: &Model, p: &Plan) -> anyhow::Result<bool> {
    if !p.blocked.is_empty() {
        return Err(anyhow!("{} cannot be migrated as it is:\n  {}", m.name, p.blocked.join("\n  ")));
    }
    let lost = p.lost();
    if lost == 0 {
        return Ok(false);
    }
    print_plan(store, p);
    if !ctx.io.confirm(&format!("this throws away {lost} value(s) of {}; go ahead?", m.name))? {
        return Err(anyhow!("nothing changed"));
    }
    Ok(true)
}

fn fields_of(specs: &[String]) -> String {
    specs.join("; ")
}

fn version_arg(s: &str) -> anyhow::Result<i64> {
    s.trim().trim_start_matches(['v', 'V']).parse().map_err(|_| anyhow!("`{s}`: give a version number, e.g. 2 or v2"))
}

fn print_model(store: &Store, m: &Model) -> anyhow::Result<()> {
    println!("{}{} · {}", m.name, if m.builtin { " (built in)" } else { "" }, d::status_text(store, m));
    if !m.description.is_empty() {
        println!("  {}", m.description);
    }
    println!("fields:");
    if m.design.fields.is_empty() {
        println!("  (none)");
    }
    for f in &m.design.fields {
        println!("  {}", d::field_text(store, f));
        if !f.about.is_empty() {
            println!("      {}", f.about);
        }
    }
    if let Some(dr) = &m.draft {
        println!("draft (not applied; `mnnn model apply {}`):", m.name);
        print_plan(store, &store.plan_model(m, dr)?);
    }
    if m.version > 0 {
        println!("records: {}", store.count_records(m)?);
    }
    let rules = store.owned_rules(OwnerKind::Model, &m.id)?;
    if !rules.is_empty() {
        println!("rules: {}", rules.iter().map(|r| format!("{}{}", r.name, if r.enabled { "" } else { " (off)" })).collect::<Vec<_>>().join(", "));
    }
    let refs = store.models_referring_to(&m.id)?;
    if !refs.is_empty() {
        println!("referred to by: {}", refs.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

fn cell(v: Option<&Value>) -> String {
    let s = v.map(d::value_text).unwrap_or_default();
    let s = s.replace('\n', " ");
    if s.chars().count() > 40 { format!("{}…", s.chars().take(39).collect::<String>()) } else { s }
}

/// Rows as an aligned table.
fn table(head: &[String], rows: &[Vec<String>]) {
    let mut w: Vec<usize> = head.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.chars().count());
        }
    }
    let line = |cells: &[String]| {
        let parts: Vec<String> = cells.iter().enumerate().map(|(i, c)| format!("{c:<width$}", width = w[i])).collect();
        println!("{}", parts.join("  ").trim_end());
    };
    line(head);
    for r in rows {
        line(r);
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn print_records(m: &Model, rs: &[Record]) {
    let names = crate::store::records::field_names(m);
    let mut head = vec!["id".to_string()];
    head.extend(names.iter().cloned());
    let rows: Vec<Vec<String>> = rs
        .iter()
        .map(|r| {
            let mut row = vec![short(&r.id)];
            row.extend(names.iter().map(|n| cell(r.values.get(n))));
            row
        })
        .collect();
    table(&head, &rows);
}

fn list_arg(items: &[String]) -> Vec<String> {
    items.iter().flat_map(|s| s.split(',')).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub fn run(ctx: &Ctx, cmd: ModelCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        ModelCommand::List { json } => {
            let all = store.models()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
                return Ok(());
            }
            let rows: Vec<Vec<String>> = all
                .iter()
                .map(|m| {
                    let n = if m.version > 0 { store.count_records(m).map(|n| n.to_string()).unwrap_or_default() } else { "-".into() };
                    vec![m.name.clone(), d::status_text(&store, m), m.pending().fields.len().to_string(), n, m.description.clone()]
                })
                .collect();
            table(&["model", "version", "fields", "records", "description"].map(String::from), &rows);
        }
        ModelCommand::Types => {
            for t in FieldType::NAMES {
                println!("{t}");
            }
        }
        ModelCommand::Create { name, fields, description, draft, project } => {
            let project = project.map(|p| store.find_project(&p)).transpose()?;
            // parse against an empty model first so refs to the new model itself work
            let (m, _) = store.create_model(&name, &description, Default::default(), false)?;
            let set = (|| -> crate::Result<Model> {
                let design = crate::store::Design { fields: d::parse_fields(&store, &m, &fields_of(&fields))? };
                store.set_model_draft(&m.id, Some(design))
            })();
            let m = match set {
                Ok(m) => m,
                Err(e) => {
                    let _ = store.remove_model(&m.id, true);
                    return Err(e.into());
                }
            };
            note(&store, d::act_model_create(&m));
            let m = if draft {
                ctx.io.info(format!(
                    "created model {} as a draft with {} field(s); `mnnn model apply {}` makes it v1",
                    m.name,
                    m.pending().fields.len(),
                    m.name
                ));
                m
            } else {
                let (m, p) = store.apply_model(&m.id, &ApplyOpts { destructive: false, note: String::new() })?;
                note(&store, d::act_model_apply(&store, &m, &p, false));
                ctx.io.info(format!("created model {} v1 with {} field(s)", m.name, m.design.fields.len()));
                m
            };
            if let Some(p) = project {
                let (it, _) = store.add_project_item(&p.id, ItemKind::Model, &m.id, json!({}))?;
                record(
                    &store,
                    "project.add",
                    format!("add model {} to project {}", m.name, p.name),
                    json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                );
            }
        }
        ModelCommand::Edit { model, add, change, drop, rename, moves, name, description, apply } => {
            let mut m = store.find_model(&model)?;
            if name.is_some() || description.is_some() {
                let old = m.name.clone();
                m = store.rename_model(&m.id, name.as_deref(), description.as_deref())?;
                note(&store, d::act_model_edit(&m, &[], (old != m.name).then_some(old.as_str())));
                ctx.io.info(format!("model {} updated", m.name));
            }
            let mut ops: Vec<FieldOp> = vec![];
            for r in &rename {
                ops.push(d::parse_rename(r)?);
            }
            ops.extend(change.into_iter().map(|spec| FieldOp::Change { spec }));
            ops.extend(drop.into_iter().map(|field| FieldOp::Drop { field }));
            ops.extend(add.into_iter().map(|spec| FieldOp::Add { spec }));
            for mv in &moves {
                ops.push(d::parse_move(mv)?);
            }
            if !ops.is_empty() {
                m = store.edit_model(&m.id, &ops)?;
                note(&store, d::act_model_edit(&m, &ops, None));
                ctx.io.info(format!("draft of {}: {}", m.name, ops.iter().map(d::op_text).collect::<Vec<_>>().join(", ")));
            }
            if apply {
                apply_draft(ctx, &store, &m)?;
            } else if !ops.is_empty() {
                if let Some(dr) = &m.draft {
                    print_plan(&store, &store.plan_model(&m, dr)?);
                }
                ctx.io.info(format!("`mnnn model apply {}` applies it", m.name));
            }
        }
        ModelCommand::Show { model, json } => {
            let m = store.find_model(&model)?;
            if json {
                let rules = store.owned_rules(OwnerKind::Model, &m.id)?;
                let records = if m.version > 0 { Some(store.count_records(&m)?) } else { None };
                println!("{}", serde_json::to_string_pretty(&json!({ "model": m, "records": records, "rules": rules }))?);
            } else {
                print_model(&store, &m)?;
            }
        }
        ModelCommand::Plan { model, then, json } => {
            let m = store.find_model(&model)?;
            let Some(dr) = &m.draft else {
                ctx.io.info(format!("{} has no draft (nothing to apply)", m.name));
                return Ok(());
            };
            if then {
                let ops = d::diff_ops(&store, &m.design, dr);
                println!("model-edit {} {}", m.name, ops.iter().map(d::op_arg).collect::<Vec<_>>().join(" "));
                return Ok(());
            }
            let p = store.plan_model(&m, dr)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                println!("{} v{} → v{} ({} record(s)):", m.name, p.from, p.to, p.rows);
                print_plan(&store, &p);
            }
        }
        ModelCommand::Apply { model, dry_run } => {
            let m = store.find_model(&model)?;
            if m.draft.is_none() {
                ctx.io.info(format!("{} has no draft (nothing to apply)", m.name));
                return Ok(());
            }
            if dry_run {
                let p = store.plan_model(&m, m.draft.as_ref().unwrap())?;
                println!("{} v{} → v{} ({} record(s)):", m.name, p.from, p.to, p.rows);
                print_plan(&store, &p);
            } else {
                apply_draft(ctx, &store, &m)?;
            }
        }
        ModelCommand::Discard { model } => {
            let m = store.find_model(&model)?;
            if m.draft.is_none() {
                ctx.io.info(format!("{} has no draft", m.name));
                return Ok(());
            }
            let m = store.set_model_draft(&m.id, None)?;
            note(&store, d::act_model_discard(&m));
            ctx.io.info(format!("draft of {} thrown away", m.name));
        }
        ModelCommand::History { model, json } => {
            let m = store.find_model(&model)?;
            let vs = store.model_versions(&m.id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&vs)?);
                return Ok(());
            }
            if vs.is_empty() {
                ctx.io.info(format!("{} has no applied versions yet", m.name));
            }
            for v in &vs {
                let who = v.author_id.as_deref().and_then(|a| store.author(a).ok().flatten()).map(|a| a.name).unwrap_or_default();
                let mut extra = vec![];
                if !v.note.is_empty() {
                    extra.push(v.note.clone());
                }
                if !who.is_empty() {
                    extra.push(who);
                }
                println!(
                    "v{}{}  {}  {} field(s), {} step(s){}",
                    v.version,
                    if v.version == m.version { "*" } else { " " },
                    fmt_ts(v.created_at),
                    v.design.fields.len(),
                    v.steps.len(),
                    if extra.is_empty() { String::new() } else { format!("  ({})", extra.join(", ")) }
                );
                for s in &v.steps {
                    println!("      {} {}", d::step_mark(s), d::step_text(&store, s));
                }
            }
        }
        ModelCommand::Rollback { model, version, dry_run } => {
            let m = store.find_model(&model)?;
            let to = version_arg(&version)?;
            let p = store.rollback_plan(&m.id, to)?;
            if dry_run {
                println!("{} v{} → v{} (the design of v{to}):", m.name, p.from, p.to);
                print_plan(&store, &p);
                return Ok(());
            }
            if m.draft.is_some() && !ctx.io.confirm(&format!("{} has a draft; roll back anyway (the draft is thrown away)?", m.name))? {
                return Err(anyhow!("nothing changed"));
            }
            let destructive = check_plan(ctx, &store, &m, &p)?;
            let (n, p) = store.rollback_model(&m.id, to, &ApplyOpts { destructive, note: String::new() })?;
            note(&store, d::act_model_apply(&store, &n, &p, true));
            ctx.io.info(format!("{} is now v{} (the design of v{to})", n.name, n.version));
        }
        ModelCommand::Remove { model } => {
            let m = store.find_model(&model)?;
            if m.builtin {
                return Err(anyhow!("{} is built in and cannot be removed (its added fields can be dropped)", m.name));
            }
            let n = if m.version > 0 { store.count_records(&m)? } else { 0 };
            if !ctx.io.confirm(&format!("remove model {} with its {n} record(s)?", m.name))? {
                return Err(anyhow!("nothing removed"));
            }
            let (m, n) = store.remove_model(&m.id, true)?;
            note(&store, d::act_model_remove(&m, n));
            ctx.io.info(format!("removed model {} ({n} record(s))", m.name));
        }
        ModelCommand::Record(c) => records(ctx, &store, c)?,
    }
    Ok(())
}

fn apply_draft(ctx: &Ctx, store: &Store, m: &Model) -> anyhow::Result<()> {
    let Some(dr) = &m.draft else {
        ctx.io.info(format!("{} has no draft (nothing to apply)", m.name));
        return Ok(());
    };
    let p = store.plan_model(m, dr)?;
    let destructive = check_plan(ctx, store, m, &p)?;
    let (n, p) = store.apply_model(&m.id, &ApplyOpts { destructive, note: String::new() })?;
    note(store, d::act_model_apply(store, &n, &p, false));
    ctx.io.info(format!("{} is now v{} ({} step(s), {} record(s))", n.name, n.version, p.steps.len(), p.rows));
    Ok(())
}

fn records(ctx: &Ctx, store: &Store, cmd: RecordCommand) -> anyhow::Result<()> {
    match cmd {
        RecordCommand::Create { model, values, json } => {
            let m = store.find_model(&model)?;
            let r = store.create_record(&m, &d::parse_assignments(&values)?)?;
            note(store, d::act_record_create(&m, &r));
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                ctx.io.info(format!("created {} record {}", m.name, r.id));
            }
        }
        RecordCommand::Update { model, id, conditions, values, dry_run } => {
            let m = store.find_model(&model)?;
            let changes = d::parse_assignments(&values)?;
            if let Some(id) = id {
                let r = store.find_record(&m, &id)?;
                if dry_run {
                    ctx.io.info(format!("would change {} record {}", m.name, r.id));
                    return Ok(());
                }
                let r = store.update_record(&m, &r.id, &changes)?;
                note(store, d::act_record_update(&m, Some(&r), 1, &[], &changes));
                ctx.io.info(format!("updated {} record {}", m.name, r.id));
            } else {
                let filter = d::parse_where(&conditions)?;
                let n = store.records(&m, &RecordQuery { filter: filter.clone(), ..Default::default() })?.len();
                if dry_run || n == 0 {
                    ctx.io.info(format!("{} {n} {} record(s)", if dry_run { "would change" } else { "no match among" }, m.name));
                    return Ok(());
                }
                if !ctx.io.confirm(&format!("change {n} {} record(s)?", m.name))? {
                    return Err(anyhow!("nothing changed"));
                }
                let n = store.update_records(&m, &filter, &changes)?;
                note(store, d::act_record_update(&m, None, n, &filter, &changes));
                ctx.io.info(format!("updated {n} {} record(s)", m.name));
            }
        }
        RecordCommand::List { model, conditions, text, sort, group, agg, limit, offset, json } => {
            let m = store.find_model(&model)?;
            let limit = match limit {
                Some(0) => None,
                Some(n) => Some(n),
                None if m.is_bars() => Some(50),
                None => None,
            };
            let q = RecordQuery { filter: d::parse_where(&conditions)?, text: text.unwrap_or_default(), sort: d::parse_sort(&sort), limit, offset };
            let by = list_arg(&group);
            if !by.is_empty() || !agg.is_empty() {
                let gs = store.group_records(&m, &q, &by, &list_arg(&agg))?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&gs)?);
                    return Ok(());
                }
                let aggs: Vec<String> = gs.first().map(|g| g.aggs.keys().cloned().collect()).unwrap_or_default();
                let mut head = by.clone();
                head.push("count".into());
                head.extend(aggs.iter().cloned());
                let rows: Vec<Vec<String>> = gs
                    .iter()
                    .map(|g| {
                        let mut row: Vec<String> = by.iter().map(|b| cell(g.key.get(b))).collect();
                        row.push(g.count.to_string());
                        row.extend(aggs.iter().map(|a| cell(g.aggs.get(a))));
                        row
                    })
                    .collect();
                table(&head, &rows);
                return Ok(());
            }
            let rs = store.records(&m, &q)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&rs)?);
            } else if rs.is_empty() {
                ctx.io.info(format!("no {} records", m.name));
            } else {
                print_records(&m, &rs);
            }
        }
        RecordCommand::Show { model, id, json } => {
            let m = store.find_model(&model)?;
            let r = store.find_record(&m, &id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&r)?);
                return Ok(());
            }
            println!("{} record {} (v{})", m.name, r.id, r.version);
            for n in crate::store::records::field_names(&m) {
                println!("  {n}: {}", r.values.get(&n).map(d::value_text).unwrap_or_default());
            }
            println!("  created {} · updated {}", fmt_ts(r.created_at), fmt_ts(r.updated_at));
        }
        RecordCommand::Delete { model, ids, conditions, dry_run } => {
            let m = store.find_model(&model)?;
            if ids.is_empty() == conditions.is_empty() {
                return Err(anyhow!("give record ids or --where (not both)"));
            }
            if !ids.is_empty() {
                let rs: Vec<Record> = ids.iter().map(|i| store.find_record(&m, i)).collect::<crate::Result<_>>()?;
                if dry_run {
                    ctx.io.info(format!("would delete {} {} record(s)", rs.len(), m.name));
                    return Ok(());
                }
                if !ctx.io.confirm(&format!("delete {} {} record(s)?", rs.len(), m.name))? {
                    return Err(anyhow!("nothing deleted"));
                }
                for r in &rs {
                    store.delete_record(&m, &r.id)?;
                    note(store, d::act_record_delete(&m, Some(r), 1, &[]));
                }
                ctx.io.info(format!("deleted {} {} record(s)", rs.len(), m.name));
            } else {
                let filter = d::parse_where(&conditions)?;
                let n = store.records(&m, &RecordQuery { filter: filter.clone(), ..Default::default() })?.len();
                if dry_run || n == 0 {
                    ctx.io.info(format!("{} {n} {} record(s)", if dry_run { "would delete" } else { "no match among" }, m.name));
                    return Ok(());
                }
                if !ctx.io.confirm(&format!("delete {n} {} record(s)?", m.name))? {
                    return Err(anyhow!("nothing deleted"));
                }
                let n = store.delete_records(&m, &filter)?;
                note(store, d::act_record_delete(&m, None, n, &filter));
                ctx.io.info(format!("deleted {n} {} record(s)", m.name));
            }
        }
    }
    Ok(())
}
