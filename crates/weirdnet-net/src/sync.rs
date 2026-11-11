//! Keys for linked-node sync: pages travel sealed under a key derived from the user's
//! signing key (which only the user's own nodes hold), on top of the exchange key. The
//! derivation and the sealing are done by `weirdnet-keys`; what is synced is the
//! application's.
use crate::error::{NetError, Result};
use crate::profile::Profile;
use libp2p::identity::Keypair;
use weirdnet_keys::{Key32, Sealed};
use zeroize::Zeroizing;

impl Profile {
    /// The sync key of a user: HKDF-SHA256 over the Ed25519 secret, salted with the user id.
    pub fn sync_key(&self, user: &Keypair, user_id: &str) -> Result<Key32> {
        let ed = user.clone().try_into_ed25519().map_err(|_| NetError::Crypto("user key is not ed25519".into()))?;
        let bytes = Zeroizing::new(ed.to_bytes());
        let ikm = Key32::from_slice(&bytes[..32])?;
        Ok(weirdnet_keys::kdf::hkdf(&ikm.0, Some(user_id.as_bytes()), self.sync_key.as_bytes())?)
    }

    /// Associated data of a sync page for `peer` (the node that asked).
    pub fn sync_aad(&self, peer: &str) -> Vec<u8> {
        format!("{}|{peer}", self.sync_aad).into_bytes()
    }

    pub fn seal_sync(&self, key: &Key32, peer: &str, plaintext: &[u8]) -> Result<Sealed> {
        Ok(weirdnet_keys::seal(key, plaintext, &self.sync_aad(peer))?)
    }

    pub fn open_sync(&self, key: &Key32, peer: &str, s: &Sealed) -> Result<Zeroizing<Vec<u8>>> {
        Ok(weirdnet_keys::open(key, s, &self.sync_aad(peer))?)
    }
}
