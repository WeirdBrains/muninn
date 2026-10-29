//! `mnnn project`.
use super::searches::record;
use super::{Ctx, ProjectCommand};
use crate::export::files::Target;
use crate::ingest::time::{fmt_ts, parse_ts};
use crate::model::{Duration, Symbol};
use crate::store::{ItemKind, Project, ProjectItem, SeriesRef, Store};
use anyhow::anyhow;

/// `binance:BTC/USDT:5m` → a series reference.
pub fn parse_result(store: &Store, s: &str, from: Option<&str>, to: Option<&str>) -> anyhow::Result<SeriesRef> {
    let parts: Vec<&str> = s.split(':').collect();
    let [src, sym, per] = parts.as_slice() else {
        return Err(anyhow!("invalid --result: `{s}` (use SOURCE:SYMBOL:PERIOD, e.g. binance:BTC/USDT:5m)"));
    };
    let ts = |v: &str, what: &str| parse_ts(v).ok_or_else(|| anyhow!("invalid {what}: `{v}` is not a date or time"));
    let r = SeriesRef {
        source: src.trim().to_string(),
        symbol: store.canonical_symbol(&Symbol::parse(sym)?)?,
        duration: Duration::parse(per)?,
        from: from.map(|v| ts(v, "--from")).transpose()?,
        to: to.map(|v| ts(v, "--to")).transpose()?,
    };
    if !store.series()?.iter().any(|x| x.source == r.source && x.symbol == r.symbol && x.duration == r.duration) {
        return Err(anyhow!("no series {} (see the sidebar or `mnnn search find`)", r.label()));
    }
    Ok(r)
}

fn params(p: &Project) -> serde_json::Value {
    serde_json::json!({ "project": p.id, "name": p.name })
}

pub fn run(ctx: &Ctx, cmd: ProjectCommand) -> anyhow::Result<()> {
    let mut store = ctx.open_store()?;
    match cmd {
        ProjectCommand::Export { project, format, out, rows, no_data } => {
            let p = store.find_project(&project)?;
            super::reports::export(ctx, &store, &Target::Project { id: p.id, rows, data: !no_data }, format.into(), out)?;
        }
        ProjectCommand::Import { file, name } => {
            let r = crate::export::archive::import_project(&mut store, &file, name.as_deref())?;
            let p = r.project.clone().expect("imported project");
            record(
                &store,
                "project.import",
                format!("import project {} from {}", p.name, file.display()),
                serde_json::json!({ "project": p.id, "name": p.name, "file": file.display().to_string(), "restored": r }),
            );
            ctx.io.info(format!("imported project {} ({}): {}", p.name, p.short_id(), r.counts(true)));
        }
        ProjectCommand::List { json } => {
            let all = store.projects()?;
            if json {
                let v: Vec<_> = all
                    .iter()
                    .map(|p| serde_json::json!({ "project": p, "items": store.project_items(&p.id).map(|v| v.len()).unwrap_or(0) }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for p in &all {
                    println!("{}\t{}\t{} items\t{}", p.short_id(), p.name, store.project_items(&p.id)?.len(), p.description);
                }
                if all.is_empty() {
                    ctx.io.info("no projects yet; create one with `mnnn project create NAME` (or P in the TUI)");
                }
            }
        }
        ProjectCommand::Create { name, description } => {
            let p = store.create_project(&name, &description)?;
            record(&store, "project.create", format!("create project {}", p.name), params(&p));
            ctx.io.info(format!("created project {} ({})", p.name, p.short_id()));
        }
        ProjectCommand::Show { project, json } => {
            let p = store.find_project(&project)?;
            let items = store.project_items(&p.id)?;
            if json {
                let v: Vec<_> = items
                    .iter()
                    .map(|i| {
                        let (name, now) = crate::live::describe_item(&store, i);
                        serde_json::json!({ "item": i, "name": name, "now": now })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "project": p, "items": v }))?);
            } else {
                println!("id:          {}", p.id);
                println!("name:        {}", p.name);
                if !p.description.is_empty() {
                    println!("description: {}", p.description);
                }
                println!("author:      {}", crate::history::author_label(&store, p.author_id.as_deref()));
                println!("created:     {}", fmt_ts(p.created_at));
                for k in [ItemKind::LiveSearch, ItemKind::Result, ItemKind::Journal, ItemKind::Report] {
                    let v: Vec<&ProjectItem> = items.iter().filter(|i| i.kind == k).collect();
                    if v.is_empty() {
                        continue;
                    }
                    println!("\n{}:", k.plural());
                    for i in v {
                        let (name, now) = crate::live::describe_item(&store, i);
                        println!("  #{}\t{name}\t{now}", i.id);
                    }
                }
                if items.is_empty() {
                    ctx.io.info("empty; add things with `mnnn project add` (or `a` in the TUI)");
                }
            }
        }
        ProjectCommand::Edit { project, name, description } => {
            let p = store.find_project(&project)?;
            let p = store.update_project(&p.id, name.as_deref(), description.as_deref())?;
            record(&store, "project.edit", format!("edit project {}", p.name), params(&p));
            ctx.io.info(format!("updated project {}", p.name));
        }
        ProjectCommand::Remove { project } => {
            let p = store.find_project(&project)?;
            if !ctx.io.confirm(&format!("remove project `{}`? Its journals become standalone journals.", p.name))? {
                return Err(crate::Error::Aborted.into());
            }
            store.remove_project(&p.id)?;
            record(&store, "project.remove", format!("remove project {}", p.name), params(&p));
            ctx.io.info(format!("removed project {}", p.name));
        }
        ProjectCommand::Add { project, search, journal, report, result, from, to } => {
            if result.is_none() && (from.is_some() || to.is_some()) {
                return Err(anyhow!("--from and --to belong to --result"));
            }
            let p = store.find_project(&project)?;
            let (it, new) = if let Some(s) = search {
                let s = store.find_live_search(&s)?;
                store.add_project_item(&p.id, ItemKind::LiveSearch, &s.id, serde_json::json!({}))?
            } else if let Some(j) = journal {
                let j = store.find_journal(&j)?;
                store.add_project_item(&p.id, ItemKind::Journal, &j.id, serde_json::json!({}))?
            } else if let Some(r) = report {
                let r = store.find_report(&r)?;
                store.add_project_item(&p.id, ItemKind::Report, &r.id, serde_json::json!({}))?
            } else {
                let r = parse_result(&store, result.as_deref().unwrap_or(""), from.as_deref(), to.as_deref())?;
                store.add_project_result(&p.id, &r)?
            };
            let (name, _) = crate::live::describe_item(&store, &it);
            if new {
                record(
                    &store,
                    "project.add",
                    format!("add {} {name} to project {}", it.kind.label(), p.name),
                    serde_json::json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                );
                ctx.io.info(format!("added {} {name} to {} (item #{})", it.kind.label(), p.name, it.id));
            } else {
                ctx.io.info(format!("{name} is already in {} (item #{})", p.name, it.id));
            }
        }
        ProjectCommand::Drop { project, item } => {
            let p = store.find_project(&project)?;
            let it =
                store.project_items(&p.id)?.into_iter().find(|i| i.id == item).ok_or_else(|| anyhow!("project {} has no item #{item}", p.name))?;
            let (name, _) = crate::live::describe_item(&store, &it);
            store.remove_project_item(it.id)?;
            record(
                &store,
                "project.drop",
                format!("drop {} {name} from project {}", it.kind.label(), p.name),
                serde_json::json!({ "project": p.id, "kind": it.kind, "ref": it.ref_id }),
            );
            ctx.io.info(format!("dropped {name} from {}", p.name));
        }
    }
    Ok(())
}
