# HTTP service

```
mnnn ingest service http --url URL [-X METHOD] [--body STRING|@FILE] [--param K=V]...
     [-H "K: V"]... [--format json|csv|ndjson] [--records PATH]
     [--paginate none|cursor|time] [--cursor-path next] [--cursor-param cursor]
     [--start-param startTime] [--from T] [--to T] [--max-pages 1000]
     [--rate-limit REQ_PER_S] [--retries 3] [--insecure] [--timeout 30]
     [--source NAME] [--symbol PAIR] [--duration 5m] [--map COL=FIELD]...
```

| flag | default | notes |
|------|---------|-------|
| `--url` | required | |
| `-X`, `--method` | `GET` | GET, POST, PUT, PATCH, DELETE, HEAD |
| `--body` | | only with POST/PUT/PATCH (rejected otherwise); `@file` reads a file |
| `--param K=V` | | repeatable, URL-encoded and appended to the query string |
| `-H "K: V"` | | repeatable; overrides the default headers |
| `--format` | from `Content-Type` | json, csv or ndjson |
| `--records` | root | where the array of bars lives (see [JSON ingest](../ingest-json.md)) |
| `--paginate` | `none` | `cursor`: read `--cursor-path` from each response and send it as `--cursor-param`; `time`: advance `--start-param` past the last bar until `--to` or an empty page |
| `--rate-limit` | | requests per second |
| `--retries` | 3 | exponential backoff on 429 and 5xx |
| `--insecure` | off | skip TLS verification (prints a warning) |

Default headers, always sent unless you override them:

```
User-Agent: muninn/<version> (+https://github.com/kommandosaldat/muninn)
Accept: application/json, text/csv;q=0.9
```

The source defaults to the URL's host when `--source` is not given and nothing is
known for the URL; the symbol and duration must come from flags (or prompts) unless
the URL path carries them (e.g. `.../BTCUSDT_5m.json`).

## Example

```
mnnn ingest service http --url https://data-api.binance.vision/api/v3/klines \
  --param symbol=BTCUSDT --param interval=5m --param limit=1000 \
  --paginate time --start-param startTime --from 2024-01-01 --to 2024-01-02 \
  --source binance --symbol BTC/USDT --duration 5m
```

## Failure modes

- `--body` with GET: rejected before any request.
- Non-2xx after retries: error with status and the first bytes of the body.
- Redirect loops, TLS errors and timeouts are reported with the URL.

See also [Redis](redis.md), [SQLite](sqlite.md), [Postgres](postgres.md).
