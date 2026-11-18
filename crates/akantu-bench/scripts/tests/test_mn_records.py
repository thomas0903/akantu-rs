# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of what the multi-node launcher records
# about its hosts and ranks (host descriptions, per-rank environment), of the
# package check of c1_images.sh export-mn-cpp, and of the comparisons' use of
# them (mn_compare.py: host descriptions printed, phase repetitions and
# environment judged), on synthetic records. Docker, SSH and the hosts are not
# used.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import io
import json
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import mn_cluster  # noqa: E402
import mn_compare  # noqa: E402
from test_mn_cpp import compare, cpp_ref, cpp_run  # noqa: E402
from test_mn_fix import make_run, pair  # noqa: E402


def describe(root):
    ns = {}
    exec(mn_cluster.HOST_PROBE, ns)  # noqa: S102 (the probe's own source text, as the node container runs it)
    return ns["describe_host"](str(root))


def write(root, rel, text):
    p = Path(root) / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)


def synthetic_host(root):
    """2 sockets x 2 cores x 2 threads (cpu c and c + 4 are the two threads of one core), 2 NUMA nodes."""
    write(root, "proc/cpuinfo", "processor\t: 0\nmodel name\t: Test CPU 9000\n\nprocessor\t: 1\n")
    write(root, "sys/devices/system/cpu/online", "0-7\n")
    for c in range(8):
        t = f"sys/devices/system/cpu/cpu{c}/topology/"
        write(root, t + "physical_package_id", f"{(c % 4) // 2}\n")
        write(root, t + "core_id", f"{c % 2}\n")
    for n, cpus in ((0, "0-1,4-5"), (1, "2-3,6-7")):
        write(root, f"sys/devices/system/node/node{n}/cpulist", cpus + "\n")
        write(root, f"sys/devices/system/node/node{n}/meminfo",
              f"Node {n} MemTotal:       {64 * 2 ** 20} kB\nNode {n} MemFree: 1 kB\n")
    write(root, "sys/devices/system/node/possible", "0-1\n")
    write(root, "proc/meminfo", f"MemTotal:       {128 * 2 ** 20} kB\nMemFree: 1 kB\n")
    write(root, "proc/sys/kernel/osrelease", "6.8.0-test\n")
    write(root, "proc/sys/kernel/numa_balancing", "1\n")
    write(root, "sys/kernel/mm/transparent_hugepage/enabled", "always [madvise] never\n")
    write(root, "proc/loadavg", "0.10 0.20 0.30 1/200 12345\n")


class HostDescription(unittest.TestCase):
    def test_synthetic_host(self):
        with tempfile.TemporaryDirectory() as tmp:
            synthetic_host(tmp)
            d = describe(tmp)
        self.assertEqual(d["cpu_model"], "Test CPU 9000")
        self.assertEqual((d["logical_cpus"], d["sockets"], d["cores"], d["threads_per_core"]), (8, 2, 4, 2))
        self.assertEqual(d["numa_nodes"], [{"node": 0, "cpus": "0-1,4-5", "mem_kib": 64 * 2 ** 20},
                                           {"node": 1, "cpus": "2-3,6-7", "mem_kib": 64 * 2 ** 20}])
        self.assertEqual(d["mem_total_kib"], 128 * 2 ** 20)
        self.assertEqual((d["kernel"], d["transparent_hugepage"], d["numa_balancing"], d["loadavg"]),
                         ("6.8.0-test", "madvise", "1", "0.10 0.20 0.30"))
        self.assertNotIn("hostname", json.dumps(d))
        line = mn_cluster.host_line(dict(d, docker="27.5.1"))
        for want in ("Test CPU 9000", "2 socket(s), 4 cores, 2 thread(s)/core", "node1 cpus 2-3,6-7 64 GiB",
                     "128 GiB", "THP madvise", "numa_balancing 1", "Docker 27.5.1"):
            self.assertIn(want, line)

    def test_missing_files_give_none(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = describe(tmp)
        self.assertEqual((d["cpu_model"], d["sockets"], d["numa_nodes"], d["numa_balancing"]), (None, None, [], None))

    def test_the_probe_runs_standalone(self):
        p = subprocess.run([sys.executable, "-I", "-c", mn_cluster.HOST_PROBE], capture_output=True, text=True,
                           check=False)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("kernel", json.loads(p.stdout))

    def test_describe_adds_the_docker_version(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            (d / "cluster.conf").write_text("backend = local\nnet_cidr = 10.9.0.0/24\nnetwork = mn-t\n")
            (d / "hosts").write_text("a slots=2\n")
            c = mn_cluster.Cluster(d)
            calls = []

            def call(argv, timeout=120, data=None, check=False):
                calls.append(argv)
                if "python3" in argv:
                    return 0, json.dumps({"cpu_model": "X", "kernel": "6.8"}).encode(), b""
                if argv[:2] == ["docker", "version"]:
                    return 0, b"27.5.1\n", b""
                return 1, b"", b""
            c.call = call
            self.assertEqual(c.describe(0), {"cpu_model": "X", "kernel": "6.8", "docker": "27.5.1"})
            self.assertIn(mn_cluster.HOST_PROBE, calls[0])


class RankEnvironment(unittest.TestCase):
    def test_record_and_parse(self):
        for v in mn_cluster.ENV_VARS:
            self.assertIn(f'echo "{v}=${{{v}:-unset}}";', mn_cluster.RECORD)
        self.assertIn('> "env_rank${OMPI_COMM_WORLD_RANK:-unset}.txt"', mn_cluster.RECORD)
        e = "AKA_SEED=1\nLD_LIBRARY_PATH=/build/install-strict/lib\nOMP_NUM_THREADS=1\n"
        env, differ = mn_cluster.env_from({"env_rank0.txt": e, "env_rank1.txt": e, "summary.json": "{}",
                                           "env_rank2.txt": e.replace("OMP_NUM_THREADS=1", "OMP_NUM_THREADS=4")})
        self.assertEqual(env["LD_LIBRARY_PATH"], "/build/install-strict/lib")
        self.assertEqual(differ, [2])
        self.assertEqual(mn_cluster.env_from({}), ({}, []))

    def test_remote_library_path_of_rust_runs(self):
        # code = rust does not forward LD_LIBRARY_PATH: PRRTE's remote launch sets one on the
        # other hosts' ranks; recorded, not a difference (code = cpp forwards it and compares it)
        head = "AKA_SEED=1\nLD_LIBRARY_PATH=unset\n"
        remote = "AKA_SEED=1\nLD_LIBRARY_PATH=/usr/lib:/usr/lib/x86_64-linux-gnu/pmix2/lib:\n"
        files = {"env_rank0.txt": head, "env_rank1.txt": remote}
        self.assertEqual(mn_cluster.env_from(files, mn_cluster.ENV_NOT_COMPARED["rust"])[1], [])
        self.assertEqual(mn_cluster.env_from(files, mn_cluster.ENV_NOT_COMPARED["cpp"])[1], [1])
        seed = {"env_rank0.txt": head, "env_rank1.txt": remote.replace("AKA_SEED=1", "AKA_SEED=2")}
        self.assertEqual(mn_cluster.env_from(seed, mn_cluster.ENV_NOT_COMPARED["rust"])[1], [1])


class Packages(unittest.TestCase):
    BASE = "libc6\t2.41-12\tamd64\nlibopenmpi40\t5.0.7-1\tamd64\npython3\t3.13.5-1\tamd64\n"

    def test_only_additions_pass(self):
        new = self.BASE + "openssh-server\t1:10.0p1-7\tamd64\n"
        self.assertEqual(mn_cluster.package_changes(self.BASE, new),
                         ([], [("openssh-server", "1:10.0p1-7", "amd64")]))

    def test_a_changed_or_removed_base_package_is_reported(self):
        new = self.BASE.replace("5.0.7-1", "5.0.7-2").replace("python3\t3.13.5-1\tamd64\n", "")
        changed, _ = mn_cluster.package_changes(self.BASE, new)
        self.assertEqual(changed, ["libopenmpi40:amd64 5.0.7-1 -> 5.0.7-2", "python3:amd64 3.13.5-1 -> absent"])


def set_status(d, **kw):
    st = mn_cluster.read_status(Path(d) / "status.txt")
    st.update(kw)
    mn_cluster.write_status(Path(d) / "status.txt", st)


HOST = {"cpu_model": "Test CPU 9000", "sockets": 1, "cores": 32, "threads_per_core": 1,
        "numa_nodes": [{"node": 0, "cpus": "0-31", "mem_kib": 128 * 2 ** 20}], "mem_total_kib": 128 * 2 ** 20,
        "kernel": "6.8.0-test", "transparent_hugepage": "madvise", "numa_balancing": "0", "loadavg": "0.0 0.0 0.0",
        "docker": "27.5.1"}


class CompareCpp(unittest.TestCase):
    def test_phase_repetitions_are_judged(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref, run = cpp_ref(Path(tmp) / "ref"), cpp_run(Path(tmp) / "run")
            (run / "chrono.csv").write_text('"psize", "solve_step", "solve_step nb_rep", "energies", '
                                            '"energies nb_rep"\n8, 4.7, 1499, 0.93, 1500\n')
            _, fails = compare(ref, run)
            self.assertIn("phase solve_step: 1500 vs 1499 repetitions", fails)
            self.assertEqual(mn_compare.chrono_row(mn_compare.Run(ref), reps=True),
                             {"solve_step": 1500, "energies": 1500})

    def test_environment_is_judged(self):
        renv = {"AKA_SEED": "1", "AKANTU_RS_FILL_NODES_TO_ELEMENTS": "joss", "LD_LIBRARY_PATH": "/build/install-strict/lib",
                "OMP_NUM_THREADS": "1", "OPENBLAS_NUM_THREADS": "1", "SCOTCH_PTHREAD_NUMBER": "1", "MKL_NUM_THREADS": None}
        with tempfile.TemporaryDirectory() as tmp:
            ref, run = cpp_ref(Path(tmp) / "ref"), cpp_run(Path(tmp) / "run")
            rj = json.loads((ref / "run.json").read_text())
            rj["environment"] = renv
            (ref / "run.json").write_text(json.dumps(rj))
            env = {k: v for k, v in renv.items() if v is not None}
            env["OPENBLAS_CORETYPE"] = "Haswell"   # not in run.json: not compared
            set_status(run, environment=env, environment_differs=[])
            out, fails = compare(ref, run)
            self.assertEqual(fails, [], out)
            set_status(run, environment=dict(env, AKANTU_RS_FILL_NODES_TO_ELEMENTS="upstream"))
            _, fails = compare(ref, run)
            self.assertTrue(any("environment AKANTU_RS_FILL_NODES_TO_ELEMENTS" in f for f in fails), fails)
            set_status(run, environment=env, environment_differs=[5])
            _, fails = compare(ref, run)
            self.assertTrue(any("ranks [5]" in f for f in fails), fails)

    def test_host_descriptions_printed(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref, run = cpp_ref(Path(tmp) / "ref"), cpp_run(Path(tmp) / "run")
            set_status(run, host_descriptions={"10.0.0.1": HOST, "10.0.0.2": HOST})
            out, _ = compare(ref, run)
            self.assertFalse(any("hosts of the reference" in ln for ln in out))   # the reference has none
            rj = json.loads((ref / "run.json").read_text())
            rj["host"] = {"cpu_model": "Big CPU", "logical_cpus": 128, "kernel": "6.8.0-big", "machine": "x86_64"}
            (ref / "run.json").write_text(json.dumps(rj))
            out, fails = compare(ref, run)
            self.assertEqual(fails, [])
            text = "\n".join(out)
            self.assertIn("hosts of the reference:\n    host: Big CPU; 128 logical CPUs; kernel 6.8.0-big", text)
            self.assertIn("host 2: Test CPU 9000; 1 socket(s), 32 cores", text)


class ComparePair(unittest.TestCase):
    def chrono(self, d, n):
        (Path(d) / "chrono.csv").write_text('"psize", "before_step", "before_step nb_rep", "solve_step", '
                                            f'"solve_step nb_rep"\n2, 0.1, 1, 4.7, {n}\n')

    def test_repetitions_and_hosts(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref, run = make_run(Path(tmp) / "a"), make_run(Path(tmp) / "b")
            self.chrono(ref, 1500)
            self.chrono(run, 1500)
            set_status(ref, host_descriptions={"x": HOST})
            set_status(run, host_descriptions={"h1": HOST, "h2": HOST})
            out, fails = pair(ref, run)
            self.assertEqual(fails, [], out)
            self.assertTrue(any("hosts of the reference" in ln for ln in out))
            self.chrono(run, 1400)
            out, fails = pair(ref, run)
            self.assertTrue(any("phase solve_step: 1500 vs 1400" in f for f in fails), fails)
            self.assertTrue(any(ln.startswith("  R ") for ln in out))

    def test_campaign_provenance_of_a_single_node_reference(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref = make_run(Path(tmp) / "camp" / "rust" / "np2" / "rep1")
            (Path(tmp) / "camp" / "provenance.txt").write_text(
                "campaign=x\nhost_cpu=Big CPU nproc=128 kernel=6.8\nmem_total_kib=1000\n")
            self.assertEqual(mn_compare.host_descriptions(mn_compare.Run(ref)),
                             [("host", "host_cpu=Big CPU nproc=128 kernel=6.8; mem_total_kib=1000")])

    def test_main_prints(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref, run = make_run(Path(tmp) / "a"), make_run(Path(tmp) / "b")
            buf = io.StringIO()
            with redirect_stdout(buf):
                rc = mn_compare.main(["mn_compare.py", "--pair", str(ref), str(run)])
            self.assertEqual(rc, 0, buf.getvalue())


if __name__ == "__main__":
    unittest.main()


class DiagnosticHooks(unittest.TestCase):
    def test_off_by_default(self):
        self.assertEqual(mn_cluster.diag_options({}), (None, [], {}))

    def test_hooks(self):
        driver, extra, rec = mn_cluster.diag_options({
            "MN_DRIVER": "/usr/local/lib/akv/diag", "MN_PRELOAD": "/usr/local/lib/libpmpistat.so",
            "MN_XENV": "AKANTU_DIAG_SKIP_AGREEMENTS=1 PMPI_BT=Iallgather"})
        self.assertEqual(driver, "/usr/local/lib/akv/diag")
        self.assertEqual(extra, ["-x", "LD_PRELOAD=/usr/local/lib/libpmpistat.so",
                                 "-x", "AKANTU_DIAG_SKIP_AGREEMENTS=1", "-x", "PMPI_BT=Iallgather"])
        self.assertEqual(rec["MN_XENV"], "AKANTU_DIAG_SKIP_AGREEMENTS=1 PMPI_BT=Iallgather")
        argv = mn_cluster.mpirun_argv(8, "/h", "/w", "10.0.0.0/8", "self", extra=extra)
        self.assertEqual(argv[-6:], extra)
        script = mn_cluster.run_script("/w", argv, ["sh", driver], driver=driver)
        self.assertIn("sha256sum /usr/local/lib/akv/diag > binary.sha256", script)

    def test_refused_values(self):
        for env in ({"MN_DRIVER": "relative/path"}, {"MN_PRELOAD": "/a b"}, {"MN_XENV": "lower=1"},
                    {"MN_XENV": "A=$(rm)"}):
            with self.assertRaises(mn_cluster.ClusterError):
                mn_cluster.diag_options(env)
