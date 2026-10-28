# Muninn documentation

Muninn (`mnnn`) ingests crypto market data from files and services, normalizes it into
bars and stores them in an encrypted SQLite database. These pages are also the in-app
help (`?` or F1 in the TUI, `mnnn help <topic>` on the command line).

## Using Muninn

- [Command line](cli.md): global flags, environment, exit codes, man pages
- [Terminal UI](tui.md): header, menus, content area
- [Menus](menus.md) and [keybindings](keybindings.md)
- [Tiling layout](tiling.md): splitting tiles, list tiles, presets, linking
- [Charts](charts.md): period, zoom, layers
- [Sidebar](sidebar.md): search, sort and filter series
- [In-app help](help.md): searching and following links

## Getting data in and out

- [CSV ingest](ingest-csv.md)
- [JSON ingest](ingest-json.md)
- Services: [HTTP](services/http.md), [Redis](services/redis.md), [SQLite](services/sqlite.md), [Postgres](services/postgres.md)
- [Export](export.md)

## Data and storage

- [Data model](data-model.md): bars, sources, symbols, bar ids
- [Storage layout](storage-layout.md): the `~/.muninn` folder
- [Preferences](preferences.md)

## Security

- [Keys](keys.md): master key, 24 words, shards, service keys, rotation, rekey
- [Encryption at rest](encryption.md)
- [Backup and recovery](backup-recovery.md)

## Look and feel

- [Themes](themes.md): built-ins, light/dark/system modes, selector and editor

## Quick start

```
export MUNINN_HOME=/tmp/mnnn-demo          # never touches ~/.muninn
mnnn keys init                             # write down the words and shards
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data --source binance -y
mnnn export --format parquet --out /tmp/btc.parquet --symbol BTC/USDT
mnnn tui
```
