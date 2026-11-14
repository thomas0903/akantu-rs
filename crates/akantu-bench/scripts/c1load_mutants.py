#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the mutation check of the doubled-node loads (D-105 /
# UD-1280), on a Linux host with Docker.
"""Apply each mutant of D-105's evidence list to a scratch clone, run the
tests that must kill it in the project image, restore the file.

    python3 -I crates/akantu-bench/scripts/c1load_mutants.py CLONE OUT_DIR [--flip]

CLONE is a scratch clone of this repository (modified and restored);
OUT_DIR receives one log per run and `summary.txt`. The baseline (no mutant)
runs first and must pass. Exit status 0 iff the baseline passes and every
mutant is killed (some test fails).

`--flip`: instead of the mutants, `CohesiveUpstreamCompat::upstream()`
without `doubled_node_keeps_external_force` (the oracle replays then run
with the corrected placement), over the release replays that combine
cohesive insertion with Neumann loads: a replay that fails depends on
upstream's placement (it needs the switch), one that passes does not.
"""
import os
import subprocess
import sys
import time
from pathlib import Path

MUTANTS = [
    (
        "M1",
        "the move skipped for ghost copies (copies the rank does not own)",
        "crates/akantu-solid/src/model.rs",
        "        store.move_doubled_nodes(mesh, f.as_mut_slice(), new_nodes, old_nodes)\n",
        "        let (new_nodes, old_nodes): (Vec<Idx>, Vec<Idx>) = new_nodes\n"
        "            .iter()\n"
        "            .zip(old_nodes)\n"
        "            .filter(|(n, _)| mesh.is_local_or_master_node(**n).unwrap_or(true))\n"
        "            .map(|(n, o)| (*n, *o))\n"
        "            .unzip();\n"
        "        store.move_doubled_nodes(mesh, f.as_mut_slice(), &new_nodes, &old_nodes)\n",
        [["-p", "akantu-bench", "--test", "doubled_node_load_side"]],
    ),
    (
        "M2",
        "the contribution moved for an element that does not use the new copy",
        "crates/akantu-model/src/neumann_store.rs",
        "                    if !origin.contains_key(&n) {\n"
        "                        continue;\n"
        "                    }\n"
        "                    let o = root(n)?;\n",
        "                    let Some((&nn, _)) = origin.iter().find(|&(_, &o)| o == n) else {\n"
        "                        continue;\n"
        "                    };\n"
        "                    let _ = root(nn)?;\n"
        "                    let (n, o) = (nn, n);\n",
        [
            ["-p", "akantu-bench", "--test", "doubled_node_load_side"],
            ["-p", "akantu-cohesive", "--test", "doubled_node_neumann"],
        ],
    ),
    (
        "M3",
        "the store not renumbered at an element erasure",
        "crates/akantu-model/src/neumann_store.rs",
        "        self.check_elements_removed(new_numbering)?;\n"
        "        for (ty, ghost) in self.blocks() {\n",
        "        self.check_elements_removed(new_numbering)?;\n"
        "        #[allow(unreachable_code)]\n"
        "        if true {\n"
        "            return Ok(());\n"
        "        }\n"
        "        for (ty, ghost) in self.blocks() {\n",
        [["-p", "akantu-solid", "--test", "neumann_store_events"]],
    ),
]


MUMPS = "@mumps-image"

FLIPS = [
    (
        "F1",
        "upstream() without doubled_node_keeps_external_force",
        "crates/akantu-cohesive/src/material.rs",
        "            doubled_node_keeps_external_force: true,\n",
        "            doubled_node_keeps_external_force: false,\n",
        [
            ["--release", "-p", "akantu", "--test", "examples_scalability_test"],
        ],
    ),
    (
        "F2",
        "the driver's rust_doubled_node_load = compat without upstream's placement",
        "crates/akantu-bench/src/config.rs",
        "            Self::Compat => compat == CompatMode::Upstream,\n",
        "            Self::Compat => {\n"
        "                let _ = compat;\n"
        "                false\n"
        "            }\n",
        [
            ["--release", "-p", "akantu-bench", "--test", "perf_cohesive_fixture"],
            [MUMPS, "--release", "-p", "akantu-bench", "--features", "akantu-bench/mumps",
             "--test", "perf_cohesive_mumps"],
        ],
    ),
]


def run_tests(clone, out, tag, test_sets):
    """Run every test set in the image; return (all passed, log names)."""
    ok = True
    names = []
    for i, args in enumerate(test_sets):
        log = out / f"{tag}-{i}.log"
        env = dict(os.environ)
        if args and args[0] == MUMPS:
            # the MUMPS image, its own target directory (other features)
            args = args[1:]
            env["AKANTU_RS_MPI_IMAGE"] = "akantu-rs-mumps:dev"
            env["AKANTU_RS_MPI_TARGET"] = "/target/mumps"
        cmd = ["scripts/mpi/in-docker.sh", "sh", "-c",
               "exec cargo test --locked " + " ".join(args) + " 2>&1"]
        t0 = time.time()
        with open(log, "w") as f:
            rc = subprocess.call(cmd, cwd=clone, stdout=f, stderr=subprocess.STDOUT, env=env)
        names.append(f"{log.name} rc={rc} {time.time() - t0:.0f}s")
        ok = ok and rc == 0
    return ok, names


def main():
    clone, out = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    flip = "--flip" in sys.argv[3:]
    mutants = FLIPS if flip else MUTANTS
    out.mkdir(parents=True, exist_ok=True)
    lines = []
    all_tests = []
    for m in mutants:
        for t in m[5]:
            if t not in all_tests:
                all_tests.append(t)
    base_ok, names = run_tests(clone, out, "baseline", all_tests)
    lines.append(f"baseline: {'PASS' if base_ok else 'FAIL'} ({'; '.join(names)})")
    killed_all = base_ok
    for tag, what, rel, old, new, tests in mutants:
        path = clone / rel
        text = path.read_text()
        if text.count(old) != 1:
            lines.append(f"{tag}: NOT APPLIED (anchor not found once in {rel})")
            killed_all = False
            continue
        path.write_text(text.replace(old, new))
        try:
            passed, names = run_tests(clone, out, tag, tests)
        finally:
            path.write_text(text)
            os.utime(path, None)  # restored files must look newer than the mutated build
        if flip:
            verdict = "UNCHANGED (does not need the switch)" if passed else \
                "CHANGED (needs the switch)"
        else:
            verdict = "SURVIVED" if passed else "KILLED"
            killed_all = killed_all and not passed
        lines.append(f"{tag} ({what}; {rel}): {verdict} ({'; '.join(names)})")
    (out / "summary.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    sys.exit(0 if (base_ok if flip else killed_all) else 1)


if __name__ == "__main__":
    main()
