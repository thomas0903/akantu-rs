# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): summarises the run
# directories of the visual check (what each run wrote, its exit code and the
# ends of its output) into one JSON file, so that no dump has to leave the
# sandbox. Plain python3, standard library only.
#   python3 -I runinfo.py <pvout-dir> > runinfo.json
import json
import os
import sys

root = sys.argv[1]


def tail(path, n=6, width=300):
    try:
        with open(path, errors="replace") as f:
            lines = f.read().splitlines()
    except OSError:
        return []
    return [ln[:width] for ln in lines[-n:]]


def head(path, n=4, width=300):
    try:
        with open(path, errors="replace") as f:
            lines = f.read().splitlines()
    except OSError:
        return []
    return [ln[:width] for ln in lines[:n]]


def census(d):
    ext, total = {}, 0
    for dp, _dn, fn in os.walk(d):
        for f in fn:
            e = f.rsplit(".", 1)[-1] if "." in f else ""
            ext[e] = ext.get(e, 0) + 1
            total += os.path.getsize(os.path.join(dp, f))
    return ext, total


def one(d):
    ext, total = census(d)
    rec = {"files_by_extension": ext, "bytes": total}
    for name in ("exit_code", "command.txt"):
        p = os.path.join(d, name)
        if os.path.exists(p):
            rec[name.split(".")[0]] = open(p).read().strip()
    rec["stdout_tail"] = tail(os.path.join(d, "stdout.txt"))
    rec["stderr_head"] = head(os.path.join(d, "stderr.txt"))
    return rec


out = {"port": {}, "oracle": {}, "python": {}}
for side in ("port", "oracle"):
    base = os.path.join(root, side)
    if not os.path.isdir(base):
        continue
    for name in sorted(os.listdir(base)):
        d = os.path.join(base, name)
        if os.path.isdir(d):
            out[side][name] = one(d)
    prov = os.path.join(base, "oracle-provenance.txt")
    if os.path.exists(prov):
        out["oracle_provenance"] = open(prov).read().splitlines()
base = os.path.join(root, "py-runs")
if os.path.isdir(base):
    for case in sorted(os.listdir(base)):
        out["python"][case] = {}
        for side in ("port", "upstream"):
            d = os.path.join(base, case, side)
            if os.path.isdir(d):
                rec = one(os.path.join(d, "run")) if os.path.isdir(os.path.join(d, "run")) else {}
                rec["exit_code"] = open(os.path.join(d, "exit_code")).read().strip() if os.path.exists(os.path.join(d, "exit_code")) else None
                rec["stdout_tail"] = tail(os.path.join(d, "stdout.txt"))
                rec["stderr_head"] = head(os.path.join(d, "stderr.txt"))
                st = os.path.join(d, "status.txt")
                rec["status"] = head(st, 3) if os.path.exists(st) else []
                out["python"][case][side] = rec
json.dump(out, sys.stdout, indent=1)
