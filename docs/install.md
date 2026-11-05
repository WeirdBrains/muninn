# Install

Every package ships a prebuilt `mnnn` binary: **no Rust or other toolchain is needed to
install**. SQLCipher and OpenSSL are compiled into the binary, so the only runtime
dependencies are the C runtime: glibc 2.39 or newer and libgcc (Linux: Ubuntu 24.04+,
Debian 13+, Fedora 40+, Arch; older systems such as Ubuntu 22.04 or Debian 12 need a
[source build](#from-source-any-platform)), the system libraries (macOS),
nothing extra (Windows). Downloads are on the GitHub Releases page with a `SHA256SUMS`
file. Packaging files live in `packaging/`.

Building code spaces is optional and needs extra tools that the packages do not bundle:
`cargo` (from [rustup](https://rustup.rs)) for math, Polars, Rust and Python; Python is
translated in process, so no Python install is needed. See
[code spaces](code-spaces.md#python-in-rust).

## Quick install

| System | Easiest way |
|---|---|
| Windows 10/11 (x64, arm64) | [`setup.exe`](#windows-setupexe), or [`winget`](#windows-winget) |
| macOS (Apple silicon, Intel) | [`.dmg` with Muninn.app](#macos-dmg), or [Homebrew](#homebrew-macos-and-linux) |
| Ubuntu 24.04+, Debian 13+ | [`.deb`](#debian--ubuntu) |
| Fedora, RHEL | [`.rpm`](#fedora--rhel) |
| Arch Linux | [`muninn-bin`](#arch-linux) |
| any other Linux | [Homebrew](#homebrew-macos-and-linux) or the [tarball](#release-tarball-any-linux-or-macos) |

### Windows: setup.exe

1. Download `muninn-2.21.0-x86_64-windows-setup.exe` (on an ARM PC,
   `muninn-2.21.0-aarch64-windows-setup.exe`) from the
   [v2.21.0 release](https://github.com/WeirdBrains/muninn/releases/tag/v2.21.0).
2. Run it. It is not code-signed, so Windows SmartScreen says "Windows protected your PC":
   click **More info**, then **Run anyway**.
3. Click through the wizard. It installs for your user only (no admin prompt) into
   `%LOCALAPPDATA%\Programs\Muninn`, adds that folder to your user `PATH`, and adds a
   **Muninn** Start Menu entry that opens the TUI (setting up keys the first time).

Open a **new** terminal (old ones keep the old `PATH`) and run `mnnn --version`.
Silent install, e.g. for scripts: `muninn-2.21.0-x86_64-windows-setup.exe /VERYSILENT /SUPPRESSMSGBOXES`.

**Windows Firewall:** the first time Muninn starts its network node, Windows Defender
Firewall asks whether `mnnn.exe` may communicate on private and public networks. That is
expected: the node listens for your other nodes and connections. **Allow access** (private
networks are enough at home) for peer-to-peer sync, sharing and collaboration. **Cancel**
keeps Muninn local-only: ingest, search, charts, journals, models, reports and exports all
still work. The firewall only blocks incoming connections, so other nodes can't reach this
one directly; connections it makes itself (for example through a relay) still go out.
Change your answer later in **Windows Security ▸ Firewall & network protection ▸ Allow an
app through firewall**.

Uninstall from **Settings ▸ Apps ▸ Installed apps ▸ Muninn ▸ Uninstall** (or Start Menu ▸
Muninn ▸ Uninstall Muninn). That removes the program folder and its `PATH` entry; your
data in `%USERPROFILE%\.muninn` stays. Windows keeps the firewall entry for `mnnn.exe`;
remove it under **Allow an app through firewall** (Change settings, select `mnnn`, Remove)
if you like.

### Windows: winget

```powershell
winget install WeirdBrains.Muninn
```

This runs the same `setup.exe` silently. Muninn's entry was submitted to the public winget
catalogue with v2.21.0; until Microsoft merges it, winget reports "No package found", so
use `setup.exe` meanwhile. Uninstall: `winget uninstall WeirdBrains.Muninn`.

### macOS: dmg

1. Download `muninn-2.21.0-macos.dmg` from the
   [v2.21.0 release](https://github.com/WeirdBrains/muninn/releases/tag/v2.21.0). One
   universal app runs on Apple silicon and Intel.
2. Open the dmg and drag **Muninn** onto **Applications**.
3. Muninn.app is not notarized, so the first time, **right-click** (or Control-click)
   Muninn in Applications and choose **Open**, then **Open** again. (On macOS 15 and newer,
   if there is no Open button: try to open it once, then go to **System Settings ▸ Privacy &
   Security** and click **Open Anyway**.) Or in Terminal:
   `xattr -dr com.apple.quarantine /Applications/Muninn.app`.

Muninn opens Terminal running `mnnn`. The first time it offers to put the `mnnn` command on
your `PATH` (`/usr/local/bin`, or `~/.local/bin`) and sets up keys, then starts the TUI.
Uninstall: drag Muninn from Applications to the Bin, and remove the `mnnn` link it made
(`/usr/local/bin/mnnn` or `~/.local/bin/mnnn`). Your data in `~/.muninn` stays.

### Homebrew (macOS and Linux)

```sh
brew install weirdbrains/tap/muninn            # the mnnn command (macOS or Linux, arm64 or x86_64)
brew install --cask weirdbrains/tap/muninn     # or Muninn.app plus mnnn (macOS)
```

Install one or the other, not both. Both come from the
[WeirdBrains/homebrew-tap](https://github.com/WeirdBrains/homebrew-tap) tap and are updated
with each release (`brew upgrade`). The cask's app is not notarized; if macOS blocks it, see
step 3 under [macOS: dmg](#macos-dmg). Uninstall: `brew uninstall muninn` (or
`brew uninstall --cask muninn`), then `brew untap weirdbrains/tap` if you like.

## Getting a release

Each release is built and checked locally, and its files are attached to its page under
[GitHub Releases](https://github.com/WeirdBrains/muninn/releases). The latest is
[v2.21.0](https://github.com/WeirdBrains/muninn/releases/tag/v2.21.0). Every release has
17 files:

| File (v2.21.0) | For |
|---|---|
| `muninn-2.21.0-x86_64-unknown-linux-gnu.tar.gz`, `muninn-2.21.0-aarch64-unknown-linux-gnu.tar.gz` | any Linux |
| `muninn-2.21.0-x86_64-apple-darwin.tar.gz`, `muninn-2.21.0-aarch64-apple-darwin.tar.gz` | macOS (Intel, Apple silicon) |
| `muninn_2.21.0-1_amd64.deb`, `muninn_2.21.0-1_arm64.deb` | Debian, Ubuntu |
| `muninn-2.21.0-1.x86_64.rpm`, `muninn-2.21.0-1.aarch64.rpm` | Fedora, RHEL |
| `muninn-bin-2.21.0-1-x86_64.pkg.tar.zst`, `muninn-bin-2.21.0-1-aarch64.pkg.tar.zst` | Arch Linux |
| `muninn-2.21.0-x86_64-windows-setup.exe`, `muninn-2.21.0-aarch64-windows-setup.exe` | Windows installer (x64, arm64) |
| `muninn-2.21.0-macos.dmg` | macOS Muninn.app (universal) |
| `muninn-2.21.0-x86_64-pc-windows-gnullvm.zip`, `muninn-2.21.0-aarch64-pc-windows-gnullvm.zip` | Windows, portable (x64, arm64) |
| `muninn-2.21.0-packaging.tar.gz` | the Homebrew formula and cask, Arch PKGBUILD and winget manifests, with this release's checksums filled in |
| `SHA256SUMS` | checksums for all of the above |

Download from the command line and check it, for example:

```sh
ver=2.21.0
base=https://github.com/WeirdBrains/muninn/releases/download/v$ver
curl -LO $base/muninn-$ver-x86_64-unknown-linux-gnu.tar.gz -LO $base/SHA256SUMS
sha256sum --ignore-missing -c SHA256SUMS      # macOS: shasum -a 256 --ignore-missing -c SHA256SUMS
```

On Windows (PowerShell) see [Windows (portable zip)](#windows-portable-zip) below.

(`gh release download v2.21.0` does the same if you have the GitHub CLI.) Muninn is
BSD-3-Clause licensed; every package carries the `LICENSE`.

Installing or uninstalling never touches your data in `~/.muninn` (or `MUNINN_HOME`);
delete that folder yourself to remove it.

## Release tarball (any Linux or macOS)

```sh
t=x86_64-unknown-linux-gnu     # or aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin
tar -xzf muninn-2.21.0-$t.tar.gz
mkdir -p ~/.local/bin ~/.local/share/man/man1
install -m755 muninn-2.21.0-$t/mnnn ~/.local/bin/mnnn
cp muninn-2.21.0-$t/man/*.1 ~/.local/share/man/man1/
```

If `mnnn` is not found afterwards, `~/.local/bin` is not on your `PATH` (the default on
macOS and on some Linux images): add `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc`
or `~/.bashrc` and open a new terminal.

macOS: the binary is not notarized. Downloaded with `curl` it runs as is; downloaded in a
browser, Gatekeeper blocks it ("cannot be opened"), so clear the flag first:
`xattr -d com.apple.quarantine muninn-2.21.0-$t.tar.gz` (before extracting) or
`xattr -d com.apple.quarantine ~/.local/bin/mnnn`.

Uninstall: remove `~/.local/bin/mnnn` and `~/.local/share/man/man1/mnnn*.1`.

## Arch Linux

**From a checkout (works today, needs Rust):**

```sh
git clone git@github.com:WeirdBrains/muninn.git && cd muninn/packaging/arch
makepkg -si            # builds the committed checkout, installs package "muninn"
```

**Prebuilt (`muninn-bin`, no Rust):** install the release's ready-made package:

```sh
sudo pacman -U muninn-bin-2.21.0-1-x86_64.pkg.tar.zst     # or …-aarch64.pkg.tar.zst
```

Or build it yourself from the release's PKGBUILD (`packaging/arch/PKGBUILD` in
`muninn-2.21.0-packaging.tar.gz`, which has the checksums filled in) with `makepkg -si`. If
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
sudo apt install ./muninn_2.21.0-1_amd64.deb      # arm64: muninn_2.21.0-1_arm64.deb
```

Needs Ubuntu 24.04+ or Debian 13+ (glibc 2.39); on Ubuntu 22.04 it installs but
`mnnn` fails with ``GLIBC_2.38' not found``. Uninstall: `sudo apt remove muninn`.
Maintainers build it with `cargo deb` (`packaging/debian/README.md`).

## Fedora / RHEL

```sh
sudo dnf install ./muninn-2.21.0-1.x86_64.rpm     # or muninn-2.21.0-1.aarch64.rpm
```

Requires glibc (detected from the binary). Uninstall: `sudo dnf remove muninn`.
Maintainers build it with `cargo generate-rpm` (`packaging/fedora/README.md`).

## Windows (portable zip)

The [setup.exe](#windows-setupexe) is simpler. Release zips hold a portable `mnnn.exe` for x64 and arm64. In PowerShell (no admin rights
needed), download, check, unpack into `%LOCALAPPDATA%\Programs\Muninn` and add it to your
user `PATH`:

```powershell
$ver = "2.21.0"; $arch = "x86_64"     # or aarch64
$zip = "muninn-$ver-$arch-pc-windows-gnullvm.zip"
$base = "https://github.com/WeirdBrains/muninn/releases/download/v$ver"
curl.exe -LO "$base/$zip" -LO "$base/SHA256SUMS"
(Get-FileHash $zip).Hash -eq ((Select-String $zip SHA256SUMS).Line -split ' ')[0].ToUpper()   # True
$dest = "$env:LOCALAPPDATA\Programs\Muninn"
Expand-Archive $zip "$env:TEMP\muninn" -Force
New-Item -ItemType Directory $dest -Force | Out-Null
Copy-Item "$env:TEMP\muninn\muninn-$ver-$arch-pc-windows-gnullvm\*" $dest -Recurse -Force
$p = [Environment]::GetEnvironmentVariable("Path", "User")
if ($p -notlike "*$dest*") { [Environment]::SetEnvironmentVariable("Path", "$p;$dest", "User") }
```

Open a **new** PowerShell or terminal window (the old one keeps the old `PATH`) and run
`mnnn --version`. Uninstall: delete `%LOCALAPPDATA%\Programs\Muninn` and remove it from
your user `PATH` (Settings ▸ System ▸ About ▸ Advanced system settings ▸ Environment
Variables). Data lives in `%USERPROFILE%\.muninn`.

## From source (any platform)

Needs Rust >= 1.99, a C compiler, `perl` and `make`:

```sh
cargo install --locked --path .
```

Uninstall: `cargo uninstall muninn`.
