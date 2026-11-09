# Muninn documentation

Muninn (`mnnn`) ingests crypto market data from files and services, normalizes it into
bars and stores them in an encrypted SQLite database. These pages are also the in-app
help (`?` or F1 in the TUI, `mnnn help <topic>` on the command line).

## Using Muninn

- [Install](install.md): packages per platform, building from source, uninstalling
- [Muninn 2.0](announcement-2.0.md): the launch post: what Muninn is, the headline features, install
- [2.0 archive](https://github.com/WeirdBrains/muninn/tree/main/docs/historical/2.0): the 2.0 plan with every phase, the announcement and the changelog as of the wrap-up
- [Releasing](release.md): the release script, local builds of all 14 files, and publishing
- [Website](website.md): the single-page Jaspr site in `site/`, how it is built and checked
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
- [Schedules](schedules.md): rule sets on time (start_at, run_every, ends_at, cron), the scheduler
- [Models](models.md): design record tables, migrate them with previews and roll back; records
- [Model design templates](model-templates.md): export, import and share model designs; built-in templates
- [Code spaces](code-spaces.md): math, Polars, Rust or Python compiled to native plugins over your data
- [Plugins](plugins.md): code spaces by kind (`mnnn plugins`, the Plug menu)
- [Ingest connectors](connectors.md): code spaces that fetch data, store it raw, pipe it and run a rule set (SDK 1.3)
- [Export formats](export-formats.md): PDF, HTML, Word, LaTeX, JSON, CSV; printing; project archives and import

## Network

- [Network](network.md): the node, peers, relays, sharing and searching across peers; the Net menu
- [Users and connections](users.md): your user identity, linked nodes, trusted connections, invites
- [Linked-node sync](sync.md): all of your data on all of your nodes, conflicts, bars on request
- [Key exchanges](key-exchanges.md): offers, answers, short codes, verifying, revoking, backups
- [Dark shards](dark-shards.md): the master key split among peers, sealed so they cannot read it
- [Organizations](organizations.md): roles, members, resources, endpoints and templates; the Orgs menu
- [Pipes](pipes.md): pipe any data through code spaces, filter it, save it on the way
- [Pipelines](pipelines.md): design note for saved pipelines that end in a model table
- [Share](share.md): share anything to a project, a journal, a code space or your exchanges
- [Reviews](reviews.md): review links, comments, suggestions, highlights, votes, merges and revisions
- [Deploying a relay](deploy-relay.md): a public relay node on a droplet: DNS, systemd, firewall, ACME
- [Org hosting](org-hosting.md): HTTPS sites with HTML, JSON and gRPC, signed member requests, Let's Encrypt
- [Notifications](notifications.md): the header bell, `mnnn notify`, rule-set notify and broadcast actions, node broadcasts
- [Live collaboration and peers](collaboration.md): messages, live editing with cursors, the peers view, running a peer's executables
- [MCP](mcp.md): Muninn as an MCP server (stdio, HTTP, HTTPS with Let's Encrypt; named clients with roles), external MCP servers for rule sets
- [MCP behind nginx](mcp-proxy.md): the MCP HTTP server behind nginx with a certbot certificate
- [Org site templates](org-templates.md): edit, preview and build an org site's look; site announcements

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
- [Flutter app](flutter-app.md): the desktop and mobile app
- [Architecture](architecture.md): the workspace crates and front ends

## Quick start

```
export MUNINN_HOME=/tmp/mnnn-demo          # never touches ~/.muninn
mnnn keys init                             # write down the words and shards
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/binance_BTCUSDT_5m.csv --source binance
mnnn code parsers --install                # parse-csv, to-bars, … as code spaces
mnnn pipe source:binance code:parse-csv 'code:to-bars?symbol=BTC/USDT&duration=5m' --to model:bars -y
mnnn export --format parquet --out /tmp/btc.parquet --symbol BTC/USDT
mnnn search save "btc 5m" --symbol BTC/USDT --period 5m
mnnn project create Research && mnnn project add Research --search "btc 5m"
mnnn tui
```
