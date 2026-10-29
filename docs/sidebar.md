# Sidebar

The sidebar sits at the right of the content area and lists every series
(`source · SYMBOL · duration`). Toggle it with `Ctrl-b` or **View ▸ Sidebar**; the
collapsed/expanded state is saved in [preferences](preferences.md). It hides on
terminals narrower than 60 columns.

```
┌ Series ─────────────────────────┐
│ / sol                           │
│ sort: year ▴   [f][p][d][y]     │
│ ×binance ×2021 · clear all      │
│ ▸ binance SOLUSDT 1h  2020–2024 │
└─────────────────────────────────┘
```

## Search

`/` focuses the sidebar and starts a fuzzy search: the letters you type must appear
in order in the source, symbol and duration (`sl1h` matches `SOLUSDT 1h`).
Separators don't matter (`btc-usdt`, `btc_usdt` and `btcusdt` all find `BTC/USDT`),
and several words match in any order (`binance btc`). Enter or
Esc ends typing; the list keeps the filter until you clear the text.

## Keys while the sidebar is focused

| key | action |
|-----|--------|
| `j` / `k`, ↑ / ↓, PgUp / PgDn, Home / End | move |
| Enter, click a row | show that series in the focused tile |
| `s` | sort by year (first bar) or by name |
| `o` | toggle ascending / descending (also works anywhere) |
| `f` `m` `p` `d` `y` | edit the source, symbol, price, period (duration) or year filter |
| `t` | set the time range (date and time picker) |
| `X` | clear all filters (also works anywhere) |
| `a` | add the selected series (with the current time range) to a [project](projects.md) |
| Esc, Tab, ← | back to the tiles |

## Filters

Active filters show as chips; click a chip's `×` to remove it, or **clear all**.

- **Source**, **Symbol**, **Period**, **Year**: checklists, Space toggles; several values
  combine with OR, different filters with AND.
- **Price**: choose the metric (open, high, low, close, typical, median, OHLC
  average, VWAP) and an optional min and/or max. A series matches when its range of
  that metric overlaps `[min, max]`.

- **Time range**: `T` anywhere (or **Data ▸ Filters ▸ Time range…**, or a click on the sidebar's `time` row) opens the date and time picker,
  first for *from*, then for *to* (`x` leaves that end open). It applies to every
  view and tile type: lists list only bars in the range, charts plot only that
  range, and the sidebar hides series with no bars in it. Chips `from …` / `to …`
  remove it.

The year filter and the time range **scope the tiles**: lists and charts only show
bars inside them (a list tile's own `[` / `]` range narrows it further). `,` / `.` in a tile step through the series the sidebar shows.
Sort and filters are saved in [preferences](preferences.md).

Also on the **Data** menu (Filter, Clear filters, Sort by year/name, Toggle order).

The current search, filters and sort can be kept as a [live search](live-searches.md)
(`B`) that always runs on the current data.
