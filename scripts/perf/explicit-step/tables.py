#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): compact markdown tables
# of run_ab.sh series for docs/BENCHMARKS.md: per case and phase,
# median [min, max] ns per element-step of each label, and ratios to the
# first label.  tables.py OUT.jsonl LABEL... [--phases p1,p2]
import json
import statistics
import sys
from collections import defaultdict

args = [a for a in sys.argv[1:] if not a.startswith("--phases")]
phases = None
for a in sys.argv[1:]:
    if a.startswith("--phases="):
        phases = a.split("=", 1)[1].split(",")
path, labels = args[0], args[1:]
rows = defaultdict(list)
meta = {}
order = ["tet4-20k", "tet4-200k", "tet4-1m", "tet10-20k", "tet10-200k", "hex8-20k", "hex8-200k", "hex8-1m"]
for line in open(path):
    if not line.startswith("{"):
        continue
    o = json.loads(line)
    label, mesh, mode, _ = o["label"].split("|")
    if o["kind"] == "meta" and o["types"] != ["cpp"]:
        meta[mesh] = (o["types"][0].strip("_"), o["nb_elements"], o["nb_nodes"])
    elif o["kind"] == "time":
        rows[(mesh, mode, o["phase"], label)].append(o["ns_el_step"])
phases = phases or ["step", "check", "fint", "stress", "grad", "btd", "integ", "asm"]
cases = sorted({(k[0], k[1]) for k in rows}, key=lambda c: (order.index(c[0]) if c[0] in order else 99, c[1]))
def fmt(v):
    if not v:
        return "—"
    m = statistics.median(v)
    p = 0 if m >= 100 else 1
    return f"{m:.{p}f} [{min(v):.{p}f}, {max(v):.{p}f}]"
print("| mesh | type | elements | mode | phase | " + " | ".join(labels) + " | " +
      " | ".join(f"{l} / {labels[0]}" for l in labels[1:]) + (" | cur / cpp" if "cpp-strict" in labels and "cur" in labels else "") + " |")
print("|" + "---|" * (5 + len(labels) + len(labels) - 1 + (1 if "cpp-strict" in labels and "cur" in labels else 0)))
for mesh, mode in cases:
    ty, ne, _ = meta.get(mesh, ("?", 0, 0))
    for ph in phases:
        vals = [rows.get((mesh, mode, ph, l)) for l in labels]
        if not any(vals):
            continue
        meds = [statistics.median(v) if v else None for v in vals]
        ratios = [f"{m / meds[0]:.2f}" if m and meds[0] else "—" for m in meds[1:]]
        extra = ""
        if "cpp-strict" in labels and "cur" in labels:
            c, k = meds[labels.index("cur")], meds[labels.index("cpp-strict")]
            extra = f" | {c / k:.2f}" if c and k else " | —"
        print(f"| {mesh} | {ty} | {ne:,} | {mode} | {ph} | " + " | ".join(fmt(v) for v in vals) +
              " | " + " | ".join(ratios) + extra + " |")
