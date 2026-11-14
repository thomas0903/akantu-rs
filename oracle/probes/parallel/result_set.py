# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: stages a "run-dependent result set" fixture (decision D-082)
# from a variation study of the parallel probe.
"""Stage every distinct upstream result of a variation study as fixtures.

  python3 -I result_set.py analyse <study_dir>... --np P --scenario NAME
  python3 -I result_set.py stage <study_dir>... --np P --scenario NAME --out STAGE
          [--min-runs N]

Each <study_dir> is the output of oracle/probes/parallel/variation-study.sh
(same probe, same oracle): one directory run<k>/stage/parallel/<scenario>/
np<P>/rank<r>/ per run; several studies are pooled (runs named
<study>/run<k>). A run's result is identified by the SHA-256 of the names and
bytes of the data files of all its ranks (rank order, sorted file names);
runs with the same digest are the same result.

`analyse` prints the distinct results with their frequencies.

`stage` writes each distinct result once, as
STAGE/parallel/<scenario>/np<P>/r<k>/rank<r>/ (manifest.json and data/),
k = 1, 2, ... in decreasing frequency (ties: first run), copied from the
first run with that result. The manifest is the probe's, with
* case_id = the new directory,
* description, generator.command naming the study and this script,
* parameters["result_set"]: the number of runs, the number of distinct
  results, this result's index, frequency, runs and digest, and the
  frequencies of all results (so every case states the whole set),
* tolerance_class "solver" for the floating-point quantities other than the
  copied node coordinates (D-082: compared with an envelope rule, see
  fixtures/parallel/<scenario>/tolerances.json).
Nothing else is changed (the data files and their checksums are the
probe's). Promote with
  python3 -I oracle/probes/python/finalize_fixtures.py fixtures --stage STAGE
which also rewrites fixtures/SHA256SUMS.

D-082 requires at least 12 runs per rank count (--min-runs, default 12).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil
import sys


def run_dirs(study: pathlib.Path) -> list[pathlib.Path]:
    runs = [p for p in study.iterdir() if p.is_dir() and p.name.startswith("run")]
    return sorted(runs, key=lambda p: int(p.name[3:]))


def case_root(run: pathlib.Path, scenario: str, np_: int) -> pathlib.Path:
    return run / "stage" / "parallel" / scenario / f"np{np_}"


def digest(run: pathlib.Path, scenario: str, np_: int) -> str:
    h = hashlib.sha256()
    base = case_root(run, scenario, np_)
    for r in range(np_):
        data = base / f"rank{r}" / "data"
        files = sorted(data.iterdir())
        if not files:
            raise SystemExit(f"{data}: no data files")
        for f in files:
            h.update(f"rank{r}/{f.name}".encode())
            h.update(f.read_bytes())
    return h.hexdigest()


def distinct(studies: list[pathlib.Path], scenario: str, np_: int):
    groups: dict[str, list[pathlib.Path]] = {}
    order: list[str] = []
    for study in studies:
        for run in run_dirs(study):
            d = digest(run, scenario, np_)
            if d not in groups:
                groups[d] = []
                order.append(d)
            groups[d].append(run)
    ranked = sorted(order, key=lambda d: (-len(groups[d]), order.index(d)))
    return [(d, groups[d]) for d in ranked], sum(len(v) for v in groups.values())


def run_name(run: pathlib.Path) -> str:
    return f"{run.parent.name}/{run.name}"


def analyse(args) -> int:
    results, runs = distinct([pathlib.Path(s) for s in args.study], args.scenario, args.np)
    print(f"{args.scenario} np{args.np}: {runs} runs, {len(results)} distinct result(s)")
    for k, (d, members) in enumerate(results, 1):
        print(f"  r{k}: {d[:16]} x{len(members)} ({', '.join(run_name(m) for m in members)})")
    return 0


def stage(args) -> int:
    results, runs = distinct([pathlib.Path(s) for s in args.study], args.scenario, args.np)
    if runs < args.min_runs:
        raise SystemExit(f"{runs} runs < {args.min_runs} (D-082)")
    out = pathlib.Path(args.out)
    freqs = [len(m) for _, m in results]
    for k, (d, members) in enumerate(results, 1):
        src = case_root(members[0], args.scenario, args.np)
        for r in range(args.np):
            case_id = f"parallel/{args.scenario}/np{args.np}/r{k}/rank{r}"
            dst = out / case_id
            if dst.exists():
                raise SystemExit(f"{dst} exists")
            dst.mkdir(parents=True)
            shutil.copytree(src / f"rank{r}" / "data", dst / "data")
            man = json.loads((src / f"rank{r}" / "manifest.json").read_text())
            old_id = man["case_id"]
            if old_id != f"parallel/{args.scenario}/np{args.np}/rank{r}":
                raise SystemExit(f"unexpected case_id {old_id}")
            man["case_id"] = case_id
            man["description"] = (
                f"{man['description']}; distinct upstream result r{k} of {len(results)} in a "
                f"{runs}-run variation study at {args.np} ranks (frequency {len(members)}/{runs}; "
                "D-082 run-dependent result set)"
            )
            man["generator"]["command"] = (
                f"oracle/probes/parallel/variation-study.sh --np {args.np} --scenarios "
                f"{args.scenario} ({len(args.study)} study/ies, {runs} runs); "
                f"oracle/probes/parallel/result_set.py stage --np {args.np} --scenario "
                f"{args.scenario}"
            )
            man["parameters"]["result_set"] = {
                "decision": "D-082",
                "runs": runs,
                "distinct_results": len(results),
                "frequencies": freqs,
                "this_result": k,
                "frequency": len(members),
                "runs_with_this_result": [run_name(m) for m in members],
                "digest_sha256": d,
                "digest_rule": "SHA-256 of rank<r>/<file name> and bytes of every data file, ranks in order, files sorted",
            }
            for q in man["quantities"]:
                if q["dtype"] == "f64" and q["name"] != "nodes/coordinates":
                    q["tolerance_class"] = "solver"
            (dst / "manifest.json").write_text(json.dumps(man, indent=2) + "\n")
    print(f"staged {len(results)} distinct result(s) x {args.np} rank(s) under {out}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("command", choices=["analyse", "stage"])
    ap.add_argument("study", nargs="+")
    ap.add_argument("--np", type=int, required=True)
    ap.add_argument("--scenario", required=True)
    ap.add_argument("--out")
    ap.add_argument("--min-runs", type=int, default=12)
    args = ap.parse_args()
    if args.command == "stage" and not args.out:
        ap.error("stage needs --out")
    return analyse(args) if args.command == "analyse" else stage(args)


if __name__ == "__main__":
    sys.exit(main())
