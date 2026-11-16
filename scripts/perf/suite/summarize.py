#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): summary tables of the
# benchmark suite's JSON lines (crates/akantu-bench/README.md "Benchmark
# suite").
"""Summarise benchmark-suite output.

    summarize.py [--ref NAME] [--phases a,b] [--cases a,b] [--unit auto|s|ms]
                 [--out FILE.md] INPUT...

INPUT is a run_ab.sh directory (DIR/NAME/CASE.r<r>.jsonl: variant NAME,
repetition r) or a JSON-lines file (variant = the file's stem, repetition =
the line's "rep"). Per case, phase and variant: the median seconds with
[min, max] over the repetitions, the per-call time, the time per element and
call (ns), and against the reference variant (--ref, default the first) the
paired ratio per repetition, median [min, max]. A difference whose ratio
range contains 1 is reported as "no measurable change" by the reader, not by
this script. The digest column says whether every run of the case (all
variants, all repetitions) produced the same digest.
"""

import argparse
import json
import re
import statistics
import sys
from collections import OrderedDict, defaultdict
from pathlib import Path


def load(inputs):
    """Rows (variant, rep, record) in input order."""
    rows = []
    for inp in inputs:
        p = Path(inp)
        if p.is_dir():
            for vdir in sorted(x for x in p.iterdir() if x.is_dir()):
                for f in sorted(vdir.glob("*.jsonl")):
                    m = re.search(r"\.r(\d+)\.jsonl$", f.name)
                    for line in f.read_text().splitlines():
                        if not line.startswith("{"):
                            continue
                        d = json.loads(line)
                        rep = int(m.group(1)) if m else d.get("rep", 0)
                        rows.append((vdir.name, rep, d))
        else:
            for line in p.read_text().splitlines():
                if not line.startswith("{"):
                    continue
                d = json.loads(line)
                rows.append((p.stem, d.get("rep", 0), d))
    return rows


def fmt_time(s, unit):
    if unit == "s" or (unit == "auto" and s >= 1.0):
        return f"{s:.3f} s"
    if unit == "ms" or (unit == "auto" and s >= 1e-3):
        return f"{s * 1e3:.2f} ms"
    return f"{s * 1e6:.1f} µs"


def fmt_range(vals, f):
    return f"{f(statistics.median(vals))} [{f(min(vals))}, {f(max(vals))}]"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("inputs", nargs="+")
    ap.add_argument("--ref", default=None)
    ap.add_argument("--phases", default=None)
    ap.add_argument("--cases", default=None)
    ap.add_argument("--unit", default="auto", choices=["auto", "s", "ms"])
    ap.add_argument("--out", default=None)
    a = ap.parse_args()

    rows = load(a.inputs)
    if not rows:
        sys.exit("summarize.py: no data")
    phases_f = set(a.phases.split(",")) if a.phases else None
    cases_f = set(a.cases.split(",")) if a.cases else None

    variants = list(OrderedDict.fromkeys(v for v, _, _ in rows))
    ref = a.ref or variants[0]
    if ref not in variants:
        sys.exit(f"summarize.py: reference variant {ref} not in {variants}")

    # (case, phase) -> variant -> rep -> record; keep case/phase order
    data = OrderedDict()
    digests = defaultdict(set)
    meta = {}
    for v, rep, d in rows:
        case, phase = d["case"], d["phase"]
        if cases_f and case not in cases_f:
            continue
        digests[case].add(d.get("digest"))
        meta.setdefault(case, d)
        if phases_f and phase not in phases_f:
            continue
        data.setdefault((case, phase), defaultdict(dict))[v][rep] = d

    out = []
    first = rows[0][2]
    out.append(
        f"Suite `{first.get('suite')}`, size `{first.get('size')}`; variants: "
        + ", ".join(f"`{v}`" for v in variants)
        + f"; reference `{ref}`. Times: median [min, max] over the repetitions; "
        "ratios: median [min, max] of the per-repetition ratios to the reference."
    )
    out.append("")
    head = ["case", "phase", "variant", "reps", "calls", "seconds", "per call", "ns/(element·call)"]
    if len(variants) > 1:
        head.append(f"ratio to {ref}")
    head.append("digest")
    out.append("| " + " | ".join(head) + " |")
    out.append("|" + "---|" * len(head))
    for (case, phase), per_v in data.items():
        same = "same" if len(digests[case]) == 1 else f"DIFFER ({len(digests[case])})"
        for v in variants:
            if v not in per_v:
                continue
            reps = per_v[v]
            secs = [r["seconds"] for r in reps.values()]
            calls = sorted({r["calls"] for r in reps.values()})
            ne = next(iter(reps.values())).get("n_elements") or 0
            per_call = [r["seconds"] / max(r["calls"], 1) for r in reps.values()]
            cells = [
                case,
                phase,
                v,
                str(len(secs)),
                "/".join(map(str, calls)),
                fmt_range(secs, lambda s: fmt_time(s, a.unit)),
                fmt_range(per_call, lambda s: fmt_time(s, a.unit)),
            ]
            if ne:
                per_el = [r["seconds"] / max(r["calls"], 1) / ne * 1e9 for r in reps.values()]
                cells.append(fmt_range(per_el, lambda x: f"{x:.1f}"))
            else:
                cells.append("–")
            if len(variants) > 1:
                if v == ref or ref not in per_v:
                    cells.append("–")
                else:
                    rr = [
                        reps[k]["seconds"] / per_v[ref][k]["seconds"]
                        for k in reps
                        if k in per_v[ref] and per_v[ref][k]["seconds"] > 0
                    ]
                    cells.append(fmt_range(rr, lambda x: f"{x:.3f}") if rr else "–")
            cells.append(same)
            out.append("| " + " | ".join(cells) + " |")
    out.append("")
    out.append("Problem sizes and digests:")
    out.append("")
    out.append("| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |")
    out.append("|---|---|---|---|---|---|---|")
    commits = defaultdict(set)
    for v, _, d in rows:
        commits[d["case"]].add(d.get("commit"))
    for case, d in meta.items():
        out.append(
            f"| {case} | {d.get('n_elements')} | {d.get('n_nodes')} | {d.get('n_dofs')} | "
            f"{d.get('ranks')} | {', '.join(sorted(c or '?' for c in commits[case]))} | "
            f"{', '.join(sorted(x or 'none' for x in digests[case]))} |"
        )
    text = "\n".join(out) + "\n"
    if a.out:
        Path(a.out).write_text(text)
    else:
        sys.stdout.write(text)
    if any(len(s) > 1 for s in digests.values()):
        print("summarize.py: WARNING: digests differ for some cases", file=sys.stderr)


if __name__ == "__main__":
    main()
