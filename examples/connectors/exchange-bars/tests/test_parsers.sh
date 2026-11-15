#!/usr/bin/env bash
# Offline parser test in a THROWAWAY home (tests/test-home/, see env.sh; never ~/.muninn):
#   MNNN=/path/to/mnnn ./test_parsers.sh
# The first run creates the home, installs the kit there (no self-check, no network) and stores the fixtures in
# sources fx-coinbase / fx-kraken under the item names the connector would give them
# ("<exchange> <BASE_QUOTE> <fetch epoch s>.json"); then both parsers run over them and every bar is checked.
set -euo pipefail
cd "$(dirname "$0")"; . ./env.sh
if ! m sources data fx-kraken >/dev/null 2>&1; then
  [ -f "$H/keys/envelope.toml" ] || ./setup_test_home.sh >/dev/null
  MNNN="$B" SKIP_SELFCHECK=1 ../install.sh >/dev/null
  F="$OUT/test-home/fixture-items"; mkdir -p "$F/cb" "$F/kr"
  cp ../fixtures/coinbase_BTC-USD.json "$F/cb/coinbase BTC_USD 1794770106.json"
  cp ../fixtures/coinbase_ETH-USD.json "$F/cb/coinbase ETH_USD 1794770106.json"
  cp ../fixtures/kraken_XBTUSD.json    "$F/kr/kraken BTC_USD 1794770106.json"
  cp ../fixtures/kraken_ETHUSD.json    "$F/kr/kraken ETH_USD 1794770106.json"
  m ingest json --path "$F/cb" --source fx-coinbase --no-progress >/dev/null
  m ingest json --path "$F/kr" --source fx-kraken --no-progress >/dev/null
fi
python3 - "$@" <<'PY'
import json, subprocess, os, sys
B=os.environ["B"]; H=os.environ["H"]
def run(space, src):
    p=subprocess.run([B,"--home",H,"--non-interactive","-y","spaces","run",space,"--input","source:"+src,"--json"],capture_output=True,text=True)
    out=p.stdout; 
    d=json.JSONDecoder(); obj,_=d.raw_decode(out[out.index("{"):]); return obj, p.stderr
def frame(o):
    # find the frame with a "columns" key anywhere
    def find(x):
        if isinstance(x,dict):
            if "columns" in x and "timestamp" in x["columns"]: return x
            for v in x.values():
                r=find(v)
                if r: return r
        if isinstance(x,list):
            for v in x:
                r=find(v)
                if r: return r
    return find(o)
for ex,space,src,files,t in [("coinbase","parse-coinbase-candles","fx-coinbase",["coinbase_BTC-USD","coinbase_ETH-USD"],"cb"),
                          ("kraken","parse-kraken-ohlc","fx-kraken",["kraken_XBTUSD","kraken_ETHUSD"],"kr")]:
    o,err=run(space,src); f=frame(o)["columns"]; n=len(f["timestamp"])
    fetch=1794770106*1000
    ok=[]
    ok.append(("columns", sorted(f.keys())==sorted(["timestamp","open","high","low","close","volume","symbol"])))
    ok.append(("ms timestamps aligned to 300000", all(int(t)%300000==0 for t in f["timestamp"])))
    ok.append(("no forming bar (open+300s <= fetch time)", all(t+300000<=fetch for t in f["timestamp"])))
    ok.append(("high>=max(open,close), low<=min(open,close)", all(h>=max(a,c) and l<=min(a,c) for a,h,l,c in zip(f["open"],f["high"],f["low"],f["close"]))))
    ok.append(("two symbols", sorted(set(f["symbol"]))==["BTC/USD","ETH/USD"]))
    # raw cross-check: every kept bar equals the raw row (independent re-read of the fixtures)
    raw={}
    for fn in files:
        d=json.load(open("../fixtures/%s.json"%fn)); sym="BTC/USD" if ("BTC" in fn or "XBT" in fn) else "ETH/USD"
        if isinstance(d,dict): k=[x for x in d["result"] if x!="last"][0]; rows=d["result"][k]; 
        else: rows=d
        for r in rows:
            if ex=="coinbase": t_,lo,hi,op,cl,vo=map(float,r[:6])
            else: t_,op,hi,lo,cl,vo=float(r[0]),float(r[1]),float(r[2]),float(r[3]),float(r[4]),float(r[6])
            raw[(sym,int(t_*1000))]=(op,hi,lo,cl,vo)
    mism=[i for i in range(n) if raw.get((f["symbol"][i],int(f["timestamp"][i])))!=(f["open"][i],f["high"][i],f["low"][i],f["close"][i],f["volume"][i])]
    ok.append(("every row equals the raw row (column mapping)", not mism))
    expected=sum(1 for k in raw if k[1]+300000<=fetch)
    ok.append(("count = raw bars closed at fetch time (%d)"%expected, n==expected))
    print("%s: %d bars; log: %s"%(ex,n,[l for l in err.splitlines() if "closed bars" in l]))
    for name,v in ok: print("   %-55s %s"%(name,"PASS" if v else "FAIL"))
    if not all(v for _,v in ok): sys.exit(1)
    print("   newest per symbol:",{s:max(t for t,sy in zip(f["timestamp"],f["symbol"]) if sy==s) for s in set(f["symbol"])})
PY
