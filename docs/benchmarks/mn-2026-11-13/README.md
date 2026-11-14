# Multi-node runs and the per-copy check at full size (2026-11-13)

Eight AWS `r7a.2xlarge` (8 cores, 64 GiB each, AMD EPYC 9R14) in one
cluster placement group, up to 8 MPI ranks per host, run with the
multi-node launcher and session of `crates/akantu-bench/scripts/mn_*`.
Every run sets `rust_cohesive_compat = upstream` and
`rust_doubled_node_load = follows_elements`. Host names are written
`host1` … `host8` in the logs and `node-host1` … `node-host8` in the status
and check files (`host1` is the head).

These runs were made before a later fix of the Neumann store for boundary
elements whose nodal shares differ. With the benchmark's `_triangle_3`
boundaries it changes them at rounding level only; the current code does
not reproduce their energies bit for bit.

| Folder | What | Driver |
|---|---|---|
| `smoke/` | the launcher check made first: `SMOKE: PASS` (Eigen on 1 host against 8 hosts bitwise; MUMPS: distribution exact, counts and insertion list equal; the first 64-rank job across eight hosts) | `b321054d…` |
| `main/` | 13 runs, all `ok`: `joss64x8` and `joss32x8` (the JOSS mesh, 5,000 steps, the 64- and 32-rank partitions of 12 November), h = 0.04 at 8 ranks on 1 / 2 / 4 / 8 hosts and at 16 / 32 / 64 ranks on 2 / 4 / 8 hosts, the weak-scaling runs `h0032-p16-h2`, `h0025-p32-h4`, `joss-p64-h8-1500` | `b321054d…` |
| `percopy/` | the per-copy load check at full size: `percopy-joss64x8` and `percopy-joss32x8`, the JOSS mesh to step 3,200, a check at every step (the mode: `docs/benchmarks/percopy-2026-11-13/`) | `d6d03541…` |

Per run: `summary.json`, `chrono.csv`, `wall.txt`, `status.txt`,
`binary.sha256`, `input.dat`, `hostfile`; `energies.csv` and
`insertions.csv` for the smoke runs, `joss32x8` and the per-copy runs
(`joss64x8`'s are in `docs/benchmarks/c1-rerun-2026-11-13/joss64x8-control/`);
`load_check.csv` for the per-copy runs. Per session: `check.json` (the preflight's binding report); every planned
run of the three sessions completed, and the sessions' logs and state are
not kept. The per-rank
summaries, the memory samples and the partition files are not kept.

The per-copy check passed in both runs: 3,201 checks each, none failed, the
largest difference 2.57e-15 of the largest nodal load (step 3,141; bound
1e-12), the first doubled loaded node at step 829, 12,174 and 12,172
doubled loaded nodes at step 3,200. It covers steps 0 to 3,200 and the
placement mechanism; the boundary connectivity after an insertion and the
integration of the tractions rest on the oracle replays.

Every number of these runs in `docs/BENCHMARKS.md` is printed by `python3
-I crates/akantu-bench/scripts/results_report.py smoke multinode percopy`
(output: `docs/benchmarks/results-2026-11-13.txt`).
