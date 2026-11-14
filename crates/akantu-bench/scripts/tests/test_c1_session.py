# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the C1 session's rules
# (crates/akantu-bench/scripts/c1_session.py): order, the
# 64 -> 32 fall-back after an out-of-memory kill, the UD-216 repetition rule,
# the budget skip, the gate, the test step's fail-fast and limit checks. The
# campaign tool and Docker are replaced by a fake runner and a fake clock.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import hashlib
import io
import json
import os
import sys
import subprocess
import tarfile
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import c1_session  # noqa: E402


class Fake:
    """Fake runner: writes the campaign tool's outputs from a scenario."""

    def __init__(self, session, outcomes=None, hours=None, test=None, limits=True, records_settings=True):
        self.s = session
        self.outcomes = outcomes or {}   # "rust-np64-a1" -> outcome
        self.hours = hours or {}         # "rust@64" -> measured hours
        self.test = test or {}           # "rust" -> outcome of the test run
        self.limits = limits
        self.records_settings = records_settings   # False: a driver without the load fix's keys
        self.commands = []
        self.campaigns = []
        self.injected = []
        self.now = 1_000_000.0

    def clock(self):
        return self.now

    def __call__(self, cmd, logfile, env=None):
        if cmd[:2] == ["docker", "run"]:
            self.injected.append(cmd[cmd.index("--name") + 1])
            return 0
        if cmd[0] == "docker":
            return 0
        if cmd[0] == "bash" and cmd[1].endswith("c1_images.sh"):
            tool = self.s.tool()
            tool.parent.mkdir(parents=True, exist_ok=True)
            tool.write_text("")
            (self.s.res / "smoke/repo/benchmarks/cohesive-perf/scripts").mkdir(parents=True, exist_ok=True)
            self.now += 0.1 * 3600
            return 0
        if cmd[0] == "bash" and cmd[1] == "-c":   # perf_volume_write of the mesh
            return 0
        assert cmd[1].endswith("c1prep_campaign.sh"), cmd
        args = cmd[2:]
        opts = {}
        while args[0].startswith("--") and args[0] != "--export-partition":
            opts[args[0]] = args[1]
            args = args[2:]
        out = self.s.campaigns
        if args[0] == "--export-partition":
            name, _, np = args[1:]
            self.campaigns.append(name)
            (out / name).mkdir(parents=True, exist_ok=True)
            (out / name / f"partition-np{np}.csv").write_text("type, element, partition\n")
            self.now += 0.02 * 3600
            return 0
        name, _, steps, np, _, cases = args[:6]
        self.campaigns.append(name)
        self.commands.append(cmd)
        rc = 0
        for c in cases.split():
            d = out / name / c / f"np{np}" / "rep1"
            d.mkdir(parents=True, exist_ok=True)
            if name.startswith("test"):
                o = self.test.get(c, "ok")
                self.now += 0.1 * 3600
            else:
                o = self.outcomes.get(name, "ok")
                if f"c1s-inject-{name}" in self.injected:
                    o = "ud216"
                if opts["--memory"] == self.s.a.oom_memory:
                    o = "oom"
                self.now += self.hours.get(f"{c}@{np}", self.s.a.est.get(f"{c}@{np}", 0.1)) * 3600
            mem = str(c1_session.mem_bytes(opts["--memory"]))
            st = {"case": c, "np": int(np), "outcome": o, "exit": 0 if o == "ok" else 1,
                  "memory_peak_bytes": 2 * 1024 ** 3, "memory_max": mem if self.limits else "max"}
            if c == "rust":
                st["cpu_max"] = f"{int(opts['--cpus']) * 100000} 100000"
                # the driver writes the user-section settings it was given into summary.json
                summ = {"rust_cohesive_compat": "upstream", "rust_doubled_node_load": "compat"}
                if self.records_settings is True or (isinstance(self.records_settings, set)
                                                      and name not in self.records_settings):
                    summ.update(dict(kv.split("=", 1) for kv in opts.get("--rust-extra", "").split()))
                (d / "summary.json").write_text(json.dumps(summ))
            (d / "status.txt").write_text("".join(f"{k}={json.dumps(v)}\n" for k, v in st.items()))
            rc |= o != "ok"
            if name.startswith("test") and int(np) > 1:
                (out / name / f"partition-np{np}.csv").write_text("type, element, partition\n")
        return rc


class SessionRules(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        self.dir = t / "tarballs"
        self.dir.mkdir()
        for f in ("SHA256SUMS", "images.txt", "akantu-rs-c1-repo.tar.gz"):
            (self.dir / f).write_text("")
        msh = b"$MeshFormat\n4.1 0 8\n$EndMeshFormat\n"
        meta = json.dumps({"sha256": hashlib.sha256(msh).hexdigest(), "nodes": 4,
                           "elements_total": 2, "elements_by_type": {"tetrahedron4": 1}}).encode()
        with tarfile.open(self.dir / "akantu-c1-mesh-joss.tar.gz", "w:gz") as tf:
            for name, data in (("cube-joss.msh", msh), ("cube-joss.json", meta)):
                ti = tarfile.TarInfo(name)
                ti.size = len(data)
                tf.addfile(ti, io.BytesIO(data))
        self.results = t / "results"

    def tearDown(self):
        self.tmp.cleanup()

    def session(self, *extra, budget="14", **fake):
        a = c1_session.parse_args([str(self.dir), "480g", "64", str(self.results), budget, *extra])
        s = c1_session.Session(a)
        f = Fake(s, **fake)
        s.runner = f
        s.clock = f.clock
        with redirect_stdout(io.StringIO()):
            rc = s.run()
        return s, f, rc

    def test_order(self):
        s, f, rc = self.session()
        self.assertEqual(rc, 0)
        self.assertEqual(f.campaigns, ["test-np64", "export-np32", "export-np8", "rust-np64-a1", "cpp-np64-a1",
                                       "rust-np32-a1", "cpp-np32-a1", "rust-np8-a1", "cpp-np8-a1"])
        self.assertEqual(s.state["status"], "finished")
        self.assertTrue(all((s.campaigns / c / "DONE").exists() for c in f.campaigns))
        self.assertEqual([r["outcome"] for r in s.state["points"]], ["ok"] * 6)

    def test_oom_at_64_falls_back_to_32_at_once(self):
        s, f, rc = self.session(outcomes={"rust-np64-a1": "oom"})
        self.assertEqual(f.campaigns[3:], ["rust-np64-a1", "rust-np32-a1", "cpp-np64-a1", "cpp-np32-a1",
                                           "rust-np8-a1", "cpp-np8-a1"])
        slots = [(r["slot"], r["np"], r["outcome"]) for r in s.state["points"]]
        self.assertEqual(slots[:2], [("rust@64", 64, "oom"), ("rust@32", 32, "ok")])
        self.assertEqual(sum(1 for c in f.campaigns if c.startswith("rust-np64")), 1)  # no second attempt

    def test_forced_oom_hook(self):
        s, f, rc = self.session("--force-oom", "cpp@64")
        self.assertEqual(f.campaigns[3:6], ["rust-np64-a1", "cpp-np64-a1", "cpp-np32-a1"])
        self.assertEqual(s.state["points"][1]["memory"], "400m")

    def test_oom_in_test_run_skips_64(self):
        s, f, rc = self.session(test={"cpp": "oom"})
        self.assertEqual(f.campaigns[3:], ["rust-np64-a1", "cpp-np32-a1", "rust-np32-a1", "rust-np8-a1",
                                           "cpp-np8-a1"])
        self.assertEqual([(r["slot"], r["attempt"], r["outcome"]) for r in s.state["points"][1:3]],
                         [("cpp@64", 0, "oom"), ("cpp@32", 1, "ok")])

    def test_ud216_repeated_once(self):
        s, f, rc = self.session(outcomes={"cpp-np32-a1": "ud216"})
        self.assertIn("cpp-np32-a2", f.campaigns)
        self.assertEqual([r["outcome"] for r in s.state["points"] if r["slot"] == "cpp@32"], ["ud216-retry", "ok"])

    def test_ud216_twice_drops_the_point(self):
        s, f, rc = self.session("--force-retry", "cpp@32:2")
        self.assertEqual([c for c in f.campaigns if c.startswith("cpp-np32")], ["cpp-np32-a1", "cpp-np32-a2"])
        self.assertEqual([r["outcome"] for r in s.state["points"] if r["slot"] == "cpp@32"],
                         ["ud216-retry", "dropped"])
        self.assertEqual(len(f.injected), 2)
        self.assertIn("cpp-np8-a1", f.campaigns)

    def test_budget_skips_cpp_last(self):
        # 0.1 + 0.1·2 (test) + 0.04 + runs 0.64 + 0.74 + 0.79 + 1.17 + 1.63 = 5.31 h; cpp@8 needs 2.77
        s, f, rc = self.session(budget="7.5")
        self.assertNotIn("cpp-np8-a1", f.campaigns)
        self.assertEqual(s.state["points"][-1]["outcome"], "skipped")
        self.assertEqual(s.state["status"], "finished")

    def test_gate_stops_when_projection_exceeds_budget(self):
        s, f, rc = self.session(budget="5", hours={"rust@64": 1.6, "cpp@64": 1.8})
        self.assertEqual(f.campaigns[-1], "cpp-np64-a1")
        self.assertEqual(s.state["status"], "stopped by the gate")
        self.assertEqual(rc, 0)

    def test_test_failure_stops_the_session(self):
        s, f, rc = self.session(test={"rust": "failed"})
        self.assertEqual(f.campaigns, ["test-np64"])
        self.assertTrue(s.state["status"].startswith("failed: test"))
        self.assertEqual(rc, 1)

    def test_limits_mismatch_stops_the_session(self):
        s, f, rc = self.session(limits=False)
        self.assertEqual(f.campaigns, ["test-np64"])
        self.assertIn("container limits", s.state["status"])

    def test_resume_skips_finished_work(self):
        s, f, rc = self.session(budget="5", hours={"rust@64": 1.6, "cpp@64": 1.8})
        self.assertEqual(s.state["status"], "stopped by the gate")
        a = c1_session.parse_args([str(self.dir), "480g", "64", str(self.results), "30", "--resume"])
        s2 = c1_session.Session(a)
        f2 = Fake(s2)
        f2.now = f.now
        s2.runner, s2.clock = f2, f2.clock
        with redirect_stdout(io.StringIO()):
            s2.run()
        self.assertEqual(f2.campaigns, ["rust-np32-a1", "cpp-np32-a1", "rust-np8-a1", "cpp-np8-a1"])

    # ----: the full-size rerun's settings (D-105 / D-108) ----------------------
    SETTINGS = "rust_cohesive_compat=upstream rust_doubled_node_load=follows_elements"

    def extra_of(self, cmd):
        return cmd[cmd.index("--rust-extra") + 1] if "--rust-extra" in cmd else None

    def test_rust_runs_carry_the_settings(self):
        s, f, rc = self.session()
        self.assertEqual(rc, 0)
        for cmd in f.commands:
            cases = cmd[-1].split()
            if "rust" in cases:
                self.assertEqual(self.extra_of(cmd), self.SETTINGS, cmd)
            else:
                self.assertIsNone(self.extra_of(cmd), cmd)   # never on the C++ runs
        self.assertEqual(s.state["rust_settings"], self.SETTINGS.split())

    def test_refused_when_the_test_run_does_not_record_them(self):
        s, f, rc = self.session(records_settings=False)
        self.assertEqual(rc, 1)
        self.assertEqual(f.campaigns, ["test-np64"])
        self.assertIn("did not record --rust-settings (refused)", s.state["status"])
        self.assertIn("rust_doubled_node_load = 'compat'", s.state["status"])

    def test_refused_when_a_run_does_not_record_them(self):
        s, f, rc = self.session(records_settings={"rust-np32-a1"})
        self.assertEqual(rc, 1)
        self.assertEqual(f.campaigns[-1], "rust-np32-a1")          # nothing after it
        self.assertEqual([r["outcome"] for r in s.state["points"]][-1], "refused")
        self.assertIn("refused", s.state["status"])

    def test_none_keeps_c1_behaviour(self):
        s, f, rc = self.session("--rust-settings", "none", records_settings=False)
        self.assertEqual(rc, 0)
        self.assertTrue(all(self.extra_of(c) is None for c in f.commands))
        self.assertEqual(f.campaigns, ["test-np64", "export-np32", "export-np8", "rust-np64-a1", "cpp-np64-a1",
                                       "rust-np32-a1", "cpp-np32-a1", "rust-np8-a1", "cpp-np8-a1"])

    def test_rerun_port_only_two_ranks_given_partitions(self):
        parts = Path(self.tmp.name) / "parts"
        parts.mkdir()
        for r in (64, 32):
            (parts / f"partition-np{r}.csv").write_text(f"type, element, partition\n_tetrahedron_4, 0, {r - 1}\n")
        s, f, rc = self.session("--codes", "rust", "--ranks", "64 32", "--partitions", str(parts))
        self.assertEqual(rc, 0, s.state.get("status"))
        self.assertEqual(f.campaigns, ["test-np64", "rust-np64-a1", "rust-np32-a1"])   # no export, no C++
        for cmd in f.commands:
            self.assertEqual(cmd[cmd.index("--partitions") + 1], str(s.partdir))
            self.assertEqual(self.extra_of(cmd), self.SETTINGS)
        self.assertEqual(sorted(s.state["partitions_sha256"]), ["partition-np32.csv", "partition-np64.csv"])
        self.assertEqual([(r["slot"], r["outcome"]) for r in s.state["points"]], [("rust@64", "ok"), ("rust@32", "ok")])
        with self.assertRaises(SystemExit):
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                c1_session.parse_args([str(self.dir), "480g", "64", str(self.results), "6", "--ranks", "64"])

    def test_percopy_full_size_check_shape(self):
        # The per-copy check of the loads at full size, port only, 64 and 32 ranks on given
        # partitions, a check-only run to step 3,200; the session refuses a run whose summary.json does
        # not record "rust_load_check_every": "1" (the driver writes the key as written)
        parts = Path(self.tmp.name) / "parts"
        parts.mkdir()
        for r in (64, 32):
            (parts / f"partition-np{r}.csv").write_text(f"type, element, partition\n_tetrahedron_4, 0, {r - 1}\n")
        settings = self.SETTINGS + " rust_load_check_every=1"
        s, f, rc = self.session("--codes", "rust", "--ranks", "64 32", "--partitions", str(parts),
                                "--steps", "3200", "--rust-settings", settings)
        self.assertEqual(rc, 0, s.state.get("status"))
        self.assertEqual(f.campaigns, ["test-np64", "rust-np64-a1", "rust-np32-a1"])
        for cmd in f.commands:
            self.assertEqual(self.extra_of(cmd), settings)
        # campaign arguments: NAME MESH STEPS RANKS REPS CASES
        self.assertEqual([(c[-6], c[-4], c[-3]) for c in f.commands],
                         [("test-np64", "20", "64"), ("rust-np64-a1", "3200", "64"), ("rust-np32-a1", "3200", "32")])
        self.results = Path(self.tmp.name) / "results-unrecorded"
        s2, f2, rc2 = self.session("--codes", "rust", "--ranks", "64 32", "--partitions", str(parts),
                                   "--steps", "3200", "--rust-settings", settings, records_settings=False)
        self.assertEqual(rc2, 1)
        self.assertIn("rust_load_check_every = None", s2.state["status"])

    def test_campaign_tool_input_carries_the_settings(self):
        tool = HERE.parent / "c1prep_campaign.sh"
        tpl = HERE.parent.parent.parent.parent / "benchmarks" / "cohesive-perf" / "inputs" / "material-cohesive.dat"
        out = subprocess.run(["bash", str(tool), "--rust-input", str(tpl), "20", "/m/cube-joss.msh",
                              "/t/partition-np64.csv", "auto", *self.SETTINGS.split()],
                             capture_output=True, text=True, check=True).stdout.splitlines()
        i = out.index("user section [")
        self.assertEqual(out[i + 1:i + 6], ["  blocked_node = global", "  energy_precision = 17",
                                            "  rust_partitioner = file:/t/partition-np64.csv",
                                            "  rust_cohesive_compat = upstream",
                                            "  rust_doubled_node_load = follows_elements"])
        self.assertIn("  max_steps = 20", out)
        self.assertIn("  mesh = /m/cube-joss.msh", out)
        # the generator writes the bytes of the former sed one: a committed input
        ref = (HERE.parents[3] / "docs" / "benchmarks" / "midsize-2026-11-12" / "c1fix-joss-rust8" / "rust"
               / "np8" / "rep1" / "input.dat").read_text()
        val = {ln.split(" = ")[0].strip(): ln.split(" = ", 1)[1] for ln in ref.splitlines() if " = " in ln}
        again = subprocess.run(["bash", str(tool), "--rust-input", str(tpl), val["max_steps"], val["mesh"],
                                val["rust_partitioner"][len("file:"):], "auto", "export_partition=true"],
                               capture_output=True, text=True, check=True).stdout
        self.assertEqual(again, ref)

    def test_status_prints(self):
        s, f, rc = self.session()
        buf = io.StringIO()
        with redirect_stdout(buf):
            c1_session.main(["c1_session.py", "status", str(self.results)])
        self.assertIn("session: finished", buf.getvalue())
        self.assertIn("cpp@8", buf.getvalue())


if __name__ == "__main__":
    unittest.main()
