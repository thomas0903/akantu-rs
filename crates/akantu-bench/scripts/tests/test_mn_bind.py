# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the multi-node launcher's rank binding
# `bind = self` (mn_cluster.py: mpirun without binding, every rank bound by
# bind-rank.sh to one core chosen by its index on its host), its preflight
# and binding records, and of bind-rank.sh itself (Linux only: it reads
# /proc/self/status). Docker, SSH and the hosts are not used.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import mn_cluster  # noqa: E402
import mn_compare  # noqa: E402
import mn_session  # noqa: E402

sys.path.insert(0, str(HERE))
from test_mn_fix import make_run, pair  # noqa: E402
from test_mn_session import PLAN, FakeCluster, make_args  # noqa: E402

REPO = HERE.parent.parent.parent.parent
BIND_RANK = REPO / "benchmarks" / "cohesive-perf" / "scripts" / "in-container" / "bind-rank.sh"
PINS = ["Haswell", "1", "1", "1", "1"]


def row(rank, host, cpus, local, sib=None):
    return [rank, host, *PINS, cpus, local, cpus if sib is None else sib]


class Conf(unittest.TestCase):
    def test_self_is_the_default_and_validated(self):
        c = mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\n")
        self.assertEqual(c["bind"], "self")
        for b in ("self", "core", "none"):
            self.assertEqual(mn_cluster.parse_conf(f"net_cidr = 10.0.0.0/8\nbind = {b}\n")["bind"], b)
        for bad in ("bind = socket\n", "bind = selfish\n", "cpuset = all\nbind = self\n"):
            with self.assertRaises(mn_cluster.ClusterError):
                mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\n" + bad)


class Commands(unittest.TestCase):
    def test_mpirun_does_not_bind_with_self(self):
        a = mn_cluster.mpirun_argv(128, "/h", "/w", "10.0.0.0/8", "self")
        self.assertEqual(a[a.index("--bind-to") + 1], "none")
        self.assertNotIn("--report-bindings", a)
        self.assertIn("slot:NOOVERSUBSCRIBE", a)
        c = mn_cluster.mpirun_argv(8, "/h", "/w", "10.0.0.0/8", "core")
        self.assertEqual(c[c.index("--bind-to") + 1], "core")
        self.assertIn("--report-bindings", c)
        n = mn_cluster.mpirun_argv(8, "/h", "/w", "10.0.0.0/8", "none")
        self.assertEqual(n[n.index("--bind-to") + 1], "none")

    def test_every_rank_goes_through_bind_rank(self):
        drv = [mn_cluster.DRIVER, "--aka_input_file", "input.dat"]
        a = mn_cluster.app_argv("self", drv, record=True)
        self.assertEqual(a[:3], ["sh", mn_cluster.BINDER, "1"])
        self.assertEqual(a[3:5], ["sh", "-c"])
        self.assertEqual(a[-3:], drv)
        self.assertIn("binding_rank${OMPI_COMM_WORLD_RANK", a[5])
        self.assertEqual(mn_cluster.app_argv("core", drv), drv)
        self.assertEqual(mn_cluster.app_argv("none", drv, record=True)[:2], ["sh", "-c"])
        # the probe of the preflight is bound the same way, without the record
        self.assertEqual(mn_cluster.app_argv("self", ["sh", "-c", "x"]), ["sh", mn_cluster.BINDER, "1", "sh", "-c", "x"])
        script = mn_cluster.run_script("/w", mn_cluster.mpirun_argv(8, "/h", "/w", "10.0.0.0/8", "self"), a)
        self.assertIn(f"sh {mn_cluster.BINDER} 1 sh -c", script)

    def test_the_image_ships_bind_rank_where_the_launcher_expects_it(self):
        df = (REPO / "docker" / "bench" / "Dockerfile.mn").read_text()
        self.assertIn(f"COPY bind-rank.sh {mn_cluster.BINDER}\n", df)
        ci = (REPO / "crates" / "akantu-bench" / "scripts" / "c1_images.sh").read_text()
        self.assertIn("benchmarks/cohesive-perf/scripts/in-container/bind-rank.sh", ci)


class Preflight(unittest.TestCase):
    hosts = (("a", 2), ("b", 2))
    good = (row(0, "a", "0", "0"), row(1, "a", "1", "1"), row(2, "b", "0", "0"), row(3, "b", "1", "1"))

    def test_one_distinct_core_per_rank_on_every_host(self):
        self.assertEqual(mn_cluster.check_problems(self.good, self.hosts, "self"), [])
        bad = [list(r) for r in self.good]
        bad[1][7] = "0"
        self.assertTrue(any("share a CPU set" in p for p in mn_cluster.check_problems(bad, self.hosts, "self")))
        bad = [list(r) for r in self.good]
        bad[3][7] = "0-1"   # not bound: the container's whole cpuset
        self.assertTrue(any("one CPU" in p for p in mn_cluster.check_problems(bad, self.hosts, "self")))
        self.assertEqual(mn_cluster.check_problems(bad, self.hosts, "none"), [])

    def test_local_rank_is_set_and_counts_per_host(self):
        bad = [list(r) for r in self.good]
        bad[2][8] = "2"   # the world rank instead of the index on the host
        self.assertTrue(any("local ranks" in p for p in mn_cluster.check_problems(bad, self.hosts, "self")))
        bad[2][8] = "unset"
        self.assertTrue(any("local ranks" in p for p in mn_cluster.check_problems(bad, self.hosts, "self")))
        self.assertEqual(mn_cluster.check_problems(bad, self.hosts, "core"), [])

    def test_two_threads_per_core_is_one_core_per_two_ranks(self):
        # a host with 2 hardware threads per core, numbered 0-3 / 4-7 (cpu c and c + 4 are one
        # core): bind-rank.sh gives local ranks 0..3 the CPUs 0..3 (four cores, fine), but with
        # threads numbered 0,1 / 2,3 (siblings adjacent) ranks 0 and 1 share core 0
        hosts = (("a", 4),)
        ok = tuple(row(r, "a", str(r), str(r), f"{r},{r + 4}") for r in range(4))
        self.assertEqual(mn_cluster.check_problems(ok, hosts, "self"), [])
        adjacent = tuple(row(r, "a", str(r), str(r), f"{r - r % 2}-{r - r % 2 + 1}") for r in range(4))
        probs = mn_cluster.check_problems(adjacent, hosts, "self")
        self.assertTrue(any("share a physical core" in p for p in probs), probs)
        # siblings unknown (`-`): not compared; the CPU check still applies
        unknown = tuple(row(r, "a", str(r), str(r), "-") for r in range(4))
        self.assertEqual(mn_cluster.check_problems(unknown, hosts, "self"), [])
        self.assertEqual(mn_cluster.check_problems(adjacent, hosts, "core"), [])

    def test_check_runs_the_bound_probe_and_parses_its_lines(self):
        out = "".join(f"MNCHECK {r[1]} {r[0]} {' '.join(PINS)} {r[7]} {r[8]} {r[9]}\n" for r in self.good) + "0\n"
        scripts = []
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            (d / "cluster.conf").write_text("backend = local\nnet_cidr = 10.9.0.0/24\nnetwork = mn-t\n")
            (d / "hosts").write_text("a slots=2\nb slots=2\n")
            c = mn_cluster.Cluster(d)
            c.alive = lambda k: "running"
            c.ssh_from_head = lambda k: True
            c.image_id = lambda k: "rootfs:x"

            def call(argv, timeout=120, data=None, check=False):
                if argv[-2:] == ["-c", argv[-1]] and argv[-3] == "bash":
                    scripts.append(argv[-1])
                if "cat" in argv and any(str(x).endswith("/out.txt") for x in argv):
                    return 0, out.encode(), b""
                return 0, b"", b""
            c.call = call
            r = c.check()
        self.assertEqual(r["problems"], [])
        self.assertEqual([x[8] for x in r["ranks"]], ["0", "1", "0", "1"])
        self.assertEqual([x[9] for x in r["ranks"]], ["0", "1", "0", "1"])
        self.assertIn("thread_siblings_list", scripts[0])
        self.assertEqual(len(scripts), 1)
        self.assertIn("--bind-to none", scripts[0])
        self.assertIn(f"sh {mn_cluster.BINDER} 1 sh -c", scripts[0])


class RunRecords(unittest.TestCase):
    def test_bindings_from_the_rank_files(self):
        files = {"binding_rank0.txt": "h1 0 4\n", "binding_rank1.txt": "h1 1 5\n", "binding_rank2.txt": "h2 0 4\n",
                 "binding_rank3.txt": "h2 1 5\n", "summary_rank0.json": "{}", "binding_rankx.txt": "h 0 1\n"}
        b = mn_cluster.bindings_from(files)
        self.assertEqual(list(b), [0, 1, 2, 3])
        self.assertEqual(b[2], ["h2", "0", "4", "-"])
        self.assertEqual(mn_cluster.binding_problems(b, 4, "self"), [])
        self.assertTrue(mn_cluster.binding_problems(b, 8, "self"))
        b[1] = ["h1", "1", "4"]
        self.assertTrue(any("share" in p for p in mn_cluster.binding_problems(b, 4, "self")))
        b[1] = ["h1", "1", "4-7"]
        self.assertTrue(any("expected one" in p for p in mn_cluster.binding_problems(b, 4, "self")))
        self.assertEqual(mn_cluster.binding_problems({}, 4, "none"), [])

    def test_two_ranks_on_the_threads_of_one_core(self):
        files = {"binding_rank0.txt": "h1 0 0 0-1\n", "binding_rank1.txt": "h1 1 1 0-1\n",
                 "binding_rank2.txt": "h2 0 0 0-1\n", "binding_rank3.txt": "h2 1 2 2-3\n"}
        b = mn_cluster.bindings_from(files)
        self.assertEqual(b[1], ["h1", "1", "1", "0-1"])
        probs = mn_cluster.binding_problems(b, 4, "self")
        self.assertEqual(len(probs), 1, probs)
        self.assertIn("ranks 0 and 1 on h1 share a physical core", probs[0])
        self.assertEqual(mn_cluster.binding_problems(b, 4, "core"), [])

    def test_the_record_writes_the_siblings(self):
        self.assertIn("thread_siblings_list", mn_cluster.RECORD)
        self.assertIn('$cpus $sib" > "binding_rank', mn_cluster.RECORD)


class BindingOutcome(unittest.TestCase):
    base = {"exit": 0, "summary": True, "nb_rank_summaries": 8, "np": 8}

    def test_binding_problems_make_the_run_fail_unless_a_host_was_lost(self):
        self.assertEqual(mn_cluster.classify(dict(self.base)), "ok")
        self.assertEqual(mn_cluster.classify(dict(self.base, binding_problems=[])), "ok")
        bad = dict(self.base, binding_problems=["ranks 0 and 1 on h1 share a physical core"])
        self.assertEqual(mn_cluster.classify(bad), "failed")
        # a lost host legitimately leaves records missing: the loss is the outcome
        lost = dict(bad, lost_hosts=["h3"], killed="host-lost",
                    binding_problems=["binding records of ranks [0, 1], expected 0..7"])
        self.assertEqual(mn_cluster.classify(lost), "host-lost")
        self.assertEqual(mn_cluster.classify(dict(bad, killed="timeout")), "timeout")

    def test_the_session_log_line_names_them(self):
        why = ["ranks 0 and 1 on 10.0.0.1 share a physical core (hardware threads 0-1)"]
        c = FakeCluster(scenario={"smoke-a1": {"outcome": "failed", "binding": why}})
        with tempfile.TemporaryDirectory() as tmp:
            s = mn_session.Session(make_args(tmp, PLAN), c, clock=c.clock)
            buf = io.StringIO()
            with redirect_stdout(buf):
                rc = s.run()
            line = next(ln for ln in buf.getvalue().splitlines() if "run 1 smoke " in ln)
            self.assertIn("failed", line)
            self.assertIn("binding problems: " + why[0], line)
            self.assertEqual(s.records()[0]["binding_problems"], why)
            self.assertNotEqual(rc, 0)   # smoke is @required


def campaign_ref(root, np_=2, mesh="/target/c1prep/meshes/frozen-cube-h0.12.msh", part=b"elem,rank\n0,0\n"):
    """A Rust run directory of c1prep_campaign.sh: <campaign>/rust/np<N>/rep1 with its own
    status.txt (no placement record, no exported partition) and <campaign>/partition-np<N>.csv."""
    d = make_run(Path(root) / "camp" / "rust" / f"np{np_}" / "rep1", np_=np_, hosts=("x",), mesh=mesh)
    mn_cluster.write_status(d / "status.txt", {"case": "rust", "np": np_, "exit": 0, "oom_kills": 0,
                                               "wall_s": 3.3, "outcome": "ok"})
    if part is not None:
        (Path(root) / "camp" / f"partition-np{np_}.csv").write_bytes(part)
    return d


class CampaignReference(unittest.TestCase):
    def multi(self, root, part=b"elem,rank\n0,0\n"):
        d = make_run(Path(root) / "mn", np_=2, hosts=("h1", "h2"), mesh="/mn/work/meshes/cube-h0.12.msh")
        st = mn_cluster.read_status(d / "status.txt")
        st.update(partition="file:/mn/work/partitions/partition-np2.csv", partition_matches=True,
                  partition_sha256=hashlib.sha256(part).hexdigest())
        mn_cluster.write_status(d / "status.txt", st)
        return d

    def test_accepted_with_the_frozen_mesh_name_and_its_partition(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, fails = pair(campaign_ref(tmp), self.multi(tmp))
            self.assertEqual(fails, [], out)
            self.assertTrue(any("partition = " in ln for ln in out), out)
            self.assertTrue(mn_compare.is_foreign(mn_compare.Run(Path(tmp) / "camp" / "rust" / "np2" / "rep1")))

    def test_another_partition_or_mesh_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, fails = pair(campaign_ref(tmp), self.multi(tmp, part=b"elem,rank\n0,1\n"))
            self.assertTrue(any("partition.csv" in f for f in fails), fails)
        with tempfile.TemporaryDirectory() as tmp:
            ref = campaign_ref(tmp, mesh="/target/c1prep/meshes/cube-h0.06.msh")
            _, fails = pair(ref, self.multi(tmp))
            self.assertTrue(any("mesh" in f for f in fails), fails)

    def test_without_the_campaign_partition_file_it_says_so(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, fails = pair(campaign_ref(tmp, part=None), self.multi(tmp))
            self.assertEqual(fails, [])
            self.assertTrue(any("not checked" in ln for ln in out), out)

    def test_another_binary_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref = campaign_ref(tmp)
            (ref / "binary.sha256").write_text("ef" * 32 + "  /usr/local/bin/cohesive_extrinsic\n")
            _, fails = pair(ref, self.multi(tmp))
            self.assertTrue(any("not the same driver" in f for f in fails), fails)


class FinalPlan(unittest.TestCase):
    def test_the_final_stage_plan(self):
        plan = mn_session.parse_plan((HERE.parent / "mn_plan_final.txt").read_text())
        got = [(r["name"], r["mesh"], r["np"], r["hosts"], r["steps"], r["partition"], r["required"]) for r in plan]
        self.assertEqual(got, [("smoke-h012", "h0.12", 8, 4, 1500, "scotch", True),
                               ("joss128x4", "joss", 128, 4, 5000, "file:partition-np128.csv", False),
                               ("joss64x2", "joss", 64, 2, 5000, "file:partition-np64.csv", False)])
        self.assertTrue(all(r["extra"] == [] and r["solver"] == "auto" for r in plan))
        # 32 ranks per host on hosts of 32 slots
        hosts = [(f"10.0.0.{k}", 32) for k in range(1, 5)]
        self.assertEqual([mn_cluster.placement(r["np"], r["hosts"], hosts) for r in plan], [2, 32, 32])


@unittest.skipUnless(Path("/proc/self/status").exists() and shutil.which("taskset"), "Linux with taskset")
class BindRankScript(unittest.TestCase):
    def run_bound(self, env):
        cmd = ["sh", str(BIND_RANK), "1", "awk", "/^Cpus_allowed_list:/ {print $2}", "/proc/self/status"]
        base = {k: v for k, v in os.environ.items() if not k.startswith(("OMPI_", "PMIX_"))}
        return subprocess.run(cmd, env={**base, **env}, capture_output=True, text=True, check=False)

    def allowed(self):
        cpus = []
        for part in Path("/proc/self/status").read_text().split("Cpus_allowed_list:")[1].split()[0].split(","):
            lo, _, hi = part.partition("-")
            cpus += list(range(int(lo), int(hi or lo) + 1))
        return cpus

    def test_local_rank_selects_the_core(self):
        cpus = self.allowed()
        p = self.run_bound({"OMPI_COMM_WORLD_RANK": "100000", "OMPI_COMM_WORLD_LOCAL_RANK": "0"})
        self.assertEqual((p.returncode, p.stdout.strip()), (0, str(cpus[0])))
        if len(cpus) > 1:
            p = self.run_bound({"OMPI_COMM_WORLD_RANK": "0", "OMPI_COMM_WORLD_LOCAL_RANK": "1"})
            self.assertEqual((p.returncode, p.stdout.strip()), (0, str(cpus[1])))

    def test_two_cores_per_rank(self):
        cpus = self.allowed()
        if len(cpus) < 4:
            self.skipTest("needs 4 CPUs")
        cmd = ["sh", str(BIND_RANK), "2", "awk", "/^Cpus_allowed_list:/ {print $2}", "/proc/self/status"]
        base = {k: v for k, v in os.environ.items() if not k.startswith(("OMPI_", "PMIX_"))}
        p = subprocess.run(cmd, env={**base, "OMPI_COMM_WORLD_LOCAL_RANK": "1"}, capture_output=True, text=True,
                           check=False)
        self.assertEqual(p.returncode, 0, p.stderr)
        got = []
        for part in p.stdout.strip().split(","):
            lo, _, hi = part.partition("-")
            got += list(range(int(lo), int(hi or lo) + 1))
        self.assertEqual(got, cpus[2:4])

    def test_world_rank_without_local_rank(self):
        cpus = self.allowed()
        p = self.run_bound({"OMPI_COMM_WORLD_RANK": "0"})
        self.assertEqual((p.returncode, p.stdout.strip()), (0, str(cpus[0])))
        self.assertEqual(self.run_bound({"OMPI_COMM_WORLD_RANK": "100000"}).returncode, 98)
        self.assertEqual(self.run_bound({}).returncode, 97)


if __name__ == "__main__":
    unittest.main()
