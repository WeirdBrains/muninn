# Install

Every package ships a prebuilt `mnnn` binary: **no Rust or other toolchain is needed to
install**. SQLCipher and OpenSSL are compiled into the binary, so the only runtime
dependencies are the C runtime: glibc and libgcc (Linux), the system libraries (macOS),
nothing extra (Windows). Downloads are on the GitHub Releases page with a `SHA256SUMS`
file. Packaging files live in `packaging/`.

> The prebuilt downloads (release tarballs, `muninn-bin`, Homebrew, winget) only work
> after a release has been published by the release workflow and is publicly
> downloadable. Until then use the Arch checkout build below or *From source*.

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

**From a checkout (works today, needs Rust):**

```sh
git clone git@github.com:kommandosaldat/muninn.git && cd muninn/packaging/arch
makepkg -si            # builds the committed checkout, installs package "muninn"
```

**Prebuilt (`muninn-bin`, no Rust):** only once a GitHub release with assets exists
and can be downloaded without logging in (public repo):

```sh
cd packaging/arch/muninn-bin && makepkg -si
```

Each release also attaches ready-made packages for both architectures,
`muninn-bin-<ver>-1-x86_64.pkg.tar.zst` and `muninn-bin-<ver>-1-aarch64.pkg.tar.zst`
(listed in `SHA256SUMS`); install one with `sudo pacman -U <file>`.

Both install `/usr/bin/mnnn`, man pages and `/usr/share/doc/muninn`. Uninstall:
`sudo pacman -R muninn` (or `muninn-bin`). Remove an earlier `cargo install` first
(`cargo uninstall muninn`), or `~/.cargo/bin/mnnn` may shadow the package, depending on
your `PATH`. `packaging/arch/build-local.sh [--source]` builds the package into
`target/pkg/arch/` without installing (`sudo pacman -U target/pkg/arch/muninn*.pkg.tar.zst`).

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

## Windows (winget)

Release zips hold a portable `mnnn.exe` for x64 and arm64
(`muninn-<ver>-<arch>-pc-windows-gnullvm.zip`). Manifests (schema 1.9) are in
`packaging/winget/manifests/`:

```powershell
winget install Kommandosaldat.Muninn              # once accepted into winget-pkgs
winget install --manifest packaging\winget\manifests\k\Kommandosaldat\Muninn\<ver>
```

Or unzip anywhere and put `mnnn.exe` on `PATH`. Uninstall: `winget uninstall
Kommandosaldat.Muninn` (or delete the folder). Data lives in `%USERPROFILE%\.muninn`.

## From source (any platform)

Needs Rust >= 1.99, a C compiler, `perl` and `make`:

```sh
cargo install --locked --path .
```

Uninstall: `cargo uninstall muninn`.
