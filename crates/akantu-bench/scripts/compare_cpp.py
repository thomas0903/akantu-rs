#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: compare interleaved C++ harness runs
# (benchmarks/cohesive-perf, run-one.sh, or crates/akantu-bench/scripts/
# timing_ab.sh) with Rust driver runs (run-series.sh, timing_ab.sh). Not a
# port of Akantu code.
"""Per-step costs of C++ and Rust runs of the cohesive scalability test.

    python3 -I compare_cpp.py --cpp <dir> --rust <dir>
                              [--cpp-label L] [--rust-label L] [--like-for-like-only]

Every directory below `--cpp` / `--rust` holding a `chrono.csv` and a
`summary.json` (and, when present, an `exit` file containing 0) is one run;
runs are grouped by (material, mesh size) read from the path
(`.../<material>/h<h>/...`). Prints one Markdown row per group: median
[min, max] of `solve_step` and `check_cohesive_stress` per step, the core-µs
per element and step (each run's own tetrahedron count), the Rust/C++ ratios
of the medians, `init_full`, `static_solve`, the median peak RSS (GNU time
`time.txt`, when present) and the final cohesive count.

Runs are never mixed silently:
- a Rust run whose `summary.json` says `"rust_static_solve": false` (the
  benchmark-only mode without the static preload; its Chrono CSV has
  `static_solve_skipped` instead of `static_solve`) is grouped apart from
  the runs with the preload and its row is labelled "no preload": its
  per-step costs are not like for like (another state; with cohesive
  insertion usually more cohesive elements), at most upper bounds;
  `--like-for-like-only` refuses such runs instead (exit 1);
- the `regime` column says whether both sides ended with the same number
  of cohesive elements; when they differ the check and step costs belong
  to different regimes;
- a run whose summary flag and Chrono phases disagree is refused.
"""

import argparse
import json
import os
import re
import statistics
import sys


def load_run(d):
    """(chrono dict, summary dict, preload flag) of the run in `d`, or None
    for a failed run."""
    exit_file = os.path.join(d, "exit")
    if os.path.exists(exit_file) and open(exit_file).read().strip() != "0":
        print(f"skipping failed run {d}", file=sys.stderr)
        return None
    lines = open(os.path.join(d, "chrono.csv")).read().splitlines()
    head = [h.strip().strip('"') for h in lines[0].split(",")]
    vals = [float(v) for v in lines[1].split(",")]
    c = dict(zip(head, vals))
    s = json.load(open(os.path.join(d, "summary.json")))
    # C++ summaries have no such key: the C++ harness always preloads
    preload = s.get("rust_static_solve", True)
    if preload and "static_solve" not in c:
        sys.exit(f"{d}: summary says the static preload ran but chrono.csv has no static_solve")
    if not preload and "static_solve" in c and "static_solve_skipped" not in c:
        # runs written before the phase-name fix kept the phase name
        print(f"note: {d}: rust_static_solve = false run with a static_solve phase "
              "(written before the phase-name fix); labelled by its summary flag", file=sys.stderr)
    rss = None
    time_txt = os.path.join(d, "time.txt")
    if os.path.exists(time_txt):
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", open(time_txt).read())
        rss = int(m.group(1)) / 1024 if m else None
    s = dict(s, _max_rss_mib=rss)
    return c, s, preload


def runs(root):
    out = {}
    for d, _dirs, files in os.walk(root):
        if "chrono.csv" not in files or "summary.json" not in files:
            continue
        m = re.search(r"/(cohesive|elastic)/h([0-9.]+)/", d + "/")
        if not m:
            continue
        r = load_run(d)
        if r is None:
            continue
        c, s, preload = r
        out.setdefault((m.group(1), m.group(2), preload), []).append((c, s))
    return out


def per_step(c, phase):
    return c.get(phase, 0.0) / max(c.get(phase + " nb_rep", 1.0), 1.0)


def med(xs):
    return statistics.median(xs)


def stat(xs):
    return f"{med(xs) * 1e3:.3g} [{min(xs) * 1e3:.3g}, {max(xs) * 1e3:.3g}]"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cpp", required=True)
    ap.add_argument("--rust", required=True)
    ap.add_argument("--cpp-label", default="C++")
    ap.add_argument("--rust-label", default="Rust")
    ap.add_argument("--like-for-like-only", action="store_true",
                    help="refuse Rust runs without the static preload (rust_static_solve = false)")
    a = ap.parse_args()
    cpp, rust = runs(a.cpp), runs(a.rust)
    if any(not k[2] for k in cpp):
        sys.exit(f"{a.cpp}: runs without the static preload on the --cpp side")
    no_preload = sorted(k for k in rust if not k[2])
    if no_preload and a.like_for_like_only:
        sys.exit("refusing Rust runs without the static preload (rust_static_solve = false): "
                 + ", ".join(f"{m} h{h}" for m, h, _ in no_preload))
    cl, rl = a.cpp_label, a.rust_label
    print(f"| material | h | {rl} preload | regime | tets {cl} / {rl} | reps | "
          f"{cl} solve [ms/step] | {rl} solve [ms/step] | solve ratio | "
          f"{cl} check [ms/step] | {rl} check [ms/step] | check ratio | "
          f"{cl} core-µs/el/step | {rl} core-µs/el/step | {cl} init / static [s] | "
          f"{rl} init / static [s] | peak RSS {cl} / {rl} [MiB] | coh. el. {cl} / {rl} |")
    print("|" + "---|" * 18)
    keys = sorted((k for k in rust if (k[0], k[1], True) in cpp),
                  key=lambda k: (k[0], float(k[1]), not k[2]))
    for key in keys:
        material, h, preload = key
        cr, rr = cpp[(material, h, True)], rust[key]

        def series(rs, phase):
            return [per_step(c, phase) for c, _ in rs]

        def core(rs):
            return [(per_step(c, "solve_step") + per_step(c, "check_cohesive_stress")) * 1e6
                    / s["nb_regular_elements_dim3"] for c, s in rs]

        def static(rs):
            if all("static_solve" in c and s.get("rust_static_solve", True) for c, s in rs):
                return f"{med([c['static_solve'] for c, _ in rs]):.3g}"
            return "skipped"

        def rss(rs):
            v = [s["_max_rss_mib"] for _, s in rs if s.get("_max_rss_mib") is not None]
            return f"{med(v):.0f}" if v else "—"

        cs, rs_ = series(cr, "solve_step"), series(rr, "solve_step")
        ck, rk = series(cr, "check_cohesive_stress"), series(rr, "check_cohesive_stress")
        has_check = any(ck) and any(rk)
        coh_c = sorted({s["nb_cohesive_elements"] for _, s in cr})
        coh_r = sorted({s["nb_cohesive_elements"] for _, s in rr})
        regime = "same" if coh_c == coh_r else "**differs**"
        label = "yes" if preload else "**no** (upper bound, not like for like)"
        tets = f"{cr[0][1]['nb_regular_elements_dim3']:,} / {rr[0][1]['nb_regular_elements_dim3']:,}"
        print(f"| {material} | {h} | {label} | {regime} | {tets} | {len(cr)}/{len(rr)} | "
              f"{stat(cs)} | {stat(rs_)} | {med(rs_) / med(cs):.2f} | "
              f"{stat(ck) if has_check else '—'} | {stat(rk) if has_check else '—'} | "
              f"{(med(rk) / med(ck)) if has_check else float('nan'):.2f} | "
              f"{med(core(cr)):.3g} | {med(core(rr)):.3g} | "
              f"{med([c['init_full'] for c, _ in cr]):.3g} / {static(cr)} | "
              f"{med([c['init_full'] for c, _ in rr]):.3g} / {static(rr)} | "
              f"{rss(cr)} / {rss(rr)} | "
              f"{'/'.join(map(str, coh_c))} / {'/'.join(map(str, coh_r))} |")


if __name__ == "__main__":
    main()
