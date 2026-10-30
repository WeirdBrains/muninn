//! `mnnn report`.
use super::searches::record;
use super::{Ctx, ReportCommand, SectionArgs};
use crate::export::docs::section_summary;
use crate::export::files::{OutFormat, Target, write_recorded};
use crate::ingest::time::fmt_ts;
use crate::store::{EntryFormat, ItemKind, NewSection, Report, Section, SectionChange, SectionKind, Store};
use anyhow::anyhow;
use std::path::PathBuf;

fn params(r: &Report) -> serde_json::Value {
    serde_json::json!({ "report": r.id, "name": r.name })
}

fn sparams(r: &Report, s: &Section) -> serde_json::Value {
    serde_json::json!({ "report": r.id, "section": s.id, "kind": s.kind.as_str(), "position": s.position })
}

/// The output file: `--out`, else `<name>.<ext>` here.
pub(super) fn out_path(store: &Store, t: &Target, f: OutFormat, out: Option<PathBuf>) -> PathBuf {
    out.unwrap_or_else(|| PathBuf::from(t.file_name(store, f)))
}

/// Export and say what was written.
pub(super) fn export(ctx: &Ctx, store: &Store, t: &Target, f: OutFormat, out: Option<PathBuf>) -> anyhow::Result<()> {
    let out = out_path(store, t, f, out);
    let files = write_recorded(store, t, f, &out, "cli")?;
    let extra = if files.len() > 1 { format!(" (and {} images beside it)", files.len() - 1) } else { String::new() };
    ctx.io.info(format!("wrote {}{extra}", out.display()));
    Ok(())
}

fn new_section(store: &Store, a: &SectionArgs) -> anyhow::Result<NewSection> {
    let mut s = if let Some(text) = a.body.read()? {
        NewSection::text("", &text, a.format.map(Into::into).unwrap_or(EntryFormat::Markdown))
    } else if let Some(r) = &a.records {
        let mut s = NewSection::new(SectionKind::Records);
        s.params.series = Some(super::projects::parse_result(store, r, a.from.as_deref(), a.to.as_deref())?);
        s.params.rows = a.rows;
        s
    } else if let Some(q) = &a.series {
        let mut s = NewSection::new(SectionKind::Series);
        if !q.is_empty() {
            s.params.search = Some(store.find_live_search(q)?.id);
        }
        s
    } else if let Some(j) = &a.journal {
        let j = store.find_journal(j)?;
        let mut s = NewSection::new(SectionKind::Journal);
        for e in &a.entries {
            let e = store.find_entry(e)?;
            if e.journal_id != j.id {
                return Err(anyhow!("entry {} is not in journal {}", e.heading(), j.name));
            }
            s.params.entries.push(e.id);
        }
        s.params.journal = Some(j.id);
        s
    } else if let Some(p) = &a.project {
        let mut s = NewSection::new(SectionKind::Project);
        s.params.project = Some(store.find_project(p)?.id);
        s
    } else if let Some(m) = &a.model {
        let mut s = NewSection::new(SectionKind::Model);
        s.params.model = Some(store.find_model(m)?.id);
        s.params.conditions = crate::designer::parse_where(&a.conditions)?;
        s.params.rows = a.rows;
        s
    } else if a.authors {
        NewSection::new(SectionKind::Authors)
    } else if let Some(c) = &a.code {
        let mut s = NewSection::new(SectionKind::Code);
        s.params.space = Some(c.clone());
        s
    } else {
        return Err(anyhow!("say what the section shows: --text/--file, --records, --series, --journal, --project, --model, --authors or --code"));
    };
    if a.format.is_some() && s.kind != SectionKind::Text {
        return Err(anyhow!("--format belongs to text sections"));
    }
    s.title = a.title.clone().unwrap_or_default();
    Ok(s)
}

pub fn run(ctx: &Ctx, cmd: ReportCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        ReportCommand::List { json } => {
            let all = store.reports()?;
            if json {
                let v: Vec<_> =
                    all.iter().map(|r| serde_json::json!({ "report": r, "sections": store.sections(&r.id).map(|v| v.len()).unwrap_or(0) })).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for r in &all {
                    println!("{}\t{}\t{} sections\t{}", r.short_id(), r.name, store.sections(&r.id)?.len(), r.description);
                }
                if all.is_empty() {
                    ctx.io.info("no reports yet; create one with `mnnn report create NAME` (or O in the TUI)");
                }
            }
        }
        ReportCommand::Create { name, description, project } => {
            let p = project.map(|p| store.find_project(&p)).transpose()?;
            let r = store.create_report(&name, &description)?;
            record(&store, "report.create", format!("create report {}", r.name), params(&r));
            ctx.io.info(format!("created report {} ({})", r.name, r.short_id()));
            if let Some(p) = p {
                let (it, _) = store.add_project_item(&p.id, ItemKind::Report, &r.id, serde_json::json!({}))?;
                record(
                    &store,
                    "project.add",
                    format!("add report {} to project {}", r.name, p.name),
                    serde_json::json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                );
                ctx.io.info(format!("added it to project {}", p.name));
            }
        }
        ReportCommand::Show { report, json } => {
            let r = store.find_report(&report)?;
            let sections = store.sections(&r.id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "report": r, "sections": sections }))?);
            } else {
                println!("id:          {}", r.id);
                println!("name:        {}", r.name);
                if !r.description.is_empty() {
                    println!("description: {}", r.description);
                }
                println!("author:      {}", crate::history::author_label(&store, r.author_id.as_deref()));
                println!("created:     {}", fmt_ts(r.created_at));
                println!("updated:     {}", fmt_ts(r.updated_at));
                println!();
                for s in &sections {
                    let who = crate::history::author_label(&store, s.author_id.as_deref());
                    println!("{:>3}  {}  {}  · {}  · {who}", s.position, s.short_id(), s.heading(), section_summary(&store, s));
                }
                if sections.is_empty() {
                    ctx.io.info("no sections; add some with `mnnn report add` (see `mnnn help reports`)");
                }
            }
        }
        ReportCommand::Edit { report, name, description } => {
            let r = store.find_report(&report)?;
            let r = store.update_report(&r.id, name.as_deref(), description.as_deref())?;
            record(&store, "report.edit", format!("edit report {}", r.name), params(&r));
            ctx.io.info(format!("updated report {}", r.name));
        }
        ReportCommand::Remove { report } => {
            let r = store.find_report(&report)?;
            if !ctx.io.confirm(&format!("remove report `{}`?", r.name))? {
                return Err(crate::Error::Aborted.into());
            }
            store.remove_report(&r.id)?;
            record(&store, "report.remove", format!("remove report {}", r.name), params(&r));
            ctx.io.info(format!("removed report {}", r.name));
        }
        ReportCommand::Add(a) => {
            let r = store.find_report(&a.report)?;
            let ns = new_section(&store, &a)?;
            let mut s = store.add_section(&r.id, &ns)?;
            if let Some(at) = a.at {
                s = store.move_section(&s.id, at)?;
            }
            record(&store, "report.section.add", format!("add {} section {} to report {}", s.kind.as_str(), s.heading(), r.name), sparams(&r, &s));
            ctx.io.info(format!("added section {} ({}) to {}", s.position, s.heading(), r.name));
        }
        ReportCommand::EditSection { report, section, title, body, format, rows } => {
            let r = store.find_report(&report)?;
            let s = store.find_section(&r.id, &section)?;
            let text = body.read()?;
            if (text.is_some() || format.is_some()) && s.kind != SectionKind::Text {
                return Err(anyhow!("section {} shows {}; only text sections have text", s.position, s.kind.as_str()));
            }
            if rows.is_some() && !matches!(s.kind, SectionKind::Records | SectionKind::Model) {
                return Err(anyhow!("--rows belongs to records and model sections"));
            }
            let params = rows.map(|n| {
                let mut p = s.params.clone();
                p.rows = Some(n);
                p
            });
            let s = store.update_section(&s.id, &SectionChange { title, body: text, format: format.map(Into::into), params })?;
            record(&store, "report.section.edit", format!("edit section {} of report {}", s.heading(), r.name), sparams(&r, &s));
            ctx.io.info(format!("updated section {} of {}", s.position, r.name));
        }
        ReportCommand::RemoveSection { report, section } => {
            let r = store.find_report(&report)?;
            let s = store.find_section(&r.id, &section)?;
            store.remove_section(&s.id)?;
            record(&store, "report.section.remove", format!("remove section {} from report {}", s.heading(), r.name), sparams(&r, &s));
            ctx.io.info(format!("removed section {} ({}) from {}", s.position, s.heading(), r.name));
        }
        ReportCommand::Move { report, section, to } => {
            let r = store.find_report(&report)?;
            let s = store.find_section(&r.id, &section)?;
            let from = s.position;
            let s = store.move_section(&s.id, to)?;
            let mut p = sparams(&r, &s);
            p["from"] = from.into();
            record(&store, "report.section.move", format!("move section {} of report {} from {from} to {}", s.heading(), r.name, s.position), p);
            ctx.io.info(format!("moved {} to position {}", s.heading(), s.position));
        }
        ReportCommand::Export { report, format, out } => {
            let r = store.find_report(&report)?;
            export(ctx, &store, &Target::Report(r.id), format.into(), out)?;
        }
        ReportCommand::Preview { report, width } => {
            let r = store.find_report(&report)?;
            print!("{}", crate::render::text::write(&crate::export::docs::report(&store, &r.id)?, width.max(40)));
        }
    }
    Ok(())
}
