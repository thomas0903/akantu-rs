Suite `solid`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 387.3 µs [368.6 µs, 408.1 µs] | 387.3 µs [368.6 µs, 408.1 µs] | 107.6 [102.4, 113.4] | – | same |
| cohesive_intrinsic_implicit | mesh | t1 | 5 | 1 | 379.2 µs [363.7 µs, 381.0 µs] | 379.2 µs [363.7 µs, 381.0 µs] | 105.3 [101.0, 105.8] | 0.970 [0.914, 1.033] | same |
| cohesive_intrinsic_implicit | mesh | t2 | 5 | 1 | 352.7 µs [343.0 µs, 381.5 µs] | 352.7 µs [343.0 µs, 381.5 µs] | 98.0 [95.3, 106.0] | 0.901 [0.862, 1.035] | same |
| cohesive_intrinsic_implicit | mesh | t4 | 5 | 1 | 378.5 µs [375.0 µs, 411.3 µs] | 378.5 µs [375.0 µs, 411.3 µs] | 105.1 [104.2, 114.3] | 0.985 [0.971, 1.020] | same |
| cohesive_intrinsic_implicit | mesh | t8 | 5 | 1 | 400.9 µs [373.0 µs, 415.5 µs] | 400.9 µs [373.0 µs, 415.5 µs] | 111.4 [103.6, 115.4] | 1.012 [0.966, 1.091] | same |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 9.46 ms [9.36 ms, 9.88 ms] | 9.46 ms [9.36 ms, 9.88 ms] | 2629.0 [2599.4, 2744.2] | – | same |
| cohesive_intrinsic_implicit | init | t1 | 5 | 1 | 8.74 ms [8.68 ms, 9.79 ms] | 8.74 ms [8.68 ms, 9.79 ms] | 2427.5 [2411.7, 2718.3] | 0.925 [0.885, 1.010] | same |
| cohesive_intrinsic_implicit | init | t2 | 5 | 1 | 8.91 ms [8.87 ms, 9.56 ms] | 8.91 ms [8.87 ms, 9.56 ms] | 2474.7 [2463.9, 2655.4] | 0.949 [0.902, 1.010] | same |
| cohesive_intrinsic_implicit | init | t4 | 5 | 1 | 9.56 ms [9.45 ms, 9.99 ms] | 9.56 ms [9.45 ms, 9.99 ms] | 2656.9 [2623.7, 2774.6] | 1.009 [0.990, 1.011] | same |
| cohesive_intrinsic_implicit | init | t8 | 5 | 1 | 9.88 ms [8.96 ms, 10.05 ms] | 9.88 ms [8.96 ms, 10.05 ms] | 2745.3 [2490.0, 2792.7] | 1.037 [0.925, 1.074] | same |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 3.052 s [3.045 s, 3.053 s] | 101.72 ms [101.49 ms, 101.78 ms] | 28256.8 [28191.7, 28272.0] | – | same |
| cohesive_intrinsic_implicit | solve_step | t1 | 5 | 30 | 2.981 s [2.977 s, 3.088 s] | 99.37 ms [99.24 ms, 102.93 ms] | 27602.4 [27565.3, 28592.7] | 0.978 [0.975, 1.011] | same |
| cohesive_intrinsic_implicit | solve_step | t2 | 5 | 30 | 2.810 s [2.795 s, 2.816 s] | 93.68 ms [93.17 ms, 93.86 ms] | 26021.5 [25880.9, 26073.2] | 0.922 [0.915, 0.924] | same |
| cohesive_intrinsic_implicit | solve_step | t4 | 5 | 30 | 2.780 s [2.767 s, 2.787 s] | 92.65 ms [92.24 ms, 92.89 ms] | 25736.8 [25623.1, 25804.2] | 0.912 [0.906, 0.913] | same |
| cohesive_intrinsic_implicit | solve_step | t8 | 5 | 30 | 2.766 s [2.754 s, 2.776 s] | 92.21 ms [91.80 ms, 92.54 ms] | 25615.2 [25501.0, 25705.2] | 0.906 [0.903, 0.910] | same |
| implicit_dynamic | mesh | base | 5 | 1 | 655.3 µs [619.0 µs, 671.1 µs] | 655.3 µs [619.0 µs, 671.1 µs] | 39.8 [37.6, 40.8] | – | same |
| implicit_dynamic | mesh | t1 | 5 | 1 | 622.7 µs [619.6 µs, 660.4 µs] | 622.7 µs [619.6 µs, 660.4 µs] | 37.8 [37.6, 40.1] | 0.994 [0.923, 1.023] | same |
| implicit_dynamic | mesh | t2 | 5 | 1 | 622.5 µs [607.6 µs, 648.5 µs] | 622.5 µs [607.6 µs, 648.5 µs] | 37.8 [36.9, 39.4] | 0.961 [0.905, 1.015] | same |
| implicit_dynamic | mesh | t4 | 5 | 1 | 652.0 µs [641.2 µs, 699.8 µs] | 652.0 µs [641.2 µs, 699.8 µs] | 39.6 [38.9, 42.5] | 0.979 [0.971, 1.105] | same |
| implicit_dynamic | mesh | t8 | 5 | 1 | 660.5 µs [613.7 µs, 673.5 µs] | 660.5 µs [613.7 µs, 673.5 µs] | 40.1 [37.3, 40.9] | 1.004 [0.957, 1.043] | same |
| implicit_dynamic | init | base | 5 | 1 | 8.48 ms [7.81 ms, 8.84 ms] | 8.48 ms [7.81 ms, 8.84 ms] | 514.9 [474.6, 537.0] | – | same |
| implicit_dynamic | init | t1 | 5 | 1 | 8.34 ms [7.95 ms, 8.46 ms] | 8.34 ms [7.95 ms, 8.46 ms] | 506.3 [482.8, 514.1] | 0.975 [0.899, 1.083] | same |
| implicit_dynamic | init | t2 | 5 | 1 | 8.18 ms [7.92 ms, 8.36 ms] | 8.18 ms [7.92 ms, 8.36 ms] | 496.9 [480.8, 507.6] | 0.956 [0.936, 1.061] | same |
| implicit_dynamic | init | t4 | 5 | 1 | 8.41 ms [8.17 ms, 8.51 ms] | 8.41 ms [8.17 ms, 8.51 ms] | 510.8 [496.3, 517.0] | 0.970 [0.951, 1.089] | same |
| implicit_dynamic | init | t8 | 5 | 1 | 8.57 ms [8.04 ms, 8.93 ms] | 8.57 ms [8.04 ms, 8.93 ms] | 520.8 [488.6, 542.2] | 1.003 [0.949, 1.142] | same |
| implicit_dynamic | mass_consistent | base | 5 | 1 | 52.79 ms [52.28 ms, 54.85 ms] | 52.79 ms [52.28 ms, 54.85 ms] | 3206.3 [3175.2, 3331.7] | – | same |
| implicit_dynamic | mass_consistent | t1 | 5 | 1 | 58.30 ms [53.76 ms, 58.90 ms] | 58.30 ms [53.76 ms, 58.90 ms] | 3540.8 [3265.6, 3577.5] | 1.095 [1.025, 1.114] | same |
| implicit_dynamic | mass_consistent | t2 | 5 | 1 | 55.35 ms [48.18 ms, 55.48 ms] | 55.35 ms [48.18 ms, 55.48 ms] | 3361.9 [2926.7, 3369.7] | 1.033 [0.913, 1.061] | same |
| implicit_dynamic | mass_consistent | t4 | 5 | 1 | 47.95 ms [47.19 ms, 48.82 ms] | 47.95 ms [47.19 ms, 48.82 ms] | 2912.5 [2866.0, 2965.3] | 0.899 [0.870, 0.934] | same |
| implicit_dynamic | mass_consistent | t8 | 5 | 1 | 47.45 ms [46.57 ms, 48.79 ms] | 47.45 ms [46.57 ms, 48.79 ms] | 2882.3 [2828.6, 2963.6] | 0.898 [0.870, 0.915] | same |
| implicit_dynamic | solve_step | base | 5 | 60 | 822.73 ms [820.76 ms, 827.51 ms] | 13.71 ms [13.68 ms, 13.79 ms] | 832.9 [830.9, 837.7] | – | same |
| implicit_dynamic | solve_step | t1 | 5 | 60 | 823.50 ms [819.18 ms, 826.51 ms] | 13.73 ms [13.65 ms, 13.78 ms] | 833.6 [829.3, 836.7] | 1.001 [0.990, 1.007] | same |
| implicit_dynamic | solve_step | t2 | 5 | 60 | 732.84 ms [731.46 ms, 738.21 ms] | 12.21 ms [12.19 ms, 12.30 ms] | 741.9 [740.5, 747.3] | 0.891 [0.884, 0.898] | same |
| implicit_dynamic | solve_step | t4 | 5 | 60 | 708.90 ms [703.36 ms, 711.83 ms] | 11.82 ms [11.72 ms, 11.86 ms] | 717.6 [712.0, 720.6] | 0.863 [0.855, 0.865] | same |
| implicit_dynamic | solve_step | t8 | 5 | 60 | 701.01 ms [697.61 ms, 705.06 ms] | 11.68 ms [11.63 ms, 11.75 ms] | 709.6 [706.2, 713.7] | 0.853 [0.843, 0.858] | same |
| implicit_dynamic | predictor | base | 5 | 60 | 7.0 µs [6.7 µs, 18.2 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | predictor | t1 | 5 | 60 | 9.4 µs [8.5 µs, 13.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.361 [0.467, 1.903] | same |
| implicit_dynamic | predictor | t2 | 5 | 60 | 9.4 µs [8.2 µs, 9.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.365 [0.450, 1.385] | same |
| implicit_dynamic | predictor | t4 | 5 | 60 | 11.3 µs [8.6 µs, 13.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.314 [0.619, 1.964] | same |
| implicit_dynamic | predictor | t8 | 5 | 60 | 9.5 µs [6.8 µs, 12.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.376 [0.429, 1.757] | same |
| implicit_dynamic | residual | base | 5 | 180 | 149.40 ms [148.24 ms, 149.67 ms] | 830.0 µs [823.6 µs, 831.5 µs] | 50.4 [50.0, 50.5] | – | same |
| implicit_dynamic | residual | t1 | 5 | 180 | 148.36 ms [147.61 ms, 148.82 ms] | 824.2 µs [820.1 µs, 826.8 µs] | 50.1 [49.8, 50.2] | 0.993 [0.991, 1.001] | same |
| implicit_dynamic | residual | t2 | 5 | 180 | 78.39 ms [78.12 ms, 78.95 ms] | 435.5 µs [434.0 µs, 438.6 µs] | 26.5 [26.4, 26.6] | 0.525 [0.523, 0.530] | same |
| implicit_dynamic | residual | t4 | 5 | 180 | 47.51 ms [47.08 ms, 47.83 ms] | 263.9 µs [261.6 µs, 265.7 µs] | 16.0 [15.9, 16.1] | 0.318 [0.316, 0.320] | same |
| implicit_dynamic | residual | t8 | 5 | 180 | 33.00 ms [32.83 ms, 33.19 ms] | 183.3 µs [182.4 µs, 184.4 µs] | 11.1 [11.1, 11.2] | 0.222 [0.220, 0.223] | same |
| implicit_dynamic | matrix | base | 5 | 420 | 143.0 µs [141.2 µs, 149.0 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | matrix | t1 | 5 | 420 | 155.0 µs [151.9 µs, 172.2 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.093 [1.020, 1.220] | same |
| implicit_dynamic | matrix | t2 | 5 | 420 | 132.2 µs [125.7 µs, 133.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.896 [0.879, 0.937] | same |
| implicit_dynamic | matrix | t4 | 5 | 420 | 124.3 µs [115.4 µs, 128.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.855 [0.817, 0.904] | same |
| implicit_dynamic | matrix | t8 | 5 | 420 | 125.3 µs [120.3 µs, 128.8 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.862 [0.841, 0.902] | same |
| implicit_dynamic | corrector | base | 5 | 120 | 24.7 µs [20.4 µs, 26.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | t1 | 5 | 120 | 19.7 µs [16.4 µs, 21.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.867 [0.633, 0.966] | same |
| implicit_dynamic | corrector | t2 | 5 | 120 | 17.9 µs [15.3 µs, 19.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.724 [0.591, 0.919] | same |
| implicit_dynamic | corrector | t4 | 5 | 120 | 19.6 µs [17.8 µs, 21.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.791 [0.687, 0.938] | same |
| implicit_dynamic | corrector | t8 | 5 | 120 | 19.9 µs [17.3 µs, 20.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.804 [0.657, 1.006] | same |
| implicit_dynamic | step_hooks | base | 5 | 120 | 140.0 µs [138.6 µs, 144.1 µs] | 1.2 µs [1.2 µs, 1.2 µs] | 0.1 [0.1, 0.1] | – | same |
| implicit_dynamic | step_hooks | t1 | 5 | 120 | 142.6 µs [134.4 µs, 145.7 µs] | 1.2 µs [1.1 µs, 1.2 µs] | 0.1 [0.1, 0.1] | 1.011 [0.970, 1.038] | same |
| implicit_dynamic | step_hooks | t2 | 5 | 120 | 134.8 µs [130.6 µs, 145.0 µs] | 1.1 µs [1.1 µs, 1.2 µs] | 0.1 [0.1, 0.1] | 0.963 [0.933, 1.035] | same |
| implicit_dynamic | step_hooks | t4 | 5 | 120 | 133.8 µs [129.8 µs, 148.2 µs] | 1.1 µs [1.1 µs, 1.2 µs] | 0.1 [0.1, 0.1] | 0.965 [0.924, 1.058] | same |
| implicit_dynamic | step_hooks | t8 | 5 | 120 | 127.2 µs [125.2 µs, 133.3 µs] | 1.1 µs [1.0 µs, 1.1 µs] | 0.1 [0.1, 0.1] | 0.908 [0.895, 0.925] | same |
| implicit_dynamic | step_rest | base | 5 | 60 | 672.80 ms [671.71 ms, 677.77 ms] | 11.21 ms [11.20 ms, 11.30 ms] | 681.1 [680.0, 686.1] | – | same |
| implicit_dynamic | step_rest | t1 | 5 | 60 | 674.35 ms [670.48 ms, 677.79 ms] | 11.24 ms [11.17 ms, 11.30 ms] | 682.6 [678.7, 686.1] | 1.002 [0.989, 1.008] | same |
| implicit_dynamic | step_rest | t2 | 5 | 60 | 654.04 ms [653.05 ms, 658.97 ms] | 10.90 ms [10.88 ms, 10.98 ms] | 662.1 [661.1, 667.1] | 0.973 [0.964, 0.979] | same |
| implicit_dynamic | step_rest | t4 | 5 | 60 | 661.10 ms [655.98 ms, 663.89 ms] | 11.02 ms [10.93 ms, 11.06 ms] | 669.2 [664.1, 672.1] | 0.984 [0.974, 0.987] | same |
| implicit_dynamic | step_rest | t8 | 5 | 60 | 667.72 ms [664.50 ms, 671.59 ms] | 11.13 ms [11.07 ms, 11.19 ms] | 675.9 [672.7, 679.9] | 0.993 [0.980, 1.000] | same |
| setup_hex8 | mesh | base | 5 | 1 | 1.57 ms [1.48 ms, 1.57 ms] | 1.57 ms [1.48 ms, 1.57 ms] | 113.8 [107.1, 113.8] | – | same |
| setup_hex8 | mesh | t1 | 5 | 1 | 1.49 ms [1.46 ms, 1.50 ms] | 1.49 ms [1.46 ms, 1.50 ms] | 108.0 [105.5, 108.3] | 0.951 [0.927, 1.008] | same |
| setup_hex8 | mesh | t2 | 5 | 1 | 1.52 ms [1.50 ms, 1.58 ms] | 1.52 ms [1.50 ms, 1.58 ms] | 109.8 [108.5, 114.2] | 0.976 [0.953, 1.066] | same |
| setup_hex8 | mesh | t4 | 5 | 1 | 1.53 ms [1.51 ms, 1.61 ms] | 1.53 ms [1.51 ms, 1.61 ms] | 110.8 [109.3, 116.1] | 0.977 [0.961, 1.084] | same |
| setup_hex8 | mesh | t8 | 5 | 1 | 1.57 ms [1.53 ms, 1.61 ms] | 1.57 ms [1.53 ms, 1.61 ms] | 113.7 [110.3, 116.1] | 1.019 [0.983, 1.030] | same |
| setup_hex8 | init_explicit | base | 5 | 1 | 57.75 ms [57.09 ms, 58.28 ms] | 57.75 ms [57.09 ms, 58.28 ms] | 4177.7 [4129.6, 4216.2] | – | same |
| setup_hex8 | init_explicit | t1 | 5 | 1 | 55.43 ms [55.01 ms, 57.20 ms] | 55.43 ms [55.01 ms, 57.20 ms] | 4009.9 [3979.4, 4137.7] | 0.961 [0.944, 0.996] | same |
| setup_hex8 | init_explicit | t2 | 5 | 1 | 54.83 ms [54.39 ms, 56.36 ms] | 54.83 ms [54.39 ms, 56.36 ms] | 3966.6 [3934.5, 4077.3] | 0.949 [0.941, 0.987] | same |
| setup_hex8 | init_explicit | t4 | 5 | 1 | 57.31 ms [57.15 ms, 58.12 ms] | 57.31 ms [57.15 ms, 58.12 ms] | 4145.6 [4133.9, 4204.0] | 0.997 [0.981, 1.006] | same |
| setup_hex8 | init_explicit | t8 | 5 | 1 | 57.74 ms [57.34 ms, 58.40 ms] | 57.74 ms [57.34 ms, 58.40 ms] | 4176.7 [4147.7, 4224.4] | 0.994 [0.993, 1.017] | same |
| setup_hex8 | shape_functions | base | 5 | 3 | 75.64 ms [75.29 ms, 76.62 ms] | 25.21 ms [25.10 ms, 25.54 ms] | 1823.9 [1815.4, 1847.5] | – | same |
| setup_hex8 | shape_functions | t1 | 5 | 3 | 75.87 ms [73.10 ms, 76.25 ms] | 25.29 ms [24.37 ms, 25.42 ms] | 1829.4 [1762.6, 1838.6] | 1.002 [0.971, 1.006] | same |
| setup_hex8 | shape_functions | t2 | 5 | 3 | 75.53 ms [75.40 ms, 76.29 ms] | 25.18 ms [25.13 ms, 25.43 ms] | 1821.2 [1818.1, 1839.6] | 1.000 [0.991, 1.008] | same |
| setup_hex8 | shape_functions | t4 | 5 | 3 | 76.11 ms [75.58 ms, 76.50 ms] | 25.37 ms [25.19 ms, 25.50 ms] | 1835.2 [1822.4, 1844.6] | 1.005 [0.994, 1.016] | same |
| setup_hex8 | shape_functions | t8 | 5 | 3 | 75.71 ms [75.18 ms, 76.02 ms] | 25.24 ms [25.06 ms, 25.34 ms] | 1825.5 [1812.8, 1833.0] | 0.999 [0.992, 1.006] | same |
| setup_hex8 | mass_lumped | base | 5 | 3 | 18.22 ms [18.15 ms, 18.56 ms] | 6.07 ms [6.05 ms, 6.19 ms] | 439.4 [437.6, 447.5] | – | same |
| setup_hex8 | mass_lumped | t1 | 5 | 3 | 18.15 ms [18.11 ms, 18.68 ms] | 6.05 ms [6.04 ms, 6.23 ms] | 437.6 [436.7, 450.5] | 0.998 [0.978, 1.007] | same |
| setup_hex8 | mass_lumped | t2 | 5 | 3 | 17.49 ms [17.44 ms, 17.78 ms] | 5.83 ms [5.81 ms, 5.93 ms] | 421.8 [420.5, 428.6] | 0.962 [0.940, 0.975] | same |
| setup_hex8 | mass_lumped | t4 | 5 | 3 | 17.09 ms [16.91 ms, 18.17 ms] | 5.70 ms [5.64 ms, 6.06 ms] | 412.1 [407.7, 438.2] | 0.939 [0.920, 0.980] | same |
| setup_hex8 | mass_lumped | t8 | 5 | 3 | 17.78 ms [17.58 ms, 18.10 ms] | 5.93 ms [5.86 ms, 6.03 ms] | 428.8 [423.9, 436.3] | 0.976 [0.947, 0.992] | same |
| setup_hex8 | init_implicit | base | 5 | 1 | 49.08 ms [48.75 ms, 50.10 ms] | 49.08 ms [48.75 ms, 50.10 ms] | 3550.4 [3526.3, 3624.1] | – | same |
| setup_hex8 | init_implicit | t1 | 5 | 1 | 49.38 ms [47.57 ms, 49.89 ms] | 49.38 ms [47.57 ms, 49.89 ms] | 3571.9 [3441.1, 3609.3] | 0.993 [0.969, 1.023] | same |
| setup_hex8 | init_implicit | t2 | 5 | 1 | 49.12 ms [48.68 ms, 50.56 ms] | 49.12 ms [48.68 ms, 50.56 ms] | 3553.0 [3521.7, 3657.1] | 0.999 [0.992, 1.030] | same |
| setup_hex8 | init_implicit | t4 | 5 | 1 | 49.14 ms [48.93 ms, 50.00 ms] | 49.14 ms [48.93 ms, 50.00 ms] | 3554.8 [3539.4, 3616.8] | 1.007 [0.977, 1.017] | same |
| setup_hex8 | init_implicit | t8 | 5 | 1 | 49.13 ms [47.12 ms, 49.36 ms] | 49.13 ms [47.12 ms, 49.36 ms] | 3553.8 [3408.3, 3570.6] | 0.996 [0.959, 1.012] | same |
| setup_hex8 | mass_consistent | base | 5 | 3 | 243.24 ms [242.25 ms, 244.59 ms] | 81.08 ms [80.75 ms, 81.53 ms] | 5865.3 [5841.3, 5897.7] | – | same |
| setup_hex8 | mass_consistent | t1 | 5 | 3 | 240.01 ms [237.66 ms, 249.40 ms] | 80.00 ms [79.22 ms, 83.13 ms] | 5787.3 [5730.6, 6013.7] | 0.985 [0.972, 1.030] | same |
| setup_hex8 | mass_consistent | t2 | 5 | 3 | 129.02 ms [128.09 ms, 131.82 ms] | 43.01 ms [42.70 ms, 43.94 ms] | 3111.0 [3088.5, 3178.4] | 0.533 [0.524, 0.542] | same |
| setup_hex8 | mass_consistent | t4 | 5 | 3 | 68.62 ms [68.32 ms, 69.88 ms] | 22.87 ms [22.77 ms, 23.29 ms] | 1654.7 [1647.3, 1684.9] | 0.283 [0.280, 0.288] | same |
| setup_hex8 | mass_consistent | t8 | 5 | 3 | 39.29 ms [37.66 ms, 40.17 ms] | 13.10 ms [12.55 ms, 13.39 ms] | 947.3 [908.2, 968.7] | 0.162 [0.155, 0.165] | same |
| setup_hex8 | stiffness_assembly | base | 5 | 3 | 599.17 ms [595.08 ms, 603.63 ms] | 199.72 ms [198.36 ms, 201.21 ms] | 14447.7 [14348.9, 14555.0] | – | same |
| setup_hex8 | stiffness_assembly | t1 | 5 | 3 | 527.24 ms [519.60 ms, 542.52 ms] | 175.75 ms [173.20 ms, 180.84 ms] | 12713.1 [12528.9, 13081.7] | 0.877 [0.864, 0.912] | same |
| setup_hex8 | stiffness_assembly | t2 | 5 | 3 | 290.35 ms [288.34 ms, 292.27 ms] | 96.78 ms [96.11 ms, 97.42 ms] | 7001.0 [6952.6, 7047.4] | 0.487 [0.478, 0.488] | same |
| setup_hex8 | stiffness_assembly | t4 | 5 | 3 | 148.84 ms [148.18 ms, 149.63 ms] | 49.61 ms [49.39 ms, 49.88 ms] | 3589.0 [3573.1, 3607.9] | 0.249 [0.246, 0.250] | same |
| setup_hex8 | stiffness_assembly | t8 | 5 | 3 | 83.34 ms [82.39 ms, 83.94 ms] | 27.78 ms [27.46 ms, 27.98 ms] | 2009.5 [1986.5, 2024.1] | 0.138 [0.138, 0.140] | same |
| setup_tet10 | mesh | base | 5 | 1 | 1.31 ms [1.28 ms, 1.33 ms] | 1.31 ms [1.28 ms, 1.33 ms] | 126.2 [123.9, 127.9] | – | same |
| setup_tet10 | mesh | t1 | 5 | 1 | 1.24 ms [1.22 ms, 1.28 ms] | 1.24 ms [1.22 ms, 1.28 ms] | 119.6 [117.9, 123.8] | 0.955 [0.922, 0.984] | same |
| setup_tet10 | mesh | t2 | 5 | 1 | 1.28 ms [1.22 ms, 1.32 ms] | 1.28 ms [1.22 ms, 1.32 ms] | 123.3 [118.1, 127.2] | 0.983 [0.935, 1.008] | same |
| setup_tet10 | mesh | t4 | 5 | 1 | 1.28 ms [1.26 ms, 1.34 ms] | 1.28 ms [1.26 ms, 1.34 ms] | 123.8 [121.3, 129.6] | 0.980 [0.968, 1.027] | same |
| setup_tet10 | mesh | t8 | 5 | 1 | 1.27 ms [1.25 ms, 1.32 ms] | 1.27 ms [1.25 ms, 1.32 ms] | 122.8 [120.7, 127.1] | 0.985 [0.957, 1.007] | same |
| setup_tet10 | init_explicit | base | 5 | 1 | 29.15 ms [28.54 ms, 29.29 ms] | 29.15 ms [28.54 ms, 29.29 ms] | 2811.2 [2752.8, 2825.0] | – | same |
| setup_tet10 | init_explicit | t1 | 5 | 1 | 27.97 ms [27.76 ms, 28.29 ms] | 27.97 ms [27.76 ms, 28.29 ms] | 2697.7 [2677.1, 2728.4] | 0.960 [0.952, 0.991] | same |
| setup_tet10 | init_explicit | t2 | 5 | 1 | 28.04 ms [27.25 ms, 29.61 ms] | 28.04 ms [27.25 ms, 29.61 ms] | 2704.3 [2628.7, 2856.3] | 0.957 [0.952, 1.022] | same |
| setup_tet10 | init_explicit | t4 | 5 | 1 | 27.92 ms [27.27 ms, 29.27 ms] | 27.92 ms [27.27 ms, 29.27 ms] | 2692.5 [2629.8, 2823.4] | 0.978 [0.931, 1.004] | same |
| setup_tet10 | init_explicit | t8 | 5 | 1 | 28.43 ms [27.72 ms, 29.45 ms] | 28.43 ms [27.72 ms, 29.45 ms] | 2742.0 [2673.3, 2840.2] | 0.983 [0.948, 1.005] | same |
| setup_tet10 | shape_functions | base | 5 | 3 | 38.77 ms [38.53 ms, 39.18 ms] | 12.92 ms [12.84 ms, 13.06 ms] | 1246.4 [1238.7, 1259.7] | – | same |
| setup_tet10 | shape_functions | t1 | 5 | 3 | 39.20 ms [38.93 ms, 41.25 ms] | 13.07 ms [12.98 ms, 13.75 ms] | 1260.4 [1251.7, 1326.1] | 1.011 [0.994, 1.067] | same |
| setup_tet10 | shape_functions | t2 | 5 | 3 | 39.11 ms [38.82 ms, 39.79 ms] | 13.04 ms [12.94 ms, 13.26 ms] | 1257.5 [1248.0, 1279.3] | 1.007 [0.999, 1.033] | same |
| setup_tet10 | shape_functions | t4 | 5 | 3 | 39.23 ms [38.96 ms, 39.74 ms] | 13.08 ms [12.99 ms, 13.25 ms] | 1261.3 [1252.5, 1277.6] | 1.012 [0.994, 1.031] | same |
| setup_tet10 | shape_functions | t8 | 5 | 3 | 39.10 ms [39.04 ms, 39.30 ms] | 13.03 ms [13.01 ms, 13.10 ms] | 1257.1 [1255.0, 1263.5] | 1.010 [0.999, 1.015] | same |
| setup_tet10 | mass_lumped | base | 5 | 3 | 4.24 ms [4.21 ms, 4.26 ms] | 1.41 ms [1.40 ms, 1.42 ms] | 136.4 [135.4, 136.9] | – | same |
| setup_tet10 | mass_lumped | t1 | 5 | 3 | 4.24 ms [4.21 ms, 4.25 ms] | 1.41 ms [1.40 ms, 1.42 ms] | 136.2 [135.3, 136.8] | 0.999 [0.990, 1.004] | same |
| setup_tet10 | mass_lumped | t2 | 5 | 3 | 4.04 ms [3.92 ms, 4.22 ms] | 1.35 ms [1.31 ms, 1.41 ms] | 130.0 [125.9, 135.5] | 0.956 [0.927, 0.991] | same |
| setup_tet10 | mass_lumped | t4 | 5 | 3 | 3.82 ms [3.74 ms, 3.97 ms] | 1.27 ms [1.25 ms, 1.32 ms] | 122.9 [120.3, 127.5] | 0.905 [0.880, 0.935] | same |
| setup_tet10 | mass_lumped | t8 | 5 | 3 | 4.00 ms [3.80 ms, 4.04 ms] | 1.33 ms [1.27 ms, 1.35 ms] | 128.6 [122.3, 130.0] | 0.943 [0.893, 0.958] | same |
| setup_tet10 | init_implicit | base | 5 | 1 | 24.74 ms [24.28 ms, 25.18 ms] | 24.74 ms [24.28 ms, 25.18 ms] | 2386.4 [2342.3, 2428.6] | – | same |
| setup_tet10 | init_implicit | t1 | 5 | 1 | 24.47 ms [24.23 ms, 25.03 ms] | 24.47 ms [24.23 ms, 25.03 ms] | 2360.6 [2336.7, 2414.0] | 1.004 [0.969, 1.018] | same |
| setup_tet10 | init_implicit | t2 | 5 | 1 | 24.17 ms [24.11 ms, 24.96 ms] | 24.17 ms [24.11 ms, 24.96 ms] | 2331.3 [2325.1, 2407.6] | 0.989 [0.960, 1.009] | same |
| setup_tet10 | init_implicit | t4 | 5 | 1 | 24.56 ms [24.22 ms, 24.76 ms] | 24.56 ms [24.22 ms, 24.76 ms] | 2369.0 [2335.8, 2387.7] | 0.997 [0.966, 1.007] | same |
| setup_tet10 | init_implicit | t8 | 5 | 1 | 23.89 ms [23.72 ms, 24.62 ms] | 23.89 ms [23.72 ms, 24.62 ms] | 2304.5 [2287.7, 2374.8] | 0.966 [0.954, 1.014] | same |
| setup_tet10 | mass_consistent | base | 5 | 3 | 179.23 ms [178.78 ms, 179.80 ms] | 59.74 ms [59.59 ms, 59.93 ms] | 5762.2 [5748.0, 5780.7] | – | same |
| setup_tet10 | mass_consistent | t1 | 5 | 3 | 176.14 ms [174.46 ms, 182.46 ms] | 58.71 ms [58.15 ms, 60.82 ms] | 5663.0 [5608.9, 5866.3] | 0.982 [0.976, 1.015] | same |
| setup_tet10 | mass_consistent | t2 | 5 | 3 | 89.17 ms [88.51 ms, 90.21 ms] | 29.72 ms [29.50 ms, 30.07 ms] | 2866.8 [2845.5, 2900.3] | 0.497 [0.495, 0.504] | same |
| setup_tet10 | mass_consistent | t4 | 5 | 3 | 47.24 ms [46.92 ms, 48.13 ms] | 15.75 ms [15.64 ms, 16.04 ms] | 1518.6 [1508.5, 1547.5] | 0.264 [0.262, 0.269] | same |
| setup_tet10 | mass_consistent | t8 | 5 | 3 | 26.61 ms [26.18 ms, 26.92 ms] | 8.87 ms [8.73 ms, 8.97 ms] | 855.7 [841.5, 865.4] | 0.148 [0.146, 0.150] | same |
| setup_tet10 | stiffness_assembly | base | 5 | 3 | 344.96 ms [342.30 ms, 351.18 ms] | 114.99 ms [114.10 ms, 117.06 ms] | 11090.6 [11005.1, 11290.4] | – | same |
| setup_tet10 | stiffness_assembly | t1 | 5 | 3 | 308.96 ms [308.39 ms, 310.34 ms] | 102.99 ms [102.80 ms, 103.45 ms] | 9933.0 [9914.9, 9977.5] | 0.894 [0.882, 0.907] | same |
| setup_tet10 | stiffness_assembly | t2 | 5 | 3 | 168.69 ms [166.38 ms, 169.12 ms] | 56.23 ms [55.46 ms, 56.37 ms] | 5423.4 [5349.2, 5437.1] | 0.489 [0.474, 0.494] | same |
| setup_tet10 | stiffness_assembly | t4 | 5 | 3 | 87.32 ms [85.99 ms, 87.55 ms] | 29.11 ms [28.66 ms, 29.18 ms] | 2807.2 [2764.6, 2814.8] | 0.251 [0.248, 0.255] | same |
| setup_tet10 | stiffness_assembly | t8 | 5 | 3 | 46.83 ms [46.32 ms, 47.24 ms] | 15.61 ms [15.44 ms, 15.75 ms] | 1505.5 [1489.3, 1518.8] | 0.135 [0.132, 0.137] | same |
| setup_tet4 | mesh | base | 5 | 1 | 5.46 ms [5.26 ms, 5.60 ms] | 5.46 ms [5.26 ms, 5.60 ms] | 33.7 [32.5, 34.5] | – | same |
| setup_tet4 | mesh | t1 | 5 | 1 | 5.42 ms [5.29 ms, 5.70 ms] | 5.42 ms [5.29 ms, 5.70 ms] | 33.5 [32.7, 35.2] | 0.994 [0.949, 1.075] | same |
| setup_tet4 | mesh | t2 | 5 | 1 | 5.03 ms [5.00 ms, 5.08 ms] | 5.03 ms [5.00 ms, 5.08 ms] | 31.0 [30.9, 31.4] | 0.917 [0.896, 0.956] | same |
| setup_tet4 | mesh | t4 | 5 | 1 | 5.45 ms [5.08 ms, 5.59 ms] | 5.45 ms [5.08 ms, 5.59 ms] | 33.7 [31.3, 34.5] | 1.000 [0.907, 1.039] | same |
| setup_tet4 | mesh | t8 | 5 | 1 | 5.69 ms [5.49 ms, 5.73 ms] | 5.69 ms [5.49 ms, 5.73 ms] | 35.1 [33.9, 35.4] | 1.043 [1.012, 1.058] | same |
| setup_tet4 | init_explicit | base | 5 | 1 | 78.12 ms [77.48 ms, 78.92 ms] | 78.12 ms [77.48 ms, 78.92 ms] | 482.2 [478.3, 487.2] | – | same |
| setup_tet4 | init_explicit | t1 | 5 | 1 | 76.10 ms [73.45 ms, 76.79 ms] | 76.10 ms [73.45 ms, 76.79 ms] | 469.8 [453.4, 474.0] | 0.973 [0.941, 0.983] | same |
| setup_tet4 | init_explicit | t2 | 5 | 1 | 76.90 ms [72.28 ms, 77.04 ms] | 76.90 ms [72.28 ms, 77.04 ms] | 474.7 [446.2, 475.6] | 0.984 [0.925, 0.989] | same |
| setup_tet4 | init_explicit | t4 | 5 | 1 | 77.53 ms [73.30 ms, 79.14 ms] | 77.53 ms [73.30 ms, 79.14 ms] | 478.6 [452.5, 488.5] | 0.998 [0.938, 1.014] | same |
| setup_tet4 | init_explicit | t8 | 5 | 1 | 78.41 ms [73.57 ms, 78.46 ms] | 78.41 ms [73.57 ms, 78.46 ms] | 484.0 [454.1, 484.3] | 0.994 [0.950, 1.004] | same |
| setup_tet4 | shape_functions | base | 5 | 3 | 98.49 ms [98.14 ms, 99.54 ms] | 32.83 ms [32.71 ms, 33.18 ms] | 202.7 [201.9, 204.8] | – | same |
| setup_tet4 | shape_functions | t1 | 5 | 3 | 98.88 ms [97.67 ms, 99.58 ms] | 32.96 ms [32.56 ms, 33.19 ms] | 203.5 [201.0, 204.9] | 0.999 [0.986, 1.011] | same |
| setup_tet4 | shape_functions | t2 | 5 | 3 | 97.67 ms [97.26 ms, 102.70 ms] | 32.56 ms [32.42 ms, 34.23 ms] | 201.0 [200.1, 211.3] | 0.992 [0.977, 1.047] | same |
| setup_tet4 | shape_functions | t4 | 5 | 3 | 97.59 ms [97.44 ms, 98.16 ms] | 32.53 ms [32.48 ms, 32.72 ms] | 200.8 [200.5, 202.0] | 0.991 [0.979, 1.000] | same |
| setup_tet4 | shape_functions | t8 | 5 | 3 | 98.86 ms [98.30 ms, 101.26 ms] | 32.95 ms [32.77 ms, 33.75 ms] | 203.4 [202.3, 208.4] | 1.007 [0.990, 1.022] | same |
| setup_tet4 | mass_lumped | base | 5 | 3 | 63.85 ms [62.48 ms, 64.12 ms] | 21.28 ms [20.83 ms, 21.37 ms] | 131.4 [128.6, 131.9] | – | same |
| setup_tet4 | mass_lumped | t1 | 5 | 3 | 64.25 ms [63.66 ms, 65.00 ms] | 21.42 ms [21.22 ms, 21.67 ms] | 132.2 [131.0, 133.7] | 1.009 [1.001, 1.028] | same |
| setup_tet4 | mass_lumped | t2 | 5 | 3 | 64.39 ms [63.93 ms, 64.55 ms] | 21.46 ms [21.31 ms, 21.52 ms] | 132.5 [131.6, 132.8] | 1.011 [0.999, 1.031] | same |
| setup_tet4 | mass_lumped | t4 | 5 | 3 | 63.75 ms [63.06 ms, 64.50 ms] | 21.25 ms [21.02 ms, 21.50 ms] | 131.2 [129.8, 132.7] | 0.996 [0.988, 1.025] | same |
| setup_tet4 | mass_lumped | t8 | 5 | 3 | 62.86 ms [62.54 ms, 63.63 ms] | 20.95 ms [20.85 ms, 21.21 ms] | 129.3 [128.7, 130.9] | 0.984 [0.979, 1.006] | same |
| setup_tet4 | init_implicit | base | 5 | 1 | 71.46 ms [70.47 ms, 72.38 ms] | 71.46 ms [70.47 ms, 72.38 ms] | 441.1 [435.0, 446.8] | – | same |
| setup_tet4 | init_implicit | t1 | 5 | 1 | 71.79 ms [71.69 ms, 72.34 ms] | 71.79 ms [71.69 ms, 72.34 ms] | 443.1 [442.5, 446.5] | 1.005 [0.992, 1.027] | same |
| setup_tet4 | init_implicit | t2 | 5 | 1 | 72.84 ms [72.01 ms, 73.36 ms] | 72.84 ms [72.01 ms, 73.36 ms] | 449.6 [444.5, 452.8] | 1.019 [1.009, 1.030] | same |
| setup_tet4 | init_implicit | t4 | 5 | 1 | 71.69 ms [71.00 ms, 74.66 ms] | 71.69 ms [71.00 ms, 74.66 ms] | 442.5 [438.3, 460.9] | 1.017 [0.981, 1.042] | same |
| setup_tet4 | init_implicit | t8 | 5 | 1 | 70.57 ms [69.69 ms, 72.08 ms] | 70.57 ms [69.69 ms, 72.08 ms] | 435.6 [430.2, 444.9] | 0.992 [0.975, 1.006] | same |
| setup_tet4 | mass_consistent | base | 5 | 3 | 343.64 ms [341.85 ms, 344.68 ms] | 114.55 ms [113.95 ms, 114.89 ms] | 707.1 [703.4, 709.2] | – | same |
| setup_tet4 | mass_consistent | t1 | 5 | 3 | 315.91 ms [311.39 ms, 317.82 ms] | 105.30 ms [103.80 ms, 105.94 ms] | 650.0 [640.7, 653.9] | 0.919 [0.903, 0.925] | same |
| setup_tet4 | mass_consistent | t2 | 5 | 3 | 170.22 ms [169.47 ms, 173.13 ms] | 56.74 ms [56.49 ms, 57.71 ms] | 350.2 [348.7, 356.2] | 0.496 [0.492, 0.504] | same |
| setup_tet4 | mass_consistent | t4 | 5 | 3 | 98.15 ms [97.72 ms, 99.83 ms] | 32.72 ms [32.57 ms, 33.28 ms] | 201.9 [201.1, 205.4] | 0.285 [0.284, 0.291] | same |
| setup_tet4 | mass_consistent | t8 | 5 | 3 | 63.70 ms [62.92 ms, 64.04 ms] | 21.23 ms [20.97 ms, 21.35 ms] | 131.1 [129.5, 131.8] | 0.185 [0.183, 0.187] | same |
| setup_tet4 | stiffness_assembly | base | 5 | 3 | 470.71 ms [469.16 ms, 473.34 ms] | 156.90 ms [156.39 ms, 157.78 ms] | 968.5 [965.3, 973.9] | – | same |
| setup_tet4 | stiffness_assembly | t1 | 5 | 3 | 350.56 ms [349.00 ms, 352.65 ms] | 116.85 ms [116.33 ms, 117.55 ms] | 721.3 [718.1, 725.6] | 0.744 [0.741, 0.748] | same |
| setup_tet4 | stiffness_assembly | t2 | 5 | 3 | 227.95 ms [226.31 ms, 231.30 ms] | 75.98 ms [75.44 ms, 77.10 ms] | 469.0 [465.7, 475.9] | 0.483 [0.481, 0.491] | same |
| setup_tet4 | stiffness_assembly | t4 | 5 | 3 | 130.10 ms [128.57 ms, 130.58 ms] | 43.37 ms [42.86 ms, 43.53 ms] | 267.7 [264.5, 268.7] | 0.276 [0.273, 0.278] | same |
| setup_tet4 | stiffness_assembly | t8 | 5 | 3 | 79.70 ms [78.94 ms, 80.13 ms] | 26.57 ms [26.31 ms, 26.71 ms] | 164.0 [162.4, 164.9] | 0.169 [0.167, 0.170] | same |
| static_linear_eigen | mesh | base | 5 | 1 | 659.4 µs [645.8 µs, 682.1 µs] | 659.4 µs [645.8 µs, 682.1 µs] | 40.1 [39.2, 41.4] | – | same |
| static_linear_eigen | mesh | t1 | 5 | 1 | 622.3 µs [610.0 µs, 673.3 µs] | 622.3 µs [610.0 µs, 673.3 µs] | 37.8 [37.0, 40.9] | 0.944 [0.900, 1.035] | same |
| static_linear_eigen | mesh | t2 | 5 | 1 | 641.0 µs [635.8 µs, 661.7 µs] | 641.0 µs [635.8 µs, 661.7 µs] | 38.9 [38.6, 40.2] | 0.972 [0.942, 1.025] | same |
| static_linear_eigen | mesh | t4 | 5 | 1 | 614.4 µs [607.9 µs, 689.8 µs] | 614.4 µs [607.9 µs, 689.8 µs] | 37.3 [36.9, 41.9] | 0.945 [0.901, 1.011] | same |
| static_linear_eigen | mesh | t8 | 5 | 1 | 637.4 µs [620.9 µs, 672.8 µs] | 637.4 µs [620.9 µs, 672.8 µs] | 38.7 [37.7, 40.9] | 0.951 [0.920, 1.034] | same |
| static_linear_eigen | init | base | 5 | 1 | 8.32 ms [8.24 ms, 8.47 ms] | 8.32 ms [8.24 ms, 8.47 ms] | 505.1 [500.6, 514.7] | – | same |
| static_linear_eigen | init | t1 | 5 | 1 | 8.11 ms [7.54 ms, 8.22 ms] | 8.11 ms [7.54 ms, 8.22 ms] | 492.7 [458.2, 499.3] | 0.957 [0.907, 0.984] | same |
| static_linear_eigen | init | t2 | 5 | 1 | 8.12 ms [7.52 ms, 8.24 ms] | 8.12 ms [7.52 ms, 8.24 ms] | 493.3 [456.9, 500.7] | 0.979 [0.906, 0.991] | same |
| static_linear_eigen | init | t4 | 5 | 1 | 8.20 ms [7.90 ms, 8.57 ms] | 8.20 ms [7.90 ms, 8.57 ms] | 498.0 [479.9, 520.6] | 0.977 [0.932, 1.040] | same |
| static_linear_eigen | init | t8 | 5 | 1 | 8.54 ms [8.33 ms, 8.76 ms] | 8.54 ms [8.33 ms, 8.76 ms] | 518.9 [506.2, 532.1] | 1.028 [0.984, 1.044] | same |
| static_linear_eigen | solve_step | base | 5 | 1 | 754.51 ms [749.23 ms, 758.43 ms] | 754.51 ms [749.23 ms, 758.43 ms] | 45827.9 [45507.2, 46066.0] | – | same |
| static_linear_eigen | solve_step | t1 | 5 | 1 | 743.84 ms [741.12 ms, 745.70 ms] | 743.84 ms [741.12 ms, 745.70 ms] | 45179.8 [45014.6, 45292.5] | 0.986 [0.979, 0.994] | same |
| static_linear_eigen | solve_step | t2 | 5 | 1 | 740.97 ms [736.91 ms, 743.56 ms] | 740.97 ms [736.91 ms, 743.56 ms] | 45005.8 [44759.2, 45162.6] | 0.982 [0.973, 0.990] | same |
| static_linear_eigen | solve_step | t4 | 5 | 1 | 740.25 ms [736.15 ms, 744.80 ms] | 740.25 ms [736.15 ms, 744.80 ms] | 44961.9 [44712.5, 45238.1] | 0.978 [0.974, 0.991] | same |
| static_linear_eigen | solve_step | t8 | 5 | 1 | 735.95 ms [734.13 ms, 737.68 ms] | 735.95 ms [734.13 ms, 737.68 ms] | 44700.8 [44590.1, 44805.4] | 0.977 [0.970, 0.982] | same |
| static_linear_eigen | predictor | base | 5 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | t1 | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.017 [0.578, 1.162] | same |
| static_linear_eigen | predictor | t2 | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.975 [0.563, 1.038] | same |
| static_linear_eigen | predictor | t4 | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.984 [0.714, 1.152] | same |
| static_linear_eigen | predictor | t8 | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.924 [0.658, 1.138] | same |
| static_linear_eigen | matrix | base | 5 | 1 | 66.55 ms [66.04 ms, 66.83 ms] | 66.55 ms [66.04 ms, 66.83 ms] | 4042.1 [4011.3, 4059.1] | – | same |
| static_linear_eigen | matrix | t1 | 5 | 1 | 64.69 ms [63.88 ms, 65.58 ms] | 64.69 ms [63.88 ms, 65.58 ms] | 3929.0 [3880.2, 3983.5] | 0.974 [0.959, 0.990] | same |
| static_linear_eigen | matrix | t2 | 5 | 1 | 59.60 ms [58.29 ms, 60.54 ms] | 59.60 ms [58.29 ms, 60.54 ms] | 3620.1 [3540.5, 3677.2] | 0.899 [0.872, 0.910] | same |
| static_linear_eigen | matrix | t4 | 5 | 1 | 57.56 ms [56.47 ms, 58.91 ms] | 57.56 ms [56.47 ms, 58.91 ms] | 3496.4 [3429.8, 3578.4] | 0.865 [0.850, 0.892] | same |
| static_linear_eigen | matrix | t8 | 5 | 1 | 57.06 ms [56.11 ms, 57.47 ms] | 57.06 ms [56.11 ms, 57.47 ms] | 3465.6 [3407.8, 3490.4] | 0.854 [0.846, 0.864] | same |
| static_linear_eigen | corrector | base | 5 | 1 | 0.4 µs [0.4 µs, 0.6 µs] | 0.4 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | corrector | t1 | 5 | 1 | 0.5 µs [0.4 µs, 0.8 µs] | 0.5 µs [0.4 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 1.373 [0.667, 1.455] | same |
| static_linear_eigen | corrector | t2 | 5 | 1 | 0.5 µs [0.4 µs, 0.7 µs] | 0.5 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 1.042 [0.929, 1.435] | same |
| static_linear_eigen | corrector | t4 | 5 | 1 | 0.6 µs [0.5 µs, 9.5 µs] | 0.6 µs [0.5 µs, 9.5 µs] | 0.0 [0.0, 0.6] | 1.616 [1.136, 15.168] | same |
| static_linear_eigen | corrector | t8 | 5 | 1 | 0.5 µs [0.4 µs, 0.5 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 0.990 [0.736, 1.390] | same |
| static_linear_eigen | step_hooks | base | 5 | 2 | 5.3 µs [4.9 µs, 5.3 µs] | 2.6 µs [2.4 µs, 2.7 µs] | 0.2 [0.1, 0.2] | – | same |
| static_linear_eigen | step_hooks | t1 | 5 | 2 | 5.0 µs [4.9 µs, 5.4 µs] | 2.5 µs [2.5 µs, 2.7 µs] | 0.2 [0.1, 0.2] | 1.001 [0.946, 1.019] | same |
| static_linear_eigen | step_hooks | t2 | 5 | 2 | 5.0 µs [4.6 µs, 5.2 µs] | 2.5 µs [2.3 µs, 2.6 µs] | 0.2 [0.1, 0.2] | 0.957 [0.872, 1.066] | same |
| static_linear_eigen | step_hooks | t4 | 5 | 2 | 5.0 µs [4.6 µs, 5.4 µs] | 2.5 µs [2.3 µs, 2.7 µs] | 0.2 [0.1, 0.2] | 0.947 [0.856, 1.093] | same |
| static_linear_eigen | step_hooks | t8 | 5 | 2 | 5.2 µs [4.9 µs, 5.6 µs] | 2.6 µs [2.4 µs, 2.8 µs] | 0.2 [0.1, 0.2] | 0.989 [0.947, 1.103] | same |
| static_linear_eigen | step_rest | base | 5 | 1 | 687.91 ms [683.18 ms, 692.10 ms] | 687.91 ms [683.18 ms, 692.10 ms] | 41782.9 [41495.6, 42037.0] | – | same |
| static_linear_eigen | step_rest | t1 | 5 | 1 | 679.55 ms [676.43 ms, 680.11 ms] | 679.55 ms [676.43 ms, 680.11 ms] | 41274.8 [41085.3, 41308.6] | 0.988 [0.979, 0.995] | same |
| static_linear_eigen | step_rest | t2 | 5 | 1 | 681.13 ms [678.62 ms, 683.01 ms] | 681.13 ms [678.62 ms, 683.01 ms] | 41371.2 [41218.3, 41485.1] | 0.989 [0.981, 0.999] | same |
| static_linear_eigen | step_rest | t4 | 5 | 1 | 682.68 ms [679.33 ms, 686.80 ms] | 682.68 ms [679.33 ms, 686.80 ms] | 41465.1 [41261.4, 41715.1] | 0.990 [0.986, 1.000] | same |
| static_linear_eigen | step_rest | t8 | 5 | 1 | 679.53 ms [676.66 ms, 680.49 ms] | 679.53 ms [676.66 ms, 680.49 ms] | 41273.6 [41099.4, 41332.2] | 0.988 [0.980, 0.995] | same |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 41.25 ms [41.05 ms, 42.37 ms] | 13.75 ms [13.68 ms, 14.12 ms] | 835.1 [831.2, 857.8] | – | same |
| static_linear_eigen | stiffness_assembly | t1 | 5 | 3 | 34.91 ms [34.72 ms, 35.50 ms] | 11.64 ms [11.57 ms, 11.83 ms] | 706.7 [702.9, 718.7] | 0.846 [0.838, 0.848] | same |
| static_linear_eigen | stiffness_assembly | t2 | 5 | 3 | 19.59 ms [18.48 ms, 19.95 ms] | 6.53 ms [6.16 ms, 6.65 ms] | 396.7 [374.1, 403.9] | 0.473 [0.450, 0.477] | same |
| static_linear_eigen | stiffness_assembly | t4 | 5 | 3 | 11.86 ms [11.34 ms, 12.04 ms] | 3.95 ms [3.78 ms, 4.01 ms] | 240.1 [229.6, 243.8] | 0.289 [0.270, 0.292] | same |
| static_linear_eigen | stiffness_assembly | t8 | 5 | 3 | 7.17 ms [6.98 ms, 7.23 ms] | 2.39 ms [2.33 ms, 2.41 ms] | 145.3 [141.2, 146.5] | 0.174 [0.167, 0.176] | same |
| static_linear_eigen | analysis | base | 5 | 1 | 50.32 ms [49.70 ms, 50.45 ms] | 50.32 ms [49.70 ms, 50.45 ms] | 3056.3 [3018.6, 3064.0] | – | same |
| static_linear_eigen | analysis | t1 | 5 | 1 | 50.94 ms [50.58 ms, 51.09 ms] | 50.94 ms [50.58 ms, 51.09 ms] | 3094.2 [3072.3, 3102.9] | 1.013 [1.010, 1.024] | same |
| static_linear_eigen | analysis | t2 | 5 | 1 | 49.61 ms [49.55 ms, 51.53 ms] | 49.61 ms [49.55 ms, 51.53 ms] | 3013.1 [3009.8, 3129.8] | 0.986 [0.982, 1.033] | same |
| static_linear_eigen | analysis | t4 | 5 | 1 | 51.16 ms [50.97 ms, 51.46 ms] | 51.16 ms [50.97 ms, 51.46 ms] | 3107.3 [3095.6, 3125.4] | 1.020 [1.013, 1.031] | same |
| static_linear_eigen | analysis | t8 | 5 | 1 | 50.67 ms [50.42 ms, 50.76 ms] | 50.67 ms [50.42 ms, 50.76 ms] | 3077.8 [3062.5, 3083.1] | 1.009 [1.003, 1.016] | same |
| static_linear_eigen | factorize | base | 5 | 2 | 1.113 s [1.110 s, 1.122 s] | 556.62 ms [554.81 ms, 560.80 ms] | 33808.5 [33698.1, 34062.5] | – | same |
| static_linear_eigen | factorize | t1 | 5 | 2 | 1.095 s [1.089 s, 1.105 s] | 547.32 ms [544.28 ms, 552.60 ms] | 33243.5 [33058.7, 33564.4] | 0.985 [0.981, 0.989] | same |
| static_linear_eigen | factorize | t2 | 5 | 2 | 1.104 s [1.096 s, 1.106 s] | 551.97 ms [548.22 ms, 552.87 ms] | 33526.0 [33298.3, 33580.3] | 0.992 [0.983, 0.995] | same |
| static_linear_eigen | factorize | t4 | 5 | 2 | 1.099 s [1.095 s, 1.111 s] | 549.33 ms [547.53 ms, 555.42 ms] | 33365.8 [33256.3, 33735.2] | 0.987 [0.980, 1.001] | same |
| static_linear_eigen | factorize | t8 | 5 | 2 | 1.098 s [1.094 s, 1.100 s] | 549.16 ms [546.86 ms, 550.21 ms] | 33355.3 [33215.5, 33419.1] | 0.986 [0.975, 0.992] | same |
| static_linear_eigen | back_substitution | base | 5 | 20 | 99.27 ms [98.96 ms, 99.68 ms] | 4.96 ms [4.95 ms, 4.98 ms] | 301.5 [300.5, 302.7] | – | same |
| static_linear_eigen | back_substitution | t1 | 5 | 20 | 100.23 ms [99.10 ms, 100.41 ms] | 5.01 ms [4.95 ms, 5.02 ms] | 304.4 [300.9, 304.9] | 1.010 [0.994, 1.015] | same |
| static_linear_eigen | back_substitution | t2 | 5 | 20 | 98.91 ms [98.75 ms, 99.37 ms] | 4.95 ms [4.94 ms, 4.97 ms] | 300.4 [299.9, 301.8] | 0.996 [0.992, 1.004] | same |
| static_linear_eigen | back_substitution | t4 | 5 | 20 | 99.11 ms [98.54 ms, 99.25 ms] | 4.96 ms [4.93 ms, 4.96 ms] | 301.0 [299.3, 301.4] | 0.997 [0.996, 0.999] | same |
| static_linear_eigen | back_substitution | t8 | 5 | 20 | 99.61 ms [98.76 ms, 100.04 ms] | 4.98 ms [4.94 ms, 5.00 ms] | 302.5 [299.9, 303.8] | 1.004 [0.991, 1.008] | same |
| static_newton_plastic | mesh | base | 5 | 1 | 250.3 µs [239.4 µs, 275.9 µs] | 250.3 µs [239.4 µs, 275.9 µs] | 41.7 [39.9, 46.0] | – | same |
| static_newton_plastic | mesh | t1 | 5 | 1 | 260.1 µs [246.9 µs, 277.8 µs] | 260.1 µs [246.9 µs, 277.8 µs] | 43.3 [41.1, 46.3] | 0.986 [0.943, 1.154] | same |
| static_newton_plastic | mesh | t2 | 5 | 1 | 239.7 µs [234.4 µs, 261.9 µs] | 239.7 µs [234.4 µs, 261.9 µs] | 40.0 [39.1, 43.7] | 0.958 [0.850, 1.094] | same |
| static_newton_plastic | mesh | t4 | 5 | 1 | 247.3 µs [240.4 µs, 272.6 µs] | 247.3 µs [240.4 µs, 272.6 µs] | 41.2 [40.1, 45.4] | 1.027 [0.890, 1.089] | same |
| static_newton_plastic | mesh | t8 | 5 | 1 | 253.2 µs [237.9 µs, 260.9 µs] | 253.2 µs [237.9 µs, 260.9 µs] | 42.2 [39.6, 43.5] | 0.996 [0.880, 1.052] | same |
| static_newton_plastic | init | base | 5 | 1 | 3.59 ms [3.42 ms, 3.74 ms] | 3.59 ms [3.42 ms, 3.74 ms] | 598.2 [570.1, 623.4] | – | same |
| static_newton_plastic | init | t1 | 5 | 1 | 3.44 ms [3.28 ms, 3.65 ms] | 3.44 ms [3.28 ms, 3.65 ms] | 573.8 [547.4, 608.0] | 0.959 [0.878, 1.019] | same |
| static_newton_plastic | init | t2 | 5 | 1 | 3.58 ms [3.40 ms, 3.83 ms] | 3.58 ms [3.40 ms, 3.83 ms] | 596.2 [567.3, 638.7] | 0.997 [0.910, 1.120] | same |
| static_newton_plastic | init | t4 | 5 | 1 | 3.71 ms [3.54 ms, 3.83 ms] | 3.71 ms [3.54 ms, 3.83 ms] | 619.1 [590.4, 637.7] | 1.021 [0.956, 1.086] | same |
| static_newton_plastic | init | t8 | 5 | 1 | 3.72 ms [3.71 ms, 3.96 ms] | 3.72 ms [3.71 ms, 3.96 ms] | 619.5 [618.9, 659.8] | 1.058 [1.003, 1.086] | same |
| static_newton_plastic | solve_step | base | 5 | 12 | 1.762 s [1.758 s, 1.765 s] | 146.87 ms [146.49 ms, 147.04 ms] | 24477.5 [24415.4, 24507.3] | – | same |
| static_newton_plastic | solve_step | t1 | 5 | 12 | 1.759 s [1.754 s, 1.761 s] | 146.57 ms [146.17 ms, 146.71 ms] | 24428.4 [24361.3, 24451.8] | 0.997 [0.995, 1.001] | same |
| static_newton_plastic | solve_step | t2 | 5 | 12 | 1.656 s [1.652 s, 1.660 s] | 137.98 ms [137.64 ms, 138.30 ms] | 22996.5 [22940.7, 23049.9] | 0.939 [0.939, 0.941] | same |
| static_newton_plastic | solve_step | t4 | 5 | 12 | 1.661 s [1.653 s, 1.673 s] | 138.41 ms [137.78 ms, 139.38 ms] | 23067.8 [22962.6, 23230.5] | 0.942 [0.939, 0.950] | same |
| static_newton_plastic | solve_step | t8 | 5 | 12 | 1.686 s [1.674 s, 1.716 s] | 140.51 ms [139.46 ms, 142.99 ms] | 23418.6 [23244.0, 23832.3] | 0.957 [0.952, 0.974] | same |
| static_newton_plastic | predictor | base | 5 | 12 | 1.5 µs [1.4 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | predictor | t1 | 5 | 12 | 1.6 µs [1.2 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.075 [0.801, 1.134] | same |
| static_newton_plastic | predictor | t2 | 5 | 12 | 1.6 µs [1.2 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.975 [0.819, 1.131] | same |
| static_newton_plastic | predictor | t4 | 5 | 12 | 1.6 µs [1.2 µs, 1.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.990 [0.762, 1.293] | same |
| static_newton_plastic | predictor | t8 | 5 | 12 | 1.4 µs [1.2 µs, 1.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.924 [0.876, 1.043] | same |
| static_newton_plastic | residual | base | 5 | 961 | 559.58 ms [558.17 ms, 563.47 ms] | 582.3 µs [580.8 µs, 586.3 µs] | 97.0 [96.8, 97.7] | – | same |
| static_newton_plastic | residual | t1 | 5 | 961 | 556.78 ms [554.68 ms, 558.94 ms] | 579.4 µs [577.2 µs, 581.6 µs] | 96.6 [96.2, 96.9] | 0.997 [0.984, 1.001] | same |
| static_newton_plastic | residual | t2 | 5 | 961 | 445.08 ms [444.71 ms, 447.76 ms] | 463.1 µs [462.8 µs, 465.9 µs] | 77.2 [77.1, 77.7] | 0.797 [0.789, 0.798] | same |
| static_newton_plastic | residual | t4 | 5 | 961 | 442.80 ms [440.09 ms, 451.14 ms] | 460.8 µs [457.9 µs, 469.4 µs] | 76.8 [76.3, 78.2] | 0.793 [0.788, 0.801] | same |
| static_newton_plastic | residual | t8 | 5 | 961 | 453.00 ms [446.86 ms, 481.37 ms] | 471.4 µs [465.0 µs, 500.9 µs] | 78.6 [77.5, 83.5] | 0.807 [0.800, 0.854] | same |
| static_newton_plastic | matrix | base | 5 | 949 | 556.4 µs [531.0 µs, 580.5 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | – | same |
| static_newton_plastic | matrix | t1 | 5 | 949 | 590.6 µs [572.0 µs, 606.6 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 1.045 [1.027, 1.112] | same |
| static_newton_plastic | matrix | t2 | 5 | 949 | 554.7 µs [541.9 µs, 570.8 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.996 [0.933, 1.075] | same |
| static_newton_plastic | matrix | t4 | 5 | 949 | 564.3 µs [508.1 µs, 584.9 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.976 [0.912, 1.069] | same |
| static_newton_plastic | matrix | t8 | 5 | 949 | 551.8 µs [542.3 µs, 598.5 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.992 [0.951, 1.127] | same |
| static_newton_plastic | corrector | base | 5 | 949 | 176.0 µs [174.7 µs, 181.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | corrector | t1 | 5 | 949 | 144.6 µs [127.8 µs, 147.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.799 [0.712, 0.844] | same |
| static_newton_plastic | corrector | t2 | 5 | 949 | 138.8 µs [133.4 µs, 146.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.788 [0.758, 0.804] | same |
| static_newton_plastic | corrector | t4 | 5 | 949 | 152.9 µs [129.4 µs, 157.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.845 [0.735, 0.880] | same |
| static_newton_plastic | corrector | t8 | 5 | 949 | 151.0 µs [138.7 µs, 173.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.832 [0.793, 0.988] | same |
| static_newton_plastic | step_hooks | base | 5 | 24 | 688.8 µs [683.5 µs, 704.9 µs] | 28.7 µs [28.5 µs, 29.4 µs] | 4.8 [4.7, 4.9] | – | same |
| static_newton_plastic | step_hooks | t1 | 5 | 24 | 682.3 µs [673.7 µs, 689.4 µs] | 28.4 µs [28.1 µs, 28.7 µs] | 4.7 [4.7, 4.8] | 0.988 [0.968, 1.008] | same |
| static_newton_plastic | step_hooks | t2 | 5 | 24 | 689.8 µs [686.4 µs, 708.1 µs] | 28.7 µs [28.6 µs, 29.5 µs] | 4.8 [4.8, 4.9] | 1.004 [0.979, 1.025] | same |
| static_newton_plastic | step_hooks | t4 | 5 | 24 | 676.8 µs [669.8 µs, 680.9 µs] | 28.2 µs [27.9 µs, 28.4 µs] | 4.7 [4.7, 4.7] | 0.983 [0.950, 0.996] | same |
| static_newton_plastic | step_hooks | t8 | 5 | 24 | 682.7 µs [672.2 µs, 702.9 µs] | 28.4 µs [28.0 µs, 29.3 µs] | 4.7 [4.7, 4.9] | 0.999 [0.954, 1.028] | same |
| static_newton_plastic | step_rest | base | 5 | 12 | 1.201 s [1.196 s, 1.203 s] | 100.11 ms [99.71 ms, 100.26 ms] | 16685.6 [16618.0, 16709.7] | – | same |
| static_newton_plastic | step_rest | t1 | 5 | 12 | 1.199 s [1.196 s, 1.202 s] | 99.92 ms [99.65 ms, 100.19 ms] | 16653.9 [16608.2, 16698.1] | 1.000 [0.994, 1.002] | same |
| static_newton_plastic | step_rest | t2 | 5 | 12 | 1.208 s [1.206 s, 1.210 s] | 100.69 ms [100.47 ms, 100.87 ms] | 16781.4 [16744.7, 16811.0] | 1.006 [1.005, 1.010] | same |
| static_newton_plastic | step_rest | t4 | 5 | 12 | 1.217 s [1.211 s, 1.224 s] | 101.42 ms [100.95 ms, 101.97 ms] | 16903.9 [16824.5, 16994.2] | 1.013 [1.007, 1.020] | same |
| static_newton_plastic | step_rest | t8 | 5 | 12 | 1.233 s [1.225 s, 1.235 s] | 102.76 ms [102.11 ms, 102.95 ms] | 17127.0 [17018.3, 17157.7] | 1.025 [1.023, 1.031] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 0bd91ba55, d404f4c70 | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 0bd91ba55, d404f4c70 | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 0bd91ba55, d404f4c70 | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 0bd91ba55, d404f4c70 | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 0bd91ba55, d404f4c70 | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 0bd91ba55, d404f4c70 | d5869f57a777d9a5 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 0bd91ba55, d404f4c70 | 7bd5f114957d7d7b |
