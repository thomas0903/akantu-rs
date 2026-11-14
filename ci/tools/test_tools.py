#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): unit tests of ci/tools/*.py.
#
#   python3 -m unittest discover -s ci/tools -p 'test_*.py' -v
#
# The licence-audit tests build tiny path-only cargo workspaces in a
# temporary directory (no registry access, nothing compiled); they are
# skipped when cargo is not installed.
"""Tests of the CI helper tools."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import count_tests  # noqa: E402
import feature_matrix  # noqa: E402
import gate_map  # noqa: E402
import pytest_skips  # noqa: E402
import licence_audit  # noqa: E402
import lock_unchanged  # noqa: E402
import package_report  # noqa: E402
import release_scope  # noqa: E402
import summarize  # noqa: E402
import version_check  # noqa: E402

ALLOW = {"MIT", "Apache-2.0", "BSD-3-Clause", "Unicode-3.0", "LGPL-3.0-or-later"}


class SpdxTests(unittest.TestCase):
    def ok(self, expr: str) -> bool:
        return licence_audit.satisfiable(licence_audit.parse(expr), ALLOW)

    def test_simple_and_or(self):
        self.assertTrue(self.ok("MIT"))
        self.assertTrue(self.ok("MIT OR Apache-2.0"))
        self.assertTrue(self.ok("GPL-3.0-only OR MIT"))
        self.assertFalse(self.ok("GPL-3.0-only"))
        self.assertTrue(self.ok("(MIT OR Apache-2.0) AND Unicode-3.0"))
        self.assertFalse(self.ok("MIT AND GPL-2.0-only"))

    def test_legacy_slash_and_plus(self):
        self.assertTrue(self.ok("MIT/Apache-2.0"))
        self.assertTrue(self.ok("Apache-2.0 / MIT"))
        self.assertTrue(self.ok("LGPL-3.0+"))
        self.assertFalse(self.ok("GPL-2.0/GPL-3.0"))

    def test_with_exception(self):
        self.assertTrue(self.ok("Apache-2.0 WITH LLVM-exception"))
        self.assertTrue(self.ok("Apache-2.0 WITH LLVM-exception OR GPL-3.0-only"))
        self.assertFalse(self.ok("GPL-2.0-only WITH Classpath-exception-2.0"))

    def test_precedence(self):
        # AND binds tighter than OR: GPL-3.0-only OR (MIT AND Unicode-3.0)
        self.assertTrue(self.ok("GPL-3.0-only OR MIT AND Unicode-3.0"))
        self.assertFalse(self.ok("(GPL-3.0-only OR MIT) AND SSPL-1.0"))

    def test_errors(self):
        for bad in ("", "MIT OR", "(MIT", "MIT)", "AND MIT", "MIT WITH", "MIT MIT"):
            with self.assertRaises(licence_audit.SpdxError, msg=bad):
                licence_audit.parse(bad)


def fake_meta(packages: list[dict]) -> dict:
    for p in packages:
        p.setdefault("id", f"{p['name']} 0.1.0")
        p.setdefault("targets", [{"kind": ["lib"]}])
        p.setdefault("features", {})
    return {"packages": packages, "workspace_members": [p["id"] for p in packages]}


class FeatureMatrixTests(unittest.TestCase):
    def test_plan_dedupes_and_marks_native_needs(self):
        meta = fake_meta([
            {"name": "plain"},
            {"name": "comm", "features": {"mpi": ["dep:mpi", "dep:cc"]}},
            {"name": "umb", "features": {"default": ["a"], "a": ["dep:x"], "b": ["a", "comm/mpi"]}},
        ])
        rows = feature_matrix.plan(meta)
        labels = [r[0] for r in rows]
        self.assertEqual(labels[:6], ["workspace:default:all-targets", "workspace:all-features:all-targets",
                                      "workspace:no-default-features", "workspace:no-default-features:all-targets",
                                      "workspace:all-features:clippy", "workspace:all-features:rustdoc"])
        self.assertNotIn("plain:no-default-features", labels)  # no features: workspace checks only
        self.assertIn("comm:feature=mpi", labels)
        needs = {r[0]: r[1] for r in rows}
        self.assertEqual(needs["comm:feature=mpi"], "mpi")
        self.assertEqual(needs["umb:feature=b"], "mpi")  # forwarded comm/mpi
        self.assertEqual(needs["umb:feature=a"], "")
        self.assertEqual(needs["workspace:all-features:all-targets"], "mpi")
        self.assertEqual(needs["workspace:all-features:clippy"], "mpi")  # feature-gated code is linted
        self.assertEqual(needs["workspace:all-features:rustdoc"], "mpi")
        # comm has no default list: its default = none, but the target
        # selection differs (lib vs all-targets), so both stay.
        self.assertIn("comm:default:all-targets", labels)
        self.assertEqual(len(labels), len(set(labels)))
        cmd = {r[0]: r[2] for r in rows}
        self.assertEqual(cmd["umb:feature=b"], "check -p umb --lib --locked --no-default-features --features b")

    def test_identical_feature_sets_are_checked_once(self):
        meta = fake_meta([{"name": "u", "features": {"x": ["y"], "y": []}}])
        labels = [r[0] for r in feature_matrix.plan(meta)]
        # all-features {x, y} == feature x alone {x, y}, but targets differ;
        # feature y alone is distinct.
        self.assertIn("u:feature=x", labels)
        self.assertIn("u:feature=y", labels)
        self.assertIn("u:all-features:all-targets", labels)


class SummarizeTests(unittest.TestCase):
    def run_case(self, tiers: dict[str, tuple[str, list[str]]]) -> tuple[int, dict]:
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            (out / "provenance.txt").write_text("schema=akantu-rs-ci-summary/1\ngit_head=abc\n")
            lines = []
            for name, (status, steps) in tiers.items():
                (out / name).mkdir()
                (out / name / "status").write_text(status + "\n")
                (out / name / "steps.tsv").write_text("".join(s + "\n" for s in steps))
                (out / name / "commands.tsv").write_text("")
                lines.append(f"{name}\t{status}\t3\t")
            (out / "summary.tsv").write_text("\n".join(lines) + "\n")
            with open(os.devnull, "w") as null:
                old = sys.stdout
                sys.stdout = null
                try:
                    code = summarize.main(["summarize.py", str(out)])
                finally:
                    sys.stdout = old
            return code, json.loads((out / "summary.json").read_text())

    def test_verdicts(self):
        code, s = self.run_case({"a": ("pass", ["x\tpass\t0\t1\t"])})
        self.assertEqual((code, s["verdict"]), (0, "PASS"))
        code, s = self.run_case({"a": ("pass", ["x\tpass\t0\t1\t"]), "b": ("incomplete", ["y\tskip\t-\t0\tno tool"])})
        self.assertEqual((code, s["verdict"], s["incomplete_tiers"]), (3, "INCOMPLETE", ["b"]))
        code, s = self.run_case({"a": ("fail", ["x\tfail\t1\t2\texit 1"]), "b": ("incomplete", [])})
        self.assertEqual((code, s["verdict"], s["failed_tiers"]), (1, "FAIL", ["a"]))
        step = s["tiers"][0]["steps"][0]
        self.assertEqual((step["exit_code"], step["seconds"], step["reason"]), (1, 2, "exit 1"))
        code, s = self.run_case({"a": ("pass", ["x\tpass\t0\t1\t"]), "b": ("na", ["x\tna\t-\t0\tnot here"])})
        self.assertEqual((code, s["verdict"]), (0, "PASS"))

    def test_nothing_ran_is_never_a_pass(self):
        # F8, F10: no tier, only n/a tiers, an unknown tier
        # status and a status that contradicts the steps are not PASS.
        code, s = self.run_case({})
        self.assertEqual((code, s["verdict"]), (1, "FAIL"))
        code, s = self.run_case({"a": ("na", ["x\tna\t-\t0\tnot here"])})
        self.assertEqual((code, s["verdict"]), (3, "INCOMPLETE"))
        code, s = self.run_case({"a": ("skipped", ["x\tpass\t0\t1\t"])})
        self.assertEqual((code, s["verdict"], s["failed_tiers"]), (1, "FAIL", ["a"]))
        code, s = self.run_case({"a": ("pass", ["x\tfail\t1\t1\texit 1"])})
        self.assertEqual((code, s["verdict"]), (1, "FAIL"))
        code, s = self.run_case({"a": ("pass", ["x\tpass\t0\t1\t", "y\tskip\t-\t0\tno tool"])})
        self.assertEqual((code, s["verdict"]), (3, "INCOMPLETE"))
        code, s = self.run_case({"a": ("pass", [])})  # recorded pass, no step ran
        self.assertEqual((code, s["verdict"]), (3, "INCOMPLETE"))
        code, s = self.run_case({"a": ("pass", ["x\tpassed\t0\t1\t"])})  # unknown step status
        self.assertEqual((code, s["verdict"]), (1, "FAIL"))


class ReleaseScopeTests(unittest.TestCase):
    META = {
        "workspace_members": ["a", "b", "c"],
        "packages": [
            {"id": "a", "name": "a", "manifest_path": "/src/a/Cargo.toml",
             "targets": [{"kind": ["lib"], "name": "a", "doctest": True}, {"kind": ["test"], "name": "t1", "doctest": False}]},
            {"id": "b", "name": "b", "manifest_path": "/src/b/Cargo.toml",
             "targets": [{"kind": ["lib"], "name": "b", "doctest": True}, {"kind": ["test"], "name": "t2", "doctest": False}]},
            {"id": "c", "name": "c", "manifest_path": "/src/c/Cargo.toml", "targets": [{"kind": ["lib"], "name": "c", "doctest": True}]},
        ],
    }
    KF = ("# header\n"
          "a | --test t1 | x::fails | B-1\n"
          "b | --test t2 | y::fails | B-2\n"
          "b | --lib | z::fails | B-3\n"
          "c | --lib | w::fails | B-4\n")

    def test_scope(self):
        specs = dict(release_scope.parse_spec(s) for s in ("a", "b:test=t2"))
        meta = release_scope.scope_metadata(self.META, specs)
        self.assertEqual([p["name"] for p in meta["packages"]], ["a", "b"])
        self.assertEqual(meta["workspace_members"], ["a", "b"])
        docs = {p["name"]: p["targets"][0]["doctest"] for p in meta["packages"]}
        self.assertEqual(docs, {"a": True, "b": False})  # b runs --test only: no doc tests expected
        self.assertTrue(self.META["packages"][1]["targets"][0]["doctest"])  # the input is not modified
        kf = release_scope.scope_baseline(self.KF, specs)
        self.assertIn("a | --test t1 | x::fails | B-1", kf)
        self.assertIn("b | --test t2 | y::fails | B-2", kf)
        self.assertNotIn("z::fails", kf)  # b's lib is not part of this run
        self.assertNotIn("w::fails", kf)  # package c is not part of this run

    def test_bad_specs(self):
        for bad in ("", ":test=x", "a:test=", "a:bin=x"):
            with self.assertRaises(ValueError, msg=bad):
                release_scope.parse_spec(bad)


class CountTestsTests(unittest.TestCase):
    LOG = (
        "test result: ok. 5 passed; 0 failed; 1 ignored; 0 measured; 2 filtered out; finished in 0.00s\n"
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n"
    )

    def run_tool(self, log: str, *args: str) -> int:
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "t.log"
            p.write_text(log)
            with open(os.devnull, "w") as null:
                old = sys.stdout
                sys.stdout = null
                try:
                    return count_tests.main(["count_tests.py", str(p), *args])
                finally:
                    sys.stdout = old

    def test_floors(self):
        self.assertEqual(count_tests.count(self.LOG)["passed"], 8)
        self.assertEqual(self.run_tool(self.LOG), 0)
        self.assertEqual(self.run_tool(self.LOG, "--min-passed", "8", "--min-results", "2"), 0)
        self.assertEqual(self.run_tool(self.LOG, "--min-passed", "9"), 1)
        self.assertEqual(self.run_tool(self.LOG, "--min-results", "3"), 1)

    def test_nothing_executed_fails_without_floors(self):
        # `cargo test <filter matching nothing>` (F6f) and an empty workspace (F6s/F6a)
        none = "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 0.00s\n"
        self.assertEqual(self.run_tool(none), 1)
        self.assertEqual(self.run_tool(""), 1)

    def test_floors_file(self):
        with tempfile.TemporaryDirectory() as d:
            f = Path(d) / "floors.txt"
            f.write_text("# c\nk.result_lines = 2\nk.passed = 9 # too many\nother.passed = 1\n")
            self.assertEqual(self.run_tool(self.LOG, "--floors", str(f), "--key", "k"), 1)
            f.write_text("k.result_lines = 2\nk.passed = 8\n")
            self.assertEqual(self.run_tool(self.LOG, "--floors", str(f), "--key", "k"), 0)


class PackageReportTests(unittest.TestCase):
    def test_publishable_member_must_be_ready(self):
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)
            for name in ("pub", "priv"):
                (repo / "crates" / name / "src").mkdir(parents=True)
                (repo / "crates" / name / "src" / "lib.rs").write_text("// SPDX-License-Identifier: LGPL-3.0-or-later\n")
                (repo / "crates" / name / "Cargo.toml").write_text("[package]\n")
            meta = {
                "workspace_root": str(repo),
                "workspace_members": ["pub", "priv"],
                "packages": [
                    {"id": "pub", "name": "pub", "manifest_path": str(repo / "crates/pub/Cargo.toml"),
                     "publish": None, "description": "d", "readme": None, "dependencies": []},
                    {"id": "priv", "name": "priv", "manifest_path": str(repo / "crates/priv/Cargo.toml"),
                     "publish": [], "description": None, "readme": None, "dependencies": []},
                ],
            }
            (repo / "meta.json").write_text(json.dumps(meta))
            lists = repo / "lists"
            lists.mkdir()
            for name in ("pub", "priv"):
                (lists / f"{name}.list").write_text("Cargo.toml\nCargo.toml.orig\nsrc/lib.rs\n")
            with open(os.devnull, "w") as null:
                old = sys.stderr, sys.stdout
                sys.stderr = sys.stdout = null
                try:
                    code = package_report.main(["p", str(repo / "meta.json"), str(lists), str(repo),
                                                "--json", str(repo / "r.json")])
                finally:
                    sys.stderr, sys.stdout = old
            r = json.loads((repo / "r.json").read_text())
            self.assertEqual(code, 1)  # `pub` has no licence text and no readme
            self.assertTrue(all(f.startswith("pub: ") for f in r["failures"]))
            self.assertEqual(r["publishable_members"], ["pub"])
            priv = next(m for m in r["members"] if m["name"] == "priv")
            self.assertTrue(any(x.startswith("R1") for x in priv["findings"]))


class LockUnchangedTests(unittest.TestCase):
    def test_only_the_scratch_crate_may_appear(self):
        old = {("a", "1.0.0", "registry+x"), ("b", "2.0.0", "")}
        self.assertEqual(lock_unchanged.compare(old, old | {("scratch", "0.0.0", "")}, {"scratch"}), [])
        self.assertTrue(lock_unchanged.compare(old, old | {("c", "1.0.0", "")}, {"scratch"}))  # unexpected package
        self.assertTrue(lock_unchanged.compare(old, {("a", "1.0.1", "registry+x"), ("b", "2.0.0", "")}, set()))  # bumped
        self.assertEqual(lock_unchanged.compare(old, {("b", "2.0.0", "")}, set()), [])  # pruned: fine


class PackageReportRuleTests(unittest.TestCase):
    def test_licence_texts_by_class(self):
        ok = package_report.licence_texts_ok
        self.assertTrue(ok(["LICENSE"], "MIT")[0])
        self.assertFalse(ok([], "MIT")[0])
        self.assertFalse(ok(["COPYING"], "LGPL-3.0-or-later")[0])  # needs the lesser text too
        self.assertFalse(ok(["COPYING.lesser"], "LGPL-3.0-or-later")[0])
        self.assertTrue(ok(["COPYING", "COPYING.lesser"], "LGPL-3.0-or-later")[0])
        self.assertTrue(ok(["COPYING"], "GPL-3.0-or-later")[0])
        self.assertFalse(ok(["COPYING.lesser"], "GPL-3.0-or-later")[0])
        self.assertTrue(ok(["COPYING", "COPYING.lesser"], "LGPL-3.0-or-later AND MPL-2.0")[0])

    def test_workspace_root_from_manifest_dir_is_found(self):
        src = 'let d = Path::new(env!("CARGO_MANIFEST_DIR"));\n    d.ancestors()\n        .nth(2)\n'
        self.assertTrue(package_report.ANCESTOR_RE.search(src))
        self.assertFalse(package_report.ANCESTOR_RE.search("let x = a.nth(2);"))


@unittest.skipUnless(shutil.which("cargo"), "cargo not installed")
class LicenceAuditWorkspaceTests(unittest.TestCase):
    """End-to-end D-042 checks on a synthetic path-only workspace."""

    POLICY = textwrap.dedent("""
        [third_party]
        allow = ["MIT"]
        [workspace]
        default = "LGPL-3.0-or-later"
        mpl_members = []
        gpl_members = ["geo-gpl"]
        [workspace.gpl_features]
        umb = ["cgal"]
        [headers]
        lgpl = ["LGPL-3.0-or-later"]
        mpl = ["LGPL-3.0-or-later", "MPL-2.0"]
        gpl = ["GPL-3.0-or-later"]
        tooling_globs = []
        """)

    def make(self, d: Path, umb_features: str, lgpl_extra_dep: str = "") -> None:
        (d / "Cargo.toml").write_text('[workspace]\nresolver = "3"\nmembers = ["crates/*"]\n')

        def crate(name: str, lic: str, deps: str, feats: str = "") -> None:
            c = d / "crates" / name
            (c / "src").mkdir(parents=True, exist_ok=True)
            spdx = "GPL-3.0-or-later" if lic.startswith("GPL") else "LGPL-3.0-or-later"
            (c / "src" / "lib.rs").write_text(f"// SPDX-License-Identifier: {spdx}\n")
            (c / "Cargo.toml").write_text(
                f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\nlicense = "{lic}"\n'
                f"publish = false\n\n[dependencies]\n{deps}\n{feats}")

        crate("geo", "LGPL-3.0-or-later", "")
        crate("geo-gpl", "GPL-3.0-or-later", 'geo = { path = "../geo" }')
        crate("igfem", "LGPL-3.0-or-later", 'geo = { path = "../geo" }\n' + lgpl_extra_dep)
        crate("umb", "LGPL-3.0-or-later",
              'geo = { path = "../geo" }\nigfem = { path = "../igfem", optional = true }\n'
              'geo-gpl = { path = "../geo-gpl", optional = true }', umb_features)
        (d / "policy.toml").write_text(self.POLICY)
        r = subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=d, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)

    def audit(self, d: Path) -> dict:
        cargo = licence_audit.Cargo("cargo", d, offline=True)
        policy = licence_audit.tomllib.loads((d / "policy.toml").read_text())
        return licence_audit.audit(d, policy, cargo)

    def test_opt_in_feature_passes(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ndefault = []\ncgal = ["dep:geo-gpl"]\nigfem = ["dep:igfem"]\n')
            r = self.audit(d)
            self.assertEqual(r["verdict"], "PASS", r["failures"])
            self.assertEqual(r["gpl_builds"], ["umb --features cgal"])

    def test_default_gpl_feature_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ndefault = ["cgal"]\ncgal = ["dep:geo-gpl"]\n')
            r = self.audit(d)
            self.assertEqual(r["verdict"], "FAIL")
            self.assertTrue(any("G1 default features" in f for f in r["failures"]), r["failures"])

    def test_unlisted_gpl_feature_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\nigfem = ["dep:igfem", "cgal"]\n')
            r = self.audit(d)
            self.assertTrue(any("G2 feature `igfem`" in f for f in r["failures"]), r["failures"])

    def test_unconditional_dependency_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n', 'geo-gpl = { path = "../geo-gpl" }')
            r = self.audit(d)
            self.assertTrue(any(f.startswith("igfem: G1") and "--no-default-features" in f
                                for f in r["failures"]), r["failures"])

    def test_wrong_header_and_licence_fail(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n')
            (d / "crates/geo/src/extra.rs").write_text("// SPDX-License-Identifier: GPL-3.0-or-later\n")
            text = (d / "crates/igfem/Cargo.toml").read_text().replace('"LGPL-3.0-or-later"', '"MIT"')
            (d / "crates/igfem/Cargo.toml").write_text(text)
            r = self.audit(d)
            self.assertTrue(any(f.startswith("geo: H1") for f in r["failures"]), r["failures"])
            self.assertTrue(any(f.startswith("igfem: W1") for f in r["failures"]), r["failures"])

    # --- the licence class applies to every source kind -----
    def failures_of(self, d: Path) -> list[str]:
        return self.audit(d)["failures"]

    def test_gpl_c_shim_in_lgpl_crate_fails(self):  # mutation R7
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n')
            (d / "crates/geo/csrc").mkdir()
            (d / "crates/geo/csrc/shim.c").write_text("/* SPDX-License-Identifier: GPL-3.0-or-later */\nint f(void) { return 0; }\n")
            self.assertTrue(any(f.startswith("geo: H3") for f in self.failures_of(d)), self.failures_of(d))

    def test_c_source_without_spdx_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n')
            (d / "crates/geo/src/shim.h").write_text("int f(void);\n")
            self.assertTrue(any(f.startswith("geo: H3") and "shim.h" in f for f in self.failures_of(d)), self.failures_of(d))

    def test_gpl_rs_outside_src_pulled_in_with_include_fails(self):  # mutation R8
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n')
            (d / "crates/geo/generated").mkdir()
            (d / "crates/geo/generated/aabb.rs").write_text("// SPDX-License-Identifier: GPL-3.0-or-later\npub fn g() {}\n")
            (d / "crates/geo/src/lib.rs").write_text(
                '// SPDX-License-Identifier: LGPL-3.0-or-later\ninclude!("../generated/aabb.rs");\n')
            f = self.failures_of(d)
            self.assertTrue(any(x.startswith("geo: H1") and "generated" in x for x in f), f)
            self.assertTrue(any(x.startswith("geo: G4") for x in f), f)

    def test_include_from_the_gpl_member_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n')
            (d / "crates/geo-gpl/src/algo.rs").write_text("// SPDX-License-Identifier: GPL-3.0-or-later\npub fn g() {}\n")
            (d / "crates/geo/src/lib.rs").write_text(
                '// SPDX-License-Identifier: LGPL-3.0-or-later\n#[path = "../../geo-gpl/src/algo.rs"]\nmod algo;\n')
            self.assertTrue(any(x.startswith("geo: G4") for x in self.failures_of(d)), self.failures_of(d))

    def test_allowed_gpl_feature_that_brings_nothing_fails(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = []\n')  # the policy allows `cgal`, but it no longer reaches the GPL member
            self.assertTrue(any("G3 policy allows feature `cgal`" in x for x in self.failures_of(d)), self.failures_of(d))

    def test_gpl_member_may_hold_gpl_files(self):
        with tempfile.TemporaryDirectory() as t:
            d = Path(t)
            self.make(d, '[features]\ncgal = ["dep:geo-gpl"]\n')
            (d / "crates/geo-gpl/src/shim.c").write_text("/* SPDX-License-Identifier: GPL-3.0-or-later */\nint f(void) { return 0; }\n")
            self.assertEqual(self.audit(d)["verdict"], "PASS", self.failures_of(d))


class VersionCheckTests(unittest.TestCase):
    META = {"packages": [{"name": "a", "version": "1.2.3"}, {"name": "b", "version": "1.2.3"}]}

    def test_tag_names_the_version(self):
        v, msgs, fails = version_check.check(self.META, "v1.2.3", None)
        self.assertEqual((v, fails), ("1.2.3", []))
        self.assertEqual(version_check.check(self.META, "1.2.3", None)[2], [])

    def test_wrong_tag_fails(self):
        self.assertEqual(len(version_check.check(self.META, "v1.2.4", None)[2]), 1)
        self.assertEqual(len(version_check.check(self.META, "release-1.2.3", None)[2]), 1)

    def test_no_tag_is_not_applicable(self):
        _, msgs, fails = version_check.check(self.META, None, None)
        self.assertEqual(fails, [])
        self.assertTrue(any("not applicable" in m for m in msgs))

    def test_mixed_versions_fail(self):
        meta = {"packages": [{"name": "a", "version": "1.0.0"}, {"name": "b", "version": "1.0.1"}]}
        self.assertEqual(len(version_check.check(meta, None, None)[2]), 1)

    def test_empty_workspace_fails(self):
        self.assertTrue(version_check.check({"packages": []}, None, None)[2])

    def test_pyproject_dynamic_version(self):
        with tempfile.TemporaryDirectory() as d:
            ok = Path(d) / "ok.toml"
            ok.write_text('[project]\nname = "x"\ndynamic = ["version"]\n\n[tool.x]\nversion = "9"\n')
            self.assertEqual(version_check.check(self.META, None, str(ok))[2], [])
            static = Path(d) / "static.toml"
            static.write_text('[project]\nname = "x"\nversion = "1.2.3"\n\n[tool.x]\n')
            self.assertEqual(len(version_check.check(self.META, None, str(static))[2]), 1)
            missing = Path(d) / "missing.toml"
            missing.write_text('[project]\nname = "x"\n\n[tool.x]\n')
            self.assertEqual(len(version_check.check(self.META, None, str(missing))[2]), 1)


class GateMapTests(unittest.TestCase):
    """gate_map.check on a synthetic repository."""

    WORKFLOW = textwrap.dedent(
        """\
        name: w
        on:
          pull_request:
            paths: ["crates/**"]
          push:
            branches: [main]
          schedule:
            - cron: "17 3 * * 1"
          workflow_dispatch:
        jobs:
          fast:
            steps:
              - run: ci/run-all.sh --preset fast
          medium:
            strategy:
              matrix:
                tier: [mpi, petsc]
            steps:
              - run: ci/run-all.sh --tiers ${{ matrix.tier }}
          gates:
            steps:
              - run: ci/run-all.sh --tiers gates,version
        """
    )
    RUN_ALL = '    fast) echo "lint test" ;;\n    medium) echo "mpi petsc" ;;\n'

    def repo(self, d: str, rows: str, gates: str = "cargo check --locked\n# c\n\ncargo test --locked\n") -> Path:
        r = Path(d)
        (r / "ci" / "github").mkdir(parents=True)
        (r / "ci" / "tiers").mkdir()
        for t in ("lint", "test", "mpi", "petsc", "gates", "version"):
            (r / "ci" / "tiers" / f"{t}.sh").write_text("")
        (r / "ci" / "run-all.sh").write_text(self.RUN_ALL)
        (r / "ci" / "tools").mkdir(exist_ok=True)
        (r / "ci" / "tools" / "feature-gates.txt").write_text(gates)
        (r / "ci" / "github" / "w.yml").write_text(self.WORKFLOW)
        (r / "ci" / "github" / "README.md").write_text(
            "x\n" + gate_map.BEGIN + "\n| Gate | Command | Job | Tiers | When |\n|---|---|---|---|---|\n" + rows + gate_map.END + "\n"
        )
        return r

    GOOD = (
        "| feature-gates.txt line 1 | `cargo check --locked` | `w.yml:gates` | gates | pr, merge |\n"
        "| feature-gates.txt line 2 | `cargo test --locked` | `w.yml:gates` | gates, version | pr, merge, schedule |\n"
        "| tier images | x | none: every job loads or builds its images | - | - |\n"
        "| tier fmt | x | `w.yml:fast` | lint | pr |\n"
        "| tier mpi | x | `w.yml:medium` | mpi | merge, schedule |\n"
        "| tier petsc | x | `w.yml:medium` | petsc | schedule |\n"
        "| tier pytest | x | `w.yml:gates` | gates | merge |\n"
        "| tier ranks-8 | x | none: the 8-rank runs are not part of the test job | - | - |\n"
    )

    def test_complete_map_passes(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(gate_map.check(self.repo(d, self.GOOD)), [])

    def test_tier_rows_are_optional(self):
        rows = "".join(r + "\n" for r in self.GOOD.splitlines() if r.startswith("| feature-gates.txt"))
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(gate_map.check(self.repo(d, rows)), [])

    def test_pre_merge_tier_row_must_claim_pr(self):
        rows = self.GOOD.replace("| tier fmt | x | `w.yml:fast` | lint | pr |", "| tier fmt | x | `w.yml:fast` | lint | merge |")
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, rows))
            self.assertTrue(any("pre-merge gate" in f and "tier fmt" in f for f in fails), fails)

    def test_missing_feature_gate_row_fails(self):
        with tempfile.TemporaryDirectory() as d:
            rows = self.GOOD.replace("feature-gates.txt line 2", "feature-gates.txt line 3")
            fails = gate_map.check(self.repo(d, rows))
            self.assertTrue(any("'feature-gates.txt line 2'" in f for f in fails), fails)

    def test_a_new_gate_line_without_a_row_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD, gates="cargo check --locked\ncargo test --locked\ncargo doc --locked\n"))
            self.assertTrue(any("line 3" in f for f in fails), fails)

    def test_changed_command_text_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD, gates="cargo check --locked --release\ncargo test --locked\n"))
            self.assertTrue(any("does not quote the command" in f for f in fails), fails)

    def test_job_that_does_not_exist_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("w.yml:fast", "w.yml:nosuch")))
            self.assertTrue(any("no job 'nosuch'" in f for f in fails), fails)

    def test_job_that_does_not_run_the_tier_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("| `w.yml:fast` | lint | pr |", "| `w.yml:fast` | mpi | pr |")))
            self.assertTrue(any("do not run tier 'mpi'" in f for f in fails), fails)

    def test_reason_must_be_stated(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("none: every job loads or builds its images", "none: no")))
            self.assertTrue(any("stated reason" in f for f in fails), fails)

    def test_unknown_tier_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("| gates | pr, merge |\n| feature-gates.txt line 2", "| nosuchtier | pr, merge |\n| feature-gates.txt line 2")))
            self.assertTrue(any("not a tier of ci/tiers" in f for f in fails), fails)

    def test_tier_never_run_by_a_workflow_is_reported(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.repo(d, self.GOOD)
            (r / "ci" / "tiers" / "orphan.sh").write_text("")
            fails = gate_map.check(r)
            self.assertTrue(any("orphan" in f for f in fails), fails)

    # --- the map says WHEN a job runs and the checker reads it
    def test_row_with_a_trigger_the_workflow_lacks_fails(self):
        # planted change: the workflow is only dispatched by hand
        with tempfile.TemporaryDirectory() as d:
            r = self.repo(d, self.GOOD)
            wf = r / "ci" / "github" / "w.yml"
            text = wf.read_text()
            head, tail = text.split("jobs:\n", 1)
            wf.write_text("name: w\non:\n  workflow_dispatch:\njobs:\n" + tail)
            self.assertNotEqual(head, "")
            fails = gate_map.check(r)
            self.assertTrue(any("has no such trigger" in f for f in fails), fails)

    def test_dropping_pull_request_from_a_gate_workflow_fails(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.repo(d, self.GOOD)
            wf = r / "ci" / "github" / "w.yml"
            wf.write_text(wf.read_text().replace('  pull_request:\n    paths: ["crates/**"]\n', ""))
            fails = gate_map.check(r)
            self.assertTrue(any("says it runs on pr" in f for f in fails), fails)

    def test_push_of_tags_only_is_not_a_merge_trigger(self):
        triggers = gate_map.workflow_triggers('name: w\non:\n  push:\n    tags: ["v*"]\njobs:\n  a:\n    steps: []\n')
        self.assertEqual(triggers, set())
        triggers = gate_map.workflow_triggers("name: w\non:\n  push:\n    branches: [main]\n  pull_request:\njobs:\n")
        self.assertEqual(triggers, {"pr", "merge"})

    def test_pre_merge_row_must_claim_pr(self):
        # the checks of every pull request (fmt, clippy, test, fixtures, fast) may not be presented as later jobs
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("| lint | pr |", "| lint | merge |")))
            self.assertTrue(any("pre-merge gate" in f and "fmt" in f for f in fails), fails)

    def test_feature_gate_row_must_claim_pr_or_schedule(self):
        # a feature gate runs on pull requests or weekly; one that only runs on a merge (or by hand) fails
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("| gates | pr, merge |", "| gates | merge |")))
            self.assertTrue(any("feature gate" in f and "line 1" in f for f in fails), fails)
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(gate_map.check(self.repo(d, self.GOOD.replace("| gates | pr, merge |", "| gates | schedule |"))), [])

    def test_unknown_when_value_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("| mpi | merge, schedule |", "| mpi | nightly |")))
            self.assertTrue(any("when 'nightly'" in f for f in fails), fails)

    def test_row_without_a_job_cannot_claim_a_trigger(self):
        with tempfile.TemporaryDirectory() as d:
            fails = gate_map.check(self.repo(d, self.GOOD.replace("images | x | none: every job loads or builds its images | - | - |", "images | x | none: every job loads or builds its images | - | pr |")))
            self.assertTrue(any("cannot say when it runs" in f for f in fails), fails)

    def test_tier_step_gated_off_with_if_fails(self):
        # planted change: `if: false` on the step that runs the tier
        with tempfile.TemporaryDirectory() as d:
            r = self.repo(d, self.GOOD)
            wf = r / "ci" / "github" / "w.yml"
            wf.write_text(wf.read_text().replace("      - run: ci/run-all.sh --tiers ${{ matrix.tier }}", "      - if: false\n        run: ci/run-all.sh --tiers ${{ matrix.tier }}"))
            fails = gate_map.check(r)
            self.assertTrue(any("tier step has an 'if:'" in f for f in fails), fails)

    def test_job_gated_off_with_if_false_fails(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.repo(d, self.GOOD)
            wf = r / "ci" / "github" / "w.yml"
            wf.write_text(wf.read_text().replace("  gates:\n", "  gates:\n    if: false\n"))
            fails = gate_map.check(r)
            self.assertTrue(any("job-level 'if: false'" in f for f in fails), fails)

    def test_a_row_may_name_several_jobs(self):
        with tempfile.TemporaryDirectory() as d:
            rows = self.GOOD.replace("| `w.yml:fast` | lint | pr |", "| `w.yml:fast, w.yml:gates` | lint, version | pr |")
            self.assertEqual(gate_map.check(self.repo(d, rows)), [])
        with tempfile.TemporaryDirectory() as d:
            rows = self.GOOD.replace("| `w.yml:fast` | lint | pr |", "| `w.yml:fast, w.yml:medium` | lint, version | pr |")
            fails = gate_map.check(self.repo(d, rows))
            self.assertTrue(any("do not run tier 'version'" in f for f in fails), fails)

    def test_real_repository_map_is_complete(self):
        repo = HERE.parents[1]
        self.assertEqual(gate_map.check(repo), [])


class AnalyzeTestsTests(unittest.TestCase):
    """analyze_tests.py on synthetic job directories (no cargo run)."""

    WS_LOG = textwrap.dedent(
        """\
             Running tests/a.rs (/target/debug/deps/a-0f)

        running 2 tests
        test known_one ... FAILED
        test fine ... ok

        failures:

        failures:
            known_one

        test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

        error: test failed, to rerun pass `-p p --test a`
             Running tests/b.rs (/target/debug/deps/b-0f)

        running 2 tests
        error: test failed, to rerun pass `-p p --test b`

        Caused by:
          process didn't exit successfully: `/target/debug/deps/b-0f` (signal: 9, SIGKILL: kill)
             Running tests/c.rs (/target/debug/deps/c-0f)

        running 2 tests
           Doc-tests p

        running 0 tests

        test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

        error: 2 targets failed:
            `-p p --test a`
            `-p p --test b`
        """
    )
    ONLY_A_LOG = textwrap.dedent(
        """\
             Running tests/a.rs (/target/debug/deps/a-0f)

        running 2 tests
        test known_one ... FAILED
        test fine ... ok

        failures:

        failures:
            known_one

        test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

        error: test failed, to rerun pass `-p p --test a`
           Doc-tests p

        running 0 tests

        test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

        error: 1 target failed:
            `-p p --test a`
        """
    )
    PASS_A_LOG = textwrap.dedent(
        """\
             Running tests/a.rs (/target/debug/deps/a-0f)

        running 2 tests
        test known_one ... ok
        test fine ... ok

        test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

           Doc-tests p

        running 0 tests

        test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

        """
    )
    FEATURE_OK = textwrap.dedent(
        """\
             Running tests/g.rs (/target/debug/deps/g-0f)

        running 1 test
        test only_with_x ... ok

        test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

        """
    )
    DOC_Q = textwrap.dedent(
        """\
           Doc-tests q

        running 0 tests

        test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

        """
    )
    FEATURE_FAIL = textwrap.dedent(
        """\
             Running tests/g.rs (/target/debug/deps/g-0f)

        running 1 test
        test only_with_x ... FAILED

        failures:

        failures:
            only_with_x

        test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

        error: test failed, to rerun pass `-p q --test g`
        """
    )

    @staticmethod
    def artifact(pkg: str, name: str) -> str:
        return json.dumps(
            {
                "reason": "compiler-artifact",
                "manifest_path": f"/src/{pkg}/Cargo.toml",
                "target": {"name": name, "kind": ["test"]},
                "profile": {"test": True},
                "executable": f"/target/debug/deps/{name}-0f",
            }
        )

    @staticmethod
    def metadata(*pkgs: str) -> str:
        return json.dumps(
            {
                "packages": [
                    {"name": p, "manifest_path": f"/src/{p}/Cargo.toml", "targets": [{"name": p, "kind": ["lib"], "doctest": True}]}
                    for p in pkgs
                ]
            }
        )

    def run_analyser(self, job: Path, *extra: str) -> tuple[int, str]:
        p = subprocess.run(
            [sys.executable, str(HERE / "analyze_tests.py"), "--job-dir", str(job), "--repo", str(job / "src"), "--known-failures", str(job / "known.txt"), *extra],
            capture_output=True,
            text=True,
            check=False,
        )
        return p.returncode, p.stdout + p.stderr

    def workspace_job(self, d: str) -> Path:
        j = Path(d)
        (j / "src" / "p").mkdir(parents=True)
        (j / "src" / "p" / "Cargo.toml").write_text('[package]\nname = "p"\n')
        (j / "metadata.json").write_text(self.metadata("p") + "\n")
        (j / "test-artifacts.jsonl").write_text("".join(self.artifact("p", t) + "\n" for t in "abc"))
        (j / "summary.tsv").write_text("test\t101\t9\n")
        (j / "checks.txt").write_text("")
        (j / "known.txt").write_text("p | --test a | known_one | issue-0\n")
        (j / "test.log").write_text(self.WS_LOG)
        return j

    def test_killed_and_early_exit_binaries_fail_and_are_named(self):
        with tempfile.TemporaryDirectory() as d:
            rc, out = self.run_analyser(self.workspace_job(d), "--gate")
            self.assertEqual(rc, 1, out)
            self.assertTrue(out.strip().splitlines()[-1].startswith("GATE: FAIL"), out)
            self.assertRegex(out, r"p \| --test b: no .test result. line.*signal: 9")
            self.assertRegex(out, r"p \| --test c: no .test result. line")
            self.assertRegex(out, r"p \| --test c: failures found in its output but cargo did not list it")

    def test_failures_equal_to_the_baseline_pass(self):
        with tempfile.TemporaryDirectory() as d:
            j = self.workspace_job(d)
            (j / "test.log").write_text(self.ONLY_A_LOG)
            (j / "test-artifacts.jsonl").write_text(self.artifact("p", "a") + "\n")
            rc, out = self.run_analyser(j, "--gate")
            self.assertEqual(rc, 0, out)
            self.assertEqual(out.strip().splitlines()[-1], "GATE: PASS (1 known failure)", out)

    def test_relative_executable_paths_are_matched(self):
        # a host run with the default target/ under the working directory
        # (e.g. macOS) prints the executables relative to it
        with tempfile.TemporaryDirectory() as d:
            j = self.workspace_job(d)
            (j / "test.log").write_text(self.ONLY_A_LOG.replace("(/target/", "(target/"))
            (j / "test-artifacts.jsonl").write_text(self.artifact("p", "a") + "\n")
            self.assertIn("Running tests/a.rs (target/", (j / "test.log").read_text())
            rc, out = self.run_analyser(j, "--gate")
            self.assertEqual(rc, 0, out)
            self.assertEqual(out.strip().splitlines()[-1], "GATE: PASS (1 known failure)", out)

    def test_known_failure_that_passes_is_improved(self):
        with tempfile.TemporaryDirectory() as d:
            j = self.workspace_job(d)
            (j / "test.log").write_text(self.PASS_A_LOG)
            (j / "test-artifacts.jsonl").write_text(self.artifact("p", "a") + "\n")
            (j / "summary.tsv").write_text("test\t0\t9\n")
            rc, out = self.run_analyser(j, "--gate")
            self.assertEqual(rc, 0, out)
            self.assertTrue(out.strip().splitlines()[-1].startswith("GATE: IMPROVED"), out)
            (j / "checks.txt").write_text("GATEFAIL a synthetic gate check\n")
            rc, out = self.run_analyser(j, "--gate")
            self.assertTrue(out.strip().splitlines()[-1].startswith("GATE: FAIL"), out)
            rc, out = self.run_analyser(j)
            self.assertTrue(out.strip().splitlines()[-1].startswith("RESULT: IMPROVED"), out)

    def feature_job(self, d: str) -> Path:
        f = Path(d)
        for p in "pq":
            (f / "src" / p).mkdir(parents=True)
            (f / "src" / p / "Cargo.toml").write_text(f'[package]\nname = "{p}"\n')
        (f / "metadata.json").write_text(self.metadata("p", "q") + "\n")
        (f / "feature-2-artifacts.jsonl").write_text(self.artifact("q", "g") + "\n")
        (f / "features.tsv").write_text("1\tcmd\tcargo clippy -p q --features x --locked -- -D warnings\n2\ttest\tcargo test -p q --features x --locked --no-fail-fast\n")
        (f / "summary.tsv").write_text("metadata\t0\t1\nfeature-1\t0\t5\nfeature-2-build\t0\t5\nfeature-2\t0\t5\n")
        (f / "checks.txt").write_text("")
        (f / "known.txt").write_text("")
        (f / "feature-2.log").write_text(self.FEATURE_OK + self.DOC_Q)
        return f

    def test_feature_runs(self):
        with tempfile.TemporaryDirectory() as d:
            f = self.feature_job(d)
            rc, out = self.run_analyser(f, "--gate")
            self.assertEqual(out.strip().splitlines()[-1], "GATE: PASS (0 known failures)", out)
            self.assertRegex(out, r"(?m)^feature-2: result_lines=2 passed=1")
            # a failing feature-gated test and a failing command are named
            (f / "feature-2.log").write_text(self.FEATURE_FAIL + self.DOC_Q + "error: 1 target failed:\n    `-p q --test g`\n")
            (f / "summary.tsv").write_text("metadata\t0\t1\nfeature-1\t101\t5\nfeature-2-build\t0\t5\nfeature-2\t101\t5\n")
            rc, out = self.run_analyser(f, "--gate")
            self.assertEqual(rc, 1, out)
            self.assertEqual(out.strip().splitlines()[-1], "GATE: FAIL (2 problems)", out)
            self.assertIn("new failure: [feature-2] q | --test g | only_with_x", out)
            self.assertRegex(out, r"step feature-1 exited 101 \(cargo clippy -p q --features x")
            # the baseline waives a known failure in a feature run
            (f / "known.txt").write_text("q | --test g | only_with_x | issue-0\n")
            (f / "summary.tsv").write_text("metadata\t0\t1\nfeature-1\t0\t5\nfeature-2-build\t0\t5\nfeature-2\t101\t5\n")
            rc, out = self.run_analyser(f, "--gate")
            self.assertEqual(out.strip().splitlines()[-1], "GATE: PASS (1 known failure)", out)
            # a listed feature step that never ran fails
            (f / "summary.tsv").write_text("metadata\t0\t1\nfeature-1\t0\t5\n")
            rc, out = self.run_analyser(f, "--gate")
            self.assertRegex(out, r"step feature-2 never ran \(cargo test -p q")

    def test_feature_line_selecting_targets_expects_no_doc_tests(self):
        with tempfile.TemporaryDirectory() as d:
            f = self.feature_job(d)
            (f / "feature-2.log").write_text(self.FEATURE_OK)
            (f / "features.tsv").write_text("1\tcmd\tcargo clippy -p q --features x --locked -- -D warnings\n2\ttest\tcargo test --release -p q --locked --no-fail-fast --test g\n")
            rc, out = self.run_analyser(f, "--gate")
            self.assertEqual(out.strip().splitlines()[-1], "GATE: PASS (0 known failures)", out)
            # a whole-package line without its doc-tests still fails
            (f / "features.tsv").write_text("1\tcmd\tcargo clippy -p q --features x --locked -- -D warnings\n2\ttest\tcargo test -p q --features x --locked --no-fail-fast\n")
            rc, out = self.run_analyser(f, "--gate")
            self.assertRegex(out, r"failed target: \[feature-2\] q \| --doc: doc-tests never ran")


class PytestSkipsTests(unittest.TestCase):
    """ci/tools/pytest_skips.py: a new skip fails."""

    LOG = "SKIPPED [2] tests/test_a.py:10: needs greenlet\nSKIPPED [1] tests/test_b.py:5: needs gmsh\n== 10 passed, 3 skipped in 1.0s ==\n"
    EXP = "# c\ngreenlet | tests/test_a.py | needs greenlet | 2\ngreenlet | tests/test_b.py | needs gmsh | 1\nother | tests/x.py | y | 4\n"

    def run_tool(self, log, exp, run="greenlet"):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "log").write_text(log)
            (Path(d) / "exp").write_text(exp)
            return pytest_skips.main([str(Path(d) / "log"), "--expected", str(Path(d) / "exp"), "--run", run])

    def test_expected_skips_pass(self):
        self.assertEqual(self.run_tool(self.LOG, self.EXP), 0)

    def test_a_new_skip_fails(self):
        self.assertEqual(self.run_tool(self.LOG + "SKIPPED [1] tests/test_c.py:1: new\n", self.EXP), 1)

    def test_more_skips_than_expected_fail(self):
        self.assertEqual(self.run_tool(self.LOG.replace("[2]", "[3]"), self.EXP), 1)

    def test_fewer_skips_pass(self):
        self.assertEqual(self.run_tool("== 12 passed, 1 skipped in 1.0s ==\nSKIPPED [1] tests/test_b.py:5: needs gmsh\n", self.EXP), 0)

    def test_version_numbers_of_an_environment_skip_are_not_part_of_the_key(self):
        log = "SKIPPED [1] tests/e.py:1: eigen depends on ['scipy']: here ['1.99.0'], in the oracle runs ['1.15.3']\n== 1 skipped in 1.0s ==\n"
        exp = "greenlet | tests/e.py | eigen depends on ['scipy']: here ['1.18.1'], in the oracle runs ['1.15.3'] | 1\n"
        self.assertEqual(self.run_tool(log, exp), 0)

    def test_log_without_pytest_summary_is_a_usage_error(self):
        self.assertEqual(self.run_tool("nothing\n", self.EXP), 2)

    # A line `NAME@KEY` applies on the platform KEY only.
    PLAT_LOG = ("SKIPPED [1] tests/e.py:1: dynamics depends on ['platform', 'numpy_avx512f']: "
                "here ['arm64-darwin', False], in the oracle runs ['x86_64-linux-glibc', True]\n== 1 skipped in 1.0s ==\n")
    PLAT_EXP = "greenlet@arm64-darwin | tests/e.py | dynamics depends on ['platform', 'numpy_avx512f']: | 1\n"

    def run_plat(self, log, exp, plat, run="greenlet"):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "log").write_text(log)
            (Path(d) / "exp").write_text(exp)
            return pytest_skips.main([str(Path(d) / "log"), "--expected", str(Path(d) / "exp"),
                                      "--run", run, "--platform", plat])

    def test_a_platform_line_applies_on_its_platform(self):
        self.assertEqual(self.run_plat(self.PLAT_LOG, self.PLAT_EXP, "arm64-darwin"), 0)

    def test_a_platform_line_is_not_accepted_on_another_platform(self):
        self.assertEqual(self.run_plat(self.PLAT_LOG, self.PLAT_EXP, "x86_64-linux-glibc"), 1)

    def test_a_platform_line_of_another_run_does_not_apply(self):
        self.assertEqual(self.run_plat(self.PLAT_LOG, self.PLAT_EXP, "arm64-darwin", run="no-greenlet"), 1)

    def test_the_default_platform_is_the_interpreters(self):
        key = pytest_skips.platform_key()
        exp = self.PLAT_EXP.replace("arm64-darwin |", f"{key} |")
        self.assertEqual(self.run_tool(self.PLAT_LOG, exp), 0)

    def test_the_repository_skip_file_parses_for_both_runs_and_platforms(self):
        text = (HERE.parent / "pytest-skips.txt").read_text()
        for run in ("greenlet", "no-greenlet"):
            linux = pytest_skips.parse_expected(text, run, "x86_64-linux-glibc")
            mac = pytest_skips.parse_expected(text, run, "arm64-darwin")
            self.assertTrue(linux and mac)
            # the macOS-only entries are not accepted on Linux, and vice versa
            self.assertNotEqual(linux, mac)


if __name__ == "__main__":
    unittest.main()
