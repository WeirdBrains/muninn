# In-app help

The help pages are these docs: every file in `docs/` is embedded into the binary at
build time, so the TUI help, `mnnn help` and the Markdown files never diverge.

## In the TUI

Open with `?`, F1, or the `6 Help` tab. Help opens on the topic for the screen you are
on (Navigator → [TUI](tui.md), Ingest → [CSV ingest](ingest-csv.md), Export →
[export](export.md), Keys → [keys](keys.md), Themes → [themes](themes.md)).

| key | action |
|-----|--------|
| Tab / → , Shift-Tab / ← | select the next / previous link |
| Enter | follow the selected link |
| Backspace / Alt-← | back (history is shown on the right) |
| `/` | search: type to filter, results update as you type; ↑/↓ and Enter to open |
| `i` | index ([documentation home](README.md)) |
| ↑/↓, PgUp/PgDn, Home | scroll |
| Esc, q, F1 | close |

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
