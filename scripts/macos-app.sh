#!/usr/bin/env bash
# Build Muninn.app (universal: arm64 + x86_64) and muninn-<ver>-macos.dmg on macOS.
#   scripts/macos-app.sh
# Environment: DIST (default dist) holds muninn-<ver>-{aarch64,x86_64}-apple-darwin.tar.gz
# (scripts/dist.sh) and receives the .dmg. Needs only what macOS ships: lipo, sips,
# iconutil, codesign (ad-hoc, no Developer ID) and hdiutil. The icon comes from
# assets/icons/muninn.svg; without rsvg-convert, set ICON_PNG to a 1024 px render of it
# (`scripts/icons.sh png 1024 muninn-1024.png` on Linux), see assets/icons/README.md.
#
# The app opens Terminal running mnnn: the first time it offers to put the `mnnn` command
# on PATH (/usr/local/bin, or ~/.local/bin when that is not writable) and sets up the keys
# when there are none; then the TUI starts.
set -euo pipefail
cd "$(dirname "$0")/.."
ver=${VER:-$(scripts/version.sh)}
dist=${DIST:-dist}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
for a in aarch64 x86_64; do
  t="$dist/muninn-$ver-$a-apple-darwin.tar.gz"
  [[ -f $t ]] || { echo "missing $t (scripts/dist.sh $a-apple-darwin)" >&2; exit 1; }
  mkdir -p "$work/$a" && tar -xzf "$t" -C "$work/$a"
done
src="$work/aarch64/muninn-$ver-aarch64-apple-darwin"
app="$work/dmg/Muninn.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/Resources/mnnn" "$src/mnnn" "$work/x86_64/muninn-$ver-x86_64-apple-darwin/mnnn"
chmod 755 "$app/Contents/Resources/mnnn"
cp -R "$src/man" "$src/README.md" "$src/CHANGELOG.md" "$src/LICENSE" "$app/Contents/Resources/"
[[ -f $src/LICENSE-FONTS ]] && cp "$src/LICENSE-FONTS" "$app/Contents/Resources/"

# icon: from assets/icons/muninn.png (scripts/icons.sh); also the dmg's volume icon
scripts/icons.sh icns "$app/Contents/Resources/muninn.icns" >/dev/null
cp "$app/Contents/Resources/muninn.icns" "$work/dmg/.VolumeIcon.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Muninn</string>
  <key>CFBundleDisplayName</key><string>Muninn</string>
  <key>CFBundleIdentifier</key><string>com.weirdbrains.muninn</string>
  <key>CFBundleVersion</key><string>$ver</string>
  <key>CFBundleShortVersionString</key><string>$ver</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>Muninn</string>
  <key>CFBundleIconFile</key><string>muninn</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHumanReadableCopyright</key><string>BSD-3-Clause, WeirdBrains</string>
</dict>
</plist>
PLIST

# The app itself only opens Terminal on the session script beside the binary.
cat > "$app/Contents/MacOS/Muninn" <<'SH'
#!/bin/bash
res="$(cd "$(dirname "$0")/../Resources" && pwd)"
exec open -a Terminal "$res/Muninn.command"
SH
cat > "$app/Contents/Resources/Muninn.command" <<'SH'
#!/bin/bash
# Runs in Terminal: offer the mnnn command on PATH once, set up keys on first use, open the TUI.
res="$(cd "$(dirname "$0")" && pwd)"
mnnn="$res/mnnn"
state="$HOME/Library/Application Support/Muninn"
if [[ ! -e "$state/cli-asked" ]]; then
  mkdir -p "$state" && touch "$state/cli-asked"
  if ! command -v mnnn >/dev/null 2>&1; then
    read -r -p "Add the mnnn command to your PATH (a link to this app's copy)? [Y/n] " yn
    if [[ ! $yn =~ ^[Nn] ]]; then
      if [[ -d /usr/local/bin && -w /usr/local/bin ]]; then dir=/usr/local/bin; else dir="$HOME/.local/bin"; mkdir -p "$dir"; fi
      ln -sf "$mnnn" "$dir/mnnn" && echo "Linked $dir/mnnn."
      case ":$PATH:" in *":$dir:"*) ;; *) echo "Add $dir to PATH, e.g. echo 'export PATH=\"$dir:\$PATH\"' >> ~/.zshrc" ;; esac
      echo
    fi
  fi
fi
if ! "$mnnn" keys status >/dev/null 2>&1; then
  echo "First run: Muninn sets up its keys and asks for a new password (typing is hidden)."
  echo "Afterwards it prints 24 recovery words: write them down and keep them offline."
  echo
  if ! "$mnnn" keys init; then
    echo
    echo "Muninn could not set up its keys (see the error above). Nothing was changed;"
    echo "open Muninn again to retry, or run 'mnnn keys init' in a terminal."
    read -r -p "Press Enter to close. "
    exit 1
  fi
  echo
  read -r -p "Write down the recovery words above now; they are not shown again. Press Enter to continue. "
  clear
fi
"$mnnn" tui || read -r -p "Press Enter to close. "
SH
chmod 755 "$app/Contents/MacOS/Muninn" "$app/Contents/Resources/Muninn.command"

xattr -cr "$app"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
ln -s /Applications "$work/dmg/Applications"
dmg="$dist/muninn-$ver-macos.dmg"
rm -f "$dmg"
# The volume shows .VolumeIcon.icns only when its root has the custom-icon flag, which
# -srcfolder drops: build read-write, set the flag on the mounted volume, then compress.
rw="$work/rw.dmg"
hdiutil create -quiet -volname "Muninn $ver" -srcfolder "$work/dmg" -ov -format UDRW "$rw"
mnt=$(hdiutil attach -nobrowse -noautoopen "$rw" | awk -F'\t' '/\/Volumes\//{print $NF}')
if command -v SetFile >/dev/null; then SetFile -a C "$mnt"; else xattr -wx com.apple.FinderInfo "0000000000000000040000000000000000000000000000000000000000000000" "$mnt"; fi
hdiutil detach -quiet "$mnt"
hdiutil convert -quiet "$rw" -format UDZO -o "$dmg"
echo "$dmg"
