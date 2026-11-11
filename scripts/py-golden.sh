#!/bin/sh
# Dev only: regenerate crates/weirdnet-code-spaces/tests/python/golden with the reference
# rythonc 1.1.0 (CPython-based). Builds it from the cargo registry source into
# target/rythonc-ref; needs libpython.
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)
src=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/rythonc-1.1.0 | head -1)
cargo build -q --release --locked --manifest-path "$src/Cargo.toml" --target-dir target/rythonc-ref
ry=$root/target/rythonc-ref/release/rythonc
work=$root/target/py-golden
corpus=crates/weirdnet-code-spaces/tests/python
rm -rf "$work" && mkdir -p "$work"
rm -f "$corpus"/golden/*
for f in "$corpus"/rythonc/*.py "$corpus"/pytests/*.py; do
  name=$(basename "$(dirname "$f")")_$(basename "$f" .py)
  # rythonc derives the module name from the path: always compile as user.py
  cp "$f" "$work/user.py"
  if (cd "$work" && "$ry" user.py -o out.rs) > "$work/log" 2>&1; then
    cp "$work/out.rs" "$corpus/golden/$name.rs"
  else
    : > "$corpus/golden/$name.err"
  fi
done
ls "$corpus"/golden | wc -l
