# Model design templates

A template is a [model](models.md)'s design in a file: its fields (types, constraints,
descriptions), its refs, the [rules](rules.md) living in it and, if you like, its records as
sample records. Import one to get the same model next to your own designs.

## Export

```
mnnn model export signals --out signals.json            # design and rules
mnnn model export signals --out signals.json --records  # plus every record as a sample
```

The file is JSON with `"format": "muninn.model-template"` and a `version` (now 1). A newer
version than this Muninn reads is refused with a note to upgrade. In a template, refs name
their model (`ref:venues`) instead of its id. The bars model exports its fields too, but
not its bars (use [export](export.md) for those).

## Import

```
mnnn model import signals.json --dry-run          # only show the plan
mnnn model import signals.json                    # a new model named as in the template
mnnn model import signals.json --name signals2    # under another name
mnnn model import signals.json --merge            # add its missing fields to `signals`
mnnn model import signals.json --draft            # a draft: `mnnn model apply` applies it
mnnn model import signals.json --no-records       # skip the sample records
```

Import first shows the migration plan, like `model plan`/`model apply`, then applies it.
A plan that loses values (only possible with `--merge`) asks first; `-y` says yes.

- **Name clashes**: when a model of that name exists, Muninn asks for a new name (offering
  `NAME_2`) or `merge`. Without a terminal, give `--name` or `--merge`.
- **Merge** adds the template's fields the model lacks; a field the model already has keeps
  its type and values (the plan lists template fields of another type as kept).
- **Refs** to other models need those models here (by name); import them first. A ref to
  the template's own model points at the new model.
- Every imported field gets a new id. **Rules** come in switched off (`mnnn rule enable`).
  **Sample records** lose their ref values (the records they pointed at are elsewhere).
- Every export and import is recorded in [history](history.md) (`model.export`, `model.import`).

## Built-in templates

```
mnnn model templates                         # the templates Muninn ships
mnnn model import builtin:bars --name candles
```

`builtin:bars` is the design of the built-in bars model (source, symbol, base, quote,
period, timestamp, open, high, low, close, volume, flags), for record tables shaped like
price bars. The bars model itself is built from it: its records are the bars, kept in its
table `m_bars`, which ingest, charts, code spaces, sync, archives and backups read and
write ([data model](data-model.md)).

## Sharing

Models are shared like anything else ([Share](share.md), `mnnn share model signals net`,
`mnnn net share`). A model fetched from a peer (`mnnn net fetch … --import`, or the
Network window) arrives as a **draft**: nothing is created until you look at it
(`mnnn model plan NAME`) and approve it with `mnnn model apply NAME`, as with code
from peers. A taken name gets `_2`. Peer models carry no rules; refs to other models
arrive as text fields.

## Terminal UI

In the Models window (**D**): **x** exports the selected model to a file (the path is
prefilled), **i** imports a file or `builtin:NAME`. A taken name asks for a new one or
`merge`; the plan is shown above the list and **y** imports it.

## Failure modes

- `not a Muninn model template`: the file has no `format` (it may be a project archive;
  use `mnnn project import`).
- `refers to model X, which is not here`: import or create X first.
- `model X already exists`: pass `--name` or `--merge`.
