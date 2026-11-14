#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): summary of run_ab.sh
# output: median [min, max] ns per element-step per (mesh, mode,
# phase, label), and the ratio of each label's median to the first label's.
#   summarize.py OUT.jsonl [LABEL...]   (label order = ratio base first)
import json
import statistics
import sys
from collections import defaultdict

rows = defaultdict(list)
meta = {}
for line in open(sys.argv[1]):
    line = line.strip()
    if not line.startswith("{"):
        continue
    o = json.loads(line)
    label, mesh, mode, _ = o["label"].split("|")
    if o["kind"] == "meta":
        meta[mesh] = (o["types"], o["nb_elements"], o["nb_nodes"])
    elif o["kind"] == "time":
        rows[(mesh, mode, o["phase"], label)].append(o["ns_el_step"])
labels = sys.argv[2:] or sorted({k[3] for k in rows})
phases = ["step", "check", "fint", "stress", "grad", "btd", "integ", "asm"]
cases = sorted({(k[0], k[1]) for k in rows})
print("| mesh | type | elements | mode | phase | " + " | ".join(labels) + " | " +
      " | ".join(f"{l}/{labels[0]}" for l in labels[1:]) + " |")
print("|" + "---|" * (5 + 2 * len(labels) - 1))
for mesh, mode in cases:
    ty, ne, _ = meta.get(mesh, (["?"], 0, 0))
    for ph in phases:
        vals = [rows.get((mesh, mode, ph, l)) for l in labels]
        if not any(vals):
            continue
        cells, meds = [], []
        for v in vals:
            if v:
                m = statistics.median(v)
                meds.append(m)
                cells.append(f"{m:.1f} [{min(v):.1f}, {max(v):.1f}] (n={len(v)})")
            else:
                meds.append(None)
                cells.append("—")
        ratios = [f"{m / meds[0]:.3f}" if m and meds[0] else "—" for m in meds[1:]]
        print(f"| {mesh} | {','.join(t.strip('_') for t in ty)} | {ne} | {mode} | {ph} | " +
              " | ".join(cells) + " | " + " | ".join(ratios) + " |")
