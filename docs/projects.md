# Projects

A project collects the things you work on together:

- **live searches** ([live searches](live-searches.md)), shown with how many matches
  they have now;
- **results**: a series (source, symbol, period), optionally with a time range,
  shown with how many bars it has now;
- **journals** ([journals](journals.md)): a journal can belong to one project (a
  *project journal*), and any journal can also be linked into other projects;
- **reports** ([reports](reports.md));
- **rules** ([rules](rules.md)): the project's *rule set*.

Items are references: a project never copies data, so everything in it reflects the
current data. Taking an item out of a project keeps the item. Removing a project keeps
its journals, as standalone journals.

Removing a project keeps its rules too.

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
mnnn project add PROJECT --report REPORT
mnnn project add PROJECT --rule RULE
mnnn project drop PROJECT ITEM                       # ITEM: the number `show` prints
mnnn project export PROJECT [--format archive|json|pdf|html|docx|latex|csv|text] [--out PATH] [--rows N] [--no-data]
mnnn project import FILE [--name NAME]               # an archive (.mnnp) or JSON export
```

`PROJECT` is an id, a unique id prefix or a name. `mnnn projects` is an alias.
Adding an item that is already in the project changes nothing.

```
mnnn project create "ETH summer" --description "Breakouts, June to August"
mnnn project add "ETH summer" --search "eth 5m"
mnnn project add "ETH summer" --result binance:ETH/USDT:5m --from 2024-06-01 --to 2024-09-01
mnnn journal new "Summer notes" --project "ETH summer"
mnnn project show "ETH summer"
mnnn project export "ETH summer" --out eth-summer.mnnp
mnnn project import eth-summer.mnnp --name "ETH summer (copy)"
```

`export` writes one archive by default: the project with its journals, images, live
searches, reports, authors and the bars it refers to, importable with `import`. Other
formats render the project as a document; see [export formats](export-formats.md).

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
| Reports… | `O` |
| New report… | |
| Add selected series to project… | `a` (sidebar focused) |

The projects page lists the projects; `n` (or **New project…**) creates one.

| key (list) | action |
|-----|--------|
| Enter, → | open the project |
| `n` / `a` | new project |
| `r` / `e` | rename / describe |
| `E` | export (archive first; any document format) |
| `I` | import a project archive (`.mnnp`) or JSON pack as a new project |
| `d` | delete (`y` confirms; its journals stay) |
| Esc, `q` | close |

| key (in a project) | action |
|-----|--------|
| `j` / `k` | move |
| Enter | open the item: a live search is shown in the sidebar, a result opens its series in the focused tile (its time range becomes the time range), a journal or [report](reports.md) opens |
| `s` | save the current search as a live search in this project |
| `a` | add the series selected in the sidebar (with the current time range) as a result |
| `n` | new journal in this project |
| `d` | take the item out of the project |
| `E` | export the project |
| `r` / `e` | rename / describe the project |
| Esc, ← | back to the list |

**Add to project** is also available on live searches (`p`), journals (`p`), reports (`p`) and the
sidebar's series (`a`): a picker lists the projects, with **+ New project…** at the
end (or `n`).

Creating, changing and removing projects and their items are recorded in the
[history](history.md) and attributed to the current [author](authors.md).

See also: [live searches](live-searches.md), [journals](journals.md), [reports](reports.md),
[export formats](export-formats.md),
[data model](data-model.md).
