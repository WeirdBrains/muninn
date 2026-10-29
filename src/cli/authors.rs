//! `mnnn author`.
use super::{AuthorCommand, Ctx};
use crate::store::authors::parse_details;
use crate::store::{Author, AuthorChange, NewAction, Store};

fn print(a: &Author, current: bool) {
    println!("id:      {}", a.id);
    println!("name:    {}{}", a.name, if current { "  (current)" } else { "" });
    println!("email:   {}", a.email.as_deref().unwrap_or("-"));
    for (k, v) in &a.details {
        println!("{k}: {v}");
    }
    println!("created: {}", crate::ingest::time::fmt_ts(a.created_at));
    println!("updated: {}", crate::ingest::time::fmt_ts(a.updated_at));
}

fn record(store: &Store, kind: &str, summary: String, a: &Author) {
    store.note_action(&NewAction::new(kind, "cli", summary).params(serde_json::json!({ "author": a.id, "name": a.name })));
}

pub fn run(ctx: &Ctx, cmd: AuthorCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let current = store.current_author()?.map(|a| a.id);
    let is_cur = |a: &Author| current.as_deref() == Some(a.id.as_str());
    match cmd {
        AuthorCommand::List { json } => {
            let all = store.authors()?;
            if json {
                let v: Vec<_> = all.iter().map(|a| serde_json::json!({ "current": is_cur(a), "author": a })).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for a in &all {
                    println!("{}\t{}\t{}\t{}", if is_cur(a) { "*" } else { " " }, a.short_id(), a.name, a.email.as_deref().unwrap_or(""));
                }
                if all.is_empty() {
                    ctx.io.info("no authors yet; add one with `mnnn author add --name NAME`");
                }
            }
        }
        AuthorCommand::Show { author, json } => {
            let a = match author {
                Some(k) => store.find_author(&k)?,
                None => store.current_author()?.ok_or_else(|| anyhow::anyhow!("no current author; pass one or run `mnnn author use`"))?,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "current": is_cur(&a), "author": a }))?);
            } else {
                print(&a, is_cur(&a));
            }
        }
        AuthorCommand::Add { name, email, details, current } => {
            let a = store.add_author(&name, email.as_deref(), &parse_details(&details)?)?;
            if current {
                store.set_current_author(Some(&a.id))?;
            }
            record(&store, "author.add", format!("add author {}", a.name), &a);
            let cur = store.current_author()?.is_some_and(|c| c.id == a.id);
            ctx.io.info(format!("added author {} ({}){}", a.display(), a.short_id(), if cur { ", now current" } else { "" }));
        }
        AuthorCommand::Edit { author, name, email, details, replace_details } => {
            let a = store.find_author(&author)?;
            let c = AuthorChange { name, email, details: parse_details(&details)?, replace_details };
            let a = store.update_author(&a.id, &c)?;
            record(&store, "author.edit", format!("edit author {}", a.name), &a);
            ctx.io.info(format!("updated author {} ({})", a.display(), a.short_id()));
        }
        AuthorCommand::Remove { author } => {
            let a = store.find_author(&author)?;
            if !ctx.io.confirm(&format!("remove author `{}`? Past actions keep its id.", a.display()))? {
                return Err(crate::Error::Aborted.into());
            }
            // Record first so the action is still attributed while the author exists.
            record(&store, "author.remove", format!("remove author {}", a.name), &a);
            store.remove_author(&a.id)?;
            ctx.io.info(format!("removed author {}", a.display()));
        }
        AuthorCommand::Use { author, none } => {
            if none {
                store.set_current_author(None)?;
                ctx.io.info("no current author");
            } else if let Some(k) = author {
                let a = store.find_author(&k)?;
                store.set_current_author(Some(&a.id))?;
                record(&store, "author.use", format!("current author {}", a.name), &a);
                ctx.io.info(format!("current author: {}", a.display()));
            }
        }
    }
    Ok(())
}
