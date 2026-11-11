//! Key material for Muninn: the `weirdnet-keys` operations under Muninn's own labels.
//!
//! Every encryption, derivation, wrap, split and export runs in `weirdnet-keys`. This
//! module only fixes Muninn's existing labels ([`MUNINN_PROFILE`]) so existing databases,
//! recovery words, shards, exports and peers keep working byte for byte, and keeps the
//! function names and error kinds the rest of Muninn uses.
pub mod keyring;

use crate::error::Result;
use secrecy::SecretString;
use weirdnet_keys::{Profile, kdf, password, shards};
use zeroize::Zeroizing;

pub use weirdnet_keys::{Key32, PasswordSealed, Sealed};

/// Muninn's labels, as they were before the extraction: envelope format 1 without a
/// recorded profile, `muninn/...` HKDF infos and associated data, `muninn-shard-v1`
/// shards, the `muninn/net/shard-claim/v1` claim salt and `muninn/net/shard/v1` dark-shard
/// data. Changing any of these is a migration, not a refactor (see `compat_tests`).
pub const MUNINN_PROFILE: Profile = Profile {
    name: None,
    envelope_format: 1,
    label: "muninn",
    shard_prefix: "muninn-shard-v1",
    claim_salt: b"muninn/net/shard-claim/v1",
    dark_shard_label: "muninn/net/shard/v1",
};

pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    Ok(weirdnet_keys::random_bytes()?)
}

// ---------- BIP39 ----------

/// 24 words encoding the master key's 256 bits.
pub fn to_words(master: &Key32) -> Result<Zeroizing<String>> {
    Ok(weirdnet_keys::words::to_words(master)?)
}

pub fn from_words(words: &str) -> Result<Key32> {
    Ok(weirdnet_keys::words::from_words(words)?)
}

// ---------- Shamir 3-of-5 ----------

pub const SHARD_THRESHOLD: usize = shards::THRESHOLD;
pub const SHARD_COUNT: usize = shards::COUNT;
#[cfg(test)]
pub(crate) const SHARD_PREFIX: &str = MUNINN_PROFILE.shard_prefix;

/// Split the master key into 5 shards (any 3 recover it). Text form:
/// `muninn-shard-v1:<set id>:<threshold>:<index>:<share hex>:<checksum>`.
pub fn split_shards(master: &Key32) -> Result<Vec<Zeroizing<String>>> {
    split_shards_k(master, SHARD_THRESHOLD, SHARD_COUNT)
}

/// Split the master key into `n` shards, any `k` of which recover it (2 <= k <= n <= 255).
pub fn split_shards_k(master: &Key32, k: usize, n: usize) -> Result<Vec<Zeroizing<String>>> {
    Ok(shards::split(&MUNINN_PROFILE, master, k, n)?)
}

/// The set id, threshold and index of a shard (no secret parts).
pub fn shard_meta(s: &str) -> Result<(String, usize, u32)> {
    Ok(shards::meta(&MUNINN_PROFILE, s)?)
}

/// Combine at least 3 distinct shards from the same set.
pub fn combine_shards(shards: &[String]) -> Result<Key32> {
    Ok(shards::combine(&MUNINN_PROFILE, shards)?)
}

/// `HKDF-SHA256(master, salt = db_id, info = "muninn/service/v{n}")`.
pub fn service_key(master: &Key32, db_id: &str, version: u32) -> Result<Key32> {
    Ok(kdf::service_key(&MUNINN_PROFILE, master, db_id, version)?)
}

/// Master-derived key-encryption key.
pub fn master_kek(master: &Key32, db_id: &str) -> Result<Key32> {
    Ok(kdf::master_kek(&MUNINN_PROFILE, master, db_id)?)
}

// ---------- AEAD ----------

pub fn seal(key: &Key32, plaintext: &[u8], aad: &[u8]) -> Result<Sealed> {
    Ok(weirdnet_keys::seal(key, plaintext, aad)?)
}

pub fn open(key: &Key32, s: &Sealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    Ok(weirdnet_keys::open(key, s, aad)?)
}

// ---------- password wrapping (Argon2id) ----------

/// A tag derived from a password alone (Argon2id with a fixed salt): lets a custodian find
/// the dark shards it holds for someone who lost everything but the shard password.
pub fn shard_claim(password: &SecretString) -> Result<String> {
    Ok(password::claim_tag(&MUNINN_PROFILE, password)?)
}

pub fn password_seal(password: &SecretString, plaintext: &[u8], aad: &[u8]) -> Result<PasswordSealed> {
    Ok(weirdnet_keys::password_seal(password, plaintext, aad)?)
}

pub fn password_open(password: &SecretString, s: &PasswordSealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    Ok(weirdnet_keys::password_open(password, s, aad)?)
}

// ---------- age exports ----------

pub fn age_encrypt(password: &SecretString, plaintext: &[u8]) -> Result<Vec<u8>> {
    Ok(weirdnet_keys::agefile::encrypt(password, plaintext)?)
}

pub fn age_decrypt(password: &SecretString, ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    Ok(weirdnet_keys::agefile::decrypt(password, ct)?)
}

pub fn is_age(bytes: &[u8]) -> bool {
    weirdnet_keys::agefile::is_age(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_round_trip() {
        let k = Key32::random().unwrap();
        let w = to_words(&k).unwrap();
        assert_eq!(w.split_whitespace().count(), 24);
        assert_eq!(from_words(&w.to_uppercase()).unwrap(), k);
        assert!(from_words("abandon abandon").is_err());
    }

    #[test]
    fn shards_three_of_five() {
        let k = Key32::random().unwrap();
        let s: Vec<String> = split_shards(&k).unwrap().iter().map(|z| z.to_string()).collect();
        assert_eq!(s.len(), 5);
        assert_eq!(combine_shards(&s[..3]).unwrap(), k);
        assert_eq!(combine_shards(&[s[4].clone(), s[1].clone(), s[3].clone()]).unwrap(), k);
        assert_eq!(combine_shards(&s).unwrap(), k);
        assert!(combine_shards(&s[..2]).is_err());
        assert!(combine_shards(&[s[0].clone(), s[0].clone(), s[1].clone()]).is_err());
        let other: Vec<String> = split_shards(&k).unwrap().iter().map(|z| z.to_string()).collect();
        assert!(combine_shards(&[s[0].clone(), s[1].clone(), other[2].clone()]).is_err());
        let mut bad = s[0].clone();
        bad.replace_range(30..31, if &bad[30..31] == "a" { "b" } else { "a" });
        assert!(combine_shards(&[bad, s[1].clone(), s[2].clone()]).is_err());
    }

    #[test]
    fn shards_k_of_n_and_claims() {
        let k = Key32::random().unwrap();
        let s: Vec<String> = split_shards_k(&k, 2, 3).unwrap().iter().map(|z| z.to_string()).collect();
        assert_eq!(s.len(), 3);
        assert_eq!(combine_shards(&s[1..]).unwrap(), k);
        assert!(combine_shards(&s[..1]).is_err());
        let (set, t, i) = shard_meta(&s[2]).unwrap();
        assert_eq!((set.len(), t, i), (8, 2, 3));
        assert!(split_shards_k(&k, 1, 3).is_err() && split_shards_k(&k, 4, 3).is_err());
        let p = SecretString::from("shard pass");
        let c = shard_claim(&p).unwrap();
        assert_eq!(c, shard_claim(&p).unwrap());
        assert_ne!(c, shard_claim(&SecretString::from("other")).unwrap());
        assert!(shard_claim(&SecretString::from("")).is_err());
    }

    #[test]
    fn hkdf_versions_differ() {
        let k = Key32::random().unwrap();
        let a = service_key(&k, "db", 1).unwrap();
        assert_eq!(a, service_key(&k, "db", 1).unwrap());
        assert_ne!(a, service_key(&k, "db", 2).unwrap());
        assert_ne!(a, master_kek(&k, "db").unwrap());
    }

    #[test]
    fn seal_and_password() {
        let k = Key32::random().unwrap();
        let s = seal(&k, b"hello", b"aad").unwrap();
        assert_eq!(&open(&k, &s, b"aad").unwrap()[..], b"hello");
        assert!(open(&k, &s, b"other").is_err());
        let pw = SecretString::from("pw");
        let p = password_seal(&pw, b"x", b"a").unwrap();
        assert_eq!(&password_open(&pw, &p, b"a").unwrap()[..], b"x");
        assert!(password_open(&SecretString::from("no"), &p, b"a").is_err());
    }

    #[test]
    fn age_round_trip() {
        let pw = SecretString::from("pw");
        let ct = age_encrypt(&pw, b"secret").unwrap();
        assert!(is_age(&ct));
        assert_eq!(&age_decrypt(&pw, &ct).unwrap()[..], b"secret");
        assert!(age_decrypt(&SecretString::from("bad"), &ct).is_err());
    }
}
