# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# cohsumm.py <series-dir> <base-variant>: cohesive driver series: physical fields equal, phase times (max over ranks)
import json, sys, os, glob, csv, statistics, collections
d, base = sys.argv[1], sys.argv[2]
SKIP = ('time', 'memory', 'mesh', 'stats')
phys = collections.defaultdict(set); times = collections.defaultdict(dict)
variants = sorted(x for x in os.listdir(d) if os.path.isdir(f'{d}/{x}'))
for v in variants:
    for rd in glob.glob(f'{d}/{v}/np*/rep*'):
        np_ = int(rd.split('/')[-2][2:]); rep = int(rd.split('rep')[-1])
        try: s = json.load(open(f'{rd}/summary.json'))
        except Exception as e: print('missing', rd, e); continue
        key = json.dumps({k: s[k] for k in sorted(s) if not any(t in k for t in SKIP)}, sort_keys=True)
        phys[np_].add(key)
        # per-rank chrono: max over ranks per phase
        per = collections.defaultdict(list)
        for f in glob.glob(f'{rd}/chrono_rank*.csv'):
            rows = list(csv.reader(open(f), skipinitialspace=True))
            for h, x in zip(rows[0], rows[1]):
                if h.endswith('nb_rep') or h == 'psize': continue
                per[h].append(float(x))
        for h, xs in per.items():
            times[(np_, h, v)][rep] = (max(xs), min(xs))
for np_ in sorted(phys):
    print(f'np{np_}: physical summary ' + ('EQUAL across variants and reps' if len(phys[np_]) == 1 else f'DIFFERS ({len(phys[np_])} versions)'))
print()
print('| np | phase | ' + ' | '.join(f'{v} s (max over ranks)' for v in variants) + f' | ratio vs {base} | min-over-ranks ' + ' / '.join(variants) + ' |')
print('|---|---|' + '---|' * (len(variants) + 2))
for (np_, h) in sorted({(k[0], k[1]) for k in times}):
    if h not in ('dist_mesh', 'init_full', 'static_solve', 'solve_step', 'before_step'): continue
    cells = []; mins = []
    for v in variants:
        xs = [a for a, b in times.get((np_, h, v), {}).values()]
        ms = [b for a, b in times.get((np_, h, v), {}).values()]
        cells.append(f'{statistics.median(xs):.4g} [{min(xs):.4g}, {max(xs):.4g}]' if xs else '-')
        mins.append(f'{statistics.median(ms):.4g}' if ms else '-')
    rat = []
    for v in variants:
        if v == base: continue
        a, b = times.get((np_, h, base), {}), times.get((np_, h, v), {})
        rs = [b[r][0] / a[r][0] for r in a if r in b and a[r][0] > 0]
        if rs: rat.append(f'{statistics.median(rs):.3f} [{min(rs):.3f}, {max(rs):.3f}]')
    print(f'| {np_} | {h} | ' + ' | '.join(cells) + ' | ' + '; '.join(rat) + ' | ' + ' / '.join(mins) + ' |')
