#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): scope the
# inputs of ci/tools/analyze_tests.py to ONE `cargo test` invocation that
# covers only part of the workspace (the `release` tier's runs), so that the
# same analyser and the same waiver list (ci/tools/known-failures.txt)
# judge them:
#   * metadata.json keeps only the packages of the selection; the lib doc-test
#     targets of a package selected with --test only are marked doctest =
#     false (cargo does not run doc tests then), otherwise the analyser would
#     demand a Doc-tests section for every workspace crate;
#   * known-failures.txt keeps only the baseline entries that belong to the
#     selection (the analyser fails a baseline entry whose target "did not
#     run", which would be wrong for a target outside this invocation).
#
#   release_scope.py --metadata FULL.json --known-failures KF --out DIR SPEC...
#
# SPEC is PKG (every target of the package) or PKG:test=NAME[,NAME...] (only
# those integration tests, no doc tests). Writes DIR/metadata.json and
# DIR/known-failures.txt. Exit 0, or 2 on a usage error / unknown package.
"""Scope analyze_tests.py inputs to one partial cargo test invocation."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def parse_spec(spec: str) -> tuple[str, set[str] | None]:
    pkg, _, rest = spec.partition(":")
    if not pkg:
        raise ValueError(f"empty package in spec {spec!r}")
    if not rest:
        return pkg, None
    if not rest.startswith("test=") or not rest[5:]:
        raise ValueError(f"bad spec {spec!r} (want PKG or PKG:test=A,B)")
    return pkg, set(rest[5:].split(","))


def scope_metadata(meta: dict, specs: dict[str, set[str] | None]) -> dict:
    out = dict(meta)
    pkgs = []
    for p in meta.get("packages", []):
        if p["name"] not in specs:
            continue
        q = dict(p)
        tests = specs[p["name"]]
        targets = []
        for t in p.get("targets", []):
            t = dict(t)
            if tests is not None:
                kinds = t.get("kind", [])
                if any(k in ("lib", "rlib", "dylib", "proc-macro") for k in kinds):
                    t["doctest"] = False
            targets.append(t)
        q["targets"] = targets
        pkgs.append(q)
    out["packages"] = pkgs
    ids = {p["id"] for p in pkgs}
    out["workspace_members"] = [i for i in meta.get("workspace_members", []) if i in ids]
    return out


def scope_baseline(text: str, specs: dict[str, set[str] | None]) -> str:
    keep = []
    for line in text.splitlines():
        body = line.split("#", 1)[0].strip()
        if not body:
            continue  # comments and blanks are not needed by the analyser
        parts = [x.strip() for x in body.split("|")]
        if len(parts) != 4:
            keep.append(line)  # malformed: let the analyser report it
            continue
        pkg, target = parts[0], parts[1]
        if pkg not in specs:
            continue
        tests = specs[pkg]
        if tests is not None and not (target.startswith("--test ") and target[7:] in tests):
            continue
        keep.append(line)
    return "\n".join(["# scoped by ci/tools/release_scope.py (the release tier)"] + keep) + "\n"


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--metadata", required=True)
    ap.add_argument("--known-failures", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("specs", nargs="+")
    a = ap.parse_args(argv[1:])
    try:
        specs = dict(parse_spec(s) for s in a.specs)
        meta = json.loads(Path(a.metadata).read_text())
        kf = Path(a.known_failures).read_text()
    except (OSError, ValueError) as e:
        print(f"release_scope: {e}", file=sys.stderr)
        return 2
    have = {p["name"] for p in meta.get("packages", [])}
    missing = sorted(set(specs) - have)
    if missing:
        print(f"release_scope: package(s) not in the workspace metadata: {', '.join(missing)}", file=sys.stderr)
        return 2
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "metadata.json").write_text(json.dumps(scope_metadata(meta, specs)))
    (out / "known-failures.txt").write_text(scope_baseline(kf, specs))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
