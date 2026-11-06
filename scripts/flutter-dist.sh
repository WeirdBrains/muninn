#!/usr/bin/env bash
# Package the Flutter desktop app for the host OS/arch.
#   Linux:
#     $DIST/muninn-app-<ver>-<triple>.tar.gz   relocatable bundle (run ./muninn)
#     $DIST/muninn-app_<ver>-1_<debarch>.deb   installs to /opt/muninn
#     $DIST/muninn-app-<ver>-1.<rpmarch>.rpm  installs to /opt/muninn (needs rpmbuild)
#   macOS:
#     $DIST/muninn-app-<ver>-<triple>.dmg      .app plus an Applications shortcut
#
# Usage:
#   scripts/flutter-dist.sh
#   DIST=/path/to/out CARGO_TARGET_DIR=/path/to/cargo-target scripts/flutter-dist.sh
#   SKIP_BUILD=1 scripts/flutter-dist.sh   # reuse an existing release bundle
#
# Package name is muninn-app so it does not clash with the TUI/CLI package muninn
# (which installs /usr/bin/mnnn). Does not touch ~/.muninn. Does not publish a
# GitHub release. Set CARGO_TARGET_DIR to keep the Rust build off a small disk
# (Native Assets honors it via cargo when the toolchain consults the env).
# Apple builds use Swift Package Manager, not CocoaPods.
# Linux .rpm creation needs rpmbuild (rpm-tools / rpm-build). A relocated
# rpmbuild should set RPM_CONFIGDIR to its usr/lib/rpm directory.
set -euo pipefail
cd "$(dirname "$0")/.."
ver="$(scripts/version.sh)"
host="$(uname -s)"
arch="$(uname -m)"
root=$PWD
dist=${DIST:-dist}
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)

package_linux() {
  local triple deb_arch flutter_arch bundle name stage deb_root installed deb_name tmp own
  case "$arch" in
    x86_64|amd64) triple=x86_64-unknown-linux-gnu; deb_arch=amd64; flutter_arch=x64 ;;
    aarch64|arm64) triple=aarch64-unknown-linux-gnu; deb_arch=arm64; flutter_arch=arm64 ;;
    *) echo "unsupported arch: $arch" >&2; exit 1 ;;
  esac
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
  cat > "$stage/INSTALL.txt" <<INSTALL
Muninn graphical app $ver ($triple)

Run from this directory:

  ./muninn

Needs GTK 3 at runtime. The data directory is MUNINN_HOME or ~/.muninn (the same
store as the TUI/CLI). This package does not include the mnnn CLI; that ships
in the separate muninn packages.

Desktop entry: com.weirdbrains.muninn.desktop (Exec=muninn). The .deb install
path is /opt/muninn, with a wrapper at /usr/bin/muninn.
INSTALL

  local -a own
  if tar --version 2>/dev/null | grep -q GNU; then own=(--owner=0 --group=0); else own=(--uid 0 --gid 0); fi
  ( cd "$dist" && tar -czf "$name.tar.gz" "${own[@]}" "$name" )
  echo "$dist/$name.tar.gz"

  deb_root=$dist/.deb-root-$$
  rpm_top=$dist/.rpmbuild-$$
  trap 'rm -rf "'"$deb_root"'" "'"$rpm_top"'"' EXIT
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
  cat > "$deb_root/DEBIAN/control" <<CONTROL
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
CONTROL

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

  case "$arch" in
    x86_64|amd64) rpm_arch=x86_64 ;;
    aarch64|arm64) rpm_arch=aarch64 ;;
    *) rpm_arch= ;;
  esac
  if [[ -z ${rpm_arch:-} ]]; then
    :
  elif ! command -v rpmbuild >/dev/null; then
    echo "skipping rpm: rpmbuild not found (install rpm-tools / rpm-build)" >&2
  else
    mkdir -p "$rpm_top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    cat > "$rpm_top/SPECS/muninn-app.spec" <<SPEC
%global debug_package %{nil}
%global _build_id_links none
%global __os_install_post %{nil}
Name: muninn-app
Version: ${ver}
Release: 1
Summary: Muninn graphical workbench for market data
License: BSD-3-Clause
URL: https://muninn.weirdbrains.com
Group: Applications/Engineering
AutoReqProv: no
BuildArch: ${rpm_arch}
Requires: gtk3
Requires: glib2

%description
The Flutter desktop app for Muninn (publisher WeirdBrains). It uses the
same encrypted store as the mnnn CLI/TUI (MUNINN_HOME or ~/.muninn).
This package does not replace the muninn CLI package (which installs
/usr/bin/mnnn).

%prep

%install
mkdir -p %{buildroot}/opt/muninn \
         %{buildroot}/usr/bin \
         %{buildroot}/usr/share/applications \
         %{buildroot}/usr/share/icons/hicolor/256x256/apps \
         %{buildroot}/usr/share/doc/muninn-app
cp -a ${bundle}/. %{buildroot}/opt/muninn/
chmod 755 %{buildroot}/opt/muninn/muninn
printf '%s\n' '#!/bin/sh' 'exec /opt/muninn/muninn "\$@"' > %{buildroot}/usr/bin/muninn
chmod 755 %{buildroot}/usr/bin/muninn
sed 's|^Exec=muninn\$|Exec=/opt/muninn/muninn|' \
  ${root}/app/linux/com.weirdbrains.muninn.desktop \
  > %{buildroot}/usr/share/applications/com.weirdbrains.muninn.desktop
install -m644 ${root}/app/assets/icon/muninn-256.png \
  %{buildroot}/usr/share/icons/hicolor/256x256/apps/com.weirdbrains.muninn.png
install -m644 ${root}/LICENSE %{buildroot}/usr/share/doc/muninn-app/copyright
install -m644 ${root}/CHANGELOG.md %{buildroot}/usr/share/doc/muninn-app/changelog
install -m644 ${root}/app/README.md %{buildroot}/usr/share/doc/muninn-app/README.md

%files
%defattr(-,root,root,-)
/opt/muninn
/usr/bin/muninn
/usr/share/applications/com.weirdbrains.muninn.desktop
/usr/share/icons/hicolor/256x256/apps/com.weirdbrains.muninn.png
%doc /usr/share/doc/muninn-app/README.md
%doc /usr/share/doc/muninn-app/changelog
%license /usr/share/doc/muninn-app/copyright
SPEC
    rpmbuild -bb --define "_topdir $rpm_top" --define "_tmppath $rpm_top" \
      --define "_dbpath $rpm_top/rpmdb" \
      "$rpm_top/SPECS/muninn-app.spec"
    rpm_name=muninn-app-${ver}-1.${rpm_arch}.rpm
    cp "$rpm_top/RPMS/${rpm_arch}/$rpm_name" "$dist/$rpm_name"
    rm -rf "$rpm_top"
    echo "$dist/$rpm_name"
  fi
  rm -rf "$deb_root"
  trap - EXIT
}

# Copy the .app into a read-write dmg, add an Applications shortcut, convert to UDZO.
# Do not use `hdiutil create -srcfolder` with a symlink to /Applications — it would
# follow the link and copy the real Applications folder.
package_macos_dmg() {
  local app="$1" out="$2" volname="$3" triple="${4:-}"
  local size tmp_dmg mount
  size=$(du -sm "$app" | awk '{print int($1) + 30}')
  [[ $size -ge 50 ]] || size=50
  tmp_dmg="${out%.dmg}.rw.$$.dmg"
  rm -f "$tmp_dmg" "$out"
  hdiutil create -size "${size}m" -fs HFS+ -volname "$volname" "$tmp_dmg" >/dev/null
  mount=$(hdiutil attach -readwrite -nobrowse -noverify -noautoopen "$tmp_dmg" | grep -o '/Volumes/.*' | tail -1)
  [[ -n $mount && -d $mount ]] || { echo "failed to mount $tmp_dmg" >&2; exit 1; }
  detach() { hdiutil detach "$mount" -quiet >/dev/null 2>&1 || true; }
  trap 'detach; rm -f "$tmp_dmg"' EXIT
  cp -R "$app" "$mount/"
  ln -s /Applications "$mount/Applications"
  cat > "$mount/INSTALL.txt" <<INSTALL
Muninn graphical app $ver ($triple)

Drag Muninn into Applications, or run the .app from this disk image.

The data directory is MUNINN_HOME or ~/.muninn (the same store as the TUI/CLI).
This package does not include the mnnn CLI; that ships in the separate muninn
packages. The image is not notarized.

Apple builds use Swift Package Manager (not CocoaPods).
INSTALL
  cp "$root/LICENSE" "$root/CHANGELOG.md" "$mount/" 2>/dev/null || true
  sync
  detach
  trap - EXIT
  hdiutil convert "$tmp_dmg" -format UDZO -imagekey zlib-level=9 -o "$out" >/dev/null
  rm -f "$tmp_dmg"
}

package_macos() {
  local triple products app_bundle name dmg
  case "$arch" in
    arm64|aarch64) triple=aarch64-apple-darwin ;;
    x86_64) triple=x86_64-apple-darwin ;;
    *) echo "unsupported arch: $arch" >&2; exit 1 ;;
  esac
  if [[ ${SKIP_BUILD:-} != 1 ]]; then
    flutter config --enable-macos-desktop --enable-swift-package-manager >/dev/null
    ( cd "$root/app" && flutter pub get && flutter build macos --release )
  fi
  products=$root/app/build/macos/Build/Products/Release
  app_bundle=
  for cand in "$products/Muninn.app" "$products/muninn.app" "$products/Runner.app"; do
    if [[ -d $cand ]]; then app_bundle=$cand; break; fi
  done
  if [[ -z $app_bundle ]]; then
    app_bundle=$(find "$products" -maxdepth 1 -name '*.app' -print 2>/dev/null | head -1 || true)
  fi
  [[ -d $app_bundle ]] || { echo "missing release .app under $products (run flutter build macos --release)" >&2; exit 1; }

  # flutter build macos --release produces a universal (arm64 + x86_64) .app.
  if command -v lipo >/dev/null; then
    bins=$(find "$app_bundle/Contents/MacOS" -type f -perm +111 2>/dev/null | head -1 || true)
    if [[ -n $bins ]] && lipo -info "$bins" 2>/dev/null | grep -q 'x86_64' && lipo -info "$bins" 2>/dev/null | grep -q 'arm64'; then
      triple=universal-apple-darwin
    fi
  fi
  name=muninn-app-$ver-$triple
  dmg=$dist/$name.dmg
  package_macos_dmg "$app_bundle" "$dmg" "Muninn" "$triple"
  echo "$dmg"
}

case "$host" in
  Linux) package_linux ;;
  Darwin) package_macos ;;
  *)
    echo "scripts/flutter-dist.sh packages Linux and macOS only (host is $host)" >&2
    exit 1
    ;;
esac
