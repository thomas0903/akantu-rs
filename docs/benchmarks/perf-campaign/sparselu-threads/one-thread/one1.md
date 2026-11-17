Suite `solid`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | base | 7 | 1 | 453.6 µs [449.6 µs, 476.7 µs] | 453.6 µs [449.6 µs, 476.7 µs] | 126.0 [124.9, 132.4] | – | same |
| cohesive_intrinsic_implicit | mesh | head | 7 | 1 | 454.7 µs [447.6 µs, 467.2 µs] | 454.7 µs [447.6 µs, 467.2 µs] | 126.3 [124.3, 129.8] | 1.003 [0.957, 1.039] | same |
| cohesive_intrinsic_implicit | init | base | 7 | 1 | 10.80 ms [10.74 ms, 10.90 ms] | 10.80 ms [10.74 ms, 10.90 ms] | 2998.6 [2982.0, 3026.6] | – | same |
| cohesive_intrinsic_implicit | init | head | 7 | 1 | 10.82 ms [10.74 ms, 10.97 ms] | 10.82 ms [10.74 ms, 10.97 ms] | 3005.4 [2983.1, 3047.8] | 1.000 [0.993, 1.020] | same |
| cohesive_intrinsic_implicit | solve_step | base | 7 | 30 | 3.057 s [3.042 s, 3.090 s] | 101.88 ms [101.39 ms, 102.99 ms] | 28301.1 [28163.4, 28608.3] | – | same |
| cohesive_intrinsic_implicit | solve_step | head | 7 | 30 | 3.064 s [3.052 s, 3.078 s] | 102.12 ms [101.74 ms, 102.59 ms] | 28366.4 [28261.1, 28496.9] | 1.002 [0.991, 1.006] | same |
| implicit_dynamic | mesh | base | 7 | 1 | 802.6 µs [799.2 µs, 831.7 µs] | 802.6 µs [799.2 µs, 831.7 µs] | 48.8 [48.5, 50.5] | – | same |
| implicit_dynamic | mesh | head | 7 | 1 | 800.3 µs [790.6 µs, 811.3 µs] | 800.3 µs [790.6 µs, 811.3 µs] | 48.6 [48.0, 49.3] | 0.992 [0.964, 1.011] | same |
| implicit_dynamic | init | base | 7 | 1 | 9.26 ms [9.22 ms, 9.31 ms] | 9.26 ms [9.22 ms, 9.31 ms] | 562.2 [560.3, 565.7] | – | same |
| implicit_dynamic | init | head | 7 | 1 | 9.30 ms [9.25 ms, 9.38 ms] | 9.30 ms [9.25 ms, 9.38 ms] | 564.6 [561.6, 569.9] | 1.001 [0.998, 1.015] | same |
| implicit_dynamic | mass_consistent | base | 7 | 1 | 63.64 ms [63.02 ms, 63.82 ms] | 63.64 ms [63.02 ms, 63.82 ms] | 3865.6 [3828.0, 3876.6] | – | same |
| implicit_dynamic | mass_consistent | head | 7 | 1 | 63.14 ms [62.78 ms, 64.42 ms] | 63.14 ms [62.78 ms, 64.42 ms] | 3834.9 [3813.4, 3912.7] | 0.997 [0.986, 1.012] | same |
| implicit_dynamic | solve_step | base | 7 | 60 | 1.034 s [1.032 s, 1.038 s] | 17.24 ms [17.20 ms, 17.30 ms] | 1046.9 [1044.4, 1050.7] | – | same |
| implicit_dynamic | solve_step | head | 7 | 60 | 1.032 s [1.029 s, 1.047 s] | 17.20 ms [17.15 ms, 17.46 ms] | 1044.8 [1041.5, 1060.3] | 0.997 [0.994, 1.014] | same |
| implicit_dynamic | predictor | base | 7 | 60 | 2.5 µs [2.3 µs, 3.5 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | predictor | head | 7 | 60 | 3.8 µs [2.5 µs, 8.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.645 [0.700, 2.909] | same |
| implicit_dynamic | residual | base | 7 | 180 | 144.87 ms [144.56 ms, 145.65 ms] | 804.8 µs [803.1 µs, 809.2 µs] | 48.9 [48.8, 49.1] | – | same |
| implicit_dynamic | residual | head | 7 | 180 | 144.39 ms [144.08 ms, 145.48 ms] | 802.1 µs [800.5 µs, 808.2 µs] | 48.7 [48.6, 49.1] | 0.997 [0.991, 1.002] | same |
| implicit_dynamic | matrix | base | 7 | 420 | 113.2 µs [106.2 µs, 117.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | matrix | head | 7 | 420 | 100.7 µs [95.7 µs, 112.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.888 [0.833, 0.997] | same |
| implicit_dynamic | corrector | base | 7 | 120 | 15.0 µs [10.8 µs, 19.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | head | 7 | 120 | 16.8 µs [13.7 µs, 26.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.117 [0.793, 1.805] | same |
| implicit_dynamic | step_hooks | base | 7 | 120 | 192.3 µs [181.4 µs, 206.1 µs] | 1.6 µs [1.5 µs, 1.7 µs] | 0.1 [0.1, 0.1] | – | same |
| implicit_dynamic | step_hooks | head | 7 | 120 | 183.0 µs [180.3 µs, 195.3 µs] | 1.5 µs [1.5 µs, 1.6 µs] | 0.1 [0.1, 0.1] | 0.939 [0.890, 1.065] | same |
| implicit_dynamic | step_rest | base | 7 | 60 | 889.26 ms [886.51 ms, 892.19 ms] | 14.82 ms [14.78 ms, 14.87 ms] | 900.2 [897.4, 903.2] | – | same |
| implicit_dynamic | step_rest | head | 7 | 60 | 887.30 ms [883.32 ms, 902.66 ms] | 14.79 ms [14.72 ms, 15.04 ms] | 898.2 [894.2, 913.8] | 0.998 [0.994, 1.016] | same |
| static_linear_eigen | mesh | base | 7 | 1 | 800.0 µs [783.4 µs, 815.6 µs] | 800.0 µs [783.4 µs, 815.6 µs] | 48.6 [47.6, 49.5] | – | same |
| static_linear_eigen | mesh | head | 7 | 1 | 805.6 µs [782.9 µs, 811.1 µs] | 805.6 µs [782.9 µs, 811.1 µs] | 48.9 [47.6, 49.3] | 1.006 [0.960, 1.019] | same |
| static_linear_eigen | init | base | 7 | 1 | 9.22 ms [9.15 ms, 9.49 ms] | 9.22 ms [9.15 ms, 9.49 ms] | 560.1 [556.0, 576.3] | – | same |
| static_linear_eigen | init | head | 7 | 1 | 9.18 ms [9.11 ms, 9.26 ms] | 9.18 ms [9.11 ms, 9.26 ms] | 557.6 [553.1, 562.2] | 0.994 [0.964, 1.011] | same |
| static_linear_eigen | solve_step | base | 7 | 1 | 717.75 ms [709.80 ms, 724.24 ms] | 717.75 ms [709.80 ms, 724.24 ms] | 43595.0 [43112.4, 43989.3] | – | same |
| static_linear_eigen | solve_step | head | 7 | 1 | 726.94 ms [723.39 ms, 733.24 ms] | 726.94 ms [723.39 ms, 733.24 ms] | 44153.2 [43937.8, 44536.0] | 1.017 [1.000, 1.027] | same |
| static_linear_eigen | predictor | base | 7 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head | 7 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.000 [0.476, 2.750] | same |
| static_linear_eigen | matrix | base | 7 | 1 | 68.68 ms [68.25 ms, 70.31 ms] | 68.68 ms [68.25 ms, 70.31 ms] | 4171.8 [4145.4, 4270.3] | – | same |
| static_linear_eigen | matrix | head | 7 | 1 | 68.44 ms [68.15 ms, 68.60 ms] | 68.44 ms [68.15 ms, 68.60 ms] | 4157.1 [4139.2, 4166.9] | 0.997 [0.976, 1.003] | same |
| static_linear_eigen | corrector | base | 7 | 1 | 0.7 µs [0.6 µs, 1.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | corrector | head | 7 | 1 | 0.6 µs [0.6 µs, 0.7 µs] | 0.6 µs [0.6 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 1.018 [0.588, 1.327] | same |
| static_linear_eigen | step_hooks | base | 7 | 2 | 4.2 µs [3.7 µs, 4.8 µs] | 2.1 µs [1.8 µs, 2.4 µs] | 0.1 [0.1, 0.1] | – | same |
| static_linear_eigen | step_hooks | head | 7 | 2 | 4.2 µs [3.9 µs, 4.8 µs] | 2.1 µs [1.9 µs, 2.4 µs] | 0.1 [0.1, 0.1] | 1.017 [0.800, 1.300] | same |
| static_linear_eigen | step_rest | base | 7 | 1 | 647.44 ms [641.18 ms, 655.99 ms] | 647.44 ms [641.18 ms, 655.99 ms] | 39324.4 [38944.2, 39843.6] | – | same |
| static_linear_eigen | step_rest | head | 7 | 1 | 658.56 ms [654.94 ms, 664.63 ms] | 658.56 ms [654.94 ms, 664.63 ms] | 39999.8 [39780.4, 40368.8] | 1.019 [1.000, 1.031] | same |
| static_linear_eigen | stiffness_assembly | base | 7 | 3 | 39.50 ms [39.39 ms, 40.11 ms] | 13.17 ms [13.13 ms, 13.37 ms] | 799.8 [797.4, 812.1] | – | same |
| static_linear_eigen | stiffness_assembly | head | 7 | 3 | 39.61 ms [39.37 ms, 40.18 ms] | 13.20 ms [13.12 ms, 13.39 ms] | 802.0 [797.2, 813.4] | 1.000 [0.996, 1.019] | same |
| static_linear_eigen | analysis | base | 7 | 1 | 48.14 ms [47.93 ms, 48.48 ms] | 48.14 ms [47.93 ms, 48.48 ms] | 2923.7 [2911.2, 2944.3] | – | same |
| static_linear_eigen | analysis | head | 7 | 1 | 48.53 ms [48.25 ms, 48.86 ms] | 48.53 ms [48.25 ms, 48.86 ms] | 2947.4 [2930.6, 2967.6] | 1.005 [1.001, 1.014] | same |
| static_linear_eigen | factorize | base | 7 | 2 | 1.027 s [1.021 s, 1.031 s] | 513.59 ms [510.62 ms, 515.46 ms] | 31195.0 [31014.1, 31308.0] | – | same |
| static_linear_eigen | factorize | head | 7 | 2 | 1.049 s [1.043 s, 1.051 s] | 524.58 ms [521.40 ms, 525.27 ms] | 31862.1 [31669.0, 31904.4] | 1.021 [1.018, 1.023] | same |
| static_linear_eigen | back_substitution | base | 7 | 20 | 130.69 ms [130.01 ms, 131.54 ms] | 6.53 ms [6.50 ms, 6.58 ms] | 396.9 [394.8, 399.5] | – | same |
| static_linear_eigen | back_substitution | head | 7 | 20 | 130.93 ms [129.68 ms, 131.24 ms] | 6.55 ms [6.48 ms, 6.56 ms] | 397.6 [393.8, 398.6] | 0.998 [0.992, 1.008] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 87dd2d994, d404f4c70 | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 87dd2d994, d404f4c70 | 13939e4cdc8bb8b8 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 87dd2d994, d404f4c70 | d5869f57a777d9a5 |
