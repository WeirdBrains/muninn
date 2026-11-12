#!/usr/bin/env bash
# Publish a prebuilt Arch package recipe to TAUR after a GitHub release exists.
# Usage: scripts/publish-taur.sh [--dry-run] [--out PATH] [--package muninn-bin|muninn-app-bin] [VERSION] [PACKAGING_TAR_OR_DIR]
#
# This is a post-release step, not part of the release cut (scripts/weirdnet_release.py, which bumps, commits
# and tags, before checksums exist). VERSION defaults to [workspace.package] via
# scripts/version.sh. --package selects the recipe (default: muninn-bin, the TUI/CLI).
# muninn-app-bin is the prebuilt Flutter desktop app. The optional path is
# muninn-<ver>-packaging.tar.gz, or a dist/packaging directory that already
# contains the filled PKGBUILD from fill-sums.py (arch/PKGBUILD for muninn-bin,
# arch/muninn-app-bin/PKGBUILD for muninn-app-bin). With no path, the packaging
# tarball is downloaded from the vVERSION GitHub release.
#
# Checks the PKGBUILD sums against the release's SHA256SUMS and the downloaded tarballs,
# builds a recipe archive with PKGBUILD and .SRCINFO, then runs:
#   taurctl pkg upload ARCHIVE --org WeirdBrains
# An upload needs the release published (not a draft) and skips a version TAUR already
# has. Sign in first with `taurctl login` if needed. --dry-run builds and checks the
# archive without uploading. Never pushes git; never reads or prints session tokens.
set -euo pipefail
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

usage() { sed -n '2,20s/^# //p;2,20s/^#$//p' "$0"; }

dry=0
out=""
pkg=muninn-bin
args=()
while (($#)); do
  case "$1" in
    --dry-run) dry=1 ;;
    --out) out="${2:?--out needs a path}"; shift ;;
    --package) pkg="${2:?--package needs muninn-bin or muninn-app-bin}"; shift ;;
    -h|--help) usage; exit 0 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) args+=("$1") ;;
  esac
  shift
done
case "$pkg" in
  muninn-bin) stem=muninn; pkg_rel=packaging/arch/PKGBUILD; pkg_rel_alt=arch/PKGBUILD ;;
  muninn-app-bin) stem=muninn-app; pkg_rel=packaging/arch/muninn-app-bin/PKGBUILD; pkg_rel_alt=arch/muninn-app-bin/PKGBUILD ;;
  *) echo "unknown package: $pkg (use muninn-bin or muninn-app-bin)" >&2; exit 2 ;;
esac
ver="${args[0]:-$(scripts/version.sh)}"
src="${args[1]:-}"
repo=WeirdBrains/muninn
org=WeirdBrains
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

find_pkgbuild() {
  local base=$1
  if [[ -f "$base/$pkg_rel" ]]; then
    echo "$base/$pkg_rel"
  elif [[ -f "$base/$pkg_rel_alt" ]]; then
    echo "$base/$pkg_rel_alt"
  else
    return 1
  fi
}

pkgbuild=""
if [[ -n "$src" && -d "$src" ]]; then
  if pkgbuild=$(find_pkgbuild "$src"); then
    cp "$pkgbuild" "$workdir/PKGBUILD"
  elif [[ "$pkg" == muninn-bin && -f "$src/PKGBUILD" ]]; then
    cp "$src/PKGBUILD" "$workdir/PKGBUILD"
  else
    echo "no $pkg_rel in directory $src" >&2
    exit 1
  fi
elif [[ -n "$src" && -f "$src" ]]; then
  tar -C "$workdir" -xzf "$src"
  if pkgbuild=$(find_pkgbuild "$workdir"); then
    mv "$pkgbuild" "$workdir/PKGBUILD"
  else
    echo "$src does not contain $pkg_rel" >&2
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
  if pkgbuild=$(find_pkgbuild "$workdir"); then
    mv "$pkgbuild" "$workdir/PKGBUILD"
  else
    echo "muninn-$ver-packaging.tar.gz does not contain $pkg_rel" >&2
    exit 1
  fi
  rm -rf "$workdir/packaging"
fi

# The release's own checksums and tarballs: the recipe must describe exactly these.
gh release download "v$ver" --repo "$repo" --pattern SHA256SUMS --dir "$assets"
for a in "${arches[@]}"; do
  gh release download "v$ver" --repo "$repo" --pattern "$stem-$ver-$a-unknown-linux-gnu.tar.gz" --dir "$assets"
done
(cd "$assets" && grep -E " \*?${stem}-$ver-(x86_64|aarch64)-unknown-linux-gnu\.tar\.gz$" SHA256SUMS | sha256sum -c --quiet -) || {
  echo "release tarballs do not match the release's SHA256SUMS" >&2; exit 1; }

# Host-independent source URLs: $CARCH would expand to the build host in .SRCINFO.
python3 - "$workdir/PKGBUILD" "$ver" "$assets/SHA256SUMS" "$pkg" "$stem" <<'PY'
import pathlib, re, sys
p, ver, sums = pathlib.Path(sys.argv[1]), sys.argv[2], pathlib.Path(sys.argv[3]).read_text()
pkg, stem = sys.argv[4], sys.argv[5]
s = p.read_text()
s2, n = re.subn(
    rf'^(source_(x86_64|aarch64)=\(".*?{re.escape(stem)}-\$pkgver-)\$CARCH(-unknown-linux-gnu\.tar\.gz"\))$',
    r'\1\2\3',
    s,
    flags=re.M,
)
if n:
    p.write_text(s2)
    s = s2
for arch in ("x86_64", "aarch64"):
    if not re.search(rf'^source_{arch}=.*{re.escape(stem)}-\$pkgver-{arch}-unknown-linux-gnu\.tar\.gz', s, re.M):
        sys.exit(f"PKGBUILD source_{arch} must use a literal {arch} tarball name, not $CARCH")
if not re.search(rf'^pkgname={re.escape(pkg)}$', s, re.M):
    sys.exit(f"refusing to publish a recipe whose pkgname is not {pkg}")
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
    name = f"{stem}-{ver}-{arch}-unknown-linux-gnu.tar.gz"
    if not m:
        sys.exit(f"PKGBUILD sha256sums_{arch} is not one sha256")
    if want.get(name) != m.group(1).lower():
        sys.exit(f"PKGBUILD sha256sums_{arch} does not match {name} in the release SHA256SUMS")
print(f"recipe {pkg} pkgver={ver}: checksums match the release", file=sys.stderr)
PY
rel=$(sed -n 's/^pkgrel=//p' "$workdir/PKGBUILD")

recipedir="$workdir/$pkg"
mkdir -p "$recipedir"
mv "$workdir/PKGBUILD" "$recipedir/PKGBUILD"
(cd "$recipedir" && makepkg --printsrcinfo > .SRCINFO)
python3 - "$recipedir/.SRCINFO" "$recipedir/PKGBUILD" "$ver" "$rel" "$pkg" "$stem" <<'PY'
import pathlib, re, sys
src = pathlib.Path(sys.argv[1]).read_text()
pb = pathlib.Path(sys.argv[2]).read_text()
ver, rel, pkg, stem = sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6]
fields = {}
for line in src.splitlines():
    if " = " in line:
        k, v = line.strip().split(" = ", 1)
        fields.setdefault(k, []).append(v)
if fields.get("pkgbase") != [pkg] or fields.get("pkgname") != [pkg]:
    sys.exit(f".SRCINFO pkgbase/pkgname mismatch: {fields.get('pkgbase')} {fields.get('pkgname')}")
if fields.get("pkgver") != [ver] or fields.get("pkgrel") != [rel]:
    sys.exit(f".SRCINFO version mismatch: {fields.get('pkgver')}-{fields.get('pkgrel')} (expected {ver}-{rel})")
for arch in ("x86_64", "aarch64"):
    urls = fields.get(f"source_{arch}", [])
    if len(urls) != 1 or not urls[0].endswith(f"/v{ver}/{stem}-{ver}-{arch}-unknown-linux-gnu.tar.gz"):
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
state=$(python3 - "$info" "$org" "$ver-$rel" "$pkg" <<'PY'
import json, sys
info, org, want, pkg = json.loads(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
res = [r for r in info.get("results", []) if r.get("Name") == pkg]
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
