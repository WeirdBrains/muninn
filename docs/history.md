# Action history

Muninn records what you do in the encrypted database: imports, exports, filter and
time range changes, theme and theme mode changes, source and symbol changes, and author
changes. Each action has an id, a kind, a summary, when it ran, where it came from
(`cli` or `tui`) and the [author](authors.md) who was current.

## A tree, not a list

The **head** is the action you are at. A new action becomes a child of the head.
Stepping back and then doing something different starts a **fork**: a new branch next
to the old one. Nothing is ever deleted, so every branch can be revisited.

```
$ mnnn history tree
│ #1 add author Ada
│ #2 import BTCUSDT_5m.csv · 401 bars · completed · run #1
│ #3 export 401 bars to out.csv (csv)
○ #4 theme nord                  ← the branch you stepped back from
○ #5 theme mode dark
  ● #6 ⑂ from #3 theme dracula   ← head, on a fork
```
Stepping back, forward or to an action restores the **view state** saved with the
actions: the sidebar filters (including the time range), the theme and the theme mode.
Data actions (imports, exports, sources) are records only; stepping past them does not
undo them.

- **Back** moves to the parent of the head (`u`, `mnnn history back [N]`).
- **Forward** moves to a child: the branch you stepped back from if there is one, else
  the newest branch (`U`, `mnnn history forward [N]`).
- **Go to** jumps to any action on any branch (Enter in the history bar,
  `mnnn history goto ID`; `0` is the start, before the first action).

## Terminal UI

The history bar is a collapsible pane on the left. Toggle it with `Y`,
**View ▸ History ▸ Show/hide history bar**, or its `«` button; whether it is open is
saved in [preferences](preferences.md) (`tui.history_open`). It needs a terminal at
least 94 columns wide.

Each row shows `#id summary` and its age. `●` marks the head, `○` other actions,
`⑂` the first action of a fork; forks are indented under the action they branch
from, and actions off the current line are dimmed. The first row, `start`, is the
state before any action.

| key | action |
|-----|--------|
| `Y` | show the history bar and focus it; again: hide it |
| `u` / `U` | step back / forward (anywhere, also **View ▸ History**) |
| `j` / `k`, ↑ / ↓, PgUp / PgDn, `g` / `G` | move (history bar focused) |
| Enter, click a selected row | go to that action |
| Esc, Tab, → | back to the tiles |

Imports that run in the background appear when they finish.

## Command line

```
mnnn history list [-n N] [--all] [--json]   # the current line, newest first; --all: every branch
mnnn history show [ID] [--json]             # one action (default: the head), with its forks
mnnn history tree                           # every branch, forks indented
mnnn history back [N]
mnnn history forward [N]
mnnn history goto ID                        # 0: before the first action
```

`back`, `forward` and `goto` write the restored theme and filters to
`preferences.toml`, so the next `mnnn tui` opens with them.

`mnnn theme set` and `mnnn theme mode` are recorded when the database opens without a
prompt (`MUNINN_PASSWORD` set); otherwise they change the theme without a history
entry, because they never ask for the password.

## Recorded actions

| kind | from | summary |
|------|------|---------|
| `import` | CLI, TUI | file(s), bars inserted, status, run number |
| `export` | CLI, TUI | rows, destination, format (scope in the params) |
| `filter`, `filter.time`, `filter.clear` | TUI | the new filters or time range |
| `theme`, `theme.mode` | TUI, CLI | the new theme or mode |
| `source.add`, `source.rename`, `source.remove`, `symbol.add` | CLI | what changed |
| `author.add`, `author.edit`, `author.remove`, `author.use` | CLI, TUI | the author |

See also: [authors](authors.md), [sidebar](sidebar.md), [themes](themes.md).
