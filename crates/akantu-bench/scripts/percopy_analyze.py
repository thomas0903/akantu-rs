#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the summary of the runs of the cohesive driver with
# the per-copy check of the loads (rust_load_check_*), written by
# percopy_campaign.sh. Python standard library only.
"""Summarise the per-copy load checks of a set of driver runs.

    python3 -I crates/akantu-bench/scripts/percopy_analyze.py RESULTS [RUN...]
    python3 -I crates/akantu-bench/scripts/percopy_analyze.py --thin OUT RESULTS [RUN...]

RESULTS holds one directory per run (percopy_campaign.sh `run NAME ...`);
without RUN every subdirectory with a summary.json is read. Run names follow
`<mesh>-np<P>-<placement>[-plain]`: `-plain` is the same run without the
check (its energies.csv and insertions.csv are compared byte for byte with
the checked run of the same name).

Prints, per run: the effective placement, the checks made and failed, the
first checked step with a doubled loaded node and the first failed check,
the largest per-copy difference (relative to the largest nodal load) before
and from the first doubled loaded node, the counts of the last check
(doubled loaded nodes, records moved summed over the ranks, moves from
written nodes, foreign writes, copies differing from the re-application)
and the largest |sum f - sum f_reapplied| / max|f|, and the largest count of
loaded boundary rows that differ from their facet's row from the first check
with a new copy on (column row_facet_mismatches; "-" for
files written before it); then the A/B pairs, the insertion histories and
total loads across rank counts, and, per mesh and rank count, the first step
at which the two placements' energies differ.

The final verdict line (exit code 1 when unexpected) requires of every
checked run: at least one check, as many rows as the
summary's check count, and, when only rust_load_check_every is given, one
check per due step 0..max_steps; no foreign write and no move from a written
node in any row (D-108's rule never exercised); then per placement: the
corrected one passes every check with a doubled loaded node and moved
records, upstream's fails exactly from its first doubled loaded node on.

--thin OUT copies, per run, the small files (summary.json, insertions.csv,
input.dat, chrono.csv, wall.txt, memory_peak) into OUT/<run>/ and writes
OUT/<run>/load_check.thin.csv: the rows of load_check.csv at step 0, every
100th step, the first steps with a doubled loaded node and the first failed
check (each with the two steps before and after), the step of the largest
difference and the last step; OUT/files.sha256 lists the SHA-256 of every
file of every run read (the full files stay on the machine that ran them).
"""

import csv
import hashlib
import json
import re
import sys
from pathlib import Path

RTOL = 1e-12
NAME = re.compile(r"^(?P<mesh>[a-z0-9.]+)-np(?P<np>\d+)-(?P<placement>[a-z_]+?)(?P<plain>-plain)?$")


def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else None


def rows(path):
    with open(path, newline="") as f:
        r = csv.reader(f, skipinitialspace=True)
        header = [h.strip() for h in next(r)]
        return [dict(zip(header, (x.strip() for x in row))) for row in r if row]


def energies(path):
    out = {}
    for line in path.read_text().splitlines()[1:]:
        f = [x.strip() for x in line.split(",")]
        out[int(f[0])] = f[1:]
    return out


def expected_checks(s):
    """The number of due steps 0..max_steps when only rust_load_check_every
    is given (None otherwise: a step list is not re-parsed here)."""
    every, steps, max_steps = s.get("rust_load_check_every"), s.get("rust_load_check_steps"), s.get("max_steps")
    if every is None or steps is not None or max_steps is None:
        return None
    try:
        e = int(str(every).strip())
    except ValueError:
        return None
    return int(max_steps) // e + 1 if e > 0 else 0


def run_info(d):
    s = json.loads((d / "summary.json").read_text())
    info = {
        "dir": d,
        "placement": s.get("rust_doubled_node_load"),
        "psize": s.get("psize"),
        "cohesive": s.get("nb_cohesive_elements"),
        "insertion_steps": s.get("nb_insertion_steps"),
        "energies_sha": sha(d / "energies.csv"),
        "insertions_sha": sha(d / "insertions.csv"),
        "summary_check": s.get("rust_load_check"),
        "expected_checks": expected_checks(s),
        "wall": (d / "wall.txt").read_text().strip() if (d / "wall.txt").exists() else "?",
    }
    lc = d / "load_check.csv"
    if not lc.exists():
        info["checks"] = None
        return info
    rs = rows(lc)
    info["checks"] = len(rs)
    if not rs:
        # the keys were given but no check was made
        info.update(failed=0, first_loaded=None, first_failed=None, max_before=None, max_from=None,
                    at_first=None, last=None, total_rel=0.0, applications=("-", "-"), totals={},
                    foreign=0, from_written=0, row_mm=None)
        return info
    info["failed"] = sum(r["verdict"] != "pass" for r in rs)
    first_loaded = next((int(r["step"]) for r in rs if int(r["doubled_loaded"]) > 0), None)
    first_failed = next((int(r["step"]) for r in rs if r["verdict"] != "pass"), None)
    info["first_loaded"], info["first_failed"] = first_loaded, first_failed

    def mx(sel):
        best = None
        for r in sel:
            v = float(r["max_rel_diff"])
            if best is None or v > best[0]:
                best = (v, int(r["step"]), r["worst_gid"], r["worst_flag"])
        return best

    info["max_before"] = mx(r for r in rs if first_loaded is None or int(r["step"]) < first_loaded)
    info["max_from"] = mx(r for r in rs if first_loaded is not None and int(r["step"]) >= first_loaded)
    info["at_first"] = next((r for r in rs if first_loaded is not None and int(r["step"]) == first_loaded), None)
    last = rs[-1]
    info["last"] = last
    info["total_rel"] = max(float(r["total_rel_diff"]) for r in rs)
    info["applications"] = (last["applications_min"], last["applications_max"])
    info["totals"] = {int(r["step"]): ([float(r[f"total_f_{c}"]) for c in "xyz"], float(r["fmax"])) for r in rs}
    info["foreign"] = max(int(r["foreign_writes_sum"]) for r in rs) if rs else 0
    info["from_written"] = max(int(r["moved_from_written_nodes_sum"]) for r in rs) if rs else 0
    first_copy = next((k for k, r in enumerate(rs) if int(r["new_copies"]) > 0), None)
    if rs and "row_facet_mismatches" in rs[0] and first_copy is not None:
        info["row_mm"] = max(int(r["row_facet_mismatches"]) for r in rs[first_copy:])
    else:
        info["row_mm"] = None
    return info


def run_problems(i):
    """What a checked run lacks: the checks made, D-108's
    rule never exercised; then the placement's own expectation."""
    out = []
    if i["checks"] is None:
        return out
    sc = (i["summary_check"] or {}).get("checks")
    if i["checks"] == 0:
        out.append("no check made")
    if sc is not None and sc != i["checks"]:
        out.append(f"summary checks {sc} != {i['checks']} rows")
    if i["expected_checks"] is not None and i["checks"] != i["expected_checks"]:
        out.append(f"{i['checks']} checks for {i['expected_checks']} due steps")
    if i["foreign"]:
        out.append(f"foreign writes ({i['foreign']})")
    if i["from_written"]:
        out.append(f"moves from written nodes ({i['from_written']})")
    if i["checks"]:
        if i["placement"] == "follows_elements" and (
                i["failed"] or not i["first_loaded"] or int(i["last"]["moved_contributions_sum"]) == 0):
            out.append("corrected placement: a failed check, no doubled loaded node or no move")
        if i["placement"] == "upstream" and (i["first_failed"] is None or i["first_failed"] != i["first_loaded"]):
            out.append("upstream placement: not failing exactly from the first doubled loaded node")
    return out


def thin(res, names, out):
    import shutil
    out.mkdir(parents=True, exist_ok=True)
    sums = []
    for n in names:
        d = res / n
        for p in sorted(d.rglob("*")):
            if p.is_file():
                sums.append(f"{sha(p)}  {n}/{p.relative_to(d)}")
        o = out / n
        o.mkdir(exist_ok=True)
        for f in ("summary.json", "insertions.csv", "input.dat", "chrono.csv", "wall.txt", "memory_peak"):
            if (d / f).exists():
                shutil.copyfile(d / f, o / f)
        lc = d / "load_check.csv"
        if not lc.exists():
            continue
        lines = lc.read_text().splitlines()
        rs = rows(lc)
        keep = {0, len(rs) - 1} | {k for k, r in enumerate(rs) if int(r["step"]) % 100 == 0}
        for cond in (lambda r: int(r["doubled_loaded"]) > 0, lambda r: r["verdict"] != "pass"):
            k = next((k for k, r in enumerate(rs) if cond(r)), None)
            if k is not None:
                keep |= {j for j in range(k - 2, k + 3) if 0 <= j < len(rs)}
        k = max(range(len(rs)), key=lambda j: float(rs[j]["max_rel_diff"]))
        keep.add(k)
        (o / "load_check.thin.csv").write_text("\n".join([lines[0]] + [lines[j + 1] for j in sorted(keep)]) + "\n")
    (out / "files.sha256").write_text("\n".join(sums) + "\n")


def fmt(v):
    return "-" if v is None else (f"{v:.3e}" if isinstance(v, float) else str(v))


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    thin_out = None
    if argv[0] == "--thin":
        thin_out, argv = Path(argv[1]), argv[2:]
    res = Path(argv[0])
    names = argv[1:] or sorted(p.name for p in res.iterdir() if (p / "summary.json").exists())
    if thin_out is not None:
        thin(res, names, thin_out)
    runs = {}
    for n in names:
        runs[n] = run_info(res / n)
    print("## Runs\n")
    print("| run | placement | ranks | cohesive / insertion steps | checks (failed) | first doubled loaded | "
          "first failed | max before | max from first (step, node, flag) | at last check: doubled loaded, "
          "moved (sum), from written, foreign writes, copies differing | max total rel | applications | "
          "row/facet mismatches after the first copy | wall s |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for n, i in runs.items():
        if i["checks"] is None:
            print(f"| {n} | {i['placement']} | {i['psize']} | {i['cohesive']} / {i['insertion_steps']} | "
                  f"no check | | | | | | | | | {i['wall']} |")
            continue
        if i["last"] is None:
            print(f"| {n} | {i['placement']} | {i['psize']} | {i['cohesive']} / {i['insertion_steps']} | "
                  f"0 (keys given, no check made) | | | | | | | | | {i['wall']} |")
            continue
        mb, mf, last = i["max_before"], i["max_from"], i["last"]
        mf_s = f"{mf[0]:.3e} ({mf[1]}, {mf[2]}, {mf[3]})" if mf else "-"
        print(f"| {n} | {i['placement']} | {i['psize']} | {i['cohesive']} / {i['insertion_steps']} | "
              f"{i['checks']} ({i['failed']}) | {fmt(i['first_loaded'])} | {fmt(i['first_failed'])} | "
              f"{fmt(mb[0] if mb else None)} | {mf_s} | {last['doubled_loaded']}, "
              f"{last['moved_contributions_sum']}, {last['moved_from_written_nodes_sum']}, "
              f"{last['foreign_writes_sum']}, {last['differing_copies']} | {i['total_rel']:.3e} | "
              f"{i['applications'][0]}..{i['applications'][1]} | {fmt(i['row_mm'])} | {i['wall']} |")
    print("\n## At the first doubled loaded node\n")
    print("| run | step | doubled loaded | moved (sum) | max rel | worst (rank, gid, flag) | f | f re-applied | verdict |")
    print("|---|---|---|---|---|---|---|---|---|")
    for n, i in runs.items():
        r = i.get("at_first")
        if not r:
            continue
        print(f"| {n} | {r['step']} | {r['doubled_loaded']} | {r['moved_contributions_sum']} | "
              f"{float(r['max_rel_diff']):.3e} | {r['worst_rank']}, {r['worst_gid']}, {r['worst_flag']} | "
              f"{r['worst_f_x']}, {r['worst_f_y']}, {r['worst_f_z']} | {r['worst_reapplied_x']}, "
              f"{r['worst_reapplied_y']}, {r['worst_reapplied_z']} | {r['verdict']} |")
    print("\n## A/B: with the check against without (energies.csv, insertions.csv)\n")
    for n, i in runs.items():
        m = NAME.match(n)
        if not m or not m.group("plain"):
            continue
        twin = n[: -len("-plain")]
        if twin not in runs:
            print(f"- {n}: no checked twin {twin}")
            continue
        t = runs[twin]
        same = i["energies_sha"] == t["energies_sha"] and i["insertions_sha"] == t["insertions_sha"]
        print(f"- {twin} vs {n}: energies {'identical' if i['energies_sha'] == t['energies_sha'] else 'DIFFER'} "
              f"(sha256 {(t['energies_sha'] or '-')[:12]}…), insertions "
              f"{'identical' if i['insertions_sha'] == t['insertions_sha'] else 'DIFFER'} "
              f"(sha256 {(t['insertions_sha'] or '-')[:12]}…): {'byte-identical' if same else 'NOT IDENTICAL'}")
    print("\n## Across rank counts (checked runs of one mesh and placement)\n")
    groups = {}
    for n, i in runs.items():
        m = NAME.match(n)
        if m and not m.group("plain") and i["checks"] is not None:
            groups.setdefault((m.group("mesh"), m.group("placement")), []).append((int(m.group("np")), n))
    for (mesh, pl), lst in sorted(groups.items()):
        lst.sort()
        shas = {runs[n]["insertions_sha"] for _, n in lst}
        line = f"- {mesh} {pl}, ranks {' '.join(str(p) for p, _ in lst)}: insertions.csv " + (
            "identical" if len(shas) == 1 else f"{len(shas)} different files")
        base = runs[lst[0][1]]["totals"]
        worst = 0.0
        for _, n in lst[1:]:
            for step, (tot, fmax) in runs[n]["totals"].items():
                if step in base and fmax > 0:
                    worst = max(worst, max(abs(a - b) for a, b in zip(tot, base[step][0])) / fmax)
        line += f"; largest |sum f(P) - sum f({lst[0][0]})| / max|f| over the common checks {worst:.3e}"
        print(line)
    print("\n## The two placements (same mesh and rank count): first step whose energies differ\n")
    for n, i in runs.items():
        m = NAME.match(n)
        if not m or m.group("plain") or m.group("placement") != "follows":
            continue
        other = f"{m.group('mesh')}-np{m.group('np')}-upstream"
        if other not in runs:
            continue
        ea, eb = energies(i["dir"] / "energies.csv"), energies(runs[other]["dir"] / "energies.csv")
        first = next((s for s in sorted(ea) if s in eb and ea[s] != eb[s]), None)
        print(f"- {n} vs {other}: energies equal up to step {fmt(None if first is None else first - 1)}, first "
              f"difference at step {fmt(first)}; first doubled loaded node at step {fmt(i.get('first_loaded'))} "
              f"/ {fmt(runs[other].get('first_loaded'))}")
    bad = [f"{n} ({'; '.join(p)})" for n, i in runs.items() if (p := run_problems(i))]
    print(f"\nverdict: {'every checked run made its checks without foreign writes; every corrected run passes every check (not vacuous); every upstream run fails from its first doubled loaded node' if not bad else 'UNEXPECTED: ' + ', '.join(bad)}")
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
