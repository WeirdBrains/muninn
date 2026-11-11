//! This node's identity: an Ed25519 keypair made on first use and kept only inside the
//! encrypted database (so it is in backups and never written in plaintext). The logic is
//! weirdnet-net's ([`weirdnet_net::identity`]); the store keeps the bytes.
use crate::error::{Error, Result};
use crate::store::Store;
use libp2p::PeerId;
use libp2p::identity::Keypair;
use weirdnet_net::identity::{self as wid, IdentityStore};
use zeroize::Zeroizing;

pub use weirdnet_net::identity::{fingerprint, group};

impl IdentityStore for Store {
    type Error = Error;
    fn node_identity(&self) -> Result<Option<(Zeroizing<Vec<u8>>, String)>> {
        Store::node_identity(self)
    }
    fn init_node_identity(&self, keypair: &[u8], peer: &str) -> Result<()> {
        Store::init_node_identity(self, keypair, peer)
    }
}

/// The node's keypair, created (and stored) the first time it is needed.
pub fn load_or_create(store: &Store) -> Result<Keypair> {
    wid::load_or_create(store)
}

/// The node's peer id if it has an identity yet.
pub fn peer_id(store: &Store) -> Result<Option<PeerId>> {
    wid::peer_id(store)
}

/// The fingerprint of the stored identity, if the node has one. Does not create a key.
pub fn fingerprint_of(store: &Store) -> Result<Option<String>> {
    wid::fingerprint_of(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_made_once_and_kept() {
        let s = Store::open_in_memory().unwrap();
        assert!(peer_id(&s).unwrap().is_none());
        assert!(fingerprint_of(&s).unwrap().is_none());
        let a = load_or_create(&s).unwrap();
        let b = load_or_create(&s).unwrap();
        assert_eq!(a.public(), b.public());
        assert_eq!(peer_id(&s).unwrap().unwrap(), a.public().to_peer_id());
        let f = fingerprint(&a.public());
        assert_eq!(f.len(), 24);
        assert_eq!(f, fingerprint(&b.public()));
        assert_eq!(fingerprint_of(&s).unwrap().as_deref(), Some(f.as_str()));
    }
}
