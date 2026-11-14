#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: (D-043) aggregation of the corpus comparison.
#
# Reads the `fixture-diff --json` report of <committed>/fixtures vs
# <staged>/fixtures and the staged manifests, and prints / writes per-area
# counts: cases regenerated on x86_64 (staged build.arch == x86_64), cases not
# regenerated (staged manifest still aarch64: untouched copy), quantities
# bit-identical / numerically equal / differing, and every differing quantity
# with its max ULP and relative difference.
#
# Usage: summarize.py <fixture-diff.json> <staged-fixtures-root> [--json OUT]
import argparse
import collections
import glob
import json
import os
import sys


def area_of(case_id):
    parts = case_id.split("/")
    if parts[0] == "model" and len(parts) > 2:
        return "/".join(parts[:2])
    return parts[0]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("report")
    ap.add_argument("staged")
    ap.add_argument("--json", dest="out")
    o = ap.parse_args()
    rep = json.load(open(o.report))
    arch = {}
    for f in glob.glob(os.path.join(o.staged, "**", "manifest.json"), recursive=True):
        try:
            m = json.load(open(f))
        except (OSError, ValueError):
            continue
        cid = m.get("case_id") or os.path.relpath(os.path.dirname(f), o.staged)
        arch[cid] = (m.get("build") or {}).get("arch", "?")
    areas = collections.defaultdict(lambda: collections.Counter())
    diffs = []
    per_case_q = collections.defaultdict(list)
    for q in rep["quantities"]:
        per_case_q[q["case_id"]].append(q)
    for cid, qs in per_case_q.items():
        a = areas[area_of(cid)]
        regen = arch.get(cid) == "x86_64"
        a["cases_regenerated" if regen else "cases_not_regenerated"] += 1
        if not regen:
            continue
        case_diff = False
        for q in qs:
            a["quantities"] += 1
            if q["n_violations"] == 0:
                a["quantities_equal"] += 1
            else:
                case_diff = True
                mu = (q.get("max_ulp") or {}).get("ulps")
                mr = (q.get("max_rel_err") or {}).get("rel_err")
                ma = (q.get("max_abs_err") or {}).get("abs_err")
                diffs.append({"case": cid, "quantity": q["quantity"], "n_diff": q["n_violations"],
                              "n": q["n_compared"], "max_ulp": mu, "max_rel": mr, "max_abs": ma})
        a["cases_with_differences" if case_diff else "cases_identical"] += 1
    bits_only = rep.get("bits_only_differences", [])
    problems = [p for p in rep.get("problems", [])]
    print(f"{'area':28s} {'regen':>6s} {'not':>5s} {'same':>5s} {'diff':>5s} {'quant':>7s} {'q_eq':>7s}")
    for k in sorted(areas):
        a = areas[k]
        print(f"{k:28s} {a['cases_regenerated']:6d} {a['cases_not_regenerated']:5d} {a['cases_identical']:5d} "
              f"{a['cases_with_differences']:5d} {a['quantities']:7d} {a['quantities_equal']:7d}")
    print(f"\ndiffering quantities: {len(diffs)}; bits-only (numerically equal, bits differ): {len(bits_only)}")
    for d in sorted(diffs, key=lambda d: (d["case"], d["quantity"])):
        print(f"  {d['case']:60s} {d['quantity']:30s} {d['n_diff']:5d}/{d['n']:<6d} ulp={d['max_ulp']} rel={d['max_rel']}")
    if problems:
        print(f"\nproblems ({len(problems)}):")
        for p in problems[:40]:
            print("  " + p[:220])
    if o.out:
        json.dump({"areas": {k: dict(v) for k, v in areas.items()}, "differences": diffs,
                   "bits_only": bits_only, "problems": problems}, open(o.out, "w"), indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
