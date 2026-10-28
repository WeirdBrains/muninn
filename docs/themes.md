# Themes

```
mnnn theme list
mnnn theme show
mnnn theme set <NAME>
mnnn theme mode <light|dark|system>
mnnn theme export <NAME> --out FILE
mnnn theme import FILE
mnnn theme reset <NAME>
```

## Built-in themes

Each has a dark and a light variant, with palettes from the published theme specs:

| theme | dark | light |
|-------|------|-------|
| `tokyo-night` (default) | Tokyo Night (Night) | Tokyo Night Day |
| `catppuccin` | Mocha | Latte |
| `gruvbox` | Gruvbox Dark | Gruvbox Light |
| `dracula` | Dracula | Alucard |
| `nord` | Nord (Polar Night) | Nord Light (Snow Storm base) |
| `one` | One Dark | One Light |
| `solarized` | Solarized Dark | Solarized Light |

Built-ins are compiled in and written to `~/.muninn/themes/<name>/` on first use. Edit
them there, or `mnnn theme reset <name>` to get the shipped version back.

## Modes

- `light` / `dark`: use that variant.
- `system`: ask the terminal for its background color (OSC 11; luminance < 0.5 means
  dark), then the OS preference (freedesktop portal `org.freedesktop.appearance
  color-scheme` on Linux, `AppleInterfaceStyle` on macOS, `AppsUseLightTheme` on
  Windows), else dark. Re-checked when the terminal regains focus.
- `--theme-mode` / `MUNINN_THEME_MODE` override the mode for one run.

Defaults in [preferences](preferences.md): `name = "tokyo-night"`, `mode = "system"`.

## interface.toml

```toml
[meta]
name = "my-theme"          # folder name: letters, digits, - and _
display_name = "My Theme"
author = "me"
base = "Tokyo Night"

[dark]
variant_name = "My Theme Dark"
bg = "bg"                  # a palette name ...
fg = "#c0caf5"             # ... or a literal #rrggbb
# every role below is required
muted = "comment"
# border, border_focus, selection_bg, selection_fg, accent, header, up, down,
# warning, error, success, info, chart_1 .. chart_6

[dark.palette]
bg = "#1a1b26"
comment = "#565f89"

[light]
# same roles

[light.palette]
```

Roles: `bg`, `fg`, `muted`, `border`, `border_focus`, `selection_bg`, `selection_fg`,
`accent`, `header`, `up` (bull candle), `down` (bear candle), `warning`, `error`,
`success`, `info`, `chart_1` ... `chart_6`. A theme missing `[dark]` or `[light]`, a role,
or with a value that is neither `#rrggbb` nor a palette name fails validation; the TUI
then falls back to the default theme and says why. `assets/` next to the file is free
for your own extras.

Terminals without truecolor get the nearest 256-colour (or 16-colour) approximation,
based on `COLORTERM` and `TERM`.

## Selector (TUI, screen 5)

↑/↓ choose a theme; the right side previews the navigator, a bar table and candles in
**both** variants side by side. Enter applies it, `m` cycles the mode
(system → dark → light), `e` opens the editor, `R` resets a built-in.

## Editor

↑/↓ pick a role, ←/→ cycle through the theme's palette, Enter types a hex value,
`v` switches between the dark and light variant, `g` generates a starting variant
from the other one, `s` saves as a new theme, `R` resets to the base, Esc leaves. The
preview updates live and a warning appears when fg/bg, selection or header contrast
drops below 4.5:1 (WCAG AA). Solarized Light's published body text is 4.13:1, so it
shows that warning by design.

Themes are part of every backup ([backup and recovery](backup-recovery.md)).
