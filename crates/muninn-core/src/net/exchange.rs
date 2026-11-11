//! Key exchanges and invitations, through weirdnet-net with Muninn's profile
//! ([`MUNINN`]): authenticated X25519 between two node identities, signed offers and
//! answers, a short code to compare, tokens (`mnnn-x1-offer:` / `mnnn-x1-answer:`) and
//! links (`muninn://join/…`, `muninn://answer/…`), and messages sealed under the exchange
//! key. The formats and the derivation are the crate's (see `weirdnet_net::exchange`); the
//! labels are Muninn's, unchanged, so they interoperate with v2.23.0 and earlier nodes. What
//! each scope grants (orgs, reviews, links) is Muninn's policy.
use super::profile::MUNINN;
use crate::crypto::{Key32, Sealed};
use crate::error::Result;
use libp2p::PeerId;
use libp2p::identity::Keypair;
use zeroize::Zeroizing;

pub use weirdnet_net::exchange::{Answer, DEFAULT_TTL_MS, Derived, Offer, Token, normalize_code};
pub use weirdnet_net::profile::{link_scope, node_scope, scope_kind};

pub const OFFER_PREFIX: &str = MUNINN.offer_prefix;
pub const ANSWER_PREFIX: &str = MUNINN.answer_prefix;
#[cfg(test)]
pub(crate) const OFFER_DOMAIN: &[u8] = MUNINN.offer_domain.as_bytes();
#[cfg(test)]
pub(crate) const ANSWER_DOMAIN: &[u8] = MUNINN.answer_domain.as_bytes();
#[cfg(test)]
pub(crate) const TRANSCRIPT_DOMAIN: &[u8] = MUNINN.transcript_domain.as_bytes();
#[cfg(test)]
pub(crate) const KEY_INFO: &[u8] = MUNINN.key_info.as_bytes();
#[cfg(test)]
pub(crate) const SAS_INFO: &[u8] = MUNINN.sas_info.as_bytes();
/// Links carry offers (invitations) and answers.
pub const JOIN_LINK: &str = MUNINN.join_link;
pub const ANSWER_LINK: &str = MUNINN.answer_link;

/// Offers under Muninn's profile.
pub trait OfferExt: Sized {
    /// A new offer and its X25519 secret (kept until the answer arrives).
    fn new(kp: &Keypair, label: &str, addrs: Vec<String>, scope: serde_json::Value, now: i64, ttl_ms: i64) -> Result<(Self, Zeroizing<[u8; 32]>)>;
    /// Check the signature, key and scope; returns the offerer's peer id.
    fn verify(&self, now: i64) -> Result<PeerId>;
    /// SHA-256 of the signed offer.
    fn hash(&self) -> Result<String>;
    fn token(&self) -> Result<String>;
}

impl OfferExt for Offer {
    fn new(kp: &Keypair, label: &str, addrs: Vec<String>, scope: serde_json::Value, now: i64, ttl_ms: i64) -> Result<(Offer, Zeroizing<[u8; 32]>)> {
        Ok(MUNINN.offer(kp, label, addrs, scope, now, ttl_ms)?)
    }
    fn verify(&self, now: i64) -> Result<PeerId> {
        Ok(MUNINN.verify_offer(self, now)?)
    }
    fn hash(&self) -> Result<String> {
        Ok(MUNINN.offer_hash(self)?)
    }
    fn token(&self) -> Result<String> {
        Ok(MUNINN.offer_token(self)?)
    }
}

/// Answers under Muninn's profile.
pub trait AnswerExt: Sized {
    /// Answer an offer (after [`OfferExt::verify`]): the answer and the derived key.
    fn new(kp: &Keypair, offer: &Offer, label: &str, addrs: Vec<String>, now: i64) -> Result<(Self, Derived)>;
    /// Check the answer against our offer; returns the answerer's peer id.
    fn verify(&self, offer: &Offer) -> Result<PeerId>;
    /// The offerer's side: the key from its secret and the answer.
    fn complete(&self, offer: &Offer, secret: &[u8]) -> Result<Derived>;
    fn token(&self) -> Result<String>;
}

impl AnswerExt for Answer {
    fn new(kp: &Keypair, offer: &Offer, label: &str, addrs: Vec<String>, now: i64) -> Result<(Answer, Derived)> {
        Ok(MUNINN.answer(kp, offer, label, addrs, now)?)
    }
    fn verify(&self, offer: &Offer) -> Result<PeerId> {
        Ok(MUNINN.verify_answer(self, offer)?)
    }
    fn complete(&self, offer: &Offer, secret: &[u8]) -> Result<Derived> {
        Ok(MUNINN.complete(self, offer, secret)?)
    }
    fn token(&self) -> Result<String> {
        Ok(MUNINN.answer_token(self)?)
    }
}

/// The link form of a token (what the user passes on: one address, nothing to verify).
pub fn link(token: &str) -> String {
    MUNINN.link(token)
}

/// A token from a link or a token (whitespace and line breaks dropped).
pub fn unlink(input: &str) -> String {
    MUNINN.unlink(input)
}

/// Does this look like a token or a link (rather than an id or a name)?
pub fn is_token(input: &str) -> bool {
    MUNINN.is_token(input)
}

/// Read an offer or answer token, or its link (whitespace and line breaks are ignored).
pub fn parse_token(s: &str) -> Result<Token> {
    Ok(MUNINN.parse_token(s)?)
}

/// Seal a message for the other side of an exchange (the exchange and both peer ids are
/// bound as associated data).
pub fn seal_msg(key: &Key32, exchange: &str, from: &PeerId, to: &PeerId, plaintext: &[u8]) -> Result<Sealed> {
    Ok(MUNINN.seal_msg(key, exchange, from, to, plaintext)?)
}

pub fn open_msg(key: &Key32, exchange: &str, from: &PeerId, to: &PeerId, s: &Sealed) -> Result<Zeroizing<Vec<u8>>> {
    Ok(MUNINN.open_msg(key, exchange, from, to, s)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn links_round_trip() {
        let t = format!("{}abc_-", super::OFFER_PREFIX);
        let l = super::link(&t);
        assert!(l.starts_with("muninn://join/"));
        assert_eq!(super::unlink(&l), t);
        assert_eq!(super::unlink(&t), t);
        assert!(super::is_token(&l) && super::is_token(&t) && !super::is_token("ben"));
    }

    use super::*;

    #[test]
    fn both_sides_derive_the_same_key_and_code() {
        let (a, b) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
        let (offer, secret) = Offer::new(&a, "anna", vec!["/ip4/127.0.0.1/tcp/4741".into()], node_scope(), 1_000, DEFAULT_TTL_MS).unwrap();
        let tok = offer.token().unwrap();
        let Token::Offer(o2) = parse_token(&format!("  {}\n{} ", &tok[..20], &tok[20..])).unwrap() else { panic!() };
        assert_eq!(o2, offer);
        assert_eq!(o2.verify(2_000).unwrap(), a.public().to_peer_id());
        let (answer, d_b) = Answer::new(&b, &o2, "ben", vec![], 2_000).unwrap();
        let Token::Answer(a2) = parse_token(&answer.token().unwrap()).unwrap() else { panic!() };
        let d_a = a2.complete(&offer, &secret[..]).unwrap();
        assert_eq!(d_a.key.0, d_b.key.0);
        assert_eq!(d_a.short_code, d_b.short_code);
        assert_eq!(d_a.fingerprint, d_b.fingerprint);
        assert_eq!(d_a.short_code.len(), 11);
        assert_eq!(a2.verify(&offer).unwrap(), b.public().to_peer_id());
    }

    #[test]
    fn tampering_expiry_and_reserved_scopes_are_refused() {
        let (a, b) = (Keypair::generate_ed25519(), Keypair::generate_ed25519());
        let (offer, secret) = Offer::new(&a, "anna", vec![], node_scope(), 1_000, 60_000).unwrap();
        // a changed address breaks the signature
        let mut bad = offer.clone();
        bad.addrs.push("/ip4/6.6.6.6/tcp/1".into());
        assert!(bad.verify(1_000).is_err());
        // someone else's key under the offerer's id
        let mut bad = offer.clone();
        bad.key = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b.public().encode_protobuf());
        assert!(bad.verify(1_000).is_err());
        assert!(offer.verify(1_000 + 61_000).unwrap_err().to_string().contains("expired"));
        assert!(Offer::new(&a, "", vec![], serde_json::json!({"kind": "organization"}), 0, 60_000).is_err());
        assert!(Offer::new(&a, "", vec![], serde_json::json!({"kind": "resource"}), 0, 60_000).is_err());
        assert!(Offer::new(&a, "", vec![], serde_json::json!({"kind": "resource", "as": "review", "review": "r1"}), 0, 60_000).is_ok());
        assert!(Offer::new(&a, "", vec![], serde_json::json!({"kind": "organization", "org": "o1", "as": "apply"}), 0, 60_000).is_ok());
        // own offer
        assert!(Answer::new(&a, &offer, "", vec![], 1_000).is_err());
        // an answer to another offer, or altered
        let (answer, _) = Answer::new(&b, &offer, "ben", vec![], 1_000).unwrap();
        let (other, _) = Offer::new(&a, "anna", vec![], node_scope(), 1_000, 60_000).unwrap();
        assert!(answer.complete(&other, &secret[..]).is_err());
        let mut bad = answer.clone();
        bad.label = "mallory".into();
        assert!(bad.complete(&offer, &secret[..]).is_err());
        // a man in the middle answering with his own key gets a different code
        let m = Keypair::generate_ed25519();
        let (_, d_m) = Answer::new(&m, &offer, "ben", vec![], 1_000).unwrap();
        let d = answer.complete(&offer, &secret[..]).unwrap();
        assert_ne!(d.short_code, d_m.short_code);
        assert!(parse_token("hello").is_err());
        assert_eq!(normalize_code("12345 67890"), "12345-67890");
    }

    #[test]
    fn sealed_messages_bind_exchange_and_direction() {
        let k = Key32([3u8; 32]);
        let (a, b) = (PeerId::random(), PeerId::random());
        let s = seal_msg(&k, "x", &a, &b, b"hi").unwrap();
        assert_eq!(&open_msg(&k, "x", &a, &b, &s).unwrap()[..], b"hi");
        assert!(open_msg(&k, "x", &b, &a, &s).is_err());
        assert!(open_msg(&k, "y", &a, &b, &s).is_err());
        assert!(open_msg(&Key32([4u8; 32]), "x", &a, &b, &s).is_err());
    }
}
