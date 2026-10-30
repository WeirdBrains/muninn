//! The decentralized network (2.6): nodes, key exchanges, sharing, searches across
//! peers and dark shards. See `docs/network.md`, `docs/key-exchanges.md` and
//! `docs/dark-shards.md`.
pub mod backup;
pub mod client;
pub mod exchange;
pub mod identity;
pub mod local;
pub mod node;
pub mod proto;
pub mod service;

pub use node::{Handler, Node, NodeConfig, Sighting, Status};

/// The last 8 characters of a peer id.
pub fn short_peer_id(p: &libp2p::PeerId) -> String {
    crate::store::net::short_peer(&p.to_string()).to_string()
}

#[cfg(test)]
mod tests;
