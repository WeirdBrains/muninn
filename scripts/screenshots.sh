#!/usr/bin/env bash
# TUI screenshots: render target/screenshots/*.html (from the ignored test) to PNG with headless Chromium.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -q --lib screenshots -- --ignored >/dev/null
for h in target/screenshots/*.html; do
  chromium --headless --disable-gpu --hide-scrollbars --window-size=1500,960 --screenshot="${h%.html}.png" "file://$PWD/$h" >/dev/null 2>&1
done
ls target/screenshots/*.png
