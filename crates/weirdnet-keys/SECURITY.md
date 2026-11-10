# weirdnet-keys security notes

Not audited. These notes record the review done when the crate was extracted from Muninn
(Phase 3), what it checked, and what it did not.

## Primitives (vetted crates only)

| Need | Construction | Crate |
| ---- | ---- | ---- |
| Random keys, salts, nonces | OS generator | `getrandom` 0.2, `rand_core::OsRng` |
| Sealing | XChaCha20-Poly1305, random 192-bit nonce per message, caller's associated data | `chacha20poly1305` 0.10 |
| Key derivation | HKDF-SHA256 (RFC 5869 vector tested) | `hkdf` 0.12, `sha2` |
| Password keys | Argon2id v1.3, 19 MiB, t=2, p=1, 16-byte random salt | `argon2` 0.5 |
| Recovery words | BIP39, 24 words, checksum checked | `bip39` 2 |
| Shards | Shamir over GF(256) | `vsss-rs` 5 |
| Agreement | X25519 with the contributory check | `x25519-dalek` 2 |
| MACs | HMAC-SHA256, constant-time verify (RFC 4231 vector tested) | `hmac` 0.12 |
| Password files | age scrypt recipient, work factor 2^16 | `age` 0.11 (feature `age`) |
| Shard checksum, claim tag | BLAKE3 | `blake3` 1 |

No cipher, KDF or MAC is implemented here.

## What was checked

- Authenticity: every unwrap is AEAD-authenticated with associated data that binds the
  purpose (`{label}/dek`, `{label}/service-key/{db id}/v{n}`, `{dark label}|{set}|{index}`).
  A wrong key, wrong password, moved file (other db id, version, set or index) or any
  flipped byte gives the same `Unlock` error; tests flip ciphertext, nonce and salt bytes.
- Domain separation: every label comes from a `Profile`. Tests show a Weirdnet home is
  refused by a legacy profile (format check), that a profile with the same format but a
  different label fails every unwrap, that shards of one profile do not parse under
  another, and that claim tags and dark-shard seals differ by profile.
- Secrets in errors and logs: no error carries key bytes, a password, words, a share or
  plaintext. `Key32` and `ExchangeSecret` print as `<redacted>`.
- Zeroization: keys (`Key32`, `ExchangeSecret`) wipe on drop; words, shard texts and
  their intermediate strings, decoded shares, hex keys and decrypted payloads are held in
  `Zeroizing`. Limits: copies made inside dependencies, by the allocator on growth, or by
  callers who `to_string()` a secret are not controlled here.
- Nonces and randomness: a fresh random 24-byte nonce per seal (safe for random nonces
  at any realistic volume); salts are random per seal; the claim tag's salt is fixed by
  design (it must be derivable from the password alone), so anyone holding a claim tag can
  test password guesses at Argon2id cost.
- Recovery thresholds: k-of-n with 2 <= k <= n <= 255; fewer than k distinct shards are
  refused; mixed sets are refused. The 32-bit shard checksum is unkeyed and catches typos
  only. A forged share with a valid checksum combines into a wrong key, which
  `keyring::recover_master` and `verify_master` reject by unwrapping the stored DEK.
- Failure paths: a failed `init` removes the database it created and its v1 service
  file; a failed `rotate` leaves the old version in force; a `rekey` whose envelope cannot
  be saved rekeys the database back (tested on the fake home and, in Muninn, on a real
  SQLCipher database); a failed `export` removes only the files it created.
- Provider boundary: the keyring touches files only through `KeyFiles` (read, list,
  write, remove) and the database only through `EncryptedDatabase`; it never opens SQLite.
  `read_maybe_age` reads the path it is given directly.

## Inputs now refused (intentionally)

These reject only malformed or hostile input; every file Muninn wrote before the
extraction passes them (its golden fixtures are tested):

- Password seals whose `kdf` is not `argon2id`, or whose Argon2 cost exceeds
  m = 2 GiB, t = 64 or p = 16 (a hostile file could otherwise demand unbounded work).
  Writers have always used 19 MiB, t=2, p=1.
- Shards whose threshold is outside 2..=255, whose index is outside 1..=255 or disagrees
  with the share's own identifier byte, whose share is not 33 bytes, whose thresholds
  disagree within one set, or two different shares with the same index.
- Dark shards whose content names a different set or index than they were stored under.
- Envelopes whose `format`/`profile` do not match the application's profile.

## Known risks and limits

- Crash atomicity of `rekey`: between re-encrypting the database and saving the new
  envelope, a crash (not an error) leaves the database on a DEK that no stored file wraps.
  The undo covers errors, not power loss or a killed process. Back up before rekeying.
- `rotate` writes the new service file, then the envelope, then retires the old file; a
  crash between steps leaves a working home (old or new version), possibly with an
  unretired old service file still wrapped by the password.
- Exports replace an existing file of the same name; on a later failure that file keeps
  the new (encrypted) content.
- File permissions, atomic replacement and fsync are the application's `write_private`.
  On Windows there are no POSIX modes. Only Linux was exercised.
- No side-channel analysis beyond relying on the dependencies' constant-time claims; no
  fuzzing; no formal review of vsss-rs or age.
