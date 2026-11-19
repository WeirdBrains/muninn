#!/usr/bin/env python3
"""Write release-ready packaging files with real versions and SHA-256 sums.

Usage: scripts/fill-sums.py [<version>] <dist-dir>
(the version defaults to [workspace.package] in Cargo.toml, via scripts/version.sh)
Reads <dist-dir>/SHA256SUMS and writes into <dist-dir>/packaging/:
  arch/PKGBUILD (muninn-bin), arch/muninn-app-bin/PKGBUILD, homebrew/Formula/muninn.rb,
  homebrew/Casks/muninn.rb, homebrew/Casks/muninn-app.rb,
  winget/manifests/w/WeirdBrains/muninn/tui/<version>/*.yaml (ready for winget-pkgs)
The TUI Linux and macOS tarballs must be listed (else it fails, naming them). The TUI
macOS .dmg, the Flutter muninn-app Linux tarballs and .dmg, and the Windows setup.exe
are built on other machines and may be missing: then that recipe, cask or the winget
manifests are skipped with a warning.
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

ZEROS = "0" * 64

def sub1(pattern, repl, text, what, flags=re.M):
    """Replace exactly one match, or fail: a template that drifted must not silently
    keep its placeholder. Patterns are anchored to whole stanza lines (never a comment)."""
    new, n = re.subn(pattern, repl, text, flags=flags)
    if n != 1:
        sys.exit(f"fill-sums: expected exactly one {what} line, found {n}")
    return new

def set_version(text, ver):
    return sub1(r'^(\s*)version ".*?"[ \t]*$', rf'\g<1>version "{ver}"', text, "version")

def set_cask_sha(text, h):
    # only the real stanza: a line that is nothing but `sha256 "<hex>"` (comments start with #)
    return sub1(r'^(\s*)sha256 "[0-9a-fA-F]{64}"[ \t]*$', rf'\g<1>sha256 "{h}"', text, "sha256")

def write(rel, text):
    if ZEROS in text or "SKIP" in re.sub(r"(?m)^\s*#.*$", "", text):
        sys.exit(f"fill-sums: {rel} still has a placeholder (all-zero sha or SKIP) after filling")
    p = out / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)
    print(p)

# Arch: muninn-bin (TUI/CLI). The Linux TUI tarballs are required.
s = (root / "packaging/arch/muninn-bin/PKGBUILD").read_text()
s = sub1(r"^pkgver=.*$", f"pkgver={ver}", s, "pkgver")
s = sub1(r"^sha256sums_x86_64=.*$", f"sha256sums_x86_64=('{sha('x86_64-unknown-linux-gnu')}')", s, "sha256sums_x86_64")
s = sub1(r"^sha256sums_aarch64=.*$", f"sha256sums_aarch64=('{sha('aarch64-unknown-linux-gnu')}')", s, "sha256sums_aarch64")
write("arch/PKGBUILD", s)

# Arch: muninn-app-bin (Flutter desktop). Optional until both Linux app tarballs are in SHA256SUMS.
app_x = optional(f"muninn-app-{ver}-x86_64-unknown-linux-gnu.tar.gz", "the muninn-app-bin PKGBUILD")
app_a = optional(f"muninn-app-{ver}-aarch64-unknown-linux-gnu.tar.gz", "the muninn-app-bin PKGBUILD") if app_x else None
if app_x and app_a:
    s = (root / "packaging/arch/muninn-app-bin/PKGBUILD").read_text()
    s = sub1(r"^pkgver=.*$", f"pkgver={ver}", s, "pkgver")
    s = sub1(r"^sha256sums_x86_64=.*$", f"sha256sums_x86_64=('{app_x}')", s, "sha256sums_x86_64")
    s = sub1(r"^sha256sums_aarch64=.*$", f"sha256sums_aarch64=('{app_a}')", s, "sha256sums_aarch64")
    write("arch/muninn-app-bin/PKGBUILD", s)

# Homebrew: each url line is followed by its sha256 line
s = (root / "packaging/homebrew/Formula/muninn.rb").read_text()
s = set_version(s, ver)
lines = s.splitlines()
filled = 0
for i, l in enumerate(lines):
    m = re.match(r'\s*url "#\{base\}/muninn-#\{version\}-([\w-]+)\.tar\.gz"\s*$', l)
    if m:
        lines[i + 1] = set_cask_sha(lines[i + 1], sha(m.group(1)))
        filled += 1
if filled != 4:
    sys.exit(f"fill-sums: expected 4 url/sha256 pairs in the formula, found {filled}")
write("homebrew/Formula/muninn.rb", "\n".join(lines) + "\n")

# Homebrew cask: the universal TUI Muninn.app dmg
dmg = optional(f"muninn-{ver}-macos.dmg", "the Homebrew cask")
if dmg:
    s = (root / "packaging/homebrew/Casks/muninn.rb").read_text()
    s = set_cask_sha(set_version(s, ver), dmg)
    write("homebrew/Casks/muninn.rb", s)

# Homebrew cask: the Flutter Muninn.app dmg from scripts/flutter-dist.sh
app_dmg = optional(f"muninn-app-{ver}-universal-apple-darwin.dmg", "the Homebrew muninn-app cask")
if app_dmg:
    s = (root / "packaging/homebrew/Casks/muninn-app.rb").read_text()
    s = set_cask_sha(set_version(s, ver), app_dmg)
    write("homebrew/Casks/muninn-app.rb", s)

# winget: newest template directory, retargeted to this version; Inno Setup installers
wroot = "winget/manifests/w/WeirdBrains/muninn/tui"
tdir = sorted((root / "packaging" / wroot).iterdir(),
              key=lambda p: [int(x) for x in p.name.split(".")])[-1]
setups = {a: optional(f"muninn-{ver}-{a}-windows-setup.exe", "the winget manifests") for a in ("x86_64", "aarch64")}
for f in sorted(tdir.glob("*.yaml")) if all(setups.values()) else []:
    s = f.read_text().replace(tdir.name, ver)
    if "installer" in f.name:
        for arch in ("x86_64", "aarch64"):
            n = f"muninn-{ver}-{arch}-windows-setup.exe"
            s = sub1(rf"^(\s*InstallerUrl: \S*{re.escape(n)}\n\s+InstallerSha256: )\"?[0-9a-fA-F]+\"?[ \t]*$",
                     rf'\g<1>"{setups[arch].upper()}"', s, f"InstallerSha256 for {n}")
        s = re.sub(r'ReleaseDate: .*', f'ReleaseDate: "{__import__("datetime").date.today()}"', s)
    write(f"{wroot}/{ver}/{f.name}", s)
