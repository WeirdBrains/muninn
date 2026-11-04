//! Library error type.
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("missing {what}: {hint}")]
    Missing { what: &'static str, hint: String },
    #[error("confirmation required: {0} (re-run with -y/--yes to confirm)")]
    NeedsConfirmation(String),
    #[error("aborted by user")]
    Aborted,
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
    #[error("muninn keys are not initialized in {0}; run `mnnn keys init` first")]
    NotInitialized(PathBuf),
    #[error("could not unlock the database: {0}")]
    Unlock(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Polars(#[from] polars::error::PolarsError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("toml: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        Error::Invalid { what, msg: msg.into() }
    }
    pub fn missing(what: &'static str, hint: impl Into<String>) -> Self {
        Error::Missing { what, hint: hint.into() }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
