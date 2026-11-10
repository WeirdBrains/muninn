//! Key errors. None of them carries key material, a password or plaintext.
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum KeysError {
    #[error("crypto error: {0}")]
    Crypto(String),
    /// Wrong password or key, or tampered data (indistinguishable on purpose).
    #[error("could not unlock: {0}")]
    Unlock(String),
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
    /// No envelope in this home.
    #[error("keys are not initialized in {0}")]
    NotInitialized(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl KeysError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        KeysError::Invalid { what, msg: msg.into() }
    }
    pub(crate) fn crypto(msg: impl Into<String>) -> Self {
        KeysError::Crypto(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, KeysError>;
