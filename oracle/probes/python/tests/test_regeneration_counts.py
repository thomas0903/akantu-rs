# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: tests of oracle/scripts/regeneration_counts.py (regeneration
# counts must be reproducible from the git trees they name).
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from helpers import REPO, write_case

sys.path.insert(0, str(REPO / "oracle" / "scripts"))
import regeneration_counts as rc  # noqa: E402


class CountPolicyTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.a = Path(self.tmp.name) / "a"
        self.b = Path(self.tmp.name) / "b"

    def tearDown(self):
        self.tmp.cleanup()

    def test_every_category_is_counted_separately(self):
        write_case(self.a, "x/same", {"u": b"\1" * 8, "v": b"\2" * 8, "gone": b"\0" * 8})
        write_case(self.b, "x/same", {"u": b"\1" * 8, "v": b"\x09" * 8, "new": b"\0" * 8})
        write_case(self.a, "x/label", {"s": b"\3" * 8})
        d = write_case(self.b, "x/label", {"s": b"\3" * 8})
        m = json.loads((d / "manifest.json").read_text())
        m["quantities"][0]["components"] = ["xx"]
        m["quantities"][0]["description"] = "relabelled"
        (d / "manifest.json").write_text(json.dumps(m))
        write_case(self.a, "x/doc", {"t": b"\4" * 8})
        d = write_case(self.b, "x/doc", {"t": b"\4" * 8})
        m = json.loads((d / "manifest.json").read_text())
        m["quantities"][0]["description"] = "new text"
        (d / "manifest.json").write_text(json.dumps(m))
        ext = {"mode": "external", "bytes": 8}
        write_case(self.a, "x/external", {"e": b"\5" * 8}, storage=ext, write_data=False)
        write_case(self.b, "x/external", {"e": b"\5" * 8}, storage=ext)
        write_case(self.a, "x/old", {"o": b"\6" * 8})
        write_case(self.b, "x/added", {"n": b"\7" * 8})
        c = rc.count(self.a, self.b)
        self.assertEqual(c["policy"], rc.POLICY)
        self.assertEqual(c["cases"], {"a": 5, "b": 5, "shared": 4, "only_a": ["x/old"],
                                      "only_b": ["x/added"]})
        self.assertEqual(c["unmaterialised"], ["x/external"])
        self.assertEqual(c["raw_payloads"], {"shared": 4, "identical": 3,
                                             "different": ["x/same::data/v.npy"]})
        q = c["quantities"]
        self.assertEqual((q["shared"], q["comparable"], q["bit_identical"]), (4, 3, 2))
        self.assertEqual(q["different"], ["x/same::v"])
        self.assertEqual(q["semantic_changes"], [{"quantity": "x/label::s",
                                                  "fields": ["components"],
                                                  "payload_identical": True}])
        self.assertEqual(q["metadata_only"], 2, "label change and description change")
        self.assertEqual(q["only_a"], ["x/same::gone"])
        self.assertEqual(q["only_b"], ["x/same::new"])
        text = rc.summary(c)
        self.assertIn("raw shared payloads: 4, byte-identical 3, different 1", text)
        self.assertIn("unmaterialised shared cases (not counted): 1", text)

    def git_trees(self) -> tuple[Path, str, str]:
        """A scratch git repository holding the synthetic trees A and B;
        returns it with the tree ids of A and B."""
        repo = Path(self.tmp.name) / "repo"
        write_case(repo / "a", "x/same", {"u": b"\1" * 8, "v": b"\2" * 8})
        write_case(repo / "b", "x/same", {"u": b"\1" * 8, "v": b"\x09" * 8})
        git = ["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@invalid"]
        subprocess.run(git + ["init", "-q"], check=True)
        subprocess.run(git + ["add", "-A"], check=True)
        subprocess.run(git + ["commit", "-qm", "trees"], check=True)
        def tree(sub: str) -> str:
            return subprocess.run(git + ["rev-parse", f"HEAD:{sub}"], check=True,
                                  capture_output=True, text=True).stdout.strip()

        return repo, tree("a"), tree("b")

    def test_records_are_recomputed_from_tree_ids(self):
        if shutil.which("git") is None:
            self.skipTest("git is not installed")
        repo, ta, tb = self.git_trees()
        counts = rc.count_git(ta, tb, repo)
        self.assertEqual(counts["raw_payloads"]["different"], ["x/same::data/v.npy"])
        rec = {"baseline": {"fixtures_tree": ta}, "candidate": {"fixtures_tree": tb},
               "counts": counts}
        d = Path(self.tmp.name) / "records"
        d.mkdir()
        (d / "r.json").write_text(json.dumps(rec))
        self.assertEqual(rc.check_records(d, repo), ["r.json"])
        rec["counts"]["raw_payloads"]["identical"] += 1
        (d / "r.json").write_text(json.dumps(rec))
        with self.assertRaisesRegex(rc.CountError, "differ from the record"):
            rc.check_records(d, repo)
        (d / "r.json").write_text(json.dumps({"counts": {}}))
        with self.assertRaisesRegex(rc.CountError, "malformed record"):
            rc.check_records(d, repo)
        with self.assertRaisesRegex(rc.CountError, "no record"):
            rc.check_records(repo / "a", repo)
        with self.assertRaisesRegex(rc.CountError, "git archive"):
            rc.count_git("0" * 40, "0" * 40, repo)

    def test_command_line(self):
        script = REPO / "oracle" / "scripts" / "regeneration_counts.py"
        p = subprocess.run([sys.executable, "-I", str(script), "--check-records", self.tmp.name],
                           capture_output=True, text=True, check=False)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("no record", p.stderr)
        p = subprocess.run([sys.executable, "-I", str(script), "--nope"],
                           capture_output=True, text=True, check=False)
        self.assertEqual(p.returncode, 2)


if __name__ == "__main__":
    unittest.main()
