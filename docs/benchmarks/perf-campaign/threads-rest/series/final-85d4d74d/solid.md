Suite `solid`, size `m`; variants: `base-t1`, `base-t8`, `head-t1`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_hex8_fd | mesh | base-t1 | 5 | 1 | 6.18 ms [6.17 ms, 6.64 ms] | 6.18 ms [6.17 ms, 6.64 ms] | 55.8 [55.8, 60.1] | – | same |
| explicit_hex8_fd | mesh | base-t8 | 5 | 1 | 6.43 ms [6.14 ms, 6.53 ms] | 6.43 ms [6.14 ms, 6.53 ms] | 58.2 [55.5, 59.0] | 1.034 [0.969, 1.058] | same |
| explicit_hex8_fd | mesh | head-t1 | 5 | 1 | 6.37 ms [6.22 ms, 6.41 ms] | 6.37 ms [6.22 ms, 6.41 ms] | 57.6 [56.3, 58.0] | 1.011 [0.958, 1.040] | same |
| explicit_hex8_fd | mesh | head-t8 | 5 | 1 | 6.30 ms [6.15 ms, 6.47 ms] | 6.30 ms [6.15 ms, 6.47 ms] | 57.0 [55.6, 58.5] | 0.998 [0.968, 1.048] | same |
| explicit_hex8_fd | init | base-t1 | 5 | 1 | 296.43 ms [292.28 ms, 301.34 ms] | 296.43 ms [292.28 ms, 301.34 ms] | 2680.4 [2642.9, 2724.8] | – | same |
| explicit_hex8_fd | init | base-t8 | 5 | 1 | 295.47 ms [292.05 ms, 302.91 ms] | 295.47 ms [292.05 ms, 302.91 ms] | 2671.7 [2640.7, 2739.0] | 1.002 [0.993, 1.007] | same |
| explicit_hex8_fd | init | head-t1 | 5 | 1 | 302.34 ms [297.67 ms, 306.93 ms] | 302.34 ms [297.67 ms, 306.93 ms] | 2733.8 [2691.6, 2775.3] | 1.019 [1.000, 1.034] | same |
| explicit_hex8_fd | init | head-t8 | 5 | 1 | 299.71 ms [295.80 ms, 302.30 ms] | 299.71 ms [295.80 ms, 302.30 ms] | 2710.0 [2674.7, 2733.5] | 1.003 [0.993, 1.025] | same |
| explicit_hex8_fd | stable_time_step | base-t1 | 5 | 5 | 23.63 ms [23.55 ms, 25.91 ms] | 4.73 ms [4.71 ms, 5.18 ms] | 42.7 [42.6, 46.9] | – | same |
| explicit_hex8_fd | stable_time_step | base-t8 | 5 | 5 | 25.24 ms [23.60 ms, 25.42 ms] | 5.05 ms [4.72 ms, 5.08 ms] | 45.6 [42.7, 46.0] | 1.068 [0.917, 1.080] | same |
| explicit_hex8_fd | stable_time_step | head-t1 | 5 | 5 | 23.11 ms [23.05 ms, 23.46 ms] | 4.62 ms [4.61 ms, 4.69 ms] | 41.8 [41.7, 42.4] | 0.979 [0.892, 0.993] | same |
| explicit_hex8_fd | stable_time_step | head-t8 | 5 | 5 | 23.80 ms [23.15 ms, 24.17 ms] | 4.76 ms [4.63 ms, 4.83 ms] | 43.0 [41.9, 43.7] | 1.011 [0.896, 1.023] | same |
| explicit_hex8_fd | solve_step | base-t1 | 5 | 30 | 1.312 s [1.300 s, 1.314 s] | 43.73 ms [43.34 ms, 43.79 ms] | 395.4 [391.9, 395.9] | – | same |
| explicit_hex8_fd | solve_step | base-t8 | 5 | 30 | 357.06 ms [350.31 ms, 359.01 ms] | 11.90 ms [11.68 ms, 11.97 ms] | 107.6 [105.6, 108.2] | 0.272 [0.269, 0.274] | same |
| explicit_hex8_fd | solve_step | head-t1 | 5 | 30 | 1.324 s [1.311 s, 1.328 s] | 44.12 ms [43.69 ms, 44.26 ms] | 398.9 [395.0, 400.2] | 1.009 [1.006, 1.013] | same |
| explicit_hex8_fd | solve_step | head-t8 | 5 | 30 | 321.25 ms [313.59 ms, 321.51 ms] | 10.71 ms [10.45 ms, 10.72 ms] | 96.8 [94.5, 96.9] | 0.245 [0.241, 0.245] | same |
| explicit_hex8_fd | predictor | base-t1 | 5 | 30 | 4.8 µs [3.8 µs, 5.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | predictor | base-t8 | 5 | 30 | 4.7 µs [3.9 µs, 6.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.012 [0.762, 1.377] | same |
| explicit_hex8_fd | predictor | head-t1 | 5 | 30 | 5.2 µs [4.6 µs, 5.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.163 [0.895, 1.536] | same |
| explicit_hex8_fd | predictor | head-t8 | 5 | 30 | 4.6 µs [3.9 µs, 5.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.925 [0.809, 1.413] | same |
| explicit_hex8_fd | residual | base-t1 | 5 | 30 | 1.254 s [1.243 s, 1.255 s] | 41.80 ms [41.44 ms, 41.83 ms] | 378.0 [374.7, 378.3] | – | same |
| explicit_hex8_fd | residual | base-t8 | 5 | 30 | 299.59 ms [293.79 ms, 300.90 ms] | 9.99 ms [9.79 ms, 10.03 ms] | 90.3 [88.5, 90.7] | 0.239 [0.236, 0.240] | same |
| explicit_hex8_fd | residual | head-t1 | 5 | 30 | 1.260 s [1.248 s, 1.264 s] | 42.00 ms [41.60 ms, 42.12 ms] | 379.8 [376.1, 380.9] | 1.004 [1.002, 1.009] | same |
| explicit_hex8_fd | residual | head-t8 | 5 | 30 | 287.90 ms [281.31 ms, 289.00 ms] | 9.60 ms [9.38 ms, 9.63 ms] | 86.8 [84.8, 87.1] | 0.230 [0.226, 0.230] | same |
| explicit_hex8_fd | lumped_matrix | base-t1 | 5 | 30 | 8.6 µs [7.5 µs, 8.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | lumped_matrix | base-t8 | 5 | 30 | 10.2 µs [10.1 µs, 12.1 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.238 [1.160, 1.621] | same |
| explicit_hex8_fd | lumped_matrix | head-t1 | 5 | 30 | 9.3 µs [7.2 µs, 10.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.127 [0.829, 1.251] | same |
| explicit_hex8_fd | lumped_matrix | head-t8 | 5 | 30 | 8.7 µs [8.4 µs, 9.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.049 [0.981, 1.136] | same |
| explicit_hex8_fd | corrector | base-t1 | 5 | 30 | 4.8 µs [4.5 µs, 6.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | corrector | base-t8 | 5 | 30 | 4.9 µs [4.5 µs, 7.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.085 [0.755, 1.332] | same |
| explicit_hex8_fd | corrector | head-t1 | 5 | 30 | 7.6 µs [7.5 µs, 8.2 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.612 [1.272, 1.691] | same |
| explicit_hex8_fd | corrector | head-t8 | 5 | 30 | 7.9 µs [7.4 µs, 8.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.547 [1.439, 1.677] | same |
| explicit_hex8_fd | step_hooks | base-t1 | 5 | 60 | 118.1 µs [113.8 µs, 122.2 µs] | 2.0 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | step_hooks | base-t8 | 5 | 60 | 122.3 µs [120.1 µs, 127.7 µs] | 2.0 µs [2.0 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 1.052 [1.001, 1.123] | same |
| explicit_hex8_fd | step_hooks | head-t1 | 5 | 60 | 118.0 µs [116.6 µs, 125.5 µs] | 2.0 µs [1.9 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 1.024 [0.999, 1.030] | same |
| explicit_hex8_fd | step_hooks | head-t8 | 5 | 60 | 129.4 µs [121.2 µs, 131.4 µs] | 2.2 µs [2.0 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 1.071 [1.048, 1.137] | same |
| explicit_hex8_fd | step_rest | base-t1 | 5 | 30 | 57.70 ms [57.09 ms, 58.40 ms] | 1.92 ms [1.90 ms, 1.95 ms] | 17.4 [17.2, 17.6] | – | same |
| explicit_hex8_fd | step_rest | base-t8 | 5 | 30 | 57.57 ms [56.38 ms, 57.95 ms] | 1.92 ms [1.88 ms, 1.93 ms] | 17.4 [17.0, 17.5] | 0.989 [0.986, 1.005] | same |
| explicit_hex8_fd | step_rest | head-t1 | 5 | 30 | 63.26 ms [62.55 ms, 63.91 ms] | 2.11 ms [2.08 ms, 2.13 ms] | 19.1 [18.9, 19.3] | 1.096 [1.075, 1.103] | same |
| explicit_hex8_fd | step_rest | head-t8 | 5 | 30 | 32.78 ms [32.13 ms, 33.23 ms] | 1.09 ms [1.07 ms, 1.11 ms] | 9.9 [9.7, 10.0] | 0.563 [0.561, 0.573] | same |
| explicit_hex8_fd | internal_forces | base-t1 | 5 | 6 | 245.68 ms [244.06 ms, 246.44 ms] | 40.95 ms [40.68 ms, 41.07 ms] | 370.2 [367.8, 371.4] | – | same |
| explicit_hex8_fd | internal_forces | base-t8 | 5 | 6 | 54.15 ms [52.81 ms, 55.57 ms] | 9.03 ms [8.80 ms, 9.26 ms] | 81.6 [79.6, 83.7] | 0.222 [0.214, 0.226] | same |
| explicit_hex8_fd | internal_forces | head-t1 | 5 | 6 | 247.32 ms [244.34 ms, 247.63 ms] | 41.22 ms [40.72 ms, 41.27 ms] | 372.7 [368.2, 373.2] | 1.006 [1.001, 1.008] | same |
| explicit_hex8_fd | internal_forces | head-t8 | 5 | 6 | 54.24 ms [53.44 ms, 54.90 ms] | 9.04 ms [8.91 ms, 9.15 ms] | 81.7 [80.5, 82.7] | 0.221 [0.219, 0.223] | same |
| explicit_hex8_fd | stresses | base-t1 | 5 | 6 | 113.13 ms [109.01 ms, 114.69 ms] | 18.86 ms [18.17 ms, 19.11 ms] | 170.5 [164.3, 172.8] | – | same |
| explicit_hex8_fd | stresses | base-t8 | 5 | 6 | 69.92 ms [67.90 ms, 70.50 ms] | 11.65 ms [11.32 ms, 11.75 ms] | 105.4 [102.3, 106.2] | 0.615 [0.596, 0.641] | same |
| explicit_hex8_fd | stresses | head-t1 | 5 | 6 | 111.18 ms [109.49 ms, 115.16 ms] | 18.53 ms [18.25 ms, 19.19 ms] | 167.6 [165.0, 173.5] | 0.995 [0.965, 1.004] | same |
| explicit_hex8_fd | stresses | head-t8 | 5 | 6 | 71.32 ms [70.98 ms, 72.34 ms] | 11.89 ms [11.83 ms, 12.06 ms] | 107.5 [107.0, 109.0] | 0.631 [0.625, 0.651] | same |
| explicit_hex8_fd | energies | base-t1 | 5 | 6 | 47.09 ms [46.84 ms, 47.29 ms] | 7.85 ms [7.81 ms, 7.88 ms] | 71.0 [70.6, 71.3] | – | same |
| explicit_hex8_fd | energies | base-t8 | 5 | 6 | 47.02 ms [46.74 ms, 47.35 ms] | 7.84 ms [7.79 ms, 7.89 ms] | 70.9 [70.4, 71.4] | 1.001 [0.998, 1.001] | same |
| explicit_hex8_fd | energies | head-t1 | 5 | 6 | 46.90 ms [46.74 ms, 47.13 ms] | 7.82 ms [7.79 ms, 7.85 ms] | 70.7 [70.4, 71.0] | 0.998 [0.991, 0.999] | same |
| explicit_hex8_fd | energies | head-t8 | 5 | 6 | 47.15 ms [46.89 ms, 47.35 ms] | 7.86 ms [7.82 ms, 7.89 ms] | 71.1 [70.7, 71.4] | 1.001 [0.998, 1.002] | same |
| explicit_hex8_ss | mesh | base-t1 | 5 | 1 | 6.43 ms [6.17 ms, 6.50 ms] | 6.43 ms [6.17 ms, 6.50 ms] | 58.1 [55.8, 58.8] | – | same |
| explicit_hex8_ss | mesh | base-t8 | 5 | 1 | 6.31 ms [6.23 ms, 6.41 ms] | 6.31 ms [6.23 ms, 6.41 ms] | 57.1 [56.3, 57.9] | 0.982 [0.968, 1.032] | same |
| explicit_hex8_ss | mesh | head-t1 | 5 | 1 | 6.32 ms [6.29 ms, 6.39 ms] | 6.32 ms [6.29 ms, 6.39 ms] | 57.1 [56.9, 57.8] | 0.983 [0.978, 1.022] | same |
| explicit_hex8_ss | mesh | head-t8 | 5 | 1 | 6.48 ms [6.20 ms, 6.64 ms] | 6.48 ms [6.20 ms, 6.64 ms] | 58.6 [56.1, 60.0] | 1.007 [0.965, 1.076] | same |
| explicit_hex8_ss | init | base-t1 | 5 | 1 | 264.91 ms [258.31 ms, 268.78 ms] | 264.91 ms [258.31 ms, 268.78 ms] | 2395.4 [2335.7, 2430.4] | – | same |
| explicit_hex8_ss | init | base-t8 | 5 | 1 | 264.46 ms [262.29 ms, 267.24 ms] | 264.46 ms [262.29 ms, 267.24 ms] | 2391.4 [2371.7, 2416.4] | 0.998 [0.976, 1.018] | same |
| explicit_hex8_ss | init | head-t1 | 5 | 1 | 269.70 ms [264.51 ms, 273.63 ms] | 269.70 ms [264.51 ms, 273.63 ms] | 2438.7 [2391.8, 2474.3] | 1.023 [0.984, 1.033] | same |
| explicit_hex8_ss | init | head-t8 | 5 | 1 | 268.48 ms [266.19 ms, 271.26 ms] | 268.48 ms [266.19 ms, 271.26 ms] | 2427.7 [2406.9, 2452.8] | 1.012 [1.003, 1.034] | same |
| explicit_hex8_ss | stable_time_step | base-t1 | 5 | 5 | 23.70 ms [23.58 ms, 25.65 ms] | 4.74 ms [4.72 ms, 5.13 ms] | 42.9 [42.6, 46.4] | – | same |
| explicit_hex8_ss | stable_time_step | base-t8 | 5 | 5 | 23.65 ms [23.60 ms, 23.71 ms] | 4.73 ms [4.72 ms, 4.74 ms] | 42.8 [42.7, 42.9] | 0.997 [0.924, 1.001] | same |
| explicit_hex8_ss | stable_time_step | head-t1 | 5 | 5 | 23.22 ms [23.09 ms, 23.88 ms] | 4.64 ms [4.62 ms, 4.78 ms] | 42.0 [41.7, 43.2] | 0.976 [0.931, 0.992] | same |
| explicit_hex8_ss | stable_time_step | head-t8 | 5 | 5 | 23.93 ms [23.15 ms, 24.22 ms] | 4.79 ms [4.63 ms, 4.84 ms] | 43.3 [41.9, 43.8] | 1.015 [0.903, 1.022] | same |
| explicit_hex8_ss | solve_step | base-t1 | 5 | 50 | 1.525 s [1.504 s, 1.537 s] | 30.51 ms [30.07 ms, 30.74 ms] | 275.8 [271.9, 278.0] | – | same |
| explicit_hex8_ss | solve_step | base-t8 | 5 | 50 | 583.87 ms [582.71 ms, 590.43 ms] | 11.68 ms [11.65 ms, 11.81 ms] | 105.6 [105.4, 106.8] | 0.387 [0.380, 0.389] | same |
| explicit_hex8_ss | solve_step | head-t1 | 5 | 50 | 1.537 s [1.514 s, 1.542 s] | 30.74 ms [30.29 ms, 30.85 ms] | 278.0 [273.9, 278.9] | 1.007 [1.003, 1.019] | same |
| explicit_hex8_ss | solve_step | head-t8 | 5 | 50 | 517.08 ms [512.60 ms, 523.39 ms] | 10.34 ms [10.25 ms, 10.47 ms] | 93.5 [92.7, 94.7] | 0.341 [0.336, 0.347] | same |
| explicit_hex8_ss | predictor | base-t1 | 5 | 50 | 7.8 µs [5.8 µs, 8.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | predictor | base-t8 | 5 | 50 | 7.4 µs [6.8 µs, 8.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.977 [0.839, 1.213] | same |
| explicit_hex8_ss | predictor | head-t1 | 5 | 50 | 9.6 µs [7.6 µs, 10.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.244 [1.053, 1.656] | same |
| explicit_hex8_ss | predictor | head-t8 | 5 | 50 | 7.2 µs [5.5 µs, 9.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.925 [0.756, 1.560] | same |
| explicit_hex8_ss | residual | base-t1 | 5 | 50 | 1.428 s [1.408 s, 1.440 s] | 28.56 ms [28.16 ms, 28.80 ms] | 258.3 [254.7, 260.4] | – | same |
| explicit_hex8_ss | residual | base-t8 | 5 | 50 | 489.76 ms [486.91 ms, 493.09 ms] | 9.80 ms [9.74 ms, 9.86 ms] | 88.6 [88.1, 89.2] | 0.345 [0.339, 0.348] | same |
| explicit_hex8_ss | residual | head-t1 | 5 | 50 | 1.432 s [1.410 s, 1.437 s] | 28.63 ms [28.21 ms, 28.74 ms] | 258.9 [255.1, 259.9] | 1.002 [0.997, 1.013] | same |
| explicit_hex8_ss | residual | head-t8 | 5 | 50 | 462.57 ms [458.83 ms, 468.43 ms] | 9.25 ms [9.18 ms, 9.37 ms] | 83.7 [83.0, 84.7] | 0.326 [0.321, 0.332] | same |
| explicit_hex8_ss | lumped_matrix | base-t1 | 5 | 50 | 15.2 µs [12.5 µs, 16.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | lumped_matrix | base-t8 | 5 | 50 | 21.5 µs [19.3 µs, 23.4 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.447 [1.416, 1.540] | same |
| explicit_hex8_ss | lumped_matrix | head-t1 | 5 | 50 | 16.6 µs [14.8 µs, 18.0 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.116 [0.916, 1.432] | same |
| explicit_hex8_ss | lumped_matrix | head-t8 | 5 | 50 | 12.4 µs [12.1 µs, 15.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.926 [0.760, 1.094] | same |
| explicit_hex8_ss | corrector | base-t1 | 5 | 50 | 8.5 µs [7.4 µs, 8.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | corrector | base-t8 | 5 | 50 | 8.1 µs [7.6 µs, 8.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.002 [0.855, 1.075] | same |
| explicit_hex8_ss | corrector | head-t1 | 5 | 50 | 13.1 µs [11.3 µs, 16.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.737 [1.328, 1.796] | same |
| explicit_hex8_ss | corrector | head-t8 | 5 | 50 | 12.8 µs [10.2 µs, 13.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.549 [1.204, 1.747] | same |
| explicit_hex8_ss | step_hooks | base-t1 | 5 | 100 | 161.2 µs [160.6 µs, 166.8 µs] | 1.6 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | step_hooks | base-t8 | 5 | 100 | 170.8 µs [158.9 µs, 173.4 µs] | 1.7 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 1.042 [0.989, 1.078] | same |
| explicit_hex8_ss | step_hooks | head-t1 | 5 | 100 | 167.3 µs [166.2 µs, 173.7 µs] | 1.7 µs [1.7 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 1.034 [1.026, 1.060] | same |
| explicit_hex8_ss | step_hooks | head-t8 | 5 | 100 | 184.2 µs [168.8 µs, 194.8 µs] | 1.8 µs [1.7 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.114 [1.049, 1.189] | same |
| explicit_hex8_ss | step_rest | base-t1 | 5 | 50 | 96.40 ms [95.11 ms, 96.97 ms] | 1.93 ms [1.90 ms, 1.94 ms] | 17.4 [17.2, 17.5] | – | same |
| explicit_hex8_ss | step_rest | base-t8 | 5 | 50 | 95.59 ms [93.59 ms, 97.11 ms] | 1.91 ms [1.87 ms, 1.94 ms] | 17.3 [16.9, 17.6] | 0.992 [0.984, 1.002] | same |
| explicit_hex8_ss | step_rest | head-t1 | 5 | 50 | 105.01 ms [103.77 ms, 105.84 ms] | 2.10 ms [2.08 ms, 2.12 ms] | 19.0 [18.8, 19.1] | 1.091 [1.085, 1.097] | same |
| explicit_hex8_ss | step_rest | head-t8 | 5 | 50 | 54.28 ms [53.54 ms, 54.73 ms] | 1.09 ms [1.07 ms, 1.09 ms] | 9.8 [9.7, 9.9] | 0.563 [0.560, 0.572] | same |
| explicit_hex8_ss | internal_forces | base-t1 | 5 | 10 | 278.16 ms [273.38 ms, 280.58 ms] | 27.82 ms [27.34 ms, 28.06 ms] | 251.5 [247.2, 253.7] | – | same |
| explicit_hex8_ss | internal_forces | base-t8 | 5 | 10 | 89.49 ms [87.33 ms, 90.33 ms] | 8.95 ms [8.73 ms, 9.03 ms] | 80.9 [79.0, 81.7] | 0.322 [0.311, 0.326] | same |
| explicit_hex8_ss | internal_forces | head-t1 | 5 | 10 | 277.97 ms [276.34 ms, 278.94 ms] | 27.80 ms [27.63 ms, 27.89 ms] | 251.3 [249.9, 252.2] | 1.002 [0.992, 1.011] | same |
| explicit_hex8_ss | internal_forces | head-t8 | 5 | 10 | 87.99 ms [87.52 ms, 88.28 ms] | 8.80 ms [8.75 ms, 8.83 ms] | 79.6 [79.1, 79.8] | 0.316 [0.312, 0.323] | same |
| explicit_hex8_ss | stresses | base-t1 | 5 | 10 | 173.68 ms [169.43 ms, 181.69 ms] | 17.37 ms [16.94 ms, 18.17 ms] | 157.0 [153.2, 164.3] | – | same |
| explicit_hex8_ss | stresses | base-t8 | 5 | 10 | 100.42 ms [98.54 ms, 102.25 ms] | 10.04 ms [9.85 ms, 10.22 ms] | 90.8 [89.1, 92.5] | 0.582 [0.553, 0.589] | same |
| explicit_hex8_ss | stresses | head-t1 | 5 | 10 | 174.49 ms [169.13 ms, 180.06 ms] | 17.45 ms [16.91 ms, 18.01 ms] | 157.8 [152.9, 162.8] | 0.991 [0.974, 1.027] | same |
| explicit_hex8_ss | stresses | head-t8 | 5 | 10 | 105.72 ms [101.04 ms, 109.30 ms] | 10.57 ms [10.10 ms, 10.93 ms] | 95.6 [91.4, 98.8] | 0.602 [0.582, 0.628] | same |
| explicit_hex8_ss | energies | base-t1 | 5 | 10 | 49.12 ms [48.94 ms, 49.94 ms] | 4.91 ms [4.89 ms, 4.99 ms] | 44.4 [44.3, 45.2] | – | same |
| explicit_hex8_ss | energies | base-t8 | 5 | 10 | 49.38 ms [49.09 ms, 49.99 ms] | 4.94 ms [4.91 ms, 5.00 ms] | 44.7 [44.4, 45.2] | 1.003 [0.992, 1.018] | same |
| explicit_hex8_ss | energies | head-t1 | 5 | 10 | 49.79 ms [48.54 ms, 49.85 ms] | 4.98 ms [4.85 ms, 4.99 ms] | 45.0 [43.9, 45.1] | 0.997 [0.992, 1.015] | same |
| explicit_hex8_ss | energies | head-t8 | 5 | 10 | 48.44 ms [48.17 ms, 48.63 ms] | 4.84 ms [4.82 ms, 4.86 ms] | 43.8 [43.6, 44.0] | 0.984 [0.970, 0.987] | same |
| explicit_marigo | mesh | base-t1 | 5 | 1 | 2.12 ms [2.10 ms, 2.15 ms] | 2.12 ms [2.10 ms, 2.15 ms] | 20.1 [19.9, 20.4] | – | same |
| explicit_marigo | mesh | base-t8 | 5 | 1 | 2.11 ms [2.02 ms, 2.19 ms] | 2.11 ms [2.02 ms, 2.19 ms] | 20.0 [19.2, 20.8] | 0.998 [0.944, 1.036] | same |
| explicit_marigo | mesh | head-t1 | 5 | 1 | 2.05 ms [2.00 ms, 2.08 ms] | 2.05 ms [2.00 ms, 2.08 ms] | 19.5 [18.9, 19.7] | 0.964 [0.948, 0.985] | same |
| explicit_marigo | mesh | head-t8 | 5 | 1 | 2.07 ms [2.02 ms, 2.18 ms] | 2.07 ms [2.02 ms, 2.18 ms] | 19.6 [19.2, 20.7] | 0.983 [0.947, 1.033] | same |
| explicit_marigo | init | base-t1 | 5 | 1 | 32.74 ms [32.48 ms, 33.09 ms] | 32.74 ms [32.48 ms, 33.09 ms] | 310.5 [308.0, 313.8] | – | same |
| explicit_marigo | init | base-t8 | 5 | 1 | 32.15 ms [31.60 ms, 33.89 ms] | 32.15 ms [31.60 ms, 33.89 ms] | 304.9 [299.7, 321.4] | 0.982 [0.973, 1.033] | same |
| explicit_marigo | init | head-t1 | 5 | 1 | 32.31 ms [31.67 ms, 33.10 ms] | 32.31 ms [31.67 ms, 33.10 ms] | 306.4 [300.3, 313.9] | 0.984 [0.967, 1.015] | same |
| explicit_marigo | init | head-t8 | 5 | 1 | 32.41 ms [31.48 ms, 33.41 ms] | 32.41 ms [31.48 ms, 33.41 ms] | 307.3 [298.5, 316.8] | 0.986 [0.969, 1.018] | same |
| explicit_marigo | stable_time_step | base-t1 | 5 | 5 | 21.04 ms [20.93 ms, 21.37 ms] | 4.21 ms [4.19 ms, 4.27 ms] | 39.9 [39.7, 40.5] | – | same |
| explicit_marigo | stable_time_step | base-t8 | 5 | 5 | 21.22 ms [21.01 ms, 21.46 ms] | 4.24 ms [4.20 ms, 4.29 ms] | 40.2 [39.8, 40.7] | 1.006 [1.004, 1.018] | same |
| explicit_marigo | stable_time_step | head-t1 | 5 | 5 | 21.05 ms [21.01 ms, 21.12 ms] | 4.21 ms [4.20 ms, 4.22 ms] | 39.9 [39.8, 40.1] | 1.003 [0.985, 1.008] | same |
| explicit_marigo | stable_time_step | head-t8 | 5 | 5 | 21.10 ms [20.98 ms, 21.53 ms] | 4.22 ms [4.20 ms, 4.31 ms] | 40.0 [39.8, 40.8] | 1.004 [0.984, 1.029] | same |
| explicit_marigo | solve_step | base-t1 | 5 | 200 | 1.523 s [1.515 s, 1.532 s] | 7.61 ms [7.57 ms, 7.66 ms] | 72.2 [71.8, 72.6] | – | same |
| explicit_marigo | solve_step | base-t8 | 5 | 200 | 634.47 ms [625.76 ms, 642.25 ms] | 3.17 ms [3.13 ms, 3.21 ms] | 30.1 [29.7, 30.5] | 0.415 [0.411, 0.422] | same |
| explicit_marigo | solve_step | head-t1 | 5 | 200 | 1.535 s [1.531 s, 1.542 s] | 7.68 ms [7.65 ms, 7.71 ms] | 72.8 [72.6, 73.1] | 1.008 [1.003, 1.011] | same |
| explicit_marigo | solve_step | head-t8 | 5 | 200 | 493.65 ms [481.67 ms, 503.40 ms] | 2.47 ms [2.41 ms, 2.52 ms] | 23.4 [22.8, 23.9] | 0.322 [0.318, 0.331] | same |
| explicit_marigo | predictor | base-t1 | 5 | 200 | 11.9 µs [9.7 µs, 23.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | predictor | base-t8 | 5 | 200 | 18.5 µs [6.2 µs, 21.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.315 [0.645, 1.990] | same |
| explicit_marigo | predictor | head-t1 | 5 | 200 | 14.7 µs [13.3 µs, 27.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.256 [0.574, 2.804] | same |
| explicit_marigo | predictor | head-t8 | 5 | 200 | 24.4 µs [20.5 µs, 27.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 2.047 [0.886, 2.718] | same |
| explicit_marigo | residual | base-t1 | 5 | 200 | 1.131 s [1.125 s, 1.139 s] | 5.65 ms [5.63 ms, 5.70 ms] | 53.6 [53.4, 54.0] | – | same |
| explicit_marigo | residual | base-t8 | 5 | 200 | 352.88 ms [347.25 ms, 354.13 ms] | 1.76 ms [1.74 ms, 1.77 ms] | 16.7 [16.5, 16.8] | 0.311 [0.309, 0.313] | same |
| explicit_marigo | residual | head-t1 | 5 | 200 | 1.136 s [1.132 s, 1.142 s] | 5.68 ms [5.66 ms, 5.71 ms] | 53.9 [53.7, 54.2] | 1.005 [0.998, 1.007] | same |
| explicit_marigo | residual | head-t8 | 5 | 200 | 325.35 ms [317.28 ms, 333.28 ms] | 1.63 ms [1.59 ms, 1.67 ms] | 15.4 [15.0, 15.8] | 0.287 [0.282, 0.295] | same |
| explicit_marigo | lumped_matrix | base-t1 | 5 | 200 | 36.1 µs [31.2 µs, 42.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | lumped_matrix | base-t8 | 5 | 200 | 43.3 µs [37.4 µs, 67.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.194 [0.874, 1.954] | same |
| explicit_marigo | lumped_matrix | head-t1 | 5 | 200 | 33.1 µs [32.0 µs, 35.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.931 [0.801, 1.047] | same |
| explicit_marigo | lumped_matrix | head-t8 | 5 | 200 | 34.4 µs [33.3 µs, 43.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.955 [0.779, 1.403] | same |
| explicit_marigo | corrector | base-t1 | 5 | 200 | 20.6 µs [18.5 µs, 27.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | corrector | base-t8 | 5 | 200 | 29.8 µs [28.9 µs, 32.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.470 [1.160, 1.598] | same |
| explicit_marigo | corrector | head-t1 | 5 | 200 | 19.4 µs [18.9 µs, 24.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.942 [0.692, 1.325] | same |
| explicit_marigo | corrector | head-t8 | 5 | 200 | 32.9 µs [24.3 µs, 46.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.316 [1.224, 2.239] | same |
| explicit_marigo | step_hooks | base-t1 | 5 | 400 | 326.90 ms [324.86 ms, 331.23 ms] | 817.2 µs [812.1 µs, 828.1 µs] | 7.7 [7.7, 7.9] | – | same |
| explicit_marigo | step_hooks | base-t8 | 5 | 400 | 204.86 ms [201.18 ms, 209.74 ms] | 512.2 µs [502.9 µs, 524.4 µs] | 4.9 [4.8, 5.0] | 0.626 [0.618, 0.642] | same |
| explicit_marigo | step_hooks | head-t1 | 5 | 400 | 330.24 ms [328.94 ms, 334.38 ms] | 825.6 µs [822.3 µs, 835.9 µs] | 7.8 [7.8, 7.9] | 1.013 [0.997, 1.023] | same |
| explicit_marigo | step_hooks | head-t8 | 5 | 400 | 123.41 ms [121.12 ms, 124.62 ms] | 308.5 µs [302.8 µs, 311.5 µs] | 2.9 [2.9, 3.0] | 0.374 [0.373, 0.381] | same |
| explicit_marigo | step_rest | base-t1 | 5 | 200 | 64.46 ms [63.70 ms, 65.19 ms] | 322.3 µs [318.5 µs, 326.0 µs] | 3.1 [3.0, 3.1] | – | same |
| explicit_marigo | step_rest | base-t8 | 5 | 200 | 76.21 ms [73.50 ms, 77.26 ms] | 381.1 µs [367.5 µs, 386.3 µs] | 3.6 [3.5, 3.7] | 1.178 [1.127, 1.200] | same |
| explicit_marigo | step_rest | head-t1 | 5 | 200 | 67.95 ms [67.57 ms, 69.06 ms] | 339.8 µs [337.8 µs, 345.3 µs] | 3.2 [3.2, 3.3] | 1.067 [1.041, 1.071] | same |
| explicit_marigo | step_rest | head-t8 | 5 | 200 | 44.04 ms [42.26 ms, 44.44 ms] | 220.2 µs [211.3 µs, 222.2 µs] | 2.1 [2.0, 2.1] | 0.676 [0.663, 0.689] | same |
| explicit_marigo | internal_forces | base-t1 | 5 | 40 | 223.85 ms [222.66 ms, 225.11 ms] | 5.60 ms [5.57 ms, 5.63 ms] | 53.1 [52.8, 53.4] | – | same |
| explicit_marigo | internal_forces | base-t8 | 5 | 40 | 46.54 ms [45.09 ms, 46.96 ms] | 1.16 ms [1.13 ms, 1.17 ms] | 11.0 [10.7, 11.1] | 0.208 [0.202, 0.211] | same |
| explicit_marigo | internal_forces | head-t1 | 5 | 40 | 223.18 ms [221.78 ms, 223.97 ms] | 5.58 ms [5.54 ms, 5.60 ms] | 52.9 [52.6, 53.1] | 0.995 [0.991, 1.002] | same |
| explicit_marigo | internal_forces | head-t8 | 5 | 40 | 43.28 ms [41.57 ms, 43.68 ms] | 1.08 ms [1.04 ms, 1.09 ms] | 10.3 [9.9, 10.4] | 0.193 [0.186, 0.195] | same |
| explicit_marigo | stresses | base-t1 | 5 | 40 | 141.11 ms [140.14 ms, 141.44 ms] | 3.53 ms [3.50 ms, 3.54 ms] | 33.5 [33.2, 33.5] | – | same |
| explicit_marigo | stresses | base-t8 | 5 | 40 | 110.58 ms [109.20 ms, 111.15 ms] | 2.76 ms [2.73 ms, 2.78 ms] | 26.2 [25.9, 26.4] | 0.783 [0.779, 0.788] | same |
| explicit_marigo | stresses | head-t1 | 5 | 40 | 141.28 ms [140.09 ms, 141.63 ms] | 3.53 ms [3.50 ms, 3.54 ms] | 33.5 [33.2, 33.6] | 1.000 [0.999, 1.003] | same |
| explicit_marigo | stresses | head-t8 | 5 | 40 | 109.03 ms [107.37 ms, 110.01 ms] | 2.73 ms [2.68 ms, 2.75 ms] | 25.8 [25.5, 26.1] | 0.771 [0.766, 0.780] | same |
| explicit_marigo | energies | base-t1 | 5 | 40 | 60.41 ms [59.09 ms, 60.53 ms] | 1.51 ms [1.48 ms, 1.51 ms] | 14.3 [14.0, 14.3] | – | same |
| explicit_marigo | energies | base-t8 | 5 | 40 | 60.23 ms [59.72 ms, 60.37 ms] | 1.51 ms [1.49 ms, 1.51 ms] | 14.3 [14.2, 14.3] | 0.998 [0.987, 1.019] | same |
| explicit_marigo | energies | head-t1 | 5 | 40 | 62.17 ms [61.93 ms, 63.02 ms] | 1.55 ms [1.55 ms, 1.58 ms] | 14.7 [14.7, 14.9] | 1.029 [1.024, 1.067] | same |
| explicit_marigo | energies | head-t8 | 5 | 40 | 45.90 ms [44.70 ms, 46.86 ms] | 1.15 ms [1.12 ms, 1.17 ms] | 10.9 [10.6, 11.1] | 0.759 [0.739, 0.793] | same |
| explicit_maxwell | mesh | base-t1 | 5 | 1 | 2.20 ms [2.11 ms, 2.28 ms] | 2.20 ms [2.11 ms, 2.28 ms] | 20.9 [20.0, 21.6] | – | same |
| explicit_maxwell | mesh | base-t8 | 5 | 1 | 2.17 ms [2.03 ms, 2.17 ms] | 2.17 ms [2.03 ms, 2.17 ms] | 20.5 [19.3, 20.6] | 0.971 [0.951, 0.994] | same |
| explicit_maxwell | mesh | head-t1 | 5 | 1 | 2.08 ms [2.03 ms, 2.13 ms] | 2.08 ms [2.03 ms, 2.13 ms] | 19.7 [19.3, 20.2] | 0.948 [0.917, 0.966] | same |
| explicit_maxwell | mesh | head-t8 | 5 | 1 | 2.08 ms [2.06 ms, 2.25 ms] | 2.08 ms [2.06 ms, 2.25 ms] | 19.8 [19.5, 21.3] | 0.946 [0.908, 1.061] | same |
| explicit_maxwell | init | base-t1 | 5 | 1 | 37.78 ms [36.47 ms, 38.08 ms] | 37.78 ms [36.47 ms, 38.08 ms] | 358.3 [345.9, 361.1] | – | same |
| explicit_maxwell | init | base-t8 | 5 | 1 | 37.79 ms [37.08 ms, 38.41 ms] | 37.79 ms [37.08 ms, 38.41 ms] | 358.4 [351.6, 364.3] | 1.006 [0.991, 1.017] | same |
| explicit_maxwell | init | head-t1 | 5 | 1 | 37.06 ms [35.89 ms, 37.68 ms] | 37.06 ms [35.89 ms, 37.68 ms] | 351.4 [340.3, 357.3] | 0.988 [0.945, 1.007] | same |
| explicit_maxwell | init | head-t8 | 5 | 1 | 37.19 ms [36.35 ms, 37.81 ms] | 37.19 ms [36.35 ms, 37.81 ms] | 352.6 [344.7, 358.5] | 0.978 [0.968, 1.037] | same |
| explicit_maxwell | stable_time_step | base-t1 | 5 | 5 | 21.04 ms [20.97 ms, 21.50 ms] | 4.21 ms [4.19 ms, 4.30 ms] | 39.9 [39.8, 40.8] | – | same |
| explicit_maxwell | stable_time_step | base-t8 | 5 | 5 | 21.01 ms [20.98 ms, 21.04 ms] | 4.20 ms [4.20 ms, 4.21 ms] | 39.8 [39.8, 39.9] | 1.000 [0.977, 1.003] | same |
| explicit_maxwell | stable_time_step | head-t1 | 5 | 5 | 21.11 ms [21.01 ms, 21.17 ms] | 4.22 ms [4.20 ms, 4.23 ms] | 40.0 [39.9, 40.1] | 1.004 [0.978, 1.006] | same |
| explicit_maxwell | stable_time_step | head-t8 | 5 | 5 | 21.10 ms [21.03 ms, 21.20 ms] | 4.22 ms [4.21 ms, 4.24 ms] | 40.0 [39.9, 40.2] | 1.006 [0.978, 1.007] | same |
| explicit_maxwell | solve_step | base-t1 | 5 | 40 | 488.44 ms [485.04 ms, 490.44 ms] | 12.21 ms [12.13 ms, 12.26 ms] | 115.8 [115.0, 116.3] | – | same |
| explicit_maxwell | solve_step | base-t8 | 5 | 40 | 184.54 ms [182.78 ms, 191.80 ms] | 4.61 ms [4.57 ms, 4.80 ms] | 43.7 [43.3, 45.5] | 0.378 [0.374, 0.393] | same |
| explicit_maxwell | solve_step | head-t1 | 5 | 40 | 491.84 ms [487.74 ms, 493.06 ms] | 12.30 ms [12.19 ms, 12.33 ms] | 116.6 [115.6, 116.9] | 1.006 [1.001, 1.009] | same |
| explicit_maxwell | solve_step | head-t8 | 5 | 40 | 169.85 ms [168.28 ms, 170.97 ms] | 4.25 ms [4.21 ms, 4.27 ms] | 40.3 [39.9, 40.5] | 0.347 [0.346, 0.350] | same |
| explicit_maxwell | predictor | base-t1 | 5 | 40 | 2.2 µs [2.0 µs, 4.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | predictor | base-t8 | 5 | 40 | 4.4 µs [1.4 µs, 4.9 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.950 [0.620, 2.508] | same |
| explicit_maxwell | predictor | head-t1 | 5 | 40 | 3.9 µs [3.5 µs, 5.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.902 [0.759, 2.406] | same |
| explicit_maxwell | predictor | head-t8 | 5 | 40 | 6.1 µs [5.7 µs, 8.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.542 [1.324, 3.984] | same |
| explicit_maxwell | residual | base-t1 | 5 | 40 | 232.15 ms [230.14 ms, 233.58 ms] | 5.80 ms [5.75 ms, 5.84 ms] | 55.0 [54.6, 55.4] | – | same |
| explicit_maxwell | residual | base-t8 | 5 | 40 | 90.43 ms [89.93 ms, 98.45 ms] | 2.26 ms [2.25 ms, 2.46 ms] | 21.4 [21.3, 23.3] | 0.387 [0.387, 0.425] | same |
| explicit_maxwell | residual | head-t1 | 5 | 40 | 232.80 ms [231.31 ms, 234.23 ms] | 5.82 ms [5.78 ms, 5.86 ms] | 55.2 [54.8, 55.5] | 1.003 [0.997, 1.009] | same |
| explicit_maxwell | residual | head-t8 | 5 | 40 | 85.00 ms [83.91 ms, 86.47 ms] | 2.13 ms [2.10 ms, 2.16 ms] | 20.2 [19.9, 20.5] | 0.366 [0.364, 0.373] | same |
| explicit_maxwell | lumped_matrix | base-t1 | 5 | 40 | 7.5 µs [5.8 µs, 7.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | lumped_matrix | base-t8 | 5 | 40 | 9.2 µs [7.9 µs, 11.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.311 [1.193, 1.421] | same |
| explicit_maxwell | lumped_matrix | head-t1 | 5 | 40 | 8.6 µs [7.7 µs, 8.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.149 [1.015, 1.542] | same |
| explicit_maxwell | lumped_matrix | head-t8 | 5 | 40 | 7.7 µs [7.2 µs, 11.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.094 [0.917, 1.512] | same |
| explicit_maxwell | corrector | base-t1 | 5 | 40 | 4.0 µs [3.4 µs, 5.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | corrector | base-t8 | 5 | 40 | 6.1 µs [5.9 µs, 6.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.535 [1.085, 1.845] | same |
| explicit_maxwell | corrector | head-t1 | 5 | 40 | 4.1 µs [3.4 µs, 5.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.022 [0.931, 1.227] | same |
| explicit_maxwell | corrector | head-t8 | 5 | 40 | 5.9 µs [5.4 µs, 7.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.367 [1.070, 2.028] | same |
| explicit_maxwell | step_hooks | base-t1 | 5 | 80 | 242.45 ms [241.42 ms, 242.93 ms] | 3.03 ms [3.02 ms, 3.04 ms] | 28.7 [28.6, 28.8] | – | same |
| explicit_maxwell | step_hooks | base-t8 | 5 | 80 | 77.69 ms [76.88 ms, 80.27 ms] | 971.2 µs [961.1 µs, 1.00 ms] | 9.2 [9.1, 9.5] | 0.320 [0.317, 0.332] | same |
| explicit_maxwell | step_hooks | head-t1 | 5 | 80 | 243.81 ms [242.20 ms, 245.34 ms] | 3.05 ms [3.03 ms, 3.07 ms] | 28.9 [28.7, 29.1] | 1.006 [1.000, 1.010] | same |
| explicit_maxwell | step_hooks | head-t8 | 5 | 80 | 74.91 ms [74.29 ms, 75.57 ms] | 936.4 µs [928.6 µs, 944.6 µs] | 8.9 [8.8, 9.0] | 0.310 [0.307, 0.311] | same |
| explicit_maxwell | step_rest | base-t1 | 5 | 40 | 13.66 ms [13.31 ms, 13.75 ms] | 341.5 µs [332.7 µs, 343.7 µs] | 3.2 [3.2, 3.3] | – | same |
| explicit_maxwell | step_rest | base-t8 | 5 | 40 | 15.90 ms [15.34 ms, 16.17 ms] | 397.4 µs [383.4 µs, 404.2 µs] | 3.8 [3.6, 3.8] | 1.160 [1.151, 1.177] | same |
| explicit_maxwell | step_rest | head-t1 | 5 | 40 | 14.24 ms [14.05 ms, 14.82 ms] | 356.0 µs [351.4 µs, 370.6 µs] | 3.4 [3.3, 3.5] | 1.042 [1.033, 1.085] | same |
| explicit_maxwell | step_rest | head-t8 | 5 | 40 | 9.26 ms [9.24 ms, 9.33 ms] | 231.6 µs [231.0 µs, 233.3 µs] | 2.2 [2.2, 2.2] | 0.678 [0.674, 0.694] | same |
| explicit_maxwell | internal_forces | base-t1 | 5 | 8 | 43.44 ms [43.26 ms, 43.58 ms] | 5.43 ms [5.41 ms, 5.45 ms] | 51.5 [51.3, 51.7] | – | same |
| explicit_maxwell | internal_forces | base-t8 | 5 | 8 | 14.16 ms [13.64 ms, 14.46 ms] | 1.77 ms [1.71 ms, 1.81 ms] | 16.8 [16.2, 17.1] | 0.326 [0.315, 0.332] | same |
| explicit_maxwell | internal_forces | head-t1 | 5 | 8 | 43.65 ms [43.10 ms, 43.94 ms] | 5.46 ms [5.39 ms, 5.49 ms] | 51.7 [51.1, 52.1] | 1.003 [0.996, 1.016] | same |
| explicit_maxwell | internal_forces | head-t8 | 5 | 8 | 13.71 ms [13.41 ms, 13.79 ms] | 1.71 ms [1.68 ms, 1.72 ms] | 16.3 [15.9, 16.3] | 0.316 [0.308, 0.319] | same |
| explicit_maxwell | stresses | base-t1 | 5 | 8 | 27.33 ms [27.16 ms, 27.38 ms] | 3.42 ms [3.39 ms, 3.42 ms] | 32.4 [32.2, 32.5] | – | same |
| explicit_maxwell | stresses | base-t8 | 5 | 8 | 21.36 ms [21.16 ms, 21.55 ms] | 2.67 ms [2.64 ms, 2.69 ms] | 25.3 [25.1, 25.5] | 0.782 [0.779, 0.789] | same |
| explicit_maxwell | stresses | head-t1 | 5 | 8 | 27.36 ms [27.08 ms, 27.55 ms] | 3.42 ms [3.39 ms, 3.44 ms] | 32.4 [32.1, 32.7] | 0.999 [0.997, 1.008] | same |
| explicit_maxwell | stresses | head-t8 | 5 | 8 | 21.13 ms [20.91 ms, 21.29 ms] | 2.64 ms [2.61 ms, 2.66 ms] | 25.0 [24.8, 25.2] | 0.773 [0.766, 0.778] | same |
| explicit_maxwell | energies | base-t1 | 5 | 8 | 7.15 ms [7.14 ms, 7.26 ms] | 893.4 µs [892.5 µs, 907.0 µs] | 8.5 [8.5, 8.6] | – | same |
| explicit_maxwell | energies | base-t8 | 5 | 8 | 7.19 ms [7.14 ms, 7.21 ms] | 898.5 µs [892.8 µs, 901.7 µs] | 8.5 [8.5, 8.6] | 1.000 [0.989, 1.007] | same |
| explicit_maxwell | energies | head-t1 | 5 | 8 | 7.48 ms [7.46 ms, 7.54 ms] | 934.9 µs [932.8 µs, 942.5 µs] | 8.9 [8.8, 8.9] | 1.044 [1.030, 1.049] | same |
| explicit_maxwell | energies | head-t8 | 5 | 8 | 7.32 ms [7.31 ms, 7.35 ms] | 914.9 µs [914.3 µs, 919.3 µs] | 8.7 [8.7, 8.7] | 1.024 [1.009, 1.026] | same |
| explicit_mazars | mesh | base-t1 | 5 | 1 | 2.18 ms [2.14 ms, 2.18 ms] | 2.18 ms [2.14 ms, 2.18 ms] | 20.7 [20.2, 20.7] | – | same |
| explicit_mazars | mesh | base-t8 | 5 | 1 | 2.06 ms [2.03 ms, 2.19 ms] | 2.06 ms [2.03 ms, 2.19 ms] | 19.5 [19.3, 20.8] | 0.941 [0.933, 1.026] | same |
| explicit_mazars | mesh | head-t1 | 5 | 1 | 2.07 ms [2.05 ms, 2.09 ms] | 2.07 ms [2.05 ms, 2.09 ms] | 19.6 [19.4, 19.9] | 0.949 [0.937, 0.979] | same |
| explicit_mazars | mesh | head-t8 | 5 | 1 | 2.13 ms [2.06 ms, 2.17 ms] | 2.13 ms [2.06 ms, 2.17 ms] | 20.2 [19.5, 20.6] | 0.973 [0.945, 1.015] | same |
| explicit_mazars | init | base-t1 | 5 | 1 | 32.93 ms [32.68 ms, 33.42 ms] | 32.93 ms [32.68 ms, 33.42 ms] | 312.3 [309.9, 317.0] | – | same |
| explicit_mazars | init | base-t8 | 5 | 1 | 33.44 ms [31.94 ms, 33.96 ms] | 33.44 ms [31.94 ms, 33.96 ms] | 317.1 [302.9, 322.0] | 1.016 [0.965, 1.037] | same |
| explicit_mazars | init | head-t1 | 5 | 1 | 32.10 ms [31.83 ms, 32.31 ms] | 32.10 ms [31.83 ms, 32.31 ms] | 304.4 [301.9, 306.3] | 0.976 [0.953, 0.983] | same |
| explicit_mazars | init | head-t8 | 5 | 1 | 32.66 ms [30.80 ms, 33.47 ms] | 32.66 ms [30.80 ms, 33.47 ms] | 309.7 [292.0, 317.4] | 0.992 [0.939, 1.011] | same |
| explicit_mazars | stable_time_step | base-t1 | 5 | 5 | 20.94 ms [20.90 ms, 21.32 ms] | 4.19 ms [4.18 ms, 4.26 ms] | 39.7 [39.6, 40.4] | – | same |
| explicit_mazars | stable_time_step | base-t8 | 5 | 5 | 20.95 ms [20.91 ms, 21.00 ms] | 4.19 ms [4.18 ms, 4.20 ms] | 39.7 [39.7, 39.8] | 1.002 [0.981, 1.002] | same |
| explicit_mazars | stable_time_step | head-t1 | 5 | 5 | 21.06 ms [20.99 ms, 21.14 ms] | 4.21 ms [4.20 ms, 4.23 ms] | 39.9 [39.8, 40.1] | 1.005 [0.992, 1.006] | same |
| explicit_mazars | stable_time_step | head-t8 | 5 | 5 | 21.12 ms [21.11 ms, 21.15 ms] | 4.22 ms [4.22 ms, 4.23 ms] | 40.1 [40.0, 40.1] | 1.009 [0.991, 1.011] | same |
| explicit_mazars | solve_step | base-t1 | 5 | 80 | 2.677 s [2.674 s, 2.720 s] | 33.46 ms [33.42 ms, 34.01 ms] | 317.3 [316.9, 322.5] | – | same |
| explicit_mazars | solve_step | base-t8 | 5 | 80 | 644.91 ms [634.77 ms, 654.83 ms] | 8.06 ms [7.93 ms, 8.19 ms] | 76.4 [75.2, 77.6] | 0.241 [0.237, 0.245] | same |
| explicit_mazars | solve_step | head-t1 | 5 | 80 | 2.684 s [2.676 s, 2.696 s] | 33.55 ms [33.45 ms, 33.70 ms] | 318.2 [317.2, 319.5] | 1.001 [0.987, 1.008] | same |
| explicit_mazars | solve_step | head-t8 | 5 | 80 | 574.02 ms [569.46 ms, 575.96 ms] | 7.18 ms [7.12 ms, 7.20 ms] | 68.0 [67.5, 68.3] | 0.213 [0.211, 0.215] | same |
| explicit_mazars | predictor | base-t1 | 5 | 80 | 6.5 µs [4.5 µs, 11.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | base-t8 | 5 | 80 | 11.3 µs [9.7 µs, 12.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.694 [0.876, 2.665] | same |
| explicit_mazars | predictor | head-t1 | 5 | 80 | 11.7 µs [11.0 µs, 16.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.687 [1.032, 3.555] | same |
| explicit_mazars | predictor | head-t8 | 5 | 80 | 11.2 µs [9.3 µs, 16.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.340 [0.826, 2.444] | same |
| explicit_mazars | residual | base-t1 | 5 | 80 | 2.514 s [2.513 s, 2.558 s] | 31.43 ms [31.41 ms, 31.98 ms] | 298.0 [297.8, 303.2] | – | same |
| explicit_mazars | residual | base-t8 | 5 | 80 | 532.84 ms [522.23 ms, 538.58 ms] | 6.66 ms [6.53 ms, 6.73 ms] | 63.2 [61.9, 63.8] | 0.211 [0.208, 0.214] | same |
| explicit_mazars | residual | head-t1 | 5 | 80 | 2.520 s [2.512 s, 2.531 s] | 31.49 ms [31.40 ms, 31.64 ms] | 298.7 [297.8, 300.0] | 1.000 [0.985, 1.007] | same |
| explicit_mazars | residual | head-t8 | 5 | 80 | 499.76 ms [496.06 ms, 501.75 ms] | 6.25 ms [6.20 ms, 6.27 ms] | 59.2 [58.8, 59.5] | 0.198 [0.195, 0.200] | same |
| explicit_mazars | lumped_matrix | base-t1 | 5 | 80 | 17.0 µs [15.8 µs, 21.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | base-t8 | 5 | 80 | 19.0 µs [15.4 µs, 20.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.977 [0.941, 1.205] | same |
| explicit_mazars | lumped_matrix | head-t1 | 5 | 80 | 15.9 µs [15.2 µs, 16.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.937 [0.775, 1.009] | same |
| explicit_mazars | lumped_matrix | head-t8 | 5 | 80 | 14.8 µs [12.6 µs, 15.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.850 [0.650, 0.974] | same |
| explicit_mazars | corrector | base-t1 | 5 | 80 | 12.8 µs [12.0 µs, 13.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | base-t8 | 5 | 80 | 13.2 µs [12.8 µs, 19.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.039 [0.999, 1.638] | same |
| explicit_mazars | corrector | head-t1 | 5 | 80 | 21.1 µs [20.7 µs, 22.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.654 [1.569, 1.879] | same |
| explicit_mazars | corrector | head-t8 | 5 | 80 | 18.0 µs [16.3 µs, 20.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.471 [1.238, 1.750] | same |
| explicit_mazars | step_hooks | base-t1 | 5 | 160 | 132.78 ms [131.91 ms, 132.98 ms] | 829.9 µs [824.4 µs, 831.1 µs] | 7.9 [7.8, 7.9] | – | same |
| explicit_mazars | step_hooks | base-t8 | 5 | 160 | 84.61 ms [82.36 ms, 86.42 ms] | 528.8 µs [514.8 µs, 540.1 µs] | 5.0 [4.9, 5.1] | 0.636 [0.620, 0.651] | same |
| explicit_mazars | step_hooks | head-t1 | 5 | 160 | 133.84 ms [133.43 ms, 134.40 ms] | 836.5 µs [833.9 µs, 840.0 µs] | 7.9 [7.9, 8.0] | 1.011 [1.003, 1.012] | same |
| explicit_mazars | step_hooks | head-t8 | 5 | 160 | 55.19 ms [54.56 ms, 55.46 ms] | 345.0 µs [341.0 µs, 346.6 µs] | 3.3 [3.2, 3.3] | 0.416 [0.414, 0.419] | same |
| explicit_mazars | step_rest | base-t1 | 5 | 80 | 29.32 ms [28.95 ms, 29.48 ms] | 366.4 µs [361.8 µs, 368.5 µs] | 3.5 [3.4, 3.5] | – | same |
| explicit_mazars | step_rest | base-t8 | 5 | 80 | 29.26 ms [29.09 ms, 29.98 ms] | 365.8 µs [363.6 µs, 374.8 µs] | 3.5 [3.4, 3.6] | 1.005 [0.993, 1.017] | same |
| explicit_mazars | step_rest | head-t1 | 5 | 80 | 30.12 ms [29.62 ms, 30.53 ms] | 376.5 µs [370.3 µs, 381.6 µs] | 3.6 [3.5, 3.6] | 1.025 [1.023, 1.048] | same |
| explicit_mazars | step_rest | head-t8 | 5 | 80 | 18.51 ms [18.38 ms, 18.59 ms] | 231.3 µs [229.7 µs, 232.4 µs] | 2.2 [2.2, 2.2] | 0.631 [0.627, 0.638] | same |
| explicit_mazars | internal_forces | base-t1 | 5 | 16 | 542.15 ms [539.72 ms, 543.16 ms] | 33.88 ms [33.73 ms, 33.95 ms] | 321.3 [319.9, 321.9] | – | same |
| explicit_mazars | internal_forces | base-t8 | 5 | 16 | 105.72 ms [104.17 ms, 106.04 ms] | 6.61 ms [6.51 ms, 6.63 ms] | 62.7 [61.7, 62.8] | 0.195 [0.193, 0.195] | same |
| explicit_mazars | internal_forces | head-t1 | 5 | 16 | 545.93 ms [543.03 ms, 547.98 ms] | 34.12 ms [33.94 ms, 34.25 ms] | 323.6 [321.8, 324.8] | 1.008 [1.000, 1.011] | same |
| explicit_mazars | internal_forces | head-t8 | 5 | 16 | 100.97 ms [100.24 ms, 101.87 ms] | 6.31 ms [6.26 ms, 6.37 ms] | 59.8 [59.4, 60.4] | 0.186 [0.186, 0.188] | same |
| explicit_mazars | stresses | base-t1 | 5 | 16 | 506.45 ms [505.12 ms, 515.33 ms] | 31.65 ms [31.57 ms, 32.21 ms] | 300.2 [299.4, 305.4] | – | same |
| explicit_mazars | stresses | base-t8 | 5 | 16 | 496.96 ms [493.25 ms, 498.69 ms] | 31.06 ms [30.83 ms, 31.17 ms] | 294.5 [292.3, 295.6] | 0.979 [0.961, 0.985] | same |
| explicit_mazars | stresses | head-t1 | 5 | 16 | 507.44 ms [505.32 ms, 509.41 ms] | 31.72 ms [31.58 ms, 31.84 ms] | 300.7 [299.5, 301.9] | 0.998 [0.989, 1.005] | same |
| explicit_mazars | stresses | head-t8 | 5 | 16 | 499.80 ms [495.34 ms, 502.59 ms] | 31.24 ms [30.96 ms, 31.41 ms] | 296.2 [293.6, 297.9] | 0.980 [0.970, 0.995] | same |
| explicit_mazars | energies | base-t1 | 5 | 16 | 22.77 ms [22.64 ms, 23.38 ms] | 1.42 ms [1.42 ms, 1.46 ms] | 13.5 [13.4, 13.9] | – | same |
| explicit_mazars | energies | base-t8 | 5 | 16 | 22.70 ms [22.38 ms, 22.84 ms] | 1.42 ms [1.40 ms, 1.43 ms] | 13.5 [13.3, 13.5] | 0.992 [0.971, 1.008] | same |
| explicit_mazars | energies | head-t1 | 5 | 16 | 23.61 ms [23.33 ms, 24.10 ms] | 1.48 ms [1.46 ms, 1.51 ms] | 14.0 [13.8, 14.3] | 1.032 [1.015, 1.064] | same |
| explicit_mazars | energies | head-t8 | 5 | 16 | 17.76 ms [17.53 ms, 18.01 ms] | 1.11 ms [1.10 ms, 1.13 ms] | 10.5 [10.4, 10.7] | 0.772 [0.769, 0.796] | same |
| explicit_mazars_non_local | mesh | base-t1 | 5 | 1 | 1.03 ms [1.02 ms, 1.06 ms] | 1.03 ms [1.02 ms, 1.06 ms] | 21.5 [21.3, 22.2] | – | same |
| explicit_mazars_non_local | mesh | base-t8 | 5 | 1 | 1.03 ms [1.01 ms, 1.05 ms] | 1.03 ms [1.01 ms, 1.05 ms] | 21.5 [21.0, 21.8] | 0.987 [0.982, 1.001] | same |
| explicit_mazars_non_local | mesh | head-t1 | 5 | 1 | 996.9 µs [982.4 µs, 1.00 ms] | 996.9 µs [982.4 µs, 1.00 ms] | 20.8 [20.5, 20.9] | 0.962 [0.944, 0.972] | same |
| explicit_mazars_non_local | mesh | head-t8 | 5 | 1 | 1.00 ms [985.1 µs, 1.02 ms] | 1.00 ms [985.1 µs, 1.02 ms] | 20.9 [20.5, 21.2] | 0.965 [0.954, 0.988] | same |
| explicit_mazars_non_local | init | base-t1 | 5 | 1 | 601.73 ms [601.29 ms, 608.48 ms] | 601.73 ms [601.29 ms, 608.48 ms] | 12536.1 [12526.8, 12676.7] | – | same |
| explicit_mazars_non_local | init | base-t8 | 5 | 1 | 602.20 ms [599.59 ms, 609.20 ms] | 602.20 ms [599.59 ms, 609.20 ms] | 12545.9 [12491.4, 12691.7] | 0.999 [0.990, 1.013] | same |
| explicit_mazars_non_local | init | head-t1 | 5 | 1 | 603.82 ms [601.67 ms, 607.85 ms] | 603.82 ms [601.67 ms, 607.85 ms] | 12579.5 [12534.7, 12663.5] | 1.002 [0.999, 1.008] | same |
| explicit_mazars_non_local | init | head-t8 | 5 | 1 | 602.73 ms [600.45 ms, 606.83 ms] | 602.73 ms [600.45 ms, 606.83 ms] | 12556.9 [12509.5, 12642.3] | 1.002 [0.990, 1.007] | same |
| explicit_mazars_non_local | stable_time_step | base-t1 | 5 | 5 | 9.57 ms [9.52 ms, 9.65 ms] | 1.91 ms [1.90 ms, 1.93 ms] | 39.9 [39.7, 40.2] | – | same |
| explicit_mazars_non_local | stable_time_step | base-t8 | 5 | 5 | 9.60 ms [9.56 ms, 9.64 ms] | 1.92 ms [1.91 ms, 1.93 ms] | 40.0 [39.8, 40.1] | 1.001 [0.995, 1.012] | same |
| explicit_mazars_non_local | stable_time_step | head-t1 | 5 | 5 | 9.66 ms [9.59 ms, 9.68 ms] | 1.93 ms [1.92 ms, 1.94 ms] | 40.3 [40.0, 40.3] | 1.007 [0.994, 1.015] | same |
| explicit_mazars_non_local | stable_time_step | head-t8 | 5 | 5 | 9.60 ms [9.58 ms, 9.80 ms] | 1.92 ms [1.92 ms, 1.96 ms] | 40.0 [39.9, 40.9] | 1.006 [0.994, 1.020] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 1.663 s [1.656 s, 1.664 s] | 41.57 ms [41.41 ms, 41.59 ms] | 866.1 [862.6, 866.5] | – | same |
| explicit_mazars_non_local | solve_step | base-t8 | 5 | 40 | 1.346 s [1.340 s, 1.354 s] | 33.65 ms [33.49 ms, 33.86 ms] | 701.0 [697.7, 705.4] | 0.809 [0.807, 0.814] | same |
| explicit_mazars_non_local | solve_step | head-t1 | 5 | 40 | 1.644 s [1.642 s, 1.647 s] | 41.10 ms [41.04 ms, 41.18 ms] | 856.3 [855.1, 857.8] | 0.989 [0.988, 0.991] | same |
| explicit_mazars_non_local | solve_step | head-t8 | 5 | 40 | 609.94 ms [607.70 ms, 623.97 ms] | 15.25 ms [15.19 ms, 15.60 ms] | 317.7 [316.5, 325.0] | 0.367 [0.366, 0.375] | same |
| explicit_mazars_non_local | predictor | base-t1 | 5 | 40 | 2.4 µs [1.5 µs, 4.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | base-t8 | 5 | 40 | 3.8 µs [2.3 µs, 4.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.704 [0.859, 2.515] | same |
| explicit_mazars_non_local | predictor | head-t1 | 5 | 40 | 4.4 µs [3.2 µs, 5.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.499 [1.197, 2.945] | same |
| explicit_mazars_non_local | predictor | head-t8 | 5 | 40 | 2.1 µs [1.7 µs, 6.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.969 [0.408, 2.494] | same |
| explicit_mazars_non_local | residual | base-t1 | 5 | 40 | 1.626 s [1.619 s, 1.626 s] | 40.64 ms [40.48 ms, 40.66 ms] | 846.7 [843.4, 847.0] | – | same |
| explicit_mazars_non_local | residual | base-t8 | 5 | 40 | 1.321 s [1.315 s, 1.329 s] | 33.03 ms [32.88 ms, 33.23 ms] | 688.1 [685.0, 692.3] | 0.813 [0.810, 0.818] | same |
| explicit_mazars_non_local | residual | head-t1 | 5 | 40 | 1.606 s [1.604 s, 1.609 s] | 40.15 ms [40.11 ms, 40.23 ms] | 836.4 [835.5, 838.1] | 0.989 [0.987, 0.991] | same |
| explicit_mazars_non_local | residual | head-t8 | 5 | 40 | 595.08 ms [592.93 ms, 609.00 ms] | 14.88 ms [14.82 ms, 15.23 ms] | 309.9 [308.8, 317.2] | 0.366 [0.365, 0.374] | same |
| explicit_mazars_non_local | lumped_matrix | base-t1 | 5 | 40 | 6.1 µs [6.0 µs, 7.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | base-t8 | 5 | 40 | 6.6 µs [5.2 µs, 8.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.066 [0.875, 1.090] | same |
| explicit_mazars_non_local | lumped_matrix | head-t1 | 5 | 40 | 6.4 µs [5.5 µs, 6.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.917 [0.796, 1.069] | same |
| explicit_mazars_non_local | lumped_matrix | head-t8 | 5 | 40 | 6.1 µs [5.8 µs, 7.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.964 [0.774, 1.263] | same |
| explicit_mazars_non_local | corrector | base-t1 | 5 | 40 | 5.9 µs [5.4 µs, 6.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | base-t8 | 5 | 40 | 6.0 µs [5.6 µs, 6.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.993 [0.823, 1.112] | same |
| explicit_mazars_non_local | corrector | head-t1 | 5 | 40 | 6.5 µs [5.8 µs, 7.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.082 [0.928, 1.198] | same |
| explicit_mazars_non_local | corrector | head-t8 | 5 | 40 | 9.0 µs [8.2 µs, 9.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.520 [1.314, 1.606] | same |
| explicit_mazars_non_local | step_hooks | base-t1 | 5 | 80 | 29.51 ms [29.39 ms, 29.56 ms] | 368.8 µs [367.4 µs, 369.5 µs] | 7.7 [7.7, 7.7] | – | same |
| explicit_mazars_non_local | step_hooks | base-t8 | 5 | 80 | 16.68 ms [16.43 ms, 17.18 ms] | 208.5 µs [205.3 µs, 214.7 µs] | 4.3 [4.3, 4.5] | 0.565 [0.558, 0.582] | same |
| explicit_mazars_non_local | step_hooks | head-t1 | 5 | 80 | 29.84 ms [29.62 ms, 30.37 ms] | 372.9 µs [370.2 µs, 379.6 µs] | 7.8 [7.7, 7.9] | 1.010 [1.008, 1.030] | same |
| explicit_mazars_non_local | step_hooks | head-t8 | 5 | 80 | 10.07 ms [10.04 ms, 10.19 ms] | 125.9 µs [125.5 µs, 127.4 µs] | 2.6 [2.6, 2.7] | 0.342 [0.341, 0.345] | same |
| explicit_mazars_non_local | step_rest | base-t1 | 5 | 40 | 7.64 ms [7.51 ms, 7.77 ms] | 191.0 µs [187.8 µs, 194.3 µs] | 4.0 [3.9, 4.0] | – | same |
| explicit_mazars_non_local | step_rest | base-t8 | 5 | 40 | 7.80 ms [7.78 ms, 7.91 ms] | 195.1 µs [194.6 µs, 197.8 µs] | 4.1 [4.1, 4.1] | 1.022 [1.005, 1.039] | same |
| explicit_mazars_non_local | step_rest | head-t1 | 5 | 40 | 7.87 ms [7.80 ms, 8.00 ms] | 196.8 µs [195.0 µs, 200.1 µs] | 4.1 [4.1, 4.2] | 1.033 [1.010, 1.040] | same |
| explicit_mazars_non_local | step_rest | head-t8 | 5 | 40 | 4.68 ms [4.63 ms, 4.80 ms] | 117.1 µs [115.7 µs, 119.9 µs] | 2.4 [2.4, 2.5] | 0.614 [0.605, 0.628] | same |
| explicit_mazars_non_local | internal_forces | base-t1 | 5 | 8 | 334.43 ms [333.16 ms, 335.21 ms] | 41.80 ms [41.64 ms, 41.90 ms] | 870.9 [867.6, 872.9] | – | same |
| explicit_mazars_non_local | internal_forces | base-t8 | 5 | 8 | 269.85 ms [266.27 ms, 270.48 ms] | 33.73 ms [33.28 ms, 33.81 ms] | 702.7 [693.4, 704.4] | 0.807 [0.796, 0.811] | same |
| explicit_mazars_non_local | internal_forces | head-t1 | 5 | 8 | 331.14 ms [329.60 ms, 331.22 ms] | 41.39 ms [41.20 ms, 41.40 ms] | 862.3 [858.3, 862.6] | 0.990 [0.986, 0.994] | same |
| explicit_mazars_non_local | internal_forces | head-t8 | 5 | 8 | 118.26 ms [117.25 ms, 120.12 ms] | 14.78 ms [14.66 ms, 15.02 ms] | 308.0 [305.3, 312.8] | 0.355 [0.351, 0.359] | same |
| explicit_mazars_non_local | stresses | base-t1 | 5 | 8 | 75.29 ms [75.00 ms, 75.71 ms] | 9.41 ms [9.37 ms, 9.46 ms] | 196.1 [195.3, 197.2] | – | same |
| explicit_mazars_non_local | stresses | base-t8 | 5 | 8 | 72.78 ms [72.64 ms, 73.36 ms] | 9.10 ms [9.08 ms, 9.17 ms] | 189.5 [189.2, 191.1] | 0.969 [0.961, 0.973] | same |
| explicit_mazars_non_local | stresses | head-t1 | 5 | 8 | 75.18 ms [75.17 ms, 75.58 ms] | 9.40 ms [9.40 ms, 9.45 ms] | 195.8 [195.7, 196.8] | 1.001 [0.993, 1.008] | same |
| explicit_mazars_non_local | stresses | head-t8 | 5 | 8 | 72.44 ms [72.16 ms, 72.82 ms] | 9.06 ms [9.02 ms, 9.10 ms] | 188.7 [187.9, 189.6] | 0.962 [0.953, 0.969] | same |
| explicit_mazars_non_local | energies | base-t1 | 5 | 8 | 5.39 ms [5.35 ms, 5.42 ms] | 673.5 µs [668.8 µs, 677.1 µs] | 14.0 [13.9, 14.1] | – | same |
| explicit_mazars_non_local | energies | base-t8 | 5 | 8 | 5.35 ms [5.34 ms, 5.38 ms] | 668.9 µs [666.9 µs, 672.8 µs] | 13.9 [13.9, 14.0] | 0.990 [0.985, 1.006] | same |
| explicit_mazars_non_local | energies | head-t1 | 5 | 8 | 5.54 ms [5.53 ms, 5.55 ms] | 693.0 µs [691.9 µs, 693.7 µs] | 14.4 [14.4, 14.5] | 1.029 [1.025, 1.034] | same |
| explicit_mazars_non_local | energies | head-t8 | 5 | 8 | 4.20 ms [4.08 ms, 4.24 ms] | 524.6 µs [510.0 µs, 529.4 µs] | 10.9 [10.6, 11.0] | 0.775 [0.762, 0.789] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t1 | 5 | 2 | 1.176 s [1.170 s, 1.180 s] | 587.98 ms [585.11 ms, 589.88 ms] | 12249.5 [12189.8, 12289.1] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t8 | 5 | 2 | 1.181 s [1.175 s, 1.183 s] | 590.52 ms [587.27 ms, 591.61 ms] | 12302.5 [12234.7, 12325.3] | 1.003 [0.999, 1.011] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t1 | 5 | 2 | 1.169 s [1.163 s, 1.173 s] | 584.68 ms [581.69 ms, 586.53 ms] | 12180.8 [12118.5, 12219.3] | 0.994 [0.992, 0.998] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t8 | 5 | 2 | 1.175 s [1.164 s, 1.188 s] | 587.41 ms [582.15 ms, 593.93 ms] | 12237.8 [12128.0, 12373.5] | 0.997 [0.991, 1.011] | same |
| explicit_multi_material | mesh | base-t1 | 5 | 1 | 2.18 ms [2.04 ms, 2.29 ms] | 2.18 ms [2.04 ms, 2.29 ms] | 20.7 [19.4, 21.7] | – | same |
| explicit_multi_material | mesh | base-t8 | 5 | 1 | 2.18 ms [2.12 ms, 2.20 ms] | 2.18 ms [2.12 ms, 2.20 ms] | 20.7 [20.1, 20.9] | 0.999 [0.945, 1.068] | same |
| explicit_multi_material | mesh | head-t1 | 5 | 1 | 2.10 ms [2.03 ms, 2.12 ms] | 2.10 ms [2.03 ms, 2.12 ms] | 19.9 [19.3, 20.1] | 0.973 [0.895, 0.999] | same |
| explicit_multi_material | mesh | head-t8 | 5 | 1 | 2.15 ms [2.08 ms, 2.18 ms] | 2.15 ms [2.08 ms, 2.18 ms] | 20.4 [19.7, 20.7] | 0.986 [0.922, 1.067] | same |
| explicit_multi_material | init | base-t1 | 5 | 1 | 34.23 ms [34.01 ms, 35.39 ms] | 34.23 ms [34.01 ms, 35.39 ms] | 324.6 [322.5, 335.6] | – | same |
| explicit_multi_material | init | base-t8 | 5 | 1 | 34.49 ms [34.37 ms, 35.54 ms] | 34.49 ms [34.37 ms, 35.54 ms] | 327.0 [326.0, 337.0] | 1.004 [0.973, 1.043] | same |
| explicit_multi_material | init | head-t1 | 5 | 1 | 33.45 ms [33.25 ms, 34.30 ms] | 33.45 ms [33.25 ms, 34.30 ms] | 317.2 [315.3, 325.3] | 0.976 [0.944, 1.002] | same |
| explicit_multi_material | init | head-t8 | 5 | 1 | 34.50 ms [34.18 ms, 35.29 ms] | 34.50 ms [34.18 ms, 35.29 ms] | 327.2 [324.2, 334.6] | 0.999 [0.984, 1.018] | same |
| explicit_multi_material | stable_time_step | base-t1 | 5 | 5 | 21.01 ms [20.94 ms, 21.07 ms] | 4.20 ms [4.19 ms, 4.21 ms] | 39.8 [39.7, 40.0] | – | same |
| explicit_multi_material | stable_time_step | base-t8 | 5 | 5 | 21.08 ms [20.99 ms, 21.15 ms] | 4.22 ms [4.20 ms, 4.23 ms] | 40.0 [39.8, 40.1] | 1.004 [1.002, 1.005] | same |
| explicit_multi_material | stable_time_step | head-t1 | 5 | 5 | 21.30 ms [21.23 ms, 22.58 ms] | 4.26 ms [4.25 ms, 4.52 ms] | 40.4 [40.3, 42.8] | 1.016 [1.009, 1.071] | same |
| explicit_multi_material | stable_time_step | head-t8 | 5 | 5 | 21.34 ms [21.17 ms, 21.88 ms] | 4.27 ms [4.23 ms, 4.38 ms] | 40.5 [40.1, 41.5] | 1.016 [1.011, 1.038] | same |
| explicit_multi_material | solve_step | base-t1 | 5 | 120 | 1.996 s [1.977 s, 2.001 s] | 16.64 ms [16.47 ms, 16.68 ms] | 157.7 [156.2, 158.1] | – | same |
| explicit_multi_material | solve_step | base-t8 | 5 | 120 | 520.08 ms [509.36 ms, 529.86 ms] | 4.33 ms [4.24 ms, 4.42 ms] | 41.1 [40.3, 41.9] | 0.260 [0.258, 0.265] | same |
| explicit_multi_material | solve_step | head-t1 | 5 | 120 | 2.010 s [1.996 s, 2.062 s] | 16.75 ms [16.64 ms, 17.18 ms] | 158.8 [157.8, 162.9] | 1.005 [1.000, 1.043] | same |
| explicit_multi_material | solve_step | head-t8 | 5 | 120 | 455.51 ms [448.28 ms, 462.72 ms] | 3.80 ms [3.74 ms, 3.86 ms] | 36.0 [35.4, 36.6] | 0.228 [0.227, 0.231] | same |
| explicit_multi_material | predictor | base-t1 | 5 | 120 | 8.2 µs [7.7 µs, 11.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | predictor | base-t8 | 5 | 120 | 10.6 µs [9.8 µs, 12.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.281 [0.925, 1.462] | same |
| explicit_multi_material | predictor | head-t1 | 5 | 120 | 20.8 µs [9.4 µs, 23.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.036 [1.138, 2.940] | same |
| explicit_multi_material | predictor | head-t8 | 5 | 120 | 11.9 µs [5.7 µs, 15.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.086 [0.637, 2.051] | same |
| explicit_multi_material | residual | base-t1 | 5 | 120 | 1.861 s [1.844 s, 1.866 s] | 15.51 ms [15.37 ms, 15.55 ms] | 147.1 [145.7, 147.4] | – | same |
| explicit_multi_material | residual | base-t8 | 5 | 120 | 418.87 ms [410.01 ms, 426.96 ms] | 3.49 ms [3.42 ms, 3.56 ms] | 33.1 [32.4, 33.7] | 0.225 [0.222, 0.229] | same |
| explicit_multi_material | residual | head-t1 | 5 | 120 | 1.873 s [1.859 s, 1.927 s] | 15.61 ms [15.49 ms, 16.05 ms] | 148.0 [146.9, 152.2] | 1.004 [0.999, 1.045] | same |
| explicit_multi_material | residual | head-t8 | 5 | 120 | 388.72 ms [382.15 ms, 395.61 ms] | 3.24 ms [3.18 ms, 3.30 ms] | 30.7 [30.2, 31.3] | 0.209 [0.207, 0.212] | same |
| explicit_multi_material | lumped_matrix | base-t1 | 5 | 120 | 26.6 µs [25.1 µs, 27.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | lumped_matrix | base-t8 | 5 | 120 | 27.9 µs [27.0 µs, 28.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.047 [1.025, 1.083] | same |
| explicit_multi_material | lumped_matrix | head-t1 | 5 | 120 | 32.0 µs [25.0 µs, 35.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.184 [0.993, 1.334] | same |
| explicit_multi_material | lumped_matrix | head-t8 | 5 | 120 | 26.5 µs [24.2 µs, 29.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.985 [0.962, 1.118] | same |
| explicit_multi_material | corrector | base-t1 | 5 | 120 | 18.6 µs [13.0 µs, 19.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | corrector | base-t8 | 5 | 120 | 16.3 µs [13.6 µs, 18.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.929 [0.839, 1.050] | same |
| explicit_multi_material | corrector | head-t1 | 5 | 120 | 18.9 µs [12.0 µs, 19.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.011 [0.645, 1.112] | same |
| explicit_multi_material | corrector | head-t8 | 5 | 120 | 15.3 µs [9.5 µs, 21.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.813 [0.525, 1.688] | same |
| explicit_multi_material | step_hooks | base-t1 | 5 | 240 | 94.56 ms [93.01 ms, 95.43 ms] | 394.0 µs [387.6 µs, 397.6 µs] | 3.7 [3.7, 3.8] | – | same |
| explicit_multi_material | step_hooks | base-t8 | 5 | 240 | 59.53 ms [59.07 ms, 61.48 ms] | 248.0 µs [246.1 µs, 256.2 µs] | 2.4 [2.3, 2.4] | 0.629 [0.626, 0.653] | same |
| explicit_multi_material | step_hooks | head-t1 | 5 | 240 | 94.79 ms [93.40 ms, 95.23 ms] | 395.0 µs [389.2 µs, 396.8 µs] | 3.7 [3.7, 3.8] | 1.004 [0.994, 1.006] | same |
| explicit_multi_material | step_hooks | head-t8 | 5 | 240 | 44.42 ms [43.82 ms, 44.62 ms] | 185.1 µs [182.6 µs, 185.9 µs] | 1.8 [1.7, 1.8] | 0.469 [0.465, 0.472] | same |
| explicit_multi_material | step_rest | base-t1 | 5 | 120 | 40.41 ms [39.36 ms, 40.95 ms] | 336.7 µs [328.0 µs, 341.3 µs] | 3.2 [3.1, 3.2] | – | same |
| explicit_multi_material | step_rest | base-t8 | 5 | 120 | 41.02 ms [39.86 ms, 41.64 ms] | 341.9 µs [332.2 µs, 347.0 µs] | 3.2 [3.2, 3.3] | 1.017 [0.999, 1.026] | same |
| explicit_multi_material | step_rest | head-t1 | 5 | 120 | 42.30 ms [41.64 ms, 42.81 ms] | 352.5 µs [347.0 µs, 356.7 µs] | 3.3 [3.3, 3.4] | 1.058 [1.021, 1.059] | same |
| explicit_multi_material | step_rest | head-t8 | 5 | 120 | 22.11 ms [21.88 ms, 22.25 ms] | 184.2 µs [182.3 µs, 185.4 µs] | 1.7 [1.7, 1.8] | 0.550 [0.540, 0.556] | same |
| explicit_multi_material | internal_forces | base-t1 | 5 | 24 | 372.87 ms [368.91 ms, 374.77 ms] | 15.54 ms [15.37 ms, 15.62 ms] | 147.3 [145.8, 148.1] | – | same |
| explicit_multi_material | internal_forces | base-t8 | 5 | 24 | 73.11 ms [71.48 ms, 73.42 ms] | 3.05 ms [2.98 ms, 3.06 ms] | 28.9 [28.2, 29.0] | 0.195 [0.194, 0.197] | same |
| explicit_multi_material | internal_forces | head-t1 | 5 | 24 | 374.30 ms [368.70 ms, 377.09 ms] | 15.60 ms [15.36 ms, 15.71 ms] | 147.9 [145.7, 149.0] | 0.999 [0.994, 1.011] | same |
| explicit_multi_material | internal_forces | head-t8 | 5 | 24 | 69.37 ms [68.20 ms, 70.21 ms] | 2.89 ms [2.84 ms, 2.93 ms] | 27.4 [26.9, 27.7] | 0.186 [0.185, 0.187] | same |
| explicit_multi_material | stresses | base-t1 | 5 | 24 | 315.32 ms [313.45 ms, 316.51 ms] | 13.14 ms [13.06 ms, 13.19 ms] | 124.6 [123.8, 125.1] | – | same |
| explicit_multi_material | stresses | base-t8 | 5 | 24 | 289.66 ms [287.88 ms, 292.24 ms] | 12.07 ms [11.99 ms, 12.18 ms] | 114.4 [113.7, 115.5] | 0.919 [0.916, 0.925] | same |
| explicit_multi_material | stresses | head-t1 | 5 | 24 | 313.88 ms [313.22 ms, 316.03 ms] | 13.08 ms [13.05 ms, 13.17 ms] | 124.0 [123.8, 124.9] | 0.999 [0.995, 0.999] | same |
| explicit_multi_material | stresses | head-t8 | 5 | 24 | 288.21 ms [285.96 ms, 293.78 ms] | 12.01 ms [11.91 ms, 12.24 ms] | 113.9 [113.0, 116.1] | 0.913 [0.912, 0.932] | same |
| explicit_multi_material | energies | base-t1 | 5 | 24 | 37.16 ms [37.04 ms, 37.37 ms] | 1.55 ms [1.54 ms, 1.56 ms] | 14.7 [14.6, 14.8] | – | same |
| explicit_multi_material | energies | base-t8 | 5 | 24 | 37.45 ms [37.22 ms, 37.61 ms] | 1.56 ms [1.55 ms, 1.57 ms] | 14.8 [14.7, 14.9] | 1.006 [1.001, 1.012] | same |
| explicit_multi_material | energies | head-t1 | 5 | 24 | 37.93 ms [37.76 ms, 38.08 ms] | 1.58 ms [1.57 ms, 1.59 ms] | 15.0 [14.9, 15.0] | 1.020 [1.015, 1.028] | same |
| explicit_multi_material | energies | head-t8 | 5 | 24 | 35.06 ms [34.64 ms, 35.21 ms] | 1.46 ms [1.44 ms, 1.47 ms] | 13.9 [13.7, 13.9] | 0.938 [0.932, 0.950] | same |
| explicit_neohookean | mesh | base-t1 | 5 | 1 | 2.18 ms [2.16 ms, 2.24 ms] | 2.18 ms [2.16 ms, 2.24 ms] | 20.6 [20.5, 21.3] | – | same |
| explicit_neohookean | mesh | base-t8 | 5 | 1 | 2.12 ms [2.09 ms, 2.19 ms] | 2.12 ms [2.09 ms, 2.19 ms] | 20.1 [19.8, 20.7] | 0.975 [0.931, 1.012] | same |
| explicit_neohookean | mesh | head-t1 | 5 | 1 | 2.03 ms [2.03 ms, 2.13 ms] | 2.03 ms [2.03 ms, 2.13 ms] | 19.2 [19.2, 20.2] | 0.933 [0.919, 0.981] | same |
| explicit_neohookean | mesh | head-t8 | 5 | 1 | 2.16 ms [2.03 ms, 2.18 ms] | 2.16 ms [2.03 ms, 2.18 ms] | 20.5 [19.3, 20.7] | 0.976 [0.933, 1.001] | same |
| explicit_neohookean | init | base-t1 | 5 | 1 | 35.35 ms [34.55 ms, 35.93 ms] | 35.35 ms [34.55 ms, 35.93 ms] | 335.2 [327.6, 340.7] | – | same |
| explicit_neohookean | init | base-t8 | 5 | 1 | 34.98 ms [34.60 ms, 36.02 ms] | 34.98 ms [34.60 ms, 36.02 ms] | 331.7 [328.1, 341.5] | 0.989 [0.980, 1.039] | same |
| explicit_neohookean | init | head-t1 | 5 | 1 | 34.12 ms [33.67 ms, 34.28 ms] | 34.12 ms [33.67 ms, 34.28 ms] | 323.5 [319.2, 325.0] | 0.970 [0.937, 0.989] | same |
| explicit_neohookean | init | head-t8 | 5 | 1 | 35.00 ms [34.37 ms, 35.24 ms] | 35.00 ms [34.37 ms, 35.24 ms] | 331.9 [325.9, 334.1] | 0.990 [0.970, 1.016] | same |
| explicit_neohookean | stable_time_step | base-t1 | 5 | 5 | 21.00 ms [20.92 ms, 21.02 ms] | 4.20 ms [4.18 ms, 4.20 ms] | 39.8 [39.7, 39.9] | – | same |
| explicit_neohookean | stable_time_step | base-t8 | 5 | 5 | 21.08 ms [20.97 ms, 21.25 ms] | 4.22 ms [4.19 ms, 4.25 ms] | 40.0 [39.8, 40.3] | 1.007 [1.002, 1.011] | same |
| explicit_neohookean | stable_time_step | head-t1 | 5 | 5 | 21.09 ms [21.02 ms, 21.13 ms] | 4.22 ms [4.20 ms, 4.23 ms] | 40.0 [39.9, 40.1] | 1.005 [1.000, 1.007] | same |
| explicit_neohookean | stable_time_step | head-t8 | 5 | 5 | 21.21 ms [21.13 ms, 23.54 ms] | 4.24 ms [4.23 ms, 4.71 ms] | 40.2 [40.1, 44.6] | 1.013 [1.009, 1.120] | same |
| explicit_neohookean | solve_step | base-t1 | 5 | 100 | 1.059 s [1.040 s, 1.068 s] | 10.59 ms [10.40 ms, 10.68 ms] | 100.4 [98.6, 101.2] | – | same |
| explicit_neohookean | solve_step | base-t8 | 5 | 100 | 261.63 ms [259.45 ms, 265.99 ms] | 2.62 ms [2.59 ms, 2.66 ms] | 24.8 [24.6, 25.2] | 0.251 [0.243, 0.251] | same |
| explicit_neohookean | solve_step | head-t1 | 5 | 100 | 1.057 s [1.050 s, 1.063 s] | 10.57 ms [10.50 ms, 10.63 ms] | 100.2 [99.5, 100.8] | 0.998 [0.992, 1.009] | same |
| explicit_neohookean | solve_step | head-t8 | 5 | 100 | 236.59 ms [234.22 ms, 239.10 ms] | 2.37 ms [2.34 ms, 2.39 ms] | 22.4 [22.2, 22.7] | 0.224 [0.222, 0.226] | same |
| explicit_neohookean | predictor | base-t1 | 5 | 100 | 4.2 µs [2.9 µs, 7.4 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | predictor | base-t8 | 5 | 100 | 7.4 µs [3.9 µs, 15.4 µs] | 0.1 µs [0.0 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.539 [0.640, 4.131] | same |
| explicit_neohookean | predictor | head-t1 | 5 | 100 | 5.5 µs [3.7 µs, 9.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.476 [0.498, 2.557] | same |
| explicit_neohookean | predictor | head-t8 | 5 | 100 | 2.8 µs [2.7 µs, 4.7 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.647 [0.385, 1.621] | same |
| explicit_neohookean | residual | base-t1 | 5 | 100 | 1.027 s [1.009 s, 1.036 s] | 10.27 ms [10.09 ms, 10.36 ms] | 97.4 [95.7, 98.2] | – | same |
| explicit_neohookean | residual | base-t8 | 5 | 100 | 226.28 ms [224.86 ms, 230.09 ms] | 2.26 ms [2.25 ms, 2.30 ms] | 21.5 [21.3, 21.8] | 0.224 [0.217, 0.224] | same |
| explicit_neohookean | residual | head-t1 | 5 | 100 | 1.023 s [1.016 s, 1.029 s] | 10.23 ms [10.16 ms, 10.29 ms] | 97.0 [96.4, 97.6] | 0.996 [0.990, 1.007] | same |
| explicit_neohookean | residual | head-t8 | 5 | 100 | 217.72 ms [215.55 ms, 219.81 ms] | 2.18 ms [2.16 ms, 2.20 ms] | 20.6 [20.4, 20.8] | 0.213 [0.210, 0.215] | same |
| explicit_neohookean | lumped_matrix | base-t1 | 5 | 100 | 16.0 µs [14.7 µs, 17.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | lumped_matrix | base-t8 | 5 | 100 | 18.1 µs [17.0 µs, 22.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.152 [0.970, 1.309] | same |
| explicit_neohookean | lumped_matrix | head-t1 | 5 | 100 | 18.6 µs [17.9 µs, 20.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.118 [1.027, 1.382] | same |
| explicit_neohookean | lumped_matrix | head-t8 | 5 | 100 | 16.3 µs [12.6 µs, 16.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.021 [0.702, 1.130] | same |
| explicit_neohookean | corrector | base-t1 | 5 | 100 | 11.7 µs [10.3 µs, 12.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | corrector | base-t8 | 5 | 100 | 15.6 µs [15.4 µs, 15.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.349 [1.287, 1.491] | same |
| explicit_neohookean | corrector | head-t1 | 5 | 100 | 12.4 µs [9.5 µs, 15.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.009 [0.795, 1.465] | same |
| explicit_neohookean | corrector | head-t8 | 5 | 100 | 14.6 µs [9.5 µs, 15.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.233 [0.833, 1.447] | same |
| explicit_neohookean | step_hooks | base-t1 | 5 | 200 | 263.3 µs [250.3 µs, 300.4 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | step_hooks | base-t8 | 5 | 200 | 279.1 µs [268.4 µs, 298.0 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.066 [0.929, 1.143] | same |
| explicit_neohookean | step_hooks | head-t1 | 5 | 200 | 265.2 µs [251.7 µs, 290.7 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.006 [0.867, 1.104] | same |
| explicit_neohookean | step_hooks | head-t8 | 5 | 200 | 268.7 µs [262.5 µs, 299.3 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.043 [0.895, 1.145] | same |
| explicit_neohookean | step_rest | base-t1 | 5 | 100 | 31.55 ms [30.87 ms, 31.87 ms] | 315.5 µs [308.7 µs, 318.7 µs] | 3.0 [2.9, 3.0] | – | same |
| explicit_neohookean | step_rest | base-t8 | 5 | 100 | 34.79 ms [34.06 ms, 35.42 ms] | 347.9 µs [340.6 µs, 354.2 µs] | 3.3 [3.2, 3.4] | 1.103 [1.082, 1.127] | same |
| explicit_neohookean | step_rest | head-t1 | 5 | 100 | 33.19 ms [32.70 ms, 33.35 ms] | 331.9 µs [327.0 µs, 333.5 µs] | 3.1 [3.1, 3.2] | 1.054 [1.036, 1.066] | same |
| explicit_neohookean | step_rest | head-t8 | 5 | 100 | 18.39 ms [18.08 ms, 18.69 ms] | 183.9 µs [180.8 µs, 186.9 µs] | 1.7 [1.7, 1.8] | 0.586 [0.573, 0.592] | same |
| explicit_neohookean | internal_forces | base-t1 | 5 | 20 | 193.93 ms [190.97 ms, 198.11 ms] | 9.70 ms [9.55 ms, 9.91 ms] | 91.9 [90.5, 93.9] | – | same |
| explicit_neohookean | internal_forces | base-t8 | 5 | 20 | 30.36 ms [30.14 ms, 30.82 ms] | 1.52 ms [1.51 ms, 1.54 ms] | 14.4 [14.3, 14.6] | 0.157 [0.154, 0.159] | same |
| explicit_neohookean | internal_forces | head-t1 | 5 | 20 | 191.89 ms [190.07 ms, 197.39 ms] | 9.59 ms [9.50 ms, 9.87 ms] | 91.0 [90.1, 93.6] | 0.982 [0.959, 1.023] | same |
| explicit_neohookean | internal_forces | head-t8 | 5 | 20 | 29.88 ms [29.42 ms, 30.16 ms] | 1.49 ms [1.47 ms, 1.51 ms] | 14.2 [13.9, 14.3] | 0.154 [0.150, 0.156] | same |
| explicit_neohookean | stresses | base-t1 | 5 | 20 | 127.55 ms [124.58 ms, 130.87 ms] | 6.38 ms [6.23 ms, 6.54 ms] | 60.5 [59.1, 62.0] | – | same |
| explicit_neohookean | stresses | base-t8 | 5 | 20 | 109.95 ms [108.96 ms, 114.33 ms] | 5.50 ms [5.45 ms, 5.72 ms] | 52.1 [51.7, 54.2] | 0.881 [0.840, 0.896] | same |
| explicit_neohookean | stresses | head-t1 | 5 | 20 | 124.53 ms [121.95 ms, 129.24 ms] | 6.23 ms [6.10 ms, 6.46 ms] | 59.0 [57.8, 61.3] | 0.976 [0.932, 1.031] | same |
| explicit_neohookean | stresses | head-t8 | 5 | 20 | 113.50 ms [106.16 ms, 119.39 ms] | 5.67 ms [5.31 ms, 5.97 ms] | 53.8 [50.3, 56.6] | 0.887 [0.852, 0.952] | same |
| explicit_neohookean | energies | base-t1 | 5 | 20 | 124.42 ms [123.38 ms, 128.33 ms] | 6.22 ms [6.17 ms, 6.42 ms] | 59.0 [58.5, 60.8] | – | same |
| explicit_neohookean | energies | base-t8 | 5 | 20 | 124.89 ms [124.39 ms, 127.56 ms] | 6.24 ms [6.22 ms, 6.38 ms] | 59.2 [59.0, 60.5] | 1.004 [0.976, 1.030] | same |
| explicit_neohookean | energies | head-t1 | 5 | 20 | 124.54 ms [122.49 ms, 130.50 ms] | 6.23 ms [6.12 ms, 6.53 ms] | 59.0 [58.1, 61.9] | 1.003 [0.954, 1.058] | same |
| explicit_neohookean | energies | head-t8 | 5 | 20 | 123.16 ms [122.94 ms, 125.60 ms] | 6.16 ms [6.15 ms, 6.28 ms] | 58.4 [58.3, 59.6] | 0.989 [0.979, 0.998] | same |
| explicit_orthotropic | mesh | base-t1 | 5 | 1 | 2.18 ms [2.08 ms, 2.24 ms] | 2.18 ms [2.08 ms, 2.24 ms] | 20.7 [19.7, 21.2] | – | same |
| explicit_orthotropic | mesh | base-t8 | 5 | 1 | 2.15 ms [2.04 ms, 2.17 ms] | 2.15 ms [2.04 ms, 2.17 ms] | 20.3 [19.3, 20.6] | 0.961 [0.936, 1.043] | same |
| explicit_orthotropic | mesh | head-t1 | 5 | 1 | 2.08 ms [2.05 ms, 2.14 ms] | 2.08 ms [2.05 ms, 2.14 ms] | 19.7 [19.4, 20.3] | 0.950 [0.927, 1.027] | same |
| explicit_orthotropic | mesh | head-t8 | 5 | 1 | 2.08 ms [2.05 ms, 2.19 ms] | 2.08 ms [2.05 ms, 2.19 ms] | 19.7 [19.5, 20.8] | 0.972 [0.938, 1.002] | same |
| explicit_orthotropic | init | base-t1 | 5 | 1 | 30.62 ms [30.41 ms, 30.77 ms] | 30.62 ms [30.41 ms, 30.77 ms] | 290.4 [288.4, 291.7] | – | same |
| explicit_orthotropic | init | base-t8 | 5 | 1 | 30.94 ms [30.74 ms, 31.44 ms] | 30.94 ms [30.74 ms, 31.44 ms] | 293.4 [291.5, 298.1] | 1.012 [1.006, 1.023] | same |
| explicit_orthotropic | init | head-t1 | 5 | 1 | 30.49 ms [30.04 ms, 30.77 ms] | 30.49 ms [30.04 ms, 30.77 ms] | 289.1 [284.9, 291.8] | 0.995 [0.978, 1.006] | same |
| explicit_orthotropic | init | head-t8 | 5 | 1 | 30.77 ms [30.02 ms, 30.95 ms] | 30.77 ms [30.02 ms, 30.95 ms] | 291.8 [284.7, 293.5] | 1.005 [0.987, 1.006] | same |
| explicit_orthotropic | stable_time_step | base-t1 | 5 | 5 | 20.98 ms [20.88 ms, 21.03 ms] | 4.20 ms [4.18 ms, 4.21 ms] | 39.8 [39.6, 39.9] | – | same |
| explicit_orthotropic | stable_time_step | base-t8 | 5 | 5 | 21.13 ms [20.89 ms, 21.32 ms] | 4.23 ms [4.18 ms, 4.26 ms] | 40.1 [39.6, 40.4] | 1.006 [1.000, 1.016] | same |
| explicit_orthotropic | stable_time_step | head-t1 | 5 | 5 | 21.07 ms [21.04 ms, 21.14 ms] | 4.21 ms [4.21 ms, 4.23 ms] | 40.0 [39.9, 40.1] | 1.005 [1.000, 1.010] | same |
| explicit_orthotropic | stable_time_step | head-t8 | 5 | 5 | 21.14 ms [21.00 ms, 21.16 ms] | 4.23 ms [4.20 ms, 4.23 ms] | 40.1 [39.8, 40.1] | 1.006 [1.004, 1.011] | same |
| explicit_orthotropic | solve_step | base-t1 | 5 | 200 | 1.004 s [964.03 ms, 1.007 s] | 5.02 ms [4.82 ms, 5.03 ms] | 47.6 [45.7, 47.7] | – | same |
| explicit_orthotropic | solve_step | base-t8 | 5 | 200 | 342.27 ms [335.70 ms, 348.35 ms] | 1.71 ms [1.68 ms, 1.74 ms] | 16.2 [15.9, 16.5] | 0.346 [0.339, 0.357] | same |
| explicit_orthotropic | solve_step | head-t1 | 5 | 200 | 1.008 s [981.05 ms, 1.034 s] | 5.04 ms [4.91 ms, 5.17 ms] | 47.8 [46.5, 49.0] | 1.004 [0.977, 1.064] | same |
| explicit_orthotropic | solve_step | head-t8 | 5 | 200 | 280.03 ms [277.97 ms, 286.74 ms] | 1.40 ms [1.39 ms, 1.43 ms] | 13.3 [13.2, 13.6] | 0.279 [0.277, 0.297] | same |
| explicit_orthotropic | predictor | base-t1 | 5 | 200 | 7.3 µs [5.8 µs, 17.9 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | predictor | base-t8 | 5 | 200 | 14.8 µs [6.0 µs, 20.5 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.825 [0.803, 3.536] | same |
| explicit_orthotropic | predictor | head-t1 | 5 | 200 | 7.6 µs [6.5 µs, 19.1 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.127 [0.366, 2.054] | same |
| explicit_orthotropic | predictor | head-t8 | 5 | 200 | 6.0 µs [5.1 µs, 16.8 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.822 [0.574, 2.404] | same |
| explicit_orthotropic | residual | base-t1 | 5 | 200 | 933.76 ms [900.83 ms, 935.89 ms] | 4.67 ms [4.50 ms, 4.68 ms] | 44.3 [42.7, 44.4] | – | same |
| explicit_orthotropic | residual | base-t8 | 5 | 200 | 271.06 ms [266.45 ms, 278.64 ms] | 1.36 ms [1.33 ms, 1.39 ms] | 12.9 [12.6, 13.2] | 0.296 [0.289, 0.302] | same |
| explicit_orthotropic | residual | head-t1 | 5 | 200 | 934.60 ms [914.19 ms, 958.27 ms] | 4.67 ms [4.57 ms, 4.79 ms] | 44.3 [43.3, 45.4] | 1.002 [0.979, 1.055] | same |
| explicit_orthotropic | residual | head-t8 | 5 | 200 | 243.81 ms [242.57 ms, 250.00 ms] | 1.22 ms [1.21 ms, 1.25 ms] | 11.6 [11.5, 11.9] | 0.261 [0.260, 0.278] | same |
| explicit_orthotropic | lumped_matrix | base-t1 | 5 | 200 | 34.4 µs [31.7 µs, 38.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | lumped_matrix | base-t8 | 5 | 200 | 36.4 µs [34.0 µs, 43.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.033 [0.967, 1.364] | same |
| explicit_orthotropic | lumped_matrix | head-t1 | 5 | 200 | 39.1 µs [35.1 µs, 42.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.115 [0.912, 1.234] | same |
| explicit_orthotropic | lumped_matrix | head-t8 | 5 | 200 | 27.7 µs [23.6 µs, 32.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.804 [0.720, 0.955] | same |
| explicit_orthotropic | corrector | base-t1 | 5 | 200 | 29.1 µs [16.9 µs, 32.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | corrector | base-t8 | 5 | 200 | 30.5 µs [29.8 µs, 31.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.062 [0.918, 1.782] | same |
| explicit_orthotropic | corrector | head-t1 | 5 | 200 | 42.1 µs [19.5 µs, 48.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.495 [0.608, 2.773] | same |
| explicit_orthotropic | corrector | head-t8 | 5 | 200 | 28.0 µs [14.3 µs, 42.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.872 [0.441, 2.226] | same |
| explicit_orthotropic | step_hooks | base-t1 | 5 | 400 | 480.7 µs [420.1 µs, 530.2 µs] | 1.2 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | step_hooks | base-t8 | 5 | 400 | 463.9 µs [430.1 µs, 475.4 µs] | 1.2 µs [1.1 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 0.987 [0.875, 1.132] | same |
| explicit_orthotropic | step_hooks | head-t1 | 5 | 400 | 491.1 µs [428.4 µs, 495.5 µs] | 1.2 µs [1.1 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 1.028 [0.808, 1.169] | same |
| explicit_orthotropic | step_hooks | head-t8 | 5 | 400 | 462.3 µs [369.8 µs, 476.7 µs] | 1.2 µs [0.9 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 0.930 [0.697, 1.128] | same |
| explicit_orthotropic | step_rest | base-t1 | 5 | 200 | 68.26 ms [62.30 ms, 69.73 ms] | 341.3 µs [311.5 µs, 348.6 µs] | 3.2 [3.0, 3.3] | – | same |
| explicit_orthotropic | step_rest | base-t8 | 5 | 200 | 69.40 ms [68.31 ms, 71.96 ms] | 347.0 µs [341.5 µs, 359.8 µs] | 3.3 [3.2, 3.4] | 1.017 [0.986, 1.148] | same |
| explicit_orthotropic | step_rest | head-t1 | 5 | 200 | 72.42 ms [64.75 ms, 74.54 ms] | 362.1 µs [323.8 µs, 372.7 µs] | 3.4 [3.1, 3.5] | 1.041 [0.948, 1.190] | same |
| explicit_orthotropic | step_rest | head-t8 | 5 | 200 | 35.58 ms [34.46 ms, 36.07 ms] | 177.9 µs [172.3 µs, 180.4 µs] | 1.7 [1.6, 1.7] | 0.516 [0.495, 0.576] | same |
| explicit_orthotropic | internal_forces | base-t1 | 5 | 40 | 172.41 ms [171.66 ms, 173.67 ms] | 4.31 ms [4.29 ms, 4.34 ms] | 40.9 [40.7, 41.2] | – | same |
| explicit_orthotropic | internal_forces | base-t8 | 5 | 40 | 33.68 ms [33.16 ms, 35.14 ms] | 842.0 µs [828.9 µs, 878.5 µs] | 8.0 [7.9, 8.3] | 0.195 [0.192, 0.202] | same |
| explicit_orthotropic | internal_forces | head-t1 | 5 | 40 | 173.38 ms [172.17 ms, 174.33 ms] | 4.33 ms [4.30 ms, 4.36 ms] | 41.1 [40.8, 41.3] | 1.004 [1.000, 1.006] | same |
| explicit_orthotropic | internal_forces | head-t8 | 5 | 40 | 30.52 ms [29.96 ms, 31.29 ms] | 763.1 µs [749.0 µs, 782.2 µs] | 7.2 [7.1, 7.4] | 0.177 [0.173, 0.182] | same |
| explicit_orthotropic | stresses | base-t1 | 5 | 40 | 92.59 ms [92.03 ms, 93.03 ms] | 2.31 ms [2.30 ms, 2.33 ms] | 21.9 [21.8, 22.1] | – | same |
| explicit_orthotropic | stresses | base-t8 | 5 | 40 | 61.67 ms [61.44 ms, 62.18 ms] | 1.54 ms [1.54 ms, 1.55 ms] | 14.6 [14.6, 14.7] | 0.667 [0.661, 0.672] | same |
| explicit_orthotropic | stresses | head-t1 | 5 | 40 | 92.45 ms [92.21 ms, 92.88 ms] | 2.31 ms [2.31 ms, 2.32 ms] | 21.9 [21.9, 22.0] | 1.002 [0.992, 1.006] | same |
| explicit_orthotropic | stresses | head-t8 | 5 | 40 | 60.57 ms [60.20 ms, 61.10 ms] | 1.51 ms [1.51 ms, 1.53 ms] | 14.4 [14.3, 14.5] | 0.654 [0.647, 0.664] | same |
| explicit_orthotropic | energies | base-t1 | 5 | 40 | 44.50 ms [44.36 ms, 44.89 ms] | 1.11 ms [1.11 ms, 1.12 ms] | 10.5 [10.5, 10.6] | – | same |
| explicit_orthotropic | energies | base-t8 | 5 | 40 | 44.84 ms [44.54 ms, 45.96 ms] | 1.12 ms [1.11 ms, 1.15 ms] | 10.6 [10.6, 10.9] | 1.001 [0.998, 1.035] | same |
| explicit_orthotropic | energies | head-t1 | 5 | 40 | 48.09 ms [48.03 ms, 48.48 ms] | 1.20 ms [1.20 ms, 1.21 ms] | 11.4 [11.4, 11.5] | 1.082 [1.074, 1.087] | same |
| explicit_orthotropic | energies | head-t8 | 5 | 40 | 47.27 ms [46.73 ms, 47.53 ms] | 1.18 ms [1.17 ms, 1.19 ms] | 11.2 [11.1, 11.3] | 1.059 [1.052, 1.069] | same |
| explicit_plastic | mesh | base-t1 | 5 | 1 | 2.18 ms [2.11 ms, 2.22 ms] | 2.18 ms [2.11 ms, 2.22 ms] | 20.6 [20.0, 21.1] | – | same |
| explicit_plastic | mesh | base-t8 | 5 | 1 | 2.12 ms [2.05 ms, 2.17 ms] | 2.12 ms [2.05 ms, 2.17 ms] | 20.1 [19.5, 20.5] | 0.978 [0.924, 1.028] | same |
| explicit_plastic | mesh | head-t1 | 5 | 1 | 2.07 ms [2.04 ms, 2.13 ms] | 2.07 ms [2.04 ms, 2.13 ms] | 19.6 [19.4, 20.2] | 0.946 [0.929, 0.993] | same |
| explicit_plastic | mesh | head-t8 | 5 | 1 | 2.05 ms [2.04 ms, 2.08 ms] | 2.05 ms [2.04 ms, 2.08 ms] | 19.4 [19.3, 19.7] | 0.941 [0.937, 0.973] | same |
| explicit_plastic | init | base-t1 | 5 | 1 | 34.25 ms [33.11 ms, 34.99 ms] | 34.25 ms [33.11 ms, 34.99 ms] | 324.7 [314.0, 331.8] | – | same |
| explicit_plastic | init | base-t8 | 5 | 1 | 33.88 ms [33.18 ms, 35.20 ms] | 33.88 ms [33.18 ms, 35.20 ms] | 321.3 [314.7, 333.8] | 0.989 [0.952, 1.063] | same |
| explicit_plastic | init | head-t1 | 5 | 1 | 33.65 ms [33.05 ms, 35.35 ms] | 33.65 ms [33.05 ms, 35.35 ms] | 319.1 [313.4, 335.2] | 1.013 [0.944, 1.015] | same |
| explicit_plastic | init | head-t8 | 5 | 1 | 34.17 ms [32.60 ms, 35.49 ms] | 34.17 ms [32.60 ms, 35.49 ms] | 324.1 [309.2, 336.5] | 1.009 [0.975, 1.032] | same |
| explicit_plastic | stable_time_step | base-t1 | 5 | 5 | 20.98 ms [20.95 ms, 21.29 ms] | 4.20 ms [4.19 ms, 4.26 ms] | 39.8 [39.7, 40.4] | – | same |
| explicit_plastic | stable_time_step | base-t8 | 5 | 5 | 21.03 ms [20.93 ms, 21.16 ms] | 4.21 ms [4.19 ms, 4.23 ms] | 39.9 [39.7, 40.1] | 0.999 [0.988, 1.009] | same |
| explicit_plastic | stable_time_step | head-t1 | 5 | 5 | 21.07 ms [21.04 ms, 21.12 ms] | 4.21 ms [4.21 ms, 4.22 ms] | 40.0 [39.9, 40.1] | 1.004 [0.989, 1.008] | same |
| explicit_plastic | stable_time_step | head-t8 | 5 | 5 | 21.10 ms [21.03 ms, 21.30 ms] | 4.22 ms [4.21 ms, 4.26 ms] | 40.0 [39.9, 40.4] | 1.007 [0.988, 1.015] | same |
| explicit_plastic | solve_step | base-t1 | 5 | 120 | 987.57 ms [983.15 ms, 989.70 ms] | 8.23 ms [8.19 ms, 8.25 ms] | 78.0 [77.7, 78.2] | – | same |
| explicit_plastic | solve_step | base-t8 | 5 | 120 | 382.36 ms [379.40 ms, 384.00 ms] | 3.19 ms [3.16 ms, 3.20 ms] | 30.2 [30.0, 30.3] | 0.388 [0.383, 0.389] | same |
| explicit_plastic | solve_step | head-t1 | 5 | 120 | 985.80 ms [983.47 ms, 992.20 ms] | 8.22 ms [8.20 ms, 8.27 ms] | 77.9 [77.7, 78.4] | 0.998 [0.996, 1.005] | same |
| explicit_plastic | solve_step | head-t8 | 5 | 120 | 346.08 ms [342.94 ms, 347.80 ms] | 2.88 ms [2.86 ms, 2.90 ms] | 27.3 [27.1, 27.5] | 0.350 [0.347, 0.352] | same |
| explicit_plastic | predictor | base-t1 | 5 | 120 | 6.5 µs [5.5 µs, 12.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | predictor | base-t8 | 5 | 120 | 4.7 µs [3.3 µs, 4.9 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.554 [0.258, 0.859] | same |
| explicit_plastic | predictor | head-t1 | 5 | 120 | 8.9 µs [7.3 µs, 14.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.280 [0.577, 2.355] | same |
| explicit_plastic | predictor | head-t8 | 5 | 120 | 8.9 µs [6.3 µs, 15.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.263 [0.703, 2.017] | same |
| explicit_plastic | residual | base-t1 | 5 | 120 | 865.41 ms [860.03 ms, 867.21 ms] | 7.21 ms [7.17 ms, 7.23 ms] | 68.4 [68.0, 68.5] | – | same |
| explicit_plastic | residual | base-t8 | 5 | 120 | 277.32 ms [274.61 ms, 278.50 ms] | 2.31 ms [2.29 ms, 2.32 ms] | 21.9 [21.7, 22.0] | 0.321 [0.317, 0.322] | same |
| explicit_plastic | residual | head-t1 | 5 | 120 | 861.31 ms [860.74 ms, 866.92 ms] | 7.18 ms [7.17 ms, 7.22 ms] | 68.1 [68.0, 68.5] | 0.997 [0.994, 1.003] | same |
| explicit_plastic | residual | head-t8 | 5 | 120 | 260.23 ms [257.90 ms, 261.36 ms] | 2.17 ms [2.15 ms, 2.18 ms] | 20.6 [20.4, 20.7] | 0.301 [0.298, 0.304] | same |
| explicit_plastic | lumped_matrix | base-t1 | 5 | 120 | 19.9 µs [17.3 µs, 26.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | lumped_matrix | base-t8 | 5 | 120 | 24.1 µs [21.9 µs, 48.4 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.261 [1.160, 1.819] | same |
| explicit_plastic | lumped_matrix | head-t1 | 5 | 120 | 23.4 µs [19.0 µs, 25.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.145 [0.920, 1.352] | same |
| explicit_plastic | lumped_matrix | head-t8 | 5 | 120 | 20.7 µs [19.3 µs, 22.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.046 [0.747, 1.200] | same |
| explicit_plastic | corrector | base-t1 | 5 | 120 | 10.1 µs [9.1 µs, 12.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | corrector | base-t8 | 5 | 120 | 18.0 µs [17.4 µs, 19.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.784 [1.374, 1.931] | same |
| explicit_plastic | corrector | head-t1 | 5 | 120 | 11.0 µs [10.3 µs, 11.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.088 [0.900, 1.127] | same |
| explicit_plastic | corrector | head-t8 | 5 | 120 | 16.3 µs [15.5 µs, 18.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.617 [1.229, 1.920] | same |
| explicit_plastic | step_hooks | base-t1 | 5 | 240 | 83.64 ms [82.53 ms, 83.94 ms] | 348.5 µs [343.9 µs, 349.7 µs] | 3.3 [3.3, 3.3] | – | same |
| explicit_plastic | step_hooks | base-t8 | 5 | 240 | 60.59 ms [59.70 ms, 60.72 ms] | 252.5 µs [248.8 µs, 253.0 µs] | 2.4 [2.4, 2.4] | 0.722 [0.712, 0.736] | same |
| explicit_plastic | step_hooks | head-t1 | 5 | 240 | 83.63 ms [82.01 ms, 84.45 ms] | 348.5 µs [341.7 µs, 351.9 µs] | 3.3 [3.2, 3.3] | 0.998 [0.994, 1.006] | same |
| explicit_plastic | step_hooks | head-t8 | 5 | 240 | 59.47 ms [58.66 ms, 60.55 ms] | 247.8 µs [244.4 µs, 252.3 µs] | 2.3 [2.3, 2.4] | 0.715 [0.699, 0.724] | same |
| explicit_plastic | step_rest | base-t1 | 5 | 120 | 38.69 ms [38.31 ms, 39.92 ms] | 322.4 µs [319.3 µs, 332.7 µs] | 3.1 [3.0, 3.2] | – | same |
| explicit_plastic | step_rest | base-t8 | 5 | 120 | 44.61 ms [44.24 ms, 45.14 ms] | 371.7 µs [368.6 µs, 376.2 µs] | 3.5 [3.5, 3.6] | 1.153 [1.111, 1.167] | same |
| explicit_plastic | step_rest | head-t1 | 5 | 120 | 40.44 ms [40.36 ms, 41.51 ms] | 337.0 µs [336.3 µs, 346.0 µs] | 3.2 [3.2, 3.3] | 1.045 [1.011, 1.078] | same |
| explicit_plastic | step_rest | head-t8 | 5 | 120 | 25.71 ms [25.49 ms, 26.31 ms] | 214.2 µs [212.4 µs, 219.3 µs] | 2.0 [2.0, 2.1] | 0.671 [0.644, 0.684] | same |
| explicit_plastic | internal_forces | base-t1 | 5 | 24 | 170.45 ms [169.52 ms, 173.54 ms] | 7.10 ms [7.06 ms, 7.23 ms] | 67.3 [67.0, 68.6] | – | same |
| explicit_plastic | internal_forces | base-t8 | 5 | 24 | 42.55 ms [41.29 ms, 42.71 ms] | 1.77 ms [1.72 ms, 1.78 ms] | 16.8 [16.3, 16.9] | 0.250 [0.238, 0.252] | same |
| explicit_plastic | internal_forces | head-t1 | 5 | 24 | 170.84 ms [170.34 ms, 171.18 ms] | 7.12 ms [7.10 ms, 7.13 ms] | 67.5 [67.3, 67.6] | 1.004 [0.983, 1.009] | same |
| explicit_plastic | internal_forces | head-t8 | 5 | 24 | 40.73 ms [39.71 ms, 42.08 ms] | 1.70 ms [1.65 ms, 1.75 ms] | 16.1 [15.7, 16.6] | 0.236 [0.233, 0.248] | same |
| explicit_plastic | stresses | base-t1 | 5 | 24 | 121.81 ms [121.59 ms, 123.61 ms] | 5.08 ms [5.07 ms, 5.15 ms] | 48.1 [48.0, 48.8] | – | same |
| explicit_plastic | stresses | base-t8 | 5 | 24 | 103.11 ms [102.21 ms, 103.71 ms] | 4.30 ms [4.26 ms, 4.32 ms] | 40.7 [40.4, 41.0] | 0.847 [0.827, 0.853] | same |
| explicit_plastic | stresses | head-t1 | 5 | 24 | 121.90 ms [121.88 ms, 123.63 ms] | 5.08 ms [5.08 ms, 5.15 ms] | 48.2 [48.2, 48.8] | 1.001 [0.986, 1.016] | same |
| explicit_plastic | stresses | head-t8 | 5 | 24 | 102.97 ms [102.64 ms, 103.46 ms] | 4.29 ms [4.28 ms, 4.31 ms] | 40.7 [40.6, 40.9] | 0.845 [0.831, 0.851] | same |
| explicit_plastic | energies | base-t1 | 5 | 24 | 40.71 ms [40.47 ms, 41.92 ms] | 1.70 ms [1.69 ms, 1.75 ms] | 16.1 [16.0, 16.6] | – | same |
| explicit_plastic | energies | base-t8 | 5 | 24 | 40.90 ms [40.57 ms, 41.21 ms] | 1.70 ms [1.69 ms, 1.72 ms] | 16.2 [16.0, 16.3] | 1.003 [0.977, 1.013] | same |
| explicit_plastic | energies | head-t1 | 5 | 24 | 41.49 ms [41.24 ms, 41.60 ms] | 1.73 ms [1.72 ms, 1.73 ms] | 16.4 [16.3, 16.4] | 1.018 [0.990, 1.023] | same |
| explicit_plastic | energies | head-t8 | 5 | 24 | 41.10 ms [41.06 ms, 42.05 ms] | 1.71 ms [1.71 ms, 1.75 ms] | 16.2 [16.2, 16.6] | 1.016 [0.979, 1.033] | same |
| explicit_sls | mesh | base-t1 | 5 | 1 | 2.17 ms [2.15 ms, 2.23 ms] | 2.17 ms [2.15 ms, 2.23 ms] | 20.6 [20.4, 21.2] | – | same |
| explicit_sls | mesh | base-t8 | 5 | 1 | 2.16 ms [2.06 ms, 2.20 ms] | 2.16 ms [2.06 ms, 2.20 ms] | 20.4 [19.5, 20.8] | 0.998 [0.936, 1.010] | same |
| explicit_sls | mesh | head-t1 | 5 | 1 | 2.09 ms [2.07 ms, 2.11 ms] | 2.09 ms [2.07 ms, 2.11 ms] | 19.9 [19.7, 20.0] | 0.962 [0.941, 0.969] | same |
| explicit_sls | mesh | head-t8 | 5 | 1 | 2.06 ms [2.04 ms, 2.19 ms] | 2.06 ms [2.04 ms, 2.19 ms] | 19.6 [19.4, 20.8] | 0.951 [0.934, 0.981] | same |
| explicit_sls | init | base-t1 | 5 | 1 | 37.66 ms [36.97 ms, 38.25 ms] | 37.66 ms [36.97 ms, 38.25 ms] | 357.1 [350.6, 362.7] | – | same |
| explicit_sls | init | base-t8 | 5 | 1 | 38.34 ms [36.32 ms, 38.62 ms] | 38.34 ms [36.32 ms, 38.62 ms] | 363.6 [344.5, 366.2] | 1.007 [0.967, 1.037] | same |
| explicit_sls | init | head-t1 | 5 | 1 | 36.59 ms [36.08 ms, 37.57 ms] | 36.59 ms [36.08 ms, 37.57 ms] | 347.0 [342.1, 356.3] | 0.972 [0.947, 1.000] | same |
| explicit_sls | init | head-t8 | 5 | 1 | 36.88 ms [36.15 ms, 37.63 ms] | 36.88 ms [36.15 ms, 37.63 ms] | 349.7 [342.8, 356.8] | 0.975 [0.945, 0.999] | same |
| explicit_sls | stable_time_step | base-t1 | 5 | 5 | 20.98 ms [20.88 ms, 21.32 ms] | 4.20 ms [4.18 ms, 4.26 ms] | 39.8 [39.6, 40.4] | – | same |
| explicit_sls | stable_time_step | base-t8 | 5 | 5 | 20.99 ms [20.95 ms, 21.61 ms] | 4.20 ms [4.19 ms, 4.32 ms] | 39.8 [39.7, 41.0] | 1.003 [1.000, 1.014] | same |
| explicit_sls | stable_time_step | head-t1 | 5 | 5 | 21.07 ms [21.03 ms, 21.51 ms] | 4.21 ms [4.21 ms, 4.30 ms] | 40.0 [39.9, 40.8] | 1.006 [0.986, 1.025] | same |
| explicit_sls | stable_time_step | head-t8 | 5 | 5 | 21.14 ms [21.06 ms, 21.19 ms] | 4.23 ms [4.21 ms, 4.24 ms] | 40.1 [39.9, 40.2] | 1.009 [0.992, 1.010] | same |
| explicit_sls | solve_step | base-t1 | 5 | 120 | 774.07 ms [742.31 ms, 783.92 ms] | 6.45 ms [6.19 ms, 6.53 ms] | 61.2 [58.7, 61.9] | – | same |
| explicit_sls | solve_step | base-t8 | 5 | 120 | 310.26 ms [304.19 ms, 313.44 ms] | 2.59 ms [2.53 ms, 2.61 ms] | 24.5 [24.0, 24.8] | 0.399 [0.393, 0.422] | same |
| explicit_sls | solve_step | head-t1 | 5 | 120 | 760.36 ms [731.09 ms, 770.47 ms] | 6.34 ms [6.09 ms, 6.42 ms] | 60.1 [57.8, 60.9] | 0.983 [0.944, 1.024] | same |
| explicit_sls | solve_step | head-t8 | 5 | 120 | 257.71 ms [241.62 ms, 273.66 ms] | 2.15 ms [2.01 ms, 2.28 ms] | 20.4 [19.1, 21.6] | 0.335 [0.312, 0.369] | same |
| explicit_sls | predictor | base-t1 | 5 | 120 | 5.0 µs [3.5 µs, 11.1 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | predictor | base-t8 | 5 | 120 | 4.8 µs [3.8 µs, 14.1 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.003 [0.346, 4.053] | same |
| explicit_sls | predictor | head-t1 | 5 | 120 | 9.1 µs [3.5 µs, 12.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.243 [0.319, 3.355] | same |
| explicit_sls | predictor | head-t8 | 5 | 120 | 3.5 µs [3.3 µs, 9.8 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.947 [0.313, 1.977] | same |
| explicit_sls | residual | base-t1 | 5 | 120 | 731.95 ms [704.14 ms, 740.97 ms] | 6.10 ms [5.87 ms, 6.17 ms] | 57.8 [55.6, 58.6] | – | same |
| explicit_sls | residual | base-t8 | 5 | 120 | 266.41 ms [260.58 ms, 269.34 ms] | 2.22 ms [2.17 ms, 2.24 ms] | 21.1 [20.6, 21.3] | 0.362 [0.356, 0.383] | same |
| explicit_sls | residual | head-t1 | 5 | 120 | 715.38 ms [691.69 ms, 725.11 ms] | 5.96 ms [5.76 ms, 6.04 ms] | 56.5 [54.7, 57.3] | 0.979 [0.945, 1.016] | same |
| explicit_sls | residual | head-t8 | 5 | 120 | 236.33 ms [221.77 ms, 248.99 ms] | 1.97 ms [1.85 ms, 2.07 ms] | 18.7 [17.5, 19.7] | 0.325 [0.303, 0.354] | same |
| explicit_sls | lumped_matrix | base-t1 | 5 | 120 | 23.7 µs [20.7 µs, 26.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | lumped_matrix | base-t8 | 5 | 120 | 22.2 µs [21.5 µs, 23.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.909 [0.847, 1.152] | same |
| explicit_sls | lumped_matrix | head-t1 | 5 | 120 | 22.6 µs [20.8 µs, 26.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.983 [0.862, 1.058] | same |
| explicit_sls | lumped_matrix | head-t8 | 5 | 120 | 19.9 µs [18.2 µs, 21.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.899 [0.696, 0.993] | same |
| explicit_sls | corrector | base-t1 | 5 | 120 | 19.8 µs [9.4 µs, 31.5 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | corrector | base-t8 | 5 | 120 | 19.5 µs [18.2 µs, 26.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.955 [0.577, 2.786] | same |
| explicit_sls | corrector | head-t1 | 5 | 120 | 18.9 µs [11.1 µs, 20.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.636 [0.561, 2.025] | same |
| explicit_sls | corrector | head-t8 | 5 | 120 | 17.0 µs [5.6 µs, 32.6 µs] | 0.1 µs [0.0 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.627 [0.289, 1.821] | same |
| explicit_sls | step_hooks | base-t1 | 5 | 240 | 466.5 µs [426.0 µs, 484.4 µs] | 1.9 µs [1.8 µs, 2.0 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | step_hooks | base-t8 | 5 | 240 | 451.7 µs [431.7 µs, 480.2 µs] | 1.9 µs [1.8 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 0.960 [0.908, 1.112] | same |
| explicit_sls | step_hooks | head-t1 | 5 | 240 | 441.1 µs [414.3 µs, 474.7 µs] | 1.8 µs [1.7 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 0.980 [0.888, 1.023] | same |
| explicit_sls | step_hooks | head-t8 | 5 | 240 | 449.7 µs [394.7 µs, 457.7 µs] | 1.9 µs [1.6 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 0.945 [0.868, 1.054] | same |
| explicit_sls | step_rest | base-t1 | 5 | 120 | 41.64 ms [37.49 ms, 42.14 ms] | 347.0 µs [312.4 µs, 351.2 µs] | 3.3 [3.0, 3.3] | – | same |
| explicit_sls | step_rest | base-t8 | 5 | 120 | 43.09 ms [42.76 ms, 43.74 ms] | 359.1 µs [356.3 µs, 364.5 µs] | 3.4 [3.4, 3.5] | 1.033 [1.025, 1.154] | same |
| explicit_sls | step_rest | head-t1 | 5 | 120 | 44.21 ms [38.68 ms, 44.58 ms] | 368.4 µs [322.3 µs, 371.5 µs] | 3.5 [3.1, 3.5] | 1.058 [0.934, 1.179] | same |
| explicit_sls | step_rest | head-t8 | 5 | 120 | 20.61 ms [19.02 ms, 23.87 ms] | 171.7 µs [158.5 µs, 198.9 µs] | 1.6 [1.5, 1.9] | 0.495 [0.452, 0.637] | same |
| explicit_sls | internal_forces | base-t1 | 5 | 24 | 141.31 ms [137.01 ms, 145.66 ms] | 5.89 ms [5.71 ms, 6.07 ms] | 55.8 [54.1, 57.6] | – | same |
| explicit_sls | internal_forces | base-t8 | 5 | 24 | 40.91 ms [39.69 ms, 41.54 ms] | 1.70 ms [1.65 ms, 1.73 ms] | 16.2 [15.7, 16.4] | 0.290 [0.278, 0.296] | same |
| explicit_sls | internal_forces | head-t1 | 5 | 24 | 135.44 ms [134.22 ms, 136.52 ms] | 5.64 ms [5.59 ms, 5.69 ms] | 53.5 [53.0, 53.9] | 0.958 [0.937, 0.995] | same |
| explicit_sls | internal_forces | head-t8 | 5 | 24 | 36.46 ms [35.65 ms, 38.67 ms] | 1.52 ms [1.49 ms, 1.61 ms] | 14.4 [14.1, 15.3] | 0.263 [0.251, 0.282] | same |
| explicit_sls | stresses | base-t1 | 5 | 24 | 89.66 ms [88.74 ms, 92.35 ms] | 3.74 ms [3.70 ms, 3.85 ms] | 35.4 [35.1, 36.5] | – | same |
| explicit_sls | stresses | base-t8 | 5 | 24 | 71.96 ms [69.05 ms, 73.45 ms] | 3.00 ms [2.88 ms, 3.06 ms] | 28.4 [27.3, 29.0] | 0.784 [0.758, 0.828] | same |
| explicit_sls | stresses | head-t1 | 5 | 24 | 86.82 ms [86.44 ms, 87.67 ms] | 3.62 ms [3.60 ms, 3.65 ms] | 34.3 [34.2, 34.6] | 0.972 [0.940, 0.975] | same |
| explicit_sls | stresses | head-t8 | 5 | 24 | 67.95 ms [67.76 ms, 68.14 ms] | 2.83 ms [2.82 ms, 2.84 ms] | 26.8 [26.8, 26.9] | 0.758 [0.734, 0.767] | same |
| explicit_sls | energies | base-t1 | 5 | 24 | 66.36 ms [66.15 ms, 66.46 ms] | 2.76 ms [2.76 ms, 2.77 ms] | 26.2 [26.1, 26.3] | – | same |
| explicit_sls | energies | base-t8 | 5 | 24 | 66.39 ms [66.36 ms, 66.49 ms] | 2.77 ms [2.76 ms, 2.77 ms] | 26.2 [26.2, 26.3] | 1.001 [0.999, 1.005] | same |
| explicit_sls | energies | head-t1 | 5 | 24 | 67.39 ms [67.07 ms, 67.70 ms] | 2.81 ms [2.79 ms, 2.82 ms] | 26.6 [26.5, 26.7] | 1.016 [1.013, 1.019] | same |
| explicit_sls | energies | head-t8 | 5 | 24 | 66.98 ms [66.71 ms, 67.17 ms] | 2.79 ms [2.78 ms, 2.80 ms] | 26.5 [26.4, 26.5] | 1.009 [1.007, 1.015] | same |
| explicit_tet10_fd | mesh | base-t1 | 5 | 1 | 6.67 ms [6.60 ms, 7.06 ms] | 6.67 ms [6.60 ms, 7.06 ms] | 63.3 [62.6, 66.9] | – | same |
| explicit_tet10_fd | mesh | base-t8 | 5 | 1 | 6.89 ms [6.63 ms, 7.02 ms] | 6.89 ms [6.63 ms, 7.02 ms] | 65.3 [62.9, 66.5] | 1.007 [0.986, 1.063] | same |
| explicit_tet10_fd | mesh | head-t1 | 5 | 1 | 6.73 ms [6.64 ms, 6.81 ms] | 6.73 ms [6.64 ms, 6.81 ms] | 63.8 [63.0, 64.5] | 1.007 [0.944, 1.023] | same |
| explicit_tet10_fd | mesh | head-t8 | 5 | 1 | 6.58 ms [6.48 ms, 6.99 ms] | 6.58 ms [6.48 ms, 6.99 ms] | 62.4 [61.4, 66.3] | 0.990 [0.932, 1.023] | same |
| explicit_tet10_fd | init | base-t1 | 5 | 1 | 188.07 ms [186.35 ms, 189.94 ms] | 188.07 ms [186.35 ms, 189.94 ms] | 1783.4 [1767.1, 1801.1] | – | same |
| explicit_tet10_fd | init | base-t8 | 5 | 1 | 185.95 ms [185.59 ms, 190.08 ms] | 185.95 ms [185.59 ms, 190.08 ms] | 1763.3 [1759.9, 1802.4] | 0.994 [0.986, 1.013] | same |
| explicit_tet10_fd | init | head-t1 | 5 | 1 | 188.36 ms [187.69 ms, 191.07 ms] | 188.36 ms [187.69 ms, 191.07 ms] | 1786.1 [1779.8, 1811.8] | 1.004 [0.992, 1.016] | same |
| explicit_tet10_fd | init | head-t8 | 5 | 1 | 184.71 ms [183.21 ms, 188.02 ms] | 184.71 ms [183.21 ms, 188.02 ms] | 1751.5 [1737.3, 1782.9] | 0.983 [0.979, 0.999] | same |
| explicit_tet10_fd | stable_time_step | base-t1 | 5 | 5 | 62.61 ms [62.27 ms, 62.77 ms] | 12.52 ms [12.45 ms, 12.55 ms] | 118.7 [118.1, 119.0] | – | same |
| explicit_tet10_fd | stable_time_step | base-t8 | 5 | 5 | 63.64 ms [63.26 ms, 64.53 ms] | 12.73 ms [12.65 ms, 12.91 ms] | 120.7 [120.0, 122.4] | 1.014 [1.010, 1.036] | same |
| explicit_tet10_fd | stable_time_step | head-t1 | 5 | 5 | 61.72 ms [61.67 ms, 61.91 ms] | 12.34 ms [12.33 ms, 12.38 ms] | 117.1 [116.9, 117.4] | 0.986 [0.983, 0.991] | same |
| explicit_tet10_fd | stable_time_step | head-t8 | 5 | 5 | 62.51 ms [62.22 ms, 63.01 ms] | 12.50 ms [12.44 ms, 12.60 ms] | 118.6 [118.0, 119.5] | 1.000 [0.995, 1.004] | same |
| explicit_tet10_fd | solve_step | base-t1 | 5 | 40 | 1.126 s [1.113 s, 1.132 s] | 28.15 ms [27.82 ms, 28.31 ms] | 266.9 [263.8, 268.4] | – | same |
| explicit_tet10_fd | solve_step | base-t8 | 5 | 40 | 376.03 ms [370.09 ms, 381.54 ms] | 9.40 ms [9.25 ms, 9.54 ms] | 89.1 [87.7, 90.5] | 0.333 [0.330, 0.338] | same |
| explicit_tet10_fd | solve_step | head-t1 | 5 | 40 | 1.137 s [1.133 s, 1.144 s] | 28.43 ms [28.32 ms, 28.60 ms] | 269.6 [268.6, 271.2] | 1.013 [1.004, 1.018] | same |
| explicit_tet10_fd | solve_step | head-t8 | 5 | 40 | 313.64 ms [311.01 ms, 316.91 ms] | 7.84 ms [7.78 ms, 7.92 ms] | 74.4 [73.7, 75.1] | 0.280 [0.276, 0.282] | same |
| explicit_tet10_fd | predictor | base-t1 | 5 | 40 | 6.2 µs [5.5 µs, 6.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | predictor | base-t8 | 5 | 40 | 6.4 µs [5.5 µs, 10.2 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.140 [0.856, 1.543] | same |
| explicit_tet10_fd | predictor | head-t1 | 5 | 40 | 6.7 µs [5.6 µs, 7.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.027 [0.960, 1.283] | same |
| explicit_tet10_fd | predictor | head-t8 | 5 | 40 | 6.3 µs [5.7 µs, 8.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.021 [0.883, 1.544] | same |
| explicit_tet10_fd | residual | base-t1 | 5 | 40 | 1.025 s [1.015 s, 1.031 s] | 25.63 ms [25.38 ms, 25.79 ms] | 243.1 [240.7, 244.5] | – | same |
| explicit_tet10_fd | residual | base-t8 | 5 | 40 | 274.50 ms [270.16 ms, 278.50 ms] | 6.86 ms [6.75 ms, 6.96 ms] | 65.1 [64.0, 66.0] | 0.266 [0.264, 0.272] | same |
| explicit_tet10_fd | residual | head-t1 | 5 | 40 | 1.029 s [1.023 s, 1.034 s] | 25.72 ms [25.59 ms, 25.86 ms] | 243.8 [242.6, 245.2] | 1.008 [0.997, 1.009] | same |
| explicit_tet10_fd | residual | head-t8 | 5 | 40 | 255.47 ms [250.86 ms, 258.50 ms] | 6.39 ms [6.27 ms, 6.46 ms] | 60.6 [59.5, 61.3] | 0.248 [0.246, 0.252] | same |
| explicit_tet10_fd | lumped_matrix | base-t1 | 5 | 40 | 11.2 µs [11.1 µs, 12.8 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | lumped_matrix | base-t8 | 5 | 40 | 17.4 µs [16.3 µs, 28.7 µs] | 0.4 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 1.561 [1.334, 2.332] | same |
| explicit_tet10_fd | lumped_matrix | head-t1 | 5 | 40 | 12.4 µs [10.5 µs, 14.5 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.098 [0.818, 1.301] | same |
| explicit_tet10_fd | lumped_matrix | head-t8 | 5 | 40 | 12.7 µs [9.7 µs, 15.2 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.139 [0.865, 1.236] | same |
| explicit_tet10_fd | corrector | base-t1 | 5 | 40 | 6.1 µs [5.8 µs, 7.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | corrector | base-t8 | 5 | 40 | 6.5 µs [6.5 µs, 6.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.079 [0.905, 1.127] | same |
| explicit_tet10_fd | corrector | head-t1 | 5 | 40 | 9.0 µs [8.3 µs, 11.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.428 [1.094, 1.977] | same |
| explicit_tet10_fd | corrector | head-t8 | 5 | 40 | 11.1 µs [9.5 µs, 26.8 µs] | 0.3 µs [0.2 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 1.728 [1.252, 4.368] | same |
| explicit_tet10_fd | step_hooks | base-t1 | 5 | 80 | 157.8 µs [147.7 µs, 168.5 µs] | 2.0 µs [1.8 µs, 2.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | step_hooks | base-t8 | 5 | 80 | 170.0 µs [156.5 µs, 174.5 µs] | 2.1 µs [2.0 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 1.054 [1.009, 1.094] | same |
| explicit_tet10_fd | step_hooks | head-t1 | 5 | 80 | 153.9 µs [151.3 µs, 165.6 µs] | 1.9 µs [1.9 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 1.030 [0.898, 1.053] | same |
| explicit_tet10_fd | step_hooks | head-t8 | 5 | 80 | 162.2 µs [157.4 µs, 173.7 µs] | 2.0 µs [2.0 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 1.030 [0.934, 1.103] | same |
| explicit_tet10_fd | step_rest | base-t1 | 5 | 40 | 100.65 ms [97.56 ms, 103.84 ms] | 2.52 ms [2.44 ms, 2.60 ms] | 23.9 [23.1, 24.6] | – | same |
| explicit_tet10_fd | step_rest | base-t8 | 5 | 40 | 101.32 ms [99.73 ms, 102.84 ms] | 2.53 ms [2.49 ms, 2.57 ms] | 24.0 [23.6, 24.4] | 1.007 [0.990, 1.022] | same |
| explicit_tet10_fd | step_rest | head-t1 | 5 | 40 | 108.31 ms [107.74 ms, 110.67 ms] | 2.71 ms [2.69 ms, 2.77 ms] | 25.7 [25.5, 26.2] | 1.076 [1.066, 1.121] | same |
| explicit_tet10_fd | step_rest | head-t8 | 5 | 40 | 58.66 ms [57.97 ms, 60.28 ms] | 1.47 ms [1.45 ms, 1.51 ms] | 13.9 [13.7, 14.3] | 0.582 [0.576, 0.618] | same |
| explicit_tet10_fd | internal_forces | base-t1 | 5 | 8 | 196.46 ms [195.84 ms, 197.33 ms] | 24.56 ms [24.48 ms, 24.67 ms] | 232.9 [232.1, 233.9] | – | same |
| explicit_tet10_fd | internal_forces | base-t8 | 5 | 8 | 43.85 ms [42.98 ms, 44.13 ms] | 5.48 ms [5.37 ms, 5.52 ms] | 52.0 [50.9, 52.3] | 0.223 [0.219, 0.224] | same |
| explicit_tet10_fd | internal_forces | head-t1 | 5 | 8 | 197.50 ms [195.71 ms, 198.17 ms] | 24.69 ms [24.46 ms, 24.77 ms] | 234.1 [232.0, 234.9] | 1.002 [0.999, 1.009] | same |
| explicit_tet10_fd | internal_forces | head-t8 | 5 | 8 | 42.62 ms [42.19 ms, 44.00 ms] | 5.33 ms [5.27 ms, 5.50 ms] | 50.5 [50.0, 52.2] | 0.216 [0.215, 0.223] | same |
| explicit_tet10_fd | stresses | base-t1 | 5 | 8 | 88.22 ms [85.78 ms, 88.44 ms] | 11.03 ms [10.72 ms, 11.05 ms] | 104.6 [101.7, 104.8] | – | same |
| explicit_tet10_fd | stresses | base-t8 | 5 | 8 | 44.35 ms [43.92 ms, 44.47 ms] | 5.54 ms [5.49 ms, 5.56 ms] | 52.6 [52.1, 52.7] | 0.502 [0.500, 0.512] | same |
| explicit_tet10_fd | stresses | head-t1 | 5 | 8 | 89.08 ms [86.84 ms, 90.29 ms] | 11.13 ms [10.86 ms, 11.29 ms] | 105.6 [102.9, 107.0] | 1.012 [1.001, 1.031] | same |
| explicit_tet10_fd | stresses | head-t8 | 5 | 8 | 44.48 ms [43.92 ms, 44.91 ms] | 5.56 ms [5.49 ms, 5.61 ms] | 52.7 [52.1, 53.2] | 0.505 [0.504, 0.512] | same |
| explicit_tet10_fd | energies | base-t1 | 5 | 8 | 36.30 ms [36.09 ms, 36.51 ms] | 4.54 ms [4.51 ms, 4.56 ms] | 43.0 [42.8, 43.3] | – | same |
| explicit_tet10_fd | energies | base-t8 | 5 | 8 | 36.19 ms [36.03 ms, 36.65 ms] | 4.52 ms [4.50 ms, 4.58 ms] | 42.9 [42.7, 43.4] | 0.995 [0.994, 1.004] | same |
| explicit_tet10_fd | energies | head-t1 | 5 | 8 | 36.39 ms [35.93 ms, 36.48 ms] | 4.55 ms [4.49 ms, 4.56 ms] | 43.1 [42.6, 43.2] | 1.003 [0.996, 1.004] | same |
| explicit_tet10_fd | energies | head-t8 | 5 | 8 | 36.06 ms [35.84 ms, 36.15 ms] | 4.51 ms [4.48 ms, 4.52 ms] | 42.7 [42.5, 42.8] | 0.993 [0.982, 0.997] | same |
| explicit_tet10_ss | mesh | base-t1 | 5 | 1 | 6.88 ms [6.55 ms, 7.07 ms] | 6.88 ms [6.55 ms, 7.07 ms] | 65.2 [62.1, 67.1] | – | same |
| explicit_tet10_ss | mesh | base-t8 | 5 | 1 | 6.89 ms [6.73 ms, 7.02 ms] | 6.89 ms [6.73 ms, 7.02 ms] | 65.4 [63.8, 66.5] | 1.002 [0.964, 1.053] | same |
| explicit_tet10_ss | mesh | head-t1 | 5 | 1 | 6.72 ms [6.57 ms, 6.79 ms] | 6.72 ms [6.57 ms, 6.79 ms] | 63.7 [62.3, 64.3] | 0.985 [0.950, 1.004] | same |
| explicit_tet10_ss | mesh | head-t8 | 5 | 1 | 6.80 ms [6.56 ms, 6.82 ms] | 6.80 ms [6.56 ms, 6.82 ms] | 64.5 [62.2, 64.7] | 0.989 [0.928, 1.041] | same |
| explicit_tet10_ss | init | base-t1 | 5 | 1 | 173.43 ms [171.75 ms, 180.49 ms] | 173.43 ms [171.75 ms, 180.49 ms] | 1644.6 [1628.6, 1711.5] | – | same |
| explicit_tet10_ss | init | base-t8 | 5 | 1 | 174.86 ms [171.82 ms, 176.09 ms] | 174.86 ms [171.82 ms, 176.09 ms] | 1658.2 [1629.3, 1669.8] | 1.002 [0.969, 1.009] | same |
| explicit_tet10_ss | init | head-t1 | 5 | 1 | 170.69 ms [169.58 ms, 171.75 ms] | 170.69 ms [169.58 ms, 171.75 ms] | 1618.6 [1608.0, 1628.6] | 0.985 [0.940, 0.994] | same |
| explicit_tet10_ss | init | head-t8 | 5 | 1 | 171.70 ms [168.80 ms, 173.71 ms] | 171.70 ms [168.80 ms, 173.71 ms] | 1628.2 [1600.6, 1647.2] | 0.980 [0.951, 1.011] | same |
| explicit_tet10_ss | stable_time_step | base-t1 | 5 | 5 | 62.96 ms [62.63 ms, 62.98 ms] | 12.59 ms [12.53 ms, 12.60 ms] | 119.4 [118.8, 119.4] | – | same |
| explicit_tet10_ss | stable_time_step | base-t8 | 5 | 5 | 63.21 ms [63.08 ms, 63.74 ms] | 12.64 ms [12.62 ms, 12.75 ms] | 119.9 [119.6, 120.9] | 1.004 [1.002, 1.015] | same |
| explicit_tet10_ss | stable_time_step | head-t1 | 5 | 5 | 61.68 ms [61.53 ms, 62.43 ms] | 12.34 ms [12.31 ms, 12.49 ms] | 117.0 [116.7, 118.4] | 0.984 [0.977, 0.992] | same |
| explicit_tet10_ss | stable_time_step | head-t8 | 5 | 5 | 62.20 ms [62.01 ms, 62.87 ms] | 12.44 ms [12.40 ms, 12.57 ms] | 118.0 [117.6, 119.2] | 0.991 [0.985, 0.999] | same |
| explicit_tet10_ss | solve_step | base-t1 | 5 | 50 | 992.55 ms [987.86 ms, 997.72 ms] | 19.85 ms [19.76 ms, 19.95 ms] | 188.2 [187.4, 189.2] | – | same |
| explicit_tet10_ss | solve_step | base-t8 | 5 | 50 | 465.21 ms [463.68 ms, 473.45 ms] | 9.30 ms [9.27 ms, 9.47 ms] | 88.2 [87.9, 89.8] | 0.469 [0.466, 0.478] | same |
| explicit_tet10_ss | solve_step | head-t1 | 5 | 50 | 1.012 s [1.005 s, 1.018 s] | 20.25 ms [20.09 ms, 20.35 ms] | 192.0 [190.5, 193.0] | 1.020 [1.007, 1.027] | same |
| explicit_tet10_ss | solve_step | head-t8 | 5 | 50 | 386.63 ms [378.13 ms, 389.71 ms] | 7.73 ms [7.56 ms, 7.79 ms] | 73.3 [71.7, 73.9] | 0.390 [0.383, 0.393] | same |
| explicit_tet10_ss | predictor | base-t1 | 5 | 50 | 7.5 µs [7.4 µs, 8.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | predictor | base-t8 | 5 | 50 | 8.2 µs [7.3 µs, 10.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.038 [0.893, 1.405] | same |
| explicit_tet10_ss | predictor | head-t1 | 5 | 50 | 10.0 µs [7.2 µs, 10.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.292 [0.968, 1.364] | same |
| explicit_tet10_ss | predictor | head-t8 | 5 | 50 | 6.1 µs [4.9 µs, 8.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.752 [0.657, 1.093] | same |
| explicit_tet10_ss | residual | base-t1 | 5 | 50 | 867.67 ms [864.22 ms, 872.88 ms] | 17.35 ms [17.28 ms, 17.46 ms] | 164.6 [163.9, 165.5] | – | same |
| explicit_tet10_ss | residual | base-t8 | 5 | 50 | 338.22 ms [336.95 ms, 343.17 ms] | 6.76 ms [6.74 ms, 6.86 ms] | 64.1 [63.9, 65.1] | 0.390 [0.387, 0.396] | same |
| explicit_tet10_ss | residual | head-t1 | 5 | 50 | 875.67 ms [869.13 ms, 879.92 ms] | 17.51 ms [17.38 ms, 17.60 ms] | 166.1 [164.8, 166.9] | 1.009 [0.996, 1.018] | same |
| explicit_tet10_ss | residual | head-t8 | 5 | 50 | 311.31 ms [304.76 ms, 313.98 ms] | 6.23 ms [6.10 ms, 6.28 ms] | 59.0 [57.8, 59.5] | 0.359 [0.353, 0.362] | same |
| explicit_tet10_ss | lumped_matrix | base-t1 | 5 | 50 | 14.7 µs [13.6 µs, 18.8 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | lumped_matrix | base-t8 | 5 | 50 | 19.6 µs [19.2 µs, 30.2 µs] | 0.4 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.303 [1.043, 2.208] | same |
| explicit_tet10_ss | lumped_matrix | head-t1 | 5 | 50 | 16.7 µs [13.6 µs, 18.4 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.135 [0.721, 1.347] | same |
| explicit_tet10_ss | lumped_matrix | head-t8 | 5 | 50 | 16.2 µs [15.4 µs, 19.8 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.097 [0.838, 1.452] | same |
| explicit_tet10_ss | corrector | base-t1 | 5 | 50 | 7.9 µs [7.5 µs, 8.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | corrector | base-t8 | 5 | 50 | 8.5 µs [7.7 µs, 13.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.011 [0.964, 1.737] | same |
| explicit_tet10_ss | corrector | head-t1 | 5 | 50 | 12.4 µs [11.6 µs, 13.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.527 [1.475, 1.657] | same |
| explicit_tet10_ss | corrector | head-t8 | 5 | 50 | 13.0 µs [12.0 µs, 13.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.604 [1.528, 1.764] | same |
| explicit_tet10_ss | step_hooks | base-t1 | 5 | 100 | 163.4 µs [157.5 µs, 175.9 µs] | 1.6 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | step_hooks | base-t8 | 5 | 100 | 168.0 µs [161.2 µs, 187.9 µs] | 1.7 µs [1.6 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.023 [0.917, 1.193] | same |
| explicit_tet10_ss | step_hooks | head-t1 | 5 | 100 | 161.6 µs [154.9 µs, 170.8 µs] | 1.6 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.989 [0.931, 1.040] | same |
| explicit_tet10_ss | step_hooks | head-t8 | 5 | 100 | 174.9 µs [156.9 µs, 178.7 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.088 [0.964, 1.110] | same |
| explicit_tet10_ss | step_rest | base-t1 | 5 | 50 | 124.62 ms [123.44 ms, 125.81 ms] | 2.49 ms [2.47 ms, 2.52 ms] | 23.6 [23.4, 23.9] | – | same |
| explicit_tet10_ss | step_rest | base-t8 | 5 | 50 | 126.83 ms [126.51 ms, 130.05 ms] | 2.54 ms [2.53 ms, 2.60 ms] | 24.1 [24.0, 24.7] | 1.022 [1.006, 1.049] | same |
| explicit_tet10_ss | step_rest | head-t1 | 5 | 50 | 136.58 ms [134.87 ms, 137.59 ms] | 2.73 ms [2.70 ms, 2.75 ms] | 25.9 [25.6, 26.1] | 1.094 [1.085, 1.106] | same |
| explicit_tet10_ss | step_rest | head-t8 | 5 | 50 | 75.09 ms [73.17 ms, 75.82 ms] | 1.50 ms [1.46 ms, 1.52 ms] | 14.2 [13.9, 14.4] | 0.602 [0.593, 0.609] | same |
| explicit_tet10_ss | internal_forces | base-t1 | 5 | 10 | 163.04 ms [162.26 ms, 163.96 ms] | 16.30 ms [16.23 ms, 16.40 ms] | 154.6 [153.9, 155.5] | – | same |
| explicit_tet10_ss | internal_forces | base-t8 | 5 | 10 | 53.84 ms [53.26 ms, 54.01 ms] | 5.38 ms [5.33 ms, 5.40 ms] | 51.1 [50.5, 51.2] | 0.329 [0.327, 0.332] | same |
| explicit_tet10_ss | internal_forces | head-t1 | 5 | 10 | 163.66 ms [163.11 ms, 165.01 ms] | 16.37 ms [16.31 ms, 16.50 ms] | 155.2 [154.7, 156.5] | 1.005 [0.998, 1.006] | same |
| explicit_tet10_ss | internal_forces | head-t8 | 5 | 10 | 51.24 ms [50.67 ms, 52.66 ms] | 5.12 ms [5.07 ms, 5.27 ms] | 48.6 [48.0, 49.9] | 0.314 [0.311, 0.322] | same |
| explicit_tet10_ss | stresses | base-t1 | 5 | 10 | 114.80 ms [105.17 ms, 115.88 ms] | 11.48 ms [10.52 ms, 11.59 ms] | 108.9 [99.7, 109.9] | – | same |
| explicit_tet10_ss | stresses | base-t8 | 5 | 10 | 48.62 ms [47.97 ms, 49.20 ms] | 4.86 ms [4.80 ms, 4.92 ms] | 46.1 [45.5, 46.7] | 0.421 [0.415, 0.462] | same |
| explicit_tet10_ss | stresses | head-t1 | 5 | 10 | 115.59 ms [114.28 ms, 116.36 ms] | 11.56 ms [11.43 ms, 11.64 ms] | 109.6 [108.4, 110.3] | 1.007 [0.995, 1.099] | same |
| explicit_tet10_ss | stresses | head-t8 | 5 | 10 | 49.06 ms [47.21 ms, 49.60 ms] | 4.91 ms [4.72 ms, 4.96 ms] | 46.5 [44.8, 47.0] | 0.428 [0.412, 0.464] | same |
| explicit_tet10_ss | energies | base-t1 | 5 | 10 | 34.97 ms [34.37 ms, 35.87 ms] | 3.50 ms [3.44 ms, 3.59 ms] | 33.2 [32.6, 34.0] | – | same |
| explicit_tet10_ss | energies | base-t8 | 5 | 10 | 33.39 ms [33.24 ms, 33.88 ms] | 3.34 ms [3.32 ms, 3.39 ms] | 31.7 [31.5, 32.1] | 0.956 [0.929, 0.986] | same |
| explicit_tet10_ss | energies | head-t1 | 5 | 10 | 35.40 ms [34.79 ms, 36.00 ms] | 3.54 ms [3.48 ms, 3.60 ms] | 33.6 [33.0, 34.1] | 1.007 [0.987, 1.043] | same |
| explicit_tet10_ss | energies | head-t8 | 5 | 10 | 33.42 ms [32.25 ms, 39.54 ms] | 3.34 ms [3.23 ms, 3.95 ms] | 31.7 [30.6, 37.5] | 0.949 [0.925, 1.106] | same |
| explicit_tet4_fd | mesh | base-t1 | 5 | 1 | 3.25 ms [3.13 ms, 3.32 ms] | 3.25 ms [3.13 ms, 3.32 ms] | 20.1 [19.3, 20.5] | – | same |
| explicit_tet4_fd | mesh | base-t8 | 5 | 1 | 3.22 ms [3.12 ms, 3.24 ms] | 3.22 ms [3.12 ms, 3.24 ms] | 19.9 [19.3, 20.0] | 0.990 [0.941, 1.030] | same |
| explicit_tet4_fd | mesh | head-t1 | 5 | 1 | 3.19 ms [3.17 ms, 3.26 ms] | 3.19 ms [3.17 ms, 3.26 ms] | 19.7 [19.6, 20.1] | 0.983 [0.956, 1.035] | same |
| explicit_tet4_fd | mesh | head-t8 | 5 | 1 | 3.23 ms [3.06 ms, 3.25 ms] | 3.23 ms [3.06 ms, 3.25 ms] | 19.9 [18.9, 20.0] | 0.973 [0.962, 1.038] | same |
| explicit_tet4_fd | init | base-t1 | 5 | 1 | 55.56 ms [53.95 ms, 56.48 ms] | 55.56 ms [53.95 ms, 56.48 ms] | 343.0 [333.0, 348.6] | – | same |
| explicit_tet4_fd | init | base-t8 | 5 | 1 | 54.63 ms [53.16 ms, 56.08 ms] | 54.63 ms [53.16 ms, 56.08 ms] | 337.2 [328.2, 346.2] | 0.986 [0.951, 1.028] | same |
| explicit_tet4_fd | init | head-t1 | 5 | 1 | 53.38 ms [52.47 ms, 54.07 ms] | 53.38 ms [52.47 ms, 54.07 ms] | 329.5 [323.9, 333.8] | 0.968 [0.944, 0.986] | same |
| explicit_tet4_fd | init | head-t8 | 5 | 1 | 55.19 ms [52.86 ms, 56.91 ms] | 55.19 ms [52.86 ms, 56.91 ms] | 340.7 [326.3, 351.3] | 1.005 [0.969, 1.023] | same |
| explicit_tet4_fd | stable_time_step | base-t1 | 5 | 5 | 32.16 ms [32.12 ms, 32.19 ms] | 6.43 ms [6.42 ms, 6.44 ms] | 39.7 [39.6, 39.7] | – | same |
| explicit_tet4_fd | stable_time_step | base-t8 | 5 | 5 | 32.21 ms [32.10 ms, 32.26 ms] | 6.44 ms [6.42 ms, 6.45 ms] | 39.8 [39.6, 39.8] | 1.002 [0.998, 1.003] | same |
| explicit_tet4_fd | stable_time_step | head-t1 | 5 | 5 | 32.35 ms [32.31 ms, 32.48 ms] | 6.47 ms [6.46 ms, 6.50 ms] | 39.9 [39.9, 40.1] | 1.007 [1.005, 1.010] | same |
| explicit_tet4_fd | stable_time_step | head-t8 | 5 | 5 | 32.57 ms [32.51 ms, 33.78 ms] | 6.51 ms [6.50 ms, 6.76 ms] | 40.2 [40.1, 41.7] | 1.013 [1.012, 1.050] | same |
| explicit_tet4_fd | solve_step | base-t1 | 5 | 150 | 1.291 s [1.287 s, 1.292 s] | 8.60 ms [8.58 ms, 8.61 ms] | 53.1 [53.0, 53.2] | – | same |
| explicit_tet4_fd | solve_step | base-t8 | 5 | 150 | 344.77 ms [344.14 ms, 346.77 ms] | 2.30 ms [2.29 ms, 2.31 ms] | 14.2 [14.2, 14.3] | 0.267 [0.267, 0.269] | same |
| explicit_tet4_fd | solve_step | head-t1 | 5 | 150 | 1.299 s [1.295 s, 1.304 s] | 8.66 ms [8.63 ms, 8.70 ms] | 53.5 [53.3, 53.7] | 1.007 [1.005, 1.011] | same |
| explicit_tet4_fd | solve_step | head-t8 | 5 | 150 | 264.87 ms [264.42 ms, 266.22 ms] | 1.77 ms [1.76 ms, 1.77 ms] | 10.9 [10.9, 11.0] | 0.206 [0.205, 0.206] | same |
| explicit_tet4_fd | predictor | base-t1 | 5 | 150 | 7.2 µs [4.8 µs, 17.4 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | predictor | base-t8 | 5 | 150 | 8.7 µs [4.4 µs, 24.2 µs] | 0.1 µs [0.0 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.914 [0.591, 3.347] | same |
| explicit_tet4_fd | predictor | head-t1 | 5 | 150 | 7.7 µs [4.4 µs, 16.5 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.604 [0.383, 3.429] | same |
| explicit_tet4_fd | predictor | head-t8 | 5 | 150 | 5.3 µs [4.2 µs, 7.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.721 [0.289, 1.468] | same |
| explicit_tet4_fd | residual | base-t1 | 5 | 150 | 1.225 s [1.222 s, 1.226 s] | 8.17 ms [8.14 ms, 8.17 ms] | 50.4 [50.3, 50.5] | – | same |
| explicit_tet4_fd | residual | base-t8 | 5 | 150 | 278.67 ms [277.93 ms, 280.68 ms] | 1.86 ms [1.85 ms, 1.87 ms] | 11.5 [11.4, 11.6] | 0.227 [0.227, 0.230] | same |
| explicit_tet4_fd | residual | head-t1 | 5 | 150 | 1.227 s [1.222 s, 1.231 s] | 8.18 ms [8.15 ms, 8.21 ms] | 50.5 [50.3, 50.7] | 1.001 [0.999, 1.005] | same |
| explicit_tet4_fd | residual | head-t8 | 5 | 150 | 235.36 ms [234.82 ms, 235.81 ms] | 1.57 ms [1.57 ms, 1.57 ms] | 9.7 [9.7, 9.7] | 0.192 [0.192, 0.193] | same |
| explicit_tet4_fd | lumped_matrix | base-t1 | 5 | 150 | 20.9 µs [19.2 µs, 26.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | lumped_matrix | base-t8 | 5 | 150 | 27.7 µs [19.8 µs, 30.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.160 [0.923, 1.460] | same |
| explicit_tet4_fd | lumped_matrix | head-t1 | 5 | 150 | 30.0 µs [23.9 µs, 32.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.269 [1.001, 1.669] | same |
| explicit_tet4_fd | lumped_matrix | head-t8 | 5 | 150 | 27.6 µs [20.5 µs, 29.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.196 [1.066, 1.438] | same |
| explicit_tet4_fd | corrector | base-t1 | 5 | 150 | 10.5 µs [9.7 µs, 13.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | corrector | base-t8 | 5 | 150 | 13.3 µs [9.0 µs, 17.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.265 [0.891, 1.328] | same |
| explicit_tet4_fd | corrector | head-t1 | 5 | 150 | 12.3 µs [11.2 µs, 18.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.154 [1.085, 1.342] | same |
| explicit_tet4_fd | corrector | head-t8 | 5 | 150 | 10.3 µs [6.7 µs, 12.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.815 [0.693, 1.205] | same |
| explicit_tet4_fd | step_hooks | base-t1 | 5 | 300 | 475.5 µs [456.4 µs, 532.3 µs] | 1.6 µs [1.5 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | step_hooks | base-t8 | 5 | 300 | 464.2 µs [454.7 µs, 520.1 µs] | 1.5 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.982 [0.874, 1.003] | same |
| explicit_tet4_fd | step_hooks | head-t1 | 5 | 300 | 463.7 µs [455.4 µs, 522.2 µs] | 1.5 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.958 [0.856, 1.144] | same |
| explicit_tet4_fd | step_hooks | head-t8 | 5 | 300 | 447.8 µs [417.1 µs, 497.2 µs] | 1.5 µs [1.4 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.909 [0.802, 1.062] | same |
| explicit_tet4_fd | step_rest | base-t1 | 5 | 150 | 64.92 ms [64.54 ms, 65.24 ms] | 432.8 µs [430.3 µs, 435.0 µs] | 2.7 [2.7, 2.7] | – | same |
| explicit_tet4_fd | step_rest | base-t8 | 5 | 150 | 65.55 ms [65.41 ms, 66.06 ms] | 437.0 µs [436.1 µs, 440.4 µs] | 2.7 [2.7, 2.7] | 1.013 [1.005, 1.017] | same |
| explicit_tet4_fd | step_rest | head-t1 | 5 | 150 | 72.48 ms [72.10 ms, 72.58 ms] | 483.2 µs [480.7 µs, 483.9 µs] | 3.0 [3.0, 3.0] | 1.117 [1.111, 1.119] | same |
| explicit_tet4_fd | step_rest | head-t8 | 5 | 150 | 29.16 ms [28.77 ms, 29.83 ms] | 194.4 µs [191.8 µs, 198.9 µs] | 1.2 [1.2, 1.2] | 0.451 [0.443, 0.457] | same |
| explicit_tet4_fd | internal_forces | base-t1 | 5 | 30 | 237.89 ms [237.15 ms, 238.46 ms] | 7.93 ms [7.91 ms, 7.95 ms] | 48.9 [48.8, 49.1] | – | same |
| explicit_tet4_fd | internal_forces | base-t8 | 5 | 30 | 45.24 ms [45.00 ms, 45.49 ms] | 1.51 ms [1.50 ms, 1.52 ms] | 9.3 [9.3, 9.4] | 0.190 [0.189, 0.192] | same |
| explicit_tet4_fd | internal_forces | head-t1 | 5 | 30 | 237.78 ms [237.18 ms, 239.28 ms] | 7.93 ms [7.91 ms, 7.98 ms] | 48.9 [48.8, 49.2] | 1.000 [0.997, 1.006] | same |
| explicit_tet4_fd | internal_forces | head-t8 | 5 | 30 | 39.82 ms [39.66 ms, 40.10 ms] | 1.33 ms [1.32 ms, 1.34 ms] | 8.2 [8.2, 8.3] | 0.167 [0.167, 0.169] | same |
| explicit_tet4_fd | stresses | base-t1 | 5 | 30 | 75.60 ms [75.44 ms, 75.81 ms] | 2.52 ms [2.51 ms, 2.53 ms] | 15.6 [15.5, 15.6] | – | same |
| explicit_tet4_fd | stresses | base-t8 | 5 | 30 | 40.92 ms [40.59 ms, 43.42 ms] | 1.36 ms [1.35 ms, 1.45 ms] | 8.4 [8.4, 8.9] | 0.541 [0.538, 0.574] | same |
| explicit_tet4_fd | stresses | head-t1 | 5 | 30 | 75.61 ms [75.16 ms, 75.95 ms] | 2.52 ms [2.51 ms, 2.53 ms] | 15.6 [15.5, 15.6] | 1.001 [0.991, 1.005] | same |
| explicit_tet4_fd | stresses | head-t8 | 5 | 30 | 40.27 ms [39.73 ms, 40.45 ms] | 1.34 ms [1.32 ms, 1.35 ms] | 8.3 [8.2, 8.3] | 0.532 [0.526, 0.536] | same |
| explicit_tet4_fd | energies | base-t1 | 5 | 30 | 77.90 ms [77.73 ms, 78.04 ms] | 2.60 ms [2.59 ms, 2.60 ms] | 16.0 [16.0, 16.1] | – | same |
| explicit_tet4_fd | energies | base-t8 | 5 | 30 | 78.14 ms [77.89 ms, 78.40 ms] | 2.60 ms [2.60 ms, 2.61 ms] | 16.1 [16.0, 16.1] | 1.005 [0.998, 1.006] | same |
| explicit_tet4_fd | energies | head-t1 | 5 | 30 | 79.75 ms [79.30 ms, 79.96 ms] | 2.66 ms [2.64 ms, 2.67 ms] | 16.4 [16.3, 16.5] | 1.022 [1.020, 1.027] | same |
| explicit_tet4_fd | energies | head-t8 | 5 | 30 | 78.95 ms [78.76 ms, 79.56 ms] | 2.63 ms [2.63 ms, 2.65 ms] | 16.2 [16.2, 16.4] | 1.014 [1.011, 1.022] | same |
| explicit_tet4_ss | mesh | base-t1 | 5 | 1 | 3.25 ms [3.10 ms, 3.35 ms] | 3.25 ms [3.10 ms, 3.35 ms] | 20.1 [19.1, 20.7] | – | same |
| explicit_tet4_ss | mesh | base-t8 | 5 | 1 | 3.21 ms [3.09 ms, 3.23 ms] | 3.21 ms [3.09 ms, 3.23 ms] | 19.8 [19.1, 20.0] | 0.990 [0.949, 1.044] | same |
| explicit_tet4_ss | mesh | head-t1 | 5 | 1 | 3.21 ms [3.18 ms, 3.25 ms] | 3.21 ms [3.18 ms, 3.25 ms] | 19.8 [19.6, 20.1] | 0.995 [0.969, 1.037] | same |
| explicit_tet4_ss | mesh | head-t8 | 5 | 1 | 3.26 ms [3.18 ms, 3.33 ms] | 3.26 ms [3.18 ms, 3.33 ms] | 20.2 [19.6, 20.5] | 0.999 [0.963, 1.058] | same |
| explicit_tet4_ss | init | base-t1 | 5 | 1 | 48.70 ms [47.94 ms, 49.11 ms] | 48.70 ms [47.94 ms, 49.11 ms] | 300.6 [295.9, 303.1] | – | same |
| explicit_tet4_ss | init | base-t8 | 5 | 1 | 48.49 ms [48.16 ms, 49.92 ms] | 48.49 ms [48.16 ms, 49.92 ms] | 299.3 [297.3, 308.2] | 0.999 [0.987, 1.041] | same |
| explicit_tet4_ss | init | head-t1 | 5 | 1 | 47.71 ms [47.03 ms, 47.77 ms] | 47.71 ms [47.03 ms, 47.77 ms] | 294.5 [290.3, 294.9] | 0.972 [0.966, 0.996] | same |
| explicit_tet4_ss | init | head-t8 | 5 | 1 | 48.22 ms [47.33 ms, 49.57 ms] | 48.22 ms [47.33 ms, 49.57 ms] | 297.6 [292.2, 306.0] | 0.993 [0.987, 1.011] | same |
| explicit_tet4_ss | stable_time_step | base-t1 | 5 | 5 | 32.25 ms [32.22 ms, 32.42 ms] | 6.45 ms [6.44 ms, 6.48 ms] | 39.8 [39.8, 40.0] | – | same |
| explicit_tet4_ss | stable_time_step | base-t8 | 5 | 5 | 32.26 ms [32.21 ms, 32.57 ms] | 6.45 ms [6.44 ms, 6.51 ms] | 39.8 [39.8, 40.2] | 1.001 [0.993, 1.010] | same |
| explicit_tet4_ss | stable_time_step | head-t1 | 5 | 5 | 32.58 ms [32.37 ms, 33.35 ms] | 6.52 ms [6.47 ms, 6.67 ms] | 40.2 [40.0, 41.2] | 1.011 [0.998, 1.033] | same |
| explicit_tet4_ss | stable_time_step | head-t8 | 5 | 5 | 32.54 ms [32.44 ms, 33.07 ms] | 6.51 ms [6.49 ms, 6.61 ms] | 40.2 [40.0, 40.8] | 1.008 [1.004, 1.026] | same |
| explicit_tet4_ss | solve_step | base-t1 | 5 | 200 | 1.286 s [1.274 s, 1.292 s] | 6.43 ms [6.37 ms, 6.46 ms] | 39.7 [39.3, 39.9] | – | same |
| explicit_tet4_ss | solve_step | base-t8 | 5 | 200 | 478.51 ms [475.55 ms, 483.75 ms] | 2.39 ms [2.38 ms, 2.42 ms] | 14.8 [14.7, 14.9] | 0.373 [0.369, 0.376] | same |
| explicit_tet4_ss | solve_step | head-t1 | 5 | 200 | 1.299 s [1.295 s, 1.306 s] | 6.49 ms [6.48 ms, 6.53 ms] | 40.1 [40.0, 40.3] | 1.011 [1.005, 1.017] | same |
| explicit_tet4_ss | solve_step | head-t8 | 5 | 200 | 348.16 ms [345.93 ms, 362.99 ms] | 1.74 ms [1.73 ms, 1.81 ms] | 10.7 [10.7, 11.2] | 0.270 [0.269, 0.282] | same |
| explicit_tet4_ss | predictor | base-t1 | 5 | 200 | 8.3 µs [6.7 µs, 9.9 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | base-t8 | 5 | 200 | 7.3 µs [6.9 µs, 7.9 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.829 [0.724, 1.095] | same |
| explicit_tet4_ss | predictor | head-t1 | 5 | 200 | 9.3 µs [6.4 µs, 20.2 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.115 [0.646, 3.013] | same |
| explicit_tet4_ss | predictor | head-t8 | 5 | 200 | 6.7 µs [5.3 µs, 12.4 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.794 [0.600, 1.494] | same |
| explicit_tet4_ss | residual | base-t1 | 5 | 200 | 1.199 s [1.188 s, 1.205 s] | 6.00 ms [5.94 ms, 6.02 ms] | 37.0 [36.7, 37.2] | – | same |
| explicit_tet4_ss | residual | base-t8 | 5 | 200 | 389.78 ms [386.64 ms, 394.65 ms] | 1.95 ms [1.93 ms, 1.97 ms] | 12.0 [11.9, 12.2] | 0.325 [0.322, 0.329] | same |
| explicit_tet4_ss | residual | head-t1 | 5 | 200 | 1.202 s [1.197 s, 1.208 s] | 6.01 ms [5.99 ms, 6.04 ms] | 37.1 [37.0, 37.3] | 1.003 [0.998, 1.008] | same |
| explicit_tet4_ss | residual | head-t8 | 5 | 200 | 307.50 ms [305.12 ms, 322.23 ms] | 1.54 ms [1.53 ms, 1.61 ms] | 9.5 [9.4, 9.9] | 0.255 [0.255, 0.269] | same |
| explicit_tet4_ss | lumped_matrix | base-t1 | 5 | 200 | 30.9 µs [29.9 µs, 38.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | base-t8 | 5 | 200 | 33.9 µs [29.9 µs, 36.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.000 [0.948, 1.098] | same |
| explicit_tet4_ss | lumped_matrix | head-t1 | 5 | 200 | 29.4 µs [26.5 µs, 38.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.950 [0.684, 1.282] | same |
| explicit_tet4_ss | lumped_matrix | head-t8 | 5 | 200 | 24.7 µs [23.1 µs, 31.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.770 [0.749, 0.891] | same |
| explicit_tet4_ss | corrector | base-t1 | 5 | 200 | 13.9 µs [11.2 µs, 18.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | base-t8 | 5 | 200 | 15.6 µs [9.9 µs, 19.5 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.127 [0.642, 1.529] | same |
| explicit_tet4_ss | corrector | head-t1 | 5 | 200 | 21.9 µs [13.2 µs, 29.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.360 [0.954, 2.520] | same |
| explicit_tet4_ss | corrector | head-t8 | 5 | 200 | 19.7 µs [13.7 µs, 22.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.226 [0.994, 1.662] | same |
| explicit_tet4_ss | step_hooks | base-t1 | 5 | 400 | 524.8 µs [509.9 µs, 548.7 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | base-t8 | 5 | 400 | 500.1 µs [458.9 µs, 515.7 µs] | 1.3 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.948 [0.900, 0.970] | same |
| explicit_tet4_ss | step_hooks | head-t1 | 5 | 400 | 534.2 µs [530.5 µs, 590.4 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.048 [0.971, 1.125] | same |
| explicit_tet4_ss | step_hooks | head-t8 | 5 | 400 | 490.7 µs [468.6 µs, 530.5 µs] | 1.2 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.894 [0.886, 1.013] | same |
| explicit_tet4_ss | step_rest | base-t1 | 5 | 200 | 86.69 ms [85.39 ms, 86.77 ms] | 433.4 µs [427.0 µs, 433.9 µs] | 2.7 [2.6, 2.7] | – | same |
| explicit_tet4_ss | step_rest | base-t8 | 5 | 200 | 88.52 ms [88.15 ms, 92.66 ms] | 442.6 µs [440.7 µs, 463.3 µs] | 2.7 [2.7, 2.9] | 1.026 [1.016, 1.069] | same |
| explicit_tet4_ss | step_rest | head-t1 | 5 | 200 | 96.75 ms [96.70 ms, 97.11 ms] | 483.7 µs [483.5 µs, 485.6 µs] | 3.0 [3.0, 3.0] | 1.120 [1.115, 1.137] | same |
| explicit_tet4_ss | step_rest | head-t8 | 5 | 200 | 40.20 ms [39.80 ms, 40.56 ms] | 201.0 µs [199.0 µs, 202.8 µs] | 1.2 [1.2, 1.3] | 0.466 [0.463, 0.467] | same |
| explicit_tet4_ss | internal_forces | base-t1 | 5 | 40 | 229.01 ms [226.60 ms, 229.68 ms] | 5.73 ms [5.67 ms, 5.74 ms] | 35.3 [35.0, 35.4] | – | same |
| explicit_tet4_ss | internal_forces | base-t8 | 5 | 40 | 61.90 ms [61.63 ms, 63.68 ms] | 1.55 ms [1.54 ms, 1.59 ms] | 9.6 [9.5, 9.8] | 0.271 [0.270, 0.278] | same |
| explicit_tet4_ss | internal_forces | head-t1 | 5 | 40 | 228.44 ms [227.35 ms, 228.98 ms] | 5.71 ms [5.68 ms, 5.72 ms] | 35.3 [35.1, 35.3] | 0.996 [0.992, 1.008] | same |
| explicit_tet4_ss | internal_forces | head-t8 | 5 | 40 | 50.97 ms [50.73 ms, 52.12 ms] | 1.27 ms [1.27 ms, 1.30 ms] | 7.9 [7.8, 8.0] | 0.223 [0.221, 0.228] | same |
| explicit_tet4_ss | stresses | base-t1 | 5 | 40 | 86.31 ms [86.20 ms, 86.87 ms] | 2.16 ms [2.15 ms, 2.17 ms] | 13.3 [13.3, 13.4] | – | same |
| explicit_tet4_ss | stresses | base-t8 | 5 | 40 | 40.03 ms [39.72 ms, 40.44 ms] | 1.00 ms [992.9 µs, 1.01 ms] | 6.2 [6.1, 6.2] | 0.461 [0.458, 0.469] | same |
| explicit_tet4_ss | stresses | head-t1 | 5 | 40 | 86.42 ms [85.53 ms, 87.00 ms] | 2.16 ms [2.14 ms, 2.18 ms] | 13.3 [13.2, 13.4] | 0.997 [0.992, 1.003] | same |
| explicit_tet4_ss | stresses | head-t8 | 5 | 40 | 38.96 ms [37.75 ms, 40.42 ms] | 974.1 µs [943.8 µs, 1.01 ms] | 6.0 [5.8, 6.2] | 0.449 [0.437, 0.466] | same |
| explicit_tet4_ss | energies | base-t1 | 5 | 40 | 82.62 ms [82.28 ms, 83.14 ms] | 2.07 ms [2.06 ms, 2.08 ms] | 12.7 [12.7, 12.8] | – | same |
| explicit_tet4_ss | energies | base-t8 | 5 | 40 | 82.96 ms [82.72 ms, 83.43 ms] | 2.07 ms [2.07 ms, 2.09 ms] | 12.8 [12.8, 12.9] | 1.005 [0.998, 1.010] | same |
| explicit_tet4_ss | energies | head-t1 | 5 | 40 | 84.68 ms [84.60 ms, 85.68 ms] | 2.12 ms [2.11 ms, 2.14 ms] | 13.1 [13.1, 13.2] | 1.027 [1.018, 1.041] | same |
| explicit_tet4_ss | energies | head-t8 | 5 | 40 | 84.12 ms [83.49 ms, 84.35 ms] | 2.10 ms [2.09 ms, 2.11 ms] | 13.0 [12.9, 13.0] | 1.016 [1.015, 1.019] | same |
| explicit_thermal | mesh | base-t1 | 5 | 1 | 2.19 ms [2.06 ms, 2.22 ms] | 2.19 ms [2.06 ms, 2.22 ms] | 20.8 [19.5, 21.1] | – | same |
| explicit_thermal | mesh | base-t8 | 5 | 1 | 2.15 ms [2.09 ms, 2.21 ms] | 2.15 ms [2.09 ms, 2.21 ms] | 20.4 [19.8, 20.9] | 0.988 [0.966, 1.017] | same |
| explicit_thermal | mesh | head-t1 | 5 | 1 | 2.12 ms [2.02 ms, 2.17 ms] | 2.12 ms [2.02 ms, 2.17 ms] | 20.1 [19.1, 20.6] | 0.970 [0.924, 1.010] | same |
| explicit_thermal | mesh | head-t8 | 5 | 1 | 2.15 ms [2.06 ms, 2.18 ms] | 2.15 ms [2.06 ms, 2.18 ms] | 20.4 [19.5, 20.7] | 0.982 [0.927, 1.060] | same |
| explicit_thermal | init | base-t1 | 5 | 1 | 31.47 ms [31.25 ms, 32.47 ms] | 31.47 ms [31.25 ms, 32.47 ms] | 298.5 [296.4, 307.9] | – | same |
| explicit_thermal | init | base-t8 | 5 | 1 | 31.84 ms [30.82 ms, 32.47 ms] | 31.84 ms [30.82 ms, 32.47 ms] | 301.9 [292.2, 307.9] | 1.005 [0.979, 1.013] | same |
| explicit_thermal | init | head-t1 | 5 | 1 | 30.70 ms [30.31 ms, 31.31 ms] | 30.70 ms [30.31 ms, 31.31 ms] | 291.2 [287.4, 296.9] | 0.977 [0.946, 0.991] | same |
| explicit_thermal | init | head-t8 | 5 | 1 | 31.70 ms [30.79 ms, 32.25 ms] | 31.70 ms [30.79 ms, 32.25 ms] | 300.6 [292.0, 305.9] | 0.994 [0.975, 1.026] | same |
| explicit_thermal | stable_time_step | base-t1 | 5 | 5 | 20.98 ms [20.92 ms, 21.19 ms] | 4.20 ms [4.18 ms, 4.24 ms] | 39.8 [39.7, 40.2] | – | same |
| explicit_thermal | stable_time_step | base-t8 | 5 | 5 | 21.03 ms [20.91 ms, 21.36 ms] | 4.21 ms [4.18 ms, 4.27 ms] | 39.9 [39.7, 40.5] | 1.002 [1.000, 1.015] | same |
| explicit_thermal | stable_time_step | head-t1 | 5 | 5 | 21.07 ms [21.06 ms, 21.11 ms] | 4.21 ms [4.21 ms, 4.22 ms] | 40.0 [39.9, 40.0] | 1.004 [0.994, 1.008] | same |
| explicit_thermal | stable_time_step | head-t8 | 5 | 5 | 21.20 ms [21.05 ms, 21.22 ms] | 4.24 ms [4.21 ms, 4.24 ms] | 40.2 [39.9, 40.2] | 1.007 [1.001, 1.014] | same |
| explicit_thermal | solve_step | base-t1 | 5 | 300 | 1.219 s [1.199 s, 1.232 s] | 4.06 ms [4.00 ms, 4.11 ms] | 38.5 [37.9, 38.9] | – | same |
| explicit_thermal | solve_step | base-t8 | 5 | 300 | 410.53 ms [402.09 ms, 412.84 ms] | 1.37 ms [1.34 ms, 1.38 ms] | 13.0 [12.7, 13.0] | 0.335 [0.335, 0.338] | same |
| explicit_thermal | solve_step | head-t1 | 5 | 300 | 1.226 s [1.211 s, 1.231 s] | 4.09 ms [4.04 ms, 4.10 ms] | 38.8 [38.3, 38.9] | 1.002 [1.000, 1.011] | same |
| explicit_thermal | solve_step | head-t8 | 5 | 300 | 282.29 ms [278.13 ms, 283.30 ms] | 941.0 µs [927.1 µs, 944.3 µs] | 8.9 [8.8, 9.0] | 0.231 [0.229, 0.233] | same |
| explicit_thermal | predictor | base-t1 | 5 | 300 | 8.6 µs [8.1 µs, 28.9 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | predictor | base-t8 | 5 | 300 | 9.0 µs [8.1 µs, 12.1 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 1.048 [0.300, 1.485] | same |
| explicit_thermal | predictor | head-t1 | 5 | 300 | 23.5 µs [12.9 µs, 37.9 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.547 [0.813, 3.017] | same |
| explicit_thermal | predictor | head-t8 | 5 | 300 | 8.6 µs [7.9 µs, 9.4 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.943 [0.273, 1.091] | same |
| explicit_thermal | residual | base-t1 | 5 | 300 | 1.130 s [1.112 s, 1.142 s] | 3.77 ms [3.71 ms, 3.81 ms] | 35.7 [35.2, 36.1] | – | same |
| explicit_thermal | residual | base-t8 | 5 | 300 | 321.70 ms [316.75 ms, 324.14 ms] | 1.07 ms [1.06 ms, 1.08 ms] | 10.2 [10.0, 10.2] | 0.284 [0.283, 0.285] | same |
| explicit_thermal | residual | head-t1 | 5 | 300 | 1.133 s [1.119 s, 1.138 s] | 3.78 ms [3.73 ms, 3.79 ms] | 35.8 [35.4, 36.0] | 0.998 [0.996, 1.006] | same |
| explicit_thermal | residual | head-t8 | 5 | 300 | 245.31 ms [241.45 ms, 246.55 ms] | 817.7 µs [804.8 µs, 821.8 µs] | 7.8 [7.6, 7.8] | 0.216 [0.215, 0.218] | same |
| explicit_thermal | lumped_matrix | base-t1 | 5 | 300 | 40.6 µs [39.0 µs, 59.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | lumped_matrix | base-t8 | 5 | 300 | 35.5 µs [30.1 µs, 43.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.871 [0.648, 0.911] | same |
| explicit_thermal | lumped_matrix | head-t1 | 5 | 300 | 42.6 µs [29.8 µs, 61.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.011 [0.710, 1.566] | same |
| explicit_thermal | lumped_matrix | head-t8 | 5 | 300 | 21.7 µs [18.9 µs, 23.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.497 [0.362, 0.580] | same |
| explicit_thermal | corrector | base-t1 | 5 | 300 | 16.6 µs [13.2 µs, 17.5 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | corrector | base-t8 | 5 | 300 | 15.4 µs [11.6 µs, 19.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.907 [0.740, 1.169] | same |
| explicit_thermal | corrector | head-t1 | 5 | 300 | 19.4 µs [17.7 µs, 23.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.217 [1.087, 1.461] | same |
| explicit_thermal | corrector | head-t8 | 5 | 300 | 15.6 µs [11.9 µs, 18.7 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.941 [0.749, 1.141] | same |
| explicit_thermal | step_hooks | base-t1 | 5 | 600 | 748.8 µs [682.1 µs, 778.4 µs] | 1.2 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | step_hooks | base-t8 | 5 | 600 | 562.6 µs [516.2 µs, 651.0 µs] | 0.9 µs [0.9 µs, 1.1 µs] | 0.0 [0.0, 0.0] | 0.757 [0.713, 0.935] | same |
| explicit_thermal | step_hooks | head-t1 | 5 | 600 | 717.3 µs [668.2 µs, 754.4 µs] | 1.2 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.964 [0.911, 1.052] | same |
| explicit_thermal | step_hooks | head-t8 | 5 | 600 | 475.6 µs [456.9 µs, 496.2 µs] | 0.8 µs [0.8 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 0.662 [0.611, 0.679] | same |
| explicit_thermal | step_rest | base-t1 | 5 | 300 | 86.91 ms [84.84 ms, 88.26 ms] | 289.7 µs [282.8 µs, 294.2 µs] | 2.7 [2.7, 2.8] | – | same |
| explicit_thermal | step_rest | base-t8 | 5 | 300 | 87.54 ms [84.14 ms, 87.73 ms] | 291.8 µs [280.5 µs, 292.4 µs] | 2.8 [2.7, 2.8] | 0.992 [0.987, 1.028] | same |
| explicit_thermal | step_rest | head-t1 | 5 | 300 | 92.02 ms [90.23 ms, 92.64 ms] | 306.7 µs [300.8 µs, 308.8 µs] | 2.9 [2.9, 2.9] | 1.057 [1.045, 1.076] | same |
| explicit_thermal | step_rest | head-t8 | 5 | 300 | 35.60 ms [34.81 ms, 35.67 ms] | 118.7 µs [116.0 µs, 118.9 µs] | 1.1 [1.1, 1.1] | 0.410 [0.397, 0.418] | same |
| explicit_thermal | internal_forces | base-t1 | 5 | 60 | 213.70 ms [213.29 ms, 215.49 ms] | 3.56 ms [3.55 ms, 3.59 ms] | 33.8 [33.7, 34.1] | – | same |
| explicit_thermal | internal_forces | base-t8 | 5 | 60 | 46.72 ms [45.98 ms, 47.22 ms] | 778.7 µs [766.3 µs, 787.0 µs] | 7.4 [7.3, 7.5] | 0.219 [0.216, 0.221] | same |
| explicit_thermal | internal_forces | head-t1 | 5 | 60 | 216.43 ms [213.57 ms, 217.92 ms] | 3.61 ms [3.56 ms, 3.63 ms] | 34.2 [33.8, 34.4] | 1.004 [1.001, 1.020] | same |
| explicit_thermal | internal_forces | head-t8 | 5 | 60 | 35.72 ms [35.46 ms, 35.92 ms] | 595.4 µs [591.0 µs, 598.6 µs] | 5.6 [5.6, 5.7] | 0.167 [0.166, 0.168] | same |
| explicit_thermal | stresses | base-t1 | 5 | 60 | 82.39 ms [81.69 ms, 82.64 ms] | 1.37 ms [1.36 ms, 1.38 ms] | 13.0 [12.9, 13.1] | – | same |
| explicit_thermal | stresses | base-t8 | 5 | 60 | 36.15 ms [34.52 ms, 37.57 ms] | 602.5 µs [575.4 µs, 626.1 µs] | 5.7 [5.5, 5.9] | 0.437 [0.423, 0.456] | same |
| explicit_thermal | stresses | head-t1 | 5 | 60 | 82.14 ms [81.30 ms, 82.84 ms] | 1.37 ms [1.36 ms, 1.38 ms] | 13.0 [12.8, 13.1] | 1.000 [0.989, 1.006] | same |
| explicit_thermal | stresses | head-t8 | 5 | 60 | 33.47 ms [33.19 ms, 33.97 ms] | 557.8 µs [553.1 µs, 566.2 µs] | 5.3 [5.2, 5.4] | 0.406 [0.405, 0.412] | same |
| explicit_thermal | energies | base-t1 | 5 | 60 | 80.23 ms [79.72 ms, 80.39 ms] | 1.34 ms [1.33 ms, 1.34 ms] | 12.7 [12.6, 12.7] | – | same |
| explicit_thermal | energies | base-t8 | 5 | 60 | 80.32 ms [80.06 ms, 81.09 ms] | 1.34 ms [1.33 ms, 1.35 ms] | 12.7 [12.7, 12.8] | 1.004 [1.001, 1.011] | same |
| explicit_thermal | energies | head-t1 | 5 | 60 | 82.72 ms [82.53 ms, 83.95 ms] | 1.38 ms [1.38 ms, 1.40 ms] | 13.1 [13.0, 13.3] | 1.035 [1.029, 1.046] | same |
| explicit_thermal | energies | head-t8 | 5 | 60 | 81.37 ms [80.87 ms, 81.68 ms] | 1.36 ms [1.35 ms, 1.36 ms] | 12.9 [12.8, 12.9] | 1.014 [1.012, 1.018] | same |
| explicit_tri3_fd | mesh | base-t1 | 5 | 1 | 4.20 ms [4.05 ms, 4.29 ms] | 4.20 ms [4.05 ms, 4.29 ms] | 23.4 [22.5, 23.8] | – | same |
| explicit_tri3_fd | mesh | base-t8 | 5 | 1 | 4.10 ms [3.91 ms, 4.29 ms] | 4.10 ms [3.91 ms, 4.29 ms] | 22.8 [21.7, 23.9] | 0.980 [0.938, 1.014] | same |
| explicit_tri3_fd | mesh | head-t1 | 5 | 1 | 4.18 ms [4.04 ms, 4.25 ms] | 4.18 ms [4.04 ms, 4.25 ms] | 23.2 [22.4, 23.6] | 0.991 [0.971, 1.029] | same |
| explicit_tri3_fd | mesh | head-t8 | 5 | 1 | 4.07 ms [3.97 ms, 4.19 ms] | 4.07 ms [3.97 ms, 4.19 ms] | 22.6 [22.1, 23.3] | 0.977 [0.930, 1.015] | same |
| explicit_tri3_fd | init | base-t1 | 5 | 1 | 40.58 ms [39.91 ms, 41.04 ms] | 40.58 ms [39.91 ms, 41.04 ms] | 225.4 [221.7, 228.0] | – | same |
| explicit_tri3_fd | init | base-t8 | 5 | 1 | 41.06 ms [39.88 ms, 43.47 ms] | 41.06 ms [39.88 ms, 43.47 ms] | 228.1 [221.6, 241.5] | 1.012 [0.999, 1.059] | same |
| explicit_tri3_fd | init | head-t1 | 5 | 1 | 39.97 ms [39.49 ms, 40.58 ms] | 39.97 ms [39.49 ms, 40.58 ms] | 222.0 [219.4, 225.4] | 0.990 [0.974, 0.996] | same |
| explicit_tri3_fd | init | head-t8 | 5 | 1 | 40.29 ms [39.84 ms, 41.23 ms] | 40.29 ms [39.84 ms, 41.23 ms] | 223.8 [221.4, 229.1] | 0.994 [0.982, 1.012] | same |
| explicit_tri3_fd | stable_time_step | base-t1 | 5 | 5 | 22.60 ms [22.52 ms, 24.14 ms] | 4.52 ms [4.50 ms, 4.83 ms] | 25.1 [25.0, 26.8] | – | same |
| explicit_tri3_fd | stable_time_step | base-t8 | 5 | 5 | 23.10 ms [22.53 ms, 23.16 ms] | 4.62 ms [4.51 ms, 4.63 ms] | 25.7 [25.0, 25.7] | 1.020 [0.958, 1.025] | same |
| explicit_tri3_fd | stable_time_step | head-t1 | 5 | 5 | 22.60 ms [22.50 ms, 23.50 ms] | 4.52 ms [4.50 ms, 4.70 ms] | 25.1 [25.0, 26.1] | 0.999 [0.973, 1.005] | same |
| explicit_tri3_fd | stable_time_step | head-t8 | 5 | 5 | 22.78 ms [22.54 ms, 23.89 ms] | 4.56 ms [4.51 ms, 4.78 ms] | 25.3 [25.0, 26.5] | 1.011 [0.943, 1.057] | same |
| explicit_tri3_fd | solve_step | base-t1 | 5 | 200 | 1.456 s [1.443 s, 1.460 s] | 7.28 ms [7.22 ms, 7.30 ms] | 40.4 [40.1, 40.6] | – | same |
| explicit_tri3_fd | solve_step | base-t8 | 5 | 200 | 497.57 ms [492.80 ms, 501.82 ms] | 2.49 ms [2.46 ms, 2.51 ms] | 13.8 [13.7, 13.9] | 0.343 [0.338, 0.345] | same |
| explicit_tri3_fd | solve_step | head-t1 | 5 | 200 | 1.475 s [1.460 s, 1.482 s] | 7.38 ms [7.30 ms, 7.41 ms] | 41.0 [40.5, 41.2] | 1.015 [1.009, 1.017] | same |
| explicit_tri3_fd | solve_step | head-t8 | 5 | 200 | 296.89 ms [292.14 ms, 298.84 ms] | 1.48 ms [1.46 ms, 1.49 ms] | 8.2 [8.1, 8.3] | 0.203 [0.202, 0.205] | same |
| explicit_tri3_fd | predictor | base-t1 | 5 | 200 | 15.0 µs [10.5 µs, 22.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | predictor | base-t8 | 5 | 200 | 20.9 µs [14.1 µs, 25.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.138 [0.828, 2.382] | same |
| explicit_tri3_fd | predictor | head-t1 | 5 | 200 | 16.5 µs [14.8 µs, 26.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.195 [0.667, 2.546] | same |
| explicit_tri3_fd | predictor | head-t8 | 5 | 200 | 8.2 µs [6.3 µs, 19.5 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.607 [0.351, 1.863] | same |
| explicit_tri3_fd | residual | base-t1 | 5 | 200 | 1.281 s [1.271 s, 1.283 s] | 6.40 ms [6.35 ms, 6.42 ms] | 35.6 [35.3, 35.6] | – | same |
| explicit_tri3_fd | residual | base-t8 | 5 | 200 | 319.51 ms [317.42 ms, 323.67 ms] | 1.60 ms [1.59 ms, 1.62 ms] | 8.9 [8.8, 9.0] | 0.251 [0.247, 0.254] | same |
| explicit_tri3_fd | residual | head-t1 | 5 | 200 | 1.280 s [1.267 s, 1.285 s] | 6.40 ms [6.33 ms, 6.43 ms] | 35.6 [35.2, 35.7] | 1.002 [0.997, 1.003] | same |
| explicit_tri3_fd | residual | head-t8 | 5 | 200 | 227.52 ms [223.54 ms, 228.82 ms] | 1.14 ms [1.12 ms, 1.14 ms] | 6.3 [6.2, 6.4] | 0.177 [0.176, 0.179] | same |
| explicit_tri3_fd | lumped_matrix | base-t1 | 5 | 200 | 38.5 µs [29.5 µs, 40.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | lumped_matrix | base-t8 | 5 | 200 | 42.9 µs [37.6 µs, 48.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.211 [0.939, 1.456] | same |
| explicit_tri3_fd | lumped_matrix | head-t1 | 5 | 200 | 42.4 µs [36.3 µs, 45.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.058 [0.906, 1.440] | same |
| explicit_tri3_fd | lumped_matrix | head-t8 | 5 | 200 | 39.2 µs [34.7 µs, 41.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.976 [0.903, 1.358] | same |
| explicit_tri3_fd | corrector | base-t1 | 5 | 200 | 16.3 µs [14.4 µs, 22.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | corrector | base-t8 | 5 | 200 | 27.6 µs [22.3 µs, 29.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.553 [1.026, 2.005] | same |
| explicit_tri3_fd | corrector | head-t1 | 5 | 200 | 22.0 µs [14.9 µs, 33.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.465 [0.687, 2.046] | same |
| explicit_tri3_fd | corrector | head-t8 | 5 | 200 | 12.0 µs [9.2 µs, 27.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.826 [0.486, 1.857] | same |
| explicit_tri3_fd | step_hooks | base-t1 | 5 | 400 | 681.5 µs [668.2 µs, 756.1 µs] | 1.7 µs [1.7 µs, 1.9 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | step_hooks | base-t8 | 5 | 400 | 667.3 µs [621.3 µs, 751.4 µs] | 1.7 µs [1.6 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 0.984 [0.826, 1.124] | same |
| explicit_tri3_fd | step_hooks | head-t1 | 5 | 400 | 677.8 µs [642.4 µs, 751.2 µs] | 1.7 µs [1.6 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 0.988 [0.850, 1.121] | same |
| explicit_tri3_fd | step_hooks | head-t8 | 5 | 400 | 610.5 µs [581.3 µs, 641.8 µs] | 1.5 µs [1.5 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 0.890 [0.807, 0.960] | same |
| explicit_tri3_fd | step_rest | base-t1 | 5 | 200 | 175.12 ms [171.65 ms, 175.75 ms] | 875.6 µs [858.3 µs, 878.7 µs] | 4.9 [4.8, 4.9] | – | same |
| explicit_tri3_fd | step_rest | base-t8 | 5 | 200 | 176.76 ms [174.63 ms, 177.39 ms] | 883.8 µs [873.2 µs, 886.9 µs] | 4.9 [4.9, 4.9] | 1.013 [0.994, 1.021] | same |
| explicit_tri3_fd | step_rest | head-t1 | 5 | 200 | 194.55 ms [192.18 ms, 196.15 ms] | 972.7 µs [960.9 µs, 980.7 µs] | 5.4 [5.3, 5.4] | 1.116 [1.103, 1.121] | same |
| explicit_tri3_fd | step_rest | head-t8 | 5 | 200 | 68.38 ms [67.89 ms, 69.26 ms] | 341.9 µs [339.4 µs, 346.3 µs] | 1.9 [1.9, 1.9] | 0.392 [0.390, 0.398] | same |
| explicit_tri3_fd | internal_forces | base-t1 | 5 | 40 | 233.85 ms [232.13 ms, 234.30 ms] | 5.85 ms [5.80 ms, 5.86 ms] | 32.5 [32.2, 32.5] | – | same |
| explicit_tri3_fd | internal_forces | base-t8 | 5 | 40 | 38.80 ms [38.52 ms, 38.86 ms] | 969.9 µs [963.0 µs, 971.4 µs] | 5.4 [5.4, 5.4] | 0.166 [0.166, 0.166] | same |
| explicit_tri3_fd | internal_forces | head-t1 | 5 | 40 | 232.82 ms [230.39 ms, 234.03 ms] | 5.82 ms [5.76 ms, 5.85 ms] | 32.3 [32.0, 32.5] | 0.997 [0.993, 0.999] | same |
| explicit_tri3_fd | internal_forces | head-t8 | 5 | 40 | 31.96 ms [31.65 ms, 32.20 ms] | 799.0 µs [791.2 µs, 805.0 µs] | 4.4 [4.4, 4.5] | 0.137 [0.136, 0.138] | same |
| explicit_tri3_fd | stresses | base-t1 | 5 | 40 | 55.99 ms [55.67 ms, 57.26 ms] | 1.40 ms [1.39 ms, 1.43 ms] | 7.8 [7.7, 8.0] | – | same |
| explicit_tri3_fd | stresses | base-t8 | 5 | 40 | 28.43 ms [27.70 ms, 29.30 ms] | 710.8 µs [692.4 µs, 732.4 µs] | 3.9 [3.8, 4.1] | 0.509 [0.493, 0.513] | same |
| explicit_tri3_fd | stresses | head-t1 | 5 | 40 | 55.79 ms [55.34 ms, 56.95 ms] | 1.39 ms [1.38 ms, 1.42 ms] | 7.7 [7.7, 7.9] | 0.995 [0.985, 1.005] | same |
| explicit_tri3_fd | stresses | head-t8 | 5 | 40 | 27.26 ms [25.63 ms, 27.44 ms] | 681.4 µs [640.8 µs, 686.0 µs] | 3.8 [3.6, 3.8] | 0.487 [0.456, 0.493] | same |
| explicit_tri3_fd | energies | base-t1 | 5 | 40 | 96.08 ms [95.19 ms, 96.54 ms] | 2.40 ms [2.38 ms, 2.41 ms] | 13.3 [13.2, 13.4] | – | same |
| explicit_tri3_fd | energies | base-t8 | 5 | 40 | 96.23 ms [95.79 ms, 96.98 ms] | 2.41 ms [2.39 ms, 2.42 ms] | 13.4 [13.3, 13.5] | 0.997 [0.996, 1.012] | same |
| explicit_tri3_fd | energies | head-t1 | 5 | 40 | 98.84 ms [98.60 ms, 99.53 ms] | 2.47 ms [2.47 ms, 2.49 ms] | 13.7 [13.7, 13.8] | 1.033 [1.021, 1.037] | same |
| explicit_tri3_fd | energies | head-t8 | 5 | 40 | 97.91 ms [96.59 ms, 98.25 ms] | 2.45 ms [2.41 ms, 2.46 ms] | 13.6 [13.4, 13.6] | 1.018 [1.013, 1.020] | same |
| explicit_tri3_ss | mesh | base-t1 | 5 | 1 | 4.01 ms [3.95 ms, 4.12 ms] | 4.01 ms [3.95 ms, 4.12 ms] | 22.3 [21.9, 22.9] | – | same |
| explicit_tri3_ss | mesh | base-t8 | 5 | 1 | 4.01 ms [3.97 ms, 4.21 ms] | 4.01 ms [3.97 ms, 4.21 ms] | 22.3 [22.0, 23.4] | 1.007 [0.978, 1.021] | same |
| explicit_tri3_ss | mesh | head-t1 | 5 | 1 | 4.05 ms [4.01 ms, 4.12 ms] | 4.05 ms [4.01 ms, 4.12 ms] | 22.5 [22.3, 22.9] | 1.014 [0.974, 1.029] | same |
| explicit_tri3_ss | mesh | head-t8 | 5 | 1 | 4.02 ms [3.98 ms, 4.15 ms] | 4.02 ms [3.98 ms, 4.15 ms] | 22.3 [22.1, 23.0] | 1.005 [0.990, 1.023] | same |
| explicit_tri3_ss | init | base-t1 | 5 | 1 | 36.99 ms [36.70 ms, 37.74 ms] | 36.99 ms [36.70 ms, 37.74 ms] | 205.5 [203.9, 209.7] | – | same |
| explicit_tri3_ss | init | base-t8 | 5 | 1 | 37.63 ms [37.24 ms, 40.33 ms] | 37.63 ms [37.24 ms, 40.33 ms] | 209.0 [206.9, 224.1] | 1.016 [1.010, 1.069] | same |
| explicit_tri3_ss | init | head-t1 | 5 | 1 | 36.83 ms [36.47 ms, 36.93 ms] | 36.83 ms [36.47 ms, 36.93 ms] | 204.6 [202.6, 205.2] | 0.993 [0.979, 0.997] | same |
| explicit_tri3_ss | init | head-t8 | 5 | 1 | 37.30 ms [36.55 ms, 38.59 ms] | 37.30 ms [36.55 ms, 38.59 ms] | 207.2 [203.1, 214.4] | 1.005 [0.985, 1.051] | same |
| explicit_tri3_ss | stable_time_step | base-t1 | 5 | 5 | 22.61 ms [22.54 ms, 23.23 ms] | 4.52 ms [4.51 ms, 4.65 ms] | 25.1 [25.0, 25.8] | – | same |
| explicit_tri3_ss | stable_time_step | base-t8 | 5 | 5 | 23.12 ms [22.56 ms, 23.46 ms] | 4.62 ms [4.51 ms, 4.69 ms] | 25.7 [25.1, 26.1] | 1.022 [0.996, 1.041] | same |
| explicit_tri3_ss | stable_time_step | head-t1 | 5 | 5 | 22.66 ms [22.50 ms, 23.11 ms] | 4.53 ms [4.50 ms, 4.62 ms] | 25.2 [25.0, 25.7] | 1.003 [0.969, 1.022] | same |
| explicit_tri3_ss | stable_time_step | head-t8 | 5 | 5 | 23.12 ms [22.58 ms, 23.51 ms] | 4.62 ms [4.52 ms, 4.70 ms] | 25.7 [25.1, 26.1] | 1.023 [0.972, 1.039] | same |
| explicit_tri3_ss | solve_step | base-t1 | 5 | 300 | 1.508 s [1.498 s, 1.910 s] | 5.03 ms [4.99 ms, 6.37 ms] | 27.9 [27.7, 35.4] | – | same |
| explicit_tri3_ss | solve_step | base-t8 | 5 | 300 | 741.80 ms [737.15 ms, 746.39 ms] | 2.47 ms [2.46 ms, 2.49 ms] | 13.7 [13.7, 13.8] | 0.492 [0.388, 0.495] | same |
| explicit_tri3_ss | solve_step | head-t1 | 5 | 300 | 1.554 s [1.533 s, 1.565 s] | 5.18 ms [5.11 ms, 5.22 ms] | 28.8 [28.4, 29.0] | 1.030 [0.810, 1.038] | same |
| explicit_tri3_ss | solve_step | head-t8 | 5 | 300 | 410.79 ms [400.24 ms, 415.31 ms] | 1.37 ms [1.33 ms, 1.38 ms] | 7.6 [7.4, 7.7] | 0.269 [0.215, 0.276] | same |
| explicit_tri3_ss | predictor | base-t1 | 5 | 300 | 20.4 µs [16.9 µs, 27.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | base-t8 | 5 | 300 | 28.8 µs [20.9 µs, 31.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.517 [0.850, 1.703] | same |
| explicit_tri3_ss | predictor | head-t1 | 5 | 300 | 22.2 µs [14.4 µs, 28.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.086 [0.584, 1.269] | same |
| explicit_tri3_ss | predictor | head-t8 | 5 | 300 | 10.7 µs [8.8 µs, 11.6 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.524 [0.359, 0.689] | same |
| explicit_tri3_ss | residual | base-t1 | 5 | 300 | 1.248 s [1.239 s, 1.647 s] | 4.16 ms [4.13 ms, 5.49 ms] | 23.1 [22.9, 30.5] | – | same |
| explicit_tri3_ss | residual | base-t8 | 5 | 300 | 477.45 ms [475.72 ms, 479.37 ms] | 1.59 ms [1.59 ms, 1.60 ms] | 8.8 [8.8, 8.9] | 0.384 [0.290, 0.385] | same |
| explicit_tri3_ss | residual | head-t1 | 5 | 300 | 1.263 s [1.244 s, 1.271 s] | 4.21 ms [4.15 ms, 4.24 ms] | 23.4 [23.0, 23.5] | 1.012 [0.763, 1.020] | same |
| explicit_tri3_ss | residual | head-t8 | 5 | 300 | 303.17 ms [297.38 ms, 307.05 ms] | 1.01 ms [991.3 µs, 1.02 ms] | 5.6 [5.5, 5.7] | 0.241 [0.185, 0.246] | same |
| explicit_tri3_ss | lumped_matrix | base-t1 | 5 | 300 | 56.2 µs [49.8 µs, 64.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | base-t8 | 5 | 300 | 58.9 µs [50.6 µs, 74.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.962 [0.879, 1.460] | same |
| explicit_tri3_ss | lumped_matrix | head-t1 | 5 | 300 | 68.3 µs [58.5 µs, 72.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.187 [1.041, 1.418] | same |
| explicit_tri3_ss | lumped_matrix | head-t8 | 5 | 300 | 53.7 µs [46.8 µs, 57.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.956 [0.720, 1.128] | same |
| explicit_tri3_ss | corrector | base-t1 | 5 | 300 | 23.9 µs [17.7 µs, 31.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | base-t8 | 5 | 300 | 33.9 µs [21.2 µs, 38.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.329 [0.868, 1.917] | same |
| explicit_tri3_ss | corrector | head-t1 | 5 | 300 | 32.9 µs [26.8 µs, 45.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.453 [0.847, 1.904] | same |
| explicit_tri3_ss | corrector | head-t8 | 5 | 300 | 12.5 µs [11.2 µs, 42.8 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.475 [0.407, 1.789] | same |
| explicit_tri3_ss | step_hooks | base-t1 | 5 | 600 | 777.2 µs [765.6 µs, 800.5 µs] | 1.3 µs [1.3 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | base-t8 | 5 | 600 | 800.5 µs [762.8 µs, 836.9 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.030 [0.953, 1.045] | same |
| explicit_tri3_ss | step_hooks | head-t1 | 5 | 600 | 814.0 µs [774.2 µs, 907.0 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.017 [0.996, 1.185] | same |
| explicit_tri3_ss | step_hooks | head-t8 | 5 | 600 | 711.5 µs [627.8 µs, 774.9 µs] | 1.2 µs [1.0 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.921 [0.785, 0.997] | same |
| explicit_tri3_ss | step_rest | base-t1 | 5 | 300 | 259.83 ms [258.45 ms, 262.44 ms] | 866.1 µs [861.5 µs, 874.8 µs] | 4.8 [4.8, 4.9] | – | same |
| explicit_tri3_ss | step_rest | base-t8 | 5 | 300 | 263.08 ms [259.67 ms, 265.99 ms] | 876.9 µs [865.6 µs, 886.6 µs] | 4.9 [4.8, 4.9] | 1.007 [1.002, 1.024] | same |
| explicit_tri3_ss | step_rest | head-t1 | 5 | 300 | 290.24 ms [287.42 ms, 293.30 ms] | 967.5 µs [958.1 µs, 977.7 µs] | 5.4 [5.3, 5.4] | 1.117 [1.106, 1.133] | same |
| explicit_tri3_ss | step_rest | head-t8 | 5 | 300 | 104.93 ms [102.08 ms, 111.23 ms] | 349.8 µs [340.3 µs, 370.8 µs] | 1.9 [1.9, 2.1] | 0.401 [0.395, 0.427] | same |
| explicit_tri3_ss | internal_forces | base-t1 | 5 | 60 | 210.22 ms [209.90 ms, 213.88 ms] | 3.50 ms [3.50 ms, 3.56 ms] | 19.5 [19.4, 19.8] | – | same |
| explicit_tri3_ss | internal_forces | base-t8 | 5 | 60 | 49.00 ms [47.90 ms, 49.60 ms] | 816.7 µs [798.4 µs, 826.7 µs] | 4.5 [4.4, 4.6] | 0.233 [0.228, 0.236] | same |
| explicit_tri3_ss | internal_forces | head-t1 | 5 | 60 | 211.54 ms [208.42 ms, 292.50 ms] | 3.53 ms [3.47 ms, 4.87 ms] | 19.6 [19.3, 27.1] | 1.007 [0.984, 1.391] | same |
| explicit_tri3_ss | internal_forces | head-t8 | 5 | 60 | 31.97 ms [30.91 ms, 32.19 ms] | 532.9 µs [515.2 µs, 536.5 µs] | 3.0 [2.9, 3.0] | 0.152 [0.146, 0.153] | same |
| explicit_tri3_ss | stresses | base-t1 | 5 | 60 | 74.36 ms [73.25 ms, 74.87 ms] | 1.24 ms [1.22 ms, 1.25 ms] | 6.9 [6.8, 6.9] | – | same |
| explicit_tri3_ss | stresses | base-t8 | 5 | 60 | 32.98 ms [32.23 ms, 34.29 ms] | 549.7 µs [537.1 µs, 571.5 µs] | 3.1 [3.0, 3.2] | 0.444 [0.433, 0.458] | same |
| explicit_tri3_ss | stresses | head-t1 | 5 | 60 | 74.18 ms [72.74 ms, 74.96 ms] | 1.24 ms [1.21 ms, 1.25 ms] | 6.9 [6.7, 6.9] | 0.991 [0.986, 1.018] | same |
| explicit_tri3_ss | stresses | head-t8 | 5 | 60 | 29.94 ms [29.80 ms, 30.90 ms] | 499.0 µs [496.6 µs, 514.9 µs] | 2.8 [2.8, 2.9] | 0.405 [0.398, 0.422] | same |
| explicit_tri3_ss | energies | base-t1 | 5 | 60 | 137.94 ms [137.11 ms, 138.21 ms] | 2.30 ms [2.29 ms, 2.30 ms] | 12.8 [12.7, 12.8] | – | same |
| explicit_tri3_ss | energies | base-t8 | 5 | 60 | 137.90 ms [137.54 ms, 138.94 ms] | 2.30 ms [2.29 ms, 2.32 ms] | 12.8 [12.7, 12.9] | 1.002 [0.998, 1.011] | same |
| explicit_tri3_ss | energies | head-t1 | 5 | 60 | 139.89 ms [139.20 ms, 141.31 ms] | 2.33 ms [2.32 ms, 2.36 ms] | 13.0 [12.9, 13.1] | 1.017 [1.009, 1.029] | same |
| explicit_tri3_ss | energies | head-t8 | 5 | 60 | 137.93 ms [136.28 ms, 138.49 ms] | 2.30 ms [2.27 ms, 2.31 ms] | 12.8 [12.6, 12.8] | 1.000 [0.988, 1.010] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | 7a6ad8f42, 85d4d74d4 | 78b072c2f464d8f9 |
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | 7a6ad8f42, 85d4d74d4 | 37ce0eeb914dd994 |
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 7a6ad8f42, 85d4d74d4 | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 36137566d8b810bb |
| explicit_tet10_fd | 105456 | 148877 | 446631 | 1 | 7a6ad8f42, 85d4d74d4 | c9f013e1a9bbd7a1 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | 7a6ad8f42, 85d4d74d4 | a12864e94a9122c8 |
| explicit_tet4_fd | 162000 | 29791 | 89373 | 1 | 7a6ad8f42, 85d4d74d4 | 9d6cdbdad424d346 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42, 85d4d74d4 | fecb3fb4b46c8b8c |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | 7a6ad8f42, 85d4d74d4 | 27979d9fbcdea83d |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 7a6ad8f42, 85d4d74d4 | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7a6ad8f42, 85d4d74d4 | b6b902f5ca71049e |
