//! The transport boundary. Everything collaboration sends between nodes is one of
//! [`CollabReq`] and comes back as one of [`CollabResp`]; the `muninn.live.v1` batches inside
//! are protobuf bytes. [`Transport`] is the only thing the rest of Muninn calls to reach a
//! peer, and [`serve`] the only thing a node answers with, so the carrier can change (today
//! sealed requests over the libp2p key-exchange connection, [`Libp2p`]; a gRPC stream later)
//! without touching sessions, messages or the CLI and TUI.
use super::proto::Batch;
use crate::error::{Error, Result};
use crate::net::proto::serde_bytes_b64;
use crate::store::{Exchange, Store};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A request to a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollabReq {
    /// A live session round: a `muninn.live.v1` `Batch` (protobuf).
    Live {
        #[serde(with = "serde_bytes_b64")]
        batch: Vec<u8>,
    },
    /// What the peer offers you: live sessions, org executables.
    Offers,
    /// A message for the peer.
    Message { id: String, from: String, about: Option<String>, text: String, at: i64 },
    /// Run one of the peer's served executables (an org endpoint); results stream back as a
    /// `run` session.
    Run { org: String, endpoint: String, input: Value },
}

/// A peer's answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CollabResp {
    Live {
        #[serde(with = "serde_bytes_b64")]
        batch: Vec<u8>,
        meta: Value,
    },
    Offers {
        body: Value,
    },
    Delivered,
    /// A run started: its session (sync it to read the results).
    Started {
        session: String,
    },
}

/// Reaching one peer.
pub trait Transport {
    fn call(&self, req: CollabReq) -> Result<CollabResp>;

    /// One live round (see [`super::session::sync_with`]).
    fn live(&self, b: Batch) -> Result<(Batch, Value)> {
        match self.call(CollabReq::Live { batch: b.bytes() })? {
            CollabResp::Live { batch, meta } => Ok((Batch::decode_bytes(&batch)?, meta)),
            _ => Err(unexpected()),
        }
    }
}

pub fn unexpected() -> Error {
    Error::Other("unexpected answer from the peer".into())
}

/// The libp2p carrier: sealed with the key exchange's keys, over the node's connection
/// (direct, hole-punched or relayed).
pub struct Libp2p<'a> {
    pub store: &'a Store,
    pub node: &'a crate::net::node::Node,
    pub exchange: &'a Exchange,
    pub relays: &'a [String],
}

impl Transport for Libp2p<'_> {
    fn call(&self, req: CollabReq) -> Result<CollabResp> {
        use crate::net::proto::{Req, Resp};
        match crate::net::client::send(self.store, self.node, self.exchange, Req::Collab { req }, self.relays)? {
            Resp::Collab { resp } => Ok(resp),
            Resp::Error { msg } => Err(Error::Other(msg)),
            _ => Err(unexpected()),
        }
    }
}

/// The exchange scopes collaboration works over.
pub fn allowed(x: &Exchange) -> bool {
    matches!(crate::net::exchange::scope_kind(&x.scope), "node" | "link" | "organization")
}

/// The active exchange that reaches `peer` (an exchange id or label, a peer label or peer
/// id); with `org`, prefer that org's membership exchange.
pub fn exchange_to(s: &Store, peer: &str, org: Option<&str>) -> Result<Exchange> {
    let k = peer.trim();
    let all: Vec<Exchange> = s.exchanges()?.into_iter().filter(|x| x.status.active() && x.peer_id.is_some() && allowed(x)).collect();
    let pid = match all.iter().find(|x| x.peer_id.as_deref() == Some(k)) {
        Some(x) => x.peer_id.clone(),
        None => match s.find_exchange(k) {
            Ok(x) => x.peer_id.clone(),
            // the node's own label (org memberships are named after the org)
            Err(_) => {
                let named = |x: &&Exchange| {
                    let p = x.peer_id.as_deref().unwrap_or_default();
                    let sc = |f: &str| x.scope.get(f).and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    let (slug, name) =
                        if crate::net::exchange::scope_kind(&x.scope) == "organization" { (sc("slug"), sc("name")) } else { Default::default() };
                    [x.peer_label.clone(), s.net_peer(p).ok().flatten().map(|n| n.label).unwrap_or_default(), slug, name]
                        .iter()
                        .any(|l| !l.is_empty() && l.eq_ignore_ascii_case(k))
                };
                let ids: std::collections::BTreeSet<_> = all.iter().filter(named).filter_map(|x| x.peer_id.clone()).collect();
                match ids.len() {
                    1 => ids.into_iter().next(),
                    0 => return Err(Error::invalid("peer", format!("`{k}`: no active connection by that name"))),
                    _ => return Err(Error::invalid("peer", format!("`{k}` names more than one node; use its exchange or peer id"))),
                }
            }
        },
    };
    let mut to: Vec<Exchange> = all.into_iter().filter(|x| x.peer_id == pid).collect();
    if let Some(o) = org {
        let id = s.find_org(o).map(|o| o.id).unwrap_or_else(|_| o.to_string());
        if let Some(i) = to.iter().position(|x| x.scope.get("org").and_then(|v| v.as_str()) == Some(&id)) {
            return Ok(to.swap_remove(i));
        }
    }
    to.sort_by_key(|x| match crate::net::exchange::scope_kind(&x.scope) {
        "node" => 0,
        "link" => 1,
        _ => 2,
    });
    to.into_iter().next().ok_or_else(|| Error::invalid("peer", format!("`{k}`: no active connection that allows collaboration")))
}

/// Answer a peer (`from`, over `x`); `me` is this node's peer id.
pub fn serve(s: &mut Store, x: &Exchange, from: &str, me: &str, req: CollabReq) -> Result<CollabResp> {
    if !allowed(x) {
        return Err(Error::invalid("exchange", "this key exchange does not allow collaboration"));
    }
    Ok(match req {
        CollabReq::Live { batch } => {
            let b = Batch::decode_bytes(&batch)?;
            let (out, meta) = super::session::receive(s, me, from, &x.title(), &b)?;
            CollabResp::Live { batch: out.bytes(), meta }
        }
        CollabReq::Offers => CollabResp::Offers { body: super::peers::offers_for(s, from)? },
        CollabReq::Message { id, from: label, about, text, at } => {
            super::msg::receive(s, x, from, me, &id, &label, about.as_deref(), &text, at)?;
            CollabResp::Delivered
        }
        CollabReq::Run { org, endpoint, input } => {
            let fromp: libp2p::PeerId = from.parse().map_err(|_| Error::invalid("peer", "bad peer id"))?;
            let out = crate::org::net::serve(s, x, &fromp, &org, crate::org::net::OrgReq::Call { endpoint: endpoint.clone(), input });
            let session = super::run::record(s, me, from, &org, &endpoint, out)?;
            CollabResp::Started { session }
        }
    })
}
