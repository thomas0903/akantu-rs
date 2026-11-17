# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the multi-node launcher's pure rules
# (crates/akantu-bench/scripts/mn_cluster.py) and of the session's rules
# (mn_session.py): plan parsing, placement, the driver
# input, the mpirun command, outcome classification, the preflight checks,
# order, budget skip and wall-clock limits, the out-of-memory rule, the
# host-lost repetition and dead hosts, failures, required runs, partitions
# from earlier runs, the test hooks and resume. Docker and the hosts are
# replaced by a fake cluster and a fake clock.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import hashlib
import io
import json
import sys
import tarfile
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import mn_cluster  # noqa: E402
import mn_session  # noqa: E402

SETTINGS = ["rust_cohesive_compat=upstream", "rust_doubled_node_load=follows_elements"]
TEMPLATE = (HERE.parent.parent.parent.parent / "benchmarks" / "cohesive-perf" / "inputs"
            / "material-cohesive.dat").read_text()


class FakeCluster:
    """Records the session's calls; run() outcomes come from a scenario."""

    def __init__(self, nhosts=4, slots=8, scenario=None, problems=None, restorable=None):
        self.hosts = [(f"10.0.0.{k + 1}", slots) for k in range(nhosts)]
        self.conf = {"backend": "ssh", "image": "akantu-rs-bench:mn", "memory": "56g", "bind": "core"}
        self.scenario = scenario or {}   # "name-aN" -> dict(outcome, hours, lost, oom)
        self.problems = problems or []
        self.restorable = set(self.addr(k) for k in range(nhosts)) if restorable is None else set(restorable)
        self.calls, self.memory, self.restored, self.puts = [], [], [], []
        self.now = 1_000_000.0

    def clock(self):
        return self.now

    def addr(self, k):
        return self.hosts[k][0]

    def load(self, tarball, expect_id=None):
        return {k: expect_id for k in range(len(self.hosts))}

    def up(self):
        self.now += 60

    def check(self):
        return {"hosts": [], "problems": list(self.problems)}

    def put(self, local, dest):
        self.puts.append(dest)
        return hashlib.sha256(Path(local).read_bytes()).hexdigest()

    def set_memory(self, k, m):
        self.memory.append((self.addr(k), m))

    def restore(self, k):
        self.restored.append(self.addr(k))
        return self.addr(k) in self.restorable

    def run(self, name, outdir, np_, nhosts, mesh, steps, partition, solver, extra, timeout_s, watch_s,
            host_idx=None, fail=None):
        sc = dict(self.scenario.get(name, {}))
        idx = host_idx if host_idx is not None else list(range(nhosts))
        self.calls.append(dict(name=name, np=np_, hosts=[self.addr(k) for k in idx], partition=partition,
                               solver=solver, timeout_s=timeout_s, fail=fail, mesh=mesh, extra=extra))
        self.now += sc.get("hours", 0.1) * 3600
        outcome = sc.get("outcome", "ok")
        st = {"outcome": outcome, "np": np_, "hosts": [self.addr(k) for k in idx], "exit": 0 if outcome == "ok" else 1,
              "lost_hosts": sc.get("lost", []), "oom_kills": {h: 1 for h in sc.get("oom", [])},
              "partition_sha256": "ab" * 32 if np_ > 1 and outcome == "ok" else None,
              "host_peak_kib": {self.addr(k): 1024 ** 2 for k in idx}, "wall_s": 1.0}
        Path(outdir).mkdir(parents=True, exist_ok=True)
        mn_cluster.write_status(Path(outdir) / "status.txt", st)
        return st


def make_args(tmp, plan, budget=10.0, extra=()):
    p = Path(tmp) / "plan.txt"
    p.write_text(plan)
    return mn_session.parse_args([str(Path(tmp) / "session"), str(Path(tmp) / "results"), str(budget),
                                  "--plan", str(p), *extra])


def run_session(tmp, plan, cluster, budget=10.0, extra=()):
    a = make_args(tmp, plan, budget, extra)
    s = mn_session.Session(a, cluster, clock=cluster.clock)
    with redirect_stdout(io.StringIO()):
        rc = s.run()
    return s, rc


def mesh_tarball(path, sha=None):
    """A mesh tarball with cube-h0.06.msh + .json (sha: the JSON's SHA-256, default the file's)."""
    msh = b"$MeshFormat\n4.1 0 8\n$EndMeshFormat\n"
    js = {"sha256": sha or hashlib.sha256(msh).hexdigest(), "nodes": 3, "elements_by_type": {"tetrahedron4": 1}}
    with tarfile.open(path, "w:gz") as t:
        for n, data in (("cube-h0.06.msh", msh), ("cube-h0.06.json", json.dumps(js).encode())):
            ti = tarfile.TarInfo(n)
            ti.size = len(data)
            t.addfile(ti, io.BytesIO(data))
    return str(path)


def outcomes(s):
    return [(r["name"], r["attempt"], r["outcome"]) for r in s.records()]


PLAN = """
# name  mesh  np hosts steps solver partition estimate
smoke   h0.12 8  4  1500  auto  scotch  0.05 @required
big     h0.12 32 4  5000  auto  scotch  1.0
small   h0.12 8  1  1500  auto  run:smoke 0.05
"""


class PureRules(unittest.TestCase):
    def test_conf_and_hosts(self):
        c = mn_cluster.parse_conf("backend = ssh\nnet_cidr = 172.31.0.0/16\nmemory = 56g\n")
        self.assertEqual((c["backend"], c["ssh_port"], c["bind"]), ("ssh", "2222", "self"))
        for bad in ("backend = ssh\n", "backend = k8s\nnet_cidr = 10.0.0.0/8\n",
                    "net_cidr = 10.0.0.0/8\nbind = socket\n", "net_cidr = 10.0.0.0/8\nfoo = 1\n",
                    "net_cidr = 10.0.0.0/8\nssh_port = 22\n"):
            with self.assertRaises(mn_cluster.ClusterError):
                mn_cluster.parse_conf(bad)
        h = mn_cluster.parse_hosts("# head first\n10.0.0.5 slots=8\n10.0.0.6 slots=8\n")
        self.assertEqual(h, [("10.0.0.5", 8), ("10.0.0.6", 8)])
        for bad in ("", "10.0.0.5\n", "10.0.0.5 slots=0\n", "a slots=2\na slots=2\n", "a b slots=2\n"):
            with self.assertRaises(mn_cluster.ClusterError):
                mn_cluster.parse_hosts(bad)

    def test_placement(self):
        hosts = [("h1", 8), ("h2", 8), ("h3", 8), ("h4", 8)]
        self.assertEqual(mn_cluster.placement(32, 4, hosts), 8)
        self.assertEqual(mn_cluster.placement(8, 2, hosts), 4)
        for np_, nh in ((12, 8), (6, 4), (64, 4), (2, 4), (8, 0)):
            with self.assertRaises(mn_cluster.ClusterError):
                mn_cluster.placement(np_, nh, hosts)
        self.assertEqual(mn_cluster.hostfile_text(hosts[:2], 4), "h1 slots=4\nh2 slots=4\n")
        self.assertEqual(mn_cluster.cpusets([("a", 2), ("b", 2), ("c", 1)], 0), ["0-1", "2-3", "4"])

    def test_input_is_the_campaign_tools(self):
        t = mn_cluster.input_text(TEMPLATE, 1500, "/mn/work/meshes/cube-h0.12.msh",
                                  "file:/mn/work/p.csv", "eigen", ["export_partition=true"])
        lines = t.splitlines()
        i = lines.index("user section [")
        self.assertEqual(lines[i + 1:i + 6], ["  blocked_node = global", "  energy_precision = 17",
                                              "  rust_partitioner = file:/mn/work/p.csv",
                                              "  rust_sparse_solver = eigen", "  export_partition = true"])
        self.assertIn("  max_steps = 1500", lines)
        self.assertIn("  mesh = /mn/work/meshes/cube-h0.12.msh", lines)
        self.assertIn("  output_energy = true", lines)
        self.assertEqual(sum(ln.startswith("  max_steps = ") for ln in lines), 1)
        t = mn_cluster.input_text(TEMPLATE, 20, "/m.msh", "scotch", "auto", [])
        self.assertNotIn("rust_partitioner", t)
        self.assertNotIn("rust_sparse_solver", t)

    def test_mpirun_command(self):
        a = mn_cluster.mpirun_argv(64, "/h", "/w", "172.31.0.0/16", "core")
        s = " ".join(a)
        for want in ("-np 64", "--hostfile /h", "--map-by slot:NOOVERSUBSCRIBE", "--bind-to core",
                     "--report-bindings", "--wdir /w", "btl_tcp_if_include 172.31.0.0/16",
                     "--prtemca oob_tcp_if_include 172.31.0.0/16"):
            self.assertIn(want, s)
        script = mn_cluster.run_script("/w", a, ["/usr/local/bin/cohesive_extrinsic", "--aka_input_file", "input.dat"])
        for v in mn_cluster.PIN_VARS:
            self.assertIn(v, script)
        for v in mn_cluster.UNSET_ENV:
            self.assertIn(v, script.split("\n")[2])
        self.assertIn("echo $? > exit", script)

    def test_classify(self):
        base = {"np": 8, "exit": 0, "summary": True, "nb_rank_summaries": 8}
        self.assertEqual(mn_cluster.classify(base), "ok")
        self.assertEqual(mn_cluster.classify(dict(base, nb_rank_summaries=7)), "failed")
        self.assertEqual(mn_cluster.classify(dict(base, exit=1)), "failed")
        self.assertEqual(mn_cluster.classify(dict(base, killed="timeout", exit=None)), "timeout")
        self.assertEqual(mn_cluster.classify(dict(base, lost_hosts=["h3"], killed="host-lost")), "host-lost")
        # an out-of-memory kill is the cause even when the job was then stopped for a lost host
        self.assertEqual(mn_cluster.classify(dict(base, lost_hosts=["h3"], oom_kills={"h2": 1, "h3": None})), "oom")
        self.assertEqual(mn_cluster.classify(dict(base, oom_kills={"h2": 0, "h3": None})), "ok")

    def test_preflight_checks(self):
        hosts = [("a", 2), ("b", 2)]
        good = [[0, "a", "Haswell", "1", "1", "1", "1", "0"], [1, "a", "Haswell", "1", "1", "1", "1", "1"],
                [2, "b", "Haswell", "1", "1", "1", "1", "2"], [3, "b", "Haswell", "1", "1", "1", "1", "3"]]
        self.assertEqual(mn_cluster.check_problems(good, hosts, "core"), [])
        bad = [list(r) for r in good]
        bad[2][3] = "8"
        self.assertTrue(any("OPENBLAS_NUM_THREADS" in p for p in mn_cluster.check_problems(bad, hosts, "core")))
        bad = [list(r) for r in good]
        bad[1][7] = "0"
        self.assertTrue(any("share a CPU set" in p for p in mn_cluster.check_problems(bad, hosts, "core")))
        self.assertEqual(mn_cluster.check_problems(bad, hosts, "none"), [])
        bad = [list(r) for r in good]
        bad[2][1] = "a"
        self.assertTrue(mn_cluster.check_problems(bad, hosts, "core"))
        self.assertTrue(mn_cluster.check_problems(good[:3], hosts, "core"))

    def test_rootfs_digest_and_images_txt(self):
        layers = '["sha256:aa", "sha256:bb"]'
        d = mn_cluster.rootfs_digest(layers)
        self.assertEqual(d, "rootfs:" + hashlib.sha256(b"sha256:aa\nsha256:bb").hexdigest())
        with tempfile.TemporaryDirectory() as tmp:
            f = Path(tmp) / "images.txt"
            f.write_text(f"akantu-rs-bench:mn sha256:0123\nakantu-rs-bench:mn-rootfs {d}\n")
            self.assertEqual(mn_session.expected_image_id(f, "akantu-rs-bench:mn"), d)

    def test_host_keys_required_and_session_lock(self):
        """Strict host keys for the other hosts (no trust on first
        use) and one session per SESSION directory."""
        self.assertEqual(mn_cluster.known_host_names("10.0.0.2 ssh-ed25519 AAAA\n[10.0.0.3]:22 ssh-ed25519 B\n"
                                                     "|1|hashed ssh-ed25519 C\n# c\n"), {"10.0.0.2", "10.0.0.3"})
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            (d / "ssh").mkdir()
            (d / "cluster.conf").write_text("backend = ssh\nnet_cidr = 10.0.0.0/24\n")
            (d / "hosts").write_text("10.0.0.1 slots=8\n10.0.0.2 slots=8\n10.0.0.3 slots=8\n")
            (d / "ssh" / "hosts_known_hosts").write_text("10.0.0.2 ssh-ed25519 AAAA\n")
            c = mn_cluster.Cluster(d)
            self.assertIn("StrictHostKeyChecking=yes", c.ssh_opts())
            with self.assertRaisesRegex(mn_cluster.ClusterError, "10.0.0.3"):
                c.require_host_keys()
            (d / "ssh" / "hosts_known_hosts").write_text("10.0.0.2 ssh-ed25519 A\n10.0.0.3 ssh-ed25519 B\n")
            c.require_host_keys()
            self.assertFalse(mn_cluster.session_busy(d))
            import fcntl
            with open(d / "session.lock", "a") as f:
                fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
                self.assertTrue(mn_cluster.session_busy(d))
                self.assertEqual(mn_cluster.main(["up", str(d)]), 2)

    def test_liveness_strikes(self):
        strikes = {}
        # one or two unanswered probes do not lose a host; an answer resets the count
        self.assertEqual(mn_cluster.watch_step(strikes, {"a": "unreachable", "b": "running"}), [])
        self.assertEqual(mn_cluster.watch_step(strikes, {"a": "unreachable", "b": "running"}), [])
        self.assertEqual(mn_cluster.watch_step(strikes, {"a": "running", "b": "running"}), [])
        self.assertEqual(strikes["a"], 0)
        for _ in range(mn_cluster.STRIKES - 1):
            self.assertEqual(mn_cluster.watch_step(strikes, {"a": "unreachable"}), [])
        self.assertEqual(mn_cluster.watch_step(strikes, {"a": "unreachable"}), ["a"])
        # a definitive state loses the host at once
        for st in ("paused", "exited", "absent", "dead"):
            self.assertEqual(mn_cluster.watch_step({}, {"c": st}), ["c"], st)

    def test_host_peak(self):
        text = "\n".join([json.dumps({"header": True}), json.dumps({"t": 1, "procs": [[1, 0, 10, 10], [2, 0, 5, 5]]}),
                          json.dumps({"t": 2, "procs": [[1, 0, 12, 12]]})])
        self.assertEqual(mn_cluster.host_peak_kib(text), 15)

    def test_aws_plan(self):
        """The AWS plan parses, fits 8 hosts x 8 slots, its estimates fit its budget (5 h), and
        every line carries the full-size rerun's settings."""
        plan = mn_session.parse_plan((HERE.parent / "mn_plan_aws.txt").read_text())
        hosts = [(f"172.31.0.{k}", 8) for k in range(1, 9)]
        for e in plan:
            mn_cluster.placement(e["np"], e["hosts"], hosts)
            self.assertEqual(sorted(e["extra"]), SETTINGS, e["name"])
        self.assertTrue(plan[0]["required"])
        self.assertEqual(plan[-1]["name"], "joss32x8")            # dropped first
        self.assertLess(sum(e["estimate_h"] for e in plan) + 0.1, 5.0)
        self.assertEqual({e["mesh"] for e in plan}, {"h0.12", "joss", "h0.04", "h0.032", "h0.025"})

    def test_aws_smoke_plan(self):
        """The smoke session: every run @required with the settings, Eigen and MUMPS at
        h = 0.12 on 1 and 8 hosts on one partition, 64 ranks across 8 hosts, within 0.4 h."""
        plan = mn_session.parse_plan((HERE.parent / "mn_plan_aws_smoke.txt").read_text())
        hosts = [(f"172.31.0.{k}", 8) for k in range(1, 9)]
        byname = {e["name"]: e for e in plan}
        self.assertEqual(sorted(byname), ["s-eig-h1", "s-eig-h8", "s-mumps-h1", "s-mumps-h8", "s-p64"])
        for e in plan:
            mn_cluster.placement(e["np"], e["hosts"], hosts)
            self.assertTrue(e["required"], e["name"])
            self.assertEqual(sorted(e["extra"]), SETTINGS, e["name"])
        for n in ("s-eig-h8", "s-mumps-h1", "s-mumps-h8"):
            self.assertEqual(byname[n]["partition"], "run:s-eig-h1")
        self.assertEqual((byname["s-p64"]["np"], byname["s-p64"]["hosts"]), (64, 8))
        self.assertLess(sum(e["estimate_h"] for e in plan) + 0.1, 0.4)

    def test_plan(self):
        p = mn_session.parse_plan(PLAN)
        self.assertEqual([e["name"] for e in p], ["smoke", "big", "small"])
        self.assertTrue(p[0]["required"])
        self.assertEqual(p[2]["partition"], "run:smoke")
        for bad in ("a h0.12 8 4 10 auto scotch 0.1\na h0.12 8 4 10 auto scotch 0.1",   # repeated name
                    "a h0.12 1 1 10 auto scotch 0.1",          # NP = 1 needs none
                    "a h0.12 8 1 10 auto none 0.1",            # NP > 1 needs a partition
                    "a h0.12 6 4 10 auto scotch 0.1",          # NP not a multiple of HOSTS
                    "a h0.12 8 4 10 auto run:b 0.1\nb h0.12 8 4 10 auto scotch 0.1",   # later run
                    "a h0.12 8 4 10 mumps scotch 0.1", "a h0.12 8 4 10 auto scotch 0.1 key"):
            with self.assertRaises(ValueError):
                mn_session.parse_plan(bad)


class SessionRules(unittest.TestCase):
    def test_order_done_markers_and_partitions(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster()
            s, rc = run_session(tmp, PLAN, c)
            self.assertEqual(rc, 0)
            self.assertEqual(outcomes(s), [("smoke", 1, "ok"), ("big", 1, "ok"), ("small", 1, "ok")])
            self.assertEqual([x["name"] for x in c.calls], ["smoke-a1", "big-a1", "small-a1"])
            self.assertEqual(c.calls[2]["partition"], "file:/mn/work/runs/smoke-a1/partition.csv")
            self.assertEqual(c.calls[2]["hosts"], ["10.0.0.1"])
            self.assertEqual(c.calls[0]["mesh"], "/mn/work/meshes/cube-h0.12.msh")
            for n in ("smoke-a1", "big-a1", "small-a1"):
                d = json.loads((Path(tmp) / "results/runs" / n / "DONE").read_text())
                self.assertEqual(d["outcome"], "ok")
            st = json.loads((Path(tmp) / "results/state.json").read_text())
            self.assertEqual(st["status"], "finished")
            self.assertEqual(st["steps_done"], ["setup", "meshes", "partitions"])
            self.assertIn("/mn/work/meshes/cube-h0.12.msh", c.puts)

    def test_budget_skip_scaling_and_limits(self):
        with tempfile.TemporaryDirectory() as tmp:
            # smoke takes twice its estimate: "big" is scaled to 2.0 h, which no longer fits 1.9 h
            c = FakeCluster(scenario={"smoke-a1": {"hours": 0.1}})
            s, rc = run_session(tmp, PLAN, c, budget=1.9, extra=("--reserve-h", "0.1"))
            self.assertEqual(outcomes(s), [("smoke", 1, "ok"), ("big", 1, "skipped"), ("small", 1, "ok")])
            big = s.records()[1]
            self.assertAlmostEqual(big["scaled_estimate_h"], 2.0)
            # limit = min(remaining, max(2 B, B + 0.1 h)), B = max(estimate, scaled); smoke: B 0.05 -> 0.15 h
            self.assertAlmostEqual(c.calls[0]["timeout_s"], 0.15 * 3600)
            # small (scaled 0.1 h after smoke's ratio 2): max(0.2, 0.2) = 0.2 h
            self.assertAlmostEqual(c.calls[1]["timeout_s"], 0.2 * 3600, places=3)

    def test_fast_small_mesh_does_not_shrink_a_large_mesh(self):
        with tempfile.TemporaryDirectory() as tmp:
            plan = "a h0.12 8 4 10 auto scotch 0.10\nb h0.06 8 4 10 auto scotch 0.50\nc h0.06 8 2 10 auto scotch 0.50\n"
            # a: 0.01 h of a 0.10 h estimate (ratio 0.1); b: 1.0 h of 0.5 (ratio 2)
            c = FakeCluster(scenario={"a-a1": {"hours": 0.01}, "b-a1": {"hours": 1.0}})
            s, rc = run_session(tmp, plan, c, budget=10.0,
                                extra=("--meshes", mesh_tarball(Path(tmp) / "m.tar.gz")))
            # b: another mesh's ratio < 1 is not applied: scaled 0.5, limit max(1.0, 0.6) = 1.0 h
            self.assertAlmostEqual(s.records()[1]["scaled_estimate_h"], 0.5)
            self.assertAlmostEqual(c.calls[1]["timeout_s"], 1.0 * 3600)
            # c: b's ratio 2 on the same mesh: scaled 1.0, limit 2.0 h
            self.assertAlmostEqual(s.records()[2]["scaled_estimate_h"], 1.0)
            self.assertAlmostEqual(c.calls[2]["timeout_s"], 2.0 * 3600)

    def test_limit_capped_by_remaining_budget(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster()
            plan = "a h0.12 8 4 10 auto scotch 0.5\n"
            s, rc = run_session(tmp, plan, c, budget=0.9, extra=("--reserve-h", "0.1"))
            # remaining after set-up (60 s of up): 0.9 - 0.0167 - 0.1 = 0.783 h < 2 x 0.5
            self.assertAlmostEqual(c.calls[0]["timeout_s"] / 3600, 0.9 - 60 / 3600 - 0.1, places=6)

    def test_oom_recorded_not_repeated_and_implied_skips(self):
        with tempfile.TemporaryDirectory() as tmp:
            plan = """
            a h0.12 16 2 10 auto scotch 0.1
            b h0.12 32 2 10 auto scotch 0.1
            c h0.12 16 4 10 auto scotch 0.1
            d h0.12 8  2 10 auto scotch 0.1
            e h0.06 32 2 10 auto scotch 0.1
            """
            c = FakeCluster(scenario={"a-a1": {"outcome": "oom", "oom": ["10.0.0.2"]}})
            s, rc = run_session(tmp, plan, c, extra=("--meshes", mesh_tarball(Path(tmp) / "m.tar.gz")))
            self.assertEqual(rc, 0)
            self.assertEqual(outcomes(s), [("a", 1, "oom"), ("b", 1, "skipped"), ("c", 1, "ok"),
                                           ("d", 1, "ok"), ("e", 1, "ok")])
            self.assertEqual(s.records()[0]["oom_hosts"], ["10.0.0.2"])
            self.assertIn("oom-implied", s.records()[1]["note"])
            self.assertEqual([x["name"] for x in c.calls], ["a-a1", "c-a1", "d-a1", "e-a1"])

    def test_host_lost_restored_and_repeated(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(scenario={"big-a1": {"outcome": "host-lost", "lost": ["10.0.0.3"]}})
            s, rc = run_session(tmp, PLAN, c)
            self.assertEqual(outcomes(s), [("smoke", 1, "ok"), ("big", 1, "host-lost"), ("big", 2, "ok"),
                                           ("small", 1, "ok")])
            self.assertEqual(c.restored, ["10.0.0.3"])
            self.assertEqual(s.records()[1]["restored_hosts"], ["10.0.0.3"])
            self.assertEqual(c.calls[2]["hosts"], ["10.0.0.1", "10.0.0.2", "10.0.0.3", "10.0.0.4"])

    def test_dead_host_unavailable_and_remaining_hosts(self):
        with tempfile.TemporaryDirectory() as tmp:
            plan = """
            a h0.12 16 2 10 auto scotch 0.1
            b h0.12 32 4 10 auto scotch 0.1
            c h0.12 24 3 10 auto scotch 0.1
            """
            c = FakeCluster(scenario={"a-a1": {"outcome": "host-lost", "lost": ["10.0.0.2"]}},
                            restorable=["10.0.0.1", "10.0.0.3", "10.0.0.4"])
            s, rc = run_session(tmp, plan, c)
            self.assertEqual(outcomes(s), [("a", 1, "host-lost"), ("a", 2, "ok"), ("b", 1, "unavailable"),
                                           ("c", 1, "ok")])
            st = json.loads((Path(tmp) / "results/state.json").read_text())
            self.assertEqual(st["dead_hosts"], ["10.0.0.2"])
            self.assertEqual(c.calls[1]["hosts"], ["10.0.0.1", "10.0.0.3"])     # the repetition
            self.assertEqual(c.calls[2]["hosts"], ["10.0.0.1", "10.0.0.3", "10.0.0.4"])

    def test_host_lost_twice_dropped(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(scenario={"big-a1": {"outcome": "host-lost", "lost": ["10.0.0.4"]},
                                      "big-a2": {"outcome": "host-lost", "lost": ["10.0.0.4"]}})
            s, rc = run_session(tmp, PLAN, c)
            self.assertEqual(outcomes(s), [("smoke", 1, "ok"), ("big", 1, "host-lost"), ("big", 2, "dropped"),
                                           ("small", 1, "ok")])

    def test_failed_and_timeout_not_repeated(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(scenario={"big-a1": {"outcome": "timeout"}, "small-a1": {"outcome": "failed"}})
            s, rc = run_session(tmp, PLAN, c)
            self.assertEqual(rc, 0)
            self.assertEqual(outcomes(s), [("smoke", 1, "ok"), ("big", 1, "timeout"), ("small", 1, "failed")])

    def test_required_failure_stops(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(scenario={"smoke-a1": {"outcome": "failed"}})
            s, rc = run_session(tmp, PLAN, c)
            self.assertEqual(rc, 1)
            self.assertEqual(outcomes(s), [("smoke", 1, "failed")])
            self.assertTrue(s.state["status"].startswith("failed: required run smoke"))

    def test_partition_source_not_ok(self):
        with tempfile.TemporaryDirectory() as tmp:
            plan = "a h0.12 8 4 10 auto scotch 0.1\nb h0.12 8 2 10 auto run:a 0.1\n"
            c = FakeCluster(scenario={"a-a1": {"outcome": "failed"}})
            s, rc = run_session(tmp, plan, c)
            self.assertEqual(outcomes(s), [("a", 1, "failed"), ("b", 1, "unavailable")])

    def test_preflight_problem_stops_before_runs(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(problems=["rank 3 on h1: {'OPENBLAS_CORETYPE': 'unset'}"])
            s, rc = run_session(tmp, PLAN, c)
            self.assertEqual(rc, 1)
            self.assertEqual(c.calls, [])
            self.assertTrue(s.state["status"].startswith("failed: preflight"))

    def test_meshes_checked(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name, sha, want_rc in (("good", None, 0), ("bad", "0" * 64, 1)):
                tb = mesh_tarball(Path(tmp) / f"{name}.tar.gz", sha)
                sub = Path(tmp) / name
                sub.mkdir()
                c = FakeCluster()
                s, rc = run_session(sub, "a h0.06 8 4 10 auto scotch 0.1\n", c, extra=("--meshes", str(tb)))
                self.assertEqual(rc, want_rc, name)
                if want_rc == 0:
                    self.assertEqual(s.state["meshes"]["h0.06"]["tets"], 1)
                    self.assertIn("/mn/work/meshes/cube-h0.06.msh", c.puts)
            sub = Path(tmp) / "missing"
            sub.mkdir()
            s, rc = run_session(sub, "a h0.06 8 4 10 auto scotch 0.1\n", FakeCluster())
            self.assertEqual(rc, 1)
            self.assertIn("the plan needs ['h0.06']", s.state["status"])

    def test_hooks(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(scenario={"big-a1": {"outcome": "host-lost", "lost": ["10.0.0.3"]}})
            s, rc = run_session(tmp, PLAN, c, extra=("--fail-host", "big:3:stop:45", "--force-oom", "small:2",
                                                     "--oom-memory", "100m"))
            self.assertEqual(c.calls[1]["fail"], (2, "stop", 45.0))
            self.assertIsNone(c.calls[2]["fail"])           # not on the repetition
            self.assertEqual(c.memory, [("10.0.0.2", "100m"), ("10.0.0.2", "56g")])
            self.assertIn("test hook", s.records()[1]["note"])

    def test_resume(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = FakeCluster(scenario={"big-a1": {"outcome": "ok"}})
            s, rc = run_session(tmp, PLAN, c)
            a = make_args(tmp, PLAN)
            with redirect_stdout(io.StringIO()):
                self.assertEqual(mn_session.Session(a, FakeCluster(), clock=c.clock).run(), 2)   # refused
            c2 = FakeCluster()
            a = make_args(tmp, PLAN, extra=("--resume",))
            with redirect_stdout(io.StringIO()):
                self.assertEqual(mn_session.Session(a, c2, clock=c2.clock).run(), 0)
            self.assertEqual(c2.calls, [])                  # every run already final


if __name__ == "__main__":
    unittest.main()
