Suite `solid`, size `m`; variants: `al-base`, `al-head`, `base-t1`, `head2-t1`; reference `al-base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to al-base | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars | mesh | al-base | 7 | 1 | 2.10 ms [2.05 ms, 2.17 ms] | 2.10 ms [2.05 ms, 2.17 ms] | 19.9 [19.4, 20.6] | – | same |
| explicit_mazars | mesh | al-head | 7 | 1 | 2.08 ms [2.03 ms, 2.18 ms] | 2.08 ms [2.03 ms, 2.18 ms] | 19.8 [19.3, 20.7] | 1.008 [0.934, 1.056] | same |
| explicit_mazars | mesh | base-t1 | 7 | 1 | 2.08 ms [2.05 ms, 2.17 ms] | 2.08 ms [2.05 ms, 2.17 ms] | 19.7 [19.5, 20.6] | 0.996 [0.957, 1.054] | same |
| explicit_mazars | mesh | head2-t1 | 7 | 1 | 2.09 ms [2.07 ms, 2.23 ms] | 2.09 ms [2.07 ms, 2.23 ms] | 19.8 [19.6, 21.1] | 1.010 [0.960, 1.050] | same |
| explicit_mazars | init | al-base | 7 | 1 | 32.08 ms [31.72 ms, 33.01 ms] | 32.08 ms [31.72 ms, 33.01 ms] | 304.2 [300.8, 313.0] | – | same |
| explicit_mazars | init | al-head | 7 | 1 | 32.54 ms [32.05 ms, 33.34 ms] | 32.54 ms [32.05 ms, 33.34 ms] | 308.6 [303.9, 316.1] | 1.009 [0.986, 1.035] | same |
| explicit_mazars | init | base-t1 | 7 | 1 | 32.67 ms [32.17 ms, 33.78 ms] | 32.67 ms [32.17 ms, 33.78 ms] | 309.8 [305.1, 320.3] | 1.018 [0.989, 1.029] | same |
| explicit_mazars | init | head2-t1 | 7 | 1 | 32.37 ms [31.69 ms, 32.57 ms] | 32.37 ms [31.69 ms, 32.57 ms] | 307.0 [300.5, 308.9] | 1.000 [0.982, 1.025] | same |
| explicit_mazars | stable_time_step | al-base | 7 | 5 | 20.93 ms [20.90 ms, 20.97 ms] | 4.19 ms [4.18 ms, 4.19 ms] | 39.7 [39.6, 39.8] | – | same |
| explicit_mazars | stable_time_step | al-head | 7 | 5 | 21.10 ms [21.02 ms, 21.65 ms] | 4.22 ms [4.20 ms, 4.33 ms] | 40.0 [39.9, 41.1] | 1.006 [1.004, 1.035] | same |
| explicit_mazars | stable_time_step | base-t1 | 7 | 5 | 20.95 ms [20.92 ms, 21.40 ms] | 4.19 ms [4.18 ms, 4.28 ms] | 39.7 [39.7, 40.6] | 1.001 [0.998, 1.023] | same |
| explicit_mazars | stable_time_step | head2-t1 | 7 | 5 | 21.05 ms [21.02 ms, 21.43 ms] | 4.21 ms [4.20 ms, 4.29 ms] | 39.9 [39.9, 40.7] | 1.006 [1.002, 1.023] | same |
| explicit_mazars | solve_step | al-base | 7 | 80 | 2.689 s [2.677 s, 2.693 s] | 33.61 ms [33.46 ms, 33.67 ms] | 318.8 [317.3, 319.3] | – | same |
| explicit_mazars | solve_step | al-head | 7 | 80 | 2.699 s [2.685 s, 2.712 s] | 33.73 ms [33.56 ms, 33.90 ms] | 319.9 [318.3, 321.5] | 1.003 [1.000, 1.013] | same |
| explicit_mazars | solve_step | base-t1 | 7 | 80 | 2.671 s [2.669 s, 2.732 s] | 33.39 ms [33.36 ms, 34.15 ms] | 316.6 [316.3, 323.8] | 0.995 [0.991, 1.020] | same |
| explicit_mazars | solve_step | head2-t1 | 7 | 80 | 2.712 s [2.706 s, 2.725 s] | 33.91 ms [33.82 ms, 34.06 ms] | 321.5 [320.7, 323.0] | 1.010 [1.006, 1.018] | same |
| explicit_mazars | predictor | al-base | 7 | 80 | 6.7 µs [6.3 µs, 8.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | al-head | 7 | 80 | 11.6 µs [5.2 µs, 14.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.797 [0.699, 2.238] | same |
| explicit_mazars | predictor | base-t1 | 7 | 80 | 7.4 µs [4.0 µs, 11.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.135 [0.585, 1.515] | same |
| explicit_mazars | predictor | head2-t1 | 7 | 80 | 14.6 µs [12.2 µs, 17.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.039 [1.409, 2.615] | same |
| explicit_mazars | residual | al-base | 7 | 80 | 2.527 s [2.515 s, 2.531 s] | 31.59 ms [31.44 ms, 31.64 ms] | 299.6 [298.1, 300.0] | – | same |
| explicit_mazars | residual | al-head | 7 | 80 | 2.535 s [2.522 s, 2.548 s] | 31.68 ms [31.53 ms, 31.85 ms] | 300.4 [299.0, 302.1] | 1.003 [1.000, 1.013] | same |
| explicit_mazars | residual | base-t1 | 7 | 80 | 2.509 s [2.507 s, 2.570 s] | 31.37 ms [31.34 ms, 32.13 ms] | 297.4 [297.2, 304.7] | 0.994 [0.991, 1.021] | same |
| explicit_mazars | residual | head2-t1 | 7 | 80 | 2.549 s [2.543 s, 2.562 s] | 31.86 ms [31.79 ms, 32.03 ms] | 302.1 [301.5, 303.7] | 1.010 [1.006, 1.019] | same |
| explicit_mazars | lumped_matrix | al-base | 7 | 80 | 17.0 µs [12.1 µs, 18.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | al-head | 7 | 80 | 19.4 µs [16.7 µs, 22.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.263 [1.071, 1.607] | same |
| explicit_mazars | lumped_matrix | base-t1 | 7 | 80 | 17.7 µs [15.3 µs, 19.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.074 [0.891, 1.605] | same |
| explicit_mazars | lumped_matrix | head2-t1 | 7 | 80 | 20.5 µs [19.5 µs, 23.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.254 [1.177, 1.898] | same |
| explicit_mazars | corrector | al-base | 7 | 80 | 13.2 µs [12.1 µs, 21.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | al-head | 7 | 80 | 21.7 µs [14.3 µs, 32.7 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.485 [1.050, 2.433] | same |
| explicit_mazars | corrector | base-t1 | 7 | 80 | 12.7 µs [12.1 µs, 13.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.949 [0.574, 1.156] | same |
| explicit_mazars | corrector | head2-t1 | 7 | 80 | 20.2 µs [13.5 µs, 21.7 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.522 [1.005, 1.802] | same |
| explicit_mazars | step_hooks | al-base | 7 | 160 | 132.25 ms [131.52 ms, 132.79 ms] | 826.5 µs [822.0 µs, 830.0 µs] | 7.8 [7.8, 7.9] | – | same |
| explicit_mazars | step_hooks | al-head | 7 | 160 | 133.46 ms [132.94 ms, 135.14 ms] | 834.1 µs [830.9 µs, 844.6 µs] | 7.9 [7.9, 8.0] | 1.011 [1.003, 1.022] | same |
| explicit_mazars | step_hooks | base-t1 | 7 | 160 | 132.13 ms [131.86 ms, 132.96 ms] | 825.8 µs [824.2 µs, 831.0 µs] | 7.8 [7.8, 7.9] | 0.999 [0.994, 1.005] | same |
| explicit_mazars | step_hooks | head2-t1 | 7 | 160 | 133.28 ms [132.97 ms, 133.92 ms] | 833.0 µs [831.1 µs, 837.0 µs] | 7.9 [7.9, 7.9] | 1.008 [1.007, 1.012] | same |
| explicit_mazars | step_rest | al-base | 7 | 80 | 29.27 ms [29.07 ms, 29.88 ms] | 365.8 µs [363.4 µs, 373.6 µs] | 3.5 [3.4, 3.5] | – | same |
| explicit_mazars | step_rest | al-head | 7 | 80 | 29.44 ms [29.15 ms, 29.95 ms] | 368.1 µs [364.4 µs, 374.4 µs] | 3.5 [3.5, 3.6] | 1.007 [0.984, 1.018] | same |
| explicit_mazars | step_rest | base-t1 | 7 | 80 | 29.17 ms [28.78 ms, 29.34 ms] | 364.6 µs [359.7 µs, 366.7 µs] | 3.5 [3.4, 3.5] | 0.985 [0.971, 1.004] | same |
| explicit_mazars | step_rest | head2-t1 | 7 | 80 | 29.04 ms [28.73 ms, 29.39 ms] | 363.0 µs [359.2 µs, 367.3 µs] | 3.4 [3.4, 3.5] | 0.993 [0.961, 1.011] | same |
| explicit_mazars | internal_forces | al-base | 7 | 16 | 543.74 ms [540.63 ms, 545.69 ms] | 33.98 ms [33.79 ms, 34.11 ms] | 322.3 [320.4, 323.4] | – | same |
| explicit_mazars | internal_forces | al-head | 7 | 16 | 547.65 ms [543.81 ms, 548.53 ms] | 34.23 ms [33.99 ms, 34.28 ms] | 324.6 [322.3, 325.1] | 1.006 [1.004, 1.009] | same |
| explicit_mazars | internal_forces | base-t1 | 7 | 16 | 542.97 ms [540.07 ms, 544.22 ms] | 33.94 ms [33.75 ms, 34.01 ms] | 321.8 [320.1, 322.5] | 0.998 [0.994, 1.007] | same |
| explicit_mazars | internal_forces | head2-t1 | 7 | 16 | 549.49 ms [545.37 ms, 553.52 ms] | 34.34 ms [34.09 ms, 34.59 ms] | 325.7 [323.2, 328.0] | 1.011 [1.003, 1.020] | same |
| explicit_mazars | stresses | al-base | 7 | 16 | 509.41 ms [507.08 ms, 511.19 ms] | 31.84 ms [31.69 ms, 31.95 ms] | 301.9 [300.5, 303.0] | – | same |
| explicit_mazars | stresses | al-head | 7 | 16 | 510.14 ms [508.65 ms, 512.36 ms] | 31.88 ms [31.79 ms, 32.02 ms] | 302.3 [301.5, 303.7] | 1.003 [0.998, 1.010] | same |
| explicit_mazars | stresses | base-t1 | 7 | 16 | 505.34 ms [504.44 ms, 507.58 ms] | 31.58 ms [31.53 ms, 31.72 ms] | 299.5 [299.0, 300.8] | 0.995 [0.987, 1.001] | same |
| explicit_mazars | stresses | head2-t1 | 7 | 16 | 513.14 ms [512.73 ms, 514.88 ms] | 32.07 ms [32.05 ms, 32.18 ms] | 304.1 [303.9, 305.2] | 1.007 [1.003, 1.015] | same |
| explicit_mazars | energies | al-base | 7 | 16 | 23.25 ms [22.94 ms, 24.18 ms] | 1.45 ms [1.43 ms, 1.51 ms] | 13.8 [13.6, 14.3] | – | same |
| explicit_mazars | energies | al-head | 7 | 16 | 24.21 ms [23.97 ms, 24.56 ms] | 1.51 ms [1.50 ms, 1.54 ms] | 14.3 [14.2, 14.6] | 1.039 [0.993, 1.069] | same |
| explicit_mazars | energies | base-t1 | 7 | 16 | 22.41 ms [22.30 ms, 23.54 ms] | 1.40 ms [1.39 ms, 1.47 ms] | 13.3 [13.2, 14.0] | 0.971 [0.922, 1.013] | same |
| explicit_mazars | energies | head2-t1 | 7 | 16 | 24.14 ms [23.64 ms, 24.55 ms] | 1.51 ms [1.48 ms, 1.53 ms] | 14.3 [14.0, 14.6] | 1.030 [0.997, 1.070] | same |
| explicit_multi_material | mesh | al-base | 7 | 1 | 2.13 ms [2.09 ms, 2.17 ms] | 2.13 ms [2.09 ms, 2.17 ms] | 20.2 [19.8, 20.6] | – | same |
| explicit_multi_material | mesh | al-head | 7 | 1 | 2.08 ms [2.05 ms, 2.15 ms] | 2.08 ms [2.05 ms, 2.15 ms] | 19.8 [19.4, 20.4] | 0.976 [0.953, 1.022] | same |
| explicit_multi_material | mesh | base-t1 | 7 | 1 | 2.08 ms [2.03 ms, 2.16 ms] | 2.08 ms [2.03 ms, 2.16 ms] | 19.8 [19.2, 20.5] | 0.975 [0.956, 1.002] | same |
| explicit_multi_material | mesh | head2-t1 | 7 | 1 | 2.15 ms [2.06 ms, 2.18 ms] | 2.15 ms [2.06 ms, 2.18 ms] | 20.4 [19.5, 20.6] | 1.001 [0.948, 1.030] | same |
| explicit_multi_material | init | al-base | 7 | 1 | 34.17 ms [33.56 ms, 35.09 ms] | 34.17 ms [33.56 ms, 35.09 ms] | 324.0 [318.2, 332.8] | – | same |
| explicit_multi_material | init | al-head | 7 | 1 | 33.69 ms [32.79 ms, 34.31 ms] | 33.69 ms [32.79 ms, 34.31 ms] | 319.5 [310.9, 325.4] | 0.996 [0.959, 1.015] | same |
| explicit_multi_material | init | base-t1 | 7 | 1 | 33.77 ms [33.11 ms, 35.11 ms] | 33.77 ms [33.11 ms, 35.11 ms] | 320.2 [314.0, 332.9] | 0.991 [0.962, 1.027] | same |
| explicit_multi_material | init | head2-t1 | 7 | 1 | 33.76 ms [33.20 ms, 34.06 ms] | 33.76 ms [33.20 ms, 34.06 ms] | 320.1 [314.8, 323.0] | 0.989 [0.946, 1.007] | same |
| explicit_multi_material | stable_time_step | al-base | 7 | 5 | 21.33 ms [21.31 ms, 21.51 ms] | 4.27 ms [4.26 ms, 4.30 ms] | 40.5 [40.4, 40.8] | – | same |
| explicit_multi_material | stable_time_step | al-head | 7 | 5 | 21.09 ms [21.07 ms, 21.20 ms] | 4.22 ms [4.21 ms, 4.24 ms] | 40.0 [40.0, 40.2] | 0.989 [0.980, 0.994] | same |
| explicit_multi_material | stable_time_step | base-t1 | 7 | 5 | 21.02 ms [20.98 ms, 21.40 ms] | 4.20 ms [4.20 ms, 4.28 ms] | 39.9 [39.8, 40.6] | 0.986 [0.978, 1.004] | same |
| explicit_multi_material | stable_time_step | head2-t1 | 7 | 5 | 24.50 ms [23.20 ms, 24.75 ms] | 4.90 ms [4.64 ms, 4.95 ms] | 46.5 [44.0, 46.9] | 1.147 [1.080, 1.161] | same |
| explicit_multi_material | solve_step | al-base | 7 | 120 | 1.998 s [1.993 s, 2.004 s] | 16.65 ms [16.61 ms, 16.70 ms] | 157.9 [157.5, 158.4] | – | same |
| explicit_multi_material | solve_step | al-head | 7 | 120 | 2.020 s [2.003 s, 2.080 s] | 16.83 ms [16.69 ms, 17.33 ms] | 159.6 [158.3, 164.3] | 1.010 [1.002, 1.041] | same |
| explicit_multi_material | solve_step | base-t1 | 7 | 120 | 1.993 s [1.976 s, 2.001 s] | 16.61 ms [16.47 ms, 16.68 ms] | 157.5 [156.1, 158.2] | 0.996 [0.992, 1.001] | same |
| explicit_multi_material | solve_step | head2-t1 | 7 | 120 | 2.030 s [2.020 s, 2.039 s] | 16.91 ms [16.83 ms, 17.00 ms] | 160.4 [159.6, 161.2] | 1.015 [1.012, 1.019] | same |
| explicit_multi_material | predictor | al-base | 7 | 120 | 11.1 µs [10.2 µs, 14.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | predictor | al-head | 7 | 120 | 12.5 µs [10.3 µs, 14.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.018 [0.936, 1.415] | same |
| explicit_multi_material | predictor | base-t1 | 7 | 120 | 8.9 µs [5.1 µs, 16.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.799 [0.465, 1.613] | same |
| explicit_multi_material | predictor | head2-t1 | 7 | 120 | 19.0 µs [17.6 µs, 21.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.750 [1.272, 2.050] | same |
| explicit_multi_material | residual | al-base | 7 | 120 | 1.863 s [1.858 s, 1.869 s] | 15.52 ms [15.49 ms, 15.58 ms] | 147.2 [146.8, 147.7] | – | same |
| explicit_multi_material | residual | al-head | 7 | 120 | 1.884 s [1.868 s, 1.945 s] | 15.70 ms [15.57 ms, 16.21 ms] | 148.9 [147.6, 153.7] | 1.010 [1.002, 1.044] | same |
| explicit_multi_material | residual | base-t1 | 7 | 120 | 1.858 s [1.841 s, 1.866 s] | 15.48 ms [15.34 ms, 15.55 ms] | 146.8 [145.5, 147.5] | 0.997 [0.991, 1.001] | same |
| explicit_multi_material | residual | head2-t1 | 7 | 120 | 1.894 s [1.885 s, 1.904 s] | 15.79 ms [15.71 ms, 15.87 ms] | 149.7 [148.9, 150.5] | 1.016 [1.012, 1.020] | same |
| explicit_multi_material | lumped_matrix | al-base | 7 | 120 | 28.9 µs [24.9 µs, 33.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | lumped_matrix | al-head | 7 | 120 | 30.4 µs [27.4 µs, 36.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.054 [0.945, 1.256] | same |
| explicit_multi_material | lumped_matrix | base-t1 | 7 | 120 | 28.3 µs [25.1 µs, 31.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.980 [0.843, 1.254] | same |
| explicit_multi_material | lumped_matrix | head2-t1 | 7 | 120 | 32.1 µs [28.2 µs, 34.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.119 [0.976, 1.291] | same |
| explicit_multi_material | corrector | al-base | 7 | 120 | 18.1 µs [16.6 µs, 20.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | corrector | al-head | 7 | 120 | 18.7 µs [15.3 µs, 24.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.968 [0.860, 1.225] | same |
| explicit_multi_material | corrector | base-t1 | 7 | 120 | 17.1 µs [16.2 µs, 21.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.912 [0.857, 1.201] | same |
| explicit_multi_material | corrector | head2-t1 | 7 | 120 | 18.2 µs [16.2 µs, 19.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.973 [0.818, 1.092] | same |
| explicit_multi_material | step_hooks | al-base | 7 | 240 | 94.19 ms [93.50 ms, 94.95 ms] | 392.5 µs [389.6 µs, 395.6 µs] | 3.7 [3.7, 3.8] | – | same |
| explicit_multi_material | step_hooks | al-head | 7 | 240 | 94.56 ms [94.29 ms, 95.41 ms] | 394.0 µs [392.9 µs, 397.5 µs] | 3.7 [3.7, 3.8] | 1.003 [0.997, 1.016] | same |
| explicit_multi_material | step_hooks | base-t1 | 7 | 240 | 94.12 ms [93.77 ms, 94.34 ms] | 392.2 µs [390.7 µs, 393.1 µs] | 3.7 [3.7, 3.7] | 1.000 [0.990, 1.009] | same |
| explicit_multi_material | step_hooks | head2-t1 | 7 | 240 | 94.52 ms [94.29 ms, 95.11 ms] | 393.9 µs [392.9 µs, 396.3 µs] | 3.7 [3.7, 3.8] | 1.003 [0.996, 1.017] | same |
| explicit_multi_material | step_rest | al-base | 7 | 120 | 40.29 ms [39.80 ms, 41.21 ms] | 335.7 µs [331.7 µs, 343.4 µs] | 3.2 [3.1, 3.3] | – | same |
| explicit_multi_material | step_rest | al-head | 7 | 120 | 40.40 ms [40.16 ms, 41.15 ms] | 336.7 µs [334.7 µs, 342.9 µs] | 3.2 [3.2, 3.3] | 1.005 [0.980, 1.028] | same |
| explicit_multi_material | step_rest | base-t1 | 7 | 120 | 40.14 ms [39.91 ms, 40.43 ms] | 334.5 µs [332.6 µs, 336.9 µs] | 3.2 [3.2, 3.2] | 1.003 [0.980, 1.005] | same |
| explicit_multi_material | step_rest | head2-t1 | 7 | 120 | 40.50 ms [40.10 ms, 41.09 ms] | 337.5 µs [334.2 µs, 342.4 µs] | 3.2 [3.2, 3.2] | 1.005 [0.979, 1.024] | same |
| explicit_multi_material | internal_forces | al-base | 7 | 24 | 371.79 ms [370.74 ms, 372.68 ms] | 15.49 ms [15.45 ms, 15.53 ms] | 146.9 [146.5, 147.2] | – | same |
| explicit_multi_material | internal_forces | al-head | 7 | 24 | 375.92 ms [373.82 ms, 377.85 ms] | 15.66 ms [15.58 ms, 15.74 ms] | 148.5 [147.7, 149.3] | 1.012 [1.005, 1.018] | same |
| explicit_multi_material | internal_forces | base-t1 | 7 | 24 | 373.00 ms [369.64 ms, 373.62 ms] | 15.54 ms [15.40 ms, 15.57 ms] | 147.4 [146.0, 147.6] | 1.003 [0.996, 1.006] | same |
| explicit_multi_material | internal_forces | head2-t1 | 7 | 24 | 377.23 ms [374.48 ms, 390.99 ms] | 15.72 ms [15.60 ms, 16.29 ms] | 149.0 [148.0, 154.5] | 1.015 [1.009, 1.052] | same |
| explicit_multi_material | stresses | al-base | 7 | 24 | 313.68 ms [312.86 ms, 314.35 ms] | 13.07 ms [13.04 ms, 13.10 ms] | 123.9 [123.6, 124.2] | – | same |
| explicit_multi_material | stresses | al-head | 7 | 24 | 317.27 ms [316.17 ms, 320.50 ms] | 13.22 ms [13.17 ms, 13.35 ms] | 125.4 [124.9, 126.6] | 1.012 [1.006, 1.024] | same |
| explicit_multi_material | stresses | base-t1 | 7 | 24 | 315.56 ms [313.99 ms, 316.30 ms] | 13.15 ms [13.08 ms, 13.18 ms] | 124.7 [124.1, 125.0] | 1.006 [1.001, 1.011] | same |
| explicit_multi_material | stresses | head2-t1 | 7 | 24 | 320.23 ms [317.47 ms, 320.74 ms] | 13.34 ms [13.23 ms, 13.36 ms] | 126.5 [125.4, 126.7] | 1.020 [1.013, 1.022] | same |
| explicit_multi_material | energies | al-base | 7 | 24 | 37.65 ms [37.42 ms, 38.03 ms] | 1.57 ms [1.56 ms, 1.58 ms] | 14.9 [14.8, 15.0] | – | same |
| explicit_multi_material | energies | al-head | 7 | 24 | 38.68 ms [38.59 ms, 38.89 ms] | 1.61 ms [1.61 ms, 1.62 ms] | 15.3 [15.2, 15.4] | 1.027 [1.019, 1.039] | same |
| explicit_multi_material | energies | base-t1 | 7 | 24 | 37.20 ms [37.04 ms, 37.50 ms] | 1.55 ms [1.54 ms, 1.56 ms] | 14.7 [14.6, 14.8] | 0.987 [0.977, 1.002] | same |
| explicit_multi_material | energies | head2-t1 | 7 | 24 | 38.56 ms [38.38 ms, 38.69 ms] | 1.61 ms [1.60 ms, 1.61 ms] | 15.2 [15.2, 15.3] | 1.027 [1.009, 1.029] | same |
| explicit_tet4_ss | mesh | al-base | 7 | 1 | 3.21 ms [3.11 ms, 3.35 ms] | 3.21 ms [3.11 ms, 3.35 ms] | 19.8 [19.2, 20.7] | – | same |
| explicit_tet4_ss | mesh | al-head | 7 | 1 | 3.24 ms [3.17 ms, 3.32 ms] | 3.24 ms [3.17 ms, 3.32 ms] | 20.0 [19.6, 20.5] | 1.006 [0.962, 1.055] | same |
| explicit_tet4_ss | mesh | base-t1 | 7 | 1 | 3.21 ms [3.16 ms, 3.26 ms] | 3.21 ms [3.16 ms, 3.26 ms] | 19.8 [19.5, 20.1] | 0.987 [0.953, 1.038] | same |
| explicit_tet4_ss | mesh | head2-t1 | 7 | 1 | 3.25 ms [3.19 ms, 3.27 ms] | 3.25 ms [3.19 ms, 3.27 ms] | 20.1 [19.7, 20.2] | 1.007 [0.977, 1.025] | same |
| explicit_tet4_ss | init | al-base | 7 | 1 | 48.10 ms [47.15 ms, 49.22 ms] | 48.10 ms [47.15 ms, 49.22 ms] | 296.9 [291.1, 303.8] | – | same |
| explicit_tet4_ss | init | al-head | 7 | 1 | 47.39 ms [46.46 ms, 47.89 ms] | 47.39 ms [46.46 ms, 47.89 ms] | 292.5 [286.8, 295.6] | 0.988 [0.946, 1.013] | same |
| explicit_tet4_ss | init | base-t1 | 7 | 1 | 47.55 ms [47.42 ms, 48.42 ms] | 47.55 ms [47.42 ms, 48.42 ms] | 293.5 [292.7, 298.9] | 0.998 [0.980, 1.006] | same |
| explicit_tet4_ss | init | head2-t1 | 7 | 1 | 47.18 ms [46.54 ms, 48.05 ms] | 47.18 ms [46.54 ms, 48.05 ms] | 291.3 [287.3, 296.6] | 0.980 [0.962, 0.999] | same |
| explicit_tet4_ss | stable_time_step | al-base | 7 | 5 | 32.17 ms [32.12 ms, 34.37 ms] | 6.43 ms [6.42 ms, 6.87 ms] | 39.7 [39.6, 42.4] | – | same |
| explicit_tet4_ss | stable_time_step | al-head | 7 | 5 | 32.38 ms [32.30 ms, 32.91 ms] | 6.48 ms [6.46 ms, 6.58 ms] | 40.0 [39.9, 40.6] | 1.005 [0.940, 1.025] | same |
| explicit_tet4_ss | stable_time_step | base-t1 | 7 | 5 | 32.16 ms [32.11 ms, 32.25 ms] | 6.43 ms [6.42 ms, 6.45 ms] | 39.7 [39.6, 39.8] | 1.000 [0.936, 1.003] | same |
| explicit_tet4_ss | stable_time_step | head2-t1 | 7 | 5 | 32.35 ms [32.28 ms, 33.81 ms] | 6.47 ms [6.46 ms, 6.76 ms] | 39.9 [39.9, 41.7] | 1.005 [0.942, 1.045] | same |
| explicit_tet4_ss | solve_step | al-base | 7 | 200 | 1.285 s [1.281 s, 1.309 s] | 6.42 ms [6.40 ms, 6.54 ms] | 39.7 [39.5, 40.4] | – | same |
| explicit_tet4_ss | solve_step | al-head | 7 | 200 | 1.281 s [1.276 s, 1.293 s] | 6.41 ms [6.38 ms, 6.47 ms] | 39.5 [39.4, 39.9] | 0.996 [0.986, 1.007] | same |
| explicit_tet4_ss | solve_step | base-t1 | 7 | 200 | 1.276 s [1.270 s, 1.283 s] | 6.38 ms [6.35 ms, 6.42 ms] | 39.4 [39.2, 39.6] | 0.996 [0.976, 0.999] | same |
| explicit_tet4_ss | solve_step | head2-t1 | 7 | 200 | 1.286 s [1.277 s, 1.511 s] | 6.43 ms [6.38 ms, 7.56 ms] | 39.7 [39.4, 46.6] | 1.000 [0.987, 1.174] | same |
| explicit_tet4_ss | predictor | al-base | 7 | 200 | 15.4 µs [11.0 µs, 21.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | al-head | 7 | 200 | 11.9 µs [8.8 µs, 23.4 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.748 [0.513, 2.136] | same |
| explicit_tet4_ss | predictor | base-t1 | 7 | 200 | 7.7 µs [6.3 µs, 10.2 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.462 [0.366, 0.934] | same |
| explicit_tet4_ss | predictor | head2-t1 | 7 | 200 | 9.9 µs [7.0 µs, 12.5 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.622 [0.392, 0.970] | same |
| explicit_tet4_ss | residual | al-base | 7 | 200 | 1.195 s [1.190 s, 1.218 s] | 5.98 ms [5.95 ms, 6.09 ms] | 36.9 [36.7, 37.6] | – | same |
| explicit_tet4_ss | residual | al-head | 7 | 200 | 1.191 s [1.186 s, 1.202 s] | 5.96 ms [5.93 ms, 6.01 ms] | 36.8 [36.6, 37.1] | 0.997 [0.986, 1.007] | same |
| explicit_tet4_ss | residual | base-t1 | 7 | 200 | 1.190 s [1.183 s, 1.196 s] | 5.95 ms [5.92 ms, 5.98 ms] | 36.7 [36.5, 36.9] | 0.999 [0.978, 1.001] | same |
| explicit_tet4_ss | residual | head2-t1 | 7 | 200 | 1.196 s [1.187 s, 1.420 s] | 5.98 ms [5.94 ms, 7.10 ms] | 36.9 [36.6, 43.8] | 1.001 [0.987, 1.185] | same |
| explicit_tet4_ss | lumped_matrix | al-base | 7 | 200 | 33.5 µs [28.1 µs, 37.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | al-head | 7 | 200 | 44.1 µs [32.8 µs, 51.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.325 [0.889, 1.697] | same |
| explicit_tet4_ss | lumped_matrix | base-t1 | 7 | 200 | 31.1 µs [29.5 µs, 36.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.988 [0.876, 1.078] | same |
| explicit_tet4_ss | lumped_matrix | head2-t1 | 7 | 200 | 41.2 µs [32.0 µs, 48.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.176 [0.955, 1.552] | same |
| explicit_tet4_ss | corrector | al-base | 7 | 200 | 16.0 µs [13.9 µs, 22.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | al-head | 7 | 200 | 16.3 µs [14.6 µs, 34.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.049 [0.930, 1.636] | same |
| explicit_tet4_ss | corrector | base-t1 | 7 | 200 | 13.7 µs [9.6 µs, 26.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.831 [0.438, 1.932] | same |
| explicit_tet4_ss | corrector | head2-t1 | 7 | 200 | 26.7 µs [16.5 µs, 30.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.427 [0.751, 1.989] | same |
| explicit_tet4_ss | step_hooks | al-base | 7 | 400 | 525.8 µs [500.7 µs, 554.5 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | al-head | 7 | 400 | 518.5 µs [482.2 µs, 566.1 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.034 [0.890, 1.112] | same |
| explicit_tet4_ss | step_hooks | base-t1 | 7 | 400 | 525.2 µs [510.1 µs, 545.7 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.018 [0.936, 1.041] | same |
| explicit_tet4_ss | step_hooks | head2-t1 | 7 | 400 | 550.1 µs [514.7 µs, 576.3 µs] | 1.4 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.060 [0.928, 1.144] | same |
| explicit_tet4_ss | step_rest | al-base | 7 | 200 | 89.78 ms [88.51 ms, 90.55 ms] | 448.9 µs [442.6 µs, 452.7 µs] | 2.8 [2.7, 2.8] | – | same |
| explicit_tet4_ss | step_rest | al-head | 7 | 200 | 89.18 ms [88.70 ms, 90.19 ms] | 445.9 µs [443.5 µs, 451.0 µs] | 2.8 [2.7, 2.8] | 0.994 [0.985, 1.008] | same |
| explicit_tet4_ss | step_rest | base-t1 | 7 | 200 | 86.15 ms [85.64 ms, 86.51 ms] | 430.8 µs [428.2 µs, 432.6 µs] | 2.7 [2.6, 2.7] | 0.962 [0.950, 0.976] | same |
| explicit_tet4_ss | step_rest | head2-t1 | 7 | 200 | 89.33 ms [88.71 ms, 90.44 ms] | 446.6 µs [443.5 µs, 452.2 µs] | 2.8 [2.7, 2.8] | 0.994 [0.987, 1.022] | same |
| explicit_tet4_ss | internal_forces | al-base | 7 | 40 | 226.68 ms [225.52 ms, 228.85 ms] | 5.67 ms [5.64 ms, 5.72 ms] | 35.0 [34.8, 35.3] | – | same |
| explicit_tet4_ss | internal_forces | al-head | 7 | 40 | 227.31 ms [225.54 ms, 228.52 ms] | 5.68 ms [5.64 ms, 5.71 ms] | 35.1 [34.8, 35.3] | 1.001 [0.993, 1.012] | same |
| explicit_tet4_ss | internal_forces | base-t1 | 7 | 40 | 227.50 ms [225.11 ms, 228.85 ms] | 5.69 ms [5.63 ms, 5.72 ms] | 35.1 [34.7, 35.3] | 0.998 [0.996, 1.009] | same |
| explicit_tet4_ss | internal_forces | head2-t1 | 7 | 40 | 226.85 ms [225.82 ms, 228.57 ms] | 5.67 ms [5.65 ms, 5.71 ms] | 35.0 [34.8, 35.3] | 1.003 [0.992, 1.009] | same |
| explicit_tet4_ss | stresses | al-base | 7 | 40 | 86.08 ms [85.22 ms, 86.32 ms] | 2.15 ms [2.13 ms, 2.16 ms] | 13.3 [13.2, 13.3] | – | same |
| explicit_tet4_ss | stresses | al-head | 7 | 40 | 85.62 ms [85.40 ms, 86.14 ms] | 2.14 ms [2.13 ms, 2.15 ms] | 13.2 [13.2, 13.3] | 0.996 [0.992, 1.003] | same |
| explicit_tet4_ss | stresses | base-t1 | 7 | 40 | 85.77 ms [85.51 ms, 86.59 ms] | 2.14 ms [2.14 ms, 2.16 ms] | 13.2 [13.2, 13.4] | 0.999 [0.993, 1.009] | same |
| explicit_tet4_ss | stresses | head2-t1 | 7 | 40 | 85.89 ms [85.45 ms, 86.40 ms] | 2.15 ms [2.14 ms, 2.16 ms] | 13.3 [13.2, 13.3] | 0.996 [0.992, 1.014] | same |
| explicit_tet4_ss | energies | al-base | 7 | 40 | 83.32 ms [83.22 ms, 83.59 ms] | 2.08 ms [2.08 ms, 2.09 ms] | 12.9 [12.8, 12.9] | – | same |
| explicit_tet4_ss | energies | al-head | 7 | 40 | 86.44 ms [86.26 ms, 86.59 ms] | 2.16 ms [2.16 ms, 2.16 ms] | 13.3 [13.3, 13.4] | 1.038 [1.033, 1.040] | same |
| explicit_tet4_ss | energies | base-t1 | 7 | 40 | 82.41 ms [82.25 ms, 82.71 ms] | 2.06 ms [2.06 ms, 2.07 ms] | 12.7 [12.7, 12.8] | 0.989 [0.986, 0.993] | same |
| explicit_tet4_ss | energies | head2-t1 | 7 | 40 | 86.38 ms [85.98 ms, 87.04 ms] | 2.16 ms [2.15 ms, 2.18 ms] | 13.3 [13.3, 13.4] | 1.034 [1.029, 1.044] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, c866d99ab | 017c8c9ef7659aa0 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, c866d99ab | ce8154043761dbc7 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42, c866d99ab | fecb3fb4b46c8b8c |
