# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: tests of build_env.py. They use
# temporary metadata trees only, never the oracle installation.
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from helpers import SHA

import build_env as be

INFO = f"""config=strict
reference_sha={SHA}
scripts_sha=8db18251c593ad166e8ed84a28f7f8e65ec161a5
image_id=sha256:aaaa
compiler=14.2.0 (/usr/bin/g++)
CMAKE_BUILD_TYPE:STRING=RelWithDebInfo
CMAKE_CXX_FLAGS:STRING=-ffp-contract=off -DEIGEN_DONT_VECTORIZE
CMAKE_CXX_FLAGS_RELWITHDEBINFO:STRING= -O2 -g -DNDEBUG
"""


class InstallCheckTest(unittest.TestCase):
    def test_matching_install(self):
        ids = be.check_install(INFO, "strict", SHA, "sha256:aaaa", False)
        self.assertEqual(ids["reference_sha"], SHA)

    def test_mismatches_are_refused(self):
        with self.assertRaisesRegex(be.ProvenanceError, "built from reference"):
            be.check_install(INFO, "strict", "0" * 40, None, False)
        with self.assertRaisesRegex(be.ProvenanceError, "configuration"):
            be.check_install(INFO, "default", SHA, None, False)
        with self.assertRaisesRegex(be.ProvenanceError, "does not record reference_sha"):
            be.check_install(INFO.replace(f"reference_sha={SHA}\n", ""), "strict", SHA, None, False)
        with self.assertRaisesRegex(be.ProvenanceError, "needs --expect-sha"):
            be.check_install(INFO, "strict", None, None, False)
        with self.assertRaisesRegex(be.ProvenanceError, "image"):
            be.check_install(INFO, "strict", SHA, "sha256:bbbb", False)
        ids = be.check_install(INFO, "strict", SHA, "sha256:bbbb", True)
        self.assertIn("sha256:bbbb", ids["image_mismatch"])

    def test_library_flags_follow_the_build_type(self):
        flags, bt = be.library_flags(INFO)
        self.assertEqual(bt, "RelWithDebInfo")
        self.assertEqual(flags, "-ffp-contract=off -DEIGEN_DONT_VECTORIZE -O2 -g -DNDEBUG")

    def test_base_command_refuses_a_stale_install(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d, "build-info.txt")
            f.write_text(INFO.replace(SHA, "1" * 40))
            out = Path(d, "out.json")
            rc = be.main(["x", "base", "strict", "installed", str(f), str(out),
                          "--expect-sha", SHA])
            self.assertEqual(rc, 3)
            self.assertFalse(out.exists())
            f.write_text(INFO)
            rc = be.main(["x", "base", "strict", "installed", str(f), str(out),
                          "--expect-sha", SHA, "--library", "k=v"])
            self.assertEqual(rc, 0)
            info = json.loads(out.read_text())
            self.assertEqual(info["libraries"]["oracle_install_reference_sha"], SHA)
            self.assertEqual(info["libraries"]["k"], "v")


class ContentIdentityTest(unittest.TestCase):
    def test_patch_entries_carry_content_hashes(self):
        with tempfile.TemporaryDirectory() as d:
            ref, pat = Path(d, "ref"), Path(d, "pat")
            for root in (ref, pat):
                (root / "cmake").mkdir(parents=True)
                (root / "src").mkdir()
                (root / "src/a.cc").write_text("same")
            (ref / "cmake/M.cmake").write_text("old")
            (pat / "cmake/M.cmake").write_text("new")
            (pat / "src/added.cc").write_text("x")
            (ref / "src/removed.cc").write_text("y")
            (pat / "src/__pycache__").mkdir()
            (pat / "src/__pycache__/z.pyc").write_text("ignored")
            e = be.patch_entries(ref, pat)
            self.assertEqual(len(e), 3, e)
            new = be.sha256_file(pat / "cmake/M.cmake")
            self.assertTrue(e[0].startswith(f"modified:cmake/M.cmake sha256={new}"), e[0])
            self.assertTrue(e[1].startswith("added:src/added.cc sha256="), e[1])
            self.assertTrue(e[2].startswith("removed:src/removed.cc"), e[2])
            # A different patch content gives a different identifier.
            (pat / "cmake/M.cmake").write_text("newer")
            self.assertNotEqual(be.patch_entries(ref, pat)[0], e[0])
            t1 = be.tree_sha256(pat)
            (pat / "src/a.cc").write_text("changed")
            self.assertNotEqual(be.tree_sha256(pat), t1)

    def test_library_flags_from_compile_commands(self):
        with tempfile.TemporaryDirectory() as d:
            cc = Path(d, "compile_commands.json")
            cmd = ("/usr/bin/g++ -Dakantu_EXPORTS -I/src -ffp-contract=off -DEIGEN_DONT_VECTORIZE "
                   "-O2 -g -DNDEBUG -flto=auto -fPIC -Wall -frounding-math -o x.o -c "
                   f"/src/{be.LIBRARY_UNIT}")
            cc.write_text(json.dumps([{"directory": "/b", "command": cmd,
                                       "file": f"/src/{be.LIBRARY_UNIT}"}]))
            flags = be.library_unit_flags(cc)
            for f in ("-ffp-contract=off", "-DEIGEN_DONT_VECTORIZE", "-DNDEBUG", "-O2",
                      "-flto=auto", "-frounding-math"):
                self.assertIn(f, flags)
            for f in ("-I/src", "-Wall", "x.o", "akantu_EXPORTS"):
                self.assertNotIn(f, flags)
            info = be.base_info("strict", "installed", INFO, {}, {}, cc)
            self.assertIn("-frounding-math", info["cxx_flags"])
            self.assertEqual(info["libraries"]["cmake_cxx_flags"],
                             "-ffp-contract=off -DEIGEN_DONT_VECTORIZE -O2 -g -DNDEBUG")
            cc.write_text("[]")
            with self.assertRaises(be.ProvenanceError):
                be.library_unit_flags(cc)

    def test_probe_command_from_compile_commands(self):
        cmd = ("/usr/bin/g++ -DAKANTU_PROBE_CONFIG=\\\"strict\\\" -I/probes/include "
               "-isystem /usr/include/eigen3 -ffp-contract=off -DEIGEN_DONT_VECTORIZE "
               "-O2 -g -DNDEBUG -std=c++20 -o CMakeFiles/x.o -c /probes/cpp/mesh_probe.cc")
        with tempfile.TemporaryDirectory() as d:
            base = Path(d, "base.json")
            base.write_text(json.dumps({"config": "strict", "libraries": {}}))
            cc = Path(d, "compile_commands.json")
            cc.write_text(json.dumps([
                {"directory": "/b", "command": cmd, "file": "/probes/cpp/mesh_probe.cc"},
                {"directory": "/b", "command": "g++ -c y.cc", "file": "/probes/cpp/element_probe.cc"}]))
            out = Path(d, "probe.json")
            rc = be.main(["x", "probe", str(base), str(out), "mesh_probe",
                          "--compile-commands", str(cc), "--source-suffix", "cpp/mesh_probe.cc"])
            self.assertEqual(rc, 0)
            info = json.loads(out.read_text())
            self.assertIn("-c /probes/cpp/mesh_probe.cc", info["probe_compile_command"])
            flags = info["probe_cxx_flags"]
            for f in ("-ffp-contract=off", "-DEIGEN_DONT_VECTORIZE", "-O2", "-std=c++20"):
                self.assertIn(f, flags)
            for f in ("-I", "/usr/include/eigen3", "mesh_probe.cc", "x.o", "/usr/bin/g++"):
                self.assertNotIn(f, flags)
            rc = be.main(["x", "probe", str(base), str(out), "nope",
                          "--compile-commands", str(cc), "--source-suffix", "cpp/nope.cc"])
            self.assertEqual(rc, 3, "a probe without compile command is an error")


if __name__ == "__main__":
    unittest.main()


class NumericalEnvTest(unittest.TestCase):
    """D-050: x86_64 numerical pins are recorded; aarch64 unchanged.
    The pin is claimed only when the library
    AND the probe translation unit carry all four flags; the x86-64 level of
    the host is recorded."""

    PIN = ("-DEIGEN_NO_CPUID -DEIGEN_DEFAULT_L1_CACHE_SIZE=16384 "
           "-DEIGEN_DEFAULT_L2_CACHE_SIZE=524288 -DEIGEN_DEFAULT_L3_CACHE_SIZE=524288")
    V3 = ("model name\t: AMD EPYC 9455 48-Core Processor\n"
          "flags\t\t: fpu sse2 avx avx2 bmi1 bmi2 f16c fma movbe xsave abm avx512f\n")
    OLD = "model name\t: Old CPU\nflags\t\t: fpu sse2 avx xsave\n"

    def test_aarch64_records_nothing(self):
        self.assertEqual(be.numerical_env("-ffp-contract=off", "aarch64", {}), {})

    def test_x86_64_pinned(self):
        env = be.numerical_env(
            "-ffp-contract=off -DEIGEN_DONT_VECTORIZE " + self.PIN,
            "x86_64", {"OPENBLAS_CORETYPE": "Haswell", "OPENBLAS_NUM_THREADS": "1"}, self.V3)
        self.assertEqual(env["openblas_coretype"], "Haswell")
        self.assertEqual(env["openblas_num_threads"], "1")
        self.assertTrue(env["eigen_cache_sizes"].startswith("16384,524288,524288"))
        self.assertEqual(env["host_cpu"], "AMD EPYC 9455 48-Core Processor")
        self.assertEqual(env["x86_64_level"], "v3")

    def test_x86_64_unpinned_is_flagged(self):
        env = be.numerical_env("-ffp-contract=off", "x86_64", {}, self.V3)
        self.assertTrue(env["openblas_coretype"].startswith("auto"))
        self.assertEqual(env["eigen_cache_sizes"], "cpuid (host-dependent)")

    def test_partial_pin_is_not_a_pin(self):
        # EIGEN_NO_CPUID alone (other default sizes) must not be recorded as D-050.
        env = be.numerical_env("-DEIGEN_NO_CPUID", "x86_64", {}, self.V3)
        self.assertEqual(env["eigen_cache_sizes"], "cpuid (host-dependent)")

    def test_probe_tu_without_the_pin_is_not_pinned(self):
        self.assertTrue(be.eigen_statement(self.PIN, self.PIN).startswith("16384,524288,524288"))
        self.assertTrue(be.eigen_statement(self.PIN, "-O2 -std=c++20").startswith("NOT pinned"))
        self.assertIn("py11_akantu", be.eigen_statement(self.PIN, None, python=True))

    def test_x86_64_level(self):
        self.assertEqual(be.x86_64_level(self.V3), "v3")
        self.assertEqual(be.x86_64_level(self.OLD),
                         "below-v3 (missing: avx2 bmi1 bmi2 f16c fma movbe abm)")
        self.assertEqual(be.numerical_env(self.PIN, "x86_64", {}, self.OLD)["x86_64_level"],
                         "below-v3 (missing: avx2 bmi1 bmi2 f16c fma movbe abm)")


class ProbeRefinementTest(unittest.TestCase):
    """`build_env.py probe` replaces the library-level Eigen statement by the
    one that also accounts for the probe TU."""

    def _run(self, probe_flags: str) -> dict:
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            base = Path(d, "base.json")
            out = Path(d, "probe.json")
            base.write_text(json.dumps({
                "config": "strict", "cxx_flags": NumericalEnvTest.PIN, "arch": "x86_64",
                "libraries": {"cmake_cxx_flags": NumericalEnvTest.PIN,
                              "eigen_cache_sizes": "library statement"}}))
            rc = be.main(["build_env.py", "probe", str(base), str(out), "solver_probe",
                          "--command", f"g++ {probe_flags} -c solver_probe.cc"])
            self.assertEqual(rc, 0)
            return json.loads(out.read_text())

    def test_pinned_probe(self):
        libs = self._run(NumericalEnvTest.PIN)["libraries"]
        self.assertTrue(libs["eigen_cache_sizes"].startswith("16384,524288,524288"))

    def test_unpinned_probe(self):
        libs = self._run("-O2")["libraries"]
        self.assertTrue(libs["eigen_cache_sizes"].startswith("NOT pinned"))
