Suite `solid`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | after | 5 | 1 | 416.7 µs [324.7 µs, 435.5 µs] | 416.7 µs [324.7 µs, 435.5 µs] | 115.8 [90.2, 121.0] | 1.049 [1.004, 1.151] | same |
| cohesive_intrinsic_implicit | mesh | before | 5 | 1 | 378.5 µs [318.5 µs, 411.7 µs] | 378.5 µs [318.5 µs, 411.7 µs] | 105.1 [88.5, 114.4] | – | same |
| cohesive_intrinsic_implicit | init | after | 5 | 1 | 11.41 ms [9.59 ms, 377.11 ms] | 11.41 ms [9.59 ms, 377.11 ms] | 3168.8 [2662.6, 104752.2] | 0.924 [0.832, 11.537] | same |
| cohesive_intrinsic_implicit | init | before | 5 | 1 | 11.72 ms [10.87 ms, 32.69 ms] | 11.72 ms [10.87 ms, 32.69 ms] | 3255.1 [3020.8, 9079.7] | – | same |
| cohesive_intrinsic_implicit | solve_step | after | 5 | 30 | 6.310 s [6.253 s, 7.820 s] | 210.34 ms [208.42 ms, 260.65 ms] | 58428.8 [57895.8, 72404.1] | 0.777 [0.763, 0.827] | same |
| cohesive_intrinsic_implicit | solve_step | before | 5 | 30 | 8.273 s [8.057 s, 10.064 s] | 275.77 ms [268.57 ms, 335.45 ms] | 76603.9 [74602.5, 93181.6] | – | same |
| implicit_dynamic | mesh | after | 5 | 1 | 684.0 µs [604.0 µs, 724.3 µs] | 684.0 µs [604.0 µs, 724.3 µs] | 41.5 [36.7, 44.0] | 1.001 [0.952, 1.133] | same |
| implicit_dynamic | mesh | before | 5 | 1 | 634.7 µs [604.4 µs, 744.2 µs] | 634.7 µs [604.4 µs, 744.2 µs] | 38.5 [36.7, 45.2] | – | same |
| implicit_dynamic | init | after | 5 | 1 | 10.85 ms [10.23 ms, 12.87 ms] | 10.85 ms [10.23 ms, 12.87 ms] | 659.1 [621.7, 781.6] | 0.993 [0.853, 1.115] | same |
| implicit_dynamic | init | before | 5 | 1 | 11.54 ms [10.88 ms, 12.21 ms] | 11.54 ms [10.88 ms, 12.21 ms] | 701.1 [661.1, 741.8] | – | same |
| implicit_dynamic | mass_consistent | after | 5 | 1 | 123.83 ms [120.53 ms, 151.47 ms] | 123.83 ms [120.53 ms, 151.47 ms] | 7521.5 [7320.5, 9199.9] | 0.694 [0.655, 0.710] | same |
| implicit_dynamic | mass_consistent | before | 5 | 1 | 180.89 ms [175.34 ms, 214.59 ms] | 180.89 ms [175.34 ms, 214.59 ms] | 10986.9 [10649.9, 13034.0] | – | same |
| implicit_dynamic | solve_step | after | 5 | 60 | 1.222 s [1.172 s, 1.807 s] | 20.37 ms [19.54 ms, 30.12 ms] | 1237.4 [1186.9, 1829.7] | 0.631 [0.623, 0.669] | same |
| implicit_dynamic | solve_step | before | 5 | 60 | 1.898 s [1.878 s, 2.700 s] | 31.64 ms [31.30 ms, 45.01 ms] | 1921.7 [1900.9, 2733.6] | – | same |
| implicit_dynamic | predictor | after | 5 | 60 | 9.5 µs [7.3 µs, 11.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.810 [0.724, 0.973] | same |
| implicit_dynamic | predictor | before | 5 | 60 | 11.6 µs [9.9 µs, 11.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | residual | after | 5 | 180 | 183.55 ms [180.35 ms, 214.49 ms] | 1.02 ms [1.00 ms, 1.19 ms] | 61.9 [60.9, 72.4] | 1.001 [0.995, 1.026] | same |
| implicit_dynamic | residual | before | 5 | 180 | 181.88 ms [180.62 ms, 208.96 ms] | 1.01 ms [1.00 ms, 1.16 ms] | 61.4 [60.9, 70.5] | – | same |
| implicit_dynamic | matrix | after | 5 | 420 | 140.4 µs [139.1 µs, 159.3 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.011 [0.960, 1.040] | same |
| implicit_dynamic | matrix | before | 5 | 420 | 144.5 µs [136.8 µs, 165.8 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | after | 5 | 120 | 22.7 µs [18.9 µs, 25.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.744 [0.575, 0.864] | same |
| implicit_dynamic | corrector | before | 5 | 120 | 30.5 µs [26.3 µs, 32.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | step_hooks | after | 5 | 120 | 10.10 ms [9.95 ms, 14.73 ms] | 84.1 µs [82.9 µs, 122.8 µs] | 5.1 [5.0, 7.5] | 0.977 [0.938, 1.003] | same |
| implicit_dynamic | step_hooks | before | 5 | 120 | 10.61 ms [10.33 ms, 14.69 ms] | 88.4 µs [86.1 µs, 122.5 µs] | 5.4 [5.2, 7.4] | – | same |
| implicit_dynamic | step_rest | after | 5 | 60 | 1.029 s [981.82 ms, 1.578 s] | 17.14 ms [16.36 ms, 26.30 ms] | 1041.2 [993.9, 1597.4] | 0.590 [0.581, 0.637] | same |
| implicit_dynamic | step_rest | before | 5 | 60 | 1.706 s [1.687 s, 2.477 s] | 28.43 ms [28.11 ms, 41.28 ms] | 1726.6 [1707.3, 2507.0] | – | same |
| setup_hex8 | mesh | after | 5 | 1 | 1.45 ms [1.39 ms, 1.48 ms] | 1.45 ms [1.39 ms, 1.48 ms] | 104.8 [100.3, 106.9] | 0.908 [0.894, 1.089] | same |
| setup_hex8 | mesh | before | 5 | 1 | 1.55 ms [1.36 ms, 1.60 ms] | 1.55 ms [1.36 ms, 1.60 ms] | 112.4 [98.1, 116.0] | – | same |
| setup_hex8 | init_explicit | after | 5 | 1 | 91.94 ms [87.07 ms, 95.42 ms] | 91.94 ms [87.07 ms, 95.42 ms] | 6651.0 [6298.2, 6902.7] | 0.984 [0.969, 1.027] | same |
| setup_hex8 | init_explicit | before | 5 | 1 | 91.87 ms [89.82 ms, 97.82 ms] | 91.87 ms [89.82 ms, 97.82 ms] | 6645.9 [6497.5, 7076.0] | – | same |
| setup_hex8 | shape_functions | after | 5 | 3 | 189.31 ms [169.09 ms, 201.53 ms] | 63.10 ms [56.36 ms, 67.18 ms] | 4564.7 [4077.3, 4859.3] | 1.015 [0.977, 1.120] | same |
| setup_hex8 | shape_functions | before | 5 | 3 | 181.66 ms [167.52 ms, 186.66 ms] | 60.55 ms [55.84 ms, 62.22 ms] | 4380.4 [4039.2, 4500.8] | – | same |
| setup_hex8 | mass_lumped | after | 5 | 3 | 26.01 ms [24.83 ms, 27.30 ms] | 8.67 ms [8.28 ms, 9.10 ms] | 627.1 [598.8, 658.2] | 1.027 [0.944, 1.071] | same |
| setup_hex8 | mass_lumped | before | 5 | 3 | 25.49 ms [24.61 ms, 26.30 ms] | 8.50 ms [8.20 ms, 8.77 ms] | 614.6 [593.5, 634.1] | – | same |
| setup_hex8 | init_implicit | after | 5 | 1 | 82.76 ms [79.99 ms, 90.42 ms] | 82.76 ms [79.99 ms, 90.42 ms] | 5986.3 [5786.2, 6540.8] | 1.036 [0.943, 1.135] | same |
| setup_hex8 | init_implicit | before | 5 | 1 | 79.63 ms [77.76 ms, 93.65 ms] | 79.63 ms [77.76 ms, 93.65 ms] | 5760.6 [5625.1, 6774.1] | – | same |
| setup_hex8 | mass_consistent | after | 5 | 3 | 5.725 s [5.571 s, 6.222 s] | 1.908 s [1.857 s, 2.074 s] | 138050.5 [134320.6, 150027.6] | 0.898 [0.895, 0.943] | same |
| setup_hex8 | mass_consistent | before | 5 | 3 | 6.250 s [6.173 s, 6.596 s] | 2.083 s [2.058 s, 2.199 s] | 150715.2 [148848.4, 159043.0] | – | same |
| setup_hex8 | stiffness_assembly | after | 5 | 3 | 2.170 s [2.104 s, 2.375 s] | 723.24 ms [701.32 ms, 791.79 ms] | 52317.4 [50732.1, 57276.2] | 0.791 [0.760, 0.813] | same |
| setup_hex8 | stiffness_assembly | before | 5 | 3 | 2.688 s [2.664 s, 2.984 s] | 895.94 ms [887.86 ms, 994.57 ms] | 64810.6 [64226.0, 71945.4] | – | same |
| setup_tet10 | mesh | after | 5 | 1 | 1.30 ms [1.16 ms, 1.40 ms] | 1.30 ms [1.16 ms, 1.40 ms] | 124.9 [111.4, 135.2] | 1.152 [0.985, 1.206] | same |
| setup_tet10 | mesh | before | 5 | 1 | 1.12 ms [1.10 ms, 1.36 ms] | 1.12 ms [1.10 ms, 1.36 ms] | 107.5 [106.4, 130.9] | – | same |
| setup_tet10 | init_explicit | after | 5 | 1 | 45.71 ms [43.33 ms, 46.68 ms] | 45.71 ms [43.33 ms, 46.68 ms] | 4408.7 [4179.4, 4501.9] | 1.011 [0.992, 1.054] | same |
| setup_tet10 | init_explicit | before | 5 | 1 | 44.34 ms [43.05 ms, 45.62 ms] | 44.34 ms [43.05 ms, 45.62 ms] | 4276.6 [4151.8, 4399.8] | – | same |
| setup_tet10 | shape_functions | after | 5 | 3 | 88.95 ms [80.94 ms, 100.88 ms] | 29.65 ms [26.98 ms, 33.63 ms] | 2859.9 [2602.3, 3243.3] | 1.099 [0.994, 1.177] | same |
| setup_tet10 | shape_functions | before | 5 | 3 | 81.42 ms [80.54 ms, 85.71 ms] | 27.14 ms [26.85 ms, 28.57 ms] | 2617.6 [2589.3, 2755.6] | – | same |
| setup_tet10 | mass_lumped | after | 5 | 3 | 3.75 ms [3.47 ms, 4.21 ms] | 1.25 ms [1.16 ms, 1.40 ms] | 120.7 [111.7, 135.2] | 1.053 [0.939, 1.207] | same |
| setup_tet10 | mass_lumped | before | 5 | 3 | 3.55 ms [3.47 ms, 4.00 ms] | 1.18 ms [1.16 ms, 1.33 ms] | 114.3 [111.7, 128.6] | – | same |
| setup_tet10 | init_implicit | after | 5 | 1 | 39.40 ms [36.10 ms, 43.93 ms] | 39.40 ms [36.10 ms, 43.93 ms] | 3800.4 [3481.5, 4236.9] | 1.034 [0.880, 1.113] | same |
| setup_tet10 | init_implicit | before | 5 | 1 | 38.46 ms [37.36 ms, 41.57 ms] | 38.46 ms [37.36 ms, 41.57 ms] | 3709.5 [3603.6, 4009.7] | – | same |
| setup_tet10 | mass_consistent | after | 5 | 3 | 3.724 s [3.656 s, 3.926 s] | 1.241 s [1.219 s, 1.309 s] | 119712.7 [117529.6, 126216.7] | 0.821 [0.816, 0.826] | same |
| setup_tet10 | mass_consistent | before | 5 | 3 | 4.529 s [4.480 s, 4.753 s] | 1.510 s [1.493 s, 1.584 s] | 145595.2 [144044.2, 152808.5] | – | same |
| setup_tet10 | stiffness_assembly | after | 5 | 3 | 1.326 s [1.283 s, 1.394 s] | 442.08 ms [427.72 ms, 464.78 ms] | 42639.2 [41253.8, 44828.3] | 0.628 [0.585, 0.633] | same |
| setup_tet10 | stiffness_assembly | before | 5 | 3 | 2.124 s [2.043 s, 2.273 s] | 707.97 ms [680.89 ms, 757.77 ms] | 68283.7 [65672.1, 73087.8] | – | same |
| setup_tet4 | mesh | after | 5 | 1 | 5.28 ms [5.09 ms, 5.64 ms] | 5.28 ms [5.09 ms, 5.64 ms] | 32.6 [31.4, 34.8] | 1.102 [0.925, 1.124] | same |
| setup_tet4 | mesh | before | 5 | 1 | 4.78 ms [4.62 ms, 5.79 ms] | 4.78 ms [4.62 ms, 5.79 ms] | 29.5 [28.5, 35.7] | – | same |
| setup_tet4 | init_explicit | after | 5 | 1 | 100.67 ms [99.57 ms, 115.78 ms] | 100.67 ms [99.57 ms, 115.78 ms] | 621.4 [614.6, 714.7] | 0.962 [0.899, 0.975] | same |
| setup_tet4 | init_explicit | before | 5 | 1 | 107.89 ms [102.12 ms, 119.40 ms] | 107.89 ms [102.12 ms, 119.40 ms] | 666.0 [630.4, 737.0] | – | same |
| setup_tet4 | shape_functions | after | 5 | 3 | 172.25 ms [168.82 ms, 203.60 ms] | 57.42 ms [56.27 ms, 67.87 ms] | 354.4 [347.4, 418.9] | 0.959 [0.955, 0.999] | same |
| setup_tet4 | shape_functions | before | 5 | 3 | 179.12 ms [172.63 ms, 213.08 ms] | 59.71 ms [57.54 ms, 71.03 ms] | 368.6 [355.2, 438.4] | – | same |
| setup_tet4 | mass_lumped | after | 5 | 3 | 64.40 ms [64.21 ms, 70.27 ms] | 21.47 ms [21.40 ms, 23.42 ms] | 132.5 [132.1, 144.6] | 0.982 [0.856, 0.998] | same |
| setup_tet4 | mass_lumped | before | 5 | 3 | 65.78 ms [64.31 ms, 82.06 ms] | 21.93 ms [21.44 ms, 27.35 ms] | 135.3 [132.3, 168.9] | – | same |
| setup_tet4 | init_implicit | after | 5 | 1 | 94.85 ms [92.97 ms, 110.26 ms] | 94.85 ms [92.97 ms, 110.26 ms] | 585.5 [573.9, 680.6] | 0.950 [0.923, 0.980] | same |
| setup_tet4 | init_implicit | before | 5 | 1 | 99.88 ms [95.63 ms, 119.51 ms] | 99.88 ms [95.63 ms, 119.51 ms] | 616.5 [590.3, 737.7] | – | same |
| setup_tet4 | mass_consistent | after | 5 | 3 | 3.012 s [2.975 s, 3.406 s] | 1.004 s [991.70 ms, 1.135 s] | 6196.6 [6121.6, 7007.3] | 0.692 [0.686, 0.705] | same |
| setup_tet4 | mass_consistent | before | 5 | 3 | 4.347 s [4.318 s, 4.909 s] | 1.449 s [1.439 s, 1.636 s] | 8944.7 [8884.4, 10100.1] | – | same |
| setup_tet4 | stiffness_assembly | after | 5 | 3 | 1.343 s [1.315 s, 1.501 s] | 447.79 ms [438.27 ms, 500.19 ms] | 2764.1 [2705.4, 3087.6] | 0.514 [0.503, 0.526] | same |
| setup_tet4 | stiffness_assembly | before | 5 | 3 | 2.614 s [2.595 s, 2.854 s] | 871.40 ms [865.07 ms, 951.29 ms] | 5379.0 [5339.9, 5872.1] | – | same |
| static_linear_eigen | mesh | after | 5 | 1 | 597.9 µs [579.0 µs, 686.6 µs] | 597.9 µs [579.0 µs, 686.6 µs] | 36.3 [35.2, 41.7] | 1.001 [0.949, 1.142] | same |
| static_linear_eigen | mesh | before | 5 | 1 | 601.1 µs [578.1 µs, 630.2 µs] | 601.1 µs [578.1 µs, 630.2 µs] | 36.5 [35.1, 38.3] | – | same |
| static_linear_eigen | init | after | 5 | 1 | 10.23 ms [9.45 ms, 11.81 ms] | 10.23 ms [9.45 ms, 11.81 ms] | 621.4 [574.2, 717.6] | 0.905 [0.843, 1.073] | same |
| static_linear_eigen | init | before | 5 | 1 | 11.32 ms [10.45 ms, 12.00 ms] | 11.32 ms [10.45 ms, 12.00 ms] | 687.5 [634.6, 728.9] | – | same |
| static_linear_eigen | solve_step | after | 5 | 1 | 2.246 s [2.188 s, 2.640 s] | 2.246 s [2.188 s, 2.640 s] | 136412.2 [132920.1, 160327.3] | 0.933 [0.902, 0.970] | same |
| static_linear_eigen | solve_step | before | 5 | 1 | 2.357 s [2.336 s, 2.787 s] | 2.357 s [2.336 s, 2.787 s] | 143170.6 [141910.3, 169278.3] | – | same |
| static_linear_eigen | predictor | after | 5 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.903 [0.455, 1.337] | same |
| static_linear_eigen | predictor | before | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | matrix | after | 5 | 1 | 69.67 ms [67.68 ms, 82.38 ms] | 69.67 ms [67.68 ms, 82.38 ms] | 4231.5 [4110.7, 5003.6] | 0.531 [0.507, 0.574] | same |
| static_linear_eigen | matrix | before | 5 | 1 | 136.54 ms [121.46 ms, 150.21 ms] | 136.54 ms [121.46 ms, 150.21 ms] | 8293.0 [7377.6, 9123.6] | – | same |
| static_linear_eigen | corrector | after | 5 | 1 | 0.5 µs [0.4 µs, 0.6 µs] | 0.5 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 0.713 [0.493, 0.790] | same |
| static_linear_eigen | corrector | before | 5 | 1 | 0.7 µs [0.7 µs, 0.9 µs] | 0.7 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | step_hooks | after | 5 | 2 | 244.6 µs [152.9 µs, 252.7 µs] | 122.3 µs [76.5 µs, 126.3 µs] | 7.4 [4.6, 7.7] | 0.990 [0.607, 1.336] | same |
| static_linear_eigen | step_hooks | before | 5 | 2 | 237.0 µs [189.1 µs, 253.6 µs] | 118.5 µs [94.5 µs, 126.8 µs] | 7.2 [5.7, 7.7] | – | same |
| static_linear_eigen | step_rest | after | 5 | 1 | 2.176 s [2.121 s, 2.557 s] | 2.176 s [2.121 s, 2.557 s] | 132192.1 [128800.0, 155309.4] | 0.958 [0.925, 0.991] | same |
| static_linear_eigen | step_rest | before | 5 | 1 | 2.220 s [2.215 s, 2.637 s] | 2.220 s [2.215 s, 2.637 s] | 134821.3 [134521.2, 160140.2] | – | same |
| static_linear_eigen | stiffness_assembly | after | 5 | 3 | 105.35 ms [99.73 ms, 138.87 ms] | 35.12 ms [33.24 ms, 46.29 ms] | 2133.0 [2019.2, 2811.5] | 0.685 [0.633, 0.704] | same |
| static_linear_eigen | stiffness_assembly | before | 5 | 3 | 157.68 ms [152.35 ms, 197.20 ms] | 52.56 ms [50.78 ms, 65.73 ms] | 3192.4 [3084.5, 3992.6] | – | same |
| static_linear_eigen | analysis | after | 5 | 1 | 42.94 ms [42.07 ms, 50.08 ms] | 42.94 ms [42.07 ms, 50.08 ms] | 2608.1 [2555.3, 3041.5] | 1.044 [1.005, 1.172] | same |
| static_linear_eigen | analysis | before | 5 | 1 | 42.42 ms [40.29 ms, 49.33 ms] | 42.42 ms [40.29 ms, 49.33 ms] | 2576.7 [2447.3, 2996.4] | – | same |
| static_linear_eigen | factorize | after | 5 | 2 | 4.184 s [4.149 s, 4.875 s] | 2.092 s [2.075 s, 2.438 s] | 127050.6 [126004.9, 148051.0] | 0.997 [0.984, 1.007] | same |
| static_linear_eigen | factorize | before | 5 | 2 | 4.250 s [4.177 s, 4.843 s] | 2.125 s [2.088 s, 2.422 s] | 129067.6 [126844.6, 147087.2] | – | same |
| static_linear_eigen | back_substitution | after | 5 | 20 | 143.35 ms [139.26 ms, 212.98 ms] | 7.17 ms [6.96 ms, 10.65 ms] | 435.3 [422.9, 646.8] | 1.059 [1.008, 1.091] | same |
| static_linear_eigen | back_substitution | before | 5 | 20 | 138.15 ms [134.21 ms, 195.21 ms] | 6.91 ms [6.71 ms, 9.76 ms] | 419.5 [407.6, 592.8] | – | same |
| static_linear_faer | mesh | after | 5 | 1 | 687.7 µs [626.4 µs, 695.4 µs] | 687.7 µs [626.4 µs, 695.4 µs] | 41.8 [38.0, 42.2] | 1.034 [0.908, 1.176] | same |
| static_linear_faer | mesh | before | 5 | 1 | 613.0 µs [591.1 µs, 757.4 µs] | 613.0 µs [591.1 µs, 757.4 µs] | 37.2 [35.9, 46.0] | – | same |
| static_linear_faer | init | after | 5 | 1 | 11.92 ms [10.09 ms, 12.08 ms] | 11.92 ms [10.09 ms, 12.08 ms] | 724.3 [612.8, 734.0] | 0.931 [0.907, 1.110] | same |
| static_linear_faer | init | before | 5 | 1 | 11.38 ms [10.77 ms, 12.81 ms] | 11.38 ms [10.77 ms, 12.81 ms] | 691.4 [654.0, 778.2] | – | same |
| static_linear_faer | solve_step | after | 5 | 1 | 4.199 s [4.179 s, 4.673 s] | 4.199 s [4.179 s, 4.673 s] | 255022.7 [253805.9, 283859.0] | 0.991 [0.952, 1.022] | same |
| static_linear_faer | solve_step | before | 5 | 1 | 4.241 s [4.203 s, 4.910 s] | 4.241 s [4.203 s, 4.910 s] | 257584.9 [255301.7, 298247.7] | – | same |
| static_linear_faer | predictor | after | 5 | 1 | 0.2 µs [0.2 µs, 0.6 µs] | 0.2 µs [0.2 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.164 [0.380, 2.671] | same |
| static_linear_faer | predictor | before | 5 | 1 | 0.2 µs [0.1 µs, 0.5 µs] | 0.2 µs [0.1 µs, 0.5 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_faer | matrix | after | 5 | 1 | 73.51 ms [69.76 ms, 83.07 ms] | 73.51 ms [69.76 ms, 83.07 ms] | 4464.7 [4237.1, 5045.6] | 0.570 [0.539, 0.596] | same |
| static_linear_faer | matrix | before | 5 | 1 | 127.08 ms [123.40 ms, 154.21 ms] | 127.08 ms [123.40 ms, 154.21 ms] | 7718.6 [7494.9, 9366.3] | – | same |
| static_linear_faer | corrector | after | 5 | 1 | 0.5 µs [0.5 µs, 0.8 µs] | 0.5 µs [0.5 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 0.914 [0.638, 1.099] | same |
| static_linear_faer | corrector | before | 5 | 1 | 0.7 µs [0.6 µs, 0.8 µs] | 0.7 µs [0.6 µs, 0.8 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_faer | step_hooks | after | 5 | 2 | 259.8 µs [254.3 µs, 264.5 µs] | 129.9 µs [127.2 µs, 132.2 µs] | 7.9 [7.7, 8.0] | 1.041 [0.953, 1.058] | same |
| static_linear_faer | step_hooks | before | 5 | 2 | 251.9 µs [240.3 µs, 268.4 µs] | 125.9 µs [120.1 µs, 134.2 µs] | 7.6 [7.3, 8.2] | – | same |
| static_linear_faer | step_rest | after | 5 | 1 | 4.129 s [4.107 s, 4.590 s] | 4.129 s [4.107 s, 4.590 s] | 250769.7 [249464.6, 278797.8] | 1.003 [0.965, 1.036] | same |
| static_linear_faer | step_rest | before | 5 | 1 | 4.116 s [4.076 s, 4.756 s] | 4.116 s [4.076 s, 4.756 s] | 249977.2 [247567.8, 288865.0] | – | same |
| static_linear_faer | stiffness_assembly | after | 5 | 3 | 108.93 ms [100.72 ms, 135.15 ms] | 36.31 ms [33.57 ms, 45.05 ms] | 2205.4 [2039.2, 2736.2] | 0.595 [0.558, 0.626] | same |
| static_linear_faer | stiffness_assembly | before | 5 | 3 | 180.45 ms [174.07 ms, 242.40 ms] | 60.15 ms [58.02 ms, 80.80 ms] | 3653.4 [3524.3, 4907.7] | – | same |
| static_linear_faer | analysis | after | 5 | 1 | 51.55 ms [50.99 ms, 61.90 ms] | 51.55 ms [50.99 ms, 61.90 ms] | 3131.0 [3097.3, 3759.8] | 1.074 [0.998, 1.089] | same |
| static_linear_faer | analysis | before | 5 | 1 | 49.51 ms [47.50 ms, 56.82 ms] | 49.51 ms [47.50 ms, 56.82 ms] | 3007.4 [2885.2, 3451.0] | – | same |
| static_linear_faer | factorize | after | 5 | 2 | 8.097 s [8.046 s, 9.630 s] | 4.048 s [4.023 s, 4.815 s] | 245889.1 [244366.1, 292444.8] | 1.012 [0.976, 1.133] | same |
| static_linear_faer | factorize | before | 5 | 2 | 8.142 s [7.936 s, 8.500 s] | 4.071 s [3.968 s, 4.250 s] | 247280.4 [241018.0, 258129.7] | – | same |
| static_linear_faer | back_substitution | after | 5 | 20 | 151.68 ms [145.55 ms, 243.74 ms] | 7.58 ms [7.28 ms, 12.19 ms] | 460.7 [442.0, 740.2] | 0.984 [0.729, 1.046] | same |
| static_linear_faer | back_substitution | before | 5 | 20 | 152.48 ms [149.57 ms, 247.63 ms] | 7.62 ms [7.48 ms, 12.38 ms] | 463.1 [454.2, 752.0] | – | same |
| static_newton_plastic | mesh | after | 5 | 1 | 247.2 µs [232.9 µs, 279.3 µs] | 247.2 µs [232.9 µs, 279.3 µs] | 41.2 [38.8, 46.6] | 1.084 [0.946, 1.115] | same |
| static_newton_plastic | mesh | before | 5 | 1 | 246.3 µs [228.1 µs, 260.7 µs] | 246.3 µs [228.1 µs, 260.7 µs] | 41.0 [38.0, 43.4] | – | same |
| static_newton_plastic | init | after | 5 | 1 | 4.83 ms [4.10 ms, 16.34 ms] | 4.83 ms [4.10 ms, 16.34 ms] | 805.7 [683.0, 2722.6] | 0.943 [0.049, 1.029] | same |
| static_newton_plastic | init | before | 5 | 1 | 5.03 ms [4.39 ms, 333.40 ms] | 5.03 ms [4.39 ms, 333.40 ms] | 838.1 [732.3, 55565.9] | – | same |
| static_newton_plastic | solve_step | after | 5 | 12 | 2.613 s [2.569 s, 2.820 s] | 217.74 ms [214.07 ms, 235.04 ms] | 36290.4 [35678.6, 39173.3] | 1.021 [1.000, 1.033] | same |
| static_newton_plastic | solve_step | before | 5 | 12 | 2.545 s [2.517 s, 2.820 s] | 212.07 ms [209.75 ms, 235.00 ms] | 35345.1 [34958.3, 39167.5] | – | same |
| static_newton_plastic | predictor | after | 5 | 12 | 1.6 µs [1.6 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.840 [0.824, 0.998] | same |
| static_newton_plastic | predictor | before | 5 | 12 | 1.9 µs [1.7 µs, 2.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | residual | after | 5 | 961 | 1.003 s [991.24 ms, 1.101 s] | 1.04 ms [1.03 ms, 1.15 ms] | 174.0 [171.9, 191.0] | 0.993 [0.978, 1.006] | same |
| static_newton_plastic | residual | before | 5 | 961 | 1.005 s [991.45 ms, 1.126 s] | 1.05 ms [1.03 ms, 1.17 ms] | 174.4 [171.9, 195.4] | – | same |
| static_newton_plastic | matrix | after | 5 | 949 | 539.5 µs [531.5 µs, 569.7 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 1.044 [0.987, 1.050] | same |
| static_newton_plastic | matrix | before | 5 | 949 | 525.1 µs [510.5 µs, 542.5 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.1 [0.1, 0.1] | – | same |
| static_newton_plastic | corrector | after | 5 | 949 | 145.7 µs [137.1 µs, 170.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.728 [0.650, 0.935] | same |
| static_newton_plastic | corrector | before | 5 | 949 | 204.0 µs [182.8 µs, 211.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | step_hooks | after | 5 | 24 | 2.76 ms [2.70 ms, 3.05 ms] | 115.0 µs [112.5 µs, 126.9 µs] | 19.2 [18.8, 21.1] | 0.970 [0.933, 0.981] | same |
| static_newton_plastic | step_hooks | before | 5 | 24 | 2.84 ms [2.79 ms, 3.26 ms] | 118.5 µs [116.1 µs, 136.0 µs] | 19.7 [19.4, 22.7] | – | same |
| static_newton_plastic | step_rest | after | 5 | 12 | 1.612 s [1.574 s, 1.715 s] | 134.34 ms [131.18 ms, 142.95 ms] | 22389.5 [21863.8, 23824.4] | 1.034 [1.015, 1.056] | same |
| static_newton_plastic | step_rest | before | 5 | 12 | 1.536 s [1.522 s, 1.690 s] | 127.99 ms [126.84 ms, 140.80 ms] | 21332.3 [21139.5, 23467.0] | – | same |
| contact_implicit_static_quad4 | mesh | after | 5 | 1 | 9.76 ms [9.14 ms, 10.45 ms] | 9.76 ms [9.14 ms, 10.45 ms] | 1393.7 [1305.1, 1493.1] | 0.984 [0.885, 1.123] | same |
| contact_implicit_static_quad4 | mesh | before | 5 | 1 | 9.86 ms [9.20 ms, 11.02 ms] | 9.86 ms [9.20 ms, 11.02 ms] | 1408.6 [1314.3, 1574.9] | – | same |
| contact_implicit_static_quad4 | init | after | 5 | 1 | 9.96 ms [9.42 ms, 10.25 ms] | 9.96 ms [9.42 ms, 10.25 ms] | 1422.3 [1346.3, 1463.9] | 0.964 [0.881, 1.031] | same |
| contact_implicit_static_quad4 | init | before | 5 | 1 | 10.02 ms [9.78 ms, 11.30 ms] | 10.02 ms [9.78 ms, 11.30 ms] | 1430.9 [1397.3, 1614.7] | – | same |
| contact_implicit_static_quad4 | solve_step | after | 5 | 5 | 1.312 s [1.306 s, 1.329 s] | 262.36 ms [261.24 ms, 265.76 ms] | 37479.9 [37319.3, 37966.1] | 0.856 [0.842, 0.859] | same |
| contact_implicit_static_quad4 | solve_step | before | 5 | 5 | 1.547 s [1.528 s, 1.566 s] | 309.36 ms [305.52 ms, 313.24 ms] | 44193.6 [43646.0, 44748.3] | – | same |
| embedded_rebar_tet4 | mesh | after | 5 | 1 | 24.86 ms [23.87 ms, 27.02 ms] | 24.86 ms [23.87 ms, 27.02 ms] | 1142.9 [1097.6, 1242.1] | 1.032 [0.944, 1.126] | same |
| embedded_rebar_tet4 | mesh | before | 5 | 1 | 23.99 ms [23.70 ms, 25.40 ms] | 23.99 ms [23.70 ms, 25.40 ms] | 1103.0 [1089.7, 1167.7] | – | same |
| embedded_rebar_tet4 | init | after | 5 | 1 | 1.240 s [1.224 s, 1.280 s] | 1.240 s [1.224 s, 1.280 s] | 57004.9 [56278.5, 58870.8] | 0.988 [0.972, 1.050] | same |
| embedded_rebar_tet4 | init | before | 5 | 1 | 1.255 s [1.219 s, 1.290 s] | 1.255 s [1.219 s, 1.290 s] | 57695.2 [56066.4, 59289.7] | – | same |
| embedded_rebar_tet4 | intersect | after | 5 | 5 | 6.071 s [6.044 s, 6.577 s] | 1.214 s [1.209 s, 1.315 s] | 55826.0 [55579.7, 60476.4] | 1.003 [0.979, 1.094] | same |
| embedded_rebar_tet4 | intersect | before | 5 | 5 | 6.057 s [6.014 s, 6.175 s] | 1.211 s [1.203 s, 1.235 s] | 55693.0 [55298.4, 56784.7] | – | same |
| embedded_rebar_tet4 | solve_step | after | 5 | 1 | 3.245 s [3.241 s, 3.297 s] | 3.245 s [3.241 s, 3.297 s] | 149178.7 [149032.9, 151606.9] | 0.995 [0.980, 1.011] | same |
| embedded_rebar_tet4 | solve_step | before | 5 | 1 | 3.263 s [3.254 s, 3.318 s] | 3.263 s [3.254 s, 3.318 s] | 150000.4 [149613.1, 152535.9] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | after | 5 | 20 | 33.75 ms [32.71 ms, 36.19 ms] | 1.69 ms [1.64 ms, 1.81 ms] | 77.6 [75.2, 83.2] | 0.991 [0.925, 1.115] | same |
| embedded_rebar_tet4 | assemble_internal_forces | before | 5 | 20 | 33.00 ms [32.45 ms, 36.49 ms] | 1.65 ms [1.62 ms, 1.82 ms] | 75.9 [74.6, 83.9] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | after | 5 | 5 | 238.46 ms [234.98 ms, 252.18 ms] | 47.69 ms [47.00 ms, 50.44 ms] | 2192.7 [2160.7, 2318.9] | 0.580 [0.563, 0.609] | same |
| embedded_rebar_tet4 | assemble_stiffness | before | 5 | 5 | 414.86 ms [411.48 ms, 418.24 ms] | 82.97 ms [82.30 ms, 83.65 ms] | 3814.8 [3783.7, 3845.9] | – | same |
| embedded_rebar_tri3 | mesh | after | 5 | 1 | 23.47 ms [22.57 ms, 26.18 ms] | 23.47 ms [22.57 ms, 26.18 ms] | 931.5 [895.5, 1038.8] | 0.941 [0.897, 1.140] | same |
| embedded_rebar_tri3 | mesh | before | 5 | 1 | 23.99 ms [22.16 ms, 26.16 ms] | 23.99 ms [22.16 ms, 26.16 ms] | 951.9 [879.3, 1038.0] | – | same |
| embedded_rebar_tri3 | init | after | 5 | 1 | 1.367 s [1.304 s, 1.415 s] | 1.367 s [1.304 s, 1.415 s] | 54236.1 [51752.3, 56160.0] | 1.007 [0.960, 1.033] | same |
| embedded_rebar_tri3 | init | before | 5 | 1 | 1.359 s [1.323 s, 1.391 s] | 1.359 s [1.323 s, 1.391 s] | 53930.0 [52502.1, 55203.5] | – | same |
| embedded_rebar_tri3 | intersect | after | 5 | 5 | 6.736 s [6.545 s, 6.991 s] | 1.347 s [1.309 s, 1.398 s] | 53462.4 [51948.0, 55484.4] | 1.003 [0.958, 1.049] | same |
| embedded_rebar_tri3 | intersect | before | 5 | 5 | 6.705 s [6.506 s, 6.987 s] | 1.341 s [1.301 s, 1.397 s] | 53210.5 [51638.3, 55448.5] | – | same |
| embedded_rebar_tri3 | solve_step | after | 5 | 1 | 407.09 ms [398.13 ms, 414.26 ms] | 407.09 ms [398.13 ms, 414.26 ms] | 16154.2 [15798.7, 16438.9] | 0.898 [0.896, 0.935] | same |
| embedded_rebar_tri3 | solve_step | before | 5 | 1 | 446.36 ms [442.84 ms, 454.68 ms] | 446.36 ms [442.84 ms, 454.68 ms] | 17712.5 [17572.9, 18042.9] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | after | 5 | 20 | 42.94 ms [42.66 ms, 46.02 ms] | 2.15 ms [2.13 ms, 2.30 ms] | 85.2 [84.6, 91.3] | 1.020 [0.924, 1.092] | same |
| embedded_rebar_tri3 | assemble_internal_forces | before | 5 | 20 | 42.14 ms [42.09 ms, 46.33 ms] | 2.11 ms [2.10 ms, 2.32 ms] | 83.6 [83.5, 91.9] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | after | 5 | 5 | 87.99 ms [86.11 ms, 92.95 ms] | 17.60 ms [17.22 ms, 18.59 ms] | 698.4 [683.4, 737.7] | 0.693 [0.680, 0.732] | same |
| embedded_rebar_tri3 | assemble_stiffness | before | 5 | 5 | 126.92 ms [122.07 ms, 127.01 ms] | 25.38 ms [24.41 ms, 25.40 ms] | 1007.3 [968.8, 1008.0] | – | same |
| heat_implicit_hex8_kvar | mesh | after | 5 | 1 | 3.95 ms [3.74 ms, 4.63 ms] | 3.95 ms [3.74 ms, 4.63 ms] | 2283.6 [2166.1, 2680.6] | 0.921 [0.799, 1.107] | same |
| heat_implicit_hex8_kvar | mesh | before | 5 | 1 | 4.27 ms [4.01 ms, 4.73 ms] | 4.27 ms [4.01 ms, 4.73 ms] | 2469.3 [2318.9, 2737.8] | – | same |
| heat_implicit_hex8_kvar | init | after | 5 | 1 | 642.0 µs [606.0 µs, 785.0 µs] | 642.0 µs [606.0 µs, 785.0 µs] | 371.5 [350.7, 454.3] | 0.731 [0.686, 1.191] | same |
| heat_implicit_hex8_kvar | init | before | 5 | 1 | 829.0 µs [659.0 µs, 911.0 µs] | 829.0 µs [659.0 µs, 911.0 µs] | 479.7 [381.4, 527.2] | – | same |
| heat_implicit_hex8_kvar | solve_step | after | 5 | 5 | 1.003 s [997.70 ms, 1.009 s] | 200.55 ms [199.54 ms, 201.71 ms] | 116061.9 [115474.3, 116732.9] | 0.933 [0.913, 0.943] | same |
| heat_implicit_hex8_kvar | solve_step | before | 5 | 5 | 1.074 s [1.058 s, 1.104 s] | 214.90 ms [211.62 ms, 220.85 ms] | 124363.0 [122466.6, 127804.9] | – | same |
| heat_static_tri6 | mesh | after | 5 | 1 | 44.92 ms [43.07 ms, 46.72 ms] | 44.92 ms [43.07 ms, 46.72 ms] | 1856.1 [1779.7, 1930.7] | 1.005 [0.957, 1.091] | same |
| heat_static_tri6 | mesh | before | 5 | 1 | 44.19 ms [42.84 ms, 47.20 ms] | 44.19 ms [42.84 ms, 47.20 ms] | 1825.9 [1770.1, 1950.3] | – | same |
| heat_static_tri6 | init | after | 5 | 1 | 4.37 ms [4.09 ms, 4.41 ms] | 4.37 ms [4.09 ms, 4.41 ms] | 180.5 [169.0, 182.1] | 0.735 [0.718, 0.760] | same |
| heat_static_tri6 | init | before | 5 | 1 | 5.91 ms [5.58 ms, 6.00 ms] | 5.91 ms [5.58 ms, 6.00 ms] | 244.4 [230.5, 247.7] | – | same |
| heat_static_tri6 | assemble_conductivity | after | 5 | 5 | 70.74 ms [70.64 ms, 73.31 ms] | 14.15 ms [14.13 ms, 14.66 ms] | 584.6 [583.8, 605.9] | 0.495 [0.481, 0.521] | same |
| heat_static_tri6 | assemble_conductivity | before | 5 | 5 | 143.90 ms [140.76 ms, 146.75 ms] | 28.78 ms [28.15 ms, 29.35 ms] | 1189.3 [1163.3, 1212.8] | – | same |
| heat_static_tri6 | solve_step | after | 5 | 1 | 4.080 s [3.981 s, 4.104 s] | 4.080 s [3.981 s, 4.104 s] | 168607.2 [164505.7, 169576.4] | 1.023 [0.995, 1.039] | same |
| heat_static_tri6 | solve_step | before | 5 | 1 | 3.943 s [3.922 s, 4.100 s] | 3.943 s [3.922 s, 4.100 s] | 162914.2 [162056.7, 169414.3] | – | same |
| phasefield_staggered_tet4 | mesh | after | 5 | 1 | 7.62 ms [7.34 ms, 8.16 ms] | 7.62 ms [7.34 ms, 8.16 ms] | 1269.3 [1223.5, 1359.3] | 0.972 [0.845, 1.094] | same |
| phasefield_staggered_tet4 | mesh | before | 5 | 1 | 8.09 ms [7.33 ms, 8.76 ms] | 8.09 ms [7.33 ms, 8.76 ms] | 1348.8 [1220.8, 1459.8] | – | same |
| phasefield_staggered_tet4 | init | after | 5 | 1 | 2.02 ms [1.78 ms, 2.20 ms] | 2.02 ms [1.78 ms, 2.20 ms] | 336.0 [295.8, 366.7] | 0.880 [0.804, 0.946] | same |
| phasefield_staggered_tet4 | init | before | 5 | 1 | 2.29 ms [2.18 ms, 2.34 ms] | 2.29 ms [2.18 ms, 2.34 ms] | 381.8 [362.7, 389.7] | – | same |
| phasefield_staggered_tet4 | solid_solve | after | 5 | 4 | 1.671 s [1.667 s, 1.779 s] | 417.66 ms [416.65 ms, 444.67 ms] | 69609.4 [69442.0, 74111.5] | 0.954 [0.953, 0.994] | same |
| phasefield_staggered_tet4 | solid_solve | before | 5 | 4 | 1.752 s [1.738 s, 1.790 s] | 437.97 ms [434.59 ms, 447.41 ms] | 72995.3 [72431.0, 74568.8] | – | same |
| phasefield_staggered_tet4 | strain_transfer | after | 5 | 4 | 693.0 µs [675.0 µs, 761.0 µs] | 173.2 µs [168.8 µs, 190.2 µs] | 28.9 [28.1, 31.7] | 0.938 [0.903, 0.996] | same |
| phasefield_staggered_tet4 | strain_transfer | before | 5 | 4 | 732.0 µs [696.0 µs, 843.0 µs] | 183.0 µs [174.0 µs, 210.8 µs] | 30.5 [29.0, 35.1] | – | same |
| phasefield_staggered_tet4 | phase_solve | after | 5 | 4 | 99.87 ms [99.51 ms, 103.28 ms] | 24.97 ms [24.88 ms, 25.82 ms] | 4161.1 [4146.4, 4303.4] | 0.861 [0.854, 0.879] | same |
| phasefield_staggered_tet4 | phase_solve | before | 5 | 4 | 116.57 ms [113.55 ms, 119.95 ms] | 29.14 ms [28.39 ms, 29.99 ms] | 4857.2 [4731.2, 4998.1] | – | same |
| phasefield_staggered_tet4 | damage_transfer | after | 5 | 4 | 1.70 ms [1.62 ms, 1.85 ms] | 425.8 µs [405.0 µs, 462.2 µs] | 71.0 [67.5, 77.0] | 0.984 [0.917, 1.104] | same |
| phasefield_staggered_tet4 | damage_transfer | before | 5 | 4 | 1.68 ms [1.63 ms, 1.77 ms] | 418.8 µs [408.5 µs, 442.2 µs] | 69.8 [68.1, 73.7] | – | same |
| phasefield_staggered_tet4 | internal_forces | after | 5 | 4 | 1.71 ms [1.63 ms, 1.82 ms] | 427.2 µs [407.5 µs, 455.8 µs] | 71.2 [67.9, 76.0] | 0.967 [0.908, 0.992] | same |
| phasefield_staggered_tet4 | internal_forces | before | 5 | 4 | 1.80 ms [1.66 ms, 1.84 ms] | 451.0 µs [415.8 µs, 459.2 µs] | 75.2 [69.3, 76.5] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | after | 5 | 5 | 47.34 ms [45.52 ms, 49.40 ms] | 9.47 ms [9.10 ms, 9.88 ms] | 1578.1 [1517.4, 1646.5] | 0.523 [0.507, 0.557] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | before | 5 | 5 | 89.46 ms [87.02 ms, 90.97 ms] | 17.89 ms [17.40 ms, 18.19 ms] | 2982.1 [2900.7, 3032.2] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | after | 5 | 5 | 10.21 ms [10.10 ms, 12.32 ms] | 2.04 ms [2.02 ms, 2.46 ms] | 340.2 [336.6, 410.7] | 0.531 [0.521, 0.641] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | before | 5 | 5 | 19.21 ms [19.09 ms, 19.45 ms] | 3.84 ms [3.82 ms, 3.89 ms] | 640.4 [636.4, 648.2] | – | same |
| phasefield_staggered_tri3 | mesh | after | 5 | 1 | 48.97 ms [47.58 ms, 51.13 ms] | 48.97 ms [47.58 ms, 51.13 ms] | 956.4 [929.3, 998.6] | 1.021 [0.963, 1.033] | same |
| phasefield_staggered_tri3 | mesh | before | 5 | 1 | 48.18 ms [47.39 ms, 51.81 ms] | 48.18 ms [47.39 ms, 51.81 ms] | 941.1 [925.5, 1011.9] | – | same |
| phasefield_staggered_tri3 | init | after | 5 | 1 | 13.91 ms [12.61 ms, 14.02 ms] | 13.91 ms [12.61 ms, 14.02 ms] | 271.6 [246.2, 273.9] | 0.796 [0.764, 0.906] | same |
| phasefield_staggered_tri3 | init | before | 5 | 1 | 16.25 ms [15.47 ms, 17.64 ms] | 16.25 ms [15.47 ms, 17.64 ms] | 317.5 [302.2, 344.5] | – | same |
| phasefield_staggered_tri3 | solid_solve | after | 5 | 4 | 8.676 s [8.639 s, 8.807 s] | 2.169 s [2.160 s, 2.202 s] | 42362.9 [42184.0, 43002.3] | 0.956 [0.952, 0.966] | same |
| phasefield_staggered_tri3 | solid_solve | before | 5 | 4 | 9.109 s [9.018 s, 9.211 s] | 2.277 s [2.254 s, 2.303 s] | 44478.0 [44032.9, 44974.1] | – | same |
| phasefield_staggered_tri3 | strain_transfer | after | 5 | 4 | 4.04 ms [3.92 ms, 4.10 ms] | 1.01 ms [979.5 µs, 1.03 ms] | 19.7 [19.1, 20.0] | 0.989 [0.936, 1.018] | same |
| phasefield_staggered_tri3 | strain_transfer | before | 5 | 4 | 4.06 ms [4.02 ms, 4.33 ms] | 1.01 ms [1.00 ms, 1.08 ms] | 19.8 [19.6, 21.1] | – | same |
| phasefield_staggered_tri3 | phase_solve | after | 5 | 4 | 1.559 s [1.552 s, 1.613 s] | 389.87 ms [387.96 ms, 403.24 ms] | 7614.6 [7577.4, 7875.8] | 0.906 [0.892, 0.927] | same |
| phasefield_staggered_tri3 | phase_solve | before | 5 | 4 | 1.740 s [1.721 s, 1.744 s] | 435.07 ms [430.35 ms, 435.88 ms] | 8497.4 [8405.4, 8513.3] | – | same |
| phasefield_staggered_tri3 | damage_transfer | after | 5 | 4 | 13.08 ms [12.98 ms, 14.43 ms] | 3.27 ms [3.25 ms, 3.61 ms] | 63.9 [63.4, 70.5] | 0.984 [0.906, 1.096] | same |
| phasefield_staggered_tri3 | damage_transfer | before | 5 | 4 | 13.20 ms [13.16 ms, 14.62 ms] | 3.30 ms [3.29 ms, 3.65 ms] | 64.5 [64.3, 71.4] | – | same |
| phasefield_staggered_tri3 | internal_forces | after | 5 | 4 | 12.76 ms [12.19 ms, 13.32 ms] | 3.19 ms [3.05 ms, 3.33 ms] | 62.3 [59.5, 65.1] | 0.983 [0.938, 1.046] | same |
| phasefield_staggered_tri3 | internal_forces | before | 5 | 4 | 12.77 ms [12.72 ms, 13.71 ms] | 3.19 ms [3.18 ms, 3.43 ms] | 62.3 [62.1, 67.0] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | after | 5 | 5 | 89.91 ms [84.80 ms, 92.16 ms] | 17.98 ms [16.96 ms, 18.43 ms] | 351.2 [331.2, 360.0] | 0.423 [0.401, 0.438] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | before | 5 | 5 | 211.60 ms [208.66 ms, 215.90 ms] | 42.32 ms [41.73 ms, 43.18 ms] | 826.6 [815.1, 843.4] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | after | 5 | 5 | 71.84 ms [68.94 ms, 72.83 ms] | 14.37 ms [13.79 ms, 14.57 ms] | 280.6 [269.3, 284.5] | 0.546 [0.513, 0.560] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | before | 5 | 5 | 132.91 ms [129.12 ms, 134.50 ms] | 26.58 ms [25.82 ms, 26.90 ms] | 519.2 [504.4, 525.4] | – | same |
| structural_beam3_frame_dynamic | mesh | after | 5 | 1 | 251.0 µs [232.0 µs, 288.0 µs] | 251.0 µs [232.0 µs, 288.0 µs] | 41.3 [38.1, 47.3] | 0.921 [0.844, 1.050] | same |
| structural_beam3_frame_dynamic | mesh | before | 5 | 1 | 275.0 µs [239.0 µs, 291.0 µs] | 275.0 µs [239.0 µs, 291.0 µs] | 45.2 [39.3, 47.8] | – | same |
| structural_beam3_frame_dynamic | init | after | 5 | 1 | 24.23 ms [23.38 ms, 25.75 ms] | 24.23 ms [23.38 ms, 25.75 ms] | 3983.4 [3843.4, 4233.1] | 0.957 [0.919, 0.989] | same |
| structural_beam3_frame_dynamic | init | before | 5 | 1 | 25.67 ms [23.63 ms, 26.37 ms] | 25.67 ms [23.63 ms, 26.37 ms] | 4219.4 [3884.3, 4335.0] | – | same |
| structural_beam3_frame_dynamic | solve_step | after | 5 | 60 | 4.053 s [4.019 s, 4.162 s] | 67.56 ms [66.98 ms, 69.37 ms] | 11104.2 [11009.5, 11401.6] | 0.827 [0.817, 0.845] | same |
| structural_beam3_frame_dynamic | solve_step | before | 5 | 60 | 4.924 s [4.856 s, 5.003 s] | 82.07 ms [80.93 ms, 83.38 ms] | 13489.4 [13301.4, 13704.7] | – | same |
| structural_beam3_frame_static | mesh | after | 5 | 1 | 243.0 µs [202.0 µs, 284.0 µs] | 243.0 µs [202.0 µs, 284.0 µs] | 39.9 [33.2, 46.7] | 1.141 [0.729, 1.327] | same |
| structural_beam3_frame_static | mesh | before | 5 | 1 | 214.0 µs [213.0 µs, 277.0 µs] | 214.0 µs [213.0 µs, 277.0 µs] | 35.2 [35.0, 45.5] | – | same |
| structural_beam3_frame_static | init | after | 5 | 1 | 25.16 ms [23.98 ms, 25.81 ms] | 25.16 ms [23.98 ms, 25.81 ms] | 4135.8 [3940.8, 4241.6] | 1.051 [0.914, 1.111] | same |
| structural_beam3_frame_static | init | before | 5 | 1 | 23.47 ms [22.92 ms, 26.25 ms] | 23.47 ms [22.92 ms, 26.25 ms] | 3857.5 [3767.4, 4313.9] | – | same |
| structural_beam3_frame_static | assemble_stiffness | after | 5 | 5 | 69.36 ms [67.96 ms, 71.31 ms] | 13.87 ms [13.59 ms, 14.26 ms] | 2280.2 [2234.0, 2344.1] | 0.517 [0.511, 0.533] | same |
| structural_beam3_frame_static | assemble_stiffness | before | 5 | 5 | 133.85 ms [130.81 ms, 135.80 ms] | 26.77 ms [26.16 ms, 27.16 ms] | 4400.1 [4300.2, 4464.2] | – | same |
| structural_beam3_frame_static | solve_step | after | 5 | 1 | 4.533 s [4.510 s, 4.585 s] | 4.533 s [4.510 s, 4.585 s] | 745028.8 [741330.4, 753592.4] | 1.017 [1.016, 1.031] | same |
| structural_beam3_frame_static | solve_step | before | 5 | 1 | 4.445 s [4.396 s, 4.502 s] | 4.445 s [4.396 s, 4.502 s] | 730673.7 [722515.0, 739995.2] | – | same |
| structural_dkt18_plate_static | mesh | after | 5 | 1 | 214.0 µs [203.0 µs, 245.0 µs] | 214.0 µs [203.0 µs, 245.0 µs] | 29.7 [28.2, 34.0] | 0.939 [0.749, 1.004] | same |
| structural_dkt18_plate_static | mesh | before | 5 | 1 | 244.0 µs [225.0 µs, 271.0 µs] | 244.0 µs [225.0 µs, 271.0 µs] | 33.9 [31.2, 37.6] | – | same |
| structural_dkt18_plate_static | init | after | 5 | 1 | 61.88 ms [59.62 ms, 63.08 ms] | 61.88 ms [59.62 ms, 63.08 ms] | 8594.4 [8280.3, 8761.4] | 0.980 [0.946, 1.058] | same |
| structural_dkt18_plate_static | init | before | 5 | 1 | 63.14 ms [59.24 ms, 63.52 ms] | 63.14 ms [59.24 ms, 63.52 ms] | 8769.0 [8228.3, 8822.1] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | after | 5 | 5 | 347.34 ms [344.80 ms, 369.84 ms] | 69.47 ms [68.96 ms, 73.97 ms] | 9648.4 [9577.9, 10273.4] | 0.631 [0.613, 0.659] | same |
| structural_dkt18_plate_static | assemble_stiffness | before | 5 | 5 | 551.77 ms [546.25 ms, 574.10 ms] | 110.35 ms [109.25 ms, 114.82 ms] | 15326.9 [15173.5, 15947.3] | – | same |
| structural_dkt18_plate_static | solve_step | after | 5 | 1 | 2.345 s [2.330 s, 2.411 s] | 2.345 s [2.330 s, 2.411 s] | 325699.9 [323594.6, 334890.4] | 1.010 [1.002, 1.027] | same |
| structural_dkt18_plate_static | solve_step | before | 5 | 1 | 2.324 s [2.297 s, 2.353 s] | 2.324 s [2.297 s, 2.353 s] | 322820.0 [319003.9, 326788.1] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 59e05aa3f, 9d5f4476b | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 59e05aa3f, 9d5f4476b | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 59e05aa3f, 9d5f4476b | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 59e05aa3f, 9d5f4476b | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 59e05aa3f, 9d5f4476b | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 59e05aa3f, 9d5f4476b | 35e60e6f09e111e7 |
| static_linear_faer | 16464 | 3375 | 10125 | 1 | 59e05aa3f, 9d5f4476b | c9951bde736748ef |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 59e05aa3f, 9d5f4476b | 7bd5f114957d7d7b |
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 9d5f4476b | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 9d5f4476b | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 9d5f4476b | 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 9d5f4476b | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 9d5f4476b | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 9d5f4476b | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 9d5f4476b | a77809465b3e7c18 |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | 9d5f4476b | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 9d5f4476b | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 9d5f4476b | 1cfc7f010c52d6b3 |
