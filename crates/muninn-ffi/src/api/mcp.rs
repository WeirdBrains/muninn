//! MCP servers and clients (`mnnn mcp` / TUI Net ▸ MCP…). Tokens and credential
//! values stay in Rust; Dart sees names, transports, roles and tool names.
use super::data::{open, open_write};
use muninn_core::mcp;
use muninn_core::store::{McpRole, NewAction, Store};
use serde_json::json;

const ORIGIN: &str = "app";

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    /// Credential *names* only (env X, header Y). Values never leave Rust.
    pub credentials: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct McpClientInfo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub revoked: bool,
}

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub approved: bool,
}

fn server_info(s: &muninn_core::store::McpServer) -> McpServerInfo {
    McpServerInfo {
        id: s.id.clone(),
        name: s.name.clone(),
        transport: s.transport.clone(),
        command: s.command.clone(),
        args: s.args.clone(),
        url: s.url.clone(),
        credentials: s.secrets.names(),
        created_at: s.created_at,
    }
}

fn client_info(c: &muninn_core::store::McpClient) -> McpClientInfo {
    McpClientInfo {
        id: c.id.clone(),
        name: c.name.clone(),
        role: c.role.as_str().to_string(),
        created_at: c.created_at,
        last_seen_at: c.last_seen_at.unwrap_or(0),
        revoked: c.revoked_at.is_some(),
    }
}

fn find_server(store: &Store, name: &str) -> anyhow::Result<muninn_core::store::McpServer> {
    let name = name.trim();
    store
        .mcp_servers()?
        .into_iter()
        .find(|s| s.id == name || s.name == name || s.id.starts_with(name) || s.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("no MCP server `{name}`"))
}

fn find_client(store: &Store, name: &str) -> anyhow::Result<muninn_core::store::McpClient> {
    let name = name.trim();
    store
        .mcp_clients()?
        .into_iter()
        .find(|c| c.id == name || c.name == name || c.id.starts_with(name) || c.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow::anyhow!("no MCP client `{name}`"))
}

pub(crate) fn mcp_servers_of(store: &Store) -> anyhow::Result<Vec<McpServerInfo>> {
    Ok(store.mcp_servers()?.iter().map(server_info).collect())
}

pub(crate) fn mcp_clients_of(store: &Store) -> anyhow::Result<Vec<McpClientInfo>> {
    Ok(store.mcp_clients()?.iter().map(client_info).collect())
}

/// TUI Net ▸ MCP… servers tab (`mnnn mcp list`).
pub fn mcp_servers() -> anyhow::Result<Vec<McpServerInfo>> {
    mcp_servers_of(&open()?)
}

/// TUI Net ▸ MCP… clients tab (`mnnn mcp client list`).
pub fn mcp_clients() -> anyhow::Result<Vec<McpClientInfo>> {
    mcp_clients_of(&open()?)
}

/// A server's tools (connects in Rust; credentials stay there) and whether each is approved.
pub fn mcp_server_tools(server: String) -> anyhow::Result<Vec<McpToolInfo>> {
    let store = open()?;
    let s = find_server(&store, &server)?;
    let tools = mcp::client::Conn::open(&s).and_then(|mut c| c.tools())?;
    let mut out = Vec::new();
    for x in tools {
        let name = x["name"].as_str().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let description = x["description"].as_str().unwrap_or_default().lines().next().unwrap_or_default().to_string();
        let approved = store.mcp_approved(&s.id, &name)?;
        out.push(McpToolInfo { name, description, approved });
    }
    Ok(out)
}

/// TUI `a` on a tool: approve or withdraw approval (`mnnn mcp approve`).
pub fn mcp_approve_tool(server: String, tool: String, on: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = find_server(&store, &server)?;
    let tool = tool.trim();
    mcp::client::approve(&store, &s, tool, ORIGIN, on)?;
    Ok(if on { format!("approved {}/{tool}", s.name) } else { format!("{}/{tool} needs approval again", s.name) })
}

/// TUI `d` on a server (`mnnn mcp remove`).
pub fn mcp_remove_server(server: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = find_server(&store, &server)?;
    store.remove_mcp_server(&s.id)?;
    store.note_action(
        &NewAction::new("mcp.server.remove", ORIGIN, format!("remove MCP server {}", s.name)).params(json!({ "server": s.id, "name": s.name })),
    );
    Ok(format!("removed {}", s.name))
}

/// TUI `o` cycles the client's role (`mnnn mcp client role`).
pub fn mcp_client_role(client: String, role: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let c = find_client(&store, &client)?;
    let role = McpRole::parse(role.trim())?;
    store.set_mcp_client_role(&c.id, role)?;
    store.note_action(
        &NewAction::new("mcp.client.role", ORIGIN, format!("MCP client {} is now {}", c.name, role.as_str()))
            .params(json!({ "client": c.name, "role": role.as_str() })),
    );
    Ok(format!("{} is now {}", c.name, role.as_str()))
}

/// TUI `r` revoke or restore (`mnnn mcp client revoke|restore`).
pub fn mcp_client_revoke(client: String, revoke: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let c = find_client(&store, &client)?;
    store.revoke_mcp_client(&c.id, revoke)?;
    let (kind, w) = if revoke { ("mcp.client.revoke", "revoked") } else { ("mcp.client.restore", "restored") };
    store.note_action(&NewAction::new(kind, ORIGIN, format!("{w} MCP client {}", c.name)).params(json!({ "client": c.name })));
    Ok(format!("{w} {}", c.name))
}

/// TUI `d` on a client (`mnnn mcp client remove`).
pub fn mcp_remove_client(client: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let c = find_client(&store, &client)?;
    store.remove_mcp_client(&c.id)?;
    store.note_action(&NewAction::new("mcp.client.remove", ORIGIN, format!("remove MCP client {}", c.name)).params(json!({ "client": c.name })));
    Ok(format!("removed {}", c.name))
}

/// A client just added. `token` is shown once and is not stored in Dart.
pub struct McpClientAdded {
    pub name: String,
    pub role: String,
    pub token: String,
}

pub(crate) fn mcp_add_server_of(store: &Store, name: &str, url: &str, command: &str, token: &str) -> anyhow::Result<String> {
    let mut sec = muninn_core::store::Secrets::default();
    if !token.is_empty() {
        sec.headers.insert("Authorization".into(), format!("Bearer {}", token.trim()));
    }
    let s = if !url.trim().is_empty() {
        store.add_mcp_server(name.trim(), "http", "", &[], url.trim(), &sec)?
    } else {
        let mut parts = command.split_whitespace();
        let Some(cmd) = parts.next() else {
            anyhow::bail!("give a URL, or the server command");
        };
        let args: Vec<String> = parts.map(str::to_string).collect();
        store.add_mcp_server(name.trim(), "stdio", cmd, &args, "", &sec)?
    };
    store.note_action(
        &NewAction::new("mcp.server.add", ORIGIN, format!("add MCP server {} ({})", s.name, s.transport))
            .params(json!({ "server": s.id, "name": s.name, "transport": s.transport, "credentials": s.secrets.names() })),
    );
    Ok(format!("added MCP server {} ({})", s.name, s.transport))
}

/// `mnnn mcp add`. The bearer token is stored in Rust and does not come back.
pub fn mcp_add_server(name: String, url: String, command: String, token: String) -> anyhow::Result<String> {
    mcp_add_server_of(&open_write()?, &name, &url, &command, &token)
}

pub(crate) fn mcp_add_client_of(store: &Store, name: &str, role: &str) -> anyhow::Result<McpClientAdded> {
    let role = if role.trim().is_empty() { "read" } else { role.trim() };
    let (c, token) = store.add_mcp_client(name.trim(), McpRole::parse(role)?)?;
    store.note_action(
        &NewAction::new("mcp.client.add", ORIGIN, format!("add MCP client {} ({})", c.name, c.role.as_str()))
            .params(json!({ "client": c.name, "role": c.role.as_str() })),
    );
    Ok(McpClientAdded { name: c.name, role: c.role.as_str().into(), token })
}

/// `mnnn mcp client add`. The token is returned once; it is not logged.
pub fn mcp_add_client(name: String, role: String) -> anyhow::Result<McpClientAdded> {
    mcp_add_client_of(&open_write()?, &name, &role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::{Secrets, Store};

    #[test]
    fn servers_and_clients_list_without_secrets() {
        let s = Store::open_in_memory().unwrap();
        assert!(mcp_servers_of(&s).unwrap().is_empty());
        assert!(mcp_clients_of(&s).unwrap().is_empty());
        let mut sec = Secrets::default();
        sec.env.insert("TOKEN".into(), "s3cret".into());
        let srv = s.add_mcp_server("weather", "http", "", &[], "http://127.0.0.1/mcp", &sec).unwrap();
        let listed = mcp_servers_of(&s).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "weather");
        assert_eq!(listed[0].transport, "http");
        assert!(listed[0].credentials.iter().any(|c| c.contains("TOKEN")));
        let blob = format!("{:?}", listed);
        assert!(!blob.contains("s3cret"), "{blob}");
        let (c, token) = s.add_mcp_client("editor", McpRole::Read).unwrap();
        assert!(!token.is_empty());
        let clients = mcp_clients_of(&s).unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].name, "editor");
        assert_eq!(clients[0].role, "read");
        let blob = format!("{:?} {token}", clients);
        // the listing itself must not include the token
        let list_blob = format!("{:?}", clients);
        assert!(!list_blob.contains(&token), "{list_blob}");
        let _ = (srv, c, blob);
    }

    #[test]
    fn add_server_and_client_keep_the_token_out_of_the_listing() {
        let s = Store::open_in_memory().unwrap();
        let added = mcp_add_server_of(&s, "docs", "https://example.com/mcp", "", "tok").unwrap();
        assert!(added.contains("docs"));
        assert!(mcp_add_server_of(&s, "local", "", "", "").is_err());
        let client = mcp_add_client_of(&s, "editor", "read").unwrap();
        assert_eq!(client.role, "read");
        assert!(!client.token.is_empty());
        let listed = mcp_servers_of(&s).unwrap();
        assert!(listed[0].credentials.iter().any(|c| c.contains("Authorization")));
        assert!(!format!("{listed:?}").contains("tok"));
        assert!(!format!("{:?}", mcp_clients_of(&s).unwrap()).contains(&client.token));
    }
}
