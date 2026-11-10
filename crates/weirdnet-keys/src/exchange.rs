//! X25519 key agreement and HKDF exchange keys.
//!
//! Message formats, signatures and transcripts belong to the protocol that uses this; it
//! passes the transcript hash and its own HKDF infos in.
use crate::error::{KeysError, Result};
use crate::kdf::hkdf_expand;
use crate::key::Key32;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

/// An X25519 secret for one exchange (wiped on drop).
pub struct ExchangeSecret(StaticSecret);

impl ExchangeSecret {
    pub fn generate() -> Self {
        ExchangeSecret(StaticSecret::random_from_rng(rand_core::OsRng))
    }
    pub fn from_bytes(b: [u8; 32]) -> Self {
        ExchangeSecret(StaticSecret::from(b))
    }
    pub fn to_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.0.to_bytes())
    }
    pub fn public(&self) -> [u8; 32] {
        PublicKey::from(&self.0).to_bytes()
    }
    /// The shared secret with `their_public`, or `None` when their key forces a
    /// non-contributory (low-order) result.
    pub fn agree(&self, their_public: &[u8; 32]) -> Option<Zeroizing<[u8; 32]>> {
        let shared = self.0.diffie_hellman(&PublicKey::from(*their_public));
        shared.was_contributory().then(|| Zeroizing::new(shared.to_bytes()))
    }
}

impl std::fmt::Debug for ExchangeSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExchangeSecret(<redacted>)")
    }
}

/// `HKDF(salt = transcript, ikm = shared)`: a 32-byte key under `key_info` and an 8-byte
/// short authentication string under `sas_info`.
pub fn derive(shared: &[u8; 32], transcript: &[u8], key_info: &[u8], sas_info: &[u8]) -> Result<(Key32, [u8; 8])> {
    if key_info == sas_info {
        return Err(KeysError::crypto("the key and SAS infos must differ"));
    }
    let mut key = Key32([0u8; 32]);
    hkdf_expand(shared, Some(transcript), key_info, &mut key.0)?;
    let mut sas = [0u8; 8];
    hkdf_expand(shared, Some(transcript), sas_info, &mut sas)?;
    Ok((key, sas))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_agree_and_low_order_keys_are_refused() {
        let (a, b) = (ExchangeSecret::generate(), ExchangeSecret::generate());
        let ab = a.agree(&b.public()).unwrap();
        assert_eq!(*ab, *b.agree(&a.public()).unwrap());
        assert!(a.agree(&[0u8; 32]).is_none());
        let a2 = ExchangeSecret::from_bytes(*a.to_bytes());
        assert_eq!(a2.public(), a.public());
        let (k1, s1) = derive(&ab, b"t1", b"k", b"s").unwrap();
        let (k2, s2) = derive(&ab, b"t2", b"k", b"s").unwrap();
        assert!(k1 != k2 && s1 != s2);
        assert!(derive(&ab, b"t", b"x", b"x").is_err());
        assert_eq!(format!("{a:?}"), "ExchangeSecret(<redacted>)");
    }
}
