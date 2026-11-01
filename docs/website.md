# Website

The Muninn website is one static page, built with [Jaspr](https://jaspr.site) (Dart) in
`site/`. It uses the app's default theme (Tokyo Night: its colours, dark and light) and
the MUNINN header art, shows a hero screenshot under the header and a Screenshots section
(one captioned shot per feature, lazy-loaded), and links the releases, the docs and the install steps.

| file | what |
|------|------|
| `site/lib/app.dart` | the page: header art, hero shot, features, screenshots, install steps, docs links |
| `site/lib/shots.dart` | the hero and screenshot list (written by `scripts/screenshots.sh`) |
| `site/web/screenshots/` | the screenshots (PNG, 1464x890) |
| `site/lib/banner.dart` | the header art (the same rows as `src/tui/header.rs`) |
| `site/lib/main.server.dart` | the entry point: the document and its stylesheet |
| `site/web/styles.css` | the theme colours as CSS variables, and the layout |
| `site/tool/check.sh` | checks a build: one page, the banner, the links, the theme |

## Building

It needs the Dart SDK (3.8 or later; Flutter's bundled `dart` works) and the Jaspr CLI; `site/pubspec.lock` pins the packages; neither is needed for Muninn
itself, and the Rust build and the release workflow never run them.

```sh
cd site
dart pub get
dart pub global activate jaspr_cli 0.23.5
jaspr serve          # http://localhost:8080, reloads on change
jaspr build          # static output in site/build/jaspr
tool/check.sh
```

## Screenshots

`scripts/screenshots.sh` redoes every screenshot: it runs the ignored test
`tui::screenshots::feature_screenshots` (a scratch home with sample ETH/BTC data, never
`~/.muninn`), which drives the TUI through each feature, some mid-action, and renders each
screen to HTML; headless Chromium turns those into PNGs and ImageMagick shrinks them into
`site/web/screenshots/`. It then rewrites `site/lib/shots.dart` and the README's screenshot
block. `HERO=name` picks the hero (default `pipeline-live`). It needs `chromium` and
`magick`. Run it when a view changes, then rebuild the site.

## Checks

CI (`.github/workflows/ci.yml`, job `site`) installs Dart in the runner, checks the
formatting, builds the site, analyzes it, runs `tool/check.sh` and keeps the built page
as the `site` artifact. `cargo test` (`tests/site.rs`) keeps the page honest without
Dart: the header art matches the TUI's, the colours match `assets/themes/tokyo-night.toml`,
every docs page it links exists, the hero and the Screenshots section and header link
are there, and every screenshot the site or the README shows exists. `tool/check.sh` checks the
same in the built page.

When the header art or the default theme changes, change the site with it.
