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
  mode and resolved variant, home folder). On terminals with fewer than 30 rows (or
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

## Dialogs

- **Export** (File ▸ Export…): out, symbol, source, duration, from, to; **Format**
  cycles csv → parquet → json. See [export](export.md).
- **Keys** (File ▸ Backup ▸ Create, Keys ▸ Rotate / Re-issue shards): create a backup
  into a directory, rotate the service key (needs the 24 words and the new password),
  re-issue shards (hidden until **Reveal/Hide**, cleared when the dialog closes). See
  [keys](keys.md) and [backup and recovery](backup-recovery.md).
- **Themes** (Theme ▸ Select…/Edit current…/New from current…): selector with live
  dark/light preview and editor. See [themes](themes.md).
