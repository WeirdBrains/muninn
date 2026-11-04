//! Reviews across nodes. A review link is an offer scoped to the review (`kind: resource`,
//! `as: review`, the 2.6 reserved scope);
//! a reviewer who takes it answers, which makes a key exchange with the author's node.
//! The reviewer then reaches the review through sealed [`ReviewReq`]s over that exchange.
use super::{Ask, Who};
use crate::error::{Error, Result};
use crate::net::client;
use crate::net::exchange::{self, Token};
use crate::net::node::Node;
use crate::net::proto::{Req, Resp};
use crate::store::{Exchange, ExchangeStatus, Review, ReviewItem, Reviewer, ReviewerStatus, Reviewing, ReviewingStatus, Store, now_ms};
use libp2p::PeerId;
use libp2p::identity::Keypair;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a reviewer asks the author's node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewReq {
    /// The review, what reviewers may do, the vote tally.
    Status,
    /// The thing under review.
    View,
    /// Every comment, suggestion, highlight and vote.
    Thread,
    Add {
        ask: Ask,
    },
    Withdraw {
        item: String,
    },
    Leave,
}

/// The scope of a review link.
pub fn scope(s: &Store, r: &Review) -> Value {
    json!({ "kind": "resource", "as": "review", "review": r.id, "title": r.title(), "resource": r.kind, "name": r.name, "allow": r.allow,
            "author": super::card(s, r)["author"] })
}

fn scope_of(v: &Value) -> Option<String> {
    if v.get("kind")?.as_str()? != "resource" || v.get("as")?.as_str()? != "review" {
        return None;
    }
    Some(v.get("review")?.as_str()?.to_string())
}

fn sv(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// How we introduce ourselves to a review: the user's name when it has one (2.10),
/// otherwise the node's.
fn me(s: &Store, node: Option<&Node>) -> (String, Vec<String>) {
    let name = s.users().ok().and_then(|u| u.into_iter().find(|u| u.mine)).map(|u| u.label).filter(|l| !l.trim().is_empty());
    (name.unwrap_or_else(|| crate::net::local::my_label(s, &crate::prefs::NetPrefs::default())), client::my_addrs(s, node))
}

/// A reviewer's name from their user identity, once their node has introduced it.
fn user_name(s: &Store, peer: &str) -> Option<String> {
    crate::net::user::user_of_peer(s, peer).ok().flatten().filter(|u| !u.mine).map(|u| u.label).filter(|l| !l.trim().is_empty())
}

// ---------- the author's node ----------

/// A link for one reviewer: an offer scoped to the review (delivered when `to` names a
/// reachable node). Each link is good for one reviewer and expires in a week.
#[allow(clippy::too_many_arguments)]
pub fn invite(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    r: &Review,
    label: &str,
    to: Option<&str>,
    relays: &[String],
    origin: &str,
) -> Result<(Reviewer, client::Created)> {
    if r.status != crate::store::ReviewStatus::Open {
        return Err(Error::invalid("review", format!("{} is closed (`mnnn review reopen` it first)", r.title())));
    }
    let (my_label, addrs) = me(s, node);
    let c = client::create_scoped(
        s,
        node,
        kp,
        &if label.is_empty() { format!("review {}", r.title()) } else { label.to_string() },
        &my_label,
        addrs,
        to,
        client::DEFAULT_TTL_MS * 7,
        relays,
        scope(s, r),
        origin,
    )?;
    let peer = c.exchange.peer_id.clone().unwrap_or_else(|| format!("invite:{}", c.exchange.id));
    let v = s.add_reviewer(&r.id, &peer, label, Some(&c.exchange.id), ReviewerStatus::Invited)?;
    super::record(
        s,
        origin,
        "review.invite",
        format!("review link for {} made", r.title()),
        json!({ "review": r.id, "reviewer": v.id, "exchange": c.exchange.id, "label": label }),
    );
    Ok((v, c))
}

/// An exchange completed (either side): settle the reviewer it is for.
pub fn answered(s: &Store, x: &Exchange) -> Result<()> {
    let Some(review) = scope_of(&x.scope) else { return Ok(()) };
    let peer = x.peer_id.clone().unwrap_or_default();
    if x.role == "offerer" {
        if let Some(v) = s.reviewer_by_exchange(&x.id)?
            && v.status == ReviewerStatus::Invited
        {
            let label = if !v.label.is_empty() { v.label.clone() } else { user_name(s, &peer).unwrap_or_else(|| x.peer_label.clone()) };
            let v = s.set_reviewer(&v.id, Some(ReviewerStatus::Joined), Some(&peer), Some(&label))?;
            let title = s.review(&v.review_id)?.map(|r| r.title()).unwrap_or_default();
            super::record(
                s,
                "network",
                "review.join",
                format!("{} joined the review of {title}", v.title()),
                json!({ "review": v.review_id, "reviewer": v.id, "peer": peer }),
            );
        }
        return Ok(());
    }
    let now = now_ms();
    let g = Reviewing {
        review_id: review,
        title: sv(&x.scope, "title"),
        kind: sv(&x.scope, "resource"),
        name: sv(&x.scope, "name"),
        node_peer: peer,
        exchange_id: Some(x.id.clone()),
        status: ReviewingStatus::Joined,
        allow: x.scope.get("allow").and_then(|a| serde_json::from_value(a.clone()).ok()).unwrap_or_default(),
        info: Value::Null,
        created_at: now,
        updated_at: now,
        copy: Value::Null,
        thread: json!([]),
        copied_at: 0,
    };
    s.save_reviewing(&g)?;
    super::record(s, "network", "review.joined", format!("joined the review of {}", g.title()), json!({ "review": g.review_id, "exchange": x.id }));
    Ok(())
}

/// The other side ended an exchange: a reviewer left, or the author closed the review.
pub fn ended(s: &Store, x: &Exchange) -> Result<()> {
    if let Some(v) = s.reviewer_by_exchange(&x.id)?
        && matches!(v.status, ReviewerStatus::Joined | ReviewerStatus::Invited)
    {
        let v = s.set_reviewer(&v.id, Some(ReviewerStatus::Left), None, None)?;
        super::record(s, "network", "review.leave", format!("{} left the review", v.title()), json!({ "review": v.review_id, "reviewer": v.id }));
    }
    if let Some(mut g) = s.reviewing_by_exchange(&x.id)?
        && g.status == ReviewingStatus::Joined
    {
        g.status = ReviewingStatus::Closed;
        g.updated_at = now_ms();
        s.save_reviewing(&g)?;
        super::record(s, "network", "review.closed", format!("the review of {} ended", g.title()), json!({ "review": g.review_id }));
    }
    Ok(())
}

/// End one reviewer's part: the exchange ends (their node is told when it can be reached).
pub fn drop_reviewer(s: &Store, node: Option<&Node>, r: &Review, v: &Reviewer, relays: &[String], origin: &str) -> Result<Reviewer> {
    let v = s.set_reviewer(&v.id, Some(ReviewerStatus::Removed), None, None)?;
    if let Some(x) = &v.exchange_id {
        let _ = client::revoke(s, node, x, relays, origin);
    }
    super::record(
        s,
        origin,
        "review.drop",
        format!("{} taken off the review of {}", v.title(), r.title()),
        json!({ "review": r.id, "reviewer": v.id }),
    );
    Ok(v)
}

/// Close a review and end every reviewer's exchange.
pub fn close(s: &Store, node: Option<&Node>, r: &Review, relays: &[String], origin: &str) -> Result<Review> {
    let r = super::close_local(s, origin, r)?;
    for v in s.reviewers(&r.id)? {
        if matches!(v.status, ReviewerStatus::Joined | ReviewerStatus::Invited) {
            let _ = s.set_reviewer(&v.id, Some(ReviewerStatus::Removed), None, None);
            if let Some(x) = &v.exchange_id {
                let _ = client::revoke(s, node, x, relays, origin);
            }
        }
    }
    Ok(r)
}

fn body(v: Value) -> Resp {
    Resp::Review { body: v }
}

/// A sealed request from a reviewer.
pub fn serve(s: &Store, x: &Exchange, from: &PeerId, review: &str, req: ReviewReq) -> Result<Resp> {
    let r = s.review(review)?.ok_or_else(|| Error::invalid("review", "no such review on this node"))?;
    let v = s.reviewer_by_exchange(&x.id)?.filter(|v| v.review_id == r.id && v.peer_id == from.to_string());
    let v = v.ok_or_else(|| Error::invalid("review", format!("this key exchange is not a link to {}", r.title())))?;
    if !matches!(req, ReviewReq::Status) && v.status != ReviewerStatus::Joined {
        return Err(Error::invalid("review", format!("you are {} in {}", v.status.as_str(), r.title())));
    }
    let who = Who::Reviewer(Box::new(v.clone()));
    Ok(match req {
        ReviewReq::Status => {
            let items = s.review_items(&r.id)?;
            let (a, c, j) = super::tally(&items);
            body(json!({ "review": super::card(s, &r), "reviewer": v.id, "status": v.status, "items": items.len(),
                         "votes": { "approve": a, "changes": c, "reject": j } }))
        }
        ReviewReq::View => {
            let out = super::view(s, &r)?;
            super::record(s, "network", "review.serve", format!("sent {} to {}", r.title(), v.title()), json!({ "review": r.id, "reviewer": v.id }));
            body(out)
        }
        ReviewReq::Thread => body(json!({ "items": super::thread(s, &r)? })),
        ReviewReq::Add { ask } => body(json!({ "item": super::add(s, "network", &r, &who, &ask)? })),
        ReviewReq::Withdraw { item } => body(json!({ "item": super::withdraw(s, "network", &r, &who, &item)? })),
        ReviewReq::Leave => {
            let v = s.set_reviewer(&v.id, Some(ReviewerStatus::Left), None, None)?;
            let _ = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("peer"));
            super::record(
                s,
                "network",
                "review.leave",
                format!("{} left the review of {}", v.title(), r.title()),
                json!({ "review": r.id, "reviewer": v.id }),
            );
            body(json!({ "left": r.id }))
        }
    })
}

// ---------- a reviewer's node ----------

/// The review a link is for (before taking it).
pub fn peek(token: &str) -> Result<Value> {
    let t = token.split_whitespace().collect::<String>();
    let Token::Offer(o) = exchange::parse_token(&t)? else {
        return Err(Error::invalid("link", "this is an answer, not a review link"));
    };
    if scope_of(&o.scope).is_none() {
        return Err(Error::invalid("link", "not a review link (use `mnnn net exchange accept` or `mnnn org join`)"));
    }
    Ok(o.scope.clone())
}

/// Take a review link: answer it (delivered to the author's node when `node` runs).
pub fn join(s: &Store, node: Option<&Node>, kp: &Keypair, token: &str, label: &str, origin: &str) -> Result<(Reviewing, client::Accepted)> {
    let sc = peek(token)?;
    let review = scope_of(&sc).unwrap_or_default();
    if let Some(g) = s.reviewing()?.into_iter().find(|g| g.review_id == review)
        && g.status == ReviewingStatus::Joined
    {
        return Err(Error::invalid("review", format!("already reviewing {}", g.title())));
    }
    let (my_label, addrs) = me(s, node);
    let acc = client::accept(s, node, kp, token, label, &my_label, addrs, origin)?;
    let g = s.reviewing_by_exchange(&acc.exchange.id)?.ok_or_else(|| Error::Other("the review link did not settle".into()))?;
    if let Some(Err(e)) = &acc.delivered {
        // the author's node turned the answer down (a link already taken, a closed review)
        let _ = s.set_exchange_status(&acc.exchange.id, ExchangeStatus::Revoked, Some("me"));
        s.forget_reviewing(&g.review_id)?;
        return Err(Error::Other(format!("{}: {e}", g.title())));
    }
    Ok((g, acc))
}

/// A sealed request to the author's node.
pub fn request(s: &Store, node: &Node, g: &Reviewing, req: ReviewReq, relays: &[String]) -> Result<Value> {
    let xid = g.exchange_id.clone().ok_or_else(|| Error::invalid("review", "no key exchange"))?;
    let x = s.find_exchange(&xid)?;
    match client::send(s, node, &x, Req::Review { review: g.review_id.clone(), req }, relays)? {
        Resp::Review { body } => Ok(body),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// Bring a review up to date (title, what we may do, open or closed).
pub fn sync(s: &Store, node: &Node, g: &Reviewing, relays: &[String]) -> Result<Reviewing> {
    let v = request(s, node, g, ReviewReq::Status, relays)?;
    let mut g = g.clone();
    if let Some(r) = v.get("review") {
        g.title = sv(r, "title");
        g.kind = sv(r, "kind");
        g.name = sv(r, "name");
        g.allow = r.get("allow").and_then(|a| serde_json::from_value(a.clone()).ok()).unwrap_or(g.allow);
        if sv(r, "status") == "closed" {
            g.status = ReviewingStatus::Closed;
        }
    }
    g.info = v;
    g.updated_at = now_ms();
    s.save_reviewing(&g)?;
    Ok(g)
}

pub fn view(s: &Store, node: &Node, g: &Reviewing, relays: &[String]) -> Result<Value> {
    request(s, node, g, ReviewReq::View, relays)
}

pub fn thread(s: &Store, node: &Node, g: &Reviewing, relays: &[String]) -> Result<Vec<ReviewItem>> {
    let v = request(s, node, g, ReviewReq::Thread, relays)?;
    serde_json::from_value(v.get("items").cloned().unwrap_or_default()).map_err(|e| Error::Other(format!("peer: bad thread ({e})")))
}

pub fn add(s: &Store, node: &Node, g: &Reviewing, ask: &Ask, relays: &[String], origin: &str) -> Result<ReviewItem> {
    let v = request(s, node, g, ReviewReq::Add { ask: ask.clone() }, relays)?;
    let it: ReviewItem = serde_json::from_value(v.get("item").cloned().unwrap_or_default()).map_err(|e| Error::Other(format!("peer: {e}")))?;
    super::record(
        s,
        origin,
        &format!("review.{}", if it.kind == crate::store::ReviewItemKind::Suggestion { "suggest" } else { it.kind.as_str() }),
        format!("{} sent to the review of {}", it.kind.as_str(), g.title()),
        json!({ "review": g.review_id, "item": it.id, "kind": it.kind, "field": it.field }),
    );
    Ok(it)
}

pub fn withdraw(s: &Store, node: &Node, g: &Reviewing, item: &str, relays: &[String], origin: &str) -> Result<ReviewItem> {
    let v = request(s, node, g, ReviewReq::Withdraw { item: item.to_string() }, relays)?;
    let it: ReviewItem = serde_json::from_value(v.get("item").cloned().unwrap_or_default()).map_err(|e| Error::Other(format!("peer: {e}")))?;
    super::record(
        s,
        origin,
        "review.withdraw",
        format!("withdrew {} from {}", it.short_id(), g.title()),
        json!({ "review": g.review_id, "item": it.id }),
    );
    Ok(it)
}

/// Leave a review (the author's node is told; the exchange ends here either way).
pub fn leave(s: &Store, node: Option<&Node>, g: &Reviewing, relays: &[String], origin: &str) -> Result<Reviewing> {
    let told = match node {
        Some(n) if g.status == ReviewingStatus::Joined => request(s, n, g, ReviewReq::Leave, relays).map(|_| ()).map_err(|e| e.to_string()),
        _ => Err("not told".into()),
    };
    if let Some(x) = &g.exchange_id {
        let _ = s.set_exchange_status(x, ExchangeStatus::Revoked, Some("me"));
    }
    let mut g = g.clone();
    g.status = ReviewingStatus::Left;
    g.updated_at = now_ms();
    s.save_reviewing(&g)?;
    super::record(s, origin, "review.leave", format!("left the review of {}", g.title()), json!({ "review": g.review_id, "told": told.is_ok() }));
    Ok(g)
}

// ---------- review copies (2.14) ----------

/// What became of a piece of review work.
#[derive(Debug)]
pub enum Sent {
    /// The author's node took it.
    Item(Box<ReviewItem>),
    /// Kept here until the author's node can be reached (the id of the queued work).
    Queued(String),
}

/// How often a running node refreshes the copies of reviews it takes part in.
pub const REFRESH_EVERY_MS: i64 = 5 * 60 * 1000;

/// A copy is read-only once the review is closed, dropped or left.
fn check_open(g: &Reviewing) -> Result<()> {
    if g.status != ReviewingStatus::Joined {
        return Err(Error::invalid("review", format!("{} is {}; its copy here is archived and read-only", g.title(), g.status.as_str())));
    }
    Ok(())
}

/// The author's node could not be reached (as opposed to it answering no).
fn unreachable(e: &Error) -> bool {
    e.to_string().starts_with("network: ")
}

/// Comment, suggest, highlight or vote: sent when the author's node answers, kept here otherwise.
pub fn send_or_queue(s: &Store, node: Option<&Node>, g: &Reviewing, ask: &Ask, relays: &[String], origin: &str) -> Result<Sent> {
    check_open(g)?;
    let perm = super::perm_of(ask.kind);
    if !g.allow.iter().any(|a| a == perm) {
        return Err(Error::invalid("review", format!("reviewers may not {perm} in {}", g.title())));
    }
    // the author's node decides when it answers (its refusals are final); unreachable, it waits here
    if let Some(n) = node {
        match add(s, n, g, ask, relays, origin) {
            Ok(it) => return Ok(Sent::Item(Box::new(it))),
            Err(e) if !unreachable(&e) => return Err(e),
            Err(_) => {}
        }
    }
    let id = s.queue_review_work(&g.review_id, &serde_json::to_value(ask)?)?;
    super::record(
        s,
        origin,
        "review.queue",
        format!("{} kept for the review of {} until its node is reachable", ask.kind.as_str(), g.title()),
        json!({ "review": g.review_id, "work": id, "kind": ask.kind }),
    );
    Ok(Sent::Queued(id))
}

/// Make or extend the model a record review needs, additively (new fields only).
pub fn ensure_model(s: &Store, g: &Reviewing, model: &Value) -> Result<Option<String>> {
    let name = sv(model, "name");
    if name.is_empty() {
        return Ok(None);
    }
    let fields: Vec<crate::store::Field> = serde_json::from_value(model.get("fields").cloned().unwrap_or_default()).unwrap_or_default();
    let known: Vec<String> = s.models()?.into_iter().map(|m| m.id).collect();
    // fresh ids, no constraints the copied values could break; refs to the author's other models become text
    let fresh = |f: &crate::store::Field| {
        let ty = match &f.ty {
            crate::store::FieldType::Ref(m) if m != "bars" && !known.contains(m) => crate::store::FieldType::Text,
            t => t.clone(),
        };
        crate::store::Field { id: crate::store::models::new_field_id(), ty, required: false, unique: false, ..f.clone() }
    };
    let params = |added: &[String]| json!({ "review": g.review_id, "model": name, "fields": added });
    match s.models()?.into_iter().find(|m| m.name == name) {
        None => {
            let d = crate::store::Design { fields: fields.iter().map(fresh).collect(), ..Default::default() };
            s.create_model(&name, &sv(model, "description"), d, true)?;
            let added: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            super::record(s, "review", "review.model", format!("model {name} made for the review of {}", g.title()), params(&added));
            Ok(Some(format!("made model {name}")))
        }
        Some(m) => {
            let missing: Vec<_> = fields.iter().filter(|f| m.design.field(&f.name).is_none()).collect();
            if missing.is_empty() {
                return Ok(None);
            }
            let mut d = m.design.clone();
            d.fields.extend(missing.iter().map(|f| fresh(f)));
            s.set_model_draft(&m.id, Some(d))?;
            s.apply_model(&m.id, &crate::store::ApplyOpts { destructive: false, note: format!("fields for the review of {}", g.title()) })?;
            let added: Vec<String> = missing.iter().map(|f| f.name.clone()).collect();
            super::record(
                s,
                "review",
                "review.model",
                format!("model {name} gained {} for the review of {}", added.join(", "), g.title()),
                params(&added),
            );
            Ok(Some(format!("added {} to model {name}", added.join(", "))))
        }
    }
}

/// What a refresh did.
#[derive(Debug, Default)]
pub struct Refreshed {
    pub sent: usize,
    /// Work the author's node turned down (dropped here), with why.
    pub refused: Vec<String>,
    /// The copy changed (a new revision, or the first copy).
    pub changed: bool,
    pub model: Option<String>,
}

/// Store what the author's node sent as this node's copy.
pub fn keep_copy(s: &Store, g: &mut Reviewing, view: Value, thread: Value) -> Result<Refreshed> {
    let mut out =
        Refreshed { changed: g.copy.get("content") != view.get("content") || g.copy.get("entries") != view.get("entries"), ..Default::default() };
    if let Some(m) = view.get("model") {
        out.model = ensure_model(s, g, m)?;
    }
    if out.changed && !g.copy.is_null() {
        super::record(s, "review", "review.refresh", format!("the copy of {} has a new revision", g.title()), json!({ "review": g.review_id }));
    }
    g.copy = view;
    g.thread = thread;
    g.copied_at = now_ms();
    g.updated_at = g.copied_at;
    s.save_reviewing(g)?;
    Ok(out)
}

/// Send waiting work, then bring the review and its copy up to date.
pub fn refresh(s: &Store, node: &Node, g: &Reviewing, relays: &[String], origin: &str) -> Result<(Reviewing, Refreshed)> {
    let mut g = sync(s, node, g, relays)?;
    let mut sent = 0;
    let mut refused = vec![];
    if g.status == ReviewingStatus::Joined {
        for (id, ask) in s.review_work(&g.review_id)? {
            let ask: Ask = match serde_json::from_value(ask) {
                Ok(a) => a,
                Err(_) => {
                    s.unqueue_review_work(&id)?;
                    continue;
                }
            };
            match add(s, node, &g, &ask, relays, origin) {
                Ok(_) => sent += 1,
                Err(e) if unreachable(&e) => return Err(e),
                Err(e) => refused.push(e.to_string()),
            }
            s.unqueue_review_work(&id)?;
        }
    }
    let view = self::view(s, node, &g, relays)?;
    let th = request(s, node, &g, ReviewReq::Thread, relays)?.get("items").cloned().unwrap_or(json!([]));
    let mut r = keep_copy(s, &mut g, view, th)?;
    r.sent = sent;
    r.refused = refused;
    Ok((g, r))
}

/// For a running node: refresh joined reviews whose turn it is, or that have work waiting.
pub fn tick(s: &Store, node: &Node, relays: &[String], origin: &str) -> Vec<String> {
    let now = now_ms();
    let mut out = vec![];
    for g in s.reviewing().unwrap_or_default().into_iter().filter(|g| g.status == ReviewingStatus::Joined) {
        let waiting = s.review_work(&g.review_id).map(|w| w.len()).unwrap_or(0);
        if waiting == 0 && now - g.copied_at < REFRESH_EVERY_MS {
            continue;
        }
        if let Ok((g, r)) = refresh(s, node, &g, relays, origin)
            && (r.sent > 0 || r.changed)
        {
            out.push(format!("review {}: {} sent, copy {}", g.title(), r.sent, if r.changed { "refreshed" } else { "current" }));
        }
    }
    out
}
