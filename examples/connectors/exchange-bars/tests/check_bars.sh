#!/usr/bin/env bash
# Report on model:bars in the throwaway home: counts per source/symbol, period, newest bar vs now, duplicates.
cd "$(dirname "$0")"; . ./env.sh
m model record list bars --limit 0 --json > /tmp/_bars.$$.json
python3 - /tmp/_bars.$$.json <<'PY'
import json,sys,time,collections
bars=json.load(open(sys.argv[1])); now=time.time()
c=collections.defaultdict(list)
for b in bars: c[(b["source"],b["symbol"])].append(b)
print("%-9s %-8s %5s  %-6s %-20s %-20s %s"%("source","symbol","bars","period","oldest open (UTC)","newest open (UTC)","newest closed?"))
fmt=lambda ms: time.strftime("%Y-%m-%d %H:%M",time.gmtime(ms/1000))
bad=False
for (s,y),v in sorted(c.items()):
    ts=sorted(b["timestamp"] for b in v); per={b["period"] for b in v}
    closed=ts[-1]/1000+300<=now
    print("%-9s %-8s %5d  %-6s %-20s %-20s %s"%(s,y,len(v),",".join(sorted(per)),fmt(ts[0]),fmt(ts[-1]),closed))
    if per!={"5m"} or not closed or len(set(ts))!=len(ts) or any(t%300000 for t in ts): bad=True
print("total bars:",len(bars),"| ids unique:",len({b['id'] for b in bars})==len(bars),"| all periods 5m:",all(b['period']=='5m' for b in bars),"| all aligned to 300000:",all(b['timestamp']%300000==0 for b in bars),"| problems:",bad)
PY
rm -f /tmp/_bars.$$.json
