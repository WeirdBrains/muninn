# Pipelines (design note)

Status: **design**. What ships today is the pipe layer underneath it ([pipes](pipes.md)):
`mnnn pipe SOURCE STEP… [--to TARGET]` (stdin included), share-to-code, the TUI `>`
action and the live Pipes window. This note fixes the shape of the first-class Pipeline.

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
- **Designer** (TUI window, like the Model Designer): add, connect, reorder and remove
  nodes; validate; preview the frame at any node; pick or create sink models; x/i export
  and import.
- **Monitor** (TUI tile): the graph while it runs, each node with status, rows in/out,
  throughput, last update and errors, and rows landed per sink; Enter jumps to a node's
  view or shows its latest frame.
- **Views**: running a pipeline opens its view nodes as tiles (three views: a 3-column
  layout). "Pipe to…" on a view edits the pipeline behind it.

## Order of work

1. Pipe layer: sources of every kind, code steps, filters, tees, cycles, history (done).
2. Stdin source (CSV/JSON/NDJSON, chunked) and live hop records with the Pipes window (done).
3. Pipeline object: storage, DAG run with fan-out/fan-in, validation, CLI, export/import.
4. Designer window, monitor tile, code space tile with a result chart, views as tiles.
