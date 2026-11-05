# SQLite service

```
mnnn ingest service sqlite --path FILE (--table NAME | --query SQL|@FILE) [--key KEY]
     [--timeout 30] [--source NAME] [--force]
```

| flag | default | notes |
|------|---------|-------|
| `--path` | required | opened **read-only** |
| `--table` / `--query` | one required | `--query @file.sql` reads the SQL from a file |
| `--key` | | passphrase if the source database is SQLCipher-encrypted |

The source defaults to the file's stem when `--source` is not given.

## What is stored

Ingest stores what the service returns as it is, in a source, and makes no bars:
the rows of the table or query, as NDJSON (one object a row, values typed as SQLite has them; blobs as hex), one item per read. `mnnn sources data NAME` lists the items. Make bars (or anything else) with a pipe:
`mnnn pipe source:NAME code:parse-ndjson 'code:to-bars?symbol=BTC/USDT&duration=5m' --to model:bars`
([pipes](../pipes.md)). `--symbol`, `--duration` and `--map` are still accepted but only
warn: they are options of the to-bars code space now.

## Example

```
mnnn ingest service sqlite --path ~/data/klines.db --table BTCUSDT_5m --source binance
mnnn ingest service sqlite --path ~/data/klines.db \
  --query "SELECT open_time AS ts, o, h, l, c, v FROM klines WHERE pair = 'ETHUSDT'" \
  --source binance
mnnn pipe source:binance code:parse-ndjson 'code:to-bars?symbol=ETH/USDT&duration=1m&map=ts:timestamp,o:open,h:high,l:low,c:close,v:volume' --to model:bars -y
```

## Failure modes

- Missing file: error; the file is never created.
- Unknown table / bad SQL: the SQLite error is shown.
- Encrypted file without `--key` (or a wrong key): "file is not a database".

See also [Postgres](postgres.md), [HTTP](http.md), [Redis](redis.md).
