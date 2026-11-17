# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the binding check of the single-node
# final campaign (scripts/perf/final/bindings.py `check`) on synthetic
# binding records. /proc and the processes of a run are not used.
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
REPO = HERE.parent.parent.parent.parent
sys.path.insert(0, str(REPO / "scripts" / "perf" / "final"))
import bindings  # noqa: E402

# 4 ranks x 1 thread on cores 0-3 of NUMA node 0, 4-7 on node 1
CORES = "0,1,2,3,4,5,6,7"
NUMA = {str(c): (0 if c < 4 else 1) for c in range(8)}


def check(ranks, np_=4, threads=1, cores=CORES):
    """bindings.check on a record with these processes [(rank or None, cpus)]: (exit, output)."""
    with tempfile.TemporaryDirectory() as tmp:
        f = Path(tmp) / "bindings.json"
        f.write_text(json.dumps({"utc": "x", "comm": "cohesive_extrin", "numa": NUMA,
                                 "ranks": [{"pid": 100 + i, "rank": r, "cpus": c}
                                           for i, (r, c) in enumerate(ranks)]}))
        out = io.StringIO()
        with redirect_stdout(out):
            rc = bindings.check(str(f), np_, threads, cores)
        return rc, out.getvalue()


class Check(unittest.TestCase):
    def test_planned_binding_passes(self):
        rc, out = check([(0, "0"), (1, "1"), (2, "2"), (3, "3")])
        self.assertEqual(rc, 0, out)
        rc, out = check([(0, "0-1"), (1, "2-3")], np_=2, threads=2)
        self.assertEqual(rc, 0, out)

    def test_unknown_ranks_on_the_free_blocks_pass(self):
        # the original failure: processes exiting at the snapshot (environment no longer
        # readable) sit on the blocks of the ranks no other process reports
        rc, out = check([(0, "0"), (None, "2"), (None, "1"), (3, "3")])
        self.assertEqual(rc, 0, out)
        self.assertIn("matched by its cores", out)

    def test_shared_core_fails(self):
        rc, out = check([(0, "0"), (1, "0"), (2, "2"), (3, "3")])
        self.assertEqual(rc, 1)
        self.assertIn("rank 1: cores [0], planned [1]", out)
        rc, out = check([(0, "0"), (None, "0"), (2, "2"), (3, "3")])
        self.assertEqual(rc, 1)
        self.assertIn("unknown rank on cores [0]", out)

    def test_unbound_process_fails(self):
        rc, out = check([(0, "0-7"), (1, "1"), (2, "2"), (3, "3")])
        self.assertEqual(rc, 1)
        self.assertIn("rank 0: cores [0, 1, 2, 3, 4, 5, 6, 7], planned [0]", out)
        self.assertIn("NUMA nodes [0, 1]", out)

    def test_duplicate_rank_fails(self):
        # two processes report rank 0 on core 0 and rank 1 is missing: passed before
        rc, out = check([(0, "0"), (0, "0"), (2, "2"), (3, "3")])
        self.assertEqual(rc, 1)
        self.assertIn("rank 0 carried by 2 processes", out)
        self.assertIn("rank 1: no process on its planned cores [1]", out)

    def test_missing_rank_fails(self):
        rc, out = check([(0, "0"), (2, "2"), (3, "3")])
        self.assertEqual(rc, 1)
        self.assertIn("3 processes recorded, 4 expected", out)
        self.assertIn("rank 1: no process on its planned cores [1]", out)
        rc, out = check([(0, "0"), (None, "3"), (2, "2")], np_=4)
        self.assertEqual(rc, 1)
        self.assertIn("rank 1: no process on its planned cores [1]", out)

    def test_no_record(self):
        with redirect_stdout(io.StringIO()):
            self.assertEqual(bindings.check("/nonexistent/bindings.json", 4, 1, CORES), 1)


if __name__ == "__main__":
    unittest.main()
