# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# summ.py <series-dir> [baseline-variant]: per (case, np, phase) median [min,max] per call (ms) per variant,
# paired ratio vs the baseline, digest check.
import json, sys, os, glob, statistics, collections
d = sys.argv[1]; base = sys.argv[2] if len(sys.argv) > 2 else None
data = collections.defaultdict(dict)   # (case, np, phase, variant) -> {rep: s/call}
dig = collections.defaultdict(set); cnt = {}
variants = sorted(x for x in os.listdir(d) if os.path.isdir(os.path.join(d, x)))
for v in variants:
    for f in glob.glob(f'{d}/{v}/np*/*.r*.jsonl'):
        np_ = int(f.split('/')[-2][2:]); case, rep = os.path.basename(f)[:-6].rsplit('.r', 1)
        for l in open(f):
            try: j = json.loads(l)
            except Exception: continue
            if 'phase' not in j: continue
            if j.get('rank', 0) not in (0, None): continue
            key = (case, np_, j['phase'])
            data[key + (v,)][int(rep)] = j['seconds'] / max(j['calls'], 1)
            cnt[key] = j['calls']
            dig[(case, np_, v)].add(j.get('digest'))
keys = sorted({k[:3] for k in data})
fmt = lambda x: f'{x*1e3:.4g}'
print(f'| case | np | phase | calls | ' + ' | '.join(f'{v} ms' for v in variants) + (' | ratio vs ' + base if base else '') + ' |')
print('|---|---|---|---|' + '---|' * len(variants) + ('---|' if base else ''))
for k in keys:
    cells = []
    for v in variants:
        xs = list(data.get(k + (v,), {}).values())
        cells.append(f'{fmt(statistics.median(xs))} [{fmt(min(xs))}, {fmt(max(xs))}]' if xs else '-')
    rat = ''
    if base:
        others = [v for v in variants if v != base]
        parts = []
        for v in others:
            a, b = data.get(k + (base,), {}), data.get(k + (v,), {})
            rs = [b[r] / a[r] for r in a if r in b and a[r] > 0]
            if rs: parts.append(f'{v}: {statistics.median(rs):.3f} [{min(rs):.3f}, {max(rs):.3f}]')
        rat = ' | ' + '; '.join(parts)
    print(f'| {k[0]} | {k[1]} | {k[2]} | {cnt[k]} | ' + ' | '.join(cells) + rat + ' |')
print()
bad = 0
for (case, np_) in sorted({(c, n) for (c, n, v) in dig}):
    ds = {v: dig[(case, np_, v)] for v in variants if (case, np_, v) in dig}
    alls = set().union(*ds.values())
    ok = len(alls) == 1 and all(len(s) == 1 for s in ds.values())
    if not ok: bad += 1
    print(f'digest {case} np{np_}: ' + ('EQUAL ' + next(iter(alls)) if ok else 'MISMATCH ' + str(ds)))
print('DIGEST MISMATCHES:', bad)
