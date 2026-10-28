#!/usr/bin/env bash
# Build the Arch binary package (muninn-bin) from the current checkout, without
# downloading: builds the release binary, makes the tarball the PKGBUILD expects
# (scripts/dist.sh), then runs makepkg. Output: target/pkg/arch/. Never installs.
# --source builds packaging/arch/PKGBUILD (git checkout build) instead.
# Install the result with: sudo pacman -U target/pkg/arch/muninn*.pkg.tar.zst
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$root/target/pkg/arch"; rm -rf "$out"; mkdir -p "$out"
if [[ "${1:-}" == --source ]]; then
  shift
  cd "$root/packaging/arch"
  PKGDEST="$out" SRCDEST="$out/srcdest" BUILDDIR="$out/build" makepkg -f --noconfirm "$@"
  if command -v namcap >/dev/null; then namcap PKGBUILD "$out"/*.pkg.tar.*; else echo "namcap not installed; skipped"; fi
  exit 0
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
