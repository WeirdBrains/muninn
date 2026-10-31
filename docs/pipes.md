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
| `search:NAME`, any live search, run when the pipe runs | records: the model's fields (several models: a `model` column and every field); series: their bars (`source symbol base quote period timestamp open high low close volume flags`, one group per series, in the search's time range); journal entries: `journal title body words _created_at _updated_at` |
| `journal:NAME` | that journal's entries, as above |
| `code:SPACE` | that space's output, run on its own default input |

## Steps

- `code:SPACE` (or a space name): runs the space over the frame; its result becomes the
  frame: a table's columns, a chart's x and lines, text as a `text` column (tiles: the
  first table or chart). Each run is approved like any run and recorded as `code.run`
  with where its input came from.
- `search:NAME`: keeps the rows matching the search's text (over the row's values) and,
  for a records search, its conditions.
- `journal:NAME`, `model:NAME`: **saves** (like `tee`): the frame goes into a new journal
  entry (a Markdown table, with the pipe that made it) or becomes new records of the model
  (columns named like its fields), and **flows on unchanged**. Save at any step, as often
  as you like; `--to` saves the final output.

A code space may appear once per pipe, and a default input that leads back to itself is
refused (a cycle). Every pipe is recorded as `pipe.run` with its hops and saves.

## Share and the TUI

Sharing data (a model, any live search, a journal, a code space's output) to a code space
runs the space over it ([share](share.md)); the space is not changed. In the TUI the
editor then shows `piped in: …` (`x` goes back to the default input), and `>` pipes the
result on: into another code space (which opens and runs over it) or saved to a journal
or model, staying where you are so you can keep piping.
