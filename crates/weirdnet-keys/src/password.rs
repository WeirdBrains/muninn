//! Password sealing (Argon2id, then XChaCha20-Poly1305) and password claim tags.
use crate::aead::{Sealed, open, seal};
use crate::error::{KeysError, Result};
use crate::kdf::{ARGON2_M_COST, ARGON2_P_COST, ARGON2_T_COST, argon2id};
use crate::key::random_bytes;
use crate::profile::Profile;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// A message sealed under a password, with the Argon2id parameters that derive its key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PasswordSealed {
    pub kdf: String,
    pub salt: String,
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    #[serde(flatten)]
    pub sealed: Sealed,
}

fn nonempty(password: &SecretString) -> Result<()> {
    if password.expose_secret().is_empty() {
        return Err(KeysError::invalid("password", "must not be empty"));
    }
    Ok(())
}

pub fn password_seal(password: &SecretString, plaintext: &[u8], aad: &[u8]) -> Result<PasswordSealed> {
    nonempty(password)?;
    let salt: [u8; 16] = random_bytes()?;
    let (m, t, p) = (ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST);
    let k = argon2id(password, &salt, m, t, p)?;
    Ok(PasswordSealed { kdf: "argon2id".into(), salt: hex::encode(salt), m_cost: m, t_cost: t, p_cost: p, sealed: seal(&k, plaintext, aad)? })
}

/// Opens a password seal; a wrong password and a tampered seal both give
/// `Unlock("wrong password")`. Only `argon2id` seals within the parameter bounds are tried.
pub fn password_open(password: &SecretString, s: &PasswordSealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if s.kdf != "argon2id" {
        return Err(KeysError::crypto(format!("unsupported password KDF {:?}", s.kdf)));
    }
    let salt = hex::decode(&s.salt).map_err(|_| KeysError::crypto("bad salt"))?;
    let k = argon2id(password, &salt, s.m_cost, s.t_cost, s.p_cost)?;
    open(&k, &s.sealed, aad).map_err(|_| KeysError::Unlock("wrong password".into()))
}

/// A tag derived from a password alone (Argon2id with the profile's fixed claim salt, then
/// keyed BLAKE3 of "claim"): lets a custodian find what it holds for someone who has only
/// the password. Anyone holding a tag can test password guesses against it at Argon2id cost.
pub fn claim_tag(profile: &Profile, password: &SecretString) -> Result<String> {
    nonempty(password)?;
    let k = argon2id(password, profile.claim_salt, ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST)?;
    Ok(hex::encode(blake3::keyed_hash(&k.0, b"claim").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_seal_rejects_wrong_password_tamper_and_bad_params() {
        let pw = SecretString::from("correct horse");
        let s = password_seal(&pw, b"key", b"aad").unwrap();
        assert_eq!(&password_open(&pw, &s, b"aad").unwrap()[..], b"key");
        assert!(matches!(password_open(&SecretString::from("nope"), &s, b"aad"), Err(KeysError::Unlock(_))));
        assert!(matches!(password_open(&pw, &s, b"other"), Err(KeysError::Unlock(_))));
        let mut salt = hex::decode(&s.salt).unwrap();
        salt[0] ^= 1;
        assert!(matches!(password_open(&pw, &PasswordSealed { salt: hex::encode(salt), ..s.clone() }, b"aad"), Err(KeysError::Unlock(_))));
        assert!(password_open(&pw, &PasswordSealed { t_cost: 3, ..s.clone() }, b"aad").is_err());
        assert!(password_open(&pw, &PasswordSealed { kdf: "scrypt".into(), ..s.clone() }, b"aad").is_err());
        assert!(password_open(&pw, &PasswordSealed { m_cost: u32::MAX, ..s }, b"aad").is_err());
        assert!(password_seal(&SecretString::from(""), b"x", b"").is_err());
    }

    #[test]
    fn claim_tags_depend_on_password_and_profile() {
        let w = Profile::WEIRDNET;
        let a = claim_tag(&w, &SecretString::from("pw one")).unwrap();
        assert_eq!(a, claim_tag(&w, &SecretString::from("pw one")).unwrap());
        assert_ne!(a, claim_tag(&w, &SecretString::from("pw two")).unwrap());
        assert_ne!(a, claim_tag(&Profile { claim_salt: b"another/claim/salt", ..w }, &SecretString::from("pw one")).unwrap());
        assert!(claim_tag(&w, &SecretString::from("")).is_err());
    }
}
