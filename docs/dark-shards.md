# Dark shards

The master key can be split among peers you exchanged keys with, so that losing every
copy of the [24 words and paper shards](keys.md) is not the end.

```
mnnn net shard entrust --to ben --to cleo --to dev --threshold 2
mnnn net shard list
mnnn net shard restore [--backup net-backup.age] [--recover] [--print-words]
mnnn net shard drop ID          # stop holding someone's shard
```

## What happens

- **Entrust.** The master key (from the words, `--words-file`, or the unlocked
  database) is split with Shamir's scheme: one shard per exchange, any *threshold*
  of them restore it (default: a majority, at least 2; the peers must be different
  nodes). Each shard is sealed with your **shard password** (Argon2id + XChaCha20,
  bound to the set and shard number) before it leaves, and travels inside the
  exchange's sealed channel. Peers store a blob they cannot open — a *dark* shard.
  Unverified exchanges ask before they are trusted with a shard.
- **Restore.** `restore` asks your peers for the shards they hold for you, opens
  them with the shard password and rebuilds the key; it checks the key against the
  database, or with `--recover` (default with `--backup`) sets a new Muninn
  password. `--print-words` shows the 24 words again.
- **From nothing.** With an [exchange backup](key-exchanges.md#backups) the node
  restores its identity and can ask its peers. Each shard is also stored under a
  *claim* derived from the shard password, so even a fresh identity can claim it.

The shard password comes from `MUNINN_SHARD_PASSWORD`, else it is asked for. Keep it
apart from the Muninn password. Nothing secret is logged; the history records only
which sets went to which exchanges (`net.shard.entrust`, `net.shard.restore`) and
the holders record `net.shard.hold`.

## In the TUI

Network ▸ Dark shards… lists the shards you entrusted and those you hold for others
(`d` drops one you hold). Entrusting and restoring need the words or the shard
password, which never pass through the TUI: `e` and `r` show the commands.
