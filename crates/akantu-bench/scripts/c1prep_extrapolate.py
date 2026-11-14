#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the measured basis of the full-size (C1) estimate
# extrapolated from the mid-size rehearsal (docs/BENCHMARKS.md, "Full-size
# (C1) extrapolation from the mid-size rehearsal"). Arithmetic only: it reads
# c1prep_analyze.py outputs and prints Markdown tables (and a JSON record on
# stderr).
"""C1 extrapolation (standard library only).

    python3 -I c1prep_extrapolate.py H006 H004 H006PRE [H012]

H006 / H004 / H006PRE / H012: summary.json of c1prep_analyze.py for the
h = 0.06 full schedule (5000 steps, insertion), the h = 0.04 campaign
(static solve + 300 pre-insertion steps), the h = 0.06 pre-insertion
campaign (300 steps) and, optionally, the h = 0.12 campaign.

Model (every input printed with its source):

* **memory** of a code on P ranks = L(code, P) · tetrahedra + M(P), with
  M = MUMPS's allocation `INFOG(19)` (sum over the ranks; the same library
  and system on both sides, read from the Rust runs; the factors stay
  allocated after the static solve on both sides, as upstream keeps its
  static solver) and L = (Σ of the ranks' whole-run VmHWM − M) / tetrahedra:
  mesh, facets, model, the `before_step` allocations (≈ 5.7 KB per
  tetrahedron on both codes) and the loop's growth, measured pre-insertion
  (h = 0.04) and with the full fragmenting schedule (h = 0.06); central L =
  the larger of the two at the same P; L at P > 8 = L(8) + (P − 8) × 0.1 GB
  of per-process baseline. M at the JOSS size: power of the DOFs between
  h = 0.06 and h = 0.04 per P ≤ 8 (**extrapolated**). Beyond 8 ranks M is
  **not extrapolated by a model** (the former M(8)·(P/8)^β with β ≈ 0.3 was
  refuted by measurement): the measured allocation at
  16 / 32 / 64 ranks on the rehearsal meshes (MEASURED_RANK_SCALING below)
  gives two readings at the JOSS size, *additive* M(8) + the largest
  measured excess M(P) − M(8), and *proportional* M(8) × the ratio
  M(P) / M(8) of the largest system measured at P; the data cannot tell which holds at the JOSS
  size, so both are printed (the 64-rank need at the JOSS size is
  unmeasured). High = 1.5 × M plus 1.15 × L (margin). Largest rank = L / P
  × 1.2 + the largest MUMPS rank (`INFOG(18)` fit, or M / P × the measured
  imbalance);
* **static solve time**: MUMPS's elimination flops `RINFOG(3)` (power of
  the DOFs per P) at the JOSS size divided by the rate each code reached at
  h = 0.04 (flops / static-solve wall time: conservative, the wall time also
  holds the assembly); the direct two-point power fit of the time is shown
  as a second estimate;
* **explicit loop**: core-µs per element and step (`solve_step` +
  `check_cohesive_stress`) pre-insertion at **matched work per rank** — JOSS
  on P ranks against h = 0.04 on P / 8 ranks (558,875 / (P / 8) tetrahedra
  per rank, within 2 % of 4.39 M / P) — times the **insertion multiplier**
  measured on one mesh: h = 0.06 full schedule over h = 0.06 pre-insertion
  (300 steps) at the same P (1 and 8 ranks);
* `init_full`, `before_step`: matched work per rank (h = 0.04 on P / 8
  ranks) for P >= 8, power of the tetrahedra (h = 0.06 → 0.04) at 1 rank;
  `dist_mesh` (centralised on rank 0): linear in the tetrahedra from the
  h = 0.04 value at the largest P available.
"""

import json
import math
import sys

JOSS_TETS = 4_392_183
JOSS_NODES = 734_594
JOSS_DOFS = 3 * JOSS_NODES
JOSS_STEPS = 5000
GIB = 1024 ** 3
BASE_GB = 0.1  # per-process baseline beyond 8 ranks (binary, MPI, libraries)
# MUMPS summed allocation INFOG(19) (MB) of the Rust driver's static solve at
# 8 / 16 / 32 / 64 ranks (static solve + 2 steps, oversubscribed 8-vCPU
# sandboxes; INFOG figures do not depend on the timing). h = 0.12 and
# h = 0.06: pscale.sh (P = 8 of h = 0.06 also the
# author's 2,136 / 2,170 MB); h = 0.05 (x86 Gmsh, 279,131 tetrahedra,
# mesh f6b1e2bf…): campaigns mscale-h005-np8/16/32 (64
# ranks would not fit the sandbox under either reading); h = 0.04: P = 8
# the rehearsal's item2-h004; 16 and 32 ranks were OOM-killed by the 14 GiB
# limit of the 15.6 GiB sandbox (consistent with both readings). A further
# system, 3-D elasticity 20³ (27,783 DOFs, INFOG(17)), gave the same excess
# at 16 / 32 ranks (+1.0 / +2.4 GB). Meshes in DOF order.
MEASURED_RANK_SCALING = {
    "h0.12 (14,472 DOFs)": {8: 446, 16: 1484, 32: 2818, 64: 10744},
    "h0.06 (97,551 DOFs)": {8: 1765, 16: 3224, 32: 4395, 64: 12982},
    "h0.05 (150,702 DOFs)": {8: 3657, 16: 4731, 32: 6409, 64: None},
    "h0.04 (293,745 DOFs)": {8: 8167, 16: None, 32: None, 64: None},
}


def load(path):
    with open(path) as f:
        return json.load(f)


def med(xs):
    xs = sorted(x for x in xs if x is not None)
    if not xs:
        return None
    n = len(xs)
    return xs[n // 2] if n % 2 else 0.5 * (xs[n // 2 - 1] + xs[n // 2])


def runs(s, case, np):
    return [r for r in s["runs"] if r["case"] == case and r["np"] == np and r["status"] == 0 and r["summary"]]


def nps_of(s, case):
    return sorted({r["np"] for r in s["runs"] if r["case"] == case and r["status"] == 0})


def stat(s, np, key):
    return med(r["summary"].get("rust_static_solver_stats", {}).get(key) for r in runs(s, "rust", np))


def dofs(s):
    return med((r["summary"].get("rust_static_matrix") or {}).get("order") for r in runs(s, "rust", 1))


def chrono(s, case, np, phase):
    return med(r["chrono"].get(phase) for r in runs(s, case, np))


def m(s, case, np, key):
    return (s["medians"].get(f"{case}/np{np}") or {}).get(key)


def power(x1, y1, x2, y2):
    if not all(v and v > 0 for v in (x1, y1, x2, y2)):
        return None
    return math.log(y2 / y1) / math.log(x2 / x1)


def at(x0, y0, x, a):
    return None if a is None or y0 is None else y0 * (x / x0) ** a


def f(x, nd=3):
    if x is None:
        return "—"
    if abs(x) >= 1e5 or (abs(x) < 1e-2 and x != 0):
        return f"{x:.2e}"
    return f"{x:.{nd}g}"


def main(argv):
    s06, s04, s06p = load(argv[1]), load(argv[2]), load(argv[3])
    s12 = load(argv[4]) if len(argv) > 4 else None
    t06, t04 = s06["tets"], s04["tets"]
    d06, d04 = dofs(s06), dofs(s04)
    P4 = nps_of(s04, "rust")
    out = [f"Inputs: h = 0.06 {t06:,} tetrahedra / {d06:,} DOFs; h = 0.04 {t04:,} / {d04:,}; JOSS "
           f"{JOSS_TETS:,} / {JOSS_DOFS:,} (× {JOSS_TETS / t04:.2f} / × {JOSS_DOFS / d04:.2f} of h = 0.04).", ""]
    rec = {}

    # ---- MUMPS allocation and flops vs DOFs -------------------------------------
    out += ["**MUMPS (static solve)**, from the Rust runs (same library, same system as the C++ "
            "runs): allocation `INFOG(19)` (sum) / `INFOG(18)` (largest rank) in MB, elimination "
            "flops `RINFOG(3)`; exponent = power of the DOFs between h = 0.06 and h = 0.04.", "",
            "| P | INFOG(19) MB h0.06 / h0.04 | exp. | JOSS GB | INFOG(18) MB h0.06 / h0.04 | exp. | JOSS GB | flops h0.06 / h0.04 | exp. | JOSS flops | ordering |",
            "|---|---|---|---|---|---|---|---|---|---|---|"]
    mumps = {}
    for p in P4:
        row = {}
        for key, name in (("infog19_mb_sum_allocated", "sum"), ("infog18_mb_max_allocated", "max"),
                          ("rinfog3_flops_elimination", "flops")):
            a, b = stat(s06, p, key), stat(s04, p, key)
            e = power(d06, a, d04, b)
            row[name] = {"h006": a, "h004": b, "exp": e, "joss": at(d04, b, JOSS_DOFS, e)}
        mumps[p] = row
        out.append(f"| {p} | {f(row['sum']['h006'])} / {f(row['sum']['h004'])} | {f(row['sum']['exp'])} | "
                   f"{f((row['sum']['joss'] or 0) / 1e3)} | {f(row['max']['h006'])} / {f(row['max']['h004'])} | "
                   f"{f(row['max']['exp'])} | {f((row['max']['joss'] or 0) / 1e3)} | {f(row['flops']['h006'])} / "
                   f"{f(row['flops']['h004'])} | {f(row['flops']['exp'])} | {f(row['flops']['joss'])} | "
                   f"{f(stat(s04, p, 'infog7_ordering'))} |")
    pmax = max(P4)
    imb = stat(s04, pmax, "infog18_mb_max_allocated") * pmax / stat(s04, pmax, "infog19_mb_sum_allocated")
    out += ["", f"Largest rank / mean at P = {pmax}, h = 0.04: {f(imb)}.", ""]
    # ---- beyond 8 ranks: measured, two readings ------------------------------------
    out += ["**MUMPS allocation beyond 8 ranks (measured)**: `INFOG(19)` MB summed over the ranks; "
            "excess = M(P) − M(8) (GB), ratio = M(P) / M(8).", "",
            "| mesh | P = 8 | 16 | 32 | 64 | excess 16 / 32 / 64 GB | ratio 16 / 32 / 64 |",
            "|---|---|---|---|---|---|---|"]
    excess, ratio = {}, {}
    for mesh_name, row in MEASURED_RANK_SCALING.items():
        m8 = row[8]
        ex = {p: None if row.get(p) is None else (row[p] - m8) / 1e3 for p in (16, 32, 64)}
        ra = {p: None if row.get(p) is None else row[p] / m8 for p in (16, 32, 64)}
        for p in (16, 32, 64):
            if ex[p] is not None:
                # additive: the largest excess of any mesh; proportional: the ratio
                # of the largest system measured at P (meshes in DOF order; a small
                # system's ratio is inflated by its small M(8): h0.12 × 24 at 64)
                excess[p] = max(excess.get(p, 0.0), ex[p])
                ratio[p] = ra[p]
        out.append(f"| {mesh_name} | {m8:,} | " + " | ".join("—" if row.get(p) is None else f"{row[p]:,}"
                                                          for p in (16, 32, 64))
                   + f" | {' / '.join(f(ex[p]) for p in (16, 32, 64))} | {' / '.join(f(ra[p]) for p in (16, 32, 64))} |")
    out += ["", "Readings at the JOSS size: additive M(P) = M(8) + the largest excess, "
            + ", ".join(f"{f(excess[p])} GB (P = {p})" for p in sorted(excess))
            + "; proportional M(P) = M(8) × the ratio of the largest system measured at P, " + ", ".join(f"{f(ratio[p])} (P = {p})" for p in sorted(ratio))
            + ". At 16 and 32 ranks the excess stays the same on three meshes 10× apart in DOFs while "
            "the ratio falls with the system size (additive); at 64 ranks only the two smallest meshes "
            "fit a sandbox (excess +10.3 / +11.2 GB): the 64-rank memory at the JOSS size is unmeasured.", ""]
    rec["mumps"] = mumps
    rec["rank_scaling"] = {"excess_gb": excess, "ratio": ratio}
    rec["imbalance"] = imb

    # ---- L per tetrahedron -----------------------------------------------------------
    def L(s, case, p, tets):
        hwm = m(s, case, p, "run_rank_peak_sum_kib")
        mb = stat(s, p, "infog19_mb_sum_allocated")
        if hwm is None or mb is None:
            return None
        return (hwm * 1024 - mb * 1e6) / tets

    out += ["**Memory besides MUMPS**, L = (Σ of the ranks' whole-run VmHWM − `INFOG(19)`) / tetrahedra, "
            "bytes per tetrahedron:", "",
            "| P | Rust h0.04 pre | Rust h0.06 full | C++ h0.04 pre | C++ h0.06 full | Rust model only (before the static solve, h0.04) |",
            "|---|---|---|---|---|---|"]
    Ls = {}
    for p in sorted(set(P4) | set(nps_of(s06, "rust"))):
        r4, r6 = L(s04, "rust", p, t04), L(s06, "rust", p, t06)
        c4, c6 = L(s04, "cpp", p, t04), L(s06, "cpp", p, t06)
        model = med(r["summary"].get("rust_memory_kib", {}).get("rss_before_static", {}).get("sum")
                    for r in runs(s04, "rust", p))
        Ls[p] = {"rust": max(x for x in (r4, r6, 0) if x is not None) or None,
                 "cpp": max(x for x in (c4, c6, 0) if x is not None) or None}
        out.append(f"| {p} | {f(r4)} | {f(r6)} | {f(c4)} | {f(c6)} | {f(None if model is None else model * 1024 / t04)} |")
    out += ["", "(C++ at 8 ranks on h = 0.04 was killed by the 14 GiB container limit during the static "
            "solve: its L is taken from h = 0.06; the kill puts it above (15.03 GB − 8.17 GB) / "
            f"{t04:,} = {f((15.03e9 - 8.17e9) / t04)} B/tet.)", ""]
    rec["L"] = Ls

    # ---- JOSS memory --------------------------------------------------------------------
    out += ["**Memory at the JOSS size** (GiB): central = L · tetrahedra + M; high = 1.15 L · tets + 1.5 M "
            "(margin); beyond 8 ranks M by the additive (add.) and the proportional (prop.) reading.", "",
            "| P | M sum add. / prop. (GiB) | M largest rank (add.) | Rust central / high (add.) | "
            "C++ central / high (add.) | Rust / C++ central (prop.) | Rust / C++ largest rank (add.) |",
            "|---|---|---|---|---|---|---|"]
    mem = {}
    for p in (1, 8, 16, 32, 64):
        if p in mumps:
            msum_a = msum_p = mumps[p]["sum"]["joss"]
            mmax = mumps[p]["max"]["joss"]
        else:
            m8 = mumps[pmax]["sum"]["joss"]
            msum_a = m8 + excess[p] * 1e3
            msum_p = m8 * ratio[p]
            mmax = msum_a / p * imb
        ma, mp_, mmax_g = msum_a * 1e6 / GIB, msum_p * 1e6 / GIB, mmax * 1e6 / GIB
        row = {"M_add": ma, "M_prop": mp_, "Mmax": mmax_g}
        cells = [str(p), f"{f(ma)} / {f(mp_)}", f(mmax_g)]
        prop = []
        for code in ("rust", "cpp"):
            base = Ls.get(p, {}).get(code) or Ls.get(pmax, {}).get(code)
            if code == "cpp" and Ls.get(p, {}).get(code) is None and p == pmax:
                base = L(s06, "cpp", p, t06)
            if base is None:
                cells += ["—"]
                prop.append(None)
                continue
            extra = max(0, p - pmax) * BASE_GB * 1e9 if p > pmax else 0.0
            lt = (base * JOSS_TETS + extra) / GIB
            row[code] = (lt + ma, 1.15 * lt + 1.5 * ma, lt / p * 1.2 + mmax_g, lt + mp_, 1.15 * lt + 1.5 * mp_)
            cells.append(f"{f(row[code][0])} / {f(row[code][1])}")
            prop.append(row[code][3])
        cells += [f"{f(prop[0])} / {f(prop[1])}",
                  f"{f(row.get('rust', (None,) * 3)[2])} / {f(row.get('cpp', (None,) * 3)[2])}"]
        mem[p] = row
        out.append("| " + " | ".join(cells) + " |")
    out.append("")
    rec["memory_gib"] = mem

    # ---- static solve time --------------------------------------------------------------
    out += ["**Static solve time at the JOSS size** (s): flops-based = JOSS flops / (flops / wall time "
            "at h = 0.04, same code and P); fit = power of the DOFs of the wall time (h = 0.06 → 0.04).", "",
            "| P | C++ rate GF/s | Rust rate GF/s | C++ flops-based | Rust flops-based | C++ fit | Rust fit |",
            "|---|---|---|---|---|---|---|"]
    st = {}
    for p in P4:
        fl04, flj = mumps[p]["flops"]["h004"], mumps[p]["flops"]["joss"]
        row = {}
        cells = [str(p)]
        rates = []
        for code in ("cpp", "rust"):
            t4 = m(s04, code, p, "static_solve_s")
            rate = None if not t4 or not fl04 else fl04 / t4
            rates.append(f(None if rate is None else rate / 1e9))
            fb = None if rate is None or flj is None else flj / rate
            a, b = m(s06, code, p, "static_solve_s"), t4
            fit = at(d04, b, JOSS_DOFS, power(d06, a, d04, b))
            row[code] = (fb, fit)
        cells += rates + [f(row["cpp"][0]), f(row["rust"][0]), f(row["cpp"][1]), f(row["rust"][1])]
        st[p] = row
        out.append("| " + " | ".join(cells) + " |")
    out.append("")
    rec["static_s"] = st

    # ---- explicit loop -------------------------------------------------------------------
    mult = {}
    for code in ("cpp", "rust"):
        mult[code] = {}
        for p in nps_of(s06p, code):
            a, b = m(s06, code, p, "core_us_el_step"), m(s06p, code, p, "core_us_el_step")
            mult[code][p] = None if not a or not b else a / b
    out += ["**Insertion multiplier** (h = 0.06: core-µs/el/step of the full schedule over the "
            "300-step pre-insertion runs, same P): "
            + "; ".join(f"{c}: " + ", ".join(f"P = {p} ×{f(v)}" for p, v in mult[c].items()) for c in mult) + ".", ""]
    out += ["**Explicit loop at the JOSS size** (5,000 steps; core-µs/el/step pre-insertion at matched work "
            "per rank: h = 0.04 on P / 8 ranks; with insertion = × the multiplier at the nearest measured P; "
            "hours = core-µs · tetrahedra · steps / P):", "",
            "| JOSS P | h0.04 ranks | C++ core-µs | Rust core-µs | C++ h pre / with insertion | Rust h pre / with insertion | Rust / C++ |",
            "|---|---|---|---|---|---|---|"]
    loop = {}
    for p in (8, 16, 32, 64):
        q = p // 8
        row = {}
        cells = [str(p), str(q)]
        for code in ("cpp", "rust"):
            c = m(s04, code, q, "core_us_el_step")
            cells.append(f(c))
            if c is None:
                row[code] = None
                continue
            mp = mult[code].get(1 if p <= 16 else 8) or max((v for v in mult[code].values() if v), default=1.0)
            pre = c * 1e-6 * JOSS_TETS * JOSS_STEPS / p / 3600
            row[code] = (pre, pre * mp)
        for code in ("cpp", "rust"):
            cells.append("—" if row[code] is None else f"{f(row[code][0])} / {f(row[code][1])}")
        cells.append("—" if not row.get("cpp") or not row.get("rust") else f(row["rust"][1] / row["cpp"][1]))
        loop[p] = row
        out.append("| " + " | ".join(cells) + " |")
    out.append("")
    rec["loop_h"] = loop
    rec["insertion_multiplier"] = mult

    # ---- set-up phases ---------------------------------------------------------------------
    out += ["**Set-up phases at the JOSS size** (s): `init_full`, `before_step` at matched work per rank "
            "(h = 0.04 on P / 8 ranks) for P >= 8, power of the tetrahedra (h = 0.06 → 0.04) at 1 rank; "
            "`read_mesh`, `dist_mesh` (rank 0) linear in the tetrahedra from h = 0.04.", "",
            "| P | C++ init_full | Rust init_full | C++ before_step | Rust before_step |",
            "|---|---|---|---|---|"]
    setup = {}
    for p in (1, 8, 32, 64):
        cells = [str(p)]
        row = {}
        for phase in ("init_full", "before_step"):
            for code in ("cpp", "rust"):
                if p == 1:
                    a, b = chrono(s06, code, 1, phase), chrono(s04, code, 1, phase)
                    v = at(t04, b, JOSS_TETS, power(t06, a, t04, b))
                else:
                    v = chrono(s04, code, p // 8, phase)
                row[f"{code}_{phase}"] = v
        cells += [f(row["cpp_init_full"]), f(row["rust_init_full"]), f(row["cpp_before_step"]),
                  f(row["rust_before_step"])]
        setup[p] = row
        out.append("| " + " | ".join(cells) + " |")
    rm = {c: chrono(s04, c, 1, "read_mesh") for c in ("cpp", "rust")}
    dm = {c: chrono(s04, c, max(nps_of(s04, c)), "dist_mesh") for c in ("cpp", "rust")}
    out += ["", "read_mesh at the JOSS size (linear from h = 0.04, rank 0): C++ "
            f"{f(None if rm['cpp'] is None else rm['cpp'] * JOSS_TETS / t04)} s, Rust "
            f"{f(None if rm['rust'] is None else rm['rust'] * JOSS_TETS / t04)} s; dist_mesh (centralised): C++ "
            f"{f(None if dm['cpp'] is None else dm['cpp'] * JOSS_TETS / t04)} s, Rust "
            f"{f(None if dm['rust'] is None else dm['rust'] * JOSS_TETS / t04)} s.", ""]
    rec["setup_s"] = setup

    # ---- run matrix ------------------------------------------------------------------------
    # Assumptions (stated in the output): C++ at P = 64 without its OOM-killed h = 0.04 8-rank
    # point = C++ h = 0.04 4 ranks × the C++ 8 / 4-rank core-µs ratio of the h = 0.06 full
    # schedule; 1 rank = the h = 0.04 1-rank core-µs grown to the JOSS size with the exponent
    # between h = 0.06 (pre-insertion) and h = 0.04 at 1 rank; static solve beyond the measured
    # ranks = the 8-rank flops-based time × (8 / P)^0.5 (no measurement beyond 8 ranks: a
    # speed-up ∝ √P is assumed and flagged); set-up = read + dist + init_full + static +
    # before_step; +30 % margin (not a JOSS-mode increment).
    def core_us_joss(code, p):
        if p >= 8 and (p // 8) in nps_of(s04, code) and m(s04, code, p // 8, "core_us_el_step"):
            return m(s04, code, p // 8, "core_us_el_step")
        if p == 64 and code == "cpp":
            a, b = m(s06, "cpp", 8, "core_us_el_step"), m(s06, "cpp", 4, "core_us_el_step")
            c4 = m(s04, "cpp", 4, "core_us_el_step")
            return None if not (a and b and c4) else c4 * a / b
        if p == 1:
            a, b = m(s06p, code, 1, "core_us_el_step"), m(s04, code, 1, "core_us_el_step")
            return at(t04, b, JOSS_TETS, power(t06, a, t04, b))
        return None

    def static_joss(code, p):
        if p in st and st[p][code][0]:
            return st[p][code][0]
        ref = st.get(pmax, {}).get("rust", (None,))[0]  # 8-rank flops-based (C++ 8-rank point OOM)
        return None if ref is None else ref * (pmax / p) ** 0.5

    out += ["**Run matrix** (one node; hours per run; set-up = read_mesh + dist_mesh + init_full + "
            "static solve + before_step; loop with insertion; +30 % margin for both codes: the "
            "rehearsal's C++ already ran in JOSS mode and the port has no such mode). "
            "1-rank points: 200 steps (the set-up dominates).", "",
            "| code | P | steps | core-µs/el/step (with insertion) | set-up h | loop h | total h | +30 % margin h | memory central / high GiB (add.) | central GiB (prop.) |",
            "|---|---|---|---|---|---|---|---|---|---|"]
    matrix = []
    for code in ("cpp", "rust"):
        for p, steps in ((64, 5000), (32, 5000), (8, 5000), (1, 200)):
            c = core_us_joss(code, p)
            mp = mult[code].get(1 if p <= 16 else 8) or 1.0
            if c is None:
                continue
            cw = c * mp
            loop_h = cw * 1e-6 * JOSS_TETS * steps / p / 3600
            stp = static_joss(code, p) or 0.0
            sp = setup.get(p, {})
            init = sp.get(f"{code}_init_full") or (setup[32][f"{code}_init_full"] if p == 64 else 0.0) or 0.0
            before = sp.get(f"{code}_before_step") or (setup[32][f"{code}_before_step"] if p == 64 else 0.0) or 0.0
            rd = (rm[code] or 0.0) * JOSS_TETS / t04
            ds = (dm[code] or 0.0) * JOSS_TETS / t04 if p > 1 else 0.0
            setup_h = (rd + ds + init + stp + before) / 3600
            tot = setup_h + loop_h
            mm = mem.get(p, {}).get(code, (None, None, None, None))
            matrix.append({"code": code, "P": p, "steps": steps, "core_us": cw, "setup_h": setup_h,
                           "loop_h": loop_h, "total_h": tot, "plus30_h": 1.3 * tot,
                           "memory_central_gib": mm[0], "memory_high_gib": mm[1],
                           "memory_central_prop_gib": mm[3]})
            out.append(f"| {'C++' if code == 'cpp' else 'Rust'} | {p} | {steps} | {f(cw)} | {f(setup_h)} | "
                       f"{f(loop_h)} | {f(tot)} | {f(1.3 * tot)} | {f(mm[0])} / {f(mm[1])} | {f(mm[3])} |")
    for code in ("cpp", "rust"):
        tt = sum(r["plus30_h"] for r in matrix if r["code"] == code)
        out.append(f"| **{'C++' if code == 'cpp' else 'Rust'} total** | | | | | | | **{f(tt)}** | | |")
    out.append("")
    rec["run_matrix"] = matrix
    if s12:
        out.append(f"(h = 0.12, {s12['tets']:,} tetrahedra: MUMPS INFOG(19) at 1 rank "
                   f"{f(stat(s12, 1, 'infog19_mb_sum_allocated'))} MB, flops {f(stat(s12, 1, 'rinfog3_flops_elimination'))}.)")
    print("\n".join(out))
    json.dump(rec, sys.stderr, indent=1, default=str)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
