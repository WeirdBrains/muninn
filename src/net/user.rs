//! User identities (2.10): a person's Ed25519 signing key, separate from node keys. The
//! user certifies each node it links (a signature over the node's peer id) and can revoke
//! it later; peers check these signatures before treating a node as acting for a user.
//! The signing key never leaves the user's own nodes. See `docs/users.md`.
use crate::error::{Error, Result};
use crate::store::Store;
use crate::store::users::{User, UserNode};
use libp2p::identity::{Keypair, PublicKey};
use sha2::Digest;

const CERT: &str = "muninn-node-cert/1";
const REVOKE: &str = "muninn-node-revoke/1";

/// The user id of a public key: 32 hex characters of its SHA-256.
pub fn user_id(key: &PublicKey) -> String {
    hex::encode(&sha2::Sha256::digest(key.encode_protobuf())[..16])
}

fn decode(bytes: &[u8]) -> Result<Keypair> {
    Keypair::from_protobuf_encoding(bytes).map_err(|e| Error::Crypto(format!("user identity: {e}")))
}

/// A user's public key from its stored bytes.
pub fn public_key(u: &User) -> Result<PublicKey> {
    PublicKey::try_decode_protobuf(&u.public_key).map_err(|e| Error::Crypto(format!("user key: {e}")))
}

/// This node's own user and its signing key, made on first use (labelled after the
/// current author) with this node certified as its first linked node.
pub fn load_or_create(store: &Store) -> Result<(User, Keypair)> {
    if let (Some(u), Some(kp)) = (store.self_user()?, store.self_user_keypair()?) {
        return Ok((u, decode(&kp)?));
    }
    let kp = Keypair::generate_ed25519();
    let bytes = zeroize::Zeroizing::new(kp.to_protobuf_encoding().map_err(|e| Error::Crypto(format!("user identity: {e}")))?);
    let author = store.current_author()?;
    let label = author.as_ref().map(|a| a.name.clone()).unwrap_or_default();
    let id = user_id(&kp.public());
    store.init_self_user(&id, &label, &kp.public().encode_protobuf(), &bytes, author.as_ref().map(|a| a.id.as_str()))?;
    let (u, kp) = match (store.self_user()?, store.self_user_keypair()?) {
        (Some(u), Some(k)) => (u, decode(&k)?),
        _ => return Err(Error::Crypto("user identity: not stored".into())),
    };
    let node = super::identity::load_or_create(store)?;
    let peer = node.public().to_peer_id().to_string();
    if store.user_of_node(&peer)?.is_none() {
        store.put_user_node(&certify(&kp, &u.id, &peer, "this node")?)?;
    }
    Ok((u, kp))
}

fn cert_bytes(user: &str, peer: &str, at: i64) -> Vec<u8> {
    format!("{CERT}\n{user}\n{peer}\n{at}").into_bytes()
}

fn revoke_bytes(user: &str, peer: &str, at: i64) -> Vec<u8> {
    format!("{REVOKE}\n{user}\n{peer}\n{at}").into_bytes()
}

/// The user's certificate for a node.
pub fn certify(kp: &Keypair, user: &str, peer: &str, label: &str) -> Result<UserNode> {
    let at = crate::store::now_ms();
    let cert = kp.sign(&cert_bytes(user, peer, at)).map_err(|e| Error::Crypto(format!("certify: {e}")))?;
    Ok(UserNode { user_id: user.into(), peer_id: peer.into(), label: label.into(), cert, issued_at: at, revoked_at: None, revocation: None })
}

/// The user's signed revocation of a node.
pub fn revoke(kp: &Keypair, user: &str, peer: &str) -> Result<(i64, Vec<u8>)> {
    let at = crate::store::now_ms();
    Ok((at, kp.sign(&revoke_bytes(user, peer, at)).map_err(|e| Error::Crypto(format!("revoke: {e}")))?))
}

/// Check a certificate (and its revocation, if any) against the user's key.
pub fn verify(key: &PublicKey, n: &UserNode) -> bool {
    if user_id(key) != n.user_id || !key.verify(&cert_bytes(&n.user_id, &n.peer_id, n.issued_at), &n.cert) {
        return false;
    }
    match (n.revoked_at, &n.revocation) {
        (None, None) => true,
        (Some(at), Some(sig)) => key.verify(&revoke_bytes(&n.user_id, &n.peer_id, at), sig),
        _ => false,
    }
}

/// Store a certificate after checking it; also remembers the user.
pub fn accept_cert(store: &Store, label: &str, key: &PublicKey, n: &UserNode) -> Result<()> {
    if !verify(key, n) {
        return Err(Error::Crypto(format!("node certificate for {} does not verify", crate::store::net::short_peer(&n.peer_id))));
    }
    store.upsert_user(&n.user_id, label, &key.encode_protobuf())?;
    store.put_user_node(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_certifies_and_revokes_nodes() {
        let s = Store::open_in_memory().unwrap();
        let (u, kp) = load_or_create(&s).unwrap();
        let (u2, _) = load_or_create(&s).unwrap();
        assert_eq!(u.id, u2.id);
        assert!(u.mine);
        let me = super::super::identity::peer_id(&s).unwrap().unwrap().to_string();
        let n = s.user_of_node(&me).unwrap().unwrap();
        assert!(verify(&kp.public(), &n));

        let other = Store::open_in_memory().unwrap();
        let c = certify(&kp, &u.id, "PEER2", "phone").unwrap();
        accept_cert(&other, "me", &kp.public(), &c).unwrap();
        assert_eq!(other.user_of_node("PEER2").unwrap().unwrap().user_id, u.id);
        assert!(!other.user(&u.id).unwrap().unwrap().mine);

        // a forged or tampered certificate is refused
        let mallory = Keypair::generate_ed25519();
        let forged = certify(&mallory, &u.id, "PEER3", "").unwrap();
        assert!(accept_cert(&other, "me", &kp.public(), &forged).is_err());
        let mut tampered = c.clone();
        tampered.peer_id = "PEER4".into();
        assert!(!verify(&kp.public(), &tampered));

        let (at, sig) = revoke(&kp, &u.id, "PEER2").unwrap();
        let mut r = c.clone();
        r.revoked_at = Some(at);
        r.revocation = Some(sig);
        assert!(verify(&kp.public(), &r));
        r.revoked_at = Some(at + 1);
        assert!(!verify(&kp.public(), &r));
    }
}
