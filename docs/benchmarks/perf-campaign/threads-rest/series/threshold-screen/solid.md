Suite `solid`, size `m`; variants: `eq16384`, `eq262144`, `eq4096`, `eq65536`; reference `eq16384`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to eq16384 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_hex8_ss | mesh | eq16384 | 5 | 1 | 6.44 ms [6.30 ms, 6.46 ms] | 6.44 ms [6.30 ms, 6.46 ms] | 58.2 [57.0, 58.4] | – | same |
| explicit_hex8_ss | mesh | eq262144 | 5 | 1 | 6.35 ms [6.09 ms, 6.51 ms] | 6.35 ms [6.09 ms, 6.51 ms] | 57.4 [55.1, 58.9] | 0.986 [0.965, 1.027] | same |
| explicit_hex8_ss | mesh | eq4096 | 5 | 1 | 6.43 ms [6.31 ms, 6.50 ms] | 6.43 ms [6.31 ms, 6.50 ms] | 58.1 [57.0, 58.8] | 1.003 [0.978, 1.026] | same |
| explicit_hex8_ss | mesh | eq65536 | 5 | 1 | 6.35 ms [6.16 ms, 6.75 ms] | 6.35 ms [6.16 ms, 6.75 ms] | 57.4 [55.7, 61.1] | 0.993 [0.957, 1.046] | same |
| explicit_hex8_ss | init | eq16384 | 5 | 1 | 260.23 ms [257.02 ms, 263.16 ms] | 260.23 ms [257.02 ms, 263.16 ms] | 2353.1 [2324.1, 2379.5] | – | same |
| explicit_hex8_ss | init | eq262144 | 5 | 1 | 262.46 ms [258.27 ms, 263.81 ms] | 262.46 ms [258.27 ms, 263.81 ms] | 2373.2 [2335.3, 2385.4] | 1.011 [0.992, 1.026] | same |
| explicit_hex8_ss | init | eq4096 | 5 | 1 | 260.49 ms [257.25 ms, 267.78 ms] | 260.49 ms [257.25 ms, 267.78 ms] | 2355.4 [2326.1, 2421.3] | 1.001 [0.990, 1.037] | same |
| explicit_hex8_ss | init | eq65536 | 5 | 1 | 262.06 ms [257.73 ms, 263.74 ms] | 262.06 ms [257.73 ms, 263.74 ms] | 2369.6 [2330.5, 2384.8] | 1.002 [0.990, 1.021] | same |
| explicit_hex8_ss | stable_time_step | eq16384 | 5 | 5 | 23.91 ms [23.86 ms, 27.43 ms] | 4.78 ms [4.77 ms, 5.49 ms] | 43.2 [43.1, 49.6] | – | same |
| explicit_hex8_ss | stable_time_step | eq262144 | 5 | 5 | 23.89 ms [23.80 ms, 25.55 ms] | 4.78 ms [4.76 ms, 5.11 ms] | 43.2 [43.0, 46.2] | 0.998 [0.931, 1.055] | same |
| explicit_hex8_ss | stable_time_step | eq4096 | 5 | 5 | 23.99 ms [23.81 ms, 24.03 ms] | 4.80 ms [4.76 ms, 4.81 ms] | 43.4 [43.1, 43.5] | 0.998 [0.876, 1.005] | same |
| explicit_hex8_ss | stable_time_step | eq65536 | 5 | 5 | 23.92 ms [23.88 ms, 26.30 ms] | 4.78 ms [4.78 ms, 5.26 ms] | 43.3 [43.2, 47.6] | 1.000 [0.872, 1.101] | same |
| explicit_hex8_ss | solve_step | eq16384 | 5 | 50 | 532.27 ms [530.27 ms, 543.31 ms] | 10.65 ms [10.61 ms, 10.87 ms] | 96.3 [95.9, 98.3] | – | same |
| explicit_hex8_ss | solve_step | eq262144 | 5 | 50 | 585.24 ms [580.43 ms, 598.37 ms] | 11.70 ms [11.61 ms, 11.97 ms] | 105.8 [105.0, 108.2] | 1.100 [1.070, 1.114] | same |
| explicit_hex8_ss | solve_step | eq4096 | 5 | 50 | 536.09 ms [530.34 ms, 540.94 ms] | 10.72 ms [10.61 ms, 10.82 ms] | 96.9 [95.9, 97.8] | 1.000 [0.987, 1.020] | same |
| explicit_hex8_ss | solve_step | eq65536 | 5 | 50 | 538.32 ms [534.79 ms, 539.05 ms] | 10.77 ms [10.70 ms, 10.78 ms] | 97.4 [96.7, 97.5] | 1.009 [0.987, 1.013] | same |
| explicit_hex8_ss | predictor | eq16384 | 5 | 50 | 10.0 µs [8.3 µs, 11.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | predictor | eq262144 | 5 | 50 | 10.4 µs [8.2 µs, 12.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.121 [0.698, 1.358] | same |
| explicit_hex8_ss | predictor | eq4096 | 5 | 50 | 8.9 µs [8.5 µs, 10.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.912 [0.788, 1.073] | same |
| explicit_hex8_ss | predictor | eq65536 | 5 | 50 | 9.1 µs [8.0 µs, 9.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.861 [0.779, 1.125] | same |
| explicit_hex8_ss | residual | eq16384 | 5 | 50 | 471.53 ms [469.52 ms, 482.47 ms] | 9.43 ms [9.39 ms, 9.65 ms] | 85.3 [84.9, 87.3] | – | same |
| explicit_hex8_ss | residual | eq262144 | 5 | 50 | 486.99 ms [482.19 ms, 500.21 ms] | 9.74 ms [9.64 ms, 10.00 ms] | 88.1 [87.2, 90.5] | 1.033 [0.999, 1.050] | same |
| explicit_hex8_ss | residual | eq4096 | 5 | 50 | 475.67 ms [469.61 ms, 478.00 ms] | 9.51 ms [9.39 ms, 9.56 ms] | 86.0 [84.9, 86.4] | 1.000 [0.986, 1.018] | same |
| explicit_hex8_ss | residual | eq65536 | 5 | 50 | 474.91 ms [471.68 ms, 476.59 ms] | 9.50 ms [9.43 ms, 9.53 ms] | 85.9 [85.3, 86.2] | 1.005 [0.981, 1.011] | same |
| explicit_hex8_ss | lumped_matrix | eq16384 | 5 | 50 | 20.5 µs [17.1 µs, 20.8 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | lumped_matrix | eq262144 | 5 | 50 | 21.0 µs [18.5 µs, 26.0 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.101 [0.898, 1.253] | same |
| explicit_hex8_ss | lumped_matrix | eq4096 | 5 | 50 | 23.1 µs [17.2 µs, 24.7 µs] | 0.5 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.126 [1.005, 1.352] | same |
| explicit_hex8_ss | lumped_matrix | eq65536 | 5 | 50 | 21.3 µs [19.6 µs, 25.5 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.073 [0.972, 1.276] | same |
| explicit_hex8_ss | corrector | eq16384 | 5 | 50 | 13.5 µs [13.2 µs, 13.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | corrector | eq262144 | 5 | 50 | 13.5 µs [13.0 µs, 13.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.992 [0.979, 0.997] | same |
| explicit_hex8_ss | corrector | eq4096 | 5 | 50 | 13.1 µs [12.9 µs, 28.4 µs] | 0.3 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 0.969 [0.956, 2.153] | same |
| explicit_hex8_ss | corrector | eq65536 | 5 | 50 | 13.1 µs [12.8 µs, 14.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.965 [0.933, 1.080] | same |
| explicit_hex8_ss | step_hooks | eq16384 | 5 | 100 | 164.1 µs [161.2 µs, 175.7 µs] | 1.6 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | step_hooks | eq262144 | 5 | 100 | 172.3 µs [168.0 µs, 182.1 µs] | 1.7 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.037 [0.968, 1.130] | same |
| explicit_hex8_ss | step_hooks | eq4096 | 5 | 100 | 161.0 µs [157.2 µs, 164.2 µs] | 1.6 µs [1.6 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 0.970 [0.931, 0.999] | same |
| explicit_hex8_ss | step_hooks | eq65536 | 5 | 100 | 168.7 µs [157.0 µs, 180.7 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 0.990 [0.960, 1.085] | same |
| explicit_hex8_ss | step_rest | eq16384 | 5 | 50 | 60.51 ms [60.34 ms, 60.63 ms] | 1.21 ms [1.21 ms, 1.21 ms] | 10.9 [10.9, 11.0] | – | same |
| explicit_hex8_ss | step_rest | eq262144 | 5 | 50 | 98.02 ms [97.92 ms, 98.68 ms] | 1.96 ms [1.96 ms, 1.97 ms] | 17.7 [17.7, 17.8] | 1.620 [1.618, 1.628] | same |
| explicit_hex8_ss | step_rest | eq4096 | 5 | 50 | 60.61 ms [60.21 ms, 62.71 ms] | 1.21 ms [1.20 ms, 1.25 ms] | 11.0 [10.9, 11.3] | 1.002 [0.993, 1.036] | same |
| explicit_hex8_ss | step_rest | eq65536 | 5 | 50 | 62.66 ms [62.24 ms, 63.34 ms] | 1.25 ms [1.24 ms, 1.27 ms] | 11.3 [11.3, 11.5] | 1.034 [1.028, 1.050] | same |
| explicit_hex8_ss | internal_forces | eq16384 | 5 | 10 | 89.50 ms [88.75 ms, 91.11 ms] | 8.95 ms [8.88 ms, 9.11 ms] | 80.9 [80.3, 82.4] | – | same |
| explicit_hex8_ss | internal_forces | eq262144 | 5 | 10 | 84.75 ms [83.40 ms, 88.88 ms] | 8.47 ms [8.34 ms, 8.89 ms] | 76.6 [75.4, 80.4] | 0.935 [0.924, 0.996] | same |
| explicit_hex8_ss | internal_forces | eq4096 | 5 | 10 | 90.01 ms [89.76 ms, 91.03 ms] | 9.00 ms [8.98 ms, 9.10 ms] | 81.4 [81.2, 82.3] | 1.007 [0.985, 1.017] | same |
| explicit_hex8_ss | internal_forces | eq65536 | 5 | 10 | 89.93 ms [89.34 ms, 90.22 ms] | 8.99 ms [8.93 ms, 9.02 ms] | 81.3 [80.8, 81.6] | 1.005 [0.984, 1.011] | same |
| explicit_hex8_ss | stresses | eq16384 | 5 | 10 | 100.33 ms [99.22 ms, 107.08 ms] | 10.03 ms [9.92 ms, 10.71 ms] | 90.7 [89.7, 96.8] | – | same |
| explicit_hex8_ss | stresses | eq262144 | 5 | 10 | 99.15 ms [98.85 ms, 100.11 ms] | 9.92 ms [9.89 ms, 10.01 ms] | 89.7 [89.4, 90.5] | 0.992 [0.926, 1.009] | same |
| explicit_hex8_ss | stresses | eq4096 | 5 | 10 | 105.37 ms [99.08 ms, 107.51 ms] | 10.54 ms [9.91 ms, 10.75 ms] | 95.3 [89.6, 97.2] | 1.030 [0.999, 1.081] | same |
| explicit_hex8_ss | stresses | eq65536 | 5 | 10 | 100.39 ms [99.61 ms, 105.36 ms] | 10.04 ms [9.96 ms, 10.54 ms] | 90.8 [90.1, 95.3] | 1.001 [0.930, 1.062] | same |
| explicit_hex8_ss | energies | eq16384 | 5 | 10 | 49.08 ms [48.54 ms, 49.74 ms] | 4.91 ms [4.85 ms, 4.97 ms] | 44.4 [43.9, 45.0] | – | same |
| explicit_hex8_ss | energies | eq262144 | 5 | 10 | 47.88 ms [47.57 ms, 48.16 ms] | 4.79 ms [4.76 ms, 4.82 ms] | 43.3 [43.0, 43.5] | 0.971 [0.965, 0.992] | same |
| explicit_hex8_ss | energies | eq4096 | 5 | 10 | 49.15 ms [49.03 ms, 49.65 ms] | 4.92 ms [4.90 ms, 4.96 ms] | 44.4 [44.3, 44.9] | 1.003 [0.997, 1.010] | same |
| explicit_hex8_ss | energies | eq65536 | 5 | 10 | 48.95 ms [48.38 ms, 49.14 ms] | 4.90 ms [4.84 ms, 4.91 ms] | 44.3 [43.7, 44.4] | 0.989 [0.984, 1.011] | same |
| explicit_tet4_ss | mesh | eq16384 | 5 | 1 | 3.13 ms [2.99 ms, 3.26 ms] | 3.13 ms [2.99 ms, 3.26 ms] | 19.3 [18.4, 20.1] | – | same |
| explicit_tet4_ss | mesh | eq262144 | 5 | 1 | 3.09 ms [3.02 ms, 3.21 ms] | 3.09 ms [3.02 ms, 3.21 ms] | 19.1 [18.6, 19.8] | 0.986 [0.983, 1.068] | same |
| explicit_tet4_ss | mesh | eq4096 | 5 | 1 | 3.09 ms [3.06 ms, 3.29 ms] | 3.09 ms [3.06 ms, 3.29 ms] | 19.1 [18.9, 20.3] | 0.998 [0.969, 1.052] | same |
| explicit_tet4_ss | mesh | eq65536 | 5 | 1 | 3.14 ms [3.10 ms, 3.31 ms] | 3.14 ms [3.10 ms, 3.31 ms] | 19.4 [19.1, 20.5] | 1.009 [0.998, 1.057] | same |
| explicit_tet4_ss | init | eq16384 | 5 | 1 | 46.58 ms [45.85 ms, 46.87 ms] | 46.58 ms [45.85 ms, 46.87 ms] | 287.6 [283.0, 289.3] | – | same |
| explicit_tet4_ss | init | eq262144 | 5 | 1 | 46.60 ms [45.88 ms, 49.86 ms] | 46.60 ms [45.88 ms, 49.86 ms] | 287.7 [283.2, 307.8] | 1.016 [0.979, 1.066] | same |
| explicit_tet4_ss | init | eq4096 | 5 | 1 | 46.68 ms [45.45 ms, 48.48 ms] | 46.68 ms [45.45 ms, 48.48 ms] | 288.2 [280.6, 299.2] | 1.006 [0.970, 1.041] | same |
| explicit_tet4_ss | init | eq65536 | 5 | 1 | 47.17 ms [46.05 ms, 48.38 ms] | 47.17 ms [46.05 ms, 48.38 ms] | 291.1 [284.3, 298.6] | 1.012 [0.989, 1.034] | same |
| explicit_tet4_ss | stable_time_step | eq16384 | 5 | 5 | 32.68 ms [32.55 ms, 32.93 ms] | 6.54 ms [6.51 ms, 6.59 ms] | 40.3 [40.2, 40.7] | – | same |
| explicit_tet4_ss | stable_time_step | eq262144 | 5 | 5 | 32.87 ms [32.58 ms, 33.37 ms] | 6.57 ms [6.52 ms, 6.67 ms] | 40.6 [40.2, 41.2] | 1.006 [0.989, 1.023] | same |
| explicit_tet4_ss | stable_time_step | eq4096 | 5 | 5 | 32.87 ms [32.65 ms, 33.49 ms] | 6.57 ms [6.53 ms, 6.70 ms] | 40.6 [40.3, 41.3] | 1.001 [0.996, 1.029] | same |
| explicit_tet4_ss | stable_time_step | eq65536 | 5 | 5 | 32.84 ms [32.68 ms, 33.34 ms] | 6.57 ms [6.54 ms, 6.67 ms] | 40.5 [40.3, 41.2] | 1.005 [1.002, 1.013] | same |
| explicit_tet4_ss | solve_step | eq16384 | 5 | 200 | 425.60 ms [424.78 ms, 428.41 ms] | 2.13 ms [2.12 ms, 2.14 ms] | 13.1 [13.1, 13.2] | – | same |
| explicit_tet4_ss | solve_step | eq262144 | 5 | 200 | 477.53 ms [477.20 ms, 484.07 ms] | 2.39 ms [2.39 ms, 2.42 ms] | 14.7 [14.7, 14.9] | 1.121 [1.117, 1.138] | same |
| explicit_tet4_ss | solve_step | eq4096 | 5 | 200 | 414.82 ms [413.78 ms, 417.25 ms] | 2.07 ms [2.07 ms, 2.09 ms] | 12.8 [12.8, 12.9] | 0.975 [0.966, 0.981] | same |
| explicit_tet4_ss | solve_step | eq65536 | 5 | 200 | 478.55 ms [476.47 ms, 481.40 ms] | 2.39 ms [2.38 ms, 2.41 ms] | 14.8 [14.7, 14.9] | 1.122 [1.120, 1.127] | same |
| explicit_tet4_ss | predictor | eq16384 | 5 | 200 | 21.4 µs [15.4 µs, 26.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | eq262144 | 5 | 200 | 29.5 µs [16.1 µs, 32.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.213 [0.735, 1.912] | same |
| explicit_tet4_ss | predictor | eq4096 | 5 | 200 | 22.8 µs [20.9 µs, 31.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.067 [0.795, 1.882] | same |
| explicit_tet4_ss | predictor | eq65536 | 5 | 200 | 24.7 µs [10.4 µs, 25.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.923 [0.628, 1.628] | same |
| explicit_tet4_ss | residual | eq16384 | 5 | 200 | 369.74 ms [367.94 ms, 372.29 ms] | 1.85 ms [1.84 ms, 1.86 ms] | 11.4 [11.4, 11.5] | – | same |
| explicit_tet4_ss | residual | eq262144 | 5 | 200 | 384.69 ms [383.74 ms, 387.02 ms] | 1.92 ms [1.92 ms, 1.94 ms] | 11.9 [11.8, 11.9] | 1.038 [1.034, 1.052] | same |
| explicit_tet4_ss | residual | eq4096 | 5 | 200 | 361.68 ms [360.03 ms, 363.64 ms] | 1.81 ms [1.80 ms, 1.82 ms] | 11.2 [11.1, 11.2] | 0.978 [0.967, 0.988] | same |
| explicit_tet4_ss | residual | eq65536 | 5 | 200 | 385.73 ms [383.34 ms, 387.97 ms] | 1.93 ms [1.92 ms, 1.94 ms] | 11.9 [11.8, 12.0] | 1.043 [1.038, 1.046] | same |
| explicit_tet4_ss | lumped_matrix | eq16384 | 5 | 200 | 36.6 µs [27.7 µs, 44.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | eq262144 | 5 | 200 | 41.8 µs [39.7 µs, 46.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.248 [0.900, 1.509] | same |
| explicit_tet4_ss | lumped_matrix | eq4096 | 5 | 200 | 30.2 µs [25.6 µs, 42.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.001 [0.685, 1.133] | same |
| explicit_tet4_ss | lumped_matrix | eq65536 | 5 | 200 | 41.3 µs [33.6 µs, 50.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.365 [0.846, 1.489] | same |
| explicit_tet4_ss | corrector | eq16384 | 5 | 200 | 20.6 µs [16.4 µs, 34.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | eq262144 | 5 | 200 | 19.1 µs [10.2 µs, 25.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.716 [0.493, 1.533] | same |
| explicit_tet4_ss | corrector | eq4096 | 5 | 200 | 19.7 µs [16.6 µs, 21.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.011 [0.595, 1.061] | same |
| explicit_tet4_ss | corrector | eq65536 | 5 | 200 | 23.2 µs [8.8 µs, 29.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.859 [0.479, 1.414] | same |
| explicit_tet4_ss | step_hooks | eq16384 | 5 | 400 | 520.9 µs [494.2 µs, 530.9 µs] | 1.3 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | eq262144 | 5 | 400 | 498.6 µs [432.8 µs, 577.9 µs] | 1.2 µs [1.1 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.939 [0.829, 1.170] | same |
| explicit_tet4_ss | step_hooks | eq4096 | 5 | 400 | 492.2 µs [482.4 µs, 534.5 µs] | 1.2 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.963 [0.927, 1.082] | same |
| explicit_tet4_ss | step_hooks | eq65536 | 5 | 400 | 510.4 µs [483.6 µs, 566.4 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.992 [0.927, 1.130] | same |
| explicit_tet4_ss | step_rest | eq16384 | 5 | 200 | 55.69 ms [55.22 ms, 56.87 ms] | 278.4 µs [276.1 µs, 284.3 µs] | 1.7 [1.7, 1.8] | – | same |
| explicit_tet4_ss | step_rest | eq262144 | 5 | 200 | 92.72 ms [92.26 ms, 96.46 ms] | 463.6 µs [461.3 µs, 482.3 µs] | 2.9 [2.8, 3.0] | 1.670 [1.657, 1.696] | same |
| explicit_tet4_ss | step_rest | eq4096 | 5 | 200 | 53.00 ms [52.55 ms, 53.43 ms] | 265.0 µs [262.8 µs, 267.1 µs] | 1.6 [1.6, 1.6] | 0.954 [0.932, 0.957] | same |
| explicit_tet4_ss | step_rest | eq65536 | 5 | 200 | 92.53 ms [92.22 ms, 92.78 ms] | 462.6 µs [461.1 µs, 463.9 µs] | 2.9 [2.8, 2.9] | 1.666 [1.626, 1.671] | same |
| explicit_tet4_ss | internal_forces | eq16384 | 5 | 40 | 61.36 ms [61.13 ms, 61.65 ms] | 1.53 ms [1.53 ms, 1.54 ms] | 9.5 [9.4, 9.5] | – | same |
| explicit_tet4_ss | internal_forces | eq262144 | 5 | 40 | 61.33 ms [61.18 ms, 61.53 ms] | 1.53 ms [1.53 ms, 1.54 ms] | 9.5 [9.4, 9.5] | 0.999 [0.992, 1.003] | same |
| explicit_tet4_ss | internal_forces | eq4096 | 5 | 40 | 61.17 ms [60.92 ms, 61.36 ms] | 1.53 ms [1.52 ms, 1.53 ms] | 9.4 [9.4, 9.5] | 0.993 [0.992, 1.001] | same |
| explicit_tet4_ss | internal_forces | eq65536 | 5 | 40 | 61.31 ms [60.78 ms, 61.63 ms] | 1.53 ms [1.52 ms, 1.54 ms] | 9.5 [9.4, 9.5] | 0.995 [0.991, 1.008] | same |
| explicit_tet4_ss | stresses | eq16384 | 5 | 40 | 39.65 ms [39.27 ms, 40.28 ms] | 991.3 µs [981.7 µs, 1.01 ms] | 6.1 [6.1, 6.2] | – | same |
| explicit_tet4_ss | stresses | eq262144 | 5 | 40 | 39.45 ms [38.98 ms, 39.95 ms] | 986.3 µs [974.5 µs, 998.7 µs] | 6.1 [6.0, 6.2] | 0.993 [0.987, 0.995] | same |
| explicit_tet4_ss | stresses | eq4096 | 5 | 40 | 39.66 ms [38.54 ms, 40.00 ms] | 991.5 µs [963.6 µs, 1.00 ms] | 6.1 [5.9, 6.2] | 1.000 [0.975, 1.004] | same |
| explicit_tet4_ss | stresses | eq65536 | 5 | 40 | 39.34 ms [38.96 ms, 39.79 ms] | 983.4 µs [974.1 µs, 994.8 µs] | 6.1 [6.0, 6.1] | 0.992 [0.968, 1.007] | same |
| explicit_tet4_ss | energies | eq16384 | 5 | 40 | 82.57 ms [81.87 ms, 83.80 ms] | 2.06 ms [2.05 ms, 2.10 ms] | 12.7 [12.6, 12.9] | – | same |
| explicit_tet4_ss | energies | eq262144 | 5 | 40 | 82.24 ms [82.20 ms, 82.26 ms] | 2.06 ms [2.06 ms, 2.06 ms] | 12.7 [12.7, 12.7] | 0.996 [0.981, 1.005] | same |
| explicit_tet4_ss | energies | eq4096 | 5 | 40 | 82.21 ms [82.09 ms, 82.33 ms] | 2.06 ms [2.05 ms, 2.06 ms] | 12.7 [12.7, 12.7] | 0.995 [0.982, 1.006] | same |
| explicit_tet4_ss | energies | eq65536 | 5 | 40 | 82.36 ms [81.91 ms, 83.39 ms] | 2.06 ms [2.05 ms, 2.08 ms] | 12.7 [12.6, 12.9] | 0.998 [0.983, 1.012] | same |
| explicit_tri3_ss | mesh | eq16384 | 5 | 1 | 3.99 ms [3.94 ms, 4.13 ms] | 3.99 ms [3.94 ms, 4.13 ms] | 22.1 [21.9, 22.9] | – | same |
| explicit_tri3_ss | mesh | eq262144 | 5 | 1 | 4.07 ms [4.05 ms, 4.20 ms] | 4.07 ms [4.05 ms, 4.20 ms] | 22.6 [22.5, 23.3] | 1.029 [0.982, 1.038] | same |
| explicit_tri3_ss | mesh | eq4096 | 5 | 1 | 4.08 ms [4.00 ms, 4.16 ms] | 4.08 ms [4.00 ms, 4.16 ms] | 22.7 [22.2, 23.1] | 1.008 [1.007, 1.024] | same |
| explicit_tri3_ss | mesh | eq65536 | 5 | 1 | 4.05 ms [3.97 ms, 4.14 ms] | 4.05 ms [3.97 ms, 4.14 ms] | 22.5 [22.0, 23.0] | 1.002 [0.972, 1.028] | same |
| explicit_tri3_ss | init | eq16384 | 5 | 1 | 38.35 ms [36.83 ms, 39.56 ms] | 38.35 ms [36.83 ms, 39.56 ms] | 213.0 [204.6, 219.8] | – | same |
| explicit_tri3_ss | init | eq262144 | 5 | 1 | 37.69 ms [37.02 ms, 38.68 ms] | 37.69 ms [37.02 ms, 38.68 ms] | 209.4 [205.7, 214.9] | 0.988 [0.948, 1.028] | same |
| explicit_tri3_ss | init | eq4096 | 5 | 1 | 38.01 ms [37.22 ms, 38.75 ms] | 38.01 ms [37.22 ms, 38.75 ms] | 211.2 [206.8, 215.3] | 1.003 [0.961, 1.010] | same |
| explicit_tri3_ss | init | eq65536 | 5 | 1 | 37.60 ms [37.17 ms, 40.15 ms] | 37.60 ms [37.17 ms, 40.15 ms] | 208.9 [206.5, 223.1] | 1.009 [0.951, 1.047] | same |
| explicit_tri3_ss | stable_time_step | eq16384 | 5 | 5 | 23.63 ms [22.95 ms, 24.61 ms] | 4.73 ms [4.59 ms, 4.92 ms] | 26.3 [25.5, 27.3] | – | same |
| explicit_tri3_ss | stable_time_step | eq262144 | 5 | 5 | 23.09 ms [22.51 ms, 23.57 ms] | 4.62 ms [4.50 ms, 4.71 ms] | 25.7 [25.0, 26.2] | 0.977 [0.915, 1.007] | same |
| explicit_tri3_ss | stable_time_step | eq4096 | 5 | 5 | 23.61 ms [22.99 ms, 23.67 ms] | 4.72 ms [4.60 ms, 4.73 ms] | 26.2 [25.5, 26.3] | 1.002 [0.960, 1.026] | same |
| explicit_tri3_ss | stable_time_step | eq65536 | 5 | 5 | 23.60 ms [23.55 ms, 24.99 ms] | 4.72 ms [4.71 ms, 5.00 ms] | 26.2 [26.2, 27.8] | 1.016 [0.962, 1.042] | same |
| explicit_tri3_ss | solve_step | eq16384 | 5 | 300 | 536.58 ms [534.53 ms, 544.59 ms] | 1.79 ms [1.78 ms, 1.82 ms] | 9.9 [9.9, 10.1] | – | same |
| explicit_tri3_ss | solve_step | eq262144 | 5 | 300 | 743.70 ms [741.73 ms, 746.86 ms] | 2.48 ms [2.47 ms, 2.49 ms] | 13.8 [13.7, 13.8] | 1.388 [1.365, 1.389] | same |
| explicit_tri3_ss | solve_step | eq4096 | 5 | 300 | 523.80 ms [519.84 ms, 531.61 ms] | 1.75 ms [1.73 ms, 1.77 ms] | 9.7 [9.6, 9.8] | 0.975 [0.956, 0.993] | same |
| explicit_tri3_ss | solve_step | eq65536 | 5 | 300 | 628.93 ms [619.56 ms, 646.89 ms] | 2.10 ms [2.07 ms, 2.16 ms] | 11.6 [11.5, 12.0] | 1.171 [1.157, 1.188] | same |
| explicit_tri3_ss | predictor | eq16384 | 5 | 300 | 26.1 µs [23.1 µs, 46.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | eq262144 | 5 | 300 | 40.8 µs [35.7 µs, 45.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.442 [0.916, 1.987] | same |
| explicit_tri3_ss | predictor | eq4096 | 5 | 300 | 33.0 µs [24.6 µs, 41.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.082 [0.594, 1.653] | same |
| explicit_tri3_ss | predictor | eq65536 | 5 | 300 | 42.9 µs [29.8 µs, 52.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.553 [0.976, 2.115] | same |
| explicit_tri3_ss | residual | eq16384 | 5 | 300 | 395.15 ms [392.64 ms, 400.15 ms] | 1.32 ms [1.31 ms, 1.33 ms] | 7.3 [7.3, 7.4] | – | same |
| explicit_tri3_ss | residual | eq262144 | 5 | 300 | 471.74 ms [469.84 ms, 474.31 ms] | 1.57 ms [1.57 ms, 1.58 ms] | 8.7 [8.7, 8.8] | 1.197 [1.175, 1.200] | same |
| explicit_tri3_ss | residual | eq4096 | 5 | 300 | 385.71 ms [382.88 ms, 394.78 ms] | 1.29 ms [1.28 ms, 1.32 ms] | 7.1 [7.1, 7.3] | 0.976 [0.958, 1.004] | same |
| explicit_tri3_ss | residual | eq65536 | 5 | 300 | 437.22 ms [430.88 ms, 457.06 ms] | 1.46 ms [1.44 ms, 1.52 ms] | 8.1 [8.0, 8.5] | 1.106 [1.093, 1.142] | same |
| explicit_tri3_ss | lumped_matrix | eq16384 | 5 | 300 | 68.2 µs [64.0 µs, 77.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | eq262144 | 5 | 300 | 73.7 µs [54.6 µs, 75.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.988 [0.853, 1.106] | same |
| explicit_tri3_ss | lumped_matrix | eq4096 | 5 | 300 | 58.2 µs [52.5 µs, 71.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.853 [0.762, 1.054] | same |
| explicit_tri3_ss | lumped_matrix | eq65536 | 5 | 300 | 67.8 µs [65.6 µs, 81.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.997 [0.865, 1.262] | same |
| explicit_tri3_ss | corrector | eq16384 | 5 | 300 | 25.7 µs [21.1 µs, 55.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | eq262144 | 5 | 300 | 45.2 µs [31.1 µs, 56.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.689 [0.903, 2.188] | same |
| explicit_tri3_ss | corrector | eq4096 | 5 | 300 | 33.3 µs [17.8 µs, 46.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.331 [0.323, 2.091] | same |
| explicit_tri3_ss | corrector | eq65536 | 5 | 300 | 39.4 µs [28.6 µs, 49.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.518 [0.714, 2.250] | same |
| explicit_tri3_ss | step_hooks | eq16384 | 5 | 600 | 790.6 µs [707.6 µs, 834.0 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | eq262144 | 5 | 600 | 857.7 µs [792.0 µs, 931.9 µs] | 1.4 µs [1.3 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 1.085 [0.950, 1.257] | same |
| explicit_tri3_ss | step_hooks | eq4096 | 5 | 600 | 797.7 µs [745.8 µs, 866.1 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.000 [0.954, 1.127] | same |
| explicit_tri3_ss | step_hooks | eq65536 | 5 | 600 | 754.9 µs [731.7 µs, 873.6 µs] | 1.3 µs [1.2 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.009 [0.901, 1.105] | same |
| explicit_tri3_ss | step_rest | eq16384 | 5 | 300 | 141.10 ms [140.55 ms, 143.43 ms] | 470.3 µs [468.5 µs, 478.1 µs] | 2.6 [2.6, 2.7] | – | same |
| explicit_tri3_ss | step_rest | eq262144 | 5 | 300 | 270.76 ms [269.97 ms, 272.68 ms] | 902.5 µs [899.9 µs, 908.9 µs] | 5.0 [5.0, 5.0] | 1.920 [1.897, 1.921] | same |
| explicit_tri3_ss | step_rest | eq4096 | 5 | 300 | 136.52 ms [135.58 ms, 138.18 ms] | 455.1 µs [451.9 µs, 460.6 µs] | 2.5 [2.5, 2.6] | 0.963 [0.950, 0.975] | same |
| explicit_tri3_ss | step_rest | eq65536 | 5 | 300 | 189.92 ms [187.72 ms, 190.60 ms] | 633.1 µs [625.7 µs, 635.3 µs] | 3.5 [3.5, 3.5] | 1.336 [1.317, 1.355] | same |
| explicit_tri3_ss | internal_forces | eq16384 | 5 | 60 | 48.24 ms [47.84 ms, 48.40 ms] | 804.0 µs [797.3 µs, 806.7 µs] | 4.5 [4.4, 4.5] | – | same |
| explicit_tri3_ss | internal_forces | eq262144 | 5 | 60 | 48.23 ms [48.02 ms, 48.45 ms] | 803.8 µs [800.4 µs, 807.6 µs] | 4.5 [4.4, 4.5] | 1.000 [0.993, 1.013] | same |
| explicit_tri3_ss | internal_forces | eq4096 | 5 | 60 | 48.21 ms [47.79 ms, 49.31 ms] | 803.4 µs [796.4 µs, 821.8 µs] | 4.5 [4.4, 4.6] | 0.998 [0.990, 1.025] | same |
| explicit_tri3_ss | internal_forces | eq65536 | 5 | 60 | 48.08 ms [47.59 ms, 48.30 ms] | 801.3 µs [793.2 µs, 805.0 µs] | 4.5 [4.4, 4.5] | 0.993 [0.986, 1.009] | same |
| explicit_tri3_ss | stresses | eq16384 | 5 | 60 | 32.56 ms [31.63 ms, 32.84 ms] | 542.7 µs [527.1 µs, 547.3 µs] | 3.0 [2.9, 3.0] | – | same |
| explicit_tri3_ss | stresses | eq262144 | 5 | 60 | 32.56 ms [32.25 ms, 32.90 ms] | 542.6 µs [537.5 µs, 548.3 µs] | 3.0 [3.0, 3.0] | 0.995 [0.984, 1.029] | same |
| explicit_tri3_ss | stresses | eq4096 | 5 | 60 | 32.70 ms [32.36 ms, 33.07 ms] | 545.0 µs [539.3 µs, 551.1 µs] | 3.0 [3.0, 3.1] | 1.009 [0.996, 1.031] | same |
| explicit_tri3_ss | stresses | eq65536 | 5 | 60 | 32.51 ms [31.90 ms, 33.61 ms] | 541.8 µs [531.7 µs, 560.1 µs] | 3.0 [3.0, 3.1] | 0.995 [0.980, 1.038] | same |
| explicit_tri3_ss | energies | eq16384 | 5 | 60 | 136.76 ms [135.70 ms, 137.54 ms] | 2.28 ms [2.26 ms, 2.29 ms] | 12.7 [12.6, 12.7] | – | same |
| explicit_tri3_ss | energies | eq262144 | 5 | 60 | 137.19 ms [136.57 ms, 137.59 ms] | 2.29 ms [2.28 ms, 2.29 ms] | 12.7 [12.6, 12.7] | 1.002 [0.993, 1.013] | same |
| explicit_tri3_ss | energies | eq4096 | 5 | 60 | 137.32 ms [137.04 ms, 137.49 ms] | 2.29 ms [2.28 ms, 2.29 ms] | 12.7 [12.7, 12.7] | 1.005 [0.997, 1.010] | same |
| explicit_tri3_ss | energies | eq65536 | 5 | 60 | 136.61 ms [135.84 ms, 137.66 ms] | 2.28 ms [2.26 ms, 2.29 ms] | 12.6 [12.6, 12.7] | 1.001 [0.993, 1.008] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | 7a6ad8f42 | 37ce0eeb914dd994 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42 | fecb3fb4b46c8b8c |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7a6ad8f42 | b6b902f5ca71049e |
