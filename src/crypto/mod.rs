//! Key material: master key, BIP39 words, Shamir shards, HKDF service keys, AEAD wrapping, password exports.
pub mod keyring;

use crate::error::{Error, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
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
        let mut k = [0u8; 32];
        getrandom::getrandom(&mut k).map_err(|e| Error::Crypto(e.to_string()))?;
        Ok(Key32(k))
    }
    pub fn from_slice(b: &[u8]) -> Result<Self> {
        let a: [u8; 32] = b.try_into().map_err(|_| Error::Crypto("key must be 32 bytes".into()))?;
        Ok(Key32(a))
    }
    pub fn hex(&self) -> Zeroizing<String> {
        Zeroizing::new(hex::encode(self.0))
    }
    pub fn from_hex(s: &str) -> Result<Self> {
        let b = Zeroizing::new(hex::decode(s.trim()).map_err(|e| Error::Crypto(format!("bad key hex: {e}")))?);
        Self::from_slice(&b)
    }
}

pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(b)
}

// ---------- BIP39 ----------

/// 24 words encoding the master key's 256 bits.
pub fn to_words(master: &Key32) -> Result<Zeroizing<String>> {
    let m = bip39::Mnemonic::from_entropy(&master.0).map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(Zeroizing::new(m.to_string()))
}

pub fn from_words(words: &str) -> Result<Key32> {
    let norm = words.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let m = bip39::Mnemonic::parse(&norm).map_err(|e| Error::Crypto(format!("invalid recovery words: {e}")))?;
    if m.word_count() != 24 {
        return Err(Error::Crypto(format!("expected 24 words, got {}", m.word_count())));
    }
    let e = Zeroizing::new(m.to_entropy());
    Key32::from_slice(&e)
}

// ---------- Shamir 3-of-5 ----------

pub const SHARD_THRESHOLD: usize = 3;
pub const SHARD_COUNT: usize = 5;
const SHARD_PREFIX: &str = "muninn-shard-v1";

/// Split the master key into 5 shards (any 3 recover it). Text form:
/// `muninn-shard-v1:<set id>:<threshold>:<index>:<share hex>:<checksum>`.
pub fn split_shards(master: &Key32) -> Result<Vec<Zeroizing<String>>> {
    split_shards_k(master, SHARD_THRESHOLD, SHARD_COUNT)
}

/// Split the master key into `n` shards, any `k` of which recover it (2 <= k <= n <= 255).
pub fn split_shards_k(master: &Key32, k: usize, n: usize) -> Result<Vec<Zeroizing<String>>> {
    if k < 2 || k > n || n > 255 {
        return Err(Error::invalid("shards", format!("need 2 <= threshold <= shards <= 255 (got {k} of {n})")));
    }
    let set: [u8; 4] = random_bytes()?;
    let set = hex::encode(set);
    let shares = vsss_rs::Gf256::split_array(k, n, master.0, rand_core::OsRng).map_err(|e| Error::Crypto(format!("shard split failed: {e:?}")))?;
    let mut out = Vec::with_capacity(n);
    for (i, sh) in shares.into_iter().enumerate() {
        let sh = Zeroizing::new(sh);
        let body = format!("{SHARD_PREFIX}:{set}:{k}:{}:{}", i + 1, hex::encode(&*sh));
        let chk = &blake3::hash(body.as_bytes()).to_hex()[..8];
        out.push(Zeroizing::new(format!("{body}:{chk}")));
    }
    Ok(out)
}

/// The set id, threshold and index of a shard (no secret parts).
pub fn shard_meta(s: &str) -> Result<(String, usize, u32)> {
    let p = parse_shard(s)?;
    Ok((p.set, p.threshold, p.index))
}

struct ParsedShard {
    set: String,
    threshold: usize,
    index: u32,
    share: Zeroizing<Vec<u8>>,
}

fn parse_shard(s: &str) -> Result<ParsedShard> {
    let s = s.trim();
    let (body, chk) = s.rsplit_once(':').ok_or_else(|| Error::Crypto("malformed shard".into()))?;
    if &blake3::hash(body.as_bytes()).to_hex()[..8] != chk {
        return Err(Error::Crypto("shard checksum mismatch (typo or corrupted shard)".into()));
    }
    let p: Vec<&str> = body.split(':').collect();
    if p.len() != 5 || p[0] != SHARD_PREFIX {
        return Err(Error::Crypto("malformed shard".into()));
    }
    Ok(ParsedShard {
        set: p[1].to_string(),
        threshold: p[2].parse().map_err(|_| Error::Crypto("malformed shard threshold".into()))?,
        index: p[3].parse().map_err(|_| Error::Crypto("malformed shard index".into()))?,
        share: Zeroizing::new(hex::decode(p[4]).map_err(|_| Error::Crypto("malformed shard data".into()))?),
    })
}

/// Combine at least 3 distinct shards from the same set.
pub fn combine_shards(shards: &[String]) -> Result<Key32> {
    let parsed: Vec<ParsedShard> = shards.iter().map(|s| parse_shard(s)).collect::<Result<_>>()?;
    let first = parsed.first().ok_or_else(|| Error::Crypto("no shards given".into()))?;
    if parsed.iter().any(|p| p.set != first.set) {
        return Err(Error::Crypto("shards come from different sets and cannot be mixed".into()));
    }
    let mut idx: Vec<u32> = parsed.iter().map(|p| p.index).collect();
    idx.sort_unstable();
    idx.dedup();
    if idx.len() < first.threshold {
        return Err(Error::Crypto(format!("need at least {} distinct shards, got {}", first.threshold, idx.len())));
    }
    let mut seen = std::collections::HashSet::new();
    let shares: Vec<Vec<u8>> = parsed.iter().filter(|p| seen.insert(p.index)).map(|p| p.share.to_vec()).collect();
    let secret = Zeroizing::new(vsss_rs::Gf256::combine_array(&shares).map_err(|e| Error::Crypto(format!("shard combine failed: {e:?}")))?);
    let mut shares = shares;
    for s in shares.iter_mut() {
        s.zeroize();
    }
    Key32::from_slice(&secret)
}

// ---------- HKDF ----------

pub(crate) fn hkdf(master: &Key32, salt: &[u8], info: &str) -> Result<Key32> {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), &master.0);
    let mut out = [0u8; 32];
    hk.expand(info.as_bytes(), &mut out).map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(Key32(out))
}

/// `HKDF-SHA256(master, salt = db_id, info = "muninn/service/v{n}")`.
pub fn service_key(master: &Key32, db_id: &str, version: u32) -> Result<Key32> {
    hkdf(master, db_id.as_bytes(), &format!("muninn/service/v{version}"))
}

/// Master-derived key-encryption key.
pub fn master_kek(master: &Key32, db_id: &str) -> Result<Key32> {
    hkdf(master, db_id.as_bytes(), "muninn/kek")
}

// ---------- AEAD ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sealed {
    pub nonce: String,
    pub ct: String,
}

pub fn seal(key: &Key32, plaintext: &[u8], aad: &[u8]) -> Result<Sealed> {
    let c = XChaCha20Poly1305::new((&key.0).into());
    let nonce: [u8; 24] = random_bytes()?;
    let ct = c.encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad }).map_err(|_| Error::Crypto("encryption failed".into()))?;
    Ok(Sealed { nonce: hex::encode(nonce), ct: hex::encode(ct) })
}

pub fn open(key: &Key32, s: &Sealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let c = XChaCha20Poly1305::new((&key.0).into());
    let nonce = hex::decode(&s.nonce).map_err(|_| Error::Crypto("bad nonce".into()))?;
    if nonce.len() != 24 {
        return Err(Error::Crypto("bad nonce".into()));
    }
    let ct = hex::decode(&s.ct).map_err(|_| Error::Crypto("bad ciphertext".into()))?;
    c.decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad }).map(Zeroizing::new).map_err(|_| Error::Unlock("wrong key or password".into()))
}

// ---------- password wrapping (Argon2id) ----------

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

fn argon2_key(password: &SecretString, salt: &[u8], m: u32, t: u32, p: u32) -> Result<Key32> {
    let params = argon2::Params::new(m, t, p, Some(32)).map_err(|e| Error::Crypto(e.to_string()))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 32];
    a.hash_password_into(password.expose_secret().as_bytes(), salt, &mut out).map_err(|e| Error::Crypto(e.to_string()))?;
    Ok(Key32(out))
}

/// A tag derived from a password alone (Argon2id with a fixed salt): lets a custodian find
/// the dark shards it holds for someone who lost everything but the shard password.
pub fn shard_claim(password: &SecretString) -> Result<String> {
    if password.expose_secret().is_empty() {
        return Err(Error::invalid("password", "must not be empty"));
    }
    let k = argon2_key(password, b"muninn/net/shard-claim/v1", 19 * 1024, 2, 1)?;
    Ok(hex::encode(blake3::keyed_hash(&k.0, b"claim").as_bytes()))
}

pub fn password_seal(password: &SecretString, plaintext: &[u8], aad: &[u8]) -> Result<PasswordSealed> {
    if password.expose_secret().is_empty() {
        return Err(Error::invalid("password", "must not be empty"));
    }
    let salt: [u8; 16] = random_bytes()?;
    let (m, t, p) = (19 * 1024, 2, 1);
    let k = argon2_key(password, &salt, m, t, p)?;
    Ok(PasswordSealed { kdf: "argon2id".into(), salt: hex::encode(salt), m_cost: m, t_cost: t, p_cost: p, sealed: seal(&k, plaintext, aad)? })
}

pub fn password_open(password: &SecretString, s: &PasswordSealed, aad: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let salt = hex::decode(&s.salt).map_err(|_| Error::Crypto("bad salt".into()))?;
    let k = argon2_key(password, &salt, s.m_cost, s.t_cost, s.p_cost)?;
    open(&k, &s.sealed, aad).map_err(|_| Error::Unlock("wrong password".into()))
}

// ---------- age exports ----------

const AGE_WORK_FACTOR: u8 = 16;

pub fn age_encrypt(password: &SecretString, plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut r = age::scrypt::Recipient::new(password.clone());
    r.set_work_factor(AGE_WORK_FACTOR);
    age::encrypt(&r, plaintext).map_err(|e| Error::Crypto(format!("age: {e}")))
}

pub fn age_decrypt(password: &SecretString, ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let id = age::scrypt::Identity::new(password.clone());
    age::decrypt(&id, ct).map(Zeroizing::new).map_err(|e| Error::Unlock(format!("could not decrypt export (wrong password?): {e}")))
}

pub fn is_age(bytes: &[u8]) -> bool {
    bytes.starts_with(b"age-encryption.org/")
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
