Suite `solid`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | mesh | base | 7 | 1 | 791.8 µs [788.3 µs, 812.1 µs] | 791.8 µs [788.3 µs, 812.1 µs] | 48.1 [47.9, 49.3] | – | same |
| static_linear_eigen | mesh | head | 7 | 1 | 793.8 µs [778.0 µs, 829.2 µs] | 793.8 µs [778.0 µs, 829.2 µs] | 48.2 [47.3, 50.4] | 0.996 [0.983, 1.051] | same |
| static_linear_eigen | init | base | 7 | 1 | 9.16 ms [9.12 ms, 9.59 ms] | 9.16 ms [9.12 ms, 9.59 ms] | 556.6 [553.7, 582.3] | – | same |
| static_linear_eigen | init | head | 7 | 1 | 9.19 ms [9.11 ms, 9.31 ms] | 9.19 ms [9.11 ms, 9.31 ms] | 558.3 [553.3, 565.2] | 1.000 [0.959, 1.014] | same |
| static_linear_eigen | solve_step | base | 7 | 1 | 713.03 ms [709.17 ms, 744.03 ms] | 713.03 ms [709.17 ms, 744.03 ms] | 43308.1 [43074.2, 45191.6] | – | same |
| static_linear_eigen | solve_step | head | 7 | 1 | 702.84 ms [699.31 ms, 719.47 ms] | 702.84 ms [699.31 ms, 719.47 ms] | 42689.2 [42475.4, 43699.4] | 0.987 [0.944, 1.011] | same |
| static_linear_eigen | predictor | base | 7 | 1 | 0.1 µs [0.1 µs, 0.3 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head | 7 | 1 | 0.1 µs [0.1 µs, 0.3 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.115 [0.900, 1.615] | same |
| static_linear_eigen | matrix | base | 7 | 1 | 68.88 ms [68.15 ms, 71.09 ms] | 68.88 ms [68.15 ms, 71.09 ms] | 4183.6 [4139.6, 4317.6] | – | same |
| static_linear_eigen | matrix | head | 7 | 1 | 68.75 ms [68.20 ms, 69.30 ms] | 68.75 ms [68.20 ms, 69.30 ms] | 4175.8 [4142.3, 4209.2] | 0.995 [0.971, 1.017] | same |
| static_linear_eigen | corrector | base | 7 | 1 | 0.7 µs [0.6 µs, 0.9 µs] | 0.7 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | corrector | head | 7 | 1 | 0.7 µs [0.5 µs, 0.7 µs] | 0.7 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.959 [0.700, 1.109] | same |
| static_linear_eigen | step_hooks | base | 7 | 2 | 4.3 µs [3.8 µs, 4.6 µs] | 2.2 µs [1.9 µs, 2.3 µs] | 0.1 [0.1, 0.1] | – | same |
| static_linear_eigen | step_hooks | head | 7 | 2 | 4.5 µs [3.9 µs, 5.0 µs] | 2.2 µs [2.0 µs, 2.5 µs] | 0.1 [0.1, 0.2] | 0.998 [0.904, 1.305] | same |
| static_linear_eigen | step_rest | base | 7 | 1 | 644.43 ms [639.57 ms, 672.94 ms] | 644.43 ms [639.57 ms, 672.94 ms] | 39142.0 [38846.7, 40873.7] | – | same |
| static_linear_eigen | step_rest | head | 7 | 1 | 634.63 ms [630.01 ms, 650.39 ms] | 634.63 ms [630.01 ms, 650.39 ms] | 38546.6 [38265.9, 39503.5] | 0.987 [0.941, 1.011] | same |
| static_linear_eigen | stiffness_assembly | base | 7 | 3 | 39.83 ms [39.69 ms, 41.05 ms] | 13.28 ms [13.23 ms, 13.68 ms] | 806.4 [803.6, 831.0] | – | same |
| static_linear_eigen | stiffness_assembly | head | 7 | 3 | 39.83 ms [39.63 ms, 39.88 ms] | 13.28 ms [13.21 ms, 13.29 ms] | 806.4 [802.3, 807.4] | 0.999 [0.969, 1.002] | same |
| static_linear_eigen | analysis | base | 7 | 1 | 48.29 ms [47.92 ms, 48.70 ms] | 48.29 ms [47.92 ms, 48.70 ms] | 2933.0 [2910.8, 2958.2] | – | same |
| static_linear_eigen | analysis | head | 7 | 1 | 48.23 ms [48.08 ms, 48.56 ms] | 48.23 ms [48.08 ms, 48.56 ms] | 2929.4 [2920.0, 2949.2] | 1.000 [0.990, 1.006] | same |
| static_linear_eigen | factorize | base | 7 | 2 | 1.027 s [1.019 s, 1.047 s] | 513.32 ms [509.51 ms, 523.49 ms] | 31178.1 [30947.0, 31796.0] | – | same |
| static_linear_eigen | factorize | head | 7 | 2 | 1.016 s [1.005 s, 1.024 s] | 508.07 ms [502.70 ms, 512.24 ms] | 30859.6 [30533.6, 31112.5] | 0.989 [0.966, 1.001] | same |
| static_linear_eigen | back_substitution | base | 7 | 20 | 128.27 ms [127.40 ms, 129.38 ms] | 6.41 ms [6.37 ms, 6.47 ms] | 389.5 [386.9, 392.9] | – | same |
| static_linear_eigen | back_substitution | head | 7 | 20 | 127.75 ms [127.18 ms, 129.12 ms] | 6.39 ms [6.36 ms, 6.46 ms] | 388.0 [386.2, 392.1] | 0.997 [0.986, 1.004] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 7a6ad8f42, 988a8c227 | d5869f57a777d9a5 |
