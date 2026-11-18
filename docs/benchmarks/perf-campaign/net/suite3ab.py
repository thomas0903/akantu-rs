#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): three-way paired comparison of the
# shared-memory runs of the fix round, suite3/{before,after,r3}: per case, ranks and
# phase, the median seconds of each build, the paired ratios r3/after and r3/before
# (median, min, max) and the number of distinct result digests over the three builds.
#   python3 -I suite3ab.py > suite3-ab.txt   (in this directory)
import collections
import glob
import json
import statistics as st

d = collections.defaultdict(lambda: collections.defaultdict(dict))
V = ("before", "after", "r3")
for v in V:
    for f in glob.glob(f"suite3/{v}/np*/*.jsonl"):
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
print("case np phase reps before_s after_s r3_s r3/after_median min max r3/before_median min max distinct_digests")
for (c, n, ph), reps in sorted(d.items()):
    ks = [r for r in reps if len(reps[r]) == 3]
    med = [st.median(reps[r][v][0] for r in ks) for v in V]
    ra = [reps[r]["r3"][0] / reps[r]["after"][0] for r in ks]
    rb = [reps[r]["r3"][0] / reps[r]["before"][0] for r in ks]
    dg = {reps[r][v][1] for r in ks for v in V}
    print(c, n, ph, len(ks), *(f"{m:.6f}" for m in med),
          f"{st.median(ra):.3f}", f"{min(ra):.3f}", f"{max(ra):.3f}",
          f"{st.median(rb):.3f}", f"{min(rb):.3f}", f"{max(rb):.3f}", len(dg))
