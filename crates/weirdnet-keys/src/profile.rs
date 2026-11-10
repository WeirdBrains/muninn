//! Label profiles: every domain label a key operation binds to.

/// The labels key operations bind to. Two profiles with different labels derive different
/// keys from the same material, and an envelope written under one is refused by the other.
///
/// [`Profile::WEIRDNET`] is the default for new data. An application with existing data
/// defines its own profile holding its existing labels and passes it explicitly; nothing is
/// converted between profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// Recorded in new envelopes (unless `None`, which keeps envelopes that never had one).
    pub name: Option<&'static str>,
    /// The envelope `format` this profile writes and accepts.
    pub envelope_format: u32,
    /// Prefix for HKDF infos and associated data: `{label}/service/v{n}`, `{label}/kek`,
    /// `{label}/dek` and `{label}/service-key/{db id}/v{n}`.
    pub label: &'static str,
    /// The first field of a shard's text form.
    pub shard_prefix: &'static str,
    /// The fixed Argon2id salt of password claim tags.
    pub claim_salt: &'static [u8],
    /// Associated data of a sealed dark shard: `{dark_shard_label}|{set}|{index}`.
    pub dark_shard_label: &'static str,
}

impl Profile {
    pub const WEIRDNET: Profile = Profile {
        name: Some("weirdnet/keys/v1"),
        envelope_format: 2,
        label: "weirdnet/keys/v1",
        shard_prefix: "weirdnet-shard-v1",
        claim_salt: b"weirdnet/keys/v1/shard-claim",
        dark_shard_label: "weirdnet/keys/v1/dark-shard",
    };

    pub fn service_info(&self, version: u32) -> String {
        format!("{}/service/v{version}", self.label)
    }
    pub fn kek_info(&self) -> String {
        format!("{}/kek", self.label)
    }
    pub fn dek_aad(&self) -> Vec<u8> {
        format!("{}/dek", self.label).into_bytes()
    }
    pub fn service_key_aad(&self, db_id: &str, version: u32) -> Vec<u8> {
        format!("{}/service-key/{db_id}/v{version}", self.label).into_bytes()
    }
    pub fn dark_shard_aad(&self, set: &str, index: u32) -> Vec<u8> {
        format!("{}|{set}|{index}", self.dark_shard_label).into_bytes()
    }
}

impl Default for Profile {
    fn default() -> Self {
        Profile::WEIRDNET
    }
}
