# Import

**File ▸ Import…** (or `i`) opens the import popup: a file browser, per-file guesses
and a background import with a live progress bar. The same pipeline as
[`muninn ingest csv|json`](ingest-csv.md) runs underneath ([JSON](ingest-json.md)).

```
┌ Import ─────────────────────────────────────────────────────────────┐
│ path /data/binance/                                                  │
│ filter         hidden off   2 selected                               │
│┌Places──────┐┌Files──────────────┐┌Preview─────────────────────────┐│
││1 home      ││▸ ..               ││symbol BTC/USDT                 ││
││2 raw/data  ││● BTCUSDT_5m.csv   ││period 5m                       ││
││3 cwd       ││○ ETHUSDT_1h.csv   ││timestamp,open,high,...         ││
│└────────────┘└───────────────────┘└────────────────────────────────┘│
│ source   local (default)                                             │
│ symbol   derive from name / data                                     │
│ duration derive from name / data                                     │
└──────────────────────────────────────────────────────────────────────┘
```

## Browsing

Only folders and data files (`csv tsv txt json ndjson jsonl`) are listed.

| key | action |
|-----|--------|
| `j` / `k`, ↑ / ↓, PgUp / PgDn, Home / End | move |
| Enter, `l`, → | open a folder (Enter on a file imports it, or the selection) |
| Backspace, `h`, ← | parent folder |
| Space | select / unselect a file (multi-select) |
| `a` | select the whole folder (again: unselect) |
| `.` | show / hide hidden files |
| `/` | filter the listing (substring) |
| Tab, `g` | type a path; Tab completes it like a shell, listing candidates when ambiguous; Enter goes there (or imports a file) |
| `1`–`9` | jump to a place: home, `raw/data` (or `$MUNINN_RAW_DATA`), the working folder, then recent folders |
| `s` `y` `d` | edit source, symbol, duration |
| `i` | import the selection |
| Esc | close |

The preview shows the guessed symbol and duration (from the file name) and the first
10 lines of the highlighted file parsed as bars with the same parser as the import
(column mapping, timestamps, validation): `time SYMBOL o h l c v [zero-vol]`. Invalid
lines are shown in red with the reason and the raw line. It reads at most 64 KiB, so
it is instant on huge files. A large JSON array cannot be parsed from its start, so
its preview shows raw lines; NDJSON previews parse.

## Resolution

Source defaults to `local`. Symbol and duration are derived from each file name and,
for the duration, from the data; type a value to override it for every file in the
run. Recently used folders are saved in [preferences](preferences.md)
(`tui.import_recent`).

## Progress and cancel

The import runs in a background thread on its own connection, so the TUI stays
responsive. Progress is byte-based:

- **file** gauge: the current file's estimated completion;
- **all** gauge: bytes done / total bytes of every file in the run, with an ETA from
  the rate so far;
- bars stored / to store and elapsed time as a secondary line, plus a log of each
  file's result.

Bytes are counted as the file is read (published every 64 KiB), so the bar moves from
the first read. Reading is fast compared with storing, so each file's bytes are split
into phases: reading 10 %, parsing and validating 5 %, storing 85 % (scaled by bars
stored, reported every 4096 rows). The estimate is therefore linear in bytes across
files and roughly linear in time within a file. **Esc cancels** after the current chunk (50 000 bars); bars
already stored stay. The summary lists inserted, duplicate, rejected rows and gaps per
series; identical files already imported are skipped. Rows failing validation are
dropped (see the CLI `--rejects` option to keep a report).
