#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): tables of a series.sh
# run. Per case and metric: median [min, max] per variant over the
# repetitions, and against the base variant the median [min, max] of the
# per-repetition paired ratios (variant / base, same repetition). Also checks
# that the cohesive driver's outputs (every .csv except chrono.csv) have the
# same SHA-256 for every variant.
#   summarize.py SERIES.jsonl BASE [VARIANT...]
import json
import statistics as st
import sys
from collections import defaultdict

path, base = sys.argv[1], sys.argv[2]
want = sys.argv[3:]
vals = defaultdict(dict)  # (case, metric) -> {(variant, rep): value}
digests = defaultdict(set)
rounds = []
variants = []
for line in open(path):
    if not line.startswith("{"):
        continue
    o = json.loads(line)
    k = o["kind"]
    if k == "round":
        rounds.append(o)
        continue
    if k == "end":
        continue
    v, case, rep = o["label"].split("|")
    if v not in variants:
        variants.append(v)
    if k == "time":
        vals[(case, o["phase"] + " [ns/el/step]")][(v, rep)] = o["ns_el_step"]
    elif k == "coh":
        if o.get("exit", 1) != 0:
            print(f"WARNING: cohesive run {o['label']} exit {o.get('exit')}")
            continue
        for m in ("solve_step", "check_cohesive_stress", "static_solve", "init_full"):
            if "t_" + m in o:
                vals[(case, m + " [s]")][(v, rep)] = o["t_" + m]
        vals[(case, "wall [s]")][(v, rep)] = o["wall_s"]
        digests[case].add((v, o["outputs_sha256"]))
    elif k == "facet":
        tot = 0.0
        for m in ("csr", "fill_e2s", "init_mesh_facets", "erase", "purify"):
            vals[(case, m + " [s]")][(v, rep)] = o[m]
            tot += o[m]
        vals[(case, "sum [s]")][(v, rep)] = tot
show = want or [v for v in variants if v != base]
print(f"repetitions: {len(rounds)}; cpu: {rounds[0]['cpu'] if rounds else '?'}")
print(f"first {rounds[0]['utc'] if rounds else '?'}; loads at round start: " + ", ".join(r["load"].split()[0] for r in rounds))
print()


def fmt(x):
    return f"{x:.4g}"


hdr = " | ".join([base] + show)
rhdr = " | ".join(f"{v} / {base}" for v in show)
print(f"| case | metric | {hdr} | {rhdr} |")
print("|---|---|" + "---|" * (1 + 2 * len(show)))
for (case, metric), d in vals.items():
    reps = sorted({r for (_, r) in d})
    cells = []
    for v in [base] + show:
        x = [d[(v, r)] for r in reps if (v, r) in d]
        cells.append(f"{fmt(st.median(x))} [{fmt(min(x))}, {fmt(max(x))}]" if x else "—")
    for v in show:
        pr = [d[(v, r)] / d[(base, r)] for r in reps if (v, r) in d and (base, r) in d and d[(base, r)] > 0]
        cells.append(f"**{st.median(pr):.3f}** [{min(pr):.3f}, {max(pr):.3f}]" if pr else "—")
    print(f"| {case} | {metric} | " + " | ".join(cells) + " |")
print()
for case, s in digests.items():
    shas = {sha for (_, sha) in s}
    print(f"{case} outputs: {'identical across variants' if len(shas) == 1 else 'DIFFER: ' + str(sorted(s))} ({len(s)} variant(s))")
