# Postgres service

```
mnnn ingest service postgres (--url URL | --username USER --host HOST [--port 5432]
     [--database postgres] [--secure]) [--password P | --password-env VAR | --password-stdin]
     [--ca-cert FILE] [--schema public] (--table NAME | --query SQL|@FILE) [--batch 10000]
     [--timeout 30] [--source NAME] [--force]
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

Each row is fetched as JSON (`row_to_json`), so any column types work. The source
defaults to the database host when `--source` is not given.

## What is stored

Ingest stores what the service returns as it is, in a source, and makes no bars:
the rows of the table or query, as NDJSON (one object a row), one item per read. `mnnn sources data NAME` lists the items. Make bars (or anything else) with a pipe:
`mnnn pipe source:NAME code:parse-ndjson 'code:to-bars?symbol=BTC/USDT&duration=5m' --to model:bars`
([pipes](../pipes.md)). `--symbol`, `--duration` and `--map` are still accepted but only
warn: they are options of the to-bars code space now.

## Example

```
mnnn ingest service postgres --username quant --host db.local --database market \
  --password-env PGPASSWORD --table btcusdt_5m --source binance
```

## Failure modes

- `--url` together with `--host` etc.: usage error.
- Neither `--table` nor `--query`: usage error.
- TLS required by the server without `--secure`: "prefer" negotiates TLS if offered.
- Authentication failure: reported without echoing the password.

See also [SQLite](sqlite.md), [HTTP](http.md), [Redis](redis.md).
