# Architecture

Muninn is a cargo workspace; every front end is a thin layer over one library.

| Path | What |
| ---- | ---- |
| `Cargo.toml`, `src/main.rs` | the `muninn` package: the `mnnn` binary (CLI and TUI) |
| `crates/muninn-core` | the library: storage, ingest, crypto, network, org hosting, MCP, themes, about, CLI handlers and TUI widgets |
| `crates/muninn-ffi` | the API the Flutter app calls (flutter_rust_bridge 2) |
| `app/` | the Flutter app for desktop and mobile ([Flutter app](flutter-app.md)) |
| `site/` | the website (Jaspr) ([website](website.md)) |
| `crates/weirdnet-themes` | the theme format, roles, built-in themes (`themes/*.toml`, shared by the TUI, the app and the site) and ratatui colours, as a reusable crate ([themes](themes.md)) |
| `crates/weirdnet-storage` | SQLCipher open with a caller's raw key, rekey, plaintext conversion, migration runner, savepoints, snapshots, private files and the sync change log, as a reusable crate ([sync](sync.md), [encryption](encryption.md)) |
| `crates/weirdnet-keys` | keys, recovery words, Shamir shards, HKDF and Argon2id, sealing, X25519 agreement, HMAC, the envelope keyring (init, unlock, rotate, rekey, recover, export), dark shards and age files, under label profiles, as a reusable crate ([encryption](encryption.md)) |
| `crates/weirdnet-rules` | rule specs (generic over the application's then-actions), the condition syntax and comparison, templates, every and cron schedules, and the matching, loop-protected engine behind a `Host` trait, as a reusable crate ([rules](rules.md)) |
| `crates/weirdnet-code-spaces` | code spaces under an application `Profile`: math, Polars and Python code generation (Python through ruff and the vendored python-ast in `src/python_ast`), the SDK copy and build cache, cargo, ABI-checked loading, trust and line-up hashes, `@imports`, the cycle check, the host table over a `Host` trait and result views, as a reusable crate ([code spaces](code-spaces.md)) |
| `crates/weirdnet-render` | structured data drawn to PNG: tables (an 8x8 bitmap font), line charts and JSON describing either, with input checks and size limits; it depends on no other Muninn or Weirdnet crate |
| `crates/weirdnet-pipelines` | pipelines as graphs of application-defined nodes: the `ID=KIND+tee` and `A>B[:NAME]` syntax, checks, ordering, and a runner (each node once, fan-out, fan-in as named frames, tees, failures skip what is downstream) behind a `Host` trait, plus helpers for JSON column frames; source, step and sink names are the application's ([pipelines](pipelines.md)) |
| `crates/weirdnet-plugin` | the plugin SDK for `weirdnet-code-spaces` (ABI 2, `weirdnet_plugin_*` symbols); its own workspace like `plugin/muninn-plugin`, and copied into `crates/weirdnet-code-spaces/sdk` |

- `mnnn` behaves exactly as before; the root package re-exports `muninn-core`, so
  `muninn::…` paths in tests still work.
- `cargo build`, `cargo test` and `cargo clippy --all-targets` at the root cover the
  whole workspace. Release binaries are still `target/<triple>/release/mnnn`.
- Docs (`docs/`), protobufs (`proto/`) and assets stay at the root; `muninn-core`
  embeds them from there.
- About Muninn (name, tagline, version, publisher, homepage) lives in
  `crates/muninn-core/src/about.rs` and feeds the TUI, the app and the site.

## Crate extraction

Parts of `muninn-core` are being split into reusable Weirdnet crates (keys, net, themes,
rules, code spaces, pipelines, storage), one phase at a time; Muninn stays their consumer and
keeps its schema, adapters and permission checks. Phase 0 added the seams without changing any format; extracted crates are listed in the
table above and below:

| Seam | Domain side (moves later) | Muninn side (stays) |
| ---- | ---- | ---- |
| Storage (extracted: `crates/weirdnet-storage`) | the crate: open with a raw key, key / rekey (verified on a fresh connection), plaintext-to-encrypted conversion (verified before it replaces the file), migration runner, savepoints, snapshots, private files and folders; its `sync` feature: the change log (triggers, log, agreed hashes, conflicts), row codec and hash, three-way decision | `Store`: schema (`MIGRATIONS`, including the sync tables), data upgrades, seed rows, queries; the synced-table registry (`store::sync::TABLES`), model tables, history import, the sync epoch and the order changes travel in; `Paths` names the `tmp-muninn` side file |
| Keys (extracted: `crates/weirdnet-keys`) | the crate: every key encryption, decryption, derivation, wrap, rotation, rekey (undone if its envelope cannot be saved), recovery, export and shard operation, on vetted crates; labels come from a `Profile` (`Profile::WEIRDNET` by default); the keyring reaches files and the database only through its `KeyFiles` and `EncryptedDatabase` traits | `crypto` keeps the old function names over `MUNINN_PROFILE` (Muninn's existing labels and envelope format 1, so existing homes, words, shards, exports and peers are unchanged); `Paths` implements both traits; protocol labels and message formats (exchange offers and answers, sync and message associated data, org sign-in headers) stay in `net` and `org` and call the crate's primitives; backup manifests and `--only` parsing stay in Muninn |
| Rules (extracted: `crates/weirdnet-rules`) | the crate: `RuleSpec<T>`, `Condition`, `Op`, `Schedule` (the spec JSON is unchanged), condition / list / word parsing and quoting, `on` patterns checked against a catalog the caller passes, `compare`, `{field}` templates, `every` and five-field cron (stepped in wall-clock time, so daylight-saving changes neither loop nor skip days), `next_run` and `due`, `check`, `dispatch` and `run` with the chain, depth and run-budget limits; it reaches the application only through `engine::Host` (enabled rules, fields, related, begin / finish / blocked run, execute one then-action) | `rules` keeps the old function names and messages over the crate; the action catalog (`EVENTS`), the `Then` enum and its parsing, printing, planning and execution, `mcp:` fields, what "related" means, the `rules` table and owners, `rule.run` history records and `rule_chain` provenance, the scheduler loop and lease, the built-in cert watcher, and every permission and approval check (MCP tool approval, code-space line-up approval, org roles) inside the then-actions; `Store` holds the engine's `Runtime` in its `RuleRuntime` and calls the engine from `record_action` |
| Code spaces (extracted: `crates/weirdnet-code-spaces`) | the crate: `Lang` (stored the same), checks, code generation, the generated crate and its manifest, the SDK files and hash, build keys, cargo and the content-addressed plugin cache, loading with the symbol and ABI check, trust hashes and `approved`, line-up hashes, `@imports` parsing and renaming, the cycle and depth check over a `spaces::Graph`, the host table (`call` / `free`, panics caught, UTF-8 checked) over a `Host` trait, and result views (`ResultView` parsed, checked and cut to its limits, table / chart / text / sparkline conversions) | `code` keeps the old function names over `MUNINN` (the `muninn-plugin` 1.3.0 SDK, ABI 1 `muninn_plugin_*` symbols, `muninn-code-trust` and `muninn-space-lineup` domains, `mnnn_code_` crates, `MUNINN_CARGO`, the same messages), so trust hashes, approvals, build keys, generated code and cached plugins are unchanged and only v1 plugins load; the `code_spaces` and spaces tables, approving and who may approve, `code.run` history, the host methods over Muninn's data (`host::Run` implements `Host`), drawing result views in the TUI, the app and reports (`code::view` re-exports the crate's; `code::png` turns a chart view into a `weirdnet-render` chart for report images), connectors, parsers, pipelines, organization executables and `MUNINN_HOME`/`MUNINN_CODE_TARGET` |
| Pipelines (extracted: `crates/weirdnet-pipelines`) | the crate: `Graph<K>`, `Node<K>`, `Edge` (the stored graph JSON is unchanged), node and edge parsing, `order` and its cycle error, `check` (`Problem`s, with an optional resolver for names), `execute` over a `Host` (output, tee, produced, failed, finished) with `NodeRun` and `PipelineRun`, and the frame helpers (`union`, `select`, `filter_rows`, `tail`, `rows_frame`, `read_data`, `csv_line`, `markdown`) | `code::pipeline` keeps `Graph`, `Node`, `NodeKind` and `run`: the node kinds (`search:`, `journal:`, `source:`, `-`, `code:`, `space:`, `view:`, `filter:`, `model:`) and their messages, `GraphExt` (`problems` with Muninn's wording and name checks against the store, `spaces`, `code_spaces`, `sinks`), the run host (sources, code spaces and their approval, `waiting approval`, live-search filters, saves to journals, models, projects, orgs and the network, `pipe.start` / `pipe.hop` / `pipe.run` / `pipe.frame` records), the `pipelines` table, drafts from peers, export and import, linear pipes (`code::pipe`), the designer and the app |
| Rendering (extracted: `crates/weirdnet-render`) | the crate: `Table`, `Chart` and JSON (`{columns, rows}`, a list of objects or of rows, or `{x, series, width, height}`) to PNG, chart pixels as Muninn drew them, `Limits` on columns, rows, cell length, series, points, image size and JSON size | `code::png` is the adapter from result views (the same images and legends in reports as before); drawing in the TUI and the app |
| Themes (extracted: `crates/weirdnet-themes`) | the crate, through `ThemeDirs` and `Mode`; ratatui behind its `ratatui` feature | `theme` re-exports it and returns `crate::Error`; `Paths` implements `ThemeDirs`; `prefs::ThemeMode` converts into `Mode` |

- The storage engine never derives, wraps or stores a key: it applies the raw key the
  keyring hands it. The keyring never opens SQLite: it asks its `EncryptedDatabase`.
- `crates/muninn-core/src/compat_tests.rs` pins every Muninn-branded wire, signature, KDF, key-format and
  plugin-ABI identifier byte for byte. Renaming one (for example `/muninn/net/1`) breaks
  peers, signatures or existing databases, so it only happens in a separately approved
  rollout with migration adapters, never as part of moving code.

Extracted crates have their own SemVer (`weirdnet-themes` 0.1.0, `weirdnet-storage` 0.1.0, `weirdnet-keys` 0.1.0, `weirdnet-rules` 0.1.0,
`weirdnet-code-spaces` 0.1.0, `weirdnet-plugin` 0.1.0, `weirdnet-render` 0.1.0, `weirdnet-pipelines` 0.1.0),
depend on nothing in Muninn, and are `publish = false` until a publication decision; Muninn
re-exports or wraps them so its paths (`muninn_core::theme::…`, `muninn_core::store::sync::Page`)
and messages ("could not unlock the database: wrong key or not a muninn database …") stay the same.
