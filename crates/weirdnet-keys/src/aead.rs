//! XChaCha20-Poly1305 sealing with a random 192-bit nonce per message.
use crate::error::{KeysError, Result};
use crate::key::{Key32, random_bytes};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// A sealed message: hex nonce and hex ciphertext (with its tag).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sealed {
    pub nonce: String,
    pub ct: String,
}

pub fn seal(key: &Key32, plaintext: &[u8], aad: &[u8]) -> Result<Sealed> {
    let c = XChaCha20Poly1305::new((&key.0).into());
    let nonce: [u8; 24] = random_bytes()?;
    let ct = c.encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad }).map_err(|_| KeysError::crypto("encryption failed"))?;
    Ok(Sealed { nonce: hex::encode(nonce), ct: hex::encode(ct) })
}

/// Opens a sealed message. A wrong key, wrong associated data or any change to the nonce
/// or ciphertext gives the same [`KeysError::Unlock`].
pub fn open(key: &Key32, s: &Sealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let c = XChaCha20Poly1305::new((&key.0).into());
    let nonce = hex::decode(&s.nonce).map_err(|_| KeysError::crypto("bad nonce"))?;
    if nonce.len() != 24 {
        return Err(KeysError::crypto("bad nonce"));
    }
    let ct = hex::decode(&s.ct).map_err(|_| KeysError::crypto("bad ciphertext"))?;
    c.decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad })
        .map(Zeroizing::new)
        .map_err(|_| KeysError::Unlock("wrong key or password".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_and_reject_tampering() {
        let k = Key32::random().unwrap();
        let s = seal(&k, b"secret", b"aad").unwrap();
        assert_eq!(&open(&k, &s, b"aad").unwrap()[..], b"secret");
        assert_ne!(seal(&k, b"secret", b"aad").unwrap().nonce, s.nonce);
        assert!(matches!(open(&Key32::random().unwrap(), &s, b"aad"), Err(KeysError::Unlock(_))));
        assert!(matches!(open(&k, &s, b"other"), Err(KeysError::Unlock(_))));
        let mut ct = hex::decode(&s.ct).unwrap();
        ct[0] ^= 1;
        assert!(matches!(open(&k, &Sealed { ct: hex::encode(ct), ..s.clone() }, b"aad"), Err(KeysError::Unlock(_))));
        let mut n = hex::decode(&s.nonce).unwrap();
        n[23] ^= 1;
        assert!(matches!(open(&k, &Sealed { nonce: hex::encode(n), ..s.clone() }, b"aad"), Err(KeysError::Unlock(_))));
        assert!(open(&k, &Sealed { nonce: "00".into(), ..s }, b"aad").is_err());
    }
}
