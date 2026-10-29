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
│ source   from file name, else local (s)                              │
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
| Space | select / unselect a file or folder (multi-select) |
| `a` | select every file in this folder (again: unselect) |
| `A` | import this whole folder, including subfolders |
| `.` | show / hide hidden files |
| `/` | filter the listing (substring) |
| Tab, `g` | type a path (see *Path completion*); Enter goes there (or imports a file) |
| `p` | focus the Places column (see below) |
| `1`–`9` | jump to the numbered place |
| `b` | bookmark the current folder (again: remove the bookmark) |
| `s` `y` `d` | edit source, symbol, duration |
| `i` | import the selection, or the highlighted file or folder |
| Esc | close |

The preview shows the guessed symbol and duration (from the file name) and the first
10 lines of the highlighted file parsed as bars with the same parser as the import
(column mapping, timestamps, validation): `time SYMBOL o h l c v [zero-vol]`. Invalid
lines are shown in red with the reason and the raw line. It reads at most 64 KiB, so
it is instant on huge files. A large JSON array cannot be parsed from its start, so
its preview shows raw lines; NDJSON previews parse.

## Path completion

In the path input, Tab works like zsh or Neovim:

1. **Tab** completes as far as the matching names agree (`/data/BT` → `/data/BTCUSDT_`).
2. If several names still match, the next **Tab** shows them as a list under the input.
3. Further **Tab** steps through the list, highlighting a candidate and filling it into
   the input; **Shift-Tab** steps backwards; both wrap around. Clicking a candidate
   picks it.
4. **Enter** or typing accepts the candidate (a second Enter goes there); **Esc** puts
   back what you had typed.

Hidden names are offered only when hidden files are shown (`.`) or you type the dot.

## Resolution

**Source** records where the data originally came from (also when the files were
copied or uploaded from elsewhere): press `s` (or click the source line) and type it to
use it for every file in the run. Left empty, each file's source is the exchange in its
name (the preview shows `source kraken (from name)`), else `local`. Symbol and duration are derived from each file name and,
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
all-or-nothing rule applies to the CLI: a file that fails part-way leaves nothing behind,
and **Ctrl-C** in `mnnn ingest csv|json` cancels the same way (by `cli`).

### Import runs and the artifact ledger

Every import (TUI or CLI) is recorded as an *import run* with a status: `running`,
`completed`, `failed` or `cancelled`. A cancel is noted when it happens: the time it
was asked for, by whom (`tui` or `cli`), the file and the phase (`read`, `parse`,
`store`).

Each run keeps a ledger of what it created: new **source** rows, new **series**,
**bars** (per series, with the count inserted), the **ingest record** used to skip
identical files, and the **recent** folder entry. Artifacts of finished files are
`kept`. Those of the cancelled or failed file are rolled back and listed as
`rolled_back`. After the run, anything the run created that ended up empty (for
example a new source whose only file was cancelled) is deleted and listed as
`removed`; if nothing was imported, the Recent entry the run added is removed too.

```
mnnn ingest runs              # latest run
mnnn ingest runs --show 12    # one run with its ledger
```

The TUI summary shows the run number.

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

## After an import

When the import finishes and no tile shows a series yet, the focused tile loads the
first series that was imported (the status line says `loaded the import: …`). If any
tile already shows data, the view is left as it is; pick the new series in the sidebar.

## When the symbol can't be guessed

If a file's name doesn't tell its symbol (for example `export_5m.csv`) and the symbol
field is empty, a **Symbol needed** popup asks for it before the import starts, one
file at a time. The popup shows the file name on its own line (`file`, wrapped if
very long, never cut) and its folder below it (`in`, shortened from the left with `…`
so the end of the path stays visible). When several files need a symbol, as in a
folder import (`A`), the title counts them: `Symbol needed · file 3 of 12`.

| key / button | action |
|---|---|
| type, Enter / `[Enter OK]` | use this symbol (BASE/QUOTE, e.g. `BTC/USDT`) for this file |
| Ctrl-a / `[Ctrl-a OK for all]` | use it for this and every remaining file |
| Tab / `[Tab skip file]` | leave this file out |
| Esc / `[Esc cancel]` / `[X]` | cancel the import (nothing is imported) |

An invalid symbol shows an error and keeps the popup open. Filling the symbol field
(`y`) beforehand skips the question.

## Mouse

Click a file or folder to highlight it and click it again to open the folder or import
the file. Click the path, filter or source/symbol/duration line to type there, a Places
entry to go there, a completion candidate to pick it. The bottom row is a bar of key
buttons (`[Enter open]`, `[Space select]`, `[A import folder]`, `[i import]`, ...) that
act like the keys; `[Close]` ends the finished-import view.

## Importing a whole folder

A selected or highlighted folder is imported with everything below it: subfolders are
searched for `.csv`, `.tsv`, `.txt`, `.json`, `.ndjson` and `.jsonl` files (hidden
entries are skipped). `A` imports the current folder that way. Progress shows the
current file (`file i/n`) and the whole run; each file is still all-or-nothing, so a
cancel keeps finished files, rolls back the current one and the ledger records it.
On the command line the same is `mnnn ingest path <DIR>` (see [CLI](cli.md)).
