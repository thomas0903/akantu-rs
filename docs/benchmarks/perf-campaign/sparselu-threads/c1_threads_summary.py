#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): static-solve medians
# of the c1_threads.sh campaigns (run inside the OUT directory).
#   python3 -I c1_threads_summary.py H
import glob
import re
import statistics as st
import sys

h = sys.argv[1]
rows = {}
bitwise = {}
for d in sorted(glob.glob(f"h{h}-*-r*")):
    variant = d[len(f"h{h}-") : d.rindex("-r")]
    with open(d + "/summary.md") as f:
        text = f.read()
    for line in text.splitlines():
        if (
            re.match(r"\| (cppeig|rusteig) \| 1 \| rep1 \|", line)
            and line.count("|") > 18
        ):
            c = [x.strip() for x in line.split("|")[1:-1]]
            rows.setdefault(variant, []).append((float(c[5]), c[15], c[13]))
            break
cpp = st.median([r[0] for r in rows.get("cpp", [])]) if "cpp" in rows else None
base = st.median([r[0] for r in rows.get("base-t1", [])])
order = ["cpp", "base-t1", "head-t1", "head-t2", "head-t4", "head-t8"]
for v in order:
    if v not in rows:
        continue
    s = [r[0] for r in rows[v]]
    med = st.median(s)
    line = (
        f"{v:8s} n={len(s)} static_solve {med:.3f} [{min(s):.3f}, {max(s):.3f}] s"
        f"  /base {med / base:.3f}"
    )
    if cpp:
        line += f"  /C++ {med / cpp:.3f}"
    line += f"  peak {sorted({r[1] for r in rows[v]})}  E_p {sorted({r[2] for r in rows[v]})}"
    print(line)
