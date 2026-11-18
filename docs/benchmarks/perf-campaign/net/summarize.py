#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): tables of the multi-host series
# of this directory from runs.csv (one line per run, written by collect.py).
#
#   python3 -I summarize.py runs.csv PREFIX [EXCLUDED_RUN,...]
#
# Runs are named PREFIX-<case>-k<repetition>, <case> = <c|r|b|a><ranks>x<hosts> (c: the C++
# reference, r: the port in the diagnosis series, b / a: the port before / after D-126) or
# <variant>-<ranks>x<hosts>. Prints the median [min, max] of
# each phase per case and the paired ratios (same repetition) named in RATIOS.
import csv
import re
import statistics
import sys

PHASES = ("loop", "check_cohesive_stress", "solve_step", "energies", "interpolation", "insertion")
# r: the port (diagnosis series); b / a: the port before / after D-126 (final series); c: C++
RATIOS = (("r8x4", "r8x1"), ("c8x4", "c8x1"), ("r8x1", "c8x1"), ("r8x4", "c8x4"), ("r32x4", "c32x4"),
          ("a8x1", "b8x1"), ("a8x4", "b8x4"), ("a32x4", "b32x4"),
          ("b8x4", "b8x1"), ("a8x4", "a8x1"),
          ("b8x1", "c8x1"), ("a8x1", "c8x1"), ("b8x4", "c8x4"), ("a8x4", "c8x4"),
          ("b32x4", "c32x4"), ("a32x4", "c32x4"))


def fmt(v):
    return f"{statistics.median(v):.1f} [{min(v):.1f}, {max(v):.1f}]" if v else "-"


def fmt3(v):
    return f"{statistics.median(v):.3f} [{min(v):.3f}, {max(v):.3f}]" if v else "-"


def main(path, prefix, excluded=()):
    with open(path, newline="") as f:
        table = list(csv.DictReader(f))
    rows = [r for r in table if r["run"].startswith(prefix + "-") and r["outcome"] == "ok"
            and r["run"] not in excluded]
    cases = {}
    for r in rows:
        mo = re.fullmatch(re.escape(prefix) + r"-(.+)-k([0-9]+)", r["run"])
        if mo:
            cases.setdefault(mo.group(1), {})[int(mo.group(2))] = r
    print("| case | n | " + " | ".join(PHASES) + " |")
    print("|---|---|" + "---|" * len(PHASES))
    for c in sorted(cases):
        reps = cases[c].values()
        cells = [fmt([float(r[p]) for r in reps if r[p] != ""]) for p in PHASES]
        print(f"| {c} | {len(cases[c])} | " + " | ".join(cells) + " |")
    print()
    print("| ratio (paired by repetition) | loop | check_cohesive_stress | solve_step | energies |")
    print("|---|---|---|---|---|")
    for a, b in RATIOS:
        if a not in cases or b not in cases:
            continue
        ks = sorted(set(cases[a]) & set(cases[b]))
        cells = [fmt3([float(cases[a][k][p]) / float(cases[b][k][p]) for k in ks])
                 for p in ("loop", "check_cohesive_stress", "solve_step", "energies")]
        print(f"| {a} / {b} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3].split(",") if len(sys.argv) > 3 else ())
