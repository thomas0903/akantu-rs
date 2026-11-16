#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): static-solve medians and
# paired ratios of the c1_series.sh campaigns. Run inside the OUT directory:
#   python3 -I c1_summary.py H

import glob
import re
import statistics as st
import sys

H = sys.argv[1]
rows = {}
seen = set()
for d in sorted(glob.glob(f"h{H}-*-r*")):
    v = d.split("-")[1]
    k = int(d.split("-r")[-1])
    with open(d + "/summary.md") as f:
        lines = f.readlines()
    for line in lines:
        if (
            re.match(r"\| (cppeig|rusteig) \| 1 \| rep1 \|", line)
            and line.count("|") > 18
            and (line.split("|")[1].strip() + str(k) + v) not in seen
        ):
            seen.add(line.split("|")[1].strip() + str(k) + v)
            c = [x.strip() for x in line.split("|")[1:-1]]
            case = c[0] + ("" if c[0] == "cppeig" else "-" + v)
            rows.setdefault(case, []).append((k, float(c[5]), float(c[3]), c[15]))
for case, r in sorted(rows.items()):
    s = [x[1] for x in r]
    print(
        f"{case:16s} n={len(s)} static_solve median {st.median(s):.3f} [{min(s):.3f}, {max(s):.3f}] s; wall median {st.median([x[2] for x in r]):.2f}; run peak rank (MiB) {sorted({x[3] for x in r})}"
    )
a = {k: s for k, s, _, _ in rows.get("rusteig-after", [])}
b = {k: s for k, s, _, _ in rows.get("rusteig-before", [])}
c = {k: s for k, s, _, _ in rows.get("cppeig", [])}
for name, ref in (("after/before", b), ("after/cpp", c), ("before/cpp", None)):
    if name == "before/cpp":
        rr = [b[k] / c[k] for k in b if k in c]
    else:
        rr = [a[k] / ref[k] for k in a if k in ref]
    if rr:
        print(
            f"{name}: {st.median(rr):.3f} [{min(rr):.3f}, {max(rr):.3f}] (n={len(rr)})"
        )
