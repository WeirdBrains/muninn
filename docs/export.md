# Export

```
mnnn export --format <csv|parquet|json|pdf|html|docx|latex|text> --out <PATH|-> [--symbol PAIR]
            [--source NAME] [--duration 5m] [--from TIME] [--to TIME]
```

Writes stored bars through Polars. `--out -` writes CSV or JSON to stdout (parquet
needs a file). `--from`/`--to` accept the same formats as ingest timestamps (epoch or
`YYYY-MM-DD HH:MM:SS`, UTC) and are inclusive.

Columns: `timestamp` (epoch ms, open time), `datetime_utc`, `source`, `symbol`,
`base`, `quote`, `duration`, `duration_s`, `open`, `high`, `low`, `close`, `volume`,
`zero_volume`. Rows are ordered by source, symbol, duration, then time.

JSON is an array of objects, one per bar.

## Examples

```
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/binance_ETHUSDT_5m.csv --source binance -y
mnnn export --format parquet --out /tmp/eth.parquet --symbol ETH/USDT
mnnn export --format csv --out - --symbol ETH/USDT --from 2024-01-01 --to 2024-01-31 | head
mnnn -q export --format json --out - --source binance --duration 5m > bars.json
```

## As a document

`--format pdf|html|docx|latex|text` writes the matching bars as a document: per series
a summary (bars, first and last, open, close, high, low, change, volume) and a table of
every bar. `--out -` works for text, HTML and LaTeX. These are made by the built-in
document layer; see [export formats](export-formats.md).

An exported CSV can be ingested again (the `timestamp` column is recognized).

## Failure modes

- `--format parquet --out -`: error, parquet needs a file.
- Unparseable `--from`/`--to`/`--symbol`/`--duration`: error naming the flag.
- No matching bars: an empty file with only the header (CSV) or `[]` (JSON).

See also [data model](data-model.md), [export formats](export-formats.md), [TUI](tui.md).
