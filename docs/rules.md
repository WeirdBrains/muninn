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
its values does. `mcp:SERVER/TOOL(key=value,…).path` reads an external
[MCP](mcp.md#in-rule-sets) tool's result (approved tools only).

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
| `run-code CODE` | runs one approved piece of code (building it if needed); a changed or unapproved one fails |
| `run-space SPACE [input=action\|SOURCE]` (or `space …`) | runs a [code space](code-spaces.md): its main, over main's own input — or with `input=action` over the triggering action as a one-row frame (`action`, `id`, `summary` and each param), or over a pipeline source (`search:NAME`, `journal:NAME`, `space:NAME`, `source:NAME`, a model query; `{field}` templates filled, e.g. `input=journal:{name}`) — with the same checks as `mnnn spaces run` — enabled, line-up approved, main's pin, cycles and the depth-8 limit; otherwise the rule's run fails and `rule test`/dry runs say why. Project packs carry the space |
| `notify TITLE [body=T] [link=KIND:ID] [level=info\|warn\|error]` | a [notification](notifications.md); `notification` actions are triggers too |
| `broadcast TOPIC [text=T] [to=nodes\|connections\|all\|org:SLUG]` | a [broadcast](notifications.md#broadcasts) to other nodes (queued; the running node sends it); `net.broadcast` triggers on one received |
| `mcp SERVER TOOL [KEY=VALUE…]` | calls an external [MCP](mcp.md) server's tool (values are templates); the tool must be approved first (`mnnn mcp approve`) |

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
mnnn rule create NAME (--on KIND | --ruleset SET | --every 6h | --cron "0 3 * * *") [--if COND]... --then ACTION... [--description TEXT]
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
mnnn rule tick [--json]                      # deprecated: mnnn scheduler tick
```

`RULE` is an id, a unique id prefix or a name; `mnnn rules` is an alias. `test` and
`dry-run` check every rule when none is named, against the newest action you did
(skipping rule runs) unless `--action` says otherwise. They never change anything.
`--quiet` hides rule messages.

## Rule sets and schedules (2.25)

Rules run on time through **rule sets** and **schedules** ([schedules.md](schedules.md)):

```
mnnn ruleset create Morning --rule Report --rule Cleanup
mnnn rule create Digest --ruleset Morning --then "note Log \"digest {date}\""   # no --on: reacts to `schedule`
mnnn schedule add morning "1 day" --ruleset Morning --start-at "tomorrow 07:00" --tz local
```

A schedule's run records a `schedule` action (`schedule`, `name`, `run`, `ruleset`) and
runs the set's enabled rules in order on it. Only the set's rules run: a `--on schedule`
rule outside the set does not fire.

`rule create --every`/`--cron` still work but are deprecated: they make rule set
`NAME (scheduled)` and a schedule `NAME` (this node's zone) and say so. 2.25 moved older
scheduled rules the same way on first open, and each project's rules into a rule set
owned by the project. `rule edit --every`/`--cron` still set the old per-rule schedule,
which the scheduler keeps running; move it with `mnnn schedule add`. `mnnn rule tick` is
`mnnn scheduler tick` with a warning.

### The cert expiry watcher

`cert-expiry-watch` is a built-in rule, rule set and schedule (all named so), made the
first time an org gets a domain, a certificate of yours or ACME. They are ordinary: show,
edit, copy, switch off or remove them (removed, they are not made again). The schedule
runs the set every 6 hours and the rule does
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
| `Tab` | Rule sets, then Schedules (`Enter` show, `Space` on/off, `r` run now), then back |
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
