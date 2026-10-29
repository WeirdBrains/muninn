//! `mnnn journal`.
use super::searches::record;
use super::{Ctx, JournalCommand, SelectArgs, TextArgs};
use crate::export::files::Target;
use crate::ingest::time::fmt_ts;
use crate::store::journals::{AnnotationDoc, select};
use crate::store::{Annotation, AnnotationKind, Entry, Journal, Span};
use anyhow::{Context, anyhow};
use std::io::Read;

impl TextArgs {
    /// The text given, if any (`--file -` reads stdin).
    pub fn read(&self) -> anyhow::Result<Option<String>> {
        if let Some(t) = &self.text {
            return Ok(Some(t.replace("\\n", "\n")));
        }
        match &self.file {
            Some(p) if p.as_os_str() == "-" => {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                Ok(Some(s))
            }
            Some(p) => Ok(Some(std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?)),
            None => Ok(None),
        }
    }
}

impl SelectArgs {
    /// The selected words of an entry, if any were given.
    pub fn span(&self, body: &str) -> anyhow::Result<Option<Span>> {
        if let Some(w) = &self.on {
            return Ok(Some(select(body, w, self.nth)?));
        }
        let Some(r) = &self.range else { return Ok(None) };
        let (a, b) = r.split_once("..").ok_or_else(|| anyhow!("invalid --range: `{r}` (use START..END)"))?;
        let n = |v: &str| v.trim().parse::<usize>().map_err(|_| anyhow!("invalid --range: `{r}`"));
        Ok(Some(Span { start: n(a)?, end: n(b)? }))
    }
}

/// Entries named on the command line, checked to be in the journal.
fn entry_ids(store: &crate::store::Store, j: &Journal, keys: &[String]) -> anyhow::Result<Vec<String>> {
    let mut v = vec![];
    for k in keys {
        let e = store.find_entry(k)?;
        if e.journal_id != j.id {
            return Err(anyhow!("entry {} is not in journal {}", e.heading(), j.name));
        }
        v.push(e.id);
    }
    Ok(v)
}

fn jparams(j: &Journal) -> serde_json::Value {
    serde_json::json!({ "journal": j.id, "name": j.name })
}

fn eparams(e: &Entry) -> serde_json::Value {
    serde_json::json!({ "journal": e.journal_id, "entry": e.id, "title": e.heading() })
}

fn ann_line(d: &AnnotationDoc) -> String {
    let a = &d.annotation;
    let who = d.author.as_deref().unwrap_or("-");
    let what = match (a.kind, &a.quote) {
        (AnnotationKind::Highlight, Some(q)) => format!("highlight \"{q}\"{}", a.color.as_deref().map(|c| format!(" ({c})")).unwrap_or_default()),
        (AnnotationKind::Note, Some(q)) => format!("note on \"{q}\""),
        (AnnotationKind::Note, None) => "note".into(),
        (AnnotationKind::Highlight, None) => "highlight".into(),
    };
    let detached = if a.detached() { " [words no longer in the entry]" } else { "" };
    let body = if a.body.is_empty() { String::new() } else { format!(": {}", a.body) };
    format!("  {} {what}{body}{detached} · {who}", a.short_id())
}

fn summary(a: &Annotation) -> String {
    let what = if a.kind == AnnotationKind::Highlight { "highlight" } else { "note" };
    match &a.quote {
        Some(q) => format!("{what} \"{}\"", q.chars().take(40).collect::<String>()),
        None if a.entry_id.is_some() => format!("{what} on an entry"),
        None => format!("{what} on the journal"),
    }
}

fn print_entry(d: &crate::store::journals::EntryDoc) {
    let e = &d.entry;
    println!("── {} · {} · {} · {} · {}", e.heading(), e.short_id(), d.author.as_deref().unwrap_or("-"), fmt_ts(e.created_at), e.format.as_str());
    if let Some(f) = &e.forked_from {
        println!("   (copied from entry {})", crate::store::projects::short_of(f));
    }
    println!("{}", e.body.trim_end());
    for a in &d.attachments {
        println!("  image {} {} ({}, {} bytes)", a.short_id(), a.name, a.mime, a.size);
    }
    for a in &d.annotations {
        println!("{}", ann_line(a));
    }
}

pub fn run(ctx: &Ctx, cmd: JournalCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        JournalCommand::List { project, json } => {
            let all = match project {
                Some(p) => store.project_journals(&store.find_project(&p)?.id)?,
                None => store.journals()?,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&all)?);
            } else {
                for j in &all {
                    let proj = match &j.project_id {
                        Some(p) => store.project(p)?.map(|p| p.name).unwrap_or_default(),
                        None => "-".into(),
                    };
                    println!(
                        "{}\t{}\t{}\tproject: {proj}\t{}",
                        j.short_id(),
                        j.name,
                        crate::live::entries(store.entries(&j.id)?.len()),
                        j.format.as_str()
                    );
                }
                if all.is_empty() {
                    ctx.io.info("no journals yet; create one with `mnnn journal new NAME` (or J in the TUI)");
                }
            }
        }
        JournalCommand::New { name, project, format } => {
            let pid = project.map(|p| store.find_project(&p)).transpose()?.map(|p| p.id);
            let j = store.create_journal(&name, pid.as_deref(), format.into())?;
            record(&store, "journal.create", format!("create journal {}", j.name), jparams(&j));
            ctx.io.info(format!("created journal {} ({})", j.name, j.short_id()));
        }
        JournalCommand::Show { journal, json } => {
            let j = store.find_journal(&journal)?;
            let doc = store.journal_doc(&j.id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&doc)?);
            } else {
                println!("journal: {} ({})", j.name, j.id);
                println!("project: {}", doc.project.as_deref().unwrap_or("-"));
                println!("author:  {}", doc.author.as_deref().unwrap_or("-"));
                println!("format:  {}", j.format.as_str());
                if let Some(f) = &j.forked_from {
                    let name = store.journal(f)?.map(|o| o.name).unwrap_or_else(|| "(removed)".into());
                    println!("fork of: {name}");
                }
                if !doc.notes.is_empty() {
                    println!("notes:");
                    for n in &doc.notes {
                        println!("{}", ann_line(n));
                    }
                }
                for e in &doc.entries {
                    println!();
                    print_entry(e);
                }
                if doc.entries.is_empty() {
                    ctx.io.info("no entries yet; write one with `mnnn journal write JOURNAL --text ...`");
                }
            }
        }
        JournalCommand::Edit { journal, name, format } => {
            let j = store.find_journal(&journal)?;
            let j = store.update_journal(&j.id, name.as_deref(), format.map(Into::into))?;
            record(&store, "journal.edit", format!("edit journal {}", j.name), jparams(&j));
            ctx.io.info(format!("updated journal {}", j.name));
        }
        JournalCommand::Remove { journal } => {
            let j = store.find_journal(&journal)?;
            let n = store.entries(&j.id)?.len();
            if !ctx.io.confirm(&format!("remove journal `{}` with its {n} entries, notes and images?", j.name))? {
                return Err(crate::Error::Aborted.into());
            }
            store.remove_journal(&j.id)?;
            record(&store, "journal.remove", format!("remove journal {}", j.name), jparams(&j));
            ctx.io.info(format!("removed journal {}", j.name));
        }
        JournalCommand::Fork { journal, project, name } => {
            let o = store.find_journal(&journal)?;
            let p = project.map(|p| store.find_project(&p)).transpose()?;
            let j = store.fork_journal(&o.id, p.as_ref().map(|p| p.id.as_str()), name.as_deref())?;
            let into = p.as_ref().map(|p| format!(" into project {}", p.name)).unwrap_or_default();
            record(&store, "journal.fork", format!("fork journal {} → {}{into}", o.name, j.name), jparams(&j));
            ctx.io.info(format!("forked {} → {} ({}){into}", o.name, j.name, j.short_id()));
        }
        JournalCommand::Write { journal, title, format, text } => {
            let j = store.find_journal(&journal)?;
            let body = text.read()?.unwrap_or_default();
            let e = store.add_entry(&j.id, &title, &body, format.map(Into::into))?;
            record(&store, "entry.add", format!("write entry {} in {}", e.heading(), j.name), eparams(&e));
            ctx.io.info(format!("wrote entry {} ({}) in {}", e.heading(), e.short_id(), j.name));
        }
        JournalCommand::Entry { entry, json } => {
            let e = store.find_entry(&entry)?;
            let doc = store.journal_doc(&e.journal_id)?;
            let d = doc.entries.into_iter().find(|d| d.entry.id == e.id).ok_or_else(|| anyhow!("entry vanished"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&d)?);
            } else {
                println!("journal: {}", doc.journal.name);
                print_entry(&d);
            }
        }
        JournalCommand::EditEntry { entry, title, format, text } => {
            let e = store.find_entry(&entry)?;
            let body = text.read()?;
            let e = store.update_entry(&e.id, title.as_deref(), body.as_deref(), format.map(Into::into))?;
            record(&store, "entry.edit", format!("edit entry {}", e.heading()), eparams(&e));
            let detached = store.entry_annotations(&e.id)?.iter().filter(|a| a.detached()).count();
            ctx.io.info(format!("updated entry {}", e.heading()));
            if detached > 0 {
                ctx.io.warn(format!("{detached} note(s)/highlight(s) lost their words and are kept detached"));
            }
        }
        JournalCommand::RemoveEntry { entry } => {
            let e = store.find_entry(&entry)?;
            if !ctx.io.confirm(&format!("remove entry `{}` with its notes and images?", e.heading()))? {
                return Err(crate::Error::Aborted.into());
            }
            store.remove_entry(&e.id)?;
            record(&store, "entry.remove", format!("remove entry {}", e.heading()), eparams(&e));
            ctx.io.info(format!("removed entry {}", e.heading()));
        }
        JournalCommand::Copy { entries, to, new, project } => {
            let ids: Vec<String> = entries.iter().map(|k| store.find_entry(k).map(|e| e.id)).collect::<crate::Result<_>>()?;
            let target = match (to, new) {
                (Some(t), _) => store.find_journal(&t)?,
                (None, Some(name)) => {
                    let pid = project.map(|p| store.find_project(&p)).transpose()?.map(|p| p.id);
                    let first = store.entry(&ids[0])?.and_then(|e| store.journal(&e.journal_id).ok().flatten());
                    let j = store.create_journal(&name, pid.as_deref(), first.map(|j| j.format).unwrap_or_default())?;
                    record(&store, "journal.create", format!("create journal {}", j.name), jparams(&j));
                    j
                }
                (None, None) => unreachable!("clap requires --to or --new"),
            };
            let copied = store.copy_entries(&ids, &target.id)?;
            record(
                &store,
                "entry.copy",
                format!("add {} to journal {}", crate::live::entries(copied.len()), target.name),
                serde_json::json!({ "journal": target.id, "entries": copied.iter().map(|e| &e.id).collect::<Vec<_>>(), "from": ids }),
            );
            ctx.io.info(format!("added {} to {} ({})", crate::live::entries(copied.len()), target.name, target.short_id()));
        }
        JournalCommand::Note { body, journal, entry, select } => {
            let (jid, e) = match (journal, entry) {
                (Some(j), _) => (store.find_journal(&j)?.id, None),
                (None, Some(e)) => {
                    let e = store.find_entry(&e)?;
                    (e.journal_id.clone(), Some(e))
                }
                (None, None) => unreachable!("clap requires --journal or --entry"),
            };
            let span = match &e {
                Some(e) => select.span(&e.body)?,
                None if select.on.is_some() || select.range.is_some() => return Err(anyhow!("words can only be selected with --entry")),
                None => None,
            };
            let a = store.add_note(&jid, e.as_ref().map(|e| e.id.as_str()), span, &body)?;
            record(
                &store,
                "note.add",
                format!("add {}", summary(&a)),
                serde_json::json!({ "journal": a.journal_id, "entry": a.entry_id, "note": a.id }),
            );
            ctx.io.info(format!("added {} ({})", summary(&a), a.short_id()));
        }
        JournalCommand::Highlight { entry, select, color, comment } => {
            let e = store.find_entry(&entry)?;
            let span = select.span(&e.body)?.ok_or_else(|| anyhow!("select words with --on or --range"))?;
            let a = store.add_highlight(&e.id, span, color.as_deref(), &comment)?;
            record(
                &store,
                "highlight.add",
                format!("add {}", summary(&a)),
                serde_json::json!({ "journal": a.journal_id, "entry": a.entry_id, "highlight": a.id }),
            );
            ctx.io.info(format!("added {} ({})", summary(&a), a.short_id()));
        }
        JournalCommand::EditNote { id, body } => {
            let a = store.find_annotation(&id)?;
            let a = store.update_annotation(&a.id, &body)?;
            record(&store, "note.edit", format!("edit {}", summary(&a)), serde_json::json!({ "journal": a.journal_id, "note": a.id }));
            ctx.io.info(format!("updated {}", summary(&a)));
        }
        JournalCommand::RemoveNote { id } => {
            let a = store.find_annotation(&id)?;
            store.remove_annotation(&a.id)?;
            let kind = if a.kind == AnnotationKind::Highlight { "highlight.remove" } else { "note.remove" };
            record(&store, kind, format!("remove {}", summary(&a)), serde_json::json!({ "journal": a.journal_id, "id": a.id }));
            ctx.io.info(format!("removed {}", summary(&a)));
        }
        JournalCommand::Attach { entry, file, name } => {
            let e = store.find_entry(&entry)?;
            let data = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
            let name = name.unwrap_or_else(|| file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into()));
            let a = store.attach_image(&e.id, &name, &data)?;
            record(&store, "image.add", format!("attach image {} to {}", a.name, e.heading()), serde_json::json!({ "entry": e.id, "image": a.id }));
            ctx.io.info(format!("attached {} ({}, {} bytes); the entry now ends with {}", a.name, a.short_id(), a.size, a.reference(e.format)));
        }
        JournalCommand::Image { id, out } => {
            let all: Vec<String> = {
                let mut v = vec![];
                for j in store.journals()? {
                    for e in store.entries(&j.id)? {
                        v.extend(store.attachments(&e.id)?.into_iter().map(|a| a.id).filter(|a| a.starts_with(id.trim())));
                    }
                }
                v
            };
            let [one] = all.as_slice() else { return Err(anyhow!("image `{id}` {}", if all.is_empty() { "not found" } else { "is ambiguous" })) };
            let (a, data) = store.attachment_data(one)?.ok_or_else(|| anyhow!("image `{id}` not found"))?;
            std::fs::write(&out, &data).with_context(|| format!("writing {}", out.display()))?;
            ctx.io.info(format!("wrote {} ({} bytes) to {}", a.name, data.len(), out.display()));
        }
        JournalCommand::Export { journal, entries, format, out } => {
            let j = store.find_journal(&journal)?;
            let entries = entry_ids(&store, &j, &entries)?;
            super::reports::export(ctx, &store, &Target::Journal { id: j.id, entries }, format.into(), out)?;
        }
        JournalCommand::Import { file, project, name } => {
            let p = project.map(|p| store.find_project(&p)).transpose()?;
            let r = crate::export::archive::import_journal(&store, &file, p.as_ref().map(|p| p.id.as_str()), name.as_deref())?;
            let j = r.journals[0].clone();
            record(
                &store,
                "journal.import",
                format!("import journal {} from {}", j.name, file.display()),
                serde_json::json!({ "journal": j.id, "name": j.name, "file": file.display().to_string(), "entries": r.entries }),
            );
            let into = p.map(|p| format!(" in project {}", p.name)).unwrap_or_default();
            ctx.io.info(format!("imported journal {} ({}){into}: {} entries, {} images", j.name, j.short_id(), r.entries, r.attachments));
        }
        JournalCommand::Print { journal, entries, printer, out, save_only } => {
            let j = store.find_journal(&journal)?;
            let entries = entry_ids(&store, &j, &entries)?;
            let p = crate::export::files::print_journal(
                &store,
                &j.id,
                &entries,
                printer.as_deref(),
                out.as_deref(),
                &ctx.paths.prints_dir(),
                !save_only,
                "cli",
            )?;
            ctx.io.info(p.message());
        }
    }
    Ok(())
}
