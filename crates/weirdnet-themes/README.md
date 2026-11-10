# weirdnet-themes

A theme format for terminal and GUI front ends: one TOML file per theme with `[meta]`,
`[dark]` and `[light]` tables, 21 semantic colour roles (palette names or `#rrggbb`),
seven built-in palettes, WCAG contrast checks and lightness-inverted variant generation.

- **Storage is yours.** `load`, `save`, `list`, `materialize` and `reset` work through the
  `ThemeDirs` trait (a themes folder plus how to create folders and write files).
- **Mode.** `Mode::{Light, Dark, System}`; `resolve_variant` maps it to a `Variant`
  (`System` asks the terminal background with the default `terminal-background` feature,
  then the OS preference, else dark).
- **Colours.** `VariantDef::resolve(role)` gives RGB; `to_hex` gives `#rrggbb` (what a Dart or
  web front end consumes). With the `ratatui` feature, `Theme::from_variant` and `to_color`
  give ratatui colours by `ColorDepth`.
- **Errors.** `ThemeError` (`Invalid`, `Io`, `TomlSer`).

```rust
let t = weirdnet_themes::builtin("nord").unwrap();
assert_eq!(weirdnet_themes::to_hex(t.dark.resolve("bg").unwrap()), "#2e3440");
```
