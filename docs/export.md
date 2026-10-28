# Export

```
mnnn export --format <csv|parquet|json> --out <PATH|-> [--symbol PAIR] [--source NAME]
            [--duration 5m] [--from TIME] [--to TIME]
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
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/ETHUSDT_5m.csv --source binance -y
mnnn export --format parquet --out /tmp/eth.parquet --symbol ETH/USDT
mnnn export --format csv --out - --symbol ETH/USDT --from 2024-01-01 --to 2024-01-31 | head
mnnn -q export --format json --out - --source binance --duration 5m > bars.json
```

An exported CSV can be ingested again (the `timestamp` column is recognized).

## Failure modes

- `--format parquet --out -`: error, parquet needs a file.
- Unparseable `--from`/`--to`/`--symbol`/`--duration`: error naming the flag.
- No matching bars: an empty file with only the header (CSV) or `[]` (JSON).

See also [data model](data-model.md), [TUI](tui.md).
