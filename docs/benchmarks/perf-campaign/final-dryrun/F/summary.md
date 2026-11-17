# Final campaign, part F: set-up, distribution and the MPI layer

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T07:14:51Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 38 planned, 38 complete and checked (only these are tabulated).

Checks (`checks.jsonl`): 8 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 2 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 24 runs, 24 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "Set-up costs" (the cohesive driver, 1 step; rank 0's Chrono phases):

| h | ranks | code | read_mesh s | dist_mesh s | init_full s | before_step s | static solve s |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | C++ | 0.0408 [0.0406, 0.0410] | 0.000119 [0.000119, 0.000119] | 1.03 [1.03, 1.03] | 0.306 [0.305, 0.307] | 0.663 [0.657, 0.669] |
| 0.12 | 1 | port before | 0.0216 [0.0215, 0.0216] | 0.00000110 [0.00000108, 0.00000112] | 0.406 [0.406, 0.406] | 0.441 [0.439, 0.443] | 0.920 [0.915, 0.925] |
| 0.12 | 1 | port | 0.0144 [0.0144, 0.0145] | 0.00000109 [0.00000103, 0.00000116] | 0.169 [0.169, 0.169] | 0.113 [0.112, 0.113] | 0.554 [0.553, 0.555] |
| 0.12 | 2 | C++ | 0.0402 [0.0398, 0.0406] | 0.255 [0.255, 0.255] | 0.508 [0.501, 0.516] | 0.146 [0.145, 0.148] | 0.453 [0.453, 0.454] |
| 0.12 | 2 | port before | 0.0220 [0.0219, 0.0220] | 0.0362 [0.0359, 0.0364] | 0.231 [0.231, 0.232] | 0.214 [0.212, 0.215] | 0.581 [0.580, 0.582] |
| 0.12 | 2 | port | 0.0147 [0.0146, 0.0148] | 0.0246 [0.0246, 0.0246] | 0.100 [0.1000, 0.100] | 0.0551 [0.0548, 0.0554] | 0.408 [0.406, 0.411] |
| 0.12 | 4 | C++ | 0.0403 [0.0401, 0.0404] | 0.280 [0.279, 0.281] | 0.259 [0.255, 0.264] | 0.0766 [0.0723, 0.0810] | 0.258 [0.253, 0.264] |
| 0.12 | 4 | port before | 0.0216 [0.0216, 0.0216] | 0.0423 [0.0423, 0.0424] | 0.131 [0.127, 0.136] | 0.106 [0.103, 0.108] | 0.316 [0.308, 0.324] |
| 0.12 | 4 | port | 0.0147 [0.0146, 0.0148] | 0.0262 [0.0261, 0.0263] | 0.0581 [0.0557, 0.0605] | 0.0282 [0.0257, 0.0307] | 0.249 [0.232, 0.265] |
| 0.12 | 8 | C++ | 0.0421 [0.0401, 0.0441] | 0.310 [0.299, 0.320] | 0.145 [0.145, 0.146] | 0.0396 [0.0358, 0.0433] | 0.192 [0.182, 0.203] |
| 0.12 | 8 | port before | 0.0222 [0.0222, 0.0222] | 0.0564 [0.0563, 0.0565] | 0.0730 [0.0720, 0.0739] | 0.0524 [0.0509, 0.0539] | 0.228 [0.219, 0.238] |
| 0.12 | 8 | port | 0.0149 [0.0148, 0.0149] | 0.0281 [0.0280, 0.0282] | 0.0375 [0.0355, 0.0396] | 0.0166 [0.0165, 0.0168] | 0.184 [0.184, 0.184] |

Table "Set-up, port / upstream" (paired):

| h | ranks | read_mesh | dist_mesh | init_full | before_step | static solve |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.353 [0.353, 0.354] | 0.00920 [0.00867, 0.00973] | 0.164 [0.164, 0.164] | 0.368 [0.366, 0.370] | 0.836 [0.827, 0.844] |
| 0.12 | 2 | 0.365 [0.364, 0.367] | 0.0964 [0.0961, 0.0966] | 0.197 [0.194, 0.200] | 0.377 [0.375, 0.379] | 0.901 [0.894, 0.909] |
| 0.12 | 4 | 0.365 [0.362, 0.368] | 0.0935 [0.0929, 0.0941] | 0.224 [0.211, 0.238] | 0.370 [0.317, 0.424] | 0.965 [0.881, 1.05] |
| 0.12 | 8 | 0.354 [0.336, 0.372] | 0.0909 [0.0876, 0.0942] | 0.258 [0.243, 0.274] | 0.424 [0.388, 0.460] | 0.960 [0.907, 1.01] |

Table "Distribution (`suite_setup` `dist_mesh_tet4`, size `l`: 1,296,000 Tet4)" (seconds of the phase, rank 0 or the case's maximum over the ranks as the case reports it; n = repetitions; rank r on the r-th core of the host's cores ordered NUMA node by NUMA node, recorded and checked; "/ head on as many cores" divides the medians of two different groups, P x T threads and P x T ranks, so it is not paired):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | distribute | 2 | 2 | 8.55 ms [8.52 ms, 8.58 ms] | 4.63 ms [4.55 ms, 4.71 ms] | 0.542 [0.531, 0.553] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 4 | 2 | 11.0 ms [11.0 ms, 11.1 ms] | 6.70 ms [6.67 ms, 6.73 ms] | 0.607 [0.601, 0.614] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 8 | 2 | 15.3 ms [13.8 ms, 16.7 ms] | 8.74 ms [8.69 ms, 8.79 ms] | 0.577 [0.519, 0.635] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 2 | 2 | 2.34 ms [2.33 ms, 2.34 ms] | 1.77 ms [1.75 ms, 1.78 ms] | 0.755 [0.752, 0.759] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 4 | 2 | 2.35 ms [2.35 ms, 2.35 ms] | 1.79 ms [1.79 ms, 1.79 ms] | 0.760 [0.759, 0.762] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 8 | 2 | 2.35 ms [2.33 ms, 2.38 ms] | 1.79 ms [1.78 ms, 1.79 ms] | 0.759 [0.750, 0.767] |  |  |  | equal |
| dist_mesh_tet4 | partition | 2 | 2 | 6.73 ms [6.70 ms, 6.75 ms] | 3.27 ms [3.19 ms, 3.35 ms] | 0.486 [0.473, 0.500] |  |  |  | equal |
| dist_mesh_tet4 | partition | 4 | 2 | 8.68 ms [8.63 ms, 8.72 ms] | 5.13 ms [5.11 ms, 5.15 ms] | 0.591 [0.586, 0.597] |  |  |  | equal |
| dist_mesh_tet4 | partition | 8 | 2 | 10.3 ms [10.3 ms, 10.3 ms] | 6.76 ms [6.71 ms, 6.80 ms] | 0.656 [0.649, 0.664] |  |  |  | equal |

Table "Method A, separate launches" (µs, median [min, max]; ping-pong: half round trip; halo: one exchange with every other rank, slowest rank):

| Transport | Case | Raw MPI (C) | akantu-comm before | akantu-comm head | head / raw, paired | before / raw | head / before |
|---|---|---|---|---|---|---|---|
| shared memory | halo, 4 ranks, 8 KiB | 4.41 [4.40, 4.41] | 10.5 [10.4, 10.6] | 9.68 [9.64, 9.71] | 2.20 [2.19, 2.20] | 2.38 [2.36, 2.41] | 0.921 [0.913, 0.930] |
| shared memory | ping-pong 8 B | 0.150 [0.150, 0.150] | 1.00 [0.990, 1.01] | 1.00 [1.00, 1.00] | 6.67 [6.67, 6.67] | 6.67 [6.60, 6.73] | 1.00 [0.990, 1.01] |
| shared memory | ping-pong 1 MiB | 54.0 [54.0, 54.0] | 171 [171, 172] | 135 [135, 136] | 2.51 [2.49, 2.52] | 3.17 [3.17, 3.18] | 0.789 [0.788, 0.791] |
| TCP | halo, 4 ranks, 8 KiB | 35.4 [33.8, 37.1] | 54.9 [54.5, 55.2] | 53.3 [53.2, 53.5] | 1.51 [1.43, 1.58] | 1.55 [1.47, 1.63] | 0.972 [0.970, 0.975] |
| TCP | ping-pong 8 B | 8.39 [8.38, 8.41] | 9.89 [9.88, 9.89] | 9.85 [9.80, 9.90] | 1.17 [1.17, 1.18] | 1.18 [1.17, 1.18] | 0.996 [0.992, 1.00] |
| TCP | ping-pong 1 MiB | 171 [169, 173] | 294 [294, 294] | 254 [254, 255] | 1.49 [1.48, 1.50] | 1.72 [1.71, 1.74] | 0.864 [0.863, 0.865] |

Table "Method B, one job" (`mpi_bench calibrated`, 9 interleaved blocks per job; µs; backend ratio = (akantu − calibration) / raw, the MPI tier's gated figure):

| Transport | Case | variant | raw | akantu-comm | calibration | akantu / raw | backend ratio |
|---|---|---|---|---|---|---|---|
| shared memory | halo 8192 B, 4 ranks | before | 4.65 [4.64, 4.66] | 10.8 [10.6, 10.9] | nan [nan, nan] | 2.31 [2.29, 2.34] | 2.31 [2.29, 2.34] |
| shared memory | halo 8192 B, 4 ranks | head | 4.74 [4.73, 4.75] | 9.94 [9.89, 9.98] | nan [nan, nan] | 2.10 [2.06, 2.14] | 2.10 [2.06, 2.14] |
| shared memory | pingpong 8 B, 2 ranks | before | 0.150 [0.150, 0.150] | 1.02 [1.01, 1.03] | 1.35 [1.35, 1.35] | 6.60 [6.55, 6.65] | -2.09 [-2.14, -2.04] |
| shared memory | pingpong 8 B, 2 ranks | head | 0.155 [0.150, 0.160] | 1.02 [1.00, 1.04] | 1.33 [1.33, 1.34] | 6.42 [6.40, 6.45] | -1.86 [-1.96, -1.76] |
| shared memory | pingpong 1048576 B, 2 ranks | before | 57.8 [57.6, 57.9] | 946 [934, 958] | 950 [940, 961] | 16.4 [16.2, 16.5] | -0.0565 [-0.0620, -0.0510] |
| shared memory | pingpong 1048576 B, 2 ranks | head | 59.2 [58.8, 59.6] | 988 [978, 999] | 968 [952, 983] | 16.7 [16.4, 17.0] | 0.309 [0.190, 0.429] |
| TCP | halo 8192 B, 4 ranks | before | 35.0 [35.0, 35.1] | 47.7 [47.7, 47.7] | nan [nan, nan] | 1.36 [1.36, 1.36] | 1.36 [1.36, 1.36] |
| TCP | halo 8192 B, 4 ranks | head | 34.2 [34.1, 34.3] | 45.9 [45.9, 45.9] | nan [nan, nan] | 1.34 [1.34, 1.34] | 1.34 [1.34, 1.34] |
| TCP | pingpong 8 B, 2 ranks | before | 8.46 [8.45, 8.48] | 9.91 [9.91, 9.91] | 1.35 [1.34, 1.35] | 1.17 [1.17, 1.17] | 1.01 [1.01, 1.01] |
| TCP | pingpong 8 B, 2 ranks | head | 8.34 [8.28, 8.41] | 9.77 [9.68, 9.85] | 1.39 [1.35, 1.43] | 1.17 [1.17, 1.17] | 1.00 [1.00, 1.01] |
| TCP | pingpong 1048576 B, 2 ranks | before | 167 [167, 168] | 1,068 [1,062, 1,075] | 978 [972, 985] | 6.39 [6.38, 6.40] | 0.544 [0.534, 0.553] |
| TCP | pingpong 1048576 B, 2 ranks | head | 171 [170, 173] | 1,119 [1,114, 1,124] | 978 [969, 988] | 6.54 [6.52, 6.56] | 0.829 [0.797, 0.860] |

The I/O cases (mesh reading and writing, dumpers, parser) and the distributed exchanges are cases of `suite_setup`: their tables are in part B.

Progress log: first line 2026-11-17T07:14:55Z, last line 2026-11-17T07:18:32Z (60 lines).
Cohesive runs: 0.03 h of run time in `index.jsonl` (all attempts).
