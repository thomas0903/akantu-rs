# suite_solid baseline, size m

Commit `c698395e8`, AWS `m7a.2xlarge` (AMD EPYC 9R14), core 2, 2026-11-16 07:12–10:19 UTC, 5 repetitions (one process per case and repetition). Command and raw data: `../README.md`. step_hooks and other sub-phases are per callback call (two hook calls per step).

## Main phases

| case | phase | reps | calls | per call, median [min, max] | ns per element and call |
|---|---|---|---|---|---|
| cohesive_extrinsic_check | solve_step | 5 | 100 | 12.01 ms [11.98 ms, 12.28 ms] | 144.8 |
| cohesive_extrinsic_check | check_insertion | 5 | 100 | 49.26 ms [49.20 ms, 50.26 ms] | 594.0 |
| cohesive_extrinsic_check | init | 5 | 1 | 1.159 s [1.158 s, 1.166 s] | 13974.8 |
| cohesive_extrinsic_insertion | solve_step | 5 | 16 | 18.95 ms [18.88 ms, 19.45 ms] | 394.8 |
| cohesive_extrinsic_insertion | step | 5 | 16 | 158.42 ms [158.25 ms, 170.12 ms] | 3300.4 |
| cohesive_extrinsic_insertion | check_insertion | 5 | 16 | 139.48 ms [139.32 ms, 150.63 ms] | 2905.7 |
| cohesive_extrinsic_insertion | init | 5 | 1 | 662.14 ms [661.93 ms, 666.46 ms] | 13794.6 |
| cohesive_extrinsic_insertion | preload | 5 | 1 | 2.176 s [2.172 s, 2.216 s] | 45331.7 |
| cohesive_intrinsic_bilinear | solve_step | 5 | 300 | 21.18 ms [21.10 ms, 21.57 ms] | 441.2 |
| cohesive_intrinsic_bilinear | energies | 5 | 60 | 6.64 ms [6.62 ms, 6.72 ms] | 138.3 |
| cohesive_intrinsic_bilinear | init | 5 | 1 | 1.063 s [1.061 s, 1.070 s] | 22136.5 |
| cohesive_intrinsic_exponential | solve_step | 5 | 300 | 27.18 ms [26.98 ms, 27.37 ms] | 566.3 |
| cohesive_intrinsic_exponential | energies | 5 | 60 | 6.63 ms [6.62 ms, 6.80 ms] | 138.0 |
| cohesive_intrinsic_exponential | init | 5 | 1 | 1.066 s [1.064 s, 1.084 s] | 22218.5 |
| cohesive_intrinsic_implicit | solve_step | 5 | 30 | 329.04 ms [326.75 ms, 339.53 ms] | 91399.3 |
| cohesive_intrinsic_implicit | init | 5 | 1 | 14.38 ms [14.29 ms, 14.52 ms] | 3993.2 |
| explicit_hex8_fd | solve_step | 5 | 30 | 120.48 ms [120.26 ms, 121.58 ms] | 1089.4 |
| explicit_hex8_fd | residual | 5 | 30 | 100.85 ms [100.49 ms, 101.56 ms] | 911.9 |
| explicit_hex8_fd | step_hooks | 5 | 60 | 8.11 ms [8.02 ms, 8.18 ms] | 73.3 |
| explicit_hex8_fd | step_rest | 5 | 30 | 3.60 ms [3.59 ms, 3.67 ms] | 32.6 |
| explicit_hex8_fd | internal_forces | 5 | 6 | 99.96 ms [99.35 ms, 100.67 ms] | 903.8 |
| explicit_hex8_fd | stresses | 5 | 6 | 27.30 ms [26.94 ms, 27.54 ms] | 246.9 |
| explicit_hex8_fd | energies | 5 | 6 | 114.91 ms [113.86 ms, 115.99 ms] | 1039.1 |
| explicit_hex8_fd | init | 5 | 1 | 970.96 ms [964.06 ms, 978.50 ms] | 8779.6 |
| explicit_hex8_ss | solve_step | 5 | 50 | 67.60 ms [66.78 ms, 68.59 ms] | 611.2 |
| explicit_hex8_ss | residual | 5 | 50 | 52.59 ms [51.82 ms, 53.36 ms] | 475.5 |
| explicit_hex8_ss | step_hooks | 5 | 100 | 5.70 ms [5.66 ms, 5.77 ms] | 51.5 |
| explicit_hex8_ss | step_rest | 5 | 50 | 3.60 ms [3.50 ms, 3.67 ms] | 32.6 |
| explicit_hex8_ss | internal_forces | 5 | 10 | 51.86 ms [50.39 ms, 52.10 ms] | 468.9 |
| explicit_hex8_ss | stresses | 5 | 10 | 22.29 ms [22.00 ms, 22.90 ms] | 201.5 |
| explicit_hex8_ss | energies | 5 | 10 | 115.23 ms [112.73 ms, 115.95 ms] | 1041.9 |
| explicit_hex8_ss | init | 5 | 1 | 879.14 ms [874.24 ms, 882.66 ms] | 7949.4 |
| explicit_marigo | solve_step | 5 | 200 | 24.11 ms [23.90 ms, 24.38 ms] | 228.6 |
| explicit_marigo | residual | 5 | 200 | 13.70 ms [13.57 ms, 13.82 ms] | 129.9 |
| explicit_marigo | step_hooks | 5 | 400 | 4.86 ms [4.82 ms, 4.98 ms] | 46.1 |
| explicit_marigo | step_rest | 5 | 200 | 0.64 ms [0.64 ms, 0.65 ms] | 6.1 |
| explicit_marigo | internal_forces | 5 | 40 | 13.26 ms [13.15 ms, 14.06 ms] | 125.7 |
| explicit_marigo | stresses | 5 | 40 | 9.02 ms [8.97 ms, 9.77 ms] | 85.5 |
| explicit_marigo | energies | 5 | 40 | 7.31 ms [7.31 ms, 7.36 ms] | 69.3 |
| explicit_marigo | init | 5 | 1 | 87.82 ms [87.26 ms, 88.18 ms] | 832.8 |
| explicit_maxwell | solve_step | 5 | 40 | 173.32 ms [171.66 ms, 175.09 ms] | 1643.5 |
| explicit_maxwell | residual | 5 | 40 | 45.19 ms [45.15 ms, 45.53 ms] | 428.5 |
| explicit_maxwell | step_hooks | 5 | 80 | 63.72 ms [62.91 ms, 64.44 ms] | 604.3 |
| explicit_maxwell | step_rest | 5 | 40 | 0.66 ms [0.64 ms, 0.67 ms] | 6.3 |
| explicit_maxwell | internal_forces | 5 | 8 | 41.59 ms [41.50 ms, 43.34 ms] | 394.4 |
| explicit_maxwell | stresses | 5 | 8 | 37.67 ms [37.61 ms, 39.56 ms] | 357.2 |
| explicit_maxwell | energies | 5 | 8 | 5.51 ms [5.51 ms, 5.61 ms] | 52.3 |
| explicit_maxwell | init | 5 | 1 | 98.94 ms [98.81 ms, 100.06 ms] | 938.2 |
| explicit_mazars | solve_step | 5 | 80 | 91.02 ms [89.84 ms, 96.60 ms] | 863.1 |
| explicit_mazars | residual | 5 | 80 | 80.27 ms [79.41 ms, 86.23 ms] | 761.2 |
| explicit_mazars | step_hooks | 5 | 160 | 4.86 ms [4.81 ms, 5.04 ms] | 46.1 |
| explicit_mazars | step_rest | 5 | 80 | 0.64 ms [0.64 ms, 0.66 ms] | 6.1 |
| explicit_mazars | internal_forces | 5 | 16 | 86.24 ms [85.09 ms, 95.41 ms] | 817.8 |
| explicit_mazars | stresses | 5 | 16 | 81.49 ms [80.71 ms, 92.72 ms] | 772.7 |
| explicit_mazars | energies | 5 | 16 | 6.65 ms [6.64 ms, 6.70 ms] | 63.1 |
| explicit_mazars | init | 5 | 1 | 87.12 ms [86.94 ms, 88.14 ms] | 826.2 |
| explicit_mazars_non_local | solve_step | 5 | 40 | 190.33 ms [188.65 ms, 195.02 ms] | 3965.3 |
| explicit_mazars_non_local | residual | 5 | 40 | 185.73 ms [184.06 ms, 190.04 ms] | 3869.4 |
| explicit_mazars_non_local | step_hooks | 5 | 80 | 2.13 ms [2.12 ms, 2.27 ms] | 44.3 |
| explicit_mazars_non_local | step_rest | 5 | 40 | 0.34 ms [0.33 ms, 0.42 ms] | 7.1 |
| explicit_mazars_non_local | internal_forces | 5 | 8 | 189.12 ms [188.69 ms, 201.38 ms] | 3939.9 |
| explicit_mazars_non_local | stresses | 5 | 8 | 30.42 ms [30.18 ms, 33.44 ms] | 633.7 |
| explicit_mazars_non_local | energies | 5 | 8 | 4.05 ms [4.04 ms, 4.05 ms] | 84.3 |
| explicit_mazars_non_local | init | 5 | 1 | 905.51 ms [900.47 ms, 907.25 ms] | 18864.8 |
| explicit_mazars_non_local | nonlocal_rebuild | 5 | 2 | 844.37 ms [843.16 ms, 958.31 ms] | 17591.1 |
| explicit_multi_material | solve_step | 5 | 120 | 46.64 ms [46.00 ms, 48.79 ms] | 442.3 |
| explicit_multi_material | residual | 5 | 120 | 40.09 ms [39.54 ms, 41.67 ms] | 380.2 |
| explicit_multi_material | step_hooks | 5 | 240 | 2.93 ms [2.92 ms, 3.19 ms] | 27.8 |
| explicit_multi_material | step_rest | 5 | 120 | 0.63 ms [0.62 ms, 0.72 ms] | 6.0 |
| explicit_multi_material | internal_forces | 5 | 24 | 41.03 ms [40.43 ms, 44.27 ms] | 389.1 |
| explicit_multi_material | stresses | 5 | 24 | 36.89 ms [35.85 ms, 37.68 ms] | 349.8 |
| explicit_multi_material | energies | 5 | 24 | 8.60 ms [8.59 ms, 8.67 ms] | 81.5 |
| explicit_multi_material | init | 5 | 1 | 89.09 ms [89.00 ms, 90.46 ms] | 844.8 |
| explicit_neohookean | solve_step | 5 | 100 | 30.35 ms [29.87 ms, 31.73 ms] | 287.8 |
| explicit_neohookean | residual | 5 | 100 | 27.87 ms [27.41 ms, 28.51 ms] | 264.3 |
| explicit_neohookean | step_hooks | 5 | 200 | 0.96 ms [0.94 ms, 1.29 ms] | 9.1 |
| explicit_neohookean | step_rest | 5 | 100 | 0.56 ms [0.56 ms, 0.64 ms] | 5.4 |
| explicit_neohookean | internal_forces | 5 | 20 | 27.62 ms [27.47 ms, 28.56 ms] | 261.9 |
| explicit_neohookean | stresses | 5 | 20 | 21.10 ms [21.00 ms, 21.49 ms] | 200.1 |
| explicit_neohookean | energies | 5 | 20 | 26.50 ms [26.30 ms, 27.95 ms] | 251.3 |
| explicit_neohookean | init | 5 | 1 | 93.06 ms [92.11 ms, 93.50 ms] | 882.5 |
| explicit_orthotropic | solve_step | 5 | 200 | 13.41 ms [13.37 ms, 15.06 ms] | 127.2 |
| explicit_orthotropic | residual | 5 | 200 | 11.71 ms [11.66 ms, 12.60 ms] | 111.0 |
| explicit_orthotropic | step_hooks | 5 | 400 | 0.58 ms [0.58 ms, 0.89 ms] | 5.5 |
| explicit_orthotropic | step_rest | 5 | 200 | 0.55 ms [0.54 ms, 0.68 ms] | 5.2 |
| explicit_orthotropic | internal_forces | 5 | 40 | 11.43 ms [11.41 ms, 12.64 ms] | 108.4 |
| explicit_orthotropic | stresses | 5 | 40 | 7.44 ms [7.43 ms, 8.05 ms] | 70.6 |
| explicit_orthotropic | energies | 5 | 40 | 9.56 ms [9.55 ms, 10.32 ms] | 90.7 |
| explicit_orthotropic | init | 5 | 1 | 81.92 ms [81.39 ms, 82.88 ms] | 776.9 |
| explicit_plastic | solve_step | 5 | 120 | 56.44 ms [56.23 ms, 57.24 ms] | 535.2 |
| explicit_plastic | residual | 5 | 120 | 49.93 ms [49.86 ms, 50.66 ms] | 473.5 |
| explicit_plastic | step_hooks | 5 | 240 | 2.93 ms [2.87 ms, 2.96 ms] | 27.7 |
| explicit_plastic | step_rest | 5 | 120 | 0.64 ms [0.63 ms, 0.65 ms] | 6.0 |
| explicit_plastic | internal_forces | 5 | 24 | 49.93 ms [49.87 ms, 51.09 ms] | 473.5 |
| explicit_plastic | stresses | 5 | 24 | 45.86 ms [45.74 ms, 47.11 ms] | 434.9 |
| explicit_plastic | energies | 5 | 24 | 9.24 ms [9.23 ms, 9.33 ms] | 87.6 |
| explicit_plastic | init | 5 | 1 | 90.96 ms [90.33 ms, 93.92 ms] | 862.6 |
| explicit_sls | solve_step | 5 | 120 | 45.08 ms [44.95 ms, 45.54 ms] | 427.4 |
| explicit_sls | residual | 5 | 120 | 41.34 ms [41.21 ms, 41.73 ms] | 392.0 |
| explicit_sls | step_hooks | 5 | 240 | 1.57 ms [1.54 ms, 1.58 ms] | 14.9 |
| explicit_sls | step_rest | 5 | 120 | 0.64 ms [0.63 ms, 0.64 ms] | 6.1 |
| explicit_sls | internal_forces | 5 | 24 | 41.23 ms [40.98 ms, 42.09 ms] | 391.0 |
| explicit_sls | stresses | 5 | 24 | 37.00 ms [36.45 ms, 38.25 ms] | 350.8 |
| explicit_sls | energies | 5 | 24 | 13.98 ms [13.98 ms, 14.04 ms] | 132.6 |
| explicit_sls | init | 5 | 1 | 100.45 ms [99.67 ms, 100.79 ms] | 952.6 |
| explicit_tet10_fd | solve_step | 5 | 40 | 74.80 ms [74.68 ms, 75.69 ms] | 709.3 |
| explicit_tet10_fd | residual | 5 | 40 | 62.37 ms [62.30 ms, 63.04 ms] | 591.4 |
| explicit_tet10_fd | step_hooks | 5 | 80 | 3.86 ms [3.84 ms, 3.92 ms] | 36.6 |
| explicit_tet10_fd | step_rest | 5 | 40 | 4.80 ms [4.51 ms, 4.84 ms] | 45.5 |
| explicit_tet10_fd | internal_forces | 5 | 8 | 60.87 ms [60.76 ms, 61.70 ms] | 577.2 |
| explicit_tet10_fd | stresses | 5 | 8 | 15.68 ms [15.63 ms, 16.10 ms] | 148.6 |
| explicit_tet10_fd | energies | 5 | 8 | 67.61 ms [66.41 ms, 68.44 ms] | 641.2 |
| explicit_tet10_fd | init | 5 | 1 | 578.87 ms [575.59 ms, 579.91 ms] | 5489.2 |
| explicit_tet10_ss | solve_step | 5 | 50 | 45.14 ms [42.73 ms, 46.19 ms] | 428.0 |
| explicit_tet10_ss | residual | 5 | 50 | 34.78 ms [33.33 ms, 35.24 ms] | 329.8 |
| explicit_tet10_ss | step_hooks | 5 | 100 | 2.78 ms [2.51 ms, 2.98 ms] | 26.4 |
| explicit_tet10_ss | step_rest | 5 | 50 | 4.80 ms [4.38 ms, 4.98 ms] | 45.6 |
| explicit_tet10_ss | internal_forces | 5 | 10 | 33.34 ms [32.22 ms, 33.81 ms] | 316.2 |
| explicit_tet10_ss | stresses | 5 | 10 | 13.35 ms [13.30 ms, 14.52 ms] | 126.6 |
| explicit_tet10_ss | energies | 5 | 10 | 67.42 ms [66.32 ms, 68.14 ms] | 639.3 |
| explicit_tet10_ss | init | 5 | 1 | 540.27 ms [528.40 ms, 544.43 ms] | 5123.2 |
| explicit_tet4_fd | solve_step | 5 | 150 | 19.45 ms [19.35 ms, 20.41 ms] | 120.0 |
| explicit_tet4_fd | residual | 5 | 150 | 15.35 ms [15.34 ms, 15.72 ms] | 94.7 |
| explicit_tet4_fd | step_hooks | 5 | 300 | 1.63 ms [1.59 ms, 1.87 ms] | 10.1 |
| explicit_tet4_fd | step_rest | 5 | 150 | 0.84 ms [0.82 ms, 0.95 ms] | 5.2 |
| explicit_tet4_fd | internal_forces | 5 | 30 | 15.06 ms [15.03 ms, 15.39 ms] | 93.0 |
| explicit_tet4_fd | stresses | 5 | 30 | 5.31 ms [5.29 ms, 5.41 ms] | 32.8 |
| explicit_tet4_fd | energies | 5 | 30 | 16.15 ms [15.96 ms, 16.75 ms] | 99.7 |
| explicit_tet4_fd | init | 5 | 1 | 142.49 ms [141.12 ms, 144.77 ms] | 879.6 |
| explicit_tet4_ss | solve_step | 5 | 200 | 13.76 ms [13.59 ms, 14.68 ms] | 84.9 |
| explicit_tet4_ss | residual | 5 | 200 | 10.75 ms [10.69 ms, 11.21 ms] | 66.4 |
| explicit_tet4_ss | step_hooks | 5 | 400 | 1.08 ms [1.04 ms, 1.27 ms] | 6.7 |
| explicit_tet4_ss | step_rest | 5 | 200 | 0.84 ms [0.82 ms, 0.92 ms] | 5.2 |
| explicit_tet4_ss | internal_forces | 5 | 40 | 10.37 ms [10.33 ms, 10.76 ms] | 64.0 |
| explicit_tet4_ss | stresses | 5 | 40 | 4.40 ms [4.38 ms, 4.51 ms] | 27.1 |
| explicit_tet4_ss | energies | 5 | 40 | 16.02 ms [15.81 ms, 16.53 ms] | 98.9 |
| explicit_tet4_ss | init | 5 | 1 | 128.05 ms [126.66 ms, 130.14 ms] | 790.4 |
| explicit_thermal | solve_step | 5 | 300 | 8.60 ms [8.56 ms, 9.67 ms] | 81.6 |
| explicit_thermal | residual | 5 | 300 | 6.77 ms [6.77 ms, 7.27 ms] | 64.2 |
| explicit_thermal | step_hooks | 5 | 600 | 0.65 ms [0.63 ms, 0.87 ms] | 6.1 |
| explicit_thermal | step_rest | 5 | 300 | 0.54 ms [0.54 ms, 0.65 ms] | 5.2 |
| explicit_thermal | internal_forces | 5 | 60 | 6.54 ms [6.53 ms, 7.05 ms] | 62.0 |
| explicit_thermal | stresses | 5 | 60 | 2.80 ms [2.79 ms, 2.86 ms] | 26.6 |
| explicit_thermal | energies | 5 | 60 | 10.37 ms [10.35 ms, 11.24 ms] | 98.4 |
| explicit_thermal | init | 5 | 1 | 83.85 ms [83.14 ms, 84.75 ms] | 795.1 |
| explicit_tri3_fd | solve_step | 5 | 200 | 12.69 ms [12.62 ms, 13.31 ms] | 70.5 |
| explicit_tri3_fd | residual | 5 | 200 | 9.38 ms [9.36 ms, 9.54 ms] | 52.1 |
| explicit_tri3_fd | step_hooks | 5 | 400 | 0.85 ms [0.80 ms, 0.98 ms] | 4.7 |
| explicit_tri3_fd | step_rest | 5 | 200 | 1.64 ms [1.60 ms, 1.82 ms] | 9.1 |
| explicit_tri3_fd | internal_forces | 5 | 40 | 8.62 ms [8.56 ms, 8.79 ms] | 47.9 |
| explicit_tri3_fd | stresses | 5 | 40 | 2.53 ms [2.50 ms, 2.59 ms] | 14.1 |
| explicit_tri3_fd | energies | 5 | 40 | 13.68 ms [13.42 ms, 14.00 ms] | 76.0 |
| explicit_tri3_fd | init | 5 | 1 | 105.80 ms [104.65 ms, 107.44 ms] | 587.8 |
| explicit_tri3_ss | solve_step | 5 | 300 | 11.46 ms [11.36 ms, 11.98 ms] | 63.7 |
| explicit_tri3_ss | residual | 5 | 300 | 8.71 ms [8.63 ms, 8.84 ms] | 48.4 |
| explicit_tri3_ss | step_hooks | 5 | 600 | 0.58 ms [0.55 ms, 0.67 ms] | 3.2 |
| explicit_tri3_ss | step_rest | 5 | 300 | 1.65 ms [1.59 ms, 1.82 ms] | 9.2 |
| explicit_tri3_ss | internal_forces | 5 | 60 | 7.85 ms [7.82 ms, 8.08 ms] | 43.6 |
| explicit_tri3_ss | stresses | 5 | 60 | 2.41 ms [2.38 ms, 2.52 ms] | 13.4 |
| explicit_tri3_ss | energies | 5 | 60 | 13.42 ms [13.26 ms, 13.79 ms] | 74.6 |
| explicit_tri3_ss | init | 5 | 1 | 98.69 ms [97.50 ms, 99.63 ms] | 548.3 |
| implicit_dynamic | solve_step | 5 | 60 | 45.74 ms [45.10 ms, 49.28 ms] | 2778.3 |
| implicit_dynamic | residual | 5 | 180 | 1.05 ms [1.05 ms, 1.14 ms] | 64.0 |
| implicit_dynamic | step_hooks | 5 | 120 | 0.10 ms [0.10 ms, 0.15 ms] | 6.2 |
| implicit_dynamic | step_rest | 5 | 60 | 42.36 ms [41.75 ms, 45.56 ms] | 2573.0 |
| implicit_dynamic | mass_consistent | 5 | 1 | 223.93 ms [221.73 ms, 232.72 ms] | 13601.4 |
| implicit_dynamic | init | 5 | 1 | 13.38 ms [13.35 ms, 13.40 ms] | 812.6 |
| setup_hex8 | stiffness_assembly | 5 | 3 | 1.036 s [1.033 s, 1.056 s] | 74971.3 |
| setup_hex8 | mass_consistent | 5 | 3 | 2.310 s [2.300 s, 2.317 s] | 167135.1 |
| setup_hex8 | mass_lumped | 5 | 3 | 10.17 ms [10.14 ms, 10.36 ms] | 735.5 |
| setup_hex8 | shape_functions | 5 | 3 | 69.45 ms [69.39 ms, 69.70 ms] | 5023.9 |
| setup_hex8 | init_explicit | 5 | 1 | 109.37 ms [109.00 ms, 109.66 ms] | 7911.4 |
| setup_hex8 | init_implicit | 5 | 1 | 107.12 ms [106.45 ms, 107.71 ms] | 7748.8 |
| setup_tet10 | stiffness_assembly | 5 | 3 | 798.66 ms [795.44 ms, 808.89 ms] | 77031.1 |
| setup_tet10 | mass_consistent | 5 | 3 | 1.681 s [1.671 s, 1.762 s] | 162146.7 |
| setup_tet10 | mass_lumped | 5 | 3 | 1.21 ms [1.20 ms, 1.21 ms] | 116.3 |
| setup_tet10 | shape_functions | 5 | 3 | 32.43 ms [32.40 ms, 35.58 ms] | 3127.7 |
| setup_tet10 | init_explicit | 5 | 1 | 52.95 ms [52.77 ms, 53.17 ms] | 5106.9 |
| setup_tet10 | init_implicit | 5 | 1 | 51.23 ms [51.13 ms, 51.73 ms] | 4941.3 |
| setup_tet4 | stiffness_assembly | 5 | 3 | 1.058 s [1.048 s, 1.070 s] | 6531.9 |
| setup_tet4 | mass_consistent | 5 | 3 | 1.716 s [1.714 s, 1.720 s] | 10589.7 |
| setup_tet4 | mass_lumped | 5 | 3 | 33.14 ms [33.07 ms, 33.99 ms] | 204.6 |
| setup_tet4 | shape_functions | 5 | 3 | 69.77 ms [69.64 ms, 73.00 ms] | 430.7 |
| setup_tet4 | init_explicit | 5 | 1 | 126.65 ms [125.95 ms, 127.76 ms] | 781.8 |
| setup_tet4 | init_implicit | 5 | 1 | 121.13 ms [120.68 ms, 122.69 ms] | 747.7 |
| static_linear_eigen | solve_step | 5 | 1 | 3.071 s [3.065 s, 3.381 s] | 186518.6 |
| static_linear_eigen | step_rest | 5 | 1 | 2.917 s [2.911 s, 3.206 s] | 177187.6 |
| static_linear_eigen | matrix | 5 | 1 | 153.81 ms [153.34 ms, 174.97 ms] | 9342.1 |
| static_linear_eigen | factorize | 5 | 2 | 2.771 s [2.767 s, 3.067 s] | 168289.9 |
| static_linear_eigen | back_substitution | 5 | 20 | 10.55 ms [10.29 ms, 12.97 ms] | 641.0 |
| static_linear_eigen | analysis | 5 | 1 | 42.52 ms [42.30 ms, 42.70 ms] | 2582.6 |
| static_linear_eigen | stiffness_assembly | 5 | 3 | 63.12 ms [62.77 ms, 64.05 ms] | 3833.8 |
| static_linear_eigen | init | 5 | 1 | 13.30 ms [13.28 ms, 13.66 ms] | 808.1 |
| static_linear_faer | solve_step | 5 | 1 | 5.036 s [4.997 s, 6.398 s] | 305898.9 |
| static_linear_faer | step_rest | 5 | 1 | 4.882 s [4.844 s, 6.203 s] | 296550.9 |
| static_linear_faer | matrix | 5 | 1 | 153.78 ms [153.13 ms, 195.09 ms] | 9340.5 |
| static_linear_faer | factorize | 5 | 2 | 4.726 s [4.713 s, 5.205 s] | 287066.6 |
| static_linear_faer | back_substitution | 5 | 20 | 10.64 ms [10.61 ms, 10.73 ms] | 646.1 |
| static_linear_faer | analysis | 5 | 1 | 50.05 ms [49.78 ms, 52.06 ms] | 3040.0 |
| static_linear_faer | stiffness_assembly | 5 | 3 | 83.71 ms [82.56 ms, 91.02 ms] | 5084.5 |
| static_linear_faer | init | 5 | 1 | 13.36 ms [13.32 ms, 13.59 ms] | 811.5 |
| static_newton_plastic | solve_step | 5 | 12 | 255.27 ms [254.57 ms, 310.59 ms] | 42545.3 |
| static_newton_plastic | residual | 5 | 961 | 1.21 ms [1.20 ms, 1.30 ms] | 200.9 |
| static_newton_plastic | step_hooks | 5 | 24 | 0.13 ms [0.13 ms, 0.14 ms] | 21.9 |
| static_newton_plastic | step_rest | 5 | 12 | 158.59 ms [157.76 ms, 206.40 ms] | 26432.2 |
| static_newton_plastic | init | 5 | 1 | 5.79 ms [5.75 ms, 5.82 ms] | 964.8 |

## Every phase (scripts/perf/suite/summarize.py)

Suite `solid`, size `m`; variants: `base`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | mesh | base | 5 | 1 | 3.58 ms [3.51 ms, 3.60 ms] | 3.58 ms [3.51 ms, 3.60 ms] | 43.1 [42.4, 43.3] | same |
| cohesive_extrinsic_check | init | base | 5 | 1 | 1.159 s [1.158 s, 1.166 s] | 1.159 s [1.158 s, 1.166 s] | 13974.8 [13956.7, 14056.2] | same |
| cohesive_extrinsic_check | check_insertion | base | 5 | 100 | 4.926 s [4.920 s, 5.026 s] | 49.26 ms [49.20 ms, 50.26 ms] | 594.0 [593.2, 605.9] | same |
| cohesive_extrinsic_check | solve_step | base | 5 | 100 | 1.201 s [1.198 s, 1.228 s] | 12.01 ms [11.98 ms, 12.28 ms] | 144.8 [144.4, 148.0] | same |
| cohesive_extrinsic_insertion | mesh | base | 5 | 1 | 2.20 ms [2.15 ms, 2.22 ms] | 2.20 ms [2.15 ms, 2.22 ms] | 45.9 [44.8, 46.3] | same |
| cohesive_extrinsic_insertion | init | base | 5 | 1 | 662.14 ms [661.93 ms, 666.46 ms] | 662.14 ms [661.93 ms, 666.46 ms] | 13794.6 [13790.2, 13884.6] | same |
| cohesive_extrinsic_insertion | preload | base | 5 | 1 | 2.176 s [2.172 s, 2.216 s] | 2.176 s [2.172 s, 2.216 s] | 45331.7 [45254.8, 46165.9] | same |
| cohesive_extrinsic_insertion | step | base | 5 | 16 | 2.535 s [2.532 s, 2.722 s] | 158.42 ms [158.25 ms, 170.12 ms] | 3300.4 [3296.9, 3544.2] | same |
| cohesive_extrinsic_insertion | check_insertion | base | 5 | 16 | 2.232 s [2.229 s, 2.410 s] | 139.48 ms [139.32 ms, 150.63 ms] | 2905.7 [2902.4, 3138.0] | same |
| cohesive_extrinsic_insertion | solve_step | base | 5 | 16 | 303.22 ms [302.08 ms, 311.26 ms] | 18.95 ms [18.88 ms, 19.45 ms] | 394.8 [393.3, 405.3] | same |
| cohesive_intrinsic_bilinear | mesh | base | 5 | 1 | 2.18 ms [2.15 ms, 2.22 ms] | 2.18 ms [2.15 ms, 2.22 ms] | 45.5 [44.9, 46.2] | same |
| cohesive_intrinsic_bilinear | init | base | 5 | 1 | 1.063 s [1.061 s, 1.070 s] | 1.063 s [1.061 s, 1.070 s] | 22136.5 [22099.1, 22300.2] | same |
| cohesive_intrinsic_bilinear | stable_time_step | base | 5 | 5 | 14.09 ms [14.08 ms, 14.12 ms] | 2.82 ms [2.82 ms, 2.82 ms] | 58.7 [58.7, 58.8] | same |
| cohesive_intrinsic_bilinear | solve_step | base | 5 | 300 | 6.353 s [6.329 s, 6.472 s] | 21.18 ms [21.10 ms, 21.57 ms] | 441.2 [439.5, 449.4] | same |
| cohesive_intrinsic_bilinear | energies | base | 5 | 60 | 398.16 ms [397.11 ms, 403.39 ms] | 6.64 ms [6.62 ms, 6.72 ms] | 138.3 [137.9, 140.1] | same |
| cohesive_intrinsic_exponential | mesh | base | 5 | 1 | 2.17 ms [2.15 ms, 2.21 ms] | 2.17 ms [2.15 ms, 2.21 ms] | 45.2 [44.8, 46.0] | same |
| cohesive_intrinsic_exponential | init | base | 5 | 1 | 1.066 s [1.064 s, 1.084 s] | 1.066 s [1.064 s, 1.084 s] | 22218.5 [22175.0, 22575.6] | same |
| cohesive_intrinsic_exponential | stable_time_step | base | 5 | 5 | 14.08 ms [14.07 ms, 14.20 ms] | 2.82 ms [2.81 ms, 2.84 ms] | 58.7 [58.6, 59.2] | same |
| cohesive_intrinsic_exponential | solve_step | base | 5 | 300 | 8.155 s [8.093 s, 8.212 s] | 27.18 ms [26.98 ms, 27.37 ms] | 566.3 [562.0, 570.2] | same |
| cohesive_intrinsic_exponential | energies | base | 5 | 60 | 397.51 ms [396.96 ms, 408.00 ms] | 6.63 ms [6.62 ms, 6.80 ms] | 138.0 [137.8, 141.7] | same |
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 474.4 µs [471.4 µs, 489.6 µs] | 474.4 µs [471.4 µs, 489.6 µs] | 131.8 [130.9, 136.0] | same |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 14.38 ms [14.29 ms, 14.52 ms] | 14.38 ms [14.29 ms, 14.52 ms] | 3993.2 [3968.8, 4032.0] | same |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 9.871 s [9.803 s, 10.186 s] | 329.04 ms [326.75 ms, 339.53 ms] | 91399.3 [90765.3, 94313.3] | same |
| explicit_hex8_fd | mesh | base | 5 | 1 | 14.29 ms [14.03 ms, 14.49 ms] | 14.29 ms [14.03 ms, 14.49 ms] | 129.2 [126.9, 131.0] | same |
| explicit_hex8_fd | init | base | 5 | 1 | 970.96 ms [964.06 ms, 978.50 ms] | 970.96 ms [964.06 ms, 978.50 ms] | 8779.6 [8717.2, 8847.9] | same |
| explicit_hex8_fd | stable_time_step | base | 5 | 5 | 31.02 ms [30.92 ms, 31.19 ms] | 6.20 ms [6.18 ms, 6.24 ms] | 56.1 [55.9, 56.4] | same |
| explicit_hex8_fd | solve_step | base | 5 | 30 | 3.614 s [3.608 s, 3.647 s] | 120.48 ms [120.26 ms, 121.58 ms] | 1089.4 [1087.4, 1099.3] | same |
| explicit_hex8_fd | predictor | base | 5 | 30 | 17.1 µs [16.0 µs, 18.8 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_hex8_fd | residual | base | 5 | 30 | 3.025 s [3.015 s, 3.047 s] | 100.85 ms [100.49 ms, 101.56 ms] | 911.9 [908.6, 918.3] | same |
| explicit_hex8_fd | lumped_matrix | base | 5 | 30 | 37.9 µs [31.2 µs, 48.2 µs] | 1.3 µs [1.0 µs, 1.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_hex8_fd | corrector | base | 5 | 30 | 20.6 µs [16.4 µs, 39.7 µs] | 0.7 µs [0.5 µs, 1.3 µs] | 0.0 [0.0, 0.0] | same |
| explicit_hex8_fd | step_hooks | base | 5 | 60 | 486.56 ms [481.12 ms, 490.93 ms] | 8.11 ms [8.02 ms, 8.18 ms] | 73.3 [72.5, 74.0] | same |
| explicit_hex8_fd | step_rest | base | 5 | 30 | 108.11 ms [107.57 ms, 110.05 ms] | 3.60 ms [3.59 ms, 3.67 ms] | 32.6 [32.4, 33.2] | same |
| explicit_hex8_fd | internal_forces | base | 5 | 6 | 599.74 ms [596.13 ms, 604.03 ms] | 99.96 ms [99.35 ms, 100.67 ms] | 903.8 [898.4, 910.3] | same |
| explicit_hex8_fd | stresses | base | 5 | 6 | 163.81 ms [161.65 ms, 165.25 ms] | 27.30 ms [26.94 ms, 27.54 ms] | 246.9 [243.6, 249.0] | same |
| explicit_hex8_fd | energies | base | 5 | 6 | 689.47 ms [683.16 ms, 695.92 ms] | 114.91 ms [113.86 ms, 115.99 ms] | 1039.1 [1029.6, 1048.8] | same |
| explicit_hex8_ss | mesh | base | 5 | 1 | 14.42 ms [14.37 ms, 14.86 ms] | 14.42 ms [14.37 ms, 14.86 ms] | 130.4 [130.0, 134.3] | same |
| explicit_hex8_ss | init | base | 5 | 1 | 879.14 ms [874.24 ms, 882.66 ms] | 879.14 ms [874.24 ms, 882.66 ms] | 7949.4 [7905.1, 7981.2] | same |
| explicit_hex8_ss | stable_time_step | base | 5 | 5 | 31.11 ms [30.95 ms, 31.54 ms] | 6.22 ms [6.19 ms, 6.31 ms] | 56.3 [56.0, 57.0] | same |
| explicit_hex8_ss | solve_step | base | 5 | 50 | 3.380 s [3.339 s, 3.429 s] | 67.60 ms [66.78 ms, 68.59 ms] | 611.2 [603.9, 620.2] | same |
| explicit_hex8_ss | predictor | base | 5 | 50 | 31.2 µs [26.1 µs, 35.6 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_hex8_ss | residual | base | 5 | 50 | 2.630 s [2.591 s, 2.668 s] | 52.59 ms [51.82 ms, 53.36 ms] | 475.5 [468.6, 482.5] | same |
| explicit_hex8_ss | lumped_matrix | base | 5 | 50 | 59.5 µs [54.3 µs, 60.2 µs] | 1.2 µs [1.1 µs, 1.2 µs] | 0.0 [0.0, 0.0] | same |
| explicit_hex8_ss | corrector | base | 5 | 50 | 38.5 µs [27.4 µs, 41.3 µs] | 0.8 µs [0.5 µs, 0.8 µs] | 0.0 [0.0, 0.0] | same |
| explicit_hex8_ss | step_hooks | base | 5 | 100 | 569.78 ms [566.10 ms, 577.32 ms] | 5.70 ms [5.66 ms, 5.77 ms] | 51.5 [51.2, 52.2] | same |
| explicit_hex8_ss | step_rest | base | 5 | 50 | 180.11 ms [175.15 ms, 183.66 ms] | 3.60 ms [3.50 ms, 3.67 ms] | 32.6 [31.7, 33.2] | same |
| explicit_hex8_ss | internal_forces | base | 5 | 10 | 518.57 ms [503.86 ms, 521.01 ms] | 51.86 ms [50.39 ms, 52.10 ms] | 468.9 [455.6, 471.1] | same |
| explicit_hex8_ss | stresses | base | 5 | 10 | 222.85 ms [220.00 ms, 229.04 ms] | 22.29 ms [22.00 ms, 22.90 ms] | 201.5 [198.9, 207.1] | same |
| explicit_hex8_ss | energies | base | 5 | 10 | 1.152 s [1.127 s, 1.160 s] | 115.23 ms [112.73 ms, 115.95 ms] | 1041.9 [1019.4, 1048.5] | same |
| explicit_marigo | mesh | base | 5 | 1 | 4.52 ms [4.47 ms, 4.92 ms] | 4.52 ms [4.47 ms, 4.92 ms] | 42.8 [42.4, 46.6] | same |
| explicit_marigo | init | base | 5 | 1 | 87.82 ms [87.26 ms, 88.18 ms] | 87.82 ms [87.26 ms, 88.18 ms] | 832.8 [827.5, 836.2] | same |
| explicit_marigo | stable_time_step | base | 5 | 5 | 30.86 ms [30.78 ms, 30.91 ms] | 6.17 ms [6.16 ms, 6.18 ms] | 58.5 [58.4, 58.6] | same |
| explicit_marigo | solve_step | base | 5 | 200 | 4.822 s [4.780 s, 4.877 s] | 24.11 ms [23.90 ms, 24.38 ms] | 228.6 [226.6, 231.2] | same |
| explicit_marigo | predictor | base | 5 | 200 | 110.0 µs [83.0 µs, 125.5 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_marigo | residual | base | 5 | 200 | 2.740 s [2.713 s, 2.765 s] | 13.70 ms [13.57 ms, 13.82 ms] | 129.9 [128.7, 131.1] | same |
| explicit_marigo | lumped_matrix | base | 5 | 200 | 179.5 µs [172.6 µs, 208.5 µs] | 0.9 µs [0.9 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_marigo | corrector | base | 5 | 200 | 118.1 µs [107.9 µs, 143.4 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_marigo | step_hooks | base | 5 | 400 | 1.943 s [1.927 s, 1.993 s] | 4.86 ms [4.82 ms, 4.98 ms] | 46.1 [45.7, 47.2] | same |
| explicit_marigo | step_rest | base | 5 | 200 | 128.29 ms [127.86 ms, 129.49 ms] | 641.5 µs [639.3 µs, 647.5 µs] | 6.1 [6.1, 6.1] | same |
| explicit_marigo | internal_forces | base | 5 | 40 | 530.41 ms [525.95 ms, 562.53 ms] | 13.26 ms [13.15 ms, 14.06 ms] | 125.7 [124.7, 133.4] | same |
| explicit_marigo | stresses | base | 5 | 40 | 360.68 ms [358.98 ms, 390.63 ms] | 9.02 ms [8.97 ms, 9.77 ms] | 85.5 [85.1, 92.6] | same |
| explicit_marigo | energies | base | 5 | 40 | 292.49 ms [292.28 ms, 294.28 ms] | 7.31 ms [7.31 ms, 7.36 ms] | 69.3 [69.3, 69.8] | same |
| explicit_maxwell | mesh | base | 5 | 1 | 4.50 ms [4.49 ms, 4.68 ms] | 4.50 ms [4.49 ms, 4.68 ms] | 42.6 [42.5, 44.4] | same |
| explicit_maxwell | init | base | 5 | 1 | 98.94 ms [98.81 ms, 100.06 ms] | 98.94 ms [98.81 ms, 100.06 ms] | 938.2 [937.0, 948.9] | same |
| explicit_maxwell | stable_time_step | base | 5 | 5 | 30.89 ms [30.77 ms, 30.97 ms] | 6.18 ms [6.15 ms, 6.19 ms] | 58.6 [58.3, 58.7] | same |
| explicit_maxwell | solve_step | base | 5 | 40 | 6.933 s [6.866 s, 7.004 s] | 173.32 ms [171.66 ms, 175.09 ms] | 1643.5 [1627.8, 1660.3] | same |
| explicit_maxwell | predictor | base | 5 | 40 | 27.0 µs [21.7 µs, 27.9 µs] | 0.7 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_maxwell | residual | base | 5 | 40 | 1.808 s [1.806 s, 1.821 s] | 45.19 ms [45.15 ms, 45.53 ms] | 428.5 [428.2, 431.7] | same |
| explicit_maxwell | lumped_matrix | base | 5 | 40 | 54.6 µs [46.4 µs, 77.5 µs] | 1.4 µs [1.2 µs, 1.9 µs] | 0.0 [0.0, 0.0] | same |
| explicit_maxwell | corrector | base | 5 | 40 | 31.1 µs [25.2 µs, 40.5 µs] | 0.8 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_maxwell | step_hooks | base | 5 | 80 | 5.098 s [5.033 s, 5.155 s] | 63.72 ms [62.91 ms, 64.44 ms] | 604.3 [596.6, 611.0] | same |
| explicit_maxwell | step_rest | base | 5 | 40 | 26.40 ms [25.57 ms, 26.87 ms] | 660.0 µs [639.2 µs, 671.7 µs] | 6.3 [6.1, 6.4] | same |
| explicit_maxwell | internal_forces | base | 5 | 8 | 332.73 ms [332.03 ms, 346.75 ms] | 41.59 ms [41.50 ms, 43.34 ms] | 394.4 [393.6, 411.0] | same |
| explicit_maxwell | stresses | base | 5 | 8 | 301.37 ms [300.88 ms, 316.47 ms] | 37.67 ms [37.61 ms, 39.56 ms] | 357.2 [356.6, 375.1] | same |
| explicit_maxwell | energies | base | 5 | 8 | 44.09 ms [44.04 ms, 44.85 ms] | 5.51 ms [5.51 ms, 5.61 ms] | 52.3 [52.2, 53.2] | same |
| explicit_mazars | mesh | base | 5 | 1 | 4.46 ms [4.41 ms, 4.60 ms] | 4.46 ms [4.41 ms, 4.60 ms] | 42.2 [41.8, 43.6] | same |
| explicit_mazars | init | base | 5 | 1 | 87.12 ms [86.94 ms, 88.14 ms] | 87.12 ms [86.94 ms, 88.14 ms] | 826.2 [824.4, 835.8] | same |
| explicit_mazars | stable_time_step | base | 5 | 5 | 30.86 ms [30.77 ms, 31.45 ms] | 6.17 ms [6.15 ms, 6.29 ms] | 58.5 [58.4, 59.6] | same |
| explicit_mazars | solve_step | base | 5 | 80 | 7.281 s [7.187 s, 7.728 s] | 91.02 ms [89.84 ms, 96.60 ms] | 863.1 [851.9, 916.1] | same |
| explicit_mazars | predictor | base | 5 | 80 | 41.2 µs [27.3 µs, 51.6 µs] | 0.5 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_mazars | residual | base | 5 | 80 | 6.422 s [6.353 s, 6.899 s] | 80.27 ms [79.41 ms, 86.23 ms] | 761.2 [753.0, 817.7] | same |
| explicit_mazars | lumped_matrix | base | 5 | 80 | 85.3 µs [81.2 µs, 105.2 µs] | 1.1 µs [1.0 µs, 1.3 µs] | 0.0 [0.0, 0.0] | same |
| explicit_mazars | corrector | base | 5 | 80 | 49.5 µs [41.5 µs, 67.4 µs] | 0.6 µs [0.5 µs, 0.8 µs] | 0.0 [0.0, 0.0] | same |
| explicit_mazars | step_hooks | base | 5 | 160 | 777.07 ms [769.85 ms, 805.86 ms] | 4.86 ms [4.81 ms, 5.04 ms] | 46.1 [45.6, 47.8] | same |
| explicit_mazars | step_rest | base | 5 | 80 | 51.44 ms [51.12 ms, 52.76 ms] | 643.0 µs [639.0 µs, 659.5 µs] | 6.1 [6.1, 6.3] | same |
| explicit_mazars | internal_forces | base | 5 | 16 | 1.380 s [1.361 s, 1.527 s] | 86.24 ms [85.09 ms, 95.41 ms] | 817.8 [806.9, 904.7] | same |
| explicit_mazars | stresses | base | 5 | 16 | 1.304 s [1.291 s, 1.484 s] | 81.49 ms [80.71 ms, 92.72 ms] | 772.7 [765.3, 879.3] | same |
| explicit_mazars | energies | base | 5 | 16 | 106.45 ms [106.16 ms, 107.19 ms] | 6.65 ms [6.64 ms, 6.70 ms] | 63.1 [62.9, 63.5] | same |
| explicit_mazars_non_local | mesh | base | 5 | 1 | 2.16 ms [2.12 ms, 2.17 ms] | 2.16 ms [2.12 ms, 2.17 ms] | 45.1 [44.2, 45.3] | same |
| explicit_mazars_non_local | init | base | 5 | 1 | 905.51 ms [900.47 ms, 907.25 ms] | 905.51 ms [900.47 ms, 907.25 ms] | 18864.8 [18759.7, 18901.0] | same |
| explicit_mazars_non_local | stable_time_step | base | 5 | 5 | 14.07 ms [14.06 ms, 14.19 ms] | 2.81 ms [2.81 ms, 2.84 ms] | 58.6 [58.6, 59.1] | same |
| explicit_mazars_non_local | solve_step | base | 5 | 40 | 7.613 s [7.546 s, 7.801 s] | 190.33 ms [188.65 ms, 195.02 ms] | 3965.3 [3930.2, 4062.8] | same |
| explicit_mazars_non_local | predictor | base | 5 | 40 | 8.5 µs [5.7 µs, 16.0 µs] | 0.2 µs [0.1 µs, 0.4 µs] | 0.0 [0.0, 0.0] | same |
| explicit_mazars_non_local | residual | base | 5 | 40 | 7.429 s [7.362 s, 7.602 s] | 185.73 ms [184.06 ms, 190.04 ms] | 3869.4 [3834.5, 3959.2] | same |
| explicit_mazars_non_local | lumped_matrix | base | 5 | 40 | 26.8 µs [22.9 µs, 39.4 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_mazars_non_local | corrector | base | 5 | 40 | 23.5 µs [22.4 µs, 36.6 µs] | 0.6 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.0] | same |
| explicit_mazars_non_local | step_hooks | base | 5 | 80 | 170.06 ms [169.94 ms, 181.82 ms] | 2.13 ms [2.12 ms, 2.27 ms] | 44.3 [44.3, 47.3] | same |
| explicit_mazars_non_local | step_rest | base | 5 | 40 | 13.67 ms [13.36 ms, 16.69 ms] | 341.9 µs [334.1 µs, 417.2 µs] | 7.1 [7.0, 8.7] | same |
| explicit_mazars_non_local | internal_forces | base | 5 | 8 | 1.513 s [1.510 s, 1.611 s] | 189.12 ms [188.69 ms, 201.38 ms] | 3939.9 [3931.0, 4195.5] | same |
| explicit_mazars_non_local | stresses | base | 5 | 8 | 243.33 ms [241.46 ms, 267.50 ms] | 30.42 ms [30.18 ms, 33.44 ms] | 633.7 [628.8, 696.6] | same |
| explicit_mazars_non_local | energies | base | 5 | 8 | 32.36 ms [32.32 ms, 32.38 ms] | 4.05 ms [4.04 ms, 4.05 ms] | 84.3 [84.2, 84.3] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base | 5 | 2 | 1.689 s [1.686 s, 1.917 s] | 844.37 ms [843.16 ms, 958.31 ms] | 17591.1 [17565.8, 19964.9] | same |
| explicit_multi_material | mesh | base | 5 | 1 | 4.56 ms [4.53 ms, 4.73 ms] | 4.56 ms [4.53 ms, 4.73 ms] | 43.3 [43.0, 44.8] | same |
| explicit_multi_material | init | base | 5 | 1 | 89.09 ms [89.00 ms, 90.46 ms] | 89.09 ms [89.00 ms, 90.46 ms] | 844.8 [844.0, 857.8] | same |
| explicit_multi_material | stable_time_step | base | 5 | 5 | 31.32 ms [31.28 ms, 31.39 ms] | 6.26 ms [6.26 ms, 6.28 ms] | 59.4 [59.3, 59.5] | same |
| explicit_multi_material | solve_step | base | 5 | 120 | 5.597 s [5.520 s, 5.855 s] | 46.64 ms [46.00 ms, 48.79 ms] | 442.3 [436.2, 462.7] | same |
| explicit_multi_material | predictor | base | 5 | 120 | 51.2 µs [47.7 µs, 68.5 µs] | 0.4 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_multi_material | residual | base | 5 | 120 | 4.811 s [4.744 s, 5.001 s] | 40.09 ms [39.54 ms, 41.67 ms] | 380.2 [374.9, 395.2] | same |
| explicit_multi_material | lumped_matrix | base | 5 | 120 | 169.1 µs [131.0 µs, 198.3 µs] | 1.4 µs [1.1 µs, 1.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_multi_material | corrector | base | 5 | 120 | 69.6 µs [53.7 µs, 89.0 µs] | 0.6 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_multi_material | step_hooks | base | 5 | 240 | 702.86 ms [699.69 ms, 766.25 ms] | 2.93 ms [2.92 ms, 3.19 ms] | 27.8 [27.6, 30.3] | same |
| explicit_multi_material | step_rest | base | 5 | 120 | 75.70 ms [74.91 ms, 85.99 ms] | 630.8 µs [624.2 µs, 716.6 µs] | 6.0 [5.9, 6.8] | same |
| explicit_multi_material | internal_forces | base | 5 | 24 | 984.74 ms [970.27 ms, 1.063 s] | 41.03 ms [40.43 ms, 44.27 ms] | 389.1 [383.4, 419.8] | same |
| explicit_multi_material | stresses | base | 5 | 24 | 885.39 ms [860.37 ms, 904.27 ms] | 36.89 ms [35.85 ms, 37.68 ms] | 349.8 [339.9, 357.3] | same |
| explicit_multi_material | energies | base | 5 | 24 | 206.35 ms [206.09 ms, 208.17 ms] | 8.60 ms [8.59 ms, 8.67 ms] | 81.5 [81.4, 82.2] | same |
| explicit_neohookean | mesh | base | 5 | 1 | 4.49 ms [4.42 ms, 4.52 ms] | 4.49 ms [4.42 ms, 4.52 ms] | 42.6 [41.9, 42.9] | same |
| explicit_neohookean | init | base | 5 | 1 | 93.06 ms [92.11 ms, 93.50 ms] | 93.06 ms [92.11 ms, 93.50 ms] | 882.5 [873.5, 886.6] | same |
| explicit_neohookean | stable_time_step | base | 5 | 5 | 30.87 ms [30.82 ms, 31.72 ms] | 6.17 ms [6.16 ms, 6.34 ms] | 58.5 [58.5, 60.1] | same |
| explicit_neohookean | solve_step | base | 5 | 100 | 3.035 s [2.987 s, 3.173 s] | 30.35 ms [29.87 ms, 31.73 ms] | 287.8 [283.3, 300.9] | same |
| explicit_neohookean | predictor | base | 5 | 100 | 37.1 µs [30.7 µs, 44.5 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | same |
| explicit_neohookean | residual | base | 5 | 100 | 2.787 s [2.741 s, 2.851 s] | 27.87 ms [27.41 ms, 28.51 ms] | 264.3 [259.9, 270.4] | same |
| explicit_neohookean | lumped_matrix | base | 5 | 100 | 68.5 µs [57.2 µs, 111.2 µs] | 0.7 µs [0.6 µs, 1.1 µs] | 0.0 [0.0, 0.0] | same |
| explicit_neohookean | corrector | base | 5 | 100 | 58.2 µs [48.6 µs, 89.4 µs] | 0.6 µs [0.5 µs, 0.9 µs] | 0.0 [0.0, 0.0] | same |
| explicit_neohookean | step_hooks | base | 5 | 200 | 191.02 ms [188.84 ms, 257.34 ms] | 955.1 µs [944.2 µs, 1.29 ms] | 9.1 [9.0, 12.2] | same |
| explicit_neohookean | step_rest | base | 5 | 100 | 56.47 ms [55.84 ms, 64.22 ms] | 564.7 µs [558.4 µs, 642.2 µs] | 5.4 [5.3, 6.1] | same |
| explicit_neohookean | internal_forces | base | 5 | 20 | 552.47 ms [549.46 ms, 571.25 ms] | 27.62 ms [27.47 ms, 28.56 ms] | 261.9 [260.5, 270.8] | same |
| explicit_neohookean | stresses | base | 5 | 20 | 422.07 ms [420.05 ms, 429.79 ms] | 21.10 ms [21.00 ms, 21.49 ms] | 200.1 [199.2, 203.8] | same |
| explicit_neohookean | energies | base | 5 | 20 | 529.98 ms [525.98 ms, 559.04 ms] | 26.50 ms [26.30 ms, 27.95 ms] | 251.3 [249.4, 265.1] | same |
| explicit_orthotropic | mesh | base | 5 | 1 | 4.47 ms [4.42 ms, 4.55 ms] | 4.47 ms [4.42 ms, 4.55 ms] | 42.4 [41.9, 43.2] | same |
| explicit_orthotropic | init | base | 5 | 1 | 81.92 ms [81.39 ms, 82.88 ms] | 81.92 ms [81.39 ms, 82.88 ms] | 776.9 [771.7, 785.9] | same |
| explicit_orthotropic | stable_time_step | base | 5 | 5 | 30.91 ms [30.87 ms, 30.98 ms] | 6.18 ms [6.17 ms, 6.20 ms] | 58.6 [58.5, 58.8] | same |
| explicit_orthotropic | solve_step | base | 5 | 200 | 2.683 s [2.673 s, 3.012 s] | 13.41 ms [13.37 ms, 15.06 ms] | 127.2 [126.7, 142.8] | same |
| explicit_orthotropic | predictor | base | 5 | 200 | 51.2 µs [47.9 µs, 110.7 µs] | 0.3 µs [0.2 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_orthotropic | residual | base | 5 | 200 | 2.342 s [2.333 s, 2.520 s] | 11.71 ms [11.66 ms, 12.60 ms] | 111.0 [110.6, 119.5] | same |
| explicit_orthotropic | lumped_matrix | base | 5 | 200 | 132.2 µs [103.3 µs, 206.6 µs] | 0.7 µs [0.5 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_orthotropic | corrector | base | 5 | 200 | 78.4 µs [60.0 µs, 108.3 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| explicit_orthotropic | step_hooks | base | 5 | 400 | 233.52 ms [230.90 ms, 355.15 ms] | 583.8 µs [577.2 µs, 887.9 µs] | 5.5 [5.5, 8.4] | same |
| explicit_orthotropic | step_rest | base | 5 | 200 | 109.75 ms [108.25 ms, 135.64 ms] | 548.8 µs [541.2 µs, 678.2 µs] | 5.2 [5.1, 6.4] | same |
| explicit_orthotropic | internal_forces | base | 5 | 40 | 457.13 ms [456.45 ms, 505.74 ms] | 11.43 ms [11.41 ms, 12.64 ms] | 108.4 [108.2, 119.9] | same |
| explicit_orthotropic | stresses | base | 5 | 40 | 297.71 ms [297.13 ms, 322.07 ms] | 7.44 ms [7.43 ms, 8.05 ms] | 70.6 [70.4, 76.4] | same |
| explicit_orthotropic | energies | base | 5 | 40 | 382.57 ms [382.13 ms, 412.81 ms] | 9.56 ms [9.55 ms, 10.32 ms] | 90.7 [90.6, 97.9] | same |
| explicit_plastic | mesh | base | 5 | 1 | 4.48 ms [4.45 ms, 4.52 ms] | 4.48 ms [4.45 ms, 4.52 ms] | 42.5 [42.2, 42.9] | same |
| explicit_plastic | init | base | 5 | 1 | 90.96 ms [90.33 ms, 93.92 ms] | 90.96 ms [90.33 ms, 93.92 ms] | 862.6 [856.6, 890.6] | same |
| explicit_plastic | stable_time_step | base | 5 | 5 | 30.88 ms [30.86 ms, 30.95 ms] | 6.18 ms [6.17 ms, 6.19 ms] | 58.6 [58.5, 58.7] | same |
| explicit_plastic | solve_step | base | 5 | 120 | 6.773 s [6.748 s, 6.869 s] | 56.44 ms [56.23 ms, 57.24 ms] | 535.2 [533.3, 542.8] | same |
| explicit_plastic | predictor | base | 5 | 120 | 55.3 µs [49.0 µs, 77.8 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_plastic | residual | base | 5 | 120 | 5.992 s [5.983 s, 6.079 s] | 49.93 ms [49.86 ms, 50.66 ms] | 473.5 [472.8, 480.4] | same |
| explicit_plastic | lumped_matrix | base | 5 | 120 | 124.1 µs [106.6 µs, 166.6 µs] | 1.0 µs [0.9 µs, 1.4 µs] | 0.0 [0.0, 0.0] | same |
| explicit_plastic | corrector | base | 5 | 120 | 88.4 µs [70.5 µs, 101.6 µs] | 0.7 µs [0.6 µs, 0.8 µs] | 0.0 [0.0, 0.0] | same |
| explicit_plastic | step_hooks | base | 5 | 240 | 702.21 ms [688.88 ms, 710.47 ms] | 2.93 ms [2.87 ms, 2.96 ms] | 27.7 [27.2, 28.1] | same |
| explicit_plastic | step_rest | base | 5 | 120 | 76.26 ms [75.32 ms, 77.93 ms] | 635.5 µs [627.7 µs, 649.5 µs] | 6.0 [6.0, 6.2] | same |
| explicit_plastic | internal_forces | base | 5 | 24 | 1.198 s [1.197 s, 1.226 s] | 49.93 ms [49.87 ms, 51.09 ms] | 473.5 [472.9, 484.5] | same |
| explicit_plastic | stresses | base | 5 | 24 | 1.101 s [1.098 s, 1.131 s] | 45.86 ms [45.74 ms, 47.11 ms] | 434.9 [433.8, 446.7] | same |
| explicit_plastic | energies | base | 5 | 24 | 221.83 ms [221.47 ms, 223.91 ms] | 9.24 ms [9.23 ms, 9.33 ms] | 87.6 [87.5, 88.5] | same |
| explicit_sls | mesh | base | 5 | 1 | 4.49 ms [4.46 ms, 4.62 ms] | 4.49 ms [4.46 ms, 4.62 ms] | 42.6 [42.3, 43.8] | same |
| explicit_sls | init | base | 5 | 1 | 100.45 ms [99.67 ms, 100.79 ms] | 100.45 ms [99.67 ms, 100.79 ms] | 952.6 [945.1, 955.7] | same |
| explicit_sls | stable_time_step | base | 5 | 5 | 30.91 ms [30.84 ms, 31.04 ms] | 6.18 ms [6.17 ms, 6.21 ms] | 58.6 [58.5, 58.9] | same |
| explicit_sls | solve_step | base | 5 | 120 | 5.409 s [5.393 s, 5.465 s] | 45.08 ms [44.95 ms, 45.54 ms] | 427.4 [426.2, 431.9] | same |
| explicit_sls | predictor | base | 5 | 120 | 67.7 µs [64.1 µs, 73.5 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_sls | residual | base | 5 | 120 | 4.960 s [4.945 s, 5.008 s] | 41.34 ms [41.21 ms, 41.73 ms] | 392.0 [390.8, 395.8] | same |
| explicit_sls | lumped_matrix | base | 5 | 120 | 148.9 µs [130.7 µs, 169.1 µs] | 1.2 µs [1.1 µs, 1.4 µs] | 0.0 [0.0, 0.0] | same |
| explicit_sls | corrector | base | 5 | 120 | 93.9 µs [82.6 µs, 102.0 µs] | 0.8 µs [0.7 µs, 0.8 µs] | 0.0 [0.0, 0.0] | same |
| explicit_sls | step_hooks | base | 5 | 240 | 376.94 ms [370.23 ms, 379.14 ms] | 1.57 ms [1.54 ms, 1.58 ms] | 14.9 [14.6, 15.0] | same |
| explicit_sls | step_rest | base | 5 | 120 | 76.91 ms [76.20 ms, 77.17 ms] | 640.9 µs [635.0 µs, 643.1 µs] | 6.1 [6.0, 6.1] | same |
| explicit_sls | internal_forces | base | 5 | 24 | 989.57 ms [983.51 ms, 1.010 s] | 41.23 ms [40.98 ms, 42.09 ms] | 391.0 [388.6, 399.2] | same |
| explicit_sls | stresses | base | 5 | 24 | 887.95 ms [874.83 ms, 917.88 ms] | 37.00 ms [36.45 ms, 38.25 ms] | 350.8 [345.7, 362.7] | same |
| explicit_sls | energies | base | 5 | 24 | 335.61 ms [335.54 ms, 337.00 ms] | 13.98 ms [13.98 ms, 14.04 ms] | 132.6 [132.6, 133.2] | same |
| explicit_tet10_fd | mesh | base | 5 | 1 | 14.29 ms [14.22 ms, 14.44 ms] | 14.29 ms [14.22 ms, 14.44 ms] | 135.5 [134.9, 136.9] | same |
| explicit_tet10_fd | init | base | 5 | 1 | 578.87 ms [575.59 ms, 579.91 ms] | 578.87 ms [575.59 ms, 579.91 ms] | 5489.2 [5458.1, 5499.1] | same |
| explicit_tet10_fd | stable_time_step | base | 5 | 5 | 86.41 ms [86.25 ms, 87.95 ms] | 17.28 ms [17.25 ms, 17.59 ms] | 163.9 [163.6, 166.8] | same |
| explicit_tet10_fd | solve_step | base | 5 | 40 | 2.992 s [2.987 s, 3.027 s] | 74.80 ms [74.68 ms, 75.69 ms] | 709.3 [708.1, 717.7] | same |
| explicit_tet10_fd | predictor | base | 5 | 40 | 27.2 µs [25.6 µs, 34.4 µs] | 0.7 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet10_fd | residual | base | 5 | 40 | 2.495 s [2.492 s, 2.521 s] | 62.37 ms [62.30 ms, 63.04 ms] | 591.4 [590.8, 597.8] | same |
| explicit_tet10_fd | lumped_matrix | base | 5 | 40 | 39.9 µs [31.5 µs, 46.4 µs] | 1.0 µs [0.8 µs, 1.2 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet10_fd | corrector | base | 5 | 40 | 31.0 µs [26.5 µs, 39.8 µs] | 0.8 µs [0.7 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet10_fd | step_hooks | base | 5 | 80 | 308.59 ms [307.33 ms, 313.87 ms] | 3.86 ms [3.84 ms, 3.92 ms] | 36.6 [36.4, 37.2] | same |
| explicit_tet10_fd | step_rest | base | 5 | 40 | 191.97 ms [180.57 ms, 193.48 ms] | 4.80 ms [4.51 ms, 4.84 ms] | 45.5 [42.8, 45.9] | same |
| explicit_tet10_fd | internal_forces | base | 5 | 8 | 486.96 ms [486.09 ms, 493.64 ms] | 60.87 ms [60.76 ms, 61.70 ms] | 577.2 [576.2, 585.1] | same |
| explicit_tet10_fd | stresses | base | 5 | 8 | 125.41 ms [125.05 ms, 128.80 ms] | 15.68 ms [15.63 ms, 16.10 ms] | 148.6 [148.2, 152.7] | same |
| explicit_tet10_fd | energies | base | 5 | 8 | 540.91 ms [531.29 ms, 547.51 ms] | 67.61 ms [66.41 ms, 68.44 ms] | 641.2 [629.8, 649.0] | same |
| explicit_tet10_ss | mesh | base | 5 | 1 | 14.10 ms [14.01 ms, 14.51 ms] | 14.10 ms [14.01 ms, 14.51 ms] | 133.7 [132.9, 137.6] | same |
| explicit_tet10_ss | init | base | 5 | 1 | 540.27 ms [528.40 ms, 544.43 ms] | 540.27 ms [528.40 ms, 544.43 ms] | 5123.2 [5010.6, 5162.6] | same |
| explicit_tet10_ss | stable_time_step | base | 5 | 5 | 86.42 ms [86.20 ms, 86.85 ms] | 17.28 ms [17.24 ms, 17.37 ms] | 163.9 [163.5, 164.7] | same |
| explicit_tet10_ss | solve_step | base | 5 | 50 | 2.257 s [2.136 s, 2.309 s] | 45.14 ms [42.73 ms, 46.19 ms] | 428.0 [405.2, 438.0] | same |
| explicit_tet10_ss | predictor | base | 5 | 50 | 29.5 µs [25.9 µs, 33.4 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet10_ss | residual | base | 5 | 50 | 1.739 s [1.666 s, 1.762 s] | 34.78 ms [33.33 ms, 35.24 ms] | 329.8 [316.0, 334.2] | same |
| explicit_tet10_ss | lumped_matrix | base | 5 | 50 | 48.0 µs [42.4 µs, 52.3 µs] | 1.0 µs [0.8 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet10_ss | corrector | base | 5 | 50 | 41.0 µs [34.6 µs, 61.4 µs] | 0.8 µs [0.7 µs, 1.2 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet10_ss | step_hooks | base | 5 | 100 | 277.99 ms [250.82 ms, 298.24 ms] | 2.78 ms [2.51 ms, 2.98 ms] | 26.4 [23.8, 28.3] | same |
| explicit_tet10_ss | step_rest | base | 5 | 50 | 240.21 ms [218.88 ms, 248.90 ms] | 4.80 ms [4.38 ms, 4.98 ms] | 45.6 [41.5, 47.2] | same |
| explicit_tet10_ss | internal_forces | base | 5 | 10 | 333.43 ms [322.16 ms, 338.12 ms] | 33.34 ms [32.22 ms, 33.81 ms] | 316.2 [305.5, 320.6] | same |
| explicit_tet10_ss | stresses | base | 5 | 10 | 133.51 ms [132.99 ms, 145.20 ms] | 13.35 ms [13.30 ms, 14.52 ms] | 126.6 [126.1, 137.7] | same |
| explicit_tet10_ss | energies | base | 5 | 10 | 674.23 ms [663.23 ms, 681.39 ms] | 67.42 ms [66.32 ms, 68.14 ms] | 639.3 [628.9, 646.1] | same |
| explicit_tet4_fd | mesh | base | 5 | 1 | 6.80 ms [6.73 ms, 6.88 ms] | 6.80 ms [6.73 ms, 6.88 ms] | 42.0 [41.5, 42.5] | same |
| explicit_tet4_fd | init | base | 5 | 1 | 142.49 ms [141.12 ms, 144.77 ms] | 142.49 ms [141.12 ms, 144.77 ms] | 879.6 [871.1, 893.6] | same |
| explicit_tet4_fd | stable_time_step | base | 5 | 5 | 47.43 ms [47.41 ms, 48.06 ms] | 9.49 ms [9.48 ms, 9.61 ms] | 58.6 [58.5, 59.3] | same |
| explicit_tet4_fd | solve_step | base | 5 | 150 | 2.917 s [2.902 s, 3.062 s] | 19.45 ms [19.35 ms, 20.41 ms] | 120.0 [119.4, 126.0] | same |
| explicit_tet4_fd | predictor | base | 5 | 150 | 49.8 µs [38.8 µs, 96.2 µs] | 0.3 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet4_fd | residual | base | 5 | 150 | 2.302 s [2.302 s, 2.358 s] | 15.35 ms [15.34 ms, 15.72 ms] | 94.7 [94.7, 97.0] | same |
| explicit_tet4_fd | lumped_matrix | base | 5 | 150 | 100.7 µs [84.8 µs, 166.8 µs] | 0.7 µs [0.6 µs, 1.1 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet4_fd | corrector | base | 5 | 150 | 79.4 µs [62.8 µs, 100.2 µs] | 0.5 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet4_fd | step_hooks | base | 5 | 300 | 489.07 ms [477.60 ms, 561.84 ms] | 1.63 ms [1.59 ms, 1.87 ms] | 10.1 [9.8, 11.6] | same |
| explicit_tet4_fd | step_rest | base | 5 | 150 | 125.87 ms [122.38 ms, 142.16 ms] | 839.2 µs [815.9 µs, 947.7 µs] | 5.2 [5.0, 5.9] | same |
| explicit_tet4_fd | internal_forces | base | 5 | 30 | 451.88 ms [451.03 ms, 461.68 ms] | 15.06 ms [15.03 ms, 15.39 ms] | 93.0 [92.8, 95.0] | same |
| explicit_tet4_fd | stresses | base | 5 | 30 | 159.37 ms [158.75 ms, 162.22 ms] | 5.31 ms [5.29 ms, 5.41 ms] | 32.8 [32.7, 33.4] | same |
| explicit_tet4_fd | energies | base | 5 | 30 | 484.39 ms [478.91 ms, 502.39 ms] | 16.15 ms [15.96 ms, 16.75 ms] | 99.7 [98.5, 103.4] | same |
| explicit_tet4_ss | mesh | base | 5 | 1 | 6.81 ms [6.76 ms, 6.99 ms] | 6.81 ms [6.76 ms, 6.99 ms] | 42.1 [41.7, 43.1] | same |
| explicit_tet4_ss | init | base | 5 | 1 | 128.05 ms [126.66 ms, 130.14 ms] | 128.05 ms [126.66 ms, 130.14 ms] | 790.4 [781.9, 803.3] | same |
| explicit_tet4_ss | stable_time_step | base | 5 | 5 | 47.40 ms [47.40 ms, 47.45 ms] | 9.48 ms [9.48 ms, 9.49 ms] | 58.5 [58.5, 58.6] | same |
| explicit_tet4_ss | solve_step | base | 5 | 200 | 2.752 s [2.719 s, 2.935 s] | 13.76 ms [13.59 ms, 14.68 ms] | 84.9 [83.9, 90.6] | same |
| explicit_tet4_ss | predictor | base | 5 | 200 | 84.3 µs [64.7 µs, 109.0 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet4_ss | residual | base | 5 | 200 | 2.150 s [2.139 s, 2.243 s] | 10.75 ms [10.69 ms, 11.21 ms] | 66.4 [66.0, 69.2] | same |
| explicit_tet4_ss | lumped_matrix | base | 5 | 200 | 136.0 µs [122.4 µs, 207.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet4_ss | corrector | base | 5 | 200 | 124.7 µs [91.8 µs, 142.0 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tet4_ss | step_hooks | base | 5 | 400 | 433.98 ms [416.27 ms, 506.95 ms] | 1.08 ms [1.04 ms, 1.27 ms] | 6.7 [6.4, 7.8] | same |
| explicit_tet4_ss | step_rest | base | 5 | 200 | 167.33 ms [163.16 ms, 184.83 ms] | 836.7 µs [815.8 µs, 924.2 µs] | 5.2 [5.0, 5.7] | same |
| explicit_tet4_ss | internal_forces | base | 5 | 40 | 414.71 ms [413.19 ms, 430.56 ms] | 10.37 ms [10.33 ms, 10.76 ms] | 64.0 [63.8, 66.4] | same |
| explicit_tet4_ss | stresses | base | 5 | 40 | 175.93 ms [175.15 ms, 180.39 ms] | 4.40 ms [4.38 ms, 4.51 ms] | 27.1 [27.0, 27.8] | same |
| explicit_tet4_ss | energies | base | 5 | 40 | 640.74 ms [632.34 ms, 661.27 ms] | 16.02 ms [15.81 ms, 16.53 ms] | 98.9 [97.6, 102.0] | same |
| explicit_thermal | mesh | base | 5 | 1 | 4.50 ms [4.48 ms, 4.58 ms] | 4.50 ms [4.48 ms, 4.58 ms] | 42.7 [42.5, 43.4] | same |
| explicit_thermal | init | base | 5 | 1 | 83.85 ms [83.14 ms, 84.75 ms] | 83.85 ms [83.14 ms, 84.75 ms] | 795.1 [788.4, 803.6] | same |
| explicit_thermal | stable_time_step | base | 5 | 5 | 30.89 ms [30.82 ms, 30.97 ms] | 6.18 ms [6.16 ms, 6.19 ms] | 58.6 [58.5, 58.7] | same |
| explicit_thermal | solve_step | base | 5 | 300 | 2.581 s [2.569 s, 2.902 s] | 8.60 ms [8.56 ms, 9.67 ms] | 81.6 [81.2, 91.7] | same |
| explicit_thermal | predictor | base | 5 | 300 | 69.9 µs [56.7 µs, 163.7 µs] | 0.2 µs [0.2 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| explicit_thermal | residual | base | 5 | 300 | 2.032 s [2.030 s, 2.182 s] | 6.77 ms [6.77 ms, 7.27 ms] | 64.2 [64.2, 69.0] | same |
| explicit_thermal | lumped_matrix | base | 5 | 300 | 148.3 µs [144.9 µs, 296.6 µs] | 0.5 µs [0.5 µs, 1.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_thermal | corrector | base | 5 | 300 | 91.1 µs [75.5 µs, 199.7 µs] | 0.3 µs [0.3 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_thermal | step_hooks | base | 5 | 600 | 387.42 ms [376.06 ms, 524.44 ms] | 645.7 µs [626.8 µs, 874.1 µs] | 6.1 [5.9, 8.3] | same |
| explicit_thermal | step_rest | base | 5 | 300 | 163.19 ms [162.08 ms, 194.30 ms] | 544.0 µs [540.3 µs, 647.7 µs] | 5.2 [5.1, 6.1] | same |
| explicit_thermal | internal_forces | base | 5 | 60 | 392.57 ms [392.06 ms, 422.91 ms] | 6.54 ms [6.53 ms, 7.05 ms] | 62.0 [62.0, 66.8] | same |
| explicit_thermal | stresses | base | 5 | 60 | 168.06 ms [167.37 ms, 171.74 ms] | 2.80 ms [2.79 ms, 2.86 ms] | 26.6 [26.5, 27.1] | same |
| explicit_thermal | energies | base | 5 | 60 | 622.47 ms [621.09 ms, 674.38 ms] | 10.37 ms [10.35 ms, 11.24 ms] | 98.4 [98.2, 106.6] | same |
| explicit_tri3_fd | mesh | base | 5 | 1 | 8.72 ms [8.61 ms, 8.76 ms] | 8.72 ms [8.61 ms, 8.76 ms] | 48.4 [47.8, 48.6] | same |
| explicit_tri3_fd | init | base | 5 | 1 | 105.80 ms [104.65 ms, 107.44 ms] | 105.80 ms [104.65 ms, 107.44 ms] | 587.8 [581.4, 596.9] | same |
| explicit_tri3_fd | stable_time_step | base | 5 | 5 | 30.41 ms [30.39 ms, 30.62 ms] | 6.08 ms [6.08 ms, 6.12 ms] | 33.8 [33.8, 34.0] | same |
| explicit_tri3_fd | solve_step | base | 5 | 200 | 2.538 s [2.523 s, 2.662 s] | 12.69 ms [12.62 ms, 13.31 ms] | 70.5 [70.1, 74.0] | same |
| explicit_tri3_fd | predictor | base | 5 | 200 | 68.2 µs [49.5 µs, 120.5 µs] | 0.3 µs [0.2 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tri3_fd | residual | base | 5 | 200 | 1.877 s [1.872 s, 1.907 s] | 9.38 ms [9.36 ms, 9.54 ms] | 52.1 [52.0, 53.0] | same |
| explicit_tri3_fd | lumped_matrix | base | 5 | 200 | 169.8 µs [122.6 µs, 235.0 µs] | 0.8 µs [0.6 µs, 1.2 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tri3_fd | corrector | base | 5 | 200 | 103.7 µs [73.7 µs, 150.8 µs] | 0.5 µs [0.4 µs, 0.8 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tri3_fd | step_hooks | base | 5 | 400 | 340.24 ms [321.33 ms, 393.75 ms] | 850.6 µs [803.3 µs, 984.4 µs] | 4.7 [4.5, 5.5] | same |
| explicit_tri3_fd | step_rest | base | 5 | 200 | 327.25 ms [320.62 ms, 363.71 ms] | 1.64 ms [1.60 ms, 1.82 ms] | 9.1 [8.9, 10.1] | same |
| explicit_tri3_fd | internal_forces | base | 5 | 40 | 344.76 ms [342.35 ms, 351.68 ms] | 8.62 ms [8.56 ms, 8.79 ms] | 47.9 [47.5, 48.8] | same |
| explicit_tri3_fd | stresses | base | 5 | 40 | 101.33 ms [100.17 ms, 103.59 ms] | 2.53 ms [2.50 ms, 2.59 ms] | 14.1 [13.9, 14.4] | same |
| explicit_tri3_fd | energies | base | 5 | 40 | 547.02 ms [536.98 ms, 560.07 ms] | 13.68 ms [13.42 ms, 14.00 ms] | 76.0 [74.6, 77.8] | same |
| explicit_tri3_ss | mesh | base | 5 | 1 | 8.70 ms [8.68 ms, 8.81 ms] | 8.70 ms [8.68 ms, 8.81 ms] | 48.3 [48.2, 48.9] | same |
| explicit_tri3_ss | init | base | 5 | 1 | 98.69 ms [97.50 ms, 99.63 ms] | 98.69 ms [97.50 ms, 99.63 ms] | 548.3 [541.7, 553.5] | same |
| explicit_tri3_ss | stable_time_step | base | 5 | 5 | 30.42 ms [30.39 ms, 30.47 ms] | 6.08 ms [6.08 ms, 6.09 ms] | 33.8 [33.8, 33.9] | same |
| explicit_tri3_ss | solve_step | base | 5 | 300 | 3.438 s [3.408 s, 3.595 s] | 11.46 ms [11.36 ms, 11.98 ms] | 63.7 [63.1, 66.6] | same |
| explicit_tri3_ss | predictor | base | 5 | 300 | 104.6 µs [91.9 µs, 141.6 µs] | 0.3 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tri3_ss | residual | base | 5 | 300 | 2.612 s [2.589 s, 2.652 s] | 8.71 ms [8.63 ms, 8.84 ms] | 48.4 [47.9, 49.1] | same |
| explicit_tri3_ss | lumped_matrix | base | 5 | 300 | 221.0 µs [182.9 µs, 353.4 µs] | 0.7 µs [0.6 µs, 1.2 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tri3_ss | corrector | base | 5 | 300 | 157.4 µs [136.5 µs, 195.7 µs] | 0.5 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | same |
| explicit_tri3_ss | step_hooks | base | 5 | 600 | 348.58 ms [327.71 ms, 401.54 ms] | 581.0 µs [546.2 µs, 669.2 µs] | 3.2 [3.0, 3.7] | same |
| explicit_tri3_ss | step_rest | base | 5 | 300 | 494.51 ms [476.55 ms, 547.24 ms] | 1.65 ms [1.59 ms, 1.82 ms] | 9.2 [8.8, 10.1] | same |
| explicit_tri3_ss | internal_forces | base | 5 | 60 | 471.09 ms [469.26 ms, 484.91 ms] | 7.85 ms [7.82 ms, 8.08 ms] | 43.6 [43.4, 44.9] | same |
| explicit_tri3_ss | stresses | base | 5 | 60 | 144.71 ms [142.57 ms, 151.23 ms] | 2.41 ms [2.38 ms, 2.52 ms] | 13.4 [13.2, 14.0] | same |
| explicit_tri3_ss | energies | base | 5 | 60 | 805.30 ms [795.35 ms, 827.54 ms] | 13.42 ms [13.26 ms, 13.79 ms] | 74.6 [73.6, 76.6] | same |
| implicit_dynamic | mesh | base | 5 | 1 | 795.9 µs [788.2 µs, 813.9 µs] | 795.9 µs [788.2 µs, 813.9 µs] | 48.3 [47.9, 49.4] | same |
| implicit_dynamic | init | base | 5 | 1 | 13.38 ms [13.35 ms, 13.40 ms] | 13.38 ms [13.35 ms, 13.40 ms] | 812.6 [810.7, 814.1] | same |
| implicit_dynamic | mass_consistent | base | 5 | 1 | 223.93 ms [221.73 ms, 232.72 ms] | 223.93 ms [221.73 ms, 232.72 ms] | 13601.4 [13467.6, 14135.1] | same |
| implicit_dynamic | solve_step | base | 5 | 60 | 2.745 s [2.706 s, 2.957 s] | 45.74 ms [45.10 ms, 49.28 ms] | 2778.3 [2739.3, 2993.0] | same |
| implicit_dynamic | predictor | base | 5 | 60 | 7.4 µs [5.9 µs, 8.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | same |
| implicit_dynamic | residual | base | 5 | 180 | 189.78 ms [188.74 ms, 204.84 ms] | 1.05 ms [1.05 ms, 1.14 ms] | 64.0 [63.7, 69.1] | same |
| implicit_dynamic | matrix | base | 5 | 420 | 147.1 µs [141.5 µs, 199.8 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| implicit_dynamic | corrector | base | 5 | 120 | 25.5 µs [21.1 µs, 46.1 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | same |
| implicit_dynamic | step_hooks | base | 5 | 120 | 12.27 ms [12.04 ms, 18.14 ms] | 102.3 µs [100.3 µs, 151.2 µs] | 6.2 [6.1, 9.2] | same |
| implicit_dynamic | step_rest | base | 5 | 60 | 2.542 s [2.505 s, 2.733 s] | 42.36 ms [41.75 ms, 45.56 ms] | 2573.0 [2535.8, 2767.0] | same |
| setup_hex8 | mesh | base | 5 | 1 | 1.88 ms [1.87 ms, 1.90 ms] | 1.88 ms [1.87 ms, 1.90 ms] | 135.8 [135.2, 137.1] | same |
| setup_hex8 | init_explicit | base | 5 | 1 | 109.37 ms [109.00 ms, 109.66 ms] | 109.37 ms [109.00 ms, 109.66 ms] | 7911.4 [7885.2, 7932.2] | same |
| setup_hex8 | shape_functions | base | 5 | 3 | 208.35 ms [208.17 ms, 209.11 ms] | 69.45 ms [69.39 ms, 69.70 ms] | 5023.9 [5019.6, 5042.3] | same |
| setup_hex8 | mass_lumped | base | 5 | 3 | 30.50 ms [30.43 ms, 31.09 ms] | 10.17 ms [10.14 ms, 10.36 ms] | 735.5 [733.7, 749.7] | same |
| setup_hex8 | init_implicit | base | 5 | 1 | 107.12 ms [106.45 ms, 107.71 ms] | 107.12 ms [106.45 ms, 107.71 ms] | 7748.8 [7700.1, 7791.3] | same |
| setup_hex8 | mass_consistent | base | 5 | 3 | 6.931 s [6.900 s, 6.952 s] | 2.310 s [2.300 s, 2.317 s] | 167135.1 [166370.1, 167634.7] | same |
| setup_hex8 | stiffness_assembly | base | 5 | 3 | 3.109 s [3.098 s, 3.169 s] | 1.036 s [1.033 s, 1.056 s] | 74971.3 [74711.6, 76418.5] | same |
| setup_tet10 | mesh | base | 5 | 1 | 1.61 ms [1.58 ms, 1.63 ms] | 1.61 ms [1.58 ms, 1.63 ms] | 155.0 [152.4, 157.0] | same |
| setup_tet10 | init_explicit | base | 5 | 1 | 52.95 ms [52.77 ms, 53.17 ms] | 52.95 ms [52.77 ms, 53.17 ms] | 5106.9 [5089.7, 5128.0] | same |
| setup_tet10 | shape_functions | base | 5 | 3 | 97.28 ms [97.21 ms, 106.73 ms] | 32.43 ms [32.40 ms, 35.58 ms] | 3127.7 [3125.2, 3431.5] | same |
| setup_tet10 | mass_lumped | base | 5 | 3 | 3.62 ms [3.61 ms, 3.62 ms] | 1.21 ms [1.20 ms, 1.21 ms] | 116.3 [115.9, 116.5] | same |
| setup_tet10 | init_implicit | base | 5 | 1 | 51.23 ms [51.13 ms, 51.73 ms] | 51.23 ms [51.13 ms, 51.73 ms] | 4941.3 [4931.3, 4989.4] | same |
| setup_tet10 | mass_consistent | base | 5 | 3 | 5.043 s [5.013 s, 5.287 s] | 1.681 s [1.671 s, 1.762 s] | 162146.7 [161171.9, 169979.1] | same |
| setup_tet10 | stiffness_assembly | base | 5 | 3 | 2.396 s [2.386 s, 2.427 s] | 798.66 ms [795.44 ms, 808.89 ms] | 77031.1 [76720.3, 78017.7] | same |
| setup_tet4 | mesh | base | 5 | 1 | 6.83 ms [6.72 ms, 6.86 ms] | 6.83 ms [6.72 ms, 6.86 ms] | 42.2 [41.5, 42.4] | same |
| setup_tet4 | init_explicit | base | 5 | 1 | 126.65 ms [125.95 ms, 127.76 ms] | 126.65 ms [125.95 ms, 127.76 ms] | 781.8 [777.5, 788.6] | same |
| setup_tet4 | shape_functions | base | 5 | 3 | 209.30 ms [208.91 ms, 218.99 ms] | 69.77 ms [69.64 ms, 73.00 ms] | 430.7 [429.8, 450.6] | same |
| setup_tet4 | mass_lumped | base | 5 | 3 | 99.41 ms [99.20 ms, 101.98 ms] | 33.14 ms [33.07 ms, 33.99 ms] | 204.6 [204.1, 209.8] | same |
| setup_tet4 | init_implicit | base | 5 | 1 | 121.13 ms [120.68 ms, 122.69 ms] | 121.13 ms [120.68 ms, 122.69 ms] | 747.7 [744.9, 757.4] | same |
| setup_tet4 | mass_consistent | base | 5 | 3 | 5.147 s [5.143 s, 5.159 s] | 1.716 s [1.714 s, 1.720 s] | 10589.7 [10582.9, 10615.2] | same |
| setup_tet4 | stiffness_assembly | base | 5 | 3 | 3.175 s [3.145 s, 3.211 s] | 1.058 s [1.048 s, 1.070 s] | 6531.9 [6471.7, 6606.0] | same |
| static_linear_eigen | mesh | base | 5 | 1 | 817.7 µs [798.1 µs, 842.0 µs] | 817.7 µs [798.1 µs, 842.0 µs] | 49.7 [48.5, 51.1] | same |
| static_linear_eigen | init | base | 5 | 1 | 13.30 ms [13.28 ms, 13.66 ms] | 13.30 ms [13.28 ms, 13.66 ms] | 808.1 [806.4, 829.7] | same |
| static_linear_eigen | solve_step | base | 5 | 1 | 3.071 s [3.065 s, 3.381 s] | 3.071 s [3.065 s, 3.381 s] | 186518.6 [186134.3, 205384.5] | same |
| static_linear_eigen | predictor | base | 5 | 1 | 0.1 µs [0.1 µs, 0.6 µs] | 0.1 µs [0.1 µs, 0.6 µs] | 0.0 [0.0, 0.0] | same |
| static_linear_eigen | matrix | base | 5 | 1 | 153.81 ms [153.34 ms, 174.97 ms] | 153.81 ms [153.34 ms, 174.97 ms] | 9342.1 [9313.4, 10627.7] | same |
| static_linear_eigen | corrector | base | 5 | 1 | 0.9 µs [0.7 µs, 1.1 µs] | 0.9 µs [0.7 µs, 1.1 µs] | 0.1 [0.0, 0.1] | same |
| static_linear_eigen | step_hooks | base | 5 | 2 | 129.3 µs [124.4 µs, 205.2 µs] | 64.7 µs [62.2 µs, 102.6 µs] | 3.9 [3.8, 6.2] | same |
| static_linear_eigen | step_rest | base | 5 | 1 | 2.917 s [2.911 s, 3.206 s] | 2.917 s [2.911 s, 3.206 s] | 177187.6 [176784.3, 194744.2] | same |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 189.36 ms [188.32 ms, 192.16 ms] | 63.12 ms [62.77 ms, 64.05 ms] | 3833.8 [3812.7, 3890.5] | same |
| static_linear_eigen | analysis | base | 5 | 1 | 42.52 ms [42.30 ms, 42.70 ms] | 42.52 ms [42.30 ms, 42.70 ms] | 2582.6 [2569.4, 2593.8] | same |
| static_linear_eigen | factorize | base | 5 | 2 | 5.541 s [5.534 s, 6.133 s] | 2.771 s [2.767 s, 3.067 s] | 168289.9 [168052.7, 186257.1] | same |
| static_linear_eigen | back_substitution | base | 5 | 20 | 211.06 ms [205.74 ms, 259.42 ms] | 10.55 ms [10.29 ms, 12.97 ms] | 641.0 [624.8, 787.9] | same |
| static_linear_faer | mesh | base | 5 | 1 | 808.5 µs [801.4 µs, 824.0 µs] | 808.5 µs [801.4 µs, 824.0 µs] | 49.1 [48.7, 50.0] | same |
| static_linear_faer | init | base | 5 | 1 | 13.36 ms [13.32 ms, 13.59 ms] | 13.36 ms [13.32 ms, 13.59 ms] | 811.5 [809.0, 825.6] | same |
| static_linear_faer | solve_step | base | 5 | 1 | 5.036 s [4.997 s, 6.398 s] | 5.036 s [4.997 s, 6.398 s] | 305898.9 [303513.1, 388608.3] | same |
| static_linear_faer | predictor | base | 5 | 1 | 0.2 µs [0.1 µs, 0.5 µs] | 0.2 µs [0.1 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| static_linear_faer | matrix | base | 5 | 1 | 153.78 ms [153.13 ms, 195.09 ms] | 153.78 ms [153.13 ms, 195.09 ms] | 9340.5 [9301.0, 11849.5] | same |
| static_linear_faer | corrector | base | 5 | 1 | 0.9 µs [0.8 µs, 1.1 µs] | 0.9 µs [0.8 µs, 1.1 µs] | 0.1 [0.0, 0.1] | same |
| static_linear_faer | step_hooks | base | 5 | 2 | 139.1 µs [121.7 µs, 185.0 µs] | 69.6 µs [60.8 µs, 92.5 µs] | 4.2 [3.7, 5.6] | same |
| static_linear_faer | step_rest | base | 5 | 1 | 4.882 s [4.844 s, 6.203 s] | 4.882 s [4.844 s, 6.203 s] | 296550.9 [294203.7, 376747.6] | same |
| static_linear_faer | stiffness_assembly | base | 5 | 3 | 251.13 ms [247.67 ms, 273.05 ms] | 83.71 ms [82.56 ms, 91.02 ms] | 5084.5 [5014.4, 5528.2] | same |
| static_linear_faer | analysis | base | 5 | 1 | 50.05 ms [49.78 ms, 52.06 ms] | 50.05 ms [49.78 ms, 52.06 ms] | 3040.0 [3023.8, 3161.8] | same |
| static_linear_faer | factorize | base | 5 | 2 | 9.453 s [9.426 s, 10.411 s] | 4.726 s [4.713 s, 5.205 s] | 287066.6 [286250.1, 316167.0] | same |
| static_linear_faer | back_substitution | base | 5 | 20 | 212.76 ms [212.26 ms, 214.53 ms] | 10.64 ms [10.61 ms, 10.73 ms] | 646.1 [644.6, 651.5] | same |
| static_newton_plastic | mesh | base | 5 | 1 | 325.4 µs [320.4 µs, 330.3 µs] | 325.4 µs [320.4 µs, 330.3 µs] | 54.2 [53.4, 55.0] | same |
| static_newton_plastic | init | base | 5 | 1 | 5.79 ms [5.75 ms, 5.82 ms] | 5.79 ms [5.75 ms, 5.82 ms] | 964.8 [957.9, 970.5] | same |
| static_newton_plastic | solve_step | base | 5 | 12 | 3.063 s [3.055 s, 3.727 s] | 255.27 ms [254.57 ms, 310.59 ms] | 42545.3 [42427.9, 51765.3] | same |
| static_newton_plastic | predictor | base | 5 | 12 | 1.0 µs [0.6 µs, 1.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | same |
| static_newton_plastic | residual | base | 5 | 961 | 1.158 s [1.157 s, 1.246 s] | 1.21 ms [1.20 ms, 1.30 ms] | 200.9 [200.6, 216.1] | same |
| static_newton_plastic | matrix | base | 5 | 949 | 334.1 µs [290.6 µs, 438.4 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.1 [0.1, 0.1] | same |
| static_newton_plastic | corrector | base | 5 | 949 | 111.1 µs [88.3 µs, 220.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | same |
| static_newton_plastic | step_hooks | base | 5 | 24 | 3.15 ms [3.09 ms, 3.34 ms] | 131.2 µs [128.7 µs, 139.1 µs] | 21.9 [21.4, 23.2] | same |
| static_newton_plastic | step_rest | base | 5 | 12 | 1.903 s [1.893 s, 2.477 s] | 158.59 ms [157.76 ms, 206.40 ms] | 26432.2 [26293.4, 34399.8] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | 82944 | 15625 | 46875 | 1 | c698395e8 | f11c449c593d38ae |
| cohesive_extrinsic_insertion | 48000 | 17302 | 51906 | 1 | c698395e8 | 0423893e61d5fb15 |
| cohesive_intrinsic_bilinear | 48000 | 46338 | 139014 | 1 | c698395e8 | b6a0645b6cf17997 |
| cohesive_intrinsic_exponential | 48000 | 46338 | 139014 | 1 | c698395e8 | 2e07a3f9b642f09a |
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | c698395e8 | 88b89af4f4608998 |
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | c698395e8 | 78b072c2f464d8f9 |
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | c698395e8 | 37ce0eeb914dd994 |
| explicit_marigo | 105456 | 19683 | 59049 | 1 | c698395e8 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | c698395e8 | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | c698395e8 | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | c698395e8 | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | c698395e8 | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | c698395e8 | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | c698395e8 | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | c698395e8 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | c698395e8 | 36137566d8b810bb |
| explicit_tet10_fd | 105456 | 148877 | 446631 | 1 | c698395e8 | c9f013e1a9bbd7a1 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | c698395e8 | a12864e94a9122c8 |
| explicit_tet4_fd | 162000 | 29791 | 89373 | 1 | c698395e8 | 9d6cdbdad424d346 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | c698395e8 | fecb3fb4b46c8b8c |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | c698395e8 | 27979d9fbcdea83d |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | c698395e8 | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | c698395e8 | b6b902f5ca71049e |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | c698395e8 | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | c698395e8 | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | c698395e8 | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | c698395e8 | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | c698395e8 | 35e60e6f09e111e7 |
| static_linear_faer | 16464 | 3375 | 10125 | 1 | c698395e8 | c9951bde736748ef |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | c698395e8 | 7bd5f114957d7d7b |
