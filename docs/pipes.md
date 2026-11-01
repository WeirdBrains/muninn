# Pipes

A code space is the right side of a pipe: data is piped in, the code works on it, and its
result can be piped on, into another code space, a filter, or saved somewhere. Code spaces
work **on** data; they keep none of it. A stored default input is only a convenience for
`mnnn code run`; whatever is piped in is what a run works on. (Named, saved pipelines that
end in a model table are being designed: [pipelines](pipelines.md).)

```
mnnn pipe SOURCE [STEP…] [--to TARGET]… [--json] [--rows N]
mnnn pipe search:usdt code:fast journal:Raw code:smooth --to journal:Notes
mnnn pipe "bars where symbol = BTC/USDT limit 500" fast
mnnn pipe journal:Notes search:alphas --to model:picks
```

## Sources

| source | the frame it gives |
|---|---|
| `none` | no data |
| a model query (`bars where … group … sort … limit N`, or `model:…`) | the model's fields, as `mnnn code edit --model` |
| `search:NAME`, any live search, run when the pipe runs | records: the model's fields (several models: a `model` column and every field); series: their rows as the bars model stores them (its own fields, one group per series, in the search's time range); journal entries: `journal title body words _created_at _updated_at` |
| `journal:NAME` | that journal's entries, as above |
| `code:SPACE` | that space's output, run on its own default input |
| `-` (CLI) | standard input, **raw**: one `text` column holding the bytes as text, nothing parsed (see [Parsers](#parsers)) |

## Steps

- `code:SPACE` (or a space name): runs the space over the frame; its result becomes the
  frame: a table's columns, a chart's x and lines, text as a `text` column (tiles: the
  first table or chart). Each run is approved like any run and recorded as `code.run`
  with where its input came from.
- `search:NAME`: keeps the rows matching the search's text (over the row's values) and,
  for a records search, its conditions.
- `project:NAME[/JOURNAL]`, `org:ORG[/JOURNAL]`, `net[:EXCHANGE|all][/JOURNAL]`: saves the
  frame as an entry in a journal (default `Pipe output`; on the network `Shared pipe output`) kept in the project, given to the
  organization, or shared on the network; the data flows on.
- `journal:NAME`, `model:NAME`: **saves** (like `tee`): the frame goes into a new journal
  entry (a Markdown table, with the pipe that made it) or becomes new records of the model
  (columns named like its fields), and **flows on unchanged**. Save at any step, as often
  as you like; `--to` saves the final output.

A code space may appear once per pipe, and a default input that leads back to itself is
refused (a cycle). Every pipe is recorded as `pipe.run` with its hops and saves.

Muninn does not define the structure of incoming data: sources hand on what they have
(records and entries as their fields, standard input as raw text), and every shape after
that, a table, bars, anything, is made by a code space step.

## Parsers

Parsing is a code space's job, placed as a step. Built-in parser code spaces ship as
templates (Rust, through the SDK), not as code in Muninn:

| built-in | does |
|---|---|
| `parse-csv` | raw text → rows: CSV with a header line, quotes as usual, numbers as numbers |
| `parse-json` | raw text → rows: a JSON array of objects (or one object) |
| `parse-ndjson` | raw text → rows: one JSON object a line |
| `to-bars` | rows → price bars (`timestamp open high low close volume`) by column names |

`mnnn code parsers` lists them, `--install` makes them code spaces, `--show NAME` prints
one; `mnnn code create mine --from parse-csv` starts your own from a copy. Installed, they
are ordinary code spaces: view, edit, share them; like any code they run once approved.

## Standard input and live runs

```
cat trades.csv | mnnn pipe - code:parse-csv code:fast --to model:trades
cat trades.csv | mnnn pipe - --format csv code:fast --to model:trades   # the same
tail -f feed.ndjson | mnnn pipe - --format ndjson code:fast --chunk 100 --to journal:Feed
```

`--format csv|json|ndjson` is a convenience: it installs the built-in parser if there is
none of that name and puts it first, a visible `code:parse-…` step in the pipe, its hops
and its history like any other. `--chunk N` runs the pipe on every N lines as they arrive
(`--keep-header` repeats the first line in every chunk, implied by `--format csv`), so long or endless input flows through. Every pipe records `pipe.start`, a
`pipe.hop` per node (rows, columns, ms, status, error) and `pipe.run`, as it goes. A TUI
open on the same home shows them live in **Projects ▸ Pipes (live)…**: each pipe with
its status and origin, and the selected one's nodes with rows, time and errors; the
status line says when one finishes or fails. Nothing to install and no daemon: the TUI
reads the history on every tick.

## Share and the TUI

Sharing data (a model, any live search, a journal, a code space's output) to a code space
runs the space over it ([share](share.md)); the space is not changed. In the TUI the
editor then shows `piped in: …` (`x` goes back to the default input), and `>` pipes the
result on: into another code space (which opens and runs over it) or saved to a journal
or model, staying where you are so you can keep piping.
