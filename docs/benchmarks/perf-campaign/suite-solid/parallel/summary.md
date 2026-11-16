# suite_solid parallel cases, size m

Commit `c698395e8` (`mpi` feature; `mumps` for the MUMPS case), AWS `m7a.2xlarge` (AMD EPYC 9R14), project images `akantu-rs-mpi:dev` / `akantu-rs-mumps:dev`, 2026-11-16 10:30–11:58 UTC; `mpirun -n P --bind-to core --map-by core`, P = 1, 2, 4 on cores 0–3 (timing wrapper), P = 8 on cores 0–7 (whole machine); 5 repetitions, rank counts rotated. Times: the largest over the ranks. Digests: one per rank count; they repeat at a fixed rank count for the two `par_*` cases (5 of 5 runs each) and differ between rank counts by construction (`crates/akantu-bench/README.md`). `static_linear_mumps` does **not** repeat at P = 2 and 4 (3 and 5 distinct digests in 5 runs; also with `OMP_NUM_THREADS=1`, `OPENBLAS_NUM_THREADS=1`, which the image sets anyway): the distributed MUMPS solve itself varies from run to run.

## par_explicit_tet4_ss, par_cohesive_extrinsic

Suite `solid`, size `m`; variants: `np1`, `np2`, `np4`, `np8`; reference `np1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to np1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| par_cohesive_extrinsic | mesh | np1 | 5 | 1 | 2.15 ms [2.14 ms, 2.58 ms] | 2.15 ms [2.14 ms, 2.58 ms] | 44.9 [44.5, 53.7] | – | DIFFER (4) |
| par_cohesive_extrinsic | mesh | np2 | 5 | 1 | 2.15 ms [2.15 ms, 2.50 ms] | 2.15 ms [2.15 ms, 2.50 ms] | 44.9 [44.8, 52.1] | 1.000 [0.970, 1.008] | DIFFER (4) |
| par_cohesive_extrinsic | mesh | np4 | 5 | 1 | 2.14 ms [2.06 ms, 2.57 ms] | 2.14 ms [2.06 ms, 2.57 ms] | 44.5 [42.9, 53.5] | 0.996 [0.924, 1.031] | DIFFER (4) |
| par_cohesive_extrinsic | mesh | np8 | 5 | 1 | 2.57 ms [2.50 ms, 2.60 ms] | 2.57 ms [2.50 ms, 2.60 ms] | 53.5 [52.1, 54.1] | 1.166 [0.996, 1.201] | DIFFER (4) |
| par_cohesive_extrinsic | init | np1 | 5 | 1 | 670.48 ms [667.65 ms, 781.71 ms] | 670.48 ms [667.65 ms, 781.71 ms] | 13968.4 [13909.3, 16285.7] | – | DIFFER (4) |
| par_cohesive_extrinsic | init | np2 | 5 | 1 | 376.19 ms [375.86 ms, 447.68 ms] | 376.19 ms [375.86 ms, 447.68 ms] | 7837.2 [7830.5, 9326.7] | 0.563 [0.561, 0.573] | DIFFER (4) |
| par_cohesive_extrinsic | init | np4 | 5 | 1 | 202.38 ms [201.25 ms, 243.88 ms] | 202.38 ms [201.25 ms, 243.88 ms] | 4216.2 [4192.8, 5080.8] | 0.302 [0.301, 0.312] | DIFFER (4) |
| par_cohesive_extrinsic | init | np8 | 5 | 1 | 130.08 ms [128.29 ms, 131.17 ms] | 130.08 ms [128.29 ms, 131.17 ms] | 2710.1 [2672.7, 2732.8] | 0.194 [0.166, 0.196] | DIFFER (4) |
| par_cohesive_extrinsic | preload | np1 | 5 | 1 | 2.171 s [2.162 s, 2.529 s] | 2.171 s [2.162 s, 2.529 s] | 45231.1 [45043.0, 52691.8] | – | DIFFER (4) |
| par_cohesive_extrinsic | preload | np2 | 5 | 1 | 1.244 s [1.227 s, 1.467 s] | 1.244 s [1.227 s, 1.467 s] | 25917.8 [25560.3, 30568.8] | 0.573 [0.566, 0.599] | DIFFER (4) |
| par_cohesive_extrinsic | preload | np4 | 5 | 1 | 783.79 ms [767.14 ms, 918.69 ms] | 783.79 ms [767.14 ms, 918.69 ms] | 16329.0 [15982.0, 19139.5] | 0.361 [0.354, 0.388] | DIFFER (4) |
| par_cohesive_extrinsic | preload | np8 | 5 | 1 | 748.01 ms [697.17 ms, 758.81 ms] | 748.01 ms [697.17 ms, 758.81 ms] | 15583.5 [14524.4, 15808.4] | 0.322 [0.296, 0.350] | DIFFER (4) |
| par_cohesive_extrinsic | step | np1 | 5 | 16 | 2.543 s [2.529 s, 3.016 s] | 158.96 ms [158.07 ms, 188.49 ms] | 3311.7 [3293.1, 3926.9] | – | DIFFER (4) |
| par_cohesive_extrinsic | step | np2 | 5 | 16 | 1.612 s [1.609 s, 1.918 s] | 100.76 ms [100.58 ms, 119.86 ms] | 2099.1 [2095.5, 2497.1] | 0.634 [0.621, 0.637] | DIFFER (4) |
| par_cohesive_extrinsic | step | np4 | 5 | 16 | 932.99 ms [931.83 ms, 1.120 s] | 58.31 ms [58.24 ms, 70.01 ms] | 1214.8 [1213.3, 1458.5] | 0.367 [0.355, 0.371] | DIFFER (4) |
| par_cohesive_extrinsic | step | np8 | 5 | 16 | 704.92 ms [680.83 ms, 711.38 ms] | 44.06 ms [42.55 ms, 44.46 ms] | 917.9 [886.5, 926.3] | 0.268 [0.234, 0.281] | DIFFER (4) |
| par_cohesive_extrinsic | check_insertion | np1 | 5 | 16 | 2.237 s [2.223 s, 2.656 s] | 139.80 ms [138.91 ms, 166.00 ms] | 2912.6 [2893.9, 3458.3] | – | DIFFER (4) |
| par_cohesive_extrinsic | check_insertion | np2 | 5 | 16 | 1.422 s [1.418 s, 1.691 s] | 88.86 ms [88.64 ms, 105.72 ms] | 1851.3 [1846.6, 2202.5] | 0.636 [0.619, 0.638] | DIFFER (4) |
| par_cohesive_extrinsic | check_insertion | np4 | 5 | 16 | 810.03 ms [807.50 ms, 974.44 ms] | 50.63 ms [50.47 ms, 60.90 ms] | 1054.7 [1051.4, 1268.8] | 0.363 [0.348, 0.367] | DIFFER (4) |
| par_cohesive_extrinsic | check_insertion | np8 | 5 | 16 | 579.84 ms [571.26 ms, 599.41 ms] | 36.24 ms [35.70 ms, 37.46 ms] | 755.0 [743.8, 780.5] | 0.258 [0.221, 0.268] | DIFFER (4) |
| par_cohesive_extrinsic | solve_step | np1 | 5 | 16 | 307.02 ms [305.99 ms, 359.27 ms] | 19.19 ms [19.12 ms, 22.45 ms] | 399.8 [398.4, 467.8] | – | DIFFER (4) |
| par_cohesive_extrinsic | solve_step | np2 | 5 | 16 | 191.21 ms [190.05 ms, 225.88 ms] | 11.95 ms [11.88 ms, 14.12 ms] | 249.0 [247.5, 294.1] | 0.625 [0.621, 0.633] | DIFFER (4) |
| par_cohesive_extrinsic | solve_step | np4 | 5 | 16 | 125.13 ms [123.56 ms, 146.75 ms] | 7.82 ms [7.72 ms, 9.17 ms] | 162.9 [160.9, 191.1] | 0.408 [0.402, 0.417] | DIFFER (4) |
| par_cohesive_extrinsic | solve_step | np8 | 5 | 16 | 118.25 ms [106.42 ms, 129.95 ms] | 7.39 ms [6.65 ms, 8.12 ms] | 154.0 [138.6, 169.2] | 0.366 [0.329, 0.425] | DIFFER (4) |
| par_explicit_tet4_ss | mesh | np1 | 5 | 1 | 6.80 ms [6.55 ms, 8.10 ms] | 6.80 ms [6.55 ms, 8.10 ms] | 42.0 [40.5, 50.0] | – | DIFFER (4) |
| par_explicit_tet4_ss | mesh | np2 | 5 | 1 | 6.77 ms [6.48 ms, 8.43 ms] | 6.77 ms [6.48 ms, 8.43 ms] | 41.8 [40.0, 52.1] | 0.998 [0.989, 1.042] | DIFFER (4) |
| par_explicit_tet4_ss | mesh | np4 | 5 | 1 | 6.96 ms [6.72 ms, 8.00 ms] | 6.96 ms [6.72 ms, 8.00 ms] | 43.0 [41.5, 49.4] | 0.999 [0.984, 1.074] | DIFFER (4) |
| par_explicit_tet4_ss | mesh | np8 | 5 | 1 | 7.92 ms [7.75 ms, 8.20 ms] | 7.92 ms [7.75 ms, 8.20 ms] | 48.9 [47.8, 50.6] | 1.165 [1.013, 1.198] | DIFFER (4) |
| par_explicit_tet4_ss | init | np1 | 5 | 1 | 129.04 ms [123.54 ms, 152.82 ms] | 129.04 ms [123.54 ms, 152.82 ms] | 796.5 [762.6, 943.3] | – | DIFFER (4) |
| par_explicit_tet4_ss | init | np2 | 5 | 1 | 113.01 ms [112.01 ms, 131.57 ms] | 113.01 ms [112.01 ms, 131.57 ms] | 697.6 [691.4, 812.2] | 0.876 [0.861, 0.907] | DIFFER (4) |
| par_explicit_tet4_ss | init | np4 | 5 | 1 | 63.06 ms [62.29 ms, 74.52 ms] | 63.06 ms [62.29 ms, 74.52 ms] | 389.2 [384.5, 460.0] | 0.489 [0.486, 0.512] | DIFFER (4) |
| par_explicit_tet4_ss | init | np8 | 5 | 1 | 37.97 ms [37.13 ms, 38.20 ms] | 37.97 ms [37.13 ms, 38.20 ms] | 234.4 [229.2, 235.8] | 0.294 [0.249, 0.305] | DIFFER (4) |
| par_explicit_tet4_ss | solve_step | np1 | 5 | 200 | 2.694 s [2.547 s, 3.206 s] | 13.47 ms [12.73 ms, 16.03 ms] | 83.1 [78.6, 99.0] | – | DIFFER (4) |
| par_explicit_tet4_ss | solve_step | np2 | 5 | 200 | 2.062 s [1.999 s, 2.304 s] | 10.31 ms [10.00 ms, 11.52 ms] | 63.6 [61.7, 71.1] | 0.751 [0.719, 0.898] | DIFFER (4) |
| par_explicit_tet4_ss | solve_step | np4 | 5 | 200 | 1.623 s [1.576 s, 1.923 s] | 8.11 ms [7.88 ms, 9.62 ms] | 50.1 [48.7, 59.4] | 0.591 [0.577, 0.755] | DIFFER (4) |
| par_explicit_tet4_ss | solve_step | np8 | 5 | 200 | 1.699 s [1.676 s, 1.839 s] | 8.50 ms [8.38 ms, 9.19 ms] | 52.4 [51.7, 56.7] | 0.622 [0.530, 0.720] | DIFFER (4) |
| par_explicit_tet4_ss | residual | np1 | 5 | 200 | 2.111 s [2.047 s, 2.499 s] | 10.55 ms [10.24 ms, 12.49 ms] | 65.1 [63.2, 77.1] | – | DIFFER (4) |
| par_explicit_tet4_ss | residual | np2 | 5 | 200 | 1.543 s [1.507 s, 1.759 s] | 7.71 ms [7.54 ms, 8.80 ms] | 47.6 [46.5, 54.3] | 0.718 [0.704, 0.845] | DIFFER (4) |
| par_explicit_tet4_ss | residual | np4 | 5 | 200 | 1.151 s [1.121 s, 1.370 s] | 5.76 ms [5.61 ms, 6.85 ms] | 35.5 [34.6, 42.3] | 0.539 [0.524, 0.669] | DIFFER (4) |
| par_explicit_tet4_ss | residual | np8 | 5 | 200 | 1.162 s [1.141 s, 1.246 s] | 5.81 ms [5.70 ms, 6.23 ms] | 35.9 [35.2, 38.4] | 0.542 [0.465, 0.608] | DIFFER (4) |
| par_explicit_tet4_ss | step_hooks | np1 | 5 | 400 | 418.89 ms [344.59 ms, 516.16 ms] | 1.05 ms [861.5 µs, 1.29 ms] | 6.5 [5.3, 8.0] | – | DIFFER (4) |
| par_explicit_tet4_ss | step_hooks | np2 | 5 | 400 | 382.52 ms [361.07 ms, 404.69 ms] | 956.3 µs [902.7 µs, 1.01 ms] | 5.9 [5.6, 6.2] | 0.905 [0.745, 1.174] | DIFFER (4) |
| par_explicit_tet4_ss | step_hooks | np4 | 5 | 400 | 356.31 ms [338.82 ms, 416.10 ms] | 890.8 µs [847.1 µs, 1.04 ms] | 5.5 [5.2, 6.4] | 0.823 [0.790, 1.208] | DIFFER (4) |
| par_explicit_tet4_ss | step_hooks | np8 | 5 | 400 | 396.51 ms [390.52 ms, 438.15 ms] | 991.3 µs [976.3 µs, 1.10 ms] | 6.1 [6.0, 6.8] | 0.950 [0.766, 1.265] | DIFFER (4) |
| par_explicit_tet4_ss | step_rest | np1 | 5 | 200 | 166.25 ms [154.56 ms, 190.78 ms] | 831.3 µs [772.8 µs, 953.9 µs] | 5.1 [4.8, 5.9] | – | DIFFER (4) |
| par_explicit_tet4_ss | step_rest | np2 | 5 | 200 | 163.52 ms [148.07 ms, 263.20 ms] | 817.6 µs [740.4 µs, 1.32 ms] | 5.0 [4.6, 8.1] | 0.967 [0.894, 1.703] | DIFFER (4) |
| par_explicit_tet4_ss | step_rest | np4 | 5 | 200 | 203.11 ms [167.87 ms, 347.26 ms] | 1.02 ms [839.4 µs, 1.74 ms] | 6.3 [5.2, 10.7] | 1.226 [1.001, 2.247] | DIFFER (4) |
| par_explicit_tet4_ss | step_rest | np8 | 5 | 200 | 214.33 ms [205.63 ms, 229.89 ms] | 1.07 ms [1.03 ms, 1.15 ms] | 6.6 [6.3, 7.1] | 1.286 [1.081, 1.487] | DIFFER (4) |
| par_explicit_tet4_ss | energies | np1 | 5 | 40 | 635.23 ms [633.97 ms, 747.42 ms] | 15.88 ms [15.85 ms, 18.69 ms] | 98.0 [97.8, 115.3] | – | DIFFER (4) |
| par_explicit_tet4_ss | energies | np2 | 5 | 40 | 353.66 ms [318.44 ms, 382.45 ms] | 8.84 ms [7.96 ms, 9.56 ms] | 54.6 [49.1, 59.0] | 0.512 [0.501, 0.558] | DIFFER (4) |
| par_explicit_tet4_ss | energies | np4 | 5 | 40 | 171.16 ms [169.71 ms, 207.77 ms] | 4.28 ms [4.24 ms, 5.19 ms] | 26.4 [26.2, 32.1] | 0.270 [0.267, 0.309] | DIFFER (4) |
| par_explicit_tet4_ss | energies | np8 | 5 | 40 | 118.41 ms [115.04 ms, 121.39 ms] | 2.96 ms [2.88 ms, 3.03 ms] | 18.3 [17.8, 18.7] | 0.183 [0.158, 0.191] | DIFFER (4) |
| par_cohesive_extrinsic | distribute | np2 | 5 | 1 | 35.17 ms [34.99 ms, 41.44 ms] | 35.17 ms [34.99 ms, 41.44 ms] | 732.8 [729.0, 863.4] | – | DIFFER (4) |
| par_cohesive_extrinsic | distribute | np4 | 5 | 1 | 39.70 ms [38.75 ms, 46.49 ms] | 39.70 ms [38.75 ms, 46.49 ms] | 827.1 [807.3, 968.6] | – | DIFFER (4) |
| par_cohesive_extrinsic | distribute | np8 | 5 | 1 | 50.79 ms [50.45 ms, 52.01 ms] | 50.79 ms [50.45 ms, 52.01 ms] | 1058.2 [1051.0, 1083.5] | – | DIFFER (4) |
| par_explicit_tet4_ss | distribute | np2 | 5 | 1 | 120.10 ms [118.50 ms, 141.95 ms] | 120.10 ms [118.50 ms, 141.95 ms] | 741.4 [731.5, 876.3] | – | DIFFER (4) |
| par_explicit_tet4_ss | distribute | np4 | 5 | 1 | 129.50 ms [128.49 ms, 152.71 ms] | 129.50 ms [128.49 ms, 152.71 ms] | 799.4 [793.2, 942.7] | – | DIFFER (4) |
| par_explicit_tet4_ss | distribute | np8 | 5 | 1 | 162.09 ms [158.88 ms, 166.02 ms] | 162.09 ms [158.88 ms, 166.02 ms] | 1000.5 [980.8, 1024.8] | – | DIFFER (4) |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| par_cohesive_extrinsic | 48000 | 17302 | 51906 | 1 | c698395e8 | 203b431fc4a2e291, 58b8a394b0fd24e1, a7683725b187d73b, f853532025302f95 |
| par_explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | c698395e8 | 3cb113542b3d2bcc, 8aa150dea45d3552, 90232d46c403f8c1, ae8176278040021f |

## static_linear_mumps (`parallel/mumps/`)

Suite `solid`, size `m`; variants: `np1`, `np2`, `np4`; reference `np1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to np1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_mumps | mesh | np1 | 5 | 1 | 882.3 µs [861.0 µs, 1.02 ms] | 882.3 µs [861.0 µs, 1.02 ms] | 53.6 [52.3, 61.7] | – | DIFFER (9) |
| static_linear_mumps | mesh | np2 | 5 | 1 | 896.2 µs [886.0 µs, 1.03 ms] | 896.2 µs [886.0 µs, 1.03 ms] | 54.4 [53.8, 62.7] | 1.017 [1.001, 1.029] | DIFFER (9) |
| static_linear_mumps | mesh | np4 | 5 | 1 | 899.6 µs [877.1 µs, 1.04 ms] | 899.6 µs [877.1 µs, 1.04 ms] | 54.6 [53.3, 63.3] | 1.019 [1.006, 1.027] | DIFFER (9) |
| static_linear_mumps | init | np1 | 5 | 1 | 13.23 ms [13.13 ms, 15.26 ms] | 13.23 ms [13.13 ms, 15.26 ms] | 803.4 [797.5, 927.1] | – | DIFFER (9) |
| static_linear_mumps | init | np2 | 5 | 1 | 9.15 ms [9.12 ms, 10.72 ms] | 9.15 ms [9.12 ms, 10.72 ms] | 555.5 [553.7, 651.4] | 0.693 [0.675, 0.703] | DIFFER (9) |
| static_linear_mumps | init | np4 | 5 | 1 | 5.65 ms [5.59 ms, 6.42 ms] | 5.65 ms [5.59 ms, 6.42 ms] | 343.1 [339.6, 390.1] | 0.425 [0.411, 0.440] | DIFFER (9) |
| static_linear_mumps | solve_step | np1 | 5 | 1 | 330.89 ms [329.19 ms, 393.78 ms] | 330.89 ms [329.19 ms, 393.78 ms] | 20097.9 [19994.8, 23917.8] | – | DIFFER (9) |
| static_linear_mumps | solve_step | np2 | 5 | 1 | 213.16 ms [208.97 ms, 243.36 ms] | 213.16 ms [208.97 ms, 243.36 ms] | 12947.2 [12692.8, 14781.6] | 0.635 [0.602, 0.644] | DIFFER (9) |
| static_linear_mumps | solve_step | np4 | 5 | 1 | 133.31 ms [132.07 ms, 156.13 ms] | 133.31 ms [132.07 ms, 156.13 ms] | 8097.3 [8021.5, 9483.4] | 0.399 [0.374, 0.405] | DIFFER (9) |
| static_linear_mumps | matrix | np1 | 5 | 1 | 155.70 ms [154.10 ms, 189.87 ms] | 155.70 ms [154.10 ms, 189.87 ms] | 9457.3 [9360.0, 11532.6] | – | DIFFER (9) |
| static_linear_mumps | matrix | np2 | 5 | 1 | 76.51 ms [75.06 ms, 87.05 ms] | 76.51 ms [75.06 ms, 87.05 ms] | 4646.8 [4559.1, 5287.3] | 0.483 [0.458, 0.495] | DIFFER (9) |
| static_linear_mumps | matrix | np4 | 5 | 1 | 37.94 ms [37.26 ms, 43.51 ms] | 37.94 ms [37.26 ms, 43.51 ms] | 2304.3 [2263.3, 2642.8] | 0.242 [0.225, 0.244] | DIFFER (9) |
| static_linear_mumps | step_rest | np1 | 5 | 1 | 176.20 ms [174.63 ms, 203.77 ms] | 176.20 ms [174.63 ms, 203.77 ms] | 10702.2 [10606.6, 12376.8] | – | DIFFER (9) |
| static_linear_mumps | step_rest | np2 | 5 | 1 | 138.77 ms [134.75 ms, 158.05 ms] | 138.77 ms [134.75 ms, 158.05 ms] | 8428.7 [8184.7, 9599.7] | 0.776 [0.736, 0.788] | DIFFER (9) |
| static_linear_mumps | step_rest | np4 | 5 | 1 | 96.53 ms [94.72 ms, 112.94 ms] | 96.53 ms [94.72 ms, 112.94 ms] | 5863.0 [5753.3, 6859.9] | 0.548 [0.523, 0.554] | DIFFER (9) |
| static_linear_mumps | distribute | np2 | 5 | 1 | 12.30 ms [12.24 ms, 14.15 ms] | 12.30 ms [12.24 ms, 14.15 ms] | 747.1 [743.7, 859.3] | – | DIFFER (9) |
| static_linear_mumps | distribute | np4 | 5 | 1 | 13.94 ms [13.91 ms, 16.00 ms] | 13.94 ms [13.91 ms, 16.00 ms] | 846.4 [844.6, 971.5] | – | DIFFER (9) |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_mumps | 16464 | 3375 | 10125 | 1 | c698395e8 | 02aec38352c4797f, 3bfcebfaa224911a, 3f68725214d0507b, 73bc0ea3f332cd47, 82c32a5fa5cf8f5b, 8d9cfcf5dd5e5c06, 9193a42d69fbf778, 9ae698ce201e67f6, abdd878a7043f3ed |
