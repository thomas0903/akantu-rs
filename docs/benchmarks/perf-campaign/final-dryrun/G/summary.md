# Final campaign, part G: the full-size run

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T05:21:08Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Checks (`checks.jsonl`): 6 points; at every point the thread count and precision of each port run (`rust_exec` of `summary.json`) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 0 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 6 runs, 6 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

The plan printed before the runs (`plan.txt`):

```text
Part G plan: mesh 0.06, 2 run(s) per point, 8 cores, memory limit 29g per run
Runs one at a time, so the machine time is the sum of the runs whatever their order;
the order removes waste: the partition exports of every rank count first (each run then
replays its file); the port at the largest rank count first (the shortest full-size run,
and it measures the memory of that rank count before the C++ run, whose peak was within
8 % of the port's at 32 and 64 ranks); the longest and least needed run (8 ranks) last,
so a short budget drops it, not a comparison.

Pinning: rank r of a run with T threads per rank on the r-th block of T entries of the
host's cores ordered NUMA node by NUMA node (this host: 0,1,2,3,4,5,6,7);
each run's actual rank -> cores is recorded from /proc and checked (runs/*.bindings.txt).

step                   cores used               expected peak GiB  expected wall  cumulative
export np8             C++, partition only                    1.0          0 min       0.01h
export np4             C++, partition only                    1.0          0 min       0.02h
8:head                 8 (8 ranks x 1)                        3.2          0 min       0.02h
8:cpp                  8 (8 ranks x 1)                        4.0          0 min       0.03h
4x2:head               8 (4 ranks x 2)                        3.0          0 min       0.03h
8:head                 8 (8 ranks x 1)                        3.2          0 min       0.03h
8:cpp                  8 (8 ranks x 1)                        4.0          0 min       0.04h
4x2:head               8 (4 ranks x 2)                        3.0          0 min       0.04h

Expected machine time of part G: 0.0 h (plus the set-up of the machine and prepare.sh);
estimates and their basis: scripts/perf/final/g-estimates.txt. A '?' has no estimate.
```

Table "Strong scaling" (one row per configuration, ranks x threads; rank 0's Chrono phases, s, per step in ms; core-µs per element and step = (`solve_step` + check) × cores / tetrahedra / steps; loop efficiency = core-µs at the smallest core count of the same code (port: 8 cores) (C++: 8 cores) / core-µs here):

| code | ranks x threads | cores | wall s | read_mesh | dist_mesh | init_full | static solve | `solve_step` ms/step | check ms/step | energies ms/step | loop s | core-µs/el/step | loop efficiency |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 8 x 1 | 8 | 7.00 [6.98, 7.03] | 0.104 [0.103, 0.104] | 0.196 [0.195, 0.196] | 0.220 [0.219, 0.221] | 2.76 [2.75, 2.77] | 5.56 [5.48, 5.64] | 9.90 [9.85, 9.95] | 0.808 [0.796, 0.821] | 3.09 [3.07, 3.12] | 0.697 [0.692, 0.703] | 100% |
| port | 4 x 2 | 8 | 10.3 [10.2, 10.5] | 0.103 [0.103, 0.103] | 0.190 [0.190, 0.190] | 0.402 [0.399, 0.404] | 4.48 [4.40, 4.56] | 6.35 [6.31, 6.38] | 15.0 [14.8, 15.2] | 1.38 [1.38, 1.39] | 4.27 [4.25, 4.30] | 0.964 [0.958, 0.970] | 72% |
| C++ | 8 x 1 | 8 | 19.6 [19.4, 19.9] | 0.294 [0.293, 0.295] | 2.11 [2.10, 2.11] | 1.18 [1.18, 1.19] | 2.90 [2.73, 3.06] | 17.8 [17.7, 17.8] | 39.8 [39.6, 40.0] | 4.09 [3.97, 4.22] | 11.5 [11.5, 11.5] | 2.59 [2.58, 2.61] | 100% |

Table "Phases" (rank 0's Chrono totals, s; per step = total / steps):

| code | ranks | wall s | read_mesh | dist_mesh | init_full | static solve | before_step | `solve_step` s (ms/step) | check s (ms/step) | energies s (ms/step) | loop s | core-µs/el/step |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 8 | 7.00 [6.98, 7.03] | 0.104 [0.103, 0.104] | 0.196 [0.195, 0.196] | 0.220 [0.219, 0.221] | 2.76 [2.75, 2.77] | 0.138 [0.137, 0.139] | 1.11 [1.10, 1.13] (5.56 [5.48, 5.64]) | 1.98 [1.97, 1.99] (9.90 [9.85, 9.95]) | 0.162 [0.159, 0.164] (0.808 [0.796, 0.821]) | 3.09 [3.07, 3.12] | 0.697 [0.692, 0.703] |
| C++ | 8 | 19.6 [19.4, 19.9] | 0.294 [0.293, 0.295] | 2.11 [2.10, 2.11] | 1.18 [1.18, 1.19] | 2.90 [2.73, 3.06] | 0.410 [0.410, 0.411] | 3.55 [3.55, 3.55] (17.8 [17.7, 17.8]) | 7.95 [7.91, 8.00] (39.8 [39.6, 40.0]) | 0.818 [0.793, 0.843] (4.09 [3.97, 4.22]) | 11.5 [11.5, 11.5] | 2.59 [2.58, 2.61] |
| port 4 x 2 | 4 | 10.3 [10.2, 10.5] | 0.103 [0.103, 0.103] | 0.190 [0.190, 0.190] | 0.402 [0.399, 0.404] | 4.48 [4.40, 4.56] | 0.272 [0.267, 0.278] | 1.27 [1.26, 1.28] (6.35 [6.31, 6.38]) | 3.00 [2.97, 3.04] (15.0 [14.8, 15.2]) | 0.277 [0.276, 0.277] (1.38 [1.38, 1.39]) | 4.27 [4.25, 4.30] | 0.964 [0.958, 0.970] |

Table "Port / upstream at the same rank count":

| ranks | wall | `init_full` | static solve | `dist_mesh` | `solve_step` | check | energies | loop | largest rank peak | total peak |
|---|---|---|---|---|---|---|---|---|---|---|
| 8 | 0.357 [0.351, 0.362] | 0.186 [0.184, 0.187] | 0.954 [0.903, 1.01] | 0.0929 [0.0927, 0.0932] | 0.313 [0.308, 0.318] | 0.249 [0.246, 0.252] | 0.198 [0.189, 0.207] | 0.269 [0.265, 0.272] | 0.894 [0.859, 0.929] | 0.801 [0.792, 0.810] |

Table "Memory" (GiB; sampler = `memsample.py`, VmHWM of every rank; container = the cgroup's `memory.peak`; static = the port's `VmHWM` at the end of the static solve):

| code | ranks | largest rank | total (sampler) | container | static: largest rank / total | per-rank peak min / median / max |
|---|---|---|---|---|---|---|
| port | 8 | 0.529 | 3.48 | 3.21 | 0.482 / 3.06 | 0.380 / 0.415 / 0.529 |
| C++ | 8 | 0.569 | 4.29 | 3.95 | — | 0.497 / 0.529 / 0.569 |
| port 4 x 2 | 4 | 0.804 | 3.08 | 2.97 | 0.669 / 2.56 | 0.741 / 0.769 / 0.804 |

Table "MUMPS" (the port's records; `INFOG` in MB, flops = `RINFOG(3)`):

| ranks | ordering `INFOG(7)` | factor entries `INFOG(29)` | allocated sum / largest rank `INFOG(19)` / `(18)` | used sum / largest `INFOG(22)` / `(21)` | elimination flops | retries | matrix entries sum / largest rank |
|---|---|---|---|---|---|---|---|
| 8 (port) | 4 | 9.1e+07 | 1,955 / 391 | 1,598 / 297 | 2.18e+11 | 0 | 2,260,803 / 284,016 |
| 4 (port 4 x 2) | 4 | 9.54e+07 | 1,520 / 430 | 1,211 / 315 | 2.48e+11 | 0 | 2,225,178 / 558,030 |

Progress log: first line 2026-11-17T05:21:09Z, last line 2026-11-17T05:22:42Z (9 lines).
Cohesive runs: 0.02 h of run time in `index.jsonl` (all attempts).
