# weirdnet-keys

Key material and key policy, built only on vetted RustCrypto, dalek, BIP39, vsss-rs and age
crates. Nothing here invents a cipher, a KDF or a format of its own.

- `Key32`: a 256-bit key, wiped on drop, never printed.
- `words`: the 24 BIP39 words for a key. `shards`: Shamir k-of-n split and combine, with a
  checksummed text form.
- `kdf`: HKDF-SHA256 and Argon2id. `aead`: XChaCha20-Poly1305 `Sealed`. `password`: Argon2id
  password sealing and claim tags. `mac`: HMAC-SHA256 with constant-time checks.
  `exchange`: X25519 agreement (contributory check) and HKDF exchange keys.
- `keyring`: envelope encryption for a database key (DEK). The DEK is wrapped by a
  versioned service key and by a key derived from the master key; the service key is
  wrapped by a password. Init, unlock, rotate, rekey (with undo on failure), recover, verify
  and export. Files and the database come in through the `KeyFiles` and `EncryptedDatabase`
  traits the application implements; this crate never opens a database.
- `dark`: shards sealed with a shard password for custodians, and the claim tag that finds
  them again.
- `age` (default feature): password age files for exports.

## Profiles

Every label (HKDF info, associated data, shard prefix, claim salt) comes from a `Profile`.
`Profile::WEIRDNET` is the default. An application with existing data defines its own
profile with its existing labels and passes it everywhere; envelopes record their format, so
a file written under one profile is refused under another. Nothing is migrated or rewrapped
automatically.

## Limits

See [SECURITY.md](SECURITY.md) for the review notes. Not audited. The shard checksum catches typos, not forgery: a forged or mixed share combines
into a wrong key, which `verify_master` and every unwrap then reject.
