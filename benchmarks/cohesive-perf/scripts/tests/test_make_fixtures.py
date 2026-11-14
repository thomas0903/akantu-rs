# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code): tests of
# make_fixtures.py on synthetic runs built
# from the committed fixture outputs. Run: benchmarks/cohesive-perf/README.md.

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import synth  # noqa: E402
from synth import mf  # noqa: E402


class FixtureGeneration(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.fx = synth.sandbox_fixtures(self.tmp.name)
        self.saved = (mf.FIXTURES, mf.FROZEN_MESH)
        mf.FIXTURES = self.fx
        mf.FROZEN_MESH = os.path.join(self.fx, "perf_cohesive", "meshes", "cube-h0.12.msh")
        self.sums_before = mf.perfcheck.read_text(os.path.join(self.fx, "SHA256SUMS"))
        self.runs = synth.fixture_runs()

    def tearDown(self):
        mf.FIXTURES, mf.FROZEN_MESH = self.saved
        self.tmp.cleanup()

    def out(self, name="perf_cohesive_test"):
        return os.path.join(self.fx, name)

    def generate(self, name="perf_cohesive_test", replace=False):
        return mf.generate(self.runs, 1500, 3, name, replace)

    def assert_rejected(self, pattern, name="perf_cohesive_test"):
        with self.assertRaisesRegex(mf.FixtureError, pattern):
            self.generate(name)
        self.assertFalse(os.path.exists(self.out(name)), "nothing may be written")
        self.assertEqual(mf.perfcheck.read_text(os.path.join(self.fx, "SHA256SUMS")), self.sums_before)
        self.assertEqual([n for n in os.listdir(self.fx) if n.startswith(".")], [],
                         "no staging directory may be left behind")

    # -- the positive path -------------------------------------------------
    def test_valid_campaign_is_written_with_labels(self):
        self.generate()
        out = self.out()
        q = mf.perfcheck.read_json(os.path.join(out, "qualification.json"))
        for case in ("coh_np1", "ela_np1"):
            self.assertTrue(all(v == "qualified" for v in q["cases"][case]["files"].values()))
        np2 = q["cases"]["coh_np2"]["files"]
        self.assertEqual(np2["energies.csv"], "diagnostic")
        self.assertEqual(np2["partition.csv"], "qualified")
        self.assertEqual(np2["insertions.csv"], "qualified")
        self.assertIn("preparatory", q["status"])
        attempts = mf.perfcheck.read_json(os.path.join(out, "attempts.json"))
        self.assertEqual(len(attempts), 4 * 3 + 4)
        self.assertTrue(all(a["valid"] and a["exit_status"] == 0 for a in attempts.values()))
        prov = mf.perfcheck.read_json(os.path.join(out, "provenance.json"))
        self.assertEqual(prov["platform"], ["x86_64"])
        self.assertEqual(len(prov["build_records"]), 2)
        self.assertEqual(mf.perfcheck.read_text(os.path.join(out, "coh_np1", "input.dat")).count(
            "mesh = ../../perf_cohesive/meshes/cube-h0.12.msh"), 1)
        sums = mf.perfcheck.read_text(os.path.join(self.fx, "SHA256SUMS"))
        self.assertIn("perf_cohesive_test/qualification.json", sums)
        self.assertIn("aaa/other.npy", sums)
        mf.check_dir(out)
        # the committed lines are untouched
        self.assertEqual([l for l in sums.splitlines() if l.split("  ")[1].startswith("perf_cohesive/")],
                         [l for l in self.sums_before.splitlines()
                          if l.split("  ")[1].startswith("perf_cohesive/")])

    def test_outputs_match_the_committed_files(self):
        self.runs = synth.fixture_runs(nonrepeatable_np2=False)
        self.generate("perf_cohesive", replace=True)
        for case in mf.CASES:
            for name in ("energies.csv", "insertions.csv", "summary.json"):
                a = mf.perfcheck.read_bytes(os.path.join(self.fx, "perf_cohesive", case, name))
                b = mf.perfcheck.read_bytes(os.path.join(synth.COMMITTED, case, name))
                self.assertEqual(a, b, f"{case}/{name}")

    def test_existing_output_needs_replace(self):
        self.generate()
        with open(os.path.join(self.out(), "stale.txt"), "w") as f:
            f.write("old")
        with self.assertRaisesRegex(mf.FixtureError, "--replace"):
            self.generate()
        self.generate(replace=True)
        self.assertFalse(os.path.exists(os.path.join(self.out(), "stale.txt")))

    # -- negative regressions ------------------------------------------------
    def test_ud216_retry_warning_is_rejected(self):
        key = "fixtures/strict/coh_np1/rep1"
        self.runs[key]["err.txt"] = synth.RETRY_WARNING.encode() + self.runs[key]["err.txt"]
        self.assert_rejected("MUMPS dynamic memory is insufficient")

    def test_retry_warning_in_stdout_is_rejected(self):
        key = "fixtures/strict/coh_np2/rep2"
        self.runs[key]["out.txt"] += synth.RETRY_WARNING.encode()
        self.assert_rejected("UD-216")

    def test_missing_stderr_is_rejected(self):
        del self.runs["fixtures/strict/ela_np1/rep3"]["err.txt"]
        self.assert_rejected("err.txt missing")

    def test_truncated_history_is_rejected(self):
        key = "fixtures/strict/coh_np1/rep2"
        lines = self.runs[key]["energies.csv"].decode().splitlines()
        self.runs[key]["energies.csv"] = ("\n".join(lines[:-1]) + "\n").encode()
        self.assert_rejected("1500 rows, expected 1501")

    def test_nonfinite_history_is_rejected(self):
        key = "fixtures/default/ela_np1/rep1"
        lines = self.runs[key]["energies.csv"].decode().splitlines()
        f = lines[10].split(", ")
        f[3] = "nan"
        lines[10] = ", ".join(f)
        self.runs[key]["energies.csv"] = ("\n".join(lines) + "\n").encode()
        self.assert_rejected("non-finite")

    def test_wrong_step_count_is_rejected(self):
        key = "fixtures/strict/coh_np2_global/rep1"
        meta = json.loads(self.runs[key]["run.json"])
        meta["max_steps"] = 1400
        self.runs[key]["run.json"] = json.dumps(meta).encode()
        self.assert_rejected("max_steps")

    def test_nonzero_exit_is_rejected(self):
        key = "fixtures/strict/coh_np1_fnte_joss/rep1"
        meta = json.loads(self.runs[key]["run.json"])
        meta["exit_status"] = 134
        self.runs[key]["run.json"] = json.dumps(meta).encode()
        self.assert_rejected("exit status 134")

    def test_missing_run_is_rejected(self):
        del self.runs["fixtures/default/coh_np1/rep1"]
        self.assert_rejected("run directory missing")

    def test_wrong_mode_in_input_is_rejected(self):
        key = "fixtures/strict/coh_np2_global/rep2"
        self.runs[key]["input.dat"] = self.runs[key]["input.dat"].replace(
            b"blocked_node = global", b"blocked_node = local")
        self.assert_rejected("input.dat differs")

    def test_inconsistent_insertion_totals_are_rejected(self):
        key = "fixtures/strict/coh_np1/rep3"
        s = json.loads(self.runs[key]["summary.json"])
        s["nb_cohesive_elements"] += 1
        self.runs[key]["summary.json"] = json.dumps(s).encode()
        self.assert_rejected("nb_cohesive_elements")

    def test_summary_energy_must_match_history(self):
        key = "fixtures/strict/ela_np1/rep1"
        s = json.loads(self.runs[key]["summary.json"])
        s["energy_potential"] *= 1.0000001
        self.runs[key]["summary.json"] = json.dumps(s).encode()
        self.assert_rejected("energy_potential")

    def test_chrono_inconsistent_with_steps_is_rejected(self):
        key = "fixtures/strict/coh_np1/rep1"
        self.runs[key]["chrono.csv"] = synth.chrono_csv(1, 1400, True, True)
        self.assert_rejected("solve_step nb_rep")

    def test_serial_nonrepeatability_is_rejected(self):
        key = "fixtures/strict/coh_np1/rep2"
        self.runs[key]["energies.csv"] = synth.perturb_energies(self.runs[key]["energies.csv"], 5)
        self.assert_rejected("serial repetitions are not bitwise identical")

    def test_nonrepeatable_partition_is_rejected(self):
        key = "fixtures/strict/coh_np2/rep2"
        lines = self.runs[key]["partition.csv"].decode().splitlines()
        a, b = lines[1].rsplit(", ", 1), lines[2].rsplit(", ", 1)
        lines[1], lines[2] = a[0] + ", " + str(1 - int(a[1])), b[0] + ", " + str(1 - int(b[1]))
        self.runs[key]["partition.csv"] = ("\n".join(lines) + "\n").encode()
        self.assert_rejected("partition not repeatable")

    def test_joss_emulation_must_not_change_physics(self):
        key = "fixtures/strict/coh_np1_fnte_joss/rep1"
        self.runs[key]["energies.csv"] = synth.perturb_energies(self.runs[key]["energies.csv"], 900)
        self.assert_rejected("JOSS emulation changes the physics")

    def test_joss_mode_must_skip_event_calls(self):
        key = "fixtures/strict/coh_np1_fnte_joss/rep1"
        r = json.loads(self.runs[key]["summary_rank0.json"])
        r["fill_nodes_to_elements"]["event_calls"] = 3
        self.runs[key]["summary_rank0.json"] = json.dumps(r).encode()
        self.assert_rejected("event calls executed in JOSS mode")

    def test_distribute_must_match_explicit_partition(self):
        key = "fixtures/strict/coh_np2_distribute/rep1"
        s = json.loads(self.runs[key]["summary.json"])
        s["nb_global_nodes_end"] += 1
        self.runs[key]["summary.json"] = json.dumps(s).encode()
        self.assert_rejected("explicit partition not equivalent")

    # -- provenance ----------------------------------------------------------
    def test_missing_build_record_is_rejected(self):
        del self.runs["fixtures/strict/coh_np2/rep3"]["build-record.json"]
        self.assert_rejected("build-record.json missing")

    def test_mixed_builds_are_rejected(self):
        other = synth.build_record("strict", tag="-rebuilt")
        key = "fixtures/strict/ela_np1/rep2"
        files = self.runs[key]
        files["build-record.json"] = json.dumps(other).encode()
        meta = json.loads(files["run.json"])
        d = mf.build_digest(other)
        meta.update(build_record_sha256=d, runtime_check={"build_record_sha256": d},
                    binary_sha256=other["binary"]["sha256"])
        files["run.json"] = json.dumps(meta).encode()
        self.assert_rejected("2 different builds")

    def test_record_digest_must_match(self):
        key = "fixtures/default/ela_np1/rep1"
        meta = json.loads(self.runs[key]["run.json"])
        meta["build_record_sha256"] = "0" * 64
        self.runs[key]["run.json"] = json.dumps(meta).encode()
        self.assert_rejected("build_record_sha256 does not match")

    def test_oversubscribed_run_is_rejected(self):
        key = "fixtures/strict/coh_np2/rep1"
        meta = json.loads(self.runs[key]["run.json"])
        meta["oversubscribed"] = True
        self.runs[key]["run.json"] = json.dumps(meta).encode()
        self.assert_rejected("oversubscribed")

    def test_needs_three_repetitions(self):
        with self.assertRaisesRegex(mf.FixtureError, "at least 3"):
            mf.generate(self.runs, 1500, 2, "perf_cohesive_test", False)


class Comparisons(unittest.TestCase):
    def test_unequal_histories_are_an_error(self):
        a = [[0, 0, 1, 0, 0, 1], [1, 0, 1, 0, 0, 1]]
        with self.assertRaisesRegex(mf.FixtureError, "length"):
            mf.compare_energies(a, a[:1], {0, 1})
        b = [[0, 0, 1, 0, 0, 1], [2, 0, 1, 0, 0, 1]]
        with self.assertRaises(mf.FixtureError):
            mf.compare_energies(a, b, {0, 1})

    def test_insertion_diff_reports_length(self):
        a = b"step, nb_inserted, nb_cohesive_total\n5, 1, 1\n7, 1, 2\n"
        b = b"step, nb_inserted, nb_cohesive_total\n5, 1, 1\n"
        d = mf.insertion_diff(a, b)
        self.assertFalse(d["identical"])
        self.assertEqual(d["first_difference (step a / step b)"], "length differs")


class CommittedFixtures(unittest.TestCase):
    def test_committed_qualification_is_reproducible(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = synth.sandbox_fixtures(tmp)
            saved = mf.FIXTURES
            mf.FIXTURES = fx
            try:
                q = mf.qualify_committed()
            finally:
                mf.FIXTURES = saved
            self.assertEqual(q["cases"]["coh_np2"]["files"]["energies.csv"], "diagnostic")
            self.assertEqual(q["cases"]["coh_np2_global"]["files"]["partition.csv"], "qualified")
            self.assertEqual(q["cases"]["coh_np2_global"]["files"]["insertions.csv"], "qualified")
            committed = os.path.join(synth.COMMITTED, "qualification.json")
            if os.path.exists(committed):
                self.assertEqual(mf.perfcheck.read_json(committed), q)

    def test_compare_dirs_is_diagnostic(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = synth.sandbox_fixtures(tmp)
            saved = mf.FIXTURES
            mf.FIXTURES = fx
            try:
                same = mf.compare_dirs("perf_cohesive", "perf_cohesive")
                p = os.path.join(fx, "perf_cohesive", "coh_np1", "energies.csv")
                data = mf.perfcheck.read_bytes(p)
                import shutil
                shutil.copytree(os.path.join(fx, "perf_cohesive"), os.path.join(fx, "perf_cohesive_b"))
                lines = data.decode().split("\n")
                fields = lines[1201].split(", ")  # step 1200
                fields[2] = repr(float(fields[2]) * (1 + 1e-12))
                lines[1201] = ", ".join(fields)
                with open(os.path.join(fx, "perf_cohesive_b", "coh_np1", "energies.csv"), "w") as f:
                    f.write("\n".join(lines))
                diff = mf.compare_dirs("perf_cohesive", "perf_cohesive_b")
            finally:
                mf.FIXTURES = saved
            self.assertTrue(all(all(c["identical_files"].values()) for c in same["cases"].values()))
            e = diff["cases"]["coh_np1"]
            self.assertFalse(e["identical_files"]["energies.csv"])
            self.assertEqual(e["energies"]["max_rel_up_to_step"]["1100"]["ep"], 0.0)
            self.assertGreater(e["energies"]["max_rel_up_to_step"]["1200"]["ep"], 0.0)

    def test_committed_fixtures_pass_the_offline_check(self):
        if os.path.exists(os.path.join(synth.COMMITTED, "qualification.json")):
            mf.check_dir(synth.COMMITTED)


if __name__ == "__main__":
    unittest.main()
