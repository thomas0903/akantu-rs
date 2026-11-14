#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): per-repetition paired ratios head/main
# of the Chrono rows (solve_step, check_cohesive_stress) of a timing_ab.sh
# campaign, and the peak RSS of every run.
import csv, statistics as st, sys, os, re
root = sys.argv[1]
def chrono(path):
    with open(path) as f:
        r = list(csv.reader(f, skipinitialspace=True))
    d = dict(zip(r[0], r[1]))
    out = {}
    for k in ("solve_step", "check_cohesive_stress", "static_solve", "init_full"):
        if k in d and float(d.get(k + " nb_rep", 0) or 0) > 0:
            out[k] = float(d[k]) / float(d[k + " nb_rep"]) * 1e3
    return out
def rss(path):
    m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", open(path).read())
    return int(m.group(1)) / 1024 if m else float("nan")
for mat in ("cohesive", "elastic"):
    reps = sorted(os.listdir(f"{root}/head/{mat}/h0.12"))
    for k in ("solve_step", "check_cohesive_stress"):
        pr, a, b = [], [], []
        for rep in reps:
            h = chrono(f"{root}/head/{mat}/h0.12/{rep}/chrono.csv"); m = chrono(f"{root}/main/{mat}/h0.12/{rep}/chrono.csv")
            if k in h and k in m:
                pr.append(h[k] / m[k]); a.append(m[k]); b.append(h[k])
        if pr:
            print(f"{mat} {k}: main {st.median(a):.3f} [{min(a):.3f}, {max(a):.3f}] ms/step, head {st.median(b):.3f} [{min(b):.3f}, {max(b):.3f}] ms/step, "
                  f"paired head/main {st.median(pr):.3f} [{min(pr):.3f}, {max(pr):.3f}] (n={len(pr)})")
    rm = [rss(f"{root}/main/{mat}/h0.12/{r}/time.txt") for r in reps]; rh = [rss(f"{root}/head/{mat}/h0.12/{r}/time.txt") for r in reps]
    print(f"{mat} peak RSS: main {st.median(rm):.0f} [{min(rm):.0f}, {max(rm):.0f}] MiB, head {st.median(rh):.0f} [{min(rh):.0f}, {max(rh):.0f}] MiB")
