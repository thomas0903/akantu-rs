# Final campaign, part D: static solves and solvers

- Machine: AMD EPYC 9R14, 8 cores, kernel 7.0.0-1014-aws.
- Sessions (UTC): 2026-11-17T17:19:43Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `4d974854d00f`.
- Size `full`, repetitions requested: 5. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 65 planned, 65 complete and checked (only these are tabulated).

The head's SparseLU factorisation uses threads: true (measured by prepare.sh: the `factorize` phase of `static_linear_eigen` at 4 threads under 0.9 of 1 thread, `sparselu_probe` in the head's VARIANT.json; threaded variants are timed only then).

Checks (`checks.jsonl`): 40 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 20 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 170 runs, 170 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "The SparseLU translation against upstream's SparseLU" (the driver's `static_solve` phase: assembly, analysis, factorisation, solve; 1 rank, 10 steps; s):

| h | DOFs | C++ SparseLU (strict) | port before | port | port / C++ | port / port before | container memory peak (cgroup memory.peak) C++ / before / port MiB |
|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.85 [2.85, 2.87] | 7.58 [7.58, 7.61] | 2.12 [2.09, 2.12] | 0.740 [0.729, 0.744] | 0.278 [0.276, 0.279] | 497 [497, 497] / 561 [560, 561] / 399 [399, 399] |
| 0.12 | port, 2 threads |  |  | 1.68 [1.66, 1.71] | 0.589 [0.585, 0.596] | 0.797 [0.786, 0.817] (against 1 thread) |  |
| 0.12 | port, 4 threads |  |  | 1.40 [1.39, 1.41] | 0.490 [0.487, 0.494] | 0.663 [0.659, 0.669] (against 1 thread) |  |
| 0.12 | port, 8 threads |  |  | 1.27 [1.27, 1.29] | 0.446 [0.444, 0.452] | 0.603 [0.602, 0.609] (against 1 thread) |  |
| 0.12 | port, nested dissection |  |  | 0.873 [0.857, 0.879] | 0.305 [0.301, 0.307] | 0.412 [0.407, 0.421] (against COLAMD) |  |
| 0.12 | port, fast precision, auto ordering |  |  | 0.880 [0.876, 0.887] | 0.307 [0.306, 0.312] | 0.418 [0.414, 0.420] (against COLAMD) |  |
| 0.06 | 97,551 | 241 [241, 242] | 781 [780, 783] | 219 [218, 220] | 0.910 [0.904, 0.912] | 0.281 [0.279, 0.281] | 7,608 [7,608, 7,608] / 6,918 [6,918, 6,918] / 5,325 [5,325, 5,325] |
| 0.06 | port, 2 threads |  |  | 138 [137, 140] | 0.572 [0.570, 0.579] | 0.630 [0.626, 0.639] (against 1 thread) |  |
| 0.06 | port, 4 threads |  |  | 86.5 [86.4, 86.6] | 0.359 [0.358, 0.359] | 0.394 [0.394, 0.397] (against 1 thread) |  |
| 0.06 | port, 8 threads |  |  | 62.4 [62.3, 62.9] | 0.259 [0.258, 0.261] | 0.285 [0.284, 0.286] (against 1 thread) |  |
| 0.06 | port, nested dissection |  |  | 40.9 [40.9, 41.0] | 0.170 [0.169, 0.170] | 0.186 [0.186, 0.188] (against COLAMD) |  |
| 0.06 | port, fast precision, auto ordering |  |  | 34.5 [34.4, 34.5] | 0.143 [0.143, 0.143] | 0.157 [0.157, 0.158] (against COLAMD) |  |

Table "MUMPS in the cohesive benchmark" (static solve phase, s; the same MUMPS 5.7.3 on the same matrix, PORD):

| h | ranks | C++ | port before | port | port / C++ | port / port before |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.620 [0.604, 0.625] | 0.795 [0.787, 0.797] | 0.516 [0.515, 0.521] | 0.834 [0.831, 0.856] | 0.650 [0.646, 0.662] |
| 0.12 | 2 | 0.407 [0.404, 0.410] | 0.491 [0.490, 0.498] | 0.370 [0.369, 0.373] | 0.907 [0.901, 0.922] | 0.753 [0.743, 0.755] |
| 0.12 | 4 | 0.227 [0.224, 0.231] | 0.272 [0.268, 0.274] | 0.217 [0.214, 0.217] | 0.946 [0.934, 0.967] | 0.797 [0.791, 0.800] |
| 0.06 | 1 | 12.9 [12.9, 13.0] | 14.1 [14.1, 14.1] | 10.2 [10.2, 10.2] | 0.791 [0.785, 0.793] | 0.727 [0.725, 0.728] |
| 0.06 | 2 | 8.62 [8.58, 8.62] | 9.22 [9.16, 9.26] | 7.42 [7.41, 7.45] | 0.864 [0.860, 0.864] | 0.806 [0.804, 0.810] |
| 0.06 | 4 | 4.64 [4.49, 5.01] | 4.95 [4.82, 5.42] | 4.06 [4.05, 4.49] | 0.877 [0.809, 1.00] | 0.831 [0.747, 0.907] |

Table "suite_solid, the sparse solvers' phases (analysis, factorize, back_substitution)" (seconds of the phase over its calls, median [min, max]; n = repetitions in the medians; ratios paired per repetition; the threads and fast-mode variants run only for the cases whose timed phases run such code, plan.py `threaded_case`, — elsewhere):

| case | phase | calls | n | before | head | head / before | head 2 threads / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1) | fast / bitwise (8) | digests |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 1 | 5 | 42.6 ms [42.5 ms, 43.1 ms] | 47.1 ms [46.8 ms, 47.3 ms] | 1.11 [1.09, 1.11] | 0.966 [0.956, 0.973] | 1.02 [0.996, 1.03] | 0.970 [0.964, 0.979] | — | — | equal |
| static_linear_eigen | back_substitution | 20 | 5 | 209 ms [203 ms, 212 ms] | 135 ms [134 ms, 135 ms] | 0.646 [0.635, 0.666] | 0.989 [0.986, 0.996] | 0.997 [0.994, 1.00] | 1.00 [0.996, 1.01] | — | — | equal |
| static_linear_eigen | corrector | 1 | 5 | 1.10 µs [0.850 µs, 1.24 µs] | 0.890 µs [0.690 µs, 1.00 µs] | 0.802 [0.726, 0.909] | 0.888 [0.760, 1.20] | 0.907 [0.630, 1.28] | 0.966 [0.773, 1.42] | — | — | equal |
| static_linear_eigen | factorize | 2 | 5 | 5.52 s [5.51 s, 5.52 s] | 960 ms [957 ms, 968 ms] | 0.174 [0.174, 0.176] | 0.866 [0.856, 0.875] | 0.787 [0.779, 0.796] | 0.740 [0.736, 0.749] | — | — | equal |
| static_linear_eigen | init | 1 | 5 | 13.1 ms [13.0 ms, 13.3 ms] | 9.14 ms [8.67 ms, 9.36 ms] | 0.689 [0.667, 0.712] | 1.01 [0.977, 1.06] | 1.03 [1.00, 1.07] | 1.03 [0.999, 1.10] | — | — | equal |
| static_linear_eigen | matrix | 1 | 5 | 152 ms [148 ms, 154 ms] | 66.6 ms [65.1 ms, 67.2 ms] | 0.438 [0.434, 0.445] | 0.934 [0.927, 0.984] | 0.910 [0.897, 0.954] | 0.886 [0.870, 0.903] | — | — | equal |
| static_linear_eigen | mesh | 1 | 5 | 784 µs [764 µs, 792 µs] | 780 µs [763 µs, 795 µs] | 0.997 [0.967, 1.04] | 1.000 [0.987, 1.03] | 0.984 [0.978, 1.01] | 0.992 [0.953, 1.02] | — | — | equal |
| static_linear_eigen | predictor | 1 | 5 | 0.210 µs [0.100 µs, 0.520 µs] | 0.210 µs [0.100 µs, 0.230 µs] | 1.00 [0.400, 2.09] | 1.00 [0.435, 2.30] | 1.05 [0.391, 2.10] | 1.00 [0.478, 2.20] | — | — | equal |
| static_linear_eigen | solve_step | 1 | 5 | 3.04 s [3.03 s, 3.04 s] | 682 ms [677 ms, 688 ms] | 0.224 [0.222, 0.227] | 0.893 [0.883, 0.894] | 0.832 [0.825, 0.848] | 0.798 [0.785, 0.804] | — | — | equal |
| static_linear_eigen | step_hooks | 2 | 5 | 122 µs [101 µs, 130 µs] | 5.28 µs [4.48 µs, 6.01 µs] | 0.0405 [0.0372, 0.0524] | 0.875 [0.765, 1.07] | 0.873 [0.641, 1.02] | 0.907 [0.697, 0.982] | — | — | equal |
| static_linear_eigen | step_rest | 1 | 5 | 2.89 s [2.88 s, 2.89 s] | 615 ms [611 ms, 621 ms] | 0.213 [0.211, 0.216] | 0.884 [0.876, 0.891] | 0.823 [0.816, 0.837] | 0.790 [0.776, 0.793] | — | — | equal |
| static_linear_eigen | stiffness_assembly | 3 | 5 | 187 ms [183 ms, 189 ms] | 30.4 ms [30.3 ms, 30.5 ms] | 0.163 [0.161, 0.166] | 0.583 [0.566, 0.588] | 0.332 [0.325, 0.340] | 0.197 [0.197, 0.209] | — | — | equal |
| static_linear_faer | analysis | 1 | 5 | 49.4 ms [49.2 ms, 49.8 ms] | 55.8 ms [55.2 ms, 56.2 ms] | 1.13 [1.12, 1.14] | 0.988 [0.968, 0.997] | 0.997 [0.983, 1.00] | 0.994 [0.983, 1.01] | — | — | equal |
| static_linear_faer | back_substitution | 20 | 5 | 207 ms [206 ms, 211 ms] | 200 ms [199 ms, 201 ms] | 0.968 [0.942, 0.970] | 1.01 [1.00, 1.02] | 1.00 [0.996, 1.02] | 0.997 [0.994, 1.01] | — | — | equal |
| static_linear_faer | corrector | 1 | 5 | 0.880 µs [0.680 µs, 1.01 µs] | 1.10 µs [0.950 µs, 1.27 µs] | 1.26 [1.09, 1.57] | 0.982 [0.677, 1.17] | 0.764 [0.740, 0.853] | 0.792 [0.732, 1.06] | — | — | equal |
| static_linear_faer | factorize | 2 | 5 | 9.35 s [9.30 s, 9.54 s] | 9.51 s [9.42 s, 9.56 s] | 1.01 [0.997, 1.03] | 0.989 [0.987, 0.998] | 0.998 [0.983, 1.03] | 0.984 [0.973, 0.992] | — | — | equal |
| static_linear_faer | init | 1 | 5 | 13.3 ms [13.2 ms, 13.5 ms] | 9.11 ms [9.02 ms, 9.14 ms] | 0.679 [0.676, 0.690] | 1.01 [0.988, 1.02] | 1.03 [1.02, 1.06] | 1.04 [1.03, 1.05] | — | — | equal |
| static_linear_faer | matrix | 1 | 5 | 152 ms [151 ms, 154 ms] | 66.9 ms [66.7 ms, 68.0 ms] | 0.441 [0.436, 0.444] | 0.934 [0.920, 0.963] | 0.900 [0.887, 0.927] | 0.891 [0.870, 0.931] | — | — | equal |
| static_linear_faer | mesh | 1 | 5 | 793 µs [785 µs, 796 µs] | 775 µs [732 µs, 799 µs] | 0.986 [0.926, 1.01] | 1.00 [0.952, 1.07] | 1.01 [0.942, 1.06] | 0.997 [0.959, 1.08] | — | — | equal |
| static_linear_faer | predictor | 1 | 5 | 0.120 µs [0.0900 µs, 0.390 µs] | 0.110 µs [0.100 µs, 0.240 µs] | 0.833 [0.615, 2.20] | 1.00 [0.500, 2.00] | 1.20 [0.375, 2.20] | 1.00 [0.875, 2.20] | — | — | equal |
| static_linear_faer | solve_step | 1 | 5 | 4.94 s [4.93 s, 4.94 s] | 4.93 s [4.79 s, 4.93 s] | 0.997 [0.969, 0.998] | 0.988 [0.978, 1.00] | 0.995 [0.986, 1.03] | 0.979 [0.967, 1.04] | — | — | equal |
| static_linear_faer | step_hooks | 2 | 5 | 135 µs [130 µs, 137 µs] | 4.98 µs [4.64 µs, 5.48 µs] | 0.0383 [0.0340, 0.0421] | 0.911 [0.785, 1.03] | 0.954 [0.901, 1.01] | 0.989 [0.807, 1.03] | — | — | equal |
| static_linear_faer | step_rest | 1 | 5 | 4.79 s [4.77 s, 4.79 s] | 4.86 s [4.72 s, 4.87 s] | 1.01 [0.986, 1.02] | 0.988 [0.978, 1.00] | 0.996 [0.987, 1.03] | 0.980 [0.968, 1.05] | — | — | equal |
| static_linear_faer | stiffness_assembly | 3 | 5 | 251 ms [250 ms, 253 ms] | 30.4 ms [30.2 ms, 30.4 ms] | 0.121 [0.120, 0.121] | 0.586 [0.579, 0.594] | 0.330 [0.326, 0.333] | 0.198 [0.197, 0.199] | — | — | equal |

Table "suite_solid static_linear_mumps at 1, 2, 4 ranks" (seconds of the phase, rank 0 or the case's maximum over the ranks as the case reports it; n = repetitions; rank r on the r-th core of the host's cores ordered NUMA node by NUMA node, recorded and checked; "/ head on as many cores" divides the medians of two different groups, P x T threads and P x T ranks, so it is not paired):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_mumps | corrector | 1 | 5 | 0.750 µs [0.500 µs, 0.950 µs] | 0.710 µs [0.680 µs, 1.03 µs] | 0.924 [0.747, 1.54] |  |  |  | equal |
| static_linear_mumps | corrector | 2 | 5 | 0.790 µs [0.640 µs, 1.24 µs] | 0.900 µs [0.840 µs, 1.09 µs] | 1.14 [0.677, 1.70] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | corrector | 4 | 5 | 0.830 µs [0.600 µs, 0.880 µs] | 0.810 µs [0.780 µs, 0.900 µs] | 1.05 [0.976, 1.30] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 2 | 5 | 81.5 ms [78.3 ms, 81.8 ms] | 31.3 ms [31.3 ms, 31.6 ms] | 0.384 [0.382, 0.403] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 4 | 5 | 91.4 ms [91.1 ms, 93.3 ms] | 44.5 ms [44.2 ms, 44.6 ms] | 0.484 [0.474, 0.490] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 1 | 5 | 13.3 ms [13.1 ms, 13.9 ms] | 8.89 ms [8.85 ms, 9.42 ms] | 0.668 [0.641, 0.719] |  |  |  | equal |
| static_linear_mumps | init | 2 | 5 | 9.11 ms [8.88 ms, 9.21 ms] | 5.70 ms [5.61 ms, 6.01 ms] | 0.619 [0.615, 0.674] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 4 | 5 | 5.86 ms [5.68 ms, 5.92 ms] | 3.67 ms [3.61 ms, 3.95 ms] | 0.644 [0.614, 0.687] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 1 | 5 | 154 ms [151 ms, 154 ms] | 66.8 ms [66.2 ms, 68.8 ms] | 0.434 [0.432, 0.446] |  |  |  | equal |
| static_linear_mumps | matrix | 2 | 5 | 74.5 ms [73.8 ms, 76.8 ms] | 33.1 ms [32.6 ms, 33.6 ms] | 0.437 [0.430, 0.452] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 4 | 5 | 37.4 ms [37.0 ms, 37.5 ms] | 17.9 ms [17.0 ms, 18.9 ms] | 0.480 [0.452, 0.505] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 1 | 5 | 905 µs [867 µs, 949 µs] | 908 µs [904 µs, 950 µs] | 1.00 [0.953, 1.10] |  |  |  | equal |
| static_linear_mumps | mesh | 2 | 5 | 847 µs [835 µs, 892 µs] | 865 µs [844 µs, 878 µs] | 1.01 [0.947, 1.04] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 4 | 5 | 883 µs [871 µs, 901 µs] | 889 µs [878 µs, 898 µs] | 1.01 [0.981, 1.02] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 1 | 5 | 0.240 µs [0.220 µs, 0.410 µs] | 0.0800 µs [0.0700 µs, 0.0800 µs] | 0.333 [0.171, 0.364] |  |  |  | equal |
| static_linear_mumps | predictor | 2 | 5 | 0.230 µs [0.220 µs, 0.250 µs] | 0.0800 µs [0.0710 µs, 0.0900 µs] | 0.348 [0.309, 0.409] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 4 | 5 | 0.240 µs [0.230 µs, 0.381 µs] | 0.0900 µs [0.0800 µs, 0.0900 µs] | 0.348 [0.236, 0.391] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 1 | 5 | 329 ms [328 ms, 330 ms] | 209 ms [209 ms, 211 ms] | 0.636 [0.632, 0.641] |  |  |  | equal |
| static_linear_mumps | solve_step | 2 | 5 | 224 ms [222 ms, 227 ms] | 169 ms [168 ms, 170 ms] | 0.755 [0.742, 0.760] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 4 | 5 | 133 ms [131 ms, 133 ms] | 107 ms [106 ms, 107 ms] | 0.808 [0.798, 0.812] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 1 | 5 | 91.1 µs [87.8 µs, 93.2 µs] | 4.32 µs [3.76 µs, 4.49 µs] | 0.0466 [0.0426, 0.0510] |  |  |  | equal |
| static_linear_mumps | step_hooks | 2 | 5 | 50.2 µs [45.7 µs, 54.3 µs] | 3.68 µs [3.52 µs, 3.83 µs] | 0.0723 [0.0678, 0.0791] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 4 | 5 | 29.8 µs [27.8 µs, 36.6 µs] | 3.95 µs [3.49 µs, 4.10 µs] | 0.131 [0.108, 0.135] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 1 | 5 | 176 ms [175 ms, 177 ms] | 142 ms [142 ms, 144 ms] | 0.809 [0.800, 0.823] |  |  |  | equal |
| static_linear_mumps | step_rest | 2 | 5 | 149 ms [149 ms, 153 ms] | 136 ms [135 ms, 137 ms] | 0.913 [0.889, 0.922] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 4 | 5 | 95.9 ms [94.0 ms, 96.8 ms] | 90.3 ms [89.3 ms, 90.7 ms] | 0.944 [0.934, 0.950] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 1 | 5 | 241 ms [240 ms, 247 ms] | 29.5 ms [29.4 ms, 29.6 ms] | 0.122 [0.119, 0.123] |  |  |  | equal |
| static_linear_mumps | stiffness_assembly | 2 | 5 | 123 ms [121 ms, 126 ms] | 15.1 ms [14.9 ms, 15.4 ms] | 0.123 [0.120, 0.124] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 4 | 5 | 47.4 ms [47.0 ms, 48.5 ms] | 8.18 ms [8.12 ms, 8.29 ms] | 0.173 [0.168, 0.174] |  |  |  | not compared (does not repeat) |

Progress log: first line 2026-11-17T17:19:47Z, last line 2026-11-17T20:20:40Z (202 lines).
Cohesive runs: 2.71 h of run time in `index.jsonl` (all attempts).
