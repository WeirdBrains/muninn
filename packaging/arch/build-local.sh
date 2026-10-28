#!/usr/bin/env bash
# Build the Arch binary package (muninn-bin) from the current checkout, without
# downloading: builds the release binary, makes the tarball the PKGBUILD expects
# (scripts/dist.sh), then runs makepkg. Output: target/pkg/arch/. Never installs.
# --source builds packaging/arch/PKGBUILD (compiles with cargo) instead.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$root/target/pkg/arch"; rm -rf "$out"; mkdir -p "$out"
if [[ "${1:-}" == --source ]]; then
  shift
  ver="$(sed -n 's/^pkgver=//p' "$root/packaging/arch/PKGBUILD")"
  cp "$root/packaging/arch/PKGBUILD" "$out/"
  git -C "$root" archive --format=tar.gz --prefix="muninn-$ver/" -o "$out/muninn-$ver.tar.gz" HEAD
else
  (cd "$root" && cargo build --locked --release)
  tarball="$("$root/scripts/dist.sh" x86_64-unknown-linux-gnu "$root/target/release/mnnn")"
  ver="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
  sed "s/^pkgver=.*/pkgver=$ver/" "$root/packaging/arch/muninn-bin/PKGBUILD" > "$out/PKGBUILD"
  cp "$root/$tarball" "$out/"
fi
cd "$out"
makepkg -f --noconfirm "$@"
if command -v namcap >/dev/null; then namcap PKGBUILD ./*.pkg.tar.*; else echo "namcap not installed; skipped"; fi
