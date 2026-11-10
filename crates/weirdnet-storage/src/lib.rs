//! General storage for SQLite databases encrypted with SQLCipher.
//!
//! Schema-free and brand-free: callers supply their migrations, name their database in
//! error messages and register the tables they sync.
//!
//! Key boundary: the caller owns every key. This crate only applies a raw 32-byte key it is
//! handed (zeroizing the literals it builds); it never derives, wraps, stores, recovers or
//! exports one, and never sees a password.
#![forbid(unsafe_code)]

mod db;
mod error;
pub mod files;
#[cfg(feature = "sync")]
pub mod sync;
mod tx;

pub use db::{
    RawKey, apply_key, encrypt_plaintext_in_place, has_cipher, migrate, open, open_foreign_readonly, open_in_memory, open_readonly, rekey,
    schema_version, snapshot_to,
};
pub use error::{Result, StorageError};
pub use rusqlite;
pub use tx::{begin, release, rollback, savepoint};

/// Quote an SQL identifier (`"name"`, inner quotes doubled).
pub fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Whether a table exists.
pub fn table_exists(conn: &rusqlite::Connection, name: &str) -> Result<bool> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |r| r.get::<_, i64>(0))? > 0)
}
