//! This node's identity: an Ed25519 keypair made on first use and kept only inside the
//! encrypted database (so it is in backups and never written in plaintext).
use crate::error::{Error, Result};
use crate::store::Store;
use libp2p::PeerId;
use libp2p::identity::{Keypair, PublicKey};
use sha2::Digest;

/// The node's keypair, created (and stored) the first time it is needed.
pub fn load_or_create(store: &Store) -> Result<Keypair> {
    if let Some((bytes, _)) = store.node_identity()? {
        return Keypair::from_protobuf_encoding(&bytes).map_err(|e| Error::Crypto(format!("node identity: {e}")));
    }
    let kp = Keypair::generate_ed25519();
    let bytes = zeroize::Zeroizing::new(kp.to_protobuf_encoding().map_err(|e| Error::Crypto(format!("node identity: {e}")))?);
    store.init_node_identity(&bytes, &kp.public().to_peer_id().to_string())?;
    // whoever stored first wins
    let (bytes, _) = store.node_identity()?.ok_or_else(|| Error::Crypto("node identity: not stored".into()))?;
    Keypair::from_protobuf_encoding(&bytes).map_err(|e| Error::Crypto(format!("node identity: {e}")))
}

/// The node's peer id if it has an identity yet.
pub fn peer_id(store: &Store) -> Result<Option<PeerId>> {
    Ok(match store.node_identity()? {
        Some((_, p)) => Some(p.parse().map_err(|_| Error::Crypto("node identity: bad peer id".into()))?),
        None => None,
    })
}

/// A short, readable fingerprint of a public key: `3f2a 9c1d 77e0 41b8 aa02`.
pub fn fingerprint(key: &PublicKey) -> String {
    let h = sha2::Sha256::digest(key.encode_protobuf());
    group(&hex::encode(&h[..10]))
}

/// `abcd1234` → `abcd 1234`.
pub fn group(hex: &str) -> String {
    hex.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).to_string()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_made_once_and_kept() {
        let s = Store::open_in_memory().unwrap();
        assert!(peer_id(&s).unwrap().is_none());
        let a = load_or_create(&s).unwrap();
        let b = load_or_create(&s).unwrap();
        assert_eq!(a.public(), b.public());
        assert_eq!(peer_id(&s).unwrap().unwrap(), a.public().to_peer_id());
        let f = fingerprint(&a.public());
        assert_eq!(f.len(), 24);
        assert_eq!(f, fingerprint(&b.public()));
    }
}
