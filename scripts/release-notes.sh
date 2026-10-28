#!/usr/bin/env bash
# Print the CHANGELOG.md section for a version (release notes). Usage: scripts/release-notes.sh 1.6.0
set -euo pipefail
cd "$(dirname "$0")/.."
# Section body only, without the heading and leading/trailing blank lines.
awk -v v="$1" '$0 ~ "^## \\[" {p = ($0 ~ "^## \\[" v "\\]"); next} p' CHANGELOG.md |
  sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
