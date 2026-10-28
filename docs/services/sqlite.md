# SQLite service

```
mnnn ingest service sqlite --path FILE (--table NAME | --query SQL|@FILE) [--key KEY]
     [--timeout 30] [--source NAME] [--symbol PAIR] [--duration 5m] [--map COL=FIELD]...
```

| flag | default | notes |
|------|---------|-------|
| `--path` | required | opened **read-only** |
| `--table` / `--query` | one required | `--query @file.sql` reads the SQL from a file |
| `--key` | | passphrase if the source database is SQLCipher-encrypted |

Columns are matched by the usual aliases (or `--map`). Values of any SQLite type are
accepted. With `--table`, the table name is also used to derive the symbol and
duration (e.g. a table named `BTCUSDT_5m`). The source defaults to the file's stem when
`--source` is not given.

## Example

```
mnnn ingest service sqlite --path ~/data/klines.db --table BTCUSDT_5m --source binance
mnnn ingest service sqlite --path ~/data/klines.db \
  --query "SELECT open_time AS ts, o, h, l, c, v FROM klines WHERE pair = 'ETHUSDT'" \
  --map ts=timestamp --symbol ETH/USDT --duration 1m --source binance
```

## Failure modes

- Missing file: error; the file is never created.
- Unknown table / bad SQL: the SQLite error is shown.
- Encrypted file without `--key` (or a wrong key): "file is not a database".

See also [Postgres](postgres.md), [HTTP](http.md), [Redis](redis.md).
