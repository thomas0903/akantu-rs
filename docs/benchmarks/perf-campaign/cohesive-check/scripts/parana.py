#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion check); not ported from Akantu.
# parana.py <root> <np>: per-rank phase times of interleaved MPI runs (<root>/np<np>/<tree>/rep<k>/)
import sys, os, csv, statistics as st
root, np_ = sys.argv[1], int(sys.argv[2])
base = os.path.join(root, f'np{np_}')
def chrono(path):
    rows = list(csv.reader(open(path), skipinitialspace=True))
    h, v = rows[0], rows[1]
    return {h[i]: (float(v[i]), int(v[i + 1])) for i in range(1, len(h), 2)}
def stats(path):
    rows = list(csv.reader(open(path), skipinitialspace=True))
    return dict(zip(rows[0], [float(x) for x in rows[1]]))
res = {}
for tree in sorted(os.listdir(base)):
    for rep in sorted(os.listdir(os.path.join(base, tree))):
        d = os.path.join(base, tree, rep)
        if open(os.path.join(d, 'exit')).read().strip() != '0':
            print('failed run', d); continue
        ranks = []
        for r in range(np_):
            c = chrono(os.path.join(d, f'chrono_rank{r}.csv'))
            s = stats(os.path.join(d, f'insertion_stats_rank{r}.csv'))
            steps = c['check_cohesive_stress'][1]
            ranks.append({'check': 1e3 * c['check_cohesive_stress'][0] / steps,
                          'solve': 1e3 * c['solve_step'][0] / steps,
                          'interp': 1e3 * s['interpolation'] / steps,
                          'checks': 1e3 * s['checks'] / steps,
                          'insertion': 1e3 * s['insertion'] / steps})
        res.setdefault(tree, []).append(ranks)
for tree, reps in res.items():
    print(f'== {tree} (np {np_}, {len(reps)} reps), ms per step, median over repetitions of [min rank, max rank]')
    for k in ['check', 'interp', 'checks', 'insertion', 'solve']:
        lo = st.median(min(r[k] for r in ranks) for ranks in reps)
        hi = st.median(max(r[k] for r in ranks) for ranks in reps)
        r0 = st.median(ranks[0][k] for ranks in reps)
        print(f'  {k:10s} rank0 {r0:8.3f}  ranks [{lo:.3f}, {hi:.3f}]')
if 'parbase' in res and 'src' in res:
    for k in ['check', 'solve']:
        rs = [max(r[k] for r in a) / max(r[k] for r in b) for a, b in zip(res['src'], res['parbase'])]
        print(f'after/before (max over ranks) {k}: {st.median(rs):.3f} [{min(rs):.3f}, {max(rs):.3f}] (n={len(rs)})')
