# Final campaign, part G: the full-size run

- Machine: AMD EPYC 9R14, 128 cores, kernel 7.0.0-1014-aws.
- Sessions (UTC): 2026-11-17T22:35:02Z, 2026-11-18T03:18:14Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `740a79a369d8`.
- Size `full`, repetitions requested: 1. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 13 planned, 13 complete and checked (only these are tabulated).

Sessions (2; cells marked ‡ combine repetitions from more than one):

| session | CPU | boot id | kernel |
|---|---|---|---|
| 20261117T223502Z | AMD EPYC 9R14 | 543c0c3f | 7.0.0-1014-aws |
| 20261118T031814Z | AMD EPYC 9R14 | 543c0c3f | 7.0.0-1014-aws |

Checks (`checks.jsonl`): 10 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 0 comparisons made, 1 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 9 runs, 9 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

The plan printed before the runs (`plan.txt`):

```text
Part G plan: size full, 1 repetition(s), 128 cores
series        groups   runs   expected
full              13     13     5.20 h
total             13     13     5.20 h

Runs one at a time, so the machine time is their sum in any order; the order removes waste:
the partitions of every rank count first (each run replays its file); the port at the largest
rank count first (the shortest full-size run, it measures that rank count's memory before the
C++ run); the longest and least needed run (8 ranks) last, so that a short budget drops it.
Placement fill: rank r with T threads on the r-th block of T cores of the host's cores
ordered NUMA node by NUMA node (one node filled before the next).
step                     cores              expected peak GiB  expected wall  cumulative
export np128             C++, partition only                            2 min       0.03h
export np64              C++, partition only                            2 min       0.07h
export np32              C++, partition only                            2 min       0.10h
export np16              C++, partition only                            2 min       0.13h
128:head                 128 (128 x 1)                  250.0         21 min       0.49h
128:cpp                  128 (128 x 1)                  235.0         38 min       1.11h
64:head                  64 (64 x 1)                    206.0         26 min       1.54h
64:cpp                   64 (64 x 1)                    192.0         45 min       2.30h
32:head                  32 (32 x 1)                    186.0         33 min       2.84h
32:cpp                   32 (32 x 1)                    194.0         1.05 h       3.89h
64x2:head                128 (64 x 2)                   206.0         22 min       4.25h
32x4:head                128 (32 x 4)                   186.0         22 min       4.62h
16x8:head                128 (16 x 8)                   175.0         35 min       5.20h
Estimates and their basis: scripts/perf/final/plan.py (EST_*), g-estimates.txt.
```

Table "Strong scaling" (one row per configuration, ranks x threads; placement: `fill` puts ranks on one NUMA node before the next, so on a two-node 128-core host the 8-, 32- and 64-rank runs use one node (one socket's memory bandwidth) and the 128-rank and 128-core hybrid runs both, which is part of the 64 → 128 efficiency; `spread` puts the same number of ranks on every node; the NUMA nodes column is read from each run's recorded binding; rank 0's Chrono phases, s, per step in ms; core-µs per element and step = (`solve_step` + check) × cores / tetrahedra / steps; loop efficiency = core-µs at the smallest core count of the same code (port: 32 cores) (C++: 32 cores) / core-µs here):

| code | ranks x threads | cores | placement, NUMA nodes | wall s | read_mesh | dist_mesh | init_full | static solve | `solve_step` ms/step | check ms/step | energies ms/step | loop s | core-µs/el/step | loop efficiency |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 32 x 1 | 32 | fill, 0 | 1,806 | 2.52 | 11.4 | 1.76 | 448 | 115 | 142 | 10.2 | 1,285 | 1.87 | 100% |
| port | 64 x 1 | 64 | fill, 0 | 1,204 | 2.43 | 11.5 | 0.881 | 337 | 76.9 | 86.0 | 6.32 | 815 | 2.37 | 79% |
| port | 128 x 1 | 128 | fill, 0,1 | 1,174 | 2.46 | 12.1 | 0.544 | 319 | 74.9 | 83.8 | 8.41 | 793 | 4.62 | 40% |
| port | 64 x 2 | 128 | fill, 0,1 | 849 | 2.49 | 11.9 | 0.680 | 262 | 50.4 | 58.1 | 5.12 | 543 | 3.16 | 59% |
| port | 32 x 4 | 128 | fill, 0,1 | 896 | 2.47 | 11.2 | 1.29 | 276 | 54.2 | 60.8 | 5.24 | 575 | 3.35 | 56% |
| port | 16 x 8 | 128 | fill, 0,1 | 1,532 | 2.50 | 11.1 | 2.78 | 641 | 91.0 | 74.8 | 8.09 | 829 | 4.83 | 39% |
| C++ | 32 x 1 | 32 | fill, 0 | 4,297 | 15.0 | 66.3 | 16.7 | 381 | 199 | 533 | 30.0 | 3,659 | 5.33 | 100% |
| C++ | 64 x 1 | 64 | fill, 0 | 2,848 | 18.0 | 85.7 | 7.55 | 299 | 134 | 326 | 26.0 | 2,300 | 6.70 | 80% |
| C++ | 128 x 1 | 128 | fill, 0,1 | 2,298 | 15.7 | 86.5 | 4.16 | 239 | 87.7 | 246 | 55.5 | 1,671 | 9.74 | 55% |

Table "Phases" (rank 0's Chrono totals, s; per step = total / steps):

| code | ranks | wall s | read_mesh | dist_mesh | init_full | static solve | before_step | `solve_step` s (ms/step) | check s (ms/step) | energies s (ms/step) | loop s | core-µs/el/step |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 128 | 1,174 | 2.46 | 12.1 | 0.544 | 319 | 0.468 | 374 (74.9) | 419 (83.8) | 42.1 (8.41) | 793 | 4.62 |
| C++ | 128 | 2,298 | 15.7 | 86.5 | 4.16 | 239 | 1.27 | 439 (87.7) | 1,232 (246) | 277 (55.5) | 1,671 | 9.74 |
| port | 64 | 1,204 | 2.43 | 11.5 | 0.881 | 337 | 0.890 | 385 (76.9) | 430 (86.0) | 31.6 (6.32) | 815 | 2.37 |
| port 64 x 2 | 64 | 849 | 2.49 | 11.9 | 0.680 | 262 | 0.606 | 252 (50.4) | 291 (58.1) | 25.6 (5.12) | 543 | 3.16 |
| C++ | 64 | 2,848 | 18.0 | 85.7 | 7.55 | 299 | 2.33 | 669 (134) | 1,631 (326) | 130 (26.0) | 2,300 | 6.70 |
| port | 32 | 1,806 | 2.52 | 11.4 | 1.76 | 448 | 1.56 | 574 (115) | 711 (142) | 51.2 (10.2) | 1,285 | 1.87 |
| port 32 x 4 | 32 | 896 | 2.47 | 11.2 | 1.29 | 276 | 1.15 | 271 (54.2) | 304 (60.8) | 26.2 (5.24) | 575 | 3.35 |
| C++ | 32 | 4,297 | 15.0 | 66.3 | 16.7 | 381 | 4.12 | 995 (199) | 2,664 (533) | 150 (30.0) | 3,659 | 5.33 |
| port 16 x 8 | 16 | 1,532 | 2.50 | 11.1 | 2.78 | 641 | 2.48 | 455 (91.0) | 374 (74.8) | 40.4 (8.09) | 829 | 4.83 |

Table "Port / upstream at the same rank count":

| ranks | wall | `init_full` | static solve | `dist_mesh` | `solve_step` | check | energies | loop | largest rank peak | total peak |
|---|---|---|---|---|---|---|---|---|---|---|
| 128 | 0.511 | 0.131 | 1.34 | 0.140 | 0.853 | 0.340 | 0.152 | 0.475 | 0.988 | 1.01 |
| 64 | 0.423 | 0.117 | 1.13 | 0.134 | 0.575 | 0.264 | 0.243 | 0.354 | 1.03 | 0.929 |
| 32 | 0.420 | 0.106 | 1.18 | 0.172 | 0.577 | 0.267 | 0.341 | 0.351 | 1.24 | 0.993 |

Table "Memory" (GiB; sampler = `memsample.py`, VmHWM of every rank; container = the cgroup's `memory.peak`; static = the port's `VmHWM` at the end of the static solve):

| code | ranks | largest rank | total (sampler) | container | static: largest rank / total | per-rank peak min / median / max |
|---|---|---|---|---|---|---|
| port | 128 | 4.53 | 321 | 206 | 4.53 / 319 | 2.05 / 2.41 / 4.53 |
| C++ | 128 | 4.59 | 318 | 217 | — | 2.10 / 2.44 / 4.59 |
| port | 64 | 5.89 | 218 | 196 | 5.88 / 216 | 2.57 / 3.21 / 5.89 |
| port 64 x 2 | 64 | 6.44 | 197 | 176 | 6.38 / 192 | 2.33 / 2.91 / 6.44 |
| C++ | 64 | 5.70 | 235 | 213 | — | 2.74 / 3.48 / 5.70 |
| port | 32 | 10.9 | 187 | 183 | 10.8 / 183 | 4.39 / 5.58 / 10.9 |
| port 32 x 4 | 32 | 8.05 | 178 | 175 | 7.91 / 171 | 4.01 / 5.41 / 8.05 |
| C++ | 32 | 8.77 | 188 | 184 | — | 4.36 / 5.36 / 8.77 |
| port 16 x 8 | 16 | 17.6 | 171 | 170 | 17.2 / 164 | 7.37 / 10.3 / 17.6 |

Table "MUMPS" (the port's records; `INFOG` in MB, flops = `RINFOG(3)`):

| ranks | ordering `INFOG(7)` | factor entries `INFOG(29)` | allocated sum / largest rank `INFOG(19)` / `(18)` | used sum / largest `INFOG(22)` / `(21)` | elimination flops | retries | matrix entries sum / largest rank |
|---|---|---|---|---|---|---|---|
| 128 (port) | 4 | 8.14e+09 | 199,175 / 3,766 | 122,002 / 2,139 | 1.67e+14 | 0 | 54,624,147 / 435,060 |
| 64 (port) | 4 | 7.86e+09 | 182,002 / 5,639 | 107,667 / 3,988 | 1.52e+14 | 0 | 53,725,776 / 846,258 |
| 64 (port 64 x 2) | 4 | 7.55e+09 | 158,306 / 6,017 | 101,194 / 5,298 | 1.33e+14 | 0 | 53,725,776 / 846,258 |
| 32 (port) | 4 | 7.97e+09 | 174,331 / 11,183 | 106,523 / 7,547 | 1.58e+14 | 0 | 53,089,626 / 1,677,093 |
| 32 (port 32 x 4) | 4 | 7.51e+09 | 155,575 / 7,569 | 97,472 / 5,150 | 1.31e+14 | 0 | 53,089,626 / 1,677,093 |
| 16 (port 16 x 8) | 4 | 8.12e+09 | 152,678 / 16,879 | 101,682 / 13,533 | 1.66e+14 | 0 | 52,508,430 / 3,297,309 |

Progress log: first line 2026-11-17T22:35:12Z, last line 2026-11-18T03:38:26Z (17 lines).
Cohesive runs: 5.03 h of run time in `index.jsonl` (all attempts).
