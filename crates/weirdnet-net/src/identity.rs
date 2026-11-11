//! A node's identity: an Ed25519 keypair made on first use. The application keeps it
//! wherever it keeps secrets (an encrypted database, say) through [`IdentityStore`]; the
//! crate never writes it anywhere itself.
use crate::error::NetError;
use libp2p::PeerId;
use libp2p::identity::{Keypair, PublicKey};
use sha2::Digest;
use zeroize::Zeroizing;

/// A stored identity: the protobuf-encoded keypair and its peer id.
pub type Stored = (Zeroizing<Vec<u8>>, String);

/// Where the application keeps the node identity.
pub trait IdentityStore {
    type Error: From<NetError>;
    fn node_identity(&self) -> Result<Option<Stored>, Self::Error>;
    /// Store a new identity unless one is there already (whoever stored first wins).
    fn init_node_identity(&self, keypair: &[u8], peer: &str) -> Result<(), Self::Error>;
}

fn decode(bytes: &[u8]) -> Result<Keypair, NetError> {
    Keypair::from_protobuf_encoding(bytes).map_err(|e| NetError::Crypto(format!("node identity: {e}")))
}

/// The node's keypair, created (and stored) the first time it is needed.
pub fn load_or_create<S: IdentityStore>(store: &S) -> Result<Keypair, S::Error> {
    if let Some((bytes, _)) = store.node_identity()? {
        return Ok(decode(&bytes)?);
    }
    let kp = Keypair::generate_ed25519();
    let bytes = Zeroizing::new(kp.to_protobuf_encoding().map_err(|e| NetError::Crypto(format!("node identity: {e}")))?);
    store.init_node_identity(&bytes, &kp.public().to_peer_id().to_string())?;
    let (bytes, _) = store.node_identity()?.ok_or_else(|| NetError::Crypto("node identity: not stored".into()))?;
    Ok(decode(&bytes)?)
}

/// The node's peer id if it has an identity yet.
pub fn peer_id<S: IdentityStore>(store: &S) -> Result<Option<PeerId>, S::Error> {
    Ok(match store.node_identity()? {
        Some((_, p)) => Some(p.parse().map_err(|_| NetError::Crypto("node identity: bad peer id".into()))?),
        None => None,
    })
}

/// The fingerprint of the stored identity, if there is one. Does not create a key.
pub fn fingerprint_of<S: IdentityStore>(store: &S) -> Result<Option<String>, S::Error> {
    let Some((bytes, _)) = store.node_identity()? else {
        return Ok(None);
    };
    Ok(Some(fingerprint(&decode(&bytes)?.public())))
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

/// The last 8 characters of a peer id.
pub fn short_peer(p: &str) -> &str {
    &p[p.len().saturating_sub(8)..]
}
