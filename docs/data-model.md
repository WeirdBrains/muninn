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
- `live_searches`: name, target (series or journals), the search as JSON, the search it
  was forked from, author, times ([live searches](live-searches.md)). Results are never
  stored; a live search runs on the current data.
- `projects` and `project_items`: a project's name and description, and its items by
  kind (`live_search`, `result`, `journal`, and `rule_set` reserved for a later
  release) and reference; a result's series and time range are in the item's payload
  ([projects](projects.md)).
- `journals`, `journal_entries`, `journal_annotations`, `journal_attachments`: journals
  (standalone or in a project, default format, forked from), entries (title, text,
  Markdown or LaTeX, position), notes and highlights (on the journal, an entry or a
  character range of an entry with the quoted words) and images (encrypted with the
  rest of the database) ([journals](journals.md)). Every row carries its author.
- `schema_version`: migrations, applied automatically on open. Version 4 adds
  `authors` and `settings`, version 5 `actions`, version 6 `live_searches`, version 7
  `projects` and `project_items`, version 8 the journal tables. All of them only add
  tables, so older databases upgrade in place on the first open.

## Decisions

- Prices are REAL (f64), not scaled integers.
- Zero-volume bars are kept and flagged; set `ingest.drop_zero_volume = true` in
  [preferences](preferences.md) to drop them.

See also: [storage layout](storage-layout.md), [export](export.md).
