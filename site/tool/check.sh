#!/usr/bin/env bash
# Check a built site (build/jaspr): one page with the banner, the theme and every link.
set -euo pipefail
cd "$(dirname "$0")/.."
out=build/jaspr
html="$out/index.html"
test -f "$html" || { echo "no $html: run jaspr build first" >&2; exit 1; }
test -f "$out/styles.css" || { echo "no styles.css in $out" >&2; exit 1; }
for want in '███╗   ███╗' 'https://github.com/WeirdBrains/muninn/releases' \
  'https://github.com/WeirdBrains/muninn/tree/main/docs' 'docs/install.md' 'id="install"' 'styles.css' \
  'class="hero-shot"' 'id="screenshots"' 'href="#screenshots"' 'loading="lazy"'; do
  grep -qF -- "$want" "$html" || { echo "index.html is missing: $want" >&2; exit 1; }
done
grep -qF -- '--header: #7aa2f7' "$out/styles.css" || { echo "styles.css lost the theme" >&2; exit 1; }
# Every screenshot the page shows must be in the build, with alt text.
srcs=$(grep -oE 'src="screenshots/[^"]+\.png"' "$html" | sed 's/^src="//;s/"$//' | sort -u)
[ -n "$srcs" ] || { echo "index.html shows no screenshots" >&2; exit 1; }
for f in $srcs; do test -f "$out/$f" || { echo "missing image: $out/$f" >&2; exit 1; }; done
if tr '\n' ' ' <"$html" | grep -oE '<img [^>]*>' | grep -v 'alt="[^"]' >/dev/null; then echo "an img has no alt text" >&2; exit 1; fi
hero=$(tr '\n' ' ' <"$html" | grep -oE 'class="hero-shot">[[:space:]]*<img [^>]*src="[^"]+"' | grep -oE 'screenshots/[^"]+' | head -1 || true)
[ -n "$hero" ] || { echo "no hero image under the header" >&2; exit 1; }
pages=$(find "$out" -name '*.html' | wc -l)
[ "$pages" -eq 1 ] || { echo "expected one page, found $pages" >&2; exit 1; }
echo "site ok: $html ($(echo "$srcs" | wc -l) screenshots, hero $hero)"
