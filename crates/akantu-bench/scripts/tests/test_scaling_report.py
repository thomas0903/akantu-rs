# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the tools
# (crates/akantu-bench/scripts/scaling_analyze.py, c1_report.py): the
# series' medians, paired ratios, weak efficiencies and strong speed-ups on
# synthetic campaigns, the exclusion of failed runs, and the refusal of
# c1_report.py's pack step when identifying data survive the redaction.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import c1_report  # noqa: E402

ANALYZE = HERE.parent / "scaling_analyze.py"


def write_campaign(out, series, h, np_, rep, tets, runs):
    """runs: case -> (solve_s_total, check_s_total, static_s, outcome, coh_el)."""
    camp = out / f"{series}-h{h}-np{np_}-r{rep}"
    camp.mkdir(parents=True)
    (camp / "provenance.txt").write_text(
        f"campaign={camp.name} h={h} steps=100 ranks={np_} reps=1 cases=cpp rust extra=\n"
        "date=2026-11-13T00:00:00Z revision=abc dirty=0 partitions=x\n"
        "host_cpu=Test CPU nproc=8 kernel=6.8\nmem_total_kib=1000\n"
        f"mesh=/perf/meshes/cube-h{h}.msh sha256={'ab' * 32} elements={tets + 10}\n"
        "cpp_image=akantu-oracle:dev sha256:11 config=strict fnte=joss memory=14g cpus=unlimited\n"
        f"rust_image=akantu-rs-mumps:dev sha256:22 rust_bin=/x sha256={'cd' * 32} volume=v memory=14g cpus=8\n")
    summ = {"tets": tets, "runs": [], "medians": {}}
    for case, (solve, check, static, outcome, coh) in runs.items():
        rd = camp / case / f"np{np_}" / "rep1"
        rd.mkdir(parents=True)
        (rd / "status.txt").write_text(f'case="{case}"\noutcome="{outcome}"\n')
        summ["runs"].append({
            "case": case, "np": np_, "rep": "rep1", "wall_s": 10.0 + solve + check + static,
            "chrono": {"solve_step": solve, "solve_step nb_rep": 100.0,
                       "check_cohesive_stress": check, "check_cohesive_stress nb_rep": 100.0,
                       "static_solve": static, "static_solve nb_rep": 1.0,
                       "init_full": 1.0, "init_full nb_rep": 1.0},
            "mem": {"run_rank_peak_max": 1024 * 100, "run_total_rss_peak": 1024 * 100 * np_},
            "summary": {"nb_cohesive_elements": coh, "max_steps": 100},
        })
    (camp / "summary.json").write_text(json.dumps(summ))


class ScalingAnalyze(unittest.TestCase):
    def run_analyze(self, out, *args):
        r = subprocess.run([sys.executable, "-I", str(ANALYZE), str(out), "s", *args],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        return json.loads((out / "s-summary.json").read_text())

    def test_weak_strong_and_pairs(self):
        with tempfile.TemporaryDirectory() as t:
            out = Path(t)
            # weak study: 1000 tets on 1 rank, 2000 on 2 ranks; strong: h=0.5 on 1 and 2 ranks
            for rep, f in ((1, 1.0), (2, 1.2), (3, 0.9)):
                write_campaign(out, "s", "1", 1, rep, 1000, {"cpp": (2.0 * f, 6.0 * f, 1.0, "ok", 0),
                                                              "rust": (1.0 * f, 3.0 * f, 1.5, "ok", 0)})
                write_campaign(out, "s", "0.5", 2, rep, 2000, {"cpp": (2.5 * f, 7.5 * f, 2.0, "ok", 0),
                                                                "rust": (1.25 * f, 3.75 * f, 3.0, "ok", 0)})
                write_campaign(out, "s", "0.5", 1, rep, 2000, {"cpp": (4.0 * f, 12.0 * f, 4.0, "ok", 0),
                                                                "rust": (2.0 * f, 6.0 * f, 6.0, "ok", 0)})
            js = self.run_analyze(out, "--weak", "W=1:1,0.5:2", "--strong", "0.5")
            pts = {(p["case"], p["h"], p["np"]): p for p in js["points"]}
            # loop per step (ms) = (solve + check) / 100 steps; median of the three repetitions
            self.assertAlmostEqual(pts[("rust", "1", 1)]["loop_ms"]["median"], 40.0)
            self.assertAlmostEqual(pts[("cpp", "1", 1)]["loop_ms"]["median"], 80.0)
            # core-µs/el/step = loop × np / tets
            self.assertAlmostEqual(pts[("rust", "0.5", 2)]["core_us"]["median"], 50.0 * 2 / 2000 * 1e3)
            # paired Rust / C++ = 0.5 in every repetition
            pv = js["rust_over_cpp"]["1:1:loop_ms"]["paired"]
            self.assertEqual(len(pv), 3)
            for v in pv:
                self.assertAlmostEqual(v, 0.5)
            # weak efficiency: core-µs at the base (40 µs·1/1000) over the point (50·2/2000) = 0.8
            ew = js["studies"]["weak"]["W"]["rust:0.5:2"]
            self.assertAlmostEqual(ew["ew_medians"], 0.8)
            for v in ew["ew_per_rep"]:
                self.assertAlmostEqual(v, 0.8)
            # strong: 80 ms on 1 rank, 50 ms on 2 -> speed-up 1.6, efficiency 0.8
            st = js["studies"]["strong"]["0.5"]["rust:2"]
            self.assertAlmostEqual(st["loop_speedup"], 1.6)
            self.assertAlmostEqual(st["loop_eff"], 0.8)
            self.assertEqual(js["inserted"], [])
            self.assertEqual(js["rejected"], [])

    def test_failed_and_inserting_runs_are_reported(self):
        with tempfile.TemporaryDirectory() as t:
            out = Path(t)
            write_campaign(out, "s", "1", 1, 1, 1000, {"cpp": (2.0, 6.0, 1.0, "oom", 0),
                                                         "rust": (1.0, 3.0, 1.0, "ok", 7)})
            js = self.run_analyze(out)
            self.assertEqual(js["rejected"], [["s-h1-np1-r1/cpp", "oom"]])
            self.assertEqual(js["inserted"], [["s-h1-np1-r1", "rust", 7]])
            self.assertEqual({p["case"] for p in js["points"]}, {"rust"})

    def test_other_series_ignored(self):
        with tempfile.TemporaryDirectory() as t:
            out = Path(t)
            write_campaign(out, "s", "1", 1, 1, 1000, {"rust": (1.0, 3.0, 1.0, "ok", 0)})
            write_campaign(out, "other", "1", 1, 1, 1000, {"rust": (9.0, 9.0, 1.0, "ok", 0)})
            js = self.run_analyze(out)
            self.assertEqual(len(js["runs"]), 1)


class C1ReportRedaction(unittest.TestCase):
    def test_home_directory_redacted(self):
        self.assertEqual(c1_report.redact("dir=/home/someone/c1-results x"), "dir=/home/<user>/c1-results x")
        self.assertEqual(c1_report.redact("/home/<user>/a"), "/home/<user>/a")

    def test_forbidden_patterns_caught(self):
        # built at run time, so that this file holds no literal identifier
        # (the publication scan looks for such strings)
        hexs = "0123456789abcdef0"
        samples = {
            "ip": "host " + ".".join(["10", "1", "2", "3"]) + " up",
            "instance": "id i-" + hexs,
            "account": "account 123456789012",
            "host": "ip-10-0-0-1.ec2.internal",
            "arn": "arn:aws:iam::x",
            "sg": "group sg-" + hexs,
            "home": "/home/someone/x",
            "users": "/Users/someone/x",
            "ubuntu": "user ubuntu",
        }
        for name, text in samples.items():
            with tempfile.TemporaryDirectory() as t:
                (Path(t) / "f.txt").write_text(text)
                self.assertTrue(c1_report.check_clean(Path(t)), name)

    def test_numbers_and_digests_pass(self):
        text = ("memory_max 515396075520 E_k 0.001415124892 revision=8ec21eaf9a280484 "
                "kernel=7.0.0-1014-aws sha256 5ad845c731ea461d96c503d05f1278419de05840d9c69686297413bb0d86a420 "
                "/home/<user>/c1")
        with tempfile.TemporaryDirectory() as t:
            (Path(t) / "f.txt").write_text(text)
            self.assertEqual(c1_report.check_clean(Path(t)), [])


if __name__ == "__main__":
    unittest.main()
