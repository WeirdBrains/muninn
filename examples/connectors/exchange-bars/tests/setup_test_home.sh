#!/usr/bin/env bash
# Create a THROWAWAY Muninn home under OUT/test-home/ (never ~/.muninn). Usage: MNNN=/path/to/mnnn ./setup_test_home.sh (HOME_NAME=x for another home)
set -euo pipefail
cd "$(dirname "$0")"; . ./env.sh
case "$H" in "$HOME/.muninn"|"$HOME/.muninn/"*|*"Containers/com.weirdbrains.muninn"*) echo REFUSE; exit 1;; esac
[ ! -f "$H/keys/envelope.toml" ] || { echo "home $H already exists"; exit 1; }
mkdir -p "$H"
setprefs() { # no network autostart, no mDNS
  python3 - "$H/preferences.toml" <<'PY'
import re,sys
p=sys.argv[1]; s=open(p).read()
if "[net]" not in s: s+="\n[net]\n"
sec=re.search(r"\[net\]\n(.*?)(?=\n\[|\Z)",s,re.S)
body=sec.group(1)
for k in ("autostart","mdns"):
    body=re.sub(r"(?m)^%s = .*$"%k,"%s = false"%k,body) if re.search(r"(?m)^%s ="%k,body) else "%s = false\n"%k+body
s=s[:sec.start(1)]+body+s[sec.end(1):]
open(p,"w").write(s)
PY
}
printf '[net]\nautostart = false\nmdns = false\n' > "$H/preferences.toml"
m keys init --json >/dev/null
setprefs   # keys init rewrites the file: set again
grep -A20 '^\[net\]' "$H/preferences.toml" | grep -E '^(autostart|mdns)'
