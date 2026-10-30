#!/bin/sh
# Dev only: regenerate tests/python/golden with the reference rythonc 1.1.0 (CPython-based).
# Builds it from the cargo registry source into target/rythonc-ref; needs libpython.
set -eu
cd "$(dirname "$0")/.."
src=$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/rythonc-1.1.0 | head -1)
cargo build -q --release --locked --manifest-path "$src/Cargo.toml" --target-dir target/rythonc-ref
ry=target/rythonc-ref/release/rythonc
work=target/py-golden
rm -rf "$work" && mkdir -p "$work"
rm -f tests/python/golden/*
for f in tests/python/rythonc/*.py vendor/python-ast/pytests/*.py; do
  name=$(basename "$(dirname "$f")")_$(basename "$f" .py)
  # rythonc derives the module name from the path: always compile as user.py like Muninn
  cp "$f" "$work/user.py"
  if (cd "$work" && "../../$ry" user.py -o out.rs) > "$work/log" 2>&1; then
    cp "$work/out.rs" "tests/python/golden/$name.rs"
  else
    : > "tests/python/golden/$name.err"
  fi
done
ls tests/python/golden | wc -l
