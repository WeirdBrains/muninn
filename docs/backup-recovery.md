# Backup and recovery

```
mnnn backup create  --out DIR [--encrypt]
mnnn backup restore --from FILE|DIR
```

## What a backup contains

`muninn-backup-<UTC timestamp>.tar.zst` (or `.tar.zst.age` with `--encrypt`):

| path | notes |
|------|-------|
| `storage/muninn.database` | SQLite online-backup copy, **still SQLCipher-encrypted** |
| `keys/` | envelope and password-wrapped service keys (already encrypted) |
| `preferences.toml` | as-is |
| `themes/` | every `<theme_name>/` folder with `interface.toml` and `assets/` |
| `manifest.json` | format, version, created_at, service-key version, BLAKE3 of every file |

`--encrypt` additionally age-encrypts the whole archive with a password
(`MUNINN_EXPORT_PASSWORD`, else `MUNINN_PASSWORD`, else a prompt). Without it,
preferences and themes inside the archive are readable, the database is not.

Key exports (master, words, shards) are **not** part of a backup; they are separate
files from `mnnn keys export` (see [keys](keys.md)).

## Restore

`--from` takes an archive, or a directory (the newest `muninn-backup-*` in it is used).

1. The archive is decrypted if needed and every file is checked against the manifest.
2. Database and `keys/`: if either already exists you are asked before overwriting.
3. `preferences.toml` is replaced.
4. Themes: each folder is restored; an existing theme with different content is only
   overwritten after confirmation. Without a TTY and without `-y` such themes are
   kept and listed as skipped.

After a restore the database opens with the password that was current when the
backup was made. If you lost it, use `mnnn keys recover` with the words or 3 shards.

## Recovery cheat sheet

| lost | recover with |
|------|--------------|
| password | `mnnn keys recover --words-file ...` or `--shard` x3 |
| master key | the 24 words, or any 3 of 5 shards |
| 24 words | re-derived from the master (`keys recover --print-words`) |
| service keys | re-derived from the master + version (`keys rotate` / `keys recover`) |
| shards | re-issued from the master (`keys shards`, a new set) |
| database / preferences / themes | `mnnn backup restore` |
| node identity and key exchanges | `mnnn net exchange restore FILE` ([key exchanges](key-exchanges.md#backups)) |
| master key, with no words or shards at hand | `mnnn net shard restore` from peers ([dark shards](dark-shards.md)) |

## Example

```
MUNINN_HOME=/tmp/mnnn-demo mnnn backup create --out ~/backups --encrypt
MUNINN_HOME=/tmp/mnnn-new  mnnn backup restore --from ~/backups -y
```

## Failure modes

- Corrupted archive: "file hashes do not match the manifest"; nothing is written.
- Overwrite without a TTY: "confirmation required ... -y".
- Wrong archive password: "could not decrypt export (wrong password?)".

See also [storage layout](storage-layout.md), [encryption](encryption.md).
