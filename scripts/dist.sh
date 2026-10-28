#!/usr/bin/env bash
# Package a prebuilt binary as a release tarball (zip on Windows targets):
#   dist/muninn-<ver>-<target>.tar.gz  containing  muninn-<ver>-<target>/{mnnn,man/,docs/,README.md,CHANGELOG.md,LICENSE}
# Usage: scripts/dist.sh [<target-triple>] [<path-to-built-binary>]
# Defaults: host triple, target/release/mnnn (or target/<triple>/release/mnnn).
set -euo pipefail
cd "$(dirname "$0")/.."
ver="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
target="${1:-$(rustc -vV | sed -n 's/host: //p')}"
exe=mnnn; [[ "$target" == *windows* ]] && exe=mnnn.exe
bin="${2:-}"
if [[ -z "$bin" ]]; then
  bin="target/$target/release/$exe"; [[ -f "$bin" ]] || bin="target/release/$exe"
fi
name="muninn-$ver-$target"
stage="dist/$name"
rm -rf "$stage" && mkdir -p "$stage"
install -m755 "$bin" "$stage/$exe"
cp -r man docs README.md CHANGELOG.md LICENSE "$stage/"
rm -rf "$stage/docs/historical"
cd dist
if [[ "$target" == *windows* ]]; then
  rm -f "$name.zip"; zip -qr "$name.zip" "$name"; echo "dist/$name.zip"
else
  tar -czf "$name.tar.gz" --owner=0 --group=0 "$name"; echo "dist/$name.tar.gz"
fi
rm -rf "$name"
