# Website

The Muninn website is a static front page, built with [Jaspr](https://jaspr.site) (Dart) in
`site/`, plus these docs as pages under `/docs/`. It uses the app's default theme (Tokyo Night: its colours, dark and light) and
the MUNINN header art, shows matched screenshot pairs (the same view, data and theme in the Flutter app and the
terminal UI, labelled) under the header and in a Screenshots section, and links the releases, the docs and the install steps.

Every `docs/**/*.md` except `docs/historical` is published at `/docs/<slug>/` (`docs/README.md`
is `/docs/`, `services/redis.md` is `/docs/services/redis/`), with a sidebar grouped like
`docs/README.md` and an "Edit this page on GitHub" link. Links between docs point at those
pages, screenshots at `/screenshots/`, and anything else in the repository at GitHub.
The pages are generated from `docs/` on every build, so they never drift.

| file | what |
|------|------|
| `site/lib/app.dart` | the page: header art, hero shot, features, screenshots, install steps, docs links |
| `site/lib/pairs.dart` | the matched Flutter app / terminal UI screenshot pairs shown on the page (the first is the hero) |
| `site/lib/shots.dart` | the TUI feature screenshot list written by `scripts/screenshots.sh` (README only; not on the page) |
| `site/web/screenshots/` | the TUI feature screenshots used by the README and docs (PNG, 1464x890) |
| `site/web/screenshots/pairs/` | the matched pairs (`app-*.png`, `tui-*.png`, 1464x890), captured from the running apps |
| `site/lib/main.server.dart` | the entry point: the document and its stylesheet |
| `site/web/styles.css` | the theme colours as CSS variables (including the logo's `--muninn-header`, `--muninn-accent`, `--muninn-bg`), and the layout |
| `site/web/logo/` | the Muninn raven (by Matematik): `muninn-raven.svg` (tile), `-mark` (bare raven), `-small` (16–32 px); used exactly as delivered |
| `site/lib/logo.dart` | verbatim copies of the tile and mark, inlined in the hero and the docs header so they follow the theme |
| `site/web/favicon.svg`, `favicon-16.png`, `favicon-32.png`, `apple-touch-icon.png` | favicons: the small mark (SVG and its 16/32 px renders) and the tile at 180 px |
| `site/tool/docs.dart` | publishes `docs/` as pages in `build/jaspr/docs/` (the `markdown` package, run after `jaspr build`) |
| `site/tool/check.sh` | checks a build: the front page (raven on top, no block-letter title), the links, the theme, a page per doc and every link in them |

## Building

It needs the Dart SDK (3.8 or later; Flutter's bundled `dart` works) and the Jaspr CLI; `site/pubspec.lock` pins the packages; neither is needed for Muninn
itself, and the Rust build and the release workflow never run them.

```sh
cd site
dart pub get
dart pub global activate jaspr_cli 0.23.5
jaspr serve          # http://localhost:8080, reloads on change
jaspr build          # static output in site/build/jaspr
dart run tool/docs.dart   # the docs pages, into build/jaspr/docs
tool/check.sh
```

## Screenshots

`scripts/screenshots.sh` redoes every screenshot: it runs the ignored test
`tui::screenshots::feature_screenshots` (a scratch home with sample ETH/BTC data, never
`~/.muninn`), which drives the TUI through each feature, some mid-action, and renders each
screen to HTML; headless Chromium turns those into PNGs and ImageMagick shrinks them into
`site/web/screenshots/`. It then rewrites `site/lib/shots.dart` and the README's screenshot
block. It needs `chromium` and `magick`. Run it when a view changes.

The front page shows only `site/web/screenshots/pairs/`: each view captured from the
running Flutter app and the running TUI on the same data, theme (Tokyo Night, dark) and
size, with any data-dir path in the header covered. Retake both halves of a pair together.

## Checks

CI (`.github/workflows/ci.yml`, job `site`) installs Dart in the runner, checks the
formatting, builds the site and its docs pages, analyzes it, runs `tool/check.sh` and keeps the built page
as the `site` artifact. `cargo test` (`tests/site.rs`) keeps the page honest without
Dart: the header art matches the TUI's, the colours match the built-in `tokyo-night` theme of [weirdnet-themes](https://github.com/WeirdBrains/weirdnet-themes),
every docs page it links exists, every doc has a title and its links to other docs resolve, the hero and the Screenshots section and header link
are there, and every screenshot the site or the README shows exists. `tool/check.sh` checks the
same in the built page.

When the header art or the default theme changes, change the site with it.

To change the logo, replace the files in `site/web/logo/` with the new artwork, copy the
tile and mark into `site/lib/logo.dart` (the test checks they match byte for byte), copy
the small mark to `site/web/favicon.svg`, its 16/32 px renders to `favicon-16.png` and
`favicon-32.png`, render `apple-touch-icon.png` with
`rsvg-convert -w 180 -h 180 site/web/logo/muninn-raven.svg`, and drop the tile into
`assets/icons/muninn.svg` for the installer icons (`assets/icons/README.md`).

## Deploying

The site is served by GitHub Pages from the `gh-pages` branch at
<https://muninn.weirdbrains.com> (a DNS-only CNAME to `weirdbrains.github.io`; GitHub
issues the Let's Encrypt certificate). The branch holds only the built page, a `CNAME`
file and `.nojekyll`. To redeploy by hand after a change:

```sh
cd site && jaspr build && dart run tool/docs.dart && tool/check.sh && cd ..
git worktree add /tmp/muninn-pages gh-pages
rsync -a --delete --exclude .git --exclude CNAME --exclude .nojekyll site/build/jaspr/ /tmp/muninn-pages/
git -C /tmp/muninn-pages add -A
git -C /tmp/muninn-pages commit -m "site: build of main $(git rev-parse --short HEAD)"
git -C /tmp/muninn-pages push origin gh-pages
git worktree remove /tmp/muninn-pages
```

Pages rebuilds within a minute or two (`gh api repos/WeirdBrains/muninn/pages/builds/latest`).
