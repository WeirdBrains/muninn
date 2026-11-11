#![doc = include_str!("../README.md")]
pub mod addr;
mod error;
pub mod exchange;
pub mod identity;
pub mod node;
pub mod profile;
pub mod sealed;
pub mod shard;
pub mod sync;
pub mod user;

pub use error::{NetError, Result};
pub use exchange::{Answer, DEFAULT_TTL_MS, Derived, Offer, Token, normalize_code};
pub use libp2p::identity::{Keypair, PublicKey};
pub use libp2p::{Multiaddr, PeerId};
pub use node::{Handler, Message, Node, NodeConfig, Sighting, Status};
pub use profile::{Profile, WEIRDNET};
pub use weirdnet_keys::{Key32, Sealed};

/// The last 8 characters of a peer id.
pub fn short_peer_id(p: &PeerId) -> String {
    identity::short_peer(&p.to_string()).to_string()
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests;
