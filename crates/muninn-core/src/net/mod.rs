//! The decentralized network (2.6): nodes, key exchanges, sharing, searches across
//! peers and dark shards. The node, exchanges, invitations, sealed messaging, identities
//! and certificates are weirdnet-net's, run with Muninn's legacy profile ([`profile`]);
//! what peers may ask for and reach, orgs, reviews, collaboration, sharing, sync data and
//! history stay here. See `docs/network.md`, `docs/key-exchanges.md` and
//! `docs/dark-shards.md`.
pub mod auto;
pub mod backup;
pub mod broadcast;
pub mod client;
pub mod exchange;
pub mod identity;
pub mod local;
pub mod node;
pub mod profile;
pub mod proto;
pub mod service;
pub mod sync;
pub mod user;

pub use node::{Handler, Node, NodeConfig, Sighting, Status};

/// The last 8 characters of a peer id.
pub fn short_peer_id(p: &libp2p::PeerId) -> String {
    weirdnet_net::short_peer_id(p)
}

#[cfg(test)]
mod net_golden_tests;
#[cfg(test)]
mod tests;
