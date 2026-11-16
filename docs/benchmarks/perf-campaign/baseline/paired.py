#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): per-phase, per-repetition
# paired ratios of two variants of a timing_ab.sh campaign
# (crates/akantu-bench/scripts/timing_ab.sh) or of c1prep_campaign.sh runs.
#
#   paired.py ROOT A B            timing_ab.sh layout ROOT/<label>/<material>/h<h>/rep<k>/
#   paired.py --c1prep ROOT A B   c1prep layout ROOT/<case>/np<N>/rep<k>/ (one material)
#
# Prints Markdown: for every (material, h) or rank count and every Chrono
# phase present in both variants, A and B as median [min, max] (seconds for
# one-off phases, ms per call for the per-step phases), the paired ratio B / A
# per repetition as median [min, max], the core-us per element and step
# ((solve_step + check_cohesive_stress) * ranks / tetrahedra / steps) and the
# peak RSS (GNU time or the container's memory peak).
import csv
import json
import os
import re
import statistics as st
import sys

ONE_OFF = ("read_mesh", "dist_mesh", "init_full", "static_solve")
PER_STEP = ("solve_step", "check_cohesive_stress", "energies")


def chrono(d):
    with open(os.path.join(d, "chrono.csv")) as f:
        r = list(csv.reader(f, skipinitialspace=True))
    row = dict(zip(r[0], r[1]))
    out = {"psize": int(row["psize"])}
    for k, v in row.items():
        if k.endswith(" nb_rep") or k == "psize":
            continue
        out[k] = (float(v), int(float(row.get(k + " nb_rep", "1") or 1)))
    return out


def rss_mib(d):
    p = os.path.join(d, "time.txt")
    if os.path.exists(p):
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", open(p).read())
        if m:
            return int(m.group(1)) / 1024
    p = os.path.join(d, "status.txt")
    if os.path.exists(p):
        for line in open(p):
            if line.startswith("memory_peak_bytes="):
                v = json.loads(line.split("=", 1)[1])
                if v:
                    return v / 2**20
    return None


def tets(d):
    s = json.load(open(os.path.join(d, "summary.json")))
    return s.get("nb_regular_elements_dim3")


def fmt(xs, nd=3):
    if not xs:
        return "—"
    return f"{st.median(xs):.{nd}g} [{min(xs):.{nd}g}, {max(xs):.{nd}g}]"


def table(groups, a, b):
    print(f"| group | phase | unit | {a} | {b} | {b} / {a} (paired) | n |")
    print("|---|---|---|---|---|---|---|")
    for g, reps in groups.items():
        ca = {k: chrono(p) for k, p in reps.get(a, {}).items()}
        cb = {k: chrono(p) for k, p in reps.get(b, {}).items()}
        common = sorted(set(ca) & set(cb))
        if not common:
            continue
        phases = [p for p in ONE_OFF + PER_STEP if all(p in ca[k] and p in cb[k] for k in common)]
        for ph in phases:
            per = ph in PER_STEP
            def val(c):
                t, n = c[ph]
                return t / n * 1e3 if per else t
            va = [val(ca[k]) for k in common]
            vb = [val(cb[k]) for k in common]
            pr = [y / x for x, y in zip(va, vb) if x > 0]
            print(f"| {g} | {ph} | {'ms/call' if per else 's'} | {fmt(va)} | {fmt(vb)} | {fmt(pr)} | {len(common)} |")
        def core(c, d):
            t = sum(c[p][0] for p in ("solve_step", "check_cohesive_stress") if p in c)
            n = c["solve_step"][1] if "solve_step" in c else 0
            ne = tets(d)
            return t * c["psize"] / ne / n * 1e6 if n and ne else None
        cora = [core(ca[k], reps[a][k]) for k in common]
        corb = [core(cb[k], reps[b][k]) for k in common]
        if all(x is not None for x in cora + corb):
            pr = [y / x for x, y in zip(cora, corb)]
            print(f"| {g} | core-us/el/step | us | {fmt(cora)} | {fmt(corb)} | {fmt(pr)} | {len(common)} |")
        ra = [rss_mib(reps[a][k]) for k in common]
        rb = [rss_mib(reps[b][k]) for k in common]
        if all(x is not None for x in ra + rb):
            pr = [y / x for x, y in zip(ra, rb)]
            print(f"| {g} | peak RSS | MiB | {fmt(ra, 4)} | {fmt(rb, 4)} | {fmt(pr)} | {len(common)} |")


def main():
    args = sys.argv[1:]
    c1 = args and args[0] == "--c1prep"
    if c1:
        args = args[1:]
    if len(args) != 3:
        sys.exit(__doc__ or "usage: paired.py [--c1prep] ROOT A B")
    root, a, b = args
    groups = {}
    for dp, dirs, files in os.walk(root):
        if "chrono.csv" not in files:
            continue
        rel = os.path.relpath(dp, root).split(os.sep)
        if c1:
            # [<campaign>/]<case>/np<N>/rep<k>[...]
            i = next(i for i, x in enumerate(rel) if re.fullmatch(r"np\d+", x))
            label, g, rep = rel[i - 1], rel[i], os.sep.join(rel[:i - 1] + rel[i + 1:])
        else:
            label, mat, h, rep = rel[0], rel[1], rel[2], rel[3]
            g = f"{mat} {h}"
        if os.path.exists(os.path.join(dp, "exit")) and open(os.path.join(dp, "exit")).read().strip() != "0":
            continue
        groups.setdefault(g, {}).setdefault(label, {})[rep] = dp
    table(dict(sorted(groups.items())), a, b)


if __name__ == "__main__":
    main()
