#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the multi-node session of the cohesive
# benchmark: a plan of runs of the Rust driver across N hosts, one run at a
# time, through mn_cluster.py. Started through mn_session.sh (detached);
# standard library only.
"""The multi-node session: the runs of a plan, one at a time, in plan order.

    python3 -I mn_session.py run    SESSION RESULTS BUDGET_H --plan PLAN [options]
    python3 -I mn_session.py check  SESSION RESULTS BUDGET_H --plan PLAN [options]
    python3 -I mn_session.py status RESULTS

SESSION is the cluster directory of mn_cluster.py (cluster.conf, hosts,
ssh/ from `mn_cluster.py keygen`); RESULTS the results directory (created;
one sub-directory per run with a DONE marker); BUDGET_H the wall-clock
budget in hours from the first start (the hosts' shutdown timers must not
be shorter).

Steps:
 1. setup   (--image-tarball: load the image on every host and check its id
            against --images-txt), start the node containers (`up`), then
            the preflight (`check`: SSH from the head to every node
            container, one mpirun over all hosts and slots: placement,
            binding, the D-050 pins on every rank). A failed preflight stops
            the session.
 2. meshes  the frozen h = 0.12 mesh of the repository subset (mesh name
            `h0.12`) and every --meshes tarball (pairs cube-<name>.msh +
            cube-<name>.json): SHA-256 checked against the JSON, copied into
            the head's work volume (/mn/work/meshes/; only rank 0 reads it).
 3. partitions  every file of --partitions DIR into /mn/work/partitions/.
 4. the runs of the plan, in order.

Plan: one run per line, `#` comments:

    NAME MESH NP HOSTS STEPS SOLVER PARTITION ESTIMATE_H [key=value ...] [@required]

  NAME       [a-z0-9][a-z0-9.-]{0,39}, unique
  MESH       a mesh name of step 2 (h0.12, h0.06, joss, ...)
  NP HOSTS   ranks and hosts: NP / HOSTS ranks on each of the first HOSTS
             live hosts of the hosts file (rank 0 on the head)
  SOLVER     auto (the input's `_auto` = MUMPS) | eigen (centralised on rank 0)
  PARTITION  none (NP = 1) | scotch (the driver's Scotch, AKA_SEED = 1) |
             file:<name> (a file of --partitions) | run:<NAME> (the
             partition.csv exported by an earlier `ok` run of this plan; every
             run at NP > 1 exports its partition)
  ESTIMATE_H hours of the run (the budget rule scales it)
  @required  a run whose outcome is neither ok nor skipped stops the session

Rules (each one unit-tested, tests/test_mn_session.py):
 * one run at a time (mn_cluster.py: session lock; refused while a
   `cohesive_extrinsic` process exists in a node container);
 * budget: a run is skipped when the remaining budget (BUDGET_H − elapsed −
   --reserve-h) is smaller than its scaled estimate (ESTIMATE_H × the
   measured / estimated ratio of the latest `ok` run on the same mesh; else
   that of the latest `ok` run on any mesh but at least 1; else 1); its
   wall-clock limit is min(remaining, max(--timeout-factor × B, B +
   --timeout-floor-h)) with B = max(ESTIMATE_H, scaled estimate): beyond it
   the job is killed (outcome `timeout`), recorded, not repeated;
 * out-of-memory: a run with an out-of-memory kill on any host (the
   container's cgroup oom_kill counter) is recorded `oom` and not repeated;
   every later run on the same mesh with at most as many hosts and at least
   as many ranks is skipped (it needs at least as much memory per host);
 * a lost host (its node container stopped, paused or gone, or its probe
   unanswered 3 times in a row, mn_cluster.py): the job is killed (no
   hang), the run recorded `host-lost`; the
   session tries once to restore each lost host (unpause / start, SSH from
   the head, leftovers killed); a host that does not come back is dead for
   the rest of the session; the run is repeated once if enough live hosts
   remain (a second host-lost drops it); later runs needing more hosts than
   remain are recorded `unavailable` (runs that fit use the first live
   hosts);
 * any other failure is recorded `failed`, not repeated;
 * every run's directory RESULTS/runs/<NAME>-a<attempt>/ holds the driver's
   outputs of all hosts (summary.json, summary_rank<r>.json, energies.csv,
   insertions.csv, chrono.csv, partition.csv, out.txt, err.txt,
   binding_rank<r>.txt: the rank's host, local rank and CPUs, summarised
   in status.txt `bindings`), the memory samples of every host (mem-h<k>.jsonl),
   status.txt (mn_cluster.py) and a DONE marker (JSON: outcome, hours,
   estimate); RESULTS/state.json holds the session state, session.log the
   log; `status` prints where the session is.

Options:
  --plan FILE               (required)
  --image-tarball F         akantu-rs-bench-mn.tar.gz (load on every host)
  --images-txt F            images.txt of the export (expected image id)
  --meshes F                a mesh tarball (repeatable)
  --partitions DIR          partition files (partition-*.csv)
  --reserve-h H             kept free at the end of the budget (0.1)
  --timeout-factor X        (2.0)   --timeout-floor-h H   (0.1)
  --watch-s S               host liveness interval during a run (15)
  --resume                  continue a session whose RESULTS/state.json exists
  --deadline EPOCH|auto     the hosts' shutdown timer (epoch seconds, or `auto`: this
                            host's /run/systemd/shutdown/scheduled): the session's
                            budget is at most (deadline − 20 min − now), BUDGET_H the cap
                            (mn_aws_hosts.py check prints it)
Test hooks (end-to-end test):
  --fail-host NAME:K:MODE:S stop | pause host K (1 = head) S seconds into
                            the first attempt of run NAME
  --force-oom NAME:K        the first attempt of run NAME with host K's
                            container memory set to --oom-memory (a real
                            out-of-memory kill), restored afterwards
  --oom-memory M            (150m)
"""

import argparse
import fcntl
import json
import os
import re
import shutil
import signal
import sys
import tarfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import mn_cluster  # noqa: E402

REPO = HERE.parent.parent.parent
FROZEN_MESH = REPO / "fixtures" / "perf_cohesive" / "meshes" / "cube-h0.12.msh"
FROZEN_JSON = REPO / "benchmarks" / "cohesive-perf" / "meshes" / "cube-h0.12.json"
FINAL = ("ok", "oom", "failed", "timeout", "dropped", "skipped", "unavailable")


def utc(t=None):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t))


def log(msg):
    print(f"{utc()} {msg}", flush=True)


def fmt(x, nd):
    return "—" if x is None else f"{x:.{nd}f}"


def parse_plan(text):
    plan, names = [], set()
    for n, line in enumerate(text.splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        f = line.split()
        if len(f) < 8:
            raise ValueError(f"plan:{n}: NAME MESH NP HOSTS STEPS SOLVER PARTITION ESTIMATE_H [key=value ...]")
        name, mesh, np_, hosts, steps, solver, part, est = f[:8]
        rest = f[8:]
        if not re.fullmatch(r"[a-z0-9][a-z0-9.-]{0,39}", name) or name in names:
            raise ValueError(f"plan:{n}: bad or repeated NAME {name!r}")
        if not re.fullmatch(r"[a-z][a-z0-9.]{0,15}", mesh):
            raise ValueError(f"plan:{n}: MESH {mesh!r}")
        try:
            np_, hosts, steps, est = int(np_), int(hosts), int(steps), float(est)
        except ValueError:
            raise ValueError(f"plan:{n}: NP, HOSTS, STEPS integers, ESTIMATE_H a number")
        if np_ < 1 or hosts < 1 or steps < 1 or est <= 0 or np_ % hosts:
            raise ValueError(f"plan:{n}: NP a positive multiple of HOSTS, STEPS > 0, ESTIMATE_H > 0")
        if solver not in ("auto", "eigen"):
            raise ValueError(f"plan:{n}: SOLVER auto | eigen")
        if not (part in ("none", "scotch") or re.fullmatch(r"(file|run):[A-Za-z0-9][A-Za-z0-9._-]*", part)):
            raise ValueError(f"plan:{n}: PARTITION none | scotch | file:NAME | run:NAME")
        if (np_ == 1) != (part == "none"):
            raise ValueError(f"plan:{n}: PARTITION none at NP = 1 only (and required there)")
        if part.startswith("run:") and part[4:] not in names:
            raise ValueError(f"plan:{n}: {part}: no earlier run of that name")
        extra, required = [], False
        for t in rest:
            if t == "@required":
                required = True
            elif re.fullmatch(r"[a-z_][a-z0-9_]*=[A-Za-z0-9_.:/+-]+", t):
                extra.append(t)
            else:
                raise ValueError(f"plan:{n}: unexpected {t!r}")
        names.add(name)
        plan.append({"name": name, "mesh": mesh, "np": np_, "hosts": hosts, "steps": steps, "solver": solver,
                     "partition": part, "estimate_h": est, "extra": extra, "required": required})
    if not plan:
        raise ValueError("plan: no run")
    return plan


def parse_args(argv):
    p = argparse.ArgumentParser(prog="mn_session.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("session")
    p.add_argument("results")
    p.add_argument("budget_h", type=float)
    p.add_argument("--plan", required=True)
    p.add_argument("--image-tarball")
    p.add_argument("--images-txt")
    p.add_argument("--meshes", action="append", default=[])
    p.add_argument("--partitions")
    p.add_argument("--reserve-h", type=float, default=0.1)
    p.add_argument("--timeout-factor", type=float, default=2.0)
    p.add_argument("--timeout-floor-h", type=float, default=0.1)
    p.add_argument("--watch-s", type=float, default=15)
    p.add_argument("--resume", action="store_true")
    p.add_argument("--fail-host", action="append", default=[])
    p.add_argument("--force-oom", action="append", default=[])
    p.add_argument("--oom-memory", default="150m")
    p.add_argument("--deadline", dest="deadline_raw")
    a = p.parse_args(argv)
    try:
        a.deadline = deadline_epoch(a.deadline_raw)
    except (OSError, ValueError) as e:
        p.error(f"--deadline: {e}")
    if a.budget_h <= 0 or a.reserve_h < 0 or a.timeout_factor < 1 or a.timeout_floor_h < 0 or a.watch_s < 1:
        p.error("BUDGET_H > 0, --reserve-h >= 0, --timeout-factor >= 1, --timeout-floor-h >= 0, --watch-s >= 1")
    try:
        a.plan_entries = parse_plan(Path(a.plan).read_text())
    except (OSError, ValueError) as e:
        p.error(str(e))
    names = {e["name"] for e in a.plan_entries}
    a.fail = {}
    for f in a.fail_host:
        mo = re.fullmatch(r"([a-z0-9][a-z0-9.-]*):([1-9][0-9]*):(stop|pause):([0-9]+(\.[0-9]+)?)", f)
        if not mo or mo.group(1) not in names:
            p.error(f"--fail-host NAME:K:stop|pause:SECONDS with NAME in the plan, got {f!r}")
        a.fail[mo.group(1)] = (int(mo.group(2)) - 1, mo.group(3), float(mo.group(4)))
    a.oom = {}
    for f in a.force_oom:
        mo = re.fullmatch(r"([a-z0-9][a-z0-9.-]*):([1-9][0-9]*)", f)
        if not mo or mo.group(1) not in names:
            p.error(f"--force-oom NAME:K with NAME in the plan, got {f!r}")
        a.oom[mo.group(1)] = int(mo.group(2)) - 1
    if not re.fullmatch(r"[1-9][0-9]*[kmg]?", a.oom_memory):
        p.error("--oom-memory like 150m")
    a.session = str(Path(a.session).resolve())
    a.results = str(Path(a.results).resolve())
    return a


SCHEDULED = "/run/systemd/shutdown/scheduled"
DEADLINE_MARGIN_S = 20 * 60   # the session ends this long before the hosts' shutdown timer


def parse_scheduled(text):
    """Epoch seconds of a systemd shutdown schedule (/run/systemd/shutdown/scheduled:
    `USEC=<microseconds since the epoch>`), or None."""
    for line in text.splitlines():
        k, _, v = line.strip().partition("=")
        if k == "USEC" and v.isdigit():
            return int(v) // 1_000_000
    return None


def deadline_epoch(raw):
    """--deadline: epoch seconds, or `auto` = this host's own shutdown timer."""
    if raw is None:
        return None
    if raw == "auto":
        d = parse_scheduled(Path(SCHEDULED).read_text())
        if d is None:
            raise ValueError(f"no USEC= in {SCHEDULED}")
        return d
    if not raw.isdigit():
        raise ValueError("epoch seconds or auto")
    return int(raw)


def expected_image_id(images_txt, image):
    """The `<image>-rootfs` line of images.txt (c1_images.sh export-mn): the image's
    identity independent of the Docker version (mn_cluster.rootfs_digest)."""
    for line in Path(images_txt).read_text().splitlines():
        f = line.split()
        if len(f) == 2 and f[0] == f"{image}-rootfs":
            return f[1]
    return None


def mesh_pairs(tarball):
    """Member names of a mesh tarball: [(name, msh member, json member)]."""
    with tarfile.open(tarball) as t:
        members = {m.name: m for m in t.getmembers()}
    pairs = []
    for n in sorted(members):
        mo = re.fullmatch(r"cube-([a-z][a-z0-9.]{0,15})\.msh", n)
        if mo and f"cube-{mo.group(1)}.json" in members:
            pairs.append((mo.group(1), n, f"cube-{mo.group(1)}.json"))
    if not pairs or len(members) != 2 * len(pairs):
        raise ValueError(f"{tarball}: expected pairs cube-<name>.msh + cube-<name>.json only, got {sorted(members)}")
    return pairs


class Session:
    def __init__(self, a, cluster=None, clock=None, sleep=None):
        self.a = a
        self.res = Path(a.results)
        self.state_path = self.res / "state.json"
        self.runs = self.res / "runs"
        self.cluster = cluster or mn_cluster.Cluster(a.session)
        self.clock = clock or time.time
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
        """BUDGET_H − elapsed − reserve; with --deadline (the hosts' shutdown timer)
        also at most (deadline − 20 min − now) − reserve."""
        r = self.a.budget_h - self.elapsed_h() - self.a.reserve_h
        if getattr(self.a, "deadline", None):
            r = min(r, (self.a.deadline - DEADLINE_MARGIN_S - self.clock()) / 3600 - self.a.reserve_h)
        return r

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

    def fail(self, why):
        self.state["status"] = f"failed: {why}"
        self.save()
        log(f"SESSION FAILED: {why}")
        return False

    # ---- steps --------------------------------------------------------------------
    def step_setup(self):
        self.set_step("setup: image, node containers, preflight")
        c = self.cluster
        try:
            if self.a.image_tarball:
                want = expected_image_id(self.a.images_txt, c.conf["image"]) if self.a.images_txt else None
                if not want:
                    return self.fail(f"setup: no {c.conf['image']}-rootfs line in {self.a.images_txt}")
                ids = c.load(self.a.image_tarball, want)
                log(f"image {c.conf['image']} loaded: {sorted(set(map(str, ids.values())))}")
            c.up()
            r = c.check()
        except mn_cluster.ClusterError as e:
            return self.fail(f"setup: {e}")
        (self.res / "check.json").write_text(json.dumps(r, indent=1) + "\n")
        self.state["hosts"] = [h for h, _ in c.hosts]
        self.state["slots"] = [s for _, s in c.hosts]
        if r["problems"]:
            return self.fail("preflight: " + "; ".join(r["problems"]))
        log(f"preflight: {len(c.hosts)} hosts, {sum(s for _, s in c.hosts)} slots, placement, binding and pins "
            f"ok on every rank ({self.res / 'check.json'})")
        self.mark("setup")
        return True

    def step_meshes(self):
        self.set_step("meshes")
        mdir = self.res / "meshes"
        mdir.mkdir(exist_ok=True)
        meshes = self.state.setdefault("meshes", {})
        items = [("h0.12", FROZEN_MESH, FROZEN_JSON)]
        for tb in self.a.meshes:
            try:
                pairs = mesh_pairs(tb)
            except (OSError, ValueError, tarfile.TarError) as e:
                return self.fail(f"meshes: {e}")
            with tarfile.open(tb) as t:
                for name, msh, js in pairs:
                    for member in (msh, js):
                        m = t.getmember(member)
                        if not m.isfile():
                            return self.fail(f"meshes: {member} is not a regular file")
                        with t.extractfile(m) as src, open(mdir / member, "wb") as dst:
                            shutil.copyfileobj(src, dst, 1 << 22)
                    items.append((name, mdir / msh, mdir / js))
        for name, msh, js in items:
            stats = json.loads(Path(js).read_text())
            got = mn_cluster.sha256_file(msh)
            if got != stats.get("sha256"):
                return self.fail(f"mesh {msh}: sha256 {got} != {stats.get('sha256')} ({js})")
            try:
                self.cluster.put(str(msh), f"{mn_cluster.WORK}/meshes/cube-{name}.msh")
            except mn_cluster.ClusterError as e:
                return self.fail(f"mesh {name}: {e}")
            meshes[name] = {"sha256": got, "nodes": stats.get("nodes"),
                            "tets": (stats.get("elements_by_type") or {}).get("tetrahedron4")}
            log(f"mesh {name}: cube-{name}.msh sha256 {got[:16]}…, {stats.get('nodes')} nodes, "
                f"{meshes[name]['tets']} tetrahedra")
        missing = sorted({e["mesh"] for e in self.a.plan_entries} - set(meshes))
        if missing:
            return self.fail(f"meshes: the plan needs {missing}, not provided (--meshes)")
        self.mark("meshes")
        return True

    def step_partitions(self):
        self.set_step("partitions")
        parts = self.state.setdefault("partitions", {})
        if self.a.partitions:
            for f in sorted(Path(self.a.partitions).iterdir()):
                if f.is_file() and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", f.name):
                    try:
                        parts[f.name] = self.cluster.put(str(f), f"{mn_cluster.WORK}/partitions/{f.name}")
                    except mn_cluster.ClusterError as e:
                        return self.fail(f"partition {f.name}: {e}")
                    log(f"partition {f.name}: sha256 {parts[f.name][:16]}…")
        need = sorted({e["partition"][5:] for e in self.a.plan_entries if e["partition"].startswith("file:")}
                      - set(parts))
        if need:
            return self.fail(f"partitions: the plan needs {need}, not provided (--partitions)")
        self.mark("partitions")
        return True

    # ---- runs ---------------------------------------------------------------------
    def records(self):
        return self.state.setdefault("points", [])

    def ratio(self, mesh):
        """Measured / estimated hours of the latest ok run on the same mesh; else that of
        the latest ok run on any mesh, but never below 1 (a small mesh's start-up dominated
        run says nothing good about a large one); else 1."""
        for r in reversed(self.records()):
            if r["outcome"] == "ok" and r.get("estimate_h") and r["mesh"] == mesh:
                return r["hours"] / r["estimate_h"]
        for r in reversed(self.records()):
            if r["outcome"] == "ok" and r.get("estimate_h"):
                return max(1.0, r["hours"] / r["estimate_h"])
        return 1.0

    def scaled(self, e):
        return e["estimate_h"] * self.ratio(e["mesh"])

    def final(self, name):
        return any(r["name"] == name and r["outcome"] in FINAL for r in self.records())

    def record(self, **kw):
        kw["seq"] = len(self.records()) + 1
        kw["finished"] = utc(self.clock())
        self.records().append(kw)
        self.save()
        log("run {seq} {name} (P={np} on {nh} hosts, attempt {attempt}): {outcome}".format(
            nh=len(kw.get("hosts") or []) or kw.get("nb_hosts", "?"), **kw)
            + (f", {kw['hours']:.3f} h (estimate {kw.get('estimate_h')})" if kw.get("hours") is not None else "")
            + (f" — {kw['note']}" if kw.get("note") else ""))

    def live_hosts(self):
        dead = set(self.state.get("dead_hosts", []))
        return [k for k in range(len(self.cluster.hosts)) if self.cluster.addr(k) not in dead]

    def partition_arg(self, e):
        part = e["partition"]
        if part.startswith("file:"):
            return f"file:{mn_cluster.WORK}/partitions/{part[5:]}", None
        if part.startswith("run:"):
            src = part[4:]
            for r in reversed(self.records()):
                if r["name"] == src and r["outcome"] == "ok" and r.get("partition_sha256"):
                    return f"file:{mn_cluster.WORK}/runs/{r['campaign']}/partition.csv", None
            return None, f"no partition: run {src} has no ok result"
        return part, None

    def oom_implied(self, e):
        for r in self.records():
            if (r["outcome"] == "oom" and r["mesh"] == e["mesh"] and r["nb_hosts"] >= e["hosts"]
                    and r["np"] <= e["np"]):
                return r
        return None

    def base(self, e, attempt):
        return dict(name=e["name"], mesh=e["mesh"], np=e["np"], nb_hosts=e["hosts"], steps=e["steps"],
                    solver=e["solver"], partition=e["partition"], attempt=attempt, estimate_h=e["estimate_h"])

    def run_entry(self, e):
        """Runs one plan entry (with the host-lost repetition); returns the final outcome."""
        attempt = 0
        while True:
            attempt += 1
            b = self.base(e, attempt)
            live = self.live_hosts()
            if e["hosts"] > len(live) or (live and live[0] != 0):
                self.record(**b, outcome="unavailable", hosts=[],
                            note=f"{len(live)} live hosts, {e['hosts']} needed")
                return "unavailable"
            r = self.oom_implied(e)
            if r:
                self.record(**b, outcome="skipped", hosts=[],
                            note=f"oom-implied: {r['name']} was OOM-killed on {r['nb_hosts']} hosts at {r['np']} ranks")
                return "skipped"
            part, why = self.partition_arg(e)
            if part is None:
                self.record(**b, outcome="unavailable", hosts=[], note=why)
                return "unavailable"
            sc = self.scaled(e)
            rem = self.remaining_h()
            if sc > rem:
                self.record(**b, outcome="skipped", hosts=[], scaled_estimate_h=sc,
                            note=f"remaining budget {rem:.2f} h < scaled estimate {sc:.2f} h")
                return "skipped"
            base = max(e["estimate_h"], sc)   # a limit never below the plan's estimate
            limit_h = min(rem, max(self.a.timeout_factor * base, base + self.a.timeout_floor_h))
            idx = live[:e["hosts"]]
            campaign = f"{e['name']}-a{attempt}"
            hook_fail = self.a.fail.get(e["name"]) if attempt == 1 else None
            hook_oom = self.a.oom.get(e["name"]) if attempt == 1 else None
            self.set_step(f"run {e['name']}: P={e['np']} on {e['hosts']} hosts, {e['steps']} steps, "
                          f"{e['solver']}, partition {e['partition']}, attempt {attempt}, limit {limit_h:.2f} h")
            t0 = self.clock()
            try:
                if hook_oom is not None:
                    self.cluster.set_memory(hook_oom, self.a.oom_memory)
                    log(f"--force-oom: host {self.cluster.addr(hook_oom)} memory {self.a.oom_memory}")
                st = self.cluster.run(campaign, self.runs / campaign, e["np"], e["hosts"],
                                      f"{mn_cluster.WORK}/meshes/cube-{e['mesh']}.msh", e["steps"], part,
                                      e["solver"], e["extra"], limit_h * 3600, self.a.watch_s, host_idx=idx,
                                      fail=hook_fail)
            except mn_cluster.ClusterError as ex:
                st = {"outcome": "failed", "error": str(ex), "hosts": [self.cluster.addr(k) for k in idx]}
                log(f"run {campaign}: launcher error: {ex}")
            finally:
                if hook_oom is not None:
                    try:
                        self.cluster.set_memory(hook_oom, self.cluster.conf["memory"])
                    except mn_cluster.ClusterError as ex:
                        log(f"--force-oom: restoring the memory limit failed: {ex}")
            hours = (self.clock() - t0) / 3600
            outcome = st.get("outcome", "failed")
            rec = dict(b, outcome=outcome, hours=hours, scaled_estimate_h=sc, limit_h=limit_h, campaign=campaign,
                       hosts=st.get("hosts"), exit=st.get("exit"), lost_hosts=st.get("lost_hosts") or [],
                       oom_hosts=[h for h, v in (st.get("oom_kills") or {}).items() if v],
                       wall_s=st.get("wall_s"), partition_sha256=st.get("partition_sha256"),
                       host_peak_gib=max([v for v in (st.get("host_peak_kib") or {}).values() if v] or [0])
                       / 1024 ** 2 or None,
                       note=("test hook: " + (f"{hook_fail[1]} host {hook_fail[0] + 1} after {hook_fail[2]:.0f} s"
                                              if hook_fail else f"oom memory {self.a.oom_memory} on host "
                                              f"{hook_oom + 1}")) if (hook_fail or hook_oom is not None) else "")
            if st.get("error"):
                rec["note"] = (rec["note"] + "; " if rec["note"] else "") + st["error"][:300]
            if st.get("binding_problems"):   # mn_cluster.py: ok only without them
                rec["binding_problems"] = st["binding_problems"]
                rec["note"] = ((rec["note"] + "; " if rec["note"] else "") + "binding problems: "
                               + "; ".join(st["binding_problems"][:8]))[:600]
            d = self.runs / campaign
            d.mkdir(parents=True, exist_ok=True)
            if outcome == "host-lost":
                restored, dead = [], []
                for h in rec["lost_hosts"]:
                    k = [i for i in range(len(self.cluster.hosts)) if self.cluster.addr(i) == h][0]
                    (restored if self.cluster.restore(k) else dead).append(h)
                self.state.setdefault("dead_hosts", []).extend(dead)
                rec["restored_hosts"], rec["dead_hosts"] = restored, dead
                log(f"host-lost: restored {restored or 'none'}, dead {dead or 'none'}")
                if attempt == 1:
                    rec["note"] = (rec["note"] + "; " if rec["note"] else "") + "host lost: repeated once"
                    (d / "DONE").write_text(json.dumps(rec, indent=1) + "\n")
                    self.record(**rec)
                    continue
                rec["outcome"] = "dropped"
                rec["note"] = (rec["note"] + "; " if rec["note"] else "") + "host lost in two attempts: dropped"
            (d / "DONE").write_text(json.dumps(rec, indent=1) + "\n")
            self.record(**rec)
            return rec["outcome"]

    def projection(self):
        rest = [e for e in self.a.plan_entries if not self.final(e["name"])]
        proj = self.elapsed_h() + self.a.reserve_h + sum(self.scaled(e) for e in rest)
        self.state["projection"] = {"projected_h": proj, "budget_h": self.a.budget_h,
                                    "remaining_runs": [e["name"] for e in rest]}
        log(f"projection: elapsed {self.elapsed_h():.2f} h + reserve {self.a.reserve_h} h + "
            f"{len(rest)} remaining runs (scaled) = {proj:.2f} h against {self.a.budget_h} h"
            + ("" if proj <= self.a.budget_h else ": the last runs will be skipped"))

    def points(self):
        for e in self.a.plan_entries:
            if self.final(e["name"]):
                continue
            outcome = self.run_entry(e)
            self.projection()
            if e["required"] and outcome not in ("ok", "skipped"):
                return self.fail(f"required run {e['name']}: {outcome}")
        return True

    # ---- main ---------------------------------------------------------------------
    def run(self):
        # one session per SESSION directory for its whole life: a second session's set-up
        # (`up`: docker rm -f of every node container) would kill the first one's job
        sdir = getattr(self.cluster, "dir", None)
        if sdir is not None:
            self._lock = open(Path(sdir) / "session.lock", "a")
            try:
                fcntl.flock(self._lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError:
                log(f"refused: another session runs on {sdir} (one session per SESSION at a time)")
                return 2
        self.res.mkdir(parents=True, exist_ok=True)
        self.runs.mkdir(exist_ok=True)
        if self.state_path.exists():
            if not self.a.resume:
                log(f"refused: {self.state_path} exists (another session's results; --resume to continue it)")
                return 2
            self.state = json.loads(self.state_path.read_text())
            log(f"resuming the session started {self.state.get('started')}")
        else:
            now = self.clock()
            self.state = {"started_epoch": now, "started": utc(now), "budget_h": self.a.budget_h,
                          "deadline": self.a.deadline, "deadline_utc": utc(self.a.deadline) if self.a.deadline else None,
                          "session": self.a.session, "plan": self.a.plan_entries,
                          "backend": self.cluster.conf["backend"], "image": self.cluster.conf["image"],
                          "memory": self.cluster.conf["memory"], "bind": self.cluster.conf["bind"],
                          "hooks": {"fail_host": self.a.fail_host, "force_oom": self.a.force_oom}}
        self.state.update({"status": "running", "pid": os.getpid()})
        (self.res / "session.pid").write_text(f"{os.getpid()}\n")
        self.save()
        log(f"mn session: {len(self.cluster.hosts)} hosts ({self.cluster.conf['backend']}), "
            f"{len(self.a.plan_entries)} runs, budget {self.a.budget_h} h (reserve {self.a.reserve_h} h"
            + (f"; the hosts' timer at {utc(self.a.deadline)}: {self.remaining_h():.2f} h left for runs"
               if self.a.deadline else "") + "), "
            f"results {self.res}")
        ok = ((self.done("setup") or self.step_setup())
              and (self.done("meshes") or self.step_meshes())
              and (self.done("partitions") or self.step_partitions())
              and self.points())
        if ok:
            self.state["status"] = "finished"
            self.state["step"] = "finished"
            self.state["step_since"] = utc(self.clock())
            self.save()
            log("session finished")
        print_status(self.state, self.res)
        return 0 if ok else 1


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
              f"{len(st.get('hosts', []))} hosts ({st.get('backend')}), memory {st.get('memory')} per host, "
              f"binding {st.get('bind')}")
    if st.get("dead_hosts"):
        print(f"dead hosts: {', '.join(st['dead_hosts'])}")
    print(f"current step: {st.get('step')} (since {st.get('step_since')})")
    print(f"steps done: {', '.join(st.get('steps_done', [])) or '—'}")
    pts = st.get("points", [])
    if pts:
        print(f"  {'#':>2} {'run':<22} {'P':>3} {'H':>2} {'att':>3} {'outcome':<12} {'hours':>7} {'est':>6} "
              f"{'GiB/host':>8}  note")
        for r in pts:
            print(f"  {r['seq']:>2} {r['name']:<22} {r['np']:>3} {r['nb_hosts']:>2} {r['attempt']:>3} "
                  f"{r['outcome']:<12} {fmt(r.get('hours'), 3):>7} {fmt(r.get('estimate_h'), 2):>6} "
                  f"{fmt(r.get('host_peak_gib'), 1):>8}  {r.get('note', '')}")
    if st.get("projection"):
        print(f"projection: {st['projection']['projected_h']:.2f} h against {st['projection']['budget_h']} h")
    logf = Path(res) / "session.log"
    if logf.exists():
        print("last log lines:")
        for line in logf.read_text(errors="replace").splitlines()[-5:]:
            print("  " + line)


def main(argv):
    if len(argv) >= 2 and argv[1] == "status":
        if len(argv) != 3:
            print("usage: mn_session.py status RESULTS", file=sys.stderr)
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
    problems = []
    try:
        c = mn_cluster.Cluster(a.session)
    except (OSError, mn_cluster.ClusterError) as e:
        print(f"mn_session.py: {e}", file=sys.stderr)
        return 2
    if not (c.ssh_dir / "id_ed25519").exists():
        problems.append(f"no session key in {c.ssh_dir} (mn_cluster.py keygen)")
    for e in a.plan_entries:
        try:
            mn_cluster.placement(e["np"], e["hosts"], c.hosts)
        except mn_cluster.ClusterError as ex:
            problems.append(f"run {e['name']}: {ex}")
    for f in [a.image_tarball, a.images_txt] + a.meshes:
        if f and not Path(f).is_file():
            problems.append(f"missing {f}")
    for tb in a.meshes:
        if Path(tb).is_file():
            try:
                mesh_pairs(tb)
            except (ValueError, tarfile.TarError) as ex:
                problems.append(str(ex))
    if a.image_tarball and not a.images_txt:
        problems.append("--image-tarball needs --images-txt (the expected image id)")
    for tool in ("docker", "tar", "sha256sum", "ssh", "scp"):
        if shutil.which(tool) is None:
            problems.append(f"{tool} not found")
    if a.deadline and (a.deadline - DEADLINE_MARGIN_S - time.time()) / 3600 < 0.1 + a.reserve_h:
        problems.append(f"--deadline {utc(a.deadline)}: less than {0.1 + a.reserve_h:.2f} h left before "
                        f"the shutdown timer minus 20 min")
    if argv[1] == "check" and (Path(a.results) / "state.json").exists() and not a.resume:
        problems.append(f"{a.results}/state.json exists (--resume to continue that session)")
    if problems:
        for pr in problems:
            print(f"mn_session.py: {pr}", file=sys.stderr)
        return 2
    if argv[1] == "check":
        return 0
    s = Session(a, c)

    def term(signum, frame):
        s.state["status"] = f"stopped (signal {signum})"
        s.save()
        log(f"stopped by signal {signum}: stopping the job on every host")
        c.abort()
        sys.exit(1)
    signal.signal(signal.SIGTERM, term)
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    return s.run()


if __name__ == "__main__":
    sys.exit(main(sys.argv))
