#!/usr/bin/env bash
# Check a built site (build/jaspr): one page with the banner, the theme and every link.
set -euo pipefail
cd "$(dirname "$0")/.."
out=build/jaspr
html="$out/index.html"
test -f "$html" || { echo "no $html: run jaspr build first" >&2; exit 1; }
test -f "$out/styles.css" || { echo "no styles.css in $out" >&2; exit 1; }
for want in '███╗   ███╗' 'https://github.com/WeirdBrains/muninn/releases' \
  'https://github.com/WeirdBrains/muninn/tree/main/docs' 'docs/install.md' 'id="install"' 'styles.css'; do
  grep -qF -- "$want" "$html" || { echo "index.html is missing: $want" >&2; exit 1; }
done
grep -qF -- '--header: #7aa2f7' "$out/styles.css" || { echo "styles.css lost the theme" >&2; exit 1; }
pages=$(find "$out" -name '*.html' | wc -l)
[ "$pages" -eq 1 ] || { echo "expected one page, found $pages" >&2; exit 1; }
echo "site ok: $html"
