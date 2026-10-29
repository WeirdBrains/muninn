# Projects

A project collects the things you work on together:

- **live searches** ([live searches](live-searches.md)), shown with how many matches
  they have now;
- **results**: a series (source, symbol, period), optionally with a time range,
  shown with how many bars it has now;
- **journals** ([journals](journals.md)): a journal can belong to one project (a
  *project journal*), and any journal can also be linked into other projects.

Items are references: a project never copies data, so everything in it reflects the
current data. Taking an item out of a project keeps the item. Removing a project keeps
its journals, as standalone journals.

Projects are also where **rule sets** will go (a later release); the database already
has a place for them.

## Command line

```
mnnn project list [--json]
mnnn project create NAME [--description TEXT]        # alias: new
mnnn project show PROJECT [--json]                   # items with what they hold now
mnnn project edit PROJECT [--name NAME] [--description TEXT]
mnnn project remove PROJECT                          # asks first; -y skips
mnnn project add PROJECT --search SEARCH
mnnn project add PROJECT --journal JOURNAL
mnnn project add PROJECT --result SOURCE:SYMBOL:PERIOD [--from TIME] [--to TIME]
mnnn project drop PROJECT ITEM                       # ITEM: the number `show` prints
```

`PROJECT` is an id, a unique id prefix or a name. `mnnn projects` is an alias.
Adding an item that is already in the project changes nothing.

```
mnnn project create "ETH summer" --description "Breakouts, June to August"
mnnn project add "ETH summer" --search "eth 5m"
mnnn project add "ETH summer" --result binance:ETH/USDT:5m --from 2024-06-01 --to 2024-09-01
mnnn journal new "Summer notes" --project "ETH summer"
mnnn project show "ETH summer"
```

## Terminal UI

The **Projects** menu (Alt+P) holds everything in this page and in
[live searches](live-searches.md) and [journals](journals.md):

| item | key |
|------|-----|
| Projects… | `P` |
| New project… | |
| Live searches… | `F` |
| Save search as live search… | `B` |
| Journals… | `J` |
| New journal… | |
| Add selected series to project… | `a` (sidebar focused) |

The projects page lists the projects; `n` (or **New project…**) creates one.

| key (list) | action |
|-----|--------|
| Enter, → | open the project |
| `n` / `a` | new project |
| `r` / `e` | rename / describe |
| `d` | delete (`y` confirms; its journals stay) |
| Esc, `q` | close |

| key (in a project) | action |
|-----|--------|
| `j` / `k` | move |
| Enter | open the item: a live search is shown in the sidebar, a result opens its series in the focused tile (its time range becomes the time range), a journal opens |
| `s` | save the current search as a live search in this project |
| `a` | add the series selected in the sidebar (with the current time range) as a result |
| `n` | new journal in this project |
| `d` | take the item out of the project |
| `r` / `e` | rename / describe the project |
| Esc, ← | back to the list |

**Add to project** is also available on live searches (`p`), journals (`p`) and the
sidebar's series (`a`): a picker lists the projects, with **+ New project…** at the
end (or `n`).

Creating, changing and removing projects and their items are recorded in the
[history](history.md) and attributed to the current [author](authors.md).

## Later

Export and import of projects (with their journals and live searches) come with
Phase 3; the store already describes each project item by kind and reference, which is
what an export will write.

See also: [live searches](live-searches.md), [journals](journals.md),
[data model](data-model.md).
