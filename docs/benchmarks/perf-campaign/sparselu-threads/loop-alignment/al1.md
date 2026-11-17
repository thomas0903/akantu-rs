Suite `solid`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | mesh | base | 7 | 1 | 801.9 µs [792.0 µs, 813.5 µs] | 801.9 µs [792.0 µs, 813.5 µs] | 48.7 [48.1, 49.4] | – | same |
| static_linear_eigen | mesh | head | 7 | 1 | 804.1 µs [787.3 µs, 815.7 µs] | 804.1 µs [787.3 µs, 815.7 µs] | 48.8 [47.8, 49.5] | 1.003 [0.985, 1.027] | same |
| static_linear_eigen | init | base | 7 | 1 | 9.26 ms [9.23 ms, 9.46 ms] | 9.26 ms [9.23 ms, 9.46 ms] | 562.5 [560.7, 574.5] | – | same |
| static_linear_eigen | init | head | 7 | 1 | 9.26 ms [9.24 ms, 9.32 ms] | 9.26 ms [9.24 ms, 9.32 ms] | 562.5 [561.5, 566.2] | 1.000 [0.980, 1.007] | same |
| static_linear_eigen | solve_step | base | 7 | 1 | 709.63 ms [707.10 ms, 723.63 ms] | 709.63 ms [707.10 ms, 723.63 ms] | 43102.1 [42948.3, 43952.4] | – | same |
| static_linear_eigen | solve_step | head | 7 | 1 | 716.81 ms [711.88 ms, 724.94 ms] | 716.81 ms [711.88 ms, 724.94 ms] | 43538.1 [43238.7, 44032.0] | 1.012 [0.991, 1.021] | same |
| static_linear_eigen | predictor | base | 7 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head | 7 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.889 [0.368, 1.000] | same |
| static_linear_eigen | matrix | base | 7 | 1 | 68.76 ms [68.53 ms, 69.78 ms] | 68.76 ms [68.53 ms, 69.78 ms] | 4176.4 [4162.2, 4238.2] | – | same |
| static_linear_eigen | matrix | head | 7 | 1 | 69.06 ms [68.12 ms, 69.65 ms] | 69.06 ms [68.12 ms, 69.65 ms] | 4194.7 [4137.8, 4230.2] | 1.006 [0.984, 1.010] | same |
| static_linear_eigen | corrector | base | 7 | 1 | 0.8 µs [0.7 µs, 0.9 µs] | 0.8 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | corrector | head | 7 | 1 | 0.7 µs [0.5 µs, 0.8 µs] | 0.7 µs [0.5 µs, 0.8 µs] | 0.0 [0.0, 0.1] | 0.855 [0.695, 1.092] | same |
| static_linear_eigen | step_hooks | base | 7 | 2 | 4.8 µs [4.0 µs, 5.2 µs] | 2.4 µs [2.0 µs, 2.6 µs] | 0.1 [0.1, 0.2] | – | same |
| static_linear_eigen | step_hooks | head | 7 | 2 | 4.5 µs [4.1 µs, 4.6 µs] | 2.2 µs [2.0 µs, 2.3 µs] | 0.1 [0.1, 0.1] | 0.898 [0.832, 1.109] | same |
| static_linear_eigen | step_rest | base | 7 | 1 | 640.58 ms [638.46 ms, 653.93 ms] | 640.58 ms [638.46 ms, 653.93 ms] | 38907.7 [38779.2, 39718.7] | – | same |
| static_linear_eigen | step_rest | head | 7 | 1 | 648.48 ms [643.75 ms, 655.29 ms] | 648.48 ms [643.75 ms, 655.29 ms] | 39388.0 [39100.6, 39801.5] | 1.012 [0.991, 1.023] | same |
| static_linear_eigen | stiffness_assembly | base | 7 | 3 | 39.93 ms [39.87 ms, 40.33 ms] | 13.31 ms [13.29 ms, 13.44 ms] | 808.5 [807.3, 816.5] | – | same |
| static_linear_eigen | stiffness_assembly | head | 7 | 3 | 39.92 ms [39.76 ms, 40.17 ms] | 13.31 ms [13.25 ms, 13.39 ms] | 808.3 [805.0, 813.3] | 0.997 [0.993, 1.007] | same |
| static_linear_eigen | analysis | base | 7 | 1 | 48.31 ms [47.96 ms, 49.04 ms] | 48.31 ms [47.96 ms, 49.04 ms] | 2934.2 [2912.8, 2978.6] | – | same |
| static_linear_eigen | analysis | head | 7 | 1 | 48.33 ms [48.30 ms, 48.72 ms] | 48.33 ms [48.30 ms, 48.72 ms] | 2935.5 [2934.0, 2959.4] | 1.001 [0.985, 1.011] | same |
| static_linear_eigen | factorize | base | 7 | 2 | 1.031 s [1.020 s, 1.061 s] | 515.55 ms [510.19 ms, 530.72 ms] | 31313.8 [30988.5, 32234.9] | – | same |
| static_linear_eigen | factorize | head | 7 | 2 | 1.036 s [1.033 s, 1.053 s] | 518.18 ms [516.32 ms, 526.44 ms] | 31473.6 [31360.7, 31975.0] | 1.006 [0.975, 1.020] | same |
| static_linear_eigen | back_substitution | base | 7 | 20 | 131.75 ms [131.59 ms, 132.67 ms] | 6.59 ms [6.58 ms, 6.63 ms] | 400.1 [399.6, 402.9] | – | same |
| static_linear_eigen | back_substitution | head | 7 | 20 | 131.41 ms [130.77 ms, 133.66 ms] | 6.57 ms [6.54 ms, 6.68 ms] | 399.1 [397.1, 405.9] | 0.996 [0.986, 1.014] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 4df3bb92e, 7a6ad8f42 | d5869f57a777d9a5 |
