# Reports

A report is an ordered list of sections that are read from the data, journals and
projects **when the report is previewed or exported**, so a report always shows what
they hold now. Reports can stand alone or be added to [projects](projects.md), and are
exported to PDF, HTML, Word (.docx), LaTeX, JSON, CSV or plain text (see
[export formats](export-formats.md)).

| section | shows |
|---------|-------|
| `text` | written text, Markdown or LaTeX |
| `records` | one series (with an optional time range): a summary (bars, first and last, open, close, high, low, change, volume) and the latest bars (`--rows`, default 20, 0 = every bar) |
| `series` | every series a [live search](live-searches.md) matches now, or every series, one summary row each |
| `journal` | a [journal](journals.md), or some of its entries, with notes as footnotes and highlights marked |
| `project` | a project's contents: live searches, results, journals and reports |
| `model` | the records of a [model](models.md), optionally only those meeting `--where` conditions, as a table of its fields (`--rows`, default 20, 0 = all) |
| `authors` | who created the report, added its sections or entries, and later edited it |
| `code` | output of a code space; a placeholder until code spaces arrive |

## Command line

```
mnnn report list [--json]                               # alias: reports
mnnn report create NAME [--description TEXT] [--project PROJECT]
mnnn report show REPORT [--json]                        # sections with what they show
mnnn report edit REPORT [--name NAME] [--description TEXT]
mnnn report remove REPORT                               # asks first; -y skips

mnnn report add REPORT [--title T] [--at N] (
      --text TEXT | --file PATH|- [--format markdown|latex]
    | --records SOURCE:SYMBOL:PERIOD [--from TIME] [--to TIME] [--rows N]
    | --series [SEARCH]                                 # no SEARCH: every series
    | --journal JOURNAL [--entry ENTRY]...
    | --project PROJECT
    | --model MODEL [--where "FIELD OP VALUE"]... [--rows N]
    | --authors
    | --code SPACE )
mnnn report edit-section REPORT SECTION [--title T] [--text T | --file F] [--format F] [--rows N]
mnnn report remove-section REPORT SECTION
mnnn report move REPORT SECTION --to N

mnnn report preview REPORT [--width 100]                # as text in the terminal
mnnn report export REPORT [--format pdf|html|docx|latex|json|csv|text] [--out PATH]
mnnn project add PROJECT --report REPORT
```

`REPORT` is an id, unique id prefix or name; `SECTION` is the position `show` prints or
a section id prefix. Without `--out`, export writes `<report-name>.<ext>` in the
current directory. Sections without a title take one from what they show.

```
mnnn report create "Week 12" --project "ETH summer"
mnnn report add "Week 12" --title Summary --text "ETH held **3800** all week."
mnnn report add "Week 12" --records binance:ETH/USDT:1h --from 2024-03-18 --to 2024-03-25 --rows 10
mnnn report add "Week 12" --series "eth 5m"
mnnn report add "Week 12" --journal "Summer notes"
mnnn report add "Week 12" --authors --at 1
mnnn report preview "Week 12"
mnnn report export "Week 12" --out week12.pdf
```

## Terminal UI

**Projects ▸ Reports…** (`O`) lists the reports; **Projects ▸ New report…** starts one.

| key (list) | action |
|-----|--------|
| Enter, → | open the report |
| `n` / `a` | new report |
| `r` / `e` | rename / describe |
| `v` | preview |
| `E` | export (choose a format, then a path) |
| `p` | add to a project |
| `d`, `D` | delete (`y` confirms) |
| Esc, `q` | close |

| key (in a report) | action |
|-----|--------|
| `j` / `k` | move |
| `a` | add a section: choose its kind; text opens the editor, journal, project and series ask which one, records use the series selected in the sidebar with the current time range |
| Enter, `e` | edit: text in the editor (Ctrl-S saves, Ctrl-T Markdown ↔ LaTeX), records ask for the number of bars, others for a title |
| `t` | retitle the section |
| `J` / `K` | move the section down / up |
| `d` | remove the section (`y` confirms) |
| `v`, `E`, `p`, `r` | preview, export, add to a project, rename |
| Esc, ← | back to the list |

In a project, Enter on a report opens it here.

Creating, editing, moving, removing and exporting reports and sections are recorded in
the [history](history.md) and attributed to the current [author](authors.md).

See also: [export formats](export-formats.md), [projects](projects.md),
[journals](journals.md).
