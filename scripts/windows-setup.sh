#!/usr/bin/env bash
# Build the Windows installers (muninn-<ver>-<arch>-windows-setup.exe) with Inno Setup on a
# Windows host reached over ssh, from the release zips that scripts/cross-windows.sh makes.
#   scripts/windows-setup.sh [x86_64] [aarch64]
# Environment: DIST (default dist) holds muninn-<ver>-<arch>-pc-windows-gnullvm.zip and
# receives the setup exes; WIN_SSH (default m3talsmith@127.0.0.1) and WIN_PORT (2222, the
# dockurr VM on alien-blue); ISCC (default %LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe).
# The Windows host needs OpenSSH server and Inno Setup 6.3+ (winget install JRSoftware.InnoSetup).
set -euo pipefail
cd "$(dirname "$0")/.."
ver=${VER:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
dist=${DIST:-dist}
host=${WIN_SSH:-m3talsmith@127.0.0.1}
port=${WIN_PORT:-2222}
iscc=${ISCC:-%LOCALAPPDATA%\\Programs\\Inno Setup 6\\ISCC.exe}
archs=("${@:-x86_64}")
[[ $# -gt 0 ]] || archs=(x86_64 aarch64)
work="muninn-setup-$ver"
win() { ssh -q -p "$port" "$host" "$@"; }
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$work/out"
for a in "${archs[@]}"; do
  z="$dist/muninn-$ver-$a-pc-windows-gnullvm.zip"
  [[ -f $z ]] || { echo "missing $z (run scripts/cross-windows.sh $a)" >&2; exit 1; }
  unzip -q "$z" -d "$stage/$work/$a"
done
cp packaging/windows/muninn.iss packaging/windows/muninn-launch.cmd assets/icons/muninn.ico "$stage/$work/"
win "if exist %USERPROFILE%\\$work rmdir /s /q %USERPROFILE%\\$work"
scp -q -P "$port" -r "$stage/$work" "$host:"
for a in "${archs[@]}"; do
  src="%USERPROFILE%\\$work\\$a\\muninn-$ver-$a-pc-windows-gnullvm"
  win "cd /d %USERPROFILE%\\$work && \"$iscc\" /Q /DAppVersion=$ver /DArch=$a \"/DSrcDir=$src\" \"/DOutDir=%USERPROFILE%\\$work\\out\" muninn.iss"
  scp -q -P "$port" "$host:$work/out/muninn-$ver-$a-windows-setup.exe" "$dist/"
  echo "$dist/muninn-$ver-$a-windows-setup.exe"
done
win "rmdir /s /q %USERPROFILE%\\$work"
