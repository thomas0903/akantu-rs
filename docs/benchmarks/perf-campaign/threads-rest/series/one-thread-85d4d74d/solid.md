Suite `solid`, size `m`; variants: `base-t1`, `head2-t1`, `head4-t1`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_hex8_fd | mesh | base-t1 | 7 | 1 | 6.49 ms [6.25 ms, 6.53 ms] | 6.49 ms [6.25 ms, 6.53 ms] | 58.7 [56.5, 59.1] | – | same |
| explicit_hex8_fd | mesh | head2-t1 | 7 | 1 | 6.66 ms [6.51 ms, 6.71 ms] | 6.66 ms [6.51 ms, 6.71 ms] | 60.2 [58.9, 60.7] | 1.026 [1.015, 1.060] | same |
| explicit_hex8_fd | mesh | head4-t1 | 7 | 1 | 6.49 ms [6.41 ms, 6.67 ms] | 6.49 ms [6.41 ms, 6.67 ms] | 58.6 [57.9, 60.3] | 1.017 [0.991, 1.026] | same |
| explicit_hex8_fd | init | base-t1 | 7 | 1 | 302.27 ms [300.99 ms, 306.43 ms] | 302.27 ms [300.99 ms, 306.43 ms] | 2733.2 [2721.7, 2770.8] | – | same |
| explicit_hex8_fd | init | head2-t1 | 7 | 1 | 303.21 ms [298.21 ms, 309.66 ms] | 303.21 ms [298.21 ms, 309.66 ms] | 2741.7 [2696.5, 2800.0] | 0.999 [0.973, 1.029] | same |
| explicit_hex8_fd | init | head4-t1 | 7 | 1 | 307.67 ms [305.90 ms, 310.71 ms] | 307.67 ms [305.90 ms, 310.71 ms] | 2782.1 [2766.1, 2809.6] | 1.018 [1.004, 1.028] | same |
| explicit_hex8_fd | stable_time_step | base-t1 | 7 | 5 | 23.66 ms [23.55 ms, 23.92 ms] | 4.73 ms [4.71 ms, 4.78 ms] | 42.8 [42.6, 43.3] | – | same |
| explicit_hex8_fd | stable_time_step | head2-t1 | 7 | 5 | 23.19 ms [23.16 ms, 23.25 ms] | 4.64 ms [4.63 ms, 4.65 ms] | 41.9 [41.9, 42.0] | 0.980 [0.970, 0.985] | same |
| explicit_hex8_fd | stable_time_step | head4-t1 | 7 | 5 | 23.19 ms [23.15 ms, 23.44 ms] | 4.64 ms [4.63 ms, 4.69 ms] | 41.9 [41.9, 42.4] | 0.981 [0.971, 0.989] | same |
| explicit_hex8_fd | solve_step | base-t1 | 7 | 30 | 1.313 s [1.307 s, 1.320 s] | 43.75 ms [43.56 ms, 43.99 ms] | 395.6 [393.9, 397.8] | – | same |
| explicit_hex8_fd | solve_step | head2-t1 | 7 | 30 | 1.315 s [1.313 s, 1.326 s] | 43.85 ms [43.76 ms, 44.19 ms] | 396.5 [395.7, 399.6] | 1.002 [1.000, 1.014] | same |
| explicit_hex8_fd | solve_step | head4-t1 | 7 | 30 | 1.319 s [1.318 s, 1.325 s] | 43.97 ms [43.94 ms, 44.17 ms] | 397.6 [397.4, 399.4] | 1.006 [1.000, 1.010] | same |
| explicit_hex8_fd | predictor | base-t1 | 7 | 30 | 4.8 µs [4.5 µs, 5.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | predictor | head2-t1 | 7 | 30 | 5.2 µs [4.2 µs, 6.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.043 [0.929, 1.268] | same |
| explicit_hex8_fd | predictor | head4-t1 | 7 | 30 | 4.9 µs [4.3 µs, 5.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.036 [0.864, 1.077] | same |
| explicit_hex8_fd | residual | base-t1 | 7 | 30 | 1.255 s [1.249 s, 1.261 s] | 41.82 ms [41.64 ms, 42.04 ms] | 378.2 [376.5, 380.2] | – | same |
| explicit_hex8_fd | residual | head2-t1 | 7 | 30 | 1.256 s [1.253 s, 1.266 s] | 41.85 ms [41.78 ms, 42.19 ms] | 378.4 [377.8, 381.5] | 1.001 [0.999, 1.013] | same |
| explicit_hex8_fd | residual | head4-t1 | 7 | 30 | 1.256 s [1.255 s, 1.262 s] | 41.88 ms [41.84 ms, 42.06 ms] | 378.7 [378.3, 380.3] | 1.002 [0.996, 1.007] | same |
| explicit_hex8_fd | lumped_matrix | base-t1 | 7 | 30 | 8.7 µs [7.4 µs, 9.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | lumped_matrix | head2-t1 | 7 | 30 | 8.3 µs [7.6 µs, 9.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.064 [0.827, 1.243] | same |
| explicit_hex8_fd | lumped_matrix | head4-t1 | 7 | 30 | 8.3 µs [7.4 µs, 10.6 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.993 [0.887, 1.210] | same |
| explicit_hex8_fd | corrector | base-t1 | 7 | 30 | 4.7 µs [4.4 µs, 5.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | corrector | head2-t1 | 7 | 30 | 5.3 µs [4.8 µs, 7.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.037 [0.972, 1.642] | same |
| explicit_hex8_fd | corrector | head4-t1 | 7 | 30 | 7.3 µs [6.9 µs, 7.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.596 [1.265, 1.728] | same |
| explicit_hex8_fd | step_hooks | base-t1 | 7 | 60 | 118.9 µs [110.9 µs, 128.4 µs] | 2.0 µs [1.8 µs, 2.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | step_hooks | head2-t1 | 7 | 60 | 115.9 µs [114.0 µs, 127.7 µs] | 1.9 µs [1.9 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 0.995 [0.953, 1.060] | same |
| explicit_hex8_fd | step_hooks | head4-t1 | 7 | 60 | 120.3 µs [115.7 µs, 128.3 µs] | 2.0 µs [1.9 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 1.052 [0.936, 1.157] | same |
| explicit_hex8_fd | step_rest | base-t1 | 7 | 30 | 57.62 ms [57.42 ms, 58.24 ms] | 1.92 ms [1.91 ms, 1.94 ms] | 17.4 [17.3, 17.6] | – | same |
| explicit_hex8_fd | step_rest | head2-t1 | 7 | 30 | 59.80 ms [59.32 ms, 60.26 ms] | 1.99 ms [1.98 ms, 2.01 ms] | 18.0 [17.9, 18.2] | 1.035 [1.031, 1.046] | same |
| explicit_hex8_fd | step_rest | head4-t1 | 7 | 30 | 62.86 ms [62.56 ms, 63.51 ms] | 2.10 ms [2.09 ms, 2.12 ms] | 18.9 [18.9, 19.1] | 1.090 [1.077, 1.099] | same |
| explicit_hex8_fd | internal_forces | base-t1 | 7 | 6 | 245.89 ms [245.16 ms, 246.92 ms] | 40.98 ms [40.86 ms, 41.15 ms] | 370.6 [369.5, 372.1] | – | same |
| explicit_hex8_fd | internal_forces | head2-t1 | 7 | 6 | 246.70 ms [245.89 ms, 248.23 ms] | 41.12 ms [40.98 ms, 41.37 ms] | 371.8 [370.6, 374.1] | 1.003 [0.998, 1.013] | same |
| explicit_hex8_fd | internal_forces | head4-t1 | 7 | 6 | 246.92 ms [246.34 ms, 247.29 ms] | 41.15 ms [41.06 ms, 41.22 ms] | 372.1 [371.2, 372.7] | 1.004 [1.001, 1.009] | same |
| explicit_hex8_fd | stresses | base-t1 | 7 | 6 | 110.98 ms [110.53 ms, 115.38 ms] | 18.50 ms [18.42 ms, 19.23 ms] | 167.2 [166.6, 173.9] | – | same |
| explicit_hex8_fd | stresses | head2-t1 | 7 | 6 | 112.82 ms [110.65 ms, 115.03 ms] | 18.80 ms [18.44 ms, 19.17 ms] | 170.0 [166.8, 173.4] | 1.002 [0.975, 1.037] | same |
| explicit_hex8_fd | stresses | head4-t1 | 7 | 6 | 111.47 ms [109.06 ms, 112.80 ms] | 18.58 ms [18.18 ms, 18.80 ms] | 168.0 [164.4, 170.0] | 0.996 [0.961, 1.018] | same |
| explicit_hex8_fd | energies | base-t1 | 7 | 6 | 47.22 ms [47.00 ms, 47.32 ms] | 7.87 ms [7.83 ms, 7.89 ms] | 71.2 [70.8, 71.3] | – | same |
| explicit_hex8_fd | energies | head2-t1 | 7 | 6 | 47.53 ms [47.42 ms, 47.80 ms] | 7.92 ms [7.90 ms, 7.97 ms] | 71.6 [71.5, 72.0] | 1.007 [1.004, 1.014] | same |
| explicit_hex8_fd | energies | head4-t1 | 7 | 6 | 47.15 ms [47.07 ms, 47.20 ms] | 7.86 ms [7.84 ms, 7.87 ms] | 71.1 [70.9, 71.1] | 0.998 [0.997, 1.003] | same |
| explicit_tet10_ss | mesh | base-t1 | 7 | 1 | 6.91 ms [6.82 ms, 7.04 ms] | 6.91 ms [6.82 ms, 7.04 ms] | 65.6 [64.6, 66.8] | – | same |
| explicit_tet10_ss | mesh | head2-t1 | 7 | 1 | 7.12 ms [6.91 ms, 7.31 ms] | 7.12 ms [6.91 ms, 7.31 ms] | 67.5 [65.5, 69.4] | 1.034 [0.982, 1.073] | same |
| explicit_tet10_ss | mesh | head4-t1 | 7 | 1 | 6.88 ms [6.81 ms, 6.99 ms] | 6.88 ms [6.81 ms, 6.99 ms] | 65.2 [64.5, 66.2] | 0.994 [0.967, 1.024] | same |
| explicit_tet10_ss | init | base-t1 | 7 | 1 | 176.99 ms [173.87 ms, 178.70 ms] | 176.99 ms [173.87 ms, 178.70 ms] | 1678.3 [1648.8, 1694.5] | – | same |
| explicit_tet10_ss | init | head2-t1 | 7 | 1 | 176.49 ms [175.22 ms, 178.02 ms] | 176.49 ms [175.22 ms, 178.02 ms] | 1673.6 [1661.6, 1688.1] | 1.001 [0.984, 1.017] | same |
| explicit_tet10_ss | init | head4-t1 | 7 | 1 | 174.75 ms [172.66 ms, 177.36 ms] | 174.75 ms [172.66 ms, 177.36 ms] | 1657.1 [1637.3, 1681.8] | 0.983 [0.975, 1.013] | same |
| explicit_tet10_ss | stable_time_step | base-t1 | 7 | 5 | 62.39 ms [62.27 ms, 62.64 ms] | 12.48 ms [12.45 ms, 12.53 ms] | 118.3 [118.1, 118.8] | – | same |
| explicit_tet10_ss | stable_time_step | head2-t1 | 7 | 5 | 61.76 ms [61.61 ms, 62.08 ms] | 12.35 ms [12.32 ms, 12.42 ms] | 117.1 [116.8, 117.7] | 0.990 [0.985, 0.996] | same |
| explicit_tet10_ss | stable_time_step | head4-t1 | 7 | 5 | 61.67 ms [61.60 ms, 62.45 ms] | 12.33 ms [12.32 ms, 12.49 ms] | 117.0 [116.8, 118.4] | 0.989 [0.985, 1.002] | same |
| explicit_tet10_ss | solve_step | base-t1 | 7 | 50 | 993.70 ms [986.28 ms, 1.001 s] | 19.87 ms [19.73 ms, 20.02 ms] | 188.5 [187.1, 189.9] | – | same |
| explicit_tet10_ss | solve_step | head2-t1 | 7 | 50 | 1.016 s [1.008 s, 1.035 s] | 20.31 ms [20.15 ms, 20.69 ms] | 192.6 [191.1, 196.2] | 1.021 [1.018, 1.041] | same |
| explicit_tet10_ss | solve_step | head4-t1 | 7 | 50 | 1.008 s [999.26 ms, 1.012 s] | 20.15 ms [19.99 ms, 20.25 ms] | 191.1 [189.5, 192.0] | 1.013 [1.009, 1.020] | same |
| explicit_tet10_ss | predictor | base-t1 | 7 | 50 | 7.7 µs [7.3 µs, 8.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | predictor | head2-t1 | 7 | 50 | 8.0 µs [7.9 µs, 8.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.056 [0.974, 1.154] | same |
| explicit_tet10_ss | predictor | head4-t1 | 7 | 50 | 9.3 µs [6.9 µs, 10.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.112 [0.853, 1.338] | same |
| explicit_tet10_ss | residual | base-t1 | 7 | 50 | 868.93 ms [862.57 ms, 874.85 ms] | 17.38 ms [17.25 ms, 17.50 ms] | 164.8 [163.6, 165.9] | – | same |
| explicit_tet10_ss | residual | head2-t1 | 7 | 50 | 887.37 ms [879.55 ms, 905.24 ms] | 17.75 ms [17.59 ms, 18.10 ms] | 168.3 [166.8, 171.7] | 1.020 [1.015, 1.041] | same |
| explicit_tet10_ss | residual | head4-t1 | 7 | 50 | 871.62 ms [864.13 ms, 876.35 ms] | 17.43 ms [17.28 ms, 17.53 ms] | 165.3 [163.9, 166.2] | 1.002 [0.995, 1.008] | same |
| explicit_tet10_ss | lumped_matrix | base-t1 | 7 | 50 | 14.8 µs [14.0 µs, 16.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | lumped_matrix | head2-t1 | 7 | 50 | 15.1 µs [13.1 µs, 18.6 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.012 [0.892, 1.328] | same |
| explicit_tet10_ss | lumped_matrix | head4-t1 | 7 | 50 | 15.8 µs [14.1 µs, 21.2 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.073 [0.866, 1.515] | same |
| explicit_tet10_ss | corrector | base-t1 | 7 | 50 | 8.9 µs [7.3 µs, 17.4 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | corrector | head2-t1 | 7 | 50 | 9.1 µs [8.0 µs, 10.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.995 [0.529, 1.325] | same |
| explicit_tet10_ss | corrector | head4-t1 | 7 | 50 | 12.0 µs [9.0 µs, 12.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.330 [0.720, 1.733] | same |
| explicit_tet10_ss | step_hooks | base-t1 | 7 | 100 | 164.7 µs [161.3 µs, 177.5 µs] | 1.6 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | step_hooks | head2-t1 | 7 | 100 | 167.5 µs [164.1 µs, 171.4 µs] | 1.7 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 1.017 [0.930, 1.060] | same |
| explicit_tet10_ss | step_hooks | head4-t1 | 7 | 100 | 163.0 µs [159.2 µs, 178.1 µs] | 1.6 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 0.987 [0.932, 1.101] | same |
| explicit_tet10_ss | step_rest | base-t1 | 7 | 50 | 124.37 ms [123.51 ms, 126.16 ms] | 2.49 ms [2.47 ms, 2.52 ms] | 23.6 [23.4, 23.9] | – | same |
| explicit_tet10_ss | step_rest | head2-t1 | 7 | 50 | 129.06 ms [127.75 ms, 130.22 ms] | 2.58 ms [2.55 ms, 2.60 ms] | 24.5 [24.2, 24.7] | 1.033 [1.027, 1.044] | same |
| explicit_tet10_ss | step_rest | head4-t1 | 7 | 50 | 135.75 ms [134.79 ms, 137.39 ms] | 2.72 ms [2.70 ms, 2.75 ms] | 25.7 [25.6, 26.1] | 1.093 [1.076, 1.112] | same |
| explicit_tet10_ss | internal_forces | base-t1 | 7 | 10 | 163.78 ms [162.02 ms, 169.35 ms] | 16.38 ms [16.20 ms, 16.93 ms] | 155.3 [153.6, 160.6] | – | same |
| explicit_tet10_ss | internal_forces | head2-t1 | 7 | 10 | 166.47 ms [165.87 ms, 167.65 ms] | 16.65 ms [16.59 ms, 16.76 ms] | 157.9 [157.3, 159.0] | 1.019 [0.982, 1.025] | same |
| explicit_tet10_ss | internal_forces | head4-t1 | 7 | 10 | 163.14 ms [162.67 ms, 164.35 ms] | 16.31 ms [16.27 ms, 16.44 ms] | 154.7 [154.3, 155.8] | 1.000 [0.963, 1.004] | same |
| explicit_tet10_ss | stresses | base-t1 | 7 | 10 | 112.44 ms [101.14 ms, 115.97 ms] | 11.24 ms [10.11 ms, 11.60 ms] | 106.6 [95.9, 110.0] | – | same |
| explicit_tet10_ss | stresses | head2-t1 | 7 | 10 | 111.50 ms [107.83 ms, 122.00 ms] | 11.15 ms [10.78 ms, 12.20 ms] | 105.7 [102.3, 115.7] | 1.064 [0.959, 1.182] | same |
| explicit_tet10_ss | stresses | head4-t1 | 7 | 10 | 114.48 ms [101.75 ms, 118.03 ms] | 11.45 ms [10.18 ms, 11.80 ms] | 108.6 [96.5, 111.9] | 1.014 [0.905, 1.126] | same |
| explicit_tet10_ss | energies | base-t1 | 7 | 10 | 34.78 ms [34.37 ms, 35.55 ms] | 3.48 ms [3.44 ms, 3.56 ms] | 33.0 [32.6, 33.7] | – | same |
| explicit_tet10_ss | energies | head2-t1 | 7 | 10 | 34.99 ms [34.46 ms, 35.55 ms] | 3.50 ms [3.45 ms, 3.55 ms] | 33.2 [32.7, 33.7] | 1.005 [0.982, 1.028] | same |
| explicit_tet10_ss | energies | head4-t1 | 7 | 10 | 34.70 ms [33.84 ms, 35.02 ms] | 3.47 ms [3.38 ms, 3.50 ms] | 32.9 [32.1, 33.2] | 0.992 [0.966, 1.018] | same |
| explicit_tet4_ss | mesh | base-t1 | 7 | 1 | 3.23 ms [3.16 ms, 3.34 ms] | 3.23 ms [3.16 ms, 3.34 ms] | 19.9 [19.5, 20.6] | – | same |
| explicit_tet4_ss | mesh | head2-t1 | 7 | 1 | 3.26 ms [3.21 ms, 3.36 ms] | 3.26 ms [3.21 ms, 3.36 ms] | 20.1 [19.8, 20.7] | 1.008 [0.985, 1.040] | same |
| explicit_tet4_ss | mesh | head4-t1 | 7 | 1 | 3.24 ms [3.07 ms, 3.30 ms] | 3.24 ms [3.07 ms, 3.30 ms] | 20.0 [18.9, 20.4] | 0.986 [0.928, 1.043] | same |
| explicit_tet4_ss | init | base-t1 | 7 | 1 | 48.14 ms [47.21 ms, 48.89 ms] | 48.14 ms [47.21 ms, 48.89 ms] | 297.2 [291.4, 301.8] | – | same |
| explicit_tet4_ss | init | head2-t1 | 7 | 1 | 47.29 ms [46.09 ms, 48.47 ms] | 47.29 ms [46.09 ms, 48.47 ms] | 291.9 [284.5, 299.2] | 0.983 [0.963, 1.027] | same |
| explicit_tet4_ss | init | head4-t1 | 7 | 1 | 47.65 ms [46.84 ms, 48.48 ms] | 47.65 ms [46.84 ms, 48.48 ms] | 294.1 [289.1, 299.2] | 0.995 [0.979, 1.005] | same |
| explicit_tet4_ss | stable_time_step | base-t1 | 7 | 5 | 32.19 ms [32.12 ms, 32.38 ms] | 6.44 ms [6.42 ms, 6.48 ms] | 39.7 [39.6, 40.0] | – | same |
| explicit_tet4_ss | stable_time_step | head2-t1 | 7 | 5 | 32.47 ms [32.33 ms, 33.40 ms] | 6.49 ms [6.47 ms, 6.68 ms] | 40.1 [39.9, 41.2] | 1.008 [1.005, 1.037] | same |
| explicit_tet4_ss | stable_time_step | head4-t1 | 7 | 5 | 32.48 ms [32.36 ms, 32.98 ms] | 6.50 ms [6.47 ms, 6.60 ms] | 40.1 [39.9, 40.7] | 1.009 [1.007, 1.023] | same |
| explicit_tet4_ss | solve_step | base-t1 | 7 | 200 | 1.286 s [1.270 s, 1.296 s] | 6.43 ms [6.35 ms, 6.48 ms] | 39.7 [39.2, 40.0] | – | same |
| explicit_tet4_ss | solve_step | head2-t1 | 7 | 200 | 1.289 s [1.276 s, 1.295 s] | 6.44 ms [6.38 ms, 6.47 ms] | 39.8 [39.4, 40.0] | 1.002 [0.996, 1.013] | same |
| explicit_tet4_ss | solve_step | head4-t1 | 7 | 200 | 1.296 s [1.283 s, 1.301 s] | 6.48 ms [6.41 ms, 6.51 ms] | 40.0 [39.6, 40.2] | 1.008 [1.002, 1.018] | same |
| explicit_tet4_ss | predictor | base-t1 | 7 | 200 | 7.9 µs [7.4 µs, 17.8 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | head2-t1 | 7 | 200 | 11.6 µs [7.7 µs, 17.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.101 [0.442, 2.362] | same |
| explicit_tet4_ss | predictor | head4-t1 | 7 | 200 | 8.2 µs [6.4 µs, 10.4 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.916 [0.462, 1.365] | same |
| explicit_tet4_ss | residual | base-t1 | 7 | 200 | 1.199 s [1.184 s, 1.208 s] | 6.00 ms [5.92 ms, 6.04 ms] | 37.0 [36.5, 37.3] | – | same |
| explicit_tet4_ss | residual | head2-t1 | 7 | 200 | 1.199 s [1.186 s, 1.204 s] | 5.99 ms [5.93 ms, 6.02 ms] | 37.0 [36.6, 37.2] | 0.999 [0.993, 1.010] | same |
| explicit_tet4_ss | residual | head4-t1 | 7 | 200 | 1.199 s [1.186 s, 1.204 s] | 6.00 ms [5.93 ms, 6.02 ms] | 37.0 [36.6, 37.2] | 0.999 [0.995, 1.011] | same |
| explicit_tet4_ss | lumped_matrix | base-t1 | 7 | 200 | 30.5 µs [29.5 µs, 37.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | head2-t1 | 7 | 200 | 37.1 µs [31.4 µs, 39.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.113 [0.949, 1.295] | same |
| explicit_tet4_ss | lumped_matrix | head4-t1 | 7 | 200 | 28.2 µs [26.1 µs, 37.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.855 [0.768, 1.243] | same |
| explicit_tet4_ss | corrector | base-t1 | 7 | 200 | 15.7 µs [11.4 µs, 21.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | head2-t1 | 7 | 200 | 22.4 µs [14.6 µs, 28.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.608 [1.030, 1.829] | same |
| explicit_tet4_ss | corrector | head4-t1 | 7 | 200 | 21.2 µs [13.0 µs, 29.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.547 [0.598, 1.864] | same |
| explicit_tet4_ss | step_hooks | base-t1 | 7 | 400 | 546.8 µs [508.9 µs, 580.6 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | head2-t1 | 7 | 400 | 552.0 µs [509.4 µs, 579.3 µs] | 1.4 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.016 [0.877, 1.118] | same |
| explicit_tet4_ss | step_hooks | head4-t1 | 7 | 400 | 505.7 µs [497.5 µs, 529.0 µs] | 1.3 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.942 [0.869, 1.011] | same |
| explicit_tet4_ss | step_rest | base-t1 | 7 | 200 | 86.08 ms [85.49 ms, 87.53 ms] | 430.4 µs [427.4 µs, 437.7 µs] | 2.7 [2.6, 2.7] | – | same |
| explicit_tet4_ss | step_rest | head2-t1 | 7 | 200 | 89.53 ms [89.16 ms, 89.84 ms] | 447.6 µs [445.8 µs, 449.2 µs] | 2.8 [2.8, 2.8] | 1.040 [1.026, 1.047] | same |
| explicit_tet4_ss | step_rest | head4-t1 | 7 | 200 | 96.57 ms [95.82 ms, 101.12 ms] | 482.9 µs [479.1 µs, 505.6 µs] | 3.0 [3.0, 3.1] | 1.122 [1.108, 1.170] | same |
| explicit_tet4_ss | internal_forces | base-t1 | 7 | 40 | 227.53 ms [225.62 ms, 230.39 ms] | 5.69 ms [5.64 ms, 5.76 ms] | 35.1 [34.8, 35.6] | – | same |
| explicit_tet4_ss | internal_forces | head2-t1 | 7 | 40 | 227.65 ms [224.95 ms, 229.70 ms] | 5.69 ms [5.62 ms, 5.74 ms] | 35.1 [34.7, 35.4] | 0.998 [0.991, 1.009] | same |
| explicit_tet4_ss | internal_forces | head4-t1 | 7 | 40 | 227.52 ms [225.80 ms, 271.01 ms] | 5.69 ms [5.65 ms, 6.78 ms] | 35.1 [34.8, 41.8] | 1.000 [0.995, 1.183] | same |
| explicit_tet4_ss | stresses | base-t1 | 7 | 40 | 86.27 ms [85.15 ms, 86.88 ms] | 2.16 ms [2.13 ms, 2.17 ms] | 13.3 [13.1, 13.4] | – | same |
| explicit_tet4_ss | stresses | head2-t1 | 7 | 40 | 86.13 ms [85.22 ms, 86.93 ms] | 2.15 ms [2.13 ms, 2.17 ms] | 13.3 [13.2, 13.4] | 0.999 [0.991, 1.008] | same |
| explicit_tet4_ss | stresses | head4-t1 | 7 | 40 | 85.85 ms [85.35 ms, 87.38 ms] | 2.15 ms [2.13 ms, 2.18 ms] | 13.2 [13.2, 13.5] | 0.998 [0.989, 1.007] | same |
| explicit_tet4_ss | energies | base-t1 | 7 | 40 | 82.64 ms [82.44 ms, 83.79 ms] | 2.07 ms [2.06 ms, 2.09 ms] | 12.8 [12.7, 12.9] | – | same |
| explicit_tet4_ss | energies | head2-t1 | 7 | 40 | 86.74 ms [86.24 ms, 87.09 ms] | 2.17 ms [2.16 ms, 2.18 ms] | 13.4 [13.3, 13.4] | 1.043 [1.035, 1.055] | same |
| explicit_tet4_ss | energies | head4-t1 | 7 | 40 | 85.02 ms [84.56 ms, 85.86 ms] | 2.13 ms [2.11 ms, 2.15 ms] | 13.1 [13.0, 13.3] | 1.026 [1.014, 1.034] | same |
| explicit_tri3_fd | mesh | base-t1 | 7 | 1 | 4.20 ms [4.11 ms, 4.27 ms] | 4.20 ms [4.11 ms, 4.27 ms] | 23.3 [22.8, 23.7] | – | same |
| explicit_tri3_fd | mesh | head2-t1 | 7 | 1 | 4.30 ms [4.26 ms, 4.34 ms] | 4.30 ms [4.26 ms, 4.34 ms] | 23.9 [23.7, 24.1] | 1.020 [1.014, 1.048] | same |
| explicit_tri3_fd | mesh | head4-t1 | 7 | 1 | 4.20 ms [4.18 ms, 4.28 ms] | 4.20 ms [4.18 ms, 4.28 ms] | 23.4 [23.2, 23.8] | 1.014 [0.982, 1.023] | same |
| explicit_tri3_fd | init | base-t1 | 7 | 1 | 40.85 ms [40.61 ms, 41.62 ms] | 40.85 ms [40.61 ms, 41.62 ms] | 226.9 [225.6, 231.2] | – | same |
| explicit_tri3_fd | init | head2-t1 | 7 | 1 | 40.65 ms [40.42 ms, 41.64 ms] | 40.65 ms [40.42 ms, 41.64 ms] | 225.8 [224.5, 231.3] | 0.997 [0.980, 1.020] | same |
| explicit_tri3_fd | init | head4-t1 | 7 | 1 | 40.63 ms [40.54 ms, 40.98 ms] | 40.63 ms [40.54 ms, 40.98 ms] | 225.7 [225.2, 227.6] | 0.994 [0.980, 1.008] | same |
| explicit_tri3_fd | stable_time_step | base-t1 | 7 | 5 | 22.65 ms [22.55 ms, 22.86 ms] | 4.53 ms [4.51 ms, 4.57 ms] | 25.2 [25.1, 25.4] | – | same |
| explicit_tri3_fd | stable_time_step | head2-t1 | 7 | 5 | 22.68 ms [22.62 ms, 23.68 ms] | 4.54 ms [4.52 ms, 4.74 ms] | 25.2 [25.1, 26.3] | 0.999 [0.992, 1.044] | same |
| explicit_tri3_fd | stable_time_step | head4-t1 | 7 | 5 | 22.69 ms [22.60 ms, 24.35 ms] | 4.54 ms [4.52 ms, 4.87 ms] | 25.2 [25.1, 27.1] | 1.003 [0.996, 1.072] | same |
| explicit_tri3_fd | solve_step | base-t1 | 7 | 200 | 1.448 s [1.443 s, 1.451 s] | 7.24 ms [7.21 ms, 7.25 ms] | 40.2 [40.1, 40.3] | – | same |
| explicit_tri3_fd | solve_step | head2-t1 | 7 | 200 | 1.456 s [1.452 s, 1.461 s] | 7.28 ms [7.26 ms, 7.30 ms] | 40.5 [40.3, 40.6] | 1.005 [1.003, 1.012] | same |
| explicit_tri3_fd | solve_step | head4-t1 | 7 | 200 | 1.467 s [1.464 s, 1.473 s] | 7.33 ms [7.32 ms, 7.37 ms] | 40.7 [40.7, 40.9] | 1.013 [1.010, 1.021] | same |
| explicit_tri3_fd | predictor | base-t1 | 7 | 200 | 18.4 µs [9.0 µs, 30.7 µs] | 0.1 µs [0.0 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | predictor | head2-t1 | 7 | 200 | 15.8 µs [10.9 µs, 23.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.017 [0.355, 1.625] | same |
| explicit_tri3_fd | predictor | head4-t1 | 7 | 200 | 16.9 µs [10.1 µs, 25.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.803 [0.543, 1.621] | same |
| explicit_tri3_fd | residual | base-t1 | 7 | 200 | 1.273 s [1.269 s, 1.276 s] | 6.36 ms [6.34 ms, 6.38 ms] | 35.4 [35.2, 35.4] | – | same |
| explicit_tri3_fd | residual | head2-t1 | 7 | 200 | 1.276 s [1.271 s, 1.280 s] | 6.38 ms [6.35 ms, 6.40 ms] | 35.5 [35.3, 35.5] | 1.001 [0.999, 1.009] | same |
| explicit_tri3_fd | residual | head4-t1 | 7 | 200 | 1.272 s [1.269 s, 1.278 s] | 6.36 ms [6.35 ms, 6.39 ms] | 35.3 [35.3, 35.5] | 1.000 [0.996, 1.007] | same |
| explicit_tri3_fd | lumped_matrix | base-t1 | 7 | 200 | 34.4 µs [30.9 µs, 42.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | lumped_matrix | head2-t1 | 7 | 200 | 40.8 µs [37.1 µs, 47.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.226 [0.901, 1.389] | same |
| explicit_tri3_fd | lumped_matrix | head4-t1 | 7 | 200 | 43.8 µs [42.3 µs, 51.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.294 [1.025, 1.417] | same |
| explicit_tri3_fd | corrector | base-t1 | 7 | 200 | 23.5 µs [13.3 µs, 29.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | corrector | head2-t1 | 7 | 200 | 20.2 µs [19.2 µs, 29.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.915 [0.748, 1.689] | same |
| explicit_tri3_fd | corrector | head4-t1 | 7 | 200 | 21.2 µs [15.3 µs, 26.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.992 [0.726, 1.397] | same |
| explicit_tri3_fd | step_hooks | base-t1 | 7 | 400 | 640.9 µs [630.0 µs, 717.1 µs] | 1.6 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | step_hooks | head2-t1 | 7 | 400 | 669.9 µs [653.7 µs, 721.1 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.031 [0.992, 1.081] | same |
| explicit_tri3_fd | step_hooks | head4-t1 | 7 | 400 | 669.1 µs [616.5 µs, 711.4 µs] | 1.7 µs [1.5 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.035 [0.860, 1.125] | same |
| explicit_tri3_fd | step_rest | base-t1 | 7 | 200 | 174.16 ms [173.23 ms, 174.61 ms] | 870.8 µs [866.2 µs, 873.0 µs] | 4.8 [4.8, 4.9] | – | same |
| explicit_tri3_fd | step_rest | head2-t1 | 7 | 200 | 180.16 ms [179.31 ms, 180.59 ms] | 900.8 µs [896.6 µs, 903.0 µs] | 5.0 [5.0, 5.0] | 1.035 [1.033, 1.041] | same |
| explicit_tri3_fd | step_rest | head4-t1 | 7 | 200 | 193.61 ms [193.34 ms, 194.93 ms] | 968.1 µs [966.7 µs, 974.6 µs] | 5.4 [5.4, 5.4] | 1.116 [1.111, 1.125] | same |
| explicit_tri3_fd | internal_forces | base-t1 | 7 | 40 | 232.88 ms [232.12 ms, 233.08 ms] | 5.82 ms [5.80 ms, 5.83 ms] | 32.3 [32.2, 32.4] | – | same |
| explicit_tri3_fd | internal_forces | head2-t1 | 7 | 40 | 233.01 ms [232.61 ms, 233.83 ms] | 5.83 ms [5.82 ms, 5.85 ms] | 32.4 [32.3, 32.5] | 1.000 [0.998, 1.007] | same |
| explicit_tri3_fd | internal_forces | head4-t1 | 7 | 40 | 232.00 ms [231.71 ms, 234.42 ms] | 5.80 ms [5.79 ms, 5.86 ms] | 32.2 [32.2, 32.6] | 0.997 [0.994, 1.008] | same |
| explicit_tri3_fd | stresses | base-t1 | 7 | 40 | 55.69 ms [55.50 ms, 55.95 ms] | 1.39 ms [1.39 ms, 1.40 ms] | 7.7 [7.7, 7.8] | – | same |
| explicit_tri3_fd | stresses | head2-t1 | 7 | 40 | 55.59 ms [55.30 ms, 56.61 ms] | 1.39 ms [1.38 ms, 1.42 ms] | 7.7 [7.7, 7.9] | 0.998 [0.989, 1.017] | same |
| explicit_tri3_fd | stresses | head4-t1 | 7 | 40 | 55.84 ms [55.19 ms, 56.17 ms] | 1.40 ms [1.38 ms, 1.40 ms] | 7.8 [7.7, 7.8] | 1.002 [0.991, 1.010] | same |
| explicit_tri3_fd | energies | base-t1 | 7 | 40 | 96.37 ms [95.88 ms, 96.58 ms] | 2.41 ms [2.40 ms, 2.41 ms] | 13.4 [13.3, 13.4] | – | same |
| explicit_tri3_fd | energies | head2-t1 | 7 | 40 | 100.46 ms [100.34 ms, 101.47 ms] | 2.51 ms [2.51 ms, 2.54 ms] | 14.0 [13.9, 14.1] | 1.046 [1.039, 1.053] | same |
| explicit_tri3_fd | energies | head4-t1 | 7 | 40 | 99.08 ms [98.69 ms, 99.54 ms] | 2.48 ms [2.47 ms, 2.49 ms] | 13.8 [13.7, 13.8] | 1.028 [1.022, 1.038] | same |
| explicit_tri3_ss | mesh | base-t1 | 7 | 1 | 4.15 ms [4.00 ms, 4.21 ms] | 4.15 ms [4.00 ms, 4.21 ms] | 23.1 [22.2, 23.4] | – | same |
| explicit_tri3_ss | mesh | head2-t1 | 7 | 1 | 4.29 ms [4.23 ms, 4.31 ms] | 4.29 ms [4.23 ms, 4.31 ms] | 23.8 [23.5, 23.9] | 1.033 [1.005, 1.075] | same |
| explicit_tri3_ss | mesh | head4-t1 | 7 | 1 | 4.19 ms [4.10 ms, 4.23 ms] | 4.19 ms [4.10 ms, 4.23 ms] | 23.3 [22.8, 23.5] | 0.997 [0.994, 1.057] | same |
| explicit_tri3_ss | init | base-t1 | 7 | 1 | 38.20 ms [37.72 ms, 38.87 ms] | 38.20 ms [37.72 ms, 38.87 ms] | 212.2 [209.6, 215.9] | – | same |
| explicit_tri3_ss | init | head2-t1 | 7 | 1 | 37.72 ms [37.13 ms, 37.86 ms] | 37.72 ms [37.13 ms, 37.86 ms] | 209.5 [206.3, 210.3] | 0.979 [0.974, 0.998] | same |
| explicit_tri3_ss | init | head4-t1 | 7 | 1 | 37.67 ms [37.39 ms, 38.08 ms] | 37.67 ms [37.39 ms, 38.08 ms] | 209.3 [207.7, 211.5] | 0.991 [0.977, 0.997] | same |
| explicit_tri3_ss | stable_time_step | base-t1 | 7 | 5 | 22.66 ms [22.58 ms, 23.57 ms] | 4.53 ms [4.52 ms, 4.71 ms] | 25.2 [25.1, 26.2] | – | same |
| explicit_tri3_ss | stable_time_step | head2-t1 | 7 | 5 | 22.63 ms [22.57 ms, 23.56 ms] | 4.53 ms [4.51 ms, 4.71 ms] | 25.1 [25.1, 26.2] | 0.999 [0.971, 1.040] | same |
| explicit_tri3_ss | stable_time_step | head4-t1 | 7 | 5 | 22.60 ms [22.51 ms, 23.84 ms] | 4.52 ms [4.50 ms, 4.77 ms] | 25.1 [25.0, 26.5] | 0.996 [0.964, 1.052] | same |
| explicit_tri3_ss | solve_step | base-t1 | 7 | 300 | 1.514 s [1.509 s, 1.522 s] | 5.05 ms [5.03 ms, 5.07 ms] | 28.0 [28.0, 28.2] | – | same |
| explicit_tri3_ss | solve_step | head2-t1 | 7 | 300 | 1.517 s [1.516 s, 1.532 s] | 5.06 ms [5.05 ms, 5.11 ms] | 28.1 [28.1, 28.4] | 1.005 [1.001, 1.010] | same |
| explicit_tri3_ss | solve_step | head4-t1 | 7 | 300 | 1.549 s [1.534 s, 1.553 s] | 5.16 ms [5.11 ms, 5.18 ms] | 28.7 [28.4, 28.8] | 1.022 [1.014, 1.028] | same |
| explicit_tri3_ss | predictor | base-t1 | 7 | 300 | 23.0 µs [13.7 µs, 31.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | head2-t1 | 7 | 300 | 28.5 µs [16.3 µs, 31.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.184 [0.914, 1.689] | same |
| explicit_tri3_ss | predictor | head4-t1 | 7 | 300 | 20.2 µs [13.9 µs, 42.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.052 [0.628, 1.504] | same |
| explicit_tri3_ss | residual | base-t1 | 7 | 300 | 1.251 s [1.248 s, 1.259 s] | 4.17 ms [4.16 ms, 4.20 ms] | 23.2 [23.1, 23.3] | – | same |
| explicit_tri3_ss | residual | head2-t1 | 7 | 300 | 1.247 s [1.245 s, 1.259 s] | 4.16 ms [4.15 ms, 4.20 ms] | 23.1 [23.1, 23.3] | 0.998 [0.995, 1.004] | same |
| explicit_tri3_ss | residual | head4-t1 | 7 | 300 | 1.255 s [1.242 s, 1.260 s] | 4.18 ms [4.14 ms, 4.20 ms] | 23.2 [23.0, 23.3] | 1.002 [0.993, 1.008] | same |
| explicit_tri3_ss | lumped_matrix | base-t1 | 7 | 300 | 54.9 µs [41.5 µs, 62.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | head2-t1 | 7 | 300 | 67.5 µs [51.9 µs, 76.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.229 [0.963, 1.780] | same |
| explicit_tri3_ss | lumped_matrix | head4-t1 | 7 | 300 | 65.2 µs [57.5 µs, 75.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.191 [1.050, 1.384] | same |
| explicit_tri3_ss | corrector | base-t1 | 7 | 300 | 24.9 µs [18.4 µs, 37.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | head2-t1 | 7 | 300 | 39.8 µs [24.0 µs, 45.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.376 [1.215, 2.301] | same |
| explicit_tri3_ss | corrector | head4-t1 | 7 | 300 | 30.1 µs [19.9 µs, 51.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.522 [0.800, 1.744] | same |
| explicit_tri3_ss | step_hooks | base-t1 | 7 | 600 | 803.8 µs [763.3 µs, 865.3 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | head2-t1 | 7 | 600 | 893.6 µs [801.9 µs, 912.3 µs] | 1.5 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.073 [0.934, 1.195] | same |
| explicit_tri3_ss | step_hooks | head4-t1 | 7 | 600 | 815.6 µs [795.4 µs, 887.2 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.009 [0.921, 1.162] | same |
| explicit_tri3_ss | step_rest | base-t1 | 7 | 300 | 260.96 ms [259.44 ms, 261.93 ms] | 869.9 µs [864.8 µs, 873.1 µs] | 4.8 [4.8, 4.9] | – | same |
| explicit_tri3_ss | step_rest | head2-t1 | 7 | 300 | 269.69 ms [268.84 ms, 271.43 ms] | 899.0 µs [896.1 µs, 904.8 µs] | 5.0 [5.0, 5.0] | 1.035 [1.029, 1.040] | same |
| explicit_tri3_ss | step_rest | head4-t1 | 7 | 300 | 291.91 ms [290.39 ms, 293.06 ms] | 973.0 µs [968.0 µs, 976.9 µs] | 5.4 [5.4, 5.4] | 1.118 [1.113, 1.126] | same |
| explicit_tri3_ss | internal_forces | base-t1 | 7 | 60 | 211.16 ms [210.04 ms, 212.62 ms] | 3.52 ms [3.50 ms, 3.54 ms] | 19.6 [19.4, 19.7] | – | same |
| explicit_tri3_ss | internal_forces | head2-t1 | 7 | 60 | 211.77 ms [210.70 ms, 213.15 ms] | 3.53 ms [3.51 ms, 3.55 ms] | 19.6 [19.5, 19.7] | 1.002 [0.994, 1.012] | same |
| explicit_tri3_ss | internal_forces | head4-t1 | 7 | 60 | 211.38 ms [209.55 ms, 212.99 ms] | 3.52 ms [3.49 ms, 3.55 ms] | 19.6 [19.4, 19.7] | 1.002 [0.992, 1.007] | same |
| explicit_tri3_ss | stresses | base-t1 | 7 | 60 | 73.51 ms [73.05 ms, 74.73 ms] | 1.23 ms [1.22 ms, 1.25 ms] | 6.8 [6.8, 6.9] | – | same |
| explicit_tri3_ss | stresses | head2-t1 | 7 | 60 | 73.81 ms [73.03 ms, 74.43 ms] | 1.23 ms [1.22 ms, 1.24 ms] | 6.8 [6.8, 6.9] | 1.002 [0.996, 1.009] | same |
| explicit_tri3_ss | stresses | head4-t1 | 7 | 60 | 73.69 ms [73.20 ms, 73.95 ms] | 1.23 ms [1.22 ms, 1.23 ms] | 6.8 [6.8, 6.8] | 1.002 [0.990, 1.009] | same |
| explicit_tri3_ss | energies | base-t1 | 7 | 60 | 137.87 ms [137.16 ms, 138.29 ms] | 2.30 ms [2.29 ms, 2.30 ms] | 12.8 [12.7, 12.8] | – | same |
| explicit_tri3_ss | energies | head2-t1 | 7 | 60 | 142.95 ms [142.74 ms, 143.34 ms] | 2.38 ms [2.38 ms, 2.39 ms] | 13.2 [13.2, 13.3] | 1.037 [1.036, 1.041] | same |
| explicit_tri3_ss | energies | head4-t1 | 7 | 60 | 140.79 ms [139.77 ms, 140.99 ms] | 2.35 ms [2.33 ms, 2.35 ms] | 13.0 [12.9, 13.1] | 1.020 [1.013, 1.026] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | 7a6ad8f42, 85d4d74d4, c866d99ab | 78b072c2f464d8f9 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | 7a6ad8f42, 85d4d74d4, c866d99ab | a12864e94a9122c8 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42, 85d4d74d4, c866d99ab | fecb3fb4b46c8b8c |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 7a6ad8f42, 85d4d74d4, c866d99ab | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7a6ad8f42, 85d4d74d4, c866d99ab | b6b902f5ca71049e |
