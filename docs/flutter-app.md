# Flutter app (desktop and mobile)

Muninn's graphical app, in `app/`. It runs the same Rust code as `mnnn` (through
`crates/muninn-ffi`), uses the same themes and follows the TUI's menus and views. On
phones it switches to a phone layout. Status: early: theme switcher, header with the raven,
the TUI menu bar, a read-only Home, the Data view with charts and the time range,
Projects (create, rename, describe, delete, attach and detach series, journals, live searches, reports, import/export archives), Net status, peers, start/stop, connect, invites, relays and sharing, Orgs list and writes, Keys status and writes, File ▸ Import… / Export…
(CSV, Parquet, JSON), and File ▸ Preferences….

## Run it

```
cd app
flutter pub get
flutter run -d linux                          # desktop, uses MUNINN_HOME or ~/.muninn
MUNINN_HOME=/tmp/mnnn-demo flutter run -d linux   # a scratch data dir
```

The first build compiles `crates/muninn-ffi` with cargo through Dart **Native Assets**:
`app/hook/build.dart` (flutter_rust_bridge_hooks) builds the crate for the target
platform and Flutter bundles the library, on every platform, with no cargokit and no
CocoaPods (iOS/macOS use Swift Package Manager). The Rust version and the targets come
from `crates/muninn-ffi/rust-toolchain.toml` (pinned; rustup installs it on first use;
it applies only inside that directory, so the workspace keeps its own toolchain). The
build output lives in `app/.dart_tool/hooks_runner/` (delete it to reclaim space).
Users never need Rust: release builds ship the compiled library.

## What it does

- **Header:** the raven (themed) and the big MUNINN wordmark, as on the website.
- **Desktop menus:** File, View, Data, Projects, Net, Orgs, Keys, Theme, Help, like the TUI.
  Items the app doesn't have yet open a dialog naming the `mnnn` command or TUI menu.
  File ▸ Import…, Export…, Preferences… and Quit are wired, as is Projects ▸ New project… /
  Live searches… / Journals… / Reports… / Import archive… and Net ▸ Status / Start / Stop / Invite or connect / Peers / Relays / Shared, Orgs ▸ Organizations… / New organization… / Join… / Apply… / Start site / Stop site and Keys ▸ Status… plus Initialize, Rotate,
  Rekey, Export, Recover and Re-issue shards. Help ▸ Topics / Search / Keybindings open the
  same embedded docs as `mnnn help` and the TUI.
  Keyboard shortcuts match the [TUI](keybindings.md) for every action the app has
  (see [Keybindings](#keybindings) below).
- **Phone layout** (narrower than 720 px): bottom navigation (Home, Data, Projects, Net,
  More), 48 px+ targets, safe areas, theme as a bottom sheet; nothing is hover-only.
- **Themes:** every theme from `assets/themes/*.toml` plus your own in `<home>/themes/`,
  light, dark or system (Theme menu, or the palette button on phones). The app starts on
  the theme in `preferences.toml` and saves your choice there (`[theme]`, the same as
  `mnnn theme set` / `mnnn theme mode`), so the TUI and the app follow each other.
- **Unlock:** Home, Data, Projects, Net and Orgs ask for the Muninn password once per session. The
  key stays in Rust memory (never in Dart, never on disk); **File ▸ Lock** forgets it.
  Dashboard reads open the store **read-only** (no migrations). Net and Orgs overviews are read-only; Net writes and Orgs writes (create, edit, join, members, roles, resources, hosting) open the store for writes after the same unlock.
  Keys status does not unlock the store (same as `mnnn keys status`).
  **File ▸ Import…**, **Export…** and project writes (create, rename, describe, delete,
  attach or detach a series, journals, live searches, reports, import/export archives)
  open it for writes after the same unlock.
- **Home:** where the data lives, and the projects and datasets in it.
- **Data:** the datasets, like the TUI [sidebar](sidebar.md), scoped by the time range;
  pick one to see it as a chart (candles when they fit, else a close line, volume
  underneath) or a list of bars, newest first. Switch with the Chart / List buttons or
  **View ▸ Change view** (`Ctrl-w t`, the same chord as the TUI). Rust downsamples to at most 600
  buckets (open = first, close = last, high/low = extremes, volume = sum), so long
  ranges stay fast. Desktop shows the list and the chart side by side; on a phone the
  chart opens on its own page. Pinch, a trackpad or the mouse wheel zooms the chart
  *inside* the selected time range (the picker and chips stay as they are; zoom does not
  re-query). Drag pans when zoomed; double-tap or **Reset zoom** shows the whole loaded
  window again. Tighten the time range if you need more detail than the 600 buckets.
- **Time range:** as in the TUI it applies to every view. The chips above every section
  show it (`from …` / `to …`, × removes an end); click **Time range**, press `T`, or use
  **Data ▸ Filters ▸ Time range…** to open the picker: a date and a time (UTC) for
  *from* and *to*, either end left open. **Clear** (or **Data ▸ Filters ▸ Clear time
  range**, or `X`) shows everything. Datasets with no bars in the range are hidden. The range is
  saved as the TUI's sidebar filter (`[tui.sidebar.filters]` from/to), so it is shared.
- **Saved state:** the theme, the time range and the dataset open in Data
  (`[app] dataset`) are restored on launch. Locking keeps the dataset; it shows again
  after the next unlock. Reading never creates `preferences.toml`; the first change does.
- **Chart readout:** hover over the chart (or touch / drag on a phone) to show that bar's
  time, open, high, low, close and volume above it; a guide line marks the bar.
- **Chart layers:** **View ▸ Chart layers…** (or the layers button next to Chart / List,
  or `L`) toggles the same kind of layers as the TUI: candles (`c`), OHLC bars (`b`), a
  close line (`n`), area under the line (`a`), volume (`v`), grid (`#`), crosshair (`x`)
  and the price (`y`) / time (`t`) axes. Defaults match the TUI (candles, volume, grid,
  crosshair and both axes on). Moving averages, wick and gap markers and the high/low
  envelope stay in the TUI (`e`, `m`, `w`, `g`, `z` are unbound here).
- **Projects:** pages for Projects, Journals, Searches and Reports, like the TUI
  Projects window. Pick a project to see the series and other items saved in it.
  **New project** (or Projects ▸ New project…) creates one (`mnnn project create`);
  the project menu renames, edits the description or deletes it (`mnnn project edit` /
  `remove` — journals stay as standalone journals), exports an archive (`.mnnp` or
  JSON; `mnnn project export`), starts a journal in the project, or adds an existing
  journal, search, report, rule, model or code space (`mnnn project add`). **Add
  current dataset** attaches the dataset open in Data, with the current time range
  (`a` in the TUI / `mnnn project add`); remove takes a series or other item out of
  the project without deleting it (`mnnn project drop`). Opening a series shows it
  in Data with the project's saved time range. **Import archive**
  (Projects ▸ Import archive…) reads a `.mnnp` or JSON pack (`mnnn project import`);
  Dart only passes the path, and archive bytes stay in Rust. **Journals** list,
  create (optionally in a project), rename, change format, fork and delete journals,
  and write, edit or remove entries plus journal-level notes (`mnnn journal`).
  Word-span highlights, images, copying entries and document export stay in the
  TUI. **Searches** save a live search (query and target: series, journals or
  records), rename, fork or delete it (`mnnn search save|edit|fork|remove`).
  Refining a search with the full sidebar filters, overwriting one or applying it
  to Data stays in the TUI. **Reports** create (optionally attached to a project),
  rename, describe and delete reports, and add, edit, move or remove text, authors,
  journal, project, series, code and model sections (`mnnn report`). Records
  sections and PDF/preview export stay in the TUI. Reviews, Notifications, Rules,
  Code spaces, Pipes and Pipelines stay in the TUI.
- **Net:** status and peers like `mnnn net status` / `mnnn net peers`, plus the write
  flows the TUI and CLI provide for the node itself: start and stop (the node runs in
  this process, like the TUI; `mnnn net stop` can still ask it to stop), connect
  (address, peer id, name or invite link; an optional hello-only pass), invites
  (create a public link that holds no secret, list yours, accept or decline requests,
  withdraw an invite), disconnect and forget, relay add/remove/serve (saved in
  `preferences.toml`, picked up the next time the node starts, like the CLI), and
  share / unshare (model, search, journal, project, report, code or pipeline). The
  node identity and exchange keys stay in Rust. On a phone the node uses the same
  platform-secure app sandbox the store already uses. Network settings, manual key
  exchanges, listing or fetching what a peer shares, network search, broadcast and
  dark shards stay in the TUI or CLI.
- **Orgs:** list and detail like `mnnn org list` / `mnnn org show` (and the TUI's
  Organizations and Joined pages), plus the write flows those surfaces already have:
  create (`mnnn org create` / New organization), edit (name, slug, description,
  visibility, access, hosted, member sign-in, gated, template, apply/invite defaults),
  remove, join with an invitation token or link, apply to an org on another node, sync
  and leave, member invite / approve / reject / role / kick, role create / edit
  permissions / remove, resource add / publish cycle / archive-restore / release, and
  hosting start / stop / autostart (`mnnn org host`). Start site serves hosted orgs
  in-process, the same way the TUI does; if another process is serving, Stop site asks
  it to stop. When hosting autostart is on, starting the node also starts the site
  (TUI / `mnnn net start`). Invite tokens are shown so the other node can join, matching
  the TUI; host certificates and private keys stay in Rust. On a phone the site and
  store use the same platform-secure app sandbox. Templates (HTML/CSS, assets, build),
  domains, certificates, ACME, browse, fetch/give/mirror, endpoints, org projects and
  code serve stay in the TUI or CLI.
- **Keys:** status like `mnnn keys status`, plus the write flows the CLI provides
  (Initialize, Rotate, Rekey, Export, Recover from words or shards, Re-issue shards).
  It uses the same data dir as the store (on a phone, the platform-secure app sandbox;
  on desktop, the TUI home). Status shows whether keys are initialized, the database id,
  the database path, the current service-key version, and each on-disk service key as
  active or retired. The master key, recovery words, shards, passphrases, wrapped keys
  and raw key bytes stay in Rust. After init, recover `--print-words`, or re-issue
  shards, a one-time sheet fetches **one line at a time** from a Rust-held session and
  dismisses (wipes) that session when you close it. Rekey locks the app session first
  (the TUI refuses to rekey while open). Export writes age-encrypted files to a directory
  you pick and returns only those paths. Typed recovery words in a rotate/recover form
  are the same pattern as the TUI Keys form (obscured fields, not stored).
- **Import** (File ▸ Import…): pick a file or folder (CSV, TSV, TXT, JSON, NDJSON, JSONL)
  and store each file as it is in a source, like `mnnn ingest path`. An optional source
  name overrides the folder's name (else `local`). Progress is byte-based; Cancel keeps
  finished files and rolls back the file in progress. Stored files are not series —
  turn them into bars with a [pipe or pipeline](pipes.md).
- **Export** (File ▸ Export…): write stored bars as CSV, Parquet or JSON, like
  `mnnn export`. The selected dataset and the time range pre-fill the filters; leave
  them empty to export everything. No matching bars writes an empty file (CSV header or `[]`).
- **Preferences** (File ▸ Preferences…): the theme, light/dark/system mode, time range
  and the dataset open in Data — the same values already saved in `preferences.toml`
  (`[theme]`, `[tui.sidebar.filters]` from/to, `[app] dataset`) and shared with the TUI.
  Changing them here uses the same save path as the Theme menu, the time range picker
  and Data. TUI-only layout (header size, sidebar, history bar) stays in `mnnn tui`.
- **Help** (Help ▸ Topics… / Search help… / Keybindings, or `?` / F1): the same
  embedded `docs/` topics as `mnnn help` and the TUI. `?` and F1 open the topic
  for the current section (Data chart → [charts](charts.md), Keys → [keys](keys.md),
  Home → [this page](flutter-app.md), and so on). Search matches titles and bodies;
  internal links follow other topics; Help ▸ Keybindings opens [keybindings](keybindings.md).
  The Flutter-specific shortcut notes on this page are part of that same help tree.
- **About** (Help ▸ About Muninn): the same text as the TUI's About box, from
  `muninn_core::about`.

## Keybindings

The app uses the same keys as the [TUI](keybindings.md) for every action it already
has. It does not invent different chords. Shortcuts are ignored while a text field
is focused (you can type `T` or `i` into a password or path). On a phone they apply
when a hardware keyboard is attached.

### Everywhere

| key | action |
|-----|--------|
| `T` | time range picker |
| `X` | clear the time range (the TUI's "clear all" filters; the app's filter is the time range) |
| F5 | refresh |
| `?`, F1 | Help for the current section (the embedded docs) |
| `i` | File ▸ Import… |
| `P` | Projects |
| `K` | Net status |
| `W` | Net peers (the Peers page of the Net view) |
| `G` | Organizations |
| `L` | Chart layers… |
| `Ctrl-w t` | change view (chart ↔ list) |
| F10 | open the File menu (stand-in for the TUI's "open the menu bar") |
| `q` | quit (desktop only) |
| Alt+letter | open that menu (File, View, Data, Projects, Net, Orgs, Keys, Theme, Help), matching each TUI menu mnemonic |

### Data

| key | action |
|-----|--------|
| `,` / `.` | previous / next dataset (the TUI's previous / next series) |
| `t` | chart ↔ list when the list is showing (data tile `t`) |

When the Data chart is showing:

| key | action |
|-----|--------|
| `c` `b` `n` `a` `v` `#` `x` `y` `t` | toggle candles, OHLC bars, line, area, volume, grid, crosshair, price axis, time axis |
| `+` / `-` | zoom in / out around the centre (inside the loaded bars, same as pinch) |
| `h` `l`, PgUp PgDn | pan a quarter / a full screen |
| Home / End, `0` | first bars / latest |

### Projects

| key | action |
|-----|--------|
| `n` | new project (when the Projects page is showing) |

### Platform exceptions

- **Ctrl-C** quits the TUI; in the app it stays the platform Copy shortcut.
- **`q`** quits the desktop app (File ▸ Quit); on a phone the system back gesture leaves the app and `q` is ignored.
- **F10** opens the File menu. Flutter's `MenuBar` has no TUI-style "menu bar focused" mode.
- **Alt+letter** uses Flutter's `MenuAcceleratorLabel` (the same letters as the TUI, including Net `N` and Orgs `O`). On macOS the system menu bar may take precedence.
- **Tiling** (`Ctrl-w` then `v`/`s`/`q`/`h`/`j`/`k`/`l`/… other than `t`): the app is not a tiled workspace, so those keys are unbound. `Ctrl-w t` still changes view.
- **Ctrl-b** (toggle sidebar): the desktop Data list is always shown; phones use bottom navigation.
- **Tab** / **Alt-1…9** tile focus: there are no tiles.
- Chart layer keys **`e` `m` `w` `g` `z`** (envelope, moving averages, wick, gap, zero-volume) stay in the TUI; those layers are not drawn yet.
- **← →** move the TUI chart crosshair; the app moves it by hover or touch.
- **`[` `]` `{` `}` `@`** period start/end on a chart tile stay in the TUI (the app uses the global `T` picker).
- Keys for windows the app does not have yet (`F` live searches, `J` journals, `E` pipelines, `A` authors, `Y` history, and so on) are left unbound — they are not remapped to something else.
- In the TUI, `X` stops a node when the network window is focused; the app has no start/stop and `X` stays "clear the time range".
- **`a`** toggles the area layer when the Data chart is showing (chart tile). Adding a series to a project is **Add current dataset** on the Projects page (the TUI's sidebar-focused `a`).
- **Esc** closes dialogs through Flutter's usual dialog behaviour.

## Data

Desktop shares the TUI's data dir: `MUNINN_HOME`, else `~/.muninn`. On a phone the
database, keys (the envelope and service keys), preferences and themes live in the
platform-secure app sandbox — the same data dir the store already uses — not a
desktop-style `~/.muninn`. Sync with a desktop node over the existing network
comes in a later phase (see [sync](sync.md)).

## How it fits together

- `crates/muninn-core`: all of Muninn (the library `mnnn` is built on).
- `crates/muninn-ffi`: the small API the app calls (version, about, data dir, themes,
  projects and datasets, downsampled bars, ingest and bar export, net status and peers, org list and writes, keys status and writes, help topics), exposed with
  flutter_rust_bridge 2.
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

## Icons

The launcher icons come from the approved raven tile, `app/assets/logo/muninn-raven.svg`
(the same file as the installers' `assets/icons/muninn.svg`, which stays the packaging
source; a test checks they match). `app/assets/icon/` holds the renders:
`muninn-1024.png` (the tile), `muninn-mark-1024.png` (the raven alone, Android's adaptive
foreground on `#1a1b26`) and `muninn-256.png` (the Linux window icon, loaded from the
bundle by `linux/runner/my_application.cc`). `flutter_launcher_icons` (config in
`app/pubspec.yaml`) writes Android mipmaps and adaptive icon, the iOS and macOS
AppIcon sets (iOS without alpha) and `windows/runner/resources/app_icon.ico`.
After a logo change:

```
cd app
rsvg-convert -w 1024 -h 1024 assets/logo/muninn-raven.svg -o assets/icon/muninn-1024.png
rsvg-convert -w 1024 -h 1024 assets/logo/muninn-raven-mark.svg -o assets/icon/muninn-mark-1024.png
rsvg-convert -w 256 -h 256 assets/logo/muninn-raven.svg -o assets/icon/muninn-256.png
dart run flutter_launcher_icons
git checkout ios/Runner.xcodeproj/project.pbxproj   # the tool rewrites a setting it shouldn't
```

## Apple and Android builds (status)

- **Swift Package Manager:** the Rust library is a native asset, so there is no plugin
  needing a `Package.swift` or podspec; the old cargokit `rust_builder/` is gone.
- **Disk:** a debug macOS build of `muninn_ffi` needs about 10 GB of Rust build output;
  free at least 25 GB before building macOS, iOS and Android on one Mac.
- **Flutter version:** the app needs Flutter 3.47.9 (Dart 3.13.7) or newer.
- **Android SDK on Linux:** install the command-line tools, then
  `sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0" "ndk;28.2.13676358"`,
  `flutter config --android-sdk ~/Android/Sdk`, `flutter doctor --android-licenses`, and
  `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.

## Tests

```
cd app && flutter analyze && flutter test
cargo test -p muninn_ffi
```

CI (`.github/workflows/ci.yml`, job `flutter`) runs `flutter analyze` and `flutter test` in
`app/` on every push to main and every pull request, with Flutter 3.47.9; the tests use a
fake API; the Native Assets hook builds the Rust library first, so the job installs the pinned toolchain.

## Failure modes

- *Wrong key or not a muninn database*: the password is wrong.
- *No store yet*: **Keys ▸ Initialize…** (or `mnnn keys init` in a scratch `MUNINN_HOME`).
- Building needs rustup the first time; a missing toolchain or target stops
  `flutter run` with "Building native assets failed" (details in
  `app/.dart_tool/hooks_runner/muninn/*/stderr.txt`).
