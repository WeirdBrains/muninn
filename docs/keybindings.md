# Keybindings

Every key runs the same named action as its [menu](menus.md) item.

## Everywhere

| key | action |
|-----|--------|
| F10 | open the menu bar |
| Alt+F/V/D/P/K/T/H | open File / View / Data / Projects / Keys / Theme / Help |
| `?`, F1 | help for the current context ([in-app help](help.md)) |
| F5 | refresh data |
| Tab / Shift-Tab | focus the next / previous tile |
| Alt-1 … Alt-9 | focus tile [1] … [9] |
| q | quit (when no dialog is open and you are not typing) |
| Ctrl-C | quit |
| Esc | close the open menu or dialog |

## Dialogs

| key | action |
|-----|--------|
| ↑ ↓ | select a field or action |
| Enter | edit the field / run the action |
| Esc | stop editing, then close |

## Tiles

Window commands are `Ctrl-w` followed by a key: `v` split right, `s` split below,
`q` close, `h j k l` focus, `Tab` next, `W` or `p` previous, `< > + -` resize, `=` equalize, `x` swap,
`z` zoom, `t` list ↔ chart, `e` new-tile direction, `y` link. See [tiling](tiling.md).

| key (list tile) | action |
|-----|--------|
| ↑ ↓ j k PgUp PgDn Home End | move |
| `s` / `S` | next / previous sort column |
| `r` | reverse sort |
| `[` / `]` / `\` | list range from / to (date and time picker) / clear |
| `,` / `.` | previous / next series |

| key (chart tile) | action |
|-----|--------|
| `c b n a e v m w g z # x y t` | toggle a layer ([charts](charts.md)) |
| `L` | layer menu |
| `N` / `M` | line metric / moving averages |
| `h` `l`, PgUp PgDn | pan a quarter / a full screen |
| `+` `-` | zoom in / out |
| ← → | move the crosshair |
| `[` `]` | period start / end (date and time picker) |
| `{` `}` `@` | type start / type end / jump to date |
| Home / End, `0` | first bars / latest (live) |
| `,` / `.` | previous / next series |

## Sidebar

| key | action |
|-----|--------|
| `Ctrl-b` | show / hide the [sidebar](sidebar.md) |
| `/` | search series |
| `o` | toggle sort order |
| `X` | clear filters |
| `T` | time range for every view (date and time picker: from, then to) |
| `s`, `f m p d y`, `t`, Enter | sort key, filter editors (source, symbol, price, period, year), time range, assign (sidebar focused) |

## History and authors

| key | action |
|-----|--------|
| `Y` | show and focus / hide the [history](history.md) bar |
| `u` / `U` | step back / forward in the history |
| `j k`, Enter, Esc | move, go to the action, leave (history bar focused) |
| `A` | the [authors](authors.md) popup (`a` add, `e` edit, `d` delete, Enter current) |

## Projects, live searches and journals

| key | action |
|-----|--------|
| `P` | the [projects](projects.md) page |
| `F` | the [live searches](live-searches.md) list |
| `B` | save the sidebar's search as a live search |
| `J` | the [journals](journals.md) list |
| `a` | add the selected series to a project (sidebar focused) |

Inside them: `n` new, `r` rename, `d` delete (`y` confirms), `f` fork, `p` add to a
project, Enter open, Esc back. In an entry: ← → move by word, `v` select, `m`
highlight, `n` note, `i` image; the editor saves with Ctrl-S and switches Markdown ↔
LaTeX with Ctrl-T. Full tables: [live searches](live-searches.md),
[projects](projects.md), [journals](journals.md).

## Import

| key | action |
|-----|--------|
| `i` | open the [import](import.md) popup |
| Space, `a`, `.`, `/`, Tab | select, whole folder, hidden, filter, path completion (in the popup) |
| Esc, `c`, [Cancel] | cancel a running import (asks first; `y`/`n`) |
| `p`, `1`–`9`, `b` | places column, jump to a place, bookmark the folder (in the popup) |
| Tab / Shift-Tab | path input: complete, list, then cycle candidates; Esc reverts |
| click [X] | close any popup (during an import: asks to cancel) |
