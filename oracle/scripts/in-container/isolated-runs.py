#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, the Rust port of Akantu (EPFL/LSMS). Oracle tooling; the
# command-line parsing mirrors upstream cmake/akantu_test_driver.sh.
"""Repeat selected ctest cases in isolated work directories.

ctest runs every case in the shared build tree: gtest captures are overwritten
by each case of a binary, several tests write the same CSV, and outputs are
mutable after the run. This tool runs each selected case R times, each run in
its own scratch copy of the case's working directory, and keeps immutable
per-run artefacts:

  <out>/<case>/run<k>/rank-<r>.stdout / .stderr   per-rank streams
  <out>/<case>/run<k>/rank-<r>.xml                per-rank gtest XML (gtests)
  <out>/<case>/run<k>/files/<rel>                 text files the run produced
  <out>/<case>/run<k>/MANIFEST.json               provenance: command, ranks,
        exit code, timing, environment, input hashes (meshes, material files),
        sha256 of every produced file (also of binary dumps that are not copied)

Runs of a case are then compared pairwise (run1 vs run k) with
compare-outputs.py (all files, field-level normalization). Summary JSON on
stdout and in <out>/summary.json.

The build tree must be mounted read-only: the scratch copy symlinks
executables, large files and subdirectories back into it, so an attempted
write through a symlink fails instead of modifying the oracle.

Usage (inside the oracle image; host wrapper: oracle/scripts/run-isolated.sh):
  isolated-runs.py --config strict --names-file F --repeats 3 --out /out/<stamp>
  isolated-runs.py --self-test
Seeds: Akantu seeds its RandomGenerator from time(nullptr) unless AKA_SEED,
--aka_seed or an input-file `seed` is given (src/common/aka_common.cc:105-117);
by default AKA_SEED is left unset, as under ctest (recorded in the manifest);
--aka-seed N sets it.
"""
import argparse
import datetime
import hashlib
import importlib.util
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("compare_outputs",
                                               os.path.join(HERE, "compare-outputs.py"))
co = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(co)

DRIVER_OPTS_WITH_VALUE = {"-e", "-E", "-n", "-N", "-p", "-r", "-s", "-w", "-v", "-P"}
# Subdirectories created empty (writable) instead of symlinked: dump/output
# directories (names starting with "paraview", e.g. paraview-sum1d-0) and
# Python byte-code caches. Everything else is a read-only symlink.
WRITABLE_DIRS = {"__pycache__", "dumps", "output"}
BIG = 8 << 20          # files above this size are symlinked, not copied
COPY_MAX = 4 << 20     # produced files above this size are hashed only
TEXT_SUFFIXES = (".csv", ".txt", ".dat", ".out", ".lastout", ".lasterr", ".log", ".json")
INPUT_SUFFIXES = (".msh", ".dat", ".geo", ".csv", ".inp", ".txt", ".py", ".verified")


def parse_driver(cmd):
    """Split an akantu_test_driver.sh command into options and test args."""
    opts, i = {}, 1
    while i < len(cmd):
        a = cmd[i]
        if a in DRIVER_OPTS_WITH_VALUE and i + 1 < len(cmd):
            opts[a] = cmd[i + 1]
            i += 2
        elif a == "--":
            i += 1
            break
        else:
            break
    return opts, cmd[i:]


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for blk in iter(lambda: f.read(1 << 20), b""):
            h.update(blk)
    return h.hexdigest()


def mirror(build_root, workdir, iso_root):
    """Scratch copy of `workdir` under iso_root with the same relative layout.

    Siblings along the path are symlinks (relative mesh paths such as
    ../patch_tests/data keep working); in the work directory itself small
    regular files are copied, executables/large files/subdirectories are
    symlinked, output directories are created empty, and stale driver
    captures are left out. Returns (iso_workdir, inputs)."""
    rel = os.path.relpath(workdir, build_root)
    if rel.startswith(".."):
        raise ValueError(f"{workdir} is not inside {build_root}")
    src, dst = build_root, iso_root
    os.makedirs(dst, exist_ok=False)
    for part in ([] if rel == "." else rel.split(os.sep)):
        for child in os.listdir(src):
            if child != part:
                os.symlink(os.path.join(src, child), os.path.join(dst, child))
        src, dst = os.path.join(src, part), os.path.join(dst, part)
        os.mkdir(dst)
    inputs = {}
    for child in sorted(os.listdir(src)):
        sp, dp = os.path.join(src, child), os.path.join(dst, child)
        if os.path.isdir(sp) and not os.path.islink(sp):
            if child in WRITABLE_DIRS or child.startswith("paraview"):
                os.mkdir(dp)
            else:
                os.symlink(sp, dp)
            continue
        if child.endswith((".lastout", ".lasterr")) or child == "out":
            continue  # captures of earlier ctest runs
        st = os.stat(sp)
        if st.st_mode & 0o111 or st.st_size > BIG or ".so" in child:
            os.symlink(sp, dp)
        else:
            shutil.copy2(sp, dp)
        if child.endswith(INPUT_SUFFIXES):
            inputs[child] = sha256_file(sp)
    return dst, inputs


def snapshot(d):
    """rel -> (size, mtime_ns) of the regular files under d (no symlinks)."""
    out = {}
    for root, dirs, files in os.walk(d):
        dirs[:] = [x for x in dirs if not os.path.islink(os.path.join(root, x))]
        for fn in files:
            p = os.path.join(root, fn)
            if os.path.islink(p):
                continue
            st = os.stat(p)
            out[os.path.relpath(p, d)] = (st.st_size, st.st_mtime_ns)
    return out


def gtest_results(xml_path):
    try:
        root = ET.parse(xml_path).getroot()
    except (OSError, ET.ParseError) as e:
        return {"error": f"{type(e).__name__}: {e}"}
    cases = []
    for tc in root.iter("testcase"):
        fails = [f.get("message", "")[:400] for f in tc.findall("failure")]
        cases.append({"name": f'{tc.get("classname")}.{tc.get("name")}',
                      "result": tc.get("result"), "status": tc.get("status"),
                      "skipped": tc.find("skipped") is not None,
                      "failures": fails})
    return {"tests": int(root.get("tests", 0)), "failures": int(root.get("failures", 0)),
            "errors": int(root.get("errors", 0)), "cases": cases}


def case_dir_name(name):
    return re.sub(r"[^A-Za-z0-9_.+-]", "_", name)


def run_once(test, k, args, out_root, env_base):
    opts, targs = parse_driver(test["command"])
    name, exe = opts["-n"], opts["-e"]
    workdir = opts.get("-w") or test.get("workdir")
    ranks = int(opts.get("-N", 2)) if "-p" in opts else 1
    is_gtest = any(a.startswith("--gtest_filter") for a in targs)
    case = case_dir_name(test["name"])
    out = os.path.join(out_root, case, f"run{k}")
    os.makedirs(out)
    iso_root = os.path.join(args.scratch, case, f"run{k}")
    iso_wd, inputs = mirror(args.build_root, workdir, iso_root)
    before = snapshot(iso_wd)
    env = dict(env_base)
    if args.aka_seed is not None:
        env["AKA_SEED"] = str(args.aka_seed)
    else:
        env.pop("AKA_SEED", None)
    prog = ([opts["-P"]] if "-P" in opts else []) + [exe] + targs
    if is_gtest:
        prog.append(f"--gtest_output=xml:{out}/rank-RANK.xml")
    # Per-rank redirection; RANK is substituted by the shell of each rank.
    wrapper = ('r="${OMPI_COMM_WORLD_RANK:-0}"; out="$1"; shift; '
               'args=(); for a in "$@"; do args+=("${a//rank-RANK/rank-$r}"); done; '
               'exec "${args[@]}" > "$out/rank-$r.stdout" 2> "$out/rank-$r.stderr"')
    shell = ["bash", "-c", wrapper, "rank-wrapper", out] + prog
    if ranks > 1:
        cmd = ["mpiexec", "--oversubscribe", "--bind-to", "none", "-n", str(ranks)] + shell
    else:
        cmd = shell
    t0 = time.time()
    timed_out = False
    p = subprocess.Popen(cmd, cwd=iso_wd, env=env, start_new_session=True,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    try:
        launcher_out, _ = p.communicate(timeout=args.timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(p.pid, signal.SIGKILL)
        launcher_out, _ = p.communicate()
    t1 = time.time()
    with open(os.path.join(out, "launcher.log"), "wb") as f:
        f.write(launcher_out)
    after = snapshot(iso_wd)
    produced = {}
    for rel, meta in sorted(after.items()):
        if before.get(rel) == meta:
            continue
        p_ = os.path.join(iso_wd, rel)
        rec = {"sha256": sha256_file(p_), "size": meta[0], "produced_in_run": True}
        if meta[0] <= COPY_MAX and rel.endswith(TEXT_SUFFIXES):
            dst = os.path.join(out, "files", rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copy2(p_, dst)
            rec["copied"] = f"files/{rel}"
        produced[rel] = rec
    gtest = {}
    if is_gtest:
        for r in range(ranks):
            gtest[str(r)] = gtest_results(os.path.join(out, f"rank-{r}.xml"))
    files = {}
    for fn in sorted(os.listdir(out)):
        p_ = os.path.join(out, fn)
        if os.path.isfile(p_):
            files[fn] = {"sha256": sha256_file(p_), "size": os.path.getsize(p_),
                         "produced_in_run": True}
    for rel, rec in produced.items():
        if "copied" in rec:
            files[rec["copied"]] = {"sha256": rec["sha256"], "size": rec["size"],
                                    "produced_in_run": True}
    man = {
        "test": test["name"], "case_dir": case, "run": k, "config": args.config,
        "driver_name": name, "command": cmd, "ctest_command": test["command"],
        "source_workdir": workdir, "isolated_workdir": iso_wd, "ranks": ranks,
        "kind": "gtest" if is_gtest else ("python" if "-P" in opts else "plain"),
        "returncode": None if timed_out else p.returncode, "timed_out": timed_out,
        "start_epoch": t0, "end_epoch": t1, "wall_s": round(t1 - t0, 3),
        "env": {k_: env.get(k_) for k_ in ("AKA_SEED", "OPENBLAS_NUM_THREADS",
                                            "OMP_NUM_THREADS", "OMPI_MCA_btl",
                                            "OMPI_MCA_hwloc_base_binding_policy")},
        "inputs_sha256": inputs, "produced": produced, "gtest": gtest,
        "provenance": args.provenance, "files": files,
    }
    with open(os.path.join(out, co.MANIFEST), "w") as f:
        json.dump(man, f, indent=1)
    shutil.rmtree(iso_root, ignore_errors=True)
    return man


def outcome(man):
    if man["timed_out"]:
        # Every rank wrote its gtest XML report (written when the test program
        # finishes) but the launcher did not exit: an Open MPI/PRRTE runtime
        # hang after the ranks terminated, not a long computation.
        if man["gtest"] and all("error" not in g for g in man["gtest"].values()):
            return "launcher-hang-after-ranks-finished"
        return "timeout"
    if man["gtest"]:
        bad = [r for r, g in man["gtest"].items()
               if "error" in g or g["failures"] or g["errors"]
               or any(c["result"] != "completed" for c in g["cases"])]
        if bad:
            return "gtest-failed"
        if any(c["skipped"] for g in man["gtest"].values() for c in g["cases"]):
            return "gtest-skipped" if man["returncode"] == 0 else "exit-nonzero"
    return "passed" if man["returncode"] == 0 else "exit-nonzero"


def main_runs(args):
    tests = {t["name"]: t for t in json.load(open(args.tests_json))["tests"]}
    names = []
    for line in open(args.names_file):
        line = line.split("#", 1)[0].strip()
        if line:
            names.append(line)
    missing = [n for n in names if n not in tests]
    if missing:
        sys.exit(f"unknown test names: {missing}")
    dups = sorted({n for n in names if names.count(n) > 1})
    if dups:
        sys.exit(f"duplicate test names in {args.names_file}: {dups}")
    dirs = {}
    for n in names:
        dirs.setdefault(case_dir_name(n), []).append(n)
    clash = {d: ns for d, ns in dirs.items() if len(ns) > 1}
    if clash:
        sys.exit(f"case directory name collision: {clash}")
    env_base = dict(os.environ)
    envfile = os.path.join(args.build_root, "akantu_environement.sh")
    dumped = subprocess.run(["bash", "-c", f'source "{envfile}" && env -0'],
                            capture_output=True, check=True).stdout
    env_base.update(dict(kv.split("=", 1) for kv in dumped.decode().split("\0") if "=" in kv))
    os.makedirs(args.out, exist_ok=True)
    summary = {"tool": "isolated-runs.py (review V10 OB-04)", "config": args.config,
               "repeats": args.repeats, "aka_seed": args.aka_seed,
               "names_file": args.names_file, "provenance": args.provenance,
               "started": datetime.datetime.now(datetime.timezone.utc).isoformat(),
               "cases": []}
    # Repeats are interleaved (all cases once, then again, ...) so that the
    # runs of one case start minutes apart: with AKA_SEED unset the seed is
    # time(nullptr) (one-second resolution), and back-to-back repeats would
    # usually share it.
    runs = {n: [] for n in names}
    for k in range(1, args.repeats + 1):
        for n in names:
            m = run_once(tests[n], k, args, args.out, env_base)
            runs[n].append(m)
            print(f"[run {k}/{args.repeats}] {n}: {outcome(m)} ({m['wall_s']} s)",
                  file=sys.stderr, flush=True)
    return summarize(args, names, runs, summary)


def dump_comparison(mans):
    """Compare the SHA-256 of every produced file (incl. binary dumps that are
    hashed but not copied) across the runs of one case. A difference that
    disappears when the per-rank files of each dump step (`*.proc<r>.*`) are
    compared as a multiset is a permutation of mesh parts among ranks."""
    sets = [set(m["produced"]) for m in mans]
    only = sorted(set.union(*sets) - set.intersection(*sets))
    first = mans[0]["produced"]
    differing = sorted(rel for rel in first
                       if len({m["produced"].get(rel, {}).get("sha256") for m in mans}) > 1)
    groups = {}
    for rel in first:
        groups.setdefault(re.sub(r"\.proc\d+", ".procX", rel), []).append(rel)
    perm = all(len({tuple(sorted(m["produced"][r]["sha256"] for r in g if r in m["produced"]))
                    for m in mans}) == 1 for g in groups.values())
    return {"produced_files": len(first), "identical_sha256": len(first) - len(differing),
            "differing": len(differing), "differing_examples": differing[:10],
            "only_in_some_runs": only[:10],
            "rank_permutation_only": bool(differing) and perm and not only}


def summarize(args, names, runs, summary):
    for n in names:
        mans = runs[n]
        case_dir = os.path.join(args.out, mans[0]["case_dir"])
        prefix = os.path.relpath(mans[0]["source_workdir"], args.build_root)
        comps = []
        for k in range(2, args.repeats + 1):
            res = co.compare_trees(os.path.join(case_dir, "run1"),
                                   os.path.join(case_dir, f"run{k}"), suffixes=None,
                                   rule_prefix=prefix)
            with open(os.path.join(case_dir, f"compare-run1-run{k}.json"), "w") as f:
                json.dump(res, f, indent=1, allow_nan=False)
            s = res["summary"]
            comps.append({"pair": f"run1-run{k}", "all_identical": co.all_identical(res),
                          "by_status": {kk: v for kk, v in s["by_status"].items() if v},
                          "normalized_fields_total": s["normalized_fields_total"],
                          "differing_files": [f["file"] for f in res["files"] if f["status"]
                                              not in ("identical_bytes",
                                                      "identical_after_normalization")],
                          "only_in": res["only_in_a"] + res["only_in_b"]})
        outs = [outcome(m) for m in mans]
        rec = {"test": n, "ranks": mans[0]["ranks"], "kind": mans[0]["kind"],
               "source_workdir": mans[0]["source_workdir"],
               "outcomes": outs, "returncodes": [m["returncode"] for m in mans],
               "start_epochs": [round(m["start_epoch"], 3) for m in mans],
               "wall_s": [m["wall_s"] for m in mans],
               "inputs_sha256": mans[0]["inputs_sha256"],
               "produced_files": sorted(mans[0]["produced"]),
               "manifest_sha256": [sha256_file(os.path.join(case_dir, f"run{m['run']}",
                                                            co.MANIFEST)) for m in mans],
               "comparisons": comps,
               "repeatable": len(set(outs)) == 1 and all(c["all_identical"] for c in comps)}
        dc = dump_comparison(mans)
        rec["produced_files_comparison"] = dc
        rec["dumps_identical"] = dc["differing"] == 0 and not dc["only_in_some_runs"]
        rec["repeatable_including_dumps"] = rec["repeatable"] and rec["dumps_identical"]
        if mans[0]["gtest"]:
            rec["gtest_failure_messages_run1"] = sorted({
                fm for g in mans[0]["gtest"].values() for c in g.get("cases", [])
                for fm in c["failures"]})[:6]
        summary["cases"].append(rec)
        print(f'{n}: {outs} repeatable={rec["repeatable"]}', file=sys.stderr, flush=True)
    cs = summary["cases"]
    summary["totals"] = {
        "cases": len(cs), "runs": len(cs) * args.repeats,
        "repeatable": sum(c["repeatable"] for c in cs),
        "not_repeatable": [c["test"] for c in cs if not c["repeatable"]],
        "by_outcome_run1": {o: sum(c["outcomes"][0] == o for c in cs)
                            for o in sorted({c["outcomes"][0] for c in cs})},
        "inconsistent_outcomes": [c["test"] for c in cs if len(set(c["outcomes"])) > 1],
        "repeatable_including_dumps": sum(c["repeatable_including_dumps"] for c in cs),
        "dumps_differ": [c["test"] for c in cs if not c["dumps_identical"]],
        "dumps_differ_rank_permutation_only": [
            c["test"] for c in cs if c["produced_files_comparison"]["rank_permutation_only"]],
        "definition": "repeatable = same outcome in every run and every captured stream/"
                      "copied text file identical (bytes or after counted normalization); "
                      "repeatable_including_dumps additionally requires identical SHA-256 of "
                      "every produced file (VTU/MTX dumps are hashed, not copied)",
    }
    summary.setdefault("finished",
                       datetime.datetime.now(datetime.timezone.utc).isoformat())
    with open(os.path.join(args.out, "summary.json"), "w") as f:
        json.dump(summary, f, indent=1, allow_nan=False)
    json.dump(summary, sys.stdout, indent=1, allow_nan=False)
    print()
    return 0 if not summary["totals"]["not_repeatable"] else 1


def resummarize(args):
    """Recompute comparisons and summary.json from the artefacts in args.out
    (run manifests are not modified)."""
    old = json.load(open(os.path.join(args.out, "summary.json")))
    names = [c["test"] for c in old["cases"]]
    runs = {}
    for n in names:
        cd = os.path.join(args.out, case_dir_name(n))
        runs[n] = [json.load(open(os.path.join(cd, f"run{k}", co.MANIFEST)))
                   for k in range(1, old["repeats"] + 1)]
    args.repeats = old["repeats"]
    args.config = old["config"]
    args.build_root = args.build_root or f"/build/{old['config']}"
    summary = {k: v for k, v in old.items() if k not in ("cases", "totals")}
    summary["resummarized"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    summary["cases"] = []
    return summarize(args, names, runs, summary)


def self_test():
    """Driver parsing, mirroring and produced-file detection on a fake tree."""
    fails = []
    o, a = parse_driver(["drv", "-n", "t", "-p", "mpiexec -n", "-N", "4", "-w", "/w",
                         "-e", "./x", "--gtest_filter=A.B", "--x"])
    if o != {"-n": "t", "-p": "mpiexec -n", "-N": "4", "-w": "/w", "-e": "./x"} or \
            a != ["--gtest_filter=A.B", "--x"]:
        fails.append(f"parse_driver: {o} {a}")
    with tempfile.TemporaryDirectory() as tmp:
        b = os.path.join(tmp, "build")
        wd = os.path.join(b, "test", "case")
        os.makedirs(os.path.join(wd, "paraview"))
        os.makedirs(os.path.join(wd, "paraview-sum1d-0"))
        os.makedirs(os.path.join(wd, "sub"))
        os.makedirs(os.path.join(b, "test", "patch_tests", "data"))
        open(os.path.join(b, "test", "patch_tests", "data", "m.msh"), "w").write("mesh")
        open(os.path.join(wd, "mesh.msh"), "w").write("m")
        open(os.path.join(wd, "old.lastout"), "w").write("stale")
        open(os.path.join(wd, "paraview", "old.vtu"), "w").write("stale")
        exe = os.path.join(wd, "prog")
        open(exe, "w").write("#!/bin/sh\necho 1 > result.csv\ncat ../patch_tests/data/m.msh\n")
        os.chmod(exe, 0o755)
        iso, inputs = mirror(b, wd, os.path.join(tmp, "iso"))
        checks = {
            "copied input": os.path.isfile(os.path.join(iso, "mesh.msh"))
            and not os.path.islink(os.path.join(iso, "mesh.msh")),
            "exe symlinked": os.path.islink(os.path.join(iso, "prog")),
            "stale capture skipped": not os.path.exists(os.path.join(iso, "old.lastout")),
            "output dir empty": os.listdir(os.path.join(iso, "paraview")) == [],
            "paraview-* dir writable": os.path.isdir(os.path.join(iso, "paraview-sum1d-0"))
            and not os.path.islink(os.path.join(iso, "paraview-sum1d-0")),
            "subdir symlinked": os.path.islink(os.path.join(iso, "sub")),
            "sibling reachable": os.path.isfile(os.path.join(iso, "..", "patch_tests", "data",
                                                             "m.msh")),
            "input hashed": "mesh.msh" in inputs,
        }
        fails += [k for k, v in checks.items() if not v]
        before = snapshot(iso)
        r = subprocess.run(["./prog"], cwd=iso, capture_output=True, text=True)
        after = snapshot(iso)
        new = [k for k, v in after.items() if before.get(k) != v]
        if r.stdout != "mesh" or new != ["result.csv"]:
            fails.append(f"run/produced detection: {r.stdout!r} {new}")
        if os.path.exists(os.path.join(wd, "result.csv")):
            fails.append("run wrote into the source work directory")
    mk = lambda d: {"produced": {k: {"sha256": v} for k, v in d.items()}}
    a = mk({"p/x_0.proc0.vtu": "A", "p/x_0.proc1.vtu": "B", "m.mtx": "M"})
    b = mk({"p/x_0.proc0.vtu": "B", "p/x_0.proc1.vtu": "A", "m.mtx": "M"})
    c = mk({"p/x_0.proc0.vtu": "C", "p/x_0.proc1.vtu": "A", "m.mtx": "M"})
    d1, d2, d3 = dump_comparison([a, a, a]), dump_comparison([a, b, a]), dump_comparison([a, c])
    if d1["differing"] or not (d2["differing"] == 2 and d2["rank_permutation_only"]) \
            or d3["rank_permutation_only"]:
        fails.append(f"dump_comparison {d1} {d2} {d3}")
    for f in fails:
        print("FAIL:", f)
    print(f"isolated-runs self-test: {len(fails)} failures")
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--config", default="strict")
    ap.add_argument("--build-root")
    ap.add_argument("--tests-json")
    ap.add_argument("--names-file")
    ap.add_argument("--repeats", type=int, default=3)
    ap.add_argument("--timeout", type=float, default=600)
    ap.add_argument("--aka-seed", type=int)
    ap.add_argument("--out")
    ap.add_argument("--scratch", default="/tmp/isolated")
    ap.add_argument("--provenance", type=json.loads, default={},
                    help="JSON object recorded in every manifest (image id, scripts sha)")
    ap.add_argument("--resummarize", metavar="OUT",
                    help="recompute comparisons and summary.json of an existing output dir")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    if args.resummarize:
        args.out = args.resummarize
        sys.exit(resummarize(args))
    args.build_root = args.build_root or f"/build/{args.config}"
    args.tests_json = args.tests_json or f"/build/logs/{args.config}/ctest/latest/tests.json"
    if not (args.names_file and args.out):
        ap.error("--names-file and --out are required")
    sys.exit(main_runs(args))


if __name__ == "__main__":
    main()
