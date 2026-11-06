//! Reviews (`mnnn review` / TUI Reviews, `V`). Exchange tokens and the node key stay in Rust;
//! Dart sees titles, thread text and the public review link (`mnnn review invite`).
use super::data::{open, open_write};
use super::info::paths;
use super::net::hosted_node;
use muninn_core::net::{exchange, identity};
use muninn_core::prefs::Preferences;
use muninn_core::review::net as rnet;
use muninn_core::review::{self, Ask, Who};
use muninn_core::store::{Review, ReviewItem, ReviewItemKind, ReviewKind, Reviewing, ReviewingStatus, Store};
use serde_json::Value;

const ORIGIN: &str = "app";

pub struct ReviewInfo {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub kind: String,
    pub name: String,
    pub note: String,
    pub status: String,
    pub allow: String,
    pub items: i64,
    pub votes_approve: i64,
    pub votes_changes: i64,
    pub votes_reject: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct ReviewerInfo {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub status: String,
    pub label: String,
}

pub struct ReviewItemInfo {
    pub id: String,
    pub short_id: String,
    pub kind: String,
    pub who: String,
    pub state: String,
    pub field: String,
    pub target: String,
    pub quote: String,
    pub body: String,
    pub value: String,
    pub line: String,
    pub created_at: i64,
}

pub struct ReviewDetail {
    pub review: ReviewInfo,
    pub mine: bool,
    pub note: String,
    pub reviewers: Vec<ReviewerInfo>,
    pub items: Vec<ReviewItemInfo>,
    pub you_may: String,
    pub peer: String,
}

pub struct ReviewingInfo {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub allow: String,
    pub peer: String,
    pub items: i64,
    pub updated_at: i64,
}

fn parse_kind(k: &str) -> anyhow::Result<ReviewKind> {
    let k = match k.to_ascii_lowercase().as_str() {
        "live-search" | "searches" => "search".to_string(),
        "entries" => "entry".into(),
        k => k.trim_end_matches('s').to_string(),
    };
    ReviewKind::parse(&k).ok_or_else(|| anyhow::anyhow!("`{k}`: review an entry, journal, project, search or record"))
}

fn item_info(i: &ReviewItem) -> ReviewItemInfo {
    ReviewItemInfo {
        short_id: i.short_id().to_string(),
        kind: i.kind.as_str().into(),
        state: i.state.as_str().into(),
        value: i
            .value
            .as_ref()
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                v => v.to_string(),
            })
            .unwrap_or_default(),
        line: review::line(i),
        id: i.id.clone(),
        who: i.who.clone(),
        field: i.field.clone(),
        target: i.target.clone(),
        quote: i.quote.clone(),
        body: i.body.clone(),
        created_at: i.created_at,
    }
}

fn review_info(r: &Review, items: &[ReviewItem]) -> ReviewInfo {
    let (a, c, j) = review::tally(items);
    ReviewInfo {
        short_id: r.short_id().to_string(),
        title: r.title(),
        kind: r.kind.as_str().into(),
        status: r.status.as_str().into(),
        allow: r.allow.join(", "),
        items: items.len() as i64,
        votes_approve: a as i64,
        votes_changes: c as i64,
        votes_reject: j as i64,
        id: r.id.clone(),
        name: r.name.clone(),
        note: r.note.clone(),
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

fn reviewing_info(g: &Reviewing) -> ReviewingInfo {
    let items: Vec<ReviewItem> = serde_json::from_value(g.thread.clone()).unwrap_or_default();
    ReviewingInfo {
        short_id: g.short_id().to_string(),
        title: g.title(),
        allow: g.allow.join(", "),
        peer: muninn_core::store::net::short_peer(&g.node_peer).to_string(),
        items: items.len() as i64,
        id: g.review_id.clone(),
        kind: g.kind.clone(),
        name: g.name.clone(),
        status: g.status.as_str().into(),
        updated_at: g.updated_at,
    }
}

enum Target {
    Mine(Review),
    Joined(Reviewing),
}

fn target(s: &Store, key: &str) -> anyhow::Result<Target> {
    match s.find_review(key) {
        Ok(r) => Ok(Target::Mine(r)),
        Err(e) => s.find_reviewing(key).map(Target::Joined).map_err(|_| anyhow::anyhow!("{e}")),
    }
}

fn mine(s: &Store, key: &str) -> anyhow::Result<Review> {
    match target(s, key)? {
        Target::Mine(r) => Ok(r),
        Target::Joined(g) => Err(anyhow::anyhow!("{} is another node's review; its author does this", g.title())),
    }
}

fn relays() -> Vec<String> {
    let Ok(p) = paths(None) else { return vec![] };
    let prefs: Preferences = std::fs::read_to_string(p.home.join("preferences.toml")).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default();
    prefs.net.relays
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

fn view_lines(v: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(c) = v["content"].as_object() {
        for (k, val) in c {
            lines.push(format!("{k}: {}", text_of(val)));
        }
    }
    if let Some(es) = v["entries"].as_array() {
        for e in es {
            let id = e["id"].as_str().unwrap_or_default();
            lines.push(String::new());
            lines.push(format!("## {} {}", muninn_core::store::projects::short_of(id), e["title"].as_str().unwrap_or_default()));
            let body = e["body"].as_str().unwrap_or_default();
            if !body.is_empty() {
                lines.push(body.to_string());
            }
        }
    }
    if lines.is_empty() {
        lines.push(v.to_string());
    }
    lines
}

pub(crate) fn reviews_of(store: &Store) -> anyhow::Result<Vec<ReviewInfo>> {
    let mut out = Vec::new();
    for r in store.reviews()? {
        let items = store.review_items(&r.id).unwrap_or_default();
        out.push(review_info(&r, &items));
    }
    Ok(out)
}

pub(crate) fn reviewing_of(store: &Store) -> anyhow::Result<Vec<ReviewingInfo>> {
    Ok(store.reviewing()?.into_iter().map(|g| reviewing_info(&g)).collect())
}

/// `mnnn review list`: reviews of mine.
pub fn reviews() -> anyhow::Result<Vec<ReviewInfo>> {
    reviews_of(&open()?)
}

/// Reviews this node joined (`mnnn review list` second half).
pub fn reviews_joined() -> anyhow::Result<Vec<ReviewingInfo>> {
    reviewing_of(&open()?)
}

/// `mnnn review request`. Does not make a link; use [review_invite].
pub fn review_request(kind: String, target: String, title: String, note: String, allow: Vec<String>) -> anyhow::Result<ReviewInfo> {
    let store = open_write()?;
    let k = parse_kind(&kind)?;
    let r = review::request(&store, ORIGIN, k, target.trim(), &title, &note, &allow)?;
    let items = store.review_items(&r.id).unwrap_or_default();
    Ok(review_info(&r, &items))
}

fn detail_of(store: &Store, r: &Review) -> anyhow::Result<ReviewDetail> {
    let items = store.review_items(&r.id)?;
    let reviewers = store.reviewers(&r.id)?;
    Ok(ReviewDetail {
        review: review_info(r, &items),
        mine: true,
        note: r.note.clone(),
        you_may: r.allow.join(", "),
        peer: String::new(),
        reviewers: reviewers
            .into_iter()
            .map(|v| ReviewerInfo {
                short_id: v.short_id().to_string(),
                title: v.title(),
                status: v.status.as_str().into(),
                label: v.label,
                id: v.id,
            })
            .collect(),
        items: items.iter().map(item_info).collect(),
    })
}

/// `mnnn review show`.
pub fn review_show(review: String) -> anyhow::Result<ReviewDetail> {
    let store = open()?;
    match target(&store, review.trim())? {
        Target::Mine(r) => detail_of(&store, &r),
        Target::Joined(g) => {
            let items: Vec<ReviewItem> = serde_json::from_value(g.thread.clone()).unwrap_or_default();
            Ok(ReviewDetail {
                review: ReviewInfo {
                    short_id: g.short_id().to_string(),
                    title: g.title(),
                    kind: g.kind.clone(),
                    name: g.name.clone(),
                    note: String::new(),
                    status: g.status.as_str().into(),
                    allow: g.allow.join(", "),
                    items: items.len() as i64,
                    votes_approve: review::tally(&items).0 as i64,
                    votes_changes: review::tally(&items).1 as i64,
                    votes_reject: review::tally(&items).2 as i64,
                    id: g.review_id.clone(),
                    created_at: g.created_at,
                    updated_at: g.updated_at,
                },
                mine: false,
                note: String::new(),
                reviewers: vec![],
                items: items.iter().map(item_info).collect(),
                you_may: g.allow.join(", "),
                peer: muninn_core::store::net::short_peer(&g.node_peer).to_string(),
            })
        }
    }
}

/// `mnnn review view`.
pub fn review_view(review: String) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let v = match target(&store, review.trim())? {
        Target::Mine(r) => review::view(&store, &r)?,
        Target::Joined(g) => g.copy,
    };
    Ok(view_lines(&v))
}

/// `mnnn review invite`: returns the public review link. The exchange token stays in Rust.
pub fn review_invite(review: String, label: String, peer: Option<String>) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = mine(&store, review.trim())?;
    let kp = identity::load_or_create(&store)?;
    let node = hosted_node();
    let peer = peer.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (_, c) = rnet::invite(&store, node.as_deref(), &kp, &r, &label, peer, &relays(), ORIGIN)?;
    match c.delivered {
        Some(Ok(_)) => Ok("sent; they join with the request title or `mnnn review join`".into()),
        Some(Err(e)) => Ok(format!("could not reach them ({e}): {}", exchange::link(&c.token))),
        None => Ok(exchange::link(&c.token)),
    }
}

/// `mnnn review join` / TUI `j`.
pub fn review_join(token: String, label: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let kp = identity::load_or_create(&store)?;
    let node = hosted_node();
    let (g, _) = rnet::join(&store, node.as_deref(), &kp, token.trim(), &label, ORIGIN)?;
    Ok(format!("reviewing {} ({} {}); you may {}", g.title(), g.kind, g.name, g.allow.join(", ")))
}

/// `mnnn review sync`. Needs a running node.
pub fn review_sync(review: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let g = store.find_reviewing(review.trim())?;
    let Some(node) = hosted_node() else {
        return Err(anyhow::anyhow!("start the node (Net ▸ Start) to refresh a joined review"));
    };
    let (g, r) = rnet::refresh(&store, node.as_ref(), &g, &relays(), ORIGIN)?;
    Ok(format!("{}: {}; you may {}; {} kept item(s) sent", g.title(), g.status.as_str(), g.allow.join(", "), r.sent))
}

fn add(store: &Store, key: &str, ask: Ask) -> anyhow::Result<String> {
    match target(store, key)? {
        Target::Mine(r) => {
            let it = review::add(store, ORIGIN, &r, &Who::Author, &ask)?;
            Ok(review::line(&it))
        }
        Target::Joined(g) => {
            let node = hosted_node();
            match rnet::send_or_queue(store, node.as_deref(), &g, &ask, &relays(), ORIGIN)? {
                rnet::Sent::Item(it) => Ok(review::line(&it)),
                rnet::Sent::Queued(_) => Ok(format!("{} kept here; it is sent when the node of {} is reachable", ask.kind.as_str(), g.title())),
            }
        }
    }
}

/// `mnnn review comment`.
pub fn review_comment(
    review: String,
    text: String,
    field: Option<String>,
    at: Option<String>,
    nth: i64,
    entry: Option<String>,
    reply: Option<String>,
) -> anyhow::Result<String> {
    add(
        &open_write()?,
        review.trim(),
        Ask {
            kind: ReviewItemKind::Comment,
            body: text,
            field: field.unwrap_or_default(),
            at,
            nth: nth.max(0) as usize,
            target: entry.unwrap_or_default(),
            reply_to: reply,
            ..Default::default()
        },
    )
}

/// `mnnn review suggest`.
pub fn review_suggest(
    review: String,
    to: String,
    why: String,
    field: Option<String>,
    at: Option<String>,
    nth: i64,
    entry: Option<String>,
) -> anyhow::Result<String> {
    add(
        &open_write()?,
        review.trim(),
        Ask {
            kind: ReviewItemKind::Suggestion,
            value: Some(Value::String(to)),
            body: why,
            field: field.unwrap_or_default(),
            at,
            nth: nth.max(0) as usize,
            target: entry.unwrap_or_default(),
            ..Default::default()
        },
    )
}

/// `mnnn review highlight`.
pub fn review_highlight(
    review: String,
    field: Option<String>,
    at: Option<String>,
    nth: i64,
    entry: Option<String>,
    color: Option<String>,
    note: String,
) -> anyhow::Result<String> {
    add(
        &open_write()?,
        review.trim(),
        Ask {
            kind: ReviewItemKind::Highlight,
            value: color.filter(|s| !s.trim().is_empty()).map(Value::String),
            body: note,
            field: field.unwrap_or_default(),
            at,
            nth: nth.max(0) as usize,
            target: entry.unwrap_or_default(),
            ..Default::default()
        },
    )
}

/// `mnnn review vote` (`approve`, `changes`, `reject`, or `up`/`down` with [on]).
pub fn review_vote(review: String, vote: String, on: Option<String>, note: String) -> anyhow::Result<String> {
    add(
        &open_write()?,
        review.trim(),
        Ask { kind: ReviewItemKind::Vote, value: Some(Value::String(vote)), reply_to: on, body: note, ..Default::default() },
    )
}

/// `mnnn review withdraw`.
pub fn review_withdraw(review: String, item: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let it = match target(&store, review.trim())? {
        Target::Mine(r) => review::withdraw(&store, ORIGIN, &r, &Who::Author, item.trim())?,
        Target::Joined(g) => {
            let Some(node) = hosted_node() else {
                return Err(anyhow::anyhow!("start the node (Net ▸ Start) to withdraw on a joined review"));
            };
            rnet::withdraw(&store, node.as_ref(), &g, item.trim(), &relays(), ORIGIN)?
        }
    };
    Ok(format!("withdrew {}", it.short_id()))
}

fn decide(review: String, items: Vec<String>, all: bool, accept: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = mine(&store, review.trim())?;
    let keys: Vec<String> = if all { vec![] } else { items.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect() };
    if keys.is_empty() && !all {
        return Err(anyhow::anyhow!("pick suggestion items, or accept/reject all"));
    }
    let out = review::decide(&store, ORIGIN, &r, &keys, accept)?;
    let verb = if accept { "accepted" } else { "rejected" };
    Ok(format!("{verb} {} suggestion(s) in {}", out.len(), r.title()))
}

/// `mnnn review accept`.
pub fn review_accept(review: String, items: Vec<String>, all: bool) -> anyhow::Result<String> {
    decide(review, items, all, true)
}

/// `mnnn review reject`.
pub fn review_reject(review: String, items: Vec<String>, all: bool) -> anyhow::Result<String> {
    decide(review, items, all, false)
}

/// `mnnn review merge`.
pub fn review_merge(review: String, force: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = mine(&store, review.trim())?;
    let m = review::merge(&store, ORIGIN, &r, force)?;
    if m.merged.is_empty() && m.conflicts.is_empty() {
        return Ok("nothing to merge: accept suggestions first".into());
    }
    let mut parts: Vec<String> = m.revisions.iter().map(|v| format!("{} rev {}", v.kind.label(), v.rev)).collect();
    if !m.conflicts.is_empty() {
        parts.push(format!("{} left (merge with force to apply them anyway)", m.conflicts.len()));
    }
    Ok(format!("merged {}: {}", m.merged.len(), parts.join(", ")))
}

/// `mnnn review close` / TUI `o`. Ends reviewer links; the node key stays in Rust.
pub fn review_close(review: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = mine(&store, review.trim())?;
    let node = hosted_node();
    let r = rnet::close(&store, node.as_deref(), &r, &relays(), ORIGIN)?;
    Ok(format!("closed {}", r.title()))
}

/// `mnnn review reopen`.
pub fn review_reopen(review: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = mine(&store, review.trim())?;
    let r = review::reopen(&store, ORIGIN, &r)?;
    Ok(format!("opened {} again; make new links with invite", r.title()))
}

/// `mnnn review drop`.
pub fn review_drop(review: String, reviewer: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let r = mine(&store, review.trim())?;
    let v = store.find_reviewer(&r.id, reviewer.trim())?;
    let node = hosted_node();
    let v = rnet::drop_reviewer(&store, node.as_deref(), &r, &v, &relays(), ORIGIN)?;
    Ok(format!("took {} off {}", v.title(), r.title()))
}

/// `mnnn review leave`.
pub fn review_leave(review: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let g = store.find_reviewing(review.trim())?;
    let node = hosted_node();
    let g = rnet::leave(&store, node.as_deref(), &g, &relays(), ORIGIN)?;
    Ok(format!("left {}", g.title()))
}

/// `mnnn review remove` (mine) or forget a left/closed joined review.
pub fn review_remove(review: String) -> anyhow::Result<String> {
    let store = open_write()?;
    match target(&store, review.trim())? {
        Target::Mine(r) => {
            let title = r.title();
            review::remove(&store, ORIGIN, &r)?;
            Ok(format!("removed {title}"))
        }
        Target::Joined(g) => {
            if g.status == ReviewingStatus::Joined {
                return Err(anyhow::anyhow!("you are still reviewing {}; leave it first", g.title()));
            }
            store.forget_reviewing(&g.review_id)?;
            Ok(format!("forgot {}", g.title()))
        }
    }
}

/// `mnnn review revisions`.
pub fn review_revisions(kind: String, target: String) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let vs = review::revisions(&store, parse_kind(&kind)?, target.trim())?;
    if vs.is_empty() {
        return Ok(vec!["no revisions yet".into()]);
    }
    Ok(vs
        .into_iter()
        .map(|v| {
            format!(
                "rev {} of {} {} ({})",
                v.rev,
                v.kind.label(),
                muninn_core::store::projects::short_of(&v.ref_id),
                muninn_core::ingest::time::fmt_ts(v.created_at)
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn reviews_request_comment_without_net() {
        let s = Store::open_in_memory().unwrap();
        let j = s.create_journal("notes", None, Default::default()).unwrap();
        assert!(reviews_of(&s).unwrap().is_empty());
        let r = review::request(&s, "test", ReviewKind::Journal, &j.id, "look over notes", "before Friday", &[]).unwrap();
        let list = reviews_of(&s).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "notes");
        assert_eq!(list[0].status, "open");
        review::add(&s, "test", &r, &Who::Author, &Ask { kind: ReviewItemKind::Comment, body: "looks good".into(), ..Default::default() }).unwrap();
        let items = s.review_items(&r.id).unwrap();
        assert_eq!(items.len(), 1);
        assert!(review::line(&items[0]).contains("looks good"));
        reviewing_of(&s).unwrap();
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(reviews().is_err());
        assert!(reviews_joined().is_err());
        assert!(review_request("journal".into(), "notes".into(), "".into(), "".into(), vec![]).is_err());
    }
}
