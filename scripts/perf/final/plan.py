#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the plan of one part of
# the final measurement campaign (scripts/perf/final/README.md).
"""What one part runs, in which order, and how long it should take.

    plan.py PART --size dry|full --reps N --cores N [--only LIST]
            [--variants PATTERNS] [--points "P..."] [--placement fill|spread]
            [--sparselu-threaded true|false] [--before DIR] [--head DIR]
            [--memory 481g] [--out DIR] [--print]

A group is the unit the runner executes, marks complete and repeats when it
is not: the variants of one configuration in one repetition, in the order
they run (rotated by the repetition). The plan lists every group of the part
in execution order (DIR/plan.jsonl, and DIR/plan.tsv for the shell runner);
the runner, `--plan-only` and summarize.py all read it, so a table row exists
for every planned point, measured or not.

--variants keeps only the variants matching one of the comma-separated
shell patterns (e.g. `cpp,head`, `before,head*`). --print writes the plan's
time estimate per series and in total (and, for part G, the expected memory
per run); the estimates and their basis are in this file (EST_*) and in
g-estimates.txt.
"""

import argparse
import fnmatch
import json
import os
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]

# ---------------------------------------------------------------- parts
EXPLICIT_FULL = ("tet4-20k:30 tet4-20k:30:fd tet4-200k:6 tet4-200k:6:fd tet10-20k:6 "
                 "tet10-20k:6:fd hex8-20k:8 hex8-20k:8:fd hex8-200k:3 hex8-200k:3:fd")
EXPLICIT_DRY = "tet4-20k:3 tet4-20k:3:fd tet10-20k:2 tet10-20k:2:fd hex8-20k:2 hex8-20k:2:fd"
A_LABELS = ["cpp-strict", "cpp-default", "before", "head", "head-t2", "head-t4",
            "head-t8", "head-fast", "head-fast-t8"]
SERIAL_ALL = ["before", "head", "head-t2", "head-t4", "head-t8", "head-fast",
              "head-fast-t8"]

# The suites' cases with a distributed form run in the MPI series only (their
# serial run in the host build would time a one-rank shortcut or skip).
MPI_CASES = {
    "suite_solid": ["par_explicit_tet4_ss", "par_cohesive_extrinsic"],
    "suite_setup": ["explicit_step_dist_tet4", "dist_mesh_tet4", "dist_facets_tet4",
                    "ghost_nodes_tet4", "ghost_elements_tet4", "reduce_scalars",
                    "partition_scotch_tet4"],
    "suite_models": ["heat_explicit_tet4", "phasefield_staggered_tri3"],
}
SERIAL_EXCLUDED = {"static_linear_mumps", "par_explicit_tet4_ss", "par_cohesive_extrinsic",
                   "explicit_step_dist_tet4", "dist_mesh_tet4", "dist_facets_tet4",
                   "ghost_nodes_tet4", "ghost_elements_tet4", "reduce_scalars",
                   "partition_scotch_tet4"}
# MPI cases at one rank only (no distributed form at P > 1)
MPI_NP1_ONLY = {"partition_scotch_tet4"}
# MPI cases whose timed phases run threaded model code: ranks x threads too
MPI_THREADED = {"par_explicit_tet4_ss", "par_cohesive_extrinsic", "explicit_step_dist_tet4",
                "heat_explicit_tet4", "phasefield_staggered_tri3"}


def threaded_case(suite, case):
    """Whether the threads and fast-mode variants can change the case's
    timed phases: the model cases of suite_solid and suite_models (element
    loops, assembly, laws, solvers run through the execution policy). The
    set-up suite (mesh reading and writing, facets, groups, partitions,
    parser, dumpers), the set-up cases of suite_solid and the geometric
    contact search run no threaded or fast-mode code: before and head only."""
    if suite == "suite_setup":
        return False
    if case.startswith("setup_") or case.startswith("contact_search_"):
        return False
    return True


FALLBACK_CASES = {   # head's --list on 2026-11-17, used when no binary is at hand
    "suite_solid": ("explicit_tet4_ss explicit_tet4_fd explicit_tet10_ss explicit_tet10_fd "
                    "explicit_hex8_ss explicit_hex8_fd explicit_tri3_ss explicit_tri3_fd "
                    "explicit_plastic explicit_marigo explicit_mazars explicit_sls "
                    "explicit_maxwell explicit_neohookean explicit_orthotropic "
                    "explicit_thermal explicit_multi_material explicit_mazars_non_local "
                    "static_linear_eigen static_linear_faer static_linear_mumps "
                    "static_newton_plastic implicit_dynamic setup_tet4 setup_tet10 setup_hex8 "
                    "cohesive_intrinsic_bilinear cohesive_intrinsic_exponential "
                    "cohesive_intrinsic_implicit cohesive_extrinsic_check "
                    "cohesive_extrinsic_insertion par_explicit_tet4_ss "
                    "par_cohesive_extrinsic").split(),
    "suite_models": ("heat_explicit_tet4 heat_explicit_quad4 heat_implicit_hex8_kvar "
                     "heat_static_tri6 phasefield_staggered_tri3 phasefield_staggered_tet4 "
                     "contact_search_quad4 contact_search_tet4 contact_explicit_quad4 "
                     "contact_implicit_static_quad4 contact_cohesive_quad4 "
                     "structural_beam3_frame_static structural_beam3_frame_dynamic "
                     "structural_dkt18_plate_static embedded_rebar_tri3 embedded_rebar_tet4 "
                     "ntn_friction_quad4").split(),
    "suite_setup": ("msh22_read_tet4 msh41_read_tet4 msh22_read_hex8 msh_write_tet4 "
                    "facets_tet4 facets_hex8 groups_tet4 erase_purify_tet4 periodic_tet4 "
                    "cohesive_insert_tet4 partition_rcb_tet4 partition_scotch_tet4 "
                    "dist_mesh_tet4 dist_facets_tet4 ghost_nodes_tet4 ghost_elements_tet4 "
                    "reduce_scalars explicit_step_dist_tet4 parser_dat parser_chain "
                    "dump_paraview_tet4 dump_paraview_ascii_tet4 dump_text_tet4 "
                    "py_twin_explicit_tri3").split(),
    "python": "import explicit_tri3 fields_tri3 dirichlet_tri3 dirichlet_group_tri3 "
              "neumann_tri3 material_tri3".split(),
}

# ---------------------------------------------------------------- estimates
# Seconds per run on an 8-core Zen 4 machine. Basis: the walls of the earlier
# series (docs/benchmarks/scaling-2026-11-13 ws/ins summaries, the mid-size
# rehearsal midsize-2026-11-12/item2-*, the baseline of the campaign
# perf-campaign/baseline), the port's runs scaled by the head / before ratio
# of the series (HEAD_FACTOR, from the campaign's tables and the dry run),
# plus RUN_OVERHEAD seconds per cohesive run (containers, copies, analysis;
# about 3 s in the dry run).
RUN_OVERHEAD = 4.0
EST_COH = {  # (series, h, np): (C++, port before)
    ("ws", "0.12", 1): (17.3, 10.6), ("ws", "0.0925", 1): (43.3, 21.3),
    ("ws", "0.0925", 2): (24.1, 13.3), ("ws", "0.0756", 2): (60.6, 29.6),
    ("ws", "0.0756", 4): (34.9, 18.7), ("ws", "0.06", 1): (225.4, 104.7),
    ("ws", "0.06", 2): (123.4, 56.9), ("ws", "0.06", 4): (67.8, 36.0),
    ("ws", "0.06", 8): (58.6, 33.2), ("ws", "0.0476", 8): (131.6, 64.8),
    ("ins", "0.12", 1): (52.1, 31.8), ("ins", "0.12", 2): (37.5, 21.5),
    ("ins", "0.12", 4): (25.9, 15.2), ("ins", "0.12", 8): (19.3, 14.8),
    # full schedule on h = 0.12: C++ with the JOSS emulation (check 0.033 s per
    # step at 1 rank), the port from the insertion-step tables
    ("full12", "0.12", 1): (230, 147), ("full12", "0.12", 2): (160, 85),
    ("full12", "0.12", 4): (110, 47), ("full12", "0.12", 8): (80, 28),
    ("reh6", "0.06", 1): (1605, 1089), ("reh6", "0.06", 2): (1041, 748),
    ("reh6", "0.06", 4): (638, 430), ("reh6", "0.06", 8): (393, 289),
    ("reh4", "0.04", 1): (456, 234), ("reh4", "0.04", 2): (267, 169),
    ("reh4", "0.04", 4): (178, 90), ("reh4", "0.04", 8): (150, 68),
    ("mem4", "0.04", 1): (456, 234), ("mem4", "0.04", 2): (267, 169),
    ("mem4", "0.04", 4): (178, 90), ("mem4", "0.04", 8): (150, 68),
    ("eig", "0.12", 1): (0, 40), ("eig", "0.12", 2): (0, 30),
    ("eig", "0.12", 4): (0, 24), ("eig", "0.12", 8): (0, 23),
    # 10 steps: the static solve (SparseLU: C++ 2.9 / 249 s, before 8 / 822 s)
    ("eig1", "0.12", 1): (4, 9), ("eig1", "0.06", 1): (260, 840),
    ("mumps", "0.12", 1): (3, 3), ("mumps", "0.12", 2): (3, 3), ("mumps", "0.12", 4): (3, 3),
    ("mumps", "0.06", 1): (30, 22), ("mumps", "0.06", 2): (18, 14), ("mumps", "0.06", 4): (10, 8),
    # one step: read, distribute, init_full, static solve, before_step
    ("setup", "0.06", 1): (30, 20), ("setup", "0.06", 2): (18, 13),
    ("setup", "0.06", 4): (10, 7), ("setup", "0.06", 8): (8, 6),
    ("setup", "0.04", 1): (145, 90), ("setup", "0.04", 2): (85, 75),
    ("setup", "0.04", 4): (50, 35), ("setup", "0.04", 8): (35, 25),
}
HEAD_FACTOR = {"ws": 0.75, "ins": 0.5, "full12": 0.5, "reh6": 0.62, "reh4": 0.85,
               "mem4": 0.85, "eig": 0.45, "eig1": 0.26, "mumps": 0.85, "setup": 0.85}
EST_DRY_COH = 8.0          # a dry cohesive run (h = 0.12, few steps)
EST_A_ROUND = {"full": 122.0, "dry": 18.0}   # one round, nine programs (measured)
EST_A_DIGESTS = {"full": 72.0, "dry": 18.0}
EST_SUITE_MPI = {"full": 8.0, "dry": 2.0}    # one container run
EST_DIST_L = 25.0
EST_PYTHON = 3.0
EST_MPI_A = 9.0            # three launches of one case
EST_MPI_B = 10.0           # two jobs of one case


def suite_baseline_seconds():
    """Seconds per case of the suites' baseline at size m (the sum of the
    phases, loop phases with their three warm-up calls), from the committed
    raw data of perf-campaign/suite-*/baseline; the port before the
    campaign."""
    res = {}
    base = REPO / "docs" / "benchmarks" / "perf-campaign"
    for suite, d in (("suite_solid", "suite-solid/baseline/base"),
                     ("suite_models", "suite-models/baseline/base"),
                     ("suite_setup", "suite-setup/baseline/base-m/base")):
        per = {}
        for f in (base / d).glob("*.r*.jsonl"):
            case = f.name.split(".r")[0]
            tot = 0.0
            for ln in f.read_text().splitlines():
                if ln.startswith("{"):
                    o = json.loads(ln)
                    calls = o.get("calls", 1)
                    tot += o["seconds"] * (1 + (3 / calls if calls > 1 else 0))
            per.setdefault(case, []).append(tot)
        for case, v in per.items():
            v.sort()
            res[(suite, case)] = v[len(v) // 2]
    return res


def est_suite_run(base_s, variant, size):
    if size == "dry":
        return 1.0
    f = 1.0 if variant == "before" else (0.7 if variant == "head" else 0.6)
    return 0.5 + (base_s if base_s is not None else 5.0) * f


def est_coh(series, h, np_, variant, size):
    if size == "dry":
        return EST_DRY_COH
    cpp, before = EST_COH.get((series, h, np_), (None, None))
    if cpp is None:
        return None
    base = variant.split("-")[0]
    if base.startswith("cpp"):
        return cpp + RUN_OVERHEAD
    hf = HEAD_FACTOR.get(series, 0.75)
    w = before if base == "before" else before * hf
    if "-t" in variant:   # ranks x threads: about the single-thread run at P x T ranks
        t = int(variant.rsplit("-t", 1)[1])
        alt = EST_COH.get((series, h, np_ * t))
        if alt:
            w = alt[1] * hf
    return w + RUN_OVERHEAD


# ---------------------------------------------------------------- helpers
class Plan:
    def __init__(self, args):
        self.a = args
        self.groups = []
        self.patterns = [p for p in (args.variants or "").split(",") if p]

    def keep(self, variants):
        if not self.patterns:
            return list(variants)
        return [v for v in variants if any(fnmatch.fnmatchcase(v, p) for p in self.patterns)]

    def want(self, series):
        return not self.a.only or series in self.a.only.split(",")

    def add(self, kind, gid, series, rep, variants, est, **fields):
        variants = self.keep(variants)
        if not variants:
            return
        self.groups.append({"id": gid, "kind": kind, "series": series, "rep": rep,
                            "variants": variants, "est_s": est(variants), **fields})


def cores_needed(variant, np_=1):
    t = int(variant.rsplit("-t", 1)[1]) if "-t" in variant else 1
    return np_ * t


def rotate(items, k):
    items = list(items)
    if not items:
        return items
    k %= len(items)
    return items[k:] + items[:k]


def case_list(binary, suite):
    if binary and os.access(binary, os.X_OK):
        try:
            out = subprocess.run([binary, "--list"], capture_output=True, text=True,
                                 timeout=60).stdout
            cases = [ln.split()[0] for ln in out.splitlines() if ln.strip()]
            if cases:
                return cases
        except (OSError, subprocess.SubprocessError):
            pass
    return FALLBACK_CASES[suite]


def python_list(vdir):
    py, script = Path(vdir or "") / "venv/bin/python", Path(vdir or "") / "suite_python.py"
    if vdir and py.exists() and script.exists():
        try:
            out = subprocess.run([str(py), "-I", str(script), "--list"], capture_output=True,
                                 text=True, timeout=120).stdout
            cases = [ln.split()[0] for ln in out.splitlines() if ln.strip()]
            if cases:
                return cases
        except (OSError, subprocess.SubprocessError):
            pass
    return FALLBACK_CASES["python"]


# ---------------------------------------------------------------- the parts
def plan_a(p):
    a = p.a
    cfgs = EXPLICIT_FULL if a.size == "full" else EXPLICIT_DRY
    labels = [v for v in A_LABELS if cores_needed(v) <= a.cores]
    if p.want("digests"):
        p.add("a_digests", "A-digests", "digests", 0, labels,
              lambda vs: EST_A_DIGESTS[a.size] * len(vs) / 9, configs=cfgs)
    if p.want("rounds"):
        for r in range(1, a.reps + 1):
            p.add("a_round", f"A-round-r{r}", "rounds", r, labels,
                  lambda vs: EST_A_ROUND[a.size] * len(vs) / 9, configs=cfgs)


def suite_groups(p, suite, cases, size, rep, before_cases, base_s):
    for c in cases:
        vs = SERIAL_ALL if threaded_case(suite, c) else ["before", "head"]
        vs = [v for v in vs if cores_needed(v) <= p.a.cores
              and (v != "before" or c in before_cases)]
        p.add("suite", f"{suite.replace('_', '-')}-{c}-np1-r{rep}", suite.split("_")[1], rep,
              rotate(vs, rep - 1),
              lambda vs, c=c: sum(est_suite_run(base_s.get((suite, c)), v, size) for v in vs),
              dir=suite.replace("_", "-"), bin=suite, case=c, size=size, np=1)


def mpi_suite_groups(p, series, outdir, binary, cases, size, nps, rep, est_one, hybrids=True):
    for c in cases:
        for np_ in nps:
            if np_ > p.a.cores or (c in MPI_NP1_ONLY and np_ > 1):
                continue
            vs = ["before", "head"]
            if hybrids and c in MPI_THREADED:
                if np_ == 2:
                    vs.append("head-t4")
                if np_ == 4:
                    vs.append("head-t2")
            vs = [v for v in vs if cores_needed(v, np_) <= p.a.cores]
            p.add("suite_mpi", f"{outdir}-{c}-np{np_}-r{rep}", series, rep, rotate(vs, rep - 1),
                  lambda vs: est_one * len(vs), dir=outdir, bin=binary, case=c, size=size, np=np_)


def plan_b(p):
    a = p.a
    size = "m" if a.size == "full" else "s"
    base_s = suite_baseline_seconds()
    for suite in ("suite_solid", "suite_models", "suite_setup"):
        series = suite.split("_")[1]
        if not p.want(series):
            continue
        head_cases = [c for c in case_list(os.path.join(a.head or "", suite), suite)
                      if c not in SERIAL_EXCLUDED]
        before_cases = set(case_list(os.path.join(a.before or "", suite), suite))
        for r in range(1, a.reps + 1):
            suite_groups(p, suite, rotate(head_cases, r - 1), size, r, before_cases, base_s)
    if p.want("mpi"):
        for r in range(1, a.reps + 1):
            for suite, cases in MPI_CASES.items():
                mpi_suite_groups(p, "mpi", "mpi-" + suite.split("_")[1], suite, cases, size,
                                 [1, 2, 4, 8], r, EST_SUITE_MPI[a.size])
    if p.want("python"):
        head_cases = python_list(a.head)
        before_cases = set(python_list(a.before))
        for r in range(1, a.reps + 1):
            for c in head_cases:
                vs = ["before", "head"] if c in before_cases else ["head"]
                p.add("python", f"python-{c}-r{r}", "python", r, rotate(vs, r - 1),
                      lambda vs: EST_PYTHON * len(vs), case=c, size=size)


def coh_series(p, series, steps, points, variants, strong=None, x2=None, x4=None, reps=None,
               placement="fill"):
    if not p.want(series):
        return
    a = p.a
    for r in range(1, (reps or a.reps) + 1):
        pts = points.split()
        if r % 2 == 0:
            pts = pts[::-1]   # point order reversed every other repetition
        for pt in pts:
            h, np_ = pt.split(":")
            np_ = int(np_)
            vs = list(variants)
            if h == strong and np_ == 2 and x2:
                vs.append(x2)
            if h == strong and np_ == 4 and x4:
                vs.append(x4)
            vs = [v for v in vs if cores_needed(v, np_) <= a.cores]
            vs = rotate(p.keep(vs), r - 1)
            if not vs:
                continue
            p.add("coh", f"{series}-h{h}-np{np_}-r{r}-{'+'.join(vs)}", series, r, vs,
                  lambda vs, h=h, np_=np_: sum((est_coh(series, h, np_, v, a.size) or 0)
                                               for v in vs),
                  h=h, steps=steps, np=np_, placement=placement)


def plan_c(p):
    if p.a.size == "full":
        coh_series(p, "ws", 500, "0.12:1 0.0925:1 0.0925:2 0.0756:2 0.0756:4 0.06:1 0.06:2 "
                   "0.06:4 0.06:8 0.0476:8", ["cpp", "before", "head"], "0.06", "head-t4", "head-t2")
        for s, n in (("ins", 1500), ("full12", 5000)):
            coh_series(p, s, n, "0.12:1 0.12:2 0.12:4 0.12:8", ["cpp", "before", "head"],
                       "0.12", "head-t4", "head-t2")
        # the full schedule on h = 0.06: C++ and the head only (before -> head
        # is measured by ws, ins, full12 and reh4; budget)
        coh_series(p, "reh6", 5000, "0.06:1 0.06:2 0.06:4 0.06:8", ["cpp", "head"],
                   "0.06", "head-t4", "head-t2")
        coh_series(p, "reh4", 300, "0.04:1 0.04:2 0.04:4 0.04:8", ["cpp", "before", "head"],
                   "0.04", "head-t4", "head-t2")
        coh_series(p, "eig", 1500, "0.12:1 0.12:2 0.12:4 0.12:8", ["before-eig", "head-eig"],
                   "0.12", "head-eig-t4", "head-eig-t2", reps=1)
    else:
        pts = "0.12:1 0.12:2 0.12:4 0.12:8"
        coh_series(p, "ws", 100, pts, ["cpp", "before", "head"], "0.12", "head-t4", "head-t2")
        coh_series(p, "ins", 800, pts, ["cpp", "before", "head"], "0.12", "head-t4", "head-t2")
        coh_series(p, "full12", 1000, "0.12:1 0.12:8", ["cpp", "before", "head"])
        coh_series(p, "reh6", 200, pts, ["cpp", "head"], "0.12", "head-t4", "head-t2")
        coh_series(p, "reh4", 50, pts, ["cpp", "before", "head"], "0.12", "head-t4", "head-t2")
        coh_series(p, "eig", 800, pts, ["before-eig", "head-eig"], "0.12", "head-eig-t4",
                   "head-eig-t2", reps=1)


def plan_d(p):
    a = p.a
    hs = ["0.12", "0.06"] if a.size == "full" else ["0.12"]
    size = "m" if a.size == "full" else "s"
    threaded = a.sparselu_threaded == "true"
    eigv = ["cppeig", "before-eig", "head-eig"] + (
        ["head-eig-t2", "head-eig-t4", "head-eig-t8"] if threaded else [])
    coh_series(p, "eig1", 10, " ".join(f"{h}:1" for h in hs), eigv)
    coh_series(p, "mumps", 10, " ".join(f"{h}:{n}" for h in hs for n in (1, 2, 4)),
               ["cpp", "before", "head"])
    if p.want("suite"):
        base_s = suite_baseline_seconds()
        labels = ["before", "head"] + (["head-t2", "head-t4", "head-t8"] if threaded else [])
        for r in range(1, a.reps + 1):
            for c in ("static_linear_eigen", "static_linear_faer"):
                vs = [v for v in labels if cores_needed(v) <= a.cores]
                p.add("suite", f"suite-solid-{c}-np1-r{r}", "suite", r, rotate(vs, r - 1),
                      lambda vs, c=c: sum(est_suite_run(base_s.get(("suite_solid", c)), v,
                                                        size) for v in vs),
                      dir="suite-solid", bin="suite_solid", case=c, size=size, np=1)
            mpi_suite_groups(p, "suite", "mpi-solid", "suite_solid_mumps",
                             ["static_linear_mumps"], size, [1, 2, 4], r,
                             EST_SUITE_MPI[a.size] * 3, hybrids=False)


def plan_e(p):
    if p.a.size == "full":
        coh_series(p, "mem4", 300, "0.04:1 0.04:2 0.04:4 0.04:8",
                   ["cpp", "before", "head", "head-release"])
    else:
        coh_series(p, "mem4", 50, "0.12:1 0.12:2 0.12:4 0.12:8",
                   ["cpp", "before", "head", "head-release"])


MPI_A_CASES = {"full": ["2 pingpong 8 20000", "2 pingpong 65536 5000", "2 pingpong 1048576 500",
                        "2 pingpong 16777216 30", "2 halo 8192 5000", "4 halo 8192 5000"],
               "dry": ["2 pingpong 8 2000", "2 pingpong 1048576 50", "4 halo 8192 500"]}
MPI_B_CASES = {"full": ["2 pingpong 8 2000", "2 pingpong 65536 500", "2 pingpong 1048576 100",
                        "2 pingpong 16777216 10", "2 halo 8192 1000", "4 halo 8192 1000"],
               "dry": ["2 pingpong 8 500", "2 pingpong 1048576 20", "4 halo 8192 200"]}


def plan_f(p):
    a = p.a
    if a.size == "full":
        coh_series(p, "setup", 1, "0.06:1 0.06:2 0.06:4 0.06:8 0.04:1 0.04:2 0.04:4 0.04:8",
                   ["cpp", "before", "head"])
    else:
        coh_series(p, "setup", 1, "0.12:1 0.12:2 0.12:4 0.12:8", ["cpp", "before", "head"])
    if p.want("dist"):
        for r in range(1, a.reps + 1):
            mpi_suite_groups(p, "dist", "dist-l", "suite_setup", ["dist_mesh_tet4"],
                             "l" if a.size == "full" else "s", [2, 4, 8], r,
                             EST_DIST_L if a.size == "full" else 2.0, hybrids=False)
    if p.want("mpilayer"):
        for r in range(1, a.reps + 1):
            for btl in ("sm", "tcp"):
                for cs in MPI_A_CASES[a.size]:
                    np_, kind, by, it = cs.split()
                    p.add("mpi_a", f"mpi-A-r{r}-{btl}-np{np_}-{kind}-{by}", "mpilayer", r,
                          rotate(["raw", "before", "head"], r - 1), lambda vs: EST_MPI_A,
                          btl=btl, np=int(np_), mkind=kind, bytes=int(by), iters=int(it))
        for r in range(1, a.reps + 1):
            for btl in ("sm", "tcp"):
                for cs in MPI_B_CASES[a.size]:
                    np_, kind, by, it = cs.split()
                    p.add("mpi_b", f"mpi-B-r{r}-{btl}-np{np_}-{kind}-{by}", "mpilayer", r,
                          rotate(["before", "head"], r - 1), lambda vs: EST_MPI_B,
                          btl=btl, np=int(np_), mkind=kind, bytes=int(by), iters=int(it))


def g_estimates():
    est = {}
    for ln in (HERE / "g-estimates.txt").read_text().splitlines():
        f = ln.split("#", 1)[0].split()
        if len(f) >= 4:
            est[(f[0], f[1])] = (float(f[2]), float(f[3]))
    return est


def plan_g(p):
    a = p.a
    mesh, steps = ("joss", 5000) if a.size == "full" else ("0.06", 200)
    points = a.points or ("128:head 128:cpp 64:head 64:cpp 32:head 32:cpp 64x2:head 32x4:head "
                          "16x8:head 8:head" if a.size == "full" else "8:head 8:cpp 4x2:head")
    est = g_estimates()
    if not p.want("full"):
        return
    seen = []
    for pt in points.split():
        ranks = pt.split(":")[0]
        np_ = int(ranks.split("x")[0])
        t = int(ranks.split("x")[1]) if "x" in ranks else 1
        if np_ > 1 and np_ * t <= a.cores and np_ not in seen:
            seen.append(np_)
            p.add("partition", f"partition-h{mesh}-np{np_}", "full", 0, ["cpp"],
                  lambda vs: est.get((mesh, "export"), (0, 0))[0], h=mesh, np=np_)
    for r in range(1, a.reps + 1):
        for pt in points.split():
            ranks, vlist = pt.split(":")
            np_ = int(ranks.split("x")[0])
            t = int(ranks.split("x")[1]) if "x" in ranks else 1
            vs = [v if t == 1 else f"{v}-t{t}" for v in vlist.split(",")]
            vs = [v for v in p.keep(vs) if np_ * t <= a.cores]
            if not vs:
                continue

            def e(vs, ranks=ranks):
                return sum(est.get((mesh, f"{ranks}:{v.split('-t')[0]}"), (0, 0))[0] for v in vs)
            mem = max((est.get((mesh, f"{ranks}:{v.split('-t')[0]}"), (0, 0))[1] for v in vs),
                      default=0)
            p.add("coh", f"full-h{mesh}-np{np_}-r{r}-{'+'.join(vs)}", "full", r, vs, e,
                  h=mesh, steps=steps, np=np_, placement=a.placement, mem_gib=mem,
                  ranks_label=ranks)


PARTS = {"A": plan_a, "B": plan_b, "C": plan_c, "D": plan_d, "E": plan_e, "F": plan_f,
         "G": plan_g}


def hours(s):
    return f"{s / 3600:.2f} h" if s >= 3600 else f"{s / 60:.0f} min"


def print_plan(p, out=sys.stdout):
    a = p.a
    per = {}
    for g in p.groups:
        s = per.setdefault(g["series"], [0, 0, 0.0])
        s[0] += 1
        s[1] += len(g["variants"])
        s[2] += g["est_s"] or 0
    total = sum(v[2] for v in per.values())
    print(f"Part {a.part} plan: size {a.size}, {a.reps} repetition(s), {a.cores} cores"
          + (f", variants {a.variants}" if a.variants else "")
          + (f", series {a.only}" if a.only else ""), file=out)
    print(f"{'series':<12} {'groups':>7} {'runs':>6} {'expected':>10}", file=out)
    for s, (ng, nr, sec) in per.items():
        print(f"{s:<12} {ng:>7} {nr:>6} {hours(sec):>10}", file=out)
    print(f"{'total':<12} {sum(v[0] for v in per.values()):>7} "
          f"{sum(v[1] for v in per.values()):>6} {hours(total):>10}", file=out)
    if a.part == "G":
        print_g(p, out)
    print("Estimates and their basis: scripts/perf/final/plan.py (EST_*), g-estimates.txt.",
          file=out)


def print_g(p, out):
    a = p.a
    lim = float(str(a.memory).rstrip("gG")) if a.memory else None
    print("", file=out)
    print("Runs one at a time, so the machine time is their sum in any order; the order "
          "removes waste:", file=out)
    print("the partitions of every rank count first (each run replays its file); the port at "
          "the largest", file=out)
    print("rank count first (the shortest full-size run, it measures that rank count's memory "
          "before the", file=out)
    print("C++ run); the longest and least needed run (8 ranks) last, so that a short budget "
          "drops it.", file=out)
    print(f"Placement {a.placement}: rank r with T threads on the r-th block of T cores of the "
          "host's cores", file=out)
    print("ordered NUMA node by NUMA node" + (" (each NUMA node holds the same number of ranks)"
                                             if a.placement == "spread" else
                                             " (one node filled before the next)") + ".",
          file=out)
    print(f"{'step':<24} {'cores':<18} {'expected peak GiB':>17} {'expected wall':>14} "
          f"{'cumulative':>11}", file=out)
    cum = 0.0
    for g in p.groups:
        cum += g["est_s"] or 0
        if g["kind"] == "partition":
            print(f"{'export np' + str(g['np']):<24} {'C++, partition only':<18} {'':>17} "
                  f"{hours(g['est_s'] or 0):>14} {cum / 3600:>10.2f}h", file=out)
            continue
        np_ = g["np"]
        t = int(g["variants"][0].rsplit("-t", 1)[1]) if "-t" in g["variants"][0] else 1
        mem = g.get("mem_gib") or 0
        warn = "  WARNING: above 90 % of the limit" if lim and mem > 0.9 * lim else ""
        name = f"{g['ranks_label']}:{','.join(v.split('-t')[0] for v in g['variants'])}"
        print(f"{name:<24} {f'{np_ * t} ({np_} x {t})':<18} {mem or '?':>17} "
              f"{hours(g['est_s'] or 0):>14} {cum / 3600:>10.2f}h{warn}", file=out)


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("part", choices=sorted(PARTS))
    ap.add_argument("--size", choices=["dry", "full"], default="full")
    ap.add_argument("--reps", type=int, default=None)
    ap.add_argument("--cores", type=int, required=True)
    ap.add_argument("--only", default="")
    ap.add_argument("--variants", default="")
    ap.add_argument("--points", default="")
    ap.add_argument("--placement", choices=["fill", "spread"], default="fill")
    ap.add_argument("--sparselu-threaded", default="false")
    ap.add_argument("--before", default="")
    ap.add_argument("--head", default="")
    ap.add_argument("--memory", default="")
    ap.add_argument("--out", default="")
    ap.add_argument("--print", action="store_true")
    a = ap.parse_args(argv[1:])
    if a.reps is None:
        a.reps = 1 if a.part == "G" else 5
    p = Plan(a)
    PARTS[a.part](p)
    if a.out:
        d = Path(a.out)
        d.mkdir(parents=True, exist_ok=True)
        with open(d / "plan.jsonl", "w") as f:
            for g in p.groups:
                f.write(json.dumps(g) + "\n")
        cols = {"a_digests": ["configs"], "a_round": ["configs"],
                "suite": ["dir", "bin", "case", "size", "np"],
                "suite_mpi": ["dir", "bin", "case", "size", "np"],
                "python": ["case", "size"], "partition": ["h", "np"],
                "coh": ["series", "h", "steps", "np", "placement"],
                "mpi_a": ["btl", "np", "mkind", "bytes", "iters"],
                "mpi_b": ["btl", "np", "mkind", "bytes", "iters"]}
        with open(d / "plan.tsv", "w") as f:
            for g in p.groups:
                vals = [str(g[c]) for c in cols[g["kind"]]]
                f.write("\t".join([g["kind"], g["id"], str(g["rep"]), " ".join(g["variants"])]
                                  + vals) + "\n")
    if a.print:
        print_plan(p)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
