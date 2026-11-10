//! HMAC-SHA256 with constant-time verification.
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// An HMAC-SHA256 in progress.
pub struct HmacSha256(Hmac<Sha256>);

impl HmacSha256 {
    pub fn new(key: &[u8]) -> Self {
        HmacSha256(<Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes any key size"))
    }
    pub fn update(&mut self, data: &[u8]) -> &mut Self {
        self.0.update(data);
        self
    }
    pub fn finalize(self) -> [u8; 32] {
        self.0.finalize().into_bytes().into()
    }
    /// Constant-time check of a full 32-byte tag.
    pub fn verify(self, tag: &[u8]) -> bool {
        self.0.verify_slice(tag).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4231_case_2_and_verify() {
        let mut m = HmacSha256::new(b"Jefe");
        m.update(b"what do ya want ").update(b"for nothing?");
        let tag = m.finalize();
        assert_eq!(hex::encode(tag), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        let mut v = HmacSha256::new(b"Jefe");
        v.update(b"what do ya want for nothing?");
        assert!(v.verify(&tag));
        let mut v = HmacSha256::new(b"Jefe");
        v.update(b"what do ya want for nothing!");
        assert!(!v.verify(&tag));
        assert!(!HmacSha256::new(b"Jefe").verify(&tag[..16]));
    }
}
