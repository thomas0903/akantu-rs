# Final campaign, part D: static solves and solvers

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T05:11:45Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

The head's SparseLU factorisation uses threads: true (from the translation's source; threaded variants are timed only then).

Checks (`checks.jsonl`): 8 points; at every point the thread count and precision of each port run (`rust_exec` of `summary.json`) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 4 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 30 runs, 30 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "The SparseLU translation against upstream's SparseLU" (the driver's `static_solve` phase: assembly, analysis, factorisation, solve; 1 rank, 10 steps; s):

| h | DOFs | C++ SparseLU (strict) | port before | port | port / C++ | port / port before | peak RSS C++ / before / port MiB |
|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.95 [2.95, 2.96] | 9.27 [9.27, 9.27] | 2.14 [2.14, 2.15] | 0.726 [0.723, 0.729] | 0.231 [0.231, 0.232] | 496 [496, 496] / 561 [560, 561] / 397 [396, 397] |
| 0.12 | port, 2 threads |  |  | 2.15 [2.14, 2.16] | 0.727 [0.722, 0.731] | 1.00 [0.999, 1.00] (against 1 thread) |  |
| 0.12 | port, 4 threads |  |  | 2.14 [2.13, 2.14] | 0.723 [0.722, 0.724] | 0.996 [0.991, 1.00] (against 1 thread) |  |
| 0.12 | port, 8 threads |  |  | 2.13 [2.12, 2.14] | 0.720 [0.715, 0.725] | 0.992 [0.989, 0.995] (against 1 thread) |  |

Table "MUMPS in the cohesive benchmark" (static solve phase, s; the same MUMPS 5.7.3 on the same matrix, PORD):

| h | ranks | C++ | port before | port | port / C++ | port / port before |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.656 [0.652, 0.660] | 0.918 [0.917, 0.920] | 0.555 [0.555, 0.556] | 0.846 [0.842, 0.850] | 0.605 [0.604, 0.605] |
| 0.12 | 2 | 0.451 [0.451, 0.452] | 0.585 [0.585, 0.585] | 0.410 [0.410, 0.411] | 0.909 [0.907, 0.911] | 0.702 [0.700, 0.703] |
| 0.12 | 4 | 0.252 [0.250, 0.254] | 0.310 [0.309, 0.311] | 0.232 [0.231, 0.233] | 0.923 [0.912, 0.933] | 0.749 [0.743, 0.755] |

Table "suite_solid, the sparse solvers' phases (analysis, factorize, back_substitution)" (seconds of the phase over its calls, median [min, max]; ratios paired per repetition):

| case | phase | calls | before | head | head / before | head 2 threads / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1) | fast / bitwise (8) | digests |
|---|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 1 | 263 µs [262 µs, 263 µs] | 294 µs [292 µs, 297 µs] | 1.12 [1.11, 1.13] | 0.994 [0.983, 1.01] | 1.03 [1.02, 1.03] | 6.18 [1.03, 11.3] | missing | missing | equal |
| static_linear_eigen | back_substitution | 2 | 34.2 µs [34.1 µs, 34.3 µs] | 23.4 µs [23.3 µs, 23.5 µs] | 0.684 [0.679, 0.689] | 1.25 [1.01, 1.48] | 1.08 [0.991, 1.17] | 1.07 [1.01, 1.14] | missing | missing | equal |
| static_linear_eigen | corrector | 1 | 0.140 µs [0.120 µs, 0.160 µs] | 0.124 µs [0.120 µs, 0.129 µs] | 0.903 [0.806, 1.00] | 0.923 [0.917, 0.930] | 0.847 [0.833, 0.860] | 0.926 [0.853, 1.00] | missing | missing | equal |
| static_linear_eigen | factorize | 1 | 978 µs [961 µs, 995 µs] | 561 µs [549 µs, 573 µs] | 0.574 [0.572, 0.576] | 0.983 [0.948, 1.02] | 0.995 [0.987, 1.00] | 0.980 [0.951, 1.01] | missing | missing | equal |
| static_linear_eigen | init | 1 | 525 µs [520 µs, 530 µs] | 492 µs [488 µs, 497 µs] | 0.938 [0.919, 0.956] | 1.17 [1.17, 1.18] | 1.29 [1.26, 1.33] | 1.76 [1.71, 1.81] | missing | missing | equal |
| static_linear_eigen | matrix | 1 | 1.65 ms [1.64 ms, 1.66 ms] | 938 µs [841 µs, 1.04 ms] | 0.569 [0.507, 0.631] | 0.889 [0.792, 0.987] | 0.865 [0.772, 0.957] | 0.860 [0.776, 0.944] | missing | missing | equal |
| static_linear_eigen | mesh | 1 | 57.9 µs [57.4 µs, 58.4 µs] | 54.7 µs [47.5 µs, 61.9 µs] | 0.945 [0.813, 1.08] | 0.874 [0.837, 0.911] | 0.792 [0.683, 0.901] | 0.794 [0.690, 0.898] | missing | missing | equal |
| static_linear_eigen | predictor | 1 | 0.100 µs [0.100 µs, 0.100 µs] | 0.0900 µs [0.0800 µs, 0.100 µs] | 0.900 [0.800, 1.00] | 0.887 [0.875, 0.900] | 0.950 [0.900, 1.00] | 0.850 [0.700, 1.00] | missing | missing | equal |
| static_linear_eigen | solve_step | 1 | 3.72 ms [3.71 ms, 3.72 ms] | 2.23 ms [2.14 ms, 2.32 ms] | 0.600 [0.576, 0.624] | 0.952 [0.905, 0.999] | 0.943 [0.897, 0.988] | 0.941 [0.898, 0.983] | missing | missing | equal |
| static_linear_eigen | step_hooks | 2 | 2.61 µs [2.53 µs, 2.69 µs] | 2.14 µs [2.11 µs, 2.18 µs] | 0.822 [0.810, 0.834] | 0.711 [0.688, 0.735] | 0.721 [0.716, 0.725] | 0.716 [0.702, 0.730] | missing | missing | equal |
| static_linear_eigen | step_rest | 1 | 2.07 ms [2.05 ms, 2.08 ms] | 1.29 ms [1.29 ms, 1.30 ms] | 0.625 [0.617, 0.632] | 1.00 [0.996, 1.01] | 1.00 [0.997, 1.01] | 1.00 [0.997, 1.01] | missing | missing | equal |
| static_linear_eigen | stiffness_assembly | 1 | 703 µs [696 µs, 709 µs] | 138 µs [137 µs, 138 µs] | 0.196 [0.193, 0.199] | 0.940 [0.936, 0.944] | 0.967 [0.948, 0.985] | 0.980 [0.947, 1.01] | missing | missing | equal |
| static_linear_faer | analysis | 1 | 307 µs [293 µs, 320 µs] | 287 µs [282 µs, 292 µs] | 0.940 [0.881, 0.998] | 1.10 [1.06, 1.14] | 1.07 [1.05, 1.09] | 1.07 [1.07, 1.08] | missing | missing | equal |
| static_linear_faer | back_substitution | 2 | 26.4 µs [24.6 µs, 28.2 µs] | 28.9 µs [28.6 µs, 29.1 µs] | 1.10 [1.02, 1.19] | 0.997 [0.995, 0.999] | 1.01 [1.00, 1.01] | 1.01 [0.999, 1.01] | missing | missing | equal |
| static_linear_faer | corrector | 1 | 0.150 µs [0.150 µs, 0.150 µs] | 0.130 µs [0.130 µs, 0.130 µs] | 0.867 [0.867, 0.867] | 0.846 [0.846, 0.846] | 0.923 [0.923, 0.923] | 0.996 [0.992, 1.00] | missing | missing | equal |
| static_linear_faer | factorize | 1 | 1.14 ms [1.13 ms, 1.14 ms] | 1.24 ms [1.24 ms, 1.24 ms] | 1.09 [1.09, 1.10] | 0.955 [0.953, 0.956] | 0.950 [0.950, 0.950] | 0.966 [0.953, 0.979] | missing | missing | equal |
| static_linear_faer | init | 1 | 529 µs [525 µs, 532 µs] | 472 µs [472 µs, 472 µs] | 0.893 [0.888, 0.898] | 1.24 [1.24, 1.25] | 1.32 [1.32, 1.33] | 1.76 [1.75, 1.77] | missing | missing | equal |
| static_linear_faer | matrix | 1 | 1.68 ms [1.68 ms, 1.69 ms] | 823 µs [823 µs, 823 µs] | 0.489 [0.488, 0.489] | 1.00 [0.991, 1.01] | 0.990 [0.988, 0.992] | 1.00 [0.994, 1.01] | missing | missing | equal |
| static_linear_faer | mesh | 1 | 57.1 µs [55.1 µs, 59.1 µs] | 56.2 µs [48.8 µs, 63.6 µs] | 0.981 [0.886, 1.08] | 0.829 [0.700, 0.957] | 0.819 [0.710, 0.927] | 0.789 [0.672, 0.907] | missing | missing | equal |
| static_linear_faer | predictor | 1 | 0.105 µs [0.0900 µs, 0.120 µs] | 0.0900 µs [0.0900 µs, 0.0900 µs] | 0.875 [0.750, 1.00] | 0.889 [0.889, 0.889] | 1.00 [0.889, 1.11] | 0.889 [0.889, 0.889] | missing | missing | equal |
| static_linear_faer | solve_step | 1 | 3.91 ms [3.91 ms, 3.91 ms] | 2.62 ms [2.61 ms, 2.63 ms] | 0.670 [0.668, 0.673] | 1.00 [0.994, 1.01] | 0.990 [0.986, 0.994] | 1.02 [1.01, 1.03] | missing | missing | equal |
| static_linear_faer | step_hooks | 2 | 2.48 µs [2.41 µs, 2.55 µs] | 2.06 µs [1.96 µs, 2.16 µs] | 0.832 [0.769, 0.896] | 0.776 [0.750, 0.801] | 0.760 [0.718, 0.801] | 0.777 [0.718, 0.837] | missing | missing | equal |
| static_linear_faer | step_rest | 1 | 2.22 ms [2.22 ms, 2.23 ms] | 1.80 ms [1.79 ms, 1.81 ms] | 0.808 [0.805, 0.811] | 1.01 [0.996, 1.01] | 0.990 [0.984, 0.997] | 1.03 [1.02, 1.04] | missing | missing | equal |
| static_linear_faer | stiffness_assembly | 1 | 858 µs [713 µs, 1.00 ms] | 139 µs [137 µs, 141 µs] | 0.167 [0.137, 0.197] | 0.940 [0.927, 0.953] | 0.984 [0.954, 1.01] | 0.952 [0.949, 0.956] | missing | missing | equal |

Table "suite_solid static_linear_mumps at 1, 2, 4 ranks" (seconds of the phase, rank 0 or the case's maximum over the ranks as the case reports it):

| case | phase | ranks | before | head | head / before | head ranks x threads | time | / head at the same cores | digests |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_mumps | corrector | 1 | 0.180 µs [0.180 µs, 0.180 µs] | 0.145 µs [0.140 µs, 0.150 µs] | 0.806 [0.778, 0.833] |  |  |  | equal |
| static_linear_mumps | corrector | 2 | 0.155 µs [0.150 µs, 0.160 µs] | 0.175 µs [0.170 µs, 0.180 µs] | 1.13 [1.06, 1.20] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | corrector | 4 | 0.315 µs [0.310 µs, 0.320 µs] | 0.360 µs [0.290 µs, 0.430 µs] | 1.14 [0.935, 1.34] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 2 | 1.12 ms [1.12 ms, 1.12 ms] | 739 µs [739 µs, 740 µs] | 0.661 [0.658, 0.663] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 4 | 1.36 ms [1.36 ms, 1.36 ms] | 927 µs [924 µs, 930 µs] | 0.682 [0.681, 0.682] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 1 | 559 µs [558 µs, 560 µs] | 548 µs [528 µs, 568 µs] | 0.980 [0.947, 1.01] |  |  |  | equal |
| static_linear_mumps | init | 2 | 575 µs [570 µs, 581 µs] | 573 µs [559 µs, 588 µs] | 0.996 [0.981, 1.01] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 4 | 773 µs [772 µs, 775 µs] | 695 µs [659 µs, 731 µs] | 0.899 [0.854, 0.944] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 1 | 1.57 ms [1.55 ms, 1.59 ms] | 753 µs [749 µs, 756 µs] | 0.480 [0.476, 0.484] |  |  |  | equal |
| static_linear_mumps | matrix | 2 | 806 µs [800 µs, 812 µs] | 396 µs [395 µs, 398 µs] | 0.492 [0.486, 0.498] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 4 | 487 µs [477 µs, 496 µs] | 296 µs [288 µs, 305 µs] | 0.610 [0.581, 0.639] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 1 | 64.7 µs [57.9 µs, 71.4 µs] | 57.7 µs [57.6 µs, 57.8 µs] | 0.902 [0.809, 0.996] |  |  |  | equal |
| static_linear_mumps | mesh | 2 | 53.8 µs [52.2 µs, 55.5 µs] | 59.6 µs [55.0 µs, 64.2 µs] | 1.11 [0.992, 1.23] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 4 | 60.6 µs [59.7 µs, 61.5 µs] | 64.8 µs [63.0 µs, 66.6 µs] | 1.07 [1.02, 1.12] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 1 | 0.0950 µs [0.0900 µs, 0.100 µs] | 0.0950 µs [0.0900 µs, 0.100 µs] | 1.00 [1.00, 1.00] |  |  |  | equal |
| static_linear_mumps | predictor | 2 | 0.105 µs [0.100 µs, 0.111 µs] | 0.154 µs [0.109 µs, 0.200 µs] | 1.45 [1.09, 1.80] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 4 | 0.110 µs [0.100 µs, 0.120 µs] | 0.105 µs [0.100 µs, 0.110 µs] | 0.967 [0.833, 1.10] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 1 | 3.34 ms [3.33 ms, 3.34 ms] | 2.13 ms [2.11 ms, 2.15 ms] | 0.639 [0.633, 0.644] |  |  |  | equal |
| static_linear_mumps | solve_step | 2 | 2.99 ms [2.99 ms, 2.99 ms] | 2.36 ms [2.34 ms, 2.39 ms] | 0.790 [0.783, 0.797] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 4 | 3.14 ms [3.12 ms, 3.16 ms] | 2.79 ms [2.77 ms, 2.81 ms] | 0.889 [0.888, 0.889] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 1 | 2.70 µs [2.59 µs, 2.81 µs] | 2.23 µs [2.19 µs, 2.26 µs] | 0.826 [0.779, 0.873] |  |  |  | equal |
| static_linear_mumps | step_hooks | 2 | 3.52 µs [3.49 µs, 3.56 µs] | 2.73 µs [2.69 µs, 2.78 µs] | 0.776 [0.755, 0.797] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 4 | 5.83 µs [5.64 µs, 6.01 µs] | 4.74 µs [3.98 µs, 5.51 µs] | 0.820 [0.662, 0.977] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 1 | 1.76 ms [1.75 ms, 1.78 ms] | 1.38 ms [1.36 ms, 1.40 ms] | 0.780 [0.776, 0.783] |  |  |  | equal |
| static_linear_mumps | step_rest | 2 | 2.20 ms [2.19 ms, 2.20 ms] | 1.97 ms [1.95 ms, 1.99 ms] | 0.896 [0.886, 0.906] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 4 | 2.72 ms [2.70 ms, 2.73 ms] | 2.55 ms [2.52 ms, 2.58 ms] | 0.938 [0.932, 0.943] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 1 | 583 µs [577 µs, 589 µs] | 148 µs [139 µs, 157 µs] | 0.254 [0.240, 0.267] |  |  |  | equal |
| static_linear_mumps | stiffness_assembly | 2 | 300 µs [298 µs, 302 µs] | 78.1 µs [76.4 µs, 79.9 µs] | 0.260 [0.256, 0.264] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 4 | 167 µs [163 µs, 170 µs] | 47.8 µs [46.6 µs, 49.0 µs] | 0.287 [0.285, 0.288] |  |  |  | not compared (does not repeat) |

Progress log: first line 2026-11-17T05:11:46Z, last line 2026-11-17T05:14:59Z (46 lines).
Cohesive runs: 0.05 h of run time in `index.jsonl` (all attempts).
