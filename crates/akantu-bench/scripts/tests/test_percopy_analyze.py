# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the verdict of
# crates/akantu-bench/scripts/percopy_analyze.py:
# a run must have made its checks (count against the
# summary and the due steps), with no foreign write and no move from a
# written node, before its placement's expectation is read. The run
# directories are written by the test.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import percopy_analyze  # noqa: E402

HEADER = ("step, increments, ranks, fmax, loaded_nodes, record_nodes, new_copies, doubled_loaded, "
          "doubled_loaded_copies, moved_contributions_sum, moved_from_written_nodes_sum, foreign_writes_sum, "
          "reset_nodes_sum, forgotten_entries_sum, ranks_with_store, applications_min, applications_max, "
          "differing_owned, differing_copies, max_rel_diff, max_rel_diff_owned, worst_rank, worst_gid, "
          "worst_flag, worst_f_x, worst_f_y, worst_f_z, worst_reapplied_x, worst_reapplied_y, worst_reapplied_z, "
          "total_f_x, total_f_y, total_f_z, total_reapplied_x, total_reapplied_y, total_reapplied_z, "
          "total_rel_diff, row_facet_mismatches, verdict")


def row(step, doubled=0, moved=0, rel=0.0, foreign=0, written=0, new=0, mismatches=0, verdict="pass",
        old_format=False):
    f = [step, 0, 1, 1.0, 10, 10, new, doubled, doubled, moved, written, foreign, 0, 0, 1, 6, 6, 0, 0, rel, rel,
         -1, -1, "-", 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0.0]
    if not old_format:
        f.append(mismatches)
    f.append(verdict)
    return ", ".join(str(x) for x in f)


def write_run(root, name, rows, placement="follows_elements", max_steps=3, every="1", summary_checks=None,
              old_format=False):
    d = root / name
    d.mkdir(parents=True)
    header = HEADER if not old_format else HEADER.replace(", row_facet_mismatches", "")
    (d / "load_check.csv").write_text("\n".join([header] + rows) + "\n")
    s = {"rust_doubled_node_load": placement, "psize": 1, "max_steps": max_steps,
         "rust_load_check_every": every,
         "rust_load_check": {"checks": len(rows) if summary_checks is None else summary_checks}}
    (d / "summary.json").write_text(json.dumps(s))
    return d


def analyse(root):
    out = io.StringIO()
    with redirect_stdout(out):
        code = percopy_analyze.main([str(root)])
    return code, out.getvalue()


GOOD = [row(0), row(1, new=2, doubled=1, moved=3), row(2, new=2, doubled=1, moved=3),
        row(3, new=4, doubled=2, moved=5)]


class Verdict(unittest.TestCase):
    def test_a_complete_corrected_run_passes(self):
        with tempfile.TemporaryDirectory() as t:
            write_run(Path(t), "h012-np1-follows", GOOD)
            code, out = analyse(Path(t))
            self.assertEqual(code, 0, out)
            self.assertIn("verdict: every checked run made its checks", out)

    def test_a_run_without_a_check_is_not_a_pass(self):
        with tempfile.TemporaryDirectory() as t:
            write_run(Path(t), "h012-np1-follows", [], every="0")
            code, out = analyse(Path(t))
            self.assertEqual(code, 1, out)
            self.assertIn("no check made", out)

    def test_missing_checks_are_found(self):
        with tempfile.TemporaryDirectory() as t:
            write_run(Path(t), "h012-np1-follows", GOOD[:3])
            code, out = analyse(Path(t))
            self.assertEqual(code, 1, out)
            self.assertIn("3 checks for 4 due steps", out)
        with tempfile.TemporaryDirectory() as t:
            write_run(Path(t), "h012-np1-follows", GOOD, summary_checks=5)
            code, out = analyse(Path(t))
            self.assertEqual(code, 1, out)
            self.assertIn("summary checks 5 != 4 rows", out)

    def test_foreign_writes_and_moves_from_written_nodes_fail(self):
        with tempfile.TemporaryDirectory() as t:
            rows = GOOD[:2] + [row(2, new=2, doubled=1, moved=3, foreign=1, written=2), GOOD[3]]
            write_run(Path(t), "h012-np1-follows", rows)
            code, out = analyse(Path(t))
            self.assertEqual(code, 1, out)
            self.assertIn("foreign writes (1)", out)
            self.assertIn("moves from written nodes (2)", out)

    def test_the_row_facet_count_is_read_from_the_first_copy_on(self):
        with tempfile.TemporaryDirectory() as t:
            rows = [row(0, mismatches=1839), row(1, new=2, doubled=1, moved=3), row(2, new=2, doubled=1, moved=3,
                                                                                    mismatches=2),
                    row(3, new=4, doubled=2, moved=5)]
            d = write_run(Path(t), "h012-np1-follows", rows)
            info = percopy_analyze.run_info(d)
            self.assertEqual(info["row_mm"], 2)
            self.assertEqual(percopy_analyze.run_problems(info), [])

    def test_files_written_before_the_row_facet_count_are_read(self):
        with tempfile.TemporaryDirectory() as t:
            rows = [row(s, new=2 * s, doubled=s, moved=s, old_format=True) for s in range(4)]
            d = write_run(Path(t), "h012-np1-follows", rows, old_format=True)
            info = percopy_analyze.run_info(d)
            self.assertIsNone(info["row_mm"])
            code, out = analyse(Path(t))
            self.assertEqual(code, 0, out)

    def test_upstream_must_fail_from_its_first_doubled_loaded_node(self):
        with tempfile.TemporaryDirectory() as t:
            rows = [row(0), row(1, new=2, doubled=1, rel=0.3, verdict="FAIL"),
                    row(2, new=2, doubled=1, rel=0.3, verdict="FAIL"), row(3, new=2, doubled=1, rel=0.3,
                                                                          verdict="FAIL")]
            write_run(Path(t), "h012-np1-upstream", rows, placement="upstream")
            code, out = analyse(Path(t))
            self.assertEqual(code, 0, out)


if __name__ == "__main__":
    unittest.main()
