Suite `solid`, size `m`; variants: `before`, `final`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_bilinear | mesh | before | 5 | 1 | 990.3 µs [985.8 µs, 1.00 ms] | 990.3 µs [985.8 µs, 1.00 ms] | 20.6 [20.5, 20.9] | – | same |
| cohesive_intrinsic_bilinear | mesh | final | 5 | 1 | 989.8 µs [981.2 µs, 1.00 ms] | 989.8 µs [981.2 µs, 1.00 ms] | 20.6 [20.4, 20.9] | 1.000 [0.979, 1.012] | same |
| cohesive_intrinsic_bilinear | init | before | 5 | 1 | 683.93 ms [683.51 ms, 691.92 ms] | 683.93 ms [683.51 ms, 691.92 ms] | 14248.6 [14239.8, 14414.9] | – | same |
| cohesive_intrinsic_bilinear | init | final | 5 | 1 | 696.38 ms [694.75 ms, 699.72 ms] | 696.38 ms [694.75 ms, 699.72 ms] | 14507.9 [14473.9, 14577.5] | 1.016 [1.010, 1.018] | same |
| cohesive_intrinsic_bilinear | stable_time_step | before | 5 | 5 | 9.64 ms [9.59 ms, 9.73 ms] | 1.93 ms [1.92 ms, 1.95 ms] | 40.2 [39.9, 40.5] | – | same |
| cohesive_intrinsic_bilinear | stable_time_step | final | 5 | 5 | 9.70 ms [9.63 ms, 9.77 ms] | 1.94 ms [1.93 ms, 1.95 ms] | 40.4 [40.1, 40.7] | 1.002 [0.997, 1.019] | same |
| cohesive_intrinsic_bilinear | solve_step | before | 5 | 300 | 3.040 s [3.036 s, 3.074 s] | 10.13 ms [10.12 ms, 10.25 ms] | 211.1 [210.8, 213.5] | – | same |
| cohesive_intrinsic_bilinear | solve_step | final | 5 | 300 | 2.926 s [2.916 s, 2.946 s] | 9.75 ms [9.72 ms, 9.82 ms] | 203.2 [202.5, 204.6] | 0.960 [0.959, 0.963] | same |
| cohesive_intrinsic_bilinear | energies | before | 5 | 60 | 144.84 ms [144.50 ms, 148.38 ms] | 2.41 ms [2.41 ms, 2.47 ms] | 50.3 [50.2, 51.5] | – | same |
| cohesive_intrinsic_bilinear | energies | final | 5 | 60 | 142.83 ms [142.40 ms, 143.12 ms] | 2.38 ms [2.37 ms, 2.39 ms] | 49.6 [49.4, 49.7] | 0.986 [0.960, 0.989] | same |
| cohesive_intrinsic_exponential | mesh | before | 5 | 1 | 1.00 ms [990.7 µs, 1.03 ms] | 1.00 ms [990.7 µs, 1.03 ms] | 20.9 [20.6, 21.4] | – | same |
| cohesive_intrinsic_exponential | mesh | final | 5 | 1 | 998.4 µs [968.0 µs, 1.01 ms] | 998.4 µs [968.0 µs, 1.01 ms] | 20.8 [20.2, 21.1] | 0.973 [0.968, 1.013] | same |
| cohesive_intrinsic_exponential | init | before | 5 | 1 | 685.29 ms [678.61 ms, 688.25 ms] | 685.29 ms [678.61 ms, 688.25 ms] | 14276.9 [14137.8, 14338.5] | – | same |
| cohesive_intrinsic_exponential | init | final | 5 | 1 | 701.15 ms [696.42 ms, 703.88 ms] | 701.15 ms [696.42 ms, 703.88 ms] | 14607.3 [14508.8, 14664.1] | 1.023 [1.018, 1.030] | same |
| cohesive_intrinsic_exponential | stable_time_step | before | 5 | 5 | 9.94 ms [9.66 ms, 9.96 ms] | 1.99 ms [1.93 ms, 1.99 ms] | 41.4 [40.2, 41.5] | – | same |
| cohesive_intrinsic_exponential | stable_time_step | final | 5 | 5 | 9.71 ms [9.64 ms, 9.89 ms] | 1.94 ms [1.93 ms, 1.98 ms] | 40.4 [40.1, 41.2] | 0.982 [0.977, 0.998] | same |
| cohesive_intrinsic_exponential | solve_step | before | 5 | 300 | 4.614 s [4.564 s, 4.638 s] | 15.38 ms [15.21 ms, 15.46 ms] | 320.4 [317.0, 322.1] | – | same |
| cohesive_intrinsic_exponential | solve_step | final | 5 | 300 | 4.700 s [4.691 s, 4.718 s] | 15.67 ms [15.64 ms, 15.73 ms] | 326.4 [325.8, 327.7] | 1.020 [1.013, 1.028] | same |
| cohesive_intrinsic_exponential | energies | before | 5 | 60 | 145.60 ms [145.24 ms, 146.00 ms] | 2.43 ms [2.42 ms, 2.43 ms] | 50.6 [50.4, 50.7] | – | same |
| cohesive_intrinsic_exponential | energies | final | 5 | 60 | 143.72 ms [143.27 ms, 143.85 ms] | 2.40 ms [2.39 ms, 2.40 ms] | 49.9 [49.7, 49.9] | 0.986 [0.984, 0.988] | same |
| cohesive_intrinsic_implicit | mesh | before | 5 | 1 | 236.0 µs [228.3 µs, 245.9 µs] | 236.0 µs [228.3 µs, 245.9 µs] | 65.6 [63.4, 68.3] | – | same |
| cohesive_intrinsic_implicit | mesh | final | 5 | 1 | 229.7 µs [225.9 µs, 239.8 µs] | 229.7 µs [225.9 µs, 239.8 µs] | 63.8 [62.8, 66.6] | 0.972 [0.952, 1.050] | same |
| cohesive_intrinsic_implicit | init | before | 5 | 1 | 6.82 ms [6.78 ms, 6.83 ms] | 6.82 ms [6.78 ms, 6.83 ms] | 1895.5 [1882.1, 1897.3] | – | same |
| cohesive_intrinsic_implicit | init | final | 5 | 1 | 6.77 ms [6.74 ms, 6.84 ms] | 6.77 ms [6.74 ms, 6.84 ms] | 1880.3 [1872.2, 1899.2] | 0.997 [0.987, 1.005] | same |
| cohesive_intrinsic_implicit | solve_step | before | 5 | 30 | 6.295 s [6.248 s, 6.334 s] | 209.84 ms [208.28 ms, 211.15 ms] | 58288.2 [57855.5, 58652.4] | – | same |
| cohesive_intrinsic_implicit | solve_step | final | 5 | 30 | 6.211 s [6.206 s, 6.276 s] | 207.04 ms [206.88 ms, 209.19 ms] | 57510.0 [57466.7, 58108.9] | 0.989 [0.985, 0.997] | same |
| explicit_marigo | mesh | before | 5 | 1 | 2.10 ms [2.05 ms, 2.14 ms] | 2.10 ms [2.05 ms, 2.14 ms] | 19.9 [19.5, 20.3] | – | same |
| explicit_marigo | mesh | final | 5 | 1 | 2.08 ms [2.02 ms, 2.12 ms] | 2.08 ms [2.02 ms, 2.12 ms] | 19.8 [19.2, 20.1] | 0.981 [0.947, 1.025] | same |
| explicit_marigo | init | before | 5 | 1 | 34.35 ms [34.10 ms, 35.07 ms] | 34.35 ms [34.10 ms, 35.07 ms] | 325.7 [323.4, 332.5] | – | same |
| explicit_marigo | init | final | 5 | 1 | 34.80 ms [34.29 ms, 34.95 ms] | 34.80 ms [34.29 ms, 34.95 ms] | 329.9 [325.1, 331.4] | 1.012 [0.978, 1.020] | same |
| explicit_marigo | stable_time_step | before | 5 | 5 | 21.18 ms [21.09 ms, 21.37 ms] | 4.24 ms [4.22 ms, 4.27 ms] | 40.2 [40.0, 40.5] | – | same |
| explicit_marigo | stable_time_step | final | 5 | 5 | 21.12 ms [21.05 ms, 21.23 ms] | 4.22 ms [4.21 ms, 4.25 ms] | 40.1 [39.9, 40.3] | 0.998 [0.993, 1.000] | same |
| explicit_marigo | solve_step | before | 5 | 200 | 2.178 s [2.162 s, 2.182 s] | 10.89 ms [10.81 ms, 10.91 ms] | 103.3 [102.5, 103.5] | – | same |
| explicit_marigo | solve_step | final | 5 | 200 | 2.123 s [2.108 s, 2.130 s] | 10.61 ms [10.54 ms, 10.65 ms] | 100.6 [100.0, 101.0] | 0.975 [0.974, 0.978] | same |
| explicit_marigo | predictor | before | 5 | 200 | 40.9 µs [34.5 µs, 54.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | predictor | final | 5 | 200 | 31.7 µs [31.4 µs, 50.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.787 [0.580, 1.242] | same |
| explicit_marigo | residual | before | 5 | 200 | 1.593 s [1.585 s, 1.596 s] | 7.97 ms [7.93 ms, 7.98 ms] | 75.5 [75.2, 75.7] | – | same |
| explicit_marigo | residual | final | 5 | 200 | 1.561 s [1.552 s, 1.563 s] | 7.80 ms [7.76 ms, 7.82 ms] | 74.0 [73.6, 74.1] | 0.980 [0.979, 0.982] | same |
| explicit_marigo | lumped_matrix | before | 5 | 200 | 89.3 µs [80.3 µs, 104.3 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | lumped_matrix | final | 5 | 200 | 105.8 µs [83.0 µs, 123.7 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.098 [1.034, 1.523] | same |
| explicit_marigo | corrector | before | 5 | 200 | 56.2 µs [54.9 µs, 57.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | corrector | final | 5 | 200 | 34.6 µs [33.2 µs, 58.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.619 [0.605, 1.016] | same |
| explicit_marigo | step_hooks | before | 5 | 400 | 502.48 ms [496.72 ms, 503.44 ms] | 1.26 ms [1.24 ms, 1.26 ms] | 11.9 [11.8, 11.9] | – | same |
| explicit_marigo | step_hooks | final | 5 | 400 | 480.80 ms [472.40 ms, 482.37 ms] | 1.20 ms [1.18 ms, 1.21 ms] | 11.4 [11.2, 11.4] | 0.957 [0.951, 0.958] | same |
| explicit_marigo | step_rest | before | 5 | 200 | 80.94 ms [78.27 ms, 81.75 ms] | 404.7 µs [391.3 µs, 408.7 µs] | 3.8 [3.7, 3.9] | – | same |
| explicit_marigo | step_rest | final | 5 | 200 | 82.87 ms [80.61 ms, 85.28 ms] | 414.3 µs [403.1 µs, 426.4 µs] | 3.9 [3.8, 4.0] | 1.036 [0.994, 1.059] | same |
| explicit_marigo | internal_forces | before | 5 | 40 | 326.55 ms [323.63 ms, 327.68 ms] | 8.16 ms [8.09 ms, 8.19 ms] | 77.4 [76.7, 77.7] | – | same |
| explicit_marigo | internal_forces | final | 5 | 40 | 320.24 ms [319.30 ms, 321.44 ms] | 8.01 ms [7.98 ms, 8.04 ms] | 75.9 [75.7, 76.2] | 0.981 [0.977, 0.993] | same |
| explicit_marigo | stresses | before | 5 | 40 | 193.90 ms [191.36 ms, 194.76 ms] | 4.85 ms [4.78 ms, 4.87 ms] | 46.0 [45.4, 46.2] | – | same |
| explicit_marigo | stresses | final | 5 | 40 | 187.20 ms [185.58 ms, 188.91 ms] | 4.68 ms [4.64 ms, 4.72 ms] | 44.4 [44.0, 44.8] | 0.967 [0.963, 0.975] | same |
| explicit_marigo | energies | before | 5 | 40 | 66.16 ms [65.83 ms, 66.39 ms] | 1.65 ms [1.65 ms, 1.66 ms] | 15.7 [15.6, 15.7] | – | same |
| explicit_marigo | energies | final | 5 | 40 | 68.28 ms [67.96 ms, 68.45 ms] | 1.71 ms [1.70 ms, 1.71 ms] | 16.2 [16.1, 16.2] | 1.033 [1.027, 1.033] | same |
| explicit_maxwell | mesh | before | 5 | 1 | 2.13 ms [2.07 ms, 2.13 ms] | 2.13 ms [2.07 ms, 2.13 ms] | 20.2 [19.6, 20.2] | – | same |
| explicit_maxwell | mesh | final | 5 | 1 | 2.07 ms [2.07 ms, 2.11 ms] | 2.07 ms [2.07 ms, 2.11 ms] | 19.7 [19.6, 20.0] | 0.985 [0.973, 0.998] | same |
| explicit_maxwell | init | before | 5 | 1 | 39.16 ms [39.04 ms, 40.23 ms] | 39.16 ms [39.04 ms, 40.23 ms] | 371.4 [370.2, 381.5] | – | same |
| explicit_maxwell | init | final | 5 | 1 | 38.80 ms [38.26 ms, 40.56 ms] | 38.80 ms [38.26 ms, 40.56 ms] | 368.0 [362.8, 384.7] | 0.977 [0.964, 1.039] | same |
| explicit_maxwell | stable_time_step | before | 5 | 5 | 21.10 ms [21.09 ms, 21.15 ms] | 4.22 ms [4.22 ms, 4.23 ms] | 40.0 [40.0, 40.1] | – | same |
| explicit_maxwell | stable_time_step | final | 5 | 5 | 21.11 ms [21.08 ms, 21.30 ms] | 4.22 ms [4.22 ms, 4.26 ms] | 40.0 [40.0, 40.4] | 1.001 [1.000, 1.007] | same |
| explicit_maxwell | solve_step | before | 5 | 40 | 3.194 s [3.180 s, 3.224 s] | 79.86 ms [79.50 ms, 80.59 ms] | 757.3 [753.9, 764.2] | – | same |
| explicit_maxwell | solve_step | final | 5 | 40 | 1.500 s [1.478 s, 1.513 s] | 37.49 ms [36.94 ms, 37.81 ms] | 355.5 [350.3, 358.6] | 0.470 [0.465, 0.471] | same |
| explicit_maxwell | predictor | before | 5 | 40 | 8.3 µs [6.8 µs, 10.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | predictor | final | 5 | 40 | 10.1 µs [6.8 µs, 10.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.170 [0.681, 1.480] | same |
| explicit_maxwell | residual | before | 5 | 40 | 888.89 ms [885.20 ms, 897.64 ms] | 22.22 ms [22.13 ms, 22.44 ms] | 210.7 [209.9, 212.8] | – | same |
| explicit_maxwell | residual | final | 5 | 40 | 426.52 ms [423.28 ms, 432.08 ms] | 10.66 ms [10.58 ms, 10.80 ms] | 101.1 [100.3, 102.4] | 0.479 [0.472, 0.486] | same |
| explicit_maxwell | lumped_matrix | before | 5 | 40 | 19.8 µs [18.2 µs, 21.5 µs] | 0.5 µs [0.5 µs, 0.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | lumped_matrix | final | 5 | 40 | 20.9 µs [16.9 µs, 22.9 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.103 [0.790, 1.155] | same |
| explicit_maxwell | corrector | before | 5 | 40 | 7.2 µs [6.5 µs, 8.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | corrector | final | 5 | 40 | 10.4 µs [7.0 µs, 11.7 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.392 [0.954, 1.779] | same |
| explicit_maxwell | step_hooks | before | 5 | 80 | 2.293 s [2.266 s, 2.315 s] | 28.66 ms [28.33 ms, 28.94 ms] | 271.8 [268.6, 274.4] | – | same |
| explicit_maxwell | step_hooks | final | 5 | 80 | 1.057 s [1.038 s, 1.064 s] | 13.21 ms [12.98 ms, 13.30 ms] | 125.3 [123.1, 126.1] | 0.461 [0.457, 0.464] | same |
| explicit_maxwell | step_rest | before | 5 | 40 | 16.08 ms [16.02 ms, 16.25 ms] | 402.1 µs [400.5 µs, 406.3 µs] | 3.8 [3.8, 3.9] | – | same |
| explicit_maxwell | step_rest | final | 5 | 40 | 16.05 ms [15.81 ms, 16.37 ms] | 401.3 µs [395.3 µs, 409.2 µs] | 3.8 [3.7, 3.9] | 0.993 [0.980, 1.018] | same |
| explicit_maxwell | internal_forces | before | 5 | 8 | 167.01 ms [165.32 ms, 168.79 ms] | 20.88 ms [20.67 ms, 21.10 ms] | 198.0 [196.0, 200.1] | – | same |
| explicit_maxwell | internal_forces | final | 5 | 8 | 74.39 ms [73.37 ms, 75.20 ms] | 9.30 ms [9.17 ms, 9.40 ms] | 88.2 [87.0, 89.1] | 0.444 [0.441, 0.447] | same |
| explicit_maxwell | stresses | before | 5 | 8 | 142.22 ms [139.94 ms, 144.04 ms] | 17.78 ms [17.49 ms, 18.00 ms] | 168.6 [165.9, 170.7] | – | same |
| explicit_maxwell | stresses | final | 5 | 8 | 48.34 ms [47.79 ms, 48.55 ms] | 6.04 ms [5.97 ms, 6.07 ms] | 57.3 [56.6, 57.5] | 0.338 [0.336, 0.341] | same |
| explicit_maxwell | energies | before | 5 | 8 | 20.39 ms [20.27 ms, 20.45 ms] | 2.55 ms [2.53 ms, 2.56 ms] | 24.2 [24.0, 24.2] | – | same |
| explicit_maxwell | energies | final | 5 | 8 | 20.28 ms [20.16 ms, 20.40 ms] | 2.53 ms [2.52 ms, 2.55 ms] | 24.0 [23.9, 24.2] | 0.995 [0.992, 1.000] | same |
| explicit_mazars | mesh | before | 5 | 1 | 2.12 ms [2.08 ms, 2.20 ms] | 2.12 ms [2.08 ms, 2.20 ms] | 20.1 [19.8, 20.9] | – | same |
| explicit_mazars | mesh | final | 5 | 1 | 2.06 ms [2.05 ms, 2.15 ms] | 2.06 ms [2.05 ms, 2.15 ms] | 19.6 [19.5, 20.4] | 0.975 [0.932, 1.014] | same |
| explicit_mazars | init | before | 5 | 1 | 34.75 ms [34.54 ms, 35.02 ms] | 34.75 ms [34.54 ms, 35.02 ms] | 329.5 [327.6, 332.1] | – | same |
| explicit_mazars | init | final | 5 | 1 | 34.78 ms [34.51 ms, 35.16 ms] | 34.78 ms [34.51 ms, 35.16 ms] | 329.8 [327.2, 333.4] | 1.000 [0.985, 1.017] | same |
| explicit_mazars | stable_time_step | before | 5 | 5 | 21.19 ms [21.12 ms, 21.49 ms] | 4.24 ms [4.22 ms, 4.30 ms] | 40.2 [40.1, 40.8] | – | same |
| explicit_mazars | stable_time_step | final | 5 | 5 | 21.23 ms [21.11 ms, 21.73 ms] | 4.25 ms [4.22 ms, 4.35 ms] | 40.3 [40.0, 41.2] | 1.002 [0.982, 1.026] | same |
| explicit_mazars | solve_step | before | 5 | 80 | 5.070 s [4.811 s, 5.336 s] | 63.38 ms [60.14 ms, 66.70 ms] | 601.0 [570.3, 632.5] | – | same |
| explicit_mazars | solve_step | final | 5 | 80 | 2.881 s [2.879 s, 2.893 s] | 36.01 ms [35.98 ms, 36.16 ms] | 341.5 [341.2, 342.9] | 0.568 [0.540, 0.601] | same |
| explicit_mazars | predictor | before | 5 | 80 | 20.1 µs [16.5 µs, 21.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | final | 5 | 80 | 17.3 µs [13.4 µs, 21.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.869 [0.763, 1.051] | same |
| explicit_mazars | residual | before | 5 | 80 | 4.849 s [4.588 s, 5.115 s] | 60.61 ms [57.35 ms, 63.94 ms] | 574.7 [543.8, 606.3] | – | same |
| explicit_mazars | residual | final | 5 | 80 | 2.669 s [2.668 s, 2.677 s] | 33.36 ms [33.35 ms, 33.47 ms] | 316.4 [316.2, 317.3] | 0.550 [0.522, 0.584] | same |
| explicit_mazars | lumped_matrix | before | 5 | 80 | 40.0 µs [28.3 µs, 41.6 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | final | 5 | 80 | 36.7 µs [32.0 µs, 40.2 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 0.936 [0.770, 1.423] | same |
| explicit_mazars | corrector | before | 5 | 80 | 22.1 µs [20.8 µs, 23.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | final | 5 | 80 | 22.3 µs [13.0 µs, 23.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.979 [0.587, 1.132] | same |
| explicit_mazars | step_hooks | before | 5 | 160 | 190.32 ms [187.88 ms, 191.20 ms] | 1.19 ms [1.17 ms, 1.19 ms] | 11.3 [11.1, 11.3] | – | same |
| explicit_mazars | step_hooks | final | 5 | 160 | 180.34 ms [178.77 ms, 183.34 ms] | 1.13 ms [1.12 ms, 1.15 ms] | 10.7 [10.6, 10.9] | 0.951 [0.948, 0.959] | same |
| explicit_mazars | step_rest | before | 5 | 80 | 31.26 ms [30.70 ms, 31.74 ms] | 390.8 µs [383.8 µs, 396.7 µs] | 3.7 [3.6, 3.8] | – | same |
| explicit_mazars | step_rest | final | 5 | 80 | 31.10 ms [30.65 ms, 31.68 ms] | 388.8 µs [383.1 µs, 396.0 µs] | 3.7 [3.6, 3.8] | 0.995 [0.982, 1.018] | same |
| explicit_mazars | internal_forces | before | 5 | 16 | 1.008 s [997.29 ms, 1.048 s] | 62.98 ms [62.33 ms, 65.53 ms] | 597.2 [591.1, 621.4] | – | same |
| explicit_mazars | internal_forces | final | 5 | 16 | 574.99 ms [574.08 ms, 577.17 ms] | 35.94 ms [35.88 ms, 36.07 ms] | 340.8 [340.2, 342.1] | 0.573 [0.548, 0.576] | same |
| explicit_mazars | stresses | before | 5 | 16 | 988.75 ms [955.22 ms, 1.008 s] | 61.80 ms [59.70 ms, 62.98 ms] | 586.0 [566.1, 597.3] | – | same |
| explicit_mazars | stresses | final | 5 | 16 | 525.09 ms [522.87 ms, 525.42 ms] | 32.82 ms [32.68 ms, 32.84 ms] | 311.2 [309.9, 311.4] | 0.529 [0.521, 0.550] | same |
| explicit_mazars | energies | before | 5 | 16 | 17.56 ms [17.43 ms, 18.11 ms] | 1.10 ms [1.09 ms, 1.13 ms] | 10.4 [10.3, 10.7] | – | same |
| explicit_mazars | energies | final | 5 | 16 | 18.38 ms [18.31 ms, 18.89 ms] | 1.15 ms [1.14 ms, 1.18 ms] | 10.9 [10.9, 11.2] | 1.046 [1.011, 1.078] | same |
| explicit_mazars_non_local | mesh | before | 5 | 1 | 1.01 ms [993.5 µs, 1.02 ms] | 1.01 ms [993.5 µs, 1.02 ms] | 21.0 [20.7, 21.2] | – | same |
| explicit_mazars_non_local | mesh | final | 5 | 1 | 985.9 µs [976.6 µs, 996.4 µs] | 985.9 µs [976.6 µs, 996.4 µs] | 20.5 [20.3, 20.8] | 0.976 [0.960, 1.003] | same |
| explicit_mazars_non_local | init | before | 5 | 1 | 616.56 ms [615.16 ms, 617.38 ms] | 616.56 ms [615.16 ms, 617.38 ms] | 12845.1 [12815.8, 12862.0] | – | same |
| explicit_mazars_non_local | init | final | 5 | 1 | 621.13 ms [618.49 ms, 625.09 ms] | 621.13 ms [618.49 ms, 625.09 ms] | 12940.1 [12885.3, 13022.6] | 1.007 [1.005, 1.016] | same |
| explicit_mazars_non_local | stable_time_step | before | 5 | 5 | 9.63 ms [9.56 ms, 9.75 ms] | 1.93 ms [1.91 ms, 1.95 ms] | 40.1 [39.8, 40.6] | – | same |
| explicit_mazars_non_local | stable_time_step | final | 5 | 5 | 9.64 ms [9.57 ms, 9.86 ms] | 1.93 ms [1.91 ms, 1.97 ms] | 40.2 [39.9, 41.1] | 1.001 [0.982, 1.028] | same |
| explicit_mazars_non_local | solve_step | before | 5 | 40 | 5.029 s [4.960 s, 5.125 s] | 125.72 ms [124.00 ms, 128.12 ms] | 2619.1 [2583.3, 2669.1] | – | same |
| explicit_mazars_non_local | solve_step | final | 5 | 40 | 4.079 s [4.052 s, 4.111 s] | 101.99 ms [101.30 ms, 102.77 ms] | 2124.7 [2110.5, 2141.1] | 0.811 [0.791, 0.829] | same |
| explicit_mazars_non_local | predictor | before | 5 | 40 | 6.1 µs [4.7 µs, 7.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | final | 5 | 40 | 5.1 µs [4.4 µs, 7.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.912 [0.685, 1.215] | same |
| explicit_mazars_non_local | residual | before | 5 | 40 | 4.976 s [4.907 s, 5.073 s] | 124.41 ms [122.68 ms, 126.83 ms] | 2591.8 [2555.9, 2642.4] | – | same |
| explicit_mazars_non_local | residual | final | 5 | 40 | 4.030 s [4.003 s, 4.062 s] | 100.76 ms [100.06 ms, 101.55 ms] | 2099.2 [2084.6, 2115.6] | 0.810 [0.789, 0.828] | same |
| explicit_mazars_non_local | lumped_matrix | before | 5 | 40 | 13.9 µs [12.8 µs, 15.4 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | final | 5 | 40 | 12.4 µs [11.9 µs, 16.8 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.911 [0.803, 1.269] | same |
| explicit_mazars_non_local | corrector | before | 5 | 40 | 8.1 µs [6.5 µs, 11.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | final | 5 | 40 | 12.4 µs [6.1 µs, 16.0 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.921 [0.514, 1.973] | same |
| explicit_mazars_non_local | step_hooks | before | 5 | 80 | 44.03 ms [43.16 ms, 44.18 ms] | 550.4 µs [539.5 µs, 552.2 µs] | 11.5 [11.2, 11.5] | – | same |
| explicit_mazars_non_local | step_hooks | final | 5 | 80 | 41.00 ms [40.38 ms, 41.33 ms] | 512.4 µs [504.8 µs, 516.6 µs] | 10.7 [10.5, 10.8] | 0.929 [0.917, 0.956] | same |
| explicit_mazars_non_local | step_rest | before | 5 | 40 | 8.23 ms [8.06 ms, 8.28 ms] | 205.7 µs [201.5 µs, 206.9 µs] | 4.3 [4.2, 4.3] | – | same |
| explicit_mazars_non_local | step_rest | final | 5 | 40 | 8.09 ms [7.92 ms, 8.45 ms] | 202.2 µs [197.9 µs, 211.2 µs] | 4.2 [4.1, 4.4] | 0.983 [0.970, 1.047] | same |
| explicit_mazars_non_local | internal_forces | before | 5 | 8 | 1.022 s [991.99 ms, 1.039 s] | 127.71 ms [124.00 ms, 129.82 ms] | 2660.7 [2583.3, 2704.6] | – | same |
| explicit_mazars_non_local | internal_forces | final | 5 | 8 | 815.68 ms [808.74 ms, 819.80 ms] | 101.96 ms [101.09 ms, 102.48 ms] | 2124.2 [2106.1, 2134.9] | 0.796 [0.788, 0.823] | same |
| explicit_mazars_non_local | stresses | before | 5 | 8 | 178.03 ms [172.23 ms, 178.58 ms] | 22.25 ms [21.53 ms, 22.32 ms] | 463.6 [448.5, 465.1] | – | same |
| explicit_mazars_non_local | stresses | final | 5 | 8 | 78.29 ms [77.97 ms, 78.55 ms] | 9.79 ms [9.75 ms, 9.82 ms] | 203.9 [203.1, 204.6] | 0.440 [0.438, 0.456] | same |
| explicit_mazars_non_local | energies | before | 5 | 8 | 10.65 ms [10.63 ms, 10.74 ms] | 1.33 ms [1.33 ms, 1.34 ms] | 27.7 [27.7, 28.0] | – | same |
| explicit_mazars_non_local | energies | final | 5 | 8 | 10.79 ms [10.74 ms, 10.85 ms] | 1.35 ms [1.34 ms, 1.36 ms] | 28.1 [28.0, 28.3] | 1.012 [1.007, 1.014] | same |
| explicit_mazars_non_local | nonlocal_rebuild | before | 5 | 2 | 1.201 s [1.192 s, 1.216 s] | 600.44 ms [596.11 ms, 607.84 ms] | 12509.1 [12419.0, 12663.3] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | final | 5 | 2 | 1.212 s [1.201 s, 1.214 s] | 606.15 ms [600.29 ms, 606.98 ms] | 12628.2 [12506.0, 12645.3] | 1.000 [0.999, 1.018] | same |
| explicit_multi_material | mesh | before | 5 | 1 | 2.12 ms [2.07 ms, 2.15 ms] | 2.12 ms [2.07 ms, 2.15 ms] | 20.1 [19.6, 20.4] | – | same |
| explicit_multi_material | mesh | final | 5 | 1 | 2.07 ms [2.05 ms, 2.13 ms] | 2.07 ms [2.05 ms, 2.13 ms] | 19.6 [19.4, 20.2] | 0.994 [0.952, 1.007] | same |
| explicit_multi_material | init | before | 5 | 1 | 36.24 ms [35.51 ms, 36.68 ms] | 36.24 ms [35.51 ms, 36.68 ms] | 343.7 [336.7, 347.9] | – | same |
| explicit_multi_material | init | final | 5 | 1 | 36.29 ms [35.69 ms, 36.63 ms] | 36.29 ms [35.69 ms, 36.63 ms] | 344.1 [338.4, 347.4] | 1.005 [0.988, 1.018] | same |
| explicit_multi_material | stable_time_step | before | 5 | 5 | 24.60 ms [23.15 ms, 24.71 ms] | 4.92 ms [4.63 ms, 4.94 ms] | 46.7 [43.9, 46.9] | – | same |
| explicit_multi_material | stable_time_step | final | 5 | 5 | 21.25 ms [21.05 ms, 21.60 ms] | 4.25 ms [4.21 ms, 4.32 ms] | 40.3 [39.9, 41.0] | 0.864 [0.852, 0.933] | same |
| explicit_multi_material | solve_step | before | 5 | 120 | 3.637 s [3.499 s, 3.699 s] | 30.31 ms [29.15 ms, 30.83 ms] | 287.4 [276.5, 292.3] | – | same |
| explicit_multi_material | solve_step | final | 5 | 120 | 2.431 s [2.416 s, 2.444 s] | 20.26 ms [20.14 ms, 20.36 ms] | 192.1 [191.0, 193.1] | 0.669 [0.661, 0.693] | same |
| explicit_multi_material | predictor | before | 5 | 120 | 20.5 µs [20.0 µs, 22.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | predictor | final | 5 | 120 | 21.5 µs [18.9 µs, 36.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.049 [0.933, 1.752] | same |
| explicit_multi_material | residual | before | 5 | 120 | 3.390 s [3.248 s, 3.444 s] | 28.25 ms [27.07 ms, 28.70 ms] | 267.9 [256.7, 272.2] | – | same |
| explicit_multi_material | residual | final | 5 | 120 | 2.186 s [2.172 s, 2.195 s] | 18.22 ms [18.10 ms, 18.29 ms] | 172.8 [171.6, 173.5] | 0.645 [0.637, 0.671] | same |
| explicit_multi_material | lumped_matrix | before | 5 | 120 | 53.0 µs [42.5 µs, 56.2 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | lumped_matrix | final | 5 | 120 | 44.3 µs [39.7 µs, 50.7 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.920 [0.774, 0.942] | same |
| explicit_multi_material | corrector | before | 5 | 120 | 21.8 µs [20.6 µs, 23.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | corrector | final | 5 | 120 | 19.3 µs [19.1 µs, 21.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.920 [0.885, 0.984] | same |
| explicit_multi_material | step_hooks | before | 5 | 240 | 202.45 ms [200.18 ms, 206.26 ms] | 843.5 µs [834.1 µs, 859.4 µs] | 8.0 [7.9, 8.1] | – | same |
| explicit_multi_material | step_hooks | final | 5 | 240 | 197.91 ms [197.00 ms, 200.66 ms] | 824.6 µs [820.8 µs, 836.1 µs] | 7.8 [7.8, 7.9] | 0.978 [0.971, 0.987] | same |
| explicit_multi_material | step_rest | before | 5 | 120 | 47.25 ms [46.45 ms, 48.14 ms] | 393.8 µs [387.1 µs, 401.2 µs] | 3.7 [3.7, 3.8] | – | same |
| explicit_multi_material | step_rest | final | 5 | 120 | 47.09 ms [46.50 ms, 47.47 ms] | 392.4 µs [387.5 µs, 395.6 µs] | 3.7 [3.7, 3.8] | 0.988 [0.984, 1.009] | same |
| explicit_multi_material | internal_forces | before | 5 | 24 | 688.20 ms [661.10 ms, 738.97 ms] | 28.68 ms [27.55 ms, 30.79 ms] | 271.9 [261.2, 292.0] | – | same |
| explicit_multi_material | internal_forces | final | 5 | 24 | 437.75 ms [434.80 ms, 438.35 ms] | 18.24 ms [18.12 ms, 18.26 ms] | 173.0 [171.8, 173.2] | 0.632 [0.592, 0.658] | same |
| explicit_multi_material | stresses | before | 5 | 24 | 572.60 ms [566.88 ms, 604.14 ms] | 23.86 ms [23.62 ms, 25.17 ms] | 226.2 [224.0, 238.7] | – | same |
| explicit_multi_material | stresses | final | 5 | 24 | 340.66 ms [340.05 ms, 341.38 ms] | 14.19 ms [14.17 ms, 14.22 ms] | 134.6 [134.4, 134.9] | 0.596 [0.564, 0.600] | same |
| explicit_multi_material | energies | before | 5 | 24 | 61.77 ms [61.42 ms, 61.88 ms] | 2.57 ms [2.56 ms, 2.58 ms] | 24.4 [24.3, 24.4] | – | same |
| explicit_multi_material | energies | final | 5 | 24 | 60.52 ms [60.17 ms, 60.75 ms] | 2.52 ms [2.51 ms, 2.53 ms] | 23.9 [23.8, 24.0] | 0.982 [0.974, 0.987] | same |
| explicit_neohookean | mesh | before | 5 | 1 | 2.07 ms [2.04 ms, 2.16 ms] | 2.07 ms [2.04 ms, 2.16 ms] | 19.6 [19.3, 20.5] | – | same |
| explicit_neohookean | mesh | final | 5 | 1 | 2.04 ms [2.01 ms, 2.12 ms] | 2.04 ms [2.01 ms, 2.12 ms] | 19.4 [19.0, 20.1] | 0.969 [0.941, 1.039] | same |
| explicit_neohookean | init | before | 5 | 1 | 36.62 ms [35.84 ms, 37.23 ms] | 36.62 ms [35.84 ms, 37.23 ms] | 347.3 [339.9, 353.0] | – | same |
| explicit_neohookean | init | final | 5 | 1 | 36.75 ms [36.06 ms, 37.16 ms] | 36.75 ms [36.06 ms, 37.16 ms] | 348.5 [341.9, 352.4] | 1.003 [0.990, 1.022] | same |
| explicit_neohookean | stable_time_step | before | 5 | 5 | 21.13 ms [21.07 ms, 21.19 ms] | 4.23 ms [4.21 ms, 4.24 ms] | 40.1 [40.0, 40.2] | – | same |
| explicit_neohookean | stable_time_step | final | 5 | 5 | 21.11 ms [21.05 ms, 21.21 ms] | 4.22 ms [4.21 ms, 4.24 ms] | 40.0 [39.9, 40.2] | 0.999 [0.994, 1.006] | same |
| explicit_neohookean | solve_step | before | 5 | 100 | 1.489 s [1.474 s, 1.500 s] | 14.89 ms [14.74 ms, 15.00 ms] | 141.2 [139.8, 142.2] | – | same |
| explicit_neohookean | solve_step | final | 5 | 100 | 1.197 s [1.190 s, 1.200 s] | 11.97 ms [11.90 ms, 12.00 ms] | 113.5 [112.9, 113.7] | 0.800 [0.797, 0.813] | same |
| explicit_neohookean | predictor | before | 5 | 100 | 13.2 µs [10.1 µs, 15.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | predictor | final | 5 | 100 | 12.1 µs [9.7 µs, 13.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.866 [0.645, 1.367] | same |
| explicit_neohookean | residual | before | 5 | 100 | 1.368 s [1.352 s, 1.378 s] | 13.68 ms [13.52 ms, 13.78 ms] | 129.7 [128.2, 130.6] | – | same |
| explicit_neohookean | residual | final | 5 | 100 | 1.073 s [1.070 s, 1.076 s] | 10.73 ms [10.70 ms, 10.76 ms] | 101.8 [101.4, 102.0] | 0.782 [0.778, 0.796] | same |
| explicit_neohookean | lumped_matrix | before | 5 | 100 | 33.8 µs [33.0 µs, 34.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | lumped_matrix | final | 5 | 100 | 27.3 µs [25.6 µs, 28.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.795 [0.761, 0.829] | same |
| explicit_neohookean | corrector | before | 5 | 100 | 14.2 µs [13.2 µs, 15.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | corrector | final | 5 | 100 | 15.2 µs [12.1 µs, 20.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.082 [0.761, 1.495] | same |
| explicit_neohookean | step_hooks | before | 5 | 200 | 87.83 ms [86.91 ms, 89.97 ms] | 439.2 µs [434.6 µs, 449.8 µs] | 4.2 [4.1, 4.3] | – | same |
| explicit_neohookean | step_hooks | final | 5 | 200 | 88.79 ms [86.10 ms, 90.26 ms] | 443.9 µs [430.5 µs, 451.3 µs] | 4.2 [4.1, 4.3] | 1.003 [0.990, 1.011] | same |
| explicit_neohookean | step_rest | before | 5 | 100 | 33.98 ms [33.94 ms, 35.01 ms] | 339.8 µs [339.4 µs, 350.1 µs] | 3.2 [3.2, 3.3] | – | same |
| explicit_neohookean | step_rest | final | 5 | 100 | 34.11 ms [33.94 ms, 34.44 ms] | 341.1 µs [339.4 µs, 344.4 µs] | 3.2 [3.2, 3.3] | 1.003 [0.978, 1.010] | same |
| explicit_neohookean | internal_forces | before | 5 | 20 | 272.52 ms [271.29 ms, 286.90 ms] | 13.63 ms [13.56 ms, 14.34 ms] | 129.2 [128.6, 136.0] | – | same |
| explicit_neohookean | internal_forces | final | 5 | 20 | 210.83 ms [210.45 ms, 212.12 ms] | 10.54 ms [10.52 ms, 10.61 ms] | 100.0 [99.8, 100.6] | 0.774 [0.735, 0.781] | same |
| explicit_neohookean | stresses | before | 5 | 20 | 176.55 ms [172.97 ms, 178.70 ms] | 8.83 ms [8.65 ms, 8.94 ms] | 83.7 [82.0, 84.7] | – | same |
| explicit_neohookean | stresses | final | 5 | 20 | 114.28 ms [113.98 ms, 114.82 ms] | 5.71 ms [5.70 ms, 5.74 ms] | 54.2 [54.0, 54.4] | 0.650 [0.638, 0.663] | same |
| explicit_neohookean | energies | before | 5 | 20 | 176.82 ms [173.33 ms, 183.75 ms] | 8.84 ms [8.67 ms, 9.19 ms] | 83.8 [82.2, 87.1] | – | same |
| explicit_neohookean | energies | final | 5 | 20 | 126.25 ms [126.02 ms, 126.85 ms] | 6.31 ms [6.30 ms, 6.34 ms] | 59.9 [59.7, 60.1] | 0.714 [0.687, 0.727] | same |
| explicit_orthotropic | mesh | before | 5 | 1 | 2.08 ms [2.06 ms, 2.10 ms] | 2.08 ms [2.06 ms, 2.10 ms] | 19.7 [19.5, 19.9] | – | same |
| explicit_orthotropic | mesh | final | 5 | 1 | 2.05 ms [2.03 ms, 2.08 ms] | 2.05 ms [2.03 ms, 2.08 ms] | 19.4 [19.3, 19.8] | 0.989 [0.973, 1.002] | same |
| explicit_orthotropic | init | before | 5 | 1 | 32.66 ms [32.08 ms, 32.98 ms] | 32.66 ms [32.08 ms, 32.98 ms] | 309.7 [304.2, 312.7] | – | same |
| explicit_orthotropic | init | final | 5 | 1 | 32.19 ms [31.78 ms, 33.40 ms] | 32.19 ms [31.78 ms, 33.40 ms] | 305.2 [301.3, 316.7] | 0.991 [0.974, 1.021] | same |
| explicit_orthotropic | stable_time_step | before | 5 | 5 | 21.07 ms [21.06 ms, 21.14 ms] | 4.21 ms [4.21 ms, 4.23 ms] | 40.0 [39.9, 40.1] | – | same |
| explicit_orthotropic | stable_time_step | final | 5 | 5 | 21.14 ms [21.05 ms, 21.20 ms] | 4.23 ms [4.21 ms, 4.24 ms] | 40.1 [39.9, 40.2] | 1.002 [1.000, 1.005] | same |
| explicit_orthotropic | solve_step | before | 5 | 200 | 1.742 s [1.726 s, 1.754 s] | 8.71 ms [8.63 ms, 8.77 ms] | 82.6 [81.8, 83.1] | – | same |
| explicit_orthotropic | solve_step | final | 5 | 200 | 1.373 s [1.364 s, 1.391 s] | 6.87 ms [6.82 ms, 6.95 ms] | 65.1 [64.7, 65.9] | 0.790 [0.782, 0.797] | same |
| explicit_orthotropic | predictor | before | 5 | 200 | 23.7 µs [21.7 µs, 27.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | predictor | final | 5 | 200 | 21.4 µs [16.8 µs, 24.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.936 [0.608, 0.993] | same |
| explicit_orthotropic | residual | before | 5 | 200 | 1.565 s [1.550 s, 1.574 s] | 7.82 ms [7.75 ms, 7.87 ms] | 74.2 [73.5, 74.6] | – | same |
| explicit_orthotropic | residual | final | 5 | 200 | 1.195 s [1.189 s, 1.208 s] | 5.98 ms [5.95 ms, 6.04 ms] | 56.7 [56.4, 57.3] | 0.767 [0.756, 0.771] | same |
| explicit_orthotropic | lumped_matrix | before | 5 | 200 | 62.8 µs [61.8 µs, 65.4 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | lumped_matrix | final | 5 | 200 | 67.1 µs [62.9 µs, 68.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.026 [1.000, 1.115] | same |
| explicit_orthotropic | corrector | before | 5 | 200 | 27.1 µs [25.3 µs, 28.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | corrector | final | 5 | 200 | 26.3 µs [17.6 µs, 37.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.975 [0.674, 1.301] | same |
| explicit_orthotropic | step_hooks | before | 5 | 400 | 110.18 ms [107.49 ms, 110.51 ms] | 275.5 µs [268.7 µs, 276.3 µs] | 2.6 [2.5, 2.6] | – | same |
| explicit_orthotropic | step_hooks | final | 5 | 400 | 109.41 ms [107.47 ms, 111.47 ms] | 273.5 µs [268.7 µs, 278.7 µs] | 2.6 [2.5, 2.6] | 1.000 [0.993, 1.009] | same |
| explicit_orthotropic | step_rest | before | 5 | 200 | 68.14 ms [67.41 ms, 68.78 ms] | 340.7 µs [337.1 µs, 343.9 µs] | 3.2 [3.2, 3.3] | – | same |
| explicit_orthotropic | step_rest | final | 5 | 200 | 70.00 ms [67.35 ms, 70.86 ms] | 350.0 µs [336.7 µs, 354.3 µs] | 3.3 [3.2, 3.4] | 1.018 [0.988, 1.050] | same |
| explicit_orthotropic | internal_forces | before | 5 | 40 | 303.81 ms [302.37 ms, 305.23 ms] | 7.60 ms [7.56 ms, 7.63 ms] | 72.0 [71.7, 72.4] | – | same |
| explicit_orthotropic | internal_forces | final | 5 | 40 | 230.48 ms [230.36 ms, 233.11 ms] | 5.76 ms [5.76 ms, 5.83 ms] | 54.6 [54.6, 55.3] | 0.762 [0.757, 0.767] | same |
| explicit_orthotropic | stresses | before | 5 | 40 | 189.25 ms [187.87 ms, 190.95 ms] | 4.73 ms [4.70 ms, 4.77 ms] | 44.9 [44.5, 45.3] | – | same |
| explicit_orthotropic | stresses | final | 5 | 40 | 112.54 ms [111.41 ms, 113.13 ms] | 2.81 ms [2.79 ms, 2.83 ms] | 26.7 [26.4, 26.8] | 0.593 [0.590, 0.596] | same |
| explicit_orthotropic | energies | before | 5 | 40 | 123.32 ms [122.76 ms, 124.32 ms] | 3.08 ms [3.07 ms, 3.11 ms] | 29.2 [29.1, 29.5] | – | same |
| explicit_orthotropic | energies | final | 5 | 40 | 124.93 ms [123.70 ms, 125.43 ms] | 3.12 ms [3.09 ms, 3.14 ms] | 29.6 [29.3, 29.7] | 1.008 [1.006, 1.017] | same |
| explicit_plastic | mesh | before | 5 | 1 | 2.08 ms [2.05 ms, 2.13 ms] | 2.08 ms [2.05 ms, 2.13 ms] | 19.7 [19.5, 20.2] | – | same |
| explicit_plastic | mesh | final | 5 | 1 | 2.08 ms [2.05 ms, 2.09 ms] | 2.08 ms [2.05 ms, 2.09 ms] | 19.7 [19.5, 19.8] | 0.999 [0.971, 1.017] | same |
| explicit_plastic | init | before | 5 | 1 | 36.50 ms [35.20 ms, 37.41 ms] | 36.50 ms [35.20 ms, 37.41 ms] | 346.1 [333.8, 354.7] | – | same |
| explicit_plastic | init | final | 5 | 1 | 36.49 ms [36.08 ms, 37.36 ms] | 36.49 ms [36.08 ms, 37.36 ms] | 346.0 [342.1, 354.3] | 1.005 [0.986, 1.031] | same |
| explicit_plastic | stable_time_step | before | 5 | 5 | 21.08 ms [21.07 ms, 21.20 ms] | 4.22 ms [4.21 ms, 4.24 ms] | 40.0 [40.0, 40.2] | – | same |
| explicit_plastic | stable_time_step | final | 5 | 5 | 21.11 ms [21.04 ms, 21.35 ms] | 4.22 ms [4.21 ms, 4.27 ms] | 40.0 [39.9, 40.5] | 0.999 [0.995, 1.013] | same |
| explicit_plastic | solve_step | before | 5 | 120 | 2.573 s [2.543 s, 2.608 s] | 21.44 ms [21.19 ms, 21.73 ms] | 203.3 [201.0, 206.1] | – | same |
| explicit_plastic | solve_step | final | 5 | 120 | 2.507 s [2.493 s, 2.531 s] | 20.89 ms [20.78 ms, 21.10 ms] | 198.1 [197.0, 200.0] | 0.976 [0.961, 0.984] | same |
| explicit_plastic | predictor | before | 5 | 120 | 24.6 µs [20.5 µs, 26.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | predictor | final | 5 | 120 | 23.2 µs [18.8 µs, 31.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.918 [0.816, 1.330] | same |
| explicit_plastic | residual | before | 5 | 120 | 2.313 s [2.293 s, 2.335 s] | 19.28 ms [19.11 ms, 19.46 ms] | 182.8 [181.2, 184.5] | – | same |
| explicit_plastic | residual | final | 5 | 120 | 2.250 s [2.236 s, 2.268 s] | 18.75 ms [18.63 ms, 18.90 ms] | 177.8 [176.7, 179.3] | 0.975 [0.963, 0.981] | same |
| explicit_plastic | lumped_matrix | before | 5 | 120 | 53.9 µs [48.5 µs, 68.6 µs] | 0.4 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | lumped_matrix | final | 5 | 120 | 56.9 µs [44.2 µs, 61.3 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.001 [0.644, 1.180] | same |
| explicit_plastic | corrector | before | 5 | 120 | 21.8 µs [20.3 µs, 31.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | corrector | final | 5 | 120 | 32.3 µs [18.0 µs, 34.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.092 [0.803, 1.693] | same |
| explicit_plastic | step_hooks | before | 5 | 240 | 212.23 ms [204.98 ms, 221.88 ms] | 884.3 µs [854.1 µs, 924.5 µs] | 8.4 [8.1, 8.8] | – | same |
| explicit_plastic | step_hooks | final | 5 | 240 | 209.75 ms [205.22 ms, 216.00 ms] | 874.0 µs [855.1 µs, 900.0 µs] | 8.3 [8.1, 8.5] | 0.991 [0.945, 1.011] | same |
| explicit_plastic | step_rest | before | 5 | 120 | 47.30 ms [44.93 ms, 49.90 ms] | 394.2 µs [374.4 µs, 415.8 µs] | 3.7 [3.6, 3.9] | – | same |
| explicit_plastic | step_rest | final | 5 | 120 | 47.41 ms [44.78 ms, 48.39 ms] | 395.1 µs [373.2 µs, 403.2 µs] | 3.7 [3.5, 3.8] | 0.997 [0.932, 1.018] | same |
| explicit_plastic | internal_forces | before | 5 | 24 | 465.92 ms [465.14 ms, 469.61 ms] | 19.41 ms [19.38 ms, 19.57 ms] | 184.1 [183.8, 185.5] | – | same |
| explicit_plastic | internal_forces | final | 5 | 24 | 458.16 ms [451.77 ms, 462.69 ms] | 19.09 ms [18.82 ms, 19.28 ms] | 181.0 [178.5, 182.8] | 0.977 [0.964, 0.995] | same |
| explicit_plastic | stresses | before | 5 | 24 | 368.67 ms [364.16 ms, 378.84 ms] | 15.36 ms [15.17 ms, 15.78 ms] | 145.7 [143.9, 149.7] | – | same |
| explicit_plastic | stresses | final | 5 | 24 | 360.49 ms [353.98 ms, 362.12 ms] | 15.02 ms [14.75 ms, 15.09 ms] | 142.4 [139.9, 143.1] | 0.975 [0.956, 0.978] | same |
| explicit_plastic | energies | before | 5 | 24 | 81.90 ms [81.29 ms, 82.22 ms] | 3.41 ms [3.39 ms, 3.43 ms] | 32.4 [32.1, 32.5] | – | same |
| explicit_plastic | energies | final | 5 | 24 | 78.44 ms [78.14 ms, 79.09 ms] | 3.27 ms [3.26 ms, 3.30 ms] | 31.0 [30.9, 31.2] | 0.958 [0.955, 0.963] | same |
| explicit_sls | mesh | before | 5 | 1 | 2.09 ms [2.05 ms, 2.11 ms] | 2.09 ms [2.05 ms, 2.11 ms] | 19.8 [19.4, 20.0] | – | same |
| explicit_sls | mesh | final | 5 | 1 | 2.08 ms [2.06 ms, 2.11 ms] | 2.08 ms [2.06 ms, 2.11 ms] | 19.7 [19.5, 20.0] | 1.000 [0.988, 1.009] | same |
| explicit_sls | init | before | 5 | 1 | 39.17 ms [38.13 ms, 39.71 ms] | 39.17 ms [38.13 ms, 39.71 ms] | 371.4 [361.6, 376.6] | – | same |
| explicit_sls | init | final | 5 | 1 | 39.19 ms [38.59 ms, 39.60 ms] | 39.19 ms [38.59 ms, 39.60 ms] | 371.7 [365.9, 375.5] | 1.011 [0.978, 1.025] | same |
| explicit_sls | stable_time_step | before | 5 | 5 | 21.11 ms [21.04 ms, 21.14 ms] | 4.22 ms [4.21 ms, 4.23 ms] | 40.0 [39.9, 40.1] | – | same |
| explicit_sls | stable_time_step | final | 5 | 5 | 21.19 ms [21.14 ms, 22.05 ms] | 4.24 ms [4.23 ms, 4.41 ms] | 40.2 [40.1, 41.8] | 1.005 [1.002, 1.044] | same |
| explicit_sls | solve_step | before | 5 | 120 | 2.340 s [2.310 s, 2.410 s] | 19.50 ms [19.25 ms, 20.08 ms] | 184.9 [182.6, 190.4] | – | same |
| explicit_sls | solve_step | final | 5 | 120 | 2.377 s [2.318 s, 2.424 s] | 19.81 ms [19.32 ms, 20.20 ms] | 187.8 [183.2, 191.6] | 1.004 [0.991, 1.036] | same |
| explicit_sls | predictor | before | 5 | 120 | 19.6 µs [17.8 µs, 28.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | predictor | final | 5 | 120 | 19.1 µs [13.6 µs, 20.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.761 [0.692, 1.072] | same |
| explicit_sls | residual | before | 5 | 120 | 2.130 s [2.106 s, 2.182 s] | 17.75 ms [17.55 ms, 18.18 ms] | 168.4 [166.4, 172.4] | – | same |
| explicit_sls | residual | final | 5 | 120 | 2.154 s [2.108 s, 2.192 s] | 17.95 ms [17.57 ms, 18.27 ms] | 170.2 [166.6, 173.2] | 1.003 [0.990, 1.029] | same |
| explicit_sls | lumped_matrix | before | 5 | 120 | 48.0 µs [44.2 µs, 58.0 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | lumped_matrix | final | 5 | 120 | 50.2 µs [41.1 µs, 54.8 µs] | 0.4 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 0.946 [0.930, 1.102] | same |
| explicit_sls | corrector | before | 5 | 120 | 21.6 µs [17.9 µs, 30.1 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | corrector | final | 5 | 120 | 20.5 µs [16.9 µs, 20.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.819 [0.684, 1.161] | same |
| explicit_sls | step_hooks | before | 5 | 240 | 169.55 ms [163.29 ms, 181.68 ms] | 706.4 µs [680.4 µs, 757.0 µs] | 6.7 [6.5, 7.2] | – | same |
| explicit_sls | step_hooks | final | 5 | 240 | 176.24 ms [167.28 ms, 185.37 ms] | 734.3 µs [697.0 µs, 772.4 µs] | 7.0 [6.6, 7.3] | 1.018 [1.007, 1.097] | same |
| explicit_sls | step_rest | before | 5 | 120 | 42.40 ms [41.05 ms, 45.66 ms] | 353.4 µs [342.1 µs, 380.5 µs] | 3.4 [3.2, 3.6] | – | same |
| explicit_sls | step_rest | final | 5 | 120 | 46.07 ms [41.77 ms, 46.64 ms] | 383.9 µs [348.1 µs, 388.7 µs] | 3.6 [3.3, 3.7] | 1.022 [1.006, 1.135] | same |
| explicit_sls | internal_forces | before | 5 | 24 | 422.71 ms [416.75 ms, 426.20 ms] | 17.61 ms [17.36 ms, 17.76 ms] | 167.0 [164.7, 168.4] | – | same |
| explicit_sls | internal_forces | final | 5 | 24 | 432.94 ms [418.71 ms, 440.64 ms] | 18.04 ms [17.45 ms, 18.36 ms] | 171.1 [165.4, 174.1] | 1.017 [0.989, 1.055] | same |
| explicit_sls | stresses | before | 5 | 24 | 330.36 ms [326.36 ms, 332.78 ms] | 13.77 ms [13.60 ms, 13.87 ms] | 130.5 [128.9, 131.5] | – | same |
| explicit_sls | stresses | final | 5 | 24 | 330.25 ms [321.18 ms, 331.82 ms] | 13.76 ms [13.38 ms, 13.83 ms] | 130.5 [126.9, 131.1] | 0.992 [0.984, 1.014] | same |
| explicit_sls | energies | before | 5 | 24 | 105.58 ms [105.40 ms, 105.85 ms] | 4.40 ms [4.39 ms, 4.41 ms] | 41.7 [41.6, 41.8] | – | same |
| explicit_sls | energies | final | 5 | 24 | 85.22 ms [85.02 ms, 85.41 ms] | 3.55 ms [3.54 ms, 3.56 ms] | 33.7 [33.6, 33.7] | 0.807 [0.807, 0.808] | same |
| explicit_thermal | mesh | before | 5 | 1 | 2.10 ms [2.07 ms, 2.12 ms] | 2.10 ms [2.07 ms, 2.12 ms] | 19.9 [19.6, 20.1] | – | same |
| explicit_thermal | mesh | final | 5 | 1 | 2.09 ms [2.07 ms, 2.13 ms] | 2.09 ms [2.07 ms, 2.13 ms] | 19.8 [19.6, 20.2] | 0.998 [0.978, 1.026] | same |
| explicit_thermal | init | before | 5 | 1 | 32.50 ms [32.29 ms, 33.74 ms] | 32.50 ms [32.29 ms, 33.74 ms] | 308.1 [306.2, 319.9] | – | same |
| explicit_thermal | init | final | 5 | 1 | 33.20 ms [32.79 ms, 33.71 ms] | 33.20 ms [32.79 ms, 33.71 ms] | 314.8 [310.9, 319.7] | 1.026 [0.972, 1.044] | same |
| explicit_thermal | stable_time_step | before | 5 | 5 | 21.14 ms [21.07 ms, 21.66 ms] | 4.23 ms [4.21 ms, 4.33 ms] | 40.1 [40.0, 41.1] | – | same |
| explicit_thermal | stable_time_step | final | 5 | 5 | 21.14 ms [21.07 ms, 21.20 ms] | 4.23 ms [4.21 ms, 4.24 ms] | 40.1 [40.0, 40.2] | 0.997 [0.973, 1.006] | same |
| explicit_thermal | solve_step | before | 5 | 300 | 1.805 s [1.792 s, 1.821 s] | 6.02 ms [5.97 ms, 6.07 ms] | 57.1 [56.6, 57.6] | – | same |
| explicit_thermal | solve_step | final | 5 | 300 | 1.836 s [1.818 s, 1.856 s] | 6.12 ms [6.06 ms, 6.19 ms] | 58.0 [57.5, 58.7] | 1.016 [1.014, 1.028] | same |
| explicit_thermal | predictor | before | 5 | 300 | 33.6 µs [27.0 µs, 36.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | predictor | final | 5 | 300 | 32.9 µs [31.2 µs, 37.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.031 [0.893, 1.156] | same |
| explicit_thermal | residual | before | 5 | 300 | 1.525 s [1.516 s, 1.534 s] | 5.08 ms [5.05 ms, 5.11 ms] | 48.2 [47.9, 48.5] | – | same |
| explicit_thermal | residual | final | 5 | 300 | 1.554 s [1.542 s, 1.574 s] | 5.18 ms [5.14 ms, 5.25 ms] | 49.1 [48.7, 49.8] | 1.019 [1.016, 1.032] | same |
| explicit_thermal | lumped_matrix | before | 5 | 300 | 101.0 µs [93.4 µs, 111.4 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | lumped_matrix | final | 5 | 300 | 82.5 µs [76.4 µs, 94.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.818 [0.741, 0.927] | same |
| explicit_thermal | corrector | before | 5 | 300 | 36.3 µs [31.7 µs, 39.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | corrector | final | 5 | 300 | 35.1 µs [32.4 µs, 40.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.962 [0.915, 1.149] | same |
| explicit_thermal | step_hooks | before | 5 | 600 | 177.45 ms [174.11 ms, 181.67 ms] | 295.8 µs [290.2 µs, 302.8 µs] | 2.8 [2.8, 2.9] | – | same |
| explicit_thermal | step_hooks | final | 5 | 600 | 179.39 ms [173.83 ms, 180.27 ms] | 299.0 µs [289.7 µs, 300.5 µs] | 2.8 [2.7, 2.8] | 0.998 [0.992, 1.011] | same |
| explicit_thermal | step_rest | before | 5 | 300 | 101.65 ms [98.95 ms, 104.06 ms] | 338.8 µs [329.8 µs, 346.9 µs] | 3.2 [3.1, 3.3] | – | same |
| explicit_thermal | step_rest | final | 5 | 300 | 101.90 ms [101.35 ms, 103.04 ms] | 339.7 µs [337.8 µs, 343.5 µs] | 3.2 [3.2, 3.3] | 1.003 [0.990, 1.024] | same |
| explicit_thermal | internal_forces | before | 5 | 60 | 272.53 ms [269.70 ms, 274.58 ms] | 4.54 ms [4.49 ms, 4.58 ms] | 43.1 [42.6, 43.4] | – | same |
| explicit_thermal | internal_forces | final | 5 | 60 | 279.28 ms [277.62 ms, 281.51 ms] | 4.65 ms [4.63 ms, 4.69 ms] | 44.1 [43.9, 44.5] | 1.021 [1.018, 1.044] | same |
| explicit_thermal | stresses | before | 5 | 60 | 109.73 ms [108.67 ms, 110.39 ms] | 1.83 ms [1.81 ms, 1.84 ms] | 17.3 [17.2, 17.4] | – | same |
| explicit_thermal | stresses | final | 5 | 60 | 109.71 ms [109.31 ms, 110.08 ms] | 1.83 ms [1.82 ms, 1.83 ms] | 17.3 [17.3, 17.4] | 1.000 [0.997, 1.007] | same |
| explicit_thermal | energies | before | 5 | 60 | 199.97 ms [199.45 ms, 201.79 ms] | 3.33 ms [3.32 ms, 3.36 ms] | 31.6 [31.5, 31.9] | – | same |
| explicit_thermal | energies | final | 5 | 60 | 196.96 ms [195.13 ms, 197.05 ms] | 3.28 ms [3.25 ms, 3.28 ms] | 31.1 [30.8, 31.1] | 0.980 [0.977, 0.985] | same |
| static_newton_plastic | mesh | before | 5 | 1 | 154.5 µs [150.4 µs, 160.2 µs] | 154.5 µs [150.4 µs, 160.2 µs] | 25.8 [25.1, 26.7] | – | same |
| static_newton_plastic | mesh | final | 5 | 1 | 150.8 µs [150.5 µs, 163.5 µs] | 150.8 µs [150.5 µs, 163.5 µs] | 25.1 [25.1, 27.2] | 1.001 [0.962, 1.020] | same |
| static_newton_plastic | init | before | 5 | 1 | 2.32 ms [2.25 ms, 2.36 ms] | 2.32 ms [2.25 ms, 2.36 ms] | 387.0 [375.7, 393.2] | – | same |
| static_newton_plastic | init | final | 5 | 1 | 2.30 ms [2.26 ms, 2.31 ms] | 2.30 ms [2.26 ms, 2.31 ms] | 382.9 [376.9, 384.8] | 0.990 [0.974, 1.019] | same |
| static_newton_plastic | solve_step | before | 5 | 12 | 1.887 s [1.849 s, 1.892 s] | 157.22 ms [154.07 ms, 157.66 ms] | 26204.0 [25678.3, 26276.0] | – | same |
| static_newton_plastic | solve_step | final | 5 | 12 | 1.837 s [1.821 s, 1.922 s] | 153.11 ms [151.73 ms, 160.19 ms] | 25518.9 [25288.8, 26698.2] | 0.994 [0.964, 1.016] | same |
| static_newton_plastic | predictor | before | 5 | 12 | 0.5 µs [0.3 µs, 0.6 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | predictor | final | 5 | 12 | 0.6 µs [0.4 µs, 1.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.388 [1.302, 1.782] | same |
| static_newton_plastic | residual | before | 5 | 961 | 518.74 ms [514.14 ms, 521.32 ms] | 539.8 µs [535.0 µs, 542.5 µs] | 90.0 [89.2, 90.4] | – | same |
| static_newton_plastic | residual | final | 5 | 961 | 490.84 ms [487.96 ms, 491.86 ms] | 510.8 µs [507.8 µs, 511.8 µs] | 85.1 [84.6, 85.3] | 0.946 [0.936, 0.957] | same |
| static_newton_plastic | matrix | before | 5 | 949 | 200.6 µs [158.0 µs, 460.8 µs] | 0.2 µs [0.2 µs, 0.5 µs] | 0.0 [0.0, 0.1] | – | same |
| static_newton_plastic | matrix | final | 5 | 949 | 431.7 µs [256.0 µs, 449.0 µs] | 0.5 µs [0.3 µs, 0.5 µs] | 0.1 [0.0, 0.1] | 1.881 [0.966, 2.152] | same |
| static_newton_plastic | corrector | before | 5 | 949 | 88.5 µs [47.7 µs, 98.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | corrector | final | 5 | 949 | 69.4 µs [56.9 µs, 84.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.805 [0.666, 1.453] | same |
| static_newton_plastic | step_hooks | before | 5 | 24 | 761.3 µs [750.2 µs, 773.2 µs] | 31.7 µs [31.3 µs, 32.2 µs] | 5.3 [5.2, 5.4] | – | same |
| static_newton_plastic | step_hooks | final | 5 | 24 | 761.0 µs [749.4 µs, 764.6 µs] | 31.7 µs [31.2 µs, 31.9 µs] | 5.3 [5.2, 5.3] | 0.989 [0.986, 1.006] | same |
| static_newton_plastic | step_rest | before | 5 | 12 | 1.364 s [1.334 s, 1.372 s] | 113.69 ms [111.14 ms, 114.34 ms] | 18948.5 [18522.6, 19056.6] | – | same |
| static_newton_plastic | step_rest | final | 5 | 12 | 1.344 s [1.330 s, 1.430 s] | 112.02 ms [110.84 ms, 119.18 ms] | 18669.8 [18473.2, 19863.4] | 1.008 [0.973, 1.042] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_bilinear | 48000 | 46338 | 139014 | 1 | 78a330615, f25a10053 | b6a0645b6cf17997 |
| cohesive_intrinsic_exponential | 48000 | 46338 | 139014 | 1 | 78a330615, f25a10053 | 2e07a3f9b642f09a |
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 78a330615, f25a10053 | 88b89af4f4608998 |
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 78a330615, f25a10053 | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 36137566d8b810bb |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | 78a330615, f25a10053 | 27979d9fbcdea83d |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 78a330615, f25a10053 | 7bd5f114957d7d7b |
