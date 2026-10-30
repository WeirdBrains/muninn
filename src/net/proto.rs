//! What nodes say to each other over `/muninn/net/1` (request-response, CBOR).
//!
//! Only three things travel in the open: `Hello` (a label and capabilities), offers and
//! answers (public keys and signatures). Everything else is a [`Req`] / [`Resp`] sealed
//! under the exchange key ([`Wire::Sealed`]), and the receiver checks that the
//! Noise-authenticated peer is the exchange's peer before opening it.
use crate::crypto::Sealed;
use crate::live::Found;
use crate::store::{SearchSpec, ShareKind};
use serde::{Deserialize, Serialize};

pub const PROTOCOL: &str = "/muninn/net/1";
pub const KAD_PROTOCOL: &str = "/muninn/kad/1";
/// The Kademlia key relays provide.
pub const RELAY_KEY: &str = "/muninn/relay/1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Wire {
    Hello,
    /// An offer token delivered to a peer (stored there as incoming).
    Offer {
        token: String,
    },
    /// An answer token delivered to the offerer.
    Answer {
        token: String,
    },
    /// A sealed [`Req`].
    Sealed {
        exchange: String,
        sealed: Sealed,
    },
    /// The public organizations this node keeps (2.7).
    Orgs,
    /// One organization by id or slug (private ones too: knowing the slug is enough to apply).
    OrgInfo {
        org: String,
    },
    /// Has my application (an offer to an org) been decided? Only its offerer may ask.
    Poll {
        exchange: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WireResp {
    Hello {
        label: String,
        agent: String,
        caps: Vec<String>,
    },
    Ok {
        msg: String,
    },
    /// A sealed [`Resp`].
    Sealed {
        sealed: Sealed,
    },
    Error {
        msg: String,
    },
    Orgs {
        orgs: Vec<crate::org::OrgCard>,
    },
    /// `applied`, `member` (with the answer token), `rejected` or `kicked`.
    Poll {
        status: String,
        answer: Option<String>,
    },
}

/// One shared item as a peer sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedItem {
    pub id: String,
    pub kind: ShareKind,
    pub name: String,
    pub about: String,
}

/// A fetched item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub kind: ShareKind,
    pub name: String,
    /// `json`, `mnnp` (a project archive) or a document format (`pdf`, `html`, ...).
    pub format: String,
    #[serde(with = "serde_bytes_b64")]
    pub data: Vec<u8>,
}

/// A sealed shard as stored and returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShardBlob {
    pub set: String,
    pub index: u32,
    pub threshold: u32,
    pub total: u32,
    pub blob: String,
}

/// Requests inside an exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Req {
    Ping,
    /// What is shared with me.
    List,
    Fetch {
        share: String,
        format: Option<String>,
    },
    /// Run a search over what is shared with me.
    Search {
        spec: SearchSpec,
        limit: usize,
    },
    /// Run one of the live searches shared with me.
    RunShared {
        share: String,
        limit: usize,
    },
    /// Keep a sealed dark shard for me.
    ShardStore {
        shard: ShardBlob,
        claim: String,
    },
    /// The shards held for me (this node's identity), optionally one set.
    ShardFetch {
        set: Option<String>,
    },
    /// The shards held under a claim (from a fresh node after losing everything).
    ShardClaim {
        claim: String,
    },
    /// The other side ended the exchange.
    Revoke,
    /// A request to an organization this exchange is a membership of (2.7).
    Org {
        org: String,
        req: crate::org::net::OrgReq,
    },
    /// A request to a review this exchange is a reviewer's link to (2.8).
    Review {
        review: String,
        req: crate::review::net::ReviewReq,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Resp {
    Pong {
        label: String,
        shared: usize,
    },
    Shared {
        items: Vec<SharedItem>,
    },
    Item {
        item: Item,
    },
    Found {
        results: Vec<Found>,
        truncated: bool,
    },
    Stored,
    Shards {
        shards: Vec<ShardBlob>,
    },
    Ok,
    Error {
        msg: String,
    },
    /// An organization's answer.
    Org {
        body: serde_json::Value,
    },
    /// A review's answer.
    Review {
        body: serde_json::Value,
    },
}

/// Bytes as base64 in JSON (sealed payloads are JSON).
mod serde_bytes_b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(s).map_err(serde::de::Error::custom)
    }
}
