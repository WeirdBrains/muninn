//! MCP (2.20): Muninn as a Model Context Protocol server (`mnnn mcp serve`, stdio or HTTP),
//! and external MCP servers you add (`mnnn mcp add`) for rule sets to call. JSON-RPC 2.0;
//! see `docs/mcp.md`.
//!
//! Access: every call comes from a named, revocable [`McpClient`](crate::store::McpClient)
//! with a role (read only by default). Writes and runs need the role, are recorded in the
//! history with origin `mcp`, and refused calls are recorded as `mcp.denied`.
pub mod client;
pub mod serve;
pub mod server;

use serde_json::{Value, json};

/// The protocol revision this Muninn speaks (older ones a client asks for are accepted).
pub const PROTOCOL: &str = "2025-06-18";
pub const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
/// Origin of actions taken over MCP.
pub const ORIGIN: &str = "mcp";
/// Where `mnnn mcp serve --http` listens unless told otherwise (local only).
pub const DEFAULT_HTTP: &str = "127.0.0.1:4743";

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
/// Not allowed: an unknown or revoked client.
pub const UNAUTHORIZED: i64 = -32001;

pub fn ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn err(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

/// The text of a tool result's content (text parts joined), for listings and rules.
pub fn result_text(result: &Value) -> String {
    result["content"].as_array().map(|a| a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default()
}

/// A tool result as data: its structured content, else its text parsed as JSON, else the text.
pub fn result_value(result: &Value) -> Value {
    if let Some(v) = result.get("structuredContent").filter(|v| !v.is_null()) {
        return v.clone();
    }
    let t = result_text(result);
    serde_json::from_str(&t).unwrap_or(Value::String(t))
}
