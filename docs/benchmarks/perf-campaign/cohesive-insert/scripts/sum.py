# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# sum.py <dir> <labelA> <labelB>: medians [min,max] per (case, phase), paired ratio, digest check
import json,glob,collections,statistics,sys
d,A,B=sys.argv[1:4]
t=collections.defaultdict(dict); dg=collections.defaultdict(set)
for f in sorted(glob.glob(d+"/*.json")):
    lab,rep=f.split("/")[-1][:-5].rsplit("-rep",1)
    for l in open(f):
        x=json.loads(l); t[(x["case"],x["phase"])].setdefault(lab,{})[int(rep)]=x["seconds"]; dg[x["case"]].add(x["digest"])
print(f"| case | phase | {A} [s] | {B} [s] | {B}/{A} (paired) |\n|---|---|---|---|---|")
for (c,p),v in t.items():
    if A not in v or B not in v: continue
    a=v[A]; b=v[B]
    if max(a.values())<0.005: continue
    r=sorted(b[k]/a[k] for k in a if k in b)
    fm=lambda s: f"{statistics.median(s.values()):.3f} [{min(s.values()):.3f}, {max(s.values()):.3f}]"
    print(f"| {c} | {p} | {fm(a)} | {fm(b)} | {statistics.median(r):.3f} [{r[0]:.3f}, {r[-1]:.3f}] |")
print("digests per case:", {c:len(s) for c,s in dg.items()})
