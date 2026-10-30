//! Users (2.10): a person's identity, separate from any node. A user has an Ed25519 signing
//! key and certifies each node it links; certificates and revocations are kept here so any
//! node can check that a peer acts for a user. Connections are user-to-user and are trusted
//! or untrusted (default untrusted). See `docs/users.md`.
//!
//! The user's private key is stored only for this node's own user, inside the encrypted
//! database, like the node identity.
use super::{Store, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub const MIGRATION: &str = r#"
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL DEFAULT '',
    public_key BLOB NOT NULL,
    keypair BLOB,
    author_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE user_nodes (
    user_id TEXT NOT NULL,
    peer_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    cert BLOB NOT NULL,
    issued_at INTEGER NOT NULL,
    revoked_at INTEGER,
    revocation BLOB,
    PRIMARY KEY (user_id, peer_id)
);
CREATE INDEX user_nodes_peer ON user_nodes(peer_id);
CREATE TABLE connections (
    user_id TEXT PRIMARY KEY,
    trusted INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
"#;

/// 2.10: organizations and memberships belong to users. Existing orgs move to this
/// node's user the first time it is made; members get their user once their card is known.
pub const MIGRATION_ORGS: &str = r#"
ALTER TABLE orgs ADD COLUMN owner_user TEXT;
ALTER TABLE org_members ADD COLUMN user_id TEXT;
CREATE INDEX org_members_user ON org_members(user_id);
"#;

const SELF_USER: &str = "self_user";

/// A user this node knows: its own, or one met through a connection, org or review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub label: String,
    pub public_key: Vec<u8>,
    /// True for this node's own user (the one holding the signing key here).
    pub mine: bool,
    pub author_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl User {
    pub fn short_id(&self) -> &str {
        &self.id[..self.id.len().min(8)]
    }
    /// The label, or the short id when there is none.
    pub fn name(&self) -> String {
        if self.label.is_empty() { self.short_id().to_string() } else { self.label.clone() }
    }
}

/// A node linked to a user: the user's signature over the node's peer id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserNode {
    pub user_id: String,
    pub peer_id: String,
    pub label: String,
    pub cert: Vec<u8>,
    pub issued_at: i64,
    pub revoked_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<Vec<u8>>,
}

impl UserNode {
    pub fn active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// A connection to another user, trusted or not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub user_id: String,
    pub trusted: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

const USER_COLS: &str = "id, label, public_key, keypair IS NOT NULL, author_id, created_at, updated_at";

impl Store {
    fn row_user(r: &rusqlite::Row) -> rusqlite::Result<User> {
        Ok(User {
            id: r.get(0)?,
            label: r.get(1)?,
            public_key: r.get(2)?,
            mine: r.get(3)?,
            author_id: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
        })
    }

    pub fn users(&self) -> Result<Vec<User>> {
        let mut st = self.conn.prepare(&format!("SELECT {USER_COLS} FROM users ORDER BY keypair IS NULL, label, id"))?;
        Ok(st.query_map([], Self::row_user)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn user(&self, id: &str) -> Result<Option<User>> {
        Ok(self.conn.query_row(&format!("SELECT {USER_COLS} FROM users WHERE id = ?1"), [id], Self::row_user).optional()?)
    }

    /// A user by id, id prefix or label.
    pub fn find_user(&self, key: &str) -> Result<User> {
        let all = self.users()?;
        if let Some(u) = all.iter().find(|u| u.id == key) {
            return Ok(u.clone());
        }
        let m: Vec<_> = all.iter().filter(|u| u.label == key || (key.len() >= 4 && u.id.starts_with(key))).collect();
        match m.as_slice() {
            [u] => Ok((*u).clone()),
            [] => Err(Error::invalid("user", format!("`{key}` not found (see `mnnn user list`)"))),
            _ => Err(Error::invalid("user", format!("`{key}` matches {} users; use more of the id", m.len()))),
        }
    }

    /// This node's own user, once made.
    pub fn self_user(&self) -> Result<Option<User>> {
        match self.setting(SELF_USER)? {
            Some(id) => self.user(&id),
            None => Ok(None),
        }
    }

    /// The signing keypair (protobuf-encoded) of this node's own user.
    pub fn self_user_keypair(&self) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let Some(id) = self.setting(SELF_USER)? else { return Ok(None) };
        Ok(self
            .conn
            .query_row("SELECT keypair FROM users WHERE id = ?1", [&id], |r| r.get::<_, Option<Vec<u8>>>(0))
            .optional()?
            .flatten()
            .map(Zeroizing::new))
    }

    /// Store this node's own user with its keypair, unless one exists already.
    pub fn init_self_user(&self, id: &str, label: &str, public_key: &[u8], keypair: &[u8], author_id: Option<&str>) -> Result<User> {
        if let Some(u) = self.self_user()? {
            return Ok(u);
        }
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO users(id, label, public_key, keypair, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET keypair = excluded.keypair, author_id = excluded.author_id, updated_at = excluded.updated_at",
            params![id, label, public_key, keypair, author_id, now],
        )?;
        self.set_setting(SELF_USER, Some(id))?;
        self.user(id)?.ok_or_else(|| Error::invalid("user", "not stored"))
    }

    /// Replace this node's own user (linking to another user's identity).
    pub fn adopt_self_user(&self, id: &str, label: &str, public_key: &[u8], keypair: &[u8]) -> Result<User> {
        let now = now_ms();
        let author = self.self_user()?.and_then(|u| u.author_id);
        self.conn.execute(
            "INSERT INTO users(id, label, public_key, keypair, author_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(id) DO UPDATE SET label = excluded.label, keypair = excluded.keypair, updated_at = excluded.updated_at",
            params![id, label, public_key, keypair, author, now],
        )?;
        if let Some(old) = self.setting(SELF_USER)?.filter(|o| o != id) {
            self.conn.execute("UPDATE users SET keypair = NULL WHERE id = ?1", [&old])?;
        }
        self.set_setting(SELF_USER, Some(id))?;
        self.user(id)?.ok_or_else(|| Error::invalid("user", "not stored"))
    }

    /// Remember another user (public key only); the label is kept if already set.
    pub fn upsert_user(&self, id: &str, label: &str, public_key: &[u8]) -> Result<User> {
        let now = now_ms();
        self.conn.execute(
            "INSERT INTO users(id, label, public_key, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET label = CASE WHEN users.label = '' THEN excluded.label ELSE users.label END",
            params![id, label, public_key, now],
        )?;
        self.user(id)?.ok_or_else(|| Error::invalid("user", "not stored"))
    }

    pub fn label_user(&self, id: &str, label: &str) -> Result<()> {
        self.conn.execute("UPDATE users SET label = ?2, updated_at = ?3 WHERE id = ?1", params![id, label, now_ms()])?;
        Ok(())
    }

    // ---------- linked nodes ----------

    fn row_user_node(r: &rusqlite::Row) -> rusqlite::Result<UserNode> {
        Ok(UserNode {
            user_id: r.get(0)?,
            peer_id: r.get(1)?,
            label: r.get(2)?,
            cert: r.get(3)?,
            issued_at: r.get(4)?,
            revoked_at: r.get(5)?,
            revocation: r.get(6)?,
        })
    }

    /// The nodes linked to a user (revoked ones included).
    pub fn user_nodes(&self, user_id: &str) -> Result<Vec<UserNode>> {
        let mut st = self.conn.prepare(
            "SELECT user_id, peer_id, label, cert, issued_at, revoked_at, revocation FROM user_nodes WHERE user_id = ?1 ORDER BY issued_at",
        )?;
        Ok(st.query_map([user_id], Self::row_user_node)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The user a node acts for (its active certificate), if any.
    pub fn user_of_node(&self, peer_id: &str) -> Result<Option<UserNode>> {
        Ok(self
            .conn
            .query_row(
                "SELECT user_id, peer_id, label, cert, issued_at, revoked_at, revocation FROM user_nodes
                 WHERE peer_id = ?1 AND revoked_at IS NULL ORDER BY issued_at DESC LIMIT 1",
                [peer_id],
                Self::row_user_node,
            )
            .optional()?)
    }

    /// Record a (checked) certificate. A revoked node stays revoked.
    pub fn put_user_node(&self, n: &UserNode) -> Result<()> {
        self.conn.execute(
            "INSERT INTO user_nodes(user_id, peer_id, label, cert, issued_at, revoked_at, revocation) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(user_id, peer_id) DO UPDATE SET
               label = CASE WHEN excluded.label = '' THEN user_nodes.label ELSE excluded.label END,
               cert = CASE WHEN user_nodes.revoked_at IS NULL AND excluded.issued_at >= user_nodes.issued_at THEN excluded.cert ELSE user_nodes.cert END,
               issued_at = CASE WHEN user_nodes.revoked_at IS NULL AND excluded.issued_at >= user_nodes.issued_at THEN excluded.issued_at ELSE user_nodes.issued_at END,
               revoked_at = COALESCE(user_nodes.revoked_at, excluded.revoked_at),
               revocation = COALESCE(user_nodes.revocation, excluded.revocation)",
            params![n.user_id, n.peer_id, n.label, n.cert, n.issued_at, n.revoked_at, n.revocation],
        )?;
        Ok(())
    }

    /// Mark a linked node revoked, with the user's signed revocation.
    pub fn revoke_user_node(&self, user_id: &str, peer_id: &str, at: i64, revocation: &[u8]) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE user_nodes SET revoked_at = ?3, revocation = ?4 WHERE user_id = ?1 AND peer_id = ?2 AND revoked_at IS NULL",
            params![user_id, peer_id, at, revocation],
        )? > 0)
    }

    /// Orgs this node owns (and not yet a user's) become this user's.
    pub fn claim_orgs(&self, user_id: &str, peer_id: &str) -> Result<usize> {
        Ok(self.conn.execute("UPDATE orgs SET owner_user = ?1 WHERE owner_user IS NULL AND owner_peer IN (?2, '')", params![user_id, peer_id])?)
    }
    /// Name the user of a member's node where it is known.
    pub fn tag_members(&self, peer_id: &str, user_id: &str) -> Result<usize> {
        Ok(self
            .conn
            .execute("UPDATE org_members SET user_id = ?2 WHERE peer_id = ?1 AND (user_id IS NULL OR user_id != ?2)", params![peer_id, user_id])?)
    }
    /// The user owning an org, when it is known.
    pub fn org_owner_user(&self, org_id: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT owner_user FROM orgs WHERE id = ?1", [org_id], |r| r.get(0)).optional()?.flatten())
    }

    // ---------- connections ----------

    fn row_connection(r: &rusqlite::Row) -> rusqlite::Result<Connection> {
        Ok(Connection { user_id: r.get(0)?, trusted: r.get(1)?, created_at: r.get(2)?, updated_at: r.get(3)? })
    }

    pub fn connections(&self) -> Result<Vec<Connection>> {
        let mut st = self.conn.prepare("SELECT user_id, trusted, created_at, updated_at FROM connections ORDER BY created_at")?;
        Ok(st.query_map([], Self::row_connection)?.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn connection(&self, user_id: &str) -> Result<Option<Connection>> {
        Ok(self
            .conn
            .query_row("SELECT user_id, trusted, created_at, updated_at FROM connections WHERE user_id = ?1", [user_id], Self::row_connection)
            .optional()?)
    }

    /// Add a connection (untrusted unless it already exists).
    pub fn add_connection(&self, user_id: &str) -> Result<(Connection, bool)> {
        let now = now_ms();
        let n = self.conn.execute(
            "INSERT INTO connections(user_id, trusted, created_at, updated_at) VALUES (?1, 0, ?2, ?2) ON CONFLICT(user_id) DO NOTHING",
            params![user_id, now],
        )?;
        Ok((self.connection(user_id)?.ok_or_else(|| Error::invalid("connection", "not stored"))?, n > 0))
    }

    /// Mark a connection trusted or not; returns whether it changed.
    pub fn set_connection_trust(&self, user_id: &str, trusted: bool) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE connections SET trusted = ?2, updated_at = ?3 WHERE user_id = ?1 AND trusted != ?2",
            params![user_id, trusted, now_ms()],
        )? > 0)
    }

    pub fn remove_connection(&self, user_id: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM connections WHERE user_id = ?1", [user_id])? > 0)
    }
}

/// After linking to another user: what the node's earlier user owned moves along.
pub fn move_owned(s: &Store, from: &str, to: &str) -> Result<()> {
    s.conn.execute("UPDATE orgs SET owner_user = ?2 WHERE owner_user = ?1", params![from, to])?;
    s.conn.execute("UPDATE org_members SET user_id = ?2 WHERE user_id = ?1", params![from, to])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn users_nodes_and_connections_are_kept() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.self_user().unwrap().is_none());
        let me = s.init_self_user("aa11", "me", b"pk", b"kp", None).unwrap();
        assert!(me.mine);
        assert_eq!(s.init_self_user("bb22", "other", b"pk2", b"kp2", None).unwrap().id, "aa11");
        assert_eq!(s.self_user_keypair().unwrap().unwrap().as_slice(), b"kp");
        let ben = s.upsert_user("cc33", "ben", b"pkb").unwrap();
        assert!(!ben.mine);
        assert_eq!(s.find_user("ben").unwrap().id, "cc33");

        let n = UserNode {
            user_id: "aa11".into(),
            peer_id: "P1".into(),
            label: "laptop".into(),
            cert: vec![1],
            issued_at: 5,
            revoked_at: None,
            revocation: None,
        };
        s.put_user_node(&n).unwrap();
        assert_eq!(s.user_of_node("P1").unwrap().unwrap().user_id, "aa11");
        assert!(s.revoke_user_node("aa11", "P1", 9, &[2]).unwrap());
        assert!(s.user_of_node("P1").unwrap().is_none());
        // a revoked node stays revoked even if its certificate arrives again
        s.put_user_node(&n).unwrap();
        assert!(!s.user_nodes("aa11").unwrap()[0].active());

        let (c, new) = s.add_connection("cc33").unwrap();
        assert!(new && !c.trusted);
        assert!(s.set_connection_trust("cc33", true).unwrap());
        assert!(!s.set_connection_trust("cc33", true).unwrap());
        assert!(s.connection("cc33").unwrap().unwrap().trusted);
        assert!(s.remove_connection("cc33").unwrap());
    }
}
