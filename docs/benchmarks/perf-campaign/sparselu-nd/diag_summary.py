# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the table of the
# diagnostic runs (README of this directory): per suite case, the first
# solve's residual and condition estimate, pivots and fill of both
# orderings, and the differences of the first solution and of the digested
# state between COLAMD and nested dissection.
#
#   python3 -I diag_summary.py DIAG_DIR > summary.md
import os, re, struct, sys, glob
D = sys.argv[1]
def rd(p):
    b = open(p, "rb").read(); return struct.unpack("<%dd" % (len(b)//8), b)
def cmp(a, b):
    s = max(abs(x) for x in a) or 1.0
    d = max(abs(x-y) for x, y in zip(a, b))
    return d / s, sum(1 for x, y in zip(a, b) if x == y and struct.pack("<d", x) == struct.pack("<d", y)), len(a)
cases = sorted({os.path.basename(p).split(".")[0] for p in glob.glob(D + "/*.err")})
print("| case | n | solves | resid COLAMD / ND (first solve) | kappa1 est. | asym | off-diag pivots C / ND | nnz(L+U) C / ND | first solve: max|dx|/max|x| | state: max|d|/max|s| (exact) |")
print("|---|---|---|---|---|---|---|---|---|---|")
for c in cases:
    L = {}
    for o in ("colamd", "nested_dissection"):
        lines = [l for l in open(f"{D}/{c}.{o}.err") if l.startswith("ND_DIAG")]
        L[o] = [dict(zip(l.split()[1::2], l.split()[2::2])) for l in lines]
    if not L["colamd"] or not L["nested_dissection"]:
        print("|", c, "| no diag |"); continue
    a, b = L["colamd"][0], L["nested_dissection"][0]
    n = int(a["n"])
    xa, xb = rd(f"{D}/{c}.colamd.x"), rd(f"{D}/{c}.nested_dissection.x")
    r1 = cmp(xa[:n], xb[:n])
    sa, sb = rd(f"{D}/{c}.colamd.state"), rd(f"{D}/{c}.nested_dissection.state")
    rs = cmp(sa, sb) if len(sa) == len(sb) else (float("nan"), 0, 0)
    km = max(float(d["kappa1"]) for d in L["colamd"])
    print(f"| {c} | {n} | {len(L['colamd'])}/{len(L['nested_dissection'])} | {a['resid']} / {b['resid']} | {float(a['kappa1']):.2e} (max {km:.1e}) | {a['asym']} | {a['offdiag']} / {b['offdiag']} | {a['nnz_lu']} / {b['nnz_lu']} | {r1[0]:.2e} | {rs[0]:.2e} ({rs[1]} of {rs[2]}) |")
