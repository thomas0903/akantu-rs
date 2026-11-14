#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Turn the runs of scripts/gen-fixtures-eigen.sh into
fixtures/perf_cohesive/{coh,ela}_np1_eigen/.

    python3 -I scripts/make_fixtures_eigen.py --steps N --reps R [--allow-dirty]
    python3 -I scripts/make_fixtures_eigen.py --preflight      (before the runs)
    python3 -I scripts/make_fixtures_eigen.py --check          (committed cases, offline)

Validates every repetition (`check_outputs`):
exit status, step count, Eigen static solve (run metadata and input), no
MUMPS message in the logs, a complete energy history (rows 0…N consecutive,
six finite fields), a consistent insertion list (increasing steps within
1…N, positive counts, running total, total = `nb_cohesive_elements`,
rows = `nb_insertion_steps`, first step = `first_insertion_step`), finite
summary values and the summary's step count and rank count. Then checks
that the strict repetitions are identical (D-035), writes the two cases
(repetition 1, normalised as scripts/make_fixtures.py does), records the
determinism and the difference to the MUMPS cases in `eigen_variant.json`,
and refreshes the `perf_cohesive/` lines of fixtures/SHA256SUMS.

Refusals before anything is written (`--preflight` runs them before the
runs): a harness tree with uncommitted changes (`--allow-dirty` records the
`git status` lines instead), and on any platform other than aarch64 an
oracle install without the D-050 pin (Eigen cache sizes fixed at compile
time: `-DEIGEN_NO_CPUID -DEIGEN_DEFAULT_L1_CACHE_SIZE=16384
-DEIGEN_DEFAULT_L2_CACHE_SIZE=524288 -DEIGEN_DEFAULT_L3_CACHE_SIZE=524288`
in the driver's compile command). Unpinned, Eigen sizes the SparseLU
blocking from the host's CPUID and the x86_64 oracle differs
(Δt rel 1.8e-14, `ed` 5.9e-7 at step 1500); pinned, it is byte-identical
to the aarch64 cases.
"""

import argparse
import datetime
import importlib.util
import io
import json
import math
import os
import subprocess
import sys
import tarfile

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("make_fixtures", os.path.join(HERE, "make_fixtures.py"))
mf = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mf)

CASES = {"coh_np1_eigen": "coh_np1", "ela_np1_eigen": "ela_np1"}
OUT = os.path.join(mf.FIXTURES, "perf_cohesive")
MESH_REF = "../meshes/cube-h0.12.msh"  # input/summary mesh path in the committed cases
ENERGY_HEADER = "step, ed, ep, ek, ew, et"
INSERTION_HEADER = "step, nb_inserted, nb_cohesive_total"
D050_DEFINES = ("-DEIGEN_NO_CPUID", "-DEIGEN_DEFAULT_L1_CACHE_SIZE=16384",
                "-DEIGEN_DEFAULT_L2_CACHE_SIZE=524288", "-DEIGEN_DEFAULT_L3_CACHE_SIZE=524288")
HARNESS_PATHS = ("benchmarks/cohesive-perf", "fixtures/perf_cohesive/meshes")


def finite_float(text):
    """`float(text)` if it is a finite number, else None."""
    try:
        v = float(text)
    except ValueError:
        return None
    return v if math.isfinite(v) else None


def check_energies(data, steps):
    problems = []
    lines = data.decode().rstrip("\n").split("\n")
    if not lines or lines[0] != ENERGY_HEADER:
        return [f"energies.csv: header {lines[0] if lines else None!r}, expected {ENERGY_HEADER!r}"]
    rows = lines[1:]
    if len(rows) != steps + 1:
        problems.append(f"energies.csv: {len(rows)} rows, expected {steps + 1} (steps 0…{steps}; "
                        "truncated or extra history)")
    for k, line in enumerate(rows):
        f = line.split(", ")
        if len(f) != 6:
            problems.append(f"energies.csv row {k}: {len(f)} fields")
            break
        if f[0] != str(k):
            problems.append(f"energies.csv row {k}: step {f[0]!r} (not consecutive)")
            break
        bad = [c for c, t in zip(("ed", "ep", "ek", "ew", "et"), f[1:]) if finite_float(t) is None]
        if bad:
            problems.append(f"energies.csv step {k}: non-finite or invalid {', '.join(bad)}")
            break
    return problems


def check_insertions(data, steps, summary):
    problems = []
    lines = data.decode().rstrip("\n").split("\n")
    if not lines or lines[0] != INSERTION_HEADER:
        return [f"insertions.csv: header {lines[0] if lines else None!r}, expected {INSERTION_HEADER!r}"]
    total, last, first = 0, 0, 0
    rows = [l for l in lines[1:] if l]
    for line in rows:
        try:
            step, nb, cum = (int(x) for x in line.split(", "))
        except ValueError:
            problems.append(f"insertions.csv: invalid row {line!r}")
            return problems
        if not last < step <= steps:
            problems.append(f"insertions.csv: step {step} not increasing within 1…{steps}")
        if nb <= 0:
            problems.append(f"insertions.csv step {step}: {nb} inserted")
        total += nb
        if cum != total:
            problems.append(f"insertions.csv step {step}: running total {cum}, expected {total}")
        first = first or step
        last = step
    expected = {"nb_cohesive_elements": total, "nb_insertion_steps": len(rows),
                "first_insertion_step": first}
    for key, value in expected.items():
        if summary.get(key) != value:
            problems.append(f"summary.json {key} = {summary.get(key)!r}, insertions.csv gives {value}")
    return problems


def check_summary(summary, steps):
    problems = []
    if summary.get("max_steps") != steps:
        problems.append(f"summary.json max_steps = {summary.get('max_steps')!r}, expected {steps}")
    if summary.get("psize") != 1:
        problems.append(f"summary.json psize = {summary.get('psize')!r}, expected 1")

    def walk(key, v):
        if isinstance(v, bool) or isinstance(v, str):
            return
        if isinstance(v, (int, float)):
            if not math.isfinite(v):
                problems.append(f"summary.json {key}: non-finite {v!r}")
        elif isinstance(v, list):
            for i, x in enumerate(v):
                walk(f"{key}[{i}]", x)
        elif isinstance(v, dict):
            for k, x in v.items():
                walk(f"{key}.{k}" if key else k, x)
    walk("", summary)
    dt = summary.get("time_step")
    if not isinstance(dt, (int, float)) or isinstance(dt, bool) or not dt > 0:
        problems.append(f"summary.json time_step = {dt!r}, expected > 0")
    return problems


def check_outputs(files, steps):
    """Problems of one run's (or one committed case's) output files; `files`
    maps names to bytes (`energies.csv`, `insertions.csv`, `summary.json`,
    `summary_rank0.json`)."""
    problems = []
    for name in ("energies.csv", "insertions.csv", "summary.json", "summary_rank0.json"):
        if name not in files:
            problems.append(f"{name} missing")
    if problems:
        return problems
    try:
        summary = json.loads(files["summary.json"])
        json.loads(files["summary_rank0.json"])
    except ValueError as e:
        return [f"invalid JSON: {e}"]
    problems += check_summary(summary, steps)
    problems += check_energies(files["energies.csv"], steps)
    problems += check_insertions(files["insertions.csv"], steps, summary)
    return problems


def check_run(r, steps):
    """`check_outputs` plus the run metadata and logs of one repetition."""
    problems = []
    try:
        meta = json.loads(r["run.json"])
    except (KeyError, ValueError):
        return ["run.json missing or invalid"]
    if meta.get("exit_status") != 0:
        problems.append(f"exit status {meta.get('exit_status')!r}")
    if meta.get("max_steps") != steps:
        problems.append(f"run.json max_steps = {meta.get('max_steps')!r}, expected {steps}")
    if meta.get("ranks") != 1:
        problems.append(f"run.json ranks = {meta.get('ranks')!r}, expected 1")
    if meta.get("static_sparse_solver") != "eigen":
        problems.append("not an Eigen static solve (run.json static_sparse_solver)")
    if b"sparse_solver_type = eigen" not in r.get("input.dat", b""):
        problems.append("input.dat lacks sparse_solver_type = eigen")
    for log in ("out.txt", "err.txt"):
        if b"MUMPS" in r.get(log, b""):
            problems.append(f"{log} mentions MUMPS (the static solve must not use it)")
    return problems + check_outputs(r, steps)


def build_info(config):
    """Lines of /perf/builds/<config>/build-info.txt (scripts/build.sh)."""
    r = subprocess.run(["docker", "run", "--rm", "-v", f"{mf.VOLUME}:/perf:ro", mf.IMAGE, "cat",
                        f"/perf/builds/{config}/build-info.txt"], capture_output=True, text=True)
    return r.stdout.splitlines()


def dirty_lines():
    out = mf.git("status", "--porcelain", "--", *HARNESS_PATHS)
    return [l for l in out.splitlines() if l.strip()]


def d050_status(build_info_lines):
    """(platform, pinned, detail) of the strict install the runs used."""
    arch = subprocess.run(["docker", "image", "inspect", "--format", "{{.Architecture}}", mf.IMAGE],
                          capture_output=True, text=True).stdout.strip() or "unknown"
    cmd = next((l for l in build_info_lines if l.startswith("driver_compile_command=")), "")
    pinned = all(d in cmd.split() for d in D050_DEFINES)
    return arch, pinned, cmd


def preflight(allow_dirty):
    """Refusals that do not need the runs; returns the provenance fields."""
    dirty = dirty_lines()
    if dirty and not allow_dirty:
        sys.exit("refusing: uncommitted changes in the harness or the frozen meshes "
                 f"({len(dirty)} paths; commit them or pass --allow-dirty):\n  " + "\n  ".join(dirty))
    info = build_info("strict")
    if not info:
        sys.exit("refusing: no /perf/builds/strict/build-info.txt (run scripts/build.sh strict)")
    arch, pinned, _cmd = d050_status(info)
    if arch != "arm64" and not pinned:
        sys.exit(f"refusing: image architecture {arch} without the D-050 pin in the driver's "
                 f"compile command (needs {' '.join(D050_DEFINES)}; use the pinned oracle install "
                 "of the x86_64 oracle, docs/ORACLE_BUILD.md)")
    return {"harness_git_dirty": bool(dirty), "harness_git_status": dirty,
            "platform": arch, "d050_eigen_cache_pin": pinned or arch == "arm64",
            "d050_note": "aarch64: Eigen's built-in cache sizes are the D-050 values"
                         if arch == "arm64" else "pinned at compile time (driver compile command)"}


def check_committed():
    """Offline validation of the committed cases (no Docker)."""
    failed = False
    for case in CASES:
        d = os.path.join(OUT, case)
        files = {}
        for n in os.listdir(d):
            with open(os.path.join(d, n), "rb") as fh:
                files[n] = fh.read()
        steps = json.loads(files["summary.json"])["max_steps"]
        problems = check_outputs(files, steps)
        if b"sparse_solver_type = eigen" not in files.get("input.dat", b""):
            problems.append("input.dat lacks sparse_solver_type = eigen")
        print(f"{case}: {'OK' if not problems else 'FAILED'} ({steps} steps)")
        for p in problems:
            print(f"  {p}")
        failed |= bool(problems)
    return 1 if failed else 0


def fetch():
    script = "cd /perf/runs && tar -cf - fixtures-eigen"
    tar = subprocess.run(["docker", "run", "--rm", "-v", f"{mf.VOLUME}:/perf:ro", mf.IMAGE,
                          "bash", "-c", script], check=True, capture_output=True).stdout
    runs = {}
    with tarfile.open(fileobj=io.BytesIO(tar)) as t:
        for m in t.getmembers():
            if m.isfile():
                d, base = os.path.split(m.name)
                runs.setdefault(d, {})[base] = t.extractfile(m).read()
    return runs


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--steps", type=int)
    ap.add_argument("--reps", type=int)
    ap.add_argument("--allow-dirty", action="store_true",
                    help="write fixtures from a dirty harness tree (recorded in the provenance)")
    ap.add_argument("--preflight", action="store_true", help="only the refusals (before the runs)")
    ap.add_argument("--check", action="store_true", help="validate the committed cases offline")
    a = ap.parse_args()
    if a.check:
        sys.exit(check_committed())
    pre = preflight(a.allow_dirty)
    if a.preflight:
        print("preflight OK:", json.dumps(pre))
        return
    if a.steps is None or a.reps is None:
        sys.exit("--steps and --reps are required")
    runs = fetch()

    def run(case, rep):
        key = f"fixtures-eigen/strict/{case}/rep{rep}"
        if key not in runs:
            sys.exit(f"missing run {key}")
        problems = check_run(runs[key], a.steps)
        if problems:
            sys.exit(f"{key}: refused:\n  " + "\n  ".join(problems))
        return runs[key]

    checkpoints = set(range(0, a.steps + 1, 100)) | {1, 10, 50, a.steps}
    record = {"settings": "strict build, AKA_SEED=1, SCOTCH_PTHREAD_NUMBER=1, static solve "
                          "with sparse_solver_type = eigen (timing fields removed)"}
    for case, mumps_case in CASES.items():
        reps = [mf.outputs(run(case, k), MESH_REF) for k in range(1, a.reps + 1)]
        files = sorted(reps[0])
        rec = {"repetitions": a.reps,
               "files": {f: [mf.sha256(r[f]) for r in reps] for f in files},
               "identical": all(r == reps[0] for r in reps),
               "validated": "check_run (exit, steps, Eigen solve, logs, histories, counts, finiteness)"}
        if not rec["identical"]:
            sys.exit(f"{case}: strict repetitions differ (not deterministic)")
        mumps_dir = os.path.join(OUT, mumps_case)
        mumps = {n: open(os.path.join(mumps_dir, n), "rb").read()
                 for n in ("energies.csv", "insertions.csv", "summary.json")}
        rec[f"vs_{mumps_case} (MUMPS static solve)"] = {
            "energies": mf.compare_energies(mf.energies(mumps["energies.csv"]),
                                            mf.energies(reps[0]["energies.csv"]), checkpoints),
            "summary": mf.summary_diff(mumps["summary.json"], reps[0]["summary.json"]),
            "insertions": mf.insertion_diff(mumps["insertions.csv"], reps[0]["insertions.csv"]),
        }
        record[case] = rec
        r = run(case, 1)
        d = os.path.join(OUT, case)
        os.makedirs(d, exist_ok=True)
        for old in os.listdir(d):
            os.remove(os.path.join(d, old))
        out = mf.outputs(r, MESH_REF)
        out["input.dat"] = mf.normalise("input.dat", r["input.dat"], MESH_REF)
        for name, data in out.items():
            with open(os.path.join(d, name), "wb") as f:
                f.write(data)
    record["provenance"] = {
        "generated_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "generator": f"benchmarks/cohesive-perf/scripts/gen-fixtures-eigen.sh --steps {a.steps} "
                     f"--reps {a.reps}",
        "harness_git_commit": mf.git("rev-parse", "HEAD"),
        "driver_sha256": mf.sha256(open(os.path.join(mf.PERF_DIR, "src", "cohesive_extrinsic.cc"),
                                        "rb").read()),
        "image_id": subprocess.run(["docker", "image", "inspect", "--format", "{{.Id}}", mf.IMAGE],
                                   capture_output=True, text=True).stdout.strip(),
        "build_info_strict": build_info("strict"),
        **pre,
    }
    with open(os.path.join(OUT, "eigen_variant.json"), "w") as f:
        json.dump(record, f, indent=1, sort_keys=True)
        f.write("\n")
    mf.update_sums("perf_cohesive")
    for case in CASES:
        e = record[case][f"vs_{CASES[case]} (MUMPS static solve)"]
        print(case, "identical reps:", record[case]["identical"],
              "| insertions vs MUMPS identical:", e["insertions"]["identical"],
              "| max rel diff vs MUMPS at last step:",
              e["energies"]["max_rel_up_to_step"].get(str(a.steps)))


if __name__ == "__main__":
    main()
