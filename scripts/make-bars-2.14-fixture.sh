#!/usr/bin/env bash
# Build tests/fixtures/bars-2.14/: a Muninn 2.14.0 home with representative bars, and
# what 2.14 itself reports about them (the upgrade test compares every row with that).
# Usage: scripts/make-bars-2.14-fixture.sh PATH/TO/mnnn-2.14.0
set -euo pipefail
BIN=${1:?mnnn 2.14.0 binary}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=$ROOT/tests/fixtures/bars-2.14
FX=$ROOT/tests/fixtures
rm -rf "$OUT" && mkdir -p "$OUT"
export MUNINN_HOME=$OUT/home MUNINN_PASSWORD="correct horse battery staple"
m() { "$BIN" --non-interactive -y "$@"; }
m keys init --json > /dev/null
m author add --name Ada > /dev/null
# two sources, two symbols, 5m bars (zero-volume bars carry flags)
m ingest csv --path "$FX/BTCUSDT_5m.csv" --source binance > /dev/null || true
m ingest csv --path "$FX/ETHUSDT_5m.csv" --source binance > /dev/null
m ingest csv --path "$FX/unnamed.csv" --source kraken --symbol BTC/USD > /dev/null
# other durations and flags, made by hand
m model record create bars source=manual symbol=SOL/USDT period=1h timestamp=2024-01-01T00:00:00Z open=100 high=110 low=95 close=105 volume=0 flags=1 > /dev/null
m model record create bars source=manual symbol=SOL/USDT period=1d timestamp=2024-01-02T00:00:00Z open=105 high=120 low=100 close=118 volume=5000 > /dev/null
# fields added to bars, a retyped built-in field, values on some bars
m model edit bars --add "note:text" --add "score:real default=0" --add "tag:text choices=a|b index" --apply > /dev/null
m model edit bars --change "flags:text optional" --apply > /dev/null
m model record update bars --where "symbol = ETH/USDT" --where "close > 300" note=eth-high score=2.5 tag=a > /dev/null
m model record update bars --where "source = manual" note=hand tag=b > /dev/null
# bars imported after the edits have no stored values yet (they read the defaults)
m ingest csv --path "$FX/unnamed.csv" --source kraken2 --symbol ETH/USD > /dev/null
# a model referring to bars, a project with a series, history
m model create picks --fields "bar:ref:bars; why:text" > /dev/null
ID=$(m model record list bars --where "source = manual" --limit 1 --json | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["id"])')
m model record create picks bar="$ID" why=breakout > /dev/null
m project create Alpha > /dev/null
m project add Alpha --result binance:BTC/USDT:5m > /dev/null
m project add Alpha --model bars > /dev/null
# what 2.14 says
m export --format csv --out "$OUT/bars.csv" > /dev/null
m model record list bars --limit 0 --json > "$OUT/bars.json"
m model show bars --json > "$OUT/model.json"
m model history bars --json > "$OUT/versions.json"
m model record list picks --json > "$OUT/picks.json"
m project show Alpha --json > "$OUT/project.json"
m history list --json > "$OUT/history.json"
rm -f "$OUT"/home/storage/*-wal "$OUT"/home/storage/*-shm
echo "wrote $OUT"
