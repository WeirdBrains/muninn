# Keys

```
mnnn keys init [--json]
mnnn keys status
mnnn keys rotate  [MASTER INPUT]
mnnn keys rekey   [MASTER INPUT]
mnnn keys export  --out DIR [--only master,words,service,shards] [MASTER INPUT]
mnnn keys recover [MASTER INPUT] [--print-words]
mnnn keys shards  [--out DIR] [MASTER INPUT]

MASTER INPUT: --words-file FILE | --shard FILE --shard FILE --shard FILE | --master-file FILE | --words "..."
```

## The pieces

| item | what it is | stored? |
|------|------------|---------|
| master key | 256 random bits from the OS | **never** |
| 24 words | BIP39 encoding of the same 256 bits | **never** |
| dark shards | Shamir 3-of-5 split of the master key | **never** |
| service key v{n} | `HKDF-SHA256(master, salt = db_id, info = "muninn/service/v{n}")` | `keys/service/v{n}.toml`, wrapped by your password (Argon2id + XChaCha20-Poly1305) |
| data key (DEK) | random 256-bit SQLCipher key | `keys/envelope.toml`, wrapped by the service key **and** by a master-derived KEK |

The password is **not** mixed into the master key: the 24 words (or any 3 shards)
alone recover it. The password only protects key material on disk and in exports.

## init

Creates the master key, data key, envelope and service key v1, and creates the
encrypted database (an existing plaintext database is encrypted in place). It prints
the 24 words and the 5 shards **once**. Write the words down and store the shards in
different places. `--json` prints `{db_id, words, shards}` for scripting.

The password comes from `MUNINN_PASSWORD` or a hidden prompt (asked twice).

## Normal use

Every database command unlocks the data key with the password-wrapped service key.
Set `MUNINN_PASSWORD` for scripts, or type it at the prompt.

## rotate

Derives service key `v{n+1}` from the master, re-wraps the data key, and retires the
old service key (its wrapped copy is deleted). Instant; the database is not rewritten.
Needs the master key and asks for confirmation (`-y`).

## rekey

Generates a new data key and runs SQLCipher `PRAGMA rekey` (rewrites the whole file),
then re-wraps it for the service key and the master. Use it if you suspect the data
key leaked. Needs the master key.

## recover

From the words, 3 shards or an exported master file: verifies the master against the
envelope, then re-wraps the current service key with a (new) password. Use this when
the password is lost. `--print-words` shows the 24 words again.

## shards

Re-issues a new 3-of-5 set. A new set cannot be mixed with shards from an older set
(each shard carries a set id and a checksum). Older sets still recover the same master
key, so destroy them if they may be compromised. Without `--out` the shards are
printed; with `--out` they are written as `shard-N.age`.

Shards can also be entrusted to peers over the network, sealed so they cannot read
them: see [dark shards](dark-shards.md).

Shard text format: `muninn-shard-v1:<set>:<threshold>:<index>:<hex>:<checksum>`.

## export

Writes each item as its own password-encrypted [age](https://age-encryption.org) file
so they can live in different places:
`master.key.age`, `words.txt.age`, `service-keys.json.age`, `shard-1.age` ... `shard-5.age`.
The export password is `MUNINN_EXPORT_PASSWORD`, else `MUNINN_PASSWORD`, else a prompt.
Writing the master/words and the shards to one directory triggers a warning and needs
confirmation. Exported files are accepted back by `--words-file`, `--shard` and
`--master-file` (age files are detected and decrypted).

## Example

```
export MUNINN_HOME=/tmp/mnnn-demo MUNINN_PASSWORD='a long passphrase'
mnnn keys init
mnnn keys export --out /media/usb --only shards
mnnn keys rotate --words-file ~/words.txt -y
mnnn keys recover --shard a.age --shard b.age --shard c.age
```

## Failure modes

- Commands before `keys init`: "keys are not initialized ... run `mnnn keys init`".
- Wrong password: "wrong password"; nothing is changed.
- Fewer than 3 shards, shards from different sets, or a typo: a clear error; a typo is
  caught by the checksum.
- Words or shards from another database: "master key does not match this database".

See [encryption](encryption.md) and [backup and recovery](backup-recovery.md).
