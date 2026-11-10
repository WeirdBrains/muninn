# weirdnet-storage

General storage for SQLite databases encrypted with SQLCipher. The crate knows no schema
and no brand: callers bring their migrations, table registry and queries.

- `open`, `open_readonly`, `open_in_memory`: open with a raw 32-byte key the caller hands in,
  verify it (wrong key is `StorageError::WrongKey`), private folder (0700) and file (0600).
- `rekey`: re-encrypt in place; `encrypt_plaintext_in_place`: convert a plaintext file,
  verified before it replaces the original.
- `migrate`: an ordered list of migrations in one transaction, with a hook after each one.
- `savepoint`, `begin`/`release`/`rollback`: all-or-nothing units that nest.
- `files`: private folders, 0600 files written through a side file and renamed into place.
- `snapshot_to`: an online copy of an open database, keyed the same way.
- `sync` (default feature): a change log kept by triggers on the tables you register, row
  read and write by primary key, a row codec and hash, and the three-way merge decision.

## Keys

The crate never derives, wraps, stores, recovers or exports a key, and never sees a
password. It applies the raw key it is given (`PRAGMA key`/`rekey`) and zeroizes the
literals it builds. Key management belongs to the caller (for example `weirdnet-keys`).
Opening with a key fails with `StorageError::NoCipher` if SQLite was built without SQLCipher,
so a key is never silently ignored.

## Features

- `sync` (default): the change-log module; adds serde, serde_json, sha2 and base64.

SQLCipher is always bundled with vendored OpenSSL (`rusqlite/bundled-sqlcipher-vendored-openssl`).
