# Pipelines

Status: the pipe layer ([pipes](pipes.md)) and the Pipeline object ship: storage with
versions, the DAG run (fan-out, fan-in with named inputs, every leaf a model table),
validation, `mnnn pipeline create|edit|show|list|run|remove|export|import` and history
records, the Designer window, the Monitor tile, views as tiles, and pipelines over
Share and `net` (peer pipelines arrive as drafts).

```sh
mnnn pipeline create flow --node src=search:usdt --node a=code:fast --node b=code:slow \
  --node fa=model:Fast --node sb=model:Slow --edge 'src>a>fa' --edge 'src>b>sb'
mnnn pipeline show flow        # in order, with anything wrong marked !
mnnn pipeline run flow         # asks to approve new code first
cat trades.csv | mnnn pipeline run feed - --chunk 500 --keep-header   # feed: in=- > c=code:parse-csv > …
mnnn pipeline export flow --out flow.json
mnnn pipeline import flow.json --dry-run      # the plan; then without --dry-run
```

Nodes are `ID=KIND`: `search:NAME`, `journal:NAME`, `source:QUERY` or `-` (stdin, raw
text: follow it with a parser node such as `code:parse-csv`, see [pipes](pipes.md#parsers)) as
sources; `code:SPACE`; `view:table|chart[:col,col]`; `filter:SEARCH`; `model:NAME`.
Saves along the way: `raw=code:fast+journal:Raw`. Edges are `FROM>TO[:NAME]` and chain
(`a>b>c`); a code space with several inputs reads `host.input()` (the first) or
`host.input_named("NAME")` and `host.inputs()` (the names). A node that fails marks
those downstream `skipped`; an unapproved space is `waiting approval`; the rest still run.
`pipeline edit` adds or replaces nodes and edges (`--remove-node`, `--remove-edge`); a
changed graph is a new version.

The file (`format: muninn-pipeline`, `version: 1`) holds the graph, each code space's
lang, source and trust hash (a source that does not match its hash is refused), and the
sink models by name with their templates (`--no-templates` leaves them out). Import plans
first: a code space already here with the same hash is used as is, different code of the
same name lands as `NAME (imported)`, new code is created unapproved; missing models are
created from their templates; a pipeline name clash is refused unless `--name OTHER` or
`--merge` (a new version); missing sources are listed as problems.

Share a pipeline (`mnnn share pipeline NAME project|journal|net …`, or `S` in the
Designer) like anything else. A peer fetching it with `mnnn net fetch … --import` gets the
same file and the same plan, but as a **draft**: it does not run, and the models it makes
are drafts too, until `mnnn pipeline approve NAME` (or `A` in the Designer); its code
spaces still need their own approval when it first runs.

## Principles

- **Code spaces work on data; they do not own it.** A code space is a stateless
  transform (frames in, a result out). A stored default input is a convenience only.
- **A pipeline is a graph (DAG), not a chain.** Nodes and edges. A node's output is
  computed once and broadcast to every node downstream of it (fan-out); a node may take
  several upstreams (fan-in).
- **Every leaf is a model table.** Pipelines run between sources, views and code spaces
  and terminate in model tables; there may be several. The designer asks for each leaf's
  model (or creates it from the columns arriving).
- **Saves are side branches.** Tees (journal, project, org, network, another model) save
  what passes; the stream keeps flowing. A node may have several.
- **Nothing is hidden.** Every run, node and save is recorded in the history; pipelines are
  versioned, exportable and shareable like model templates, and code in them is approved.

## The object

```
Pipeline { id, name, version, nodes: [Node], edges: [Edge], author, times }
Node  { id, kind, config, tees: [Tee] }
kind  = source(stdin | search:NAME | model query | journal:NAME | net:…)
      | code(SPACE)               # a transform
      | view(chart | table, columns, x)   # shows what passes, passes it on
      | filter(search:NAME)       # keeps the rows a search matches
      | sink(model:NAME)          # a leaf: rows land in this model table
Edge  { from, to, as: NAME? }     # `as` names the input on fan-in
Tee   = journal | project | org | net | model
```

Stored in `pipelines` (design JSON) and `pipeline_versions`, as models are, with drafts.

## Fan-out and fan-in

- **Fan-out**: a node runs once per pipeline run; every edge out of it gets the same frame.
- **Fan-in**: a node with several upstreams waits for all of them. Each input is a named
  frame (`as`, default the upstream node's name); code reads `host.input()` (the first, or
  the only one) or `host.input_named("NAME")`; `host.inputs()` lists them. A view shows
  them side by side; a sink takes their rows in order (union by column name).

## Validation

At least one source; acyclic (topological order exists); every leaf is a sink model and
every sink is a leaf; a sink's model must take the columns arriving (missing fields are
offered as a migration plan, the Model Designer's plan/apply); code nodes are checked as
`mnnn code check` does. Unapproved code leaves the pipeline "waiting approval", not
invalid.

## Running

Nodes run in topological order; independent branches may run as soon as their inputs are
ready. Each node records a hop (status idle, running, error or waiting approval; rows in
and out; ms; error) and the run records rows landed per sink. A CLI run
(`cat data.csv | mnnn pipeline run NAME -`, `--chunk N` to stream) writes hops as it
goes; an open TUI on the same home sees them on its next tick (history polling, as the
Pipes window does today: nothing to install, no daemon).

## Export, import and sharing

`mnnn pipeline export NAME --out FILE` writes a versioned template: nodes, edges, view
configs, code space sources with their trust hashes, and the sink models by name with
optional embedded model templates. `mnnn pipeline import FILE` shows a plan first, renames
or merges on clashes, imports code as unapproved, and can create missing models from the
embedded templates. Share and `net share`/`net fetch` carry pipelines; a peer's arrive as
drafts to approve, as models do.

## Surfaces

- **CLI** (thin): `mnnn pipeline create|edit|show|list|run|remove|export|import`; the
  graph is edited by node and edge (`--node ID=KIND:CONFIG`, `--edge A>B[:as]`).
  `mnnn pipe …` stays the one-off chain form.
- **Designer** (`E`, Projects ▸ Pipelines…): the list (n new, Enter edit, r run,
  m monitor, x export, i import, d remove) and a pipeline's nodes in run order with what
  each feeds and what is still wrong. `a` adds a node (`ID=KIND`), `e` connects the
  selected node (`A>B[:NAME]`), `t` adds a save there, `M` ends its branch in a model
  table (a new model takes its fields from the columns last seen at that node), `D`
  removes it, Enter opens its view or code result as a tile. Import shows the plan and
  asks; type `PATH as NAME` to rename or `PATH merge` for a new version. Runs go to a
  worker thread.
- **Monitor** (TUI tile): the graph while it runs, each node with status, rows in/out,
  throughput, last update and errors, and rows landed per sink; Enter opens a node's
  view or code tile; `r` runs. See [tiling](tiling.md#data-tiles).
- **Views**: running a pipeline, here or on the command line with a TUI open, opens its
  view nodes as tiles side by side (three views: three columns), updated in place by the
  next run or chunk.

## Order of work

1. Pipe layer: sources of every kind, code steps, filters, tees, cycles, history (done).
2. Stdin source (raw, parsed by parser code spaces; chunked) and live hop records with the Pipes window (done).
3. Pipeline object: storage, DAG run with fan-out/fan-in, validation, CLI, export/import (done).
4. Designer window, monitor tile, code space tile with a result chart, views as tiles (done).
5. Pipelines over Share and `net share`/`net fetch` (peer pipelines as drafts) (done).
6. Saves to projects, orgs and the network.
