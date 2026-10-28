# Releasing

Releases are cut locally and published by GitHub Actions when the tag is pushed.

1. `scripts/release.sh X.Y.Z "Summary" "Changelog bullet" ...` bumps the version,
   regenerates man pages, adds the `CHANGELOG.md` entry, commits `release: vX.Y.Z` and
   creates the annotated tag `vX.Y.Z`. It never pushes. Also bump `pkgver` in the Arch
   PKGBUILDs and add a winget manifest directory for the new version if they changed.
2. `git push origin main vX.Y.Z` (by the maintainer) starts `.github/workflows/release.yml`:

| Job | Runs on | Produces |
|---|---|---|
| unix | ubuntu-22.04, ubuntu-22.04-arm, macos-15-intel, macos-15 | `muninn-<ver>-<target>.tar.gz` for Linux x86_64/aarch64 and macOS x86_64/aarch64; `.deb` (amd64, arm64) and `.rpm` (x86_64, aarch64) |
| windows | ubuntu-22.04 + llvm-mingw | `muninn-<ver>-{x86_64,aarch64}-pc-windows-gnullvm.zip` |
| arch | archlinux container | `muninn-bin-<ver>-1-x86_64.pkg.tar.zst` |
| release | ubuntu-22.04 | `SHA256SUMS`, `muninn-<ver>-packaging.tar.gz` (Arch PKGBUILD, Homebrew formula and winget manifests with real sums, by `scripts/fill-sums.py`), and the GitHub Release |

Only the highest version tag is marked **Latest**: the publish step compares the tag
with `git tag -l 'v*' --sort=-v:refname` and passes `make_latest` accordingly, so runs
for several pushed tags can finish in any order. (No `concurrency` group: GitHub would
cancel queued tag runs beyond one pending.) The Arch job builds `muninn-bin` for
`x86_64` and `aarch64` (the aarch64 one repackages the prebuilt aarch64 tarball with
`CARCH=aarch64`); both packages are release assets listed in `SHA256SUMS`.

The release is named `Muninn vX.Y.Z` and its notes are the version's `CHANGELOG.md`
section (`scripts/release-notes.sh X.Y.Z`). Write each bullet as the feature plus how to
use it (the command or key), e.g. ``Import a whole folder: `mnnn ingest path <DIR>` …``.
`release.sh` puts bullets under `### Added`, or `### Fixed` / `### Changed` when they
start with `Fixed: ` / `Changed: `.

To rewrite the notes of already-published releases from `CHANGELOG.md` (allowed for
immutable releases; only assets and tags are locked) and mark the highest tag Latest:

```sh
scripts/sync-release-notes.sh --dry-run --latest   # preview
scripts/sync-release-notes.sh --latest             # every release (or list tags: v1.7.0 v1.8.0)
```
 `.github/workflows/ci.yml` builds, lints and tests
pushes to `main` and pull requests.

After publishing: submit the winget manifests to microsoft/winget-pkgs and copy the
formula and PKGBUILD to the Homebrew tap and AUR.

## Local checks

```sh
actionlint                                  # workflow syntax (user-local install)
scripts/dist.sh                             # host tarball into dist/
LLVM_MINGW=~/.local/opt/llvm-mingw-... scripts/cross-windows.sh
packaging/arch/build-local.sh               # muninn-bin package
cargo deb && cargo generate-rpm             # after cargo build --release
```
