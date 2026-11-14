#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: memory sampler of the cohesive scalability runs
# (C++ harness and Rust driver alike), run on the Linux host that runs the
# containers (an x86_64 Linux host).
"""Sample the resident memory of every benchmark process.

    python3 -I memsample.py OUT.jsonl [--interval 0.25] [--comm cohesive_extrin]

Every `--interval` seconds, for each process whose `/proc/<pid>/comm` is
COMM (the kernel truncates names to 15 characters: both drivers'
executables are called `cohesive_extrinsic`), one JSON line

    {"t": <uptime s>, "procs": [[pid, start_s, VmRSS_kiB, VmHWM_kiB], ...]}

is appended to OUT (only when at least one such process exists); `start_s`
is the process start in the same clock (`/proc/<pid>/stat` field 22 over
`SC_CLK_TCK`). The first line is a header with the clock tick and interval.
Runs until SIGTERM or SIGINT. Reads `/proc` only (world-readable files), so
it sees container processes from the host without entering them.
"""

import json
import os
import signal
import sys
import time


def uptime():
    with open("/proc/uptime") as f:
        return float(f.read().split()[0])


def status_kib(pid, keys):
    out = {}
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                k, _, rest = line.partition(":")
                if k in keys:
                    out[k] = int(rest.split()[0])
    except (OSError, ValueError, IndexError):
        return None
    return out


def start_s(pid, tick):
    try:
        with open(f"/proc/{pid}/stat") as f:
            data = f.read()
    except OSError:
        return None
    # field 2 (comm) may contain spaces: split after its closing parenthesis
    rest = data[data.rfind(")") + 2:].split()
    return int(rest[19]) / tick  # field 22 = rest[19] (rest starts at field 3)


def main(argv):
    out = None
    interval = 0.25
    comm = "cohesive_extrin"
    args = iter(argv[1:])
    for a in args:
        if a == "--interval":
            interval = float(next(args))
        elif a == "--comm":
            comm = next(args)
        elif out is None:
            out = a
        else:
            raise SystemExit(f"memsample.py: unexpected argument {a}")
    if out is None:
        raise SystemExit(__doc__)
    tick = os.sysconf("SC_CLK_TCK")
    stop = [False]

    def handler(signum, frame):
        stop[0] = True

    signal.signal(signal.SIGTERM, handler)
    signal.signal(signal.SIGINT, handler)
    with open(out, "w") as f:
        f.write(json.dumps({"header": True, "clk_tck": tick, "interval": interval,
                            "comm": comm, "t0": uptime()}) + "\n")
        while not stop[0]:
            t = uptime()
            procs = []
            for name in os.listdir("/proc"):
                if not name.isdigit():
                    continue
                try:
                    with open(f"/proc/{name}/comm") as c:
                        if c.read().strip() != comm:
                            continue
                except OSError:
                    continue
                st = status_kib(name, ("VmRSS", "VmHWM"))
                s0 = start_s(name, tick)
                if st and "VmRSS" in st and s0 is not None:
                    procs.append([int(name), s0, st["VmRSS"], st.get("VmHWM", 0)])
            if procs:
                f.write(json.dumps({"t": round(t, 3), "procs": procs}) + "\n")
                f.flush()
            time.sleep(interval)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
