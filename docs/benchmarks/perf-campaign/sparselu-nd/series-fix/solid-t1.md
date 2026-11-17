Suite `solid`, size `m`; variants: `auto`, `base`, `colamd`; reference `colamd`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to colamd | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_extrinsic_insertion | mesh | auto | 5 | 1 | 2.19 ms [2.16 ms, 2.21 ms] | 2.19 ms [2.16 ms, 2.21 ms] | 45.6 [45.1, 46.1] | 1.000 [0.989, 1.015] | same |
| cohesive_extrinsic_insertion | mesh | base | 5 | 1 | 2.22 ms [2.20 ms, 2.24 ms] | 2.22 ms [2.20 ms, 2.24 ms] | 46.2 [45.8, 46.6] | 1.017 [1.008, 1.021] | same |
| cohesive_extrinsic_insertion | mesh | colamd | 5 | 1 | 2.18 ms [2.16 ms, 2.21 ms] | 2.18 ms [2.16 ms, 2.21 ms] | 45.3 [45.1, 46.1] | – | same |
| cohesive_extrinsic_insertion | init | auto | 5 | 1 | 243.74 ms [242.10 ms, 244.67 ms] | 243.74 ms [242.10 ms, 244.67 ms] | 5078.0 [5043.7, 5097.3] | 0.998 [0.989, 1.008] | same |
| cohesive_extrinsic_insertion | init | base | 5 | 1 | 243.49 ms [241.21 ms, 246.01 ms] | 243.49 ms [241.21 ms, 246.01 ms] | 5072.7 [5025.3, 5125.2] | 0.994 [0.987, 1.014] | same |
| cohesive_extrinsic_insertion | init | colamd | 5 | 1 | 242.66 ms [242.52 ms, 246.65 ms] | 242.66 ms [242.52 ms, 246.65 ms] | 5055.4 [5052.6, 5138.4] | – | same |
| cohesive_extrinsic_insertion | preload | auto | 5 | 1 | 807.08 ms [804.61 ms, 809.93 ms] | 807.08 ms [804.61 ms, 809.93 ms] | 16814.2 [16762.6, 16873.6] | 0.983 [0.981, 0.990] | same |
| cohesive_extrinsic_insertion | preload | base | 5 | 1 | 802.96 ms [801.45 ms, 812.97 ms] | 802.96 ms [801.45 ms, 812.97 ms] | 16728.4 [16696.9, 16936.8] | 0.984 [0.979, 0.986] | same |
| cohesive_extrinsic_insertion | preload | colamd | 5 | 1 | 818.78 ms [815.62 ms, 825.43 ms] | 818.78 ms [815.62 ms, 825.43 ms] | 17057.9 [16992.0, 17196.4] | – | same |
| cohesive_extrinsic_insertion | step | auto | 5 | 16 | 1.796 s [1.791 s, 1.802 s] | 112.22 ms [111.91 ms, 112.64 ms] | 2338.0 [2331.4, 2346.6] | 0.998 [0.983, 1.005] | same |
| cohesive_extrinsic_insertion | step | base | 5 | 16 | 1.790 s [1.788 s, 1.815 s] | 111.88 ms [111.78 ms, 113.42 ms] | 2330.9 [2328.7, 2362.9] | 0.997 [0.983, 1.010] | same |
| cohesive_extrinsic_insertion | step | colamd | 5 | 16 | 1.800 s [1.793 s, 1.822 s] | 112.47 ms [112.09 ms, 113.85 ms] | 2343.2 [2335.2, 2371.8] | – | same |
| cohesive_extrinsic_insertion | check_insertion | auto | 5 | 16 | 1.596 s [1.591 s, 1.603 s] | 99.77 ms [99.42 ms, 100.22 ms] | 2078.6 [2071.2, 2087.9] | 0.999 [0.981, 1.006] | same |
| cohesive_extrinsic_insertion | check_insertion | base | 5 | 16 | 1.592 s [1.589 s, 1.614 s] | 99.49 ms [99.33 ms, 100.85 ms] | 2072.7 [2069.4, 2101.1] | 0.997 [0.980, 1.010] | same |
| cohesive_extrinsic_insertion | check_insertion | colamd | 5 | 16 | 1.600 s [1.594 s, 1.622 s] | 100.01 ms [99.62 ms, 101.36 ms] | 2083.5 [2075.3, 2111.7] | – | same |
| cohesive_extrinsic_insertion | solve_step | auto | 5 | 16 | 199.40 ms [198.22 ms, 200.40 ms] | 12.46 ms [12.39 ms, 12.52 ms] | 259.6 [258.1, 260.9] | 1.000 [0.994, 1.003] | same |
| cohesive_extrinsic_insertion | solve_step | base | 5 | 16 | 198.99 ms [197.81 ms, 200.63 ms] | 12.44 ms [12.36 ms, 12.54 ms] | 259.1 [257.6, 261.2] | 0.998 [0.987, 1.008] | same |
| cohesive_extrinsic_insertion | solve_step | colamd | 5 | 16 | 199.14 ms [198.31 ms, 201.65 ms] | 12.45 ms [12.39 ms, 12.60 ms] | 259.3 [258.2, 262.6] | – | same |
| cohesive_intrinsic_implicit | mesh | auto | 5 | 1 | 459.2 µs [457.2 µs, 499.8 µs] | 459.2 µs [457.2 µs, 499.8 µs] | 127.6 [127.0, 138.8] | 1.011 [0.960, 1.101] | DIFFER (2) |
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 454.4 µs [450.9 µs, 500.3 µs] | 454.4 µs [450.9 µs, 500.3 µs] | 126.2 [125.2, 139.0] | 1.009 [0.952, 1.101] | DIFFER (2) |
| cohesive_intrinsic_implicit | mesh | colamd | 5 | 1 | 454.3 µs [450.4 µs, 476.1 µs] | 454.3 µs [450.4 µs, 476.1 µs] | 126.2 [125.1, 132.2] | – | DIFFER (2) |
| cohesive_intrinsic_implicit | init | auto | 5 | 1 | 10.85 ms [10.79 ms, 10.92 ms] | 10.85 ms [10.79 ms, 10.92 ms] | 3013.9 [2997.9, 3032.4] | 1.002 [0.990, 1.013] | DIFFER (2) |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 10.80 ms [10.77 ms, 10.86 ms] | 10.80 ms [10.77 ms, 10.86 ms] | 2998.9 [2992.4, 3015.3] | 0.995 [0.989, 1.007] | DIFFER (2) |
| cohesive_intrinsic_implicit | init | colamd | 5 | 1 | 10.83 ms [10.78 ms, 10.90 ms] | 10.83 ms [10.78 ms, 10.90 ms] | 3008.5 [2994.0, 3027.8] | – | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | auto | 5 | 30 | 1.976 s [1.970 s, 1.981 s] | 65.86 ms [65.65 ms, 66.02 ms] | 18293.8 [18236.5, 18338.4] | 0.655 [0.643, 0.659] | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 2.999 s [2.992 s, 3.040 s] | 99.96 ms [99.75 ms, 101.33 ms] | 27767.8 [27707.5, 28146.5] | 0.994 [0.975, 1.009] | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | colamd | 5 | 30 | 3.011 s [3.005 s, 3.077 s] | 100.38 ms [100.18 ms, 102.58 ms] | 27884.2 [27828.2, 28494.4] | – | DIFFER (2) |
| implicit_dynamic | mesh | auto | 5 | 1 | 820.0 µs [798.5 µs, 839.2 µs] | 820.0 µs [798.5 µs, 839.2 µs] | 49.8 [48.5, 51.0] | 0.999 [0.996, 1.033] | DIFFER (2) |
| implicit_dynamic | mesh | base | 5 | 1 | 817.7 µs [799.8 µs, 821.5 µs] | 817.7 µs [799.8 µs, 821.5 µs] | 49.7 [48.6, 49.9] | 1.015 [0.979, 1.027] | DIFFER (2) |
| implicit_dynamic | mesh | colamd | 5 | 1 | 805.7 µs [793.6 µs, 828.2 µs] | 805.7 µs [793.6 µs, 828.2 µs] | 48.9 [48.2, 50.3] | – | DIFFER (2) |
| implicit_dynamic | init | auto | 5 | 1 | 9.35 ms [9.31 ms, 9.37 ms] | 9.35 ms [9.31 ms, 9.37 ms] | 568.0 [565.7, 569.3] | 1.000 [0.986, 1.004] | DIFFER (2) |
| implicit_dynamic | init | base | 5 | 1 | 9.28 ms [9.25 ms, 9.48 ms] | 9.28 ms [9.25 ms, 9.48 ms] | 563.9 [561.6, 575.5] | 0.990 [0.982, 1.013] | DIFFER (2) |
| implicit_dynamic | init | colamd | 5 | 1 | 9.35 ms [9.34 ms, 9.47 ms] | 9.35 ms [9.34 ms, 9.47 ms] | 568.0 [567.0, 575.3] | – | DIFFER (2) |
| implicit_dynamic | mass_consistent | auto | 5 | 1 | 63.50 ms [63.28 ms, 63.88 ms] | 63.50 ms [63.28 ms, 63.88 ms] | 3856.8 [3843.5, 3880.1] | 0.994 [0.989, 1.003] | DIFFER (2) |
| implicit_dynamic | mass_consistent | base | 5 | 1 | 63.21 ms [62.84 ms, 64.98 ms] | 63.21 ms [62.84 ms, 64.98 ms] | 3839.4 [3816.9, 3946.7] | 0.991 [0.982, 1.020] | DIFFER (2) |
| implicit_dynamic | mass_consistent | colamd | 5 | 1 | 63.72 ms [63.54 ms, 64.02 ms] | 63.72 ms [63.54 ms, 64.02 ms] | 3870.0 [3859.4, 3888.6] | – | DIFFER (2) |
| implicit_dynamic | solve_step | auto | 5 | 60 | 734.57 ms [731.55 ms, 740.21 ms] | 12.24 ms [12.19 ms, 12.34 ms] | 743.6 [740.6, 749.3] | 0.710 [0.709, 0.714] | DIFFER (2) |
| implicit_dynamic | solve_step | base | 5 | 60 | 1.034 s [1.025 s, 1.053 s] | 17.23 ms [17.08 ms, 17.56 ms] | 1046.3 [1037.2, 1066.3] | 1.001 [0.993, 1.019] | DIFFER (2) |
| implicit_dynamic | solve_step | colamd | 5 | 60 | 1.033 s [1.030 s, 1.036 s] | 17.22 ms [17.17 ms, 17.27 ms] | 1045.7 [1042.6, 1048.8] | – | DIFFER (2) |
| implicit_dynamic | predictor | auto | 5 | 60 | 3.3 µs [2.4 µs, 4.4 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.868 [0.560, 1.465] | DIFFER (2) |
| implicit_dynamic | predictor | base | 5 | 60 | 3.4 µs [2.6 µs, 5.3 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.032 [0.810, 1.405] | DIFFER (2) |
| implicit_dynamic | predictor | colamd | 5 | 60 | 3.2 µs [3.0 µs, 4.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| implicit_dynamic | residual | auto | 5 | 180 | 143.20 ms [142.97 ms, 144.50 ms] | 795.6 µs [794.3 µs, 802.8 µs] | 48.3 [48.2, 48.8] | 0.994 [0.994, 1.003] | DIFFER (2) |
| implicit_dynamic | residual | base | 5 | 180 | 144.19 ms [143.21 ms, 145.16 ms] | 801.1 µs [795.6 µs, 806.5 µs] | 48.7 [48.3, 49.0] | 1.005 [0.991, 1.008] | DIFFER (2) |
| implicit_dynamic | residual | colamd | 5 | 180 | 144.03 ms [143.49 ms, 144.49 ms] | 800.1 µs [797.1 µs, 802.7 µs] | 48.6 [48.4, 48.8] | – | DIFFER (2) |
| implicit_dynamic | matrix | auto | 5 | 420 | 93.5 µs [78.4 µs, 99.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.913 [0.757, 0.970] | DIFFER (2) |
| implicit_dynamic | matrix | base | 5 | 420 | 98.3 µs [94.5 µs, 108.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.967 [0.887, 1.062] | DIFFER (2) |
| implicit_dynamic | matrix | colamd | 5 | 420 | 101.9 µs [96.4 µs, 108.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| implicit_dynamic | corrector | auto | 5 | 120 | 11.9 µs [11.4 µs, 14.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.832 [0.730, 1.008] | DIFFER (2) |
| implicit_dynamic | corrector | base | 5 | 120 | 12.4 µs [7.8 µs, 16.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.840 [0.490, 1.160] | DIFFER (2) |
| implicit_dynamic | corrector | colamd | 5 | 120 | 14.8 µs [11.8 µs, 16.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| implicit_dynamic | step_hooks | auto | 5 | 120 | 180.8 µs [178.1 µs, 201.0 µs] | 1.5 µs [1.5 µs, 1.7 µs] | 0.1 [0.1, 0.1] | 0.955 [0.893, 0.990] | DIFFER (2) |
| implicit_dynamic | step_hooks | base | 5 | 120 | 196.0 µs [179.2 µs, 205.0 µs] | 1.6 µs [1.5 µs, 1.7 µs] | 0.1 [0.1, 0.1] | 0.982 [0.883, 1.030] | DIFFER (2) |
| implicit_dynamic | step_hooks | colamd | 5 | 120 | 200.7 µs [187.1 µs, 203.1 µs] | 1.7 µs [1.6 µs, 1.7 µs] | 0.1 [0.1, 0.1] | – | DIFFER (2) |
| implicit_dynamic | step_rest | auto | 5 | 60 | 591.07 ms [587.60 ms, 595.41 ms] | 9.85 ms [9.79 ms, 9.92 ms] | 598.3 [594.8, 602.7] | 0.665 [0.663, 0.668] | DIFFER (2) |
| implicit_dynamic | step_rest | base | 5 | 60 | 889.04 ms [881.04 ms, 907.84 ms] | 14.82 ms [14.68 ms, 15.13 ms] | 900.0 [891.9, 919.0] | 1.000 [0.993, 1.021] | DIFFER (2) |
| implicit_dynamic | step_rest | colamd | 5 | 60 | 888.80 ms [885.59 ms, 891.69 ms] | 14.81 ms [14.76 ms, 14.86 ms] | 899.7 [896.5, 902.7] | – | DIFFER (2) |
| static_linear_eigen | mesh | auto | 5 | 1 | 809.7 µs [789.3 µs, 813.3 µs] | 809.7 µs [789.3 µs, 813.3 µs] | 49.2 [47.9, 49.4] | 0.991 [0.957, 0.997] | DIFFER (2) |
| static_linear_eigen | mesh | base | 5 | 1 | 789.5 µs [764.6 µs, 802.5 µs] | 789.5 µs [764.6 µs, 802.5 µs] | 48.0 [46.4, 48.7] | 0.965 [0.922, 0.990] | DIFFER (2) |
| static_linear_eigen | mesh | colamd | 5 | 1 | 818.1 µs [796.5 µs, 836.8 µs] | 818.1 µs [796.5 µs, 836.8 µs] | 49.7 [48.4, 50.8] | – | DIFFER (2) |
| static_linear_eigen | init | auto | 5 | 1 | 9.29 ms [9.20 ms, 9.40 ms] | 9.29 ms [9.20 ms, 9.40 ms] | 564.1 [558.7, 570.9] | 1.007 [0.958, 1.013] | DIFFER (2) |
| static_linear_eigen | init | base | 5 | 1 | 9.17 ms [9.03 ms, 9.20 ms] | 9.17 ms [9.03 ms, 9.20 ms] | 557.2 [548.4, 559.1] | 0.977 [0.951, 0.999] | DIFFER (2) |
| static_linear_eigen | init | colamd | 5 | 1 | 9.28 ms [9.22 ms, 9.68 ms] | 9.28 ms [9.22 ms, 9.68 ms] | 563.4 [559.8, 587.7] | – | DIFFER (2) |
| static_linear_eigen | solve_step | auto | 5 | 1 | 446.19 ms [442.11 ms, 454.26 ms] | 446.19 ms [442.11 ms, 454.26 ms] | 27101.1 [26853.0, 27591.4] | 0.631 [0.616, 0.641] | DIFFER (2) |
| static_linear_eigen | solve_step | base | 5 | 1 | 709.84 ms [707.00 ms, 711.16 ms] | 709.84 ms [707.00 ms, 711.16 ms] | 43114.6 [42942.4, 43194.9] | 1.003 [0.989, 1.011] | DIFFER (2) |
| static_linear_eigen | solve_step | colamd | 5 | 1 | 708.58 ms [700.31 ms, 717.89 ms] | 708.58 ms [700.31 ms, 717.89 ms] | 43038.3 [42535.9, 43603.5] | – | DIFFER (2) |
| static_linear_eigen | predictor | auto | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.909 [0.474, 1.000] | DIFFER (2) |
| static_linear_eigen | predictor | base | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.800 [0.421, 0.909] | DIFFER (2) |
| static_linear_eigen | predictor | colamd | 5 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| static_linear_eigen | matrix | auto | 5 | 1 | 68.79 ms [68.58 ms, 70.22 ms] | 68.79 ms [68.58 ms, 70.22 ms] | 4178.4 [4165.4, 4264.8] | 0.997 [0.987, 1.017] | DIFFER (2) |
| static_linear_eigen | matrix | base | 5 | 1 | 68.59 ms [68.36 ms, 69.02 ms] | 68.59 ms [68.36 ms, 69.02 ms] | 4166.1 [4152.0, 4192.2] | 0.998 [0.984, 1.000] | DIFFER (2) |
| static_linear_eigen | matrix | colamd | 5 | 1 | 69.01 ms [68.49 ms, 69.71 ms] | 69.01 ms [68.49 ms, 69.71 ms] | 4191.4 [4159.8, 4233.9] | – | DIFFER (2) |
| static_linear_eigen | corrector | auto | 5 | 1 | 0.8 µs [0.7 µs, 0.8 µs] | 0.8 µs [0.7 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 1.083 [0.780, 1.129] | DIFFER (2) |
| static_linear_eigen | corrector | base | 5 | 1 | 0.7 µs [0.6 µs, 1.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | 1.016 [0.580, 1.417] | DIFFER (2) |
| static_linear_eigen | corrector | colamd | 5 | 1 | 0.7 µs [0.6 µs, 1.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | – | DIFFER (2) |
| static_linear_eigen | step_hooks | auto | 5 | 2 | 4.6 µs [4.1 µs, 5.2 µs] | 2.3 µs [2.0 µs, 2.6 µs] | 0.1 [0.1, 0.2] | 0.980 [0.810, 1.158] | DIFFER (2) |
| static_linear_eigen | step_hooks | base | 5 | 2 | 4.0 µs [3.6 µs, 4.2 µs] | 2.0 µs [1.8 µs, 2.1 µs] | 0.1 [0.1, 0.1] | 0.888 [0.721, 0.978] | DIFFER (2) |
| static_linear_eigen | step_hooks | colamd | 5 | 2 | 4.7 µs [4.1 µs, 5.1 µs] | 2.4 µs [2.1 µs, 2.5 µs] | 0.1 [0.1, 0.2] | – | DIFFER (2) |
| static_linear_eigen | step_rest | auto | 5 | 1 | 376.23 ms [373.42 ms, 385.68 ms] | 376.23 ms [373.42 ms, 385.68 ms] | 22852.0 [22680.8, 23425.6] | 0.591 [0.576, 0.603] | DIFFER (2) |
| static_linear_eigen | step_rest | base | 5 | 1 | 641.18 ms [638.48 ms, 642.14 ms] | 641.18 ms [638.48 ms, 642.14 ms] | 38944.3 [38780.1, 39002.5] | 1.004 [0.989, 1.013] | DIFFER (2) |
| static_linear_eigen | step_rest | colamd | 5 | 1 | 639.57 ms [631.40 ms, 648.18 ms] | 639.57 ms [631.40 ms, 648.18 ms] | 38846.6 [38350.1, 39369.3] | – | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | auto | 5 | 3 | 32.89 ms [32.76 ms, 33.05 ms] | 10.96 ms [10.92 ms, 11.02 ms] | 666.0 [663.2, 669.1] | 1.005 [1.004, 1.014] | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 32.02 ms [31.91 ms, 32.09 ms] | 10.67 ms [10.64 ms, 10.70 ms] | 648.3 [646.1, 649.6] | 0.981 [0.975, 0.984] | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | colamd | 5 | 3 | 32.63 ms [32.59 ms, 32.76 ms] | 10.88 ms [10.86 ms, 10.92 ms] | 660.7 [659.9, 663.4] | – | DIFFER (2) |
| static_linear_eigen | analysis | auto | 5 | 1 | 86.17 ms [86.12 ms, 86.33 ms] | 86.17 ms [86.12 ms, 86.33 ms] | 5233.6 [5230.7, 5243.7] | 1.733 [1.729, 1.739] | DIFFER (2) |
| static_linear_eigen | analysis | base | 5 | 1 | 50.15 ms [50.10 ms, 50.86 ms] | 50.15 ms [50.10 ms, 50.86 ms] | 3046.3 [3043.3, 3089.1] | 1.011 [1.008, 1.022] | DIFFER (2) |
| static_linear_eigen | analysis | colamd | 5 | 1 | 49.73 ms [49.56 ms, 49.81 ms] | 49.73 ms [49.56 ms, 49.81 ms] | 3020.3 [3010.4, 3025.5] | – | DIFFER (2) |
| static_linear_eigen | factorize | auto | 5 | 2 | 461.21 ms [458.52 ms, 466.57 ms] | 230.61 ms [229.26 ms, 233.28 ms] | 14006.7 [13924.9, 14169.4] | 0.458 [0.454, 0.463] | DIFFER (2) |
| static_linear_eigen | factorize | base | 5 | 2 | 1.014 s [1.007 s, 1.049 s] | 507.06 ms [503.56 ms, 524.39 ms] | 30798.3 [30585.6, 31850.9] | 1.015 [0.998, 1.039] | DIFFER (2) |
| static_linear_eigen | factorize | colamd | 5 | 2 | 1.007 s [999.61 ms, 1.012 s] | 503.34 ms [499.80 ms, 506.22 ms] | 30572.4 [30357.3, 30747.1] | – | DIFFER (2) |
| static_linear_eigen | back_substitution | auto | 5 | 20 | 81.57 ms [80.57 ms, 82.27 ms] | 4.08 ms [4.03 ms, 4.11 ms] | 247.7 [244.7, 249.8] | 0.618 [0.615, 0.627] | DIFFER (2) |
| static_linear_eigen | back_substitution | base | 5 | 20 | 129.28 ms [128.11 ms, 132.32 ms] | 6.46 ms [6.41 ms, 6.62 ms] | 392.6 [389.1, 401.8] | 0.989 [0.979, 0.993] | DIFFER (2) |
| static_linear_eigen | back_substitution | colamd | 5 | 20 | 130.93 ms [129.49 ms, 133.22 ms] | 6.55 ms [6.47 ms, 6.66 ms] | 397.6 [393.2, 404.6] | – | DIFFER (2) |
| static_newton_plastic | mesh | auto | 5 | 1 | 315.3 µs [308.6 µs, 324.1 µs] | 315.3 µs [308.6 µs, 324.1 µs] | 52.6 [51.4, 54.0] | 1.003 [0.890, 1.041] | DIFFER (2) |
| static_newton_plastic | mesh | base | 5 | 1 | 330.9 µs [311.1 µs, 333.4 µs] | 330.9 µs [311.1 µs, 333.4 µs] | 55.1 [51.8, 55.6] | 1.028 [0.934, 1.069] | DIFFER (2) |
| static_newton_plastic | mesh | colamd | 5 | 1 | 311.3 µs [307.7 µs, 354.4 µs] | 311.3 µs [307.7 µs, 354.4 µs] | 51.9 [51.3, 59.1] | – | DIFFER (2) |
| static_newton_plastic | init | auto | 5 | 1 | 4.18 ms [4.16 ms, 4.25 ms] | 4.18 ms [4.16 ms, 4.25 ms] | 697.3 [693.8, 708.1] | 1.000 [0.997, 1.009] | DIFFER (2) |
| static_newton_plastic | init | base | 5 | 1 | 4.16 ms [4.15 ms, 4.20 ms] | 4.16 ms [4.15 ms, 4.20 ms] | 693.2 [692.1, 700.7] | 0.992 [0.989, 1.011] | DIFFER (2) |
| static_newton_plastic | init | colamd | 5 | 1 | 4.20 ms [4.16 ms, 4.21 ms] | 4.20 ms [4.16 ms, 4.21 ms] | 699.4 [693.2, 701.9] | – | DIFFER (2) |
| static_newton_plastic | solve_step | auto | 5 | 12 | 1.341 s [1.339 s, 1.351 s] | 111.78 ms [111.56 ms, 112.62 ms] | 18630.6 [18592.6, 18769.2] | 0.792 [0.786, 0.798] | DIFFER (2) |
| static_newton_plastic | solve_step | base | 5 | 12 | 1.702 s [1.679 s, 1.710 s] | 141.84 ms [139.93 ms, 142.53 ms] | 23640.6 [23321.8, 23755.4] | 1.003 [0.994, 1.009] | DIFFER (2) |
| static_newton_plastic | solve_step | colamd | 5 | 12 | 1.694 s [1.689 s, 1.707 s] | 141.19 ms [140.77 ms, 142.29 ms] | 23532.0 [23461.4, 23715.2] | – | DIFFER (2) |
| static_newton_plastic | predictor | auto | 5 | 12 | 0.7 µs [0.5 µs, 1.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.922 [0.776, 2.347] | DIFFER (2) |
| static_newton_plastic | predictor | base | 5 | 12 | 0.8 µs [0.6 µs, 0.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.408 [0.718, 1.627] | DIFFER (2) |
| static_newton_plastic | predictor | colamd | 5 | 12 | 0.6 µs [0.5 µs, 0.9 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| static_newton_plastic | residual | auto | 5 | 961 | 582.58 ms [580.52 ms, 584.41 ms] | 606.2 µs [604.1 µs, 608.1 µs] | 101.0 [100.7, 101.4] | 0.994 [0.988, 0.996] | DIFFER (2) |
| static_newton_plastic | residual | base | 5 | 961 | 587.20 ms [583.36 ms, 588.98 ms] | 611.0 µs [607.0 µs, 612.9 µs] | 101.8 [101.2, 102.1] | 0.999 [0.993, 1.007] | DIFFER (2) |
| static_newton_plastic | residual | colamd | 5 | 961 | 587.33 ms [584.64 ms, 587.86 ms] | 611.2 µs [608.4 µs, 611.7 µs] | 101.9 [101.4, 102.0] | – | DIFFER (2) |
| static_newton_plastic | matrix | auto | 5 | 949 | 244.1 µs [180.4 µs, 302.1 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.1] | 0.971 [0.717, 1.333] | DIFFER (2) |
| static_newton_plastic | matrix | base | 5 | 949 | 233.6 µs [206.8 µs, 251.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.965 [0.761, 1.101] | DIFFER (2) |
| static_newton_plastic | matrix | colamd | 5 | 949 | 251.5 µs [200.0 µs, 306.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.1] | – | DIFFER (2) |
| static_newton_plastic | corrector | auto | 5 | 949 | 65.0 µs [51.3 µs, 79.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.956 [0.760, 1.412] | DIFFER (2) |
| static_newton_plastic | corrector | base | 5 | 949 | 59.7 µs [56.2 µs, 81.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.884 [0.781, 1.448] | DIFFER (2) |
| static_newton_plastic | corrector | colamd | 5 | 949 | 68.0 µs [56.3 µs, 75.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| static_newton_plastic | step_hooks | auto | 5 | 24 | 633.3 µs [619.1 µs, 645.6 µs] | 26.4 µs [25.8 µs, 26.9 µs] | 4.4 [4.3, 4.5] | 0.983 [0.933, 1.030] | DIFFER (2) |
| static_newton_plastic | step_hooks | base | 5 | 24 | 640.9 µs [631.2 µs, 657.5 µs] | 26.7 µs [26.3 µs, 27.4 µs] | 4.5 [4.4, 4.6] | 0.995 [0.959, 1.049] | DIFFER (2) |
| static_newton_plastic | step_hooks | colamd | 5 | 24 | 634.4 µs [626.7 µs, 678.5 µs] | 26.4 µs [26.1 µs, 28.3 µs] | 4.4 [4.4, 4.7] | – | DIFFER (2) |
| static_newton_plastic | step_rest | auto | 5 | 12 | 757.81 ms [753.50 ms, 769.88 ms] | 63.15 ms [62.79 ms, 64.16 ms] | 10525.1 [10465.3, 10692.7] | 0.688 [0.675, 0.695] | DIFFER (2) |
| static_newton_plastic | step_rest | base | 5 | 12 | 1.114 s [1.095 s, 1.122 s] | 92.81 ms [91.24 ms, 93.51 ms] | 15468.1 [15206.8, 15585.3] | 1.005 [0.993, 1.013] | DIFFER (2) |
| static_newton_plastic | step_rest | colamd | 5 | 12 | 1.106 s [1.101 s, 1.122 s] | 92.20 ms [91.71 ms, 93.49 ms] | 15367.4 [15284.8, 15581.8] | – | DIFFER (2) |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_extrinsic_insertion | 48000 | 17302 | 51906 | 1 | 3b1764d2a, 53de1eeef | 0423893e61d5fb15 |
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 3b1764d2a, 53de1eeef | 88b89af4f4608998, bd166b6884da9239 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 3b1764d2a, 53de1eeef | 0bad43447a75a362, 13939e4cdc8bb8b8 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 3b1764d2a, 53de1eeef | 83c1ac7e34efcd05, d5869f57a777d9a5 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 3b1764d2a, 53de1eeef | 7bd5f114957d7d7b, e1b550d7684c485a |
