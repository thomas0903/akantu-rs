#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): records and checks the
# core binding of the ranks of one cohesive run of the final measurement
# campaign (scripts/perf/final/README.md).
"""Record which cores every rank of a run may use, and check it.

    bindings.py record OUT.json NP [--comm cohesive_extrin] [--timeout S]
    bindings.py check OUT.json NP THREADS CORE_LIST

record: polls /proc every 0.1 s (world-readable files; container processes
are visible from the host) until NP processes named COMM exist, then writes
for each its MPI rank (OMPI_COMM_WORLD_RANK from /proc/<pid>/environ when
readable, else null), pid and Cpus_allowed_list, and the NUMA node of every
host CPU (/sys/devices/system/node). Exits 0 after writing, 1 at the
timeout (default: until killed).

check: the recorded binding is the planned one: NP ranks, rank r allowed on
exactly THREADS cores, the r-th block of THREADS cores of CORE_LIST (the
host's cores ordered NUMA node by NUMA node), every block inside one NUMA
node. Prints the rank -> cores table; exit 1 with the reason otherwise.
"""

import json
import os
import sys
import time


def cpulist(text):
    out = []
    for part in text.strip().split(","):
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-")
            out.extend(range(int(a), int(b) + 1))
        else:
            out.append(int(part))
    return out


def numa_of_cpus():
    res = {}
    base = "/sys/devices/system/node"
    if not os.path.isdir(base):
        return res
    for d in os.listdir(base):
        if d.startswith("node") and d[4:].isdigit():
            try:
                with open(f"{base}/{d}/cpulist") as f:
                    for c in cpulist(f.read()):
                        res[c] = int(d[4:])
            except OSError:
                pass
    return res


def snapshot(comm):
    procs = []
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            with open(f"/proc/{p}/comm") as f:
                if f.read().strip() != comm:
                    continue
            with open(f"/proc/{p}/status") as f:
                allowed = next(ln.split(":", 1)[1].strip() for ln in f
                               if ln.startswith("Cpus_allowed_list"))
        except (OSError, StopIteration):
            continue
        rank = None
        try:
            with open(f"/proc/{p}/environ", "rb") as f:
                for kv in f.read().split(b"\0"):
                    if kv.startswith(b"OMPI_COMM_WORLD_RANK="):
                        rank = int(kv.split(b"=", 1)[1])
        except (OSError, ValueError):
            pass
        procs.append({"pid": int(p), "rank": rank, "cpus": allowed})
    return procs


def record(out, np_, comm, timeout):
    t0 = time.time()
    while timeout is None or time.time() - t0 < timeout:
        procs = snapshot(comm)
        if len(procs) >= np_ and all(
                p["rank"] is not None for p in procs) or len(procs) > np_:
            break
        if len(procs) == np_ and time.time() - t0 > 5:
            break   # ranks unreadable (another uid): pid order then
        time.sleep(0.1)
    else:
        return 1
    procs.sort(key=lambda p: (p["rank"] is None, p["rank"], p["pid"]))
    json.dump({"utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "comm": comm, "ranks": procs,
               "numa": {str(c): n for c, n in sorted(numa_of_cpus().items())}},
              open(out, "w"), indent=1)
    return 0


def check(path, np_, threads, core_list):
    if not os.path.exists(path):
        print(f"no binding record {path}")
        return 1
    d = json.load(open(path))
    numa = {int(k): v for k, v in d.get("numa", {}).items()}
    order = [int(c) for c in core_list.split(",") if c]
    ranks = d["ranks"]
    bad = []
    if len(ranks) != np_:
        bad.append(f"{len(ranks)} processes recorded, {np_} expected")
    for i, p in enumerate(ranks):
        r = p["rank"] if p["rank"] is not None else i
        got = sorted(cpulist(p["cpus"]))
        want = sorted(order[r * threads:(r + 1) * threads])
        nodes = sorted({numa.get(c, -1) for c in got})
        print(f"rank {r}: cores {p['cpus']} (NUMA {','.join(map(str, nodes))})")
        if got != want:
            bad.append(f"rank {r}: cores {got}, planned {want}")
        if len(nodes) > 1:
            bad.append(f"rank {r}: cores on NUMA nodes {nodes}")
    for b in bad:
        print("BINDING:", b)
    return 1 if bad else 0


def main(argv):
    if len(argv) < 4 or argv[1] not in ("record", "check"):
        print(__doc__)
        return 2
    if argv[1] == "record":
        comm, timeout = "cohesive_extrin", None
        if "--comm" in argv:
            comm = argv[argv.index("--comm") + 1]
        if "--timeout" in argv:
            timeout = float(argv[argv.index("--timeout") + 1])
        return record(argv[2], int(argv[3]), comm, timeout)
    return check(argv[2], int(argv[3]), int(argv[4]), argv[5])


if __name__ == "__main__":
    sys.exit(main(sys.argv))
