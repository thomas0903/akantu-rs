# Final campaign, part D: static solves and solvers

- Machine: AMD EPYC 9R14, 8 cores, kernel 7.0.0-1014-aws.
- Sessions (UTC): 2026-11-17T13:14:30Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `53de1eeefb78`.
- Size `full`, repetitions requested: 5. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 65 planned, 65 complete and checked (only these are tabulated).

The head's SparseLU factorisation uses threads: true (measured by prepare.sh: the `factorize` phase of `static_linear_eigen` at 4 threads under 0.9 of 1 thread, `sparselu_probe` in the head's VARIANT.json; threaded variants are timed only then).

Checks (`checks.jsonl`): 40 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 20 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 150 runs, 150 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "The SparseLU translation against upstream's SparseLU" (the driver's `static_solve` phase: assembly, analysis, factorisation, solve; 1 rank, 10 steps; s):

| h | DOFs | C++ SparseLU (strict) | port before | port | port / C++ | port / port before | container memory peak (cgroup memory.peak) C++ / before / port MiB |
|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.86 [2.84, 2.87] | 7.59 [7.57, 7.60] | 2.11 [2.11, 2.13] | 0.740 [0.737, 0.743] | 0.279 [0.277, 0.280] | 497 [496, 524] / 561 [560, 567] / 399 [399, 400] |
| 0.12 | port, 2 threads |  |  | 1.70 [1.68, 1.70] | 0.595 [0.587, 0.598] | 0.803 [0.797, 0.807] (against 1 thread) |  |
| 0.12 | port, 4 threads |  |  | 1.41 [1.40, 1.42] | 0.494 [0.489, 0.495] | 0.666 [0.663, 0.669] (against 1 thread) |  |
| 0.12 | port, 8 threads |  |  | 1.27 [1.27, 1.29] | 0.447 [0.443, 0.452] | 0.603 [0.598, 0.613] (against 1 thread) |  |
| 0.06 | 97,551 | 241 [240, 243] | 778 [777, 779] | 219 [218, 221] | 0.909 [0.906, 0.918] | 0.281 [0.281, 0.283] | 7,608 [7,608, 7,608] / 6,918 [6,918, 6,918] / 5,326 [5,326, 5,326] |
| 0.06 | port, 2 threads |  |  | 136 [135, 139] | 0.566 [0.558, 0.577] | 0.622 [0.614, 0.634] (against 1 thread) |  |
| 0.06 | port, 4 threads |  |  | 86.2 [86.0, 86.3] | 0.358 [0.355, 0.359] | 0.393 [0.391, 0.396] (against 1 thread) |  |
| 0.06 | port, 8 threads |  |  | 62.7 [62.6, 63.0] | 0.260 [0.259, 0.261] | 0.286 [0.284, 0.288] (against 1 thread) |  |

Table "MUMPS in the cohesive benchmark" (static solve phase, s; the same MUMPS 5.7.3 on the same matrix, PORD):

| h | ranks | C++ | port before | port | port / C++ | port / port before |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.622 [0.617, 0.628] | 0.792 [0.791, 0.813] | 0.520 [0.517, 0.531] | 0.832 [0.823, 0.861] | 0.658 [0.636, 0.670] |
| 0.12 | 2 | 0.408 [0.406, 0.411] | 0.492 [0.486, 0.497] | 0.376 [0.368, 0.379] | 0.915 [0.898, 0.933] | 0.763 [0.740, 0.769] |
| 0.12 | 4 | 0.227 [0.226, 0.232] | 0.269 [0.266, 0.271] | 0.216 [0.215, 0.218] | 0.949 [0.934, 0.965] | 0.804 [0.799, 0.813] |
| 0.06 | 1 | 12.9 [12.8, 13.0] | 14.0 [14.0, 14.1] | 10.2 [10.1, 10.3] | 0.789 [0.785, 0.798] | 0.727 [0.725, 0.730] |
| 0.06 | 2 | 8.60 [8.57, 8.62] | 9.18 [9.14, 9.21] | 7.39 [7.36, 7.41] | 0.859 [0.858, 0.863] | 0.805 [0.805, 0.806] |
| 0.06 | 4 | 4.59 [4.52, 5.25] | 4.88 [4.81, 5.30] | 4.07 [4.03, 4.53] | 0.889 [0.776, 1.00] | 0.835 [0.826, 0.930] |

Table "suite_solid, the sparse solvers' phases (analysis, factorize, back_substitution)" (seconds of the phase over its calls, median [min, max]; n = repetitions in the medians; ratios paired per repetition; the threads and fast-mode variants run only for the cases whose timed phases run such code, plan.py `threaded_case`, — elsewhere):

| case | phase | calls | n | before | head | head / before | head 2 threads / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1) | fast / bitwise (8) | digests |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 1 | 5 | 42.6 ms [42.1 ms, 43.1 ms] | 47.6 ms [47.1 ms, 47.7 ms] | 1.12 [1.09, 1.13] | 0.951 [0.945, 0.961] | 0.967 [0.964, 0.991] | 0.980 [0.972, 1.00] | — | — | equal |
| static_linear_eigen | back_substitution | 20 | 5 | 210 ms [207 ms, 213 ms] | 134 ms [134 ms, 135 ms] | 0.642 [0.626, 0.648] | 0.990 [0.984, 1.00] | 0.995 [0.993, 1.00] | 1.00 [0.995, 1.01] | — | — | equal |
| static_linear_eigen | corrector | 1 | 5 | 1.06 µs [0.860 µs, 1.14 µs] | 0.780 µs [0.640 µs, 0.990 µs] | 0.717 [0.604, 1.15] | 1.09 [0.937, 1.38] | 1.06 [0.787, 1.28] | 1.08 [0.889, 1.44] | — | — | equal |
| static_linear_eigen | factorize | 2 | 5 | 5.53 s [5.51 s, 5.55 s] | 978 ms [970 ms, 988 ms] | 0.176 [0.175, 0.179] | 0.855 [0.842, 0.861] | 0.774 [0.771, 0.783] | 0.734 [0.723, 0.737] | — | — | equal |
| static_linear_eigen | init | 1 | 5 | 13.1 ms [13.0 ms, 13.2 ms] | 8.91 ms [8.62 ms, 9.06 ms] | 0.685 [0.665, 0.691] | 1.03 [1.02, 1.05] | 1.04 [1.02, 1.06] | 1.04 [1.04, 1.09] | — | — | equal |
| static_linear_eigen | matrix | 1 | 5 | 151 ms [147 ms, 153 ms] | 66.5 ms [64.1 ms, 67.0 ms] | 0.438 [0.435, 0.441] | 0.928 [0.921, 0.986] | 0.917 [0.884, 0.947] | 0.894 [0.857, 0.927] | — | — | equal |
| static_linear_eigen | mesh | 1 | 5 | 779 µs [758 µs, 783 µs] | 783 µs [748 µs, 800 µs] | 1.00 [0.960, 1.05] | 0.991 [0.981, 1.03] | 1.02 [0.972, 1.04] | 0.994 [0.963, 1.02] | — | — | equal |
| static_linear_eigen | predictor | 1 | 5 | 0.110 µs [0.110 µs, 0.350 µs] | 0.0800 µs [0.0800 µs, 0.210 µs] | 0.727 [0.229, 1.00] | 1.00 [1.00, 2.50] | 1.12 [0.476, 1.25] | 1.12 [0.429, 2.88] | — | — | equal |
| static_linear_eigen | solve_step | 1 | 5 | 3.04 s [3.03 s, 3.07 s] | 684 ms [680 ms, 689 ms] | 0.225 [0.222, 0.227] | 0.889 [0.868, 0.898] | 0.828 [0.817, 0.841] | 0.791 [0.783, 0.794] | — | — | equal |
| static_linear_eigen | step_hooks | 2 | 5 | 123 µs [69.3 µs, 131 µs] | 4.61 µs [4.55 µs, 9.62 µs] | 0.0375 [0.0348, 0.0830] | 0.902 [0.432, 1.15] | 0.890 [0.397, 0.989] | 0.926 [0.389, 0.989] | — | — | equal |
| static_linear_eigen | step_rest | 1 | 5 | 2.89 s [2.88 s, 2.91 s] | 617 ms [615 ms, 622 ms] | 0.214 [0.211, 0.215] | 0.885 [0.862, 0.895] | 0.822 [0.809, 0.830] | 0.782 [0.771, 0.784] | — | — | equal |
| static_linear_eigen | stiffness_assembly | 3 | 5 | 189 ms [181 ms, 192 ms] | 29.8 ms [29.6 ms, 29.8 ms] | 0.158 [0.155, 0.164] | 0.596 [0.571, 0.602] | 0.337 [0.330, 0.342] | 0.204 [0.202, 0.205] | — | — | equal |
| static_linear_faer | analysis | 1 | 5 | 49.7 ms [49.4 ms, 50.2 ms] | 55.2 ms [54.9 ms, 55.6 ms] | 1.11 [1.10, 1.12] | 0.996 [0.979, 0.999] | 0.993 [0.983, 1.00] | 0.995 [0.986, 1.00] | — | — | equal |
| static_linear_faer | back_substitution | 20 | 5 | 205 ms [204 ms, 206 ms] | 196 ms [195 ms, 197 ms] | 0.959 [0.952, 0.963] | 1.02 [1.01, 1.02] | 1.01 [1.00, 1.02] | 1.01 [1.00, 1.02] | — | — | equal |
| static_linear_faer | corrector | 1 | 5 | 1.09 µs [0.800 µs, 1.25 µs] | 1.08 µs [0.980 µs, 1.33 µs] | 1.06 [0.818, 1.25] | 0.898 [0.662, 1.11] | 0.774 [0.596, 1.00] | 0.731 [0.697, 1.15] | — | — | equal |
| static_linear_faer | factorize | 2 | 5 | 9.42 s [9.36 s, 9.47 s] | 9.38 s [9.36 s, 9.44 s] | 0.996 [0.988, 1.01] | 1.00 [0.992, 1.02] | 1.00 [0.998, 1.01] | 1.01 [0.994, 1.02] | — | — | equal |
| static_linear_faer | init | 1 | 5 | 13.2 ms [13.1 ms, 13.3 ms] | 9.02 ms [8.94 ms, 9.15 ms] | 0.683 [0.673, 0.698] | 1.01 [0.975, 1.03] | 1.04 [1.02, 1.07] | 1.05 [1.04, 1.07] | — | — | equal |
| static_linear_faer | matrix | 1 | 5 | 154 ms [150 ms, 161 ms] | 66.6 ms [64.9 ms, 66.8 ms] | 0.431 [0.413, 0.439] | 0.953 [0.935, 0.970] | 0.905 [0.890, 0.948] | 0.885 [0.859, 0.914] | — | — | equal |
| static_linear_faer | mesh | 1 | 5 | 775 µs [772 µs, 779 µs] | 788 µs [770 µs, 793 µs] | 1.02 [0.994, 1.02] | 0.977 [0.967, 1.04] | 0.984 [0.967, 1.00] | 1.00 [0.960, 1.02] | — | — | equal |
| static_linear_faer | predictor | 1 | 5 | 0.110 µs [0.100 µs, 0.110 µs] | 0.200 µs [0.0800 µs, 0.210 µs] | 1.82 [0.800, 1.91] | 0.429 [0.350, 1.12] | 0.450 [0.400, 1.00] | 0.476 [0.400, 1.25] | — | — | equal |
| static_linear_faer | solve_step | 1 | 5 | 5.01 s [4.94 s, 5.04 s] | 4.81 s [4.80 s, 4.89 s] | 0.960 [0.955, 0.990] | 1.01 [0.989, 1.02] | 1.00 [0.984, 1.03] | 1.01 [0.983, 1.03] | — | — | equal |
| static_linear_faer | step_hooks | 2 | 5 | 117 µs [99.3 µs, 142 µs] | 4.64 µs [4.05 µs, 4.92 µs] | 0.0391 [0.0347, 0.0408] | 0.904 [0.873, 1.03] | 0.862 [0.746, 1.19] | 0.874 [0.807, 1.03] | — | — | equal |
| static_linear_faer | step_rest | 1 | 5 | 4.86 s [4.79 s, 4.89 s] | 4.75 s [4.74 s, 4.83 s] | 0.977 [0.973, 1.01] | 1.01 [0.989, 1.02] | 1.00 [0.985, 1.03] | 1.01 [0.984, 1.03] | — | — | equal |
| static_linear_faer | stiffness_assembly | 3 | 5 | 250 ms [248 ms, 253 ms] | 30.1 ms [30.0 ms, 30.2 ms] | 0.120 [0.119, 0.121] | 0.588 [0.583, 0.591] | 0.331 [0.329, 0.334] | 0.200 [0.199, 0.211] | — | — | equal |

Table "suite_solid static_linear_mumps at 1, 2, 4 ranks" (seconds of the phase, rank 0 or the case's maximum over the ranks as the case reports it; n = repetitions; rank r on the r-th core of the host's cores ordered NUMA node by NUMA node, recorded and checked; "/ head on as many cores" divides the medians of two different groups, P x T threads and P x T ranks, so it is not paired):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_mumps | corrector | 1 | 5 | 0.850 µs [0.700 µs, 1.00 µs] | 0.840 µs [0.580 µs, 0.990 µs] | 0.965 [0.604, 1.37] |  |  |  | equal |
| static_linear_mumps | corrector | 2 | 5 | 0.780 µs [0.730 µs, 0.870 µs] | 0.850 µs [0.720 µs, 1.07 µs] | 1.09 [0.986, 1.43] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | corrector | 4 | 5 | 0.870 µs [0.760 µs, 1.06 µs] | 0.910 µs [0.840 µs, 1.28 µs] | 1.11 [0.802, 1.54] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 2 | 5 | 78.4 ms [78.2 ms, 79.1 ms] | 31.3 ms [31.3 ms, 31.3 ms] | 0.399 [0.396, 0.400] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 4 | 5 | 92.0 ms [91.8 ms, 94.2 ms] | 44.5 ms [44.5 ms, 44.5 ms] | 0.484 [0.473, 0.485] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 1 | 5 | 13.2 ms [13.1 ms, 13.7 ms] | 8.87 ms [8.84 ms, 9.45 ms] | 0.670 [0.643, 0.715] |  |  |  | equal |
| static_linear_mumps | init | 2 | 5 | 8.96 ms [8.84 ms, 9.31 ms] | 5.76 ms [5.68 ms, 6.05 ms] | 0.634 [0.611, 0.685] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | init | 4 | 5 | 5.66 ms [5.57 ms, 5.88 ms] | 3.72 ms [3.64 ms, 3.93 ms] | 0.656 [0.620, 0.706] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 1 | 5 | 154 ms [152 ms, 156 ms] | 66.8 ms [66.6 ms, 68.3 ms] | 0.434 [0.428, 0.447] |  |  |  | equal |
| static_linear_mumps | matrix | 2 | 5 | 76.1 ms [75.1 ms, 77.5 ms] | 32.7 ms [32.5 ms, 33.4 ms] | 0.428 [0.419, 0.445] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 4 | 5 | 37.4 ms [37.2 ms, 37.6 ms] | 17.6 ms [16.9 ms, 18.0 ms] | 0.474 [0.451, 0.480] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 1 | 5 | 891 µs [848 µs, 953 µs] | 901 µs [873 µs, 946 µs] | 0.980 [0.931, 1.12] |  |  |  | equal |
| static_linear_mumps | mesh | 2 | 5 | 850 µs [834 µs, 852 µs] | 858 µs [853 µs, 871 µs] | 1.02 [1.01, 1.02] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | mesh | 4 | 5 | 877 µs [862 µs, 891 µs] | 889 µs [875 µs, 942 µs] | 1.03 [0.982, 1.08] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 1 | 5 | 0.240 µs [0.220 µs, 0.370 µs] | 0.0700 µs [0.0700 µs, 0.0800 µs] | 0.292 [0.189, 0.364] |  |  |  | equal |
| static_linear_mumps | predictor | 2 | 5 | 0.240 µs [0.230 µs, 0.340 µs] | 0.0800 µs [0.0800 µs, 0.0900 µs] | 0.333 [0.235, 0.360] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | predictor | 4 | 5 | 0.290 µs [0.220 µs, 0.460 µs] | 0.0800 µs [0.0700 µs, 0.0800 µs] | 0.276 [0.174, 0.333] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 1 | 5 | 330 ms [327 ms, 337 ms] | 209 ms [209 ms, 212 ms] | 0.634 [0.621, 0.649] |  |  |  | equal |
| static_linear_mumps | solve_step | 2 | 5 | 225 ms [224 ms, 227 ms] | 169 ms [168 ms, 170 ms] | 0.750 [0.741, 0.756] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 4 | 5 | 132 ms [132 ms, 132 ms] | 107 ms [106 ms, 107 ms] | 0.807 [0.801, 0.812] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 1 | 5 | 87.8 µs [81.0 µs, 111 µs] | 4.29 µs [4.01 µs, 4.88 µs] | 0.0507 [0.0362, 0.0602] |  |  |  | equal |
| static_linear_mumps | step_hooks | 2 | 5 | 51.2 µs [49.9 µs, 53.3 µs] | 3.24 µs [3.12 µs, 3.84 µs] | 0.0634 [0.0599, 0.0750] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_hooks | 4 | 5 | 29.4 µs [28.9 µs, 37.8 µs] | 3.88 µs [3.54 µs, 4.00 µs] | 0.127 [0.0946, 0.138] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 1 | 5 | 176 ms [175 ms, 181 ms] | 143 ms [142 ms, 144 ms] | 0.809 [0.787, 0.825] |  |  |  | equal |
| static_linear_mumps | step_rest | 2 | 5 | 151 ms [150 ms, 153 ms] | 137 ms [136 ms, 137 ms] | 0.906 [0.890, 0.917] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | step_rest | 4 | 5 | 96.0 ms [95.3 ms, 96.5 ms] | 90.6 ms [89.7 ms, 90.7 ms] | 0.945 [0.934, 0.951] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 1 | 5 | 242 ms [239 ms, 250 ms] | 29.7 ms [29.6 ms, 30.8 ms] | 0.124 [0.119, 0.127] |  |  |  | equal |
| static_linear_mumps | stiffness_assembly | 2 | 5 | 122 ms [119 ms, 125 ms] | 15.3 ms [15.1 ms, 15.4 ms] | 0.127 [0.122, 0.127] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 4 | 5 | 47.8 ms [47.3 ms, 48.5 ms] | 8.30 ms [8.16 ms, 8.51 ms] | 0.173 [0.171, 0.178] |  |  |  | not compared (does not repeat) |

Progress log: first line 2026-11-17T13:14:39Z, last line 2026-11-17T16:05:46Z (181 lines).
Cohesive runs: 2.56 h of run time in `index.jsonl` (all attempts).
