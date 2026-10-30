# Action history

Muninn records what you do in the encrypted database: imports, exports, filter and
time range changes, theme and theme mode changes, source and symbol changes, author
changes, and work on live searches, projects, journals, reports and rules. Each action has an id, a kind, a summary, when it ran, where it came from
(`cli`, `tui`, or `rule` for what a [rule](rules.md) did) and the [author](authors.md) who was current.

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
| `search.save`, `search.fork`, `search.edit`, `search.remove` | CLI, TUI | the [live search](live-searches.md) |
| `project.create`, `project.edit`, `project.remove`, `project.add`, `project.drop` | CLI, TUI | the [project](projects.md) and item |
| `journal.create`, `journal.edit`, `journal.remove`, `journal.fork` | CLI, TUI | the [journal](journals.md) |
| `entry.add`, `entry.edit`, `entry.remove`, `entry.copy` | CLI, TUI | the entry (copies: how many, to which journal) |
| `note.add`, `note.edit`, `note.remove`, `highlight.add`, `highlight.remove`, `image.add` | CLI, TUI | the note, highlight or image |
| `report.create`, `report.edit`, `report.remove` | CLI, TUI | the [report](reports.md) |
| `report.section.add`, `report.section.edit`, `report.section.remove`, `report.section.move` | CLI, TUI | the section, its kind and position |
| `journal.export`, `entry.export`, `report.export`, `project.export` | CLI, TUI | what was exported, the format and file ([export formats](export-formats.md)) |
| `journal.print` | CLI, TUI | the PDF and the print command (or none) |
| `journal.import`, `project.import` | CLI | the file and what was restored |
| `rule.create`, `rule.edit`, `rule.remove`, `rule.enable`, `rule.disable` | CLI, TUI | the [rule](rules.md) |
| `model.create`, `model.edit`, `model.apply`, `model.rollback`, `model.remove` | CLI, TUI, rules | the [model](models.md), its version, plan steps and values lost |
| `record.create`, `record.update`, `record.delete` | CLI, TUI, rules | the model and record, or the filter and how many |
| `rule.run` | rules | the rule, the action that triggered it, its status (`ok`, `failed`, `blocked`); what it did is recorded under it |

Stepping back over these actions moves the head only; it does not undo the change
(stepping back restores view state: filters, time range and theme).

See also: [authors](authors.md), [sidebar](sidebar.md), [themes](themes.md).
