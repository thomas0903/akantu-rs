#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Accounting of
# the upstream test meshes, input files and golden files (parity rows
# GOLD-001…015, MDATA-001…006). No C++ counterpart.
"""Account for every upstream test mesh, input file and golden file.

For every data file of the reference tree (`test/`, `extra_packages/*/test/`,
`third-party/*/test/`, `examples/`: `.msh .geo .dat .verified .csv .out .in`,
the Matrix-Market file `matrix` and the IGFEM `mesh_info_*.txt`) the manifest
records:

* its upstream path and SHA-256;
* every **verbatim copy** in this repository (crate test data, fixture
  `inputs/`, parser corpus, frozen meshes …): same SHA-256;
* for a `.geo`, the frozen Gmsh outputs generated from it
  (`fixtures/meshes/frozen_meshes.json`, D-009);
* for a `.dat` without a file copy, the Rust sources that embed its content
  (every significant line, in order);
* the inventory IDs that consume it (appendix of
  `docs/inventory/tests_examples_python.md`, or EXTRA_CONSUMERS for the
  third-party test data), plus the successors of every consumer TEST/EX/PY
  row that is `Superseded` (`split → …` in its parity row),
  and which of them are ported;
* for a `.geo`, whether an active (uncommented) line of a CMakeLists.txt of
  its directory or of an ancestor directory names it (`build_rule`);
* a status: `copied`, `frozen`, `embedded`, `source-only` (a `.geo` without
  a build rule: source of a committed mesh), `owed` (a consumer
  row — a TEST/EX/PY row — is ported
  — `In progress`, `Implemented, unverified` or `Verified` — but the file is
  not in the repository: a gap, listed with a reason in NOTES or fixed), or
  `not-needed` (no ported consumer).

Usage:
  scripts/gold/upstream_data_manifest.py [--reference DIR] [--check]
Writes `crates/akantu-validation/tests/data/upstream_data_manifest.json` (or,
with `--check`, compares the freshly computed manifest with the committed one,
prints the differences and exits 1 on a difference). Exits 1 as well when a
file is `owed` without a note, when an upstream data file has no consumer
row, or when a superseded consumer row names no successor.

Every merge that adds, removes or changes a copy of an upstream file (crate
test data, fixture `inputs/`, …), changes `fixtures/meshes/frozen_meshes.json`
or the status of a TEST/EX/PY row regenerates the manifest in the merge
commit (docs/TESTING.md). The Rust test
`akantu-validation/tests/upstream_data_manifest.rs` re-derives, without the
reference checkout, everything that depends on this repository (copies by
SHA-256 over the git-visible files of `copy_roots`, frozen meshes, consumers,
ported consumers, statuses, the `tests/data/upstream` READMEs) and fails when
the committed manifest no longer matches.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REF_SHA = "35a851780d0b9471d258ed53a003ae98aaa95ea0"
MANIFEST = ROOT / "crates/akantu-validation/tests/data/upstream_data_manifest.json"
INVENTORY = ROOT / "docs/inventory/tests_examples_python.md"
PARITY = ROOT / "docs/FEATURE_PARITY.md"
FROZEN = ROOT / "fixtures/meshes/frozen_meshes.json"

DATA_EXT = {".msh", ".geo", ".dat", ".verified", ".csv", ".out", ".in"}
PORTED = {"In progress", "Implemented, unverified", "Verified"}
# Directories of this repository searched for copies (relative to ROOT);
# recorded in the manifest so that the Rust test scans the same trees.
COPY_ROOTS = ["crates", "fixtures", "oracle/probes", "examples", "python"]
# Empty files (.gitkeep …) are no copy of anything; larger files are skipped.
MAX_COPY_BYTES = 64 << 20
SKIP_DIRS = {".git", "__pycache__"}
# Test/example ID prefixes whose parity status decides whether a file is
# owed (GOLD/MDATA rows are this accounting itself).
CONSUMER_PREFIXES = ("TEST", "EX", "PY")

# Consumers of upstream data files outside the appendix of
# docs/inventory/tests_examples_python.md: the IOHelper test data (its test
# sources are mapped to DUMP-004/DUMP-005 in the appendix of
# docs/inventory/core.md; ported by akantu-io/tests/dump_oracle.rs
# `iohelper_tests_match_oracle`). Recorded in the manifest.
EXTRA_CONSUMERS: dict[str, list[str]] = {
    "third-party/iohelper/test/cube.geo": ["DUMP-004", "DUMP-005"],
    "third-party/iohelper/test/cube.msh": ["DUMP-004", "DUMP-005"],
}

# Upstream files with a ported consumer that are deliberately not in the
# repository, with the reason (reviewed by hand; see docs/inventory/
# golden_files.md). Keys are upstream paths.
_PAR_COH_VERIFIED = (
    "verified output of test_cohesive_parallel_intrinsic_implicit_insertion (TEST-242, not "
    "started): written by a pre-v4 build (DistributedSynchronizer API, removed) with a Scotch "
    "partition of that time, so no current run is comparable; the ported tests of the "
    "directory (TEST-235, TEST-237..241, TEST-243, TEST-244) do not read it"
)
NOTES: dict[str, str] = {
    "test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/"
    f"test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_{r}_out_of_{n}.out":
    _PAR_COH_VERIFIED
    for n in (2, 4)
    for r in range(n)
}
NOTES.update(
    {
        # TEST-228 (PETSc backend)
        "test/test_solver/test_petsc_matrix_profile.verified": (
            "TEST-228: upstream test unregistered (register_test commented out in "
            "test/test_solver/CMakeLists.txt), so upstream never compares this text; the port "
            "ports the test's intent (matrix profile and assembly, petsc_mpi.rs) without the "
            "printed dump; byte comparison owed with GOLD-003 (docs/inventory/golden_files.md #17)"
        ),
        "test/test_solver/test_petsc_matrix_profile_parallel.verified": (
            "TEST-228: upstream test unregistered (register_test commented out in "
            "test/test_solver/CMakeLists.txt), so upstream never compares this text; the port "
            "ports the test's intent (matrix profile and assembly on 1/2/4 ranks, petsc_mpi.rs) "
            "without the printed dump; byte comparison owed with GOLD-003 "
            "(docs/inventory/golden_files.md #18)"
        ),
    }
)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def is_data_file(rel: str) -> bool:
    name = rel.rsplit("/", 1)[-1]
    if name == "matrix" and "test_embedded_interface" in rel:
        return True
    if name.startswith("mesh_info_") and name.endswith(".txt"):
        return True
    return os.path.splitext(name)[1] in DATA_EXT


def upstream_files(ref: Path) -> list[str]:
    out = []
    roots = ([ref / "test", ref / "examples"] + sorted((ref / "extra_packages").glob("*/test"))
             + sorted((ref / "third-party").glob("*/test")))
    for base in roots:
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS)
            for fn in sorted(filenames):
                rel = os.path.relpath(os.path.join(dirpath, fn), ref).replace(os.sep, "/")
                if rel.startswith("test/ci/"):
                    continue
                if is_data_file(rel):
                    out.append(rel)
    return sorted(out)


def repo_files() -> list[Path]:
    """Files of this repository visible to git (tracked, or untracked and not
    ignored) under COPY_ROOTS: ignored/external fixture data is not part of
    a clone and cannot serve as a copy."""
    import subprocess
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--cached", "--others",
                          "--exclude-standard", "--"] + COPY_ROOTS,
                         capture_output=True, check=True).stdout.decode()
    return sorted({ROOT / p for p in out.split("\0") if p and (ROOT / p).is_file()})


def expand_ranges(ids: list[str], text: str) -> list[str]:
    """`TEST-014…TEST-021` style ranges of an appendix cell."""
    out = set(ids)
    for a, b in re.findall(r"\b([A-Z]+-\d{3})…([A-Z]+-\d{3})\b", text):
        pa, na = a.rsplit("-", 1)
        pb, nb = b.rsplit("-", 1)
        if pa == pb:
            for n in range(int(na), int(nb) + 1):
                out.add(f"{pa}-{n:03d}")
    return sorted(out)


def inventory_consumers_full() -> dict[str, list[str]]:
    cons: dict[str, list[str]] = {}
    in_app = False
    for line in INVENTORY.read_text().splitlines():
        if line.startswith("## Appendix: file coverage"):
            in_app = True
            continue
        if not in_app:
            continue
        m = re.match(r"^\| `([^`]+)` \| (.+) \|\s*$", line)
        if m:
            ids = sorted(set(re.findall(r"\b[A-Z]+-\d{3}\b", m.group(2))))
            cons[m.group(1)] = expand_ranges(ids, m.group(2))
    return cons


def parity_rows() -> dict[str, dict[str, str]]:
    """Cells of every parity row, by column name (the header of its table).
    A row with more cells than its header (an unescaped `|` in a late cell)
    keeps the cells of the header's columns in order."""
    rows: dict[str, dict[str, str]] = {}
    header: list[str] | None = None
    for line in PARITY.read_text().splitlines():
        if line.startswith("| ID |"):
            header = [c.strip() for c in re.split(r"(?<!\\)\|", line.strip())[1:-1]]
            continue
        if header is not None and line.startswith("| "):
            cells = [c.strip() for c in re.split(r"(?<!\\)\|", line.strip())[1:-1]]
            if cells and re.match(r"^[A-Z]+-\d{3}$", cells[0]):
                rows[cells[0]] = dict(zip(header, cells))
    return rows


def parity_status(rows: dict[str, dict[str, str]]) -> dict[str, str]:
    return {k: v.get("Status", "") for k, v in rows.items()}


def is_superseded(status: str) -> bool:
    return status.strip("* ").startswith("Superseded")


def successors(row: str, rows: dict[str, dict[str, str]], seen: frozenset = frozenset()) -> list[str]:
    """Rows that replace a `Superseded` row: the IDs (and `A…B` ranges)
    after `→` in its Status/Notes cells (`split → TEST-235…TEST-244`),
    recursively. Empty when none is named."""
    cells = rows.get(row, {})
    text = cells.get("Status", "") + " " + cells.get("Notes", cells.get("Discrepancies", ""))
    out: set[str] = set()
    for m in re.finditer(r"→\s*([^;(]*)", text):
        seg = m.group(1)
        for s in expand_ranges(re.findall(r"\b[A-Z]+-\d{3}\b", seg), seg):
            if s == row or s in seen:
                continue
            out.add(s)
            if is_superseded(rows.get(s, {}).get("Status", "")):
                out.update(successors(s, rows, seen | {row}))
    return sorted(out)


def superseded_consumers(consumers: set[str], rows: dict[str, dict[str, str]]) -> dict[str, list[str]]:
    """Successors of the consumer test/example rows that are `Superseded`
    (a file consumed by a superseded row is owed as soon
    as one of its successors is ported)."""
    out = {}
    for c in sorted(consumers):
        if c.split("-")[0] in CONSUMER_PREFIXES and is_superseded(rows.get(c, {}).get("Status", "")):
            out[c] = successors(c, rows)
    return out


def cmake_code(text: str) -> str:
    """CMake source without comments: bracket comments `#[[…]]`/`#[=[…]=]`
    and line comments from an unquoted `#` (commented-out
    `#add_mesh(…)` lines are no build rule)."""
    text = re.sub(r"#\[(=*)\[.*?\]\1\]", "", text, flags=re.S)
    out, quoted = [], False
    for line in text.splitlines():
        res, i = [], 0
        while i < len(line):
            ch = line[i]
            if quoted and ch == "\\":
                res.append(line[i:i + 2])
                i += 2
                continue
            if ch == '"':
                quoted = not quoted
            elif ch == "#" and not quoted:
                break
            res.append(ch)
            i += 1
        out.append("".join(res))
    return "\n".join(out)


def has_build_rule(ref: Path, rel: str, top: Path) -> bool:
    """Whether an active line of the CMakeLists.txt of the file's directory,
    or of an ancestor directory up to `top`, names it (relative to that
    CMakeLists: `add_mesh` source, `FILES_TO_COPY`, …)."""
    f = ref / rel
    d = f.parent
    while True:
        cm = d / "CMakeLists.txt"
        if cm.exists():
            name = f.relative_to(d).as_posix()
            if re.search(r"(^|[\s/}])" + re.escape(name) + r"($|[\s)\"])",
                         cmake_code(cm.read_text(errors="replace")), re.M):
                return True
        if d == top or d == ref:
            return False
        d = d.parent


def upstream_top(ref: Path, rel: str) -> Path:
    parts = rel.split("/")
    if parts[0] in ("extra_packages", "third-party"):
        return ref / "/".join(parts[:3])
    return ref / parts[0]


def significant_lines(text: str) -> list[str]:
    out = []
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        line = re.sub(r"\s+", " ", line)
        if line:
            out.append(line)
    return out


def rust_sources() -> list[tuple[str, list[str]]]:
    out = []
    for p in sorted((ROOT / "crates").rglob("*.rs")):
        if "target" in p.parts:
            continue
        rel = p.relative_to(ROOT).as_posix()
        # string-literal content approximated by every source line, stripped
        # of leading/trailing quotes, escapes and comments
        lines = []
        for line in p.read_text(errors="replace").splitlines():
            line = line.strip().strip('"').replace("\\n", "").replace('\\"', '"')
            line = line.split("#", 1)[0].strip()
            line = re.sub(r"\s+", " ", line)
            if line:
                lines.append(line)
        out.append((rel, lines))
    return out


def embedded_in(sig: list[str], sources: list[tuple[str, list[str]]]) -> list[str]:
    """Rust files containing every significant line of a `.dat`, in order
    (at least 3 lines, to avoid trivial matches)."""
    if len(sig) < 3:
        return []
    hits = []
    for rel, lines in sources:
        i = 0
        for line in lines:
            if i < len(sig) and line == sig[i]:
                i += 1
        if i == len(sig):
            hits.append(rel)
    return hits


def build(ref: Path) -> tuple[dict, list[str]]:
    """The manifest, and the errors that make the run fail."""
    errors: list[str] = []
    head = (ref / ".git").exists()
    ups = upstream_files(ref)
    by_sha: dict[str, list[str]] = {}
    for p in repo_files():
        try:
            size = p.stat().st_size
        except OSError:
            continue
        if size == 0 or size > MAX_COPY_BYTES:
            continue
        by_sha.setdefault(sha256(p), []).append(p.relative_to(ROOT).as_posix())
    frozen = json.loads(FROZEN.read_text())
    frozen_by_geo: dict[str, list[dict]] = {}
    for m in frozen["meshes"]:
        frozen_by_geo.setdefault(m["geo"], []).append(m)
    cons = inventory_consumers_full()
    rows = parity_rows()
    status = parity_status(rows)
    sources = rust_sources()
    entries = []
    all_superseded: dict[str, list[str]] = {}
    for rel in ups:
        s = sha256(ref / rel)
        copies = sorted(by_sha.get(s, []))
        direct = set(cons.get(rel, [])) | set(EXTRA_CONSUMERS.get(rel, []))
        if not direct:
            errors.append(f"{rel}: no consumer row (appendix of {INVENTORY.relative_to(ROOT)} "
                          "or EXTRA_CONSUMERS)")
        sup = superseded_consumers(direct, rows)
        for c, succ in sup.items():
            all_superseded[c] = succ
            if not succ:
                errors.append(f"{rel}: consumer {c} is Superseded but its parity row names no "
                              "successor (`split → …`)")
        consumers = sorted(direct.union(*sup.values()))
        ported = sorted(c for c in consumers
                        if c.split("-")[0] in CONSUMER_PREFIXES and status.get(c) in PORTED)
        e: dict = {
            "upstream": rel,
            "sha256": s,
            "bytes": (ref / rel).stat().st_size,
            "copies": copies,
            "consumers": consumers,
            "ported_consumers": ported,
        }
        frozen_out = []
        if rel.endswith(".geo"):
            e["build_rule"] = has_build_rule(ref, rel, upstream_top(ref, rel))
            for m in frozen_by_geo.get(rel, []):
                if m["geo_sha256"] != s:
                    raise SystemExit(f"{rel}: frozen_meshes.json geo SHA-256 differs")
                frozen_out.append({"path": "fixtures/meshes/" + m["path"], "sha256": m["sha256"],
                                   "order": m["order"], "dim": m["dim"]})
        if frozen_out:
            e["frozen"] = frozen_out
        emb = []
        if not copies and rel.endswith(".dat"):
            emb = embedded_in(significant_lines((ref / rel).read_text(errors="replace")), sources)
            if emb:
                e["embedded_in"] = emb
        if copies:
            st = "copied"
        elif frozen_out:
            st = "frozen"
        elif emb:
            st = "embedded"
        elif rel.endswith(".geo") and not e["build_rule"]:
            # source geometry of a committed mesh: not used by the upstream
            # build (no add_mesh, no FILES_TO_COPY)
            st = "source-only"
        elif ported:
            st = "owed"
        else:
            st = "not-needed"
        e["status"] = st
        if rel in NOTES:
            e["note"] = NOTES[rel]
        if st == "owed" and rel not in NOTES:
            errors.append(f"OWED (no note): {rel} {ported}")
        entries.append(e)
    summary: dict[str, int] = {}
    for e in entries:
        summary[e["status"]] = summary.get(e["status"], 0) + 1
    manifest = {
        "description": "Accounting of upstream test meshes, input files and golden files "
                       "(scripts/gold/upstream_data_manifest.py). `copies` are byte-identical to the "
                       "upstream file (SHA-256); `frozen` are the Gmsh outputs of a .geo (D-009).",
        "reference_sha": REF_SHA,
        "reference_is_git_checkout": head,
        "copy_roots": COPY_ROOTS,
        "max_copy_bytes": MAX_COPY_BYTES,
        "consumer_prefixes": list(CONSUMER_PREFIXES),
        "extra_consumers": EXTRA_CONSUMERS,
        "superseded_consumers": dict(sorted(all_superseded.items())),
        "summary": dict(sorted(summary.items())),
        "copy_pairs": sum(len(e["copies"]) for e in entries),
        "copy_files": len({c for e in entries for c in e["copies"]}),
        "files": entries,
    }
    return manifest, errors


def describe_diff(old: dict, new: dict, limit: int = 60) -> list[str]:
    """Human-readable differences between two manifests (per upstream file)."""
    out = []
    for k in sorted(set(old) | set(new)):
        if k != "files" and old.get(k) != new.get(k):
            out.append(f"{k}: {json.dumps(old.get(k))} -> {json.dumps(new.get(k))}")
    of = {e["upstream"]: e for e in old.get("files", [])}
    nf = {e["upstream"]: e for e in new.get("files", [])}
    for up in sorted(set(of) | set(nf)):
        a, b = of.get(up), nf.get(up)
        if a is None:
            out.append(f"+ {up} ({b['status']})")
        elif b is None:
            out.append(f"- {up}")
        elif a != b:
            for key in sorted(set(a) | set(b)):
                va, vb = a.get(key), b.get(key)
                if va == vb:
                    continue
                if isinstance(va, list) and isinstance(vb, list) and all(
                        isinstance(x, str) for x in va + vb):
                    added = sorted(set(vb) - set(va))
                    removed = sorted(set(va) - set(vb))
                    out.append(f"~ {up}: {key} +{added} -{removed}")
                else:
                    out.append(f"~ {up}: {key} {json.dumps(va)} -> {json.dumps(vb)}")
    if len(out) > limit:
        out = out[:limit] + [f"… {len(out) - limit} more differences"]
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--reference", default=os.environ.get("AKANTU_SRC_HOST"),
                    help="the upstream Akantu checkout (default: $AKANTU_SRC_HOST)")
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()
    if not args.reference:
        print("no reference: pass --reference or set AKANTU_SRC_HOST to the upstream Akantu "
              "checkout", file=sys.stderr)
        return 1
    ref = Path(args.reference)
    if not (ref / "test").is_dir():
        print(f"{ref}: not an Akantu source tree (pass --reference)", file=sys.stderr)
        return 1
    if (ref / ".git").exists():
        import subprocess
        sha = subprocess.run(["git", "-C", str(ref), "rev-parse", "HEAD"], capture_output=True,
                             text=True, check=True).stdout.strip()
        if sha != REF_SHA:
            print(f"reference at {sha}, expected {REF_SHA}", file=sys.stderr)
            return 1
    m, errors = build(ref)
    text = json.dumps(m, indent=1, sort_keys=True) + "\n"
    rc = 0
    if args.check:
        old = json.loads(MANIFEST.read_text()) if MANIFEST.exists() else {}
        if not MANIFEST.exists() or MANIFEST.read_text() != text:
            print(f"{MANIFEST.relative_to(ROOT)} is not up to date (regenerate it with "
                  "scripts/gold/upstream_data_manifest.py):", file=sys.stderr)
            for line in describe_diff(old, m):
                print("  " + line, file=sys.stderr)
            rc = 1
        else:
            print("manifest up to date:", m["summary"], f"({m['copy_pairs']} copy pairs, "
                  f"{m['copy_files']} distinct copy files)")
    else:
        MANIFEST.parent.mkdir(parents=True, exist_ok=True)
        MANIFEST.write_text(text)
        print("wrote", MANIFEST.relative_to(ROOT), m["summary"], f"({m['copy_pairs']} copy pairs, "
              f"{m['copy_files']} distinct copy files)")
    for e in errors:
        print("ERROR:", e, file=sys.stderr)
    return 1 if errors else rc


if __name__ == "__main__":
    sys.exit(main())
