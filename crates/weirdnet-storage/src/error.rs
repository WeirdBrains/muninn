//! Storage errors.

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The key did not open the file, or the file is not a database of this kind.
    #[error("wrong key or not a {what} database ({detail})")]
    WrongKey { what: String, detail: String },
    /// Conversion was asked for a file that is not plaintext SQLite.
    #[error("existing database is not a plaintext SQLite file")]
    NotPlaintext,
    /// A rekey left the file unencrypted (it was never encrypted).
    #[error("database is not encrypted; rekey cannot encrypt it (convert it first)")]
    NotEncrypted,
    /// The new key does not open the file afterwards (SQLCipher can report success without
    /// rekeying, for example on a read-only connection).
    #[error("rekey did not take effect; the database keeps its old key")]
    RekeyFailed,
    /// SQLite was built without SQLCipher: a key would be ignored.
    #[error("this SQLite has no encryption (SQLCipher); refusing to apply a key")]
    NoCipher,
    #[error("invalid {what}: {msg}")]
    Invalid { what: &'static str, msg: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[cfg(feature = "sync")]
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl StorageError {
    pub fn invalid(what: &'static str, msg: impl Into<String>) -> Self {
        StorageError::Invalid { what, msg: msg.into() }
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;
