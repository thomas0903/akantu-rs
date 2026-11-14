# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: tests of drop_nondeterministic.py.
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from helpers import PYTHON_DIR, REPO, manifest, msh2, write_case

import drop_nondeterministic as dn

CASE = "mesh_io/test_x/data/mixed"
BAD = "mesh_data/tag_2/_segment_2"


class DeterminismTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.src = self.root / "src"
        # _segment_2 elements with 2 and 3 tags: tag_2 is corrupted (UD-064).
        msh2(self.src / "test/test_x/data/mixed.msh", [(1, 2), (1, 3), (2, 2)])
        msh2(self.src / "test/test_x/data/uniform.msh", [(1, 2), (1, 2)])
        self.allow = {(CASE, BAD): {"case_id": CASE, "quantity": BAD, "defect": "UD-064",
                                    "reason": "test", "evidence": "test"}}

    def tearDown(self):
        self.tmp.cleanup()

    def runs(self, q1: dict, q2: dict | None = None, *, case=CASE, mesh="test/test_x/data/mixed.msh",
             in2: bytes = b"mesh"):
        r1, r2 = self.root / "run1", self.root / "run2"
        write_case(r1, case, q1, {"mesh": (b"mesh", f"upstream:{mesh}")})
        write_case(r2, case, q2 if q2 is not None else q1, {"mesh": (in2, f"upstream:{mesh}")})
        return r1, r2

    def test_missing_second_run_is_an_error(self):
        r1, _ = self.runs({"nodes": b"\0" * 8})
        with self.assertRaisesRegex(dn.DeterminismError, "second run"):
            dn.plan(r1, self.root / "nope", self.src, self.allow)

    def test_case_inventories_must_match(self):
        r1, r2 = self.runs({"nodes": b"\0" * 8})
        write_case(r1, "mesh_io/test_x/data/only1", {"nodes": b"\0" * 8})
        with self.assertRaisesRegex(dn.DeterminismError, "case only in run 1"):
            dn.plan(r1, r2, self.src, self.allow)
        write_case(r2, "mesh_io/test_x/data/only1", {"nodes": b"\0" * 8})
        write_case(r2, "mesh_io/test_x/data/only2", {"nodes": b"\0" * 8})
        with self.assertRaisesRegex(dn.DeterminismError, "case only in run 2"):
            dn.plan(r1, r2, self.src, self.allow)

    def test_quantity_inventory_and_inputs_must_match(self):
        r1, r2 = self.runs({"nodes": b"\0" * 8, BAD: b"\1" * 8}, {"nodes": b"\0" * 8})
        with self.assertRaisesRegex(dn.DeterminismError, "quantities differ"):
            dn.plan(r1, r2, self.src, self.allow)
        r1, r2 = self.runs({"nodes": b"\0" * 8, BAD: b"\1" * 8}, in2=b"other mesh")
        with self.assertRaisesRegex(dn.DeterminismError, "inputs differ"):
            dn.plan(r1, r2, self.src, self.allow)

    def test_unexpected_difference_stops_and_modifies_nothing(self):
        r1, r2 = self.runs({"nodes": b"\0" * 8, BAD: b"\1" * 8},
                           {"nodes": b"\2" * 8, BAD: b"\1" * 8})
        before = (r1 / CASE / "manifest.json").read_text()
        with self.assertRaisesRegex(dn.DeterminismError, "nodes: unexpected nondeterminism"):
            dn.plan(r1, r2, self.src, self.allow)
        rc = dn.main(["x", str(r1), str(r2), str(self.src), "--allow-list", self.write_allow()])
        self.assertEqual(rc, 1)
        self.assertEqual((r1 / CASE / "manifest.json").read_text(), before)
        self.assertTrue((r1 / CASE / "data/nodes.npy").is_file())

    def test_allow_listed_but_unexplained_difference_is_an_error(self):
        # Allow-listed name, but in a mesh whose tag counts are uniform.
        uni = "mesh_io/test_x/data/uniform"
        allow = {(uni, BAD): dict(self.allow[(CASE, BAD)], case_id=uni)}
        r1, r2 = self.runs({BAD: b"\1" * 8}, {BAD: b"\3" * 8}, case=uni,
                           mesh="test/test_x/data/uniform.msh")
        with self.assertRaisesRegex(dn.DeterminismError, "does not explain it"):
            dn.plan(r1, r2, self.src, allow)

    def test_stale_allow_list_entry_is_an_error(self):
        uni = "mesh_io/test_x/data/uniform"
        allow = {(uni, BAD): dict(self.allow[(CASE, BAD)], case_id=uni)}
        r1, r2 = self.runs({BAD: b"\1" * 8}, case=uni, mesh="test/test_x/data/uniform.msh")
        with self.assertRaisesRegex(dn.DeterminismError, "stale"):
            dn.plan(r1, r2, self.src, allow)
        allow = {(CASE, "mesh_data/tag_9/_segment_2"): dict(self.allow[(CASE, BAD)],
                                                            quantity="mesh_data/tag_9/_segment_2")}
        r1, r2 = self.runs({BAD: b"\1" * 8})
        with self.assertRaises(dn.DeterminismError):
            dn.plan(r1, r2, self.src, allow)

    def test_known_defect_is_dropped_and_recorded(self):
        # Identical bytes in both runs (tail zero-initialised): still dropped
        # by the tag-count rule, recorded with defect, reason and detection.
        r1, r2 = self.runs({"nodes": b"\0" * 8, BAD: b"\1" * 8})
        rc = dn.main(["x", str(r1), str(r2), str(self.src), "--allow-list", self.write_allow()])
        self.assertEqual(rc, 0)
        m = manifest(r1, CASE)
        self.assertEqual([q["name"] for q in m["quantities"]], ["nodes"])
        rec = m["parameters"]["nondeterministic_quantities"]
        self.assertEqual(rec, [{"name": BAD, "defect": "UD-064", "reason": "test",
                                "criterion": "reviewed allow-list entry, derived by the "
                                             "Gmsh 2.x tag-count rule"}])
        self.assertIn("UD-064", m["notes"])
        self.assertFalse((r1 / CASE / f"data/{BAD.replace('/', '__')}.npy").exists())
        self.assertEqual(m["storage"]["bytes"], 8 + 4)  # nodes + mesh input
        # Differing bytes are reported as a second detection reason.
        r1, r2 = self.runs({"nodes": b"\0" * 8, BAD: b"\1" * 8},
                           {"nodes": b"\0" * 8, BAD: b"\7" * 8})
        drops = dn.plan(r1, r2, self.src, self.allow)
        self.assertEqual(len(drops[CASE][0][2]), 2)

    def test_command_line(self):
        r1, r2 = self.runs({"nodes": b"\0" * 8})
        script = str(PYTHON_DIR / "drop_nondeterministic.py")
        p = subprocess.run([sys.executable, "-I", script, str(r1), str(r2), str(self.src)],
                           capture_output=True, text=True, check=False)
        self.assertEqual(p.returncode, 2, "--allow-list is mandatory")
        p = subprocess.run([sys.executable, "-I", script, str(r1), str(self.root / "missing"),
                            str(self.src), "--allow-list", self.write_allow()],
                           capture_output=True, text=True, check=False)
        self.assertEqual(p.returncode, 1)
        self.assertIn("second run", p.stderr)

    def write_allow(self) -> str:
        p = self.root / "allow.json"
        p.write_text(json.dumps({"format_version": 1, "entries": list(self.allow.values())}))
        return str(p)


class ShippedAllowListTest(unittest.TestCase):
    """The reviewed allow-list holds exactly the six Gmsh exclusions, and the
    committed fixtures exclude exactly those (with their reasons)."""

    def setUp(self):
        self.allow = dn.load_allow_list(PYTHON_DIR / "known_nondeterminism.json")

    def test_exactly_the_six_gmsh_exclusions(self):
        self.assertEqual(len(self.allow), 6)
        self.assertEqual({e["defect"] for e in self.allow.values()}, {"UD-064"})
        expected = {f"mesh_io/test_model/{p}/data/cohesive_strait_2D"
                    for p in ("test_contact_mechanics_model/test_detector",
                              "test_solid_mechanics_model/test_cohesive")}
        self.assertEqual({c for c, _ in self.allow}, expected)
        self.assertEqual({q for _, q in self.allow},
                         {"mesh_data/tag_2/_segment_2", "mesh_data/tag_3/_segment_2",
                          "mesh_data/tag_5/_triangle_3"})

    def test_committed_fixtures_record_exactly_the_allow_list(self):
        recorded = set()
        for m in (REPO / "fixtures/mesh_io").rglob("manifest.json"):
            j = json.loads(m.read_text())
            for e in j.get("parameters", {}).get("nondeterministic_quantities", []):
                self.assertIsInstance(e, dict, f"{j['case_id']}: exclusion without reason")
                self.assertEqual(e["defect"], self.allow[(j["case_id"], e["name"])]["defect"])
                recorded.add((j["case_id"], e["name"]))
        self.assertEqual(recorded, set(self.allow))

    @unittest.skipUnless(os.environ.get("AKANTU_SRC_HOST")
                         and Path(os.environ["AKANTU_SRC_HOST"], "test").is_dir(),
                         "reference checkout not available (set AKANTU_SRC_HOST)")
    def test_rule_derives_the_allow_list_from_the_reference_meshes(self):
        src = Path(os.environ["AKANTU_SRC_HOST"])
        derived = set()
        for case, _ in self.allow:
            rel = "test/" + case[len("mesh_io/"):] + ".msh"
            derived |= {(case, q) for q in dn.msh2_bad_tags(src / rel)}
        self.assertEqual(derived, set(self.allow))


if __name__ == "__main__":
    unittest.main()
