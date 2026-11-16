#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): table of the
# pool-overhead series (D-123). Per thread count, amount of work and variant:
# the median over the rounds of the per-round median region time (µs), and the
# overhead = region time − serial time / threads.
#   summarize_pool.py POOL.jsonl
import json
import statistics as st
import sys
from collections import defaultdict

t = defaultdict(list)
for line in open(sys.argv[1]):
    if line.startswith("{"):
        o = json.loads(line)
        t[(o["threads"], o["work_us"], o["variant"])].append(o["median_us"])
variants = ["exec", "rayon-b", "spin"]
print("| threads | work (µs, nominal) | serial (µs) | " + " | ".join(f"{v} (µs) | {v} overhead" for v in variants) + " |")
print("|---|---|---|" + "---|---|" * len(variants))
for n, w in sorted({(n, w) for (n, w, _) in t}):
    s = st.median(t[(n, w, "serial")])
    cells = []
    for v in variants:
        x = t.get((n, w, v))
        if not x:
            cells.append("— | —")
            continue
        m = st.median(x)
        cells.append(f"{m:.2f} [{min(x):.2f}, {max(x):.2f}] | {m - s / n:.2f}")
    print(f"| {n} | {w} | {s:.2f} | " + " | ".join(cells) + " |")
