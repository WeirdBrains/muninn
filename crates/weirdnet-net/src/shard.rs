//! Dark-shard blobs as custodians keep and return them. The blob is sealed by
//! `weirdnet-keys` (`dark::seal`) before it reaches the network; nodes only carry it and
//! never see what is inside. Splitting, sealing, claim tags and recovery are the keys
//! crate's; which custodians to trust is the application's.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShardBlob {
    pub set: String,
    pub index: u32,
    pub threshold: u32,
    pub total: u32,
    /// Opaque sealed text.
    pub blob: String,
}
