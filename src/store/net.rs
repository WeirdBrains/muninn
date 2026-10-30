//! The decentralized network's tables (2.6): this node's identity, peers seen, key
//! exchanges, what is shared with whom, and dark shards (entrusted to peers and held for
//! them). The protocol and node are `crate::net`; see `docs/network.md`.
//!
//! Secrets (the identity keypair, exchange secrets and keys) live only here, inside the
//! encrypted database, so backups carry them and nothing writes them in plaintext.
use super::{Store, new_id, now_ms};
use crate::crypto::Key32;
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const MIGRATION: &str = r#"
CREATE TABLE net_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    keypair BLOB NOT NULL,
    peer_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE net_peers (
    peer_id TEXT PRIMARY KEY,
    label TEXT NOT NULL DEFAULT '',
    addrs TEXT NOT NULL DEFAULT '[]',
    caps TEXT NOT NULL DEFAULT '[]',
    agent TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'manual',
    first_seen INTEGER NOT NULL,
    last_seen INTEGER NOT NULL
);
CREATE TABLE net_exchanges (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL,
    status TEXT NOT NULL,
    peer_id TEXT,
    peer_label TEXT NOT NULL DEFAULT '',
    scope TEXT NOT NULL DEFAULT '{"kind":"node"}',
    offer TEXT NOT NULL,
    answer TEXT,
    secret BLOB,
    key BLOB,
    short_code TEXT,
    fingerprint TEXT,
    expires_at INTEGER,
    revoked_by TEXT,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    verified_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX net_exchanges_peer ON net_exchanges(peer_id);
CREATE TABLE net_shares (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    ref_id TEXT NOT NULL,
    name TEXT NOT NULL,
    exchange_id TEXT NOT NULL DEFAULT '*',
    author_id TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(kind, ref_id, exchange_id)
);
CREATE TABLE net_entrusted (
    id TEXT PRIMARY KEY,
    set_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    threshold INTEGER NOT NULL,
    total INTEGER NOT NULL,
    exchange_id TEXT NOT NULL,
    peer_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(set_id, idx)
);
CREATE TABLE net_custody (
    id TEXT PRIMARY KEY,
    exchange_id TEXT NOT NULL,
    owner_peer TEXT NOT NULL,
    owner_label TEXT NOT NULL DEFAULT '',
    set_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    threshold INTEGER NOT NULL,
    total INTEGER NOT NULL,
    claim TEXT NOT NULL,
    blob TEXT NOT NULL,
    stored_at INTEGER NOT NULL,
    UNIQUE(owner_peer, set_id, idx)
);
"#;

/// A peer this node has seen or was told about.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetPeer {
    pub peer_id: String,
    pub label: String,
    pub addrs: Vec<String>,
    /// What it offers (`relay`, `muninn`, `kad`, ...).
    pub caps: Vec<String>,
    pub agent: String,
    /// How it was found: `mdns`, `identify`, `kad`, `exchange`, `manual`.
    pub source: String,
    pub first_seen: i64,
    pub last_seen: i64,
}

impl NetPeer {
    pub fn short_id(&self) -> &str {
        short_peer(&self.peer_id)
    }
    pub fn is_relay(&self) -> bool {
        self.caps.iter().any(|c| c == "relay")
    }
}

/// A new id for a held shard.
pub fn new_held_id() -> String {
    new_id()
}

/// The last 8 characters of a peer id (the start is the same for every Ed25519 id).
pub fn short_peer(p: &str) -> &str {
    &p[p.len().saturating_sub(8)..]
}

/// Where an exchange stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExchangeStatus {
    /// We made an offer and wait for its answer.
    Offered,
    /// A peer sent us an offer; nothing happens until it is accepted here.
    Incoming,
    /// Both sides hold the shared key.
    Complete,
    /// Complete, and the short codes were compared and matched.
    Verified,
    /// Ended (here or by the peer); the key is erased, the record kept.
    Revoked,
}

impl ExchangeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ExchangeStatus::Offered => "offered",
            ExchangeStatus::Incoming => "incoming",
            ExchangeStatus::Complete => "complete",
            ExchangeStatus::Verified => "verified",
            ExchangeStatus::Revoked => "revoked",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "offered" => ExchangeStatus::Offered,
            "incoming" => ExchangeStatus::Incoming,
            "complete" => ExchangeStatus::Complete,
            "verified" => ExchangeStatus::Verified,
            _ => ExchangeStatus::Revoked,
        }
    }
    /// The exchange carries data (complete or verified).
    pub fn active(self) -> bool {
        matches!(self, ExchangeStatus::Complete | ExchangeStatus::Verified)
    }
}

/// A key exchange with another node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    pub id: String,
    pub label: String,
    /// `offerer` or `answerer`.
    pub role: String,
    pub status: ExchangeStatus,
    pub peer_id: Option<String>,
    pub peer_label: String,
    /// What the exchange is for (`{"kind":"node"}`; organizations and resources later).
    pub scope: serde_json::Value,
    /// The offer token (signed, public).
    pub offer: String,
    /// The answer token (signed, public).
    pub answer: Option<String>,
    /// Ten digits both people compare (`12345-67890`).
    pub short_code: Option<String>,
    pub fingerprint: Option<String>,
    pub expires_at: Option<i64>,
    /// `me` or `peer`.
    pub revoked_by: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub verified_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

impl Exchange {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// The label, else the peer's label, else its short id.
    pub fn title(&self) -> String {
        if !self.label.is_empty() {
            self.label.clone()
        } else if !self.peer_label.is_empty() {
            self.peer_label.clone()
        } else {
            self.peer_id.as_deref().map(short_peer).unwrap_or("(no peer yet)").to_string()
        }
    }
}

/// What can be shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareKind {
    /// A model with its records (`bars`: the imported series, searchable by peers).
    Model,
    LiveSearch,
    Journal,
    Project,
    Report,
    /// A code space's source.
    Code,
}

impl ShareKind {
    pub const ALL: [ShareKind; 6] =
        [ShareKind::Model, ShareKind::LiveSearch, ShareKind::Journal, ShareKind::Project, ShareKind::Report, ShareKind::Code];
    pub fn as_str(self) -> &'static str {
        match self {
            ShareKind::Model => "model",
            ShareKind::LiveSearch => "live_search",
            ShareKind::Journal => "journal",
            ShareKind::Project => "project",
            ShareKind::Report => "report",
            ShareKind::Code => "code",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "model" | "models" => ShareKind::Model,
            "live_search" | "search" | "searches" | "live_searches" => ShareKind::LiveSearch,
            "journal" | "journals" => ShareKind::Journal,
            "project" | "projects" => ShareKind::Project,
            "report" | "reports" => ShareKind::Report,
            "code" | "code_space" | "code_spaces" => ShareKind::Code,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            ShareKind::Model => "model",
            ShareKind::LiveSearch => "live search",
            ShareKind::Journal => "journal",
            ShareKind::Project => "project",
            ShareKind::Report => "report",
            ShareKind::Code => "code space",
        }
    }
}

/// One item shared with one exchange, or with every active exchange (`exchange_id` None).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Share {
    pub id: String,
    pub kind: ShareKind,
    pub ref_id: String,
    pub name: String,
    pub exchange_id: Option<String>,
    pub author_id: Option<String>,
    pub created_at: i64,
}

impl Share {
    pub fn short_id(&self) -> &str {
        super::short(&self.id)
    }
    /// Shared over this exchange.
    pub fn applies_to(&self, exchange: &str) -> bool {
        self.exchange_id.as_deref().is_none_or(|e| e == exchange)
    }
}

/// A dark shard of our master key entrusted to a peer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entrusted {
    pub id: String,
    pub set_id: String,
    pub index: u32,
    pub threshold: u32,
    pub total: u32,
    pub exchange_id: String,
    pub peer_id: String,
    pub created_at: i64,
}

/// A peer's dark shard we hold (still sealed with their shard password).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Held {
    pub id: String,
    pub exchange_id: String,
    pub owner_peer: String,
    pub owner_label: String,
    pub set_id: String,
    pub index: u32,
    pub threshold: u32,
    pub total: u32,
    /// Lets the owner reclaim it from a fresh node (derived from their shard password).
    pub claim: String,
    /// The sealed shard (JSON of a password-sealed blob).
    pub blob: String,
    pub stored_at: i64,
}

/// Fields of a new exchange.
#[derive(Debug, Clone, Default)]
pub struct NewExchange {
    pub id: String,
    pub label: String,
    pub role: String,
    pub status: Option<ExchangeStatus>,
    pub peer_id: Option<String>,
    pub peer_label: String,
    pub scope: serde_json::Value,
    pub offer: String,
    pub expires_at: Option<i64>,
}

const EX_COLS: &str = "id, label, role, status, peer_id, peer_label, scope, offer, answer, short_code, fingerprint, expires_at, \
                       revoked_by, author_id, created_at, updated_at, verified_at, revoked_at";

fn json_list(s: String) -> Vec<String> {
    serde_json::from_str(&s).unwrap_or_default()
}

impl Store {
    // ---------- identity ----------

    /// This node's identity keypair (protobuf-encoded) and peer id, once created.
    pub fn node_identity(&self) -> Result<Option<(Zeroizing<Vec<u8>>, String)>> {
        Ok(self
            .conn
            .query_row("SELECT keypair, peer_id FROM net_identity WHERE id = 1", [], |r| Ok((Zeroizing::new(r.get::<_, Vec<u8>>(0)?), r.get(1)?)))
            .optional()?)
    }

    /// Store a new identity unless one exists (another process may have made one first).
    pub fn init_node_identity(&self, keypair: &[u8], peer_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO net_identity(id, keypair, peer_id, created_at) VALUES (1, ?1, ?2, ?3) ON CONFLICT(id) DO NOTHING",
            params![keypair, peer_id, now_ms()],
        )?;
        Ok(())
    }

    /// Store (or replace) this node's identity.
    pub fn set_node_identity(&self, keypair: &[u8], peer_id: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO net_identity(id, keypair, peer_id, created_at) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET keypair = excluded.keypair, peer_id = excluded.peer_id, created_at = excluded.created_at",
            params![keypair, peer_id, now_ms()],
        )?;
        Ok(())
    }

    // ---------- peers ----------

    fn row_peer(r: &rusqlite::Row) -> rusqlite::Result<NetPeer> {
        Ok(NetPeer {
            peer_id: r.get(0)?,
            label: r.get(1)?,
            addrs: json_list(r.get(2)?),
            caps: json_list(r.get(3)?),
            agent: r.get(4)?,
            source: r.get(5)?,
            first_seen: r.get(6)?,
            last_seen: r.get(7)?,
        })
    }

    pub fn net_peers(&self) -> Result<Vec<NetPeer>> {
        let mut st = self
            .conn
            .prepare("SELECT peer_id, label, addrs, caps, agent, source, first_seen, last_seen FROM net_peers ORDER BY last_seen DESC, peer_id")?;
        let v = st.query_map([], Self::row_peer)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn net_peer(&self, peer_id: &str) -> Result<Option<NetPeer>> {
        Ok(self
            .conn
            .query_row(
                "SELECT peer_id, label, addrs, caps, agent, source, first_seen, last_seen FROM net_peers WHERE peer_id = ?1",
                [peer_id],
                Self::row_peer,
            )
            .optional()?)
    }

    /// Find a peer by id, the end of its id (as listings show it) or label.
    pub fn find_net_peer(&self, key: &str) -> Result<NetPeer> {
        let k = key.trim();
        let all = self.net_peers()?;
        if let Some(p) = all.iter().find(|p| p.peer_id == k) {
            return Ok(p.clone());
        }
        let m: Vec<&NetPeer> = all.iter().filter(|p| !k.is_empty() && (p.label.eq_ignore_ascii_case(k) || p.peer_id.ends_with(k))).collect();
        match m.len() {
            1 => Ok(m[0].clone()),
            0 => Err(Error::invalid("peer", format!("`{k}` not found (see `mnnn net peers`)"))),
            _ => Err(Error::invalid("peer", format!("`{k}` matches more than one; use the full peer id"))),
        }
    }

    /// Record a sighting: addresses and capabilities merge, the label is kept unless given.
    pub fn upsert_net_peer(
        &self,
        peer_id: &str,
        label: Option<&str>,
        addrs: &[String],
        caps: Option<&[String]>,
        agent: Option<&str>,
        source: &str,
    ) -> Result<NetPeer> {
        let now = now_ms();
        let old = self.net_peer(peer_id)?;
        let mut all_addrs = old.as_ref().map(|p| p.addrs.clone()).unwrap_or_default();
        for a in addrs {
            if !all_addrs.contains(a) {
                all_addrs.push(a.clone());
            }
        }
        // keep the newest few
        if all_addrs.len() > 16 {
            let n = all_addrs.len() - 16;
            all_addrs.drain(..n);
        }
        let caps = caps.map(|c| c.to_vec()).or_else(|| old.as_ref().map(|p| p.caps.clone())).unwrap_or_default();
        let label = label.map(str::to_string).filter(|l| !l.is_empty()).or_else(|| old.as_ref().map(|p| p.label.clone())).unwrap_or_default();
        let agent = agent.map(str::to_string).or_else(|| old.as_ref().map(|p| p.agent.clone())).unwrap_or_default();
        let source = old.as_ref().map(|p| p.source.clone()).unwrap_or_else(|| source.to_string());
        self.conn.execute(
            "INSERT INTO net_peers(peer_id, label, addrs, caps, agent, source, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(peer_id) DO UPDATE SET label = excluded.label, addrs = excluded.addrs, caps = excluded.caps,
                agent = excluded.agent, last_seen = excluded.last_seen",
            params![peer_id, label, serde_json::to_string(&all_addrs)?, serde_json::to_string(&caps)?, agent, source, now],
        )?;
        self.net_peer(peer_id)?.ok_or_else(|| Error::Other("peer vanished".into()))
    }

    pub fn label_net_peer(&self, peer_id: &str, label: &str) -> Result<()> {
        self.conn.execute("UPDATE net_peers SET label = ?2 WHERE peer_id = ?1", params![peer_id, label.trim()])?;
        Ok(())
    }

    pub fn remove_net_peer(&self, peer_id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM net_peers WHERE peer_id = ?1", [peer_id])? > 0)
    }

    // ---------- exchanges ----------

    fn row_exchange(r: &rusqlite::Row) -> rusqlite::Result<Exchange> {
        let status: String = r.get(3)?;
        let scope: String = r.get(6)?;
        Ok(Exchange {
            id: r.get(0)?,
            label: r.get(1)?,
            role: r.get(2)?,
            status: ExchangeStatus::parse(&status),
            peer_id: r.get(4)?,
            peer_label: r.get(5)?,
            scope: serde_json::from_str(&scope).unwrap_or(serde_json::Value::Null),
            offer: r.get(7)?,
            answer: r.get(8)?,
            short_code: r.get(9)?,
            fingerprint: r.get(10)?,
            expires_at: r.get(11)?,
            revoked_by: r.get(12)?,
            author_id: r.get(13)?,
            created_at: r.get(14)?,
            updated_at: r.get(15)?,
            verified_at: r.get(16)?,
            revoked_at: r.get(17)?,
        })
    }

    pub fn exchanges(&self) -> Result<Vec<Exchange>> {
        let mut st = self.conn.prepare(&format!("SELECT {EX_COLS} FROM net_exchanges ORDER BY created_at DESC, id"))?;
        let v = st.query_map([], Self::row_exchange)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn exchange(&self, id: &str) -> Result<Option<Exchange>> {
        Ok(self.conn.query_row(&format!("SELECT {EX_COLS} FROM net_exchanges WHERE id = ?1"), [id], Self::row_exchange).optional()?)
    }

    /// By id, unique id prefix, label or peer label.
    pub fn find_exchange(&self, key: &str) -> Result<Exchange> {
        let k = key.trim();
        let all = self.exchanges()?;
        if let Some(x) = all.iter().find(|x| x.id == k) {
            return Ok(x.clone());
        }
        let by_name: Vec<&Exchange> = all
            .iter()
            .filter(|x| !k.is_empty() && (x.label.eq_ignore_ascii_case(k) || (x.label.is_empty() && x.peer_label.eq_ignore_ascii_case(k))))
            .collect();
        let by_prefix: Vec<&Exchange> = all.iter().filter(|x| !k.is_empty() && x.id.starts_with(k)).collect();
        match (by_name.len(), by_prefix.len()) {
            (1, _) => Ok(by_name[0].clone()),
            (0, 1) => Ok(by_prefix[0].clone()),
            (0, 0) => Err(Error::invalid("exchange", format!("`{k}` not found (see `mnnn net exchange list`)"))),
            _ => Err(Error::invalid("exchange", format!("`{k}` matches more than one; use the id"))),
        }
    }

    /// Active (complete or verified) exchanges with a peer, newest first.
    pub fn active_exchanges_with(&self, peer_id: &str) -> Result<Vec<Exchange>> {
        Ok(self.exchanges()?.into_iter().filter(|x| x.status.active() && x.peer_id.as_deref() == Some(peer_id)).collect())
    }

    pub fn insert_exchange(&self, n: &NewExchange, secret: Option<&[u8]>) -> Result<Exchange> {
        let now = now_ms();
        let author = self.current_author_id()?;
        let status = n.status.unwrap_or(ExchangeStatus::Offered);
        let scope = if n.scope.is_null() { serde_json::json!({"kind": "node"}) } else { n.scope.clone() };
        let id = if n.id.is_empty() { new_id() } else { n.id.clone() };
        self.conn.execute(
            "INSERT INTO net_exchanges(id, label, role, status, peer_id, peer_label, scope, offer, secret, expires_at, author_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
            params![
                id,
                n.label.trim(),
                n.role,
                status.as_str(),
                n.peer_id,
                n.peer_label,
                scope.to_string(),
                n.offer,
                secret,
                n.expires_at,
                author,
                now
            ],
        )?;
        self.exchange(&id)?.ok_or_else(|| Error::Other("exchange vanished".into()))
    }

    /// The ephemeral X25519 secret of an offer we made (until it completes).
    pub fn exchange_secret(&self, id: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let v: Option<Option<Vec<u8>>> = self.conn.query_row("SELECT secret FROM net_exchanges WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        Ok(v.flatten().map(Zeroizing::new))
    }

    /// The shared key of a complete exchange.
    pub fn exchange_key(&self, id: &str) -> Result<Option<Key32>> {
        let v: Option<Option<Vec<u8>>> = self.conn.query_row("SELECT key FROM net_exchanges WHERE id = ?1", [id], |r| r.get(0)).optional()?;
        match v.flatten() {
            Some(b) => {
                let b = Zeroizing::new(b);
                Ok(Some(Key32::from_slice(&b)?))
            }
            None => Ok(None),
        }
    }

    /// Complete an exchange: store the key and codes, erase the ephemeral secret.
    #[allow(clippy::too_many_arguments)]
    pub fn complete_exchange(
        &self,
        id: &str,
        peer_id: &str,
        peer_label: &str,
        answer: &str,
        key: &Key32,
        short_code: &str,
        fingerprint: &str,
    ) -> Result<Exchange> {
        let n = self.conn.execute(
            "UPDATE net_exchanges SET status = 'complete', peer_id = ?2, peer_label = ?3, answer = ?4, key = ?5, secret = NULL,
                short_code = ?6, fingerprint = ?7, updated_at = ?8 WHERE id = ?1",
            params![id, peer_id, peer_label, answer, &key.0[..], short_code, fingerprint, now_ms()],
        )?;
        if n == 0 {
            return Err(Error::invalid("exchange", format!("`{id}` not found")));
        }
        self.exchange(id)?.ok_or_else(|| Error::Other("exchange vanished".into()))
    }

    pub fn set_exchange_status(&self, id: &str, status: ExchangeStatus, by: Option<&str>) -> Result<Exchange> {
        let now = now_ms();
        match status {
            ExchangeStatus::Verified => {
                self.conn.execute("UPDATE net_exchanges SET status = 'verified', verified_at = ?2, updated_at = ?2 WHERE id = ?1", params![id, now])?
            }
            ExchangeStatus::Revoked => self.conn.execute(
                "UPDATE net_exchanges SET status = 'revoked', key = NULL, secret = NULL, revoked_at = ?2, revoked_by = ?3, updated_at = ?2 WHERE id = ?1",
                params![id, now, by.unwrap_or("me")],
            )?,
            s => self.conn.execute("UPDATE net_exchanges SET status = ?2, updated_at = ?3 WHERE id = ?1", params![id, s.as_str(), now])?,
        };
        self.exchange(id)?.ok_or_else(|| Error::invalid("exchange", format!("`{id}` not found")))
    }

    pub fn label_exchange(&self, id: &str, label: &str) -> Result<()> {
        self.conn.execute("UPDATE net_exchanges SET label = ?2, updated_at = ?3 WHERE id = ?1", params![id, label.trim(), now_ms()])?;
        Ok(())
    }

    /// Exchanges with every secret column, for `mnnn net exchange export`.
    pub fn exchange_rows_raw(&self) -> Result<Vec<serde_json::Value>> {
        let mut st = self.conn.prepare(&format!("SELECT {EX_COLS}, secret, key FROM net_exchanges ORDER BY created_at"))?;
        let rows = st.query_map([], |r| {
            let x = Self::row_exchange(r)?;
            let secret: Option<Vec<u8>> = r.get(18)?;
            let key: Option<Vec<u8>> = r.get(19)?;
            Ok((x, secret, key))
        })?;
        let mut out = vec![];
        for row in rows {
            let (x, secret, key) = row?;
            let secret = secret.map(Zeroizing::new);
            let key = key.map(Zeroizing::new);
            let mut v = serde_json::to_value(&x)?;
            v["secret"] = secret.map(|s| serde_json::Value::String(hex::encode(&*s))).unwrap_or(serde_json::Value::Null);
            v["key"] = key.map(|s| serde_json::Value::String(hex::encode(&*s))).unwrap_or(serde_json::Value::Null);
            out.push(v);
        }
        Ok(out)
    }

    /// Put back an exported exchange (replacing one with the same id).
    pub fn restore_exchange_raw(&self, x: &Exchange, secret: Option<&[u8]>, key: Option<&[u8]>) -> Result<()> {
        self.conn.execute("DELETE FROM net_exchanges WHERE id = ?1", [&x.id])?;
        self.conn.execute(
            &format!("INSERT INTO net_exchanges({EX_COLS}, secret, key) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)"),
            params![
                x.id,
                x.label,
                x.role,
                x.status.as_str(),
                x.peer_id,
                x.peer_label,
                x.scope.to_string(),
                x.offer,
                x.answer,
                x.short_code,
                x.fingerprint,
                x.expires_at,
                x.revoked_by,
                x.author_id,
                x.created_at,
                x.updated_at,
                x.verified_at,
                x.revoked_at,
                secret,
                key
            ],
        )?;
        Ok(())
    }

    // ---------- shares ----------

    fn row_share(r: &rusqlite::Row) -> rusqlite::Result<Share> {
        let kind: String = r.get(1)?;
        let ex: String = r.get(4)?;
        Ok(Share {
            id: r.get(0)?,
            kind: ShareKind::parse(&kind).unwrap_or(ShareKind::Model),
            ref_id: r.get(2)?,
            name: r.get(3)?,
            exchange_id: if ex == "*" { None } else { Some(ex) },
            author_id: r.get(5)?,
            created_at: r.get(6)?,
        })
    }

    pub fn shares(&self) -> Result<Vec<Share>> {
        let mut st =
            self.conn.prepare("SELECT id, kind, ref_id, name, exchange_id, author_id, created_at FROM net_shares ORDER BY kind, name, created_at")?;
        let v = st.query_map([], Self::row_share)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Shared over one exchange (shares with everyone included).
    pub fn shares_for(&self, exchange: &str) -> Result<Vec<Share>> {
        Ok(self.shares()?.into_iter().filter(|s| s.applies_to(exchange)).collect())
    }

    /// Share an item (`exchange` None: with every active exchange). Returns it and whether it is new.
    pub fn add_share(&self, kind: ShareKind, ref_id: &str, name: &str, exchange: Option<&str>) -> Result<(Share, bool)> {
        let ex = exchange.unwrap_or("*");
        let existing: Option<String> = self
            .conn
            .query_row("SELECT id FROM net_shares WHERE kind = ?1 AND ref_id = ?2 AND exchange_id = ?3", params![kind.as_str(), ref_id, ex], |r| {
                r.get(0)
            })
            .optional()?;
        let (id, new) = match existing {
            Some(id) => (id, false),
            None => {
                let id = new_id();
                self.conn.execute(
                    "INSERT INTO net_shares(id, kind, ref_id, name, exchange_id, author_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![id, kind.as_str(), ref_id, name, ex, self.current_author_id()?, now_ms()],
                )?;
                (id, true)
            }
        };
        let s = self.shares()?.into_iter().find(|s| s.id == id).ok_or_else(|| Error::Other("share vanished".into()))?;
        Ok((s, new))
    }

    /// Stop sharing an item (`exchange` None: everywhere it is shared). Returns what was removed.
    pub fn remove_share(&self, kind: ShareKind, ref_id: &str, exchange: Option<&str>) -> Result<Vec<Share>> {
        let gone: Vec<Share> = self
            .shares()?
            .into_iter()
            .filter(|s| s.kind == kind && s.ref_id == ref_id && exchange.is_none_or(|e| s.exchange_id.as_deref() == Some(e)))
            .collect();
        for s in &gone {
            self.conn.execute("DELETE FROM net_shares WHERE id = ?1", [&s.id])?;
        }
        Ok(gone)
    }

    // ---------- dark shards ----------

    pub fn add_entrusted(&self, e: &Entrusted) -> Result<()> {
        self.conn.execute(
            "INSERT INTO net_entrusted(id, set_id, idx, threshold, total, exchange_id, peer_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(set_id, idx) DO UPDATE SET exchange_id = excluded.exchange_id, peer_id = excluded.peer_id, created_at = excluded.created_at",
            params![e.id, e.set_id, e.index, e.threshold, e.total, e.exchange_id, e.peer_id, e.created_at],
        )?;
        Ok(())
    }

    pub fn entrusted(&self) -> Result<Vec<Entrusted>> {
        let mut st = self.conn.prepare(
            "SELECT id, set_id, idx, threshold, total, exchange_id, peer_id, created_at FROM net_entrusted ORDER BY created_at DESC, set_id, idx",
        )?;
        let v = st
            .query_map([], |r| {
                Ok(Entrusted {
                    id: r.get(0)?,
                    set_id: r.get(1)?,
                    index: r.get(2)?,
                    threshold: r.get(3)?,
                    total: r.get(4)?,
                    exchange_id: r.get(5)?,
                    peer_id: r.get(6)?,
                    created_at: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Hold a peer's sealed shard (replacing the same set and index from that peer).
    pub fn hold_shard(&self, h: &Held) -> Result<()> {
        self.conn.execute(
            "INSERT INTO net_custody(id, exchange_id, owner_peer, owner_label, set_id, idx, threshold, total, claim, blob, stored_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(owner_peer, set_id, idx) DO UPDATE SET exchange_id = excluded.exchange_id, owner_label = excluded.owner_label,
                claim = excluded.claim, blob = excluded.blob, stored_at = excluded.stored_at",
            params![h.id, h.exchange_id, h.owner_peer, h.owner_label, h.set_id, h.index, h.threshold, h.total, h.claim, h.blob, h.stored_at],
        )?;
        Ok(())
    }

    pub fn held_shards(&self) -> Result<Vec<Held>> {
        let mut st = self.conn.prepare(
            "SELECT id, exchange_id, owner_peer, owner_label, set_id, idx, threshold, total, claim, blob, stored_at FROM net_custody
             ORDER BY stored_at DESC, set_id, idx",
        )?;
        let v = st
            .query_map([], |r| {
                Ok(Held {
                    id: r.get(0)?,
                    exchange_id: r.get(1)?,
                    owner_peer: r.get(2)?,
                    owner_label: r.get(3)?,
                    set_id: r.get(4)?,
                    index: r.get(5)?,
                    threshold: r.get(6)?,
                    total: r.get(7)?,
                    claim: r.get(8)?,
                    blob: r.get(9)?,
                    stored_at: r.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Drop a held shard (by id or id prefix). Returns whether one was removed.
    pub fn drop_held_shard(&self, key: &str) -> Result<bool> {
        let k = key.trim();
        let ids: Vec<String> =
            self.held_shards()?.into_iter().filter(|h| h.id == k || (!k.is_empty() && h.id.starts_with(k))).map(|h| h.id).collect();
        if ids.len() > 1 {
            return Err(Error::invalid("shard", format!("`{k}` matches more than one; use the id")));
        }
        for id in &ids {
            self.conn.execute("DELETE FROM net_custody WHERE id = ?1", [id])?;
        }
        Ok(!ids.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_merge_sightings() {
        let s = Store::open_in_memory().unwrap();
        s.upsert_net_peer("12D3KooWabc", Some("lab"), &["/ip4/1.2.3.4/tcp/4741".into()], None, None, "mdns").unwrap();
        let p = s
            .upsert_net_peer("12D3KooWabc", None, &["/ip4/5.6.7.8/tcp/4741".into()], Some(&["relay".into()]), Some("muninn/2.6"), "identify")
            .unwrap();
        assert_eq!(p.label, "lab");
        assert_eq!(p.addrs.len(), 2);
        assert!(p.is_relay());
        assert_eq!(p.source, "mdns");
        assert_eq!(s.find_net_peer("lab").unwrap().peer_id, "12D3KooWabc");
        assert_eq!(s.find_net_peer("KooWabc").unwrap().peer_id, "12D3KooWabc");
        assert!(s.find_net_peer("zzz").is_err());
        assert!(s.remove_net_peer("12D3KooWabc").unwrap());
        assert!(s.net_peers().unwrap().is_empty());
    }

    #[test]
    fn exchanges_complete_verify_and_revoke_erasing_keys() {
        let s = Store::open_in_memory().unwrap();
        let x = s
            .insert_exchange(
                &NewExchange { label: "anna".into(), role: "offerer".into(), offer: "tok".into(), ..Default::default() },
                Some(&[7u8; 32]),
            )
            .unwrap();
        assert_eq!(x.status, ExchangeStatus::Offered);
        assert_eq!(x.scope["kind"], "node");
        assert_eq!(s.exchange_secret(&x.id).unwrap().unwrap().len(), 32);
        let k = Key32([9u8; 32]);
        let x = s.complete_exchange(&x.id, "12D3KooWpeer", "anna's lab", "ans", &k, "12345-67890", "ab12 cd34").unwrap();
        assert_eq!(x.status, ExchangeStatus::Complete);
        assert!(s.exchange_secret(&x.id).unwrap().is_none());
        assert_eq!(s.exchange_key(&x.id).unwrap().unwrap().0, [9u8; 32]);
        assert_eq!(s.active_exchanges_with("12D3KooWpeer").unwrap().len(), 1);
        assert_eq!(s.find_exchange("anna").unwrap().id, x.id);
        let x = s.set_exchange_status(&x.id, ExchangeStatus::Verified, None).unwrap();
        assert!(x.verified_at.is_some());
        let raw = s.exchange_rows_raw().unwrap();
        assert_eq!(raw[0]["key"], hex::encode([9u8; 32]));
        let x = s.set_exchange_status(&x.id, ExchangeStatus::Revoked, Some("peer")).unwrap();
        assert_eq!(x.revoked_by.as_deref(), Some("peer"));
        assert!(s.exchange_key(&x.id).unwrap().is_none());
        assert!(s.active_exchanges_with("12D3KooWpeer").unwrap().is_empty());
        // restore the exported row
        let back: Exchange = serde_json::from_value(raw[0].clone()).unwrap();
        s.restore_exchange_raw(&back, None, Some(&[9u8; 32])).unwrap();
        assert_eq!(s.exchange(&x.id).unwrap().unwrap().status, ExchangeStatus::Verified);
        assert!(s.exchange_key(&x.id).unwrap().is_some());
    }

    #[test]
    fn shares_are_per_exchange_or_everyone() {
        let s = Store::open_in_memory().unwrap();
        let (a, new) = s.add_share(ShareKind::Journal, "j1", "lab notes", Some("ex1")).unwrap();
        assert!(new);
        assert!(!s.add_share(ShareKind::Journal, "j1", "lab notes", Some("ex1")).unwrap().1);
        s.add_share(ShareKind::Model, "m1", "trades", None).unwrap();
        assert_eq!(s.shares_for("ex1").unwrap().len(), 2);
        assert_eq!(s.shares_for("ex2").unwrap().len(), 1);
        assert!(a.applies_to("ex1") && !a.applies_to("ex2"));
        assert_eq!(s.remove_share(ShareKind::Journal, "j1", None).unwrap().len(), 1);
        assert_eq!(s.shares().unwrap().len(), 1);
    }

    #[test]
    fn shards_held_and_entrusted() {
        let s = Store::open_in_memory().unwrap();
        let e = Entrusted {
            id: "e1".into(),
            set_id: "abcd".into(),
            index: 1,
            threshold: 2,
            total: 3,
            exchange_id: "x".into(),
            peer_id: "p".into(),
            created_at: 1,
        };
        s.add_entrusted(&e).unwrap();
        assert_eq!(s.entrusted().unwrap(), vec![e]);
        let h = Held {
            id: "h1".into(),
            exchange_id: "x".into(),
            owner_peer: "p".into(),
            owner_label: "anna".into(),
            set_id: "abcd".into(),
            index: 2,
            threshold: 2,
            total: 3,
            claim: "c".into(),
            blob: "{}".into(),
            stored_at: 1,
        };
        s.hold_shard(&h).unwrap();
        s.hold_shard(&Held { id: "h2".into(), blob: "{\"v\":2}".into(), ..h.clone() }).unwrap();
        let held = s.held_shards().unwrap();
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].blob, "{\"v\":2}");
        assert!(s.drop_held_shard("h1").unwrap());
        assert!(s.held_shards().unwrap().is_empty());
    }
}
