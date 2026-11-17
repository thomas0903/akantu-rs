# Final campaign, part D: static solves and solvers

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T07:08:20Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 18 planned, 18 complete and checked (only these are tabulated).

The head's SparseLU factorisation uses threads: false (measured by prepare.sh: the `factorize` phase of `static_linear_eigen` at 4 threads under 0.9 of 1 thread, `sparselu_probe` in the head's VARIANT.json; threaded variants are timed only then).

Checks (`checks.jsonl`): 8 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 4 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 24 runs, 24 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "The SparseLU translation against upstream's SparseLU" (the driver's `static_solve` phase: assembly, analysis, factorisation, solve; 1 rank, 10 steps; s):

| h | DOFs | C++ SparseLU (strict) | port before | port | port / C++ | port / port before | container memory peak (cgroup memory.peak) C++ / before / port MiB |
|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.96 [2.96, 2.97] | 9.25 [9.21, 9.29] | 2.17 [2.16, 2.17] | 0.731 [0.729, 0.733] | 0.234 [0.234, 0.235] | 496 [496, 496] / 561 [560, 561] / 396 [396, 396] |

Table "MUMPS in the cohesive benchmark" (static solve phase, s; the same MUMPS 5.7.3 on the same matrix, PORD):

| h | ranks | C++ | port before | port | port / C++ | port / port before |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.662 [0.661, 0.663] | 0.923 [0.921, 0.924] | 0.558 [0.558, 0.558] | 0.843 [0.841, 0.844] | 0.605 [0.604, 0.605] |
| 0.12 | 2 | 0.455 [0.455, 0.455] | 0.582 [0.579, 0.585] | 0.413 [0.412, 0.414] | 0.909 [0.907, 0.911] | 0.710 [0.705, 0.716] |
| 0.12 | 4 | 0.266 [0.257, 0.274] | 0.317 [0.315, 0.320] | 0.234 [0.232, 0.235] | 0.881 [0.848, 0.914] | 0.736 [0.735, 0.737] |

Table "suite_solid, the sparse solvers' phases (analysis, factorize, back_substitution)" (seconds of the phase over its calls, median [min, max]; n = repetitions in the medians; ratios paired per repetition; the threads and fast-mode variants run only for the cases whose timed phases run such code, plan.py `threaded_case`, — elsewhere):

| case | phase | calls | n | before | head | head / before | digests |
|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 1 | 2 | 262 µs [254 µs, 270 µs] | 287 µs [282 µs, 292 µs] | 1.09 [1.08, 1.11] | equal |
| static_linear_eigen | back_substitution | 2 | 2 | 40.3 µs [34.1 µs, 46.6 µs] | 24.8 µs [23.5 µs, 26.1 µs] | 0.634 [0.504, 0.764] | equal |
| static_linear_eigen | corrector | 1 | 2 | 0.140 µs [0.140 µs, 0.140 µs] | 0.120 µs [0.110 µs, 0.130 µs] | 0.857 [0.786, 0.929] | equal |
| static_linear_eigen | factorize | 1 | 2 | 955 µs [948 µs, 962 µs] | 558 µs [555 µs, 561 µs] | 0.584 [0.583, 0.585] | equal |
| static_linear_eigen | init | 1 | 2 | 557 µs [535 µs, 579 µs] | 504 µs [501 µs, 506 µs] | 0.905 [0.874, 0.936] | equal |
| static_linear_eigen | matrix | 1 | 2 | 1.65 ms [1.64 ms, 1.66 ms] | 817 µs [815 µs, 819 µs] | 0.495 [0.494, 0.496] | equal |
| static_linear_eigen | mesh | 1 | 2 | 54.3 µs [53.9 µs, 54.7 µs] | 47.2 µs [46.7 µs, 47.7 µs] | 0.870 [0.854, 0.886] | equal |
| static_linear_eigen | predictor | 1 | 2 | 0.0955 µs [0.0910 µs, 0.100 µs] | 0.0800 µs [0.0800 µs, 0.0800 µs] | 0.840 [0.800, 0.879] | equal |
| static_linear_eigen | solve_step | 1 | 2 | 3.68 ms [3.66 ms, 3.70 ms] | 2.10 ms [2.10 ms, 2.10 ms] | 0.570 [0.568, 0.573] | equal |
| static_linear_eigen | step_hooks | 2 | 2 | 2.58 µs [2.56 µs, 2.61 µs] | 2.22 µs [2.20 µs, 2.24 µs] | 0.859 [0.843, 0.875] | equal |
| static_linear_eigen | step_rest | 1 | 2 | 2.02 ms [2.01 ms, 2.04 ms] | 1.28 ms [1.28 ms, 1.28 ms] | 0.632 [0.628, 0.635] | equal |
| static_linear_eigen | stiffness_assembly | 1 | 2 | 705 µs [702 µs, 708 µs] | 138 µs [138 µs, 138 µs] | 0.196 [0.196, 0.196] | equal |
| static_linear_faer | analysis | 1 | 2 | 300 µs [299 µs, 302 µs] | 294 µs [287 µs, 301 µs] | 0.979 [0.951, 1.01] | equal |
| static_linear_faer | back_substitution | 2 | 2 | 27.2 µs [25.8 µs, 28.6 µs] | 29.4 µs [29.3 µs, 29.5 µs] | 1.08 [1.03, 1.14] | equal |
| static_linear_faer | corrector | 1 | 2 | 0.135 µs [0.130 µs, 0.140 µs] | 0.115 µs [0.110 µs, 0.120 µs] | 0.852 [0.846, 0.857] | equal |
| static_linear_faer | factorize | 1 | 2 | 1.17 ms [1.14 ms, 1.20 ms] | 1.25 ms [1.25 ms, 1.25 ms] | 1.07 [1.04, 1.10] | equal |
| static_linear_faer | init | 1 | 2 | 516 µs [509 µs, 524 µs] | 484 µs [474 µs, 494 µs] | 0.939 [0.905, 0.972] | equal |
| static_linear_faer | matrix | 1 | 2 | 1.67 ms [1.66 ms, 1.67 ms] | 811 µs [807 µs, 815 µs] | 0.487 [0.485, 0.488] | equal |
| static_linear_faer | mesh | 1 | 2 | 56.0 µs [55.6 µs, 56.4 µs] | 44.9 µs [43.9 µs, 45.8 µs] | 0.801 [0.779, 0.822] | equal |
| static_linear_faer | predictor | 1 | 2 | 0.0800 µs [0.0800 µs, 0.0800 µs] | 0.0800 µs [0.0800 µs, 0.0800 µs] | 1.00 [1.00, 1.00] | equal |
| static_linear_faer | solve_step | 1 | 2 | 3.91 ms [3.86 ms, 3.96 ms] | 2.60 ms [2.58 ms, 2.61 ms] | 0.663 [0.659, 0.668] | equal |
| static_linear_faer | step_hooks | 2 | 2 | 2.31 µs [2.27 µs, 2.34 µs] | 1.91 µs [1.79 µs, 2.04 µs] | 0.830 [0.789, 0.872] | equal |
| static_linear_faer | step_rest | 1 | 2 | 2.24 ms [2.20 ms, 2.29 ms] | 1.78 ms [1.77 ms, 1.79 ms] | 0.795 [0.784, 0.806] | equal |
| static_linear_faer | stiffness_assembly | 1 | 2 | 708 µs [696 µs, 720 µs] | 137 µs [136 µs, 137 µs] | 0.193 [0.190, 0.196] | equal |

Table "suite_solid static_linear_mumps at 1, 2, 4 ranks" (seconds of the phase, rank 0 or the case's maximum over the ranks as the case reports it; n = repetitions; rank r on the r-th core of the host's cores ordered NUMA node by NUMA node, recorded and checked; "/ head on as many cores" divides the medians of two different groups, P x T threads and P x T ranks, so it is not paired):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_mumps | corrector | 1 | 2 | 0.175 µs [0.170 µs, 0.180 µs] | 0.150 µs [0.140 µs, 0.160 µs] | 0.856 [0.824, 0.889] |  |  |  | equal |
| static_linear_mumps | corrector | 2 | 2 | 0.180 µs [0.180 µs, 0.180 µs] | 0.185 µs [0.180 µs, 0.190 µs] | 1.03 [1.00, 1.06] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | corrector | 4 | 2 | 0.220 µs [0.220 µs, 0.220 µs] | 0.285 µs [0.190 µs, 0.380 µs] | 1.30 [0.864, 1.73] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 2 | 2 | 1.13 ms [1.13 ms, 1.13 ms] | 725 µs [720 µs, 729 µs] | 0.640 [0.636, 0.643] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 4 | 2 | 1.36 ms [1.34 ms, 1.38 ms] | 924 µs [913 µs, 934 µs] | 0.678 [0.661, 0.695] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 1 | 2 | 594 µs [566 µs, 623 µs] | 505 µs [501 µs, 509 µs] | 0.852 [0.817, 0.886] |  |  |  | equal |
| static_linear_mumps | init | 2 | 2 | 620 µs [599 µs, 641 µs] | 543 µs [529 µs, 557 µs] | 0.877 [0.825, 0.929] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 4 | 2 | 761 µs [723 µs, 799 µs] | 663 µs [651 µs, 676 µs] | 0.873 [0.845, 0.900] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 1 | 2 | 1.57 ms [1.57 ms, 1.58 ms] | 762 µs [743 µs, 782 µs] | 0.485 [0.470, 0.500] |  |  |  | equal |
| static_linear_mumps | matrix | 2 | 2 | 796 µs [788 µs, 804 µs] | 415 µs [406 µs, 425 µs] | 0.522 [0.504, 0.540] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 4 | 2 | 459 µs [457 µs, 461 µs] | 292 µs [289 µs, 296 µs] | 0.637 [0.627, 0.647] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 1 | 2 | 58.9 µs [56.1 µs, 61.6 µs] | 59.4 µs [57.5 µs, 61.2 µs] | 1.01 [0.993, 1.03] |  |  |  | equal |
| static_linear_mumps | mesh | 2 | 2 | 55.8 µs [55.5 µs, 56.2 µs] | 56.0 µs [55.2 µs, 56.8 µs] | 1.00 [0.983, 1.02] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 4 | 2 | 66.3 µs [66.0 µs, 66.5 µs] | 64.6 µs [60.7 µs, 68.6 µs] | 0.976 [0.912, 1.04] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 1 | 2 | 0.105 µs [0.0900 µs, 0.120 µs] | 0.0900 µs [0.0900 µs, 0.0900 µs] | 0.875 [0.750, 1.00] |  |  |  | equal |
| static_linear_mumps | predictor | 2 | 2 | 0.0950 µs [0.0900 µs, 0.100 µs] | 0.105 µs [0.100 µs, 0.110 µs] | 1.11 [1.00, 1.22] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 4 | 2 | 0.110 µs [0.100 µs, 0.120 µs] | 0.115 µs [0.100 µs, 0.131 µs] | 1.07 [0.833, 1.31] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 1 | 2 | 3.33 ms [3.33 ms, 3.34 ms] | 2.17 ms [2.15 ms, 2.19 ms] | 0.651 [0.647, 0.655] |  |  |  | equal |
| static_linear_mumps | solve_step | 2 | 2 | 2.94 ms [2.94 ms, 2.95 ms] | 2.36 ms [2.35 ms, 2.38 ms] | 0.803 [0.799, 0.806] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 4 | 2 | 3.14 ms [3.06 ms, 3.23 ms] | 2.74 ms [2.73 ms, 2.76 ms] | 0.873 [0.856, 0.890] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 1 | 2 | 2.67 µs [2.67 µs, 2.67 µs] | 2.27 µs [2.10 µs, 2.43 µs] | 0.848 [0.787, 0.910] |  |  |  | equal |
| static_linear_mumps | step_hooks | 2 | 2 | 4.02 µs [3.97 µs, 4.07 µs] | 2.67 µs [2.50 µs, 2.84 µs] | 0.664 [0.630, 0.698] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 4 | 2 | 6.28 µs [5.85 µs, 6.70 µs] | 3.82 µs [3.63 µs, 4.02 µs] | 0.614 [0.542, 0.687] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 1 | 2 | 1.76 ms [1.76 ms, 1.76 ms] | 1.41 ms [1.37 ms, 1.44 ms] | 0.799 [0.778, 0.821] |  |  |  | equal |
| static_linear_mumps | step_rest | 2 | 2 | 2.16 ms [2.15 ms, 2.16 ms] | 1.96 ms [1.95 ms, 1.97 ms] | 0.908 [0.906, 0.909] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 4 | 2 | 2.74 ms [2.66 ms, 2.82 ms] | 2.49 ms [2.45 ms, 2.52 ms] | 0.907 [0.894, 0.921] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 1 | 2 | 597 µs [595 µs, 600 µs] | 139 µs [139 µs, 139 µs] | 0.233 [0.232, 0.233] |  |  |  | equal |
| static_linear_mumps | stiffness_assembly | 2 | 2 | 305 µs [304 µs, 306 µs] | 77.4 µs [77.2 µs, 77.6 µs] | 0.254 [0.252, 0.255] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 4 | 2 | 176 µs [168 µs, 183 µs] | 64.5 µs [62.2 µs, 66.8 µs] | 0.368 [0.364, 0.371] |  |  |  | not compared (does not repeat) |

Progress log: first line 2026-11-17T07:08:23Z, last line 2026-11-17T07:11:34Z (39 lines).
Cohesive runs: 0.04 h of run time in `index.jsonl` (all attempts).
