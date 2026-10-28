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
| `p` | focus the Places column (see below) |
| `1`–`9` | jump to the numbered place |
| `b` | bookmark the current folder (again: remove the bookmark) |
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
files and roughly linear in time within a file. The summary lists inserted, duplicate, rejected rows and gaps per
series; identical files already imported are skipped. Rows failing validation are
dropped (see the CLI `--rejects` option to keep a report).

## Cancelling

The progress popup has a **[Cancel]** button; `Esc`, `c` and the popup's **[X]** do the
same. Cancelling always asks first (**[Yes, cancel]** / **[No, continue]**, or `y` /
`n`). After yes the popup shows *Cancelling…*: the background import stops at its next
read (every 64 KiB) or row batch (every 4096 rows), usually within a fraction of a
second.

What stays and what goes:

- **Files that finished** before the cancel stay imported.
- **The file being imported** is rolled back entirely: each file is imported in one
  all-or-nothing transaction, so none of its bars, its series totals or its import
  record remain. Importing it again later works normally (it is not skipped as
  "already imported").
- **Files not started** are not touched.

The summary says how many files were kept, discarded and not started. The same
all-or-nothing rule applies to the CLI: a file that fails part-way leaves nothing behind.

## Places

The left column of the browser, titled **Places (p)**, lists folders you can jump to.
It has up to three sections:

- **Places**: built-in shortcuts: Home, `~/Projects`, `raw/data` (the kommandosaldat
  raw data folder, or `$MUNINN_RAW_DATA`) and the folder Muninn was started from. Ones
  that don't exist on this machine are left out.
- **Bookmarks**: folders you bookmarked with `b` (while browsing, `b` bookmarks the
  folder you are in; pressing it again removes the bookmark).
- **Recent**: the last 10 folders you imported from (newest first).

To use a place: click it, press its number `1`–`9`, or press `p` to move into the
column, pick one with `j`/`k` and press Enter. In the column, `x` removes a bookmark or
recent entry (built-ins stay). Bookmarks and recents are saved in
[preferences](preferences.md) (`tui.import_bookmarks`, `tui.import_recent`). The
popup opens in the most recent folder.
