Suite `solid`, size `m`; variants: `base-t1`, `base-t2`, `base-t3`, `base-t4`, `base-t5`, `base-t8`, `head-t1`, `head-t2`, `head-t3`, `head-t4`, `head-t5`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars_non_local | mesh | base-t1 | 5 | 1 | 1.03 ms [1.00 ms, 1.04 ms] | 1.03 ms [1.00 ms, 1.04 ms] | 21.4 [20.9, 21.7] | – | same |
| explicit_mazars_non_local | mesh | base-t2 | 5 | 1 | 1.03 ms [1.00 ms, 1.03 ms] | 1.03 ms [1.00 ms, 1.03 ms] | 21.4 [20.9, 21.5] | 0.995 [0.964, 1.024] | same |
| explicit_mazars_non_local | mesh | base-t3 | 5 | 1 | 1.01 ms [1.00 ms, 1.02 ms] | 1.01 ms [1.00 ms, 1.02 ms] | 21.1 [20.8, 21.2] | 0.987 [0.976, 0.999] | same |
| explicit_mazars_non_local | mesh | base-t4 | 5 | 1 | 1.00 ms [991.4 µs, 1.02 ms] | 1.00 ms [991.4 µs, 1.02 ms] | 20.9 [20.7, 21.2] | 0.975 [0.963, 1.006] | same |
| explicit_mazars_non_local | mesh | base-t5 | 5 | 1 | 1.01 ms [999.2 µs, 1.05 ms] | 1.01 ms [999.2 µs, 1.05 ms] | 21.0 [20.8, 21.9] | 0.997 [0.975, 1.008] | same |
| explicit_mazars_non_local | mesh | base-t8 | 5 | 1 | 1.02 ms [1.01 ms, 1.03 ms] | 1.02 ms [1.01 ms, 1.03 ms] | 21.3 [21.1, 21.4] | 0.998 [0.983, 1.010] | same |
| explicit_mazars_non_local | mesh | head-t1 | 5 | 1 | 986.8 µs [983.0 µs, 1.02 ms] | 986.8 µs [983.0 µs, 1.02 ms] | 20.6 [20.5, 21.2] | 0.971 [0.957, 0.985] | same |
| explicit_mazars_non_local | mesh | head-t2 | 5 | 1 | 1.02 ms [999.9 µs, 1.04 ms] | 1.02 ms [999.9 µs, 1.04 ms] | 21.3 [20.8, 21.6] | 0.998 [0.995, 1.004] | same |
| explicit_mazars_non_local | mesh | head-t3 | 5 | 1 | 1.01 ms [1.00 ms, 1.03 ms] | 1.01 ms [1.00 ms, 1.03 ms] | 21.0 [20.9, 21.4] | 0.993 [0.973, 0.999] | same |
| explicit_mazars_non_local | mesh | head-t4 | 5 | 1 | 1.01 ms [1.00 ms, 1.02 ms] | 1.01 ms [1.00 ms, 1.02 ms] | 21.1 [20.9, 21.3] | 0.986 [0.983, 1.001] | same |
| explicit_mazars_non_local | mesh | head-t5 | 5 | 1 | 1.02 ms [979.1 µs, 1.03 ms] | 1.02 ms [979.1 µs, 1.03 ms] | 21.3 [20.4, 21.5] | 0.991 [0.949, 1.020] | same |
| explicit_mazars_non_local | mesh | head-t8 | 5 | 1 | 1.02 ms [997.9 µs, 1.05 ms] | 1.02 ms [997.9 µs, 1.05 ms] | 21.2 [20.8, 21.8] | 0.991 [0.959, 1.033] | same |
| explicit_mazars_non_local | init | base-t1 | 5 | 1 | 606.16 ms [603.03 ms, 610.05 ms] | 606.16 ms [603.03 ms, 610.05 ms] | 12628.3 [12563.2, 12709.4] | – | same |
| explicit_mazars_non_local | init | base-t2 | 5 | 1 | 605.89 ms [605.07 ms, 613.75 ms] | 605.89 ms [605.07 ms, 613.75 ms] | 12622.6 [12605.6, 12786.4] | 1.003 [0.999, 1.010] | same |
| explicit_mazars_non_local | init | base-t3 | 5 | 1 | 608.39 ms [603.21 ms, 609.55 ms] | 608.39 ms [603.21 ms, 609.55 ms] | 12674.9 [12566.8, 12699.0] | 1.004 [0.995, 1.005] | same |
| explicit_mazars_non_local | init | base-t4 | 5 | 1 | 608.01 ms [601.52 ms, 613.15 ms] | 608.01 ms [601.52 ms, 613.15 ms] | 12666.8 [12531.6, 12773.9] | 1.003 [0.986, 1.011] | same |
| explicit_mazars_non_local | init | base-t5 | 5 | 1 | 604.88 ms [602.84 ms, 609.27 ms] | 604.88 ms [602.84 ms, 609.27 ms] | 12601.6 [12559.1, 12693.1] | 0.998 [0.994, 1.007] | same |
| explicit_mazars_non_local | init | base-t8 | 5 | 1 | 607.44 ms [600.42 ms, 614.66 ms] | 607.44 ms [600.42 ms, 614.66 ms] | 12655.0 [12508.8, 12805.4] | 1.002 [0.992, 1.014] | same |
| explicit_mazars_non_local | init | head-t1 | 5 | 1 | 604.70 ms [602.14 ms, 615.55 ms] | 604.70 ms [602.14 ms, 615.55 ms] | 12597.9 [12544.7, 12823.9] | 0.998 [0.987, 1.015] | same |
| explicit_mazars_non_local | init | head-t2 | 5 | 1 | 605.87 ms [603.57 ms, 608.46 ms] | 605.87 ms [603.57 ms, 608.46 ms] | 12622.4 [12574.5, 12676.2] | 1.003 [0.990, 1.005] | same |
| explicit_mazars_non_local | init | head-t3 | 5 | 1 | 607.83 ms [604.87 ms, 610.14 ms] | 607.83 ms [604.87 ms, 610.14 ms] | 12663.0 [12601.5, 12711.2] | 1.003 [0.996, 1.006] | same |
| explicit_mazars_non_local | init | head-t4 | 5 | 1 | 607.57 ms [602.03 ms, 610.60 ms] | 607.57 ms [602.03 ms, 610.60 ms] | 12657.8 [12542.2, 12720.8] | 1.007 [0.993, 1.008] | same |
| explicit_mazars_non_local | init | head-t5 | 5 | 1 | 607.54 ms [602.75 ms, 610.68 ms] | 607.54 ms [602.75 ms, 610.68 ms] | 12657.0 [12557.4, 12722.5] | 1.002 [0.996, 1.007] | same |
| explicit_mazars_non_local | init | head-t8 | 5 | 1 | 606.79 ms [605.04 ms, 616.68 ms] | 606.79 ms [605.04 ms, 616.68 ms] | 12641.4 [12605.0, 12847.4] | 1.000 [0.999, 1.017] | same |
| explicit_mazars_non_local | stable_time_step | base-t1 | 5 | 5 | 9.67 ms [9.61 ms, 9.81 ms] | 1.93 ms [1.92 ms, 1.96 ms] | 40.3 [40.0, 40.9] | – | same |
| explicit_mazars_non_local | stable_time_step | base-t2 | 5 | 5 | 9.66 ms [9.62 ms, 9.73 ms] | 1.93 ms [1.92 ms, 1.95 ms] | 40.3 [40.1, 40.5] | 1.005 [0.982, 1.007] | same |
| explicit_mazars_non_local | stable_time_step | base-t3 | 5 | 5 | 9.70 ms [9.60 ms, 9.71 ms] | 1.94 ms [1.92 ms, 1.94 ms] | 40.4 [40.0, 40.5] | 1.002 [0.987, 1.011] | same |
| explicit_mazars_non_local | stable_time_step | base-t4 | 5 | 5 | 9.64 ms [9.62 ms, 9.68 ms] | 1.93 ms [1.92 ms, 1.94 ms] | 40.2 [40.1, 40.3] | 0.998 [0.983, 1.006] | same |
| explicit_mazars_non_local | stable_time_step | base-t5 | 5 | 5 | 9.65 ms [9.59 ms, 9.70 ms] | 1.93 ms [1.92 ms, 1.94 ms] | 40.2 [39.9, 40.4] | 0.994 [0.984, 1.008] | same |
| explicit_mazars_non_local | stable_time_step | base-t8 | 5 | 5 | 9.62 ms [9.60 ms, 9.71 ms] | 1.92 ms [1.92 ms, 1.94 ms] | 40.1 [40.0, 40.5] | 0.996 [0.991, 0.999] | same |
| explicit_mazars_non_local | stable_time_step | head-t1 | 5 | 5 | 9.60 ms [9.59 ms, 9.61 ms] | 1.92 ms [1.92 ms, 1.92 ms] | 40.0 [39.9, 40.0] | 0.991 [0.980, 0.999] | same |
| explicit_mazars_non_local | stable_time_step | head-t2 | 5 | 5 | 9.64 ms [9.58 ms, 9.73 ms] | 1.93 ms [1.92 ms, 1.95 ms] | 40.2 [39.9, 40.6] | 0.997 [0.993, 1.002] | same |
| explicit_mazars_non_local | stable_time_step | head-t3 | 5 | 5 | 9.62 ms [9.58 ms, 9.66 ms] | 1.92 ms [1.92 ms, 1.93 ms] | 40.1 [39.9, 40.2] | 0.994 [0.985, 1.001] | same |
| explicit_mazars_non_local | stable_time_step | head-t4 | 5 | 5 | 9.65 ms [9.58 ms, 9.72 ms] | 1.93 ms [1.92 ms, 1.94 ms] | 40.2 [39.9, 40.5] | 0.998 [0.986, 1.012] | same |
| explicit_mazars_non_local | stable_time_step | head-t5 | 5 | 5 | 9.66 ms [9.57 ms, 9.69 ms] | 1.93 ms [1.91 ms, 1.94 ms] | 40.3 [39.9, 40.4] | 0.996 [0.987, 1.004] | same |
| explicit_mazars_non_local | stable_time_step | head-t8 | 5 | 5 | 9.72 ms [9.61 ms, 9.75 ms] | 1.94 ms [1.92 ms, 1.95 ms] | 40.5 [40.0, 40.6] | 0.998 [0.991, 1.015] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 1.671 s [1.659 s, 1.673 s] | 41.76 ms [41.48 ms, 41.82 ms] | 870.1 [864.1, 871.3] | – | same |
| explicit_mazars_non_local | solve_step | base-t2 | 5 | 40 | 1.479 s [1.474 s, 1.481 s] | 36.97 ms [36.84 ms, 37.01 ms] | 770.2 [767.6, 771.1] | 0.885 [0.881, 0.892] | same |
| explicit_mazars_non_local | solve_step | base-t3 | 5 | 40 | 1.428 s [1.420 s, 1.435 s] | 35.69 ms [35.50 ms, 35.87 ms] | 743.6 [739.7, 747.4] | 0.858 [0.850, 0.860] | same |
| explicit_mazars_non_local | solve_step | base-t4 | 5 | 40 | 1.395 s [1.391 s, 1.399 s] | 34.88 ms [34.77 ms, 34.99 ms] | 726.6 [724.4, 728.9] | 0.834 [0.833, 0.844] | same |
| explicit_mazars_non_local | solve_step | base-t5 | 5 | 40 | 1.385 s [1.380 s, 1.395 s] | 34.62 ms [34.49 ms, 34.87 ms] | 721.2 [718.5, 726.4] | 0.830 [0.826, 0.835] | same |
| explicit_mazars_non_local | solve_step | base-t8 | 5 | 40 | 1.361 s [1.350 s, 1.381 s] | 34.03 ms [33.74 ms, 34.51 ms] | 708.9 [703.0, 719.0] | 0.815 [0.807, 0.826] | same |
| explicit_mazars_non_local | solve_step | head-t1 | 5 | 40 | 1.647 s [1.642 s, 1.648 s] | 41.17 ms [41.05 ms, 41.20 ms] | 857.6 [855.3, 858.3] | 0.986 [0.984, 0.990] | same |
| explicit_mazars_non_local | solve_step | head-t2 | 5 | 40 | 1.203 s [1.200 s, 1.206 s] | 30.08 ms [30.00 ms, 30.14 ms] | 626.7 [624.9, 627.9] | 0.721 [0.719, 0.723] | same |
| explicit_mazars_non_local | solve_step | head-t3 | 5 | 40 | 1.067 s [1.061 s, 1.076 s] | 26.67 ms [26.52 ms, 26.90 ms] | 555.6 [552.5, 560.5] | 0.641 [0.635, 0.644] | same |
| explicit_mazars_non_local | solve_step | head-t4 | 5 | 40 | 1.000 s [991.97 ms, 1.002 s] | 25.01 ms [24.80 ms, 25.06 ms] | 521.0 [516.7, 522.0] | 0.599 [0.594, 0.600] | same |
| explicit_mazars_non_local | solve_step | head-t5 | 5 | 40 | 900.14 ms [876.63 ms, 909.13 ms] | 22.50 ms [21.92 ms, 22.73 ms] | 468.8 [456.6, 473.5] | 0.539 [0.525, 0.548] | same |
| explicit_mazars_non_local | solve_step | head-t8 | 5 | 40 | 618.89 ms [609.66 ms, 630.13 ms] | 15.47 ms [15.24 ms, 15.75 ms] | 322.3 [317.5, 328.2] | 0.371 [0.364, 0.377] | same |
| explicit_mazars_non_local | predictor | base-t1 | 5 | 40 | 4.6 µs [3.1 µs, 7.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | base-t2 | 5 | 40 | 4.3 µs [3.1 µs, 5.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.132 [0.466, 1.382] | same |
| explicit_mazars_non_local | predictor | base-t3 | 5 | 40 | 5.7 µs [4.9 µs, 7.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.321 [0.884, 1.705] | same |
| explicit_mazars_non_local | predictor | base-t4 | 5 | 40 | 6.1 µs [4.1 µs, 7.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.141 [0.846, 2.267] | same |
| explicit_mazars_non_local | predictor | base-t5 | 5 | 40 | 5.6 µs [3.3 µs, 8.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.872 [0.716, 2.555] | same |
| explicit_mazars_non_local | predictor | base-t8 | 5 | 40 | 5.6 µs [4.9 µs, 6.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.261 [0.677, 1.806] | same |
| explicit_mazars_non_local | predictor | head-t1 | 5 | 40 | 2.3 µs [1.4 µs, 3.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.413 [0.279, 0.981] | same |
| explicit_mazars_non_local | predictor | head-t2 | 5 | 40 | 2.9 µs [2.5 µs, 3.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.619 [0.362, 0.927] | same |
| explicit_mazars_non_local | predictor | head-t3 | 5 | 40 | 3.0 µs [2.0 µs, 5.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.747 [0.279, 1.210] | same |
| explicit_mazars_non_local | predictor | head-t4 | 5 | 40 | 1.9 µs [1.3 µs, 4.8 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.625 [0.213, 1.059] | same |
| explicit_mazars_non_local | predictor | head-t5 | 5 | 40 | 1.4 µs [1.3 µs, 2.1 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.407 [0.186, 0.464] | same |
| explicit_mazars_non_local | predictor | head-t8 | 5 | 40 | 2.4 µs [1.1 µs, 2.7 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.532 [0.149, 0.843] | same |
| explicit_mazars_non_local | residual | base-t1 | 5 | 40 | 1.633 s [1.622 s, 1.636 s] | 40.83 ms [40.54 ms, 40.89 ms] | 850.6 [844.7, 852.0] | – | same |
| explicit_mazars_non_local | residual | base-t2 | 5 | 40 | 1.450 s [1.445 s, 1.452 s] | 36.26 ms [36.13 ms, 36.31 ms] | 755.4 [752.8, 756.4] | 0.888 [0.884, 0.895] | same |
| explicit_mazars_non_local | residual | base-t3 | 5 | 40 | 1.402 s [1.394 s, 1.409 s] | 35.05 ms [34.86 ms, 35.21 ms] | 730.3 [726.2, 733.6] | 0.862 [0.854, 0.864] | same |
| explicit_mazars_non_local | residual | base-t4 | 5 | 40 | 1.368 s [1.365 s, 1.373 s] | 34.21 ms [34.12 ms, 34.33 ms] | 712.7 [710.9, 715.1] | 0.837 [0.836, 0.847] | same |
| explicit_mazars_non_local | residual | base-t5 | 5 | 40 | 1.358 s [1.353 s, 1.368 s] | 33.95 ms [33.83 ms, 34.19 ms] | 707.3 [704.9, 712.3] | 0.832 [0.829, 0.837] | same |
| explicit_mazars_non_local | residual | base-t8 | 5 | 40 | 1.335 s [1.324 s, 1.355 s] | 33.38 ms [33.11 ms, 33.87 ms] | 695.5 [689.8, 705.7] | 0.818 [0.810, 0.830] | same |
| explicit_mazars_non_local | residual | head-t1 | 5 | 40 | 1.609 s [1.605 s, 1.610 s] | 40.22 ms [40.11 ms, 40.25 ms] | 837.9 [835.7, 838.6] | 0.985 [0.984, 0.989] | same |
| explicit_mazars_non_local | residual | head-t2 | 5 | 40 | 1.180 s [1.177 s, 1.183 s] | 29.51 ms [29.43 ms, 29.57 ms] | 614.8 [613.1, 616.0] | 0.723 [0.722, 0.726] | same |
| explicit_mazars_non_local | residual | head-t3 | 5 | 40 | 1.049 s [1.043 s, 1.058 s] | 26.21 ms [26.07 ms, 26.45 ms] | 546.1 [543.2, 551.1] | 0.645 [0.639, 0.648] | same |
| explicit_mazars_non_local | residual | head-t4 | 5 | 40 | 983.47 ms [975.28 ms, 985.51 ms] | 24.59 ms [24.38 ms, 24.64 ms] | 512.2 [508.0, 513.3] | 0.602 [0.597, 0.603] | same |
| explicit_mazars_non_local | residual | head-t5 | 5 | 40 | 883.56 ms [860.04 ms, 892.52 ms] | 22.09 ms [21.50 ms, 22.31 ms] | 460.2 [447.9, 464.9] | 0.541 [0.527, 0.550] | same |
| explicit_mazars_non_local | residual | head-t8 | 5 | 40 | 603.80 ms [594.50 ms, 614.81 ms] | 15.09 ms [14.86 ms, 15.37 ms] | 314.5 [309.6, 320.2] | 0.370 [0.363, 0.376] | same |
| explicit_mazars_non_local | lumped_matrix | base-t1 | 5 | 40 | 9.6 µs [8.1 µs, 10.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | base-t2 | 5 | 40 | 10.2 µs [9.6 µs, 10.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.044 [0.999, 1.249] | same |
| explicit_mazars_non_local | lumped_matrix | base-t3 | 5 | 40 | 10.2 µs [9.7 µs, 10.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.132 [0.975, 1.295] | same |
| explicit_mazars_non_local | lumped_matrix | base-t4 | 5 | 40 | 9.8 µs [8.7 µs, 19.2 µs] | 0.2 µs [0.2 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.078 [0.936, 1.922] | same |
| explicit_mazars_non_local | lumped_matrix | base-t5 | 5 | 40 | 10.6 µs [9.5 µs, 17.2 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.178 [0.953, 1.721] | same |
| explicit_mazars_non_local | lumped_matrix | base-t8 | 5 | 40 | 9.8 µs [9.5 µs, 11.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.014 [0.964, 1.368] | same |
| explicit_mazars_non_local | lumped_matrix | head-t1 | 5 | 40 | 8.5 µs [7.6 µs, 9.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.883 [0.760, 1.051] | same |
| explicit_mazars_non_local | lumped_matrix | head-t2 | 5 | 40 | 11.3 µs [10.8 µs, 12.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.236 [1.175, 1.332] | same |
| explicit_mazars_non_local | lumped_matrix | head-t3 | 5 | 40 | 12.5 µs [11.2 µs, 14.7 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.303 [1.207, 1.633] | same |
| explicit_mazars_non_local | lumped_matrix | head-t4 | 5 | 40 | 11.4 µs [11.0 µs, 12.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.144 [1.142, 1.504] | same |
| explicit_mazars_non_local | lumped_matrix | head-t5 | 5 | 40 | 11.3 µs [10.0 µs, 12.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.224 [1.155, 1.245] | same |
| explicit_mazars_non_local | lumped_matrix | head-t8 | 5 | 40 | 11.0 µs [10.3 µs, 12.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.117 [1.099, 1.339] | same |
| explicit_mazars_non_local | corrector | base-t1 | 5 | 40 | 6.1 µs [5.2 µs, 6.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | base-t2 | 5 | 40 | 6.9 µs [6.2 µs, 9.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.076 [1.018, 1.821] | same |
| explicit_mazars_non_local | corrector | base-t3 | 5 | 40 | 6.5 µs [5.7 µs, 7.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.071 [0.901, 1.418] | same |
| explicit_mazars_non_local | corrector | base-t4 | 5 | 40 | 6.4 µs [6.1 µs, 7.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.141 [0.963, 1.184] | same |
| explicit_mazars_non_local | corrector | base-t5 | 5 | 40 | 6.7 µs [6.5 µs, 7.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.174 [0.980, 1.289] | same |
| explicit_mazars_non_local | corrector | base-t8 | 5 | 40 | 6.3 µs [6.2 µs, 6.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.068 [1.001, 1.215] | same |
| explicit_mazars_non_local | corrector | head-t1 | 5 | 40 | 6.3 µs [6.1 µs, 9.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.117 [1.022, 1.400] | same |
| explicit_mazars_non_local | corrector | head-t2 | 5 | 40 | 6.2 µs [6.0 µs, 7.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.117 [0.905, 1.288] | same |
| explicit_mazars_non_local | corrector | head-t3 | 5 | 40 | 6.1 µs [5.3 µs, 6.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.017 [0.838, 1.170] | same |
| explicit_mazars_non_local | corrector | head-t4 | 5 | 40 | 6.6 µs [6.1 µs, 7.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.179 [1.048, 1.194] | same |
| explicit_mazars_non_local | corrector | head-t5 | 5 | 40 | 6.9 µs [6.8 µs, 13.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.297 [1.074, 2.050] | same |
| explicit_mazars_non_local | corrector | head-t8 | 5 | 40 | 6.3 µs [5.6 µs, 6.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.008 [0.877, 1.275] | same |
| explicit_mazars_non_local | step_hooks | base-t1 | 5 | 80 | 29.65 ms [29.49 ms, 29.79 ms] | 370.7 µs [368.6 µs, 372.4 µs] | 7.7 [7.7, 7.8] | – | same |
| explicit_mazars_non_local | step_hooks | base-t2 | 5 | 80 | 20.55 ms [20.48 ms, 20.61 ms] | 256.8 µs [256.0 µs, 257.6 µs] | 5.4 [5.3, 5.4] | 0.693 [0.690, 0.696] | same |
| explicit_mazars_non_local | step_hooks | base-t3 | 5 | 80 | 18.10 ms [17.85 ms, 18.70 ms] | 226.3 µs [223.2 µs, 233.8 µs] | 4.7 [4.6, 4.9] | 0.608 [0.600, 0.633] | same |
| explicit_mazars_non_local | step_hooks | base-t4 | 5 | 80 | 18.80 ms [18.11 ms, 19.51 ms] | 235.0 µs [226.4 µs, 243.9 µs] | 4.9 [4.7, 5.1] | 0.632 [0.608, 0.662] | same |
| explicit_mazars_non_local | step_hooks | base-t5 | 5 | 80 | 18.62 ms [18.38 ms, 19.06 ms] | 232.7 µs [229.7 µs, 238.2 µs] | 4.8 [4.8, 5.0] | 0.626 [0.623, 0.645] | same |
| explicit_mazars_non_local | step_hooks | base-t8 | 5 | 80 | 17.55 ms [17.45 ms, 17.83 ms] | 219.4 µs [218.1 µs, 222.9 µs] | 4.6 [4.5, 4.6] | 0.591 [0.588, 0.605] | same |
| explicit_mazars_non_local | step_hooks | head-t1 | 5 | 80 | 29.85 ms [29.78 ms, 29.89 ms] | 373.2 µs [372.2 µs, 373.6 µs] | 7.8 [7.8, 7.8] | 1.005 [1.002, 1.014] | same |
| explicit_mazars_non_local | step_hooks | head-t2 | 5 | 80 | 16.43 ms [16.36 ms, 16.73 ms] | 205.3 µs [204.5 µs, 209.2 µs] | 4.3 [4.3, 4.4] | 0.556 [0.549, 0.563] | same |
| explicit_mazars_non_local | step_hooks | head-t3 | 5 | 80 | 12.43 ms [12.38 ms, 12.61 ms] | 155.4 µs [154.7 µs, 157.7 µs] | 3.2 [3.2, 3.3] | 0.419 [0.415, 0.427] | same |
| explicit_mazars_non_local | step_hooks | head-t4 | 5 | 80 | 11.51 ms [11.45 ms, 11.52 ms] | 143.9 µs [143.1 µs, 144.0 µs] | 3.0 [3.0, 3.0] | 0.387 [0.387, 0.389] | same |
| explicit_mazars_non_local | step_hooks | head-t5 | 5 | 80 | 11.39 ms [11.37 ms, 11.44 ms] | 142.3 µs [142.1 µs, 143.0 µs] | 3.0 [3.0, 3.0] | 0.383 [0.383, 0.387] | same |
| explicit_mazars_non_local | step_hooks | head-t8 | 5 | 80 | 10.36 ms [10.32 ms, 10.57 ms] | 129.5 µs [128.9 µs, 132.1 µs] | 2.7 [2.7, 2.8] | 0.350 [0.347, 0.358] | same |
| explicit_mazars_non_local | step_rest | base-t1 | 5 | 40 | 7.47 ms [7.34 ms, 7.51 ms] | 186.8 µs [183.5 µs, 187.8 µs] | 3.9 [3.8, 3.9] | – | same |
| explicit_mazars_non_local | step_rest | base-t2 | 5 | 40 | 7.65 ms [7.52 ms, 7.67 ms] | 191.3 µs [187.9 µs, 191.7 µs] | 4.0 [3.9, 4.0] | 1.023 [1.006, 1.038] | same |
| explicit_mazars_non_local | step_rest | base-t3 | 5 | 40 | 7.62 ms [7.50 ms, 7.82 ms] | 190.5 µs [187.4 µs, 195.4 µs] | 4.0 [3.9, 4.1] | 1.028 [1.003, 1.046] | same |
| explicit_mazars_non_local | step_rest | base-t4 | 5 | 40 | 7.68 ms [7.65 ms, 7.77 ms] | 192.1 µs [191.3 µs, 194.2 µs] | 4.0 [4.0, 4.0] | 1.027 [1.022, 1.059] | same |
| explicit_mazars_non_local | step_rest | base-t5 | 5 | 40 | 7.75 ms [7.64 ms, 7.89 ms] | 193.8 µs [191.1 µs, 197.2 µs] | 4.0 [4.0, 4.1] | 1.037 [1.022, 1.067] | same |
| explicit_mazars_non_local | step_rest | base-t8 | 5 | 40 | 7.78 ms [7.66 ms, 7.89 ms] | 194.6 µs [191.6 µs, 197.3 µs] | 4.1 [4.0, 4.1] | 1.044 [1.034, 1.055] | same |
| explicit_mazars_non_local | step_rest | head-t1 | 5 | 40 | 7.81 ms [7.72 ms, 7.85 ms] | 195.2 µs [192.9 µs, 196.2 µs] | 4.1 [4.0, 4.1] | 1.042 [1.033, 1.069] | same |
| explicit_mazars_non_local | step_rest | head-t2 | 5 | 40 | 6.12 ms [5.98 ms, 6.25 ms] | 153.1 µs [149.4 µs, 156.3 µs] | 3.2 [3.1, 3.3] | 0.818 [0.812, 0.837] | same |
| explicit_mazars_non_local | step_rest | head-t3 | 5 | 40 | 5.36 ms [5.34 ms, 5.49 ms] | 134.0 µs [133.6 µs, 137.2 µs] | 2.8 [2.8, 2.9] | 0.717 [0.716, 0.735] | same |
| explicit_mazars_non_local | step_rest | head-t4 | 5 | 40 | 5.11 ms [5.07 ms, 5.21 ms] | 127.8 µs [126.9 µs, 130.2 µs] | 2.7 [2.6, 2.7] | 0.690 [0.679, 0.697] | same |
| explicit_mazars_non_local | step_rest | head-t5 | 5 | 40 | 5.11 ms [5.07 ms, 5.13 ms] | 127.7 µs [126.8 µs, 128.1 µs] | 2.7 [2.6, 2.7] | 0.685 [0.678, 0.696] | same |
| explicit_mazars_non_local | step_rest | head-t8 | 5 | 40 | 4.78 ms [4.67 ms, 4.80 ms] | 119.4 µs [116.8 µs, 120.1 µs] | 2.5 [2.4, 2.5] | 0.639 [0.625, 0.646] | same |
| explicit_mazars_non_local | internal_forces | base-t1 | 5 | 8 | 336.21 ms [332.68 ms, 338.01 ms] | 42.03 ms [41.59 ms, 42.25 ms] | 875.5 [866.4, 880.2] | – | same |
| explicit_mazars_non_local | internal_forces | base-t2 | 5 | 8 | 295.58 ms [294.52 ms, 297.43 ms] | 36.95 ms [36.81 ms, 37.18 ms] | 769.8 [767.0, 774.5] | 0.877 [0.875, 0.894] | same |
| explicit_mazars_non_local | internal_forces | base-t3 | 5 | 8 | 285.74 ms [284.29 ms, 287.83 ms] | 35.72 ms [35.54 ms, 35.98 ms] | 744.1 [740.3, 749.6] | 0.848 [0.845, 0.862] | same |
| explicit_mazars_non_local | internal_forces | base-t4 | 5 | 8 | 278.89 ms [277.71 ms, 279.96 ms] | 34.86 ms [34.71 ms, 34.99 ms] | 726.3 [723.2, 729.1] | 0.831 [0.824, 0.842] | same |
| explicit_mazars_non_local | internal_forces | base-t5 | 5 | 8 | 276.36 ms [275.02 ms, 279.34 ms] | 34.54 ms [34.38 ms, 34.92 ms] | 719.7 [716.2, 727.4] | 0.827 [0.815, 0.833] | same |
| explicit_mazars_non_local | internal_forces | base-t8 | 5 | 8 | 272.10 ms [269.59 ms, 274.71 ms] | 34.01 ms [33.70 ms, 34.34 ms] | 708.6 [702.1, 715.4] | 0.809 [0.798, 0.819] | same |
| explicit_mazars_non_local | internal_forces | head-t1 | 5 | 8 | 331.28 ms [331.03 ms, 334.02 ms] | 41.41 ms [41.38 ms, 41.75 ms] | 862.7 [862.0, 869.9] | 0.987 [0.980, 0.996] | same |
| explicit_mazars_non_local | internal_forces | head-t2 | 5 | 8 | 239.82 ms [239.39 ms, 240.66 ms] | 29.98 ms [29.92 ms, 30.08 ms] | 624.5 [623.4, 626.7] | 0.713 [0.712, 0.720] | same |
| explicit_mazars_non_local | internal_forces | head-t3 | 5 | 8 | 212.18 ms [211.39 ms, 214.67 ms] | 26.52 ms [26.42 ms, 26.83 ms] | 552.5 [550.5, 559.0] | 0.638 [0.628, 0.638] | same |
| explicit_mazars_non_local | internal_forces | head-t4 | 5 | 8 | 197.97 ms [197.10 ms, 199.63 ms] | 24.75 ms [24.64 ms, 24.95 ms] | 515.6 [513.3, 519.9] | 0.592 [0.586, 0.595] | same |
| explicit_mazars_non_local | internal_forces | head-t5 | 5 | 8 | 174.94 ms [172.62 ms, 177.56 ms] | 21.87 ms [21.58 ms, 22.19 ms] | 455.6 [449.5, 462.4] | 0.520 [0.513, 0.530] | same |
| explicit_mazars_non_local | internal_forces | head-t8 | 5 | 8 | 120.34 ms [118.74 ms, 123.17 ms] | 15.04 ms [14.84 ms, 15.40 ms] | 313.4 [309.2, 320.8] | 0.359 [0.353, 0.367] | same |
| explicit_mazars_non_local | stresses | base-t1 | 5 | 8 | 75.42 ms [75.16 ms, 75.73 ms] | 9.43 ms [9.40 ms, 9.47 ms] | 196.4 [195.7, 197.2] | – | same |
| explicit_mazars_non_local | stresses | base-t2 | 5 | 8 | 73.65 ms [73.64 ms, 74.20 ms] | 9.21 ms [9.21 ms, 9.28 ms] | 191.8 [191.8, 193.2] | 0.977 [0.976, 0.982] | same |
| explicit_mazars_non_local | stresses | base-t3 | 5 | 8 | 73.03 ms [72.82 ms, 73.12 ms] | 9.13 ms [9.10 ms, 9.14 ms] | 190.2 [189.6, 190.4] | 0.966 [0.965, 0.972] | same |
| explicit_mazars_non_local | stresses | base-t4 | 5 | 8 | 73.33 ms [72.85 ms, 73.42 ms] | 9.17 ms [9.11 ms, 9.18 ms] | 191.0 [189.7, 191.2] | 0.971 [0.963, 0.976] | same |
| explicit_mazars_non_local | stresses | base-t5 | 5 | 8 | 73.56 ms [73.01 ms, 73.71 ms] | 9.20 ms [9.13 ms, 9.21 ms] | 191.6 [190.1, 191.9] | 0.973 [0.969, 0.978] | same |
| explicit_mazars_non_local | stresses | base-t8 | 5 | 8 | 73.37 ms [73.13 ms, 73.46 ms] | 9.17 ms [9.14 ms, 9.18 ms] | 191.1 [190.4, 191.3] | 0.971 [0.969, 0.977] | same |
| explicit_mazars_non_local | stresses | head-t1 | 5 | 8 | 75.63 ms [75.17 ms, 75.88 ms] | 9.45 ms [9.40 ms, 9.48 ms] | 196.9 [195.8, 197.6] | 1.002 [0.998, 1.006] | same |
| explicit_mazars_non_local | stresses | head-t2 | 5 | 8 | 73.81 ms [73.54 ms, 73.93 ms] | 9.23 ms [9.19 ms, 9.24 ms] | 192.2 [191.5, 192.5] | 0.978 [0.971, 0.983] | same |
| explicit_mazars_non_local | stresses | head-t3 | 5 | 8 | 73.10 ms [73.03 ms, 73.52 ms] | 9.14 ms [9.13 ms, 9.19 ms] | 190.4 [190.2, 191.5] | 0.969 [0.967, 0.978] | same |
| explicit_mazars_non_local | stresses | head-t4 | 5 | 8 | 72.89 ms [72.81 ms, 73.05 ms] | 9.11 ms [9.10 ms, 9.13 ms] | 189.8 [189.6, 190.2] | 0.965 [0.964, 0.972] | same |
| explicit_mazars_non_local | stresses | head-t5 | 5 | 8 | 72.95 ms [72.75 ms, 73.30 ms] | 9.12 ms [9.09 ms, 9.16 ms] | 190.0 [189.4, 190.9] | 0.968 [0.961, 0.970] | same |
| explicit_mazars_non_local | stresses | head-t8 | 5 | 8 | 72.55 ms [72.23 ms, 73.06 ms] | 9.07 ms [9.03 ms, 9.13 ms] | 188.9 [188.1, 190.3] | 0.961 [0.958, 0.967] | same |
| explicit_mazars_non_local | energies | base-t1 | 5 | 8 | 5.50 ms [5.47 ms, 5.55 ms] | 688.0 µs [684.0 µs, 693.3 µs] | 14.3 [14.2, 14.4] | – | same |
| explicit_mazars_non_local | energies | base-t2 | 5 | 8 | 5.53 ms [5.50 ms, 5.54 ms] | 691.2 µs [688.0 µs, 691.9 µs] | 14.4 [14.3, 14.4] | 1.004 [0.992, 1.011] | same |
| explicit_mazars_non_local | energies | base-t3 | 5 | 8 | 5.50 ms [5.47 ms, 5.51 ms] | 687.5 µs [683.3 µs, 688.8 µs] | 14.3 [14.2, 14.3] | 1.000 [0.986, 1.007] | same |
| explicit_mazars_non_local | energies | base-t4 | 5 | 8 | 5.52 ms [5.50 ms, 5.66 ms] | 689.5 µs [687.5 µs, 707.2 µs] | 14.4 [14.3, 14.7] | 1.002 [0.994, 1.020] | same |
| explicit_mazars_non_local | energies | base-t5 | 5 | 8 | 5.50 ms [5.49 ms, 5.53 ms] | 687.1 µs [686.4 µs, 691.1 µs] | 14.3 [14.3, 14.4] | 1.001 [0.991, 1.004] | same |
| explicit_mazars_non_local | energies | base-t8 | 5 | 8 | 5.55 ms [5.52 ms, 5.65 ms] | 694.1 µs [690.1 µs, 706.5 µs] | 14.5 [14.4, 14.7] | 1.010 [0.997, 1.022] | same |
| explicit_mazars_non_local | energies | head-t1 | 5 | 8 | 5.64 ms [5.60 ms, 5.70 ms] | 705.5 µs [700.3 µs, 712.5 µs] | 14.7 [14.6, 14.8] | 1.024 [1.018, 1.030] | same |
| explicit_mazars_non_local | energies | head-t2 | 5 | 8 | 4.70 ms [4.69 ms, 4.72 ms] | 587.8 µs [586.3 µs, 589.7 µs] | 12.2 [12.2, 12.3] | 0.854 [0.846, 0.862] | same |
| explicit_mazars_non_local | energies | head-t3 | 5 | 8 | 4.52 ms [4.52 ms, 4.60 ms] | 565.4 µs [564.6 µs, 575.5 µs] | 11.8 [11.8, 12.0] | 0.822 [0.814, 0.837] | same |
| explicit_mazars_non_local | energies | head-t4 | 5 | 8 | 4.54 ms [4.51 ms, 4.55 ms] | 567.3 µs [563.8 µs, 568.4 µs] | 11.8 [11.7, 11.8] | 0.825 [0.813, 0.827] | same |
| explicit_mazars_non_local | energies | head-t5 | 5 | 8 | 4.61 ms [4.57 ms, 4.64 ms] | 576.7 µs [571.5 µs, 579.9 µs] | 12.0 [11.9, 12.1] | 0.839 [0.826, 0.846] | same |
| explicit_mazars_non_local | energies | head-t8 | 5 | 8 | 4.35 ms [4.32 ms, 4.39 ms] | 543.7 µs [540.0 µs, 549.1 µs] | 11.3 [11.2, 11.4] | 0.789 [0.784, 0.796] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t1 | 5 | 2 | 1.178 s [1.169 s, 1.181 s] | 589.09 ms [584.36 ms, 590.45 ms] | 12272.8 [12174.1, 12301.1] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t2 | 5 | 2 | 1.177 s [1.173 s, 1.183 s] | 588.62 ms [586.34 ms, 591.61 ms] | 12262.9 [12215.4, 12325.1] | 0.999 [0.995, 1.012] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t3 | 5 | 2 | 1.188 s [1.178 s, 1.200 s] | 594.23 ms [588.87 ms, 599.93 ms] | 12379.9 [12268.1, 12498.6] | 1.007 [1.005, 1.027] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t4 | 5 | 2 | 1.179 s [1.173 s, 1.186 s] | 589.67 ms [586.65 ms, 593.10 ms] | 12284.9 [12221.8, 12356.2] | 1.004 [0.999, 1.007] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t5 | 5 | 2 | 1.178 s [1.172 s, 1.186 s] | 589.20 ms [585.97 ms, 593.24 ms] | 12275.1 [12207.8, 12359.3] | 1.005 [0.995, 1.009] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t8 | 5 | 2 | 1.186 s [1.177 s, 1.188 s] | 593.21 ms [588.44 ms, 593.80 ms] | 12358.6 [12259.2, 12370.8] | 1.008 [0.997, 1.014] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t1 | 5 | 2 | 1.189 s [1.179 s, 1.195 s] | 594.50 ms [589.53 ms, 597.44 ms] | 12385.3 [12281.9, 12446.6] | 1.012 [1.001, 1.017] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t2 | 5 | 2 | 1.179 s [1.177 s, 1.192 s] | 589.75 ms [588.71 ms, 595.83 ms] | 12286.4 [12264.7, 12413.0] | 1.005 [0.998, 1.009] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t3 | 5 | 2 | 1.179 s [1.176 s, 1.182 s] | 589.42 ms [588.24 ms, 591.07 ms] | 12279.5 [12254.9, 12314.0] | 1.001 [0.996, 1.009] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t4 | 5 | 2 | 1.185 s [1.178 s, 1.202 s] | 592.72 ms [589.17 ms, 601.11 ms] | 12348.4 [12274.4, 12523.1] | 1.007 [1.003, 1.018] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t5 | 5 | 2 | 1.188 s [1.177 s, 1.196 s] | 593.84 ms [588.61 ms, 598.00 ms] | 12371.6 [12262.7, 12458.4] | 1.007 [0.997, 1.023] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t8 | 5 | 2 | 1.182 s [1.178 s, 1.194 s] | 591.11 ms [589.19 ms, 597.20 ms] | 12314.7 [12274.7, 12441.7] | 1.006 [1.003, 1.011] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 7365a08f4, e8061c922 | 849f4cd05e9ad628 |
