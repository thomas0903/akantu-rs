#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Project tooling for akantu-rs (no Akantu C++ source is ported here).
"""Print the counts that `docs/VALIDATION_SUMMARY.md` states.

Read-only, standard library only. Everything is recomputed from the files in
the tree, so a reader can check the summary's numbers and see how they were
taken:

* `fixtures`: oracle fixture cases, i.e. directories with a committed
  `manifest.json` (the harness's unit, `docs/NUMERICAL_VALIDATION.md`),
  per area and per area/group; plus the fixture sets without manifests
  (example runs, Python examples, dumper files), counted as directories;
* `defects`: the entries of `docs/UPSTREAM_DEFECTS.md` by the category the
  *Cat.* column starts with (A, B, C; "A+" etc. when the cell also names a
  second category, e.g. `A/B`, `C (B for …)`), by status (first word of the
  *In the port* column);
* `ledger`: the marker line of `docs/TOLERANCE_LEDGER.md`
  (`scripts/gold/tolerance_ledger.py`).

The parity-matrix counts come from `python3 scripts/parity/stats.py`.

Usage: scripts/docs/validation_counts.py [fixtures|defects|ledger ...]
"""

from __future__ import annotations

import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures"


def fixtures() -> None:
    cases = sorted(p.parent.relative_to(FIXTURES) for p in FIXTURES.rglob("manifest.json"))
    by_area = Counter(c.parts[0] for c in cases)
    by_group = Counter("/".join(c.parts[:2]) for c in cases if len(c.parts) > 1)
    print(f"fixture cases (directories with manifest.json): {len(cases)}")
    for area, n in sorted(by_area.items()):
        print(f"  {area}: {n}")
        for group, m in sorted(by_group.items()):
            if group.split("/")[0] == area and group != area:
                print(f"    {group}: {m}")
    for rel in ("examples", "python/examples", "dump"):
        d = FIXTURES / rel
        if d.is_dir():
            n = sum(1 for p in d.iterdir() if p.is_dir())
            print(f"fixture directories without manifests, {rel}/*: {n}")


def split_row(line: str) -> list[str]:
    """Cells of a Markdown table row, split as GitHub splits it: at every
    `|` that is not escaped (`\\|`), also inside a code span."""
    cells, cur, i = [], "", 0
    s = line.strip()
    while i < len(s):
        ch = s[i]
        if ch == "\\" and i + 1 < len(s):
            cur += s[i : i + 2]
            i += 2
            continue
        if ch == "|":
            cells.append(cur.strip())
            cur = ""
        else:
            cur += ch
        i += 1
    cells.append(cur.strip())
    return cells[1:-1]


REGISTER_CELLS = 7  # ID, upstream location, what happens, Cat., in the port, switch or patch, tests


CATEGORY = re.compile(r"^(A|B|C)(?![A-Za-z])")
OTHER_CATEGORY = re.compile(r"(?<![A-Za-z])(A|B|C)(?![A-Za-z])")

def register_status(cell: str) -> str:
    """Status of a register entry: the first word of its *In the port* cell
    (`Implemented.`, `Decided:` …), lower case; `Not ported` is two words."""
    words = re.sub(r"[*_`]", "", cell).strip().split(" ")
    word = re.sub(r"[^a-z]", "", words[0].lower())
    return "not ported" if word == "not" else word


def defects() -> int:
    cats, statuses = Counter(), Counter()
    rows = 0
    bad: list[str] = []
    for line in (ROOT / "docs" / "UPSTREAM_DEFECTS.md").read_text(encoding="utf-8").splitlines():
        if not line.startswith("| UD-"):
            continue
        rows += 1
        cells = split_row(line)
        if len(cells) != REGISTER_CELLS or not line.rstrip().endswith("|"):
            # A row that GitHub would render with another cell count (an
            # unescaped `|`, a missing closing `|`): its category and status
            # cannot be read reliably, so it is an error, not a guess.
            bad.append(f"{cells[0] if cells else line[:12]}: {len(cells)} cells")
            continue
        cat_cell, status = cells[3], cells[4]
        m = CATEGORY.match(cat_cell)
        if m:
            others = set(OTHER_CATEGORY.findall(cat_cell)) - {m.group(1)}
            key = m.group(1) + ("+" if others else "")
        elif cat_cell.lower().startswith("test"):
            key = "test infrastructure"
        elif cat_cell.startswith("—"):
            key = "none (no defect / no category)"
        else:
            key = "other: " + cat_cell.split("(")[0].strip()
        cats[key] += 1
        statuses[register_status(status) or "(empty)"] += 1
    print(f"upstream-defect entries: {rows}")
    print("  by category (Cat. column starts with; '+' = names a second category too):")
    for k, v in sorted(cats.items()):
        print(f"    {k}: {v}")
    print("  by status (first word of the In the port column):")
    for k, v in statuses.most_common():
        print(f"    {k}: {v}")
    if bad:
        print(f"ERROR: {len(bad)} register row(s) do not have {REGISTER_CELLS} cells:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    return 0


def ledger() -> None:
    text = (ROOT / "docs" / "TOLERANCE_LEDGER.md").read_text(encoding="utf-8")
    m = re.search(r"<!-- ledger: (.*?) -->", text)
    print("tolerance ledger:", m.group(1) if m else "no marker line")
    m = re.search(r"By criterion: (.*?)\.\n", text)
    if m:
        print("  by criterion:", m.group(1))


def main() -> int:
    parts = sys.argv[1:] or ["fixtures", "defects", "ledger"]
    status = 0
    for p in parts:
        status |= {"fixtures": fixtures, "defects": defects, "ledger": ledger}[p]() or 0
    return status


if __name__ == "__main__":
    sys.exit(main())
