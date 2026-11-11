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

/// Code-space errors keep Muninn's messages and kinds (invalid sources and plugins, a missing
/// toolchain, build failures as they were).
impl From<weirdnet_code_spaces::SpacesError> for Error {
    fn from(e: weirdnet_code_spaces::SpacesError) -> Self {
        use weirdnet_code_spaces::SpacesError as S;
        match e {
            S::Invalid { what, msg } => Error::Invalid { what, msg },
            S::Missing { what, hint } => Error::Missing { what, hint },
            S::Build(m) => Error::Other(m),
            S::Io(e) => Error::Io(e),
        }
    }
}

/// Storage errors keep Muninn's messages and kinds ("could not unlock the database: wrong key
/// or not a muninn database (…)", SQLite and I/O errors as they were).
impl From<weirdnet_storage::StorageError> for Error {
    fn from(e: weirdnet_storage::StorageError) -> Self {
        use weirdnet_storage::StorageError as S;
        match e {
            e @ (S::WrongKey { .. } | S::NotPlaintext | S::NotEncrypted | S::RekeyFailed | S::NoCipher) => Error::Unlock(e.to_string()),
            S::Invalid { what, msg } => Error::Invalid { what, msg },
            S::Io(e) => Error::Io(e),
            S::Sqlite(e) => Error::Sqlite(e),
            S::Json(e) => Error::Json(e),
        }
    }
}

/// Theme errors keep their messages and kinds.
impl From<weirdnet_themes::ThemeError> for Error {
    fn from(e: weirdnet_themes::ThemeError) -> Self {
        match e {
            weirdnet_themes::ThemeError::Invalid { what, msg } => Error::Invalid { what, msg },
            weirdnet_themes::ThemeError::Io(e) => Error::Io(e),
            weirdnet_themes::ThemeError::TomlSer(e) => Error::TomlSer(e),
        }
    }
}

/// Key errors keep Muninn's messages and kinds ("could not unlock the database: wrong
/// password", "muninn keys are not initialized in …", and so on).
impl From<weirdnet_keys::KeysError> for Error {
    fn from(e: weirdnet_keys::KeysError) -> Self {
        use weirdnet_keys::KeysError as K;
        match e {
            K::Crypto(m) => Error::Crypto(m),
            K::Unlock(m) => Error::Unlock(m),
            K::Invalid { what, msg } => Error::Invalid { what, msg },
            K::NotInitialized(p) => Error::NotInitialized(p),
            K::Io(e) => Error::Io(e),
            K::TomlDe(e) => Error::TomlDe(e),
            K::TomlSer(e) => Error::TomlSer(e),
            K::Json(e) => Error::Json(e),
        }
    }
}

/// Pipeline graph and frame errors keep their messages ("invalid pipeline: …", "invalid input: …").
impl From<weirdnet_pipelines::PipelineError> for Error {
    fn from(e: weirdnet_pipelines::PipelineError) -> Self {
        match e {
            weirdnet_pipelines::PipelineError::Invalid { what, msg } => Error::Invalid { what, msg },
        }
    }
}

/// Rule syntax errors keep their messages ("invalid --if: …", "invalid --cron: …").
impl From<weirdnet_rules::RulesError> for Error {
    fn from(e: weirdnet_rules::RulesError) -> Self {
        match e {
            weirdnet_rules::RulesError::Invalid { what, msg } => Error::Invalid { what, msg },
        }
    }
}
