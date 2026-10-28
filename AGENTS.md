# AGENTS.md — rules for agents working on Muninn

## Where to work
- Always work directly on the owner's machine (alien-blue), in
  `~/Projects/kommandosaldat/code/muninn`. No cloud agents, no clones elsewhere.
- Toolchain: Rust >= 1.99 (edition 2024), cargo on this machine. Prefer bundled/vendored
  crate features over installing system packages; ask before installing anything.

## Source of truth
- `PLAN.md` is the spec. Read it before changing anything; record new decisions and any
  deviations there (section 10 / 10.1).
- CLI (clap), TUI (ratatui) and any other front end are thin layers over the library;
  no business logic in them. Menus, keybindings and help dispatch the same named actions.

## Safety
- Never touch the real `~/.muninn`. Tests, examples and manual runs use a temp dir via
  `MUNINN_HOME` / `--home`.
- Never modify or move the raw data in `~/Projects/kommandosaldat/raw/data`.
  Cut small fixtures into `tests/fixtures/`; full-file tests are `#[ignore]` and read
  `MUNINN_RAW_DATA`.
- Never edit files outside this project folder (cargo caches excepted).
- Git: commit locally in logical steps. Do **not** push commits or tags, change remotes,
  force-push, rewrite published history, or skip hooks.
- Secrets (keys, words, shards, passwords) are never logged, printed in `-v` output, or
  committed.

## Versioning and releases
- Semantic versioning. Pre-1.0: MINOR bump per new feature, PATCH for fixes/docs-only.
- Each release: bump `Cargo.toml` (+ `Cargo.lock`), regenerate man pages, update the
  version shown in the TUI header, add a `CHANGELOG.md` entry, commit as
  `release: vX.Y.Z`, and create an annotated tag `vX.Y.Z` (local only).
- `CHANGELOG.md` (Keep a Changelog format) stays **summary level**: a few short one-line
  bullets per release naming the feature. No implementation details, file lists or
  sub-bullets. Tag messages are one short line.

## Documentation
- Every feature is documented in `docs/` (one Markdown file per feature, indexed in
  `docs/README.md`): how it works, flags/defaults, examples using the raw data paths,
  failure modes. Update docs in the same change as the code.
- In-app TUI help is built from `docs/` (single source). Topics are searchable and
  cross-linked; the no-broken-links test must pass. `mnnn help <topic>` prints the same.
- Man pages are generated from the clap definitions (clap_mangen) into `man/`,
  plus hand-written FILES / ENVIRONMENT / EXIT STATUS / SECURITY / EXAMPLES sections.
  Regenerate whenever the CLI or version changes; the out-of-date test must pass.

## Quality bar (before every commit)
- `cargo build`, `cargo clippy --all-targets` (no warnings), `cargo test` all pass.
- New behaviour comes with tests (TUI: ratatui `TestBackend` render tests).

## Fixed design decisions (see PLAN.md for detail)
- Binary `mnnn`; global flags include `--home`, `--db`, `--non-interactive`, `-v`,
  `-q/--quiet`, `-y/--yes` (yes never invents missing values).
- Storage: `~/.muninn/{storage/muninn.database, keys/, themes/<name>/{assets,interface.toml},
  preferences.toml}`. Backups/recovery include database, keys, preferences and themes.
- Bar id = BLAKE3 of `ts_ms|source.name|base/quote|duration_s`; base and quote kept separate.
- Encryption: SQLCipher with envelope keys; master key <-> 24 BIP39 words; 3-of-5 shards;
  HKDF service keys; `keys/` never stores master, words or shards.
- Themes: every theme has dark and light variants; mode light/dark/system; default
  Tokyo Night + system.
- TUI: large header (name + version), categorized clickable menu bar with submenus
  (File, View, Data, Keys, Theme, Help), built-in i3-style tiling content area where any
  tile is splittable, collapsible right sidebar with search/sort/filters, toggleable
  chart layers, import as a popup with a folder browser.

## Communication style
- Keep summaries, changelog entries and commit messages concise.
