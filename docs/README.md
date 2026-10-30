# Muninn documentation

Muninn (`mnnn`) ingests crypto market data from files and services, normalizes it into
bars and stores them in an encrypted SQLite database. These pages are also the in-app
help (`?` or F1 in the TUI, `mnnn help <topic>` on the command line).

## Using Muninn

- [Install](install.md): packages per platform, building from source, uninstalling
- [Releasing](release.md): release script and the GitHub Actions release workflow
- [Command line](cli.md): global flags, environment, exit codes, man pages
- [Terminal UI](tui.md): header, menus, content area
- [Menus](menus.md) and [keybindings](keybindings.md)
- [Tiling layout](tiling.md): splitting tiles, list tiles, presets, linking
- [Charts](charts.md): period, zoom, layers
- [Sidebar](sidebar.md): search, sort and filter series
- [Import](import.md): file browser and background import
- [In-app help](help.md): searching and following links
- [Authors](authors.md): who actions are attributed to, the current author
- [Action history](history.md): the history bar, stepping back and forward, forks

## Research

- [Live searches](live-searches.md): saved searches that always run on current data; fork and optimize
- [Projects](projects.md): live searches, results and journals together
- [Journals](journals.md): Markdown or LaTeX entries with images, notes and highlights
- [Reports](reports.md): sections built from records, journals, projects and authors
- [Rules](rules.md): when an action happens, run searches, export, write entries or notes, feed models
- [Models](models.md): design record tables, migrate them with previews and roll back; records
- [Code spaces](code-spaces.md): math, Polars, Rust or Python compiled to native plugins over your data
- [Export formats](export-formats.md): PDF, HTML, Word, LaTeX, JSON, CSV; printing; project archives and import

## Network

- [Network](network.md): the node, peers, relays, sharing and searching across peers; the Network menu
- [Key exchanges](key-exchanges.md): offers, answers, short codes, verifying, revoking, backups
- [Dark shards](dark-shards.md): the master key split among peers, sealed so they cannot read it

## Getting data in and out

- [CSV ingest](ingest-csv.md)
- [JSON ingest](ingest-json.md)
- Services: [HTTP](services/http.md), [Redis](services/redis.md), [SQLite](services/sqlite.md), [Postgres](services/postgres.md)
- [Export](export.md)

## Data and storage

- [Data model](data-model.md): bars, sources, symbols, bar ids, models and records
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
mnnn search save "btc 5m" --symbol BTC/USDT --period 5m
mnnn project create Research && mnnn project add Research --search "btc 5m"
mnnn tui
```
