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
    // existing orgs move to the new user
    store.claim_orgs(&u.id, &peer)?;
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

// ---------- cards: how nodes tell each other whom they act for ----------

/// What a node says about its user over an exchange: the user's public key and the
/// certificates (and revocations) of the user's nodes. Holds no secret.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Card {
    pub id: String,
    pub label: String,
    pub key: Vec<u8>,
    pub nodes: Vec<UserNode>,
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

/// This node's card (making its user on first use).
pub fn my_card(store: &Store) -> Result<Card> {
    let (u, _) = load_or_create(store)?;
    Ok(Card { id: u.id.clone(), label: u.label.clone(), key: u.public_key.clone(), nodes: store.user_nodes(&u.id)? })
}

/// What a card meant here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Met {
    /// Another user; `new` when the connection was just made.
    Connection { user: String, new: bool },
    /// One of my own nodes.
    Mine,
}

/// Take a card a peer sent over an exchange: it must carry a valid, unrevoked
/// certificate for that very peer. Every certificate that verifies is kept (revocations
/// too); another user becomes an (untrusted) connection.
pub fn take_card(store: &Store, from_peer: &str, card: &Card, origin: &str) -> Result<Met> {
    let key = PublicKey::try_decode_protobuf(&card.key).map_err(|e| Error::Crypto(format!("user key: {e}")))?;
    if user_id(&key) != card.id {
        return Err(Error::Crypto("user card: the id does not match the key".into()));
    }
    if !card.nodes.iter().any(|n| n.peer_id == from_peer && n.user_id == card.id && n.active() && verify(&key, n)) {
        return Err(Error::Crypto("user card: no valid certificate for the sending node".into()));
    }
    // a node revoked here stays revoked, whatever it says now
    if store.user_nodes(&card.id)?.iter().any(|n| n.peer_id == from_peer && !n.active()) {
        return Err(Error::Crypto("user card: the sending node was revoked by its user".into()));
    }
    let mine = store.self_user()?.is_some_and(|u| u.id == card.id);
    if !mine {
        store.upsert_user(&card.id, &card.label, &card.key)?;
    }
    for n in card.nodes.iter().filter(|n| n.user_id == card.id && verify(&key, n)) {
        store.put_user_node(n)?;
    }
    if mine {
        store.tag_members(from_peer, &card.id)?;
        return Ok(Met::Mine);
    }
    store.tag_members(from_peer, &card.id)?;
    let (_, new) = store.add_connection(&card.id)?;
    if new {
        let name = if card.label.is_empty() { card.id[..8].to_string() } else { card.label.clone() };
        store.note_action(
            &crate::store::NewAction::new("connection.add", origin, format!("connected with {name} (untrusted)"))
                .params(serde_json::json!({ "user": card.id, "peer": from_peer, "trusted": false })),
        );
    }
    Ok(Met::Connection { user: card.id.clone(), new })
}

/// The user a peer acts for, when its certificate is known and not revoked.
pub fn user_of_peer(store: &Store, peer: &str) -> Result<Option<User>> {
    match store.user_of_node(peer)? {
        Some(n) => store.user(&n.user_id),
        None => Ok(None),
    }
}

/// Mark a connection trusted or untrusted, recorded in history. Returns whether it changed.
pub fn set_trust(store: &Store, key: &str, trusted: bool, origin: &str) -> Result<(User, bool)> {
    let u = store.find_user(key)?;
    if u.mine {
        return Err(Error::invalid("connection", "that is you"));
    }
    store.add_connection(&u.id)?;
    let changed = store.set_connection_trust(&u.id, trusted)?;
    if changed {
        store.note_action(
            &crate::store::NewAction::new(
                "connection.trust",
                origin,
                format!("{} is now {}", u.name(), if trusted { "trusted" } else { "untrusted" }),
            )
            .params(serde_json::json!({ "user": u.id, "trusted": trusted })),
        );
    }
    Ok((u, changed))
}

/// Forget a connection (the user and their certificates stay known), recorded in history.
pub fn remove_connection(store: &Store, key: &str, origin: &str) -> Result<User> {
    let u = store.find_user(key)?;
    if !store.remove_connection(&u.id)? {
        return Err(Error::invalid("connection", format!("no connection with {}", u.name())));
    }
    store.note_action(
        &crate::store::NewAction::new("connection.remove", origin, format!("connection with {} removed", u.name()))
            .params(serde_json::json!({ "user": u.id })),
    );
    Ok(u)
}

/// Is this peer's user a trusted connection (or one of my own nodes)?
pub fn trusted_peer(store: &Store, peer: &str) -> Result<bool> {
    let Some(u) = user_of_peer(store, peer)? else { return Ok(false) };
    if u.mine {
        return Ok(true);
    }
    Ok(store.connection(&u.id)?.is_some_and(|c| c.trusted))
}

/// Revoke one of my nodes (by peer id, its last characters, or its label): sign the
/// revocation with my user key and keep it. Peers learn it with my next card.
pub fn revoke_node(store: &Store, key: &str, origin: &str) -> Result<UserNode> {
    let (u, kp) = load_or_create(store)?;
    let me = super::identity::peer_id(store)?.map(|p| p.to_string());
    let k = key.trim();
    let nodes = store.user_nodes(&u.id)?;
    let n = nodes
        .iter()
        .find(|n| n.peer_id == k || (k.len() >= 6 && n.peer_id.ends_with(k)) || (!n.label.is_empty() && n.label.eq_ignore_ascii_case(k)))
        .cloned()
        .ok_or_else(|| Error::invalid("node", format!("no linked node `{k}` (see `mnnn user nodes`)")))?;
    if me.as_deref() == Some(n.peer_id.as_str()) {
        return Err(Error::invalid("node", "this is the node you are on; revoke it from another of your nodes"));
    }
    if !n.active() {
        return Err(Error::invalid("node", "that node is already revoked"));
    }
    let (at, sig) = revoke(&kp, &u.id, &n.peer_id)?;
    store.revoke_user_node(&u.id, &n.peer_id, at, &sig)?;
    store.note_action(
        &crate::store::NewAction::new(
            "user.node.revoke",
            origin,
            format!("node {} revoked", if n.label.is_empty() { crate::store::net::short_peer(&n.peer_id) } else { &n.label }),
        )
        .params(serde_json::json!({ "user": u.id, "peer": n.peer_id })),
    );
    Ok(store.user_nodes(&u.id)?.into_iter().find(|x| x.peer_id == n.peer_id).unwrap_or(n))
}

/// What the approving node hands a node it links: its card (with a fresh certificate
/// for the new node) and the user's signing key, sealed under the link exchange.
pub fn link_grant(store: &Store, peer: &str, label: &str, origin: &str) -> Result<(Card, zeroize::Zeroizing<Vec<u8>>)> {
    let (u, kp) = load_or_create(store)?;
    if store.user_nodes(&u.id)?.iter().any(|n| n.peer_id == peer && !n.active()) {
        return Err(Error::invalid("node", "that node was revoked; it cannot be linked again"));
    }
    if store.user_of_node(peer)?.is_none_or(|n| n.user_id != u.id) {
        store.put_user_node(&certify(&kp, &u.id, peer, label)?)?;
        store.note_action(
            &crate::store::NewAction::new(
                "user.node.link",
                origin,
                format!("node {} linked", if label.is_empty() { crate::store::net::short_peer(peer) } else { label }),
            )
            .params(serde_json::json!({ "user": u.id, "peer": peer })),
        );
    }
    let bytes = zeroize::Zeroizing::new(kp.to_protobuf_encoding().map_err(|e| Error::Crypto(format!("user identity: {e}")))?);
    Ok((my_card(store)?, bytes))
}

/// Become the user of a grant (on the node being linked). The node's earlier user is
/// kept as a known user without its key.
pub fn adopt(store: &Store, from_peer: &str, card: &Card, keypair: &[u8], origin: &str) -> Result<User> {
    let kp = decode(keypair)?;
    if user_id(&kp.public()) != card.id || kp.public().encode_protobuf() != card.key {
        return Err(Error::Crypto("link: the key does not match the user".into()));
    }
    let me = super::identity::peer_id(store)?.map(|p| p.to_string()).unwrap_or_default();
    let key = kp.public();
    if !card.nodes.iter().any(|n| n.peer_id == me && n.active() && verify(&key, n)) {
        return Err(Error::Crypto("link: no certificate for this node".into()));
    }
    if !card.nodes.iter().any(|n| n.peer_id == from_peer && n.active() && verify(&key, n)) {
        return Err(Error::Crypto("link: the approving node is not the user's".into()));
    }
    let old = store.self_user()?;
    if old.as_ref().is_some_and(|o| o.id == card.id) {
        return Ok(old.expect("checked"));
    }
    let u = store.adopt_self_user(&card.id, &card.label, &card.key, keypair)?;
    for n in card.nodes.iter().filter(|n| verify(&key, n)) {
        store.put_user_node(n)?;
    }
    if let Some(o) = old {
        crate::store::users::move_owned(store, &o.id, &u.id)?;
    }
    store.note_action(
        &crate::store::NewAction::new("user.link", origin, format!("this node now acts for {}", u.name()))
            .params(serde_json::json!({ "user": u.id, "by": from_peer })),
    );
    Ok(u)
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
