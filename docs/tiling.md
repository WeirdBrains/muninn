# Tiling layout

The content area is always a tiling layout, i3 style. A fresh session has one list
tile; every view is a tile, and any tile can be split, closed, swapped, resized,
zoomed or switched between list and chart at any time, at any depth.

## Model

The layout is a binary tree. Inner nodes are containers
`{ dir: horizontal | vertical, mode: split | tabbed | stacked, ratio }`; leaves are
tiles `{ view: list | chart, series, sort, link, chart options }`. Each tile has its
own series and options. The whole tree is saved to `[tui.layout]` in
[preferences](preferences.md) after every change and restored on start.

## Window commands (`Ctrl-w` then a key)

| keys | action | menu |
|------|--------|------|
| `Ctrl-w v` | split right (new tile copies the series) | View ▸ Split ▸ Right |
| `Ctrl-w s` | split below | View ▸ Split ▸ Below |
| `Ctrl-w q` | close the tile (its sibling takes the space) | View ▸ Close tile |
| `Ctrl-w h/j/k/l` (or arrows) | focus the tile left / below / above / right | |
| `Tab`, `Ctrl-w Tab`, `Ctrl-w w` | focus the next tile | |
| `Ctrl-w >` / `<` | make the tile wider / narrower | |
| `Ctrl-w +` / `-` | make the tile taller / shorter | |
| `Ctrl-w =` | equalize all splits | View ▸ Equalize |
| `Ctrl-w x` | swap the tile with the next one | |
| `Ctrl-w z` | zoom (maximize) the tile, again to restore | View ▸ Zoom tile |
| `Ctrl-w t` | switch the tile between list and chart | View ▸ Change view |
| `Ctrl-w e` | toggle the direction for **View ▸ New tile** (right / below) | |
| `Ctrl-w y` | link to the next tile / cycle / unlink | View ▸ Link tile |

Mouse: click a tile to focus it; click a tab or stack title to show that branch.

**Minimum tile size** is 20×6 cells. A split or resize that would make any tile
smaller is refused with a message in the status line.

## Containers

**View ▸ Container ▸ Split / Tabbed / Stacked** changes the container that holds the
focused tile. Tabbed shows one child at a time with a one-line tab bar; stacked shows
one title row per child. Moving focus into a hidden child (`Tab`) shows it. A single
tile has no container: split it first.

## Presets and saved layouts

**View ▸ Layouts**:

| preset | shape |
|--------|-------|
| 2-up | list │ chart |
| 3-column | list │ chart │ chart |
| Main + stack | big chart │ (list over chart) |
| 2×2 | 2×2 grid, i3 style |

Presets copy the focused tile's series into every tile. **Save current…** stores the
layout under a name (`[tui.layouts.<name>]`); **Saved…** restores one.

## List tiles

A virtualized table of bars: only the visible rows (plus a screen either side) are
read from the database, so a million-bar series scrolls instantly.

Columns: time, open, high, low, close, volume, and derived per-bar values:

| column | formula |
|--------|---------|
| chg | close − open |
| chg% | (close − open) / open × 100 |
| range | high − low |
| body | \|close − open\| |
| uwick | high − max(open, close) |
| lwick | min(open, close) − low |
| b/r | body / range |
| flags | `∅` = zero volume |

Narrow tiles show the columns that fit, from the left.

| key | action |
|-----|--------|
| ↑ ↓ / j k, PgUp PgDn, Home End | move the selection |
| `s` / `S` | sort by the next / previous column |
| `r` | reverse (ascending ↔ descending) |
| click a column header | sort by it; click again to reverse |
| `[` / `]`, click `from …` / `to …` in the title | list only bars from / to a date and time ([picker](charts.md#date-and-time-picker)) |
| `\` | clear the list's time range |
| `,` / `.` | previous / next series in this tile |

## Chart tiles

Candles with volume, period selectors, zoom/pan and toggleable layers: see
[charts](charts.md).

## Linked tiles

**View ▸ Link tile** (`Ctrl-w y`) links the focused tile to the next tile; press again
to cycle the target, and once more to unlink. The title shows `⛓<id>`.

- Moving the selection in a linked **list** centres the linked **chart** on that bar
  and puts the crosshair on it.
- Moving a linked **chart** (crosshair, pan, zoom, period) scrolls linked
  time-sorted lists to the crosshair bar (or the window start).
