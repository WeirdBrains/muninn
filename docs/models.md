# Models

A **model** is a record table that you design: typed fields, references to other models,
constraints and defaults. You change a design in a **draft**. **Applying** the draft turns it into
the next **version**, which is a migration you can preview first and roll back later. Every
record carries the version it was last written or migrated under. Rules can create and edit
models, and they can feed records into them. Projects, `.mnnp` archives and backups carry
models with their history, records and rules.

Imported bars are the records of the built-in **bars** model. It is designed like any other
model: rename, retype, change or drop its fields (`source`, `symbol`, `period`, `timestamp`,
`open` … `flags`, shown as *from imports*), add your own (a `flag` or a `note` on each bar),
rename the model, or remove it. Muninn does not decide what your bars mean.

## Fields

A field spec is `NAME:TYPE` followed by constraints:

| Type | Holds |
|------|-------|
| `text` | text (aliases `string`, `str`) |
| `integer` | whole numbers (`int`) |
| `real` | numbers (`float`, `number`, `decimal`) |
| `bool` | true/false (`yes`/`no`, `1`/`0`) |
| `timestamp` | a time: epoch seconds or milliseconds, or a date such as `2024-04-20 12:00` |
| `json` | any JSON value |
| `ref:MODEL` | the id of a record of another model (or of the same model) |

| Constraint | Meaning |
|-----------|---------|
| `required` or a trailing `!` (`price:real!`) | every record has a value |
| `unique` | no two records share a value |
| `index` | indexed for fast filters (refs always are) |
| `default=V` | used when a record leaves the field out, and for existing records when the field is added |
| `choices=a\|b\|c` | text fields only: the allowed values |
| `about="…"` | a description |
| `optional`, `-unique`, `-index`, `-default`, `-choices`, `-about` | take a constraint away |

Several specs are separated by `;`, for example
`symbol:text!; price:real!; side:text choices=buy|sell; idea:ref:ideas`.

## Drafts, plans and migrations

Every change goes to the model's draft: adding, changing, dropping, renaming or moving a
field. The **plan** compares the draft with the applied version, record by record:

- `+` adds a field (existing records get its default)
- `-` drops a field and says how many values that throws away
- `~` changes a type or a constraint; it says how many values would not convert, and which
  records break a new `required` or `unique`
- renames keep the values, because fields are matched by an id that never changes

A plan that cannot work is **blocked**, and it tells you what to fix (for example, add a
default before making a field with empty values required). A plan that loses values asks
for confirmation (`-y` on the command line, `y` in the TUI, `destructive=yes` in a rule).
A migration is a single transaction, so it either completes or changes nothing.

`history` lists the versions: when each was made, by which author, whether a rule made it,
and its steps. **Rolling back** to v*N* applies the design of v*N* as a new version, so the
history only grows. The rollback has its own plan, because values written since v*N* may not
fit.

How the built-in bars model changes:
- the bars keep their imported values whatever the design says, so imports, charts, the
  sidebar, symbol and time filters and bar ids work as before;
- renaming a field from imports, or changing its about, default, index, required, unique or
  choices, loses nothing (required, unique and choices are checked against the bars);
- dropping one hides it from the model (nothing is lost: the bars keep the values, and a
  rollback shows them again). Searches, rules and code that name it get an error saying it
  was dropped, or what it is called now;
- giving one another type copies its converted values into the model's own table, where
  they can then hold anything; the plan counts values that do not convert, and changing the
  type back reads the imported values again;
- a field you make required needs a default (new bars from imports get it);
- rolling back to v0 restores the design as shipped;
- removing the bars model deletes every bar (it asks first) and starts the model again as
  shipped.

Models that other models refer to cannot be removed until those references are dropped.

## Records

Records have an `id` and the hidden fields `_version`, `_created_at`, `_updated_at` and
`_author`. Values are converted to the field's type (`price=42000` is a number). Conditions
take the same form as rule conditions: `FIELD OP VALUE` with `= != ~ !~ > >= < <=`, or
`FIELD ?` (present) and `FIELD !?` (missing). `=` takes `*` wildcards, and text comparisons ignore
case. On bars, comparisons on `timestamp`, `open` … `volume`, and `=` on `period`, `source`,
`base` and `quote`, go straight to the bars' own indexed columns, so they stay fast on millions of bars.

Grouping counts the records per value of one or more fields, and can add `sum:F`,
`avg:F`, `min:F` or `max:F` for each group.

Every change is recorded in the [action history](history.md) with the current author:
`model.create`, `model.edit`, `model.apply`, `model.rollback`, `model.remove`,
`record.create`, `record.update` and `record.delete`.

## Command line

```
mnnn model list                                    # every model, its version and records
mnnn model types
mnnn model create signals --fields "symbol:text!; price:real!; side:text choices=buy|sell" \
    --description "trade ideas" [--draft] [--project P]
mnnn model edit signals --add "note:text default=new" --rename side:direction \
    --change "price:real default=0" --drop old --move note:1 [--apply]
mnnn model edit signals --name trade_signals --description "…"
mnnn model show signals [--json]
mnnn model plan signals [--json] [--then]          # --then prints a model-edit rule action
mnnn model apply signals [--dry-run]               # -y when values are lost
mnnn model discard signals
mnnn model history signals [--json]
mnnn model rollback signals 2 [--dry-run]
mnnn -y model remove signals
mnnn model export signals --out signals.json [--records]   # a template file
mnnn model import signals.json [--name N | --merge] [--draft] [--dry-run]
mnnn model templates                                       # built-in templates (builtin:bars)

mnnn model record create signals symbol=BTC/USDT price=42000 side=buy
mnnn model record update signals --id 3f2a note=checked
mnnn model record update signals --where "side = buy" --where "price > 40000" note=big
mnnn model record list signals --where "price > 40000" --sort -price --limit 20 [--json]
mnnn model record list signals --text btc
mnnn model record list signals --group side --agg avg:price --agg max:price
mnnn model record show signals 3f2a
mnnn model record delete signals 3f2a … | --where "side = sell" [--dry-run]
```

In `edit`, the operations run in this order: renames, then changes, drops, adds and moves.
Assignments are `FIELD=VALUE`, and an empty value clears a field. Changing or deleting
records with `--where` asks first. Bars are records like any other: create them (give the
source, symbol, period, timestamp and prices, under the names the model gives them; fields
left out get their defaults), update any field (a change to source, symbol, period or
timestamp gives the bar a new id) and delete them. `model record list` on bars shows
50 records unless `--limit` is given (`--limit 0` lists all).

## Terminal UI

**D** (or **Data ▸ Model designer…**) opens the designer.

| Screen | Keys |
|--------|------|
| Models | **Enter** design · **n** new (name, then fields) · **r** records · **h** history · **f** rules · **p** add to a project · **x** export a template · **i** import one · **d** remove |
| Design | **a** add a field · **e**/**Enter** edit it (the whole spec; the field keeps its id, so a rename keeps values) · **x** drop · **u** undo the draft's change to the field · **K**/**J** move up/down · **A** apply (asks when values are lost) · **z** discard the draft · **N** rename the model · **i** description · **d** remove |
| History | **Enter** show a version (fields, steps, what rolling back to it would do) · **b**/**R** roll back to it (asks) |
| Records | **Enter** show · **n** new · **e** edit · **d** delete · **w** where · **/** text · **s** sort (`price`, `-price`) · **g** group (`side, sum:price`) · **U** update all matching · **D** delete all matching · **c** clear · **PgUp**/**PgDn** |
| Rules | **n** new rule living in the model (name → on → if → then) · **x** dry run on the newest action · **Enter** run now · **Space** on/off · **d** delete |

**m**, **h**, **r** and **f** switch screens, and **Esc** goes back. The design screen shows the
draft's fields with their marks (`+` new, `~` changed, `-` dropped, `was price` after a
rename), and next to them the migration plan for the records as they are now. A new rule
is prefilled with a `record-create` action that copies the trigger's fields into the model.

## Rules

Rules can react to `model.*` and `record.*` actions, and they can act on models (see
[Rules](rules.md)):

```
model-create NAME fields="a:text; b:real" [about=T] [draft=yes]
model-edit [MODEL] add=SPEC change=SPEC drop=F rename=A:B move=F:N [apply=yes] [destructive=yes]
model-apply [MODEL] [destructive=yes]
model-remove [MODEL] [destructive=yes]
record-create [MODEL] FIELD=VALUE…
record-update [MODEL] id=ID|where="F OP V; …" FIELD=VALUE…
record-delete [MODEL] id=ID|where="F OP V; …"
```

- If a rule leaves the model out, it uses the model it lives in (`mnnn rule create … --model signals`).
- Values and conditions take templates: `{summary}`, `{FIELD}` and the rest.
- `model-create` does nothing when the model already exists.
- Versions a rule makes have the note `rule NAME`.
- If an `apply=yes` edit is refused, the draft stays as it was.

```
mnnn rule create Feed --model notes --on entry.add --then "record-create text={summary} kind=entry"
mnnn rule create Grow --on record.create --if "name = notes" \
    --then 'model-edit notes add="seen:bool default=false" apply=yes'
```

## Live searches, reports and projects

- **Live searches** can search records: across every model (not bars) or one model, with
  text and conditions (see [Live searches](live-searches.md)).
- **Reports** have a *model records* section: a model's fields and a table of its records,
  optionally filtered (see [Reports](reports.md)).
- **Projects** can hold models (`mnnn project add P --model signals`).
- **Project archives** (`.mnnp`) carry:
  - the models used by their items, report sections, live searches and rules, plus the
    models those refer to;
  - each model's versions, records (record ids are kept), the rules that live in it and the rules that
    created or migrated it.

  On import:
  - a model whose name is taken is imported as `NAME_imported`;
  - imported rules start switched off;
  - fields added to bars arrive as optional fields on the local bars model, with their
    values on the archive's bars (the local names of the fields from imports stay).
- **Backups** contain everything.

See [Model design templates](model-templates.md) for exporting, importing and sharing designs.

## More commands

```
mnnn model remove MODEL          # removes the model and its records; rules in it become standalone and off
```
