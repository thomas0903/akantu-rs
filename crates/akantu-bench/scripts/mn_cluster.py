#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: launcher of the cohesive benchmark driver
# (`cohesive_extrinsic`, crates/akantu-bench) for one MPI job across several
# hosts, each host running the image akantu-rs-bench:mn in Docker
# (docker/bench/Dockerfile.mn). Standard library only.
"""One MPI job of the cohesive benchmark driver across several hosts.

    python3 -I mn_cluster.py keygen SESSION          # head node: the session's SSH material
    python3 -I mn_cluster.py up     SESSION [--load IMAGE.tar.gz] [--expect-id ID]
    python3 -I mn_cluster.py check  SESSION          # preflight across all hosts
    python3 -I mn_cluster.py status SESSION
    python3 -I mn_cluster.py down   SESSION [--volumes]
    python3 -I mn_cluster.py put    SESSION FILE /mn/work/<path>   # into the head node's work volume
    python3 -I mn_cluster.py run    SESSION OUTDIR --np P --hosts H --mesh /mn/work/meshes/X.msh \
                                    --steps N [--partition scotch|none|file:/mn/work/..] \
                                    [--solver auto|eigen] [--timeout-s S] [key=value ...]

SESSION is a directory on the head node (never inside the repository) with

  cluster.conf   key = value lines:
                   backend     local | ssh
                   image       akantu-rs-bench:mn
                   memory      memory limit of every node container, no swap (e.g. 56g)
                   shm         /dev/shm size of a node container (4g)
                   ssh_port    port of the containers' sshd (2222)
                   net_cidr    the hosts' network (Open MPI's btl_tcp_if_include and
                               PRRTE's oob_tcp_if_include; required)
                   prefix      container / volume name prefix (mn)
                   code        rust (default: the port's driver, akantu-rs-bench:mn) |
                               cpp (the C++ reference harness of
                               benchmarks/cohesive-perf, strict build, in
                               akantu-rs-bench:mncpp: c1_images.sh export-mn-cpp)
                   fnte        cpp: AKANTU_RS_FILL_NODES_TO_ELEMENTS of the harness
                               (joss, default: the JOSS-patch emulation of the
                               final campaign | upstream)
                   bind        rank binding: self (default) | core | none.
                               self: mpirun --bind-to none, every rank binds
                               itself to one core of its container's cpuset with
                               bind-rank.sh (the rank's index on its host picks
                               the core: the placement of --bind-to core);
                               core: mpirun --bind-to core (Open MPI 5.0.7 in a
                               container with Docker's default seccomp profile
                               cannot start more than 10 such ranks per host,
                               see bind-rank.sh); none: no binding
                   network     local: the user-defined Docker network (created by `up`)
                   cpuset_base local: first CPU of the first node container (0)
                   cpuset      local: blocks (default: `slots` CPUs of its own per
                               container) | all (every container sees every CPU:
                               a stand-in for more cores than the machine has, e.g.
                               the smoke plan's 64 ranks on an 8-CPU sandbox; with
                               bind = none; timings meaningless)
                   ssh_user    ssh: the hosts' login (ubuntu)
                   remote_dir  ssh: absolute path of SESSION on the other hosts
                               (default: the same path as on the head)
  hosts          one host per line, Open MPI hostfile syntax `ADDRESS slots=N`;
                 the first line is the head node (where this script runs and
                 rank 0 lives). local: ADDRESS is the node container's name and
                 hostname on the Docker network; ssh: the host's private address.
  ssh/           written by `keygen` (mode 0700; never committed, never copied
                 off the hosts): id_ed25519 (+ .pub; the client key of the
                 containers' user akantu, and in the ssh backend the head's
                 login key on the other hosts), ssh_host_ed25519_key (+ .pub;
                 one host key for all node containers of the session),
                 known_hosts ("* <host key>": strict checking, no
                 trust-on-first-use), hosts_known_hosts (ssh backend: the
                 other hosts' own sshd keys, one `ADDRESS KEYTYPE KEY` line per
                 host, written by the operator after `keygen` from a channel
                 that does not trust the network (EC2: the console output's
                 SSH HOST KEY block); strict checking, `up` and `--load`
                 refuse a host without a line).

Backends:
  local  the "hosts" are containers on this Docker daemon, on a user-defined
         network (net_cidr), each with its own CPU set (`slots` consecutive
         CPUs from cpuset_base), memory limit and work volume: the
         end-to-end test without AWS (docs/TESTING.md).
  ssh    every host runs Docker; the head reaches the others over SSH as
         ssh_user with the session key (its public half added to their
         authorized_keys beforehand: docs/TESTING.md);
         the node containers use --network host and their sshd listens on
         ssh_port of the private address.

In both, Open MPI's mpirun runs in the head's node container as `akantu`
and starts its daemons (prted) in the other node containers over SSH
(ssh_port, the session key); ranks are mapped by slot over the first H
hosts with P / H ranks each (rank 0 on the head) and bound to one core each
(bind = self: bind-rank.sh, the image's copy of
benchmarks/cohesive-perf/scripts/in-container/bind-rank.sh; every rank
writes its host, local rank and Cpus_allowed_list to binding_rank<r>.txt,
collected into OUTDIR and summarised in status.txt); the thread and BLAS pins (D-050:
OMP_NUM_THREADS = OPENBLAS_NUM_THREADS = SCOTCH_PTHREAD_NUMBER = 1,
OPENBLAS_CORETYPE = Haswell from the image) and AKA_SEED (1) are forwarded
with `-x` (an SSH session does not inherit a container's ENV); `check`
verifies them on every rank.

`run` writes the driver's input (benchmarks/cohesive-perf/inputs/
material-cohesive.dat with max_steps, mesh, output_energy = true,
blocked_node = global, energy_precision = 17, the partition and solver
lines: the input of the C1 session's Rust runs, c1prep_campaign.sh
--rust-run), creates the run directory /mn/work/runs/<name> on every host
(each rank reads input.dat and writes summary_rank<r>.json; rank 0 reads
the mesh and the partition and writes the other outputs), samples memory on
every host, watches the job (host liveness every --watch-s seconds: a
stopped, paused or vanished node container is lost at once, a host whose
probe gets no answer only after 3 such probes in a row; the wall-clock
limit --timeout-s), kills it if a host is lost or the limit is
reached, collects every host's files into OUTDIR and classifies the outcome
(status.txt): ok | oom | host-lost | timeout | failed.

code = cpp runs the C++ reference harness (benchmarks/cohesive-perf, strict
build, image akantu-rs-bench:mncpp from c1_images.sh export-mn-cpp) as the
single-node campaign does (c1prep_campaign.sh ... cpp, in-container/run-one.sh):
the input derived from the same upstream file by run-one.sh's rules
(blocked_node = global, energy_precision = 17, output_energy = true,
export_partition = true at P > 1), the strict driver and the oracle install's
libraries (LD_LIBRARY_PATH) with AKANTU_RS_FILL_NODES_TO_ELEMENTS (`fnte`),
AKA_SEED and SCOTCH_PTHREAD_NUMBER forwarded to every rank, the same binding,
preflight (plus the harness's variables, driver and libraries on every rank),
sampling, kill paths and outcomes. The harness partitions with Scotch itself:
`--partition file:...` names the file its exported partition.csv must equal
(partition_matches; a mismatch makes the run `failed`). run-one.sh's early
abort on a MUMPS error or warning (UD-216) is kept: aborted.txt,
ud216_markers, outcome `failed` with the reason in `failure`. One run at a time
(lock SESSION/run.lock; refused while a `cohesive_extrinsic` process exists
in a node container).

Diagnostic hooks of `run` (environment variables of this script, off when unset,
recorded in status.txt `diagnostics`): MN_DRIVER (another port driver inside the
node containers, code = rust), MN_PRELOAD (LD_PRELOAD of every rank, e.g. the PMPI
interposer of docs/benchmarks/perf-campaign/net/pmpistat.c), MN_XENV (`VAR=value
...` forwarded to every rank with -x).
"""

import argparse
import fcntl
import hashlib
import io
import json
import os
import re
import shlex
import subprocess
import sys
import tarfile
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent
TEMPLATE = REPO / "benchmarks" / "cohesive-perf" / "inputs" / "material-cohesive.dat"
DRIVER = "/usr/local/bin/cohesive_extrinsic"
SAMPLER = "/usr/local/share/akantu-rs-bench/memsample.py"
# docker/bench/Dockerfile.mn: benchmarks/cohesive-perf/scripts/in-container/bind-rank.sh
BINDER = "/usr/local/share/akantu-rs-bench/bind-rank.sh"
BINDS = ("self", "core", "none")
CODES = ("rust", "cpp")
# code = cpp: the C++ harness as the single-node campaign runs it (c1prep_campaign.sh ... cpp,
# benchmarks/cohesive-perf/scripts/in-container/run-one.sh): the strict build's driver and the
# oracle install's libraries at the paths of the harness volumes, copied into the image
# akantu-rs-bench:mncpp (docker/bench/Dockerfile.mncpp, c1_images.sh export-mn-cpp)
CPP_DRIVER = "/perf/builds/strict/cohesive_extrinsic"
CPP_LIBDIR = "/build/install-strict/lib"   # LD_LIBRARY_PATH of the install's env.sh
# variables run-one.sh forwards to every rank (mpiexec -x) besides the thread pins; the
# library path is inherited there from env.sh in the one container and must be forwarded here
CPP_VARS = ("AKANTU_RS_FILL_NODES_TO_ELEMENTS", "LD_LIBRARY_PATH")
# run-one.sh's early abort: upstream's MUMPS retry path (UD-216: the static solution is wrong)
MUMPS_MARKERS = ("MUMPS dynamic memory is insufficient", "The MUMPS workarray is too small",
                 "Error in mumps during solve process", "Warning in mumps during solve process")
WORK = "/mn/work"
# forwarded with -x; also the column order of the preflight's probe lines
PIN_VARS = ("OPENBLAS_CORETYPE", "OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "SCOTCH_PTHREAD_NUMBER", "AKA_SEED")
PIN_EXPECT = {"OMP_NUM_THREADS": "1", "OPENBLAS_NUM_THREADS": "1", "OPENBLAS_CORETYPE": "Haswell",
              "SCOTCH_PTHREAD_NUMBER": "1", "AKA_SEED": "1"}
DEFAULTS = {"backend": "local", "image": "akantu-rs-bench:mn", "memory": "12g", "shm": "4g",
            "ssh_port": "2222", "net_cidr": "", "prefix": "mn", "bind": "self", "network": "",
            "cpuset_base": "0", "cpuset": "blocks", "ssh_user": "ubuntu", "remote_dir": "",
            "code": "rust", "fnte": "joss"}
OUTCOMES = ("ok", "oom", "host-lost", "timeout", "failed")
# liveness: a node container in one of these states has lost its processes (or never had
# them): the host is lost at once; a probe that gets no answer ("unreachable": the Docker
# daemon or the host's sshd did not answer within the probe's timeout) counts as a strike,
# and only STRIKES consecutive strikes make the host lost (a single slow answer must not
# kill an hour-long job)
LOST_STATES = ("paused", "exited", "dead", "absent", "created", "removing", "restarting")
STRIKES = 3
# a lost host that does not answer is asked again for this long before it is declared dead
RESTORE_UNREACHABLE_S = 180
# run names (they become /mn/work/runs/<name> on every host): the session's NAME pattern
RUN_NAME = r"[a-z0-9][a-z0-9.-]{0,47}"
# image ENV settings of akantu-rs-mpi:dev that contradict a bound, not
# oversubscribed multi-host run; unset in the run script, the flags rule
UNSET_ENV = ("OMPI_MCA_rmaps_base_oversubscribe", "PRTE_MCA_rmaps_default_mapping_policy",
             "OMPI_MCA_hwloc_base_binding_policy", "PRTE_MCA_hwloc_default_binding_policy")


class ClusterError(Exception):
    pass


def utc(t=None):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t))


def log(msg):
    print(f"{utc()} {msg}", flush=True)


# ---- configuration (pure) ------------------------------------------------------
def parse_conf(text):
    conf = dict(DEFAULTS)
    for n, line in enumerate(text.splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        k, sep, v = line.partition("=")
        k, v = k.strip(), v.strip()
        if not sep or k not in DEFAULTS:
            raise ClusterError(f"cluster.conf:{n}: expected `key = value` with key in {sorted(DEFAULTS)}")
        conf[k] = v
    if conf["backend"] not in ("local", "ssh"):
        raise ClusterError("cluster.conf: backend must be local or ssh")
    if conf["code"] not in CODES:
        raise ClusterError("cluster.conf: code must be rust or cpp")
    if conf["fnte"] not in ("joss", "upstream"):
        raise ClusterError("cluster.conf: fnte must be joss or upstream")
    if conf["bind"] not in BINDS:
        raise ClusterError("cluster.conf: bind must be self, core or none")
    if not re.fullmatch(r"[0-9]{1,3}(\.[0-9]{1,3}){3}/[0-9]{1,2}", conf["net_cidr"]):
        raise ClusterError("cluster.conf: net_cidr (the hosts' network, e.g. 10.77.0.0/24) is required")
    for k in ("memory", "shm"):
        if not re.fullmatch(r"[1-9][0-9]*[kmg]?", conf[k]):
            raise ClusterError(f"cluster.conf: {k} must look like 56g / 400m")
    if not re.fullmatch(r"[a-z][a-z0-9]{0,11}", conf["prefix"]):
        raise ClusterError("cluster.conf: prefix: [a-z][a-z0-9]{0,11}")
    if not conf["ssh_port"].isdigit() or not 1024 <= int(conf["ssh_port"]) <= 65535:
        raise ClusterError("cluster.conf: ssh_port must be in 1024..65535")
    if conf["backend"] == "local" and not re.fullmatch(r"[a-z][a-z0-9-]{0,30}", conf["network"] or "x"):
        raise ClusterError("cluster.conf: network: [a-z][a-z0-9-]{0,30}")
    if not conf["cpuset_base"].isdigit():
        raise ClusterError("cluster.conf: cpuset_base must be a CPU number")
    if conf["cpuset"] not in ("blocks", "all"):
        raise ClusterError("cluster.conf: cpuset must be blocks or all")
    if conf["cpuset"] == "all" and conf["bind"] != "none":
        raise ClusterError("cluster.conf: cpuset = all needs bind = none (the containers share the CPUs)")
    if conf["remote_dir"] and not conf["remote_dir"].startswith("/"):
        raise ClusterError("cluster.conf: remote_dir must be absolute")
    if not re.fullmatch(r"[a-z_][a-z0-9_-]{0,31}", conf["ssh_user"]):
        raise ClusterError("cluster.conf: ssh_user")
    return conf


def parse_hosts(text):
    """[(address, slots)]; the first host is the head."""
    hosts = []
    for n, line in enumerate(text.splitlines(), 1):
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        mo = re.fullmatch(r"([A-Za-z0-9][A-Za-z0-9.-]{0,62})\s+slots=([1-9][0-9]{0,3})", line)
        if not mo:
            raise ClusterError(f"hosts:{n}: expected `ADDRESS slots=N`, got {line!r}")
        hosts.append((mo.group(1), int(mo.group(2))))
    if not hosts:
        raise ClusterError("hosts: no host")
    addrs = [h for h, _ in hosts]
    if len(set(addrs)) != len(addrs):
        raise ClusterError("hosts: an address is listed twice")
    return hosts


def placement(np_, nhosts, hosts):
    """Ranks per host for P ranks on the first H hosts (P / H each, rank 0 on the head)."""
    if nhosts < 1 or nhosts > len(hosts):
        raise ClusterError(f"{nhosts} hosts requested, {len(hosts)} available")
    if np_ < nhosts or np_ % nhosts:
        raise ClusterError(f"{np_} ranks do not divide evenly over {nhosts} hosts")
    per = np_ // nhosts
    for addr, slots in hosts[:nhosts]:
        if per > slots:
            raise ClusterError(f"{per} ranks per host exceed the {slots} slots of {addr}")
    return per


def hostfile_text(hosts, per):
    return "".join(f"{addr} slots={per}\n" for addr, _ in hosts)


def cpusets(hosts, base):
    """local backend: consecutive CPU blocks of `slots` CPUs per node container."""
    out, c = [], base
    for _, slots in hosts:
        out.append(f"{c}-{c + slots - 1}" if slots > 1 else f"{c}")
        c += slots
    return out


def input_text(template, steps, mesh, partition, solver, extra):
    """The driver input of c1prep_campaign.sh --rust-run (same substitutions)."""
    lines = ["  blocked_node = global", "  energy_precision = 17"]
    if partition.startswith("file:"):
        lines.append(f"  rust_partitioner = {partition}")
    if solver != "auto":
        lines.append(f"  rust_sparse_solver = {solver}")
    for kv in extra:
        k, _, v = kv.partition("=")
        lines.append(f"  {k} = {v}")
    out = []
    for line in template.splitlines():
        if re.match(r"^  max_steps = ", line):
            line = f"  max_steps = {steps}"
        elif re.match(r"^  mesh = ", line):
            line = f"  mesh = {mesh}"
        elif re.match(r"^  output_energy = ", line):
            line = "  output_energy = true"
        out.append(line)
        if line == "user section [":
            out.extend(lines)
    return "\n".join(out) + "\n"


def input_text_cpp(template, steps, mesh, solver, extra):
    """The C++ harness's input as run-one.sh derives it (sed on the upstream file): max_steps,
    mesh and every key=value that exists in the file replaced in place, the other keys inserted
    in order right after `user section [`; solver eigen: `sparse_solver_type = eigen` after
    `    name = static` (PERF_STATIC_SPARSE_SOLVER)."""
    lines = template.splitlines()
    present = {m.group(1) for m in (re.match(r"^  ([^ ]+) = ", ln) for ln in lines) if m}
    repl = {"max_steps": str(steps), "mesh": mesh}
    insert = []
    for kv in extra:
        k, _, v = kv.partition("=")
        if k in present:
            repl[k] = v
        else:
            insert.append(f"  {k} = {v}")
    out = []
    for ln in lines:
        m = re.match(r"^  ([^ ]+) = ", ln)
        if m and m.group(1) in repl:
            ln = f"  {m.group(1)} = {repl[m.group(1)]}"
        out.append(ln)
        if ln == "user section [":
            out.extend(insert)
        if solver != "auto" and ln == "    name = static":
            out.append(f"    sparse_solver_type = {solver}")
    return "\n".join(out) + "\n"


def mpirun_argv(np_, hostfile, wdir, cidr, bind, yield_idle=False, extra=()):
    # pml ob1 (+ the TCP / shared-memory BTLs) explicitly: the MPI image ships libucx0, and
    # Open MPI 5 only drops UCX by itself because no Mellanox device is found.
    # bind = self: no binding by mpirun, the ranks bind themselves (app_argv)
    report = ["--report-bindings"] if bind == "core" else []
    argv = ["mpirun", "-np", str(np_), "--hostfile", hostfile, "--map-by", "slot:NOOVERSUBSCRIBE",
            "--bind-to", "none" if bind == "self" else bind, *report, "--wdir", wdir,
            "--mca", "pml", "ob1", "--mca", "btl", "tcp,self,sm", "--mca", "btl_tcp_if_include", cidr,
            "--prtemca", "oob_tcp_if_include", cidr]
    if yield_idle:   # local `cpuset = all`: more ranks than cores; idle ranks must give their core away
        argv += ["--mca", "mpi_yield_when_idle", "1"]
    return argv + list(extra)


# Diagnostic hooks of `run` (environment of mn_cluster.py; off when unset; recorded in
# status.txt `diagnostics`): MN_DRIVER, the path of another port driver inside the node
# containers (code = rust; e.g. a build copied there with docker cp); MN_PRELOAD, a library
# preloaded into every rank (LD_PRELOAD, e.g. a PMPI interposer); MN_XENV, `VAR=value ...`
# set in every rank's environment (mpirun -x).
DIAG_PATH = r"/[A-Za-z0-9._/-]+"
DIAG_KV = r"[A-Z_][A-Z0-9_]*=[A-Za-z0-9._/:,=-]*"


def diag_options(environ):
    """(driver or None, extra mpirun arguments, record) from the MN_* hooks of `environ`."""
    rec, extra, driver = {}, [], None
    for k in ("MN_DRIVER", "MN_PRELOAD"):
        v = environ.get(k, "")
        if v and not re.fullmatch(DIAG_PATH, v):
            raise ClusterError(f"{k}: an absolute path of [A-Za-z0-9._/-], got {v!r}")
        if v:
            rec[k] = v
    if rec.get("MN_DRIVER"):
        driver = rec["MN_DRIVER"]
    if rec.get("MN_PRELOAD"):
        extra += ["-x", "LD_PRELOAD=" + rec["MN_PRELOAD"]]
    kvs = environ.get("MN_XENV", "").split()
    for kv in kvs:
        if not re.fullmatch(DIAG_KV, kv):
            raise ClusterError(f"MN_XENV: expected VAR=value words, got {kv!r}")
        extra += ["-x", kv]
    if kvs:
        rec["MN_XENV"] = " ".join(kvs)
    return driver, extra, rec


# sh: $cpus = the process's Cpus_allowed_list, $sib = the hardware threads of the physical
# core of its first CPU (thread_siblings_list; `-` when unreadable): two ranks with the same
# $sib run on one physical core even when their CPU numbers differ
CPUS_SIB = ('cpus=$(awk \'/^Cpus_allowed_list:/ {print $2}\' /proc/self/status); c=${cpus%%[,-]*}; '
            'sib=$(cat "/sys/devices/system/cpu/cpu$c/topology/thread_siblings_list" 2>/dev/null); '
            'sib=${sib:--}; ')
# the environment a rank of a run receives (recorded per rank: env_rank<r>.txt, VAR=value
# lines, `unset` when absent): the thread pins, the seed, and the C++ harness's variables
ENV_VARS = ("OPENBLAS_CORETYPE", "OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS", "SCOTCH_PTHREAD_NUMBER",
            "AKA_SEED", "AKANTU_RS_FILL_NODES_TO_ELEMENTS", "LD_LIBRARY_PATH")
# every rank records where it runs and what it received (in --wdir, before it execs the
# driver); with bind = self after bind-rank.sh has bound it, so the lines are the binding
# and the environment the driver runs with
RECORD = (CPUS_SIB + 'echo "$(hostname) ${OMPI_COMM_WORLD_LOCAL_RANK:-unset} $cpus $sib" '
          '> "binding_rank${OMPI_COMM_WORLD_RANK:-unset}.txt" && '
          + "{ " + " ".join(f'echo "{v}=${{{v}:-unset}}";' for v in ENV_VARS)
          + ' } > "env_rank${OMPI_COMM_WORLD_RANK:-unset}.txt" && exec "$@"')

# The description of a host (preflight check.json `hosts[].description`, every run's
# status.txt `host_descriptions`), read inside its node container from /proc and /sys
# (the host's kernel view; no host name): CPU model, sockets, cores, threads per core, NUMA
# nodes (CPU list, memory), total memory, kernel release, transparent huge pages, NUMA
# balancing, load average. Kept as source text: run with `python3 -I -c` in the container
# (the image has no copy of this module) and exec'd by the offline tests on a synthetic tree.
HOST_PROBE = r"""
import json, os, platform, re


def describe_host(root="/"):
    def rd(*parts):
        try:
            with open(os.path.join(root, *parts)) as f:
                return f.read()
        except OSError:
            return None

    def cpulist(text):
        out = []
        for part in (text or "").strip().split(","):
            if part:
                a, _, b = part.partition("-")
                out.extend(range(int(a), int(b or a) + 1))
        return out

    d = {}
    model = None
    for line in (rd("proc", "cpuinfo") or "").splitlines():
        if line.lower().startswith(("model name", "cpu model")):
            model = line.split(":", 1)[1].strip()
            break
    d["cpu_model"] = model
    online = cpulist(rd("sys", "devices", "system", "cpu", "online"))
    pkgs, cores = set(), set()
    for c in online:
        t = os.path.join("sys", "devices", "system", "cpu", f"cpu{c}", "topology")
        pk, co = rd(t, "physical_package_id"), rd(t, "core_id")
        if pk is not None and co is not None:
            pkgs.add(pk.strip())
            cores.add((pk.strip(), co.strip()))
    d["logical_cpus"] = len(online) or None
    d["sockets"] = len(pkgs) or None
    d["cores"] = len(cores) or None
    d["threads_per_core"] = (len(online) // len(cores)) if cores else None
    nodes = []
    base = os.path.join(root, "sys", "devices", "system", "node")
    for n in sorted((x for x in (os.listdir(base) if os.path.isdir(base) else [])
                     if re.fullmatch(r"node[0-9]+", x)), key=lambda x: int(x[4:])):
        m = re.search(r"MemTotal:\s+([0-9]+) kB", rd("sys", "devices", "system", "node", n, "meminfo") or "")
        nodes.append({"node": int(n[4:]), "cpus": (rd("sys", "devices", "system", "node", n, "cpulist") or "").strip(),
                      "mem_kib": int(m.group(1)) if m else None})
    d["numa_nodes"] = nodes
    m = re.search(r"MemTotal:\s+([0-9]+) kB", rd("proc", "meminfo") or "")
    d["mem_total_kib"] = int(m.group(1)) if m else None
    d["kernel"] = (rd("proc", "sys", "kernel", "osrelease") or platform.release()).strip()
    thp = rd("sys", "kernel", "mm", "transparent_hugepage", "enabled")
    m = re.search(r"\[([a-z]+)\]", thp or "")
    d["transparent_hugepage"] = m.group(1) if m else (thp.strip() if thp else None)
    nb = rd("proc", "sys", "kernel", "numa_balancing")
    d["numa_balancing"] = nb.strip() if nb is not None else None
    la = rd("proc", "loadavg")
    d["loadavg"] = " ".join(la.split()[:3]) if la else None
    return d


if __name__ == "__main__":
    print(json.dumps(describe_host()))
"""


def host_line(d):
    """One line of a host description (mn_compare.py prints it)."""
    if not d:
        return "no description"
    numa = ", ".join(f"node{n['node']} cpus {n['cpus']} {((n.get('mem_kib') or 0) / 2 ** 20):.0f} GiB"
                     for n in d.get("numa_nodes") or [])
    return (f"{d.get('cpu_model')}; {d.get('sockets')} socket(s), {d.get('cores')} cores, "
            f"{d.get('threads_per_core')} thread(s)/core; NUMA [{numa}]; "
            f"{((d.get('mem_total_kib') or 0) / 2 ** 20):.0f} GiB; kernel {d.get('kernel')}; "
            f"THP {d.get('transparent_hugepage')}; numa_balancing {d.get('numa_balancing')}; "
            f"load {d.get('loadavg')}; Docker {d.get('docker')}")


def package_changes(base_tsv, new_tsv):
    """Debian packages of two images (`dpkg-query -W -f '${Package}\\t${Version}\\t${Architecture}\\n'`):
    ([base packages whose version or architecture changed or that disappeared, as text],
    [(package, version, architecture) added by the new image]) (c1_images.sh export-mn-cpp)."""
    def parse(text):
        out = {}
        for line in text.splitlines():
            f = line.split("\t")
            if len(f) == 3 and f[0]:
                out[(f[0], f[2])] = f[1]
        return out
    a, b = parse(base_tsv), parse(new_tsv)
    changed = [f"{p}:{arch} {v} -> {b.get((p, arch), 'absent')}" for (p, arch), v in sorted(a.items())
               if b.get((p, arch)) != v]
    added = [(p, v, arch) for (p, arch), v in sorted(b.items()) if (p, arch) not in a]
    return changed, added


# variables the launcher does not set for a code: recorded, not compared between ranks.
# code = rust: LD_LIBRARY_PATH is not forwarded; the ranks PRRTE starts on the other hosts
# get the one of their remote launch (Open MPI 5.0.7 of the image:
# `/usr/lib:/usr/lib/x86_64-linux-gnu/pmix2/lib:`), the head's ranks none
ENV_NOT_COMPARED = {"rust": ("LD_LIBRARY_PATH",), "cpp": ()}


def env_from(files, ignore=()):
    """({VAR: value} of rank 0, [ranks whose environment differs from rank 0's in a variable
    not in `ignore`]) from the env_rank<r>.txt texts {name: text}."""
    envs = {}
    for name, text in files.items():
        mo = re.fullmatch(r"env_rank([0-9]+)\.txt", name)
        if mo:
            envs[int(mo.group(1))] = dict(ln.split("=", 1) for ln in text.splitlines() if "=" in ln)
    if not envs:
        return {}, []
    ref = envs[min(envs)]

    def compared(e):
        return {k: v for k, v in e.items() if k not in ignore}
    return ref, sorted(r for r, e in envs.items() if compared(e) != compared(ref))


def app_argv(bind, argv, record=False):
    """What mpirun starts for every rank: argv, behind bind-rank.sh with one core per rank
    (bind = self) and behind the binding record (record = True)."""
    out = list(argv)
    if record:
        out = ["sh", "-c", RECORD, "record", *out]
    if bind == "self":
        out = ["sh", BINDER, "1", *out]
    return out


def bindings_from(files):
    """{rank: [hostname, local_rank, cpus, siblings]} from the binding_rank<r>.txt texts {name: text}
    (siblings `-` in a record without them)."""
    out = {}
    for name, text in files.items():
        mo = re.fullmatch(r"binding_rank([0-9]+)\.txt", name)
        f = text.split()
        if mo and len(f) in (3, 4):
            out[int(mo.group(1))] = (f + ["-"])[:4]
    return dict(sorted(out.items()))


def binding_problems(bindings, np_, bind):
    """Problems of a run's recorded bindings: every rank recorded; with bind = self or core,
    one CPU per rank (self), no CPU shared by two ranks of one host and, with bind = self, no
    physical core shared (the same hardware-thread sibling set; `-` = unknown, not compared)."""
    if bind == "none":
        return []
    if sorted(bindings) != list(range(np_)):
        return [f"binding records of ranks {sorted(bindings)}, expected 0..{np_ - 1}"]
    problems, seen, cores = [], {}, {}
    for r, rec in bindings.items():
        host, _, cpus, sib = (list(rec) + ["-"])[:4]
        if bind == "self" and not cpus.isdigit():
            problems.append(f"rank {r} on {host}: CPUs {cpus}, expected one")
        if cpus in seen.setdefault(host, {}):
            problems.append(f"ranks {seen[host][cpus]} and {r} on {host} share CPUs {cpus}")
        elif bind == "self" and sib != "-" and sib in cores.setdefault(host, {}):
            problems.append(f"ranks {cores[host][sib]} and {r} on {host} share a physical core "
                            f"(hardware threads {sib})")
        seen[host][cpus] = r
        if sib != "-":
            cores.setdefault(host, {}).setdefault(sib, r)
    return problems


def run_script(rd, mpirun, app, code="rust", fnte="joss", driver=DRIVER):
    """bash script run in the head's node container (as akantu): the job. code = cpp: the
    harness's environment (FNTE, the install's library path, forwarded to every rank) and
    run-one.sh's early abort on a MUMPS error or warning (UD-216 class: aborted.txt, TERM to
    mpirun)."""
    unset = " ".join(UNSET_ENV)
    xs = " ".join(PIN_VARS + (CPP_VARS if code == "cpp" else ()))
    env = (f"export AKANTU_RS_FILL_NODES_TO_ELEMENTS={shlex.quote(fnte)} LD_LIBRARY_PATH={CPP_LIBDIR}\n"
           if code == "cpp" else "")
    run = f'{shlex.join(mpirun)} "${{xs[@]}}" {shlex.join(app)} > out.txt 2> err.txt'
    if code == "cpp":
        grep = " ".join(f"-e {shlex.quote(m)}" for m in MUMPS_MARKERS)
        run = f"""{run} &
job=$!
( while kill -0 "$job" 2>/dev/null; do
    if grep -qF {grep} out.txt err.txt 2>/dev/null; then
      echo "MUMPS error/warning (UD-216 class): run stopped early" > aborted.txt
      pkill -TERM -x mpirun; pkill -TERM -x prterun
      break
    fi
    sleep 2
  done ) &
watcher=$!
wait "$job"; rc=$?
kill "$watcher" 2>/dev/null
(exit "$rc")"""
    return f"""set -u
cd {shlex.quote(rd)} || exit 90
unset {unset}
export OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 AKA_SEED="${{AKA_SEED:-1}}"
{env}xs=()
for v in {xs}; do [ -n "${{!v+x}}" ] && xs+=(-x "$v"); done
t0=$(date +%s.%N)
{run}
echo $? > exit
t1=$(date +%s.%N)
python3 -c "print(f'{{$t1-$t0:.3f}}')" > wall.txt
sha256sum {CPP_DRIVER if code == "cpp" else driver} > binary.sha256
"""


def classify(st):
    """Outcome of a run from its facts (status dict):
    oom        an out-of-memory kill in a node container during the run (the
               kernel's cgroup oom_kill counter rose on some host): the cause
               even if the job was then killed for another reason;
    host-lost  a host's node container stopped, paused or became unreachable
               during the run (seen by the watchdog or after the job ended);
    timeout    the wall-clock limit was reached;
    ok         mpirun exited 0, summary.json exists, every rank wrote its
               summary_rank<r>.json and the binding records show no problem
               (binding_problems empty);
    failed     anything else (with binding problems, a MUMPS error or warning
               of the C++ harness (UD-216; c1prep_campaign.sh's `ud216`) or a
               C++ partition that differs from the plan's file: the reason in
               `failure`)."""
    if any((v or 0) > 0 for v in (st.get("oom_kills") or {}).values()):
        return "oom"
    if st.get("lost_hosts"):
        return "host-lost"
    if st.get("killed") == "timeout":
        return "timeout"
    cpp = st.get("case") == "cpp"
    if cpp and (st.get("aborted") or st.get("ud216_markers")):   # UD-216 (c1prep_campaign.sh `ud216`)
        return "failed"
    if cpp and st.get("partition_matches") is False:   # the harness did not re-create the partition
        return "failed"
    if (st.get("exit") == 0 and st.get("summary") and st.get("nb_rank_summaries") == st.get("np")
            and not st.get("binding_problems")):
        return "ok"
    return "failed"


def failure_reasons(st, code, partition, exported):
    """The reasons of a `failed` run that the status facts name (status dict st; exported:
    the run directory has a partition.csv)."""
    why = []
    if st.get("binding_problems"):
        why.append("binding: " + "; ".join(st["binding_problems"][:8]))
    if code == "cpp" and (st.get("aborted") or st.get("ud216_markers")):
        why.append(f"UD-216: MUMPS error or warning ({st.get('ud216_markers')} marker(s)"
                   + (", run stopped early" if st.get("aborted") else "") + ")")
    if code == "cpp" and st.get("partition_matches") is False:
        ref = partition[len("file:"):]
        if not exported:
            why.append("partition: the harness exported no partition.csv")
        elif st.get("partition_input_sha256") is None:
            why.append(f"partition: {ref} is missing on the head (nothing to compare with)")
        else:
            why.append("partition: the harness's partition.csv differs from " + ref)
    return why


def watch_step(strikes, states):
    """One liveness probe of the run's hosts: states {host: alive()}; strikes {host:
    consecutive unanswered probes} is updated. Returns the hosts that are lost now."""
    lost = []
    for h, s in states.items():
        if s == "running":
            strikes[h] = 0
        elif s in LOST_STATES:
            lost.append(h)
        else:   # unreachable (no answer) or an unknown state: a strike
            strikes[h] = strikes.get(h, 0) + 1
            if strikes[h] >= STRIKES:
                lost.append(h)
    return lost


def write_status(path, st):
    path.write_text("".join(f"{k}={json.dumps(v)}\n" for k, v in st.items()))


def read_status(path):
    out = {}
    if not Path(path).exists():
        return out
    for line in Path(path).read_text().splitlines():
        if "=" in line:
            k, v = line.split("=", 1)
            try:
                out[k] = json.loads(v)
            except ValueError:
                out[k] = v
    return out


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 22), b""):
            h.update(chunk)
    return h.hexdigest()


def rootfs_digest(layers_json):
    """`rootfs:<sha256>` of an image's layer diff ids (`docker image inspect --format
    '{{json .RootFS.Layers}}'`), one per line: the same for a `docker save` tarball loaded
    by any Docker version, unlike the image id (config digest in the classic image store,
    manifest digest in the containerd store)."""
    layers = json.loads(layers_json)
    return "rootfs:" + hashlib.sha256("\n".join(layers).encode()).hexdigest()


def host_peak_kib(jsonl_text):
    """Largest sum of the sampled processes' VmRSS (memsample.py output)."""
    peak = 0
    for line in jsonl_text.splitlines():
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if d.get("header"):
            continue
        peak = max(peak, sum(p[2] for p in d.get("procs", [])))
    return peak


# ---- the cluster ------------------------------------------------------------------
class Cluster:
    def __init__(self, session):
        self.dir = Path(session).resolve()
        self.conf = parse_conf((self.dir / "cluster.conf").read_text())
        self.hosts = parse_hosts((self.dir / "hosts").read_text())
        self.ssh_dir = self.dir / "ssh"
        self.local = self.conf["backend"] == "local"
        self.port = self.conf["ssh_port"]
        self.cidr = self.conf["net_cidr"]
        self.remote_dir = self.conf["remote_dir"] or str(self.dir)

    def shared_cpus(self):
        """local `cpuset = all`: the node containers share the machine's CPUs (oversubscribed)."""
        return self.local and self.conf["cpuset"] == "all"

    # -- naming
    def addr(self, k):
        return self.hosts[k][0]

    def container(self, k):
        return self.addr(k) if self.local else f"{self.conf['prefix']}-node"

    def volume(self, k):
        return f"{self.conf['prefix']}-work-{self.addr(k)}" if self.local else f"{self.conf['prefix']}-work"

    # -- command construction
    def ssh_opts(self):
        return ["-i", str(self.ssh_dir / "id_ed25519"), "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes",
                "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=10", "-o", "ServerAliveCountMax=3",
                "-o", f"UserKnownHostsFile={self.ssh_dir / 'hosts_known_hosts'}",
                "-o", "StrictHostKeyChecking=yes", "-o", "LogLevel=ERROR"]

    def require_host_keys(self):
        """ssh backend: every host but the head needs a line in ssh/hosts_known_hosts
        (no trust on first use); raises ClusterError listing the hosts without one."""
        if self.local:
            return
        known = known_host_names((self.ssh_dir / "hosts_known_hosts").read_text()
                                 if (self.ssh_dir / "hosts_known_hosts").exists() else "")
        missing = [self.addr(k) for k in range(1, len(self.hosts)) if self.addr(k) not in known]
        if missing:
            raise ClusterError(f"no host key in {self.ssh_dir / 'hosts_known_hosts'} for {', '.join(missing)} "
                               "(write the hosts' sshd keys there after keygen; docs/TESTING.md)")

    def host_argv(self, k, argv):
        """argv run on host k (the head, and every host of the local backend: here)."""
        if self.local or k == 0:
            return list(argv)
        return ["ssh", *self.ssh_opts(), f"{self.conf['ssh_user']}@{self.addr(k)}", shlex.join(argv)]

    def node_argv(self, k, argv, user="akantu", stdin=False, detach=False):
        flags = (["-i"] if stdin else []) + (["-d"] if detach else [])
        return self.host_argv(k, ["docker", "exec", *flags, "-u", user, self.container(k), *argv])

    def call(self, argv, timeout=120, data=None, check=False):
        try:
            p = subprocess.run(argv, input=data, capture_output=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            if check:
                raise ClusterError(f"timed out after {timeout} s: {shlex.join(argv)[:200]}")
            return 124, b"", b"timeout"
        if check and p.returncode != 0:
            raise ClusterError(f"exit {p.returncode}: {shlex.join(argv)[:300]}: "
                               f"{p.stderr.decode(errors='replace')[-500:]}")
        return p.returncode, p.stdout, p.stderr

    def each(self, fn, ks):
        """fn(k) for every k, in parallel threads (ssh backend) or in order; {k: result}."""
        res, errs = {}, {}

        def wrap(k):
            try:
                res[k] = fn(k)
            except Exception as e:  # noqa: BLE001 — reported per host below
                errs[k] = e
        if self.local:
            for k in ks:
                wrap(k)
        else:
            ts = [threading.Thread(target=wrap, args=(k,)) for k in ks]
            for t in ts:
                t.start()
            for t in ts:
                t.join()
        if errs:
            raise ClusterError("; ".join(f"{self.addr(k)}: {e}" for k, e in sorted(errs.items())))
        return res

    # -- keys
    def keygen(self):
        if (self.ssh_dir / "id_ed25519").exists():
            raise ClusterError(f"{self.ssh_dir} exists (one key per session: start a new SESSION)")
        self.ssh_dir.mkdir(mode=0o700)
        stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
        for name, comment in (("id_ed25519", "akantu-mn-session"), ("ssh_host_ed25519_key", "akantu-mn-host")):
            self.call(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", f"{comment}-{stamp}",
                       "-f", str(self.ssh_dir / name)], check=True)
        host_pub = (self.ssh_dir / "ssh_host_ed25519_key.pub").read_text().split()
        (self.ssh_dir / "known_hosts").write_text(f"* {host_pub[0]} {host_pub[1]}\n")
        (self.ssh_dir / "hosts_known_hosts").write_text("")
        for f in self.ssh_dir.iterdir():
            f.chmod(0o600)
        return (self.ssh_dir / "id_ed25519.pub").read_text().strip()

    # -- node containers
    def load(self, tarball, expect_id=None):
        tarball = Path(tarball).resolve()
        rdir = self.remote_dir
        self.require_host_keys()

        def one(k):
            if self.local and k > 0:
                return None
            if self.local or k == 0:
                self.call(["docker", "load", "-i", str(tarball)], timeout=1800, check=True)
            else:
                dst = f"{self.conf['ssh_user']}@{self.addr(k)}:{rdir}/{tarball.name}"
                self.call(self.host_argv(k, ["mkdir", "-p", rdir]), check=True)
                self.call(["scp", "-q", *self.ssh_opts(), str(tarball), dst], timeout=3600, check=True)
                self.call(self.host_argv(k, ["docker", "load", "-i", f"{rdir}/{tarball.name}"]),
                          timeout=1800, check=True)
            return self.image_id(k)
        ids = self.each(one, range(len(self.hosts)))
        if expect_id:
            bad = {self.addr(k): i for k, i in ids.items() if i is not None and i != expect_id}
            if bad:
                raise ClusterError(f"image id differs from {expect_id}: {bad}")
        return ids

    def image_id(self, k):
        """The image's identity on host k: rootfs_digest() of its layer diff ids (the image
        id itself depends on the Docker version's image store)."""
        rc, out, _ = self.call(self.host_argv(k, ["docker", "image", "inspect", "--format",
                                                  "{{json .RootFS.Layers}}", self.conf["image"]]), timeout=60)
        return rootfs_digest(out.decode()) if rc == 0 else None

    def run_node_argv(self, k, cpuset=None):
        c = self.conf
        argv = ["docker", "run", "-d", "--init", "--name", self.container(k), "--label", "akantu-mn=node",
                "--memory", c["memory"], "--memory-swap", c["memory"], "--shm-size", c["shm"],
                "-e", f"MN_SSH_PORT={self.port}"]
        if self.local:
            argv += ["--hostname", self.addr(k), "--network", c["network"], "--cpuset-cpus", cpuset,
                     "-v", f"{self.ssh_dir}:/mn-ssh:ro"]
        else:
            argv += ["--network", "host", "-e", f"MN_LISTEN={self.addr(k)}",
                     "-v", f"{self.ssh_dir if k == 0 else self.remote_dir + '/ssh'}:/mn-ssh:ro"]
        argv += ["-v", f"{self.volume(k)}:{WORK}", c["image"]]
        return argv

    def up(self):
        if not (self.ssh_dir / "id_ed25519").exists():
            raise ClusterError("no session key: run `keygen` first")
        self.require_host_keys()
        ncpu = os.cpu_count() or 1
        if self.local and self.conf["cpuset"] == "all":
            sets = [f"0-{ncpu - 1}"] * len(self.hosts)
        else:
            sets = cpusets(self.hosts, int(self.conf["cpuset_base"])) if self.local else [None] * len(self.hosts)
        if self.local:
            last = int(self.conf["cpuset_base"]) + sum(s for _, s in self.hosts)
            if self.conf["cpuset"] == "blocks" and last > ncpu:
                raise ClusterError(f"the hosts' slots need CPUs up to {last - 1}, this machine has {ncpu}")
            if self.call(["docker", "network", "inspect", self.conf["network"]])[0] != 0:
                self.call(["docker", "network", "create", "--subnet", self.cidr, self.conf["network"]], check=True)

        def one(k):
            if not self.local and k > 0:
                self.call(self.host_argv(k, ["mkdir", "-p", "-m", "0700", self.remote_dir]), check=True)
                self.call(["scp", "-q", "-r", "-p", *self.ssh_opts(), str(self.ssh_dir),
                           f"{self.conf['ssh_user']}@{self.addr(k)}:{self.remote_dir}/"], check=True)
            self.call(self.host_argv(k, ["docker", "rm", "-f", self.container(k)]))
            self.call(self.host_argv(k, self.run_node_argv(k, sets[k])), check=True)
            return True
        self.each(one, range(len(self.hosts)))
        # sshd up in every node container, reachable from the head's
        for k in range(len(self.hosts)):
            for _ in range(30):
                if self.ssh_from_head(k):
                    break
                time.sleep(1)
            else:
                raise ClusterError(f"{self.addr(k)}: the head's node container cannot log in over SSH")
        log(f"up: {len(self.hosts)} node containers ({self.conf['backend']} backend), SSH from the head ok")

    def ssh_from_head(self, k):
        if k == 0:
            return self.alive(0) == "running"
        rc, _, _ = self.call(self.node_argv(0, ["ssh", "-o", "ConnectTimeout=5", self.addr(k), "true"]),
                             timeout=30)
        return rc == 0

    def down(self, volumes=False):
        for k in range(len(self.hosts)):
            self.call(self.host_argv(k, ["docker", "rm", "-f", self.container(k)]))
            if volumes:
                self.call(self.host_argv(k, ["docker", "volume", "rm", self.volume(k)]))
        if self.local:
            self.call(["docker", "network", "rm", self.conf["network"]])

    def alive(self, k):
        """running | paused | exited | ... | absent | unreachable"""
        rc, out, err = self.call(self.host_argv(k, ["docker", "inspect", "--format", "{{.State.Status}}",
                                                    self.container(k)]), timeout=20)
        if rc == 0:
            return out.decode().strip()
        e = err.decode(errors="replace")
        if "No such object" in e or "No such container" in e:
            return "absent"
        return "unreachable"   # timeout, the host's sshd (255) or the Docker daemon did not answer

    def status(self):
        return {self.addr(k): self.alive(k) for k in range(len(self.hosts))}

    def fail_host(self, k, mode):
        """Test hook: stop or pause host k's node container (a failing host)."""
        if mode not in ("stop", "pause"):
            raise ClusterError("fail mode: stop | pause")
        return subprocess.Popen(self.host_argv(k, ["docker", mode, self.container(k)]),
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    def restore(self, k, unreachable_s=RESTORE_UNREACHABLE_S, pause=10):
        """Bring host k's node container back (unpause / start); True when SSH works again.
        A host that does not answer is asked again for unreachable_s (3 min) before it is
        given up (one network or sshd hiccup must not make it dead)."""
        st = self.alive(k)
        t0 = time.time()
        while st == "unreachable" and time.time() - t0 < unreachable_s:
            time.sleep(pause)
            st = self.alive(k)
        if st == "paused":
            self.call(self.host_argv(k, ["docker", "unpause", self.container(k)]))
        elif st in ("exited", "created", "dead"):
            self.call(self.host_argv(k, ["docker", "start", self.container(k)]))
        elif st != "running":
            return False
        for _ in range(30):
            if self.alive(k) == "running" and self.ssh_from_head(k):
                # leftovers of the job that was running when the host failed
                for prog in ("cohesive_extrin", "prted"):
                    self.call(self.node_argv(k, ["pkill", "-KILL", "-x", prog], user="root"), timeout=20)
                self.call(self.node_argv(k, ["pkill", "-TERM", "-f", "memsample.py"], user="root"), timeout=20)
                return True
            time.sleep(1)
        return False

    def abort(self):
        """Stops any job of the session on every reachable host (mn_session.sh stop)."""
        for k in range(len(self.hosts)):
            if self.alive(k) == "running":
                for prog in ("mpirun", "prterun", "cohesive_extrin", "prted", "python3"):
                    self.call(self.node_argv(k, ["pkill", "-KILL", "-x", prog], user="root"), timeout=20)

    def set_memory(self, k, memory):
        """Test hook (--force-oom): change host k's container memory limit."""
        self.call(self.host_argv(k, ["docker", "update", "--memory", memory, "--memory-swap", memory,
                                     self.container(k)]), check=True)

    def put(self, local_file, dest):
        if not dest.startswith(WORK + "/") or ".." in dest.split("/"):
            raise ClusterError(f"put: destination must be under {WORK}/")
        with open(local_file, "rb") as f:
            data = f.read()
        self.call(self.node_argv(0, ["sh", "-c", 'mkdir -p "$(dirname "$1")" && cat > "$1.tmp" && mv "$1.tmp" "$1"',
                                     "_", dest], stdin=True), data=data, timeout=1800, check=True)
        _, out, _ = self.call(self.node_argv(0, ["sha256sum", dest]), check=True)
        got = out.decode().split()[0]
        want = hashlib.sha256(data).hexdigest()
        if got != want:
            raise ClusterError(f"put {dest}: sha256 {got} != {want}")
        return got

    # -- preflight
    def check(self):
        report = {"hosts": [], "problems": []}
        for k, (addr, slots) in enumerate(self.hosts):
            st = self.alive(k)
            ok = st == "running" and self.ssh_from_head(k)
            report["hosts"].append({"address": addr, "slots": slots, "state": st, "ssh_from_head": ok,
                                    "image": self.image_id(k),
                                    "description": self.describe(k) if st == "running" else None})
            if not ok:
                report["problems"].append(f"{addr}: container {st}, SSH from the head {'ok' if ok else 'failed'}")
        ids = {h["image"] for h in report["hosts"]}
        if len(ids) != 1:
            report["problems"].append(f"image ids differ between hosts: {sorted(map(str, ids))}")
        if report["problems"]:
            return report
        np_all = sum(s for _, s in self.hosts)
        rd = f"{WORK}/check"
        for k in range(len(self.hosts)):   # --wdir must exist on every host
            self.call(self.node_argv(k, ["mkdir", "-p", rd]), check=True)
        self.call(self.node_argv(0, ["sh", "-c", f"cat > {rd}/hostfile"], stdin=True),
                  data="".join(f"{a} slots={s}\n" for a, s in self.hosts).encode(), check=True)
        probe = (CPUS_SIB + 'echo "MNCHECK $(hostname) $OMPI_COMM_WORLD_RANK ${OPENBLAS_CORETYPE:-unset} '
                 '${OPENBLAS_NUM_THREADS:-unset} ${OMP_NUM_THREADS:-unset} ${SCOTCH_PTHREAD_NUMBER:-unset} '
                 '${AKA_SEED:-unset} $cpus ${OMPI_COMM_WORLD_LOCAL_RANK:-unset} $sib"')
        cpp = self.conf["code"] == "cpp"
        if cpp:   # the harness's variables on every rank, and its driver resolving its libraries there
            probe += ('; echo "MNCPP $OMPI_COMM_WORLD_RANK ${AKANTU_RS_FILL_NODES_TO_ELEMENTS:-unset} '
                      '${LD_LIBRARY_PATH:-unset} $(sha256sum ' + CPP_DRIVER + ' | cut -d\' \' -f1) '
                      '$(ldd ' + CPP_DRIVER + ' | grep -c \'not found\') '
                      '$(ldd ' + CPP_DRIVER + ' | awk \'$1 ~ /^libakantu/ {print $3}\')"')
        script = run_script(rd, mpirun_argv(np_all, f"{rd}/hostfile", rd, self.cidr, self.conf["bind"], self.shared_cpus()),
                            app_argv(self.conf["bind"], ["sh", "-c", probe]), self.conf["code"], self.conf["fnte"])
        rc, _, _ = self.call(self.node_argv(0, ["bash", "-c", script]), timeout=300)
        _, out, _ = self.call(self.node_argv(0, ["cat", f"{rd}/out.txt", f"{rd}/exit"]))
        _, err, _ = self.call(self.node_argv(0, ["cat", f"{rd}/err.txt"]))
        lines = [ln.split() for ln in out.decode(errors="replace").splitlines() if ln.startswith("MNCHECK ")]
        report["mpirun_exit"] = rc
        report["ranks"] = sorted(([int(f[2])] + f[1:2] + f[3:] for f in lines if len(f) == 11),
                                 key=lambda r: r[0])
        report["report_bindings"] = [ln for ln in err.decode(errors="replace").splitlines() if "bound" in ln][:64]
        report["problems"] += check_problems(report["ranks"], self.hosts, self.conf["bind"])
        if cpp:
            cl = [ln.split() for ln in out.decode(errors="replace").splitlines() if ln.startswith("MNCPP ")]
            report["cpp"] = sorted(([int(f[1])] + f[2:] for f in cl if len(f) == 7), key=lambda r: r[0])
            report["problems"] += cpp_check_problems(report["cpp"], np_all, self.conf["fnte"])
        return report

    # -- one run
    def run(self, name, outdir, np_, nhosts, mesh, steps, partition="scotch", solver="auto", extra=(),
            timeout_s=None, watch_s=15, host_idx=None, fail=None, template=TEMPLATE):
        """One MPI run; returns its status dict (also OUTDIR/status.txt)."""
        outdir = Path(outdir)
        outdir.mkdir(parents=True, exist_ok=True)
        idx = list(host_idx) if host_idx is not None else list(range(nhosts))
        hosts = [self.hosts[k] for k in idx]
        per = placement(np_, len(idx), hosts)
        if partition not in ("scotch", "none") and not partition.startswith("file:"):
            raise ClusterError("partition: scotch | none | file:/mn/work/...")
        if solver not in ("auto", "eigen"):
            raise ClusterError("solver: auto | eigen")
        if not re.fullmatch(RUN_NAME, name or ""):   # No `.`, no `/`
            raise ClusterError(f"run name {name!r}: {RUN_NAME}")
        rd = f"{WORK}/runs/{name}"
        lock = open(self.dir / "run.lock", "w")
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            raise ClusterError("refused: another run holds the session lock (one run at a time)")
        try:
            return self._run(name, outdir, np_, idx, hosts, per, rd, mesh, steps, partition, solver,
                             list(extra), timeout_s, watch_s, fail, template)
        finally:
            lock.close()

    def _run(self, name, outdir, np_, idx, hosts, per, rd, mesh, steps, partition, solver, extra,
             timeout_s, watch_s, fail, template):
        code = self.conf["code"]
        st = {"case": code, "name": name, "np": np_, "nb_hosts": len(idx),
              "hosts": [a for a, _ in hosts], "ranks_per_host": per, "mesh": mesh, "steps": steps,
              "partition": partition, "solver": solver, "extra": extra, "timeout_s": timeout_s,
              "bind": self.conf["bind"], "backend": self.conf["backend"], "started": utc()}
        driver, extra_mpirun, diag = diag_options(os.environ)
        if driver and code == "cpp":
            raise ClusterError("MN_DRIVER applies to code = rust only")
        driver = driver or (CPP_DRIVER if code == "cpp" else DRIVER)
        if diag:
            st["diagnostics"] = diag
        down = self._settle(idx, {})
        if down:
            raise ClusterError(f"refused: host(s) {', '.join(down)} not running")
        for k in idx:
            rc, _, _ = self.call(self.node_argv(k, ["pgrep", "-x", "cohesive_extrin"]), timeout=30)
            if rc == 0:
                raise ClusterError(f"refused: a cohesive_extrinsic process runs on {self.addr(k)} (one run at a time)")
        if (np_ == 1) != (partition == "none"):
            raise ClusterError("partition: none at 1 rank, scotch or file:... at more ranks")
        extra_in = list(extra) + (["export_partition=true"] if np_ > 1 else [])
        if code == "cpp":
            # c1prep_campaign.sh's C++ run: run-one.sh's arguments blocked_node=global
            # energy_precision=17 output_energy=true [export_partition=true] [extra]; the harness
            # partitions with Scotch itself (AKA_SEED = 1): partition file:... is the file its
            # exported partition.csv must equal (partition_matches), not an input
            st["fnte"] = self.conf["fnte"]
            text = input_text_cpp(Path(template).read_text(), steps, mesh, solver,
                                  ["blocked_node=global", "energy_precision=17", "output_energy=true"]
                                  + (["export_partition=true"] if np_ > 1 else []) + list(extra))
        else:
            text = input_text(Path(template).read_text(), steps, mesh, partition, solver, extra_in)
        (outdir / "input.dat").write_text(text)
        hostfile = hostfile_text(hosts, per)
        (outdir / "hostfile").write_text(hostfile)
        limits = self.each(lambda k: self._prepare(k, rd, text, hostfile), idx)
        st["limits"] = {self.addr(k): v for k, v in limits.items()}
        st["host_descriptions"] = {self.addr(k): v for k, v in self.each(self.describe, idx).items()}
        oom0 = self.each(self._oom_kills, idx)
        for k in idx:
            self.call(self.node_argv(k, ["python3", "-I", SAMPLER, f"{rd}/mem-h{idx.index(k) + 1}.jsonl"],
                                     detach=True), timeout=60)
        script = run_script(rd, mpirun_argv(np_, f"{rd}/hostfile", rd, self.cidr, self.conf["bind"], self.shared_cpus(),
                                            extra_mpirun),
                            app_argv(self.conf["bind"], [driver, "--aka_input_file", "input.dat"], record=True),
                            code, self.conf["fnte"], driver)
        (outdir / "run.sh").write_text(script)
        t0 = time.time()
        log(f"run {name}: {np_} ranks on {len(idx)} hosts ({per} per host: {', '.join(st['hosts'])}), "
            f"{steps} steps, partition {partition}, solver {solver}"
            + (f", limit {timeout_s / 3600:.2f} h" if timeout_s else ""))
        with open(outdir / "launch.log", "wb") as lf:
            proc = subprocess.Popen(self.node_argv(0, ["bash", "-c", script]), stdout=lf,
                                    stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, start_new_session=True)
            killed, lost, next_watch, failer, strikes = None, [], t0 + watch_s, None, {}
            while proc.poll() is None:
                time.sleep(1)
                now = time.time()
                if fail and failer is None and now - t0 >= fail[2]:
                    log(f"run {name}: test hook: {fail[1]} host {self.addr(fail[0])} after {fail[2]} s")
                    failer = self.fail_host(fail[0], fail[1])
                if timeout_s and now - t0 > timeout_s:
                    killed = "timeout"
                elif now >= next_watch:
                    next_watch = now + watch_s
                    lost = self._probe(idx, strikes)
                    if lost:
                        killed = "host-lost"
                if killed:
                    log(f"run {name}: {killed}" + (f" ({', '.join(lost)})" if lost else "") + ": stopping the job")
                    self._kill(proc, idx)
                    break
            proc.wait()
            if failer is not None:
                failer.wait()
        wall = time.time() - t0
        if not lost:
            # a failing host can end the job (its daemon's connection closes) before its
            # container or machine is seen gone: settle the probes, look again after a moment
            lost = self._settle(idx, strikes)
            if not lost and not killed:
                time.sleep(3)
                lost = self._settle(idx, strikes)
        st["strikes"] = {h: v for h, v in strikes.items() if v}
        st["killed"] = killed
        st["lost_hosts"] = lost
        st["launcher_wall_s"] = round(wall, 3)
        alive = [k for k in idx if self.addr(k) not in lost]
        for k in alive:
            self.call(self.node_argv(k, ["pkill", "-TERM", "-f", "memsample.py"]), timeout=30)
        time.sleep(1)
        oom1 = self.each(self._oom_kills, alive) if alive else {}
        st["oom_kills"] = {self.addr(k): (oom1[k] - oom0[k]) if k in oom1 and oom1[k] is not None
                           and oom0.get(k) is not None else None for k in idx}
        rank_hosts = {}
        for k in alive:
            names = self._collect(k, rd, outdir, head=(k == idx[0]), tag=idx.index(k) + 1)
            rank_hosts[self.addr(k)] = sorted(int(m.group(1)) for m in
                                              (re.fullmatch(r"summary_rank([0-9]+)\.json", n) for n in names) if m)
        st["rank_hosts"] = rank_hosts
        st["bindings"] = bindings_from({p.name: p.read_text() for p in outdir.glob("binding_rank*.txt")})
        st["environment"], st["environment_differs"] = env_from({p.name: p.read_text()
                                                                for p in outdir.glob("env_rank*.txt")},
                                                               ENV_NOT_COMPARED[code])
        st["binding_problems"] = binding_problems(st["bindings"], np_, self.conf["bind"])
        st["nb_rank_summaries"] = sum(len(v) for v in rank_hosts.values())
        st["summary"] = (outdir / "summary.json").exists()
        ex = (outdir / "exit").read_text().strip() if (outdir / "exit").exists() else ""
        st["exit"] = int(ex) if ex.lstrip("-").isdigit() else None
        try:
            st["wall_s"] = float((outdir / "wall.txt").read_text())
        except (OSError, ValueError):
            st["wall_s"] = None
        st["host_peak_kib"] = {}
        for i, k in enumerate(idx):
            f = outdir / f"mem-h{i + 1}.jsonl"
            st["host_peak_kib"][self.addr(k)] = host_peak_kib(f.read_text()) if f.exists() else None
        if np_ > 1 and partition.startswith("file:") and (outdir / "partition.csv").exists():
            _, out, _ = self.call(self.node_argv(0, ["sha256sum", partition[len("file:"):]]), timeout=120)
            st["partition_input_sha256"] = out.decode().split()[0] if out else None
            st["partition_matches"] = st["partition_input_sha256"] == sha256_file(outdir / "partition.csv")
        if (outdir / "partition.csv").exists():
            st["partition_sha256"] = sha256_file(outdir / "partition.csv")
        if (outdir / "binary.sha256").exists():
            st["binary_sha256"] = ((outdir / "binary.sha256").read_text().split() or [None])[0]
        if code == "cpp":
            st["aborted"] = (outdir / "aborted.txt").read_text().strip() if (outdir / "aborted.txt").exists() else None
            st["ud216_markers"] = sum((outdir / n).read_text(errors="replace").count(m)
                                      for n in ("out.txt", "err.txt") if (outdir / n).exists() for m in MUMPS_MARKERS)
            if np_ > 1 and partition.startswith("file:") and "partition_matches" not in st:
                st["partition_matches"] = False   # no partition.csv: the harness did not export it
        st["outcome"] = classify(st)
        if st["outcome"] == "failed":
            why = failure_reasons(st, code, partition, (outdir / "partition.csv").exists())
            if why:
                st["failure"] = "; ".join(why)
        st["finished"] = utc()
        write_status(outdir / "status.txt", st)
        log(f"run {name}: {st['outcome']} (exit {st['exit']}, {wall:.0f} s"
            + (f", lost {', '.join(lost)}" if lost else "")
            + (f", oom on {[h for h, v in st['oom_kills'].items() if v]}" if st["outcome"] == "oom" else "")
            + (f", binding problems: {'; '.join(st['binding_problems'][:8])}" if st.get("binding_problems") else "")
            + (f", {st['failure']}" if st.get("failure") and not st.get("binding_problems") else "")
            + ")")
        return st

    def _probe(self, idx, strikes):
        return watch_step(strikes, {self.addr(k): self.alive(k) for k in idx})

    def _settle(self, idx, strikes, pause=5):
        """Probes until no host is between its first and its last strike; the lost hosts."""
        lost = self._probe(idx, strikes)
        while any(0 < strikes.get(self.addr(k), 0) < STRIKES and self.addr(k) not in lost for k in idx):
            time.sleep(pause)
            lost = self._probe(idx, strikes)
        return lost

    def _prepare(self, k, rd, text, hostfile):
        self.call(self.node_argv(k, ["sh", "-c", 'rm -rf "$1" && mkdir -p "$1" && cat > "$1/input.dat"', "_", rd],
                                 stdin=True), data=text.encode(), check=True)
        self.call(self.node_argv(k, ["sh", "-c", 'cat > "$1/hostfile"', "_", rd], stdin=True),
                  data=hostfile.encode(), check=True)
        _, out, _ = self.call(self.node_argv(k, ["sh", "-c",
                                                 'echo "memory.max $(cat /sys/fs/cgroup/memory.max)"; '
                                                 'echo "cpuset $(cat /sys/fs/cgroup/cpuset.cpus.effective 2>/dev/null)"; '
                                                 'echo "nproc $(env -u OMP_NUM_THREADS nproc)"; echo "hostname $(hostname)"']), check=True)
        return dict(line.split(" ", 1) for line in out.decode().splitlines() if " " in line)

    def describe(self, k):
        """Host k's description (HOST_PROBE in its node container, the Docker version of its
        daemon); None fields where something is not readable."""
        rc, out, _ = self.call(self.node_argv(k, ["python3", "-I", "-c", HOST_PROBE]), timeout=60)
        try:
            d = json.loads(out.decode()) if rc == 0 else {}
        except ValueError:
            d = {}
        rc, out, _ = self.call(self.host_argv(k, ["docker", "version", "--format", "{{.Server.Version}}"]),
                               timeout=60)
        d["docker"] = out.decode().strip() if rc == 0 and out.strip() else None
        return d

    def _oom_kills(self, k):
        rc, out, _ = self.call(self.node_argv(k, ["cat", "/sys/fs/cgroup/memory.events"]), timeout=30)
        if rc != 0:
            return None
        for line in out.decode().splitlines():
            f = line.split()
            if len(f) == 2 and f[0] == "oom_kill":
                return int(f[1])
        return None

    def _kill(self, proc, idx):
        for prog in ("mpirun", "prterun"):
            self.call(self.node_argv(idx[0], ["pkill", "-TERM", "-x", prog], user="root"), timeout=20)
        for _ in range(30):
            if proc.poll() is not None:
                break
            time.sleep(1)
        for k in idx:
            if self.alive(k) == "running":
                for prog in ("cohesive_extrin", "prted", "prterun", "mpirun"):
                    self.call(self.node_argv(k, ["pkill", "-KILL", "-x", prog], user="root"), timeout=20)
        for _ in range(15):
            if proc.poll() is not None:
                return
            time.sleep(1)
        os.killpg(proc.pid, 9)

    def _collect(self, k, rd, outdir, head, tag):
        """Copies host k's files of the run into outdir; returns their names."""
        pat = "-type f" if head else ("-type f \\( -name 'summary_rank*.json' -o -name 'mem-h*.jsonl' "
                                      "-o -name 'limits-h*.txt' -o -name 'binding_rank*.txt' -o -name 'env_rank*.txt' \\)")
        rc, out, err = self.call(self.node_argv(k, ["sh", "-c", f'cd "$1" && find . -maxdepth 1 {pat} -print0 '
                                                    f'| tar --null -T - -cf -', "_", rd]), timeout=1800)
        if rc != 0:
            log(f"collect {self.addr(k)}: exit {rc}: {err.decode(errors='replace')[-300:]}")
            return []
        names = []
        with tarfile.open(fileobj=io.BytesIO(out), mode="r:") as t:
            for m in t.getmembers():
                n = m.name[2:] if m.name.startswith("./") else m.name
                if not m.isfile() or "/" in n or n in ("", ".", ".."):
                    continue
                if not head and n.startswith(("summary_rank", "binding_rank", "env_rank")) and (outdir / n).exists():
                    continue
                with t.extractfile(m) as src:
                    (outdir / n).write_bytes(src.read())
                names.append(n)
        return names


def known_host_names(text):
    """Host names of plain (not hashed) known_hosts lines; `[addr]:22` counts as addr."""
    names = set()
    for line in text.splitlines():
        f = line.split()
        if len(f) < 3 or f[0].startswith(("#", "|", "@")):
            continue
        for n in f[0].split(","):
            names.add(n[1:-4] if n.startswith("[") and n.endswith("]:22") else n)
    return names


def session_busy(session_dir):
    """True while a session (mn_session.py run) holds SESSION/session.lock."""
    try:
        f = open(Path(session_dir) / "session.lock", "a")
    except OSError:
        return False
    try:
        fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(f, fcntl.LOCK_UN)
        return False
    except OSError:
        return True
    finally:
        f.close()


def cpp_check_problems(rows, np_all, fnte):
    """code = cpp: problems of the preflight's per-rank lines [rank, fnte, LD_LIBRARY_PATH, driver
    sha256, unresolved libraries, libakantu path]: every rank, the forwarded variables, one driver
    on every host, all its libraries resolved, libakantu from the install."""
    if [r[0] for r in rows] != list(range(np_all)):
        return [f"preflight (cpp): {len(rows)} driver lines, expected ranks 0..{np_all - 1}"]
    problems = []
    for r in rows:
        if r[1] != fnte or r[2] != CPP_LIBDIR:
            problems.append(f"rank {r[0]}: AKANTU_RS_FILL_NODES_TO_ELEMENTS {r[1]}, LD_LIBRARY_PATH {r[2]} "
                            f"(expected {fnte}, {CPP_LIBDIR})")
        if r[4] != "0" or not r[5].startswith(CPP_LIBDIR + "/"):
            problems.append(f"rank {r[0]}: {r[4]} unresolved libraries, libakantu {r[5]}")
    shas = {r[3] for r in rows}
    if len(shas) != 1:
        problems.append(f"the C++ driver differs between ranks: {sorted(shas)}")
    return problems


def check_problems(ranks, hosts, bind):
    """Problems in the preflight's per-rank lines [rank, hostname, coretype, ob, omp, scotch, seed, cpus,
    local rank, hardware-thread siblings of the first CPU (`-`: unreadable)]."""
    problems = []
    np_all = sum(s for _, s in hosts)
    if len(ranks) != np_all or [r[0] for r in ranks] != list(range(np_all)):
        return [f"preflight: {len(ranks)} rank lines, expected ranks 0..{np_all - 1}"]
    start = 0
    for addr, slots in hosts:
        group = ranks[start:start + slots]
        names = {r[1] for r in group}
        if len(names) != 1:
            problems.append(f"{addr}: ranks {start}..{start + slots - 1} on several hostnames {sorted(names)}")
        if start + slots < np_all and ranks[start + slots][1] in names:
            problems.append(f"{addr}: more than {slots} ranks on {sorted(names)}")
        if bind in ("core", "self"):
            sets = [r[7] for r in group]
            if len(set(sets)) != len(sets):
                problems.append(f"{addr}: ranks share a CPU set {sets}")
            if bind == "self" and not all(c.isdigit() for c in sets):
                problems.append(f"{addr}: a rank is not bound to one CPU {sets}")
            # bind-rank.sh picks the core by the rank's index on its host
            local = [r[8] if len(r) > 8 else "unset" for r in group]
            if bind == "self" and local != [str(i) for i in range(slots)]:
                problems.append(f"{addr}: local ranks {local}, expected 0..{slots - 1}")
            # one physical core per rank: distinct hardware-thread sibling sets (a host with two
            # threads per core gives ranks 0 and 1 the two threads of one core otherwise)
            sibs = [r[9] for r in group if len(r) > 9 and r[9] != "-"]
            if bind == "self" and len(set(sibs)) != len(sibs):
                problems.append(f"{addr}: ranks share a physical core (hardware-thread siblings {sibs})")
        start += slots
    for r in ranks:
        got = dict(zip(PIN_VARS, r[2:7]))
        bad = {k: v for k, v in got.items() if v != PIN_EXPECT[k]}
        if bad:
            problems.append(f"rank {r[0]} on {r[1]}: {bad}")
    return problems


def main(argv):
    p = argparse.ArgumentParser(prog="mn_cluster.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for c in ("keygen", "check", "status"):
        sub.add_parser(c).add_argument("session")
    s = sub.add_parser("up")
    s.add_argument("session")
    s.add_argument("--load")
    s.add_argument("--expect-id")
    s = sub.add_parser("down")
    s.add_argument("session")
    s.add_argument("--volumes", action="store_true")
    s = sub.add_parser("put")
    s.add_argument("session")
    s.add_argument("file")
    s.add_argument("dest")
    s = sub.add_parser("run")
    s.add_argument("session")
    s.add_argument("outdir")
    s.add_argument("--name")
    s.add_argument("--np", type=int, required=True)
    s.add_argument("--hosts", type=int, required=True)
    s.add_argument("--mesh", required=True)
    s.add_argument("--steps", type=int, required=True)
    s.add_argument("--partition", default="scotch")
    s.add_argument("--solver", default="auto")
    s.add_argument("--timeout-s", type=float)
    s.add_argument("--watch-s", type=float, default=15)
    s.add_argument("--fail-host", help="test hook K:MODE:DELAY_S (host number from 1, stop|pause)")
    s.add_argument("extra", nargs="*")
    a = p.parse_args(argv)
    try:
        c = Cluster(a.session)
        if a.cmd in ("up", "down", "run") and session_busy(c.dir):
            raise ClusterError(f"refused: a session runs on {c.dir} (mn_session.sh status / stop first)")
        if a.cmd == "keygen":
            print(c.keygen())
            print(f"(public key of the session; private key in {c.ssh_dir}, never to be copied off the hosts)",
                  file=sys.stderr)
        elif a.cmd == "up":
            if a.load:
                print(json.dumps(c.load(a.load, a.expect_id)))
            c.up()
        elif a.cmd == "check":
            r = c.check()
            (c.dir / "check.json").write_text(json.dumps(r, indent=1) + "\n")
            for h in r["hosts"]:
                print(f"{h['address']:<20} slots {h['slots']:>3} {h['state']:<10} ssh {h['ssh_from_head']} {h['image']}")
            for rk in r.get("ranks", []):
                print("  rank " + " ".join(map(str, rk)))
            print("CHECK: " + ("PASS" if not r["problems"] else "FAIL: " + "; ".join(r["problems"])))
            return 0 if not r["problems"] else 1
        elif a.cmd == "status":
            for h, s_ in c.status().items():
                print(f"{h} {s_}")
        elif a.cmd == "down":
            c.down(a.volumes)
        elif a.cmd == "put":
            print(c.put(a.file, a.dest))
        elif a.cmd == "run":
            for kv in a.extra:
                if not re.fullmatch(r"[a-z_][a-z0-9_]*=[A-Za-z0-9_.:/+-]+", kv):
                    p.error(f"extra parameter {kv!r}: key=value")
            fail = None
            if a.fail_host:
                k, mode, d = a.fail_host.split(":")
                fail = (int(k) - 1, mode, float(d))
            st = c.run(a.name or Path(a.outdir).name, a.outdir, a.np, a.hosts, a.mesh, a.steps, a.partition,
                       a.solver, a.extra, a.timeout_s, a.watch_s, fail=fail)
            return 0 if st["outcome"] == "ok" else 1
    except ClusterError as e:
        print(f"mn_cluster.py: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
