# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (pre-1.0: minor = feature, patch = fix/docs).

## [Unreleased]

### Added
- Org TLS: orgs get their own domains (mnnn org domain add|list|remove), served by name from the one site port; each org's certificate is yours (org cert set|clear, key kept in the encrypted store), else Let's Encrypt for its domains (org cert acme --org, HTTP-01 or TLS-ALPN-01), else self-signed; org cert status shows source and expiry; ACME certificates renew by themselves with backoff; TUI Organizations page w / c / L, listed on the Site page
- Scheduled rule sets: --every 6h or --cron "0 3 * * *", shown, edited (TUI "on" field too), tested and dry-run like any rule; run by the TUI, a running org host or mnnn rule tick
- Built-in cert-expiry-watch rule set (editable, removable): renews Let's Encrypt certificates about 30 days before expiry and notifies for your own at 30, 14 and 3 days and on expiry; notifications are notify actions and rule triggers
- Parser code spaces: sources pass data through raw (standard input as one `text` column) and code shapes it; built-in parse-csv, parse-json, parse-ndjson and to-bars ship as templates (mnnn code parsers [--install|--show], code create --from); mnnn pipe - --format csv|json|ndjson puts the parser in as a visible first step; --keep-header for chunked CSV
- Pipe saves to projects, organizations and the network (project:NAME[/J], org:ORG[/J], net[:EXCHANGE|all][/J]), also from the TUI > picker
- Pipelines over Share (projects, journals) and the network: net share/fetch carry the pipeline file; a peer's pipeline and the models it brings arrive as drafts (mnnn pipeline approve, A in the Designer), its code unapproved
- Pipeline Designer (E, Projects ▸ Pipelines…): make and edit pipelines node by node, validated as you go; M ends a branch in a model table (made when new, fields from what passed there); run off the UI thread, open the monitor, export and import (plan first, rename or merge)
- Data tiles: a code space's result as a chart (time x, numeric series) or table (t), with columns or series picked (c), updating on every run and on runs anywhere on the same home; running a pipeline opens its view nodes as tiles side by side (three views: three live columns); a pipeline monitor tile with each node's status, rows in/out, rate, last update, errors and rows landed
- Piped code runs and pipeline runs in the TUI build and run on a worker thread: the UI stays live
- Pipelines: saved, versioned DAGs of sources, code spaces, views and filters that end in model tables; fan-out computed once, fan-in with named inputs (host.input_named); validated (acyclic, every leaf a model); mnnn pipeline create|edit|show|list|run|remove, export/import with code by trust hash and embedded model templates (plan first, rename or merge, imported code unapproved)
- Model design templates: export a model's design to a file and import it (plan first, rename or merge on clashes); built-in bars template; models from peers arrive as drafts to approve
- Code space input picker (i or [Input]): pick a source, or a model then its where, matching, group, sort and limit with the line previewed
- Pipes from standard input (CSV, JSON, NDJSON; --chunk streams) and a live Pipes window in the TUI that follows pipes run anywhere on the same home
- Pipes: code spaces work on data piped in and keep none of it. Any live search (records, series, journals), journal, model query or code space output is a source; steps are code spaces and search filters; saves to journals and models branch off at any step while the data flows on (mnnn pipe, share to a code space, TUI >)

- Math code spaces read named inputs in a pipeline's fan-in: `INPUT.field` (`spread = fast.close - slow.close`)

### Changed
- Standard input to a pipe or pipeline is no longer parsed by Muninn: add a parser step (or --format)
- Series sources give their stored rows as generic structured data through the general record load, with no bar shaping or per-series grouping (a code space such as to-bars makes bars)
- Bars are the records of the built-in bars model, kept in its own table; upgrading moves every bar and every value stored on bars losslessly (same ids)

## [2.15.0] - 2026-10-31

### Added
- Share: send any view's item to anything that takes shares (projects, a new journal entry, code space inputs, connections). Press S or click the share button, or run mnnn share
- Clickable hints: every key shown in a hint is also a button you can click

### Changed
- Docs cover every subcommand; the old net exchange man pages are gone; the repository is now WeirdBrains/muninn

## [2.14.0] - 2026-10-30

### Added
- Review copies: joining copies what is reviewed to your node; `mnnn review view|show` refresh it when the author's node answers and show the copy otherwise (TUI Reviews, Joined tab: Enter)
- Offline review work: comments, suggestions, highlights and votes made while the author's node is unreachable are kept and sent on `mnnn review sync` (or s in the TUI), and by a running node every few minutes
- Record reviews make their model on the reviewer's node or add the missing fields, additively; recorded in history as `review.model`
- Closed, dropped or left reviews keep their copy archived and read-only; `mnnn review remove` forgets it

## [2.13.0] - 2026-10-30

### Added
- Code scopes: every code space is global, user or organization, shown and filtered with `mnnn code list --scope user|org|org:NAME|global` and in the TUI (C, f cycles)
- Code permissions: roles grant code.view, code.run, code.edit and code.manage (nested); older roles carry over (resources.manage, call, read)
- Per-node served code: `mnnn org code list|serve|stop ORG [SPACE]` picks which of an org's spaces this node runs, also for joined orgs
- Give code to an org with `mnnn org give ORG code SPACE` or g in the TUI; members pull it with `mnnn org sync-code ORG` (new, changed or same; approval asked before running)
- Executables in protobuf: `GET /o/ORG/api/executables`, `POST /o/ORG/api/run/NAME`, gRPC `muninn.code.v1.Executables/List` and `<pkg>.<Service>/Run`, `.proto` files under `/o/ORG/proto/`, and gRPC server reflection
- Gated private sites: `mnnn org edit ORG --gated on` shows guests a landing, sign-in and join page only; members sign a browser in with `mnnn org sign-in ORG --site URL` (one-time link, 12-hour cookie)
- Web invite links: `mnnn org member invite` prints `https://HOST/o/ORG/join#…` while the site is served; the invite stays in the fragment and the join page starts the key exchange
- Built-in executables shipped with Muninn (invite-redeem, membership-apply): `mnnn code builtin list|show|override`

## [2.12.1] - 2026-10-30

### Added
- Peers behind a relay stayed unreachable after connecting: each of the relay's addresses made its own circuit and the first failing one cancelled the rest; one circuit per relay is used now, the most reachable
- Fe80 and Docker bridge addresses are never offered or announced, even inside relay circuits

## [2.12.0] - 2026-10-30

### Added
- Python code spaces are translated in process (ruff_python_parser 0.0.20 into vendored python-ast 1.1.0 codegen, Apache-2.0, no pyo3/libpython); MUNINN_RYTHONC is gone
- Checked against Rython test files: output matches CPython-based rythonc 1.1.0 on all 44 (goldens made in dev only)

## [2.11.2] - 2026-10-30

### Added
- External addresses: `mnnn net config --external alien.duckdns.org:4741` (or net.external_addresses) announces a public name or IP and puts it first in invite links
- Invite links leave out Docker bridge addresses (172.17/16, 172.18/16)

## [2.11.1] - 2026-10-30

### Added
- Peers are named after their user, and a connection name works wherever a peer is asked for
- Entrusting shards to a trusted connection no longer asks about short codes
- Piping output into head no longer panics
- Journal entry times show local time
- Invite links leave out IPv6 link-local addresses

## [2.11.0] - 2026-10-30

### Added
- Linked nodes sync all of a user's data end to end encrypted (`mnnn sync status|now`), about once a minute while a node runs
- Conflicts keep the newer row and save the other (`mnnn sync conflicts|show|resolve`); an edit beats a delete
- Bars sync on request (`mnnn sync bars`); revoked nodes are not served

### Changed
- Other nodes' history is added under local ids; a linking node drops its old identity

## [2.10.0] - 2026-10-30

### Added
- User identities: each node acts for a user, linked nodes are approved and can be revoked (mnnn user show|link|approve|nodes|revoke)
- Connections with trust, recorded in history (mnnn connection list|trust|remove; TUI Net ▸ Connections…, t)
- One invite system for connections, orgs and reviews (mnnn invite create|list|revoke|accept|decline; TUI Net ▸ Invites…)

### Changed
- Orgs belong to users (existing orgs move to the current user) and a member's linked nodes act with the member's role
- Dark shards go only to trusted connections (the TUI lists only those; mnnn net shard entrust asks first); reviewer names come from the user
- The Net window's Connections page is now Nodes; pages 1–9 and 0

### Fixed
- A withdrawn or used invite link now fails for whoever uses it instead of looking connected

## [2.9.2] - 2026-10-30

### Fixed
- Net status, peers, requests and exchanges show times in local time with the UTC offset (were UTC without a label)
- Dark-shards docs: entrust takes the words, shards or an exported master key file, not the unlocked database

## [2.9.1] - 2026-10-30

### Changed
- The Net menu groups node actions under Node and dark shards under Dark shards, and adds Invite or connect

### Fixed
- The menu bar fits in 80 columns (Network and Organizations are now Net and Orgs; the hint shows only when there is room)
- Dropdowns open below their category and never cover the menu bar on short terminals

## [2.9.0] - 2026-10-30

### Added
- Key exchanges run automatically between nodes: connect, org apply/join/invite, reviews and shares show only the action and its status
- Connect requests with accept/decline (mnnn net requests|accept|decline), trust on first use pinned to the node key, and a warning when a known label changes key
- Invite links (mnnn net invite, muninn://join/...) usable from the CLI, a TUI paste or a link
- Net.autostart (default on) and hosting.autostart (default off), settable in prefs, CLI and TUI
- TUI dark-shard entrust form: pick peers, set the threshold, masked shard password, per-peer status and a restore check

### Changed
- The low-level token exchange stays under mnnn net exchange as an advanced command

### Fixed
- The TUI highlight prompt no longer prefills @

## [2.8.0] - 2026-10-30

### Added
- `mnnn review`: request, invite, join, view, comment, suggest, highlight, vote, withdraw, accept, reject, merge, close, reopen, drop, leave, remove and revisions
- Review links: single-use key exchanges scoped to one review, with per-review permissions
- Merges make numbered revisions (`mnnn review revisions`); conflicts are reported unless `--force`
- TUI: the Reviews window (`V`, Projects ▸ Reviews)
- Rules can react to `review.*` actions

### Fixed
- Rules can react to organization actions (`org.*`)

## [2.7.0] - 2026-10-30

### Added
- Organizations owned by your node, private and members-only by default, public or open on request - mnnn org create|edit|remove|list|show, or G (Organizations menu) in the TUI
- Roles with ranks and permissions (owner, admin, user, viewer, plus your own) and default roles for applicants and invitees - mnnn org role list|perms|create|edit|remove, mnnn org member defaults
- Membership through org key exchanges: browse a node, apply, approve or reject, invite and join, change roles, kick (keeping, archiving or publishing what they gave) and leave - mnnn org browse|apply|sync|join|leave, mnnn org member approve|reject|invite|role|kick
- Org resources and projects: add or give journals, projects, reports, models, searches and code, publish to members or everyone, archive, fetch - mnnn org resource|publish|unpublish|project|give|fetch
- Endpoints that run rules, code spaces or record queries, gated by role - mnnn org endpoint add|list|remove, mnnn org call
- HTTPS org sites with HTML, JSON and gRPC (muninn.org.v1.Org), templates with assets, member-signed requests and mirrors; off until started, localhost by default - mnnn org host start|stop|status, mnnn org template, mnnn org sign, mnnn org mirror
- Short-lived certificates: self-signed at each start, or Let's Encrypt via ACME (opt-in with a domain and email) renewed automatically - mnnn org cert status|acme|renew

## [2.6.0] - 2026-10-30

### Added
- Peer-to-peer node over TCP and QUIC with Noise encryption, local discovery (mDNS), a wide network (Kademlia) and relays with hole punching; off until started - mnnn net start|status|stop, or K (Network menu) in the TUI
- Key exchanges authenticated by signed offer/answer tokens and a short code to compare; verify, label, revoke and back up (age-encrypted) - mnnn net exchange create|accept|verify|revoke|export|restore
- Opt-in sharing per exchange or with all exchanges; list, fetch and import a peer's shared items, and run live searches across peers - mnnn net share|list|fetch|search
- Dark shards: the master key split among peers, each shard sealed with your password so holders cannot read it; recover from peers - mnnn net shard entrust|restore
- Peers, relays, settings and history of every network action (never keys, words, shards or passwords) - mnnn net peers|relay|config, Network window pages 1-8

## [2.5.1] - 2026-10-29

### Changed
- Bars fields from imports can be renamed, retyped, re-constrained or dropped like any field, with plans, versions and rollback (v0 is the shipped design); the bars keep their imported values - mnnn model edit bars, or D > bars in the TUI
- Bars records can be created, updated in any field and deleted, and the bars model renamed or removed (removing deletes every bar, after asking) - mnnn model record create|delete bars, mnnn -y model remove bars

### Fixed
- Imports, charts, filters, exports and archives keep working after bars fields change; searches naming a renamed or dropped field say so

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

[Unreleased]: https://github.com/WeirdBrains/muninn/compare/v2.15.0...HEAD
[2.15.0]: https://github.com/WeirdBrains/muninn/compare/v2.14.0...v2.15.0
[2.14.0]: https://github.com/WeirdBrains/muninn/compare/v2.13.0...v2.14.0
[2.13.0]: https://github.com/WeirdBrains/muninn/compare/v2.12.1...v2.13.0
[2.12.1]: https://github.com/WeirdBrains/muninn/compare/v2.12.0...v2.12.1
[2.12.0]: https://github.com/WeirdBrains/muninn/compare/v2.11.2...v2.12.0
[2.11.2]: https://github.com/WeirdBrains/muninn/compare/v2.11.1...v2.11.2
[2.11.1]: https://github.com/WeirdBrains/muninn/compare/v2.11.0...v2.11.1
[2.11.0]: https://github.com/WeirdBrains/muninn/compare/v2.10.0...v2.11.0
[2.10.0]: https://github.com/WeirdBrains/muninn/compare/v2.9.2...v2.10.0
[2.9.2]: https://github.com/WeirdBrains/muninn/compare/v2.9.1...v2.9.2
[2.9.1]: https://github.com/WeirdBrains/muninn/compare/v2.9.0...v2.9.1
[2.9.0]: https://github.com/WeirdBrains/muninn/compare/v2.8.0...v2.9.0
[2.8.0]: https://github.com/WeirdBrains/muninn/compare/v2.7.0...v2.8.0
[2.7.0]: https://github.com/WeirdBrains/muninn/compare/v2.6.0...v2.7.0
[2.6.0]: https://github.com/WeirdBrains/muninn/compare/v2.5.1...v2.6.0
[2.5.1]: https://github.com/WeirdBrains/muninn/compare/v2.5.0...v2.5.1
[2.5.0]: https://github.com/WeirdBrains/muninn/compare/v2.4.0...v2.5.0
[2.4.0]: https://github.com/WeirdBrains/muninn/compare/v2.3.0...v2.4.0
[2.3.0]: https://github.com/WeirdBrains/muninn/compare/v2.2.0...v2.3.0
[2.2.0]: https://github.com/WeirdBrains/muninn/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/WeirdBrains/muninn/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/WeirdBrains/muninn/compare/v1.16.0...v2.0.0
[1.16.0]: https://github.com/WeirdBrains/muninn/compare/v1.15.0...v1.16.0
[1.15.0]: https://github.com/WeirdBrains/muninn/compare/v1.14.0...v1.15.0
[1.14.0]: https://github.com/WeirdBrains/muninn/compare/v1.13.0...v1.14.0
[1.13.0]: https://github.com/WeirdBrains/muninn/compare/v1.12.3...v1.13.0
[1.12.3]: https://github.com/WeirdBrains/muninn/compare/v1.12.2...v1.12.3
[1.12.2]: https://github.com/WeirdBrains/muninn/compare/v1.12.1...v1.12.2
[1.12.1]: https://github.com/WeirdBrains/muninn/compare/v1.12.0...v1.12.1
[1.12.0]: https://github.com/WeirdBrains/muninn/compare/v1.11.0...v1.12.0
[1.11.0]: https://github.com/WeirdBrains/muninn/compare/v1.10.0...v1.11.0
[1.10.0]: https://github.com/WeirdBrains/muninn/compare/v1.9.0...v1.10.0
[1.9.0]: https://github.com/WeirdBrains/muninn/compare/v1.8.0...v1.9.0
[1.8.0]: https://github.com/WeirdBrains/muninn/compare/v1.7.0...v1.8.0
[1.7.0]: https://github.com/WeirdBrains/muninn/compare/v1.6.1...v1.7.0
[1.6.1]: https://github.com/WeirdBrains/muninn/compare/v1.6.0...v1.6.1
[1.6.0]: https://github.com/WeirdBrains/muninn/compare/v1.5.0...v1.6.0
[1.5.0]: https://github.com/WeirdBrains/muninn/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/WeirdBrains/muninn/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/WeirdBrains/muninn/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/WeirdBrains/muninn/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/WeirdBrains/muninn/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/WeirdBrains/muninn/compare/v0.8.0...v1.0.0
[0.8.0]: https://github.com/WeirdBrains/muninn/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/WeirdBrains/muninn/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/WeirdBrains/muninn/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/WeirdBrains/muninn/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/WeirdBrains/muninn/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/WeirdBrains/muninn/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/WeirdBrains/muninn/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/WeirdBrains/muninn/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/WeirdBrains/muninn/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/WeirdBrains/muninn/releases/tag/v0.1.0
