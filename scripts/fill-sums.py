#!/usr/bin/env python3
"""Write release-ready packaging files with real versions and SHA-256 sums.

Usage: scripts/fill-sums.py [<version>] <dist-dir>
(the version defaults to [workspace.package] in Cargo.toml, via scripts/version.sh)
Reads <dist-dir>/SHA256SUMS and writes into <dist-dir>/packaging/:
  arch/PKGBUILD (muninn-bin), homebrew/Formula/muninn.rb, homebrew/Casks/muninn.rb,
  homebrew/Casks/muninn-app.rb,
  winget/manifests/w/WeirdBrains/Muninn/<version>/*.yaml (ready for winget-pkgs)
The Linux and macOS tarballs must be listed (else it fails, naming them). The TUI
macOS .dmg, the Flutter muninn-app .dmg and the Windows setup.exe are built on
other machines and may be missing: then that cask or the winget manifests are
skipped with a warning.
"""
import pathlib, re, subprocess, sys

root = pathlib.Path(__file__).resolve().parent.parent
if len(sys.argv) == 2:
    ver = subprocess.check_output([root / "scripts/version.sh"], text=True).strip()
else:
    ver = sys.argv[1]
dist = pathlib.Path(sys.argv[-1])
sums = {}
for line in (dist / "SHA256SUMS").read_text().splitlines():
    h, name = line.split(maxsplit=1)
    sums[name.lstrip("*")] = h
out = dist / "packaging"

def sha(target, ext="tar.gz"):
    n = f"muninn-{ver}-{target}.{ext}"
    if n not in sums:
        sys.exit(f"fill-sums: {n} is not in {dist / 'SHA256SUMS'}")
    return sums[n]

def optional(name, skipping):
    """The sum for an asset built elsewhere, or None with a warning."""
    if name not in sums:
        print(f"warning: {name} is not in SHA256SUMS; skipping {skipping}", file=sys.stderr)
    return sums.get(name)

def write(rel, text):
    p = out / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)
    print(p)

# Arch
s = (root / "packaging/arch/muninn-bin/PKGBUILD").read_text()
s = re.sub(r"^pkgver=.*$", f"pkgver={ver}", s, flags=re.M)
s = re.sub(r"^sha256sums_x86_64=.*$", f"sha256sums_x86_64=('{sha('x86_64-unknown-linux-gnu')}')", s, flags=re.M)
s = re.sub(r"^sha256sums_aarch64=.*$", f"sha256sums_aarch64=('{sha('aarch64-unknown-linux-gnu')}')", s, flags=re.M)
write("arch/PKGBUILD", s)

# Homebrew: each url line is followed by its sha256 line
s = (root / "packaging/homebrew/Formula/muninn.rb").read_text()
s = re.sub(r'version ".*?"', f'version "{ver}"', s, count=1)
lines = s.splitlines()
for i, l in enumerate(lines):
    m = re.search(r"muninn-#\{version\}-([\w-]+)\.tar\.gz", l)
    if m:
        lines[i + 1] = re.sub(r'sha256 ".*"', f'sha256 "{sha(m.group(1))}"', lines[i + 1])
write("homebrew/Formula/muninn.rb", "\n".join(lines) + "\n")

# Homebrew cask: the universal TUI Muninn.app dmg
dmg = optional(f"muninn-{ver}-macos.dmg", "the Homebrew cask")
if dmg:
    s = (root / "packaging/homebrew/Casks/muninn.rb").read_text()
    s = re.sub(r'version ".*?"', f'version "{ver}"', s, count=1)
    s = re.sub(r'sha256 ".*?"', f'sha256 "{dmg}"', s, count=1)
    write("homebrew/Casks/muninn.rb", s)

# Homebrew cask: the Flutter Muninn.app dmg from scripts/flutter-dist.sh
app_dmg = optional(f"muninn-app-{ver}-universal-apple-darwin.dmg", "the Homebrew muninn-app cask")
if app_dmg:
    s = (root / "packaging/homebrew/Casks/muninn-app.rb").read_text()
    s = re.sub(r'version ".*?"', f'version "{ver}"', s, count=1)
    s = re.sub(r'sha256 ".*?"', f'sha256 "{app_dmg}"', s, count=1)
    write("homebrew/Casks/muninn-app.rb", s)

# winget: newest template directory, retargeted to this version; Inno Setup installers
wroot = "winget/manifests/w/WeirdBrains/Muninn"
tdir = sorted((root / "packaging" / wroot).iterdir(),
              key=lambda p: [int(x) for x in p.name.split(".")])[-1]
setups = {a: optional(f"muninn-{ver}-{a}-windows-setup.exe", "the winget manifests") for a in ("x86_64", "aarch64")}
for f in sorted(tdir.glob("*.yaml")) if all(setups.values()) else []:
    s = f.read_text().replace(tdir.name, ver)
    if "installer" in f.name:
        for arch in ("x86_64", "aarch64"):
            n = f"muninn-{ver}-{arch}-windows-setup.exe"
            s = re.sub(rf"({re.escape(n)}\n\s+InstallerSha256: )\"?[0-9a-fA-F]+\"?",
                       rf'\g<1>"{setups[arch].upper()}"', s)
        s = re.sub(r'ReleaseDate: .*', f'ReleaseDate: "{__import__("datetime").date.today()}"', s)
    write(f"{wroot}/{ver}/{f.name}", s)
