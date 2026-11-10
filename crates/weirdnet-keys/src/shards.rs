//! Shamir k-of-n secret sharing of a 256-bit key (vsss-rs over GF(256)).
//!
//! Text form: `{prefix}:{set id}:{threshold}:{index}:{share hex}:{checksum}`, where the
//! checksum is the first 8 hex digits of BLAKE3 over the rest. The checksum catches typos;
//! it is not a MAC. A forged or foreign share combines into a wrong key, which the caller
//! must reject by unwrapping something with it (see `keyring::verify_master`).
use crate::error::{KeysError, Result};
use crate::key::{Key32, random_bytes};
use crate::profile::Profile;
use zeroize::{Zeroize, Zeroizing};

/// The default split: any 3 of 5.
pub const THRESHOLD: usize = 3;
pub const COUNT: usize = 5;

/// Split `master` into `n` shards, any `k` of which recover it (2 <= k <= n <= 255).
pub fn split(profile: &Profile, master: &Key32, k: usize, n: usize) -> Result<Vec<Zeroizing<String>>> {
    if k < 2 || k > n || n > 255 {
        return Err(KeysError::invalid("shards", format!("need 2 <= threshold <= shards <= 255 (got {k} of {n})")));
    }
    let set = hex::encode(random_bytes::<4>()?);
    let shares =
        vsss_rs::Gf256::split_array(k, n, master.0, rand_core::OsRng).map_err(|e| KeysError::crypto(format!("shard split failed: {e:?}")))?;
    let mut out = Vec::with_capacity(n);
    for (i, sh) in shares.into_iter().enumerate() {
        let sh = Zeroizing::new(sh);
        let share_hex = Zeroizing::new(hex::encode(&*sh));
        let body = Zeroizing::new(format!("{}:{set}:{k}:{}:{}", profile.shard_prefix, i + 1, share_hex.as_str()));
        let chk = &blake3::hash(body.as_bytes()).to_hex()[..8];
        out.push(Zeroizing::new(format!("{}:{chk}", body.as_str())));
    }
    Ok(out)
}

/// The set id, threshold and index of a shard (no secret parts).
pub fn meta(profile: &Profile, s: &str) -> Result<(String, usize, u32)> {
    let p = parse(profile, s)?;
    Ok((p.set, p.threshold, p.index))
}

struct Parsed {
    set: String,
    threshold: usize,
    index: u32,
    share: Zeroizing<Vec<u8>>,
}

fn malformed(what: &str) -> KeysError {
    KeysError::crypto(format!("malformed shard{what}"))
}

fn parse(profile: &Profile, s: &str) -> Result<Parsed> {
    let s = s.trim();
    let (body, chk) = s.rsplit_once(':').ok_or_else(|| malformed(""))?;
    if &blake3::hash(body.as_bytes()).to_hex()[..8] != chk {
        return Err(KeysError::crypto("shard checksum mismatch (typo or corrupted shard)"));
    }
    let p: Vec<&str> = body.split(':').collect();
    if p.len() != 5 || p[0] != profile.shard_prefix {
        return Err(malformed(""));
    }
    let threshold: usize = p[2].parse().map_err(|_| malformed(" threshold"))?;
    let index: u32 = p[3].parse().map_err(|_| malformed(" index"))?;
    if !(2..=255).contains(&threshold) {
        return Err(malformed(" threshold"));
    }
    let share = Zeroizing::new(hex::decode(p[4]).map_err(|_| malformed(" data"))?);
    // A GF(256) share is its identifier byte followed by one byte per secret byte.
    if !(1..=255).contains(&index) || share.len() != 33 || u32::from(share[0]) != index {
        return Err(malformed(" data"));
    }
    Ok(Parsed { set: p[1].to_string(), threshold, index, share })
}

/// Combine at least `threshold` distinct shards of one set made under `profile`.
pub fn combine(profile: &Profile, shards: &[String]) -> Result<Key32> {
    let parsed: Vec<Parsed> = shards.iter().map(|s| parse(profile, s)).collect::<Result<_>>()?;
    let first = parsed.first().ok_or_else(|| KeysError::crypto("no shards given"))?;
    if parsed.iter().any(|p| p.set != first.set) {
        return Err(KeysError::crypto("shards come from different sets and cannot be mixed"));
    }
    if parsed.iter().any(|p| p.threshold != first.threshold) {
        return Err(KeysError::crypto("shards disagree on their threshold"));
    }
    let mut seen = std::collections::HashMap::new();
    for p in &parsed {
        if let Some(prev) = seen.insert(p.index, &p.share)
            && prev[..] != p.share[..]
        {
            return Err(KeysError::crypto("two different shards carry the same index"));
        }
    }
    if seen.len() < first.threshold {
        return Err(KeysError::crypto(format!("need at least {} distinct shards, got {}", first.threshold, seen.len())));
    }
    let mut shares: Vec<Vec<u8>> = seen.values().map(|s| s.to_vec()).collect();
    let secret = vsss_rs::Gf256::combine_array(&shares).map(Zeroizing::new);
    for s in shares.iter_mut() {
        s.zeroize();
    }
    let secret = secret.map_err(|e| KeysError::crypto(format!("shard combine failed: {e:?}")))?;
    Key32::from_slice(&secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[Zeroizing<String>], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| v[i].to_string()).collect()
    }

    #[test]
    fn any_k_recover_fewer_do_not() {
        let w = Profile::WEIRDNET;
        let m = Key32::random().unwrap();
        let sh = split(&w, &m, THRESHOLD, COUNT).unwrap();
        assert!(sh.iter().all(|x| x.starts_with("weirdnet-shard-v1:")));
        for idx in [[0, 1, 2], [0, 2, 4], [4, 3, 1]] {
            assert_eq!(combine(&w, &s(&sh, &idx)).unwrap(), m);
        }
        assert_eq!(combine(&w, &s(&sh, &[0, 1, 2, 3, 4])).unwrap(), m);
        assert!(combine(&w, &s(&sh, &[0, 1])).is_err());
        assert!(combine(&w, &s(&sh, &[0, 1, 1])).is_err());
        assert!(combine(&w, &[]).is_err());
        assert_eq!(meta(&w, &sh[3]).unwrap().1..meta(&w, &sh[3]).unwrap().2 as usize, 3..4);
        let k2 = split(&w, &m, 2, 2).unwrap();
        assert_eq!(combine(&w, &s(&k2, &[1, 0])).unwrap(), m);
        assert!(split(&w, &m, 1, 3).is_err() && split(&w, &m, 4, 3).is_err() && split(&w, &m, 2, 256).is_err());
    }

    #[test]
    fn tampered_mixed_and_foreign_shards_fail() {
        let w = Profile::WEIRDNET;
        let m = Key32::random().unwrap();
        let a = split(&w, &m, 3, 5).unwrap();
        let b = split(&w, &m, 3, 5).unwrap();
        assert!(combine(&w, &[a[0].to_string(), a[1].to_string(), b[2].to_string()]).is_err());
        // one flipped hex digit fails the checksum
        let mut t = a[0].to_string();
        let i = t.len() - 12;
        let c = if &t[i..=i] == "0" { "1" } else { "0" };
        t.replace_range(i..=i, c);
        assert!(combine(&w, &[t, a[1].to_string(), a[2].to_string()]).is_err());
        // a forged share with a valid checksum combines into a wrong key, never the right one
        let (body, _) = a[0].rsplit_once(':').unwrap();
        let mut f: Vec<&str> = body.split(':').collect();
        let forged_share = format!("01{}", "ab".repeat(32));
        f[4] = &forged_share;
        let fb = f.join(":");
        let forged = format!("{fb}:{}", &blake3::hash(fb.as_bytes()).to_hex()[..8]);
        assert_ne!(combine(&w, &[forged, a[1].to_string(), a[2].to_string()]).unwrap(), m);
        // a lowered threshold on one shard is refused
        let (body, _) = a[1].rsplit_once(':').unwrap();
        let lb = body.replacen(":3:", ":2:", 1);
        let lowered = format!("{lb}:{}", &blake3::hash(lb.as_bytes()).to_hex()[..8]);
        assert!(combine(&w, &[a[0].to_string(), lowered, a[2].to_string()]).is_err());
        // another profile's shards are refused outright
        let other = Profile { shard_prefix: "other-shard-v1", ..w };
        assert!(combine(&other, &s(&a, &[0, 1, 2])).is_err());
        assert!(meta(&other, &a[0]).is_err());
    }
}
