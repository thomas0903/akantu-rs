#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): the tables of the full-size
# runs on one machine and across hosts, from the committed records alone.
"""Print the tables of README.md and of docs/BENCHMARKS.md.

    python3 -I table.py > table.md

Reads the run folders of this directory (status.txt, summary.json,
chrono.csv), part G's compact records (../G/runs.jsonl,
../G/first-128head/runs.jsonl) and upstream's published timings
(benchmarks/cohesive-perf/results/published-jed/). Times are rank 0's Chrono
totals in seconds. init = read_mesh + dist_mesh + init_full + initial
conditions + static solve + before_step; step + check = solve_step +
check_cohesive_stress; loop = step + check + energies + boundary conditions
(the driver's loop_time_s). Every configuration ran once; ratios divide two
single runs.
"""

import csv
import json
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
TETS = 4392183
STEPS = 5000
INIT = ("read_mesh", "dist_mesh", "init_full", "initial_conditons", "static_solve", "before_step")
LOOP = ("solve_step", "check_cohesive_stress", "energies", "boundary_conditions")

# folder -> (session label, code label); the order of the tables.
SESSIONS = {
    "s1-port": ("1", "port 740a79a36"),
    "s1-cpp": ("1", "C++"),
    "s1-cpp64": ("1", "C++"),
    "s2-after": ("2", "port 6cdb1b601"),
    "s2-before": ("2", "port 740a79a36"),
    "s2-cpp": ("2", "C++"),
    "s3-final128": ("3", "port 4a8d9d38e"),
    "s3-final64": ("3", "port 4a8d9d38e"),
}
G_CODE = {"head": "port 740a79a36", "cpp": "C++"}


def status(p):
    out = {}
    for ln in p.read_text().splitlines():
        k, _, v = ln.partition("=")
        try:
            out[k] = json.loads(v)
        except ValueError:
            out[k] = v
    return out


def chrono(p):
    r = list(csv.reader(p.read_text().splitlines(), skipinitialspace=True))
    return {k: float(v) for k, v in zip(r[0], r[1])}


def run(where, session, code, ranks, hosts, ch, summary, wall, start, peak, peak_kind, threads=1):
    return {"where": where, "session": session, "code": code, "ranks": ranks, "threads": threads,
            "hosts": hosts, "ch": ch, "coh": summary["nb_cohesive_elements"], "wall": wall,
            "start": start, "peak": peak, "peak_kind": peak_kind,
            "init": sum(ch[k] for k in INIT), "sc": ch["solve_step"] + ch["check_cohesive_stress"],
            "loop": sum(ch.get(k, 0.0) for k in LOOP)}


def multi_host_runs():
    out = []
    for folder, (session, code) in SESSIONS.items():
        for d in sorted((HERE / folder).iterdir()):
            if not d.is_dir() or d.name.startswith("smoke"):
                continue
            st = status(d / "status.txt")
            peaks = [v for v in st.get("host_peak_kib", {}).values() if v]
            peak = (max(peaks) / 2**20, sum(peaks) / 2**20) if peaks else None
            out.append(run(f"{folder}/{d.name}", session, code, st["np"], st["nb_hosts"],
                           chrono(d / "chrono.csv"), json.loads((d / "summary.json").read_text()),
                           st["wall_s"], st["started"], peak, "hosts"))
    return out


def one_machine_runs():
    out = []
    for path, session in ((HERE.parent / "G" / "runs.jsonl", "G"),
                          (HERE.parent / "G" / "first-128head" / "runs.jsonl", "G, first sample")):
        for ln in path.read_text().splitlines():
            r = json.loads(ln)
            name = r["campaign"]
            variant = name.split("-r1-")[1]
            threads = int(variant.split("-t")[1]) if "-t" in variant else 1
            code = G_CODE[variant.split("-t")[0]]
            st = r["status"]
            where = f"G/{name}" if session == "G" else "G/first-128head"
            out.append(run(where, session, code, int(r["np"][2:]), 1,
                           {k: float(v) for k, v in r["chrono_rank0"].items() if " " not in k},
                           r["summary"], st["wall_s"], "", (st["memory_peak_bytes"] / 2**30,),
                           "container", threads))
    d = HERE / "one-machine-final" / "run"
    st = status(d / "status.txt")
    out.append(run("one-machine-final/run", "4", "port 4a8d9d38e", st["np"], 1, chrono(d / "chrono.csv"),
                   json.loads((d / "summary.json").read_text()), st["wall_s"], "2026-11-18T19:18:52Z",
                   (st["memory_peak_bytes"] / 2**30,), "container"))
    return out


def f(x, nd=1):
    return f"{x:,.{nd}f}"


def table(head, rows):
    lines = ["| " + " | ".join(head) + " |", "|" + "|".join("---" for _ in head) + "|"]
    lines += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(lines)


def layout(r):
    if r["hosts"] == 1:
        t = f" x {r['threads']} threads" if r["threads"] > 1 else ""
        return f"{r['ranks']}{t}, one 128-core machine"
    return f"{r['ranks']}, {r['hosts']} hosts x {r['ranks'] // r['hosts']}"


def every_run(runs):
    rows = []
    for r in runs:
        ch = r["ch"]
        if r["peak"] is None:
            peak = "—"
        elif r["peak_kind"] == "hosts":
            peak = f"{r['peak'][0]:.1f} / {r['peak'][1]:.1f} (hosts)"
        else:
            peak = f"{r['peak'][0]:.1f} (container)"
        rows.append([r["where"], r["session"], r["code"], layout(r), f(r["wall"], 0), f(r["init"]),
                     f(ch["dist_mesh"]), f(ch["static_solve"]), f(ch["solve_step"]),
                     f(ch["check_cohesive_stress"]), f(ch["energies"]), f(r["sc"]), f(r["loop"]),
                     f"{r['coh']:,}", peak])
    return table(["run", "session", "code", "ranks, placement", "wall", "init", "dist_mesh",
                  "static solve", "solve_step", "check", "energies", "step + check", "loop",
                  "cohesive elements", "peak GiB"], rows)


def pick(runs, where):
    return next(r for r in runs if r["where"] == where)


def ratio_rows(runs, pairs):
    rows = []
    for label, a, b in pairs:
        ra, rb = pick(runs, a), pick(runs, b)
        cells = [ra[k] / rb[k] for k in ("wall", "init")]
        cells += [ra["ch"][k] / rb["ch"][k] for k in ("static_solve", "solve_step",
                                                      "check_cohesive_stress", "energies")]
        cells += [ra["sc"] / rb["sc"], ra["loop"] / rb["loop"]]
        rows.append([label] + [f"{c:.3f}" for c in cells])
    return table(["ratio (single runs)", "wall", "init", "static solve", "solve_step", "check",
                  "energies", "step + check", "loop"], rows)


def jed():
    p = REPO / "benchmarks/cohesive-perf/results/published-jed/timmings_cohesive_gcc_jed_v5.0.4.csv"
    by = {}
    for row in csv.DictReader(p.read_text().splitlines(), skipinitialspace=True):
        cores = int(row["psize"])
        by.setdefault(cores, []).append(float(row["solve_step"]) + float(row["check_cohesive_stress"]))
    return {c: (len(v), statistics.median(v), min(v), max(v)) for c, v in sorted(by.items())}


def main():
    mh, om = multi_host_runs(), one_machine_runs()
    runs = om + mh
    print("<!-- generated by table.py from the committed records; do not edit -->\n")
    print("## Every run\n")
    print("Rank 0's Chrono totals in seconds; peak memory: one machine, the container's cgroup")
    print("peak; across hosts, the largest sampled sum of the ranks' VmRSS on one host / the sum")
    print("of those peaks over the hosts. Session 1: 18 Nov 04:12–06:23 UTC; 2: 14:10–15:25;")
    print("3: 18:23–18:56; 4: the one-machine run 19:18–19:37; G: part G, 17 Nov 22:35 to")
    print("18 Nov 03:38.\n")
    print(every_run(runs))
    print("\n## Loop by configuration (s)\n")
    cells = {
        "128, one 128-core machine": ("G/full-hjoss-np128-r1-head", "G/full-hjoss-np128-r1-cpp",
                                      None, "one-machine-final/run"),
        "128, four 32-core hosts (session 1)": ("s1-port/joss128x4-a1", "s1-cpp/joss128x4-a1", None, None),
        "128, four 32-core hosts (session 2)": ("s2-before/joss128x4-a1", "s2-cpp/joss128x4-a1",
                                                "s2-after/joss128x4-a1", None),
        "128, four 32-core hosts (session 3)": (None, None, None, "s3-final128/joss128x4-a1"),
        "64, one 128-core machine": ("G/full-hjoss-np64-r1-head", "G/full-hjoss-np64-r1-cpp", None, None),
        "64, two 32-core hosts (sessions 1, 3)": ("s1-port/joss64x2-a1", "s1-cpp64/joss64x2-a1",
                                                  None, "s3-final64/joss64x2-a1"),
        "32, one 128-core machine": ("G/full-hjoss-np32-r1-head", "G/full-hjoss-np32-r1-cpp", None, None),
    }
    rows = []
    for label, (before, cpp, mid, final) in cells.items():
        rows.append([label] + ["—" if w is None else f(pick(runs, w)["loop"]) for w in (before, mid, final, cpp)])
    print(table(["ranks, machines", "port 740a79a36", "port 6cdb1b601", "port 4a8d9d38e", "C++"], rows))
    print("\nThe first 128-rank sample of the port at 740a79a36 on the 128-core machine"
          f" (`G/first-128head`, not in the table): loop {f(pick(runs, 'G/first-128head')['loop'])} s.")
    print("\n## Port / C++, same configuration\n")
    print(ratio_rows(runs, [
        ("128, one machine: port 740a79a36 / C++", "G/full-hjoss-np128-r1-head", "G/full-hjoss-np128-r1-cpp"),
        ("128, one machine: port 4a8d9d38e / C++ (other session)", "one-machine-final/run",
         "G/full-hjoss-np128-r1-cpp"),
        ("64, one machine: port 740a79a36 / C++", "G/full-hjoss-np64-r1-head", "G/full-hjoss-np64-r1-cpp"),
        ("32, one machine: port 740a79a36 / C++", "G/full-hjoss-np32-r1-head", "G/full-hjoss-np32-r1-cpp"),
        ("128, four hosts, session 1: port 740a79a36 / C++", "s1-port/joss128x4-a1", "s1-cpp/joss128x4-a1"),
        ("128, four hosts, session 2: port 740a79a36 / C++", "s2-before/joss128x4-a1", "s2-cpp/joss128x4-a1"),
        ("128, four hosts, session 2: port 6cdb1b601 / C++", "s2-after/joss128x4-a1", "s2-cpp/joss128x4-a1"),
        ("128, four hosts: port 4a8d9d38e (session 3) / C++ (session 2)", "s3-final128/joss128x4-a1",
         "s2-cpp/joss128x4-a1"),
        ("128, four hosts: port 4a8d9d38e (session 3) / C++ (session 1)", "s3-final128/joss128x4-a1",
         "s1-cpp/joss128x4-a1"),
        ("64, two hosts, session 1: port 740a79a36 / C++", "s1-port/joss64x2-a1", "s1-cpp64/joss64x2-a1"),
        ("64, two hosts: port 4a8d9d38e (session 3) / C++ (session 1)", "s3-final64/joss64x2-a1",
         "s1-cpp64/joss64x2-a1"),
    ]))
    print("\n## Across hosts / one machine, same code and binary\n")
    print(ratio_rows(runs, [
        ("port 740a79a36, 128: four hosts (session 1) / one machine", "s1-port/joss128x4-a1",
         "G/full-hjoss-np128-r1-head"),
        ("port 740a79a36, 128: four hosts (session 2) / one machine", "s2-before/joss128x4-a1",
         "G/full-hjoss-np128-r1-head"),
        ("port 740a79a36, 64: two hosts / one machine", "s1-port/joss64x2-a1", "G/full-hjoss-np64-r1-head"),
        ("port 4a8d9d38e, 128: four hosts (session 3) / one machine", "s3-final128/joss128x4-a1",
         "one-machine-final/run"),
        ("C++, 128: four hosts (session 1) / one machine", "s1-cpp/joss128x4-a1", "G/full-hjoss-np128-r1-cpp"),
        ("C++, 128: four hosts (session 2) / one machine", "s2-cpp/joss128x4-a1", "G/full-hjoss-np128-r1-cpp"),
        ("C++, 64: two hosts / one machine", "s1-cpp64/joss64x2-a1", "G/full-hjoss-np64-r1-cpp"),
    ]))
    print("\n## The collective checks at commit points (D-126)\n")
    print(ratio_rows(runs, [
        ("128, four hosts, session 2: port 6cdb1b601 / port 740a79a36", "s2-after/joss128x4-a1",
         "s2-before/joss128x4-a1"),
        ("128, four hosts: port 4a8d9d38e (session 3) / port 740a79a36 (session 2)",
         "s3-final128/joss128x4-a1", "s2-before/joss128x4-a1"),
        ("64, two hosts: port 4a8d9d38e (session 3) / port 740a79a36 (session 1)", "s3-final64/joss64x2-a1",
         "s1-port/joss64x2-a1"),
        ("128, one machine: port 4a8d9d38e / port 740a79a36", "one-machine-final/run",
         "G/full-hjoss-np128-r1-head"),
        ("128, one machine: port 4a8d9d38e / port 740a79a36, first sample", "one-machine-final/run",
         "G/first-128head"),
            ]))
    print("\n## Two sessions on different hosts of the same type (128 ranks, four hosts)\n")
    print(ratio_rows(runs, [
        ("port 740a79a36: session 2 / session 1", "s2-before/joss128x4-a1", "s1-port/joss128x4-a1"),
        ("C++: session 2 / session 1", "s2-cpp/joss128x4-a1", "s1-cpp/joss128x4-a1"),
    ]))
    print("\n## Step + check against upstream's published data\n")
    print("jed: medians [min, max] of `solve_step` + `check_cohesive_stress` over the runs of the")
    print("v5.0.4 cohesive file (`timmings_cohesive_gcc_jed_v5.0.4.csv`, Intel Xeon 8360Y, 72 cores")
    print("per node). Here: single runs. core-µs/el/step = (step + check) x cores / tetrahedra / steps.\n")
    rows = []
    for c, (n, med, lo, hi) in jed().items():
        if c >= 9:
            rows.append([f"jed, {c} cores", n, f"{f(med)} [{f(lo)}, {f(hi)}]", f"{med * c / TETS / STEPS * 1e6:.2f}"])
    for w in ("G/full-hjoss-np32-r1-head", "G/full-hjoss-np64-r1-head", "G/full-hjoss-np128-r1-head",
              "one-machine-final/run", "s1-port/joss64x2-a1", "s3-final64/joss64x2-a1", "s1-port/joss128x4-a1",
              "s2-after/joss128x4-a1", "s3-final128/joss128x4-a1", "G/full-hjoss-np32-r1-cpp",
              "G/full-hjoss-np64-r1-cpp", "G/full-hjoss-np128-r1-cpp", "s1-cpp64/joss64x2-a1",
              "s1-cpp/joss128x4-a1", "s2-cpp/joss128x4-a1"):
        r = pick(runs, w)
        rows.append([f"{r['code']}, {layout(r)} (`{w}`)", 1, f(r["sc"]),
                     f"{r['sc'] * r['ranks'] * r['threads'] / TETS / STEPS * 1e6:.2f}"])
    print(table(["run", "runs", "step + check s", "core-µs/el/step"], rows))


if __name__ == "__main__":
    main()
