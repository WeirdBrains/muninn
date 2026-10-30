# Storage layout

```
~/.muninn/
  storage/
    muninn.database       # SQLCipher-encrypted SQLite (bars, authors, history, projects, journals, models and records)
  keys/
    envelope.toml         # wrapped data key + service-key version
    service/              # password-wrapped service key per version
  themes/
    <theme_name>/
      interface.toml      # palette + role mapping, [dark] and [light]
      assets/             # optional extras
  preferences.toml        # active theme and mode, ingest defaults
  prints/                 # PDFs made by `journal print` (and P in the TUI)
```

- The root is `~/.muninn` unless `--home <DIR>` or `MUNINN_HOME` is set. Tests and
  examples always use a temporary home.
- `--db <PATH>` / `MUNINN_DB` points at a different database file; keys still come
  from `<home>/keys/`.
- Directories are created with mode `0700`, files with `0600`.
- Built-in themes are written to `themes/<name>/` on first use if missing.
- `keys/` never holds the master key, the 24 words or the shards. See [keys](keys.md).

Everything here is included in a backup; see [backup and recovery](backup-recovery.md).
Preferences are described in [preferences](preferences.md), themes in [themes](themes.md).
