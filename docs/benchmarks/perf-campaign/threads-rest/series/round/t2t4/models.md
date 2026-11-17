Suite `models`, size `m`; variants: `base-t1`, `base-t2`, `base-t4`, `head-t2`, `head-t4`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_quad4 | mesh | base-t1 | 3 | 1 | 106.05 ms [104.70 ms, 106.82 ms] | 106.05 ms [104.70 ms, 106.82 ms] | 662.8 [654.4, 667.6] | – | same |
| heat_explicit_quad4 | mesh | base-t2 | 3 | 1 | 106.71 ms [105.60 ms, 114.26 ms] | 106.71 ms [105.60 ms, 114.26 ms] | 667.0 [660.0, 714.2] | 1.009 [0.999, 1.078] | same |
| heat_explicit_quad4 | mesh | base-t4 | 3 | 1 | 105.32 ms [104.52 ms, 105.85 ms] | 105.32 ms [104.52 ms, 105.85 ms] | 658.2 [653.2, 661.5] | 0.998 [0.986, 0.998] | same |
| heat_explicit_quad4 | mesh | head-t2 | 3 | 1 | 105.28 ms [104.43 ms, 106.31 ms] | 105.28 ms [104.43 ms, 106.31 ms] | 658.0 [652.7, 664.5] | 1.003 [0.978, 1.006] | same |
| heat_explicit_quad4 | mesh | head-t4 | 3 | 1 | 106.29 ms [105.71 ms, 106.62 ms] | 106.29 ms [105.71 ms, 106.62 ms] | 664.3 [660.7, 666.4] | 1.005 [0.990, 1.015] | same |
| heat_explicit_quad4 | init | base-t1 | 3 | 1 | 15.86 ms [15.46 ms, 16.20 ms] | 15.86 ms [15.46 ms, 16.20 ms] | 99.1 [96.6, 101.3] | – | same |
| heat_explicit_quad4 | init | base-t2 | 3 | 1 | 15.94 ms [15.41 ms, 16.13 ms] | 15.94 ms [15.41 ms, 16.13 ms] | 99.6 [96.3, 100.8] | 1.017 [0.951, 1.031] | same |
| heat_explicit_quad4 | init | base-t4 | 3 | 1 | 15.53 ms [15.42 ms, 15.60 ms] | 15.53 ms [15.42 ms, 15.60 ms] | 97.1 [96.4, 97.5] | 0.979 [0.962, 0.998] | same |
| heat_explicit_quad4 | init | head-t2 | 3 | 1 | 16.05 ms [15.44 ms, 16.25 ms] | 16.05 ms [15.44 ms, 16.25 ms] | 100.3 [96.5, 101.6] | 1.003 [0.973, 1.038] | same |
| heat_explicit_quad4 | init | head-t4 | 3 | 1 | 16.11 ms [15.90 ms, 16.45 ms] | 16.11 ms [15.90 ms, 16.45 ms] | 100.7 [99.4, 102.8] | 1.002 [0.994, 1.064] | same |
| heat_explicit_quad4 | stable_time_step | base-t1 | 3 | 1 | 2.55 ms [2.47 ms, 2.69 ms] | 2.55 ms [2.47 ms, 2.69 ms] | 15.9 [15.4, 16.8] | – | same |
| heat_explicit_quad4 | stable_time_step | base-t2 | 3 | 1 | 2.55 ms [2.48 ms, 2.58 ms] | 2.55 ms [2.48 ms, 2.58 ms] | 15.9 [15.5, 16.1] | 1.000 [0.920, 1.044] | same |
| heat_explicit_quad4 | stable_time_step | base-t4 | 3 | 1 | 2.45 ms [2.45 ms, 2.46 ms] | 2.45 ms [2.45 ms, 2.46 ms] | 15.3 [15.3, 15.4] | 0.962 [0.911, 0.998] | same |
| heat_explicit_quad4 | stable_time_step | head-t2 | 3 | 1 | 2.49 ms [2.49 ms, 2.50 ms] | 2.49 ms [2.49 ms, 2.50 ms] | 15.6 [15.6, 15.6] | 0.982 [0.926, 1.009] | same |
| heat_explicit_quad4 | stable_time_step | head-t4 | 3 | 1 | 2.50 ms [2.49 ms, 2.62 ms] | 2.50 ms [2.49 ms, 2.62 ms] | 15.6 [15.6, 16.3] | 0.979 [0.927, 1.060] | same |
| heat_explicit_quad4 | solve_step | base-t1 | 3 | 120 | 1.210 s [1.204 s, 1.228 s] | 10.09 ms [10.03 ms, 10.23 ms] | 63.0 [62.7, 63.9] | – | same |
| heat_explicit_quad4 | solve_step | base-t2 | 3 | 120 | 763.72 ms [763.44 ms, 770.35 ms] | 6.36 ms [6.36 ms, 6.42 ms] | 39.8 [39.8, 40.1] | 0.631 [0.622, 0.640] | same |
| heat_explicit_quad4 | solve_step | base-t4 | 3 | 120 | 661.22 ms [659.58 ms, 713.80 ms] | 5.51 ms [5.50 ms, 5.95 ms] | 34.4 [34.4, 37.2] | 0.549 [0.545, 0.581] | same |
| heat_explicit_quad4 | solve_step | head-t2 | 3 | 120 | 718.24 ms [713.07 ms, 732.22 ms] | 5.99 ms [5.94 ms, 6.10 ms] | 37.4 [37.1, 38.1] | 0.589 [0.585, 0.608] | same |
| heat_explicit_quad4 | solve_step | head-t4 | 3 | 120 | 585.69 ms [545.05 ms, 590.35 ms] | 4.88 ms [4.54 ms, 4.92 ms] | 30.5 [28.4, 30.7] | 0.484 [0.444, 0.490] | same |
| heat_explicit_tet4 | mesh | base-t1 | 3 | 1 | 152.39 ms [150.98 ms, 152.98 ms] | 152.39 ms [150.98 ms, 152.98 ms] | 544.4 [539.3, 546.5] | – | same |
| heat_explicit_tet4 | mesh | base-t2 | 3 | 1 | 152.02 ms [150.68 ms, 154.27 ms] | 152.02 ms [150.68 ms, 154.27 ms] | 543.0 [538.3, 551.1] | 0.994 [0.989, 1.022] | same |
| heat_explicit_tet4 | mesh | base-t4 | 3 | 1 | 153.41 ms [151.48 ms, 154.14 ms] | 153.41 ms [151.48 ms, 154.14 ms] | 548.0 [541.1, 550.6] | 1.007 [0.990, 1.021] | same |
| heat_explicit_tet4 | mesh | head-t2 | 3 | 1 | 152.02 ms [151.46 ms, 152.53 ms] | 152.02 ms [151.46 ms, 152.53 ms] | 543.0 [541.1, 544.9] | 0.998 [0.990, 1.010] | same |
| heat_explicit_tet4 | mesh | head-t4 | 3 | 1 | 151.19 ms [150.78 ms, 152.18 ms] | 151.19 ms [150.78 ms, 152.18 ms] | 540.1 [538.6, 543.6] | 0.989 [0.988, 1.008] | same |
| heat_explicit_tet4 | init | base-t1 | 3 | 1 | 10.01 ms [9.97 ms, 10.21 ms] | 10.01 ms [9.97 ms, 10.21 ms] | 35.8 [35.6, 36.5] | – | same |
| heat_explicit_tet4 | init | base-t2 | 3 | 1 | 10.12 ms [10.04 ms, 10.18 ms] | 10.12 ms [10.04 ms, 10.18 ms] | 36.1 [35.9, 36.4] | 1.011 [0.984, 1.021] | same |
| heat_explicit_tet4 | init | base-t4 | 3 | 1 | 9.98 ms [9.94 ms, 10.07 ms] | 9.98 ms [9.94 ms, 10.07 ms] | 35.6 [35.5, 36.0] | 0.997 [0.977, 1.006] | same |
| heat_explicit_tet4 | init | head-t2 | 3 | 1 | 10.29 ms [10.19 ms, 10.32 ms] | 10.29 ms [10.19 ms, 10.32 ms] | 36.8 [36.4, 36.9] | 1.023 [1.011, 1.028] | same |
| heat_explicit_tet4 | init | head-t4 | 3 | 1 | 10.14 ms [10.01 ms, 10.23 ms] | 10.14 ms [10.01 ms, 10.23 ms] | 36.2 [35.8, 36.5] | 1.000 [0.993, 1.026] | same |
| heat_explicit_tet4 | stable_time_step | base-t1 | 3 | 1 | 9.60 ms [9.57 ms, 9.60 ms] | 9.60 ms [9.57 ms, 9.60 ms] | 34.3 [34.2, 34.3] | – | same |
| heat_explicit_tet4 | stable_time_step | base-t2 | 3 | 1 | 9.57 ms [9.55 ms, 9.63 ms] | 9.57 ms [9.55 ms, 9.63 ms] | 34.2 [34.1, 34.4] | 1.000 [0.994, 1.003] | same |
| heat_explicit_tet4 | stable_time_step | base-t4 | 3 | 1 | 9.55 ms [9.54 ms, 9.57 ms] | 9.55 ms [9.54 ms, 9.57 ms] | 34.1 [34.1, 34.2] | 0.994 [0.994, 1.000] | same |
| heat_explicit_tet4 | stable_time_step | head-t2 | 3 | 1 | 9.62 ms [9.61 ms, 9.67 ms] | 9.62 ms [9.61 ms, 9.67 ms] | 34.4 [34.3, 34.6] | 1.005 [1.002, 1.008] | same |
| heat_explicit_tet4 | stable_time_step | head-t4 | 3 | 1 | 9.57 ms [9.56 ms, 9.58 ms] | 9.57 ms [9.56 ms, 9.58 ms] | 34.2 [34.1, 34.2] | 0.998 [0.996, 0.999] | same |
| heat_explicit_tet4 | solve_step | base-t1 | 3 | 100 | 895.69 ms [894.05 ms, 899.93 ms] | 8.96 ms [8.94 ms, 9.00 ms] | 32.0 [31.9, 32.1] | – | same |
| heat_explicit_tet4 | solve_step | base-t2 | 3 | 100 | 535.50 ms [522.59 ms, 537.18 ms] | 5.35 ms [5.23 ms, 5.37 ms] | 19.1 [18.7, 19.2] | 0.595 [0.583, 0.601] | same |
| heat_explicit_tet4 | solve_step | base-t4 | 3 | 100 | 363.37 ms [362.71 ms, 364.39 ms] | 3.63 ms [3.63 ms, 3.64 ms] | 13.0 [13.0, 13.0] | 0.406 [0.404, 0.407] | same |
| heat_explicit_tet4 | solve_step | head-t2 | 3 | 100 | 504.06 ms [488.11 ms, 507.31 ms] | 5.04 ms [4.88 ms, 5.07 ms] | 18.0 [17.4, 18.1] | 0.563 [0.542, 0.567] | same |
| heat_explicit_tet4 | solve_step | head-t4 | 3 | 100 | 314.77 ms [314.53 ms, 315.94 ms] | 3.15 ms [3.15 ms, 3.16 ms] | 11.2 [11.2, 11.3] | 0.352 [0.350, 0.353] | same |
| phasefield_staggered_tet4 | mesh | base-t1 | 3 | 1 | 4.12 ms [4.12 ms, 4.14 ms] | 4.12 ms [4.12 ms, 4.14 ms] | 687.3 [686.7, 689.8] | – | same |
| phasefield_staggered_tet4 | mesh | base-t2 | 3 | 1 | 4.06 ms [4.05 ms, 4.10 ms] | 4.06 ms [4.05 ms, 4.10 ms] | 677.2 [674.5, 684.0] | 0.985 [0.982, 0.992] | same |
| phasefield_staggered_tet4 | mesh | base-t4 | 3 | 1 | 4.10 ms [4.08 ms, 4.12 ms] | 4.10 ms [4.08 ms, 4.12 ms] | 683.5 [680.5, 686.0] | 0.994 [0.990, 0.995] | same |
| phasefield_staggered_tet4 | mesh | head-t2 | 3 | 1 | 4.03 ms [3.94 ms, 4.06 ms] | 4.03 ms [3.94 ms, 4.06 ms] | 672.2 [656.7, 677.2] | 0.978 [0.956, 0.982] | same |
| phasefield_staggered_tet4 | mesh | head-t4 | 3 | 1 | 4.08 ms [3.98 ms, 4.08 ms] | 4.08 ms [3.98 ms, 4.08 ms] | 679.5 [663.2, 680.3] | 0.985 [0.966, 0.990] | same |
| phasefield_staggered_tet4 | init | base-t1 | 3 | 1 | 1.41 ms [1.39 ms, 1.44 ms] | 1.41 ms [1.39 ms, 1.44 ms] | 235.7 [232.0, 240.3] | – | same |
| phasefield_staggered_tet4 | init | base-t2 | 3 | 1 | 1.32 ms [1.32 ms, 1.38 ms] | 1.32 ms [1.32 ms, 1.38 ms] | 220.5 [219.5, 230.3] | 0.946 [0.936, 0.958] | same |
| phasefield_staggered_tet4 | init | base-t4 | 3 | 1 | 1.40 ms [1.40 ms, 1.42 ms] | 1.40 ms [1.40 ms, 1.42 ms] | 232.7 [232.5, 237.0] | 1.003 [0.967, 1.006] | same |
| phasefield_staggered_tet4 | init | head-t2 | 3 | 1 | 1.35 ms [1.29 ms, 1.41 ms] | 1.35 ms [1.29 ms, 1.41 ms] | 224.3 [215.3, 235.3] | 0.952 [0.928, 0.979] | same |
| phasefield_staggered_tet4 | init | head-t4 | 3 | 1 | 1.34 ms [1.27 ms, 1.41 ms] | 1.34 ms [1.27 ms, 1.41 ms] | 223.7 [211.5, 234.5] | 0.931 [0.912, 0.995] | same |
| phasefield_staggered_tet4 | solid_solve | base-t1 | 3 | 4 | 317.40 ms [316.04 ms, 318.15 ms] | 79.35 ms [79.01 ms, 79.54 ms] | 13225.2 [13168.1, 13256.3] | – | same |
| phasefield_staggered_tet4 | solid_solve | base-t2 | 3 | 4 | 307.94 ms [307.89 ms, 309.58 ms] | 76.98 ms [76.97 ms, 77.40 ms] | 12830.7 [12828.8, 12899.3] | 0.973 [0.970, 0.974] | same |
| phasefield_staggered_tet4 | solid_solve | base-t4 | 3 | 4 | 306.64 ms [306.35 ms, 307.91 ms] | 76.66 ms [76.59 ms, 76.98 ms] | 12776.7 [12764.6, 12829.7] | 0.969 [0.964, 0.970] | same |
| phasefield_staggered_tet4 | solid_solve | head-t2 | 3 | 4 | 310.80 ms [309.67 ms, 310.80 ms] | 77.70 ms [77.42 ms, 77.70 ms] | 12950.0 [12902.8, 12950.0] | 0.979 [0.973, 0.983] | same |
| phasefield_staggered_tet4 | solid_solve | head-t4 | 3 | 4 | 308.03 ms [306.88 ms, 308.56 ms] | 77.01 ms [76.72 ms, 77.14 ms] | 12834.6 [12786.9, 12856.6] | 0.972 [0.965, 0.975] | same |
| phasefield_staggered_tet4 | strain_transfer | base-t1 | 3 | 4 | 415.0 µs [405.0 µs, 425.0 µs] | 103.8 µs [101.2 µs, 106.2 µs] | 17.3 [16.9, 17.7] | – | same |
| phasefield_staggered_tet4 | strain_transfer | base-t2 | 3 | 4 | 417.0 µs [416.0 µs, 424.0 µs] | 104.2 µs [104.0 µs, 106.0 µs] | 17.4 [17.3, 17.7] | 1.005 [0.979, 1.047] | same |
| phasefield_staggered_tet4 | strain_transfer | base-t4 | 3 | 4 | 432.0 µs [414.0 µs, 436.0 µs] | 108.0 µs [103.5 µs, 109.0 µs] | 18.0 [17.2, 18.2] | 1.026 [0.998, 1.067] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t2 | 3 | 4 | 426.0 µs [422.0 µs, 442.0 µs] | 106.5 µs [105.5 µs, 110.5 µs] | 17.8 [17.6, 18.4] | 1.040 [1.017, 1.052] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t4 | 3 | 4 | 424.0 µs [424.0 µs, 432.0 µs] | 106.0 µs [106.0 µs, 108.0 µs] | 17.7 [17.7, 18.0] | 1.022 [0.998, 1.067] | same |
| phasefield_staggered_tet4 | phase_solve | base-t1 | 3 | 4 | 42.52 ms [42.39 ms, 42.61 ms] | 10.63 ms [10.60 ms, 10.65 ms] | 1771.7 [1766.4, 1775.3] | – | same |
| phasefield_staggered_tet4 | phase_solve | base-t2 | 3 | 4 | 41.94 ms [41.88 ms, 42.00 ms] | 10.48 ms [10.47 ms, 10.50 ms] | 1747.4 [1745.0, 1750.1] | 0.986 [0.985, 0.989] | same |
| phasefield_staggered_tet4 | phase_solve | base-t4 | 3 | 4 | 42.46 ms [42.40 ms, 42.57 ms] | 10.61 ms [10.60 ms, 10.64 ms] | 1769.1 [1766.8, 1773.9] | 0.997 [0.996, 1.004] | same |
| phasefield_staggered_tet4 | phase_solve | head-t2 | 3 | 4 | 42.05 ms [42.04 ms, 42.11 ms] | 10.51 ms [10.51 ms, 10.53 ms] | 1752.0 [1751.8, 1754.7] | 0.990 [0.987, 0.992] | same |
| phasefield_staggered_tet4 | phase_solve | head-t4 | 3 | 4 | 42.27 ms [42.27 ms, 42.35 ms] | 10.57 ms [10.57 ms, 10.59 ms] | 1761.4 [1761.2, 1764.7] | 0.994 [0.992, 0.999] | same |
| phasefield_staggered_tet4 | damage_transfer | base-t1 | 3 | 4 | 1.06 ms [1.06 ms, 1.06 ms] | 265.8 µs [264.8 µs, 266.0 µs] | 44.3 [44.1, 44.3] | – | same |
| phasefield_staggered_tet4 | damage_transfer | base-t2 | 3 | 4 | 1.06 ms [1.05 ms, 1.07 ms] | 266.2 µs [263.0 µs, 266.5 µs] | 44.4 [43.8, 44.4] | 1.002 [0.990, 1.006] | same |
| phasefield_staggered_tet4 | damage_transfer | base-t4 | 3 | 4 | 1.06 ms [1.05 ms, 1.06 ms] | 264.5 µs [262.2 µs, 265.8 µs] | 44.1 [43.7, 44.3] | 0.999 [0.987, 0.999] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t2 | 3 | 4 | 1.06 ms [1.05 ms, 1.06 ms] | 264.8 µs [263.8 µs, 265.8 µs] | 44.1 [44.0, 44.3] | 0.996 [0.996, 0.999] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t4 | 3 | 4 | 1.06 ms [1.06 ms, 1.06 ms] | 265.0 µs [264.2 µs, 265.5 µs] | 44.2 [44.0, 44.2] | 0.997 [0.993, 1.003] | same |
| phasefield_staggered_tet4 | internal_forces | base-t1 | 3 | 4 | 1.10 ms [1.10 ms, 1.12 ms] | 276.2 µs [274.5 µs, 280.0 µs] | 46.0 [45.8, 46.7] | – | same |
| phasefield_staggered_tet4 | internal_forces | base-t2 | 3 | 4 | 710.0 µs [705.0 µs, 723.0 µs] | 177.5 µs [176.2 µs, 180.8 µs] | 29.6 [29.4, 30.1] | 0.642 [0.634, 0.654] | same |
| phasefield_staggered_tet4 | internal_forces | base-t4 | 3 | 4 | 649.0 µs [641.0 µs, 652.0 µs] | 162.2 µs [160.2 µs, 163.0 µs] | 27.0 [26.7, 27.2] | 0.580 [0.579, 0.594] | same |
| phasefield_staggered_tet4 | internal_forces | head-t2 | 3 | 4 | 715.0 µs [712.0 µs, 719.0 µs] | 178.8 µs [178.0 µs, 179.8 µs] | 29.8 [29.7, 30.0] | 0.651 [0.636, 0.651] | same |
| phasefield_staggered_tet4 | internal_forces | head-t4 | 3 | 4 | 644.0 µs [618.0 µs, 655.0 µs] | 161.0 µs [154.5 µs, 163.8 µs] | 26.8 [25.8, 27.3] | 0.575 [0.559, 0.597] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t1 | 3 | 5 | 16.04 ms [15.94 ms, 16.13 ms] | 3.21 ms [3.19 ms, 3.23 ms] | 534.7 [531.4, 537.7] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t2 | 3 | 5 | 8.16 ms [8.14 ms, 8.23 ms] | 1.63 ms [1.63 ms, 1.65 ms] | 272.1 [271.5, 274.5] | 0.509 [0.505, 0.517] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t4 | 3 | 5 | 5.28 ms [5.28 ms, 5.30 ms] | 1.06 ms [1.06 ms, 1.06 ms] | 176.1 [176.0, 176.6] | 0.330 [0.328, 0.331] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t2 | 3 | 5 | 8.14 ms [8.13 ms, 8.16 ms] | 1.63 ms [1.63 ms, 1.63 ms] | 271.4 [270.9, 271.8] | 0.507 [0.506, 0.511] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t4 | 3 | 5 | 5.27 ms [5.26 ms, 5.28 ms] | 1.05 ms [1.05 ms, 1.06 ms] | 175.7 [175.2, 176.1] | 0.329 [0.326, 0.331] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t1 | 3 | 5 | 3.23 ms [3.22 ms, 3.25 ms] | 645.6 µs [644.6 µs, 649.2 µs] | 107.6 [107.4, 108.2] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t2 | 3 | 5 | 2.95 ms [2.91 ms, 2.95 ms] | 589.2 µs [582.8 µs, 590.0 µs] | 98.2 [97.1, 98.3] | 0.914 [0.898, 0.914] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t4 | 3 | 5 | 3.24 ms [3.18 ms, 3.25 ms] | 648.0 µs [636.8 µs, 649.4 µs] | 108.0 [106.1, 108.2] | 1.000 [0.988, 1.004] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t2 | 3 | 5 | 2.93 ms [2.93 ms, 2.94 ms] | 586.6 µs [585.0 µs, 588.8 µs] | 97.8 [97.5, 98.1] | 0.907 [0.906, 0.910] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t4 | 3 | 5 | 3.20 ms [3.06 ms, 3.22 ms] | 639.2 µs [612.6 µs, 643.4 µs] | 106.5 [102.1, 107.2] | 0.985 [0.950, 0.997] | same |
| phasefield_staggered_tri3 | mesh | base-t1 | 3 | 1 | 25.48 ms [25.43 ms, 25.60 ms] | 25.48 ms [25.43 ms, 25.60 ms] | 497.7 [496.6, 500.0] | – | same |
| phasefield_staggered_tri3 | mesh | base-t2 | 3 | 1 | 25.38 ms [25.34 ms, 25.55 ms] | 25.38 ms [25.34 ms, 25.55 ms] | 495.7 [495.0, 499.0] | 0.996 [0.990, 1.005] | same |
| phasefield_staggered_tri3 | mesh | base-t4 | 3 | 1 | 25.43 ms [25.26 ms, 25.44 ms] | 25.43 ms [25.26 ms, 25.44 ms] | 496.7 [493.4, 496.8] | 0.993 [0.991, 1.000] | same |
| phasefield_staggered_tri3 | mesh | head-t2 | 3 | 1 | 25.43 ms [25.19 ms, 25.44 ms] | 25.43 ms [25.19 ms, 25.44 ms] | 496.6 [492.0, 496.8] | 0.994 [0.989, 1.000] | same |
| phasefield_staggered_tri3 | mesh | head-t4 | 3 | 1 | 25.35 ms [25.24 ms, 25.62 ms] | 25.35 ms [25.24 ms, 25.62 ms] | 495.1 [492.9, 500.4] | 0.993 [0.990, 1.005] | same |
| phasefield_staggered_tri3 | init | base-t1 | 3 | 1 | 9.23 ms [9.21 ms, 9.42 ms] | 9.23 ms [9.21 ms, 9.42 ms] | 180.4 [179.9, 184.1] | – | same |
| phasefield_staggered_tri3 | init | base-t2 | 3 | 1 | 9.22 ms [9.01 ms, 9.44 ms] | 9.22 ms [9.01 ms, 9.44 ms] | 180.0 [176.0, 184.3] | 0.979 [0.978, 1.022] | same |
| phasefield_staggered_tri3 | init | base-t4 | 3 | 1 | 9.12 ms [9.09 ms, 9.34 ms] | 9.12 ms [9.09 ms, 9.34 ms] | 178.1 [177.5, 182.5] | 0.984 [0.968, 1.015] | same |
| phasefield_staggered_tri3 | init | head-t2 | 3 | 1 | 9.32 ms [9.28 ms, 9.57 ms] | 9.32 ms [9.28 ms, 9.57 ms] | 182.1 [181.2, 186.9] | 1.004 [0.989, 1.039] | same |
| phasefield_staggered_tri3 | init | head-t4 | 3 | 1 | 9.31 ms [9.17 ms, 9.63 ms] | 9.31 ms [9.17 ms, 9.63 ms] | 181.9 [179.1, 188.2] | 1.011 [0.993, 1.022] | same |
| phasefield_staggered_tri3 | solid_solve | base-t1 | 3 | 4 | 1.889 s [1.881 s, 1.889 s] | 472.27 ms [470.34 ms, 472.32 ms] | 9224.0 [9186.4, 9225.0] | – | same |
| phasefield_staggered_tri3 | solid_solve | base-t2 | 3 | 4 | 1.797 s [1.796 s, 1.808 s] | 449.21 ms [449.06 ms, 452.10 ms] | 8773.6 [8770.7, 8830.0] | 0.955 [0.951, 0.957] | same |
| phasefield_staggered_tri3 | solid_solve | base-t4 | 3 | 4 | 1.881 s [1.877 s, 1.890 s] | 470.18 ms [469.21 ms, 472.40 ms] | 9183.1 [9164.2, 9226.5] | 0.998 [0.996, 1.000] | same |
| phasefield_staggered_tri3 | solid_solve | head-t2 | 3 | 4 | 1.797 s [1.794 s, 1.799 s] | 449.29 ms [448.57 ms, 449.64 ms] | 8775.2 [8761.2, 8782.0] | 0.952 [0.950, 0.955] | same |
| phasefield_staggered_tri3 | solid_solve | head-t4 | 3 | 4 | 1.833 s [1.829 s, 1.833 s] | 458.18 ms [457.19 ms, 458.27 ms] | 8948.8 [8929.4, 8950.5] | 0.970 [0.968, 0.974] | same |
| phasefield_staggered_tri3 | strain_transfer | base-t1 | 3 | 4 | 2.31 ms [2.31 ms, 2.34 ms] | 578.5 µs [576.8 µs, 585.5 µs] | 11.3 [11.3, 11.4] | – | same |
| phasefield_staggered_tri3 | strain_transfer | base-t2 | 3 | 4 | 2.31 ms [2.29 ms, 2.34 ms] | 576.8 µs [572.0 µs, 585.2 µs] | 11.3 [11.2, 11.4] | 0.997 [0.992, 1.000] | same |
| phasefield_staggered_tri3 | strain_transfer | base-t4 | 3 | 4 | 2.32 ms [2.32 ms, 2.34 ms] | 580.8 µs [580.5 µs, 584.2 µs] | 11.3 [11.3, 11.4] | 1.003 [0.992, 1.013] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t2 | 3 | 4 | 2.40 ms [2.39 ms, 2.46 ms] | 600.0 µs [597.0 µs, 613.8 µs] | 11.7 [11.7, 12.0] | 1.040 [1.032, 1.048] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t4 | 3 | 4 | 2.44 ms [2.38 ms, 2.46 ms] | 610.2 µs [596.2 µs, 615.5 µs] | 11.9 [11.6, 12.0] | 1.055 [1.018, 1.067] | same |
| phasefield_staggered_tri3 | phase_solve | base-t1 | 3 | 4 | 510.91 ms [510.63 ms, 513.97 ms] | 127.73 ms [127.66 ms, 128.49 ms] | 2494.7 [2493.3, 2509.6] | – | same |
| phasefield_staggered_tri3 | phase_solve | base-t2 | 3 | 4 | 487.50 ms [486.14 ms, 487.89 ms] | 121.88 ms [121.54 ms, 121.97 ms] | 2380.4 [2373.8, 2382.3] | 0.955 [0.946, 0.955] | same |
| phasefield_staggered_tri3 | phase_solve | base-t4 | 3 | 4 | 482.19 ms [480.41 ms, 482.42 ms] | 120.55 ms [120.10 ms, 120.60 ms] | 2354.5 [2345.7, 2355.6] | 0.940 [0.938, 0.945] | same |
| phasefield_staggered_tri3 | phase_solve | head-t2 | 3 | 4 | 486.54 ms [485.18 ms, 489.37 ms] | 121.63 ms [121.30 ms, 122.34 ms] | 2375.7 [2369.1, 2389.5] | 0.952 [0.944, 0.958] | same |
| phasefield_staggered_tri3 | phase_solve | head-t4 | 3 | 4 | 480.30 ms [479.96 ms, 480.47 ms] | 120.07 ms [119.99 ms, 120.12 ms] | 2345.2 [2343.6, 2346.0] | 0.939 [0.934, 0.941] | same |
| phasefield_staggered_tri3 | damage_transfer | base-t1 | 3 | 4 | 8.00 ms [7.98 ms, 8.01 ms] | 2.00 ms [1.99 ms, 2.00 ms] | 39.1 [39.0, 39.1] | – | same |
| phasefield_staggered_tri3 | damage_transfer | base-t2 | 3 | 4 | 8.05 ms [8.00 ms, 8.06 ms] | 2.01 ms [2.00 ms, 2.01 ms] | 39.3 [39.0, 39.3] | 1.007 [0.999, 1.008] | same |
| phasefield_staggered_tri3 | damage_transfer | base-t4 | 3 | 4 | 8.02 ms [8.01 ms, 8.09 ms] | 2.00 ms [2.00 ms, 2.02 ms] | 39.1 [39.1, 39.5] | 1.004 [1.001, 1.011] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t2 | 3 | 4 | 8.00 ms [7.97 ms, 8.12 ms] | 2.00 ms [1.99 ms, 2.03 ms] | 39.1 [38.9, 39.6] | 1.003 [0.995, 1.013] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t4 | 3 | 4 | 8.02 ms [8.01 ms, 8.03 ms] | 2.01 ms [2.00 ms, 2.01 ms] | 39.2 [39.1, 39.2] | 1.002 [1.001, 1.006] | same |
| phasefield_staggered_tri3 | internal_forces | base-t1 | 3 | 4 | 4.95 ms [4.89 ms, 5.05 ms] | 1.24 ms [1.22 ms, 1.26 ms] | 24.2 [23.9, 24.7] | – | same |
| phasefield_staggered_tri3 | internal_forces | base-t2 | 3 | 4 | 2.81 ms [2.80 ms, 2.86 ms] | 701.8 µs [700.0 µs, 715.2 µs] | 13.7 [13.7, 14.0] | 0.567 [0.566, 0.572] | same |
| phasefield_staggered_tri3 | internal_forces | base-t4 | 3 | 4 | 2.38 ms [2.33 ms, 2.41 ms] | 596.0 µs [582.2 µs, 603.5 µs] | 11.6 [11.4, 11.8] | 0.478 [0.476, 0.481] | same |
| phasefield_staggered_tri3 | internal_forces | head-t2 | 3 | 4 | 2.63 ms [2.60 ms, 2.66 ms] | 657.8 µs [649.5 µs, 665.5 µs] | 12.8 [12.7, 13.0] | 0.538 [0.514, 0.538] | same |
| phasefield_staggered_tri3 | internal_forces | head-t4 | 3 | 4 | 2.19 ms [2.18 ms, 2.24 ms] | 546.8 µs [546.0 µs, 559.8 µs] | 10.7 [10.7, 10.9] | 0.447 [0.432, 0.452] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t1 | 3 | 5 | 29.69 ms [28.89 ms, 29.77 ms] | 5.94 ms [5.78 ms, 5.95 ms] | 116.0 [112.9, 116.3] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t2 | 3 | 5 | 16.56 ms [16.55 ms, 16.78 ms] | 3.31 ms [3.31 ms, 3.36 ms] | 64.7 [64.6, 65.5] | 0.564 [0.557, 0.573] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t4 | 3 | 5 | 9.57 ms [9.53 ms, 9.57 ms] | 1.91 ms [1.91 ms, 1.91 ms] | 37.4 [37.2, 37.4] | 0.322 [0.322, 0.330] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t2 | 3 | 5 | 16.21 ms [16.02 ms, 16.22 ms] | 3.24 ms [3.20 ms, 3.24 ms] | 63.3 [62.6, 63.4] | 0.545 [0.540, 0.561] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t4 | 3 | 5 | 9.28 ms [9.28 ms, 9.34 ms] | 1.86 ms [1.86 ms, 1.87 ms] | 36.3 [36.2, 36.5] | 0.315 [0.312, 0.321] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t1 | 3 | 5 | 22.06 ms [22.05 ms, 22.13 ms] | 4.41 ms [4.41 ms, 4.43 ms] | 86.2 [86.1, 86.4] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t2 | 3 | 5 | 13.78 ms [13.65 ms, 13.80 ms] | 2.76 ms [2.73 ms, 2.76 ms] | 53.8 [53.3, 53.9] | 0.624 [0.619, 0.625] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t4 | 3 | 5 | 8.33 ms [8.32 ms, 8.40 ms] | 1.67 ms [1.66 ms, 1.68 ms] | 32.6 [32.5, 32.8] | 0.378 [0.376, 0.381] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t2 | 3 | 5 | 13.40 ms [13.34 ms, 13.51 ms] | 2.68 ms [2.67 ms, 2.70 ms] | 52.4 [52.1, 52.8] | 0.608 [0.605, 0.610] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t4 | 3 | 5 | 8.33 ms [8.29 ms, 8.46 ms] | 1.67 ms [1.66 ms, 1.69 ms] | 32.5 [32.4, 33.0] | 0.377 [0.376, 0.382] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | 7365a08f4, f8babbe8e | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 7365a08f4, f8babbe8e | b4798ad9264a3e5e |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 7365a08f4, f8babbe8e | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 7365a08f4, f8babbe8e | a77809465b3e7c18 |
