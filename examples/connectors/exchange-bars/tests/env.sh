# Test harness env (throwaway home only). Source from OUT: . ./env.sh
OUT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"   # the tests/ folder
export B="${MNNN:-$(command -v mnnn)}"   # mnnn 2.26.0 or later
export H="$OUT/test-home/${HOME_NAME:-home}"
export MUNINN_HOME="$H"
[ -f "$OUT/test-home/.pw" ] || { mkdir -p "$OUT/test-home"; openssl rand -hex 16 > "$OUT/test-home/.pw"; }  # THROWAWAY password for the test home only
export MUNINN_PASSWORD="$(cat "$OUT/test-home/.pw")"
m() { [ "$H" = "$HOME/.muninn" ] && { echo REFUSE; return 1; }; "$B" --home "$H" --non-interactive "$@"; }
