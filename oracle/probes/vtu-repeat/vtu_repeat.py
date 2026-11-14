#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, the Rust port of Akantu (EPFL/LSMS). Oracle probe:
# full-precision run-to-run comparison of ParaView dumps.
"""Run ctest cases R times in isolated scratch work directories and compare
their VTU dumps array by array at full binary64 precision.

For every dump file the base64 binary DataArrays are decoded; Float64 arrays
are compared element-wise (max |a-b| / max |a| per array), other arrays for
equality. Per dump step the multiset of per-rank file hashes is also compared,
to recognise runs whose parts are only assigned to different ranks.
Usage (inside the image): vtu_repeat.py --config strict --runs 3 [--aka-seed N]
                          TEST_NAME [TEST_NAME ...]   (JSON on stdout)
"""
import argparse, base64, collections, glob, hashlib, importlib.util, json, os
import re, shutil, struct, subprocess, sys

spec = importlib.util.spec_from_file_location(
    "isolated_runs", "/oracle/scripts/in-container/isolated-runs.py")
iso = importlib.util.module_from_spec(spec)
spec.loader.exec_module(iso)
ARR = re.compile(r'<DataArray type="(\w+)"[^>]*Name="([^"]+)"[^>]*format="binary">\s*'
                 r'([A-Za-z0-9+/=]+)')


def arrays(path):
    out = {}
    for t, name, b64 in ARR.findall(open(path).read()):
        raw = base64.b64decode(b64)
        n = struct.unpack("<I", raw[:4])[0]  # iohelper: 4-byte length header
        out[name] = (t, raw[4:4 + n])
    return out


def floats(raw):
    return struct.unpack("<%dd" % (len(raw) // 8), raw[:8 * (len(raw) // 8)])


def run_case(test, k, cfg, seed, env_base, scratch):
    opts, targs = iso.parse_driver(test["command"])
    ranks = int(opts.get("-N", 2)) if "-p" in opts else 1
    root = os.path.join(scratch, re.sub(r"[^A-Za-z0-9_.+-]", "_", test["name"]), f"run{k}")
    shutil.rmtree(root, ignore_errors=True)
    wd, _ = iso.mirror(f"/build/{cfg}", opts["-w"], root)
    env = dict(env_base)
    env.pop("AKA_SEED", None)
    if seed is not None:
        env["AKA_SEED"] = str(seed)
    prog = ([opts["-P"]] if "-P" in opts else []) + [opts["-e"]] + targs
    cmd = (["mpiexec", "--oversubscribe", "--bind-to", "none", "-n", str(ranks)]
           if ranks > 1 else []) + prog
    r = subprocess.run(cmd, cwd=wd, env=env, capture_output=True, timeout=600)
    return wd, r.returncode


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", default="strict")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--aka-seed", type=int)
    ap.add_argument("tests", nargs="+")
    a = ap.parse_args()
    tests = {t["name"]: t for t in json.load(
        open(f"/build/logs/{a.config}/ctest/latest/tests.json"))["tests"]}
    envfile = f"/build/{a.config}/akantu_environement.sh"
    dumped = subprocess.run(["bash", "-c", f'source "{envfile}" && env -0'],
                            capture_output=True, check=True).stdout
    env_base = dict(os.environ)
    env_base.update(dict(kv.split("=", 1) for kv in dumped.decode().split("\0") if "=" in kv))
    report = {"probe": "vtu-repeat", "config": a.config, "runs": a.runs,
              "aka_seed": a.aka_seed, "image_id": os.environ.get("ORACLE_IMAGE_ID"),
              "cases": []}
    for name in a.tests:
        wds, rcs = [], []
        for k in range(1, a.runs + 1):
            wd, rc = run_case(tests[name], k, a.config, a.aka_seed, env_base, "/tmp/vtu-repeat")
            wds.append(wd)
            rcs.append(rc)
        files = sorted(os.path.relpath(p, wds[0])
                       for p in glob.glob(os.path.join(wds[0], "**", "*.vtu"), recursive=True))
        rec = {"test": name, "returncodes": rcs, "vtu_files": len(files),
               "byte_identical_files": [], "array_max_normwise_rel": {}, "nonfloat_array_diffs": 0,
               "permutation_of_ranks_only": {}}
        sha = lambda p: hashlib.sha256(open(p, "rb").read()).hexdigest()
        for k in range(1, a.runs):
            other = wds[k]
            same = 0
            worst = collections.defaultdict(float)
            for f in files:
                pa, pb = os.path.join(wds[0], f), os.path.join(other, f)
                if not os.path.exists(pb):
                    continue
                if sha(pa) == sha(pb):
                    same += 1
                    continue
                A, B = arrays(pa), arrays(pb)
                for n, (t, ra) in A.items():
                    tb, rb = B.get(n, (None, None))
                    if ra == rb:
                        continue
                    if t == "Float64" and tb == t and len(ra) == len(rb):
                        xa, xb = floats(ra), floats(rb)
                        sc = max((abs(x) for x in xa), default=0.0) or 1.0
                        worst[n] = max(worst[n], max(abs(x - y) for x, y in zip(xa, xb)) / sc)
                    else:
                        rec["nonfloat_array_diffs"] += 1
            pair = f"run1-run{k + 1}"
            rec["byte_identical_files"].append(same)
            rec["array_max_normwise_rel"][pair] = dict(worst)
            groups = collections.defaultdict(list)
            for f in files:
                groups[re.sub(r"\.proc\d+", ".procX", f)].append(f)
            rec["permutation_of_ranks_only"][pair] = all(
                sorted(sha(os.path.join(wds[0], f)) for f in g)
                == sorted(sha(os.path.join(other, f)) for f in g) for g in groups.values())
        report["cases"].append(rec)
    json.dump(report, sys.stdout, indent=1)
    print()


if __name__ == "__main__":
    main()
