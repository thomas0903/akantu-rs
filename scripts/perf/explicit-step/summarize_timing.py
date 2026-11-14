#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): summary of timing_rotated.sh
# output: per (mesh, mode, phase, program) the median [min, max] of ns per
# element-step over the rounds, ratios of the medians and the median
# [min, max] of the per-round paired ratios (same round, adjacent runs:
# drift-robust).
#   summarize_timing.py TIMING.jsonl [PHASE...]
import json
import statistics as st
import sys
from collections import defaultdict

rows = defaultdict(dict)
meta = {}
rounds = []
for line in open(sys.argv[1]):
    if not line.startswith("{"):
        continue
    o = json.loads(line)
    if o["kind"] == "round":
        rounds.append(o)
        continue
    label, mesh, mode, r = o["label"].split("|")
    if o["kind"] == "meta" and not label.startswith("cpp-"):
        meta[mesh] = (o["types"][0].strip("_"), o["nb_elements"])
    elif o["kind"] == "time":
        rows[(mesh, mode, o["phase"])][(label, r)] = o["ns_el_step"]
phases = sys.argv[2:] or ["step", "fint", "stress"]
labels = ["cpp-strict", "cpp-default", "main", "v98head", "head"]
pairs = [("head", "cpp-strict"), ("head", "cpp-default"), ("main", "cpp-strict"), ("head", "main"), ("head", "v98head")]


def med(v):
    return f"{st.median(v):.0f} [{min(v):.0f}, {max(v):.0f}]" if v else "—"


print(f"rounds: {len(rounds)}; orders: " + "; ".join(r["order"] for r in rounds))
print(f"cpu: {rounds[0]['cpu'] if rounds else '?'}; load at round start: " + ", ".join(r["load"].split()[0] for r in rounds))
print(f"first round {rounds[0]['utc'] if rounds else '?'}, last round {rounds[-1]['utc'] if rounds else '?'}")
print()
hdr = " | ".join(["C++ strict", "C++ default", "main", "V98 head", "head"])
phdr = " | ".join(f"{a} / {b} (paired)" for a, b in pairs)
print(f"| mesh | type | elements | mode | phase | {hdr} | {phdr} |")
print("|---|---|---|---|---|" + "---|" * (len(labels) + len(pairs)))
order = ["tet4-20k", "tet4-200k", "tet10-20k", "hex8-20k", "hex8-200k"]
summary = defaultdict(list)
for mesh in order:
    for mode in ["ss", "fd"]:
        for ph in phases:
            d = rows.get((mesh, mode, ph))
            if not d:
                continue
            vals = {l: [v for (ll, _), v in d.items() if ll == l] for l in labels}
            rs = sorted({r for (_, r) in d})
            cells = []
            for a, b in pairs:
                pr = [d[(a, r)] / d[(b, r)] for r in rs if (a, r) in d and (b, r) in d]
                if pr:
                    cells.append(f"**{st.median(pr):.2f}** [{min(pr):.2f}, {max(pr):.2f}]")
                    if ph == "step":
                        summary[(mode, a, b)].append(st.median(pr))
                else:
                    cells.append("—")
            ty, ne = meta.get(mesh, ("?", 0))
            print(f"| {mesh} | {ty} | {ne:,} | {mode} | {ph} | " + " | ".join(med(vals[l]) for l in labels) + " | " + " | ".join(cells) + " |")
print()
for (mode, a, b), v in sorted(summary.items()):
    print(f"step {mode} {a}/{b} (median of paired ratios per case): {min(v):.2f}–{max(v):.2f} (n={len(v)})")
