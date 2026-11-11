//! Key exchanges: authenticated X25519 Diffie-Hellman between two node identities, and the
//! invitation tokens and links that carry them.
//!
//! The offerer signs an *offer* (its node key, a fresh X25519 public key, where to reach it,
//! the scope and an expiry) with its Ed25519 node identity. The answerer checks the
//! signature, signs an *answer* bound to the offer's hash with its own fresh X25519 key,
//! and both compute the same secret:
//!
//! ```text
//! transcript = SHA-256(TRANSCRIPT ‖ OFFER ‖ 0 ‖ offer ‖ ANSWER ‖ 0 ‖ answer)   (both signed)
//! key        = HKDF-SHA256(ikm = X25519(a, B), salt = transcript, info = KEY)
//! short code = HKDF(..., info = SAS) as ten digits, 12345-67890
//! ```
//!
//! The labels come from the [`Profile`]; the agreement and the derivations are done by
//! `weirdnet-keys`. Both people compare the short code (or the fingerprint) out of band to
//! rule out a man in the middle. Tokens are the profile's prefix and base64url JSON, safe
//! to paste anywhere: they hold only public keys and signatures.
use crate::error::{NetError, Result};
use crate::profile::Profile;
use base64::Engine;
use libp2p::PeerId;
use libp2p::identity::{Keypair, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use weirdnet_keys::exchange::{self as kx, ExchangeSecret};
use weirdnet_keys::{Key32, Sealed};
use zeroize::Zeroizing;

/// Offers are good for a week unless asked otherwise (never less than a minute).
pub const DEFAULT_TTL_MS: i64 = 7 * 24 * 3600 * 1000;
const MIN_TTL_MS: i64 = 60_000;

/// A signed offer (an invitation to exchange keys).
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

/// A pasted token.
#[derive(Debug, Clone)]
pub enum Token {
    Offer(Offer),
    Answer(Answer),
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
}

fn signed_bytes<T: Serialize>(domain: &str, t: &T) -> Result<Vec<u8>> {
    let mut v = domain.as_bytes().to_vec();
    v.push(0);
    v.extend(serde_json::to_vec(t)?);
    Ok(v)
}

fn public_key(b64key: &str, peer: &str) -> Result<PublicKey> {
    let raw = b64().decode(b64key).map_err(|_| NetError::invalid("exchange", "bad node key"))?;
    let pk = PublicKey::try_decode_protobuf(&raw).map_err(|_| NetError::invalid("exchange", "bad node key"))?;
    if pk.to_peer_id().to_string() != peer {
        return Err(NetError::invalid("exchange", "the node key does not match the peer id"));
    }
    Ok(pk)
}

fn x_public(hexkey: &str) -> Result<[u8; 32]> {
    hex::decode(hexkey).ok().and_then(|v| v.try_into().ok()).ok_or_else(|| NetError::invalid("exchange", "bad exchange key"))
}

impl Offer {
    /// The offerer's node key (checked against its peer id).
    pub fn public_key(&self) -> Result<PublicKey> {
        public_key(&self.key, &self.peer)
    }
}

impl Answer {
    /// The answerer's node key (checked against its peer id).
    pub fn public_key(&self) -> Result<PublicKey> {
        public_key(&self.key, &self.peer)
    }
}

/// Normalize a typed short code (`1234567890`, `12345 67890`) to `12345-67890`.
pub fn normalize_code(s: &str) -> String {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() == 10 { format!("{}-{}", &d[..5], &d[5..]) } else { s.trim().to_string() }
}

impl Profile {
    /// A new offer and its X25519 secret (kept until the answer arrives).
    pub fn offer(
        &self,
        kp: &Keypair,
        label: &str,
        addrs: Vec<String>,
        scope: serde_json::Value,
        now: i64,
        ttl_ms: i64,
    ) -> Result<(Offer, Zeroizing<[u8; 32]>)> {
        (self.scopes)(&scope)?;
        let secret = ExchangeSecret::generate();
        let mut o = Offer {
            v: 1,
            id: uuid::Uuid::new_v4().simple().to_string(),
            peer: kp.public().to_peer_id().to_string(),
            key: b64().encode(kp.public().encode_protobuf()),
            eph: hex::encode(secret.public()),
            addrs,
            label: label.to_string(),
            scope,
            created: now,
            expires: now + ttl_ms.max(MIN_TTL_MS),
            sig: String::new(),
        };
        let sig = kp.sign(&signed_bytes(self.offer_domain, &o)?).map_err(|e| NetError::Crypto(e.to_string()))?;
        o.sig = b64().encode(sig);
        Ok((o, secret.to_bytes()))
    }

    /// Check an offer's version, scope, key, signature and expiry; returns the offerer.
    pub fn verify_offer(&self, o: &Offer, now: i64) -> Result<PeerId> {
        if o.v != 1 {
            return Err(NetError::invalid("exchange", format!("offer version {} is not supported", o.v)));
        }
        (self.scopes)(&o.scope)?;
        let pk = public_key(&o.key, &o.peer)?;
        let sig = b64().decode(&o.sig).map_err(|_| NetError::invalid("exchange", "bad signature"))?;
        let unsigned = Offer { sig: String::new(), ..o.clone() };
        if !pk.verify(&signed_bytes(self.offer_domain, &unsigned)?, &sig) {
            return Err(NetError::invalid("exchange", "the offer's signature does not verify"));
        }
        if now > o.expires {
            return Err(NetError::invalid("exchange", "the offer has expired; ask for a new one"));
        }
        Ok(pk.to_peer_id())
    }

    /// SHA-256 of the signed offer (hex).
    pub fn offer_hash(&self, o: &Offer) -> Result<String> {
        Ok(hex::encode(sha2::Sha256::digest(signed_bytes(self.offer_domain, o)?)))
    }

    pub fn offer_token(&self, o: &Offer) -> Result<String> {
        Ok(format!("{}{}", self.offer_prefix, b64().encode(serde_json::to_vec(o)?)))
    }

    /// Answer an offer (after [`Profile::verify_offer`]): the answer and the derived key.
    pub fn answer(&self, kp: &Keypair, offer: &Offer, label: &str, addrs: Vec<String>, now: i64) -> Result<(Answer, Derived)> {
        if offer.peer == kp.public().to_peer_id().to_string() {
            return Err(NetError::invalid("exchange", "this is this node's own offer; it is answered on the other node"));
        }
        let secret = ExchangeSecret::generate();
        let mut a = Answer {
            v: 1,
            offer: offer.id.clone(),
            offer_hash: self.offer_hash(offer)?,
            peer: kp.public().to_peer_id().to_string(),
            key: b64().encode(kp.public().encode_protobuf()),
            eph: hex::encode(secret.public()),
            addrs,
            label: label.to_string(),
            created: now,
            sig: String::new(),
        };
        let sig = kp.sign(&signed_bytes(self.answer_domain, &a)?).map_err(|e| NetError::Crypto(e.to_string()))?;
        a.sig = b64().encode(sig);
        let shared = secret.agree(&x_public(&offer.eph)?).ok_or_else(|| NetError::invalid("exchange", "the offer's exchange key is invalid"))?;
        let d = self.derive(&shared, offer, &a)?;
        Ok((a, d))
    }

    /// Check an answer against our offer; returns the answerer.
    pub fn verify_answer(&self, a: &Answer, offer: &Offer) -> Result<PeerId> {
        if a.v != 1 {
            return Err(NetError::invalid("exchange", format!("answer version {} is not supported", a.v)));
        }
        if a.offer != offer.id || a.offer_hash != self.offer_hash(offer)? {
            return Err(NetError::invalid("exchange", "the answer is for a different offer"));
        }
        let pk = public_key(&a.key, &a.peer)?;
        let sig = b64().decode(&a.sig).map_err(|_| NetError::invalid("exchange", "bad signature"))?;
        let unsigned = Answer { sig: String::new(), ..a.clone() };
        if !pk.verify(&signed_bytes(self.answer_domain, &unsigned)?, &sig) {
            return Err(NetError::invalid("exchange", "the answer's signature does not verify"));
        }
        if a.peer == offer.peer {
            return Err(NetError::invalid("exchange", "an offer cannot be answered by the node that made it"));
        }
        Ok(pk.to_peer_id())
    }

    /// The offerer's side: the key from its kept secret and the answer.
    pub fn complete(&self, a: &Answer, offer: &Offer, secret: &[u8]) -> Result<Derived> {
        self.verify_answer(a, offer)?;
        let s: [u8; 32] = secret.try_into().map_err(|_| NetError::Crypto("bad exchange secret".into()))?;
        let secret = ExchangeSecret::from_bytes(s);
        let shared = secret.agree(&x_public(&a.eph)?).ok_or_else(|| NetError::invalid("exchange", "the answer's exchange key is invalid"))?;
        self.derive(&shared, offer, a)
    }

    pub fn answer_token(&self, a: &Answer) -> Result<String> {
        Ok(format!("{}{}", self.answer_prefix, b64().encode(serde_json::to_vec(a)?)))
    }

    fn derive(&self, dh: &[u8; 32], offer: &Offer, answer: &Answer) -> Result<Derived> {
        let mut t = sha2::Sha256::new();
        t.update(self.transcript_domain.as_bytes());
        t.update(signed_bytes(self.offer_domain, offer)?);
        t.update(signed_bytes(self.answer_domain, answer)?);
        let transcript = t.finalize();
        let (key, sas) = kx::derive(dh, &transcript, self.key_info.as_bytes(), self.sas_info.as_bytes())?;
        let n = u64::from_be_bytes(sas) % 10_000_000_000;
        let code = format!("{:05}-{:05}", n / 100_000, n % 100_000);
        Ok(Derived { key, short_code: code, fingerprint: crate::identity::group(&hex::encode(&transcript[..8])) })
    }

    /// Read an offer or answer token, or its link (whitespace and line breaks ignored).
    pub fn parse_token(&self, s: &str) -> Result<Token> {
        let s: String = self.unlink(s);
        let (body, offer) = if let Some(b) = s.strip_prefix(self.offer_prefix) {
            (b, true)
        } else if let Some(b) = s.strip_prefix(self.answer_prefix) {
            (b, false)
        } else {
            return Err(NetError::invalid(
                "token",
                format!("not an exchange token (they start with `{}` or `{}`)", self.offer_prefix, self.answer_prefix),
            ));
        };
        let raw = b64().decode(body).map_err(|_| NetError::invalid("token", "damaged (base64)"))?;
        Ok(if offer {
            Token::Offer(serde_json::from_slice(&raw).map_err(|e| NetError::invalid("token", format!("damaged ({e})")))?)
        } else {
            Token::Answer(serde_json::from_slice(&raw).map_err(|e| NetError::invalid("token", format!("damaged ({e})")))?)
        })
    }

    fn msg_aad(&self, exchange: &str, from: &PeerId, to: &PeerId) -> Vec<u8> {
        format!("{}|{exchange}|{from}|{to}", self.msg_aad).into_bytes()
    }

    /// Seal a message for the other side of an exchange (XChaCha20-Poly1305 under the
    /// exchange key, by `weirdnet-keys`; the exchange and both peer ids are bound as
    /// associated data, so a message can't be replayed into another exchange or direction).
    pub fn seal_msg(&self, key: &Key32, exchange: &str, from: &PeerId, to: &PeerId, plaintext: &[u8]) -> Result<Sealed> {
        Ok(weirdnet_keys::seal(key, plaintext, &self.msg_aad(exchange, from, to))?)
    }

    /// Open a message sealed by [`Profile::seal_msg`]; every failure reads the same.
    pub fn open_msg(&self, key: &Key32, exchange: &str, from: &PeerId, to: &PeerId, s: &Sealed) -> Result<Zeroizing<Vec<u8>>> {
        weirdnet_keys::open(key, s, &self.msg_aad(exchange, from, to)).map_err(|_| NetError::Crypto("a message failed to authenticate".into()))
    }
}
