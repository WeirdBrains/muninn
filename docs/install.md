# Install

Muninn ships one binary, `mnnn`, plus man pages and these docs. Packaging files live in
`packaging/` in the source tree. Data is never touched by installing or uninstalling:
it stays in `~/.muninn` (or `MUNINN_HOME`); delete that folder yourself to remove it.

## From source (any platform)

Needs Rust >= 1.99, a C compiler, `perl` and `make` (SQLCipher and OpenSSL are built in).

```sh
cargo install --locked --path .        # installs ~/.cargo/bin/mnnn
mkdir -p ~/.local/share/man/man1 && cp man/*.1 ~/.local/share/man/man1/
```

Uninstall: `cargo uninstall muninn` and remove `~/.local/share/man/man1/mnnn*.1`.

## Arch Linux

`packaging/arch/PKGBUILD` builds from the GitHub release tarball. To build from a local
checkout instead (no download):

```sh
packaging/arch/build-local.sh          # -> target/pkg/arch/muninn-<ver>-1-x86_64.pkg.tar.zst
sudo pacman -U target/pkg/arch/muninn-*.pkg.tar.zst
```

Installs `/usr/bin/mnnn`, man pages in `/usr/share/man/man1`, docs in
`/usr/share/doc/muninn`. Uninstall: `sudo pacman -R muninn`.

## Debian / Ubuntu

Built with `cargo deb` from `[package.metadata.deb]` in `Cargo.toml`
(see `packaging/debian/README.md`):

```sh
cargo install --locked cargo-deb
cargo deb                              # -> target/debian/muninn_<ver>-1_amd64.deb
sudo apt install ./target/debian/muninn_*_amd64.deb
```

Same layout as Arch (`/usr/bin/mnnn`, man pages, `/usr/share/doc/muninn`).
Uninstall: `sudo apt remove muninn`.

## Fedora / RHEL

Built with `cargo generate-rpm` from `[package.metadata.generate-rpm]` in `Cargo.toml`
(see `packaging/fedora/README.md`):

```sh
cargo install --locked cargo-generate-rpm
cargo build --release && cargo generate-rpm   # -> target/generate-rpm/muninn-<ver>-1.x86_64.rpm
sudo dnf install ./target/generate-rpm/muninn-*.x86_64.rpm
```

Uninstall: `sudo dnf remove muninn`.
