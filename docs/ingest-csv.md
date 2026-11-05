# CSV ingest

```
mnnn ingest csv --path <FILE|DIR> [--source NAME] [--symbol PAIR] [--duration 5m]
                [--delimiter ,] [--quote '"'] [--escape C] [--no-header] [--skip-rows 0]
                [--map COL=FIELD]... [--rejects FILE] [--force] [--progress | --no-progress]
```

Every row becomes a bar (see [data model](data-model.md)). A directory (searched recursively; `--no-recursive` for the top level only, hidden entries skipped) ingests every
`.csv`/`.tsv`/`.txt` file in it, sorted by name, as one run.

## Example with the raw data

```
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/binance_BTCUSDT_5m.csv --source binance -y
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
3. **Source** (where the data originally came from, even for uploaded copies):
   `--source NAME` (created if unknown) on `ingest csv`, `ingest json` and `ingest path`;
   else an exchange named in the file name (`coinbase_BTC-USD_5m.csv` → `coinbase`;
   binance, coinbase/gdax, kraken, bitstamp, bitfinex, bybit, okx, kucoin, gemini, …),
   per file, so a folder of mixed exchanges lands in the right sources; else a source
   whose locator equals the path, else a prompt.
4. **Symbol**: `--symbol` (`BTC/USDT`, `BTC-USD` or `BTCUSDT`), else parsed from the file
   name against the known symbol list (longest quote suffix wins, aliases like `XBT`
   applied), else a prompt. The symbol is the traded pair: the asset, a slash, then the
   currency it is priced in, e.g. `BTC/USDT` or `BTC/USD`. The prompt says so and, when
   the file name names an asset (`BTC_5m_combined.csv`), uses it in the example:

   ```
   Symbol for BTC_5m_combined_part1.csv: the traded pair as BASE/QUOTE, e.g. BTC/USDT or BTC/USD: BTC
   warning: `BTC` is only one asset: add the currency it is priced in, e.g. BTC/USDT or BTC/USD
   Symbol for BTC_5m_combined_part1.csv: the traded pair as BASE/QUOTE, e.g. BTC/USDT or BTC/USD: BTC/USDT
   ```

   A bad answer asks again (up to 5 times; an empty answer or end of input stops the
   run). A prompted answer is reused for the rest of the run. To skip the prompt, pass
   `--symbol BTC/USDT`; it applies to every file in the run.
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
9. **Progress** (stderr only; stdout is untouched): see [Progress](#progress).
10. **Report**: rows, inserted, duplicate, rejected, zero-volume, gaps and missing bars,
   coverage range.

## Progress

Large ingests show progress on stderr. On a terminal it is on by default and redraws one
line per running file, then prints a summary line when the file is done:

```
[1/3] BTC_5m_combined_part1.csv   42% · 180.2 MiB/429.0 MiB · 18.0 MiB/s · ETA 1:02 · overall 14%
done BTC_5m_combined_part1.csv: 1,234,567 bars in 42.1s (29,324 bars/s)
```

The percentage covers reading, parsing and storing (storing dominates), measured
against the file size. `--progress` forces it on when stderr is not a terminal (logs,
CI): then it prints a `[i/n] file (size)` line when each file starts, a plain progress
line every 5 seconds and the summary line. `--no-progress` (or `-q`) turns it off.
Progress pauses while a prompt is waiting. The same flags work on `ingest json` and
`ingest path`.

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
