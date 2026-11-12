# Flutter app (desktop and mobile)

Muninn's graphical app, in `app/`. It runs the same Rust code as `mnnn` (through
`crates/muninn-ffi`), uses the same themes and follows the TUI's menus and views. On
phones it switches to a phone layout. Status: early: theme switcher, header with the raven,
the TUI menu bar, Home (tap a project to open it, or a dataset to open it in Data), the Data view with charts and the time range,
Projects (create, rename, describe, delete, attach and detach series, journals, live searches, reports, code spaces, live pipes, pipelines, rules, reviews, notifications, import/export archives), Net status, peers, start/stop, connect, invites, relays, sharing, settings, connections, key exchanges, search, broadcast, dark shards, MCP, and stored peer messages and live sessions, Orgs list and writes, Keys status and writes, File ▸ Import… / Export…
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
  Preferences sets the size to auto, compact or large (`[tui] header`, shared with the TUI).
- **Desktop menus:** File, View, Data, Projects, Code, Plug, Net, Orgs, Keys, Theme, Help, like the TUI.
  Items the app doesn't have yet open a dialog naming the `mnnn` command or TUI menu.
  File ▸ Import…, Export…, Backup ▸ Create… (Restore stays `mnnn backup restore`), Preferences… and Quit are wired, as is Projects ▸ New project… /
  Live searches… / Journals… / New journal… / Reports… / Code spaces… / Rules… / Rule sets… / Schedules… / Reviews… / Notifications… / Create then share…, Code ▸ Code… / Code spaces… / Migrate… / Live pipes… / Pipelines…, Plug ▸ Parsers / Transforms / Analysis / Ingest connectors / All plugins / Install plugin…, and Net ▸ Status / Start / Stop / Settings / Connections / Invite or connect / Offer token… / Accept token… / Nodes / Peers / Peers, messages & live / MCP / Relays / Shared / Network search / Broadcast / Dark shards, Orgs ▸ Organizations… / New organization… / Join… / Apply… / Browse… / Certificates… / Let's Encrypt… / Start site / Stop site / Announce site… / Announced sites…, Keys ▸ Status… plus Initialize, Rotate,
  Rekey, Export, Recover (words, shards, or a shard backup) and Re-issue shards, and Data ▸ Sources… / Symbols… / Filters / Sort / Coverage & gaps…. Theme ▸ Edit current…, New from current…, Import… and Export… edit both variants. Help ▸ Topics / Search / Keybindings open the
  same embedded docs as `mnnn help` and the TUI.
  Keyboard shortcuts match the [TUI](keybindings.md) for every action the app has
  (see [Keybindings](#keybindings) below).
- **Phone layout** (narrower than 720 px): bottom navigation (Home, Data, Projects, Net,
  More), 48 px+ targets, safe areas, theme as a bottom sheet; nothing is hover-only.
- **Themes:** every built-in theme from [weirdnet-themes](https://github.com/WeirdBrains/weirdnet-themes) (`themes/*.toml`) plus your own in `<home>/themes/`,
  light, dark or system (Theme menu, or the palette button on phones). The app starts on
  the theme in `preferences.toml` and saves your choice there (`[theme]`, the same as
  `mnnn theme set` / `mnnn theme mode`), so the TUI and the app follow each other.
- **Unlock:** Home, Data, Projects, Net and Orgs ask for the Muninn password once per session. The
  key stays in Rust memory (never in Dart, never on disk); **File ▸ Lock** forgets it.
  Dashboard reads open the store **read-only** (no migrations). Net and Orgs overviews are read-only; Net writes and Orgs writes (create, edit, join, members, roles, resources, hosting) open the store for writes after the same unlock.
  Keys status does not unlock the store (same as `mnnn keys status`).
  **File ▸ Import…**, **Export…** and project writes (create, rename, describe, delete,
  attach or detach a series, journals, live searches, reports, import/export archives)
  and Data ▸ Sources… / Symbols… writes (add, rename, remove, add symbol)
  open it for writes after the same unlock.
- **Home:** where the data lives, and the projects and datasets in it. Tapping a project opens it in Projects (selected, with its series). Tapping a dataset opens it in the Data workspace's first tile, like picking a series in the TUI sidebar.
- **Data:** the datasets, like the TUI [sidebar](sidebar.md), scoped by the time range.
  On a wide window the sidebar sits next to a tiling workspace of chart and list
  tiles, matching the TUI [tiling](tiling.md) model (binary tree, split / close /
  focus / resize / zoom / link / tabbed and stacked containers, and the 2-up,
  3-column, Main + stack and 2×2 presets). A session starts with one chart tile
  (the app's existing Data default; the TUI starts with a list tile) and the layout
  is written to `[tui.layout]` after unlock (shared with the TUI). Pick a dataset to see it as a
  chart (candles when they fit, else a close line, volume
  underneath) or a list of bars, newest first. **Data ▸ Sources…** lists ingest
  sources (`mnnn sources list`) with kind, locator and stored-item totals; you can
  add, rename or remove a source (remove asks the same confirmation as the CLI and
  deletes its bars and stored items) and inspect stored items (`mnnn sources data`),
  saving one to a file (`mnnn sources cat`) without the bytes entering Dart.
  **Data ▸ Symbols…** lists known pairs and aliases (`mnnn symbols list`) and can
  add a pair with optional base aliases (`mnnn symbols add`). The TUI Sources /
  Symbols windows are read-only lists; writes match the CLI. These are not the
  sidebar Source / Symbol *filters*. Switch with the Chart / List buttons or
  **View ▸ Change view** (`Ctrl-w t`, the same chord as the TUI). The other
  **View** tile actions and `Ctrl-w` chords match the TUI (see Keybindings). Rust downsamples to at most 600
  buckets (open = first, close = last, high/low = extremes, volume = sum), so long
  ranges stay fast. On a phone the dataset list is the page and the chart opens on
  its own route — there is no tiling. Pinch, a trackpad or the mouse wheel zooms the chart
  *inside* the selected time range (the picker and chips stay as they are; zoom does not
  re-query). Drag pans when zoomed; double-tap or **Reset zoom** shows the whole loaded
  window again. Tighten the time range if you need more detail than the 600 buckets.
- **Time range:** as in the TUI it applies to every view. The chips above every section
  show it (`from …` / `to …`, × removes an end); click **Time range**, press `T`, or use
  **Data ▸ Filters ▸ Time range…** to open the picker: a date and a time (UTC) for
  *from* and *to*, either end left open. The same Filters menu sets source, symbol,
  price, period and year, **Sort** sets year or name and asc/desc, and
  **Coverage & gaps…** lists missing bars. **Clear all** (or `X`) clears the time
  range and those filters. Datasets that do not match are hidden. The filters are
  saved as the TUI's sidebar filter (`[tui.sidebar]`), so they are shared.
- **Saved state:** the theme, the time range and the dataset open in Data
  (`[app] dataset`) are restored on launch. Locking keeps the dataset; it shows again
  after the next unlock. Reading never creates `preferences.toml`; the first change does.
- **Chart readout:** hover over the chart (or touch / drag on a phone) to show that bar's
  time, open, high, low, close and volume above it; a guide line marks the bar.
- **Chart layers:** **View ▸ Chart layers…** (or the layers button next to Chart / List,
  or `L`) toggles the same layers as the TUI: candles (`c`), OHLC bars (`b`), a line
  (`n`), area under the line (`a`), high/low envelope (`e`, 20-bar highest high / lowest
  low), volume (`v`), moving averages (`m`; default `sma20,sma50`, edit with `M` or the
  sheet), wick markers (`w`, including a ▼ on shooting stars), gap markers (`g`, on by
  default), zero-volume markers (`z`, a circle where volume is 0), grid (`#`), crosshair
  (`x`) and the price (`y`) / time (`t`) axes. Defaults match the TUI (candles, volume,
  gaps, grid, crosshair and both axes on). `N` cycles the line metric (close, typical,
  median, …) the same way a TUI chart tile does. Themes colour envelope, averages and
  markers from `context.muninn` (`chart_*`, `warning`).
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
  **Highlights** mark selected words of an entry (optional comment; TUI `m` /
  `mnnn journal highlight`); the entry view has a TUI-style word cursor so you
  can tap (and shift-tap) words to prefill the highlight. **Notes on words** attach a note to an entry or a
  span (`mnnn journal note --entry`). **Images** attach from a file path (Rust
  reads the bytes) and save an attached image to a path (`mnnn journal attach` /
  `image`); the entry view shows a downscaled PNG thumbnail, and Dart never sees
  the original bytes. **Copy** adds selected entries (with their
  notes, highlights and images) to another journal or a new one (`mnnn journal
  copy`). **Export** writes PDF, Word, HTML, LaTeX, text or JSON to a path
  (`mnnn journal export`); **Import journal** reads a JSON export (`mnnn journal
  import`); **Print** saves a PDF and can send it to `lp`/`lpr` (`mnnn journal
  print` / TUI `P`). File bytes stay in Rust. **Searches** save a live search (query, target and the full sidebar filters: sources, symbols, periods, years, time range, price, sort), refine or overwrite one (`mnnn search save|edit`, TUI `w`), apply a series search to Data (TUI Enter), and rename, fork or delete it (`mnnn search fork|remove`). Shift+F opens Live searches, matching the TUI's `F`. Applying a journals or records search opens a found-views list (TUI Enter on those targets); a series search still applies to Data. Tapping a found record opens the Models pane on that model and record (TUI Data ▸ Model designer… / `D`). The Models pane creates designs, adds, edits, drops and reorders fields as a draft, restores a dropped field with the same id (TUI `u`), imports and exports templates (`builtin:bars` or a `.template.json` file), creates, edits, deletes and bulk-updates records, previews and applies migrations (with confirmation when values would be lost), discards a draft, shows version history and rollback, and attaches, tests, dry-runs or runs model-owned rules on the last history action (`mnnn model` / `mnnn rule create --model`). **Reports** create (optionally attached to a project), rename, describe and delete reports, add, edit, move or remove text, records, authors, journal, project, series, code and model sections, preview a report as text (TUI `v` / `mnnn report preview`) and export PDF, Word, HTML, LaTeX, text, JSON or CSV to a path (TUI `E` / `mnnn report export`). File bytes stay in Rust.   **Code** (Code ▸ Code…) lists single code units: create
  (from a language template, another space or a built-in parser), edit source and
  input, approve, check, build, run and remove (`mnnn code` / TUI Code, `C`).
  **Code spaces** (Code ▸ Code spaces…, and the Spaces chip) are collections:
  a kind, a main and members (`mnnn spaces`). Open a space for its members
  (make main, pin or unpin, add or remove code), approve the line-up, enable
  or disable, run the main, or remove the space (its code stays). **Migrate…**
  (and the banner the first time code is not in a space) shows a dry run, then
  writes a backup and creates the spaces (`mnnn migrate`). Unclear kinds are
  skipped unless you set one. Later dismisses the banner. **Plug** lists the
  same spaces by kind, with Info… (`mnnn plugins info`) and enable or disable.
  **Install plugin…** takes a library path or an existing code name
  (`mnnn plugins install`); **Remove** drops the plugin unless a schedule
  still uses it.
  Built-in parsers can be listed and installed (`mnnn code parsers --install`).
  **Give to org…** (the TUI Code window `g`) assigns a space to an organization
  you own (`mnnn org resource add`); members pull it with Sync code. **Pipe result…**
  sends that code unit's last run into another code unit, a code space, a journal,
  a model, a project, an org, or the network (`>`). Writing the plugin SDK stays
  in the CLI. **Result tile…** (TUI `T`) opens a Data workspace tile (`code:SPACE`) with the space’s latest run as a table. **Pipes** show live
  and recent runs from history (the TUI Pipes window) and run a pipe
  (`mnnn pipe SOURCE STEP… [--to TARGET]`). Source `-` is standard input: paste
  the text or choose a file (Rust reads the bytes), with the CLI's `--format`
  (csv, json or ndjson), `--chunk` and `--keep-header`. **Pipelines** list, create
  and edit a graph (nodes `ID=KIND`, edges `FROM>TO`), run (including `mnnn
  pipeline run NAME -` with pasted or file stdin), approve a draft, remove, and
  export or import a pipeline file (`mnnn pipeline` / TUI Pipelines, `E`); file
  bytes stay in Rust. Tee… (TUI `t`) saves a selected node's output to
  `journal:NAME` or `model:NAME` as it flows on; End in model… (TUI `M`) ends a
  branch in a model table, making the model when it is new; Monitor… shows the
  same per-node status, rows and landed data as a TUI `monitor:PIPELINE` tile
  in the Data workspace (shared layout key `monitor:PIPELINE`). **Rules** list, create, edit, enable or disable, test,
  dry-run, tick due scheduled rules, list event kinds and remove (`mnnn rule` /
  TUI Rules, `R`). **Reviews** list yours and ones you joined, request a review,
  show the thread and the thing under review, comment, suggest, highlight, vote,
  invite (a public link; the exchange token stays in Rust), join, sync, accept or
  reject suggestions, merge, close, reopen, leave, drop a reviewer and remove
  (`mnnn review` / TUI Reviews, `V`). Sending an invite only works when the peer
  is reachable; otherwise the link is shown to copy. Syncing a joined review
  needs a running node. **Notifications** list (optionally unread only), open
  (marks read), mark read or unread, mark all read, send one, and mute or unmute
  built-in kinds (`mnnn notify` / TUI Notifications, `!`).
- **Net:** status and peers like `mnnn net status` / `mnnn net peers`, plus the write
  flows the TUI and CLI provide for the node itself: start and stop (the node runs in
  this process, like the TUI; `mnnn net stop` can still ask it to stop), connect
  (address, peer id, name or invite link; an optional hello-only pass), invites
  (create a public link that holds no secret, list yours, accept or decline requests,
  withdraw an invite), disconnect and forget, relay add/remove/serve (saved in
  `preferences.toml`, picked up the next time the node starts, like the CLI), and
  share / unshare (model, search, journal, project, report, code or pipeline),
  network settings (`mnnn net config` / TUI Settings: label, port, listen, mDNS,
  relay-for-others, autostart, host autostart, who to accept, bootstrap, plus the
  CLI extras for external addresses and collab transport — relays stay on the Relays
  page), connections (`mnnn connection` / TUI Connections: list, trust, remove; no
  user keys leave Rust), key exchanges (`mnnn net exchange` / TUI Nodes: list,
  verify, label, revoke, list what a peer shares and fetch it — import or write a
  path; bytes stay in Rust), network search (run a saved live search here and on
  peers, optionally `--only` a subset of exchanges), broadcast (`mnnn net broadcast`,
  including `org:SLUG`), export or restore a password-sealed exchange backup
  (`mnnn net exchange export|restore`; bytes and keys stay in Rust), dark shards (list entrusted and held,
  entrust from a key file, check that gathered shards restore this database's key,
  drop a held shard), MCP servers and clients (`mnnn mcp` / TUI Net ▸ MCP…: list
  servers and clients, show a server's tools, approve or withdraw a tool, change a
  client's role, revoke, restore or remove, and add a server or client — `mnnn mcp add`
  / `mnnn mcp client add`; a bearer token stays in Rust and a new client token is
  shown once), and Peers, messages & live (TUI `W`: list stored conversations and
  live sessions, mark messages read, send a message, host a live session and join
  one you were invited to). Sending, hosting and joining use the running node when
  this app started it, otherwise a short-lived node. The node identity, exchange
  offer/answer tokens, shard blobs and the master key stay in Rust. Short codes and
  fingerprints are meant to be read aloud, as in the TUI. On a phone the node uses
  the same platform-secure app sandbox the store already uses. **Offer token…** and
  **Accept token…** are `mnnn net exchange create|accept`; the raw token or answer
  is shown once when it was not delivered. Restoring a key from words, from shards
  you hold, or from a shard backup when this database cannot open is on the unlock
  card and on Keys (`mnnn keys recover` / `mnnn net shard restore`). Words are not
  printed.
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
  store use the same platform-secure app sandbox. **Templates** list, create (from
  the built-in look), edit HTML/CSS, add or remove assets (Rust reads the file),
  set a build step and run it, preview (a local host path, good for an hour) and
  remove (`mnnn org template` / TUI Templates). **Domains** add, list and remove
  (`mnnn org domain`). **Certificates** show public status (source, names, expiry,
  ACME) for the node and each org, set or clear your own certificate from PEM
  paths, turn ACME / Let's Encrypt on or off (an org needs a domain first; the
  node's directory, profile, challenge and port apply to orgs) and ask to renew
  (`mnnn org cert`). Renew needs the challenge reachable. Certificate PEM and
  private keys stay in Rust. **Browse** lists the public organizations of a peer
  (`mnnn org browse` / TUI Joined `b`). **Fetch, give and mirror** a joined org's
  resources or public site (`mnnn org fetch|give|mirror`); fetched binary bytes
  are written to a path in Rust. **Sync code** copies a joined org's code spaces
  here (`mnnn org sync-code`). **Endpoints** add, remove and call (`mnnn org endpoint`
  / `mnnn org call`). **Org projects** attach, detach and add a member
  (`mnnn org project`). **Code serve** lists spaces this node runs for the org and
  can start or stop one (`mnnn org code`). **Sign in** mints a one-time browser
  link to a joined org's site (`mnnn org sign-in`, valid five minutes). **Announce…**
  tells peers about a hosted site, or every hosted site, including a closed
  announcement when the site is not running (`mnnn org announce`). **Announced sites…**
  lists what peers have announced.
- **Keys:** status like `mnnn keys status`, plus the write flows the CLI provides
  (Initialize, Rotate, Rekey, Export, Recover from words or shards, restore from a
  shard backup, Re-issue shards). The same recover actions are on the unlock card,
  so they work when the database will not open.
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
  and Data. Header size stays in `mnnn tui`. Named layouts write `[tui.layouts]`
  (shared with the TUI); the live tile tree is written to `[tui.layout]` after unlock, like the TUI. The history bar (`Y` / `u` / `U`) stores
  `[tui.history_open]` the same way the TUI does.
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
| `X` | clear every sidebar filter, including the time range |
| F5 | refresh |
| `?`, F1 | Help for the current section (the embedded docs) |
| `i` | File ▸ Import… |
| `P` | Projects |
| `K` | Net status |
| `W` | Peers, messages & live (stored messages and sessions, plus send, host and join) |
| `G` | Organizations |
| `C` | Code spaces (Shift+C; lowercase `c` on the chart still toggles candles) |
| `E` | Pipelines (Shift+E; lowercase `e` on the chart still toggles the envelope) |
| `R` | Rules (Shift+R) |
| `V` | Reviews (Shift+V; lowercase `v` on the chart still toggles volume) |
| `F` | Live searches (Shift+F) |
| `J` | Journals (Shift+J, the TUI's `J`) |
| `!` | Notifications |
| `L` | Chart layers… |
| `A` | Authors… (Shift+A; lowercase `a` on the chart still toggles area) |
| `Y` | toggle the history bar (Shift+Y; lowercase `y` on the chart still toggles the price axis) |
| `u` / `U` | history step back / forward |
| `Ctrl-w t` | change view (chart ↔ list) of the focused tile |
| `Ctrl-w v` / `s` | split the focused tile right / below |
| `Ctrl-w q` / `c` | close the focused tile (the last tile cannot close) |
| `Ctrl-w h` `j` `k` `l` or arrows | focus the tile in that direction |
| `Ctrl-w Tab` / `w` | next tile |
| `Ctrl-w Shift-Tab` / `W` / `p` | previous tile |
| `Ctrl-w >` `<` | grow / shrink the tile horizontally |
| `Ctrl-w +` `-` | grow / shrink the tile vertically |
| `Ctrl-w =` | equalize split ratios |
| `Ctrl-w x` | swap with the next tile |
| `Ctrl-w z` | zoom (maximize) the focused tile |
| `Ctrl-w e` | toggle the direction used by View ▸ New tile |
| `Ctrl-w y` | link the focused tile to the next one, then unlink (title shows ⛓id) |
| `Ctrl-b` | toggle the Data sidebar (wide windows) |
| `/` | show the Data sidebar and focus it |
| Tab / Shift-Tab | next / previous tile (when the sidebar is not focused) |
| `Alt-1`…`Alt-9` | focus the tile with that id |
| Esc | unfocus the sidebar |
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
| `c` `b` `n` `a` `e` `v` `m` `w` `g` `z` `#` `x` `y` `t` | toggle candles, OHLC bars, line, area, envelope, volume, moving averages, wick markers, gap markers, zero-volume markers, grid, crosshair, price axis, time axis |
| `N` | cycle the line metric (open → high → low → close → typical → median → ohlc avg → vwap) |
| `M` | edit moving averages (`sma20, ema50`, period 2..1000) |
| `+` / `-` | zoom in / out around the centre (inside the loaded bars, same as pinch) |
| `h` `l`, PgUp PgDn | pan a quarter / a full screen |
| Home / End, `0` | first bars / latest |
| `[` / `]` | calendar pick for the visible window's start / end (among loaded bars) |
| `{` / `}` | type a date (`YYYY-MM-DD [HH:MM]`) for the visible window's start / end |
| `@` | jump the visible window to a typed date (a no-op when the chart is not zoomed) |

### Projects

| key | action |
|-----|--------|
| `n` | new project (when the Projects page is showing) |

### Platform exceptions

- **Ctrl-C** quits the TUI; in the app it stays the platform Copy shortcut.
- **`q`** quits the desktop app (File ▸ Quit); on a phone the system back gesture leaves the app and `q` is ignored.
- **F10** opens the File menu. Flutter's `MenuBar` has no TUI-style "menu bar focused" mode.
- **Alt+letter** uses Flutter's `MenuAcceleratorLabel` (the same letters as the TUI, including Net `N` and Orgs `O`). On macOS the system menu bar may take precedence.
- **Tiling** follows the TUI's i3-style binary tree on a wide window (width ≥ 720).
  `Ctrl-w` is a prefix (the same as the TUI): it is not registered as `Ctrl+V` /
  `Ctrl+S` / `Ctrl+Q`, so paste, save and quit stay with the platform. Minimum tile
  size is 200×96 pixels (a stand-in for the TUI's 20×6 cells). Chart, list, pipeline-monitor (`monitor:PIPELINE`) and data-frame tiles (`code:SPACE` from Code ▸ Result tile… / TUI `T`, and `view:PIPELINE/NODE` when a saved layout includes them).
  The TUI has no embedded PTY/terminal tile type — it is itself a terminal UI — so the app does not invent a terminal emulator.
  The live layout is written to `[tui.layout]` after unlock (shared with the TUI). A fresh session without a saved layout is one
  chart tile so existing Data behaviour stays; the TUI starts with a list tile.
  Chart layers and the visible window are stored on each tile (like the TUI). Linking records
  a pairing and shows ⛓id in the title; selecting a row in a linked list centres the paired
  chart, and panning a linked chart updates the list cursor.
  View ▸ Layouts has 2-up, 3-column, Main + stack and 2×2, plus Save current… / Saved…
  written to `[tui.layouts]` (shared with the TUI).
- **Ctrl-b** toggles the Data sidebar on a wide window. On a phone the dataset list
  *is* the page, so `Ctrl-b` is unused.
- **Tab** / **Shift-Tab** and **Alt-1…9** focus tiles on a wide window. `/` shows
  and focuses the sidebar (highlights it) but does not invent the TUI's type-ahead
  sidebar search. Esc or Tab leaves the sidebar. On a phone those keys do not
  manage tiles.
- **macOS:** the system menu bar may take Alt/Option letter accelerators, and
  Option+1…9 may produce special characters instead of focusing a tile. Use the
  View menu or Tab in that case.
- Chart layer keys **`e` `m` `w` `g` `z`** match the TUI (envelope, moving averages, wick, gap, zero-volume). The averages use the loaded bars (including those off-screen when zoomed) the same way the TUI uses warmup history; they do not fetch extra bars beyond `queryBars`.
- **← →** move the TUI chart crosshair; the app moves it by hover or touch.
- **`[` `]` `{` `}` `@`** set the chart's visible window among already-loaded bars (the same start/end/jump idea as a TUI chart tile). They do not change the global `T` time-range filter, and they do not invent a separate bucket or query. `{` `}` `@` type a date; `[` `]` open the same calendar+time pickers the time-range dialog uses. At scale 1 (every loaded bar visible) `@` has nothing to pan.
- **`A`** opens File ▸ Authors… (the TUI's authors window). Lowercase `a` on a chart still toggles the area layer.
- **`Y`** toggles the history bar; **`u`** / **`U`** step back / forward (the TUI's history keys). Lowercase `y` on a chart still toggles the price axis. Journals use Shift+J (the TUI's `J`). Live searches use Shift+F (the TUI's `F`); pipelines use Shift+E.
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
  projects and datasets, downsampled bars, ingest and bar export, sources and symbols, code spaces, live pipes and pipelines, rules, reviews and notifications, net status, peers, settings, connections, exchanges, search, broadcast and dark shards, org list and writes (including templates, domains, certificates/ACME, browse, fetch/give/mirror, endpoints, org projects, code serve and sign-in), journal extras (highlights, images, copy, export/import and print), keys status and writes, help topics), exposed with
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
  Windows MSIX/Store identity: `WeirdBrains.Muninn`. The TUI winget package is
  `WeirdBrains.muninn.tui` (not this id).
- Apple builds use Swift Package Manager (`flutter config --enable-swift-package-manager`),
  not CocoaPods.

## Distribution

Desktop packages of the Flutter app are built with `scripts/flutter-dist.sh`
(Linux, macOS, and Windows via Git Bash or `scripts/flutter-dist.ps1`).
The package name is `muninn-app` so it does not clash with the TUI/CLI `muninn`
packages (which install `/usr/bin/mnnn` or `muninn-*-windows-setup.exe`). File
bytes stay in the bundle; the script does not touch `~/.muninn` /
`%USERPROFILE%\.muninn` and does not publish a GitHub release.

```
DIST=/path/to/out CARGO_TARGET_DIR=/path/to/cargo-target scripts/flutter-dist.sh
```

On Linux amd64 that writes:

- `$DIST/muninn-app-<ver>-x86_64-unknown-linux-gnu.tar.gz` — relocatable bundle; run `./muninn`
- `$DIST/muninn-app_<ver>-1_amd64.deb` — installs to `/opt/muninn`, with a wrapper at `/usr/bin/muninn`
- `$DIST/muninn-app-<ver>-1.x86_64.rpm` — same layout as the `.deb`. Needs `rpmbuild`
  (`rpm-tools` / `rpm-build`). A relocated `rpmbuild` should set `RPM_CONFIGDIR` to
  its `usr/lib/rpm` directory. On aarch64 the filename uses `aarch64`.

Needs GTK 3 at runtime (`gtk3` and `glib2` on the RPM). `SKIP_BUILD=1` reuses
`app/build/linux/<arch>/release/bundle`. The script skips the `.rpm` if `rpmbuild`
is not on `PATH`.

### Arch Linux: pacman installer

Build a real `muninn-app` package from the existing Linux tarball (no rebuild,
no root needed). Requires Arch `makepkg` and its normal base-devel tools:

```sh
cd packaging/arch/muninn-app
MUNINN_APP_ARCHIVE=/absolute/path/muninn-app-2.22.0-x86_64-unknown-linux-gnu.tar.gz \
  PKGDEST=/absolute/path/to/out makepkg
sudo pacman -U /absolute/path/to/out/muninn-app-2.22.0-1-x86_64.pkg.tar.zst
```

This installs the GUI under `/opt/muninn-app`, `/usr/bin/muninn`, a menu entry,
and the approved raven icon. Runtime dependencies: GTK 3, GLib, glibc >= 2.39,
and GCC runtime libraries. The separate TUI package owns `mnnn`, so both can be
installed together. Remove the GUI cleanly with `sudo pacman -R muninn-app`;
user data is untouched. Any earlier `~/.local/bin/muninn` wrapper can shadow
`/usr/bin/muninn`, and `~/.local/share/applications/muninn-app.desktop` can
shadow the menu entry; review those manually (packaging does not delete them).

On macOS (Swift Package Manager, not CocoaPods) the same script writes:

- `$DIST/muninn-app-<ver>-universal-apple-darwin.dmg` — `Muninn.app` plus an Applications
  shortcut, LICENSE and changelog (`universal` when `flutter build macos --release`
  produces a fat arm64+x86_64 binary; otherwise `aarch64-apple-darwin` or
  `x86_64-apple-darwin`). `SKIP_BUILD=1` reuses `app/build/macos/Build/Products/Release/*.app`.

The image is not notarized. Drag Muninn into Applications, or open the `.app` from
the disk image. The data directory is still `MUNINN_HOME` or `~/.muninn`.

A Homebrew cask lives at `packaging/homebrew/Casks/muninn-app.rb` (token `muninn-app`,
tap [weirdbrains/tap](https://github.com/WeirdBrains/homebrew-tap)). It installs the
Flutter `Muninn.app` and conflicts with the TUI cask `muninn` (both use that name).
It does not install `mnnn`; use the `muninn` formula for the CLI. `scripts/fill-sums.py`
fills the sha256 when `muninn-app-<ver>-universal-apple-darwin.dmg` is listed in
`SHA256SUMS`. Until a release publishes that asset (this script does not cut a tag), Homebrew 7+
will not install a raw `.rb` path. Copy the cask into a local tap and point it at
the built image:

```sh
brew tap-new --no-git "$USER/muninn-local"   # once
cask="$(brew --repo "$USER/muninn-local")/Casks/muninn-app.rb"
cp packaging/homebrew/Casks/muninn-app.rb "$cask"
dmg=/absolute/path/to/muninn-app-<ver>-universal-apple-darwin.dmg
sha=$(shasum -a 256 "$dmg" | awk '{print $1}')
# In that copy only (do not commit it): url "file://$dmg" and sha256 "$sha"
brew install --cask --appdir="${HOME}/Applications" "$USER/muninn-local/muninn-app"
```

`--appdir` keeps the TUI `Muninn.app` in `/Applications` untouched if it is already
there. After a release that includes the Flutter `.dmg` and the filled cask is
copied into the tap:

```sh
brew install --cask weirdbrains/tap/muninn-app
```

On Windows (Git Bash / MINGW, or `scripts/flutter-dist.ps1` from PowerShell) the
same script writes:

- `$DIST/muninn-app-<ver>-x86_64-pc-windows-msvc.zip` — portable Flutter release
  runner folder (`muninn.exe`, DLLs, `data/`); run `muninn.exe`
- `$DIST/muninn-app-<ver>-x86_64-windows-setup.exe` — Inno Setup 6.3+ installer
  from `packaging/windows/muninn-app.iss` (AppId and `{localappdata}\Programs\MuninnApp`
  are separate from the TUI `muninn.iss` / `{userpf}\Muninn`). Adds a Start Menu
  entry and optionally the install folder to the user PATH for `muninn.exe`
- `$DIST/muninn-app-<ver>-x86_64.msix` — `dart run msix:create` (`store: false`,
  test-certificate signed with `install_certificate: false` so packaging stays
  non-interactive; skipped if the `msix` package is missing). To install the
  eyeball package: enable Developer Mode, or install the test certificate once
  (`certutil -user -addstore Root test_certificate.pfx` after exporting / using
  the cert the `msix` package generates under the app directory), then
  `Add-AppxPackage .\muninn-app-<ver>-x86_64.msix`.

The Flutter MSIX/Store `identity_name` is `WeirdBrains.Muninn` (`app/pubspec.yaml`).
The TUI/CLI winget id is `WeirdBrains.muninn.tui` (see `packaging/winget/`).

Needs Visual Studio Build Tools with the Windows desktop C++ workload, Flutter
Windows desktop enabled (`flutter config --enable-windows-desktop`), and Inno
Setup (`winget install --id JRSoftware.InnoSetup -e`) for the setup.exe. Set
`CARGO_TARGET_DIR` to a scratch directory under the user profile so Native Assets
Rust output does not fill the clone. `SKIP_BUILD=1` reuses
`app/build/windows/x64/runner/Release`. The script does not touch
`%USERPROFILE%\.muninn`.

PowerShell:

```powershell
$env:DIST = 'C:\path\to\out'
$env:CARGO_TARGET_DIR = 'C:\Users\...\.cache\muninn-flutter-windows-cargo'
.\scripts\flutter-dist.ps1
```

Still planned: Apple notarization, iOS / TestFlight (see the iOS opt-out below),
a signed Play upload / testing track, Windows code-signing / Store listing, then
store listings. Desktop, Android and Arch recipes are wired in `release.toml`;
attach the files below to a GitHub release only after they are all in `dist/`
(`docs/release.md`). This script does not cut a tag.

Release asset names (unsigned or test-signed where noted):

- Linux (alien-blue, `scripts/flutter-dist.sh`): `muninn-app-<ver>-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`,
  `muninn-app_<ver>-1_{amd64,arm64}.deb`, `muninn-app-<ver>-1.{x86_64,aarch64}.rpm`,
  plus `muninn-app-bin-<ver>-1-{x86_64,aarch64}.pkg.tar.zst` from the
  `packaging/arch/muninn-app-bin` recipe (TAUR package `muninn-app-bin`).
- macOS (Michael's Mac, `scripts/flutter-dist.sh`): `muninn-app-<ver>-universal-apple-darwin.dmg`
  (**not notarized**).
- Windows (WIN-R0LRC6FBTOM, `scripts/flutter-dist.ps1`): `muninn-app-<ver>-x86_64-pc-windows-msvc.zip`,
  **unsigned** `muninn-app-<ver>-x86_64-windows-setup.exe`, **test-certificate**
  `muninn-app-<ver>-x86_64.msix`. Flutter Windows is x86_64-only.
- Android (Michael's Mac or alien-blue): rename the unsigned release artifacts to
  `muninn-app-<ver>-arm64-v8a-release.apk` and `muninn-app-<ver>-release.aab`.
- iOS: not shipped. A distributable ipa needs an Apple Developer signing identity
  and provisioning profile on Michael's Mac. The committed project uses Automatic
  signing with `iPhone Developer` and has no `DEVELOPMENT_TEAM`.

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
- **Android SDK on Linux:** if `flutter doctor` reports "Unable to locate Android SDK",
  install the command-line tools, then
  `sdkmanager "platform-tools" "platforms;android-36" "build-tools;36.0.0" "ndk;28.2.13676358"`,
  `flutter config --android-sdk ~/Android/Sdk`, `flutter doctor --android-licenses`, and
  `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.
- **Android SDK on macOS:** Flutter can use Android Studio's bundled JBR even when
  `/usr/libexec/java_home` fails and `/usr/bin/java` is only a stub. Set `JAVA_HOME`
  to that JBR (for example `$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home`)
  so `sdkmanager` runs. Pending licenses can be accepted non-interactively with
  `yes | sdkmanager --sdk_root="$ANDROID_HOME" --licenses`. Then:
  `cd app && flutter build apk --release --target-platform android-arm64`
  writes `app/build/app/outputs/flutter-apk/app-release.apk`. `flutter build appbundle --release`
  writes `app/build/app/outputs/bundle/release/app-release.aab`. Copy and rename them
  into `dist/` as `muninn-app-<ver>-arm64-v8a-release.apk` and
  `muninn-app-<ver>-release.aab`. Native Assets compile into `app/.dart_tool/hooks_runner`
  (clear that and `app/build` after copying the artifacts). These are unsigned
  sideload / GitHub artifacts, not a Play upload.

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
