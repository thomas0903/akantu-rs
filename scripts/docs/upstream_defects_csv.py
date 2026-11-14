#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (Rust port of Akantu, EPFL/LSMS). Not derived from an
# upstream file: a bookkeeping script over docs/UPSTREAM_DEFECTS.md.
"""Write docs/reports/data/upstream-defects.csv (one row per register entry)
and the chart data of the upstream-defects report.

Usage: python3 scripts/docs/upstream_defects_csv.py [--check]

Every column is derived mechanically from docs/UPSTREAM_DEFECTS.md: the area
is the section the entry's table sits in, the other columns come from the
entry's cells (rules next to each function). --check prints the
totals and exits non-zero when they disagree with the expected totals of the
register (596 entries; A 148, B 242, C 176, test infrastructure 20, other 10),
which scripts/docs/validation_counts.py reads the same way.

docs/reports/data/upstream-defects-found-by.csv is not written here: it is a
keyword classification of the register's former description column, kept
as it was when the register was rewritten in its present layout (the
present cells no longer say how an entry was found).
"""
from __future__ import annotations

import csv
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTER = ROOT / "docs" / "UPSTREAM_DEFECTS.md"
OUT = ROOT / "docs" / "reports" / "data"

sys.path.insert(0, str(ROOT / "scripts" / "docs"))
from validation_counts import (  # noqa: E402
    CATEGORY, OTHER_CATEGORY, REGISTER_CELLS, register_status, split_row)

# Section heading of the register -> area label of the CSV and the chart.
HEADING_AREA = {
    "Solid mechanics model and materials": "solid mechanics",
    "Cohesive elements": "cohesive",
    "Contact mechanics": "contact",
    "Phase field": "phase field",
    "Structural mechanics": "structural",
    "Diffusion (heat transfer)": "diffusion",
    "Solvers, DOF managers and time integration": "solvers",
    "Parallel: communicator, synchronizers, partitioning": "parallel",
    "Mesh, mesh utilities and geometry": "mesh",
    "FE engine and core types": "FE engine and core",
    "Input and output": "I/O",
    "Python bindings": "Python",
    "Build system": "build",
    "Extra packages": "extra packages",
    "Tests, examples and documentation": "tests, examples, docs",
}
AREA_ORDER = list(HEADING_AREA.values())


def category(cell: str) -> tuple[str, str]:
    """(category, other categories named): as validation_counts.py."""
    m = CATEGORY.match(cell)
    if m:
        others = sorted(set(OTHER_CATEGORY.findall(cell)) - {m.group(1)})
        return m.group(1), "/".join(others)
    if cell.lower().startswith("test"):
        return "test infrastructure", ""
    if cell.startswith("—"):
        return "none", ""
    return cell.split("(")[0].strip(), ""


PATH = re.compile(
    r"((?:[\w.+-]+/)*[\w.+-]+\.(?:cc|hh|cpp|hpp|tcc|py|cmake|sh|txt|in|geo|dat|rst|i))"
    r"(?::(\d[\d,\s–-]*\d|\d))?"
)


def upstream_file(loc: str) -> str:
    """First upstream source path (with its line range) of the Upstream
    location cell; empty when the cell names no file."""
    m = PATH.search(loc)
    if not m:
        return ""
    return m.group(1) + (":" + m.group(2).replace(" ", "") if m.group(2) else "")


PATCH = re.compile(r"patch(?:es)? `([^`]+)`")


def switch_and_patch(cell: str) -> tuple[str, str]:
    """('yes' when the Switch or patch cell names a switch, the oracle
    patch(es) it names). The cell starts with `No switch` or is `—` when
    there is none."""
    patches = "; ".join(PATCH.findall(cell))
    text = PATCH.sub("", cell).strip(" ;.,")
    has = bool(text) and text != "—" and not re.match(r"(?i)no switch|none\b|—", text)
    return ("yes" if has else "no"), patches


def one_line(desc: str) -> str:
    d = re.sub(r"\*\*|`", "", desc)
    d = re.sub(r"\s+", " ", d).strip()
    m = re.search(r"(?<=[a-z0-9)\]])\.\s+(?=[A-Z(])", d)
    if m and m.start() > 40:
        d = d[: m.start() + 1]
    if len(d) > 240:
        d = d[:237].rsplit(" ", 1)[0] + " …"
    return d


DISTRIBUTED = re.compile(
    r"\b(ranks?|distributed|MPI|partition\w*|several processes|multi-rank|ghost)\b", re.I)


def distributed(desc: str) -> str:
    """'yes' when the What happens cell concerns distributed runs (it names
    ranks, ghosts, partitions, MPI or a distributed mesh), whatever the area."""
    return "yes" if DISTRIBUTED.search(desc) else "no"


def read_rows() -> list[dict[str, str]]:
    rows, area, bad = [], None, []
    for line in REGISTER.read_text(encoding="utf-8").splitlines():
        if line.startswith("## "):
            area = HEADING_AREA.get(line[3:].strip())
            continue
        if not line.startswith("| UD-"):
            continue
        c = split_row(line)
        if len(c) != REGISTER_CELLS or area is None:
            bad.append(f"{c[0]}: {len(c)} cells, area {area}")
            continue
        uid, loc, what, cat, port, sw, _tests = c
        main_cat, other = category(cat)
        has_switch, patch = switch_and_patch(sw)
        rows.append({
            "id": uid,
            "area": area,
            "category": main_cat,
            "also_names": other,
            "description": one_line(what),
            "upstream_file": upstream_file(loc),
            "status": register_status(port),
            "compat_switch": has_switch,
            "oracle_patch": patch,
            "distributed": distributed(what),
        })
    if bad:
        raise SystemExit("register rows not readable:\n  " + "\n  ".join(bad))
    return rows


def main() -> int:
    rows = read_rows()
    OUT.mkdir(parents=True, exist_ok=True)
    with (OUT / "upstream-defects.csv").open("w", newline="", encoding="utf-8") as f:
        f.write("# source: docs/UPSTREAM_DEFECTS.md, register tables (all rows); "
                "derived by scripts/docs/upstream_defects_csv.py\n")
        w = csv.DictWriter(f, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)

    cats = ["A", "B", "C", "test infrastructure", "other"]

    def cat_bin(r):
        return r["category"] if r["category"] in cats[:4] else "other"
    by = Counter((r["area"], cat_bin(r)) for r in rows)
    with (OUT / "upstream-defects-by-area.csv").open("w", newline="", encoding="utf-8") as f:
        f.write("# source: docs/UPSTREAM_DEFECTS.md, register tables; area = section of the "
                "register, category = first letter of the Cat. column\n")
        w = csv.writer(f)
        w.writerow(["area"] + cats + ["total"])
        for a in AREA_ORDER:
            vals = [by[(a, k)] for k in cats]
            w.writerow([a] + vals + [sum(vals)])
        tot = [sum(by[(a, k)] for a in AREA_ORDER) for k in cats]
        w.writerow(["total"] + tot + [sum(tot)])

    n = len(rows)
    cc = Counter(cat_bin(r) for r in rows)
    print(f"{n} entries; categories {dict(Counter(r['category'] for r in rows))}")
    print("areas", dict(Counter(r["area"] for r in rows)))
    print("status", dict(Counter(r["status"] for r in rows)))
    print("switch", dict(Counter(r["compat_switch"] for r in rows)))
    print("patch", sum(1 for r in rows if r["oracle_patch"]))
    print("distributed", dict(Counter(r["distributed"] for r in rows)))
    print("no upstream file", sum(1 for r in rows if not r["upstream_file"]))
    if "--check" in sys.argv:
        ok = (n == 596 and cc["A"] == 148 and cc["B"] == 242 and cc["C"] == 176
              and cc["test infrastructure"] == 20 and cc["other"] == 10)
        print("check", "OK" if ok else "MISMATCH")
        return 0 if ok else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
