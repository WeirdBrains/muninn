//! `mnnn sync` (2.11): linked-node sync. Thin over [`crate::net::sync`].
use super::net::Net;
use super::{Ctx, SyncCommand};
use crate::ingest::time::fmt_local;
use crate::model::Symbol;
use crate::net::sync;
use crate::store::sync::Conflict;
use anyhow::anyhow;

const ORIGIN: &str = "cli";

fn short(p: &str) -> &str {
    crate::store::net::short_peer(p)
}

fn name(n: &crate::store::users::UserNode) -> String {
    if n.label.is_empty() { short(&n.peer_id).to_string() } else { format!("{} ({})", n.label, short(&n.peer_id)) }
}

fn pick(n: &Net, key: Option<&str>) -> anyhow::Result<Vec<(crate::store::users::UserNode, crate::store::Exchange)>> {
    let all = sync::linked(&n.store)?;
    if all.is_empty() {
        return Err(anyhow!("no linked nodes: link one with `mnnn user link` (see `mnnn help users`)"));
    }
    let chosen: Vec<_> = all
        .into_iter()
        .filter(|(u, _)| key.is_none_or(|k| u.peer_id == k || u.label.eq_ignore_ascii_case(k) || (k.len() >= 6 && u.peer_id.ends_with(k))))
        .collect();
    if chosen.is_empty() {
        return Err(anyhow!("`{}` is not one of your linked nodes (`mnnn user nodes`)", key.unwrap_or_default()));
    }
    let mut out = vec![];
    for (u, x) in chosen {
        match x {
            Some(x) => out.push((u, x)),
            None => eprintln!("{}: no key exchange to reach it by yet", name(&u)),
        }
    }
    Ok(out)
}

fn when(t: Option<i64>) -> String {
    t.map(fmt_local).unwrap_or_else(|| "never".into())
}

fn print_conflict(c: &Conflict, full: bool) {
    let other = if c.kept == "mine" { "theirs" } else { "mine" };
    println!(
        "{}  {} {}  kept {}  {}  {}{}",
        &c.id[..8.min(c.id.len())],
        c.tbl,
        c.pk,
        c.kept,
        c.reason,
        fmt_local(c.created_at),
        c.resolution.as_ref().map(|r| format!("  [{r}]")).unwrap_or_default()
    );
    if full {
        println!("  mine:   {}", c.mine.as_deref().unwrap_or("(deleted)"));
        println!("  theirs: {}", c.theirs.as_deref().unwrap_or("(deleted)"));
        if c.resolved_at.is_none() {
            println!("  `mnnn sync resolve {} --take-other` puts {other} back", &c.id[..8.min(c.id.len())]);
        }
    }
}

pub fn run(ctx: &Ctx, cmd: SyncCommand) -> anyhow::Result<()> {
    match cmd {
        SyncCommand::Status { json } => {
            let n = Net::open(ctx)?;
            crate::net::user::load_or_create(&n.store)?;
            let nodes = sync::linked(&n.store)?;
            let open = n.store.sync_conflicts(false)?.len();
            if json {
                let v: Vec<_> = nodes
                    .iter()
                    .map(|(u, x)| {
                        let st = sync::state(&n.store, &u.peer_id);
                        serde_json::json!({ "peer": u.peer_id, "label": u.label, "reachable": x.is_some(), "at": st.at, "last": st.last, "error": st.error })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "nodes": v, "conflicts": open }))?);
                return Ok(());
            }
            if nodes.is_empty() {
                println!("no linked nodes: link one with `mnnn user link` (see `mnnn help users`)");
            }
            for (u, x) in &nodes {
                let st = sync::state(&n.store, &u.peer_id);
                let what = match (&st.error, x) {
                    (_, None) => "no key exchange yet".to_string(),
                    (Some(e), _) => format!("failed: {e}"),
                    (None, _) if st.at.is_some() => sync::summary(&st.last),
                    _ => "not synced yet".into(),
                };
                println!("{}  last {}  {}", name(u), when(st.at), what);
            }
            println!("{open} open conflict(s){}", if open > 0 { " (`mnnn sync conflicts`)" } else { "" });
            Ok(())
        }
        SyncCommand::Now { node } => {
            let mut n = Net::open(ctx)?;
            crate::net::user::load_or_create(&n.store)?;
            let targets = pick(&n, node.as_deref())?;
            let relays = n.relays();
            n.node()?;
            let node = n.node.as_ref().expect("node");
            let mut failed = 0;
            for (u, x) in targets {
                match sync::pull(&n.store, node, &x, &relays, ORIGIN) {
                    Ok(a) => println!("{}: {}", name(&u), sync::summary(&a)),
                    Err(e) => {
                        failed += 1;
                        eprintln!("{}: {e}", name(&u));
                    }
                }
            }
            if failed > 0 {
                return Err(anyhow!("{failed} node(s) could not be synced"));
            }
            Ok(())
        }
        SyncCommand::Conflicts { all, json } => {
            let n = Net::open(ctx)?;
            let cs = n.store.sync_conflicts(all)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&cs)?);
                return Ok(());
            }
            if cs.is_empty() {
                println!("no {}conflicts", if all { "" } else { "open " });
            }
            for c in &cs {
                print_conflict(c, false);
            }
            Ok(())
        }
        SyncCommand::Show { id } => {
            let n = Net::open(ctx)?;
            print_conflict(&n.store.sync_conflict(&id)?, true);
            Ok(())
        }
        SyncCommand::Resolve { id, take_other } => {
            let n = Net::open(ctx)?;
            let c = n.store.sync_resolve(&id, take_other)?;
            n.store.note_action(
                &crate::store::NewAction::new(
                    "sync.resolve",
                    ORIGIN,
                    format!("sync conflict in {} {}", c.tbl, c.resolution.clone().unwrap_or_default()),
                )
                .params(serde_json::json!({ "conflict": c.id, "tbl": c.tbl, "resolution": c.resolution })),
            );
            println!("{}: {}", &c.id[..8.min(c.id.len())], c.resolution.unwrap_or_default());
            Ok(())
        }
        SyncCommand::Bars { from, source, symbol } => {
            let mut n = Net::open(ctx)?;
            crate::net::user::load_or_create(&n.store)?;
            let sym = symbol.as_deref().map(Symbol::parse).transpose()?;
            let targets = pick(&n, from.as_deref())?;
            let relays = n.relays();
            n.node()?;
            let node = n.node.take().expect("node");
            let mut total = 0u64;
            for (u, x) in targets {
                let series = sync::remote_series(&n.store, &node, &x, &relays)?;
                for r in series.iter().filter(|r| {
                    source.as_deref().is_none_or(|s| s == r.source) && sym.as_ref().is_none_or(|s| s.base == r.base && s.quote == r.quote)
                }) {
                    let added = sync::pull_bars(&mut n.store, &node, &x, r, &relays)?;
                    total += added;
                    println!("{}: {} {}/{} {}s: {added} new bar(s)", name(&u), r.source, r.base, r.quote, r.duration);
                }
            }
            if total > 0 {
                n.store.note_action(
                    &crate::store::NewAction::new("sync.bars", ORIGIN, format!("fetched {total} bar(s) from linked nodes"))
                        .params(serde_json::json!({ "bars": total })),
                );
            }
            println!("{total} bar(s) added");
            Ok(())
        }
    }
}
