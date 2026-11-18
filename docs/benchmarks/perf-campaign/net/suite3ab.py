#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): paired comparison of shared-memory
# suite runs DIR/{BUILD...}: per case, ranks and phase, the median seconds of each
# build, the paired ratios of the last build to each other one (median, min, max) and
# the number of distinct result digests over the builds.
#   python3 -I suite3ab.py > suite3-ab.txt                      (in this directory)
#   python3 -I suite3ab.py suite4 n2base n2head > suite4-ab.txt
import collections
import glob
import json
import statistics as st
import sys

d = collections.defaultdict(lambda: collections.defaultdict(dict))
DIR = sys.argv[1] if len(sys.argv) > 1 else "suite3"
V = tuple(sys.argv[2:]) if len(sys.argv) > 2 else ("before", "after", "r3")
for v in V:
    for f in glob.glob(f"{DIR}/{v}/np*/*.jsonl"):
        np_ = f.split("/")[2]
        rep = int(f.split(".r")[-1].split(".")[0])
        with open(f) as fh:
            for line in fh:
                if not line.startswith("{"):
                    continue
                j = json.loads(line)
                if "phase" not in j:
                    continue
                d[(j["case"], np_, j["phase"])][rep][v] = (j["seconds"], j["digest"])
last, others = V[-1], list(reversed(V[:-1]))
print("case np phase reps " + " ".join(f"{v}_s" for v in V) + " "
      + " ".join(f"{last}/{o}_median min max" for o in others) + " distinct_digests")
for (c, n, ph), reps in sorted(d.items()):
    ks = [r for r in reps if len(reps[r]) == len(V)]
    med = [st.median(reps[r][v][0] for r in ks) for v in V]
    cells = []
    for o in others:
        rat = [reps[r][last][0] / reps[r][o][0] for r in ks]
        cells += [f"{st.median(rat):.3f}", f"{min(rat):.3f}", f"{max(rat):.3f}"]
    dg = {reps[r][v][1] for r in ks for v in V}
    print(c, n, ph, len(ks), *(f"{m:.6f}" for m in med), *cells, len(dg))
