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

fn me(s: &Store, node: Option<&Node>) -> (String, Vec<String>) {
    (crate::net::local::my_label(s, &crate::prefs::NetPrefs::default()), client::my_addrs(s, node))
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
            let label = if v.label.is_empty() { x.peer_label.clone() } else { v.label.clone() };
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
