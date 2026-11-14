#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: reproducible regeneration counts between two fixture trees
# (docs/NUMERICAL_VALIDATION.md).
"""Count what a fixture regeneration changed, reproducibly.

  python3 -I oracle/scripts/regeneration_counts.py <A> <B> [--json]
  python3 -I oracle/scripts/regeneration_counts.py --git <tree-ish A> <tree-ish B> [--json]
  python3 -I oracle/scripts/regeneration_counts.py --check-records [DIR]

A and B are fixture roots (directories) or, with --git, git tree-ishes of
the fixtures directory (`<commit>:fixtures` or a tree id), extracted with
`git archive`. Tree ids are content addresses: they survive a history
rewrite, commit ids do not.
`--check-records DIR` recomputes every record (*.json) of DIR from its two
tree ids and fails
unless the counts are identical; it needs a clone whose history holds both
trees. A record whose `scope` names a subtree
(e.g. `fixtures/material`) counts that subtree only: its `fixtures_tree`
ids are the subtree's tree ids (`<commit>:fixtures/material`) and
`root_tree` names the whole fixtures tree they belong to (whole-tree totals
are not area totals).

Counting policy (version 1):

* cases: directories holding a manifest.json, shared or present on one side;
* unmaterialised: shared cases with a manifest-listed payload missing on
  either side (external data of a fresh clone or of a git archive): reported,
  nothing of them is counted below;
* raw payloads: `quantities[].file` paths listed by both manifests of a
  shared materialised case: byte-identical or different;
* quantities (same name in both manifests of a shared materialised case):
  - comparable: equal structural fields (dtype, shape, axes, layout,
    components, ragged, ordering: the fields whose change makes
    `fixture-diff` report a structural problem instead of comparing),
    then bit-identical or different payload;
  - semantic changes: a structural field differs (listed with the fields
    and whether the payload bytes are identical);
  - metadata-only: payload bytes identical while the quantity entry
    differs in any field besides `sha256` (semantic label changes and
    descriptions);
  - only in A / only in B.
"""
from __future__ import annotations

import io
import json
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

POLICY = "regeneration_counts v1"
STRUCTURAL = ("dtype", "shape", "axes", "layout", "components", "ragged", "ordering")
REPO = Path(__file__).resolve().parents[2]


class CountError(Exception):
    """The trees cannot be counted (missing git object, malformed record)."""


def cases(root: Path) -> dict[str, dict]:
    out = {}
    for m in sorted(root.rglob("manifest.json")):
        rel = m.parent.relative_to(root).as_posix()
        out[rel] = json.loads(m.read_text(encoding="utf-8"))
    return out


def materialised(root: Path, case: str, m: dict) -> bool:
    d = root.joinpath(*case.split("/"))
    files = [q["file"] for q in m.get("quantities", [])]
    files += [i["path"] for i in m.get("inputs", []) if i.get("path")]
    return all((d / f).is_file() for f in files)


def count(a: Path, b: Path) -> dict:
    """Counts of the regeneration A -> B (see the module documentation)."""
    ca, cb = cases(a), cases(b)
    shared = sorted(set(ca) & set(cb))
    unmat = [c for c in shared if not (materialised(a, c, ca[c]) and materialised(b, c, cb[c]))]
    raw = {"shared": 0, "identical": 0, "different": []}
    qs = {"shared": 0, "comparable": 0, "bit_identical": 0, "different": [],
          "semantic_changes": [], "metadata_only": 0, "only_a": [], "only_b": []}
    for c in shared:
        if c in unmat:
            continue
        da, db = a.joinpath(*c.split("/")), b.joinpath(*c.split("/"))
        fa = {q["file"] for q in ca[c].get("quantities", [])}
        fb = {q["file"] for q in cb[c].get("quantities", [])}
        for f in sorted(fa & fb):
            raw["shared"] += 1
            if (da / f).read_bytes() == (db / f).read_bytes():
                raw["identical"] += 1
            else:
                raw["different"].append(f"{c}::{f}")
        qa = {q["name"]: q for q in ca[c].get("quantities", [])}
        qb = {q["name"]: q for q in cb[c].get("quantities", [])}
        qs["only_a"] += [f"{c}::{n}" for n in sorted(set(qa) - set(qb))]
        qs["only_b"] += [f"{c}::{n}" for n in sorted(set(qb) - set(qa))]
        for n in sorted(set(qa) & set(qb)):
            x, y = qa[n], qb[n]
            qs["shared"] += 1
            same_bytes = (da / x["file"]).read_bytes() == (db / y["file"]).read_bytes()
            fields = [k for k in STRUCTURAL if x.get(k) != y.get(k)]
            if fields:
                qs["semantic_changes"].append({"quantity": f"{c}::{n}", "fields": fields,
                                               "payload_identical": same_bytes})
            else:
                qs["comparable"] += 1
                if same_bytes:
                    qs["bit_identical"] += 1
                else:
                    qs["different"].append(f"{c}::{n}")
            rest = set(x) | set(y)
            rest.discard("sha256")
            if same_bytes and any(x.get(k) != y.get(k) for k in rest):
                qs["metadata_only"] += 1
    return {"policy": POLICY,
            "cases": {"a": len(ca), "b": len(cb), "shared": len(shared),
                      "only_a": sorted(set(ca) - set(cb)), "only_b": sorted(set(cb) - set(ca))},
            "unmaterialised": unmat, "raw_payloads": raw, "quantities": qs}


def git_extract(treeish: str, dest: Path, repo: Path = REPO) -> None:
    """Extract a git tree-ish (the fixtures directory) into dest."""
    p = subprocess.run(["git", "-C", str(repo), "archive", "--format=tar", treeish],
                       capture_output=True, check=False)
    if p.returncode != 0:
        raise CountError(f"git archive {treeish}: {p.stderr.decode(errors='replace').strip()}")
    with tarfile.open(fileobj=io.BytesIO(p.stdout)) as t:
        for member in t.getmembers():
            name = Path(member.name)
            if name.is_absolute() or ".." in name.parts or not (member.isfile() or member.isdir()):
                raise CountError(f"unexpected archive member {member.name!r}")
        t.extractall(dest)  # noqa: S202 - members checked above (own repository objects)


def count_git(a: str, b: str, repo: Path = REPO) -> dict:
    with tempfile.TemporaryDirectory() as tmp:
        ra, rb = Path(tmp) / "a", Path(tmp) / "b"
        ra.mkdir()
        rb.mkdir()
        git_extract(a, ra, repo)
        git_extract(b, rb, repo)
        return count(ra, rb)


def summary(c: dict) -> str:
    q, r = c["quantities"], c["raw_payloads"]
    sem = q["semantic_changes"]
    lines = [
        f"cases: {c['cases']['a']} -> {c['cases']['b']} ({c['cases']['shared']} shared, "
        f"{len(c['cases']['only_a'])} only in A, {len(c['cases']['only_b'])} only in B)",
        f"unmaterialised shared cases (not counted): {len(c['unmaterialised'])}"
        + "".join(f"\n  {x}" for x in c["unmaterialised"]),
        f"raw shared payloads: {r['shared']}, byte-identical {r['identical']}, "
        f"different {len(r['different'])}" + "".join(f"\n  {x}" for x in r["different"]),
        f"shared quantities: {q['shared']}; comparable {q['comparable']} "
        f"(bit-identical {q['bit_identical']}, different {len(q['different'])}); "
        f"semantic changes {len(sem)} "
        f"({sum(1 for s in sem if s['payload_identical'])} with identical payload); "
        f"metadata-only {q['metadata_only']}; only in A {len(q['only_a'])}, "
        f"only in B {len(q['only_b'])}",
    ]
    lines += [f"  different: {x}" for x in q["different"]]
    lines += [f"  semantic: {s['quantity']} {s['fields']}" for s in sem]
    return "\n".join(lines)


def check_records(directory: Path, repo: Path = REPO) -> list[str]:
    """Recompute every record; returns the names of the checked records."""
    checked = []
    for f in sorted(directory.glob("*.json")):
        rec = json.loads(f.read_text(encoding="utf-8"))
        try:
            a, b = rec["baseline"]["fixtures_tree"], rec["candidate"]["fixtures_tree"]
            expected = rec["counts"]
        except (KeyError, TypeError) as e:
            raise CountError(f"{f}: malformed record ({e!r})") from e
        got = count_git(a, b, repo)
        if got != expected:
            raise CountError(f"{f}: recomputed counts differ from the record:\n{summary(got)}")
        checked.append(f.name)
    if not checked:
        raise CountError(f"no record under {directory}")
    return checked


def main(argv: list[str]) -> int:
    args = argv[1:]
    as_json = "--json" in args
    args = [a for a in args if a != "--json"]
    try:
        if args and args[0] == "--check-records":
            if len(args) != 2:
                print(__doc__, file=sys.stderr)
                return 2
            names = check_records(Path(args[1]))
            print(f"regeneration counts: {len(names)} record(s) reproduced: {', '.join(names)}")
            return 0
        if args and args[0] == "--git" and len(args) == 3:
            c = count_git(args[1], args[2])
        elif len(args) == 2 and all(Path(x).is_dir() for x in args):
            c = count(Path(args[0]), Path(args[1]))
        else:
            print(__doc__, file=sys.stderr)
            return 2
    except (CountError, OSError, json.JSONDecodeError, KeyError) as e:
        print(f"regeneration_counts: {e}", file=sys.stderr)
        return 1
    print(json.dumps(c, indent=2, sort_keys=True) if as_json else summary(c))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
