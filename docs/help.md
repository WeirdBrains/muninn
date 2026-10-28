# In-app help

The help pages are these docs: every file in `docs/` is embedded into the binary at
build time, so the TUI help, `mnnn help` and the Markdown files never diverge.

## In the TUI

Open with `?`, F1 or **Help ▸ Help topics**; **Help ▸ Search help…** opens straight
into search and **Help ▸ Keybindings** shows the [key reference](keybindings.md).
Help opens on the topic for what you are doing: the export dialog opens
[export](export.md), the keys dialog [keys](keys.md), the theme dialog
[themes](themes.md), an open menu [menus](menus.md), otherwise [TUI](tui.md).

| key | action |
|-----|--------|
| Tab / → , Shift-Tab / ← | select the next / previous link |
| Enter | follow the selected link |
| Backspace / Alt-← | back (history is shown on the right) |
| `/` | search from any topic: type to filter, results update as you type; ↑/↓ and Enter to open, Esc stops searching |
| `i` | index ([documentation home](README.md)) |
| ↑/↓, PgUp/PgDn, Home | scroll |
| Esc, q, F1 | close |

A key bar along the bottom of the help popup lists these shortcuts; while searching it
switches to the search keys. Every button in it is clickable.

With the mouse: click a link to follow it, click a search result or a history entry
(on the right) to open it, click the search box to start searching, and `[X]` closes.
Selecting a link with Tab scrolls it into view.

Search matches titles and bodies (substring); titles also match fuzzily, so `bckprcv`
finds "Backup and recovery". Headings, lists, `code` and links are rendered in the
active theme's colors.

## On the command line

```
mnnn help                 # list topics
mnnn help keys            # print a topic
mnnn help services/redis
mnnn help --search shard  # topics matching a query
```

See also [command line](cli.md).
