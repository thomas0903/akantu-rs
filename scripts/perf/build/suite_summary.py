#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): tables of a
# suite_series.sh directory: per case the total of its timed phases
# (seconds), median [min, max] per variant over the repetitions and the
# median [min, max] of the per-repetition paired ratios against BASE; the
# digests of every variant and repetition must agree.
#   suite_summary.py OUT_DIR BASE
import glob
import json
import os
import statistics as st
import sys
from collections import defaultdict

root, base = sys.argv[1], sys.argv[2]
tot = defaultdict(dict)  # case -> {(variant, rep): seconds}
dig = defaultdict(set)
variants = sorted(d for d in os.listdir(root) if os.path.isdir(os.path.join(root, d)))
for v in variants:
    for f in glob.glob(os.path.join(root, v, "*.jsonl")):
        case, rep = os.path.basename(f)[: -len(".jsonl")].rsplit(".r", 1)
        s = 0.0
        for line in open(f):
            o = json.loads(line)
            s += o["seconds"]
            dig[case].add(o.get("digest"))
        tot[case][(v, rep)] = s
others = [v for v in variants if v != base]
print(f"| case | {base} [s] | " + " | ".join(f"{v} [s]" for v in others) + " | " + " | ".join(f"{v} / {base}" for v in others) + " | digests |")
print("|---|---|" + "---|" * (2 * len(others)) + "---|")
for case in sorted(tot):
    d = tot[case]
    reps = sorted({r for (_, r) in d})
    def cell(v):
        x = [d[(v, r)] for r in reps if (v, r) in d]
        return f"{st.median(x):.3f} [{min(x):.3f}, {max(x):.3f}]" if x else "—"
    rat = []
    for v in others:
        p = [d[(v, r)] / d[(base, r)] for r in reps if (v, r) in d and (base, r) in d]
        rat.append(f"**{st.median(p):.3f}** [{min(p):.3f}, {max(p):.3f}]" if p else "—")
    print(f"| {case} | {cell(base)} | " + " | ".join(cell(v) for v in others) + " | " + " | ".join(rat)
          + f" | {'identical' if len(dig[case]) == 1 else 'DIFFER'} ({len(reps)} reps) |")
