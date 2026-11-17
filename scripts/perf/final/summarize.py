#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the tables of the final
# measurement campaign (scripts/perf/final/README.md).
"""Turn the data directory of one part into its summary.md.

    summarize.py DATA/<PART> [--out FILE]

The tables have the columns of the corresponding tables of
docs/BENCHMARKS.md, so that the document can be updated by copying rows.
Every value is the median [min, max] over the repetitions; a ratio A / B is
the median [min, max] of the per-repetition ratios (the two runs of one
repetition ran next to each other). A point without data is written
"missing", never estimated. The part is read from the directory's
provenance.txt (lines "part=... size=...").
"""

import csv
import json
import math
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path

MISSING = "missing"


# ---------------------------------------------------------------- formatting
def sig(x, n=3):
    """n significant digits, thousands separated (as docs/BENCHMARKS.md)."""
    if x is None:
        return MISSING
    if not math.isfinite(x):
        return str(x)
    if x == 0:
        return "0"
    a = abs(x)
    if a >= 10 ** n:
        return f"{x:,.0f}"
    digits = max(n - 1 - int(f"{a:e}".split("e")[1]), 0)
    return f"{x:,.{digits}f}"


def med(vals, n=3):
    vals = [v for v in vals if v is not None]
    if not vals:
        return MISSING
    m = statistics.median(vals)
    if len(vals) == 1:
        return sig(m, n)
    return f"{sig(m, n)} [{sig(min(vals), n)}, {sig(max(vals), n)}]"


def ratio(a, b, n=3):
    """Paired ratio a/b: a, b map repetition -> value."""
    rs = [a[k] / b[k] for k in a if k in b and a[k] is not None
          and b[k] not in (None, 0)]
    return med(rs, n)


def unit_time(seconds):
    """A duration with its unit (s, ms, µs)."""
    if seconds is None:
        return MISSING
    if seconds >= 1:
        return f"{sig(seconds)} s"
    if seconds >= 1e-3:
        return f"{sig(seconds * 1e3)} ms"
    return f"{sig(seconds * 1e6)} µs"


def med_time(vals):
    vals = [v for v in vals if v is not None]
    if not vals:
        return MISSING
    m = statistics.median(vals)
    if len(vals) == 1:
        return unit_time(m)
    return f"{unit_time(m)} [{unit_time(min(vals))}, {unit_time(max(vals))}]"


def table(header, rows):
    out = ["| " + " | ".join(header) + " |",
           "|" + "---|" * len(header)]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------- provenance
def read_provenance(d):
    p = d / "provenance.txt"
    info = {"part": d.name, "size": "full", "cpu": "unknown", "dates": []}
    if not p.exists():
        return info
    for line in p.read_text(errors="replace").splitlines():
        if line.startswith("== session"):
            info["dates"].append(line.split()[2])
        for key in ("part", "size", "reps", "cpu", "before", "head", "cpp",
                    "cores", "kernel", "sparselu_threaded"):
            m = re.search(rf"(?:^|\s){key}=(\S+(?: [^=]+?)?)(?=\s\S+=|$)",
                          line)
            if m and key not in ("part",):
                info.setdefault(key + "_all", []).append(m.group(1))
                info[key] = m.group(1)
            elif m:
                info[key] = m.group(1)
    return info


def header(d, info, title):
    dry = info.get("size") == "dry"
    out = [f"# Final campaign, part {info.get('part', d.name)}: {title}", ""]
    if dry:
        out += [("**Dry run on a development machine: the smallest sizes "
                "and 2 repetitions, a check of the campaign's chain from "
                "the build to the tables. These numbers are not results "
                "and are not comparable with the published tables.**"), ""]
    out += [(f"- Machine: {info.get('cpu')}, {info.get('cores', '?')} cores,"
            f" kernel {info.get('kernel', '?')}."),
            f"- Sessions (UTC): {', '.join(info['dates']) or 'unknown'}.",
            (f"- Revisions: C++ reference `{info.get('cpp', '?')[:12]}`, "
            f"port before `{info.get('before', '?')[:12]}`, port head "
            f"`{info.get('head', '?')[:12]}`."),
            (f"- Size `{info.get('size')}`, repetitions requested: "
            f"{info.get('reps', '?')}. Provenance: `provenance.txt`; every "
            "command: `commands.log`; progress: `progress.log`."), ""]
    for name, what in (("CHECKS_FAILED", "Failed checks"),
                       ("FAILED", "Failed runs")):
        p = d / name
        if p.exists() and p.read_text().strip():
            lines = p.read_text().strip().splitlines()
            more = ([f"... and {len(lines) - 30} more lines"]
                    if len(lines) > 30 else [])
            out += [f"**{what}** (`{name}`, {len(lines)} lines):", "",
                    "```text"] + lines[:30] + more + ["```", ""]
    return out


# ---------------------------------------------------------------- part A
LABEL_NAMES = {"cpp-strict": "C++ strict", "cpp-default": "C++ default",
               "before": "port before", "head": "port"}
MESHES = ["tet4-20k", "tet4-200k", "tet10-20k", "hex8-20k", "hex8-200k"]
DRY_MESHES = ["tet4-20k", "tet10-20k", "hex8-20k"]


def summarize_a(d, info):
    rows = defaultdict(dict)      # (mesh, mode, phase) -> {(label, r): ns}
    meta = {}
    rounds = []
    for f in sorted((d / "rounds").glob("r*.jsonl")):
        if not (d / "rounds" / (f.stem + ".ok")).exists():
            continue
        for line in f.read_text().splitlines():
            if not line.startswith("{"):
                continue
            o = json.loads(line)
            if o["kind"] == "round":
                rounds.append(o)
                continue
            label, mesh, mode, r = o["label"].split("|")
            if o["kind"] == "meta" and not label.startswith("cpp-"):
                meta[mesh] = (o["types"][0].strip("_"), o["nb_elements"])
            elif o["kind"] == "time":
                rows[(mesh, mode, o["phase"])][(label, r)] = o["ns_el_step"]
    out = header(d, info, "the explicit step on one core")
    out += [f"Rounds: {len(rounds)} (program order rotated per round: "
            + "; ".join(r["order"] for r in rounds) + ").", ""]

    def vals(dd, lab):
        return {r: v for (ll, r), v in dd.items() if ll == lab}

    meshes = DRY_MESHES if info.get("size") == "dry" else MESHES

    for phase, title in (("step", "Explicit step (`solve_step`)"),
                         ("fint", "Internal forces (`fint`)"),
                         ("stress", "Stresses (`stress`)")):
        hdr = ["mesh", "type", "elements", "mode", "C++ strict",
               "C++ default", "port before", "port", "port / C++ strict",
               "port / C++ default", "port / port before",
               "port before / C++ strict"]
        trs = []
        for mode in ("ss", "fd"):
            for mesh in meshes:
                dd = rows.get((mesh, mode, phase), {})
                v = {lab: vals(dd, lab) for lab in
                     ("cpp-strict", "cpp-default", "before", "head")}
                t, n = meta.get(mesh, ("?", None))
                trs.append([mesh, t, f"{n:,}" if n else "?", mode]
                           + [med(list(v[lab].values()), 3) for lab in
                              ("cpp-strict", "cpp-default", "before",
                               "head")]
                           + [ratio(v["head"], v["cpp-strict"], 2),
                              ratio(v["head"], v["cpp-default"], 2),
                              ratio(v["head"], v["before"], 2),
                              ratio(v["before"], v["cpp-strict"], 2)])
        out += [f"Table \"{title}, ns per element and step\":", "",
                table(hdr, trs)]
    # threads and fast mode
    hdr = ["mesh", "mode", "1 thread", "2", "4", "8", "2 / 1", "4 / 1",
           "8 / 1", "fast / bitwise (1 thread)", "fast / bitwise (8)"]
    trs = []
    for mode in ("ss", "fd"):
        for mesh in meshes:
            dd = rows.get((mesh, mode, "step"), {})
            v = {lab: vals(dd, lab) for lab in
                 ("head", "head-t2", "head-t4", "head-t8", "head-fast",
                  "head-fast-t8")}
            trs.append([mesh, mode]
                       + [med(list(v[lab].values())) for lab in
                          ("head", "head-t2", "head-t4", "head-t8")]
                       + [ratio(v[f"head-t{n}"], v["head"], 2)
                          for n in (2, 4, 8)]
                       + [ratio(v["head-fast"], v["head"], 2),
                          ratio(v["head-fast-t8"], v["head-t8"], 2)])
    out += [("Table \"The port's step at 2, 4 and 8 threads "
            "(`AKANTU_NUM_THREADS`, cores 0..N-1) and in fast mode "
            "(`AKANTU_PRECISION=fast`)\", ns per element and step; ratios "
            "paired per round:"), "", table(hdr, trs)]
    dg = d / "digests.tsv"
    if dg.exists():
        lines = [ln.split("\t") for ln in dg.read_text().splitlines() if ln]
        nbad = sum(1 for ln in lines if "DIFFERS" in ln[-1])
        out += [f"State digests (`digests.tsv`, FNV-1a of `u`, `v`, `a`, "
                f"`f_int` after the recorded n steps): {len(lines)} "
                f"program-configuration pairs, {nbad} differing; every "
                "bitwise program (C++ strict, port before, port at 1, 2, 4, "
                "8 threads) equal to the recorded digest of "
                "`docs/benchmarks/explicit-step/final/digests.txt`"
                + (" in every configuration" if nbad == 0 else
                   ": NO (see `digests.tsv`)")
                + "; fast mode not compared.", ""]
    else:
        out += ["State digests: missing.", ""]
    return out


# ---------------------------------------------------------------- suites
def _unrepeatable():
    f = Path(__file__).with_name("digest-exceptions.txt")
    if not f.exists():
        return set()
    return {ln.split("#", 1)[0].strip()[1:].strip()
            for ln in f.read_text().splitlines()
            if ln.split("#", 1)[0].strip().startswith("!")}


UNREPEATABLE = _unrepeatable()


def load_suite_dir(base, nested_np=False):
    """{(variant, np, case, phase): {rep: record}} of a suite directory."""
    res = defaultdict(dict)
    if not base.is_dir():
        return res
    for vdir in sorted(x for x in base.iterdir() if x.is_dir()
                       and not x.name.startswith(".") and x.name != "progs"):
        subdirs = ([x for x in vdir.iterdir() if x.is_dir()] if nested_np
                   else [vdir])
        for sd in subdirs:
            np_ = int(sd.name[2:]) if nested_np else 1
            for f in sorted(sd.glob("*.jsonl")):
                m = re.match(r"(.+)\.r(\d+)\.jsonl$", f.name)
                if not m:
                    continue
                for line in f.read_text().splitlines():
                    if not line.startswith("{"):
                        continue
                    o = json.loads(line)
                    res[(vdir.name, np_, o["case"], o["phase"])][
                        int(m.group(2))] = o
    return res


def done_marks(base):
    return {p.name for p in (base / ".done").glob("*")} \
        if (base / ".done").is_dir() else set()


def suite_tables(base, title, serial=True):
    data = load_suite_dir(base, nested_np=not serial)
    if not data:
        return [f"{title}: missing.", ""]
    keys = sorted({(c, ph) for (_, _, c, ph) in data},
                  key=lambda k: (k[0], k[1]))
    out = []
    sk = base / ".skipped"
    if sk.exists():
        reasons = sorted({f"`{m.group(1)}` ({m.group(2).strip()})"
                          for m in (re.search(r"(\S+): skip: (.*)", ln)
                                    for ln in sk.read_text().splitlines())
                          if m})
        out += [f"{title}: cases not run in this build: "
                + "; ".join(reasons) + ".", ""]

    def sec(v, np_, c, ph):
        return {r: o["seconds"] for r, o in
                data.get((v, np_, c, ph), {}).items()}

    def calls(v, np_, c, ph):
        recs = data.get((v, np_, c, ph), {})
        return next(iter(recs.values()))["calls"] if recs else None

    def digest_ok(c, np_):
        ds = {(v, o.get("digest")) for (v, n, cc, _), recs in data.items()
              if cc == c and n == np_ and "fast" not in v
              for o in recs.values()}
        vals = {dg for _, dg in ds}
        heads = {dg for v, dg in ds if not v.startswith("before")}
        if not ds:
            return MISSING
        if f"{c}@np{np_}" in UNREPEATABLE or c in UNREPEATABLE:
            return "not compared (does not repeat)"
        if len(vals) == 1:
            return "equal"
        return "equal (head)" if len(heads) == 1 else "DIFFER"

    if serial:
        threaded = any(v != "head" and v.startswith("head")
                       for (v, _, _, _) in data)
        hdr = ["case", "phase", "calls", "before", "head", "head / before"]
        if threaded:
            hdr += ["head 2 threads / 1", "4 / 1", "8 / 1",
                    "fast / bitwise (1)", "fast / bitwise (8)"]
        hdr += ["digests"]
        rows = []
        for c, ph in keys:
            b, h = sec("before", 1, c, ph), sec("head", 1, c, ph)
            if not any(b.values()) and not any(h.values()):
                continue   # a phase that does not run at 1 rank
            row = [c, ph, calls("head", 1, c, ph) or
                   calls("before", 1, c, ph),
                   med_time(list(b.values())),
                   med_time(list(h.values())), ratio(h, b)]
            if threaded:
                row += [ratio(sec(f"head-t{n}", 1, c, ph), h)
                        for n in (2, 4, 8)]
                row += [ratio(sec("head-fast", 1, c, ph), h),
                        ratio(sec("head-fast-t8", 1, c, ph),
                              sec("head-t8", 1, c, ph))]
            rows.append(row + [digest_ok(c, 1)])
        out += [(f"Table \"{title}\" (seconds of the phase over its calls, "
                "median [min, max]; ratios paired per repetition):"), "",
                table(hdr, rows)]
    else:
        nps = sorted({n for (_, n, _, _) in data})
        hdr = ["case", "phase", "ranks", "before", "head", "head / before",
               "head ranks x threads", "time", "/ head at the same cores",
               "digests"]
        rows = []
        for c, ph in keys:
            for np_ in nps:
                b, h = sec("before", np_, c, ph), sec("head", np_, c, ph)
                if not any(b.values()) and not any(h.values()):
                    continue
                rt, tt, cmp_ = "", "", ""
                for t in (4, 2):
                    x = sec(f"head-t{t}", np_, c, ph)
                    if x:
                        rt = f"{np_} x {t}"
                        tt = med_time(list(x.values()))
                        cmp_ = ratio(x, sec("head", np_ * t, c, ph))
                rows.append([c, ph, np_, med_time(list(b.values())),
                             med_time(list(h.values())), ratio(h, b), rt,
                             tt, cmp_, digest_ok(c, np_)])
        out += [(f"Table \"{title}\" (seconds of the phase, rank 0 or the "
                "case's maximum over the ranks as the case reports it):"),
                "", table(hdr, rows)]
    return out


def summarize_b(d, info):
    out = header(d, info, "the benchmark suites")
    out += [("Cases at the baseline's sizes (`m`; `s` in a dry run), one "
            "process per run pinned to one core (threads on cores "
            "0..N-1), the variants of a case adjacent and rotated per "
            "repetition (`scripts/perf/suite/run_ab.sh`). No case of the "
            "suites has a C++ twin (the suites were written for the port; "
            "`SUITE.md` of the campaign), so the C++ column of the "
            "baseline exists only for the explicit step, the cohesive "
            "driver and the static solve (parts A, C, D)."), ""]
    for s in ("solid", "models", "setup"):
        out += suite_tables(d / f"suite-{s}", f"suite_{s}, serial cases")
    for s in ("solid", "setup", "models"):
        out += suite_tables(d / f"mpi-{s}", f"suite_{s}, distributed cases "
                            "at 1, 2, 4, 8 ranks", serial=False)
    out += suite_tables(d / "python", "suite_python.py (Python suite)")
    return out


# ---------------------------------------------------------------- cohesive
def read_chrono(p):
    """Rank 0's Chrono totals: {phase: seconds}."""
    if not p.exists():
        return {}
    rows = list(csv.reader(p.read_text().splitlines(),
                           skipinitialspace=True))
    if len(rows) < 2:
        return {}
    res = {}
    for k, v in zip(rows[0], rows[1], strict=False):
        if k.endswith("nb_rep") or k == "psize":
            continue
        try:
            res[k] = float(v)
        except ValueError:
            pass
    return res


def read_mem(p):
    """Peaks of the memory sampler: per process the largest VmHWM (kiB)."""
    if not p.exists():
        return None
    peaks = {}
    for line in p.read_text().splitlines():
        try:
            o = json.loads(line)
        except json.JSONDecodeError:
            continue
        for pid, _start, _rss, hwm in o.get("procs", []):
            peaks[pid] = max(peaks.get(pid, 0), hwm)
    if not peaks:
        return None
    v = sorted(peaks.values())
    return {"max": v[-1], "sum": sum(v), "min": v[0],
            "median": statistics.median(v), "n": len(v)}


def read_status(p):
    st = {}
    if p.exists():
        for line in p.read_text().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                try:
                    st[k] = json.loads(v)
                except json.JSONDecodeError:
                    st[k] = v
    return st


def load_runs(d):
    """Completed runs of a cohesive part: the last index entry per run."""
    idx = {}
    p = d / "index.jsonl"
    if not p.exists():
        return []
    for line in p.read_text().splitlines():
        o = json.loads(line)
        key = (o["series"], o["h"], o["np"], o["rep"], o["variant"])
        idx[key] = o
    runs = []
    for o in idx.values():
        if o["outcome"] != "ok":
            continue
        rd = d / "runs" / o["campaign"] / o["case"] / f"np{o['np']}" / "rep1"
        if not rd.is_dir():
            continue
        r = dict(o)
        r["chrono"] = read_chrono(rd / "chrono.csv")
        s = rd / "summary.json"
        r["summary"] = json.loads(s.read_text()) if s.exists() else {}
        r["mem"] = read_mem(rd.parent / "rep1.mem.jsonl") or \
            read_mem(rd / "mem.jsonl")
        r["status"] = read_status(rd / "status.txt")
        tets = r["summary"].get("nb_regular_elements_dim3")
        ch = r["chrono"]
        steps = o["steps"]
        cores = o["np"] * o["threads"]
        loop = None
        if "solve_step" in ch and "check_cohesive_stress" in ch:
            loop = ch["solve_step"] + ch["check_cohesive_stress"]
        r["m"] = {
            "wall": r["status"].get("wall_s"),
            "read_mesh": ch.get("read_mesh"),
            "dist_mesh": ch.get("dist_mesh"),
            "init_full": ch.get("init_full"),
            "static": ch.get("static_solve"),
            "before_step": ch.get("before_step"),
            "solve_step": ch.get("solve_step"),
            "check": ch.get("check_cohesive_stress"),
            "energies": ch.get("energies"),
            "loop": loop,
            "step_ms": ch["solve_step"] / steps * 1e3
            if "solve_step" in ch else None,
            "check_ms": ch["check_cohesive_stress"] / steps * 1e3
            if "check_cohesive_stress" in ch else None,
            "energies_ms": ch["energies"] / steps * 1e3
            if "energies" in ch else None,
            "core_us": loop * cores / tets / steps * 1e6
            if loop and tets else None,
            "peak_max_mib": r["mem"]["max"] / 1024 if r["mem"] else None,
            "peak_sum_mib": r["mem"]["sum"] / 1024 if r["mem"] else None,
            "container_mib": r["status"]["memory_peak_bytes"] / 2 ** 20
            if isinstance(r["status"].get("memory_peak_bytes"), int)
            else None,
            "tets": tets,
            "dofs": (r["summary"].get("rust_static_matrix") or {})
            .get("order"),
            "infog19": (r["summary"].get("rust_static_solver_stats") or {})
            .get("infog19_mb_sum_allocated"),
        }
        runs.append(r)
    return runs


CODE_NAMES = {"cpp": "C++", "cppeig": "C++ SparseLU", "before": "port before",
              "head": "port", "before-eig": "port before SparseLU",
              "head-eig": "port SparseLU", "head-release": "port, factors "
              "released"}


def code_name(v, np_):
    if "-t" in v:
        base, t = v.rsplit("-t", 1)
        return f"{CODE_NAMES.get(base, base)} {np_} x {t}"
    return CODE_NAMES.get(v, v)


def by_point(runs, series):
    """{(h, np): {variant: {rep: run}}}."""
    res = defaultdict(lambda: defaultdict(dict))
    for r in runs:
        if r["series"] == series:
            res[(r["h"], r["np"])][r["variant"]][r["rep"]] = r
    return res


def hkey(h):
    try:
        return -float(h)
    except ValueError:
        return 0.0


def metric(reps, k):
    return {rep: r["m"][k] for rep, r in reps.items()
            if r["m"].get(k) is not None}


def cohesive_series_tables(runs, series, title, steps_note=""):
    pts = by_point(runs, series)
    if not pts:
        return [f"Series `{series}` ({title}): missing.", ""]
    out = [f"## Series `{series}`: {title}", ""]
    if steps_note:
        out += [steps_note, ""]
    # per point, absolute values (the rehearsal tables' columns)
    hdr = ["h", "ranks", "code", "runs", "wall s", "init_full s",
           "static solve s", "`solve_step` ms/step", "check ms/step",
           "core-µs/el/step", "loop efficiency", "largest rank peak MiB",
           "Σ rank peaks MiB"]
    rows = []
    one_rank = {}
    for (h, np_), vs in pts.items():
        if np_ == 1:
            for v, reps in vs.items():
                c = metric(reps, "core_us")
                if c:
                    one_rank[(h, v)] = statistics.median(c.values())
    for (h, np_) in sorted(pts, key=lambda k: (hkey(k[0]), k[1])):
        vs = pts[(h, np_)]
        for v in sorted(vs, key=lambda x: (x != "cpp", x)):
            reps = vs[v]
            c = metric(reps, "core_us")
            base = v.rsplit("-t", 1)[0]
            eff = MISSING
            if c and (h, base) in one_rank:
                eff = f"{one_rank[(h, base)] / statistics.median(c.values()):.0%}"
            rows.append([h, np_, code_name(v, np_), len(reps)]
                        + [med(list(metric(reps, k).values())) for k in
                           ("wall", "init_full", "static", "step_ms",
                            "check_ms", "core_us")]
                        + [eff] + [med(list(metric(reps, k).values()))
                                   for k in ("peak_max_mib",
                                             "peak_sum_mib")])
    out += [("Table \"Points\" (rank 0's Chrono phases; core-µs per element "
            "and step = (`solve_step` + check) × cores / tetrahedra / "
            "steps; loop efficiency = core-µs at 1 rank / core-µs at P, "
            "same code; peaks of the memory sampler):"), "",
            table(hdr, rows)]
    # paired ratios, as "Port / upstream, series ws"
    for num, den, name in (("head", "cpp", "Port / upstream"),
                           ("head", "before", "Port / port before"),
                           ("before", "cpp", "Port before / upstream")):
        hdr = ["h", "ranks", "loop", "`solve_step`", "check",
               "static solve", "`init_full`", "wall"]
        rows = []
        for (h, np_) in sorted(pts, key=lambda k: (hkey(k[0]), k[1])):
            vs = pts[(h, np_)]
            if num not in vs or den not in vs:
                continue
            a, b = vs[num], vs[den]
            rows.append([h, np_] + [ratio(metric(a, k), metric(b, k))
                                    for k in ("loop", "solve_step", "check",
                                              "static", "init_full",
                                              "wall")])
        if rows:
            out += [(f"Table \"{name}, series {series}\" (paired: the runs "
                    "of each repetition; median [min, max]):"), "",
                    table(hdr, rows)]
    # ranks x threads against ranks
    hdr = ["h", "configuration", "loop s", "against 8 ranks",
           "against the same ranks, 1 thread"]
    rows = []
    for (h, np_), vs in sorted(pts.items(), key=lambda k: (hkey(k[0][0]),
                                                            k[0][1])):
        for v, reps in vs.items():
            if "-t" not in v:
                continue
            t = int(v.rsplit("-t", 1)[1])
            base = v.rsplit("-t", 1)[0]
            ref8 = pts.get((h, np_ * t), {}).get(base, {})
            rows.append([h, f"{np_} ranks x {t} threads",
                         med(list(metric(reps, "loop").values())),
                         ratio(metric(reps, "loop"), metric(ref8, "loop")),
                         ratio(metric(reps, "loop"),
                               metric(vs.get(base, {}), "loop"))])
    if rows:
        out += [("Table \"Ranks × threads\" (the port, loop = `solve_step` "
                "+ check, paired ratios):"), "", table(hdr, rows)]
    return out


def strong_weak(runs, series_list, studies):
    out = []
    for name, spec in studies:
        pts = [(h, int(n)) for h, n in (p.split(":") for p in spec)]
        rows = []
        for v, label in (("head", "port"), ("before", "port before"),
                         ("cpp", "upstream")):
            vals = []
            for h, np_ in pts:
                c = []
                for s in series_list:
                    c = list(metric(by_point(runs, s)[(h, np_)]
                                    .get(v, {}), "core_us").values()) or c
                vals.append(statistics.median(c) if c else None)
            if vals[0] is None:
                continue
            if not any(vals[1:]):
                continue
            eff = [f"{vals[0] / x:.2f}" if x else MISSING for x in vals[1:]]
            rows.append([name, " / ".join(str(n) for _, n in pts[1:]),
                         label, " / ".join(eff)])
        if rows:
            out += table(["study", "ranks", "code",
                          "efficiency (core-µs at the first point / at P)"],
                         rows).splitlines()
    return out + [""] if out else []


def check_lines(d):
    return _check_lines(d) + binding_lines(d)


def _check_lines(d):
    p = d / "checks.jsonl"
    if not p.exists():
        return ["Checks: none recorded.", ""]
    n, bad, cmp_ = 0, 0, 0
    for line in p.read_text().splitlines():
        o = json.loads(line)
        n += 1
        bad += bool(o["failures"])
        cmp_ += sum(len(v) > 1 for v in o["compared"].values())
    return [(f"Checks (`checks.jsonl`): {n} points; at every point the "
            "thread count and precision of each port run (`rust_exec` of "
            "`summary.json`) against the request; per-step energies and "
            "insertions (`energies.csv`, `insertions.csv`, 17 digits) "
            "identical across the bitwise variants at 1 rank (C++ "
            "included) and in the SparseLU series at every rank count; "
            f"{cmp_} comparisons made, {bad} points failed. The MUMPS runs "
            "at P > 1 are compared on time only (the distributed solve "
            "does not repeat run to run)."), ""]


def summarize_c(d, info):
    runs = load_runs(d)
    out = header(d, info, "the cohesive benchmark at small and mid sizes, "
                 "1 to 8 ranks")
    out += [("C++: the strict install with the JOSS emulation (`FNTE = "
            "joss`), MUMPS 5.7.3; the port's MPI build (`mpi,scotch,"
            "mumps`); the same mesh and Scotch partition for both "
            "(`c1prep_campaign.sh`); ranks on cores 0..P-1; one container "
            "per run; the variants of a point adjacent, rotated per "
            "repetition, the point order reversed every other repetition."),
            ""]
    out += check_lines(d)
    for s, t in (("ws", ("500 steps before the first insertion (the series "
                  "`ws` of the scaling study)")),
                 ("ins", "the insertion regime on h = 0.12, 1,500 steps"),
                 ("full12", ("the full schedule on h = 0.12, 5,000 steps "
                  "(the insertion-check tables)")),
                 ("reh6", ("the full schedule on h = 0.06 (\"The mid-size "
                  "rehearsal\")")),
                 ("reh4", ("static solve + 300 pre-insertion steps on "
                  "h = 0.04 (\"The mid-size rehearsal\")")),
                 ("eig", ("the port's SparseLU static solve at 1 to 8 ranks "
                  "(bitwise check of the distributed runs)"))):
        out += cohesive_series_tables(runs, s, t)
    out += ["## Weak and strong scaling", ""]
    out += strong_weak(runs, ["ws"], [
        ("weak A (≈ 23 k tets per rank)",
         ["0.12:1", "0.0925:2", "0.0756:4", "0.06:8"]),
        ("weak B (≈ 45 k tets per rank)",
         ["0.0925:1", "0.0756:2", "0.06:4", "0.0476:8"]),
        ("strong, h = 0.06, before insertion",
         ["0.06:1", "0.06:2", "0.06:4", "0.06:8"]),
        ("strong, h = 0.12, before insertion (the dry run's mesh)",
         ["0.12:1", "0.12:2", "0.12:4", "0.12:8"])])
    out += strong_weak(runs, ["ins"], [
        ("strong, h = 0.12, with insertion",
         ["0.12:1", "0.12:2", "0.12:4", "0.12:8"])])
    return out


def summarize_d(d, info):
    runs = load_runs(d)
    out = header(d, info, "static solves and solvers")
    out += [(f"The head's SparseLU factorisation uses threads: "
            f"{info.get('sparselu_threaded', 'unknown')} (from the "
            "translation's source; threaded variants are timed only then)."),
            ""]
    out += check_lines(d)
    pts = by_point(runs, "eig1")
    hdr = ["h", "DOFs", "C++ SparseLU (strict)", "port before", "port",
           "port / C++", "port / port before", ("peak RSS C++ / before / "
           "port MiB")]
    rows = []
    for (h, np_) in sorted(pts, key=lambda k: hkey(k[0])):
        vs = pts[(h, np_)]
        c, b, p = (vs.get(x, {}) for x in ("cppeig", "before-eig",
                                            "head-eig"))
        dofs = next((r["m"]["dofs"] for r in p.values()), None)
        rows.append([h, f"{dofs:,}" if dofs else MISSING]
                    + [med(list(metric(x, "static").values()))
                       for x in (c, b, p)]
                    + [ratio(metric(p, "static"), metric(c, "static")),
                       ratio(metric(p, "static"), metric(b, "static")),
                       " / ".join(med(list(metric(x, "container_mib")
                                           .values()), 3)
                                  for x in (c, b, p))])
        for t in (2, 4, 8):
            x = vs.get(f"head-eig-t{t}")
            if x:
                rows.append([h, f"port, {t} threads",
                             "", "", med(list(metric(x, "static").values())),
                             ratio(metric(x, "static"), metric(c, "static")),
                             ratio(metric(x, "static"), metric(p, "static"))
                             + " (against 1 thread)", ""])
    out += [("Table \"The SparseLU translation against upstream's "
            "SparseLU\" (the driver's `static_solve` phase: assembly, "
            "analysis, factorisation, solve; 1 rank, 10 steps; s):"), "",
            table(hdr, rows) if rows else "missing\n"]
    pts = by_point(runs, "mumps")
    hdr = ["h", "ranks", "C++", "port before", "port", "port / C++",
           "port / port before"]
    rows = []
    for (h, np_) in sorted(pts, key=lambda k: (hkey(k[0]), k[1])):
        vs = pts[(h, np_)]
        c, b, p = (vs.get(x, {}) for x in ("cpp", "before", "head"))
        rows.append([h, np_] + [med(list(metric(x, "static").values()))
                                for x in (c, b, p)]
                    + [ratio(metric(p, "static"), metric(c, "static")),
                       ratio(metric(p, "static"), metric(b, "static"))])
    out += [("Table \"MUMPS in the cohesive benchmark\" (static solve "
            "phase, s; the same MUMPS 5.7.3 on the same matrix, PORD):"), "",
            table(hdr, rows) if rows else "missing\n"]
    out += suite_tables(d / "suite-solid", "suite_solid, the sparse "
                        "solvers' phases (analysis, factorize, "
                        "back_substitution)")
    out += suite_tables(d / "mpi-solid", "suite_solid static_linear_mumps "
                        "at 1, 2, 4 ranks", serial=False)
    return out


def summarize_e(d, info):
    runs = load_runs(d)
    out = header(d, info, "memory")
    out += check_lines(d)
    pts = by_point(runs, "mem4")
    hdr = ["ranks", "code", "largest rank MiB", "Σ rank peaks MiB",
           "container MiB", "static: largest / Σ (port) MiB",
           "port model before static: largest / Σ MiB", "MUMPS INFOG(19) MB",
           "besides MUMPS, bytes per tetrahedron"]
    rows = []
    for (h, np_) in sorted(pts, key=lambda k: (hkey(k[0]), k[1])):
        vs = pts[(h, np_)]
        inf = next((r["m"]["infog19"] for v in ("head", "before")
                    for r in vs.get(v, {}).values()
                    if r["m"]["infog19"]), None)
        for v in sorted(vs, key=lambda x: (x != "cpp", x)):
            reps = vs[v]
            any_r = next(iter(reps.values()))
            mk = any_r["summary"].get("rust_memory_kib") or {}

            def pair(field, mk=mk):
                f = mk.get(field)
                return (f"{f['max'] / 1024:,.0f} / {f['sum'] / 1024:,.0f}"
                        if f else "—")
            tets = any_r["m"]["tets"]
            lb = MISSING
            sums = list(metric(reps, "peak_sum_mib").values())
            if sums and tets and inf:
                lb = sig((statistics.median(sums) * 2 ** 20 - inf * 1e6)
                         / tets)
            rows.append([np_, code_name(v, np_)]
                        + [med(list(metric(reps, k).values())) for k in
                           ("peak_max_mib", "peak_sum_mib",
                            "container_mib")]
                        + [pair("hwm_static"), pair("rss_before_static"),
                           inf if inf else MISSING, lb])
        out += [(f"Table \"Memory, h = {h}\" (peaks of the sampler: "
                "`memsample.py`, VmHWM of every rank every 0.25 s; "
                "container = the cgroup's `memory.peak`; the port's own "
                "records in `summary.json`; the last column = (Σ rank "
                "peaks − the port's `INFOG(19)`) / tetrahedra, as the "
                "document's table \"Memory besides MUMPS\"; \"factors "
                "released\" = `rust_release_static_solver = true`):"), ""]
        out += [table(hdr, rows)]
        rows = []
    return out


def summarize_f(d, info):
    runs = load_runs(d)
    out = header(d, info, "set-up, distribution and the MPI layer")
    out += check_lines(d)
    pts = by_point(runs, "setup")
    hdr = ["h", "ranks", "code", "read_mesh s", "dist_mesh s",
           "init_full s", "before_step s", "static solve s"]
    rows = []
    rat = []
    for (h, np_) in sorted(pts, key=lambda k: (hkey(k[0]), k[1])):
        vs = pts[(h, np_)]
        for v in sorted(vs, key=lambda x: (x != "cpp", x)):
            rows.append([h, np_, code_name(v, np_)]
                        + [med(list(metric(vs[v], k).values())) for k in
                           ("read_mesh", "dist_mesh", "init_full",
                            "before_step", "static")])
        if "head" in vs and "cpp" in vs:
            rat.append([h, np_] + [ratio(metric(vs["head"], k),
                                         metric(vs["cpp"], k))
                                   for k in ("read_mesh", "dist_mesh",
                                             "init_full", "before_step",
                                             "static")])
    out += [("Table \"Set-up costs\" (the cohesive driver, 1 step; rank 0's "
            "Chrono phases):"), "", table(hdr, rows)]
    if rat:
        out += ["Table \"Set-up, port / upstream\" (paired):", "",
                table(["h", "ranks", "read_mesh", "dist_mesh", "init_full",
                       "before_step", "static solve"], rat)]
    out += suite_tables(d / "dist-l", "Distribution (`suite_setup` "
                        "`dist_mesh_tet4`, size `l`: 1,296,000 Tet4)",
                        serial=False)
    out += mpilayer_tables(d / "mpilayer")
    out += [("The I/O cases (mesh reading and writing, dumpers, parser) and "
            "the distributed exchanges are cases of `suite_setup`: their "
            "tables are in part B."), ""]
    return out


def mpilayer_tables(m):
    p = m / "separate.tsv"
    if not p.exists():
        return ["MPI layer: missing.", ""]
    vals = defaultdict(dict)
    for row in csv.DictReader(p.read_text().splitlines(), delimiter="\t"):
        try:
            us = float(row["us"])
        except ValueError:
            continue
        vals[(row["btl"], row["np"], row["kind"], int(row["bytes"]),
              row["prog"])][int(row["rep"])] = us
    keys = sorted({k[:4] for k in vals}, key=lambda k: (k[0] != "sm", k[2],
                                                        k[1], k[3]))
    hdr = ["Transport", "Case", "Raw MPI (C)", "akantu-comm before",
           "akantu-comm head", "head / raw, paired", "before / raw",
           "head / before"]
    rows = []
    for btl, np_, kind, by in keys:
        def g(prog, k=(btl, np_, kind, by)):
            return vals.get(k + (prog,), {})
        size = (f"{by // 2 ** 20} MiB" if by >= 2 ** 20 else
                f"{by // 1024} KiB" if by >= 1024 else f"{by} B")
        case = (f"ping-pong {size}" if kind == "pingpong" else
                f"halo, {np_} ranks, {size}")
        rows.append(["shared memory" if btl == "sm" else "TCP", case]
                    + [med(list(g(x).values())) for x in
                       ("raw", "before", "head")]
                    + [ratio(g("head"), g("raw")),
                       ratio(g("before"), g("raw")),
                       ratio(g("head"), g("before"))])
    out = [("Table \"Method A, separate launches\" (µs, median [min, max]; "
           "ping-pong: half round trip; halo: one exchange with every "
           "other rank, slowest rank):"), "", table(hdr, rows)]
    cal = m / "calibrated.txt"
    if cal.exists():
        bv = defaultdict(dict)
        for ln in cal.read_text().splitlines():
            kv = dict(x.split("=", 1) for x in ln.split() if "=" in x)
            try:
                bv[(kv["btl"], kv["np"], kv["kind"], int(kv["len"]),
                    kv["prog"])][int(kv["rep"])] = kv
            except (KeyError, ValueError):
                continue
        rows = []
        for btl, np_, kind, by in sorted(
                {k[:4] for k in bv}, key=lambda k: (k[0] != "sm", k[2], k[1],
                                                    k[3])):
            for prog in ("before", "head"):
                reps = bv.get((btl, np_, kind, by, prog), {})
                if not reps:
                    continue

                def col(f, reps=reps):
                    return med([float(v[f]) for v in reps.values()])
                rows.append(["shared memory" if btl == "sm" else "TCP",
                             f"{kind} {by} B, {np_} ranks", prog,
                             col("raw_med_us"), col("akantu_med_us"),
                             col("calib_med_us"), col("ratio_med"),
                             col("backend_ratio_med")])
        out += [("Table \"Method B, one job\" (`mpi_bench calibrated`, 9 "
                 "interleaved blocks per job; µs; backend ratio = (akantu − "
                 "calibration) / raw, the MPI tier's gated figure):"), "",
                table(["Transport", "Case", "variant", "raw", "akantu-comm",
                       "calibration", "akantu / raw", "backend ratio"],
                      rows)]
    return out


def strong_scaling(pts):
    """Rows of the strong-scaling table: every configuration by cores."""
    rows_in = []
    for (h, np_), vs in pts.items():
        for v, reps in vs.items():
            t = int(v.rsplit("-t", 1)[1]) if "-t" in v else 1
            rows_in.append((np_ * t, v == "cpp" or v.startswith("cpp"),
                            np_, t, v, reps))
    rows_in.sort(key=lambda x: (x[1], x[0], -x[2]))
    base = {}
    for cores, is_cpp, np_, t, v, reps in rows_in:
        c = list(metric(reps, "core_us").values())
        if c and (is_cpp not in base or cores < base[is_cpp][0]):
            base[is_cpp] = (cores, statistics.median(c))
    rows = []
    for cores, is_cpp, np_, t, v, reps in rows_in:
        def mm(k, reps=reps):
            return med(list(metric(reps, k).values()))
        c = list(metric(reps, "core_us").values())
        eff = MISSING
        if c and is_cpp in base:
            eff = f"{base[is_cpp][1] / statistics.median(c):.0%}"
        rows.append(["C++" if is_cpp else "port", f"{np_} x {t}", cores,
                     mm("wall"), mm("read_mesh"), mm("dist_mesh"),
                     mm("init_full"), mm("static"), mm("step_ms"),
                     mm("check_ms"), mm("energies_ms"), mm("loop"),
                     mm("core_us"), eff])
    return rows, base


def binding_lines(d):
    files = sorted((d / "runs").glob("*.bindings.txt"))
    if not files:
        return ["Core bindings: none recorded.", ""]
    bad = [f.name for f in files if "BINDING" in f.read_text()]
    out = [f"Core bindings (`runs/*.bindings.txt`, rank -> cores read from "
           f"/proc during each run): {len(files)} runs, "
           f"{len(files) - len(bad)} as planned (rank r on the r-th block of "
           "its thread count in the core list ordered NUMA node by NUMA "
           "node, every block inside one NUMA node)"
           + (f"; not as planned: {', '.join(bad)}" if bad else "") + ".", ""]
    return out


def summarize_g(d, info):
    runs = load_runs(d)
    out = header(d, info, "the full-size run")
    out += check_lines(d)
    if (d / "plan.txt").exists():
        out += ["The plan printed before the runs (`plan.txt`):", "",
                "```text", (d / "plan.txt").read_text().rstrip(), "```", ""]
    pts = by_point(runs, "full")
    rows, base = strong_scaling(pts)
    out += ["Table \"Strong scaling\" (one row per configuration, ranks x "
            "threads; rank 0's Chrono phases, s, per step in ms; core-µs per "
            "element and step = (`solve_step` + check) × cores / tetrahedra / "
            "steps; loop efficiency = core-µs at the smallest core count of "
            "the same code"
            + "".join(f" ({'C++' if k else 'port'}: {v[0]} cores)"
                      for k, v in sorted(base.items()))
            + " / core-µs here):", "",
            table(["code", "ranks x threads", "cores", "wall s", "read_mesh",
                   "dist_mesh", "init_full", "static solve",
                   "`solve_step` ms/step", "check ms/step",
                   "energies ms/step", "loop s", "core-µs/el/step",
                   "loop efficiency"], rows)]
    hdr = ["code", "ranks", "wall s", "read_mesh", "dist_mesh", "init_full",
           "static solve", "before_step", "`solve_step` s (ms/step)",
           "check s (ms/step)", "energies s (ms/step)", "loop s",
           "core-µs/el/step"]
    rows, rat, mem, mumps = [], [], [], []
    for (h, np_) in sorted(pts, key=lambda k: -k[1]):
        vs = pts[(h, np_)]
        for v in sorted(vs, key=lambda x: (x == "cpp", x)):
            reps = vs[v]

            def mm(k, reps=reps):
                return med(list(metric(reps, k).values()))
            rows.append([code_name(v, np_), np_, mm("wall"), mm("read_mesh"),
                         mm("dist_mesh"), mm("init_full"), mm("static"),
                         mm("before_step"),
                         f"{mm('solve_step')} ({mm('step_ms')})",
                         f"{mm('check')} ({mm('check_ms')})",
                         f"{mm('energies')} ({mm('energies_ms')})",
                         mm("loop"), mm("core_us")])
            r0 = next(iter(reps.values()))
            mk = r0["summary"].get("rust_memory_kib") or {}
            hs = mk.get("hwm_static")
            m = r0["mem"]
            mem.append([code_name(v, np_), np_,
                        sig(m["max"] / 2 ** 20) if m else MISSING,
                        sig(m["sum"] / 2 ** 20) if m else MISSING,
                        sig(r0["m"]["container_mib"] / 1024)
                        if r0["m"]["container_mib"] else MISSING,
                        f"{sig(hs['max'] / 2 ** 20)} / "
                        f"{sig(hs['sum'] / 2 ** 20)}" if hs else "—",
                        f"{sig(m['min'] / 2 ** 20)} / "
                        f"{sig(m['median'] / 2 ** 20)} / "
                        f"{sig(m['max'] / 2 ** 20)}" if m else MISSING])
            st = r0["summary"].get("rust_static_solver_stats") or {}
            mx = r0["summary"].get("rust_static_matrix") or {}
            if st:
                mumps.append([f"{np_} ({code_name(v, np_)})",
                              st.get("infog7_ordering"),
                              f"{st.get('infog29_factor_entries_effective', 0):.3g}",
                              (f"{st.get('infog19_mb_sum_allocated', 0):,} / "
                              f"{st.get('infog18_mb_max_allocated', 0):,}"),
                              (f"{st.get('infog22_mb_sum_used', 0):,} / "
                              f"{st.get('infog21_mb_max_used', 0):,}"),
                              f"{st.get('rinfog3_flops_elimination', 0):.3g}",
                              st.get("retries"),
                              (f"{mx.get('nnz_sum', 0):,} / "
                              f"{mx.get('nnz_max', 0):,}")])
        if "head" in vs and "cpp" in vs:
            a, b = vs["head"], vs["cpp"]
            rat.append([np_] + [ratio(metric(a, k), metric(b, k)) for k in
                                ("wall", "init_full", "static", "dist_mesh",
                                 "solve_step", "check", "energies", "loop",
                                 "peak_max_mib", "peak_sum_mib")])
    out += [("Table \"Phases\" (rank 0's Chrono totals, s; per step = total "
            "/ steps):"), "", table(hdr, rows)]
    out += ["Table \"Port / upstream at the same rank count\":", "",
            table(["ranks", "wall", "`init_full`", "static solve",
                   "`dist_mesh`", "`solve_step`", "check", "energies",
                   "loop", "largest rank peak", "total peak"], rat)]
    out += [("Table \"Memory\" (GiB; sampler = `memsample.py`, VmHWM of "
            "every rank; container = the cgroup's `memory.peak`; static = "
            "the port's `VmHWM` at the end of the static solve):"), "",
            table(["code", "ranks", "largest rank", "total (sampler)",
                   "container", "static: largest rank / total",
                   "per-rank peak min / median / max"], mem)]
    if mumps:
        out += [("Table \"MUMPS\" (the port's records; `INFOG` in MB, flops "
                "= `RINFOG(3)`):"), "",
                table(["ranks", "ordering `INFOG(7)`",
                       "factor entries `INFOG(29)`",
                       "allocated sum / largest rank `INFOG(19)` / `(18)`",
                       "used sum / largest `INFOG(22)` / `(21)`",
                       "elimination flops", "retries",
                       "matrix entries sum / largest rank"], mumps)]
    return out


PARTS = {"A": summarize_a, "B": summarize_b, "C": summarize_c,
         "D": summarize_d, "E": summarize_e, "F": summarize_f,
         "G": summarize_g}


def machine_time(d):
    """Sum of the run times recorded by the part (cohesive index) and the
    first / last progress time."""
    p = d / "progress.log"
    if not p.exists():
        return []
    ts = [ln.split()[0] for ln in p.read_text().splitlines() if ln]
    out = [(f"Progress log: first line {ts[0]}, last line {ts[-1]} "
           f"({len(ts)} lines).")] if ts else []
    idx = d / "index.jsonl"
    if idx.exists():
        w = sum(json.loads(ln)["wall_s"] for ln in
                idx.read_text().splitlines() if ln)
        out.append(f"Cohesive runs: {w / 3600:.2f} h of run time in "
                   "`index.jsonl` (all attempts).")
    return out + [""]


def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help"):
        print(__doc__)
        return 2
    d = Path(argv[1])
    outp = Path(argv[argv.index("--out") + 1]) if "--out" in argv \
        else d / "summary.md"
    info = read_provenance(d)
    part = info.get("part", d.name)
    if part not in PARTS:
        print(f"summarize.py: unknown part {part!r}", file=sys.stderr)
        return 2
    lines = PARTS[part](d, info) + machine_time(d)
    outp.write_text("\n".join(lines).rstrip() + "\n")
    print(f"summarize.py: {outp}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
