# Preferences

`~/.muninn/preferences.toml` is plain, human-editable TOML. It is created with
defaults on first run:

```toml
[theme]
name = "tokyo-night"
mode = "system"

[ingest]
drop_zero_volume = false

[tui]
header = "auto"
sidebar_collapsed = false
history_open = false

[net]
label = ""
port = 4741
listen = []
mdns = true
relay_server = false
relays = []
bootstrap = []
autostart = true
accept = "known"
external_addresses = []

[hosting]
autostart = false
```

A `version` key records the file's format; older files are upgraded on load (2.9 turns
`net.autostart` on once, as the new default).

| key | default | meaning |
|-----|---------|---------|
| `theme.name` | `tokyo-night` | active theme folder under `themes/` |
| `theme.mode` | `system` | `light`, `dark` or `system` (see [themes](themes.md)) |
| `ingest.drop_zero_volume` | `false` | drop zero-volume bars instead of keeping them flagged |
| `tui.header` | `auto` | `auto` (large when the terminal has ≥ 30 rows), `compact` or `large` ([TUI](tui.md)) |
| `tui.sidebar_collapsed` | `false` | start with the sidebar hidden |
| `tui.history_open` | `false` | show the [history](history.md) bar on the left |
| `tui.sidebar` | | [sidebar](sidebar.md) sort (`sort`, `desc`) and `filters` (sources, durations, years, price metric/min/max) |
| `tui.import_recent` | `[]` | recently imported folders ([import](import.md)) |
| `tui.import_bookmarks` | `[]` | bookmarked import folders (Places) |
| `net.label` | `""` | how the node introduces itself (empty: the current author) ([network](network.md)) |
| `net.port` | `4741` | TCP and QUIC port (0: any) |
| `net.listen` | `[]` | listen addresses replacing all interfaces on `port` |
| `net.mdns` | `true` | find peers on the local network while the node runs |
| `net.relay_server` | `false` | relay traffic for other nodes |
| `net.relays`, `net.bootstrap` | `[]` | relays to reserve a slot on; nodes to join the wide network through |
| `net.autostart` | `true` | start the node when the TUI opens (`mnnn net config --autostart off`) |
| `net.accept` | `known` | `known`: nodes you were never connected to ask first; `anyone`: connect at once ([key exchanges](key-exchanges.md)) |
| `net.external_addresses` | `[]` | how other hosts reach this node from outside: `host`, `host:port` or a multiaddr, e.g. `["alien.duckdns.org:4741"]` after forwarding TCP+UDP 4741 on the router; announced and listed first in invite links (DNS names resolve when dialed) |
| `hosting.autostart` | `false` | start the [org web server](org-hosting.md) with the node (`mnnn org host autostart on`) |
| `hosting.bind` | `127.0.0.1` | address [org sites](org-hosting.md) listen on (`0.0.0.0` or `::`: other machines) |
| `hosting.port` | `8443` | HTTPS port for pages, JSON and gRPC |
| `hosting.names`, `hosting.cert_days` | `[]`, `7` | extra self-signed certificate names; its life in days |
| `hosting.acme.enabled`, `.domain`, `.email` | off | Let's Encrypt, on with a domain and an email (`mnnn org cert acme`) |
| `hosting.acme.directory`, `.profile`, `.http_port` | `letsencrypt`, `shortlived`, `80` | the CA, certificate profile and HTTP-01 port |
| `tui.layout` | one list tile | the current [tiling layout](tiling.md), saved on every change |
| `tui.layouts.<name>` | none | named layouts |

`mnnn theme set <name>`, `mnnn theme mode <mode>` and **File ▸ Preferences…** in the TUI edit the file for you.
Preferences are part of every backup ([backup and recovery](backup-recovery.md)).

Failure modes: invalid TOML makes every command fail with a `toml:` error naming
the problem; fix or delete the file (it is recreated with defaults).
