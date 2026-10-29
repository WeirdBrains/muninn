//! `mnnn sources` and `mnnn symbols`.
use super::{Ctx, SourcesCommand, SymbolsCommand};
use crate::model::Symbol;
use crate::store::NewAction;
use serde_json::json;

pub fn sources(ctx: &Ctx, cmd: SourcesCommand) -> anyhow::Result<()> {
    let mut store = ctx.open_store()?;
    match cmd {
        SourcesCommand::List => {
            for s in store.sources()? {
                println!("{}\t{}\t{}\t{}", s.name, s.kind, s.locator.unwrap_or_default(), s.id);
            }
        }
        SourcesCommand::Add { name, kind, locator } => {
            let s = store.add_source(&name, &kind, locator.as_deref())?;
            store.note_action(
                &NewAction::new("source.add", "cli", format!("add source {}", s.name)).params(json!({ "source": s.name, "kind": s.kind })),
            );
            ctx.io.info(format!("added source {} ({})", s.name, s.id));
        }
        SourcesCommand::Rename { from, to } => {
            let n = store.rename_source(&from, &to)?;
            store.note_action(
                &NewAction::new("source.rename", "cli", format!("rename source {from} to {to}")).params(json!({ "from": from, "to": to, "bars": n })),
            );
            ctx.io.info(format!("renamed {from} -> {to}; recomputed {n} bar ids"));
        }
        SourcesCommand::Remove { name } => {
            if !ctx.io.confirm(&format!("remove source `{name}` and all of its bars?"))? {
                return Err(crate::Error::Aborted.into());
            }
            let n = store.remove_source(&name)?;
            store.note_action(
                &NewAction::new("source.remove", "cli", format!("remove source {name} ({n} bars)")).params(json!({ "source": name, "bars": n })),
            );
            ctx.io.info(format!("removed {name} ({n} bars)"));
        }
    }
    Ok(())
}

pub fn symbols(ctx: &Ctx, cmd: SymbolsCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        SymbolsCommand::List => {
            for s in store.symbols()? {
                println!("{s}");
            }
            if !ctx.io.quiet {
                for (a, c) in store.aliases()? {
                    println!("alias {a} -> {c}");
                }
            }
        }
        SymbolsCommand::Add { symbol, aliases } => {
            let sym = Symbol::parse(&symbol)?;
            store.add_symbol(&sym, &aliases)?;
            store.note_action(
                &NewAction::new("symbol.add", "cli", format!("add symbol {sym}")).params(json!({ "symbol": sym.to_string(), "aliases": aliases })),
            );
            ctx.io.info(format!("added {sym}"));
        }
    }
    Ok(())
}
