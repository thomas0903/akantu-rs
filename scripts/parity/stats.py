#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
"""Count parity-matrix rows by area and status; optionally refresh the
summary block in docs/FEATURE_PARITY.md and the status numbers of
README.md (``--update``), or print the README block (``--readme``; checked
by ``crates/akantu/tests/manual.rs::readme_status_matches_parity_stats``).

Percentages are defined as: rows with status `Verified` (or
`Exception (approved)`) divided by all non-`Superseded` rows of the area
(`Rows` also excludes superseded parents, whose scope moved to child rows). Rows are the
features of the source inventory, so the figure measures verified
functional coverage of the inventory, nothing more.
"""

from __future__ import annotations

import re
import sys
import textwrap
from collections import Counter, OrderedDict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MATRIX = ROOT / "docs" / "FEATURE_PARITY.md"
README = ROOT / "README.md"
README_BEGIN = "<!-- BEGIN GENERATED: parity-stats (scripts/parity/stats.py --update) -->\n"
README_END = "<!-- END GENERATED: parity-stats -->"
STATUSES = [
    "Not started",
    "In progress",
    "Implemented, unverified",
    "Verified",
    "Blocked",
    "Exception (approved)",
    "Superseded",
]


def split_row(line: str) -> list[str]:
    """Cells of a GFM table row: every pipe not preceded by a backslash
    separates cells, inside code spans too (write `\\|` for a literal pipe;
    GFM renders it as `|` in a code span)."""
    parts = re.split(r"(?<!\\)\|", line.strip())
    return [c.strip() for c in parts[1:-1]]


class MatrixError(Exception):
    """A row the counter cannot classify."""


def count(text: str) -> "OrderedDict[str, Counter]":
    """Rows per area and status. A row whose cell count differs from its
    table's header, or whose Status cell is not one of STATUSES, is an
    error: an unescaped pipe shifts the cells, and the row would otherwise
    be counted under a wrong status (CORE-019 and CORE-033 were
    read as "n/a" and "architecture reviews …")."""
    area = None
    counts: "OrderedDict[str, Counter]" = OrderedDict()
    status_idx = None
    width = None
    errors = []
    seen: dict[str, int] = {}
    for n, line in enumerate(text.splitlines(), 1):
        m = re.match(r"^## (\S+)(?: —|:) ", line)
        if m:
            area = m.group(1)
            counts.setdefault(area, Counter())
            continue
        if line.startswith("| ID |"):
            header = split_row(line)
            status_idx = header.index("Status")
            width = len(header)
            continue
        if area and line.startswith("|") and status_idx is not None:
            if re.fullmatch(r"\|[-: |]+\|", line.strip()):
                continue  # the separator line
            cells = split_row(line)
            if not line.startswith("| ") or not cells or not re.fullmatch(r"[A-Z]+-\d+", cells[0]):
                # e.g. "|CORE-001 |": not counted by a "| " test, silently
                errors.append(f"line {n}: malformed table row {line[:40]!r}")
                continue
            if cells[0] in seen:
                errors.append(f"line {n} ({cells[0]}): duplicate row id (first on line {seen[cells[0]]})")
                continue
            seen[cells[0]] = n
            if len(cells) != width:
                errors.append(
                    f"line {n} ({cells[0]}): {len(cells)} cells, the header has {width} "
                    "(escape a literal pipe as \\|, also inside code spans)"
                )
                continue
            st = cells[status_idx]
            if st not in STATUSES:
                errors.append(f"line {n} ({cells[0]}): unknown status {st!r}")
                continue
            counts[area][st] += 1
    if errors:
        raise MatrixError("\n".join(errors))
    return counts


def self_test() -> None:
    """Checks of the row parser on synthetic tables; run before every count
    (so `readme_status_matches_parity_stats` exercises it) and alone with
    `--self-test`."""
    head = "## X: test area\n\n| ID | Feature | Status | Notes |\n|---|---|---|---|\n"
    ok = head + (
        "| X-001 | escaped pipes `max \\|Im v\\| > 0` | Verified | `<pid>[R<rank>\\|S<size>]` |\n"
        "| X-002 | a backslash code span `\\` and `\\|a\\|` | Implemented, unverified | — |\n"
        "| X-003 | plain | Superseded | — |\n"
    )
    c = count(ok)
    assert c["X"] == Counter({"Verified": 1, "Implemented, unverified": 1, "Superseded": 1}), c
    assert split_row("| a \\| b | c |") == ["a \\| b", "c"]
    for bad, why in [
        (head + "| X-004 | unescaped `max |Im v|` | Verified | — |\n", "cells"),
        (head + "| X-005 | f | Done | — |\n", "unknown status"),
        # A duplicate id was counted twice, an unspaced
        # row not at all
        (head + "| X-006 | a | Verified | — |\n| X-006 | b | Verified | — |\n", "duplicate row id"),
        (head + "|X-007 | a | Verified | — |\n", "malformed table row"),
        (head + "| not a row | a | Verified | — |\n", "malformed table row"),
    ]:
        try:
            count(bad)
        except MatrixError as e:
            assert why in str(e), (why, str(e))
        else:
            raise AssertionError(f"stats.py accepted a malformed row ({why})")


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print("stats.py self-test: ok")
        return 0
    text = MATRIX.read_text()
    try:
        counts = count(text)
    except MatrixError as e:
        print(f"{MATRIX.relative_to(ROOT)}: malformed rows:\n{e}", file=sys.stderr)
        return 1

    total = Counter()
    for c in counts.values():
        total.update(c)
    if "--readme" in sys.argv:
        sys.stdout.write(readme_block(total))
        return 0
    lines = ["| Area | Rows | " + " | ".join(STATUSES) + " | Verified % |", "|" + "---|" * (len(STATUSES) + 3)]
    for a, c in list(counts.items()) + [("**Total**", total)]:
        n = sum(c.values()) - c["Superseded"]
        ver = c["Verified"] + c["Exception (approved)"]
        pct = f"{100.0 * ver / n:.1f}" if n else "–"
        lines.append(f"| {a} | {n} | " + " | ".join(str(c[s]) for s in STATUSES) + f" | {pct} |")
    table = "\n".join(lines)
    print(table)
    if "--update" in sys.argv:
        new = re.sub(
            r"<!-- STATS-BEGIN -->.*?<!-- STATS-END -->",
            "<!-- STATS-BEGIN -->\n" + table + "\n<!-- STATS-END -->",
            text,
            flags=re.S,
        )
        MATRIX.write_text(new)
        readme = README.read_text()
        b = readme.index(README_BEGIN) + len(README_BEGIN)
        e = readme.index(README_END, b)
        README.write_text(readme[:b] + readme_block(total) + readme[e:])
    return 0


def readme_block(total: Counter) -> str:
    """The README's status sentence (Markdown, wrapped at 75 columns)."""
    rows = sum(total.values())
    n = rows - total["Superseded"]
    ver = total["Verified"] + total["Exception (approved)"]
    pct = f"{100.0 * ver / n:.1f}" if n else "–"
    text = (
        f"The feature parity matrix [`docs/FEATURE_PARITY.md`](docs/FEATURE_PARITY.md) "
        f"has {rows:,} rows, {total['Superseded']:,} of them superseded by finer rows. "
        f"Of the other {n:,}: {total['Verified']:,} *Verified* and "
        f"{total['Exception (approved)']:,} approved exceptions ({pct} % together), "
        f"{total['Implemented, unverified']:,} *Implemented, unverified*, "
        f"{total['In progress']:,} *In progress*, {total['Blocked']:,} *Blocked* and "
        f"{total['Not started']:,} *Not started*."
    )
    return textwrap.fill(text, width=75, break_long_words=False, break_on_hyphens=False) + "\n"


if __name__ == "__main__":
    sys.exit(main())
