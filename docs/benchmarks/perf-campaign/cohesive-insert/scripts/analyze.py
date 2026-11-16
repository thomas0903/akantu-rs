#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion step); not ported from Akantu.
# analyze.py ser <root>       : serial before/after series (ab.sh layout <root>/<label>/rep<k>/)
# analyze.py par <root> <np>  : MPI before/after series (par-ab.sh layout <root>/np<np>/<label>/rep<k>/)
# Markdown tables: ms per step, median [min, max] over the repetitions, paired after/before ratios.
import sys, os, csv, re, statistics as st

def chrono(path):
    rows = list(csv.reader(open(path), skipinitialspace=True))
    h, v = rows[0], rows[1]
    return {h[i]: (float(v[i]), int(v[i + 1])) for i in range(1, len(h), 2)}

def stats(path):
    rows = list(csv.reader(open(path), skipinitialspace=True))
    return dict(zip(rows[0], [float(x) for x in rows[1]]))

def rss(path):
    m = re.search(r'Maximum resident set size \(kbytes\): (\d+)', open(path).read())
    return int(m.group(1)) / 1024 if m else float('nan')

def fmt(v):
    return f"{st.median(v):.3f} [{min(v):.3f}, {max(v):.3f}]"

def table(rows, labels=('before', 'after')):
    print(f"| quantity | {labels[0]} | {labels[1]} | {labels[1]} / {labels[0]} (paired) |")
    print("|---|---|---|---|")
    for name, a, b in rows:
        reps = sorted(set(a) & set(b))
        r = [b[k] / a[k] for k in reps if a[k] > 0]
        rs = fmt(r) if r else "–"
        print(f"| {name} | {fmt(list(a.values()))} | {fmt(list(b.values()))} | {rs} |")

def ser(root):
    q = {}
    for lab in ('before', 'after'):
        for rep in sorted(os.listdir(os.path.join(root, lab))):
            d = os.path.join(root, lab, rep)
            if open(os.path.join(d, 'exit')).read().strip() != '0':
                print('failed run', d); continue
            c = chrono(os.path.join(d, 'chrono.csv'))
            s = stats(os.path.join(d, 'insertion_stats_rank0.csv'))
            n = c['check_cohesive_stress'][1]
            vals = {
                'check_cohesive_stress [ms/step]': 1e3 * c['check_cohesive_stress'][0] / n,
                '  of which facet stresses and laws\' checks [ms/step]': 1e3 * (s['interpolation'] + s['checks']) / n,
                '  of which insertion [ms/step]': 1e3 * s['insertion'] / n,
                'solve_step [ms/step]': 1e3 * c['solve_step'][0] / c['solve_step'][1],
                'peak RSS [MiB]': rss(os.path.join(d, 'err.txt')),
            }
            for k, v in vals.items():
                q.setdefault(k, {}).setdefault(lab, {})[rep] = v
    ins = {open(os.path.join(root, lab, r, 'insertions.csv')).read().strip().splitlines()[-1]
           for lab in ('before', 'after') for r in os.listdir(os.path.join(root, lab))}
    print(f"last line of insertions.csv over every run: {sorted(ins)}")
    table([(k, v['before'], v['after']) for k, v in q.items()])

def par(root, np_):
    base = os.path.join(root, f'np{np_}')
    q = {}
    for lab in ('before', 'after'):
        for rep in sorted(os.listdir(os.path.join(base, lab))):
            d = os.path.join(base, lab, rep)
            if open(os.path.join(d, 'exit')).read().strip() != '0':
                print('failed run', d); continue
            ranks = []
            for r in range(np_):
                c = chrono(os.path.join(d, f'chrono_rank{r}.csv'))
                s = stats(os.path.join(d, f'insertion_stats_rank{r}.csv'))
                n = c['check_cohesive_stress'][1]
                ranks.append({
                    'check_cohesive_stress, slowest rank [ms/step]': 1e3 * c['check_cohesive_stress'][0] / n,
                    '  facet stresses, slowest rank [ms/step]': 1e3 * s['interpolation'] / n,
                    '  laws\' checks, slowest rank [ms/step]': 1e3 * s['checks'] / n,
                    '  insertion, slowest rank [ms/step]': 1e3 * s['insertion'] / n,
                    'solve_step, slowest rank [ms/step]': 1e3 * c['solve_step'][0] / c['solve_step'][1],
                })
            for k in ranks[0]:
                q.setdefault(k, {}).setdefault(lab, {})[rep] = max(x[k] for x in ranks)
    table([(k, v['before'], v['after']) for k, v in q.items()])

if sys.argv[1] == 'ser':
    ser(sys.argv[2])
else:
    par(sys.argv[2], int(sys.argv[3]))
