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
