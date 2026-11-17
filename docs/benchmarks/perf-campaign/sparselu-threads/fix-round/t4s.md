Suite `solid`, size `m`; variants: `al`, `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | mesh | al | 7 | 1 | 800.4 µs [779.0 µs, 808.0 µs] | 800.4 µs [779.0 µs, 808.0 µs] | 48.6 [47.3, 49.1] | 1.002 [0.977, 1.024] | same |
| static_linear_eigen | mesh | base | 7 | 1 | 797.3 µs [786.0 µs, 806.8 µs] | 797.3 µs [786.0 µs, 806.8 µs] | 48.4 [47.7, 49.0] | – | same |
| static_linear_eigen | mesh | head | 7 | 1 | 803.8 µs [796.0 µs, 838.7 µs] | 803.8 µs [796.0 µs, 838.7 µs] | 48.8 [48.3, 50.9] | 1.006 [0.997, 1.067] | same |
| static_linear_eigen | init | al | 7 | 1 | 9.20 ms [9.13 ms, 9.38 ms] | 9.20 ms [9.13 ms, 9.38 ms] | 558.9 [554.8, 569.4] | 0.999 [0.996, 1.023] | same |
| static_linear_eigen | init | base | 7 | 1 | 9.18 ms [9.15 ms, 9.22 ms] | 9.18 ms [9.15 ms, 9.22 ms] | 557.7 [555.7, 559.8] | – | same |
| static_linear_eigen | init | head | 7 | 1 | 9.20 ms [9.17 ms, 9.37 ms] | 9.20 ms [9.17 ms, 9.37 ms] | 558.8 [556.8, 569.1] | 1.002 [0.996, 1.020] | same |
| static_linear_eigen | solve_step | al | 7 | 1 | 705.52 ms [698.96 ms, 710.32 ms] | 705.52 ms [698.96 ms, 710.32 ms] | 42852.2 [42454.0, 43143.7] | 0.995 [0.986, 1.001] | same |
| static_linear_eigen | solve_step | base | 7 | 1 | 709.51 ms [705.18 ms, 715.45 ms] | 709.51 ms [705.18 ms, 715.45 ms] | 43094.4 [42831.6, 43455.1] | – | same |
| static_linear_eigen | solve_step | head | 7 | 1 | 709.53 ms [704.69 ms, 711.67 ms] | 709.53 ms [704.69 ms, 711.67 ms] | 43095.9 [42801.6, 43226.0] | 1.000 [0.993, 1.003] | same |
| static_linear_eigen | predictor | al | 7 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.900 [0.643, 1.889] | same |
| static_linear_eigen | predictor | base | 7 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head | 7 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.286 [0.818, 2.222] | same |
| static_linear_eigen | matrix | al | 7 | 1 | 68.49 ms [68.20 ms, 68.96 ms] | 68.49 ms [68.20 ms, 68.96 ms] | 4159.8 [4142.2, 4188.8] | 0.998 [0.985, 1.009] | same |
| static_linear_eigen | matrix | base | 7 | 1 | 68.56 ms [68.32 ms, 69.28 ms] | 68.56 ms [68.32 ms, 69.28 ms] | 4164.5 [4149.9, 4208.1] | – | same |
| static_linear_eigen | matrix | head | 7 | 1 | 68.54 ms [68.15 ms, 68.75 ms] | 68.54 ms [68.15 ms, 68.75 ms] | 4162.8 [4139.6, 4175.8] | 0.997 [0.988, 1.004] | same |
| static_linear_eigen | corrector | al | 7 | 1 | 0.8 µs [0.7 µs, 0.9 µs] | 0.8 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.103 [0.937, 1.294] | same |
| static_linear_eigen | corrector | base | 7 | 1 | 0.7 µs [0.7 µs, 0.8 µs] | 0.7 µs [0.7 µs, 0.8 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | corrector | head | 7 | 1 | 0.7 µs [0.6 µs, 1.0 µs] | 0.7 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | 1.029 [0.811, 1.456] | same |
| static_linear_eigen | step_hooks | al | 7 | 2 | 4.4 µs [4.0 µs, 5.0 µs] | 2.2 µs [2.0 µs, 2.5 µs] | 0.1 [0.1, 0.2] | 1.000 [0.938, 1.198] | same |
| static_linear_eigen | step_hooks | base | 7 | 2 | 4.3 µs [3.9 µs, 4.7 µs] | 2.1 µs [1.9 µs, 2.3 µs] | 0.1 [0.1, 0.1] | – | same |
| static_linear_eigen | step_hooks | head | 7 | 2 | 4.4 µs [4.1 µs, 5.0 µs] | 2.2 µs [2.1 µs, 2.5 µs] | 0.1 [0.1, 0.2] | 1.068 [1.005, 1.116] | same |
| static_linear_eigen | step_rest | al | 7 | 1 | 636.93 ms [630.44 ms, 641.86 ms] | 636.93 ms [630.44 ms, 641.86 ms] | 38686.0 [38291.9, 38985.6] | 0.994 [0.984, 1.001] | same |
| static_linear_eigen | step_rest | base | 7 | 1 | 641.11 ms [636.85 ms, 646.97 ms] | 641.11 ms [636.85 ms, 646.97 ms] | 38940.3 [38681.4, 39295.8] | – | same |
| static_linear_eigen | step_rest | head | 7 | 1 | 641.35 ms [636.53 ms, 643.10 ms] | 641.35 ms [636.53 ms, 643.10 ms] | 38954.6 [38661.7, 39060.7] | 1.000 [0.993, 1.003] | same |
| static_linear_eigen | stiffness_assembly | al | 7 | 3 | 39.74 ms [39.56 ms, 39.93 ms] | 13.25 ms [13.19 ms, 13.31 ms] | 804.5 [800.9, 808.4] | 0.996 [0.990, 1.004] | same |
| static_linear_eigen | stiffness_assembly | base | 7 | 3 | 39.88 ms [39.58 ms, 40.15 ms] | 13.29 ms [13.19 ms, 13.38 ms] | 807.5 [801.4, 812.9] | – | same |
| static_linear_eigen | stiffness_assembly | head | 7 | 3 | 39.78 ms [39.59 ms, 40.00 ms] | 13.26 ms [13.20 ms, 13.33 ms] | 805.5 [801.5, 809.9] | 0.998 [0.991, 1.005] | same |
| static_linear_eigen | analysis | al | 7 | 1 | 48.37 ms [47.88 ms, 48.56 ms] | 48.37 ms [47.88 ms, 48.56 ms] | 2937.9 [2908.4, 2949.7] | 1.009 [0.989, 1.014] | same |
| static_linear_eigen | analysis | base | 7 | 1 | 47.95 ms [47.84 ms, 48.83 ms] | 47.95 ms [47.84 ms, 48.83 ms] | 2912.4 [2905.9, 2966.1] | – | same |
| static_linear_eigen | analysis | head | 7 | 1 | 48.13 ms [47.82 ms, 48.80 ms] | 48.13 ms [47.82 ms, 48.80 ms] | 2923.3 [2904.7, 2963.8] | 1.004 [0.979, 1.020] | same |
| static_linear_eigen | factorize | al | 7 | 2 | 1.015 s [1.012 s, 1.020 s] | 507.36 ms [505.79 ms, 510.11 ms] | 30816.1 [30721.1, 30983.2] | 0.990 [0.979, 1.002] | same |
| static_linear_eigen | factorize | base | 7 | 2 | 1.025 s [1.015 s, 1.038 s] | 512.49 ms [507.42 ms, 518.90 ms] | 31127.8 [30819.7, 31517.3] | – | same |
| static_linear_eigen | factorize | head | 7 | 2 | 1.013 s [1.003 s, 1.022 s] | 506.72 ms [501.45 ms, 511.10 ms] | 30777.2 [30457.2, 31043.6] | 0.989 [0.968, 0.997] | same |
| static_linear_eigen | back_substitution | al | 7 | 20 | 128.78 ms [127.93 ms, 130.80 ms] | 6.44 ms [6.40 ms, 6.54 ms] | 391.1 [388.5, 397.2] | 1.005 [1.002, 1.014] | same |
| static_linear_eigen | back_substitution | base | 7 | 20 | 128.20 ms [127.63 ms, 129.58 ms] | 6.41 ms [6.38 ms, 6.48 ms] | 389.3 [387.6, 393.5] | – | same |
| static_linear_eigen | back_substitution | head | 7 | 20 | 128.57 ms [127.06 ms, 129.75 ms] | 6.43 ms [6.35 ms, 6.49 ms] | 390.5 [385.9, 394.1] | 1.002 [0.992, 1.009] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 35250f369, 7a6ad8f42, 8c714206b | d5869f57a777d9a5 |
