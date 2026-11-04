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
