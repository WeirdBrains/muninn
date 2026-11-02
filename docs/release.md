# Releasing

Releases are built, checked and published locally. A GitHub Actions workflow
(`.github/workflows/release.yml`) can build the same files, but it is not how current
releases are made.

1. `scripts/release.sh X.Y.Z "Summary" "Changelog bullet" ...` bumps the version,
   regenerates man pages, adds the `CHANGELOG.md` entry, commits `release: vX.Y.Z` and
   creates the annotated tag `vX.Y.Z`. It never pushes. Also bump `pkgver` in both Arch
   PKGBUILDs and `version` in `packaging/homebrew/muninn.rb`. Then
   `git push origin main --follow-tags`.
2. Build the 14 files from the tag:

| Built on | Files |
|---|---|
| Linux x86_64 (cross-compiling aarch64) | `cargo build --locked --release --target T`, strip, `scripts/dist.sh T`, `cargo deb`, `cargo generate-rpm`: tarballs, `.deb` and `.rpm` for x86_64 and aarch64 |
| Linux, llvm-mingw | `scripts/cross-windows.sh x86_64 aarch64`: `muninn-<ver>-{x86_64,aarch64}-pc-windows-gnullvm.zip` |
| macOS | `COPYFILE_DISABLE=1`, `scripts/dist.sh` for `aarch64-apple-darwin` and `x86_64-apple-darwin` |
| Arch (makepkg) | `packaging/arch/muninn-bin` built from the local tarballs, `x86_64` and (with `CARCH=aarch64`) `aarch64` |

3. In `dist/`, run `sha256sum -- * > SHA256SUMS` and `scripts/fill-sums.py X.Y.Z dist`
   (the Arch PKGBUILD, Homebrew formula and winget manifests, with real checksums). Tar
   `dist/packaging` into `muninn-<ver>-packaging.tar.gz`, add its checksum, then check
   with `sha256sum -c SHA256SUMS`, the entry counts and the `LICENSE` in every package.
4. Publish only when all 14 files are there:
   `gh release create vX.Y.Z --verify-tag --title "Muninn vX.Y.Z" --notes-file <(scripts/release-notes.sh X.Y.Z) dist/*`.
   Only the highest version is marked **Latest**: an older tag published later gets
   `--latest=false`. Assets can't change once a release is published.

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

After publishing, the packaging tarball's files are what get submitted to
microsoft/winget-pkgs, the Homebrew tap and the AUR.

## Local checks

```sh
actionlint                                  # workflow syntax (user-local install)
scripts/dist.sh                             # host tarball into dist/
LLVM_MINGW=~/.local/opt/llvm-mingw-... scripts/cross-windows.sh
packaging/arch/build-local.sh               # muninn-bin package
cargo deb && cargo generate-rpm             # after cargo build --release
```
