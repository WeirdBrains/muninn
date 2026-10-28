# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (pre-1.0: minor = feature, patch = fix/docs).

## [Unreleased]

## [0.6.0] - 2026-10-27

### Added
- File browser with Tab path completion, hidden toggle, filter, places (home, raw/data, cwd, recents)
- Multi-select and whole-folder import with guessed symbol/duration and preview
- Background import with live progress bar, Esc cancel and per-file summary

## [0.5.0] - 2026-10-27

### Added
- Collapsible sidebar (Ctrl-b) with fuzzy series search (/)
- Sort by year or name with asc/desc toggle (o)
- Source, price (metric min/max), period and year filter chips; year filter scopes tiles

## [0.4.0] - 2026-10-27

### Added
- Chart tiles with start/end period selectors, zoom, pan, jump to date and a crosshair readout.
- Toggleable per-tile chart layers (candles, OHLC, line, area, envelope, volume, moving averages, markers, grid, axes).
- Linked tiles: list selection centres charts, chart movement scrolls lists.

## [0.3.0] - 2026-10-27

### Added
- Built-in i3-style tiling content area: split any tile, close, resize, swap, zoom, tabbed/stacked containers.
- Layout presets (2-up, 3-column, main + stack, 2×2), named saved layouts, layout persisted in preferences.
- Virtualized list tiles sortable by every bar column and derived values.

## [0.2.0] - 2026-10-27

### Added
- TUI header with the MUNINN banner, version and status; compact on short terminals.
- Menu bar (File, View, Data, Keys, Theme, Help) with submenus, mnemonics, shortcuts and mouse support.
- Menus, keys and help share named actions; Menus and Keybindings help topics.

## [0.1.0] - 2026-10-27

### Added
- `mnnn` CLI with `~/.muninn` home, preferences and global flags.
- SQLite store with migrations, sources and symbols.
- CSV ingest with source/symbol/duration derivation, validation and idempotent upserts.
- Export to CSV, Parquet and JSON.
- Master key, 24 recovery words, 3-of-5 shards, service keys and SQLCipher encryption.
- Backup and restore, optionally age-encrypted.
- Terminal UI with navigator, ingest, export, keys and theme screens.
- Seven built-in themes with dark/light variants and system mode detection.
- Searchable help built from `docs/`, `mnnn help` and man pages.
- JSON ingest and service ingest from HTTP, Redis, SQLite and Postgres.

[Unreleased]: https://github.com/kommandosaldat/muninn/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/kommandosaldat/muninn/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/kommandosaldat/muninn/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kommandosaldat/muninn/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kommandosaldat/muninn/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kommandosaldat/muninn/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kommandosaldat/muninn/releases/tag/v0.1.0
