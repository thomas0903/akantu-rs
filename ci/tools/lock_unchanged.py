#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): the
# downstream-consumer check of the `package` tier runs `cargo check` on a
# scratch crate with a COPY of the workspace's Cargo.lock; cargo may rewrite
# that copy. cargo prunes the packages the scratch crate does not need, so
# removals are fine; what must not happen is a CHANGE: every (name, version,
# source) of the new lockfile must exist in the original, except the scratch
# crate itself (a re-resolved dependency would mean the check ran against
# versions other than the locked ones).
#
#   lock_unchanged.py ORIGINAL.lock NEW.lock [ALLOWED_NEW_PACKAGE]
#
# Exit 0 = every package of the new lockfile is a locked one, 1 = a package
# with another version/source (or an unexpected new one) appeared, 2 =
# unreadable. Python >= 3.11.
"""Check that a copied Cargo.lock kept every locked package."""

from __future__ import annotations

import sys

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    tomllib = None


def packages(path: str) -> set[tuple[str, str, str]]:
    with open(path, "rb") as f:
        doc = tomllib.load(f)
    return {(p["name"], p["version"], p.get("source", "")) for p in doc.get("package", [])}


def compare(old: set, new: set, allowed: set[str]) -> list[str]:
    return [f"package not in the locked set: {n} {v} {s}".rstrip() for (n, v, s) in sorted(new - old) if n not in allowed]


def main(argv: list[str]) -> int:
    if len(argv) not in (3, 4) or tomllib is None:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        old, new = packages(argv[1]), packages(argv[2])
    except (OSError, ValueError, KeyError) as e:
        print(f"lock_unchanged: {e}", file=sys.stderr)
        return 2
    problems = compare(old, new, set(argv[3:]))
    for p in problems:
        print(f"lock_unchanged: {p}", file=sys.stderr)
    if not problems:
        print(f"lock_unchanged: {len(new)} of {len(old)} locked packages used, none re-resolved")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
