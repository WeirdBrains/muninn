//! Linked-node sync over the network (2.11). A node pulls from each of its user's other
//! nodes what changed since it last asked ([`crate::store::sync`]); bars only when asked.
//!
//! Only nodes of the same user (certified, not revoked) are answered. Pages are sealed
//! with a key derived from the user's signing key, which only the user's nodes hold, on
//! top of the exchange key.
use super::client;
use super::node::Node;
use super::proto::{Req, Resp};
use crate::crypto::{self, Key32, Sealed};
use crate::error::{Error, Result};
use crate::model::{Bar, Duration, Symbol};
use crate::store::sync::{Applied, Page};
use crate::store::{BarFilter, Exchange, NewAction, Store, now_ms};
use libp2p::PeerId;
use serde::{Deserialize, Serialize};

/// How often a running node pulls from each linked node.
pub const SYNC_EVERY_MS: i64 = 60 * 1000;
const PAGE: usize = 2000;
const PAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_PAGES: usize = 200;
const BARS_PAGE: i64 = 50_000;

/// What a sync request asks for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncReq {
    /// Changes after a cursor.
    Pull { epoch: String, cursor: i64 },
    /// The bar series this node has.
    Series,
    /// Bars of one series after a time.
    Bars { source: String, base: String, quote: String, duration: i64, after: i64, limit: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSeries {
    pub source: String,
    pub base: String,
    pub quote: String,
    pub duration: i64,
    pub count: i64,
    pub first: i64,
    pub last: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum SyncResp {
    Page(Page),
    Series(Vec<RemoteSeries>),
    /// (time, open, high, low, close, volume, flags)
    Bars(Vec<(i64, f64, f64, f64, f64, f64, i64)>),
}

/// Where we are with one linked node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub epoch: String,
    #[serde(default)]
    pub cursor: i64,
    #[serde(default)]
    pub at: Option<i64>,
    #[serde(default)]
    pub last: Applied,
    #[serde(default)]
    pub error: Option<String>,
}

fn state_key(peer: &str) -> String {
    format!("sync.state.{peer}")
}

pub fn state(s: &Store, peer: &str) -> State {
    s.setting(&state_key(peer)).ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

fn save_state(s: &Store, peer: &str, st: &State) -> Result<()> {
    s.set_setting(&state_key(peer), Some(&serde_json::to_string(st)?))
}

/// The key sync pages are sealed with: only nodes holding the user's key have it.
fn sync_key(s: &Store) -> Result<Key32> {
    let (u, kp) = super::user::load_or_create(s)?;
    let ed = kp.try_into_ed25519().map_err(|_| Error::Crypto("user key is not ed25519".into()))?;
    let bytes = zeroize::Zeroizing::new(ed.to_bytes());
    let ikm = Key32::from_slice(&bytes[..32])?;
    crypto::hkdf(&ikm, u.id.as_bytes(), "muninn/sync/v1/key")
}

fn aad(peer: &str) -> Vec<u8> {
    format!("muninn/sync/v1|{peer}").into_bytes()
}

/// Is this peer one of my user's nodes (certified and not revoked)?
pub fn is_mine(s: &Store, peer: &str) -> bool {
    let (Ok(Some(me)), Ok(Some(u))) = (s.self_user(), super::user::user_of_peer(s, peer)) else { return false };
    me.id == u.id
}

/// Answer a sync request from `from`.
pub fn serve(s: &Store, from: &PeerId, req: SyncReq) -> Result<Resp> {
    let peer = from.to_string();
    if !is_mine(s, &peer) {
        return Err(Error::invalid("sync", "only nodes of the same user sync (link it with `mnnn user link`)"));
    }
    let body = match req {
        SyncReq::Pull { epoch, cursor } => {
            let mine = s.sync_epoch()?;
            let cursor = if epoch == mine { cursor } else { 0 };
            SyncResp::Page(s.sync_changes(&peer, cursor, PAGE, PAGE_BYTES)?)
        }
        SyncReq::Series => SyncResp::Series(
            s.series()?
                .into_iter()
                .map(|x| RemoteSeries {
                    source: x.source,
                    base: x.symbol.base,
                    quote: x.symbol.quote,
                    duration: x.duration.0,
                    count: x.count,
                    first: x.first,
                    last: x.last,
                })
                .collect(),
        ),
        SyncReq::Bars { source, base, quote, duration, after, limit } => {
            let f = BarFilter {
                source: Some(source),
                symbol: Some(Symbol { base, quote }),
                duration: Some(Duration(duration)),
                from: Some(after.saturating_add(1)),
                to: None,
                limit: Some(limit.clamp(1, BARS_PAGE)),
                offset: None,
            };
            SyncResp::Bars(
                s.query_bars(&f)?
                    .into_iter()
                    .map(|b| (b.bar.ts, b.bar.open, b.bar.high, b.bar.low, b.bar.close, b.bar.volume, b.bar.flags))
                    .collect(),
            )
        }
    };
    let key = sync_key(s)?;
    let sealed = crypto::seal(&key, &serde_json::to_vec(&body)?, &aad(&peer))?;
    Ok(Resp::Sync { sealed })
}

fn ask(s: &Store, node: &Node, x: &Exchange, req: SyncReq, relays: &[String]) -> Result<SyncResp> {
    match client::send(s, node, x, Req::Sync { req }, relays)? {
        Resp::Sync { sealed } => open(s, node, &sealed),
        Resp::Error { msg } => Err(Error::Other(msg)),
        _ => Err(Error::Other("unexpected answer to a sync request".into())),
    }
}

fn open(s: &Store, node: &Node, sealed: &Sealed) -> Result<SyncResp> {
    let key = sync_key(s)?;
    let plain = crypto::open(&key, sealed, &aad(&node.peer_id().to_string()))?;
    Ok(serde_json::from_slice(&plain)?)
}

/// My other nodes, each with an exchange to reach it by (if there is one).
pub fn linked(s: &Store) -> Result<Vec<(crate::store::users::UserNode, Option<Exchange>)>> {
    let Some(me) = s.self_user()? else { return Ok(vec![]) };
    let here = super::identity::load_or_create(s)?.public().to_peer_id().to_string();
    let xs = s.exchanges()?;
    Ok(s.user_nodes(&me.id)?
        .into_iter()
        .filter(|n| n.active() && n.peer_id != here)
        .map(|n| {
            let x = xs
                .iter()
                .filter(|x| x.status.active() && x.peer_id.as_deref() == Some(n.peer_id.as_str()))
                .find(|x| matches!(super::exchange::scope_kind(&x.scope), "link" | "node"))
                .cloned();
            (n, x)
        })
        .collect())
}

/// Pull everything new from one linked node.
pub fn pull(s: &Store, node: &Node, x: &Exchange, relays: &[String], origin: &str) -> Result<Applied> {
    let peer = x.peer_id.clone().ok_or_else(|| Error::invalid("sync", "the exchange has no peer"))?;
    if !is_mine(s, &peer) {
        return Err(Error::invalid("sync", format!("{} is not one of your nodes", x.title())));
    }
    let mut st = state(s, &peer);
    let mut total = Applied::default();
    let r = (|| -> Result<()> {
        for _ in 0..MAX_PAGES {
            let SyncResp::Page(p) = ask(s, node, x, SyncReq::Pull { epoch: st.epoch.clone(), cursor: st.cursor }, relays)? else {
                return Err(Error::Other("unexpected sync answer".into()));
            };
            if p.epoch != st.epoch {
                st.epoch = p.epoch.clone();
            }
            total.add(&s.sync_apply(&peer, &p)?);
            st.cursor = p.next;
            save_state(s, &peer, &st)?;
            if !p.more {
                break;
            }
        }
        Ok(())
    })();
    st.at = Some(now_ms());
    st.last = total.clone();
    st.error = r.as_ref().err().map(|e| e.to_string());
    save_state(s, &peer, &st)?;
    r?;
    if total.changed() > 0 || total.conflicts > 0 {
        s.note_action(&NewAction::new("sync.pull", origin, format!("synced from {}: {}", x.title(), summary(&total))).params(serde_json::json!({
            "peer": peer,
            "taken": total.taken,
            "deleted": total.deleted,
            "conflicts": total.conflicts,
        })));
    }
    Ok(total)
}

pub fn summary(a: &Applied) -> String {
    let mut parts = vec![format!("{} changed", a.taken), format!("{} deleted", a.deleted)];
    if a.conflicts > 0 {
        parts.push(format!("{} conflict(s) kept (`mnnn sync conflicts`)", a.conflicts));
    }
    parts.join(", ")
}

/// For a running node: pull from linked nodes whose turn it is. Lines say what changed.
pub fn tick(s: &Store, node: &Node, relays: &[String], origin: &str) -> Vec<String> {
    let mut out = vec![];
    let now = now_ms();
    let Ok(nodes) = linked(s) else { return out };
    for (n, x) in nodes {
        let Some(x) = x else { continue };
        if state(s, &n.peer_id).at.is_some_and(|t| now - t < SYNC_EVERY_MS) {
            continue;
        }
        if let Ok(a) = pull(s, node, &x, relays, origin)
            && (a.changed() > 0 || a.conflicts > 0)
        {
            out.push(format!("synced from {}: {}", if n.label.is_empty() { x.title() } else { n.label.clone() }, summary(&a)));
        }
    }
    out
}

/// What bars a linked node has.
pub fn remote_series(s: &Store, node: &Node, x: &Exchange, relays: &[String]) -> Result<Vec<RemoteSeries>> {
    match ask(s, node, x, SyncReq::Series, relays)? {
        SyncResp::Series(v) => Ok(v),
        _ => Err(Error::Other("unexpected sync answer".into())),
    }
}

/// Fetch the bars of one series that this node does not have yet. Returns bars added.
pub fn pull_bars(s: &mut Store, node: &Node, x: &Exchange, r: &RemoteSeries, relays: &[String]) -> Result<u64> {
    let symbol = Symbol { base: r.base.clone(), quote: r.quote.clone() };
    let local = s.series()?.into_iter().find(|l| l.source == r.source && l.symbol == symbol && l.duration.0 == r.duration);
    // fill from the start unless what is here already begins where theirs does
    let mut after = match &local {
        Some(l) if l.first <= r.first && l.count >= r.count => return Ok(0),
        Some(l) if l.first <= r.first => l.last,
        _ => i64::MIN,
    };
    let source = s.get_or_create_source(&r.source, "sync", None)?;
    let mut added = 0u64;
    loop {
        let req =
            SyncReq::Bars { source: r.source.clone(), base: r.base.clone(), quote: r.quote.clone(), duration: r.duration, after, limit: BARS_PAGE };
        let SyncResp::Bars(rows) = ask(s, node, x, req, relays)? else {
            return Err(Error::Other("unexpected sync answer".into()));
        };
        let bars: Vec<Bar> =
            rows.into_iter().map(|(ts, open, high, low, close, volume, flags)| Bar { ts, open, high, low, close, volume, flags }).collect();
        if bars.is_empty() {
            break;
        }
        after = bars.iter().map(|b| b.ts).max().unwrap_or(after);
        added += s.upsert_bars(&source, &symbol, Duration(r.duration), &bars)?.inserted;
        if (bars.len() as i64) < BARS_PAGE {
            break;
        }
    }
    Ok(added)
}
