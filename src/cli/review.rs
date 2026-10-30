//! `mnnn review`: reviews. Thin over [`crate::review`].
use super::net::{Net, read_token};
use super::{Ctx, ReviewAnchor, ReviewCommand};
use crate::ingest::time::fmt_ts;
use crate::review::net as rnet;
use crate::review::{self, Ask, Who};
use crate::store::{Review, ReviewItem, ReviewItemKind, ReviewKind, Reviewing, Store};
use anyhow::{Context, anyhow, bail};
use serde_json::{Value, json};

const ORIGIN: &str = "cli";

/// A review of mine, or one I joined on another node.
enum Target {
    Mine(Review),
    Joined(Reviewing),
}

fn target(s: &Store, key: &str) -> anyhow::Result<Target> {
    match s.find_review(key) {
        Ok(r) => Ok(Target::Mine(r)),
        Err(e) => s.find_reviewing(key).map(Target::Joined).map_err(|_| anyhow!(e)),
    }
}

fn mine(s: &Store, key: &str) -> anyhow::Result<Review> {
    match target(s, key)? {
        Target::Mine(r) => Ok(r),
        Target::Joined(g) => bail!("{} is another node's review; its author does this", g.title()),
    }
}

fn kind(k: &str) -> anyhow::Result<ReviewKind> {
    let k = match k.to_ascii_lowercase().as_str() {
        "live-search" | "searches" => "search".to_string(),
        "entries" => "entry".into(),
        k => k.trim_end_matches('s').to_string(),
    };
    ReviewKind::parse(&k).ok_or_else(|| anyhow!("`{k}`: review an entry, journal, project, search or record"))
}

fn pretty(v: &impl serde::Serialize) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

fn ask(kind: ReviewItemKind, a: ReviewAnchor) -> Ask {
    Ask { kind, target: a.entry.unwrap_or_default(), field: a.field.unwrap_or_default(), at: a.at, nth: a.nth, ..Default::default() }
}

/// Add an item: here (my review) or through the author's node (a joined one).
fn add(ctx: &Ctx, key: &str, ask: Ask) -> anyhow::Result<()> {
    let mut n = Net::open(ctx)?;
    let it = match target(&n.store, key)? {
        Target::Mine(r) => review::add(&n.store, ORIGIN, &r, &Who::Author, &ask)?,
        Target::Joined(g) => {
            let relays = n.relays();
            n.node()?;
            rnet::add(&n.store, n.node.as_ref().expect("node"), &g, &ask, &relays, ORIGIN)?
        }
    };
    ctx.io.info(review::line(&it));
    Ok(())
}

/// Print a link for a reviewer (or that it was sent).
fn link(ctx: &Ctx, c: &crate::net::client::Created) {
    match &c.delivered {
        Some(Ok(_)) => ctx.io.note("Sent; they join with `mnnn review join` (or the TUI)."),
        other => {
            if let Some(Err(e)) = other {
                ctx.io.warn(format!("could not reach them ({e})"));
            }
            ctx.io.note("Give them this link (they run `mnnn review join TOKEN`; it is good for one reviewer for a week):");
            println!("{}", c.token);
        }
    }
}

fn print_thread(items: &[ReviewItem]) {
    let (a, c, r) = review::tally(items);
    println!("votes: {a} approve, {c} changes, {r} reject");
    for i in items {
        println!("  {}", review::line(i));
    }
}

fn print_view(v: &Value) {
    if let Some(c) = v["content"].as_object() {
        for (k, val) in c {
            println!("{k}: {}", text(val));
        }
    }
    if let Some(es) = v["entries"].as_array() {
        for e in es {
            let id = e["id"].as_str().unwrap_or_default();
            println!(
                "\n## {} {}\n{}",
                crate::store::projects::short_of(id),
                e["title"].as_str().unwrap_or_default(),
                e["body"].as_str().unwrap_or_default()
            );
        }
    }
}

pub fn run(ctx: &Ctx, cmd: ReviewCommand) -> anyhow::Result<()> {
    match cmd {
        ReviewCommand::Request { kind: k, target: t, title, note, allow, label, peer, no_link } => {
            let k = kind(&k)?;
            let mut n = Net::open(ctx)?;
            let r = review::request(&n.store, ORIGIN, k, &t, &title, &note, &allow)?;
            ctx.io.info(format!("review {} of {} {} ({}; reviewers may {})", r.short_id(), k.label(), r.name, r.title(), r.allow.join(", ")));
            if no_link {
                ctx.io.note("Make links with `mnnn review invite`.");
                return Ok(());
            }
            let relays = n.relays();
            if peer.is_some() {
                n.node()?;
            }
            let (_, c) = rnet::invite(&n.store, n.node.as_ref(), &n.kp, &r, &label, peer.as_deref(), &relays, ORIGIN)?;
            link(ctx, &c);
            Ok(())
        }
        ReviewCommand::Invite { review: key, label, peer } => {
            let mut n = Net::open(ctx)?;
            let r = mine(&n.store, &key)?;
            let relays = n.relays();
            if peer.is_some() {
                n.node()?;
            }
            let (v, c) = rnet::invite(&n.store, n.node.as_ref(), &n.kp, &r, &label, peer.as_deref(), &relays, ORIGIN)?;
            ctx.io.info(format!("link {} for {}", v.short_id(), r.title()));
            link(ctx, &c);
            Ok(())
        }
        ReviewCommand::List { json } => {
            let n = Net::open(ctx)?;
            let s = &n.store;
            let mine = s.reviews()?;
            let joined = s.reviewing()?;
            if json {
                return pretty(&json!({ "mine": mine, "joined": joined }));
            }
            if mine.is_empty() && joined.is_empty() {
                ctx.io.note("No reviews yet; ask for one with `mnnn review request`.");
            }
            for r in &mine {
                let items = s.review_items(&r.id)?;
                let (a, c, j) = review::tally(&items);
                println!(
                    "{:<8} {:<28} {:<7} {:<6} {:>3} items  {a}/{c}/{j} votes  {}",
                    r.short_id(),
                    r.title(),
                    r.kind.as_str(),
                    r.status.as_str(),
                    items.len(),
                    fmt_ts(r.updated_at)
                );
            }
            for g in &joined {
                println!(
                    "{:<8} {:<28} {:<7} {:<6} reviewing on {}",
                    g.short_id(),
                    g.title(),
                    g.kind,
                    g.status.as_str(),
                    crate::store::net::short_peer(&g.node_peer)
                );
            }
            Ok(())
        }
        ReviewCommand::Show { review: key, json } => {
            let mut n = Net::open(ctx)?;
            match target(&n.store, &key)? {
                Target::Mine(r) => {
                    let s = &n.store;
                    let reviewers = s.reviewers(&r.id)?;
                    let items = s.review_items(&r.id)?;
                    if json {
                        let (a, c, j) = review::tally(&items);
                        return pretty(&json!({ "review": r, "reviewers": reviewers, "items": items,
                                               "votes": { "approve": a, "changes": c, "reject": j } }));
                    }
                    println!("{} ({} {}) {}", r.title(), r.kind.label(), r.name, r.status.as_str());
                    if !r.note.is_empty() {
                        println!("note: {}", r.note);
                    }
                    println!("reviewers may: {}", r.allow.join(", "));
                    for v in &reviewers {
                        println!("  reviewer {} {:<20} {}", v.short_id(), v.title(), v.status.as_str());
                    }
                    print_thread(&items);
                }
                Target::Joined(g) => {
                    let relays = n.relays();
                    n.node()?;
                    let node = n.node.as_ref().expect("node");
                    let g = rnet::sync(&n.store, node, &g, &relays)?;
                    let items = rnet::thread(&n.store, node, &g, &relays)?;
                    if json {
                        return pretty(&json!({ "reviewing": g, "items": items }));
                    }
                    println!("{} ({} {}) {} on {}", g.title(), g.kind, g.name, g.status.as_str(), crate::store::net::short_peer(&g.node_peer));
                    println!("you may: {}", g.allow.join(", "));
                    print_thread(&items);
                }
            }
            Ok(())
        }
        ReviewCommand::View { review: key, json } => {
            let mut n = Net::open(ctx)?;
            let v = match target(&n.store, &key)? {
                Target::Mine(r) => review::view(&n.store, &r)?,
                Target::Joined(g) => {
                    let relays = n.relays();
                    n.node()?;
                    rnet::view(&n.store, n.node.as_ref().expect("node"), &g, &relays)?
                }
            };
            if json {
                return pretty(&v);
            }
            print_view(&v);
            Ok(())
        }
        ReviewCommand::Join { token, label } => {
            let mut n = Net::open(ctx)?;
            let token = read_token(&token)?;
            n.node()?;
            let (g, a) = rnet::join(&n.store, n.node.as_ref(), &n.kp, &token, &label, ORIGIN)?;
            ctx.io.info(format!("reviewing {} ({} {}); you may {}", g.title(), g.kind, g.name, g.allow.join(", ")));
            if let Some(ans) = &a.answer
                && !matches!(a.delivered, Some(Ok(_)))
            {
                ctx.io.note("Their node was not reachable; give this answer back (they run `mnnn net exchange accept TOKEN`):");
                println!("{ans}");
            }
            Ok(())
        }
        ReviewCommand::Sync { review: key } => {
            let mut n = Net::open(ctx)?;
            let g = n.store.find_reviewing(&key)?;
            let relays = n.relays();
            n.node()?;
            let g = rnet::sync(&n.store, n.node.as_ref().expect("node"), &g, &relays)?;
            let items = g.info["items"].as_u64().unwrap_or(0);
            ctx.io.info(format!("{}: {}, {items} items; you may {}", g.title(), g.status.as_str(), g.allow.join(", ")));
            Ok(())
        }
        ReviewCommand::Comment { review: key, text, anchor, reply } => {
            let mut a = ask(ReviewItemKind::Comment, anchor);
            a.body = text;
            a.reply_to = reply;
            add(ctx, &key, a)
        }
        ReviewCommand::Suggest { review: key, to, json, why, anchor } => {
            let mut a = ask(ReviewItemKind::Suggestion, anchor);
            a.value = Some(if json { serde_json::from_str(&to).context("--to is not JSON")? } else { Value::String(to) });
            a.body = why;
            add(ctx, &key, a)
        }
        ReviewCommand::Highlight { review: key, anchor, color, note } => {
            let mut a = ask(ReviewItemKind::Highlight, anchor);
            a.value = color.map(Value::String);
            a.body = note;
            add(ctx, &key, a)
        }
        ReviewCommand::Vote { review: key, vote, on, note } => {
            let a = Ask { kind: ReviewItemKind::Vote, value: Some(Value::String(vote)), reply_to: on, body: note, ..Default::default() };
            add(ctx, &key, a)
        }
        ReviewCommand::Withdraw { review: key, item } => {
            let mut n = Net::open(ctx)?;
            let it = match target(&n.store, &key)? {
                Target::Mine(r) => review::withdraw(&n.store, ORIGIN, &r, &Who::Author, &item)?,
                Target::Joined(g) => {
                    let relays = n.relays();
                    n.node()?;
                    rnet::withdraw(&n.store, n.node.as_ref().expect("node"), &g, &item, &relays, ORIGIN)?
                }
            };
            ctx.io.info(format!("withdrew {}", it.short_id()));
            Ok(())
        }
        ReviewCommand::Accept { review: key, items, all } => decide(ctx, &key, &items, all, true),
        ReviewCommand::Reject { review: key, items, all } => decide(ctx, &key, &items, all, false),
        ReviewCommand::Merge { review: key, force } => {
            let n = Net::open(ctx)?;
            let r = mine(&n.store, &key)?;
            let m = review::merge(&n.store, ORIGIN, &r, force)?;
            if m.merged.is_empty() && m.conflicts.is_empty() {
                ctx.io.note("Nothing to merge; accept suggestions first (`mnnn review accept`).");
            }
            for v in &m.revisions {
                ctx.io.info(format!(
                    "{} {} is at revision {} ({} suggestions)",
                    v.kind.label(),
                    crate::store::projects::short_of(&v.ref_id),
                    v.rev,
                    v.items.len()
                ));
            }
            for (id, why) in &m.conflicts {
                ctx.io.warn(format!("{} left accepted: {why}", crate::store::projects::short_of(id)));
            }
            if !m.conflicts.is_empty() {
                ctx.io.note("Fix them by hand, reject them, or merge with --force.");
            }
            Ok(())
        }
        ReviewCommand::Close { review: key } => {
            let mut n = Net::open(ctx)?;
            let r = mine(&n.store, &key)?;
            let relays = n.relays();
            let _ = n.node();
            let r = rnet::close(&n.store, n.node.as_ref(), &r, &relays, ORIGIN)?;
            ctx.io.info(format!("closed {}", r.title()));
            Ok(())
        }
        ReviewCommand::Reopen { review: key } => {
            let n = Net::open(ctx)?;
            let r = mine(&n.store, &key)?;
            let r = review::reopen(&n.store, ORIGIN, &r)?;
            ctx.io.info(format!("opened {} again; make new links with `mnnn review invite`", r.title()));
            Ok(())
        }
        ReviewCommand::Drop { review: key, reviewer } => {
            let mut n = Net::open(ctx)?;
            let r = mine(&n.store, &key)?;
            let v = n.store.find_reviewer(&r.id, &reviewer)?;
            let relays = n.relays();
            let _ = n.node();
            let v = rnet::drop_reviewer(&n.store, n.node.as_ref(), &r, &v, &relays, ORIGIN)?;
            ctx.io.info(format!("took {} off {}", v.title(), r.title()));
            Ok(())
        }
        ReviewCommand::Leave { review: key } => {
            let mut n = Net::open(ctx)?;
            let g = n.store.find_reviewing(&key)?;
            let relays = n.relays();
            let _ = n.node();
            let g = rnet::leave(&n.store, n.node.as_ref(), &g, &relays, ORIGIN)?;
            ctx.io.info(format!("left {}", g.title()));
            Ok(())
        }
        ReviewCommand::Remove { review: key } => {
            let n = Net::open(ctx)?;
            match target(&n.store, &key)? {
                Target::Mine(r) => {
                    let items = n.store.review_items(&r.id)?.len();
                    if !ctx.io.confirm(&format!("Delete the review {} and its {items} thread items? (revisions stay)", r.title()))? {
                        return Ok(());
                    }
                    review::remove(&n.store, ORIGIN, &r)?;
                    ctx.io.info(format!("removed {}", r.title()));
                }
                Target::Joined(g) => {
                    if g.status == crate::store::ReviewingStatus::Joined {
                        bail!("you are still reviewing {}; `mnnn review leave` it first", g.title());
                    }
                    n.store.forget_reviewing(&g.review_id)?;
                    ctx.io.info(format!("forgot {}", g.title()));
                }
            }
            Ok(())
        }
        ReviewCommand::Revisions { kind: k, target: t, json } => {
            let n = Net::open(ctx)?;
            let vs = review::revisions(&n.store, kind(&k)?, &t)?;
            if json {
                return pretty(&vs);
            }
            if vs.is_empty() {
                ctx.io.note("No revisions yet; merging a review makes one.");
            }
            for v in &vs {
                let by = v.author_id.as_deref().and_then(|a| n.store.author(a).ok().flatten()).map(|a| a.name).unwrap_or_default();
                let review = v.review_id.as_deref().map(crate::store::projects::short_of).unwrap_or("-");
                println!("rev {:<3} {}  review {review}  {} suggestions  {by}", v.rev, fmt_ts(v.created_at), v.items.len());
                if let (Some(b), Some(a)) = (v.before.as_object(), v.after.as_object()) {
                    for (k, after) in a {
                        if b.get(k) != Some(after) {
                            println!("    {k}: {} → {}", text(b.get(k).unwrap_or(&Value::Null)), text(after));
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

fn decide(ctx: &Ctx, key: &str, items: &[String], all: bool, accept: bool) -> anyhow::Result<()> {
    if items.is_empty() && !all {
        bail!("name the suggestions, or give --all");
    }
    let n = Net::open(ctx)?;
    let r = mine(&n.store, key)?;
    let out = review::decide(&n.store, ORIGIN, &r, items, accept)?;
    let verb = if accept { "accepted" } else { "rejected" };
    if out.is_empty() {
        ctx.io.note("No open suggestions.");
    }
    for i in &out {
        ctx.io.info(format!("{verb} {}", review::line(i)));
    }
    if accept && !out.is_empty() {
        ctx.io.note("Merge them with `mnnn review merge`.");
    }
    Ok(())
}
