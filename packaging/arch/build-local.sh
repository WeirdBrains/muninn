#!/usr/bin/env bash
# Build the Arch package from the current checkout and install it.
#   default    muninn-bin: release binary + tarball (scripts/dist.sh), no download
#   --source   packaging/arch/PKGBUILD (makepkg builds the git checkout)
#   --no-install  only build into target/pkg/arch/
# namcap (if installed) lints the result; it never blocks the install.
# Other arguments go to makepkg. Install runs: sudo pacman -U <package>
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
out="${BUILD_LOCAL_OUT:-$root/target/pkg/arch}"
source=0; install=1; args=()
for a in "$@"; do
  case "$a" in
    --source) source=1 ;;
    --no-install) install=0 ;;
    *) args+=("$a") ;;
  esac
done
rm -rf "$out"; mkdir -p "$out"
if [[ $source -eq 1 ]]; then
  pkgbuild="$root/packaging/arch/PKGBUILD"
  (cd "$root/packaging/arch" && PKGDEST="$out" SRCDEST="$out/srcdest" BUILDDIR="$out/build" makepkg -f --noconfirm "${args[@]}")
else
  (cd "$root" && cargo build --locked --release)
  tarball="$("$root/scripts/dist.sh" x86_64-unknown-linux-gnu "$root/target/release/mnnn")"
  ver="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
  sed "s/^pkgver=.*/pkgver=$ver/" "$root/packaging/arch/muninn-bin/PKGBUILD" > "$out/PKGBUILD"
  cp "$root/$tarball" "$out/"
  pkgbuild="$out/PKGBUILD"
  (cd "$out" && makepkg -f --noconfirm "${args[@]}")
fi

shopt -s nullglob
pkgs=()
for p in "$out"/*.pkg.tar.*; do
  [[ "$p" == *-debug-* || "$p" == *.sig ]] || pkgs+=("$p")
done
[[ ${#pkgs[@]} -gt 0 ]] || { echo "build-local: makepkg produced no package in $out" >&2; exit 1; }
echo "built: ${pkgs[*]}"

# Lint only: a missing namcap or namcap warnings never stop the install.
if command -v namcap >/dev/null 2>&1; then
  namcap "$pkgbuild" "${pkgs[@]}" || echo "build-local: namcap reported problems (not fatal)" >&2
else
  echo "build-local: namcap not installed; lint skipped (pacman -S namcap to enable)"
fi

if [[ $install -eq 1 ]]; then
  echo "installing: sudo pacman -U --needed ${pkgs[*]}"
  sudo pacman -U --needed "${pkgs[@]}"
else
  echo "not installed (--no-install); install with: sudo pacman -U ${pkgs[*]}"
fi
