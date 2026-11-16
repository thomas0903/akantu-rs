#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): table of the clean
# build times (paired per round against `base`) and the binary sizes of
# build_variant.sh output lines carrying a "round" field.
#   buildtime_summary.py BUILDTIME.jsonl
import json
import statistics as st
import sys

rows = [json.loads(line) for line in open(sys.argv[1]) if line.startswith("{")]
by = {}
for r in rows:
    by.setdefault(r["variant"], {})[r["round"]] = r
base = by["base"]
print("| variant | clean build [s] per round | / base (per round) | explicit_step_timing: file / .text [MB] | cohesive_extrinsic: file / .text [MB] | facet_scaling .text [MB] | load at start |")
print("|---|---|---|---|---|---|---|")
for v, d in by.items():
    ks = sorted(d)
    t = ", ".join(f"{d[k]['build_s']:.0f}" for k in ks)
    rat = ", ".join(f"{d[k]['build_s'] / base[k]['build_s']:.2f}" for k in ks if k in base)
    r = d[ks[0]]
    loads = ", ".join(d[k]["load"].split()[0] for k in ks)
    print(f"| {v} | {t} | {rat} | {r['explicit_bytes'] / 1e6:.1f} / {r['explicit_text'] / 1e6:.2f} | "
          f"{r['cohesive_bytes'] / 1e6:.1f} / {r['cohesive_text'] / 1e6:.2f} | {r['facet_text'] / 1e6:.2f} | {loads} |")
