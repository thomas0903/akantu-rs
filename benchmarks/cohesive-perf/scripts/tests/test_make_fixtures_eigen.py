#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Unit tests of the run validation of scripts/make_fixtures_eigen.py:
the committed `_eigen` cases pass, and
every kind of defective run is refused. Offline (no Docker).

    python3 -I benchmarks/cohesive-perf/scripts/tests/test_make_fixtures_eigen.py
"""

import importlib.util
import json
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPTS = os.path.dirname(HERE)
_spec = importlib.util.spec_from_file_location(
    "make_fixtures_eigen", os.path.join(SCRIPTS, "make_fixtures_eigen.py"))
mfe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mfe)

STEPS = 1500


def committed(case):
    d = os.path.join(mfe.OUT, case)
    files = {}
    for n in os.listdir(d):
        with open(os.path.join(d, n), "rb") as f:
            files[n] = f.read()
    return files


def as_run(case):
    """A synthetic repetition built from the committed case (what
    run-one.sh writes, with an Eigen static solve)."""
    files = committed(case)
    files["run.json"] = json.dumps({"exit_status": 0, "max_steps": STEPS, "ranks": 1,
                                    "static_sparse_solver": "eigen"}).encode()
    files["out.txt"] = b"Time step: 1.62714e-07\n"
    files["err.txt"] = b'"psize", "before_step"\n1, 0.1\n'
    return files


def edit(files, name, fn):
    out = dict(files)
    out[name] = fn(out[name])
    return out


def edit_json(files, name, fn):
    def f(data):
        d = json.loads(data)
        fn(d)
        return json.dumps(d).encode()
    return edit(files, name, f)


class CheckRun(unittest.TestCase):
    def assert_refused(self, files, fragment):
        problems = mfe.check_run(files, STEPS)
        self.assertTrue(any(fragment in p for p in problems),
                        f"expected a problem containing {fragment!r}, got {problems}")

    def test_committed_cases_pass(self):
        for case in mfe.CASES:
            self.assertEqual(mfe.check_run(as_run(case), STEPS), [], case)

    def test_truncated_history(self):
        run = edit(as_run("coh_np1_eigen"), "energies.csv",
                   lambda d: b"\n".join(d.split(b"\n")[:1200]) + b"\n")
        self.assert_refused(run, "truncated")

    def test_non_finite_energy(self):
        run = edit(as_run("ela_np1_eigen"), "energies.csv",
                   lambda d: d.replace(b"\n17, 0, ", b"\n17, nan, ", 1))
        self.assert_refused(run, "non-finite")

    def test_non_consecutive_steps(self):
        run = edit(as_run("ela_np1_eigen"), "energies.csv",
                   lambda d: d.replace(b"\n17, ", b"\n18, ", 1))
        self.assert_refused(run, "not consecutive")

    def test_wrong_step_count_and_exit(self):
        run = edit_json(as_run("coh_np1_eigen"), "run.json", lambda d: d.update(max_steps=200))
        self.assert_refused(run, "max_steps")
        run = edit_json(as_run("coh_np1_eigen"), "run.json", lambda d: d.update(exit_status=1))
        self.assert_refused(run, "exit status")

    def test_not_an_eigen_solve(self):
        run = edit_json(as_run("coh_np1_eigen"), "run.json",
                        lambda d: d.update(static_sparse_solver=None))
        self.assert_refused(run, "Eigen")
        run = edit(as_run("coh_np1_eigen"), "input.dat",
                   lambda d: d.replace(b"sparse_solver_type = eigen", b""))
        self.assert_refused(run, "sparse_solver_type")
        run = edit(as_run("coh_np1_eigen"), "err.txt", lambda d: d + b"MUMPS error -9\n")
        self.assert_refused(run, "MUMPS")

    def test_insertion_list_inconsistent(self):
        run = edit(as_run("coh_np1_eigen"), "insertions.csv",
                   lambda d: d.replace(b"\n775, 1, 2\n", b"\n775, 1, 3\n", 1))
        self.assert_refused(run, "running total")
        run = edit_json(as_run("coh_np1_eigen"), "summary.json",
                        lambda d: d.update(nb_cohesive_elements=403))
        self.assert_refused(run, "nb_cohesive_elements")
        run = edit(as_run("coh_np1_eigen"), "insertions.csv",
                   lambda d: b"\n".join(d.split(b"\n")[:100]) + b"\n")
        self.assert_refused(run, "nb_insertion_steps")

    def test_summary_values(self):
        run = edit_json(as_run("coh_np1_eigen"), "summary.json",
                        lambda d: d.update(time_step=-1.0))
        self.assert_refused(run, "time_step")
        run = edit(as_run("coh_np1_eigen"), "summary.json",
                   lambda d: d.replace(b'"energy_kinetic": 16280.110610860802',
                                       b'"energy_kinetic": NaN'))
        self.assert_refused(run, "non-finite")
        run = edit_json(as_run("coh_np1_eigen"), "summary.json", lambda d: d.update(psize=2))
        self.assert_refused(run, "psize")

    def test_missing_file(self):
        run = as_run("coh_np1_eigen")
        del run["summary_rank0.json"]
        self.assert_refused(run, "summary_rank0.json missing")


if __name__ == "__main__":
    unittest.main()
