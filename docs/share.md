# Share

One action shares anything shareable to anything that takes shares.

| item ↓ / target → | project | journal | code | net |
|---|---|---|---|---|
| model | added as an item | a new entry about it | becomes the input model | exchange share |
| live search, report, code space | added as an item | a new entry about it | – | exchange share |
| journal | added as an item | a new entry (not into itself) | – | exchange share |
| project | – | a new entry about it | – | exchange share |

- **project**: the item is added to the project (once; sharing again changes nothing).
- **journal**: a new Markdown entry "Shared: NAME" naming the item, with your note.
- **code**: a code space reads the model (its input `model`).
- **net**: the same as `mnnn net share` (with exchanges named, or all of them).

Every share is recorded in [history](history.md).

## CLI

```
mnnn share KIND ITEM TARGET [TO…] [--note TEXT] [--json]
mnnn share KIND --targets
mnnn share search usdt project Alpha
mnnn share code calc journal Notes --note "worth a look"
mnnn share model signals code calc
mnnn share report summary net            # every exchange
```

KIND: model, search, journal, project, report, code. TARGET: project, journal, code, net.

## TUI

In the Models, Code spaces, Journals, Live searches, Reports and Projects windows, `S` or
the `[S share]` button on the window's top border shares the selected item: pick a target
from the list (`target · name`), Enter shares, Esc goes back.

Every key named in hint text anywhere in the TUI (key bars, the status bar, prompts such as
`y confirms`) can be clicked: the click presses that key. See [keybindings](keybindings.md).
