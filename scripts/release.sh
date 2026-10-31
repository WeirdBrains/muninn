#!/usr/bin/env bash
# Local release: bump version, regenerate man pages, changelog entry, commit, annotated tag.
# Usage: scripts/release.sh 0.2.0 "One-line tag summary" "Bullet" ["Bullet" ...]
# Bullets go under "### Added" unless prefixed "Fixed: " or "Changed: ". They become the
# GitHub release notes, so write each as: feature, then how to use it (command or key).
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
secs = {"Added": [], "Changed": [], "Fixed": []}
for b in bullets:
    m = re.match(r"(Fixed|Changed): (.*)", b, re.S)
    text = m.group(2) if m else b
    secs[m.group(1) if m else "Added"].append(text[:1].upper() + text[1:])
entry = f"## [{ver}] - {datetime.date.today()}\n\n" + "\n".join(
    f"### {k}\n" + "".join(f"- {b}\n" for b in v) for k, v in secs.items() if v) + "\n"
s = s.replace("## [Unreleased]\n\n", "## [Unreleased]\n\n" + entry, 1)
prev = re.search(r"compare/v([0-9.]+)\.\.\.HEAD", s).group(1)
s = s.replace(f"[Unreleased]: https://github.com/WeirdBrains/muninn/compare/v{prev}...HEAD",
              f"[Unreleased]: https://github.com/WeirdBrains/muninn/compare/v{ver}...HEAD\n"
              f"[{ver}]: https://github.com/WeirdBrains/muninn/compare/v{prev}...v{ver}")
open(p, "w").write(s)
PY
cargo test -q --lib man >/dev/null 2>&1 || true
git add -A
git commit -qm "release: v$ver"
git tag -a "v$ver" -m "v$ver: $summary"
git log --oneline -1
