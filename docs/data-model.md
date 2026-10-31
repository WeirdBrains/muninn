# Data model

Everything Muninn ingests becomes **bars** (OHLCV candles) stored in one SQLite
database (encrypted, see [encryption](encryption.md)). Your own record tables are
[models](#models-and-records).

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
- `live_searches`: name, target (series, journals or records), the search as JSON, the search it
  was forked from, author, times ([live searches](live-searches.md)). Results are never
  stored; a live search runs on the current data.
- `projects` and `project_items`: a project's name and description, and its items by
  kind (`live_search`, `result`, `journal`, `report`, `rule_set`, `model`) and reference; a result's series and time range are in the item's payload
  ([projects](projects.md)).
- `journals`, `journal_entries`, `journal_annotations`, `journal_attachments`: journals
  (standalone or in a project, default format, forked from), entries (title, text,
  Markdown or LaTeX, position), notes and highlights (on the journal, an entry or a
  character range of an entry with the quoted words) and images (encrypted with the
  rest of the database) ([journals](journals.md)). Every row carries its author.
- `reports` and `report_sections`: a report's name and description, and its sections in
  order with their kind (`text`, `records`, `series`, `journal`, `project`, `authors`,
  `code`, `model`), title, text and format, and what they show as JSON (series and range, live
  search, journal and entries, project, code space, model and filter) ([reports](reports.md)).
- `rules`: name, description, home (standalone, journal, live search or model), on/off, the
  spec as JSON (on, if, then), author, times, and run count, last run and status
  ([rules](rules.md)).
- `schema_version`: migrations, applied automatically on open. Version 4 adds
  `authors` and `settings`, version 5 `actions`, version 6 `live_searches`, version 7
  `projects` and `project_items`, version 8 the journal tables, version 9 `reports` and
  `report_sections`; project items may now be reports; version 10 `rules`; version 11
  `models` and `model_versions` (with the built-in bars model). All of them only add
  tables, so older databases upgrade in place on the first open.

## Models and records

[Models](models.md) are record tables you design. `models` holds each model's name,
description, applied version, design and draft as JSON, author and times. `model_versions`
holds every applied version's design, migration steps, note, the rule that made it (if
any), author and time. A model's records live in their own table, `m_NAME`, with `id`,
`_version`, `_created_at`, `_updated_at`, `_author` and one column per field. Text, JSON
and refs are TEXT; integers, bools and timestamps (epoch ms) are INTEGER; reals are REAL.
Required fields are `NOT NULL`, and unique and indexed fields get their own indexes.
These tables are created and rebuilt only by migrations.

The bars are the records of the built-in `bars` model and live in its table, `m_bars`,
like any model's: the bars' own columns (`_base`, `_quote`, `_timestamp`, `_open` …
`_volume`, `_source_id`, `_duration_s`, `_flags`) next to a column per field added to
it, so removing a bar removes its values too. A bar's id is the BLAKE3 hash of its
timestamp, source, symbol and period. Reads go through the `bar_rows` view (the bars'
columns under their plain names). Upgrading to 2.16 moves the old `bars` and `mx_bars`
tables into `m_bars` in one transaction, keeping every id, value, stamp and version.

## Decisions

- Prices are REAL (f64), not scaled integers.
- Zero-volume bars are kept and flagged; set `ingest.drop_zero_volume = true` in
  [preferences](preferences.md) to drop them.

See also: [storage layout](storage-layout.md), [export](export.md).
