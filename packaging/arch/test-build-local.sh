#!/usr/bin/env bash
# Check build-local.sh installs whether or not namcap is present, using stub makepkg, sudo
# and namcap on PATH (nothing is built or installed). Run: packaging/arch/test-build-local.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
mk() { printf '#!/usr/bin/env bash\n%s\n' "$2" > "$tmp/$1/$3"; chmod +x "$tmp/$1/$3"; }
for d in with without failing; do
  mkdir -p "$tmp/$d"
  mk "$d" 'touch "${PKGDEST:-.}/muninn-9.9.9-1-x86_64.pkg.tar.zst" "${PKGDEST:-.}/muninn-debug-9.9.9-1-x86_64.pkg.tar.zst"' makepkg
  mk "$d" 'echo "SUDO $*" >> "$LOG"' sudo
done
mk with 'echo "NAMCAP $*" >> "$LOG"' namcap
mk failing 'echo "NAMCAP $*" >> "$LOG"; exit 1' namcap
# PATH without any real namcap: only the stub dir plus coreutils/bash locations
base="/usr/bin:/bin"; [[ -x /usr/bin/namcap ]] && { mkdir -p "$tmp/core"; for c in bash env sed rm mkdir touch cat dirname head; do ln -sf "$(command -v $c)" "$tmp/core/$c"; done; base="$tmp/core"; }
fail=0; export BUILD_LOCAL_OUT="$tmp/out"
for d in without with failing; do
  export LOG="$tmp/$d.log"; : > "$LOG"
  PATH="$tmp/$d:$base" bash "$here/build-local.sh" --source > "$tmp/$d.out" 2>&1 || { echo "FAIL $d: exit $?"; cat "$tmp/$d.out"; fail=1; continue; }
  grep -q '^SUDO pacman -U --needed .*/muninn-9.9.9-1-x86_64.pkg.tar.zst$' "$LOG" || { echo "FAIL $d: install not run"; cat "$tmp/$d.out" "$LOG"; fail=1; continue; }
  grep -q debug "$LOG" && { echo "FAIL $d: debug package installed"; fail=1; continue; }
  case $d in without) grep -q NAMCAP "$LOG" && { echo "FAIL $d: namcap ran"; fail=1; continue; } ;;
             *) grep -q NAMCAP "$LOG" || { echo "FAIL $d: namcap not run"; fail=1; continue; } ;; esac
  echo "ok   $d namcap: installs"
done
export LOG="$tmp/n.log"; : > "$LOG"
PATH="$tmp/without:$base" bash "$here/build-local.sh" --source --no-install >/dev/null 2>&1
grep -q SUDO "$LOG" && { echo "FAIL --no-install installed"; fail=1; } || echo "ok   --no-install: builds only"
exit $fail
