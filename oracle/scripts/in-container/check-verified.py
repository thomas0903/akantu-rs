#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, the Rust port of Akantu (EPFL/LSMS). Oracle tooling; the
# comparison reproduces the intent of upstream cmake/akantu_test_driver.sh.
"""Compare test outputs with their registered `.verified` references.

Upstream's cmake/akantu_test_driver.sh exits (`if $directory; then exit 0`)
before comparing a test's output with its registered `.verified` reference
(UD-018), so ctest never checks them. This script performs the comparison the
driver intended, `diff -biwI '([0-9]\\+ ms\\( total\\)\\?)' <output> <reference>`,
for every reference registered in one oracle configuration.

Modes
  rerun (default)  every test is executed afresh by this script in a scratch
                   copy of its work directory (build tree may be read-only);
                   stdout, stderr and the exit code are kept under --out and
                   hashed in the report. Gtest cases sharing one reference
                   (the reference concatenates all cases) run the binary once
                   without --gtest_filter.
  ctest-run DIR    use the `.lastout` captures of the ctest run in DIR
                   (default /build/logs/<config>/ctest/latest): the test must
                   have passed in that run's summary.json and the capture's
                   mtime must lie inside the run's [StartTestTime,
                   EndTestTime] window; gtest groups are still re-run.

Statuses (each reference gets exactly one)
  match              program exited 0 and diff found no difference
  differ             program exited 0 and diff reported differences
  execution_failed   non-zero exit, signal or timeout (diff still recorded)
  missing_output     no output capture exists
  stale_output       capture not produced by the selected ctest run
  diff_error         diff itself failed (exit >= 2, e.g. missing reference)
Exit status: 0 only if every reference has status `match`, else 1.

Usage: check-verified.py <config> [--mode rerun|ctest-run] [--ctest-run DIR]
                         [--out DIR] > report.json
       check-verified.py --self-test
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
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
IGNORE = r"([0-9]\+ ms\( total\)\?)"
STATUSES = ("match", "differ", "execution_failed", "missing_output", "stale_output",
            "diff_error")


def _load(mod, fn):
    spec = importlib.util.spec_from_file_location(mod, os.path.join(HERE, fn))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


iso = _load("isolated_runs", "isolated-runs.py")


def sha256(p):
    try:
        return iso.sha256_file(p)
    except OSError:
        return None


def tail(p, n=15):
    try:
        with open(p, errors="replace") as f:
            return [ln.rstrip("\n") for ln in f.readlines()[-n:]]
    except OSError:
        return None


def groups_of(tests):
    groups = {}
    for t in tests:
        c = t.get("command", [])
        if "-r" in c:
            groups.setdefault(c[c.index("-r") + 1], []).append(t)
    return groups


def run_fresh(cmd, workdir, build_root, scratch, out, stem, env, timeout):
    """Execute cmd in a scratch mirror of workdir; returns execution record."""
    iso_root = os.path.join(scratch, stem)
    shutil.rmtree(iso_root, ignore_errors=True)
    iso_wd, _ = iso.mirror(build_root, workdir, iso_root)
    o, e = os.path.join(out, stem + ".stdout"), os.path.join(out, stem + ".stderr")
    rec = {"command": cmd, "isolated_workdir": iso_wd, "stdout": o, "stderr": e}
    with open(o, "wb") as fo, open(e, "wb") as fe:
        p = subprocess.Popen(cmd, cwd=iso_wd, env=env, stdout=fo, stderr=fe,
                             start_new_session=True)
        try:
            rec["returncode"] = p.wait(timeout=timeout)
            rec["timed_out"] = False
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            p.wait()
            rec["returncode"], rec["timed_out"] = None, True
    shutil.rmtree(iso_root, ignore_errors=True)
    rec["stdout_sha256"] = sha256(o)
    rec["stderr_tail"] = tail(e)
    return rec


def ctest_window(run_dir):
    try:
        root = ET.parse(os.path.join(run_dir, "Test.xml")).getroot()
        start = float(root.findtext(".//StartTestTime"))
        end = float(root.findtext(".//EndTestTime"))
        return start, end
    except (OSError, ET.ParseError, TypeError, ValueError):
        return None


def check(cfg, tests, build_root, src_root, mode, run_dir, out, scratch, env, timeout):
    results = []
    statuses = {}
    window = None
    if mode == "ctest-run":
        try:
            statuses = {t["name"]: t["status"] for t in
                        json.load(open(os.path.join(run_dir, "summary.json")))["tests"]}
        except (OSError, ValueError, KeyError):
            statuses = {}
        window = ctest_window(run_dir)
    for ref, ts in sorted(groups_of(tests).items()):
        c = ts[0]["command"]
        opts, targs = iso.parse_driver(c)
        wd, name, exe = opts["-w"], opts["-n"], opts["-e"]
        is_gtest = any(a.startswith("--gtest_filter") for a in targs)
        rec = {"reference": os.path.relpath(ref, src_root) if src_root else ref,
               "reference_sha256": sha256(ref), "tests": [t["name"] for t in ts],
               "executable_sha256": sha256(exe if os.path.isabs(exe)
                                           else os.path.join(wd, exe))}
        status = None
        if is_gtest or mode == "rerun":
            prog = ([opts["-P"]] if "-P" in opts else []) + [exe]
            if not is_gtest:
                prog += targs
            rec["mode"] = "fresh run" + (", gtest binary unfiltered (all cases)"
                                         if is_gtest else "")
            ex = run_fresh(prog, wd, build_root, scratch, out,
                           re.sub(r"[^A-Za-z0-9_.-]", "_", name), env, timeout)
            rec["execution"] = ex
            outp = ex["stdout"]
            if ex["timed_out"] or ex["returncode"] != 0:
                status = "execution_failed"
        else:
            rec["mode"] = "ctest-run capture"
            outp = os.path.join(wd, name + ".lastout")
            rec["capture"] = outp
            rec["capture_sha256"] = sha256(outp)
            rec["lasterr_tail"] = tail(os.path.join(wd, name + ".lasterr"))
            st = [statuses.get(t["name"]) for t in ts]
            rec["ctest_status"] = st
            if not os.path.exists(outp):
                status = "missing_output"
            elif window is None or not (window[0] <= os.stat(outp).st_mtime <= window[1]):
                status = "stale_output"
                rec["capture_mtime"] = os.stat(outp).st_mtime
                rec["run_window"] = window
            elif any(s != "passed" for s in st):
                status = "execution_failed"
        if os.path.exists(outp):
            r = subprocess.run(["diff", "-biwI", IGNORE, outp, ref],
                               capture_output=True, text=True, errors="replace")
            diff_lines = [ln for ln in r.stdout.splitlines() if ln[:1] in "<>"]
            rec["diff"] = {"returncode": r.returncode, "differing_lines": len(diff_lines),
                           "first_differences": diff_lines[:6],
                           "stderr": r.stderr.strip()[:500]}
            if status is None:
                status = {0: "match", 1: "differ"}.get(r.returncode, "diff_error")
        elif status is None:
            status = "missing_output"
        rec["status"] = status
        results.append(rec)
    return results


def report(cfg, mode, results, provenance):
    return {
        "tool": "check-verified.py v2 (review V10 OB-05)",
        "config": cfg, "mode": mode, "provenance": provenance,
        "references": len(results),
        "tests_covered": sum(len(r["tests"]) for r in results),
        "by_status": {s: sum(r["status"] == s for r in results) for s in STATUSES},
        "all_match": all(r["status"] == "match" for r in results),
        "not_matching": [r["reference"] for r in results if r["status"] != "match"],
        "results": results,
    }


def require_gnu_diff():
    """The ignore pattern uses GNU BRE extensions (\\+, \\?): GNU diff only."""
    try:
        v = subprocess.run(["diff", "--version"], capture_output=True, text=True).stdout
    except OSError:
        v = ""
    if "GNU diffutils" not in v:
        sys.exit("check-verified.py needs GNU diff (run it inside the oracle image)")


def self_test():
    require_gnu_diff()
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        b, src, out, scratch = (os.path.join(tmp, x) for x in ("build", "src", "out", "scr"))
        for d in (b, src, out, scratch):
            os.makedirs(d)
        wd = os.path.join(b, "t")
        os.makedirs(wd)

        def prog(nm, body):
            p = os.path.join(wd, nm)
            with open(p, "w") as f:
                f.write("#!/bin/sh\n" + body + "\n")
            os.chmod(p, 0o755)

        def ref(nm, text):
            p = os.path.join(src, nm + ".verified")
            if text is not None:
                with open(p, "w") as f:
                    f.write(text)
            return p
        prog("ok", 'echo "value 1"; echo "Elapsed (12 ms)"')
        prog("bad", 'echo "value 2"')
        prog("crash", 'echo "value 1"; echo boom >&2; exit 3')
        prog("noref", 'echo "value 1"')
        prog("gt", 'case "$1" in --gtest_filter*) echo filtered;; *) echo "all cases";; esac')
        expect_rerun = {"ok": "match", "bad": "differ", "crash": "execution_failed",
                        "noref": "diff_error", "gt": "match"}
        tests = []
        for nm in expect_rerun:
            r = ref(nm, None if nm == "noref" else
                    ("all cases\n" if nm == "gt" else "value 1\nElapsed (3 ms)\n"))
            args = ["--gtest_filter=A/0.B"] if nm == "gt" else []
            tests.append({"name": nm, "command": ["drv", "-n", nm, "-w", wd, "-r", r,
                                                  "-e", "./" + nm] + args})
        env = dict(os.environ)
        res = check("t", tests, b, src, "rerun", None, out, scratch, env, 30)
        got = {r["tests"][0]: r["status"] for r in res}
        if got != expect_rerun:
            fails.append(f"rerun statuses {got}")
        crash = next(r for r in res if r["tests"][0] == "crash")
        if crash["execution"]["returncode"] != 3 or crash["execution"]["stderr_tail"] != ["boom"]:
            fails.append(f"crash record {crash['execution']}")
        if crash["diff"]["returncode"] != 0:
            fails.append("crash: matching stdout should still be diffed and recorded")
        rep = report("t", "rerun", res, {})
        if rep["all_match"] or rep["by_status"]["match"] != 2:
            fails.append(f"report {rep['by_status']}")
        # ctest-run mode: captures, statuses and the run window.
        run = os.path.join(tmp, "run")
        os.makedirs(run)
        with open(os.path.join(run, "Test.xml"), "w") as f:
            f.write("<Site><Testing><StartTestTime>1000</StartTestTime>"
                    "<EndTestTime>2000</EndTestTime></Testing></Site>")
        with open(os.path.join(run, "summary.json"), "w") as f:
            json.dump({"tests": [{"name": "ok", "status": "passed"},
                                 {"name": "bad", "status": "passed"},
                                 {"name": "crash", "status": "failed"},
                                 {"name": "noref", "status": "passed"}]}, f)
        caps = {"ok": ("value 1\n", 1500), "bad": ("value 2\n", 1500),
                "crash": ("value 1\n", 1500), "noref": ("value 1\n", 2500)}
        for nm, (txt, mt) in caps.items():
            p = os.path.join(wd, nm + ".lastout")
            with open(p, "w") as f:
                f.write(txt)
            os.utime(p, (mt, mt))
        tests2 = [t for t in tests if t["name"] != "gt"]
        tests2.append({"name": "missing", "command": ["drv", "-n", "missing", "-w", wd, "-r",
                                                      ref("missing", "x\n"), "-e", "./ok"]})
        res2 = check("t", tests2, b, src, "ctest-run", run, out, scratch, env, 30)
        got2 = {r["tests"][0]: r["status"] for r in res2}
        exp2 = {"ok": "match", "bad": "differ", "crash": "execution_failed",
                "noref": "stale_output", "missing": "missing_output"}
        if got2 != exp2:
            fails.append(f"ctest-run statuses {got2}")
    for f in fails:
        print("FAIL:", f)
    print(f"check-verified self-test: {len(fails)} failures")
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("config", nargs="?")
    ap.add_argument("--mode", choices=("rerun", "ctest-run"), default="rerun")
    ap.add_argument("--ctest-run")
    ap.add_argument("--tests-json")
    ap.add_argument("--out", default=None)
    ap.add_argument("--scratch", default="/tmp/check-verified")
    ap.add_argument("--timeout", type=float, default=900)
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        sys.exit(self_test())
    if not a.config:
        ap.error("config required")
    require_gnu_diff()
    cfg = a.config
    build_root, src_root = f"/build/{cfg}", f"/build/src-{cfg}"
    run_dir = a.ctest_run or f"/build/logs/{cfg}/ctest/latest"
    run_dir = os.path.realpath(run_dir)
    tests_json = a.tests_json or os.path.join(run_dir, "tests.json")
    tests = json.load(open(tests_json))["tests"]
    out = a.out or tempfile.mkdtemp(prefix=f"verified-{cfg}-")
    os.makedirs(out, exist_ok=True)
    env = dict(os.environ)
    envfile = os.path.join(build_root, "akantu_environement.sh")
    dumped = subprocess.run(["bash", "-c", f'source "{envfile}" && env -0'],
                            capture_output=True, check=True).stdout
    env.update(dict(kv.split("=", 1) for kv in dumped.decode().split("\0") if "=" in kv))
    prov = {"started": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "image_id": os.environ.get("ORACLE_IMAGE_ID"),
            "scripts_sha": os.environ.get("ORACLE_SCRIPTS_SHA"),
            "tests_json": tests_json, "tests_json_sha256": sha256(tests_json),
            "output_dir": out}
    if a.mode == "ctest-run":
        prov["ctest_run"] = run_dir
        prov["ctest_window"] = ctest_window(run_dir)
    results = check(cfg, tests, build_root, src_root, a.mode, run_dir, out, a.scratch, env,
                    a.timeout)
    rep = report(cfg, a.mode, results, prov)
    json.dump(rep, sys.stdout, indent=1, allow_nan=False)
    print()
    print(json.dumps({k: rep[k] for k in ("config", "mode", "by_status", "not_matching")}),
          file=sys.stderr)
    sys.exit(0 if rep["all_match"] else 1)


if __name__ == "__main__":
    main()
