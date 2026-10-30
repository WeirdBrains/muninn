//! Key exchanges: authenticated X25519 Diffie-Hellman between two node identities.
//!
//! The offerer signs an *offer* (its node key, a fresh X25519 public key, where to reach it,
//! the scope and an expiry) with its Ed25519 node identity. The answerer checks the
//! signature, signs an *answer* bound to the offer's hash with its own fresh X25519 key,
//! and both compute the same secret:
//!
//! ```text
//! transcript = SHA-256("muninn/net/exchange/v1" ‖ offer ‖ answer)      (both signed)
//! key        = HKDF-SHA256(ikm = X25519(a, B), salt = transcript, info = ".../key")
//! short code = HKDF(..., info = ".../sas") as ten digits, 12345-67890
//! ```
//!
//! Both people compare the short code (or the fingerprint) out of band to rule out a
//! man in the middle; `mnnn net exchange verify` records that they matched. Tokens are
//! `mnnn-x1-offer:` / `mnnn-x1-answer:` and base64url JSON, safe to paste anywhere: they
//! hold only public keys and signatures.
use crate::crypto::Key32;
use crate::error::{Error, Result};
use base64::Engine;
use libp2p::PeerId;
use libp2p::identity::{Keypair, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use x25519_dalek::{PublicKey as XPublic, StaticSecret};
use zeroize::Zeroizing;

pub const OFFER_PREFIX: &str = "mnnn-x1-offer:";
pub const ANSWER_PREFIX: &str = "mnnn-x1-answer:";
const OFFER_DOMAIN: &[u8] = b"muninn/net/offer/v1";
const ANSWER_DOMAIN: &[u8] = b"muninn/net/answer/v1";
const TRANSCRIPT_DOMAIN: &[u8] = b"muninn/net/exchange/v1";
/// Offers are good for a week unless asked otherwise.
pub const DEFAULT_TTL_MS: i64 = 7 * 24 * 3600 * 1000;

/// What an exchange is for. Only `node` exists now; `organization` (2.7) and `resource`
/// (2.8, review links) are reserved so their offers are refused instead of misread.
pub fn node_scope() -> serde_json::Value {
    serde_json::json!({ "kind": "node" })
}

fn check_scope(scope: &serde_json::Value) -> Result<()> {
    match scope.get("kind").and_then(|k| k.as_str()) {
        Some("node") => Ok(()),
        Some(k @ ("organization" | "resource")) => {
            Err(Error::invalid("exchange", format!("`{k}` exchanges come with a later version of Muninn; this one only exchanges between nodes")))
        }
        _ => Err(Error::invalid("exchange", "unknown scope")),
    }
}

/// A signed offer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offer {
    pub v: u8,
    /// The exchange id (the same on both sides).
    pub id: String,
    pub peer: String,
    /// The offerer's node public key (protobuf, base64url).
    pub key: String,
    /// Its fresh X25519 public key (hex).
    pub eph: String,
    pub addrs: Vec<String>,
    pub label: String,
    pub scope: serde_json::Value,
    pub created: i64,
    pub expires: i64,
    #[serde(default)]
    pub sig: String,
}

/// A signed answer to an offer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub v: u8,
    /// The offer's exchange id.
    pub offer: String,
    /// SHA-256 of the signed offer (hex).
    pub offer_hash: String,
    pub peer: String,
    pub key: String,
    pub eph: String,
    pub addrs: Vec<String>,
    pub label: String,
    pub created: i64,
    #[serde(default)]
    pub sig: String,
}

/// What both sides derive.
pub struct Derived {
    pub key: Key32,
    /// `12345-67890`.
    pub short_code: String,
    /// The transcript's first 8 bytes, grouped: `ab12 cd34 ef56 7890`.
    pub fingerprint: String,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

fn signed_bytes<T: Serialize>(domain: &[u8], t: &T) -> Result<Vec<u8>> {
    let mut v = domain.to_vec();
    v.push(0);
    v.extend(serde_json::to_vec(t)?);
    Ok(v)
}

fn public_key(b64key: &str, peer: &str) -> Result<PublicKey> {
    let raw = b64().decode(b64key).map_err(|_| Error::invalid("exchange", "bad node key"))?;
    let pk = PublicKey::try_decode_protobuf(&raw).map_err(|_| Error::invalid("exchange", "bad node key"))?;
    if pk.to_peer_id().to_string() != peer {
        return Err(Error::invalid("exchange", "the node key does not match the peer id"));
    }
    Ok(pk)
}

fn x_public(hexkey: &str) -> Result<XPublic> {
    let b: [u8; 32] = hex::decode(hexkey).ok().and_then(|v| v.try_into().ok()).ok_or_else(|| Error::invalid("exchange", "bad exchange key"))?;
    Ok(XPublic::from(b))
}

impl Offer {
    /// A new offer and its X25519 secret (kept until the answer arrives).
    pub fn new(
        kp: &Keypair,
        label: &str,
        addrs: Vec<String>,
        scope: serde_json::Value,
        now: i64,
        ttl_ms: i64,
    ) -> Result<(Offer, Zeroizing<[u8; 32]>)> {
        check_scope(&scope)?;
        let secret = StaticSecret::random_from_rng(rand_core::OsRng);
        let mut o = Offer {
            v: 1,
            id: uuid::Uuid::new_v4().simple().to_string(),
            peer: kp.public().to_peer_id().to_string(),
            key: b64().encode(kp.public().encode_protobuf()),
            eph: hex::encode(XPublic::from(&secret).as_bytes()),
            addrs,
            label: label.to_string(),
            scope,
            created: now,
            expires: now + ttl_ms.max(60_000),
            sig: String::new(),
        };
        let sig = kp.sign(&signed_bytes(OFFER_DOMAIN, &o)?).map_err(|e| Error::Crypto(e.to_string()))?;
        o.sig = b64().encode(sig);
        Ok((o, Zeroizing::new(secret.to_bytes())))
    }

    /// Check the signature, key and scope; returns the offerer's peer id.
    pub fn verify(&self, now: i64) -> Result<PeerId> {
        if self.v != 1 {
            return Err(Error::invalid("exchange", format!("offer version {} is not supported", self.v)));
        }
        check_scope(&self.scope)?;
        let pk = public_key(&self.key, &self.peer)?;
        let sig = b64().decode(&self.sig).map_err(|_| Error::invalid("exchange", "bad signature"))?;
        let unsigned = Offer { sig: String::new(), ..self.clone() };
        if !pk.verify(&signed_bytes(OFFER_DOMAIN, &unsigned)?, &sig) {
            return Err(Error::invalid("exchange", "the offer's signature does not verify"));
        }
        if now > self.expires {
            return Err(Error::invalid("exchange", "the offer has expired; ask for a new one"));
        }
        Ok(pk.to_peer_id())
    }

    pub fn public_key(&self) -> Result<PublicKey> {
        public_key(&self.key, &self.peer)
    }

    /// SHA-256 of the signed offer.
    pub fn hash(&self) -> Result<String> {
        Ok(hex::encode(sha2::Sha256::digest(signed_bytes(OFFER_DOMAIN, self)?)))
    }

    pub fn token(&self) -> Result<String> {
        Ok(format!("{OFFER_PREFIX}{}", b64().encode(serde_json::to_vec(self)?)))
    }
}

impl Answer {
    /// Answer an offer (after [`Offer::verify`]): the answer and the derived key.
    pub fn new(kp: &Keypair, offer: &Offer, label: &str, addrs: Vec<String>, now: i64) -> Result<(Answer, Derived)> {
        if offer.peer == kp.public().to_peer_id().to_string() {
            return Err(Error::invalid("exchange", "this is this node's own offer; it is answered on the other node"));
        }
        let secret = StaticSecret::random_from_rng(rand_core::OsRng);
        let mut a = Answer {
            v: 1,
            offer: offer.id.clone(),
            offer_hash: offer.hash()?,
            peer: kp.public().to_peer_id().to_string(),
            key: b64().encode(kp.public().encode_protobuf()),
            eph: hex::encode(XPublic::from(&secret).as_bytes()),
            addrs,
            label: label.to_string(),
            created: now,
            sig: String::new(),
        };
        let sig = kp.sign(&signed_bytes(ANSWER_DOMAIN, &a)?).map_err(|e| Error::Crypto(e.to_string()))?;
        a.sig = b64().encode(sig);
        let shared = secret.diffie_hellman(&x_public(&offer.eph)?);
        if !shared.was_contributory() {
            return Err(Error::invalid("exchange", "the offer's exchange key is invalid"));
        }
        let d = derive(shared.as_bytes(), offer, &a)?;
        Ok((a, d))
    }

    /// Check the answer against our offer; returns the answerer's peer id.
    pub fn verify(&self, offer: &Offer) -> Result<PeerId> {
        if self.v != 1 {
            return Err(Error::invalid("exchange", format!("answer version {} is not supported", self.v)));
        }
        if self.offer != offer.id || self.offer_hash != offer.hash()? {
            return Err(Error::invalid("exchange", "the answer is for a different offer"));
        }
        let pk = public_key(&self.key, &self.peer)?;
        let sig = b64().decode(&self.sig).map_err(|_| Error::invalid("exchange", "bad signature"))?;
        let unsigned = Answer { sig: String::new(), ..self.clone() };
        if !pk.verify(&signed_bytes(ANSWER_DOMAIN, &unsigned)?, &sig) {
            return Err(Error::invalid("exchange", "the answer's signature does not verify"));
        }
        if self.peer == offer.peer {
            return Err(Error::invalid("exchange", "an offer cannot be answered by the node that made it"));
        }
        Ok(pk.to_peer_id())
    }

    /// The offerer's side: the key from its secret and the answer.
    pub fn complete(&self, offer: &Offer, secret: &[u8]) -> Result<Derived> {
        self.verify(offer)?;
        let s: [u8; 32] = secret.try_into().map_err(|_| Error::Crypto("bad exchange secret".into()))?;
        let secret = StaticSecret::from(s);
        let shared = secret.diffie_hellman(&x_public(&self.eph)?);
        if !shared.was_contributory() {
            return Err(Error::invalid("exchange", "the answer's exchange key is invalid"));
        }
        derive(shared.as_bytes(), offer, self)
    }

    pub fn public_key(&self) -> Result<PublicKey> {
        public_key(&self.key, &self.peer)
    }

    pub fn token(&self) -> Result<String> {
        Ok(format!("{ANSWER_PREFIX}{}", b64().encode(serde_json::to_vec(self)?)))
    }
}

fn derive(dh: &[u8; 32], offer: &Offer, answer: &Answer) -> Result<Derived> {
    let mut t = sha2::Sha256::new();
    t.update(TRANSCRIPT_DOMAIN);
    t.update(signed_bytes(OFFER_DOMAIN, offer)?);
    t.update(signed_bytes(ANSWER_DOMAIN, answer)?);
    let transcript = t.finalize();
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(&transcript), dh);
    let mut key = [0u8; 32];
    hk.expand(b"muninn/net/exchange/v1/key", &mut key).map_err(|e| Error::Crypto(e.to_string()))?;
    let mut sas = [0u8; 8];
    hk.expand(b"muninn/net/exchange/v1/sas", &mut sas).map_err(|e| Error::Crypto(e.to_string()))?;
    let n = u64::from_be_bytes(sas) % 10_000_000_000;
    let code = format!("{:05}-{:05}", n / 100_000, n % 100_000);
    Ok(Derived { key: Key32(key), short_code: code, fingerprint: super::identity::group(&hex::encode(&transcript[..8])) })
}

/// A pasted token.
#[derive(Debug, Clone)]
pub enum Token {
    Offer(Offer),
    Answer(Answer),
}

/// Read an offer or answer token (whitespace and line breaks are ignored).
pub fn parse_token(s: &str) -> Result<Token> {
    let s: String = s.split_whitespace().collect();
    let (body, offer) = if let Some(b) = s.strip_prefix(OFFER_PREFIX) {
        (b, true)
    } else if let Some(b) = s.strip_prefix(ANSWER_PREFIX) {
        (b, false)
    } else {
        return Err(Error::invalid("token", format!("not an exchange token (they start with `{OFFER_PREFIX}` or `{ANSWER_PREFIX}`)")));
    };
    let raw = b64().decode(body).map_err(|_| Error::invalid("token", "damaged (base64)"))?;
    Ok(if offer {
        Token::Offer(serde_json::from_slice(&raw).map_err(|e| Error::invalid("token", format!("damaged ({e})")))?)
    } else {
        Token::Answer(serde_json::from_slice(&raw).map_err(|e| Error::invalid("token", format!("damaged ({e})")))?)
    })
}

/// Normalize a typed short code (`1234567890`, `12345 67890`) to `12345-67890`.
pub fn normalize_code(s: &str) -> String {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() == 10 { format!("{}-{}", &d[..5], &d[5..]) } else { s.trim().to_string() }
}

// ---------- sealed messages ----------

fn aad(exchange: &str, from: &PeerId, to: &PeerId) -> Vec<u8> {
    format!("muninn/net/msg/v1|{exchange}|{from}|{to}").into_bytes()
}

/// Seal a message for the other side of an exchange (XChaCha20-Poly1305 under the
/// exchange key; the exchange and both peer ids are bound as associated data).
pub fn seal_msg(key: &Key32, exchange: &str, from: &PeerId, to: &PeerId, plaintext: &[u8]) -> Result<crate::crypto::Sealed> {
    crate::crypto::seal(key, plaintext, &aad(exchange, from, to))
}

pub fn open_msg(key: &Key32, exchange: &str, from: &PeerId, to: &PeerId, s: &crate::crypto::Sealed) -> Result<Zeroizing<Vec<u8>>> {
    crate::crypto::open(key, s, &aad(exchange, from, to)).map_err(|_| Error::Crypto("a message failed to authenticate".into()))
}

#[cfg(test)]
mod tests {
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
        bad.key = b64().encode(b.public().encode_protobuf());
        assert!(bad.verify(1_000).is_err());
        assert!(offer.verify(1_000 + 61_000).unwrap_err().to_string().contains("expired"));
        assert!(Offer::new(&a, "", vec![], serde_json::json!({"kind": "organization"}), 0, 60_000).is_err());
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
