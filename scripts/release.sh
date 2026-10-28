#!/usr/bin/env bash
# Local release: bump version, regenerate man pages, changelog entry, commit, annotated tag.
# Usage: scripts/release.sh 0.2.0 "One-line tag summary" "Added bullet" ["Added bullet" ...]
# Never pushes.
set -euo pipefail
cd "$(dirname "$0")/.."
ver="$1"; summary="$2"; shift 2
[[ -z "$(git status --porcelain)" ]] || { echo "working tree not clean" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$ver" >/dev/null && { echo "tag v$ver exists" >&2; exit 1; }
sed -i "0,/^version = \".*\"/s//version = \"$ver\"/" Cargo.toml
# keep packaging versions in step
sed -i "s/^pkgver=.*/pkgver=$ver/" packaging/arch/PKGBUILD packaging/arch/muninn-bin/PKGBUILD
sed -i "0,/^  version \".*\"/s//  version \"$ver\"/" packaging/homebrew/muninn.rb
cargo build -q
cargo run -q -- man --out man >/dev/null
python3 - "$ver" "$@" <<'PY'
import sys, datetime, re
ver, bullets = sys.argv[1], sys.argv[2:]
p = "CHANGELOG.md"; s = open(p).read()
entry = f"## [{ver}] - {datetime.date.today()}\n\n### Added\n" + "".join(f"- {b}\n" for b in bullets) + "\n"
s = s.replace("## [Unreleased]\n\n", "## [Unreleased]\n\n" + entry, 1)
prev = re.search(r"compare/v([0-9.]+)\.\.\.HEAD", s).group(1)
s = s.replace(f"[Unreleased]: https://github.com/kommandosaldat/muninn/compare/v{prev}...HEAD",
              f"[Unreleased]: https://github.com/kommandosaldat/muninn/compare/v{ver}...HEAD\n"
              f"[{ver}]: https://github.com/kommandosaldat/muninn/compare/v{prev}...v{ver}")
open(p, "w").write(s)
PY
cargo test -q --lib man >/dev/null 2>&1 || true
git add -A
git commit -qm "release: v$ver"
git tag -a "v$ver" -m "v$ver: $summary"
git log --oneline -1
