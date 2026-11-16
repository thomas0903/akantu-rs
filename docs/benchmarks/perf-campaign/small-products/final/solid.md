Suite `solid`, size `m`; variants: `before`, `final`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | mesh | before | 5 | 1 | 1.84 ms [1.68 ms, 2.09 ms] | 1.84 ms [1.68 ms, 2.09 ms] | 22.1 [20.2, 25.2] | – | same |
| cohesive_extrinsic_check | mesh | final | 5 | 1 | 1.84 ms [1.82 ms, 1.95 ms] | 1.84 ms [1.82 ms, 1.95 ms] | 22.2 [21.9, 23.5] | 1.063 [0.884, 1.095] | same |
| cohesive_extrinsic_check | init | before | 5 | 1 | 880.05 ms [857.10 ms, 937.60 ms] | 880.05 ms [857.10 ms, 937.60 ms] | 10610.1 [10333.5, 11304.0] | – | same |
| cohesive_extrinsic_check | init | final | 5 | 1 | 757.88 ms [752.65 ms, 824.65 ms] | 757.88 ms [752.65 ms, 824.65 ms] | 9137.2 [9074.2, 9942.3] | 0.880 [0.807, 0.937] | same |
| cohesive_extrinsic_check | check_insertion | before | 5 | 100 | 3.969 s [3.932 s, 4.335 s] | 39.69 ms [39.32 ms, 43.35 ms] | 478.5 [474.1, 522.6] | – | same |
| cohesive_extrinsic_check | check_insertion | final | 5 | 100 | 3.108 s [3.088 s, 3.336 s] | 31.08 ms [30.88 ms, 33.36 ms] | 374.7 [372.3, 402.2] | 0.783 [0.717, 0.841] | same |
| cohesive_extrinsic_check | solve_step | before | 5 | 100 | 940.18 ms [905.70 ms, 988.85 ms] | 9.40 ms [9.06 ms, 9.89 ms] | 113.4 [109.2, 119.2] | – | same |
| cohesive_extrinsic_check | solve_step | final | 5 | 100 | 983.30 ms [933.81 ms, 1.030 s] | 9.83 ms [9.34 ms, 10.30 ms] | 118.5 [112.6, 124.1] | 1.041 [0.993, 1.119] | same |
| cohesive_intrinsic_bilinear | mesh | before | 5 | 1 | 1.16 ms [1.05 ms, 1.19 ms] | 1.16 ms [1.05 ms, 1.19 ms] | 24.1 [21.9, 24.7] | – | same |
| cohesive_intrinsic_bilinear | mesh | final | 5 | 1 | 1.20 ms [1.06 ms, 1.20 ms] | 1.20 ms [1.06 ms, 1.20 ms] | 25.0 [22.1, 25.0] | 1.018 [0.897, 1.112] | same |
| cohesive_intrinsic_bilinear | init | before | 5 | 1 | 832.35 ms [823.20 ms, 865.42 ms] | 832.35 ms [823.20 ms, 865.42 ms] | 17340.5 [17150.0, 18029.5] | – | same |
| cohesive_intrinsic_bilinear | init | final | 5 | 1 | 759.27 ms [725.56 ms, 829.16 ms] | 759.27 ms [725.56 ms, 829.16 ms] | 15818.2 [15115.8, 17274.2] | 0.912 [0.849, 1.007] | same |
| cohesive_intrinsic_bilinear | stable_time_step | before | 5 | 5 | 11.92 ms [11.68 ms, 12.53 ms] | 2.38 ms [2.34 ms, 2.51 ms] | 49.7 [48.7, 52.2] | – | same |
| cohesive_intrinsic_bilinear | stable_time_step | final | 5 | 5 | 11.34 ms [10.50 ms, 11.91 ms] | 2.27 ms [2.10 ms, 2.38 ms] | 47.2 [43.8, 49.6] | 0.971 [0.839, 0.984] | same |
| cohesive_intrinsic_bilinear | solve_step | before | 5 | 300 | 5.197 s [5.097 s, 5.261 s] | 17.32 ms [16.99 ms, 17.54 ms] | 360.9 [353.9, 365.3] | – | same |
| cohesive_intrinsic_bilinear | solve_step | final | 5 | 300 | 3.473 s [3.265 s, 3.482 s] | 11.58 ms [10.88 ms, 11.61 ms] | 241.2 [226.8, 241.8] | 0.662 [0.635, 0.683] | same |
| cohesive_intrinsic_bilinear | energies | before | 5 | 60 | 326.64 ms [306.07 ms, 334.31 ms] | 5.44 ms [5.10 ms, 5.57 ms] | 113.4 [106.3, 116.1] | – | same |
| cohesive_intrinsic_bilinear | energies | final | 5 | 60 | 150.05 ms [148.79 ms, 175.93 ms] | 2.50 ms [2.48 ms, 2.93 ms] | 52.1 [51.7, 61.1] | 0.457 [0.447, 0.540] | same |
| explicit_hex8_ss | mesh | before | 5 | 1 | 6.96 ms [6.61 ms, 7.56 ms] | 6.96 ms [6.61 ms, 7.56 ms] | 62.9 [59.8, 68.4] | – | same |
| explicit_hex8_ss | mesh | final | 5 | 1 | 7.64 ms [6.77 ms, 8.16 ms] | 7.64 ms [6.77 ms, 8.16 ms] | 69.1 [61.2, 73.8] | 1.101 [0.895, 1.221] | same |
| explicit_hex8_ss | init | before | 5 | 1 | 619.98 ms [587.31 ms, 643.19 ms] | 619.98 ms [587.31 ms, 643.19 ms] | 5606.0 [5310.6, 5815.9] | – | same |
| explicit_hex8_ss | init | final | 5 | 1 | 311.15 ms [286.38 ms, 340.92 ms] | 311.15 ms [286.38 ms, 340.92 ms] | 2813.5 [2589.5, 3082.7] | 0.492 [0.461, 0.580] | same |
| explicit_hex8_ss | stable_time_step | before | 5 | 5 | 24.07 ms [23.57 ms, 27.06 ms] | 4.81 ms [4.71 ms, 5.41 ms] | 43.5 [42.6, 48.9] | – | same |
| explicit_hex8_ss | stable_time_step | final | 5 | 5 | 23.99 ms [23.61 ms, 28.17 ms] | 4.80 ms [4.72 ms, 5.63 ms] | 43.4 [42.7, 51.0] | 1.002 [0.890, 1.170] | same |
| explicit_hex8_ss | solve_step | before | 5 | 50 | 2.544 s [2.345 s, 2.599 s] | 50.89 ms [46.90 ms, 51.97 ms] | 460.2 [424.1, 470.0] | – | same |
| explicit_hex8_ss | solve_step | final | 5 | 50 | 2.574 s [2.486 s, 2.595 s] | 51.48 ms [49.72 ms, 51.91 ms] | 465.5 [449.6, 469.4] | 1.014 [0.957, 1.107] | same |
| explicit_hex8_ss | predictor | before | 5 | 50 | 15.2 µs [13.7 µs, 15.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | predictor | final | 5 | 50 | 15.8 µs [13.9 µs, 16.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.032 [0.974, 1.156] | same |
| explicit_hex8_ss | residual | before | 5 | 50 | 2.082 s [1.924 s, 2.126 s] | 41.63 ms [38.48 ms, 42.52 ms] | 376.4 [347.9, 384.4] | – | same |
| explicit_hex8_ss | residual | final | 5 | 50 | 2.100 s [2.031 s, 2.118 s] | 42.00 ms [40.63 ms, 42.36 ms] | 379.8 [367.4, 383.1] | 1.010 [0.956, 1.101] | same |
| explicit_hex8_ss | lumped_matrix | before | 5 | 50 | 25.4 µs [19.1 µs, 29.3 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | lumped_matrix | final | 5 | 50 | 27.7 µs [24.7 µs, 28.8 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.044 [0.960, 1.404] | same |
| explicit_hex8_ss | corrector | before | 5 | 50 | 14.6 µs [13.7 µs, 17.6 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | corrector | final | 5 | 50 | 15.8 µs [15.2 µs, 16.8 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.129 [0.859, 1.180] | same |
| explicit_hex8_ss | step_hooks | before | 5 | 100 | 341.14 ms [310.97 ms, 346.00 ms] | 3.41 ms [3.11 ms, 3.46 ms] | 30.8 [28.1, 31.3] | – | same |
| explicit_hex8_ss | step_hooks | final | 5 | 100 | 350.07 ms [335.67 ms, 351.26 ms] | 3.50 ms [3.36 ms, 3.51 ms] | 31.7 [30.4, 31.8] | 1.030 [0.970, 1.126] | same |
| explicit_hex8_ss | step_rest | before | 5 | 50 | 121.68 ms [110.06 ms, 126.80 ms] | 2.43 ms [2.20 ms, 2.54 ms] | 22.0 [19.9, 22.9] | – | same |
| explicit_hex8_ss | step_rest | final | 5 | 50 | 123.52 ms [118.98 ms, 127.13 ms] | 2.47 ms [2.38 ms, 2.54 ms] | 22.3 [21.5, 23.0] | 1.022 [0.938, 1.155] | same |
| explicit_hex8_ss | internal_forces | before | 5 | 10 | 392.75 ms [371.94 ms, 443.64 ms] | 39.28 ms [37.19 ms, 44.36 ms] | 355.1 [336.3, 401.1] | – | same |
| explicit_hex8_ss | internal_forces | final | 5 | 10 | 381.35 ms [367.77 ms, 410.39 ms] | 38.14 ms [36.78 ms, 41.04 ms] | 344.8 [332.5, 371.1] | 0.958 [0.925, 1.011] | same |
| explicit_hex8_ss | stresses | before | 5 | 10 | 199.76 ms [166.18 ms, 206.34 ms] | 19.98 ms [16.62 ms, 20.63 ms] | 180.6 [150.3, 186.6] | – | same |
| explicit_hex8_ss | stresses | final | 5 | 10 | 170.99 ms [160.24 ms, 186.36 ms] | 17.10 ms [16.02 ms, 18.64 ms] | 154.6 [144.9, 168.5] | 0.903 [0.802, 1.023] | same |
| explicit_hex8_ss | energies | before | 5 | 10 | 618.26 ms [605.10 ms, 684.08 ms] | 61.83 ms [60.51 ms, 68.41 ms] | 559.0 [547.1, 618.6] | – | same |
| explicit_hex8_ss | energies | final | 5 | 10 | 413.51 ms [403.64 ms, 449.27 ms] | 41.35 ms [40.36 ms, 44.93 ms] | 373.9 [365.0, 406.2] | 0.659 [0.644, 0.692] | same |
| explicit_marigo | mesh | before | 5 | 1 | 2.20 ms [2.10 ms, 2.29 ms] | 2.20 ms [2.10 ms, 2.29 ms] | 20.9 [19.9, 21.7] | – | same |
| explicit_marigo | mesh | final | 5 | 1 | 2.21 ms [2.05 ms, 2.52 ms] | 2.21 ms [2.05 ms, 2.52 ms] | 21.0 [19.5, 23.9] | 1.005 [0.899, 1.202] | same |
| explicit_marigo | init | before | 5 | 1 | 54.07 ms [52.59 ms, 58.68 ms] | 54.07 ms [52.59 ms, 58.68 ms] | 512.7 [498.7, 556.5] | – | same |
| explicit_marigo | init | final | 5 | 1 | 35.40 ms [34.93 ms, 38.15 ms] | 35.40 ms [34.93 ms, 38.15 ms] | 335.7 [331.3, 361.8] | 0.664 [0.600, 0.706] | same |
| explicit_marigo | stable_time_step | before | 5 | 5 | 24.54 ms [22.65 ms, 26.01 ms] | 4.91 ms [4.53 ms, 5.20 ms] | 46.5 [43.0, 49.3] | – | same |
| explicit_marigo | stable_time_step | final | 5 | 5 | 23.06 ms [21.49 ms, 23.69 ms] | 4.61 ms [4.30 ms, 4.74 ms] | 43.7 [40.8, 44.9] | 0.911 [0.876, 1.029] | same |
| explicit_marigo | solve_step | before | 5 | 200 | 3.457 s [3.289 s, 3.586 s] | 17.29 ms [16.44 ms, 17.93 ms] | 163.9 [155.9, 170.0] | – | same |
| explicit_marigo | solve_step | final | 5 | 200 | 2.417 s [2.304 s, 2.446 s] | 12.08 ms [11.52 ms, 12.23 ms] | 114.6 [109.2, 116.0] | 0.690 [0.666, 0.735] | same |
| explicit_marigo | predictor | before | 5 | 200 | 46.0 µs [42.8 µs, 57.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | predictor | final | 5 | 200 | 52.5 µs [40.5 µs, 54.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.926 [0.881, 1.283] | same |
| explicit_marigo | residual | before | 5 | 200 | 1.996 s [1.897 s, 2.080 s] | 9.98 ms [9.48 ms, 10.40 ms] | 94.6 [89.9, 98.6] | – | same |
| explicit_marigo | residual | final | 5 | 200 | 1.758 s [1.690 s, 1.788 s] | 8.79 ms [8.45 ms, 8.94 ms] | 83.4 [80.1, 84.8] | 0.873 [0.847, 0.927] | same |
| explicit_marigo | lumped_matrix | before | 5 | 200 | 103.0 µs [78.4 µs, 121.3 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | lumped_matrix | final | 5 | 200 | 92.4 µs [90.1 µs, 107.5 µs] | 0.5 µs [0.5 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 0.954 [0.762, 1.372] | same |
| explicit_marigo | corrector | before | 5 | 200 | 62.7 µs [57.4 µs, 68.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | corrector | final | 5 | 200 | 60.1 µs [56.7 µs, 63.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.957 [0.929, 1.070] | same |
| explicit_marigo | step_hooks | before | 5 | 400 | 1.367 s [1.307 s, 1.411 s] | 3.42 ms [3.27 ms, 3.53 ms] | 32.4 [31.0, 33.5] | – | same |
| explicit_marigo | step_hooks | final | 5 | 400 | 560.96 ms [527.46 ms, 563.89 ms] | 1.40 ms [1.32 ms, 1.41 ms] | 13.3 [12.5, 13.4] | 0.400 [0.386, 0.431] | same |
| explicit_marigo | step_rest | before | 5 | 200 | 92.13 ms [84.04 ms, 93.09 ms] | 460.6 µs [420.2 µs, 465.5 µs] | 4.4 [4.0, 4.4] | – | same |
| explicit_marigo | step_rest | final | 5 | 200 | 92.34 ms [84.73 ms, 93.51 ms] | 461.7 µs [423.7 µs, 467.5 µs] | 4.4 [4.0, 4.4] | 1.002 [0.913, 1.113] | same |
| explicit_marigo | internal_forces | before | 5 | 40 | 396.40 ms [388.35 ms, 409.43 ms] | 9.91 ms [9.71 ms, 10.24 ms] | 94.0 [92.1, 97.1] | – | same |
| explicit_marigo | internal_forces | final | 5 | 40 | 355.55 ms [331.12 ms, 392.34 ms] | 8.89 ms [8.28 ms, 9.81 ms] | 84.3 [78.5, 93.0] | 0.901 [0.853, 0.967] | same |
| explicit_marigo | stresses | before | 5 | 40 | 277.58 ms [246.82 ms, 298.48 ms] | 6.94 ms [6.17 ms, 7.46 ms] | 65.8 [58.5, 70.8] | – | same |
| explicit_marigo | stresses | final | 5 | 40 | 205.75 ms [199.66 ms, 225.48 ms] | 5.14 ms [4.99 ms, 5.64 ms] | 48.8 [47.3, 53.5] | 0.755 [0.719, 0.834] | same |
| explicit_marigo | energies | before | 5 | 40 | 227.65 ms [220.06 ms, 239.03 ms] | 5.69 ms [5.50 ms, 5.98 ms] | 54.0 [52.2, 56.7] | – | same |
| explicit_marigo | energies | final | 5 | 40 | 69.01 ms [66.71 ms, 79.74 ms] | 1.73 ms [1.67 ms, 1.99 ms] | 16.4 [15.8, 18.9] | 0.306 [0.279, 0.350] | same |
| explicit_maxwell | mesh | before | 5 | 1 | 2.23 ms [2.08 ms, 2.56 ms] | 2.23 ms [2.08 ms, 2.56 ms] | 21.1 [19.7, 24.3] | – | same |
| explicit_maxwell | mesh | final | 5 | 1 | 2.29 ms [2.14 ms, 2.45 ms] | 2.29 ms [2.14 ms, 2.45 ms] | 21.7 [20.3, 23.2] | 0.960 [0.835, 1.176] | same |
| explicit_maxwell | init | before | 5 | 1 | 63.42 ms [57.39 ms, 66.98 ms] | 63.42 ms [57.39 ms, 66.98 ms] | 601.4 [544.2, 635.2] | – | same |
| explicit_maxwell | init | final | 5 | 1 | 41.05 ms [40.16 ms, 45.32 ms] | 41.05 ms [40.16 ms, 45.32 ms] | 389.2 [380.8, 429.7] | 0.647 [0.601, 0.786] | same |
| explicit_maxwell | stable_time_step | before | 5 | 5 | 26.14 ms [22.59 ms, 26.79 ms] | 5.23 ms [4.52 ms, 5.36 ms] | 49.6 [42.8, 50.8] | – | same |
| explicit_maxwell | stable_time_step | final | 5 | 5 | 22.26 ms [22.07 ms, 25.02 ms] | 4.45 ms [4.41 ms, 5.00 ms] | 42.2 [41.9, 47.5] | 0.844 [0.831, 1.084] | same |
| explicit_maxwell | solve_step | before | 5 | 40 | 4.382 s [4.310 s, 4.530 s] | 109.55 ms [107.76 ms, 113.24 ms] | 1038.8 [1021.8, 1073.8] | – | same |
| explicit_maxwell | solve_step | final | 5 | 40 | 3.600 s [3.475 s, 3.678 s] | 90.00 ms [86.88 ms, 91.96 ms] | 853.5 [823.9, 872.0] | 0.814 [0.796, 0.839] | same |
| explicit_maxwell | predictor | before | 5 | 40 | 9.2 µs [8.0 µs, 13.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | predictor | final | 5 | 40 | 10.5 µs [8.7 µs, 18.9 µs] | 0.3 µs [0.2 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.025 [0.819, 2.350] | same |
| explicit_maxwell | residual | before | 5 | 40 | 1.372 s [1.356 s, 1.440 s] | 34.30 ms [33.90 ms, 35.99 ms] | 325.3 [321.5, 341.3] | – | same |
| explicit_maxwell | residual | final | 5 | 40 | 978.12 ms [954.48 ms, 1.005 s] | 24.45 ms [23.86 ms, 25.12 ms] | 231.9 [226.3, 238.2] | 0.713 [0.698, 0.723] | same |
| explicit_maxwell | lumped_matrix | before | 5 | 40 | 22.4 µs [20.8 µs, 27.6 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | lumped_matrix | final | 5 | 40 | 20.1 µs [17.5 µs, 24.4 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 0.783 [0.695, 1.148] | same |
| explicit_maxwell | corrector | before | 5 | 40 | 12.1 µs [7.9 µs, 12.2 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | corrector | final | 5 | 40 | 8.4 µs [7.6 µs, 12.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.698 [0.688, 1.521] | same |
| explicit_maxwell | step_hooks | before | 5 | 80 | 2.982 s [2.936 s, 3.071 s] | 37.28 ms [36.70 ms, 38.38 ms] | 353.5 [348.0, 364.0] | – | same |
| explicit_maxwell | step_hooks | final | 5 | 80 | 2.584 s [2.503 s, 2.660 s] | 32.30 ms [31.28 ms, 33.25 ms] | 306.2 [296.7, 315.3] | 0.858 [0.841, 0.892] | same |
| explicit_maxwell | step_rest | before | 5 | 40 | 18.18 ms [17.98 ms, 18.76 ms] | 454.6 µs [449.4 µs, 468.9 µs] | 4.3 [4.3, 4.4] | – | same |
| explicit_maxwell | step_rest | final | 5 | 40 | 18.70 ms [17.84 ms, 18.90 ms] | 467.6 µs [445.9 µs, 472.4 µs] | 4.4 [4.2, 4.5] | 1.017 [0.965, 1.046] | same |
| explicit_maxwell | internal_forces | before | 5 | 8 | 263.90 ms [251.62 ms, 275.36 ms] | 32.99 ms [31.45 ms, 34.42 ms] | 312.8 [298.3, 326.4] | – | same |
| explicit_maxwell | internal_forces | final | 5 | 8 | 190.49 ms [175.09 ms, 192.74 ms] | 23.81 ms [21.89 ms, 24.09 ms] | 225.8 [207.5, 228.5] | 0.700 [0.660, 0.763] | same |
| explicit_maxwell | stresses | before | 5 | 8 | 234.43 ms [217.67 ms, 260.01 ms] | 29.30 ms [27.21 ms, 32.50 ms] | 277.9 [258.0, 308.2] | – | same |
| explicit_maxwell | stresses | final | 5 | 8 | 152.24 ms [142.22 ms, 175.82 ms] | 19.03 ms [17.78 ms, 21.98 ms] | 180.4 [168.6, 208.4] | 0.607 [0.563, 0.808] | same |
| explicit_maxwell | energies | before | 5 | 8 | 38.26 ms [34.56 ms, 40.92 ms] | 4.78 ms [4.32 ms, 5.11 ms] | 45.3 [41.0, 48.5] | – | same |
| explicit_maxwell | energies | final | 5 | 8 | 22.83 ms [20.46 ms, 24.66 ms] | 2.85 ms [2.56 ms, 3.08 ms] | 27.1 [24.3, 29.2] | 0.597 [0.514, 0.713] | same |
| explicit_mazars | mesh | before | 5 | 1 | 2.42 ms [2.14 ms, 2.92 ms] | 2.42 ms [2.14 ms, 2.92 ms] | 22.9 [20.3, 27.7] | – | same |
| explicit_mazars | mesh | final | 5 | 1 | 2.48 ms [2.16 ms, 2.81 ms] | 2.48 ms [2.16 ms, 2.81 ms] | 23.5 [20.4, 26.6] | 0.983 [0.875, 1.162] | same |
| explicit_mazars | init | before | 5 | 1 | 57.62 ms [54.17 ms, 63.88 ms] | 57.62 ms [54.17 ms, 63.88 ms] | 546.4 [513.7, 605.8] | – | same |
| explicit_mazars | init | final | 5 | 1 | 42.36 ms [36.39 ms, 44.03 ms] | 42.36 ms [36.39 ms, 44.03 ms] | 401.7 [345.1, 417.5] | 0.686 [0.664, 0.782] | same |
| explicit_mazars | stable_time_step | before | 5 | 5 | 23.47 ms [22.67 ms, 26.59 ms] | 4.69 ms [4.53 ms, 5.32 ms] | 44.5 [43.0, 50.4] | – | same |
| explicit_mazars | stable_time_step | final | 5 | 5 | 25.58 ms [21.66 ms, 25.85 ms] | 5.12 ms [4.33 ms, 5.17 ms] | 48.5 [41.1, 49.0] | 0.995 [0.932, 1.128] | same |
| explicit_mazars | solve_step | before | 5 | 80 | 5.949 s [5.758 s, 6.102 s] | 74.36 ms [71.98 ms, 76.28 ms] | 705.1 [682.6, 723.3] | – | same |
| explicit_mazars | solve_step | final | 5 | 80 | 5.556 s [5.339 s, 5.654 s] | 69.45 ms [66.74 ms, 70.67 ms] | 658.6 [632.9, 670.2] | 0.914 [0.899, 0.969] | same |
| explicit_mazars | predictor | before | 5 | 80 | 23.3 µs [16.2 µs, 44.7 µs] | 0.3 µs [0.2 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | final | 5 | 80 | 21.0 µs [17.8 µs, 24.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.818 [0.399, 1.314] | same |
| explicit_mazars | residual | before | 5 | 80 | 5.351 s [5.190 s, 5.491 s] | 66.88 ms [64.88 ms, 68.64 ms] | 634.2 [615.2, 650.9] | – | same |
| explicit_mazars | residual | final | 5 | 80 | 5.310 s [5.086 s, 5.353 s] | 66.37 ms [63.57 ms, 66.91 ms] | 629.4 [602.8, 634.5] | 0.967 [0.951, 1.025] | same |
| explicit_mazars | lumped_matrix | before | 5 | 80 | 40.8 µs [27.4 µs, 47.9 µs] | 0.5 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | final | 5 | 80 | 41.6 µs [35.0 µs, 50.7 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 0.968 [0.791, 1.624] | same |
| explicit_mazars | corrector | before | 5 | 80 | 24.6 µs [23.6 µs, 26.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | final | 5 | 80 | 25.6 µs [25.0 µs, 26.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.018 [0.986, 1.108] | same |
| explicit_mazars | step_hooks | before | 5 | 160 | 553.85 ms [532.79 ms, 590.06 ms] | 3.46 ms [3.33 ms, 3.69 ms] | 32.8 [31.6, 35.0] | – | same |
| explicit_mazars | step_hooks | final | 5 | 160 | 223.20 ms [210.42 ms, 258.39 ms] | 1.40 ms [1.32 ms, 1.61 ms] | 13.2 [12.5, 15.3] | 0.413 [0.381, 0.462] | same |
| explicit_mazars | step_rest | before | 5 | 80 | 37.66 ms [34.58 ms, 41.80 ms] | 470.7 µs [432.3 µs, 522.5 µs] | 4.5 [4.1, 5.0] | – | same |
| explicit_mazars | step_rest | final | 5 | 80 | 38.32 ms [35.31 ms, 41.65 ms] | 479.0 µs [441.4 µs, 520.7 µs] | 4.5 [4.2, 4.9] | 0.991 [0.938, 1.108] | same |
| explicit_mazars | internal_forces | before | 5 | 16 | 1.137 s [1.116 s, 1.151 s] | 71.05 ms [69.77 ms, 71.96 ms] | 673.7 [661.6, 682.4] | – | same |
| explicit_mazars | internal_forces | final | 5 | 16 | 1.171 s [1.056 s, 1.239 s] | 73.21 ms [65.99 ms, 77.43 ms] | 694.3 [625.8, 734.2] | 1.030 [0.936, 1.076] | same |
| explicit_mazars | stresses | before | 5 | 16 | 1.106 s [1.027 s, 1.245 s] | 69.16 ms [64.18 ms, 77.82 ms] | 655.8 [608.6, 737.9] | – | same |
| explicit_mazars | stresses | final | 5 | 16 | 1.061 s [1.027 s, 1.093 s] | 66.34 ms [64.18 ms, 68.29 ms] | 629.1 [608.6, 647.6] | 0.985 [0.833, 1.034] | same |
| explicit_mazars | energies | before | 5 | 16 | 79.64 ms [78.37 ms, 88.22 ms] | 4.98 ms [4.90 ms, 5.51 ms] | 47.2 [46.4, 52.3] | – | same |
| explicit_mazars | energies | final | 5 | 16 | 19.11 ms [18.20 ms, 20.06 ms] | 1.19 ms [1.14 ms, 1.25 ms] | 11.3 [10.8, 11.9] | 0.233 [0.209, 0.249] | same |
| explicit_neohookean | mesh | before | 5 | 1 | 2.29 ms [2.15 ms, 2.38 ms] | 2.29 ms [2.15 ms, 2.38 ms] | 21.7 [20.4, 22.6] | – | same |
| explicit_neohookean | mesh | final | 5 | 1 | 2.20 ms [2.13 ms, 2.28 ms] | 2.20 ms [2.13 ms, 2.28 ms] | 20.9 [20.2, 21.6] | 0.960 [0.906, 1.040] | same |
| explicit_neohookean | init | before | 5 | 1 | 58.98 ms [55.36 ms, 61.51 ms] | 58.98 ms [55.36 ms, 61.51 ms] | 559.2 [525.0, 583.3] | – | same |
| explicit_neohookean | init | final | 5 | 1 | 40.18 ms [37.90 ms, 40.54 ms] | 40.18 ms [37.90 ms, 40.54 ms] | 381.1 [359.4, 384.5] | 0.683 [0.639, 0.732] | same |
| explicit_neohookean | stable_time_step | before | 5 | 5 | 23.42 ms [22.42 ms, 24.74 ms] | 4.68 ms [4.48 ms, 4.95 ms] | 44.4 [42.5, 46.9] | – | same |
| explicit_neohookean | stable_time_step | final | 5 | 5 | 22.29 ms [21.48 ms, 22.78 ms] | 4.46 ms [4.30 ms, 4.56 ms] | 42.3 [40.7, 43.2] | 0.957 [0.901, 0.958] | same |
| explicit_neohookean | solve_step | before | 5 | 100 | 2.407 s [2.369 s, 2.522 s] | 24.07 ms [23.69 ms, 25.22 ms] | 228.3 [224.7, 239.1] | – | same |
| explicit_neohookean | solve_step | final | 5 | 100 | 1.743 s [1.704 s, 1.776 s] | 17.43 ms [17.04 ms, 17.76 ms] | 165.3 [161.6, 168.4] | 0.710 [0.691, 0.750] | same |
| explicit_neohookean | predictor | before | 5 | 100 | 17.2 µs [13.9 µs, 23.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | predictor | final | 5 | 100 | 17.3 µs [13.5 µs, 22.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.926 [0.883, 1.243] | same |
| explicit_neohookean | residual | before | 5 | 100 | 2.264 s [2.216 s, 2.369 s] | 22.64 ms [22.16 ms, 23.69 ms] | 214.7 [210.1, 224.7] | – | same |
| explicit_neohookean | residual | final | 5 | 100 | 1.593 s [1.543 s, 1.616 s] | 15.93 ms [15.43 ms, 16.16 ms] | 151.1 [146.3, 153.3] | 0.689 [0.667, 0.729] | same |
| explicit_neohookean | lumped_matrix | before | 5 | 100 | 32.9 µs [28.3 µs, 35.0 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | lumped_matrix | final | 5 | 100 | 38.8 µs [37.6 µs, 45.7 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.305 [1.114, 1.330] | same |
| explicit_neohookean | corrector | before | 5 | 100 | 23.7 µs [16.6 µs, 29.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | corrector | final | 5 | 100 | 23.2 µs [20.8 µs, 28.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.206 [0.759, 1.309] | same |
| explicit_neohookean | step_hooks | before | 5 | 200 | 110.25 ms [99.91 ms, 112.53 ms] | 551.2 µs [499.5 µs, 562.6 µs] | 5.2 [4.7, 5.3] | – | same |
| explicit_neohookean | step_hooks | final | 5 | 200 | 110.05 ms [107.17 ms, 116.66 ms] | 550.2 µs [535.9 µs, 583.3 µs] | 5.2 [5.1, 5.5] | 1.039 [0.975, 1.101] | same |
| explicit_neohookean | step_rest | before | 5 | 100 | 40.75 ms [38.26 ms, 42.00 ms] | 407.5 µs [382.6 µs, 420.0 µs] | 3.9 [3.6, 4.0] | – | same |
| explicit_neohookean | step_rest | final | 5 | 100 | 42.52 ms [41.52 ms, 44.20 ms] | 425.2 µs [415.2 µs, 442.0 µs] | 4.0 [3.9, 4.2] | 1.056 [0.990, 1.111] | same |
| explicit_neohookean | internal_forces | before | 5 | 20 | 484.67 ms [458.78 ms, 507.33 ms] | 24.23 ms [22.94 ms, 25.37 ms] | 229.8 [217.5, 240.5] | – | same |
| explicit_neohookean | internal_forces | final | 5 | 20 | 299.90 ms [289.40 ms, 312.29 ms] | 15.00 ms [14.47 ms, 15.61 ms] | 142.2 [137.2, 148.1] | 0.623 [0.571, 0.643] | same |
| explicit_neohookean | stresses | before | 5 | 20 | 332.97 ms [321.07 ms, 377.64 ms] | 16.65 ms [16.05 ms, 18.88 ms] | 157.9 [152.2, 179.1] | – | same |
| explicit_neohookean | stresses | final | 5 | 20 | 187.24 ms [182.34 ms, 214.14 ms] | 9.36 ms [9.12 ms, 10.71 ms] | 88.8 [86.5, 101.5] | 0.572 [0.533, 0.588] | same |
| explicit_neohookean | energies | before | 5 | 20 | 420.95 ms [410.87 ms, 452.75 ms] | 21.05 ms [20.54 ms, 22.64 ms] | 199.6 [194.8, 214.7] | – | same |
| explicit_neohookean | energies | final | 5 | 20 | 208.31 ms [198.04 ms, 222.46 ms] | 10.42 ms [9.90 ms, 11.12 ms] | 98.8 [93.9, 105.5] | 0.487 [0.476, 0.533] | same |
| explicit_orthotropic | mesh | before | 5 | 1 | 2.33 ms [2.09 ms, 2.54 ms] | 2.33 ms [2.09 ms, 2.54 ms] | 22.1 [19.8, 24.1] | – | same |
| explicit_orthotropic | mesh | final | 5 | 1 | 2.24 ms [2.11 ms, 2.46 ms] | 2.24 ms [2.11 ms, 2.46 ms] | 21.3 [20.0, 23.3] | 0.993 [0.844, 1.155] | same |
| explicit_orthotropic | init | before | 5 | 1 | 55.67 ms [51.24 ms, 59.48 ms] | 55.67 ms [51.24 ms, 59.48 ms] | 527.9 [485.9, 564.1] | – | same |
| explicit_orthotropic | init | final | 5 | 1 | 36.90 ms [33.56 ms, 39.13 ms] | 36.90 ms [33.56 ms, 39.13 ms] | 349.9 [318.3, 371.0] | 0.663 [0.564, 0.741] | same |
| explicit_orthotropic | stable_time_step | before | 5 | 5 | 25.85 ms [22.98 ms, 26.63 ms] | 5.17 ms [4.60 ms, 5.33 ms] | 49.0 [43.6, 50.5] | – | same |
| explicit_orthotropic | stable_time_step | final | 5 | 5 | 22.20 ms [21.32 ms, 24.59 ms] | 4.44 ms [4.26 ms, 4.92 ms] | 42.1 [40.4, 46.6] | 0.859 [0.825, 1.038] | same |
| explicit_orthotropic | solve_step | before | 5 | 200 | 2.090 s [2.058 s, 2.130 s] | 10.45 ms [10.29 ms, 10.65 ms] | 99.1 [97.6, 101.0] | – | same |
| explicit_orthotropic | solve_step | final | 5 | 200 | 1.985 s [1.914 s, 2.118 s] | 9.92 ms [9.57 ms, 10.59 ms] | 94.1 [90.8, 100.4] | 0.950 [0.898, 1.014] | same |
| explicit_orthotropic | predictor | before | 5 | 200 | 27.6 µs [21.0 µs, 33.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | predictor | final | 5 | 200 | 35.5 µs [34.0 µs, 60.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.244 [1.048, 2.866] | same |
| explicit_orthotropic | residual | before | 5 | 200 | 1.881 s [1.853 s, 1.902 s] | 9.40 ms [9.26 ms, 9.51 ms] | 89.2 [87.8, 90.2] | – | same |
| explicit_orthotropic | residual | final | 5 | 200 | 1.768 s [1.707 s, 1.888 s] | 8.84 ms [8.54 ms, 9.44 ms] | 83.8 [81.0, 89.5] | 0.945 [0.908, 1.002] | same |
| explicit_orthotropic | lumped_matrix | before | 5 | 200 | 64.6 µs [61.6 µs, 79.6 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | lumped_matrix | final | 5 | 200 | 62.6 µs [59.3 µs, 68.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.999 [0.745, 1.033] | same |
| explicit_orthotropic | corrector | before | 5 | 200 | 31.1 µs [27.3 µs, 54.2 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | corrector | final | 5 | 200 | 45.6 µs [37.1 µs, 46.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.272 [0.857, 1.465] | same |
| explicit_orthotropic | step_hooks | before | 5 | 400 | 130.21 ms [125.84 ms, 164.13 ms] | 325.5 µs [314.6 µs, 410.3 µs] | 3.1 [3.0, 3.9] | – | same |
| explicit_orthotropic | step_hooks | final | 5 | 400 | 131.28 ms [120.69 ms, 140.77 ms] | 328.2 µs [301.7 µs, 351.9 µs] | 3.1 [2.9, 3.3] | 0.981 [0.784, 1.119] | same |
| explicit_orthotropic | step_rest | before | 5 | 200 | 80.81 ms [77.83 ms, 85.17 ms] | 404.1 µs [389.2 µs, 425.8 µs] | 3.8 [3.7, 4.0] | – | same |
| explicit_orthotropic | step_rest | final | 5 | 200 | 81.25 ms [76.14 ms, 88.85 ms] | 406.2 µs [380.7 µs, 444.3 µs] | 3.9 [3.6, 4.2] | 1.005 [0.909, 1.142] | same |
| explicit_orthotropic | internal_forces | before | 5 | 40 | 344.20 ms [331.55 ms, 405.56 ms] | 8.60 ms [8.29 ms, 10.14 ms] | 81.6 [78.6, 96.1] | – | same |
| explicit_orthotropic | internal_forces | final | 5 | 40 | 365.26 ms [322.84 ms, 382.13 ms] | 9.13 ms [8.07 ms, 9.55 ms] | 86.6 [76.5, 90.6] | 0.942 [0.932, 1.105] | same |
| explicit_orthotropic | stresses | before | 5 | 40 | 230.79 ms [222.20 ms, 246.99 ms] | 5.77 ms [5.56 ms, 6.17 ms] | 54.7 [52.7, 58.6] | – | same |
| explicit_orthotropic | stresses | final | 5 | 40 | 222.70 ms [201.07 ms, 234.03 ms] | 5.57 ms [5.03 ms, 5.85 ms] | 52.8 [47.7, 55.5] | 0.973 [0.871, 0.976] | same |
| explicit_orthotropic | energies | before | 5 | 40 | 306.35 ms [296.10 ms, 340.10 ms] | 7.66 ms [7.40 ms, 8.50 ms] | 72.6 [70.2, 80.6] | – | same |
| explicit_orthotropic | energies | final | 5 | 40 | 151.95 ms [131.54 ms, 156.30 ms] | 3.80 ms [3.29 ms, 3.91 ms] | 36.0 [31.2, 37.1] | 0.487 [0.387, 0.513] | same |
| explicit_plastic | mesh | before | 5 | 1 | 2.36 ms [2.10 ms, 2.61 ms] | 2.36 ms [2.10 ms, 2.61 ms] | 22.4 [19.9, 24.7] | – | same |
| explicit_plastic | mesh | final | 5 | 1 | 2.44 ms [2.30 ms, 2.51 ms] | 2.44 ms [2.30 ms, 2.51 ms] | 23.1 [21.8, 23.8] | 0.980 [0.936, 1.113] | same |
| explicit_plastic | init | before | 5 | 1 | 62.42 ms [54.19 ms, 65.82 ms] | 62.42 ms [54.19 ms, 65.82 ms] | 591.9 [513.9, 624.1] | – | same |
| explicit_plastic | init | final | 5 | 1 | 43.19 ms [37.70 ms, 44.58 ms] | 43.19 ms [37.70 ms, 44.58 ms] | 409.6 [357.5, 422.7] | 0.685 [0.604, 0.748] | same |
| explicit_plastic | stable_time_step | before | 5 | 5 | 25.95 ms [22.64 ms, 27.34 ms] | 5.19 ms [4.53 ms, 5.47 ms] | 49.2 [42.9, 51.8] | – | same |
| explicit_plastic | stable_time_step | final | 5 | 5 | 24.69 ms [21.62 ms, 25.82 ms] | 4.94 ms [4.32 ms, 5.16 ms] | 46.8 [41.0, 49.0] | 0.944 [0.833, 1.074] | same |
| explicit_plastic | solve_step | before | 5 | 120 | 3.853 s [3.772 s, 3.953 s] | 32.11 ms [31.43 ms, 32.94 ms] | 304.5 [298.1, 312.4] | – | same |
| explicit_plastic | solve_step | final | 5 | 120 | 3.006 s [2.930 s, 3.093 s] | 25.05 ms [24.41 ms, 25.78 ms] | 237.6 [231.5, 244.4] | 0.786 [0.755, 0.806] | same |
| explicit_plastic | predictor | before | 5 | 120 | 29.8 µs [25.7 µs, 31.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | predictor | final | 5 | 120 | 25.0 µs [21.0 µs, 39.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.905 [0.701, 1.376] | same |
| explicit_plastic | residual | before | 5 | 120 | 3.327 s [3.262 s, 3.414 s] | 27.73 ms [27.19 ms, 28.45 ms] | 262.9 [257.8, 269.7] | – | same |
| explicit_plastic | residual | final | 5 | 120 | 2.697 s [2.626 s, 2.770 s] | 22.48 ms [21.89 ms, 23.09 ms] | 213.1 [207.5, 218.9] | 0.813 [0.784, 0.835] | same |
| explicit_plastic | lumped_matrix | before | 5 | 120 | 56.1 µs [52.0 µs, 67.9 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | lumped_matrix | final | 5 | 120 | 45.8 µs [38.8 µs, 74.8 µs] | 0.4 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 0.817 [0.609, 1.438] | same |
| explicit_plastic | corrector | before | 5 | 120 | 24.6 µs [23.5 µs, 37.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | corrector | final | 5 | 120 | 24.1 µs [22.8 µs, 25.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.979 [0.649, 1.027] | same |
| explicit_plastic | step_hooks | before | 5 | 240 | 469.55 ms [454.66 ms, 481.19 ms] | 1.96 ms [1.89 ms, 2.00 ms] | 18.6 [18.0, 19.0] | – | same |
| explicit_plastic | step_hooks | final | 5 | 240 | 252.69 ms [238.96 ms, 264.59 ms] | 1.05 ms [995.7 µs, 1.10 ms] | 10.0 [9.4, 10.5] | 0.556 [0.497, 0.568] | same |
| explicit_plastic | step_rest | before | 5 | 120 | 55.42 ms [54.22 ms, 57.63 ms] | 461.8 µs [451.8 µs, 480.3 µs] | 4.4 [4.3, 4.6] | – | same |
| explicit_plastic | step_rest | final | 5 | 120 | 55.58 ms [51.39 ms, 58.52 ms] | 463.1 µs [428.2 µs, 487.6 µs] | 4.4 [4.1, 4.6] | 1.025 [0.892, 1.074] | same |
| explicit_plastic | internal_forces | before | 5 | 24 | 651.40 ms [640.72 ms, 674.69 ms] | 27.14 ms [26.70 ms, 28.11 ms] | 257.4 [253.2, 266.6] | – | same |
| explicit_plastic | internal_forces | final | 5 | 24 | 551.25 ms [533.43 ms, 558.69 ms] | 22.97 ms [22.23 ms, 23.28 ms] | 217.8 [210.8, 220.7] | 0.836 [0.817, 0.848] | same |
| explicit_plastic | stresses | before | 5 | 24 | 553.38 ms [547.62 ms, 594.74 ms] | 23.06 ms [22.82 ms, 24.78 ms] | 218.6 [216.4, 235.0] | – | same |
| explicit_plastic | stresses | final | 5 | 24 | 450.65 ms [434.95 ms, 462.85 ms] | 18.78 ms [18.12 ms, 19.29 ms] | 178.1 [171.9, 182.9] | 0.795 [0.756, 0.835] | same |
| explicit_plastic | energies | before | 5 | 24 | 180.32 ms [170.36 ms, 182.62 ms] | 7.51 ms [7.10 ms, 7.61 ms] | 71.2 [67.3, 72.2] | – | same |
| explicit_plastic | energies | final | 5 | 24 | 86.54 ms [82.92 ms, 96.99 ms] | 3.61 ms [3.46 ms, 4.04 ms] | 34.2 [32.8, 38.3] | 0.488 [0.460, 0.554] | same |
| explicit_sls | mesh | before | 5 | 1 | 2.50 ms [2.18 ms, 2.58 ms] | 2.50 ms [2.18 ms, 2.58 ms] | 23.7 [20.7, 24.4] | – | same |
| explicit_sls | mesh | final | 5 | 1 | 2.53 ms [2.45 ms, 2.73 ms] | 2.53 ms [2.45 ms, 2.73 ms] | 24.0 [23.2, 25.9] | 0.993 [0.978, 1.158] | same |
| explicit_sls | init | before | 5 | 1 | 66.41 ms [58.80 ms, 67.84 ms] | 66.41 ms [58.80 ms, 67.84 ms] | 629.8 [557.6, 643.3] | – | same |
| explicit_sls | init | final | 5 | 1 | 47.75 ms [45.71 ms, 49.31 ms] | 47.75 ms [45.71 ms, 49.31 ms] | 452.8 [433.5, 467.6] | 0.711 [0.688, 0.839] | same |
| explicit_sls | stable_time_step | before | 5 | 5 | 25.17 ms [24.48 ms, 27.13 ms] | 5.03 ms [4.90 ms, 5.43 ms] | 47.7 [46.4, 51.4] | – | same |
| explicit_sls | stable_time_step | final | 5 | 5 | 25.13 ms [24.40 ms, 26.34 ms] | 5.03 ms [4.88 ms, 5.27 ms] | 47.7 [46.3, 49.9] | 0.984 [0.944, 1.076] | same |
| explicit_sls | solve_step | before | 5 | 120 | 3.134 s [2.916 s, 3.336 s] | 26.12 ms [24.30 ms, 27.80 ms] | 247.6 [230.4, 263.7] | – | same |
| explicit_sls | solve_step | final | 5 | 120 | 2.837 s [2.747 s, 3.042 s] | 23.64 ms [22.89 ms, 25.35 ms] | 224.2 [217.1, 240.4] | 0.925 [0.898, 0.964] | same |
| explicit_sls | predictor | before | 5 | 120 | 28.6 µs [21.2 µs, 37.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | predictor | final | 5 | 120 | 25.0 µs [22.4 µs, 38.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.083 [0.678, 1.159] | same |
| explicit_sls | residual | before | 5 | 120 | 2.863 s [2.678 s, 3.001 s] | 23.85 ms [22.32 ms, 25.01 ms] | 226.2 [211.6, 237.1] | – | same |
| explicit_sls | residual | final | 5 | 120 | 2.569 s [2.499 s, 2.706 s] | 21.41 ms [20.83 ms, 22.55 ms] | 203.0 [197.5, 213.8] | 0.917 [0.891, 0.951] | same |
| explicit_sls | lumped_matrix | before | 5 | 120 | 59.7 µs [42.7 µs, 68.5 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | lumped_matrix | final | 5 | 120 | 55.0 µs [37.6 µs, 79.1 µs] | 0.5 µs [0.3 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.991 [0.735, 1.274] | same |
| explicit_sls | corrector | before | 5 | 120 | 25.3 µs [19.7 µs, 28.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | corrector | final | 5 | 120 | 34.2 µs [25.3 µs, 42.3 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.474 [1.003, 1.549] | same |
| explicit_sls | step_hooks | before | 5 | 240 | 215.34 ms [189.68 ms, 274.63 ms] | 897.3 µs [790.3 µs, 1.14 ms] | 8.5 [7.5, 10.9] | – | same |
| explicit_sls | step_hooks | final | 5 | 240 | 213.86 ms [197.49 ms, 273.88 ms] | 891.1 µs [822.9 µs, 1.14 ms] | 8.4 [7.8, 10.8] | 1.008 [0.982, 1.115] | same |
| explicit_sls | step_rest | before | 5 | 120 | 55.54 ms [47.43 ms, 63.36 ms] | 462.9 µs [395.2 µs, 528.0 µs] | 4.4 [3.7, 5.0] | – | same |
| explicit_sls | step_rest | final | 5 | 120 | 53.71 ms [49.68 ms, 63.95 ms] | 447.5 µs [414.0 µs, 533.0 µs] | 4.2 [3.9, 5.1] | 1.009 [0.967, 1.120] | same |
| explicit_sls | internal_forces | before | 5 | 24 | 570.98 ms [552.38 ms, 605.83 ms] | 23.79 ms [23.02 ms, 25.24 ms] | 225.6 [218.2, 239.4] | – | same |
| explicit_sls | internal_forces | final | 5 | 24 | 519.50 ms [506.06 ms, 529.44 ms] | 21.65 ms [21.09 ms, 22.06 ms] | 205.3 [200.0, 209.2] | 0.897 [0.868, 0.940] | same |
| explicit_sls | stresses | before | 5 | 24 | 483.03 ms [463.33 ms, 539.59 ms] | 20.13 ms [19.31 ms, 22.48 ms] | 190.8 [183.1, 213.2] | – | same |
| explicit_sls | stresses | final | 5 | 24 | 405.48 ms [373.87 ms, 437.79 ms] | 16.90 ms [15.58 ms, 18.24 ms] | 160.2 [147.7, 173.0] | 0.817 [0.772, 0.904] | same |
| explicit_sls | energies | before | 5 | 24 | 268.10 ms [258.26 ms, 289.28 ms] | 11.17 ms [10.76 ms, 12.05 ms] | 105.9 [102.0, 114.3] | – | same |
| explicit_sls | energies | final | 5 | 24 | 119.34 ms [109.06 ms, 127.64 ms] | 4.97 ms [4.54 ms, 5.32 ms] | 47.2 [43.1, 50.4] | 0.443 [0.377, 0.474] | same |
| explicit_tet4_ss | mesh | before | 5 | 1 | 3.34 ms [3.08 ms, 3.85 ms] | 3.34 ms [3.08 ms, 3.85 ms] | 20.6 [19.0, 23.8] | – | same |
| explicit_tet4_ss | mesh | final | 5 | 1 | 3.42 ms [3.22 ms, 3.85 ms] | 3.42 ms [3.22 ms, 3.85 ms] | 21.1 [19.9, 23.8] | 1.053 [0.835, 1.091] | same |
| explicit_tet4_ss | init | before | 5 | 1 | 82.50 ms [76.38 ms, 93.72 ms] | 82.50 ms [76.38 ms, 93.72 ms] | 509.3 [471.5, 578.5] | – | same |
| explicit_tet4_ss | init | final | 5 | 1 | 53.12 ms [50.77 ms, 61.25 ms] | 53.12 ms [50.77 ms, 61.25 ms] | 327.9 [313.4, 378.1] | 0.665 [0.556, 0.676] | same |
| explicit_tet4_ss | stable_time_step | before | 5 | 5 | 40.09 ms [34.47 ms, 41.86 ms] | 8.02 ms [6.89 ms, 8.37 ms] | 49.5 [42.6, 51.7] | – | same |
| explicit_tet4_ss | stable_time_step | final | 5 | 5 | 34.13 ms [32.77 ms, 38.37 ms] | 6.83 ms [6.55 ms, 7.67 ms] | 42.1 [40.5, 47.4] | 0.916 [0.825, 1.036] | same |
| explicit_tet4_ss | solve_step | before | 5 | 200 | 2.172 s [2.117 s, 2.340 s] | 10.86 ms [10.58 ms, 11.70 ms] | 67.1 [65.3, 72.2] | – | same |
| explicit_tet4_ss | solve_step | final | 5 | 200 | 2.139 s [1.993 s, 2.280 s] | 10.70 ms [9.96 ms, 11.40 ms] | 66.0 [61.5, 70.4] | 0.958 [0.914, 1.018] | same |
| explicit_tet4_ss | predictor | before | 5 | 200 | 30.4 µs [24.3 µs, 36.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | final | 5 | 200 | 40.8 µs [27.0 µs, 44.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.218 [1.114, 1.628] | same |
| explicit_tet4_ss | residual | before | 5 | 200 | 1.816 s [1.769 s, 1.949 s] | 9.08 ms [8.84 ms, 9.74 ms] | 56.0 [54.6, 60.1] | – | same |
| explicit_tet4_ss | residual | final | 5 | 200 | 1.779 s [1.663 s, 1.884 s] | 8.90 ms [8.32 ms, 9.42 ms] | 54.9 [51.3, 58.1] | 0.955 [0.913, 1.010] | same |
| explicit_tet4_ss | lumped_matrix | before | 5 | 200 | 65.0 µs [61.7 µs, 73.3 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | final | 5 | 200 | 70.0 µs [58.2 µs, 74.3 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.029 [0.846, 1.134] | same |
| explicit_tet4_ss | corrector | before | 5 | 200 | 37.4 µs [30.0 µs, 46.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | final | 5 | 200 | 42.1 µs [38.2 µs, 49.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.123 [0.924, 1.544] | same |
| explicit_tet4_ss | step_hooks | before | 5 | 400 | 241.32 ms [234.15 ms, 266.67 ms] | 603.3 µs [585.4 µs, 666.7 µs] | 3.7 [3.6, 4.1] | – | same |
| explicit_tet4_ss | step_hooks | final | 5 | 400 | 243.93 ms [222.40 ms, 269.85 ms] | 609.8 µs [556.0 µs, 674.6 µs] | 3.8 [3.4, 4.2] | 0.970 [0.915, 1.066] | same |
| explicit_tet4_ss | step_rest | before | 5 | 200 | 115.33 ms [111.07 ms, 124.53 ms] | 576.6 µs [555.3 µs, 622.7 µs] | 3.6 [3.4, 3.8] | – | same |
| explicit_tet4_ss | step_rest | final | 5 | 200 | 116.12 ms [106.81 ms, 126.53 ms] | 580.6 µs [534.0 µs, 632.6 µs] | 3.6 [3.3, 3.9] | 0.986 [0.932, 1.049] | same |
| explicit_tet4_ss | internal_forces | before | 5 | 40 | 377.39 ms [346.30 ms, 397.85 ms] | 9.43 ms [8.66 ms, 9.95 ms] | 58.2 [53.4, 61.4] | – | same |
| explicit_tet4_ss | internal_forces | final | 5 | 40 | 385.57 ms [333.29 ms, 387.61 ms] | 9.64 ms [8.33 ms, 9.69 ms] | 59.5 [51.4, 59.8] | 0.972 [0.883, 1.113] | same |
| explicit_tet4_ss | stresses | before | 5 | 40 | 150.55 ms [148.27 ms, 158.38 ms] | 3.76 ms [3.71 ms, 3.96 ms] | 23.2 [22.9, 24.4] | – | same |
| explicit_tet4_ss | stresses | final | 5 | 40 | 132.12 ms [116.67 ms, 146.83 ms] | 3.30 ms [2.92 ms, 3.67 ms] | 20.4 [18.0, 22.7] | 0.852 [0.781, 0.934] | same |
| explicit_tet4_ss | energies | before | 5 | 40 | 505.17 ms [482.69 ms, 539.29 ms] | 12.63 ms [12.07 ms, 13.48 ms] | 78.0 [74.5, 83.2] | – | same |
| explicit_tet4_ss | energies | final | 5 | 40 | 227.98 ms [220.21 ms, 242.10 ms] | 5.70 ms [5.51 ms, 6.05 ms] | 35.2 [34.0, 37.4] | 0.442 [0.408, 0.502] | same |
| implicit_dynamic | mesh | before | 5 | 1 | 411.7 µs [406.0 µs, 440.1 µs] | 411.7 µs [406.0 µs, 440.1 µs] | 25.0 [24.7, 26.7] | – | same |
| implicit_dynamic | mesh | final | 5 | 1 | 398.4 µs [384.9 µs, 427.6 µs] | 398.4 µs [384.9 µs, 427.6 µs] | 24.2 [23.4, 26.0] | 0.966 [0.875, 1.039] | same |
| implicit_dynamic | init | before | 5 | 1 | 8.92 ms [8.43 ms, 9.80 ms] | 8.92 ms [8.43 ms, 9.80 ms] | 541.6 [512.1, 595.0] | – | same |
| implicit_dynamic | init | final | 5 | 1 | 5.62 ms [5.61 ms, 5.88 ms] | 5.62 ms [5.61 ms, 5.88 ms] | 341.2 [340.5, 357.1] | 0.630 [0.572, 0.685] | same |
| implicit_dynamic | mass_consistent | before | 5 | 1 | 162.34 ms [144.90 ms, 187.63 ms] | 162.34 ms [144.90 ms, 187.63 ms] | 9860.5 [8800.8, 11396.3] | – | same |
| implicit_dynamic | mass_consistent | final | 5 | 1 | 91.58 ms [87.85 ms, 97.24 ms] | 91.58 ms [87.85 ms, 97.24 ms] | 5562.7 [5336.1, 5906.1] | 0.585 [0.468, 0.671] | same |
| implicit_dynamic | solve_step | before | 5 | 60 | 2.236 s [2.122 s, 2.314 s] | 37.27 ms [35.37 ms, 38.57 ms] | 2263.7 [2148.2, 2342.5] | – | same |
| implicit_dynamic | solve_step | final | 5 | 60 | 2.236 s [2.199 s, 2.278 s] | 37.26 ms [36.64 ms, 37.96 ms] | 2263.3 [2225.7, 2305.7] | 1.000 [0.961, 1.036] | same |
| implicit_dynamic | predictor | before | 5 | 60 | 6.4 µs [4.7 µs, 7.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | predictor | final | 5 | 60 | 4.6 µs [3.1 µs, 7.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.714 [0.525, 1.485] | same |
| implicit_dynamic | residual | before | 5 | 180 | 163.78 ms [156.01 ms, 166.03 ms] | 909.9 µs [866.7 µs, 922.4 µs] | 55.3 [52.6, 56.0] | – | same |
| implicit_dynamic | residual | final | 5 | 180 | 146.99 ms [144.74 ms, 149.65 ms] | 816.6 µs [804.1 µs, 831.4 µs] | 49.6 [48.8, 50.5] | 0.893 [0.885, 0.958] | same |
| implicit_dynamic | matrix | before | 5 | 420 | 142.1 µs [129.4 µs, 144.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | matrix | final | 5 | 420 | 130.2 µs [118.4 µs, 136.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.962 [0.819, 1.006] | same |
| implicit_dynamic | corrector | before | 5 | 120 | 20.6 µs [19.0 µs, 35.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | final | 5 | 120 | 18.2 µs [16.8 µs, 28.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.879 [0.507, 1.463] | same |
| implicit_dynamic | step_hooks | before | 5 | 120 | 5.40 ms [4.90 ms, 5.53 ms] | 45.0 µs [40.8 µs, 46.0 µs] | 2.7 [2.5, 2.8] | – | same |
| implicit_dynamic | step_hooks | final | 5 | 120 | 5.14 ms [5.01 ms, 5.39 ms] | 42.9 µs [41.7 µs, 45.0 µs] | 2.6 [2.5, 2.7] | 0.974 [0.927, 1.062] | same |
| implicit_dynamic | step_rest | before | 5 | 60 | 2.064 s [1.961 s, 2.145 s] | 34.41 ms [32.68 ms, 35.74 ms] | 2089.8 [1985.1, 2171.0] | – | same |
| implicit_dynamic | step_rest | final | 5 | 60 | 2.083 s [2.049 s, 2.122 s] | 34.72 ms [34.15 ms, 35.37 ms] | 2109.1 [2074.0, 2148.6] | 1.009 [0.966, 1.045] | same |
| setup_hex8 | mesh | before | 5 | 1 | 1.03 ms [920.0 µs, 1.05 ms] | 1.03 ms [920.0 µs, 1.05 ms] | 74.3 [66.6, 75.9] | – | same |
| setup_hex8 | mesh | final | 5 | 1 | 992.6 µs [940.3 µs, 1.03 ms] | 992.6 µs [940.3 µs, 1.03 ms] | 71.8 [68.0, 74.2] | 0.978 [0.903, 1.079] | same |
| setup_hex8 | init_explicit | before | 5 | 1 | 75.58 ms [69.31 ms, 79.87 ms] | 75.58 ms [69.31 ms, 79.87 ms] | 5467.3 [5013.5, 5777.3] | – | same |
| setup_hex8 | init_explicit | final | 5 | 1 | 40.37 ms [35.62 ms, 42.24 ms] | 40.37 ms [35.62 ms, 42.24 ms] | 2920.3 [2576.6, 3055.2] | 0.529 [0.471, 0.559] | same |
| setup_hex8 | shape_functions | before | 5 | 3 | 169.64 ms [163.91 ms, 177.06 ms] | 56.55 ms [54.64 ms, 59.02 ms] | 4090.6 [3952.4, 4269.3] | – | same |
| setup_hex8 | shape_functions | final | 5 | 3 | 65.44 ms [60.30 ms, 69.15 ms] | 21.81 ms [20.10 ms, 23.05 ms] | 1577.9 [1453.9, 1667.5] | 0.392 [0.341, 0.408] | same |
| setup_hex8 | mass_lumped | before | 5 | 3 | 24.36 ms [22.24 ms, 24.99 ms] | 8.12 ms [7.41 ms, 8.33 ms] | 587.3 [536.2, 602.5] | – | same |
| setup_hex8 | mass_lumped | final | 5 | 3 | 15.47 ms [12.04 ms, 15.97 ms] | 5.16 ms [4.01 ms, 5.32 ms] | 373.0 [290.3, 385.2] | 0.635 [0.541, 0.674] | same |
| setup_hex8 | init_implicit | before | 5 | 1 | 75.44 ms [69.71 ms, 80.93 ms] | 75.44 ms [69.71 ms, 80.93 ms] | 5456.8 [5042.9, 5854.6] | – | same |
| setup_hex8 | init_implicit | final | 5 | 1 | 39.59 ms [35.30 ms, 41.70 ms] | 39.59 ms [35.30 ms, 41.70 ms] | 2863.7 [2553.8, 3016.7] | 0.530 [0.471, 0.559] | same |
| setup_hex8 | mass_consistent | before | 5 | 3 | 4.616 s [4.391 s, 4.898 s] | 1.539 s [1.464 s, 1.633 s] | 111307.3 [105880.5, 118095.3] | – | same |
| setup_hex8 | mass_consistent | final | 5 | 3 | 1.034 s [976.94 ms, 1.294 s] | 344.83 ms [325.65 ms, 431.30 ms] | 24944.3 [23556.5, 31199.1] | 0.224 [0.209, 0.295] | same |
| setup_hex8 | stiffness_assembly | before | 5 | 3 | 2.433 s [2.315 s, 2.628 s] | 810.88 ms [771.75 ms, 876.16 ms] | 58657.5 [55826.7, 63379.4] | – | same |
| setup_hex8 | stiffness_assembly | final | 5 | 3 | 1.713 s [1.610 s, 1.832 s] | 570.87 ms [536.52 ms, 610.58 ms] | 41295.8 [38811.0, 44168.2] | 0.691 [0.612, 0.791] | same |
| setup_tet10 | mesh | before | 5 | 1 | 853.2 µs [779.6 µs, 920.8 µs] | 853.2 µs [779.6 µs, 920.8 µs] | 82.3 [75.2, 88.8] | – | same |
| setup_tet10 | mesh | final | 5 | 1 | 840.9 µs [775.8 µs, 951.6 µs] | 840.9 µs [775.8 µs, 951.6 µs] | 81.1 [74.8, 91.8] | 1.008 [0.909, 1.097] | same |
| setup_tet10 | init_explicit | before | 5 | 1 | 38.76 ms [34.76 ms, 40.37 ms] | 38.76 ms [34.76 ms, 40.37 ms] | 3738.3 [3352.9, 3893.5] | – | same |
| setup_tet10 | init_explicit | final | 5 | 1 | 19.99 ms [18.16 ms, 22.15 ms] | 19.99 ms [18.16 ms, 22.15 ms] | 1928.5 [1751.6, 2136.8] | 0.544 [0.469, 0.571] | same |
| setup_tet10 | shape_functions | before | 5 | 3 | 83.58 ms [79.33 ms, 90.47 ms] | 27.86 ms [26.44 ms, 30.16 ms] | 2687.0 [2550.3, 2908.7] | – | same |
| setup_tet10 | shape_functions | final | 5 | 3 | 30.57 ms [29.07 ms, 34.76 ms] | 10.19 ms [9.69 ms, 11.59 ms] | 982.8 [934.6, 1117.5] | 0.378 [0.348, 0.385] | same |
| setup_tet10 | mass_lumped | before | 5 | 3 | 3.16 ms [2.95 ms, 3.37 ms] | 1.05 ms [983.7 µs, 1.12 ms] | 101.6 [94.9, 108.4] | – | same |
| setup_tet10 | mass_lumped | final | 5 | 3 | 3.13 ms [2.93 ms, 3.40 ms] | 1.04 ms [978.0 µs, 1.13 ms] | 100.6 [94.3, 109.3] | 0.990 [0.948, 1.036] | same |
| setup_tet10 | init_implicit | before | 5 | 1 | 35.50 ms [34.16 ms, 39.69 ms] | 35.50 ms [34.16 ms, 39.69 ms] | 3423.7 [3295.1, 3828.4] | – | same |
| setup_tet10 | init_implicit | final | 5 | 1 | 19.06 ms [18.17 ms, 20.73 ms] | 19.06 ms [18.17 ms, 20.73 ms] | 1838.7 [1752.5, 1999.1] | 0.526 [0.480, 0.558] | same |
| setup_tet10 | mass_consistent | before | 5 | 3 | 3.337 s [3.219 s, 3.774 s] | 1.112 s [1.073 s, 1.258 s] | 107283.6 [103477.7, 121320.1] | – | same |
| setup_tet10 | mass_consistent | final | 5 | 3 | 1.217 s [1.113 s, 1.600 s] | 405.65 ms [370.94 ms, 533.21 ms] | 39124.8 [35777.4, 51428.1] | 0.348 [0.322, 0.486] | same |
| setup_tet10 | stiffness_assembly | before | 5 | 3 | 2.006 s [1.816 s, 2.291 s] | 668.76 ms [605.48 ms, 763.61 ms] | 64502.0 [58399.2, 73650.6] | – | same |
| setup_tet10 | stiffness_assembly | final | 5 | 3 | 1.482 s [1.439 s, 2.055 s] | 494.07 ms [479.62 ms, 684.84 ms] | 47653.8 [46259.4, 66053.1] | 0.816 [0.628, 1.010] | same |
| setup_tet4 | mesh | before | 5 | 1 | 3.54 ms [3.32 ms, 4.01 ms] | 3.54 ms [3.32 ms, 4.01 ms] | 21.8 [20.5, 24.8] | – | same |
| setup_tet4 | mesh | final | 5 | 1 | 3.75 ms [3.26 ms, 4.08 ms] | 3.75 ms [3.26 ms, 4.08 ms] | 23.1 [20.1, 25.2] | 1.055 [0.983, 1.098] | same |
| setup_tet4 | init_explicit | before | 5 | 1 | 79.71 ms [78.97 ms, 98.82 ms] | 79.71 ms [78.97 ms, 98.82 ms] | 492.0 [487.4, 610.0] | – | same |
| setup_tet4 | init_explicit | final | 5 | 1 | 56.49 ms [51.83 ms, 61.83 ms] | 56.49 ms [51.83 ms, 61.83 ms] | 348.7 [319.9, 381.7] | 0.687 [0.589, 0.715] | same |
| setup_tet4 | shape_functions | before | 5 | 3 | 180.66 ms [174.08 ms, 189.95 ms] | 60.22 ms [58.03 ms, 63.32 ms] | 371.7 [358.2, 390.8] | – | same |
| setup_tet4 | shape_functions | final | 5 | 3 | 89.20 ms [82.26 ms, 96.48 ms] | 29.73 ms [27.42 ms, 32.16 ms] | 183.5 [169.3, 198.5] | 0.473 [0.467, 0.517] | same |
| setup_tet4 | mass_lumped | before | 5 | 3 | 62.46 ms [59.77 ms, 65.96 ms] | 20.82 ms [19.92 ms, 21.99 ms] | 128.5 [123.0, 135.7] | – | same |
| setup_tet4 | mass_lumped | final | 5 | 3 | 49.38 ms [46.89 ms, 54.20 ms] | 16.46 ms [15.63 ms, 18.07 ms] | 101.6 [96.5, 111.5] | 0.810 [0.714, 0.837] | same |
| setup_tet4 | init_implicit | before | 5 | 1 | 88.14 ms [82.51 ms, 89.35 ms] | 88.14 ms [82.51 ms, 89.35 ms] | 544.1 [509.3, 551.5] | – | same |
| setup_tet4 | init_implicit | final | 5 | 1 | 51.20 ms [50.54 ms, 59.90 ms] | 51.20 ms [50.54 ms, 59.90 ms] | 316.0 [312.0, 369.8] | 0.614 [0.573, 0.671] | same |
| setup_tet4 | mass_consistent | before | 5 | 3 | 3.791 s [3.381 s, 3.902 s] | 1.264 s [1.127 s, 1.301 s] | 7800.5 [6957.0, 8028.3] | – | same |
| setup_tet4 | mass_consistent | final | 5 | 3 | 2.047 s [1.996 s, 2.190 s] | 682.23 ms [665.25 ms, 729.94 ms] | 4211.3 [4106.5, 4505.8] | 0.571 [0.525, 0.590] | same |
| setup_tet4 | stiffness_assembly | before | 5 | 3 | 2.400 s [2.329 s, 2.658 s] | 800.13 ms [776.39 ms, 885.98 ms] | 4939.1 [4792.5, 5469.0] | – | same |
| setup_tet4 | stiffness_assembly | final | 5 | 3 | 2.239 s [2.141 s, 2.440 s] | 746.33 ms [713.54 ms, 813.26 ms] | 4607.0 [4404.6, 5020.1] | 0.898 [0.828, 1.019] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | 82944 | 15625 | 46875 | 1 | 184dbb6e5, 59e05aa3f | f11c449c593d38ae |
| cohesive_intrinsic_bilinear | 48000 | 46338 | 139014 | 1 | 184dbb6e5, 59e05aa3f | b6a0645b6cf17997 |
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | 184dbb6e5, 59e05aa3f | 37ce0eeb914dd994 |
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | 017c8c9ef7659aa0 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 184dbb6e5, 59e05aa3f | 36137566d8b810bb |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 184dbb6e5, 59e05aa3f | fecb3fb4b46c8b8c |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 184dbb6e5, 59e05aa3f | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 184dbb6e5, 59e05aa3f | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 184dbb6e5, 59e05aa3f | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 184dbb6e5, 59e05aa3f | 562b4d2b6ad29d67 |
