# Terminal UI

```
mnnn tui
```

The TUI asks for the password first (or uses `MUNINN_PASSWORD`), then opens. It uses
the same library calls as the CLI. Mouse clicks work on the menu bar, dropdowns, tiles, tabs and list headers
(mouse capture is on while the TUI runs; hold Shift to select text in most terminals).

## Screen layout

```
┌ header: MUNINN banner · version · lock state · theme mode · home ┐
├ menu bar: File  View  Data  Keys  Theme  Help ───────────────────┤
│ content area                                                      │
├ status line: theme · variant · last message ─────────────────────┘
```

- **Header**: MUNINN in block letters with the version and status (unlocked, theme
  mode and resolved variant, home folder, current author). On terminals with fewer than 30 rows (or
  fewer than 80 columns) it collapses to one line, `Muninn v0.x.y`. Force compact or
  large in **File ▸ Preferences…** (`[tui] header` in [preferences](preferences.md)).
- **Menu bar**: categorized menus with submenus, mouse and keyboard ([menus](menus.md)).
- **Content area**: tiles (see below).
- **Dialogs** (export, keys, themes, preferences, help, info) open over the content
  area; Esc closes them.

All keys: [keybindings](keybindings.md).

## Content area

Always a [tiling layout](tiling.md): one list tile to start, split and rearranged
with `Ctrl-w` commands or the **View** menu. Each tile shows a list or a chart of
its own series.

## Performance

A tile's series is loaded into memory once, in the background (a tile shows
`loading…` meanwhile; about 5 s for a million 5m bars). After that sorting, zooming,
panning and filtering run in memory, typically 1–30 ms on a million bars. Memory is
about 56 bytes per bar per loaded series.

## Import

`i` or **File ▸ Import…** opens a file browser with path completion, multi-select,
preview and a background import with progress. See [import](import.md).

## Sidebar

A collapsible series list at the right (`Ctrl-b`) with fuzzy search (`/`), sort and
filters that also scope the tiles. See [sidebar](sidebar.md).

## History bar

A collapsible pane at the left (`Y`) with the action tree: the head, the current
line and every fork. `u` / `U` step back and forward, restoring filters and theme. See
[history](history.md); actions are attributed to the current [author](authors.md)
(`A`).

## Projects, live searches and journals

The **Projects** menu (Alt+P) opens [projects](projects.md) (`P`),
[live searches](live-searches.md) (`F`, save with `B`) and [journals](journals.md)
(`J`). They open over the content area like other dialogs; opening a live search or a
project's result changes the sidebar and tiles behind them.

## Dialogs

Every popup has a red **[X]** at the right of its title bar: click it to close (same as
Esc). Clicks outside a popup are ignored while it is open.

## Loading

Series load in a background thread, so the UI keeps responding (menus, other tiles,
help) while a large series is read. Until the bars arrive the tile shows what is
happening:

- a progress bar, `▕████░░░░▏ 42%  12,345 / 29,000 bars`, once the bar count is known
  (it is counted first, then the bars are read 50,000 at a time);
- a spinner, `⠹ loading series…`, while the total is not known yet;
- `could not load the series: …` if reading fails.

The tile title shows `loading…` instead of the bar count meanwhile.

## Mouse

Everything you can act on is clickable; there are no keyboard-only actions:

| Where | Click |
|---|---|
| menu bar, dropdowns, submenus | open / run the item |
| tiles, tabs | focus; list header sorts; list rows select a bar |
| charts | period arrows ◂ ▸, plot moves the cursor; wheel zooms |
| sidebar | search box, sort key and order, filters, chips (remove), series rows |
| popup lists (Preferences, Chart layers, saved layouts, filters, themes) | select and activate the row (themes: first click selects, second applies) |
| forms (Export, Keys) | a field starts editing it, a `[ button ]` runs it |
| help | links (follow), search box, results and history entries, the key bar along the bottom |
| import picker | files/folders (click highlights, second click opens or imports), Places, path / filter / source / symbol / duration inputs, Tab-completion candidates |
| key buttons such as `[i import]`, `[OK]`, `[Cancel]`, `[Close]` | the same as pressing that key |
| `[X]` | close the popup |

Keys still work everywhere; the key buttons show which key does the same.

- **Export** (File ▸ Export…): out, symbol, source, duration, from, to; **Format**
  cycles csv → parquet → json. See [export](export.md).
- **Keys** (File ▸ Backup ▸ Create, Keys ▸ Rotate / Re-issue shards): create a backup
  into a directory, rotate the service key (needs the 24 words and the new password),
  re-issue shards (hidden until **Reveal/Hide**, cleared when the dialog closes). See
  [keys](keys.md) and [backup and recovery](backup-recovery.md).
- **Themes** (Theme ▸ Select…/Edit current…/New from current…): selector with live
  dark/light preview and editor. See [themes](themes.md).
