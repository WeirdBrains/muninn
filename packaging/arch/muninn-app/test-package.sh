#!/usr/bin/env bash
# Validate a built pacman package without installing it or running the GUI.
set -euo pipefail
pkg=$(realpath "${1:?Usage: test-package.sh /path/to/muninn-app.pkg.tar.zst}")
root=$(cd "$(dirname "$0")/../../.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
bsdtar -xf "$pkg" -C "$tmp"
grep -qx 'pkgname = muninn-app' "$tmp/.PKGINFO"
for dep in gtk3 glib2 'glibc>=2.39' gcc-libs; do
  grep -qx "depend = $dep" "$tmp/.PKGINFO"
  test -z "$(pacman -T "$dep")"
done
test -x "$tmp/opt/muninn-app/muninn"
test -x "$tmp/usr/bin/muninn"
grep -Fx 'exec /opt/muninn-app/muninn "$@"' "$tmp/usr/bin/muninn"
grep -qx 'Exec=muninn' "$tmp/usr/share/applications/muninn-app.desktop"
grep -qx 'Icon=com.weirdbrains.muninn' "$tmp/usr/share/applications/muninn-app.desktop"
cmp "$root/app/assets/icon/muninn-256.png" "$tmp/usr/share/icons/hicolor/256x256/apps/com.weirdbrains.muninn.png"
for file in "$tmp/opt/muninn-app/muninn" "$tmp/opt/muninn-app/lib/"*.so; do
  libs=$(ldd "$file")
  if [[ $libs == *'not found'* ]]; then printf '%s\n' "$libs"; exit 1; fi
done
# TUI package owns /usr/bin/mnnn; the GUI must not contain it or TUI man pages.
test ! -e "$tmp/usr/bin/mnnn"
test ! -e "$tmp/usr/share/man"
test ! -e "$tmp/home"
command -v desktop-file-validate >/dev/null && desktop-file-validate "$tmp/usr/share/applications/muninn-app.desktop"
echo 'Arch package paths, dependencies, libraries, desktop entry, and approved icon: OK'
