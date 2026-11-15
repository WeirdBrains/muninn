#!/usr/bin/env bash
# Install (or update) the exchange-bars ingest in a Muninn home. Idempotent.
#   MNNN=/path/to/mnnn  MUNINN_HOME=/path/to/home  ./install.sh
# MNNN defaults to `mnnn`; MUNINN_HOME is optional (else mnnn's default home). Every call is --non-interactive,
# so mnnn reads MUNINN_PASSWORD from the environment; without it, this script asks once (silently) and passes it on.
# Env knobs: SYMBOLS (default BTC/USD,ETH/USD for a NEW install; on an existing install it changes the schedules'
# symbols in place, cursors kept, and is left alone when you do not set it), SKIP_SELFCHECK=1,
# START_AT="YYYY-MM-DD HH:MM:SS" (UTC, tests).
# Needs mnnn 2.26.0 or later (conditional rules on connector.run, schedule
# edit --option, schedule state). Re-running upgrades an install made by the first version of this connector
# (removes its `bars-ok` rule set and `ok_ruleset` option).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MNNN="${MNNN:-mnnn}"
SYMBOLS_GIVEN="${SYMBOLS+yes}"   # was SYMBOLS set explicitly? (an existing install keeps its symbols otherwise)
SYMBOLS="${SYMBOLS:-BTC/USD,ETH/USD}"
args=(--non-interactive)
[ -n "${MUNINN_HOME:-}" ] && args=(--home "$MUNINN_HOME" --non-interactive)
m() { "$MNNN" "${args[@]}" "$@"; }
say() { printf '  %s\n' "$*"; }
die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

command -v "$MNNN" >/dev/null 2>&1 || [ -x "$MNNN" ] || die "mnnn not found (set MNNN=/path/to/mnnn)"
echo "mnnn: $("$MNNN" --version)  (needs 2.26.0 or later)"
m schedule state --help >/dev/null 2>&1 || die "this mnnn has no 'schedule state': use mnnn 2.26.0 or later"
edit_help="$(m schedule edit --help 2>/dev/null || true)"; grep -q -- '--unset-option' <<<"$edit_help" || die "this mnnn's 'schedule edit' has no --option/--unset-option: use mnnn 2.26.0 or later"
command -v cargo >/dev/null 2>&1 || die "cargo (a Rust toolchain) is needed: code spaces are compiled locally"
m code check >/dev/null 2>&1 || say "warning: 'mnnn code check' failed; code spaces may not build"

# --- password: from MUNINN_PASSWORD, else asked once here (silent) and passed to every mnnn call below --------------
if [ -z "${MUNINN_PASSWORD:-}" ]; then
  { : </dev/tty; } 2>/dev/null || die "set MUNINN_PASSWORD (there is no terminal to ask on)"
  while IFS= read -r -s -t 1 _ </dev/tty; do :; done  # drop keystrokes already waiting (a terminal Run button's extra Enter)
  printf 'Muninn password: ' >/dev/tty; IFS= read -r -s MUNINN_PASSWORD </dev/tty; printf '\n' >/dev/tty
  [ -n "$MUNINN_PASSWORD" ] || die "no password given"
  export MUNINN_PASSWORD
fi
m schedule list >/dev/null 2>&1 || die "could not unlock the database: wrong password? (nothing was changed)"

# --- existence checks (capture the listing first: `cmd | grep -q` can SIGPIPE `cmd` and fail under pipefail) ----------
has_json() { local out; out=$(m "$@" --json 2>/dev/null) || return 1; grep -q "\"name\": *\"$NAME\"" <<<"$out"; }
has_code()     { NAME="$1" has_json code list; }
has_space()    { NAME="$1" has_json spaces list; }
has_pipeline() { NAME="$1" has_json pipeline list; }
has_schedule() { NAME="$1" has_json schedule list; }
has_rule()     { NAME="$1" has_json rule list; }
has_source()   { local out; out=$(m sources list 2>/dev/null) || return 1; cut -f1 <<<"$out" | grep -qx "$1"; }
has_ruleset()  { local out; out=$(m ruleset list 2>/dev/null) || return 1; grep -qE "(^|[[:space:]])$1([[:space:]]|\$)" <<<"$out"; }

# code space NAME from FILE (kind KIND); creates, updates when the source changed, approves
ensure_space() {
  local name="$1" file="$2" kind="$3" changed=0
  if has_code "$name"; then
    if [ "$(m code show "$name" --source)" != "$(cat "$file")" ]; then
      m code edit "$name" -f "$file" >/dev/null; changed=1; say "code $name: source updated"
    else say "code $name: up to date"; fi
  else
    m code create "$name" --lang rust --source "$(cat "$file")" >/dev/null; changed=1; say "code $name: created"
  fi
  if has_space "$name"; then say "space $name: exists"
  else m spaces create "$name" --kind "$kind" --main "$name" >/dev/null; changed=1; say "space $name: created"; fi
  # approvals are cheap and idempotent; they cover a changed source too
  m -y code approve "$name" >/dev/null
  m -y spaces approve "$name" >/dev/null
  return 0
}

echo "1. built-in parsers (to-bars) ..."
if has_code to-bars && has_space to-bars; then say "to-bars: present"; else m code parsers --install >/dev/null; say "installed built-in parsers"; fi
m -y code approve to-bars >/dev/null; m -y spaces approve to-bars >/dev/null

echo "2. code spaces ..."
ensure_space exchange-bars         "$HERE/src/exchange-bars.rs"          ingest-connector
ensure_space parse-coinbase-candles "$HERE/src/parse-coinbase-candles.rs" parser
ensure_space parse-kraken-ohlc      "$HERE/src/parse-kraken-ohlc.rs"      parser

echo "3. sources and pipelines ..."
# The pipeline's input must be `-` (the frame host.pipe() hands over: ONLY this run's raw items). A `source:NAME` node
# would re-read EVERY item ever stored in the source on each run, duplicating work and growing.
for ex in coinbase kraken; do
  has_source "$ex" && say "source $ex: exists" || { m sources add "$ex" >/dev/null; say "source $ex: created"; }
  parser=$([ "$ex" = coinbase ] && echo parse-coinbase-candles || echo parse-kraken-ohlc)
  if has_pipeline "$ex-5m"; then say "pipeline $ex-5m: exists"
  else
    m pipeline create "$ex-5m" --description "$ex 5m bars: connector items (stdin) > parser > to-bars > model:bars" \
      --node "src=-" --node "p=space:$parser" --node "b=space:to-bars?duration=5m&source=$ex" --node "out=model:bars" \
      --edge 'src>p>b>out' >/dev/null
    say "pipeline $ex-5m: created"
  fi
done

echo "4. gap alarm ..."
# One rule set, one conditional rule: when a run's connector.run action says landed = 0 (the run stored and piped
# nothing: no symbol had a bar newer than its cursor), notify. `landed`, not `inserted`: after a failed run the retry
# re-lands bars that are already stored (inserted 0, landed > 0), which is not a gap. The connector always runs the rule set; the rule's `on` and `if` decide.
GAP_IF="landed = 0"
GAP_THEN='notify "{name}: nothing new" body="run {run}: stored {stored}, landed {landed}" level=warn'
GAP_DESC="alarm: a connector run found no new closed bar (landed = 0)"
if has_ruleset bars-gap; then say "ruleset bars-gap: exists"; else m ruleset create bars-gap >/dev/null; say "ruleset bars-gap: created"; fi
if has_rule bars-gap; then
  # same rule? (on, if, notify title/body/level). An install by the first version had `--on schedule`, no condition: fix in place.
  if m rule show bars-gap --json | python3 -c '
import json,sys
sp=json.load(sys.stdin)["rule"]["spec"]
t=(sp["then"] or [{}])[0]
ok=(sp["on"]==["connector.run"] and [(c["field"],c["op"],str(c["value"])) for c in sp["if"]]==[("landed","=","0")]
    and len(sp["then"])==1 and t.get("do")=="notify" and t.get("title")=="{name}: nothing new"
    and t.get("body")=="run {run}: stored {stored}, landed {landed}" and t.get("level")=="warn")
sys.exit(0 if ok else 1)'; then say "rule bars-gap: exists"
  else
    m rule edit bars-gap --on connector.run --if "$GAP_IF" --then "$GAP_THEN" --description "$GAP_DESC" >/dev/null
    say "rule bars-gap: updated (on connector.run, if $GAP_IF)"
  fi
  if ! m ruleset list --json | python3 -c '
import json,sys
rs=[r for r in json.load(sys.stdin) if r["name"]=="bars-gap"][0]
sys.exit(0 if any(r["name"]=="bars-gap" for r in rs["rules"]) else 1)'; then
    m ruleset add bars-gap bars-gap >/dev/null; say "rule bars-gap: added to ruleset bars-gap"
  fi
else
  m rule create bars-gap --ruleset bars-gap --on connector.run --if "$GAP_IF" --then "$GAP_THEN" --description "$GAP_DESC" >/dev/null
  say "rule bars-gap: created in ruleset bars-gap (on connector.run, if $GAP_IF)"
fi

echo "5. schedules (a new one: first run at the next 5-minute boundary + 20 s, UTC) ..."
if [ -n "${START_AT:-}" ]; then start="$START_AT"
else
  now=$(date -u +%s); next=$(( (now / 300 + 1) * 300 + 20 ))
  start=$(date -u -r "$next" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date -u -d "@$next" '+%Y-%m-%d %H:%M:%S')
fi
# What an existing schedule needs to match, as `schedule edit` arguments, one per line (empty: nothing to change).
# `schedule edit` keeps the schedule's id, its runs and its state (the cursors); options are set one at a time.
sched_args() { # $1 = exchange
  EX="$1" WANT_SYMBOLS="$SYMBOLS" SYMBOLS_GIVEN="$SYMBOLS_GIVEN" SCHED="$(m schedule list --json)" SPACES="$(m spaces list --json)" \
  RS="$(m ruleset list --json)" PIPES="$(m pipeline list --json)" SRCS="$(m sources list)" python3 - <<'EOF_PY'
import json, os
ex = os.environ["EX"]
s = next(x for x in json.loads(os.environ["SCHED"]) if x["name"] == ex + "-5m")
nid = lambda var, n: next(x["id"] for x in json.loads(os.environ[var]) if x["name"] == n)
src = next(l.split("\t")[3] for l in os.environ["SRCS"].splitlines() if l.split("\t")[0] == ex)
o = s.get("options") or {}
out = []
if o.get("exchange") != ex: out += ["--option", "exchange=" + ex]
if os.environ["SYMBOLS_GIVEN"] == "yes" and o.get("symbols") != os.environ["WANT_SYMBOLS"]: out += ["--option", "symbols=" + os.environ["WANT_SYMBOLS"]]
if "ok_ruleset" in o: out += ["--unset-option", "ok_ruleset"]   # first version of this connector
if s.get("plugin_id") != nid("SPACES", "exchange-bars"): out += ["--plugin", "exchange-bars"]
if s.get("source_id") != src: out += ["--source", ex]
if s.get("pipeline_id") != nid("PIPES", ex + "-5m"): out += ["--pipeline", ex + "-5m"]
if s.get("ruleset_id") != nid("RS", "bars-gap"): out += ["--ruleset", "bars-gap"]
print("\n".join(out))
EOF_PY
}
for ex in coinbase kraken; do
  if has_schedule "$ex-5m"; then
    edit=(); while IFS= read -r a; do [ -n "$a" ] && edit+=("$a"); done < <(sched_args "$ex")
    if [ "${#edit[@]}" -eq 0 ]; then say "schedule $ex-5m: up to date"
    else
      m schedule edit "$ex-5m" "${edit[@]}" >/dev/null
      say "schedule $ex-5m: edited in place (same id, cursors kept): ${edit[*]}"
    fi
  else
    m schedule add "$ex-5m" 5m --plugin exchange-bars --source "$ex" --pipeline "$ex-5m" --connector-ruleset bars-gap \
      --option "exchange=$ex" --option "symbols=$SYMBOLS" --start-at "$start" --tz UTC --past next-interval >/dev/null
    say "schedule $ex-5m: created, starts $start UTC"
  fi
done

# first version of this connector had an empty `bars-ok` rule set (named by an `ok_ruleset` option, removed above):
# no schedule runs it, so it can go. (Removal is after the schedules are edited, in case one still referenced it.)
if has_ruleset bars-ok; then m -y ruleset remove bars-ok >/dev/null; say "ruleset bars-ok: removed (no longer used)"; fi

if [ "${SKIP_SELFCHECK:-0}" != 1 ]; then
  echo "6. self-check: connector dry run (1 request to api.kraken.com, nothing stored) ..."
  out=$(m -y spaces run exchange-bars --connector-dry-run --option exchange=kraken --option symbols=BTC/USD 2>&1) || { echo "$out"; die "self-check failed: the connector did not run; see the output above"; }
  echo "$out" | sed 's/^/    /' | head -30
  echo "$out" | grep -q "fetch" || die "self-check: no fetch call listed in the dry run"
  say "OK if the table shows BTC/USD with status ok/up to date and the calls list fetch and store_raw."
  say "A dry run only RECORDS pipe(); whether the pipeline's space: steps run shows on the first"
  say "real run:  $MNNN schedule run-now kraken-5m  then  schedule runs kraken-5m  (status ok, not 'failed: ... space')."
fi
echo "done."
