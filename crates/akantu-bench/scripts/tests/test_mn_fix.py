# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests: the
# comparator's same-binary class C and D-108 class J, the smoke session's
# verdict, the deadline of the hosts' timers, the hosts'
# readiness script's logic, run-name validation and the pinned pml,
# the retry of an unreachable host, the local backend's shared CPU
# set. Docker, SSH and the hosts are not used.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import io
import json
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import mn_aws_hosts  # noqa: E402
import mn_cluster  # noqa: E402
import mn_compare  # noqa: E402
import mn_session  # noqa: E402

BIN = "ab" * 32
SET = {"rust_cohesive_compat": "upstream", "rust_doubled_node_load": "follows_elements"}


def make_run(d, np_=2, hosts=("h1", "h2"), solver="auto", mesh="/mn/work/meshes/cube-h0.12.msh",
             energies=None, insertions=None, binary=BIN, settings=SET, counts=(404, 294, 710),
             outcome="ok", stats_np=None):
    """A run directory as mn_cluster.py writes it (the files mn_compare.py reads)."""
    d = Path(d)
    d.mkdir(parents=True, exist_ok=True)
    per = np_ // len(hosts)
    st = {"outcome": outcome, "np": np_, "nb_hosts": len(hosts), "hosts": list(hosts), "ranks_per_host": per,
          "rank_hosts": {h: list(range(i * per, (i + 1) * per)) for i, h in enumerate(hosts)},
          "partition": "scotch", "partition_sha256": "cd" * 32, "solver": solver, "mesh": mesh, "wall_s": 10.0}
    mn_cluster.write_status(d / "status.txt", st)
    summary = {"psize": np_, "mesh": mesh, "max_steps": 1500, "nb_global_nodes_start": 4824,
               "nb_regular_elements_dim3": 23156, "rust_static_sparse_solver": "eigen" if solver == "eigen" else "mumps",
               "rust_static_matrix": {"order": 14472, "nnz_sum": 1, "nnz_max": 1},
               "nb_cohesive_elements": counts[0], "nb_insertion_steps": counts[1], "first_insertion_step": counts[2],
               "time_step": 1e-7, "nb_global_nodes_end": 4900, "nb_owned_nodes_sum": 4900,
               "energy_dissipated": 3.0, "energy_potential": 6e5, "energy_kinetic": 1.6e4,
               "displacement_l2_norm": 0.17, "displacement_max_abs": 0.005, "displacement_sum": [1, 2, 3],
               "rust_static_solver_stats": {"nb_proc": stats_np or np_, "sym": 2, "infog7_ordering": 4}}
    summary.update(settings)
    (d / "summary.json").write_text(json.dumps(summary))
    for r in range(np_):
        (d / f"summary_rank{r}.json").write_text(json.dumps(
            {"nb_regular_elements_not_ghost": 100 + r, "rust_static_matrix_nnz": 7, "blocked_node_rule": "global",
             "blocked_y_nodes": [], "nb_nodes": 10, "nb_owned_nodes": 9, "nb_elements_dim3_not_ghost": 5,
             "nb_elements_dim3_ghost": 1, "nb_cohesive_not_ghost": 2}))
    rows = energies or [[s, 0.0, 6e5, 1e4 + s, 1.0, 6e5] for s in range(0, 20)]
    (d / "energies.csv").write_text("step, ed, ep, ek, ew, et\n" + "".join(
        ", ".join(f"{x:.17g}" if i else f"{int(x)}" for i, x in enumerate(r)) + "\n" for r in rows))
    ins = insertions if insertions is not None else ["10, 1, 1", "12, 2, 3"]
    (d / "insertions.csv").write_text("step, nb_inserted, nb_cohesive_total\n" + "".join(x + "\n" for x in ins))
    if binary:
        (d / "binary.sha256").write_text(f"{binary}  /usr/local/bin/cohesive_extrinsic\n")
    return d


def pair(ref, run, d108=False):
    out, fails = [], []
    mn_compare.compare_pair(mn_compare.Run(ref), mn_compare.Run(run), "t", out, fails, d108=d108)
    return out, fails


class Comparator(unittest.TestCase):
    def test_c_is_the_same_binary_reference_not_constants(self):
        with tempfile.TemporaryDirectory() as tmp:
            # counts that are not the pre-fix constants pass when they equal the reference's
            a = make_run(Path(tmp) / "a", counts=(400, 290, 700))
            b = make_run(Path(tmp) / "b", counts=(400, 290, 700))
            out, fails = pair(a, b)
            self.assertEqual(fails, [], out)
            self.assertTrue(any("C counts and insertion list" in x and x.endswith("ok") for x in out))
            # a different count or insertion list fails C
            c = make_run(Path(tmp) / "c", counts=(401, 290, 700))
            self.assertTrue(any(" C: " in f for f in pair(a, c)[1]))
            e = make_run(Path(tmp) / "e", counts=(400, 290, 700), insertions=["10, 1, 1", "13, 2, 3"])
            self.assertTrue(any("insertions.csv differs" in f for f in pair(a, e)[1]))
            # another binary fails (A, and C needs one binary)
            f = make_run(Path(tmp) / "f", counts=(400, 290, 700), binary="cd" * 32)
            self.assertTrue(any("binary.sha256" in x for x in pair(a, f)[1]))
            # a reference without a binary digest cannot judge C
            g = make_run(Path(tmp) / "g", counts=(400, 290, 700), binary=None)
            self.assertTrue(any("C needs two runs of one binary" in x for x in pair(g, b)[1]))

    def test_settings_compared(self):
        with tempfile.TemporaryDirectory() as tmp:
            a = make_run(Path(tmp) / "a")
            b = make_run(Path(tmp) / "b", settings={"rust_cohesive_compat": "upstream",
                                                     "rust_doubled_node_load": "compat"})
            self.assertTrue(any("rust_doubled_node_load" in x for x in pair(a, b)[1]))

    def test_j_d108(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = [[s, 0.0, 6e5, 1e4 + s, 1.0, 6e5] for s in range(30)]
            a = make_run(Path(tmp) / "a", mesh="/m/cube-joss.msh", energies=base,
                         insertions=["10, 1, 1", "20, 2, 3"])
            # within 1e-10 of the largest energy (6e5): 1e-6 absolute is 1.7e-12 of it
            near = [[r[0], r[1], r[2] + 1e-6, r[3], r[4], r[5]] for r in base]
            b = make_run(Path(tmp) / "b", mesh="/m/cube-joss.msh", energies=near,
                         insertions=["10, 1, 1", "20, 2, 3"])
            out, fails = pair(a, b, d108=True)
            self.assertEqual(fails, [], out)
            self.assertFalse(any(" C " in x for x in out))           # no C with --d108
            # beyond 1e-10 of the largest energy before the first count difference: J fails
            far = [[r[0], r[1], r[2] + (1.0 if r[0] == 5 else 0.0), r[3], r[4], r[5]] for r in base]
            c = make_run(Path(tmp) / "c", mesh="/m/cube-joss.msh", energies=far,
                         insertions=["10, 1, 1", "20, 2, 3"])
            self.assertTrue(any(" J: " in f for f in pair(a, c, d108=True)[1]))
            # the same difference after the first count difference (step 20 -> 21) is not judged
            # by (b); (c) needs the counts to agree through step 1,122 (shortened here to 15 / 25)
            late = [[r[0], r[1], r[2] + (1.0 if r[0] == 25 else 0.0), r[3], r[4], r[5]] for r in base]
            e = make_run(Path(tmp) / "e", mesh="/m/cube-joss.msh", energies=late,
                         insertions=["10, 1, 1", "21, 2, 3"])
            keep = mn_compare.D108_COUNTS_THROUGH
            try:
                mn_compare.D108_COUNTS_THROUGH = 15
                out, fails = pair(a, e, d108=True)
                self.assertEqual(fails, [], out)
                self.assertTrue(any("first step whose insertion counts differ: 20" in x for x in out), out)
                mn_compare.D108_COUNTS_THROUGH = 25
                self.assertTrue(any("(c) the insertion counts differ at step 20" in f
                                    for f in pair(a, e, d108=True)[1]))
            finally:
                mn_compare.D108_COUNTS_THROUGH = keep
            # D-108 1 (c) with its own step: counts differing at step 20 < 1,122 fail
            self.assertTrue(any("(c)" in f for f in pair(a, e, d108=True)[1]))
            # (a): two runs without the settings fail J even though they agree with each other
            n1 = make_run(Path(tmp) / "n1", mesh="/m/cube-joss.msh", energies=base, settings={},
                          insertions=["10, 1, 1", "20, 2, 3"])
            n2 = make_run(Path(tmp) / "n2", mesh="/m/cube-joss.msh", energies=base, settings={},
                          insertions=["10, 1, 1", "20, 2, 3"])
            self.assertTrue(any("(a) reference summary rust_doubled_node_load" in f
                                for f in pair(n1, n2, d108=True)[1]))
            # (b) jump: the external work's difference 1e-15 -> 5e-11 of the largest energy in one step
            jw = [[r[0], r[1], r[2], r[3], r[4] + (3e-5 if r[0] >= 7 else 6e-10), r[5]] for r in base]
            g = make_run(Path(tmp) / "g", mesh="/m/cube-joss.msh", energies=jw,
                         insertions=["10, 1, 1", "20, 2, 3"])
            self.assertTrue(any("(b) the external work's difference jumps at step 7" in f
                                for f in pair(a, g, d108=True)[1]), pair(a, g, d108=True))

    def test_smoke_verdict(self):
        with tempfile.TemporaryDirectory() as tmp:
            res = Path(tmp) / "res"
            runs = res / "runs"
            make_run(runs / "s-eig-h1-a1", np_=8, hosts=["h1"], solver="eigen")
            make_run(runs / "s-eig-h8-a1", np_=8, hosts=[f"h{k}" for k in range(1, 9)], solver="eigen")
            make_run(runs / "s-mumps-h1-a1", np_=8, hosts=["h1"])
            make_run(runs / "s-mumps-h8-a1", np_=8, hosts=[f"h{k}" for k in range(1, 9)])
            make_run(runs / "s-p64-a1", np_=64, hosts=[f"h{k}" for k in range(1, 9)],
                     mesh="/mn/work/meshes/cube-h0.06.msh")
            plan = [{"name": n} for n in ("s-eig-h1", "s-eig-h8", "s-mumps-h1", "s-mumps-h8", "s-p64")]
            (res / "state.json").write_text(json.dumps({"status": "finished", "plan": plan}))
            (res / "check.json").write_text(json.dumps({"problems": []}))
            img = Path(tmp) / "images.txt"
            img.write_text(f"akantu-rs-bench:mn sha256:01\nrust_bin_sha256 {BIN}\n")
            out, fails = mn_compare.smoke(res, img)
            self.assertEqual(fails, [], "\n".join(out))
            with redirect_stdout(io.StringIO()) as o:
                self.assertEqual(mn_compare.main(["x", "--smoke", str(res), str(img)]), 0)
            self.assertIn("SMOKE: PASS", o.getvalue())
            # another binary than images.txt's, a missing setting, a 64-rank job on 63 MUMPS processes
            img.write_text("rust_bin_sha256 " + "ef" * 32 + "\n")
            self.assertTrue(any("binary.sha256" in f for f in mn_compare.smoke(res, img)[1]))
            img.write_text(f"rust_bin_sha256 {BIN}\n")
            make_run(runs / "s-p64-a1", np_=64, hosts=[f"h{k}" for k in range(1, 9)], stats_np=63,
                     settings={"rust_cohesive_compat": "upstream"})
            f = mn_compare.smoke(res, img)[1]
            self.assertTrue(any("nb_proc 63" in x for x in f), f)
            self.assertTrue(any("rust_doubled_node_load" in x for x in f), f)
            # a session that did not finish, a preflight problem
            (res / "state.json").write_text(json.dumps({"status": "failed: x", "plan": plan}))
            (res / "check.json").write_text(json.dumps({"problems": ["rank 3: pins"]}))
            f = mn_compare.smoke(res, img)[1]
            self.assertTrue(any("not finished" in x for x in f) and any("preflight" in x for x in f))


class Deadline(unittest.TestCase):
    def test_parse_and_budget(self):
        t = "USEC=1794500000123456\nWARN_WALL=1\nMODE=poweroff\n"
        self.assertEqual(mn_session.parse_scheduled(t), 1794500000)
        self.assertEqual(mn_aws_hosts.parse_scheduled(t), 1794500000)
        self.assertIsNone(mn_session.parse_scheduled("MODE=poweroff\n"))
        self.assertEqual(mn_session.deadline_epoch("1794500000"), 1794500000)
        with self.assertRaises(ValueError):
            mn_session.deadline_epoch("tomorrow")
        now = 1_000_000.0
        self.assertAlmostEqual(mn_aws_hosts.budget_h(now + 3 * 3600, now, 5.0), 3 - 1 / 3)
        self.assertEqual(mn_aws_hosts.budget_h(now + 9 * 3600, now, 5.0), 5.0)

    def test_session_budget_capped_by_the_timer(self):
        with tempfile.TemporaryDirectory() as tmp:
            plan = Path(tmp) / "plan.txt"
            plan.write_text("a h0.12 8 1 10 auto scotch 0.1\n")
            a = mn_session.parse_args([str(Path(tmp) / "s"), str(Path(tmp) / "r"), "5.0", "--plan", str(plan),
                                       "--deadline", str(int(time.time()) + 2 * 3600)])
            s = mn_session.Session(a, cluster=type("C", (), {"conf": {}})(), clock=time.time)
            s.state = {"started_epoch": time.time()}
            # 2 h to the timer, minus 20 min, minus the 0.1-h reserve: ~1.57 h, not 4.9 h
            self.assertAlmostEqual(s.remaining_h(), 2 - 1 / 3 - 0.1, places=2)

    def test_arming_plan(self):
        now = 1_000_000
        tgt, arm = mn_aws_hosts.arming_plan({"a": now + 3600, "b": None, "c": now + 7200}, now, 330)
        self.assertEqual((tgt, arm), (now + 3600, {"b": 60}))
        tgt, arm = mn_aws_hosts.arming_plan({"a": None, "b": None}, now, 330)
        self.assertEqual((tgt, arm), (now + 330 * 60, {"a": 330, "b": 330}))

    def test_hosts_commands_strict_and_through_the_head(self):
        hs = mn_aws_hosts.Hosts("mn-ssh.conf", "198.51.100.7", ["10.0.0.2"], dry=True)
        a = hs.argv("10.0.0.2", "cloud-init status --wait")
        self.assertEqual(a[:3], ["ssh", "-F", "mn-ssh.conf"])
        self.assertIn("-J", a)
        self.assertEqual(a[a.index("-J") + 1], "198.51.100.7")
        self.assertNotIn("-J", hs.argv("198.51.100.7", "true"))
        with redirect_stdout(io.StringIO()) as o:
            self.assertEqual(mn_aws_hosts.main(["halt", "-F", "c", "--head", "H", "--workers", "W2", "W3",
                                                "--dry-run"]), 0)
        lines = [ln for ln in o.getvalue().splitlines() if ln.startswith("DRY ")]
        # workers first, through the head, then the head
        self.assertTrue(lines[0].endswith("W2 'sudo shutdown -h now'") and "-J H" in lines[0])
        self.assertTrue(lines[-1].endswith("H 'sudo shutdown -h now'") and "-J" not in lines[-1])


class Launcher(unittest.TestCase):
    def cluster(self, tmp, extra=""):
        d = Path(tmp)
        (d / "cluster.conf").write_text("backend = local\nnet_cidr = 10.9.0.0/24\nnetwork = mn-t\n" + extra)
        (d / "hosts").write_text("t-h1 slots=4\nt-h2 slots=4\n")
        return mn_cluster.Cluster(d)

    def test_pml_and_run_names(self):
        a = mn_cluster.mpirun_argv(8, "/h", "/w", "10.0.0.0/8", "core")
        self.assertEqual(a[a.index("pml") - 1:a.index("pml") + 2], ["--mca", "pml", "ob1"])
        with tempfile.TemporaryDirectory() as tmp:
            c = self.cluster(tmp)
            for bad in ("../meshes", "a/b", "", ".x", "A"):
                with self.assertRaisesRegex(mn_cluster.ClusterError, "run name"):
                    c.run(bad, Path(tmp) / "out", 8, 2, "/m.msh", 10)

    def test_cpuset_all_needs_no_binding(self):
        with self.assertRaises(mn_cluster.ClusterError):
            mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\ncpuset = all\n")
        c = mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\ncpuset = all\nbind = none\n")
        self.assertEqual(c["cpuset"], "all")
        self.assertIn("mpi_yield_when_idle", mn_cluster.mpirun_argv(64, "/h", "/w", "10.0.0.0/8", "none", True))
        self.assertNotIn("mpi_yield_when_idle", mn_cluster.mpirun_argv(64, "/h", "/w", "10.0.0.0/8", "core"))

    def test_restore_retries_an_unreachable_host(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = self.cluster(tmp)
            answers = ["unreachable", "unreachable", "exited", "running"]
            c.alive = lambda k: answers.pop(0) if len(answers) > 1 else answers[0]
            c.ssh_from_head = lambda k: True
            c.call = lambda *a, **kw: (0, b"", b"")
            self.assertTrue(c.restore(1, unreachable_s=5, pause=0.01))
            c.alive = lambda k: "unreachable"
            t0 = time.time()
            self.assertFalse(c.restore(1, unreachable_s=0.2, pause=0.05))
            self.assertGreaterEqual(time.time() - t0, 0.2)


if __name__ == "__main__":
    unittest.main()
