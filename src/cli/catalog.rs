//! `mnnn sources` and `mnnn symbols`.
use super::{Ctx, SourcesCommand, SymbolsCommand};
use crate::model::Symbol;

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
            ctx.io.info(format!("added source {} ({})", s.name, s.id));
        }
        SourcesCommand::Rename { from, to } => {
            let n = store.rename_source(&from, &to)?;
            ctx.io.info(format!("renamed {from} -> {to}; recomputed {n} bar ids"));
        }
        SourcesCommand::Remove { name } => {
            if !ctx.io.confirm(&format!("remove source `{name}` and all of its bars?"))? {
                return Err(crate::Error::Aborted.into());
            }
            let n = store.remove_source(&name)?;
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
            ctx.io.info(format!("added {sym}"));
        }
    }
    Ok(())
}
