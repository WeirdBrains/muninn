# Share

One action shares anything shareable to anything that takes shares.

| item ↓ / target → | project | journal | code | space | net |
|---|---|---|---|---|---|
| model | added as an item | a new entry about it | runs over it | runs over it | exchange share |
| live search (any kind) | added as an item | a new entry about it | runs over its matches | runs over its matches | exchange share |
| code | added as an item | a new entry about it | runs over its output (not into itself) | runs over its output | exchange share |
| code space | added as an item | a new entry about it | runs over its output | runs over its output (not into itself) | exchange share |
| journal | added as an item | a new entry (not into itself) | runs over its entries | runs over its entries | exchange share |
| report | added as an item | a new entry about it | – | – | exchange share |
| project | – | a new entry about it | – | – | exchange share |
| pipeline | added as an item | a new entry about it | – | – | exchange share (peers get a draft) |

- **project**: the item is added to the project (once; sharing again changes nothing).
- **journal**: a new Markdown entry "Shared: NAME" naming the item, with your note.
- **code**: the code space runs over the data ([pipes](pipes.md)); the space keeps none of
  it and its input is not changed. Records, series (their bars) and journal entries all
  work. The TUI opens the space and runs it, showing the output (a new or changed source
  asks to be approved first, as `r` does), and `>` pipes the result on; the CLI runs it
  and prints the result (`-y` approves, as `mnnn code run`).
- **net**: the same as `mnnn net share` (with exchanges named, or all of them).

A model fetched from a peer arrives as a draft to approve ([model templates](model-templates.md)).

Every share is recorded in [history](history.md).

## Create then share

Wherever a share can be added, the thing can be created there too (item and destination).
Create is recorded in history before the share, with the same author. Cancelling a create
makes nothing. A net `--create-to LABEL` (or **+ New exchange…** in the picker) offers a
key exchange with that label, then shares to it (`--peer` / `--hours` optional). Sharing
with every exchange still needs no TO.

| Where | Create |
|-------|--------|
| `mnnn share` | `--create` makes the item; `--create-to NAME` makes the project/journal/code/space/net exchange |
| Orgs ▸ Resources / `mnnn org resource add` | `n` / `--create` |
| `mnnn org give` | `--create` |
| Org projects / `mnnn org project add` | `+ New project…` / `--create` |
| `mnnn project add` | `--create KIND NAME` |
| Net ▸ Shared / `mnnn net share` | `n` / `--create` |
| Reviews ask / `mnnn review request` | `+journal NAME` / `--create` |
| Org code serve | `--create` |
| Share / pipe pickers | each creatable target starts with **+ New …** |

Create flags (`--description`, `--lang`, `--code-from`, `--space-kind`, `--main`, `--format`,
`--query`) match the usual create commands. A new code space without `--main` gets a
`{name}-main` code first.

## CLI

```
mnnn share KIND ITEM TARGET [TO…] [--note TEXT] [--json]
mnnn share KIND NAME project --create --create-to Alpha
mnnn share journal Notes net --create-to lab [--peer NODE]
mnnn project add P --create result binance:BTC/USDT:5m   # series must already be imported
mnnn review request entry Monday --create [--journal J]
mnnn review request record Signals --create [--model Signals]
mnnn share KIND --targets
mnnn share search usdt project Alpha
mnnn share code calc journal Notes --note "worth a look"
mnnn share model signals code calc
mnnn share search usdt code calc -y        # run calc over the search's bars
mnnn share report summary net            # every exchange
mnnn org resource add lab journal Notes --create
mnnn project add Alpha --create journal Notes
mnnn net share journal Notes --create
```

KIND: model, search, journal, project, report, code, space, pipeline. TARGET: project, journal, code, space, net.

## TUI

In the Models, Code spaces, Journals, Live searches, Reports, Projects and Pipelines windows, `S` or
the `[S share]` button on the window's top border shares the selected item: pick a target
from the list (`target · name`), Enter shares, Esc goes back. Each creatable target starts
with **+ New target…** (name it, then the share continues).

Every key named in hint text anywhere in the TUI (key bars, the status bar, prompts such as
`y confirms`) can be clicked: the click presses that key. See [keybindings](keybindings.md).
