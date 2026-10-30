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
use super::proto::{Wire, WireResp};
use super::service;
use crate::error::{Error, Result};
use crate::store::{Exchange, ExchangeStatus, NewAction, NewExchange, Store, now_ms};
use libp2p::identity::Keypair;
use libp2p::{Multiaddr, PeerId};
use serde::Serialize;

/// Where a connection (or a membership, or a review link) stands, in words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    /// Asked; the other side has to accept (or an owner to approve).
    Pending,
    /// They asked us; it waits for our yes or no.
    Requested,
    /// Keys exchanged: the two nodes talk.
    Connected,
    /// Ended here or by them, or turned down.
    Ended,
    /// It did not work (the reason is given).
    Failed,
}

impl LinkStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkStatus::Pending => "pending",
            LinkStatus::Requested => "requested",
            LinkStatus::Connected => "connected",
            LinkStatus::Ended => "ended",
            LinkStatus::Failed => "failed",
        }
    }
}

/// The status of an exchange, as the user sees it.
pub fn status_of(x: &Exchange) -> LinkStatus {
    match x.status {
        ExchangeStatus::Offered => {
            if x.expires_at.is_some_and(|e| e < now_ms()) {
                LinkStatus::Ended
            } else {
                LinkStatus::Pending
            }
        }
        ExchangeStatus::Incoming => LinkStatus::Requested,
        ExchangeStatus::Complete | ExchangeStatus::Verified => LinkStatus::Connected,
        ExchangeStatus::Revoked => LinkStatus::Ended,
    }
}

/// What an exchange is for, in a word: `node`, `org NAME`, `review TITLE`.
pub fn purpose(x: &Exchange) -> String {
    let s = |k: &str| x.scope.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match s("kind").as_str() {
        "organization" => format!("org {}", s("name")),
        "resource" => format!("review {}", s("title")),
        _ => "node".into(),
    }
}

/// The result of an action that needed a key exchange.
#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub status: LinkStatus,
    /// The peer's name (label, else its short id).
    pub peer: String,
    pub peer_id: Option<String>,
    pub exchange: Option<Exchange>,
    /// One plain line: what happened or what it waits for.
    pub detail: String,
    /// Set when the node uses a name another known node has (see [`impostor`]).
    pub warning: Option<String>,
}

impl Link {
    /// `connected to ben`, `pending: …`, `failed: …` and a warning line, if any.
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![format!("{}: {}", self.status.as_str(), self.detail)];
        if let Some(w) = &self.warning {
            v.push(format!("warning: {w}"));
        }
        v
    }
}

fn record(s: &Store, origin: &str, kind: &str, summary: String, params: serde_json::Value) {
    s.note_action(&NewAction::new(kind, origin, summary).params(params));
}

/// Whether a key exchange with this node was ever completed (and not ended by us): its key
/// is pinned here.
pub fn known(s: &Store, peer: &str) -> bool {
    s.exchanges()
        .map(|xs| xs.iter().any(|x| x.peer_id.as_deref() == Some(peer) && x.answer.is_some() && x.revoked_by.as_deref() != Some("me")))
        .unwrap_or(false)
}

/// Pinned: any completed exchange with the node, whoever ended it.
fn pinned(s: &Store, peer: &str) -> bool {
    s.exchanges().map(|xs| xs.iter().any(|x| x.peer_id.as_deref() == Some(peer) && x.answer.is_some())).unwrap_or(false)
}

/// A warning when `peer` calls itself by the name of another node whose key is pinned here.
pub fn impostor(s: &Store, peer: &str, label: &str) -> Option<String> {
    let l = label.trim();
    if l.is_empty() || pinned(s, peer) {
        return None;
    }
    let xs = s.exchanges().ok()?;
    let other = xs.iter().find(|x| {
        x.answer.is_some()
            && x.peer_id.as_deref().is_some_and(|p| p != peer)
            && (x.peer_label.eq_ignore_ascii_case(l) || x.label.eq_ignore_ascii_case(l))
    })?;
    Some(format!(
        "this node calls itself {l}, but its key is not the one of the {l} you know ({}); it is a different node",
        super::short_peer_id(&other.peer_id.as_deref().unwrap_or_default().parse().unwrap_or(PeerId::random()))
    ))
}

/// A peer by address, id, or name; a name prefers the node whose key is pinned.
pub fn resolve(s: &Store, target: &str, relays: &[String]) -> Result<(PeerId, Vec<Multiaddr>)> {
    let t = target.trim();
    if !t.starts_with('/') && t.parse::<PeerId>().is_err() {
        let all = s.net_peers()?;
        let named: Vec<_> = all.iter().filter(|p| p.label.eq_ignore_ascii_case(t)).collect();
        if named.len() > 1
            && let Some(p) = named.iter().find(|p| pinned(s, &p.peer_id))
        {
            let pid: PeerId = p.peer_id.parse().map_err(|_| Error::invalid("peer", "bad peer id"))?;
            return Ok((pid, client::peer_addrs(s, &p.peer_id, relays)));
        }
    }
    client::resolve(s, target, relays)
}

/// The node-scoped exchange we have with a peer: active first, else pending either way.
pub fn node_link(s: &Store, peer: &str) -> Result<Option<Exchange>> {
    let xs: Vec<Exchange> = s
        .exchanges()?
        .into_iter()
        .filter(|x| x.peer_id.as_deref() == Some(peer) && x.scope.get("kind").and_then(|k| k.as_str()) == Some("node"))
        .collect();
    let pick = |f: &dyn Fn(&Exchange) -> bool| xs.iter().find(|x| f(x)).cloned();
    Ok(pick(&|x| x.status.active())
        .or_else(|| pick(&|x| x.status == ExchangeStatus::Incoming))
        .or_else(|| pick(&|x| status_of(x) == LinkStatus::Pending)))
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

fn title_of(s: &Store, peer: &PeerId) -> String {
    s.net_peer(&peer.to_string()).ok().flatten().map(|p| p.label).filter(|l| !l.is_empty()).unwrap_or_else(|| super::short_peer_id(peer))
}

/// What delivering an offer gave.
#[allow(clippy::large_enum_variant)]
pub enum Delivered {
    /// The peer answered on the spot; the exchange is complete here.
    Answered(Exchange),
    /// The peer keeps it for its owner (the message says so).
    Kept(String),
}

/// Carry one of our offers to its peer: `Connect` (2.9), falling back to `Offer` for an
/// older node. An answer that comes back completes the exchange here.
pub fn deliver(s: &Store, node: &Node, peer: PeerId, addrs: &[Multiaddr], token: &str) -> std::result::Result<Delivered, String> {
    let r = match node.request(peer, addrs, Wire::Connect { token: token.to_string() }) {
        Ok(WireResp::Error { msg }) if msg.contains("unknown variant") || msg.contains("Connect") => {
            node.request(peer, addrs, Wire::Offer { token: token.to_string() })
        }
        Err(e) if e.to_string().contains("decode") || e.to_string().contains("variant") => {
            node.request(peer, addrs, Wire::Offer { token: token.to_string() })
        }
        r => r,
    };
    match r {
        Ok(WireResp::Answered { token: t }) => {
            let Ok(Token::Answer(a)) = exchange::parse_token(&t) else { return Err("the peer sent back something that is not an answer".into()) };
            if a.peer != peer.to_string() {
                return Err("the answer was signed by a different node".into());
            }
            match service::complete_offer(s, &a, &t) {
                Ok(x) => {
                    record(
                        s,
                        "network",
                        "net.exchange.complete",
                        format!("key exchange with {} complete", x.title()),
                        serde_json::json!({ "exchange": x.id, "peer": x.peer_id, "auto": true }),
                    );
                    Ok(Delivered::Answered(x))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        Ok(WireResp::Ok { msg }) => Ok(Delivered::Kept(msg)),
        Ok(WireResp::Error { msg }) => Err(msg),
        Ok(_) => Err("unexpected answer".into()),
        Err(e) => Err(plain(&e.to_string())),
    }
}

/// Network errors in plain words.
pub fn plain(e: &str) -> String {
    let l = e.to_ascii_lowercase();
    if l.contains("timeout") || l.contains("timed out") {
        "the node did not answer in time (is it running and reachable?)".into()
    } else if l.contains("dial") || l.contains("unreachable") || l.contains("no addresses") || l.contains("connection refused") {
        "could not reach the node (is it running? check the address, or add a relay)".into()
    } else {
        e.trim_start_matches("network: ").to_string()
    }
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
    let (p, dial) = resolve(s, target, relays)?;
    let pid = p.to_string();
    let fail = |detail: String| Link {
        status: LinkStatus::Failed,
        peer: title_of(s, &p),
        peer_id: Some(pid.clone()),
        exchange: None,
        detail,
        warning: None,
    };
    // hello: is it there, and what does it call itself
    let hello = match node.request(p, &dial, Wire::Hello) {
        Ok(WireResp::Hello { label: l, agent, .. }) => {
            let reach: Vec<String> =
                dial.iter().filter(|a| !a.iter().any(|x| x == libp2p::multiaddr::Protocol::P2pCircuit)).map(|a| a.to_string()).collect();
            let name = label.map(str::to_string).filter(|x| !x.is_empty());
            let warning = impostor(s, &pid, &l);
            s.upsert_net_peer(&pid, name.as_deref().or(Some(&l)), &reach, None, Some(&agent), "manual")?;
            warning
        }
        Ok(WireResp::Error { msg }) => return Ok(fail(msg)),
        Ok(_) => return Ok(fail("that node does not speak Muninn".into())),
        Err(e) => return Ok(fail(plain(&e.to_string()))),
    };
    let peer = title_of(s, &p);
    let link = |status, exchange: Option<Exchange>, detail: String| Link {
        status,
        peer: peer.clone(),
        peer_id: Some(pid.clone()),
        exchange,
        detail,
        warning: hello.clone(),
    };
    match node_link(s, &pid)? {
        Some(x) if x.status.active() => return Ok(link(LinkStatus::Connected, Some(x), format!("already connected to {peer}"))),
        // they asked first: connecting is our yes
        Some(x) if x.status == ExchangeStatus::Incoming => {
            let a = accept(s, Some(node), kp, &x.id, my_label, addrs, origin)?;
            return Ok(a);
        }
        Some(x) => {
            // our earlier request: carry it again
            return Ok(match deliver(s, node, p, &dial, &x.offer) {
                Ok(Delivered::Answered(x)) => link(LinkStatus::Connected, Some(x), format!("connected to {peer}")),
                Ok(Delivered::Kept(_)) => link(LinkStatus::Pending, Some(x), format!("waiting for {peer} to accept")),
                Err(e) => link(LinkStatus::Failed, Some(x), e),
            });
        }
        None => {}
    }
    let x = offer_to(s, kp, &pid, exchange::node_scope(), label.unwrap_or(""), my_label, addrs, client::DEFAULT_TTL_MS, origin)?;
    Ok(match deliver(s, node, p, &dial, &x.offer) {
        Ok(Delivered::Answered(x)) => link(LinkStatus::Connected, Some(x), format!("connected to {peer}")),
        Ok(Delivered::Kept(_)) => link(LinkStatus::Pending, Some(x), format!("waiting for {peer} to accept")),
        Err(e) => {
            let _ = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("me"));
            link(LinkStatus::Failed, Some(x), e)
        }
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
    let (offer, secret) = exchange::Offer::new(kp, my_label, addrs, scope, now_ms(), ttl_ms)?;
    let token = offer.token()?;
    let x = s.insert_exchange(
        &NewExchange {
            id: offer.id.clone(),
            label: label.to_string(),
            role: "offerer".into(),
            status: Some(ExchangeStatus::Offered),
            peer_id: Some(peer.to_string()),
            peer_label: String::new(),
            scope: offer.scope.clone(),
            offer: token,
            expires_at: Some(offer.expires),
        },
        Some(&secret[..]),
    )?;
    record(
        s,
        origin,
        "net.exchange.create",
        "key exchange requested".to_string(),
        serde_json::json!({ "exchange": x.id, "peer": peer, "auto": true }),
    );
    Ok(x)
}

/// Accept a request (an incoming exchange, by id, peer name or peer id): answer it and
/// deliver the answer (or leave it for [`settle`] when the peer cannot be reached now).
pub fn accept(s: &Store, node: Option<&Node>, kp: &Keypair, key: &str, my_label: &str, addrs: Vec<String>, origin: &str) -> Result<Link> {
    let x = incoming(s, key)?;
    let a: Accepted = client::accept(s, node, kp, &x.id, "", my_label, addrs, origin)?;
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

// ---------- the receiving node ----------

/// `Wire::Connect`: keep the offer, and answer it on the spot when the scope is a plain
/// node connection and the peer is known here, has a request of ours open, or anyone is
/// accepted. Memberships and review links always wait for the owner.
pub fn connect_in(s: &Store, from: &PeerId, token: &str, my_label: &str, accept_anyone: bool) -> Result<WireResp> {
    let msg = service::incoming_offer(s, from, token)?;
    let Token::Offer(o) = exchange::parse_token(token)? else { return Err(Error::invalid("token", "expected an offer")) };
    let Some(x) = s.exchange(&o.id)? else { return Ok(WireResp::Ok { msg }) };
    if x.status.active() && x.role == "answerer" {
        // asked again: the same answer
        if let Some(a) = x.answer.clone() {
            s.mark_answer_delivered(&x.id)?;
            return Ok(WireResp::Answered { token: a });
        }
    }
    if x.status != ExchangeStatus::Incoming {
        return Ok(WireResp::Ok { msg });
    }
    let peer = from.to_string();
    let node_scope = o.scope.get("kind").and_then(|k| k.as_str()) == Some("node");
    let ours = s
        .exchanges()?
        .into_iter()
        .filter(|y| {
            y.id != x.id
                && y.peer_id.as_deref() == Some(&peer)
                && y.status == ExchangeStatus::Offered
                && y.scope.get("kind").and_then(|k| k.as_str()) == Some("node")
        })
        .collect::<Vec<_>>();
    let already = s.active_exchanges_with(&peer)?.into_iter().any(|y| y.scope.get("kind").and_then(|k| k.as_str()) == Some("node"));
    if !node_scope || !(accept_anyone || known(s, &peer) || !ours.is_empty() || already) {
        return Ok(WireResp::Ok { msg: "request received; it waits for the owner to accept".into() });
    }
    let kp = super::identity::load_or_create(s)?;
    let a = client::accept(s, None, &kp, &x.id, "", my_label, client::my_addrs(s, None), "network")?;
    s.mark_answer_delivered(&x.id)?;
    // both asked at once: theirs is the one; ours goes
    for y in ours {
        let _ = s.set_exchange_status(&y.id, ExchangeStatus::Revoked, Some("me"));
    }
    record(
        s,
        "network",
        "net.exchange.auto",
        format!("connected to {} automatically", a.exchange.title()),
        serde_json::json!({ "exchange": x.id, "peer": peer }),
    );
    match a.answer {
        Some(t) => Ok(WireResp::Answered { token: t }),
        None => Ok(WireResp::Ok { msg }),
    }
}

/// `Wire::Poll`: where one of the asker's offers stands here (only its offerer may ask).
pub fn poll_in(s: &Store, from: &PeerId, exchange: &str) -> Result<(String, Option<String>)> {
    let x = s.exchange(exchange)?.ok_or_else(|| Error::invalid("exchange", "unknown request"))?;
    if x.peer_id.as_deref() != Some(&from.to_string()) || x.role != "answerer" {
        return Err(Error::invalid("exchange", "unknown request"));
    }
    let kind = x.scope.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    let how = x.scope.get("as").and_then(|k| k.as_str()).unwrap_or("");
    if kind == "organization" && how == "apply" {
        let r = crate::org::net::poll(s, from, exchange)?;
        if r.1.is_some() {
            s.mark_answer_delivered(&x.id)?;
        }
        return Ok(r);
    }
    Ok(match x.status {
        st if st.active() => {
            s.mark_answer_delivered(&x.id)?;
            ("connected".into(), x.answer.clone())
        }
        ExchangeStatus::Revoked => ("declined".into(), None),
        _ => ("pending".into(), None),
    })
}

// ---------- keeping things moving ----------

/// Bring pending exchanges along, quietly: ask peers about our open requests (an answer
/// completes them), and deliver answers of ours that have not reached their offerer.
/// Returns one line per change. Unreachable peers are skipped until next time.
pub fn settle(s: &Store, node: &Node, relays: &[String], origin: &str) -> Vec<String> {
    let mut out = vec![];
    let now = now_ms();
    let Ok(xs) = s.exchanges() else { return out };
    for x in xs.iter().filter(|x| x.status == ExchangeStatus::Offered && x.expires_at.is_none_or(|e| e > now)) {
        let Some(peer) = x.peer_id.as_deref().and_then(|p| p.parse::<PeerId>().ok()) else { continue };
        // org applications have their own bookkeeping
        if let Ok(Some(j)) = s.joined_by_exchange(&x.id) {
            if j.status == crate::store::MemberStatus::Applied
                && let Ok(j2) = crate::org::net::sync(s, node, &j, relays, origin)
                && j2.status != j.status
            {
                out.push(format!("{}: {}", j2.name, if j2.status == crate::store::MemberStatus::Member { "approved" } else { j2.status.as_str() }));
            }
            continue;
        }
        let addrs = client::peer_addrs(s, &peer.to_string(), relays);
        match node.request(peer, &addrs, Wire::Poll { exchange: x.id.clone() }) {
            Ok(WireResp::Poll { answer: Some(t), .. }) => {
                if let Ok(Token::Answer(a)) = exchange::parse_token(&t)
                    && a.peer == peer.to_string()
                    && let Ok(x) = service::complete_offer(s, &a, &t)
                {
                    record(
                        s,
                        origin,
                        "net.exchange.complete",
                        format!("key exchange with {} complete", x.title()),
                        serde_json::json!({ "exchange": x.id, "peer": x.peer_id, "auto": true }),
                    );
                    out.push(format!("connected: {} ({})", x.title(), purpose(&x)));
                }
            }
            Ok(WireResp::Poll { status, .. })
                if (status == "declined" || status == "rejected") && s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("peer")).is_ok() =>
            {
                out.push(format!("declined: {} ({})", x.title(), purpose(x)));
            }
            _ => {}
        }
    }
    if let Ok(xs) = s.undelivered_answers() {
        for x in xs {
            let Some(peer) = x.peer_id.as_deref().and_then(|p| p.parse::<PeerId>().ok()) else { continue };
            let Some(t) = x.answer.clone() else { continue };
            let addrs = client::peer_addrs(s, &peer.to_string(), relays);
            match node.request(peer, &addrs, Wire::Answer { token: t }) {
                Ok(WireResp::Ok { .. }) => {
                    let _ = s.mark_answer_delivered(&x.id);
                    out.push(format!("{} got our answer ({})", x.title(), purpose(&x)));
                }
                Ok(WireResp::Error { msg }) if msg.contains("already") => {
                    let _ = s.mark_answer_delivered(&x.id);
                }
                _ => {}
            }
        }
    }
    out
}
