#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream counterpart:
# the judgement of the full-size rerun (two runs of the port at two rank counts)
# against the statement written before it, docs/DECISIONS.md D-108, D-105 (4).
"""Judge two runs of the cohesive benchmark at different rank counts against
D-108's statement (docs/DECISIONS.md, "D-105 (4), the statement written before
the full-size rerun").

    python3 -I d108_rerun.py RUN_A RUN_B [--through 1122]

RUN_A, RUN_B: run directories of the campaign tool (each holds summary.json,
energies.csv with the columns step, ed, ep, ek, ew, et, and insertions.csv with
the columns step, nb_inserted, nb_cohesive_total, one row per insertion step).

Point 1, must hold (verdict "D108: PASS" only if all three do):
  (a) both summaries record rust_cohesive_compat = upstream and
      rust_doubled_node_load = follows_elements;
  (b) up to the first step where the global insertion counts differ, the
      external work of the two runs agrees to rounding: less than 1e-10 of the
      largest energy of the step, and no jump of more than two orders of
      magnitude in one step (counted for differences above 1e-13 of the step's
      largest energy; note of 2026-11-13 in D-108);
  (c) the counts agree through step 1,122 (--through) and beyond it.

Point 2, reported with numbers, not judged: the step of the first count
difference; the first steps where the largest energy difference exceeds 1e-12
and 1e-10 of the largest energy; which energy leaves rounding first; the final
counts and energies with their relative differences; the difference of the
static solves at step 0.

The comparison between rank counts is not the pair check of mn_compare.py
(same rank count, same partition): nothing here is expected to be bitwise.
"""
import csv
import json
import sys
from pathlib import Path

ENERGIES = ("ed", "ep", "ek", "ew", "et")
NAMES = {"ed": "dissipated", "ep": "potential", "ek": "kinetic", "ew": "external work", "et": "total"}
SETTINGS = {"rust_cohesive_compat": "upstream", "rust_doubled_node_load": "follows_elements"}
BOUND = 1e-10
FLOOR = 1e-13
JUMP = 100.0


def read_energies(run: Path) -> dict[int, dict[str, float]]:
    out = {}
    with open(run / "energies.csv", newline="") as f:
        for row in csv.DictReader(f, skipinitialspace=True):
            out[int(row["step"])] = {k: float(row[k]) for k in ENERGIES}
    return out


def read_counts(run: Path) -> dict[int, int]:
    """Cumulative number of cohesive elements after each insertion step."""
    out = {}
    with open(run / "insertions.csv", newline="") as f:
        for row in csv.DictReader(f, skipinitialspace=True):
            out[int(row["step"])] = int(row["nb_cohesive_total"])
    return out


def total_at(counts: dict[int, int], steps: list[int]) -> dict[int, int]:
    """The cumulative count at every step (constant between insertion steps)."""
    out, cur, keys, i = {}, 0, sorted(counts), 0
    for s in steps:
        while i < len(keys) and keys[i] <= s:
            cur = counts[keys[i]]
            i += 1
        out[s] = cur
    return out


def main(argv: list[str]) -> int:
    args = [a for a in argv if not a.startswith("--")]
    through = 1122
    if "--through" in argv:
        through = int(argv[argv.index("--through") + 1])
        args = [a for a in args if a != str(through)]
    if len(args) != 2:
        print(__doc__)
        return 2
    run_a, run_b = Path(args[0]), Path(args[1])
    sa, sb = (json.load(open(r / "summary.json")) for r in (run_a, run_b))
    ea, eb = read_energies(run_a), read_energies(run_b)
    ca, cb = read_counts(run_a), read_counts(run_b)
    steps = sorted(set(ea) & set(eb))
    if not steps:
        print("D108: FAIL (no common step in the two energies.csv)")
        return 1
    print(f"runs: A = {run_a} (psize {sa.get('psize')}), B = {run_b} (psize {sb.get('psize')})")
    print(f"steps compared: {steps[0]} … {steps[-1]} ({len(steps)}; A has {len(ea)}, B has {len(eb)})")
    failures = []

    # 1 (a)
    for key, want in SETTINGS.items():
        for name, s in (("A", sa), ("B", sb)):
            got = s.get(key)
            if got != want:
                failures.append(f"(a) run {name}: {key} = {got!r}, not {want!r}")
    print("1 (a) settings: " + ("both runs record " + ", ".join(f"{k} = {v}" for k, v in SETTINGS.items())
                               if not any(f.startswith("(a)") for f in failures) else "NOT as required"))

    # the first step where the global insertion counts differ
    ta, tb = total_at(ca, steps), total_at(cb, steps)
    first_diff = next((s for s in steps if ta[s] != tb[s]), None)
    print(f"2: first step where the cumulative cohesive counts differ: "
          f"{first_diff if first_diff is not None else 'none in the compared steps'}"
          + (f" (A {ta[first_diff]}, B {tb[first_diff]})" if first_diff is not None else ""))

    # 1 (c)
    if steps[-1] < through:
        failures.append(f"(c) the runs end at step {steps[-1]}, before step {through}")
    elif first_diff is not None and first_diff <= through:
        failures.append(f"(c) the counts differ at step {first_diff}, not after step {through}")
    print(f"1 (c) counts through step {through}: "
          + ("agree, and beyond it" + (f" up to step {first_diff - 1}" if first_diff is not None else " to the end")
             if not any(f.startswith("(c)") for f in failures) else "DO NOT agree"))

    # 1 (b): the external work up to the first count difference
    last_b = (first_diff - 1) if first_diff is not None else steps[-1]
    worst_ew, worst_ew_step, prev_rel, worst_jump, worst_jump_step = 0.0, None, None, 0.0, None
    for s in steps:
        if s > last_b:
            break
        scale = max(abs(v) for run in (ea[s], eb[s]) for v in run.values())
        if scale == 0.0:
            prev_rel = None
            continue
        rel = abs(ea[s]["ew"] - eb[s]["ew"]) / scale
        if rel > worst_ew:
            worst_ew, worst_ew_step = rel, s
        if prev_rel is not None and rel > FLOOR:
            jump = rel / max(prev_rel, FLOOR)
            if jump > worst_jump:
                worst_jump, worst_jump_step = jump, s
        prev_rel = rel
    if worst_ew >= BOUND:
        failures.append(f"(b) external work: {worst_ew:.3e} of the largest energy at step {worst_ew_step} (bound {BOUND:g})")
    if worst_jump > JUMP:
        failures.append(f"(b) external work: a jump of a factor {worst_jump:.3g} in one step at step {worst_jump_step}")
    print(f"1 (b) external work up to step {last_b}: largest difference {worst_ew:.3e} of the step's largest energy"
          f" (step {worst_ew_step}); largest one-step growth above the floor: factor {worst_jump:.3g}"
          + (f" (step {worst_jump_step})" if worst_jump_step is not None else ""))

    # 2: reported
    first12 = first10 = None
    leaves = {}
    for s in steps:
        scale = max(abs(v) for run in (ea[s], eb[s]) for v in run.values())
        if scale == 0.0:
            continue
        rels = {k: abs(ea[s][k] - eb[s][k]) / scale for k in ENERGIES}
        m = max(rels.values())
        if first12 is None and m > 1e-12:
            first12 = s
        if first10 is None and m > 1e-10:
            first10 = s
        for k, r in rels.items():
            if k not in leaves and r > 1e-10:
                leaves[k] = s
    print(f"2: first step where the largest energy difference exceeds 1e-12 of the largest energy: {first12}; 1e-10: {first10}")
    if leaves:
        order = sorted(leaves.items(), key=lambda kv: kv[1])
        print("2: energies leaving 1e-10, in order: " + ", ".join(f"{NAMES[k]} at step {s}" for k, s in order))
    else:
        print("2: no energy leaves 1e-10 of the largest energy in the compared steps")
    s0 = steps[0]
    scale0 = max(abs(v) for run in (ea[s0], eb[s0]) for v in run.values()) or 1.0
    print(f"2: static solves at step {s0}: potential energy A {ea[s0]['ep']!r}, B {eb[s0]['ep']!r}, "
          f"difference {abs(ea[s0]['ep'] - eb[s0]['ep']) / scale0:.3e} of the largest energy")
    sl = steps[-1]
    print(f"2: final (step {sl}): cohesive elements A {ta[sl]}, B {tb[sl]}; insertion steps A {len(ca)}, B {len(cb)}")
    for k in ENERGIES:
        a, b = ea[sl][k], eb[sl][k]
        den = max(abs(a), abs(b)) or 1.0
        print(f"   {NAMES[k]:14s} A {a!r}  B {b!r}  relative difference {abs(a - b) / den:.3e}")

    if failures:
        print("D108: FAIL")
        for f in failures:
            print("  " + f)
        return 1
    print("D108: PASS (points 1 (a), 1 (b), 1 (c))")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
