#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): runs.csv of this directory from
# mn_cluster.py run directories (status.txt, summary.json, chrono.csv, the insertion
# split of rank 0) and the NIC counters read on every host before and after each run
# (<run>.nic0 / <run>.nic1: one line per host, tx packets, tx bytes, rx packets, rx bytes).
# rank 0's phases (s), the insertion check's split, NIC packets and bytes per step (all hosts)
import csv
import json
import os
import re
import sys
from pathlib import Path

cols = ["run", "code", "np", "hosts", "per_host", "mesh", "steps", "outcome", "binary_sha256", "started",
        "loop", "init", "check_cohesive_stress", "solve_step", "energies", "static_solve",
        "interpolation", "checks", "insertion", "nic_tx_pkt_per_step", "nic_tx_bytes_per_step", "env"]
# env: AKANTU_DIAG_* switches, LD_PRELOAD (the PMPI interposer), PMPI_BT, eigen (static solve)
out = []
for d in sys.argv[2:]:
    d = d.rstrip("/")
    st = {}
    for ln in Path(d + "/status.txt").read_text().splitlines():
        k, _, v = ln.partition("=")
        try:
            st[k] = json.loads(v)
        except ValueError:
            st[k] = v.strip()
    row = {"run": os.path.basename(d), "code": st.get("case"), "np": st.get("np"), "hosts": st.get("nb_hosts"),
           "per_host": st.get("ranks_per_host"), "mesh": os.path.basename(st.get("mesh", "")),
           "steps": st.get("steps"), "outcome": st.get("outcome"), "binary_sha256": st.get("binary_sha256"),
           "started": st.get("started")}
    if st.get("outcome") == "ok":
        sm = json.loads(Path(d + "/summary.json").read_text())
        r = list(csv.reader(Path(d + "/chrono.csv").read_text().splitlines(), skipinitialspace=True))
        ch = dict(zip(r[0], r[1]))
        row.update(loop=sm["loop_time_s"], init=sm["init_time_s"],
                   **{k: float(ch[k]) for k in ("check_cohesive_stress", "solve_step", "energies", "static_solve")})
        if os.path.exists(d + "/insertion_stats_rank0.csv"):
            q = list(csv.reader(Path(d + "/insertion_stats_rank0.csv").read_text().splitlines(), skipinitialspace=True))
            row.update(interpolation=float(q[1][1]), checks=float(q[1][2]), insertion=float(q[1][3]))
    try:
        a = [list(map(int, ln.split()[1:])) for ln in Path(d + ".nic0").read_text().splitlines()]
        b = [list(map(int, ln.split()[1:])) for ln in Path(d + ".nic1").read_text().splitlines()]
        row["nic_tx_pkt_per_step"] = round(sum(y[0] - x[0] for x, y in zip(a, b)) / st["steps"], 1)
        row["nic_tx_bytes_per_step"] = round(sum(y[1] - x[1] for x, y in zip(a, b)) / st["steps"])
    except (OSError, ValueError, IndexError):
        pass
    sh = Path(d + "/run.sh").read_text() if os.path.exists(d + "/run.sh") else ""
    env = sorted(set(re.findall(r"AKANTU_DIAG_[A-Z_]+|PMPI_BT|LD_PRELOAD", sh)))
    if "rust_sparse_solver = eigen" in Path(d + "/input.dat").read_text():
        env.append("eigen")
    row["env"] = " ".join(env)
    out.append([row.get(c, "") for c in cols])
with open(sys.argv[1], "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(cols)
    w.writerows(out)
