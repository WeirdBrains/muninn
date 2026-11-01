#!/usr/bin/env bash
# Feature screenshots for the website and the README, redone each release.
#
# 1. The ignored test `tui::screenshots::feature_screenshots` drives the TUI from a scratch
#    home (a temp dir with sample data; never ~/.muninn) and writes each screen as HTML
#    (coloured cells in the theme) to target/screenshots/, with list.tsv (name, caption).
# 2. Headless Chromium renders each page to PNG; ImageMagick shrinks it to a 256-colour
#    PNG in site/web/screenshots/.
# 3. site/lib/shots.dart (the site's list) and the README's screenshot block are written
#    from list.tsv.
#
# Needs: cargo, chromium, magick (ImageMagick). HERO names the hero shot.
set -euo pipefail
cd "$(dirname "$0")/.."
HERO=${HERO:-pipeline-live}
out=site/web/screenshots
cargo test -q --lib screenshots -- --ignored >/dev/null
mkdir -p "$out"
rm -f "$out"/*.png
profile=$(mktemp -d)
trap 'rm -rf "$profile"' EXIT
while IFS=$'\t' read -r name caption; do
  chromium --headless=new --no-sandbox --disable-gpu --hide-scrollbars --user-data-dir="$profile" \
    --window-size=1464,890 --screenshot="target/screenshots/$name.png" "file://$PWD/target/screenshots/$name.html" >/dev/null 2>&1
  magick "target/screenshots/$name.png" -strip -colors 256 "PNG8:$out/$name.png"
done < target/screenshots/list.tsv

dart_str() { sed -e "s/\\\\/\\\\\\\\/g" -e "s/'/\\\\'/g" -e 's/\$/\\$/g'; }
{
  echo "// Written by scripts/screenshots.sh from the TUI screenshot test; do not edit."
  echo "// The hero shot, then every feature shot: (file, caption)."
  echo "const heroShot = ('screenshots/$HERO.png', '$(grep -P "^$HERO\t" target/screenshots/list.tsv | cut -f2 | dart_str)');"
  echo "const shots = ["
  while IFS=$'\t' read -r name caption; do
    echo "  ('screenshots/$name.png', '$(printf %s "$caption" | dart_str)'),"
  done < target/screenshots/list.tsv
  echo "];"
} > site/lib/shots.dart
command -v dart >/dev/null && dart format site/lib/shots.dart >/dev/null

block=$(mktemp)
{
  echo "<!-- screenshots: written by scripts/screenshots.sh -->"
  echo "![$(grep -P "^$HERO\t" target/screenshots/list.tsv | cut -f2)](site/web/screenshots/$HERO.png)"
  echo
  echo "<details><summary>Every feature (${HERO} above; $(wc -l < target/screenshots/list.tsv) screens)</summary>"
  echo
  while IFS=$'\t' read -r name caption; do
    printf '**%s**\n\n![%s](site/web/screenshots/%s.png)\n\n' "$caption" "$caption" "$name"
  done < target/screenshots/list.tsv
  echo "</details>"
  echo "<!-- /screenshots -->"
} > "$block"
python3 - "$block" <<'PY'
import re, sys
block = open(sys.argv[1]).read().rstrip("\n")
s = open("README.md").read()
pat = re.compile(r"<!-- screenshots: written by scripts/screenshots.sh -->.*?<!-- /screenshots -->", re.S)
if pat.search(s):
    s = pat.sub(lambda m: block, s)
else:
    s = s.replace("\n## Install\n", "\n## Screenshots\n\n" + block + "\n\n## Install\n", 1)
open("README.md", "w").write(s)
PY
rm -f "$block"
du -ch "$out"/*.png | tail -1
ls "$out" | wc -l
