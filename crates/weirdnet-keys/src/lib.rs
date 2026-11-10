//! Key material and key policy: keys, words, shards, derivation, sealing, agreement, MACs,
//! the envelope keyring, dark shards and password exports.
//!
//! Every operation that encrypts, decrypts, derives, wraps, rotates, recovers, backs up or
//! exports key material lives here, on vetted crates only. Labels come from a [`Profile`];
//! files and the database come in through traits the application implements.
#![forbid(unsafe_code)]

pub mod aead;
#[cfg(feature = "age")]
pub mod agefile;
pub mod dark;
mod error;
pub mod exchange;
pub mod kdf;
mod key;
pub mod keyring;
pub mod mac;
pub mod password;
mod profile;
pub mod shards;
pub mod words;

pub use aead::{Sealed, open, seal};
pub use error::{KeysError, Result};
pub use key::{Key32, random_bytes};
pub use password::{PasswordSealed, password_open, password_seal};
pub use profile::Profile;
pub use secrecy::{self, ExposeSecret, SecretString};
pub use zeroize::{self, Zeroizing};
