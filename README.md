# Muninn (`mnnn`)

Local, encrypted store for OHLCV market bars. Ingest from CSV, JSON, HTTP APIs,
Redis, SQLite and Postgres; export to CSV, Parquet or JSON; browse it in a TUI.

Data lives in `~/.muninn` (override with `MUNINN_HOME` or `--home`), in a
SQLCipher database protected by an envelope key scheme (see `docs/encryption.md`).

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
