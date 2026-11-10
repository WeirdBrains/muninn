//! Derivation: HKDF-SHA256 and Argon2id.
use crate::error::{KeysError, Result};
use crate::key::Key32;
use crate::profile::Profile;
use secrecy::{ExposeSecret, SecretString};

/// `HKDF-SHA256(ikm, salt, info)` into a 32-byte key.
pub fn hkdf(ikm: &[u8], salt: Option<&[u8]>, info: &[u8]) -> Result<Key32> {
    let mut k = Key32([0u8; 32]);
    hkdf_expand(ikm, salt, info, &mut k.0)?;
    Ok(k)
}

/// `HKDF-SHA256(ikm, salt, info)` into `out` (at most 8160 bytes).
pub fn hkdf_expand(ikm: &[u8], salt: Option<&[u8]>, info: &[u8], out: &mut [u8]) -> Result<()> {
    hkdf::Hkdf::<sha2::Sha256>::new(salt, ikm).expand(info, out).map_err(|e| KeysError::crypto(e.to_string()))
}

/// The service key of version `n`: `HKDF(master, salt = db_id, info = {label}/service/v{n})`.
pub fn service_key(profile: &Profile, master: &Key32, db_id: &str, version: u32) -> Result<Key32> {
    hkdf(&master.0, Some(db_id.as_bytes()), profile.service_info(version).as_bytes())
}

/// The master-derived key-encryption key: `HKDF(master, salt = db_id, info = {label}/kek)`.
pub fn master_kek(profile: &Profile, master: &Key32, db_id: &str) -> Result<Key32> {
    hkdf(&master.0, Some(db_id.as_bytes()), profile.kek_info().as_bytes())
}

/// Argon2id parameters written for new password seals (OWASP's 19 MiB, 2 passes, 1 lane).
pub const ARGON2_M_COST: u32 = 19 * 1024;
pub const ARGON2_T_COST: u32 = 2;
pub const ARGON2_P_COST: u32 = 1;
/// Upper bounds accepted from a stored seal, so a hostile file cannot demand unbounded work.
pub const ARGON2_MAX_M_COST: u32 = 2 * 1024 * 1024;
pub const ARGON2_MAX_T_COST: u32 = 64;
pub const ARGON2_MAX_P_COST: u32 = 16;

/// Argon2id (v1.3) of a password into a 32-byte key.
pub fn argon2id(password: &SecretString, salt: &[u8], m_cost: u32, t_cost: u32, p_cost: u32) -> Result<Key32> {
    if m_cost > ARGON2_MAX_M_COST || t_cost > ARGON2_MAX_T_COST || p_cost > ARGON2_MAX_P_COST {
        return Err(KeysError::crypto("unsupported password KDF parameters"));
    }
    let params = argon2::Params::new(m_cost, t_cost, p_cost, Some(32)).map_err(|e| KeysError::crypto(e.to_string()))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut k = Key32([0u8; 32]);
    a.hash_password_into(password.expose_secret().as_bytes(), salt, &mut k.0).map_err(|e| KeysError::crypto(e.to_string()))?;
    Ok(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derivations_are_deterministic_and_separated() {
        let m = Key32::random().unwrap();
        let w = Profile::WEIRDNET;
        assert_eq!(service_key(&w, &m, "db", 1).unwrap(), service_key(&w, &m, "db", 1).unwrap());
        assert_ne!(service_key(&w, &m, "db", 1).unwrap(), service_key(&w, &m, "db", 2).unwrap());
        assert_ne!(service_key(&w, &m, "db", 1).unwrap(), service_key(&w, &m, "db2", 1).unwrap());
        assert_ne!(master_kek(&w, &m, "db").unwrap(), service_key(&w, &m, "db", 1).unwrap());
        let other = Profile { label: "other", ..w };
        assert_ne!(master_kek(&w, &m, "db").unwrap(), master_kek(&other, &m, "db").unwrap());
        // RFC 5869 test case 1
        let ikm = [0x0bu8; 22];
        let salt: Vec<u8> = (0u8..=0x0c).collect();
        let info: Vec<u8> = (0xf0u8..=0xf9).collect();
        let mut okm = [0u8; 42];
        hkdf_expand(&ikm, Some(&salt), &info, &mut okm).unwrap();
        assert_eq!(hex::encode(okm), "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865");
    }

    #[test]
    fn argon2_bounds_are_enforced() {
        let pw = SecretString::from("pw");
        assert!(argon2id(&pw, b"saltsalt", ARGON2_MAX_M_COST + 1, 1, 1).is_err());
        assert!(argon2id(&pw, b"saltsalt", 64, ARGON2_MAX_T_COST + 1, 1).is_err());
        assert!(argon2id(&pw, b"short", 64, 1, 1).is_err());
        assert_eq!(argon2id(&pw, b"saltsalt", 64, 1, 1).unwrap(), argon2id(&pw, b"saltsalt", 64, 1, 1).unwrap());
    }
}
