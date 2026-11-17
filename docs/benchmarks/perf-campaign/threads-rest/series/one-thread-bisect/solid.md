Suite `solid`, size `m`; variants: `b-2ecf3c90a`, `b-4a90c1ad9`, `b-4db9886a9`, `b-9d3bf6eda`, `b-b6b3f5aa3`, `base-t1`, `head2-t1`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars | mesh | b-2ecf3c90a | 7 | 1 | 2.05 ms [2.03 ms, 2.14 ms] | 2.05 ms [2.03 ms, 2.14 ms] | 19.4 [19.3, 20.3] | 0.996 [0.953, 1.046] | same |
| explicit_mazars | mesh | b-4a90c1ad9 | 7 | 1 | 2.10 ms [2.08 ms, 2.19 ms] | 2.10 ms [2.08 ms, 2.19 ms] | 19.9 [19.7, 20.8] | 1.011 [0.981, 1.053] | same |
| explicit_mazars | mesh | b-4db9886a9 | 7 | 1 | 2.09 ms [2.05 ms, 2.20 ms] | 2.09 ms [2.05 ms, 2.20 ms] | 19.9 [19.5, 20.9] | 1.008 [0.975, 1.056] | same |
| explicit_mazars | mesh | b-9d3bf6eda | 7 | 1 | 2.07 ms [2.04 ms, 2.13 ms] | 2.07 ms [2.04 ms, 2.13 ms] | 19.6 [19.4, 20.2] | 0.999 [0.962, 1.027] | same |
| explicit_mazars | mesh | b-b6b3f5aa3 | 7 | 1 | 2.06 ms [2.02 ms, 2.11 ms] | 2.06 ms [2.02 ms, 2.11 ms] | 19.5 [19.2, 20.0] | 1.004 [0.958, 1.021] | same |
| explicit_mazars | mesh | base-t1 | 7 | 1 | 2.07 ms [2.04 ms, 2.15 ms] | 2.07 ms [2.04 ms, 2.15 ms] | 19.6 [19.3, 20.4] | – | same |
| explicit_mazars | mesh | head2-t1 | 7 | 1 | 2.08 ms [2.06 ms, 2.12 ms] | 2.08 ms [2.06 ms, 2.12 ms] | 19.8 [19.5, 20.1] | 1.001 [0.985, 1.035] | same |
| explicit_mazars | init | b-2ecf3c90a | 7 | 1 | 32.31 ms [31.74 ms, 33.23 ms] | 32.31 ms [31.74 ms, 33.23 ms] | 306.4 [300.9, 315.1] | 0.984 [0.947, 1.032] | same |
| explicit_mazars | init | b-4a90c1ad9 | 7 | 1 | 32.85 ms [32.41 ms, 33.08 ms] | 32.85 ms [32.41 ms, 33.08 ms] | 311.5 [307.3, 313.6] | 0.991 [0.983, 1.007] | same |
| explicit_mazars | init | b-4db9886a9 | 7 | 1 | 34.01 ms [33.58 ms, 34.66 ms] | 34.01 ms [33.58 ms, 34.66 ms] | 322.5 [318.4, 328.7] | 1.039 [1.001, 1.056] | same |
| explicit_mazars | init | b-9d3bf6eda | 7 | 1 | 32.30 ms [31.61 ms, 32.78 ms] | 32.30 ms [31.61 ms, 32.78 ms] | 306.3 [299.8, 310.8] | 0.979 [0.943, 1.003] | same |
| explicit_mazars | init | b-b6b3f5aa3 | 7 | 1 | 32.89 ms [32.30 ms, 33.27 ms] | 32.89 ms [32.30 ms, 33.27 ms] | 311.9 [306.3, 315.5] | 1.000 [0.976, 1.010] | same |
| explicit_mazars | init | base-t1 | 7 | 1 | 32.87 ms [32.20 ms, 33.53 ms] | 32.87 ms [32.20 ms, 33.53 ms] | 311.7 [305.3, 317.9] | – | same |
| explicit_mazars | init | head2-t1 | 7 | 1 | 32.66 ms [32.43 ms, 33.45 ms] | 32.66 ms [32.43 ms, 33.45 ms] | 309.7 [307.5, 317.2] | 1.001 [0.970, 1.014] | same |
| explicit_mazars | stable_time_step | b-2ecf3c90a | 7 | 5 | 21.10 ms [21.06 ms, 21.19 ms] | 4.22 ms [4.21 ms, 4.24 ms] | 40.0 [39.9, 40.2] | 1.006 [1.001, 1.009] | same |
| explicit_mazars | stable_time_step | b-4a90c1ad9 | 7 | 5 | 21.09 ms [21.03 ms, 21.49 ms] | 4.22 ms [4.21 ms, 4.30 ms] | 40.0 [39.9, 40.8] | 1.006 [1.000, 1.020] | same |
| explicit_mazars | stable_time_step | b-4db9886a9 | 7 | 5 | 21.11 ms [21.00 ms, 21.67 ms] | 4.22 ms [4.20 ms, 4.33 ms] | 40.0 [39.8, 41.1] | 1.005 [1.002, 1.032] | same |
| explicit_mazars | stable_time_step | b-9d3bf6eda | 7 | 5 | 21.10 ms [21.04 ms, 21.56 ms] | 4.22 ms [4.21 ms, 4.31 ms] | 40.0 [39.9, 40.9] | 1.008 [1.001, 1.030] | same |
| explicit_mazars | stable_time_step | b-b6b3f5aa3 | 7 | 5 | 20.96 ms [20.94 ms, 21.38 ms] | 4.19 ms [4.19 ms, 4.28 ms] | 39.7 [39.7, 40.6] | 1.001 [0.993, 1.015] | same |
| explicit_mazars | stable_time_step | base-t1 | 7 | 5 | 21.00 ms [20.92 ms, 21.08 ms] | 4.20 ms [4.18 ms, 4.22 ms] | 39.8 [39.7, 40.0] | – | same |
| explicit_mazars | stable_time_step | head2-t1 | 7 | 5 | 21.23 ms [21.10 ms, 22.01 ms] | 4.25 ms [4.22 ms, 4.40 ms] | 40.3 [40.0, 41.7] | 1.011 [1.005, 1.044] | same |
| explicit_mazars | solve_step | b-2ecf3c90a | 7 | 80 | 2.700 s [2.688 s, 2.713 s] | 33.75 ms [33.60 ms, 33.91 ms] | 320.0 [318.6, 321.6] | 1.006 [1.003, 1.010] | same |
| explicit_mazars | solve_step | b-4a90c1ad9 | 7 | 80 | 2.723 s [2.711 s, 2.786 s] | 34.04 ms [33.89 ms, 34.83 ms] | 322.8 [321.3, 330.2] | 1.014 [1.011, 1.037] | same |
| explicit_mazars | solve_step | b-4db9886a9 | 7 | 80 | 2.725 s [2.716 s, 2.731 s] | 34.07 ms [33.95 ms, 34.14 ms] | 323.0 [321.9, 323.7] | 1.015 [1.012, 1.019] | same |
| explicit_mazars | solve_step | b-9d3bf6eda | 7 | 80 | 2.715 s [2.701 s, 2.724 s] | 33.93 ms [33.76 ms, 34.05 ms] | 321.8 [320.1, 322.8] | 1.011 [1.005, 1.016] | same |
| explicit_mazars | solve_step | b-b6b3f5aa3 | 7 | 80 | 2.701 s [2.696 s, 2.708 s] | 33.76 ms [33.70 ms, 33.85 ms] | 320.1 [319.6, 321.0] | 1.006 [1.005, 1.009] | same |
| explicit_mazars | solve_step | base-t1 | 7 | 80 | 2.685 s [2.680 s, 2.687 s] | 33.56 ms [33.51 ms, 33.59 ms] | 318.2 [317.7, 318.5] | – | same |
| explicit_mazars | solve_step | head2-t1 | 7 | 80 | 2.729 s [2.717 s, 2.733 s] | 34.12 ms [33.97 ms, 34.16 ms] | 323.5 [322.1, 323.9] | 1.016 [1.012, 1.017] | same |
| explicit_mazars | predictor | b-2ecf3c90a | 7 | 80 | 14.7 µs [7.4 µs, 17.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.695 [0.929, 4.423] | same |
| explicit_mazars | predictor | b-4a90c1ad9 | 7 | 80 | 15.8 µs [12.3 µs, 20.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.511 [1.237, 4.473] | same |
| explicit_mazars | predictor | b-4db9886a9 | 7 | 80 | 17.8 µs [17.5 µs, 19.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.222 [1.487, 5.835] | same |
| explicit_mazars | predictor | b-9d3bf6eda | 7 | 80 | 13.7 µs [11.2 µs, 16.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.481 [1.083, 5.008] | same |
| explicit_mazars | predictor | b-b6b3f5aa3 | 7 | 80 | 9.8 µs [7.6 µs, 16.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.049 [0.738, 4.451] | same |
| explicit_mazars | predictor | base-t1 | 7 | 80 | 8.0 µs [3.3 µs, 12.7 µs] | 0.1 µs [0.0 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | head2-t1 | 7 | 80 | 15.8 µs [13.8 µs, 18.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.896 [1.229, 5.454] | same |
| explicit_mazars | residual | b-2ecf3c90a | 7 | 80 | 2.536 s [2.524 s, 2.549 s] | 31.69 ms [31.55 ms, 31.86 ms] | 300.5 [299.2, 302.1] | 1.006 [1.002, 1.009] | same |
| explicit_mazars | residual | b-4a90c1ad9 | 7 | 80 | 2.559 s [2.547 s, 2.622 s] | 31.99 ms [31.84 ms, 32.78 ms] | 303.3 [302.0, 310.8] | 1.014 [1.011, 1.038] | same |
| explicit_mazars | residual | b-4db9886a9 | 7 | 80 | 2.561 s [2.552 s, 2.566 s] | 32.01 ms [31.90 ms, 32.08 ms] | 303.5 [302.5, 304.2] | 1.015 [1.013, 1.018] | same |
| explicit_mazars | residual | b-9d3bf6eda | 7 | 80 | 2.550 s [2.537 s, 2.559 s] | 31.87 ms [31.71 ms, 31.98 ms] | 302.2 [300.7, 303.3] | 1.011 [1.005, 1.016] | same |
| explicit_mazars | residual | b-b6b3f5aa3 | 7 | 80 | 2.538 s [2.535 s, 2.546 s] | 31.73 ms [31.69 ms, 31.82 ms] | 300.9 [300.5, 301.8] | 1.006 [1.005, 1.010] | same |
| explicit_mazars | residual | base-t1 | 7 | 80 | 2.521 s [2.519 s, 2.526 s] | 31.52 ms [31.49 ms, 31.57 ms] | 298.9 [298.6, 299.4] | – | same |
| explicit_mazars | residual | head2-t1 | 7 | 80 | 2.565 s [2.555 s, 2.568 s] | 32.06 ms [31.93 ms, 32.10 ms] | 304.0 [302.8, 304.4] | 1.017 [1.011, 1.018] | same |
| explicit_mazars | lumped_matrix | b-2ecf3c90a | 7 | 80 | 16.6 µs [15.2 µs, 22.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.881 [0.693, 1.454] | same |
| explicit_mazars | lumped_matrix | b-4a90c1ad9 | 7 | 80 | 22.6 µs [21.2 µs, 24.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.259 [0.998, 1.590] | same |
| explicit_mazars | lumped_matrix | b-4db9886a9 | 7 | 80 | 16.7 µs [15.3 µs, 30.1 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.908 [0.803, 1.535] | same |
| explicit_mazars | lumped_matrix | b-9d3bf6eda | 7 | 80 | 17.9 µs [15.2 µs, 20.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.895 [0.753, 1.178] | same |
| explicit_mazars | lumped_matrix | b-b6b3f5aa3 | 7 | 80 | 16.5 µs [15.5 µs, 17.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.880 [0.779, 1.085] | same |
| explicit_mazars | lumped_matrix | base-t1 | 7 | 80 | 17.9 µs [15.2 µs, 22.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | head2-t1 | 7 | 80 | 22.0 µs [18.8 µs, 27.1 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.212 [0.942, 1.517] | same |
| explicit_mazars | corrector | b-2ecf3c90a | 7 | 80 | 14.3 µs [13.8 µs, 22.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.126 [1.040, 1.741] | same |
| explicit_mazars | corrector | b-4a90c1ad9 | 7 | 80 | 14.3 µs [13.3 µs, 16.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.149 [1.004, 1.247] | same |
| explicit_mazars | corrector | b-4db9886a9 | 7 | 80 | 13.7 µs [12.8 µs, 14.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.044 [0.964, 1.140] | same |
| explicit_mazars | corrector | b-9d3bf6eda | 7 | 80 | 21.4 µs [20.0 µs, 22.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.660 [1.580, 1.740] | same |
| explicit_mazars | corrector | b-b6b3f5aa3 | 7 | 80 | 12.8 µs [12.4 µs, 21.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.035 [0.924, 1.689] | same |
| explicit_mazars | corrector | base-t1 | 7 | 80 | 12.6 µs [12.4 µs, 13.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | head2-t1 | 7 | 80 | 21.4 µs [13.8 µs, 31.4 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.726 [1.038, 2.538] | same |
| explicit_mazars | step_hooks | b-2ecf3c90a | 7 | 160 | 133.95 ms [133.67 ms, 134.26 ms] | 837.2 µs [835.5 µs, 839.1 µs] | 7.9 [7.9, 8.0] | 1.012 [1.001, 1.017] | same |
| explicit_mazars | step_hooks | b-4a90c1ad9 | 7 | 160 | 133.78 ms [133.16 ms, 134.39 ms] | 836.1 µs [832.2 µs, 840.0 µs] | 7.9 [7.9, 8.0] | 1.014 [1.002, 1.016] | same |
| explicit_mazars | step_hooks | b-4db9886a9 | 7 | 160 | 133.75 ms [133.37 ms, 134.12 ms] | 835.9 µs [833.6 µs, 838.2 µs] | 7.9 [7.9, 7.9] | 1.011 [0.999, 1.017] | same |
| explicit_mazars | step_hooks | b-9d3bf6eda | 7 | 160 | 134.08 ms [133.31 ms, 134.89 ms] | 838.0 µs [833.2 µs, 843.0 µs] | 7.9 [7.9, 8.0] | 1.012 [1.003, 1.022] | same |
| explicit_mazars | step_hooks | b-b6b3f5aa3 | 7 | 160 | 132.82 ms [132.00 ms, 134.27 ms] | 830.1 µs [825.0 µs, 839.2 µs] | 7.9 [7.8, 8.0] | 1.005 [0.989, 1.015] | same |
| explicit_mazars | step_hooks | base-t1 | 7 | 160 | 132.33 ms [131.38 ms, 133.50 ms] | 827.1 µs [821.2 µs, 834.4 µs] | 7.8 [7.8, 7.9] | – | same |
| explicit_mazars | step_hooks | head2-t1 | 7 | 160 | 133.64 ms [132.84 ms, 134.40 ms] | 835.3 µs [830.2 µs, 840.0 µs] | 7.9 [7.9, 8.0] | 1.012 [1.001, 1.015] | same |
| explicit_mazars | step_rest | b-2ecf3c90a | 7 | 80 | 30.14 ms [29.81 ms, 30.36 ms] | 376.7 µs [372.7 µs, 379.6 µs] | 3.6 [3.5, 3.6] | 1.030 [1.019, 1.043] | same |
| explicit_mazars | step_rest | b-4a90c1ad9 | 7 | 80 | 29.48 ms [29.04 ms, 30.29 ms] | 368.5 µs [363.0 µs, 378.7 µs] | 3.5 [3.4, 3.6] | 1.003 [0.988, 1.046] | same |
| explicit_mazars | step_rest | b-4db9886a9 | 7 | 80 | 30.21 ms [29.93 ms, 30.44 ms] | 377.6 µs [374.1 µs, 380.5 µs] | 3.6 [3.5, 3.6] | 1.034 [1.020, 1.043] | same |
| explicit_mazars | step_rest | b-9d3bf6eda | 7 | 80 | 30.38 ms [30.01 ms, 30.66 ms] | 379.7 µs [375.1 µs, 383.3 µs] | 3.6 [3.6, 3.6] | 1.036 [1.021, 1.053] | same |
| explicit_mazars | step_rest | b-b6b3f5aa3 | 7 | 80 | 28.60 ms [28.49 ms, 29.12 ms] | 357.5 µs [356.1 µs, 364.0 µs] | 3.4 [3.4, 3.5] | 0.979 [0.972, 0.986] | same |
| explicit_mazars | step_rest | base-t1 | 7 | 80 | 29.37 ms [28.95 ms, 29.53 ms] | 367.1 µs [361.9 µs, 369.1 µs] | 3.5 [3.4, 3.5] | – | same |
| explicit_mazars | step_rest | head2-t1 | 7 | 80 | 29.40 ms [29.02 ms, 30.31 ms] | 367.5 µs [362.7 µs, 378.9 µs] | 3.5 [3.4, 3.6] | 1.008 [0.994, 1.031] | same |
| explicit_mazars | internal_forces | b-2ecf3c90a | 7 | 16 | 545.70 ms [537.06 ms, 548.47 ms] | 34.11 ms [33.57 ms, 34.28 ms] | 323.4 [318.3, 325.1] | 1.006 [0.995, 1.013] | same |
| explicit_mazars | internal_forces | b-4a90c1ad9 | 7 | 16 | 549.96 ms [547.06 ms, 553.32 ms] | 34.37 ms [34.19 ms, 34.58 ms] | 325.9 [324.2, 327.9] | 1.013 [1.010, 1.019] | same |
| explicit_mazars | internal_forces | b-4db9886a9 | 7 | 16 | 549.94 ms [547.68 ms, 562.38 ms] | 34.37 ms [34.23 ms, 35.15 ms] | 325.9 [324.6, 333.3] | 1.018 [1.006, 1.031] | same |
| explicit_mazars | internal_forces | b-9d3bf6eda | 7 | 16 | 546.77 ms [540.82 ms, 549.97 ms] | 34.17 ms [33.80 ms, 34.37 ms] | 324.0 [320.5, 326.0] | 1.011 [1.002, 1.014] | same |
| explicit_mazars | internal_forces | b-b6b3f5aa3 | 7 | 16 | 548.34 ms [545.35 ms, 557.39 ms] | 34.27 ms [34.08 ms, 34.84 ms] | 325.0 [323.2, 330.3] | 1.012 [1.001, 1.022] | same |
| explicit_mazars | internal_forces | base-t1 | 7 | 16 | 541.75 ms [539.19 ms, 545.23 ms] | 33.86 ms [33.70 ms, 34.08 ms] | 321.1 [319.6, 323.1] | – | same |
| explicit_mazars | internal_forces | head2-t1 | 7 | 16 | 549.35 ms [545.65 ms, 551.89 ms] | 34.33 ms [34.10 ms, 34.49 ms] | 325.6 [323.4, 327.1] | 1.013 [1.001, 1.024] | same |
| explicit_mazars | stresses | b-2ecf3c90a | 7 | 16 | 510.25 ms [502.12 ms, 513.33 ms] | 31.89 ms [31.38 ms, 32.08 ms] | 302.4 [297.6, 304.2] | 1.007 [0.987, 1.010] | same |
| explicit_mazars | stresses | b-4a90c1ad9 | 7 | 16 | 514.92 ms [512.84 ms, 516.34 ms] | 32.18 ms [32.05 ms, 32.27 ms] | 305.2 [303.9, 306.0] | 1.011 [1.009, 1.018] | same |
| explicit_mazars | stresses | b-4db9886a9 | 7 | 16 | 514.92 ms [514.19 ms, 517.55 ms] | 32.18 ms [32.14 ms, 32.35 ms] | 305.2 [304.7, 306.7] | 1.015 [1.009, 1.019] | same |
| explicit_mazars | stresses | b-9d3bf6eda | 7 | 16 | 512.76 ms [510.60 ms, 514.83 ms] | 32.05 ms [31.91 ms, 32.18 ms] | 303.9 [302.6, 305.1] | 1.009 [1.004, 1.016] | same |
| explicit_mazars | stresses | b-b6b3f5aa3 | 7 | 16 | 512.35 ms [510.66 ms, 517.59 ms] | 32.02 ms [31.92 ms, 32.35 ms] | 303.7 [302.7, 306.8] | 1.008 [1.004, 1.019] | same |
| explicit_mazars | stresses | base-t1 | 7 | 16 | 508.09 ms [506.73 ms, 509.47 ms] | 31.76 ms [31.67 ms, 31.84 ms] | 301.1 [300.3, 301.9] | – | same |
| explicit_mazars | stresses | head2-t1 | 7 | 16 | 516.12 ms [514.39 ms, 516.99 ms] | 32.26 ms [32.15 ms, 32.31 ms] | 305.9 [304.9, 306.4] | 1.015 [1.013, 1.018] | same |
| explicit_mazars | energies | b-2ecf3c90a | 7 | 16 | 23.90 ms [23.80 ms, 24.21 ms] | 1.49 ms [1.49 ms, 1.51 ms] | 14.2 [14.1, 14.3] | 1.059 [1.028, 1.079] | same |
| explicit_mazars | energies | b-4a90c1ad9 | 7 | 16 | 23.98 ms [23.51 ms, 24.20 ms] | 1.50 ms [1.47 ms, 1.51 ms] | 14.2 [13.9, 14.3] | 1.052 [1.036, 1.082] | same |
| explicit_mazars | energies | b-4db9886a9 | 7 | 16 | 23.02 ms [22.76 ms, 23.09 ms] | 1.44 ms [1.42 ms, 1.44 ms] | 13.6 [13.5, 13.7] | 1.020 [0.989, 1.030] | same |
| explicit_mazars | energies | b-9d3bf6eda | 7 | 16 | 24.06 ms [23.80 ms, 24.90 ms] | 1.50 ms [1.49 ms, 1.56 ms] | 14.3 [14.1, 14.8] | 1.062 [1.034, 1.106] | same |
| explicit_mazars | energies | b-b6b3f5aa3 | 7 | 16 | 23.38 ms [23.24 ms, 23.81 ms] | 1.46 ms [1.45 ms, 1.49 ms] | 13.9 [13.8, 14.1] | 1.041 [1.006, 1.046] | same |
| explicit_mazars | energies | base-t1 | 7 | 16 | 22.50 ms [22.36 ms, 23.25 ms] | 1.41 ms [1.40 ms, 1.45 ms] | 13.3 [13.3, 13.8] | – | same |
| explicit_mazars | energies | head2-t1 | 7 | 16 | 23.74 ms [23.52 ms, 24.46 ms] | 1.48 ms [1.47 ms, 1.53 ms] | 14.1 [13.9, 14.5] | 1.055 [1.012, 1.089] | same |
| explicit_multi_material | mesh | b-2ecf3c90a | 7 | 1 | 2.09 ms [2.06 ms, 2.15 ms] | 2.09 ms [2.06 ms, 2.15 ms] | 19.8 [19.5, 20.4] | 1.006 [0.966, 1.024] | same |
| explicit_multi_material | mesh | b-4a90c1ad9 | 7 | 1 | 2.12 ms [2.08 ms, 2.17 ms] | 2.12 ms [2.08 ms, 2.17 ms] | 20.1 [19.7, 20.6] | 1.004 [0.992, 1.025] | same |
| explicit_multi_material | mesh | b-4db9886a9 | 7 | 1 | 2.13 ms [2.07 ms, 2.16 ms] | 2.13 ms [2.07 ms, 2.16 ms] | 20.2 [19.6, 20.5] | 1.006 [0.980, 1.047] | same |
| explicit_multi_material | mesh | b-9d3bf6eda | 7 | 1 | 2.12 ms [2.08 ms, 2.19 ms] | 2.12 ms [2.08 ms, 2.19 ms] | 20.1 [19.7, 20.7] | 1.016 [0.973, 1.031] | same |
| explicit_multi_material | mesh | b-b6b3f5aa3 | 7 | 1 | 2.13 ms [2.03 ms, 2.15 ms] | 2.13 ms [2.03 ms, 2.15 ms] | 20.2 [19.2, 20.4] | 1.007 [0.972, 1.027] | same |
| explicit_multi_material | mesh | base-t1 | 7 | 1 | 2.11 ms [2.05 ms, 2.13 ms] | 2.11 ms [2.05 ms, 2.13 ms] | 20.0 [19.5, 20.2] | – | same |
| explicit_multi_material | mesh | head2-t1 | 7 | 1 | 2.11 ms [2.06 ms, 2.18 ms] | 2.11 ms [2.06 ms, 2.18 ms] | 20.0 [19.6, 20.6] | 0.991 [0.972, 1.046] | same |
| explicit_multi_material | init | b-2ecf3c90a | 7 | 1 | 33.93 ms [32.99 ms, 34.86 ms] | 33.93 ms [32.99 ms, 34.86 ms] | 321.8 [312.9, 330.6] | 0.999 [0.926, 1.026] | same |
| explicit_multi_material | init | b-4a90c1ad9 | 7 | 1 | 33.83 ms [33.38 ms, 34.42 ms] | 33.83 ms [33.38 ms, 34.42 ms] | 320.8 [316.6, 326.4] | 1.006 [0.945, 1.026] | same |
| explicit_multi_material | init | b-4db9886a9 | 7 | 1 | 35.36 ms [35.00 ms, 35.75 ms] | 35.36 ms [35.00 ms, 35.75 ms] | 335.3 [331.8, 339.0] | 1.040 [1.003, 1.063] | same |
| explicit_multi_material | init | b-9d3bf6eda | 7 | 1 | 33.93 ms [33.75 ms, 35.29 ms] | 33.93 ms [33.75 ms, 35.29 ms] | 321.8 [320.0, 334.6] | 1.012 [0.947, 1.038] | same |
| explicit_multi_material | init | b-b6b3f5aa3 | 7 | 1 | 33.77 ms [33.44 ms, 34.18 ms] | 33.77 ms [33.44 ms, 34.18 ms] | 320.2 [317.1, 324.1] | 0.994 [0.955, 1.019] | same |
| explicit_multi_material | init | base-t1 | 7 | 1 | 33.98 ms [33.47 ms, 35.64 ms] | 33.98 ms [33.47 ms, 35.64 ms] | 322.2 [317.4, 337.9] | – | same |
| explicit_multi_material | init | head2-t1 | 7 | 1 | 34.14 ms [33.26 ms, 34.38 ms] | 34.14 ms [33.26 ms, 34.38 ms] | 323.8 [315.4, 326.0] | 1.001 [0.963, 1.018] | same |
| explicit_multi_material | stable_time_step | b-2ecf3c90a | 7 | 5 | 21.22 ms [21.13 ms, 21.61 ms] | 4.24 ms [4.23 ms, 4.32 ms] | 40.2 [40.1, 41.0] | 1.007 [1.006, 1.028] | same |
| explicit_multi_material | stable_time_step | b-4a90c1ad9 | 7 | 5 | 21.12 ms [21.07 ms, 21.27 ms] | 4.22 ms [4.21 ms, 4.25 ms] | 40.1 [40.0, 40.3] | 1.006 [1.003, 1.014] | same |
| explicit_multi_material | stable_time_step | b-4db9886a9 | 7 | 5 | 21.16 ms [21.09 ms, 21.40 ms] | 4.23 ms [4.22 ms, 4.28 ms] | 40.1 [40.0, 40.6] | 1.008 [1.003, 1.020] | same |
| explicit_multi_material | stable_time_step | b-9d3bf6eda | 7 | 5 | 21.18 ms [21.11 ms, 21.59 ms] | 4.24 ms [4.22 ms, 4.32 ms] | 40.2 [40.0, 41.0] | 1.008 [1.005, 1.029] | same |
| explicit_multi_material | stable_time_step | b-b6b3f5aa3 | 7 | 5 | 21.02 ms [20.94 ms, 21.56 ms] | 4.20 ms [4.19 ms, 4.31 ms] | 39.9 [39.7, 40.9] | 0.998 [0.997, 1.025] | same |
| explicit_multi_material | stable_time_step | base-t1 | 7 | 5 | 21.03 ms [20.98 ms, 21.06 ms] | 4.21 ms [4.20 ms, 4.21 ms] | 39.9 [39.8, 39.9] | – | same |
| explicit_multi_material | stable_time_step | head2-t1 | 7 | 5 | 24.46 ms [23.34 ms, 24.65 ms] | 4.89 ms [4.67 ms, 4.93 ms] | 46.4 [44.3, 46.8] | 1.163 [1.112, 1.171] | same |
| explicit_multi_material | solve_step | b-2ecf3c90a | 7 | 120 | 2.029 s [2.020 s, 2.040 s] | 16.91 ms [16.84 ms, 17.00 ms] | 160.3 [159.6, 161.2] | 1.017 [1.011, 1.021] | same |
| explicit_multi_material | solve_step | b-4a90c1ad9 | 7 | 120 | 2.036 s [2.024 s, 2.047 s] | 16.97 ms [16.86 ms, 17.06 ms] | 160.9 [159.9, 161.8] | 1.020 [1.012, 1.023] | same |
| explicit_multi_material | solve_step | b-4db9886a9 | 7 | 120 | 2.042 s [2.026 s, 2.046 s] | 17.02 ms [16.88 ms, 17.05 ms] | 161.4 [160.1, 161.7] | 1.020 [1.019, 1.024] | same |
| explicit_multi_material | solve_step | b-9d3bf6eda | 7 | 120 | 2.026 s [2.020 s, 2.037 s] | 16.89 ms [16.84 ms, 16.98 ms] | 160.1 [159.7, 161.0] | 1.016 [1.010, 1.018] | same |
| explicit_multi_material | solve_step | b-b6b3f5aa3 | 7 | 120 | 2.019 s [2.000 s, 2.027 s] | 16.82 ms [16.67 ms, 16.89 ms] | 159.5 [158.1, 160.2] | 1.006 [1.004, 1.015] | same |
| explicit_multi_material | solve_step | base-t1 | 7 | 120 | 1.998 s [1.988 s, 2.006 s] | 16.65 ms [16.57 ms, 16.71 ms] | 157.9 [157.1, 158.5] | – | same |
| explicit_multi_material | solve_step | head2-t1 | 7 | 120 | 2.035 s [2.019 s, 2.040 s] | 16.96 ms [16.83 ms, 17.00 ms] | 160.8 [159.6, 161.2] | 1.019 [1.010, 1.021] | same |
| explicit_multi_material | predictor | b-2ecf3c90a | 7 | 120 | 10.7 µs [8.3 µs, 17.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.041 [0.730, 1.594] | same |
| explicit_multi_material | predictor | b-4a90c1ad9 | 7 | 120 | 17.6 µs [16.3 µs, 24.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.755 [1.200, 2.214] | same |
| explicit_multi_material | predictor | b-4db9886a9 | 7 | 120 | 23.5 µs [18.2 µs, 29.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 2.098 [1.536, 3.023] | same |
| explicit_multi_material | predictor | b-9d3bf6eda | 7 | 120 | 14.1 µs [10.5 µs, 19.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.517 [0.711, 1.815] | same |
| explicit_multi_material | predictor | b-b6b3f5aa3 | 7 | 120 | 13.2 µs [11.3 µs, 13.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.208 [0.921, 1.740] | same |
| explicit_multi_material | predictor | base-t1 | 7 | 120 | 10.6 µs [7.8 µs, 14.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | predictor | head2-t1 | 7 | 120 | 17.4 µs [14.8 µs, 22.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.781 [1.221, 2.099] | same |
| explicit_multi_material | residual | b-2ecf3c90a | 7 | 120 | 1.892 s [1.883 s, 1.903 s] | 15.76 ms [15.69 ms, 15.86 ms] | 149.5 [148.8, 150.4] | 1.017 [1.011, 1.021] | same |
| explicit_multi_material | residual | b-4a90c1ad9 | 7 | 120 | 1.901 s [1.888 s, 1.911 s] | 15.84 ms [15.74 ms, 15.92 ms] | 150.2 [149.2, 151.0] | 1.021 [1.013, 1.023] | same |
| explicit_multi_material | residual | b-4db9886a9 | 7 | 120 | 1.905 s [1.890 s, 1.909 s] | 15.88 ms [15.75 ms, 15.90 ms] | 150.6 [149.4, 150.8] | 1.021 [1.020, 1.024] | same |
| explicit_multi_material | residual | b-9d3bf6eda | 7 | 120 | 1.889 s [1.883 s, 1.899 s] | 15.74 ms [15.69 ms, 15.83 ms] | 149.3 [148.8, 150.1] | 1.016 [1.010, 1.019] | same |
| explicit_multi_material | residual | b-b6b3f5aa3 | 7 | 120 | 1.884 s [1.867 s, 1.890 s] | 15.70 ms [15.56 ms, 15.75 ms] | 148.8 [147.5, 149.4] | 1.008 [1.005, 1.015] | same |
| explicit_multi_material | residual | base-t1 | 7 | 120 | 1.863 s [1.854 s, 1.870 s] | 15.53 ms [15.45 ms, 15.58 ms] | 147.3 [146.5, 147.7] | – | same |
| explicit_multi_material | residual | head2-t1 | 7 | 120 | 1.899 s [1.883 s, 1.903 s] | 15.83 ms [15.69 ms, 15.86 ms] | 150.1 [148.8, 150.4] | 1.020 [1.010, 1.022] | same |
| explicit_multi_material | lumped_matrix | b-2ecf3c90a | 7 | 120 | 26.3 µs [24.3 µs, 28.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.982 [0.849, 1.100] | same |
| explicit_multi_material | lumped_matrix | b-4a90c1ad9 | 7 | 120 | 35.6 µs [31.7 µs, 40.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.362 [1.033, 1.443] | same |
| explicit_multi_material | lumped_matrix | b-4db9886a9 | 7 | 120 | 26.0 µs [23.6 µs, 27.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.974 [0.783, 1.074] | same |
| explicit_multi_material | lumped_matrix | b-9d3bf6eda | 7 | 120 | 26.9 µs [25.1 µs, 28.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.006 [0.794, 1.091] | same |
| explicit_multi_material | lumped_matrix | b-b6b3f5aa3 | 7 | 120 | 27.6 µs [24.5 µs, 29.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.976 [0.775, 1.156] | same |
| explicit_multi_material | lumped_matrix | base-t1 | 7 | 120 | 26.7 µs [24.7 µs, 31.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | lumped_matrix | head2-t1 | 7 | 120 | 32.8 µs [29.2 µs, 36.4 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.236 [1.009, 1.407] | same |
| explicit_multi_material | corrector | b-2ecf3c90a | 7 | 120 | 18.6 µs [14.5 µs, 19.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.935 [0.791, 1.223] | same |
| explicit_multi_material | corrector | b-4a90c1ad9 | 7 | 120 | 19.7 µs [18.2 µs, 33.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.065 [1.008, 1.689] | same |
| explicit_multi_material | corrector | b-4db9886a9 | 7 | 120 | 20.3 µs [16.2 µs, 22.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.107 [0.880, 1.234] | same |
| explicit_multi_material | corrector | b-9d3bf6eda | 7 | 120 | 18.6 µs [14.3 µs, 21.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.074 [0.731, 1.189] | same |
| explicit_multi_material | corrector | b-b6b3f5aa3 | 7 | 120 | 18.3 µs [11.8 µs, 19.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.951 [0.728, 1.134] | same |
| explicit_multi_material | corrector | base-t1 | 7 | 120 | 18.4 µs [15.6 µs, 19.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | corrector | head2-t1 | 7 | 120 | 18.1 µs [8.6 µs, 19.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.950 [0.460, 1.185] | same |
| explicit_multi_material | step_hooks | b-2ecf3c90a | 7 | 240 | 94.94 ms [94.27 ms, 95.64 ms] | 395.6 µs [392.8 µs, 398.5 µs] | 3.8 [3.7, 3.8] | 1.006 [0.994, 1.010] | same |
| explicit_multi_material | step_hooks | b-4a90c1ad9 | 7 | 240 | 95.19 ms [94.30 ms, 95.68 ms] | 396.6 µs [392.9 µs, 398.7 µs] | 3.8 [3.7, 3.8] | 1.004 [0.994, 1.010] | same |
| explicit_multi_material | step_hooks | b-4db9886a9 | 7 | 240 | 95.02 ms [94.39 ms, 95.23 ms] | 395.9 µs [393.3 µs, 396.8 µs] | 3.8 [3.7, 3.8] | 1.004 [0.994, 1.009] | same |
| explicit_multi_material | step_hooks | b-9d3bf6eda | 7 | 240 | 94.83 ms [94.38 ms, 95.54 ms] | 395.1 µs [393.3 µs, 398.1 µs] | 3.7 [3.7, 3.8] | 1.005 [0.990, 1.012] | same |
| explicit_multi_material | step_hooks | b-b6b3f5aa3 | 7 | 240 | 94.75 ms [93.72 ms, 96.90 ms] | 394.8 µs [390.5 µs, 403.8 µs] | 3.7 [3.7, 3.8] | 0.999 [0.991, 1.023] | same |
| explicit_multi_material | step_hooks | base-t1 | 7 | 240 | 94.69 ms [93.98 ms, 95.73 ms] | 394.6 µs [391.6 µs, 398.9 µs] | 3.7 [3.7, 3.8] | – | same |
| explicit_multi_material | step_hooks | head2-t1 | 7 | 240 | 94.88 ms [94.56 ms, 95.11 ms] | 395.3 µs [394.0 µs, 396.3 µs] | 3.7 [3.7, 3.8] | 1.003 [0.994, 1.009] | same |
| explicit_multi_material | step_rest | b-2ecf3c90a | 7 | 120 | 41.63 ms [41.08 ms, 41.91 ms] | 346.9 µs [342.4 µs, 349.2 µs] | 3.3 [3.2, 3.3] | 1.034 [1.019, 1.051] | same |
| explicit_multi_material | step_rest | b-4a90c1ad9 | 7 | 120 | 40.68 ms [40.01 ms, 40.89 ms] | 339.0 µs [333.4 µs, 340.8 µs] | 3.2 [3.2, 3.2] | 1.009 [1.002, 1.026] | same |
| explicit_multi_material | step_rest | b-4db9886a9 | 7 | 120 | 41.32 ms [40.88 ms, 42.17 ms] | 344.4 µs [340.6 µs, 351.4 µs] | 3.3 [3.2, 3.3] | 1.031 [1.016, 1.058] | same |
| explicit_multi_material | step_rest | b-9d3bf6eda | 7 | 120 | 41.84 ms [41.40 ms, 42.58 ms] | 348.6 µs [345.0 µs, 354.8 µs] | 3.3 [3.3, 3.4] | 1.045 [1.021, 1.065] | same |
| explicit_multi_material | step_rest | b-b6b3f5aa3 | 7 | 120 | 39.58 ms [39.31 ms, 39.91 ms] | 329.8 µs [327.6 µs, 332.6 µs] | 3.1 [3.1, 3.2] | 0.984 [0.981, 1.001] | same |
| explicit_multi_material | step_rest | base-t1 | 7 | 120 | 40.14 ms [39.87 ms, 40.69 ms] | 334.5 µs [332.3 µs, 339.1 µs] | 3.2 [3.2, 3.2] | – | same |
| explicit_multi_material | step_rest | head2-t1 | 7 | 120 | 40.28 ms [40.05 ms, 41.21 ms] | 335.7 µs [333.7 µs, 343.5 µs] | 3.2 [3.2, 3.3] | 1.001 [0.994, 1.034] | same |
| explicit_multi_material | internal_forces | b-2ecf3c90a | 7 | 24 | 379.09 ms [376.57 ms, 383.72 ms] | 15.80 ms [15.69 ms, 15.99 ms] | 149.8 [148.8, 151.6] | 1.012 [1.004, 1.026] | same |
| explicit_multi_material | internal_forces | b-4a90c1ad9 | 7 | 24 | 378.27 ms [376.35 ms, 393.64 ms] | 15.76 ms [15.68 ms, 16.40 ms] | 149.5 [148.7, 155.5] | 1.012 [1.007, 1.047] | same |
| explicit_multi_material | internal_forces | b-4db9886a9 | 7 | 24 | 379.77 ms [377.65 ms, 380.70 ms] | 15.82 ms [15.74 ms, 15.86 ms] | 150.1 [149.2, 150.4] | 1.012 [1.010, 1.018] | same |
| explicit_multi_material | internal_forces | b-9d3bf6eda | 7 | 24 | 378.17 ms [376.63 ms, 381.85 ms] | 15.76 ms [15.69 ms, 15.91 ms] | 149.4 [148.8, 150.9] | 1.011 [1.008, 1.018] | same |
| explicit_multi_material | internal_forces | b-b6b3f5aa3 | 7 | 24 | 376.54 ms [373.13 ms, 376.75 ms] | 15.69 ms [15.55 ms, 15.70 ms] | 148.8 [147.4, 148.9] | 1.003 [0.998, 1.007] | same |
| explicit_multi_material | internal_forces | base-t1 | 7 | 24 | 374.67 ms [372.15 ms, 375.88 ms] | 15.61 ms [15.51 ms, 15.66 ms] | 148.0 [147.0, 148.5] | – | same |
| explicit_multi_material | internal_forces | head2-t1 | 7 | 24 | 378.18 ms [376.53 ms, 379.88 ms] | 15.76 ms [15.69 ms, 15.83 ms] | 149.4 [148.8, 150.1] | 1.010 [1.006, 1.015] | same |
| explicit_multi_material | stresses | b-2ecf3c90a | 7 | 24 | 322.82 ms [317.37 ms, 325.24 ms] | 13.45 ms [13.22 ms, 13.55 ms] | 127.6 [125.4, 128.5] | 1.019 [1.001, 1.028] | same |
| explicit_multi_material | stresses | b-4a90c1ad9 | 7 | 24 | 320.59 ms [318.80 ms, 324.35 ms] | 13.36 ms [13.28 ms, 13.51 ms] | 126.7 [126.0, 128.2] | 1.013 [1.001, 1.026] | same |
| explicit_multi_material | stresses | b-4db9886a9 | 7 | 24 | 321.00 ms [319.17 ms, 322.13 ms] | 13.38 ms [13.30 ms, 13.42 ms] | 126.8 [126.1, 127.3] | 1.016 [1.001, 1.019] | same |
| explicit_multi_material | stresses | b-9d3bf6eda | 7 | 24 | 320.56 ms [319.36 ms, 322.35 ms] | 13.36 ms [13.31 ms, 13.43 ms] | 126.7 [126.2, 127.4] | 1.015 [0.997, 1.017] | same |
| explicit_multi_material | stresses | b-b6b3f5aa3 | 7 | 24 | 317.84 ms [315.62 ms, 318.65 ms] | 13.24 ms [13.15 ms, 13.28 ms] | 125.6 [124.7, 125.9] | 1.005 [0.994, 1.010] | same |
| explicit_multi_material | stresses | base-t1 | 7 | 24 | 316.33 ms [314.03 ms, 320.17 ms] | 13.18 ms [13.08 ms, 13.34 ms] | 125.0 [124.1, 126.5] | – | same |
| explicit_multi_material | stresses | head2-t1 | 7 | 24 | 320.72 ms [317.47 ms, 322.07 ms] | 13.36 ms [13.23 ms, 13.42 ms] | 126.7 [125.4, 127.3] | 1.014 [1.006, 1.021] | same |
| explicit_multi_material | energies | b-2ecf3c90a | 7 | 24 | 38.35 ms [38.19 ms, 39.59 ms] | 1.60 ms [1.59 ms, 1.65 ms] | 15.2 [15.1, 15.6] | 1.031 [1.019, 1.063] | same |
| explicit_multi_material | energies | b-4a90c1ad9 | 7 | 24 | 38.46 ms [38.22 ms, 38.62 ms] | 1.60 ms [1.59 ms, 1.61 ms] | 15.2 [15.1, 15.3] | 1.033 [1.018, 1.036] | same |
| explicit_multi_material | energies | b-4db9886a9 | 7 | 24 | 37.57 ms [37.43 ms, 38.09 ms] | 1.57 ms [1.56 ms, 1.59 ms] | 14.8 [14.8, 15.1] | 1.008 [1.001, 1.024] | same |
| explicit_multi_material | energies | b-9d3bf6eda | 7 | 24 | 38.38 ms [38.10 ms, 38.68 ms] | 1.60 ms [1.59 ms, 1.61 ms] | 15.2 [15.1, 15.3] | 1.031 [1.015, 1.037] | same |
| explicit_multi_material | energies | b-b6b3f5aa3 | 7 | 24 | 38.05 ms [37.86 ms, 38.47 ms] | 1.59 ms [1.58 ms, 1.60 ms] | 15.0 [15.0, 15.2] | 1.022 [1.008, 1.034] | same |
| explicit_multi_material | energies | base-t1 | 7 | 24 | 37.23 ms [37.20 ms, 37.55 ms] | 1.55 ms [1.55 ms, 1.56 ms] | 14.7 [14.7, 14.8] | – | same |
| explicit_multi_material | energies | head2-t1 | 7 | 24 | 38.54 ms [38.48 ms, 39.10 ms] | 1.61 ms [1.60 ms, 1.63 ms] | 15.2 [15.2, 15.4] | 1.035 [1.025, 1.051] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 2ecf3c90a, 4a90c1ad9, 4db9886a9, 7a6ad8f42, 9d3bf6eda, b6b3f5aa3, c866d99ab | 017c8c9ef7659aa0 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 2ecf3c90a, 4a90c1ad9, 4db9886a9, 7a6ad8f42, 9d3bf6eda, b6b3f5aa3, c866d99ab | ce8154043761dbc7 |
