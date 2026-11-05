# JSON ingest

```
mnnn ingest json --path <FILE|DIR> [--source NAME] [--force] [--no-recursive]
                 [--progress | --no-progress]
```

Stores `.json`, `.ndjson` and `.jsonl` files (a directory stores all of them,
recursively; `--no-recursive` for the top level only) exactly as they are, the same way
as [CSV ingest](ingest-csv.md): one item per file in a source (`--source`, else the
folder's name), skipped when the same bytes are already there unless `--force`. Nothing
is parsed and no bars are made; the format (`json`, `ndjson`) is only a hint from the
file name.

## Reading it

The built-in `parse-json` and `parse-ndjson` code spaces turn the stored text into
rows; `to-bars` turns rows into bars:

```
cat > /tmp/feed/BTCUSDT_5m.json <<'J'
{"data": {"bars": [
  {"t": "2017-08-17 04:00:00", "o": 4261.48, "h": 4280.56, "l": 4261.48, "c": 4261.48, "v": 2.189061}
]}}
J
mnnn ingest json --path /tmp/feed/BTCUSDT_5m.json          # source "feed"
mnnn code parsers --install
mnnn pipe source:feed 'code:parse-json?records=data.bars' \
  'code:to-bars?symbol=BTC/USDT&duration=5m&map=t:timestamp,o:open,h:high,l:low,c:close,v:volume' \
  --to model:bars -y
```

- **Array of objects**: keys become columns, in the order they first appear.
- **Array of arrays** (e.g. Binance klines `[openTime, open, high, low, close, …]`):
  columns `column_1`, `column_2`, …; map them in to-bars
  (`map=column_1:timestamp,column_2:open,…`).
- **Nested**: `parse-json?records=data.bars` (a dotted path or a JSON pointer `/data/bars`).
- **NDJSON**: `code:parse-ndjson`, one object a line.

`--records` on `ingest json` is still accepted but only warns: it is now the
`records` option of parse-json.

See also [pipes](pipes.md), [HTTP service](services/http.md).
