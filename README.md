# Muninn (`mnnn`)

Local, encrypted store for OHLCV market bars. Ingest from CSV, JSON, HTTP APIs,
Redis, SQLite and Postgres; export to CSV, Parquet or JSON; browse it in a TUI.

Data lives in `~/.muninn` (override with `MUNINN_HOME` or `--home`), in a
SQLCipher database protected by an envelope key scheme (see `docs/encryption.md`).

## Screenshots

<!-- screenshots: written by scripts/screenshots.sh -->
![cat ticks.csv | mnnn pipeline run feed -: three views as three live columns](site/web/screenshots/pipeline-live.png)

<details><summary>Every feature (pipeline-live above; 34 screens)</summary>

**Home: every series in a list, the header with the node's status**

![Home: every series in a list, the header with the node's status](site/web/screenshots/home.png)

**A chart tile beside the list: ETH/USDT 5m candles in the Muninn theme**

![A chart tile beside the list: ETH/USDT 5m candles in the Muninn theme](site/web/screenshots/home-chart.png)

**The action history bar: every step, forks kept, step back and on**

![The action history bar: every step, forks kept, step back and on](site/web/screenshots/history.png)

**Live searches: saved questions that rerun on new data**

![Live searches: saved questions that rerun on new data](site/web/screenshots/live-searches.png)

**Share view, mid-action: saving the view as a live search**

![Share view, mid-action: saving the view as a live search](site/web/screenshots/share-view-name.png)

**Share: pick where it goes (a project, an org, a connection)**

![Share: pick where it goes (a project, an org, a connection)](site/web/screenshots/share-view-picker.png)

**Journals with their entries**

![Journals with their entries](site/web/screenshots/journals.png)

**A journal's entries in Markdown**

![A journal's entries in Markdown](site/web/screenshots/journal-entries.png)

**Sharing a journal entry: the picker open**

![Sharing a journal entry: the picker open](site/web/screenshots/share-picker.png)

**Reports: sections of text, series and journals, exported to PDF, Docx or HTML**

![Reports: sections of text, series and journals, exported to PDF, Docx or HTML](site/web/screenshots/reports.png)

**A report's sections**

![A report's sections](site/web/screenshots/report-sections.png)

**Rule sets: on actions or on a schedule (every 1h, cron), with the built-in cert watcher**

![Rule sets: on actions or on a schedule (every 1h, cron), with the built-in cert watcher](site/web/screenshots/rules.png)

**A scheduled rule: its schedule, next run and then-actions**

![A scheduled rule: its schedule, next run and then-actions](site/web/screenshots/rule-scheduled.png)

**New rule, mid-action: "on" takes action kinds or a schedule**

![New rule, mid-action: "on" takes action kinds or a schedule](site/web/screenshots/rule-new.png)

**Model designer: models, the built-in bars model among them**

![Model designer: models, the built-in bars model among them](site/web/screenshots/models.png)

**Designing a model: the migration plan before apply**

![Designing a model: the migration plan before apply](site/web/screenshots/model-plan.png)

**Records of the new model**

![Records of the new model](site/web/screenshots/model-records.png)

**Model templates, mid-action: importing a design (planned first, rename or merge)**

![Model templates, mid-action: importing a design (planned first, rename or merge)](site/web/screenshots/model-template-import.png)

**New code space, mid-action: choosing a language (math, Polars, Rust, Python)**

![New code space, mid-action: choosing a language (math, Polars, Rust, Python)](site/web/screenshots/code-new.png)

**Editing a code space**

![Editing a code space](site/web/screenshots/code-editor.png)

**Code spaces and their scope**

![Code spaces and their scope](site/web/screenshots/code-list.png)

**A code space's result as a live chart tile beside the series**

![A code space's result as a live chart tile beside the series](site/web/screenshots/code-chart.png)

**cat ticks.csv | mnnn pipeline run feed -: three views as three live columns**

![cat ticks.csv | mnnn pipeline run feed -: three views as three live columns](site/web/screenshots/pipeline-live.png)

**The pipeline monitor: each node's status, rows, rate and what landed**

![The pipeline monitor: each node's status, rows, rate and what landed](site/web/screenshots/pipeline-monitor.png)

**Pipes: data from stdin or any search through code spaces, followed live**

![Pipes: data from stdin or any search through code spaces, followed live](site/web/screenshots/pipes.png)

**The pipeline designer: a graph of sources, code, views and model tables**

![The pipeline designer: a graph of sources, code, views and model tables](site/web/screenshots/pipeline-designer.png)

**Pipeline designer, mid-action: adding a code node**

![Pipeline designer, mid-action: adding a code node](site/web/screenshots/pipeline-designer-add.png)

**Organizations: visibility, access, hosting, domains (w), certificate (c), ACME (L)**

![Organizations: visibility, access, hosting, domains (w), certificate (c), ACME (L)](site/web/screenshots/orgs.png)

**Org TLS, mid-action: adding a domain**

![Org TLS, mid-action: adding a domain](site/web/screenshots/org-domain.png)

**The org site: each org's domains and certificate (yours, Let's Encrypt or self-signed)**

![The org site: each org's domains and certificate (yours, Let's Encrypt or self-signed)](site/web/screenshots/org-site.png)

**Reviews: comments, suggestions, highlights and votes on shared work**

![Reviews: comments, suggestions, highlights and votes on shared work](site/web/screenshots/reviews.png)

**The network: this node, relays, connections and shares, end to end encrypted**

![The network: this node, relays, connections and shares, end to end encrypted](site/web/screenshots/network.png)

**Peers: nodes seen nearby and through relays**

![Peers: nodes seen nearby and through relays](site/web/screenshots/peers.png)

**Connections: people you trust, made through invites**

![Connections: people you trust, made through invites](site/web/screenshots/connections.png)

</details>
<!-- /screenshots -->

## Install

Packages for Arch, Debian/Ubuntu, Fedora, Homebrew and winget, plus uninstall steps:
see [docs/install.md](docs/install.md) (also `mnnn help install`).

## Build

```sh
cargo build --release
./target/release/mnnn --help
```

## First run

```sh
export MUNINN_HOME=/tmp/muninn-demo        # try it somewhere disposable first
mnnn keys init                              # prompts for a password, prints 24 words + shards
# write the words to a file you keep offline, then export age-encrypted copies:
mnnn keys export --out ~/muninn-keys --words-file words.txt
```

Scripts can set `MUNINN_PASSWORD` instead of the prompt; add `--non-interactive -y`
to never prompt.

## Ingest

```sh
RAW=~/Projects/kommandosaldat/raw/data
# source, symbol and bar size come from flags or the file name (BTCUSDT_5m.csv -> BTC/USDT 5m)
mnnn ingest csv --path $RAW/BTCUSDT_5m.csv --source binance
mnnn ingest csv --path $RAW --source binance          # every CSV in a directory
mnnn ingest csv --path $RAW/ETHUSD_5m.csv --source kraken --rejects /tmp/rejects.csv

mnnn ingest json --path bars.json --records data.bars --source myfeed

mnnn ingest service http --url https://api.example.com/klines \
    --param symbol=BTCUSDT --param interval=5m --records result \
    --paginate time --from 2024-01-01 --symbol BTC/USDT --duration 5m
mnnn ingest service sqlite --path exchange.db --table BTCUSDT_1h
mnnn ingest service redis --uri redis://localhost --pattern 'bars:*' --value-format fields
mnnn ingest service postgres --host db --username me --password-env PGPASS \
    --database market --table btc_5m --symbol BTC/USDT
```

Re-ingesting an unchanged file is skipped; `--force` re-reads it (duplicates are ignored).

## Query and export

```sh
mnnn sources list
mnnn symbols list
mnnn export --format csv --out - --symbol BTC/USDT --from 2017-09-01 --to 2017-09-02
mnnn export --format parquet --out btc.parquet --symbol BTC/USDT --duration 5m
```

## Backup and recovery

```sh
MUNINN_EXPORT_PASSWORD=... mnnn backup create --out ~/muninn-backups --encrypt
mnnn backup restore --from ~/muninn-backups     # newest archive in the directory
mnnn keys recover --words-file words.txt      # or --shard ... --shard ... --shard ...
```

## TUI, themes, help

```sh
mnnn tui                      # ? or F1 opens help, F10 the menu bar
                              # Ctrl-w tiling, Ctrl-b sidebar, / search, i import
mnnn theme list
mnnn theme set catppuccin
mnnn theme mode dark
mnnn help ingest-csv
mnnn help --search shard
```

## Research

Projects, live searches, journals, reports, rules and models are described in
[docs/README.md](docs/README.md). Code spaces run your own math, Polars, Rust or
Python over the data, compiled to native plugins ([docs/code-spaces.md](docs/code-spaces.md)):

```sh
mnnn code check                                # which languages can build here
mnnn code create fast --lang math --model bars --group symbol --source "fast = sma(close, 10)"
mnnn code run fast                             # asks before the first run of new code
```

## Man pages

Pages are pre-generated in `man/`. Install for your user:

```sh
mkdir -p ~/.local/share/man/man1
cp man/*.1 ~/.local/share/man/man1/
mandb -q ~/.local/share/man 2>/dev/null || true
man mnnn
```

Or read one without installing: `MANPAGER=cat man -l man/mnnn.1`.
After changing the CLI, regenerate with `cargo run -- man --out man`.

## Docs and tests

Feature docs are in `docs/` (also shown by the TUI help). Run `cargo test`; tests use
temporary homes and never touch `~/.muninn`. The full-file ingest test:

```sh
MUNINN_RAW_DATA=~/Projects/kommandosaldat/raw/data cargo test --release --test cli_ingest -- --ignored
```

## Versioning

Semantic versioning with annotated git tags (`v1.0.0`, ...): new features bump the
minor version, fixes/docs the patch version, breaking changes the major. See
`CHANGELOG.md`. The 1.0 spec, announcement and changelog are archived in
`docs/historical/1.0/`; `PLAN.md` is the plan for 2.0.
