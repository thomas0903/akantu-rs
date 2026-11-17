Suite `solid`, size `m`; variants: `park262144`, `park4096`, `spin262144`, `spin4096`; reference `park262144`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to park262144 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_tet4_ss | mesh | park262144 | 5 | 1 | 3.13 ms [3.05 ms, 3.23 ms] | 3.13 ms [3.05 ms, 3.23 ms] | 19.3 [18.8, 19.9] | – | same |
| explicit_tet4_ss | mesh | park4096 | 5 | 1 | 3.20 ms [3.09 ms, 3.22 ms] | 3.20 ms [3.09 ms, 3.22 ms] | 19.7 [19.1, 19.9] | 1.020 [0.956, 1.057] | same |
| explicit_tet4_ss | mesh | spin262144 | 5 | 1 | 3.18 ms [3.09 ms, 3.24 ms] | 3.18 ms [3.09 ms, 3.24 ms] | 19.7 [19.1, 20.0] | 1.014 [0.986, 1.034] | same |
| explicit_tet4_ss | mesh | spin4096 | 5 | 1 | 3.23 ms [3.13 ms, 3.23 ms] | 3.23 ms [3.13 ms, 3.23 ms] | 19.9 [19.4, 20.0] | 1.006 [1.000, 1.061] | same |
| explicit_tet4_ss | init | park262144 | 5 | 1 | 47.72 ms [47.01 ms, 48.35 ms] | 47.72 ms [47.01 ms, 48.35 ms] | 294.6 [290.2, 298.5] | – | same |
| explicit_tet4_ss | init | park4096 | 5 | 1 | 47.88 ms [46.56 ms, 48.22 ms] | 47.88 ms [46.56 ms, 48.22 ms] | 295.6 [287.4, 297.7] | 1.005 [0.963, 1.019] | same |
| explicit_tet4_ss | init | spin262144 | 5 | 1 | 47.16 ms [46.79 ms, 48.21 ms] | 47.16 ms [46.79 ms, 48.21 ms] | 291.1 [288.8, 297.6] | 0.988 [0.980, 1.026] | same |
| explicit_tet4_ss | init | spin4096 | 5 | 1 | 47.55 ms [47.43 ms, 48.41 ms] | 47.55 ms [47.43 ms, 48.41 ms] | 293.5 [292.8, 298.8] | 1.009 [0.983, 1.020] | same |
| explicit_tet4_ss | stable_time_step | park262144 | 5 | 5 | 32.76 ms [32.58 ms, 32.80 ms] | 6.55 ms [6.52 ms, 6.56 ms] | 40.4 [40.2, 40.5] | – | same |
| explicit_tet4_ss | stable_time_step | park4096 | 5 | 5 | 32.95 ms [32.75 ms, 33.69 ms] | 6.59 ms [6.55 ms, 6.74 ms] | 40.7 [40.4, 41.6] | 1.007 [0.999, 1.032] | same |
| explicit_tet4_ss | stable_time_step | spin262144 | 5 | 5 | 32.76 ms [32.66 ms, 33.04 ms] | 6.55 ms [6.53 ms, 6.61 ms] | 40.4 [40.3, 40.8] | 1.005 [0.996, 1.009] | same |
| explicit_tet4_ss | stable_time_step | spin4096 | 5 | 5 | 32.78 ms [32.58 ms, 32.86 ms] | 6.56 ms [6.52 ms, 6.57 ms] | 40.5 [40.2, 40.6] | 1.001 [0.993, 1.006] | same |
| explicit_tet4_ss | solve_step | park262144 | 5 | 200 | 481.79 ms [479.62 ms, 483.56 ms] | 2.41 ms [2.40 ms, 2.42 ms] | 14.9 [14.8, 14.9] | – | same |
| explicit_tet4_ss | solve_step | park4096 | 5 | 200 | 423.70 ms [422.25 ms, 424.38 ms] | 2.12 ms [2.11 ms, 2.12 ms] | 13.1 [13.0, 13.1] | 0.880 [0.876, 0.883] | same |
| explicit_tet4_ss | solve_step | spin262144 | 5 | 200 | 479.05 ms [477.49 ms, 479.64 ms] | 2.40 ms [2.39 ms, 2.40 ms] | 14.8 [14.7, 14.8] | 0.994 [0.988, 1.000] | same |
| explicit_tet4_ss | solve_step | spin4096 | 5 | 200 | 414.96 ms [414.02 ms, 417.64 ms] | 2.07 ms [2.07 ms, 2.09 ms] | 12.8 [12.8, 12.9] | 0.864 [0.857, 0.867] | same |
| explicit_tet4_ss | predictor | park262144 | 5 | 200 | 15.7 µs [11.5 µs, 29.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | park4096 | 5 | 200 | 22.6 µs [19.2 µs, 40.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.671 [0.659, 3.559] | same |
| explicit_tet4_ss | predictor | spin262144 | 5 | 200 | 18.9 µs [11.2 µs, 35.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.862 [0.585, 2.825] | same |
| explicit_tet4_ss | predictor | spin4096 | 5 | 200 | 21.2 µs [17.3 µs, 32.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.347 [0.830, 2.801] | same |
| explicit_tet4_ss | residual | park262144 | 5 | 200 | 389.61 ms [388.19 ms, 390.91 ms] | 1.95 ms [1.94 ms, 1.95 ms] | 12.0 [12.0, 12.1] | – | same |
| explicit_tet4_ss | residual | park4096 | 5 | 200 | 367.71 ms [366.78 ms, 368.18 ms] | 1.84 ms [1.83 ms, 1.84 ms] | 11.3 [11.3, 11.4] | 0.944 [0.941, 0.947] | same |
| explicit_tet4_ss | residual | spin262144 | 5 | 200 | 385.15 ms [383.54 ms, 386.00 ms] | 1.93 ms [1.92 ms, 1.93 ms] | 11.9 [11.8, 11.9] | 0.987 [0.984, 0.994] | same |
| explicit_tet4_ss | residual | spin4096 | 5 | 200 | 361.90 ms [360.19 ms, 362.86 ms] | 1.81 ms [1.80 ms, 1.81 ms] | 11.2 [11.1, 11.2] | 0.930 [0.921, 0.932] | same |
| explicit_tet4_ss | lumped_matrix | park262144 | 5 | 200 | 30.7 µs [24.8 µs, 34.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | park4096 | 5 | 200 | 28.8 µs [27.9 µs, 34.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.066 [0.800, 1.253] | same |
| explicit_tet4_ss | lumped_matrix | spin262144 | 5 | 200 | 35.7 µs [32.6 µs, 40.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.244 [1.057, 1.437] | same |
| explicit_tet4_ss | lumped_matrix | spin4096 | 5 | 200 | 37.6 µs [28.9 µs, 48.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.287 [1.128, 1.394] | same |
| explicit_tet4_ss | corrector | park262144 | 5 | 200 | 20.5 µs [9.5 µs, 24.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | park4096 | 5 | 200 | 13.5 µs [10.4 µs, 26.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.656 [0.444, 2.799] | same |
| explicit_tet4_ss | corrector | spin262144 | 5 | 200 | 28.0 µs [12.7 µs, 29.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.394 [0.530, 2.963] | same |
| explicit_tet4_ss | corrector | spin4096 | 5 | 200 | 20.0 µs [17.8 µs, 42.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.219 [0.796, 2.115] | same |
| explicit_tet4_ss | step_hooks | park262144 | 5 | 400 | 497.8 µs [466.9 µs, 514.3 µs] | 1.2 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | park4096 | 5 | 400 | 487.4 µs [474.5 µs, 524.3 µs] | 1.2 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.993 [0.935, 1.053] | same |
| explicit_tet4_ss | step_hooks | spin262144 | 5 | 400 | 498.0 µs [447.1 µs, 550.7 µs] | 1.2 µs [1.1 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.045 [0.898, 1.071] | same |
| explicit_tet4_ss | step_hooks | spin4096 | 5 | 400 | 534.7 µs [525.7 µs, 561.3 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.074 [1.053, 1.164] | same |
| explicit_tet4_ss | step_rest | park262144 | 5 | 200 | 91.59 ms [90.86 ms, 92.85 ms] | 457.9 µs [454.3 µs, 464.2 µs] | 2.8 [2.8, 2.9] | – | same |
| explicit_tet4_ss | step_rest | park4096 | 5 | 200 | 55.37 ms [54.91 ms, 55.93 ms] | 276.8 µs [274.5 µs, 279.6 µs] | 1.7 [1.7, 1.7] | 0.604 [0.602, 0.607] | same |
| explicit_tet4_ss | step_rest | spin262144 | 5 | 200 | 92.82 ms [91.93 ms, 95.50 ms] | 464.1 µs [459.6 µs, 477.5 µs] | 2.9 [2.8, 2.9] | 1.017 [0.990, 1.050] | same |
| explicit_tet4_ss | step_rest | spin4096 | 5 | 200 | 53.32 ms [52.10 ms, 54.12 ms] | 266.6 µs [260.5 µs, 270.6 µs] | 1.6 [1.6, 1.7] | 0.582 [0.573, 0.587] | same |
| explicit_tet4_ss | internal_forces | park262144 | 5 | 40 | 62.73 ms [62.22 ms, 62.83 ms] | 1.57 ms [1.56 ms, 1.57 ms] | 9.7 [9.6, 9.7] | – | same |
| explicit_tet4_ss | internal_forces | park4096 | 5 | 40 | 62.26 ms [62.13 ms, 62.45 ms] | 1.56 ms [1.55 ms, 1.56 ms] | 9.6 [9.6, 9.6] | 0.994 [0.991, 1.004] | same |
| explicit_tet4_ss | internal_forces | spin262144 | 5 | 40 | 61.32 ms [61.09 ms, 61.41 ms] | 1.53 ms [1.53 ms, 1.54 ms] | 9.5 [9.4, 9.5] | 0.979 [0.972, 0.983] | same |
| explicit_tet4_ss | internal_forces | spin4096 | 5 | 40 | 61.25 ms [61.08 ms, 61.57 ms] | 1.53 ms [1.53 ms, 1.54 ms] | 9.5 [9.4, 9.5] | 0.980 [0.976, 0.984] | same |
| explicit_tet4_ss | stresses | park262144 | 5 | 40 | 39.48 ms [39.13 ms, 39.56 ms] | 987.0 µs [978.3 µs, 989.0 µs] | 6.1 [6.0, 6.1] | – | same |
| explicit_tet4_ss | stresses | park4096 | 5 | 40 | 39.28 ms [39.00 ms, 39.40 ms] | 982.0 µs [975.0 µs, 985.1 µs] | 6.1 [6.0, 6.1] | 0.996 [0.990, 1.004] | same |
| explicit_tet4_ss | stresses | spin262144 | 5 | 40 | 38.73 ms [38.52 ms, 39.09 ms] | 968.4 µs [962.9 µs, 977.3 µs] | 6.0 [5.9, 6.0] | 0.985 [0.977, 0.989] | same |
| explicit_tet4_ss | stresses | spin4096 | 5 | 40 | 39.28 ms [39.08 ms, 39.64 ms] | 982.0 µs [977.0 µs, 990.9 µs] | 6.1 [6.0, 6.1] | 0.995 [0.990, 1.013] | same |
| explicit_tet4_ss | energies | park262144 | 5 | 40 | 81.96 ms [81.77 ms, 82.38 ms] | 2.05 ms [2.04 ms, 2.06 ms] | 12.6 [12.6, 12.7] | – | same |
| explicit_tet4_ss | energies | park4096 | 5 | 40 | 82.02 ms [81.67 ms, 82.13 ms] | 2.05 ms [2.04 ms, 2.05 ms] | 12.7 [12.6, 12.7] | 0.998 [0.996, 1.004] | same |
| explicit_tet4_ss | energies | spin262144 | 5 | 40 | 81.95 ms [81.66 ms, 82.43 ms] | 2.05 ms [2.04 ms, 2.06 ms] | 12.6 [12.6, 12.7] | 1.001 [0.991, 1.006] | same |
| explicit_tet4_ss | energies | spin4096 | 5 | 40 | 81.96 ms [81.86 ms, 82.21 ms] | 2.05 ms [2.05 ms, 2.06 ms] | 12.6 [12.6, 12.7] | 1.000 [0.995, 1.004] | same |
| explicit_tri3_ss | mesh | park262144 | 5 | 1 | 4.09 ms [4.02 ms, 4.13 ms] | 4.09 ms [4.02 ms, 4.13 ms] | 22.7 [22.3, 23.0] | – | same |
| explicit_tri3_ss | mesh | park4096 | 5 | 1 | 4.07 ms [4.05 ms, 4.09 ms] | 4.07 ms [4.05 ms, 4.09 ms] | 22.6 [22.5, 22.7] | 0.995 [0.985, 1.010] | same |
| explicit_tri3_ss | mesh | spin262144 | 5 | 1 | 4.03 ms [3.96 ms, 4.10 ms] | 4.03 ms [3.96 ms, 4.10 ms] | 22.4 [22.0, 22.8] | 0.989 [0.961, 1.020] | same |
| explicit_tri3_ss | mesh | spin4096 | 5 | 1 | 4.04 ms [4.01 ms, 4.11 ms] | 4.04 ms [4.01 ms, 4.11 ms] | 22.5 [22.3, 22.8] | 0.997 [0.976, 1.009] | same |
| explicit_tri3_ss | init | park262144 | 5 | 1 | 38.49 ms [37.94 ms, 39.45 ms] | 38.49 ms [37.94 ms, 39.45 ms] | 213.8 [210.8, 219.2] | – | same |
| explicit_tri3_ss | init | park4096 | 5 | 1 | 37.99 ms [37.83 ms, 38.97 ms] | 37.99 ms [37.83 ms, 38.97 ms] | 211.0 [210.2, 216.5] | 0.993 [0.959, 1.027] | same |
| explicit_tri3_ss | init | spin262144 | 5 | 1 | 37.95 ms [37.69 ms, 38.25 ms] | 37.95 ms [37.69 ms, 38.25 ms] | 210.8 [209.4, 212.5] | 0.987 [0.969, 0.993] | same |
| explicit_tri3_ss | init | spin4096 | 5 | 1 | 38.53 ms [37.61 ms, 38.69 ms] | 38.53 ms [37.61 ms, 38.69 ms] | 214.1 [209.0, 215.0] | 0.995 [0.977, 1.005] | same |
| explicit_tri3_ss | stable_time_step | park262144 | 5 | 5 | 23.15 ms [22.59 ms, 23.46 ms] | 4.63 ms [4.52 ms, 4.69 ms] | 25.7 [25.1, 26.1] | – | same |
| explicit_tri3_ss | stable_time_step | park4096 | 5 | 5 | 23.16 ms [22.59 ms, 24.07 ms] | 4.63 ms [4.52 ms, 4.81 ms] | 25.7 [25.1, 26.7] | 1.000 [0.971, 1.066] | same |
| explicit_tri3_ss | stable_time_step | spin262144 | 5 | 5 | 22.64 ms [22.56 ms, 23.17 ms] | 4.53 ms [4.51 ms, 4.63 ms] | 25.2 [25.1, 25.7] | 0.995 [0.962, 1.026] | same |
| explicit_tri3_ss | stable_time_step | spin4096 | 5 | 5 | 23.06 ms [22.58 ms, 23.15 ms] | 4.61 ms [4.52 ms, 4.63 ms] | 25.6 [25.1, 25.7] | 0.995 [0.976, 1.021] | same |
| explicit_tri3_ss | solve_step | park262144 | 5 | 300 | 743.40 ms [741.42 ms, 744.57 ms] | 2.48 ms [2.47 ms, 2.48 ms] | 13.8 [13.7, 13.8] | – | same |
| explicit_tri3_ss | solve_step | park4096 | 5 | 300 | 534.66 ms [532.93 ms, 537.30 ms] | 1.78 ms [1.78 ms, 1.79 ms] | 9.9 [9.9, 10.0] | 0.719 [0.718, 0.722] | same |
| explicit_tri3_ss | solve_step | spin262144 | 5 | 300 | 743.45 ms [739.97 ms, 764.92 ms] | 2.48 ms [2.47 ms, 2.55 ms] | 13.8 [13.7, 14.2] | 0.999 [0.997, 1.028] | same |
| explicit_tri3_ss | solve_step | spin4096 | 5 | 300 | 521.02 ms [519.78 ms, 528.13 ms] | 1.74 ms [1.73 ms, 1.76 ms] | 9.6 [9.6, 9.8] | 0.701 [0.699, 0.709] | same |
| explicit_tri3_ss | predictor | park262144 | 5 | 300 | 43.3 µs [28.2 µs, 54.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | park4096 | 5 | 300 | 34.3 µs [23.0 µs, 38.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.814 [0.547, 1.244] | same |
| explicit_tri3_ss | predictor | spin262144 | 5 | 300 | 37.4 µs [17.3 µs, 63.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.865 [0.612, 1.473] | same |
| explicit_tri3_ss | predictor | spin4096 | 5 | 300 | 35.8 µs [27.0 µs, 48.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.081 [0.561, 1.619] | same |
| explicit_tri3_ss | residual | park262144 | 5 | 300 | 472.68 ms [471.09 ms, 472.89 ms] | 1.58 ms [1.57 ms, 1.58 ms] | 8.8 [8.7, 8.8] | – | same |
| explicit_tri3_ss | residual | park4096 | 5 | 300 | 392.76 ms [392.32 ms, 394.76 ms] | 1.31 ms [1.31 ms, 1.32 ms] | 7.3 [7.3, 7.3] | 0.833 [0.830, 0.836] | same |
| explicit_tri3_ss | residual | spin262144 | 5 | 300 | 470.27 ms [469.87 ms, 493.21 ms] | 1.57 ms [1.57 ms, 1.64 ms] | 8.7 [8.7, 9.1] | 0.998 [0.994, 1.045] | same |
| explicit_tri3_ss | residual | spin4096 | 5 | 300 | 383.70 ms [383.22 ms, 392.11 ms] | 1.28 ms [1.28 ms, 1.31 ms] | 7.1 [7.1, 7.3] | 0.813 [0.811, 0.829] | same |
| explicit_tri3_ss | lumped_matrix | park262144 | 5 | 300 | 66.1 µs [62.4 µs, 74.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | park4096 | 5 | 300 | 74.3 µs [72.1 µs, 78.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.145 [0.984, 1.171] | same |
| explicit_tri3_ss | lumped_matrix | spin262144 | 5 | 300 | 72.7 µs [61.1 µs, 76.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.152 [0.893, 1.190] | same |
| explicit_tri3_ss | lumped_matrix | spin4096 | 5 | 300 | 72.5 µs [57.6 µs, 74.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.051 [0.924, 1.150] | same |
| explicit_tri3_ss | corrector | park262144 | 5 | 300 | 57.5 µs [39.4 µs, 73.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | park4096 | 5 | 300 | 25.7 µs [20.0 µs, 38.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.507 [0.345, 0.543] | same |
| explicit_tri3_ss | corrector | spin262144 | 5 | 300 | 41.4 µs [16.1 µs, 54.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.720 [0.409, 1.006] | same |
| explicit_tri3_ss | corrector | spin4096 | 5 | 300 | 35.7 µs [18.7 µs, 40.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.524 [0.360, 0.906] | same |
| explicit_tri3_ss | step_hooks | park262144 | 5 | 600 | 838.0 µs [718.4 µs, 871.0 µs] | 1.4 µs [1.2 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | park4096 | 5 | 600 | 800.4 µs [774.4 µs, 865.0 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.961 [0.918, 1.150] | same |
| explicit_tri3_ss | step_hooks | spin262144 | 5 | 600 | 809.9 µs [771.6 µs, 890.0 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 0.959 [0.915, 1.183] | same |
| explicit_tri3_ss | step_hooks | spin4096 | 5 | 600 | 795.5 µs [780.7 µs, 889.9 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 0.984 [0.896, 1.239] | same |
| explicit_tri3_ss | step_rest | park262144 | 5 | 300 | 269.51 ms [268.40 ms, 270.77 ms] | 898.4 µs [894.7 µs, 902.6 µs] | 5.0 [5.0, 5.0] | – | same |
| explicit_tri3_ss | step_rest | park4096 | 5 | 300 | 140.66 ms [139.45 ms, 141.55 ms] | 468.9 µs [464.8 µs, 471.8 µs] | 2.6 [2.6, 2.6] | 0.522 [0.519, 0.525] | same |
| explicit_tri3_ss | step_rest | spin262144 | 5 | 300 | 270.27 ms [268.67 ms, 272.64 ms] | 900.9 µs [895.6 µs, 908.8 µs] | 5.0 [5.0, 5.0] | 1.000 [0.998, 1.012] | same |
| explicit_tri3_ss | step_rest | spin4096 | 5 | 300 | 135.61 ms [134.96 ms, 137.05 ms] | 452.0 µs [449.9 µs, 456.8 µs] | 2.5 [2.5, 2.5] | 0.505 [0.498, 0.506] | same |
| explicit_tri3_ss | internal_forces | park262144 | 5 | 60 | 48.52 ms [48.37 ms, 49.47 ms] | 808.6 µs [806.2 µs, 824.4 µs] | 4.5 [4.5, 4.6] | – | same |
| explicit_tri3_ss | internal_forces | park4096 | 5 | 60 | 48.54 ms [48.27 ms, 48.58 ms] | 809.1 µs [804.5 µs, 809.7 µs] | 4.5 [4.5, 4.5] | 1.001 [0.982, 1.001] | same |
| explicit_tri3_ss | internal_forces | spin262144 | 5 | 60 | 48.35 ms [48.02 ms, 48.81 ms] | 805.8 µs [800.3 µs, 813.6 µs] | 4.5 [4.4, 4.5] | 0.996 [0.977, 0.998] | same |
| explicit_tri3_ss | internal_forces | spin4096 | 5 | 60 | 48.27 ms [47.88 ms, 48.33 ms] | 804.5 µs [798.0 µs, 805.5 µs] | 4.5 [4.4, 4.5] | 0.988 [0.976, 0.996] | same |
| explicit_tri3_ss | stresses | park262144 | 5 | 60 | 33.16 ms [32.90 ms, 33.64 ms] | 552.6 µs [548.4 µs, 560.6 µs] | 3.1 [3.0, 3.1] | – | same |
| explicit_tri3_ss | stresses | park4096 | 5 | 60 | 32.87 ms [32.83 ms, 33.39 ms] | 547.8 µs [547.1 µs, 556.5 µs] | 3.0 [3.0, 3.1] | 0.998 [0.976, 1.005] | same |
| explicit_tri3_ss | stresses | spin262144 | 5 | 60 | 32.91 ms [32.19 ms, 35.99 ms] | 548.4 µs [536.5 µs, 599.8 µs] | 3.0 [3.0, 3.3] | 0.983 [0.975, 1.085] | same |
| explicit_tri3_ss | stresses | spin4096 | 5 | 60 | 32.52 ms [32.32 ms, 32.94 ms] | 542.0 µs [538.7 µs, 549.0 µs] | 3.0 [3.0, 3.0] | 0.979 [0.961, 1.001] | same |
| explicit_tri3_ss | energies | park262144 | 5 | 60 | 136.24 ms [135.88 ms, 137.50 ms] | 2.27 ms [2.26 ms, 2.29 ms] | 12.6 [12.6, 12.7] | – | same |
| explicit_tri3_ss | energies | park4096 | 5 | 60 | 136.89 ms [136.11 ms, 137.18 ms] | 2.28 ms [2.27 ms, 2.29 ms] | 12.7 [12.6, 12.7] | 1.002 [0.997, 1.007] | same |
| explicit_tri3_ss | energies | spin262144 | 5 | 60 | 136.60 ms [136.23 ms, 137.44 ms] | 2.28 ms [2.27 ms, 2.29 ms] | 12.6 [12.6, 12.7] | 1.003 [0.995, 1.007] | same |
| explicit_tri3_ss | energies | spin4096 | 5 | 60 | 136.48 ms [136.09 ms, 137.22 ms] | 2.27 ms [2.27 ms, 2.29 ms] | 12.6 [12.6, 12.7] | 1.002 [0.998, 1.004] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42 | fecb3fb4b46c8b8c |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7a6ad8f42 | b6b902f5ca71049e |
