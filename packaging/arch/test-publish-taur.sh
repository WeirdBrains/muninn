#!/usr/bin/env bash
# Check scripts/publish-taur.sh with stub gh, taurctl and makepkg on PATH: nothing is
# downloaded or uploaded. Run: packaging/arch/test-publish-taur.sh
set -euo pipefail
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
ver=9.9.9
bin="$tmp/bin"; mkdir -p "$bin"
export ASSETS="$tmp/assets" LOG="$tmp/log" DRAFT=false TAUR_VER="1.0.0-1" TAUR_ORG=WeirdBrains
# gh: release view --jq .isDraft prints $DRAFT; release download copies from $ASSETS.
cat > "$bin/gh" <<'SH'
#!/usr/bin/env bash
echo "GH $*" >> "$LOG"
case "$1 $2" in
  "release view") [[ -d "$ASSETS" ]] || exit 1; echo "$DRAFT" ;;
  "release download")
    pat=""; dir=.
    while (($#)); do case "$1" in --pattern) pat=$2; shift ;; --dir) dir=$2; shift ;; esac; shift; done
    cp "$ASSETS/$pat" "$dir/" ;;
  *) exit 3 ;;
esac
SH
cat > "$bin/taurctl" <<'SH'
#!/usr/bin/env bash
echo "TAURCTL $*" >> "$LOG"
case "$1" in
  info) printf '{"results":[{"Name":"muninn-bin","Version":"%s","Organization":"%s"}]}\n' "$TAUR_VER" "$TAUR_ORG" ;;
  pkg) cp "$3" "$ASSETS/../uploaded.tar.gz" ;;
  *) exit 3 ;;
esac
SH
# makepkg --printsrcinfo: the fields publish-taur.sh checks, read from the PKGBUILD.
cat > "$bin/makepkg" <<'SH'
#!/usr/bin/env bash
[[ "$1" == --printsrcinfo ]] || exit 3
source ./PKGBUILD
echo "pkgbase = $pkgname"; echo "	pkgver = $pkgver"; echo "	pkgrel = $pkgrel"
for a in "${arch[@]}"; do
  s="source_$a[@]"; h="sha256sums_$a[@]"
  for v in "${!s}"; do echo "	source_$a = $v"; done
  for v in "${!h}"; do echo "	sha256sums_$a = $v"; done
done
echo; echo "pkgname = $pkgname"
SH
chmod +x "$bin"/*

fixture() { # $1: how to write the PKGBUILD (good, skip, carch, wrongsum)
  rm -rf "$ASSETS" "$tmp/uploaded.tar.gz"; mkdir -p "$ASSETS" "$tmp/pk/packaging/arch"
  for a in x86_64 aarch64; do echo "$a binary" > "$tmp/m-$a"; tar -C "$tmp" -czf "$ASSETS/muninn-$ver-$a-unknown-linux-gnu.tar.gz" "m-$a"; done
  (cd "$ASSETS" && sha256sum muninn-$ver-*-unknown-linux-gnu.tar.gz > SHA256SUMS)
  local x a; x=$(sed -n 's/ .*x86_64.*//p' "$ASSETS/SHA256SUMS"); a=$(sed -n 's/ .*aarch64.*//p' "$ASSETS/SHA256SUMS")
  case $1 in skip) x=SKIP ;; wrongsum) a=$x ;; esac
  sed -e "s/^pkgver=.*/pkgver=$ver/" -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$x')/" \
      -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$a')/" "$root/packaging/arch/muninn-bin/PKGBUILD" > "$tmp/pk/packaging/arch/PKGBUILD"
  [[ $1 == carch ]] && sed -i 's/-x86_64-unknown/-$CARCH-unknown/' "$tmp/pk/packaging/arch/PKGBUILD"
  tar -C "$tmp/pk" -czf "$ASSETS/muninn-$ver-packaging.tar.gz" packaging
  : > "$LOG"
}
run() { TMPDIR="$tmp" PATH="$bin:$PATH" "$root/scripts/publish-taur.sh" "$@" > "$tmp/out" 2>&1; }
fail=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; cat "$tmp/out" "$LOG"; fail=1; }

fixture good
if run --dry-run --out "$tmp/dry.tar.gz" "$ver" && [[ -f "$tmp/dry.tar.gz" ]] && ! grep -q "TAURCTL" "$LOG"; then
  list=$(tar -tzf "$tmp/dry.tar.gz" | LC_ALL=C sort | tr '\n' ' ')
  mkdir -p "$tmp/d" && tar -C "$tmp/d" -xzf "$tmp/dry.tar.gz"
  if [[ "$list" == "muninn-bin/.SRCINFO muninn-bin/PKGBUILD " ]] && grep -q "source_aarch64 = .*/v$ver/muninn-$ver-aarch64-unknown-linux-gnu.tar.gz" "$tmp/d/muninn-bin/.SRCINFO"; then
    ok "dry-run: recipe archive with PKGBUILD and .SRCINFO, no taurctl"
  else bad "dry-run archive: $list"; fi
else bad "dry-run"; fi

fixture good
if run "$ver" && grep -q "^TAURCTL pkg upload .*/muninn-bin-$ver-1.src.tar.gz --org WeirdBrains -y$" "$LOG" && [[ -f "$tmp/uploaded.tar.gz" ]]; then
  ok "upload: taurctl pkg upload ARCHIVE --org WeirdBrains -y"
else bad "upload"; fi

fixture good; TAUR_VER="$ver-1"
if run "$ver" && grep -q "already has muninn-bin $ver-1" "$tmp/out" && ! grep -q "pkg upload" "$LOG"; then
  ok "same version on TAUR: no upload"
else bad "same version"; fi
TAUR_VER="1.0.0-1"

fixture good; TAUR_ORG=someone-else
if ! run "$ver" && grep -q "belongs to someone-else" "$tmp/out" && ! grep -q "pkg upload" "$LOG"; then
  ok "other org on TAUR: refused"
else bad "other org"; fi
TAUR_ORG=WeirdBrains

fixture good; DRAFT=true
if ! run "$ver" && grep -q "still a draft" "$tmp/out" && ! grep -q TAURCTL "$LOG"; then
  ok "draft release: refused before taurctl"
else bad "draft"; fi
DRAFT=false

fixture good; rm -rf "$ASSETS"
if ! run "$ver" && grep -q "not found" "$tmp/out"; then ok "no release: refused"; else bad "no release"; fi

for f in skip wrongsum carch; do
  fixture $f
  case $f in skip) want="SKIP checksums" ;; wrongsum) want="does not match muninn-$ver-aarch64" ;; carch) want="" ;; esac
  if [[ $f == carch ]]; then
    if run --dry-run --out "$tmp/c.tar.gz" "$ver" && tar -xzOf "$tmp/c.tar.gz" muninn-bin/.SRCINFO | grep -q "source_x86_64 = .*-x86_64-unknown"; then
      ok "\$CARCH source rewritten to the literal arch"; else bad carch; fi
  elif ! run --dry-run "$ver" && grep -q "$want" "$tmp/out"; then ok "$f: refused"; else bad "$f"; fi
done

fixture good; echo "tampered" >> "$tmp/m-x86_64"; tar -C "$tmp" -czf "$ASSETS/muninn-$ver-x86_64-unknown-linux-gnu.tar.gz" m-x86_64
if ! run --dry-run "$ver" && grep -q "do not match the release's SHA256SUMS" "$tmp/out"; then
  ok "tarball differs from SHA256SUMS: refused"; else bad "tampered tarball"; fi

fixture good
if run --dry-run --out "$tmp/l.tar.gz" "$ver" "$ASSETS/muninn-$ver-packaging.tar.gz" && ! grep -q "packaging.tar.gz --dir" "$LOG"; then
  ok "local packaging tarball: used without downloading it"; else bad "local tarball"; fi
exit $fail
