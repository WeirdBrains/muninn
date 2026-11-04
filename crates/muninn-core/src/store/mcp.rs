//! MCP (2.20): the clients allowed to call this node's MCP server, the external MCP servers
//! you added (with their credentials), and which external tools you allowed rule sets to
//! call. All node-local (not synced): credentials stay on the node they were given to.
//! The protocol is `crate::mcp`; see `docs/mcp.md`.
use super::{Store, new_id, now_ms};
use crate::error::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MIGRATION: &str = r#"
CREATE TABLE mcp_clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL DEFAULT 'read',
    token_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER,
    revoked_at INTEGER
);
CREATE TABLE mcp_servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    transport TEXT NOT NULL,
    command TEXT NOT NULL DEFAULT '',
    args TEXT NOT NULL DEFAULT '[]',
    url TEXT NOT NULL DEFAULT '',
    secrets TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE mcp_approvals (
    server_id TEXT NOT NULL,
    tool TEXT NOT NULL,
    approved_at INTEGER NOT NULL,
    PRIMARY KEY (server_id, tool)
);
"#;

/// What an MCP client may do. `read` is the default; `write` adds changes, `run` adds
/// running searches, code spaces and executables, `full` is both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpRole {
    #[default]
    Read,
    Write,
    Run,
    Full,
}

impl McpRole {
    pub const ALL: [McpRole; 4] = [McpRole::Read, McpRole::Write, McpRole::Run, McpRole::Full];
    pub fn as_str(self) -> &'static str {
        match self {
            McpRole::Read => "read",
            McpRole::Write => "write",
            McpRole::Run => "run",
            McpRole::Full => "full",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.as_str().eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| Error::invalid("role", format!("`{s}`: read, write, run or full")))
    }
    pub fn can_write(self) -> bool {
        matches!(self, McpRole::Write | McpRole::Full)
    }
    pub fn can_run(self) -> bool {
        matches!(self, McpRole::Run | McpRole::Full)
    }
}

/// A named, revocable caller of this node's MCP server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpClient {
    pub id: String,
    pub name: String,
    pub role: McpRole,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

/// An external MCP server: a command run over stdio, or an HTTP endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServer {
    pub id: String,
    pub name: String,
    /// `stdio` or `http`.
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    /// Credentials: environment variables (stdio) or HTTP headers, by name. Never printed.
    #[serde(skip_serializing)]
    pub secrets: Secrets,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
}

impl Secrets {
    /// Names only, for listings.
    pub fn names(&self) -> Vec<String> {
        self.env.keys().map(|k| format!("env {k}")).chain(self.headers.keys().map(|k| format!("header {k}"))).collect()
    }
}

const CCOLS: &str = "id, name, role, created_at, last_seen_at, revoked_at";
const SCOLS: &str = "id, name, transport, command, args, url, secrets, created_at, updated_at";

fn crow(r: &rusqlite::Row) -> rusqlite::Result<McpClient> {
    Ok(McpClient {
        id: r.get(0)?,
        name: r.get(1)?,
        role: McpRole::parse(&r.get::<_, String>(2)?).unwrap_or_default(),
        created_at: r.get(3)?,
        last_seen_at: r.get(4)?,
        revoked_at: r.get(5)?,
    })
}

fn srow(r: &rusqlite::Row) -> rusqlite::Result<McpServer> {
    Ok(McpServer {
        id: r.get(0)?,
        name: r.get(1)?,
        transport: r.get(2)?,
        command: r.get(3)?,
        args: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
        url: r.get(5)?,
        secrets: serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default(),
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

/// The stored form of a client token.
pub fn token_hash(token: &str) -> String {
    blake3::hash(token.trim().as_bytes()).to_hex().to_string()
}

fn check_name(what: &'static str, n: &str) -> Result<String> {
    let n = n.trim();
    if n.is_empty() || !n.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err(Error::invalid(what, format!("`{n}`: letters, digits, - _ and . only")));
    }
    Ok(n.to_string())
}

impl Store {
    // ---------- clients ----------

    /// Add a client; returns it with its token (shown once, only its hash is kept).
    pub fn add_mcp_client(&self, name: &str, role: McpRole) -> Result<(McpClient, String)> {
        let name = check_name("client name", name)?;
        if self.mcp_clients()?.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
            return Err(Error::invalid("client name", format!("`{name}` exists (`mnnn mcp client remove {name}` first)")));
        }
        let mut b = [0u8; 32];
        getrandom::getrandom(&mut b).map_err(|e| Error::Crypto(e.to_string()))?;
        let token = format!("mnnn_mcp_{}", hex::encode(b));
        let c = McpClient { id: new_id(), name, role, created_at: now_ms(), last_seen_at: None, revoked_at: None };
        self.conn.execute(
            "INSERT INTO mcp_clients(id, name, role, token_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![c.id, c.name, c.role.as_str(), token_hash(&token), c.created_at],
        )?;
        Ok((c, token))
    }

    pub fn mcp_clients(&self) -> Result<Vec<McpClient>> {
        let mut st = self.conn.prepare(&format!("SELECT {CCOLS} FROM mcp_clients ORDER BY name"))?;
        Ok(st.query_map([], crow)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn find_mcp_client(&self, key: &str) -> Result<McpClient> {
        super::pick(self.mcp_clients()?, key, |c| &c.id, |c| &c.name, "MCP client", "mnnn mcp client list")
    }

    /// The client a token belongs to (revoked ones included; the caller checks).
    pub fn mcp_client_by_token(&self, token: &str) -> Result<Option<McpClient>> {
        Ok(self.conn.query_row(&format!("SELECT {CCOLS} FROM mcp_clients WHERE token_hash = ?1"), [token_hash(token)], crow).optional()?)
    }

    pub fn set_mcp_client_role(&self, id: &str, role: McpRole) -> Result<()> {
        self.conn.execute("UPDATE mcp_clients SET role = ?2 WHERE id = ?1", params![id, role.as_str()])?;
        Ok(())
    }

    /// Revoke (or restore) a client.
    pub fn revoke_mcp_client(&self, id: &str, revoked: bool) -> Result<()> {
        self.conn.execute("UPDATE mcp_clients SET revoked_at = ?2 WHERE id = ?1", params![id, revoked.then(now_ms)])?;
        Ok(())
    }

    pub fn remove_mcp_client(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM mcp_clients WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn touch_mcp_client(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE mcp_clients SET last_seen_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    // ---------- external servers ----------

    pub fn add_mcp_server(&self, name: &str, transport: &str, command: &str, args: &[String], url: &str, secrets: &Secrets) -> Result<McpServer> {
        let name = check_name("server name", name)?;
        if self.mcp_servers()?.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
            return Err(Error::invalid("server name", format!("`{name}` exists (`mnnn mcp remove {name}` first)")));
        }
        match transport {
            "stdio" if command.trim().is_empty() => return Err(Error::invalid("server", "a stdio server needs a command")),
            "http" if !(url.starts_with("http://") || url.starts_with("https://")) => {
                return Err(Error::invalid("server", format!("`{url}`: an http(s) URL")));
            }
            "stdio" | "http" => {}
            t => return Err(Error::invalid("transport", format!("`{t}`: stdio or http"))),
        }
        let now = now_ms();
        let s = McpServer {
            id: new_id(),
            name,
            transport: transport.into(),
            command: command.trim().into(),
            args: args.to_vec(),
            url: url.trim().into(),
            secrets: secrets.clone(),
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            &format!("INSERT INTO mcp_servers({SCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"),
            params![s.id, s.name, s.transport, s.command, serde_json::to_string(&s.args)?, s.url, serde_json::to_string(&s.secrets)?, now, now],
        )?;
        Ok(s)
    }

    pub fn mcp_servers(&self) -> Result<Vec<McpServer>> {
        let mut st = self.conn.prepare(&format!("SELECT {SCOLS} FROM mcp_servers ORDER BY name"))?;
        Ok(st.query_map([], srow)?.collect::<rusqlite::Result<_>>()?)
    }

    pub fn mcp_server(&self, id: &str) -> Result<Option<McpServer>> {
        Ok(self.conn.query_row(&format!("SELECT {SCOLS} FROM mcp_servers WHERE id = ?1"), [id], srow).optional()?)
    }

    pub fn find_mcp_server(&self, key: &str) -> Result<McpServer> {
        super::pick(self.mcp_servers()?, key, |s| &s.id, |s| &s.name, "MCP server", "mnnn mcp list")
    }

    pub fn set_mcp_secrets(&self, id: &str, secrets: &Secrets) -> Result<()> {
        self.conn
            .execute("UPDATE mcp_servers SET secrets = ?2, updated_at = ?3 WHERE id = ?1", params![id, serde_json::to_string(secrets)?, now_ms()])?;
        Ok(())
    }

    pub fn remove_mcp_server(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM mcp_approvals WHERE server_id = ?1", [id])?;
        self.conn.execute("DELETE FROM mcp_servers WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---------- approvals ----------

    pub fn mcp_approved(&self, server: &str, tool: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row("SELECT 1 FROM mcp_approvals WHERE server_id = ?1 AND tool = ?2", params![server, tool], |_| Ok(()))
            .optional()?
            .is_some())
    }

    pub fn set_mcp_approved(&self, server: &str, tool: &str, on: bool) -> Result<()> {
        if on {
            self.conn
                .execute("INSERT OR IGNORE INTO mcp_approvals(server_id, tool, approved_at) VALUES (?1, ?2, ?3)", params![server, tool, now_ms()])?;
        } else {
            self.conn.execute("DELETE FROM mcp_approvals WHERE server_id = ?1 AND tool = ?2", params![server, tool])?;
        }
        Ok(())
    }

    pub fn mcp_approvals(&self, server: &str) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT tool FROM mcp_approvals WHERE server_id = ?1 ORDER BY tool")?;
        Ok(st.query_map([server], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_servers_and_approvals() {
        let s = Store::open_in_memory().unwrap();
        let (c, token) = s.add_mcp_client("claude", McpRole::Read).unwrap();
        assert!(token.starts_with("mnnn_mcp_"));
        assert_eq!(s.mcp_client_by_token(&token).unwrap().unwrap().id, c.id);
        assert!(s.mcp_client_by_token("nope").unwrap().is_none());
        assert!(s.add_mcp_client("Claude", McpRole::Full).is_err());
        assert!(s.add_mcp_client("bad name", McpRole::Full).is_err());
        s.revoke_mcp_client(&c.id, true).unwrap();
        assert!(s.find_mcp_client("claude").unwrap().revoked_at.is_some());
        let mut sec = Secrets::default();
        sec.headers.insert("Authorization".into(), "Bearer sk-secret".into());
        let v = s.add_mcp_server("weather", "http", "", &[], "https://example.test/mcp", &sec).unwrap();
        assert!(s.add_mcp_server("x", "http", "", &[], "ftp://x", &Secrets::default()).is_err());
        assert!(s.add_mcp_server("y", "stdio", " ", &[], "", &Secrets::default()).is_err());
        let back = s.find_mcp_server("weather").unwrap();
        assert_eq!(back.secrets, sec);
        // credentials are never serialized
        assert!(!serde_json::to_string(&back).unwrap().contains("sk-secret"));
        assert!(!s.mcp_approved(&v.id, "forecast").unwrap());
        s.set_mcp_approved(&v.id, "forecast", true).unwrap();
        assert!(s.mcp_approved(&v.id, "forecast").unwrap());
        assert_eq!(s.mcp_approvals(&v.id).unwrap(), vec!["forecast"]);
        s.remove_mcp_server(&v.id).unwrap();
        assert!(s.mcp_servers().unwrap().is_empty());
        assert!(!s.mcp_approved(&v.id, "forecast").unwrap());
    }
}
