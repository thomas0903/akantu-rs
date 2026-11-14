#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; the cargo-feature counterpart
# of upstream's package switches, cmake/Modules/CMakePackagesSystem.cmake and
# packages/*.cmake, parity rows CI-018, CI-026, CI-027): plan the
# feature-matrix check from `cargo metadata --no-deps` output.
#
#   feature_matrix.py plan <metadata.json>   # TSV: label, needs (or -), cargo args
#   feature_matrix.py table <metadata.json>  # Markdown: crate, feature, closure, native needs
#
# Plan (one `cargo check` per line, deduplicated by effective feature set and
# target selection):
#   * workspace: default features with --all-targets; --all-features with
#     --all-targets; --no-default-features (libraries and binaries) and with
#     --all-targets (tests, examples, benches without default features);
#   * lint and documentation of the feature-gated code:
#     clippy -D warnings and rustdoc -D warnings with --all-features over
#     the workspace (the `lint` and `docs` tiers use default features only,
#     so mpi / scotch / mumps / cgal / python code was never linted);
#   * every member that declares features: --no-default-features, each
#     feature alone (--no-default-features --features F), both for the
#     library/binaries; default features and --all-features with
#     --all-targets (tests, examples and benches compile with both).
# `needs` lists the native prerequisites of the feature set (mpi, scotch,
# python, mumps, petsc), from NATIVE below; the tier skips (never passes) a
# check whose prerequisites the backend lacks.
"""Plan the cargo feature matrix of the workspace."""

from __future__ import annotations

import json
import sys

# Features that need native libraries or tools at build time (crate-level
# names used in this workspace; `docs/ARCHITECTURE.md`). A feature not listed is
# pure Rust. Matching is on the local feature name and on `crate/feature`
# entries of its definition, so umbrella features forwarding to these are
# covered by the closure.
NATIVE = {
    "mpi": ("mpi",),  # rsmpi + libclang + mpicc (akantu-comm build.rs)
    "scotch": ("scotch",),  # libscotch + C shim (akantu-mesh build.rs)
    "python": ("python",),  # pyo3 needs a Python 3 interpreter/headers
    "mumps": ("mumps",),  # MUMPS C shim
    "petsc": ("petsc",),  # PETSc C shim
    "parallel": ("mpi", "scotch"),
}


def closure(features: dict, start: list[str]) -> set[str]:
    """Local feature names reachable from `start` (dep: and crate/ entries
    are kept as strings so native needs of forwarded features are seen)."""
    seen: set[str] = set()
    todo = list(start)
    while todo:
        f = todo.pop()
        if f in seen:
            continue
        seen.add(f)
        for entry in features.get(f, []):
            if entry.startswith("dep:"):
                seen.add(entry)
            elif "/" in entry:
                seen.add(entry)
                local = entry.split("/", 1)[0].rstrip("?")
                if local in features:
                    todo.append(local)
            elif entry in features:
                todo.append(entry)
    return seen


def needs_of(names: set[str]) -> list[str]:
    out: set[str] = set()
    for n in names:
        base = n[4:] if n.startswith("dep:") else n
        last = base.rsplit("/", 1)[-1].rstrip("?")
        for key in (base, last):
            out.update(NATIVE.get(key, ()))
    return sorted(out)


def members(meta: dict) -> list[dict]:
    ids = set(meta.get("workspace_members", []))
    pkgs = [p for p in meta["packages"] if p["id"] in ids]
    return sorted(pkgs, key=lambda p: p["name"])


def plan(meta: dict) -> list[tuple[str, str, str]]:
    rows: list[tuple[str, str, str]] = []
    pkgs = members(meta)
    all_needs: set[str] = set()
    for p in pkgs:
        feats = p.get("features", {})
        all_needs.update(needs_of(closure(feats, [f for f in feats if f != "default"])))
    rows.append(("workspace:default:all-targets", "", "check --workspace --all-targets --locked"))
    rows.append(("workspace:all-features:all-targets", ",".join(sorted(all_needs)),
                 "check --workspace --all-targets --all-features --locked"))
    rows.append(("workspace:no-default-features", "", "check --workspace --lib --bins --no-default-features --locked"))
    rows.append(("workspace:no-default-features:all-targets", "",
                 "check --workspace --all-targets --no-default-features --locked"))
    allneeds = ",".join(sorted(all_needs))
    rows.append(("workspace:all-features:clippy", allneeds,
                 "clippy --workspace --all-targets --all-features --locked -- -D warnings"))
    rows.append(("workspace:all-features:rustdoc", allneeds,
                 "doc --workspace --no-deps --all-features --locked --keep-going"))
    for p in pkgs:
        feats = p.get("features", {})
        names = [f for f in feats if f != "default"]
        if not names:
            continue
        name = p["name"]
        has_bins = any("bin" in t.get("kind", []) for t in p.get("targets", []))
        lib_targets = "--lib --bins" if has_bins else "--lib"
        seen: set[tuple[frozenset, str]] = set()

        def add(label: str, start: list[str], args: str, targets: str) -> None:
            eff = frozenset(closure(feats, start))
            key = (eff, targets)
            if key in seen:
                return
            seen.add(key)
            rows.append((f"{name}:{label}", ",".join(needs_of(set(eff))),
                         f"check -p {name} {targets} --locked {args}".rstrip()))

        add("no-default-features", [], "--no-default-features", lib_targets)
        for f in sorted(names):
            add(f"feature={f}", [f], f"--no-default-features --features {f}", lib_targets)
        add("default:all-targets", ["default"] if "default" in feats else [], "", "--all-targets")
        add("all-features:all-targets", names, "--all-features", "--all-targets")
    return rows


def table(meta: dict) -> list[str]:
    out = ["| Crate | Feature | Default | Enables (closure) | Native needs |", "|---|---|---|---|---|"]
    for p in members(meta):
        feats = p.get("features", {})
        default = set(feats.get("default", []))
        for f in sorted(x for x in feats if x != "default"):
            cl = sorted(closure(feats, [f]) - {f})
            needs = needs_of(closure(feats, [f]))
            out.append(f"| `{p['name']}` | `{f}` | {'yes' if f in default else 'no'} | "
                       f"{', '.join(f'`{c}`' for c in cl) or '—'} | {', '.join(needs) or '—'} |")
    return out


def main(argv: list[str]) -> int:
    if len(argv) != 3 or argv[1] not in ("plan", "table"):
        print(__doc__ + "\nusage: feature_matrix.py plan|table <metadata.json>", file=sys.stderr)
        return 2
    with open(argv[2]) as f:
        meta = json.load(f)
    if argv[1] == "plan":
        for label, needs, args in plan(meta):
            # "-" for no needs: an empty field would vanish when a shell
            # splits the line on tabs (IFS whitespace collapses).
            print(f"{label}\t{needs or '-'}\t{args}")
    else:
        print("\n".join(table(meta)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
