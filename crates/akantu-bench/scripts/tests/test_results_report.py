# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the results tool
# (crates/akantu-bench/scripts/results_report.py): the pair measures of
# D-108 on synthetic runs (thresholds, the first count
# difference, the one-step growth above the floor, the envelope ratio, the
# injection window), the Chrono phase sums, and the regeneration of the
# committed output docs/benchmarks/results-2026-11-13.txt from the committed
# run files.
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
import results_report as rr  # noqa: E402

ROOT = HERE.parents[3]
SCRIPT = HERE.parent / "results_report.py"


def write_run(d, energies, insertions, time_step=1e-8, **extra):
    """energies: list of (ed, ep, ek, ew, et) per step from 0; insertions: [(step, n)]."""
    d.mkdir(parents=True)
    lines = ["step, ed, ep, ek, ew, et"]
    lines += [f"{s}, " + ", ".join(repr(x) for x in row) for s, row in enumerate(energies)]
    (d / "energies.csv").write_text("\n".join(lines) + "\n")
    tot, ins = 0, ["step, nb_inserted, nb_cohesive_total"]
    for s, k in insertions:
        tot += k
        ins.append(f"{s}, {k}, {tot}")
    (d / "insertions.csv").write_text("\n".join(ins) + "\n")
    summ = {"time_step": time_step, **rr.SETTINGS, **extra}
    (d / "summary.json").write_text(json.dumps(summ))


def base(steps):
    """Energies of size ~1e6 (ep the largest), growing slowly."""
    return [(1.0, 1e6 + s, 10.0 * s, 20.0 * s, 1e6 - 9.0 * s) for s in range(steps)]


class PairMeasures(unittest.TestCase):
    def make(self, tmp, db, ins_a, ins_b, steps=200):
        a, b = base(steps), base(steps)
        b = [tuple(x + db(s, k) for k, x in enumerate(row)) for s, row in enumerate(b)]
        write_run(tmp / "a", a, ins_a)
        write_run(tmp / "b", b, ins_b, time_step=1e-8 * (1 + 4e-13))
        return rr.Pair(tmp / "a", tmp / "b")

    def test_thresholds_counts_and_final(self):
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            # B's potential energy leaves A by 1e-11 of L from step 50, by 1e-9 from step 120;
            # the counts part at step 150 (one element one step later)
            def db(s, k):
                if k != 1:
                    return 0.0
                return 1e6 * (1e-9 if s >= 120 else 1e-11 if s >= 50 else 0.0)
            p = self.make(t, db, [(10, 3), (150, 2), (151, 1)], [(10, 3), (150, 1), (151, 2)])
            self.assertEqual(p.first_count, 150)
            self.assertEqual(p.last_equal, 149)
            self.assertEqual(p.first_above(1e-12), 50)
            self.assertEqual(p.first_above(1e-10), 120)
            self.assertIsNone(p.first_above(1e-10, "ew"))
            v, s, k = p.largest(0, p.last_equal)
            self.assertEqual((s, k), (120, "ep"))
            self.assertAlmostEqual(v / (1e-9 * 1e6 / (1e6 + 120)), 1.0, delta=1e-6)
            self.assertEqual(p.ca[151], p.cb[151])
            self.assertAlmostEqual(rr.time_step_rel(t / "a", t / "b") / 4e-13, 1.0, delta=1e-3)
            self.assertEqual(rr.ep0_rel(t / "a", t / "b"), 0.0)
            fin = p.final()
            self.assertEqual(fin["ek"], 0.0)
            self.assertGreater(fin["ep"], 0.0)
            text = "\n".join(rr.pair_lines(p, "x")[0])
            self.assertIn("1 (c) cumulative counts equal through step 149: 3 elements in 1 insertion steps", text)
            self.assertIn("[as required]", text)

    def test_growth_floor_and_envelope(self):
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            # the external work: rounding noise below the floor, then one step from 1e-12 to
            # 5e-10 of L (factor 500) at step 100, then smooth
            def db(s, k):
                if k != 3:
                    return 0.0
                L = 1e6 + s
                if s < 80:
                    return 1e-15 * L * (1 if s % 2 else -1)
                if s < 100:
                    return 1e-12 * L
                return 5e-10 * L
            p = self.make(t, db, [(10, 1)], [(10, 1)])
            self.assertIsNone(p.first_count)
            g, s = p.d108_growth(p.last_equal)
            self.assertEqual(s, 100)
            self.assertAlmostEqual(g / 500.0, 1.0, delta=1e-6)
            j, js = p.j_max("ew", p.last_equal)
            self.assertEqual(js, 100)
            self.assertGreater(j, 400)
            # below the floor a ratio is not counted: the steps 1 ... 79 alternate in sign
            g79, _ = p.d108_growth(79)
            self.assertEqual(g79, 0.0)
            self.assertEqual(p.zero_crossing_before("ew", 79), 79)

    def test_injection_window(self):
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            # a quiet difference of 1e-12 of L up to step 1,300, then 1e-6 of L: an injected
            # signature of ~1e-5 J (1e-11 of L) stands out before step 1,300 only
            def db(s, k):
                if k != 3:
                    return 0.0
                return (1e-12 if s < 1300 else 1e-6) * (1e6 + s)
            p = self.make(t, db, [(10, 1)], [(10, 1)], steps=1500)
            tpl = [1e-5 * (k + 1) for k in range(19)]
            w = p.inject(tpl, lo=700)
            self.assertEqual(w["onsets"], (700, 1500 - 1 - 19))
            # an onset at s0 is caught while its first step lies before 1,300 (the jump of the
            # background itself at 1,300 also fires J, so the window ends at the first onset
            # whose 19 steps all lie beyond the background's step)
            self.assertGreaterEqual(w["j1"], 1299)
            self.assertLess(w["j1"], 1300 + 50)
            # D-108's growth > 100 never fires: the first injected step (1e-11 of L) grows the
            # 1e-12 background by a factor 11 only (window = the first onset - 1: none detected)
            self.assertEqual(w["d108"], 699)


class ChronoAndTables(unittest.TestCase):
    def test_chrono_sums(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            head = ['"psize"']
            vals = ["8"]
            for ph, v in (("before_step", 1.0), ("boundary_conditions", 0.5), ("check_cohesive_stress", 10.0),
                          ("dist_mesh", 2.0), ("energies", 3.0), ("init_full", 4.0), ("initial_conditons", 0.25),
                          ("read_mesh", 0.75), ("solve_step", 20.0), ("static_solve", 8.0)):
                head += [f'"{ph}"', f'"{ph} nb_rep"']
                vals += [repr(v), "1"]
            (d / "chrono.csv").write_text(", ".join(head) + "\n" + ", ".join(vals) + "\n")
            (d / "wall.txt").write_text("50.5\n")
            (d / "summary.json").write_text(json.dumps({"psize": 8, "max_steps": 100,
                                                        "nb_regular_elements_dim3": 800}))
            r = rr.run_info(d)
            self.assertEqual(r["init"], 16.0)
            self.assertEqual(r["loop"], 33.5)
            self.assertEqual(r["wall"], 50.5)
            self.assertEqual(r["rep"]["solve_step"], 1)

    def test_table_and_format(self):
        self.assertEqual(rr.g(3.2240e-14, 2), "3.2e-14")
        self.assertEqual(rr.g(0.0), "0")
        self.assertEqual(rr.table(("a", "b"), [(1, 2)]), "| a | b |\n|---|---|\n| 1 | 2 |")


class CommittedOutput(unittest.TestCase):
    def test_committed_output_is_current(self):
        """docs/benchmarks/results-2026-11-13.txt is the script's output on the committed files."""
        committed = (ROOT / "docs/benchmarks/results-2026-11-13.txt").read_text()
        r = subprocess.run([sys.executable, "-I", str(SCRIPT), "--root", str(ROOT)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, committed)

    def test_unknown_section_refused(self):
        r = subprocess.run([sys.executable, "-I", str(SCRIPT), "nonsense"], capture_output=True, text=True)
        self.assertEqual(r.returncode, 2)
        self.assertIn("unknown section", r.stderr)


if __name__ == "__main__":
    unittest.main()
