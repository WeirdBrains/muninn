# Muninn — Plan

A data ingestor and normalizer that takes data from cryptocurrency exchanges and
normalizes it into bars stored in an encrypted SQLite database.

Binary: `mnnn` (crate `muninn`). Rust >= 1.99, edition 2024.

## 1. Interfaces

### 1.1 CLI (clap, derive)

Subcommand tree:

```
mnnn ingest csv     --path <FILE|DIR> [--source <NAME>] [--symbol <PAIR>] [--duration <5m>]
                    [--delimiter ,] [--quote '"'] [--escape <C>] [--no-header] [--skip-rows 0]
mnnn ingest json    --path <FILE|DIR> [--source ..] [--symbol ..] [--duration ..]
mnnn ingest service <http|redis|sqlite|postgres> [service flags, see 1.1.1] [--source ..] [--symbol ..] [--duration ..]
mnnn export         --format <csv|parquet|json> --out <PATH> [--symbol ..] [--source ..] [--from ..] [--to ..] [--duration ..]
mnnn sources        list | add | rename | remove
mnnn symbols        list | add
mnnn keys           init | rotate | export | recover | shards
mnnn backup         create --out <DIR> | restore --from <DIR>
mnnn theme          list | set <NAME> | mode <light|dark|system> | export <NAME> --out <FILE> | import <FILE>
mnnn tui
```

Global flags: `--home <DIR>` (default `~/.muninn`, env `MUNINN_HOME`), `--db <PATH>`, `--non-interactive`, `-v/--verbose`,
`-q/--quiet`, `-y/--yes`.

- `-q/--quiet` — print nothing but errors (and requested data on stdout, e.g. exports to
  `-`). Suppresses progress bars and summaries. Conflicts with `-v`. Exit code still
  reports success/failure.
- `-y/--yes` — answer "yes" to every confirmation (overwrite theme on restore, rotate
  service key, rekey, re-issue shards, write keys to one directory, drop rejected rows).
  It does **not** invent missing values: a missing source/symbol/duration still prompts,
  or errors under `--non-interactive`.
- `MUNINN_QUIET` / `MUNINN_YES` env equivalents.

Rules:
- Every value that could be prompted for also has a flag.
- `--non-interactive`, or no TTY on stdin, means missing values are an error, never a prompt
  (so tests and scripts never hang).
- No `unwrap()` in arg parsing; rely on clap's native parsers for `char`, `bool`, `usize`,
  `PathBuf` and `ValueEnum`. Don't use `-h` for anything but help.
- Sensible defaults: delimiter `,`, quote `"`, header on, skip 0, escape none.

### 1.1.1 Service flags

Each service is its own subcommand with its own flag struct (clap `Subcommand` +
`Args`), so flags are validated per service. Shared by all: `--source`, `--symbol`,
`--duration`, `--map <col>=<field>` (repeatable column mapping), `--timeout <secs>`
(default 30).

Secrets (`--password`) can also come from `--password-env <VAR>`, `--password-stdin`,
or an interactive hidden prompt, since a literal flag value ends up in shell history
and `ps`. Secrets are never logged and are redacted in `-v` output.

**http** — `mnnn ingest service http`

| flag | required | default | notes |
|------|----------|---------|-------|
| `--url <URL>` | yes | | |
| `-X, --method <GET\|POST\|...>` | | `GET` | |
| `--body <STRING\|@FILE>` | only with POST/PUT/PATCH | | rejected for GET |
| `--param <KEY=VALUE>` | | | repeatable; appended to the query string (URL-encoded) |
| `-H, --header <KEY: VALUE>` | | | repeatable; builds a header map, overrides defaults |
| `--format <json\|csv\|ndjson>` | | from `Content-Type` | |
| `--records <JSONPATH>` | | root | where the array of bars lives in the response |
| `--paginate <none\|cursor\|time>` | | `none` | e.g. walk `startTime`/`endTime` windows |
| `--rate-limit <req/s>` | | | |
| `--retries <N>` | | 3 | exponential backoff on 429/5xx |
| `--insecure` | | false | skip TLS verification (warns) |

Default headers (always sent unless overridden):
`User-Agent: muninn/<version> (+https://github.com/kommandosaldat/muninn)`,
`Accept: application/json, text/csv;q=0.9`.

**redis** — `mnnn ingest service redis`

| flag | required | default | notes |
|------|----------|---------|-------|
| `--uri <URI>` | yes | | host or `redis://` / `rediss://` URI |
| `--port <N>` | | 6379 | ignored if the URI has a port |
| `--username <USER>` | | | Redis 6+ ACL user |
| `--password <PASS>` | | | see secrets above |
| `--db <N>` | | 0 | logical database index |
| `--tls` | | false | implied by `rediss://` |
| `--key <KEY>` | one of key/pattern | | single key to read |
| `--pattern <GLOB>` | one of key/pattern | | `SCAN MATCH`; each key ingested separately |
| `--type <auto\|stream\|list\|zset\|hash\|string>` | | `auto` (via `TYPE`) | how bars are stored |
| `--value-format <json\|csv\|fields>` | | `json` | encoding of each entry |
| `--from <ID\|SCORE>` / `--to <ID\|SCORE>` | | full range | for streams / sorted sets |
| `--batch <N>` | | 1000 | `XRANGE COUNT` / `SCAN COUNT` |

**sqlite** — `mnnn ingest service sqlite`

| flag | required | default | notes |
|------|----------|---------|-------|
| `--path <FILE>` | yes | | opened read-only |
| `--table <NAME>` | one of table/query | | |
| `--query <SQL\|@FILE>` | one of table/query | | |
| `--key <KEY>` | | | if the source DB is SQLCipher-encrypted |

**postgres** — `mnnn ingest service postgres`

| flag | required | default | notes |
|------|----------|---------|-------|
| `--url <URL>` | | | `postgres://user:pass@host:port/db?sslmode=...`; if given, the flags below are not needed (and conflict) |
| `--username <USER>` | yes, unless `--url` | | |
| `--password <PASS>` | | empty | see secrets above |
| `--host <HOST>` | yes, unless `--url` | | |
| `--port <N>` | | 5432 | |
| `--database <NAME>` | | `postgres` | |
| `--secure` | | false | true -> `sslmode=require`; false -> `sslmode=prefer` |
| `--ca-cert <FILE>` | | | with `--secure`, verify the server (`verify-full`) |
| `--schema <NAME>` | | `public` | |
| `--table <NAME>` | one of table/query | | |
| `--query <SQL\|@FILE>` | one of table/query | | |
| `--batch <N>` | | 10000 | rows per fetch (server-side cursor) |

Clap wiring: `ArgGroup`s for the one-of pairs (`key`/`pattern`, `table`/`query`),
`conflicts_with = "url"` on the postgres connection flags, `required_unless_present = "url"`
on `username`/`host`, and a custom check that `--body` is only used with a body method.

### 1.2 TUI (ratatui + crossterm)

- **Navigator:** sources -> symbols -> durations -> bar table (paged, virtualized),
  with a sparkline/candle view and coverage/gap summary per series.
- **Ingest:** file picker, preview of the first N parsed rows, inline prompts for missing
  source/symbol/duration, progress bar, result summary (inserted / duplicate / rejected).
- **Export:** choose filter + format + path, write out.
- **Keys/backup screens:** wrap the same library calls as the CLI; secrets shown once
  behind an explicit reveal, never logged.
- **Theme selector and editor** (see section 5).

CLI and TUI are thin layers over one library crate; no business logic in either.

## 2. Data model

### 2.1 Bars

| column      | type    | notes |
|-------------|---------|-------|
| id          | BLOB PK | 32-byte hash, see 2.3 |
| base        | TEXT    | e.g. `BTC` |
| quote       | TEXT    | e.g. `USDT`, `USD` (kept separate: Binance USDT != Coinbase USD) |
| timestamp   | INTEGER | UTC epoch milliseconds, bar **open** time |
| open        | REAL    | |
| high        | REAL    | |
| low         | REAL    | |
| close       | REAL    | |
| volume      | REAL    | base-asset volume |
| source_id   | TEXT FK | -> sources.id |
| duration_s  | INTEGER | seconds; rendered as `5m`, `1h`, `1d` in the UI |

Indexes: `(base, quote, source_id, duration_s, timestamp)`.

`symbol` in the UI = `base/quote` (e.g. `BTC/USDT`).

### 2.2 Sources

| column | type | notes |
|--------|------|-------|
| id     | TEXT PK | UUID v4 |
| name   | TEXT UNIQUE | e.g. `binance`, `coinbase`, `bitstamp`, or a file stem |
| kind   | TEXT | `file`, `directory`, `url`, `service` |
| locator| TEXT | the path or URL it came from |
| created_at | INTEGER | |

### 2.3 Bar id

`id = BLAKE3("{timestamp_ms}|{source.name}|{base}/{quote}|{duration_s}")`

- Duration is included: otherwise a 5m and a 1h bar at the same timestamp collide.
- `|` separators prevent ambiguous concatenations.
- Uses `source.name` (not the uuid) so ids are stable across databases. Consequence:
  renaming a source requires recomputing its bars' ids (done in one transaction).

### 2.4 Other tables

- `symbols` — known symbol list (base, quote, aliases such as `XBT` -> `BTC`).
- `ingest_runs` — source, file hash, started/finished, counts; lets re-ingest of an
  identical file be skipped.
- `schema_version` — migrations.

### 2.5 Storage layout

```
~/.muninn/
  storage/
    muninn.database        # SQLCipher-encrypted SQLite
  themes/
    <theme_name>/
      interface.toml      # palette + role mapping
      assets/             # optional: logos, glyph sets, background art, etc.
  keys/
    envelope.toml         # wrapped DEK (by service key + by master KEK), service-key version
    service/              # per-version service-key metadata (keys re-derivable from master)
  preferences.toml        # active theme, defaults for ingest, zero-volume policy, ...
```

- Root overridable with `--home` / `MUNINN_HOME` (tests use a temp dir).
- Created on first run with `0700` on the root and `0600` on files.
- Built-in themes are compiled into the binary and written to `themes/<name>/` on first
  run if missing; user edits live in those folders (or a new folder for a new theme).
- Preferences and themes are plain TOML on disk (not in the DB), so they are
  human-editable; the database and key file are the only encrypted artifacts.

## 3. Ingestion pipeline

1. **Read** with Polars (lazy CSV / JSON / Parquet). Column mapping by name with an
   alias table (`timestamp_utc`, `open_time`, `time`, `date` ...); ask/flag when ambiguous.
2. **Source** — from `--source`, else a known source matched by locator, else prompt.
3. **Symbol** — from `--symbol`, else parse the file name against the known symbol
   list (longest quote-suffix match: `BTCUSDT_5m.csv` -> `BTC/USDT`), else prompt.
   The answer is reused for the rest of that file/directory run.
4. **Timestamp** — parse epoch s/ms/us or ISO/`YYYY-MM-DD HH:MM:SS`; assume UTC unless
   an offset is present. Normalize to epoch ms.
5. **Duration** — from `--duration`, else from the file name (`_5m`), else the **mode**
   of consecutive timestamp differences across the whole file (not per row, so gaps
   don't produce wrong durations). Warn if the mode covers < 90% of diffs.
6. **Validate** — `high >= max(open, close)`, `low <= min(open, close)`, `volume >= 0`,
   no NaN, timestamp aligned to duration. Rejects go to a report, not the DB.
   Zero-volume bars are kept but flagged (Bitstamp pads empty periods with them);
   a preference controls whether they are dropped.
7. **Dedupe/upsert** on `id` in batched transactions. Re-ingesting the same file is a no-op.
8. **Report** — inserted / duplicate / rejected counts, gaps found, coverage range.

## 4. Security

### 4.1 Keys

- **Master key:** 256 bits from the OS CSPRNG (`rand::rngs::OsRng`).
- **24 words:** BIP39 encoding of the master key's 256 bits (`bip39` crate). Same secret,
  human-transcribable form. Words -> master key is exact recovery.
- **Password + salt:** Argon2id(password, random 16-byte salt) -> a *wrapping key* used to
  encrypt key material on disk and in exports. The password is **not** mixed into the
  master key, so the 24 words alone are enough to recover it.
  *(Open decision: if you want the password to be required in addition to the words,
  mix it in BIP39-passphrase style instead — then words without password recover nothing.)*
- **Dark shards:** Shamir secret sharing, 3-of-5, over the master key (`vsss-rs`;
  SLIP-39 if shards should also be word lists). Re-issuing shards produces a new,
  incompatible set — old and new shards can't be mixed.
- **Service keys:** `HKDF-SHA256(master, salt = db_id, info = "muninn/service/v{n}")`.
  Rotation = increment `n`. Any service key can be re-derived from the master.

### 4.2 Database encryption (envelope)

- SQLCipher via `rusqlite` (`bundled-sqlcipher-vendored-openssl`), keyed with a raw
  256-bit **data key (DEK)**.
- A key file `keys/envelope.toml` holds the DEK wrapped twice
  (XChaCha20-Poly1305):
  1. by the current service key (version recorded),
  2. by a master-derived KEK (`HKDF(master, info = "muninn/kek")`).
- Normal operation unlocks with the service key; the master key (from words or 3 shards)
  can always unlock.
- **Rotate service key:** derive `v{n+1}`, re-wrap the DEK, delete the old wrap. Instant.
- **Rekey (full):** generate a new DEK and `PRAGMA rekey`; optional, for suspected compromise.
- `zeroize` + `secrecy` for all key material; secrets never logged or echoed.

### 4.3 Backups

`mnnn backup create --out <DIR>` writes:

- `storage/muninn.database` — SQLite online backup API, still SQLCipher-encrypted.
- `keys/` — envelope key file and service-key metadata (already encrypted).
- `preferences.toml` — copied as-is.
- `themes/` — every `<theme_name>/` folder, including `interface.toml` and `assets/`.
- Packed as one `muninn-backup-<timestamp>.tar.zst`, optionally `age`-encrypted with the
  password.
- `manifest.json` — version, created_at, file hashes, key-file version.

Key exports (`mnnn keys export`) are **separate files**, each password-encrypted, so they
can be stored in different places:

- `master.key.age`
- `words.txt.age` (24 words)
- `service-keys.json.age` (versions + wrapped DEK; service keys themselves are
  re-derivable from the master)
- `shard-1.age` ... `shard-5.age`

Warn whenever master, words and >= 3 shards are written to the same directory — that
defeats the vault/shard split.

### 4.4 Recovery

- **Database:** restore `storage/muninn.database` + `keys/`; unlock with
  service key, master key, 24 words, or any 3 shards.
- **Preferences and themes:** restore `preferences.toml` and `themes/` into `~/.muninn`
  (prompt before overwriting existing themes with the same name).
- **Master key:** from 24 words, or from any 3 of 5 shards.
- **24 words:** re-derived from the master key.
- **Service keys:** re-derived from the master key + version.
- **Dark shards:** re-issued from the master key (new set; old set invalidated).

## 5. Themes

- Theme = named palette mapped to semantic roles: `bg`, `fg`, `muted`, `border`,
  `border_focus`, `selection_bg`, `selection_fg`, `accent`, `header`, `up` (bull candle),
  `down` (bear candle), `warning`, `error`, `success`, `info`, `chart_1..chart_6`.
- Each theme is a folder `~/.muninn/themes/<theme_name>/` with `interface.toml`
  (palette + role mapping) and an `assets/` folder. Built-ins ship in the binary and are
  materialized there; "reset" restores the built-in version.
- **Every theme has a dark and a light variant.** `interface.toml` has `[meta]`
  (name, author, base theme), `[dark]` and `[light]` tables, each mapping every semantic
  role. A theme missing either variant fails validation (the editor can generate a
  starting light variant from a dark one, or vice versa, for the user to tune).
- **Mode:** `light`, `dark`, or `system`.
  - `system` resolves at startup and on focus/resize: first the terminal background
    (OSC 11 query, luminance < 0.5 -> dark), then the OS preference (freedesktop portal
    `org.freedesktop.appearance color-scheme` on Linux, `AppleInterfaceStyle` on macOS,
    `AppsUseLightTheme` on Windows), else dark.
  - `MUNINN_THEME_MODE` env and `--theme-mode` flag override for one run.
- **Defaults** (`preferences.toml`):
  ```toml
  [theme]
  name = "tokyo-night"
  mode = "system"
  ```
- Built-in themes (dark / light, palettes from the published theme specs):
  | theme | dark | light |
  |-------|------|-------|
  | `tokyo-night` (default) | Tokyo Night (Night) | Tokyo Night Day |
  | `catppuccin` | Mocha | Latte |
  | `gruvbox` | Gruvbox Dark | Gruvbox Light |
  | `dracula` | Dracula | Alucard |
  | `nord` | Nord (Polar Night) | Nord Light (Snow Storm base) |
  | `one` | One Dark | One Light |
  | `solarized` | Solarized Dark | Solarized Light |
- **Selector:** list with live preview of the navigator, bar table and a candle chart; a
  mode toggle (light / dark / system) previews both variants side by side.
- **Editor:** pick variant (dark/light) and a role, edit hex or choose from the theme's palette, live preview,
  save as new theme, reset to base. Contrast warning when fg/bg contrast < 4.5:1.
- Fallback to 256-colour / 16-colour approximations when the terminal lacks truecolor.
- Active theme and mode stored in `preferences.toml`; themes included in backup and recovery (4.3/4.4).

## 6. Crate layout

```
src/
  main.rs            # bin `mnnn`: parse CLI, dispatch
  lib.rs
  cli/               # clap definitions + handlers
  tui/               # app state, screens, widgets, theme editor
  ingest/            # readers (csv, json, service), column mapping, normalize, validate
  model/             # Bar, Source, Symbol, Duration, BarId
  store/             # sqlite/sqlcipher, migrations, queries, upsert
  crypto/            # master, mnemonic, shards, service keys, envelope, export
  backup/            # create/restore
  theme/             # palette, built-ins, (de)serialize
  prefs/             # preferences.toml
  paths.rs           # ~/.muninn layout, permissions
tests/
  fixtures/          # small slices cut from raw data
```

## 7. Dependencies (initial)

clap (derive, env), reqwest (rustls, json), redis (tokio-comp, tls-rustls), tokio-postgres + postgres-native-tls or tokio-postgres-rustls, tokio, url, rpassword, polars (lazy, csv, json, parquet, dtype-datetime), rusqlite
(bundled-sqlcipher-vendored-openssl), ratatui, crossterm, serde, serde_json, toml,
uuid (v4), blake3, bip39, vsss-rs, argon2, hkdf, sha2, chacha20poly1305, age,
zeroize, secrecy, rand, thiserror, anyhow, tracing, dialoguer (CLI prompts), dirs.

## 8. Test data

Source: `~/Projects/kommandosaldat/raw/data` (`BTCUSDT_5m.csv`, `ETHUSDT_5m.csv`;
header `timestamp_utc,open,high,low,close,volume`, UTC open times, 5m bars from
2017-08-17, contains gaps and zero-volume bars).

- Commit small fixtures under `tests/fixtures/` (a few hundred rows each) that include
  at least one gap, zero-volume bars, and a deliberately invalid row.
- Full-file tests are `#[ignore]` and read the path from `MUNINN_RAW_DATA`.
- Examples in `examples/` and README use the same files.

Key test cases: symbol + duration from file name; duration by mode with gaps;
idempotent re-ingest; bar-id collision across durations; prompt vs `--non-interactive`;
key init -> words -> recover; 3-of-5 shard recovery (and 2 shards fails);
service-key rotation keeps DB readable; master unlocks after rotation; backup/restore
round-trip including preferences and themes; wrong password fails cleanly.

## 9. Milestones

1. **Skeleton:** `mnnn` bin, CLI tree, error handling, config/db paths.
2. **Model + store:** schema, migrations, plain SQLite first.
3. **CSV ingest:** normalize, derive, validate, upsert; fixtures + tests.
4. **Export** (csv/parquet/json).
5. **Crypto:** master/words/shards/service keys; switch store to SQLCipher with envelope keys.
6. **Backup/recovery.**
7. **TUI:** navigator, ingest, export.
8. **Themes:** built-ins, selector, editor, backup integration.
9. **JSON + service ingest.**

### 9.1 Milestone notes (as implemented)

| Milestone | Commit |
|---|---|
| 1 Skeleton | `7aee339` |
| 2 Model + store | `584f7d1` |
| 3 CSV ingest | `3a676eb` |
| 4 Export | `dae917b` |
| 5 Crypto | `490bac6` |
| 6 Backup/recovery | `c9deba0` |
| 7+8 TUI + themes (+ help, man pages, JSON reader) | `7925fb5` |
| 9 JSON + service ingest | `f66ef03` |

Docs, TUI help and man pages (sections 11–13) were added alongside milestone 7/8.

Full-file run (`--release`, temp MUNINN_HOME, raw/data/BTCUSDT_5m.csv, 965,366 data
rows): 965,366 inserted, 0 rejected, 932 zero-volume (kept), 32 gaps (1,944 missing
bars), coverage 2017-08-17 04:00 .. 2026-10-27 21:05 UTC, 41.3 s ingest; the
re-run is skipped by content hash.

## 10. Decisions (resolved)

- The password is **not** mixed into the master key: the 24 words or a shard quorum
  alone recover the data key. The password only unwraps the service key (Argon2id).
- Shards are `vsss-rs` binary shares (GF(256), 3-of-N) encoded as text with a checksum:
  `muninn-shard-v1:<set>:<threshold>:<index>:<hex>:<checksum8>`. Not SLIP-39.
- Prices are stored as REAL (f64).
- Zero-volume bars are **kept** and flagged (`flags & FLAG_ZERO_VOLUME`);
  `preferences.toml [ingest] drop_zero_volume = true` drops them instead.
- `keys/` never stores the master key, the 24 words or the shards, plain or
  encrypted. They leave only through `mnnn keys export` (age-encrypted files).

### 10.1 Deviations from this plan

- Milestones 7 and 8 were one commit (plus help, man pages and the JSON reader).
- Redis logical DB flag is `--db-index` (the global `--db` flag owns the name).
- `bars` has an extra `flags` column (zero-volume marker).
- CSV escape: only RFC 4180 quote doubling is supported.
- TUI ingest shows done/idle, no live progress bar.
- Prompts are a small custom implementation, not `dialoguer`.
- JSON is parsed with serde, then turned into a Polars frame.
- `keys rekey` requires the master key; restoring over existing data needs confirmation.
- The 16-colour theme fallback is heuristic.
- Data commands fail with a hint until `mnnn keys init` has been run.
- Redis/Postgres have unit tests plus live tests that are `#[ignore]`d and need
  `MUNINN_TEST_REDIS` / `MUNINN_TEST_POSTGRES`.

## 11. Documentation (docs/)

One Markdown file per feature under `docs/` (cli, ingest-csv, ingest-json,
services/{http,redis,sqlite,postgres}, data-model, storage-layout, export, keys,
encryption, backup-recovery, themes, preferences, tui, help). Docs change with the
code in the same commit.

## 12. TUI help

`?`/`F1` or the Help tab. Topics are compiled from `docs/` by `build.rs`. Substring and
fuzzy search, links between topics with history/back, context-sensitive (opens on the
current screen's topic). `mnnn help <topic>` / `mnnn help --search <q>` print the same
text. Tests cover search and broken links.

## 13. Man pages

Generated with `clap_mangen` (hidden `mnnn man --out man`) and committed under `man/`:
`mnnn(1)` with extra FILES, ENVIRONMENT, EXIT STATUS, SECURITY, EXAMPLES and SEE ALSO
sections, plus one page per subcommand (39 pages). A test fails if a page is missing
or stale. Install steps: README.md and docs/cli.md.
