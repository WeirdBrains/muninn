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
```

| key | default | meaning |
|-----|---------|---------|
| `theme.name` | `tokyo-night` | active theme folder under `themes/` |
| `theme.mode` | `system` | `light`, `dark` or `system` (see [themes](themes.md)) |
| `ingest.drop_zero_volume` | `false` | drop zero-volume bars instead of keeping them flagged |
| `tui.header` | `auto` | `auto` (large when the terminal has ≥ 30 rows), `compact` or `large` ([TUI](tui.md)) |
| `tui.sidebar_collapsed` | `false` | start with the sidebar hidden |
| `tui.sidebar` | | [sidebar](sidebar.md) sort (`sort`, `desc`) and `filters` (sources, durations, years, price metric/min/max) |
| `tui.layout` | one list tile | the current [tiling layout](tiling.md), saved on every change |
| `tui.layouts.<name>` | none | named layouts |

`mnnn theme set <name>`, `mnnn theme mode <mode>` and **File ▸ Preferences…** in the TUI edit the file for you.
Preferences are part of every backup ([backup and recovery](backup-recovery.md)).

Failure modes: invalid TOML makes every command fail with a `toml:` error naming
the problem; fix or delete the file (it is recreated with defaults).
