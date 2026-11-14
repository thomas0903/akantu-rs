# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code): tests of
# collect.py and analyze.py on a
# synthetic campaign. Run: benchmarks/cohesive-perf/README.md.

import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import synth  # noqa: E402
from synth import mf  # noqa: E402
import collect  # noqa: E402
import analyze  # noqa: E402

read_bytes = mf.perfcheck.read_bytes
CAMPAIGN = "unit-campaign"
MESH = "/perf/meshes/cube-h0.12.msh"


def timing_run(ranks, rep, rec, started="2026-11-11T12:00:00Z", steps=1500, **over):
    case = "coh_np1" if ranks == 1 else "coh_np2"
    src = os.path.join(synth.COMMITTED, case)
    files = {"summary.json": synth.denormalise_summary(
        read_bytes(os.path.join(src, "summary.json")), MESH),
        "insertions.csv": read_bytes(os.path.join(src, "insertions.csv"))}
    for r in range(ranks):
        files[f"summary_rank{r}.json"] = synth.denormalise_rank(
            read_bytes(os.path.join(src, f"summary_rank{r}.json")))
    files["chrono.csv"] = synth.chrono_csv(ranks, steps, True)
    files["err.txt"] = files["chrono.csv"]
    files["out.txt"] = b"ok\n"
    files["build-record.json"] = json.dumps(rec).encode()
    files["run.json"] = synth.run_json("default", ranks, "cohesive", MESH, "meshsha", steps, [],
                                       "upstream", rec, b"input", started=started, **over)
    return files


def run_dir(ranks, rep):
    return f"{CAMPAIGN}/default/cohesive/h0.12/np{ranks}/rep{rep}"


def plan(ranks_list, reps, created="2026-11-11T11:00:00Z", steps=1500):
    runs = [{"run_dir": run_dir(n, k), "config": "default", "material": "cohesive", "ranks": n,
             "mesh": MESH, "max_steps": steps, "aka_seed": "1",
             "fill_nodes_to_elements": "upstream", "extra_parameters": []}
            for k in range(1, reps + 1) for n in ranks_list]
    return json.dumps({"format": "cohesive-perf plan v1", "campaign": CAMPAIGN,
                       "created_utc": created, "expected_runs": runs}).encode()


class Collect(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.runs_root = os.path.join(self.tmp.name, "runs")
        self.results = os.path.join(self.tmp.name, "results")
        os.makedirs(self.results)
        self.saved = collect.RESULTS
        collect.RESULTS = self.results
        self.rec = synth.build_record("default")
        self.files = {f"{CAMPAIGN}/_plans/p1.json": plan([1, 2], 2)}
        for n in (1, 2):
            for k in (1, 2):
                for name, data in timing_run(n, k, self.rec).items():
                    self.files[f"{run_dir(n, k)}/{name}"] = data

    def tearDown(self):
        collect.RESULTS = self.saved
        self.tmp.cleanup()

    def run_collect(self, *args):
        for path, data in self.files.items():
            p = os.path.join(self.runs_root, path)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "wb") as f:
                f.write(data)
        argv = sys.argv
        sys.argv = ["collect.py", "--runs-dir", self.runs_root, *args, CAMPAIGN]
        out, err = io.StringIO(), io.StringIO()
        try:
            with redirect_stdout(out), redirect_stderr(err):
                collect.main()
            code = 0
        except SystemExit as e:
            code = e.code
        finally:
            sys.argv = argv
        report = os.path.join(self.results, CAMPAIGN, "collection.json")
        return code, (mf.perfcheck.read_json(report) if os.path.exists(report) else None), err.getvalue()

    def table(self):
        return os.path.join(self.results, CAMPAIGN, "timmings_cohesive_default_h0.12.csv")

    def test_complete_campaign(self):
        code, report, _ = self.run_collect()
        self.assertEqual(code, 0)
        self.assertTrue(report["complete"])
        self.assertEqual(report["counts"]["valid"], 4)
        self.assertEqual(len(mf.perfcheck.read_text(self.table()).splitlines()), 1 + 4)
        prov = mf.perfcheck.read_json(os.path.join(self.results, CAMPAIGN, "provenance.json"))
        self.assertEqual(list(prov["build_records"]), [mf.build_digest(self.rec)])

    def test_stale_tables_are_removed(self):
        stale = os.path.join(self.results, CAMPAIGN, "timmings_elastic_default_h0.12.csv")
        os.makedirs(os.path.dirname(stale))
        with open(stale, "w") as f:
            f.write("OLD VALID TIMING\n")
        self.run_collect()
        self.assertFalse(os.path.exists(stale))
        self.assertEqual([n for n in os.listdir(self.results) if n.startswith(".")], [])

    def test_stale_table_removed_when_all_runs_invalid(self):
        # previous table, current run invalid -> no old CSV survives
        self.run_collect()
        self.assertTrue(os.path.exists(self.table()))
        for n in (1, 2):
            for k in (1, 2):
                self.files[f"{run_dir(n, k)}/err.txt"] = synth.RETRY_WARNING.encode()
        code, report, err = self.run_collect()
        self.assertEqual(code, 1)
        self.assertFalse(report["complete"])
        self.assertFalse(os.path.exists(self.table()))
        self.assertIn("FAILURE", err)

    def test_retry_run_is_invalid_and_listed_for_rerun(self):
        self.files[f"{run_dir(2, 1)}/err.txt"] = (synth.RETRY_WARNING.encode()
                                                  + self.files[f"{run_dir(2, 1)}/err.txt"])
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertEqual([r["run_dir"] for r in report["rerun"]], [run_dir(2, 1)])
        self.assertEqual(len(mf.perfcheck.read_text(self.table()).splitlines()), 1 + 3)
        code, report, _ = self.run_collect("--allow-incomplete")
        self.assertEqual(code, 0)
        self.assertFalse(report["complete"])

    def test_invalid_attempt_is_kept_and_costed(self):
        for name, data in timing_run(2, 1, self.rec).items():
            self.files[f"{run_dir(2, 1)}.invalid-1/{name}"] = data
        self.files[f"{run_dir(2, 1)}.invalid-1/err.txt"] = synth.RETRY_WARNING.encode()
        code, report, _ = self.run_collect()
        self.assertEqual(code, 0)
        self.assertEqual(report["counts"]["invalid attempt"], 1)
        self.assertEqual(report["rejected_attempts"], 1)
        self.assertEqual(report["rejected_attempts_wall_s"], 12.5)

    def test_missing_planned_run_fails(self):
        for k in list(self.files):
            if k.startswith(run_dir(1, 2) + "/"):
                del self.files[k]
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertIn(f"{run_dir(1, 2)}: planned run missing", report["failures"])

    def test_run_older_than_plan_is_stale(self):
        self.files[f"{CAMPAIGN}/_plans/p2.json"] = plan([1, 2], 2, created="2026-11-11T13:00:00Z")
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("stale" in f for f in report["failures"]))

    def test_run_not_matching_plan_fails(self):
        self.files[f"{CAMPAIGN}/_plans/p2.json"] = plan([1], 1, created="2026-11-11T11:30:00Z",
                                                        steps=500)
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("does not match its plan (max_steps)" in f for f in report["failures"]))

    def test_incompatible_identities_are_not_combined(self):
        del self.files[f"{CAMPAIGN}/_plans/p1.json"]
        # valid on its own, but another seed: must not share the table
        for name, data in timing_run(1, 2, self.rec, aka_seed="2").items():
            self.files[f"{run_dir(1, 2)}/{name}"] = data
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("incompatible measurement identities" in f for f in report["failures"]))
        self.assertFalse(os.path.exists(self.table()))

    def test_chrono_psize_mismatch_is_invalid(self):
        self.files[f"{run_dir(2, 2)}/chrono.csv"] = synth.chrono_csv(1, 1500, True)
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("psize" in f for f in report["failures"]))

    def test_nonfinite_chrono_is_invalid(self):
        c = synth.chrono_csv(1, 1500, True).decode().splitlines()
        f = c[1].split(", ")
        f[1] = "nan"
        self.files[f"{run_dir(1, 1)}/chrono.csv"] = ("\n".join([c[0], ", ".join(f)]) + "\n").encode()
        self.files[f"{run_dir(1, 1)}/err.txt"] = b""
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("not a finite" in f for f in report["failures"]))

    def test_missing_rank_summary_is_invalid(self):
        del self.files[f"{run_dir(2, 1)}/summary_rank1.json"]
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("summary_rank files" in f for f in report["failures"]))

    def test_build_record_required(self):
        del self.files[f"{run_dir(1, 1)}/build-record.json"]
        code, report, _ = self.run_collect()
        self.assertEqual(code, 1)
        self.assertTrue(any("no build record" in f for f in report["failures"]))

    def test_oversubscribed_runs_stay_out_of_tables(self):
        for name, data in timing_run(2, 2, self.rec, oversubscribed=True).items():
            self.files[f"{run_dir(2, 2)}/{name}"] = data
        code, report, err = self.run_collect()
        self.assertEqual(code, 0)
        self.assertEqual(report["counts"]["oversubscribed"], 1)
        self.assertEqual(len(mf.perfcheck.read_text(self.table()).splitlines()), 1 + 3)
        self.assertIn("oversubscribed", err)

    def test_analyze_refuses_incomplete_campaign(self):
        self.files[f"{run_dir(2, 1)}/err.txt"] = synth.RETRY_WARNING.encode()
        self.run_collect()
        saved = analyze.PERF_DIR
        analyze.PERF_DIR = os.path.dirname(self.results)
        try:
            with self.assertRaises(SystemExit):
                analyze.load_campaign(CAMPAIGN)
        finally:
            analyze.PERF_DIR = saved


class CampaignNames(unittest.TestCase):
    def test_traversal_and_metacharacters_are_rejected(self):
        for bad in ("../../../v49-outside-results", "/tmp/v49-outside-results", "a b", "x'y",
                    "a/b", "..", ".hidden", "a..b", "$(id)", "", "x" * 65):
            with self.assertRaises(ValueError, msg=bad):
                collect.perfcheck.check_campaign(bad)
        for good in ("baseline", "full-schedule", "review-v49", "x86_64.baseline_2"):
            self.assertEqual(collect.perfcheck.check_campaign(good), good)

    def test_reserved_campaign(self):
        argv = sys.argv
        sys.argv = ["collect.py", "published-jed"]
        try:
            with self.assertRaises(SystemExit):
                collect.main()
        finally:
            sys.argv = argv

    def test_confined(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = os.path.join(tmp, "results")
            os.makedirs(root)
            os.symlink(tmp, os.path.join(root, "escape"))
            self.assertTrue(collect.perfcheck.confined(root, os.path.join(root, "camp")))
            self.assertFalse(collect.perfcheck.confined(root, os.path.join(root, "escape", "x")))
            self.assertFalse(collect.perfcheck.confined(root, os.path.join(root, "..", "x")))
            self.assertFalse(collect.perfcheck.confined(root, root))


if __name__ == "__main__":
    unittest.main()
