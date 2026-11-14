#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Derived timings of collected campaigns, as Markdown tables on stdout.

    python3 -I scripts/analyze.py <campaign>... [--published]

Per (material, config, mesh, ranks), over the repetitions: median [min, max]
of the per-phase times. Chrono values are *accumulated seconds* (the driver
sums every measure; `nb_rep` is the number of measures), so the per-step cost
of a phase is `value / nb_rep`. Derived quantities:

* `solve/step`, `check/step` — seconds per explicit step on rank 0;
* `TTS` — solve_step + check_cohesive_stress (accumulated, s). Upstream
  plot.py multiplies each accumulated value by nb_rep once more; that constant
  factor does not change its log-log plots but is not a time, so it is not
  reproduced here;
* `core-µs/el/step` — (solve + check) per step × ranks / number of
  tetrahedra: the cost of one element-step in core-microseconds (constant
  under ideal strong scaling and linear complexity);
* speed-up and parallel efficiency relative to the 1-rank median;
* spread — (max − min) / median of the step time over repetitions.

`--published` adds the same quantities for the published jed CSVs
(results/published-jed/, 4 392 183 tetrahedra, 5000 steps).

Only valid, not oversubscribed runs enter the tables. Runs combined in one
table must share their measurement identity (collect.identity: mesh, steps,
seed, Scotch threads, interposer mode, extra parameters, build, image, host
CPU) and every table names the machine that produced it:
results of different machines are never combined or ratioed. A
campaign whose collection.json says `complete: false` is refused unless
--allow-incomplete; campaigns collected before collection.json existed
(baseline, full-schedule) are read as legacy data (host from their
provenance.json).
"""

import json
import os
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PERF_DIR = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import collect  # noqa: E402

ORIGINAL_TETS = 4392183  # stats of the original cube.msh (meshes/README.md)


def num(x):
    return float(x)


def mesh_stats(mesh, sha):
    """Statistics of the mesh a run used: meshes/cube-<h>.json or a
    per-machine meshes/cube-<h>.<machine>.json (gen-meshes.sh), selected by
    the run's mesh digest (a mesh generated on another architecture may have
    other bytes and counts)."""
    d = os.path.join(PERF_DIR, "meshes")
    names = sorted(n for n in os.listdir(d)
                   if n == f"cube-{mesh}.json" or (n.startswith(f"cube-{mesh}.") and n.endswith(".json")))
    stats = [collect.perfcheck.read_json(os.path.join(d, n)) for n in names]
    for s in stats:
        if sha is None or s.get("sha256") == sha:
            return s
    sys.exit(f"no statistics in meshes/ for cube-{mesh} with sha256 {sha}")


def load_campaign(campaign, allow_incomplete=False):
    d = os.path.join(PERF_DIR, "results", collect.perfcheck.check_campaign(campaign))
    report_path = os.path.join(d, "collection.json")
    legacy_host = None
    if os.path.exists(report_path):
        report = collect.perfcheck.read_json(report_path)
        if not report["complete"] and not allow_incomplete:
            sys.exit(f"{campaign}: collection incomplete ({len(report['failures'])} failures, "
                     "see collection.json); --allow-incomplete to tabulate the valid runs")
    else:
        prov = collect.perfcheck.read_json(os.path.join(d, "provenance.json"))
        legacy_host = {"machine": prov.get("host_machine"), "cpu_model": prov.get("cpu_brand"),
                       "note": "legacy campaign: host from provenance.json"}
    recs = []
    with open(os.path.join(d, "runs.jsonl")) as f:
        for line in f:
            r = json.loads(line)
            if r.get("exit_status") != 0 or not r.get("chrono"):
                continue
            if r.get("status", "valid" if r.get("valid", True) else "invalid") != "valid":
                continue  # invalid (e.g. UD-216 MUMPS retry), attempt, oversubscribed
            mesh = os.path.basename(r["mesh"])[len("cube-"):-len(".msh")]
            tets = mesh_stats(mesh, r.get("mesh_sha256"))["elements_by_type"]["tetrahedron4"]
            ident = collect.identity(r)
            host = r.get("host") or legacy_host or {}
            if legacy_host:
                ident["machine"], ident["cpu_model"] = host.get("machine"), host.get("cpu_model")
            recs.append({"source": campaign, "material": r["material"],
                         "config": r.get("variant", r["config"]),
                         "mesh": mesh, "tets": tets, "ranks": r["ranks"],
                         "chrono": r["chrono"], "summary": r.get("summary", {}),
                         "mem_peak": r.get("container_memory_peak_bytes"),
                         "max_rss_kib": r.get("max_rss_per_process_kib"),
                         "wall": r.get("wall_s"), "load": r.get("loadavg_start"),
                         "identity": json.dumps(ident, sort_keys=True),
                         "host": f"{host.get('cpu_model')} ({host.get('machine')})"})
    return recs


def load_published():
    recs = []
    d = os.path.join(PERF_DIR, "results", "published-jed")
    for name in sorted(os.listdir(d)):
        if not name.endswith(".csv"):
            continue
        material = name.split("_")[1]
        version = name.rsplit("_", 1)[1][:-4]
        with open(os.path.join(d, name)) as f:
            lines = [l for l in f if l.strip()]  # v5.0.4 elastic starts with a blank line
            header = [h.strip().strip('"') for h in lines[0].split(",")]
            for line in lines[1:]:
                if not line.strip():
                    continue
                c = dict(zip(header, [v.strip() for v in line.split(",")]))
                recs.append({"source": "jed", "material": material, "config": f"jed-{version}",
                             "mesh": "original", "tets": ORIGINAL_TETS,
                             "ranks": int(c["psize"]), "chrono": c, "summary": {},
                             "mem_peak": None, "max_rss_kib": None, "wall": None,
                             "load": None, "identity": f"jed {version} {material}",
                             "host": "EPFL jed (published)"})
    return recs


def per_step(c, phase):
    if phase not in c:
        return 0.0
    return num(c[phase]) / num(c[phase + " nb_rep"])


def med(xs):
    return statistics.median(xs)


def fmt(x, digits=3):
    if x is None:
        return "—"
    if x == 0:
        return "0"
    if abs(x) >= 100:
        return f"{x:.0f}"
    return f"{x:.{digits}g}"


def group(recs):
    g = {}
    for r in recs:
        g.setdefault((r["material"], r["config"], r["mesh"]), {}).setdefault(r["ranks"], []).append(r)
    return g


def table(recs, title):
    print(f"\n### {title}\n")
    for (material, config, mesh), by_ranks in sorted(group(recs).items(),
                                                       key=lambda kv: (kv[0][0], kv[0][1], -next(iter(kv[1].values()))[0]["tets"])):
        any_r = next(iter(by_ranks.values()))[0]
        idents = {r["identity"] for v in by_ranks.values() for r in v}
        if len(idents) > 1:
            sys.exit(f"{material}, {config}, mesh {mesh}: runs with {len(idents)} different "
                     "measurement identities (steps, seed, build, host, ...) cannot share a table")
        tets = any_r["tets"]
        steps = int(num(any_r["chrono"]["solve_step nb_rep"]))
        print(f"**{material}, {config}, mesh {mesh}** ({tets:,} tetrahedra, {steps} steps; "
              f"median [min, max] over repetitions; host: {any_r['host']})\n")
        cohesive = any("check_cohesive_stress" in r["chrono"] for v in by_ranks.values() for r in v)
        cols = ["ranks", "reps", "solve/step [s]"]
        if cohesive:
            cols += ["check/step [s]"]
        cols += ["TTS [s]", "spread", "speed-up", "efficiency", "core-µs/el/step",
                 "init_full [s]", "static_solve [s]", "dist_mesh [s]", "mem peak [GiB]"]
        if cohesive:
            cols += ["coh. el. (end)"]
        print("| " + " | ".join(cols) + " |")
        print("|" + "---|" * len(cols))
        base = None
        for n in sorted(by_ranks):
            rs = by_ranks[n]
            solve = [per_step(r["chrono"], "solve_step") for r in rs]
            check = [per_step(r["chrono"], "check_cohesive_stress") for r in rs]
            step = [a + b for a, b in zip(solve, check)]
            tts = [num(r["chrono"]["solve_step"]) + num(r["chrono"].get("check_cohesive_stress", 0))
                   for r in rs]
            m = med(tts)
            if n == min(by_ranks):
                base = (n, m)
            speedup = base[1] / m * base[0] if base else None
            eff = speedup / n if speedup else None
            spread = (max(step) - min(step)) / med(step)
            row = [str(n), str(len(rs)),
                   f"{fmt(med(solve))} [{fmt(min(solve))}, {fmt(max(solve))}]"]
            if cohesive:
                row.append(f"{fmt(med(check))} [{fmt(min(check))}, {fmt(max(check))}]")
            mem = [r["mem_peak"] for r in rs if r["mem_peak"]]
            coh = sorted({r["summary"].get("nb_cohesive_elements") for r in rs} - {None})
            row += [fmt(m), f"{spread * 100:.1f} %", fmt(speedup), f"{eff * 100:.0f} %" if eff else "—",
                    fmt(med(step) * n / tets * 1e6),
                    fmt(med([num(r["chrono"]["init_full"]) for r in rs])),
                    fmt(med([num(r["chrono"]["static_solve"]) for r in rs])),
                    fmt(med([num(r["chrono"]["dist_mesh"]) for r in rs])),
                    fmt(max(mem) / 2**30) if mem else "—"]
            if cohesive:
                row.append(", ".join(str(c) for c in coh) if coh else "—")
            print("| " + " | ".join(row) + " |")
        print()


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    recs = []
    for c in args:
        recs += load_campaign(c, "--allow-incomplete" in sys.argv)
    if recs:
        table(recs, "Measured (" + ", ".join(args) + ")")
    if "--published" in sys.argv:
        pub = [r for r in load_published() if r["ranks"] <= 72]
        table(pub, "Published jed results (results/published-jed, ranks ≤ 72)")


if __name__ == "__main__":
    main()
