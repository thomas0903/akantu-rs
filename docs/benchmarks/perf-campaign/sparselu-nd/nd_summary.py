# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): tables of the series
# written by series.sh (README of this directory).
#
#   python3 -I nd_summary.py OUT_DIR > summary.md
#
# Per case and phase: the median time of each variant (COLAMD, nested
# dissection, auto, the branch point) and the median [min, max] of the
# per-repetition ratios to COLAMD at the same thread count; the thread
# scaling of COLAMD and nested dissection against COLAMD at one thread;
# the median peak RSS of the process per variant; the digests.
import collections
import json
import os
import re
import statistics as st
import sys

PHASES = ["analysis", "factorize", "back_substitution", "solve_step", "solid_solve", "phase_solve"]
SOLID = ["static_linear_eigen", "static_newton_plastic", "implicit_dynamic", "cohesive_intrinsic_implicit"]
MODELS = ["heat_static_tri6", "heat_implicit_hex8_kvar", "structural_beam3_frame_static",
          "structural_beam3_frame_dynamic", "structural_dkt18_plate_static", "phasefield_staggered_tri3",
          "phasefield_staggered_tet4", "contact_implicit_static_quad4", "embedded_rebar_tri3",
          "embedded_rebar_tet4"]
# the runs of series.sh in order: (tag, threads, cases, variants)
RUNS = [("solid", 1, SOLID, ["base", "colamd", "nested_dissection", "auto"]),
        ("models", 1, MODELS, ["base", "colamd", "nested_dissection", "auto"])]
for t in (2, 4, 8):
    RUNS += [("solid", t, SOLID, ["colamd", "nested_dissection", "auto"]),
             ("models", t, MODELS, ["colamd", "nested_dissection", "auto"])]
RUNS += [("large", t, ["static_linear_eigen"], ["colamd", "nested_dissection"]) for t in (1, 2, 4, 8)]
SUITE = {"solid": "solid", "models": "models", "large": "solid"}
SHORT = {"base": "base", "colamd": "COLAMD", "nested_dissection": "ND", "auto": "auto"}


def load(out):
    """data[(tag, t, variant, case, rep)] = (phases dict, digest, extras)."""
    data = {}
    pat = re.compile(r"^(.*)\.r(\d+)\.jsonl$")
    for tag, t, _, variants in RUNS:
        d = os.path.join(out, "%s-t%d" % (tag, t))
        for v in variants:
            vd = os.path.join(d, v)
            if not os.path.isdir(vd):
                continue
            for f in sorted(os.listdir(vd)):
                m = pat.match(f)
                if not m:
                    continue
                ph, dig, ex = collections.defaultdict(float), None, {}
                for line in open(os.path.join(vd, f)):
                    try:
                        j = json.loads(line)
                    except ValueError:
                        continue
                    if "phase" in j:
                        ph[j["phase"]] += j.get("seconds", 0.0)
                    dig = j.get("digest", dig)
                    for k in ("nnz_lu", "off_diagonal_pivots"):
                        if k in j:
                            ex[k] = j[k]
                data[(tag, t, v, m.group(1), int(m.group(2)))] = (dict(ph), dig, ex)
    return data


def rss(out):
    """rss[(tag, t, variant, case)] = list of peak RSS (MiB), from the
    wrappers' files (lines in run order)."""
    lines = {}
    rdir = os.path.join(out, "rss")
    for f in os.listdir(rdir) if os.path.isdir(rdir) else []:
        lines[f[:-4]] = [l.split() for l in open(os.path.join(rdir, f)) if l.strip()]
    pos = collections.defaultdict(int)
    res = collections.defaultdict(list)
    reps = int(os.environ.get("REPS", "5"))
    for tag, t, cases, variants in RUNS:
        for v in variants:
            key = "%s_%s" % (v, SUITE[tag])
            for _ in range(reps * len(cases)):
                i = pos[key]
                if key not in lines or i >= len(lines[key]):
                    break
                c, kib = lines[key][i]
                pos[key] += 1
                res[(tag, t, v, c)].append(int(kib) / 1024.0)
    return res


def fmt_t(x):
    return "%.3f s" % x if x >= 1 else "%.1f ms" % (x * 1e3)


def ratios(data, tag, t, v, case, ph, ref_t=None):
    """Per-repetition ratios of variant v at t to COLAMD at ref_t (default t)."""
    rt = t if ref_t is None else ref_t
    out = []
    for r in range(50):
        a = data.get((tag, t, v, case, r))
        b = data.get((tag, rt, "colamd", case, r))
        if a and b and ph in a[0] and ph in b[0] and b[0][ph] > 0:
            out.append(a[0][ph] / b[0][ph])
    return out


def med(xs):
    return st.median(xs) if xs else float("nan")


def rng(xs):
    return "%.3f [%.3f, %.3f]" % (med(xs), min(xs), max(xs)) if xs else "–"


def main():
    out = sys.argv[1]
    data = load(out)
    mem = rss(out)
    tags = [("solid", SOLID), ("models", MODELS)]
    print("## One thread, size m\n")
    print("Ratio = variant / COLAMD per repetition, median [min, max]; peak RSS of the process, median (MiB).\n")
    print("| case | phase | COLAMD | ND | auto | base | RSS COLAMD / ND / auto | digests |")
    print("|---|---|---|---|---|---|---|---|")
    for tag, cases in tags:
        for c in cases:
            for ph in PHASES:
                col = [data[k][0][ph] for k in data if k[:4] == (tag, 1, "colamd", c) and ph in data[k][0]]
                if not col:
                    continue
                digs = {v: {data[k][1] for k in data if k[:4] == (tag, 1, v, c)} for v in ("base", "colamd", "nested_dissection", "auto")}
                same = "base = COLAMD" if digs["base"] == digs["colamd"] else "base ≠ COLAMD"
                aut = "auto = ND" if digs["auto"] == digs["nested_dissection"] else ("auto = COLAMD" if digs["auto"] == digs["colamd"] else "auto mixed")
                m = " / ".join("%.0f" % med(mem.get((tag, 1, v, c), [])) for v in ("colamd", "nested_dissection", "auto"))
                print("| %s | %s | %s | %s | %s | %s | %s | %s; %s |" % (
                    c, ph, fmt_t(med(col)),
                    rng(ratios(data, tag, 1, "nested_dissection", c, ph)),
                    rng(ratios(data, tag, 1, "auto", c, ph)),
                    rng(ratios(data, tag, 1, "base", c, ph)), m, same, aut))
    print("\n## Threads, size m\n")
    print("Ratio to COLAMD at one thread, median of the per-repetition ratios.\n")
    print("| case | phase | COLAMD 1 | COLAMD 2 | 4 | 8 | ND 1 | ND 2 | 4 | 8 | auto 8 |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    for tag, cases in tags:
        for c in cases:
            for ph in PHASES:
                col = [data[k][0][ph] for k in data if k[:4] == (tag, 1, "colamd", c) and ph in data[k][0]]
                if not col:
                    continue
                cells = [fmt_t(med(col))]
                for t in (2, 4, 8):
                    cells.append("%.3f" % med(ratios(data, tag, t, "colamd", c, ph, 1)))
                for t in (1, 2, 4, 8):
                    cells.append("%.3f" % med(ratios(data, tag, t, "nested_dissection", c, ph, 1)))
                cells.append("%.3f" % med(ratios(data, tag, 8, "auto", c, ph, 1)))
                print("| %s | %s | %s |" % (c, ph, " | ".join(cells)))
    print("\n## static_linear_eigen, size l\n")
    print("| phase | threads | COLAMD | ND | ND / COLAMD | RSS COLAMD / ND (MiB) | nnz(L+U) COLAMD / ND |")
    print("|---|---|---|---|---|---|---|")
    for t in (1, 2, 4, 8):
        for ph in ("analysis", "factorize", "back_substitution", "solve_step"):
            col = [data[k][0][ph] for k in data if k[:4] == ("large", t, "colamd", "static_linear_eigen") and ph in data[k][0]]
            nd = [data[k][0][ph] for k in data if k[:4] == ("large", t, "nested_dissection", "static_linear_eigen") and ph in data[k][0]]
            if not col or not nd:
                continue
            nn = [next((data[k][2].get("nnz_lu") for k in data if k[:4] == ("large", t, v, "static_linear_eigen")), None) for v in ("colamd", "nested_dissection")]
            m = " / ".join("%.0f" % med(mem.get(("large", t, v, "static_linear_eigen"), [])) for v in ("colamd", "nested_dissection"))
            print("| %s | %d | %s | %s | %s | %s | %s |" % (ph, t, fmt_t(med(col)), fmt_t(med(nd)),
                  rng(ratios(data, "large", t, "nested_dissection", "static_linear_eigen", ph)), m,
                  " / ".join(str(x) for x in nn)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
