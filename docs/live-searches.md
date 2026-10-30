# Live searches

A live search is a saved search. Muninn stores **what you searched for**, not what it
found, so a live search always runs on the current data: import more bars or a new
series and the live search shows them the next time you open it.

A search has a **target**:

- **series** (the default): the same search the [sidebar](sidebar.md) does — fuzzy
  text, source, symbol, period, year, time range and price filters, plus a sort.
- **journals**: fuzzy text over the title and text of every [journal](journals.md)
  entry.
- **records**: the records of a designed [model](models.md) (or of every model but
  bars): fuzzy text over their text fields plus `FIELD OP VALUE` conditions.

Live searches live in the encrypted database. They can be added to
[projects](projects.md).

## Fork and optimize

- **Fork** makes a linked copy (it remembers which search it came from), optionally
  with changes. Use it to try a variation and keep the original.
- **Optimize** (`edit`, or `w` in the TUI) changes a live search in place: rename it,
  or narrow or widen what it searches for.

## Command line

```
mnnn search list [--json]                       # with how many matches each has now
mnnn search show SEARCH [--json]                # the definition
mnnn search run SEARCH [--json]                 # run it on the current data (alias: results)
mnnn search find [SPEC] [--save NAME] [--json]  # search once; --save keeps it
mnnn search save NAME [SPEC]
mnnn search fork SEARCH [--name NAME] [SPEC]    # a linked copy, changed by SPEC
mnnn search edit SEARCH [--name NAME] [SPEC]    # optimize in place (alias: optimize)
mnnn search remove SEARCH                       # asks first; -y skips (alias: rm)
```

`SEARCH` is an id, a unique id prefix (the 8 characters `list` shows) or a name.
`SPEC` is any of:

| option | meaning |
|--------|---------|
| `--query TEXT` | fuzzy text, words in any order |
| `--series` / `--journals` / `--records` | the target (default: series) |
| `--model MODEL` | records of one model (`all`: every model but bars); implies `--records` |
| `--where "FIELD OP VALUE"` | records meeting a condition, as in [rules](rules.md) (`= != ~ !~ > >= < <= ? !?`); repeatable; implies `--records` |
| `--source NAME`, `--symbol SYM`, `--period 5m`, `--year 2024` | repeatable filters |
| `--from TIME`, `--to TIME` | time range (UTC date or date and time) |
| `--price METRIC --min X --max Y` | price filter (open, high, low, close, typical, median, ohlc, vwap) |
| `--sort year\|name`, `--desc` / `--asc` | order of the series |
| `--from-view` | start from the filters and sort the TUI shows now |
| `--clear` | start with no filters (in `edit` and `fork`: drop the existing ones first) |

In `edit` and `fork` the options change the existing search; options you leave out
keep their value. `run` and `find` print only the matches (one per line, or `--json`),
so they can be piped.

```
mnnn search save "eth 5m" --symbol ETH/USDT --period 5m
mnnn search run "eth 5m"
mnnn search fork "eth 5m" --name "eth 2024" --year 2024
mnnn search edit "eth 2024" --price close --min 1000
mnnn search find --journals --query "breakout" --save breakouts
mnnn search save big --model trades --where "price > 3000"
```

## Terminal UI

**Projects ▸ Live searches…** or `F` opens the list, with the current number of
matches of each. **Projects ▸ Save search as live search…** or `B` saves what the
sidebar shows now (its search text, filters, time range and sort).

| key | action |
|-----|--------|
| `j` / `k`, ↑ / ↓ | move |
| Enter, click | show it: the sidebar takes its search and filters (a journals search lists the matching entries) |
| `n` / `a` | save the current view as a new live search |
| `f` | fork the selected search |
| `w` | optimize: replace the selected search with the current view |
| `r` | rename |
| `d` | delete (`y` confirms) |
| `p` | add to a [project](projects.md) |
| Esc, `q`, [X] | close |

Saving, forking, optimizing, renaming and removing are recorded in the
[history](history.md) and attributed to the current [author](authors.md).

See also: [projects](projects.md), [sidebar](sidebar.md), [journals](journals.md).
