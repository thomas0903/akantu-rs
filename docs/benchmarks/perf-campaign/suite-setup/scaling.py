#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): growth exponents of the
# set-up suite's phases between problem sizes.
"""Growth exponents of suite phases with the problem size.

    scaling.py [--per-call] DIR_OR_JSONL...

Every input holds the runs of one size (run_ab.sh directories or JSON-lines
files). Per (case, phase): the median seconds per size, the element count
per size and the exponent b of t ~ N^b between consecutive sizes
(b = log(t2/t1) / log(N2/N1)). Loop phases are compared per call. A phase
whose exponent exceeds 1.15 between the two largest sizes is marked.
"""

import json
import math
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path


def rows(inp):
    p = Path(inp)
    files = sorted(p.rglob("*.jsonl")) if p.is_dir() else [p]
    for f in files:
        for line in f.read_text().splitlines():
            if line.startswith("{"):
                yield json.loads(line)


def main(argv):
    data = defaultdict(lambda: defaultdict(list))  # (case, phase) -> size -> [t]
    counts = {}
    order = []
    for inp in argv:
        for d in rows(inp):
            key = (d["case"], d["phase"])
            if key not in data:
                order.append(key)
            t = d["seconds"] / max(d["calls"], 1)
            data[key][d["size"]].append(t)
            counts[(d["case"], d["size"])] = max(d["n_elements"], 1)
    sizes = [s for s in "sml" if any(s in v for v in data.values())]
    print("| case | phase | " + " | ".join(f"{s}: N, median" for s in sizes)
          + " | " + " | ".join(f"b {a}→{b}" for a, b in zip(sizes, sizes[1:])) + " |")
    print("|---|---|" + "---|" * (len(sizes) + len(sizes) - 1))
    for key in order:
        case, phase = key
        med = {s: statistics.median(v) for s, v in data[key].items()}
        cells = []
        for s in sizes:
            if s in med:
                cells.append(f"{counts[(case, s)]:,}, {med[s]:.4g} s")
            else:
                cells.append("–")
        bs = []
        for a, b in zip(sizes, sizes[1:]):
            na, nb = counts.get((case, a)), counts.get((case, b))
            if a in med and b in med and na and nb and nb != na and med[a] > 0:
                e = math.log(med[b] / med[a]) / math.log(nb / na)
                mark = " **>1**" if (b == sizes[-1] and e > 1.15) else ""
                bs.append(f"{e:.2f}{mark}")
            else:
                bs.append("–")
        print(f"| {case} | {phase} | " + " | ".join(cells) + " | " + " | ".join(bs) + " |")


if __name__ == "__main__":
    main(sys.argv[1:])
