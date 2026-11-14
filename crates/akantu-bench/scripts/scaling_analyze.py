#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: analysis of a scaling_series.sh series.
"""Weak and strong scaling tables of an interleaved series (standard library only).

    python3 -I scaling_analyze.py OUT SERIES [--weak NAME=H:NP,H:NP,...]... [--strong H]... [--expect-insertion]

OUT/SERIES-h<H>-np<NP>-r<k>/ are the campaigns of scaling_series.sh (one run
per case each, analysed by c1prep_analyze.py: summary.json, provenance.txt,
<case>/np<NP>/rep1/status.txt). Writes OUT/SERIES-summary.md and
OUT/SERIES-summary.json:

* per point (case, h, NP): the number of valid runs, median [min, max] of
  the rank-0 Chrono phases (ms per step of `solve_step` and of
  `check_cohesive_stress`, their sum = the explicit loop per step;
  `init_full`, `static_solve`, `dist_mesh` in s), the wall time, core-µs per
  element and step (loop per step × NP / tetrahedra), the memory sampler's
  whole-run peaks (largest rank, total), the cohesive elements at the end
  (the series is meant to stay before the first insertion: any run that
  inserted is listed);
* Rust / C++ per point: ratio of the medians and the paired ratio (the two
  codes' runs of the same repetition, adjacent in time), median [min, max];
* per weak study (--weak, points with nearly constant tetrahedra per rank):
  per code and point, elements per rank, the loop per step, the weak
  efficiency of the loop E_w = core-µs(base) / core-µs(point) (normalises the
  small differences of elements per rank; = t(base) / t(point) at exactly
  constant elements per rank) both from the medians and per repetition
  (base and point of the same repetition), and the set-up phases;
* per strong study (--strong H: every NP measured on mesh h): speed-up and
  efficiency of the loop, the static solve, `init_full` and the wall time
  against the smallest NP, from the medians and per repetition.

Runs whose outcome is not `ok` are excluded and listed. Exit status 0 when at
least one run was read, 1 otherwise.
"""

import argparse
import json
import re
import statistics
import sys
from pathlib import Path

CAMP_RE = re.compile(r"^(?P<series>.+)-h(?P<h>[0-9.]+)-np(?P<np>[0-9]+)-r(?P<rep>[0-9]+)$")


def med(xs):
    xs = [x for x in xs if x is not None]
    return statistics.median(xs) if xs else None


def rng(xs):
    xs = [x for x in xs if x is not None]
    return (min(xs), max(xs)) if xs else (None, None)


def fmt(x, digits=3):
    if x is None:
        return "—"
    if x == 0:
        return "0"
    a = abs(x)
    if a >= 1000:
        return f"{x:,.0f}"
    return f"{x:.{digits}g}"


def mmm(xs, digits=3):
    m = med(xs)
    lo, hi = rng(xs)
    if m is None:
        return "—"
    if len([x for x in xs if x is not None]) == 1:
        return fmt(m, digits)
    return f"{fmt(m, digits)} [{fmt(lo, digits)}, {fmt(hi, digits)}]"


def read_status(d):
    st = {}
    f = d / "status.txt"
    if f.exists():
        for line in f.read_text().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                try:
                    st[k] = json.loads(v)
                except ValueError:
                    st[k] = v
    return st


def provenance(camp):
    p = camp / "provenance.txt"
    out = {}
    if not p.exists():
        return out
    text = p.read_text()
    m = re.search(r"^mesh=(\S+) sha256=([0-9a-f]+) elements=(\d+)", text, re.M)
    if m:
        out["mesh"], out["mesh_sha256"], out["mesh_elements"] = m.group(1), m.group(2), int(m.group(3))
    m = re.search(r"rust_bin=\S+ sha256=(\S+)", text)
    if m:
        out["rust_binary_sha256"] = m.group(1)
    m = re.search(r"^cpp_image=\S+ (sha256:[0-9a-f]+) config=(\S+) fnte=(\S+)", text, re.M)
    if m:
        out["cpp_image"], out["cpp_config"], out["cpp_fnte"] = m.group(1), m.group(2), m.group(3)
    m = re.search(r"^rust_image=\S+ (sha256:[0-9a-f]+)", text, re.M)
    if m:
        out["rust_image"] = m.group(1)
    m = re.search(r"revision=(\S+) dirty=(\S+)", text)
    if m:
        out["revision"], out["dirty"] = m.group(1), m.group(2)
    m = re.search(r"^host_cpu=(.*) nproc=(\d+) kernel=(\S+)", text, re.M)
    if m:
        out["host_cpu"], out["nproc"], out["kernel"] = m.group(1), int(m.group(2)), m.group(3)
    m = re.search(r"^mem_total_kib=(\d+)", text, re.M)
    if m:
        out["mem_total_kib"] = int(m.group(1))
    return out


def per_step(ch, name):
    v, n = ch.get(name), ch.get(name + " nb_rep")
    if v is None or not n:
        return None
    return v / n


def collect(out, series):
    runs, rejected, prov = [], [], {}
    for camp in sorted(out.iterdir()):
        m = CAMP_RE.match(camp.name)
        if not camp.is_dir() or not m or m.group("series") != series:
            continue
        h, np_, rep = m.group("h"), int(m.group("np")), int(m.group("rep"))
        sj = camp / "summary.json"
        if not sj.exists():
            rejected.append((camp.name, "no summary.json"))
            continue
        summ = json.loads(sj.read_text())
        pv = provenance(camp)
        tets = summ.get("tets")
        for r in summ.get("runs", []):
            case = r["case"]
            rd = camp / case / f"np{np_}" / "rep1"
            st = read_status(rd)
            if st.get("outcome") != "ok":
                rejected.append((f"{camp.name}/{case}", st.get("outcome", "no status")))
                continue
            ch = r.get("chrono", {})
            s = r.get("summary", {})
            mem = r.get("mem") or {}
            solve, check = per_step(ch, "solve_step"), per_step(ch, "check_cohesive_stress")
            loop = solve + check if solve is not None and check is not None else None
            binary = None
            if case.startswith("cpp") and (rd / "run.json").exists():
                binary = json.loads((rd / "run.json").read_text()).get("binary_sha256")
            elif (rd / "binary.sha256").exists():
                binary = (rd / "binary.sha256").read_text().split()[0]
            run = {
                "campaign": camp.name, "case": case, "h": h, "np": np_, "rep": rep, "tets": tets,
                "solve_ms": solve * 1e3 if solve is not None else None,
                "check_ms": check * 1e3 if check is not None else None,
                "loop_ms": loop * 1e3 if loop is not None else None,
                "core_us": loop * 1e6 * np_ / tets if loop is not None and tets else None,
                "init_full_s": ch.get("init_full"), "static_solve_s": ch.get("static_solve"),
                "dist_mesh_s": ch.get("dist_mesh"), "read_mesh_s": ch.get("read_mesh"),
                "before_step_s": ch.get("before_step"), "wall_s": r.get("wall_s"),
                "rank_peak_mib": mem.get("run_rank_peak_max") / 1024 if mem.get("run_rank_peak_max") else None,
                "total_peak_mib": mem.get("run_total_rss_peak") / 1024 if mem.get("run_total_rss_peak") else None,
                "coh_el": s.get("nb_cohesive_elements"), "steps": s.get("max_steps"),
                "binary_sha256": binary,
            }
            runs.append(run)
            key = (h, case)
            prov.setdefault(h, {"mesh_sha256": pv.get("mesh_sha256"), "tets": tets})
            for k in ("revision", "dirty", "host_cpu", "nproc", "kernel", "mem_total_kib", "cpp_image",
                      "cpp_config", "cpp_fnte", "rust_image"):
                if k in pv:
                    prov.setdefault("_host", {}).setdefault(k, set()).add(str(pv[k]))
            prov.setdefault("_binaries", {}).setdefault(case, set()).add(str(binary))
            del key
    return runs, rejected, prov


FIELDS = ("solve_ms", "check_ms", "loop_ms", "core_us", "init_full_s", "static_solve_s", "dist_mesh_s",
          "wall_s", "rank_peak_mib", "total_peak_mib")


def point_table(runs):
    pts = {}
    for r in runs:
        pts.setdefault((r["case"], r["h"], r["np"]), []).append(r)
    rows = {}
    for (case, h, np_), rs in sorted(pts.items(), key=lambda kv: (kv[0][1], kv[0][2], kv[0][0])):
        row = {"case": case, "h": h, "np": np_, "n": len(rs), "tets": rs[0]["tets"],
               "el_per_rank": rs[0]["tets"] / np_ if rs[0]["tets"] else None,
               "coh_el": sorted({r["coh_el"] for r in rs}), "reps": sorted(r["rep"] for r in rs)}
        for f in FIELDS:
            vals = [r[f] for r in rs]
            row[f] = {"median": med(vals), "min": rng(vals)[0], "max": rng(vals)[1], "values": vals}
        rows[(case, h, np_)] = row
    return rows


def by_rep(runs):
    d = {}
    for r in runs:
        d[(r["case"], r["h"], r["np"], r["rep"])] = r
    return d


def paired(br, a_case, b_case, h, np_, field):
    vals = []
    for (c, hh, n, rep), r in br.items():
        if c != a_case or hh != h or n != np_:
            continue
        o = br.get((b_case, h, np_, rep))
        if o and r[field] and o[field]:
            vals.append(r[field] / o[field])
    return vals


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("series")
    ap.add_argument("--weak", action="append", default=[], help="NAME=H:NP,H:NP,...")
    ap.add_argument("--strong", action="append", default=[], help="H")
    ap.add_argument("--expect-insertion", action="store_true",
                    help="the series runs past the first insertion: list the counts instead of flagging them")
    a = ap.parse_args()
    runs, rejected, prov = collect(a.out, a.series)
    if not runs:
        print("no valid run found", file=sys.stderr)
        return 1
    rows = point_table(runs)
    br = by_rep(runs)
    cases = sorted({r["case"] for r in runs})
    md = [f"# Scaling series `{a.series}`", ""]
    host = {k: sorted(v) for k, v in prov.get("_host", {}).items()}
    bins = {k: sorted(v) for k, v in prov.get("_binaries", {}).items()}
    md.append("Provenance (every campaign of the series): " + "; ".join(f"{k} = {', '.join(v)}" for k, v in host.items()))
    md.append("")
    md.append("Binaries (SHA-256): " + "; ".join(f"{k}: {', '.join(x[:16] + '…' for x in v)}" for k, v in bins.items()))
    md.append("")
    md.append("Meshes: " + "; ".join(f"h = {h}: {v['tets']:,} tetrahedra, `{(v['mesh_sha256'] or '?')[:16]}…`"
                                     for h, v in sorted(prov.items()) if not h.startswith("_")))
    md.append("")
    md.append(f"Valid runs: {len(runs)}; excluded: {len(rejected)}" +
              ("".join(f"\n- {n}: {why}" for n, why in rejected) if rejected else ""))
    inserted = [r for r in runs if r["coh_el"]]
    md.append("")
    if a.expect_insertion:
        counts = {}
        for r in runs:
            counts.setdefault((r["case"], r["h"], r["np"]), []).append(r["coh_el"])
        md.append("Cohesive elements at the end, per point, as measured (values over the repetitions; "
                  "not a comparison between rank counts or codes): " +
                  "; ".join(f"{c} h = {h} on {n}: {', '.join(str(v) for v in sorted(set(vals)))}"
                            for (c, h, n), vals in sorted(counts.items(), key=lambda kv: (kv[0][1], kv[0][2], kv[0][0]))))
    else:
        md.append("Runs with cohesive insertion (the series is meant to stay before the first insertion): " +
                  (", ".join(f"{r['campaign']}/{r['case']} ({r['coh_el']})" for r in inserted) if inserted else "none"))
    md.append("")
    md.append("## Points (median [min, max] over the repetitions)")
    md.append("")
    md.append("| case | h | ranks | n | tets | tets / rank | `solve_step` ms/step | check ms/step | loop ms/step | "
              "core-µs/el/step | init_full s | static solve s | dist_mesh s | wall s | peak rank MiB | peak total MiB |")
    md.append("|---|---|---:|---:|---:|---:|---|---|---|---|---|---|---|---|---|---|")
    for (case, h, np_), row in rows.items():
        md.append(f"| {case} | {h} | {np_} | {row['n']} | {row['tets']:,} | {row['el_per_rank']:,.0f} | "
                  + " | ".join(mmm(row[f]["values"]) for f in FIELDS[:4]) + " | "
                  + " | ".join(mmm(row[f]["values"]) for f in FIELDS[4:]) + " |")
    md.append("")
    ratios = {}
    if "rust" in cases and "cpp" in cases:
        md.append("## Rust / C++ per point (ratio of the medians; paired: same repetition, median [min, max])")
        md.append("")
        md.append("| h | ranks | loop | `solve_step` | check | static solve | init_full | wall |")
        md.append("|---|---:|---|---|---|---|---|---|")
        for (case, h, np_), row in rows.items():
            if case != "rust" or ("cpp", h, np_) not in rows:
                continue
            c = rows[("cpp", h, np_)]
            cells = []
            for f in ("loop_ms", "solve_ms", "check_ms", "static_solve_s", "init_full_s", "wall_s"):
                rm = row[f]["median"] / c[f]["median"] if row[f]["median"] and c[f]["median"] else None
                pv = paired(br, "rust", "cpp", h, np_, f)
                ratios[f"{h}:{np_}:{f}"] = {"medians": rm, "paired": pv}
                cells.append(f"{fmt(rm)}; {mmm(pv)}")
            md.append(f"| {h} | {np_} | " + " | ".join(cells) + " |")
        md.append("")
    studies = {"weak": {}, "strong": {}}
    for spec in a.weak:
        name, _, pts = spec.partition("=")
        plist = [(p.split(":")[0], int(p.split(":")[1])) for p in pts.split(",") if p]
        md.append(f"## Weak study `{name}` (points {', '.join(f'h = {h} on {n}' for h, n in plist)})")
        md.append("")
        md.append("E_w = core-µs/el/step at the base point / at the point (the loop per step normalised by "
                  "the elements per rank); per repetition: base and point of the same repetition.")
        md.append("")
        md.append("| case | h | ranks | tets / rank | loop ms/step | core-µs/el/step | E_w (medians) | E_w per repetition | "
                  "static solve s | init_full s |")
        md.append("|---|---|---:|---:|---|---|---:|---|---|---|")
        st = {}
        for case in cases:
            base = rows.get((case, plist[0][0], plist[0][1]))
            if not base:
                continue
            for h, n in plist:
                row = rows.get((case, h, n))
                if not row:
                    continue
                ew = base["core_us"]["median"] / row["core_us"]["median"] if row["core_us"]["median"] else None
                per = []
                for rep in row["reps"]:
                    b = br.get((case, plist[0][0], plist[0][1], rep))
                    p = br.get((case, h, n, rep))
                    if b and p and p["core_us"]:
                        per.append(b["core_us"] / p["core_us"])
                st[f"{case}:{h}:{n}"] = {"ew_medians": ew, "ew_per_rep": per, "el_per_rank": row["el_per_rank"]}
                md.append(f"| {case} | {h} | {n} | {row['el_per_rank']:,.0f} | {mmm(row['loop_ms']['values'])} | "
                          f"{mmm(row['core_us']['values'])} | {fmt(ew)} | {mmm(per)} | "
                          f"{mmm(row['static_solve_s']['values'])} | {mmm(row['init_full_s']['values'])} |")
        studies["weak"][name] = st
        md.append("")
    for h in a.strong:
        nps = sorted({n for (c, hh, n) in rows if hh == h})
        md.append(f"## Strong study h = {h} (ranks {', '.join(map(str, nps))})")
        md.append("")
        md.append("Speed-up S = t(smallest NP) / t(NP) of the medians, efficiency = S × NP0 / NP; per "
                  "repetition: both points of the same repetition.")
        md.append("")
        md.append("| case | ranks | loop S | loop eff. | loop eff. per repetition | static solve S | init_full S | wall S |")
        md.append("|---|---:|---:|---:|---|---:|---:|---:|")
        st = {}
        for case in cases:
            n0 = nps[0]
            base = rows.get((case, h, n0))
            if not base:
                continue
            for n in nps:
                row = rows.get((case, h, n))
                if not row:
                    continue

                def sp(f):
                    return base[f]["median"] / row[f]["median"] if row[f]["median"] else None

                per = []
                for rep in row["reps"]:
                    b = br.get((case, h, n0, rep))
                    p = br.get((case, h, n, rep))
                    if b and p and p["loop_ms"]:
                        per.append(b["loop_ms"] / p["loop_ms"] * n0 / n)
                s_loop = sp("loop_ms")
                eff = s_loop * n0 / n if s_loop else None
                st[f"{case}:{n}"] = {"loop_speedup": s_loop, "loop_eff": eff, "loop_eff_per_rep": per,
                                     "static_speedup": sp("static_solve_s"), "init_speedup": sp("init_full_s"),
                                     "wall_speedup": sp("wall_s")}
                md.append(f"| {case} | {n} | {fmt(s_loop)} | {fmt(eff)} | {mmm(per)} | {fmt(sp('static_solve_s'))} | "
                          f"{fmt(sp('init_full_s'))} | {fmt(sp('wall_s'))} |")
        studies["strong"][h] = st
        md.append("")
    (a.out / f"{a.series}-summary.md").write_text("\n".join(md) + "\n")
    js = {
        "series": a.series,
        "provenance": {"host": host, "binaries": bins,
                       "meshes": {h: v for h, v in prov.items() if not h.startswith("_")}},
        "rejected": rejected,
        "inserted": [(r["campaign"], r["case"], r["coh_el"]) for r in inserted],
        "points": [{**{k: v for k, v in row.items() if k not in FIELDS},
                    **{f: row[f] for f in FIELDS}} for row in rows.values()],
        "rust_over_cpp": ratios,
        "studies": studies,
        "runs": runs,
    }
    (a.out / f"{a.series}-summary.json").write_text(json.dumps(js, indent=1, sort_keys=True) + "\n")
    print("\n".join(md))
    return 0


if __name__ == "__main__":
    sys.exit(main())
