//! The libp2p node is weirdnet-net's ([`weirdnet_net::node`]); Muninn runs it with its own
//! profile ([`MUNINN`]) and its own request and response types ([`Wire`], [`WireResp`]),
//! answered by [`super::service::Service`].
use super::profile::MUNINN;
use super::proto::{Wire, WireResp};
use crate::error::Result;
use libp2p::identity::Keypair;
use std::sync::Arc;

pub use weirdnet_net::addr::{addr_peer, parse_addrs, reach_rank, routable, shareable};
pub use weirdnet_net::{Handler, NodeConfig, Sighting, Status};

/// A running Muninn node.
pub type Node = weirdnet_net::Node<Wire, WireResp>;

/// The default port (TCP and QUIC).
pub const DEFAULT_PORT: u16 = MUNINN.default_port;

/// Start a node on Muninn's profile (agent `muninn/VERSION`); returns once its listeners
/// are up (or failed).
pub fn start(key: Keypair, mut cfg: NodeConfig, handler: Arc<dyn Handler<Wire, WireResp>>) -> Result<Node> {
    if cfg.version.is_empty() {
        cfg.version = env!("CARGO_PKG_VERSION").to_string();
    }
    Ok(Node::start(&MUNINN, key, cfg, handler)?)
}
