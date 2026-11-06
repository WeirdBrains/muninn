#!/usr/bin/env bash
# Package the Flutter desktop app for the host Linux arch.
#   $DIST/muninn-app-<ver>-<triple>.tar.gz   relocatable bundle (run ./muninn)
#   $DIST/muninn-app_<ver>-1_<debarch>.deb   installs to /opt/muninn
#
# Usage:
#   scripts/flutter-dist.sh
#   DIST=/path/to/out CARGO_TARGET_DIR=/path/to/cargo-target scripts/flutter-dist.sh
#   SKIP_BUILD=1 scripts/flutter-dist.sh   # reuse app/build/linux/<arch>/release/bundle
#
# Package name is muninn-app so it does not clash with the TUI/CLI package muninn
# (which installs /usr/bin/mnnn). Does not touch ~/.muninn. Does not publish a
# GitHub release. Set CARGO_TARGET_DIR to keep the Rust build off a small disk
# (Native Assets honors it via cargo).
set -euo pipefail
cd "$(dirname "$0")/.."
ver="$(scripts/version.sh)"
host="$(uname -s)"
arch="$(uname -m)"
[[ $host == Linux ]] || { echo "scripts/flutter-dist.sh packages Linux only (host is $host)" >&2; exit 1; }
case "$arch" in
  x86_64|amd64) triple=x86_64-unknown-linux-gnu; deb_arch=amd64; flutter_arch=x64 ;;
  aarch64|arm64) triple=aarch64-unknown-linux-gnu; deb_arch=arm64; flutter_arch=arm64 ;;
  *) echo "unsupported arch: $arch" >&2; exit 1 ;;
esac

root=$PWD
dist=${DIST:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
bundle=$root/app/build/linux/$flutter_arch/release/bundle

if [[ ${SKIP_BUILD:-} != 1 ]]; then
  ( cd "$root/app" && flutter pub get && flutter build linux --release )
fi
[[ -x $bundle/muninn ]] || { echo "missing release bundle at $bundle (run flutter build linux --release)" >&2; exit 1; }

name=muninn-app-$ver-$triple
stage=$dist/$name
rm -rf "$stage"
mkdir -p "$stage"
cp -a "$bundle/." "$stage/"
chmod 755 "$stage/muninn"
cp "$root/LICENSE" "$root/CHANGELOG.md" "$root/app/README.md" "$stage/"
cp "$root/app/linux/com.weirdbrains.muninn.desktop" "$stage/"
cat > "$stage/INSTALL.txt" <<EOF
Muninn graphical app $ver ($triple)

Run from this directory:

  ./muninn

Needs GTK 3 at runtime. The data directory is MUNINN_HOME or ~/.muninn (the same
store as the TUI/CLI). This package does not include the mnnn CLI; that ships
in the separate muninn packages.

Desktop entry: com.weirdbrains.muninn.desktop (Exec=muninn). The .deb install
path is /opt/muninn, with a wrapper at /usr/bin/muninn.
EOF

if tar --version 2>/dev/null | grep -q GNU; then own=(--owner=0 --group=0); else own=(--uid 0 --gid 0); fi
( cd "$dist" && tar -czf "$name.tar.gz" "${own[@]}" "$name" )
echo "$dist/$name.tar.gz"

deb_root=$dist/.deb-root-$$
cleanup() { rm -rf "$deb_root"; }
trap cleanup EXIT
mkdir -p "$deb_root/opt/muninn" \
         "$deb_root/usr/bin" \
         "$deb_root/usr/share/applications" \
         "$deb_root/usr/share/icons/hicolor/256x256/apps" \
         "$deb_root/usr/share/doc/muninn-app" \
         "$deb_root/DEBIAN"
cp -a "$bundle/." "$deb_root/opt/muninn/"
chmod 755 "$deb_root/opt/muninn/muninn"
printf '%s\n' '#!/bin/sh' 'exec /opt/muninn/muninn "$@"' > "$deb_root/usr/bin/muninn"
chmod 755 "$deb_root/usr/bin/muninn"
sed 's|^Exec=muninn$|Exec=/opt/muninn/muninn|' \
  "$root/app/linux/com.weirdbrains.muninn.desktop" \
  > "$deb_root/usr/share/applications/com.weirdbrains.muninn.desktop"
install -m644 "$root/app/assets/icon/muninn-256.png" \
  "$deb_root/usr/share/icons/hicolor/256x256/apps/com.weirdbrains.muninn.png"
install -m644 "$root/LICENSE" "$deb_root/usr/share/doc/muninn-app/copyright"
install -m644 "$root/CHANGELOG.md" "$deb_root/usr/share/doc/muninn-app/changelog"
install -m644 "$root/app/README.md" "$deb_root/usr/share/doc/muninn-app/README.md"

installed=$(du -sk "$deb_root/opt" "$deb_root/usr" | awk '{s += $1} END {print s}')
cat > "$deb_root/DEBIAN/control" <<EOF
Package: muninn-app
Version: ${ver}-1
Section: utils
Priority: optional
Architecture: ${deb_arch}
Maintainer: Michael Christenson II <m3talsmith@gmail.com>
Installed-Size: ${installed}
Depends: libgtk-3-0, libglib2.0-0, libstdc++6, libgcc-s1, libc6
Homepage: https://muninn.weirdbrains.com
Description: Muninn graphical workbench for market data
 The Flutter desktop app for Muninn (publisher WeirdBrains). It uses the
 same encrypted store as the mnnn CLI/TUI (MUNINN_HOME or ~/.muninn).
 This package does not replace the muninn CLI package (which installs
 /usr/bin/mnnn).
EOF

deb_name=muninn-app_${ver}-1_${deb_arch}.deb
if command -v dpkg-deb >/dev/null; then
  dpkg-deb --root-owner-group --build "$deb_root" "$dist/$deb_name" >/dev/null
else
  tmp=$(mktemp -d)
  ( cd "$deb_root" && tar "${own[@]}" -czf "$tmp/data.tar.gz" --exclude=./DEBIAN . )
  ( cd "$deb_root/DEBIAN" && tar "${own[@]}" -czf "$tmp/control.tar.gz" . )
  printf '2.0\n' > "$tmp/debian-binary"
  rm -f "$dist/$deb_name"
  ( cd "$tmp" && ar rc "$dist/$deb_name" debian-binary control.tar.gz data.tar.gz )
  rm -rf "$tmp"
fi
echo "$dist/$deb_name"
