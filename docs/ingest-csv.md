# CSV ingest

```
mnnn ingest csv --path <FILE|DIR> [--source NAME] [--symbol PAIR] [--duration 5m]
                [--delimiter ,] [--quote '"'] [--escape C] [--no-header] [--skip-rows 0]
                [--map COL=FIELD]... [--rejects FILE] [--force]
```

Every row becomes a bar (see [data model](data-model.md)). A directory ingests every
`.csv`/`.tsv`/`.txt` file in it, sorted by name, as one run.

## Example with the raw data

```
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/BTCUSDT_5m.csv --source binance -y
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data --source binance -y
```

The file name gives the symbol (`BTCUSDT` -> `BTC/USDT`) and the duration (`_5m`).

## Pipeline

1. **Read** with Polars. All columns are read as text, then parsed.
2. **Columns** are matched by name, case-insensitively, against aliases:
   `timestamp_utc`, `timestamp`, `open_time`, `time`, `date`, `datetime`, `ts` ...;
   `open`/`o`, `high`/`h`, `low`/`l`, `close`/`c`, `volume`/`vol`/`v`. Override with
   `--map <col>=<field>`, e.g. `--map ts=timestamp`. With `--no-header`, columns are
   taken in the order timestamp, open, high, low, close, volume.
3. **Source**: `--source` (created if unknown), else a source whose locator equals the
   path, else a prompt.
4. **Symbol**: `--symbol` (`BTC/USDT`, `BTC-USD` or `BTCUSDT`), else parsed from the file
   name against the known symbol list (longest quote suffix wins, aliases like `XBT`
   applied), else a prompt. A prompted answer is reused for the rest of the run.
5. **Timestamp**: epoch seconds / ms / us / ns (detected by magnitude), RFC 3339 or
   `YYYY-MM-DD HH:MM:SS`. UTC unless an offset is present. Stored as epoch ms.
6. **Duration**: `--duration`, else a file-name token (`5m`, `1h`, `15min`), else the
   **mode** of consecutive timestamp differences across the whole file, so gaps don't
   matter. A warning is printed when the mode covers less than 90% of the differences.
7. **Validate**: `high >= max(open, close)`, `low <= min(open, close)`, `volume >= 0`,
   no missing / non-numeric values, timestamp aligned to the duration (for durations up
   to 1d). Rejected rows never reach the database. Zero-volume bars are kept and flagged
   unless `ingest.drop_zero_volume` is set ([preferences](preferences.md)).
8. **Upsert** in batched transactions on the bar id. Re-ingesting is a no-op; an
   identical file (same BLAKE3 content hash, source and series) is skipped outright
   unless `--force`.
9. **Report**: rows, inserted, duplicate, rejected, zero-volume, gaps and missing bars,
   coverage range.

## Flags and defaults

| flag | default | notes |
|------|---------|-------|
| `--path`, `-p` | required | file or directory |
| `--delimiter` | `,` | single ASCII character |
| `--quote` | `"` | |
| `--escape` | none | only RFC 4180 doubling (the quote character) is supported |
| `--no-header` | header on | |
| `--skip-rows` | `0` | lines skipped before the header |
| `--rejects FILE` | | CSV of rejected rows: input, row, timestamp, reason |
| `--force` | off | re-process an identical input |

## Failure modes

- Missing source / symbol / duration with `--non-interactive` or no TTY: error naming
  the flag to pass. Nothing is written.
- Rows fail validation: a confirmation asks whether to drop them. Without a TTY this is
  an error unless `-y/--yes` is given.
- A required column cannot be found: error listing the columns present and a `--map` hint.
- Locked or unreadable database: see [encryption](encryption.md).

See also [JSON ingest](ingest-json.md), [services](services/http.md), [CLI](cli.md).
