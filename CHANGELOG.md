# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (pre-1.0: minor = feature, patch = fix/docs).

## [Unreleased]

## [1.6.1] - 2026-10-28

### Added
- Fixed: Arch install — packaging/arch/PKGBUILD now builds from the git checkout (makepkg -si), since the GitHub tarball and release assets it downloaded don't exist publicly; pkgver kept in step by release.sh

## [1.6.0] - 2026-10-28

### Added
- GitHub Actions release workflow: Linux/macOS/Windows x86_64 and arm64 binaries, .deb, .rpm, Arch, Homebrew and winget assets, SHA256SUMS, GitHub Release; CI workflow

## [1.5.0] - 2026-10-28

### Added
- winget manifests (packaging/winget) with portable x64 and arm64 mnnn.exe, cross-built by scripts/cross-windows.sh

## [1.4.0] - 2026-10-28

### Added
- Homebrew formula (packaging/homebrew) installing prebuilt macOS/Linux arm64 and x86_64 binaries
- Arch muninn-bin PKGBUILD and scripts/dist.sh release tarballs; installing no longer needs Rust

## [1.3.0] - 2026-10-28

### Added
- Fedora/RHEL .rpm via cargo generate-rpm (packaging/fedora)

## [1.2.0] - 2026-10-28

### Added
- Debian/Ubuntu .deb via cargo deb (packaging/debian)

## [1.1.0] - 2026-10-28

### Added
- Arch Linux PKGBUILD (packaging/arch) and docs/install.md

## [1.0.0] - 2026-10-28

### Added
- 1.0: stable CLI, preferences, storage layout and schema; 1.0 spec, announcement and changelog archived in docs/historical/1.0/

## [0.8.0] - 2026-10-28

### Added
- Import runs record cancels (time, by, file, phase) and an artifact ledger; cancelled work and empty leftovers are cleaned up (mnnn ingest runs)
- CLI Ctrl-C cancels an ingest cleanly
- Path input Tab completion cycles through candidates like zsh/Neovim

## [0.7.0] - 2026-10-28

### Added
- Import can be cancelled ([Cancel], Esc or c, with confirm); finished files stay, the current file is rolled back
- Every popup has a clickable [X] close button
- Import Places column with Home, Projects, raw/data, bookmarks (b) and recent folders, clickable

## [0.6.2] - 2026-10-28

### Added
- Import progress is byte-based per file and overall with ETA, moving from the first read
- Import preview parses the first 10 lines as bars and marks invalid lines

## [0.6.1] - 2026-10-27

### Added
- Tiles load series in the background and sort, zoom and filter in memory (16 s to under 35 ms on 1M bars)
- Series listing uses a maintained series_stats table (schema migration 2)

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

[Unreleased]: https://github.com/kommandosaldat/muninn/compare/v1.6.1...HEAD
[1.6.1]: https://github.com/kommandosaldat/muninn/compare/v1.6.0...v1.6.1
[1.6.0]: https://github.com/kommandosaldat/muninn/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/kommandosaldat/muninn/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/kommandosaldat/muninn/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/kommandosaldat/muninn/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/kommandosaldat/muninn/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/kommandosaldat/muninn/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/kommandosaldat/muninn/compare/v0.8.0...v1.0.0
[0.8.0]: https://github.com/kommandosaldat/muninn/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/kommandosaldat/muninn/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/kommandosaldat/muninn/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/kommandosaldat/muninn/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/kommandosaldat/muninn/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/kommandosaldat/muninn/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kommandosaldat/muninn/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kommandosaldat/muninn/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kommandosaldat/muninn/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kommandosaldat/muninn/releases/tag/v0.1.0
