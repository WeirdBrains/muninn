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

A rule lives in one place, its **home**: standalone, a [journal](journals.md), a
[live search](live-searches.md) or a [model](models.md). Independently, it can be in any number of
[projects](projects.md); a project's rules are its **rule set** (`project add --rule`,
or `--project` on `rule create`). The home is also the default target: a then-action
without a journal, live search, model or project uses the rule's own.

`--related` (or "Lives in" with a journal or live search) makes the rule only react to
actions that mention its home, for example entries written in its journal.

Removing a journal, live search or model keeps its rules, as standalone rules that are
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
| `model-create NAME fields="a:text; b:real" [about=T] [draft=yes]` | creates a [model](models.md) (applied as v1 unless `draft=yes`; nothing happens if it exists) |
| `model-edit [MODEL] add=SPEC change=SPEC drop=F rename=A:B move=F:N [apply=yes] [destructive=yes]` | changes a model's draft, in the order given; `apply=yes` applies it |
| `model-apply [MODEL] [destructive=yes]` | applies a model's draft |
| `model-remove [MODEL] [destructive=yes]` | removes a model (with records: `destructive=yes`) |
| `record-create [MODEL] FIELD=VALUE…` | adds a record |
| `record-update [MODEL] id=ID\|where="F OP V; …" FIELD=VALUE…` | changes one record, or all that match |
| `record-delete [MODEL] id=ID\|where="F OP V; …"` | deletes one record, or all that match |
| `run-code SPACE` | runs an approved [code space](code-spaces.md) (building it if needed); a changed or unapproved one fails |

The format comes from the file extension unless `format=` says otherwise
([export formats](export-formats.md)); `~/` is your home folder. Texts, titles and
paths take templates: `{kind}`, `{summary}`, `{origin}`, `{author}`, `{action}`,
`{rule}`, `{date}` (2026-10-29), `{time}` (143005), `{now}` (2026-10-29 14:30) and
any field, like `{inserted}` or `{name}`; so do record values and `where=` values.
Separate several then-actions in one string with `;`.

Migrations a rule applies are noted `rule NAME` in the model's history. One that would
throw values away fails unless it says `destructive=yes`, and a refused `model-edit …
apply=yes` leaves the draft as it was. A model's own **Rules** screen (`D`, then `f`)
makes rules that live in the model, starting from a `record-create` that copies the
trigger's fields.

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
mnnn rule create NAME (--on KIND | --every 6h | --cron "0 3 * * *") [--if COND]... --then ACTION... [--description TEXT]
                [--journal J | --search S | --model M] [--project P] [--related] [--disabled]
mnnn rule edit RULE [--name N] [--on ...] [--if ...] [--clear-if] [--then ...]
                [--journal J | --search S | --model M | --standalone] [--related | --unrelated]
                [--every I | --cron C | --no-schedule]
mnnn rule show RULE [--json]                 # spec, home, projects, recent runs
mnnn rule enable RULE / disable RULE
mnnn rule remove RULE                        # asks first; -y skips
mnnn rule test [RULE] [--action ID]          # would it match? each condition shown
mnnn rule dry-run [RULE] [--action ID]       # ... and what it would do
mnnn rule events                             # kinds and their fields
mnnn rule tick [--json]                      # run the scheduled rules that are due
```

`RULE` is an id, a unique id prefix or a name; `mnnn rules` is an alias. `test` and
`dry-run` check every rule when none is named, against the newest action you did
(skipping rule runs) unless `--action` says otherwise. They never change anything.
`--quiet` hides rule messages.

## Scheduled rule sets

A rule can run on a schedule instead of (or as well as) on actions:

- `--every 30m`, `6h`, `1d`, `1w` (at least a minute): an interval after its last run; a
  new one runs at once.
- `--cron "0 3 * * *"`: five fields, minute hour day month weekday, in local time, with
  `*`, lists (`0,30`), ranges (`9-17`) and steps (`*/15`); Sunday is 0 or 7.

When a scheduled rule is due, a `schedule` action naming it is recorded and the rule runs
on it like on any action (its `--if` conditions see `rule`, `name` and `schedule`). Only
that rule reacts to its schedule action, so a `--on "*"` rule does not fire every tick.
`show` gives the schedule and the next run; `test` and `dry-run` check a scheduled rule
against its schedule action unless `--action` is given; `edit --every`/`--cron` changes
the schedule and `--no-schedule` drops it (the rule then needs `--on`).

Who runs them: the TUI (about once a minute) and a running org host
(`mnnn org host start`, or the TUI's site) check for due rules; when a host serves from
this node, it does and the TUI leaves it to it. With neither running, call
`mnnn rule tick` from cron or a systemd timer. A run missed while nothing was running
happens once when something next checks.

### The cert expiry watcher

`cert-expiry-watch` is a built-in scheduled rule set, made the first time an org gets a
domain, a certificate of yours or ACME. It is an ordinary rule: show, edit, copy,
switch off or remove it (removed, it is not made again). It runs every 6 hours and does
`cert-watch renew=30 notify=30,14,3`:

- **Let's Encrypt certificates** with fewer than `renew` days left (or none yet) are
  renewed by the running host (it is asked; renewals retry with backoff, an hour doubling
  to a day, and failures notify). With no host running it notifies you to start one.
- **Your own certificates** notify once at each of the `notify` days and when they
  expire, telling you to renew with your provider and `mnnn org cert set` the new one.

Notifications are `notify` actions (`level`, `about` = `cert.expiry` or `cert.renew`,
`message`, `org`, `days_left`): in the history, on the status line, and triggers for
rules (`--on notify --if "about = cert.expiry" --then "status {message}"`). The header
bell and node broadcasts come with Notifications (a later release) and read these same
actions.

## Terminal UI

`R` or **Projects ▸ Rules…** opens the Rules window: each rule with `●` on / `○` off,
its trigger, then-actions, home and last run. Opening a rule item in a project shows
that rule.

| key | action |
|-----|--------|
| `j` / `k`, ↑ / ↓ | move |
| Enter | show the rule, its runs and spec |
| `n` | new rule: name, on, if, then, lives in (each step checked as you go); "on" also takes `every 6h` or `cron "0 3 * * *"`, optionally `; KINDS` |
| `e` | edit the selected rule (the same steps, filled in) |
| Space | switch on / off |
| `t` / `x` | test / dry run against the newest action (a scheduled rule: its schedule) |
| `p` | add to a project |
| `d` | delete (`y` confirms) |
| Esc | close the details, then the window |

"Lives in" takes `journal:NAME`, `search:NAME`, `model:NAME`, `project:NAME` (stays standalone and
joins the project) or nothing. Rule messages appear on the status line as rules run,
also after background imports.

## Archives

Project archives (`.mnnp`, [export formats](export-formats.md)) carry the project's
rules and the rules owned by its journals, live searches and models. Imported rules come in
**switched off**, so a file from someone else never acts until you turn it on.

## Rule specs

A rule is stored as JSON (`mnnn rule show --json`):

```json
{"version": 1, "on": ["import"], "if": [{"field": "inserted", "op": ">", "value": 0}],
 "then": [{"do": "status", "text": "{inserted} new bars"}]}
```

A scheduled rule adds `"schedule": {"every": 21600000}` (milliseconds) or
`{"cron": "0 3 * * *"}`; rules without one read as before. The version and the `do` tags
leave room for new triggers and then-actions without changing existing rules.

See also: [action history](history.md), [projects](projects.md).

## More commands

```
mnnn rule disable RULE           # switch a rule off (enable turns it back on)
```
