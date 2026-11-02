//! `mnnn notify` (2.18).
use super::{Ctx, NotifyCommand};
use crate::ingest::time::fmt_ts;
use crate::notify;
use crate::store::{NewNotification, Notification, Store};

fn line(store: &Store, n: &Notification) -> String {
    let link = n.link.as_deref().map(|l| format!("\t→ {}", notify::describe_link(store, l))).unwrap_or_default();
    format!(
        "{} {}\t{}\t{}\t{}\t{}{link}",
        if n.read_at.is_none() { "●" } else { " " },
        &n.id[..n.id.len().min(8)],
        fmt_ts(n.created_at),
        n.level,
        n.kind,
        n.title
    )
}

pub fn run(ctx: &Ctx, cmd: NotifyCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        NotifyCommand::List { unread, limit, json } => {
            let v = store.notifications(unread, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else if v.is_empty() {
                ctx.io.info(if unread { "no unread notifications" } else { "no notifications" });
            } else {
                for n in &v {
                    println!("{}", line(&store, n));
                }
            }
        }
        NotifyCommand::Show { id, json } => {
            let n = store.find_notification(&id)?;
            store.mark_notification(&n.id, true)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&n)?);
            } else {
                println!("{} [{}] {}", n.title, n.level, fmt_ts(n.created_at));
                println!("from: {}", n.kind);
                if let Some(l) = &n.link {
                    println!("about: {} ({l})", notify::describe_link(&store, l));
                }
                if !n.body.is_empty() {
                    println!("\n{}", n.body);
                }
            }
        }
        NotifyCommand::Read { id, all } => {
            if all || id.is_none() {
                let k = store.mark_all_notifications()?;
                ctx.io.info(format!("marked {k} read"));
            } else {
                let n = store.find_notification(id.as_deref().unwrap_or_default())?;
                store.mark_notification(&n.id, true)?;
                ctx.io.info(format!("read: {}", n.title));
            }
        }
        NotifyCommand::Unread { id } => {
            let n = store.find_notification(&id)?;
            store.mark_notification(&n.id, false)?;
            ctx.io.info(format!("unread: {}", n.title));
        }
        NotifyCommand::Send { title, body, link, level } => {
            let n = notify::add(&store, "cli", &NewNotification { kind: "notify".into(), level, title, body, link, action_id: None })?;
            ctx.io.info(format!("notified: {} ({})", n.title, &n.id[..8.min(n.id.len())]));
        }
        NotifyCommand::Kinds { json } => {
            let m = notify::muted(&store);
            if json {
                let v: Vec<_> = notify::BUILTINS
                    .iter()
                    .map(|b| serde_json::json!({ "kind": b.kind, "about": b.about, "muted": m.iter().any(|k| k == b.kind) }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for b in notify::BUILTINS {
                    println!("{}{}\t{}", b.kind, if m.iter().any(|k| k == b.kind) { " (muted)" } else { "" }, b.about);
                }
            }
        }
        NotifyCommand::Mute { kind } => {
            notify::set_muted(&store, &kind, true)?;
            ctx.io.info(format!("muted {kind}"));
        }
        NotifyCommand::Unmute { kind } => {
            notify::set_muted(&store, &kind, false)?;
            ctx.io.info(format!("unmuted {kind}"));
        }
    }
    Ok(())
}
