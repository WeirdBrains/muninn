#!/usr/bin/env bash
# Cross-build Windows binaries from Linux with llvm-mingw (no system packages):
#   LLVM_MINGW=~/.local/opt/llvm-mingw-<ver>-ucrt-ubuntu-22.04-x86_64 scripts/cross-windows.sh [x86_64|aarch64 ...]
# Needs: rustup target add x86_64-pc-windows-gnullvm aarch64-pc-windows-gnullvm
# Output: target/<arch>-pc-windows-gnullvm/release/mnnn.exe, then dist/*.zip via scripts/dist.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${LLVM_MINGW:?set LLVM_MINGW to an llvm-mingw toolchain directory}"
export PATH="$LLVM_MINGW/bin:$HOME/.cargo/bin:$PATH"   # rustup cargo has the windows targets
for arch in "${@:-x86_64 aarch64}"; do
  for a in $arch; do
    t="$a-pc-windows-gnullvm"; T="$(echo "$t" | tr a-z- A-Z_)"; u="${t//-/_}"
    export "CARGO_TARGET_${T}_LINKER=$a-w64-mingw32-clang"
    # static CRT pieces (libunwind) so mnnn.exe needs only DLLs that ship with Windows
    export "CARGO_TARGET_${T}_RUSTFLAGS=-C target-feature=+crt-static"
    export "CC_$u=$a-w64-mingw32-clang" "CXX_$u=$a-w64-mingw32-clang++" "AR_$u=llvm-ar" "RANLIB_$u=llvm-ranlib"
    if [[ $a == aarch64 ]]; then
      # openssl-src has no mapping for aarch64 mingw: build OpenSSL (mingwarm64) from the
      # vendored openssl-src sources once and link it statically instead.
      ossl="$PWD/target/openssl-$t"
      if [[ ! -f "$ossl/lib/libcrypto.a" ]]; then
        cargo fetch --locked >/dev/null
        srcdir="$(ls -d "${CARGO_HOME:-$HOME/.cargo}"/registry/src/*/openssl-src-300.*/openssl | sort -V | tail -1)"
        bld="$PWD/target/openssl-build-$t"; rm -rf "$bld"; cp -r "$srcdir" "$bld"
        (cd "$bld" && ./Configure mingwarm64 no-shared no-module no-legacy no-dso no-engine no-apps no-tests no-docs no-ui-console --prefix="$ossl" --libdir=lib \
            CC=aarch64-w64-mingw32-clang AR=llvm-ar RANLIB=llvm-ranlib RC=aarch64-w64-mingw32-windres \
          && make -j"$(nproc)" build_libs >/dev/null && make install_dev >/dev/null)
      fi
      export OPENSSL_NO_VENDOR=1 OPENSSL_STATIC=1 OPENSSL_DIR="$ossl"
    fi
    cargo build --locked --release --target "$t"
    unset OPENSSL_NO_VENDOR OPENSSL_STATIC OPENSSL_DIR
    scripts/dist.sh "$t"
  done
done
