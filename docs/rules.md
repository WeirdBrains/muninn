# Rules

A rule says **when this happens, do that**. Every action Muninn records in the
[action history](history.md) is an event; a rule names the kinds it reacts to
(`on`), optional conditions on the action (`if`), and then-actions to run in order
(`then`).

```
mnnn rule create "Weekly after import" --on import --if "inserted > 0" \
    --then "export-report Weekly to=~/reports/weekly-{date}.pdf" \
    --then "status exported the weekly report"
```

## Where rules live

A rule lives in one place, its **home**: standalone, a [journal](journals.md) or a
[live search](live-searches.md). Independently, it can be in any number of
[projects](projects.md); a project's rules are its **rule set** (`project add --rule`,
or `--project` on `rule create`). The home is also the default target: a then-action
without a journal, live search or project uses the rule's own.

`--related` (or "Lives in" with a journal or live search) makes the rule only react to
actions that mention its home, for example entries written in its journal.

Removing a journal or live search keeps its rules, as standalone rules that are
switched off. Removing a rule takes it out of every project.

## When: events and fields

`--on` takes action kinds: `import`, `journal.create`, `journal.*` or `*`. `mnnn rule
events` lists every kind with its fields. Rule actions (`rule.*`) never trigger rules.

`--if` conditions all have to hold: `FIELD OP VALUE`.

| op | meaning |
|----|---------|
| `=` `!=` | equal (text ignores case; `*` is a wildcard) / not equal |
| `~` `!~` | contains / does not contain |
| `>` `>=` `<` `<=` | numbers compare as numbers |
| `FIELD ?` / `FIELD !?` | present / missing |

Fields: `kind`, `summary`, `origin` (`cli`, `tui`, `rule`), `author`, `author_id`,
`action` (the action id), `before.X` and `after.X` (view state), and the action's own
params (`inserted`, `name`, `journal`, `files`, …). A list field matches when any of
its values does.

## Then: actions

| then | does |
|------|------|
| `status TEXT` | a message: the TUI status line, or stderr on the command line |
| `search [SEARCH]` | runs a live search and reports how many it matches |
| `export-report REPORT to=PATH [format=F]` | exports a [report](reports.md) |
| `export-journal [JOURNAL] to=PATH [format=F]` | exports a journal |
| `add-to-project [PROJECT] [item=subject\|search:S\|journal:J\|report:R]` | adds an item to a project; `subject` is what the action was about |
| `entry [JOURNAL] "TEXT" [title=T]` | writes a journal entry |
| `note [JOURNAL] "TEXT"` | adds a note to a journal |

The format comes from the file extension unless `format=` says otherwise
([export formats](export-formats.md)); `~/` is your home folder. Texts, titles and
paths take templates: `{kind}`, `{summary}`, `{origin}`, `{author}`, `{action}`,
`{rule}`, `{date}` (2026-10-29), `{time}` (143005), `{now}` (2026-10-29 14:30) and
any field, like `{inserted}` or `{name}`. Separate several then-actions in one
string with `;`.

What a rule does is recorded like anything else: a `rule.run` action under the action
that triggered it, with the then-actions as its children (origin `rule`, the current
[author](authors.md)). A failing then-action does not stop the others; the run is
marked `failed` with the reason.

## Loops

Rules may trigger rules, within limits:

- a rule never runs again inside its own chain (A → B → A stops at the second A);
- chains stop at depth 3;
- at most 50 rule runs follow one action.

A stopped run is recorded as a `rule.run` with status `blocked`.

## Command line

```
mnnn rule list [--json]
mnnn rule create NAME --on KIND [--if COND]... --then ACTION... [--description TEXT]
                [--journal J | --search S] [--project P] [--related] [--disabled]
mnnn rule edit RULE [--name N] [--on ...] [--if ...] [--clear-if] [--then ...]
                [--journal J | --search S | --standalone] [--related | --unrelated]
mnnn rule show RULE [--json]                 # spec, home, projects, recent runs
mnnn rule enable RULE / disable RULE
mnnn rule remove RULE                        # asks first; -y skips
mnnn rule test [RULE] [--action ID]          # would it match? each condition shown
mnnn rule dry-run [RULE] [--action ID]       # ... and what it would do
mnnn rule events                             # kinds and their fields
```

`RULE` is an id, a unique id prefix or a name; `mnnn rules` is an alias. `test` and
`dry-run` check every rule when none is named, against the newest action you did
(skipping rule runs) unless `--action` says otherwise. They never change anything.
`--quiet` hides rule messages.

## Terminal UI

`R` or **Projects ▸ Rules…** opens the Rules window: each rule with `●` on / `○` off,
its trigger, then-actions, home and last run. Opening a rule item in a project shows
that rule.

| key | action |
|-----|--------|
| `j` / `k`, ↑ / ↓ | move |
| Enter | show the rule, its runs and spec |
| `n` | new rule: name, on, if, then, lives in (each step checked as you go) |
| `e` | edit the selected rule (the same steps, filled in) |
| Space | switch on / off |
| `t` / `x` | test / dry run against the newest action |
| `p` | add to a project |
| `d` | delete (`y` confirms) |
| Esc | close the details, then the window |

"Lives in" takes `journal:NAME`, `search:NAME`, `project:NAME` (stays standalone and
joins the project) or nothing. Rule messages appear on the status line as rules run,
also after background imports.

## Archives

Project archives (`.mnnp`, [export formats](export-formats.md)) carry the project's
rules and the rules owned by its journals and live searches. Imported rules come in
**switched off**, so a file from someone else never acts until you turn it on.

## Rule specs

A rule is stored as JSON (`mnnn rule show --json`):

```json
{"version": 1, "on": ["import"], "if": [{"field": "inserted", "op": ">", "value": 0}],
 "then": [{"do": "status", "text": "{inserted} new bars"}]}
```

The version and the `do` tags leave room for new triggers and then-actions (for example
scheduled or data-driven events) without changing existing rules.

See also: [action history](history.md), [projects](projects.md).
