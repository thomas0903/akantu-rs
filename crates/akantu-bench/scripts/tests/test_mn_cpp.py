# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: offline unit tests of the multi-node launcher's C++ harness
# setting (`code = cpp` in mn_cluster.py: input, forwarded variables, early
# abort, preflight, outcome) and of the comparison with a single-node C++ run
# (mn_compare.py --cpp). Docker, SSH and the hosts are not used.
#
#   python3 -I -m unittest discover -s crates/akantu-bench/scripts/tests
import hashlib
import io
import json
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

TEMPLATE = mn_cluster.TEMPLATE.read_text()
RUN_ONE = mn_cluster.REPO / "benchmarks" / "cohesive-perf" / "scripts" / "in-container" / "run-one.sh"


def gnu_sed():
    """A GNU sed (run-one.sh's insertion uses `\\n` in a replacement, a GNU extension), or None."""
    for exe in ("sed", "gsed"):
        path = shutil.which(exe)
        if path:
            try:
                if "GNU" in subprocess.run([path, "--version"], capture_output=True, text=True, check=False).stdout:
                    return path
            except OSError:
                pass
    return None


def run_one_input(template_path, steps, mesh, extra, solver=""):
    """The input run-one.sh writes: its own lines from `src=` to the `sed ... > input.dat` line,
    run by bash with src = the template (the rest of run-one.sh needs the image)."""
    text = RUN_ONE.read_text().splitlines()
    i = next(k for k, ln in enumerate(text) if ln.startswith('src="/harness/inputs/'))
    j = next(k for k, ln in enumerate(text) if ln.startswith('sed "${sedargs[@]}" "${src}" > input.dat'))
    body = "\n".join(text[i + 1:j]) + '\nsed "${sedargs[@]}" "${src}"\n'
    sed = gnu_sed()
    script = f'set -eu\nsed() {{ "{sed}" "$@"; }}\nsrc="$1"; steps="$2"; mesh="$3"; shift 3\n' + body
    p = subprocess.run(["bash", "-c", script, "run-one-input", str(template_path), str(steps), mesh, *extra],
                       env={"PATH": "/usr/bin:/bin", "PERF_STATIC_SPARSE_SOLVER": solver},
                       capture_output=True, text=True, check=False)
    if p.returncode != 0:
        raise AssertionError(p.stderr)
    return p.stdout
SINGLE_NODE_EXTRA = ["blocked_node=global", "energy_precision=17", "output_energy=true", "export_partition=true"]
PINS = ["Haswell", "1", "1", "1", "1"]


class Conf(unittest.TestCase):
    def test_code_and_fnte(self):
        c = mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\n")
        self.assertEqual((c["code"], c["fnte"]), ("rust", "joss"))
        c = mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\ncode = cpp\nfnte = upstream\n")
        self.assertEqual((c["code"], c["fnte"]), ("cpp", "upstream"))
        for bad in ("code = fortran\n", "fnte = maybe\n"):
            with self.assertRaises(mn_cluster.ClusterError):
                mn_cluster.parse_conf("net_cidr = 10.0.0.0/8\n" + bad)


class Input(unittest.TestCase):
    def test_the_input_of_run_one(self):
        t = mn_cluster.input_text_cpp(TEMPLATE, 5000, "/mn/work/meshes/cube-joss.msh", "auto", SINGLE_NODE_EXTRA)
        lines = t.splitlines()
        i = lines.index("user section [")
        # keys absent from the upstream file: inserted in order right after `user section [`
        self.assertEqual(lines[i + 1:i + 4], ["  blocked_node = global", "  energy_precision = 17",
                                              "  export_partition = true"])
        # keys of the file: replaced in place
        self.assertIn("  output_energy = true", lines)
        self.assertEqual([ln for ln in lines if ln.startswith("  max_steps = ")], ["  max_steps = 5000"])
        self.assertEqual([ln for ln in lines if ln.startswith("  mesh = ")], ["  mesh = /mn/work/meshes/cube-joss.msh"])
        self.assertNotIn("rust_", t)
        self.assertNotIn("sparse_solver_type", t)
        # everything else is the upstream file, line for line
        src = [ln for ln in TEMPLATE.splitlines() if not ln.startswith(("  max_steps = ", "  mesh = ",
                                                                         "  output_energy = "))]
        got = [ln for ln in lines if not ln.startswith(("  max_steps = ", "  mesh = ", "  output_energy = ",
                                                        "  blocked_node = ", "  energy_precision = ",
                                                        "  export_partition = "))]
        self.assertEqual(got, src)

    @unittest.skipUnless(gnu_sed() and shutil.which("bash"), "needs bash and GNU sed (run-one.sh's sed)")
    def test_equals_what_run_one_writes(self):
        for steps, mesh, extra, solver in (
                (5000, "/mn/work/meshes/cube-joss.msh", SINGLE_NODE_EXTRA, "auto"),
                (1500, "/mn/work/meshes/cube-h0.12.msh", SINGLE_NODE_EXTRA[:3], "auto"),
                (10, "/m.msh", [*SINGLE_NODE_EXTRA, "shear=-1e6"], "eigen")):
            want = run_one_input(mn_cluster.TEMPLATE, steps, mesh, extra, "" if solver == "auto" else solver)
            self.assertEqual(mn_cluster.input_text_cpp(TEMPLATE, steps, mesh, solver, extra), want)

    def test_eigen_static_solve(self):
        t = mn_cluster.input_text_cpp(TEMPLATE, 10, "/m.msh", "eigen", [])
        lines = t.splitlines()
        self.assertEqual(lines[lines.index("    name = static") + 1], "    sparse_solver_type = eigen")


class Script(unittest.TestCase):
    def script(self, code):
        app = mn_cluster.app_argv("self", [mn_cluster.CPP_DRIVER if code == "cpp" else mn_cluster.DRIVER,
                                           "--aka_input_file", "input.dat"], record=True)
        return mn_cluster.run_script("/w", mn_cluster.mpirun_argv(8, "/h", "/w", "10.0.0.0/8", "self"), app,
                                     code, "joss")

    def test_cpp_environment_and_early_abort(self):
        s = self.script("cpp")
        self.assertIn(f"export AKANTU_RS_FILL_NODES_TO_ELEMENTS=joss LD_LIBRARY_PATH={mn_cluster.CPP_LIBDIR}", s)
        xs = next(ln for ln in s.splitlines() if ln.startswith("for v in "))
        for v in mn_cluster.PIN_VARS + mn_cluster.CPP_VARS:
            self.assertIn(v, xs)
        for m in mn_cluster.MUMPS_MARKERS:
            self.assertIn(m, s)
        self.assertIn("> aborted.txt", s)
        self.assertIn(f"sha256sum {mn_cluster.CPP_DRIVER} > binary.sha256", s)
        self.assertIn("bind-rank.sh 1 sh -c", s)
        self.assertIn(f"record {mn_cluster.CPP_DRIVER} --aka_input_file input.dat", s)

    def test_rust_unchanged(self):
        s = self.script("rust")
        self.assertNotIn("export AKANTU_RS_FILL_NODES_TO_ELEMENTS", s)
        xs = next(ln for ln in s.splitlines() if ln.startswith("for v in "))
        self.assertNotIn("LD_LIBRARY_PATH", xs)   # recorded per rank (env_rank<r>.txt), not forwarded
        self.assertNotIn("aborted.txt", s)
        self.assertIn(f"sha256sum {mn_cluster.DRIVER} > binary.sha256", s)


class Outcome(unittest.TestCase):
    def setUp(self):
        self.base = {"exit": 0, "summary": True, "nb_rank_summaries": 8, "np": 8, "case": "cpp"}

    def test_ud216_and_partition(self):
        self.assertEqual(mn_cluster.classify(dict(self.base)), "ok")
        self.assertEqual(mn_cluster.classify(dict(self.base, ud216_markers=1)), "failed")
        self.assertEqual(mn_cluster.classify(dict(self.base, aborted="MUMPS error", exit=143)), "failed")
        self.assertEqual(mn_cluster.classify(dict(self.base, partition_matches=False)), "failed")
        self.assertEqual(mn_cluster.classify(dict(self.base, partition_matches=True)), "ok")
        # the port's runs: as before (the comparison judges the partition)
        self.assertEqual(mn_cluster.classify(dict(self.base, case="rust", partition_matches=False)), "ok")
        self.assertEqual(mn_cluster.classify(dict(self.base, ud216_markers=1, lost_hosts=["h"])), "host-lost")


class Preflight(unittest.TestCase):
    def rows(self, n=4, fnte="joss"):
        return [[r, fnte, mn_cluster.CPP_LIBDIR, "ab" * 32, "0", mn_cluster.CPP_LIBDIR + "/libakantu.so.5.0"]
                for r in range(n)]

    def test_cpp_lines(self):
        self.assertEqual(mn_cluster.cpp_check_problems(self.rows(), 4, "joss"), [])
        self.assertTrue(mn_cluster.cpp_check_problems(self.rows(3), 4, "joss"))
        self.assertTrue(mn_cluster.cpp_check_problems(self.rows(fnte="unset"), 4, "joss"))
        r = self.rows()
        r[2][4] = "1"
        self.assertTrue(any("unresolved" in p for p in mn_cluster.cpp_check_problems(r, 4, "joss")))
        r = self.rows()
        r[3][3] = "cd" * 32
        self.assertTrue(any("differs between ranks" in p for p in mn_cluster.cpp_check_problems(r, 4, "joss")))

    def test_check_with_code_cpp(self):
        good = [(0, "a", "0"), (1, "a", "1"), (2, "b", "0"), (3, "b", "1")]
        out = "".join(f"MNCHECK {h} {r} {' '.join(PINS)} {c} {r % 2} {c}\n" for r, h, c in good)
        out += "".join(f"MNCPP {r} joss {mn_cluster.CPP_LIBDIR} {'ab' * 32} 0 {mn_cluster.CPP_LIBDIR}/libakantu.so.5.0\n"
                       for r in range(4)) + "0\n"
        scripts = []
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            (d / "cluster.conf").write_text("backend = local\nnet_cidr = 10.9.0.0/24\nnetwork = mn-t\ncode = cpp\n")
            (d / "hosts").write_text("a slots=2\nb slots=2\n")
            c = mn_cluster.Cluster(d)
            c.alive = lambda k: "running"
            c.ssh_from_head = lambda k: True
            c.image_id = lambda k: "rootfs:x"

            def call(argv, timeout=120, data=None, check=False):
                if argv[-3] == "bash" and argv[-2] == "-c":
                    scripts.append(argv[-1])
                if "cat" in argv and any(str(x).endswith("/out.txt") for x in argv):
                    return 0, out.encode(), b""
                return 0, b"", b""
            c.call = call
            r = c.check()
        self.assertEqual(r["problems"], [])
        self.assertEqual(len(r["cpp"]), 4)
        self.assertIn("MNCPP", scripts[0])
        self.assertIn("LD_LIBRARY_PATH=/build/install-strict/lib", scripts[0])


def cpp_ref(d, np_=8, binary="ab" * 32, part=b"elem,rank\n0,0\n", mesh="/perf/meshes/frozen-cube-h0.12.msh",
            inp="user section [\n  mesh = M\n]\n"):
    """A single-node C++ run directory of c1prep_campaign.sh."""
    d = Path(d)
    d.mkdir(parents=True)
    mn_cluster.write_status(d / "status.txt", {"case": "cpp", "np": np_, "exit": 0, "aborted": None,
                                               "ud216_markers": 0, "partition_matches": True, "outcome": "ok"})
    (d / "run.json").write_text(json.dumps({"binary_sha256": binary, "ranks": np_, "fill_nodes_to_elements": "joss",
                                            "static_sparse_solver": None, "wall_s": 10.0}))
    common(d, np_, mesh, part, inp)
    return d


def common(d, np_, mesh, part, inp):
    (d / "summary.json").write_text(json.dumps({
        "psize": np_, "mesh": mesh, "nb_global_nodes_start": 4824, "nb_regular_elements_dim3": 23156,
        "max_steps": 1500, "fill_nodes_to_elements": "joss", "init_time_s": 0.7, "loop_time_s": 14.0,
        "time_step": 1e-7, "first_insertion_step": 10, "nb_cohesive_elements": 405}))
    (d / "partition.csv").write_bytes(part)
    (d / "input.dat").write_text(inp.replace("M", mesh))
    (d / "chrono.csv").write_text('"psize", "solve_step", "solve_step nb_rep", "energies", "energies nb_rep"\n'
                                  f"{np_}, 4.7, 1500, 0.93, 1500\n")
    (d / "energies.csv").write_text("step, ed, ep, ek, ew, et\n0, 0, 1, 2, 3, 4\n")
    (d / "insertions.csv").write_text("step, nb_inserted, nb_cohesive_total\n10, 1, 1\n")


def cpp_run(d, np_=8, binary="ab" * 32, part=b"elem,rank\n0,0\n", inp="user section [\n  mesh = M\n]\n"):
    """A launcher run directory with code = cpp on 2 hosts."""
    d = Path(d)
    d.mkdir(parents=True)
    per = np_ // 2
    mn_cluster.write_status(d / "status.txt", {
        "case": "cpp", "np": np_, "nb_hosts": 2, "hosts": ["h1", "h2"], "ranks_per_host": per,
        "rank_hosts": {"h1": list(range(per)), "h2": list(range(per, np_))}, "solver": "auto", "fnte": "joss",
        "partition": "file:/mn/work/partitions/p.csv", "partition_matches": True,
        "partition_sha256": hashlib.sha256(part).hexdigest(), "binary_sha256": binary, "binding_problems": [],
        "aborted": None, "ud216_markers": 0, "wall_s": 12.0, "outcome": "ok"})
    common(d, np_, "/mn/work/meshes/cube-h0.12.msh", part, inp)
    return d


def compare(ref, run):
    out, fails = [], []
    mn_compare.compare_cpp(mn_compare.Run(ref), mn_compare.Run(run), out, fails)
    return out, fails


class Compare(unittest.TestCase):
    def test_pass_and_phases(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, fails = compare(cpp_ref(Path(tmp) / "ref"), cpp_run(Path(tmp) / "run"))
            self.assertEqual(fails, [], out)
            self.assertTrue(any(ln.split()[:1] == ["solve_step"] for ln in out), out)
            self.assertEqual(mn_compare.chrono_row(mn_compare.Run(Path(tmp) / "ref")),
                             {"solve_step": 4.7, "energies": 0.93})

    def test_failures(self):
        cases = {"driver": {"binary": "cd" * 32}, "partition": {"part": b"elem,rank\n0,1\n"},
                 "input": {"inp": "user section [\n  mesh = M\n  max_steps = 9\n]\n"}}
        for what, kw in cases.items():
            with tempfile.TemporaryDirectory() as tmp:
                _, fails = compare(cpp_ref(Path(tmp) / "ref"), cpp_run(Path(tmp) / "run", **kw))
                self.assertTrue(any(what in f for f in fails), (what, fails))
        with tempfile.TemporaryDirectory() as tmp:
            run = cpp_run(Path(tmp) / "run")
            st = mn_cluster.read_status(run / "status.txt")
            st.update(outcome="failed", ud216_markers=2, failure="UD-216")
            mn_cluster.write_status(run / "status.txt", st)
            _, fails = compare(cpp_ref(Path(tmp) / "ref"), run)
            self.assertTrue(any("UD-216" in f for f in fails), fails)

    def test_main(self):
        with tempfile.TemporaryDirectory() as tmp:
            ref, run = cpp_ref(Path(tmp) / "ref"), cpp_run(Path(tmp) / "run")
            buf = io.StringIO()
            with redirect_stdout(buf):
                rc = mn_compare.main(["mn_compare.py", "--cpp", str(ref), str(run)])
            self.assertEqual(rc, 0, buf.getvalue())
            self.assertIn("CPP-PAIR: PASS", buf.getvalue())


class Plan(unittest.TestCase):
    def test_the_cpp_plan(self):
        plan = mn_session.parse_plan((HERE.parent / "mn_plan_final_cpp.txt").read_text())
        got = [(r["name"], r["mesh"], r["np"], r["hosts"], r["steps"], r["partition"], r["estimate_h"], r["required"])
               for r in plan]
        self.assertEqual(got, [("smoke-h012", "h0.12", 8, 4, 1500, "scotch", 0.03, True),
                               ("joss128x4", "joss", 128, 4, 5000, "file:partition-np128.csv", 1.0, False),
                               ("joss64x2", "joss", 64, 2, 5000, "file:partition-np64.csv", 1.3, False)])
        self.assertTrue(all(r["extra"] == [] and r["solver"] == "auto" for r in plan))


if __name__ == "__main__":
    unittest.main()


class FailureReasons(unittest.TestCase):
    P = "file:/mn/work/partitions/p.csv"

    def test_partition_reasons(self):
        st = {"partition_matches": False, "partition_input_sha256": "ab"}
        self.assertEqual(mn_cluster.failure_reasons(st, "cpp", self.P, True),
                         ["partition: the harness's partition.csv differs from /mn/work/partitions/p.csv"])
        st["partition_input_sha256"] = None
        self.assertEqual(mn_cluster.failure_reasons(st, "cpp", self.P, True),
                         ["partition: /mn/work/partitions/p.csv is missing on the head (nothing to compare with)"])
        self.assertEqual(mn_cluster.failure_reasons(st, "cpp", self.P, False),
                         ["partition: the harness exported no partition.csv"])
        self.assertEqual(mn_cluster.failure_reasons(st, "rust", self.P, True), [])

    def test_other_reasons(self):
        st = {"binding_problems": ["rank 1 on h: CPUs 0-1, expected one"], "ud216_markers": 2, "aborted": "x"}
        self.assertEqual(mn_cluster.failure_reasons(st, "cpp", "scotch", True),
                         ["binding: rank 1 on h: CPUs 0-1, expected one",
                          "UD-216: MUMPS error or warning (2 marker(s), run stopped early)"])
        self.assertEqual(mn_cluster.failure_reasons(st, "rust", "scotch", True),
                         ["binding: rank 1 on h: CPUs 0-1, expected one"])
