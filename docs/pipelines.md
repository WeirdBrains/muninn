# Pipelines (design note)

Status: **design**. What ships today is the pipe layer underneath it ([pipes](pipes.md)):
`mnnn pipe SOURCE STEP… [--to TARGET]`, share-to-code and the TUI `>` action. This note
fixes the shape of the first-class Pipeline object built on top of it.

## Principles

- **Code spaces work on data; they do not own it.** A code space is a stateless
  transform (a frame in, a result out). A stored "default input" is a convenience only.
- **Pipelines bind data to code.** A pipeline is a saved, named graph: what flows from
  where, through what, into which model table.
- **Every pipeline ends in a model table.** The terminating node is required; the designer
  asks for one (or creates it from the last node's columns). Views and code spaces are
  intermediate steps.
- **Saves are side branches.** A tee (journal, project, org, network, another model) saves
  what passes and the stream keeps flowing. Any node may have several.
- **Nothing is hidden.** Every run, hop and save is in the history; pipelines are versioned,
  exportable and shareable like model templates, and code in them is approved like code.

## The object

```
Pipeline { id, name, version, nodes: [Node], edges: [(from, to)], author, times }
Node = Source(stdin | search:NAME | model query | journal:NAME | net:…)
     | Code(SPACE)                         # a transform
     | View(chart | table, columns…)       # shows the frame passing; passes it on
     | Filter(search:NAME)                 # keeps the rows a search matches
     | Tee(journal | project | org | net | model)   # saves, passes on
     | Sink(model:NAME)                    # required end: rows land here
```

Stored in `pipelines` (design JSON) and `pipeline_versions`, as models are. A run is a
`pipeline.run` action with a hop record per node (rows in/out, ms, status, error).

## Validation

One or more sources; every path ends in exactly one sink model; no cycles (code spaces
may appear once per path); a code node's language/inputs are checked as `mnnn code check`
does; a sink's model must take the columns arriving (missing fields are offered as a
migration plan, the same plan/apply as the Model Designer). Unapproved code makes the
pipeline "waiting approval", not invalid.

## Surfaces

- **CLI** (thin): `mnnn pipeline create|edit|show|list|run|remove|export|import`;
  `mnnn pipe …` stays as the one-off form (`--save NAME` turns it into a pipeline).
- **Designer** (TUI window): add, connect, reorder and remove nodes; validate; preview the
  frame at any node; pick or create the sink model.
- **Monitor** (TUI tile): the graph of a running pipeline, each node with status (idle,
  running, error, waiting approval), rows in/out and throughput, last update, errors, and
  rows landed in the sink; select a node to open its view or inspect its latest frame.
- **Views**: running a pipeline opens its view nodes as tiles (a 3-column layout = a
  pipeline with three view nodes). "Pipe to…" on a view edits the pipeline behind it.
- **Live**: a CLI run (e.g. `cat data.csv | mnnn pipeline run NAME -`) writes its hop
  records as it goes; an open TUI on the same home picks them up on its history refresh
  (no daemon, nothing to install).

## Order of work

1. Pipe layer (done): sources of every kind, code steps, filters, tees, cycles, history.
2. Stdin source (CSV/JSON/NDJSON) and live hop records.
3. Pipeline object + CLI + validation (sink model required).
4. Designer window, then the monitor tile, then views as tiles.
