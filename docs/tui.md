# Terminal UI

```
mnnn tui
```

The TUI asks for the password first (or uses `MUNINN_PASSWORD`), then opens the
navigator. It uses the same library calls as the CLI.

## Global keys

| key | action |
|-----|--------|
| Tab / Shift-Tab | next / previous screen |
| 1-5 | Navigator, Ingest, Export, Keys & Backup, Themes |
| 6, `?`, F1 | help for the current screen ([in-app help](help.md)) |
| q | quit (when not typing in a field) |
| Ctrl-C | quit |

## Navigator (1)

Left: every series as `source symbol duration bars`. Right: coverage (first/last bar,
missing bars), a candle chart of the visible page, and the bar table.

↑/↓ select a series, Enter (or →) focuses the table, Esc (or ←) goes back,
PgUp/PgDn page through bars (200 per page, loaded on demand), Home/End jump, `r`
reloads. Zero-volume bars are marked `∅`.

## Ingest (2)

Fields: path (file or directory, `~` allowed), source, symbol, duration. ↑/↓ select,
Enter edits a field (Enter or Esc to finish). **Preview** shows the first parsed rows
and what would be derived from the file name. **Ingest** runs the import; if the source,
symbol or duration can't be derived, the cursor jumps to that field so you can type it
and run again. Rejected rows are dropped (the TUI runs with "yes"). The result panel
shows the report. See [CSV ingest](ingest-csv.md) and [JSON ingest](ingest-json.md).

## Export (3)

Fields: out, symbol, source, duration, from, to. **Format** cycles csv → parquet → json;
**Export** writes the file. See [export](export.md).

## Keys & Backup (4)

Shows the database id and service-key version. Actions: **Create backup** into the
given directory, **Rotate service key** (needs the 24 words and the password that wraps
the new key), **Re-issue shards** (needs the words; the new shards stay hidden until
you select **Reveal/Hide**, and are cleared when hidden). Word and password fields are
masked. See [keys](keys.md) and [backup and recovery](backup-recovery.md).

## Themes (5)

Theme selector with live dark/light preview, mode toggle and editor. See [themes](themes.md).
