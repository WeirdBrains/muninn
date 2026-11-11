//! The crate's error type.

/// What can go wrong checking, building, loading or wiring code spaces. Messages are
/// complete sentences an application can show as they are.
#[derive(Debug, thiserror::Error)]
pub enum SpacesError {
    /// A source, plugin or line-up is wrong (`what` names the part: `math`, `plugin`, `space`, …).
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
    /// Something needed is not there (`what`: "Rust toolchain", …), with what to do about it.
    #[error("missing {what}: {hint}")]
    Missing { what: &'static str, hint: String },
    /// Cargo failed; the message has the end of its output.
    #[error("{0}")]
    Build(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl SpacesError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        SpacesError::Invalid { what, msg: msg.into() }
    }
    pub fn missing(what: &'static str, hint: impl Into<String>) -> Self {
        SpacesError::Missing { what, hint: hint.into() }
    }
}

pub(crate) type Error = SpacesError;
pub type Result<T> = std::result::Result<T, SpacesError>;
