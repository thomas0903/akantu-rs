#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): a floor
# on the number of tests a cargo/libtest log executed. A run that executed
# (almost) nothing is never a pass: an empty workspace, a test filter that
# matches nothing (`cargo test array_storage` with the module renamed) and a
# vanished test binary all exit 0 for cargo.
#
#   count_tests.py LOG [--floors FILE --key K] [--min-results N] [--min-passed N]
#
# Counts the libtest `test result:` lines of LOG. The floors are the larger
# of the command-line values, the entries `K.result_lines = N` /
# `K.passed = N` of the floors file, and the built-in minimum 1 (so an empty
# run fails even without a floors file). Prints one line with the counts and
# the floors; exit 0 when both floors are met, 1 when not, 2 on usage /
# unreadable log. Standard library only.
"""Floor on the number of executed tests in a libtest log."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

RESULT = re.compile(
    r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out"
)


def read_floors(path: Path | None, key: str | None) -> dict[str, int]:
    out: dict[str, int] = {}
    if path is None or key is None:
        return out
    for raw in path.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        if "=" not in line:
            continue
        name, value = (x.strip() for x in line.split("=", 1))
        if name.startswith(key + ".") and value.isdigit():
            out[name[len(key) + 1:]] = int(value)
    return out


def count(text: str) -> dict[str, int]:
    tot = {"result_lines": 0, "passed": 0, "failed": 0, "ignored": 0}
    for m in RESULT.finditer(text):
        tot["result_lines"] += 1
        tot["passed"] += int(m.group(1))
        tot["failed"] += int(m.group(2))
        tot["ignored"] += int(m.group(3))
    return tot


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("log")
    ap.add_argument("--floors", default=None)
    ap.add_argument("--key", default=None)
    ap.add_argument("--min-results", type=int, default=0)
    ap.add_argument("--min-passed", type=int, default=0)
    a = ap.parse_args(argv[1:])
    try:
        text = Path(a.log).read_text(errors="replace")
        floors = read_floors(Path(a.floors) if a.floors else None, a.key)
    except OSError as e:
        print(f"count_tests: cannot read {e}", file=sys.stderr)
        return 2
    c = count(text)
    min_results = max(1, a.min_results, floors.get("result_lines", 0))
    min_passed = max(1, a.min_passed, floors.get("passed", 0))
    ok = c["result_lines"] >= min_results and c["passed"] >= min_passed
    print(
        f"{a.key or 'tests'}: {c['result_lines']} result lines (floor {min_results}), "
        f"{c['passed']} passed (floor {min_passed}), {c['failed']} failed, {c['ignored']} ignored: "
        + ("floors met" if ok else "BELOW THE FLOOR (a vanished test binary, an empty workspace or a filter that matches nothing; "
           "if tests were removed on purpose, lower the floor in ci/floors.txt in the same reviewed change)")
    )
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
