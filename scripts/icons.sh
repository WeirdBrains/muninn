#!/usr/bin/env bash
# Make every installer and app icon from the one source image, assets/icons/muninn.svg
# (the Muninn raven tile, rendered with its Tokyo Night fallback colours). Nothing generated
# is committed; the packaging scripts call this at build time, so a new logo is a file
# drop plus a rebuild.
#   scripts/icons.sh png SIZE OUT.png    one square PNG
#   scripts/icons.sh ico  OUT.ico        Windows: setup.exe, uninstaller, Start Menu (16-256 px)
#   scripts/icons.sh icns OUT.icns       macOS: Muninn.app and the dmg volume (16-1024 px)
#   scripts/icons.sh check               check the source and that it can be rendered
# Renders SVG with rsvg-convert or ImageMagick (with librsvg). macOS has neither by default
# (Quick Look fills the transparent corners with white), so there set ICON_PNG to a
# 1024 px render made elsewhere with `scripts/icons.sh png 1024 muninn-1024.png`; sips
# scales it. icns needs macOS (sips, iconutil); ico needs ImageMagick or Python Pillow.
set -euo pipefail
cd "$(dirname "$0")/.."
src=${ICON_SRC:-assets/icons/muninn.svg}
[[ -f $src ]] || { echo "missing $src" >&2; exit 1; }
kind=${1:?usage: icons.sh png|ico|icns|check ...}

render() { # SIZE OUT
  local s=$1 out=$2
  if [[ -n ${ICON_PNG:-} ]]; then
    if command -v sips >/dev/null; then sips -z "$s" "$s" "$ICON_PNG" --out "$out" >/dev/null
    else python3 -c 'import sys;from PIL import Image;Image.open(sys.argv[1]).resize((int(sys.argv[2]),)*2,Image.LANCZOS).save(sys.argv[3])' "$ICON_PNG" "$s" "$out"; fi
  elif command -v rsvg-convert >/dev/null; then rsvg-convert -w "$s" -h "$s" "$src" -o "$out"
  elif command -v magick >/dev/null; then magick -background none -density 384 "$src" -resize "${s}x$s" "$out"
  else
    echo "no SVG renderer (rsvg-convert or ImageMagick); set ICON_PNG to a 1024 px render of $src" >&2; exit 1
  fi
}
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
case $kind in
  check)
    grep -q '<svg' "$src" || { echo "$src is not an SVG" >&2; exit 1; }
    render 64 "$tmp/t.png"; echo "$src ok" ;;
  png) render "${2:?SIZE}" "${3:?OUT.png}"; echo "$3" ;;
  ico)
    out=${2:?OUT.ico}; files=()
    for s in 16 24 32 48 64 128 256; do render $s "$tmp/$s.png"; files+=("$tmp/$s.png"); done
    if command -v magick >/dev/null; then magick "${files[@]}" "$out"
    else python3 - "$out" "${files[@]}" <<'PY'
import sys
from PIL import Image
ims = [Image.open(f).convert("RGBA") for f in sys.argv[2:]]
ims[-1].save(sys.argv[1], sizes=[i.size for i in ims], append_images=ims[:-1])
PY
    fi
    echo "$out" ;;
  icns)
    out=${2:?OUT.icns}; set_dir="$tmp/muninn.iconset"; mkdir -p "$set_dir"
    for s in 16 32 128 256 512; do
      render $s "$set_dir/icon_${s}x${s}.png"
      render $((s * 2)) "$set_dir/icon_${s}x${s}@2x.png"
    done
    iconutil -c icns "$set_dir" -o "$out"
    echo "$out" ;;
  *) echo "unknown kind $kind" >&2; exit 1 ;;
esac
