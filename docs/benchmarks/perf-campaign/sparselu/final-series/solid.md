Suite `solid`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | after | 5 | 1 | 470.4 µs [466.6 µs, 487.2 µs] | 470.4 µs [466.6 µs, 487.2 µs] | 130.7 [129.6, 135.3] | 0.996 [0.957, 1.004] | same |
| cohesive_intrinsic_implicit | mesh | before | 5 | 1 | 482.1 µs [470.3 µs, 489.0 µs] | 482.1 µs [470.3 µs, 489.0 µs] | 133.9 [130.6, 135.8] | – | same |
| cohesive_intrinsic_implicit | init | after | 5 | 1 | 15.01 ms [14.91 ms, 15.03 ms] | 15.01 ms [14.91 ms, 15.03 ms] | 4170.6 [4141.5, 4174.1] | 0.996 [0.989, 1.000] | same |
| cohesive_intrinsic_implicit | init | before | 5 | 1 | 15.03 ms [15.02 ms, 15.11 ms] | 15.03 ms [15.02 ms, 15.11 ms] | 4175.5 [4173.1, 4196.2] | – | same |
| cohesive_intrinsic_implicit | solve_step | after | 5 | 30 | 5.264 s [5.254 s, 5.322 s] | 175.47 ms [175.15 ms, 177.40 ms] | 48742.6 [48652.4, 49278.1] | 0.518 [0.517, 0.524] | same |
| cohesive_intrinsic_implicit | solve_step | before | 5 | 30 | 10.164 s [10.150 s, 10.171 s] | 338.81 ms [338.34 ms, 339.04 ms] | 94115.0 [93984.0, 94179.0] | – | same |
| implicit_dynamic | mesh | after | 5 | 1 | 796.7 µs [792.6 µs, 808.7 µs] | 796.7 µs [792.6 µs, 808.7 µs] | 48.4 [48.1, 49.1] | 0.997 [0.989, 1.008] | same |
| implicit_dynamic | mesh | before | 5 | 1 | 801.4 µs [792.5 µs, 803.8 µs] | 801.4 µs [792.5 µs, 803.8 µs] | 48.7 [48.1, 48.8] | – | same |
| implicit_dynamic | init | after | 5 | 1 | 13.91 ms [13.82 ms, 14.03 ms] | 13.91 ms [13.82 ms, 14.03 ms] | 845.0 [839.4, 852.0] | 0.993 [0.991, 1.003] | same |
| implicit_dynamic | init | before | 5 | 1 | 13.98 ms [13.95 ms, 14.01 ms] | 13.98 ms [13.95 ms, 14.01 ms] | 848.9 [847.3, 851.2] | – | same |
| implicit_dynamic | mass_consistent | after | 5 | 1 | 230.29 ms [229.47 ms, 232.11 ms] | 230.29 ms [229.47 ms, 232.11 ms] | 13987.4 [13937.6, 14097.9] | 0.991 [0.968, 1.003] | same |
| implicit_dynamic | mass_consistent | before | 5 | 1 | 231.88 ms [231.25 ms, 236.98 ms] | 231.88 ms [231.25 ms, 236.98 ms] | 14084.0 [14045.8, 14393.8] | – | same |
| implicit_dynamic | solve_step | after | 5 | 60 | 2.315 s [2.309 s, 2.319 s] | 38.59 ms [38.49 ms, 38.64 ms] | 2343.8 [2337.8, 2347.2] | 0.856 [0.848, 0.867] | same |
| implicit_dynamic | solve_step | before | 5 | 60 | 2.708 s [2.670 s, 2.731 s] | 45.13 ms [44.50 ms, 45.51 ms] | 2741.0 [2702.8, 2764.3] | – | same |
| implicit_dynamic | predictor | after | 5 | 60 | 6.3 µs [4.8 µs, 9.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.699 [0.513, 0.956] | same |
| implicit_dynamic | predictor | before | 5 | 60 | 9.3 µs [6.5 µs, 12.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | residual | after | 5 | 180 | 198.46 ms [198.27 ms, 199.39 ms] | 1.10 ms [1.10 ms, 1.11 ms] | 67.0 [66.9, 67.3] | 0.992 [0.986, 0.998] | same |
| implicit_dynamic | residual | before | 5 | 180 | 200.15 ms [198.57 ms, 201.02 ms] | 1.11 ms [1.10 ms, 1.12 ms] | 67.5 [67.0, 67.8] | – | same |
| implicit_dynamic | matrix | after | 5 | 420 | 133.2 µs [129.3 µs, 138.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.962 [0.847, 1.033] | same |
| implicit_dynamic | matrix | before | 5 | 420 | 140.6 µs [127.0 µs, 157.3 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | after | 5 | 120 | 19.2 µs [13.3 µs, 33.4 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.764 [0.551, 1.650] | same |
| implicit_dynamic | corrector | before | 5 | 120 | 20.6 µs [18.1 µs, 43.7 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | step_hooks | after | 5 | 120 | 6.78 ms [6.73 ms, 6.89 ms] | 56.5 µs [56.1 µs, 57.4 µs] | 3.4 [3.4, 3.5] | 0.985 [0.961, 1.005] | same |
| implicit_dynamic | step_hooks | before | 5 | 120 | 6.85 ms [6.75 ms, 7.05 ms] | 57.1 µs [56.3 µs, 58.8 µs] | 3.5 [3.4, 3.6] | – | same |
| implicit_dynamic | step_rest | after | 5 | 60 | 2.110 s [2.104 s, 2.112 s] | 35.17 ms [35.07 ms, 35.20 ms] | 2136.1 [2129.8, 2138.2] | 0.845 [0.837, 0.856] | same |
| implicit_dynamic | step_rest | before | 5 | 60 | 2.501 s [2.464 s, 2.522 s] | 41.68 ms [41.07 ms, 42.04 ms] | 2531.3 [2494.8, 2553.4] | – | same |
| static_linear_eigen | mesh | after | 5 | 1 | 801.9 µs [784.4 µs, 831.3 µs] | 801.9 µs [784.4 µs, 831.3 µs] | 48.7 [47.6, 50.5] | 0.987 [0.981, 1.052] | same |
| static_linear_eigen | mesh | before | 5 | 1 | 799.4 µs [790.3 µs, 827.4 µs] | 799.4 µs [790.3 µs, 827.4 µs] | 48.6 [48.0, 50.3] | – | same |
| static_linear_eigen | init | after | 5 | 1 | 13.76 ms [13.74 ms, 13.92 ms] | 13.76 ms [13.74 ms, 13.92 ms] | 835.6 [834.3, 845.7] | 0.996 [0.988, 1.011] | same |
| static_linear_eigen | init | before | 5 | 1 | 13.82 ms [13.78 ms, 13.91 ms] | 13.82 ms [13.78 ms, 13.91 ms] | 839.7 [836.9, 844.7] | – | same |
| static_linear_eigen | solve_step | after | 5 | 1 | 872.21 ms [864.39 ms, 877.47 ms] | 872.21 ms [864.39 ms, 877.47 ms] | 52977.0 [52502.0, 53296.5] | 0.267 [0.264, 0.269] | same |
| static_linear_eigen | solve_step | before | 5 | 1 | 3.270 s [3.249 s, 3.273 s] | 3.270 s [3.249 s, 3.273 s] | 198601.7 [197366.0, 198781.6] | – | same |
| static_linear_eigen | predictor | after | 5 | 1 | 0.3 µs [0.1 µs, 0.4 µs] | 0.3 µs [0.1 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.240 [0.846, 2.667] | same |
| static_linear_eigen | predictor | before | 5 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | matrix | after | 5 | 1 | 161.62 ms [160.97 ms, 161.94 ms] | 161.62 ms [160.97 ms, 161.94 ms] | 9816.6 [9777.0, 9836.3] | 0.996 [0.995, 0.999] | same |
| static_linear_eigen | matrix | before | 5 | 1 | 161.92 ms [161.21 ms, 162.77 ms] | 161.92 ms [161.21 ms, 162.77 ms] | 9834.9 [9791.7, 9886.5] | – | same |
| static_linear_eigen | corrector | after | 5 | 1 | 0.7 µs [0.7 µs, 0.9 µs] | 0.7 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 0.947 [0.726, 1.234] | same |
| static_linear_eigen | corrector | before | 5 | 1 | 0.8 µs [0.7 µs, 0.9 µs] | 0.8 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | step_hooks | after | 5 | 2 | 110.9 µs [79.0 µs, 112.1 µs] | 55.5 µs [39.5 µs, 56.0 µs] | 3.4 [2.4, 3.4] | 0.993 [0.712, 1.427] | same |
| static_linear_eigen | step_hooks | before | 5 | 2 | 111.0 µs [71.8 µs, 117.5 µs] | 55.5 µs [35.9 µs, 58.7 µs] | 3.4 [2.2, 3.6] | – | same |
| static_linear_eigen | step_rest | after | 5 | 1 | 710.16 ms [702.66 ms, 715.76 ms] | 710.16 ms [702.66 ms, 715.76 ms] | 43133.9 [42678.6, 43474.4] | 0.229 [0.226, 0.231] | same |
| static_linear_eigen | step_rest | before | 5 | 1 | 3.107 s [3.088 s, 3.111 s] | 3.107 s [3.088 s, 3.111 s] | 188728.9 [187570.0, 188939.5] | – | same |
| static_linear_eigen | stiffness_assembly | after | 5 | 3 | 257.35 ms [255.61 ms, 265.15 ms] | 85.78 ms [85.20 ms, 88.38 ms] | 5210.4 [5175.2, 5368.3] | 1.298 [1.291, 1.342] | same |
| static_linear_eigen | stiffness_assembly | before | 5 | 3 | 198.07 ms [197.51 ms, 198.77 ms] | 66.02 ms [65.84 ms, 66.26 ms] | 4010.1 [3998.9, 4024.4] | – | same |
| static_linear_eigen | analysis | after | 5 | 1 | 50.34 ms [50.05 ms, 50.52 ms] | 50.34 ms [50.05 ms, 50.52 ms] | 3057.9 [3039.8, 3068.8] | 1.104 [1.087, 1.108] | same |
| static_linear_eigen | analysis | before | 5 | 1 | 45.44 ms [45.33 ms, 46.47 ms] | 45.44 ms [45.33 ms, 46.47 ms] | 2760.1 [2753.2, 2822.5] | – | same |
| static_linear_eigen | factorize | after | 5 | 2 | 1.028 s [1.024 s, 1.065 s] | 514.17 ms [512.17 ms, 532.39 ms] | 31230.2 [31108.5, 32336.8] | 0.174 [0.173, 0.179] | same |
| static_linear_eigen | factorize | before | 5 | 2 | 5.934 s [5.917 s, 5.946 s] | 2.967 s [2.959 s, 2.973 s] | 180197.2 [179695.6, 180562.6] | – | same |
| static_linear_eigen | back_substitution | after | 5 | 20 | 130.28 ms [129.78 ms, 132.14 ms] | 6.51 ms [6.49 ms, 6.61 ms] | 395.6 [394.1, 401.3] | 0.638 [0.631, 0.644] | same |
| static_linear_eigen | back_substitution | before | 5 | 20 | 205.65 ms [201.52 ms, 207.99 ms] | 10.28 ms [10.08 ms, 10.40 ms] | 624.5 [612.0, 631.7] | – | same |
| static_newton_plastic | mesh | after | 5 | 1 | 316.9 µs [312.6 µs, 333.0 µs] | 316.9 µs [312.6 µs, 333.0 µs] | 52.8 [52.1, 55.5] | 1.016 [0.948, 1.058] | same |
| static_newton_plastic | mesh | before | 5 | 1 | 314.7 µs [311.8 µs, 332.8 µs] | 314.7 µs [311.8 µs, 332.8 µs] | 52.4 [52.0, 55.5] | – | same |
| static_newton_plastic | init | after | 5 | 1 | 5.88 ms [5.86 ms, 5.91 ms] | 5.88 ms [5.86 ms, 5.91 ms] | 979.3 [976.0, 984.2] | 0.994 [0.994, 1.003] | same |
| static_newton_plastic | init | before | 5 | 1 | 5.90 ms [5.89 ms, 5.92 ms] | 5.90 ms [5.89 ms, 5.92 ms] | 983.6 [981.0, 986.3] | – | same |
| static_newton_plastic | solve_step | after | 5 | 12 | 2.468 s [2.466 s, 2.487 s] | 205.69 ms [205.51 ms, 207.23 ms] | 34282.3 [34251.1, 34538.3] | 0.799 [0.794, 0.803] | same |
| static_newton_plastic | solve_step | before | 5 | 12 | 3.094 s [3.083 s, 3.108 s] | 257.79 ms [256.89 ms, 259.02 ms] | 42965.6 [42815.3, 43170.3] | – | same |
| static_newton_plastic | predictor | after | 5 | 12 | 1.3 µs [0.9 µs, 1.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.927 [0.503, 1.200] | same |
| static_newton_plastic | predictor | before | 5 | 12 | 1.4 µs [1.1 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | residual | after | 5 | 961 | 1.210 s [1.200 s, 1.212 s] | 1.26 ms [1.25 ms, 1.26 ms] | 209.8 [208.1, 210.3] | 0.992 [0.988, 0.994] | same |
| static_newton_plastic | residual | before | 5 | 961 | 1.217 s [1.215 s, 1.221 s] | 1.27 ms [1.26 ms, 1.27 ms] | 211.1 [210.7, 211.7] | – | same |
| static_newton_plastic | matrix | after | 5 | 949 | 247.8 µs [241.2 µs, 263.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.858 [0.719, 1.133] | same |
| static_newton_plastic | matrix | before | 5 | 949 | 288.7 µs [213.5 µs, 351.8 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.1 [0.0, 0.1] | – | same |
| static_newton_plastic | corrector | after | 5 | 949 | 76.5 µs [67.3 µs, 92.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.836 [0.626, 1.169] | same |
| static_newton_plastic | corrector | before | 5 | 949 | 86.1 µs [79.4 µs, 113.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | step_hooks | after | 5 | 24 | 3.33 ms [3.32 ms, 3.36 ms] | 139.0 µs [138.4 µs, 140.2 µs] | 23.2 [23.1, 23.4] | 0.994 [0.984, 1.012] | same |
| static_newton_plastic | step_hooks | before | 5 | 24 | 3.35 ms [3.33 ms, 3.39 ms] | 139.6 µs [138.6 µs, 141.2 µs] | 23.3 [23.1, 23.5] | – | same |
| static_newton_plastic | step_rest | after | 5 | 12 | 1.256 s [1.252 s, 1.273 s] | 104.63 ms [104.30 ms, 106.12 ms] | 17438.0 [17384.1, 17686.0] | 0.674 [0.665, 0.679] | same |
| static_newton_plastic | step_rest | before | 5 | 12 | 1.875 s [1.858 s, 1.887 s] | 156.27 ms [154.85 ms, 157.26 ms] | 26044.6 [25807.7, 26209.4] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 24f73b94e, f204c7196 | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 24f73b94e, f204c7196 | 13939e4cdc8bb8b8 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 24f73b94e, f204c7196 | d5869f57a777d9a5 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 24f73b94e, f204c7196 | 7bd5f114957d7d7b |
