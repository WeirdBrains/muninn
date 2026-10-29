//! `mnnn history`.
use super::{Ctx, HistoryCommand};
use crate::history::{ViewState, apply_move, author_label};
use crate::ingest::time::fmt_ts;
use crate::store::{ActionRecord, Move, Store};

fn line(store: &Store, a: &ActionRecord, head: Option<i64>) -> String {
    format!(
        "{} #{}\t{}\t{}\t{}\t{}\t{}",
        if head == Some(a.id) { "*" } else { " " },
        a.id,
        fmt_ts(a.created_at),
        a.kind,
        a.origin,
        author_label(store, a.author_id.as_deref()),
        a.summary
    )
}

fn moved(ctx: &Ctx, store: &Store, m: &Move) -> anyhow::Result<()> {
    let mut prefs = ctx.prefs()?;
    let v = apply_move(m, &mut prefs, &ctx.paths)?;
    let at = match m.to {
        Some(id) => store.action(id)?.map(|a| format!("#{id} {}", a.summary)).unwrap_or_else(|| format!("#{id}")),
        None => "the start (before any action)".into(),
    };
    ctx.io.info(format!("head: {at}"));
    if let Some(v) = v {
        ctx.io.info(format!("view: theme {} ({}), {}", v.theme, v.mode, v.filters_label()));
    }
    Ok(())
}

pub fn run(ctx: &Ctx, cmd: HistoryCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let head = store.history_head()?;
    match cmd {
        HistoryCommand::List { limit, all, json } => {
            let list: Vec<ActionRecord> = if all {
                let mut v = store.actions()?;
                v.reverse();
                v
            } else {
                store.action_path(head)?
            };
            let list: Vec<_> = list.into_iter().take(limit).collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for a in &list {
                    println!("{}", line(&store, a, head));
                }
                if list.is_empty() {
                    ctx.io.info("no actions recorded yet");
                }
            }
        }
        HistoryCommand::Show { id, json } => {
            let id = id.or(head).ok_or_else(|| anyhow::anyhow!("no actions recorded yet"))?;
            let a = store.action(id)?.ok_or_else(|| anyhow::anyhow!("no action #{id}"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&a)?);
                return Ok(());
            }
            println!("action:  #{}{}", a.id, if head == Some(a.id) { "  (head)" } else { "" });
            println!("kind:    {}", a.kind);
            println!("summary: {}", a.summary);
            println!("when:    {}", fmt_ts(a.created_at));
            println!("origin:  {}", a.origin);
            println!("author:  {}", author_label(&store, a.author_id.as_deref()));
            println!("parent:  {}", a.parent.map(|p| format!("#{p}")).unwrap_or_else(|| "-".into()));
            let kids = store.action_children(Some(a.id))?;
            if !kids.is_empty() {
                let k: Vec<String> = kids.iter().map(|k| format!("#{k}")).collect();
                println!("next:    {}{}", k.join(", "), if kids.len() > 1 { "  (forks)" } else { "" });
            }
            if a.params.as_object().is_some_and(|o| !o.is_empty()) {
                println!("params:  {}", serde_json::to_string(&a.params)?);
            }
            for (label, v) in [("before", &a.before), ("after", &a.after)] {
                if let Some(s) = v.as_ref().and_then(ViewState::from_value) {
                    println!("{label}:  theme {} ({}), {}", s.theme, s.mode, s.filters_label());
                }
            }
        }
        HistoryCommand::Tree => {
            let rows = store.history_tree()?;
            if rows.is_empty() {
                ctx.io.info("no actions recorded yet");
            }
            for r in rows {
                let mark = if r.head {
                    "●"
                } else if r.current {
                    "│"
                } else {
                    "○"
                };
                let fork = match (r.fork, r.action.parent) {
                    (true, Some(p)) => format!("⑂ from #{p} "),
                    (true, None) => "⑂ from start ".into(),
                    _ => String::new(),
                };
                println!(
                    "{}{mark} #{} {fork}{}  [{} · {}]",
                    "  ".repeat(r.depth),
                    r.action.id,
                    r.action.summary,
                    r.action.kind,
                    fmt_ts(r.action.created_at)
                );
            }
        }
        HistoryCommand::Back { steps } => moved(ctx, &store, &store.history_back(steps)?)?,
        HistoryCommand::Forward { steps } => moved(ctx, &store, &store.history_forward(steps)?)?,
        HistoryCommand::Goto { id } => moved(ctx, &store, &store.history_goto((id != 0).then_some(id))?)?,
    }
    Ok(())
}
