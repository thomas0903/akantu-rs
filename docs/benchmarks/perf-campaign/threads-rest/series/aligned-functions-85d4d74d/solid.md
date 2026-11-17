Suite `solid`, size `m`; variants: `al-base`, `al-head4`, `base-t1`, `head4-t1`; reference `al-base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to al-base | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars | mesh | al-base | 7 | 1 | 2.09 ms [2.06 ms, 2.21 ms] | 2.09 ms [2.06 ms, 2.21 ms] | 19.9 [19.5, 21.0] | – | same |
| explicit_mazars | mesh | al-head4 | 7 | 1 | 2.11 ms [2.07 ms, 2.19 ms] | 2.11 ms [2.07 ms, 2.19 ms] | 20.0 [19.6, 20.8] | 0.997 [0.963, 1.053] | same |
| explicit_mazars | mesh | base-t1 | 7 | 1 | 2.09 ms [2.05 ms, 2.16 ms] | 2.09 ms [2.05 ms, 2.16 ms] | 19.8 [19.5, 20.5] | 0.985 [0.949, 1.032] | same |
| explicit_mazars | mesh | head4-t1 | 7 | 1 | 2.08 ms [2.03 ms, 2.11 ms] | 2.08 ms [2.03 ms, 2.11 ms] | 19.7 [19.3, 20.0] | 0.982 [0.955, 1.012] | same |
| explicit_mazars | init | al-base | 7 | 1 | 33.22 ms [32.44 ms, 33.68 ms] | 33.22 ms [32.44 ms, 33.68 ms] | 315.1 [307.7, 319.3] | – | same |
| explicit_mazars | init | al-head4 | 7 | 1 | 33.21 ms [32.32 ms, 33.34 ms] | 33.21 ms [32.32 ms, 33.34 ms] | 314.9 [306.5, 316.2] | 0.986 [0.977, 1.028] | same |
| explicit_mazars | init | base-t1 | 7 | 1 | 32.75 ms [32.36 ms, 33.19 ms] | 32.75 ms [32.36 ms, 33.19 ms] | 310.5 [306.9, 314.8] | 0.980 [0.967, 1.023] | same |
| explicit_mazars | init | head4-t1 | 7 | 1 | 32.75 ms [32.18 ms, 33.43 ms] | 32.75 ms [32.18 ms, 33.43 ms] | 310.5 [305.1, 317.0] | 0.980 [0.969, 1.016] | same |
| explicit_mazars | stable_time_step | al-base | 7 | 5 | 21.06 ms [20.95 ms, 21.54 ms] | 4.21 ms [4.19 ms, 4.31 ms] | 39.9 [39.7, 40.8] | – | same |
| explicit_mazars | stable_time_step | al-head4 | 7 | 5 | 21.10 ms [21.02 ms, 21.70 ms] | 4.22 ms [4.20 ms, 4.34 ms] | 40.0 [39.9, 41.2] | 1.007 [0.977, 1.030] | same |
| explicit_mazars | stable_time_step | base-t1 | 7 | 5 | 20.99 ms [20.91 ms, 21.65 ms] | 4.20 ms [4.18 ms, 4.33 ms] | 39.8 [39.7, 41.1] | 0.997 [0.978, 1.017] | same |
| explicit_mazars | stable_time_step | head4-t1 | 7 | 5 | 21.13 ms [21.07 ms, 21.38 ms] | 4.23 ms [4.21 ms, 4.28 ms] | 40.1 [40.0, 40.5] | 1.003 [0.979, 1.019] | same |
| explicit_mazars | solve_step | al-base | 7 | 80 | 2.701 s [2.684 s, 2.706 s] | 33.76 ms [33.55 ms, 33.82 ms] | 320.1 [318.2, 320.7] | – | same |
| explicit_mazars | solve_step | al-head4 | 7 | 80 | 2.695 s [2.683 s, 2.762 s] | 33.68 ms [33.53 ms, 34.52 ms] | 319.4 [318.0, 327.3] | 1.000 [0.993, 1.023] | same |
| explicit_mazars | solve_step | base-t1 | 7 | 80 | 2.684 s [2.675 s, 2.692 s] | 33.56 ms [33.44 ms, 33.66 ms] | 318.2 [317.1, 319.1] | 0.994 [0.991, 1.001] | same |
| explicit_mazars | solve_step | head4-t1 | 7 | 80 | 2.697 s [2.682 s, 2.716 s] | 33.71 ms [33.52 ms, 33.95 ms] | 319.6 [317.9, 321.9] | 0.998 [0.994, 1.011] | same |
| explicit_mazars | predictor | al-base | 7 | 80 | 9.5 µs [7.3 µs, 14.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | al-head4 | 7 | 80 | 13.7 µs [12.7 µs, 17.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.434 [0.981, 1.779] | same |
| explicit_mazars | predictor | base-t1 | 7 | 80 | 10.5 µs [5.2 µs, 12.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.014 [0.570, 1.664] | same |
| explicit_mazars | predictor | head4-t1 | 7 | 80 | 13.6 µs [12.1 µs, 15.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.283 [0.971, 2.190] | same |
| explicit_mazars | residual | al-base | 7 | 80 | 2.538 s [2.521 s, 2.540 s] | 31.72 ms [31.51 ms, 31.76 ms] | 300.8 [298.8, 301.1] | – | same |
| explicit_mazars | residual | al-head4 | 7 | 80 | 2.531 s [2.518 s, 2.597 s] | 31.64 ms [31.48 ms, 32.46 ms] | 300.0 [298.5, 307.8] | 0.998 [0.993, 1.023] | same |
| explicit_mazars | residual | base-t1 | 7 | 80 | 2.522 s [2.513 s, 2.529 s] | 31.53 ms [31.41 ms, 31.61 ms] | 299.0 [297.8, 299.8] | 0.994 [0.990, 1.001] | same |
| explicit_mazars | residual | head4-t1 | 7 | 80 | 2.531 s [2.518 s, 2.549 s] | 31.64 ms [31.48 ms, 31.86 ms] | 300.0 [298.5, 302.1] | 0.997 [0.994, 1.010] | same |
| explicit_mazars | lumped_matrix | al-base | 7 | 80 | 17.7 µs [15.9 µs, 31.9 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | al-head4 | 7 | 80 | 19.0 µs [15.8 µs, 22.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.990 [0.625, 1.072] | same |
| explicit_mazars | lumped_matrix | base-t1 | 7 | 80 | 18.0 µs [15.6 µs, 22.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.933 [0.689, 1.133] | same |
| explicit_mazars | lumped_matrix | head4-t1 | 7 | 80 | 17.4 µs [15.3 µs, 25.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.998 [0.543, 1.530] | same |
| explicit_mazars | corrector | al-base | 7 | 80 | 14.2 µs [12.1 µs, 22.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | al-head4 | 7 | 80 | 21.2 µs [14.5 µs, 23.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.497 [0.679, 1.749] | same |
| explicit_mazars | corrector | base-t1 | 7 | 80 | 13.2 µs [12.6 µs, 22.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.934 [0.574, 1.884] | same |
| explicit_mazars | corrector | head4-t1 | 7 | 80 | 21.2 µs [20.5 µs, 22.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.497 [0.959, 1.686] | same |
| explicit_mazars | step_hooks | al-base | 7 | 160 | 132.96 ms [132.32 ms, 134.43 ms] | 831.0 µs [827.0 µs, 840.2 µs] | 7.9 [7.8, 8.0] | – | same |
| explicit_mazars | step_hooks | al-head4 | 7 | 160 | 134.12 ms [133.70 ms, 135.63 ms] | 838.2 µs [835.6 µs, 847.7 µs] | 7.9 [7.9, 8.0] | 1.007 [0.995, 1.025] | same |
| explicit_mazars | step_hooks | base-t1 | 7 | 160 | 132.60 ms [132.45 ms, 133.37 ms] | 828.8 µs [827.8 µs, 833.6 µs] | 7.9 [7.8, 7.9] | 0.997 [0.989, 1.002] | same |
| explicit_mazars | step_hooks | head4-t1 | 7 | 160 | 134.48 ms [132.98 ms, 135.18 ms] | 840.5 µs [831.1 µs, 844.9 µs] | 8.0 [7.9, 8.0] | 1.008 [1.000, 1.017] | same |
| explicit_mazars | step_rest | al-base | 7 | 80 | 29.83 ms [29.40 ms, 30.30 ms] | 372.8 µs [367.5 µs, 378.8 µs] | 3.5 [3.5, 3.6] | – | same |
| explicit_mazars | step_rest | al-head4 | 7 | 80 | 29.89 ms [29.51 ms, 30.05 ms] | 373.6 µs [368.9 µs, 375.6 µs] | 3.5 [3.5, 3.6] | 1.004 [0.986, 1.019] | same |
| explicit_mazars | step_rest | base-t1 | 7 | 80 | 29.47 ms [29.29 ms, 29.81 ms] | 368.3 µs [366.2 µs, 372.6 µs] | 3.5 [3.5, 3.5] | 0.989 [0.974, 1.004] | same |
| explicit_mazars | step_rest | head4-t1 | 7 | 80 | 30.63 ms [30.06 ms, 31.05 ms] | 382.9 µs [375.8 µs, 388.2 µs] | 3.6 [3.6, 3.7] | 1.018 [1.013, 1.051] | same |
| explicit_mazars | internal_forces | al-base | 7 | 16 | 545.28 ms [539.28 ms, 548.64 ms] | 34.08 ms [33.70 ms, 34.29 ms] | 323.2 [319.6, 325.2] | – | same |
| explicit_mazars | internal_forces | al-head4 | 7 | 16 | 545.52 ms [540.43 ms, 551.16 ms] | 34.09 ms [33.78 ms, 34.45 ms] | 323.3 [320.3, 326.7] | 1.002 [0.993, 1.011] | same |
| explicit_mazars | internal_forces | base-t1 | 7 | 16 | 541.82 ms [539.20 ms, 543.75 ms] | 33.86 ms [33.70 ms, 33.98 ms] | 321.1 [319.6, 322.3] | 0.997 [0.983, 1.004] | same |
| explicit_mazars | internal_forces | head4-t1 | 7 | 16 | 546.95 ms [543.14 ms, 553.97 ms] | 34.18 ms [33.95 ms, 34.62 ms] | 324.2 [321.9, 328.3] | 1.006 [0.992, 1.015] | same |
| explicit_mazars | stresses | al-base | 7 | 16 | 510.50 ms [509.33 ms, 512.87 ms] | 31.91 ms [31.83 ms, 32.05 ms] | 302.6 [301.9, 304.0] | – | same |
| explicit_mazars | stresses | al-head4 | 7 | 16 | 508.71 ms [506.42 ms, 513.55 ms] | 31.79 ms [31.65 ms, 32.10 ms] | 301.5 [300.1, 304.4] | 0.997 [0.989, 1.008] | same |
| explicit_mazars | stresses | base-t1 | 7 | 16 | 508.34 ms [505.99 ms, 510.24 ms] | 31.77 ms [31.62 ms, 31.89 ms] | 301.3 [299.9, 302.4] | 0.996 [0.988, 0.999] | same |
| explicit_mazars | stresses | head4-t1 | 7 | 16 | 510.09 ms [507.87 ms, 524.47 ms] | 31.88 ms [31.74 ms, 32.78 ms] | 302.3 [301.0, 310.8] | 0.999 [0.990, 1.027] | same |
| explicit_mazars | energies | al-base | 7 | 16 | 23.17 ms [22.99 ms, 23.73 ms] | 1.45 ms [1.44 ms, 1.48 ms] | 13.7 [13.6, 14.1] | – | same |
| explicit_mazars | energies | al-head4 | 7 | 16 | 24.17 ms [23.96 ms, 24.57 ms] | 1.51 ms [1.50 ms, 1.54 ms] | 14.3 [14.2, 14.6] | 1.038 [1.034, 1.058] | same |
| explicit_mazars | energies | base-t1 | 7 | 16 | 22.69 ms [22.39 ms, 22.88 ms] | 1.42 ms [1.40 ms, 1.43 ms] | 13.4 [13.3, 13.6] | 0.974 [0.963, 0.995] | same |
| explicit_mazars | energies | head4-t1 | 7 | 16 | 23.76 ms [23.23 ms, 24.06 ms] | 1.49 ms [1.45 ms, 1.50 ms] | 14.1 [13.8, 14.3] | 1.025 [0.997, 1.034] | same |
| explicit_tet10_ss | mesh | al-base | 7 | 1 | 7.09 ms [6.98 ms, 7.11 ms] | 7.09 ms [6.98 ms, 7.11 ms] | 67.2 [66.2, 67.5] | – | same |
| explicit_tet10_ss | mesh | al-head4 | 7 | 1 | 7.09 ms [6.96 ms, 7.22 ms] | 7.09 ms [6.96 ms, 7.22 ms] | 67.2 [66.0, 68.4] | 1.004 [0.982, 1.018] | same |
| explicit_tet10_ss | mesh | base-t1 | 7 | 1 | 6.89 ms [6.86 ms, 6.94 ms] | 6.89 ms [6.86 ms, 6.94 ms] | 65.4 [65.1, 65.8] | 0.970 [0.966, 0.995] | same |
| explicit_tet10_ss | mesh | head4-t1 | 7 | 1 | 6.98 ms [6.89 ms, 7.01 ms] | 6.98 ms [6.89 ms, 7.01 ms] | 66.2 [65.3, 66.4] | 0.985 [0.970, 0.987] | same |
| explicit_tet10_ss | init | al-base | 7 | 1 | 175.42 ms [174.75 ms, 185.01 ms] | 175.42 ms [174.75 ms, 185.01 ms] | 1663.4 [1657.0, 1754.4] | – | same |
| explicit_tet10_ss | init | al-head4 | 7 | 1 | 178.49 ms [175.26 ms, 180.77 ms] | 178.49 ms [175.26 ms, 180.77 ms] | 1692.5 [1661.9, 1714.2] | 1.021 [0.961, 1.032] | same |
| explicit_tet10_ss | init | base-t1 | 7 | 1 | 175.50 ms [174.07 ms, 180.60 ms] | 175.50 ms [174.07 ms, 180.60 ms] | 1664.2 [1650.6, 1712.5] | 0.994 [0.954, 1.030] | same |
| explicit_tet10_ss | init | head4-t1 | 7 | 1 | 175.43 ms [173.55 ms, 182.07 ms] | 175.43 ms [173.55 ms, 182.07 ms] | 1663.6 [1645.7, 1726.5] | 0.998 [0.953, 1.038] | same |
| explicit_tet10_ss | stable_time_step | al-base | 7 | 5 | 62.55 ms [62.44 ms, 63.42 ms] | 12.51 ms [12.49 ms, 12.68 ms] | 118.6 [118.4, 120.3] | – | same |
| explicit_tet10_ss | stable_time_step | al-head4 | 7 | 5 | 62.15 ms [62.00 ms, 62.32 ms] | 12.43 ms [12.40 ms, 12.46 ms] | 117.9 [117.6, 118.2] | 0.993 [0.980, 0.996] | same |
| explicit_tet10_ss | stable_time_step | base-t1 | 7 | 5 | 62.39 ms [62.35 ms, 62.63 ms] | 12.48 ms [12.47 ms, 12.53 ms] | 118.3 [118.3, 118.8] | 0.999 [0.984, 1.001] | same |
| explicit_tet10_ss | stable_time_step | head4-t1 | 7 | 5 | 61.70 ms [61.66 ms, 61.92 ms] | 12.34 ms [12.33 ms, 12.38 ms] | 117.0 [116.9, 117.4] | 0.988 [0.973, 0.990] | same |
| explicit_tet10_ss | solve_step | al-base | 7 | 50 | 1.014 s [1.011 s, 1.027 s] | 20.28 ms [20.21 ms, 20.54 ms] | 192.4 [191.7, 194.7] | – | same |
| explicit_tet10_ss | solve_step | al-head4 | 7 | 50 | 1.005 s [1.002 s, 1.013 s] | 20.11 ms [20.04 ms, 20.26 ms] | 190.7 [190.0, 192.1] | 0.991 [0.982, 0.995] | same |
| explicit_tet10_ss | solve_step | base-t1 | 7 | 50 | 1.005 s [993.69 ms, 1.007 s] | 20.10 ms [19.87 ms, 20.14 ms] | 190.6 [188.5, 191.0] | 0.985 [0.974, 0.994] | same |
| explicit_tet10_ss | solve_step | head4-t1 | 7 | 50 | 1.009 s [1.006 s, 1.020 s] | 20.19 ms [20.11 ms, 20.40 ms] | 191.4 [190.7, 193.5] | 0.997 [0.979, 1.002] | same |
| explicit_tet10_ss | predictor | al-base | 7 | 50 | 8.2 µs [7.6 µs, 8.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | predictor | al-head4 | 7 | 50 | 9.9 µs [8.5 µs, 11.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.222 [1.079, 1.327] | same |
| explicit_tet10_ss | predictor | base-t1 | 7 | 50 | 8.2 µs [6.1 µs, 8.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.000 [0.703, 1.109] | same |
| explicit_tet10_ss | predictor | head4-t1 | 7 | 50 | 8.7 µs [7.4 µs, 10.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.043 [0.895, 1.304] | same |
| explicit_tet10_ss | residual | al-base | 7 | 50 | 883.99 ms [881.58 ms, 894.92 ms] | 17.68 ms [17.63 ms, 17.90 ms] | 167.7 [167.2, 169.7] | – | same |
| explicit_tet10_ss | residual | al-head4 | 7 | 50 | 874.95 ms [871.63 ms, 881.60 ms] | 17.50 ms [17.43 ms, 17.63 ms] | 165.9 [165.3, 167.2] | 0.989 [0.983, 0.994] | same |
| explicit_tet10_ss | residual | base-t1 | 7 | 50 | 877.50 ms [868.90 ms, 880.82 ms] | 17.55 ms [17.38 ms, 17.62 ms] | 166.4 [164.8, 167.1] | 0.988 [0.980, 0.994] | same |
| explicit_tet10_ss | residual | head4-t1 | 7 | 50 | 873.15 ms [870.13 ms, 883.33 ms] | 17.46 ms [17.40 ms, 17.67 ms] | 165.6 [165.0, 167.5] | 0.988 [0.972, 0.992] | same |
| explicit_tet10_ss | lumped_matrix | al-base | 7 | 50 | 15.8 µs [13.8 µs, 21.7 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | lumped_matrix | al-head4 | 7 | 50 | 18.1 µs [13.2 µs, 25.2 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.156 [0.794, 1.708] | same |
| explicit_tet10_ss | lumped_matrix | base-t1 | 7 | 50 | 18.5 µs [12.5 µs, 20.6 µs] | 0.4 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.169 [0.640, 1.499] | same |
| explicit_tet10_ss | lumped_matrix | head4-t1 | 7 | 50 | 15.5 µs [14.2 µs, 17.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.966 [0.716, 1.124] | same |
| explicit_tet10_ss | corrector | al-base | 7 | 50 | 9.0 µs [7.5 µs, 18.2 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | corrector | al-head4 | 7 | 50 | 12.8 µs [11.5 µs, 13.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.547 [0.656, 1.690] | same |
| explicit_tet10_ss | corrector | base-t1 | 7 | 50 | 8.2 µs [7.5 µs, 10.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.947 [0.451, 1.110] | same |
| explicit_tet10_ss | corrector | head4-t1 | 7 | 50 | 12.5 µs [9.6 µs, 13.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.347 [0.623, 1.677] | same |
| explicit_tet10_ss | step_hooks | al-base | 7 | 100 | 163.6 µs [159.6 µs, 173.1 µs] | 1.6 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | step_hooks | al-head4 | 7 | 100 | 163.8 µs [160.0 µs, 172.9 µs] | 1.6 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 1.010 [0.940, 1.063] | same |
| explicit_tet10_ss | step_hooks | base-t1 | 7 | 100 | 170.8 µs [162.3 µs, 178.5 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.044 [0.997, 1.119] | same |
| explicit_tet10_ss | step_hooks | head4-t1 | 7 | 100 | 172.7 µs [164.7 µs, 181.4 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.068 [0.961, 1.115] | same |
| explicit_tet10_ss | step_rest | al-base | 7 | 50 | 130.05 ms [128.72 ms, 132.82 ms] | 2.60 ms [2.57 ms, 2.66 ms] | 24.7 [24.4, 25.2] | – | same |
| explicit_tet10_ss | step_rest | al-head4 | 7 | 50 | 129.80 ms [129.42 ms, 130.97 ms] | 2.60 ms [2.59 ms, 2.62 ms] | 24.6 [24.5, 24.8] | 1.001 [0.978, 1.006] | same |
| explicit_tet10_ss | step_rest | base-t1 | 7 | 50 | 125.96 ms [124.21 ms, 128.29 ms] | 2.52 ms [2.48 ms, 2.57 ms] | 23.9 [23.6, 24.3] | 0.965 [0.938, 0.996] | same |
| explicit_tet10_ss | step_rest | head4-t1 | 7 | 50 | 135.97 ms [135.35 ms, 137.32 ms] | 2.72 ms [2.71 ms, 2.75 ms] | 25.8 [25.7, 26.0] | 1.046 [1.019, 1.067] | same |
| explicit_tet10_ss | internal_forces | al-base | 7 | 10 | 166.77 ms [164.81 ms, 172.05 ms] | 16.68 ms [16.48 ms, 17.20 ms] | 158.1 [156.3, 163.1] | – | same |
| explicit_tet10_ss | internal_forces | al-head4 | 7 | 10 | 164.94 ms [163.23 ms, 166.54 ms] | 16.49 ms [16.32 ms, 16.65 ms] | 156.4 [154.8, 157.9] | 0.991 [0.949, 0.997] | same |
| explicit_tet10_ss | internal_forces | base-t1 | 7 | 10 | 164.08 ms [162.52 ms, 169.38 ms] | 16.41 ms [16.25 ms, 16.94 ms] | 155.6 [154.1, 160.6] | 0.989 [0.954, 1.008] | same |
| explicit_tet10_ss | internal_forces | head4-t1 | 7 | 10 | 165.37 ms [163.59 ms, 169.02 ms] | 16.54 ms [16.36 ms, 16.90 ms] | 156.8 [155.1, 160.3] | 0.990 [0.954, 1.026] | same |
| explicit_tet10_ss | stresses | al-base | 7 | 10 | 116.04 ms [112.84 ms, 127.92 ms] | 11.60 ms [11.28 ms, 12.79 ms] | 110.0 [107.0, 121.3] | – | same |
| explicit_tet10_ss | stresses | al-head4 | 7 | 10 | 105.92 ms [102.87 ms, 117.12 ms] | 10.59 ms [10.29 ms, 11.71 ms] | 100.4 [97.5, 111.1] | 0.920 [0.822, 1.002] | same |
| explicit_tet10_ss | stresses | base-t1 | 7 | 10 | 105.95 ms [103.85 ms, 115.05 ms] | 10.59 ms [10.39 ms, 11.51 ms] | 100.5 [98.5, 109.1] | 0.919 [0.812, 1.012] | same |
| explicit_tet10_ss | stresses | head4-t1 | 7 | 10 | 107.45 ms [102.73 ms, 116.17 ms] | 10.74 ms [10.27 ms, 11.62 ms] | 101.9 [97.4, 110.2] | 0.926 [0.803, 1.027] | same |
| explicit_tet10_ss | energies | al-base | 7 | 10 | 35.15 ms [34.65 ms, 35.37 ms] | 3.52 ms [3.47 ms, 3.54 ms] | 33.3 [32.9, 33.5] | – | same |
| explicit_tet10_ss | energies | al-head4 | 7 | 10 | 35.03 ms [34.76 ms, 35.82 ms] | 3.50 ms [3.48 ms, 3.58 ms] | 33.2 [33.0, 34.0] | 1.001 [0.991, 1.020] | same |
| explicit_tet10_ss | energies | base-t1 | 7 | 10 | 34.72 ms [34.09 ms, 36.28 ms] | 3.47 ms [3.41 ms, 3.63 ms] | 32.9 [32.3, 34.4] | 1.002 [0.965, 1.033] | same |
| explicit_tet10_ss | energies | head4-t1 | 7 | 10 | 34.70 ms [34.22 ms, 35.92 ms] | 3.47 ms [3.42 ms, 3.59 ms] | 32.9 [32.4, 34.1] | 0.990 [0.969, 1.037] | same |
| explicit_tet4_ss | mesh | al-base | 7 | 1 | 3.25 ms [3.19 ms, 3.38 ms] | 3.25 ms [3.19 ms, 3.38 ms] | 20.1 [19.7, 20.9] | – | same |
| explicit_tet4_ss | mesh | al-head4 | 7 | 1 | 3.24 ms [3.12 ms, 3.34 ms] | 3.24 ms [3.12 ms, 3.34 ms] | 20.0 [19.3, 20.6] | 0.996 [0.955, 1.046] | same |
| explicit_tet4_ss | mesh | base-t1 | 7 | 1 | 3.21 ms [3.17 ms, 3.29 ms] | 3.21 ms [3.17 ms, 3.29 ms] | 19.8 [19.6, 20.3] | 0.986 [0.955, 1.023] | same |
| explicit_tet4_ss | mesh | head4-t1 | 7 | 1 | 3.24 ms [3.18 ms, 3.33 ms] | 3.24 ms [3.18 ms, 3.33 ms] | 20.0 [19.6, 20.6] | 0.993 [0.950, 1.025] | same |
| explicit_tet4_ss | init | al-base | 7 | 1 | 48.45 ms [47.29 ms, 48.82 ms] | 48.45 ms [47.29 ms, 48.82 ms] | 299.0 [291.9, 301.4] | – | same |
| explicit_tet4_ss | init | al-head4 | 7 | 1 | 47.68 ms [47.01 ms, 48.39 ms] | 47.68 ms [47.01 ms, 48.39 ms] | 294.3 [290.2, 298.7] | 0.985 [0.966, 1.013] | same |
| explicit_tet4_ss | init | base-t1 | 7 | 1 | 48.19 ms [47.52 ms, 48.82 ms] | 48.19 ms [47.52 ms, 48.82 ms] | 297.5 [293.3, 301.4] | 1.006 [0.976, 1.032] | same |
| explicit_tet4_ss | init | head4-t1 | 7 | 1 | 47.18 ms [46.63 ms, 48.17 ms] | 47.18 ms [46.63 ms, 48.17 ms] | 291.2 [287.8, 297.3] | 0.979 [0.960, 0.992] | same |
| explicit_tet4_ss | stable_time_step | al-base | 7 | 5 | 32.21 ms [32.14 ms, 32.27 ms] | 6.44 ms [6.43 ms, 6.45 ms] | 39.8 [39.7, 39.8] | – | same |
| explicit_tet4_ss | stable_time_step | al-head4 | 7 | 5 | 32.38 ms [32.30 ms, 32.51 ms] | 6.48 ms [6.46 ms, 6.50 ms] | 40.0 [39.9, 40.1] | 1.006 [1.003, 1.011] | same |
| explicit_tet4_ss | stable_time_step | base-t1 | 7 | 5 | 32.21 ms [32.14 ms, 32.84 ms] | 6.44 ms [6.43 ms, 6.57 ms] | 39.8 [39.7, 40.5] | 1.001 [1.000, 1.020] | same |
| explicit_tet4_ss | stable_time_step | head4-t1 | 7 | 5 | 32.56 ms [32.41 ms, 34.53 ms] | 6.51 ms [6.48 ms, 6.91 ms] | 40.2 [40.0, 42.6] | 1.009 [1.005, 1.074] | same |
| explicit_tet4_ss | solve_step | al-base | 7 | 200 | 1.292 s [1.289 s, 1.311 s] | 6.46 ms [6.45 ms, 6.56 ms] | 39.9 [39.8, 40.5] | – | same |
| explicit_tet4_ss | solve_step | al-head4 | 7 | 200 | 1.293 s [1.284 s, 1.319 s] | 6.46 ms [6.42 ms, 6.60 ms] | 39.9 [39.6, 40.7] | 0.996 [0.994, 1.010] | same |
| explicit_tet4_ss | solve_step | base-t1 | 7 | 200 | 1.295 s [1.282 s, 1.314 s] | 6.47 ms [6.41 ms, 6.57 ms] | 40.0 [39.6, 40.6] | 1.001 [0.992, 1.010] | same |
| explicit_tet4_ss | solve_step | head4-t1 | 7 | 200 | 1.301 s [1.290 s, 1.326 s] | 6.51 ms [6.45 ms, 6.63 ms] | 40.2 [39.8, 40.9] | 1.006 [0.998, 1.014] | same |
| explicit_tet4_ss | predictor | al-base | 7 | 200 | 15.3 µs [10.1 µs, 17.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | al-head4 | 7 | 200 | 13.5 µs [8.0 µs, 23.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.280 [0.528, 1.425] | same |
| explicit_tet4_ss | predictor | base-t1 | 7 | 200 | 8.1 µs [5.8 µs, 17.0 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.521 [0.421, 1.116] | same |
| explicit_tet4_ss | predictor | head4-t1 | 7 | 200 | 8.1 µs [5.8 µs, 22.6 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.613 [0.376, 1.484] | same |
| explicit_tet4_ss | residual | al-base | 7 | 200 | 1.203 s [1.199 s, 1.219 s] | 6.01 ms [6.00 ms, 6.09 ms] | 37.1 [37.0, 37.6] | – | same |
| explicit_tet4_ss | residual | al-head4 | 7 | 200 | 1.202 s [1.195 s, 1.227 s] | 6.01 ms [5.97 ms, 6.13 ms] | 37.1 [36.9, 37.9] | 0.996 [0.993, 1.011] | same |
| explicit_tet4_ss | residual | base-t1 | 7 | 200 | 1.207 s [1.195 s, 1.226 s] | 6.03 ms [5.97 ms, 6.13 ms] | 37.3 [36.9, 37.8] | 1.003 [0.995, 1.013] | same |
| explicit_tet4_ss | residual | head4-t1 | 7 | 200 | 1.204 s [1.192 s, 1.228 s] | 6.02 ms [5.96 ms, 6.14 ms] | 37.2 [36.8, 37.9] | 1.001 [0.993, 1.009] | same |
| explicit_tet4_ss | lumped_matrix | al-base | 7 | 200 | 37.8 µs [31.0 µs, 45.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | al-head4 | 7 | 200 | 35.5 µs [25.8 µs, 45.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.011 [0.763, 1.128] | same |
| explicit_tet4_ss | lumped_matrix | base-t1 | 7 | 200 | 31.3 µs [29.2 µs, 40.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.783 [0.649, 1.296] | same |
| explicit_tet4_ss | lumped_matrix | head4-t1 | 7 | 200 | 32.2 µs [28.7 µs, 38.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.789 [0.719, 1.133] | same |
| explicit_tet4_ss | corrector | al-base | 7 | 200 | 17.5 µs [12.5 µs, 28.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | al-head4 | 7 | 200 | 24.2 µs [15.4 µs, 28.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.262 [0.941, 2.233] | same |
| explicit_tet4_ss | corrector | base-t1 | 7 | 200 | 15.0 µs [9.4 µs, 19.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.832 [0.535, 1.105] | same |
| explicit_tet4_ss | corrector | head4-t1 | 7 | 200 | 17.4 µs [14.6 µs, 27.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.897 [0.671, 2.178] | same |
| explicit_tet4_ss | step_hooks | al-base | 7 | 400 | 532.1 µs [502.2 µs, 557.9 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | al-head4 | 7 | 400 | 527.5 µs [493.2 µs, 569.1 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.991 [0.944, 1.037] | same |
| explicit_tet4_ss | step_hooks | base-t1 | 7 | 400 | 534.5 µs [521.0 µs, 577.9 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.004 [0.950, 1.151] | same |
| explicit_tet4_ss | step_hooks | head4-t1 | 7 | 400 | 529.7 µs [519.8 µs, 574.8 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.996 [0.932, 1.145] | same |
| explicit_tet4_ss | step_rest | al-base | 7 | 200 | 89.54 ms [88.83 ms, 91.67 ms] | 447.7 µs [444.2 µs, 458.4 µs] | 2.8 [2.7, 2.8] | – | same |
| explicit_tet4_ss | step_rest | al-head4 | 7 | 200 | 89.78 ms [89.07 ms, 91.87 ms] | 448.9 µs [445.4 µs, 459.4 µs] | 2.8 [2.7, 2.8] | 1.002 [0.995, 1.007] | same |
| explicit_tet4_ss | step_rest | base-t1 | 7 | 200 | 86.87 ms [86.23 ms, 87.79 ms] | 434.4 µs [431.2 µs, 438.9 µs] | 2.7 [2.7, 2.7] | 0.968 [0.957, 0.974] | same |
| explicit_tet4_ss | step_rest | head4-t1 | 7 | 200 | 96.91 ms [96.29 ms, 98.15 ms] | 484.5 µs [481.4 µs, 490.8 µs] | 3.0 [3.0, 3.0] | 1.079 [1.071, 1.093] | same |
| explicit_tet4_ss | internal_forces | al-base | 7 | 40 | 227.73 ms [225.98 ms, 231.06 ms] | 5.69 ms [5.65 ms, 5.78 ms] | 35.1 [34.9, 35.7] | – | same |
| explicit_tet4_ss | internal_forces | al-head4 | 7 | 40 | 228.73 ms [226.63 ms, 230.93 ms] | 5.72 ms [5.67 ms, 5.77 ms] | 35.3 [35.0, 35.6] | 1.001 [0.990, 1.010] | same |
| explicit_tet4_ss | internal_forces | base-t1 | 7 | 40 | 228.25 ms [226.41 ms, 231.76 ms] | 5.71 ms [5.66 ms, 5.79 ms] | 35.2 [34.9, 35.8] | 1.004 [0.991, 1.016] | same |
| explicit_tet4_ss | internal_forces | head4-t1 | 7 | 40 | 230.12 ms [227.83 ms, 271.74 ms] | 5.75 ms [5.70 ms, 6.79 ms] | 35.5 [35.2, 41.9] | 1.014 [0.996, 1.188] | same |
| explicit_tet4_ss | stresses | al-base | 7 | 40 | 86.75 ms [85.70 ms, 87.37 ms] | 2.17 ms [2.14 ms, 2.18 ms] | 13.4 [13.2, 13.5] | – | same |
| explicit_tet4_ss | stresses | al-head4 | 7 | 40 | 86.52 ms [85.63 ms, 87.93 ms] | 2.16 ms [2.14 ms, 2.20 ms] | 13.4 [13.2, 13.6] | 0.999 [0.990, 1.011] | same |
| explicit_tet4_ss | stresses | base-t1 | 7 | 40 | 86.83 ms [85.87 ms, 87.51 ms] | 2.17 ms [2.15 ms, 2.19 ms] | 13.4 [13.3, 13.5] | 1.001 [0.986, 1.014] | same |
| explicit_tet4_ss | stresses | head4-t1 | 7 | 40 | 86.60 ms [85.76 ms, 87.79 ms] | 2.17 ms [2.14 ms, 2.19 ms] | 13.4 [13.2, 13.5] | 1.007 [0.984, 1.014] | same |
| explicit_tet4_ss | energies | al-base | 7 | 40 | 83.73 ms [83.11 ms, 84.42 ms] | 2.09 ms [2.08 ms, 2.11 ms] | 12.9 [12.8, 13.0] | – | same |
| explicit_tet4_ss | energies | al-head4 | 7 | 40 | 86.80 ms [86.36 ms, 88.00 ms] | 2.17 ms [2.16 ms, 2.20 ms] | 13.4 [13.3, 13.6] | 1.035 [1.028, 1.046] | same |
| explicit_tet4_ss | energies | base-t1 | 7 | 40 | 83.02 ms [82.53 ms, 83.85 ms] | 2.08 ms [2.06 ms, 2.10 ms] | 12.8 [12.7, 12.9] | 0.991 [0.978, 1.001] | same |
| explicit_tet4_ss | energies | head4-t1 | 7 | 40 | 85.30 ms [84.81 ms, 86.21 ms] | 2.13 ms [2.12 ms, 2.16 ms] | 13.2 [13.1, 13.3] | 1.019 [1.009, 1.030] | same |
| explicit_tri3_fd | mesh | al-base | 7 | 1 | 4.29 ms [4.25 ms, 4.40 ms] | 4.29 ms [4.25 ms, 4.40 ms] | 23.8 [23.6, 24.5] | – | same |
| explicit_tri3_fd | mesh | al-head4 | 7 | 1 | 4.29 ms [4.26 ms, 4.33 ms] | 4.29 ms [4.26 ms, 4.33 ms] | 23.8 [23.7, 24.1] | 1.004 [0.973, 1.012] | same |
| explicit_tri3_fd | mesh | base-t1 | 7 | 1 | 4.25 ms [4.18 ms, 4.30 ms] | 4.25 ms [4.18 ms, 4.30 ms] | 23.6 [23.2, 23.9] | 0.989 [0.966, 1.007] | same |
| explicit_tri3_fd | mesh | head4-t1 | 7 | 1 | 4.24 ms [4.20 ms, 4.32 ms] | 4.24 ms [4.20 ms, 4.32 ms] | 23.6 [23.3, 24.0] | 0.984 [0.969, 1.005] | same |
| explicit_tri3_fd | init | al-base | 7 | 1 | 41.55 ms [41.16 ms, 41.85 ms] | 41.55 ms [41.16 ms, 41.85 ms] | 230.8 [228.7, 232.5] | – | same |
| explicit_tri3_fd | init | al-head4 | 7 | 1 | 41.15 ms [40.84 ms, 41.60 ms] | 41.15 ms [40.84 ms, 41.60 ms] | 228.6 [226.9, 231.1] | 0.997 [0.978, 1.008] | same |
| explicit_tri3_fd | init | base-t1 | 7 | 1 | 41.61 ms [41.20 ms, 42.41 ms] | 41.61 ms [41.20 ms, 42.41 ms] | 231.2 [228.9, 235.6] | 1.003 [0.984, 1.021] | same |
| explicit_tri3_fd | init | head4-t1 | 7 | 1 | 41.39 ms [40.91 ms, 41.77 ms] | 41.39 ms [40.91 ms, 41.77 ms] | 230.0 [227.3, 232.1] | 0.997 [0.987, 1.011] | same |
| explicit_tri3_fd | stable_time_step | al-base | 7 | 5 | 22.66 ms [22.58 ms, 22.73 ms] | 4.53 ms [4.52 ms, 4.55 ms] | 25.2 [25.1, 25.3] | – | same |
| explicit_tri3_fd | stable_time_step | al-head4 | 7 | 5 | 22.59 ms [22.52 ms, 22.86 ms] | 4.52 ms [4.50 ms, 4.57 ms] | 25.1 [25.0, 25.4] | 0.997 [0.992, 1.010] | same |
| explicit_tri3_fd | stable_time_step | base-t1 | 7 | 5 | 22.63 ms [22.61 ms, 22.71 ms] | 4.53 ms [4.52 ms, 4.54 ms] | 25.1 [25.1, 25.2] | 0.999 [0.996, 1.002] | same |
| explicit_tri3_fd | stable_time_step | head4-t1 | 7 | 5 | 22.77 ms [22.67 ms, 23.26 ms] | 4.55 ms [4.53 ms, 4.65 ms] | 25.3 [25.2, 25.8] | 1.003 [0.998, 1.024] | same |
| explicit_tri3_fd | solve_step | al-base | 7 | 200 | 1.460 s [1.444 s, 1.470 s] | 7.30 ms [7.22 ms, 7.35 ms] | 40.6 [40.1, 40.8] | – | same |
| explicit_tri3_fd | solve_step | al-head4 | 7 | 200 | 1.458 s [1.448 s, 1.465 s] | 7.29 ms [7.24 ms, 7.32 ms] | 40.5 [40.2, 40.7] | 0.999 [0.995, 1.002] | same |
| explicit_tri3_fd | solve_step | base-t1 | 7 | 200 | 1.457 s [1.453 s, 1.476 s] | 7.28 ms [7.26 ms, 7.38 ms] | 40.5 [40.4, 41.0] | 0.997 [0.995, 1.012] | same |
| explicit_tri3_fd | solve_step | head4-t1 | 7 | 200 | 1.482 s [1.473 s, 1.489 s] | 7.41 ms [7.36 ms, 7.44 ms] | 41.2 [40.9, 41.4] | 1.016 [1.007, 1.030] | same |
| explicit_tri3_fd | predictor | al-base | 7 | 200 | 15.5 µs [12.3 µs, 21.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | predictor | al-head4 | 7 | 200 | 14.3 µs [9.2 µs, 19.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.009 [0.441, 1.298] | same |
| explicit_tri3_fd | predictor | base-t1 | 7 | 200 | 16.3 µs [12.2 µs, 23.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.944 [0.748, 1.932] | same |
| explicit_tri3_fd | predictor | head4-t1 | 7 | 200 | 13.9 µs [7.3 µs, 19.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.833 [0.511, 1.227] | same |
| explicit_tri3_fd | residual | al-base | 7 | 200 | 1.278 s [1.265 s, 1.286 s] | 6.39 ms [6.33 ms, 6.43 ms] | 35.5 [35.1, 35.7] | – | same |
| explicit_tri3_fd | residual | al-head4 | 7 | 200 | 1.276 s [1.268 s, 1.281 s] | 6.38 ms [6.34 ms, 6.41 ms] | 35.4 [35.2, 35.6] | 1.000 [0.996, 1.002] | same |
| explicit_tri3_fd | residual | base-t1 | 7 | 200 | 1.280 s [1.277 s, 1.289 s] | 6.40 ms [6.38 ms, 6.44 ms] | 35.6 [35.5, 35.8] | 1.002 [0.999, 1.016] | same |
| explicit_tri3_fd | residual | head4-t1 | 7 | 200 | 1.285 s [1.277 s, 1.290 s] | 6.42 ms [6.39 ms, 6.45 ms] | 35.7 [35.5, 35.8] | 1.005 [0.998, 1.020] | same |
| explicit_tri3_fd | lumped_matrix | al-base | 7 | 200 | 40.9 µs [38.7 µs, 47.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | lumped_matrix | al-head4 | 7 | 200 | 43.0 µs [34.4 µs, 53.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.080 [0.841, 1.127] | same |
| explicit_tri3_fd | lumped_matrix | base-t1 | 7 | 200 | 35.8 µs [26.1 µs, 42.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.799 [0.674, 1.043] | same |
| explicit_tri3_fd | lumped_matrix | head4-t1 | 7 | 200 | 42.8 µs [34.5 µs, 47.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.012 [0.796, 1.143] | same |
| explicit_tri3_fd | corrector | al-base | 7 | 200 | 19.0 µs [13.5 µs, 24.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | corrector | al-head4 | 7 | 200 | 23.2 µs [15.5 µs, 25.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.094 [0.725, 1.720] | same |
| explicit_tri3_fd | corrector | base-t1 | 7 | 200 | 18.9 µs [12.2 µs, 28.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.922 [0.594, 1.471] | same |
| explicit_tri3_fd | corrector | head4-t1 | 7 | 200 | 17.4 µs [12.9 µs, 33.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.126 [0.656, 1.770] | same |
| explicit_tri3_fd | step_hooks | al-base | 7 | 400 | 650.5 µs [610.3 µs, 694.5 µs] | 1.6 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | step_hooks | al-head4 | 7 | 400 | 648.6 µs [598.8 µs, 702.4 µs] | 1.6 µs [1.5 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 0.998 [0.921, 1.074] | same |
| explicit_tri3_fd | step_hooks | base-t1 | 7 | 400 | 666.1 µs [641.3 µs, 756.0 µs] | 1.7 µs [1.6 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.000 [0.923, 1.193] | same |
| explicit_tri3_fd | step_hooks | head4-t1 | 7 | 400 | 677.5 µs [642.0 µs, 712.7 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.042 [0.952, 1.118] | same |
| explicit_tri3_fd | step_rest | al-base | 7 | 200 | 181.62 ms [178.37 ms, 183.04 ms] | 908.1 µs [891.9 µs, 915.2 µs] | 5.0 [5.0, 5.1] | – | same |
| explicit_tri3_fd | step_rest | al-head4 | 7 | 200 | 181.38 ms [179.05 ms, 182.40 ms] | 906.9 µs [895.2 µs, 912.0 µs] | 5.0 [5.0, 5.1] | 0.999 [0.988, 1.004] | same |
| explicit_tri3_fd | step_rest | base-t1 | 7 | 200 | 175.61 ms [175.13 ms, 185.89 ms] | 878.1 µs [875.6 µs, 929.5 µs] | 4.9 [4.9, 5.2] | 0.969 [0.964, 1.016] | same |
| explicit_tri3_fd | step_rest | head4-t1 | 7 | 200 | 196.69 ms [194.77 ms, 203.52 ms] | 983.5 µs [973.8 µs, 1.02 ms] | 5.5 [5.4, 5.7] | 1.081 [1.072, 1.119] | same |
| explicit_tri3_fd | internal_forces | al-base | 7 | 40 | 233.41 ms [231.34 ms, 234.58 ms] | 5.84 ms [5.78 ms, 5.86 ms] | 32.4 [32.1, 32.6] | – | same |
| explicit_tri3_fd | internal_forces | al-head4 | 7 | 40 | 233.08 ms [231.49 ms, 233.82 ms] | 5.83 ms [5.79 ms, 5.85 ms] | 32.4 [32.2, 32.5] | 0.998 [0.993, 1.001] | same |
| explicit_tri3_fd | internal_forces | base-t1 | 7 | 40 | 234.37 ms [233.71 ms, 234.96 ms] | 5.86 ms [5.84 ms, 5.87 ms] | 32.6 [32.5, 32.6] | 1.003 [0.999, 1.015] | same |
| explicit_tri3_fd | internal_forces | head4-t1 | 7 | 40 | 234.30 ms [232.74 ms, 235.21 ms] | 5.86 ms [5.82 ms, 5.88 ms] | 32.5 [32.3, 32.7] | 1.002 [0.994, 1.017] | same |
| explicit_tri3_fd | stresses | al-base | 7 | 40 | 56.00 ms [55.00 ms, 56.44 ms] | 1.40 ms [1.37 ms, 1.41 ms] | 7.8 [7.6, 7.8] | – | same |
| explicit_tri3_fd | stresses | al-head4 | 7 | 40 | 56.20 ms [55.27 ms, 57.10 ms] | 1.40 ms [1.38 ms, 1.43 ms] | 7.8 [7.7, 7.9] | 1.004 [0.995, 1.014] | same |
| explicit_tri3_fd | stresses | base-t1 | 7 | 40 | 56.45 ms [55.92 ms, 56.97 ms] | 1.41 ms [1.40 ms, 1.42 ms] | 7.8 [7.8, 7.9] | 1.009 [0.998, 1.036] | same |
| explicit_tri3_fd | stresses | head4-t1 | 7 | 40 | 56.71 ms [55.63 ms, 56.96 ms] | 1.42 ms [1.39 ms, 1.42 ms] | 7.9 [7.7, 7.9] | 1.008 [0.992, 1.032] | same |
| explicit_tri3_fd | energies | al-base | 7 | 40 | 96.29 ms [95.57 ms, 96.71 ms] | 2.41 ms [2.39 ms, 2.42 ms] | 13.4 [13.3, 13.4] | – | same |
| explicit_tri3_fd | energies | al-head4 | 7 | 40 | 99.39 ms [99.24 ms, 100.24 ms] | 2.48 ms [2.48 ms, 2.51 ms] | 13.8 [13.8, 13.9] | 1.034 [1.029, 1.040] | same |
| explicit_tri3_fd | energies | base-t1 | 7 | 40 | 96.37 ms [96.16 ms, 96.96 ms] | 2.41 ms [2.40 ms, 2.42 ms] | 13.4 [13.4, 13.5] | 1.001 [0.996, 1.014] | same |
| explicit_tri3_fd | energies | head4-t1 | 7 | 40 | 99.27 ms [98.85 ms, 99.59 ms] | 2.48 ms [2.47 ms, 2.49 ms] | 13.8 [13.7, 13.8] | 1.031 [1.023, 1.042] | same |
| explicit_tri3_ss | mesh | al-base | 7 | 1 | 4.28 ms [4.12 ms, 4.31 ms] | 4.28 ms [4.12 ms, 4.31 ms] | 23.8 [22.9, 24.0] | – | same |
| explicit_tri3_ss | mesh | al-head4 | 7 | 1 | 4.25 ms [4.14 ms, 4.32 ms] | 4.25 ms [4.14 ms, 4.32 ms] | 23.6 [23.0, 24.0] | 0.997 [0.960, 1.034] | same |
| explicit_tri3_ss | mesh | base-t1 | 7 | 1 | 4.18 ms [4.02 ms, 4.26 ms] | 4.18 ms [4.02 ms, 4.26 ms] | 23.2 [22.3, 23.7] | 0.983 [0.966, 0.999] | same |
| explicit_tri3_ss | mesh | head4-t1 | 7 | 1 | 4.22 ms [4.07 ms, 4.29 ms] | 4.22 ms [4.07 ms, 4.29 ms] | 23.5 [22.6, 23.9] | 0.989 [0.979, 1.016] | same |
| explicit_tri3_ss | init | al-base | 7 | 1 | 38.22 ms [38.01 ms, 39.16 ms] | 38.22 ms [38.01 ms, 39.16 ms] | 212.3 [211.1, 217.5] | – | same |
| explicit_tri3_ss | init | al-head4 | 7 | 1 | 38.21 ms [38.09 ms, 38.99 ms] | 38.21 ms [38.09 ms, 38.99 ms] | 212.3 [211.6, 216.6] | 1.003 [0.976, 1.024] | same |
| explicit_tri3_ss | init | base-t1 | 7 | 1 | 38.69 ms [38.17 ms, 39.27 ms] | 38.69 ms [38.17 ms, 39.27 ms] | 214.9 [212.0, 218.2] | 1.009 [0.987, 1.027] | same |
| explicit_tri3_ss | init | head4-t1 | 7 | 1 | 38.03 ms [37.56 ms, 38.47 ms] | 38.03 ms [37.56 ms, 38.47 ms] | 211.3 [208.7, 213.7] | 0.991 [0.971, 1.011] | same |
| explicit_tri3_ss | stable_time_step | al-base | 7 | 5 | 22.68 ms [22.57 ms, 23.39 ms] | 4.54 ms [4.51 ms, 4.68 ms] | 25.2 [25.1, 26.0] | – | same |
| explicit_tri3_ss | stable_time_step | al-head4 | 7 | 5 | 22.69 ms [22.54 ms, 22.90 ms] | 4.54 ms [4.51 ms, 4.58 ms] | 25.2 [25.0, 25.4] | 0.997 [0.977, 1.002] | same |
| explicit_tri3_ss | stable_time_step | base-t1 | 7 | 5 | 22.69 ms [22.67 ms, 23.47 ms] | 4.54 ms [4.53 ms, 4.69 ms] | 25.2 [25.2, 26.1] | 1.002 [0.970, 1.037] | same |
| explicit_tri3_ss | stable_time_step | head4-t1 | 7 | 5 | 22.70 ms [22.60 ms, 23.43 ms] | 4.54 ms [4.52 ms, 4.69 ms] | 25.2 [25.1, 26.0] | 0.999 [0.970, 1.028] | same |
| explicit_tri3_ss | solve_step | al-base | 7 | 300 | 1.539 s [1.528 s, 1.550 s] | 5.13 ms [5.09 ms, 5.17 ms] | 28.5 [28.3, 28.7] | – | same |
| explicit_tri3_ss | solve_step | al-head4 | 7 | 300 | 1.542 s [1.529 s, 1.914 s] | 5.14 ms [5.10 ms, 6.38 ms] | 28.6 [28.3, 35.4] | 1.007 [0.992, 1.244] | same |
| explicit_tri3_ss | solve_step | base-t1 | 7 | 300 | 1.532 s [1.521 s, 1.542 s] | 5.11 ms [5.07 ms, 5.14 ms] | 28.4 [28.2, 28.6] | 0.993 [0.988, 1.007] | same |
| explicit_tri3_ss | solve_step | head4-t1 | 7 | 300 | 1.563 s [1.558 s, 1.569 s] | 5.21 ms [5.19 ms, 5.23 ms] | 28.9 [28.9, 29.0] | 1.016 [1.009, 1.025] | same |
| explicit_tri3_ss | predictor | al-base | 7 | 300 | 25.4 µs [18.0 µs, 33.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | al-head4 | 7 | 300 | 24.3 µs [20.0 µs, 28.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.922 [0.656, 1.134] | same |
| explicit_tri3_ss | predictor | base-t1 | 7 | 300 | 19.5 µs [13.6 µs, 37.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.811 [0.419, 1.707] | same |
| explicit_tri3_ss | predictor | head4-t1 | 7 | 300 | 18.4 µs [13.8 µs, 30.7 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.813 [0.572, 1.208] | same |
| explicit_tri3_ss | residual | al-base | 7 | 300 | 1.265 s [1.258 s, 1.273 s] | 4.22 ms [4.19 ms, 4.24 ms] | 23.4 [23.3, 23.6] | – | same |
| explicit_tri3_ss | residual | al-head4 | 7 | 300 | 1.267 s [1.257 s, 1.641 s] | 4.22 ms [4.19 ms, 5.47 ms] | 23.5 [23.3, 30.4] | 1.004 [0.993, 1.299] | same |
| explicit_tri3_ss | residual | base-t1 | 7 | 300 | 1.267 s [1.259 s, 1.275 s] | 4.22 ms [4.20 ms, 4.25 ms] | 23.5 [23.3, 23.6] | 1.001 [0.994, 1.013] | same |
| explicit_tri3_ss | residual | head4-t1 | 7 | 300 | 1.269 s [1.265 s, 1.274 s] | 4.23 ms [4.22 ms, 4.25 ms] | 23.5 [23.4, 23.6] | 1.003 [0.997, 1.012] | same |
| explicit_tri3_ss | lumped_matrix | al-base | 7 | 300 | 60.8 µs [40.8 µs, 71.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | al-head4 | 7 | 300 | 64.5 µs [59.2 µs, 78.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.123 [0.906, 1.453] | same |
| explicit_tri3_ss | lumped_matrix | base-t1 | 7 | 300 | 59.0 µs [52.9 µs, 68.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.971 [0.868, 1.324] | same |
| explicit_tri3_ss | lumped_matrix | head4-t1 | 7 | 300 | 59.5 µs [47.9 µs, 80.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.102 [0.917, 1.175] | same |
| explicit_tri3_ss | corrector | al-base | 7 | 300 | 26.1 µs [20.9 µs, 40.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | al-head4 | 7 | 300 | 30.9 µs [25.8 µs, 38.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.189 [0.938, 1.254] | same |
| explicit_tri3_ss | corrector | base-t1 | 7 | 300 | 22.5 µs [15.8 µs, 34.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.734 [0.486, 1.505] | same |
| explicit_tri3_ss | corrector | head4-t1 | 7 | 300 | 29.7 µs [26.6 µs, 35.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.270 [0.697, 1.587] | same |
| explicit_tri3_ss | step_hooks | al-base | 7 | 600 | 860.8 µs [843.1 µs, 911.3 µs] | 1.4 µs [1.4 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | al-head4 | 7 | 600 | 828.5 µs [747.7 µs, 892.0 µs] | 1.4 µs [1.2 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 0.945 [0.882, 1.058] | same |
| explicit_tri3_ss | step_hooks | base-t1 | 7 | 600 | 845.9 µs [791.5 µs, 875.9 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 0.981 [0.869, 1.019] | same |
| explicit_tri3_ss | step_hooks | head4-t1 | 7 | 600 | 820.9 µs [790.9 µs, 941.7 µs] | 1.4 µs [1.3 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 0.952 [0.895, 1.033] | same |
| explicit_tri3_ss | step_rest | al-base | 7 | 300 | 272.91 ms [269.44 ms, 275.94 ms] | 909.7 µs [898.1 µs, 919.8 µs] | 5.1 [5.0, 5.1] | – | same |
| explicit_tri3_ss | step_rest | al-head4 | 7 | 300 | 272.06 ms [270.42 ms, 276.73 ms] | 906.9 µs [901.4 µs, 922.4 µs] | 5.0 [5.0, 5.1] | 0.996 [0.990, 1.021] | same |
| explicit_tri3_ss | step_rest | base-t1 | 7 | 300 | 263.43 ms [261.60 ms, 265.71 ms] | 878.1 µs [872.0 µs, 885.7 µs] | 4.9 [4.8, 4.9] | 0.965 [0.957, 0.980] | same |
| explicit_tri3_ss | step_rest | head4-t1 | 7 | 300 | 292.60 ms [291.36 ms, 294.06 ms] | 975.3 µs [971.2 µs, 980.2 µs] | 5.4 [5.4, 5.4] | 1.071 [1.066, 1.086] | same |
| explicit_tri3_ss | internal_forces | al-base | 7 | 60 | 213.52 ms [212.26 ms, 214.15 ms] | 3.56 ms [3.54 ms, 3.57 ms] | 19.8 [19.7, 19.8] | – | same |
| explicit_tri3_ss | internal_forces | al-head4 | 7 | 60 | 212.98 ms [211.67 ms, 215.09 ms] | 3.55 ms [3.53 ms, 3.58 ms] | 19.7 [19.6, 19.9] | 1.000 [0.991, 1.013] | same |
| explicit_tri3_ss | internal_forces | base-t1 | 7 | 60 | 213.36 ms [212.62 ms, 214.27 ms] | 3.56 ms [3.54 ms, 3.57 ms] | 19.8 [19.7, 19.8] | 0.999 [0.996, 1.009] | same |
| explicit_tri3_ss | internal_forces | head4-t1 | 7 | 60 | 212.64 ms [211.38 ms, 214.35 ms] | 3.54 ms [3.52 ms, 3.57 ms] | 19.7 [19.6, 19.8] | 0.997 [0.992, 1.002] | same |
| explicit_tri3_ss | stresses | al-base | 7 | 60 | 74.30 ms [73.89 ms, 75.60 ms] | 1.24 ms [1.23 ms, 1.26 ms] | 6.9 [6.8, 7.0] | – | same |
| explicit_tri3_ss | stresses | al-head4 | 7 | 60 | 74.66 ms [73.51 ms, 74.82 ms] | 1.24 ms [1.23 ms, 1.25 ms] | 6.9 [6.8, 6.9] | 1.003 [0.989, 1.010] | same |
| explicit_tri3_ss | stresses | base-t1 | 7 | 60 | 74.39 ms [74.17 ms, 74.87 ms] | 1.24 ms [1.24 ms, 1.25 ms] | 6.9 [6.9, 6.9] | 1.002 [0.981, 1.013] | same |
| explicit_tri3_ss | stresses | head4-t1 | 7 | 60 | 74.57 ms [72.99 ms, 74.84 ms] | 1.24 ms [1.22 ms, 1.25 ms] | 6.9 [6.8, 6.9] | 1.002 [0.983, 1.008] | same |
| explicit_tri3_ss | energies | al-base | 7 | 60 | 136.18 ms [135.48 ms, 137.54 ms] | 2.27 ms [2.26 ms, 2.29 ms] | 12.6 [12.5, 12.7] | – | same |
| explicit_tri3_ss | energies | al-head4 | 7 | 60 | 140.80 ms [139.99 ms, 142.72 ms] | 2.35 ms [2.33 ms, 2.38 ms] | 13.0 [13.0, 13.2] | 1.039 [1.019, 1.051] | same |
| explicit_tri3_ss | energies | base-t1 | 7 | 60 | 138.13 ms [137.27 ms, 138.72 ms] | 2.30 ms [2.29 ms, 2.31 ms] | 12.8 [12.7, 12.8] | 1.017 [1.001, 1.024] | same |
| explicit_tri3_ss | energies | head4-t1 | 7 | 60 | 140.91 ms [140.08 ms, 141.63 ms] | 2.35 ms [2.33 ms, 2.36 ms] | 13.0 [13.0, 13.1] | 1.036 [1.025, 1.040] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 017c8c9ef7659aa0 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | 7a6ad8f42, 85d4d74d4 | a12864e94a9122c8 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42, 85d4d74d4 | fecb3fb4b46c8b8c |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 7a6ad8f42, 85d4d74d4 | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7a6ad8f42, 85d4d74d4 | b6b902f5ca71049e |
