//! Invitation automation: the nodes run key exchanges themselves. An offer goes to the peer
//! over the node (a `connect` request), signed by the node that sends it; the answer comes back
//! in the reply, or later (a `poll` from the offerer, or the answerer delivering it) when the
//! other side has to decide first. Two nodes that offer to each other at once settle on one
//! exchange. A node *is* its key: the first completed exchange pins it (trust on first use),
//! and another node using a pinned node's name is flagged ([`impostor`]).
//!
//! The application keeps its records ([`Host`]), its messages ([`Ask`], [`Say`]) and every
//! decision: what a kept offer may be ([`Host::admit`]), whether to answer one on the spot
//! (the `decide` given to [`connect_in`]), and its own bookkeeping ([`Host::kept`],
//! [`Host::completed`], [`Host::poll`]). Nothing here accepts more than the host allows.
use crate::addr::{addr_peer, one_circuit_per_relay, parse_addrs, shareable};
use crate::exchange::{Answer, Derived, Offer, Token};
use crate::{Keypair, Message, Multiaddr, NetError, Node, PeerId, Profile, now_ms, short_peer_id};
use libp2p::multiaddr::Protocol;
use serde::Serialize;
use serde_json::{Value, json};
use zeroize::Zeroizing;

/// Where an exchange stands in the host's records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Ours, waiting for the answer.
    Offered,
    /// Theirs, waiting for our owner (or policy).
    Incoming,
    Complete,
    /// Complete, and the short codes were compared.
    Verified,
    Revoked,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Offered => "offered",
            Status::Incoming => "incoming",
            Status::Complete => "complete",
            Status::Verified => "verified",
            Status::Revoked => "revoked",
        }
    }
    /// Keys exchanged.
    pub fn active(self) -> bool {
        matches!(self, Status::Complete | Status::Verified)
    }
}

/// Which side of an exchange this node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Offerer,
    Answerer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Offerer => "offerer",
            Role::Answerer => "answerer",
        }
    }
}

/// One exchange record, as the host keeps it.
pub trait Rec: Clone {
    fn id(&self) -> &str;
    fn role(&self) -> Role;
    fn status(&self) -> Status;
    fn peer_id(&self) -> Option<&str>;
    fn label(&self) -> &str;
    fn peer_label(&self) -> &str;
    fn scope(&self) -> &Value;
    /// The offer token.
    fn offer(&self) -> &str;
    /// The answer token, once there is one.
    fn answer(&self) -> Option<&str>;
    fn expires_at(&self) -> Option<i64>;
    /// `me` or `peer`.
    fn revoked_by(&self) -> Option<&str>;
    fn fingerprint(&self) -> Option<&str>;
    /// The name to show for it.
    fn title(&self) -> String;
    /// What it is for, in a word.
    fn purpose(&self) -> String {
        crate::profile::scope_kind(self.scope()).to_string()
    }
}

/// A record to keep.
pub struct NewRec<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub role: Role,
    pub status: Status,
    pub peer_id: &'a str,
    pub peer_label: &'a str,
    pub scope: &'a Value,
    pub offer: &'a str,
    pub expires_at: i64,
}

/// The application's records and decisions.
pub trait Host {
    type Error: From<NetError> + std::fmt::Display;
    type Rec: Rec;
    fn records(&self) -> Result<Vec<Self::Rec>, Self::Error>;
    fn get(&self, id: &str) -> Result<Option<Self::Rec>, Self::Error>;
    /// Keep a new record (with the offer's X25519 secret when it is ours).
    fn insert(&self, n: &NewRec, secret: Option<&[u8]>) -> Result<Self::Rec, Self::Error>;
    /// Store the key and codes and erase the secret.
    fn complete(&self, id: &str, peer: &str, peer_label: &str, answer: &str, d: &Derived) -> Result<Self::Rec, Self::Error>;
    fn secret(&self, id: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Self::Error>;
    fn set_status(&self, id: &str, status: Status, by: &str) -> Result<Self::Rec, Self::Error>;
    fn relabel(&self, id: &str, label: &str) -> Result<(), Self::Error>;
    fn answer_delivered(&self, id: &str) -> Result<(), Self::Error>;
    /// Our answers that have not reached their offerer.
    fn undelivered(&self) -> Result<Vec<Self::Rec>, Self::Error>;
    /// Remember a peer: its label, addresses and agent, and where we learned them.
    fn saw(&self, peer: &str, label: Option<&str>, addrs: &[String], agent: Option<&str>, source: &str) -> Result<(), Self::Error>;
    /// Addresses seen for a peer.
    fn sightings(&self, peer: &str) -> Vec<String>;
    /// A peer's remembered label.
    fn peer_label(&self, peer: &str) -> Option<String>;
    /// Peer ids remembered under a label.
    fn named(&self, label: &str) -> Vec<String>;
    /// A known peer by label or short id (the host's own error when there is none).
    fn find_peer(&self, key: &str) -> Result<String, Self::Error>;
    /// Note what happened (an audit line).
    fn note(&self, origin: &str, kind: &str, summary: String, params: Value);

    /// Refuse a delivered offer before it is kept (an error), or let it in.
    fn admit(&self, _offer: &Offer) -> Result<(), Self::Error> {
        Ok(())
    }
    /// A delivered offer was kept: `Some(reply)` when the host took it over.
    fn kept(&self, _offer: &Offer, _rec: &Self::Rec) -> Result<Option<String>, Self::Error> {
        Ok(None)
    }
    /// An exchange completed, either side.
    fn completed(&self, _rec: &Self::Rec) -> Result<(), Self::Error> {
        Ok(())
    }
    /// Answer a poll for this record instead (the host's own bookkeeping).
    fn poll(&self, _rec: &Self::Rec, _from: &PeerId) -> Option<Result<Polled, Self::Error>> {
        None
    }
    /// How many offers may wait here.
    fn max_incoming(&self) -> usize {
        50
    }
    /// What a peer that answers hello in another protocol is told.
    fn not_ours(&self) -> String {
        "that node does not speak this protocol".into()
    }
}

/// Requests the automation sends, in the application's message type.
pub trait Ask: Message {
    fn hello() -> Self;
    fn connect(token: String) -> Self;
    /// The older delivery (no answer on the spot).
    fn offer(token: String) -> Self;
    fn answer(token: String) -> Self;
    fn poll(exchange: String) -> Self;
}

/// A reply, as the automation reads it.
pub enum Said {
    Hello { label: String, agent: String },
    Ok(String),
    Answered(String),
    Poll { status: String, answer: Option<String> },
    Error(String),
    Other,
}

/// Replies in the application's message type.
pub trait Say: Message {
    fn said(self) -> Said;
}

/// Where a link stands, in words.
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

/// The link status of a record.
pub fn status_of<X: Rec>(x: &X) -> LinkStatus {
    match x.status() {
        Status::Offered => {
            if x.expires_at().is_some_and(|e| e < now_ms()) {
                LinkStatus::Ended
            } else {
                LinkStatus::Pending
            }
        }
        Status::Incoming => LinkStatus::Requested,
        Status::Complete | Status::Verified => LinkStatus::Connected,
        Status::Revoked => LinkStatus::Ended,
    }
}

/// The result of an action that needed a key exchange.
#[derive(Debug, Clone, Serialize)]
pub struct Link<X> {
    pub status: LinkStatus,
    /// The peer's name (label, else its short id).
    pub peer: String,
    pub peer_id: Option<String>,
    pub exchange: Option<X>,
    /// One plain line: what happened or what it waits for.
    pub detail: String,
    /// Set when the node uses a name another pinned node has (see [`impostor`]).
    pub warning: Option<String>,
}

impl<X> Link<X> {
    /// `connected: …` and a warning line, if any.
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![format!("{}: {}", self.status.as_str(), self.detail)];
        if let Some(w) = &self.warning {
            v.push(format!("warning: {w}"));
        }
        v
    }
}

/// What delivering an offer gave.
pub enum Delivered<X> {
    /// The peer answered on the spot; the exchange is complete here.
    Answered(X),
    /// The peer keeps it (the message says so).
    Kept(String),
}

/// An offer answered here.
pub struct Accepted<X> {
    pub exchange: X,
    pub answer: Option<String>,
    /// Whether the answer reached the offerer (`None`: no node to carry it).
    pub delivered: Option<std::result::Result<String, String>>,
}

type R<T, H> = std::result::Result<T, <H as Host>::Error>;

/// A poll's answer: the status (`pending`, `connected`, `declined`, or the host's own) and
/// the answer token once there is one.
pub type Polled = (String, Option<String>);

fn bad(what: &'static str, msg: impl Into<String>) -> NetError {
    NetError::invalid(what, msg)
}

// ---------- trust on first use ----------

/// A completed exchange with the node that we did not end: its key is pinned here.
pub fn known<H: Host>(h: &H, peer: &str) -> bool {
    h.records().map(|xs| xs.iter().any(|x| x.peer_id() == Some(peer) && x.answer().is_some() && x.revoked_by() != Some("me"))).unwrap_or(false)
}

/// Pinned: any completed exchange with the node, whoever ended it.
pub fn pinned<H: Host>(h: &H, peer: &str) -> bool {
    h.records().map(|xs| xs.iter().any(|x| x.peer_id() == Some(peer) && x.answer().is_some())).unwrap_or(false)
}

/// A warning when `peer` calls itself by the name of another node whose key is pinned here.
pub fn impostor<H: Host>(h: &H, peer: &str, label: &str) -> Option<String> {
    let l = label.trim();
    if l.is_empty() || pinned(h, peer) {
        return None;
    }
    let xs = h.records().ok()?;
    let other = xs.iter().find(|x| {
        x.answer().is_some()
            && x.peer_id().is_some_and(|p| p != peer)
            && (x.peer_label().eq_ignore_ascii_case(l) || x.label().eq_ignore_ascii_case(l))
    })?;
    Some(format!(
        "this node calls itself {l}, but its key is not the one of the {l} you know ({}); it is a different node",
        short_peer_id(&other.peer_id().unwrap_or_default().parse().unwrap_or(PeerId::random()))
    ))
}

// ---------- finding peers ----------

/// Addresses we know for a peer: sightings, its exchange tokens, and circuits through the
/// configured relays (one per relay, shareable ones only).
pub fn peer_addrs<H: Host>(p: &Profile, h: &H, peer: &str, relays: &[String]) -> Vec<Multiaddr> {
    let mut v: Vec<String> = h.sightings(peer);
    if let Ok(xs) = h.records() {
        for x in xs.into_iter().filter(|x| x.peer_id() == Some(peer)) {
            let tokens = [Some(x.offer().to_string()), x.answer().map(str::to_string)];
            for t in tokens.into_iter().flatten() {
                match p.parse_token(&t) {
                    Ok(Token::Offer(o)) if o.peer == peer => v.extend(o.addrs),
                    Ok(Token::Answer(a)) if a.peer == peer => v.extend(a.addrs),
                    _ => {}
                }
            }
        }
    }
    let Ok(pid) = peer.parse::<PeerId>() else { return vec![] };
    let mut out: Vec<Multiaddr> = vec![];
    for a in parse_addrs(&v) {
        // drop a trailing /p2p/ of the same peer; libp2p adds it
        let a = match addr_peer(&a) {
            Some(p) if p == pid => {
                let mut a = a;
                a.pop();
                a
            }
            Some(_) if !a.iter().any(|p| p == Protocol::P2pCircuit) => continue,
            _ => a,
        };
        if !out.contains(&a) {
            out.push(a);
        }
    }
    let configured: Vec<Multiaddr> = parse_addrs(relays).into_iter().filter(|r| addr_peer(r).is_some()).collect();
    for r in &configured {
        let c = r.clone().with(Protocol::P2pCircuit);
        if !out.contains(&c) {
            out.push(c);
        }
    }
    one_circuit_per_relay(out.into_iter().filter(shareable).collect(), &configured)
}

/// A peer by full address (`…/p2p/ID`), peer id, or a known peer's label or short id.
pub fn resolve_addr<H: Host>(p: &Profile, h: &H, target: &str, relays: &[String]) -> R<(PeerId, Vec<Multiaddr>), H> {
    let t = target.trim();
    if t.starts_with('/') {
        let a: Multiaddr = t.parse().map_err(|e| bad("address", format!("{t}: {e}")))?;
        let pid = addr_peer(&a).ok_or_else(|| bad("address", "needs the peer id at the end (…/p2p/12D3Koo…)"))?;
        let mut v = vec![a];
        v.extend(peer_addrs(p, h, &pid.to_string(), relays));
        return Ok((pid, v));
    }
    let peer = match t.parse::<PeerId>() {
        Ok(x) => x.to_string(),
        Err(_) => h.find_peer(t)?,
    };
    let pid: PeerId = peer.parse().map_err(|_| bad("peer", "bad peer id"))?;
    Ok((pid, peer_addrs(p, h, &peer, relays)))
}

/// Like [`resolve_addr`], but a name shared by several nodes means the pinned one.
pub fn resolve<H: Host>(p: &Profile, h: &H, target: &str, relays: &[String]) -> R<(PeerId, Vec<Multiaddr>), H> {
    let t = target.trim();
    if !t.starts_with('/') && t.parse::<PeerId>().is_err() {
        let named = h.named(t);
        if named.len() > 1
            && let Some(peer) = named.iter().find(|x| pinned(h, x))
        {
            let pid: PeerId = peer.parse().map_err(|_| bad("peer", "bad peer id"))?;
            return Ok((pid, peer_addrs(p, h, peer, relays)));
        }
    }
    resolve_addr(p, h, target, relays)
}

/// The record of one scope kind we have with a peer: active first, else theirs waiting,
/// else ours pending.
pub fn kind_link<H: Host>(h: &H, peer: &str, kind: &str) -> R<Option<H::Rec>, H> {
    let xs: Vec<H::Rec> = h.records()?.into_iter().filter(|x| x.peer_id() == Some(peer) && crate::profile::scope_kind(x.scope()) == kind).collect();
    let pick = |f: &dyn Fn(&H::Rec) -> bool| xs.iter().find(|x| f(x)).cloned();
    Ok(pick(&|x| x.status().active())
        .or_else(|| pick(&|x| x.status() == Status::Incoming))
        .or_else(|| pick(&|x| status_of(x) == LinkStatus::Pending)))
}

fn title_of<H: Host>(h: &H, peer: &PeerId) -> String {
    h.peer_label(&peer.to_string()).filter(|l| !l.is_empty()).unwrap_or_else(|| short_peer_id(peer))
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

// ---------- the offering node ----------

/// Make an offer for a known peer and keep it, without delivering it.
#[allow(clippy::too_many_arguments)]
pub fn offer_to<H: Host>(
    p: &Profile,
    h: &H,
    kp: &Keypair,
    peer: &str,
    scope: Value,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    ttl_ms: i64,
    origin: &str,
) -> R<H::Rec, H> {
    let (offer, secret) = p.offer(kp, my_label, addrs, scope, now_ms(), ttl_ms)?;
    let token = p.offer_token(&offer)?;
    let x = h.insert(
        &NewRec {
            id: &offer.id,
            label,
            role: Role::Offerer,
            status: Status::Offered,
            peer_id: peer,
            peer_label: "",
            scope: &offer.scope,
            offer: &token,
            expires_at: offer.expires,
        },
        Some(&secret[..]),
    )?;
    h.note(origin, "net.exchange.create", "key exchange requested".to_string(), json!({ "exchange": x.id(), "peer": peer, "auto": true }));
    Ok(x)
}

/// Complete our offer with an answer (from the network or pasted).
pub fn complete_offer<H: Host>(p: &Profile, h: &H, a: &Answer, token: &str) -> R<H::Rec, H> {
    let x = h.get(&a.offer)?.ok_or_else(|| bad("exchange", "this answer is for an offer this node did not make"))?;
    if x.role() != Role::Offerer {
        return Err(bad("exchange", "this answer is for an offer this node did not make").into());
    }
    if x.status() != Status::Offered {
        return Err(bad("exchange", format!("the exchange is already {}", x.status().as_str())).into());
    }
    let Token::Offer(o) = p.parse_token(x.offer())? else {
        return Err(NetError::Other("stored offer is damaged".into()).into());
    };
    let secret = h.secret(x.id())?.ok_or_else(|| NetError::Other("the offer's secret is gone".into()))?;
    let d = p.complete(a, &o, &secret)?;
    let x = h.complete(x.id(), &a.peer, &a.label, &token.split_whitespace().collect::<String>(), &d)?;
    h.saw(&a.peer, Some(&a.label), &a.addrs, None, "exchange")?;
    h.completed(&x)?;
    Ok(x)
}

/// Carry one of our offers to its peer (`connect`, falling back to the older `offer`). An
/// answer that comes back completes the exchange here, if the node that signed it is the
/// peer we sent it to.
pub fn deliver<H: Host, Q: Ask, S: Say>(
    p: &Profile,
    h: &H,
    node: &Node<Q, S>,
    peer: PeerId,
    addrs: &[Multiaddr],
    token: &str,
) -> std::result::Result<Delivered<H::Rec>, String> {
    let r = match node.request(peer, addrs, Q::connect(token.to_string())).map(Say::said) {
        Ok(Said::Error(msg)) if msg.contains("unknown variant") || msg.contains("Connect") => {
            node.request(peer, addrs, Q::offer(token.to_string())).map(Say::said)
        }
        Err(e) if e.to_string().contains("decode") || e.to_string().contains("variant") => {
            node.request(peer, addrs, Q::offer(token.to_string())).map(Say::said)
        }
        r => r,
    };
    match r {
        Ok(Said::Answered(t)) => {
            let Ok(Token::Answer(a)) = p.parse_token(&t) else { return Err("the peer sent back something that is not an answer".into()) };
            if a.peer != peer.to_string() {
                return Err("the answer was signed by a different node".into());
            }
            match complete_offer(p, h, &a, &t) {
                Ok(x) => {
                    h.note(
                        "network",
                        "net.exchange.complete",
                        format!("key exchange with {} complete", x.title()),
                        json!({ "exchange": x.id(), "peer": x.peer_id(), "auto": true }),
                    );
                    Ok(Delivered::Answered(x))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        Ok(Said::Ok(msg)) => Ok(Delivered::Kept(msg)),
        Ok(Said::Error(msg)) => Err(msg),
        Ok(_) => Err("unexpected answer".into()),
        Err(e) => Err(plain(&e.to_string())),
    }
}

/// Connect to a node: say hello, then make a key exchange of `scope` with it unless there is
/// one. Their request waiting here is answered with `accept`; ours is carried again.
#[allow(clippy::too_many_arguments)]
pub fn connect<H: Host, Q: Ask, S: Say>(
    p: &Profile,
    h: &H,
    node: &Node<Q, S>,
    kp: &Keypair,
    target: &str,
    label: Option<&str>,
    my_label: &str,
    addrs: Vec<String>,
    relays: &[String],
    origin: &str,
    scope: Value,
    ttl_ms: i64,
    accept: impl FnOnce(&H::Rec, Vec<String>) -> R<Link<H::Rec>, H>,
) -> R<Link<H::Rec>, H> {
    let kind = crate::profile::scope_kind(&scope).to_string();
    let (pid, dial) = resolve(p, h, target, relays)?;
    let peer_id = pid.to_string();
    let fail = |detail: String| Link {
        status: LinkStatus::Failed,
        peer: title_of(h, &pid),
        peer_id: Some(peer_id.clone()),
        exchange: None,
        detail,
        warning: None,
    };
    // hello: is it there, and what does it call itself
    let warning = match node.request(pid, &dial, Q::hello()).map(Say::said) {
        Ok(Said::Hello { label: l, agent }) => {
            let reach: Vec<String> = dial.iter().filter(|a| !a.iter().any(|x| x == Protocol::P2pCircuit)).map(|a| a.to_string()).collect();
            let name = label.map(str::to_string).filter(|x| !x.is_empty());
            let warning = impostor(h, &peer_id, &l);
            h.saw(&peer_id, name.as_deref().or(Some(&l)), &reach, Some(&agent), "manual")?;
            warning
        }
        Ok(Said::Error(msg)) => return Ok(fail(msg)),
        Ok(_) => return Ok(fail(h.not_ours())),
        Err(e) => return Ok(fail(plain(&e.to_string()))),
    };
    let peer = title_of(h, &pid);
    let link = |status, exchange: Option<H::Rec>, detail: String| Link {
        status,
        peer: peer.clone(),
        peer_id: Some(peer_id.clone()),
        exchange,
        detail,
        warning: warning.clone(),
    };
    match kind_link(h, &peer_id, &kind)? {
        Some(x) if x.status().active() => return Ok(link(LinkStatus::Connected, Some(x), format!("already connected to {peer}"))),
        // they asked first: connecting is our yes
        Some(x) if x.status() == Status::Incoming => return accept(&x, addrs),
        Some(x) => {
            // our earlier request: carry it again
            return Ok(match deliver(p, h, node, pid, &dial, x.offer()) {
                Ok(Delivered::Answered(x)) => link(LinkStatus::Connected, Some(x), format!("connected to {peer}")),
                Ok(Delivered::Kept(_)) => link(LinkStatus::Pending, Some(x), format!("waiting for {peer} to accept")),
                Err(e) => link(LinkStatus::Failed, Some(x), e),
            });
        }
        None => {}
    }
    let x = offer_to(p, h, kp, &peer_id, scope, label.unwrap_or(""), my_label, addrs, ttl_ms, origin)?;
    Ok(match deliver(p, h, node, pid, &dial, x.offer()) {
        Ok(Delivered::Answered(x)) => link(LinkStatus::Connected, Some(x), format!("connected to {peer}")),
        Ok(Delivered::Kept(_)) => link(LinkStatus::Pending, Some(x), format!("waiting for {peer} to accept")),
        Err(e) => {
            let _ = h.set_status(x.id(), Status::Revoked, "me");
            link(LinkStatus::Failed, Some(x), e)
        }
    })
}

// ---------- answering ----------

/// Take a token: an answer completes our offer; an offer is answered here (kept first if it
/// is new) and the answer delivered when a node is given. An answer the offerer refuses ends
/// the exchange.
#[allow(clippy::too_many_arguments)]
pub fn accept_token<H: Host, Q: Ask, S: Say>(
    p: &Profile,
    h: &H,
    node: Option<&Node<Q, S>>,
    kp: &Keypair,
    token: &str,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    origin: &str,
) -> R<Accepted<H::Rec>, H> {
    match p.parse_token(token)? {
        Token::Answer(a) => {
            let x = complete_offer(p, h, &a, token)?;
            if !label.is_empty() {
                h.relabel(x.id(), label)?;
            }
            let x = h.get(x.id())?.unwrap_or(x);
            h.note(
                origin,
                "net.exchange.complete",
                format!("key exchange with {} complete", x.title()),
                json!({ "exchange": x.id(), "peer": x.peer_id(), "fingerprint": x.fingerprint() }),
            );
            Ok(Accepted { exchange: x, answer: None, delivered: None })
        }
        Token::Offer(o) => {
            let from = p.verify_offer(&o, now_ms())?;
            if let Some(x) = h.get(&o.id)?
                && x.status() != Status::Incoming
            {
                return Err(bad("exchange", format!("this offer was already {}", x.status().as_str())).into());
            }
            let (a, d) = p.answer(kp, &o, my_label, addrs, now_ms())?;
            let atoken = p.answer_token(&a)?;
            if h.get(&o.id)?.is_none() {
                h.insert(
                    &NewRec {
                        id: &o.id,
                        label,
                        role: Role::Answerer,
                        status: Status::Incoming,
                        peer_id: &o.peer,
                        peer_label: &o.label,
                        scope: &o.scope,
                        offer: token,
                        expires_at: o.expires,
                    },
                    None,
                )?;
            } else if !label.is_empty() {
                h.relabel(&o.id, label)?;
            }
            let x = h.complete(&o.id, &o.peer, &o.label, &atoken, &d)?;
            h.saw(&o.peer, Some(&o.label), &o.addrs, None, "exchange")?;
            h.completed(&x)?;
            let mut refused: Option<String> = None;
            let delivered = node.map(|n| {
                let addrs = peer_addrs(p, h, &from.to_string(), &[]);
                match n.request(from, &addrs, Q::answer(atoken.clone())).map(Say::said) {
                    Ok(Said::Ok(msg)) => Ok(msg),
                    Ok(Said::Error(msg)) => {
                        refused = Some(msg.clone());
                        Err(msg)
                    }
                    Ok(_) => Err("unexpected answer".into()),
                    Err(e) => Err(plain(&e.to_string())),
                }
            });
            if matches!(delivered, Some(Ok(_))) {
                h.answer_delivered(x.id())?;
            }
            h.note(
                origin,
                "net.exchange.accept",
                format!("key exchange with {} accepted", x.title()),
                json!({ "exchange": x.id(), "peer": x.peer_id(), "fingerprint": x.fingerprint(), "delivered": delivered.as_ref().map(|d| d.is_ok()) }),
            );
            // their node turned the answer down (a revoked, used or unknown invite): it is over
            let x = match refused {
                Some(_) => {
                    let x = h.set_status(x.id(), Status::Revoked, "peer")?;
                    h.note(
                        origin,
                        "net.exchange.revoke",
                        format!("{} turned the invite down", x.title()),
                        json!({ "exchange": x.id(), "peer": x.peer_id(), "by": "peer" }),
                    );
                    x
                }
                None => x,
            };
            Ok(Accepted { exchange: x, answer: Some(atoken), delivered })
        }
    }
}

// ---------- the receiving node ----------

/// A peer delivered an offer: check it was signed by the sender, let the host admit it, and
/// keep it as incoming (an offer already known is left as it is).
pub fn incoming_offer<H: Host>(p: &Profile, h: &H, from: &PeerId, token: &str) -> R<String, H> {
    let Token::Offer(o) = p.parse_token(token)? else {
        return Err(bad("token", "expected an offer").into());
    };
    let signer = p.verify_offer(&o, now_ms())?;
    if signer != *from {
        return Err(bad("exchange", "the offer was sent by a different node than the one that signed it").into());
    }
    if let Some(x) = h.get(&o.id)? {
        return Ok(format!("already known ({})", x.status().as_str()));
    }
    h.admit(&o)?;
    let incoming = h.records()?.into_iter().filter(|x| x.status() == Status::Incoming).count();
    if incoming >= h.max_incoming() {
        return Err(bad("exchange", "too many pending offers on this node").into());
    }
    let offer: String = token.split_whitespace().collect();
    let x = h.insert(
        &NewRec {
            id: &o.id,
            label: "",
            role: Role::Answerer,
            status: Status::Incoming,
            peer_id: &o.peer,
            peer_label: &o.label,
            scope: &o.scope,
            offer: &offer,
            expires_at: o.expires,
        },
        None,
    )?;
    h.saw(&o.peer, Some(&o.label), &o.addrs, None, "exchange")?;
    if let Some(msg) = h.kept(&o, &x)? {
        return Ok(msg);
    }
    h.note(
        "network",
        "net.exchange.incoming",
        format!("key exchange offered by {} ({})", x.title(), short_peer_id(from)),
        json!({ "exchange": x.id(), "peer": o.peer, "label": o.label }),
    );
    Ok("offer received; it waits to be accepted".into())
}

/// A peer delivered the answer to one of our offers: complete it.
pub fn incoming_answer<H: Host>(p: &Profile, h: &H, from: &PeerId, token: &str) -> R<String, H> {
    let Token::Answer(a) = p.parse_token(token)? else {
        return Err(bad("token", "expected an answer").into());
    };
    if a.peer != from.to_string() {
        return Err(bad("exchange", "the answer was sent by a different node than the one that signed it").into());
    }
    let x = complete_offer(p, h, &a, token)?;
    h.note(
        "network",
        "net.exchange.complete",
        format!("key exchange with {} complete", x.title()),
        json!({ "exchange": x.id(), "peer": x.peer_id(), "fingerprint": x.fingerprint() }),
    );
    Ok("exchange complete".into())
}

/// What the host weighs before answering a delivered offer on the spot.
#[derive(Debug, Clone)]
pub struct Facts {
    /// The offer's scope kind.
    pub kind: String,
    /// The peer's key is pinned here and we did not end it.
    pub known: bool,
    /// Our own open offers of the same kind to the peer (we both asked at once).
    pub crossing: usize,
    /// An active exchange of the same kind with the peer already.
    pub already: bool,
}

/// A `connect` request: keep the offer ([`incoming_offer`]), and answer it on the spot when
/// `decide` says so. When both nodes offered at once and this one answers, its own offers
/// go (theirs is the one). `me` gives the keypair and addresses to answer with.
pub fn connect_in<H: Host>(
    p: &Profile,
    h: &H,
    from: &PeerId,
    token: &str,
    my_label: &str,
    me: impl FnOnce() -> R<(Keypair, Vec<String>), H>,
    decide: impl FnOnce(&Facts) -> bool,
) -> R<Said, H> {
    let msg = incoming_offer(p, h, from, token)?;
    let Token::Offer(o) = p.parse_token(token)? else { return Err(bad("token", "expected an offer").into()) };
    let Some(x) = h.get(&o.id)? else { return Ok(Said::Ok(msg)) };
    if x.status().active() && x.role() == Role::Answerer {
        // asked again: the same answer
        if let Some(a) = x.answer() {
            h.answer_delivered(x.id())?;
            return Ok(Said::Answered(a.to_string()));
        }
    }
    if x.status() != Status::Incoming {
        return Ok(Said::Ok(msg));
    }
    let peer = from.to_string();
    let kind = crate::profile::scope_kind(&o.scope).to_string();
    let same = |y: &H::Rec| crate::profile::scope_kind(y.scope()) == kind;
    let all = h.records()?;
    let ours: Vec<&H::Rec> =
        all.iter().filter(|y| y.id() != x.id() && y.peer_id() == Some(&peer) && y.status() == Status::Offered && same(y)).collect();
    let already = all.iter().any(|y| y.peer_id() == Some(&peer) && y.status().active() && same(y));
    let facts = Facts { kind, known: known(h, &peer), crossing: ours.len(), already };
    if !decide(&facts) {
        return Ok(Said::Ok("request received; it waits for the owner to accept".into()));
    }
    let (kp, addrs) = me()?;
    let a = accept_token::<H, (), ()>(p, h, None, &kp, x.offer(), "", my_label, addrs, "network")?;
    h.answer_delivered(x.id())?;
    // both asked at once: theirs is the one; ours goes
    for y in ours {
        let _ = h.set_status(y.id(), Status::Revoked, "me");
    }
    h.note("network", "net.exchange.auto", format!("connected to {} automatically", a.exchange.title()), json!({ "exchange": x.id(), "peer": peer }));
    Ok(match a.answer {
        Some(t) => Said::Answered(t),
        None => Said::Ok(msg),
    })
}

/// A `poll`: where one of the asker's offers stands here (only its offerer may ask).
pub fn poll_in<H: Host>(h: &H, from: &PeerId, exchange: &str) -> R<Polled, H> {
    let x = h.get(exchange)?.ok_or_else(|| bad("exchange", "unknown request"))?;
    if x.peer_id() != Some(&from.to_string()) || x.role() != Role::Answerer {
        return Err(bad("exchange", "unknown request").into());
    }
    if let Some(r) = h.poll(&x, from) {
        let r = r?;
        if r.1.is_some() {
            h.answer_delivered(x.id())?;
        }
        return Ok(r);
    }
    Ok(match x.status() {
        st if st.active() => {
            h.answer_delivered(x.id())?;
            ("connected".into(), x.answer().map(str::to_string))
        }
        Status::Revoked => ("declined".into(), None),
        _ => ("pending".into(), None),
    })
}

// ---------- keeping things moving ----------

/// Ask peers about our open offers: an answer completes them (if signed by that peer), a
/// decline ends them. `own` takes records the host settles itself (it returns their lines).
/// Unreachable peers are skipped until next time. One line per change.
pub fn settle_offers<H: Host, Q: Ask, S: Say>(
    p: &Profile,
    h: &H,
    node: &Node<Q, S>,
    relays: &[String],
    origin: &str,
    mut own: impl FnMut(&H::Rec) -> Option<Vec<String>>,
) -> Vec<String> {
    let mut out = vec![];
    let now = now_ms();
    let Ok(xs) = h.records() else { return out };
    for x in xs.iter().filter(|x| x.status() == Status::Offered && x.expires_at().is_none_or(|e| e > now)) {
        let Some(peer) = x.peer_id().and_then(|p| p.parse::<PeerId>().ok()) else { continue };
        if let Some(lines) = own(x) {
            out.extend(lines);
            continue;
        }
        let addrs = peer_addrs(p, h, &peer.to_string(), relays);
        match node.request(peer, &addrs, Q::poll(x.id().to_string())).map(Say::said) {
            Ok(Said::Poll { answer: Some(t), .. }) => {
                if let Ok(Token::Answer(a)) = p.parse_token(&t)
                    && a.peer == peer.to_string()
                    && let Ok(x) = complete_offer(p, h, &a, &t)
                {
                    h.note(
                        origin,
                        "net.exchange.complete",
                        format!("key exchange with {} complete", x.title()),
                        json!({ "exchange": x.id(), "peer": x.peer_id(), "auto": true }),
                    );
                    out.push(format!("connected: {} ({})", x.title(), x.purpose()));
                }
            }
            Ok(Said::Poll { status, .. })
                if (status == "declined" || status == "rejected") && h.set_status(x.id(), Status::Revoked, "peer").is_ok() =>
            {
                out.push(format!("declined: {} ({})", x.title(), x.purpose()));
            }
            _ => {}
        }
    }
    out
}

/// Deliver our answers that have not reached their offerer yet.
pub fn deliver_answers<H: Host, Q: Ask, S: Say>(p: &Profile, h: &H, node: &Node<Q, S>, relays: &[String]) -> Vec<String> {
    let mut out = vec![];
    if let Ok(xs) = h.undelivered() {
        for x in xs {
            let Some(peer) = x.peer_id().and_then(|p| p.parse::<PeerId>().ok()) else { continue };
            let Some(t) = x.answer().map(str::to_string) else { continue };
            let addrs = peer_addrs(p, h, &peer.to_string(), relays);
            match node.request(peer, &addrs, Q::answer(t)).map(Say::said) {
                Ok(Said::Ok(_)) => {
                    let _ = h.answer_delivered(x.id());
                    out.push(format!("{} got our answer ({})", x.title(), x.purpose()));
                }
                Ok(Said::Error(msg)) if msg.contains("already") => {
                    let _ = h.answer_delivered(x.id());
                }
                _ => {}
            }
        }
    }
    out
}

/// The answer side of the wire, for a host's node handler: `connect`, `offer`, `answer` and
/// `poll` requests, answered as [`Said`] (`Ok`, `Answered`, `Poll`).
pub fn handle<H: Host>(
    p: &Profile,
    h: &H,
    from: &PeerId,
    req: Asked,
    my_label: &str,
    me: impl FnOnce() -> R<(Keypair, Vec<String>), H>,
    decide: impl FnOnce(&Facts) -> bool,
) -> R<Said, H> {
    match req {
        Asked::Connect(t) => connect_in(p, h, from, &t, my_label, me, decide),
        Asked::Offer(t) => incoming_offer(p, h, from, &t).map(Said::Ok),
        Asked::Answer(t) => incoming_answer(p, h, from, &t).map(Said::Ok),
        Asked::Poll(x) => poll_in(h, from, &x).map(|(status, answer)| Said::Poll { status, answer }),
    }
}

/// An invitation request, as the receiving node reads it.
pub enum Asked {
    Connect(String),
    Offer(String),
    Answer(String),
    Poll(String),
}

// `accept_token` with no node needs message types; these never travel.
impl Ask for () {
    fn hello() -> Self {}
    fn connect(_: String) -> Self {}
    fn offer(_: String) -> Self {}
    fn answer(_: String) -> Self {}
    fn poll(_: String) -> Self {}
}
impl Say for () {
    fn said(self) -> Said {
        Said::Other
    }
}

/// Addresses fit for offers and invite links: shareable, one circuit per relay, without IPv6
/// link-local or Docker bridge addresses (they make links longer and fail elsewhere).
pub fn offer_addrs(all: &[String]) -> Vec<String> {
    let keep: Vec<Multiaddr> = parse_addrs(all).into_iter().filter(shareable).collect();
    let mut v: Vec<String> = one_circuit_per_relay(keep, &[]).iter().map(|a| a.to_string()).collect();
    v.retain(|a| !crate::addr::link_local(a) && !crate::addr::docker_bridge(a));
    v
}
