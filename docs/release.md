# Releasing

Releases are built, checked and published locally. A GitHub Actions workflow
(`.github/workflows/release.yml`) can build the same files, but it is not how current
releases are made.

1. `python3 scripts/weirdnet_release.py [--patch|--major|--version X.Y.Z] "Summary" "Changelog bullet" ...`
   (the shared WeirdBrains release script, vendored from weirdnet-project-helpers and configured
   by `release.toml`; MINOR by default, `--dry-run` first) bumps the version, regenerates man
   pages, adds the `CHANGELOG.md` entry, runs the man and packaging tests and the full quality
   bar (fmt, clippy, tests), commits `release: vX.Y.Z`, creates the annotated tag `vX.Y.Z`
   (`vX.Y.Z: Summary`) and pushes `main` and that tag in one atomic push (`--no-push` keeps them
   local). It also bumps `pkgver` in both Arch PKGBUILDs, `version` in the Homebrew formula and
   cask (`packaging/homebrew/`), the other pinned files listed in `release.toml`, and moves the
   winget template (`packaging/winget/`) to the new version. Then it publishes `muninn-plugin`
   to crates.io if its version isn't there yet (asking first). Don't edit the vendored copy:
   change weirdnet-project-helpers and re-vendor it.
2. Build every asset listed in `release.toml` `[[platforms]]` from the tag, into one
   `dist/` directory. GitHub release assets are immutable once published, so collect
   the TUI/CLI and Flutter files *before* `gh release create`. Do not use
   `.github/workflows/release.yml` as the sole publisher: that workflow still only
   builds the TUI/CLI Linux and Windows files and would lock a release without the
   Flutter assets.

   Unsigned or test-signed packages must stay clearly marked (filenames stay as
   `flutter-dist` produces them; the notes below, the Homebrew muninn-app cask
   caveats, the Windows `INSTALL.txt`, and the `muninn-app-bin` PKGBUILD comment
   say so):

   - macOS `muninn-app-<ver>-universal-apple-darwin.dmg` is **not notarized**.
   - Windows `muninn-app-<ver>-x86_64-windows-setup.exe` is **unsigned**.
   - Windows `muninn-app-<ver>-x86_64.msix` is **test-certificate signed** (not a
     Store or trusted cert).
   - Android APK and AAB are **unsigned release builds**, for GitHub / sideload,
     not a Play or F-Droid upload.
   - iOS is opted out of the asset matrix until a signing identity exists (see
     below).

| Built on | Files |
|---|---|
| alien-blue (Linux x86_64, cross-compiling aarch64) | TUI: `cargo build --locked --release -p muninn --target T`, strip, `scripts/dist.sh T`, `cargo deb`, `cargo generate-rpm`: tarballs, `.deb` and `.rpm` for x86_64 and aarch64 |
| alien-blue, llvm-mingw | TUI: `scripts/cross-windows.sh x86_64 aarch64`: `muninn-<ver>-{x86_64,aarch64}-pc-windows-gnullvm.zip` |
| Michael's Mac | TUI: `COPYFILE_DISABLE=1`, `scripts/dist.sh` (which runs `xattr -cr` on the staged files before `tar`) for `aarch64-apple-darwin` and `x86_64-apple-darwin` |
| Windows host with Inno Setup, from Linux (alien-blue) | TUI: `scripts/windows-setup.sh`: `muninn-<ver>-{x86_64,aarch64}-windows-setup.exe` from the zips (`packaging/windows/`) |
| Michael's Mac | TUI: `scripts/macos-app.sh`, after both darwin tarballs: universal `Muninn.app` in `muninn-<ver>-macos.dmg` |
| alien-blue (Arch, makepkg) | TUI: `packaging/arch/muninn-bin` built from the local tarballs, `x86_64` and (with `CARCH=aarch64`) `aarch64` |
| alien-blue | Flutter Linux: `scripts/flutter-dist.sh` → `muninn-app-<ver>-{x86_64,aarch64}-unknown-linux-gnu.tar.gz`, `muninn-app_<ver>-1_{amd64,arm64}.deb`, `muninn-app-<ver>-1.{x86_64,aarch64}.rpm`. Then makepkg `packaging/arch/muninn-app-bin` from the filled recipe (or the tarballs) for `muninn-app-bin-<ver>-1-{x86_64,aarch64}.pkg.tar.zst` |
| Michael's Mac | Flutter macOS: `scripts/flutter-dist.sh` → `muninn-app-<ver>-universal-apple-darwin.dmg` (not notarized) |
| WIN-R0LRC6FBTOM (Windows VM, machineId `c4b27d34-b4f9-416b-a44e-b37376c1cf0d`) | Flutter Windows: `scripts/flutter-dist.ps1` → `muninn-app-<ver>-x86_64-pc-windows-msvc.zip`, unsigned `muninn-app-<ver>-x86_64-windows-setup.exe`, test-cert `muninn-app-<ver>-x86_64.msix`. Flutter Windows is x86_64-only. |
| Michael's Mac or alien-blue | Android (unsigned): `cd app && flutter build apk --release --target-platform android-arm64` and `flutter build appbundle --release`, then rename to `muninn-app-<ver>-arm64-v8a-release.apk` and `muninn-app-<ver>-release.aab` in `dist/` |
| Michael's Mac | iOS ipa: **not built for the release**. The project has no committed `DEVELOPMENT_TEAM` or provisioning profile (Automatic / `iPhone Developer` only). A distributable sideload, AltStore, TestFlight or App Store ipa waits on a signing identity. `release.toml` opts iOS out for this reason. |

3. In `dist/`, run `sha256sum -- * > SHA256SUMS` and `scripts/fill-sums.py X.Y.Z dist`
   (the Arch `muninn-bin` and, when the Flutter Linux tarballs are present,
   `muninn-app-bin` PKGBUILDs, Homebrew formula and casks, and winget manifests,
   with real checksums). Tar `dist/packaging` into `muninn-<ver>-packaging.tar.gz`,
   add its checksum, then check with `sha256sum -c SHA256SUMS`, the entry counts
   and the `LICENSE` in every package. `fill-sums.py` still requires the TUI Linux
   and macOS tarballs; a missing Flutter Linux tarball, app dmg or Windows setup
   skips that recipe, cask or manifest with a warning.
4. Publish only when every `release.toml` asset is in `dist/`:
   `gh release create vX.Y.Z --verify-tag --title "Muninn vX.Y.Z" --notes-file <(scripts/release-notes.sh X.Y.Z) dist/*`.
   Only the highest version is marked **Latest**: an older tag published later gets
   `--latest=false`. Assets can't change once a release is published, so do not
   create the release until the Flutter files are there too.
5. Publish the package managers from `dist/packaging`: copy `homebrew/Formula/muninn.rb`,
   `homebrew/Casks/muninn.rb` and `homebrew/Casks/muninn-app.rb` into
   [WeirdBrains/homebrew-tap](https://github.com/WeirdBrains/homebrew-tap) and push; copy
   `winget/manifests/w/WeirdBrains/muninn/tui/X.Y.Z` into a fork of microsoft/winget-pkgs,
   run `winget validate --manifest` on Windows, and open a PR. Then publish the
   prebuilt Arch packages `muninn-bin` (TUI/CLI) and `muninn-app-bin` (Flutter desktop)
   to TAUR:

   ```sh
   python3 scripts/weirdnet_release.py taur X.Y.Z --dry-run   # both recipes
   python3 scripts/weirdnet_release.py taur X.Y.Z             # then upload them
   # or one package: python3 scripts/weirdnet_release.py taur X.Y.Z --package muninn-app-bin --dry-run
   ```

   Each `[[taur.packages]]` entry in `release.toml` runs
   `scripts/publish-taur.sh --package <name> {version}`.

   Run it only after the GitHub release is published: it refuses a draft release. It
   downloads the release's packaging tarball (or takes `dist/muninn-X.Y.Z-packaging.tar.gz`
   as a second argument), checks the PKGBUILD's `sha256sums_*` against the release's
   `SHA256SUMS` and the downloaded Linux tarballs (`muninn-…` or `muninn-app-…`),
   writes `.SRCINFO` with `makepkg --printsrcinfo` and checks its name, version,
   sources and sums, packs `PKGBUILD` and `.SRCINFO`, and runs
   `taurctl pkg upload <archive> --org WeirdBrains -y`. It does nothing when TAUR
   already has that version and refuses if the package belongs to another
   organization. Sign in with `taurctl login` first if needed; the script uses
   the signed-in taurctl session and never reads or prints the token. It needs `gh`,
   `makepkg` (Arch) and `taurctl`. `packaging/arch/test-publish-taur.sh` checks both
   packages with stub tools (CI runs it).

The release is named `Muninn vX.Y.Z` and its notes are the version's `CHANGELOG.md`
section (`scripts/release-notes.sh X.Y.Z`). Write each bullet as the feature plus how to
use it (the command or key), e.g. ``Import a whole folder: `mnnn ingest path <DIR>` …``.
The release script puts bullets under `### Added`, or `### Fixed` / `### Changed` when they
start with `Fixed: ` / `Changed: `. The first message is the summary (tag message only).
`python3 scripts/weirdnet_release.py assets X.Y.Z` checks the release has every platform file
listed in `release.toml` (including Flutter desktop and Android; iOS is opted out).

To rewrite the notes of already-published releases from `CHANGELOG.md` (allowed for
immutable releases; only assets and tags are locked) and mark the highest tag Latest:

```sh
scripts/sync-release-notes.sh --dry-run --latest   # preview
scripts/sync-release-notes.sh --latest             # every release (or list tags: v1.7.0 v1.8.0)
```
 `.github/workflows/ci.yml` builds, lints and tests
pushes to `main` and pull requests.

After publishing, the packaging tarball's files are what get submitted to
microsoft/winget-pkgs, the Homebrew tap and TAUR (`muninn-bin` and `muninn-app-bin` via
`scripts/publish-taur.sh` / `taurctl pkg upload`).

## Local checks

```sh
actionlint                                  # workflow syntax (user-local install)
scripts/dist.sh                             # host tarball into dist/
LLVM_MINGW=~/.local/opt/llvm-mingw-... scripts/cross-windows.sh
packaging/arch/build-local.sh               # muninn-bin package
cargo deb && cargo generate-rpm             # after cargo build --release
```
