# Share

One action shares anything shareable to anything that takes shares.

| item ↓ / target → | project | journal | code | net |
|---|---|---|---|---|
| model | added as an item | a new entry about it | runs over it | exchange share |
| live search (any kind) | added as an item | a new entry about it | runs over its matches | exchange share |
| code space | added as an item | a new entry about it | runs over its output (not into itself) | exchange share |
| journal | added as an item | a new entry (not into itself) | runs over its entries | exchange share |
| report | added as an item | a new entry about it | – | exchange share |
| project | – | a new entry about it | – | exchange share |

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

## CLI

```
mnnn share KIND ITEM TARGET [TO…] [--note TEXT] [--json]
mnnn share KIND --targets
mnnn share search usdt project Alpha
mnnn share code calc journal Notes --note "worth a look"
mnnn share model signals code calc
mnnn share search usdt code calc -y        # run calc over the search's bars
mnnn share report summary net            # every exchange
```

KIND: model, search, journal, project, report, code. TARGET: project, journal, code, net.

## TUI

In the Models, Code spaces, Journals, Live searches, Reports and Projects windows, `S` or
the `[S share]` button on the window's top border shares the selected item: pick a target
from the list (`target · name`), Enter shares, Esc goes back.

Every key named in hint text anywhere in the TUI (key bars, the status bar, prompts such as
`y confirms`) can be clicked: the click presses that key. See [keybindings](keybindings.md).
