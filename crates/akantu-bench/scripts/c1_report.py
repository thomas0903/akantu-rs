#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the full-size (C1) session written up.
"""Pack the full-size session's small result files and print its tables.

    python3 -I c1_report.py pack RESULTS OUTDIR   # RESULTS = the session's results directory
    python3 -I c1_report.py tables OUTDIR          # Markdown tables from OUTDIR only

`pack` copies, for the six campaigns of the session (the 20-step test at 64
ranks and the five timed points), `summary.md`, `summary.json` and
`provenance.txt`, plus the session's `smoke.log` and, from its `state.json`,
the per-point record the tables read (`POINT_KEYS`; the session log and the
rest of the state, scheduling data, are digested in `sources.sha256` but not
copied), replacing the home directory of the host's account
(`/home/<name>`) by `/home/<user>`; it writes `extract.json` with what the
tables need from the per-run files that are not committed (Chrono phases,
the run records and build record of the C++ harness, the Rust binary
digest and container limits, per-rank counts and memory, the memory
sampler's per-rank peaks) and `sources.sha256` (SHA-256 of every source
file read, before redaction). It then refuses (exit 1, nothing left
behind) if any written file holds an IPv4 address, an EC2 instance id, a
12-digit number next to the word "account", an AWS host name, ARN,
security group, image id or key file, a `/home/<name>` other than
`/home/<user>`, a `/Users/` path or the names `ubuntu` / `ec2-user`.

`tables` reads only OUTDIR (the committed files) and prints the tables of
`docs/BENCHMARKS.md`, "The full-size run".
"""

import hashlib
import json
import re
import shutil
import statistics
import sys
from pathlib import Path

CAMPAIGNS = ("test-np64", "rust-np64-a1", "cpp-np64-a1", "rust-np32-a1", "cpp-np32-a1", "rust-np8-a1")
TIMED = CAMPAIGNS[1:]
TOP_FILES = ("session.log", "state.json", "smoke.log")  # read and digested
COPIED_TOP_FILES = ("smoke.log",)
# The fields of each state.json point that `tables` reads, plus what names the point.
POINT_KEYS = ("slot", "code", "np", "outcome", "hours", "estimate_h", "campaign")
CAMPAIGN_FILES = ("summary.md", "summary.json", "provenance.txt")
HOME_RE = re.compile(r"/home/(?!<user>)[A-Za-z0-9._-]+")
FORBIDDEN = (
    ("IPv4 address", re.compile(r"(?<![\d.])(?:\d{1,3}\.){3}\d{1,3}(?![\d.])")),
    ("EC2 instance id", re.compile(r"\bi-[0-9a-f]{8,17}\b")),
    ("account id", re.compile(r"(?i)account[^\n]{0,40}?\b\d{12}\b")),
    ("AWS host name", re.compile(r"\bip-\d+-\d+-\d+-\d+\b|amazonaws\.com|compute\.internal")),
    ("ARN, security group, image, key file",
     re.compile(r"\barn:aws|\bsg-[0-9a-f]{8,}|\bami-[0-9a-f]{8,}|\.pem\b")),
    ("home directory", HOME_RE),
    ("user name", re.compile(r"/Users/|\bubuntu\b|ec2-user")),
)
# Run matrix of docs/BENCHMARKS.md "Full-size (C1) extrapolation from the
# mid-size rehearsal" (c1prep_extrapolate.py): set-up and loop
# hours before the +30 % margin, and the loop's core-µs/el/step with insertion;
# the session's estimates (state.json) add +30 % and, at 64 / 32 ranks, +50 % of
# the loop for memory bandwidth (docs/TESTING.md).
PLAN = {
    ("rust", 64): (0.118, 0.27, 2.83), ("cpp", 64): (0.133, 0.313, 3.29),
    ("rust", 32): (0.165, 0.316, 1.66), ("cpp", 32): (0.179, 0.517, 2.71),
    ("rust", 8): (0.327, 0.927, 1.22), ("cpp", 8): (0.352, 1.55, 2.03),
}
THREAD_ENV = ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "SCOTCH_PTHREAD_NUMBER", "AKA_SEED",
              "OMPI_MCA_btl", "OMPI_MCA_hwloc_base_binding_policy", "AKANTU_RS_FILL_NODES_TO_ELEMENTS")


def sha256(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def chrono(d):
    f = d / "chrono.csv"
    if not f.exists():
        return {}
    lines = f.read_text().splitlines()
    hdr = [h.strip().strip('"') for h in lines[0].split(",")]
    return dict(zip(hdr, (float(v) for v in lines[1].split(","))))


def kv_file(p):
    out = {}
    if p.exists():
        for line in p.read_text().splitlines():
            if "=" in line:
                k, v = line.split("=", 1)
                try:
                    out[k] = json.loads(v)
                except ValueError:
                    out[k] = v
    return out


def sampler_rank_peaks(p):
    """Per process: the largest VmHWM over the samples (KiB), sorted."""
    peaks = {}
    if not p.exists():
        return []
    for line in p.read_text().splitlines():
        rec = json.loads(line)
        for proc in rec.get("procs", []):
            pid, hwm = proc[0], proc[3]
            peaks[pid] = max(peaks.get(pid, 0), hwm)
    return sorted(peaks.values())


def run_dirs(camp):
    for case_dir in sorted(p for p in camp.iterdir() if p.is_dir() and p.name in ("rust", "cpp")):
        for np_dir in sorted(case_dir.iterdir()):
            if np_dir.is_dir():
                for rep in sorted(p for p in np_dir.iterdir() if p.is_dir()):
                    yield case_dir.name, int(np_dir.name[2:]), rep


def extract_run(case, np_, d, sources, root):
    def note(p):
        if p.exists():
            sources[str(p.relative_to(root))] = sha256(p)
        return p

    note(d / "chrono.csv")
    run = {"case": case, "np": np_, "status": kv_file(note(d / "status.txt")), "chrono": chrono(d)}
    for n in ("energies.csv", "insertions.csv", "summary.json", "out.txt", "err.txt", "mem.jsonl"):
        note(d / n)
    if case == "cpp":
        rj = json.loads(note(d / "run.json").read_text())
        run["run"] = {k: rj.get(k) for k in ("binary_sha256", "build_record_sha256", "config", "container_cpus",
                                            "container_memory_max", "container_memory_peak_bytes",
                                            "container_oom_kills", "exit_status", "fill_nodes_to_elements",
                                            "extra_parameters", "image_id", "loadavg_start", "loadavg_end",
                                            "wall_s", "launcher", "host")}
        run["run"]["environment"] = {k: rj.get("environment", {}).get(k) for k in THREAD_ENV}
        br = json.loads(note(d / "build-record.json").read_text())
        run["build_record"] = {"binary": br.get("binary"), "compiler": br.get("compiler"),
                               "config": br.get("config"), "install": br.get("install"),
                               "install_ready": br.get("install_ready"), "image_id": br.get("image_id"),
                               "libakantu": {k: br.get("libakantu", {}).get(k) for k in ("build_id", "sha256", "size")},
                               "built_utc": br.get("built_utc")}
    else:
        run["binary_sha256"] = note(d / "binary.sha256").read_text().split()[0]
        run["limits"] = note(d / "limits.txt").read_text().split("\n")
        run["limits"] = [x for x in run["limits"] if x]
        run["wall_txt"] = float(note(d / "wall.txt").read_text())
    ranks = []
    for f in sorted(d.glob("summary_rank*.json"), key=lambda p: int(p.stem[len("summary_rank"):])):
        note(f)
        r = json.loads(f.read_text())
        ranks.append({k: r.get(k) for k in ("rank", "nb_nodes", "nb_owned_nodes", "nb_regular_elements_not_ghost",
                                            "nb_elements_dim3_ghost", "nb_cohesive_not_ghost", "rust_memory_kib",
                                            "rust_static_matrix_nnz") if k in r})
    run["ranks"] = ranks
    run["sampler_rank_peaks_kib"] = sampler_rank_peaks(d / "mem.jsonl")
    return run


def redact(text):
    return HOME_RE.sub("/home/<user>", text)


def check_clean(outdir):
    bad = []
    for p in sorted(outdir.rglob("*")):
        if p.is_file():
            text = p.read_text(errors="replace")
            for what, rx in FORBIDDEN:
                for m in rx.finditer(text):
                    bad.append(f"{p.relative_to(outdir)}: {what}: {m.group(0)!r}")
    return bad


def pack(results, outdir):
    if outdir.exists():
        print(f"refused: {outdir} exists", file=sys.stderr)
        return 2
    tmp = outdir.with_name(outdir.name + ".tmp")
    if tmp.exists():
        shutil.rmtree(tmp)
    tmp.mkdir(parents=True)
    sources = {}
    for n in TOP_FILES:
        src = results / n
        sources[n] = sha256(src)
        if n in COPIED_TOP_FILES:
            (tmp / n).write_text(redact(src.read_text()))
    st = json.loads((results / "state.json").read_text())
    points = [{k: p.get(k) for k in POINT_KEYS} for p in st["points"]]
    (tmp / "state.json").write_text(redact(json.dumps({"points": points}, indent=1)) + "\n")
    extract = {"format": "c1_report extract v1", "campaigns": {}}
    for c in CAMPAIGNS:
        camp = results / "campaigns" / c
        (tmp / c).mkdir()
        for n in CAMPAIGN_FILES:
            src = camp / n
            sources[f"campaigns/{c}/{n}"] = sha256(src)
            (tmp / c / n).write_text(redact(src.read_text()))
        runs = [extract_run(case, np_, d, sources, results) for case, np_, d in run_dirs(camp)]
        extract["campaigns"][c] = {"runs": runs}
    (tmp / "extract.json").write_text(redact(json.dumps(extract, indent=1, sort_keys=True)) + "\n")
    (tmp / "sources.sha256").write_text("".join(f"{h}  {p}\n" for p, h in sorted(sources.items())))
    bad = check_clean(tmp)
    if bad:
        print("refused: identifying data left after redaction:\n  " + "\n  ".join(bad[:50]), file=sys.stderr)
        shutil.rmtree(tmp)
        return 1
    tmp.rename(outdir)
    print(f"packed {len(sources)} source files into {outdir}")
    return 0


# ---------------------------------------------------------------- tables
def load(outdir):
    ex = json.loads((outdir / "extract.json").read_text())
    st = json.loads((outdir / "state.json").read_text())
    pts = {}
    for c in CAMPAIGNS:
        summ = json.loads((outdir / c / "summary.json").read_text())
        tets = summ["tets"]
        exruns = {(r["case"], r["np"]): r for r in ex["campaigns"][c]["runs"]}
        for r in summ["runs"]:
            key = (c, r["case"], r["np"])
            pts[key] = {"summary_run": r, "extract": exruns[(r["case"], r["np"])], "tets": tets,
                        "medians": summ["medians"][f"{r['case']}/np{r['np']}"], "campaign": c}
    hours = {p["campaign"]: p for p in st["points"] if p.get("campaign")}
    return pts, hours, st


def g(x, d=3):
    if x is None:
        return "—"
    a = abs(x)
    if a >= 10000:
        return f"{x:,.0f}"
    if a >= 100:
        return f"{x:,.0f}" if d <= 3 else f"{x:,.1f}"
    return f"{x:.{d}g}"


def pct(x):
    return "—" if x is None else f"{100 * x:.0f} %"


def phase(ch, name, per_step=False):
    v = ch.get(name)
    if v is None:
        return None
    return v / ch.get(name + " nb_rep", 1) if per_step else v


def tables(outdir):
    pts, hours, st = load(outdir)
    gib = 1024 ** 2  # KiB per GiB
    order = [("rust-np64-a1", "rust", 64), ("cpp-np64-a1", "cpp", 64), ("rust-np32-a1", "rust", 32),
             ("cpp-np32-a1", "cpp", 32), ("rust-np8-a1", "rust", 8)]
    out = []
    w = out.append
    w("**Phases** (rank 0's Chrono totals, s; per step = total / 5,000; `chrono.csv` of each run, in "
      "`extract.json` and `summary.json`; wall = the driver's measured run time; hours = the session's "
      "record, `state.json`):")
    w("")
    w("| code | ranks | wall s | session h (estimate) | read_mesh | dist_mesh | init_full | static solve | "
      "before_step | `solve_step` s (ms/step) | check s (ms/step) | energies s (ms/step) | loop s | core-µs/el/step |")
    w("|---|---:|---:|---|---:|---:|---:|---:|---:|---|---|---|---:|---:|")
    data = {}
    for c, case, np_ in order:
        p = pts[(c, case, np_)]
        ch = p["extract"]["chrono"]
        s = p["summary_run"]["summary"]
        tets = p["tets"]
        solve, check, en = (phase(ch, n) for n in ("solve_step", "check_cohesive_stress", "energies"))
        loop = s["loop_time_s"]
        core = (solve + check) / 5000 * np_ / tets * 1e6
        hr = hours[c]
        data[(case, np_)] = {"wall": p["summary_run"]["wall_s"], "init": ch["init_full"], "static": ch["static_solve"],
                             "solve": solve / 5000, "check": check / 5000, "energies": en / 5000, "loop": loop,
                             "core": core, "dist": ch["dist_mesh"], "read": ch["read_mesh"],
                             "before": ch["before_step"], "hours": hr["hours"], "est": hr["estimate_h"],
                             "setup": s["init_time_s"], "p": p}
        w(f"| {'Rust' if case == 'rust' else 'C++'} | {np_} | {g(p['summary_run']['wall_s'])} | "
          f"{hr['hours']:.3f} ({hr['estimate_h']:.2f}) | {g(ch['read_mesh'])} | {g(ch['dist_mesh'])} | "
          f"{g(ch['init_full'])} | {g(ch['static_solve'])} | {g(ch['before_step'])} | "
          f"{g(solve)} ({g(solve / 5 )}) | {g(check)} ({g(check / 5)}) | {g(en)} ({g(en / 5)}) | {g(loop)} | {core:.2f} |")
    w("")
    w("**Share of the wall time** (rank 0; set-up = read + distribution + `init_full` + static solve + "
      "`before_step`; the rest of the wall is MPI start-up and the end):")
    w("")
    w("| code | ranks | static solve | `dist_mesh` | other set-up | `solve_step` | check | energies | rest |")
    w("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for c, case, np_ in order:
        d = data[(case, np_)]
        wall = d["wall"]
        other = d["read"] + d["init"] + d["before"]
        rest = wall - (d["static"] + d["dist"] + other + (d["solve"] + d["check"] + d["energies"]) * 5000)
        w(f"| {'Rust' if case == 'rust' else 'C++'} | {np_} | {pct(d['static'] / wall)} | {pct(d['dist'] / wall)} | "
          f"{pct(other / wall)} | {pct(d['solve'] * 5000 / wall)} | {pct(d['check'] * 5000 / wall)} | "
          f"{pct(d['energies'] * 5000 / wall)} | {pct(rest / wall)} |")
    w("")
    w("**Port / upstream at the same rank count** (ratio of the single runs; the static solve of the "
      "20-step test runs in brackets, same binaries and partition):")
    w("")
    w("| ranks | wall | `init_full` | static solve | `dist_mesh` | `solve_step` | check | energies | loop | "
      "largest rank peak | total peak |")
    w("|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|")
    test = {case: pts[("test-np64", case, 64)] for case in ("rust", "cpp")}
    for np_ in (64, 32):
        r, cc = data[("rust", np_)], data[("cpp", np_)]
        rm, cm = r["p"]["medians"], cc["p"]["medians"]
        extra = ""
        if np_ == 64:
            tr, tc = (test[k]["extract"]["chrono"]["static_solve"] for k in ("rust", "cpp"))
            extra = f" ({tr / tc:.2f})"
        w(f"| {np_} | {r['wall'] / cc['wall']:.3f} | {r['init'] / cc['init']:.2f} | {r['static'] / cc['static']:.2f}{extra} | "
          f"{r['dist'] / cc['dist']:.2f} | {r['solve'] / cc['solve']:.2f} | {r['check'] / cc['check']:.2f} | "
          f"{r['energies'] / cc['energies']:.2f} | {r['loop'] / cc['loop']:.2f} | "
          f"{rm['run_rank_peak_max_kib'] / cm['run_rank_peak_max_kib']:.2f} | "
          f"{rm['run_total_rss_peak_kib'] / cm['run_total_rss_peak_kib']:.2f} |")
    w("")
    w("**Strong scaling** (speed-up against the smallest rank count of each code; efficiency = speed-up × "
      "P0 / P; single runs):")
    w("")
    w("| code | ranks | wall: S (eff.) | loop: S (eff.) | `solve_step`: S (eff.) | check: S (eff.) | "
      "static solve: S (eff.) | `init_full`: S | core-µs/el/step |")
    w("|---|---:|---|---|---|---|---|---:|---:|")
    for case, nps in (("rust", (8, 32, 64)), ("cpp", (32, 64))):
        b = data[(case, nps[0])]
        for np_ in nps:
            d = data[(case, np_)]

            def se(f):
                s = b[f] / d[f]
                return f"{s:.2f} ({100 * s * nps[0] / np_:.0f} %)"

            w(f"| {'Rust' if case == 'rust' else 'C++'} | {np_} | {se('wall')} | {se('loop')} | {se('solve')} | "
              f"{se('check')} | {se('static')} | {b['init'] / d['init']:.2f} | {d['core']:.2f} |")
    w("")
    w("**Memory** (GiB; sampler = `memsample.py`, every 0.25 s, VmRSS / VmHWM of every rank; container = "
      "the cgroup's `memory.peak`; static = the sampler's peaks up to the end of the static solve; Rust "
      "records = the driver's own VmRSS before the static solve and VmHWM at its end and at the end of the "
      "run; per-rank peaks: the sampler's VmHWM per process, min / median / max):")
    w("")
    w("| code | ranks | largest rank | total (sampler) | container | static: largest rank / total | "
      "per-rank peak min / median / max | Rust model before static: largest / sum | Rust HWM static: largest / sum | "
      "Rust HWM end: largest / sum |")
    w("|---|---:|---:|---:|---:|---|---|---|---|---|")
    for c, case, np_ in order:
        p = data[(case, np_)]["p"]
        m = p["medians"]
        ex = p["extract"]
        cont = ex["status"].get("memory_peak_bytes")
        rp = ex["sampler_rank_peaks_kib"]
        rp_s = f"{rp[0] / gib:.2f} / {statistics.median(rp) / gib:.2f} / {rp[-1] / gib:.2f}" if rp else "—"

        def pair(a, b):
            return "—" if m.get(a) is None else f"{m[a] / gib:.2f} / {m[b] / gib:.1f}"

        w(f"| {'Rust' if case == 'rust' else 'C++'} | {np_} | {m['run_rank_peak_max_kib'] / gib:.2f} | "
          f"{m['run_total_rss_peak_kib'] / gib:.1f} | {cont / 1024 ** 3:.1f} | "
          f"{m['static_rank_peak_max_kib'] / gib:.2f} / {m['static_total_rss_peak_kib'] / gib:.1f} | {rp_s} | "
          f"{pair('rust_rss_before_static_max_kib', 'rust_rss_before_static_sum_kib')} | "
          f"{pair('rust_hwm_static_max_kib', 'rust_hwm_static_sum_kib')} | "
          f"{pair('rust_hwm_end_max_kib', 'rust_hwm_end_sum_kib')} |")
    w("")
    w("**MUMPS** (the port's runs record MUMPS's own statistics; upstream hands MUMPS the same matrix; "
      "`INFOG` in MB, flops = `RINFOG(3)`):")
    w("")
    w("| ranks | ordering `INFOG(7)` | factor entries `INFOG(29)` | allocated sum / largest rank `INFOG(19)` / `(18)` | "
      "used sum / largest `INFOG(22)` / `(21)` | elimination flops | rate (flops / static-solve wall) | retries | "
      "matrix entries sum / largest rank |")
    w("|---:|---:|---:|---|---|---:|---:|---:|---|")
    for c, np_ in (("test-np64", 64), ("rust-np64-a1", 64), ("rust-np32-a1", 32), ("rust-np8-a1", 8)):
        p = pts[(c, "rust", np_)]
        rs = p["summary_run"]["rust_static"]
        s, mx = rs["stats"], rs["matrix"]
        sec = p["extract"]["chrono"]["static_solve"]
        lbl = f"{np_} (20-step test)" if c == "test-np64" else f"{np_}"
        w(f"| {lbl} | {s['infog7_ordering']} | {s['infog29_factor_entries_effective']:.3g} | "
          f"{s['infog19_mb_sum_allocated']:,} / {s['infog18_mb_max_allocated']:,} | "
          f"{s['infog22_mb_sum_used']:,} / {s['infog21_mb_max_used']:,} | {s['rinfog3_flops_elimination']:.3g} | "
          f"{s['rinfog3_flops_elimination'] / sec / 1e9:.0f} GF/s | {s['retries']} | "
          f"{mx['nnz_sum']:,} / {mx['nnz_max']:,} |")
    w("")
    w("**Pre-insertion costs at 64 ranks** (the 20-step test runs, same binaries and partition as the "
      "timed runs, no insertion within 20 steps) against the 5,000-step averages:")
    w("")
    w("| code | static solve s: test / timed | `solve_step` ms/step: test / timed | check ms/step: test / timed | "
      "core-µs/el/step: test / timed |")
    w("|---|---|---|---|---|")
    for case in ("rust", "cpp"):
        t = test[case]["extract"]["chrono"]
        d = data[(case, 64)]
        ts, tc = t["solve_step"] / t["solve_step nb_rep"], t["check_cohesive_stress"] / t["check_cohesive_stress nb_rep"]
        tcore = (ts + tc) * 64 / test[case]["tets"] * 1e6
        w(f"| {'Rust' if case == 'rust' else 'C++'} | {t['static_solve']:.0f} / {d['static']:.0f} | "
          f"{ts * 1e3:.1f} / {d['solve'] * 1e3:.1f} | {tc * 1e3:.0f} / {d['check'] * 1e3:.0f} | "
          f"{tcore:.2f} / {d['core']:.2f} |")
    w("")
    w("**Measured against the session's estimates** (plan = the run matrix of \"Full-size (C1) "
      "extrapolation\": set-up and loop hours before margins, loop core-µs/el/step with insertion; "
      "session estimate = plan + 30 %, + 50 % of the loop at 32 / 64 ranks):")
    w("")
    w("| code | ranks | session h / estimate (ratio) | set-up h: measured / plan | loop h: measured / plan (ratio) | "
      "core-µs/el/step: measured / plan (ratio) |")
    w("|---|---:|---|---|---|---|")
    for c, case, np_ in order:
        d = data[(case, np_)]
        su, lp, core = PLAN[(case, np_)]
        w(f"| {'Rust' if case == 'rust' else 'C++'} | {np_} | {d['hours']:.3f} / {d['est']:.2f} "
          f"({d['hours'] / d['est']:.2f}) | {d['setup'] / 3600:.3f} / {su:.3f} | {d['loop'] / 3600:.3f} / {lp:.3f} "
          f"({d['loop'] / 3600 / lp:.2f}) | {d['core']:.2f} / {core:.2f} ({d['core'] / core:.2f}) |")
    w("")
    w("**Counts and energies at step 5,000, as measured** (`summary.json`; the invariants hold in every run: "
      "owned nodes summed = global nodes, regular elements summed = tetrahedra):")
    w("")
    w("| code | ranks | cohesive elements | insertion steps | first insertion | global nodes at the end | "
      "E_d J | E_p J | E_k J | cohesive elements per rank: min / median / max |")
    w("|---|---:|---:|---:|---:|---:|---:|---:|---:|---|")
    for c, case, np_ in order:
        p = data[(case, np_)]["p"]
        s = p["summary_run"]["summary"]
        cr = sorted(r["nb_cohesive_not_ghost"] for r in p["extract"]["ranks"])
        w(f"| {'Rust' if case == 'rust' else 'C++'} | {np_} | {s['nb_cohesive_elements']:,} | {s['nb_insertion_steps']:,} | "
          f"{s['first_insertion_step']} | {s['nb_global_nodes_end']:,} | {s['energy_dissipated']:.4f} | "
          f"{s['energy_potential']:,.1f} | {s['energy_kinetic']:,.1f} | "
          f"{cr[0]:,} / {statistics.median(cr):,.0f} / {cr[-1]:,} |")
    w("")
    w("**Elements per rank** (regular tetrahedra not ghost, min / median / max over the ranks; the "
      "partition is the C++ harness's Scotch export, replayed by the port):")
    w("")
    w("| ranks | tetrahedra per rank: min / median / max | ghost tetrahedra per rank: min / median / max |")
    w("|---:|---|---|")
    for c, case, np_ in (("rust-np64-a1", "rust", 64), ("rust-np32-a1", "rust", 32), ("rust-np8-a1", "rust", 8)):
        rk = pts[(c, case, np_)]["extract"]["ranks"]
        el = sorted(r["nb_regular_elements_not_ghost"] for r in rk)
        gh = sorted(r["nb_elements_dim3_ghost"] for r in rk)
        w(f"| {np_} | {el[0]:,} / {statistics.median(el):,.0f} / {el[-1]:,} | "
          f"{gh[0]:,} / {statistics.median(gh):,.0f} / {gh[-1]:,} |")
    print("\n".join(out))
    return 0


def main(argv):
    if len(argv) == 4 and argv[1] == "pack":
        return pack(Path(argv[2]).expanduser(), Path(argv[3]))
    if len(argv) == 3 and argv[1] == "tables":
        return tables(Path(argv[2]))
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
