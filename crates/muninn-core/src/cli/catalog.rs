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
                let (n, b) = store.source_data_totals(&s.id)?;
                println!("{}\t{}\t{}\t{}\t{n} item(s), {b} bytes", s.name, s.kind, s.locator.unwrap_or_default(), s.id);
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
        SourcesCommand::Data { name, json } => {
            let src = store.source_by_name(&name)?.ok_or_else(|| anyhow::anyhow!("no source `{name}`"))?;
            let items = store.source_data(&src.id)?;
            if json {
                let v: Vec<_> = items
                    .iter()
                    .map(|i| json!({ "id": i.id, "name": i.name, "format": i.format, "locator": i.locator, "size": i.size, "content_hash": i.content_hash, "received_at": i.received_at }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for i in items {
                    println!(
                        "{}\t{}\t{}\t{} bytes\t{}\t{}",
                        &i.id[..8],
                        crate::ingest::time::fmt_ts(i.received_at),
                        i.format,
                        i.size,
                        i.name,
                        i.locator
                    );
                }
            }
        }
        SourcesCommand::Cat { name, item } => {
            use std::io::Write;
            let src = store.source_by_name(&name)?.ok_or_else(|| anyhow::anyhow!("no source `{name}`"))?;
            let found: Vec<_> = store.source_data(&src.id)?.into_iter().filter(|i| i.id.starts_with(item.trim())).collect();
            let i = match found.as_slice() {
                [one] => one,
                [] => anyhow::bail!("no item `{item}` in {name} (`mnnn sources data {name}`)"),
                _ => anyhow::bail!("`{item}` matches {} items; give more of the id", found.len()),
            };
            std::io::stdout().write_all(&store.source_data_bytes(&i.id)?)?;
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
