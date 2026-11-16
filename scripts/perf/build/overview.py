#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): one row per build
# configuration over several series.sh files: the median [min, max] of the
# per-repetition paired ratios variant / base (base of the same series) for
# the main metric of each case.
#   overview.py SERIES.jsonl[:VARIANT,VARIANT...] ...
import json
import statistics as st
import sys
from collections import defaultdict

cols = [("es-tet4-ss", "time", "step", "tet4 ss step"), ("es-tet4-fd", "time", "step", "tet4 fd step"),
        ("es-hex8-ss", "time", "step", "hex8 ss step"), ("es-hex8-fd", "time", "step", "hex8 fd step"),
        ("coh", "coh", "t_solve_step", "driver explicit step"),
        ("coh", "coh", "t_check_cohesive_stress", "driver insertion check"),
        ("coh", "coh", "t_static_solve", "driver static solve"), ("coh", "coh", "wall_s", "driver wall"),
        ("facet", "facet", "sum", "facet set-up")]
print("| configuration | " + " | ".join(c[3] for c in cols) + " |")
print("|---|" + "---|" * len(cols))
for arg in sys.argv[1:]:
    path, _, want = arg.partition(":")
    vals = defaultdict(dict)
    order = []
    for line in open(path):
        if not line.startswith("{"):
            continue
        o = json.loads(line)
        if o["kind"] in ("round", "end"):
            continue
        v, case, rep = o["label"].split("|")
        if v not in order:
            order.append(v)
        if o["kind"] == "time":
            vals[(case, o["phase"])][(v, rep)] = o["ns_el_step"]
        elif o["kind"] == "coh" and o.get("exit") == 0:
            for k in ("t_solve_step", "t_check_cohesive_stress", "t_static_solve", "wall_s"):
                if k in o:
                    vals[(case, k)][(v, rep)] = o[k]
        elif o["kind"] == "facet":
            vals[(case, "sum")][(v, rep)] = sum(o[k] for k in ("csr", "fill_e2s", "init_mesh_facets", "erase", "purify"))
    for v in (want.split(",") if want else [x for x in order if x != "base"]):
        cells = []
        for case, _, metric, _ in cols:
            d = vals.get((case, metric), {})
            reps = sorted({r for (_, r) in d})
            p = [d[(v, r)] / d[("base", r)] for r in reps if (v, r) in d and ("base", r) in d]
            cells.append(f"{st.median(p):.3f} [{min(p):.3f}, {max(p):.3f}]" if p else "—")
        print(f"| `{v}` | " + " | ".join(cells) + " |")
