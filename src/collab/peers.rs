//! The peers view (2.19): your connections and what each has for you: shared items, live
//! sessions you are invited to, org executables you may run and the sites it announced.
use super::wire::{CollabReq, CollabResp, Transport, unexpected};
use crate::error::Result;
use crate::store::{MemberStatus, Store};
use serde::Serialize;
use serde_json::{Value, json};

/// Host: what this node offers `peer`.
pub fn offers_for(s: &Store, peer: &str) -> Result<Value> {
    let mut exe = vec![];
    for o in s.orgs()? {
        if !s.member_by_peer(&o.id, peer)?.is_some_and(|m| m.status == MemberStatus::Member) {
            continue;
        }
        for e in s.endpoints(&o.id)? {
            exe.push(json!({ "org": o.slug, "org_name": o.name, "endpoint": e.name, "kind": e.kind, "perm": e.perm }));
        }
    }
    Ok(json!({ "live": super::session::offers(s, peer)?, "executables": exe }))
}

/// One peer, as this node knows it.
#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    pub peer: String,
    pub label: String,
    /// The exchanges that reach it (`node`, `link`, `organization`).
    pub scopes: Vec<String>,
    pub exchange: String,
    pub unread: usize,
    /// Sites it announced (name and where to open it).
    pub sites: Vec<Value>,
}

/// Your peers (active exchanges that allow collaboration), one per node.
pub fn list(s: &Store) -> Result<Vec<Peer>> {
    let mut out: Vec<Peer> = vec![];
    let msgs = s.messages(None, None)?;
    let sites = crate::org::announce::received(s)?;
    for x in s.exchanges()?.into_iter().filter(|x| x.status.active() && super::wire::allowed(x)) {
        let Some(pid) = x.peer_id.clone() else { continue };
        let kind = crate::net::exchange::scope_kind(&x.scope).to_string();
        if let Some(p) = out.iter_mut().find(|p| p.peer == pid) {
            if !p.scopes.contains(&kind) {
                p.scopes.push(kind);
            }
            continue;
        }
        let unread = msgs.iter().filter(|m| m.peer == pid && m.direction == "in" && m.read_at.is_none()).count();
        let sites =
            sites.iter().filter(|(a, c)| a.peer == pid && c.open).map(|(_, c)| json!({ "org": c.slug, "name": c.name, "urls": c.urls })).collect();
        out.push(Peer { peer: pid, label: x.title(), scopes: vec![kind], exchange: x.id.clone(), unread, sites });
    }
    out.sort_by_key(|p| p.label.to_lowercase());
    Ok(out)
}

/// Guest: ask a peer what it offers.
pub fn offers(t: &dyn Transport) -> Result<Value> {
    match t.call(CollabReq::Offers)? {
        CollabResp::Offers { body } => Ok(body),
        _ => Err(unexpected()),
    }
}
