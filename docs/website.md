# Website

The Muninn website is one static page, built with [Jaspr](https://jaspr.site) (Dart) in
`site/`. It uses the app's default theme (Tokyo Night: its colours, dark and light) and
the MUNINN header art, and links the releases, the docs and the install steps.

| file | what |
|------|------|
| `site/lib/app.dart` | the page: header art, features, install steps, docs links |
| `site/lib/banner.dart` | the header art (the same rows as `src/tui/header.rs`) |
| `site/lib/main.server.dart` | the entry point: the document and its stylesheet |
| `site/web/styles.css` | the theme colours as CSS variables, and the layout |
| `site/tool/check.sh` | checks a build: one page, the banner, the links, the theme |

## Building

It needs the Dart SDK (3.8 or later) and the Jaspr CLI; neither is needed for Muninn
itself, and the Rust build and the release workflow never run them.

```sh
cd site
dart pub get
dart pub global activate jaspr_cli 0.23.5
jaspr serve          # http://localhost:8080, reloads on change
jaspr build          # static output in site/build/jaspr
tool/check.sh
```

## Checks

CI (`.github/workflows/ci.yml`, job `site`) installs Dart in the runner, checks the
formatting, builds the site, analyzes it, runs `tool/check.sh` and keeps the built page
as the `site` artifact. `cargo test` (`tests/site.rs`) keeps the page honest without
Dart: the header art matches the TUI's, the colours match `assets/themes/tokyo-night.toml`,
and every docs page it links exists.

When the header art or the default theme changes, change the site with it.
