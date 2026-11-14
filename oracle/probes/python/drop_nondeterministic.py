# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: determinism filter of the fixture generator.
"""Determinism check of a probe run against an independent second run.

  python3 -I drop_nondeterministic.py <area_run1> <area_run2> <akantu_src>
          --allow-list <known_nondeterminism.json>

Both runs must be complete and structurally identical: the same cases, the
same quantities (name, dtype, shape, axes), the same inputs (role, SHA-256)
and the same expectations. A missing second run, case, quantity or input is
an error (exit 1): golden data is never promoted without the determinism
check.

A quantity is excluded from run 1 only when it is listed in the reviewed
allow-list *and* is explained by a known reference defect:

* bytes differing between the two runs (oracle reading uninitialised memory);
* the Gmsh 2.x `tag_j` defect (UD-064, src/io/mesh_io/mesh_io_msh.cc ~l.400):
  tag j is appended only for elements having a j-th tag, so when elements of
  one type carry different tag counts `tag_j` is misaligned and its tail is
  uninitialised (possibly zero, hence invisible to the two-run comparison).
  The rule derives the affected arrays from the mesh file.

Any other difference stops the run (exit 1, nothing is modified). An
allow-list entry for a case of this run that the rule no longer derives is
stale and also stops the run. Every exclusion is recorded in the manifest
(`parameters.nondeterministic_quantities`: name, defect, reason, criterion)
and in `notes`; how it showed up in this run is logged.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

GMSH_TYPES = {1: "_segment_2", 2: "_triangle_3", 3: "_quadrangle_4", 4: "_tetrahedron_4",
              5: "_hexahedron_8", 6: "_pentahedron_6", 8: "_segment_3", 9: "_triangle_6",
              11: "_tetrahedron_10", 15: "_point_1", 16: "_quadrangle_8",
              17: "_hexahedron_20", 18: "_pentahedron_15"}


class DeterminismError(Exception):
    """The two runs cannot certify run 1."""


def msh2_tag_counts(path: Path) -> dict[str, set[int]]:
    """Number of tags per element of each Akantu type in a Gmsh 2.x file."""
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    if len(lines) < 2 or not lines[0].startswith("$MeshFormat") or not lines[1].startswith("2"):
        return {}
    try:
        start = lines.index("$Elements")
    except ValueError:
        return {}
    counts: dict[str, set[int]] = {}
    for line in lines[start + 2:]:
        if line.startswith("$EndElements"):
            break
        f = line.split()
        if len(f) >= 3 and int(f[1]) in GMSH_TYPES:
            counts.setdefault(GMSH_TYPES[int(f[1])], set()).add(int(f[2]))
    return counts


def msh2_bad_tags(path: Path) -> set[str]:
    """Quantity names `mesh_data/tag_j/<type>` corrupted by the reader."""
    bad = set()
    for t, c in msh2_tag_counts(path).items():
        if len(c) > 1:
            bad |= {f"mesh_data/tag_{j}/{t}" for j in range(min(c), max(c))}
    return bad


def load_allow_list(path: Path) -> dict[tuple[str, str], dict]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if data.get("format_version") != 1:
        raise DeterminismError(f"{path}: unsupported format_version")
    out = {}
    for e in data["entries"]:
        for key in ("case_id", "quantity", "defect", "reason", "evidence"):
            if not str(e.get(key, "")).strip():
                raise DeterminismError(f"{path}: entry {e} lacks `{key}`")
        k = (e["case_id"], e["quantity"])
        if k in out:
            raise DeterminismError(f"{path}: duplicate entry {k}")
        out[k] = e
    return out


def manifests(root: Path) -> dict[str, Path]:
    out = {}
    for m in sorted(root.rglob("manifest.json")):
        case = json.loads(m.read_text(encoding="utf-8"))["case_id"]
        out[case] = m
    return out


def structure(m: dict) -> dict:
    """Comparison-relevant structure of a manifest (everything but data
    checksums and run-specific provenance)."""
    return {
        "inputs": sorted((i["role"], i.get("source", ""), i["sha256"]) for i in m.get("inputs", [])),
        "expect": m.get("expect"),
        "status": m.get("status"),
        "steps": m.get("steps"),
        "quantities": sorted((q["name"], q["dtype"], tuple(q["shape"]), tuple(q["axes"]),
                              q["layout"]) for q in m["quantities"]),
    }


def plan(run1: Path, run2: Path, src: Path, allow: dict[tuple[str, str], dict]) -> dict:
    """Check both runs; return {case_id: [(quantity, entry, detected_by)]}."""
    if not run1.is_dir():
        raise DeterminismError(f"run 1 directory {run1} does not exist")
    if not run2.is_dir():
        raise DeterminismError(f"second run {run2} does not exist: the probe's second run "
                               "failed or was skipped; nothing can be certified")
    m1, m2 = manifests(run1), manifests(run2)
    if not m1:
        raise DeterminismError(f"run 1 {run1} contains no case")
    problems = [f"{c}: case only in run 1" for c in sorted(set(m1) - set(m2))]
    problems += [f"{c}: case only in run 2" for c in sorted(set(m2) - set(m1))]
    drops: dict[str, list] = {}
    for case in sorted(set(m1) & set(m2)):
        a = json.loads(m1[case].read_text(encoding="utf-8"))
        b = json.loads(m2[case].read_text(encoding="utf-8"))
        sa, sb = structure(a), structure(b)
        for key in sa:
            if sa[key] != sb[key]:
                problems.append(f"{case}: {key} differ between the runs")
        if any(sa[k] != sb[k] for k in sa):
            continue
        bsha = {q["name"]: q["sha256"] for q in b["quantities"]}
        known: set[str] = set()
        for i in a.get("inputs", []):
            if i.get("role") == "mesh" and i.get("source", "").startswith("upstream:"):
                known |= msh2_bad_tags(src / i["source"][len("upstream:"):])
        names = {q["name"] for q in a["quantities"]}
        found = []
        for q in a["quantities"]:
            how = []
            if bsha[q["name"]] != q["sha256"]:
                how.append("bytes differ between two independent runs")
            if q["name"] in known:
                how.append("Gmsh 2.x tag-count rule (UD-064)")
            if not how:
                continue
            entry = allow.get((case, q["name"]))
            if entry is None:
                problems.append(f"{case}: {q['name']}: unexpected nondeterminism ({'; '.join(how)}) "
                                "not in the reviewed allow-list")
                continue
            if q["name"] not in known:
                problems.append(f"{case}: {q['name']}: allow-listed for {entry['defect']} but the "
                                "defect rule does not explain it")
                continue
            found.append((q["name"], entry, how))
        for (c, qn), entry in allow.items():
            if c != case:
                continue
            if qn not in names:
                problems.append(f"{case}: allow-listed quantity {qn} is not produced by the probe")
            elif qn not in known:
                problems.append(f"{case}: allow-list entry {qn} ({entry['defect']}) is stale: "
                                "the defect rule no longer derives it")
        if found:
            drops[case] = found
    if problems:
        raise DeterminismError("determinism check failed:\n  " + "\n  ".join(problems))
    return drops


def apply(run1: Path, drops: dict) -> int:
    total = 0
    m1 = manifests(run1)
    for case, found in drops.items():
        path = m1[case]
        a = json.loads(path.read_text(encoding="utf-8"))
        names = {n for n, _, _ in found}
        for q in a["quantities"]:
            if q["name"] in names:
                (path.parent / q["file"]).unlink()
        a["quantities"] = [q for q in a["quantities"] if q["name"] not in names]
        # Deterministic record (whether the uninitialised bytes happened to
        # differ is a property of one run: logged, not stored).
        a.setdefault("parameters", {})["nondeterministic_quantities"] = [
            {"name": n, "defect": e["defect"], "reason": e["reason"],
             "criterion": "reviewed allow-list entry, derived by the Gmsh 2.x tag-count rule"}
            for n, e, _ in found]
        note = ("Unreliable oracle output excluded (reviewed allow-list "
                "oracle/probes/python/known_nondeterminism.json): " +
                ", ".join(f"{n} ({e['defect']})" for n, e, _ in found) +
                ". See docs/NUMERICAL_VALIDATION.md.")
        a["notes"] = (a.get("notes", "") + " " + note).strip()
        a.setdefault("storage", {})["bytes"] = sum(
            p.stat().st_size for p in path.parent.rglob("*")
            if p.is_file() and p.name not in ("manifest.json", "tolerances.json"))
        path.write_text(json.dumps(a, indent=2) + "\n", encoding="utf-8")
        total += len(found)
        for n, _, how in found:
            print(f"nondeterministic: {case}: excluded {n} ({'; '.join(how)})")
    return total


def main(argv: list[str]) -> int:
    args = list(argv[1:])
    allow_path = None
    if "--allow-list" in args:
        i = args.index("--allow-list")
        if i + 1 >= len(args):
            print("drop_nondeterministic: --allow-list needs a file", file=sys.stderr)
            return 2
        allow_path = Path(args[i + 1])
        del args[i:i + 2]
    if len(args) != 3 or allow_path is None:
        print(__doc__, file=sys.stderr)
        return 2
    run1, run2, src = (Path(a) for a in args)
    try:
        drops = plan(run1, run2, src, load_allow_list(allow_path))
    except (DeterminismError, OSError, json.JSONDecodeError, KeyError) as e:
        print(f"drop_nondeterministic: {e}", file=sys.stderr)
        return 1
    total = apply(run1, drops)
    print(f"determinism filter: {len(manifests(run1))} case(s) identical in both runs except "
          f"{total} allow-listed quantity(ies)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
