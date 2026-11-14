# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: tests of finalize_fixtures.py and of
# the stored regeneration commands.
from __future__ import annotations

import json
import os
import shlex
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from helpers import REPO, manifest, write_case

import finalize_fixtures as ff

BIG = "mesh_io/test_big/data/big"
SMALL = "mesh_io/test_small/data/small"
LIMIT = 64  # bytes, via the `limit` argument
NOW = "2026-11-10T00:00:00Z"


def snapshot(root: Path) -> dict:
    """Every file (bytes) and directory of a tree: byte-for-byte state."""
    files, dirs = {}, set()
    for p in sorted(root.rglob("*")):
        rel = p.relative_to(root).as_posix()
        if p.is_dir():
            dirs.add(rel)
        else:
            files[rel] = p.read_bytes()
    return {"files": files, "dirs": dirs}


def dry_run(command: str) -> subprocess.CompletedProcess:
    """Validate a stored driver command without Docker (--dry-run)."""
    args = shlex.split(command)
    assert args[0] == "oracle/scripts/generate-fixtures.sh", args
    return subprocess.run(["bash", str(REPO / args[0]), "--dry-run", *args[1:]],
                          capture_output=True, text=True, check=False)


class FinalizeTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "fixtures"
        self.stage = Path(self.tmp.name) / "stage"
        self.root.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def external_clone(self) -> Path:
        """A fresh clone: external manifest committed, data git-ignored."""
        storage = {"mode": "external", "bytes": 100, "generate": "old",
                   "location": "regenerate locally (no remote store yet)"}
        d = write_case(self.root, BIG, {"nodes": b"\1" * 100}, storage=storage, write_data=False)
        (self.root / ".gitignore").write_text(f"/{BIG}/data/\n/{BIG}/inputs/\n")
        write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        return d

    def finalize(self, stage=None, created=None, areas=()):
        return ff.finalize(self.root, stage, created, list(areas), LIMIT)

    def test_fresh_clone_preserves_unmaterialised_external_case(self):
        d = self.external_clone()
        before = (d / "manifest.json").read_text()
        s = self.finalize()
        self.assertEqual(s["unmaterialised"], [BIG])
        self.assertEqual((d / "manifest.json").read_text(), before, "manifest must not change")
        ignore = (self.root / ".gitignore").read_text()
        self.assertIn(f"/{BIG}/data/", ignore)
        self.assertIn(f"/{BIG}/inputs/", ignore)
        sums = (self.root / "SHA256SUMS").read_text()
        self.assertIn(f"{BIG}/manifest.json", sums)
        self.assertIn(f"{SMALL}/data/nodes.npy", sums)
        # An unrelated partial run (stage with the small case only).
        write_case(self.stage, SMALL, {"nodes": b"\3" * 8}, created=NOW)
        s = self.finalize(self.stage, NOW)
        self.assertEqual(s["promoted"], [SMALL])
        self.assertEqual((d / "manifest.json").read_text(), before)
        self.assertIn(f"/{BIG}/data/", (self.root / ".gitignore").read_text())

    def assert_fails_unchanged(self, pattern, stage=None, created=None, areas=()):
        """Finalisation fails with `pattern` and leaves the tree untouched."""
        before = snapshot(self.root)
        with self.assertRaisesRegex(ff.FinalizeError, pattern):
            self.finalize(stage, created, areas)
        self.assertEqual(snapshot(self.root), before)

    def test_partial_or_corrupt_external_data_is_an_error(self):
        d = self.external_clone()
        (d / "data").mkdir()
        (d / "data/nodes.npy").write_bytes(b"\1" * 99)
        self.assert_fails_unchanged("manifest checksum")
        write_case(self.root, BIG, {"nodes": b"\1" * 100, "x": b"\0" * 8},
                   storage={"mode": "external", "bytes": 108}, write_data=False)
        (d / "data/nodes.npy").write_bytes(b"\1" * 100)
        self.assert_fails_unchanged("partially present")

    def test_committed_case_without_data_is_an_error(self):
        write_case(self.root, SMALL, {"nodes": b"\2" * 8}, write_data=False)
        self.assert_fails_unchanged("committed case has no data")

    def test_case_written_by_this_run_must_have_its_data(self):
        write_case(self.root, BIG, {"nodes": b"\1" * 100}, created=NOW, write_data=False,
                   storage={"mode": "external", "bytes": 100})
        self.assert_fails_unchanged("although this run wrote it", created=NOW)

    def test_incomplete_staged_case_is_an_error(self):
        write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        self.finalize()
        d = write_case(self.stage, SMALL, {"nodes": b"\4" * 8, "x": b"\5" * 8}, created=NOW)
        (d / "data/x.npy").unlink()
        self.assert_fails_unchanged("partially present", self.stage, NOW)

    def test_materialising_identical_external_data_keeps_the_manifest(self):
        d = self.external_clone()
        write_case(self.stage, BIG, {"nodes": b"\1" * 100}, created=NOW)
        self.finalize(self.stage, NOW)
        m = manifest(self.root, BIG)
        self.assertEqual(m["created"], "2026-11-09T00:00:00Z", "committed manifest kept")
        # ... except that the broken placeholder command becomes concrete.
        self.assertEqual(m["storage"]["generate"],
                         f"oracle/scripts/generate-fixtures.sh --config strict mesh:{BIG}")
        self.assertEqual(m["storage"]["mode"], "external")
        self.assertTrue((d / "data/nodes.npy").is_file())
        self.assertNotIn(f"{BIG}/data", (self.root / "SHA256SUMS").read_text())

    def test_regeneration_with_new_provenance_replaces_the_manifest(self):
        self.external_clone()
        write_case(self.stage, BIG, {"nodes": b"\1" * 100}, created=NOW,
                   extra={"build": {"config": "strict", "compiler": "g++ 15", "arch": "aarch64"}})
        self.finalize(self.stage, NOW)
        m = manifest(self.root, BIG)
        self.assertEqual(m["created"], NOW)
        self.assertEqual(m["build"]["compiler"], "g++ 15")
        self.assertEqual(m["storage"]["mode"], "external")

    def test_large_new_case_becomes_external_with_a_runnable_command(self):
        write_case(self.stage, BIG, {"nodes": b"\1" * 100}, created=NOW)
        self.finalize(self.stage, NOW)
        m = manifest(self.root, BIG)
        self.assertEqual(m["storage"]["mode"], "external")
        self.assertEqual(m["storage"]["bytes"], 100)
        cmd = m["storage"]["generate"]
        self.assertNotIn("<", cmd)
        self.assertNotIn("#", cmd)
        p = dry_run(cmd)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn(f"mesh:{BIG}", p.stdout)
        self.assertIn(f"/{BIG}/data/", (self.root / ".gitignore").read_text())

    def test_small_external_case_becomes_committed_once_materialised(self):
        storage = {"mode": "external", "bytes": 8, "generate": "x"}
        write_case(self.root, SMALL, {"nodes": b"\2" * 8}, storage=storage)
        (self.root / ".gitignore").write_text(f"/{SMALL}/data/\n/{SMALL}/inputs/\n")
        self.finalize()
        self.assertEqual(manifest(self.root, SMALL)["storage"], {"mode": "committed", "bytes": 8})
        self.assertNotIn(SMALL, (self.root / ".gitignore").read_text())
        self.assertIn(f"{SMALL}/data/nodes.npy", (self.root / "SHA256SUMS").read_text())

    def test_complete_area_detects_stale_cases(self):
        write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        write_case(self.stage, "mesh_io/test_other/data/x", {"nodes": b"\2" * 8}, created=NOW)
        self.assert_fails_unchanged("stale case", self.stage, NOW, ["mesh_io"])

    def test_promotion_never_touches_tolerances(self):
        d = write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        (d / "tolerances.json").write_text("{reviewed}")
        (d / "data/stale.npy").write_bytes(b"old")
        write_case(self.stage, SMALL, {"nodes": b"\4" * 8}, created=NOW)
        self.finalize(self.stage, NOW)
        self.assertEqual((d / "tolerances.json").read_text(), "{reviewed}")
        self.assertFalse((d / "data/stale.npy").exists(), "data/ is replaced as a whole")
        self.assertEqual((d / "data/nodes.npy").read_bytes(), b"\4" * 8)
        self.assertNotIn("tolerances.json", (self.root / "SHA256SUMS").read_text())

    def test_regeneration_targets(self):
        for probe, case, target in [
                ("mesh_probe", "mesh_io/a/b", "mesh:mesh_io/a/b"),
                ("material_probe", "material/elastic/2d_shear", "material:material/elastic/2d_shear"),
                ("material_probe", "regression/material/t", "regression:regression/material/t"),
                ("model_probe", "model/static/patch_triangle_3", "model:model/static/patch_triangle_3"),
                ("element_probe", "fe/element_class/_triangle_3", "element"),
                ("writer_selftest_cpp", "selftest/cpp_writer_roundtrip", "selftest")]:
            m = {"case_id": case, "generator": {"probe": probe}, "build": {"config": "strict"}}
            cmd = ff.regeneration_command(m)
            self.assertTrue(cmd.endswith(" " + target), cmd)
            p = dry_run(cmd)
            self.assertEqual(p.returncode, 0, f"{cmd}: {p.stdout}{p.stderr}")


OTHER = "mesh_io/test_other/data/other"
NEW = "mesh_io/test_new/data/new"
EXT = "mesh_io/test_ext/data/ext"


class TransactionTest(unittest.TestCase):
    """Finalisation validates the complete candidate tree before it
    writes anything and swaps it in transactionally: a failed finalisation
    leaves every prior manifest, payload, ignore file and checksum index
    byte-for-byte unchanged."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "fixtures"
        self.stage = Path(self.tmp.name) / "stage"
        self.root.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def test_corrupt_staged_payload_leaves_the_tree_unchanged(self):
        """The review's reproduction with the real selftest/cpp_writer_roundtrip
        case: flip the last byte of the staged data/special.npy without
        updating its manifest checksum."""
        case = "selftest/cpp_writer_roundtrip"
        src = REPO / "fixtures" / case
        shutil.copytree(src, self.root / case)
        shutil.copytree(src, self.stage / case)
        ff.finalize(self.root, None, None, [], 1024 ** 2)  # valid root
        before = snapshot(self.root)
        for rel in (f"{case}/manifest.json", f"{case}/data/special.npy", ".gitignore",
                    "SHA256SUMS"):
            self.assertIn(rel, before["files"])
        payload = self.stage / case / "data/special.npy"
        original = payload.read_bytes()
        corrupt = bytearray(original)
        corrupt[-1] ^= 0xFF
        payload.write_bytes(bytes(corrupt))
        with self.assertRaisesRegex(ff.FinalizeError,
                                    "special.npy does not match its manifest checksum"):
            ff.finalize(self.root, self.stage, None, [], 1024 ** 2)
        self.assertEqual(snapshot(self.root), before)
        self.assertEqual((self.root / case / "data/special.npy").read_bytes(), original)
        # The intact staged case promotes to the identical tree.
        payload.write_bytes(original)
        s = ff.finalize(self.root, self.stage, None, [], 1024 ** 2)
        self.assertEqual(s["promoted"], [case])
        self.assertEqual(snapshot(self.root), before)

    def test_stale_case_with_valid_staged_data_leaves_the_tree_unchanged(self):
        write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        write_case(self.root, OTHER, {"nodes": b"\5" * 8})
        ff.finalize(self.root, None, None, [], LIMIT)
        before = snapshot(self.root)
        write_case(self.stage, SMALL, {"nodes": b"\4" * 8}, created=NOW)  # valid, new payload
        with self.assertRaisesRegex(ff.FinalizeError, "stale case"):
            ff.finalize(self.root, self.stage, NOW, ["mesh_io"], LIMIT)
        self.assertEqual(snapshot(self.root), before)
        self.assertEqual((self.root / SMALL / "data/nodes.npy").read_bytes(), b"\2" * 8)

    def scenario(self) -> None:
        """A run that replaces a payload, adds a new case (new directories),
        rewrites a manifest by the size policy, keeps an unmaterialised
        external case and rewrites .gitignore and SHA256SUMS."""
        write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        write_case(self.root, BIG, {"nodes": b"\1" * 100}, write_data=False,
                   storage={"mode": "external", "bytes": 100, "generate": "x"})
        write_case(self.root, EXT, {"nodes": b"\3" * 8},
                   storage={"mode": "external", "bytes": 8, "generate": "x"})
        (self.root / ".gitignore").write_text(f"/{BIG}/data/\n/{BIG}/inputs/\n"
                                              f"/{EXT}/data/\n/{EXT}/inputs/\n")
        (self.root / "SHA256SUMS").write_text("stale index\n")
        (self.root / "mesh_io" / "tolerances.json").write_text("{reviewed}")
        write_case(self.stage, SMALL, {"nodes": b"\4" * 8}, created=NOW)
        write_case(self.stage, NEW, {"nodes": b"\6" * 8},
                   inputs={"mesh": (b"mesh", "upstream:x.msh")}, created=NOW)

    def count_calls(self, name: str) -> int:
        """Calls of the I/O primitive `name` in a successful run of the
        scenario (on a copy)."""
        copy = Path(self.tmp.name) / "count"
        shutil.copytree(self.root, copy / "fixtures")
        real = getattr(ff, name)
        calls = []

        def counting(*a):
            calls.append(a)
            return real(*a)

        with mock.patch.object(ff, name, side_effect=counting):
            ff.finalize(copy / "fixtures", self.stage, NOW, [], LIMIT)
        shutil.rmtree(copy)
        return len(calls)

    def failing(self, name: str, *at: int):
        """Side effect for primitive `name` raising OSError at the given
        (1-based) calls and behaving normally otherwise."""
        real = getattr(ff, name)
        n = [0]

        def effect(*a):
            n[0] += 1
            if n[0] in at:
                raise OSError(f"injected failure of {name} call {n[0]}")
            return real(*a)

        return effect

    def test_io_failure_at_any_step_rolls_every_change_back(self):
        self.scenario()
        before = snapshot(self.root)
        txn = self.root / ff.TRANSACTION
        for name in ("_copy", "_write", "_move"):
            n = self.count_calls(name)
            self.assertGreater(n, 0, name)
            for k in range(1, n + 1):
                with mock.patch.object(ff, name, side_effect=self.failing(name, k)):
                    with self.assertRaisesRegex(OSError, "injected"):
                        ff.finalize(self.root, self.stage, NOW, [], LIMIT)
                self.assertEqual(snapshot(self.root), before, f"{name} call {k}")
                self.assertFalse(txn.exists(), f"{name} call {k}")
        # Without failures every planned change is applied.
        s = ff.finalize(self.root, self.stage, NOW, [], LIMIT)
        self.assertEqual(s["promoted"], [NEW, SMALL])
        self.assertEqual(s["unmaterialised"], [BIG])
        self.assertFalse(txn.exists())
        self.assertEqual((self.root / SMALL / "data/nodes.npy").read_bytes(), b"\4" * 8)
        self.assertEqual((self.root / NEW / "inputs/mesh.txt").read_bytes(), b"mesh")
        self.assertEqual(manifest(self.root, EXT)["storage"], {"mode": "committed", "bytes": 8})
        ignore = (self.root / ".gitignore").read_text()
        self.assertIn(f"/{BIG}/data/", ignore)
        self.assertNotIn(EXT, ignore)
        sums = (self.root / "SHA256SUMS").read_text()
        self.assertIn(f"{NEW}/data/nodes.npy", sums)
        self.assertIn(f"{EXT}/manifest.json", sums)
        self.assertNotIn("tolerances.json", sums)
        self.assertEqual((self.root / "mesh_io" / "tolerances.json").read_text(), "{reviewed}")
        for line in sums.splitlines():
            digest, rel = line.split("  ", 1)
            self.assertEqual(ff.sha256_bytes((self.root / rel).read_bytes()), digest, rel)

    def test_failed_rollback_keeps_the_previous_versions_and_blocks_the_next_run(self):
        self.scenario()
        before = snapshot(self.root)
        n = self.count_calls("_move")
        # Fail half-way through the swap, then fail the first rollback move.
        k = n // 2 + 1
        with mock.patch.object(ff, "_move", side_effect=self.failing("_move", k, k + 1)):
            with self.assertRaisesRegex(ff.FinalizeError, "rollback failed"):
                ff.finalize(self.root, self.stage, NOW, [], LIMIT)
        txn = self.root / ff.TRANSACTION
        journal = json.loads((txn / "journal.json").read_text())
        self.assertIn(f"{SMALL}/data", journal["paths"])
        self.assertIn("SHA256SUMS", journal["paths"])
        # Every original is still in place or recoverable from old/<path>
        # (replaced directories are moved whole, so old/<path> holds files
        # below them too).
        for rel, data in before["files"].items():
            here, kept = self.root / rel, txn / "old" / rel
            self.assertTrue((here.is_file() and here.read_bytes() == data)
                            or (kept.is_file() and kept.read_bytes() == data), rel)
        self.assertTrue(any((txn / "old").rglob("*")), "previous versions kept")
        mid = snapshot(self.root)
        with self.assertRaisesRegex(ff.FinalizeError, "interrupted"):
            ff.finalize(self.root, self.stage, NOW, [], LIMIT)
        self.assertEqual(snapshot(self.root), mid)

    def test_interrupted_transaction_blocks_finalisation(self):
        write_case(self.root, SMALL, {"nodes": b"\2" * 8})
        (self.root / ff.TRANSACTION).mkdir()
        before = snapshot(self.root)
        with self.assertRaisesRegex(ff.FinalizeError, "interrupted"):
            ff.finalize(self.root, None, None, [], LIMIT)
        self.assertEqual(snapshot(self.root), before)

    def test_command_line_failure_leaves_the_tree_unchanged(self):
        self.scenario()
        before = snapshot(self.root)
        d = self.stage.joinpath(*SMALL.split("/"))
        (d / "data/nodes.npy").write_bytes(b"\7" * 8)  # checksum mismatch
        p = subprocess.run(["python3", "-I", str(Path(ff.__file__)), str(self.root),
                            "--stage", str(self.stage), "--created", NOW, "--limit-kib", "1"],
                           capture_output=True, text=True, check=False)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn("nothing was modified", p.stderr)
        self.assertEqual(snapshot(self.root), before)


class DriverGrammarTest(unittest.TestCase):
    def test_placeholders_and_malformed_targets_are_rejected(self):
        for bad in ["--config strict <target>", "regression x/y", "mesh:../etc",
                    "model:static/patch", "--standalone mesh", "--config", "frobnicate"]:
            p = dry_run("oracle/scripts/generate-fixtures.sh " + bad)
            self.assertNotEqual(p.returncode, 0, bad)

    def test_committed_external_commands_are_runnable(self):
        n = 0
        for m in (REPO / "fixtures").rglob("manifest.json"):
            j = json.loads(m.read_text())
            if j.get("storage", {}).get("mode") == "external":
                n += 1
                p = dry_run(j["storage"]["generate"])
                self.assertEqual(p.returncode, 0, f"{j['case_id']}: {p.stderr}")
        self.assertGreaterEqual(n, 1)


if __name__ == "__main__":
    unittest.main()


def _lane(root: str, stage: str, created: str, out: str) -> None:
    """One regeneration lane finalising its staged cases (separate process)."""
    try:
        ff.finalize(Path(root), Path(stage), created, [], LIMIT)
        Path(out).write_text("ok")
    except Exception as e:  # reported to the parent test
        Path(out).write_text(f"{type(e).__name__}: {e}")


class ConcurrentLanesTest(unittest.TestCase):
    """Lanes finalising into one root in parallel
    processes no longer collide on the transaction directory, and the final
    SHA256SUMS indexes every lane's cases and matches the tree."""

    def test_parallel_lanes_are_serialised(self):
        import multiprocessing
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "fixtures"
            root.mkdir()
            write_case(root, "area/base", {"nodes": b"\0" * 8})
            ff.finalize(root, None, None, [], LIMIT)
            lanes = []
            for k in range(6):
                stage = Path(tmp) / f"stage{k}"
                for j in range(20):
                    write_case(stage, f"lane{k}/case{j}", {"x": bytes([k, j]) * 4},
                               created=f"2026-11-11T00:00:0{k}Z")
                lanes.append((str(root), str(stage), f"2026-11-11T00:00:0{k}Z",
                              str(Path(tmp) / f"out{k}")))
            ctx = multiprocessing.get_context("spawn")
            procs = [ctx.Process(target=_lane, args=a) for a in lanes]
            for p in procs:
                p.start()
            for p in procs:
                p.join(120)
            results = [Path(a[3]).read_text() for a in lanes]
            self.assertEqual(results, ["ok"] * 6)
            self.assertFalse((root / ff.TRANSACTION).exists())
            index = (root / "SHA256SUMS").read_text().splitlines()
            indexed = {line.split("  ", 1)[1] for line in index}
            for k in range(6):
                for j in range(20):
                    self.assertIn(f"lane{k}/case{j}/manifest.json", indexed)
            for line in index:
                digest, rel = line.split("  ", 1)
                self.assertEqual(ff.sha256_bytes((root / rel).read_bytes()), digest, rel)

