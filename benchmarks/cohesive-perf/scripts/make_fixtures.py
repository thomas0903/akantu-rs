#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Turn the runs of scripts/gen-fixtures.sh into fixtures/<out>/.

    python3 -I scripts/make_fixtures.py --steps N --reps R [--out NAME] [--replace]
                                        [--runs-dir DIR]
    python3 -I scripts/make_fixtures.py --qualify-committed   # fixtures/perf_cohesive
    python3 -I scripts/make_fixtures.py --check [--out NAME]
    python3 -I scripts/make_fixtures.py --sums-only
    python3 -I scripts/make_fixtures.py --out NAME --compare perf_cohesive   # diagnostic

Reads /perf/runs/fixtures/ from the scratch volume (or DIR, a local copy with
the same layout `fixtures/<config>/<case>/rep<k>/`) and, in this order:

1. validates **every attempt** (attempts.json keeps each exit status and
   decision): exit status 0; stdout/stderr present and free of Akantu's
   MUMPS error/warning messages (UD-216 retry: exit 0 but a wrong solution);
   run.json identity (config, ranks, material, steps, seed, Scotch threads,
   interposer mode, extra parameters, frozen-mesh digest) and the exact
   input.dat the harness derives from the upstream inputs; energies.csv with
   the exact header, rows for steps 0..N, finite values; insertions.csv
   consistent (increasing steps, running total); summary.json modes, counts
   and final energies consistent with the histories and the per-rank
   summaries; partition.csv for the 2-rank cases; a Chrono row consistent
   with the metadata; a build record (build-record.json + run-time check)
   identical for all runs of one build configuration and one image;
2. enforces D-035(b) repeatability of the serial cases (all outputs bitwise
   identical over the R >= 3 repetitions), of the 2-rank partitions, and the
   equivalences the README states: JOSS-patch emulation = upstream physics
   (coh_np1), explicit partition = `mesh.distribute()` (same insertions and
   counts at 2 ranks);
3. labels every output file (and every summary field) of every case as
   `qualified` (bitwise repeatable over the R runs) or `diagnostic`
   (not repeatable: unqualified data under D-035) in qualification.json —
   the 2-rank energies are diagnostic;
4. writes the cases, determinism.json, sensitivity.json, qualification.json,
   attempts.json and provenance.json (per-run build records, never the
   current build directory) into a staging directory and replaces
   fixtures/<out>/ only after everything above passed; then refreshes the
   `<out>/` lines of fixtures/SHA256SUMS.

Nothing is written when any check fails (exit status 1, reasons on stderr).
An existing fixtures/<out>/ with cases is replaced only with --replace
(committed golden files are never replaced without a maintainer's approval).

--qualify-committed writes qualification.json for the committed
fixtures/perf_cohesive/ from its determinism.json (per-repetition digests)
after validating the committed files; --check validates fixtures/<out>/
offline (histories, counts, qualification labels, SHA256SUMS).
"""

import argparse
import datetime
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import perfcheck  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
PERF_DIR = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(PERF_DIR))
FIXTURES = os.path.join(REPO, "fixtures")
FROZEN_MESH = os.path.join(FIXTURES, "perf_cohesive", "meshes", "cube-h0.12.msh")
VOLUME = os.environ.get("PERF_VOLUME", "akantu-perf-cohesive")
IMAGE = os.environ.get("PERF_IMAGE", "akantu-oracle:dev")
MESH_IN_VOLUME = "/perf/meshes/fixture-cube-h0.12.msh"
OUT_RE = re.compile(r"^perf_cohesive(_[a-z0-9]+)*$")
COMMON = ["output_energy=true", "energy_precision=17"]
TIMING_KEYS = ("loop_time_s", "init_time_s")
RANK_TIMING_KEYS = ("event_time_s", "other_time_s")
ENERGY_COLUMNS = ("ed", "ep", "ek", "ew", "et")
OUTPUTS = ("energies.csv", "insertions.csv", "partition.csv", "summary.json")

# The runs of gen-fixtures.sh: case -> what the run must be.
SPECS = {
    "coh_np1": dict(ranks=1, material="cohesive", extra=[], fnte="upstream", blocked="local"),
    "ela_np1": dict(ranks=1, material="elastic", extra=[], fnte="upstream", blocked="local"),
    "coh_np2": dict(ranks=2, material="cohesive", extra=["export_partition=true"],
                    fnte="upstream", blocked="local"),
    "coh_np2_global": dict(ranks=2, material="cohesive",
                           extra=["export_partition=true", "blocked_node=global"],
                           fnte="upstream", blocked="global"),
    "coh_np2_distribute": dict(ranks=2, material="cohesive", extra=[], fnte="upstream",
                               blocked="local"),
    "coh_np1_fnte_joss": dict(ranks=1, material="cohesive", extra=[], fnte="joss",
                              blocked="local"),
}
CASES = ("coh_np1", "ela_np1", "coh_np2", "coh_np2_global")  # written, x reps
SERIAL = ("coh_np1", "ela_np1")
SINGLE = (("strict", "coh_np2_distribute"), ("strict", "coh_np1_fnte_joss"),
          ("default", "coh_np1"), ("default", "ela_np1"))  # x 1


class FixtureError(Exception):
    pass


def sha256(data):
    return hashlib.sha256(data).hexdigest()


# --------------------------------------------------------------------------- input
def fetch_volume():
    script = "cd /perf/runs && tar -cf - fixtures"
    tar = subprocess.run(["docker", "run", "--rm", "-v", f"{VOLUME}:/perf:ro", IMAGE,
                          "bash", "-c", script], check=True, capture_output=True).stdout
    runs = {}
    with tarfile.open(fileobj=io.BytesIO(tar)) as t:
        for m in t.getmembers():
            if m.isfile():
                d, base = os.path.split(m.name)
                runs.setdefault(d, {})[base] = t.extractfile(m).read()
    return runs


def fetch_dir(root):
    runs = {}
    for d, _, files in os.walk(os.path.join(root, "fixtures")):
        rel = os.path.relpath(d, root)
        for name in files:
            with open(os.path.join(d, name), "rb") as f:
                runs.setdefault(rel, {})[name] = f.read()
    return runs


def expected_input(material, steps, mesh, extra):
    """The input.dat in-container/run-one.sh derives from inputs/material-*.dat."""
    lines = perfcheck.read_text(os.path.join(PERF_DIR, "inputs", f"material-{material}.dat")).split("\n")
    keys = {kv.split("=", 1)[0]: kv.split("=", 1)[1] for kv in extra}
    present = {line[2:].split(" = ", 1)[0] for line in lines
               if line.startswith("  ") and " = " in line}
    out = []
    for line in lines:
        k = line[2:].split(" = ", 1)[0] if line.startswith("  ") and " = " in line else None
        if k == "max_steps":
            line = f"  max_steps = {steps}"
        elif k == "mesh":
            line = f"  mesh = {mesh}"
        elif k in keys:
            line = f"  {k} = {keys[k]}"
        out.append(line)
        if line == "user section [":
            out += [f"  {k} = {v}" for k, v in keys.items() if k not in present]
    return "\n".join(out).encode()


# --------------------------------------------------------------------------- validation
def validate_attempt(files, config, case, steps, mesh_sha):
    """List of problems of one attempt (empty = valid)."""
    spec = SPECS[case]
    extra = COMMON + spec["extra"]
    p = []
    if "run.json" not in files:
        return ["run.json missing (run refused or never started)"]
    meta = perfcheck.load_json(files["run.json"], "run.json", p)
    if meta is None:
        return p
    if meta.get("exit_status") != 0:
        p.append(f"exit status {meta.get('exit_status')}")
    for name in ("out.txt", "err.txt"):
        if name not in files:
            p.append(f"{name} missing (no evidence of a clean solve)")
    texts = [files.get(n, b"").decode("utf-8", "replace") for n in ("out.txt", "err.txt")]
    for marker, n in perfcheck.mumps_findings(*texts).items():
        p.append(f"MUMPS message {marker!r} x{n} (UD-216 / non-nominal solve)")
    want = {"config": config, "ranks": spec["ranks"], "material": spec["material"],
            "mesh": MESH_IN_VOLUME, "mesh_sha256": mesh_sha, "max_steps": steps,
            "extra_parameters": extra, "aka_seed": "1", "scotch_pthread_number": "1",
            "omp_num_threads": "1", "fill_nodes_to_elements": spec["fnte"]}
    for k, v in want.items():
        if meta.get(k) != v:
            p.append(f"run.json {k} = {meta.get(k)!r}, expected {v!r}")
    if meta.get("oversubscribed") is not False:
        p.append("run oversubscribed (or not recorded)")
    exp_input = expected_input(spec["material"], steps, MESH_IN_VOLUME, extra)
    if files.get("input.dat") != exp_input:
        p.append("input.dat differs from the harness-derived input")
    elif meta.get("input_sha256") != sha256(exp_input):
        p.append("run.json input_sha256 does not match input.dat")
    # build record
    if "build-record.json" not in files:
        p.append("build-record.json missing")
    else:
        rec = perfcheck.load_json(files["build-record.json"], "build-record.json", p)
        if rec is not None:
            d = build_digest(rec)
            if meta.get("build_record_sha256") != d:
                p.append("run.json build_record_sha256 does not match build-record.json")
            if rec.get("config") != config:
                p.append(f"build record of config {rec.get('config')!r}")
            if (meta.get("runtime_check") or {}).get("build_record_sha256") != d:
                p.append("run-time build check missing or for another record")
            if meta.get("binary_sha256") != (rec.get("binary") or {}).get("sha256"):
                p.append("binary digest differs from the build record")
            if meta.get("image_id") != rec.get("image_id"):
                p.append("image id differs from the build record")
    # outputs
    energies, ep = (perfcheck.parse_energies(files["energies.csv"], steps)
                    if "energies.csv" in files else ([], ["energies.csv missing"]))
    p += ep
    insertions, ip = (perfcheck.parse_insertions(files["insertions.csv"], steps)
                      if "insertions.csv" in files else (None, ["insertions.csv missing"]))
    p += ip
    summary = (perfcheck.load_json(files["summary.json"], "summary.json", p)
               if "summary.json" in files else None)
    if summary is None:
        p.append("summary.json missing or invalid")
    ranks = {}
    for name in files:
        m = re.match(r"^summary_rank(\d+)\.json$", name)
        if m:
            r = perfcheck.load_json(files[name], name, p)
            if r is not None:
                ranks[int(m.group(1))] = r
    if summary is not None and not ep and not ip:
        expect = {"cohesive_insertion": spec["material"] == "cohesive",
                  "blocked_node": spec["blocked"], "fill_nodes_to_elements": spec["fnte"],
                  "mesh": MESH_IN_VOLUME}
        p += perfcheck.check_summary(summary, spec["ranks"], steps, energies, insertions,
                                     ranks, expect)
    wants_partition = "export_partition=true" in spec["extra"]
    if wants_partition != ("partition.csv" in files):
        p.append("partition.csv " + ("missing" if wants_partition else "unexpected"))
    if wants_partition and "partition.csv" in files:
        lines = files["partition.csv"].decode("utf-8", "replace").splitlines()
        parts = {line.rsplit(",", 1)[-1].strip() for line in lines[1:]}
        if lines[:1] != ["type, element, partition"] or parts != {str(i) for i in range(spec["ranks"])}:
            p.append("partition.csv malformed or not using every part")
    _, _, chrono = perfcheck.parse_chrono(files.get("chrono.csv", b"").decode("utf-8", "replace"))
    p += perfcheck.check_chrono(chrono, spec["ranks"], steps, spec["material"] == "cohesive")
    return p


def build_digest(rec):
    return hashlib.sha256(json.dumps(rec, sort_keys=True, separators=(",", ":"))
                          .encode()).hexdigest()


def plan(reps):
    keys = [("strict", c, k) for k in range(1, reps + 1) for c in CASES]
    return keys + [(cfg, c, 1) for cfg, c in SINGLE]


def validate_all(runs, steps, reps, mesh_sha):
    """attempts record {key: {...}}; raises FixtureError listing every problem."""
    attempts = {}
    bad = []
    for config, case, rep in plan(reps):
        key = f"fixtures/{config}/{case}/rep{rep}"
        files = runs.get(key)
        problems = (["run directory missing"] if files is None
                    else validate_attempt(files, config, case, steps, mesh_sha))
        meta = {}
        if files and "run.json" in files:
            try:
                meta = json.loads(files["run.json"])
            except ValueError:
                pass
        attempts[key] = {"exit_status": meta.get("exit_status"), "valid": not problems,
                         "problems": problems, "started_utc": meta.get("started_utc"),
                         "build_record_sha256": meta.get("build_record_sha256")}
        if problems:
            bad.append(f"{key}: " + "; ".join(problems))
    # every attempt kept in the volume, also the ones not used (e.g. extra reps)
    for key in sorted(runs):
        if key.startswith("fixtures/") and key not in attempts and "run.json" in runs[key]:
            try:
                meta = json.loads(runs[key]["run.json"])
            except ValueError:
                meta = {}
            attempts[key] = {"exit_status": meta.get("exit_status"), "valid": None,
                             "problems": ["not part of the fixture plan (ignored)"],
                             "started_utc": meta.get("started_utc"),
                             "build_record_sha256": meta.get("build_record_sha256")}
    # one build record per configuration, one image for all
    digests, images = {}, set()
    for config, case, rep in plan(reps):
        a = attempts[f"fixtures/{config}/{case}/rep{rep}"]
        if a["valid"]:
            digests.setdefault(config, set()).add(a["build_record_sha256"])
            images.add(json.loads(runs[f"fixtures/{config}/{case}/rep{rep}"]["run.json"])
                       .get("image_id"))
    for config, ds in digests.items():
        if len(ds) != 1:
            bad.append(f"runs of config {config} come from {len(ds)} different builds")
    if len(images) > 1:
        bad.append(f"runs come from {len(images)} different images")
    return attempts, bad


# --------------------------------------------------------------------------- normalisation
def normalise(name, data, mesh_ref):
    """Fixture form of an output file (timings and volume paths removed)."""
    if name == "summary.json":
        s = json.loads(data)
        for k in TIMING_KEYS:
            s.pop(k, None)
        s["mesh"] = mesh_ref
        return (json.dumps(s, indent=1) + "\n").encode()
    if name.startswith("summary_rank"):
        s = json.loads(data)
        for k in RANK_TIMING_KEYS:
            s.get("fill_nodes_to_elements", {}).pop(k, None)
        return (json.dumps(s, sort_keys=False) + "\n").encode()
    if name == "input.dat":
        return data.replace(f"mesh = {MESH_IN_VOLUME}".encode(), f"mesh = {mesh_ref}".encode())
    return data


def outputs(run, mesh_ref):
    names = [n for n in run if n in OUTPUTS or re.match(r"^summary_rank\d+\.json$", n)]
    return {n: normalise(n, run[n], mesh_ref) for n in sorted(names)}


def energies(data):
    rows = [line.split(",") for line in data.decode().strip().splitlines()[1:]]
    return [[float(x) for x in r] for r in rows]


def rel(a, b):
    m = max(abs(a), abs(b))
    return 0.0 if m == 0 else abs(a - b) / m


def compare_energies(ea, eb, checkpoints):
    """Max relative difference per column up to each checkpoint step, and the
    first step at which the relative difference exceeds each threshold.
    Histories of different lengths or steps are an error (never a prefix)."""
    if len(ea) != len(eb) or any(ra[0] != rb[0] for ra, rb in zip(ea, eb)):
        raise FixtureError(f"energy histories differ in length or steps ({len(ea)} vs {len(eb)} rows)")
    out = {"max_rel_up_to_step": {}, "first_step_rel_above": {}}
    worst = {c: 0.0 for c in ENERGY_COLUMNS}
    thresholds = (1e-14, 1e-12, 1e-9, 1e-6, 1e-3)
    first = {c: {str(t): None for t in thresholds} for c in ENERGY_COLUMNS}
    for ra, rb in zip(ea, eb):
        step = int(ra[0])
        for i, c in enumerate(ENERGY_COLUMNS, start=1):
            r = rel(ra[i], rb[i])
            worst[c] = max(worst[c], r)
            for t in thresholds:
                if r > t and first[c][str(t)] is None:
                    first[c][str(t)] = step
        if step in checkpoints:
            out["max_rel_up_to_step"][str(step)] = {c: worst[c] for c in ENERGY_COLUMNS}
    out["first_step_rel_above"] = first
    return out


def summary_diff(sa, sb):
    a, b = json.loads(sa), json.loads(sb)
    d = {}
    for k in a:
        if isinstance(a[k], (int, float)) and not isinstance(a[k], bool) and k in b:
            d[k] = {"a": a[k], "b": b[k], "rel": rel(float(a[k]), float(b[k]))}
    return d


def insertion_diff(ia, ib):
    la = ia.decode().strip().splitlines()[1:]
    lb = ib.decode().strip().splitlines()[1:]
    first = None
    for x, y in zip(la, lb):
        if x != y:
            first = x.split(",")[0] + " / " + y.split(",")[0]
            break
    if first is None and len(la) != len(lb):
        first = "length differs"
    return {"insertion_steps": [len(la), len(lb)], "first_difference (step a / step b)": first,
            "identical": la == lb}


# --------------------------------------------------------------------------- qualification
POLICY = ("D-035(b): an output is qualified when it is bitwise identical over >= 3 fixed-seed "
          "runs (AKA_SEED=1, SCOTCH_PTHREAD_NUMBER=1, strict build); otherwise it is "
          "diagnostic (unqualified) data that must not be used as a golden value or to derive "
          "a tolerance without a reviewed workload-specific policy amendment")
STATUS = ("preparatory: plain CSV/JSON without manifest.json or tolerances.json, not DiffTest "
          "golden data (bounded format exception, fixtures/perf_cohesive/README.md). Gate before "
          "any golden/parity use: (1) only files labelled qualified; (2) a manifest and "
          "tolerance rules per docs/NUMERICAL_VALIDATION.md section 2/3, reviewed per section "
          "3.6; (3) scope limited to this mesh, these steps, ranks and modes")
INPUTS_NOTE = ("<case>/input.dat and ../meshes/cube-h0.12.msh are the run inputs (not outputs, "
               "not labelled); the frozen mesh is D-009 input data")
COMPARISONS = {
    "coh_np1_fnte_joss_vs_coh_np1": "enforced at generation: identical energies.csv and "
                                    "insertions.csv, identical summaries apart from the "
                                    "interposer mode and counters",
    "coh_np2_distribute_vs_coh_np2": "enforced at generation: identical insertions.csv and "
                                     "counts (same Scotch partition through mesh.distribute()); "
                                     "energies diagnostic (2-rank run-to-run variation)",
    "same_partition_reproduction": "a re-run of coh_np2 / coh_np2_global (same partition.csv, "
                                   "same rule) reproduces the qualified files exactly; its "
                                   "energies are diagnostic (determinism.json gives the measured "
                                   "repeat variation; no tolerance is proposed)",
    "cross_rank_physics (coh_np1 vs coh_np2_global)": "diagnostic only: measured on P = 2, this "
                                   "mesh, 1500 steps, blocked_node = global, strict build "
                                   "(sensitivity.json); not a rule for other P, meshes, steps "
                                   "or modes",
    "coh_np1 vs coh_np2": "not comparable: upstream rule blocks local node 3 of each rank "
                          "(different problems)",
}


def json_field_labels(reps_data):
    """{'qualified': [...], 'diagnostic': [...]} per top-level JSON field."""
    docs = [json.loads(d) for d in reps_data]
    q, dg = [], []
    for k in docs[0]:
        (q if all(d.get(k) == docs[0][k] for d in docs) else dg).append(k)
    return {"qualified": q, "diagnostic": dg}


def qualify_case(reps_outputs):
    files = sorted(reps_outputs[0])
    labels, fields = {}, {}
    for f in files:
        same = all(r.get(f) == reps_outputs[0][f] for r in reps_outputs)
        labels[f] = "qualified" if same else "diagnostic"
        if f.endswith(".json") and not same:
            fields[f] = json_field_labels([r[f] for r in reps_outputs])
    return labels, fields


# --------------------------------------------------------------------------- generation
def generate(runs, steps, reps, out_name, replace):
    if reps < 3:
        raise FixtureError("D-035(b) needs at least 3 repetitions")
    out = os.path.join(FIXTURES, out_name)
    mesh_ref = ("../meshes/cube-h0.12.msh" if out_name == "perf_cohesive"
                else "../../perf_cohesive/meshes/cube-h0.12.msh")
    if os.path.isdir(out) and any(c in os.listdir(out) for c in CASES) and not replace:
        raise FixtureError(f"fixtures/{out_name} already holds cases: pass --replace (never for "
                           "committed golden files without a maintainer's approval)")
    mesh_sha = sha256(perfcheck.read_bytes(FROZEN_MESH))
    attempts, bad = validate_all(runs, steps, reps, mesh_sha)
    if bad:
        raise FixtureError("invalid attempts:\n  " + "\n  ".join(bad))

    def run(config, case, rep):
        return runs[f"fixtures/{config}/{case}/rep{rep}"]

    checkpoints = set(range(0, steps + 1, 100)) | {1, 10, 50, steps}
    failures = []
    determinism = {"settings": "AKA_SEED=1, SCOTCH_PTHREAD_NUMBER=1 (timing fields removed)"}
    qualification = {"format": "cohesive-perf qualification v1", "status": STATUS,
                     "policy": POLICY, "repetitions": reps, "max_steps": steps, "cases": {},
                     "inputs": INPUTS_NOTE, "comparisons": COMPARISONS}
    for case in CASES:
        reps_out = [outputs(run("strict", case, k), mesh_ref) for k in range(1, reps + 1)]
        files = sorted(reps_out[0])
        rec = {"repetitions": reps,
               "files": {f: [sha256(r[f]) for r in reps_out] for f in files},
               "identical": all(r == reps_out[0] for r in reps_out)}
        if not rec["identical"]:
            rec["energies_vs_rep1"] = [
                compare_energies(energies(reps_out[0]["energies.csv"]),
                                 energies(r["energies.csv"]), checkpoints) for r in reps_out[1:]]
            rec["same_file_vs_rep1"] = [{f: reps_out[0].get(f) == r.get(f) for f in files}
                                        for r in reps_out[1:]]
        determinism[case] = rec
        labels, fields = qualify_case(reps_out)
        qualification["cases"][case] = {"ranks": SPECS[case]["ranks"], "files": labels,
                                        "json_fields": fields}
        if case in SERIAL and not rec["identical"]:
            failures.append(f"{case}: serial repetitions are not bitwise identical (D-035(b))")
        if "partition.csv" in labels and labels["partition.csv"] != "qualified":
            failures.append(f"{case}: partition not repeatable (SCOTCH_PTHREAD_NUMBER/AKA_SEED?)")

    equiv_a = outputs(run("strict", "coh_np2", 1), mesh_ref)
    equiv_b = outputs(run("strict", "coh_np2_distribute", 1), mesh_ref)
    counts = ("nb_global_nodes_start", "nb_global_nodes_end", "nb_owned_nodes_sum",
              "nb_regular_elements_dim3", "nb_cohesive_elements", "nb_insertion_steps",
              "first_insertion_step")
    sa, sb = json.loads(equiv_a["summary.json"]), json.loads(equiv_b["summary.json"])
    dist = {"same_file": {k: equiv_a.get(k) == equiv_b.get(k)
                          for k in sorted(set(equiv_a) | set(equiv_b)) if k != "partition.csv"},
            "same_counts": all(sa[k] == sb[k] for k in counts),
            "energies": compare_energies(energies(equiv_a["energies.csv"]),
                                         energies(equiv_b["energies.csv"]), checkpoints)}
    determinism["coh_np2_distribute_vs_coh_np2"] = dist
    if not (dist["same_file"]["insertions.csv"] and dist["same_counts"]):
        failures.append("coh_np2_distribute: insertions or counts differ from coh_np2 "
                        "(explicit partition not equivalent to mesh.distribute())")

    joss = outputs(run("strict", "coh_np1_fnte_joss", 1), mesh_ref)
    base = outputs(run("strict", "coh_np1", 1), mesh_ref)
    jsame = {k: base.get(k) == joss.get(k) for k in sorted(set(base) | set(joss))
             if not k.startswith("summary")}

    def strip_mode(data):  # interposer mode (summary.json) / counters (summary_rank*)
        s = json.loads(data)
        s.pop("fill_nodes_to_elements", None)
        return s
    for k in sorted(set(base) | set(joss)):
        if k.startswith("summary"):
            jsame[k + " (without interposer fields)"] = (
                k in base and k in joss and strip_mode(base[k]) == strip_mode(joss[k]))
    determinism["coh_np1_fnte_joss_vs_coh_np1"] = jsame
    if not all(jsame.values()):
        failures.append(f"JOSS emulation changes the physics: {jsame}")

    sensitivity = {"description": (
        "strict (-ffp-contract=off -DEIGEN_DONT_VECTORIZE) vs default (FMA contraction, "
        "vectorised Eigen) builds of the same library and driver, same inputs: a "
        "perturbation of the order of one rounding error per operation. Relative "
        "difference = |a-b|/max(|a|,|b|) per energy column and step. Diagnostic "
        "measurements, not tolerances.")}
    for case in SERIAL:
        s, d = run("strict", case, 1), run("default", case, 1)
        sensitivity[case] = {
            "energies": compare_energies(energies(s["energies.csv"]), energies(d["energies.csv"]),
                                         checkpoints),
            "summary": summary_diff(normalise("summary.json", s["summary.json"], mesh_ref),
                                    normalise("summary.json", d["summary.json"], mesh_ref)),
            "insertions": insertion_diff(s["insertions.csv"], d["insertions.csv"]),
        }
    s1 = run("strict", "coh_np1", 1)
    for case, note in (("coh_np2", "different problems (upstream: local node 3 of each rank)"),
                       ("coh_np2_global", "same problem (global node 3); cross-rank physics "
                                          "comparison, diagnostic only (P = 2, this mesh and "
                                          "window)")):
        s2 = run("strict", case, 1)
        sensitivity[f"coh_np1_vs_{case}"] = {
            "note": note,
            "energies": compare_energies(energies(s1["energies.csv"]), energies(s2["energies.csv"]),
                                         checkpoints),
            "summary": summary_diff(normalise("summary.json", s1["summary.json"], mesh_ref),
                                    normalise("summary.json", s2["summary.json"], mesh_ref)),
            "insertions": insertion_diff(s1["insertions.csv"], s2["insertions.csv"]),
        }
    if failures:
        raise FixtureError("generation checks failed:\n  " + "\n  ".join(failures))

    # provenance from the runs' own build records
    records, per_case, images, machines = {}, {}, set(), set()
    for config, case, rep in plan(reps):
        r = run(config, case, rep)
        rec = json.loads(r["build-record.json"])
        meta = json.loads(r["run.json"])
        records[build_digest(rec)] = rec
        images.add(meta.get("image_id"))
        machines.add((meta.get("host") or {}).get("machine"))
        if rep == 1:
            per_case[f"{config}/{case}"] = {
                "build_record_sha256": meta["build_record_sha256"],
                "started_utc": meta.get("started_utc"), "host": meta.get("host"),
                "launcher": meta.get("launcher"), "runtime_check": meta.get("runtime_check"),
                "harness_matches_build": meta.get("harness_matches_build")}
    provenance = {
        "generated_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "generator": "benchmarks/cohesive-perf/scripts/gen-fixtures.sh "
                     f"--steps {steps} --reps {reps}" + (f" --out {out_name}"
                                                         if out_name != "perf_cohesive" else ""),
        "harness_git_commit": git("rev-parse", "HEAD"),
        "harness_git_dirty": bool(git("status", "--porcelain", "benchmarks/cohesive-perf")),
        "platform": sorted(m for m in machines if m),
        "image_id": sorted(i for i in images if i),
        "build_records": records,
        "runs": per_case,
        "mesh": "perf_cohesive/meshes/cube-h0.12.msh", "mesh_sha256": mesh_sha,
        "mesh_command": "gmsh -3 -setnumber h 0.12 -format msh41 -o cube-h0.12.msh "
                        "benchmarks/cohesive-perf/inputs/cube.geo (Gmsh 4.13.1, oracle image)",
        "max_steps": steps,
        "cases": {
            "coh_np1": "cohesive_insertion = true, 1 MPI rank, strict",
            "ela_np1": "cohesive_insertion = false (material-elastic.dat), 1 MPI rank, strict",
            "coh_np2": "cohesive_insertion = true, 2 MPI ranks, upstream blocked_dofs(3, _y) on "
                       "local node 3 of each rank, export_partition = true (partition.csv = Scotch "
                       "partition of every element of the global mesh), strict",
            "coh_np2_global": "as coh_np2 with blocked_node = global (node of global id 3 on "
                              "every rank holding it: the 1-rank problem), strict",
        },
        "run_command": "mpiexec --oversubscribe -n <ranks> -x AKA_SEED -x SCOTCH_PTHREAD_NUMBER "
                       "-x AKANTU_RS_FILL_NODES_TO_ELEMENTS cohesive_extrinsic --aka_input_file "
                       "input.dat (AKA_SEED=1, SCOTCH_PTHREAD_NUMBER=1, OMP_NUM_THREADS=1, image "
                       "OpenMPI settings, single node)",
    }
    if len(provenance["image_id"]) != 1 or len(provenance["platform"]) != 1:
        raise FixtureError(f"runs span images {provenance['image_id']} / platforms "
                           f"{provenance['platform']}")

    # stage, then replace
    staging = os.path.join(FIXTURES, f".{out_name}.staging-{os.getpid()}")
    if os.path.exists(staging):
        shutil.rmtree(staging)
    os.makedirs(staging)
    try:
        for case in CASES:
            r = run("strict", case, 1)
            d = os.path.join(staging, case)
            os.makedirs(d)
            files = outputs(r, mesh_ref)
            files["input.dat"] = normalise("input.dat", r["input.dat"], mesh_ref)
            for name, data in files.items():
                with open(os.path.join(d, name), "wb") as f:
                    f.write(data)
        for name, obj in (("determinism.json", determinism), ("sensitivity.json", sensitivity),
                          ("qualification.json", qualification), ("attempts.json", attempts),
                          ("provenance.json", provenance)):
            dump(os.path.join(staging, name), obj)
        for keep in ("README.md", "meshes"):
            src = os.path.join(out, keep)
            if os.path.isdir(src):
                shutil.copytree(src, os.path.join(staging, keep))
            elif os.path.isfile(src):
                shutil.copy2(src, os.path.join(staging, keep))
        if not os.path.exists(os.path.join(staging, "README.md")):
            with open(os.path.join(staging, "README.md"), "w") as f:
                f.write(f"# {out_name}\n\nRegeneration of `fixtures/perf_cohesive/` (same cases, "
                        "same frozen mesh) on the platform in `provenance.json`; read "
                        "`../perf_cohesive/README.md` and `qualification.json` before any use.\n")
        check_dir(staging, steps)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    old = os.path.join(FIXTURES, f".{out_name}.old-{os.getpid()}") if os.path.exists(out) else None
    if old:
        os.rename(out, old)
    try:
        os.rename(staging, out)
    except BaseException:
        if old:
            os.rename(old, out)  # put the previous fixtures back
        shutil.rmtree(staging, ignore_errors=True)
        raise
    if old:
        shutil.rmtree(old)
    update_sums(out_name)
    return determinism, sensitivity


def dump(path, obj):
    with open(path, "w") as f:
        json.dump(obj, f, indent=1, sort_keys=True)
        f.write("\n")


def git(*args):
    return subprocess.run(["git", "-C", REPO, *args], capture_output=True, text=True).stdout.strip()


# --------------------------------------------------------------------------- committed data
def check_dir(d, steps=None):
    """Offline validation of a fixture directory; raises FixtureError."""
    problems = []
    q = perfcheck.read_json(os.path.join(d, "qualification.json")) if os.path.exists(
        os.path.join(d, "qualification.json")) else None
    if steps is None:
        steps = perfcheck.read_json(os.path.join(d, "provenance.json"))["max_steps"]
    for case in CASES:
        cd = os.path.join(d, case)
        spec = SPECS[case]
        files = {n: perfcheck.read_bytes(os.path.join(cd, n)) for n in os.listdir(cd)}
        e, ep = perfcheck.parse_energies(files.get("energies.csv", b""), steps)
        i, ip = perfcheck.parse_insertions(files.get("insertions.csv", b""), steps)
        p = ep + ip
        ranks = {int(n[len("summary_rank"):-5]): json.loads(files[n]) for n in files
                 if re.match(r"^summary_rank\d+\.json$", n)}
        s = json.loads(files["summary.json"])
        if not p:
            p += perfcheck.check_summary(
                s, spec["ranks"], steps, e, i, ranks,
                {"cohesive_insertion": spec["material"] == "cohesive",
                 "blocked_node": spec["blocked"], "fill_nodes_to_elements": spec["fnte"]})
        if ("export_partition=true" in spec["extra"]) != ("partition.csv" in files):
            p.append("partition.csv presence wrong")
        if q is not None:
            labels = q["cases"].get(case, {}).get("files", {})
            listed = set(labels)
            present = {n for n in files if n != "input.dat"}
            if listed != present:
                p.append(f"qualification labels {sorted(listed)} != files {sorted(present)}")
            if case in SERIAL and any(v != "qualified" for v in labels.values()):
                p.append("serial case with unqualified files")
        problems += [f"{case}: {x}" for x in p]
    if q is None:
        problems.append("qualification.json missing")
    if problems:
        raise FixtureError("fixture check failed:\n  " + "\n  ".join(problems))


def qualify_committed():
    """qualification.json for the committed fixtures/perf_cohesive/ (from the
    per-repetition digests in its determinism.json)."""
    out = os.path.join(FIXTURES, "perf_cohesive")
    det = perfcheck.read_json(os.path.join(out, "determinism.json"))
    prov = perfcheck.read_json(os.path.join(out, "provenance.json"))
    q = {"format": "cohesive-perf qualification v1", "status": STATUS, "policy": POLICY,
         "repetitions": None, "max_steps": prov["max_steps"], "cases": {},
         "inputs": INPUTS_NOTE, "comparisons": COMPARISONS,
         "derived_from": "determinism.json per-repetition SHA-256 of every output file (the "
                         "committed files are repetition 1); field-level labels are not "
                         "available for this generation (whole JSON files are labelled)"}
    reps = set()
    for case in CASES:
        files = det[case]["files"]
        reps.add(det[case]["repetitions"])
        labels = {}
        for f, hashes in files.items():
            labels[f] = "qualified" if len(set(hashes)) == 1 else "diagnostic"
            committed = sha256(perfcheck.read_bytes(os.path.join(out, case, f)))
            if committed != hashes[0]:
                raise FixtureError(f"{case}/{f}: committed file is not repetition 1 of "
                                   "determinism.json")
        q["cases"][case] = {"ranks": SPECS[case]["ranks"], "files": labels, "json_fields": {}}
    if reps != {3}:
        raise FixtureError(f"unexpected repetition counts {reps}")
    q["repetitions"] = 3
    dump(os.path.join(out, "qualification.json"), q)
    check_dir(out)
    update_sums("perf_cohesive")
    return q


def compare_dirs(a_name, b_name):
    """Diagnostic comparison of two fixture directories with the same cases
    (e.g. an x86_64 regeneration against the committed aarch64 one, D-043):
    file identity, energy differences per column, insertion sequences,
    summary differences. Never a pass/fail."""
    a_dir, b_dir = os.path.join(FIXTURES, a_name), os.path.join(FIXTURES, b_name)
    pa = perfcheck.read_json(os.path.join(a_dir, "provenance.json"))
    pb = perfcheck.read_json(os.path.join(b_dir, "provenance.json"))
    steps = pa["max_steps"]
    if pb["max_steps"] != steps:
        raise FixtureError("the two directories have different step counts")
    checkpoints = set(range(0, steps + 1, 100)) | {1, 10, 50, steps}
    out = {"a": a_name, "b": b_name, "a_platform": pa.get("platform", "aarch64 (2026-11-10)"),
           "b_platform": pb.get("platform", "aarch64 (2026-11-10)"), "cases": {}}
    for case in CASES:
        fa = {n: perfcheck.read_bytes(os.path.join(a_dir, case, n))
              for n in os.listdir(os.path.join(a_dir, case))}
        fb = {n: perfcheck.read_bytes(os.path.join(b_dir, case, n))
              for n in os.listdir(os.path.join(b_dir, case))}
        out["cases"][case] = {
            "identical_files": {n: fa.get(n) == fb.get(n) for n in sorted(set(fa) | set(fb))
                                if n != "input.dat"},
            "energies": compare_energies(energies(fa["energies.csv"]), energies(fb["energies.csv"]),
                                         checkpoints),
            "insertions": insertion_diff(fa["insertions.csv"], fb["insertions.csv"]),
            "summary": summary_diff(fa["summary.json"], fb["summary.json"]),
        }
    return out


def update_sums(out_name):
    """Replace the <out_name>/ lines of fixtures/SHA256SUMS (atomically)."""
    sums_path = os.path.join(FIXTURES, "SHA256SUMS")
    prefix = out_name + "/"
    lines = [line for line in perfcheck.read_text(sums_path).splitlines(keepends=True)
             if not line.split("  ", 1)[1].startswith(prefix)]
    for root, _, files in os.walk(os.path.join(FIXTURES, out_name)):
        for name in files:
            p = os.path.join(root, name)
            relp = os.path.relpath(p, FIXTURES)
            lines.append(f"{sha256(perfcheck.read_bytes(p))}  {relp}\n")
    lines.sort(key=lambda line: line.split("  ", 1)[1].encode())
    tmp = sums_path + f".tmp-{os.getpid()}"
    with open(tmp, "w") as f:
        f.writelines(lines)
    os.replace(tmp, sums_path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", type=int)
    ap.add_argument("--reps", type=int)
    ap.add_argument("--out", default="perf_cohesive")
    ap.add_argument("--replace", action="store_true")
    ap.add_argument("--runs-dir", help="local copy of /perf/runs (layout fixtures/<config>/...)")
    ap.add_argument("--sums-only", action="store_true",
                    help="only refresh the <out>/ lines of fixtures/SHA256SUMS")
    ap.add_argument("--qualify-committed", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--compare", metavar="OTHER", help="diagnostic comparison of fixtures/<out> "
                    "with fixtures/OTHER (JSON on stdout)")
    a = ap.parse_args()
    if not OUT_RE.match(a.out):
        sys.exit(f"--out must match {OUT_RE.pattern} (a directory directly under fixtures/)")
    try:
        if a.sums_only:
            update_sums(a.out)
            return
        if a.qualify_committed:
            qualify_committed()
            print("fixtures/perf_cohesive/qualification.json written")
            return
        if a.compare:
            if not OUT_RE.match(a.compare):
                sys.exit(f"--compare must match {OUT_RE.pattern}")
            print(json.dumps(compare_dirs(a.compare, a.out), indent=1, sort_keys=True))
            return
        if a.check:
            check_dir(os.path.join(FIXTURES, a.out))
            print(f"fixtures/{a.out}: OK")
            return
        if not a.steps or not a.reps or a.steps < 1:
            sys.exit("--steps and --reps are required")
        runs = fetch_dir(a.runs_dir) if a.runs_dir else fetch_volume()
        determinism, sensitivity = generate(runs, a.steps, a.reps, a.out, a.replace)
    except FixtureError as e:
        sys.exit(f"make_fixtures.py: {e}")
    print(json.dumps({k: (v.get("identical", v) if isinstance(v, dict) else v)
                      for k, v in determinism.items()}, indent=1)[:3000])
    for case in SERIAL:
        e = sensitivity[case]["energies"]["first_step_rel_above"]
        print(case, "first step rel diff > 1e-9:", {c: e[c]["1e-09"] for c in ENERGY_COLUMNS})


if __name__ == "__main__":
    main()
