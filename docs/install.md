# Install

Every package ships a prebuilt `mnnn` binary: **no Rust or other toolchain is needed to
install**. SQLCipher and OpenSSL are compiled into the binary, so the only runtime
dependencies are the C runtime: glibc and libgcc (Linux), the system libraries (macOS),
nothing extra (Windows). Downloads are on the GitHub Releases page with a `SHA256SUMS`
file. Packaging files live in `packaging/`.

Building code spaces is optional and needs extra tools that the packages do not bundle:
`cargo` (from [rustup](https://rustup.rs)) for math, Polars, Rust and Python; Python is
translated in process, so no Python install is needed. See
[code spaces](code-spaces.md#python-in-rust).

## Getting a release

Each release is built and checked locally, and its files are attached to its page under
[GitHub Releases](https://github.com/WeirdBrains/muninn/releases). The latest is
[v2.18.0](https://github.com/WeirdBrains/muninn/releases/tag/v2.18.0). Every release has
14 files:

| File (v2.18.0) | For |
|---|---|
| `muninn-2.18.0-x86_64-unknown-linux-gnu.tar.gz`, `muninn-2.18.0-aarch64-unknown-linux-gnu.tar.gz` | any Linux |
| `muninn-2.18.0-x86_64-apple-darwin.tar.gz`, `muninn-2.18.0-aarch64-apple-darwin.tar.gz` | macOS (Intel, Apple silicon) |
| `muninn_2.18.0-1_amd64.deb`, `muninn_2.18.0-1_arm64.deb` | Debian, Ubuntu |
| `muninn-2.18.0-1.x86_64.rpm`, `muninn-2.18.0-1.aarch64.rpm` | Fedora, RHEL |
| `muninn-bin-2.18.0-1-x86_64.pkg.tar.zst`, `muninn-bin-2.18.0-1-aarch64.pkg.tar.zst` | Arch Linux |
| `muninn-2.18.0-x86_64-pc-windows-gnullvm.zip`, `muninn-2.18.0-aarch64-pc-windows-gnullvm.zip` | Windows (x64, arm64) |
| `muninn-2.18.0-packaging.tar.gz` | the Homebrew formula, Arch PKGBUILD and winget manifests, with this release's checksums filled in |
| `SHA256SUMS` | checksums for all of the above |

Download from the command line and check it, for example:

```sh
ver=2.18.0
base=https://github.com/WeirdBrains/muninn/releases/download/v$ver
curl -LO $base/muninn-$ver-x86_64-unknown-linux-gnu.tar.gz -LO $base/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS      # macOS: shasum -a 256 --ignore-missing -c SHA256SUMS
```

(`gh release download v2.18.0` does the same if you have the GitHub CLI.) Muninn is
BSD-3-Clause licensed; every package carries the `LICENSE`.

Installing or uninstalling never touches your data in `~/.muninn` (or `MUNINN_HOME`);
delete that folder yourself to remove it.

## Release tarball (any Linux or macOS)

```sh
t=x86_64-unknown-linux-gnu     # or aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin
tar -xzf muninn-2.18.0-$t.tar.gz
install -Dm755 muninn-2.18.0-$t/mnnn ~/.local/bin/mnnn
mkdir -p ~/.local/share/man/man1 && cp muninn-2.18.0-$t/man/*.1 ~/.local/share/man/man1/
```

Uninstall: remove `~/.local/bin/mnnn` and `~/.local/share/man/man1/mnnn*.1`.

## Arch Linux

**From a checkout (works today, needs Rust):**

```sh
git clone git@github.com:WeirdBrains/muninn.git && cd muninn/packaging/arch
makepkg -si            # builds the committed checkout, installs package "muninn"
```

**Prebuilt (`muninn-bin`, no Rust):** install the release's ready-made package:

```sh
sudo pacman -U muninn-bin-2.18.0-1-x86_64.pkg.tar.zst     # or …-aarch64.pkg.tar.zst
```

Or build it yourself from the release's PKGBUILD (`packaging/arch/PKGBUILD` in
`muninn-2.18.0-packaging.tar.gz`, which has the checksums filled in) with `makepkg -si`. If
`muninn-bin` is in the AUR when you read this, `yay -S muninn-bin` or `paru -S muninn-bin`
installs the same package.

Both install `/usr/bin/mnnn`, man pages and `/usr/share/doc/muninn`. Uninstall:
`sudo pacman -R muninn` (or `muninn-bin`). Remove an earlier `cargo install` first
(`cargo uninstall muninn`), or `~/.cargo/bin/mnnn` may shadow the package, depending on
your `PATH`. 

**Build and install from a checkout in one step:** `packaging/arch/build-local.sh`
builds `muninn-bin` from the current checkout (no download) and installs it with
`sudo pacman -U --needed`. `--source` builds `packaging/arch/PKGBUILD` (package
`muninn`) instead, `--no-install` only builds into `target/pkg/arch/`. If `namcap` is
installed it lints the package first; it is optional and never blocks the install.

## Debian / Ubuntu

```sh
sudo apt install ./muninn_2.18.0-1_amd64.deb      # arm64: muninn_2.18.0-1_arm64.deb
```

Depends on `libc6 (>= 2.34)` and `libgcc-s1`. Uninstall: `sudo apt remove muninn`.
Maintainers build it with `cargo deb` (`packaging/debian/README.md`).

## Fedora / RHEL

```sh
sudo dnf install ./muninn-2.18.0-1.x86_64.rpm     # or muninn-2.18.0-1.aarch64.rpm
```

Requires glibc (detected from the binary). Uninstall: `sudo dnf remove muninn`.
Maintainers build it with `cargo generate-rpm` (`packaging/fedora/README.md`).

## macOS and Linux (Homebrew)

`packaging/homebrew/muninn.rb` installs the prebuilt binary for macOS arm64/x86_64 or
Linux arm64/x86_64:

The formula is in the release's packaging tarball, with that release's checksums filled in:

```sh
tar -xzf muninn-2.18.0-packaging.tar.gz
brew install ./packaging/homebrew/muninn.rb
```

Once a Homebrew tap is published, `brew install muninn` from it installs the same thing.

Uninstall: `brew uninstall muninn`.

## Windows (winget)

Release zips hold a portable `mnnn.exe` for x64 and arm64
(`muninn-2.18.0-x86_64-pc-windows-gnullvm.zip`, `muninn-2.18.0-aarch64-pc-windows-gnullvm.zip`).
The simplest install: unzip anywhere and put `mnnn.exe` on `PATH`.

The release's packaging tarball has winget manifests (schema 1.9) with its checksums filled in:

```powershell
tar -xzf muninn-2.18.0-packaging.tar.gz
winget install --manifest packaging\winget\2.18.0
```

Once the package is accepted into winget-pkgs, `winget install Kommandosaldat.Muninn` works too. Uninstall: `winget uninstall
Kommandosaldat.Muninn` (or delete the folder). Data lives in `%USERPROFILE%\.muninn`.

## From source (any platform)

Needs Rust >= 1.99, a C compiler, `perl` and `make`:

```sh
cargo install --locked --path .
```

Uninstall: `cargo uninstall muninn`.
