# Encryption at rest

The database `storage/muninn.database` is a SQLCipher database (rusqlite with
`bundled-sqlcipher-vendored-openssl`; no system OpenSSL needed), keyed with a raw
256-bit data key (DEK). Without the key the file is indistinguishable from random data.

## Envelope

```
password --Argon2id--> wrapping key --XChaCha20-Poly1305--> service key v{n}   (keys/service/v{n}.toml)
service key --XChaCha20-Poly1305--> DEK                                          (keys/envelope.toml: service_wrap)
master --HKDF "muninn/kek"--> KEK --XChaCha20-Poly1305--> DEK                    (keys/envelope.toml: master_wrap)
DEK --SQLCipher--> muninn.database
```

So the database can be unlocked by the service key (normal use, via your password) or
by the master key (words, shards or exported master), and rotating the service key only
re-wraps the DEK. See [keys](keys.md) for the commands.

Key material is held in zeroize-on-drop types; passwords use `secrecy`. Secrets are never
logged; SQLCipher's own logging is turned off.

## What is and isn't encrypted

| file | protection |
|------|------------|
| `storage/muninn.database` | SQLCipher (DEK) |
| `keys/envelope.toml` | DEK only in wrapped form |
| `keys/service/v{n}.toml` | service key wrapped by the password |
| `preferences.toml`, `themes/` | plain text by design (human-editable) |

## Failure modes

- Wrong key or password: "wrong password" / "wrong key or not a muninn database".
- A database copied without its `keys/` folder cannot be opened by the password; it
  needs the master key *and* the matching envelope. Back them up together
  ([backup and recovery](backup-recovery.md)).
