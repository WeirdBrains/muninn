# Postgres service

```
mnnn ingest service postgres (--url URL | --username USER --host HOST [--port 5432]
     [--database postgres] [--secure]) [--password P | --password-env VAR | --password-stdin]
     [--ca-cert FILE] [--schema public] (--table NAME | --query SQL|@FILE) [--batch 10000]
     [--timeout 30] [--source NAME] [--symbol PAIR] [--duration 5m] [--map COL=FIELD]...
```

| flag | default | notes |
|------|---------|-------|
| `--url` | | `postgres://user:pass@host:port/db?sslmode=...`; conflicts with the connection flags |
| `--username` | required unless `--url` | |
| `--password` | empty | prefer `--password-env` / `--password-stdin` |
| `--host` | required unless `--url` | |
| `--port` | 5432 | |
| `--database` | `postgres` | |
| `--secure` | off | on: `sslmode=require`; off: `sslmode=prefer` |
| `--ca-cert` | | verify the server certificate and host name (verify-full) |
| `--schema` | `public` | used with `--table` |
| `--table` / `--query` | one required | |
| `--batch` | 10000 | rows per fetch from a server-side cursor |

Each row is fetched as JSON (`row_to_json`), so any column types work. With `--table`
the table name is used to derive the symbol and duration. The source defaults to the
database host when `--source` is not given.

## Example

```
mnnn ingest service postgres --username quant --host db.local --database market \
  --password-env PGPASSWORD --table btcusdt_5m --source binance --symbol BTC/USDT
```

## Failure modes

- `--url` together with `--host` etc.: usage error.
- Neither `--table` nor `--query`: usage error.
- TLS required by the server without `--secure`: "prefer" negotiates TLS if offered.
- Authentication failure: reported without echoing the password.

See also [SQLite](sqlite.md), [HTTP](http.md), [Redis](redis.md).
