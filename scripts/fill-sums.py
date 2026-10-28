#!/usr/bin/env python3
"""Write release-ready packaging files with real versions and SHA-256 sums.

Usage: scripts/fill-sums.py <version> <dist-dir>
Reads <dist-dir>/SHA256SUMS and writes into <dist-dir>/packaging/:
  arch/PKGBUILD (muninn-bin), homebrew/muninn.rb, winget/<version>/*.yaml
"""
import pathlib, re, sys

ver, dist = sys.argv[1], pathlib.Path(sys.argv[2])
root = pathlib.Path(__file__).resolve().parent.parent
sums = {}
for line in (dist / "SHA256SUMS").read_text().splitlines():
    h, name = line.split(maxsplit=1)
    sums[name.lstrip("*")] = h
out = dist / "packaging"

def sha(target, ext="tar.gz"):
    return sums[f"muninn-{ver}-{target}.{ext}"]

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
s = (root / "packaging/homebrew/muninn.rb").read_text()
s = re.sub(r'version ".*?"', f'version "{ver}"', s, count=1)
lines = s.splitlines()
for i, l in enumerate(lines):
    m = re.search(r"muninn-#\{version\}-([\w-]+)\.tar\.gz", l)
    if m:
        lines[i + 1] = re.sub(r'sha256 ".*"', f'sha256 "{sha(m.group(1))}"', lines[i + 1])
write("homebrew/muninn.rb", "\n".join(lines) + "\n")

# winget: newest template directory, retargeted to this version
tdir = sorted((root / "packaging/winget/manifests/k/Kommandosaldat/Muninn").iterdir(),
              key=lambda p: [int(x) for x in p.name.split(".")])[-1]
for f in sorted(tdir.glob("*.yaml")):
    s = f.read_text().replace(tdir.name, ver)
    if "installer" in f.name:
        for arch in ("x86_64", "aarch64"):
            t = f"{arch}-pc-windows-gnullvm"
            s = re.sub(rf"({re.escape(t)}\.zip\n\s+InstallerSha256: )\"?[0-9a-fA-F]+\"?",
                       rf'\g<1>"{sha(t, "zip").upper()}"', s)
        s = re.sub(r'ReleaseDate: .*', f'ReleaseDate: "{__import__("datetime").date.today()}"', s)
    write(f"winget/{ver}/{f.name}", s)
