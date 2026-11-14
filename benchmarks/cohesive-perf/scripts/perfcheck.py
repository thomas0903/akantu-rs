# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Validation shared by collect.py and make_fixtures.py.
Pure functions on file contents: unit-tested with
synthetic runs in scripts/tests/.

A run directory is valid only when every check passes; each check returns a
list of human-readable problems (empty = pass), so callers can record *why*
an attempt was rejected.
"""

import json
import math
import os
import re

# Akantu's SparseSolverMumps::printError() messages (sparse_solver_mumps.cc:
# 322-358 at the reference commit). The first is UD-216: the retry path
# returns a wrong solution with exit status 0. The others are MUMPS errors or
# warnings: never a nominal solve, so the run is rejected (fail closed).
MUMPS_MARKERS = (
    "MUMPS dynamic memory is insufficient",
    "The MUMPS workarray is too small",
    "Error in mumps during solve process",
    "Warning in mumps during solve process",
)

CAMPAIGN_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
ENERGY_HEADER = "step, ed, ep, ek, ew, et"
INSERTION_HEADER = "step, nb_inserted, nb_cohesive_total"
# Phases every Chrono row of the driver has (cohesive_extrinsic.cc), in
# addition to the optional ones (boundary_conditions every 100 steps,
# check_cohesive_stress with insertion, energies with output_energy).
CHRONO_ONCE = ("read_mesh", "dist_mesh", "init_full", "initial_conditons", "static_solve",
               "before_step")


def read_bytes(path):
    with open(path, "rb") as f:
        return f.read()


def read_text(path):
    with open(path) as f:
        return f.read()


def read_json(path):
    with open(path) as f:
        return json.load(f)


def check_campaign(campaign):
    if not CAMPAIGN_RE.match(campaign) or ".." in campaign:
        raise ValueError(f"invalid campaign {campaign!r} (one component "
                         "[A-Za-z0-9][A-Za-z0-9._-]{0,63}, no '..')")
    return campaign


def confined(root, path):
    """True when `path` resolves strictly inside `root` (symlinks resolved)."""
    real_root = os.path.realpath(root)
    real = os.path.realpath(path)
    return os.path.commonpath([real_root, real]) == real_root and real != real_root


def mumps_findings(*texts):
    """{marker: count} of MUMPS error/warning messages in the given texts."""
    found = {}
    for marker in MUMPS_MARKERS:
        n = sum(t.count(marker) for t in texts)
        if n:
            found[marker] = n
    return found


def parse_chrono(text):
    """(header line, row line, parsed dict) of a Chrono CSV, or Nones when the
    row is missing or malformed (field count differs from the header)."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.startswith('"psize"') and i + 1 < len(lines):
            header = [h.strip().strip('"') for h in line.split(",")]
            values = [v.strip() for v in lines[i + 1].split(",")]
            if len(values) != len(header) or len(set(header)) != len(header):
                return None, None, None
            return line.strip(), lines[i + 1].strip(), dict(zip(header, values))
    return None, None, None


def _finite(s):
    try:
        x = float(s)
    except (TypeError, ValueError):
        return None
    return x if math.isfinite(x) else None


def _count(s):
    try:
        return int(s) if str(s).strip() == str(int(s)) else None
    except (TypeError, ValueError):
        return None


def check_chrono(c, ranks, max_steps, cohesive_insertion):
    """Problems of a parsed Chrono row against the run metadata."""
    if c is None:
        return ["Chrono row missing or malformed"]
    p = []
    if _count(c.get("psize")) != ranks:
        p.append(f"Chrono psize {c.get('psize')!r} != ranks {ranks}")
    phases = [k for k in c if k != "psize" and not k.endswith(" nb_rep")]
    for k in phases:
        if _finite(c[k]) is None or _finite(c[k]) < 0:
            p.append(f"Chrono {k} = {c[k]!r} is not a finite non-negative number")
        n = _count(c.get(k + " nb_rep"))
        if n is None or n < 1:
            p.append(f"Chrono '{k} nb_rep' = {c.get(k + ' nb_rep')!r} is not a positive count")
    for k in c:
        if k.endswith(" nb_rep") and k[:-len(" nb_rep")] not in c:
            p.append(f"Chrono {k!r} without its phase")
    expected = {k: 1 for k in CHRONO_ONCE}
    expected["solve_step"] = max_steps
    if cohesive_insertion:
        expected["check_cohesive_stress"] = max_steps
    if max_steps >= 100:
        expected["boundary_conditions"] = max_steps // 100
    for k, n in expected.items():
        if k not in c:
            p.append(f"Chrono phase {k} missing")
        elif _count(c.get(k + " nb_rep")) != n:
            p.append(f"Chrono '{k} nb_rep' = {c.get(k + ' nb_rep')!r}, expected {n}")
    if not cohesive_insertion and "check_cohesive_stress" in c:
        p.append("Chrono has check_cohesive_stress without cohesive insertion")
    return p


def parse_energies(data, max_steps):
    """(rows, problems): energies.csv must have the exact header, rows for
    steps 0..max_steps in order, six finite values each."""
    p = []
    try:
        lines = data.decode().splitlines()
    except UnicodeDecodeError:
        return [], ["energies.csv is not text"]
    if not lines or lines[0].strip() != ENERGY_HEADER:
        return [], [f"energies.csv header is not {ENERGY_HEADER!r}"]
    rows = []
    for i, line in enumerate(lines[1:]):
        fields = [f.strip() for f in line.split(",")]
        vals = [_finite(f) for f in fields]
        if len(fields) != 6 or any(v is None for v in vals):
            p.append(f"energies.csv row {i + 1} malformed or non-finite: {line!r}")
            break
        if _count(fields[0]) != i:
            p.append(f"energies.csv row {i + 1} has step {fields[0]!r}, expected {i}")
            break
        rows.append(vals)
    if not p and len(rows) != max_steps + 1:
        p.append(f"energies.csv has {len(rows)} rows, expected {max_steps + 1} (steps 0..{max_steps})")
    return rows, p


def parse_insertions(data, max_steps):
    """(rows, problems): insertions.csv rows (step, inserted, cumulative) with
    strictly increasing steps in 1..max_steps, positive counts and a
    cumulative column equal to the running sum."""
    p = []
    try:
        lines = data.decode().splitlines()
    except UnicodeDecodeError:
        return [], ["insertions.csv is not text"]
    if not lines or lines[0].strip() != INSERTION_HEADER:
        return [], [f"insertions.csv header is not {INSERTION_HEADER!r}"]
    rows, total, last = [], 0, 0
    for line in lines[1:]:
        f = [_count(x.strip()) for x in line.split(",")]
        if len(f) != 3 or any(x is None for x in f):
            p.append(f"insertions.csv row malformed: {line!r}")
            break
        step, nb, cum = f
        total += nb
        if not last < step <= max_steps or nb < 1 or cum != total:
            p.append(f"insertions.csv row inconsistent: {line!r}")
            break
        last = step
        rows.append(f)
    return rows, p


def _same_float(a, b):
    return isinstance(a, (int, float)) and isinstance(b, (int, float)) and float(a) == float(b)


def check_summary(summary, ranks, max_steps, energies, insertions, rank_summaries, expect):
    """Problems of summary.json against the run, its histories and the
    per-rank summaries; `expect` holds the requested modes
    (cohesive_insertion, blocked_node, fill_nodes_to_elements)."""
    p = []
    for k, v in (("psize", ranks), ("max_steps", max_steps)):
        if summary.get(k) != v:
            p.append(f"summary.json {k} = {summary.get(k)!r}, expected {v}")
    for k, v in expect.items():
        if summary.get(k) != v:
            p.append(f"summary.json {k} = {summary.get(k)!r}, expected {v!r}")
    for k, v in summary.items():
        if isinstance(v, float) and not math.isfinite(v):
            p.append(f"summary.json {k} is not finite")
    if insertions is not None:
        # insertions.csv records checkCohesiveStress()'s return value: the
        # all-reduced size of each rank's new-element list, which includes the
        # ghost copies of cohesive elements on partition boundaries (UD-347).
        # Its running total therefore equals the global cohesive count at one
        # rank and is at least that count at several ranks; the insertion
        # steps are exact at any rank count.
        final = insertions[-1][2] if insertions else 0
        checks = (("nb_insertion_steps", len(insertions)),
                  ("first_insertion_step", insertions[0][0] if insertions else 0))
        for k, v in checks:
            if summary.get(k) != v:
                p.append(f"summary.json {k} = {summary.get(k)!r} but insertions.csv gives {v}")
        count = summary.get("nb_cohesive_elements")
        if not isinstance(count, int) or (final != count if ranks == 1 else final < count):
            p.append(f"summary.json nb_cohesive_elements = {count!r} but insertions.csv totals "
                     f"{final} ({'must be equal' if ranks == 1 else 'must be >='} at {ranks} rank(s))")
        if not expect.get("cohesive_insertion", True) and insertions:
            p.append("insertions recorded without cohesive insertion")
    if energies:
        last = energies[-1]
        for k, col in (("energy_dissipated", 1), ("energy_potential", 2), ("energy_kinetic", 3)):
            if not _same_float(summary.get(k), last[col]):
                p.append(f"summary.json {k} = {summary.get(k)!r} differs from the last "
                         f"energies.csv row ({last[col]!r})")
    if len(rank_summaries) != ranks:
        p.append(f"{len(rank_summaries)} summary_rank files for {ranks} ranks")
    else:
        rs = [rank_summaries.get(r) for r in range(ranks)]
        if any(r is None for r in rs):
            p.append("summary_rank files are not numbered 0..ranks-1")
        else:
            coh = sum(r.get("nb_cohesive_not_ghost", -1) for r in rs)
            if coh != summary.get("nb_cohesive_elements"):
                p.append(f"per-rank cohesive counts sum to {coh}, summary says "
                         f"{summary.get('nb_cohesive_elements')}")
            owned = sum(r.get("nb_owned_nodes", -1) for r in rs)
            if owned != summary.get("nb_owned_nodes_sum"):
                p.append(f"per-rank owned nodes sum to {owned}, summary says "
                         f"{summary.get('nb_owned_nodes_sum')}")
            mode = expect.get("fill_nodes_to_elements")
            for r in rs:
                fnte = r.get("fill_nodes_to_elements", {})
                if mode and fnte.get("mode") != mode:
                    p.append(f"rank {r.get('rank')} interposer mode {fnte.get('mode')!r} != {mode!r}")
                if mode == "joss" and fnte.get("event_calls", 0) != 0:
                    p.append(f"rank {r.get('rank')}: event calls executed in JOSS mode")
                if mode == "upstream" and fnte.get("event_calls_skipped", 0) != 0:
                    p.append(f"rank {r.get('rank')}: event calls skipped in upstream mode")
    return p


def load_json(data, name, problems):
    try:
        return json.loads(data)
    except (ValueError, TypeError):
        problems.append(f"{name} is not valid JSON")
        return None
