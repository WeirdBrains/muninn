//! Password (scrypt) age files.
use crate::error::{KeysError, Result};
use secrecy::SecretString;
use zeroize::Zeroizing;

/// scrypt work factor for new files (2^16).
pub const WORK_FACTOR: u8 = 16;

pub fn encrypt(password: &SecretString, plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut r = age::scrypt::Recipient::new(password.clone());
    r.set_work_factor(WORK_FACTOR);
    age::encrypt(&r, plaintext).map_err(|e| KeysError::crypto(format!("age: {e}")))
}

/// Decrypts; age bounds the work factor it accepts. A wrong password and a damaged file
/// both give [`KeysError::Unlock`].
pub fn decrypt(password: &SecretString, ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let id = age::scrypt::Identity::new(password.clone());
    age::decrypt(&id, ct).map(Zeroizing::new).map_err(|e| KeysError::Unlock(format!("could not decrypt export (wrong password?): {e}")))
}

pub fn is_age(bytes: &[u8]) -> bool {
    bytes.starts_with(b"age-encryption.org/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wrong_password_and_tamper() {
        let pw = SecretString::from("pw");
        let ct = encrypt(&pw, b"hello").unwrap();
        assert!(is_age(&ct) && !is_age(b"hello"));
        assert_eq!(&decrypt(&pw, &ct).unwrap()[..], b"hello");
        assert!(matches!(decrypt(&SecretString::from("no"), &ct), Err(KeysError::Unlock(_))));
        let mut t = ct.clone();
        let n = t.len() - 3;
        t[n] ^= 1;
        assert!(decrypt(&pw, &t).is_err());
    }
}
