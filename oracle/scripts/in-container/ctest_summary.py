#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Summarise a CTest dashboard Test.xml (+ `ctest --show-only=json-v1`) into a
# compact JSON document: one record per test (name, status, duration, MPI
# ranks, kind, failure details) plus totals. Used by in-container/ctest.sh.
# Ranks come from the driver's -p/-N arguments (the
# PROCESSORS property is absent for examples registered with -N 2).
import argparse
import json
import os
import re
import xml.etree.ElementTree as ET


def load_properties(path):
    props = {}
    try:
        with open(path) as f:
            data = json.load(f)
    except (OSError, ValueError):
        return props
    for t in data.get("tests", []):
        p = {x["name"]: x["value"] for x in t.get("properties", [])}
        props[t["name"]] = {
            "processors": p.get("PROCESSORS", 1),
            "labels": p.get("LABELS", []),
            "command": t.get("command", []),
        }
    return props


def kind_of(name, cmd):
    text = " ".join(cmd) if isinstance(cmd, list) else str(cmd)
    if "--gtest_filter" in text or ".gtest" in text or "_gtest" in text:
        return "gtest"
    if re.search(r"python3?(\s|$)|\.py(\s|$)", text):
        return "python"
    if "/examples/" in text:
        return "example"
    return "test"


def ranks_of(p, meas):
    """MPI ranks a test runs on.

    akantu_test_driver.sh runs `<-p wrapper> <-N value> exe` whenever -p is
    given; the PROCESSORS property is only a scheduling hint and is missing
    for examples registered with -N (e.g. parallel_2d-test runs on 2 ranks).
    """
    cmd = p.get("command", [])
    if isinstance(cmd, list) and "-p" in cmd:
        try:
            return int(cmd[cmd.index("-N") + 1]) if "-N" in cmd else 2  # driver default
        except (ValueError, IndexError):
            pass
    return int(p.get("processors", meas.get("Processors") or 1))


def cache_value(cache, key):
    try:
        with open(cache) as f:
            for line in f:
                if line.startswith(key + ":"):
                    return line.split("=", 1)[1].rstrip("\n")
    except OSError:
        pass
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", required=True)
    ap.add_argument("--test-xml", required=True)
    ap.add_argument("--tests-json", required=True)
    ap.add_argument("--cache", required=True)
    ap.add_argument("--wall-seconds", type=int, default=0)
    ap.add_argument("--ctest-args", default="")
    ap.add_argument("--ctest-rc", type=int, default=0)
    a = ap.parse_args()

    props = load_properties(a.tests_json)
    root = ET.parse(a.test_xml).getroot()
    tests = []
    for t in root.iter("Test"):
        status = t.get("Status")
        name = t.findtext("Name")
        if status is None or name is None:
            continue  # TestList entries
        rec = {"name": name, "status": status}
        meas = {}
        for nm in t.iter("NamedMeasurement"):
            meas[nm.get("name")] = nm.findtext("Value")
        try:
            rec["duration_s"] = round(float(meas["Execution Time"]), 3)
        except (KeyError, TypeError, ValueError):
            rec["duration_s"] = None  # not run (e.g. gtest DISABLED_)
        p = props.get(name, {})
        rec["ranks"] = ranks_of(p, meas)
        rec["kind"] = kind_of(name, p.get("command", t.findtext("FullCommandLine") or ""))
        if p.get("labels"):
            rec["labels"] = p["labels"]
        if status != "passed":
            rec["completion_status"] = meas.get("Completion Status")
            rec["exit_code"] = meas.get("Exit Code")
            rec["exit_value"] = meas.get("Exit Value")
            out = t.findtext("Results/Measurement/Value") or ""
            lines = [ln for ln in out.splitlines() if ln.strip()]
            rec["output_tail"] = lines[-25:]
        tests.append(rec)

    totals = {
        "total": len(tests),
        "passed": sum(1 for t in tests if t["status"] == "passed"),
        "failed": sum(1 for t in tests if t["status"] == "failed"),
        "notrun": sum(1 for t in tests if t["status"] == "notrun"),
    }
    by_ranks = {}
    for t in tests:
        k = str(t["ranks"])
        d = by_ranks.setdefault(k, {"total": 0, "passed": 0})
        d["total"] += 1
        d["passed"] += t["status"] == "passed"
    doc = {
        "config": a.config,
        "reference_sha": "35a851780d0b9471d258ed53a003ae98aaa95ea0",
        "scripts_sha": os.environ.get("ORACLE_SCRIPTS_SHA"),
        "image_id": os.environ.get("ORACLE_IMAGE_ID"),
        "build_type": cache_value(a.cache, "CMAKE_BUILD_TYPE"),
        "cxx_flags": cache_value(a.cache, "CMAKE_CXX_FLAGS"),
        "arch": os.uname().machine,
        "site_build_stamp": root.get("BuildStamp"),
        "ctest_args": a.ctest_args.strip(),
        "ctest_rc": a.ctest_rc,
        "wall_seconds": a.wall_seconds,
        "totals": totals,
        "by_ranks": by_ranks,
        "tests": sorted(tests, key=lambda t: t["name"]),
    }
    json.dump(doc, __import__("sys").stdout, indent=1, sort_keys=False,
              allow_nan=False)
    print()


if __name__ == "__main__":
    main()
