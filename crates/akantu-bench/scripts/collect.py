#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: collect a campaign of crates/akantu-bench/scripts/
# run-series.sh (not a port of Akantu code; table columns as
# benchmarks/cohesive-perf/scripts/analyze.py).
"""Summarise the runs of run-series.sh.

    python3 -I collect.py <campaign dir> [--elements h=count ...]

Writes `timmings_<material>_rust_h<h>.csv` (upstream Chrono format, one row
per run) and `runs.jsonl` into the campaign directory and prints one
Markdown table per (material, h): median [min, max] over the repetitions of
the per-step `solve_step` and `check_cohesive_stress` costs, TTS (sum of
the two totals), core-µs per element and step, init_full, static_solve,
peak RSS and the final cohesive count. `--elements` gives the tetrahedron
count of each mesh (default: read from summary.json).

Runs without the static preload (`"rust_static_solve": false` in
summary.json, the benchmark-only timing mode) are never mixed with upstream
runs: they form their own groups, their Chrono rows go to
`timmings_<material>_rust-no-preload_h<h>.csv` (not the upstream-format
`timmings_<material>_rust_h<h>.csv`) and their tables are labelled.
"""

import argparse
import json
import os
import re
import statistics
import sys


def chrono(path):
    lines = open(path).read().splitlines()
    head = [h.strip().strip('"') for h in lines[0].split(",")]
    vals = [v.strip() for v in lines[1].split(",")]
    return lines[0], lines[1], dict(zip(head, vals))


def max_rss_gib(path):
    m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", open(path).read())
    return int(m.group(1)) / 1024 / 1024 if m else float("nan")


def fmt(v, digits=3):
    return f"{v:.{digits}g}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("campaign")
    ap.add_argument("--elements", nargs="*", default=[])
    a = ap.parse_args()
    elements = dict(kv.split("=") for kv in a.elements)
    runs = []
    for root, _dirs, files in os.walk(a.campaign):
        if "run.json" not in files:
            continue
        meta = json.load(open(os.path.join(root, "run.json")))
        if meta["exit_status"] != 0 or "chrono.csv" not in files:
            print(f"skipping failed run {root}", file=sys.stderr)
            continue
        header, row, c = chrono(os.path.join(root, "chrono.csv"))
        summary = json.load(open(os.path.join(root, "summary.json")))
        preload = summary.get("rust_static_solve", True)
        if preload and "static_solve" not in c:
            sys.exit(f"{root}: summary says the static preload ran but chrono.csv has no static_solve")
        meta.update(chrono_header=header, chrono_row=row, chrono=c, summary=summary, preload=preload,
                    max_rss_gib=max_rss_gib(os.path.join(root, "time.txt")), dir=root)
        runs.append(meta)
    runs.sort(key=lambda r: (r["material"], float(r["h"]), not r["preload"], r["rep"]))
    with open(os.path.join(a.campaign, "runs.jsonl"), "w") as f:
        for r in runs:
            f.write(json.dumps({k: v for k, v in r.items() if k != "chrono_row"}) + "\n")
    groups = {}
    for r in runs:
        groups.setdefault((r["material"], r["h"], r["preload"]), []).append(r)
    for (material, h, preload), rs in sorted(groups.items(),
                                             key=lambda x: (x[0][0], float(x[0][1]), not x[0][2])):
        variant = "rust" if preload else "rust-no-preload"
        with open(os.path.join(a.campaign, f"timmings_{material}_{variant}_h{h}.csv"), "w") as f:
            f.write(rs[0]["chrono_header"] + "\n")
            for r in rs:
                f.write(r["chrono_row"] + "\n")
        nel = int(elements.get(h, rs[0]["summary"]["nb_regular_elements_dim3"]))

        def per_step(r, phase):
            c = r["chrono"]
            if phase not in c:
                return 0.0
            return float(c[phase]) / max(int(c[phase + " nb_rep"]), 1)

        def stat(values):
            return f"{fmt(statistics.median(values))} [{fmt(min(values))}, {fmt(max(values))}]"

        solve = [per_step(r, "solve_step") for r in rs]
        check = [per_step(r, "check_cohesive_stress") for r in rs]
        tts = [float(r["chrono"].get("solve_step", 0)) + float(r["chrono"].get("check_cohesive_stress", 0))
               for r in rs]
        core = [(s + k) * 1e6 / nel for s, k in zip(solve, check)]
        label = "" if preload else (", **no static preload** (rust_static_solve = false: "
                                    "not the upstream problem, not like for like)")
        print(f"\n**{material}, Rust, mesh h{h}{label}** ({nel:,} tetrahedra, {rs[0]['steps']} steps, "
              f"solver {rs[0]['solver']}; median [min, max] over {len(rs)} repetitions)\n")
        print("| reps | solve/step [s] | check/step [s] | TTS [s] | core-µs/el/step | init_full [s] "
              "| static_solve [s] | mem peak [GiB] | coh. el. (end) |")
        print("|---|---|---|---|---|---|---|---|---|")
        print(f"| {len(rs)} | {stat(solve)} | {stat(check) if any(check) else '—'} | "
              f"{fmt(statistics.median(tts))} | {fmt(statistics.median(core))} | "
              f"{fmt(statistics.median(float(r['chrono']['init_full']) for r in rs))} | "
              f"{fmt(statistics.median(float(r['chrono']['static_solve']) for r in rs)) if preload else 'skipped'} | "
              f"{fmt(statistics.median(r['max_rss_gib'] for r in rs))} | "
              f"{rs[0]['summary']['nb_cohesive_elements']} |")


if __name__ == "__main__":
    main()
