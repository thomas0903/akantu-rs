#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): the set
# of skipped pytest cases is part of the verdict. A case that skips in every
# workflow (an oracle comparison whose environment is not the oracle's) is
# invisible in a green run; a NEW skip is a test that stopped running.
"""Compare the skips of a `pytest -rs` log with the expected skip set.

    pytest_skips.py LOG --expected FILE --run NAME [--platform KEY]

FILE holds lines ``run | path | reason | count`` (``#`` comments, blanks
ignored). A line whose run is ``NAME@KEY`` applies only on the platform KEY
(``<machine>-<sys.platform>[-<libc>]``, the key of
python/tests/examples: ``x86_64-linux-glibc``, ``arm64-darwin``; default:
this interpreter's), so an environment skip that belongs to one platform
is never accepted on another. The skips of LOG come from
its ``SKIPPED [N] path:line: reason``
summary lines (pytest's ``-rs``; a skipped case without that line is not
visible to this tool). Exit 0 when every skip of LOG is expected with at
least its count (fewer skips than expected are noted, not failed: a case that
now runs is good news, the file is then tightened on purpose), 1 otherwise,
2 on a usage error or a log without a pytest summary line.
"""

from __future__ import annotations

import argparse
import platform as _platform
import re
import sys
from collections import Counter
from pathlib import Path

SKIP = re.compile(r"^SKIPPED \[(\d+)\] (\S+?):\d+: (.*)$")
SUMMARY = re.compile(r"=+ .*(passed|skipped|failed).* in [\d.]+s")
SUMMARY_Q = re.compile(r"^\d+ (passed|skipped|failed).* in [\d.]+s")


def norm(reason: str) -> str:
    """The version numbers of an environment skip ("here ['1.18.1'], in the
    oracle runs ['1.15.3']") move with every SciPy release: not part of the key."""
    return re.sub(r"\s+here \[.*$", "", reason.strip())


def parse_log(text: str) -> Counter:
    out: Counter = Counter()
    for line in text.splitlines():
        m = SKIP.match(line.strip())
        if m:
            out[(m.group(2), norm(m.group(3)))] += int(m.group(1))
    return out


def platform_key() -> str:
    """This interpreter's platform key, as python/examples/cases.py computes it."""
    libc = _platform.libc_ver()[0]
    return "-".join(x for x in (_platform.machine(), sys.platform, libc) if x)


def parse_expected(text: str, run: str, platform: str = "") -> Counter:
    out: Counter = Counter()
    for raw in text.splitlines():
        s = raw.split("#", 1)[0].strip() if not raw.lstrip().startswith("#") else ""
        if not s:
            continue
        p = [x.strip() for x in s.split("|")]
        if len(p) != 4 or not p[3].isdigit():
            raise ValueError(f"bad line (want: run | path | reason | count): {raw}")
        if p[0] == run or (platform and p[0] == f"{run}@{platform}"):
            out[(p[1], norm(p[2]))] += int(p[3])
    return out


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("log")
    ap.add_argument("--expected", required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--platform", default=None,
                    help="platform key of the lines NAME@KEY (default: this interpreter's)")
    a = ap.parse_args(argv)
    plat = a.platform if a.platform is not None else platform_key()
    try:
        text = Path(a.log).read_text(errors="replace")
        want = parse_expected(Path(a.expected).read_text(), a.run, plat)
    except (OSError, ValueError) as e:
        print(f"pytest_skips: {e}", file=sys.stderr)
        return 2
    if not any(SUMMARY.search(x) or SUMMARY_Q.match(x) for x in text.splitlines()):
        print("pytest_skips: no pytest summary line in the log", file=sys.stderr)
        return 2
    got = parse_log(text)
    bad = 0
    for k, n in sorted(got.items()):
        if want.get(k, 0) < n:
            bad += 1
            print(f"FAIL unexpected skip ({a.run}): {n} x {k[0]}: {k[1]} (expected {want.get(k, 0)})")
    for k, n in sorted(want.items()):
        if got.get(k, 0) < n:
            print(f"note: {a.run}: {n - got.get(k, 0)} of the {n} expected skips of {k[0]} did not skip any more: {k[1]}")
    total = sum(got.values())
    print(f"{'FAIL' if bad else 'ok'}   {a.run} ({plat}): {total} skips, {len(got)} distinct, {bad} unexpected")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
