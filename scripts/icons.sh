#!/usr/bin/env bash
# Make every installer icon from the one source image, assets/icons/muninn.png
# (square PNG, 1024x1024 or larger). Nothing generated is committed; the packaging
# scripts call this at build time, so a new logo is a file drop plus a rebuild.
#   scripts/icons.sh ico  OUT.ico        Windows: setup.exe, uninstaller, Start Menu (16-256 px)
#   scripts/icons.sh icns OUT.icns       macOS: Muninn.app and the dmg volume (16-1024 px)
#   scripts/icons.sh check               validate the source image
# ico needs Python Pillow or ImageMagick; icns needs macOS (sips, iconutil).
set -euo pipefail
cd "$(dirname "$0")/.."
src=${ICON_SRC:-assets/icons/muninn.png}
kind=${1:?usage: icons.sh ico|icns|check [OUT]}
size() {
  if command -v sips >/dev/null; then
    sips -g pixelWidth -g pixelHeight "$src" | awk '/pixel/{printf "%s ", $2}'
  else
    python3 -c 'import struct,sys;d=open(sys.argv[1],"rb").read(24);assert d[:8]==b"\x89PNG\r\n\x1a\n","not a PNG";print(*struct.unpack(">II",d[16:24]))' "$src"
  fi
}
read -r w h <<<"$(size)"
[[ $w == "$h" ]] || { echo "$src must be square (is ${w}x$h)" >&2; exit 1; }
(( w >= 1024 )) || { echo "$src must be at least 1024x1024 (is ${w}x$h)" >&2; exit 1; }
case $kind in
  check) echo "$src ok (${w}x$h)" ;;
  ico)
    out=${2:?OUT.ico}
    if python3 -c 'import PIL' 2>/dev/null; then
      python3 -c 'import sys;from PIL import Image;Image.open(sys.argv[1]).convert("RGBA").save(sys.argv[2],sizes=[(s,s) for s in (16,24,32,48,64,128,256)])' "$src" "$out"
    else
      magick "$src" -define icon:auto-resize=256,128,64,48,32,24,16 "$out"
    fi
    echo "$out" ;;
  icns)
    out=${2:?OUT.icns}
    set_dir=$(mktemp -d)/muninn.iconset; mkdir -p "$set_dir"
    for s in 16 32 128 256 512; do
      sips -z $s $s "$src" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
      sips -z $((s * 2)) $((s * 2)) "$src" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
    done
    iconutil -c icns "$set_dir" -o "$out"
    rm -rf "$(dirname "$set_dir")"
    echo "$out" ;;
  *) echo "unknown kind $kind" >&2; exit 1 ;;
esac
