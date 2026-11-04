#!/usr/bin/env bash
# Check a built site (build/jaspr): one page with the raven on top, the theme and every link.
set -euo pipefail
cd "$(dirname "$0")/.."
out=build/jaspr
html="$out/index.html"
test -f "$html" || { echo "no $html: run jaspr build first" >&2; exit 1; }
test -f "$out/styles.css" || { echo "no styles.css in $out" >&2; exit 1; }
for want in '<h1 class="sr-only">Muninn</h1>' 'https://github.com/WeirdBrains/muninn/releases' \
  'href="/docs/"' 'href="/docs/install/"' 'id="install"' 'styles.css' \
  'class="hero-shot"' 'id="screenshots"' 'href="#screenshots"' 'loading="lazy"'; do
  grep -qF -- "$want" "$html" || { echo "index.html is missing: $want" >&2; exit 1; }
done
! grep -qF '███' "$html" || { echo "index.html still has the block-letter title" >&2; exit 1; }
grep -qF -- '--header: #7aa2f7' "$out/styles.css" || { echo "styles.css lost the theme" >&2; exit 1; }
# The raven logo: inline (not <img>) in the hero and every docs header, themed, plus favicons.
grep -qF -- '--muninn-header: var(--header)' "$out/styles.css" || { echo "styles.css does not theme the logo" >&2; exit 1; }
tr '\n' ' ' <"$html" | grep -qE 'class="logo hero-logo"[^>]*>[[:space:]]*<svg' || { echo "no inline raven in the hero" >&2; exit 1; }
for f in favicon.svg favicon-16.png favicon-32.png apple-touch-icon.png; do
  test -f "$out/$f" || { echo "missing $out/$f" >&2; exit 1; }
  grep -qF "href=\"$f\"" "$html" || { echo "index.html does not link $f" >&2; exit 1; }
done
grep -qF '<span class="logo" aria-hidden="true"><svg' "$out/docs/index.html" || { echo "no inline raven in the docs header" >&2; exit 1; }
tr '\n' ' ' <"$html" | grep -qE 'class="logo about-logo"[^>]*>[[:space:]]*<svg' || { echo "no raven in the About footer" >&2; exit 1; }
grep -qF "published by WeirdBrains" "$html" || { echo "no publisher in the About footer" >&2; exit 1; }
# Every screenshot the page shows must be in the build, with alt text.
srcs=$(grep -oE 'src="screenshots/[^"]+\.png"' "$html" | sed 's/^src="//;s/"$//' | sort -u)
[ -n "$srcs" ] || { echo "index.html shows no screenshots" >&2; exit 1; }
for f in $srcs; do test -f "$out/$f" || { echo "missing image: $out/$f" >&2; exit 1; }; done
if tr '\n' ' ' <"$html" | grep -oE '<img [^>]*>' | grep -v 'alt="[^"]' >/dev/null; then echo "an img has no alt text" >&2; exit 1; fi
hero=$(tr '\n' ' ' <"$html" | grep -oE 'class="hero-shot">[[:space:]]*<img [^>]*src="[^"]+"' | grep -oE 'screenshots/[^"]+' | head -1 || true)
[ -n "$hero" ] || { echo "no hero image under the header" >&2; exit 1; }
pages=$(find "$out" -maxdepth 1 -name '*.html' | wc -l)
[ "$pages" -eq 1 ] || { echo "expected one front page, found $pages" >&2; exit 1; }
# Every doc has a page (dart run tool/docs.dart), and every site link in them resolves.
ndocs=0
while IFS= read -r md; do
  rel=${md#../docs/}; slug=${rel%.md}; [ "$rel" = README.md ] && slug=""
  f="$out/docs/${slug:+$slug/}index.html"
  test -f "$f" || { echo "no page for docs/$rel ($f): run dart run tool/docs.dart" >&2; exit 1; }
  ndocs=$((ndocs + 1))
done < <(find ../docs -name '*.md' -not -path '../docs/historical/*')
while IFS= read -r l; do
  page=${l%%:*}; h=${l#*:}; path=${h%%#*}; frag=${h#"$path"}; frag=${frag#\#}
  f="$out$path"; case "$path" in */) f="${f}index.html" ;; esac
  test -f "$f" || { echo "$page links $h, which is not in the build" >&2; exit 1; }
  if [ -n "$frag" ] && [ "${f##*.}" = html ]; then
    grep -qF "id=\"$frag\"" "$f" || { echo "$page links $h: no #$frag there" >&2; exit 1; }
  fi
done < <(grep -oE '(href|src)="/[^"]*"' -r "$out/docs" | sed -E 's/:(href|src)="/:/; s/"$//')
echo "site ok: $html, $ndocs doc pages ($(echo "$srcs" | wc -l) screenshots, hero $hero)"
