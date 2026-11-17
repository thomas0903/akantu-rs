#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# anat.py ser <root> [ref]: serial driver series (abt.sh layout <root>/<label>/rep<k>/), ms per step,
#   median [min, max], paired ratio to <ref> (default base); the check's split; peak RSS
# anat.py mpi <root>: ranks x threads runs (<root>/<label>/rep<k>/, chrono_rank<r>.csv), slowest rank
import sys, os, csv, re, statistics as st
def chrono(p):
    r = list(csv.reader(open(p), skipinitialspace=True)); h, v = r[0], r[1]
    return {h[i]: (float(v[i]), int(v[i + 1])) for i in range(1, len(h), 2)}
def stats(p):
    r = list(csv.reader(open(p), skipinitialspace=True)); return dict(zip(r[0], [float(x) for x in r[1]]))
def rss(p):
    m = re.search(r'Maximum resident set size \(kbytes\): (\d+)', open(p).read()); return int(m.group(1)) / 1024 if m else float('nan')
fmt = lambda v: f"{st.median(v):.3f} [{min(v):.3f}, {max(v):.3f}]"
def table(q, labels, ref):
    print("| quantity | " + " | ".join(labels) + " |"); print("|---|" + "---|" * len(labels))
    for k, byl in q.items():
        cells = []
        for l in labels:
            v = byl.get(l, {})
            if not v: cells.append("–"); continue
            c = fmt(list(v.values()))
            if l != ref and ref in byl:
                r = sorted(v[x] / byl[ref][x] for x in v if x in byl[ref] and byl[ref][x] > 0)
                if r: c += f" (×{st.median(r):.3f} [{r[0]:.3f}, {r[-1]:.3f}])"
            cells.append(c)
        print(f"| {k} | " + " | ".join(cells) + " |")
def ser(root, ref):
    q = {}; labels = sorted(os.listdir(root), key=lambda l: (l != ref, l))
    ins = set()
    for l in labels:
        for rep in sorted(os.listdir(os.path.join(root, l))):
            d = os.path.join(root, l, rep)
            if not os.path.exists(os.path.join(d, 'exit')) or open(os.path.join(d, 'exit')).read().strip() != '0': continue
            c = chrono(os.path.join(d, 'chrono.csv')); s = stats(os.path.join(d, 'insertion_stats_rank0.csv'))
            n = c['check_cohesive_stress'][1]
            vals = {'check_cohesive_stress [ms/step]': 1e3 * c['check_cohesive_stress'][0] / n,
                    '  facet stresses [ms/step]': 1e3 * s['interpolation'] / n,
                    "  laws' checks [ms/step]": 1e3 * s['checks'] / n,
                    '  insertion [ms/step]': 1e3 * s['insertion'] / n,
                    'solve_step [ms/step]': 1e3 * c['solve_step'][0] / c['solve_step'][1],
                    'peak RSS [MiB]': rss(os.path.join(d, 'err.txt'))}
            for k, v in vals.items(): q.setdefault(k, {}).setdefault(l, {})[rep] = v
            ins.add(open(os.path.join(d, 'insertions.csv')).read().strip().splitlines()[-1])
    print("last line of insertions.csv over every run:", sorted(ins)); table(q, labels, ref)
def mpi(root, ref):
    q = {}; labels = sorted(os.listdir(root), key=lambda l: (l != ref, l))
    for l in labels:
        for rep in sorted(os.listdir(os.path.join(root, l))):
            d = os.path.join(root, l, rep)
            if not os.path.exists(os.path.join(d, 'exit')) or open(os.path.join(d, 'exit')).read().strip() != '0': continue
            ranks = sorted(f for f in os.listdir(d) if f.startswith('chrono_rank'))
            vals = {}
            for f in ranks:
                c = chrono(os.path.join(d, f)); n = c['check_cohesive_stress'][1]
                s = stats(os.path.join(d, f.replace('chrono', 'insertion_stats')))
                for k, v in {'check_cohesive_stress [ms/step]': 1e3 * c['check_cohesive_stress'][0] / n,
                             '  insertion [ms/step]': 1e3 * s['insertion'] / n,
                             'solve_step [ms/step]': 1e3 * c['solve_step'][0] / c['solve_step'][1],
                             'step (check + solve) [ms/step]': 1e3 * (c['check_cohesive_stress'][0] + c['solve_step'][0]) / n}.items():
                    vals[k] = max(vals.get(k, 0.0), v)
            for k, v in vals.items(): q.setdefault(k, {}).setdefault(l, {})[rep] = v
    table(q, labels, ref)
(ser if sys.argv[1] == 'ser' else mpi)(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else ('base' if sys.argv[1] == 'ser' else 'base-np8-t1'))
