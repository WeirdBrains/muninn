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

Release notes are the tag annotation plus the version's `CHANGELOG.md` section
(`scripts/release-notes.sh X.Y.Z`). `.github/workflows/ci.yml` builds, lints and tests
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
