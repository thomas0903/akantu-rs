Suite `solid`, size `m`; variants: `t2eq16384`, `t2eq262144`, `t2eq4096`, `t4eq16384`, `t4eq262144`, `t4eq4096`; reference `t2eq16384`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to t2eq16384 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_plastic | mesh | t2eq16384 | 5 | 1 | 2.02 ms [1.98 ms, 2.03 ms] | 2.02 ms [1.98 ms, 2.03 ms] | 19.2 [18.8, 19.2] | – | same |
| explicit_plastic | mesh | t2eq262144 | 5 | 1 | 2.13 ms [2.10 ms, 2.15 ms] | 2.13 ms [2.10 ms, 2.15 ms] | 20.2 [19.9, 20.3] | 1.057 [1.037, 1.075] | same |
| explicit_plastic | mesh | t2eq4096 | 5 | 1 | 2.04 ms [2.01 ms, 2.09 ms] | 2.04 ms [2.01 ms, 2.09 ms] | 19.3 [19.1, 19.8] | 1.007 [0.996, 1.032] | same |
| explicit_plastic | mesh | t4eq16384 | 5 | 1 | 2.05 ms [2.02 ms, 2.14 ms] | 2.05 ms [2.02 ms, 2.14 ms] | 19.5 [19.1, 20.3] | 1.016 [0.998, 1.060] | same |
| explicit_plastic | mesh | t4eq262144 | 5 | 1 | 2.09 ms [2.02 ms, 2.14 ms] | 2.09 ms [2.02 ms, 2.14 ms] | 19.8 [19.2, 20.3] | 1.033 [1.004, 1.060] | same |
| explicit_plastic | mesh | t4eq4096 | 5 | 1 | 2.13 ms [2.07 ms, 2.13 ms] | 2.13 ms [2.07 ms, 2.13 ms] | 20.2 [19.7, 20.2] | 1.051 [1.029, 1.073] | same |
| explicit_plastic | init | t2eq16384 | 5 | 1 | 34.15 ms [33.82 ms, 34.92 ms] | 34.15 ms [33.82 ms, 34.92 ms] | 323.9 [320.7, 331.1] | – | same |
| explicit_plastic | init | t2eq262144 | 5 | 1 | 34.47 ms [33.32 ms, 34.91 ms] | 34.47 ms [33.32 ms, 34.91 ms] | 326.8 [316.0, 331.1] | 1.009 [0.954, 1.032] | same |
| explicit_plastic | init | t2eq4096 | 5 | 1 | 34.21 ms [33.71 ms, 36.17 ms] | 34.21 ms [33.71 ms, 36.17 ms] | 324.4 [319.7, 343.0] | 1.000 [0.965, 1.060] | same |
| explicit_plastic | init | t4eq16384 | 5 | 1 | 34.16 ms [33.53 ms, 35.80 ms] | 34.16 ms [33.53 ms, 35.80 ms] | 324.0 [317.9, 339.5] | 0.991 [0.982, 1.049] | same |
| explicit_plastic | init | t4eq262144 | 5 | 1 | 34.51 ms [34.35 ms, 35.18 ms] | 34.51 ms [34.35 ms, 35.18 ms] | 327.2 [325.7, 333.6] | 1.006 [0.990, 1.040] | same |
| explicit_plastic | init | t4eq4096 | 5 | 1 | 34.71 ms [33.97 ms, 35.30 ms] | 34.71 ms [33.97 ms, 35.30 ms] | 329.2 [322.2, 334.7] | 1.024 [0.973, 1.034] | same |
| explicit_plastic | stable_time_step | t2eq16384 | 5 | 5 | 21.35 ms [21.28 ms, 21.46 ms] | 4.27 ms [4.26 ms, 4.29 ms] | 40.5 [40.4, 40.7] | – | same |
| explicit_plastic | stable_time_step | t2eq262144 | 5 | 5 | 21.32 ms [21.27 ms, 21.36 ms] | 4.26 ms [4.25 ms, 4.27 ms] | 40.4 [40.3, 40.5] | 0.999 [0.993, 1.002] | same |
| explicit_plastic | stable_time_step | t2eq4096 | 5 | 5 | 21.33 ms [21.28 ms, 21.76 ms] | 4.27 ms [4.26 ms, 4.35 ms] | 40.5 [40.4, 41.3] | 1.001 [0.994, 1.019] | same |
| explicit_plastic | stable_time_step | t4eq16384 | 5 | 5 | 21.52 ms [21.28 ms, 21.61 ms] | 4.30 ms [4.26 ms, 4.32 ms] | 40.8 [40.4, 41.0] | 1.008 [0.992, 1.016] | same |
| explicit_plastic | stable_time_step | t4eq262144 | 5 | 5 | 21.33 ms [21.27 ms, 21.76 ms] | 4.27 ms [4.25 ms, 4.35 ms] | 40.4 [40.3, 41.3] | 0.998 [0.994, 1.023] | same |
| explicit_plastic | stable_time_step | t4eq4096 | 5 | 5 | 21.30 ms [21.29 ms, 21.76 ms] | 4.26 ms [4.26 ms, 4.35 ms] | 40.4 [40.4, 41.3] | 1.001 [0.992, 1.019] | same |
| explicit_plastic | solve_step | t2eq16384 | 5 | 120 | 605.00 ms [603.41 ms, 619.31 ms] | 5.04 ms [5.03 ms, 5.16 ms] | 47.8 [47.7, 48.9] | – | same |
| explicit_plastic | solve_step | t2eq262144 | 5 | 120 | 615.64 ms [614.54 ms, 618.18 ms] | 5.13 ms [5.12 ms, 5.15 ms] | 48.6 [48.6, 48.8] | 1.019 [0.992, 1.024] | same |
| explicit_plastic | solve_step | t2eq4096 | 5 | 120 | 601.18 ms [598.92 ms, 602.94 ms] | 5.01 ms [4.99 ms, 5.02 ms] | 47.5 [47.3, 47.6] | 0.993 [0.970, 0.999] | same |
| explicit_plastic | solve_step | t4eq16384 | 5 | 120 | 426.62 ms [424.09 ms, 430.55 ms] | 3.56 ms [3.53 ms, 3.59 ms] | 33.7 [33.5, 34.0] | 0.704 [0.685, 0.713] | same |
| explicit_plastic | solve_step | t4eq262144 | 5 | 120 | 446.72 ms [445.20 ms, 449.19 ms] | 3.72 ms [3.71 ms, 3.74 ms] | 35.3 [35.2, 35.5] | 0.738 [0.725, 0.742] | same |
| explicit_plastic | solve_step | t4eq4096 | 5 | 120 | 422.89 ms [421.41 ms, 425.21 ms] | 3.52 ms [3.51 ms, 3.54 ms] | 33.4 [33.3, 33.6] | 0.699 [0.680, 0.703] | same |
| explicit_plastic | predictor | t2eq16384 | 5 | 120 | 23.2 µs [22.2 µs, 26.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | predictor | t2eq262144 | 5 | 120 | 21.6 µs [20.9 µs, 36.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.932 [0.778, 1.625] | same |
| explicit_plastic | predictor | t2eq4096 | 5 | 120 | 23.3 µs [21.6 µs, 25.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.997 [0.848, 1.139] | same |
| explicit_plastic | predictor | t4eq16384 | 5 | 120 | 22.8 µs [18.5 µs, 25.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.964 [0.796, 1.085] | same |
| explicit_plastic | predictor | t4eq262144 | 5 | 120 | 21.9 µs [19.1 µs, 25.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.948 [0.744, 1.100] | same |
| explicit_plastic | predictor | t4eq4096 | 5 | 120 | 23.1 µs [19.1 µs, 26.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.872 [0.846, 1.128] | same |
| explicit_plastic | residual | t2eq16384 | 5 | 120 | 489.14 ms [487.55 ms, 504.78 ms] | 4.08 ms [4.06 ms, 4.21 ms] | 38.7 [38.5, 39.9] | – | same |
| explicit_plastic | residual | t2eq262144 | 5 | 120 | 492.07 ms [491.11 ms, 492.93 ms] | 4.10 ms [4.09 ms, 4.11 ms] | 38.9 [38.8, 39.0] | 1.006 [0.973, 1.011] | same |
| explicit_plastic | residual | t2eq4096 | 5 | 120 | 488.39 ms [486.59 ms, 490.18 ms] | 4.07 ms [4.05 ms, 4.08 ms] | 38.6 [38.5, 38.7] | 0.996 [0.968, 1.002] | same |
| explicit_plastic | residual | t4eq16384 | 5 | 120 | 331.11 ms [329.68 ms, 334.13 ms] | 2.76 ms [2.75 ms, 2.78 ms] | 26.2 [26.1, 26.4] | 0.677 [0.654, 0.683] | same |
| explicit_plastic | residual | t4eq262144 | 5 | 120 | 338.02 ms [335.01 ms, 338.77 ms] | 2.82 ms [2.79 ms, 2.82 ms] | 26.7 [26.5, 26.8] | 0.691 [0.671, 0.693] | same |
| explicit_plastic | residual | t4eq4096 | 5 | 120 | 328.63 ms [327.52 ms, 329.97 ms] | 2.74 ms [2.73 ms, 2.75 ms] | 26.0 [25.9, 26.1] | 0.673 [0.649, 0.674] | same |
| explicit_plastic | lumped_matrix | t2eq16384 | 5 | 120 | 30.6 µs [28.9 µs, 32.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | lumped_matrix | t2eq262144 | 5 | 120 | 31.3 µs [30.1 µs, 39.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.050 [0.960, 1.279] | same |
| explicit_plastic | lumped_matrix | t2eq4096 | 5 | 120 | 30.4 µs [29.7 µs, 33.4 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.981 [0.934, 1.107] | same |
| explicit_plastic | lumped_matrix | t4eq16384 | 5 | 120 | 28.9 µs [26.7 µs, 31.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.986 [0.820, 1.021] | same |
| explicit_plastic | lumped_matrix | t4eq262144 | 5 | 120 | 30.6 µs [29.0 µs, 42.5 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.981 [0.892, 1.391] | same |
| explicit_plastic | lumped_matrix | t4eq4096 | 5 | 120 | 31.5 µs [27.4 µs, 33.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.028 [0.880, 1.098] | same |
| explicit_plastic | corrector | t2eq16384 | 5 | 120 | 20.3 µs [19.6 µs, 26.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | corrector | t2eq262144 | 5 | 120 | 20.0 µs [18.1 µs, 27.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.983 [0.689, 1.392] | same |
| explicit_plastic | corrector | t2eq4096 | 5 | 120 | 20.7 µs [19.5 µs, 27.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.004 [0.804, 1.394] | same |
| explicit_plastic | corrector | t4eq16384 | 5 | 120 | 22.0 µs [18.8 µs, 26.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.016 [0.839, 1.316] | same |
| explicit_plastic | corrector | t4eq262144 | 5 | 120 | 20.9 µs [18.4 µs, 25.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.989 [0.901, 1.237] | same |
| explicit_plastic | corrector | t4eq4096 | 5 | 120 | 25.1 µs [21.3 µs, 31.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.233 [0.875, 1.567] | same |
| explicit_plastic | step_hooks | t2eq16384 | 5 | 240 | 79.11 ms [78.60 ms, 79.45 ms] | 329.6 µs [327.5 µs, 331.1 µs] | 3.1 [3.1, 3.1] | – | same |
| explicit_plastic | step_hooks | t2eq262144 | 5 | 240 | 79.18 ms [79.15 ms, 79.83 ms] | 329.9 µs [329.8 µs, 332.6 µs] | 3.1 [3.1, 3.2] | 1.007 [0.996, 1.011] | same |
| explicit_plastic | step_hooks | t2eq4096 | 5 | 240 | 78.97 ms [78.25 ms, 79.10 ms] | 329.0 µs [326.0 µs, 329.6 µs] | 3.1 [3.1, 3.1] | 0.996 [0.989, 1.006] | same |
| explicit_plastic | step_hooks | t4eq16384 | 5 | 240 | 64.39 ms [63.40 ms, 65.46 ms] | 268.3 µs [264.2 µs, 272.8 µs] | 2.5 [2.5, 2.6] | 0.814 [0.803, 0.833] | same |
| explicit_plastic | step_hooks | t4eq262144 | 5 | 240 | 64.27 ms [63.79 ms, 65.67 ms] | 267.8 µs [265.8 µs, 273.6 µs] | 2.5 [2.5, 2.6] | 0.812 [0.807, 0.835] | same |
| explicit_plastic | step_hooks | t4eq4096 | 5 | 240 | 63.85 ms [63.45 ms, 65.26 ms] | 266.1 µs [264.4 µs, 271.9 µs] | 2.5 [2.5, 2.6] | 0.812 [0.802, 0.821] | same |
| explicit_plastic | step_rest | t2eq16384 | 5 | 120 | 35.62 ms [35.37 ms, 36.24 ms] | 296.9 µs [294.8 µs, 302.0 µs] | 2.8 [2.8, 2.9] | – | same |
| explicit_plastic | step_rest | t2eq262144 | 5 | 120 | 43.34 ms [43.12 ms, 45.54 ms] | 361.1 µs [359.3 µs, 379.5 µs] | 3.4 [3.4, 3.6] | 1.225 [1.196, 1.278] | same |
| explicit_plastic | step_rest | t2eq4096 | 5 | 120 | 33.27 ms [33.13 ms, 33.61 ms] | 277.3 µs [276.1 µs, 280.1 µs] | 2.6 [2.6, 2.7] | 0.934 [0.927, 0.939] | same |
| explicit_plastic | step_rest | t4eq16384 | 5 | 120 | 30.42 ms [30.31 ms, 30.81 ms] | 253.5 µs [252.6 µs, 256.7 µs] | 2.4 [2.4, 2.4] | 0.854 [0.845, 0.861] | same |
| explicit_plastic | step_rest | t4eq262144 | 5 | 120 | 44.34 ms [43.56 ms, 45.59 ms] | 369.5 µs [363.0 µs, 379.9 µs] | 3.5 [3.4, 3.6] | 1.253 [1.213, 1.271] | same |
| explicit_plastic | step_rest | t4eq4096 | 5 | 120 | 29.88 ms [29.66 ms, 29.94 ms] | 249.0 µs [247.2 µs, 249.5 µs] | 2.4 [2.3, 2.4] | 0.839 [0.818, 0.846] | same |
| explicit_plastic | internal_forces | t2eq16384 | 5 | 24 | 92.86 ms [92.61 ms, 94.24 ms] | 3.87 ms [3.86 ms, 3.93 ms] | 36.7 [36.6, 37.2] | – | same |
| explicit_plastic | internal_forces | t2eq262144 | 5 | 24 | 93.14 ms [92.30 ms, 95.88 ms] | 3.88 ms [3.85 ms, 4.00 ms] | 36.8 [36.5, 37.9] | 1.002 [0.979, 1.033] | same |
| explicit_plastic | internal_forces | t2eq4096 | 5 | 24 | 92.96 ms [92.78 ms, 93.58 ms] | 3.87 ms [3.87 ms, 3.90 ms] | 36.7 [36.7, 37.0] | 1.000 [0.986, 1.008] | same |
| explicit_plastic | internal_forces | t4eq16384 | 5 | 24 | 54.91 ms [53.42 ms, 54.98 ms] | 2.29 ms [2.23 ms, 2.29 ms] | 21.7 [21.1, 21.7] | 0.583 [0.577, 0.592] | same |
| explicit_plastic | internal_forces | t4eq262144 | 5 | 24 | 54.25 ms [53.06 ms, 55.79 ms] | 2.26 ms [2.21 ms, 2.32 ms] | 21.4 [21.0, 22.0] | 0.582 [0.572, 0.592] | same |
| explicit_plastic | internal_forces | t4eq4096 | 5 | 24 | 54.81 ms [53.44 ms, 55.72 ms] | 2.28 ms [2.23 ms, 2.32 ms] | 21.7 [21.1, 22.0] | 0.582 [0.571, 0.602] | same |
| explicit_plastic | stresses | t2eq16384 | 5 | 24 | 110.41 ms [110.03 ms, 110.73 ms] | 4.60 ms [4.58 ms, 4.61 ms] | 43.6 [43.5, 43.7] | – | same |
| explicit_plastic | stresses | t2eq262144 | 5 | 24 | 110.73 ms [109.58 ms, 110.91 ms] | 4.61 ms [4.57 ms, 4.62 ms] | 43.8 [43.3, 43.8] | 1.005 [0.991, 1.006] | same |
| explicit_plastic | stresses | t2eq4096 | 5 | 24 | 110.78 ms [110.20 ms, 116.00 ms] | 4.62 ms [4.59 ms, 4.83 ms] | 43.8 [43.5, 45.8] | 1.002 [1.000, 1.051] | same |
| explicit_plastic | stresses | t4eq16384 | 5 | 24 | 105.25 ms [104.77 ms, 105.42 ms] | 4.39 ms [4.37 ms, 4.39 ms] | 41.6 [41.4, 41.7] | 0.953 [0.947, 0.957] | same |
| explicit_plastic | stresses | t4eq262144 | 5 | 24 | 105.39 ms [105.04 ms, 105.95 ms] | 4.39 ms [4.38 ms, 4.41 ms] | 41.6 [41.5, 41.9] | 0.956 [0.950, 0.963] | same |
| explicit_plastic | stresses | t4eq4096 | 5 | 24 | 105.37 ms [105.07 ms, 106.57 ms] | 4.39 ms [4.38 ms, 4.44 ms] | 41.6 [41.5, 42.1] | 0.955 [0.949, 0.964] | same |
| explicit_plastic | energies | t2eq16384 | 5 | 24 | 40.54 ms [40.35 ms, 40.90 ms] | 1.69 ms [1.68 ms, 1.70 ms] | 16.0 [15.9, 16.2] | – | same |
| explicit_plastic | energies | t2eq262144 | 5 | 24 | 40.64 ms [40.53 ms, 41.67 ms] | 1.69 ms [1.69 ms, 1.74 ms] | 16.1 [16.0, 16.5] | 1.002 [0.991, 1.033] | same |
| explicit_plastic | energies | t2eq4096 | 5 | 24 | 40.71 ms [40.39 ms, 40.74 ms] | 1.70 ms [1.68 ms, 1.70 ms] | 16.1 [16.0, 16.1] | 1.005 [0.994, 1.009] | same |
| explicit_plastic | energies | t4eq16384 | 5 | 24 | 40.52 ms [40.51 ms, 40.57 ms] | 1.69 ms [1.69 ms, 1.69 ms] | 16.0 [16.0, 16.0] | 1.000 [0.992, 1.004] | same |
| explicit_plastic | energies | t4eq262144 | 5 | 24 | 40.76 ms [40.48 ms, 41.25 ms] | 1.70 ms [1.69 ms, 1.72 ms] | 16.1 [16.0, 16.3] | 1.004 [0.990, 1.022] | same |
| explicit_plastic | energies | t4eq4096 | 5 | 24 | 40.77 ms [40.47 ms, 41.53 ms] | 1.70 ms [1.69 ms, 1.73 ms] | 16.1 [16.0, 16.4] | 1.006 [0.994, 1.028] | same |
| explicit_tet4_ss | mesh | t2eq16384 | 5 | 1 | 3.06 ms [3.03 ms, 3.13 ms] | 3.06 ms [3.03 ms, 3.13 ms] | 18.9 [18.7, 19.3] | – | same |
| explicit_tet4_ss | mesh | t2eq262144 | 5 | 1 | 3.12 ms [3.02 ms, 3.22 ms] | 3.12 ms [3.02 ms, 3.22 ms] | 19.3 [18.6, 19.9] | 1.004 [0.970, 1.053] | same |
| explicit_tet4_ss | mesh | t2eq4096 | 5 | 1 | 3.06 ms [3.04 ms, 3.09 ms] | 3.06 ms [3.04 ms, 3.09 ms] | 18.9 [18.7, 19.1] | 0.993 [0.985, 1.010] | same |
| explicit_tet4_ss | mesh | t4eq16384 | 5 | 1 | 3.09 ms [3.06 ms, 3.27 ms] | 3.09 ms [3.06 ms, 3.27 ms] | 19.1 [18.9, 20.2] | 1.012 [0.997, 1.044] | same |
| explicit_tet4_ss | mesh | t4eq262144 | 5 | 1 | 3.12 ms [3.06 ms, 3.16 ms] | 3.12 ms [3.06 ms, 3.16 ms] | 19.3 [18.9, 19.5] | 1.013 [1.001, 1.029] | same |
| explicit_tet4_ss | mesh | t4eq4096 | 5 | 1 | 3.07 ms [3.03 ms, 3.15 ms] | 3.07 ms [3.03 ms, 3.15 ms] | 18.9 [18.7, 19.4] | 1.002 [0.981, 1.030] | same |
| explicit_tet4_ss | init | t2eq16384 | 5 | 1 | 47.59 ms [45.79 ms, 48.22 ms] | 47.59 ms [45.79 ms, 48.22 ms] | 293.8 [282.7, 297.6] | – | same |
| explicit_tet4_ss | init | t2eq262144 | 5 | 1 | 46.76 ms [46.46 ms, 47.11 ms] | 46.76 ms [46.46 ms, 47.11 ms] | 288.6 [286.8, 290.8] | 0.987 [0.970, 1.029] | same |
| explicit_tet4_ss | init | t2eq4096 | 5 | 1 | 47.32 ms [46.36 ms, 48.19 ms] | 47.32 ms [46.36 ms, 48.19 ms] | 292.1 [286.2, 297.5] | 1.003 [0.981, 1.031] | same |
| explicit_tet4_ss | init | t4eq16384 | 5 | 1 | 47.36 ms [45.86 ms, 47.85 ms] | 47.36 ms [45.86 ms, 47.85 ms] | 292.3 [283.1, 295.4] | 0.995 [0.987, 1.024] | same |
| explicit_tet4_ss | init | t4eq262144 | 5 | 1 | 47.49 ms [45.85 ms, 47.96 ms] | 47.49 ms [45.85 ms, 47.96 ms] | 293.1 [283.0, 296.0] | 1.001 [0.971, 1.016] | same |
| explicit_tet4_ss | init | t4eq4096 | 5 | 1 | 47.28 ms [46.91 ms, 47.65 ms] | 47.28 ms [46.91 ms, 47.65 ms] | 291.8 [289.6, 294.2] | 1.001 [0.981, 1.025] | same |
| explicit_tet4_ss | stable_time_step | t2eq16384 | 5 | 5 | 32.61 ms [32.54 ms, 32.98 ms] | 6.52 ms [6.51 ms, 6.60 ms] | 40.3 [40.2, 40.7] | – | same |
| explicit_tet4_ss | stable_time_step | t2eq262144 | 5 | 5 | 32.69 ms [32.56 ms, 33.26 ms] | 6.54 ms [6.51 ms, 6.65 ms] | 40.4 [40.2, 41.1] | 0.998 [0.994, 1.022] | same |
| explicit_tet4_ss | stable_time_step | t2eq4096 | 5 | 5 | 32.61 ms [32.49 ms, 32.74 ms] | 6.52 ms [6.50 ms, 6.55 ms] | 40.3 [40.1, 40.4] | 0.998 [0.993, 1.005] | same |
| explicit_tet4_ss | stable_time_step | t4eq16384 | 5 | 5 | 33.25 ms [32.70 ms, 33.41 ms] | 6.65 ms [6.54 ms, 6.68 ms] | 41.1 [40.4, 41.2] | 1.020 [0.992, 1.024] | same |
| explicit_tet4_ss | stable_time_step | t4eq262144 | 5 | 5 | 32.80 ms [32.60 ms, 33.03 ms] | 6.56 ms [6.52 ms, 6.61 ms] | 40.5 [40.2, 40.8] | 1.004 [1.000, 1.008] | same |
| explicit_tet4_ss | stable_time_step | t4eq4096 | 5 | 5 | 32.78 ms [32.63 ms, 32.86 ms] | 6.56 ms [6.53 ms, 6.57 ms] | 40.5 [40.3, 40.6] | 1.001 [0.994, 1.008] | same |
| explicit_tet4_ss | solve_step | t2eq16384 | 5 | 200 | 712.35 ms [709.43 ms, 718.45 ms] | 3.56 ms [3.55 ms, 3.59 ms] | 22.0 [21.9, 22.2] | – | same |
| explicit_tet4_ss | solve_step | t2eq262144 | 5 | 200 | 737.11 ms [734.19 ms, 748.85 ms] | 3.69 ms [3.67 ms, 3.74 ms] | 22.8 [22.7, 23.1] | 1.036 [1.024, 1.051] | same |
| explicit_tet4_ss | solve_step | t2eq4096 | 5 | 200 | 707.55 ms [704.06 ms, 708.80 ms] | 3.54 ms [3.52 ms, 3.54 ms] | 21.8 [21.7, 21.9] | 0.989 [0.983, 0.997] | same |
| explicit_tet4_ss | solve_step | t4eq16384 | 5 | 200 | 484.96 ms [483.74 ms, 486.98 ms] | 2.42 ms [2.42 ms, 2.43 ms] | 15.0 [14.9, 15.0] | 0.679 [0.676, 0.682] | same |
| explicit_tet4_ss | solve_step | t4eq262144 | 5 | 200 | 534.08 ms [533.57 ms, 535.09 ms] | 2.67 ms [2.67 ms, 2.68 ms] | 16.5 [16.5, 16.5] | 0.749 [0.744, 0.754] | same |
| explicit_tet4_ss | solve_step | t4eq4096 | 5 | 200 | 476.43 ms [474.85 ms, 479.53 ms] | 2.38 ms [2.37 ms, 2.40 ms] | 14.7 [14.7, 14.8] | 0.667 [0.663, 0.673] | same |
| explicit_tet4_ss | predictor | t2eq16384 | 5 | 200 | 29.1 µs [25.9 µs, 35.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | t2eq262144 | 5 | 200 | 28.6 µs [26.0 µs, 34.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.964 [0.926, 1.126] | same |
| explicit_tet4_ss | predictor | t2eq4096 | 5 | 200 | 29.3 µs [28.0 µs, 31.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.980 [0.857, 1.126] | same |
| explicit_tet4_ss | predictor | t4eq16384 | 5 | 200 | 25.1 µs [16.9 µs, 28.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.863 [0.546, 1.004] | same |
| explicit_tet4_ss | predictor | t4eq262144 | 5 | 200 | 27.4 µs [25.9 µs, 27.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.942 [0.739, 1.065] | same |
| explicit_tet4_ss | predictor | t4eq4096 | 5 | 200 | 25.4 µs [16.4 µs, 34.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.734 [0.544, 1.210] | same |
| explicit_tet4_ss | residual | t2eq16384 | 5 | 200 | 643.14 ms [640.13 ms, 648.83 ms] | 3.22 ms [3.20 ms, 3.24 ms] | 19.8 [19.8, 20.0] | – | same |
| explicit_tet4_ss | residual | t2eq262144 | 5 | 200 | 646.36 ms [643.53 ms, 656.93 ms] | 3.23 ms [3.22 ms, 3.28 ms] | 19.9 [19.9, 20.3] | 1.005 [0.994, 1.021] | same |
| explicit_tet4_ss | residual | t2eq4096 | 5 | 200 | 642.41 ms [639.37 ms, 643.48 ms] | 3.21 ms [3.20 ms, 3.22 ms] | 19.8 [19.7, 19.9] | 0.994 [0.988, 1.004] | same |
| explicit_tet4_ss | residual | t4eq16384 | 5 | 200 | 426.57 ms [424.83 ms, 427.82 ms] | 2.13 ms [2.12 ms, 2.14 ms] | 13.2 [13.1, 13.2] | 0.661 [0.659, 0.664] | same |
| explicit_tet4_ss | residual | t4eq262144 | 5 | 200 | 443.72 ms [443.44 ms, 444.04 ms] | 2.22 ms [2.22 ms, 2.22 ms] | 13.7 [13.7, 13.7] | 0.689 [0.684, 0.693] | same |
| explicit_tet4_ss | residual | t4eq4096 | 5 | 200 | 423.51 ms [422.01 ms, 426.58 ms] | 2.12 ms [2.11 ms, 2.13 ms] | 13.1 [13.0, 13.2] | 0.656 [0.653, 0.663] | same |
| explicit_tet4_ss | lumped_matrix | t2eq16384 | 5 | 200 | 43.3 µs [34.4 µs, 54.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | t2eq262144 | 5 | 200 | 48.9 µs [43.1 µs, 52.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.129 [0.898, 1.515] | same |
| explicit_tet4_ss | lumped_matrix | t2eq4096 | 5 | 200 | 40.0 µs [36.2 µs, 50.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.051 [0.742, 1.173] | same |
| explicit_tet4_ss | lumped_matrix | t4eq16384 | 5 | 200 | 36.1 µs [34.2 µs, 41.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.834 [0.760, 1.045] | same |
| explicit_tet4_ss | lumped_matrix | t4eq262144 | 5 | 200 | 39.6 µs [37.1 µs, 44.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.857 [0.724, 1.259] | same |
| explicit_tet4_ss | lumped_matrix | t4eq4096 | 5 | 200 | 30.7 µs [25.6 µs, 36.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.744 [0.561, 0.866] | same |
| explicit_tet4_ss | corrector | t2eq16384 | 5 | 200 | 29.1 µs [23.9 µs, 40.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | t2eq262144 | 5 | 200 | 28.4 µs [22.9 µs, 36.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.191 [0.562, 1.259] | same |
| explicit_tet4_ss | corrector | t2eq4096 | 5 | 200 | 24.1 µs [22.6 µs, 36.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.863 [0.649, 1.246] | same |
| explicit_tet4_ss | corrector | t4eq16384 | 5 | 200 | 25.1 µs [16.5 µs, 29.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.925 [0.556, 1.053] | same |
| explicit_tet4_ss | corrector | t4eq262144 | 5 | 200 | 20.4 µs [15.5 µs, 26.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.616 [0.504, 1.090] | same |
| explicit_tet4_ss | corrector | t4eq4096 | 5 | 200 | 18.4 µs [13.8 µs, 42.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.659 [0.339, 1.762] | same |
| explicit_tet4_ss | step_hooks | t2eq16384 | 5 | 400 | 545.2 µs [495.5 µs, 588.5 µs] | 1.4 µs [1.2 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | t2eq262144 | 5 | 400 | 543.0 µs [523.1 µs, 572.8 µs] | 1.4 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.996 [0.889, 1.147] | same |
| explicit_tet4_ss | step_hooks | t2eq4096 | 5 | 400 | 543.9 µs [501.3 µs, 588.8 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.004 [0.930, 1.080] | same |
| explicit_tet4_ss | step_hooks | t4eq16384 | 5 | 400 | 507.5 µs [453.3 µs, 540.1 µs] | 1.3 µs [1.1 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.889 [0.815, 1.090] | same |
| explicit_tet4_ss | step_hooks | t4eq262144 | 5 | 400 | 500.7 µs [467.6 µs, 510.6 µs] | 1.3 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.892 [0.851, 1.022] | same |
| explicit_tet4_ss | step_hooks | t4eq4096 | 5 | 400 | 500.0 µs [454.5 µs, 522.1 µs] | 1.2 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.910 [0.840, 1.009] | same |
| explicit_tet4_ss | step_rest | t2eq16384 | 5 | 200 | 68.58 ms [68.06 ms, 68.97 ms] | 342.9 µs [340.3 µs, 344.8 µs] | 2.1 [2.1, 2.1] | – | same |
| explicit_tet4_ss | step_rest | t2eq262144 | 5 | 200 | 90.16 ms [89.96 ms, 91.28 ms] | 450.8 µs [449.8 µs, 456.4 µs] | 2.8 [2.8, 2.8] | 1.319 [1.308, 1.333] | same |
| explicit_tet4_ss | step_rest | t2eq4096 | 5 | 200 | 64.47 ms [64.06 ms, 64.67 ms] | 322.4 µs [320.3 µs, 323.3 µs] | 2.0 [2.0, 2.0] | 0.940 [0.933, 0.944] | same |
| explicit_tet4_ss | step_rest | t4eq16384 | 5 | 200 | 58.30 ms [57.84 ms, 58.52 ms] | 291.5 µs [289.2 µs, 292.6 µs] | 1.8 [1.8, 1.8] | 0.850 [0.841, 0.859] | same |
| explicit_tet4_ss | step_rest | t4eq262144 | 5 | 200 | 89.52 ms [89.30 ms, 90.77 ms] | 447.6 µs [446.5 µs, 453.9 µs] | 2.8 [2.8, 2.8] | 1.314 [1.298, 1.316] | same |
| explicit_tet4_ss | step_rest | t4eq4096 | 5 | 200 | 52.27 ms [51.72 ms, 52.37 ms] | 261.3 µs [258.6 µs, 261.8 µs] | 1.6 [1.6, 1.6] | 0.759 [0.754, 0.768] | same |
| explicit_tet4_ss | internal_forces | t2eq16384 | 5 | 40 | 117.74 ms [117.24 ms, 118.57 ms] | 2.94 ms [2.93 ms, 2.96 ms] | 18.2 [18.1, 18.3] | – | same |
| explicit_tet4_ss | internal_forces | t2eq262144 | 5 | 40 | 117.67 ms [117.33 ms, 118.16 ms] | 2.94 ms [2.93 ms, 2.95 ms] | 18.2 [18.1, 18.2] | 1.000 [0.993, 1.004] | same |
| explicit_tet4_ss | internal_forces | t2eq4096 | 5 | 40 | 117.85 ms [116.81 ms, 118.82 ms] | 2.95 ms [2.92 ms, 2.97 ms] | 18.2 [18.0, 18.3] | 1.001 [0.992, 1.003] | same |
| explicit_tet4_ss | internal_forces | t4eq16384 | 5 | 40 | 73.75 ms [73.19 ms, 73.82 ms] | 1.84 ms [1.83 ms, 1.85 ms] | 11.4 [11.3, 11.4] | 0.626 [0.617, 0.627] | same |
| explicit_tet4_ss | internal_forces | t4eq262144 | 5 | 40 | 73.61 ms [72.96 ms, 73.78 ms] | 1.84 ms [1.82 ms, 1.84 ms] | 11.4 [11.3, 11.4] | 0.626 [0.615, 0.628] | same |
| explicit_tet4_ss | internal_forces | t4eq4096 | 5 | 40 | 73.59 ms [73.34 ms, 73.91 ms] | 1.84 ms [1.83 ms, 1.85 ms] | 11.4 [11.3, 11.4] | 0.625 [0.621, 0.627] | same |
| explicit_tet4_ss | stresses | t2eq16384 | 5 | 40 | 57.92 ms [57.68 ms, 58.16 ms] | 1.45 ms [1.44 ms, 1.45 ms] | 8.9 [8.9, 9.0] | – | same |
| explicit_tet4_ss | stresses | t2eq262144 | 5 | 40 | 57.68 ms [57.39 ms, 58.21 ms] | 1.44 ms [1.43 ms, 1.46 ms] | 8.9 [8.9, 9.0] | 0.996 [0.991, 1.005] | same |
| explicit_tet4_ss | stresses | t2eq4096 | 5 | 40 | 57.75 ms [57.51 ms, 57.97 ms] | 1.44 ms [1.44 ms, 1.45 ms] | 8.9 [8.9, 8.9] | 0.997 [0.994, 0.999] | same |
| explicit_tet4_ss | stresses | t4eq16384 | 5 | 40 | 44.15 ms [43.82 ms, 44.38 ms] | 1.10 ms [1.10 ms, 1.11 ms] | 6.8 [6.8, 6.8] | 0.762 [0.754, 0.766] | same |
| explicit_tet4_ss | stresses | t4eq262144 | 5 | 40 | 44.20 ms [43.80 ms, 44.29 ms] | 1.11 ms [1.09 ms, 1.11 ms] | 6.8 [6.8, 6.8] | 0.762 [0.756, 0.764] | same |
| explicit_tet4_ss | stresses | t4eq4096 | 5 | 40 | 44.06 ms [43.90 ms, 44.28 ms] | 1.10 ms [1.10 ms, 1.11 ms] | 6.8 [6.8, 6.8] | 0.761 [0.757, 0.765] | same |
| explicit_tet4_ss | energies | t2eq16384 | 5 | 40 | 82.07 ms [81.53 ms, 82.39 ms] | 2.05 ms [2.04 ms, 2.06 ms] | 12.7 [12.6, 12.7] | – | same |
| explicit_tet4_ss | energies | t2eq262144 | 5 | 40 | 81.73 ms [81.54 ms, 82.19 ms] | 2.04 ms [2.04 ms, 2.05 ms] | 12.6 [12.6, 12.7] | 0.999 [0.990, 1.003] | same |
| explicit_tet4_ss | energies | t2eq4096 | 5 | 40 | 81.89 ms [81.82 ms, 82.27 ms] | 2.05 ms [2.05 ms, 2.06 ms] | 12.6 [12.6, 12.7] | 1.001 [0.993, 1.004] | same |
| explicit_tet4_ss | energies | t4eq16384 | 5 | 40 | 81.90 ms [81.60 ms, 82.04 ms] | 2.05 ms [2.04 ms, 2.05 ms] | 12.6 [12.6, 12.7] | 0.998 [0.992, 1.005] | same |
| explicit_tet4_ss | energies | t4eq262144 | 5 | 40 | 81.93 ms [81.91 ms, 82.43 ms] | 2.05 ms [2.05 ms, 2.06 ms] | 12.6 [12.6, 12.7] | 1.003 [0.994, 1.005] | same |
| explicit_tet4_ss | energies | t4eq4096 | 5 | 40 | 82.00 ms [81.90 ms, 82.24 ms] | 2.05 ms [2.05 ms, 2.06 ms] | 12.7 [12.6, 12.7] | 1.001 [0.994, 1.006] | same |
| explicit_tri3_ss | mesh | t2eq16384 | 5 | 1 | 3.93 ms [3.92 ms, 4.08 ms] | 3.93 ms [3.92 ms, 4.08 ms] | 21.9 [21.8, 22.7] | – | same |
| explicit_tri3_ss | mesh | t2eq262144 | 5 | 1 | 4.05 ms [3.93 ms, 4.16 ms] | 4.05 ms [3.93 ms, 4.16 ms] | 22.5 [21.8, 23.1] | 1.025 [0.994, 1.056] | same |
| explicit_tri3_ss | mesh | t2eq4096 | 5 | 1 | 3.91 ms [3.90 ms, 4.06 ms] | 3.91 ms [3.90 ms, 4.06 ms] | 21.7 [21.7, 22.6] | 0.995 [0.955, 1.033] | same |
| explicit_tri3_ss | mesh | t4eq16384 | 5 | 1 | 4.06 ms [4.02 ms, 4.16 ms] | 4.06 ms [4.02 ms, 4.16 ms] | 22.6 [22.3, 23.1] | 1.029 [1.017, 1.035] | same |
| explicit_tri3_ss | mesh | t4eq262144 | 5 | 1 | 4.01 ms [3.96 ms, 4.19 ms] | 4.01 ms [3.96 ms, 4.19 ms] | 22.3 [22.0, 23.3] | 1.019 [0.977, 1.068] | same |
| explicit_tri3_ss | mesh | t4eq4096 | 5 | 1 | 3.96 ms [3.88 ms, 4.10 ms] | 3.96 ms [3.88 ms, 4.10 ms] | 22.0 [21.5, 22.8] | 1.004 [0.986, 1.036] | same |
| explicit_tri3_ss | init | t2eq16384 | 5 | 1 | 38.12 ms [37.85 ms, 38.74 ms] | 38.12 ms [37.85 ms, 38.74 ms] | 211.8 [210.3, 215.2] | – | same |
| explicit_tri3_ss | init | t2eq262144 | 5 | 1 | 38.29 ms [37.11 ms, 38.88 ms] | 38.29 ms [37.11 ms, 38.88 ms] | 212.7 [206.2, 216.0] | 1.004 [0.958, 1.027] | same |
| explicit_tri3_ss | init | t2eq4096 | 5 | 1 | 38.08 ms [36.83 ms, 38.12 ms] | 38.08 ms [36.83 ms, 38.12 ms] | 211.6 [204.6, 211.8] | 0.997 [0.951, 1.006] | same |
| explicit_tri3_ss | init | t4eq16384 | 5 | 1 | 38.22 ms [37.60 ms, 38.47 ms] | 38.22 ms [37.60 ms, 38.47 ms] | 212.3 [208.9, 213.7] | 1.002 [0.970, 1.017] | same |
| explicit_tri3_ss | init | t4eq262144 | 5 | 1 | 37.98 ms [37.45 ms, 38.65 ms] | 37.98 ms [37.45 ms, 38.65 ms] | 211.0 [208.1, 214.7] | 0.999 [0.967, 1.014] | same |
| explicit_tri3_ss | init | t4eq4096 | 5 | 1 | 38.06 ms [37.57 ms, 38.61 ms] | 38.06 ms [37.57 ms, 38.61 ms] | 211.5 [208.7, 214.5] | 0.999 [0.970, 1.012] | same |
| explicit_tri3_ss | stable_time_step | t2eq16384 | 5 | 5 | 23.14 ms [22.54 ms, 23.15 ms] | 4.63 ms [4.51 ms, 4.63 ms] | 25.7 [25.0, 25.7] | – | same |
| explicit_tri3_ss | stable_time_step | t2eq262144 | 5 | 5 | 23.20 ms [22.58 ms, 23.44 ms] | 4.64 ms [4.52 ms, 4.69 ms] | 25.8 [25.1, 26.0] | 1.013 [0.975, 1.030] | same |
| explicit_tri3_ss | stable_time_step | t2eq4096 | 5 | 5 | 23.14 ms [22.59 ms, 24.18 ms] | 4.63 ms [4.52 ms, 4.84 ms] | 25.7 [25.1, 26.9] | 1.001 [0.976, 1.072] | same |
| explicit_tri3_ss | stable_time_step | t4eq16384 | 5 | 5 | 23.42 ms [22.55 ms, 23.83 ms] | 4.68 ms [4.51 ms, 4.77 ms] | 26.0 [25.1, 26.5] | 1.012 [0.997, 1.057] | same |
| explicit_tri3_ss | stable_time_step | t4eq262144 | 5 | 5 | 24.05 ms [23.15 ms, 26.25 ms] | 4.81 ms [4.63 ms, 5.25 ms] | 26.7 [25.7, 29.2] | 1.067 [1.000, 1.160] | same |
| explicit_tri3_ss | stable_time_step | t4eq4096 | 5 | 5 | 23.13 ms [22.61 ms, 24.23 ms] | 4.63 ms [4.52 ms, 4.85 ms] | 25.7 [25.1, 26.9] | 1.023 [0.977, 1.066] | same |
| explicit_tri3_ss | solve_step | t2eq16384 | 5 | 300 | 828.60 ms [826.79 ms, 832.33 ms] | 2.76 ms [2.76 ms, 2.77 ms] | 15.3 [15.3, 15.4] | – | same |
| explicit_tri3_ss | solve_step | t2eq262144 | 5 | 300 | 955.19 ms [952.70 ms, 958.68 ms] | 3.18 ms [3.18 ms, 3.20 ms] | 17.7 [17.6, 17.8] | 1.151 [1.148, 1.160] | same |
| explicit_tri3_ss | solve_step | t2eq4096 | 5 | 300 | 828.11 ms [825.36 ms, 830.22 ms] | 2.76 ms [2.75 ms, 2.77 ms] | 15.3 [15.3, 15.4] | 1.000 [0.992, 1.002] | same |
| explicit_tri3_ss | solve_step | t4eq16384 | 5 | 300 | 596.67 ms [595.56 ms, 607.98 ms] | 1.99 ms [1.99 ms, 2.03 ms] | 11.0 [11.0, 11.3] | 0.720 [0.719, 0.730] | same |
| explicit_tri3_ss | solve_step | t4eq262144 | 5 | 300 | 786.29 ms [784.14 ms, 787.53 ms] | 2.62 ms [2.61 ms, 2.63 ms] | 14.6 [14.5, 14.6] | 0.949 [0.942, 0.953] | same |
| explicit_tri3_ss | solve_step | t4eq4096 | 5 | 300 | 590.20 ms [588.74 ms, 596.49 ms] | 1.97 ms [1.96 ms, 1.99 ms] | 10.9 [10.9, 11.0] | 0.713 [0.711, 0.717] | same |
| explicit_tri3_ss | predictor | t2eq16384 | 5 | 300 | 44.1 µs [41.0 µs, 49.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | t2eq262144 | 5 | 300 | 39.9 µs [34.3 µs, 41.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.904 [0.721, 1.020] | same |
| explicit_tri3_ss | predictor | t2eq4096 | 5 | 300 | 35.8 µs [27.0 µs, 61.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.762 [0.567, 1.385] | same |
| explicit_tri3_ss | predictor | t4eq16384 | 5 | 300 | 29.1 µs [24.8 µs, 38.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.673 [0.561, 0.921] | same |
| explicit_tri3_ss | predictor | t4eq262144 | 5 | 300 | 39.1 µs [37.9 µs, 47.6 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.899 [0.794, 1.080] | same |
| explicit_tri3_ss | predictor | t4eq4096 | 5 | 300 | 29.4 µs [21.2 µs, 46.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.690 [0.490, 0.969] | same |
| explicit_tri3_ss | residual | t2eq16384 | 5 | 300 | 643.71 ms [641.62 ms, 645.60 ms] | 2.15 ms [2.14 ms, 2.15 ms] | 11.9 [11.9, 12.0] | – | same |
| explicit_tri3_ss | residual | t2eq262144 | 5 | 300 | 685.70 ms [684.23 ms, 689.27 ms] | 2.29 ms [2.28 ms, 2.30 ms] | 12.7 [12.7, 12.8] | 1.065 [1.062, 1.074] | same |
| explicit_tri3_ss | residual | t2eq4096 | 5 | 300 | 643.77 ms [639.55 ms, 644.68 ms] | 2.15 ms [2.13 ms, 2.15 ms] | 11.9 [11.8, 11.9] | 1.001 [0.991, 1.002] | same |
| explicit_tri3_ss | residual | t4eq16384 | 5 | 300 | 448.41 ms [447.46 ms, 456.65 ms] | 1.49 ms [1.49 ms, 1.52 ms] | 8.3 [8.3, 8.5] | 0.698 [0.694, 0.707] | same |
| explicit_tri3_ss | residual | t4eq262144 | 5 | 300 | 518.23 ms [516.67 ms, 518.84 ms] | 1.73 ms [1.72 ms, 1.73 ms] | 9.6 [9.6, 9.6] | 0.804 [0.803, 0.808] | same |
| explicit_tri3_ss | residual | t4eq4096 | 5 | 300 | 445.71 ms [443.83 ms, 449.00 ms] | 1.49 ms [1.48 ms, 1.50 ms] | 8.3 [8.2, 8.3] | 0.693 [0.691, 0.695] | same |
| explicit_tri3_ss | lumped_matrix | t2eq16384 | 5 | 300 | 75.4 µs [62.8 µs, 85.1 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | t2eq262144 | 5 | 300 | 71.1 µs [52.3 µs, 80.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.001 [0.615, 1.286] | same |
| explicit_tri3_ss | lumped_matrix | t2eq4096 | 5 | 300 | 77.9 µs [66.2 µs, 78.4 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.015 [0.888, 1.241] | same |
| explicit_tri3_ss | lumped_matrix | t4eq16384 | 5 | 300 | 66.4 µs [61.5 µs, 68.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.891 [0.785, 0.981] | same |
| explicit_tri3_ss | lumped_matrix | t4eq262144 | 5 | 300 | 73.1 µs [59.5 µs, 79.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.948 [0.792, 1.030] | same |
| explicit_tri3_ss | lumped_matrix | t4eq4096 | 5 | 300 | 67.7 µs [59.8 µs, 74.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.898 [0.821, 1.020] | same |
| explicit_tri3_ss | corrector | t2eq16384 | 5 | 300 | 44.0 µs [36.3 µs, 49.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | t2eq262144 | 5 | 300 | 42.9 µs [29.8 µs, 50.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.975 [0.710, 1.181] | same |
| explicit_tri3_ss | corrector | t2eq4096 | 5 | 300 | 35.7 µs [31.0 µs, 66.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.982 [0.666, 1.645] | same |
| explicit_tri3_ss | corrector | t4eq16384 | 5 | 300 | 24.4 µs [21.3 µs, 46.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.538 [0.484, 1.281] | same |
| explicit_tri3_ss | corrector | t4eq262144 | 5 | 300 | 36.8 µs [20.6 µs, 56.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.899 [0.468, 1.546] | same |
| explicit_tri3_ss | corrector | t4eq4096 | 5 | 300 | 31.8 µs [24.9 µs, 40.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.857 [0.565, 0.993] | same |
| explicit_tri3_ss | step_hooks | t2eq16384 | 5 | 600 | 823.1 µs [761.1 µs, 850.3 µs] | 1.4 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | t2eq262144 | 5 | 600 | 839.4 µs [755.3 µs, 869.1 µs] | 1.4 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.038 [0.888, 1.102] | same |
| explicit_tri3_ss | step_hooks | t2eq4096 | 5 | 600 | 837.9 µs [799.2 µs, 896.8 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.018 [0.970, 1.148] | same |
| explicit_tri3_ss | step_hooks | t4eq16384 | 5 | 600 | 741.5 µs [730.7 µs, 846.3 µs] | 1.2 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.974 [0.859, 1.035] | same |
| explicit_tri3_ss | step_hooks | t4eq262144 | 5 | 600 | 770.3 µs [740.7 µs, 845.2 µs] | 1.3 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.927 [0.899, 1.110] | same |
| explicit_tri3_ss | step_hooks | t4eq4096 | 5 | 600 | 816.3 µs [731.0 µs, 868.4 µs] | 1.4 µs [1.2 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 1.013 [0.957, 1.054] | same |
| explicit_tri3_ss | step_rest | t2eq16384 | 5 | 300 | 184.47 ms [183.19 ms, 185.70 ms] | 614.9 µs [610.6 µs, 619.0 µs] | 3.4 [3.4, 3.4] | – | same |
| explicit_tri3_ss | step_rest | t2eq262144 | 5 | 300 | 268.47 ms [266.77 ms, 268.72 ms] | 894.9 µs [889.2 µs, 895.7 µs] | 5.0 [4.9, 5.0] | 1.456 [1.447, 1.458] | same |
| explicit_tri3_ss | step_rest | t2eq4096 | 5 | 300 | 184.32 ms [183.31 ms, 184.97 ms] | 614.4 µs [611.0 µs, 616.6 µs] | 3.4 [3.4, 3.4] | 1.000 [0.995, 1.001] | same |
| explicit_tri3_ss | step_rest | t4eq16384 | 5 | 300 | 147.20 ms [146.07 ms, 150.30 ms] | 490.7 µs [486.9 µs, 501.0 µs] | 2.7 [2.7, 2.8] | 0.800 [0.793, 0.809] | same |
| explicit_tri3_ss | step_rest | t4eq262144 | 5 | 300 | 267.26 ms [264.98 ms, 268.03 ms] | 890.9 µs [883.3 µs, 893.4 µs] | 4.9 [4.9, 5.0] | 1.449 [1.427, 1.459] | same |
| explicit_tri3_ss | step_rest | t4eq4096 | 5 | 300 | 144.04 ms [143.26 ms, 146.44 ms] | 480.1 µs [477.5 µs, 488.1 µs] | 2.7 [2.7, 2.7] | 0.782 [0.779, 0.789] | same |
| explicit_tri3_ss | internal_forces | t2eq16384 | 5 | 60 | 101.66 ms [101.49 ms, 102.15 ms] | 1.69 ms [1.69 ms, 1.70 ms] | 9.4 [9.4, 9.5] | – | same |
| explicit_tri3_ss | internal_forces | t2eq262144 | 5 | 60 | 101.89 ms [101.46 ms, 102.51 ms] | 1.70 ms [1.69 ms, 1.71 ms] | 9.4 [9.4, 9.5] | 1.004 [0.997, 1.008] | same |
| explicit_tri3_ss | internal_forces | t2eq4096 | 5 | 60 | 101.36 ms [100.04 ms, 102.53 ms] | 1.69 ms [1.67 ms, 1.71 ms] | 9.4 [9.3, 9.5] | 0.997 [0.986, 1.009] | same |
| explicit_tri3_ss | internal_forces | t4eq16384 | 5 | 60 | 64.97 ms [64.90 ms, 66.08 ms] | 1.08 ms [1.08 ms, 1.10 ms] | 6.0 [6.0, 6.1] | 0.639 [0.636, 0.651] | same |
| explicit_tri3_ss | internal_forces | t4eq262144 | 5 | 60 | 64.90 ms [64.62 ms, 65.09 ms] | 1.08 ms [1.08 ms, 1.08 ms] | 6.0 [6.0, 6.0] | 0.637 [0.635, 0.641] | same |
| explicit_tri3_ss | internal_forces | t4eq4096 | 5 | 60 | 64.97 ms [64.62 ms, 72.47 ms] | 1.08 ms [1.08 ms, 1.21 ms] | 6.0 [6.0, 6.7] | 0.639 [0.636, 0.713] | same |
| explicit_tri3_ss | stresses | t2eq16384 | 5 | 60 | 49.60 ms [49.24 ms, 49.90 ms] | 826.6 µs [820.6 µs, 831.6 µs] | 4.6 [4.6, 4.6] | – | same |
| explicit_tri3_ss | stresses | t2eq262144 | 5 | 60 | 49.43 ms [49.29 ms, 50.03 ms] | 823.8 µs [821.5 µs, 833.9 µs] | 4.6 [4.6, 4.6] | 0.998 [0.990, 1.008] | same |
| explicit_tri3_ss | stresses | t2eq4096 | 5 | 60 | 49.19 ms [48.53 ms, 49.67 ms] | 819.9 µs [808.9 µs, 827.8 µs] | 4.6 [4.5, 4.6] | 0.990 [0.978, 1.002] | same |
| explicit_tri3_ss | stresses | t4eq16384 | 5 | 60 | 37.86 ms [37.21 ms, 39.12 ms] | 631.0 µs [620.2 µs, 652.1 µs] | 3.5 [3.4, 3.6] | 0.769 [0.750, 0.784] | same |
| explicit_tri3_ss | stresses | t4eq262144 | 5 | 60 | 37.46 ms [37.16 ms, 38.04 ms] | 624.3 µs [619.3 µs, 634.1 µs] | 3.5 [3.4, 3.5] | 0.756 [0.745, 0.771] | same |
| explicit_tri3_ss | stresses | t4eq4096 | 5 | 60 | 38.17 ms [37.67 ms, 39.42 ms] | 636.2 µs [627.9 µs, 657.0 µs] | 3.5 [3.5, 3.7] | 0.775 [0.756, 0.794] | same |
| explicit_tri3_ss | energies | t2eq16384 | 5 | 60 | 136.43 ms [135.66 ms, 136.96 ms] | 2.27 ms [2.26 ms, 2.28 ms] | 12.6 [12.6, 12.7] | – | same |
| explicit_tri3_ss | energies | t2eq262144 | 5 | 60 | 136.45 ms [135.68 ms, 136.67 ms] | 2.27 ms [2.26 ms, 2.28 ms] | 12.6 [12.6, 12.7] | 1.000 [0.997, 1.002] | same |
| explicit_tri3_ss | energies | t2eq4096 | 5 | 60 | 136.05 ms [135.72 ms, 136.55 ms] | 2.27 ms [2.26 ms, 2.28 ms] | 12.6 [12.6, 12.6] | 0.999 [0.993, 1.001] | same |
| explicit_tri3_ss | energies | t4eq16384 | 5 | 60 | 136.57 ms [136.03 ms, 137.36 ms] | 2.28 ms [2.27 ms, 2.29 ms] | 12.6 [12.6, 12.7] | 1.001 [0.993, 1.013] | same |
| explicit_tri3_ss | energies | t4eq262144 | 5 | 60 | 136.72 ms [135.89 ms, 136.83 ms] | 2.28 ms [2.26 ms, 2.28 ms] | 12.7 [12.6, 12.7] | 1.002 [0.998, 1.003] | same |
| explicit_tri3_ss | energies | t4eq4096 | 5 | 60 | 136.55 ms [135.97 ms, 136.83 ms] | 2.28 ms [2.27 ms, 2.28 ms] | 12.6 [12.6, 12.7] | 1.003 [0.993, 1.003] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 7a6ad8f42 | 5c844dbf1dc6d9c8 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7a6ad8f42 | fecb3fb4b46c8b8c |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7a6ad8f42 | b6b902f5ca71049e |
