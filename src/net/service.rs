//! Answering peers: the [`Handler`] a node runs with. It owns its own database connection.
//!
//! Offers are stored as *incoming* and never accepted on their own; answers complete only
//! our own open offers; everything else must arrive sealed under the key of an active
//! exchange whose peer is the Noise-authenticated sender, and only reaches what is shared
//! over that exchange.
use super::exchange::{self, Token};
use super::node::{Handler, Sighting};
use super::proto::{Item, Req, Resp, ShardBlob, SharedItem, Wire, WireResp};
use crate::error::{Error, Result};
use crate::live::Found;
use crate::store::{Exchange, ExchangeStatus, Held, ItemKind, NewAction, NewExchange, SearchTarget, Share, ShareKind, Store, now_ms};
use libp2p::PeerId;
use std::collections::HashSet;
use std::sync::Mutex;

/// At most this many incoming offers are kept.
const MAX_INCOMING: usize = 50;
/// Sealed shards are small; anything bigger is refused.
const MAX_SHARD_BLOB: usize = 16 * 1024;
/// At most this many shards are held per peer.
const MAX_HELD_PER_PEER: usize = 64;

/// The handler for a node.
pub struct Service {
    store: Mutex<Store>,
    local: PeerId,
    label: String,
    /// Answer every node's connection request on our own (`net.accept = "anyone"`);
    /// otherwise only known nodes are answered and the rest wait for the owner.
    accept_anyone: bool,
}

impl Service {
    pub fn new(store: Store, local: PeerId, label: &str) -> Self {
        Service { store: Mutex::new(store), local, label: label.to_string(), accept_anyone: false }
    }

    /// Accept connection requests from any node, not only known ones.
    pub fn accept_anyone(mut self, on: bool) -> Self {
        self.accept_anyone = on;
        self
    }

    fn with<T>(&self, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let mut s = self.store.lock().map_err(|_| Error::Other("store lock poisoned".into()))?;
        f(&mut s)
    }

    fn wire(&self, from: &PeerId, req: Wire) -> Result<WireResp> {
        match req {
            Wire::Hello => {
                Ok(WireResp::Hello { label: self.label.clone(), agent: format!("muninn/{}", env!("CARGO_PKG_VERSION")), caps: vec!["muninn".into()] })
            }
            Wire::Offer { token } => self.with(|s| incoming_offer(s, from, &token)).map(|msg| WireResp::Ok { msg }),
            Wire::Answer { token } => self.with(|s| incoming_answer(s, from, &token)).map(|msg| WireResp::Ok { msg }),
            Wire::Orgs => self.with(|s| crate::org::net::listed(s)).map(|orgs| WireResp::Orgs { orgs }),
            Wire::OrgInfo { org } => self.with(|s| crate::org::net::info(s, &org)).map(|o| WireResp::Orgs { orgs: vec![o] }),
            Wire::Poll { exchange } => {
                self.with(|s| super::auto::poll_in(s, from, &exchange)).map(|(status, answer)| WireResp::Poll { status, answer })
            }
            Wire::Connect { token } => self.with(|s| super::auto::connect_in(s, from, &token, &self.label, self.accept_anyone)),
            Wire::Sealed { exchange, sealed } => self.with(|s| {
                let x = s.exchange(&exchange)?.ok_or_else(|| Error::invalid("exchange", "unknown exchange"))?;
                if !x.status.active() {
                    return Err(Error::invalid("exchange", "this exchange is not active (revoked or never completed)"));
                }
                if x.peer_id.as_deref() != Some(&from.to_string()) {
                    return Err(Error::invalid("exchange", "this exchange belongs to another node"));
                }
                let key = s.exchange_key(&x.id)?.ok_or_else(|| Error::invalid("exchange", "no key"))?;
                let plain = exchange::open_msg(&key, &x.id, from, &self.local, &sealed)?;
                let req: Req = serde_json::from_slice(&plain).map_err(|e| Error::invalid("request", e.to_string()))?;
                let done = match req {
                    Req::Org { org, req } => crate::org::net::serve(s, &x, from, &org, req),
                    Req::Review { review, req } => crate::review::net::serve(s, &x, from, &review, req),
                    req => answer(s, &x, from, req, &self.label),
                };
                let resp = match done {
                    Ok(r) => r,
                    Err(e) => Resp::Error { msg: e.to_string() },
                };
                let out = serde_json::to_vec(&resp)?;
                let sealed = exchange::seal_msg(&key, &x.id, &self.local, from, &out)?;
                Ok(WireResp::Sealed { sealed })
            }),
        }
    }
}

impl Handler for Service {
    fn request(&self, from: &PeerId, req: Wire) -> WireResp {
        match self.wire(from, req) {
            Ok(r) => r,
            Err(e) => WireResp::Error { msg: e.to_string() },
        }
    }

    fn seen(&self, s: Sighting) {
        let _ = self.with(|st| {
            st.upsert_net_peer(&s.peer.to_string(), None, &s.addrs, s.caps.as_deref(), s.agent.as_deref(), s.source)?;
            Ok(())
        });
    }
}

fn record(s: &Store, kind: &str, summary: String, params: serde_json::Value) {
    s.note_action(&NewAction::new(kind, "network", summary).params(params));
}

/// A peer delivered an offer: keep it as incoming (replacing an earlier copy).
pub fn incoming_offer(s: &Store, from: &PeerId, token: &str) -> Result<String> {
    let Token::Offer(o) = exchange::parse_token(token)? else {
        return Err(Error::invalid("token", "expected an offer"));
    };
    let p = o.verify(now_ms())?;
    if p != *from {
        return Err(Error::invalid("exchange", "the offer was sent by a different node than the one that signed it"));
    }
    if let Some(x) = s.exchange(&o.id)? {
        return Ok(format!("already known ({})", x.status.as_str()));
    }
    crate::org::net::check_application(s, &o)?;
    let incoming = s.exchanges()?.into_iter().filter(|x| x.status == ExchangeStatus::Incoming).count();
    if incoming >= MAX_INCOMING {
        return Err(Error::invalid("exchange", "too many pending offers on this node"));
    }
    let x = s.insert_exchange(
        &NewExchange {
            id: o.id.clone(),
            label: String::new(),
            role: "answerer".into(),
            status: Some(ExchangeStatus::Incoming),
            peer_id: Some(o.peer.clone()),
            peer_label: o.label.clone(),
            scope: o.scope.clone(),
            offer: token.split_whitespace().collect(),
            expires_at: Some(o.expires),
        },
        None,
    )?;
    s.upsert_net_peer(&o.peer, Some(&o.label), &o.addrs, None, None, "exchange")?;
    if let Some(msg) = crate::org::net::applied(s, &o, &x)? {
        return Ok(msg);
    }
    record(
        s,
        "net.exchange.incoming",
        format!("key exchange offered by {} ({})", x.title(), super::short_peer_id(from)),
        serde_json::json!({ "exchange": x.id, "peer": o.peer, "label": o.label }),
    );
    Ok("offer received; it waits to be accepted".into())
}

/// A peer delivered the answer to one of our offers: complete it.
pub fn incoming_answer(s: &Store, from: &PeerId, token: &str) -> Result<String> {
    let Token::Answer(a) = exchange::parse_token(token)? else {
        return Err(Error::invalid("token", "expected an answer"));
    };
    if a.peer != from.to_string() {
        return Err(Error::invalid("exchange", "the answer was sent by a different node than the one that signed it"));
    }
    let x = complete_offer(s, &a, token)?;
    record(
        s,
        "net.exchange.complete",
        format!("key exchange with {} complete", x.title()),
        serde_json::json!({ "exchange": x.id, "peer": x.peer_id, "fingerprint": x.fingerprint }),
    );
    Ok("exchange complete".into())
}

/// Complete our offer with an answer (from the network or pasted).
pub fn complete_offer(s: &Store, a: &exchange::Answer, token: &str) -> Result<Exchange> {
    let x = s.exchange(&a.offer)?.ok_or_else(|| Error::invalid("exchange", "this answer is for an offer this node did not make"))?;
    if x.role != "offerer" {
        return Err(Error::invalid("exchange", "this answer is for an offer this node did not make"));
    }
    if x.status != ExchangeStatus::Offered {
        return Err(Error::invalid("exchange", format!("the exchange is already {}", x.status.as_str())));
    }
    let Token::Offer(o) = exchange::parse_token(&x.offer)? else {
        return Err(Error::Other("stored offer is damaged".into()));
    };
    let secret = s.exchange_secret(&x.id)?.ok_or_else(|| Error::Other("the offer's secret is gone".into()))?;
    let d = a.complete(&o, &secret)?;
    let x = s.complete_exchange(&x.id, &a.peer, &a.label, &token.split_whitespace().collect::<String>(), &d.key, &d.short_code, &d.fingerprint)?;
    s.upsert_net_peer(&a.peer, Some(&a.label), &a.addrs, None, None, "exchange")?;
    crate::org::net::answered(s, &x)?;
    crate::review::net::answered(s, &x)?;
    Ok(x)
}

/// Journals and models reachable through what is shared over an exchange.
pub struct Entitled {
    pub shares: Vec<Share>,
    pub journals: HashSet<String>,
    pub models: HashSet<String>,
    pub bars: bool,
}

pub fn entitled(s: &Store, exchange: &str) -> Result<Entitled> {
    let shares = s.shares_for(exchange)?;
    let bars_id = s.models()?.into_iter().find(|m| m.is_bars()).map(|m| m.id);
    let mut e = Entitled { shares: shares.clone(), journals: HashSet::new(), models: HashSet::new(), bars: false };
    for sh in &shares {
        match sh.kind {
            ShareKind::Journal => {
                e.journals.insert(sh.ref_id.clone());
            }
            ShareKind::Model => {
                if Some(&sh.ref_id) == bars_id.as_ref() {
                    e.bars = true;
                } else {
                    e.models.insert(sh.ref_id.clone());
                }
            }
            ShareKind::Project => {
                for it in s.project_items(&sh.ref_id)? {
                    match it.kind {
                        ItemKind::Journal => {
                            e.journals.insert(it.ref_id);
                        }
                        ItemKind::Model => {
                            if Some(&it.ref_id) == bars_id.as_ref() {
                                e.bars = true;
                            } else {
                                e.models.insert(it.ref_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(e)
}

impl Entitled {
    /// Whether a search result may go to the peer.
    pub fn allows(&self, f: &Found) -> bool {
        match f {
            Found::Series { .. } => self.bars,
            Found::Entry { journal, .. } => self.journals.contains(journal),
            Found::Record { model, .. } => self.models.contains(model),
        }
    }
}

fn share_by(e: &Entitled, key: &str) -> Result<Share> {
    let k = key.trim();
    let m: Vec<&Share> = e
        .shares
        .iter()
        .filter(|s| {
            s.id == k || s.ref_id == k || s.name.eq_ignore_ascii_case(k) || (k.len() >= 4 && (s.id.starts_with(k) || s.ref_id.starts_with(k)))
        })
        .collect();
    match m.len() {
        0 => Err(Error::invalid("item", format!("`{k}` is not shared with you"))),
        1 => Ok(m[0].clone()),
        _ => Err(Error::invalid("item", format!("`{k}` matches more than one shared item; use its id"))),
    }
}

fn doc_format(f: &str) -> Option<crate::render::DocFormat> {
    use crate::render::DocFormat;
    Some(match f.to_ascii_lowercase().as_str() {
        "pdf" => DocFormat::Pdf,
        "html" => DocFormat::Html,
        "docx" | "word" => DocFormat::Docx,
        "latex" | "tex" => DocFormat::Latex,
        "text" | "txt" => DocFormat::Text,
        _ => return None,
    })
}

/// A shared item's content.
pub fn item(s: &Store, sh: &Share, format: Option<&str>) -> Result<Item> {
    item_of(s, sh.kind, &sh.ref_id, &sh.name, format)
}

/// The content of one of this node's things (`json`, a document format, or `mnnp`).
pub fn item_of(s: &Store, kind: ShareKind, ref_id: &str, name: &str, format: Option<&str>) -> Result<Item> {
    let sh = Share { id: String::new(), kind, ref_id: ref_id.to_string(), name: name.to_string(), exchange_id: None, author_id: None, created_at: 0 };
    let sh = &sh;
    let fmt = format.map(|f| f.trim().to_ascii_lowercase()).filter(|f| !f.is_empty());
    let json = |v: &serde_json::Value| -> Result<Item> {
        Ok(Item { kind: sh.kind, name: sh.name.clone(), format: "json".into(), data: serde_json::to_vec_pretty(v)? })
    };
    let doc = |d: crate::render::Doc, f: &str| -> Result<Item> {
        let df = doc_format(f).ok_or_else(|| Error::invalid("format", format!("`{f}`: use json, pdf, html, docx, latex or text")))?;
        Ok(Item { kind: sh.kind, name: sh.name.clone(), format: df.ext().into(), data: crate::render::render(&d, df)? })
    };
    match sh.kind {
        ShareKind::Model => {
            let m = s.find_model(&sh.ref_id)?;
            if m.is_bars() {
                let series: Vec<crate::live::SeriesInfoOut> = s.series()?.iter().map(|x| x.into()).collect();
                return json(&serde_json::json!({ "model": m, "series": series }));
            }
            let versions = s.model_versions(&m.id)?;
            let records = if m.version > 0 { s.records(&m, &Default::default())? } else { vec![] };
            json(&serde_json::to_value(crate::store::archive::ModelPack { model: m, versions, records })?)
        }
        ShareKind::LiveSearch => {
            let ls = s.find_live_search(&sh.ref_id)?;
            let found = crate::live::run(s, &ls.spec)?;
            json(&serde_json::json!({ "live_search": ls, "results": found }))
        }
        ShareKind::Journal => match fmt.as_deref() {
            None | Some("json") => Ok(Item {
                kind: sh.kind,
                name: sh.name.clone(),
                format: "json".into(),
                data: crate::export::archive::journal_json(s, &sh.ref_id, &[])?.into_bytes(),
            }),
            Some(f) => doc(crate::export::docs::journal(s, &sh.ref_id, &[])?, f),
        },
        ShareKind::Project => match fmt.as_deref() {
            None | Some("mnnp") | Some("archive") => {
                let (bytes, _) = crate::export::archive::project_archive(s, &sh.ref_id, true, 50)?;
                Ok(Item { kind: sh.kind, name: sh.name.clone(), format: "mnnp".into(), data: bytes })
            }
            Some("json") => {
                let (j, _) = crate::export::archive::project_json(s, &sh.ref_id, true)?;
                Ok(Item { kind: sh.kind, name: sh.name.clone(), format: "json".into(), data: j.into_bytes() })
            }
            Some(f) => doc(crate::export::docs::project(s, &sh.ref_id, 50)?, f),
        },
        ShareKind::Report => match fmt.as_deref() {
            None | Some("json") => {
                let r = s.find_report(&sh.ref_id)?;
                let sections = s.sections(&r.id)?;
                json(&serde_json::to_value(crate::store::archive::ReportPack { report: r, sections })?)
            }
            Some(f) => doc(crate::export::docs::report(s, &sh.ref_id)?, f),
        },
        ShareKind::Code => {
            let mut c = s.find_code_space(&sh.ref_id)?;
            c.approved_hash = None;
            c.last_run = None;
            json(&serde_json::to_value(c)?)
        }
    }
}

fn limit(v: Vec<Found>, n: usize) -> (Vec<Found>, bool) {
    let n = if n == 0 { 500 } else { n.min(5000) };
    let truncated = v.len() > n;
    (v.into_iter().take(n).collect(), truncated)
}

/// Answer a sealed request.
pub fn answer(s: &Store, x: &Exchange, from: &PeerId, req: Req, label: &str) -> Result<Resp> {
    Ok(match req {
        Req::Ping => Resp::Pong { label: label.to_string(), shared: s.shares_for(&x.id)?.len() },
        Req::List => {
            let e = entitled(s, &x.id)?;
            let items = e
                .shares
                .iter()
                .map(|sh| SharedItem {
                    id: sh.ref_id.clone(),
                    kind: sh.kind,
                    name: sh.name.clone(),
                    about: if sh.kind == ShareKind::LiveSearch {
                        s.live_search(&sh.ref_id).ok().flatten().map(|l| l.spec.describe()).unwrap_or_default()
                    } else {
                        String::new()
                    },
                })
                .collect();
            Resp::Shared { items }
        }
        Req::Fetch { share, format } => {
            let e = entitled(s, &x.id)?;
            let sh = share_by(&e, &share)?;
            let it = item(s, &sh, format.as_deref())?;
            record(
                s,
                "net.serve",
                format!("sent {} {} to {}", sh.kind.label(), sh.name, x.title()),
                serde_json::json!({ "exchange": x.id, "kind": sh.kind, "item": sh.ref_id, "name": sh.name, "bytes": it.data.len() }),
            );
            Resp::Item { item: it }
        }
        Req::Search { spec, limit: n } => {
            let e = entitled(s, &x.id)?;
            let reachable = match spec.target {
                SearchTarget::Series => e.bars,
                SearchTarget::Journals => !e.journals.is_empty(),
                SearchTarget::Records => !e.models.is_empty(),
            };
            let found = if reachable { crate::live::run(s, &spec)?.into_iter().filter(|f| e.allows(f)).collect() } else { vec![] };
            let (results, truncated) = limit(found, n);
            Resp::Found { results, truncated }
        }
        Req::RunShared { share, limit: n } => {
            let e = entitled(s, &x.id)?;
            let sh = share_by(&e, &share)?;
            if sh.kind != ShareKind::LiveSearch {
                return Err(Error::invalid("item", format!("{} is a {}, not a live search", sh.name, sh.kind.label())));
            }
            let ls = s.find_live_search(&sh.ref_id)?;
            let (results, truncated) = limit(crate::live::run(s, &ls.spec)?, n);
            Resp::Found { results, truncated }
        }
        Req::ShardStore { shard, claim } => {
            if shard.blob.len() > MAX_SHARD_BLOB || claim.len() > 128 {
                return Err(Error::invalid("shard", "too large"));
            }
            let owner = from.to_string();
            let held = s.held_shards()?;
            if held.iter().filter(|h| h.owner_peer == owner).count() >= MAX_HELD_PER_PEER {
                return Err(Error::invalid("shard", "this node holds too many shards for you already"));
            }
            s.hold_shard(&Held {
                id: crate::store::net::new_held_id(),
                exchange_id: x.id.clone(),
                owner_peer: owner,
                owner_label: x.title(),
                set_id: shard.set.clone(),
                index: shard.index,
                threshold: shard.threshold,
                total: shard.total,
                claim,
                blob: shard.blob,
                stored_at: now_ms(),
            })?;
            record(
                s,
                "net.shard.hold",
                format!("holding dark shard {} of set {} for {}", shard.index, shard.set, x.title()),
                serde_json::json!({ "exchange": x.id, "set": shard.set, "index": shard.index }),
            );
            Resp::Stored
        }
        Req::ShardFetch { set } => {
            let owner = from.to_string();
            let shards = s
                .held_shards()?
                .into_iter()
                .filter(|h| h.owner_peer == owner && set.as_ref().is_none_or(|st| &h.set_id == st))
                .map(blob_of)
                .collect();
            Resp::Shards { shards }
        }
        Req::ShardClaim { claim } => {
            let shards: Vec<ShardBlob> = s.held_shards()?.into_iter().filter(|h| !claim.is_empty() && ct_eq(&h.claim, &claim)).map(blob_of).collect();
            if !shards.is_empty() {
                record(
                    s,
                    "net.shard.release",
                    format!("released {} dark shard(s) to {} by claim", shards.len(), x.title()),
                    serde_json::json!({ "exchange": x.id, "count": shards.len() }),
                );
            }
            Resp::Shards { shards }
        }
        Req::Org { .. } => return Err(Error::Other("organization requests are answered by the org's node".into())),
        Req::Review { .. } => return Err(Error::Other("review requests are answered by the author's node".into())),
        Req::Revoke => {
            let x = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("peer"))?;
            crate::org::net::ended(s, &x)?;
            crate::review::net::ended(s, &x)?;
            record(s, "net.exchange.revoke", format!("{} ended the key exchange", x.title()), serde_json::json!({ "exchange": x.id, "by": "peer" }));
            Resp::Ok
        }
    })
}

fn blob_of(h: Held) -> ShardBlob {
    ShardBlob { set: h.set_id, index: h.index, threshold: h.threshold, total: h.total, blob: h.blob }
}

fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
