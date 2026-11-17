Suite `models`, size `m`; variants: `base-t1`, `base-t8`, `head-t1`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_quad4 | mesh | base-t1 | 5 | 1 | 105.49 ms [105.33 ms, 105.56 ms] | 105.49 ms [105.33 ms, 105.56 ms] | 659.3 [658.3, 659.7] | – | same |
| heat_explicit_quad4 | mesh | base-t8 | 5 | 1 | 105.57 ms [104.53 ms, 106.15 ms] | 105.57 ms [104.53 ms, 106.15 ms] | 659.8 [653.3, 663.5] | 1.001 [0.990, 1.007] | same |
| heat_explicit_quad4 | mesh | head-t1 | 5 | 1 | 105.55 ms [104.52 ms, 105.79 ms] | 105.55 ms [104.52 ms, 105.79 ms] | 659.7 [653.3, 661.2] | 1.002 [0.990, 1.004] | same |
| heat_explicit_quad4 | mesh | head-t8 | 5 | 1 | 104.91 ms [104.46 ms, 107.75 ms] | 104.91 ms [104.46 ms, 107.75 ms] | 655.7 [652.9, 673.4] | 0.995 [0.990, 1.021] | same |
| heat_explicit_quad4 | init | base-t1 | 5 | 1 | 16.09 ms [15.57 ms, 16.44 ms] | 16.09 ms [15.57 ms, 16.44 ms] | 100.5 [97.3, 102.7] | – | same |
| heat_explicit_quad4 | init | base-t8 | 5 | 1 | 15.94 ms [15.46 ms, 16.42 ms] | 15.94 ms [15.46 ms, 16.42 ms] | 99.6 [96.6, 102.6] | 0.994 [0.961, 1.016] | same |
| heat_explicit_quad4 | init | head-t1 | 5 | 1 | 15.55 ms [15.40 ms, 15.74 ms] | 15.55 ms [15.40 ms, 15.74 ms] | 97.2 [96.3, 98.4] | 0.961 [0.955, 1.011] | same |
| heat_explicit_quad4 | init | head-t8 | 5 | 1 | 15.86 ms [15.74 ms, 16.45 ms] | 15.86 ms [15.74 ms, 16.45 ms] | 99.1 [98.4, 102.8] | 0.996 [0.979, 1.019] | same |
| heat_explicit_quad4 | stable_time_step | base-t1 | 5 | 1 | 2.57 ms [2.56 ms, 2.77 ms] | 2.57 ms [2.56 ms, 2.77 ms] | 16.1 [16.0, 17.3] | – | same |
| heat_explicit_quad4 | stable_time_step | base-t8 | 5 | 1 | 2.45 ms [2.44 ms, 2.60 ms] | 2.45 ms [2.44 ms, 2.60 ms] | 15.3 [15.3, 16.2] | 0.953 [0.915, 1.013] | same |
| heat_explicit_quad4 | stable_time_step | head-t1 | 5 | 1 | 2.55 ms [2.54 ms, 2.68 ms] | 2.55 ms [2.54 ms, 2.68 ms] | 15.9 [15.9, 16.8] | 0.991 [0.920, 1.046] | same |
| heat_explicit_quad4 | stable_time_step | head-t8 | 5 | 1 | 2.56 ms [2.46 ms, 2.73 ms] | 2.56 ms [2.46 ms, 2.73 ms] | 16.0 [15.4, 17.1] | 0.987 [0.961, 1.005] | same |
| heat_explicit_quad4 | solve_step | base-t1 | 5 | 120 | 1.193 s [1.189 s, 1.203 s] | 9.94 ms [9.91 ms, 10.03 ms] | 62.1 [61.9, 62.7] | – | same |
| heat_explicit_quad4 | solve_step | base-t8 | 5 | 120 | 576.73 ms [574.75 ms, 579.74 ms] | 4.81 ms [4.79 ms, 4.83 ms] | 30.0 [29.9, 30.2] | 0.485 [0.479, 0.486] | same |
| heat_explicit_quad4 | solve_step | head-t1 | 5 | 120 | 1.191 s [1.188 s, 1.197 s] | 9.92 ms [9.90 ms, 9.98 ms] | 62.0 [61.9, 62.4] | 0.998 [0.988, 1.004] | same |
| heat_explicit_quad4 | solve_step | head-t8 | 5 | 120 | 496.30 ms [474.90 ms, 498.64 ms] | 4.14 ms [3.96 ms, 4.16 ms] | 25.8 [24.7, 26.0] | 0.416 [0.398, 0.418] | same |
| heat_explicit_tet4 | mesh | base-t1 | 5 | 1 | 151.54 ms [150.01 ms, 153.25 ms] | 151.54 ms [150.01 ms, 153.25 ms] | 541.3 [535.9, 547.5] | – | same |
| heat_explicit_tet4 | mesh | base-t8 | 5 | 1 | 151.17 ms [149.55 ms, 151.96 ms] | 151.17 ms [149.55 ms, 151.96 ms] | 540.0 [534.2, 542.8] | 0.996 [0.987, 1.008] | same |
| heat_explicit_tet4 | mesh | head-t1 | 5 | 1 | 151.10 ms [149.89 ms, 151.93 ms] | 151.10 ms [149.89 ms, 151.93 ms] | 539.8 [535.4, 542.7] | 0.996 [0.983, 1.007] | same |
| heat_explicit_tet4 | mesh | head-t8 | 5 | 1 | 152.31 ms [152.03 ms, 153.37 ms] | 152.31 ms [152.03 ms, 153.37 ms] | 544.1 [543.1, 547.9] | 1.004 [1.001, 1.013] | same |
| heat_explicit_tet4 | init | base-t1 | 5 | 1 | 10.18 ms [10.06 ms, 10.33 ms] | 10.18 ms [10.06 ms, 10.33 ms] | 36.4 [35.9, 36.9] | – | same |
| heat_explicit_tet4 | init | base-t8 | 5 | 1 | 10.07 ms [9.95 ms, 10.17 ms] | 10.07 ms [9.95 ms, 10.17 ms] | 36.0 [35.5, 36.3] | 0.991 [0.965, 1.002] | same |
| heat_explicit_tet4 | init | head-t1 | 5 | 1 | 10.09 ms [10.01 ms, 10.17 ms] | 10.09 ms [10.01 ms, 10.17 ms] | 36.1 [35.8, 36.3] | 0.992 [0.970, 1.011] | same |
| heat_explicit_tet4 | init | head-t8 | 5 | 1 | 10.21 ms [10.04 ms, 10.31 ms] | 10.21 ms [10.04 ms, 10.31 ms] | 36.5 [35.9, 36.8] | 0.999 [0.987, 1.025] | same |
| heat_explicit_tet4 | stable_time_step | base-t1 | 5 | 1 | 9.53 ms [9.51 ms, 9.56 ms] | 9.53 ms [9.51 ms, 9.56 ms] | 34.0 [34.0, 34.2] | – | same |
| heat_explicit_tet4 | stable_time_step | base-t8 | 5 | 1 | 9.52 ms [9.51 ms, 9.65 ms] | 9.52 ms [9.51 ms, 9.65 ms] | 34.0 [34.0, 34.5] | 0.999 [0.996, 1.014] | same |
| heat_explicit_tet4 | stable_time_step | head-t1 | 5 | 1 | 9.53 ms [9.51 ms, 9.87 ms] | 9.53 ms [9.51 ms, 9.87 ms] | 34.0 [34.0, 35.3] | 1.000 [0.997, 1.037] | same |
| heat_explicit_tet4 | stable_time_step | head-t8 | 5 | 1 | 9.54 ms [9.54 ms, 9.58 ms] | 9.54 ms [9.54 ms, 9.58 ms] | 34.1 [34.1, 34.2] | 1.002 [1.001, 1.002] | same |
| heat_explicit_tet4 | solve_step | base-t1 | 5 | 100 | 893.85 ms [893.19 ms, 896.58 ms] | 8.94 ms [8.93 ms, 8.97 ms] | 31.9 [31.9, 32.0] | – | same |
| heat_explicit_tet4 | solve_step | base-t8 | 5 | 100 | 315.01 ms [314.51 ms, 316.05 ms] | 3.15 ms [3.15 ms, 3.16 ms] | 11.3 [11.2, 11.3] | 0.352 [0.352, 0.353] | same |
| heat_explicit_tet4 | solve_step | head-t1 | 5 | 100 | 888.07 ms [885.72 ms, 890.35 ms] | 8.88 ms [8.86 ms, 8.90 ms] | 31.7 [31.6, 31.8] | 0.993 [0.992, 0.995] | same |
| heat_explicit_tet4 | solve_step | head-t8 | 5 | 100 | 253.53 ms [253.21 ms, 254.44 ms] | 2.54 ms [2.53 ms, 2.54 ms] | 9.1 [9.0, 9.1] | 0.284 [0.283, 0.285] | same |
| phasefield_staggered_tet4 | mesh | base-t1 | 5 | 1 | 4.05 ms [4.03 ms, 4.06 ms] | 4.05 ms [4.03 ms, 4.06 ms] | 675.0 [672.3, 676.3] | – | same |
| phasefield_staggered_tet4 | mesh | base-t8 | 5 | 1 | 4.05 ms [4.04 ms, 4.06 ms] | 4.05 ms [4.04 ms, 4.06 ms] | 675.3 [673.0, 676.2] | 1.001 [1.000, 1.001] | same |
| phasefield_staggered_tet4 | mesh | head-t1 | 5 | 1 | 3.97 ms [3.95 ms, 4.07 ms] | 3.97 ms [3.95 ms, 4.07 ms] | 662.0 [657.7, 678.5] | 0.979 [0.975, 1.009] | same |
| phasefield_staggered_tet4 | mesh | head-t8 | 5 | 1 | 4.06 ms [4.05 ms, 4.06 ms] | 4.06 ms [4.05 ms, 4.06 ms] | 676.0 [674.3, 676.7] | 1.002 [0.999, 1.005] | same |
| phasefield_staggered_tet4 | init | base-t1 | 5 | 1 | 1.38 ms [1.37 ms, 1.39 ms] | 1.38 ms [1.37 ms, 1.39 ms] | 229.8 [228.8, 231.7] | – | same |
| phasefield_staggered_tet4 | init | base-t8 | 5 | 1 | 1.37 ms [1.36 ms, 1.50 ms] | 1.37 ms [1.36 ms, 1.50 ms] | 228.7 [226.7, 249.2] | 0.999 [0.986, 1.076] | same |
| phasefield_staggered_tet4 | init | head-t1 | 5 | 1 | 1.27 ms [1.22 ms, 1.39 ms] | 1.27 ms [1.22 ms, 1.39 ms] | 212.2 [203.8, 232.2] | 0.923 [0.889, 1.006] | same |
| phasefield_staggered_tet4 | init | head-t8 | 5 | 1 | 1.37 ms [1.31 ms, 1.39 ms] | 1.37 ms [1.31 ms, 1.39 ms] | 228.7 [219.2, 231.7] | 0.994 [0.958, 1.010] | same |
| phasefield_staggered_tet4 | solid_solve | base-t1 | 5 | 4 | 324.90 ms [321.34 ms, 327.23 ms] | 81.23 ms [80.34 ms, 81.81 ms] | 13537.7 [13389.2, 13634.5] | – | same |
| phasefield_staggered_tet4 | solid_solve | base-t8 | 5 | 4 | 306.94 ms [305.54 ms, 309.54 ms] | 76.73 ms [76.39 ms, 77.38 ms] | 12789.0 [12730.9, 12897.4] | 0.946 [0.938, 0.963] | same |
| phasefield_staggered_tet4 | solid_solve | head-t1 | 5 | 4 | 323.36 ms [322.27 ms, 325.62 ms] | 80.84 ms [80.57 ms, 81.41 ms] | 13473.2 [13428.0, 13567.6] | 0.994 [0.985, 1.013] | same |
| phasefield_staggered_tet4 | solid_solve | head-t8 | 5 | 4 | 304.64 ms [303.81 ms, 309.54 ms] | 76.16 ms [75.95 ms, 77.38 ms] | 12693.3 [12658.9, 12897.5] | 0.937 [0.931, 0.963] | same |
| phasefield_staggered_tet4 | strain_transfer | base-t1 | 5 | 4 | 432.0 µs [427.0 µs, 442.0 µs] | 108.0 µs [106.8 µs, 110.5 µs] | 18.0 [17.8, 18.4] | – | same |
| phasefield_staggered_tet4 | strain_transfer | base-t8 | 5 | 4 | 418.0 µs [410.0 µs, 453.0 µs] | 104.5 µs [102.5 µs, 113.2 µs] | 17.4 [17.1, 18.9] | 0.968 [0.943, 1.030] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t1 | 5 | 4 | 455.0 µs [450.0 µs, 458.0 µs] | 113.8 µs [112.5 µs, 114.5 µs] | 19.0 [18.8, 19.1] | 1.044 [1.029, 1.058] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t8 | 5 | 4 | 452.0 µs [447.0 µs, 459.0 µs] | 113.0 µs [111.8 µs, 114.8 µs] | 18.8 [18.6, 19.1] | 1.046 [1.011, 1.065] | same |
| phasefield_staggered_tet4 | phase_solve | base-t1 | 5 | 4 | 46.71 ms [46.59 ms, 47.11 ms] | 11.68 ms [11.65 ms, 11.78 ms] | 1946.1 [1941.2, 1963.0] | – | same |
| phasefield_staggered_tet4 | phase_solve | base-t8 | 5 | 4 | 42.55 ms [42.52 ms, 42.80 ms] | 10.64 ms [10.63 ms, 10.70 ms] | 1772.8 [1771.8, 1783.2] | 0.911 [0.903, 0.917] | same |
| phasefield_staggered_tet4 | phase_solve | head-t1 | 5 | 4 | 46.49 ms [46.40 ms, 46.61 ms] | 11.62 ms [11.60 ms, 11.65 ms] | 1937.0 [1933.3, 1942.0] | 0.993 [0.989, 0.998] | same |
| phasefield_staggered_tet4 | phase_solve | head-t8 | 5 | 4 | 42.45 ms [42.36 ms, 42.65 ms] | 10.61 ms [10.59 ms, 10.66 ms] | 1768.8 [1765.1, 1777.2] | 0.907 [0.901, 0.915] | same |
| phasefield_staggered_tet4 | damage_transfer | base-t1 | 5 | 4 | 1.05 ms [1.05 ms, 1.06 ms] | 262.0 µs [261.5 µs, 265.8 µs] | 43.7 [43.6, 44.3] | – | same |
| phasefield_staggered_tet4 | damage_transfer | base-t8 | 5 | 4 | 1.05 ms [1.05 ms, 1.07 ms] | 263.8 µs [263.2 µs, 268.0 µs] | 44.0 [43.9, 44.7] | 1.006 [0.992, 1.025] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t1 | 5 | 4 | 1.05 ms [1.04 ms, 1.05 ms] | 263.5 µs [261.2 µs, 263.8 µs] | 43.9 [43.5, 44.0] | 1.002 [0.983, 1.009] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t8 | 5 | 4 | 1.05 ms [1.04 ms, 1.05 ms] | 263.0 µs [261.0 µs, 263.8 µs] | 43.8 [43.5, 44.0] | 1.003 [0.986, 1.009] | same |
| phasefield_staggered_tet4 | internal_forces | base-t1 | 5 | 4 | 1.10 ms [1.06 ms, 1.12 ms] | 275.2 µs [264.8 µs, 279.8 µs] | 45.9 [44.1, 46.6] | – | same |
| phasefield_staggered_tet4 | internal_forces | base-t8 | 5 | 4 | 575.0 µs [543.0 µs, 578.0 µs] | 143.8 µs [135.8 µs, 144.5 µs] | 24.0 [22.6, 24.1] | 0.519 [0.493, 0.546] | same |
| phasefield_staggered_tet4 | internal_forces | head-t1 | 5 | 4 | 1.10 ms [1.07 ms, 1.12 ms] | 275.5 µs [267.0 µs, 281.2 µs] | 45.9 [44.5, 46.9] | 1.000 [0.970, 1.054] | same |
| phasefield_staggered_tet4 | internal_forces | head-t8 | 5 | 4 | 576.0 µs [568.0 µs, 591.0 µs] | 144.0 µs [142.0 µs, 147.8 µs] | 24.0 [23.7, 24.6] | 0.523 [0.508, 0.558] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t1 | 5 | 5 | 17.64 ms [17.52 ms, 17.84 ms] | 3.53 ms [3.50 ms, 3.57 ms] | 588.1 [584.0, 594.7] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t8 | 5 | 5 | 3.79 ms [3.55 ms, 3.80 ms] | 757.4 µs [710.4 µs, 760.6 µs] | 126.2 [118.4, 126.8] | 0.213 [0.201, 0.217] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t1 | 5 | 5 | 17.69 ms [17.52 ms, 17.89 ms] | 3.54 ms [3.50 ms, 3.58 ms] | 589.7 [584.0, 596.2] | 1.003 [0.993, 1.010] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t8 | 5 | 5 | 3.77 ms [3.74 ms, 3.80 ms] | 754.2 µs [748.0 µs, 759.0 µs] | 125.7 [124.7, 126.5] | 0.214 [0.211, 0.215] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t1 | 5 | 5 | 5.72 ms [5.71 ms, 5.79 ms] | 1.14 ms [1.14 ms, 1.16 ms] | 190.8 [190.3, 193.0] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t8 | 5 | 5 | 3.79 ms [3.78 ms, 3.82 ms] | 758.6 µs [755.8 µs, 763.2 µs] | 126.4 [126.0, 127.2] | 0.662 [0.658, 0.665] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t1 | 5 | 5 | 5.69 ms [5.66 ms, 5.71 ms] | 1.14 ms [1.13 ms, 1.14 ms] | 189.7 [188.8, 190.2] | 0.992 [0.979, 0.997] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t8 | 5 | 5 | 3.79 ms [3.78 ms, 3.80 ms] | 757.8 µs [755.8 µs, 759.4 µs] | 126.3 [126.0, 126.6] | 0.662 [0.653, 0.665] | same |
| phasefield_staggered_tri3 | mesh | base-t1 | 5 | 1 | 25.31 ms [25.10 ms, 25.51 ms] | 25.31 ms [25.10 ms, 25.51 ms] | 494.3 [490.3, 498.3] | – | same |
| phasefield_staggered_tri3 | mesh | base-t8 | 5 | 1 | 25.17 ms [25.03 ms, 25.24 ms] | 25.17 ms [25.03 ms, 25.24 ms] | 491.6 [488.8, 492.9] | 0.993 [0.981, 1.004] | same |
| phasefield_staggered_tri3 | mesh | head-t1 | 5 | 1 | 25.40 ms [25.20 ms, 25.41 ms] | 25.40 ms [25.20 ms, 25.41 ms] | 496.1 [492.1, 496.3] | 0.997 [0.996, 1.012] | same |
| phasefield_staggered_tri3 | mesh | head-t8 | 5 | 1 | 25.24 ms [25.16 ms, 25.58 ms] | 25.24 ms [25.16 ms, 25.58 ms] | 493.0 [491.5, 499.6] | 1.004 [0.987, 1.011] | same |
| phasefield_staggered_tri3 | init | base-t1 | 5 | 1 | 9.34 ms [8.98 ms, 9.39 ms] | 9.34 ms [8.98 ms, 9.39 ms] | 182.5 [175.5, 183.4] | – | same |
| phasefield_staggered_tri3 | init | base-t8 | 5 | 1 | 9.35 ms [9.19 ms, 9.44 ms] | 9.35 ms [9.19 ms, 9.44 ms] | 182.7 [179.4, 184.3] | 1.003 [0.999, 1.036] | same |
| phasefield_staggered_tri3 | init | head-t1 | 5 | 1 | 9.14 ms [9.04 ms, 9.20 ms] | 9.14 ms [9.04 ms, 9.20 ms] | 178.5 [176.6, 179.7] | 0.983 [0.976, 1.015] | same |
| phasefield_staggered_tri3 | init | head-t8 | 5 | 1 | 9.41 ms [9.04 ms, 9.46 ms] | 9.41 ms [9.04 ms, 9.46 ms] | 183.8 [176.6, 184.8] | 1.010 [0.963, 1.048] | same |
| phasefield_staggered_tri3 | solid_solve | base-t1 | 5 | 4 | 1.825 s [1.821 s, 1.837 s] | 456.36 ms [455.19 ms, 459.37 ms] | 8913.3 [8890.4, 8972.1] | – | same |
| phasefield_staggered_tri3 | solid_solve | base-t8 | 5 | 4 | 1.825 s [1.813 s, 1.828 s] | 456.28 ms [453.22 ms, 456.88 ms] | 8911.6 [8851.9, 8923.4] | 0.998 [0.993, 1.001] | same |
| phasefield_staggered_tri3 | solid_solve | head-t1 | 5 | 4 | 1.829 s [1.827 s, 1.837 s] | 457.27 ms [456.87 ms, 459.37 ms] | 8931.1 [8923.2, 8972.1] | 1.002 [0.997, 1.009] | same |
| phasefield_staggered_tri3 | solid_solve | head-t8 | 5 | 4 | 1.792 s [1.790 s, 1.804 s] | 448.06 ms [447.48 ms, 451.08 ms] | 8751.2 [8739.9, 8810.2] | 0.982 [0.977, 0.987] | same |
| phasefield_staggered_tri3 | strain_transfer | base-t1 | 5 | 4 | 2.38 ms [2.34 ms, 2.47 ms] | 595.8 µs [584.2 µs, 617.0 µs] | 11.6 [11.4, 12.1] | – | same |
| phasefield_staggered_tri3 | strain_transfer | base-t8 | 5 | 4 | 2.41 ms [2.37 ms, 2.45 ms] | 602.8 µs [592.8 µs, 612.0 µs] | 11.8 [11.6, 12.0] | 1.026 [0.962, 1.041] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t1 | 5 | 4 | 2.31 ms [2.27 ms, 2.36 ms] | 578.0 µs [566.8 µs, 591.2 µs] | 11.3 [11.1, 11.5] | 0.951 [0.927, 1.012] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t8 | 5 | 4 | 2.33 ms [2.30 ms, 2.37 ms] | 582.2 µs [576.0 µs, 592.0 µs] | 11.4 [11.2, 11.6] | 0.969 [0.934, 1.013] | same |
| phasefield_staggered_tri3 | phase_solve | base-t1 | 5 | 4 | 524.38 ms [522.76 ms, 525.57 ms] | 131.10 ms [130.69 ms, 131.39 ms] | 2560.5 [2552.6, 2566.3] | – | same |
| phasefield_staggered_tri3 | phase_solve | base-t8 | 5 | 4 | 501.85 ms [498.87 ms, 502.77 ms] | 125.46 ms [124.72 ms, 125.69 ms] | 2450.5 [2435.9, 2454.9] | 0.955 [0.952, 0.959] | same |
| phasefield_staggered_tri3 | phase_solve | head-t1 | 5 | 4 | 522.71 ms [521.64 ms, 523.12 ms] | 130.68 ms [130.41 ms, 130.78 ms] | 2552.3 [2547.1, 2554.3] | 0.997 [0.995, 0.999] | same |
| phasefield_staggered_tri3 | phase_solve | head-t8 | 5 | 4 | 491.56 ms [490.44 ms, 492.18 ms] | 122.89 ms [122.61 ms, 123.04 ms] | 2400.2 [2394.7, 2403.2] | 0.937 [0.935, 0.940] | same |
| phasefield_staggered_tri3 | damage_transfer | base-t1 | 5 | 4 | 7.97 ms [7.94 ms, 8.00 ms] | 1.99 ms [1.99 ms, 2.00 ms] | 38.9 [38.8, 39.0] | – | same |
| phasefield_staggered_tri3 | damage_transfer | base-t8 | 5 | 4 | 8.00 ms [7.97 ms, 8.02 ms] | 2.00 ms [1.99 ms, 2.01 ms] | 39.1 [38.9, 39.2] | 1.001 [0.999, 1.010] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t1 | 5 | 4 | 7.93 ms [7.92 ms, 7.94 ms] | 1.98 ms [1.98 ms, 1.99 ms] | 38.7 [38.7, 38.8] | 0.995 [0.991, 0.998] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t8 | 5 | 4 | 7.96 ms [7.93 ms, 8.01 ms] | 1.99 ms [1.98 ms, 2.00 ms] | 38.9 [38.7, 39.1] | 1.000 [0.992, 1.005] | same |
| phasefield_staggered_tri3 | internal_forces | base-t1 | 5 | 4 | 4.80 ms [4.75 ms, 4.87 ms] | 1.20 ms [1.19 ms, 1.22 ms] | 23.4 [23.2, 23.8] | – | same |
| phasefield_staggered_tri3 | internal_forces | base-t8 | 5 | 4 | 2.01 ms [1.89 ms, 2.06 ms] | 501.5 µs [472.0 µs, 514.5 µs] | 9.8 [9.2, 10.0] | 0.419 [0.397, 0.425] | same |
| phasefield_staggered_tri3 | internal_forces | head-t1 | 5 | 4 | 4.80 ms [4.77 ms, 4.80 ms] | 1.20 ms [1.19 ms, 1.20 ms] | 23.4 [23.3, 23.4] | 0.993 [0.985, 1.010] | same |
| phasefield_staggered_tri3 | internal_forces | head-t8 | 5 | 4 | 1.78 ms [1.75 ms, 1.89 ms] | 445.2 µs [437.8 µs, 472.0 µs] | 8.7 [8.5, 9.2] | 0.372 [0.366, 0.393] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t1 | 5 | 5 | 41.15 ms [41.13 ms, 41.98 ms] | 8.23 ms [8.23 ms, 8.40 ms] | 160.7 [160.7, 164.0] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t8 | 5 | 5 | 10.92 ms [10.84 ms, 10.96 ms] | 2.18 ms [2.17 ms, 2.19 ms] | 42.7 [42.3, 42.8] | 0.265 [0.260, 0.266] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t1 | 5 | 5 | 41.15 ms [40.90 ms, 41.47 ms] | 8.23 ms [8.18 ms, 8.29 ms] | 160.8 [159.8, 162.0] | 1.000 [0.974, 1.008] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t8 | 5 | 5 | 10.73 ms [10.65 ms, 10.88 ms] | 2.15 ms [2.13 ms, 2.18 ms] | 41.9 [41.6, 42.5] | 0.259 [0.258, 0.264] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t1 | 5 | 5 | 41.73 ms [41.60 ms, 42.26 ms] | 8.35 ms [8.32 ms, 8.45 ms] | 163.0 [162.5, 165.1] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t8 | 5 | 5 | 24.75 ms [24.65 ms, 24.78 ms] | 4.95 ms [4.93 ms, 4.96 ms] | 96.7 [96.3, 96.8] | 0.593 [0.583, 0.595] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t1 | 5 | 5 | 41.42 ms [41.20 ms, 41.56 ms] | 8.28 ms [8.24 ms, 8.31 ms] | 161.8 [160.9, 162.4] | 0.995 [0.979, 0.999] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t8 | 5 | 5 | 24.14 ms [24.06 ms, 24.30 ms] | 4.83 ms [4.81 ms, 4.86 ms] | 94.3 [94.0, 94.9] | 0.578 [0.573, 0.581] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | 7a6ad8f42, 85d4d74d4 | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 7a6ad8f42, 85d4d74d4 | b4798ad9264a3e5e |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 7a6ad8f42, 85d4d74d4 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 7a6ad8f42, 85d4d74d4 | a77809465b3e7c18 |
