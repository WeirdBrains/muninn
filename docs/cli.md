# Command line

```
mnnn [GLOBAL FLAGS] <COMMAND>

  ingest csv|json|path|service <http|redis|sqlite|postgres>
  export
  sources  list|add|rename|remove
  symbols  list|add
  keys     init|status|rotate|rekey|export|recover|shards
  backup   create|restore
  theme    list|show|set|mode|export|import|reset
  author   list|show|add|edit|remove|use
  history  list|show|tree|back|forward|goto
  search   list|show|run|find|save|fork|edit|remove
  project  list|create|show|edit|remove|add|drop|export|import
  journal  list|new|show|edit|remove|fork|write|entry|edit-entry|remove-entry|copy|
           note|highlight|edit-note|remove-note|attach|image|export|import|print
  report   list|create|show|edit|remove|add|edit-section|remove-section|move|export|preview
  rule     list|create|show|edit|remove|enable|disable|test|dry-run|events
  model    list|types|create|edit|show|plan|apply|discard|history|rollback|remove|
           record create|update|list|show|delete
  code     list|create|edit|show|remove|approve|build|run|check|sdk
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
| `MUNINN_CARGO` | cargo used to build [code spaces](code-spaces.md) (default: `cargo` on `PATH`) |
| `MUNINN_CODE_TARGET` | cargo target directory for code space builds (default: `<home>/code/target`) |
| `MUNINN_RYTHONC` | the `rythonc` binary for Python code spaces (default: `rythonc` on `PATH`) |

## Exit status

`0` on success, `1` on any error (the message goes to stderr), `2` for invalid
command-line usage (clap).

## Examples

```
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/binance_BTCUSDT_5m.csv --source binance -y
mnnn -q export --format csv --out - --symbol BTC/USDT --from 2026-10-01 | head
mnnn sources list
mnnn author add --name "Ada Lovelace" --email ada@example.org
mnnn history tree
mnnn model create trades --fields "symbol:text!; price:real; side:text choices=buy|sell"
mnnn model record list trades --where "price > 3000" --sort -price
mnnn code create fast --lang math --model bars --group symbol --source "fast = sma(close, 10)"
mnnn code run fast -y
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

## Importing a whole path

```sh
mnnn ingest path ~/Projects/kommandosaldat/raw/data -y              # source from each name (binance_…, coinbase_…)
mnnn ingest path ~/Downloads/export --source coinbase -y           # set it explicitly
mnnn ingest path ./exports --no-recursive --records data.bars
```

`mnnn ingest path <PATH>` (or `--path`) ingests every CSV-like (`.csv .tsv .txt`) and
JSON-like (`.json .ndjson .jsonl`) file under a directory, recursively; hidden entries
are skipped. CSV options: `--delimiter`, `--no-header`; JSON: `--records`. Each file
prints `[i/n] relative/path` on stderr, and on a terminal a live `file % · overall %`
line. Each file is its own transaction; Ctrl-C keeps finished files, rolls back the
current one and is recorded in the run ledger (`mnnn ingest runs`). `ingest csv` and
`ingest json` also search directories recursively now; `--no-recursive` limits any
of them to the top level.
