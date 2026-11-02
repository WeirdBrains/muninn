//! Live collaboration (2.19): messages between nodes, live editing sessions and their
//! operation logs, presence, and remote runs. Messages are synced to your linked nodes;
//! sessions, operations and presence are node-local (the merged result is written to the
//! item itself, which syncs as usual). See `crate::collab` and `docs/collaboration.md`.
use super::{Store, new_id, now_ms};
use crate::error::Result;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    peer TEXT NOT NULL,
    peer_label TEXT NOT NULL DEFAULT '',
    direction TEXT NOT NULL,
    about TEXT,
    text TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    delivered_at INTEGER,
    read_at INTEGER,
    error TEXT
);
CREATE INDEX messages_peer ON messages(peer, created_at);
CREATE TABLE live_sessions (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    item_id TEXT NOT NULL DEFAULT '',
    field TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL,
    host_peer TEXT NOT NULL DEFAULT '',
    exchange_id TEXT,
    status TEXT NOT NULL DEFAULT 'open',
    seen INTEGER NOT NULL DEFAULT 0,
    peers TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE live_ops (
    session TEXT NOT NULL,
    seq INTEGER NOT NULL,
    site TEXT NOT NULL,
    doc TEXT NOT NULL DEFAULT '',
    body BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (session, seq)
);
CREATE TABLE live_presence (
    session TEXT NOT NULL,
    peer TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    cursor INTEGER,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (session, peer)
);
"#;

/// A message to or from another node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    /// The other node's peer id.
    pub peer: String,
    pub peer_label: String,
    /// `in` or `out`.
    pub direction: String,
    /// What it is about (`journal:ID`, `entry:ID`, `review:ID`, …), for per-item threads.
    pub about: Option<String>,
    pub text: String,
    /// The peer id of the node that wrote or received it (only that node delivers it).
    pub origin: String,
    pub created_at: i64,
    pub delivered_at: Option<i64>,
    pub read_at: Option<i64>,
    pub error: Option<String>,
}

const MCOLS: &str = "id, peer, peer_label, direction, about, text, origin, created_at, delivered_at, read_at, error";

fn mrow(r: &rusqlite::Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        peer: r.get(1)?,
        peer_label: r.get(2)?,
        direction: r.get(3)?,
        about: r.get(4)?,
        text: r.get(5)?,
        origin: r.get(6)?,
        created_at: r.get(7)?,
        delivered_at: r.get(8)?,
        read_at: r.get(9)?,
        error: r.get(10)?,
    })
}

/// A live session on this node: one you host or one you joined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveSession {
    pub id: String,
    /// `entry`, `journal`, `code`, `record` or `run`.
    pub kind: String,
    pub item_id: String,
    /// A record's field (records), else empty.
    pub field: String,
    pub title: String,
    /// `host` or `guest`.
    pub role: String,
    /// The host node (guests), else empty.
    pub host_peer: String,
    /// The exchange a guest reaches the host over.
    pub exchange_id: Option<String>,
    /// `open` or `closed`.
    pub status: String,
    /// The last operation seq this node has (guests: from the host).
    pub seen: i64,
    /// Hosts: the peer ids invited to join.
    pub peers: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

const SCOLS: &str = "id, kind, item_id, field, title, role, host_peer, exchange_id, status, seen, peers, created_at, updated_at";

fn srow(r: &rusqlite::Row) -> rusqlite::Result<LiveSession> {
    Ok(LiveSession {
        id: r.get(0)?,
        kind: r.get(1)?,
        item_id: r.get(2)?,
        field: r.get(3)?,
        title: r.get(4)?,
        role: r.get(5)?,
        host_peer: r.get(6)?,
        exchange_id: r.get(7)?,
        status: r.get(8)?,
        seen: r.get(9)?,
        peers: serde_json::from_str(&r.get::<_, String>(10)?).unwrap_or_default(),
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

/// One logged operation (its body is a `muninn.live.v1` protobuf message).
#[derive(Debug, Clone, PartialEq)]
pub struct LiveOp {
    pub seq: i64,
    pub site: String,
    /// Which text in the session: `body`, `source`, an entry id, a record field.
    pub doc: String,
    pub body: Vec<u8>,
    pub created_at: i64,
}

/// Who is in a session, and where.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Presence {
    pub peer: String,
    pub label: String,
    pub cursor: Option<i64>,
    pub seen_at: i64,
}

impl Store {
    // ---------- messages ----------

    /// Record a message; one with a known id is ignored (a resend). True when new.
    pub fn put_message(&self, m: &Message) -> Result<bool> {
        let n = self.conn.execute(
            &format!("INSERT OR IGNORE INTO messages({MCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
            params![m.id, m.peer, m.peer_label, m.direction, m.about, m.text, m.origin, m.created_at, m.delivered_at, m.read_at, m.error],
        )?;
        Ok(n == 1)
    }

    /// A new outgoing message (not yet delivered).
    pub fn new_message(&self, peer: &str, peer_label: &str, about: Option<&str>, text: &str, origin: &str) -> Result<Message> {
        let m = Message {
            id: new_id(),
            peer: peer.into(),
            peer_label: peer_label.into(),
            direction: "out".into(),
            about: about.map(String::from),
            text: text.into(),
            origin: origin.into(),
            created_at: now_ms(),
            delivered_at: None,
            read_at: Some(now_ms()),
            error: None,
        };
        self.put_message(&m)?;
        Ok(m)
    }

    pub fn message(&self, id: &str) -> Result<Option<Message>> {
        Ok(self.conn.query_row(&format!("SELECT {MCOLS} FROM messages WHERE id = ?1"), [id], mrow).optional()?)
    }

    /// Messages, oldest first, optionally with one peer and/or about one item.
    pub fn messages(&self, peer: Option<&str>, about: Option<&str>) -> Result<Vec<Message>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {MCOLS} FROM messages WHERE (?1 IS NULL OR peer = ?1) AND (?2 IS NULL OR about = ?2) ORDER BY created_at, id"
        ))?;
        let v = st.query_map(params![peer, about], mrow)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// Outgoing messages written on node `origin` and not yet delivered.
    pub fn undelivered_messages(&self, origin: &str) -> Result<Vec<Message>> {
        let mut st = self
            .conn
            .prepare(&format!("SELECT {MCOLS} FROM messages WHERE direction = 'out' AND delivered_at IS NULL AND origin = ?1 ORDER BY created_at"))?;
        let v = st.query_map([origin], mrow)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn mark_message_delivered(&self, id: &str, error: Option<&str>) -> Result<()> {
        if error.is_some() {
            self.conn.execute("UPDATE messages SET error = ?2 WHERE id = ?1", params![id, error])?;
        } else {
            self.conn.execute("UPDATE messages SET delivered_at = ?2, error = NULL WHERE id = ?1", params![id, now_ms()])?;
        }
        Ok(())
    }

    /// Mark incoming messages read (all, or one peer's); how many.
    pub fn mark_messages_read(&self, peer: Option<&str>) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE messages SET read_at = ?1 WHERE read_at IS NULL AND direction = 'in' AND (?2 IS NULL OR peer = ?2)",
            params![now_ms(), peer],
        )?)
    }

    pub fn unread_messages(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM messages WHERE read_at IS NULL AND direction = 'in'", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    // ---------- live sessions ----------

    pub fn put_live_session(&self, s: &LiveSession) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO live_sessions({SCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title, status = excluded.status, seen = excluded.seen,
                 peers = excluded.peers, updated_at = excluded.updated_at"
            ),
            params![
                s.id,
                s.kind,
                s.item_id,
                s.field,
                s.title,
                s.role,
                s.host_peer,
                s.exchange_id,
                s.status,
                s.seen,
                serde_json::to_string(&s.peers)?,
                s.created_at,
                s.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn live_session(&self, id: &str) -> Result<Option<LiveSession>> {
        Ok(self.conn.query_row(&format!("SELECT {SCOLS} FROM live_sessions WHERE id = ?1"), [id], srow).optional()?)
    }

    /// Newest first.
    pub fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        let mut st = self.conn.prepare(&format!("SELECT {SCOLS} FROM live_sessions ORDER BY updated_at DESC, id"))?;
        let v = st.query_map([], srow)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    /// The open session this node hosts for an item, if any.
    pub fn hosted_session(&self, kind: &str, item: &str, field: &str) -> Result<Option<LiveSession>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SCOLS} FROM live_sessions WHERE role = 'host' AND status = 'open' AND kind = ?1 AND item_id = ?2 AND field = ?3"),
                params![kind, item, field],
                srow,
            )
            .optional()?)
    }

    pub fn find_live_session(&self, key: &str) -> Result<LiveSession> {
        let all = self.live_sessions()?;
        super::pick(all, key.trim(), |s| &s.id, |s| &s.title, "live session", "mnnn live list")
    }

    pub fn set_live_session(&self, id: &str, status: Option<&str>, seen: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE live_sessions SET status = COALESCE(?2, status), seen = COALESCE(?3, seen), updated_at = ?4 WHERE id = ?1",
            params![id, status, seen, now_ms()],
        )?;
        Ok(())
    }

    /// Append an operation at the next seq; returns it.
    pub fn append_live_op(&self, session: &str, site: &str, doc: &str, body: &[u8]) -> Result<i64> {
        let seq: i64 = self.conn.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM live_ops WHERE session = ?1", [session], |r| r.get(0))?;
        self.conn.execute(
            "INSERT INTO live_ops(session, seq, site, doc, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session, seq, site, doc, body, now_ms()],
        )?;
        Ok(seq)
    }

    /// Store an operation at the seq the host gave it (guests); ignored if present.
    pub fn put_live_op(&self, session: &str, op: &LiveOp) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO live_ops(session, seq, site, doc, body, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session, op.seq, op.site, op.doc, op.body, op.created_at],
        )?;
        Ok(())
    }

    /// Operations after `since`, in order.
    pub fn live_ops(&self, session: &str, since: i64) -> Result<Vec<LiveOp>> {
        let mut st = self.conn.prepare("SELECT seq, site, doc, body, created_at FROM live_ops WHERE session = ?1 AND seq > ?2 ORDER BY seq")?;
        let v = st
            .query_map(params![session, since], |r| {
                Ok(LiveOp { seq: r.get(0)?, site: r.get(1)?, doc: r.get(2)?, body: r.get(3)?, created_at: r.get(4)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }

    pub fn live_head(&self, session: &str) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM live_ops WHERE session = ?1", [session], |r| r.get(0))?)
    }

    pub fn set_presence(&self, session: &str, peer: &str, label: &str, cursor: Option<i64>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO live_presence(session, peer, label, cursor, seen_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session, peer) DO UPDATE SET label = excluded.label, cursor = excluded.cursor, seen_at = excluded.seen_at",
            params![session, peer, label, cursor, now_ms()],
        )?;
        Ok(())
    }

    /// Who was seen in a session within `fresh_ms`.
    pub fn presence(&self, session: &str, fresh_ms: i64) -> Result<Vec<Presence>> {
        let mut st =
            self.conn.prepare("SELECT peer, label, cursor, seen_at FROM live_presence WHERE session = ?1 AND seen_at >= ?2 ORDER BY label, peer")?;
        let v = st
            .query_map(params![session, now_ms() - fresh_ms], |r| {
                Ok(Presence { peer: r.get(0)?, label: r.get(1)?, cursor: r.get(2)?, seen_at: r.get(3)? })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_dedupe_deliver_and_read() {
        let s = Store::open_in_memory().unwrap();
        let m = s.new_message("peerB", "ben", Some("journal:j1"), "hi", "peerA").unwrap();
        assert!(!s.put_message(&m).unwrap(), "a resend is ignored");
        assert_eq!(s.undelivered_messages("peerA").unwrap().len(), 1);
        assert!(s.undelivered_messages("peerC").unwrap().is_empty(), "only the writing node delivers");
        s.mark_message_delivered(&m.id, None).unwrap();
        assert!(s.undelivered_messages("peerA").unwrap().is_empty());
        let inc = Message { id: "x".into(), direction: "in".into(), read_at: None, ..m.clone() };
        s.put_message(&inc).unwrap();
        assert_eq!(s.unread_messages().unwrap(), 1);
        assert_eq!(s.messages(Some("peerB"), Some("journal:j1")).unwrap().len(), 2);
        assert_eq!(s.mark_messages_read(None).unwrap(), 1);
    }

    #[test]
    fn sessions_ops_and_presence() {
        let s = Store::open_in_memory().unwrap();
        let now = now_ms();
        let ls = LiveSession {
            id: "s1".into(),
            kind: "entry".into(),
            item_id: "e1".into(),
            field: String::new(),
            title: "Notes".into(),
            role: "host".into(),
            host_peer: String::new(),
            exchange_id: None,
            status: "open".into(),
            seen: 0,
            peers: vec!["peerB".into()],
            created_at: now,
            updated_at: now,
        };
        s.put_live_session(&ls).unwrap();
        assert_eq!(s.hosted_session("entry", "e1", "").unwrap().unwrap().id, "s1");
        assert_eq!(s.append_live_op("s1", "a", "body", b"x").unwrap(), 1);
        assert_eq!(s.append_live_op("s1", "b", "body", b"y").unwrap(), 2);
        assert_eq!(s.live_session("s1").unwrap().unwrap().peers, vec!["peerB".to_string()]);
        assert_eq!(s.live_ops("s1", 1).unwrap().len(), 1);
        assert_eq!(s.live_head("s1").unwrap(), 2);
        s.set_presence("s1", "peerB", "ben", Some(3)).unwrap();
        assert_eq!(s.presence("s1", 60_000).unwrap()[0].cursor, Some(3));
        s.set_live_session("s1", Some("closed"), None).unwrap();
        assert!(s.hosted_session("entry", "e1", "").unwrap().is_none());
    }
}
