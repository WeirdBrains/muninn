# Redis service

```
mnnn ingest service redis --uri HOST|redis://...|rediss://... (--key KEY | --pattern GLOB)
     [--port 6379] [--username USER] [--password P | --password-env VAR | --password-stdin]
     [--db-index 0] [--tls] [--type auto|stream|list|zset|hash|string]
     [--value-format json|csv|fields] [--from ID|SCORE] [--to ID|SCORE] [--batch 1000]
     [--timeout 30] [--source NAME] [--symbol PAIR] [--duration 5m] [--map COL=FIELD]...
```

| flag | default | notes |
|------|---------|-------|
| `--uri` | required | host name or URI; `rediss://` implies TLS |
| `--port` | 6379 | ignored when the URI has a port |
| `--username` / `--password` | | Redis 6 ACL; prefer `--password-env` or `--password-stdin` |
| `--db-index` | 0 | logical database (`--db` is the global database-file flag) |
| `--key` / `--pattern` | one required | `--pattern` uses `SCAN MATCH`; each key is ingested separately and its name is used to derive the symbol and duration |
| `--type` | `auto` | detected with `TYPE` |
| `--value-format` | `json` | entry encoding: JSON object/array, CSV line `ts,o,h,l,c,v`, or stream/hash `fields` |
| `--from` / `--to` | full range | stream ids (`XRANGE`) or scores (`ZRANGEBYSCORE`) |
| `--batch` | 1000 | `COUNT` for XRANGE/SCAN |

How each type is read:

| type | read with | each record |
|------|-----------|-------------|
| stream | `XRANGE key from to COUNT n` (paged) | the entry's fields (`fields`) or its single `json`/`csv` field |
| list | `LRANGE 0 -1` | each element |
| zset | `ZRANGEBYSCORE` | each member; the score is used as the timestamp if no timestamp column exists |
| hash | `HGETALL` | each value; the field name is used as the timestamp if no timestamp column exists |
| string | `GET` | a JSON array, NDJSON or CSV document |

## Example

```
mnnn ingest service redis --uri localhost --pattern 'bars:binance:*_5m' --type stream \
  --value-format fields --source binance --password-env REDIS_PASSWORD
```

## Failure modes

- Neither or both of `--key`/`--pattern`: usage error.
- Connection refused / auth failure: error with the host (the password is never printed).
- A key of an unexpected type with `--type` set: error naming the key and its type.

See also [HTTP](http.md), [SQLite](sqlite.md), [Postgres](postgres.md).
