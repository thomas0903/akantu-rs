#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, the Rust port of Akantu (EPFL/LSMS). Oracle tooling, not
# derived from an Akantu source file.
"""Snapshot the output captures of a ctest run with a provenance manifest.

The build tree is mutable: later runs overwrite `*.lastout` captures and CSV
files. Called by ctest.sh at the end of every run, this
copies every `*.lastout` / `*.csv` of the build tree (CMakeFiles/Testing
excluded) into <out>/ and writes <out>/MANIFEST.json with, per file, its
SHA-256, size, mtime and `produced_in_run` = mtime inside [start, end]. Files
byte-identical to the reference source tree are recorded as `input_copy` and
not copied. compare-outputs.py reads the manifest and reports files not
produced by the run as `stale`.

Usage: snapshot-outputs.py --build-dir D --out O --start EPOCH --end EPOCH
                           [--source-root S] [--meta JSON]
       snapshot-outputs.py --self-test
"""
import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile

SUFFIXES = (".lastout", ".csv")
SKIP_DIRS = ("CMakeFiles", "Testing")


def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for blk in iter(lambda: f.read(1 << 20), b""):
            h.update(blk)
    return h.hexdigest()


def snapshot(build, out, start, end, source_root=None, meta=None):
    files = {}
    for d, dirs, fns in os.walk(build):
        dirs[:] = [x for x in dirs if x not in SKIP_DIRS]
        for fn in fns:
            if not fn.endswith(SUFFIXES):
                continue
            p = os.path.join(d, fn)
            rel = os.path.relpath(p, build)
            st = os.stat(p)
            rec = {"sha256": sha256(p), "size": st.st_size, "mtime": st.st_mtime,
                   "produced_in_run": start <= st.st_mtime <= end}
            src = os.path.join(source_root, rel) if source_root else None
            if src and os.path.isfile(src) and sha256(src) == rec["sha256"]:
                rec["input_copy"] = True
            else:
                dst = os.path.join(out, rel)
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                shutil.copy2(p, dst)
            files[rel] = rec
    man = dict(meta or {})
    man.update({"build_dir": build, "start_epoch": start, "end_epoch": end,
                "source_root": source_root, "files": files,
                "counts": {"files": len(files),
                           "produced_in_run": sum(f["produced_in_run"] for f in files.values()),
                           "input_copies": sum(f.get("input_copy", False)
                                               for f in files.values())}})
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "MANIFEST.json"), "w") as f:
        json.dump(man, f, indent=1)
    return man


def self_test():
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        b, s, o = (os.path.join(tmp, x) for x in ("build", "src", "out"))
        for rel, txt, mt in (("t/a.lastout", "1\n", 1500), ("t/old.csv", "2\n", 500),
                             ("t/in.csv", "3\n", 400), ("CMakeFiles/x.csv", "4\n", 1500)):
            p = os.path.join(b, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w") as f:
                f.write(txt)
            os.utime(p, (mt, mt))
        os.makedirs(os.path.join(s, "t"))
        with open(os.path.join(s, "t", "in.csv"), "w") as f:
            f.write("3\n")
        m = snapshot(b, o, 1000, 2000, s, {"config": "x"})
        fl = m["files"]
        checks = {
            "files": sorted(fl) == ["t/a.lastout", "t/in.csv", "t/old.csv"],
            "produced": fl["t/a.lastout"]["produced_in_run"]
            and not fl["t/old.csv"]["produced_in_run"],
            "input copy not copied": fl["t/in.csv"].get("input_copy")
            and not os.path.exists(os.path.join(o, "t", "in.csv")),
            "copied": os.path.isfile(os.path.join(o, "t", "a.lastout")),
            "mtime kept": os.stat(os.path.join(o, "t", "old.csv")).st_mtime == 500,
            "meta": m.get("config") == "x" and m["counts"]["produced_in_run"] == 1,
        }
        fails = [k for k, v in checks.items() if not v]
    for f in fails:
        print("FAIL:", f)
    print(f"snapshot-outputs self-test: {len(fails)} failures")
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--build-dir")
    ap.add_argument("--out")
    ap.add_argument("--start", type=float)
    ap.add_argument("--end", type=float)
    ap.add_argument("--source-root")
    ap.add_argument("--meta", type=json.loads, default={})
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        sys.exit(self_test())
    if None in (a.build_dir, a.out, a.start, a.end):
        ap.error("--build-dir, --out, --start and --end are required")
    m = snapshot(a.build_dir, a.out, a.start, a.end, a.source_root, a.meta)
    print(json.dumps(m["counts"]))


if __name__ == "__main__":
    main()
