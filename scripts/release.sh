#!/usr/bin/env bash
# Local release: bump the version, regenerate man pages, changelog entry, commit, annotated tag.
# Usage: scripts/release.sh 0.2.0 "One-line tag summary" "Bullet" ["Bullet" ...]
# Bullets go under "### Added" unless prefixed "Fixed: " or "Changed: ". They become the
# GitHub release notes, so write each as: feature, then how to use it (command or key).
#
# The version lives in one place, [workspace.package] in the root Cargo.toml (scripts/version.sh
# reads it). This script moves it and every file that pins it (packaging templates, the site,
# the install guide, the winget template directory), then stages exactly those files by path:
# never `git add -A` or `commit -a`, so it is safe in a shared checkout or a git worktree.
# CARGO_BUILD_JOBS defaults to 8. Never pushes.
set -euo pipefail
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
ver="$1"; summary="$2"; shift 2
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-8}
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || { echo "working tree has uncommitted changes" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$ver" >/dev/null && { echo "tag v$ver exists" >&2; exit 1; }
old=$(scripts/version.sh)
[[ $old != "$ver" ]] || { echo "already at $ver" >&2; exit 1; }
oldre=${old//./\\.}

# The one place: [workspace.package] version.
python3 - "$ver" <<'PY'
import re, sys
p = "Cargo.toml"; s = open(p).read()
s, n = re.subn(r'(?ms)^(\[workspace\.package\]\n(?:(?!\[).*\n)*?)version = "[^"]*"', rf'\g<1>version = "{sys.argv[1]}"', s, count=1)
assert n == 1, "no version in [workspace.package]"
open(p, "w").write(s)
PY

# Files that pin the version (release filenames and URLs included).
pinned=(
  packaging/arch/PKGBUILD
  packaging/arch/muninn-bin/PKGBUILD
  packaging/homebrew/Formula/muninn.rb
  packaging/homebrew/Casks/muninn.rb
  site/lib/app.dart
  docs/install.md
  docs/deploy-relay.md
  packaging/windows/muninn.iss
)
sed -i "s/$oldre/$ver/g" "${pinned[@]}"
# winget: move the template directory to the new version
wd=packaging/winget/manifests/w/WeirdBrains/Muninn
git mv "$wd/$old" "$wd/$ver"
sed -i "s/$oldre/$ver/g" "$wd/$ver"/*.yaml

cargo build -q
rm -f man/*.1
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
# The man staleness check and the version pins (both must pass before tagging).
cargo test -q -p muninn-core --lib man
cargo test -q --test packaging
git add -- Cargo.toml Cargo.lock CHANGELOG.md "${pinned[@]}" "$wd/$ver" man
git diff --cached --stat
git commit -qm "release: v$ver"
git tag -a "v$ver" -m "v$ver: $summary"
git log --oneline -1
