#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the full-size (C1) session of the cohesive
# scalability benchmark (C++ harness and Rust driver, one x86_64 Linux host
# with Docker, from the tarballs of c1_images.sh export). Started through
# c1_session.sh (detached); standard library only.
"""The full-size (C1) session: one run at a time, in this order.

    python3 -I c1_session.py run    DIR MEMORY CPUS RESULTS BUDGET_H [options]
    python3 -I c1_session.py check  DIR MEMORY CPUS RESULTS BUDGET_H [options]
    python3 -I c1_session.py status RESULTS

Order (RANKS = "P1 P2 P3", default "64 32 8"):

 1. setup   c1_images.sh smoke DIR: checksums, image load and ids, oracle
            installs volume, repository subset (RESULTS/smoke/repo), C++
            harness build, the campaign tool on h = 0.12 at 2 ranks (counts).
 2. mesh    the mesh tarball (default DIR/akantu-c1-mesh-joss.tar.gz:
            cube-joss.msh + cube-joss.json) extracted to RESULTS/mesh, its
            SHA-256 checked against the JSON, copied into the perf volume
            (the campaign tool copies it into the Rust volume per run).
 3. test    the campaign tool at P1 ranks, TEST_STEPS (20) steps, Rust then
            C++, on the mesh: it exports the P1 partition first (C++ 1-step
            run with export_partition = true, stopped once partition.csv is
            complete) and checks that MEMORY / CPUS took effect in the
            containers (cgroup memory.max, cpu.max). A run that fails for
            another reason than an out-of-memory kill or an upstream MUMPS
            retry (UD-216) stops the session here (launcher or setting
            wrong). An out-of-memory kill counts as the code's attempt at P1:
            that code falls back to P2 (no attempt at P1).
 4. exports the partition-export runs for P2 and P3 (same kind of C++ run);
            with the test's P1 export these are all the export runs.
 5. Rust P1, C++ P1 (STEPS = 5000 steps each).
 6. gate    prints every finished run's measured hours against the
            estimate table and its container memory peak against MEMORY and
            the host's RAM, then projects elapsed + the remaining runs
            (estimates scaled by each code's measured / estimated ratio;
            C++ P3 excluded, see 9) + RESERVE_H; stops the session if that
            exceeds BUDGET_H.
 7. Rust P2, C++ P2
 8. Rust P3
 9. C++ P3, last; like every run it is skipped when the remaining budget
    (BUDGET_H − elapsed − RESERVE_H) is smaller than its scaled estimate.

Rules:
 * one run at a time (the campaign tool enforces it: lock + no foreign
   `cohesive_extrinsic` process; its memory sampler is host-wide);
 * an out-of-memory kill at P1 (in the test run or in the timed run) falls
   back to P2 for that code: its P2 run follows at once (before the gate,
   which then has a measurement of that code) and there is no second
   attempt at P1; an out-of-memory kill at P2 or P3 is recorded, not
   repeated;
 * a C++ run that hits upstream's MUMPS retry path (UD-216: the harness
   stops it, or its logs carry a MUMPS marker) is repeated at most once per
   point, then the point is dropped; the Rust driver's MUMPS retries are
   internal (UD-216 fixed) and need no repetition;
 * any other failed run is recorded and not repeated;
 * after every campaign (test, export, run) its directory under
   RESULTS/campaigns/ holds the run directories (summary, energies,
   insertions, per-phase timings, memory samples and cgroup peaks, logs,
   status.txt) and a DONE marker (JSON: outcomes, hours); RESULTS/state.json
   holds the session state, RESULTS/session.log the log;
 * `status` prints where the session is.

Options:
  --ranks "P1 P2 [P3]"   two or three decreasing rank counts (default "64 32 8")
  --codes "rust cpp"     which codes run (default both; "rust": the port only,
                         e.g. the full-size rerun of D-105 (4) / D-108)
  --rust-settings "K=V ..." | none
                         user-section settings of every Rust run (default
                         "rust_cohesive_compat=upstream
                         rust_doubled_node_load=follows_elements": upstream's
                         laws with the corrected doubled-node load placement,
                         UD-1280); every Rust run's summary.json must record
                         each of them, else the session is refused there.
                         `none` passes nothing: the driver's defaults, i.e.
                         C1's runs of 2026-11-12 (reproducible with it)
  --partitions DIR       DIR/partition-np<P>.csv for every P (e.g. C1's
                         exported 64- and 32-rank partitions): replayed by
                         every run, no partition-export run (step 4 skipped)
  --steps N              steps of the runs (default 5000)
  --test-steps N         steps of the test run (default 20)
  --mesh-tarball FILE    (default DIR/akantu-c1-mesh-joss.tar.gz)
  --estimates FILE       hours per run, lines "rust@64 0.64" (default: the
                         table below, for 64 / 32 / 8 ranks)
  --reserve-h H          kept free at the end of the budget (default 0.15)
  --volumes PREFIX       Docker volumes PREFIX-oracle-build, PREFIX-perf,
                         PREFIX-rust (default c1; must be new or empty)
  --resume               continue a session whose RESULTS/state.json exists
Test hooks (the end-to-end test on a small mesh, docs/TESTING.md):
  --force-oom CODE@P     that point's first attempt runs with --oom-memory
                         (a real out-of-memory kill)
  --oom-memory M         (default 400m)
  --force-retry cpp@P[:N]  inject upstream's MUMPS retry marker into the
                         first N (default 1) attempts of that C++ point (the
                         harness's watcher then stops the run as it does for
                         a real UD-216 retry)

Default estimates (hours on a 64-core node, 5,000 steps on the JOSS mesh;
docs/BENCHMARKS.md run matrix with +30 %, plus +50 % of the loop for memory
bandwidth at 64 / 32 ranks and the UD-216 allowance on C++ P = 8).
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tarfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
DEFAULT_RUST_SETTINGS = "rust_cohesive_compat=upstream rust_doubled_node_load=follows_elements"
DEFAULT_ESTIMATES = {"rust@64": 0.64, "cpp@64": 0.74, "rust@32": 0.79, "cpp@32": 1.17,
                     "rust@8": 1.63, "cpp@8": 2.77}
MARKER = "MUMPS dynamic memory is insufficient (c1_session.py --force-retry test hook)"
FINAL = ("ok", "oom", "failed", "dropped", "skipped", "unavailable", "refused")


def utc(t=None):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t))


def log(msg):
    print(f"{utc()} {msg}", flush=True)


def mem_bytes(m):
    """Docker memory string (binary units) to bytes."""
    mo = re.fullmatch(r"([1-9][0-9]*)([bkmg]?)", m)
    if not mo:
        raise ValueError(f"memory must look like 480g / 12g / 400m, got {m!r}")
    return int(mo.group(1)) * {"": 1, "b": 1, "k": 1024, "m": 1024 ** 2, "g": 1024 ** 3}[mo.group(2)]


def parse_args(argv):
    p = argparse.ArgumentParser(prog="c1_session.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("dir")
    p.add_argument("memory")
    p.add_argument("cpus", type=int)
    p.add_argument("results")
    p.add_argument("budget_h", type=float)
    p.add_argument("--ranks", default="64 32 8")
    p.add_argument("--codes", default="rust cpp")
    p.add_argument("--rust-settings", default=DEFAULT_RUST_SETTINGS)
    p.add_argument("--partitions")
    p.add_argument("--steps", type=int, default=5000)
    p.add_argument("--test-steps", type=int, default=20)
    p.add_argument("--mesh-tarball")
    p.add_argument("--estimates")
    p.add_argument("--reserve-h", type=float, default=0.15)
    p.add_argument("--volumes", default="c1")
    p.add_argument("--resume", action="store_true")
    p.add_argument("--force-oom", action="append", default=[])
    p.add_argument("--oom-memory", default="400m")
    p.add_argument("--force-retry", action="append", default=[])
    a = p.parse_args(argv)
    a.ranks = [int(x) for x in a.ranks.split()]
    if (len(a.ranks) not in (2, 3) or sorted(set(a.ranks), reverse=True) != a.ranks or min(a.ranks) < 2):
        p.error("--ranks needs two or three decreasing rank counts > 1, e.g. \"64 32 8\" or \"64 32\"")
    codes = a.codes.split()
    if not codes or len(set(codes)) != len(codes) or not set(codes) <= {"rust", "cpp"}:
        p.error("--codes: rust, cpp or \"rust cpp\"")
    a.codes = [c for c in ("rust", "cpp") if c in codes]
    a.rust_settings = [] if a.rust_settings == "none" else a.rust_settings.split()
    for kv in a.rust_settings:
        if not re.fullmatch(r"rust_[a-z0-9_]+=[A-Za-z0-9_.:/+-]+", kv):
            p.error(f"--rust-settings: rust_<key>=<value> items or none, got {kv!r}")
    a.rust_expect = dict(kv.split("=", 1) for kv in a.rust_settings)
    if a.partitions:
        a.partitions = str(Path(a.partitions).resolve())
    mem_bytes(a.memory)
    mem_bytes(a.oom_memory)
    if a.cpus < 1 or a.budget_h <= 0 or a.steps < 1 or a.test_steps < 1:
        p.error("CPUS, BUDGET_H, --steps and --test-steps must be positive")
    if not re.fullmatch(r"[a-z][a-z0-9]{0,7}", a.volumes):
        p.error("--volumes: [a-z][a-z0-9]{0,7}")
    a.dir = str(Path(a.dir).resolve())
    a.results = str(Path(a.results).resolve())
    a.mesh_tarball = str(Path(a.mesh_tarball or Path(a.dir) / "akantu-c1-mesh-joss.tar.gz").resolve())
    est = dict(DEFAULT_ESTIMATES)
    if a.estimates:
        est = {}
        for line in Path(a.estimates).read_text().splitlines():
            if line.strip() and not line.startswith("#"):
                k, v = line.split()
                est[k] = float(v)
    a.est = est
    a.oom_points = set(a.force_oom)
    a.retry_points = {}
    for r in a.force_retry:
        k, _, n = r.partition(":")
        if not k.startswith("cpp@"):
            p.error("--force-retry applies to C++ points (cpp@P)")
        a.retry_points[k] = int(n or 1)
    return a


class SettingsRefused(Exception):
    """A Rust run's summary.json does not record the --rust-settings it was given."""


class Session:
    def __init__(self, a, runner=None):
        self.a = a
        self.res = Path(a.results)
        self.state_path = self.res / "state.json"
        self.campaigns = self.res / "campaigns"
        self.partdir = self.res / "partitions"
        self.prefix = "c1s"
        self.ovol, self.pvol, self.rvol = (f"{a.volumes}-oracle-build", f"{a.volumes}-perf", f"{a.volumes}-rust")
        self.runner = runner or self._run_cmd  # replaced in the unit tests
        self.clock = time.time
        self.state = {}

    # ---- state ------------------------------------------------------------------
    def save(self):
        self.state["updated"] = utc(self.clock())
        tmp = self.state_path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.state, indent=1))
        tmp.replace(self.state_path)

    def elapsed_h(self):
        return (self.clock() - self.state["started_epoch"]) / 3600

    def remaining_h(self):
        return self.a.budget_h - self.elapsed_h() - self.a.reserve_h

    def set_step(self, step):
        self.state["step"] = step
        self.state["step_since"] = utc(self.clock())
        self.save()
        log(f"== {step} (elapsed {self.elapsed_h():.2f} h of {self.a.budget_h} h)")

    def done(self, step):
        return step in self.state.get("steps_done", [])

    def mark(self, step):
        self.state.setdefault("steps_done", []).append(step)
        self.save()

    # ---- external commands ------------------------------------------------------
    def env(self):
        e = dict(os.environ)
        e.update({"ORACLE_VOLUME": self.ovol, "PERF_VOLUME": self.pvol, "RUST_VOLUME": self.rvol,
                  "OUT": str(self.campaigns), "FNTE": "joss", "CPP_CONFIG": "strict", "AKA_SEED": "1",
                  "SCOTCH_PTHREAD_NUMBER": "1", "C1_CONTAINER_PREFIX": self.prefix})
        return e

    def _run_cmd(self, cmd, logfile, env=None):
        with open(logfile, "a") as f:
            f.write(f"$ {' '.join(cmd)}\n")
            f.flush()
            return subprocess.run(cmd, env=env or self.env(), stdout=f, stderr=subprocess.STDOUT,
                                  stdin=subprocess.DEVNULL).returncode

    def tool(self):
        return self.res / "smoke" / "repo" / "crates" / "akantu-bench" / "scripts" / "c1prep_campaign.sh"

    def campaign(self, name, ranks, steps, cases, memory=None, partitions=True, export=False):
        """Runs the campaign tool; returns {case: status dict}, hours, exit code."""
        cmd = ["bash", str(self.tool()), "--memory", memory or self.a.memory, "--cpus", str(self.a.cpus),
               "--rust-image", "akantu-rs-bench:c1", "--rust-bin", "/usr/local/bin/cohesive_extrinsic",
               "--rust-volume", self.rvol]
        if "rust" in cases and self.a.rust_settings:
            cmd += ["--rust-extra", " ".join(self.a.rust_settings)]
        if partitions:
            cmd += ["--partitions", str(self.partdir)]
        if export:
            cmd += ["--export-partition", name, self.state["mesh_name"], str(ranks)]
        else:
            cmd += [name, self.state["mesh_name"], str(steps), str(ranks), "1", " ".join(cases)]
        t0 = self.clock()
        rc = self.runner(cmd, self.campaigns / f"{name}.log")
        hours = (self.clock() - t0) / 3600
        st = {}
        for c in cases:
            f = self.campaigns / name / c / f"np{ranks}" / "rep1" / "status.txt"
            st[c] = read_status(f)
        return st, hours, rc

    def settings_problems(self, name, ranks):
        """The --rust-settings missing from the Rust run's summary.json (the
        full-size rerun must run the corrected placement, not the driver's defaults)."""
        if not self.a.rust_expect:
            return []
        f = self.campaigns / name / "rust" / f"np{ranks}" / "rep1" / "summary.json"
        try:
            summ = json.loads(f.read_text())
        except (OSError, ValueError):
            return [f"no readable {f}"]
        return [f"{k} = {summ.get(k)!r} in {f}, expected {v!r}" for k, v in self.a.rust_expect.items()
                if summ.get(k) != v]

    def write_done(self, name, record):
        d = self.campaigns / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "DONE").write_text(json.dumps(record, indent=1) + "\n")

    # ---- steps ------------------------------------------------------------------
    def step_setup(self):
        self.set_step("setup: c1_images.sh smoke")
        env = self.env()
        env.update({"WORK": str(self.res / "smoke"), "SMOKE_MEMORY": "8g", "SMOKE_CPUS": "2"})
        t0 = self.clock()
        rc = self.runner(["bash", str(HERE / "c1_images.sh"), "smoke", self.a.dir], self.res / "smoke.log", env)
        log(f"smoke exit {rc} after {(self.clock() - t0) / 60:.1f} min ({self.res / 'smoke.log'})")
        if rc != 0 or not self.tool().exists():
            return self.fail(f"smoke failed (exit {rc}): see {self.res / 'smoke.log'}")
        self.mark("setup")
        return True

    def step_mesh(self):
        self.set_step("mesh")
        mdir = self.res / "mesh"
        mdir.mkdir(exist_ok=True)
        with tarfile.open(self.a.mesh_tarball) as t:
            names = [m.name for m in t.getmembers()]
            msh = [n for n in names if re.fullmatch(r"cube-[a-z][a-z0-9]{0,15}\.msh", n)]
            if len(msh) != 1 or msh[0][:-4] + ".json" not in names or len(names) != 2:
                return self.fail(f"mesh tarball {self.a.mesh_tarball}: expected cube-<name>.msh + .json, got {names}")
            for m in t.getmembers():
                if not m.isfile():
                    return self.fail(f"mesh tarball: {m.name} is not a regular file")
                with t.extractfile(m) as src, open(mdir / m.name, "wb") as dst:
                    shutil.copyfileobj(src, dst, 1 << 22)
        mesh = mdir / msh[0]
        stats = json.loads((mdir / (msh[0][:-4] + ".json")).read_text())
        h = hashlib.sha256()
        with open(mesh, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 22), b""):
                h.update(chunk)
        if h.hexdigest() != stats.get("sha256"):
            return self.fail(f"mesh {mesh}: sha256 {h.hexdigest()} != {stats.get('sha256')} (its JSON)")
        name = msh[0][len("cube-"):-4]
        rc = self.runner(["bash", "-c", "set -euo pipefail; . \"$1\"; perf_volume_write \"$2\" < \"$3\"", "_",
                          str(self.res / "smoke/repo/benchmarks/cohesive-perf/scripts/common.sh"),
                          f"/perf/meshes/{msh[0]}", str(mesh)], self.res / "mesh.log")
        if rc != 0:
            return self.fail(f"mesh: copy into the perf volume failed (exit {rc})")
        self.state.update({"mesh_name": name, "mesh_sha256": stats["sha256"],
                           "mesh_elements": stats.get("elements_total"), "mesh_tets":
                           (stats.get("elements_by_type") or {}).get("tetrahedron4")})
        log(f"mesh cube-{name}.msh sha256 {stats['sha256'][:16]}… {stats.get('nodes')} nodes, "
            f"{self.state['mesh_tets']} tetrahedra: in volume {self.pvol}")
        self.mark("mesh")
        return True

    def check_limits(self, st):
        """MEMORY and CPUS as the containers saw them (cgroup v2)."""
        want_mem = str(mem_bytes(self.a.memory))
        problems = []
        for c, s in st.items():
            if s.get("outcome") in (None, "failed") or s.get("memory_max") is None:
                continue
            if str(s.get("memory_max")).strip() != want_mem:
                problems.append(f"{c}: memory.max {s.get('memory_max')} != {want_mem} ({self.a.memory})")
            if c == "rust" and str(s.get("cpu_max")).strip() != f"{self.a.cpus * 100000} 100000":
                problems.append(f"rust: cpu.max {s.get('cpu_max')} != {self.a.cpus * 100000} 100000")
        return problems

    def step_test(self):
        p1 = self.a.ranks[0]
        name = f"test-np{p1}"
        self.set_step(f"test: campaign tool, {self.a.test_steps} steps, {p1} ranks, {' then '.join(self.a.codes)}")
        st, hours, rc = self.campaign(name, p1, self.a.test_steps, self.a.codes, partitions=bool(self.a.partitions))
        part = self.campaigns / name / f"partition-np{p1}.csv"
        if self.a.partitions:   # replayed, not exported: the given file
            part = self.partdir / f"partition-np{p1}.csv"
        self.write_done(name, {"step": "test", "outcomes": {c: s.get("outcome") for c, s in st.items()},
                               "hours": hours, "exit": rc})
        outcomes = ", ".join(f"{c} {s.get('outcome')}" for c, s in st.items())
        log(f"test: {outcomes} ({hours:.3f} h, campaign exit {rc})")
        self.state.setdefault("test", {}).update({c: s for c, s in st.items()})
        if not part.exists():
            return self.fail(f"test: no {p1}-rank partition exported ({self.campaigns / (name + '.log')})")
        self.partdir.mkdir(exist_ok=True)
        if part.parent != self.partdir:
            shutil.copyfile(part, self.partdir / part.name)
        if "rust" in st and st["rust"].get("outcome") == "ok":
            bad = self.settings_problems(name, p1)
            if bad:
                return self.fail("test: the Rust driver did not record --rust-settings (refused): " + "; ".join(bad))
        bad = [c for c, s in st.items() if s.get("outcome") not in ("ok", "oom", "ud216")]
        if bad:
            return self.fail(f"test: {', '.join(bad)} failed (not OOM, not UD-216): launcher or setting wrong; "
                             f"see {self.campaigns / name}")
        problems = self.check_limits(st)
        if problems:
            return self.fail("test: container limits differ from the arguments: " + "; ".join(problems))
        for c, s in st.items():
            if s.get("outcome") == "oom":
                self.state.setdefault("fallback", {})[c] = f"out-of-memory kill in the {p1}-rank test run"
                log(f"test: {c} was OOM-killed at {p1} ranks: {c} falls back to {self.a.ranks[1]} ranks")
        self.mark("test")
        return True

    def step_partitions(self):
        """--partitions: the given partition-np<P>.csv files, copied once into RESULTS/partitions."""
        self.set_step(f"partitions from {self.a.partitions}")
        self.partdir.mkdir(exist_ok=True)
        sums = {}
        for p in self.a.ranks:
            src = Path(self.a.partitions) / f"partition-np{p}.csv"
            if not src.exists():
                return self.fail(f"partitions: no {src}")
            shutil.copyfile(src, self.partdir / src.name)
            h = hashlib.sha256()
            with open(src, "rb") as fh:
                for chunk in iter(lambda: fh.read(1 << 22), b""):
                    h.update(chunk)
            sums[src.name] = h.hexdigest()
            log(f"partition {src.name}: sha256 {sums[src.name][:16]}… (replayed by every {p}-rank run)")
        self.state["partitions_sha256"] = sums
        self.mark("partitions")
        return True

    def step_exports(self):
        if self.a.partitions:   # every partition given: no export run
            return True
        for p in self.a.ranks[1:]:
            step = f"export-np{p}"
            if self.done(step):
                continue
            self.set_step(f"partition export, {p} ranks")
            st, hours, rc = self.campaign(f"export-np{p}", p, 1, [], partitions=False, export=True)
            f = self.campaigns / f"export-np{p}" / f"partition-np{p}.csv"
            self.write_done(f"export-np{p}", {"step": step, "exit": rc, "hours": hours})
            if rc == 0 and f.exists():
                shutil.copyfile(f, self.partdir / f.name)
                log(f"export np{p}: ok ({hours * 60:.1f} min)")
            else:
                self.state.setdefault("unavailable", []).append(p)
                log(f"export np{p}: FAILED (exit {rc}): the {p}-rank points are not run")
            self.mark(step)
        return True

    # ---- points -----------------------------------------------------------------
    def records(self):
        return self.state.setdefault("points", [])

    def ratio(self, code):
        """Measured / estimated hours of the code's latest full run (else the other code's, else 1)."""
        for want in (code, None):
            for r in reversed(self.records()):
                if r["outcome"] == "ok" and r.get("estimate_h") and (want is None or r["code"] == want):
                    return r["hours"] / r["estimate_h"]
        return 1.0

    def scaled(self, code, p):
        e = self.a.est.get(f"{code}@{p}")
        return None if e is None else e * self.ratio(code)

    def slot_final(self, slot):
        return any(r["slot"] == slot and r["outcome"] in FINAL for r in self.records())

    def record(self, **kw):
        kw["seq"] = len(self.records()) + 1
        kw["finished"] = utc(self.clock())
        self.records().append(kw)
        self.save()
        log("point {seq} {slot} ({code} P={np}, attempt {attempt}): {outcome}".format(**kw)
            + (f", {kw['hours']:.3f} h (estimate {kw.get('estimate_h')})" if kw.get("hours") is not None else "")
            + (f", peak {kw['peak_gib']:.1f} GiB" if kw.get("peak_gib") else "")
            + (f" — {kw['note']}" if kw.get("note") else ""))

    def run_point(self, code, p, slot, note=""):
        """Runs code at p ranks (with the UD-216 rule); returns the final outcome."""
        if p in self.state.get("unavailable", []):
            self.record(slot=slot, code=code, np=p, attempt=0, outcome="unavailable",
                        note=f"no {p}-rank partition")
            return "unavailable"
        attempt = 0
        while True:
            attempt += 1
            key = f"{code}@{p}"
            est = self.a.est.get(key)
            sc = self.scaled(code, p)
            if sc is not None and sc > self.remaining_h():
                self.record(slot=slot, code=code, np=p, attempt=attempt, outcome="skipped",
                            estimate_h=est, scaled_estimate_h=sc,
                            note=f"remaining budget {self.remaining_h():.2f} h < scaled estimate {sc:.2f} h")
                return "skipped"
            memory = self.a.memory
            if key in self.a.oom_points and attempt == 1:
                memory = self.a.oom_memory
            name = f"{code}-np{p}-a{attempt}"
            self.set_step(f"run {slot}: {code} at {p} ranks, {self.a.steps} steps, attempt {attempt}"
                          + (f" (memory {memory}: --force-oom test hook)" if memory != self.a.memory else ""))
            inject = self.a.retry_points.get(key, 0) >= attempt
            if inject:
                self.start_injector(name, p)
            st, hours, rc = self.campaign(name, p, self.a.steps, [code], memory=memory)
            if inject:
                self.stop_injector(name)
            s = st.get(code, {})
            outcome = s.get("outcome") or "failed"
            peak = s.get("memory_peak_bytes")
            rec = dict(slot=slot, code=code, np=p, attempt=attempt, outcome=outcome, hours=hours,
                       estimate_h=est, scaled_estimate_h=sc, campaign=name, exit=rc,
                       peak_gib=None if not isinstance(peak, int) else peak / 1024 ** 3,
                       memory=memory, note=note)
            if code == "rust" and outcome == "ok":
                bad = self.settings_problems(name, p)
                if bad:
                    rec["outcome"] = "refused"
                    rec["note"] = (note + "; " if note else "") + "settings not recorded: " + "; ".join(bad)
                    self.write_done(name, rec)
                    self.record(**rec)
                    raise SettingsRefused("; ".join(bad))
            self.write_done(name, rec)
            if outcome == "ud216" and code == "cpp":
                if attempt == 1:
                    rec["note"] = (note + "; " if note else "") + "UD-216: upstream's MUMPS retry; repeated once"
                    rec["outcome"] = "ud216-retry"
                    self.record(**rec)
                    continue
                rec["outcome"] = "dropped"
                rec["note"] = (note + "; " if note else "") + "UD-216 in two attempts: point dropped"
                self.record(**rec)
                return "dropped"
            self.record(**rec)
            return outcome

    def start_injector(self, name, p):
        f = f"/perf/runs/{name}/cpp/np{p}/rep1/err.txt"
        script = (f"f={f}; for i in $(seq 1800); do [ -e $f ] && break; sleep 1; done; sleep 2; "
                  f"for i in $(seq 30); do [ -e $(dirname $f)/aborted.txt ] && break; "
                  f"echo '{MARKER}' >> $f; sleep 1; done")
        self.runner(["docker", "run", "-d", "--rm", "--name", f"{self.prefix}-inject-{name}", "-u", "0",
                     "-v", f"{self.pvol}:/perf", "akantu-oracle:dev", "sh", "-c", script],
                    self.campaigns / f"{name}.log")
        log(f"--force-retry: injecting the UD-216 marker into {f}")

    def stop_injector(self, name):
        self.runner(["docker", "rm", "-f", f"{self.prefix}-inject-{name}"], self.campaigns / f"{name}.log")

    def gate(self):
        self.set_step("gate")
        host = host_mem_bytes()
        lines = ["gate: measured against the table",
                 f"  {'run':<10} {'outcome':<12} {'hours':>7} {'estimate':>9} {'ratio':>6} {'peak GiB':>9} "
                 f"{'of MEMORY':>9} {'of RAM':>7}"]
        for r in self.records():
            ratio = r["hours"] / r["estimate_h"] if r.get("hours") and r.get("estimate_h") else None
            pk = r.get("peak_gib")
            lines.append(f"  {r['code'] + '@' + str(r['np']):<10} {r['outcome']:<12} "
                         f"{fmt(r.get('hours'), 3):>7} {fmt(r.get('estimate_h'), 2):>9} {fmt(ratio, 2):>6} "
                         f"{fmt(pk, 1):>9} "
                         f"{fmt(None if pk is None else 100 * pk * 1024 ** 3 / mem_bytes(r.get('memory') or self.a.memory), 0):>8}% "
                         f"{fmt(None if pk is None or not host else 100 * pk * 1024 ** 3 / host, 0):>6}%")
        last_cpp = ("cpp", self.a.ranks[2]) if len(self.a.ranks) == 3 else None
        remaining = [(c, p) for c, p in self.plan_after_gate() if (c, p) != last_cpp]
        proj = self.elapsed_h() + self.a.reserve_h
        parts = []
        for c, p in remaining:
            if self.slot_final(f"{c}@{p}") or p in self.state.get("unavailable", []):
                continue
            sc = self.scaled(c, p) or 0.0
            proj += sc
            parts.append(f"{c}@{p} {sc:.2f} h")
        lines.append(f"  projected total {proj:.2f} h = elapsed {self.elapsed_h():.2f} + reserve "
                     f"{self.a.reserve_h} + {' + '.join(parts) or 'nothing'}"
                     + (f" (C++ P = {self.a.ranks[2]} excluded: skipped later if it does not fit)" if last_cpp else "")
                     + f"; budget {self.a.budget_h} h")
        for ln in lines:
            log(ln)
        self.state["gate"] = {"projected_h": proj, "budget_h": self.a.budget_h, "table": lines}
        if proj > self.a.budget_h:
            self.state["status"] = "stopped by the gate"
            self.save()
            log("gate: projected total exceeds the budget: STOP")
            return False
        log("gate: within the budget: continue")
        self.mark("gate")
        return True

    def plan_after_gate(self):
        return [(c, p) for p in self.a.ranks[1:] for c in self.a.codes]

    def points(self):
        p1, p2 = self.a.ranks[0], self.a.ranks[1]
        fb = self.state.setdefault("fallback", {})
        for code in self.a.codes:
            slot = f"{code}@{p1}"
            if self.slot_final(slot):
                continue
            if code in fb:   # OOM-killed in the test run: that was the attempt at P1
                self.record(slot=slot, code=code, np=p1, attempt=0, outcome="oom",
                            note=f"{fb[code]}: no attempt at {p1} ranks")
            elif self.run_point(code, p1, slot) == "oom":
                fb[code] = f"out-of-memory kill at {p1} ranks"
            else:
                continue
            log(f"{code}: falls back to {p2} ranks now (no second attempt at {p1})")
            self.run_point(code, p2, f"{code}@{p2}", note=f"fall-back from {p1} ranks, run at once")
        if not self.done("gate") and not self.gate():
            return False
        for code, p in self.plan_after_gate():
            if not self.slot_final(f"{code}@{p}"):
                self.run_point(code, p, f"{code}@{p}")
        return True

    def fail(self, why):
        self.state["status"] = f"failed: {why}"
        self.save()
        log(f"SESSION FAILED: {why}")
        return False

    # ---- main -------------------------------------------------------------------
    def run(self):
        self.res.mkdir(parents=True, exist_ok=True)
        self.campaigns.mkdir(exist_ok=True)
        if self.state_path.exists():
            if not self.a.resume:
                log(f"refused: {self.state_path} exists (another session's results; --resume to continue it)")
                return 2
            self.state = json.loads(self.state_path.read_text())
            log(f"resuming the session started {self.state.get('started')}")
        else:
            now = self.clock()
            self.state = {"started_epoch": now, "started": utc(now), "budget_h": self.a.budget_h,
                          "memory": self.a.memory, "cpus": self.a.cpus, "ranks": self.a.ranks,
                          "steps": self.a.steps, "test_steps": self.a.test_steps, "dir": self.a.dir,
                          "mesh_tarball": self.a.mesh_tarball, "estimates": self.a.est,
                          "prefix": self.prefix, "volumes": [self.ovol, self.pvol, self.rvol],
                          "codes": self.a.codes, "rust_settings": self.a.rust_settings,
                          "partitions": self.a.partitions,
                          "hooks": {"force_oom": sorted(self.a.oom_points),
                                    "force_retry": self.a.retry_points}}
        self.state.update({"status": "running", "pid": os.getpid(), "pgid": os.getpgid(0)})
        (self.res / "session.pid").write_text(f"{os.getpid()}\n")
        self.save()
        log(f"c1 session: codes {' '.join(self.a.codes)}, Rust settings {' '.join(self.a.rust_settings) or 'none'}"
            + (f", partitions {self.a.partitions}" if self.a.partitions else ""))
        log(f"c1 session: memory {self.a.memory}, cpus {self.a.cpus}, ranks {self.a.ranks}, steps "
            f"{self.a.steps}, budget {self.a.budget_h} h (reserve {self.a.reserve_h} h), results {self.res}")
        try:
            ok = ((self.done("setup") or self.step_setup())
                  and (self.done("mesh") or self.step_mesh())
                  and (not self.a.partitions or self.done("partitions") or self.step_partitions())
                  and (self.done("test") or self.step_test())
                  and self.step_exports()
                  and self.points())
        except SettingsRefused as e:
            ok = self.fail(f"a Rust run did not record --rust-settings (refused): {e}")
        if ok:
            self.state["status"] = "finished"
            self.state["step"] = "finished"
            self.state["step_since"] = utc(self.clock())
            self.save()
            log("session finished")
        print_status(self.state, self.res)
        return 0 if ok or self.state.get("status") == "stopped by the gate" else 1


def read_status(path):
    if not path.exists():
        return {}
    out = {}
    for line in path.read_text().splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            try:
                out[k] = json.loads(v)
            except ValueError:
                out[k] = v
    return out


def host_mem_bytes():
    try:
        for line in open("/proc/meminfo"):
            if line.startswith("MemTotal:"):
                return int(line.split()[1]) * 1024
    except OSError:
        pass
    return None


def fmt(x, nd):
    return "—" if x is None else f"{x:.{nd}f}"


def print_status(st, res):
    alive = False
    try:
        os.kill(int(st.get("pid", 0)), 0)
        alive = st.get("status") == "running"
    except (OSError, ValueError):
        pass
    print(f"session: {st.get('status')}" + (" (process alive)" if alive else
                                            (" (process not running)" if st.get("status") == "running" else "")))
    if "started_epoch" in st:
        el = (time.time() - st["started_epoch"]) / 3600
        print(f"started {st.get('started')}, elapsed {el:.2f} h of the {st.get('budget_h')} h budget; "
              f"memory {st.get('memory')}, cpus {st.get('cpus')}, ranks {st.get('ranks')}, steps {st.get('steps')}, "
              f"mesh cube-{st.get('mesh_name', '?')}.msh")
        print(f"codes {' '.join(st.get('codes') or ['rust', 'cpp'])}; Rust settings "
              f"{' '.join(st.get('rust_settings') or []) or 'none (the driver defaults)'}"
              + (f"; partitions {st.get('partitions')}" if st.get("partitions") else ""))
    print(f"current step: {st.get('step')} (since {st.get('step_since')})")
    print(f"steps done: {', '.join(st.get('steps_done', [])) or '—'}")
    if st.get("fallback"):
        print("fall-backs: " + "; ".join(f"{k}: {v}" for k, v in st["fallback"].items()))
    pts = st.get("points", [])
    if pts:
        print(f"  {'#':>2} {'slot':<9} {'run':<9} {'att':>3} {'outcome':<13} {'hours':>7} {'est':>6} "
              f"{'peak GiB':>8}  campaign / note")
        for r in pts:
            print(f"  {r['seq']:>2} {r['slot']:<9} {r['code'] + '@' + str(r['np']):<9} {r['attempt']:>3} "
                  f"{r['outcome']:<13} {fmt(r.get('hours'), 3):>7} {fmt(r.get('estimate_h'), 2):>6} "
                  f"{fmt(r.get('peak_gib'), 1):>8}  {r.get('campaign', '')} {r.get('note', '')}")
    if st.get("gate"):
        print(f"gate: projected {st['gate']['projected_h']:.2f} h against {st['gate']['budget_h']} h")
    logf = Path(res) / "session.log"
    if logf.exists():
        print("last log lines:")
        for line in logf.read_text(errors="replace").splitlines()[-5:]:
            print("  " + line)


def main(argv):
    if len(argv) >= 2 and argv[1] == "status":
        if len(argv) != 3:
            print("usage: c1_session.py status RESULTS", file=sys.stderr)
            return 2
        sp = Path(argv[2]) / "state.json"
        if not sp.exists():
            print(f"no session state in {argv[2]}")
            return 1
        print_status(json.loads(sp.read_text()), argv[2])
        return 0
    if len(argv) < 2 or argv[1] not in ("run", "check"):
        print(__doc__)
        return 2
    a = parse_args(argv[2:])
    missing = [f for f in ("SHA256SUMS", "images.txt", "akantu-rs-c1-repo.tar.gz") if not (Path(a.dir) / f).exists()]
    if a.partitions:
        missing += [str(Path(a.partitions) / f"partition-np{r}.csv") for r in a.ranks
                    if not (Path(a.partitions) / f"partition-np{r}.csv").exists()]
    if missing or not Path(a.mesh_tarball).exists():
        print(f"c1_session.py: missing in {a.dir}: {missing + ([] if Path(a.mesh_tarball).exists() else [a.mesh_tarball])}",
              file=sys.stderr)
        return 2
    for tool in ("docker", "tar", "gzip", "sha256sum", "flock"):
        if shutil.which(tool) is None:
            print(f"c1_session.py: {tool} not found", file=sys.stderr)
            return 2
    host = host_mem_bytes()
    if host and mem_bytes(a.memory) > host:
        print(f"c1_session.py: MEMORY {a.memory} exceeds the host's RAM ({host / 1024 ** 3:.0f} GiB)", file=sys.stderr)
        return 2
    if argv[1] == "check":
        if (Path(a.results) / "state.json").exists() and not a.resume:
            print(f"c1_session.py: {a.results}/state.json exists (--resume to continue that session)", file=sys.stderr)
            return 2
        return 0
    s = Session(a)

    def term(signum, frame):
        s.state["status"] = f"stopped (signal {signum})"
        s.save()
        log(f"stopped by signal {signum}")
        sys.exit(1)
    signal.signal(signal.SIGTERM, term)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    return s.run()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
