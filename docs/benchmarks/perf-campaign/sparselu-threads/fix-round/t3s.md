Suite `solid`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | mesh | base | 7 | 1 | 793.9 µs [787.3 µs, 840.5 µs] | 793.9 µs [787.3 µs, 840.5 µs] | 48.2 [47.8, 51.0] | – | same |
| static_linear_eigen | mesh | head | 7 | 1 | 807.7 µs [779.8 µs, 826.7 µs] | 807.7 µs [779.8 µs, 826.7 µs] | 49.1 [47.4, 50.2] | 1.010 [0.951, 1.044] | same |
| static_linear_eigen | init | base | 7 | 1 | 9.19 ms [9.08 ms, 9.33 ms] | 9.19 ms [9.08 ms, 9.33 ms] | 558.4 [551.6, 566.7] | – | same |
| static_linear_eigen | init | head | 7 | 1 | 9.19 ms [9.12 ms, 9.40 ms] | 9.19 ms [9.12 ms, 9.40 ms] | 558.0 [553.9, 570.8] | 0.999 [0.983, 1.023] | same |
| static_linear_eigen | solve_step | base | 7 | 1 | 717.23 ms [706.59 ms, 731.09 ms] | 717.23 ms [706.59 ms, 731.09 ms] | 43563.6 [42917.2, 44405.5] | – | same |
| static_linear_eigen | solve_step | head | 7 | 1 | 707.51 ms [705.48 ms, 715.92 ms] | 707.51 ms [705.48 ms, 715.92 ms] | 42973.4 [42850.0, 43483.9] | 0.992 [0.968, 1.000] | same |
| static_linear_eigen | predictor | base | 7 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head | 7 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.286 [0.900, 2.500] | same |
| static_linear_eigen | matrix | base | 7 | 1 | 69.00 ms [68.60 ms, 71.33 ms] | 69.00 ms [68.60 ms, 71.33 ms] | 4191.1 [4166.7, 4332.3] | – | same |
| static_linear_eigen | matrix | head | 7 | 1 | 68.55 ms [68.24 ms, 69.96 ms] | 68.55 ms [68.24 ms, 69.96 ms] | 4163.5 [4144.9, 4249.4] | 0.989 [0.964, 1.018] | same |
| static_linear_eigen | corrector | base | 7 | 1 | 0.7 µs [0.5 µs, 0.8 µs] | 0.7 µs [0.5 µs, 0.8 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | corrector | head | 7 | 1 | 0.7 µs [0.6 µs, 0.9 µs] | 0.7 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.056 [0.851, 1.564] | same |
| static_linear_eigen | step_hooks | base | 7 | 2 | 4.4 µs [4.1 µs, 4.8 µs] | 2.2 µs [2.0 µs, 2.4 µs] | 0.1 [0.1, 0.1] | – | same |
| static_linear_eigen | step_hooks | head | 7 | 2 | 4.3 µs [3.9 µs, 4.5 µs] | 2.2 µs [2.0 µs, 2.3 µs] | 0.1 [0.1, 0.1] | 0.937 [0.818, 1.106] | same |
| static_linear_eigen | step_rest | base | 7 | 1 | 648.63 ms [637.88 ms, 659.76 ms] | 648.63 ms [637.88 ms, 659.76 ms] | 39396.6 [38743.7, 40072.9] | – | same |
| static_linear_eigen | step_rest | head | 7 | 1 | 639.16 ms [635.75 ms, 647.67 ms] | 639.16 ms [635.75 ms, 647.67 ms] | 38821.7 [38614.5, 39338.7] | 0.992 [0.968, 1.002] | same |
| static_linear_eigen | stiffness_assembly | base | 7 | 3 | 39.92 ms [39.69 ms, 40.23 ms] | 13.31 ms [13.23 ms, 13.41 ms] | 808.2 [803.6, 814.5] | – | same |
| static_linear_eigen | stiffness_assembly | head | 7 | 3 | 39.79 ms [39.71 ms, 39.98 ms] | 13.26 ms [13.24 ms, 13.33 ms] | 805.6 [804.0, 809.4] | 0.998 [0.991, 1.003] | same |
| static_linear_eigen | analysis | base | 7 | 1 | 48.34 ms [47.91 ms, 49.21 ms] | 48.34 ms [47.91 ms, 49.21 ms] | 2936.0 [2910.1, 2988.9] | – | same |
| static_linear_eigen | analysis | head | 7 | 1 | 48.36 ms [47.95 ms, 48.51 ms] | 48.36 ms [47.95 ms, 48.51 ms] | 2937.2 [2912.3, 2946.1] | 0.995 [0.983, 1.011] | same |
| static_linear_eigen | factorize | base | 7 | 2 | 1.033 s [1.021 s, 1.052 s] | 516.69 ms [510.44 ms, 525.76 ms] | 31383.3 [31003.7, 31934.0] | – | same |
| static_linear_eigen | factorize | head | 7 | 2 | 1.021 s [1.013 s, 1.054 s] | 510.61 ms [506.34 ms, 526.88 ms] | 31013.8 [30754.6, 32002.2] | 0.987 [0.975, 1.032] | same |
| static_linear_eigen | back_substitution | base | 7 | 20 | 128.53 ms [128.05 ms, 129.53 ms] | 6.43 ms [6.40 ms, 6.48 ms] | 390.3 [388.9, 393.4] | – | same |
| static_linear_eigen | back_substitution | head | 7 | 20 | 128.83 ms [128.09 ms, 128.96 ms] | 6.44 ms [6.40 ms, 6.45 ms] | 391.3 [389.0, 391.6] | 0.999 [0.995, 1.006] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 7a6ad8f42, 8c714206b | d5869f57a777d9a5 |
