//! Dark shards: shards sealed under a shard password for custodians to hold, plus the claim
//! tag that finds them again. Carrying them between nodes is the protocol's job.
use crate::error::{KeysError, Result};
use crate::key::Key32;
use crate::password::{PasswordSealed, claim_tag, password_open, password_seal};
use crate::profile::Profile;
use crate::shards;
use secrecy::SecretString;
use zeroize::Zeroizing;

/// One sealed shard and its public metadata.
#[derive(Debug, Clone)]
pub struct DarkShard {
    pub set: String,
    pub index: u32,
    pub threshold: usize,
    pub sealed: PasswordSealed,
}

/// Split `master` k-of-n and seal each shard under `password`; returns the claim tag too.
pub fn seal(profile: &Profile, master: &Key32, password: &SecretString, k: usize, n: usize) -> Result<(String, Vec<DarkShard>)> {
    let texts = shards::split(profile, master, k, n)?;
    let claim = claim_tag(profile, password)?;
    let mut out = Vec::with_capacity(n);
    for t in &texts {
        let (set, threshold, index) = shards::meta(profile, t)?;
        let sealed = password_seal(password, t.as_bytes(), &profile.dark_shard_aad(&set, index))?;
        out.push(DarkShard { set, index, threshold, sealed });
    }
    Ok((claim, out))
}

/// Opens one sealed shard held as `set`/`index`. A wrong password, a tampered seal or a
/// seal moved to another set or index gives `Unlock("wrong shard password")`; a seal whose
/// content disagrees with its set or index is refused.
pub fn open(profile: &Profile, password: &SecretString, set: &str, index: u32, sealed: &PasswordSealed) -> Result<Zeroizing<String>> {
    let plain = password_open(password, sealed, &profile.dark_shard_aad(set, index)).map_err(|_| KeysError::Unlock("wrong shard password".into()))?;
    let text = Zeroizing::new(String::from_utf8(plain.to_vec()).map_err(|_| KeysError::crypto("malformed shard"))?);
    let (s, _, i) = shards::meta(profile, &text)?;
    if s != set || i != index {
        return Err(KeysError::crypto("the shard does not match its set and index"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_combine_and_reject() {
        let w = Profile::WEIRDNET;
        let m = Key32::random().unwrap();
        let pw = SecretString::from("shard pw");
        let (claim, ds) = seal(&w, &m, &pw, 2, 3).unwrap();
        assert_eq!(claim, claim_tag(&w, &pw).unwrap());
        let texts: Vec<String> = ds[1..].iter().map(|d| open(&w, &pw, &d.set, d.index, &d.sealed).unwrap().to_string()).collect();
        assert_eq!(shards::combine(&w, &texts).unwrap(), m);
        let d = &ds[0];
        assert!(matches!(open(&w, &SecretString::from("bad"), &d.set, d.index, &d.sealed), Err(KeysError::Unlock(_))));
        assert!(matches!(open(&w, &pw, &d.set, d.index + 1, &d.sealed), Err(KeysError::Unlock(_))));
        let other = Profile { dark_shard_label: "other/dark", ..w };
        assert!(matches!(open(&other, &pw, &d.set, d.index, &d.sealed), Err(KeysError::Unlock(_))));
    }
}
