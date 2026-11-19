#!/usr/bin/env bash
# Regression test for scripts/fill-sums.py: with a complete synthetic SHA256SUMS, every
# generated recipe must carry the right sum on its real stanza line and no placeholder
# (all-zero sha256, SKIP). Guards the 2.28.0 bug where the sum went into a cask comment.
set -euo pipefail
cd "$(dirname "$0")/.."
ver=9.8.7
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/dist"
n=0
mk() { n=$((n+1)); printf '%064x  %s\n' "$n" "$1" >> "$tmp/dist/SHA256SUMS"; }
for t in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu aarch64-apple-darwin x86_64-apple-darwin; do mk "muninn-$ver-$t.tar.gz"; done
mk "muninn-$ver-macos.dmg"
mk "muninn-app-$ver-universal-apple-darwin.dmg"
mk "muninn-app-$ver-x86_64-unknown-linux-gnu.tar.gz"
mk "muninn-app-$ver-aarch64-unknown-linux-gnu.tar.gz"
for a in x86_64 aarch64; do mk "muninn-$ver-$a-windows-setup.exe"; done
python3 scripts/fill-sums.py "$ver" "$tmp/dist" >/dev/null
p="$tmp/dist/packaging"
sum_of() { grep -F "  $1" "$tmp/dist/SHA256SUMS" | cut -d' ' -f1; }
fail() { echo "test-fill-sums: FAIL: $*" >&2; exit 1; }
expect() { grep -qxF "$2" "$1" || fail "$1 lacks line: $2"; }
# no placeholders anywhere in the output, comments included
if grep -rEn '0{64}|SKIP' "$p"; then fail "placeholder left in output"; fi
# casks: the real stanza line, and the comment lines untouched
expect "$p/homebrew/Casks/muninn.rb" "  sha256 \"$(sum_of muninn-$ver-macos.dmg)\""
expect "$p/homebrew/Casks/muninn-app.rb" "  sha256 \"$(sum_of muninn-app-$ver-universal-apple-darwin.dmg)\""
expect "$p/homebrew/Casks/muninn-app.rb" '#   # in the copy: url "file://$dmg" and sha256 "$sha" (do not commit that copy)' 2>/dev/null \
  || fail "a comment line in the muninn-app cask was rewritten"
for c in muninn muninn-app; do expect "$p/homebrew/Casks/$c.rb" "  version \"$ver\""; done
# formula: each url paired with its own arch's sum
f="$p/homebrew/Formula/muninn.rb"
for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
  grep -A1 "muninn-#{version}-$t.tar.gz" "$f" | tail -1 | grep -qF "$(sum_of muninn-$ver-$t.tar.gz)" || fail "formula sum for $t"
done
# PKGBUILDs
expect "$p/arch/PKGBUILD" "sha256sums_x86_64=('$(sum_of muninn-$ver-x86_64-unknown-linux-gnu.tar.gz)')"
expect "$p/arch/PKGBUILD" "sha256sums_aarch64=('$(sum_of muninn-$ver-aarch64-unknown-linux-gnu.tar.gz)')"
expect "$p/arch/muninn-app-bin/PKGBUILD" "sha256sums_x86_64=('$(sum_of muninn-app-$ver-x86_64-unknown-linux-gnu.tar.gz)')"
expect "$p/arch/muninn-app-bin/PKGBUILD" "sha256sums_aarch64=('$(sum_of muninn-app-$ver-aarch64-unknown-linux-gnu.tar.gz)')"
# winget
w=$(echo "$p"/winget/manifests/w/WeirdBrains/muninn/tui/$ver/*installer.yaml)
for a in x86_64 aarch64; do
  up=$(sum_of muninn-$ver-$a-windows-setup.exe | tr a-f A-F)
  grep -A1 "muninn-$ver-$a-windows-setup.exe" "$w" | grep -qF "\"$up\"" || fail "winget sum for $a"
done
echo "test-fill-sums: ok"
