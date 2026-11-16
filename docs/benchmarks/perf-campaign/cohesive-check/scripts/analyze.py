#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (WP-COHCHECK); not ported from Akantu.
# analyze.py <outroot> [elements]: per phase ms/step of an interleaved before/after series
# (ab.sh layout <outroot>/<label>/rep<k>/chrono.csv), medians [min, max] and paired ratios.
import sys, os, csv, json, statistics as st
root = sys.argv[1]
labels = sorted(x for x in os.listdir(root) if os.path.isdir(os.path.join(root, x)))
data = {}
for lab in labels:
    for rep in sorted(os.listdir(os.path.join(root, lab))):
        d = os.path.join(root, lab, rep)
        try:
            rows = list(csv.reader(open(os.path.join(d, 'chrono.csv')), skipinitialspace=True))
        except FileNotFoundError:
            continue
        h, v = rows[0], rows[1]
        ph = {h[i]: (float(v[i]), int(v[i + 1])) for i in range(1, len(h), 2)}
        s = json.load(open(os.path.join(d, 'summary.json')))
        data.setdefault(lab, {})[rep] = (ph, s)
phases = ['check_cohesive_stress', 'solve_step', 'energies', 'boundary_conditions']
first = next(iter(next(iter(data.values())).values()))[1]
ne = first.get('nb_regular_elements_dim3', 0)
print(f"elements {ne}, steps {first['max_steps']}, cohesive at end {first.get('nb_cohesive_elements')}, first insertion {first.get('first_insertion_step')}")
def ms(ph, p):
    t, n = ph.get(p, (0.0, 1))
    return 1e3 * t / max(n, 1)
for p in phases + ['loop']:
    line = f"{p:24s}"
    vals = {}
    for lab in labels:
        xs = {r: (1e3 * s['loop_time_s'] / s['max_steps'] if p == 'loop' else ms(ph, p)) for r, (ph, s) in data[lab].items()}
        vals[lab] = xs
        v = list(xs.values())
        line += f" {lab}: {st.median(v):8.3f} [{min(v):.3f}, {max(v):.3f}] ms/step"
        if p in ('check_cohesive_stress', 'loop') and ne:
            line += f" ({st.median(v) * 1e3 / ne * 1e3:.0f} ns/el)"
    if 'before' in vals and 'after' in vals:
        rs = [vals['after'][r] / vals['before'][r] for r in vals['after'] if r in vals['before'] and vals['before'][r] > 0]
        if rs:
            line += f"  after/before {st.median(rs):.3f} [{min(rs):.3f}, {max(rs):.3f}] (n={len(rs)})"
    print(line)
