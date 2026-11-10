//! 256-bit keys and randomness (the operating system's generator).
use crate::error::{KeysError, Result};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// A 256-bit secret key, wiped on drop and never printed.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct Key32(pub [u8; 32]);

impl std::fmt::Debug for Key32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key32(<redacted>)")
    }
}

impl Key32 {
    pub fn random() -> Result<Self> {
        let mut k = Key32([0u8; 32]);
        getrandom::getrandom(&mut k.0).map_err(|e| KeysError::crypto(e.to_string()))?;
        Ok(k)
    }
    pub fn from_slice(b: &[u8]) -> Result<Self> {
        if b.len() != 32 {
            return Err(KeysError::crypto("key must be 32 bytes"));
        }
        let mut k = Key32([0u8; 32]);
        k.0.copy_from_slice(b);
        Ok(k)
    }
    pub fn hex(&self) -> Zeroizing<String> {
        Zeroizing::new(hex::encode(self.0))
    }
    pub fn from_hex(s: &str) -> Result<Self> {
        let b = Zeroizing::new(hex::decode(s.trim()).map_err(|e| KeysError::crypto(format!("bad key hex: {e}")))?);
        Self::from_slice(&b)
    }
}

/// `N` random bytes from the operating system.
pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|e| KeysError::crypto(e.to_string()))?;
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_random_redacted_and_sized() {
        let (a, b) = (Key32::random().unwrap(), Key32::random().unwrap());
        assert_ne!(a, b);
        assert_eq!(format!("{a:?}"), "Key32(<redacted>)");
        assert_eq!(Key32::from_hex(&a.hex()).unwrap(), a);
        assert!(Key32::from_slice(&[0u8; 31]).is_err());
        assert!(Key32::from_hex("zz").is_err());
    }
}
