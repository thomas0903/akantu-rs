#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: compare the Rust MUMPS backend's solutions with the
# oracle's (upstream SparseSolverMumps, oracle/probes/mumps) on the same
# rank-partial systems. No upstream counterpart.
#
#   compare.py <exchange-dir> [--fixtures <dir>]  -> JSON on stdout
#   (--fixtures: also write the 1-rank oracle solutions, one value per line,
#   as <dir>/<sys>.p1.<tag>.x.txt for tags oracle and oracle-retry)
#
# Rust solutions: <in>/<sys>.p<P>.<tag>.x.txt (one value per line, global
# equation order); oracle solutions: <out>/<sys>.p<P>.<tag>.r<rank>.sol
# ("g x" per local-or-master equation). Values are compared as binary64
# bit patterns (exact decimal round trips on both sides).
import glob
import json
import os
import re
import struct
import sys


def bits(v):
    return struct.unpack("<q", struct.pack("<d", v))[0]


def rust(path):
    with open(path) as f:
        return [float(l) for l in f if l.strip()]


def oracle(out, name, p, tag):
    files = sorted(glob.glob(os.path.join(out, f"{name}.p{p}.{tag}.r*.sol")))
    if len(files) != p:
        return None
    vals = {}
    for fn in files:
        with open(fn) as f:
            for line in f:
                g, v = line.split()
                vals[int(g)] = float(v)
    n = max(vals) + 1 if vals else 0
    if len(vals) != n:
        return None
    return [vals[g] for g in range(n)]


def diff(a, b):
    if a is None or b is None or len(a) != len(b):
        return None
    scale = max((abs(v) for v in b), default=0.0) or 1.0
    nb = sum(1 for x, y in zip(a, b) if bits(x) == bits(y))
    mx = max((abs(x - y) for x, y in zip(a, b)), default=0.0)
    return {"bitwise": nb == len(a), "equal_entries": nb, "n": len(a),
            "max_abs": mx, "max_rel_to_max": mx / scale}


def identical(runs):
    if not runs:
        return False
    return all((diff(r, runs[0]) or {}).get("bitwise", False) for r in runs)


def main():
    x = sys.argv[1]
    inp, out = os.path.join(x, "in"), os.path.join(x, "out")
    names = sorted({re.sub(r"\.p1\.r0\.txt$", "", os.path.basename(f))
                    for f in glob.glob(os.path.join(inp, "*.p1.r0.txt"))})
    ranks = sorted({int(m.group(1)) for f in glob.glob(os.path.join(inp, "*.r0.txt"))
                    for m in [re.search(r"\.p(\d+)\.r0\.txt$", f)] if m})
    res = []
    for name in names:
        for p in ranks:
            r1 = os.path.join(inp, f"{name}.p{p}.rust.x.txt")
            r2 = os.path.join(inp, f"{name}.p{p}.rust2.x.txt")
            if not os.path.exists(r1):
                continue
            xr, xr2 = rust(r1), (rust(r2) if os.path.exists(r2) else None)
            xo, xo2 = oracle(out, name, p, "oracle"), oracle(out, name, p, "oracle2")
            # every run of each side (D-035: >= 3 identical runs for goldens)
            rruns = [rust(f) for t in ("rust", "rust2", "rust3")
                     for f in [os.path.join(inp, f"{name}.p{p}.{t}.x.txt")] if os.path.exists(f)]
            oruns = [v for t in ("oracle", "oracle2", "oracle3")
                     for v in [oracle(out, name, p, t)] if v is not None]
            same = identical
            res.append({"system": name, "ranks": p,
                        "rust_runs": len(rruns), "oracle_runs": len(oruns),
                        "rust_runs_identical": same(rruns),
                        "oracle_runs_identical": same(oruns),
                        "all_runs_rust_eq_oracle": bool(rruns and oruns) and same(rruns + oruns),
                        "rust_vs_oracle": diff(xr, xo),
                        "rust2_vs_oracle2": diff(xr2, xo2),
                        "rust_run_to_run": diff(xr2, xr),
                        "oracle_run_to_run": diff(xo2, xo),
                        "rust_vs_rust_1rank": diff(xr, rust(os.path.join(inp, f"{name}.p1.rust.x.txt")))})
    ud216 = []
    for p in ranks:
        xo = oracle(out, "elasticity_6", p, "oracle-retry")
        ref = oracle(out, "elasticity_6", p, "oracle")
        rc = os.path.join(inp, f"elasticity_6.p{p}.rust-compat.x.txt")
        rr = os.path.join(inp, f"elasticity_6.p{p}.rust-retry.x.txt")
        if xo is None or not os.path.exists(rc):
            continue
        retry_runs = [v for t in ("oracle-retry", "oracle-retry2", "oracle-retry3")
                      for v in [oracle(out, "elasticity_6", p, t)] if v is not None]
        compat_runs = [rust(f) for t in ("rust", "rust2", "rust3")
                       for f in [os.path.join(inp, f"elasticity_6.p{p}.{t}-compat.x.txt")] if os.path.exists(f)]
        ud216.append({"ranks": p,
                      "oracle_retry_runs": len(retry_runs), "rust_compat_runs": len(compat_runs),
                      "all_retry_runs_identical": bool(retry_runs and compat_runs) and identical(
                          retry_runs + compat_runs),
                      "oracle_retry_vs_oracle_no_retry": diff(xo, ref),
                      "rust_compat_vs_oracle_retry": diff(rust(rc), xo),
                      "rust_corrected_vs_oracle_no_retry": diff(rust(rr), ref)})
    prov = {}
    pf = os.path.join(out, "oracle-provenance.txt")
    if os.path.exists(pf):
        with open(pf) as f:
            prov = dict(l.rstrip("\n").split("=", 1) for l in f if "=" in l)
    if "--fixtures" in sys.argv:
        fx = sys.argv[sys.argv.index("--fixtures") + 1]
        os.makedirs(fx, exist_ok=True)
        for name in names:
            for tag in ("oracle", "oracle-retry"):
                v = oracle(out, name, 1, tag)
                if v is not None:
                    with open(os.path.join(fx, f"{name}.p1.{tag}.x.txt"), "w") as f:
                        f.writelines(f"{y!r}\n" for y in v)
    json.dump({"comparison": res, "ud216": ud216, "oracle": prov}, sys.stdout, indent=1)
    print()


if __name__ == "__main__":
    main()
