# Muninn 1.0

**Your market data, on your disk, encrypted, in one place.**

Muninn (`mnnn`) pulls OHLCV bars from exchange dumps and APIs, normalizes them into one
schema and keeps them in an encrypted SQLite database you own. No cloud, no account, no
lock-in: ingest from anywhere, export to anything, browse it all in a fast terminal UI.

## Key features

- **Ingest anything:** CSV and JSON files or folders, HTTP APIs (with pagination),
  Redis, SQLite and Postgres. Source, symbol and bar size are taken from flags or the
  file name. Unchanged files are skipped by content hash; re-imports never duplicate.
- **One clean model:** every bar keyed by a deterministic BLAKE3 id; gaps, zero-volume
  bars and rejects are reported, not hidden. A million-row file ingests in ~40 s.
- **Encrypted by default:** SQLCipher with envelope keys, a 24-word recovery phrase,
  3-of-5 recovery shards, and keys that never sit on disk in the clear.
- **Backups that restore:** one command archives database, keys, preferences and
  themes, optionally age-encrypted.
- **Export:** CSV, Parquet or JSON, filtered by source, symbol, range and bar size.
- **A real TUI:** menu bar, i3-style tiling, list and chart tiles with toggleable
  layers, a filterable sidebar, mouse support everywhere, built-in searchable help.
- **Safe imports:** a folder-browser import popup with Places, zsh-style Tab
  completion, live progress, cancel at any point, and a durable run ledger that
  records and cleans up every cancelled import (`mnnn ingest runs`).
- **Themes:** Tokyo Night, Catppuccin and more, each with light and dark variants that
  follow your system.
- **Documented:** feature docs, in-app help and 40 man pages generated from one source.
