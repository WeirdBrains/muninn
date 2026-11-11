//! User identities: a person's Ed25519 signing key, separate from node keys. The user
//! certifies each node it links (a signature over the node's peer id) and can revoke it
//! later; peers check these before treating a node as acting for a user. A [`Card`] is what
//! a node says about its user: the public key and the certificates, never a secret.
//!
//! Which users are trusted, what a connection may do and where certificates are kept are
//! the application's decisions; this module only makes and checks the signatures.
use crate::error::{NetError, Result};
use crate::profile::Profile;
use libp2p::identity::{Keypair, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// A user's certificate for one node, with its revocation when there is one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeCert {
    pub user_id: String,
    pub peer_id: String,
    pub label: String,
    pub cert: Vec<u8>,
    pub issued_at: i64,
    pub revoked_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revocation: Option<Vec<u8>>,
}

impl NodeCert {
    /// Not revoked.
    pub fn active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// What a node says about its user over an exchange: the user's public key (protobuf) and
/// the certificates (and revocations) of the user's nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub label: String,
    pub key: Vec<u8>,
    pub nodes: Vec<NodeCert>,
}

impl Card {
    /// A short fingerprint of what the card says (to re-introduce only after a change).
    pub fn digest(&self) -> String {
        let mut h = sha2::Sha256::new();
        h.update(self.id.as_bytes());
        h.update(self.label.as_bytes());
        for n in &self.nodes {
            h.update(n.peer_id.as_bytes());
            h.update(n.issued_at.to_le_bytes());
            h.update(n.revoked_at.unwrap_or(0).to_le_bytes());
        }
        hex::encode(&h.finalize()[..8])
    }
}

/// The user id of a public key: 32 hex characters of its SHA-256.
pub fn user_id(key: &PublicKey) -> String {
    hex::encode(&sha2::Sha256::digest(key.encode_protobuf())[..16])
}

/// A user keypair from its protobuf encoding.
pub fn decode_keypair(bytes: &[u8]) -> Result<Keypair> {
    Keypair::from_protobuf_encoding(bytes).map_err(|e| NetError::Crypto(format!("user identity: {e}")))
}

/// A user public key from its protobuf encoding.
pub fn decode_public(bytes: &[u8]) -> Result<PublicKey> {
    PublicKey::try_decode_protobuf(bytes).map_err(|e| NetError::Crypto(format!("user key: {e}")))
}

impl Profile {
    pub fn cert_bytes(&self, user: &str, peer: &str, at: i64) -> Vec<u8> {
        format!("{}\n{user}\n{peer}\n{at}", self.cert).into_bytes()
    }

    pub fn revoke_bytes(&self, user: &str, peer: &str, at: i64) -> Vec<u8> {
        format!("{}\n{user}\n{peer}\n{at}", self.revoke).into_bytes()
    }

    /// The user's certificate for a node, issued at `at` (ms).
    pub fn certify(&self, kp: &Keypair, user: &str, peer: &str, label: &str, at: i64) -> Result<NodeCert> {
        let cert = kp.sign(&self.cert_bytes(user, peer, at)).map_err(|e| NetError::Crypto(format!("certify: {e}")))?;
        Ok(NodeCert { user_id: user.into(), peer_id: peer.into(), label: label.into(), cert, issued_at: at, revoked_at: None, revocation: None })
    }

    /// The user's signed revocation of a node at `at` (ms).
    pub fn revoke(&self, kp: &Keypair, user: &str, peer: &str, at: i64) -> Result<Vec<u8>> {
        kp.sign(&self.revoke_bytes(user, peer, at)).map_err(|e| NetError::Crypto(format!("revoke: {e}")))
    }

    /// Check a certificate (and its revocation, if any) against the user's key. A
    /// revocation time without its signature (or the other way round) fails.
    pub fn verify_cert(&self, key: &PublicKey, n: &NodeCert) -> bool {
        if user_id(key) != n.user_id || !key.verify(&self.cert_bytes(&n.user_id, &n.peer_id, n.issued_at), &n.cert) {
            return false;
        }
        match (n.revoked_at, &n.revocation) {
            (None, None) => true,
            (Some(at), Some(sig)) => key.verify(&self.revoke_bytes(&n.user_id, &n.peer_id, at), sig),
            _ => false,
        }
    }

    /// Check a card a peer sent: the id must be its key's, and it must carry a valid,
    /// unrevoked certificate for that very peer. Returns the user's key; which of the
    /// card's certificates to keep (those that verify) is up to the caller.
    pub fn check_card(&self, from_peer: &str, card: &Card) -> Result<PublicKey> {
        let key = decode_public(&card.key)?;
        if user_id(&key) != card.id {
            return Err(NetError::Crypto("user card: the id does not match the key".into()));
        }
        if !card.nodes.iter().any(|n| n.peer_id == from_peer && n.user_id == card.id && n.active() && self.verify_cert(&key, n)) {
            return Err(NetError::Crypto("user card: no valid certificate for the sending node".into()));
        }
        Ok(key)
    }

    /// On a node being linked: check a grant (the user's card and signing key) from the
    /// approving node. The key must be the card's, the card must certify this node (`me`)
    /// and the approving node, neither revoked. Returns the user keypair.
    pub fn check_grant(&self, me: &str, from_peer: &str, card: &Card, keypair: &[u8]) -> Result<Keypair> {
        let kp = decode_keypair(keypair)?;
        if user_id(&kp.public()) != card.id || kp.public().encode_protobuf() != card.key {
            return Err(NetError::Crypto("link: the key does not match the user".into()));
        }
        let key = kp.public();
        if !card.nodes.iter().any(|n| n.peer_id == me && n.active() && self.verify_cert(&key, n)) {
            return Err(NetError::Crypto("link: no certificate for this node".into()));
        }
        if !card.nodes.iter().any(|n| n.peer_id == from_peer && n.active() && self.verify_cert(&key, n)) {
            return Err(NetError::Crypto("link: the approving node is not the user's".into()));
        }
        Ok(kp)
    }
}
