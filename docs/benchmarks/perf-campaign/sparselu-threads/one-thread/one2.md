Suite `solid`, size `m`; variants: `base`, `head`, `vB`, `vC`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | mesh | base | 9 | 1 | 812.2 µs [777.4 µs, 816.4 µs] | 812.2 µs [777.4 µs, 816.4 µs] | 49.3 [47.2, 49.6] | – | same |
| static_linear_eigen | mesh | head | 9 | 1 | 810.3 µs [795.5 µs, 820.6 µs] | 810.3 µs [795.5 µs, 820.6 µs] | 49.2 [48.3, 49.8] | 1.007 [0.978, 1.035] | same |
| static_linear_eigen | mesh | vB | 9 | 1 | 796.7 µs [788.5 µs, 826.6 µs] | 796.7 µs [788.5 µs, 826.6 µs] | 48.4 [47.9, 50.2] | 0.981 [0.969, 1.063] | same |
| static_linear_eigen | mesh | vC | 9 | 1 | 810.9 µs [790.9 µs, 816.1 µs] | 810.9 µs [790.9 µs, 816.1 µs] | 49.2 [48.0, 49.6] | 0.999 [0.969, 1.043] | same |
| static_linear_eigen | init | base | 9 | 1 | 9.21 ms [9.17 ms, 9.31 ms] | 9.21 ms [9.17 ms, 9.31 ms] | 559.7 [557.1, 565.3] | – | same |
| static_linear_eigen | init | head | 9 | 1 | 9.21 ms [9.18 ms, 9.37 ms] | 9.21 ms [9.18 ms, 9.37 ms] | 559.6 [557.8, 569.0] | 1.001 [0.996, 1.008] | same |
| static_linear_eigen | init | vB | 9 | 1 | 9.20 ms [9.15 ms, 9.43 ms] | 9.20 ms [9.15 ms, 9.43 ms] | 558.7 [556.0, 573.0] | 0.998 [0.984, 1.023] | same |
| static_linear_eigen | init | vC | 9 | 1 | 9.19 ms [9.13 ms, 9.25 ms] | 9.19 ms [9.13 ms, 9.25 ms] | 558.3 [554.8, 561.9] | 0.996 [0.985, 1.004] | same |
| static_linear_eigen | solve_step | base | 9 | 1 | 715.47 ms [708.97 ms, 722.20 ms] | 715.47 ms [708.97 ms, 722.20 ms] | 43456.4 [43061.7, 43865.6] | – | same |
| static_linear_eigen | solve_step | head | 9 | 1 | 725.23 ms [718.39 ms, 732.34 ms] | 725.23 ms [718.39 ms, 732.34 ms] | 44049.5 [43633.9, 44481.6] | 1.015 [1.004, 1.027] | same |
| static_linear_eigen | solve_step | vB | 9 | 1 | 729.12 ms [720.42 ms, 734.64 ms] | 729.12 ms [720.42 ms, 734.64 ms] | 44285.9 [43757.3, 44621.1] | 1.020 [1.009, 1.030] | same |
| static_linear_eigen | solve_step | vC | 9 | 1 | 717.91 ms [714.37 ms, 723.33 ms] | 717.91 ms [714.37 ms, 723.33 ms] | 43605.1 [43390.0, 43933.8] | 1.004 [0.998, 1.017] | same |
| static_linear_eigen | predictor | base | 9 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head | 9 | 1 | 0.1 µs [0.1 µs, 0.3 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.125 [0.474, 2.889] | same |
| static_linear_eigen | predictor | vB | 9 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.889 [0.421, 1.667] | same |
| static_linear_eigen | predictor | vC | 9 | 1 | 0.1 µs [0.1 µs, 0.3 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.000 [0.421, 3.222] | same |
| static_linear_eigen | matrix | base | 9 | 1 | 69.09 ms [68.43 ms, 69.35 ms] | 69.09 ms [68.43 ms, 69.35 ms] | 4196.2 [4156.1, 4212.1] | – | same |
| static_linear_eigen | matrix | head | 9 | 1 | 68.73 ms [68.42 ms, 69.17 ms] | 68.73 ms [68.42 ms, 69.17 ms] | 4174.7 [4156.0, 4201.1] | 0.994 [0.989, 1.004] | same |
| static_linear_eigen | matrix | vB | 9 | 1 | 68.47 ms [68.05 ms, 68.98 ms] | 68.47 ms [68.05 ms, 68.98 ms] | 4158.8 [4133.2, 4189.9] | 0.992 [0.982, 1.001] | same |
| static_linear_eigen | matrix | vC | 9 | 1 | 68.95 ms [68.48 ms, 69.33 ms] | 68.95 ms [68.48 ms, 69.33 ms] | 4187.6 [4159.4, 4211.1] | 1.000 [0.991, 1.002] | same |
| static_linear_eigen | corrector | base | 9 | 1 | 0.8 µs [0.6 µs, 1.0 µs] | 0.8 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | corrector | head | 9 | 1 | 0.7 µs [0.6 µs, 1.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | 0.867 [0.709, 1.299] | same |
| static_linear_eigen | corrector | vB | 9 | 1 | 0.7 µs [0.6 µs, 0.9 µs] | 0.7 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 0.974 [0.730, 1.569] | same |
| static_linear_eigen | corrector | vC | 9 | 1 | 0.7 µs [0.6 µs, 1.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | 0.974 [0.734, 1.188] | same |
| static_linear_eigen | step_hooks | base | 9 | 2 | 4.3 µs [3.7 µs, 8.1 µs] | 2.2 µs [1.8 µs, 4.1 µs] | 0.1 [0.1, 0.2] | – | same |
| static_linear_eigen | step_hooks | head | 9 | 2 | 4.4 µs [4.0 µs, 4.6 µs] | 2.2 µs [2.0 µs, 2.3 µs] | 0.1 [0.1, 0.1] | 1.005 [0.547, 1.236] | same |
| static_linear_eigen | step_hooks | vB | 9 | 2 | 4.6 µs [4.5 µs, 5.1 µs] | 2.3 µs [2.2 µs, 2.6 µs] | 0.1 [0.1, 0.2] | 1.062 [0.603, 1.228] | same |
| static_linear_eigen | step_hooks | vC | 9 | 2 | 4.9 µs [3.9 µs, 5.1 µs] | 2.4 µs [2.0 µs, 2.5 µs] | 0.1 [0.1, 0.2] | 1.075 [0.562, 1.375] | same |
| static_linear_eigen | step_rest | base | 9 | 1 | 646.11 ms [639.92 ms, 653.31 ms] | 646.11 ms [639.92 ms, 653.31 ms] | 39243.7 [38867.7, 39681.1] | – | same |
| static_linear_eigen | step_rest | head | 9 | 1 | 656.48 ms [649.59 ms, 663.17 ms] | 656.48 ms [649.59 ms, 663.17 ms] | 39873.8 [39455.3, 40280.2] | 1.017 [1.005, 1.029] | same |
| static_linear_eigen | step_rest | vB | 9 | 1 | 660.15 ms [651.43 ms, 666.59 ms] | 660.15 ms [651.43 ms, 666.59 ms] | 40096.8 [39567.1, 40487.6] | 1.022 [1.010, 1.033] | same |
| static_linear_eigen | step_rest | vC | 9 | 1 | 648.58 ms [645.72 ms, 654.35 ms] | 648.58 ms [645.72 ms, 654.35 ms] | 39393.7 [39220.1, 39744.1] | 1.005 [0.998, 1.018] | same |
| static_linear_eigen | stiffness_assembly | base | 9 | 3 | 39.73 ms [39.41 ms, 40.52 ms] | 13.24 ms [13.14 ms, 13.51 ms] | 804.4 [797.9, 820.3] | – | same |
| static_linear_eigen | stiffness_assembly | head | 9 | 3 | 39.57 ms [39.41 ms, 40.68 ms] | 13.19 ms [13.14 ms, 13.56 ms] | 801.2 [797.9, 823.6] | 0.999 [0.979, 1.027] | same |
| static_linear_eigen | stiffness_assembly | vB | 9 | 3 | 39.83 ms [39.59 ms, 40.85 ms] | 13.28 ms [13.20 ms, 13.62 ms] | 806.5 [801.6, 827.0] | 1.004 [0.983, 1.033] | same |
| static_linear_eigen | stiffness_assembly | vC | 9 | 3 | 39.60 ms [39.43 ms, 40.72 ms] | 13.20 ms [13.14 ms, 13.57 ms] | 801.7 [798.4, 824.4] | 0.997 [0.984, 1.011] | same |
| static_linear_eigen | analysis | base | 9 | 1 | 48.19 ms [47.88 ms, 48.72 ms] | 48.19 ms [47.88 ms, 48.72 ms] | 2927.2 [2908.0, 2958.9] | – | same |
| static_linear_eigen | analysis | head | 9 | 1 | 48.56 ms [48.32 ms, 48.97 ms] | 48.56 ms [48.32 ms, 48.97 ms] | 2949.6 [2934.8, 2974.5] | 1.005 [1.002, 1.023] | same |
| static_linear_eigen | analysis | vB | 9 | 1 | 48.26 ms [48.05 ms, 48.65 ms] | 48.26 ms [48.05 ms, 48.65 ms] | 2931.2 [2918.3, 2954.9] | 1.005 [0.988, 1.016] | same |
| static_linear_eigen | analysis | vC | 9 | 1 | 48.36 ms [48.00 ms, 48.96 ms] | 48.36 ms [48.00 ms, 48.96 ms] | 2937.5 [2915.7, 2973.8] | 1.002 [0.991, 1.016] | same |
| static_linear_eigen | factorize | base | 9 | 2 | 1.031 s [1.013 s, 1.041 s] | 515.34 ms [506.53 ms, 520.56 ms] | 31301.2 [30765.9, 31618.2] | – | same |
| static_linear_eigen | factorize | head | 9 | 2 | 1.050 s [1.036 s, 1.064 s] | 524.91 ms [518.05 ms, 531.86 ms] | 31882.3 [31465.5, 32304.4] | 1.015 [1.004, 1.044] | same |
| static_linear_eigen | factorize | vB | 9 | 2 | 1.055 s [1.039 s, 1.064 s] | 527.56 ms [519.48 ms, 532.14 ms] | 32043.4 [31552.7, 32321.2] | 1.026 [1.014, 1.030] | same |
| static_linear_eigen | factorize | vC | 9 | 2 | 1.035 s [1.026 s, 1.050 s] | 517.58 ms [512.81 ms, 524.97 ms] | 31436.9 [31147.3, 31885.9] | 1.005 [0.992, 1.022] | same |
| static_linear_eigen | back_substitution | base | 9 | 20 | 130.60 ms [129.86 ms, 134.96 ms] | 6.53 ms [6.49 ms, 6.75 ms] | 396.6 [394.4, 409.9] | – | same |
| static_linear_eigen | back_substitution | head | 9 | 20 | 130.95 ms [129.30 ms, 135.08 ms] | 6.55 ms [6.46 ms, 6.75 ms] | 397.7 [392.7, 410.2] | 1.002 [0.991, 1.010] | same |
| static_linear_eigen | back_substitution | vB | 9 | 20 | 130.40 ms [129.85 ms, 132.24 ms] | 6.52 ms [6.49 ms, 6.61 ms] | 396.0 [394.3, 401.6] | 0.999 [0.977, 1.008] | same |
| static_linear_eigen | back_substitution | vC | 9 | 20 | 131.25 ms [130.19 ms, 133.24 ms] | 6.56 ms [6.51 ms, 6.66 ms] | 398.6 [395.4, 404.6] | 1.004 [0.976, 1.016] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 87dd2d994, 92439d3a2, d404f4c70 | d5869f57a777d9a5 |
