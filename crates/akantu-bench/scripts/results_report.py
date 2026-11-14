#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the tables of the runs of 13 November (the
# full-size rerun with the corrected load placement, its controls, the
# multi-node runs and the per-copy check at full size) computed from the
# committed run files alone. Standard library only.
"""Tables of the runs of 13 November, from the committed files only.

    python3 -I crates/akantu-bench/scripts/results_report.py [--root DIR] [SECTION ...]

SECTION is one or more of `rerun`, `controls`, `percopy`, `smoke`,
`multinode`, `joss` (default: all, in that order). DIR is the repository root
(default: three levels above this script). Nothing is written; the output
committed as `docs/benchmarks/results-2026-11-13.txt` is the output of the
command without arguments (the offline test regenerates and compares it).

Definitions (D-108 "D-105 (4)" and its note):

* two runs A, B of the JOSS problem; d_k(s) = A_k(s) - B_k(s) for the five
  energies k of `energies.csv` (ed, ep, ek, ew, et; ew is cumulative);
  L(s) = the step's largest energy: the largest absolute value of the ten
  numbers of both runs at step s; r_k(s) = |d_k(s)| / L(s);
* the cumulative cohesive count c(s) from `insertions.csv` (the count after
  the last insertion at or before s); the first count difference is the
  first step at which c_A != c_B; "the window" is every step before it;
* D-108's one-step growth of the external work: r_ew(s) / max(r_ew(s-1),
  1e-13), counted where r_ew(s) > 1e-13 (the floor of that note);
* the envelope ratio J_ew(s) = |d_ew(s) - d_ew(s-1)| /
  max(max_{s-50 <= t < s} |d_ew(t)|, 1e-13 L(s));
* the injection test: the external-work signature of the 12 November
  departure, d_ew(1122 + k) - d_ew(1121) for k = 0 ... 18, read from the
  committed detail table of the 12 November analysis
  (`docs/benchmarks/c1-2026-11-12/joss_detail_r64_r32_1100_1140.txt`, the
  64- against the 32-rank run of that day), added unscaled to A's d_ew from an
  onset s0 on; a criterion detects it at s0 when it fires at one of the
  steps s0 ... s0 + 18; the window of a criterion is the last onset up to
  which every onset from step 700 on is detected.

Times: rank 0's Chrono totals (`chrono.csv`); initialisation = read_mesh +
dist_mesh + init_full + initial_conditons + static_solve + before_step,
loop = boundary_conditions + check_cohesive_stress + solve_step + energies
(+ load_check); wall = the launcher's measured run time (`wall.txt`). One
run per point.
"""

import json
import math
import statistics
import sys
from pathlib import Path

E = ("ed", "ep", "ek", "ew", "et")
NAME = {"ed": "dissipated", "ep": "potential", "ek": "kinetic", "ew": "external work", "et": "total"}
FLOOR = 1e-13
WIN = 50
INIT_PHASES = ("read_mesh", "dist_mesh", "init_full", "initial_conditons", "static_solve", "before_step")
LOOP_PHASES = ("boundary_conditions", "check_cohesive_stress", "solve_step", "energies", "load_check")
RERUN = "docs/benchmarks/c1-rerun-2026-11-13"
MN = "docs/benchmarks/mn-2026-11-13"
PRE = "docs/benchmarks/c1-2026-11-12"
DETAIL = "docs/benchmarks/c1-2026-11-12/joss_detail_r64_r32_1100_1140.txt"
SETTINGS = {"rust_cohesive_compat": "upstream", "rust_doubled_node_load": "follows_elements"}


# ---------------------------------------------------------------- reading

def read_energies(path):
    """step -> {k: value} from an energies.csv (header `step, ed, ep, ek, ew, et`)."""
    out = {}
    lines = Path(path).read_text().splitlines()
    head = [h.strip() for h in lines[0].split(",")]
    if head != ["step", *E]:
        raise ValueError(f"{path}: unexpected header {head}")
    for line in lines[1:]:
        if not line.strip():
            continue
        parts = [p.strip() for p in line.split(",")]
        out[int(parts[0])] = dict(zip(E, (float(x) for x in parts[1:6])))
    return out


def read_insertions(path):
    """[(step, inserted, total)] from an insertions.csv."""
    rows = []
    lines = Path(path).read_text().splitlines()
    for line in lines[1:]:
        if line.strip():
            s, n, t = (int(p) for p in line.split(","))
            rows.append((s, n, t))
    return rows


def cumulative(rows, last_step):
    """c[s] for s = 0 ... last_step (the count after the insertions of step s)."""
    c, i, cur = [], 0, 0
    for s in range(last_step + 1):
        while i < len(rows) and rows[i][0] == s:
            cur = rows[i][2]
            i += 1
        c.append(cur)
    return c


def per_step(rows):
    return {s: n for s, n, _ in rows}


def read_chrono(path):
    """{phase: seconds} and {phase: repetitions} from a chrono.csv (rank 0)."""
    lines = Path(path).read_text().splitlines()
    head = [h.strip().strip('"') for h in lines[0].split(",")]
    vals = [v.strip() for v in lines[1].split(",")]
    sec, rep = {}, {}
    for h, v in zip(head, vals):
        if h == "psize":
            continue
        if h.endswith(" nb_rep"):
            rep[h[: -len(" nb_rep")]] = int(float(v))
        else:
            sec[h] = float(v)
    return sec, rep


def read_status(path):
    """key = value lines (JSON values where they parse) of a status.txt."""
    out = {}
    for line in Path(path).read_text().splitlines():
        if "=" not in line:
            continue
        k, v = line.split("=", 1)
        try:
            out[k] = json.loads(v)
        except json.JSONDecodeError:
            out[k] = v
    return out


def wall(run_dir):
    return float(Path(run_dir, "wall.txt").read_text().split()[0])


def summary(run_dir):
    return json.loads(Path(run_dir, "summary.json").read_text())


# --------------------------------------------------------------- formatting

ROOT = [Path(".")]


def shown(path):
    """A path relative to the repository root when it lies inside it."""
    try:
        return str(Path(path).relative_to(ROOT[0]))
    except ValueError:
        return str(path)


def g(x, digits=2):
    """x with `digits` significant digits in e-notation (rounded half-even by format)."""
    if x == 0:
        return "0"
    return f"{x:.{digits - 1}e}"


def n(x):
    return f"{x:,}"


def table(head, rows):
    out = ["| " + " | ".join(head) + " |", "|" + "|".join("---" for _ in head) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


# ---------------------------------------------------------- pair analysis

class Pair:
    """Two runs of the same problem: energies, counts and the measures of D-108."""

    def __init__(self, a_dir, b_dir):
        self.a_dir, self.b_dir = Path(a_dir), Path(b_dir)
        ea, eb = read_energies(self.a_dir / "energies.csv"), read_energies(self.b_dir / "energies.csv")
        last = min(max(ea), max(eb))
        self.steps = list(range(last + 1))
        if any(s not in ea or s not in eb for s in self.steps):
            raise ValueError("energies.csv: a step is missing")
        self.ea, self.eb = ea, eb
        self.L = [max(abs(v) for run in (ea[s], eb[s]) for v in run.values()) for s in self.steps]
        self.d = {k: [ea[s][k] - eb[s][k] for s in self.steps] for k in E}
        self.r = {k: [abs(self.d[k][s]) / self.L[s] if self.L[s] else 0.0 for s in self.steps] for k in E}
        self.rmax = [max(self.r[k][s] for k in E) for s in self.steps]
        self.ia = read_insertions(self.a_dir / "insertions.csv")
        self.ib = read_insertions(self.b_dir / "insertions.csv")
        self.ca, self.cb = cumulative(self.ia, last), cumulative(self.ib, last)
        self.first_count = next((s for s in self.steps if self.ca[s] != self.cb[s]), None)
        self.last_equal = (self.first_count - 1) if self.first_count is not None else last

    def first_above(self, t, k=None, upto=None):
        series = self.rmax if k is None else self.r[k]
        hi = len(self.steps) - 1 if upto is None else upto
        return next((s for s in range(hi + 1) if series[s] > t), None)

    def largest(self, lo, hi, k=None):
        """(value, step, energy) of the largest r over steps lo ... hi."""
        best = (0.0, None, None)
        for s in range(lo, hi + 1):
            for kk in (E if k is None else (k,)):
                if self.r[kk][s] > best[0]:
                    best = (self.r[kk][s], s, kk)
        return best

    def d108_growth(self, hi):
        """D-108's largest one-step growth of r_ew above the floor, steps 1 ... hi: (factor, step)."""
        best, step = 0.0, None
        for s in range(1, hi + 1):
            r1, r0 = self.r["ew"][s], self.r["ew"][s - 1]
            if r1 > FLOOR:
                f = r1 / max(r0, FLOOR)
                if f > best:
                    best, step = f, s
        return best, step

    def j(self, k, s, d=None):
        dd = self.d[k] if d is None else d
        back = max((abs(dd[t]) for t in range(max(0, s - WIN), s)), default=0.0)
        return abs(dd[s] - dd[s - 1]) / max(back, FLOOR * self.L[s])

    def j_max(self, k, hi):
        best, step = 0.0, None
        for s in range(1, hi + 1):
            v = self.j(k, s)
            if v > best:
                best, step = v, s
        return best, step

    def zero_crossing_before(self, k, s):
        """The last t <= s at which d_k changes sign between t - 1 and t, or None."""
        for t in range(s, 0, -1):
            if self.d[k][t - 1] * self.d[k][t] < 0:
                return t
        return None

    def final(self):
        s = self.steps[-1]
        out = {}
        for k in E:
            a, b = self.ea[s][k], self.eb[s][k]
            out[k] = abs(a - b) / max(abs(a), abs(b)) if max(abs(a), abs(b)) else 0.0
        return out

    def inject(self, template, lo=700):
        """The window of each criterion for the injected 12 November signature."""
        hi = self.last_equal - len(template)
        dew = self.d["ew"]
        res = []
        for s0 in range(lo, hi + 1):
            dd = list(dew)
            for k, x in enumerate(template):
                dd[s0 + k] += x
            g108 = jmax = 0.0
            for s in range(s0, s0 + len(template)):
                r1, r0 = abs(dd[s]) / self.L[s], abs(dd[s - 1]) / self.L[s - 1]
                if r1 > FLOOR:
                    g108 = max(g108, r1 / max(r0, FLOOR))
                jmax = max(jmax, self.j("ew", s, dd))
            res.append((s0, g108, jmax))

        def window(cond):
            miss = next((s0 for s0, gg, jj in res if not cond(gg, jj)), None)
            return (miss - 1) if miss is not None else res[-1][0]

        return {"d108": window(lambda gg, jj: gg > 100), "j1": window(lambda gg, jj: jj > 1),
                "j2": window(lambda gg, jj: jj > 2), "onsets": (lo, hi)}


def template_12nov(root):
    dew = {}
    for line in Path(root, DETAIL).read_text().splitlines():
        parts = [x.strip() for x in line.split("|")]
        if len(parts) == 14 and parts[0].isdigit():
            dew[int(parts[0])] = float(parts[10])
    return [dew[1122 + k] - dew[1121] for k in range(19)]


def time_step_rel(a_dir, b_dir):
    ta, tb = summary(a_dir)["time_step"], summary(b_dir)["time_step"]
    return abs(ta - tb) / max(abs(ta), abs(tb))


def ep0_rel(a_dir, b_dir):
    ea, eb = read_energies(Path(a_dir, "energies.csv")), read_energies(Path(b_dir, "energies.csv"))
    L = max(abs(v) for run in (ea[0], eb[0]) for v in run.values())
    return abs(ea[0]["ep"] - eb[0]["ep"]) / L


def pair_lines(p, label):
    """The D-108 figures of one pair, as text lines."""
    out = [f"## {label}", f"A = {shown(p.a_dir)}", f"B = {shown(p.b_dir)}"]
    sa, sb = summary(p.a_dir), summary(p.b_dir)
    for name, s in (("A", sa), ("B", sb)):
        rec = {k: s.get(k) for k in SETTINGS}
        out.append(f"1 (a) {name}: " + ", ".join(f"{k} = {v}" for k, v in rec.items())
                   + ("  [as required]" if rec == SETTINGS else "  [NOT as required]"))
    fc, le = p.first_count, p.last_equal
    out.append(f"steps compared: 0 ... {p.steps[-1]}")
    if fc is None:
        out.append("cumulative counts: equal at every step")
    else:
        psa, psb = per_step(p.ia), per_step(p.ib)
        nins = sum(1 for s, _, _ in p.ia if s <= le)
        at1122 = (f" (count at step 1,122: {n(p.ca[1122])} / {n(p.cb[1122])})"
                  if len(p.ca) > 1122 else "")
        out.append(f"1 (c) cumulative counts equal through step {n(le)}: {n(p.ca[le])} elements in "
                   f"{n(nins)} insertion steps{at1122}")
        out.append(f"first count difference: step {n(fc)} (cumulative {n(p.ca[fc])} / {n(p.cb[fc])}; inserted "
                   f"in that step {psa.get(fc, 0)} / {psb.get(fc, 0)}; in step {n(fc + 1)}: "
                   f"{psa.get(fc + 1, 0)} / {psb.get(fc + 1, 0)}; cumulative at {n(fc + 1)}: "
                   f"{n(p.ca[fc + 1])} / {n(p.cb[fc + 1])})")
        diff_after = [p.ca[s] - p.cb[s] for s in range(fc, p.steps[-1] + 1)]
        out.append(f"after it: largest |c_A - c_B| {max(abs(x) for x in diff_after)}, "
                   f"at the last step {diff_after[-1]:+d}")
    v, s, k = p.largest(0, le, "ew")
    out.append(f"1 (b) magnitude: r_ew first exceeds 1e-10 at step {p.first_above(1e-10, 'ew', le)}; "
               f"largest r_ew in the window {g(v, 3)} (step {s})")
    gro, gs = p.d108_growth(le)
    if gs is None:
        out.append("1 (b) jump: r_ew never exceeds the floor in the window")
    else:
        zc = p.zero_crossing_before("ew", gs)
        out.append(f"1 (b) jump: largest one-step growth of r_ew above the floor {gro:.1f} at step {gs} "
                   f"(d_ew {p.d['ew'][gs - 1]:+.3e} -> {p.d['ew'][gs]:+.3e} J; last sign change of d_ew "
                   f"between steps {zc - 1 if zc else None} and {zc})")
    jv, js = p.j_max("ew", le)
    out.append(f"envelope ratio J_ew in the window: at most {jv:.2f} (step {js})")
    for kk in ("ep", "ek", "ed", "et"):
        jv2, js2 = p.j_max(kk, le)
        if js2 is None:
            out.append(f"  J_{kk}: 0 at every step")
            continue
        above = [s for s in range(1, le + 1) if p.j(kk, s) > 1]
        out.append(f"  J_{kk} at most {jv2:.2f} (step {js2}; d_{kk} {p.d[kk][js2 - 1]:+.3e} -> {p.d[kk][js2]:+.3e} J, "
                   f"the change {g(abs(p.d[kk][js2] - p.d[kk][js2 - 1]) / p.L[js2], 2)} of L, "
                   f"|{kk}_A| / L {g(abs(p.ea[js2][kk]) / p.L[js2], 2)}); J_{kk} > 1 at {len(above)} steps"
                   + (f": {', '.join(map(str, above[:12]))}{' …' if len(above) > 12 else ''}" if above else ""))
    v, s, k = p.largest(0, le)
    out.append(f"largest r over the five energies in the window: {g(v, 2)} ({NAME[k]}, step {s})")
    v2, s2, k2 = p.largest(0, min(2000, le))
    out.append(f"largest r over steps 0 ... 2,000: {g(v2, 2)} ({NAME[k2]}, step {s2})")
    for t in (1e-13, 1e-12, 1e-11, 1e-10, 1e-9, 1e-8):
        s = p.first_above(t)
        lead = None if s is None else max(E, key=lambda kk: p.r[kk][s])
        out.append(f"first step with max_k r_k > {t:g}: {s} ({NAME[lead] if lead else '-'} largest there"
                   + (f"; cumulative count A {n(p.ca[s])} at that step" if s is not None else "") + ")")
    s12 = p.first_above(1e-12)
    if s12 is not None:
        out.append(f"cumulative count A at step {s12 - 1} (the last step below 1e-12): {n(p.ca[s12 - 1])}")
    s10 = p.first_above(1e-10)
    order = sorted(((p.first_above(1e-10, k), k) for k in E if p.first_above(1e-10, k) is not None))
    out.append("energies first exceeding 1e-10, in order: "
               + ", ".join(f"{NAME[k]} at step {s} (sign of d {'+' if p.d[k][s] > 0 else '-'})" for s, k in order))
    if fc is not None and s10 is not None:
        out.append(f"first count difference minus the first step above 1e-10: {fc - s10} steps")
    for lo in (s12, s10):
        if lo is None or le <= lo:
            continue
        ratios = []
        opposite = 0
        for s in range(lo, le + 1):
            m = max(abs(p.d["ep"][s]), abs(p.d["ek"][s]))
            if m:
                ratios.append(abs(p.d["ew"][s]) / m)
            if p.d["ep"][s] * p.d["ek"][s] < 0:
                opposite += 1
        out.append(f"steps {lo} ... {le}: |d_ew| / max(|d_ep|, |d_ek|) median {statistics.median(ratios):.3f}; "
                   f"d_ep and d_ek of opposite sign at {opposite} of {le - lo + 1} steps")
    s8 = p.first_above(1e-8)
    if s12 is not None and s10 is not None:
        out.append(f"mean growth of max_k r_k: 1e-12 -> 1e-10 in {s10 - s12} steps "
                   f"({(100 ** (1 / (s10 - s12)) - 1) * 100:.1f} % per step)"
                   + (f"; 1e-10 -> 1e-8 in {s8 - s10} steps ({(100 ** (1 / (s8 - s10)) - 1) * 100:.1f} % per step)"
                      if s8 is not None and s8 > s10 else ""))
    fin = p.final()
    s = p.steps[-1]
    out.append(f"final (step {n(s)}): cohesive elements {n(p.ca[s])} / {n(p.cb[s])} "
               f"(relative {g(abs(p.ca[s] - p.cb[s]) / max(p.ca[s], p.cb[s]), 2)}); insertion steps "
               f"{n(len(p.ia))} / {n(len(p.ib))}; energies relative to their own values: "
               + ", ".join(f"{NAME[k]} {g(fin[k], 2)}" for k in E)
               + f"; largest {g(max(fin.values()), 2)}")
    out.append(f"static solves (step 0): r(E_p) {g(ep0_rel(p.a_dir, p.b_dir), 2)}; stable time step "
               f"relative difference {g(time_step_rel(p.a_dir, p.b_dir), 2)}")
    return out, p


# ------------------------------------------------------------- sections

def sec_rerun(root):
    a, b = root / RERUN / "rust-np64-a1/run", root / RERUN / "rust-np32-a1/run"
    lines, p = pair_lines(Pair(a, b), "The full-size rerun: 64 ranks (A) against 32 ranks (B), one node each")
    t = root / RERUN / "test-np64/run"
    lines.append(f"same binary, same 64-rank partition, same host (test-np64 against rust-np64-a1): "
                 f"r(E_p(0)) {g(ep0_rel(t, a), 2)}, stable time step {g(time_step_rel(t, a), 2)}")
    inj = p.inject(template_12nov(root))
    lo, hi = inj["onsets"]
    lines.append(f"injection test (onsets {lo} ... {hi}): detected at every onset up to "
                 f"{inj['d108']} by D-108's growth > 100 (cumulative count A there {n(p.ca[inj['d108']])}), "
                 f"up to {inj['j1']} by J_ew > 1 ({n(p.ca[inj['j1']])}), up to {inj['j2']} by J_ew > 2")
    # the pre-fix pair of 12 November (campaign summaries)
    pre = {}
    for np_ in (64, 32):
        d = json.loads((root / PRE / f"rust-np{np_}-a1/summary.json").read_text())
        pre[np_] = d["runs"][0]["summary"]
    c64, c32 = pre[64]["nb_cohesive_elements"], pre[32]["nb_cohesive_elements"]

    def rel(x, y):
        return abs(x - y) / max(abs(x), abs(y))

    lines.append(f"12 November (upstream's placement), 64 against 32 ranks at step 5,000: cohesive elements "
                 f"{n(c64)} / {n(c32)} (relative {g(rel(c64, c32), 2)}); kinetic "
                 f"{g(rel(pre[64]['energy_kinetic'], pre[32]['energy_kinetic']), 2)}, potential "
                 f"{g(rel(pre[64]['energy_potential'], pre[32]['energy_potential']), 2)}, dissipated "
                 f"{g(rel(pre[64]['energy_dissipated'], pre[32]['energy_dissipated']), 2)}")
    r64 = json.loads((root / RERUN / "rust-np64-a1/summary.json").read_text())["runs"][0]["insertion_steps"]
    p64 = json.loads((root / PRE / "rust-np64-a1/summary.json").read_text())["runs"][0]["insertion_steps"]
    first = next((x for x, y in zip(sorted(r64), sorted(p64)) if x != y), None)
    sym = sorted(set(r64) ^ set(p64))
    lines.append(f"insertion steps of the rerun's and 12 November's 64-rank runs: first difference of the "
                 f"ordered lists at entry step {first}; smallest step in only one of the two lists: "
                 f"{sym[0] if sym else None}")
    return lines


def sec_controls(root):
    out = []
    pairs = (
        ("64 ranks: one node (A, the rerun) against eight hosts (B, joss64x8)",
         root / RERUN / "rust-np64-a1/run", root / RERUN / "joss64x8-control"),
        ("32 ranks: one node (A, the rerun) against eight hosts (B, joss32x8)",
         root / RERUN / "rust-np32-a1/run", root / MN / "main/joss32x8-a1"),
    )
    rows = []
    for label, a, b in pairs:
        lines, p = pair_lines(Pair(a, b), label)
        out += lines + [""]
        sa, sb = summary(a), summary(b)
        same = {k: sa.get(k) == sb.get(k) for k in (
            "psize", "max_steps", "nb_global_nodes_start", "nb_regular_elements_dim3", "rust_static_matrix",
            "rust_static_sparse_solver", "rust_cohesive_compat", "rust_doubled_node_load")}
        mumps = {k: (sa["rust_static_solver_stats"][k], sb["rust_static_solver_stats"][k])
                 for k in ("nb_proc", "sym", "infog7_ordering")}
        st = read_status(Path(b, "status.txt")) if Path(b, "status.txt").exists() else {}
        out.append("recorded in both summaries and equal: " + ", ".join(k for k, v in same.items() if v)
                   + ("; DIFFERENT: " + ", ".join(k for k, v in same.items() if not v) if not all(same.values())
                      else ""))
        out.append("MUMPS (A, B): " + ", ".join(f"{k} {v[0]} / {v[1]}" for k, v in mumps.items()))
        bin_a = Path(a, "binary.sha256").read_text().split()[0]
        bin_b = Path(b, "binary.sha256").read_text().split()[0]
        out.append(f"binary.sha256 equal: {bin_a == bin_b} ({bin_a[:8]}…)")
        if st:
            out.append(f"B's partition: {st.get('partition')}, sha256 {str(st.get('partition_sha256'))[:8]}…, "
                       f"matches the replayed file: {st.get('partition_matches')}; ranks per host "
                       f"{st.get('ranks_per_host')}, bind {st.get('bind')}")
        out.append("")
        fin = p.final()
        rows.append((label.split(":")[0], n(p.first_above(1e-12)), n(p.first_above(1e-10)),
                     n(p.first_count) if p.first_count else "none", g(p.largest(0, p.last_equal)[0], 2),
                     f"{n(p.ca[-1])} / {n(p.cb[-1])}", g(max(fin.values()), 2)))
    # the dissipation of the step with the control's largest one-step change of d_ed, in the four runs
    ctl = Pair(pairs[0][1], pairs[0][2])
    _, js = ctl.j_max("ed", ctl.last_equal)
    four = (("rust-np64-a1", root / RERUN / "rust-np64-a1/run"), ("rust-np32-a1", root / RERUN / "rust-np32-a1/run"),
            ("joss64x8", root / RERUN / "joss64x8-control"), ("joss32x8", root / MN / "main/joss32x8-a1"))
    inc = {}
    for nm, d in four:
        e = read_energies(Path(d, "energies.csv"))
        inc[nm] = e[js]["ed"] - e[js - 1]["ed"]
    lo_, hi_ = min(inc.values()), max(inc.values())
    out.append(f"dissipated energy of step {js} (the 64-rank control's largest J_ed) in the four runs: "
               + ", ".join(f"{nm} {v:.10e} J" for nm, v in inc.items())
               + f"; spread {g(hi_ - lo_, 2)} J")
    out.append("")
    rr = Pair(root / RERUN / "rust-np64-a1/run", root / RERUN / "rust-np32-a1/run")
    fin = rr.final()
    rows.insert(0, ("64 against 32 ranks, one node each", n(rr.first_above(1e-12)), n(rr.first_above(1e-10)),
                    n(rr.first_count), g(rr.largest(0, rr.last_equal)[0], 2), f"{n(rr.ca[-1])} / {n(rr.cb[-1])}",
                    g(max(fin.values()), 2)))
    out.append("## Summary of the three pairs")
    out.append(table(("Pair", "First step > 1e-12 of L", "First step > 1e-10 of L", "First count difference",
                      "Largest r before it", "Cohesive elements at 5,000", "Largest relative energy difference at 5,000"),
                     rows))
    return out


def percopy_run(d):
    rows = []
    lines = Path(d, "load_check.csv").read_text().splitlines()
    head = [h.strip() for h in lines[0].split(",")]
    for line in lines[1:]:
        if line.strip():
            rows.append(dict(zip(head, (x.strip() for x in line.split(",")))))
    return rows


def sec_percopy(root):
    out = ["## The per-copy check at full size (percopy-joss64x8, percopy-joss32x8)"]
    runs = {}
    for name in ("percopy-joss64x8-a1", "percopy-joss32x8-a1"):
        d = root / MN / "percopy" / name
        s, rows, st = summary(d), percopy_run(d), read_status(d / "status.txt")
        lc = s["rust_load_check"]
        P = s["psize"]
        steps = [int(r["step"]) for r in rows]
        fails = [r for r in rows if r["verdict"] != "pass"]
        mx = max(rows, key=lambda r: float(r["max_rel_diff"]))
        first_dl = next((int(r["step"]) for r in rows if int(r["doubled_loaded"]) > 0), None)
        nondecr = all(int(rows[i]["doubled_loaded"]) <= int(rows[i + 1]["doubled_loaded"])
                      for i in range(len(rows) - 1))
        apps_ok = all(int(r["applications_min"]) == int(r["applications_max"])
                      == 6 + 4 * (min(int(r["step"]), 9999) // 100) for r in rows)
        zero_cols = {c: max(int(r[c]) for r in rows) for c in (
            "moved_from_written_nodes_sum", "foreign_writes_sum", "reset_nodes_sum", "forgotten_entries_sum")}
        store = min(int(r["ranks_with_store"]) for r in rows)
        last = rows[-1]
        tot = max(float(r["total_rel_diff"]) for r in rows)
        out.append(f"### {name}: {P} ranks on {st['nb_hosts']} hosts ({st['ranks_per_host']} per host), "
                   f"outcome {st['outcome']}, exit {st['exit']}")
        out.append(f"1 provenance: rust_cohesive_compat = {s.get('rust_cohesive_compat')}, rust_doubled_node_load = "
                   f"{s.get('rust_doubled_node_load')}, rust_load_check_every = {s.get('rust_load_check_every')}, "
                   f"max_steps = {s['max_steps']}, psize = {P}; partition sha256 "
                   f"{str(st.get('partition_sha256'))[:8]}… (matches the replayed file: {st.get('partition_matches')}); "
                   f"binary {Path(d, 'binary.sha256').read_text().split()[0][:8]}…")
        out.append(f"2 completeness: summary checks {lc['checks']}, last_step {lc['last_step']}; load_check.csv "
                   f"{len(rows)} rows, steps {steps[0]} ... {steps[-1]}, contiguous: "
                   f"{steps == list(range(steps[0], steps[-1] + 1))}")
        out.append(f"3 verdict: failed_checks {lc['failed_checks']}, verdict {lc['verdict']}, rows not 'pass': "
                   f"{len(fails)}; largest max_rel_diff {g(float(mx['max_rel_diff']), 3)} at step {mx['step']} "
                   f"(summary: {g(lc['max_rel_diff'], 3)} at step {lc['max_rel_diff_step']}); bound "
                   f"{g(lc['tolerance'], 1)}")
        out.append(f"4 not vacuous: first doubled loaded node at step {first_dl} (summary "
                   f"{lc['first_doubled_loaded_step']}); at the last step doubled loaded nodes "
                   f"{n(int(last['doubled_loaded']))} (summary {n(lc['doubled_loaded_nodes'])}), moved contributions "
                   f"{n(int(last['moved_contributions_sum']))} (summary {n(lc['moved_contributions'])}); "
                   f"doubled_loaded never decreasing: {nondecr}")
        out.append("5 D-108's rule never exercised: " + ", ".join(f"{c} max {v}" for c, v in zero_cols.items())
                   + f" (summary: moved_from_written_nodes {lc['moved_from_written_nodes']}, foreign_writes "
                   f"{lc['foreign_writes']})")
        out.append(f"6 consistency: ranks_with_store min {store} (P = {P}); applications_min = applications_max = "
                   f"6 + 4 floor(step / 100) in every row: {apps_ok} ({last['applications_max']} at step "
                   f"{last['step']}); largest total_rel_diff {g(tot, 2)}")
        out.append(f"cohesive elements at step {s['max_steps']}: {n(s['nb_cohesive_elements'])}; "
                   f"chrono load_check {read_chrono(d / 'chrono.csv')[0].get('load_check', 0):.1f} s of a wall "
                   f"{wall(d):.0f} s")
        runs[P] = (rows, read_insertions(d / "insertions.csv"), s)
    # 7: between the two runs
    (r64, i64, s64), (r32, i32, s32) = runs[64], runs[32]
    last = min(len(r64), len(r32)) - 1
    c64, c32 = cumulative(i64, last), cumulative(i32, last)
    fc = next((s for s in range(last + 1) if c64[s] != c32[s]), None)
    le = (fc - 1) if fc is not None else last
    fm, tt = 0.0, 0.0
    for s in range(le + 1):
        a, b = r64[s], r32[s]
        fa, fb = float(a["fmax"]), float(b["fmax"])
        fm = max(fm, abs(fa - fb) / max(fa, fb))
        for c in ("total_f_x", "total_f_y", "total_f_z"):
            tt = max(tt, abs(float(a[c]) - float(b[c])) / max(fa, fb))
    out.append(f"7 between the two runs: cumulative insertion counts equal through step {le}"
               + (f" (first difference at step {fc})" if fc is not None else "")
               + f"; first doubled loaded node {s64['rust_load_check']['first_doubled_loaded_step']} / "
               f"{s32['rust_load_check']['first_doubled_loaded_step']}; through step {le}: fmax relative difference "
               f"at most {g(fm, 2)}, owned totals |Σf_64 - Σf_32| / fmax at most {g(tt, 2)}")
    rr = read_insertions(root / RERUN / "rust-np64-a1/run/insertions.csv")
    c = cumulative(rr, 5000)
    out.append(f"the 5,000-step rerun at 64 ranks holds {n(c[3200])} cohesive elements at step 3,200 "
               f"({c[3200] / c[5000] * 100:.1f} % of its {n(c[5000])})")
    return out


def sec_smoke(root):
    base = root / MN / "smoke"
    out = ["## The smoke session (mn-2026-11-13/smoke)"]
    names = ("s-eig-h1-a1", "s-eig-h8-a1", "s-mumps-h1-a1", "s-mumps-h8-a1", "s-p64-a1")
    bins = set()
    for nm in names:
        st, s = read_status(base / nm / "status.txt"), summary(base / nm)
        bins.add(Path(base / nm / "binary.sha256").read_text().split()[0])
        rec = {k: s.get(k) for k in SETTINGS}
        out.append(f"{nm}: outcome {st['outcome']}, {st['np']} ranks on {st['nb_hosts']} hosts "
                   f"({st['ranks_per_host']} per host), solver {st['solver']}, static {s.get('rust_static_sparse_solver')}"
                   f"{' (MUMPS nb_proc ' + str(s['rust_static_solver_stats'].get('nb_proc')) + ')' if s.get('rust_static_sparse_solver') == 'mumps' else ''}"
                   f", settings as required: {rec == SETTINGS}, cohesive {s['nb_cohesive_elements']}, wall {wall(base / nm):.0f} s")
    out.append(f"binary.sha256: {len(bins)} distinct value(s) ({sorted(bins)[0][:8]}…)")
    a, b = base / "s-eig-h1-a1", base / "s-eig-h8-a1"
    same_e = Path(a, "energies.csv").read_bytes() == Path(b, "energies.csv").read_bytes()
    same_i = Path(a, "insertions.csv").read_bytes() == Path(b, "insertions.csv").read_bytes()
    keys = ("time_step", "nb_global_nodes_end", "nb_cohesive_elements", "nb_insertion_steps", "first_insertion_step",
            "energy_dissipated", "energy_potential", "energy_kinetic", "displacement_l2_norm", "displacement_max_abs",
            "displacement_sum")
    sa, sb = summary(a), summary(b)
    same_s = all(sa[k] == sb[k] for k in keys)
    out.append(f"Eigen, 8 ranks, 1 host against 8 hosts: energies.csv byte-identical {same_e}, insertions.csv "
               f"byte-identical {same_i}, summary physics keys identical {same_s}")
    a, b = base / "s-mumps-h1-a1", base / "s-mumps-h8-a1"
    same_i = Path(a, "insertions.csv").read_bytes() == Path(b, "insertions.csv").read_bytes()
    p = Pair(a, b)
    worst = {}
    for k in E:
        worst[k] = max((abs(p.d[k][s]) / max(abs(p.ea[s][k]), abs(p.eb[s][k]))
                        for s in p.steps if max(abs(p.ea[s][k]), abs(p.eb[s][k]))), default=0.0)
    out.append(f"MUMPS, 8 ranks, 1 host against 8 hosts: insertions.csv byte-identical {same_i}; cohesive "
               f"{summary(a)['nb_cohesive_elements']} / {summary(b)['nb_cohesive_elements']}; largest relative "
               f"difference per energy over the run (own value): "
               + ", ".join(f"{NAME[k]} {g(v, 2)}" for k, v in worst.items())
               + f"; largest r (of the step's largest energy) {g(max(p.rmax), 2)}")
    st = read_status(base / "s-p64-a1" / "status.txt")
    per_host = sorted({len(v) for v in st["rank_hosts"].values()})
    out.append(f"s-p64: ranks per host {per_host} on {len(st['rank_hosts'])} hosts, mesh {summary(base / 's-p64-a1')['mesh']}")
    return out


def run_info(d):
    sec, rep = read_chrono(d / "chrono.csv")
    s = summary(d)
    st = read_status(d / "status.txt") if (d / "status.txt").exists() else {}
    return {
        "dir": d, "sec": sec, "rep": rep, "s": s, "st": st, "wall": wall(d), "P": s["psize"],
        "hosts": st.get("nb_hosts", 1), "steps": s["max_steps"], "tets": s["nb_regular_elements_dim3"],
        "init": sum(sec.get(k, 0.0) for k in INIT_PHASES), "loop": sum(sec.get(k, 0.0) for k in LOOP_PHASES),
        "peak_gib": (max(st["host_peak_kib"].values()) / 2 ** 20) if st.get("host_peak_kib") else None,
    }


def phase_cols(r):
    sec = r["sec"]
    return [f"{r['wall']:,.0f}", f"{r['init']:,.0f}", f"{sec.get('read_mesh', 0):.2f}", f"{sec.get('dist_mesh', 0):.1f}",
            f"{sec.get('init_full', 0):.2f}", f"{sec.get('static_solve', 0):.1f}", f"{sec.get('before_step', 0):.2f}",
            f"{sec.get('solve_step', 0):,.1f}", f"{sec.get('check_cohesive_stress', 0):,.1f}",
            f"{sec.get('energies', 0):.1f}", f"{r['loop']:,.0f}"]


PHASE_HEAD = ("wall s", "init s", "read_mesh", "dist_mesh", "init_full", "static solve", "before_step",
              "solve_step", "check", "energies", "loop s")


def sec_multinode(root):
    m = root / MN / "main"
    out = ["## The multi-node runs: rank 0's Chrono totals (s), one run per point"]
    R = {nm: run_info(m / f"{nm}-a1") for nm in (
        "joss64x8", "joss32x8", "h004-p8-h1", "h004-p8-h2", "h004-p8-h4", "h004-p8-h8", "h004-p16-h2",
        "h004-p32-h4", "h004-p64-h8", "h0032-p16-h2", "h0025-p32-h4", "joss-p64-h8-1500", "smoke-h012")}
    one = {64: run_info(root / RERUN / "rust-np64-a1/run"), 32: run_info(root / RERUN / "rust-np32-a1/run")}
    # (a) the full size, one node against eight hosts
    rows = []
    for P, nm in ((64, "joss64x8"), (32, "joss32x8")):
        a, b = one[P], R[nm]
        rows.append((P, "1 (r7a.16xlarge)", *phase_cols(a)))
        rows.append((P, f"8 ({b['st']['ranks_per_host']} per host)", *phase_cols(b)))
        ratio = [b["wall"] / a["wall"], b["init"] / a["init"]]
        ratio += [b["sec"].get(k, 0) / a["sec"][k] for k in (
            "read_mesh", "dist_mesh", "init_full", "static_solve", "before_step", "solve_step",
            "check_cohesive_stress", "energies")]
        ratio += [b["loop"] / a["loop"]]
        rows.append((P, "ratio 8 / 1", *(f"{x:.2f}" for x in ratio)))
    out.append("### (a) The JOSS mesh, 5,000 steps: the single-node rerun against the eight-host run "
               "(same binary, same partition)")
    out.append(table(("ranks", "hosts", *PHASE_HEAD), rows))
    rows = []
    for P, nm in ((64, "joss64x8"), (32, "joss32x8")):
        a, b = one[P], R[nm]
        for lab, r in (("1 node", a), ("8 hosts", b)):
            sec = r["sec"]
            rows.append((P, lab, f"{sec['solve_step'] / r['steps'] * 1e3:.1f}",
                         f"{sec['check_cohesive_stress'] / r['steps'] * 1e3:.1f}",
                         f"{sec['energies'] / r['steps'] * 1e3:.1f}", f"{r['loop'] / r['steps'] * 1e3:.1f}",
                         n(r["s"]["nb_cohesive_elements"])))
    t = run_info(root / RERUN / "test-np64/run")
    out.append("")
    out.append(f"static solve of the same binary on the same 64-rank partition and host: test-np64 (20 steps) "
               f"{t['sec']['static_solve']:.1f} s against rust-np64-a1 {one[64]['sec']['static_solve']:.1f} s "
               f"(ratio {t['sec']['static_solve'] / one[64]['sec']['static_solve']:.2f}); dist_mesh "
               f"{t['sec']['dist_mesh']:.1f} s against {one[64]['sec']['dist_mesh']:.1f} s")
    out.append("")
    out.append("per step (ms): ")
    out.append(table(("ranks", "where", "solve_step", "check", "energies", "loop", "cohesive elements"), rows))
    # (b) 8 ranks on 1 / 2 / 4 / 8 hosts
    out.append("")
    out.append("### (b) h = 0.04, 8 ranks on 1 / 2 / 4 / 8 hosts (one partition, replayed)")
    rows = []
    base = R["h004-p8-h1"]
    for nm in ("h004-p8-h1", "h004-p8-h2", "h004-p8-h4", "h004-p8-h8"):
        r = R[nm]
        rows.append((r["hosts"], r["st"]["ranks_per_host"], *phase_cols(r), n(r["s"]["nb_cohesive_elements"]),
                     f"{r['wall'] / base['wall']:.2f}", f"{r['loop'] / base['loop']:.2f}",
                     f"{r['sec']['solve_step'] / base['sec']['solve_step']:.2f}",
                     f"{r['sec']['check_cohesive_stress'] / base['sec']['check_cohesive_stress']:.2f}",
                     f"{r['sec']['static_solve'] / base['sec']['static_solve']:.2f}",
                     f"{r['peak_gib']:.1f}"))
    out.append(table(("hosts", "ranks per host", *PHASE_HEAD, "cohesive", "wall / 1 host", "loop / 1 host",
                      "solve_step / 1 host", "check / 1 host", "static / 1 host", "peak GiB per host"), rows))
    parts = {R[nm]["st"].get("partition_sha256") for nm in ("h004-p8-h1", "h004-p8-h2", "h004-p8-h4", "h004-p8-h8")}
    ins = {Path(R[nm]["dir"], "insertions.csv").exists() for nm in ("h004-p8-h1",)}
    out.append(f"partition sha256 of the four runs: {len(parts)} distinct value(s); insertions.csv committed: {ins}")
    # (c) strong scaling at h = 0.04
    out.append("")
    out.append("### (c) h = 0.04, 8 / 16 / 32 / 64 ranks on 1 / 2 / 4 / 8 hosts (8 per host): strong scaling")
    rows = []
    b0 = R["h004-p8-h1"]
    for nm in ("h004-p8-h1", "h004-p16-h2", "h004-p32-h4", "h004-p64-h8"):
        r = R[nm]
        f = r["P"] / b0["P"]

        def se(key):
            t0 = b0[key] if key in ("wall", "loop", "init") else b0["sec"][key]
            t1 = r[key] if key in ("wall", "loop", "init") else r["sec"][key]
            sp = t0 / t1
            return f"{sp:.2f} ({sp / f * 100:.0f} %)"

        rows.append((r["P"], r["hosts"], f"{r['tets'] // r['P']:,}",
                     f"{n(r['s']['nb_cohesive_elements'])} ({r['s']['nb_insertion_steps']}, {r['s']['first_insertion_step']})",
                     f"{r['loop']:.1f}", se("loop"), se("solve_step"), se("check_cohesive_stress"), se("static_solve"),
                     se("wall"), f"{r['sec']['dist_mesh']:.1f}", f"{r['sec']['init_full']:.2f}"))
    out.append(table(("ranks", "hosts", "tets / rank", "cohesive (insertion steps, first)", "loop s", "loop S (eff.)", "solve_step S (eff.)",
                      "check S (eff.)", "static solve S (eff.)", "wall S (eff.)", "dist_mesh s", "init_full s"), rows))
    # (d) weak scaling
    out.append("")
    out.append("### (d) weak scaling, about 70 k tetrahedra per rank, 8 ranks per host, 1,500 steps")
    rows = []
    w0 = R["h004-p8-h1"]
    for nm in ("h004-p8-h1", "h0032-p16-h2", "h0025-p32-h4", "joss-p64-h8-1500"):
        r = R[nm]
        sec = r["sec"]
        ms = r["loop"] / r["steps"] * 1e3
        cus = (sec["solve_step"] + sec["check_cohesive_stress"]) * r["P"] / r["tets"] / r["steps"] * 1e6
        rows.append((r["P"], r["hosts"], Path(r["s"]["mesh"]).name, n(r["tets"]), f"{r['tets'] / r['P']:,.0f}",
                     n(r["s"]["nb_cohesive_elements"]), r["s"]["first_insertion_step"],
                     f"{sec['solve_step'] / r['steps'] * 1e3:.1f}",
                     f"{sec['check_cohesive_stress'] / r['steps'] * 1e3:.1f}", f"{ms:.1f}",
                     f"{(w0['loop'] / w0['steps']) / (r['loop'] / r['steps']):.2f}", f"{cus:.2f}",
                     f"{sec['static_solve']:.1f}", f"{r['wall']:.0f}", f"{r['peak_gib']:.1f}"))
    out.append(table(("ranks", "hosts", "mesh", "tets", "tets / rank", "cohesive", "first insertion",
                      "solve_step ms/step", "check ms/step", "loop ms/step", "E_w (loop, 8 ranks / P)",
                      "core-µs/el/step", "static solve s", "wall s", "peak GiB per host"), rows))
    r = R["smoke-h012"]
    out.append("")
    out.append(f"smoke-h012 (8 ranks, 1 per host on 8 hosts, h = 0.12): outcome {r['st']['outcome']}, wall {r['wall']:.0f} s")
    return out


def sec_joss(root):
    """The rows of the JOSS page's full-size table for the rerun (the corrected placement)."""
    out = ["## The JOSS page's full-size table: the rerun's rows (port, corrected placement, one node)"]
    rows = []
    for P in (64, 32):
        d = root / RERUN / f"rust-np{P}-a1/run"
        r = run_info(d)
        sec, s = r["sec"], r["s"]
        st = read_status(d / "status.txt")
        peak = int(Path(d, "memory_peak").read_text().split()[0]) / 2 ** 30
        cus = (sec["solve_step"] + sec["check_cohesive_stress"]) * P / r["tets"] / r["steps"] * 1e6
        rows.append((P, "port, corrected placement", f"{r['wall']:,.0f}", f"{sec['init_full']:.2f}",
                     f"{sec['static_solve']:.0f}", f"{sec['solve_step'] / r['steps'] * 1e3:.0f}",
                     f"{sec['check_cohesive_stress'] / r['steps'] * 1e3:.0f}", f"{cus:.2f}",
                     n(s["nb_cohesive_elements"]), f"{n(s['nb_insertion_steps'])} ({s['first_insertion_step']})",
                     f"{peak:.1f}",
                     f"{s['energy_dissipated']:.2f} / {s['energy_potential']:,.1f} / {s['energy_kinetic']:,.1f}"))
        if st.get("memory_peak_bytes") is not None and int(st["memory_peak_bytes"]) != int(
                Path(d, "memory_peak").read_text().split()[0]):
            out.append(f"WARNING: status.txt and memory_peak differ for {P} ranks")
    out.append(table(("Processes", "Code", "Wall s", "init_full s", "Static solve s", "solve_step ms/step",
                      "Check ms/step", "core-µs/el/step", "Cohesive elements inserted", "Insertion steps (first)",
                      "Peak memory GiB", "Energies at the end: dissipated / potential / kinetic J"), rows))
    return out


SECTIONS = {"rerun": sec_rerun, "controls": sec_controls, "percopy": sec_percopy, "smoke": sec_smoke,
            "multinode": sec_multinode, "joss": sec_joss}


def report(root, sections=None):
    ROOT[0] = Path(root)
    out = []
    for name in (sections or list(SECTIONS)):
        out += SECTIONS[name](root)
        out.append("")
    return "\n".join(out)


def main(argv):
    args = argv[1:]
    root = Path(__file__).resolve().parents[3]
    if args[:1] == ["--root"]:
        if len(args) < 2:
            print(__doc__, file=sys.stderr)
            return 2
        root, args = Path(args[1]), args[2:]
    bad = [a for a in args if a not in SECTIONS]
    if bad:
        print(f"unknown section(s) {bad}; known: {', '.join(SECTIONS)}", file=sys.stderr)
        return 2
    sys.stdout.write(report(root, args or None))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
