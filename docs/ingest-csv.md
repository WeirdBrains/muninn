# CSV ingest

```
mnnn ingest csv --path <FILE|DIR> [--source NAME] [--force] [--no-recursive]
                [--progress | --no-progress]
```

Ingest stores data as it arrives. Each file is kept byte for byte in a **source**
(the `source_data` table); nothing is parsed, guessed or validated, and no bars are
made. What the data is, and what becomes of it, is decided by
[pipelines](pipelines.md) and their code spaces. A directory (searched recursively;
`--no-recursive` for the top level only, hidden entries skipped) stores every
`.csv`/`.tsv`/`.txt` file in it, sorted by name, as one run.

## Example with the raw data

```
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/binance_BTCUSDT_5m.csv --source binance
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data        # source: "data", the folder
```

Then make bars from it with a pipe (or a stored pipeline):

```
mnnn code parsers --install
mnnn pipe source:binance code:parse-csv 'code:to-bars?symbol=BTC/USDT&duration=5m' --to model:bars -y
```

## What ingest does

1. **Source**: `--source NAME` (created if unknown); else the source already holding
   this path; else the folder's name (`~/raw/kraken/x.csv` → `kraken`). Only a path with
   no folder name to use would ask, and never about anything else. The only other
   question ingest can ask is the database password.
2. **Store**: the file's bytes, its name, a format hint from the extension (`csv`), its
   path, size and BLAKE3 hash, and when it arrived. All or nothing per file.
3. **Skip**: the same bytes already in the source are skipped unless `--force`.
4. **Report**: `stored N bytes (csv) in source S as ID`. `mnnn sources data S` lists
   what a source holds; `mnnn sources cat S ID` writes one item back out, unchanged.

## Reading it: parse-csv and to-bars

`source:NAME` in a pipe or pipeline gives one row per stored item (columns `text`,
`name`, `format`, `locator`, `received_at`, `item`, `source`). The built-in code spaces
(`mnnn code parsers --install`) take it from there; options ride on the node:

| code space | option | default | notes |
|------------|--------|---------|-------|
| parse-csv | `delimiter` | `,` | one character, or `tab` |
| | `quote` | `"` | quotes are escaped by doubling (RFC 4180) |
| | `header` | `true` | `false`: columns are `column_1`, `column_2`, … |
| | `skip_rows` | `0` | lines skipped before the header |
| to-bars | `symbol` | a `symbol` (or `base`+`quote`) column | `BTC/USDT`, the pair for every row |
| | `source` | a `source`/`_source` column, else `pipeline` | the bars' source |
| | `duration` | a `period` column, else the most common step | `5m`, `1h`, `1d` or seconds |
| | `map` | by column name | `map=ts:timestamp,px:close` |
| | `rejects` | pass the good rows | `fail`: stop when any row is rejected |
| | `drop_zero_volume` | keep and flag | `true`: reject volume-0 bars |
| | `check` | `true` | `false`: no checks |

to-bars finds columns by name (`timestamp`/`ts`/`time`/`date`/`open_time`, `open`,
`high`, `low`, `close`, `volume`/`vol`), reads epoch s/ms/µs/ns or date text (UTC unless
an offset is given) and checks `high >= max(open, close)`, `low <= min(open, close)`,
`volume >= 0` and alignment to the duration. Rejected rows are its second output table,
`Rejected` (row, reason); they never reach the bars. The `model:bars` leaf adds only bars
not there yet, so a pipeline can run again after more data arrives.

## Old flags

`--symbol`, `--duration`, `--map`, `--rejects`, `--delimiter`, `--quote`, `--escape`,
`--no-header` and `--skip-rows` are still accepted so scripts keep running, but each
only prints a one-line warning naming its new home, e.g.

```
warning: --symbol is ignored: ingest stores data as it arrives; it is now an option of the to-bars code space (code:to-bars?symbol=BTC/USDT), see docs/pipes.md
```

## Progress

Large ingests show progress on stderr (stdout is untouched). On a terminal it is on by
default and redraws one line per running file, then prints a summary line:

```
[1/3] BTC_5m_combined_part1.csv   42% · 180.2 MiB/429.0 MiB · 18.0 MiB/s · ETA 0:12 · overall 14%
done BTC_5m_combined_part1.csv: 429.0 MiB stored in 21.1s (20.3 MiB/s)
```

`--progress` forces it on when stderr is not a terminal (logs, CI): then it prints a
`[i/n] file (size)` line when each file starts, a plain line every 5 seconds and the
summary line. `--no-progress` (or `-q`) turns it off. The same flags work on
`ingest json` and `ingest path`.

## Failure modes

- No source and no folder name to use, with `--non-interactive` or no TTY: an error
  naming `--source`. Nothing is written.
- Unreadable file: an error; earlier files of the run stay stored.
- Locked or unreadable database: see [encryption](encryption.md).

See also [JSON ingest](ingest-json.md), [pipes](pipes.md), [services](services/http.md), [CLI](cli.md).
