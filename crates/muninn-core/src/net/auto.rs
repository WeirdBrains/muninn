//! Automatic key exchanges (2.9): the nodes run the exchange themselves when their owners
//! act (connect to a peer, apply to or join an org, invite, take a review link, share), and
//! the owners see only the action and where it stands: [`LinkStatus`].
//!
//! The cryptography is the one in [`super::exchange`]; what changes is who carries it.
//! An offer goes to the peer over libp2p (`Wire::Connect`), signed by the node that sends
//! it, and the answer comes back in the reply, or later (`Wire::Poll` from the offerer, or
//! the answerer re-delivering it) when the other side has to decide first.
//!
//! **Identity without comparing codes.** A node *is* its Ed25519 key: the peer id in an
//! address (`…/p2p/12D3KooW…`) or invite is a hash of it, libp2p's Noise handshake proves
//! the far end holds it, and offers and answers are accepted only from the node that signed
//! them. So an exchange carried this way can only be made with the node the address or
//! invite names. The first successful exchange pins that key to the peer's name (trust on
//! first use): names resolve to the pinned node, and another node using a known name is
//! flagged ([`impostor`]) instead of being taken for it. The short code is still derived
//! and can be compared with `mnnn net exchange verify` (advanced) for extra assurance.
use super::client::{self, Accepted};
use super::exchange::{self, Token};
use super::node::Node;
use super::profile::MUNINN;
use super::proto::WireResp;
use crate::error::{Error, Result};
use crate::store::{Exchange, ExchangeStatus, NewAction, Store, now_ms};
use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use weirdnet_net::invite::{self, Said};

pub use weirdnet_net::invite::LinkStatus;

/// The status of an exchange, as the user sees it.
pub fn status_of(x: &Exchange) -> LinkStatus {
    invite::status_of(x)
}

/// What an exchange is for, in a word: `node`, `org NAME`, `review TITLE`.
pub fn purpose(x: &Exchange) -> String {
    let s = |k: &str| x.scope.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match s("kind").as_str() {
        "organization" => format!("org {}", s("name")),
        "resource" => format!("review {}", s("title")),
        "link" => "link node".into(),
        _ => "node".into(),
    }
}

/// The result of an action that needed a key exchange (see `weirdnet_net::invite::Link`).
pub type Link = invite::Link<Exchange>;

fn record(s: &Store, origin: &str, kind: &str, summary: String, params: serde_json::Value) {
    s.note_action(&NewAction::new(kind, origin, summary).params(params));
}

/// Whether a key exchange with this node was ever completed (and not ended by us): its key
/// is pinned here.
pub fn known(s: &Store, peer: &str) -> bool {
    invite::known(s, peer)
}

/// A warning when `peer` calls itself by the name of another node whose key is pinned here.
pub fn impostor(s: &Store, peer: &str, label: &str) -> Option<String> {
    invite::impostor(s, peer, label)
}

/// A peer by address, id, or name; a name prefers the node whose key is pinned.
pub fn resolve(s: &Store, target: &str, relays: &[String]) -> Result<(PeerId, Vec<Multiaddr>)> {
    invite::resolve(&MUNINN, s, target, relays)
}

/// The node-scoped exchange we have with a peer: active first, else pending either way.
pub fn node_link(s: &Store, peer: &str) -> Result<Option<Exchange>> {
    kind_link(s, peer, "node")
}

/// The exchange of one scope kind (`node`, `link`) we have with a peer.
pub fn kind_link(s: &Store, peer: &str, kind: &str) -> Result<Option<Exchange>> {
    invite::kind_link(s, peer, kind)
}

/// The exchange to share over for a key (an exchange, or a peer): active, or still pending
/// (shares made now reach the peer once it is connected).
pub fn link_for(s: &Store, key: &str) -> Result<Exchange> {
    if let Ok(x) = s.find_exchange(key) {
        return Ok(x);
    }
    let p = s.find_net_peer(key)?;
    node_link(s, &p.peer_id)?.ok_or_else(|| {
        Error::invalid("peer", format!("not connected to {} (`mnnn net connect` it first)", if p.label.is_empty() { p.short_id() } else { &p.label }))
    })
}

/// What delivering an offer gave.
pub type Delivered = invite::Delivered<Exchange>;

/// Carry one of our offers to its peer: `Connect` (2.9), falling back to `Offer` for an
/// older node. An answer that comes back completes the exchange here.
pub fn deliver(s: &Store, node: &Node, peer: PeerId, addrs: &[Multiaddr], token: &str) -> std::result::Result<Delivered, String> {
    invite::deliver(&MUNINN, s, node, peer, addrs, token)
}

/// Network errors in plain words.
pub fn plain(e: &str) -> String {
    invite::plain(e)
}

/// Connect to a node: say hello, then make a key exchange with it unless there is one.
/// The other node answers on its own when it knows us (or accepts anyone); otherwise its
/// owner accepts the request and the answer comes back later ([`settle`]).
#[allow(clippy::too_many_arguments)]
pub fn connect(
    s: &Store,
    node: &Node,
    kp: &Keypair,
    target: &str,
    label: Option<&str>,
    my_label: &str,
    addrs: Vec<String>,
    relays: &[String],
    origin: &str,
) -> Result<Link> {
    let l = connect_as(s, node, kp, target, label, my_label, addrs, relays, origin, "node")?;
    if l.status == LinkStatus::Connected
        && let Some(x) = &l.exchange
    {
        let _ = introduce(s, node, x, relays);
    }
    Ok(l)
}

#[allow(clippy::too_many_arguments)]
fn connect_as(
    s: &Store,
    node: &Node,
    kp: &Keypair,
    target: &str,
    label: Option<&str>,
    my_label: &str,
    addrs: Vec<String>,
    relays: &[String],
    origin: &str,
    kind: &str,
) -> Result<Link> {
    // an invite link names the node; for a node link we only need where it is
    let target = match (kind, exchange::parse_token(target.trim())) {
        ("link", Ok(Token::Offer(o))) => {
            for a in &o.addrs {
                if let Ok(m) = a.parse::<Multiaddr>() {
                    let _ = s.upsert_net_peer(
                        &o.peer,
                        Some(&o.label).filter(|l| !l.is_empty()).map(|x| x.as_str()),
                        &[m.to_string()],
                        None,
                        None,
                        "manual",
                    );
                }
            }
            o.peer.clone()
        }
        _ => target.to_string(),
    };
    let scope = if kind == "link" { exchange::link_scope() } else { exchange::node_scope() };
    invite::connect(&MUNINN, s, node, kp, &target, label, my_label, addrs, relays, origin, scope, client::DEFAULT_TTL_MS, |x, addrs| {
        accept(s, Some(node), kp, &x.id, my_label, addrs, origin)
    })
}

/// Make an offer for a known peer (stored with its peer id) without delivering it.
#[allow(clippy::too_many_arguments)]
pub fn offer_to(
    s: &Store,
    kp: &Keypair,
    peer: &str,
    scope: serde_json::Value,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    ttl_ms: i64,
    origin: &str,
) -> Result<Exchange> {
    invite::offer_to(&MUNINN, s, kp, peer, scope, label, my_label, addrs, ttl_ms, origin)
}

/// Accept a request (an incoming exchange, by id, peer name or peer id): answer it and
/// deliver the answer (or leave it for [`settle`] when the peer cannot be reached now).
pub fn accept(s: &Store, node: Option<&Node>, kp: &Keypair, key: &str, my_label: &str, addrs: Vec<String>, origin: &str) -> Result<Link> {
    let x = incoming(s, key)?;
    let a: Accepted = client::accept(s, node, kp, &x.id, "", my_label, addrs, origin)?;
    if exchange::scope_kind(&a.exchange.scope) == "link" {
        record(
            s,
            origin,
            "user.node.approve",
            format!("{} may link to your user", a.exchange.title()),
            serde_json::json!({ "exchange": a.exchange.id, "peer": a.exchange.peer_id }),
        );
    }
    let peer = a.exchange.title();
    let (status, detail) = match &a.delivered {
        Some(Ok(_)) => (LinkStatus::Connected, format!("connected to {peer}")),
        _ => (LinkStatus::Connected, format!("connected to {peer} (they are told when their node is reachable)")),
    };
    Ok(Link { status, peer, peer_id: a.exchange.peer_id.clone(), exchange: Some(a.exchange), detail, warning: None })
}

/// Use an invite link (`muninn://join/…`, or a pasted token): answer it and deliver the
/// answer. When their node cannot be reached now, it is delivered later ([`settle`]) and a
/// link that finishes it by hand is given too.
#[allow(clippy::too_many_arguments)]
pub fn use_link(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    link: &str,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    origin: &str,
) -> Result<Link> {
    let a: Accepted = client::accept(s, node, kp, link, label, my_label, addrs, origin)?;
    let peer = a.exchange.title();
    if a.exchange.status == ExchangeStatus::Revoked
        && let Some(Err(e)) = &a.delivered
    {
        return Err(Error::invalid("invite", format!("{peer} turned it down: {e}")));
    }
    let detail = match (&a.answer, &a.delivered) {
        (Some(t), d) if !d.as_ref().is_some_and(|d| d.is_ok()) => {
            format!("connected to {peer}; their node is told when it is reachable (or send them {})", exchange::link(t))
        }
        _ => format!("connected to {peer}"),
    };
    Ok(Link { status: status_of(&a.exchange), peer, peer_id: a.exchange.peer_id.clone(), exchange: Some(a.exchange), detail, warning: None })
}

/// Turn a request down; the other node learns it when it asks.
pub fn decline(s: &Store, key: &str, origin: &str) -> Result<Exchange> {
    let x = incoming(s, key)?;
    let x = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("me"))?;
    record(
        s,
        origin,
        "net.exchange.decline",
        format!("request from {} declined", x.title()),
        serde_json::json!({ "exchange": x.id, "peer": x.peer_id }),
    );
    Ok(x)
}

/// An incoming request by exchange id, peer id or name (newest first).
pub fn incoming(s: &Store, key: &str) -> Result<Exchange> {
    let reqs = requests(s)?;
    if let Ok(x) = s.find_exchange(key)
        && x.status == ExchangeStatus::Incoming
    {
        return Ok(x);
    }
    let k = key.trim();
    let peer = s.find_net_peer(k).ok().map(|p| p.peer_id);
    reqs.into_iter()
        .find(|x| peer.as_deref().is_some_and(|p| x.peer_id.as_deref() == Some(p)) || x.peer_label.eq_ignore_ascii_case(k))
        .ok_or_else(|| Error::invalid("request", format!("no pending request from `{k}` (see `mnnn net requests`)")))
}

/// Requests waiting for us, newest first (expired ones are left out).
pub fn requests(s: &Store) -> Result<Vec<Exchange>> {
    let now = now_ms();
    Ok(s.exchanges()?.into_iter().filter(|x| x.status == ExchangeStatus::Incoming && x.expires_at.is_none_or(|e| e > now)).collect())
}

// ---------- users over exchanges (2.10) ----------

/// How long to wait before telling a peer again whom we act for, after it did not take it.
const INTRO_RETRY_MS: i64 = 10 * 60 * 1000;

/// The setting remembering which card we last gave over an exchange.
pub fn intro_key(exchange: &str) -> String {
    format!("net.intro.{exchange}")
}

/// Tell the peer of an exchange whom we act for and learn whom they do (both cards
/// are checked). Remembers the card given, so [`settle`] only repeats it after a change.
pub fn introduce(s: &Store, node: &Node, x: &Exchange, relays: &[String]) -> Result<super::user::Met> {
    let card = super::user::my_card(s)?;
    let digest = card.digest();
    let r = client::send(s, node, x, super::proto::Req::Introduce { card }, relays);
    // given: not again until our card changes; not taken (an older node, or not
    // reachable now): again in a while
    let mark = if r.is_ok() { digest } else { format!("retry@{}", now_ms()) };
    s.set_setting(&intro_key(&x.id), Some(&mark))?;
    match r? {
        super::proto::Resp::Introduced { card } => super::user::take_card(s, x.peer_id.as_deref().unwrap_or(""), &card, "network"),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// Ask to act for the user of another node of mine (by address, peer, name or invite link).
/// The other node's owner approves (`mnnn user approve`, or accepts the request); then this
/// node takes the user's key ([`link_fetch`], also done by [`settle`]).
#[allow(clippy::too_many_arguments)]
pub fn link_to(
    s: &Store,
    node: &Node,
    kp: &Keypair,
    target: &str,
    my_label: &str,
    addrs: Vec<String>,
    relays: &[String],
    origin: &str,
) -> Result<Link> {
    let mut l = connect_as(s, node, kp, target, None, my_label, addrs, relays, origin, "link")?;
    if l.status == LinkStatus::Connected
        && let Some(x) = &l.exchange
    {
        match link_fetch(s, node, x, my_label, relays, origin) {
            Ok(u) => l.detail = format!("this node now acts for {} (linked through {})", u.name(), l.peer),
            Err(e) => {
                l.status = LinkStatus::Failed;
                l.detail = e.to_string();
            }
        }
    } else if l.status == LinkStatus::Pending {
        l.detail = format!("waiting for {} to approve the link (`mnnn user approve` there)", l.peer);
    }
    Ok(l)
}

/// Over a completed node-link exchange we offered: take the user's card and key.
pub fn link_fetch(s: &Store, node: &Node, x: &Exchange, my_label: &str, relays: &[String], origin: &str) -> Result<crate::store::users::User> {
    if exchange::scope_kind(&x.scope) != "link" || x.role != "offerer" {
        return Err(Error::invalid("link", "not a node link we asked for"));
    }
    match client::send(s, node, x, super::proto::Req::LinkFetch { label: my_label.to_string() }, relays)? {
        super::proto::Resp::Linked { card, keypair } => {
            use base64::Engine;
            let kp = zeroize::Zeroizing::new(
                base64::engine::general_purpose::STANDARD.decode(keypair.as_bytes()).map_err(|_| Error::Crypto("link: bad key".into()))?,
            );
            super::user::adopt(s, x.peer_id.as_deref().unwrap_or(""), &card, &kp, origin)
        }
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

// ---------- the receiving node ----------

/// `Wire::Connect`: keep the offer, and answer it on the spot when the scope is a plain
/// node connection and the peer is known here, has a request of ours open, or anyone is
/// accepted. Memberships and review links always wait for the owner.
pub fn connect_in(s: &Store, from: &PeerId, token: &str, my_label: &str, accept_anyone: bool) -> Result<WireResp> {
    let said = invite::connect_in(
        &MUNINN,
        s,
        from,
        token,
        my_label,
        || Ok((super::identity::load_or_create(s)?, client::my_addrs(s, None))),
        // only plain node connections, from a node known here, one we asked too, or anyone
        // when the owner said so
        |f| f.kind == "node" && (accept_anyone || f.known || f.crossing > 0 || f.already),
    )?;
    Ok(match said {
        Said::Answered(token) => WireResp::Answered { token },
        Said::Ok(msg) => WireResp::Ok { msg },
        _ => WireResp::Error { msg: "unexpected".into() },
    })
}

/// `Wire::Poll`: where one of the asker's offers stands here (only its offerer may ask).
pub fn poll_in(s: &Store, from: &PeerId, exchange: &str) -> Result<(String, Option<String>)> {
    invite::poll_in(s, from, exchange)
}

// ---------- keeping things moving ----------

/// Bring pending exchanges along, quietly: ask peers about our open requests (an answer
/// completes them), and deliver answers of ours that have not reached their offerer.
/// Returns one line per change. Unreachable peers are skipped until next time.
pub fn settle(s: &Store, node: &Node, relays: &[String], origin: &str) -> Vec<String> {
    let now = now_ms();
    let mut out = invite::settle_offers(&MUNINN, s, node, relays, origin, |x| {
        // org applications have their own bookkeeping
        let Ok(Some(j)) = s.joined_by_exchange(&x.id) else { return None };
        let mut v = vec![];
        if j.status == crate::store::MemberStatus::Applied
            && let Ok(j2) = crate::org::net::sync(s, node, &j, relays, origin)
            && j2.status != j.status
        {
            v.push(format!("{}: {}", j2.name, if j2.status == crate::store::MemberStatus::Member { "approved" } else { j2.status.as_str() }));
        }
        Some(v)
    });
    // 2.10: users. Finish node links we asked for, and tell peers whom we act for
    // (again after our card changed: a node linked or revoked).
    if let Ok(xs) = s.exchanges() {
        let label = String::new();
        for x in xs.iter().filter(|x| x.status.active()) {
            let kind = exchange::scope_kind(&x.scope);
            if kind == "link"
                && x.role == "offerer"
                && x.peer_id
                    .as_deref()
                    .is_some_and(|p| s.user_of_node(p).ok().flatten().is_none_or(|n| s.self_user().ok().flatten().is_none_or(|u| u.id != n.user_id)))
            {
                if let Ok(u) = link_fetch(s, node, x, &label, relays, origin) {
                    out.push(format!("linked: this node now acts for {}", u.name()));
                }
                continue;
            }
            if kind != "node" && kind != "link" {
                continue;
            }
            let Ok(card) = super::user::my_card(s) else { continue };
            let given = s.setting(&intro_key(&x.id)).ok().flatten().unwrap_or_default();
            if given == card.digest() || given.strip_prefix("retry@").and_then(|t| t.parse::<i64>().ok()).is_some_and(|t| now - t < INTRO_RETRY_MS) {
                continue;
            }
            let _ = introduce(s, node, x, relays);
        }
    }
    out.extend(invite::deliver_answers(&MUNINN, s, node, relays));
    out
}
