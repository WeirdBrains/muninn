# Install

Every package ships a prebuilt `mnnn` binary: **no Rust or other toolchain is needed to
install**. SQLCipher and OpenSSL are compiled into the binary, so the only runtime
dependencies are the C runtime: glibc and libgcc (Linux), the system libraries (macOS),
nothing extra (Windows). Downloads are on the GitHub Releases page with a `SHA256SUMS`
file. Packaging files live in `packaging/`.

Installing or uninstalling never touches your data in `~/.muninn` (or `MUNINN_HOME`);
delete that folder yourself to remove it.

## Release tarball (any Linux or macOS)

```sh
tar -xzf muninn-<ver>-<target>.tar.gz      # e.g. x86_64-unknown-linux-gnu, aarch64-apple-darwin
install -Dm755 muninn-<ver>-<target>/mnnn ~/.local/bin/mnnn
mkdir -p ~/.local/share/man/man1 && cp muninn-<ver>-<target>/man/*.1 ~/.local/share/man/man1/
```

Uninstall: remove `~/.local/bin/mnnn` and `~/.local/share/man/man1/mnnn*.1`.

## Arch Linux

`packaging/arch/muninn-bin/PKGBUILD` repackages the release tarball (AUR-style
`muninn-bin`, conflicts with a source build):

```sh
makepkg -si                                # in a copy of packaging/arch/muninn-bin/
```

Uninstall: `sudo pacman -R muninn-bin`. For a source build use `packaging/arch/PKGBUILD`
(needs Rust); `packaging/arch/build-local.sh [--source]` builds either from a checkout.

## Debian / Ubuntu

```sh
sudo apt install ./muninn_<ver>-1_amd64.deb
```

Depends on `libc6 (>= 2.34)` and `libgcc-s1`. Uninstall: `sudo apt remove muninn`.
Maintainers build it with `cargo deb` (`packaging/debian/README.md`).

## Fedora / RHEL

```sh
sudo dnf install ./muninn-<ver>-1.x86_64.rpm
```

Requires glibc (detected from the binary). Uninstall: `sudo dnf remove muninn`.
Maintainers build it with `cargo generate-rpm` (`packaging/fedora/README.md`).

## macOS and Linux (Homebrew)

`packaging/homebrew/muninn.rb` installs the prebuilt binary for macOS arm64/x86_64 or
Linux arm64/x86_64:

```sh
brew install ./packaging/homebrew/muninn.rb     # or from a tap: brew install muninn
```

Uninstall: `brew uninstall muninn`.

## From source (any platform)

Needs Rust >= 1.99, a C compiler, `perl` and `make`:

```sh
cargo install --locked --path .
```

Uninstall: `cargo uninstall muninn`.
