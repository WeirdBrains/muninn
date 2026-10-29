# Data model

Everything Muninn ingests becomes **bars** (OHLCV candles) stored in one SQLite
database (encrypted, see [encryption](encryption.md)).

## Bars

| column | type | notes |
|--------|------|-------|
| `id` | BLOB PK | 32-byte BLAKE3 hash, see below |
| `base` | TEXT | e.g. `BTC` |
| `quote` | TEXT | e.g. `USDT`, `USD` (kept separate: Binance USDT is not Coinbase USD) |
| `timestamp` | INTEGER | UTC epoch **milliseconds**, bar **open** time |
| `open`, `high`, `low`, `close` | REAL | f64 |
| `volume` | REAL | base-asset volume |
| `source_id` | TEXT | references `sources.id` |
| `duration_s` | INTEGER | seconds; shown as `5m`, `1h`, `1d` |
| `flags` | INTEGER | bit 1 = zero-volume bar |

The UI shows a symbol as `base/quote`, for example `BTC/USDT`.

## Bar id

```
id = BLAKE3("{timestamp_ms}|{source.name}|{base}/{quote}|{duration_s}")
```

- The duration is part of the id, so a 5m bar and a 1h bar at the same time never collide.
- `|` separators prevent ambiguous concatenations (source names may not contain `|`).
- The source **name** (not its uuid) is hashed so ids are stable across databases.
  Renaming a source recomputes all of its bar ids in one transaction
  (`mnnn sources rename old new`).

## Sources

`sources(id uuid, name unique, kind, locator, created_at)`. `kind` is `file`,
`directory`, `url` or `service`; `locator` is the path or URL it came from.

```
mnnn sources list
mnnn sources add binance --kind file --locator ~/Projects/kommandosaldat/raw/data
mnnn sources rename binance binance-spot
mnnn sources remove binance-spot      # asks for confirmation, -y to skip
```

## Symbols and aliases

The known symbol list seeds common base assets (BTC, ETH, SOL, ...) and quote
currencies (USDT, USDC, USD, EUR, ...), plus aliases such as `XBT -> BTC`. It is
used to derive a symbol from a file name (see [CSV ingest](ingest-csv.md)).

```
mnnn symbols list
mnnn symbols add FOO/USD --alias FO
```

## Other tables

- `ingest_runs`: content hash, source, series, counts. An identical input is
  skipped the next time (use `--force` to re-process).
- `authors`: name, email, details (JSON); see [authors](authors.md).
- `actions`: the [action history](history.md) tree (parent, kind, summary, params,
  origin, author, time, view state before and after).
- `settings`: small key/value pairs (current author, history head).
- `schema_version`: migrations, applied automatically on open. Version 4 adds
  `authors` and `settings`, version 5 `actions`; both only add tables, so 1.x
  databases upgrade in place on the first open by 2.x.

## Decisions

- Prices are REAL (f64), not scaled integers.
- Zero-volume bars are kept and flagged; set `ingest.drop_zero_volume = true` in
  [preferences](preferences.md) to drop them.

See also: [storage layout](storage-layout.md), [export](export.md).
