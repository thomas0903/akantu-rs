#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the part of a final
# campaign data directory that goes into the repository
# (scripts/perf/final/README.md).
"""Copy a part's records and write compact per-run records.

    extract.py DATA/<PART> OUTDIR

Copies the part's own records as they are (summary.md, provenance.txt,
plan.txt, plan.jsonl, groups.jsonl, sessions.jsonl, checks.jsonl,
index.jsonl, progress.log, commands.log, digests.tsv, part A's rounds/,
part F's mpilayer/ tables) and replaces the per-run directories, which hold
meshes, partitions, logs and traces, by two compact files with exactly what
summarize.py reads from them:

- runs.jsonl (cohesive parts C to G): one line per run directory of
  runs/: rank 0's Chrono totals (chrono.csv), the run's status.txt, the
  keys of summary.json that the tables use, the memory sampler's peak VmHWM
  per process (mem.jsonl), the SHA-256 the run recorded for its binary, the
  recorded binding (bindings.txt), and the SHA-256 of energies.csv and
  insertions.csv (equal digests = byte-identical per-step energies and
  insertions; the checks compared the files themselves);
- suite-records.tsv (suite directories of parts B, D, F): one line per
  JSON line of the suites (directory, variant, ranks, case, repetition,
  phase, calls, seconds, element count, threads, precision, digest), and
  suite-bindings.tsv (the recorded binding of every MPI suite run).

Nothing is computed: the values are those of the files, unrounded.
"""

import csv
import hashlib
import json
import re
import shutil
import sys
from pathlib import Path

COPY = ["summary.md", "provenance.txt", "plan.txt", "plan.jsonl", "groups.jsonl",
        "sessions.jsonl", "checks.jsonl", "index.jsonl", "progress.log", "commands.log",
        "digests.tsv"]
SUMMARY_KEYS = ["psize", "max_steps", "nb_regular_elements_dim3", "nb_cohesive_elements",
                "nb_insertion_steps", "first_insertion_step", "nb_global_nodes_start",
                "nb_global_nodes_end", "energy_dissipated", "energy_potential",
                "energy_kinetic", "displacement_l2_norm", "rust_exec", "rust_memory_kib",
                "rust_static_matrix", "rust_static_solver_stats", "rust_static_sparse_solver",
                "rust_sparse_solver", "fill_nodes_to_elements"]
SUITE_DIRS = re.compile(r"^(suite-|mpi-|dist-l$|python$)")


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None


def chrono(p):
    if not p.exists():
        return None
    rows = list(csv.reader(p.read_text().splitlines(), skipinitialspace=True))
    return dict(zip(rows[0], rows[1], strict=False)) if len(rows) >= 2 else None


def status(p):
    st = {}
    if p.exists():
        for ln in p.read_text().splitlines():
            if "=" in ln:
                k, v = ln.split("=", 1)
                try:
                    st[k] = json.loads(v)
                except json.JSONDecodeError:
                    st[k] = v
    return st


def mem_peaks(p):
    if not p.exists():
        return None
    peaks = {}
    for ln in p.read_text().splitlines():
        try:
            o = json.loads(ln)
        except json.JSONDecodeError:
            continue
        for pid, _start, _rss, hwm in o.get("procs", []):
            peaks[pid] = max(peaks.get(pid, 0), hwm)
    return sorted(peaks.values()) or None


def cohesive_runs(d, out):
    runs = d / "runs"
    if not runs.is_dir():
        return 0
    n = 0
    with open(out / "runs.jsonl", "w") as f:
        for camp in sorted(x for x in runs.iterdir() if x.is_dir()):
            for case_dir in sorted(x for x in camp.iterdir() if x.is_dir()):
                for npd in sorted(x for x in case_dir.iterdir() if x.is_dir()):
                    rd = npd / "rep1"
                    if not rd.is_dir():
                        continue
                    s = rd / "summary.json"
                    summ = json.loads(s.read_text()) if s.exists() else {}
                    run_json = rd / "run.json"
                    rec = {
                        "campaign": camp.name, "case": case_dir.name, "np": npd.name,
                        "chrono_rank0": chrono(rd / "chrono.csv"),
                        "status": status(rd / "status.txt"),
                        "summary": {k: summ[k] for k in SUMMARY_KEYS if k in summ},
                        "vmhwm_kib_per_process": mem_peaks(npd / "rep1.mem.jsonl")
                        or mem_peaks(rd / "mem.jsonl"),
                        "binary_sha256": (
                            (rd / "binary.sha256").read_text().split()[0]
                            if (rd / "binary.sha256").exists() else
                            json.loads(run_json.read_text()).get("binary_sha256")
                            if run_json.exists() else None),
                        "binding": ((runs / f"{camp.name}.bindings.txt").read_text()
                                    if (runs / f"{camp.name}.bindings.txt").exists() else None),
                        "energies_csv_sha256": sha(rd / "energies.csv"),
                        "insertions_csv_sha256": sha(rd / "insertions.csv"),
                    }
                    f.write(json.dumps(rec) + "\n")
                    n += 1
    return n


def suite_records(d, out):
    dirs = [x for x in sorted(d.iterdir()) if x.is_dir() and SUITE_DIRS.match(x.name)]
    if not dirs:
        return 0
    n = 0
    with open(out / "suite-records.tsv", "w") as f, open(out / "suite-bindings.tsv", "w") as b:
        f.write("dir\tvariant\tranks\tcase\trep\tphase\tcalls\tseconds\tn_elements\tthreads\t"
                "precision\tdigest\n")
        b.write("dir\tvariant\tranks\tcase\trep\tbinding\n")
        for sd in dirs:
            for p in sorted(sd.rglob("*.jsonl")):
                m = re.match(r"(.+)\.r(\d+)\.jsonl$", p.name)
                rel = p.relative_to(sd).parts
                if not m or rel[0] in ("progs",):
                    continue
                variant = rel[0]
                np_ = rel[1][2:] if len(rel) == 3 and rel[1].startswith("np") else "1"
                for ln in p.read_text().splitlines():
                    if not ln.startswith("{"):
                        continue
                    o = json.loads(ln)
                    f.write("\t".join(str(x) for x in (
                        sd.name, variant, np_, o.get("case"), m.group(2), o.get("phase"),
                        o.get("calls"), o.get("seconds"), o.get("n_elements"),
                        o.get("threads"), o.get("precision", ""), o.get("digest"))) + "\n")
                    n += 1
                bt = p.with_name(p.name[:-len(".jsonl")] + ".bindings.txt")
                if bt.exists():
                    b.write("\t".join((sd.name, variant, np_, m.group(1), m.group(2),
                                       bt.read_text().strip().replace("\n", "; "))) + "\n")
            sk = sd / ".skipped"
            if sk.exists():
                shutil.copy(sk, out / f"{sd.name}.skipped.txt")
    return n


def main(argv):
    if len(argv) != 3:
        print(__doc__)
        return 2
    d, out = Path(argv[1]), Path(argv[2])
    out.mkdir(parents=True, exist_ok=True)
    for name in COPY:
        if (d / name).exists():
            shutil.copy(d / name, out / name)
    if (d / "rounds").is_dir():
        (out / "rounds").mkdir(exist_ok=True)
        for p in sorted((d / "rounds").glob("r*.jsonl")):
            shutil.copy(p, out / "rounds" / p.name)
    for name in ("separate.tsv", "calibrated.txt", "perf-lines.txt"):
        p = d / "mpilayer" / name
        if p.exists():
            (out / "mpilayer").mkdir(exist_ok=True)
            shutil.copy(p, out / "mpilayer" / name)
    nr = cohesive_runs(d, out)
    ns = suite_records(d, out)
    print(f"extract.py: {out}: {nr} cohesive runs, {ns} suite records")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
