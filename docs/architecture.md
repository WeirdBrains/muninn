# Architecture

Muninn is a cargo workspace; every front end is a thin layer over one library.

| Path | What |
| ---- | ---- |
| `Cargo.toml`, `src/main.rs` | the `muninn` package: the `mnnn` binary (CLI and TUI) |
| `crates/muninn-core` | the library: storage, ingest, crypto, network, org hosting, MCP, themes, about, CLI handlers and TUI widgets |
| `crates/muninn-ffi` | the API the Flutter app calls (flutter_rust_bridge 2) |
| `app/` | the Flutter app for desktop and mobile ([Flutter app](flutter-app.md)) |
| `site/` | the website (Jaspr) ([website](website.md)) |
| `assets/themes/` | the themes shared by the TUI, the app and the site ([themes](themes.md)) |

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
keeps its schema, adapters and permission checks. Phase 0 adds the seams, without moving code
or changing any format:

| Seam | Domain side (moves later) | Muninn side (stays) |
| ---- | ---- | ---- |
| Storage engine | `store/engine.rs`: open with a raw key, key / rekey pragmas, plaintext-to-encrypted conversion, migration runner, private modes | `Store`: schema (`MIGRATIONS`), data upgrades, seed rows, queries |
| Keys | `crypto/keyring.rs` through its `KeyFiles` and `EncryptedDatabase` traits; it owns every key format and operation | `Paths` implements both traits (folder layout, SQLCipher work through the engine) |
| Themes | `theme/` through `ThemeDirs` and `theme::Mode`; ratatui only in `theme/term.rs` | `Paths` implements `ThemeDirs`; `prefs::ThemeMode` converts into `Mode` |

- The storage engine never derives, wraps or stores a key: it applies the raw key the
  keyring hands it. The keyring never opens SQLite: it asks its `EncryptedDatabase`.
- `crates/muninn-core/src/compat_tests.rs` pins every Muninn-branded wire, signature, KDF, key-format and
  plugin-ABI identifier byte for byte. Renaming one (for example `/muninn/net/1`) breaks
  peers, signatures or existing databases, so it only happens in a separately approved
  rollout with migration adapters, never as part of moving code.
