#!/usr/bin/env bash
# Build the Arch package from the current checkout (no download): makes the tarball the
# PKGBUILD expects with `git archive`, then runs makepkg. Output: target/pkg/arch/.
# Does not install anything (no pacman -U).
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
ver="$(sed -n 's/^pkgver=//p' "$root/packaging/arch/PKGBUILD")"
out="$root/target/pkg/arch"
mkdir -p "$out"
cp "$root/packaging/arch/PKGBUILD" "$out/"
git -C "$root" archive --format=tar.gz --prefix="muninn-$ver/" -o "$out/muninn-$ver.tar.gz" HEAD
cd "$out"
makepkg -f --noconfirm "$@"
command -v namcap >/dev/null && namcap PKGBUILD ./*.pkg.tar.* || echo "namcap not installed; skipped"
