# Flutter app (desktop and mobile)

Muninn's graphical app, in `app/`. It runs the same Rust code as `mnnn` (through
`crates/muninn-ffi`), uses the same themes and follows the TUI's menus and views. On
phones it switches to a phone layout. Status: early: theme switcher, header with the raven,
the TUI menu bar, a read-only Home, the Data view with charts and the time range, and
read-only Projects.

## Run it

```
cd app
flutter pub get
flutter run -d linux                          # desktop, uses MUNINN_HOME or ~/.muninn
MUNINN_HOME=/tmp/mnnn-demo flutter run -d linux   # a scratch data dir
```

The first build compiles `crates/muninn-ffi` with cargo (cargokit, in
`app/rust_builder/`). Users never need Rust: release builds ship the compiled library.

## What it does

- **Header:** the raven (themed) and the big MUNINN wordmark, as on the website.
- **Desktop menus:** File, View, Data, Projects, Net, Orgs, Keys, Theme, Help, like the TUI.
  Items the app doesn't have yet open a dialog naming the `mnnn` command or TUI menu.
- **Phone layout** (narrower than 720 px): bottom navigation (Home, Data, Projects, Net,
  More), 48 px+ targets, safe areas, theme as a bottom sheet; nothing is hover-only.
- **Themes:** every theme from `assets/themes/*.toml` plus your own in `<home>/themes/`,
  light, dark or system (Theme menu, or the palette button on phones). The app starts on
  the theme in `preferences.toml` and saves your choice there (`[theme]`, the same as
  `mnnn theme set` / `mnnn theme mode`), so the TUI and the app follow each other.
- **Unlock:** Home, Data and Projects ask for the Muninn password once per session. The
  key stays in Rust memory (never in Dart, never on disk); **File ▸ Lock** forgets it.
  The store is opened **read-only** (no migrations, no writes).
- **Home:** where the data lives, and the projects and datasets in it.
- **Data:** the datasets, like the TUI [sidebar](sidebar.md), scoped by the time range;
  pick one to see it as a chart (candles when they fit, else a close line, volume
  underneath) or a list of bars, newest first. Switch with the Chart / List buttons or
  **View ▸ Change view** (`Ctrl-w t` in the TUI). Rust downsamples to at most 600
  buckets (open = first, close = last, high/low = extremes, volume = sum), so long
  ranges stay fast. Desktop shows the list and the chart side by side; on a phone the
  chart opens on its own page.
- **Time range:** as in the TUI it applies to every view. The chips above every section
  show it (`from …` / `to …`, × removes an end); click **Time range**, press `T`, or use
  **Data ▸ Filters ▸ Time range…** to open the picker: a date and a time (UTC) for
  *from* and *to*, either end left open. **Clear** (or **Data ▸ Filters ▸ Clear time
  range**) shows everything. Datasets with no bars in the range are hidden. The range is
  saved as the TUI's sidebar filter (`[tui.sidebar.filters]` from/to), so it is shared.
- **Saved state:** the theme, the time range and the dataset open in Data
  (`[app] dataset`) are restored on launch. Locking keeps the dataset; it shows again
  after the next unlock. Reading never creates `preferences.toml`; the first change does.
- **Chart readout:** hover over the chart (or touch / drag on a phone) to show that bar's
  time, open, high, low, close and volume above it; a guide line marks the bar.
- **Projects:** pick a project to see the series saved in it (`mnnn project add`);
  opening one shows it in Data with the project's saved time range. Read-only for now.
- **About** (Help ▸ About Muninn): the same text as the TUI's About box, from
  `muninn_core::about`.

## Data

Desktop shares the TUI's data dir: `MUNINN_HOME`, else `~/.muninn`. Mobile keeps its
own data in the app sandbox and will sync with a desktop node over the existing
network and sync (see [sync](sync.md)); that comes in a later phase.

## How it fits together

- `crates/muninn-core`: all of Muninn (the library `mnnn` is built on).
- `crates/muninn-ffi`: the small API the app calls (version, about, data dir, themes,
  projects and datasets, downsampled bars), exposed with flutter_rust_bridge 2.
- `app/lib/src/rust/`: generated Dart bindings. Regenerate after changing the API:
  `cd app && flutter_rust_bridge_codegen generate`.
- State is Riverpod. Themes are resolved to hex roles in Rust and mapped to Flutter's
  `ColorScheme` plus a `MuninnColors` theme extension with every TUI role.
- The raven SVGs (`app/assets/logo/`) use `var(--muninn-header|accent|bg)`; the app
  substitutes the theme's colours and draws them with flutter_svg.
- Charts get data already downsampled in Rust (`query_bars`, at most `max_points`
  buckets).

See [architecture](architecture.md) for the crate layout.

## Identity

- Publisher **WeirdBrains**; homepage and support: https://muninn.weirdbrains.com
- Bundle / application id: `com.weirdbrains.muninn` (Android, iOS, macOS); Linux
  application id `com.weirdbrains.muninn`; Windows company name WeirdBrains.
- Apple builds use Swift Package Manager (`flutter config --enable-swift-package-manager`),
  not CocoaPods.

## Distribution (planned)

- Desktop: built-in installers (Windows setup, macOS .dmg), winget, Microsoft Store
  (MSIX, publisher WeirdBrains), Homebrew, AUR, .deb and .rpm, for amd64 and arm64.
- Mobile: TestFlight and Google Play testing tracks first, then the App Store and Play
  Store. Builds are also attached to GitHub releases (APKs, TestFlight links).

## Tests

```
cd app && flutter analyze && flutter test
cargo test -p muninn_ffi
```

## Failure modes

- *Wrong key or not a muninn database*: the password is wrong.
- *No store yet*: run `mnnn keys init` first (in a scratch `MUNINN_HOME` to try it out).
- Building needs cargo the first time; a missing toolchain stops `flutter run` with a
  cargokit error.
