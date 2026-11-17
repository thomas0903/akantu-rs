Suite `solid`, size `m`; variants: `al-base`, `al-head3`, `base-t1`, `head3-t1`, `head3-t8`, `prev-t8`; reference `al-base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to al-base | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars | mesh | al-base | 7 | 1 | 2.07 ms [2.05 ms, 2.11 ms] | 2.07 ms [2.05 ms, 2.11 ms] | 19.6 [19.5, 20.0] | – | same |
| explicit_mazars | mesh | al-head3 | 7 | 1 | 2.10 ms [2.07 ms, 2.13 ms] | 2.10 ms [2.07 ms, 2.13 ms] | 19.9 [19.6, 20.2] | 1.019 [0.984, 1.023] | same |
| explicit_mazars | mesh | base-t1 | 7 | 1 | 2.05 ms [2.01 ms, 2.18 ms] | 2.05 ms [2.01 ms, 2.18 ms] | 19.4 [19.1, 20.7] | 0.984 [0.963, 1.053] | same |
| explicit_mazars | mesh | head3-t1 | 7 | 1 | 2.09 ms [2.04 ms, 2.10 ms] | 2.09 ms [2.04 ms, 2.10 ms] | 19.8 [19.3, 19.9] | 0.998 [0.988, 1.016] | same |
| explicit_mazars | mesh | head3-t8 | 7 | 1 | 2.17 ms [2.05 ms, 2.20 ms] | 2.17 ms [2.05 ms, 2.20 ms] | 20.6 [19.4, 20.9] | 1.040 [0.989, 1.055] | same |
| explicit_mazars | mesh | prev-t8 | 7 | 1 | 2.11 ms [2.06 ms, 2.18 ms] | 2.11 ms [2.06 ms, 2.18 ms] | 20.0 [19.6, 20.7] | 1.023 [1.001, 1.054] | same |
| explicit_mazars | init | al-base | 7 | 1 | 32.81 ms [32.23 ms, 33.90 ms] | 32.81 ms [32.23 ms, 33.90 ms] | 311.1 [305.6, 321.5] | – | same |
| explicit_mazars | init | al-head3 | 7 | 1 | 32.56 ms [31.97 ms, 33.39 ms] | 32.56 ms [31.97 ms, 33.39 ms] | 308.8 [303.1, 316.7] | 0.988 [0.956, 1.032] | same |
| explicit_mazars | init | base-t1 | 7 | 1 | 33.12 ms [32.57 ms, 33.87 ms] | 33.12 ms [32.57 ms, 33.87 ms] | 314.1 [308.8, 321.2] | 1.007 [0.963, 1.047] | same |
| explicit_mazars | init | head3-t1 | 7 | 1 | 32.90 ms [32.33 ms, 33.82 ms] | 32.90 ms [32.33 ms, 33.82 ms] | 312.0 [306.6, 320.7] | 1.003 [0.965, 1.021] | same |
| explicit_mazars | init | head3-t8 | 7 | 1 | 34.09 ms [32.72 ms, 34.47 ms] | 34.09 ms [32.72 ms, 34.47 ms] | 323.2 [310.3, 326.9] | 1.037 [0.982, 1.061] | same |
| explicit_mazars | init | prev-t8 | 7 | 1 | 33.58 ms [32.94 ms, 34.46 ms] | 33.58 ms [32.94 ms, 34.46 ms] | 318.4 [312.3, 326.7] | 1.018 [0.988, 1.065] | same |
| explicit_mazars | stable_time_step | al-base | 7 | 5 | 20.99 ms [20.95 ms, 21.04 ms] | 4.20 ms [4.19 ms, 4.21 ms] | 39.8 [39.7, 39.9] | – | same |
| explicit_mazars | stable_time_step | al-head3 | 7 | 5 | 21.14 ms [21.05 ms, 21.58 ms] | 4.23 ms [4.21 ms, 4.32 ms] | 40.1 [39.9, 40.9] | 1.008 [1.002, 1.030] | same |
| explicit_mazars | stable_time_step | base-t1 | 7 | 5 | 21.07 ms [20.96 ms, 21.26 ms] | 4.21 ms [4.19 ms, 4.25 ms] | 40.0 [39.7, 40.3] | 1.004 [0.998, 1.011] | same |
| explicit_mazars | stable_time_step | head3-t1 | 7 | 5 | 21.16 ms [21.09 ms, 21.68 ms] | 4.23 ms [4.22 ms, 4.34 ms] | 40.1 [40.0, 41.1] | 1.009 [1.003, 1.033] | same |
| explicit_mazars | stable_time_step | head3-t8 | 7 | 5 | 21.20 ms [21.11 ms, 21.25 ms] | 4.24 ms [4.22 ms, 4.25 ms] | 40.2 [40.0, 40.3] | 1.011 [1.006, 1.014] | same |
| explicit_mazars | stable_time_step | prev-t8 | 7 | 5 | 21.26 ms [21.21 ms, 21.68 ms] | 4.25 ms [4.24 ms, 4.34 ms] | 40.3 [40.2, 41.1] | 1.012 [1.010, 1.035] | same |
| explicit_mazars | solve_step | al-base | 7 | 80 | 2.689 s [2.683 s, 2.713 s] | 33.62 ms [33.54 ms, 33.91 ms] | 318.8 [318.1, 321.5] | – | same |
| explicit_mazars | solve_step | al-head3 | 7 | 80 | 2.702 s [2.693 s, 2.707 s] | 33.78 ms [33.66 ms, 33.83 ms] | 320.3 [319.2, 320.8] | 1.003 [0.996, 1.007] | same |
| explicit_mazars | solve_step | base-t1 | 7 | 80 | 2.682 s [2.675 s, 2.692 s] | 33.53 ms [33.44 ms, 33.66 ms] | 318.0 [317.1, 319.1] | 0.996 [0.987, 1.001] | same |
| explicit_mazars | solve_step | head3-t1 | 7 | 80 | 2.725 s [2.689 s, 2.782 s] | 34.06 ms [33.61 ms, 34.77 ms] | 323.0 [318.7, 329.7] | 1.010 [0.991, 1.034] | same |
| explicit_mazars | solve_step | head3-t8 | 7 | 80 | 583.55 ms [578.08 ms, 587.91 ms] | 7.29 ms [7.23 ms, 7.35 ms] | 69.2 [68.5, 69.7] | 0.216 [0.215, 0.219] | same |
| explicit_mazars | solve_step | prev-t8 | 7 | 80 | 577.91 ms [575.76 ms, 579.84 ms] | 7.22 ms [7.20 ms, 7.25 ms] | 68.5 [68.2, 68.7] | 0.215 [0.212, 0.216] | same |
| explicit_mazars | predictor | al-base | 7 | 80 | 8.8 µs [6.6 µs, 10.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | al-head3 | 7 | 80 | 12.5 µs [10.3 µs, 14.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.367 [1.092, 1.861] | same |
| explicit_mazars | predictor | base-t1 | 7 | 80 | 8.3 µs [5.5 µs, 11.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.934 [0.612, 1.596] | same |
| explicit_mazars | predictor | head3-t1 | 7 | 80 | 13.3 µs [11.2 µs, 16.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.510 [1.148, 2.025] | same |
| explicit_mazars | predictor | head3-t8 | 7 | 80 | 15.0 µs [11.3 µs, 18.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.637 [1.236, 2.744] | same |
| explicit_mazars | predictor | prev-t8 | 7 | 80 | 9.1 µs [2.4 µs, 11.4 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.987 [0.264, 1.725] | same |
| explicit_mazars | residual | al-base | 7 | 80 | 2.526 s [2.519 s, 2.549 s] | 31.57 ms [31.49 ms, 31.87 ms] | 299.4 [298.6, 302.2] | – | same |
| explicit_mazars | residual | al-head3 | 7 | 80 | 2.538 s [2.529 s, 2.543 s] | 31.72 ms [31.61 ms, 31.79 ms] | 300.8 [299.8, 301.5] | 1.002 [0.996, 1.007] | same |
| explicit_mazars | residual | base-t1 | 7 | 80 | 2.519 s [2.514 s, 2.530 s] | 31.49 ms [31.42 ms, 31.63 ms] | 298.6 [298.0, 299.9] | 0.996 [0.987, 1.002] | same |
| explicit_mazars | residual | head3-t1 | 7 | 80 | 2.561 s [2.524 s, 2.619 s] | 32.01 ms [31.55 ms, 32.74 ms] | 303.5 [299.2, 310.5] | 1.010 [0.990, 1.037] | same |
| explicit_mazars | residual | head3-t8 | 7 | 80 | 508.25 ms [503.24 ms, 512.00 ms] | 6.35 ms [6.29 ms, 6.40 ms] | 60.2 [59.6, 60.7] | 0.200 [0.199, 0.203] | same |
| explicit_mazars | residual | prev-t8 | 7 | 80 | 502.82 ms [500.24 ms, 503.98 ms] | 6.29 ms [6.25 ms, 6.30 ms] | 59.6 [59.3, 59.7] | 0.199 [0.196, 0.200] | same |
| explicit_mazars | lumped_matrix | al-base | 7 | 80 | 19.4 µs [15.4 µs, 20.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | al-head3 | 7 | 80 | 21.3 µs [17.1 µs, 23.1 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.141 [0.878, 1.490] | same |
| explicit_mazars | lumped_matrix | base-t1 | 7 | 80 | 18.2 µs [16.7 µs, 21.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.081 [0.913, 1.240] | same |
| explicit_mazars | lumped_matrix | head3-t1 | 7 | 80 | 20.8 µs [16.7 µs, 31.9 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.182 [0.862, 1.639] | same |
| explicit_mazars | lumped_matrix | head3-t8 | 7 | 80 | 21.6 µs [20.4 µs, 37.2 µs] | 0.3 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.117 [1.070, 2.414] | same |
| explicit_mazars | lumped_matrix | prev-t8 | 7 | 80 | 15.8 µs [14.5 µs, 20.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.933 [0.784, 1.046] | same |
| explicit_mazars | corrector | al-base | 7 | 80 | 13.9 µs [13.0 µs, 22.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | al-head3 | 7 | 80 | 21.3 µs [14.9 µs, 21.7 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.486 [0.972, 1.541] | same |
| explicit_mazars | corrector | base-t1 | 7 | 80 | 12.7 µs [11.7 µs, 13.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.916 [0.526, 1.029] | same |
| explicit_mazars | corrector | head3-t1 | 7 | 80 | 21.2 µs [14.9 µs, 22.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.475 [0.980, 1.662] | same |
| explicit_mazars | corrector | head3-t8 | 7 | 80 | 18.3 µs [13.8 µs, 39.6 µs] | 0.2 µs [0.2 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.133 [0.619, 2.812] | same |
| explicit_mazars | corrector | prev-t8 | 7 | 80 | 19.6 µs [16.6 µs, 22.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.342 [0.848, 1.602] | same |
| explicit_mazars | step_hooks | al-base | 7 | 160 | 133.33 ms [132.82 ms, 133.49 ms] | 833.3 µs [830.1 µs, 834.3 µs] | 7.9 [7.9, 7.9] | – | same |
| explicit_mazars | step_hooks | al-head3 | 7 | 160 | 133.68 ms [133.29 ms, 134.62 ms] | 835.5 µs [833.1 µs, 841.4 µs] | 7.9 [7.9, 8.0] | 1.006 [0.999, 1.011] | same |
| explicit_mazars | step_hooks | base-t1 | 7 | 160 | 132.87 ms [131.71 ms, 133.56 ms] | 830.5 µs [823.2 µs, 834.8 µs] | 7.9 [7.8, 7.9] | 0.998 [0.988, 1.004] | same |
| explicit_mazars | step_hooks | head3-t1 | 7 | 160 | 134.13 ms [132.82 ms, 134.91 ms] | 838.3 µs [830.1 µs, 843.2 µs] | 7.9 [7.9, 8.0] | 1.005 [0.997, 1.012] | same |
| explicit_mazars | step_hooks | head3-t8 | 7 | 160 | 56.59 ms [55.74 ms, 56.72 ms] | 353.7 µs [348.4 µs, 354.5 µs] | 3.4 [3.3, 3.4] | 0.425 [0.418, 0.426] | same |
| explicit_mazars | step_hooks | prev-t8 | 7 | 160 | 56.41 ms [55.77 ms, 56.90 ms] | 352.5 µs [348.6 µs, 355.6 µs] | 3.3 [3.3, 3.4] | 0.423 [0.418, 0.427] | same |
| explicit_mazars | step_rest | al-base | 7 | 80 | 30.01 ms [29.45 ms, 30.21 ms] | 375.1 µs [368.2 µs, 377.6 µs] | 3.6 [3.5, 3.6] | – | same |
| explicit_mazars | step_rest | al-head3 | 7 | 80 | 29.63 ms [29.33 ms, 30.44 ms] | 370.4 µs [366.6 µs, 380.5 µs] | 3.5 [3.5, 3.6] | 0.994 [0.976, 1.009] | same |
| explicit_mazars | step_rest | base-t1 | 7 | 80 | 29.61 ms [29.18 ms, 29.78 ms] | 370.1 µs [364.7 µs, 372.2 µs] | 3.5 [3.5, 3.5] | 0.992 [0.970, 1.007] | same |
| explicit_mazars | step_rest | head3-t1 | 7 | 80 | 29.15 ms [29.09 ms, 29.42 ms] | 364.4 µs [363.6 µs, 367.7 µs] | 3.5 [3.4, 3.5] | 0.973 [0.963, 0.999] | same |
| explicit_mazars | step_rest | head3-t8 | 7 | 80 | 18.53 ms [18.36 ms, 18.71 ms] | 231.6 µs [229.5 µs, 233.9 µs] | 2.2 [2.2, 2.2] | 0.623 [0.608, 0.630] | same |
| explicit_mazars | step_rest | prev-t8 | 7 | 80 | 18.59 ms [18.26 ms, 18.91 ms] | 232.4 µs [228.2 µs, 236.3 µs] | 2.2 [2.2, 2.2] | 0.623 [0.615, 0.639] | same |
| explicit_mazars | internal_forces | al-base | 7 | 16 | 545.76 ms [542.71 ms, 546.58 ms] | 34.11 ms [33.92 ms, 34.16 ms] | 323.4 [321.6, 323.9] | – | same |
| explicit_mazars | internal_forces | al-head3 | 7 | 16 | 546.34 ms [545.82 ms, 549.61 ms] | 34.15 ms [34.11 ms, 34.35 ms] | 323.8 [323.5, 325.7] | 1.001 [1.000, 1.013] | same |
| explicit_mazars | internal_forces | base-t1 | 7 | 16 | 543.07 ms [541.71 ms, 545.48 ms] | 33.94 ms [33.86 ms, 34.09 ms] | 321.9 [321.1, 323.3] | 0.994 [0.993, 1.005] | same |
| explicit_mazars | internal_forces | head3-t1 | 7 | 16 | 547.77 ms [544.10 ms, 551.63 ms] | 34.24 ms [34.01 ms, 34.48 ms] | 324.6 [322.5, 326.9] | 1.008 [1.000, 1.013] | same |
| explicit_mazars | internal_forces | head3-t8 | 7 | 16 | 102.35 ms [101.67 ms, 104.17 ms] | 6.40 ms [6.35 ms, 6.51 ms] | 60.7 [60.3, 61.7] | 0.187 [0.186, 0.192] | same |
| explicit_mazars | internal_forces | prev-t8 | 7 | 16 | 101.49 ms [100.44 ms, 102.72 ms] | 6.34 ms [6.28 ms, 6.42 ms] | 60.1 [59.5, 60.9] | 0.186 [0.185, 0.189] | same |
| explicit_mazars | stresses | al-base | 7 | 16 | 508.96 ms [504.25 ms, 512.47 ms] | 31.81 ms [31.52 ms, 32.03 ms] | 301.6 [298.8, 303.7] | – | same |
| explicit_mazars | stresses | al-head3 | 7 | 16 | 512.53 ms [510.04 ms, 513.37 ms] | 32.03 ms [31.88 ms, 32.09 ms] | 303.8 [302.3, 304.3] | 1.002 [0.997, 1.017] | same |
| explicit_mazars | stresses | base-t1 | 7 | 16 | 506.58 ms [506.34 ms, 509.76 ms] | 31.66 ms [31.65 ms, 31.86 ms] | 300.2 [300.1, 302.1] | 0.997 [0.988, 1.004] | same |
| explicit_mazars | stresses | head3-t1 | 7 | 16 | 515.83 ms [506.25 ms, 523.43 ms] | 32.24 ms [31.64 ms, 32.71 ms] | 305.7 [300.0, 310.2] | 1.010 [0.988, 1.031] | same |
| explicit_mazars | stresses | head3-t8 | 7 | 16 | 502.06 ms [498.48 ms, 510.20 ms] | 31.38 ms [31.15 ms, 31.89 ms] | 297.6 [295.4, 302.4] | 0.988 [0.974, 0.996] | same |
| explicit_mazars | stresses | prev-t8 | 7 | 16 | 501.13 ms [495.93 ms, 503.00 ms] | 31.32 ms [31.00 ms, 31.44 ms] | 297.0 [293.9, 298.1] | 0.984 [0.968, 0.996] | same |
| explicit_mazars | energies | al-base | 7 | 16 | 23.55 ms [23.12 ms, 23.97 ms] | 1.47 ms [1.45 ms, 1.50 ms] | 14.0 [13.7, 14.2] | – | same |
| explicit_mazars | energies | al-head3 | 7 | 16 | 24.33 ms [24.00 ms, 24.60 ms] | 1.52 ms [1.50 ms, 1.54 ms] | 14.4 [14.2, 14.6] | 1.034 [1.019, 1.052] | same |
| explicit_mazars | energies | base-t1 | 7 | 16 | 22.64 ms [22.40 ms, 23.04 ms] | 1.41 ms [1.40 ms, 1.44 ms] | 13.4 [13.3, 13.7] | 0.968 [0.940, 0.993] | same |
| explicit_mazars | energies | head3-t1 | 7 | 16 | 23.72 ms [23.56 ms, 24.21 ms] | 1.48 ms [1.47 ms, 1.51 ms] | 14.1 [14.0, 14.3] | 1.012 [0.990, 1.034] | same |
| explicit_mazars | energies | head3-t8 | 7 | 16 | 18.21 ms [17.89 ms, 18.71 ms] | 1.14 ms [1.12 ms, 1.17 ms] | 10.8 [10.6, 11.1] | 0.769 [0.759, 0.799] | same |
| explicit_mazars | energies | prev-t8 | 7 | 16 | 18.02 ms [17.95 ms, 18.46 ms] | 1.13 ms [1.12 ms, 1.15 ms] | 10.7 [10.6, 10.9] | 0.770 [0.754, 0.787] | same |
| explicit_multi_material | mesh | al-base | 7 | 1 | 2.12 ms [2.05 ms, 2.17 ms] | 2.12 ms [2.05 ms, 2.17 ms] | 20.1 [19.5, 20.6] | – | same |
| explicit_multi_material | mesh | al-head3 | 7 | 1 | 2.14 ms [2.07 ms, 2.16 ms] | 2.14 ms [2.07 ms, 2.16 ms] | 20.3 [19.6, 20.5] | 1.002 [0.980, 1.053] | same |
| explicit_multi_material | mesh | base-t1 | 7 | 1 | 2.15 ms [2.04 ms, 2.27 ms] | 2.15 ms [2.04 ms, 2.27 ms] | 20.4 [19.4, 21.5] | 1.021 [0.965, 1.050] | same |
| explicit_multi_material | mesh | head3-t1 | 7 | 1 | 2.13 ms [2.06 ms, 2.16 ms] | 2.13 ms [2.06 ms, 2.16 ms] | 20.2 [19.5, 20.5] | 0.983 [0.972, 1.052] | same |
| explicit_multi_material | mesh | head3-t8 | 7 | 1 | 2.20 ms [2.06 ms, 2.27 ms] | 2.20 ms [2.06 ms, 2.27 ms] | 20.9 [19.6, 21.5] | 1.020 [0.973, 1.107] | same |
| explicit_multi_material | mesh | prev-t8 | 7 | 1 | 2.17 ms [2.04 ms, 2.20 ms] | 2.17 ms [2.04 ms, 2.20 ms] | 20.6 [19.3, 20.9] | 1.024 [0.958, 1.040] | same |
| explicit_multi_material | init | al-base | 7 | 1 | 33.94 ms [33.57 ms, 34.75 ms] | 33.94 ms [33.57 ms, 34.75 ms] | 321.9 [318.3, 329.5] | – | same |
| explicit_multi_material | init | al-head3 | 7 | 1 | 34.06 ms [33.02 ms, 34.45 ms] | 34.06 ms [33.02 ms, 34.45 ms] | 323.0 [313.1, 326.6] | 0.996 [0.967, 1.021] | same |
| explicit_multi_material | init | base-t1 | 7 | 1 | 34.56 ms [33.91 ms, 34.97 ms] | 34.56 ms [33.91 ms, 34.97 ms] | 327.7 [321.6, 331.6] | 1.018 [0.989, 1.026] | same |
| explicit_multi_material | init | head3-t1 | 7 | 1 | 33.77 ms [33.55 ms, 33.89 ms] | 33.77 ms [33.55 ms, 33.89 ms] | 320.2 [318.1, 321.3] | 0.994 [0.972, 1.008] | same |
| explicit_multi_material | init | head3-t8 | 7 | 1 | 34.79 ms [34.50 ms, 35.40 ms] | 34.79 ms [34.50 ms, 35.40 ms] | 329.9 [327.1, 335.7] | 1.026 [1.009, 1.050] | same |
| explicit_multi_material | init | prev-t8 | 7 | 1 | 35.32 ms [34.41 ms, 35.39 ms] | 35.32 ms [34.41 ms, 35.39 ms] | 334.9 [326.3, 335.6] | 1.035 [0.990, 1.053] | same |
| explicit_multi_material | stable_time_step | al-base | 7 | 5 | 21.42 ms [21.34 ms, 21.75 ms] | 4.28 ms [4.27 ms, 4.35 ms] | 40.6 [40.5, 41.2] | – | same |
| explicit_multi_material | stable_time_step | al-head3 | 7 | 5 | 21.24 ms [21.17 ms, 21.56 ms] | 4.25 ms [4.23 ms, 4.31 ms] | 40.3 [40.1, 40.9] | 0.989 [0.973, 1.008] | same |
| explicit_multi_material | stable_time_step | base-t1 | 7 | 5 | 21.12 ms [21.05 ms, 21.59 ms] | 4.22 ms [4.21 ms, 4.32 ms] | 40.0 [39.9, 40.9] | 0.987 [0.982, 1.008] | same |
| explicit_multi_material | stable_time_step | head3-t1 | 7 | 5 | 21.19 ms [21.12 ms, 21.29 ms] | 4.24 ms [4.22 ms, 4.26 ms] | 40.2 [40.1, 40.4] | 0.991 [0.974, 0.995] | same |
| explicit_multi_material | stable_time_step | head3-t8 | 7 | 5 | 21.19 ms [21.13 ms, 21.33 ms] | 4.24 ms [4.23 ms, 4.27 ms] | 40.2 [40.1, 40.5] | 0.990 [0.974, 0.994] | same |
| explicit_multi_material | stable_time_step | prev-t8 | 7 | 5 | 21.35 ms [21.29 ms, 21.78 ms] | 4.27 ms [4.26 ms, 4.36 ms] | 40.5 [40.4, 41.3] | 0.998 [0.981, 1.014] | same |
| explicit_multi_material | solve_step | al-base | 7 | 120 | 2.007 s [2.000 s, 2.070 s] | 16.72 ms [16.67 ms, 17.25 ms] | 158.6 [158.0, 163.6] | – | same |
| explicit_multi_material | solve_step | al-head3 | 7 | 120 | 2.011 s [1.999 s, 2.016 s] | 16.76 ms [16.65 ms, 16.80 ms] | 158.9 [157.9, 159.3] | 1.000 [0.968, 1.007] | same |
| explicit_multi_material | solve_step | base-t1 | 7 | 120 | 2.005 s [1.981 s, 2.007 s] | 16.71 ms [16.51 ms, 16.72 ms] | 158.5 [156.5, 158.6] | 1.000 [0.969, 1.003] | same |
| explicit_multi_material | solve_step | head3-t1 | 7 | 120 | 2.014 s [2.007 s, 2.020 s] | 16.79 ms [16.73 ms, 16.83 ms] | 159.2 [158.6, 159.6] | 1.003 [0.976, 1.010] | same |
| explicit_multi_material | solve_step | head3-t8 | 7 | 120 | 458.57 ms [455.79 ms, 462.21 ms] | 3.82 ms [3.80 ms, 3.85 ms] | 36.2 [36.0, 36.5] | 0.228 [0.222, 0.230] | same |
| explicit_multi_material | solve_step | prev-t8 | 7 | 120 | 459.48 ms [454.23 ms, 460.85 ms] | 3.83 ms [3.79 ms, 3.84 ms] | 36.3 [35.9, 36.4] | 0.229 [0.222, 0.230] | same |
| explicit_multi_material | predictor | al-base | 7 | 120 | 10.9 µs [8.3 µs, 11.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | predictor | al-head3 | 7 | 120 | 12.6 µs [9.3 µs, 18.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.352 [0.853, 1.712] | same |
| explicit_multi_material | predictor | base-t1 | 7 | 120 | 9.1 µs [6.6 µs, 14.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.925 [0.598, 1.286] | same |
| explicit_multi_material | predictor | head3-t1 | 7 | 120 | 17.5 µs [16.6 µs, 22.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.616 [1.497, 2.193] | same |
| explicit_multi_material | predictor | head3-t8 | 7 | 120 | 20.4 µs [12.4 µs, 26.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.113 [1.133, 2.463] | same |
| explicit_multi_material | predictor | prev-t8 | 7 | 120 | 4.0 µs [3.5 µs, 10.1 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.394 [0.321, 0.953] | same |
| explicit_multi_material | residual | al-base | 7 | 120 | 1.871 s [1.865 s, 1.935 s] | 15.59 ms [15.54 ms, 16.13 ms] | 147.9 [147.4, 152.9] | – | same |
| explicit_multi_material | residual | al-head3 | 7 | 120 | 1.875 s [1.863 s, 1.879 s] | 15.63 ms [15.53 ms, 15.66 ms] | 148.2 [147.2, 148.5] | 1.000 [0.965, 1.007] | same |
| explicit_multi_material | residual | base-t1 | 7 | 120 | 1.869 s [1.846 s, 1.872 s] | 15.57 ms [15.39 ms, 15.60 ms] | 147.7 [145.9, 147.9] | 1.000 [0.967, 1.002] | same |
| explicit_multi_material | residual | head3-t1 | 7 | 120 | 1.877 s [1.872 s, 1.884 s] | 15.64 ms [15.60 ms, 15.70 ms] | 148.3 [147.9, 148.9] | 1.003 [0.974, 1.010] | same |
| explicit_multi_material | residual | head3-t8 | 7 | 120 | 392.55 ms [390.23 ms, 396.03 ms] | 3.27 ms [3.25 ms, 3.30 ms] | 31.0 [30.8, 31.3] | 0.209 [0.203, 0.212] | same |
| explicit_multi_material | residual | prev-t8 | 7 | 120 | 394.02 ms [388.45 ms, 394.82 ms] | 3.28 ms [3.24 ms, 3.29 ms] | 31.1 [30.7, 31.2] | 0.210 [0.203, 0.212] | same |
| explicit_multi_material | lumped_matrix | al-base | 7 | 120 | 27.9 µs [25.0 µs, 31.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | lumped_matrix | al-head3 | 7 | 120 | 30.5 µs [29.1 µs, 34.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.137 [0.941, 1.229] | same |
| explicit_multi_material | lumped_matrix | base-t1 | 7 | 120 | 27.0 µs [25.9 µs, 33.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.967 [0.922, 1.296] | same |
| explicit_multi_material | lumped_matrix | head3-t1 | 7 | 120 | 31.1 µs [29.3 µs, 35.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.080 [0.980, 1.338] | same |
| explicit_multi_material | lumped_matrix | head3-t8 | 7 | 120 | 40.9 µs [32.8 µs, 45.8 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.438 [1.128, 1.832] | same |
| explicit_multi_material | lumped_matrix | prev-t8 | 7 | 120 | 29.9 µs [26.8 µs, 33.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.118 [0.884, 1.351] | same |
| explicit_multi_material | corrector | al-base | 7 | 120 | 17.6 µs [16.9 µs, 20.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | corrector | al-head3 | 7 | 120 | 18.9 µs [14.0 µs, 20.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.073 [0.693, 1.122] | same |
| explicit_multi_material | corrector | base-t1 | 7 | 120 | 18.5 µs [14.8 µs, 24.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.068 [0.814, 1.362] | same |
| explicit_multi_material | corrector | head3-t1 | 7 | 120 | 18.4 µs [16.0 µs, 19.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.042 [0.791, 1.171] | same |
| explicit_multi_material | corrector | head3-t8 | 7 | 120 | 17.8 µs [14.1 µs, 21.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.004 [0.765, 1.069] | same |
| explicit_multi_material | corrector | prev-t8 | 7 | 120 | 15.5 µs [9.6 µs, 16.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.766 [0.571, 0.970] | same |
| explicit_multi_material | step_hooks | al-base | 7 | 240 | 94.50 ms [94.02 ms, 94.91 ms] | 393.8 µs [391.8 µs, 395.5 µs] | 3.7 [3.7, 3.8] | – | same |
| explicit_multi_material | step_hooks | al-head3 | 7 | 240 | 94.86 ms [94.61 ms, 95.60 ms] | 395.3 µs [394.2 µs, 398.3 µs] | 3.7 [3.7, 3.8] | 1.007 [0.998, 1.011] | same |
| explicit_multi_material | step_hooks | base-t1 | 7 | 240 | 94.69 ms [94.34 ms, 95.25 ms] | 394.5 µs [393.1 µs, 396.9 µs] | 3.7 [3.7, 3.8] | 0.999 [0.994, 1.012] | same |
| explicit_multi_material | step_hooks | head3-t1 | 7 | 240 | 94.86 ms [94.21 ms, 95.78 ms] | 395.3 µs [392.5 µs, 399.1 µs] | 3.7 [3.7, 3.8] | 1.004 [0.997, 1.017] | same |
| explicit_multi_material | step_hooks | head3-t8 | 7 | 240 | 44.04 ms [43.81 ms, 44.11 ms] | 183.5 µs [182.5 µs, 183.8 µs] | 1.7 [1.7, 1.7] | 0.465 [0.464, 0.469] | same |
| explicit_multi_material | step_hooks | prev-t8 | 7 | 240 | 43.85 ms [42.67 ms, 44.39 ms] | 182.7 µs [177.8 µs, 185.0 µs] | 1.7 [1.7, 1.8] | 0.465 [0.450, 0.469] | same |
| explicit_multi_material | step_rest | al-base | 7 | 120 | 40.51 ms [40.27 ms, 41.02 ms] | 337.6 µs [335.5 µs, 341.8 µs] | 3.2 [3.2, 3.2] | – | same |
| explicit_multi_material | step_rest | al-head3 | 7 | 120 | 40.82 ms [40.32 ms, 41.30 ms] | 340.1 µs [336.0 µs, 344.1 µs] | 3.2 [3.2, 3.3] | 1.007 [0.988, 1.022] | same |
| explicit_multi_material | step_rest | base-t1 | 7 | 120 | 40.32 ms [40.03 ms, 40.67 ms] | 336.0 µs [333.6 µs, 338.9 µs] | 3.2 [3.2, 3.2] | 0.991 [0.976, 1.008] | same |
| explicit_multi_material | step_rest | head3-t1 | 7 | 120 | 40.13 ms [39.57 ms, 40.82 ms] | 334.4 µs [329.8 µs, 340.1 µs] | 3.2 [3.1, 3.2] | 0.993 [0.965, 1.002] | same |
| explicit_multi_material | step_rest | head3-t8 | 7 | 120 | 21.47 ms [21.30 ms, 21.71 ms] | 178.9 µs [177.5 µs, 180.9 µs] | 1.7 [1.7, 1.7] | 0.531 [0.520, 0.534] | same |
| explicit_multi_material | step_rest | prev-t8 | 7 | 120 | 21.73 ms [21.45 ms, 21.99 ms] | 181.1 µs [178.8 µs, 183.2 µs] | 1.7 [1.7, 1.7] | 0.536 [0.524, 0.540] | same |
| explicit_multi_material | internal_forces | al-base | 7 | 24 | 373.31 ms [371.93 ms, 374.66 ms] | 15.55 ms [15.50 ms, 15.61 ms] | 147.5 [147.0, 148.0] | – | same |
| explicit_multi_material | internal_forces | al-head3 | 7 | 24 | 373.73 ms [371.80 ms, 375.70 ms] | 15.57 ms [15.49 ms, 15.65 ms] | 147.7 [146.9, 148.4] | 1.001 [0.997, 1.006] | same |
| explicit_multi_material | internal_forces | base-t1 | 7 | 24 | 374.29 ms [372.17 ms, 375.86 ms] | 15.60 ms [15.51 ms, 15.66 ms] | 147.9 [147.0, 148.5] | 1.005 [0.995, 1.007] | same |
| explicit_multi_material | internal_forces | head3-t1 | 7 | 24 | 373.64 ms [372.03 ms, 375.78 ms] | 15.57 ms [15.50 ms, 15.66 ms] | 147.6 [147.0, 148.5] | 1.002 [0.997, 1.004] | same |
| explicit_multi_material | internal_forces | head3-t8 | 7 | 24 | 70.86 ms [69.86 ms, 71.20 ms] | 2.95 ms [2.91 ms, 2.97 ms] | 28.0 [27.6, 28.1] | 0.190 [0.187, 0.191] | same |
| explicit_multi_material | internal_forces | prev-t8 | 7 | 24 | 70.90 ms [69.77 ms, 71.31 ms] | 2.95 ms [2.91 ms, 2.97 ms] | 28.0 [27.6, 28.2] | 0.190 [0.186, 0.191] | same |
| explicit_multi_material | stresses | al-base | 7 | 24 | 314.55 ms [313.85 ms, 316.30 ms] | 13.11 ms [13.08 ms, 13.18 ms] | 124.3 [124.0, 125.0] | – | same |
| explicit_multi_material | stresses | al-head3 | 7 | 24 | 316.52 ms [314.22 ms, 317.01 ms] | 13.19 ms [13.09 ms, 13.21 ms] | 125.1 [124.2, 125.3] | 1.001 [0.998, 1.010] | same |
| explicit_multi_material | stresses | base-t1 | 7 | 24 | 316.76 ms [313.50 ms, 317.23 ms] | 13.20 ms [13.06 ms, 13.22 ms] | 125.2 [123.9, 125.3] | 1.005 [0.995, 1.010] | same |
| explicit_multi_material | stresses | head3-t1 | 7 | 24 | 317.06 ms [312.69 ms, 318.74 ms] | 13.21 ms [13.03 ms, 13.28 ms] | 125.3 [123.5, 125.9] | 1.006 [0.993, 1.011] | same |
| explicit_multi_material | stresses | head3-t8 | 7 | 24 | 290.62 ms [287.50 ms, 292.08 ms] | 12.11 ms [11.98 ms, 12.17 ms] | 114.8 [113.6, 115.4] | 0.925 [0.912, 0.931] | same |
| explicit_multi_material | stresses | prev-t8 | 7 | 24 | 292.26 ms [290.30 ms, 295.45 ms] | 12.18 ms [12.10 ms, 12.31 ms] | 115.5 [114.7, 116.7] | 0.929 [0.918, 0.940] | same |
| explicit_multi_material | energies | al-base | 7 | 24 | 37.88 ms [37.66 ms, 38.39 ms] | 1.58 ms [1.57 ms, 1.60 ms] | 15.0 [14.9, 15.2] | – | same |
| explicit_multi_material | energies | al-head3 | 7 | 24 | 38.89 ms [38.80 ms, 39.14 ms] | 1.62 ms [1.62 ms, 1.63 ms] | 15.4 [15.3, 15.5] | 1.025 [1.013, 1.036] | same |
| explicit_multi_material | energies | base-t1 | 7 | 24 | 37.48 ms [37.18 ms, 38.20 ms] | 1.56 ms [1.55 ms, 1.59 ms] | 14.8 [14.7, 15.1] | 0.986 [0.969, 1.008] | same |
| explicit_multi_material | energies | head3-t1 | 7 | 24 | 38.60 ms [38.49 ms, 39.04 ms] | 1.61 ms [1.60 ms, 1.63 ms] | 15.3 [15.2, 15.4] | 1.019 [1.005, 1.037] | same |
| explicit_multi_material | energies | head3-t8 | 7 | 24 | 35.56 ms [35.34 ms, 35.78 ms] | 1.48 ms [1.47 ms, 1.49 ms] | 14.0 [14.0, 14.1] | 0.935 [0.928, 0.948] | same |
| explicit_multi_material | energies | prev-t8 | 7 | 24 | 35.20 ms [35.04 ms, 35.53 ms] | 1.47 ms [1.46 ms, 1.48 ms] | 13.9 [13.8, 14.0] | 0.930 [0.914, 0.933] | same |
| explicit_plastic | mesh | al-base | 7 | 1 | 2.09 ms [2.06 ms, 2.21 ms] | 2.09 ms [2.06 ms, 2.21 ms] | 19.8 [19.6, 20.9] | – | same |
| explicit_plastic | mesh | al-head3 | 7 | 1 | 2.10 ms [2.08 ms, 2.19 ms] | 2.10 ms [2.08 ms, 2.19 ms] | 19.9 [19.7, 20.7] | 1.014 [0.941, 1.046] | same |
| explicit_plastic | mesh | base-t1 | 7 | 1 | 2.18 ms [2.16 ms, 2.26 ms] | 2.18 ms [2.16 ms, 2.26 ms] | 20.7 [20.5, 21.4] | 1.046 [1.011, 1.083] | same |
| explicit_plastic | mesh | head3-t1 | 7 | 1 | 2.06 ms [2.06 ms, 2.11 ms] | 2.06 ms [2.06 ms, 2.11 ms] | 19.5 [19.5, 20.0] | 0.994 [0.958, 1.015] | same |
| explicit_plastic | mesh | head3-t8 | 7 | 1 | 2.24 ms [2.10 ms, 2.27 ms] | 2.24 ms [2.10 ms, 2.27 ms] | 21.2 [19.9, 21.5] | 1.038 [1.004, 1.087] | same |
| explicit_plastic | mesh | prev-t8 | 7 | 1 | 2.17 ms [2.06 ms, 2.19 ms] | 2.17 ms [2.06 ms, 2.19 ms] | 20.5 [19.5, 20.8] | 1.036 [0.970, 1.063] | same |
| explicit_plastic | init | al-base | 7 | 1 | 34.62 ms [34.25 ms, 35.59 ms] | 34.62 ms [34.25 ms, 35.59 ms] | 328.3 [324.8, 337.5] | – | same |
| explicit_plastic | init | al-head3 | 7 | 1 | 34.31 ms [33.64 ms, 35.04 ms] | 34.31 ms [33.64 ms, 35.04 ms] | 325.4 [319.0, 332.2] | 0.986 [0.977, 1.021] | same |
| explicit_plastic | init | base-t1 | 7 | 1 | 34.96 ms [34.81 ms, 35.47 ms] | 34.96 ms [34.81 ms, 35.47 ms] | 331.5 [330.1, 336.3] | 1.017 [0.984, 1.024] | same |
| explicit_plastic | init | head3-t1 | 7 | 1 | 34.14 ms [33.78 ms, 35.43 ms] | 34.14 ms [33.78 ms, 35.43 ms] | 323.8 [320.4, 336.0] | 0.991 [0.972, 1.027] | same |
| explicit_plastic | init | head3-t8 | 7 | 1 | 34.97 ms [34.43 ms, 36.71 ms] | 34.97 ms [34.43 ms, 36.71 ms] | 331.6 [326.5, 348.1] | 1.008 [0.994, 1.060] | same |
| explicit_plastic | init | prev-t8 | 7 | 1 | 35.09 ms [34.30 ms, 35.64 ms] | 35.09 ms [34.30 ms, 35.64 ms] | 332.8 [325.2, 338.0] | 1.013 [0.984, 1.039] | same |
| explicit_plastic | stable_time_step | al-base | 7 | 5 | 20.99 ms [20.95 ms, 21.01 ms] | 4.20 ms [4.19 ms, 4.20 ms] | 39.8 [39.7, 39.9] | – | same |
| explicit_plastic | stable_time_step | al-head3 | 7 | 5 | 21.16 ms [21.13 ms, 21.18 ms] | 4.23 ms [4.23 ms, 4.24 ms] | 40.1 [40.1, 40.2] | 1.009 [1.007, 1.009] | same |
| explicit_plastic | stable_time_step | base-t1 | 7 | 5 | 21.12 ms [20.98 ms, 21.36 ms] | 4.22 ms [4.20 ms, 4.27 ms] | 40.0 [39.8, 40.5] | 1.006 [1.000, 1.019] | same |
| explicit_plastic | stable_time_step | head3-t1 | 7 | 5 | 21.14 ms [21.08 ms, 21.19 ms] | 4.23 ms [4.22 ms, 4.24 ms] | 40.1 [40.0, 40.2] | 1.007 [1.004, 1.010] | same |
| explicit_plastic | stable_time_step | head3-t8 | 7 | 5 | 21.16 ms [21.08 ms, 21.62 ms] | 4.23 ms [4.22 ms, 4.32 ms] | 40.1 [40.0, 41.0] | 1.009 [1.004, 1.032] | same |
| explicit_plastic | stable_time_step | prev-t8 | 7 | 5 | 21.20 ms [21.12 ms, 21.26 ms] | 4.24 ms [4.22 ms, 4.25 ms] | 40.2 [40.1, 40.3] | 1.010 [1.005, 1.014] | same |
| explicit_plastic | solve_step | al-base | 7 | 120 | 986.85 ms [981.87 ms, 1.008 s] | 8.22 ms [8.18 ms, 8.40 ms] | 78.0 [77.6, 79.7] | – | same |
| explicit_plastic | solve_step | al-head3 | 7 | 120 | 989.01 ms [984.76 ms, 991.68 ms] | 8.24 ms [8.21 ms, 8.26 ms] | 78.2 [77.8, 78.4] | 1.002 [0.981, 1.005] | same |
| explicit_plastic | solve_step | base-t1 | 7 | 120 | 987.87 ms [984.48 ms, 1.017 s] | 8.23 ms [8.20 ms, 8.47 ms] | 78.1 [77.8, 80.3] | 1.000 [0.986, 1.030] | same |
| explicit_plastic | solve_step | head3-t1 | 7 | 120 | 987.97 ms [984.37 ms, 991.02 ms] | 8.23 ms [8.20 ms, 8.26 ms] | 78.1 [77.8, 78.3] | 1.003 [0.980, 1.006] | same |
| explicit_plastic | solve_step | head3-t8 | 7 | 120 | 347.73 ms [346.71 ms, 350.54 ms] | 2.90 ms [2.89 ms, 2.92 ms] | 27.5 [27.4, 27.7] | 0.353 [0.348, 0.354] | same |
| explicit_plastic | solve_step | prev-t8 | 7 | 120 | 348.19 ms [344.54 ms, 352.45 ms] | 2.90 ms [2.87 ms, 2.94 ms] | 27.5 [27.2, 27.9] | 0.352 [0.345, 0.357] | same |
| explicit_plastic | predictor | al-base | 7 | 120 | 11.4 µs [10.3 µs, 12.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | predictor | al-head3 | 7 | 120 | 10.6 µs [9.8 µs, 14.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.963 [0.863, 1.176] | same |
| explicit_plastic | predictor | base-t1 | 7 | 120 | 6.3 µs [5.2 µs, 13.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.537 [0.503, 1.192] | same |
| explicit_plastic | predictor | head3-t1 | 7 | 120 | 13.4 µs [10.2 µs, 17.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.200 [0.854, 1.688] | same |
| explicit_plastic | predictor | head3-t8 | 7 | 120 | 19.7 µs [17.8 µs, 24.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.775 [1.471, 2.418] | same |
| explicit_plastic | predictor | prev-t8 | 7 | 120 | 4.8 µs [3.1 µs, 8.2 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.472 [0.283, 0.709] | same |
| explicit_plastic | residual | al-base | 7 | 120 | 862.78 ms [859.75 ms, 882.94 ms] | 7.19 ms [7.16 ms, 7.36 ms] | 68.2 [67.9, 69.8] | – | same |
| explicit_plastic | residual | al-head3 | 7 | 120 | 863.62 ms [861.38 ms, 866.66 ms] | 7.20 ms [7.18 ms, 7.22 ms] | 68.2 [68.1, 68.5] | 1.002 [0.978, 1.004] | same |
| explicit_plastic | residual | base-t1 | 7 | 120 | 865.97 ms [861.45 ms, 893.21 ms] | 7.22 ms [7.18 ms, 7.44 ms] | 68.4 [68.1, 70.6] | 1.002 [0.986, 1.035] | same |
| explicit_plastic | residual | head3-t1 | 7 | 120 | 865.64 ms [862.45 ms, 868.11 ms] | 7.21 ms [7.19 ms, 7.23 ms] | 68.4 [68.2, 68.6] | 1.005 [0.979, 1.008] | same |
| explicit_plastic | residual | head3-t8 | 7 | 120 | 262.25 ms [260.52 ms, 262.66 ms] | 2.19 ms [2.17 ms, 2.19 ms] | 20.7 [20.6, 20.8] | 0.303 [0.297, 0.305] | same |
| explicit_plastic | residual | prev-t8 | 7 | 120 | 262.19 ms [259.21 ms, 264.45 ms] | 2.18 ms [2.16 ms, 2.20 ms] | 20.7 [20.5, 20.9] | 0.303 [0.297, 0.306] | same |
| explicit_plastic | lumped_matrix | al-base | 7 | 120 | 25.1 µs [21.1 µs, 30.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | lumped_matrix | al-head3 | 7 | 120 | 24.1 µs [22.2 µs, 28.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.968 [0.854, 1.174] | same |
| explicit_plastic | lumped_matrix | base-t1 | 7 | 120 | 21.2 µs [18.2 µs, 25.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.864 [0.639, 1.055] | same |
| explicit_plastic | lumped_matrix | head3-t1 | 7 | 120 | 29.1 µs [27.2 µs, 41.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.140 [0.886, 1.983] | same |
| explicit_plastic | lumped_matrix | head3-t8 | 7 | 120 | 32.7 µs [25.8 µs, 33.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.307 [0.888, 1.611] | same |
| explicit_plastic | lumped_matrix | prev-t8 | 7 | 120 | 22.1 µs [20.3 µs, 23.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.843 [0.745, 1.082] | same |
| explicit_plastic | corrector | al-base | 7 | 120 | 12.6 µs [10.7 µs, 14.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | corrector | al-head3 | 7 | 120 | 10.0 µs [8.4 µs, 11.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.865 [0.629, 0.999] | same |
| explicit_plastic | corrector | base-t1 | 7 | 120 | 10.1 µs [9.4 µs, 11.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.875 [0.664, 1.014] | same |
| explicit_plastic | corrector | head3-t1 | 7 | 120 | 12.5 µs [9.5 µs, 20.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.082 [0.661, 1.494] | same |
| explicit_plastic | corrector | head3-t8 | 7 | 120 | 20.0 µs [18.7 µs, 30.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.624 [1.434, 2.573] | same |
| explicit_plastic | corrector | prev-t8 | 7 | 120 | 21.7 µs [17.5 µs, 27.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.659 [1.359, 2.228] | same |
| explicit_plastic | step_hooks | al-base | 7 | 240 | 84.53 ms [83.01 ms, 84.74 ms] | 352.2 µs [345.9 µs, 353.1 µs] | 3.3 [3.3, 3.3] | – | same |
| explicit_plastic | step_hooks | al-head3 | 7 | 240 | 84.56 ms [83.19 ms, 85.18 ms] | 352.3 µs [346.6 µs, 354.9 µs] | 3.3 [3.3, 3.4] | 1.000 [0.991, 1.005] | same |
| explicit_plastic | step_hooks | base-t1 | 7 | 240 | 83.82 ms [83.18 ms, 85.08 ms] | 349.3 µs [346.6 µs, 354.5 µs] | 3.3 [3.3, 3.4] | 0.997 [0.983, 1.010] | same |
| explicit_plastic | step_hooks | head3-t1 | 7 | 240 | 83.59 ms [83.03 ms, 84.51 ms] | 348.3 µs [346.0 µs, 352.1 µs] | 3.3 [3.3, 3.3] | 0.995 [0.980, 1.005] | same |
| explicit_plastic | step_hooks | head3-t8 | 7 | 240 | 59.94 ms [58.65 ms, 61.40 ms] | 249.7 µs [244.4 µs, 255.8 µs] | 2.4 [2.3, 2.4] | 0.713 [0.693, 0.725] | same |
| explicit_plastic | step_hooks | prev-t8 | 7 | 240 | 59.68 ms [58.97 ms, 60.87 ms] | 248.7 µs [245.7 µs, 253.6 µs] | 2.4 [2.3, 2.4] | 0.710 [0.702, 0.720] | same |
| explicit_plastic | step_rest | al-base | 7 | 120 | 39.37 ms [38.73 ms, 40.37 ms] | 328.1 µs [322.8 µs, 336.4 µs] | 3.1 [3.1, 3.2] | – | same |
| explicit_plastic | step_rest | al-head3 | 7 | 120 | 39.60 ms [39.15 ms, 40.27 ms] | 330.0 µs [326.3 µs, 335.6 µs] | 3.1 [3.1, 3.2] | 0.999 [0.995, 1.029] | same |
| explicit_plastic | step_rest | base-t1 | 7 | 120 | 38.81 ms [37.70 ms, 39.25 ms] | 323.4 µs [314.2 µs, 327.0 µs] | 3.1 [3.0, 3.1] | 0.979 [0.962, 0.997] | same |
| explicit_plastic | step_rest | head3-t1 | 7 | 120 | 38.54 ms [38.34 ms, 39.07 ms] | 321.2 µs [319.5 µs, 325.6 µs] | 3.0 [3.0, 3.1] | 0.981 [0.967, 0.994] | same |
| explicit_plastic | step_rest | head3-t8 | 7 | 120 | 25.98 ms [25.44 ms, 26.56 ms] | 216.5 µs [212.0 µs, 221.3 µs] | 2.1 [2.0, 2.1] | 0.663 [0.649, 0.668] | same |
| explicit_plastic | step_rest | prev-t8 | 7 | 120 | 25.98 ms [25.79 ms, 26.53 ms] | 216.5 µs [214.9 µs, 221.1 µs] | 2.1 [2.0, 2.1] | 0.662 [0.644, 0.667] | same |
| explicit_plastic | internal_forces | al-base | 7 | 24 | 170.75 ms [169.27 ms, 171.66 ms] | 7.11 ms [7.05 ms, 7.15 ms] | 67.5 [66.9, 67.8] | – | same |
| explicit_plastic | internal_forces | al-head3 | 7 | 24 | 170.34 ms [169.74 ms, 171.65 ms] | 7.10 ms [7.07 ms, 7.15 ms] | 67.3 [67.1, 67.8] | 0.995 [0.992, 1.006] | same |
| explicit_plastic | internal_forces | base-t1 | 7 | 24 | 170.76 ms [170.38 ms, 190.53 ms] | 7.12 ms [7.10 ms, 7.94 ms] | 67.5 [67.3, 75.3] | 1.001 [0.993, 1.116] | same |
| explicit_plastic | internal_forces | head3-t1 | 7 | 24 | 171.28 ms [170.52 ms, 171.67 ms] | 7.14 ms [7.10 ms, 7.15 ms] | 67.7 [67.4, 67.8] | 1.001 [0.998, 1.014] | same |
| explicit_plastic | internal_forces | head3-t8 | 7 | 24 | 41.11 ms [39.87 ms, 43.29 ms] | 1.71 ms [1.66 ms, 1.80 ms] | 16.2 [15.8, 17.1] | 0.239 [0.233, 0.254] | same |
| explicit_plastic | internal_forces | prev-t8 | 7 | 24 | 41.22 ms [39.53 ms, 42.62 ms] | 1.72 ms [1.65 ms, 1.78 ms] | 16.3 [15.6, 16.8] | 0.240 [0.231, 0.250] | same |
| explicit_plastic | stresses | al-base | 7 | 24 | 121.67 ms [120.87 ms, 122.22 ms] | 5.07 ms [5.04 ms, 5.09 ms] | 48.1 [47.8, 48.3] | – | same |
| explicit_plastic | stresses | al-head3 | 7 | 24 | 121.75 ms [121.15 ms, 124.24 ms] | 5.07 ms [5.05 ms, 5.18 ms] | 48.1 [47.9, 49.1] | 1.001 [0.999, 1.017] | same |
| explicit_plastic | stresses | base-t1 | 7 | 24 | 121.84 ms [121.57 ms, 122.57 ms] | 5.08 ms [5.07 ms, 5.11 ms] | 48.1 [48.0, 48.4] | 1.000 [0.995, 1.008] | same |
| explicit_plastic | stresses | head3-t1 | 7 | 24 | 122.29 ms [121.95 ms, 122.46 ms] | 5.10 ms [5.08 ms, 5.10 ms] | 48.3 [48.2, 48.4] | 1.005 [0.999, 1.011] | same |
| explicit_plastic | stresses | head3-t8 | 7 | 24 | 103.65 ms [103.15 ms, 104.49 ms] | 4.32 ms [4.30 ms, 4.35 ms] | 41.0 [40.8, 41.3] | 0.852 [0.846, 0.857] | same |
| explicit_plastic | stresses | prev-t8 | 7 | 24 | 103.07 ms [102.30 ms, 103.95 ms] | 4.29 ms [4.26 ms, 4.33 ms] | 40.7 [40.4, 41.1] | 0.846 [0.841, 0.855] | same |
| explicit_plastic | energies | al-base | 7 | 24 | 40.90 ms [40.62 ms, 41.07 ms] | 1.70 ms [1.69 ms, 1.71 ms] | 16.2 [16.0, 16.2] | – | same |
| explicit_plastic | energies | al-head3 | 7 | 24 | 42.07 ms [41.43 ms, 42.49 ms] | 1.75 ms [1.73 ms, 1.77 ms] | 16.6 [16.4, 16.8] | 1.028 [1.016, 1.038] | same |
| explicit_plastic | energies | base-t1 | 7 | 24 | 40.89 ms [40.69 ms, 41.10 ms] | 1.70 ms [1.70 ms, 1.71 ms] | 16.2 [16.1, 16.2] | 1.000 [0.998, 1.007] | same |
| explicit_plastic | energies | head3-t1 | 7 | 24 | 42.35 ms [42.01 ms, 42.59 ms] | 1.76 ms [1.75 ms, 1.77 ms] | 16.7 [16.6, 16.8] | 1.038 [1.027, 1.044] | same |
| explicit_plastic | energies | head3-t8 | 7 | 24 | 41.73 ms [41.57 ms, 42.26 ms] | 1.74 ms [1.73 ms, 1.76 ms] | 16.5 [16.4, 16.7] | 1.024 [1.012, 1.031] | same |
| explicit_plastic | energies | prev-t8 | 7 | 24 | 41.46 ms [41.21 ms, 42.35 ms] | 1.73 ms [1.72 ms, 1.76 ms] | 16.4 [16.3, 16.7] | 1.013 [1.003, 1.043] | same |
| explicit_tet4_ss | mesh | al-base | 7 | 1 | 3.20 ms [3.12 ms, 3.26 ms] | 3.20 ms [3.12 ms, 3.26 ms] | 19.7 [19.2, 20.1] | – | same |
| explicit_tet4_ss | mesh | al-head3 | 7 | 1 | 3.20 ms [3.12 ms, 3.26 ms] | 3.20 ms [3.12 ms, 3.26 ms] | 19.8 [19.2, 20.1] | 1.006 [0.957, 1.016] | same |
| explicit_tet4_ss | mesh | base-t1 | 7 | 1 | 3.35 ms [3.16 ms, 3.40 ms] | 3.35 ms [3.16 ms, 3.40 ms] | 20.7 [19.5, 21.0] | 1.050 [0.979, 1.068] | same |
| explicit_tet4_ss | mesh | head3-t1 | 7 | 1 | 3.21 ms [3.08 ms, 3.28 ms] | 3.21 ms [3.08 ms, 3.28 ms] | 19.8 [19.0, 20.2] | 0.984 [0.956, 1.052] | same |
| explicit_tet4_ss | mesh | head3-t8 | 7 | 1 | 3.31 ms [3.17 ms, 3.41 ms] | 3.31 ms [3.17 ms, 3.41 ms] | 20.4 [19.6, 21.1] | 1.048 [0.982, 1.065] | same |
| explicit_tet4_ss | mesh | prev-t8 | 7 | 1 | 3.28 ms [3.18 ms, 3.42 ms] | 3.28 ms [3.18 ms, 3.42 ms] | 20.2 [19.7, 21.1] | 1.037 [0.986, 1.062] | same |
| explicit_tet4_ss | init | al-base | 7 | 1 | 48.27 ms [46.62 ms, 48.77 ms] | 48.27 ms [46.62 ms, 48.77 ms] | 298.0 [287.8, 301.0] | – | same |
| explicit_tet4_ss | init | al-head3 | 7 | 1 | 47.85 ms [46.53 ms, 48.54 ms] | 47.85 ms [46.53 ms, 48.54 ms] | 295.3 [287.2, 299.6] | 0.991 [0.973, 1.040] | same |
| explicit_tet4_ss | init | base-t1 | 7 | 1 | 48.48 ms [47.83 ms, 49.40 ms] | 48.48 ms [47.83 ms, 49.40 ms] | 299.2 [295.3, 305.0] | 1.013 [0.982, 1.058] | same |
| explicit_tet4_ss | init | head3-t1 | 7 | 1 | 47.65 ms [46.46 ms, 50.02 ms] | 47.65 ms [46.46 ms, 50.02 ms] | 294.2 [286.8, 308.8] | 0.999 [0.958, 1.073] | same |
| explicit_tet4_ss | init | head3-t8 | 7 | 1 | 49.39 ms [47.72 ms, 50.86 ms] | 49.39 ms [47.72 ms, 50.86 ms] | 304.9 [294.5, 313.9] | 1.028 [1.006, 1.069] | same |
| explicit_tet4_ss | init | prev-t8 | 7 | 1 | 49.12 ms [47.67 ms, 50.41 ms] | 49.12 ms [47.67 ms, 50.41 ms] | 303.2 [294.3, 311.1] | 1.029 [0.997, 1.081] | same |
| explicit_tet4_ss | stable_time_step | al-base | 7 | 5 | 32.29 ms [32.23 ms, 33.16 ms] | 6.46 ms [6.45 ms, 6.63 ms] | 39.9 [39.8, 40.9] | – | same |
| explicit_tet4_ss | stable_time_step | al-head3 | 7 | 5 | 32.55 ms [32.44 ms, 33.26 ms] | 6.51 ms [6.49 ms, 6.65 ms] | 40.2 [40.1, 41.1] | 1.009 [0.978, 1.032] | same |
| explicit_tet4_ss | stable_time_step | base-t1 | 7 | 5 | 32.29 ms [32.22 ms, 32.35 ms] | 6.46 ms [6.44 ms, 6.47 ms] | 39.9 [39.8, 39.9] | 0.999 [0.974, 1.003] | same |
| explicit_tet4_ss | stable_time_step | head3-t1 | 7 | 5 | 32.48 ms [32.40 ms, 33.13 ms] | 6.50 ms [6.48 ms, 6.63 ms] | 40.1 [40.0, 40.9] | 1.005 [0.999, 1.008] | same |
| explicit_tet4_ss | stable_time_step | head3-t8 | 7 | 5 | 32.69 ms [32.49 ms, 33.51 ms] | 6.54 ms [6.50 ms, 6.70 ms] | 40.4 [40.1, 41.4] | 1.012 [0.986, 1.036] | same |
| explicit_tet4_ss | stable_time_step | prev-t8 | 7 | 5 | 32.56 ms [32.43 ms, 33.06 ms] | 6.51 ms [6.49 ms, 6.61 ms] | 40.2 [40.0, 40.8] | 1.008 [0.979, 1.022] | same |
| explicit_tet4_ss | solve_step | al-base | 7 | 200 | 1.297 s [1.293 s, 1.302 s] | 6.48 ms [6.46 ms, 6.51 ms] | 40.0 [39.9, 40.2] | – | same |
| explicit_tet4_ss | solve_step | al-head3 | 7 | 200 | 1.294 s [1.280 s, 1.300 s] | 6.47 ms [6.40 ms, 6.50 ms] | 39.9 [39.5, 40.1] | 0.996 [0.990, 0.999] | same |
| explicit_tet4_ss | solve_step | base-t1 | 7 | 200 | 1.294 s [1.283 s, 1.300 s] | 6.47 ms [6.42 ms, 6.50 ms] | 40.0 [39.6, 40.1] | 1.000 [0.985, 1.001] | same |
| explicit_tet4_ss | solve_step | head3-t1 | 7 | 200 | 1.291 s [1.288 s, 1.294 s] | 6.45 ms [6.44 ms, 6.47 ms] | 39.8 [39.7, 39.9] | 0.995 [0.989, 0.999] | same |
| explicit_tet4_ss | solve_step | head3-t8 | 7 | 200 | 348.50 ms [345.73 ms, 350.93 ms] | 1.74 ms [1.73 ms, 1.75 ms] | 10.8 [10.7, 10.8] | 0.269 [0.267, 0.271] | same |
| explicit_tet4_ss | solve_step | prev-t8 | 7 | 200 | 347.48 ms [346.53 ms, 363.38 ms] | 1.74 ms [1.73 ms, 1.82 ms] | 10.7 [10.7, 11.2] | 0.268 [0.266, 0.281] | same |
| explicit_tet4_ss | predictor | al-base | 7 | 200 | 15.1 µs [13.7 µs, 17.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | al-head3 | 7 | 200 | 18.0 µs [8.6 µs, 21.5 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.031 [0.535, 1.506] | same |
| explicit_tet4_ss | predictor | base-t1 | 7 | 200 | 8.8 µs [7.2 µs, 18.0 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.555 [0.464, 1.311] | same |
| explicit_tet4_ss | predictor | head3-t1 | 7 | 200 | 18.1 µs [15.2 µs, 26.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.280 [1.008, 1.850] | same |
| explicit_tet4_ss | predictor | head3-t8 | 7 | 200 | 15.9 µs [8.9 µs, 22.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.029 [0.551, 1.570] | same |
| explicit_tet4_ss | predictor | prev-t8 | 7 | 200 | 6.1 µs [5.3 µs, 15.4 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.396 [0.332, 0.919] | same |
| explicit_tet4_ss | residual | al-base | 7 | 200 | 1.206 s [1.203 s, 1.212 s] | 6.03 ms [6.01 ms, 6.06 ms] | 37.2 [37.1, 37.4] | – | same |
| explicit_tet4_ss | residual | al-head3 | 7 | 200 | 1.203 s [1.190 s, 1.209 s] | 6.02 ms [5.95 ms, 6.05 ms] | 37.1 [36.7, 37.3] | 0.996 [0.990, 1.000] | same |
| explicit_tet4_ss | residual | base-t1 | 7 | 200 | 1.207 s [1.196 s, 1.212 s] | 6.03 ms [5.98 ms, 6.06 ms] | 37.2 [36.9, 37.4] | 1.002 [0.987, 1.004] | same |
| explicit_tet4_ss | residual | head3-t1 | 7 | 200 | 1.204 s [1.200 s, 1.207 s] | 6.02 ms [6.00 ms, 6.03 ms] | 37.2 [37.0, 37.2] | 0.998 [0.991, 1.002] | same |
| explicit_tet4_ss | residual | head3-t8 | 7 | 200 | 308.03 ms [305.21 ms, 310.01 ms] | 1.54 ms [1.53 ms, 1.55 ms] | 9.5 [9.4, 9.6] | 0.255 [0.253, 0.258] | same |
| explicit_tet4_ss | residual | prev-t8 | 7 | 200 | 306.75 ms [306.06 ms, 322.43 ms] | 1.53 ms [1.53 ms, 1.61 ms] | 9.5 [9.4, 10.0] | 0.255 [0.253, 0.268] | same |
| explicit_tet4_ss | lumped_matrix | al-base | 7 | 200 | 35.8 µs [26.0 µs, 44.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | al-head3 | 7 | 200 | 31.2 µs [27.4 µs, 39.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.001 [0.619, 1.403] | same |
| explicit_tet4_ss | lumped_matrix | base-t1 | 7 | 200 | 32.7 µs [20.4 µs, 36.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.938 [0.549, 1.258] | same |
| explicit_tet4_ss | lumped_matrix | head3-t1 | 7 | 200 | 35.4 µs [29.8 µs, 37.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.996 [0.757, 1.384] | same |
| explicit_tet4_ss | lumped_matrix | head3-t8 | 7 | 200 | 38.9 µs [31.8 µs, 49.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.970 [0.888, 1.889] | same |
| explicit_tet4_ss | lumped_matrix | prev-t8 | 7 | 200 | 32.1 µs [20.5 µs, 36.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.932 [0.550, 1.238] | same |
| explicit_tet4_ss | corrector | al-base | 7 | 200 | 15.9 µs [13.2 µs, 19.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | al-head3 | 7 | 200 | 17.0 µs [11.6 µs, 26.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.288 [0.678, 1.704] | same |
| explicit_tet4_ss | corrector | base-t1 | 7 | 200 | 17.2 µs [13.2 µs, 22.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.978 [0.799, 1.691] | same |
| explicit_tet4_ss | corrector | head3-t1 | 7 | 200 | 15.2 µs [13.4 µs, 19.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.926 [0.867, 1.217] | same |
| explicit_tet4_ss | corrector | head3-t8 | 7 | 200 | 18.5 µs [12.7 µs, 28.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.222 [0.724, 1.703] | same |
| explicit_tet4_ss | corrector | prev-t8 | 7 | 200 | 16.7 µs [12.2 µs, 25.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.067 [0.626, 1.664] | same |
| explicit_tet4_ss | step_hooks | al-base | 7 | 400 | 537.0 µs [508.2 µs, 592.9 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | al-head3 | 7 | 400 | 554.5 µs [490.9 µs, 581.9 µs] | 1.4 µs [1.2 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.015 [0.853, 1.091] | same |
| explicit_tet4_ss | step_hooks | base-t1 | 7 | 400 | 532.8 µs [513.4 µs, 599.4 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 0.992 [0.878, 1.061] | same |
| explicit_tet4_ss | step_hooks | head3-t1 | 7 | 400 | 520.4 µs [497.7 µs, 568.3 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.971 [0.861, 1.060] | same |
| explicit_tet4_ss | step_hooks | head3-t8 | 7 | 400 | 477.5 µs [461.9 µs, 541.1 µs] | 1.2 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.890 [0.779, 0.982] | same |
| explicit_tet4_ss | step_hooks | prev-t8 | 7 | 400 | 490.6 µs [448.7 µs, 568.5 µs] | 1.2 µs [1.1 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.888 [0.767, 1.002] | same |
| explicit_tet4_ss | step_rest | al-base | 7 | 200 | 90.10 ms [89.48 ms, 90.30 ms] | 450.5 µs [447.4 µs, 451.5 µs] | 2.8 [2.8, 2.8] | – | same |
| explicit_tet4_ss | step_rest | al-head3 | 7 | 200 | 89.73 ms [89.14 ms, 90.54 ms] | 448.7 µs [445.7 µs, 452.7 µs] | 2.8 [2.8, 2.8] | 0.997 [0.993, 1.005] | same |
| explicit_tet4_ss | step_rest | base-t1 | 7 | 200 | 86.81 ms [86.32 ms, 87.29 ms] | 434.1 µs [431.6 µs, 436.5 µs] | 2.7 [2.7, 2.7] | 0.965 [0.957, 0.970] | same |
| explicit_tet4_ss | step_rest | head3-t1 | 7 | 200 | 86.68 ms [86.28 ms, 86.97 ms] | 433.4 µs [431.4 µs, 434.9 µs] | 2.7 [2.7, 2.7] | 0.961 [0.956, 0.969] | same |
| explicit_tet4_ss | step_rest | head3-t8 | 7 | 200 | 40.28 ms [39.55 ms, 40.60 ms] | 201.4 µs [197.8 µs, 203.0 µs] | 1.2 [1.2, 1.3] | 0.448 [0.438, 0.451] | same |
| explicit_tet4_ss | step_rest | prev-t8 | 7 | 200 | 40.05 ms [39.92 ms, 40.73 ms] | 200.2 µs [199.6 µs, 203.6 µs] | 1.2 [1.2, 1.3] | 0.447 [0.442, 0.452] | same |
| explicit_tet4_ss | internal_forces | al-base | 7 | 40 | 227.67 ms [226.43 ms, 230.20 ms] | 5.69 ms [5.66 ms, 5.76 ms] | 35.1 [34.9, 35.5] | – | same |
| explicit_tet4_ss | internal_forces | al-head3 | 7 | 40 | 227.89 ms [226.58 ms, 228.43 ms] | 5.70 ms [5.66 ms, 5.71 ms] | 35.2 [35.0, 35.3] | 1.000 [0.992, 1.007] | same |
| explicit_tet4_ss | internal_forces | base-t1 | 7 | 40 | 229.31 ms [226.40 ms, 230.19 ms] | 5.73 ms [5.66 ms, 5.75 ms] | 35.4 [34.9, 35.5] | 1.001 [0.993, 1.017] | same |
| explicit_tet4_ss | internal_forces | head3-t1 | 7 | 40 | 230.30 ms [228.40 ms, 232.04 ms] | 5.76 ms [5.71 ms, 5.80 ms] | 35.5 [35.2, 35.8] | 1.012 [0.995, 1.018] | same |
| explicit_tet4_ss | internal_forces | head3-t8 | 7 | 40 | 51.32 ms [50.79 ms, 53.97 ms] | 1.28 ms [1.27 ms, 1.35 ms] | 7.9 [7.8, 8.3] | 0.226 [0.223, 0.238] | same |
| explicit_tet4_ss | internal_forces | prev-t8 | 7 | 40 | 51.42 ms [51.11 ms, 53.17 ms] | 1.29 ms [1.28 ms, 1.33 ms] | 7.9 [7.9, 8.2] | 0.226 [0.223, 0.234] | same |
| explicit_tet4_ss | stresses | al-base | 7 | 40 | 86.44 ms [86.15 ms, 87.40 ms] | 2.16 ms [2.15 ms, 2.18 ms] | 13.3 [13.3, 13.5] | – | same |
| explicit_tet4_ss | stresses | al-head3 | 7 | 40 | 86.35 ms [85.82 ms, 87.19 ms] | 2.16 ms [2.15 ms, 2.18 ms] | 13.3 [13.2, 13.5] | 0.997 [0.993, 1.004] | same |
| explicit_tet4_ss | stresses | base-t1 | 7 | 40 | 86.40 ms [86.23 ms, 87.08 ms] | 2.16 ms [2.16 ms, 2.18 ms] | 13.3 [13.3, 13.4] | 1.001 [0.987, 1.007] | same |
| explicit_tet4_ss | stresses | head3-t1 | 7 | 40 | 86.79 ms [86.32 ms, 87.05 ms] | 2.17 ms [2.16 ms, 2.18 ms] | 13.4 [13.3, 13.4] | 1.003 [0.993, 1.005] | same |
| explicit_tet4_ss | stresses | head3-t8 | 7 | 40 | 38.85 ms [38.27 ms, 39.80 ms] | 971.2 µs [956.8 µs, 994.9 µs] | 6.0 [5.9, 6.1] | 0.450 [0.443, 0.459] | same |
| explicit_tet4_ss | stresses | prev-t8 | 7 | 40 | 39.02 ms [38.04 ms, 39.54 ms] | 975.5 µs [951.0 µs, 988.6 µs] | 6.0 [5.9, 6.1] | 0.452 [0.440, 0.454] | same |
| explicit_tet4_ss | energies | al-base | 7 | 40 | 83.47 ms [83.28 ms, 85.05 ms] | 2.09 ms [2.08 ms, 2.13 ms] | 12.9 [12.9, 13.1] | – | same |
| explicit_tet4_ss | energies | al-head3 | 7 | 40 | 86.99 ms [86.76 ms, 87.20 ms] | 2.17 ms [2.17 ms, 2.18 ms] | 13.4 [13.4, 13.5] | 1.040 [1.025, 1.045] | same |
| explicit_tet4_ss | energies | base-t1 | 7 | 40 | 83.02 ms [82.81 ms, 83.43 ms] | 2.08 ms [2.07 ms, 2.09 ms] | 12.8 [12.8, 12.9] | 0.993 [0.979, 1.002] | same |
| explicit_tet4_ss | energies | head3-t1 | 7 | 40 | 86.94 ms [86.77 ms, 87.55 ms] | 2.17 ms [2.17 ms, 2.19 ms] | 13.4 [13.4, 13.5] | 1.042 [1.021, 1.048] | same |
| explicit_tet4_ss | energies | head3-t8 | 7 | 40 | 85.33 ms [84.94 ms, 85.85 ms] | 2.13 ms [2.12 ms, 2.15 ms] | 13.2 [13.1, 13.2] | 1.022 [1.003, 1.024] | same |
| explicit_tet4_ss | energies | prev-t8 | 7 | 40 | 84.43 ms [84.01 ms, 84.80 ms] | 2.11 ms [2.10 ms, 2.12 ms] | 13.0 [13.0, 13.1] | 1.012 [0.993, 1.017] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, ce03eaad5, f1dbf028b | 017c8c9ef7659aa0 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, ce03eaad5, f1dbf028b | ce8154043761dbc7 |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, ce03eaad5, f1dbf028b | 5c844dbf1dc6d9c8 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42, ce03eaad5, f1dbf028b | fecb3fb4b46c8b8c |
