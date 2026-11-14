#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: analysis of a c1prep_campaign.sh campaign.
"""Tables of a C++/Rust cohesive campaign (standard library only).

    python3 -I c1prep_analyze.py CAMPAIGN_DIR [--tets N]

CAMPAIGN_DIR/<case>/np<N>/rep<k>/ hold the run directories written by
c1prep_campaign.sh (C++: run.json, chrono.csv, summary*.json, energies.csv,
insertions.csv, partition.csv; Rust: exit, wall.txt, the same outputs) and
mem.jsonl (memsample.py). Writes CAMPAIGN_DIR/summary.md and summary.json:

* per run and as medians per (case, ranks): wall, phases (init_full,
  static_solve, ms per step of solve_step and check_cohesive_stress),
  core-µs per element and step, insertion counts, final energies, memory:
  the sampler's whole-run peaks (largest rank's VmHWM, and the largest sum
  of the ranks' VmRSS over the samples: exact up to the sampling interval,
  same method for both codes); the Rust driver's own records (VmRSS before
  the static solve = mesh + model, VmHWM at the end of the static solve,
  VmHWM of the loop after a reset, largest rank); MUMPS's INFOG memory
  figures (from the Rust run; the C++ run hands MUMPS the same system). The
  sampler's split at the static-solve boundary (columns `~static`, `~loop`)
  is approximate: the boundary is placed from the rank-0 Chrono phases and
  the process start, without the MPI start-up time;
* Rust / C++ ratios of the medians per rank count;
* energy histories compared column by column (ed, ep, ek, ew, et), relative
  difference |a - b| / max(|a|, |b|) (0 when both are 0), maximum over the
  step windows [0, 800], [801, 1500], [1501, end] and at the last step, for
  Rust vs C++ (every pair of repetitions), C++ vs C++ and Rust vs Rust (run
  to run), and at 1 rank the MUMPS vs Eigen pairs of each side; bitwise
  identity of the energies text; insertion steps compared exactly.
"""

import json
import statistics
import sys
from itertools import combinations, product
from pathlib import Path

COLS = ("ed", "ep", "ek", "ew", "et")
BOUNDARY_PHASES = ("read_mesh", "dist_mesh", "init_full", "initial_conditons", "static_solve")


def chrono(d):
    f = d / "chrono.csv"
    if not f.exists():
        return {}
    lines = f.read_text().splitlines()
    hdr = [h.strip().strip('"') for h in lines[0].split(",")]
    row = [float(v) for v in lines[1].split(",")]
    return dict(zip(hdr, row))


def status(d):
    if (d / "exit").exists():
        return int((d / "exit").read_text().strip() or -1)
    if (d / "run.json").exists():
        return int(json.loads((d / "run.json").read_text()).get("exit_status", -1))
    return -1


def wall(d):
    if (d / "wall.txt").exists():
        return float((d / "wall.txt").read_text())
    if (d / "run.json").exists():
        return float(json.loads((d / "run.json").read_text())["wall_s"])
    return None


def energies(d):
    f = d / "energies.csv"
    if not f.exists():
        return None, None
    text = [ln for ln in f.read_text().splitlines() if not ln.startswith("#")]
    rows = {}
    for ln in text[1:]:
        p = [x.strip() for x in ln.split(",")]
        rows[int(p[0])] = [float(x) for x in p[1:6]]
    return rows, "\n".join(text)


def insertion_steps(d):
    f = d / "insertions.csv"
    if not f.exists():
        return None
    out = []
    for ln in f.read_text().splitlines():
        if ln.startswith("#") or ln.startswith("step"):
            continue
        out.append(int(ln.split(",")[0]))
    return out


def memory(d, ch):
    """Sampler figures (kiB): static / loop / overall peaks, per rank max and total."""
    f = d / "mem.jsonl"
    if not f.exists():
        return {}
    samples = []
    interval = 0.25
    for ln in f.read_text().splitlines():
        r = json.loads(ln)
        if r.get("header"):
            interval = r.get("interval", interval)
            continue
        samples.append(r)
    if not samples:
        return {}
    starts = [p[1] for s in samples for p in s["procs"]]
    boundary = min(starts) + sum(ch.get(k, 0.0) for k in BOUNDARY_PHASES)
    out = {"nb_samples": len(samples), "nb_procs": len({p[0] for s in samples for p in s["procs"]})}
    pre = [s for s in samples if s["t"] <= boundary + 2 * interval]
    post = [s for s in samples if s["t"] >= boundary + 4 * interval]
    if pre:
        hwm = {}
        for s in pre:
            for p in s["procs"]:
                hwm[p[0]] = max(hwm.get(p[0], 0), p[3])
        out["static_rank_peak_max"] = max(hwm.values())
        out["static_rank_peak_sum"] = sum(hwm.values())
        out["static_total_rss_peak"] = max(sum(p[2] for p in s["procs"]) for s in pre)
    if post:
        rss = {}
        for s in post:
            for p in s["procs"]:
                rss[p[0]] = max(rss.get(p[0], 0), p[2])
        out["loop_rank_rss_peak_max"] = max(rss.values())
        out["loop_total_rss_peak"] = max(sum(p[2] for p in s["procs"]) for s in post)
    last = {}
    for s in samples:
        for p in s["procs"]:
            last[p[0]] = max(last.get(p[0], 0), p[3])
    out["run_rank_peak_max"] = max(last.values())
    out["run_rank_peak_sum"] = sum(last.values())
    out["run_total_rss_peak"] = max(sum(p[2] for p in s["procs"]) for s in samples)
    return out


def load_run(case, np, rep_dir):
    d = rep_dir
    ch = chrono(d)
    s = json.loads((d / "summary.json").read_text()) if (d / "summary.json").exists() else {}
    steps = s.get("max_steps") or 0
    e, etext = energies(d)
    run = {
        "case": case, "np": np, "rep": d.name, "dir": str(d), "status": status(d), "wall_s": wall(d),
        "chrono": ch, "summary": s, "energies": e, "energies_text": etext,
        "insertion_steps": insertion_steps(d), "mem": memory(d, ch), "steps": steps,
    }
    rj = d / "run.json"
    if rj.exists():
        r = json.loads(rj.read_text())
        run["container_peak_kib"] = (r.get("container_memory_peak_bytes") or 0) // 1024 or None
        run["max_rss_process_kib"] = r.get("max_rss_per_process_kib")
    if "rust_memory_kib" in s:
        run["rust_memory"] = s["rust_memory_kib"]
        run["rust_static"] = {"solver": s.get("rust_static_sparse_solver"),
                              "stats": s.get("rust_static_solver_stats"),
                              "matrix": s.get("rust_static_matrix")}
    p = d / "partition.csv"
    run["partition"] = p.read_text() if p.exists() else None
    return run


def rel(a, b):
    m = max(abs(a), abs(b))
    return 0.0 if m == 0 else abs(a - b) / m


def compare(e1, e2):
    """Max relative difference per window and column."""
    last = min(max(e1), max(e2))
    windows = {"0-800": (0, 800), "801-1500": (801, 1500), "1501-end": (1501, last), "last": (last, last)}
    out = {}
    for w, (a, b) in windows.items():
        if a > last:
            continue
        mx = [0.0] * 5
        for st in range(a, min(b, last) + 1):
            if st in e1 and st in e2:
                for c in range(5):
                    mx[c] = max(mx[c], rel(e1[st][c], e2[st][c]))
        out[w] = dict(zip(COLS, mx))
    return out


def merge_max(dst, src):
    for w, cols in src.items():
        d = dst.setdefault(w, {})
        for c, v in cols.items():
            d[c] = max(d.get(c, 0.0), v)


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def fmt(x, nd=3):
    if x is None:
        return "—"
    if isinstance(x, float):
        if x == 0:
            return "0"
        if abs(x) >= 1e4 or abs(x) < 1e-3:
            return f"{x:.2e}"
        return f"{x:.{nd}g}"
    return str(x)


def main(argv):
    root = Path(argv[1])
    tets = None
    if "--tets" in argv:
        tets = int(argv[argv.index("--tets") + 1])
    runs = []
    for cd in sorted(p for p in root.iterdir() if p.is_dir()):
        for npd in sorted(cd.glob("np*")):
            for rd in sorted(x for x in npd.iterdir() if x.is_dir()):
                run = load_run(cd.name, int(npd.name[2:]), rd)
                run["dir"] = str(rd.relative_to(root))  # relative to the campaign directory
                runs.append(run)
    ok = [r for r in runs if r["status"] == 0 and r["summary"]]
    if tets is None:
        tets = next((r["summary"].get("nb_regular_elements_dim3") for r in ok), None)
    md = [f"# Campaign `{root.name}`", ""]
    prov = root / "provenance.txt"
    if prov.exists():
        md += ["```", prov.read_text().strip(), "```", ""]
    failed = [r for r in runs if r not in ok]
    md.append(f"Runs: {len(runs)} ({len(ok)} complete, {len(failed)} failed: "
              + ", ".join(f"{r['case']}/np{r['np']}/{r['rep']} rc={r['status']}" for r in failed) + ")")
    md.append(f"Tetrahedra: {tets}")
    md.append("")

    def per_step(r, ph):
        v = r["chrono"].get(ph)
        n = r["steps"]
        return None if v is None or not n else 1e3 * v / n

    def core_us(r):
        s, c = per_step(r, "solve_step"), per_step(r, "check_cohesive_stress")
        if s is None or not tets:
            return None
        return (s + (c or 0.0)) * 1e3 * r["np"] / tets

    md += ["## Runs", "",
           "Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the "
           "sampling interval). `Rust model / static / loop`: the driver's own records, largest rank "
           "(VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). "
           "`~static`, `~loop`: the sampler split at the approximate boundary (largest rank).", "",
           "| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |",
           "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    mib = lambda k: None if k is None else k / 1024
    for r in sorted(ok, key=lambda r: (r["np"], r["case"], r["rep"])):
        s, m = r["summary"], r["mem"]
        self_mem = "—"
        if "rust_memory" in r:
            rm = r["rust_memory"]
            hb = rm.get("rss_before_static", {}).get("max")
            hs, he = rm.get("hwm_static", {}).get("max"), rm.get("hwm_end", {}).get("max")
            self_mem = f"{fmt(mib(hb))} / {fmt(mib(hs))} / {fmt(mib(he))}"
        md.append("| " + " | ".join([
            r["case"], str(r["np"]), r["rep"], fmt(r["wall_s"]), fmt(r["chrono"].get("init_full")),
            fmt(r["chrono"].get("static_solve")), fmt(per_step(r, "solve_step")),
            fmt(per_step(r, "check_cohesive_stress")), fmt(core_us(r)),
            str(s.get("nb_cohesive_elements")), str(s.get("nb_insertion_steps")),
            str(s.get("first_insertion_step")), f"{s.get('energy_dissipated', 0):.10g}",
            f"{s.get('energy_potential', 0):.10g}", f"{s.get('energy_kinetic', 0):.10g}",
            fmt(mib(m.get("run_rank_peak_max"))), fmt(mib(m.get("run_total_rss_peak"))),
            fmt(mib(r.get("container_peak_kib"))), self_mem,
            fmt(mib(m.get("static_rank_peak_max"))), fmt(mib(m.get("loop_rank_rss_peak_max")))]) + " |")
    md.append("")

    groups = {}
    for r in ok:
        groups.setdefault((r["case"], r["np"]), []).append(r)
    md += ["## Medians per case and rank count", "",
           "Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples "
           "(can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper "
           "bound of the simultaneous total). Rust columns: the driver's own records (largest rank; "
           "sum over the ranks in parentheses).", "",
           "| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |",
           "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    medians = {}
    for (c, np), rs in sorted(groups.items(), key=lambda kv: (kv[0][1], kv[0][0])):
        mm = {
            "n": len(rs), "wall_s": med(r["wall_s"] for r in rs),
            "init_full_s": med(r["chrono"].get("init_full") for r in rs),
            "static_solve_s": med(r["chrono"].get("static_solve") for r in rs),
            "solve_step_ms": med(per_step(r, "solve_step") for r in rs),
            "check_ms": med(per_step(r, "check_cohesive_stress") for r in rs),
            "core_us_el_step": med(core_us(r) for r in rs),
        }
        for k in ("static_rank_peak_max", "static_total_rss_peak", "loop_rank_rss_peak_max",
                  "loop_total_rss_peak", "run_total_rss_peak", "run_rank_peak_max", "run_rank_peak_sum"):
            mm[k + "_kib"] = med(r["mem"].get(k) for r in rs)
        for k in ("rss_before_static", "hwm_static", "hwm_end", "rss_end"):
            for agg in ("max", "sum"):
                mm[f"rust_{k}_{agg}_kib"] = med(r.get("rust_memory", {}).get(k, {}).get(agg) for r in rs)
        medians[f"{c}/np{np}"] = mm

        def both(k):
            a, b = mm.get(f"rust_{k}_max_kib"), mm.get(f"rust_{k}_sum_kib")
            return "—" if a is None else f"{fmt(mib(a))} ({fmt(mib(b))})"
        md.append("| " + " | ".join([c, str(np), str(len(rs)), fmt(mm["wall_s"]), fmt(mm["init_full_s"]),
                                     fmt(mm["static_solve_s"]), fmt(mm["solve_step_ms"]), fmt(mm["check_ms"]),
                                     fmt(mm["core_us_el_step"]),
                                     fmt(mib(mm["run_rank_peak_max_kib"])),
                                     fmt(mib(mm["run_total_rss_peak_kib"])),
                                     fmt(mib(mm["run_rank_peak_sum_kib"])),
                                     both("rss_before_static"), both("hwm_static"), both("hwm_end")]) + " |")
    md.append("")

    nps = sorted({r["np"] for r in ok})
    md += ["## Rust / C++ (medians)", "", "| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |",
           "|---|---|---|---|---|---|---|---|"]
    for np in nps:
        a, b = medians.get(f"rust/np{np}"), medians.get(f"cpp/np{np}")
        if not a or not b:
            continue
        q = lambda k: fmt(a[k] / b[k]) if a.get(k) and b.get(k) else "—"
        md.append(f"| {np} | {q('wall_s')} | {q('init_full_s')} | {q('static_solve_s')} | {q('solve_step_ms')} | "
                  f"{q('check_ms')} | {q('run_rank_peak_max_kib')} | {q('run_total_rss_peak_kib')} |")
    md.append("")

    # ---- correctness ---------------------------------------------------------
    comp = {}
    md += ["## Energy histories and insertions", "",
           "Max relative difference per column (ed, ep, ek, ew, et) over the step windows; "
           "`bitwise` = identical energies.csv text.", ""]

    def pair_block(label, pairs):
        if not pairs:
            return
        agg, bitwise, ins_equal, dts = {}, 0, 0, []
        for x, y in pairs:
            if x["energies"] is None or y["energies"] is None:
                continue
            merge_max(agg, compare(x["energies"], y["energies"]))
            bitwise += x["energies_text"] == y["energies_text"]
            ins_equal += x["insertion_steps"] == y["insertion_steps"]
            dts.append(rel(x["summary"].get("time_step", 0), y["summary"].get("time_step", 0)))
        comp[label] = {"pairs": len(pairs), "bitwise": bitwise, "insertion_steps_equal": ins_equal,
                       "time_step_rel_max": max(dts) if dts else None, "windows": agg}
        md.append(f"**{label}** — {len(pairs)} pairs, bitwise {bitwise}, insertion steps equal {ins_equal}, "
                  f"Δt rel ≤ {fmt(max(dts) if dts else None)}")
        md.append("")
        md.append("| window | " + " | ".join(COLS) + " |")
        md.append("|---|" + "---|" * len(COLS))
        for w, cols in agg.items():
            md.append(f"| {w} | " + " | ".join(fmt(cols[c]) for c in COLS) + " |")
        md.append("")

    for np in nps:
        cpp = groups.get(("cpp", np), [])
        rust = groups.get(("rust", np), [])
        pair_block(f"np{np}: Rust (MUMPS) vs C++ (MUMPS)", list(product(rust, cpp)))
        pair_block(f"np{np}: C++ vs C++ (run to run)", list(combinations(cpp, 2)))
        pair_block(f"np{np}: Rust vs Rust (run to run)", list(combinations(rust, 2)))
        if np == 1:
            pair_block("np1: Rust MUMPS vs Rust Eigen", list(product(rust, groups.get(("rusteig", 1), []))))
            pair_block("np1: C++ MUMPS vs C++ Eigen", list(product(cpp, groups.get(("cppeig", 1), []))))
            pair_block("np1: Rust Eigen vs C++ Eigen", list(product(groups.get(("rusteig", 1), []),
                                                                     groups.get(("cppeig", 1), []))))
        parts = {r["partition"] for r in cpp if r["partition"]}
        if np > 1:
            md.append(f"np{np}: distinct C++ partition.csv over the runs: {len(parts)}")
            md.append("")
    md += ["## Counts and oracle-free invariants per run", "",
           "Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned "
           "once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + "
           "the nodes doubled by insertion (≥ start).", "",
           "| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |",
           "|---|---|---|---|---|---|---|---|---|---|---|"]
    inv_fail = []
    for r in sorted(ok, key=lambda r: (r["np"], r["case"], r["rep"])):
        s = r["summary"]
        own = s.get("nb_owned_nodes_sum") == s.get("nb_global_nodes_end")
        reg = s.get("nb_regular_elements_dim3") == tets
        grow = (s.get("nb_global_nodes_end") or 0) >= (s.get("nb_global_nodes_start") or 0)
        if not (own and reg and grow):
            inv_fail.append(f"{r['case']}/np{r['np']}/{r['rep']}")
        md.append(f"| {r['case']} | {r['np']} | {r['rep']} | {s.get('nb_cohesive_elements')} | "
                  f"{s.get('nb_insertion_steps')} | {s.get('first_insertion_step')} | "
                  f"{s.get('nb_global_nodes_start')} | {s.get('nb_global_nodes_end')} | {own} | {reg} | "
                  f"{s.get('energy_dissipated')!r} |")
    md.append("")
    md.append("Invariant failures: " + (", ".join(inv_fail) if inv_fail else "none"))
    md.append("")
    rust_static = [(r["np"], r["rep"], r["rust_static"]) for r in ok if "rust_static" in r]
    if rust_static:
        md += ["## Rust static solve (solver, matrix, MUMPS statistics)", ""]
        for np, rep, st in sorted(rust_static, key=lambda x: (x[0], x[1])):
            md.append(f"- np{np} {rep}: {json.dumps(st, sort_keys=False)}")
        md.append("")
    (root / "summary.md").write_text("\n".join(md) + "\n")
    out = {"tets": tets, "medians": medians, "comparisons": comp, "invariant_failures": inv_fail,
           "runs": [{k: v for k, v in r.items() if k not in ("energies", "energies_text", "partition")}
                    for r in runs]}
    (root / "summary.json").write_text(json.dumps(out, indent=1, sort_keys=True) + "\n")
    print("\n".join(md))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
