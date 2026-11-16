#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): classify the
# differences between two result dumps of minmax_probe.rs (two builds).
# Per operation and kind: the number of differing results, and how many of
# them are NaN in both builds (which NaN an operation with NaN operands
# returns: payload and sign selection), and how many are anything else
# (a different number, a different zero sign, NaN against non-NaN).
#   minmax_compare.py DUMP_A DUMP_B
import sys
from collections import Counter


def isnan(bits):
    return (bits >> 52) & 0x7FF == 0x7FF and bits & ((1 << 52) - 1) != 0


def load(p):
    return {tuple(l.split()[:3]): l.split()[3:] for l in open(p)}


a, b = load(sys.argv[1]), load(sys.argv[2])
nan_nan, other, total = Counter(), Counter(), Counter()
examples = {}
for k, va in a.items():
    vb = b[k]
    op = k[0] + " " + k[1]
    total[op] += 1
    ra, rb = int(va[2], 16), int(vb[2], 16)
    if ra == rb:
        continue
    if isnan(ra) and isnan(rb):
        nan_nan[op] += 1
    else:
        other[op] += 1
        examples.setdefault(op, (va, vb))
bad = 0
for op in total:
    if nan_nan[op] or other[op]:
        print(f"{op:18} differing {nan_nan[op] + other[op]:4} of {total[op]}: NaN in both builds {nan_nan[op]}, other {other[op]}"
              + (f"  e.g. a={examples[op][0][0]} b={examples[op][0][1]} -> {examples[op][0][2]} vs {examples[op][1][2]}" if op in examples else ""))
        bad += other[op]
print("only NaN selections differ" if bad == 0 else f"{bad} NON-NaN DIFFERENCES")
sys.exit(1 if bad else 0)
