#!/usr/bin/env bash
# Print the release version: [workspace.package] version in the root Cargo.toml, the one
# place it is set (every crate uses version.workspace = true; scripts/weirdnet_release.py bumps it, see release.toml).
# Usage: scripts/version.sh [path/to/Cargo.toml]
set -euo pipefail
toml=${1:-"$(dirname "$0")/../Cargo.toml"}
awk '/^\[/{f=($0=="[workspace.package]")} f&&/^version[[:space:]]*=/{sub(/^[^"]*"/,""); sub(/".*/,""); print; exit}' "$toml"
