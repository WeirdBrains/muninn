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
| `O` | the [reports](reports.md) list |
| `R` | the [rules](rules.md) window (`n` new, `e` edit, Space on/off, `t` test, `x` dry run) |
| `D` | the [model designer](models.md) |
| `C` | the [code spaces](code-spaces.md) window (`n` new, Enter edit, `r` run) |
| `K` | the [network](network.md) window (node status first) |
| `G` | the [organizations](organizations.md) window |
| `V` | the [reviews](reviews.md) window |
| `a` | add the selected series to a project (sidebar focused) |

Inside them: `n` new, `r` rename, `d` delete (`y` confirms), `f` fork, `p` add to a
project, Enter open, Esc back. In an entry: ← → move by word, `v` select, `m`
highlight, `n` note, `i` image; the editor saves with Ctrl-S and switches Markdown ↔
LaTeX with Ctrl-T. `E` exports (journals, entries, reports, projects) and `P` prints
(journals, entries); `I` in the projects list imports an archive; in a report `a` adds a section, `J` / `K` move it, `v` previews.
In the model designer: `m` design, `r` records, `h` history, `f` rules; in the design
`a` add, `e` edit, `x` drop, `u` undo, `K` / `J` move, `A` apply, `z` discard; in history
`b` rolls back; in records `w` where, `/` text, `s` sort, `g` group, `U` / `D` update /
delete all matching.
In the network window: Tab / ← → / `1`–`9`, `0` change page, `S` start and `X` stop the
node, `g` refresh; Peers `c` connect, `l` label, `d` forget; Relays `a` add, `d` remove,
`r` serve; Connections Enter shows a user's nodes, `t` trust or untrust, `d` forget;
Invites `n` invite link, `p` paste a link, `a` accept and `d` decline a request, `r`
withdraw an invite; Nodes `c` connect, `a` accept and `d` decline a request, `n` invite
link, `p` paste a link, `s` share, `L` their items (Enter imports), `l` label, `r`
disconnect (`v` checks a short code by hand); Shared `a` share, `d` unshare; Dark shards
`e` entrust or check (Space picks nodes), `d` drop a held shard; Search Enter runs a saved search across peers; Settings
Enter edits or toggles.
In the organizations window: Tab / ← → / `1`–`6` change page, `g` refresh; Orgs `n` new,
Enter pick, `r` rename, `v` visibility, `o` access, `h` hosted, `m` member sign-in, `d`
delete; Members `i` invite, Enter details, `a` approve, `x` reject, `o` role, `k` kick;
Roles `n` new, `e` / Enter permissions, `P` all permissions, `a` / `i` apply / invite
default, `d` remove; Resources `a` add, `p` / Enter publish, `z` archive, `d` release;
Joined `b` browse, `A` apply, `j` join, `s` sync, `m` mirror, `l` leave; Site `S` / Enter
start or stop, `X` stop, `e` ACME.
In the reviews window: `1`–`3` / Tab / ← → change page, `g` refresh; My reviews `n`
new, Enter thread, `i` link, `p` view, `H` revisions, `o` close / reopen, `d` delete;
Thread `c` comment, `e` suggest, `h` highlight, `v` vote, `r` reply, `a` / `x` accept /
reject, `A` / `X` all, `m` merge, `F` force, `w` withdraw, `D` drop; Reviewing `j` join,
Enter view, `s` sync, `l` leave ([reviews](reviews.md)).
In a code space's editor: `e` edits the source (Esc stops), `s` / Ctrl-S saves, `b`
builds, `r` / Ctrl-R runs, `c` checks, `i` sets the input, `t` widens the result tile.
Full tables: [code spaces](code-spaces.md), [models](models.md), [live searches](live-searches.md),
[projects](projects.md), [journals](journals.md), [reports](reports.md), [rules](rules.md),
[export formats](export-formats.md).

## Import

| key | action |
|-----|--------|
| `i` | open the [import](import.md) popup |
| Space, `a`, `.`, `/`, Tab | select, whole folder, hidden, filter, path completion (in the popup) |
| Esc, `c`, [Cancel] | cancel a running import (asks first; `y`/`n`) |
| `p`, `1`–`9`, `b` | places column, jump to a place, bookmark the folder (in the popup) |
| Tab / Shift-Tab | path input: complete, list, then cycle candidates; Esc reverts |
| click [X] | close any popup (during an import: asks to cancel) |
