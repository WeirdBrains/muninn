# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (pre-1.0: minor = feature, patch = fix/docs).

## [Unreleased]

## [2.5.0] - 2026-10-29

### Added
- Code spaces in math, Polars, Rust or Python (via Rython), compiled to native plugins - mnnn code create NAME --lang L, or C (Projects > Code spaces) in the TUI
- Run over any model with filters, grouping and sorting; results are tables, charts, text or tiles - mnnn code run, or r in the code window
- Plugins load, update and create records and models through the host API, recorded in history; new or changed code asks before its first run - mnnn code approve
- Builds cached by content hash; prebuilt plugins load too; the SDK uses a stable C ABI (v1) - mnnn code build|check|sdk
- Code spaces join projects, report sections, rules and archives (source only) - mnnn project add --code, mnnn report add --code, then-action run-code

## [2.4.0] - 2026-10-29

### Added
- Model designer: typed fields with constraints, drafts, migration plans, apply and roll back - mnnn model create|edit|plan|apply|rollback, or D (Data > Model designer) in the TUI
- Records for any model with filters, text search, sort, group and totals - mnnn model record create|update|list|show|delete, or r in the designer
- Bars take added fields of their own; built-in bar fields stay fixed
- Rules feed models - then-actions model-create|edit|apply|remove and record-create|update|delete; rules can live in a model (--model)
- Live searches and report sections over records - mnnn search save --model M --where COND, mnnn report add --model M
- Projects and archives carry models with their versions, records and rules - mnnn project add --model

## [2.3.0] - 2026-10-29

### Added
- Rules on action-history events with conditions — `mnnn rule create NAME --on KIND --if COND --then ACTION`, or Projects ▸ Rules… (R) in the TUI
- Then-actions: status message, run a live search, export a report or journal, add to a project, write an entry or note
- Rules live standalone, in a journal or live search, and in project rule sets — `mnnn project add --rule`; archives carry them, imported switched off
- Test and dry-run rules against any action — `mnnn rule test|dry-run [RULE] [--action ID]`, or t / x in the TUI; `mnnn rule events` lists kinds and fields
- Rule runs recorded in history with loop protection (no re-entry, depth 3, 50 runs per action)

## [2.2.0] - 2026-10-29

### Added
- Reports: build from text, records, series, journals, projects, authors and a code placeholder — `mnnn report create|add|edit-section|move|export|preview`, or Reports… (O) in the TUI
- Document export in PDF, Word, HTML, LaTeX, JSON, CSV and text, rendered in pure Rust from LaTeX — `--format` on `report|journal|project export`, or E in the TUI
- Journal export, import and print — `mnnn journal export|import|print` (`--save-only` keeps the PDF), or E / P in the TUI
- Project archives that restore everything — `mnnn project export` (.mnnp) and `mnnn project import`, or E / I in the TUI
- Bars as documents — `mnnn export --format pdf|docx|html|latex`

## [2.1.0] - 2026-10-29

### Added
- Live searches: saved searches that always run on current data; mnnn search save|run|fork|edit, or B to save and F to open them in the TUI
- Projects: live searches, results and journals together; mnnn project create|add|show, or Projects ▸ Projects… (P) in the TUI, with a in the sidebar to add a series
- Journals: Markdown or LaTeX entries with images, notes and highlights on selected words; mnnn journal new|write|note|highlight|attach, or J in the TUI (v selects, m highlights, n notes)
- Fork a journal into a project and copy selected entries to another or a new journal: mnnn journal fork|copy, or f and c in the TUI
- Projects menu (Alt+P) in the TUI and help topics live-searches, projects and journals

## [2.0.0] - 2026-10-29

### Added
- Authors: `mnnn author add --name NAME [--email E] [--detail k=v]`, `list`, `show`, `edit`, `remove`, `use`; in the TUI `A` (**File ▸ Authors…**) adds, edits, deletes and picks the current author, shown in the header
- Action history: imports, exports, filter and time range changes, theme changes, sources, symbols and author changes are saved with their author; `mnnn history list|show|tree`
- Step back and forward through the history: `u` / `U` in the TUI or `mnnn history back|forward|goto` restore filters, time range and theme; acting after a step back starts a fork, and every branch is kept
- History bar: `Y` (**View ▸ History**) opens a collapsible left pane with the action tree and its forks; Enter or a click goes to an action

### Changed
- The database schema moves to version 5 (new authors, settings and actions tables); 1.x databases upgrade in place on first open

## [1.16.0] - 2026-10-29

### Added
- Time range on every view: press `T` (or **Data ▸ Filters ▸ Time range…**, or click the sidebar's `time` row) to pick a date and time from, then to; lists, charts and the sidebar show only bars in that range, `x` leaves an end open
- Symbol filter: `m` in the sidebar (or **Data ▸ Filters ▸ Symbol**) lists only the checked symbols

### Fixed
- Sidebar search found no symbol when typed with `-` or `_` (`btc-usdt`) or with words in another order (`binance btc`)
- The import preview showed `?` as the symbol for names like `Kraken-ETHUSDT_1h.csv` although the import found it

## [1.15.0] - 2026-10-29

### Added
- Set where data came from: `mnnn ingest path <DIR> --source coinbase` (also `ingest csv` / `json`), or in the TUI import press `s` and type it; left empty, each file's source is the exchange in its name (`coinbase_BTC-USD_5m.csv` → `coinbase`), else `local` in the TUI
- The TUI import preview shows the source each file will get, e.g. `source kraken (from name)`

## [1.14.0] - 2026-10-29

### Added
- Tile navigation: `Shift-Tab` (or `Ctrl-w p`) focuses the previous tile, `Tab` the next, and `Alt-1` … `Alt-9` jumps straight to tile `[n]`, also inside tabs and stacks; the status line lists the keys

## [1.13.0] - 2026-10-29

### Added
- Date and time picker: in a chart, `[` / `]` or a click on the start or end date opens a picker for year, month, day, hour and minute; change fields with the arrows, digits or the clickable `▲` / `▼`
- List time range: in a list tile, `[` / `]` or a click on `from …` / `to …` in the title lists only bars in that range; `\` clears it

### Changed
- Typing a chart start or end moved to `{` / `}`

## [1.12.3] - 2026-10-29

### Fixed
- The TUI symbol popup shows the file name on its own line and its folder below (shortened with `…`), so long paths no longer hide it; folder imports (`A`) count `file 3 of 12` in the title

## [1.12.2] - 2026-10-28

### Fixed
- `packaging/arch/build-local.sh` now installs the package it builds (`sudo pacman -U --needed`), with or without namcap; `--no-install` only builds
- Arch packages pass namcap cleanly: stripped `mnnn` binary, `libgcc` dependency, `$CARCH` sources

## [1.12.1] - 2026-10-28

### Added
- Arch package for ARM: releases now include `muninn-bin-<ver>-1-aarch64.pkg.tar.zst` next to the x86_64 one (in `SHA256SUMS`); install with `sudo pacman -U <file>`

### Changed
- Release notes list each feature with how to use it; `scripts/sync-release-notes.sh --latest` rewrites published releases' notes from the changelog

### Fixed
- The newest version is now the release marked Latest on GitHub, even when several tags are pushed together

## [1.12.0] - 2026-10-28

### Added
- Loading indicator: while a series loads, its tile shows a progress bar (`42%  12,345 / 29,000 bars`) or a spinner until the total is known
- Series load in the background in chunks, so menus, help and other tiles keep responding; a failed load shows its error in the tile

## [1.11.0] - 2026-10-28

### Added
- Clickable help: click a link to follow it, or select one with `Tab` / `Shift-Tab` and press `Enter`; `Backspace` goes back
- Help key bar: the shortcuts are listed along the bottom of the help popup and every one is clickable
- Search from anywhere in help: press `/` (or click the search box) on any topic; click a result or history entry to open it
- `man mnnn-tui` and `man mnnn-help` gain a NOTES section on importing, help keys and loading

## [1.10.0] - 2026-10-28

### Added
- Auto-load after import: when a TUI import (`i`) finishes and no tile shows data, the focused tile loads the imported series

## [1.9.0] - 2026-10-28

### Added
- Symbol popup: when the TUI import (`i`) can't guess a symbol from a file name, a popup asks for it; `OK for all` reuses it for the remaining files, or skip the file

## [1.8.0] - 2026-10-28

### Added
- Everything actionable in the TUI is clickable: import picker files, folders and inputs, popup rows, form fields and buttons, themes, list rows
- Key buttons such as `[Close]` and `[OK]` act like pressing that key

## [1.7.0] - 2026-10-28

### Added
- Import a whole folder: `mnnn ingest path <DIR>` imports every CSV and JSON file under it, with per-file and overall progress; in the TUI import (`i`), `A` imports the current folder, or select folders with `Space`
- `mnnn ingest csv` / `json` given a directory now search it recursively; `--no-recursive` limits them to the top level

## [1.6.1] - 2026-10-28

### Fixed
- Arch install: `cd packaging/arch && makepkg -si` now builds from your git checkout instead of downloading GitHub sources that aren't public

## [1.6.0] - 2026-10-28

### Added
- Release builds: pushing a `vX.Y.Z` tag publishes Linux, macOS and Windows binaries (x86_64 and arm64), `.deb`, `.rpm`, an Arch package, Homebrew and winget manifests and `SHA256SUMS` as a GitHub Release
- CI checks every push and pull request (build, clippy, fmt, tests)

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

[Unreleased]: https://github.com/kommandosaldat/muninn/compare/v2.5.0...HEAD
[2.5.0]: https://github.com/kommandosaldat/muninn/compare/v2.4.0...v2.5.0
[2.4.0]: https://github.com/kommandosaldat/muninn/compare/v2.3.0...v2.4.0
[2.3.0]: https://github.com/kommandosaldat/muninn/compare/v2.2.0...v2.3.0
[2.2.0]: https://github.com/kommandosaldat/muninn/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/kommandosaldat/muninn/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/kommandosaldat/muninn/compare/v1.16.0...v2.0.0
[1.16.0]: https://github.com/kommandosaldat/muninn/compare/v1.15.0...v1.16.0
[1.15.0]: https://github.com/kommandosaldat/muninn/compare/v1.14.0...v1.15.0
[1.14.0]: https://github.com/kommandosaldat/muninn/compare/v1.13.0...v1.14.0
[1.13.0]: https://github.com/kommandosaldat/muninn/compare/v1.12.3...v1.13.0
[1.12.3]: https://github.com/kommandosaldat/muninn/compare/v1.12.2...v1.12.3
[1.12.2]: https://github.com/kommandosaldat/muninn/compare/v1.12.1...v1.12.2
[1.12.1]: https://github.com/kommandosaldat/muninn/compare/v1.12.0...v1.12.1
[1.12.0]: https://github.com/kommandosaldat/muninn/compare/v1.11.0...v1.12.0
[1.11.0]: https://github.com/kommandosaldat/muninn/compare/v1.10.0...v1.11.0
[1.10.0]: https://github.com/kommandosaldat/muninn/compare/v1.9.0...v1.10.0
[1.9.0]: https://github.com/kommandosaldat/muninn/compare/v1.8.0...v1.9.0
[1.8.0]: https://github.com/kommandosaldat/muninn/compare/v1.7.0...v1.8.0
[1.7.0]: https://github.com/kommandosaldat/muninn/compare/v1.6.1...v1.7.0
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
