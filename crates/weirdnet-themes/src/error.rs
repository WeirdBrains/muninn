//! The crate's error type.

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    /// A theme, name or role that does not check out (`what` names the thing).
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    TomlSer(#[from] toml::ser::Error),
}

impl ThemeError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        ThemeError::Invalid { what, msg: msg.into() }
    }
}

pub type Result<T> = std::result::Result<T, ThemeError>;
