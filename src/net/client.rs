//! Our side: making and accepting exchanges, asking peers for what they share, searching
//! across the network and dark shards. The CLI and TUI call these; every change is
//! recorded in the action history.
use super::exchange::{self, Answer, Offer, Token};
use super::node::{Node, addr_peer, parse_addrs};
use super::proto::{Item, Req, Resp, ShardBlob, SharedItem, Wire, WireResp};
use super::service;
use crate::crypto::{self, Key32};
use crate::error::{Error, Result};
use crate::live::Found;
use crate::store::{Entrusted, Exchange, ExchangeStatus, NewAction, NewExchange, SearchSpec, Share, ShareKind, Store, now_ms};
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};
use secrecy::SecretString;
use serde::Serialize;

pub use super::exchange::DEFAULT_TTL_MS;

/// The setting holding the addresses `mnnn net start` last listened on (for offers made
/// while the node is not running).
pub const ADDRS_SETTING: &str = "net.addrs";

fn record(s: &Store, origin: &str, kind: &str, summary: String, params: serde_json::Value) {
    s.note_action(&NewAction::new(kind, origin, summary).params(params));
}

/// The error for operations that need a running node.
pub fn no_node() -> Error {
    Error::Other("network: no node (start one with `mnnn net start`, or the TUI's Network ▸ Start node)".into())
}

/// Addresses we know for a peer: from sightings, its exchange tokens, and through relays.
pub fn peer_addrs(s: &Store, peer: &str, relays: &[String]) -> Vec<Multiaddr> {
    let mut v: Vec<String> = s.net_peer(peer).ok().flatten().map(|p| p.addrs).unwrap_or_default();
    if let Ok(xs) = s.exchanges() {
        for x in xs.into_iter().filter(|x| x.peer_id.as_deref() == Some(peer)) {
            let tokens = [Some(x.offer.clone()), x.answer.clone()];
            for t in tokens.into_iter().flatten() {
                match exchange::parse_token(&t) {
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
    for r in parse_addrs(relays) {
        if addr_peer(&r).is_some() {
            let c = r.with(Protocol::P2pCircuit);
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// A peer by full address (`…/p2p/ID`), peer id, or a known peer's label or short id.
pub fn resolve(s: &Store, target: &str, relays: &[String]) -> Result<(PeerId, Vec<Multiaddr>)> {
    let t = target.trim();
    if t.starts_with('/') {
        let a: Multiaddr = t.parse().map_err(|e| Error::invalid("address", format!("{t}: {e}")))?;
        let p = addr_peer(&a).ok_or_else(|| Error::invalid("address", "needs the peer id at the end (…/p2p/12D3Koo…)"))?;
        let mut v = vec![a];
        v.extend(peer_addrs(s, &p.to_string(), relays));
        return Ok((p, v));
    }
    let peer = match t.parse::<PeerId>() {
        Ok(p) => p.to_string(),
        Err(_) => s.find_net_peer(t)?.peer_id,
    };
    let p: PeerId = peer.parse().map_err(|_| Error::invalid("peer", "bad peer id"))?;
    Ok((p, peer_addrs(s, &peer, relays)))
}

/// Say hello to a peer: its label and capabilities, remembered.
pub fn hello(s: &Store, node: &Node, target: &str, label: Option<&str>, relays: &[String]) -> Result<crate::store::NetPeer> {
    let (p, addrs) = resolve(s, target, relays)?;
    match node.request(p, &addrs, Wire::Hello)? {
        WireResp::Hello { label: l, agent, caps } => {
            let addrs: Vec<String> = addrs.iter().filter(|a| !a.iter().any(|p| p == Protocol::P2pCircuit)).map(|a| a.to_string()).collect();
            let name = label.map(str::to_string).filter(|x| !x.is_empty()).unwrap_or(l);
            let peer = s.upsert_net_peer(&p.to_string(), Some(&name), &addrs, None, Some(&agent), "manual")?;
            let _ = caps;
            Ok(peer)
        }
        WireResp::Error { msg } => Err(Error::Other(format!("peer: {msg}"))),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// A new offer.
#[derive(Debug, Clone, Serialize)]
pub struct Created {
    pub exchange: Exchange,
    pub token: String,
    /// Sent to the peer (`--peer`): Ok, or why not.
    pub delivered: Option<std::result::Result<String, String>>,
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    to: Option<&str>,
    ttl_ms: i64,
    relays: &[String],
    origin: &str,
) -> Result<Created> {
    create_scoped(s, node, kp, label, my_label, addrs, to, ttl_ms, relays, exchange::node_scope(), origin)
}

/// [`create`] with a scope (a membership of an organization).
#[allow(clippy::too_many_arguments)]
pub fn create_scoped(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    to: Option<&str>,
    ttl_ms: i64,
    relays: &[String],
    scope: serde_json::Value,
    origin: &str,
) -> Result<Created> {
    let target = match to {
        Some(t) => Some(resolve(s, t, relays)?),
        None => None,
    };
    let (offer, secret) = Offer::new(kp, my_label, addrs, scope, now_ms(), ttl_ms)?;
    let token = offer.token()?;
    let x = s.insert_exchange(
        &NewExchange {
            id: offer.id.clone(),
            label: label.to_string(),
            role: "offerer".into(),
            status: Some(ExchangeStatus::Offered),
            peer_id: target.as_ref().map(|(p, _)| p.to_string()),
            peer_label: String::new(),
            scope: offer.scope.clone(),
            offer: token.clone(),
            expires_at: Some(offer.expires),
        },
        Some(&secret[..]),
    )?;
    let delivered = match (&target, node) {
        (Some((p, a)), Some(n)) => Some(match super::auto::deliver(s, n, *p, a, &token) {
            Ok(super::auto::Delivered::Kept(msg)) => Ok(msg),
            Ok(super::auto::Delivered::Answered(_)) => Ok("connected".into()),
            Err(e) => Err(e),
        }),
        (Some(_), None) => Some(Err("no node running".into())),
        _ => None,
    };
    record(
        s,
        origin,
        "net.exchange.create",
        format!("key exchange offered{}", if label.is_empty() { String::new() } else { format!(" ({label})") }),
        serde_json::json!({ "exchange": x.id, "label": label, "peer": x.peer_id, "delivered": delivered.as_ref().map(|d| d.is_ok()) }),
    );
    let x = s.exchange(&x.id)?.unwrap_or(x);
    Ok(Created { exchange: x, token, delivered })
}

/// An accepted offer or answer.
#[derive(Debug, Clone, Serialize)]
pub struct Accepted {
    pub exchange: Exchange,
    /// When we answered an offer: the answer token (for the offerer, if not delivered).
    pub answer: Option<String>,
    pub delivered: Option<std::result::Result<String, String>>,
}

/// Accept an offer (a token or an incoming exchange) or complete ours with an answer token.
#[allow(clippy::too_many_arguments)]
pub fn accept(
    s: &Store,
    node: Option<&Node>,
    kp: &Keypair,
    input: &str,
    label: &str,
    my_label: &str,
    addrs: Vec<String>,
    origin: &str,
) -> Result<Accepted> {
    let token = if exchange::is_token(input) {
        exchange::unlink(input)
    } else {
        let x = s.find_exchange(input)?;
        match x.status {
            ExchangeStatus::Incoming => x.offer.clone(),
            ExchangeStatus::Offered => {
                return Err(Error::invalid("exchange", "this is our own offer: paste the answer token the other side printed"));
            }
            st => return Err(Error::invalid("exchange", format!("already {}", st.as_str()))),
        }
    };
    match exchange::parse_token(&token)? {
        Token::Answer(a) => {
            let x = service::complete_offer(s, &a, &token)?;
            if !label.is_empty() {
                s.label_exchange(&x.id, label)?;
            }
            let x = s.exchange(&x.id)?.unwrap_or(x);
            record(
                s,
                origin,
                "net.exchange.complete",
                format!("key exchange with {} complete", x.title()),
                serde_json::json!({ "exchange": x.id, "peer": x.peer_id, "fingerprint": x.fingerprint }),
            );
            Ok(Accepted { exchange: x, answer: None, delivered: None })
        }
        Token::Offer(o) => {
            let from = o.verify(now_ms())?;
            if let Some(x) = s.exchange(&o.id)?
                && x.status != ExchangeStatus::Incoming
            {
                return Err(Error::invalid("exchange", format!("this offer was already {}", x.status.as_str())));
            }
            let (a, d) = Answer::new(kp, &o, my_label, addrs, now_ms())?;
            let atoken = a.token()?;
            if s.exchange(&o.id)?.is_none() {
                s.insert_exchange(
                    &NewExchange {
                        id: o.id.clone(),
                        label: label.to_string(),
                        role: "answerer".into(),
                        status: Some(ExchangeStatus::Incoming),
                        peer_id: Some(o.peer.clone()),
                        peer_label: o.label.clone(),
                        scope: o.scope.clone(),
                        offer: token.clone(),
                        expires_at: Some(o.expires),
                    },
                    None,
                )?;
            } else if !label.is_empty() {
                s.label_exchange(&o.id, label)?;
            }
            let x = s.complete_exchange(&o.id, &o.peer, &o.label, &atoken, &d.key, &d.short_code, &d.fingerprint)?;
            s.upsert_net_peer(&o.peer, Some(&o.label), &o.addrs, None, None, "exchange")?;
            crate::org::net::answered(s, &x)?;
            crate::review::net::answered(s, &x)?;
            let delivered = node.map(|n| {
                let addrs = peer_addrs(s, &from.to_string(), &[]);
                match n.request(from, &addrs, Wire::Answer { token: atoken.clone() }) {
                    Ok(WireResp::Ok { msg }) => Ok(msg),
                    Ok(WireResp::Error { msg }) => Err(msg),
                    Ok(_) => Err("unexpected answer".into()),
                    Err(e) => Err(super::auto::plain(&e.to_string())),
                }
            });
            if matches!(delivered, Some(Ok(_))) {
                s.mark_answer_delivered(&x.id)?;
            }
            record(
                s,
                origin,
                "net.exchange.accept",
                format!("key exchange with {} accepted", x.title()),
                serde_json::json!({ "exchange": x.id, "peer": x.peer_id, "fingerprint": x.fingerprint, "delivered": delivered.as_ref().map(|d| d.is_ok()) }),
            );
            Ok(Accepted { exchange: x, answer: Some(atoken), delivered })
        }
    }
}

/// Record that the short codes matched.
pub fn verify(s: &Store, key: &str, code: &str, origin: &str) -> Result<Exchange> {
    let x = s.find_exchange(key)?;
    if !x.status.active() {
        return Err(Error::invalid("exchange", format!("is {}, not complete", x.status.as_str())));
    }
    let want = x.short_code.clone().unwrap_or_default();
    if exchange::normalize_code(code) != want {
        return Err(Error::invalid(
            "short code",
            "does not match: someone may be in the middle. Revoke this exchange (`mnnn net exchange revoke`) and make a new one",
        ));
    }
    let x = s.set_exchange_status(&x.id, ExchangeStatus::Verified, None)?;
    record(s, origin, "net.exchange.verify", format!("key exchange with {} verified", x.title()), serde_json::json!({ "exchange": x.id }));
    Ok(x)
}

/// End an exchange (telling the peer when it can be reached). The record stays; the key goes.
pub fn revoke(
    s: &Store,
    node: Option<&Node>,
    key: &str,
    relays: &[String],
    origin: &str,
) -> Result<(Exchange, Option<std::result::Result<(), String>>)> {
    let x = s.find_exchange(key)?;
    if x.status == ExchangeStatus::Revoked {
        return Ok((x, None));
    }
    let told = match (node, x.status.active()) {
        (Some(n), true) => Some(send(s, n, &x, Req::Revoke, relays).map(|_| ()).map_err(|e| e.to_string())),
        _ => None,
    };
    let x = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("me"))?;
    record(
        s,
        origin,
        "net.exchange.revoke",
        format!("key exchange with {} revoked", x.title()),
        serde_json::json!({ "exchange": x.id, "by": "me", "told": told.as_ref().map(|t| t.is_ok()) }),
    );
    Ok((x, told))
}

/// Send a sealed request over an exchange and open the answer.
pub fn send(s: &Store, node: &Node, x: &Exchange, req: Req, relays: &[String]) -> Result<Resp> {
    if !x.status.active() {
        return Err(Error::invalid("exchange", format!("{} is {}", x.title(), x.status.as_str())));
    }
    let peer: PeerId = x.peer_id.as_deref().and_then(|p| p.parse().ok()).ok_or_else(|| Error::invalid("exchange", "no peer"))?;
    let key = s.exchange_key(&x.id)?.ok_or_else(|| Error::invalid("exchange", "no key"))?;
    let me = node.peer_id();
    let sealed = exchange::seal_msg(&key, &x.id, &me, &peer, &serde_json::to_vec(&req)?)?;
    let addrs = peer_addrs(s, &peer.to_string(), relays);
    match node.request(peer, &addrs, Wire::Sealed { exchange: x.id.clone(), sealed })? {
        WireResp::Sealed { sealed } => {
            let plain = exchange::open_msg(&key, &x.id, &peer, &me, &sealed)?;
            let r: Resp = serde_json::from_slice(&plain).map_err(|e| Error::Other(format!("peer: bad answer ({e})")))?;
            match r {
                Resp::Error { msg } => Err(Error::Other(format!("{}: {msg}", x.title()))),
                r => Ok(r),
            }
        }
        WireResp::Error { msg } => Err(Error::Other(format!("{}: {msg}", x.title()))),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// The exchange to use for a key (an exchange, or a peer with one active exchange).
pub fn exchange_for(s: &Store, key: &str) -> Result<Exchange> {
    if let Ok(x) = s.find_exchange(key) {
        return Ok(x);
    }
    let p = s.find_net_peer(key)?;
    s.active_exchanges_with(&p.peer_id)?.into_iter().next().ok_or_else(|| {
        Error::invalid("exchange", format!("no active key exchange with {}", if p.label.is_empty() { p.short_id() } else { &p.label }))
    })
}

/// What a peer shares with us.
pub fn remote_list(s: &Store, node: &Node, x: &Exchange, relays: &[String]) -> Result<Vec<SharedItem>> {
    match send(s, node, x, Req::List, relays)? {
        Resp::Shared { items } => Ok(items),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// Fetch one shared item.
pub fn fetch(s: &Store, node: &Node, x: &Exchange, item: &str, format: Option<&str>, relays: &[String], origin: &str) -> Result<Item> {
    match send(s, node, x, Req::Fetch { share: item.to_string(), format: format.map(str::to_string) }, relays)? {
        Resp::Item { item: it } => {
            record(
                s,
                origin,
                "net.fetch",
                format!("fetched {} {} from {}", it.kind.label(), it.name, x.title()),
                serde_json::json!({ "exchange": x.id, "kind": it.kind, "name": it.name, "format": it.format, "bytes": it.data.len() }),
            );
            Ok(it)
        }
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// One node's answer to a network search.
#[derive(Debug, Clone, Serialize)]
pub struct Origin {
    /// `local` or the peer's label.
    pub origin: String,
    pub peer: Option<String>,
    pub exchange: Option<String>,
    pub found: Vec<Found>,
    pub truncated: bool,
    pub error: Option<String>,
}

/// Run a search here and on every peer with an active exchange (or the `only` ones);
/// each peer searches only what it shares with us.
#[allow(clippy::too_many_arguments)]
pub fn search(
    s: &Store,
    node: Option<&Node>,
    spec: &SearchSpec,
    only: &[String],
    local: bool,
    limit: usize,
    relays: &[String],
    origin: &str,
) -> Result<Vec<Origin>> {
    let mut out = vec![];
    if local {
        out.push(Origin { origin: "local".into(), peer: None, exchange: None, found: crate::live::run(s, spec)?, truncated: false, error: None });
    }
    let mut xs: Vec<Exchange> = if only.is_empty() {
        s.exchanges()?.into_iter().filter(|x| x.status.active()).collect()
    } else {
        only.iter().map(|k| exchange_for(s, k)).collect::<Result<_>>()?
    };
    // one exchange per peer
    let mut seen = std::collections::HashSet::new();
    xs.retain(|x| seen.insert(x.peer_id.clone()));
    for x in xs {
        let mut o = Origin { origin: x.title(), peer: x.peer_id.clone(), exchange: Some(x.id.clone()), found: vec![], truncated: false, error: None };
        match node {
            None => o.error = Some("no node".into()),
            Some(n) => match send(s, n, &x, Req::Search { spec: spec.clone(), limit }, relays) {
                Ok(Resp::Found { results, truncated }) => {
                    o.found = results;
                    o.truncated = truncated;
                }
                Ok(_) => o.error = Some("unexpected answer".into()),
                Err(e) => o.error = Some(e.to_string()),
            },
        }
        out.push(o);
    }
    let peers = out.iter().filter(|o| o.peer.is_some()).count();
    let hits: usize = out.iter().map(|o| o.found.len()).sum();
    record(
        s,
        origin,
        "net.search",
        format!("searched {} across {peers} peer(s): {hits} match(es)", spec.describe()),
        serde_json::json!({ "spec": spec, "peers": peers, "matches": hits }),
    );
    Ok(out)
}

/// Run one live search a peer shares.
pub fn run_shared(s: &Store, node: &Node, x: &Exchange, item: &str, limit: usize, relays: &[String]) -> Result<(Vec<Found>, bool)> {
    match send(s, node, x, Req::RunShared { share: item.to_string(), limit }, relays)? {
        Resp::Found { results, truncated } => Ok((results, truncated)),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

// ---------- sharing ----------

/// An item by kind and id/name: its id and name.
pub fn find_item(s: &Store, kind: ShareKind, key: &str) -> Result<(String, String)> {
    Ok(match kind {
        ShareKind::Model => {
            let m = s.find_model(key)?;
            (m.id, m.name)
        }
        ShareKind::LiveSearch => {
            let l = s.find_live_search(key)?;
            (l.id, l.name)
        }
        ShareKind::Journal => {
            let j = s.find_journal(key)?;
            (j.id, j.name)
        }
        ShareKind::Project => {
            let p = s.find_project(key)?;
            (p.id, p.name)
        }
        ShareKind::Report => {
            let r = s.find_report(key)?;
            (r.id, r.name)
        }
        ShareKind::Code => {
            let c = s.find_code_space(key)?;
            (c.id, c.name)
        }
    })
}

/// Share an item with some exchanges (or every active one when `with` is empty).
pub fn share(s: &Store, kind: ShareKind, key: &str, with: &[String], origin: &str) -> Result<Vec<(Share, bool)>> {
    let (id, name) = find_item(s, kind, key)?;
    let mut out = vec![];
    if with.is_empty() {
        out.push(s.add_share(kind, &id, &name, None)?);
    } else {
        for w in with {
            let x = super::auto::link_for(s, w)?;
            if x.status == ExchangeStatus::Revoked {
                return Err(Error::invalid("exchange", format!("{} is revoked", x.title())));
            }
            out.push(s.add_share(kind, &id, &name, Some(&x.id))?);
        }
    }
    let to: Vec<String> = if with.is_empty() { vec!["everyone with an exchange".into()] } else { with.to_vec() };
    record(
        s,
        origin,
        "net.share",
        format!("shared {} {} with {}", kind.label(), name, to.join(", ")),
        serde_json::json!({ "kind": kind, "item": id, "name": name, "with": to }),
    );
    Ok(out)
}

/// Stop sharing (everywhere, or with some exchanges).
pub fn unshare(s: &Store, kind: ShareKind, key: &str, with: &[String], origin: &str) -> Result<Vec<Share>> {
    let (id, name) = match find_item(s, kind, key) {
        Ok(v) => v,
        // the item may be gone already: match the share itself
        Err(e) => match s.shares()?.into_iter().find(|x| x.kind == kind && (x.ref_id == key || x.name.eq_ignore_ascii_case(key))) {
            Some(x) => (x.ref_id, x.name),
            None => return Err(e),
        },
    };
    let mut gone = vec![];
    if with.is_empty() {
        gone = s.remove_share(kind, &id, None)?;
    } else {
        for w in with {
            let x = exchange_for(s, w)?;
            gone.extend(s.remove_share(kind, &id, Some(&x.id))?);
        }
    }
    if gone.is_empty() {
        return Err(Error::invalid("item", format!("{} {name} is not shared{}", kind.label(), if with.is_empty() { "" } else { " there" })));
    }
    record(
        s,
        origin,
        "net.unshare",
        format!("stopped sharing {} {}", kind.label(), name),
        serde_json::json!({ "kind": kind, "item": id, "name": name, "with": with }),
    );
    Ok(gone)
}

// ---------- dark shards ----------

fn shard_aad(set: &str, index: u32) -> Vec<u8> {
    format!("muninn/net/shard/v1|{set}|{index}").into_bytes()
}

/// Where the shards went.
#[derive(Debug, Clone, Serialize)]
pub struct Entrust {
    pub set: String,
    pub threshold: usize,
    pub placed: Vec<Entrusted>,
}

/// Split the master key `threshold`-of-n over n exchanges (one shard each, different
/// peers). Each shard is sealed with the shard password before it leaves, and travels
/// inside the exchange's sealed channel: custodians never see a usable shard.
#[allow(clippy::too_many_arguments)]
pub fn entrust(
    s: &Store,
    node: &Node,
    master: &Key32,
    password: &SecretString,
    to: &[String],
    threshold: usize,
    relays: &[String],
    origin: &str,
) -> Result<Entrust> {
    let mut xs: Vec<Exchange> = vec![];
    for k in to {
        let x = exchange_for(s, k)?;
        if !x.status.active() {
            return Err(Error::invalid("exchange", format!("{} is {}", x.title(), x.status.as_str())));
        }
        if xs.iter().any(|y| y.peer_id == x.peer_id) {
            return Err(Error::invalid("exchange", format!("{} appears twice: each shard must go to a different node", x.title())));
        }
        xs.push(x);
    }
    let n = xs.len();
    if n < 2 {
        return Err(Error::invalid("--to", "name at least two exchanges (different nodes)"));
    }
    let k = if threshold == 0 { n.div_ceil(2).max(2).min(n) } else { threshold };
    let shards = crypto::split_shards_k(master, k, n)?;
    let claim = crypto::shard_claim(password)?;
    let mut placed = vec![];
    let mut set = String::new();
    for (x, sh) in xs.iter().zip(shards.iter()) {
        let (set_id, t, index) = crypto::shard_meta(sh)?;
        set = set_id.clone();
        let sealed = crypto::password_seal(password, sh.as_bytes(), &shard_aad(&set_id, index))?;
        let blob = ShardBlob { set: set_id.clone(), index, threshold: t as u32, total: n as u32, blob: serde_json::to_string(&sealed)? };
        match send(s, node, x, Req::ShardStore { shard: blob, claim: claim.clone() }, relays)? {
            Resp::Stored => {}
            _ => return Err(Error::Other(format!("{}: did not store the shard", x.title()))),
        }
        let e = Entrusted {
            id: crate::store::net::new_held_id(),
            set_id,
            index,
            threshold: t as u32,
            total: n as u32,
            exchange_id: x.id.clone(),
            peer_id: x.peer_id.clone().unwrap_or_default(),
            created_at: now_ms(),
        };
        s.add_entrusted(&e)?;
        placed.push(e);
    }
    record(
        s,
        origin,
        "net.shard.entrust",
        format!("dark shards ({k} of {n}) entrusted to {}", xs.iter().map(|x| x.title()).collect::<Vec<_>>().join(", ")),
        serde_json::json!({ "set": set, "threshold": k, "total": n, "exchanges": xs.iter().map(|x| x.id.clone()).collect::<Vec<_>>() }),
    );
    Ok(Entrust { set, threshold: k, placed })
}

/// What a restore gathered.
#[derive(Debug, Clone, Serialize)]
pub struct Gathered {
    pub set: String,
    pub threshold: u32,
    pub got: usize,
    pub from: Vec<String>,
    pub errors: Vec<String>,
}

/// Ask peers for our shards (by this node's identity and by the password's claim), open
/// them with the shard password and combine them into the master key.
pub fn gather(
    s: &Store,
    node: &Node,
    password: &SecretString,
    set: Option<&str>,
    from: &[String],
    relays: &[String],
    origin: &str,
) -> Result<(Key32, Gathered)> {
    let claim = crypto::shard_claim(password)?;
    let xs: Vec<Exchange> = if from.is_empty() {
        s.exchanges()?.into_iter().filter(|x| x.status.active()).collect()
    } else {
        from.iter().map(|k| exchange_for(s, k)).collect::<Result<_>>()?
    };
    let mut blobs: Vec<(ShardBlob, String)> = vec![];
    let mut errors = vec![];
    for x in &xs {
        let mut got = vec![];
        for req in [Req::ShardFetch { set: set.map(str::to_string) }, Req::ShardClaim { claim: claim.clone() }] {
            match send(s, node, x, req, relays) {
                Ok(Resp::Shards { shards }) => got.extend(shards),
                Ok(_) => {}
                Err(e) => {
                    errors.push(e.to_string());
                    break;
                }
            }
        }
        for b in got {
            if set.is_none_or(|st| b.set == st) && !blobs.iter().any(|(o, _)| o.set == b.set && o.index == b.index) {
                blobs.push((b, x.title()));
            }
        }
    }
    // the set with the most shards that has enough
    let mut sets: Vec<(String, u32, usize)> = vec![];
    for (b, _) in &blobs {
        match sets.iter_mut().find(|(st, _, _)| *st == b.set) {
            Some(e) => e.2 += 1,
            None => sets.push((b.set.clone(), b.threshold, 1)),
        }
    }
    sets.sort_by(|a, b| (b.2 >= b.1 as usize).cmp(&(a.2 >= a.1 as usize)).then(b.2.cmp(&a.2)));
    let Some((set_id, threshold, n)) = sets.first().cloned() else {
        return Err(Error::invalid(
            "dark shards",
            format!("no peer returned a shard{}", if errors.is_empty() { String::new() } else { format!(" ({})", errors.join("; ")) }),
        ));
    };
    if n < threshold as usize {
        return Err(Error::invalid("dark shards", format!("set {set_id}: {n} of the {threshold} shards needed came back")));
    }
    let mut texts = vec![];
    let mut sources = vec![];
    for (b, who) in blobs.iter().filter(|(b, _)| b.set == set_id) {
        let sealed: crypto::PasswordSealed = serde_json::from_str(&b.blob).map_err(|_| Error::Crypto("a shard came back damaged".into()))?;
        let plain =
            crypto::password_open(password, &sealed, &shard_aad(&b.set, b.index)).map_err(|_| Error::Unlock("wrong shard password".into()))?;
        texts.push(String::from_utf8(plain.to_vec()).map_err(|_| Error::Crypto("a shard came back damaged".into()))?);
        sources.push(who.clone());
    }
    let key = crypto::combine_shards(&texts)?;
    for t in texts.iter_mut() {
        zeroize::Zeroize::zeroize(t);
    }
    record(
        s,
        origin,
        "net.shard.restore",
        format!("master key restored from {} dark shard(s) of set {set_id}", sources.len()),
        serde_json::json!({ "set": set_id, "shards": sources.len(), "from": sources }),
    );
    Ok((key, Gathered { set: set_id, threshold, got: sources.len(), from: sources, errors }))
}

/// A peer's capabilities as one short text (for listings).
pub fn caps_text(caps: &[String]) -> String {
    if caps.is_empty() { "-".into() } else { caps.join(",") }
}

/// Reachable addresses: the node's, else those `mnnn net start` last saved.
pub fn my_addrs(s: &Store, node: Option<&Node>) -> Vec<String> {
    if let Some(n) = node {
        let v = n.reachable_addrs();
        if !v.is_empty() {
            return v;
        }
    }
    s.setting(ADDRS_SETTING).ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

/// Whether a token reached the peer, as one line.
pub fn describe_delivery(d: &Option<std::result::Result<String, String>>) -> Option<String> {
    d.as_ref().map(|r| match r {
        Ok(m) => format!("delivered: {m}"),
        Err(e) => format!("not delivered ({e})"),
    })
}

/// For `Resp` matching in callers.
pub fn pong(s: &Store, node: &Node, x: &Exchange, relays: &[String]) -> Result<(String, usize)> {
    match send(s, node, x, Req::Ping, relays)? {
        Resp::Pong { label, shared } => Ok((label, shared)),
        _ => Err(Error::Other("peer: unexpected answer".into())),
    }
}

/// A free name: `name`, else `name (from PEER)`, else numbered.
fn free_name(taken: impl Fn(&str) -> bool, name: &str, from: &str) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let base = format!("{name} (from {from})");
    if !taken(&base) {
        return base;
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| !taken(n)).unwrap_or(base)
}

/// Add a fetched item to this database: projects (archive or JSON), journals (JSON), live
/// searches and code spaces (which must be approved before they run). Names that are
/// taken get " (from PEER)". Returns what was made, in words.
pub fn import_item(s: &mut Store, it: &Item, from: &str, origin: &str) -> Result<String> {
    Ok(import_item_as(s, it, from, origin)?.2)
}

/// [`import_item`], also giving the new thing's id and name.
pub fn import_item_as(s: &mut Store, it: &Item, from: &str, origin: &str) -> Result<(String, String, String)> {
    let made_id: (String, String);
    let made = match (it.kind, it.format.as_str()) {
        (ShareKind::Project, "mnnp" | "json") => {
            let (p, blobs) = crate::export::archive::read_project(&it.data)?;
            let names: Vec<String> = s.projects()?.into_iter().map(|p| p.name.to_lowercase()).collect();
            let name = free_name(|n| names.contains(&n.to_lowercase()), &p.project.name, from);
            let r = s.import_project(&p, Some(&name), blobs.as_ref())?;
            made_id = (s.find_project(&name).map(|p| p.id).unwrap_or_default(), name.clone());
            format!("project {name} ({} journal(s), {} entr(ies))", r.journals.len(), r.entries)
        }
        (ShareKind::Journal, "json") => {
            let f: crate::store::archive::JournalFile =
                serde_json::from_slice(&it.data).map_err(|e| Error::invalid("journal", format!("not a Muninn journal ({e})")))?;
            let names: Vec<String> = s.journals()?.into_iter().map(|j| j.name.to_lowercase()).collect();
            let name = free_name(|n| names.contains(&n.to_lowercase()), &it.name, from);
            let r = s.import_journal(&f, None, Some(&name), &crate::store::archive::Inline)?;
            made_id = (s.find_journal(&name).map(|j| j.id).unwrap_or_default(), name.clone());
            format!("journal {name} ({} entr(ies))", r.entries)
        }
        (ShareKind::LiveSearch, "json") => {
            let v: serde_json::Value = serde_json::from_slice(&it.data)?;
            let ls: crate::store::LiveSearch =
                serde_json::from_value(v["live_search"].clone()).map_err(|e| Error::invalid("live search", format!("damaged ({e})")))?;
            let names: Vec<String> = s.live_searches()?.into_iter().map(|l| l.name.to_lowercase()).collect();
            let name = free_name(|n| names.contains(&n.to_lowercase()), &ls.name, from);
            s.save_live_search(&name, &ls.spec, None)?;
            made_id = (s.find_live_search(&name).map(|l| l.id).unwrap_or_default(), name.clone());
            format!("live search {name}")
        }
        (ShareKind::Code, "json") => {
            let c: crate::store::CodeSpace = serde_json::from_slice(&it.data).map_err(|e| Error::invalid("code space", format!("damaged ({e})")))?;
            let names: Vec<String> = s.code_spaces()?.into_iter().map(|c| c.name.to_lowercase()).collect();
            let name = free_name(|n| names.contains(&n.to_lowercase()), &c.name, from);
            let made = s.create_code_space(&name, &c.description, c.lang, &c.source, c.input.clone())?;
            made_id = (made.id.clone(), made.name.clone());
            format!("code space {} (review and approve it before it runs)", made.name)
        }
        (k, f) => {
            return Err(Error::invalid(
                "import",
                format!("a {} as {f} cannot be imported (projects, journals, live searches and code spaces can); save it with --out", k.label()),
            ));
        }
    };
    record(s, origin, "net.import", format!("imported {made} from {from}"), serde_json::json!({ "kind": it.kind, "name": it.name, "from": from }));
    Ok((made_id.0, made_id.1, made))
}
