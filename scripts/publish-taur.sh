#!/usr/bin/env bash
# Publish muninn-bin (the prebuilt TUI/CLI Arch package) to TAUR after a GitHub release exists.
# Usage: scripts/publish-taur.sh [--dry-run] [--out PATH] [VERSION] [PACKAGING_TAR_OR_DIR]
#
# This is a post-release step, not part of scripts/release.sh (which only bumps, commits
# and tags, before checksums exist). VERSION defaults to [workspace.package] via
# scripts/version.sh. The optional path is muninn-<ver>-packaging.tar.gz, or a
# dist/packaging directory that already contains arch/PKGBUILD from fill-sums.py.
# With no path, the packaging tarball is downloaded from the vVERSION GitHub release.
#
# Checks the PKGBUILD sums against the release's SHA256SUMS and the downloaded tarballs,
# builds a recipe archive with PKGBUILD and .SRCINFO, then runs:
#   taurctl pkg upload ARCHIVE --org WeirdBrains
# An upload needs the release published (not a draft) and skips a version TAUR already
# has. Sign in first with `taurctl login` if needed. --dry-run builds and checks the
# archive without uploading. Never pushes git; never reads or prints session tokens.
set -euo pipefail
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

usage() { sed -n '2,17s/^# //p;2,17s/^#$//p' "$0"; }

dry=0
out=""
args=()
while (($#)); do
  case "$1" in
    --dry-run) dry=1 ;;
    --out) out="${2:?--out needs a path}"; shift ;;
    -h|--help) usage; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) args+=("$1") ;;
  esac
  shift
done
ver="${args[0]:-$(scripts/version.sh)}"
src="${args[1]:-}"
repo=WeirdBrains/muninn
org=WeirdBrains
pkg=muninn-bin
arches=(x86_64 aarch64)

workdir=$(mktemp -d)
cleanup() { rm -rf "$workdir"; }
trap cleanup EXIT
assets="$workdir/assets"
mkdir -p "$assets"

# Order: the release must exist with its assets before anything is uploaded.
draft=$(gh release view "v$ver" --repo "$repo" --json isDraft --jq .isDraft) || {
  echo "release v$ver not found on $repo; publish the GitHub release first" >&2; exit 1; }
if [[ "$draft" != "false" ]]; then
  if [[ "$dry" -eq 1 ]]; then
    echo "warning: release v$ver is still a draft; an upload would be refused" >&2
  else
    echo "release v$ver is still a draft; publish it before uploading to TAUR" >&2; exit 1
  fi
fi

pkgbuild=""
if [[ -n "$src" && -d "$src" ]]; then
  if [[ -f "$src/arch/PKGBUILD" ]]; then
    pkgbuild="$src/arch/PKGBUILD"
  elif [[ -f "$src/PKGBUILD" ]]; then
    pkgbuild="$src/PKGBUILD"
  else
    echo "no arch/PKGBUILD in directory $src" >&2
    exit 1
  fi
  cp "$pkgbuild" "$workdir/PKGBUILD"
elif [[ -n "$src" && -f "$src" ]]; then
  tar -C "$workdir" -xzf "$src"
  if [[ -f "$workdir/packaging/arch/PKGBUILD" ]]; then
    mv "$workdir/packaging/arch/PKGBUILD" "$workdir/PKGBUILD"
  elif [[ -f "$workdir/arch/PKGBUILD" ]]; then
    mv "$workdir/arch/PKGBUILD" "$workdir/PKGBUILD"
  else
    echo "$src does not contain packaging/arch/PKGBUILD" >&2
    exit 1
  fi
  rm -rf "$workdir/packaging" "$workdir/arch"
elif [[ -n "$src" ]]; then
  echo "no such file or directory: $src" >&2
  exit 1
else
  echo "downloading muninn-$ver-packaging.tar.gz from GitHub release v$ver" >&2
  gh release download "v$ver" --repo "$repo" --pattern "muninn-$ver-packaging.tar.gz" --dir "$assets"
  tar -C "$workdir" -xzf "$assets/muninn-$ver-packaging.tar.gz"
  mv "$workdir/packaging/arch/PKGBUILD" "$workdir/PKGBUILD"
  rm -rf "$workdir/packaging"
fi

# The release's own checksums and tarballs: the recipe must describe exactly these.
gh release download "v$ver" --repo "$repo" --pattern SHA256SUMS --dir "$assets"
for a in "${arches[@]}"; do
  gh release download "v$ver" --repo "$repo" --pattern "muninn-$ver-$a-unknown-linux-gnu.tar.gz" --dir "$assets"
done
(cd "$assets" && grep -E " \*?muninn-$ver-(x86_64|aarch64)-unknown-linux-gnu\.tar\.gz$" SHA256SUMS | sha256sum -c --quiet -) || {
  echo "release tarballs do not match the release's SHA256SUMS" >&2; exit 1; }

# Host-independent source URLs: $CARCH would expand to the build host in .SRCINFO.
python3 - "$workdir/PKGBUILD" "$ver" "$assets/SHA256SUMS" <<'PY'
import pathlib, re, sys
p, ver, sums = pathlib.Path(sys.argv[1]), sys.argv[2], pathlib.Path(sys.argv[3]).read_text()
s = p.read_text()
s2, n = re.subn(
    r'^(source_(x86_64|aarch64)=\(".*?muninn-\$pkgver-)\$CARCH(-unknown-linux-gnu\.tar\.gz"\))$',
    r'\1\2\3',
    s,
    flags=re.M,
)
if n:
    p.write_text(s2)
    s = s2
for arch in ("x86_64", "aarch64"):
    if not re.search(rf'^source_{arch}=.*muninn-\$pkgver-{arch}-unknown-linux-gnu\.tar\.gz', s, re.M):
        sys.exit(f"PKGBUILD source_{arch} must use a literal {arch} tarball name, not $CARCH")
if not re.search(r'^pkgname=muninn-bin$', s, re.M):
    sys.exit("refusing to publish a recipe whose pkgname is not muninn-bin")
if re.search(r"^sha256sums_(x86_64|aarch64)=\('SKIP'\)$", s, re.M):
    sys.exit("PKGBUILD still has SKIP checksums; use the filled packaging tarball from a published release")
got = re.search(r'^pkgver=(.+)$', s, re.M).group(1)
if got != ver:
    sys.exit(f"PKGBUILD pkgver={got}, expected {ver}")
want = {}
for line in sums.splitlines():
    parts = line.split()
    if len(parts) == 2:
        want[parts[1].lstrip("*")] = parts[0].lower()
for arch in ("x86_64", "aarch64"):
    m = re.search(rf"^sha256sums_{arch}=\('([0-9a-fA-F]{{64}})'\)$", s, re.M)
    name = f"muninn-{ver}-{arch}-unknown-linux-gnu.tar.gz"
    if not m:
        sys.exit(f"PKGBUILD sha256sums_{arch} is not one sha256")
    if want.get(name) != m.group(1).lower():
        sys.exit(f"PKGBUILD sha256sums_{arch} does not match {name} in the release SHA256SUMS")
print(f"recipe pkgver={ver}: checksums match the release", file=sys.stderr)
PY
rel=$(sed -n 's/^pkgrel=//p' "$workdir/PKGBUILD")

recipedir="$workdir/$pkg"
mkdir -p "$recipedir"
mv "$workdir/PKGBUILD" "$recipedir/PKGBUILD"
(cd "$recipedir" && makepkg --printsrcinfo > .SRCINFO)
python3 - "$recipedir/.SRCINFO" "$recipedir/PKGBUILD" "$ver" "$rel" <<'PY'
import pathlib, re, sys
src = pathlib.Path(sys.argv[1]).read_text()
pb = pathlib.Path(sys.argv[2]).read_text()
ver, rel = sys.argv[3], sys.argv[4]
fields = {}
for line in src.splitlines():
    if " = " in line:
        k, v = line.strip().split(" = ", 1)
        fields.setdefault(k, []).append(v)
if fields.get("pkgbase") != ["muninn-bin"] or fields.get("pkgname") != ["muninn-bin"]:
    sys.exit(f".SRCINFO pkgbase/pkgname mismatch: {fields.get('pkgbase')} {fields.get('pkgname')}")
if fields.get("pkgver") != [ver] or fields.get("pkgrel") != [rel]:
    sys.exit(f".SRCINFO version mismatch: {fields.get('pkgver')}-{fields.get('pkgrel')} (expected {ver}-{rel})")
for arch in ("x86_64", "aarch64"):
    urls = fields.get(f"source_{arch}", [])
    if len(urls) != 1 or not urls[0].endswith(f"/v{ver}/muninn-{ver}-{arch}-unknown-linux-gnu.tar.gz"):
        sys.exit(f".SRCINFO source_{arch} is not the {arch} tarball: {urls}")
    want = re.search(rf"^sha256sums_{arch}=\('([0-9a-fA-F]{{64}})'\)$", pb, re.M).group(1)
    if fields.get(f"sha256sums_{arch}") != [want]:
        sys.exit(f".SRCINFO sha256sums_{arch} does not match the PKGBUILD")
PY

archive="$workdir/$pkg-$ver-$rel.src.tar.gz"
tar -C "$workdir" --owner=0 --group=0 --numeric-owner -czf "$archive" "$pkg/PKGBUILD" "$pkg/.SRCINFO"
listing=$(tar -tzf "$archive" | LC_ALL=C sort | tr '\n' ' ')
[[ "$listing" == "$pkg/.SRCINFO $pkg/PKGBUILD " ]] || { echo "unexpected archive contents: $listing" >&2; exit 1; }
echo "recipe archive $pkg-$ver-$rel.src.tar.gz sha256 $(sha256sum "$archive" | cut -d' ' -f1)" >&2

if [[ "$dry" -eq 1 ]]; then
  dest="${out:-${TMPDIR:-/tmp}/$pkg-$ver-$rel.src.tar.gz}"
  cp -f "$archive" "$dest"
  echo "dry-run: wrote $dest"
  echo "dry-run: would run: taurctl pkg upload $dest --org $org -y"
  exit 0
fi

# What TAUR has now (public RPC, no secrets): same org, and not this version already.
info=$(taurctl info "$pkg" --json)
state=$(python3 - "$info" "$org" "$ver-$rel" <<'PY'
import json, sys
info, org, want = json.loads(sys.argv[1]), sys.argv[2], sys.argv[3]
res = [r for r in info.get("results", []) if r.get("Name") == "muninn-bin"]
if not res:
    print("new")
elif res[0].get("Organization") != org:
    print("org:" + str(res[0].get("Organization")))
elif res[0].get("Version") == want:
    print("same")
else:
    print("update")
PY
)
case "$state" in
  same) echo "TAUR already has $pkg $ver-$rel; nothing to upload"; exit 0 ;;
  org:*) echo "TAUR $pkg belongs to ${state#org:}, not $org; not uploading" >&2; exit 1 ;;
esac

# -y: non-interactive confirmation. Session comes from the signed-in taurctl config.
taurctl pkg upload "$archive" --org "$org" -y
