# Charts

A chart tile (`Ctrl-w t` on a list tile, **View ▸ New tile ▸ Chart**, or a preset)
draws its series over a time window with toggleable layers. Each chart tile has its
own series, window, zoom and layers; all of it is saved with the
[layout](tiling.md).

```
start ◂ 2017-09-01 10:00 ▸   end ◂ 2017-09-02 10:00 ▸   5m/col · live
2017-09-02 09:55:00  o 4580.0  h 4590.1  l 4575.2  c 4588.8  v 12.3  +0.19%
│ candles, lines, markers                                       │  4590.10
│                                                               │  4580.00
│ volume sub-pane                                               │
└09-01 10:00     └09-01 11:20 ...
```

## Window, zoom and pan

The window ends at **end** (or the latest bar when `live`) and spans one bucket per
column. Zoom sets the bucket size in bars per column (1×, 2×, 4×, …); buckets
aggregate bars (open = first, close = last, high/low = extremes, volume = sum), so even
a full million-bar series zoomed out reads only one row per column.

| input | action |
|-------|--------|
| `+` / `-`, mouse wheel | zoom in / out (keeps the centre) |
| `h` / `l` | pan a quarter screen earlier / later |
| PgUp / PgDn, `H` | pan a full screen |
| Home | first bars of the series |
| End, `0` | back to the latest bar (`live`, follows new data) |
| `[`, click the start date | pick the period **start** (keeps the end, zooms to fit) |
| `]`, click the end date | pick the period **end** (`x` in the picker: back to live) |
| `{` / `}` | type the start / end instead |
| `@` | jump to a date (centres it and puts the crosshair there) |
| click `◂` / `▸` | move start or end by a quarter of the window |
| ← / →, click in the chart | move the crosshair |
| `,` / `.` | previous / next series |

## Date and time picker

`[` / `]` (and clicking a date) open a picker with year, month, day, hour and minute
(UTC). The same picker sets a list tile's range ([tiling](tiling.md)).

| input | action |
|---|---|
| ← / →, Tab / Shift-Tab, click a field | select a field |
| ↑ / ↓, click `▲` / `▼` | change it (months and years keep the day valid); PgUp / PgDn by 10 |
| digits | type the field, e.g. `2017-08-17 06:30` fills every field |
| `n` | now |
| Enter / `[Enter OK]` | apply |
| `x` / `[x clear]` | clear a list range end, or make the chart end live |
| Esc / `[Esc cancel]` / `[X]` | close without changes |

Typed dates (`{` `}` `@`) are `YYYY-MM-DD`, `YYYY-MM-DD HH:MM[:SS]` (UTC) or epoch seconds/ms.

## Layers

Every element is a layer, toggled independently with its key, from the layer menu
(`L`, or **View ▸ Chart layers…**), and saved per tile.

| layer | key | default |
|-------|-----|---------|
| Candles | `c` | on |
| OHLC bars (`┤` open, `├` close; shown when candles are off) | `b` | off |
| Line (metric: open/high/low/close/typical/median/OHLC avg; `N` cycles) | `n` | off |
| Area under the line | `a` | off |
| High/low envelope (20-bucket highest high / lowest low, `┄`) | `e` | off |
| Volume bars (sub-pane, up/down colours) | `v` | on |
| Moving averages (default SMA 20 and 50; `M` sets e.g. `sma20, ema50, sma200`) | `m` | off |
| Wick markers (upper wick longer than the body highlighted; `▼` over shooting stars) | `w` | off |
| Gap markers (`┊` where buckets are missing) | `g` | on |
| Zero-volume markers (`○`) | `z` | off |
| Grid | `#` | on |
| Crosshair + value readout | `x` | on |
| Price axis / time axis | `y` / `t` | on |

Layers draw in a fixed order: grid < area < volume < candles/bars < lines (line,
envelope, moving averages) < markers < crosshair, coloured with the theme roles
`up`, `down`, `chart_1`…`chart_6`, `warning` and `muted` ([themes](themes.md)).

A shooting star is a bar whose upper wick is at least twice the body and at least 60%
of the range, with a lower wick under 25% of the range.

## Linking

Link a chart to a list (`Ctrl-w y`) to keep them in step: see
[linked tiles](tiling.md).
