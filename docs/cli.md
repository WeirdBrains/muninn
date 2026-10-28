# Command line

```
mnnn [GLOBAL FLAGS] <COMMAND>

  ingest csv|json|service <http|redis|sqlite|postgres>
  export
  sources  list|add|rename|remove
  symbols  list|add
  keys     init|status|rotate|rekey|export|recover|shards
  backup   create|restore
  theme    list|show|set|mode|export|import|reset
  tui
  help     [TOPIC] [--search TEXT]
```

## Global flags

| flag | env | default | meaning |
|------|-----|---------|---------|
| `--home DIR` | `MUNINN_HOME` | `~/.muninn` | root folder ([storage layout](storage-layout.md)) |
| `--db PATH` | `MUNINN_DB` | `<home>/storage/muninn.database` | database file |
| `--non-interactive` | `MUNINN_NON_INTERACTIVE` | off | never prompt; missing values are errors |
| `-v`, `--verbose` | | | more logging (repeat for more); conflicts with `-q` |
| `-q`, `--quiet` | `MUNINN_QUIET` | off | only errors and requested data (exports to `-`, secrets) |
| `-y`, `--yes` | `MUNINN_YES` | off | answer yes to confirmations |
| `--theme-mode MODE` | `MUNINN_THEME_MODE` | from preferences | `light`, `dark` or `system` for this run |

`-h` is help, `-V` the version. There is no other `-h`.

Prompts only happen when stdin is a terminal and `--non-interactive` is not set, so
scripts and tests never hang. `-y` answers confirmations (overwrite on restore, rotate,
rekey, re-issue shards, writing all keys to one directory, dropping rejected rows) but
never invents a missing source, symbol or duration.

## Other environment variables

| variable | meaning |
|----------|---------|
| `MUNINN_PASSWORD` | password that unlocks the service key (else a hidden prompt) |
| `MUNINN_EXPORT_PASSWORD` | password for key exports and encrypted backups (falls back to `MUNINN_PASSWORD`) |
| `MUNINN_LOG` | tracing filter, e.g. `debug` |
| `MUNINN_RAW_DATA` | folder with the full raw CSVs for the ignored full-file tests |

## Exit status

`0` on success, `1` on any error (the message goes to stderr), `2` for invalid
command-line usage (clap).

## Examples

```
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/BTCUSDT_5m.csv --source binance -y
mnnn -q export --format csv --out - --symbol BTC/USDT --from 2026-10-01 | head
mnnn sources list
mnnn help keys
mnnn help --search shard
```

## Man pages

Man pages are generated from the same clap definitions, so they never drift:
`mnnn.1` plus one page per subcommand (`mnnn-ingest-csv.1`, `mnnn-keys-rotate.1`, ...).
They are committed under `man/` and regenerated with:

```
cargo run -- man --out man          # hidden subcommand
```

Install for your user and read:

```
mkdir -p ~/.local/share/man/man1
cp man/*.1 ~/.local/share/man/man1/
man mnnn
man mnnn-ingest-csv
```

See also [TUI](tui.md), [in-app help](help.md).
