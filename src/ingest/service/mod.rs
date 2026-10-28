//! Service readers: http, redis, sqlite, postgres. Each produces `Input`s for the shared pipeline.
pub mod http;
pub mod postgres;
pub mod redis;
pub mod sqlite;

use super::{IngestOptions, IngestReport, Input, Session};
use crate::error::{Error, Result};
use crate::interact::Interact;
use crate::store::Store;
use std::io::Read;

/// Where a secret comes from.
#[derive(Debug, Clone, Default)]
pub struct Secret {
    pub literal: Option<String>,
    pub env: Option<String>,
    pub stdin: bool,
}

impl Secret {
    /// Resolve; prompts (hidden) only when `prompt` is set and the session is interactive.
    pub fn resolve(&self, io: &Interact, prompt: Option<&str>) -> Result<Option<String>> {
        if let Some(p) = &self.literal {
            return Ok(Some(p.clone()));
        }
        if let Some(var) = &self.env {
            return std::env::var(var)
                .map(Some)
                .map_err(|_| Error::missing("password", format!("environment variable {var} is not set")));
        }
        if self.stdin {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            return Ok(Some(s.trim_end_matches(['\n', '\r']).to_string()));
        }
        match prompt {
            Some(q) if io.interactive => Ok(Some(io.password(q, "")?)),
            _ => Ok(None),
        }
    }
}

/// Read `SQL` or `@FILE`.
pub fn sql_or_file(s: &str) -> Result<String> {
    match s.strip_prefix('@') {
        Some(p) => Ok(std::fs::read_to_string(p)?),
        None => Ok(s.to_string()),
    }
}

/// Quote an SQL identifier.
pub fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Run inputs through one session.
pub fn run_inputs(
    store: &mut Store,
    io: Interact,
    opts: IngestOptions,
    locator: &str,
    default_source: Option<String>,
    inputs: Vec<Input>,
) -> Result<Vec<IngestReport>> {
    let mut s = Session::new(store, io, opts, "service", locator);
    s.default_source = default_source;
    let mut out = vec![];
    for i in inputs {
        out.push(s.ingest(i)?);
    }
    Ok(out)
}
