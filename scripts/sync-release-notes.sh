#!/usr/bin/env bash
# Rewrite the notes of published GitHub releases from CHANGELOG.md (same text the release
# workflow uses) and optionally mark the highest tag as Latest. Immutable releases allow this:
# only their assets and tag are locked.
# Usage: scripts/sync-release-notes.sh [--latest] [--dry-run] [vX.Y.Z ...]   (default: every release)
# Needs gh, logged in with write access (gh auth login).
set -euo pipefail
cd "$(dirname "$0")/.."
latest=0; dry=0; tags=()
for a in "$@"; do
  case "$a" in
    --latest) latest=1 ;;
    --dry-run) dry=1 ;;
    v*) tags+=("$a") ;;
    *) echo "unknown argument: $a" >&2; exit 2 ;;
  esac
done
if [[ ${#tags[@]} -eq 0 ]]; then
  mapfile -t tags < <(gh release list --limit 200 --json tagName --jq '.[].tagName')
fi
tmp="$(mktemp)"; trap 'rm -f "$tmp"' EXIT
for tag in "${tags[@]}"; do
  scripts/release-notes.sh "${tag#v}" > "$tmp"
  if [[ ! -s "$tmp" ]]; then echo "skip $tag: no CHANGELOG section" >&2; continue; fi
  if [[ $dry -eq 1 ]]; then echo "== $tag"; cat "$tmp"; continue; fi
  gh release edit "$tag" --title "Muninn $tag" --notes-file "$tmp" >/dev/null && echo "updated $tag"
done
if [[ $latest -eq 1 ]]; then
  top="$(git tag -l 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -1)"
  if [[ $dry -eq 1 ]]; then echo "would mark $top as Latest"; else gh release edit "$top" --latest >/dev/null && echo "marked $top as Latest"; fi
fi
