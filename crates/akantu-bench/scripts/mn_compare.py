#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: verdict of the multi-container end-to-end
# test (mn_e2e.sh): the multi-host runs against the single-container run at
# the same rank count and partition. Standard library only.
"""Compare the end-to-end test's multi-host runs with the single-container runs.

    python3 -I mn_compare.py WORK        # WORK of mn_e2e.sh: r1/, r2/, r4/, r8/ (session results)
    python3 -I mn_compare.py --pair A B  # two run directories (status.txt, summary.json, ...)
    python3 -I mn_compare.py --pair REF RUN --d108   # joss64x8 against the full-size rerun's
                                         # single-node run of the same binary and partition
    REF may be a run directory of c1prep_campaign.sh (<campaign>/rust/np<N>/rep<k>/: no
    placement record, the partition not exported by the run): A then checks the
    partition against <campaign>/partition-np<N>.csv when that file exists (else
    it says that the partition was not checked) and compares the mesh by file
    name without the `frozen-` prefix the campaign tool gives the h = 0.12 mesh.
    python3 -I mn_compare.py --smoke RESULTS IMAGES_TXT   # the AWS smoke session's verdict

What is compared and how (docs/TESTING.md):

 A  exact, every run (independent of the static solver): outcome `ok`;
    the placement (every host wrote the summary_rank<r>.json of exactly its
    ranks: P / H consecutive ranks per host, rank 0 on the head); the
    partition (partition.csv identical, and equal to the replayed file);
    summary.json psize, mesh, max_steps, nb_global_nodes_start,
    nb_regular_elements_dim3, the static solver kind and the static matrix
    (order, nnz_sum, nnz_max); every rank's nb_regular_elements_not_ghost,
    rust_static_matrix_nnz, blocked node rule and blocked nodes (fixed by
    the distribution); for MUMPS: nb_proc = P, sym, the ordering kind; the
    two cohesive settings (rust_cohesive_compat, rust_doubled_node_load) and
    binary.sha256 equal when both runs record them; every compared key
    present in the reference (no pass on keys neither run recorded).
 B  bitwise, Eigen static solve (`eigen`: centralised on rank 0, no MUMPS):
    energies.csv and insertions.csv byte-identical; summary.json time_step,
    node counts, cohesive counts, insertion steps, energies, displacement
    norms and sums identical; every rank's final nb_nodes, nb_owned_nodes,
    nb_elements_dim3_not_ghost, nb_elements_dim3_ghost and
    nb_cohesive_not_ghost (they grow with the insertions). Nothing in
    such a run depends on MUMPS, so a multi-host run must compute exactly
    what the single-container run computes.
 C  MUMPS on h = 0.12 (1,500 steps): the run's cohesive count, insertion
    steps, first insertion step and insertions.csv equal those of the
    reference, which must be a run of the same binary (binary.sha256): the
    1-host run of the same session (the earlier constants
    404 / 294 / 710 belonged to the pre-fix binary).
 J  with --d108 (MUMPS, no C): D-108's statement 1 (a)-(c) for two runs of
    one binary on one partition each with its own MUMPS solve: (a) both
    summaries record rust_cohesive_compat = upstream and
    rust_doubled_node_load = follows_elements; (b) up to the first step whose
    insertion counts differ, every energy within 1e-10 of the largest energy
    of that step (D-108 names the external work; the other energies are
    judged too, and the line names the energy that set the maximum), and no
    step in which the external work's difference grows by more than two
    orders of magnitude (counted from 1e-13 of the step's largest energy,
    above rounding); (c) no count difference through step 1,122. The
    first count difference, the first steps beyond 1e-12 and 1e-10 (and the
    energy that leaves first) and the final differences are printed.
 D  printed, not judged (MUMPS's parallel factorisation differs from run to
    run, UD-348; the fracture process amplifies it, see the analysis): the
    relative difference of the stable time step, the largest relative
    difference per energy column before the first insertion and over the
    whole run, the first differing insertions.csv entry, the final counts;
    printed next to the same figures for two single-container runs (the
    run-to-run band, `-rep2`).
 E  failure tests: a host stopped / paused mid-run ends the job with the
    outcome `host-lost` (time from the failure to the end printed), the host
    is restored and the repeated run is `ok` (and passes A, D against the
    reference); a forced out-of-memory kill gives `oom` on that host.
Verdict lines: `E2E: PASS | FAIL: <reasons>` (WORK), `PAIR: PASS | FAIL`,
`SMOKE: PASS | FAIL: <reasons>` (--smoke: every run of the smoke session ok,
the preflight clean, Eigen 1 vs 8 hosts A + B, MUMPS 1 vs 8 hosts A + C, the
64-rank job placed 8 per host with MUMPS on 64 processes, both settings
recorded by every run, every binary.sha256 = rust_bin_sha256 of images.txt).
"""

import hashlib
import json
import sys
from pathlib import Path

PHYS = ("time_step", "nb_global_nodes_end", "nb_owned_nodes_sum", "nb_cohesive_elements", "nb_insertion_steps",
        "first_insertion_step", "energy_dissipated", "energy_potential", "energy_kinetic",
        "displacement_l2_norm", "displacement_max_abs", "displacement_sum")
STATIC = ("psize", "mesh", "max_steps", "nb_global_nodes_start", "nb_regular_elements_dim3",
          "rust_static_sparse_solver", "rust_static_matrix")
# per-rank quantities fixed by the distribution (before any insertion)
RANK = ("nb_regular_elements_not_ghost", "rust_static_matrix_nnz", "blocked_node_rule", "blocked_y_nodes")
# per-rank quantities of the final state (they grow with every inserted cohesive element)
RANK_FINAL = ("nb_nodes", "nb_owned_nodes", "nb_elements_dim3_not_ghost", "nb_elements_dim3_ghost",
              "nb_cohesive_not_ghost")
COLS = ("ed", "ep", "ek", "ew", "et")
# settings of the cohesive model that must be the same in two compared runs (D-105 / D-108)
SETTINGS = ("rust_cohesive_compat", "rust_doubled_node_load")


def read_status(path):
    out = {}
    if Path(path).exists():
        for line in Path(path).read_text().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                try:
                    out[k] = json.loads(v)
                except ValueError:
                    out[k] = v
    return out


class Run:
    def __init__(self, d):
        self.d = Path(d)
        self.st = read_status(self.d / "status.txt")
        self.s = self.json("summary.json") or {}
        self.np = self.s.get("psize") or self.st.get("np") or 0

    def json(self, name):
        p = self.d / name
        return json.loads(p.read_text()) if p.exists() else None

    def text(self, name):
        p = self.d / name
        return p.read_bytes() if p.exists() else None

    def binary(self):
        t = self.text("binary.sha256")
        return t.decode().split()[0] if t and t.split() else None

    def rank(self, r):
        return self.json(f"summary_rank{r}.json") or {}

    def energies(self):
        t = self.text("energies.csv")
        if t is None:
            return []
        rows = []
        for ln in t.decode().splitlines()[1:]:
            if ln and not ln.startswith("#"):
                rows.append([float(x) for x in ln.split(",")])
        return rows

    def insertions(self):
        t = self.text("insertions.csv")
        return [] if t is None else [ln for ln in t.decode().splitlines()[1:] if ln and not ln.startswith("#")]


def is_foreign(ref):
    """A reference written by another tool (c1prep_campaign.sh): no placement record."""
    return "rank_hosts" not in ref.st


def mesh_name(path):
    """File name of a summary's mesh; the campaign tool copies the frozen h = 0.12 mesh
    as frozen-cube-h0.12.msh (the same bytes as the session's cube-h0.12.msh)."""
    return Path(path or "").name.removeprefix("frozen-")


def foreign_partition(ref, run):
    """For a c1prep_campaign.sh reference at P > 1: (failures, note). The partition the
    reference replayed is <campaign>/partition-np<P>.csv (the run directory is
    <campaign>/<case>/np<P>/rep<k>); the run's exported partition must equal it."""
    if not is_foreign(ref) or ref.np <= 1:
        return [], None
    f = ref.d.parent.parent.parent / f"partition-np{ref.np}.csv"
    if not f.exists():
        return [], f"partition of the reference not checked: no {f}"
    h = hashlib.sha256(f.read_bytes()).hexdigest()
    if h != run.st.get("partition_sha256"):
        return [f"partition.csv {run.st.get('partition_sha256')} != the reference's {f} ({h})"], None
    return [], f"partition = {f} (sha256 {h[:16]}…)"


def rel(a, b):
    m = max(abs(a), abs(b))
    return abs(a - b) / m if m else 0.0


def check_a(ref, run):
    """Exact checks A; returns a list of failures."""
    bad = []
    for name, r in (("reference", ref), ("run", run)):
        if r.st.get("outcome") != "ok":
            bad.append(f"{name} {r.d.name}: outcome {r.st.get('outcome')}")
    if bad:
        return bad
    per = run.st.get("ranks_per_host")
    want = {h: list(range(i * per, (i + 1) * per)) for i, h in enumerate(run.st.get("hosts", []))}
    if run.st.get("rank_hosts") != want:
        bad.append(f"placement {run.st.get('rank_hosts')} != {want}")
    # a reference written by another tool (a C1 campaign run directory: no
    # placement record, the replayed partition not exported) is compared
    # without the partition record; the distribution checks below still apply
    foreign = is_foreign(ref)
    if ref.np > 1 and not foreign:
        if ref.st.get("partition_sha256") is None or ref.st.get("partition_sha256") != run.st.get("partition_sha256"):
            bad.append(f"partition.csv {ref.st.get('partition_sha256')} vs {run.st.get('partition_sha256')}")
    for r in ((run,) if foreign else (ref, run)):
        if r.np > 1 and str(r.st.get("partition", "")).startswith("file:") and r.st.get("partition_matches") is not True:
            bad.append(f"{r.d.name}: exported partition differs from the replayed file")
    ba, bb = ref.binary(), run.binary()
    if ba and bb and ba != bb:
        bad.append(f"binary.sha256 {ba[:16]}… vs {bb[:16]}…: not the same driver")
    for k in SETTINGS:
        if ref.s.get(k) != run.s.get(k):
            bad.append(f"summary {k}: {ref.s.get(k)} vs {run.s.get(k)}")
    for k in STATIC:
        a, b = ref.s.get(k), run.s.get(k)
        if a is None:   # never pass on a key that neither run recorded
            bad.append(f"summary {k}: missing in the reference")
        if k == "mesh":   # the same file under another directory (a C1 campaign reference)
            a, b = mesh_name(a), mesh_name(b)
        if a != b:
            bad.append(f"summary {k}: {a} vs {b}")
    bad += foreign_partition(ref, run)[0]
    for rk in range(ref.np):
        a, b = ref.rank(rk), run.rank(rk)
        if not a or not b:
            bad.append(f"summary_rank{rk}.json missing")
            continue
        for k in RANK:
            if a.get(k) is None:
                bad.append(f"rank {rk} {k}: missing in the reference")
            elif a.get(k) != b.get(k):
                bad.append(f"rank {rk} {k}: {a.get(k)} vs {b.get(k)}")
    if ref.s.get("rust_static_sparse_solver") == "mumps":
        sa, sb = ref.s.get("rust_static_solver_stats") or {}, run.s.get("rust_static_solver_stats") or {}
        for s in (sa, sb):
            if s.get("nb_proc") != ref.np:
                bad.append(f"MUMPS nb_proc {s.get('nb_proc')} != {ref.np}")
        for k in ("sym", "infog7_ordering"):
            if sa.get(k) != sb.get(k):
                bad.append(f"MUMPS {k}: {sa.get(k)} vs {sb.get(k)}")
    return bad


def check_b(ref, run):
    bad = []
    for f in ("energies.csv", "insertions.csv"):
        if ref.text(f) is None or ref.text(f) != run.text(f):
            bad.append(f"{f} not byte-identical")
    for k in PHYS:
        if ref.s.get(k) is None:
            bad.append(f"summary {k}: missing in the reference")
        elif ref.s.get(k) != run.s.get(k):
            bad.append(f"summary {k}: {ref.s.get(k)!r} vs {run.s.get(k)!r}")
    for rk in range(ref.np):
        for k in RANK_FINAL:
            if ref.rank(rk).get(k) != run.rank(rk).get(k):
                bad.append(f"rank {rk} {k}: {ref.rank(rk).get(k)} vs {run.rank(rk).get(k)}")
    return bad


def check_c(ref, run):
    """C (MUMPS, h = 0.12): the run's counts and insertion list equal the reference's, the
    reference being a run of the same binary (its 1-host run; no constants,
    which belonged to one binary)."""
    bad = []
    ba, bb = ref.binary(), run.binary()
    if not ba or not bb or ba != bb:
        bad.append(f"C needs two runs of one binary: binary.sha256 {ba} vs {bb}")
    keys = ("nb_cohesive_elements", "nb_insertion_steps", "first_insertion_step")
    ca, cb = tuple(ref.s.get(k) for k in keys), tuple(run.s.get(k) for k in keys)
    if None in ca or ca != cb:
        bad.append(f"(cohesive, insertion steps, first) {cb} vs the reference's {ca}")
    if not ref.insertions() or ref.insertions() != run.insertions():
        bad.append("insertions.csv differs from the reference's (or is empty)")
    return bad


D108_SETTINGS = {"rust_cohesive_compat": "upstream", "rust_doubled_node_load": "follows_elements"}
D108_COUNTS_THROUGH = 1122   # D-108 1 (c): where the pre-fix 64- and 32-rank runs departed
D108_JUMP = 100.0            # D-108 1 (b): no jump of more than two orders of magnitude in one step
D108_JUMP_FLOOR = 1e-13      # ... counted from this fraction of the step's largest energy (above rounding; proposal)


def check_j(ref, run, scale=1e-10):
    """J, D-108's statement 1 (a)-(c) for two runs of one binary on one partition, each with
    its own MUMPS solve (joss64x8 against the full-size rerun's single-node 64-rank run).
    Returns (failures, report lines)."""
    bad, rep = [], []
    for name, r in (("reference", ref), ("run", run)):          # (a)
        for k, v in D108_SETTINGS.items():
            if r.s.get(k) != v:
                bad.append(f"(a) {name} summary {k} = {r.s.get(k)!r}, D-108 needs {v!r}")
    ea, eb = ref.energies(), run.energies()
    if not ea or not eb:
        return bad + ["energies.csv missing or empty"], rep
    if len(ea) != len(eb):
        bad.append(f"energies.csv rows {len(ea)} vs {len(eb)}")
    ia = {ln.split(",")[0].strip(): ln.replace(" ", "") for ln in ref.insertions()}
    ib = {ln.split(",")[0].strip(): ln.replace(" ", "") for ln in run.insertions()}
    steps = sorted(set(ia) | set(ib), key=lambda x: int(x))
    first_diff = next((int(x) for x in steps if ia.get(x) != ib.get(x)), None)
    worst_in, worst_col, first12, first10, leaves, jump, prev_ew = 0.0, None, None, None, None, None, None
    iew = COLS.index("ew")
    for a, b in zip(ea, eb):
        if a[0] != b[0]:
            return bad + [f"energies.csv steps differ ({a[0]} vs {b[0]})"], rep
        step = int(a[0])
        big = max(max(abs(x), abs(y)) for x, y in zip(a[1:], b[1:])) or 1.0
        diffs = [abs(x - y) / big for x, y in zip(a[1:], b[1:])]
        d = max(diffs)
        if first_diff is None or step < first_diff:
            if d > worst_in:
                worst_in, worst_col = d, COLS[diffs.index(d)]
            dew = diffs[iew]
            if (jump is None and prev_ew is not None
                    and dew > D108_JUMP * max(prev_ew, D108_JUMP_FLOOR)):
                jump = (step, prev_ew, dew)
            prev_ew = dew
        if first12 is None and d > 1e-12:
            first12, leaves = step, COLS[diffs.index(d)]
        if first10 is None and d > 1e-10:
            first10 = step
    window = f"steps 0-{first_diff - 1}" if first_diff is not None else f"all {min(len(ea), len(eb))} steps"
    rep.append(f"first step whose insertion counts differ: {first_diff if first_diff is not None else 'none'}; "
               f"judged window: {window}, largest energy difference {worst_in:.1e} of the step's largest energy"
               + (f" ({worst_col})" if worst_col else ""))
    rep.append(f"first step beyond 1e-12: {first12} ({leaves} first); beyond 1e-10: {first10}")
    fa, fb = ea[-1], eb[-1]
    rep.append("final (step %d): " % int(fa[0]) + ", ".join(f"{k} {rel(x, y):.1e}" for k, x, y in zip(COLS, fa[1:], fb[1:]))
               + f"; cohesive {ref.s.get('nb_cohesive_elements')} / {run.s.get('nb_cohesive_elements')}")
    if worst_in >= scale:                                          # (b)
        bad.append(f"(b) an energy ({worst_col}) differs by {worst_in:.1e} of the step's largest energy before "
                   f"the first count difference (D-108: < {scale:g})")
    if jump is not None:
        bad.append(f"(b) the external work's difference jumps at step {jump[0]}: {jump[1]:.1e} -> {jump[2]:.1e} "
                   f"of the step's largest energy (more than {D108_JUMP:g}x in one step)")
    if first_diff is not None and first_diff <= D108_COUNTS_THROUGH:   # (c)
        bad.append(f"(c) the insertion counts differ at step {first_diff}, D-108 needs agreement through step "
                   f"{D108_COUNTS_THROUGH}")
    return bad, rep


def figures_d(ref, run):
    """Printed, not judged: the MUMPS-dependent differences."""
    ea, eb = ref.energies(), run.energies()
    first = ref.s.get("first_insertion_step") or len(ea)
    out = {"dt": rel(ref.s.get("time_step") or 0.0, run.s.get("time_step") or 0.0)}
    for w, (lo, hi) in (("pre", (0, first)), ("all", (0, min(len(ea), len(eb))))):
        worst = dict.fromkeys(COLS, 0.0)
        for a, b in zip(ea[lo:hi], eb[lo:hi]):
            for k, x, y in zip(COLS, a[1:], b[1:]):
                worst[k] = max(worst[k], rel(x, y))
        out[w] = worst
    ia, ib = ref.insertions(), run.insertions()
    k = next((i for i, (x, y) in enumerate(zip(ia, ib)) if x != y), None)
    if k is None and len(ia) != len(ib):
        k = min(len(ia), len(ib))
    out["first_diff_insertion"] = None if k is None else (k, ia[k] if k < len(ia) else None,
                                                          ib[k] if k < len(ib) else None)
    out["counts"] = ((ref.s.get("nb_cohesive_elements"), run.s.get("nb_cohesive_elements")))
    out["bitwise_energies"] = ea == eb and ref.text("energies.csv") == run.text("energies.csv")
    return out


def fmt_d(f):
    s = f"Δt {f['dt']:.1e}; pre-insertion " + " ".join(f"{k} {v:.1e}" for k, v in f["pre"].items())
    s += "; whole run " + " ".join(f"{k} {v:.1e}" for k, v in f["all"].items())
    s += f"; insertions {'identical' if f['first_diff_insertion'] is None else 'first differ at entry ' + str(f['first_diff_insertion'])}"
    s += f"; cohesive {f['counts'][0]} / {f['counts'][1]}"
    if f["bitwise_energies"]:
        s += "; energies.csv byte-identical"
    return s


def compare_pair(ref, run, label, out, fails, d108=False):
    a = check_a(ref, run)
    solver = ref.st.get("solver") or ("eigen" if ref.s.get("rust_static_sparse_solver") == "eigen" else "auto")
    out.append(f"{label}: {run.d} vs {ref.d}")
    out.append(f"  A exact ({'placement, partition, distribution, matrix' + (', MUMPS nb_proc/sym/ordering' if solver == 'auto' else '')}): "
               + ("ok" if not a else "FAIL: " + "; ".join(a[:6])))
    fails += [f"{label} A: {x}" for x in a]
    note = foreign_partition(ref, run)[1]
    if note:
        out.append(f"  reference of c1prep_campaign.sh: {note}")
    if a:
        return
    if solver == "eigen":
        b = check_b(ref, run)
        out.append("  B bitwise (energies.csv, insertions.csv, summary, per-rank final counts): "
                   + ("ok" if not b else "FAIL: " + "; ".join(b[:6])))
        fails += [f"{label} B: {x}" for x in b]
    else:
        if "h0.12" in str(ref.st.get("mesh") or ref.s.get("mesh")) and not d108:
            c = check_c(ref, run)
            out.append("  C counts and insertion list = the same-binary reference's: "
                       + ("ok" if not c else "FAIL: " + "; ".join(c)))
            fails += [f"{label} C: {x}" for x in c]
        if d108:
            j, rep = check_j(ref, run)
            out.append("  J D-108 1 (a) settings, (b) each energy within 1e-10 of the step's largest and no "
                       "100x jump of the external work up to the first count difference, (c) counts through "
                       f"step {D108_COUNTS_THROUGH}: " + ("ok" if not j else "FAIL: " + "; ".join(j)))
            out += [f"     {r}" for r in rep]
            fails += [f"{label} J: {x}" for x in j]
        out.append("  D printed, not judged (MUMPS run to run): " + fmt_d(figures_d(ref, run)))
    wa, wb = ref.st.get("wall_s"), run.st.get("wall_s")
    if wa and wb:
        out.append(f"  wall {wa:.1f} s (reference) / {wb:.1f} s ({run.st.get('nb_hosts')} host(s)): "
                   f"{wb / wa:.2f} (timing on one sandbox, not a measurement of a network)")


def failure_tests(work, out, fails):
    for n, name, mode, host in ((4, "h012-stop", "stop", 2), (4, "h012-pause", "pause", 1)):
        sp = work / f"r{n}" / "state.json"
        if not sp.exists():
            continue
        state = json.loads(sp.read_text())
        if name not in [e["name"] for e in state.get("plan", [])]:
            continue   # not planned (E2E_QUICK)
        pts = [p for p in state.get("points", []) if p["name"] == name]
        hosts = state.get("hosts", [])
        want_host = hosts[host] if host < len(hosts) else None
        seq = [(p["attempt"], p["outcome"], p.get("lost_hosts"), p.get("restored_hosts")) for p in pts]
        st1 = read_status(work / f"r{n}" / "runs" / f"{name}-a1" / "status.txt")
        delay = 6.0
        end = st1.get("launcher_wall_s")
        out.append(f"E {name} ({mode} host {want_host} {delay:.0f} s into the run): attempts {seq}; "
                   f"job ended {end - delay:.0f} s after the failure" if end else f"E {name}: attempts {seq}")
        ok = (len(pts) == 2 and pts[0]["outcome"] == "host-lost" and pts[0].get("lost_hosts") == [want_host]
              and pts[0].get("restored_hosts") == [want_host] and pts[1]["outcome"] == "ok")
        if not ok:
            fails.append(f"E {name}: attempts {seq}")
    sp = work / "r8" / "state.json"
    if sp.exists():
        st = json.loads(sp.read_text())
        if "h006-oom" not in [e["name"] for e in st.get("plan", [])]:
            return
        pts = [p for p in st.get("points", []) if p["name"] == "h006-oom"]
        want = st.get("hosts", [None, None])[1]
        seq = [(p["attempt"], p["outcome"], p.get("oom_hosts")) for p in pts]
        out.append(f"E h006-oom (host {want} at the --oom-memory limit): {seq}")
        if not (len(pts) == 1 and pts[0]["outcome"] == "oom" and pts[0].get("oom_hosts") == [want]):
            fails.append(f"E h006-oom: {seq}")


def final_attempt(runs, name, notes=None):
    """The run's last attempt directory (a run is repeated once after a lost host); earlier
    attempts are appended to `notes`, never hidden."""
    runs = Path(runs)
    atts = sorted((int(d.name.rsplit("-a", 1)[1]), d) for d in runs.glob(f"{name}-a*")
                  if d.name.rsplit("-a", 1)[0] == name and d.name.rsplit("-a", 1)[1].isdigit())
    if not atts:
        return None
    if notes is not None:
        for a, d in atts[:-1]:
            notes.append(f"{d.parent.parent.name}/{d.name}: {read_status(d / 'status.txt').get('outcome')} "
                         f"(lost {read_status(d / 'status.txt').get('lost_hosts')}), repeated as attempt {atts[-1][0]}")
    return atts[-1][1]


SMOKE_SETTINGS = {"rust_cohesive_compat": "upstream", "rust_doubled_node_load": "follows_elements"}


def images_bin(images_txt):
    for line in Path(images_txt).read_text().splitlines():
        f = line.split()
        if len(f) == 2 and f[0] == "rust_bin_sha256":
            return f[1]
    return None


def smoke(results, images_txt):
    """Verdict of the AWS smoke session (mn_plan_aws_smoke.txt): every
    run ok, the preflight clean, Eigen 1 vs 8 hosts A + B bitwise, MUMPS 1 vs 8 hosts A + C
    (counts and insertion list equal), the 64-rank job across the 8 hosts placed 8 per host
    with MUMPS on 64 processes, both settings recorded in every summary, every run's
    binary.sha256 equal to rust_bin_sha256 of images.txt. Returns (lines, failures)."""
    res = Path(results)
    out, fails, notes = [], [], []
    sp = res / "state.json"
    if not sp.exists():
        return out, [f"no {sp}"]
    st = json.loads(sp.read_text())
    if st.get("status") != "finished":
        fails.append(f"session status {st.get('status')!r}, not finished")
    ck = res / "check.json"
    prob = json.loads(ck.read_text()).get("problems") if ck.exists() else ["no check.json"]
    out.append("preflight (check.json): " + ("ok" if not prob else "FAIL: " + "; ".join(prob)))
    fails += [f"preflight: {x}" for x in prob or []]
    want = images_bin(images_txt)
    if not want:
        fails.append(f"no rust_bin_sha256 in {images_txt}")
    names = [e["name"] for e in st.get("plan", [])]
    for need in ("s-eig-h1", "s-eig-h8", "s-mumps-h1", "s-mumps-h8", "s-p64"):
        if need not in names:
            fails.append(f"plan has no run {need}")
    dirs = {}
    for n in names:
        d = final_attempt(res / "runs", n, notes)
        if d is None:
            fails.append(f"{n}: no run directory")
            continue
        dirs[n] = d
        r = Run(d)
        bad = []
        if r.st.get("outcome") != "ok":
            bad.append(f"outcome {r.st.get('outcome')}")
        if r.binary() != want:
            bad.append(f"binary.sha256 {r.binary()} != images.txt {want}")
        for k, v in SMOKE_SETTINGS.items():
            if r.s.get(k) != v:
                bad.append(f"summary {k} = {r.s.get(k)!r}, expected {v!r}")
        out.append(f"{n}: " + ("ok (outcome, binary, settings)" if not bad else "FAIL: " + "; ".join(bad)))
        fails += [f"{n}: {x}" for x in bad]
    if "s-eig-h1" in dirs and "s-eig-h8" in dirs:
        compare_pair(Run(dirs["s-eig-h1"]), Run(dirs["s-eig-h8"]), "Eigen 1 vs 8 hosts", out, fails)
    if "s-mumps-h1" in dirs and "s-mumps-h8" in dirs:
        compare_pair(Run(dirs["s-mumps-h1"]), Run(dirs["s-mumps-h8"]), "MUMPS 1 vs 8 hosts", out, fails)
    if "s-p64" in dirs:
        r = Run(dirs["s-p64"])
        per = r.st.get("ranks_per_host")
        want_p = {h: list(range(i * per, (i + 1) * per)) for i, h in enumerate(r.st.get("hosts", []))} if per else None
        nb = (r.s.get("rust_static_solver_stats") or {}).get("nb_proc")
        bad = []
        if r.st.get("np") != 64 or r.st.get("nb_hosts") != 8 or r.st.get("rank_hosts") != want_p:
            bad.append(f"placement np {r.st.get('np')} on {r.st.get('nb_hosts')} hosts: {r.st.get('rank_hosts')}")
        if nb != 64:
            bad.append(f"MUMPS nb_proc {nb}, expected 64")
        out.append("s-p64 (the first 64-rank MPI job across the hosts): "
                   + ("ok (8 ranks per host, MUMPS on 64 processes)" if not bad else "FAIL: " + "; ".join(bad)))
        fails += [f"s-p64: {x}" for x in bad]
    for nt in notes:
        out.append(f"note: {nt}")
    return out, fails


def main(argv):
    if len(argv) in (4, 5) and argv[1] == "--pair" and (len(argv) == 4 or argv[4] == "--d108"):
        out, fails = [], []
        compare_pair(Run(argv[2]), Run(argv[3]), "pair", out, fails, d108=len(argv) == 5)
        print("\n".join(out))
        print("PAIR: " + ("PASS" if not fails else "FAIL"))
        return 0 if not fails else 1
    if len(argv) == 4 and argv[1] == "--smoke":
        out, fails = smoke(argv[2], argv[3])
        print("\n".join(out))
        print("SMOKE: " + ("PASS" if not fails else "FAIL: " + "; ".join(fails[:20])))
        return 0 if not fails else 1
    if len(argv) != 2:
        print(__doc__)
        return 2
    work = Path(argv[1])
    out, fails = [], []
    ref_dir = work / "r1" / "runs"
    if not ref_dir.exists():
        print(f"no reference session in {work / 'r1'}")
        return 2
    notes = []

    def final(runs, name):
        return final_attempt(runs, name, notes)

    cases = [c for c in ("h012", "h012-eig", "h006", "h006-eig") if final(ref_dir, c)]
    notes.clear()
    if not cases:
        fails.append(f"no reference run in {ref_dir}: nothing compared")
    for c in cases:
        ref = Run(final(ref_dir, c))
        rep = final(ref_dir, f"{c}-rep2")
        if rep:
            compare_pair(ref, Run(rep), f"band {c} (1 container, two runs)", out, fails)
        for n in (2, 4, 8):
            d = final(work / f"r{n}" / "runs", c) if (work / f"r{n}").exists() else None
            if d:
                compare_pair(ref, Run(d), f"{c} on {n} hosts", out, fails)
            elif (work / f"r{n}").exists():
                fails.append(f"{c} on {n} hosts: no run directory")
    for n, name in ((4, "h012-stop"), (4, "h012-pause")):
        d = work / f"r{n}" / "runs" / f"{name}-a2"
        if d.exists() and (ref_dir / "h012-a1").exists():
            compare_pair(Run(ref_dir / "h012-a1"), Run(d), f"{name} repeated after the restore", out, fails)
    failure_tests(work, out, fails)
    for nt in notes:
        out.append(f"note: {nt}")
    print("\n".join(out))
    print("E2E: " + ("PASS" if not fails else "FAIL: " + "; ".join(fails[:20]))
          + (f" ({len(notes)} run(s) repeated after a lost host: notes above)" if notes else ""))
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
