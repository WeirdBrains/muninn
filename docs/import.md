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
lines of the highlighted file; it reads only the start of the file.

## Resolution

Source defaults to `local`. Symbol and duration are derived from each file name and,
for the duration, from the data; type a value to override it for every file in the
run. Recently used folders are saved in [preferences](preferences.md)
(`tui.import_recent`).

## Progress and cancel

The import runs in a background thread on its own connection, so the TUI stays
responsive. A gauge shows bars stored / bars to store for the current run and a log
lists each file's result. **Esc cancels** after the current chunk (50 000 bars); bars
already stored stay. The summary lists inserted, duplicate, rejected rows and gaps per
series; identical files already imported are skipped. Rows failing validation are
dropped (see the CLI `--rejects` option to keep a report).
