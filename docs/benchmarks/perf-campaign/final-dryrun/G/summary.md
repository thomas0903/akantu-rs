# Final campaign, part G: the full-size run

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T07:18:32Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 8 planned, 8 complete and checked (only these are tabulated).

Checks (`checks.jsonl`): 6 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 0 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 6 runs, 6 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

The plan printed before the runs (`plan.txt`):

```text
Part G plan: size dry, 2 repetition(s), 8 cores
series        groups   runs   expected
full               8      8      3 min
total              8      8      3 min

Runs one at a time, so the machine time is their sum in any order; the order removes waste:
the partitions of every rank count first (each run replays its file); the port at the largest
rank count first (the shortest full-size run, it measures that rank count's memory before the
C++ run); the longest and least needed run (8 ranks) last, so that a short budget drops it.
Placement fill: rank r with T threads on the r-th block of T cores of the host's cores
ordered NUMA node by NUMA node (one node filled before the next).
step                     cores              expected peak GiB  expected wall  cumulative
export np8               C++, partition only                            0 min       0.01h
export np4               C++, partition only                            0 min       0.02h
8:head                   8 (8 x 1)                        3.2          0 min       0.02h
8:cpp                    8 (8 x 1)                        4.0          0 min       0.03h
4x2:head                 8 (4 x 2)                        3.0          0 min       0.03h
8:head                   8 (8 x 1)                        3.2          0 min       0.03h
8:cpp                    8 (8 x 1)                        4.0          0 min       0.04h
4x2:head                 8 (4 x 2)                        3.0          0 min       0.04h
Estimates and their basis: scripts/perf/final/plan.py (EST_*), g-estimates.txt.
```

Table "Strong scaling" (one row per configuration, ranks x threads; placement: `fill` puts ranks on one NUMA node before the next, so on a two-node 128-core host the 8-, 32- and 64-rank runs use one node (one socket's memory bandwidth) and the 128-rank and 128-core hybrid runs both, which is part of the 64 → 128 efficiency; `spread` puts the same number of ranks on every node; the NUMA nodes column is read from each run's recorded binding; rank 0's Chrono phases, s, per step in ms; core-µs per element and step = (`solve_step` + check) × cores / tetrahedra / steps; loop efficiency = core-µs at the smallest core count of the same code (port: 8 cores) (C++: 8 cores) / core-µs here):

| code | ranks x threads | cores | placement, NUMA nodes | wall s | read_mesh | dist_mesh | init_full | static solve | `solve_step` ms/step | check ms/step | energies ms/step | loop s | core-µs/el/step | loop efficiency |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 8 x 1 | 8 | fill, 0 | 7.51 [7.46, 7.57] | 0.103 [0.101, 0.104] | 0.200 [0.199, 0.200] | 0.226 [0.225, 0.227] | 2.95 [2.90, 3.00] | 6.16 [6.12, 6.21] | 10.6 [10.6, 10.6] | 0.892 [0.874, 0.910] | 3.36 [3.35, 3.37] | 0.757 [0.755, 0.759] | 100% |
| port | 4 x 2 | 8 | fill, 0 | 10.1 [9.86, 10.2] | 0.103 [0.103, 0.104] | 0.191 [0.190, 0.192] | 0.399 [0.398, 0.401] | 4.18 [4.02, 4.33] | 6.44 [6.37, 6.50] | 15.0 [14.9, 15.1] | 1.39 [1.39, 1.39] | 4.29 [4.26, 4.32] | 0.967 [0.961, 0.974] | 78% |
| C++ | 8 x 1 | 8 | fill, 0 | 20.6 [20.4, 20.7] | 0.295 [0.293, 0.297] | 2.12 [2.11, 2.13] | 1.21 [1.20, 1.21] | 3.14 [3.03, 3.24] | 18.5 [18.2, 18.8] | 41.8 [41.7, 41.8] | 4.45 [4.12, 4.78] | 12.1 [12.0, 12.1] | 2.72 [2.71, 2.74] | 100% |

Table "Phases" (rank 0's Chrono totals, s; per step = total / steps):

| code | ranks | wall s | read_mesh | dist_mesh | init_full | static solve | before_step | `solve_step` s (ms/step) | check s (ms/step) | energies s (ms/step) | loop s | core-µs/el/step |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 8 | 7.51 [7.46, 7.57] | 0.103 [0.101, 0.104] | 0.200 [0.199, 0.200] | 0.226 [0.225, 0.227] | 2.95 [2.90, 3.00] | 0.153 [0.150, 0.157] | 1.23 [1.22, 1.24] (6.16 [6.12, 6.21]) | 2.13 [2.12, 2.13] (10.6 [10.6, 10.6]) | 0.178 [0.175, 0.182] (0.892 [0.874, 0.910]) | 3.36 [3.35, 3.37] | 0.757 [0.755, 0.759] |
| C++ | 8 | 20.6 [20.4, 20.7] | 0.295 [0.293, 0.297] | 2.12 [2.11, 2.13] | 1.21 [1.20, 1.21] | 3.14 [3.03, 3.24] | 0.412 [0.410, 0.414] | 3.71 [3.65, 3.76] (18.5 [18.2, 18.8]) | 8.36 [8.35, 8.37] (41.8 [41.7, 41.8]) | 0.890 [0.824, 0.956] (4.45 [4.12, 4.78]) | 12.1 [12.0, 12.1] | 2.72 [2.71, 2.74] |
| port 4 x 2 | 4 | 10.1 [9.86, 10.2] | 0.103 [0.103, 0.104] | 0.191 [0.190, 0.192] | 0.399 [0.398, 0.401] | 4.18 [4.02, 4.33] | 0.272 [0.270, 0.275] | 1.29 [1.27, 1.30] (6.44 [6.37, 6.50]) | 3.00 [2.98, 3.02] (15.0 [14.9, 15.1]) | 0.278 [0.278, 0.278] (1.39 [1.39, 1.39]) | 4.29 [4.26, 4.32] | 0.967 [0.961, 0.974] |

Table "Port / upstream at the same rank count":

| ranks | wall | `init_full` | static solve | `dist_mesh` | `solve_step` | check | energies | loop | largest rank peak | total peak |
|---|---|---|---|---|---|---|---|---|---|---|
| 8 | 0.365 [0.361, 0.370] | 0.187 [0.187, 0.187] | 0.942 [0.895, 0.989] | 0.0941 [0.0935, 0.0948] | 0.333 [0.330, 0.335] | 0.254 [0.254, 0.255] | 0.201 [0.190, 0.212] | 0.278 [0.278, 0.279] | 0.854 [0.762, 0.946] | 0.822 [0.785, 0.859] |

Table "Memory" (GiB; sampler = `memsample.py`, VmHWM of every rank; container = the cgroup's `memory.peak`; static = the port's `VmHWM` at the end of the static solve):

| code | ranks | largest rank | total (sampler) | container | static: largest rank / total | per-rank peak min / median / max |
|---|---|---|---|---|---|---|
| port | 8 | 0.501 | 3.55 | 3.28 | 0.451 / 3.12 | 0.399 / 0.444 / 0.501 |
| C++ | 8 | 0.657 | 4.13 | 3.78 | — | 0.459 / 0.494 / 0.657 |
| port 4 x 2 | 4 | 0.792 | 3.05 | 2.94 | 0.655 / 2.52 | 0.711 / 0.775 / 0.792 |

Table "MUMPS" (the port's records; `INFOG` in MB, flops = `RINFOG(3)`):

| ranks | ordering `INFOG(7)` | factor entries `INFOG(29)` | allocated sum / largest rank `INFOG(19)` / `(18)` | used sum / largest `INFOG(22)` / `(21)` | elimination flops | retries | matrix entries sum / largest rank |
|---|---|---|---|---|---|---|---|
| 8 (port) | 4 | 9.16e+07 | 2,030 / 304 | 1,528 / 245 | 2.27e+11 | 0 | 2,260,803 / 284,016 |
| 4 (port 4 x 2) | 4 | 9.17e+07 | 1,526 / 417 | 1,138 / 307 | 2.26e+11 | 0 | 2,225,178 / 558,030 |

Progress log: first line 2026-11-17T07:18:36Z, last line 2026-11-17T07:20:23Z (11 lines).
Cohesive runs: 0.03 h of run time in `index.jsonl` (all attempts).
