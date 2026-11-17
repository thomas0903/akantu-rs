Suite `solid`, size `m`; variants: `base-t1`, `base-t8`, `head5-t1`, `head5-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_hex8_fd | mesh | base-t1 | 5 | 1 | 6.42 ms [6.19 ms, 6.48 ms] | 6.42 ms [6.19 ms, 6.48 ms] | 58.0 [56.0, 58.6] | – | same |
| explicit_hex8_fd | mesh | base-t8 | 5 | 1 | 6.42 ms [6.13 ms, 6.92 ms] | 6.42 ms [6.13 ms, 6.92 ms] | 58.1 [55.5, 62.6] | 1.001 [0.982, 1.078] | same |
| explicit_hex8_fd | mesh | head5-t1 | 5 | 1 | 6.31 ms [6.29 ms, 6.51 ms] | 6.31 ms [6.29 ms, 6.51 ms] | 57.1 [56.8, 58.9] | 0.984 [0.970, 1.052] | same |
| explicit_hex8_fd | mesh | head5-t8 | 5 | 1 | 6.29 ms [6.22 ms, 6.45 ms] | 6.29 ms [6.22 ms, 6.45 ms] | 56.9 [56.2, 58.3] | 1.000 [0.969, 1.036] | same |
| explicit_hex8_fd | init | base-t1 | 5 | 1 | 294.78 ms [288.90 ms, 296.58 ms] | 294.78 ms [288.90 ms, 296.58 ms] | 2665.5 [2612.3, 2681.7] | – | same |
| explicit_hex8_fd | init | base-t8 | 5 | 1 | 298.98 ms [294.06 ms, 300.04 ms] | 298.98 ms [294.06 ms, 300.04 ms] | 2703.5 [2659.0, 2713.0] | 1.012 [1.012, 1.033] | same |
| explicit_hex8_fd | init | head5-t1 | 5 | 1 | 296.80 ms [291.99 ms, 309.65 ms] | 296.80 ms [291.99 ms, 309.65 ms] | 2683.8 [2640.3, 2799.9] | 1.017 [1.001, 1.048] | same |
| explicit_hex8_fd | init | head5-t8 | 5 | 1 | 306.45 ms [291.01 ms, 311.82 ms] | 306.45 ms [291.01 ms, 311.82 ms] | 2771.0 [2631.4, 2819.6] | 1.033 [0.987, 1.072] | same |
| explicit_hex8_fd | stable_time_step | base-t1 | 5 | 5 | 23.61 ms [23.57 ms, 25.20 ms] | 4.72 ms [4.71 ms, 5.04 ms] | 42.7 [42.6, 45.6] | – | same |
| explicit_hex8_fd | stable_time_step | base-t8 | 5 | 5 | 26.22 ms [26.16 ms, 28.25 ms] | 5.24 ms [5.23 ms, 5.65 ms] | 47.4 [47.3, 51.1] | 1.111 [1.038, 1.197] | same |
| explicit_hex8_fd | stable_time_step | head5-t1 | 5 | 5 | 23.15 ms [23.12 ms, 24.75 ms] | 4.63 ms [4.62 ms, 4.95 ms] | 41.9 [41.8, 44.8] | 0.980 [0.919, 1.050] | same |
| explicit_hex8_fd | stable_time_step | head5-t8 | 5 | 5 | 26.24 ms [25.28 ms, 27.83 ms] | 5.25 ms [5.06 ms, 5.57 ms] | 47.4 [45.7, 50.3] | 1.073 [1.057, 1.179] | same |
| explicit_hex8_fd | solve_step | base-t1 | 5 | 30 | 1.312 s [1.308 s, 1.319 s] | 43.74 ms [43.59 ms, 43.97 ms] | 395.5 [394.1, 397.6] | – | same |
| explicit_hex8_fd | solve_step | base-t8 | 5 | 30 | 357.44 ms [356.11 ms, 359.22 ms] | 11.91 ms [11.87 ms, 11.97 ms] | 107.7 [107.3, 108.3] | 0.272 [0.271, 0.273] | same |
| explicit_hex8_fd | solve_step | head5-t1 | 5 | 30 | 1.322 s [1.319 s, 1.327 s] | 44.08 ms [43.98 ms, 44.23 ms] | 398.6 [397.7, 399.9] | 1.009 [1.004, 1.010] | same |
| explicit_hex8_fd | solve_step | head5-t8 | 5 | 30 | 317.92 ms [317.05 ms, 318.67 ms] | 10.60 ms [10.57 ms, 10.62 ms] | 95.8 [95.6, 96.1] | 0.242 [0.242, 0.243] | same |
| explicit_hex8_fd | predictor | base-t1 | 5 | 30 | 4.7 µs [4.6 µs, 5.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | predictor | base-t8 | 5 | 30 | 4.4 µs [3.0 µs, 4.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.925 [0.635, 0.963] | same |
| explicit_hex8_fd | predictor | head5-t1 | 5 | 30 | 4.6 µs [4.5 µs, 6.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.991 [0.926, 1.256] | same |
| explicit_hex8_fd | predictor | head5-t8 | 5 | 30 | 3.1 µs [2.9 µs, 4.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.653 [0.619, 0.970] | same |
| explicit_hex8_fd | residual | base-t1 | 5 | 30 | 1.254 s [1.250 s, 1.260 s] | 41.80 ms [41.67 ms, 42.01 ms] | 378.0 [376.8, 379.9] | – | same |
| explicit_hex8_fd | residual | base-t8 | 5 | 30 | 299.45 ms [298.18 ms, 301.20 ms] | 9.98 ms [9.94 ms, 10.04 ms] | 90.3 [89.9, 90.8] | 0.239 [0.238, 0.240] | same |
| explicit_hex8_fd | residual | head5-t1 | 5 | 30 | 1.259 s [1.256 s, 1.263 s] | 41.95 ms [41.88 ms, 42.11 ms] | 379.3 [378.7, 380.8] | 1.005 [1.000, 1.006] | same |
| explicit_hex8_fd | residual | head5-t8 | 5 | 30 | 285.07 ms [283.98 ms, 286.03 ms] | 9.50 ms [9.47 ms, 9.53 ms] | 85.9 [85.6, 86.2] | 0.227 [0.227, 0.228] | same |
| explicit_hex8_fd | lumped_matrix | base-t1 | 5 | 30 | 9.3 µs [7.8 µs, 10.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | lumped_matrix | base-t8 | 5 | 30 | 11.7 µs [8.8 µs, 12.6 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.237 [0.946, 1.506] | same |
| explicit_hex8_fd | lumped_matrix | head5-t1 | 5 | 30 | 8.1 µs [7.5 µs, 15.7 µs] | 0.3 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.042 [0.743, 1.497] | same |
| explicit_hex8_fd | lumped_matrix | head5-t8 | 5 | 30 | 8.4 µs [7.6 µs, 9.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.906 [0.784, 1.175] | same |
| explicit_hex8_fd | corrector | base-t1 | 5 | 30 | 4.8 µs [4.3 µs, 6.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | corrector | base-t8 | 5 | 30 | 5.0 µs [4.2 µs, 5.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.965 [0.869, 1.259] | same |
| explicit_hex8_fd | corrector | head5-t1 | 5 | 30 | 5.2 µs [4.4 µs, 7.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.050 [0.832, 1.581] | same |
| explicit_hex8_fd | corrector | head5-t8 | 5 | 30 | 6.8 µs [4.8 µs, 8.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.387 [1.015, 1.853] | same |
| explicit_hex8_fd | step_hooks | base-t1 | 5 | 60 | 115.8 µs [111.6 µs, 122.3 µs] | 1.9 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | step_hooks | base-t8 | 5 | 60 | 120.2 µs [114.5 µs, 122.9 µs] | 2.0 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 1.048 [0.938, 1.078] | same |
| explicit_hex8_fd | step_hooks | head5-t1 | 5 | 60 | 117.9 µs [114.0 µs, 122.7 µs] | 2.0 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 1.022 [0.951, 1.060] | same |
| explicit_hex8_fd | step_hooks | head5-t8 | 5 | 60 | 133.0 µs [122.6 µs, 137.1 µs] | 2.2 µs [2.0 µs, 2.3 µs] | 0.0 [0.0, 0.0] | 1.093 [1.059, 1.229] | same |
| explicit_hex8_fd | step_rest | base-t1 | 5 | 30 | 58.16 ms [57.34 ms, 58.49 ms] | 1.94 ms [1.91 ms, 1.95 ms] | 17.5 [17.3, 17.6] | – | same |
| explicit_hex8_fd | step_rest | base-t8 | 5 | 30 | 57.78 ms [57.22 ms, 57.89 ms] | 1.93 ms [1.91 ms, 1.93 ms] | 17.4 [17.2, 17.4] | 0.990 [0.980, 1.009] | same |
| explicit_hex8_fd | step_rest | head5-t1 | 5 | 30 | 63.38 ms [62.98 ms, 63.67 ms] | 2.11 ms [2.10 ms, 2.12 ms] | 19.1 [19.0, 19.2] | 1.090 [1.079, 1.099] | same |
| explicit_hex8_fd | step_rest | head5-t8 | 5 | 30 | 32.69 ms [32.49 ms, 32.91 ms] | 1.09 ms [1.08 ms, 1.10 ms] | 9.9 [9.8, 9.9] | 0.562 [0.555, 0.568] | same |
| explicit_hex8_fd | internal_forces | base-t1 | 5 | 6 | 246.02 ms [245.12 ms, 247.68 ms] | 41.00 ms [40.85 ms, 41.28 ms] | 370.8 [369.4, 373.3] | – | same |
| explicit_hex8_fd | internal_forces | base-t8 | 5 | 6 | 54.60 ms [53.91 ms, 55.06 ms] | 9.10 ms [8.98 ms, 9.18 ms] | 82.3 [81.2, 83.0] | 0.222 [0.219, 0.224] | same |
| explicit_hex8_fd | internal_forces | head5-t1 | 5 | 6 | 246.61 ms [245.48 ms, 247.57 ms] | 41.10 ms [40.91 ms, 41.26 ms] | 371.7 [369.9, 373.1] | 1.001 [0.997, 1.008] | same |
| explicit_hex8_fd | internal_forces | head5-t8 | 5 | 6 | 53.71 ms [53.58 ms, 54.30 ms] | 8.95 ms [8.93 ms, 9.05 ms] | 80.9 [80.7, 81.8] | 0.219 [0.218, 0.220] | same |
| explicit_hex8_fd | stresses | base-t1 | 5 | 6 | 112.96 ms [110.68 ms, 117.04 ms] | 18.83 ms [18.45 ms, 19.51 ms] | 170.2 [166.8, 176.4] | – | same |
| explicit_hex8_fd | stresses | base-t8 | 5 | 6 | 68.06 ms [68.01 ms, 68.16 ms] | 11.34 ms [11.33 ms, 11.36 ms] | 102.6 [102.5, 102.7] | 0.603 [0.582, 0.615] | same |
| explicit_hex8_fd | stresses | head5-t1 | 5 | 6 | 114.33 ms [110.51 ms, 117.32 ms] | 19.06 ms [18.42 ms, 19.55 ms] | 172.3 [166.5, 176.8] | 0.995 [0.977, 1.048] | same |
| explicit_hex8_fd | stresses | head5-t8 | 5 | 6 | 71.64 ms [69.61 ms, 72.97 ms] | 11.94 ms [11.60 ms, 12.16 ms] | 108.0 [104.9, 110.0] | 0.634 [0.597, 0.642] | same |
| explicit_hex8_fd | energies | base-t1 | 5 | 6 | 47.13 ms [46.82 ms, 47.29 ms] | 7.86 ms [7.80 ms, 7.88 ms] | 71.0 [70.6, 71.3] | – | same |
| explicit_hex8_fd | energies | base-t8 | 5 | 6 | 47.21 ms [46.92 ms, 47.28 ms] | 7.87 ms [7.82 ms, 7.88 ms] | 71.1 [70.7, 71.3] | 1.002 [0.996, 1.010] | same |
| explicit_hex8_fd | energies | head5-t1 | 5 | 6 | 47.13 ms [47.01 ms, 47.32 ms] | 7.85 ms [7.83 ms, 7.89 ms] | 71.0 [70.8, 71.3] | 1.004 [0.996, 1.004] | same |
| explicit_hex8_fd | energies | head5-t8 | 5 | 6 | 47.03 ms [46.77 ms, 47.21 ms] | 7.84 ms [7.79 ms, 7.87 ms] | 70.9 [70.5, 71.1] | 0.998 [0.992, 1.007] | same |
| explicit_tet10_fd | mesh | base-t1 | 5 | 1 | 6.83 ms [6.78 ms, 7.05 ms] | 6.83 ms [6.78 ms, 7.05 ms] | 64.8 [64.3, 66.9] | – | same |
| explicit_tet10_fd | mesh | base-t8 | 5 | 1 | 6.93 ms [6.68 ms, 6.96 ms] | 6.93 ms [6.68 ms, 6.96 ms] | 65.7 [63.3, 66.0] | 1.005 [0.976, 1.020] | same |
| explicit_tet10_fd | mesh | head5-t1 | 5 | 1 | 6.74 ms [6.62 ms, 6.83 ms] | 6.74 ms [6.62 ms, 6.83 ms] | 63.9 [62.8, 64.8] | 0.976 [0.961, 1.001] | same |
| explicit_tet10_fd | mesh | head5-t8 | 5 | 1 | 6.57 ms [6.52 ms, 6.84 ms] | 6.57 ms [6.52 ms, 6.84 ms] | 62.3 [61.8, 64.9] | 0.963 [0.931, 1.009] | same |
| explicit_tet10_fd | init | base-t1 | 5 | 1 | 189.33 ms [186.96 ms, 191.22 ms] | 189.33 ms [186.96 ms, 191.22 ms] | 1795.4 [1772.8, 1813.3] | – | same |
| explicit_tet10_fd | init | base-t8 | 5 | 1 | 189.01 ms [185.51 ms, 191.36 ms] | 189.01 ms [185.51 ms, 191.36 ms] | 1792.3 [1759.1, 1814.6] | 1.000 [0.989, 1.003] | same |
| explicit_tet10_fd | init | head5-t1 | 5 | 1 | 187.69 ms [185.59 ms, 193.35 ms] | 187.69 ms [185.59 ms, 193.35 ms] | 1779.7 [1759.8, 1833.5] | 0.999 [0.982, 1.021] | same |
| explicit_tet10_fd | init | head5-t8 | 5 | 1 | 185.55 ms [185.00 ms, 187.64 ms] | 185.55 ms [185.00 ms, 187.64 ms] | 1759.5 [1754.3, 1779.3] | 0.982 [0.976, 0.990] | same |
| explicit_tet10_fd | stable_time_step | base-t1 | 5 | 5 | 62.57 ms [62.40 ms, 62.61 ms] | 12.51 ms [12.48 ms, 12.52 ms] | 118.7 [118.3, 118.7] | – | same |
| explicit_tet10_fd | stable_time_step | base-t8 | 5 | 5 | 62.88 ms [62.75 ms, 62.96 ms] | 12.58 ms [12.55 ms, 12.59 ms] | 119.2 [119.0, 119.4] | 1.006 [1.004, 1.007] | same |
| explicit_tet10_fd | stable_time_step | head5-t1 | 5 | 5 | 61.79 ms [61.46 ms, 62.10 ms] | 12.36 ms [12.29 ms, 12.42 ms] | 117.2 [116.6, 117.8] | 0.989 [0.983, 0.993] | same |
| explicit_tet10_fd | stable_time_step | head5-t8 | 5 | 5 | 62.26 ms [62.19 ms, 63.05 ms] | 12.45 ms [12.44 ms, 12.61 ms] | 118.1 [117.9, 119.6] | 0.995 [0.993, 1.008] | same |
| explicit_tet10_fd | solve_step | base-t1 | 5 | 40 | 1.128 s [1.124 s, 1.132 s] | 28.20 ms [28.09 ms, 28.30 ms] | 267.4 [266.4, 268.3] | – | same |
| explicit_tet10_fd | solve_step | base-t8 | 5 | 40 | 377.11 ms [371.74 ms, 380.07 ms] | 9.43 ms [9.29 ms, 9.50 ms] | 89.4 [88.1, 90.1] | 0.333 [0.330, 0.337] | same |
| explicit_tet10_fd | solve_step | head5-t1 | 5 | 40 | 1.138 s [1.136 s, 1.138 s] | 28.45 ms [28.39 ms, 28.45 ms] | 269.8 [269.2, 269.8] | 1.007 [1.005, 1.012] | same |
| explicit_tet10_fd | solve_step | head5-t8 | 5 | 40 | 315.79 ms [313.66 ms, 316.27 ms] | 7.89 ms [7.84 ms, 7.91 ms] | 74.9 [74.4, 75.0] | 0.280 [0.278, 0.281] | same |
| explicit_tet10_fd | predictor | base-t1 | 5 | 40 | 6.5 µs [5.9 µs, 15.8 µs] | 0.2 µs [0.1 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | predictor | base-t8 | 5 | 40 | 6.2 µs [5.6 µs, 6.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.920 [0.417, 0.999] | same |
| explicit_tet10_fd | predictor | head5-t1 | 5 | 40 | 7.2 µs [6.4 µs, 7.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.108 [0.454, 1.193] | same |
| explicit_tet10_fd | predictor | head5-t8 | 5 | 40 | 5.7 µs [4.5 µs, 7.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.967 [0.324, 1.166] | same |
| explicit_tet10_fd | residual | base-t1 | 5 | 40 | 1.028 s [1.024 s, 1.031 s] | 25.70 ms [25.60 ms, 25.78 ms] | 243.7 [242.7, 244.4] | – | same |
| explicit_tet10_fd | residual | base-t8 | 5 | 40 | 274.62 ms [271.26 ms, 277.16 ms] | 6.87 ms [6.78 ms, 6.93 ms] | 65.1 [64.3, 65.7] | 0.266 [0.264, 0.270] | same |
| explicit_tet10_fd | residual | head5-t1 | 5 | 40 | 1.029 s [1.027 s, 1.029 s] | 25.72 ms [25.68 ms, 25.73 ms] | 243.9 [243.5, 244.0] | 1.000 [0.998, 1.004] | same |
| explicit_tet10_fd | residual | head5-t8 | 5 | 40 | 256.07 ms [253.42 ms, 256.18 ms] | 6.40 ms [6.34 ms, 6.40 ms] | 60.7 [60.1, 60.7] | 0.249 [0.247, 0.250] | same |
| explicit_tet10_fd | lumped_matrix | base-t1 | 5 | 40 | 10.7 µs [8.9 µs, 11.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | lumped_matrix | base-t8 | 5 | 40 | 16.3 µs [13.5 µs, 18.3 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.525 [1.295, 1.717] | same |
| explicit_tet10_fd | lumped_matrix | head5-t1 | 5 | 40 | 12.2 µs [9.4 µs, 13.4 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.132 [0.942, 1.281] | same |
| explicit_tet10_fd | lumped_matrix | head5-t8 | 5 | 40 | 14.4 µs [12.0 µs, 15.5 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.425 [1.033, 1.614] | same |
| explicit_tet10_fd | corrector | base-t1 | 5 | 40 | 6.5 µs [6.0 µs, 7.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | corrector | base-t8 | 5 | 40 | 6.4 µs [5.8 µs, 10.3 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.989 [0.968, 1.338] | same |
| explicit_tet10_fd | corrector | head5-t1 | 5 | 40 | 9.3 µs [7.1 µs, 10.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.381 [1.185, 1.539] | same |
| explicit_tet10_fd | corrector | head5-t8 | 5 | 40 | 10.2 µs [6.8 µs, 11.2 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.377 [0.928, 1.761] | same |
| explicit_tet10_fd | step_hooks | base-t1 | 5 | 80 | 154.4 µs [152.5 µs, 164.5 µs] | 1.9 µs [1.9 µs, 2.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | step_hooks | base-t8 | 5 | 80 | 159.8 µs [156.9 µs, 182.4 µs] | 2.0 µs [2.0 µs, 2.3 µs] | 0.0 [0.0, 0.0] | 1.037 [0.964, 1.189] | same |
| explicit_tet10_fd | step_hooks | head5-t1 | 5 | 80 | 176.4 µs [151.4 µs, 182.1 µs] | 2.2 µs [1.9 µs, 2.3 µs] | 0.0 [0.0, 0.0] | 1.072 [0.952, 1.194] | same |
| explicit_tet10_fd | step_hooks | head5-t8 | 5 | 80 | 177.4 µs [153.9 µs, 179.6 µs] | 2.2 µs [1.9 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 1.092 [0.945, 1.170] | same |
| explicit_tet10_fd | step_rest | base-t1 | 5 | 40 | 99.87 ms [99.48 ms, 100.59 ms] | 2.50 ms [2.49 ms, 2.51 ms] | 23.7 [23.6, 23.8] | – | same |
| explicit_tet10_fd | step_rest | base-t8 | 5 | 40 | 102.30 ms [100.25 ms, 102.72 ms] | 2.56 ms [2.51 ms, 2.57 ms] | 24.3 [23.8, 24.4] | 1.017 [1.004, 1.029] | same |
| explicit_tet10_fd | step_rest | head5-t1 | 5 | 40 | 108.61 ms [108.36 ms, 109.31 ms] | 2.72 ms [2.71 ms, 2.73 ms] | 25.7 [25.7, 25.9] | 1.085 [1.084, 1.099] | same |
| explicit_tet10_fd | step_rest | head5-t8 | 5 | 40 | 59.90 ms [59.29 ms, 60.03 ms] | 1.50 ms [1.48 ms, 1.50 ms] | 14.2 [14.1, 14.2] | 0.596 [0.594, 0.602] | same |
| explicit_tet10_fd | internal_forces | base-t1 | 5 | 8 | 197.04 ms [196.49 ms, 197.28 ms] | 24.63 ms [24.56 ms, 24.66 ms] | 233.6 [232.9, 233.8] | – | same |
| explicit_tet10_fd | internal_forces | base-t8 | 5 | 8 | 43.99 ms [43.77 ms, 44.27 ms] | 5.50 ms [5.47 ms, 5.53 ms] | 52.1 [51.9, 52.5] | 0.223 [0.222, 0.225] | same |
| explicit_tet10_fd | internal_forces | head5-t1 | 5 | 8 | 197.62 ms [197.49 ms, 197.93 ms] | 24.70 ms [24.69 ms, 24.74 ms] | 234.2 [234.1, 234.6] | 1.003 [1.001, 1.007] | same |
| explicit_tet10_fd | internal_forces | head5-t8 | 5 | 8 | 43.22 ms [41.85 ms, 43.77 ms] | 5.40 ms [5.23 ms, 5.47 ms] | 51.2 [49.6, 51.9] | 0.220 [0.212, 0.222] | same |
| explicit_tet10_fd | stresses | base-t1 | 5 | 8 | 97.58 ms [88.62 ms, 98.55 ms] | 12.20 ms [11.08 ms, 12.32 ms] | 115.7 [105.0, 116.8] | – | same |
| explicit_tet10_fd | stresses | base-t8 | 5 | 8 | 44.48 ms [44.27 ms, 45.00 ms] | 5.56 ms [5.53 ms, 5.62 ms] | 52.7 [52.5, 53.3] | 0.456 [0.451, 0.508] | same |
| explicit_tet10_fd | stresses | head5-t1 | 5 | 8 | 97.50 ms [89.12 ms, 99.03 ms] | 12.19 ms [11.14 ms, 12.38 ms] | 115.6 [105.6, 117.4] | 0.999 [0.904, 1.108] | same |
| explicit_tet10_fd | stresses | head5-t8 | 5 | 8 | 44.65 ms [44.38 ms, 49.15 ms] | 5.58 ms [5.55 ms, 6.14 ms] | 52.9 [52.6, 58.3] | 0.458 [0.450, 0.555] | same |
| explicit_tet10_fd | energies | base-t1 | 5 | 8 | 36.24 ms [36.18 ms, 36.37 ms] | 4.53 ms [4.52 ms, 4.55 ms] | 43.0 [42.9, 43.1] | – | same |
| explicit_tet10_fd | energies | base-t8 | 5 | 8 | 36.44 ms [36.15 ms, 36.68 ms] | 4.55 ms [4.52 ms, 4.59 ms] | 43.2 [42.8, 43.5] | 1.005 [0.994, 1.014] | same |
| explicit_tet10_fd | energies | head5-t1 | 5 | 8 | 36.33 ms [36.09 ms, 36.34 ms] | 4.54 ms [4.51 ms, 4.54 ms] | 43.1 [42.8, 43.1] | 1.001 [0.996, 1.004] | same |
| explicit_tet10_fd | energies | head5-t8 | 5 | 8 | 36.01 ms [35.92 ms, 36.26 ms] | 4.50 ms [4.49 ms, 4.53 ms] | 42.7 [42.6, 43.0] | 0.994 [0.988, 1.002] | same |
| explicit_tet4_fd | mesh | base-t1 | 5 | 1 | 3.27 ms [3.03 ms, 3.40 ms] | 3.27 ms [3.03 ms, 3.40 ms] | 20.2 [18.7, 21.0] | – | same |
| explicit_tet4_fd | mesh | base-t8 | 5 | 1 | 3.26 ms [3.13 ms, 3.31 ms] | 3.26 ms [3.13 ms, 3.31 ms] | 20.1 [19.4, 20.4] | 0.990 [0.963, 1.061] | same |
| explicit_tet4_fd | mesh | head5-t1 | 5 | 1 | 3.17 ms [3.13 ms, 3.23 ms] | 3.17 ms [3.13 ms, 3.23 ms] | 19.6 [19.3, 19.9] | 0.974 [0.920, 1.052] | same |
| explicit_tet4_fd | mesh | head5-t8 | 5 | 1 | 3.23 ms [3.09 ms, 3.29 ms] | 3.23 ms [3.09 ms, 3.29 ms] | 20.0 [19.1, 20.3] | 0.978 [0.939, 1.086] | same |
| explicit_tet4_fd | init | base-t1 | 5 | 1 | 52.05 ms [51.39 ms, 55.70 ms] | 52.05 ms [51.39 ms, 55.70 ms] | 321.3 [317.2, 343.8] | – | same |
| explicit_tet4_fd | init | base-t8 | 5 | 1 | 53.21 ms [51.66 ms, 56.18 ms] | 53.21 ms [51.66 ms, 56.18 ms] | 328.5 [318.9, 346.8] | 0.993 [0.967, 1.093] | same |
| explicit_tet4_fd | init | head5-t1 | 5 | 1 | 52.45 ms [51.26 ms, 54.77 ms] | 52.45 ms [51.26 ms, 54.77 ms] | 323.8 [316.4, 338.1] | 0.998 [0.983, 1.017] | same |
| explicit_tet4_fd | init | head5-t8 | 5 | 1 | 53.15 ms [52.65 ms, 56.13 ms] | 53.15 ms [52.65 ms, 56.13 ms] | 328.1 [325.0, 346.5] | 1.017 [0.969, 1.080] | same |
| explicit_tet4_fd | stable_time_step | base-t1 | 5 | 5 | 32.19 ms [32.12 ms, 32.23 ms] | 6.44 ms [6.42 ms, 6.45 ms] | 39.7 [39.7, 39.8] | – | same |
| explicit_tet4_fd | stable_time_step | base-t8 | 5 | 5 | 32.22 ms [32.14 ms, 32.26 ms] | 6.44 ms [6.43 ms, 6.45 ms] | 39.8 [39.7, 39.8] | 1.001 [0.999, 1.004] | same |
| explicit_tet4_fd | stable_time_step | head5-t1 | 5 | 5 | 32.38 ms [32.32 ms, 32.59 ms] | 6.48 ms [6.46 ms, 6.52 ms] | 40.0 [39.9, 40.2] | 1.006 [1.004, 1.012] | same |
| explicit_tet4_fd | stable_time_step | head5-t8 | 5 | 5 | 32.45 ms [32.34 ms, 32.46 ms] | 6.49 ms [6.47 ms, 6.49 ms] | 40.1 [39.9, 40.1] | 1.007 [1.005, 1.008] | same |
| explicit_tet4_fd | solve_step | base-t1 | 5 | 150 | 1.290 s [1.289 s, 1.296 s] | 8.60 ms [8.59 ms, 8.64 ms] | 53.1 [53.0, 53.3] | – | same |
| explicit_tet4_fd | solve_step | base-t8 | 5 | 150 | 345.59 ms [343.58 ms, 350.46 ms] | 2.30 ms [2.29 ms, 2.34 ms] | 14.2 [14.1, 14.4] | 0.268 [0.266, 0.272] | same |
| explicit_tet4_fd | solve_step | head5-t1 | 5 | 150 | 1.298 s [1.293 s, 1.307 s] | 8.66 ms [8.62 ms, 8.71 ms] | 53.4 [53.2, 53.8] | 1.007 [1.003, 1.009] | same |
| explicit_tet4_fd | solve_step | head5-t8 | 5 | 150 | 258.22 ms [257.43 ms, 272.46 ms] | 1.72 ms [1.72 ms, 1.82 ms] | 10.6 [10.6, 11.2] | 0.200 [0.200, 0.210] | same |
| explicit_tet4_fd | predictor | base-t1 | 5 | 150 | 5.2 µs [4.6 µs, 13.2 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | predictor | base-t8 | 5 | 150 | 8.0 µs [4.4 µs, 9.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.844 [0.605, 1.965] | same |
| explicit_tet4_fd | predictor | head5-t1 | 5 | 150 | 17.0 µs [14.7 µs, 18.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 3.108 [1.191, 3.776] | same |
| explicit_tet4_fd | predictor | head5-t8 | 5 | 150 | 5.1 µs [4.1 µs, 11.3 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.099 [0.310, 1.403] | same |
| explicit_tet4_fd | residual | base-t1 | 5 | 150 | 1.225 s [1.223 s, 1.230 s] | 8.16 ms [8.15 ms, 8.20 ms] | 50.4 [50.3, 50.6] | – | same |
| explicit_tet4_fd | residual | base-t8 | 5 | 150 | 278.70 ms [277.13 ms, 284.27 ms] | 1.86 ms [1.85 ms, 1.90 ms] | 11.5 [11.4, 11.7] | 0.228 [0.226, 0.232] | same |
| explicit_tet4_fd | residual | head5-t1 | 5 | 150 | 1.226 s [1.220 s, 1.233 s] | 8.17 ms [8.14 ms, 8.22 ms] | 50.4 [50.2, 50.7] | 1.001 [0.998, 1.003] | same |
| explicit_tet4_fd | residual | head5-t8 | 5 | 150 | 228.15 ms [227.40 ms, 242.56 ms] | 1.52 ms [1.52 ms, 1.62 ms] | 9.4 [9.4, 10.0] | 0.186 [0.186, 0.197] | same |
| explicit_tet4_fd | lumped_matrix | base-t1 | 5 | 150 | 26.0 µs [20.2 µs, 28.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | lumped_matrix | base-t8 | 5 | 150 | 27.2 µs [22.7 µs, 30.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.045 [0.805, 1.493] | same |
| explicit_tet4_fd | lumped_matrix | head5-t1 | 5 | 150 | 34.7 µs [32.4 µs, 37.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.338 [1.225, 1.846] | same |
| explicit_tet4_fd | lumped_matrix | head5-t8 | 5 | 150 | 27.3 µs [20.1 µs, 45.6 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.047 [0.825, 1.725] | same |
| explicit_tet4_fd | corrector | base-t1 | 5 | 150 | 9.2 µs [8.5 µs, 10.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | corrector | base-t8 | 5 | 150 | 11.4 µs [7.3 µs, 20.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.168 [0.757, 2.438] | same |
| explicit_tet4_fd | corrector | head5-t1 | 5 | 150 | 15.2 µs [10.2 µs, 19.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.651 [1.094, 2.055] | same |
| explicit_tet4_fd | corrector | head5-t8 | 5 | 150 | 10.7 µs [7.7 µs, 17.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.163 [0.793, 2.052] | same |
| explicit_tet4_fd | step_hooks | base-t1 | 5 | 300 | 487.1 µs [461.8 µs, 525.7 µs] | 1.6 µs [1.5 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | step_hooks | base-t8 | 5 | 300 | 485.7 µs [445.2 µs, 499.7 µs] | 1.6 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 1.013 [0.847, 1.042] | same |
| explicit_tet4_fd | step_hooks | head5-t1 | 5 | 300 | 574.6 µs [472.7 µs, 598.0 µs] | 1.9 µs [1.6 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 1.224 [0.954, 1.247] | same |
| explicit_tet4_fd | step_hooks | head5-t8 | 5 | 300 | 435.6 µs [406.3 µs, 475.1 µs] | 1.5 µs [1.4 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 0.870 [0.829, 1.029] | same |
| explicit_tet4_fd | step_rest | base-t1 | 5 | 150 | 65.14 ms [64.64 ms, 65.60 ms] | 434.3 µs [430.9 µs, 437.3 µs] | 2.7 [2.7, 2.7] | – | same |
| explicit_tet4_fd | step_rest | base-t8 | 5 | 150 | 66.22 ms [65.65 ms, 67.08 ms] | 441.4 µs [437.7 µs, 447.2 µs] | 2.7 [2.7, 2.8] | 1.019 [1.005, 1.023] | same |
| explicit_tet4_fd | step_rest | head5-t1 | 5 | 150 | 72.51 ms [72.24 ms, 73.64 ms] | 483.4 µs [481.6 µs, 491.0 µs] | 3.0 [3.0, 3.0] | 1.117 [1.106, 1.123] | same |
| explicit_tet4_fd | step_rest | head5-t8 | 5 | 150 | 29.51 ms [29.42 ms, 30.02 ms] | 196.7 µs [196.1 µs, 200.2 µs] | 1.2 [1.2, 1.2] | 0.456 [0.448, 0.460] | same |
| explicit_tet4_fd | internal_forces | base-t1 | 5 | 30 | 238.14 ms [237.87 ms, 239.28 ms] | 7.94 ms [7.93 ms, 7.98 ms] | 49.0 [48.9, 49.2] | – | same |
| explicit_tet4_fd | internal_forces | base-t8 | 5 | 30 | 45.28 ms [45.22 ms, 45.68 ms] | 1.51 ms [1.51 ms, 1.52 ms] | 9.3 [9.3, 9.4] | 0.190 [0.190, 0.191] | same |
| explicit_tet4_fd | internal_forces | head5-t1 | 5 | 30 | 238.08 ms [237.37 ms, 239.52 ms] | 7.94 ms [7.91 ms, 7.98 ms] | 49.0 [48.8, 49.3] | 1.000 [0.998, 1.001] | same |
| explicit_tet4_fd | internal_forces | head5-t8 | 5 | 30 | 38.72 ms [38.48 ms, 38.99 ms] | 1.29 ms [1.28 ms, 1.30 ms] | 8.0 [7.9, 8.0] | 0.163 [0.162, 0.163] | same |
| explicit_tet4_fd | stresses | base-t1 | 5 | 30 | 75.94 ms [75.54 ms, 76.37 ms] | 2.53 ms [2.52 ms, 2.55 ms] | 15.6 [15.5, 15.7] | – | same |
| explicit_tet4_fd | stresses | base-t8 | 5 | 30 | 41.25 ms [40.91 ms, 41.52 ms] | 1.37 ms [1.36 ms, 1.38 ms] | 8.5 [8.4, 8.5] | 0.544 [0.539, 0.546] | same |
| explicit_tet4_fd | stresses | head5-t1 | 5 | 30 | 76.08 ms [75.87 ms, 76.41 ms] | 2.54 ms [2.53 ms, 2.55 ms] | 15.7 [15.6, 15.7] | 1.001 [0.999, 1.011] | same |
| explicit_tet4_fd | stresses | head5-t8 | 5 | 30 | 40.46 ms [39.81 ms, 40.91 ms] | 1.35 ms [1.33 ms, 1.36 ms] | 8.3 [8.2, 8.4] | 0.535 [0.524, 0.536] | same |
| explicit_tet4_fd | energies | base-t1 | 5 | 30 | 78.03 ms [77.84 ms, 78.33 ms] | 2.60 ms [2.59 ms, 2.61 ms] | 16.1 [16.0, 16.1] | – | same |
| explicit_tet4_fd | energies | base-t8 | 5 | 30 | 78.05 ms [77.92 ms, 79.18 ms] | 2.60 ms [2.60 ms, 2.64 ms] | 16.1 [16.0, 16.3] | 1.003 [0.997, 1.011] | same |
| explicit_tet4_fd | energies | head5-t1 | 5 | 30 | 79.80 ms [79.46 ms, 80.25 ms] | 2.66 ms [2.65 ms, 2.68 ms] | 16.4 [16.4, 16.5] | 1.022 [1.020, 1.030] | same |
| explicit_tet4_fd | energies | head5-t8 | 5 | 30 | 78.84 ms [78.67 ms, 79.38 ms] | 2.63 ms [2.62 ms, 2.65 ms] | 16.2 [16.2, 16.3] | 1.012 [1.008, 1.020] | same |
| explicit_tri3_fd | mesh | base-t1 | 5 | 1 | 4.20 ms [4.02 ms, 4.31 ms] | 4.20 ms [4.02 ms, 4.31 ms] | 23.3 [22.3, 24.0] | – | same |
| explicit_tri3_fd | mesh | base-t8 | 5 | 1 | 4.22 ms [3.96 ms, 4.24 ms] | 4.22 ms [3.96 ms, 4.24 ms] | 23.4 [22.0, 23.6] | 1.007 [0.930, 1.055] | same |
| explicit_tri3_fd | mesh | head5-t1 | 5 | 1 | 4.10 ms [4.01 ms, 4.22 ms] | 4.10 ms [4.01 ms, 4.22 ms] | 22.8 [22.3, 23.5] | 1.006 [0.944, 1.021] | same |
| explicit_tri3_fd | mesh | head5-t8 | 5 | 1 | 4.17 ms [3.97 ms, 4.22 ms] | 4.17 ms [3.97 ms, 4.22 ms] | 23.1 [22.0, 23.4] | 0.992 [0.920, 1.036] | same |
| explicit_tri3_fd | init | base-t1 | 5 | 1 | 40.49 ms [39.89 ms, 43.02 ms] | 40.49 ms [39.89 ms, 43.02 ms] | 224.9 [221.6, 239.0] | – | same |
| explicit_tri3_fd | init | base-t8 | 5 | 1 | 42.78 ms [40.55 ms, 43.65 ms] | 42.78 ms [40.55 ms, 43.65 ms] | 237.7 [225.3, 242.5] | 1.027 [1.001, 1.071] | same |
| explicit_tri3_fd | init | head5-t1 | 5 | 1 | 39.98 ms [39.22 ms, 41.31 ms] | 39.98 ms [39.22 ms, 41.31 ms] | 222.1 [217.9, 229.5] | 0.983 [0.960, 0.992] | same |
| explicit_tri3_fd | init | head5-t8 | 5 | 1 | 41.54 ms [40.26 ms, 43.29 ms] | 41.54 ms [40.26 ms, 43.29 ms] | 230.8 [223.6, 240.5] | 1.006 [0.994, 1.066] | same |
| explicit_tri3_fd | stable_time_step | base-t1 | 5 | 5 | 23.24 ms [22.55 ms, 23.32 ms] | 4.65 ms [4.51 ms, 4.66 ms] | 25.8 [25.1, 25.9] | – | same |
| explicit_tri3_fd | stable_time_step | base-t8 | 5 | 5 | 23.16 ms [22.53 ms, 23.50 ms] | 4.63 ms [4.51 ms, 4.70 ms] | 25.7 [25.0, 26.1] | 0.996 [0.969, 1.042] | same |
| explicit_tri3_fd | stable_time_step | head5-t1 | 5 | 5 | 23.00 ms [22.76 ms, 23.65 ms] | 4.60 ms [4.55 ms, 4.73 ms] | 25.6 [25.3, 26.3] | 1.017 [0.980, 1.020] | same |
| explicit_tri3_fd | stable_time_step | head5-t8 | 5 | 5 | 22.79 ms [22.57 ms, 23.23 ms] | 4.56 ms [4.51 ms, 4.65 ms] | 25.3 [25.1, 25.8] | 0.997 [0.968, 1.020] | same |
| explicit_tri3_fd | solve_step | base-t1 | 5 | 200 | 1.452 s [1.450 s, 1.461 s] | 7.26 ms [7.25 ms, 7.30 ms] | 40.3 [40.3, 40.6] | – | same |
| explicit_tri3_fd | solve_step | base-t8 | 5 | 200 | 496.40 ms [490.79 ms, 504.05 ms] | 2.48 ms [2.45 ms, 2.52 ms] | 13.8 [13.6, 14.0] | 0.342 [0.338, 0.345] | same |
| explicit_tri3_fd | solve_step | head5-t1 | 5 | 200 | 1.474 s [1.473 s, 1.481 s] | 7.37 ms [7.36 ms, 7.40 ms] | 41.0 [40.9, 41.1] | 1.015 [1.013, 1.017] | same |
| explicit_tri3_fd | solve_step | head5-t8 | 5 | 200 | 288.41 ms [287.44 ms, 299.33 ms] | 1.44 ms [1.44 ms, 1.50 ms] | 8.0 [8.0, 8.3] | 0.198 [0.198, 0.206] | same |
| explicit_tri3_fd | predictor | base-t1 | 5 | 200 | 12.9 µs [9.9 µs, 13.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | predictor | base-t8 | 5 | 200 | 12.3 µs [7.7 µs, 15.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.929 [0.634, 1.518] | same |
| explicit_tri3_fd | predictor | head5-t1 | 5 | 200 | 18.6 µs [14.0 µs, 24.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.403 [1.148, 2.447] | same |
| explicit_tri3_fd | predictor | head5-t8 | 5 | 200 | 10.6 µs [7.9 µs, 21.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.071 [0.650, 1.606] | same |
| explicit_tri3_fd | residual | base-t1 | 5 | 200 | 1.277 s [1.274 s, 1.284 s] | 6.39 ms [6.37 ms, 6.42 ms] | 35.5 [35.4, 35.7] | – | same |
| explicit_tri3_fd | residual | base-t8 | 5 | 200 | 319.78 ms [315.41 ms, 326.34 ms] | 1.60 ms [1.58 ms, 1.63 ms] | 8.9 [8.8, 9.1] | 0.250 [0.247, 0.254] | same |
| explicit_tri3_fd | residual | head5-t1 | 5 | 200 | 1.280 s [1.278 s, 1.284 s] | 6.40 ms [6.39 ms, 6.42 ms] | 35.5 [35.5, 35.7] | 1.002 [1.000, 1.003] | same |
| explicit_tri3_fd | residual | head5-t8 | 5 | 200 | 218.65 ms [217.21 ms, 229.22 ms] | 1.09 ms [1.09 ms, 1.15 ms] | 6.1 [6.0, 6.4] | 0.171 [0.170, 0.179] | same |
| explicit_tri3_fd | lumped_matrix | base-t1 | 5 | 200 | 34.1 µs [31.8 µs, 46.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | lumped_matrix | base-t8 | 5 | 200 | 39.3 µs [33.7 µs, 49.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.012 [0.862, 1.555] | same |
| explicit_tri3_fd | lumped_matrix | head5-t1 | 5 | 200 | 55.3 µs [42.7 µs, 56.7 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.581 [1.185, 1.719] | same |
| explicit_tri3_fd | lumped_matrix | head5-t8 | 5 | 200 | 46.1 µs [37.7 µs, 55.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.228 [1.032, 1.714] | same |
| explicit_tri3_fd | corrector | base-t1 | 5 | 200 | 14.6 µs [10.4 µs, 15.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | corrector | base-t8 | 5 | 200 | 15.6 µs [12.4 µs, 20.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.069 [0.969, 1.924] | same |
| explicit_tri3_fd | corrector | head5-t1 | 5 | 200 | 18.6 µs [15.1 µs, 24.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.573 [1.028, 2.238] | same |
| explicit_tri3_fd | corrector | head5-t8 | 5 | 200 | 13.4 µs [10.0 µs, 18.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.868 [0.684, 1.810] | same |
| explicit_tri3_fd | step_hooks | base-t1 | 5 | 400 | 648.5 µs [618.2 µs, 691.1 µs] | 1.6 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | step_hooks | base-t8 | 5 | 400 | 671.1 µs [644.7 µs, 766.2 µs] | 1.7 µs [1.6 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.043 [1.014, 1.109] | same |
| explicit_tri3_fd | step_hooks | head5-t1 | 5 | 400 | 677.4 µs [667.9 µs, 828.4 µs] | 1.7 µs [1.7 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 1.080 [0.981, 1.199] | same |
| explicit_tri3_fd | step_hooks | head5-t8 | 5 | 400 | 576.5 µs [561.7 µs, 680.4 µs] | 1.4 µs [1.4 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.932 [0.813, 0.985] | same |
| explicit_tri3_fd | step_rest | base-t1 | 5 | 200 | 174.42 ms [171.66 ms, 176.51 ms] | 872.1 µs [858.3 µs, 882.6 µs] | 4.8 [4.8, 4.9] | – | same |
| explicit_tri3_fd | step_rest | base-t8 | 5 | 200 | 176.37 ms [174.52 ms, 178.93 ms] | 881.8 µs [872.6 µs, 894.6 µs] | 4.9 [4.8, 5.0] | 1.011 [1.002, 1.026] | same |
| explicit_tri3_fd | step_rest | head5-t1 | 5 | 200 | 194.08 ms [193.62 ms, 196.29 ms] | 970.4 µs [968.1 µs, 981.4 µs] | 5.4 [5.4, 5.5] | 1.113 [1.112, 1.130] | same |
| explicit_tri3_fd | step_rest | head5-t8 | 5 | 200 | 69.10 ms [68.64 ms, 69.53 ms] | 345.5 µs [343.2 µs, 347.6 µs] | 1.9 [1.9, 1.9] | 0.398 [0.389, 0.403] | same |
| explicit_tri3_fd | internal_forces | base-t1 | 5 | 40 | 233.87 ms [232.98 ms, 234.01 ms] | 5.85 ms [5.82 ms, 5.85 ms] | 32.5 [32.4, 32.5] | – | same |
| explicit_tri3_fd | internal_forces | base-t8 | 5 | 40 | 38.93 ms [38.57 ms, 39.18 ms] | 973.2 µs [964.3 µs, 979.4 µs] | 5.4 [5.4, 5.4] | 0.167 [0.165, 0.168] | same |
| explicit_tri3_fd | internal_forces | head5-t1 | 5 | 40 | 233.39 ms [233.16 ms, 233.60 ms] | 5.83 ms [5.83 ms, 5.84 ms] | 32.4 [32.4, 32.4] | 0.998 [0.998, 1.001] | same |
| explicit_tri3_fd | internal_forces | head5-t8 | 5 | 40 | 30.37 ms [30.14 ms, 30.50 ms] | 759.4 µs [753.6 µs, 762.5 µs] | 4.2 [4.2, 4.2] | 0.130 [0.129, 0.131] | same |
| explicit_tri3_fd | stresses | base-t1 | 5 | 40 | 56.38 ms [55.86 ms, 56.51 ms] | 1.41 ms [1.40 ms, 1.41 ms] | 7.8 [7.8, 7.8] | – | same |
| explicit_tri3_fd | stresses | base-t8 | 5 | 40 | 28.59 ms [27.79 ms, 28.84 ms] | 714.7 µs [694.7 µs, 720.9 µs] | 4.0 [3.9, 4.0] | 0.507 [0.497, 0.515] | same |
| explicit_tri3_fd | stresses | head5-t1 | 5 | 40 | 55.99 ms [55.23 ms, 56.03 ms] | 1.40 ms [1.38 ms, 1.40 ms] | 7.8 [7.7, 7.8] | 0.991 [0.984, 1.003] | same |
| explicit_tri3_fd | stresses | head5-t8 | 5 | 40 | 26.21 ms [25.94 ms, 27.41 ms] | 655.3 µs [648.4 µs, 685.1 µs] | 3.6 [3.6, 3.8] | 0.464 [0.464, 0.485] | same |
| explicit_tri3_fd | energies | base-t1 | 5 | 40 | 96.15 ms [95.30 ms, 96.62 ms] | 2.40 ms [2.38 ms, 2.42 ms] | 13.4 [13.2, 13.4] | – | same |
| explicit_tri3_fd | energies | base-t8 | 5 | 40 | 96.24 ms [95.71 ms, 96.99 ms] | 2.41 ms [2.39 ms, 2.42 ms] | 13.4 [13.3, 13.5] | 1.004 [0.998, 1.008] | same |
| explicit_tri3_fd | energies | head5-t1 | 5 | 40 | 98.84 ms [98.42 ms, 99.33 ms] | 2.47 ms [2.46 ms, 2.48 ms] | 13.7 [13.7, 13.8] | 1.029 [1.025, 1.035] | same |
| explicit_tri3_fd | energies | head5-t8 | 5 | 40 | 97.59 ms [96.75 ms, 99.11 ms] | 2.44 ms [2.42 ms, 2.48 ms] | 13.6 [13.4, 13.8] | 1.015 [1.008, 1.030] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | 2188f0e2b, 7a6ad8f42 | 78b072c2f464d8f9 |
| explicit_tet10_fd | 105456 | 148877 | 446631 | 1 | 2188f0e2b, 7a6ad8f42 | c9f013e1a9bbd7a1 |
| explicit_tet4_fd | 162000 | 29791 | 89373 | 1 | 2188f0e2b, 7a6ad8f42 | 9d6cdbdad424d346 |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 2188f0e2b, 7a6ad8f42 | 284a1d26bc19eda4 |
