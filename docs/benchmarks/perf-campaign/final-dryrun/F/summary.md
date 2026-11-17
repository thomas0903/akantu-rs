# Final campaign, part F: set-up, distribution and the MPI layer

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T05:18:14Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Checks (`checks.jsonl`): 8 points; at every point the thread count and precision of each port run (`rust_exec` of `summary.json`) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 2 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 24 runs, 24 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "Set-up costs" (the cohesive driver, 1 step; rank 0's Chrono phases):

| h | ranks | code | read_mesh s | dist_mesh s | init_full s | before_step s | static solve s |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | C++ | 0.0402 [0.0401, 0.0402] | 0.000119 [0.000118, 0.000119] | 1.03 [1.02, 1.03] | 0.304 [0.301, 0.307] | 0.660 [0.657, 0.662] |
| 0.12 | 1 | port before | 0.0215 [0.0215, 0.0216] | 0.00000110 [0.00000109, 0.00000111] | 0.407 [0.406, 0.407] | 0.437 [0.435, 0.438] | 0.919 [0.916, 0.922] |
| 0.12 | 1 | port | 0.0146 [0.0145, 0.0147] | 0.00000125 [0.00000117, 0.00000132] | 0.172 [0.171, 0.173] | 0.112 [0.112, 0.112] | 0.556 [0.554, 0.557] |
| 0.12 | 2 | C++ | 0.0405 [0.0401, 0.0409] | 0.255 [0.254, 0.255] | 0.508 [0.500, 0.516] | 0.145 [0.144, 0.146] | 0.453 [0.451, 0.455] |
| 0.12 | 2 | port before | 0.0219 [0.0217, 0.0220] | 0.0361 [0.0359, 0.0364] | 0.232 [0.231, 0.232] | 0.212 [0.209, 0.215] | 0.579 [0.579, 0.580] |
| 0.12 | 2 | port | 0.0146 [0.0146, 0.0146] | 0.0247 [0.0246, 0.0248] | 0.100 [0.100, 0.100] | 0.0547 [0.0541, 0.0552] | 0.414 [0.412, 0.416] |
| 0.12 | 4 | C++ | 0.0404 [0.0403, 0.0405] | 0.280 [0.279, 0.281] | 0.258 [0.258, 0.259] | 0.0767 [0.0763, 0.0770] | 0.256 [0.256, 0.256] |
| 0.12 | 4 | port before | 0.0219 [0.0217, 0.0220] | 0.0424 [0.0421, 0.0427] | 0.127 [0.127, 0.127] | 0.105 [0.104, 0.106] | 0.318 [0.315, 0.321] |
| 0.12 | 4 | port | 0.0146 [0.0145, 0.0146] | 0.0260 [0.0259, 0.0260] | 0.0560 [0.0559, 0.0560] | 0.0263 [0.0262, 0.0263] | 0.233 [0.228, 0.237] |
| 0.12 | 8 | C++ | 0.0397 [0.0396, 0.0398] | 0.295 [0.294, 0.295] | 0.138 [0.137, 0.140] | 0.0368 [0.0366, 0.0371] | 0.185 [0.185, 0.186] |
| 0.12 | 8 | port before | 0.0219 [0.0219, 0.0220] | 0.0563 [0.0563, 0.0563] | 0.0731 [0.0717, 0.0746] | 0.0534 [0.0525, 0.0542] | 0.215 [0.210, 0.221] |
| 0.12 | 8 | port | 0.0147 [0.0147, 0.0148] | 0.0279 [0.0272, 0.0285] | 0.0345 [0.0322, 0.0369] | 0.0125 [0.0125, 0.0126] | 0.177 [0.170, 0.184] |

Table "Set-up, port / upstream" (paired):

| h | ranks | read_mesh | dist_mesh | init_full | before_step | static solve |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.363 [0.361, 0.365] | 0.0105 [0.00983, 0.0112] | 0.167 [0.166, 0.168] | 0.369 [0.365, 0.373] | 0.842 [0.837, 0.848] |
| 0.12 | 2 | 0.361 [0.357, 0.366] | 0.0970 [0.0969, 0.0972] | 0.197 [0.194, 0.201] | 0.376 [0.370, 0.383] | 0.913 [0.912, 0.913] |
| 0.12 | 4 | 0.361 [0.360, 0.362] | 0.0929 [0.0923, 0.0935] | 0.217 [0.216, 0.217] | 0.343 [0.341, 0.345] | 0.908 [0.893, 0.924] |
| 0.12 | 8 | 0.371 [0.369, 0.373] | 0.0945 [0.0925, 0.0965] | 0.249 [0.235, 0.263] | 0.340 [0.339, 0.342] | 0.956 [0.916, 0.995] |

Table "Distribution (`suite_setup` `dist_mesh_tet4`, size `l`: 1,296,000 Tet4)" (seconds of the phase, rank 0 or the case's maximum over the ranks as the case reports it):

| case | phase | ranks | before | head | head / before | head ranks x threads | time | / head at the same cores | digests |
|---|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | distribute | 2 | 8.50 ms [8.49 ms, 8.52 ms] | 4.59 ms [4.58 ms, 4.61 ms] | 0.540 [0.539, 0.541] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 4 | 11.0 ms [11.0 ms, 11.0 ms] | 6.67 ms [6.66 ms, 6.68 ms] | 0.604 [0.604, 0.604] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 8 | 13.9 ms [13.8 ms, 14.1 ms] | 8.69 ms [8.69 ms, 8.69 ms] | 0.624 [0.618, 0.630] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 2 | 2.35 ms [2.35 ms, 2.35 ms] | 1.79 ms [1.76 ms, 1.81 ms] | 0.759 [0.748, 0.771] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 4 | 2.37 ms [2.35 ms, 2.38 ms] | 1.79 ms [1.79 ms, 1.80 ms] | 0.758 [0.751, 0.766] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 8 | 2.33 ms [2.33 ms, 2.34 ms] | 1.80 ms [1.79 ms, 1.80 ms] | 0.770 [0.769, 0.771] |  |  |  | equal |
| dist_mesh_tet4 | partition | 2 | 6.67 ms [6.67 ms, 6.68 ms] | 3.22 ms [3.21 ms, 3.23 ms] | 0.483 [0.482, 0.484] |  |  |  | equal |
| dist_mesh_tet4 | partition | 4 | 8.66 ms [8.66 ms, 8.66 ms] | 5.13 ms [5.12 ms, 5.13 ms] | 0.592 [0.592, 0.593] |  |  |  | equal |
| dist_mesh_tet4 | partition | 8 | 10.4 ms [10.2 ms, 10.5 ms] | 6.71 ms [6.71 ms, 6.72 ms] | 0.648 [0.639, 0.658] |  |  |  | equal |

Table "Method A, separate launches" (µs, median [min, max]; ping-pong: half round trip; halo: one exchange with every other rank, slowest rank):

| Transport | Case | Raw MPI (C) | akantu-comm before | akantu-comm head | head / raw, paired | before / raw | head / before |
|---|---|---|---|---|---|---|---|
| shared memory | halo, 4 ranks, 8 KiB | 4.37 [4.30, 4.44] | 16.8 [10.3, 23.3] | 18.3 [9.54, 27.1] | 4.23 [2.15, 6.30] | 3.82 [2.39, 5.25] | 1.52 [0.409, 2.63] |
| shared memory | ping-pong 8 B | 0.155 [0.150, 0.160] | 0.995 [0.990, 1.00] | 0.990 [0.990, 0.990] | 6.39 [6.19, 6.60] | 6.43 [6.19, 6.67] | 0.995 [0.990, 1.00] |
| shared memory | ping-pong 1 MiB | 53.7 [53.6, 53.9] | 171 [171, 172] | 134 [134, 134] | 2.50 [2.49, 2.50] | 3.19 [3.17, 3.21] | 0.782 [0.778, 0.786] |
| TCP | halo, 4 ranks, 8 KiB | 33.3 [32.7, 33.9] | 46.5 [46.1, 46.9] | 45.4 [45.4, 45.5] | 1.37 [1.34, 1.39] | 1.40 [1.36, 1.43] | 0.977 [0.967, 0.987] |
| TCP | ping-pong 8 B | 8.36 [8.28, 8.44] | 9.77 [9.67, 9.86] | 9.72 [9.64, 9.80] | 1.16 [1.16, 1.16] | 1.17 [1.17, 1.17] | 0.995 [0.994, 0.997] |
| TCP | ping-pong 1 MiB | 170 [170, 170] | 294 [293, 295] | 251 [251, 252] | 1.48 [1.47, 1.48] | 1.73 [1.72, 1.73] | 0.855 [0.855, 0.855] |

Table "Method B, one job" (`mpi_bench calibrated`, 9 interleaved blocks per job; µs; backend ratio = (akantu − calibration) / raw, the MPI tier's gated figure):

| Transport | Case | variant | raw | akantu-comm | calibration | akantu / raw | backend ratio |
|---|---|---|---|---|---|---|---|
| shared memory | halo 8192 B, 4 ranks | before | 4.70 [4.62, 4.78] | 10.6 [10.6, 10.7] | nan [nan, nan] | 2.26 [2.19, 2.33] | 2.26 [2.19, 2.33] |
| shared memory | halo 8192 B, 4 ranks | head | 4.59 [4.54, 4.64] | 9.88 [9.76, 10.0] | nan [nan, nan] | 2.16 [2.10, 2.22] | 2.16 [2.10, 2.22] |
| shared memory | pingpong 8 B, 2 ranks | before | 0.160 [0.160, 0.160] | 1.00 [1.00, 1.00] | 1.39 [1.35, 1.43] | 6.38 [6.30, 6.45] | -2.29 [-2.54, -2.04] |
| shared memory | pingpong 8 B, 2 ranks | head | 0.160 [0.160, 0.160] | 1.03 [1.03, 1.03] | 1.38 [1.35, 1.40] | 6.49 [6.49, 6.50] | -2.13 [-2.26, -1.99] |
| shared memory | pingpong 1048576 B, 2 ranks | before | 58.2 [57.5, 58.9] | 953 [945, 961] | 964 [957, 972] | 16.4 [16.0, 16.8] | -0.181 [-0.182, -0.180] |
| shared memory | pingpong 1048576 B, 2 ranks | head | 58.3 [57.5, 59.1] | 976 [966, 986] | 950 [937, 962] | 16.7 [16.7, 16.7] | 0.469 [0.414, 0.523] |
| TCP | halo 8192 B, 4 ranks | before | 34.7 [34.5, 34.8] | 47.3 [47.1, 47.5] | nan [nan, nan] | 1.37 [1.36, 1.37] | 1.37 [1.36, 1.37] |
| TCP | halo 8192 B, 4 ranks | head | 33.6 [33.1, 34.1] | 45.5 [44.8, 46.3] | nan [nan, nan] | 1.35 [1.35, 1.35] | 1.35 [1.35, 1.35] |
| TCP | pingpong 8 B, 2 ranks | before | 8.35 [8.23, 8.47] | 9.81 [9.76, 9.86] | 1.35 [1.34, 1.35] | 1.17 [1.16, 1.19] | 1.01 [1.00, 1.02] |
| TCP | pingpong 8 B, 2 ranks | head | 8.37 [8.27, 8.47] | 9.77 [9.68, 9.85] | 1.34 [1.33, 1.35] | 1.17 [1.16, 1.17] | 1.01 [1.00, 1.01] |
| TCP | pingpong 1048576 B, 2 ranks | before | 169 [167, 172] | 1,056 [1,045, 1,066] | 972 [964, 980] | 6.24 [6.23, 6.25] | 0.485 [0.481, 0.489] |
| TCP | pingpong 1048576 B, 2 ranks | head | 169 [167, 170] | 1,119 [1,115, 1,123] | 966 [962, 970] | 6.64 [6.59, 6.69] | 0.907 [0.892, 0.923] |

The I/O cases (mesh reading and writing, dumpers, parser) and the distributed exchanges are cases of `suite_setup`: their tables are in part B.

Progress log: first line 2026-11-17T05:18:15Z, last line 2026-11-17T05:20:58Z (71 lines).
Cohesive runs: 0.03 h of run time in `index.jsonl` (all attempts).
