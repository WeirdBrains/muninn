# JSON ingest

```
mnnn ingest json --path <FILE|DIR> [--records PATH] [--source NAME] [--symbol PAIR]
                 [--duration 5m] [--map COL=FIELD]... [--rejects FILE] [--force]
```

Reads `.json`, `.ndjson` and `.jsonl` files (a directory ingests all of them). After
reading, the pipeline is the same as [CSV ingest](ingest-csv.md): source, symbol,
duration derivation, validation, idempotent upsert and the report.

## Shapes

- **Array of objects**: keys become columns and are matched by the usual aliases
  (`timestamp`/`time`/`t`, `open`/`o`, ...). Override with `--map`.
- **Array of arrays** (e.g. Binance klines `[openTime, open, high, low, close, volume, ...]`):
  the first six positions are timestamp, open, high, low, close, volume.
- **NDJSON** (`.ndjson`/`.jsonl`): one object (or array) per line.
- **Nested**: point `--records` at the array: a dotted path `data.bars`, a JSON pointer
  `/data/bars`, or `$.data.bars`.

Numbers and numeric strings are both accepted; timestamps may be epoch s/ms/us/ns or
date strings.

## Example

```
cat > /tmp/BTCUSDT_5m.json <<'J'
{"data": {"bars": [
  {"t": "2017-08-17 04:00:00", "o": 4261.48, "h": 4280.56, "l": 4261.48, "c": 4261.48, "v": 2.189061}
]}}
J
mnnn ingest json --path /tmp/BTCUSDT_5m.json --records data.bars --source demo
```

## Failure modes

- `--records` path not found or not an array: an error naming the segment.
- Mixed objects and arrays: "records must all be objects or all be arrays".
- Bad NDJSON line: the line number is reported.

See also [HTTP service](services/http.md), which uses the same JSON handling.
