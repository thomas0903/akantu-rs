Suite `solid`, size `m`; variants: `base-t1`, `base-t2`, `base-t4`, `head-t2`, `head-t4`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_hex8_fd | mesh | base-t1 | 3 | 1 | 6.60 ms [6.50 ms, 6.62 ms] | 6.60 ms [6.50 ms, 6.62 ms] | 59.7 [58.8, 59.9] | – | same |
| explicit_hex8_fd | mesh | base-t2 | 3 | 1 | 6.62 ms [6.53 ms, 6.67 ms] | 6.62 ms [6.53 ms, 6.67 ms] | 59.8 [59.0, 60.3] | 1.005 [1.003, 1.007] | same |
| explicit_hex8_fd | mesh | base-t4 | 3 | 1 | 6.61 ms [6.56 ms, 6.67 ms] | 6.61 ms [6.56 ms, 6.67 ms] | 59.7 [59.3, 60.3] | 0.998 [0.994, 1.026] | same |
| explicit_hex8_fd | mesh | head-t2 | 3 | 1 | 6.34 ms [6.33 ms, 6.44 ms] | 6.34 ms [6.33 ms, 6.44 ms] | 57.3 [57.2, 58.3] | 0.975 [0.955, 0.977] | same |
| explicit_hex8_fd | mesh | head-t4 | 3 | 1 | 6.44 ms [6.41 ms, 6.60 ms] | 6.44 ms [6.41 ms, 6.60 ms] | 58.2 [58.0, 59.7] | 0.991 [0.972, 0.997] | same |
| explicit_hex8_fd | init | base-t1 | 3 | 1 | 298.94 ms [297.44 ms, 302.00 ms] | 298.94 ms [297.44 ms, 302.00 ms] | 2703.1 [2689.6, 2730.7] | – | same |
| explicit_hex8_fd | init | base-t2 | 3 | 1 | 301.29 ms [300.27 ms, 306.52 ms] | 301.29 ms [300.27 ms, 306.52 ms] | 2724.4 [2715.1, 2771.7] | 1.009 [1.008, 1.015] | same |
| explicit_hex8_fd | init | base-t4 | 3 | 1 | 300.98 ms [299.88 ms, 301.55 ms] | 300.98 ms [299.88 ms, 301.55 ms] | 2721.5 [2711.6, 2726.7] | 1.007 [0.993, 1.014] | same |
| explicit_hex8_fd | init | head-t2 | 3 | 1 | 304.33 ms [299.29 ms, 306.12 ms] | 304.33 ms [299.29 ms, 306.12 ms] | 2751.8 [2706.3, 2768.0] | 1.014 [1.001, 1.023] | same |
| explicit_hex8_fd | init | head-t4 | 3 | 1 | 301.67 ms [300.51 ms, 301.88 ms] | 301.67 ms [300.51 ms, 301.88 ms] | 2727.8 [2717.2, 2729.7] | 1.009 [0.995, 1.015] | same |
| explicit_hex8_fd | stable_time_step | base-t1 | 3 | 5 | 23.24 ms [23.23 ms, 24.19 ms] | 4.65 ms [4.65 ms, 4.84 ms] | 42.0 [42.0, 43.8] | – | same |
| explicit_hex8_fd | stable_time_step | base-t2 | 3 | 5 | 23.27 ms [23.23 ms, 24.24 ms] | 4.65 ms [4.65 ms, 4.85 ms] | 42.1 [42.0, 43.8] | 1.001 [0.960, 1.043] | same |
| explicit_hex8_fd | stable_time_step | base-t4 | 3 | 5 | 23.24 ms [23.15 ms, 23.78 ms] | 4.65 ms [4.63 ms, 4.76 ms] | 42.0 [41.9, 43.0] | 0.996 [0.983, 1.001] | same |
| explicit_hex8_fd | stable_time_step | head-t2 | 3 | 5 | 23.22 ms [23.21 ms, 23.24 ms] | 4.64 ms [4.64 ms, 4.65 ms] | 42.0 [42.0, 42.0] | 0.999 [0.959, 1.000] | same |
| explicit_hex8_fd | stable_time_step | head-t4 | 3 | 5 | 24.43 ms [23.25 ms, 25.44 ms] | 4.89 ms [4.65 ms, 5.09 ms] | 44.2 [42.0, 46.0] | 1.010 [1.000, 1.095] | same |
| explicit_hex8_fd | solve_step | base-t1 | 3 | 30 | 1.320 s [1.314 s, 1.321 s] | 44.01 ms [43.80 ms, 44.02 ms] | 398.0 [396.1, 398.1] | – | same |
| explicit_hex8_fd | solve_step | base-t2 | 3 | 30 | 735.11 ms [734.49 ms, 741.09 ms] | 24.50 ms [24.48 ms, 24.70 ms] | 221.6 [221.4, 223.4] | 0.559 [0.557, 0.561] | same |
| explicit_hex8_fd | solve_step | base-t4 | 3 | 30 | 419.06 ms [418.84 ms, 419.60 ms] | 13.97 ms [13.96 ms, 13.99 ms] | 126.3 [126.2, 126.5] | 0.317 [0.317, 0.319] | same |
| explicit_hex8_fd | solve_step | head-t2 | 3 | 30 | 715.77 ms [715.53 ms, 719.35 ms] | 23.86 ms [23.85 ms, 23.98 ms] | 215.7 [215.7, 216.8] | 0.542 [0.542, 0.547] | same |
| explicit_hex8_fd | solve_step | head-t4 | 3 | 30 | 382.60 ms [382.53 ms, 383.63 ms] | 12.75 ms [12.75 ms, 12.79 ms] | 115.3 [115.3, 115.6] | 0.290 [0.290, 0.292] | same |
| explicit_hex8_fd | predictor | base-t1 | 3 | 30 | 4.7 µs [4.2 µs, 4.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | predictor | base-t2 | 3 | 30 | 5.4 µs [4.7 µs, 18.0 µs] | 0.2 µs [0.2 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.121 [1.113, 3.807] | same |
| explicit_hex8_fd | predictor | base-t4 | 3 | 30 | 5.2 µs [4.1 µs, 5.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.069 [0.877, 1.404] | same |
| explicit_hex8_fd | predictor | head-t2 | 3 | 30 | 4.2 µs [4.2 µs, 4.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.892 [0.867, 1.141] | same |
| explicit_hex8_fd | predictor | head-t4 | 3 | 30 | 4.8 µs [4.2 µs, 6.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.136 [0.886, 1.255] | same |
| explicit_hex8_fd | residual | base-t1 | 3 | 30 | 1.262 s [1.256 s, 1.262 s] | 42.06 ms [41.86 ms, 42.08 ms] | 380.3 [378.5, 380.5] | – | same |
| explicit_hex8_fd | residual | base-t2 | 3 | 30 | 678.08 ms [677.25 ms, 683.50 ms] | 22.60 ms [22.57 ms, 22.78 ms] | 204.4 [204.1, 206.0] | 0.539 [0.537, 0.541] | same |
| explicit_hex8_fd | residual | base-t4 | 3 | 30 | 361.40 ms [361.17 ms, 361.97 ms] | 12.05 ms [12.04 ms, 12.07 ms] | 108.9 [108.9, 109.1] | 0.286 [0.286, 0.288] | same |
| explicit_hex8_fd | residual | head-t2 | 3 | 30 | 672.21 ms [672.20 ms, 676.49 ms] | 22.41 ms [22.41 ms, 22.55 ms] | 202.6 [202.6, 203.9] | 0.533 [0.533, 0.539] | same |
| explicit_hex8_fd | residual | head-t4 | 3 | 30 | 346.37 ms [346.35 ms, 347.47 ms] | 11.55 ms [11.54 ms, 11.58 ms] | 104.4 [104.4, 104.7] | 0.274 [0.274, 0.277] | same |
| explicit_hex8_fd | lumped_matrix | base-t1 | 3 | 30 | 7.9 µs [7.7 µs, 8.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | lumped_matrix | base-t2 | 3 | 30 | 9.3 µs [9.2 µs, 10.4 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.199 [1.062, 1.307] | same |
| explicit_hex8_fd | lumped_matrix | base-t4 | 3 | 30 | 11.5 µs [11.3 µs, 11.7 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.450 [1.352, 1.467] | same |
| explicit_hex8_fd | lumped_matrix | head-t2 | 3 | 30 | 9.3 µs [9.1 µs, 10.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.174 [1.051, 1.304] | same |
| explicit_hex8_fd | lumped_matrix | head-t4 | 3 | 30 | 9.5 µs [8.3 µs, 10.7 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.088 [1.079, 1.347] | same |
| explicit_hex8_fd | corrector | base-t1 | 3 | 30 | 7.4 µs [6.9 µs, 8.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | corrector | base-t2 | 3 | 30 | 7.9 µs [6.9 µs, 10.1 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.140 [0.943, 1.260] | same |
| explicit_hex8_fd | corrector | base-t4 | 3 | 30 | 8.1 µs [7.8 µs, 8.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.170 [0.974, 1.171] | same |
| explicit_hex8_fd | corrector | head-t2 | 3 | 30 | 6.3 µs [6.0 µs, 8.6 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.821 [0.782, 1.251] | same |
| explicit_hex8_fd | corrector | head-t4 | 3 | 30 | 7.7 µs [7.7 µs, 8.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.083 [0.963, 1.113] | same |
| explicit_hex8_fd | step_hooks | base-t1 | 3 | 60 | 119.4 µs [114.9 µs, 121.4 µs] | 2.0 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_fd | step_hooks | base-t2 | 3 | 60 | 114.4 µs [113.6 µs, 118.1 µs] | 1.9 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 0.989 [0.942, 0.989] | same |
| explicit_hex8_fd | step_hooks | base-t4 | 3 | 60 | 118.6 µs [114.7 µs, 132.9 µs] | 2.0 µs [1.9 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 0.994 [0.945, 1.156] | same |
| explicit_hex8_fd | step_hooks | head-t2 | 3 | 60 | 123.3 µs [122.7 µs, 125.0 µs] | 2.1 µs [2.0 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 1.028 [1.015, 1.088] | same |
| explicit_hex8_fd | step_hooks | head-t4 | 3 | 60 | 122.3 µs [120.6 µs, 122.8 µs] | 2.0 µs [2.0 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 1.012 [1.010, 1.065] | same |
| explicit_hex8_fd | step_rest | base-t1 | 3 | 30 | 58.20 ms [58.04 ms, 58.38 ms] | 1.94 ms [1.93 ms, 1.95 ms] | 17.5 [17.5, 17.6] | – | same |
| explicit_hex8_fd | step_rest | base-t2 | 3 | 30 | 57.09 ms [56.87 ms, 57.45 ms] | 1.90 ms [1.90 ms, 1.92 ms] | 17.2 [17.1, 17.3] | 0.984 [0.974, 0.987] | same |
| explicit_hex8_fd | step_rest | base-t4 | 3 | 30 | 57.48 ms [57.30 ms, 57.73 ms] | 1.92 ms [1.91 ms, 1.92 ms] | 17.3 [17.3, 17.4] | 0.990 [0.981, 0.992] | same |
| explicit_hex8_fd | step_rest | head-t2 | 3 | 30 | 43.18 ms [42.71 ms, 43.40 ms] | 1.44 ms [1.42 ms, 1.45 ms] | 13.0 [12.9, 13.1] | 0.740 [0.736, 0.746] | same |
| explicit_hex8_fd | step_rest | head-t4 | 3 | 30 | 36.03 ms [36.01 ms, 36.08 ms] | 1.20 ms [1.20 ms, 1.20 ms] | 10.9 [10.9, 10.9] | 0.620 [0.617, 0.620] | same |
| explicit_hex8_fd | internal_forces | base-t1 | 3 | 6 | 247.76 ms [247.33 ms, 247.91 ms] | 41.29 ms [41.22 ms, 41.32 ms] | 373.4 [372.7, 373.6] | – | same |
| explicit_hex8_fd | internal_forces | base-t2 | 3 | 6 | 129.03 ms [127.94 ms, 129.41 ms] | 21.50 ms [21.32 ms, 21.57 ms] | 194.5 [192.8, 195.0] | 0.520 [0.517, 0.522] | same |
| explicit_hex8_fd | internal_forces | base-t4 | 3 | 6 | 65.93 ms [65.51 ms, 66.00 ms] | 10.99 ms [10.92 ms, 11.00 ms] | 99.4 [98.7, 99.5] | 0.266 [0.264, 0.267] | same |
| explicit_hex8_fd | internal_forces | head-t2 | 3 | 6 | 128.51 ms [127.90 ms, 128.64 ms] | 21.42 ms [21.32 ms, 21.44 ms] | 193.7 [192.7, 193.9] | 0.518 [0.516, 0.520] | same |
| explicit_hex8_fd | internal_forces | head-t4 | 3 | 6 | 64.98 ms [64.36 ms, 64.98 ms] | 10.83 ms [10.73 ms, 10.83 ms] | 97.9 [97.0, 97.9] | 0.262 [0.260, 0.263] | same |
| explicit_hex8_fd | stresses | base-t1 | 3 | 6 | 112.83 ms [110.61 ms, 113.48 ms] | 18.81 ms [18.44 ms, 18.91 ms] | 170.0 [166.7, 171.0] | – | same |
| explicit_hex8_fd | stresses | base-t2 | 3 | 6 | 81.86 ms [81.45 ms, 82.49 ms] | 13.64 ms [13.58 ms, 13.75 ms] | 123.4 [122.8, 124.3] | 0.727 [0.725, 0.736] | same |
| explicit_hex8_fd | stresses | base-t4 | 3 | 6 | 70.52 ms [70.13 ms, 71.93 ms] | 11.75 ms [11.69 ms, 11.99 ms] | 106.3 [105.7, 108.4] | 0.634 [0.622, 0.638] | same |
| explicit_hex8_fd | stresses | head-t2 | 3 | 6 | 81.31 ms [80.82 ms, 81.65 ms] | 13.55 ms [13.47 ms, 13.61 ms] | 122.5 [121.8, 123.0] | 0.724 [0.712, 0.735] | same |
| explicit_hex8_fd | stresses | head-t4 | 3 | 6 | 69.76 ms [69.65 ms, 70.15 ms] | 11.63 ms [11.61 ms, 11.69 ms] | 105.1 [105.0, 105.7] | 0.617 [0.615, 0.634] | same |
| explicit_hex8_fd | energies | base-t1 | 3 | 6 | 47.31 ms [47.28 ms, 47.32 ms] | 7.89 ms [7.88 ms, 7.89 ms] | 71.3 [71.3, 71.3] | – | same |
| explicit_hex8_fd | energies | base-t2 | 3 | 6 | 47.35 ms [47.28 ms, 47.36 ms] | 7.89 ms [7.88 ms, 7.89 ms] | 71.4 [71.2, 71.4] | 1.001 [1.000, 1.001] | same |
| explicit_hex8_fd | energies | base-t4 | 3 | 6 | 47.49 ms [47.22 ms, 47.63 ms] | 7.92 ms [7.87 ms, 7.94 ms] | 71.6 [71.2, 71.8] | 1.004 [0.998, 1.007] | same |
| explicit_hex8_fd | energies | head-t2 | 3 | 6 | 47.51 ms [47.39 ms, 47.61 ms] | 7.92 ms [7.90 ms, 7.94 ms] | 71.6 [71.4, 71.8] | 1.004 [1.002, 1.006] | same |
| explicit_hex8_fd | energies | head-t4 | 3 | 6 | 47.47 ms [47.34 ms, 47.52 ms] | 7.91 ms [7.89 ms, 7.92 ms] | 71.5 [71.3, 71.6] | 1.003 [1.001, 1.004] | same |
| explicit_hex8_ss | mesh | base-t1 | 3 | 1 | 6.61 ms [6.52 ms, 6.71 ms] | 6.61 ms [6.52 ms, 6.71 ms] | 59.7 [58.9, 60.7] | – | same |
| explicit_hex8_ss | mesh | base-t2 | 3 | 1 | 6.65 ms [6.55 ms, 6.71 ms] | 6.65 ms [6.55 ms, 6.71 ms] | 60.1 [59.3, 60.7] | 1.005 [0.991, 1.016] | same |
| explicit_hex8_ss | mesh | base-t4 | 3 | 1 | 6.58 ms [6.52 ms, 6.95 ms] | 6.58 ms [6.52 ms, 6.95 ms] | 59.5 [59.0, 62.9] | 1.009 [0.972, 1.052] | same |
| explicit_hex8_ss | mesh | head-t2 | 3 | 1 | 6.41 ms [6.32 ms, 6.54 ms] | 6.41 ms [6.32 ms, 6.54 ms] | 57.9 [57.2, 59.1] | 0.970 [0.970, 0.974] | same |
| explicit_hex8_ss | mesh | head-t4 | 3 | 1 | 6.42 ms [6.37 ms, 6.65 ms] | 6.42 ms [6.37 ms, 6.65 ms] | 58.0 [57.6, 60.1] | 0.978 [0.956, 1.007] | same |
| explicit_hex8_ss | init | base-t1 | 3 | 1 | 264.86 ms [264.32 ms, 268.56 ms] | 264.86 ms [264.32 ms, 268.56 ms] | 2394.9 [2390.1, 2428.4] | – | same |
| explicit_hex8_ss | init | base-t2 | 3 | 1 | 267.82 ms [264.58 ms, 271.58 ms] | 267.82 ms [264.58 ms, 271.58 ms] | 2421.7 [2392.4, 2455.7] | 1.001 [0.997, 1.025] | same |
| explicit_hex8_ss | init | base-t4 | 3 | 1 | 267.40 ms [267.37 ms, 269.89 ms] | 267.40 ms [267.37 ms, 269.89 ms] | 2417.9 [2417.7, 2440.5] | 1.009 [0.996, 1.021] | same |
| explicit_hex8_ss | init | head-t2 | 3 | 1 | 265.68 ms [264.26 ms, 271.65 ms] | 265.68 ms [264.26 ms, 271.65 ms] | 2402.3 [2389.5, 2456.3] | 1.003 [1.000, 1.012] | same |
| explicit_hex8_ss | init | head-t4 | 3 | 1 | 267.13 ms [265.59 ms, 275.86 ms] | 267.13 ms [265.59 ms, 275.86 ms] | 2415.5 [2401.5, 2494.4] | 1.009 [1.005, 1.027] | same |
| explicit_hex8_ss | stable_time_step | base-t1 | 3 | 5 | 23.28 ms [23.26 ms, 24.16 ms] | 4.66 ms [4.65 ms, 4.83 ms] | 42.1 [42.1, 43.7] | – | same |
| explicit_hex8_ss | stable_time_step | base-t2 | 3 | 5 | 23.28 ms [23.24 ms, 23.36 ms] | 4.66 ms [4.65 ms, 4.67 ms] | 42.1 [42.0, 42.2] | 1.001 [0.962, 1.003] | same |
| explicit_hex8_ss | stable_time_step | base-t4 | 3 | 5 | 23.25 ms [23.23 ms, 23.30 ms] | 4.65 ms [4.65 ms, 4.66 ms] | 42.0 [42.0, 42.1] | 0.999 [0.962, 1.001] | same |
| explicit_hex8_ss | stable_time_step | head-t2 | 3 | 5 | 23.29 ms [23.24 ms, 25.23 ms] | 4.66 ms [4.65 ms, 5.05 ms] | 42.1 [42.0, 45.6] | 1.000 [0.962, 1.085] | same |
| explicit_hex8_ss | stable_time_step | head-t4 | 3 | 5 | 23.28 ms [23.21 ms, 25.45 ms] | 4.66 ms [4.64 ms, 5.09 ms] | 42.1 [42.0, 46.0] | 1.000 [0.961, 1.094] | same |
| explicit_hex8_ss | solve_step | base-t1 | 3 | 50 | 1.532 s [1.520 s, 1.534 s] | 30.64 ms [30.41 ms, 30.68 ms] | 277.1 [275.0, 277.4] | – | same |
| explicit_hex8_ss | solve_step | base-t2 | 3 | 50 | 920.50 ms [903.30 ms, 920.89 ms] | 18.41 ms [18.07 ms, 18.42 ms] | 166.5 [163.4, 166.5] | 0.600 [0.594, 0.601] | same |
| explicit_hex8_ss | solve_step | base-t4 | 3 | 50 | 659.03 ms [658.31 ms, 661.91 ms] | 13.18 ms [13.17 ms, 13.24 ms] | 119.2 [119.1, 119.7] | 0.430 [0.429, 0.435] | same |
| explicit_hex8_ss | solve_step | head-t2 | 3 | 50 | 878.52 ms [872.95 ms, 879.50 ms] | 17.57 ms [17.46 ms, 17.59 ms] | 158.9 [157.9, 159.1] | 0.573 [0.570, 0.578] | same |
| explicit_hex8_ss | solve_step | head-t4 | 3 | 50 | 599.18 ms [597.80 ms, 601.88 ms] | 11.98 ms [11.96 ms, 12.04 ms] | 108.4 [108.1, 108.8] | 0.391 [0.390, 0.396] | same |
| explicit_hex8_ss | predictor | base-t1 | 3 | 50 | 9.7 µs [7.3 µs, 10.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | predictor | base-t2 | 3 | 50 | 9.6 µs [7.5 µs, 11.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.022 [0.995, 1.104] | same |
| explicit_hex8_ss | predictor | base-t4 | 3 | 50 | 9.5 µs [8.7 µs, 9.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.986 [0.947, 1.180] | same |
| explicit_hex8_ss | predictor | head-t2 | 3 | 50 | 8.4 µs [7.5 µs, 8.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.813 [0.781, 1.176] | same |
| explicit_hex8_ss | predictor | head-t4 | 3 | 50 | 5.5 µs [4.0 µs, 6.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.619 [0.388, 0.745] | same |
| explicit_hex8_ss | residual | base-t1 | 3 | 50 | 1.435 s [1.424 s, 1.437 s] | 28.70 ms [28.48 ms, 28.73 ms] | 259.5 [257.5, 259.8] | – | same |
| explicit_hex8_ss | residual | base-t2 | 3 | 50 | 823.51 ms [807.35 ms, 824.24 ms] | 16.47 ms [16.15 ms, 16.48 ms] | 148.9 [146.0, 149.1] | 0.574 [0.567, 0.574] | same |
| explicit_hex8_ss | residual | base-t4 | 3 | 50 | 563.77 ms [563.38 ms, 565.97 ms] | 11.28 ms [11.27 ms, 11.32 ms] | 102.0 [101.9, 102.4] | 0.393 [0.392, 0.398] | same |
| explicit_hex8_ss | residual | head-t2 | 3 | 50 | 806.07 ms [803.85 ms, 807.73 ms] | 16.12 ms [16.08 ms, 16.15 ms] | 145.8 [145.4, 146.1] | 0.562 [0.560, 0.566] | same |
| explicit_hex8_ss | residual | head-t4 | 3 | 50 | 538.89 ms [537.21 ms, 541.37 ms] | 10.78 ms [10.74 ms, 10.83 ms] | 97.5 [97.2, 97.9] | 0.375 [0.374, 0.380] | same |
| explicit_hex8_ss | lumped_matrix | base-t1 | 3 | 50 | 16.6 µs [14.7 µs, 17.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | lumped_matrix | base-t2 | 3 | 50 | 19.6 µs [18.9 µs, 20.6 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.241 [1.095, 1.335] | same |
| explicit_hex8_ss | lumped_matrix | base-t4 | 3 | 50 | 19.4 µs [19.1 µs, 20.1 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.209 [1.106, 1.315] | same |
| explicit_hex8_ss | lumped_matrix | head-t2 | 3 | 50 | 14.6 µs [14.1 µs, 19.2 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.990 [0.818, 1.158] | same |
| explicit_hex8_ss | lumped_matrix | head-t4 | 3 | 50 | 19.3 µs [16.1 µs, 29.3 µs] | 0.4 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.162 [1.094, 1.699] | same |
| explicit_hex8_ss | corrector | base-t1 | 3 | 50 | 13.8 µs [13.4 µs, 14.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | corrector | base-t2 | 3 | 50 | 13.4 µs [13.2 µs, 14.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.997 [0.962, 1.004] | same |
| explicit_hex8_ss | corrector | base-t4 | 3 | 50 | 13.6 µs [13.0 µs, 13.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.969 [0.960, 0.991] | same |
| explicit_hex8_ss | corrector | head-t2 | 3 | 50 | 10.6 µs [9.3 µs, 12.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.745 [0.678, 0.920] | same |
| explicit_hex8_ss | corrector | head-t4 | 3 | 50 | 12.7 µs [12.5 µs, 13.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.934 [0.887, 0.961] | same |
| explicit_hex8_ss | step_hooks | base-t1 | 3 | 100 | 168.9 µs [167.4 µs, 180.6 µs] | 1.7 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_hex8_ss | step_hooks | base-t2 | 3 | 100 | 168.2 µs [162.4 µs, 177.9 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 0.996 [0.899, 1.063] | same |
| explicit_hex8_ss | step_hooks | base-t4 | 3 | 100 | 168.4 µs [164.9 µs, 171.2 µs] | 1.7 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.985 [0.932, 1.014] | same |
| explicit_hex8_ss | step_hooks | head-t2 | 3 | 100 | 183.9 µs [180.9 µs, 187.8 µs] | 1.8 µs [1.8 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.099 [1.002, 1.112] | same |
| explicit_hex8_ss | step_hooks | head-t4 | 3 | 100 | 177.1 µs [165.5 µs, 183.1 µs] | 1.8 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.049 [0.917, 1.094] | same |
| explicit_hex8_ss | step_rest | base-t1 | 3 | 50 | 97.11 ms [96.41 ms, 97.13 ms] | 1.94 ms [1.93 ms, 1.94 ms] | 17.6 [17.4, 17.6] | – | same |
| explicit_hex8_ss | step_rest | base-t2 | 3 | 50 | 96.42 ms [95.73 ms, 96.77 ms] | 1.93 ms [1.91 ms, 1.94 ms] | 17.4 [17.3, 17.5] | 0.993 [0.993, 0.997] | same |
| explicit_hex8_ss | step_rest | base-t4 | 3 | 50 | 95.04 ms [94.71 ms, 95.71 ms] | 1.90 ms [1.89 ms, 1.91 ms] | 17.2 [17.1, 17.3] | 0.979 [0.975, 0.993] | same |
| explicit_hex8_ss | step_rest | head-t2 | 3 | 50 | 71.55 ms [68.86 ms, 72.21 ms] | 1.43 ms [1.38 ms, 1.44 ms] | 12.9 [12.5, 13.1] | 0.737 [0.709, 0.749] | same |
| explicit_hex8_ss | step_rest | head-t4 | 3 | 50 | 60.28 ms [60.05 ms, 60.37 ms] | 1.21 ms [1.20 ms, 1.21 ms] | 10.9 [10.9, 10.9] | 0.622 [0.618, 0.625] | same |
| explicit_hex8_ss | internal_forces | base-t1 | 3 | 10 | 278.22 ms [277.66 ms, 281.02 ms] | 27.82 ms [27.77 ms, 28.10 ms] | 251.6 [251.1, 254.1] | – | same |
| explicit_hex8_ss | internal_forces | base-t2 | 3 | 10 | 152.48 ms [149.75 ms, 152.50 ms] | 15.25 ms [14.98 ms, 15.25 ms] | 137.9 [135.4, 137.9] | 0.543 [0.539, 0.548] | same |
| explicit_hex8_ss | internal_forces | base-t4 | 3 | 10 | 102.92 ms [102.55 ms, 105.66 ms] | 10.29 ms [10.26 ms, 10.57 ms] | 93.1 [92.7, 95.5] | 0.369 [0.366, 0.381] | same |
| explicit_hex8_ss | internal_forces | head-t2 | 3 | 10 | 151.49 ms [150.71 ms, 152.47 ms] | 15.15 ms [15.07 ms, 15.25 ms] | 137.0 [136.3, 137.9] | 0.546 [0.536, 0.548] | same |
| explicit_hex8_ss | internal_forces | head-t4 | 3 | 10 | 103.21 ms [102.80 ms, 103.79 ms] | 10.32 ms [10.28 ms, 10.38 ms] | 93.3 [93.0, 93.8] | 0.371 [0.366, 0.374] | same |
| explicit_hex8_ss | stresses | base-t1 | 3 | 10 | 175.74 ms [173.66 ms, 175.85 ms] | 17.57 ms [17.37 ms, 17.58 ms] | 158.9 [157.0, 159.0] | – | same |
| explicit_hex8_ss | stresses | base-t2 | 3 | 10 | 124.28 ms [124.10 ms, 124.78 ms] | 12.43 ms [12.41 ms, 12.48 ms] | 112.4 [112.2, 112.8] | 0.710 [0.707, 0.715] | same |
| explicit_hex8_ss | stresses | base-t4 | 3 | 10 | 106.10 ms [105.72 ms, 106.53 ms] | 10.61 ms [10.57 ms, 10.65 ms] | 95.9 [95.6, 96.3] | 0.606 [0.604, 0.609] | same |
| explicit_hex8_ss | stresses | head-t2 | 3 | 10 | 141.98 ms [123.72 ms, 145.24 ms] | 14.20 ms [12.37 ms, 14.52 ms] | 128.4 [111.9, 131.3] | 0.818 [0.704, 0.826] | same |
| explicit_hex8_ss | stresses | head-t4 | 3 | 10 | 104.11 ms [103.47 ms, 104.49 ms] | 10.41 ms [10.35 ms, 10.45 ms] | 94.1 [93.6, 94.5] | 0.592 [0.589, 0.602] | same |
| explicit_hex8_ss | energies | base-t1 | 3 | 10 | 48.32 ms [48.18 ms, 49.26 ms] | 4.83 ms [4.82 ms, 4.93 ms] | 43.7 [43.6, 44.5] | – | same |
| explicit_hex8_ss | energies | base-t2 | 3 | 10 | 49.43 ms [49.42 ms, 49.47 ms] | 4.94 ms [4.94 ms, 4.95 ms] | 44.7 [44.7, 44.7] | 1.023 [1.004, 1.026] | same |
| explicit_hex8_ss | energies | base-t4 | 3 | 10 | 48.75 ms [48.29 ms, 48.75 ms] | 4.87 ms [4.83 ms, 4.87 ms] | 44.1 [43.7, 44.1] | 1.009 [0.980, 1.012] | same |
| explicit_hex8_ss | energies | head-t2 | 3 | 10 | 48.53 ms [48.02 ms, 48.96 ms] | 4.85 ms [4.80 ms, 4.90 ms] | 43.9 [43.4, 44.3] | 0.994 [0.994, 1.007] | same |
| explicit_hex8_ss | energies | head-t4 | 3 | 10 | 48.30 ms [48.02 ms, 49.20 ms] | 4.83 ms [4.80 ms, 4.92 ms] | 43.7 [43.4, 44.5] | 0.999 [0.997, 0.999] | same |
| explicit_marigo | mesh | base-t1 | 3 | 1 | 2.17 ms [2.17 ms, 2.20 ms] | 2.17 ms [2.17 ms, 2.20 ms] | 20.6 [20.5, 20.9] | – | same |
| explicit_marigo | mesh | base-t2 | 3 | 1 | 2.05 ms [2.05 ms, 2.10 ms] | 2.05 ms [2.05 ms, 2.10 ms] | 19.5 [19.4, 19.9] | 0.947 [0.933, 0.965] | same |
| explicit_marigo | mesh | base-t4 | 3 | 1 | 2.09 ms [2.07 ms, 2.09 ms] | 2.09 ms [2.07 ms, 2.09 ms] | 19.8 [19.7, 19.8] | 0.963 [0.943, 0.966] | same |
| explicit_marigo | mesh | head-t2 | 3 | 1 | 2.02 ms [2.01 ms, 2.02 ms] | 2.02 ms [2.01 ms, 2.02 ms] | 19.1 [19.0, 19.2] | 0.924 [0.919, 0.931] | same |
| explicit_marigo | mesh | head-t4 | 3 | 1 | 2.07 ms [1.99 ms, 2.08 ms] | 2.07 ms [1.99 ms, 2.08 ms] | 19.7 [18.9, 19.8] | 0.948 [0.921, 0.954] | same |
| explicit_marigo | init | base-t1 | 3 | 1 | 33.31 ms [32.88 ms, 33.64 ms] | 33.31 ms [32.88 ms, 33.64 ms] | 315.9 [311.8, 319.0] | – | same |
| explicit_marigo | init | base-t2 | 3 | 1 | 32.50 ms [31.90 ms, 32.52 ms] | 32.50 ms [31.90 ms, 32.52 ms] | 308.2 [302.5, 308.3] | 0.967 [0.958, 0.988] | same |
| explicit_marigo | init | base-t4 | 3 | 1 | 33.50 ms [32.69 ms, 33.56 ms] | 33.50 ms [32.69 ms, 33.56 ms] | 317.6 [310.0, 318.2] | 0.996 [0.994, 1.007] | same |
| explicit_marigo | init | head-t2 | 3 | 1 | 33.19 ms [32.89 ms, 33.41 ms] | 33.19 ms [32.89 ms, 33.41 ms] | 314.8 [311.9, 316.8] | 0.996 [0.978, 1.016] | same |
| explicit_marigo | init | head-t4 | 3 | 1 | 32.64 ms [32.01 ms, 32.82 ms] | 32.64 ms [32.01 ms, 32.82 ms] | 309.5 [303.5, 311.2] | 0.985 [0.952, 0.993] | same |
| explicit_marigo | stable_time_step | base-t1 | 3 | 5 | 21.10 ms [21.10 ms, 21.15 ms] | 4.22 ms [4.22 ms, 4.23 ms] | 40.0 [40.0, 40.1] | – | same |
| explicit_marigo | stable_time_step | base-t2 | 3 | 5 | 21.16 ms [21.13 ms, 21.41 ms] | 4.23 ms [4.23 ms, 4.28 ms] | 40.1 [40.1, 40.6] | 1.003 [0.999, 1.015] | same |
| explicit_marigo | stable_time_step | base-t4 | 3 | 5 | 21.15 ms [21.13 ms, 21.21 ms] | 4.23 ms [4.23 ms, 4.24 ms] | 40.1 [40.1, 40.2] | 1.001 [1.000, 1.006] | same |
| explicit_marigo | stable_time_step | head-t2 | 3 | 5 | 21.20 ms [21.08 ms, 21.59 ms] | 4.24 ms [4.22 ms, 4.32 ms] | 40.2 [40.0, 40.9] | 1.005 [0.999, 1.021] | same |
| explicit_marigo | stable_time_step | head-t4 | 3 | 5 | 21.16 ms [21.09 ms, 21.39 ms] | 4.23 ms [4.22 ms, 4.28 ms] | 40.1 [40.0, 40.6] | 1.003 [0.999, 1.011] | same |
| explicit_marigo | solve_step | base-t1 | 3 | 200 | 1.519 s [1.519 s, 1.531 s] | 7.60 ms [7.60 ms, 7.66 ms] | 72.0 [72.0, 72.6] | – | same |
| explicit_marigo | solve_step | base-t2 | 3 | 200 | 963.33 ms [962.96 ms, 967.80 ms] | 4.82 ms [4.81 ms, 4.84 ms] | 45.7 [45.7, 45.9] | 0.634 [0.632, 0.634] | same |
| explicit_marigo | solve_step | base-t4 | 3 | 200 | 754.75 ms [752.66 ms, 756.98 ms] | 3.77 ms [3.76 ms, 3.78 ms] | 35.8 [35.7, 35.9] | 0.497 [0.491, 0.498] | same |
| explicit_marigo | solve_step | head-t2 | 3 | 200 | 882.12 ms [880.56 ms, 886.98 ms] | 4.41 ms [4.40 ms, 4.43 ms] | 41.8 [41.8, 42.1] | 0.581 [0.575, 0.584] | same |
| explicit_marigo | solve_step | head-t4 | 3 | 200 | 624.43 ms [623.52 ms, 630.69 ms] | 3.12 ms [3.12 ms, 3.15 ms] | 29.6 [29.6, 29.9] | 0.411 [0.407, 0.415] | same |
| explicit_marigo | predictor | base-t1 | 3 | 200 | 29.5 µs [27.5 µs, 32.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | predictor | base-t2 | 3 | 200 | 30.3 µs [29.0 µs, 35.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.102 [0.983, 1.108] | same |
| explicit_marigo | predictor | base-t4 | 3 | 200 | 29.8 µs [29.2 µs, 37.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.061 [0.930, 1.264] | same |
| explicit_marigo | predictor | head-t2 | 3 | 200 | 17.0 µs [16.4 µs, 24.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.555 [0.529, 0.881] | same |
| explicit_marigo | predictor | head-t4 | 3 | 200 | 24.4 µs [8.4 µs, 34.0 µs] | 0.1 µs [0.0 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.829 [0.307, 1.061] | same |
| explicit_marigo | residual | base-t1 | 3 | 200 | 1.128 s [1.127 s, 1.138 s] | 5.64 ms [5.64 ms, 5.69 ms] | 53.5 [53.5, 54.0] | – | same |
| explicit_marigo | residual | base-t2 | 3 | 200 | 649.67 ms [647.53 ms, 650.77 ms] | 3.25 ms [3.24 ms, 3.25 ms] | 30.8 [30.7, 30.9] | 0.574 [0.572, 0.576] | same |
| explicit_marigo | residual | base-t4 | 3 | 200 | 459.18 ms [457.27 ms, 461.82 ms] | 2.30 ms [2.29 ms, 2.31 ms] | 21.8 [21.7, 21.9] | 0.407 [0.402, 0.410] | same |
| explicit_marigo | residual | head-t2 | 3 | 200 | 634.09 ms [633.24 ms, 636.82 ms] | 3.17 ms [3.17 ms, 3.18 ms] | 30.1 [30.0, 30.2] | 0.562 [0.556, 0.565] | same |
| explicit_marigo | residual | head-t4 | 3 | 200 | 431.95 ms [429.54 ms, 435.20 ms] | 2.16 ms [2.15 ms, 2.18 ms] | 20.5 [20.4, 20.6] | 0.383 [0.377, 0.386] | same |
| explicit_marigo | lumped_matrix | base-t1 | 3 | 200 | 42.7 µs [39.1 µs, 46.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | lumped_matrix | base-t2 | 3 | 200 | 55.4 µs [52.4 µs, 56.8 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.296 [1.214, 1.339] | same |
| explicit_marigo | lumped_matrix | base-t4 | 3 | 200 | 56.2 µs [52.6 µs, 70.4 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.316 [1.123, 1.799] | same |
| explicit_marigo | lumped_matrix | head-t2 | 3 | 200 | 55.4 µs [53.6 µs, 56.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.326 [1.146, 1.416] | same |
| explicit_marigo | lumped_matrix | head-t4 | 3 | 200 | 52.8 µs [52.5 µs, 52.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.239 [1.122, 1.351] | same |
| explicit_marigo | corrector | base-t1 | 3 | 200 | 25.2 µs [25.1 µs, 26.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_marigo | corrector | base-t2 | 3 | 200 | 43.3 µs [43.3 µs, 49.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.728 [1.650, 1.971] | same |
| explicit_marigo | corrector | base-t4 | 3 | 200 | 37.4 µs [34.8 µs, 46.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.423 [1.382, 1.871] | same |
| explicit_marigo | corrector | head-t2 | 3 | 200 | 39.1 µs [38.6 µs, 51.7 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.550 [1.471, 2.062] | same |
| explicit_marigo | corrector | head-t4 | 3 | 200 | 30.4 µs [26.7 µs, 37.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.159 [1.060, 1.489] | same |
| explicit_marigo | step_hooks | base-t1 | 3 | 400 | 327.63 ms [327.27 ms, 328.80 ms] | 819.1 µs [818.2 µs, 822.0 µs] | 7.8 [7.8, 7.8] | – | same |
| explicit_marigo | step_hooks | base-t2 | 3 | 400 | 240.56 ms [239.94 ms, 241.32 ms] | 601.4 µs [599.8 µs, 603.3 µs] | 5.7 [5.7, 5.7] | 0.734 [0.733, 0.734] | same |
| explicit_marigo | step_hooks | base-t4 | 3 | 400 | 219.86 ms [218.43 ms, 220.18 ms] | 549.6 µs [546.1 µs, 550.5 µs] | 5.2 [5.2, 5.2] | 0.669 [0.667, 0.673] | same |
| explicit_marigo | step_hooks | head-t2 | 3 | 400 | 191.95 ms [191.35 ms, 193.69 ms] | 479.9 µs [478.4 µs, 484.2 µs] | 4.6 [4.5, 4.6] | 0.587 [0.582, 0.591] | same |
| explicit_marigo | step_hooks | head-t4 | 3 | 400 | 146.55 ms [144.94 ms, 147.31 ms] | 366.4 µs [362.4 µs, 368.3 µs] | 3.5 [3.4, 3.5] | 0.446 [0.443, 0.450] | same |
| explicit_marigo | step_rest | base-t1 | 3 | 200 | 63.43 ms [62.95 ms, 63.86 ms] | 317.2 µs [314.7 µs, 319.3 µs] | 3.0 [3.0, 3.0] | – | same |
| explicit_marigo | step_rest | base-t2 | 3 | 200 | 73.72 ms [72.60 ms, 74.56 ms] | 368.6 µs [363.0 µs, 372.8 µs] | 3.5 [3.4, 3.5] | 1.162 [1.153, 1.168] | same |
| explicit_marigo | step_rest | base-t4 | 3 | 200 | 74.41 ms [74.21 ms, 75.51 ms] | 372.1 µs [371.1 µs, 377.5 µs] | 3.5 [3.5, 3.6] | 1.179 [1.165, 1.190] | same |
| explicit_marigo | step_rest | head-t2 | 3 | 200 | 54.92 ms [54.86 ms, 55.38 ms] | 274.6 µs [274.3 µs, 276.9 µs] | 2.6 [2.6, 2.6] | 0.873 [0.859, 0.873] | same |
| explicit_marigo | step_rest | head-t4 | 3 | 200 | 46.45 ms [46.32 ms, 47.07 ms] | 232.2 µs [231.6 µs, 235.4 µs] | 2.2 [2.2, 2.2] | 0.738 [0.725, 0.742] | same |
| explicit_marigo | internal_forces | base-t1 | 3 | 40 | 223.09 ms [222.38 ms, 224.04 ms] | 5.58 ms [5.56 ms, 5.60 ms] | 52.9 [52.7, 53.1] | – | same |
| explicit_marigo | internal_forces | base-t2 | 3 | 40 | 120.02 ms [119.51 ms, 120.26 ms] | 3.00 ms [2.99 ms, 3.01 ms] | 28.5 [28.3, 28.5] | 0.539 [0.533, 0.540] | same |
| explicit_marigo | internal_forces | base-t4 | 3 | 40 | 67.92 ms [67.77 ms, 68.19 ms] | 1.70 ms [1.69 ms, 1.70 ms] | 16.1 [16.1, 16.2] | 0.305 [0.303, 0.306] | same |
| explicit_marigo | internal_forces | head-t2 | 3 | 40 | 118.14 ms [117.03 ms, 118.61 ms] | 2.95 ms [2.93 ms, 2.97 ms] | 28.0 [27.7, 28.1] | 0.529 [0.526, 0.530] | same |
| explicit_marigo | internal_forces | head-t4 | 3 | 40 | 65.24 ms [65.06 ms, 66.24 ms] | 1.63 ms [1.63 ms, 1.66 ms] | 15.5 [15.4, 15.7] | 0.292 [0.290, 0.298] | same |
| explicit_marigo | stresses | base-t1 | 3 | 40 | 141.34 ms [141.18 ms, 141.89 ms] | 3.53 ms [3.53 ms, 3.55 ms] | 33.5 [33.5, 33.6] | – | same |
| explicit_marigo | stresses | base-t2 | 3 | 40 | 123.61 ms [122.91 ms, 124.16 ms] | 3.09 ms [3.07 ms, 3.10 ms] | 29.3 [29.1, 29.4] | 0.871 [0.870, 0.880] | same |
| explicit_marigo | stresses | base-t4 | 3 | 40 | 114.46 ms [114.41 ms, 116.03 ms] | 2.86 ms [2.86 ms, 2.90 ms] | 27.1 [27.1, 27.5] | 0.810 [0.807, 0.821] | same |
| explicit_marigo | stresses | head-t2 | 3 | 40 | 123.14 ms [122.40 ms, 123.75 ms] | 3.08 ms [3.06 ms, 3.09 ms] | 29.2 [29.0, 29.3] | 0.868 [0.866, 0.877] | same |
| explicit_marigo | stresses | head-t4 | 3 | 40 | 113.59 ms [113.53 ms, 113.86 ms] | 2.84 ms [2.84 ms, 2.85 ms] | 26.9 [26.9, 27.0] | 0.804 [0.800, 0.807] | same |
| explicit_marigo | energies | base-t1 | 3 | 40 | 62.35 ms [61.07 ms, 62.56 ms] | 1.56 ms [1.53 ms, 1.56 ms] | 14.8 [14.5, 14.8] | – | same |
| explicit_marigo | energies | base-t2 | 3 | 40 | 61.92 ms [60.76 ms, 63.34 ms] | 1.55 ms [1.52 ms, 1.58 ms] | 14.7 [14.4, 15.0] | 0.990 [0.974, 1.037] | same |
| explicit_marigo | energies | base-t4 | 3 | 40 | 62.06 ms [60.36 ms, 62.38 ms] | 1.55 ms [1.51 ms, 1.56 ms] | 14.7 [14.3, 14.8] | 0.995 [0.965, 1.021] | same |
| explicit_marigo | energies | head-t2 | 3 | 40 | 52.81 ms [52.62 ms, 53.08 ms] | 1.32 ms [1.32 ms, 1.33 ms] | 12.5 [12.5, 12.6] | 0.844 [0.844, 0.869] | same |
| explicit_marigo | energies | head-t4 | 3 | 40 | 48.82 ms [48.23 ms, 49.61 ms] | 1.22 ms [1.21 ms, 1.24 ms] | 11.6 [11.4, 11.8] | 0.790 [0.780, 0.796] | same |
| explicit_maxwell | mesh | base-t1 | 3 | 1 | 2.19 ms [2.16 ms, 2.23 ms] | 2.19 ms [2.16 ms, 2.23 ms] | 20.8 [20.4, 21.1] | – | same |
| explicit_maxwell | mesh | base-t2 | 3 | 1 | 2.08 ms [2.02 ms, 2.18 ms] | 2.08 ms [2.02 ms, 2.18 ms] | 19.7 [19.2, 20.6] | 0.939 [0.934, 0.993] | same |
| explicit_maxwell | mesh | base-t4 | 3 | 1 | 2.11 ms [2.08 ms, 2.15 ms] | 2.11 ms [2.08 ms, 2.15 ms] | 20.0 [19.7, 20.4] | 0.966 [0.950, 0.978] | same |
| explicit_maxwell | mesh | head-t2 | 3 | 1 | 2.06 ms [2.04 ms, 2.12 ms] | 2.06 ms [2.04 ms, 2.12 ms] | 19.5 [19.4, 20.1] | 0.948 [0.940, 0.952] | same |
| explicit_maxwell | mesh | head-t4 | 3 | 1 | 2.20 ms [2.08 ms, 2.21 ms] | 2.20 ms [2.08 ms, 2.21 ms] | 20.9 [19.7, 20.9] | 1.008 [0.933, 1.021] | same |
| explicit_maxwell | init | base-t1 | 3 | 1 | 36.56 ms [36.45 ms, 37.50 ms] | 36.56 ms [36.45 ms, 37.50 ms] | 346.7 [345.6, 355.6] | – | same |
| explicit_maxwell | init | base-t2 | 3 | 1 | 36.08 ms [35.95 ms, 37.07 ms] | 36.08 ms [35.95 ms, 37.07 ms] | 342.1 [340.9, 351.5] | 0.988 [0.983, 0.990] | same |
| explicit_maxwell | init | base-t4 | 3 | 1 | 38.30 ms [36.58 ms, 38.55 ms] | 38.30 ms [36.58 ms, 38.55 ms] | 363.1 [346.9, 365.5] | 1.021 [1.000, 1.058] | same |
| explicit_maxwell | init | head-t2 | 3 | 1 | 37.07 ms [37.00 ms, 37.45 ms] | 37.07 ms [37.00 ms, 37.45 ms] | 351.5 [350.9, 355.1] | 1.015 [0.988, 1.024] | same |
| explicit_maxwell | init | head-t4 | 3 | 1 | 37.55 ms [37.15 ms, 37.96 ms] | 37.55 ms [37.15 ms, 37.96 ms] | 356.1 [352.3, 359.9] | 1.016 [1.012, 1.030] | same |
| explicit_maxwell | stable_time_step | base-t1 | 3 | 5 | 21.35 ms [21.11 ms, 21.59 ms] | 4.27 ms [4.22 ms, 4.32 ms] | 40.5 [40.0, 41.0] | – | same |
| explicit_maxwell | stable_time_step | base-t2 | 3 | 5 | 21.18 ms [21.13 ms, 21.67 ms] | 4.24 ms [4.23 ms, 4.33 ms] | 40.2 [40.1, 41.1] | 1.001 [0.992, 1.004] | same |
| explicit_maxwell | stable_time_step | base-t4 | 3 | 5 | 21.23 ms [21.17 ms, 21.26 ms] | 4.25 ms [4.23 ms, 4.25 ms] | 40.3 [40.2, 40.3] | 0.992 [0.984, 1.006] | same |
| explicit_maxwell | stable_time_step | head-t2 | 3 | 5 | 21.15 ms [21.08 ms, 21.67 ms] | 4.23 ms [4.22 ms, 4.33 ms] | 40.1 [40.0, 41.1] | 1.002 [0.987, 1.004] | same |
| explicit_maxwell | stable_time_step | head-t4 | 3 | 5 | 21.11 ms [21.08 ms, 21.17 ms] | 4.22 ms [4.22 ms, 4.23 ms] | 40.0 [40.0, 40.1] | 0.989 [0.976, 1.003] | same |
| explicit_maxwell | solve_step | base-t1 | 3 | 40 | 491.56 ms [491.51 ms, 493.45 ms] | 12.29 ms [12.29 ms, 12.34 ms] | 116.5 [116.5, 117.0] | – | same |
| explicit_maxwell | solve_step | base-t2 | 3 | 40 | 278.96 ms [278.51 ms, 285.32 ms] | 6.97 ms [6.96 ms, 7.13 ms] | 66.1 [66.0, 67.6] | 0.567 [0.564, 0.580] | same |
| explicit_maxwell | solve_step | base-t4 | 3 | 40 | 202.24 ms [201.22 ms, 202.67 ms] | 5.06 ms [5.03 ms, 5.07 ms] | 47.9 [47.7, 48.0] | 0.410 [0.409, 0.412] | same |
| explicit_maxwell | solve_step | head-t2 | 3 | 40 | 271.90 ms [271.85 ms, 272.16 ms] | 6.80 ms [6.80 ms, 6.80 ms] | 64.5 [64.4, 64.5] | 0.553 [0.551, 0.554] | same |
| explicit_maxwell | solve_step | head-t4 | 3 | 40 | 191.56 ms [190.96 ms, 192.70 ms] | 4.79 ms [4.77 ms, 4.82 ms] | 45.4 [45.3, 45.7] | 0.390 [0.387, 0.392] | same |
| explicit_maxwell | predictor | base-t1 | 3 | 40 | 5.5 µs [5.3 µs, 6.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | predictor | base-t2 | 3 | 40 | 7.7 µs [5.5 µs, 8.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.440 [0.843, 1.575] | same |
| explicit_maxwell | predictor | base-t4 | 3 | 40 | 6.7 µs [5.2 µs, 8.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.250 [0.799, 1.548] | same |
| explicit_maxwell | predictor | head-t2 | 3 | 40 | 3.9 µs [3.4 µs, 4.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.697 [0.516, 0.859] | same |
| explicit_maxwell | predictor | head-t4 | 3 | 40 | 3.0 µs [2.4 µs, 4.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.567 [0.439, 0.647] | same |
| explicit_maxwell | residual | base-t1 | 3 | 40 | 231.96 ms [231.39 ms, 232.01 ms] | 5.80 ms [5.78 ms, 5.80 ms] | 55.0 [54.9, 55.0] | – | same |
| explicit_maxwell | residual | base-t2 | 3 | 40 | 134.96 ms [134.88 ms, 141.84 ms] | 3.37 ms [3.37 ms, 3.55 ms] | 32.0 [32.0, 33.6] | 0.583 [0.582, 0.611] | same |
| explicit_maxwell | residual | base-t4 | 3 | 40 | 102.14 ms [101.46 ms, 102.32 ms] | 2.55 ms [2.54 ms, 2.56 ms] | 24.2 [24.1, 24.3] | 0.440 [0.438, 0.441] | same |
| explicit_maxwell | residual | head-t2 | 3 | 40 | 132.39 ms [132.37 ms, 132.85 ms] | 3.31 ms [3.31 ms, 3.32 ms] | 31.4 [31.4, 31.5] | 0.572 [0.571, 0.573] | same |
| explicit_maxwell | residual | head-t4 | 3 | 40 | 97.40 ms [97.02 ms, 97.67 ms] | 2.44 ms [2.43 ms, 2.44 ms] | 23.1 [23.0, 23.2] | 0.420 [0.418, 0.422] | same |
| explicit_maxwell | lumped_matrix | base-t1 | 3 | 40 | 10.3 µs [9.3 µs, 10.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | lumped_matrix | base-t2 | 3 | 40 | 11.9 µs [11.0 µs, 12.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.159 [1.157, 1.180] | same |
| explicit_maxwell | lumped_matrix | base-t4 | 3 | 40 | 12.5 µs [8.9 µs, 14.5 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.218 [0.808, 1.558] | same |
| explicit_maxwell | lumped_matrix | head-t2 | 3 | 40 | 11.7 µs [10.8 µs, 12.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.135 [0.989, 1.314] | same |
| explicit_maxwell | lumped_matrix | head-t4 | 3 | 40 | 8.6 µs [8.3 µs, 11.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.805 [0.790, 1.268] | same |
| explicit_maxwell | corrector | base-t1 | 3 | 40 | 4.0 µs [3.7 µs, 5.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_maxwell | corrector | base-t2 | 3 | 40 | 9.6 µs [6.6 µs, 9.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.921 [1.667, 2.571] | same |
| explicit_maxwell | corrector | base-t4 | 3 | 40 | 7.0 µs [6.3 µs, 7.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.868 [1.240, 1.936] | same |
| explicit_maxwell | corrector | head-t2 | 3 | 40 | 6.6 µs [6.2 µs, 6.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.674 [1.231, 1.806] | same |
| explicit_maxwell | corrector | head-t4 | 3 | 40 | 5.9 µs [5.4 µs, 6.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.354 [1.171, 1.809] | same |
| explicit_maxwell | step_hooks | base-t1 | 3 | 80 | 246.74 ms [246.06 ms, 248.05 ms] | 3.08 ms [3.08 ms, 3.10 ms] | 29.2 [29.2, 29.4] | – | same |
| explicit_maxwell | step_hooks | base-t2 | 3 | 80 | 128.18 ms [128.10 ms, 128.54 ms] | 1.60 ms [1.60 ms, 1.61 ms] | 15.2 [15.2, 15.2] | 0.521 [0.517, 0.521] | same |
| explicit_maxwell | step_hooks | base-t4 | 3 | 80 | 84.37 ms [84.28 ms, 84.59 ms] | 1.05 ms [1.05 ms, 1.06 ms] | 10.0 [10.0, 10.0] | 0.342 [0.340, 0.344] | same |
| explicit_maxwell | step_hooks | head-t2 | 3 | 80 | 127.85 ms [127.81 ms, 127.99 ms] | 1.60 ms [1.60 ms, 1.60 ms] | 15.2 [15.1, 15.2] | 0.518 [0.516, 0.519] | same |
| explicit_maxwell | step_hooks | head-t4 | 3 | 80 | 83.88 ms [83.83 ms, 85.18 ms] | 1.05 ms [1.05 ms, 1.06 ms] | 9.9 [9.9, 10.1] | 0.340 [0.338, 0.346] | same |
| explicit_maxwell | step_rest | base-t1 | 3 | 40 | 13.26 ms [13.25 ms, 13.26 ms] | 331.4 µs [331.2 µs, 331.6 µs] | 3.1 [3.1, 3.1] | – | same |
| explicit_maxwell | step_rest | base-t2 | 3 | 40 | 15.11 ms [15.10 ms, 15.25 ms] | 377.6 µs [377.6 µs, 381.3 µs] | 3.6 [3.6, 3.6] | 1.139 [1.139, 1.151] | same |
| explicit_maxwell | step_rest | base-t4 | 3 | 40 | 15.46 ms [15.22 ms, 15.49 ms] | 386.6 µs [380.6 µs, 387.4 µs] | 3.7 [3.6, 3.7] | 1.166 [1.149, 1.168] | same |
| explicit_maxwell | step_rest | head-t2 | 3 | 40 | 11.27 ms [11.22 ms, 11.34 ms] | 281.7 µs [280.5 µs, 283.6 µs] | 2.7 [2.7, 2.7] | 0.850 [0.846, 0.856] | same |
| explicit_maxwell | step_rest | head-t4 | 3 | 40 | 9.82 ms [9.82 ms, 9.89 ms] | 245.5 µs [245.4 µs, 247.2 µs] | 2.3 [2.3, 2.3] | 0.741 [0.741, 0.745] | same |
| explicit_maxwell | internal_forces | base-t1 | 3 | 8 | 43.48 ms [43.43 ms, 43.61 ms] | 5.43 ms [5.43 ms, 5.45 ms] | 51.5 [51.5, 51.7] | – | same |
| explicit_maxwell | internal_forces | base-t2 | 3 | 8 | 24.28 ms [23.72 ms, 24.90 ms] | 3.04 ms [2.96 ms, 3.11 ms] | 28.8 [28.1, 29.5] | 0.557 [0.546, 0.573] | same |
| explicit_maxwell | internal_forces | base-t4 | 3 | 8 | 17.09 ms [16.08 ms, 17.54 ms] | 2.14 ms [2.01 ms, 2.19 ms] | 20.3 [19.1, 20.8] | 0.393 [0.370, 0.402] | same |
| explicit_maxwell | internal_forces | head-t2 | 3 | 8 | 23.84 ms [23.46 ms, 24.42 ms] | 2.98 ms [2.93 ms, 3.05 ms] | 28.3 [27.8, 28.9] | 0.548 [0.540, 0.560] | same |
| explicit_maxwell | internal_forces | head-t4 | 3 | 8 | 16.79 ms [16.67 ms, 17.02 ms] | 2.10 ms [2.08 ms, 2.13 ms] | 19.9 [19.8, 20.2] | 0.386 [0.382, 0.392] | same |
| explicit_maxwell | stresses | base-t1 | 3 | 8 | 27.28 ms [27.19 ms, 27.37 ms] | 3.41 ms [3.40 ms, 3.42 ms] | 32.3 [32.2, 32.4] | – | same |
| explicit_maxwell | stresses | base-t2 | 3 | 8 | 23.75 ms [23.72 ms, 23.79 ms] | 2.97 ms [2.97 ms, 2.97 ms] | 28.1 [28.1, 28.2] | 0.870 [0.867, 0.875] | same |
| explicit_maxwell | stresses | base-t4 | 3 | 8 | 22.07 ms [21.95 ms, 22.09 ms] | 2.76 ms [2.74 ms, 2.76 ms] | 26.2 [26.0, 26.2] | 0.807 [0.806, 0.810] | same |
| explicit_maxwell | stresses | head-t2 | 3 | 8 | 23.81 ms [23.79 ms, 23.90 ms] | 2.98 ms [2.97 ms, 2.99 ms] | 28.2 [28.2, 28.3] | 0.873 [0.869, 0.879] | same |
| explicit_maxwell | stresses | head-t4 | 3 | 8 | 22.01 ms [22.01 ms, 22.16 ms] | 2.75 ms [2.75 ms, 2.77 ms] | 26.1 [26.1, 26.3] | 0.809 [0.807, 0.810] | same |
| explicit_maxwell | energies | base-t1 | 3 | 8 | 7.46 ms [7.44 ms, 7.50 ms] | 932.9 µs [930.4 µs, 937.1 µs] | 8.8 [8.8, 8.9] | – | same |
| explicit_maxwell | energies | base-t2 | 3 | 8 | 7.52 ms [7.47 ms, 7.53 ms] | 940.6 µs [933.9 µs, 941.1 µs] | 8.9 [8.9, 8.9] | 1.009 [0.997, 1.011] | same |
| explicit_maxwell | energies | base-t4 | 3 | 8 | 7.50 ms [7.49 ms, 7.54 ms] | 937.8 µs [936.6 µs, 942.6 µs] | 8.9 [8.9, 8.9] | 1.006 [1.004, 1.008] | same |
| explicit_maxwell | energies | head-t2 | 3 | 8 | 7.59 ms [7.58 ms, 7.59 ms] | 948.8 µs [947.3 µs, 949.1 µs] | 9.0 [9.0, 9.0] | 1.017 [1.011, 1.020] | same |
| explicit_maxwell | energies | head-t4 | 3 | 8 | 7.53 ms [7.51 ms, 7.54 ms] | 941.6 µs [939.1 µs, 942.4 µs] | 8.9 [8.9, 8.9] | 1.007 [1.006, 1.012] | same |
| explicit_mazars | mesh | base-t1 | 3 | 1 | 2.14 ms [2.09 ms, 2.17 ms] | 2.14 ms [2.09 ms, 2.17 ms] | 20.3 [19.9, 20.6] | – | same |
| explicit_mazars | mesh | base-t2 | 3 | 1 | 2.09 ms [2.06 ms, 2.15 ms] | 2.09 ms [2.06 ms, 2.15 ms] | 19.8 [19.6, 20.4] | 0.977 [0.951, 1.026] | same |
| explicit_mazars | mesh | base-t4 | 3 | 1 | 2.16 ms [2.13 ms, 2.19 ms] | 2.16 ms [2.13 ms, 2.19 ms] | 20.5 [20.2, 20.8] | 1.023 [0.983, 1.033] | same |
| explicit_mazars | mesh | head-t2 | 3 | 1 | 2.04 ms [2.00 ms, 2.12 ms] | 2.04 ms [2.00 ms, 2.12 ms] | 19.4 [19.0, 20.1] | 0.942 [0.937, 1.012] | same |
| explicit_mazars | mesh | head-t4 | 3 | 1 | 2.11 ms [2.10 ms, 2.16 ms] | 2.11 ms [2.10 ms, 2.16 ms] | 20.0 [19.9, 20.4] | 0.980 [0.971, 1.029] | same |
| explicit_mazars | init | base-t1 | 3 | 1 | 32.85 ms [32.57 ms, 33.10 ms] | 32.85 ms [32.57 ms, 33.10 ms] | 311.5 [308.8, 313.9] | – | same |
| explicit_mazars | init | base-t2 | 3 | 1 | 34.04 ms [32.65 ms, 34.68 ms] | 34.04 ms [32.65 ms, 34.68 ms] | 322.7 [309.6, 328.9] | 1.045 [0.994, 1.048] | same |
| explicit_mazars | init | base-t4 | 3 | 1 | 32.46 ms [32.28 ms, 33.00 ms] | 32.46 ms [32.28 ms, 33.00 ms] | 307.8 [306.1, 312.9] | 0.983 [0.981, 1.013] | same |
| explicit_mazars | init | head-t2 | 3 | 1 | 33.21 ms [32.71 ms, 33.41 ms] | 33.21 ms [32.71 ms, 33.41 ms] | 315.0 [310.2, 316.9] | 1.009 [0.996, 1.020] | same |
| explicit_mazars | init | head-t4 | 3 | 1 | 33.72 ms [32.62 ms, 33.91 ms] | 33.72 ms [32.62 ms, 33.91 ms] | 319.7 [309.3, 321.6] | 1.018 [0.993, 1.041] | same |
| explicit_mazars | stable_time_step | base-t1 | 3 | 5 | 21.19 ms [21.15 ms, 21.24 ms] | 4.24 ms [4.23 ms, 4.25 ms] | 40.2 [40.1, 40.3] | – | same |
| explicit_mazars | stable_time_step | base-t2 | 3 | 5 | 21.23 ms [21.09 ms, 21.54 ms] | 4.25 ms [4.22 ms, 4.31 ms] | 40.3 [40.0, 40.9] | 1.002 [0.993, 1.018] | same |
| explicit_mazars | stable_time_step | base-t4 | 3 | 5 | 21.26 ms [21.07 ms, 21.49 ms] | 4.25 ms [4.21 ms, 4.30 ms] | 40.3 [40.0, 40.8] | 1.005 [0.994, 1.012] | same |
| explicit_mazars | stable_time_step | head-t2 | 3 | 5 | 21.16 ms [21.15 ms, 21.66 ms] | 4.23 ms [4.23 ms, 4.33 ms] | 40.1 [40.1, 41.1] | 0.998 [0.997, 1.024] | same |
| explicit_mazars | stable_time_step | head-t4 | 3 | 5 | 21.10 ms [21.09 ms, 21.19 ms] | 4.22 ms [4.22 ms, 4.24 ms] | 40.0 [40.0, 40.2] | 0.997 [0.995, 0.998] | same |
| explicit_mazars | solve_step | base-t1 | 3 | 80 | 2.717 s [2.710 s, 2.718 s] | 33.97 ms [33.88 ms, 33.97 ms] | 322.1 [321.2, 322.2] | – | same |
| explicit_mazars | solve_step | base-t2 | 3 | 80 | 1.489 s [1.481 s, 1.492 s] | 18.61 ms [18.51 ms, 18.65 ms] | 176.5 [175.5, 176.8] | 0.548 [0.545, 0.550] | same |
| explicit_mazars | solve_step | base-t4 | 3 | 80 | 941.15 ms [938.98 ms, 943.48 ms] | 11.76 ms [11.74 ms, 11.79 ms] | 111.6 [111.3, 111.8] | 0.346 [0.346, 0.347] | same |
| explicit_mazars | solve_step | head-t2 | 3 | 80 | 1.455 s [1.445 s, 1.457 s] | 18.19 ms [18.06 ms, 18.21 ms] | 172.5 [171.2, 172.7] | 0.535 [0.532, 0.538] | same |
| explicit_mazars | solve_step | head-t4 | 3 | 80 | 882.84 ms [879.44 ms, 885.17 ms] | 11.04 ms [10.99 ms, 11.06 ms] | 104.6 [104.2, 104.9] | 0.325 [0.324, 0.327] | same |
| explicit_mazars | predictor | base-t1 | 3 | 80 | 12.0 µs [10.6 µs, 12.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | predictor | base-t2 | 3 | 80 | 11.6 µs [10.0 µs, 12.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.955 [0.832, 1.095] | same |
| explicit_mazars | predictor | base-t4 | 3 | 80 | 12.6 µs [10.9 µs, 12.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.045 [0.868, 1.214] | same |
| explicit_mazars | predictor | head-t2 | 3 | 80 | 6.6 µs [6.4 µs, 7.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.545 [0.508, 0.741] | same |
| explicit_mazars | predictor | head-t4 | 3 | 80 | 5.7 µs [5.0 µs, 8.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.452 [0.417, 0.793] | same |
| explicit_mazars | residual | base-t1 | 3 | 80 | 2.556 s [2.549 s, 2.556 s] | 31.95 ms [31.87 ms, 31.96 ms] | 302.9 [302.2, 303.0] | – | same |
| explicit_mazars | residual | base-t2 | 3 | 80 | 1.365 s [1.360 s, 1.369 s] | 17.07 ms [17.00 ms, 17.11 ms] | 161.8 [161.2, 162.3] | 0.534 [0.532, 0.537] | same |
| explicit_mazars | residual | base-t4 | 3 | 80 | 824.66 ms [823.62 ms, 825.31 ms] | 10.31 ms [10.30 ms, 10.32 ms] | 97.7 [97.6, 97.8] | 0.323 [0.323, 0.323] | same |
| explicit_mazars | residual | head-t2 | 3 | 80 | 1.357 s [1.349 s, 1.357 s] | 16.96 ms [16.86 ms, 16.97 ms] | 160.8 [159.9, 160.9] | 0.531 [0.528, 0.532] | same |
| explicit_mazars | residual | head-t4 | 3 | 80 | 800.22 ms [797.08 ms, 803.45 ms] | 10.00 ms [9.96 ms, 10.04 ms] | 94.9 [94.5, 95.2] | 0.313 [0.312, 0.315] | same |
| explicit_mazars | lumped_matrix | base-t1 | 3 | 80 | 18.6 µs [17.5 µs, 19.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | lumped_matrix | base-t2 | 3 | 80 | 21.0 µs [21.0 µs, 23.4 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.196 [1.127, 1.196] | same |
| explicit_mazars | lumped_matrix | base-t4 | 3 | 80 | 22.1 µs [21.8 µs, 25.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.168 [1.128, 1.459] | same |
| explicit_mazars | lumped_matrix | head-t2 | 3 | 80 | 21.7 µs [21.4 µs, 22.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.146 [1.110, 1.265] | same |
| explicit_mazars | lumped_matrix | head-t4 | 3 | 80 | 22.4 µs [20.1 µs, 25.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.149 [1.147, 1.391] | same |
| explicit_mazars | corrector | base-t1 | 3 | 80 | 12.6 µs [12.3 µs, 20.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars | corrector | base-t2 | 3 | 80 | 14.7 µs [14.5 µs, 18.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.176 [0.716, 1.454] | same |
| explicit_mazars | corrector | base-t4 | 3 | 80 | 20.4 µs [13.8 µs, 21.4 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.621 [0.674, 1.742] | same |
| explicit_mazars | corrector | head-t2 | 3 | 80 | 18.6 µs [17.9 µs, 21.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.456 [0.908, 1.706] | same |
| explicit_mazars | corrector | head-t4 | 3 | 80 | 16.5 µs [15.2 µs, 19.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.204 [0.808, 1.620] | same |
| explicit_mazars | step_hooks | base-t1 | 3 | 160 | 132.36 ms [132.00 ms, 132.61 ms] | 827.2 µs [825.0 µs, 828.8 µs] | 7.8 [7.8, 7.9] | – | same |
| explicit_mazars | step_hooks | base-t2 | 3 | 160 | 93.12 ms [92.42 ms, 94.30 ms] | 582.0 µs [577.6 µs, 589.4 µs] | 5.5 [5.5, 5.6] | 0.705 [0.698, 0.711] | same |
| explicit_mazars | step_hooks | base-t4 | 3 | 160 | 86.88 ms [86.30 ms, 89.97 ms] | 543.0 µs [539.4 µs, 562.3 µs] | 5.1 [5.1, 5.3] | 0.655 [0.654, 0.680] | same |
| explicit_mazars | step_hooks | head-t2 | 3 | 160 | 75.61 ms [72.90 ms, 76.80 ms] | 472.6 µs [455.6 µs, 480.0 µs] | 4.5 [4.3, 4.6] | 0.571 [0.550, 0.582] | same |
| explicit_mazars | step_hooks | head-t4 | 3 | 160 | 62.57 ms [62.04 ms, 62.65 ms] | 391.1 µs [387.8 µs, 391.6 µs] | 3.7 [3.7, 3.7] | 0.472 [0.470, 0.473] | same |
| explicit_mazars | step_rest | base-t1 | 3 | 80 | 28.59 ms [28.41 ms, 28.69 ms] | 357.3 µs [355.1 µs, 358.7 µs] | 3.4 [3.4, 3.4] | – | same |
| explicit_mazars | step_rest | base-t2 | 3 | 80 | 28.64 ms [28.42 ms, 29.04 ms] | 358.0 µs [355.3 µs, 363.0 µs] | 3.4 [3.4, 3.4] | 1.002 [0.990, 1.022] | same |
| explicit_mazars | step_rest | base-t4 | 3 | 80 | 28.55 ms [28.40 ms, 28.61 ms] | 356.9 µs [355.0 µs, 357.6 µs] | 3.4 [3.4, 3.4] | 0.999 [0.990, 1.007] | same |
| explicit_mazars | step_rest | head-t2 | 3 | 80 | 22.13 ms [22.06 ms, 22.47 ms] | 276.6 µs [275.8 µs, 280.9 µs] | 2.6 [2.6, 2.7] | 0.779 [0.769, 0.786] | same |
| explicit_mazars | step_rest | head-t4 | 3 | 80 | 19.25 ms [19.24 ms, 19.57 ms] | 240.6 µs [240.5 µs, 244.6 µs] | 2.3 [2.3, 2.3] | 0.677 [0.673, 0.682] | same |
| explicit_mazars | internal_forces | base-t1 | 3 | 16 | 549.04 ms [548.59 ms, 550.24 ms] | 34.31 ms [34.29 ms, 34.39 ms] | 325.4 [325.1, 326.1] | – | same |
| explicit_mazars | internal_forces | base-t2 | 3 | 16 | 289.16 ms [288.31 ms, 289.78 ms] | 18.07 ms [18.02 ms, 18.11 ms] | 171.4 [170.9, 171.7] | 0.526 [0.525, 0.528] | same |
| explicit_mazars | internal_forces | base-t4 | 3 | 16 | 170.59 ms [170.57 ms, 172.40 ms] | 10.66 ms [10.66 ms, 10.77 ms] | 101.1 [101.1, 102.2] | 0.311 [0.311, 0.313] | same |
| explicit_mazars | internal_forces | head-t2 | 3 | 16 | 287.68 ms [286.17 ms, 288.21 ms] | 17.98 ms [17.89 ms, 18.01 ms] | 170.5 [169.6, 170.8] | 0.523 [0.521, 0.525] | same |
| explicit_mazars | internal_forces | head-t4 | 3 | 16 | 167.89 ms [166.52 ms, 168.07 ms] | 10.49 ms [10.41 ms, 10.50 ms] | 99.5 [98.7, 99.6] | 0.305 [0.303, 0.306] | same |
| explicit_mazars | stresses | base-t1 | 3 | 16 | 516.33 ms [514.39 ms, 516.51 ms] | 32.27 ms [32.15 ms, 32.28 ms] | 306.0 [304.9, 306.1] | – | same |
| explicit_mazars | stresses | base-t2 | 3 | 16 | 505.95 ms [502.10 ms, 506.84 ms] | 31.62 ms [31.38 ms, 31.68 ms] | 299.9 [297.6, 300.4] | 0.980 [0.976, 0.981] | same |
| explicit_mazars | stresses | base-t4 | 3 | 16 | 506.99 ms [504.66 ms, 507.35 ms] | 31.69 ms [31.54 ms, 31.71 ms] | 300.5 [299.1, 300.7] | 0.982 [0.981, 0.983] | same |
| explicit_mazars | stresses | head-t2 | 3 | 16 | 497.29 ms [494.22 ms, 504.12 ms] | 31.08 ms [30.89 ms, 31.51 ms] | 294.7 [292.9, 298.8] | 0.967 [0.957, 0.976] | same |
| explicit_mazars | stresses | head-t4 | 3 | 16 | 501.20 ms [500.95 ms, 501.32 ms] | 31.33 ms [31.31 ms, 31.33 ms] | 297.0 [296.9, 297.1] | 0.971 [0.970, 0.974] | same |
| explicit_mazars | energies | base-t1 | 3 | 16 | 23.27 ms [23.24 ms, 23.70 ms] | 1.45 ms [1.45 ms, 1.48 ms] | 13.8 [13.8, 14.0] | – | same |
| explicit_mazars | energies | base-t2 | 3 | 16 | 23.18 ms [23.09 ms, 23.48 ms] | 1.45 ms [1.44 ms, 1.47 ms] | 13.7 [13.7, 13.9] | 0.992 [0.978, 1.010] | same |
| explicit_mazars | energies | base-t4 | 3 | 16 | 23.92 ms [23.15 ms, 23.93 ms] | 1.49 ms [1.45 ms, 1.50 ms] | 14.2 [13.7, 14.2] | 1.028 [0.977, 1.029] | same |
| explicit_mazars | energies | head-t2 | 3 | 16 | 19.80 ms [19.69 ms, 20.56 ms] | 1.24 ms [1.23 ms, 1.28 ms] | 11.7 [11.7, 12.2] | 0.851 [0.847, 0.867] | same |
| explicit_mazars | energies | head-t4 | 3 | 16 | 19.00 ms [18.76 ms, 19.03 ms] | 1.19 ms [1.17 ms, 1.19 ms] | 11.3 [11.1, 11.3] | 0.806 [0.802, 0.819] | same |
| explicit_mazars_non_local | mesh | base-t1 | 3 | 1 | 1.03 ms [996.1 µs, 1.03 ms] | 1.03 ms [996.1 µs, 1.03 ms] | 21.4 [20.8, 21.5] | – | same |
| explicit_mazars_non_local | mesh | base-t2 | 3 | 1 | 1.02 ms [988.9 µs, 1.02 ms] | 1.02 ms [988.9 µs, 1.02 ms] | 21.3 [20.6, 21.3] | 0.993 [0.987, 0.996] | same |
| explicit_mazars_non_local | mesh | base-t4 | 3 | 1 | 1.02 ms [993.7 µs, 1.03 ms] | 1.02 ms [993.7 µs, 1.03 ms] | 21.3 [20.7, 21.4] | 0.994 [0.967, 1.026] | same |
| explicit_mazars_non_local | mesh | head-t2 | 3 | 1 | 1.02 ms [984.0 µs, 1.02 ms] | 1.02 ms [984.0 µs, 1.02 ms] | 21.2 [20.5, 21.3] | 0.989 [0.951, 1.027] | same |
| explicit_mazars_non_local | mesh | head-t4 | 3 | 1 | 1.02 ms [980.1 µs, 1.02 ms] | 1.02 ms [980.1 µs, 1.02 ms] | 21.3 [20.4, 21.3] | 0.995 [0.948, 1.024] | same |
| explicit_mazars_non_local | init | base-t1 | 3 | 1 | 608.41 ms [607.27 ms, 610.07 ms] | 608.41 ms [607.27 ms, 610.07 ms] | 12675.2 [12651.4, 12709.8] | – | same |
| explicit_mazars_non_local | init | base-t2 | 3 | 1 | 612.08 ms [605.04 ms, 612.34 ms] | 612.08 ms [605.04 ms, 612.34 ms] | 12751.8 [12605.0, 12757.2] | 1.004 [0.994, 1.008] | same |
| explicit_mazars_non_local | init | base-t4 | 3 | 1 | 606.32 ms [604.40 ms, 608.41 ms] | 606.32 ms [604.40 ms, 608.41 ms] | 12631.7 [12591.6, 12675.2] | 0.997 [0.995, 0.997] | same |
| explicit_mazars_non_local | init | head-t2 | 3 | 1 | 606.97 ms [605.65 ms, 608.09 ms] | 606.97 ms [605.65 ms, 608.09 ms] | 12645.1 [12617.7, 12668.6] | 0.999 [0.993, 1.000] | same |
| explicit_mazars_non_local | init | head-t4 | 3 | 1 | 611.11 ms [610.44 ms, 611.96 ms] | 611.11 ms [610.44 ms, 611.96 ms] | 12731.4 [12717.4, 12749.1] | 1.003 [1.003, 1.006] | same |
| explicit_mazars_non_local | stable_time_step | base-t1 | 3 | 5 | 9.64 ms [9.61 ms, 9.74 ms] | 1.93 ms [1.92 ms, 1.95 ms] | 40.1 [40.0, 40.6] | – | same |
| explicit_mazars_non_local | stable_time_step | base-t2 | 3 | 5 | 9.64 ms [9.63 ms, 9.71 ms] | 1.93 ms [1.93 ms, 1.94 ms] | 40.2 [40.1, 40.5] | 0.999 [0.998, 1.003] | same |
| explicit_mazars_non_local | stable_time_step | base-t4 | 3 | 5 | 9.67 ms [9.64 ms, 9.77 ms] | 1.93 ms [1.93 ms, 1.95 ms] | 40.3 [40.2, 40.7] | 1.003 [1.003, 1.004] | same |
| explicit_mazars_non_local | stable_time_step | head-t2 | 3 | 5 | 9.70 ms [9.63 ms, 9.78 ms] | 1.94 ms [1.93 ms, 1.96 ms] | 40.4 [40.1, 40.8] | 1.005 [1.002, 1.007] | same |
| explicit_mazars_non_local | stable_time_step | head-t4 | 3 | 5 | 9.63 ms [9.60 ms, 9.66 ms] | 1.93 ms [1.92 ms, 1.93 ms] | 40.1 [40.0, 40.2] | 0.999 [0.989, 1.002] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 3 | 40 | 1.672 s [1.668 s, 1.672 s] | 41.80 ms [41.70 ms, 41.80 ms] | 870.8 [868.8, 870.9] | – | same |
| explicit_mazars_non_local | solve_step | base-t2 | 3 | 40 | 1.479 s [1.474 s, 1.484 s] | 36.98 ms [36.85 ms, 37.10 ms] | 770.4 [767.7, 773.0] | 0.887 [0.882, 0.888] | same |
| explicit_mazars_non_local | solve_step | base-t4 | 3 | 40 | 1.403 s [1.392 s, 1.405 s] | 35.07 ms [34.79 ms, 35.13 ms] | 730.7 [724.8, 732.0] | 0.839 [0.832, 0.843] | same |
| explicit_mazars_non_local | solve_step | head-t2 | 3 | 40 | 1.204 s [1.201 s, 1.205 s] | 30.10 ms [30.03 ms, 30.12 ms] | 627.1 [625.6, 627.5] | 0.720 [0.720, 0.720] | same |
| explicit_mazars_non_local | solve_step | head-t4 | 3 | 40 | 1.060 s [1.055 s, 1.063 s] | 26.49 ms [26.38 ms, 26.58 ms] | 551.9 [549.7, 553.7] | 0.635 [0.631, 0.636] | same |
| explicit_mazars_non_local | predictor | base-t1 | 3 | 40 | 4.5 µs [3.7 µs, 5.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | base-t2 | 3 | 40 | 5.0 µs [3.3 µs, 6.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.361 [0.648, 1.364] | same |
| explicit_mazars_non_local | predictor | base-t4 | 3 | 40 | 4.8 µs [4.0 µs, 5.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.062 [0.790, 1.596] | same |
| explicit_mazars_non_local | predictor | head-t2 | 3 | 40 | 3.5 µs [2.9 µs, 3.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.786 [0.573, 0.963] | same |
| explicit_mazars_non_local | predictor | head-t4 | 3 | 40 | 2.2 µs [1.5 µs, 2.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.487 [0.293, 0.755] | same |
| explicit_mazars_non_local | residual | base-t1 | 3 | 40 | 1.635 s [1.631 s, 1.635 s] | 40.87 ms [40.77 ms, 40.87 ms] | 851.4 [849.4, 851.4] | – | same |
| explicit_mazars_non_local | residual | base-t2 | 3 | 40 | 1.451 s [1.446 s, 1.456 s] | 36.27 ms [36.14 ms, 36.40 ms] | 755.7 [753.0, 758.3] | 0.890 [0.884, 0.891] | same |
| explicit_mazars_non_local | residual | base-t4 | 3 | 40 | 1.376 s [1.365 s, 1.380 s] | 34.41 ms [34.13 ms, 34.49 ms] | 716.8 [711.0, 718.6] | 0.842 [0.835, 0.846] | same |
| explicit_mazars_non_local | residual | head-t2 | 3 | 40 | 1.181 s [1.178 s, 1.182 s] | 29.53 ms [29.46 ms, 29.55 ms] | 615.2 [613.7, 615.7] | 0.723 [0.723, 0.723] | same |
| explicit_mazars_non_local | residual | head-t4 | 3 | 40 | 1.043 s [1.038 s, 1.046 s] | 26.06 ms [25.96 ms, 26.14 ms] | 543.0 [540.9, 544.6] | 0.639 [0.635, 0.640] | same |
| explicit_mazars_non_local | lumped_matrix | base-t1 | 3 | 40 | 9.1 µs [8.0 µs, 14.5 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | base-t2 | 3 | 40 | 8.8 µs [8.1 µs, 9.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.015 [0.605, 1.051] | same |
| explicit_mazars_non_local | lumped_matrix | base-t4 | 3 | 40 | 9.6 µs [7.3 µs, 10.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.045 [0.504, 1.246] | same |
| explicit_mazars_non_local | lumped_matrix | head-t2 | 3 | 40 | 10.3 µs [10.0 µs, 11.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.125 [0.819, 1.247] | same |
| explicit_mazars_non_local | lumped_matrix | head-t4 | 3 | 40 | 11.5 µs [9.7 µs, 21.7 µs] | 0.3 µs [0.2 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.060 [0.789, 2.708] | same |
| explicit_mazars_non_local | corrector | base-t1 | 3 | 40 | 6.6 µs [5.5 µs, 7.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | base-t2 | 3 | 40 | 6.1 µs [5.9 µs, 6.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.029 [0.826, 1.117] | same |
| explicit_mazars_non_local | corrector | base-t4 | 3 | 40 | 6.6 µs [6.0 µs, 7.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.935 [0.913, 1.365] | same |
| explicit_mazars_non_local | corrector | head-t2 | 3 | 40 | 6.5 µs [6.0 µs, 7.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.910 [0.905, 1.311] | same |
| explicit_mazars_non_local | corrector | head-t4 | 3 | 40 | 6.4 µs [5.9 µs, 7.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.965 [0.838, 1.364] | same |
| explicit_mazars_non_local | step_hooks | base-t1 | 3 | 80 | 29.60 ms [29.56 ms, 29.82 ms] | 370.0 µs [369.5 µs, 372.7 µs] | 7.7 [7.7, 7.8] | – | same |
| explicit_mazars_non_local | step_hooks | base-t2 | 3 | 80 | 20.48 ms [20.47 ms, 20.52 ms] | 255.9 µs [255.9 µs, 256.5 µs] | 5.3 [5.3, 5.3] | 0.693 [0.687, 0.693] | same |
| explicit_mazars_non_local | step_hooks | base-t4 | 3 | 80 | 18.62 ms [17.92 ms, 18.84 ms] | 232.8 µs [224.0 µs, 235.6 µs] | 4.8 [4.7, 4.9] | 0.625 [0.606, 0.637] | same |
| explicit_mazars_non_local | step_hooks | head-t2 | 3 | 80 | 16.45 ms [16.39 ms, 16.47 ms] | 205.6 µs [204.9 µs, 205.8 µs] | 4.3 [4.3, 4.3] | 0.556 [0.550, 0.556] | same |
| explicit_mazars_non_local | step_hooks | head-t4 | 3 | 80 | 11.87 ms [11.82 ms, 12.36 ms] | 148.4 µs [147.8 µs, 154.6 µs] | 3.1 [3.1, 3.2] | 0.401 [0.400, 0.415] | same |
| explicit_mazars_non_local | step_rest | base-t1 | 3 | 40 | 7.44 ms [7.44 ms, 7.51 ms] | 186.0 µs [186.0 µs, 187.6 µs] | 3.9 [3.9, 3.9] | – | same |
| explicit_mazars_non_local | step_rest | base-t2 | 3 | 40 | 7.61 ms [7.55 ms, 7.71 ms] | 190.3 µs [188.7 µs, 192.7 µs] | 4.0 [3.9, 4.0] | 1.015 [1.014, 1.036] | same |
| explicit_mazars_non_local | step_rest | base-t4 | 3 | 40 | 7.60 ms [7.55 ms, 7.79 ms] | 190.1 µs [188.7 µs, 194.8 µs] | 4.0 [3.9, 4.1] | 1.022 [1.006, 1.047] | same |
| explicit_mazars_non_local | step_rest | head-t2 | 3 | 40 | 6.14 ms [6.06 ms, 6.15 ms] | 153.4 µs [151.4 µs, 153.8 µs] | 3.2 [3.2, 3.2] | 0.818 [0.814, 0.827] | same |
| explicit_mazars_non_local | step_rest | head-t4 | 3 | 40 | 5.06 ms [5.01 ms, 5.09 ms] | 126.4 µs [125.3 µs, 127.3 µs] | 2.6 [2.6, 2.7] | 0.679 [0.668, 0.684] | same |
| explicit_mazars_non_local | internal_forces | base-t1 | 3 | 8 | 336.26 ms [336.25 ms, 336.43 ms] | 42.03 ms [42.03 ms, 42.05 ms] | 875.7 [875.7, 876.1] | – | same |
| explicit_mazars_non_local | internal_forces | base-t2 | 3 | 8 | 296.75 ms [295.33 ms, 298.54 ms] | 37.09 ms [36.92 ms, 37.32 ms] | 772.8 [769.1, 777.5] | 0.883 [0.878, 0.888] | same |
| explicit_mazars_non_local | internal_forces | base-t4 | 3 | 8 | 281.60 ms [278.92 ms, 282.13 ms] | 35.20 ms [34.87 ms, 35.27 ms] | 733.3 [726.4, 734.7] | 0.837 [0.829, 0.839] | same |
| explicit_mazars_non_local | internal_forces | head-t2 | 3 | 8 | 239.66 ms [239.61 ms, 240.74 ms] | 29.96 ms [29.95 ms, 30.09 ms] | 624.1 [624.0, 626.9] | 0.713 [0.712, 0.716] | same |
| explicit_mazars_non_local | internal_forces | head-t4 | 3 | 8 | 209.60 ms [206.02 ms, 209.61 ms] | 26.20 ms [25.75 ms, 26.20 ms] | 545.8 [536.5, 545.9] | 0.623 [0.612, 0.623] | same |
| explicit_mazars_non_local | stresses | base-t1 | 3 | 8 | 75.19 ms [75.08 ms, 75.21 ms] | 9.40 ms [9.38 ms, 9.40 ms] | 195.8 [195.5, 195.9] | – | same |
| explicit_mazars_non_local | stresses | base-t2 | 3 | 8 | 73.69 ms [73.61 ms, 74.07 ms] | 9.21 ms [9.20 ms, 9.26 ms] | 191.9 [191.7, 192.9] | 0.980 [0.979, 0.987] | same |
| explicit_mazars_non_local | stresses | base-t4 | 3 | 8 | 73.23 ms [72.88 ms, 73.25 ms] | 9.15 ms [9.11 ms, 9.16 ms] | 190.7 [189.8, 190.7] | 0.974 [0.971, 0.974] | same |
| explicit_mazars_non_local | stresses | head-t2 | 3 | 8 | 73.67 ms [73.63 ms, 73.73 ms] | 9.21 ms [9.20 ms, 9.22 ms] | 191.9 [191.7, 192.0] | 0.980 [0.980, 0.981] | same |
| explicit_mazars_non_local | stresses | head-t4 | 3 | 8 | 73.37 ms [73.11 ms, 73.50 ms] | 9.17 ms [9.14 ms, 9.19 ms] | 191.1 [190.4, 191.4] | 0.977 [0.972, 0.977] | same |
| explicit_mazars_non_local | energies | base-t1 | 3 | 8 | 5.52 ms [5.51 ms, 5.66 ms] | 690.3 µs [688.9 µs, 707.1 µs] | 14.4 [14.4, 14.7] | – | same |
| explicit_mazars_non_local | energies | base-t2 | 3 | 8 | 5.55 ms [5.48 ms, 5.56 ms] | 693.8 µs [685.5 µs, 694.7 µs] | 14.5 [14.3, 14.5] | 0.993 [0.982, 1.007] | same |
| explicit_mazars_non_local | energies | base-t4 | 3 | 8 | 5.51 ms [5.50 ms, 5.51 ms] | 689.0 µs [687.3 µs, 689.3 µs] | 14.4 [14.3, 14.4] | 0.996 [0.974, 1.000] | same |
| explicit_mazars_non_local | energies | head-t2 | 3 | 8 | 4.73 ms [4.73 ms, 4.77 ms] | 591.7 µs [591.3 µs, 596.4 µs] | 12.3 [12.3, 12.4] | 0.857 [0.837, 0.866] | same |
| explicit_mazars_non_local | energies | head-t4 | 3 | 8 | 4.50 ms [4.50 ms, 4.53 ms] | 562.2 µs [561.9 µs, 566.1 µs] | 11.7 [11.7, 11.8] | 0.814 [0.801, 0.816] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t1 | 3 | 2 | 1.177 s [1.175 s, 1.191 s] | 588.45 ms [587.56 ms, 595.27 ms] | 12259.3 [12240.8, 12401.5] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t2 | 3 | 2 | 1.176 s [1.174 s, 1.185 s] | 588.04 ms [586.80 ms, 592.62 ms] | 12250.8 [12225.0, 12346.2] | 0.997 [0.996, 1.001] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t4 | 3 | 2 | 1.180 s [1.177 s, 1.185 s] | 589.91 ms [588.62 ms, 592.42 ms] | 12289.8 [12262.9, 12342.0] | 1.004 [0.989, 1.007] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t2 | 3 | 2 | 1.178 s [1.177 s, 1.185 s] | 588.78 ms [588.38 ms, 592.44 ms] | 12266.2 [12257.9, 12342.4] | 1.000 [0.995, 1.002] | same |
| explicit_mazars_non_local | nonlocal_rebuild | head-t4 | 3 | 2 | 1.180 s [1.179 s, 1.186 s] | 590.22 ms [589.53 ms, 593.06 ms] | 12296.2 [12281.9, 12355.4] | 1.003 [0.990, 1.009] | same |
| explicit_multi_material | mesh | base-t1 | 3 | 1 | 2.20 ms [2.17 ms, 2.29 ms] | 2.20 ms [2.17 ms, 2.29 ms] | 20.9 [20.6, 21.8] | – | same |
| explicit_multi_material | mesh | base-t2 | 3 | 1 | 2.15 ms [2.12 ms, 2.17 ms] | 2.15 ms [2.12 ms, 2.17 ms] | 20.4 [20.1, 20.5] | 0.964 [0.936, 0.996] | same |
| explicit_multi_material | mesh | base-t4 | 3 | 1 | 2.19 ms [2.09 ms, 2.41 ms] | 2.19 ms [2.09 ms, 2.41 ms] | 20.8 [19.8, 22.8] | 1.009 [0.911, 1.094] | same |
| explicit_multi_material | mesh | head-t2 | 3 | 1 | 2.03 ms [2.03 ms, 2.05 ms] | 2.03 ms [2.03 ms, 2.05 ms] | 19.3 [19.2, 19.5] | 0.924 [0.882, 0.943] | same |
| explicit_multi_material | mesh | head-t4 | 3 | 1 | 2.18 ms [2.08 ms, 2.18 ms] | 2.18 ms [2.08 ms, 2.18 ms] | 20.7 [19.8, 20.7] | 0.958 [0.950, 0.993] | same |
| explicit_multi_material | init | base-t1 | 3 | 1 | 34.30 ms [34.17 ms, 35.11 ms] | 34.30 ms [34.17 ms, 35.11 ms] | 325.3 [324.1, 333.0] | – | same |
| explicit_multi_material | init | base-t2 | 3 | 1 | 34.14 ms [33.78 ms, 34.39 ms] | 34.14 ms [33.78 ms, 34.39 ms] | 323.8 [320.3, 326.1] | 0.988 [0.979, 0.995] | same |
| explicit_multi_material | init | base-t4 | 3 | 1 | 33.83 ms [33.79 ms, 33.89 ms] | 33.83 ms [33.79 ms, 33.89 ms] | 320.8 [320.4, 321.4] | 0.985 [0.965, 0.990] | same |
| explicit_multi_material | init | head-t2 | 3 | 1 | 34.34 ms [33.73 ms, 34.88 ms] | 34.34 ms [33.73 ms, 34.88 ms] | 325.6 [319.9, 330.7] | 1.005 [0.961, 1.017] | same |
| explicit_multi_material | init | head-t4 | 3 | 1 | 35.11 ms [33.93 ms, 35.87 ms] | 35.11 ms [33.93 ms, 35.87 ms] | 333.0 [321.7, 340.1] | 1.000 [0.993, 1.046] | same |
| explicit_multi_material | stable_time_step | base-t1 | 3 | 5 | 21.37 ms [21.27 ms, 21.44 ms] | 4.27 ms [4.25 ms, 4.29 ms] | 40.5 [40.3, 40.7] | – | same |
| explicit_multi_material | stable_time_step | base-t2 | 3 | 5 | 21.35 ms [21.27 ms, 21.38 ms] | 4.27 ms [4.25 ms, 4.28 ms] | 40.5 [40.3, 40.5] | 1.000 [0.996, 1.000] | same |
| explicit_multi_material | stable_time_step | base-t4 | 3 | 5 | 21.74 ms [21.33 ms, 21.83 ms] | 4.35 ms [4.27 ms, 4.37 ms] | 41.2 [40.5, 41.4] | 1.021 [0.995, 1.022] | same |
| explicit_multi_material | stable_time_step | head-t2 | 3 | 5 | 21.26 ms [21.25 ms, 21.57 ms] | 4.25 ms [4.25 ms, 4.31 ms] | 40.3 [40.3, 40.9] | 0.999 [0.992, 1.009] | same |
| explicit_multi_material | stable_time_step | head-t4 | 3 | 5 | 21.20 ms [21.15 ms, 21.44 ms] | 4.24 ms [4.23 ms, 4.29 ms] | 40.2 [40.1, 40.7] | 0.994 [0.989, 1.003] | same |
| explicit_multi_material | solve_step | base-t1 | 3 | 120 | 2.012 s [2.011 s, 2.015 s] | 16.77 ms [16.76 ms, 16.79 ms] | 159.0 [158.9, 159.2] | – | same |
| explicit_multi_material | solve_step | base-t2 | 3 | 120 | 1.127 s [1.120 s, 1.128 s] | 9.39 ms [9.33 ms, 9.40 ms] | 89.1 [88.5, 89.1] | 0.560 [0.557, 0.560] | same |
| explicit_multi_material | solve_step | base-t4 | 3 | 120 | 713.32 ms [707.12 ms, 727.51 ms] | 5.94 ms [5.89 ms, 6.06 ms] | 56.4 [55.9, 57.5] | 0.355 [0.352, 0.361] | same |
| explicit_multi_material | solve_step | head-t2 | 3 | 120 | 1.089 s [1.084 s, 1.112 s] | 9.07 ms [9.04 ms, 9.27 ms] | 86.1 [85.7, 87.9] | 0.540 [0.539, 0.553] | same |
| explicit_multi_material | solve_step | head-t4 | 3 | 120 | 660.68 ms [659.58 ms, 662.92 ms] | 5.51 ms [5.50 ms, 5.52 ms] | 52.2 [52.1, 52.4] | 0.328 [0.328, 0.330] | same |
| explicit_multi_material | predictor | base-t1 | 3 | 120 | 20.2 µs [16.3 µs, 26.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | predictor | base-t2 | 3 | 120 | 21.7 µs [19.8 µs, 26.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.076 [1.017, 1.209] | same |
| explicit_multi_material | predictor | base-t4 | 3 | 120 | 20.2 µs [19.2 µs, 23.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.171 [0.775, 1.174] | same |
| explicit_multi_material | predictor | head-t2 | 3 | 120 | 9.6 µs [8.8 µs, 11.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.438 [0.437, 0.586] | same |
| explicit_multi_material | predictor | head-t4 | 3 | 120 | 7.4 µs [6.8 µs, 7.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.336 [0.284, 0.469] | same |
| explicit_multi_material | residual | base-t1 | 3 | 120 | 1.878 s [1.877 s, 1.880 s] | 15.65 ms [15.64 ms, 15.67 ms] | 148.4 [148.3, 148.6] | – | same |
| explicit_multi_material | residual | base-t2 | 3 | 120 | 1.014 s [1.009 s, 1.015 s] | 8.45 ms [8.41 ms, 8.46 ms] | 80.2 [79.7, 80.2] | 0.540 [0.537, 0.540] | same |
| explicit_multi_material | residual | base-t4 | 3 | 120 | 609.91 ms [605.76 ms, 621.54 ms] | 5.08 ms [5.05 ms, 5.18 ms] | 48.2 [47.9, 49.1] | 0.325 [0.323, 0.331] | same |
| explicit_multi_material | residual | head-t2 | 3 | 120 | 994.29 ms [989.53 ms, 1.017 s] | 8.29 ms [8.25 ms, 8.47 ms] | 78.6 [78.2, 80.3] | 0.529 [0.527, 0.541] | same |
| explicit_multi_material | residual | head-t4 | 3 | 120 | 586.82 ms [586.21 ms, 588.52 ms] | 4.89 ms [4.89 ms, 4.90 ms] | 46.4 [46.3, 46.5] | 0.312 [0.312, 0.314] | same |
| explicit_multi_material | lumped_matrix | base-t1 | 3 | 120 | 32.3 µs [32.0 µs, 33.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | lumped_matrix | base-t2 | 3 | 120 | 36.3 µs [35.2 µs, 38.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.132 [1.089, 1.140] | same |
| explicit_multi_material | lumped_matrix | base-t4 | 3 | 120 | 36.8 µs [31.2 µs, 39.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.150 [0.964, 1.178] | same |
| explicit_multi_material | lumped_matrix | head-t2 | 3 | 120 | 36.0 µs [36.0 µs, 36.4 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.123 [1.079, 1.125] | same |
| explicit_multi_material | lumped_matrix | head-t4 | 3 | 120 | 35.3 µs [34.7 µs, 36.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.100 [1.040, 1.112] | same |
| explicit_multi_material | corrector | base-t1 | 3 | 120 | 19.5 µs [18.0 µs, 20.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_multi_material | corrector | base-t2 | 3 | 120 | 20.4 µs [19.3 µs, 22.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.070 [0.992, 1.153] | same |
| explicit_multi_material | corrector | base-t4 | 3 | 120 | 21.2 µs [18.9 µs, 22.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.033 [0.968, 1.258] | same |
| explicit_multi_material | corrector | head-t2 | 3 | 120 | 18.6 µs [16.5 µs, 18.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.920 [0.847, 1.030] | same |
| explicit_multi_material | corrector | head-t4 | 3 | 120 | 19.7 µs [15.7 µs, 22.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.088 [0.807, 1.096] | same |
| explicit_multi_material | step_hooks | base-t1 | 3 | 240 | 94.39 ms [94.11 ms, 94.73 ms] | 393.3 µs [392.1 µs, 394.7 µs] | 3.7 [3.7, 3.7] | – | same |
| explicit_multi_material | step_hooks | base-t2 | 3 | 240 | 71.71 ms [71.27 ms, 73.52 ms] | 298.8 µs [297.0 µs, 306.3 µs] | 2.8 [2.8, 2.9] | 0.760 [0.757, 0.776] | same |
| explicit_multi_material | step_hooks | base-t4 | 3 | 240 | 61.90 ms [61.10 ms, 62.38 ms] | 257.9 µs [254.6 µs, 259.9 µs] | 2.4 [2.4, 2.5] | 0.656 [0.649, 0.659] | same |
| explicit_multi_material | step_hooks | head-t2 | 3 | 240 | 63.82 ms [63.46 ms, 63.84 ms] | 265.9 µs [264.4 µs, 266.0 µs] | 2.5 [2.5, 2.5] | 0.674 [0.672, 0.678] | same |
| explicit_multi_material | step_hooks | head-t4 | 3 | 240 | 47.71 ms [47.32 ms, 47.80 ms] | 198.8 µs [197.2 µs, 199.2 µs] | 1.9 [1.9, 1.9] | 0.504 [0.501, 0.508] | same |
| explicit_multi_material | step_rest | base-t1 | 3 | 120 | 39.23 ms [39.20 ms, 39.74 ms] | 326.9 µs [326.7 µs, 331.1 µs] | 3.1 [3.1, 3.1] | – | same |
| explicit_multi_material | step_rest | base-t2 | 3 | 120 | 39.85 ms [39.65 ms, 40.17 ms] | 332.1 µs [330.5 µs, 334.8 µs] | 3.1 [3.1, 3.2] | 1.012 [1.003, 1.024] | same |
| explicit_multi_material | step_rest | base-t4 | 3 | 120 | 41.07 ms [39.85 ms, 43.09 ms] | 342.3 µs [332.1 µs, 359.1 µs] | 3.2 [3.1, 3.4] | 1.047 [1.017, 1.084] | same |
| explicit_multi_material | step_rest | head-t2 | 3 | 120 | 30.55 ms [30.34 ms, 31.08 ms] | 254.5 µs [252.8 µs, 259.0 µs] | 2.4 [2.4, 2.5] | 0.779 [0.763, 0.792] | same |
| explicit_multi_material | step_rest | head-t4 | 3 | 120 | 25.64 ms [25.54 ms, 26.09 ms] | 213.7 µs [212.8 µs, 217.5 µs] | 2.0 [2.0, 2.1] | 0.651 [0.645, 0.666] | same |
| explicit_multi_material | internal_forces | base-t1 | 3 | 24 | 374.97 ms [372.86 ms, 375.37 ms] | 15.62 ms [15.54 ms, 15.64 ms] | 148.2 [147.3, 148.3] | – | same |
| explicit_multi_material | internal_forces | base-t2 | 3 | 24 | 195.87 ms [194.99 ms, 196.41 ms] | 8.16 ms [8.12 ms, 8.18 ms] | 77.4 [77.0, 77.6] | 0.522 [0.519, 0.527] | same |
| explicit_multi_material | internal_forces | base-t4 | 3 | 24 | 113.23 ms [112.73 ms, 113.61 ms] | 4.72 ms [4.70 ms, 4.73 ms] | 44.7 [44.5, 44.9] | 0.303 [0.300, 0.304] | same |
| explicit_multi_material | internal_forces | head-t2 | 3 | 24 | 195.10 ms [193.46 ms, 198.14 ms] | 8.13 ms [8.06 ms, 8.26 ms] | 77.1 [76.4, 78.3] | 0.520 [0.519, 0.528] | same |
| explicit_multi_material | internal_forces | head-t4 | 3 | 24 | 109.70 ms [108.86 ms, 110.11 ms] | 4.57 ms [4.54 ms, 4.59 ms] | 43.3 [43.0, 43.5] | 0.294 [0.290, 0.294] | same |
| explicit_multi_material | stresses | base-t1 | 3 | 24 | 316.41 ms [316.11 ms, 317.35 ms] | 13.18 ms [13.17 ms, 13.22 ms] | 125.0 [124.9, 125.4] | – | same |
| explicit_multi_material | stresses | base-t2 | 3 | 24 | 300.00 ms [298.61 ms, 301.59 ms] | 12.50 ms [12.44 ms, 12.57 ms] | 118.5 [118.0, 119.2] | 0.949 [0.944, 0.950] | same |
| explicit_multi_material | stresses | base-t4 | 3 | 24 | 294.10 ms [293.90 ms, 294.32 ms] | 12.25 ms [12.25 ms, 12.26 ms] | 116.2 [116.1, 116.3] | 0.929 [0.927, 0.931] | same |
| explicit_multi_material | stresses | head-t2 | 3 | 24 | 298.68 ms [295.79 ms, 299.21 ms] | 12.45 ms [12.32 ms, 12.47 ms] | 118.0 [116.9, 118.2] | 0.941 [0.936, 0.946] | same |
| explicit_multi_material | stresses | head-t4 | 3 | 24 | 291.88 ms [291.87 ms, 292.28 ms] | 12.16 ms [12.16 ms, 12.18 ms] | 115.3 [115.3, 115.5] | 0.922 [0.920, 0.925] | same |
| explicit_multi_material | energies | base-t1 | 3 | 24 | 38.04 ms [38.00 ms, 38.17 ms] | 1.59 ms [1.58 ms, 1.59 ms] | 15.0 [15.0, 15.1] | – | same |
| explicit_multi_material | energies | base-t2 | 3 | 24 | 37.95 ms [37.92 ms, 38.03 ms] | 1.58 ms [1.58 ms, 1.58 ms] | 15.0 [15.0, 15.0] | 0.998 [0.994, 1.000] | same |
| explicit_multi_material | energies | base-t4 | 3 | 24 | 38.07 ms [38.04 ms, 38.25 ms] | 1.59 ms [1.58 ms, 1.59 ms] | 15.0 [15.0, 15.1] | 1.002 [1.000, 1.002] | same |
| explicit_multi_material | energies | head-t2 | 3 | 24 | 36.57 ms [36.50 ms, 36.62 ms] | 1.52 ms [1.52 ms, 1.53 ms] | 14.4 [14.4, 14.5] | 0.962 [0.956, 0.963] | same |
| explicit_multi_material | energies | head-t4 | 3 | 24 | 35.90 ms [35.85 ms, 36.13 ms] | 1.50 ms [1.49 ms, 1.51 ms] | 14.2 [14.2, 14.3] | 0.945 [0.942, 0.947] | same |
| explicit_neohookean | mesh | base-t1 | 3 | 1 | 2.04 ms [2.02 ms, 2.18 ms] | 2.04 ms [2.02 ms, 2.18 ms] | 19.3 [19.2, 20.7] | – | same |
| explicit_neohookean | mesh | base-t2 | 3 | 1 | 2.06 ms [2.01 ms, 2.13 ms] | 2.06 ms [2.01 ms, 2.13 ms] | 19.5 [19.0, 20.2] | 1.009 [0.919, 1.056] | same |
| explicit_neohookean | mesh | base-t4 | 3 | 1 | 2.03 ms [1.97 ms, 2.17 ms] | 2.03 ms [1.97 ms, 2.17 ms] | 19.2 [18.7, 20.6] | 0.993 [0.975, 0.997] | same |
| explicit_neohookean | mesh | head-t2 | 3 | 1 | 2.02 ms [1.99 ms, 2.11 ms] | 2.02 ms [1.99 ms, 2.11 ms] | 19.2 [18.9, 20.0] | 0.987 [0.927, 1.036] | same |
| explicit_neohookean | mesh | head-t4 | 3 | 1 | 2.03 ms [2.01 ms, 2.20 ms] | 2.03 ms [2.01 ms, 2.20 ms] | 19.2 [19.0, 20.8] | 0.995 [0.920, 1.089] | same |
| explicit_neohookean | init | base-t1 | 3 | 1 | 34.69 ms [34.20 ms, 35.23 ms] | 34.69 ms [34.20 ms, 35.23 ms] | 329.0 [324.3, 334.1] | – | same |
| explicit_neohookean | init | base-t2 | 3 | 1 | 34.12 ms [33.74 ms, 34.92 ms] | 34.12 ms [33.74 ms, 34.92 ms] | 323.5 [319.9, 331.1] | 0.998 [0.958, 1.007] | same |
| explicit_neohookean | init | base-t4 | 3 | 1 | 35.62 ms [34.38 ms, 35.99 ms] | 35.62 ms [34.38 ms, 35.99 ms] | 337.7 [326.0, 341.3] | 1.022 [1.005, 1.027] | same |
| explicit_neohookean | init | head-t2 | 3 | 1 | 34.82 ms [34.19 ms, 35.32 ms] | 34.82 ms [34.19 ms, 35.32 ms] | 330.2 [324.2, 335.0] | 1.004 [0.970, 1.033] | same |
| explicit_neohookean | init | head-t4 | 3 | 1 | 34.76 ms [34.74 ms, 34.92 ms] | 34.76 ms [34.74 ms, 34.92 ms] | 329.6 [329.4, 331.2] | 1.007 [0.987, 1.016] | same |
| explicit_neohookean | stable_time_step | base-t1 | 3 | 5 | 21.10 ms [21.10 ms, 21.11 ms] | 4.22 ms [4.22 ms, 4.22 ms] | 40.0 [40.0, 40.0] | – | same |
| explicit_neohookean | stable_time_step | base-t2 | 3 | 5 | 22.43 ms [22.07 ms, 24.17 ms] | 4.49 ms [4.41 ms, 4.83 ms] | 42.5 [41.9, 45.8] | 1.063 [1.046, 1.145] | same |
| explicit_neohookean | stable_time_step | base-t4 | 3 | 5 | 21.23 ms [21.12 ms, 21.28 ms] | 4.25 ms [4.22 ms, 4.26 ms] | 40.3 [40.0, 40.4] | 1.006 [1.001, 1.008] | same |
| explicit_neohookean | stable_time_step | head-t2 | 3 | 5 | 22.12 ms [22.11 ms, 23.45 ms] | 4.42 ms [4.42 ms, 4.69 ms] | 42.0 [41.9, 44.5] | 1.048 [1.048, 1.112] | same |
| explicit_neohookean | stable_time_step | head-t4 | 3 | 5 | 21.09 ms [21.04 ms, 21.19 ms] | 4.22 ms [4.21 ms, 4.24 ms] | 40.0 [39.9, 40.2] | 1.000 [0.997, 1.004] | same |
| explicit_neohookean | solve_step | base-t1 | 3 | 100 | 1.048 s [1.045 s, 1.056 s] | 10.48 ms [10.45 ms, 10.56 ms] | 99.4 [99.1, 100.1] | – | same |
| explicit_neohookean | solve_step | base-t2 | 3 | 100 | 610.88 ms [610.80 ms, 616.24 ms] | 6.11 ms [6.11 ms, 6.16 ms] | 57.9 [57.9, 58.4] | 0.585 [0.579, 0.588] | same |
| explicit_neohookean | solve_step | base-t4 | 3 | 100 | 380.73 ms [379.41 ms, 383.44 ms] | 3.81 ms [3.79 ms, 3.83 ms] | 36.1 [36.0, 36.4] | 0.363 [0.359, 0.367] | same |
| explicit_neohookean | solve_step | head-t2 | 3 | 100 | 594.22 ms [593.76 ms, 599.27 ms] | 5.94 ms [5.94 ms, 5.99 ms] | 56.3 [56.3, 56.8] | 0.567 [0.563, 0.574] | same |
| explicit_neohookean | solve_step | head-t4 | 3 | 100 | 355.81 ms [354.61 ms, 366.45 ms] | 3.56 ms [3.55 ms, 3.66 ms] | 33.7 [33.6, 34.7] | 0.341 [0.336, 0.350] | same |
| explicit_neohookean | predictor | base-t1 | 3 | 100 | 9.0 µs [6.1 µs, 13.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | predictor | base-t2 | 3 | 100 | 10.9 µs [9.4 µs, 11.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.039 [0.796, 1.784] | same |
| explicit_neohookean | predictor | base-t4 | 3 | 100 | 10.8 µs [9.6 µs, 11.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.067 [0.824, 1.759] | same |
| explicit_neohookean | predictor | head-t2 | 3 | 100 | 2.8 µs [2.8 µs, 4.6 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.332 [0.311, 0.454] | same |
| explicit_neohookean | predictor | head-t4 | 3 | 100 | 2.6 µs [2.5 µs, 2.9 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.325 [0.181, 0.419] | same |
| explicit_neohookean | residual | base-t1 | 3 | 100 | 1.017 s [1.013 s, 1.025 s] | 10.17 ms [10.13 ms, 10.25 ms] | 96.4 [96.1, 97.2] | – | same |
| explicit_neohookean | residual | base-t2 | 3 | 100 | 576.55 ms [576.07 ms, 581.90 ms] | 5.77 ms [5.76 ms, 5.82 ms] | 54.7 [54.6, 55.2] | 0.569 [0.563, 0.572] | same |
| explicit_neohookean | residual | base-t4 | 3 | 100 | 345.79 ms [344.75 ms, 348.06 ms] | 3.46 ms [3.45 ms, 3.48 ms] | 32.8 [32.7, 33.0] | 0.340 [0.336, 0.344] | same |
| explicit_neohookean | residual | head-t2 | 3 | 100 | 569.09 ms [568.82 ms, 573.40 ms] | 5.69 ms [5.69 ms, 5.73 ms] | 54.0 [53.9, 54.4] | 0.559 [0.555, 0.566] | same |
| explicit_neohookean | residual | head-t4 | 3 | 100 | 334.95 ms [333.41 ms, 345.79 ms] | 3.35 ms [3.33 ms, 3.46 ms] | 31.8 [31.6, 32.8] | 0.331 [0.325, 0.340] | same |
| explicit_neohookean | lumped_matrix | base-t1 | 3 | 100 | 24.2 µs [22.9 µs, 24.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | lumped_matrix | base-t2 | 3 | 100 | 25.6 µs [23.4 µs, 25.7 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.039 [1.020, 1.062] | same |
| explicit_neohookean | lumped_matrix | base-t4 | 3 | 100 | 23.1 µs [19.9 µs, 27.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.008 [0.821, 1.099] | same |
| explicit_neohookean | lumped_matrix | head-t2 | 3 | 100 | 26.4 µs [26.0 µs, 27.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.120 [1.072, 1.133] | same |
| explicit_neohookean | lumped_matrix | head-t4 | 3 | 100 | 25.6 µs [25.5 µs, 26.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.086 [1.038, 1.115] | same |
| explicit_neohookean | corrector | base-t1 | 3 | 100 | 12.2 µs [10.2 µs, 17.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | corrector | base-t2 | 3 | 100 | 17.5 µs [15.4 µs, 20.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.434 [1.171, 1.515] | same |
| explicit_neohookean | corrector | base-t4 | 3 | 100 | 21.0 µs [18.6 µs, 23.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.827 [1.182, 1.897] | same |
| explicit_neohookean | corrector | head-t2 | 3 | 100 | 16.1 µs [15.8 µs, 19.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.289 [0.910, 1.891] | same |
| explicit_neohookean | corrector | head-t4 | 3 | 100 | 15.3 µs [14.0 µs, 25.5 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.379 [1.255, 1.440] | same |
| explicit_neohookean | step_hooks | base-t1 | 3 | 200 | 260.9 µs [255.3 µs, 261.3 µs] | 1.3 µs [1.3 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_neohookean | step_hooks | base-t2 | 3 | 200 | 283.9 µs [269.3 µs, 297.5 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.088 [1.055, 1.139] | same |
| explicit_neohookean | step_hooks | base-t4 | 3 | 200 | 268.8 µs [260.3 µs, 300.3 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.053 [0.996, 1.151] | same |
| explicit_neohookean | step_hooks | head-t2 | 3 | 200 | 274.3 µs [270.5 µs, 303.8 µs] | 1.4 µs [1.4 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.060 [1.050, 1.164] | same |
| explicit_neohookean | step_hooks | head-t4 | 3 | 200 | 264.0 µs [260.2 µs, 297.9 µs] | 1.3 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.019 [1.010, 1.142] | same |
| explicit_neohookean | step_rest | base-t1 | 3 | 100 | 30.63 ms [30.57 ms, 30.84 ms] | 306.3 µs [305.7 µs, 308.4 µs] | 2.9 [2.9, 2.9] | – | same |
| explicit_neohookean | step_rest | base-t2 | 3 | 100 | 33.79 ms [33.76 ms, 34.14 ms] | 337.9 µs [337.6 µs, 341.4 µs] | 3.2 [3.2, 3.2] | 1.104 [1.103, 1.107] | same |
| explicit_neohookean | step_rest | base-t4 | 3 | 100 | 34.39 ms [34.06 ms, 34.77 ms] | 343.9 µs [340.6 µs, 347.7 µs] | 3.3 [3.2, 3.3] | 1.125 [1.112, 1.127] | same |
| explicit_neohookean | step_rest | head-t2 | 3 | 100 | 24.55 ms [24.35 ms, 25.28 ms] | 245.5 µs [243.5 µs, 252.8 µs] | 2.3 [2.3, 2.4] | 0.801 [0.796, 0.820] | same |
| explicit_neohookean | step_rest | head-t4 | 3 | 100 | 20.28 ms [20.11 ms, 20.66 ms] | 202.8 µs [201.1 µs, 206.6 µs] | 1.9 [1.9, 2.0] | 0.658 [0.657, 0.674] | same |
| explicit_neohookean | internal_forces | base-t1 | 3 | 20 | 195.70 ms [192.05 ms, 197.57 ms] | 9.78 ms [9.60 ms, 9.88 ms] | 92.8 [91.1, 93.7] | – | same |
| explicit_neohookean | internal_forces | base-t2 | 3 | 20 | 103.53 ms [102.96 ms, 103.89 ms] | 5.18 ms [5.15 ms, 5.19 ms] | 49.1 [48.8, 49.3] | 0.531 [0.521, 0.539] | same |
| explicit_neohookean | internal_forces | base-t4 | 3 | 20 | 54.99 ms [54.96 ms, 55.18 ms] | 2.75 ms [2.75 ms, 2.76 ms] | 26.1 [26.1, 26.2] | 0.281 [0.279, 0.286] | same |
| explicit_neohookean | internal_forces | head-t2 | 3 | 20 | 103.27 ms [103.10 ms, 104.41 ms] | 5.16 ms [5.15 ms, 5.22 ms] | 49.0 [48.9, 49.5] | 0.527 [0.523, 0.544] | same |
| explicit_neohookean | internal_forces | head-t4 | 3 | 20 | 54.54 ms [54.52 ms, 54.77 ms] | 2.73 ms [2.73 ms, 2.74 ms] | 25.9 [25.8, 26.0] | 0.279 [0.276, 0.285] | same |
| explicit_neohookean | stresses | base-t1 | 3 | 20 | 128.43 ms [127.47 ms, 135.44 ms] | 6.42 ms [6.37 ms, 6.77 ms] | 60.9 [60.4, 64.2] | – | same |
| explicit_neohookean | stresses | base-t2 | 3 | 20 | 117.03 ms [114.43 ms, 119.69 ms] | 5.85 ms [5.72 ms, 5.98 ms] | 55.5 [54.3, 56.8] | 0.898 [0.884, 0.911] | same |
| explicit_neohookean | stresses | base-t4 | 3 | 20 | 111.97 ms [111.83 ms, 122.48 ms] | 5.60 ms [5.59 ms, 6.12 ms] | 53.1 [53.0, 58.1] | 0.877 [0.827, 0.954] | same |
| explicit_neohookean | stresses | head-t2 | 3 | 20 | 117.94 ms [115.48 ms, 119.80 ms] | 5.90 ms [5.77 ms, 5.99 ms] | 55.9 [54.8, 56.8] | 0.906 [0.885, 0.918] | same |
| explicit_neohookean | stresses | head-t4 | 3 | 20 | 112.75 ms [111.40 ms, 114.83 ms] | 5.64 ms [5.57 ms, 5.74 ms] | 53.5 [52.8, 54.4] | 0.867 [0.848, 0.885] | same |
| explicit_neohookean | energies | base-t1 | 3 | 20 | 123.22 ms [122.45 ms, 124.70 ms] | 6.16 ms [6.12 ms, 6.23 ms] | 58.4 [58.1, 59.1] | – | same |
| explicit_neohookean | energies | base-t2 | 3 | 20 | 125.68 ms [123.44 ms, 131.13 ms] | 6.28 ms [6.17 ms, 6.56 ms] | 59.6 [58.5, 62.2] | 1.026 [1.002, 1.052] | same |
| explicit_neohookean | energies | base-t4 | 3 | 20 | 124.70 ms [122.35 ms, 125.33 ms] | 6.24 ms [6.12 ms, 6.27 ms] | 59.1 [58.0, 59.4] | 1.005 [0.993, 1.018] | same |
| explicit_neohookean | energies | head-t2 | 3 | 20 | 124.67 ms [124.24 ms, 126.41 ms] | 6.23 ms [6.21 ms, 6.32 ms] | 59.1 [58.9, 59.9] | 1.014 [1.008, 1.018] | same |
| explicit_neohookean | energies | head-t4 | 3 | 20 | 127.35 ms [124.66 ms, 128.30 ms] | 6.37 ms [6.23 ms, 6.42 ms] | 60.4 [59.1, 60.8] | 1.021 [1.012, 1.048] | same |
| explicit_orthotropic | mesh | base-t1 | 3 | 1 | 2.19 ms [2.07 ms, 2.26 ms] | 2.19 ms [2.07 ms, 2.26 ms] | 20.8 [19.6, 21.4] | – | same |
| explicit_orthotropic | mesh | base-t2 | 3 | 1 | 2.05 ms [2.02 ms, 2.08 ms] | 2.05 ms [2.02 ms, 2.08 ms] | 19.4 [19.2, 19.8] | 0.934 [0.924, 0.980] | same |
| explicit_orthotropic | mesh | base-t4 | 3 | 1 | 2.16 ms [2.05 ms, 2.18 ms] | 2.16 ms [2.05 ms, 2.18 ms] | 20.5 [19.4, 20.6] | 0.956 [0.936, 1.053] | same |
| explicit_orthotropic | mesh | head-t2 | 3 | 1 | 2.00 ms [2.00 ms, 2.08 ms] | 2.00 ms [2.00 ms, 2.08 ms] | 19.0 [18.9, 19.7] | 0.913 [0.885, 1.005] | same |
| explicit_orthotropic | mesh | head-t4 | 3 | 1 | 2.16 ms [2.08 ms, 2.19 ms] | 2.16 ms [2.08 ms, 2.19 ms] | 20.5 [19.7, 20.8] | 1.002 [0.922, 1.044] | same |
| explicit_orthotropic | init | base-t1 | 3 | 1 | 30.08 ms [29.93 ms, 30.76 ms] | 30.08 ms [29.93 ms, 30.76 ms] | 285.3 [283.8, 291.7] | – | same |
| explicit_orthotropic | init | base-t2 | 3 | 1 | 30.42 ms [29.81 ms, 31.03 ms] | 30.42 ms [29.81 ms, 31.03 ms] | 288.5 [282.6, 294.2] | 1.011 [0.969, 1.037] | same |
| explicit_orthotropic | init | base-t4 | 3 | 1 | 31.35 ms [30.66 ms, 31.87 ms] | 31.35 ms [30.66 ms, 31.87 ms] | 297.2 [290.7, 302.2] | 1.024 [1.019, 1.059] | same |
| explicit_orthotropic | init | head-t2 | 3 | 1 | 30.33 ms [30.27 ms, 30.77 ms] | 30.33 ms [30.27 ms, 30.77 ms] | 287.6 [287.1, 291.7] | 1.013 [0.984, 1.023] | same |
| explicit_orthotropic | init | head-t4 | 3 | 1 | 30.83 ms [30.28 ms, 31.19 ms] | 30.83 ms [30.28 ms, 31.19 ms] | 292.3 [287.2, 295.8] | 1.012 [1.002, 1.037] | same |
| explicit_orthotropic | stable_time_step | base-t1 | 3 | 5 | 21.17 ms [21.10 ms, 21.20 ms] | 4.23 ms [4.22 ms, 4.24 ms] | 40.1 [40.0, 40.2] | – | same |
| explicit_orthotropic | stable_time_step | base-t2 | 3 | 5 | 21.19 ms [21.13 ms, 21.29 ms] | 4.24 ms [4.23 ms, 4.26 ms] | 40.2 [40.1, 40.4] | 1.001 [0.997, 1.009] | same |
| explicit_orthotropic | stable_time_step | base-t4 | 3 | 5 | 21.26 ms [21.17 ms, 21.42 ms] | 4.25 ms [4.23 ms, 4.28 ms] | 40.3 [40.2, 40.6] | 1.003 [1.000, 1.015] | same |
| explicit_orthotropic | stable_time_step | head-t2 | 3 | 5 | 21.10 ms [21.09 ms, 21.11 ms] | 4.22 ms [4.22 ms, 4.22 ms] | 40.0 [40.0, 40.0] | 0.997 [0.995, 1.000] | same |
| explicit_orthotropic | stable_time_step | head-t4 | 3 | 5 | 21.12 ms [21.08 ms, 21.14 ms] | 4.22 ms [4.22 ms, 4.23 ms] | 40.1 [40.0, 40.1] | 0.998 [0.994, 1.001] | same |
| explicit_orthotropic | solve_step | base-t1 | 3 | 200 | 969.27 ms [959.46 ms, 998.79 ms] | 4.85 ms [4.80 ms, 4.99 ms] | 46.0 [45.5, 47.4] | – | same |
| explicit_orthotropic | solve_step | base-t2 | 3 | 200 | 588.05 ms [585.83 ms, 590.25 ms] | 2.94 ms [2.93 ms, 2.95 ms] | 27.9 [27.8, 28.0] | 0.604 [0.591, 0.613] | same |
| explicit_orthotropic | solve_step | base-t4 | 3 | 200 | 410.23 ms [407.16 ms, 416.62 ms] | 2.05 ms [2.04 ms, 2.08 ms] | 19.5 [19.3, 19.8] | 0.423 [0.417, 0.424] | same |
| explicit_orthotropic | solve_step | head-t2 | 3 | 200 | 562.91 ms [562.47 ms, 563.70 ms] | 2.81 ms [2.81 ms, 2.82 ms] | 26.7 [26.7, 26.7] | 0.581 [0.563, 0.588] | same |
| explicit_orthotropic | solve_step | head-t4 | 3 | 200 | 362.16 ms [359.75 ms, 363.08 ms] | 1.81 ms [1.80 ms, 1.82 ms] | 17.2 [17.1, 17.2] | 0.375 [0.360, 0.377] | same |
| explicit_orthotropic | predictor | base-t1 | 3 | 200 | 18.9 µs [18.2 µs, 21.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | predictor | base-t2 | 3 | 200 | 21.3 µs [16.7 µs, 24.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.993 [0.916, 1.274] | same |
| explicit_orthotropic | predictor | base-t4 | 3 | 200 | 19.2 µs [18.4 µs, 23.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.055 [0.857, 1.219] | same |
| explicit_orthotropic | predictor | head-t2 | 3 | 200 | 5.5 µs [5.4 µs, 6.4 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.295 [0.288, 0.298] | same |
| explicit_orthotropic | predictor | head-t4 | 3 | 200 | 5.4 µs [5.1 µs, 5.9 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.294 [0.238, 0.314] | same |
| explicit_orthotropic | residual | base-t1 | 3 | 200 | 906.97 ms [898.40 ms, 930.10 ms] | 4.53 ms [4.49 ms, 4.65 ms] | 43.0 [42.6, 44.1] | – | same |
| explicit_orthotropic | residual | base-t2 | 3 | 200 | 518.92 ms [515.80 ms, 520.77 ms] | 2.59 ms [2.58 ms, 2.60 ms] | 24.6 [24.5, 24.7] | 0.569 [0.560, 0.578] | same |
| explicit_orthotropic | residual | base-t4 | 3 | 200 | 339.67 ms [337.62 ms, 346.66 ms] | 1.70 ms [1.69 ms, 1.73 ms] | 16.1 [16.0, 16.4] | 0.375 [0.373, 0.376] | same |
| explicit_orthotropic | residual | head-t2 | 3 | 200 | 512.41 ms [511.37 ms, 512.87 ms] | 2.56 ms [2.56 ms, 2.56 ms] | 24.3 [24.2, 24.3] | 0.565 [0.550, 0.571] | same |
| explicit_orthotropic | residual | head-t4 | 3 | 200 | 322.02 ms [319.09 ms, 322.99 ms] | 1.61 ms [1.60 ms, 1.61 ms] | 15.3 [15.1, 15.3] | 0.356 [0.343, 0.358] | same |
| explicit_orthotropic | lumped_matrix | base-t1 | 3 | 200 | 40.5 µs [38.9 µs, 46.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | lumped_matrix | base-t2 | 3 | 200 | 48.5 µs [45.9 µs, 51.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.200 [0.982, 1.312] | same |
| explicit_orthotropic | lumped_matrix | base-t4 | 3 | 200 | 45.6 µs [45.1 µs, 52.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.127 [0.966, 1.336] | same |
| explicit_orthotropic | lumped_matrix | head-t2 | 3 | 200 | 54.4 µs [51.8 µs, 56.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.329 [1.208, 1.345] | same |
| explicit_orthotropic | lumped_matrix | head-t4 | 3 | 200 | 53.1 µs [51.4 µs, 55.9 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.313 [1.100, 1.436] | same |
| explicit_orthotropic | corrector | base-t1 | 3 | 200 | 27.2 µs [20.5 µs, 41.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | corrector | base-t2 | 3 | 200 | 36.4 µs [32.9 µs, 37.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.361 [0.871, 1.605] | same |
| explicit_orthotropic | corrector | base-t4 | 3 | 200 | 30.3 µs [29.5 µs, 31.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.141 [0.704, 1.477] | same |
| explicit_orthotropic | corrector | head-t2 | 3 | 200 | 29.1 µs [28.4 µs, 38.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.407 [0.679, 1.418] | same |
| explicit_orthotropic | corrector | head-t4 | 3 | 200 | 28.7 µs [26.2 µs, 34.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.965 [0.824, 1.399] | same |
| explicit_orthotropic | step_hooks | base-t1 | 3 | 400 | 419.6 µs [404.9 µs, 483.8 µs] | 1.0 µs [1.0 µs, 1.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_orthotropic | step_hooks | base-t2 | 3 | 400 | 448.7 µs [446.8 µs, 518.3 µs] | 1.1 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 1.108 [0.923, 1.235] | same |
| explicit_orthotropic | step_hooks | base-t4 | 3 | 400 | 408.1 µs [398.9 µs, 483.7 µs] | 1.0 µs [1.0 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 1.008 [0.824, 1.153] | same |
| explicit_orthotropic | step_hooks | head-t2 | 3 | 400 | 486.1 µs [472.7 µs, 522.8 µs] | 1.2 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 1.126 [1.081, 1.201] | same |
| explicit_orthotropic | step_hooks | head-t4 | 3 | 400 | 467.8 µs [445.2 µs, 475.2 µs] | 1.2 µs [1.1 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 1.061 [0.982, 1.155] | same |
| explicit_orthotropic | step_rest | base-t1 | 3 | 200 | 61.39 ms [60.17 ms, 67.66 ms] | 307.0 µs [300.9 µs, 338.3 µs] | 2.9 [2.9, 3.2] | – | same |
| explicit_orthotropic | step_rest | base-t2 | 3 | 200 | 68.47 ms [68.14 ms, 68.95 ms] | 342.4 µs [340.7 µs, 344.7 µs] | 3.2 [3.2, 3.3] | 1.123 [1.012, 1.132] | same |
| explicit_orthotropic | step_rest | base-t4 | 3 | 200 | 68.96 ms [68.53 ms, 69.49 ms] | 344.8 µs [342.7 µs, 347.4 µs] | 3.3 [3.2, 3.3] | 1.132 [1.019, 1.139] | same |
| explicit_orthotropic | step_rest | head-t2 | 3 | 200 | 49.75 ms [49.44 ms, 50.02 ms] | 248.8 µs [247.2 µs, 250.1 µs] | 2.4 [2.3, 2.4] | 0.805 [0.739, 0.827] | same |
| explicit_orthotropic | step_rest | head-t4 | 3 | 200 | 39.10 ms [39.07 ms, 39.56 ms] | 195.5 µs [195.4 µs, 197.8 µs] | 1.9 [1.9, 1.9] | 0.636 [0.585, 0.650] | same |
| explicit_orthotropic | internal_forces | base-t1 | 3 | 40 | 172.82 ms [172.68 ms, 173.99 ms] | 4.32 ms [4.32 ms, 4.35 ms] | 41.0 [40.9, 41.2] | – | same |
| explicit_orthotropic | internal_forces | base-t2 | 3 | 40 | 93.70 ms [92.74 ms, 93.97 ms] | 2.34 ms [2.32 ms, 2.35 ms] | 22.2 [22.0, 22.3] | 0.540 [0.537, 0.542] | same |
| explicit_orthotropic | internal_forces | base-t4 | 3 | 40 | 53.81 ms [53.54 ms, 56.40 ms] | 1.35 ms [1.34 ms, 1.41 ms] | 12.8 [12.7, 13.4] | 0.310 [0.309, 0.326] | same |
| explicit_orthotropic | internal_forces | head-t2 | 3 | 40 | 93.94 ms [93.72 ms, 97.33 ms] | 2.35 ms [2.34 ms, 2.43 ms] | 22.3 [22.2, 23.1] | 0.544 [0.539, 0.564] | same |
| explicit_orthotropic | internal_forces | head-t4 | 3 | 40 | 52.69 ms [52.29 ms, 53.09 ms] | 1.32 ms [1.31 ms, 1.33 ms] | 12.5 [12.4, 12.6] | 0.305 [0.303, 0.305] | same |
| explicit_orthotropic | stresses | base-t1 | 3 | 40 | 92.93 ms [92.30 ms, 95.72 ms] | 2.32 ms [2.31 ms, 2.39 ms] | 22.0 [21.9, 22.7] | – | same |
| explicit_orthotropic | stresses | base-t2 | 3 | 40 | 74.50 ms [74.11 ms, 74.97 ms] | 1.86 ms [1.85 ms, 1.87 ms] | 17.7 [17.6, 17.8] | 0.803 [0.778, 0.807] | same |
| explicit_orthotropic | stresses | base-t4 | 3 | 40 | 65.29 ms [64.87 ms, 65.49 ms] | 1.63 ms [1.62 ms, 1.64 ms] | 15.5 [15.4, 15.5] | 0.703 [0.684, 0.703] | same |
| explicit_orthotropic | stresses | head-t2 | 3 | 40 | 78.49 ms [78.22 ms, 78.78 ms] | 1.96 ms [1.96 ms, 1.97 ms] | 18.6 [18.5, 18.7] | 0.848 [0.820, 0.848] | same |
| explicit_orthotropic | stresses | head-t4 | 3 | 40 | 69.45 ms [69.11 ms, 69.70 ms] | 1.74 ms [1.73 ms, 1.74 ms] | 16.5 [16.4, 16.5] | 0.744 [0.726, 0.755] | same |
| explicit_orthotropic | energies | base-t1 | 3 | 40 | 46.27 ms [46.21 ms, 47.23 ms] | 1.16 ms [1.16 ms, 1.18 ms] | 11.0 [11.0, 11.2] | – | same |
| explicit_orthotropic | energies | base-t2 | 3 | 40 | 46.02 ms [45.59 ms, 46.18 ms] | 1.15 ms [1.14 ms, 1.15 ms] | 10.9 [10.8, 10.9] | 0.987 [0.974, 0.998] | same |
| explicit_orthotropic | energies | base-t4 | 3 | 40 | 46.27 ms [45.95 ms, 46.37 ms] | 1.16 ms [1.15 ms, 1.16 ms] | 11.0 [10.9, 11.0] | 1.001 [0.973, 1.002] | same |
| explicit_orthotropic | energies | head-t2 | 3 | 40 | 48.49 ms [48.49 ms, 48.64 ms] | 1.21 ms [1.21 ms, 1.22 ms] | 11.5 [11.5, 11.5] | 1.048 [1.030, 1.049] | same |
| explicit_orthotropic | energies | head-t4 | 3 | 40 | 48.34 ms [48.25 ms, 48.60 ms] | 1.21 ms [1.21 ms, 1.21 ms] | 11.5 [11.4, 11.5] | 1.044 [1.023, 1.050] | same |
| explicit_plastic | mesh | base-t1 | 3 | 1 | 2.25 ms [2.20 ms, 2.25 ms] | 2.25 ms [2.20 ms, 2.25 ms] | 21.3 [20.9, 21.3] | – | same |
| explicit_plastic | mesh | base-t2 | 3 | 1 | 2.14 ms [2.10 ms, 2.22 ms] | 2.14 ms [2.10 ms, 2.22 ms] | 20.3 [19.9, 21.0] | 0.971 [0.932, 0.987] | same |
| explicit_plastic | mesh | base-t4 | 3 | 1 | 2.11 ms [2.06 ms, 2.16 ms] | 2.11 ms [2.06 ms, 2.16 ms] | 20.0 [19.6, 20.5] | 0.961 [0.918, 0.962] | same |
| explicit_plastic | mesh | head-t2 | 3 | 1 | 2.03 ms [2.00 ms, 2.04 ms] | 2.03 ms [2.00 ms, 2.04 ms] | 19.2 [18.9, 19.3] | 0.908 [0.889, 0.921] | same |
| explicit_plastic | mesh | head-t4 | 3 | 1 | 2.14 ms [2.02 ms, 2.19 ms] | 2.14 ms [2.02 ms, 2.19 ms] | 20.3 [19.1, 20.8] | 0.950 [0.917, 0.976] | same |
| explicit_plastic | init | base-t1 | 3 | 1 | 35.10 ms [34.60 ms, 35.21 ms] | 35.10 ms [34.60 ms, 35.21 ms] | 332.8 [328.1, 333.9] | – | same |
| explicit_plastic | init | base-t2 | 3 | 1 | 33.55 ms [33.36 ms, 34.42 ms] | 33.55 ms [33.36 ms, 34.42 ms] | 318.1 [316.4, 326.4] | 0.956 [0.948, 0.995] | same |
| explicit_plastic | init | base-t4 | 3 | 1 | 34.61 ms [34.02 ms, 34.72 ms] | 34.61 ms [34.02 ms, 34.72 ms] | 328.2 [322.6, 329.3] | 0.983 [0.969, 1.003] | same |
| explicit_plastic | init | head-t2 | 3 | 1 | 34.15 ms [33.17 ms, 34.51 ms] | 34.15 ms [33.17 ms, 34.51 ms] | 323.8 [314.6, 327.3] | 0.973 [0.942, 0.997] | same |
| explicit_plastic | init | head-t4 | 3 | 1 | 34.35 ms [34.07 ms, 35.11 ms] | 34.35 ms [34.07 ms, 35.11 ms] | 325.7 [323.1, 332.9] | 0.993 [0.971, 0.997] | same |
| explicit_plastic | stable_time_step | base-t1 | 3 | 5 | 21.21 ms [21.11 ms, 21.42 ms] | 4.24 ms [4.22 ms, 4.28 ms] | 40.2 [40.0, 40.6] | – | same |
| explicit_plastic | stable_time_step | base-t2 | 3 | 5 | 21.29 ms [21.13 ms, 22.63 ms] | 4.26 ms [4.23 ms, 4.53 ms] | 40.4 [40.1, 42.9] | 1.008 [0.986, 1.067] | same |
| explicit_plastic | stable_time_step | base-t4 | 3 | 5 | 21.72 ms [21.23 ms, 21.81 ms] | 4.34 ms [4.25 ms, 4.36 ms] | 41.2 [40.3, 41.4] | 1.028 [0.991, 1.029] | same |
| explicit_plastic | stable_time_step | head-t2 | 3 | 5 | 21.12 ms [21.05 ms, 21.56 ms] | 4.22 ms [4.21 ms, 4.31 ms] | 40.1 [39.9, 40.9] | 0.992 [0.986, 1.021] | same |
| explicit_plastic | stable_time_step | head-t4 | 3 | 5 | 21.16 ms [21.09 ms, 21.60 ms] | 4.23 ms [4.22 ms, 4.32 ms] | 40.1 [40.0, 41.0] | 1.002 [0.994, 1.009] | same |
| explicit_plastic | solve_step | base-t1 | 3 | 120 | 991.46 ms [987.51 ms, 1.018 s] | 8.26 ms [8.23 ms, 8.49 ms] | 78.3 [78.0, 80.5] | – | same |
| explicit_plastic | solve_step | base-t2 | 3 | 120 | 604.12 ms [603.96 ms, 606.45 ms] | 5.03 ms [5.03 ms, 5.05 ms] | 47.7 [47.7, 47.9] | 0.609 [0.595, 0.612] | same |
| explicit_plastic | solve_step | base-t4 | 3 | 120 | 438.46 ms [436.17 ms, 439.04 ms] | 3.65 ms [3.63 ms, 3.66 ms] | 34.6 [34.5, 34.7] | 0.440 [0.431, 0.445] | same |
| explicit_plastic | solve_step | head-t2 | 3 | 120 | 581.93 ms [578.13 ms, 583.11 ms] | 4.85 ms [4.82 ms, 4.86 ms] | 46.0 [45.7, 46.1] | 0.583 [0.571, 0.590] | same |
| explicit_plastic | solve_step | head-t4 | 3 | 120 | 408.69 ms [405.13 ms, 409.50 ms] | 3.41 ms [3.38 ms, 3.41 ms] | 32.3 [32.0, 32.4] | 0.410 [0.401, 0.413] | same |
| explicit_plastic | predictor | base-t1 | 3 | 120 | 17.3 µs [17.2 µs, 17.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | predictor | base-t2 | 3 | 120 | 23.3 µs [20.8 µs, 24.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.336 [1.202, 1.396] | same |
| explicit_plastic | predictor | base-t4 | 3 | 120 | 23.3 µs [19.6 µs, 25.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.337 [1.132, 1.461] | same |
| explicit_plastic | predictor | head-t2 | 3 | 120 | 11.1 µs [10.6 µs, 12.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.645 [0.608, 0.749] | same |
| explicit_plastic | predictor | head-t4 | 3 | 120 | 10.3 µs [6.2 µs, 16.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.587 [0.360, 0.938] | same |
| explicit_plastic | residual | base-t1 | 3 | 120 | 869.13 ms [866.55 ms, 897.42 ms] | 7.24 ms [7.22 ms, 7.48 ms] | 68.7 [68.5, 70.9] | – | same |
| explicit_plastic | residual | base-t2 | 3 | 120 | 481.60 ms [481.51 ms, 483.79 ms] | 4.01 ms [4.01 ms, 4.03 ms] | 38.1 [38.0, 38.2] | 0.554 [0.539, 0.556] | same |
| explicit_plastic | residual | base-t4 | 3 | 120 | 329.75 ms [329.52 ms, 331.29 ms] | 2.75 ms [2.75 ms, 2.76 ms] | 26.1 [26.0, 26.2] | 0.381 [0.367, 0.381] | same |
| explicit_plastic | residual | head-t2 | 3 | 120 | 471.09 ms [466.95 ms, 471.61 ms] | 3.93 ms [3.89 ms, 3.93 ms] | 37.2 [36.9, 37.3] | 0.537 [0.525, 0.544] | same |
| explicit_plastic | residual | head-t4 | 3 | 120 | 316.85 ms [313.98 ms, 318.02 ms] | 2.64 ms [2.62 ms, 2.65 ms] | 25.0 [24.8, 25.1] | 0.362 [0.353, 0.366] | same |
| explicit_plastic | lumped_matrix | base-t1 | 3 | 120 | 27.3 µs [24.1 µs, 30.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | lumped_matrix | base-t2 | 3 | 120 | 31.6 µs [25.2 µs, 32.4 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.080 [1.043, 1.158] | same |
| explicit_plastic | lumped_matrix | base-t4 | 3 | 120 | 30.6 µs [28.9 µs, 31.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.134 [0.965, 1.270] | same |
| explicit_plastic | lumped_matrix | head-t2 | 3 | 120 | 32.5 µs [27.9 µs, 32.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.085 [1.021, 1.361] | same |
| explicit_plastic | lumped_matrix | head-t4 | 3 | 120 | 31.4 µs [31.4 µs, 32.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.190 [1.048, 1.303] | same |
| explicit_plastic | corrector | base-t1 | 3 | 120 | 13.0 µs [12.4 µs, 13.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_plastic | corrector | base-t2 | 3 | 120 | 22.0 µs [21.4 µs, 28.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.772 [1.639, 2.069] | same |
| explicit_plastic | corrector | base-t4 | 3 | 120 | 23.0 µs [19.8 µs, 29.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.763 [1.591, 2.190] | same |
| explicit_plastic | corrector | head-t2 | 3 | 120 | 22.4 µs [22.4 µs, 26.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.800 [1.716, 1.950] | same |
| explicit_plastic | corrector | head-t4 | 3 | 120 | 22.1 µs [20.0 µs, 25.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.776 [1.480, 1.940] | same |
| explicit_plastic | step_hooks | base-t1 | 3 | 240 | 83.29 ms [83.17 ms, 84.17 ms] | 347.0 µs [346.6 µs, 350.7 µs] | 3.3 [3.3, 3.3] | – | same |
| explicit_plastic | step_hooks | base-t2 | 3 | 240 | 78.85 ms [78.65 ms, 79.13 ms] | 328.5 µs [327.7 µs, 329.7 µs] | 3.1 [3.1, 3.1] | 0.946 [0.940, 0.947] | same |
| explicit_plastic | step_hooks | base-t4 | 3 | 240 | 64.86 ms [63.01 ms, 64.91 ms] | 270.2 µs [262.5 µs, 270.5 µs] | 2.6 [2.5, 2.6] | 0.779 [0.749, 0.780] | same |
| explicit_plastic | step_hooks | head-t2 | 3 | 240 | 78.69 ms [77.84 ms, 78.88 ms] | 327.9 µs [324.3 µs, 328.7 µs] | 3.1 [3.1, 3.1] | 0.937 [0.935, 0.946] | same |
| explicit_plastic | step_hooks | head-t4 | 3 | 240 | 63.25 ms [62.97 ms, 63.26 ms] | 263.5 µs [262.4 µs, 263.6 µs] | 2.5 [2.5, 2.5] | 0.757 [0.751, 0.759] | same |
| explicit_plastic | step_rest | base-t1 | 3 | 120 | 37.43 ms [37.42 ms, 37.79 ms] | 311.9 µs [311.8 µs, 314.9 µs] | 3.0 [3.0, 3.0] | – | same |
| explicit_plastic | step_rest | base-t2 | 3 | 120 | 43.18 ms [42.91 ms, 43.24 ms] | 359.8 µs [357.6 µs, 360.4 µs] | 3.4 [3.4, 3.4] | 1.154 [1.136, 1.156] | same |
| explicit_plastic | step_rest | base-t4 | 3 | 120 | 43.54 ms [41.36 ms, 43.79 ms] | 362.8 µs [344.7 µs, 364.9 µs] | 3.4 [3.3, 3.5] | 1.164 [1.095, 1.170] | same |
| explicit_plastic | step_rest | head-t2 | 3 | 120 | 32.23 ms [31.72 ms, 32.44 ms] | 268.6 µs [264.3 µs, 270.3 µs] | 2.5 [2.5, 2.6] | 0.861 [0.839, 0.867] | same |
| explicit_plastic | step_rest | head-t4 | 3 | 120 | 27.64 ms [27.55 ms, 27.94 ms] | 230.3 µs [229.6 µs, 232.8 µs] | 2.2 [2.2, 2.2] | 0.736 [0.731, 0.747] | same |
| explicit_plastic | internal_forces | base-t1 | 3 | 24 | 171.47 ms [169.94 ms, 171.60 ms] | 7.14 ms [7.08 ms, 7.15 ms] | 67.7 [67.1, 67.8] | – | same |
| explicit_plastic | internal_forces | base-t2 | 3 | 24 | 92.81 ms [92.14 ms, 94.00 ms] | 3.87 ms [3.84 ms, 3.92 ms] | 36.7 [36.4, 37.1] | 0.541 [0.537, 0.553] | same |
| explicit_plastic | internal_forces | base-t4 | 3 | 24 | 52.19 ms [52.04 ms, 54.22 ms] | 2.17 ms [2.17 ms, 2.26 ms] | 20.6 [20.6, 21.4] | 0.306 [0.304, 0.316] | same |
| explicit_plastic | internal_forces | head-t2 | 3 | 24 | 90.99 ms [90.48 ms, 94.54 ms] | 3.79 ms [3.77 ms, 3.94 ms] | 35.9 [35.7, 37.4] | 0.532 [0.531, 0.551] | same |
| explicit_plastic | internal_forces | head-t4 | 3 | 24 | 52.21 ms [50.94 ms, 52.47 ms] | 2.18 ms [2.12 ms, 2.19 ms] | 20.6 [20.1, 20.7] | 0.304 [0.300, 0.306] | same |
| explicit_plastic | stresses | base-t1 | 3 | 24 | 122.87 ms [122.04 ms, 123.00 ms] | 5.12 ms [5.09 ms, 5.12 ms] | 48.5 [48.2, 48.6] | – | same |
| explicit_plastic | stresses | base-t2 | 3 | 24 | 111.68 ms [111.35 ms, 111.74 ms] | 4.65 ms [4.64 ms, 4.66 ms] | 44.1 [44.0, 44.1] | 0.909 [0.905, 0.915] | same |
| explicit_plastic | stresses | base-t4 | 3 | 24 | 106.26 ms [106.03 ms, 106.32 ms] | 4.43 ms [4.42 ms, 4.43 ms] | 42.0 [41.9, 42.0] | 0.865 [0.864, 0.869] | same |
| explicit_plastic | stresses | head-t2 | 3 | 24 | 111.15 ms [110.15 ms, 111.30 ms] | 4.63 ms [4.59 ms, 4.64 ms] | 43.9 [43.5, 44.0] | 0.904 [0.896, 0.912] | same |
| explicit_plastic | stresses | head-t4 | 3 | 24 | 105.09 ms [104.04 ms, 105.93 ms] | 4.38 ms [4.34 ms, 4.41 ms] | 41.5 [41.1, 41.9] | 0.855 [0.852, 0.861] | same |
| explicit_plastic | energies | base-t1 | 3 | 24 | 42.09 ms [41.92 ms, 42.23 ms] | 1.75 ms [1.75 ms, 1.76 ms] | 16.6 [16.6, 16.7] | – | same |
| explicit_plastic | energies | base-t2 | 3 | 24 | 42.17 ms [41.74 ms, 42.82 ms] | 1.76 ms [1.74 ms, 1.78 ms] | 16.7 [16.5, 16.9] | 1.006 [0.992, 1.014] | same |
| explicit_plastic | energies | base-t4 | 3 | 24 | 41.97 ms [41.84 ms, 42.10 ms] | 1.75 ms [1.74 ms, 1.75 ms] | 16.6 [16.5, 16.6] | 0.994 [0.994, 1.004] | same |
| explicit_plastic | energies | head-t2 | 3 | 24 | 42.04 ms [41.76 ms, 42.32 ms] | 1.75 ms [1.74 ms, 1.76 ms] | 16.6 [16.5, 16.7] | 0.995 [0.992, 1.009] | same |
| explicit_plastic | energies | head-t4 | 3 | 24 | 42.03 ms [41.49 ms, 42.20 ms] | 1.75 ms [1.73 ms, 1.76 ms] | 16.6 [16.4, 16.7] | 0.999 [0.986, 1.003] | same |
| explicit_sls | mesh | base-t1 | 3 | 1 | 2.11 ms [2.09 ms, 2.25 ms] | 2.11 ms [2.09 ms, 2.25 ms] | 20.0 [19.9, 21.3] | – | same |
| explicit_sls | mesh | base-t2 | 3 | 1 | 2.10 ms [2.10 ms, 2.18 ms] | 2.10 ms [2.10 ms, 2.18 ms] | 19.9 [19.9, 20.7] | 0.994 [0.971, 1.002] | same |
| explicit_sls | mesh | base-t4 | 3 | 1 | 2.09 ms [2.06 ms, 2.13 ms] | 2.09 ms [2.06 ms, 2.13 ms] | 19.8 [19.5, 20.2] | 0.983 [0.948, 0.990] | same |
| explicit_sls | mesh | head-t2 | 3 | 1 | 2.04 ms [2.03 ms, 2.06 ms] | 2.04 ms [2.03 ms, 2.06 ms] | 19.4 [19.2, 19.5] | 0.963 [0.909, 0.985] | same |
| explicit_sls | mesh | head-t4 | 3 | 1 | 2.08 ms [2.04 ms, 2.10 ms] | 2.08 ms [2.04 ms, 2.10 ms] | 19.7 [19.3, 19.9] | 0.985 [0.905, 1.003] | same |
| explicit_sls | init | base-t1 | 3 | 1 | 37.36 ms [36.19 ms, 37.39 ms] | 37.36 ms [36.19 ms, 37.39 ms] | 354.2 [343.2, 354.5] | – | same |
| explicit_sls | init | base-t2 | 3 | 1 | 37.51 ms [37.50 ms, 37.54 ms] | 37.51 ms [37.50 ms, 37.54 ms] | 355.7 [355.6, 356.0] | 1.004 [1.004, 1.036] | same |
| explicit_sls | init | base-t4 | 3 | 1 | 37.02 ms [36.81 ms, 37.53 ms] | 37.02 ms [36.81 ms, 37.53 ms] | 351.1 [349.1, 355.9] | 1.004 [0.985, 1.023] | same |
| explicit_sls | init | head-t2 | 3 | 1 | 37.25 ms [36.72 ms, 39.30 ms] | 37.25 ms [36.72 ms, 39.30 ms] | 353.2 [348.2, 372.7] | 1.029 [0.982, 1.052] | same |
| explicit_sls | init | head-t4 | 3 | 1 | 37.40 ms [36.79 ms, 38.69 ms] | 37.40 ms [36.79 ms, 38.69 ms] | 354.6 [348.9, 366.9] | 1.033 [0.984, 1.036] | same |
| explicit_sls | stable_time_step | base-t1 | 3 | 5 | 21.17 ms [21.09 ms, 21.20 ms] | 4.23 ms [4.22 ms, 4.24 ms] | 40.2 [40.0, 40.2] | – | same |
| explicit_sls | stable_time_step | base-t2 | 3 | 5 | 21.20 ms [21.17 ms, 21.23 ms] | 4.24 ms [4.23 ms, 4.25 ms] | 40.2 [40.2, 40.3] | 1.000 [1.000, 1.007] | same |
| explicit_sls | stable_time_step | base-t4 | 3 | 5 | 21.14 ms [21.12 ms, 21.16 ms] | 4.23 ms [4.22 ms, 4.23 ms] | 40.1 [40.1, 40.1] | 0.998 [0.998, 1.003] | same |
| explicit_sls | stable_time_step | head-t2 | 3 | 5 | 21.20 ms [21.12 ms, 21.49 ms] | 4.24 ms [4.22 ms, 4.30 ms] | 40.2 [40.1, 40.8] | 1.005 [0.998, 1.014] | same |
| explicit_sls | stable_time_step | head-t4 | 3 | 5 | 21.12 ms [21.08 ms, 21.20 ms] | 4.22 ms [4.22 ms, 4.24 ms] | 40.0 [40.0, 40.2] | 1.001 [0.994, 1.001] | same |
| explicit_sls | solve_step | base-t1 | 3 | 120 | 761.99 ms [760.14 ms, 822.45 ms] | 6.35 ms [6.33 ms, 6.85 ms] | 60.2 [60.1, 65.0] | – | same |
| explicit_sls | solve_step | base-t2 | 3 | 120 | 433.06 ms [419.88 ms, 434.27 ms] | 3.61 ms [3.50 ms, 3.62 ms] | 34.2 [33.2, 34.3] | 0.568 [0.511, 0.571] | same |
| explicit_sls | solve_step | base-t4 | 3 | 120 | 341.83 ms [341.32 ms, 343.58 ms] | 2.85 ms [2.84 ms, 2.86 ms] | 27.0 [27.0, 27.2] | 0.449 [0.418, 0.449] | same |
| explicit_sls | solve_step | head-t2 | 3 | 120 | 415.97 ms [399.42 ms, 418.09 ms] | 3.47 ms [3.33 ms, 3.48 ms] | 32.9 [31.6, 33.0] | 0.547 [0.486, 0.549] | same |
| explicit_sls | solve_step | head-t4 | 3 | 120 | 284.08 ms [273.59 ms, 311.87 ms] | 2.37 ms [2.28 ms, 2.60 ms] | 22.4 [21.6, 24.6] | 0.359 [0.345, 0.410] | same |
| explicit_sls | predictor | base-t1 | 3 | 120 | 9.8 µs [9.3 µs, 11.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | predictor | base-t2 | 3 | 120 | 12.6 µs [9.8 µs, 16.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.088 [0.995, 1.737] | same |
| explicit_sls | predictor | base-t4 | 3 | 120 | 11.7 µs [9.5 µs, 12.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.191 [0.823, 1.323] | same |
| explicit_sls | predictor | head-t2 | 3 | 120 | 3.9 µs [3.6 µs, 4.2 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.362 [0.334, 0.448] | same |
| explicit_sls | predictor | head-t4 | 3 | 120 | 3.9 µs [3.5 µs, 4.5 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.421 [0.300, 0.454] | same |
| explicit_sls | residual | base-t1 | 3 | 120 | 719.50 ms [718.46 ms, 785.24 ms] | 6.00 ms [5.99 ms, 6.54 ms] | 56.9 [56.8, 62.1] | – | same |
| explicit_sls | residual | base-t2 | 3 | 120 | 391.60 ms [382.76 ms, 392.70 ms] | 3.26 ms [3.19 ms, 3.27 ms] | 30.9 [30.2, 31.0] | 0.544 [0.487, 0.547] | same |
| explicit_sls | residual | base-t4 | 3 | 120 | 299.37 ms [298.88 ms, 300.59 ms] | 2.49 ms [2.49 ms, 2.50 ms] | 23.7 [23.6, 23.8] | 0.416 [0.383, 0.416] | same |
| explicit_sls | residual | head-t2 | 3 | 120 | 384.93 ms [371.85 ms, 386.69 ms] | 3.21 ms [3.10 ms, 3.22 ms] | 30.4 [29.4, 30.6] | 0.536 [0.474, 0.537] | same |
| explicit_sls | residual | head-t4 | 3 | 120 | 261.33 ms [252.45 ms, 285.46 ms] | 2.18 ms [2.10 ms, 2.38 ms] | 20.7 [19.9, 22.6] | 0.351 [0.333, 0.397] | same |
| explicit_sls | lumped_matrix | base-t1 | 3 | 120 | 25.9 µs [25.9 µs, 28.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | lumped_matrix | base-t2 | 3 | 120 | 30.8 µs [28.7 µs, 35.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.189 [1.108, 1.249] | same |
| explicit_sls | lumped_matrix | base-t4 | 3 | 120 | 27.0 µs [25.8 µs, 29.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.043 [0.921, 1.123] | same |
| explicit_sls | lumped_matrix | head-t2 | 3 | 120 | 32.7 µs [29.8 µs, 35.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.169 [1.153, 1.376] | same |
| explicit_sls | lumped_matrix | head-t4 | 3 | 120 | 29.1 µs [25.2 µs, 35.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.125 [0.900, 1.387] | same |
| explicit_sls | corrector | base-t1 | 3 | 120 | 31.8 µs [11.3 µs, 33.2 µs] | 0.3 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | corrector | base-t2 | 3 | 120 | 20.2 µs [14.3 µs, 24.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.723 [0.638, 1.267] | same |
| explicit_sls | corrector | base-t4 | 3 | 120 | 23.0 µs [21.5 µs, 24.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.785 [0.647, 2.030] | same |
| explicit_sls | corrector | head-t2 | 3 | 120 | 22.5 µs [13.2 µs, 30.3 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.954 [0.678, 1.164] | same |
| explicit_sls | corrector | head-t4 | 3 | 120 | 12.8 µs [6.6 µs, 19.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.596 [0.208, 1.131] | same |
| explicit_sls | step_hooks | base-t1 | 3 | 240 | 449.3 µs [412.6 µs, 474.8 µs] | 1.9 µs [1.7 µs, 2.0 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_sls | step_hooks | base-t2 | 3 | 240 | 420.9 µs [398.9 µs, 436.0 µs] | 1.8 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 0.967 [0.886, 0.970] | same |
| explicit_sls | step_hooks | base-t4 | 3 | 240 | 416.8 µs [415.5 µs, 433.5 µs] | 1.7 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 0.965 [0.878, 1.007] | same |
| explicit_sls | step_hooks | head-t2 | 3 | 240 | 456.7 µs [454.8 µs, 466.2 µs] | 1.9 µs [1.9 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.017 [0.982, 1.102] | same |
| explicit_sls | step_hooks | head-t4 | 3 | 240 | 404.4 µs [400.2 µs, 498.2 µs] | 1.7 µs [1.7 µs, 2.1 µs] | 0.0 [0.0, 0.0] | 0.980 [0.891, 1.049] | same |
| explicit_sls | step_rest | base-t1 | 3 | 120 | 40.87 ms [36.50 ms, 41.70 ms] | 340.6 µs [304.2 µs, 347.5 µs] | 3.2 [2.9, 3.3] | – | same |
| explicit_sls | step_rest | base-t2 | 3 | 120 | 40.69 ms [36.42 ms, 40.76 ms] | 339.0 µs [303.5 µs, 339.6 µs] | 3.2 [2.9, 3.2] | 0.997 [0.976, 0.998] | same |
| explicit_sls | step_rest | base-t4 | 3 | 120 | 41.67 ms [41.65 ms, 42.19 ms] | 347.2 µs [347.1 µs, 351.6 µs] | 3.3 [3.3, 3.3] | 1.020 [0.999, 1.156] | same |
| explicit_sls | step_rest | head-t2 | 3 | 120 | 30.21 ms [26.80 ms, 30.59 ms] | 251.8 µs [223.3 µs, 254.9 µs] | 2.4 [2.1, 2.4] | 0.734 [0.734, 0.739] | same |
| explicit_sls | step_rest | head-t4 | 3 | 120 | 22.03 ms [20.45 ms, 25.57 ms] | 183.6 µs [170.4 µs, 213.1 µs] | 1.7 [1.6, 2.0] | 0.604 [0.490, 0.626] | same |
| explicit_sls | internal_forces | base-t1 | 3 | 24 | 136.11 ms [134.34 ms, 136.28 ms] | 5.67 ms [5.60 ms, 5.68 ms] | 53.8 [53.1, 53.8] | – | same |
| explicit_sls | internal_forces | base-t2 | 3 | 24 | 71.85 ms [70.97 ms, 72.14 ms] | 2.99 ms [2.96 ms, 3.01 ms] | 28.4 [28.0, 28.5] | 0.528 [0.527, 0.530] | same |
| explicit_sls | internal_forces | base-t4 | 3 | 24 | 45.95 ms [45.40 ms, 46.99 ms] | 1.91 ms [1.89 ms, 1.96 ms] | 18.2 [17.9, 18.6] | 0.338 [0.337, 0.345] | same |
| explicit_sls | internal_forces | head-t2 | 3 | 24 | 74.55 ms [69.64 ms, 74.58 ms] | 3.11 ms [2.90 ms, 3.11 ms] | 29.5 [27.5, 29.5] | 0.547 [0.518, 0.548] | same |
| explicit_sls | internal_forces | head-t4 | 3 | 24 | 43.74 ms [43.30 ms, 45.99 ms] | 1.82 ms [1.80 ms, 1.92 ms] | 17.3 [17.1, 18.2] | 0.326 [0.318, 0.338] | same |
| explicit_sls | stresses | base-t1 | 3 | 24 | 86.82 ms [85.78 ms, 86.94 ms] | 3.62 ms [3.57 ms, 3.62 ms] | 34.3 [33.9, 34.4] | – | same |
| explicit_sls | stresses | base-t2 | 3 | 24 | 76.10 ms [75.76 ms, 76.12 ms] | 3.17 ms [3.16 ms, 3.17 ms] | 30.1 [29.9, 30.1] | 0.875 [0.873, 0.887] | same |
| explicit_sls | stresses | base-t4 | 3 | 24 | 70.55 ms [69.88 ms, 70.59 ms] | 2.94 ms [2.91 ms, 2.94 ms] | 27.9 [27.6, 27.9] | 0.813 [0.804, 0.823] | same |
| explicit_sls | stresses | head-t2 | 3 | 24 | 76.02 ms [75.46 ms, 76.27 ms] | 3.17 ms [3.14 ms, 3.18 ms] | 30.0 [29.8, 30.1] | 0.878 [0.874, 0.880] | same |
| explicit_sls | stresses | head-t4 | 3 | 24 | 70.41 ms [70.15 ms, 70.61 ms] | 2.93 ms [2.92 ms, 2.94 ms] | 27.8 [27.7, 27.9] | 0.813 [0.807, 0.821] | same |
| explicit_sls | energies | base-t1 | 3 | 24 | 67.50 ms [67.27 ms, 67.67 ms] | 2.81 ms [2.80 ms, 2.82 ms] | 26.7 [26.6, 26.7] | – | same |
| explicit_sls | energies | base-t2 | 3 | 24 | 67.51 ms [67.49 ms, 67.57 ms] | 2.81 ms [2.81 ms, 2.82 ms] | 26.7 [26.7, 26.7] | 1.001 [0.997, 1.004] | same |
| explicit_sls | energies | base-t4 | 3 | 24 | 67.42 ms [67.38 ms, 67.53 ms] | 2.81 ms [2.81 ms, 2.81 ms] | 26.6 [26.6, 26.7] | 1.000 [0.996, 1.002] | same |
| explicit_sls | energies | head-t2 | 3 | 24 | 67.56 ms [67.50 ms, 67.97 ms] | 2.81 ms [2.81 ms, 2.83 ms] | 26.7 [26.7, 26.9] | 1.004 [0.997, 1.007] | same |
| explicit_sls | energies | head-t4 | 3 | 24 | 67.62 ms [67.58 ms, 67.76 ms] | 2.82 ms [2.82 ms, 2.82 ms] | 26.7 [26.7, 26.8] | 1.002 [0.999, 1.007] | same |
| explicit_tet10_fd | mesh | base-t1 | 3 | 1 | 6.98 ms [6.87 ms, 7.02 ms] | 6.98 ms [6.87 ms, 7.02 ms] | 66.2 [65.1, 66.6] | – | same |
| explicit_tet10_fd | mesh | base-t2 | 3 | 1 | 6.96 ms [6.95 ms, 7.05 ms] | 6.96 ms [6.95 ms, 7.05 ms] | 66.0 [65.9, 66.8] | 1.009 [0.991, 1.013] | same |
| explicit_tet10_fd | mesh | base-t4 | 3 | 1 | 6.88 ms [6.75 ms, 7.06 ms] | 6.88 ms [6.75 ms, 7.06 ms] | 65.3 [64.0, 67.0] | 0.985 [0.961, 1.029] | same |
| explicit_tet10_fd | mesh | head-t2 | 3 | 1 | 6.72 ms [6.56 ms, 6.86 ms] | 6.72 ms [6.56 ms, 6.86 ms] | 63.7 [62.2, 65.0] | 0.977 [0.940, 0.978] | same |
| explicit_tet10_fd | mesh | head-t4 | 3 | 1 | 6.74 ms [6.59 ms, 6.82 ms] | 6.74 ms [6.59 ms, 6.82 ms] | 63.9 [62.5, 64.7] | 0.965 [0.960, 0.972] | same |
| explicit_tet10_fd | init | base-t1 | 3 | 1 | 188.61 ms [188.57 ms, 190.98 ms] | 188.61 ms [188.57 ms, 190.98 ms] | 1788.5 [1788.1, 1810.9] | – | same |
| explicit_tet10_fd | init | base-t2 | 3 | 1 | 192.85 ms [188.63 ms, 193.27 ms] | 192.85 ms [188.63 ms, 193.27 ms] | 1828.7 [1788.7, 1832.7] | 1.012 [1.000, 1.022] | same |
| explicit_tet10_fd | init | base-t4 | 3 | 1 | 189.93 ms [189.59 ms, 190.71 ms] | 189.93 ms [189.59 ms, 190.71 ms] | 1801.0 [1797.8, 1808.5] | 1.007 [0.993, 1.011] | same |
| explicit_tet10_fd | init | head-t2 | 3 | 1 | 191.42 ms [190.21 ms, 192.02 ms] | 191.42 ms [190.21 ms, 192.02 ms] | 1815.1 [1803.7, 1820.8] | 1.009 [1.005, 1.015] | same |
| explicit_tet10_fd | init | head-t4 | 3 | 1 | 190.81 ms [189.66 ms, 192.16 ms] | 190.81 ms [189.66 ms, 192.16 ms] | 1809.4 [1798.5, 1822.1] | 1.012 [0.993, 1.019] | same |
| explicit_tet10_fd | stable_time_step | base-t1 | 3 | 5 | 62.51 ms [61.85 ms, 62.63 ms] | 12.50 ms [12.37 ms, 12.53 ms] | 118.5 [117.3, 118.8] | – | same |
| explicit_tet10_fd | stable_time_step | base-t2 | 3 | 5 | 63.03 ms [62.57 ms, 63.22 ms] | 12.61 ms [12.51 ms, 12.64 ms] | 119.5 [118.7, 119.9] | 1.009 [1.001, 1.019] | same |
| explicit_tet10_fd | stable_time_step | base-t4 | 3 | 5 | 62.47 ms [61.68 ms, 62.90 ms] | 12.49 ms [12.34 ms, 12.58 ms] | 118.5 [117.0, 119.3] | 0.999 [0.985, 1.017] | same |
| explicit_tet10_fd | stable_time_step | head-t2 | 3 | 5 | 62.31 ms [61.70 ms, 62.47 ms] | 12.46 ms [12.34 ms, 12.49 ms] | 118.2 [117.0, 118.5] | 0.997 [0.987, 1.007] | same |
| explicit_tet10_fd | stable_time_step | head-t4 | 3 | 5 | 62.79 ms [62.67 ms, 62.93 ms] | 12.56 ms [12.53 ms, 12.59 ms] | 119.1 [118.9, 119.3] | 1.003 [1.003, 1.017] | same |
| explicit_tet10_fd | solve_step | base-t1 | 3 | 40 | 1.129 s [1.127 s, 1.130 s] | 28.22 ms [28.17 ms, 28.25 ms] | 267.6 [267.1, 267.9] | – | same |
| explicit_tet10_fd | solve_step | base-t2 | 3 | 40 | 732.11 ms [722.24 ms, 733.08 ms] | 18.30 ms [18.06 ms, 18.33 ms] | 173.6 [171.2, 173.8] | 0.648 [0.641, 0.650] | same |
| explicit_tet10_fd | solve_step | base-t4 | 3 | 40 | 451.63 ms [447.54 ms, 451.95 ms] | 11.29 ms [11.19 ms, 11.30 ms] | 107.1 [106.1, 107.1] | 0.400 [0.396, 0.401] | same |
| explicit_tet10_fd | solve_step | head-t2 | 3 | 40 | 682.57 ms [679.15 ms, 684.71 ms] | 17.06 ms [16.98 ms, 17.12 ms] | 161.8 [161.0, 162.3] | 0.604 [0.603, 0.607] | same |
| explicit_tet10_fd | solve_step | head-t4 | 3 | 40 | 389.98 ms [388.89 ms, 392.59 ms] | 9.75 ms [9.72 ms, 9.81 ms] | 92.5 [92.2, 93.1] | 0.346 [0.344, 0.348] | same |
| explicit_tet10_fd | predictor | base-t1 | 3 | 40 | 8.6 µs [7.1 µs, 9.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | predictor | base-t2 | 3 | 40 | 6.8 µs [6.2 µs, 8.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.869 [0.789, 0.940] | same |
| explicit_tet10_fd | predictor | base-t4 | 3 | 40 | 7.7 µs [6.8 µs, 8.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.902 [0.755, 1.167] | same |
| explicit_tet10_fd | predictor | head-t2 | 3 | 40 | 6.0 µs [6.0 µs, 6.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.781 [0.669, 0.846] | same |
| explicit_tet10_fd | predictor | head-t4 | 3 | 40 | 6.8 µs [6.1 µs, 7.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.895 [0.678, 0.953] | same |
| explicit_tet10_fd | residual | base-t1 | 3 | 40 | 1.029 s [1.027 s, 1.029 s] | 25.72 ms [25.67 ms, 25.74 ms] | 243.8 [243.4, 244.0] | – | same |
| explicit_tet10_fd | residual | base-t2 | 3 | 40 | 631.42 ms [622.00 ms, 632.29 ms] | 15.79 ms [15.55 ms, 15.81 ms] | 149.7 [147.5, 149.9] | 0.613 [0.606, 0.615] | same |
| explicit_tet10_fd | residual | base-t4 | 3 | 40 | 349.86 ms [346.97 ms, 350.87 ms] | 8.75 ms [8.67 ms, 8.77 ms] | 82.9 [82.3, 83.2] | 0.341 [0.337, 0.341] | same |
| explicit_tet10_fd | residual | head-t2 | 3 | 40 | 610.42 ms [607.37 ms, 612.51 ms] | 15.26 ms [15.18 ms, 15.31 ms] | 144.7 [144.0, 145.2] | 0.593 [0.591, 0.595] | same |
| explicit_tet10_fd | residual | head-t4 | 3 | 40 | 326.38 ms [325.77 ms, 329.12 ms] | 8.16 ms [8.14 ms, 8.23 ms] | 77.4 [77.2, 78.0] | 0.318 [0.316, 0.320] | same |
| explicit_tet10_fd | lumped_matrix | base-t1 | 3 | 40 | 13.3 µs [13.0 µs, 13.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | lumped_matrix | base-t2 | 3 | 40 | 19.9 µs [14.8 µs, 20.6 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.512 [1.117, 1.526] | same |
| explicit_tet10_fd | lumped_matrix | base-t4 | 3 | 40 | 18.1 µs [17.9 µs, 19.4 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.379 [1.366, 1.424] | same |
| explicit_tet10_fd | lumped_matrix | head-t2 | 3 | 40 | 17.1 µs [16.3 µs, 18.5 µs] | 0.4 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.289 [1.256, 1.361] | same |
| explicit_tet10_fd | lumped_matrix | head-t4 | 3 | 40 | 12.8 µs [12.4 µs, 15.6 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.969 [0.914, 1.198] | same |
| explicit_tet10_fd | corrector | base-t1 | 3 | 40 | 10.2 µs [9.7 µs, 10.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | corrector | base-t2 | 3 | 40 | 10.4 µs [10.1 µs, 10.5 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.989 [0.958, 1.070] | same |
| explicit_tet10_fd | corrector | base-t4 | 3 | 40 | 10.9 µs [10.9 µs, 11.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.072 [1.002, 1.120] | same |
| explicit_tet10_fd | corrector | head-t2 | 3 | 40 | 10.3 µs [10.3 µs, 10.4 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.008 [0.948, 1.060] | same |
| explicit_tet10_fd | corrector | head-t4 | 3 | 40 | 10.5 µs [9.6 µs, 10.6 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.983 [0.959, 1.038] | same |
| explicit_tet10_fd | step_hooks | base-t1 | 3 | 80 | 151.0 µs [146.1 µs, 151.1 µs] | 1.9 µs [1.8 µs, 1.9 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_fd | step_hooks | base-t2 | 3 | 80 | 152.1 µs [149.8 µs, 154.8 µs] | 1.9 µs [1.9 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.025 [0.991, 1.041] | same |
| explicit_tet10_fd | step_hooks | base-t4 | 3 | 80 | 155.0 µs [149.1 µs, 156.2 µs] | 1.9 µs [1.9 µs, 2.0 µs] | 0.0 [0.0, 0.0] | 1.033 [0.987, 1.061] | same |
| explicit_tet10_fd | step_hooks | head-t2 | 3 | 80 | 173.2 µs [167.9 µs, 178.3 µs] | 2.2 µs [2.1 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 1.149 [1.147, 1.180] | same |
| explicit_tet10_fd | step_hooks | head-t4 | 3 | 80 | 176.4 µs [169.9 µs, 186.3 µs] | 2.2 µs [2.1 µs, 2.3 µs] | 0.0 [0.0, 0.0] | 1.207 [1.124, 1.233] | same |
| explicit_tet10_fd | step_rest | base-t1 | 3 | 40 | 99.88 ms [99.77 ms, 100.35 ms] | 2.50 ms [2.49 ms, 2.51 ms] | 23.7 [23.7, 23.8] | – | same |
| explicit_tet10_fd | step_rest | base-t2 | 3 | 40 | 100.50 ms [100.04 ms, 100.59 ms] | 2.51 ms [2.50 ms, 2.51 ms] | 23.8 [23.7, 23.8] | 1.003 [1.002, 1.007] | same |
| explicit_tet10_fd | step_rest | base-t4 | 3 | 40 | 100.88 ms [100.38 ms, 101.57 ms] | 2.52 ms [2.51 ms, 2.54 ms] | 23.9 [23.8, 24.1] | 1.010 [1.000, 1.018] | same |
| explicit_tet10_fd | step_rest | head-t2 | 3 | 40 | 71.94 ms [71.56 ms, 71.99 ms] | 1.80 ms [1.79 ms, 1.80 ms] | 17.1 [17.0, 17.1] | 0.717 [0.717, 0.721] | same |
| explicit_tet10_fd | step_rest | head-t4 | 3 | 40 | 63.25 ms [62.89 ms, 63.39 ms] | 1.58 ms [1.57 ms, 1.58 ms] | 15.0 [14.9, 15.0] | 0.633 [0.627, 0.635] | same |
| explicit_tet10_fd | internal_forces | base-t1 | 3 | 8 | 197.84 ms [197.21 ms, 198.73 ms] | 24.73 ms [24.65 ms, 24.84 ms] | 234.5 [233.8, 235.6] | – | same |
| explicit_tet10_fd | internal_forces | base-t2 | 3 | 8 | 113.73 ms [112.26 ms, 114.23 ms] | 14.22 ms [14.03 ms, 14.28 ms] | 134.8 [133.1, 135.4] | 0.572 [0.569, 0.577] | same |
| explicit_tet10_fd | internal_forces | base-t4 | 3 | 8 | 59.91 ms [58.68 ms, 59.91 ms] | 7.49 ms [7.33 ms, 7.49 ms] | 71.0 [69.6, 71.0] | 0.303 [0.295, 0.304] | same |
| explicit_tet10_fd | internal_forces | head-t2 | 3 | 8 | 112.67 ms [111.98 ms, 112.72 ms] | 14.08 ms [14.00 ms, 14.09 ms] | 133.5 [132.7, 133.6] | 0.568 [0.567, 0.570] | same |
| explicit_tet10_fd | internal_forces | head-t4 | 3 | 8 | 57.35 ms [57.05 ms, 57.48 ms] | 7.17 ms [7.13 ms, 7.19 ms] | 68.0 [67.6, 68.1] | 0.291 [0.287, 0.291] | same |
| explicit_tet10_fd | stresses | base-t1 | 3 | 8 | 88.45 ms [87.84 ms, 88.78 ms] | 11.06 ms [10.98 ms, 11.10 ms] | 104.8 [104.1, 105.2] | – | same |
| explicit_tet10_fd | stresses | base-t2 | 3 | 8 | 61.41 ms [58.61 ms, 62.13 ms] | 7.68 ms [7.33 ms, 7.77 ms] | 72.8 [69.5, 73.6] | 0.694 [0.660, 0.707] | same |
| explicit_tet10_fd | stresses | base-t4 | 3 | 8 | 46.31 ms [45.50 ms, 46.34 ms] | 5.79 ms [5.69 ms, 5.79 ms] | 54.9 [53.9, 54.9] | 0.524 [0.512, 0.527] | same |
| explicit_tet10_fd | stresses | head-t2 | 3 | 8 | 62.12 ms [61.29 ms, 62.96 ms] | 7.77 ms [7.66 ms, 7.87 ms] | 73.6 [72.6, 74.6] | 0.700 [0.693, 0.717] | same |
| explicit_tet10_fd | stresses | head-t4 | 3 | 8 | 45.76 ms [45.47 ms, 45.83 ms] | 5.72 ms [5.68 ms, 5.73 ms] | 54.2 [53.9, 54.3] | 0.517 [0.512, 0.522] | same |
| explicit_tet10_fd | energies | base-t1 | 3 | 8 | 36.56 ms [36.52 ms, 36.75 ms] | 4.57 ms [4.56 ms, 4.59 ms] | 43.3 [43.3, 43.6] | – | same |
| explicit_tet10_fd | energies | base-t2 | 3 | 8 | 36.50 ms [36.47 ms, 36.57 ms] | 4.56 ms [4.56 ms, 4.57 ms] | 43.3 [43.2, 43.3] | 0.997 [0.995, 1.000] | same |
| explicit_tet10_fd | energies | base-t4 | 3 | 8 | 36.48 ms [36.42 ms, 36.57 ms] | 4.56 ms [4.55 ms, 4.57 ms] | 43.2 [43.2, 43.4] | 0.997 [0.995, 0.998] | same |
| explicit_tet10_fd | energies | head-t2 | 3 | 8 | 36.46 ms [36.14 ms, 36.46 ms] | 4.56 ms [4.52 ms, 4.56 ms] | 43.2 [42.8, 43.2] | 0.992 [0.990, 0.997] | same |
| explicit_tet10_fd | energies | head-t4 | 3 | 8 | 36.27 ms [36.27 ms, 36.38 ms] | 4.53 ms [4.53 ms, 4.55 ms] | 43.0 [43.0, 43.1] | 0.992 [0.987, 0.996] | same |
| explicit_tet10_ss | mesh | base-t1 | 3 | 1 | 7.03 ms [6.87 ms, 7.42 ms] | 7.03 ms [6.87 ms, 7.42 ms] | 66.7 [65.2, 70.4] | – | same |
| explicit_tet10_ss | mesh | base-t2 | 3 | 1 | 7.15 ms [7.05 ms, 7.32 ms] | 7.15 ms [7.05 ms, 7.32 ms] | 67.8 [66.9, 69.4] | 1.026 [0.963, 1.041] | same |
| explicit_tet10_ss | mesh | base-t4 | 3 | 1 | 6.93 ms [6.78 ms, 7.04 ms] | 6.93 ms [6.78 ms, 7.04 ms] | 65.7 [64.3, 66.7] | 1.000 [0.914, 1.008] | same |
| explicit_tet10_ss | mesh | head-t2 | 3 | 1 | 6.72 ms [6.72 ms, 6.75 ms] | 6.72 ms [6.72 ms, 6.75 ms] | 63.7 [63.7, 64.0] | 0.959 [0.906, 0.978] | same |
| explicit_tet10_ss | mesh | head-t4 | 3 | 1 | 6.65 ms [6.57 ms, 6.78 ms] | 6.65 ms [6.57 ms, 6.78 ms] | 63.0 [62.3, 64.3] | 0.934 [0.896, 0.986] | same |
| explicit_tet10_ss | init | base-t1 | 3 | 1 | 174.97 ms [173.17 ms, 180.50 ms] | 174.97 ms [173.17 ms, 180.50 ms] | 1659.2 [1642.1, 1711.6] | – | same |
| explicit_tet10_ss | init | base-t2 | 3 | 1 | 174.38 ms [171.91 ms, 176.65 ms] | 174.38 ms [171.91 ms, 176.65 ms] | 1653.6 [1630.1, 1675.1] | 0.982 [0.979, 1.007] | same |
| explicit_tet10_ss | init | base-t4 | 3 | 1 | 173.27 ms [172.79 ms, 175.28 ms] | 173.27 ms [172.79 ms, 175.28 ms] | 1643.0 [1638.5, 1662.2] | 1.001 [0.957, 1.002] | same |
| explicit_tet10_ss | init | head-t2 | 3 | 1 | 175.93 ms [175.17 ms, 179.15 ms] | 175.93 ms [175.17 ms, 179.15 ms] | 1668.3 [1661.0, 1698.8] | 1.012 [0.975, 1.024] | same |
| explicit_tet10_ss | init | head-t4 | 3 | 1 | 173.89 ms [173.36 ms, 176.08 ms] | 173.89 ms [173.36 ms, 176.08 ms] | 1649.0 [1643.9, 1669.7] | 0.991 [0.976, 1.004] | same |
| explicit_tet10_ss | stable_time_step | base-t1 | 3 | 5 | 62.70 ms [61.91 ms, 63.04 ms] | 12.54 ms [12.38 ms, 12.61 ms] | 118.9 [117.4, 119.6] | – | same |
| explicit_tet10_ss | stable_time_step | base-t2 | 3 | 5 | 63.01 ms [62.82 ms, 63.89 ms] | 12.60 ms [12.56 ms, 12.78 ms] | 119.5 [119.1, 121.2] | 1.018 [0.997, 1.019] | same |
| explicit_tet10_ss | stable_time_step | base-t4 | 3 | 5 | 62.09 ms [61.79 ms, 62.50 ms] | 12.42 ms [12.36 ms, 12.50 ms] | 117.8 [117.2, 118.5] | 0.997 [0.980, 1.003] | same |
| explicit_tet10_ss | stable_time_step | head-t2 | 3 | 5 | 62.25 ms [61.71 ms, 62.62 ms] | 12.45 ms [12.34 ms, 12.52 ms] | 118.1 [117.0, 118.8] | 0.993 [0.979, 1.011] | same |
| explicit_tet10_ss | stable_time_step | head-t4 | 3 | 5 | 62.42 ms [62.05 ms, 62.75 ms] | 12.48 ms [12.41 ms, 12.55 ms] | 118.4 [117.7, 119.0] | 0.996 [0.995, 1.002] | same |
| explicit_tet10_ss | solve_step | base-t1 | 3 | 50 | 996.74 ms [990.30 ms, 997.17 ms] | 19.93 ms [19.81 ms, 19.94 ms] | 189.0 [187.8, 189.1] | – | same |
| explicit_tet10_ss | solve_step | base-t2 | 3 | 50 | 712.26 ms [704.85 ms, 719.39 ms] | 14.25 ms [14.10 ms, 14.39 ms] | 135.1 [133.7, 136.4] | 0.719 [0.707, 0.721] | same |
| explicit_tet10_ss | solve_step | base-t4 | 3 | 50 | 534.54 ms [530.22 ms, 536.28 ms] | 10.69 ms [10.60 ms, 10.73 ms] | 101.4 [100.6, 101.7] | 0.536 [0.535, 0.538] | same |
| explicit_tet10_ss | solve_step | head-t2 | 3 | 50 | 655.34 ms [653.61 ms, 661.97 ms] | 13.11 ms [13.07 ms, 13.24 ms] | 124.3 [124.0, 125.5] | 0.662 [0.656, 0.664] | same |
| explicit_tet10_ss | solve_step | head-t4 | 3 | 50 | 464.02 ms [459.88 ms, 467.94 ms] | 9.28 ms [9.20 ms, 9.36 ms] | 88.0 [87.2, 88.7] | 0.465 [0.464, 0.469] | same |
| explicit_tet10_ss | predictor | base-t1 | 3 | 50 | 10.0 µs [8.8 µs, 10.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | predictor | base-t2 | 3 | 50 | 10.0 µs [9.4 µs, 11.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.953 [0.942, 1.310] | same |
| explicit_tet10_ss | predictor | base-t4 | 3 | 50 | 10.1 µs [9.1 µs, 22.1 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.013 [0.866, 2.510] | same |
| explicit_tet10_ss | predictor | head-t2 | 3 | 50 | 7.7 µs [7.3 µs, 8.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.773 [0.691, 0.940] | same |
| explicit_tet10_ss | predictor | head-t4 | 3 | 50 | 7.8 µs [6.4 µs, 8.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.739 [0.726, 0.829] | same |
| explicit_tet10_ss | residual | base-t1 | 3 | 50 | 871.15 ms [865.64 ms, 871.34 ms] | 17.42 ms [17.31 ms, 17.43 ms] | 165.2 [164.2, 165.3] | – | same |
| explicit_tet10_ss | residual | base-t2 | 3 | 50 | 585.89 ms [579.30 ms, 591.69 ms] | 11.72 ms [11.59 ms, 11.83 ms] | 111.1 [109.9, 112.2] | 0.677 [0.665, 0.679] | same |
| explicit_tet10_ss | residual | base-t4 | 3 | 50 | 408.40 ms [405.69 ms, 409.99 ms] | 8.17 ms [8.11 ms, 8.20 ms] | 77.5 [76.9, 77.8] | 0.469 [0.469, 0.471] | same |
| explicit_tet10_ss | residual | head-t2 | 3 | 50 | 565.37 ms [563.26 ms, 570.38 ms] | 11.31 ms [11.27 ms, 11.41 ms] | 107.2 [106.8, 108.2] | 0.653 [0.647, 0.655] | same |
| explicit_tet10_ss | residual | head-t4 | 3 | 50 | 385.20 ms [380.81 ms, 388.50 ms] | 7.70 ms [7.62 ms, 7.77 ms] | 73.1 [72.2, 73.7] | 0.442 [0.440, 0.446] | same |
| explicit_tet10_ss | lumped_matrix | base-t1 | 3 | 50 | 15.5 µs [12.1 µs, 17.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | lumped_matrix | base-t2 | 3 | 50 | 22.9 µs [21.9 µs, 23.5 µs] | 0.5 µs [0.4 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.415 [1.308, 1.940] | same |
| explicit_tet10_ss | lumped_matrix | base-t4 | 3 | 50 | 21.3 µs [18.9 µs, 21.4 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.376 [1.224, 1.562] | same |
| explicit_tet10_ss | lumped_matrix | head-t2 | 3 | 50 | 19.8 µs [18.3 µs, 20.4 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.319 [1.131, 1.509] | same |
| explicit_tet10_ss | lumped_matrix | head-t4 | 3 | 50 | 19.4 µs [17.2 µs, 29.1 µs] | 0.4 µs [0.3 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.604 [1.114, 1.663] | same |
| explicit_tet10_ss | corrector | base-t1 | 3 | 50 | 13.3 µs [11.0 µs, 13.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | corrector | base-t2 | 3 | 50 | 13.5 µs [13.0 µs, 14.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.055 [0.974, 1.222] | same |
| explicit_tet10_ss | corrector | base-t4 | 3 | 50 | 14.2 µs [12.5 µs, 14.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.075 [1.064, 1.135] | same |
| explicit_tet10_ss | corrector | head-t2 | 3 | 50 | 13.0 µs [12.6 µs, 13.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.000 [0.973, 1.139] | same |
| explicit_tet10_ss | corrector | head-t4 | 3 | 50 | 13.0 µs [12.9 µs, 13.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.973 [0.970, 1.182] | same |
| explicit_tet10_ss | step_hooks | base-t1 | 3 | 100 | 161.5 µs [152.0 µs, 162.8 µs] | 1.6 µs [1.5 µs, 1.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet10_ss | step_hooks | base-t2 | 3 | 100 | 172.0 µs [162.3 µs, 182.2 µs] | 1.7 µs [1.6 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.068 [1.056, 1.128] | same |
| explicit_tet10_ss | step_hooks | base-t4 | 3 | 100 | 166.6 µs [165.2 µs, 185.9 µs] | 1.7 µs [1.7 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.087 [1.023, 1.151] | same |
| explicit_tet10_ss | step_hooks | head-t2 | 3 | 100 | 167.2 µs [165.5 µs, 178.6 µs] | 1.7 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.089 [1.036, 1.097] | same |
| explicit_tet10_ss | step_hooks | head-t4 | 3 | 100 | 174.1 µs [172.5 µs, 187.3 µs] | 1.7 µs [1.7 µs, 1.9 µs] | 0.0 [0.0, 0.0] | 1.134 [1.069, 1.160] | same |
| explicit_tet10_ss | step_rest | base-t1 | 3 | 50 | 125.38 ms [124.46 ms, 125.61 ms] | 2.51 ms [2.49 ms, 2.51 ms] | 23.8 [23.6, 23.8] | – | same |
| explicit_tet10_ss | step_rest | base-t2 | 3 | 50 | 126.15 ms [125.32 ms, 127.46 ms] | 2.52 ms [2.51 ms, 2.55 ms] | 23.9 [23.8, 24.2] | 1.014 [1.000, 1.015] | same |
| explicit_tet10_ss | step_rest | base-t4 | 3 | 50 | 125.88 ms [124.31 ms, 126.07 ms] | 2.52 ms [2.49 ms, 2.52 ms] | 23.9 [23.6, 23.9] | 1.004 [0.999, 1.004] | same |
| explicit_tet10_ss | step_rest | head-t2 | 3 | 50 | 90.13 ms [89.75 ms, 91.36 ms] | 1.80 ms [1.80 ms, 1.83 ms] | 17.1 [17.0, 17.3] | 0.721 [0.719, 0.727] | same |
| explicit_tet10_ss | step_rest | head-t4 | 3 | 50 | 78.83 ms [78.58 ms, 79.19 ms] | 1.58 ms [1.57 ms, 1.58 ms] | 15.0 [14.9, 15.0] | 0.632 [0.626, 0.633] | same |
| explicit_tet10_ss | internal_forces | base-t1 | 3 | 10 | 162.76 ms [162.66 ms, 163.28 ms] | 16.28 ms [16.27 ms, 16.33 ms] | 154.3 [154.2, 154.8] | – | same |
| explicit_tet10_ss | internal_forces | base-t2 | 3 | 10 | 100.64 ms [100.24 ms, 101.86 ms] | 10.06 ms [10.02 ms, 10.19 ms] | 95.4 [95.1, 96.6] | 0.619 [0.616, 0.624] | same |
| explicit_tet10_ss | internal_forces | base-t4 | 3 | 10 | 66.65 ms [66.10 ms, 67.52 ms] | 6.67 ms [6.61 ms, 6.75 ms] | 63.2 [62.7, 64.0] | 0.408 [0.406, 0.415] | same |
| explicit_tet10_ss | internal_forces | head-t2 | 3 | 10 | 99.78 ms [99.23 ms, 100.94 ms] | 9.98 ms [9.92 ms, 10.09 ms] | 94.6 [94.1, 95.7] | 0.613 [0.610, 0.618] | same |
| explicit_tet10_ss | internal_forces | head-t4 | 3 | 10 | 64.40 ms [63.07 ms, 64.72 ms] | 6.44 ms [6.31 ms, 6.47 ms] | 61.1 [59.8, 61.4] | 0.396 [0.388, 0.396] | same |
| explicit_tet10_ss | stresses | base-t1 | 3 | 10 | 114.19 ms [114.17 ms, 114.86 ms] | 11.42 ms [11.42 ms, 11.49 ms] | 108.3 [108.3, 108.9] | – | same |
| explicit_tet10_ss | stresses | base-t2 | 3 | 10 | 69.98 ms [69.02 ms, 70.59 ms] | 7.00 ms [6.90 ms, 7.06 ms] | 66.4 [65.5, 66.9] | 0.613 [0.604, 0.615] | same |
| explicit_tet10_ss | stresses | base-t4 | 3 | 10 | 49.55 ms [48.80 ms, 50.14 ms] | 4.95 ms [4.88 ms, 5.01 ms] | 47.0 [46.3, 47.6] | 0.434 [0.427, 0.437] | same |
| explicit_tet10_ss | stresses | head-t2 | 3 | 10 | 78.34 ms [78.17 ms, 78.78 ms] | 7.83 ms [7.82 ms, 7.88 ms] | 74.3 [74.1, 74.7] | 0.686 [0.685, 0.686] | same |
| explicit_tet10_ss | stresses | head-t4 | 3 | 10 | 57.03 ms [49.43 ms, 57.39 ms] | 5.70 ms [4.94 ms, 5.74 ms] | 54.1 [46.9, 54.4] | 0.497 [0.433, 0.503] | same |
| explicit_tet10_ss | energies | base-t1 | 3 | 10 | 34.82 ms [34.12 ms, 35.05 ms] | 3.48 ms [3.41 ms, 3.50 ms] | 33.0 [32.4, 33.2] | – | same |
| explicit_tet10_ss | energies | base-t2 | 3 | 10 | 33.80 ms [33.48 ms, 34.28 ms] | 3.38 ms [3.35 ms, 3.43 ms] | 32.1 [31.8, 32.5] | 0.981 [0.965, 0.984] | same |
| explicit_tet10_ss | energies | base-t4 | 3 | 10 | 33.66 ms [33.48 ms, 34.03 ms] | 3.37 ms [3.35 ms, 3.40 ms] | 31.9 [31.7, 32.3] | 0.977 [0.955, 0.986] | same |
| explicit_tet10_ss | energies | head-t2 | 3 | 10 | 33.55 ms [33.43 ms, 33.64 ms] | 3.36 ms [3.34 ms, 3.36 ms] | 31.8 [31.7, 31.9] | 0.966 [0.957, 0.980] | same |
| explicit_tet10_ss | energies | head-t4 | 3 | 10 | 33.63 ms [33.11 ms, 33.72 ms] | 3.36 ms [3.31 ms, 3.37 ms] | 31.9 [31.4, 32.0] | 0.968 [0.945, 0.986] | same |
| explicit_tet4_fd | mesh | base-t1 | 3 | 1 | 3.35 ms [3.34 ms, 3.40 ms] | 3.35 ms [3.34 ms, 3.40 ms] | 20.7 [20.6, 21.0] | – | same |
| explicit_tet4_fd | mesh | base-t2 | 3 | 1 | 3.24 ms [3.23 ms, 3.53 ms] | 3.24 ms [3.23 ms, 3.53 ms] | 20.0 [19.9, 21.8] | 0.965 [0.953, 1.058] | same |
| explicit_tet4_fd | mesh | base-t4 | 3 | 1 | 3.24 ms [3.09 ms, 3.26 ms] | 3.24 ms [3.09 ms, 3.26 ms] | 20.0 [19.1, 20.1] | 0.958 [0.926, 0.967] | same |
| explicit_tet4_fd | mesh | head-t2 | 3 | 1 | 3.14 ms [3.12 ms, 3.14 ms] | 3.14 ms [3.12 ms, 3.14 ms] | 19.4 [19.3, 19.4] | 0.938 [0.919, 0.942] | same |
| explicit_tet4_fd | mesh | head-t4 | 3 | 1 | 3.22 ms [3.11 ms, 3.24 ms] | 3.22 ms [3.11 ms, 3.24 ms] | 19.9 [19.2, 20.0] | 0.961 [0.914, 0.971] | same |
| explicit_tet4_fd | init | base-t1 | 3 | 1 | 54.60 ms [53.73 ms, 55.04 ms] | 54.60 ms [53.73 ms, 55.04 ms] | 337.0 [331.7, 339.8] | – | same |
| explicit_tet4_fd | init | base-t2 | 3 | 1 | 53.96 ms [52.89 ms, 54.75 ms] | 53.96 ms [52.89 ms, 54.75 ms] | 333.1 [326.5, 337.9] | 0.984 [0.980, 1.003] | same |
| explicit_tet4_fd | init | base-t4 | 3 | 1 | 53.87 ms [53.27 ms, 54.59 ms] | 53.87 ms [53.27 ms, 54.59 ms] | 332.5 [328.9, 337.0] | 0.987 [0.968, 1.016] | same |
| explicit_tet4_fd | init | head-t2 | 3 | 1 | 55.05 ms [54.58 ms, 55.97 ms] | 55.05 ms [54.58 ms, 55.97 ms] | 339.8 [336.9, 345.5] | 1.008 [0.992, 1.042] | same |
| explicit_tet4_fd | init | head-t4 | 3 | 1 | 54.19 ms [53.96 ms, 54.24 ms] | 54.19 ms [53.96 ms, 54.24 ms] | 334.5 [333.1, 334.8] | 0.992 [0.980, 1.009] | same |
| explicit_tet4_fd | stable_time_step | base-t1 | 3 | 5 | 32.47 ms [32.41 ms, 32.49 ms] | 6.49 ms [6.48 ms, 6.50 ms] | 40.1 [40.0, 40.1] | – | same |
| explicit_tet4_fd | stable_time_step | base-t2 | 3 | 5 | 32.65 ms [32.43 ms, 34.18 ms] | 6.53 ms [6.49 ms, 6.84 ms] | 40.3 [40.0, 42.2] | 1.005 [1.001, 1.053] | same |
| explicit_tet4_fd | stable_time_step | base-t4 | 3 | 5 | 32.76 ms [32.64 ms, 34.87 ms] | 6.55 ms [6.53 ms, 6.97 ms] | 40.4 [40.3, 43.0] | 1.009 [1.004, 1.076] | same |
| explicit_tet4_fd | stable_time_step | head-t2 | 3 | 5 | 32.55 ms [32.39 ms, 32.84 ms] | 6.51 ms [6.48 ms, 6.57 ms] | 40.2 [40.0, 40.5] | 1.004 [0.998, 1.011] | same |
| explicit_tet4_fd | stable_time_step | head-t4 | 3 | 5 | 32.57 ms [32.42 ms, 32.71 ms] | 6.51 ms [6.48 ms, 6.54 ms] | 40.2 [40.0, 40.4] | 1.003 [1.000, 1.006] | same |
| explicit_tet4_fd | solve_step | base-t1 | 3 | 150 | 1.293 s [1.290 s, 1.297 s] | 8.62 ms [8.60 ms, 8.65 ms] | 53.2 [53.1, 53.4] | – | same |
| explicit_tet4_fd | solve_step | base-t2 | 3 | 150 | 720.04 ms [718.76 ms, 722.41 ms] | 4.80 ms [4.79 ms, 4.82 ms] | 29.6 [29.6, 29.7] | 0.557 [0.555, 0.559] | same |
| explicit_tet4_fd | solve_step | base-t4 | 3 | 150 | 438.78 ms [437.72 ms, 439.78 ms] | 2.93 ms [2.92 ms, 2.93 ms] | 18.1 [18.0, 18.1] | 0.339 [0.337, 0.341] | same |
| explicit_tet4_fd | solve_step | head-t2 | 3 | 150 | 673.85 ms [672.46 ms, 677.55 ms] | 4.49 ms [4.48 ms, 4.52 ms] | 27.7 [27.7, 27.9] | 0.520 [0.520, 0.525] | same |
| explicit_tet4_fd | solve_step | head-t4 | 3 | 150 | 364.62 ms [363.87 ms, 365.83 ms] | 2.43 ms [2.43 ms, 2.44 ms] | 15.0 [15.0, 15.1] | 0.282 [0.281, 0.283] | same |
| explicit_tet4_fd | predictor | base-t1 | 3 | 150 | 10.5 µs [10.4 µs, 17.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | predictor | base-t2 | 3 | 150 | 16.9 µs [15.3 µs, 17.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.461 [0.951, 1.633] | same |
| explicit_tet4_fd | predictor | base-t4 | 3 | 150 | 15.3 µs [12.8 µs, 16.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.227 [0.949, 1.463] | same |
| explicit_tet4_fd | predictor | head-t2 | 3 | 150 | 7.3 µs [5.8 µs, 8.7 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.699 [0.326, 0.835] | same |
| explicit_tet4_fd | predictor | head-t4 | 3 | 150 | 5.3 µs [4.7 µs, 8.9 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.444 [0.297, 0.855] | same |
| explicit_tet4_fd | residual | base-t1 | 3 | 150 | 1.228 s [1.225 s, 1.231 s] | 8.19 ms [8.17 ms, 8.21 ms] | 50.5 [50.4, 50.7] | – | same |
| explicit_tet4_fd | residual | base-t2 | 3 | 150 | 653.45 ms [652.58 ms, 655.88 ms] | 4.36 ms [4.35 ms, 4.37 ms] | 26.9 [26.9, 27.0] | 0.533 [0.531, 0.534] | same |
| explicit_tet4_fd | residual | base-t4 | 3 | 150 | 372.78 ms [371.63 ms, 373.82 ms] | 2.49 ms [2.48 ms, 2.49 ms] | 15.3 [15.3, 15.4] | 0.304 [0.302, 0.305] | same |
| explicit_tet4_fd | residual | head-t2 | 3 | 150 | 626.59 ms [625.82 ms, 630.16 ms] | 4.18 ms [4.17 ms, 4.20 ms] | 25.8 [25.8, 25.9] | 0.510 [0.509, 0.514] | same |
| explicit_tet4_fd | residual | head-t4 | 3 | 150 | 330.23 ms [329.85 ms, 331.75 ms] | 2.20 ms [2.20 ms, 2.21 ms] | 13.6 [13.6, 13.7] | 0.269 [0.268, 0.271] | same |
| explicit_tet4_fd | lumped_matrix | base-t1 | 3 | 150 | 35.2 µs [30.5 µs, 38.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | lumped_matrix | base-t2 | 3 | 150 | 29.8 µs [24.2 µs, 32.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.825 [0.687, 0.980] | same |
| explicit_tet4_fd | lumped_matrix | base-t4 | 3 | 150 | 30.2 µs [29.3 µs, 33.5 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.952 [0.754, 0.992] | same |
| explicit_tet4_fd | lumped_matrix | head-t2 | 3 | 150 | 38.3 µs [36.4 µs, 38.9 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.034 [0.985, 1.278] | same |
| explicit_tet4_fd | lumped_matrix | head-t4 | 3 | 150 | 29.5 µs [25.2 µs, 32.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.840 [0.649, 1.064] | same |
| explicit_tet4_fd | corrector | base-t1 | 3 | 150 | 17.8 µs [11.5 µs, 19.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | corrector | base-t2 | 3 | 150 | 20.8 µs [16.5 µs, 23.7 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.206 [0.928, 1.802] | same |
| explicit_tet4_fd | corrector | base-t4 | 3 | 150 | 18.8 µs [12.6 µs, 22.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.059 [0.640, 1.936] | same |
| explicit_tet4_fd | corrector | head-t2 | 3 | 150 | 17.0 µs [13.4 µs, 17.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.960 [0.880, 1.159] | same |
| explicit_tet4_fd | corrector | head-t4 | 3 | 150 | 10.0 µs [7.7 µs, 10.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.515 [0.432, 0.864] | same |
| explicit_tet4_fd | step_hooks | base-t1 | 3 | 300 | 492.6 µs [478.7 µs, 517.6 µs] | 1.6 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_fd | step_hooks | base-t2 | 3 | 300 | 494.5 µs [475.3 µs, 496.9 µs] | 1.6 µs [1.6 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.965 [0.955, 1.038] | same |
| explicit_tet4_fd | step_hooks | base-t4 | 3 | 300 | 467.3 µs [451.7 µs, 479.5 µs] | 1.6 µs [1.5 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 0.974 [0.873, 0.976] | same |
| explicit_tet4_fd | step_hooks | head-t2 | 3 | 300 | 535.6 µs [523.6 µs, 547.2 µs] | 1.8 µs [1.7 µs, 1.8 µs] | 0.0 [0.0, 0.0] | 1.094 [1.035, 1.111] | same |
| explicit_tet4_fd | step_hooks | head-t4 | 3 | 300 | 447.0 µs [432.7 µs, 486.7 µs] | 1.5 µs [1.4 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 0.907 [0.836, 1.017] | same |
| explicit_tet4_fd | step_rest | base-t1 | 3 | 150 | 64.52 ms [64.49 ms, 65.43 ms] | 430.2 µs [429.9 µs, 436.2 µs] | 2.7 [2.7, 2.7] | – | same |
| explicit_tet4_fd | step_rest | base-t2 | 3 | 150 | 65.93 ms [65.58 ms, 66.01 ms] | 439.5 µs [437.2 µs, 440.0 µs] | 2.7 [2.7, 2.7] | 1.017 [1.009, 1.022] | same |
| explicit_tet4_fd | step_rest | base-t4 | 3 | 150 | 65.44 ms [65.42 ms, 65.54 ms] | 436.3 µs [436.2 µs, 436.9 µs] | 2.7 [2.7, 2.7] | 1.014 [1.002, 1.014] | same |
| explicit_tet4_fd | step_rest | head-t2 | 3 | 150 | 46.63 ms [46.01 ms, 46.77 ms] | 310.9 µs [306.8 µs, 311.8 µs] | 1.9 [1.9, 1.9] | 0.713 [0.713, 0.725] | same |
| explicit_tet4_fd | step_rest | head-t4 | 3 | 150 | 33.58 ms [33.12 ms, 34.20 ms] | 223.9 µs [220.8 µs, 228.0 µs] | 1.4 [1.4, 1.4] | 0.521 [0.506, 0.530] | same |
| explicit_tet4_fd | internal_forces | base-t1 | 3 | 30 | 238.40 ms [238.18 ms, 239.32 ms] | 7.95 ms [7.94 ms, 7.98 ms] | 49.1 [49.0, 49.2] | – | same |
| explicit_tet4_fd | internal_forces | base-t2 | 3 | 30 | 122.74 ms [122.47 ms, 123.15 ms] | 4.09 ms [4.08 ms, 4.10 ms] | 25.3 [25.2, 25.3] | 0.515 [0.514, 0.515] | same |
| explicit_tet4_fd | internal_forces | base-t4 | 3 | 30 | 68.10 ms [67.77 ms, 68.11 ms] | 2.27 ms [2.26 ms, 2.27 ms] | 14.0 [13.9, 14.0] | 0.286 [0.283, 0.286] | same |
| explicit_tet4_fd | internal_forces | head-t2 | 3 | 30 | 119.01 ms [118.63 ms, 120.17 ms] | 3.97 ms [3.95 ms, 4.01 ms] | 24.5 [24.4, 24.7] | 0.498 [0.497, 0.505] | same |
| explicit_tet4_fd | internal_forces | head-t4 | 3 | 30 | 62.31 ms [62.30 ms, 62.56 ms] | 2.08 ms [2.08 ms, 2.09 ms] | 12.8 [12.8, 12.9] | 0.261 [0.260, 0.263] | same |
| explicit_tet4_fd | stresses | base-t1 | 3 | 30 | 76.29 ms [75.61 ms, 76.69 ms] | 2.54 ms [2.52 ms, 2.56 ms] | 15.7 [15.6, 15.8] | – | same |
| explicit_tet4_fd | stresses | base-t2 | 3 | 30 | 55.03 ms [54.82 ms, 55.32 ms] | 1.83 ms [1.83 ms, 1.84 ms] | 11.3 [11.3, 11.4] | 0.721 [0.719, 0.728] | same |
| explicit_tet4_fd | stresses | base-t4 | 3 | 30 | 44.95 ms [44.67 ms, 44.96 ms] | 1.50 ms [1.49 ms, 1.50 ms] | 9.2 [9.2, 9.3] | 0.586 [0.585, 0.595] | same |
| explicit_tet4_fd | stresses | head-t2 | 3 | 30 | 54.71 ms [54.59 ms, 54.74 ms] | 1.82 ms [1.82 ms, 1.82 ms] | 11.3 [11.2, 11.3] | 0.715 [0.714, 0.724] | same |
| explicit_tet4_fd | stresses | head-t4 | 3 | 30 | 44.18 ms [44.13 ms, 44.38 ms] | 1.47 ms [1.47 ms, 1.48 ms] | 9.1 [9.1, 9.1] | 0.582 [0.576, 0.584] | same |
| explicit_tet4_fd | energies | base-t1 | 3 | 30 | 79.97 ms [79.82 ms, 80.22 ms] | 2.67 ms [2.66 ms, 2.67 ms] | 16.5 [16.4, 16.5] | – | same |
| explicit_tet4_fd | energies | base-t2 | 3 | 30 | 79.98 ms [79.88 ms, 80.03 ms] | 2.67 ms [2.66 ms, 2.67 ms] | 16.5 [16.4, 16.5] | 1.001 [0.996, 1.002] | same |
| explicit_tet4_fd | energies | base-t4 | 3 | 30 | 79.85 ms [79.85 ms, 79.99 ms] | 2.66 ms [2.66 ms, 2.67 ms] | 16.4 [16.4, 16.5] | 1.000 [0.995, 1.000] | same |
| explicit_tet4_fd | energies | head-t2 | 3 | 30 | 80.50 ms [80.30 ms, 80.58 ms] | 2.68 ms [2.68 ms, 2.69 ms] | 16.6 [16.5, 16.6] | 1.006 [1.004, 1.008] | same |
| explicit_tet4_fd | energies | head-t4 | 3 | 30 | 80.09 ms [79.92 ms, 80.11 ms] | 2.67 ms [2.66 ms, 2.67 ms] | 16.5 [16.4, 16.5] | 1.001 [0.999, 1.001] | same |
| explicit_tet4_ss | mesh | base-t1 | 3 | 1 | 3.38 ms [3.19 ms, 3.39 ms] | 3.38 ms [3.19 ms, 3.39 ms] | 20.9 [19.7, 20.9] | – | same |
| explicit_tet4_ss | mesh | base-t2 | 3 | 1 | 3.23 ms [3.19 ms, 3.25 ms] | 3.23 ms [3.19 ms, 3.25 ms] | 19.9 [19.7, 20.1] | 0.962 [0.943, 1.013] | same |
| explicit_tet4_ss | mesh | base-t4 | 3 | 1 | 3.23 ms [3.23 ms, 3.40 ms] | 3.23 ms [3.23 ms, 3.40 ms] | 19.9 [19.9, 21.0] | 1.004 [0.954, 1.013] | same |
| explicit_tet4_ss | mesh | head-t2 | 3 | 1 | 3.11 ms [3.08 ms, 3.12 ms] | 3.11 ms [3.08 ms, 3.12 ms] | 19.2 [19.0, 19.2] | 0.919 [0.912, 0.977] | same |
| explicit_tet4_ss | mesh | head-t4 | 3 | 1 | 3.25 ms [3.12 ms, 3.32 ms] | 3.25 ms [3.12 ms, 3.32 ms] | 20.1 [19.3, 20.5] | 0.962 [0.923, 1.041] | same |
| explicit_tet4_ss | init | base-t1 | 3 | 1 | 47.66 ms [46.58 ms, 48.23 ms] | 47.66 ms [46.58 ms, 48.23 ms] | 294.2 [287.5, 297.7] | – | same |
| explicit_tet4_ss | init | base-t2 | 3 | 1 | 47.55 ms [46.47 ms, 47.59 ms] | 47.55 ms [46.47 ms, 47.59 ms] | 293.5 [286.8, 293.8] | 0.998 [0.963, 1.022] | same |
| explicit_tet4_ss | init | base-t4 | 3 | 1 | 47.68 ms [47.64 ms, 48.43 ms] | 47.68 ms [47.64 ms, 48.43 ms] | 294.3 [294.1, 298.9] | 1.004 [0.999, 1.024] | same |
| explicit_tet4_ss | init | head-t2 | 3 | 1 | 48.49 ms [47.96 ms, 48.89 ms] | 48.49 ms [47.96 ms, 48.89 ms] | 299.3 [296.1, 301.8] | 1.026 [0.994, 1.041] | same |
| explicit_tet4_ss | init | head-t4 | 3 | 1 | 49.16 ms [48.94 ms, 49.63 ms] | 49.16 ms [48.94 ms, 49.63 ms] | 303.5 [302.1, 306.3] | 1.031 [1.029, 1.051] | same |
| explicit_tet4_ss | stable_time_step | base-t1 | 3 | 5 | 32.69 ms [32.44 ms, 33.54 ms] | 6.54 ms [6.49 ms, 6.71 ms] | 40.4 [40.1, 41.4] | – | same |
| explicit_tet4_ss | stable_time_step | base-t2 | 3 | 5 | 38.95 ms [38.90 ms, 39.02 ms] | 7.79 ms [7.78 ms, 7.80 ms] | 48.1 [48.0, 48.2] | 1.190 [1.161, 1.203] | same |
| explicit_tet4_ss | stable_time_step | base-t4 | 3 | 5 | 32.59 ms [32.58 ms, 32.62 ms] | 6.52 ms [6.52 ms, 6.52 ms] | 40.2 [40.2, 40.3] | 0.997 [0.971, 1.005] | same |
| explicit_tet4_ss | stable_time_step | head-t2 | 3 | 5 | 37.90 ms [37.84 ms, 38.10 ms] | 7.58 ms [7.57 ms, 7.62 ms] | 46.8 [46.7, 47.0] | 1.160 [1.136, 1.166] | same |
| explicit_tet4_ss | stable_time_step | head-t4 | 3 | 5 | 32.43 ms [32.33 ms, 32.57 ms] | 6.49 ms [6.47 ms, 6.51 ms] | 40.0 [39.9, 40.2] | 0.992 [0.971, 0.997] | same |
| explicit_tet4_ss | solve_step | base-t1 | 3 | 200 | 1.285 s [1.276 s, 1.303 s] | 6.42 ms [6.38 ms, 6.51 ms] | 39.7 [39.4, 40.2] | – | same |
| explicit_tet4_ss | solve_step | base-t2 | 3 | 200 | 750.50 ms [749.76 ms, 753.00 ms] | 3.75 ms [3.75 ms, 3.76 ms] | 23.2 [23.1, 23.2] | 0.586 [0.576, 0.588] | same |
| explicit_tet4_ss | solve_step | base-t4 | 3 | 200 | 536.36 ms [536.10 ms, 538.70 ms] | 2.68 ms [2.68 ms, 2.69 ms] | 16.6 [16.5, 16.6] | 0.419 [0.412, 0.420] | same |
| explicit_tet4_ss | solve_step | head-t2 | 3 | 200 | 681.28 ms [679.89 ms, 682.26 ms] | 3.41 ms [3.40 ms, 3.41 ms] | 21.0 [21.0, 21.1] | 0.529 [0.524, 0.534] | same |
| explicit_tet4_ss | solve_step | head-t4 | 3 | 200 | 422.33 ms [422.21 ms, 423.95 ms] | 2.11 ms [2.11 ms, 2.12 ms] | 13.0 [13.0, 13.1] | 0.329 [0.325, 0.331] | same |
| explicit_tet4_ss | predictor | base-t1 | 3 | 200 | 14.1 µs [7.4 µs, 19.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | predictor | base-t2 | 3 | 200 | 23.0 µs [17.0 µs, 25.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.632 [1.323, 2.293] | same |
| explicit_tet4_ss | predictor | base-t4 | 3 | 200 | 15.7 µs [15.1 µs, 17.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.252 [0.792, 2.127] | same |
| explicit_tet4_ss | predictor | head-t2 | 3 | 200 | 6.2 µs [6.1 µs, 12.2 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.824 [0.328, 0.864] | same |
| explicit_tet4_ss | predictor | head-t4 | 3 | 200 | 7.8 µs [5.9 µs, 9.4 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.555 [0.492, 0.791] | same |
| explicit_tet4_ss | residual | base-t1 | 3 | 200 | 1.198 s [1.189 s, 1.215 s] | 5.99 ms [5.95 ms, 6.08 ms] | 37.0 [36.7, 37.5] | – | same |
| explicit_tet4_ss | residual | base-t2 | 3 | 200 | 661.95 ms [660.98 ms, 664.44 ms] | 3.31 ms [3.30 ms, 3.32 ms] | 20.4 [20.4, 20.5] | 0.555 [0.544, 0.557] | same |
| explicit_tet4_ss | residual | base-t4 | 3 | 200 | 448.71 ms [448.31 ms, 450.98 ms] | 2.24 ms [2.24 ms, 2.25 ms] | 13.8 [13.8, 13.9] | 0.377 [0.369, 0.377] | same |
| explicit_tet4_ss | residual | head-t2 | 3 | 200 | 618.58 ms [617.57 ms, 618.88 ms] | 3.09 ms [3.09 ms, 3.09 ms] | 19.1 [19.1, 19.1] | 0.516 [0.509, 0.520] | same |
| explicit_tet4_ss | residual | head-t4 | 3 | 200 | 378.28 ms [377.48 ms, 379.23 ms] | 1.89 ms [1.89 ms, 1.90 ms] | 11.7 [11.7, 11.7] | 0.316 [0.312, 0.317] | same |
| explicit_tet4_ss | lumped_matrix | base-t1 | 3 | 200 | 47.5 µs [43.8 µs, 49.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | lumped_matrix | base-t2 | 3 | 200 | 53.1 µs [50.7 µs, 58.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.070 [1.067, 1.341] | same |
| explicit_tet4_ss | lumped_matrix | base-t4 | 3 | 200 | 41.4 µs [34.0 µs, 53.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.945 [0.685, 1.135] | same |
| explicit_tet4_ss | lumped_matrix | head-t2 | 3 | 200 | 48.5 µs [48.4 µs, 52.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.100 [0.979, 1.105] | same |
| explicit_tet4_ss | lumped_matrix | head-t4 | 3 | 200 | 36.8 µs [34.3 µs, 41.1 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.784 [0.742, 0.866] | same |
| explicit_tet4_ss | corrector | base-t1 | 3 | 200 | 15.1 µs [15.1 µs, 31.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | corrector | base-t2 | 3 | 200 | 28.0 µs [27.1 µs, 34.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.790 [0.899, 2.280] | same |
| explicit_tet4_ss | corrector | base-t4 | 3 | 200 | 28.1 µs [24.7 µs, 33.0 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.632 [0.903, 2.187] | same |
| explicit_tet4_ss | corrector | head-t2 | 3 | 200 | 16.9 µs [15.0 µs, 22.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.995 [0.544, 1.464] | same |
| explicit_tet4_ss | corrector | head-t4 | 3 | 200 | 12.7 µs [11.5 µs, 13.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.763 [0.408, 0.904] | same |
| explicit_tet4_ss | step_hooks | base-t1 | 3 | 400 | 531.2 µs [520.4 µs, 542.9 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tet4_ss | step_hooks | base-t2 | 3 | 400 | 535.6 µs [524.2 µs, 536.3 µs] | 1.3 µs [1.3 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 1.007 [0.987, 1.010] | same |
| explicit_tet4_ss | step_hooks | base-t4 | 3 | 400 | 457.7 µs [438.1 µs, 479.0 µs] | 1.1 µs [1.1 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 0.843 [0.842, 0.902] | same |
| explicit_tet4_ss | step_hooks | head-t2 | 3 | 400 | 580.5 µs [545.5 µs, 602.8 µs] | 1.5 µs [1.4 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 1.110 [1.027, 1.115] | same |
| explicit_tet4_ss | step_hooks | head-t4 | 3 | 400 | 431.4 µs [421.8 µs, 515.3 µs] | 1.1 µs [1.1 µs, 1.3 µs] | 0.0 [0.0, 0.0] | 0.829 [0.777, 0.970] | same |
| explicit_tet4_ss | step_rest | base-t1 | 3 | 200 | 86.45 ms [86.08 ms, 86.69 ms] | 432.3 µs [430.4 µs, 433.5 µs] | 2.7 [2.7, 2.7] | – | same |
| explicit_tet4_ss | step_rest | base-t2 | 3 | 200 | 87.91 ms [87.86 ms, 88.10 ms] | 439.5 µs [439.3 µs, 440.5 µs] | 2.7 [2.7, 2.7] | 1.017 [1.016, 1.021] | same |
| explicit_tet4_ss | step_rest | base-t4 | 3 | 200 | 87.15 ms [87.05 ms, 87.23 ms] | 435.7 µs [435.2 µs, 436.1 µs] | 2.7 [2.7, 2.7] | 1.008 [1.006, 1.011] | same |
| explicit_tet4_ss | step_rest | head-t2 | 3 | 200 | 62.06 ms [61.64 ms, 62.65 ms] | 310.3 µs [308.2 µs, 313.2 µs] | 1.9 [1.9, 1.9] | 0.721 [0.713, 0.723] | same |
| explicit_tet4_ss | step_rest | head-t4 | 3 | 200 | 44.12 ms [43.55 ms, 44.22 ms] | 220.6 µs [217.7 µs, 221.1 µs] | 1.4 [1.3, 1.4] | 0.510 [0.504, 0.513] | same |
| explicit_tet4_ss | internal_forces | base-t1 | 3 | 40 | 227.29 ms [226.86 ms, 227.80 ms] | 5.68 ms [5.67 ms, 5.70 ms] | 35.1 [35.0, 35.2] | – | same |
| explicit_tet4_ss | internal_forces | base-t2 | 3 | 40 | 120.10 ms [118.96 ms, 121.13 ms] | 3.00 ms [2.97 ms, 3.03 ms] | 18.5 [18.4, 18.7] | 0.528 [0.522, 0.534] | same |
| explicit_tet4_ss | internal_forces | base-t4 | 3 | 40 | 74.80 ms [74.18 ms, 74.84 ms] | 1.87 ms [1.85 ms, 1.87 ms] | 11.5 [11.4, 11.5] | 0.328 [0.327, 0.329] | same |
| explicit_tet4_ss | internal_forces | head-t2 | 3 | 40 | 113.91 ms [113.81 ms, 113.96 ms] | 2.85 ms [2.85 ms, 2.85 ms] | 17.6 [17.6, 17.6] | 0.501 [0.500, 0.502] | same |
| explicit_tet4_ss | internal_forces | head-t4 | 3 | 40 | 64.64 ms [64.55 ms, 74.28 ms] | 1.62 ms [1.61 ms, 1.86 ms] | 10.0 [10.0, 11.5] | 0.285 [0.284, 0.326] | same |
| explicit_tet4_ss | stresses | base-t1 | 3 | 40 | 86.49 ms [85.97 ms, 86.51 ms] | 2.16 ms [2.15 ms, 2.16 ms] | 13.3 [13.3, 13.4] | – | same |
| explicit_tet4_ss | stresses | base-t2 | 3 | 40 | 58.11 ms [57.85 ms, 58.29 ms] | 1.45 ms [1.45 ms, 1.46 ms] | 9.0 [8.9, 9.0] | 0.673 [0.672, 0.674] | same |
| explicit_tet4_ss | stresses | base-t4 | 3 | 40 | 44.87 ms [44.58 ms, 45.44 ms] | 1.12 ms [1.11 ms, 1.14 ms] | 6.9 [6.9, 7.0] | 0.519 [0.519, 0.525] | same |
| explicit_tet4_ss | stresses | head-t2 | 3 | 40 | 57.95 ms [57.20 ms, 58.29 ms] | 1.45 ms [1.43 ms, 1.46 ms] | 8.9 [8.8, 9.0] | 0.670 [0.665, 0.674] | same |
| explicit_tet4_ss | stresses | head-t4 | 3 | 40 | 43.42 ms [43.33 ms, 44.29 ms] | 1.09 ms [1.08 ms, 1.11 ms] | 6.7 [6.7, 6.8] | 0.504 [0.502, 0.512] | same |
| explicit_tet4_ss | energies | base-t1 | 3 | 40 | 84.71 ms [84.54 ms, 85.14 ms] | 2.12 ms [2.11 ms, 2.13 ms] | 13.1 [13.0, 13.1] | – | same |
| explicit_tet4_ss | energies | base-t2 | 3 | 40 | 84.83 ms [84.68 ms, 85.09 ms] | 2.12 ms [2.12 ms, 2.13 ms] | 13.1 [13.1, 13.1] | 1.000 [0.996, 1.006] | same |
| explicit_tet4_ss | energies | base-t4 | 3 | 40 | 85.01 ms [84.85 ms, 85.01 ms] | 2.13 ms [2.12 ms, 2.13 ms] | 13.1 [13.1, 13.1] | 1.003 [0.997, 1.006] | same |
| explicit_tet4_ss | energies | head-t2 | 3 | 40 | 85.73 ms [85.57 ms, 85.91 ms] | 2.14 ms [2.14 ms, 2.15 ms] | 13.2 [13.2, 13.3] | 1.012 [1.009, 1.012] | same |
| explicit_tet4_ss | energies | head-t4 | 3 | 40 | 85.62 ms [85.17 ms, 85.72 ms] | 2.14 ms [2.13 ms, 2.14 ms] | 13.2 [13.1, 13.2] | 1.007 [1.006, 1.012] | same |
| explicit_thermal | mesh | base-t1 | 3 | 1 | 2.19 ms [2.17 ms, 2.21 ms] | 2.19 ms [2.17 ms, 2.21 ms] | 20.7 [20.6, 20.9] | – | same |
| explicit_thermal | mesh | base-t2 | 3 | 1 | 2.16 ms [2.16 ms, 2.17 ms] | 2.16 ms [2.16 ms, 2.17 ms] | 20.5 [20.5, 20.6] | 0.986 [0.980, 1.002] | same |
| explicit_thermal | mesh | base-t4 | 3 | 1 | 2.08 ms [2.00 ms, 2.34 ms] | 2.08 ms [2.00 ms, 2.34 ms] | 19.7 [19.0, 22.2] | 0.957 [0.906, 1.070] | same |
| explicit_thermal | mesh | head-t2 | 3 | 1 | 2.04 ms [2.02 ms, 2.07 ms] | 2.04 ms [2.02 ms, 2.07 ms] | 19.3 [19.1, 19.7] | 0.925 [0.923, 0.956] | same |
| explicit_thermal | mesh | head-t4 | 3 | 1 | 2.14 ms [2.03 ms, 2.14 ms] | 2.14 ms [2.03 ms, 2.14 ms] | 20.3 [19.3, 20.3] | 0.973 [0.929, 0.986] | same |
| explicit_thermal | init | base-t1 | 3 | 1 | 31.18 ms [30.59 ms, 31.42 ms] | 31.18 ms [30.59 ms, 31.42 ms] | 295.7 [290.1, 297.9] | – | same |
| explicit_thermal | init | base-t2 | 3 | 1 | 31.08 ms [30.87 ms, 31.20 ms] | 31.08 ms [30.87 ms, 31.20 ms] | 294.7 [292.7, 295.8] | 0.997 [0.993, 1.009] | same |
| explicit_thermal | init | base-t4 | 3 | 1 | 31.51 ms [30.56 ms, 31.66 ms] | 31.51 ms [30.56 ms, 31.66 ms] | 298.8 [289.8, 300.2] | 1.008 [0.999, 1.010] | same |
| explicit_thermal | init | head-t2 | 3 | 1 | 31.34 ms [30.65 ms, 31.68 ms] | 31.34 ms [30.65 ms, 31.68 ms] | 297.2 [290.6, 300.4] | 1.005 [0.975, 1.036] | same |
| explicit_thermal | init | head-t4 | 3 | 1 | 31.55 ms [31.29 ms, 32.01 ms] | 31.55 ms [31.29 ms, 32.01 ms] | 299.1 [296.7, 303.6] | 1.012 [0.996, 1.047] | same |
| explicit_thermal | stable_time_step | base-t1 | 3 | 5 | 21.12 ms [21.12 ms, 21.25 ms] | 4.22 ms [4.22 ms, 4.25 ms] | 40.1 [40.1, 40.3] | – | same |
| explicit_thermal | stable_time_step | base-t2 | 3 | 5 | 21.44 ms [21.42 ms, 21.47 ms] | 4.29 ms [4.28 ms, 4.29 ms] | 40.7 [40.6, 40.7] | 1.014 [1.010, 1.015] | same |
| explicit_thermal | stable_time_step | base-t4 | 3 | 5 | 21.20 ms [21.14 ms, 21.55 ms] | 4.24 ms [4.23 ms, 4.31 ms] | 40.2 [40.1, 40.9] | 1.001 [0.998, 1.020] | same |
| explicit_thermal | stable_time_step | head-t2 | 3 | 5 | 21.34 ms [21.34 ms, 21.35 ms] | 4.27 ms [4.27 ms, 4.27 ms] | 40.5 [40.5, 40.5] | 1.010 [1.005, 1.010] | same |
| explicit_thermal | stable_time_step | head-t4 | 3 | 5 | 21.10 ms [21.08 ms, 21.19 ms] | 4.22 ms [4.22 ms, 4.24 ms] | 40.0 [40.0, 40.2] | 0.999 [0.992, 1.003] | same |
| explicit_thermal | solve_step | base-t1 | 3 | 300 | 1.215 s [1.213 s, 1.216 s] | 4.05 ms [4.04 ms, 4.05 ms] | 38.4 [38.4, 38.4] | – | same |
| explicit_thermal | solve_step | base-t2 | 3 | 300 | 686.56 ms [684.83 ms, 693.75 ms] | 2.29 ms [2.28 ms, 2.31 ms] | 21.7 [21.6, 21.9] | 0.566 [0.563, 0.571] | same |
| explicit_thermal | solve_step | base-t4 | 3 | 300 | 469.70 ms [469.62 ms, 473.47 ms] | 1.57 ms [1.57 ms, 1.58 ms] | 14.8 [14.8, 15.0] | 0.387 [0.386, 0.389] | same |
| explicit_thermal | solve_step | head-t2 | 3 | 300 | 620.87 ms [615.98 ms, 626.95 ms] | 2.07 ms [2.05 ms, 2.09 ms] | 19.6 [19.5, 19.8] | 0.512 [0.507, 0.516] | same |
| explicit_thermal | solve_step | head-t4 | 3 | 300 | 361.85 ms [359.82 ms, 363.34 ms] | 1.21 ms [1.20 ms, 1.21 ms] | 11.4 [11.4, 11.5] | 0.298 [0.296, 0.299] | same |
| explicit_thermal | predictor | base-t1 | 3 | 300 | 25.2 µs [19.0 µs, 31.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | predictor | base-t2 | 3 | 300 | 12.1 µs [8.6 µs, 14.4 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.571 [0.274, 0.635] | same |
| explicit_thermal | predictor | base-t4 | 3 | 300 | 11.8 µs [10.5 µs, 15.9 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.468 [0.334, 0.839] | same |
| explicit_thermal | predictor | head-t2 | 3 | 300 | 8.3 µs [8.1 µs, 8.8 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.331 [0.279, 0.429] | same |
| explicit_thermal | predictor | head-t4 | 3 | 300 | 8.0 µs [7.7 µs, 8.2 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.306 [0.255, 0.434] | same |
| explicit_thermal | residual | base-t1 | 3 | 300 | 1.130 s [1.127 s, 1.130 s] | 3.77 ms [3.76 ms, 3.77 ms] | 35.7 [35.6, 35.7] | – | same |
| explicit_thermal | residual | base-t2 | 3 | 300 | 601.63 ms [599.96 ms, 608.73 ms] | 2.01 ms [2.00 ms, 2.03 ms] | 19.0 [19.0, 19.2] | 0.534 [0.531, 0.539] | same |
| explicit_thermal | residual | base-t4 | 3 | 300 | 384.70 ms [384.33 ms, 387.56 ms] | 1.28 ms [1.28 ms, 1.29 ms] | 12.2 [12.1, 12.3] | 0.341 [0.340, 0.343] | same |
| explicit_thermal | residual | head-t2 | 3 | 300 | 561.15 ms [555.97 ms, 566.39 ms] | 1.87 ms [1.85 ms, 1.89 ms] | 17.7 [17.6, 17.9] | 0.498 [0.492, 0.501] | same |
| explicit_thermal | residual | head-t4 | 3 | 300 | 317.68 ms [316.63 ms, 319.05 ms] | 1.06 ms [1.06 ms, 1.06 ms] | 10.0 [10.0, 10.1] | 0.281 [0.280, 0.283] | same |
| explicit_thermal | lumped_matrix | base-t1 | 3 | 300 | 66.1 µs [44.8 µs, 73.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | lumped_matrix | base-t2 | 3 | 300 | 64.4 µs [58.9 µs, 67.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.891 [0.877, 1.517] | same |
| explicit_thermal | lumped_matrix | base-t4 | 3 | 300 | 52.1 µs [51.4 µs, 60.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.922 [0.699, 1.163] | same |
| explicit_thermal | lumped_matrix | head-t2 | 3 | 300 | 57.0 µs [54.7 µs, 60.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.862 [0.745, 1.340] | same |
| explicit_thermal | lumped_matrix | head-t4 | 3 | 300 | 41.4 µs [38.1 µs, 44.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.576 [0.564, 1.003] | same |
| explicit_thermal | corrector | base-t1 | 3 | 300 | 19.3 µs [15.9 µs, 19.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | corrector | base-t2 | 3 | 300 | 19.3 µs [18.6 µs, 28.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.216 [0.948, 1.469] | same |
| explicit_thermal | corrector | base-t4 | 3 | 300 | 19.7 µs [10.7 µs, 25.1 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.003 [0.552, 1.578] | same |
| explicit_thermal | corrector | head-t2 | 3 | 300 | 17.0 µs [16.1 µs, 17.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.928 [0.820, 1.072] | same |
| explicit_thermal | corrector | head-t4 | 3 | 300 | 14.3 µs [10.0 µs, 15.6 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.791 [0.519, 0.903] | same |
| explicit_thermal | step_hooks | base-t1 | 3 | 600 | 714.1 µs [695.5 µs, 776.4 µs] | 1.2 µs [1.2 µs, 1.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_thermal | step_hooks | base-t2 | 3 | 600 | 693.2 µs [605.0 µs, 731.4 µs] | 1.2 µs [1.0 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 0.997 [0.779, 1.024] | same |
| explicit_thermal | step_hooks | base-t4 | 3 | 600 | 542.0 µs [528.9 µs, 611.7 µs] | 0.9 µs [0.9 µs, 1.0 µs] | 0.0 [0.0, 0.0] | 0.761 [0.698, 0.857] | same |
| explicit_thermal | step_hooks | head-t2 | 3 | 600 | 697.0 µs [645.4 µs, 701.1 µs] | 1.2 µs [1.1 µs, 1.2 µs] | 0.0 [0.0, 0.0] | 0.904 [0.903, 1.002] | same |
| explicit_thermal | step_hooks | head-t4 | 3 | 600 | 511.3 µs [505.9 µs, 555.4 µs] | 0.9 µs [0.8 µs, 0.9 µs] | 0.0 [0.0, 0.0] | 0.727 [0.659, 0.778] | same |
| explicit_thermal | step_rest | base-t1 | 3 | 300 | 84.19 ms [83.68 ms, 84.72 ms] | 280.6 µs [278.9 µs, 282.4 µs] | 2.7 [2.6, 2.7] | – | same |
| explicit_thermal | step_rest | base-t2 | 3 | 300 | 83.58 ms [83.52 ms, 83.77 ms] | 278.6 µs [278.4 µs, 279.2 µs] | 2.6 [2.6, 2.6] | 0.995 [0.987, 0.998] | same |
| explicit_thermal | step_rest | base-t4 | 3 | 300 | 84.19 ms [83.79 ms, 84.78 ms] | 280.6 µs [279.3 µs, 282.6 µs] | 2.7 [2.6, 2.7] | 1.001 [0.994, 1.007] | same |
| explicit_thermal | step_rest | head-t2 | 3 | 300 | 58.57 ms [58.36 ms, 59.08 ms] | 195.2 µs [194.5 µs, 196.9 µs] | 1.9 [1.8, 1.9] | 0.700 [0.689, 0.702] | same |
| explicit_thermal | step_rest | head-t4 | 3 | 300 | 43.04 ms [42.06 ms, 43.08 ms] | 143.5 µs [140.2 µs, 143.6 µs] | 1.4 [1.3, 1.4] | 0.508 [0.503, 0.511] | same |
| explicit_thermal | internal_forces | base-t1 | 3 | 60 | 215.20 ms [214.25 ms, 215.40 ms] | 3.59 ms [3.57 ms, 3.59 ms] | 34.0 [33.9, 34.0] | – | same |
| explicit_thermal | internal_forces | base-t2 | 3 | 60 | 111.27 ms [110.11 ms, 111.63 ms] | 1.85 ms [1.84 ms, 1.86 ms] | 17.6 [17.4, 17.6] | 0.519 [0.511, 0.519] | same |
| explicit_thermal | internal_forces | base-t4 | 3 | 60 | 66.97 ms [66.74 ms, 67.18 ms] | 1.12 ms [1.11 ms, 1.12 ms] | 10.6 [10.5, 10.6] | 0.311 [0.310, 0.314] | same |
| explicit_thermal | internal_forces | head-t2 | 3 | 60 | 105.68 ms [104.57 ms, 105.78 ms] | 1.76 ms [1.74 ms, 1.76 ms] | 16.7 [16.5, 16.7] | 0.491 [0.488, 0.491] | same |
| explicit_thermal | internal_forces | head-t4 | 3 | 60 | 56.93 ms [56.85 ms, 56.99 ms] | 948.8 µs [947.5 µs, 949.8 µs] | 9.0 [9.0, 9.0] | 0.265 [0.264, 0.266] | same |
| explicit_thermal | stresses | base-t1 | 3 | 60 | 82.16 ms [81.73 ms, 82.21 ms] | 1.37 ms [1.36 ms, 1.37 ms] | 13.0 [12.9, 13.0] | – | same |
| explicit_thermal | stresses | base-t2 | 3 | 60 | 54.91 ms [54.13 ms, 54.96 ms] | 915.1 µs [902.2 µs, 915.9 µs] | 8.7 [8.6, 8.7] | 0.669 [0.659, 0.672] | same |
| explicit_thermal | stresses | base-t4 | 3 | 60 | 41.70 ms [41.42 ms, 41.72 ms] | 695.0 µs [690.3 µs, 695.4 µs] | 6.6 [6.5, 6.6] | 0.508 [0.504, 0.510] | same |
| explicit_thermal | stresses | head-t2 | 3 | 60 | 54.43 ms [54.40 ms, 54.58 ms] | 907.2 µs [906.6 µs, 909.7 µs] | 8.6 [8.6, 8.6] | 0.664 [0.662, 0.666] | same |
| explicit_thermal | stresses | head-t4 | 3 | 60 | 40.55 ms [40.49 ms, 40.84 ms] | 675.8 µs [674.8 µs, 680.7 µs] | 6.4 [6.4, 6.5] | 0.493 [0.493, 0.500] | same |
| explicit_thermal | energies | base-t1 | 3 | 60 | 82.68 ms [82.55 ms, 82.77 ms] | 1.38 ms [1.38 ms, 1.38 ms] | 13.1 [13.0, 13.1] | – | same |
| explicit_thermal | energies | base-t2 | 3 | 60 | 82.67 ms [82.57 ms, 84.05 ms] | 1.38 ms [1.38 ms, 1.40 ms] | 13.1 [13.0, 13.3] | 1.000 [0.999, 1.017] | same |
| explicit_thermal | energies | base-t4 | 3 | 60 | 82.96 ms [82.64 ms, 82.97 ms] | 1.38 ms [1.38 ms, 1.38 ms] | 13.1 [13.1, 13.1] | 1.003 [0.999, 1.005] | same |
| explicit_thermal | energies | head-t2 | 3 | 60 | 83.32 ms [83.20 ms, 83.33 ms] | 1.39 ms [1.39 ms, 1.39 ms] | 13.2 [13.1, 13.2] | 1.007 [1.006, 1.009] | same |
| explicit_thermal | energies | head-t4 | 3 | 60 | 83.02 ms [82.86 ms, 83.42 ms] | 1.38 ms [1.38 ms, 1.39 ms] | 13.1 [13.1, 13.2] | 1.004 [1.004, 1.008] | same |
| explicit_tri3_fd | mesh | base-t1 | 3 | 1 | 4.23 ms [4.12 ms, 4.37 ms] | 4.23 ms [4.12 ms, 4.37 ms] | 23.5 [22.9, 24.3] | – | same |
| explicit_tri3_fd | mesh | base-t2 | 3 | 1 | 4.26 ms [4.24 ms, 4.28 ms] | 4.26 ms [4.24 ms, 4.28 ms] | 23.7 [23.5, 23.8] | 1.001 [0.976, 1.039] | same |
| explicit_tri3_fd | mesh | base-t4 | 3 | 1 | 4.11 ms [4.07 ms, 4.12 ms] | 4.11 ms [4.07 ms, 4.12 ms] | 22.8 [22.6, 22.9] | 0.961 [0.942, 1.000] | same |
| explicit_tri3_fd | mesh | head-t2 | 3 | 1 | 3.92 ms [3.90 ms, 4.08 ms] | 3.92 ms [3.90 ms, 4.08 ms] | 21.8 [21.7, 22.6] | 0.926 [0.894, 0.989] | same |
| explicit_tri3_fd | mesh | head-t4 | 3 | 1 | 4.03 ms [4.02 ms, 4.07 ms] | 4.03 ms [4.02 ms, 4.07 ms] | 22.4 [22.3, 22.6] | 0.962 [0.924, 0.976] | same |
| explicit_tri3_fd | init | base-t1 | 3 | 1 | 40.78 ms [40.69 ms, 41.01 ms] | 40.78 ms [40.69 ms, 41.01 ms] | 226.5 [226.1, 227.8] | – | same |
| explicit_tri3_fd | init | base-t2 | 3 | 1 | 40.76 ms [40.71 ms, 41.01 ms] | 40.76 ms [40.71 ms, 41.01 ms] | 226.5 [226.2, 227.8] | 1.001 [0.994, 1.006] | same |
| explicit_tri3_fd | init | base-t4 | 3 | 1 | 41.07 ms [40.96 ms, 41.57 ms] | 41.07 ms [40.96 ms, 41.57 ms] | 228.2 [227.5, 231.0] | 1.007 [0.999, 1.022] | same |
| explicit_tri3_fd | init | head-t2 | 3 | 1 | 40.84 ms [40.81 ms, 41.02 ms] | 40.84 ms [40.81 ms, 41.02 ms] | 226.9 [226.7, 227.9] | 1.004 [0.995, 1.006] | same |
| explicit_tri3_fd | init | head-t4 | 3 | 1 | 40.72 ms [40.68 ms, 40.91 ms] | 40.72 ms [40.68 ms, 40.91 ms] | 226.2 [226.0, 227.3] | 1.000 [0.993, 1.003] | same |
| explicit_tri3_fd | stable_time_step | base-t1 | 3 | 5 | 23.09 ms [22.84 ms, 23.55 ms] | 4.62 ms [4.57 ms, 4.71 ms] | 25.7 [25.4, 26.2] | – | same |
| explicit_tri3_fd | stable_time_step | base-t2 | 3 | 5 | 23.40 ms [23.06 ms, 23.61 ms] | 4.68 ms [4.61 ms, 4.72 ms] | 26.0 [25.6, 26.2] | 1.009 [1.002, 1.013] | same |
| explicit_tri3_fd | stable_time_step | base-t4 | 3 | 5 | 23.00 ms [22.69 ms, 23.57 ms] | 4.60 ms [4.54 ms, 4.71 ms] | 25.6 [25.2, 26.2] | 0.996 [0.993, 1.001] | same |
| explicit_tri3_fd | stable_time_step | head-t2 | 3 | 5 | 23.33 ms [23.33 ms, 23.78 ms] | 4.67 ms [4.67 ms, 4.76 ms] | 25.9 [25.9, 26.4] | 1.010 [1.010, 1.021] | same |
| explicit_tri3_fd | stable_time_step | head-t4 | 3 | 5 | 23.72 ms [22.61 ms, 23.78 ms] | 4.74 ms [4.52 ms, 4.76 ms] | 26.4 [25.1, 26.4] | 1.007 [0.979, 1.041] | same |
| explicit_tri3_fd | solve_step | base-t1 | 3 | 200 | 1.453 s [1.447 s, 1.453 s] | 7.27 ms [7.24 ms, 7.27 ms] | 40.4 [40.2, 40.4] | – | same |
| explicit_tri3_fd | solve_step | base-t2 | 3 | 200 | 845.95 ms [842.75 ms, 846.40 ms] | 4.23 ms [4.21 ms, 4.23 ms] | 23.5 [23.4, 23.5] | 0.582 [0.582, 0.582] | same |
| explicit_tri3_fd | solve_step | base-t4 | 3 | 200 | 590.51 ms [589.06 ms, 593.61 ms] | 2.95 ms [2.95 ms, 2.97 ms] | 16.4 [16.4, 16.5] | 0.406 [0.405, 0.410] | same |
| explicit_tri3_fd | solve_step | head-t2 | 3 | 200 | 735.30 ms [733.90 ms, 736.69 ms] | 3.68 ms [3.67 ms, 3.68 ms] | 20.4 [20.4, 20.5] | 0.506 [0.505, 0.509] | same |
| explicit_tri3_fd | solve_step | head-t4 | 3 | 200 | 413.23 ms [412.48 ms, 414.19 ms] | 2.07 ms [2.06 ms, 2.07 ms] | 11.5 [11.5, 11.5] | 0.284 [0.284, 0.286] | same |
| explicit_tri3_fd | predictor | base-t1 | 3 | 200 | 16.3 µs [15.4 µs, 26.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | predictor | base-t2 | 3 | 200 | 24.1 µs [20.5 µs, 33.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.256 [1.239, 1.560] | same |
| explicit_tri3_fd | predictor | base-t4 | 3 | 200 | 16.2 µs [14.4 µs, 20.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.882 [0.751, 1.048] | same |
| explicit_tri3_fd | predictor | head-t2 | 3 | 200 | 12.7 µs [9.2 µs, 12.9 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.566 [0.481, 0.824] | same |
| explicit_tri3_fd | predictor | head-t4 | 3 | 200 | 8.8 µs [8.5 µs, 10.9 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.569 [0.318, 0.669] | same |
| explicit_tri3_fd | residual | base-t1 | 3 | 200 | 1.278 s [1.273 s, 1.279 s] | 6.39 ms [6.37 ms, 6.39 ms] | 35.5 [35.4, 35.5] | – | same |
| explicit_tri3_fd | residual | base-t2 | 3 | 200 | 671.07 ms [668.04 ms, 671.36 ms] | 3.36 ms [3.34 ms, 3.36 ms] | 18.6 [18.6, 18.6] | 0.525 [0.525, 0.525] | same |
| explicit_tri3_fd | residual | base-t4 | 3 | 200 | 417.37 ms [416.30 ms, 419.65 ms] | 2.09 ms [2.08 ms, 2.10 ms] | 11.6 [11.6, 11.7] | 0.326 [0.326, 0.330] | same |
| explicit_tri3_fd | residual | head-t2 | 3 | 200 | 616.76 ms [615.79 ms, 617.99 ms] | 3.08 ms [3.08 ms, 3.09 ms] | 17.1 [17.1, 17.2] | 0.482 [0.482, 0.485] | same |
| explicit_tri3_fd | residual | head-t4 | 3 | 200 | 332.47 ms [332.39 ms, 333.54 ms] | 1.66 ms [1.66 ms, 1.67 ms] | 9.2 [9.2, 9.3] | 0.260 [0.260, 0.262] | same |
| explicit_tri3_fd | lumped_matrix | base-t1 | 3 | 200 | 50.2 µs [46.6 µs, 53.3 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | lumped_matrix | base-t2 | 3 | 200 | 50.1 µs [42.4 µs, 54.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.076 [0.796, 1.085] | same |
| explicit_tri3_fd | lumped_matrix | base-t4 | 3 | 200 | 46.4 µs [45.3 µs, 51.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.902 [0.870, 1.113] | same |
| explicit_tri3_fd | lumped_matrix | head-t2 | 3 | 200 | 51.4 µs [50.2 µs, 58.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.077 [1.022, 1.092] | same |
| explicit_tri3_fd | lumped_matrix | head-t4 | 3 | 200 | 46.2 µs [44.6 µs, 51.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.887 [0.867, 1.100] | same |
| explicit_tri3_fd | corrector | base-t1 | 3 | 200 | 24.3 µs [23.0 µs, 38.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | corrector | base-t2 | 3 | 200 | 28.4 µs [21.1 µs, 35.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.922 [0.871, 1.235] | same |
| explicit_tri3_fd | corrector | base-t4 | 3 | 200 | 16.4 µs [12.8 µs, 18.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.716 [0.330, 0.752] | same |
| explicit_tri3_fd | corrector | head-t2 | 3 | 200 | 18.0 µs [15.1 µs, 22.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.620 [0.588, 0.786] | same |
| explicit_tri3_fd | corrector | head-t4 | 3 | 200 | 10.2 µs [9.9 µs, 15.4 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.446 [0.253, 0.636] | same |
| explicit_tri3_fd | step_hooks | base-t1 | 3 | 400 | 649.2 µs [644.5 µs, 651.4 µs] | 1.6 µs [1.6 µs, 1.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_fd | step_hooks | base-t2 | 3 | 400 | 652.4 µs [650.2 µs, 652.8 µs] | 1.6 µs [1.6 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 1.002 [1.001, 1.013] | same |
| explicit_tri3_fd | step_hooks | base-t4 | 3 | 400 | 607.9 µs [568.2 µs, 673.4 µs] | 1.5 µs [1.4 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.933 [0.875, 1.045] | same |
| explicit_tri3_fd | step_hooks | head-t2 | 3 | 400 | 668.9 µs [667.6 µs, 695.0 µs] | 1.7 µs [1.7 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 1.038 [1.025, 1.071] | same |
| explicit_tri3_fd | step_hooks | head-t4 | 3 | 400 | 603.8 µs [603.2 µs, 692.9 µs] | 1.5 µs [1.5 µs, 1.7 µs] | 0.0 [0.0, 0.0] | 0.936 [0.927, 1.067] | same |
| explicit_tri3_fd | step_rest | base-t1 | 3 | 200 | 173.87 ms [173.17 ms, 174.62 ms] | 869.4 µs [865.9 µs, 873.1 µs] | 4.8 [4.8, 4.9] | – | same |
| explicit_tri3_fd | step_rest | base-t2 | 3 | 200 | 173.93 ms [173.77 ms, 174.55 ms] | 869.7 µs [868.8 µs, 872.7 µs] | 4.8 [4.8, 4.8] | 1.004 [0.995, 1.004] | same |
| explicit_tri3_fd | step_rest | base-t4 | 3 | 200 | 172.36 ms [172.10 ms, 173.23 ms] | 861.8 µs [860.5 µs, 866.1 µs] | 4.8 [4.8, 4.8] | 0.991 [0.986, 1.000] | same |
| explicit_tri3_fd | step_rest | head-t2 | 3 | 200 | 117.74 ms [117.31 ms, 117.89 ms] | 588.7 µs [586.6 µs, 589.4 µs] | 3.3 [3.3, 3.3] | 0.677 [0.672, 0.681] | same |
| explicit_tri3_fd | step_rest | head-t4 | 3 | 200 | 79.96 ms [79.30 ms, 80.03 ms] | 399.8 µs [396.5 µs, 400.2 µs] | 2.2 [2.2, 2.2] | 0.458 [0.456, 0.462] | same |
| explicit_tri3_fd | internal_forces | base-t1 | 3 | 40 | 233.09 ms [232.05 ms, 233.73 ms] | 5.83 ms [5.80 ms, 5.84 ms] | 32.4 [32.2, 32.5] | – | same |
| explicit_tri3_fd | internal_forces | base-t2 | 3 | 40 | 112.68 ms [112.23 ms, 112.71 ms] | 2.82 ms [2.81 ms, 2.82 ms] | 15.7 [15.6, 15.7] | 0.482 [0.481, 0.486] | same |
| explicit_tri3_fd | internal_forces | base-t4 | 3 | 40 | 63.81 ms [63.70 ms, 63.98 ms] | 1.60 ms [1.59 ms, 1.60 ms] | 8.9 [8.8, 8.9] | 0.274 [0.273, 0.275] | same |
| explicit_tri3_fd | internal_forces | head-t2 | 3 | 40 | 107.54 ms [107.48 ms, 108.02 ms] | 2.69 ms [2.69 ms, 2.70 ms] | 14.9 [14.9, 15.0] | 0.463 [0.460, 0.463] | same |
| explicit_tri3_fd | internal_forces | head-t4 | 3 | 40 | 56.20 ms [56.16 ms, 56.31 ms] | 1.40 ms [1.40 ms, 1.41 ms] | 7.8 [7.8, 7.8] | 0.241 [0.240, 0.243] | same |
| explicit_tri3_fd | stresses | base-t1 | 3 | 40 | 56.00 ms [55.87 ms, 56.90 ms] | 1.40 ms [1.40 ms, 1.42 ms] | 7.8 [7.8, 7.9] | – | same |
| explicit_tri3_fd | stresses | base-t2 | 3 | 40 | 39.97 ms [39.91 ms, 40.24 ms] | 999.3 µs [997.9 µs, 1.01 ms] | 5.6 [5.5, 5.6] | 0.714 [0.702, 0.719] | same |
| explicit_tri3_fd | stresses | base-t4 | 3 | 40 | 32.14 ms [31.78 ms, 32.50 ms] | 803.6 µs [794.4 µs, 812.4 µs] | 4.5 [4.4, 4.5] | 0.574 [0.558, 0.582] | same |
| explicit_tri3_fd | stresses | head-t2 | 3 | 40 | 39.21 ms [39.19 ms, 39.58 ms] | 980.2 µs [979.7 µs, 989.4 µs] | 5.4 [5.4, 5.5] | 0.702 [0.689, 0.707] | same |
| explicit_tri3_fd | stresses | head-t4 | 3 | 40 | 31.03 ms [30.42 ms, 31.50 ms] | 775.8 µs [760.6 µs, 787.4 µs] | 4.3 [4.2, 4.4] | 0.545 [0.543, 0.564] | same |
| explicit_tri3_fd | energies | base-t1 | 3 | 40 | 99.11 ms [98.49 ms, 99.60 ms] | 2.48 ms [2.46 ms, 2.49 ms] | 13.8 [13.7, 13.8] | – | same |
| explicit_tri3_fd | energies | base-t2 | 3 | 40 | 99.25 ms [98.74 ms, 99.32 ms] | 2.48 ms [2.47 ms, 2.48 ms] | 13.8 [13.7, 13.8] | 1.001 [0.991, 1.008] | same |
| explicit_tri3_fd | energies | base-t4 | 3 | 40 | 98.85 ms [98.39 ms, 98.90 ms] | 2.47 ms [2.46 ms, 2.47 ms] | 13.7 [13.7, 13.7] | 0.997 [0.988, 1.004] | same |
| explicit_tri3_fd | energies | head-t2 | 3 | 40 | 99.90 ms [99.40 ms, 100.11 ms] | 2.50 ms [2.48 ms, 2.50 ms] | 13.9 [13.8, 13.9] | 1.010 [0.998, 1.014] | same |
| explicit_tri3_fd | energies | head-t4 | 3 | 40 | 99.38 ms [99.02 ms, 99.54 ms] | 2.48 ms [2.48 ms, 2.49 ms] | 13.8 [13.8, 13.8] | 1.003 [0.994, 1.011] | same |
| explicit_tri3_ss | mesh | base-t1 | 3 | 1 | 4.12 ms [4.06 ms, 4.16 ms] | 4.12 ms [4.06 ms, 4.16 ms] | 22.9 [22.5, 23.1] | – | same |
| explicit_tri3_ss | mesh | base-t2 | 3 | 1 | 4.35 ms [4.14 ms, 4.54 ms] | 4.35 ms [4.14 ms, 4.54 ms] | 24.2 [23.0, 25.2] | 1.046 [1.004, 1.120] | same |
| explicit_tri3_ss | mesh | base-t4 | 3 | 1 | 4.21 ms [4.07 ms, 4.38 ms] | 4.21 ms [4.07 ms, 4.38 ms] | 23.4 [22.6, 24.3] | 1.038 [0.979, 1.063] | same |
| explicit_tri3_ss | mesh | head-t2 | 3 | 1 | 4.01 ms [3.95 ms, 4.04 ms] | 4.01 ms [3.95 ms, 4.04 ms] | 22.3 [21.9, 22.4] | 0.972 [0.949, 0.994] | same |
| explicit_tri3_ss | mesh | head-t4 | 3 | 1 | 4.10 ms [3.95 ms, 4.11 ms] | 4.10 ms [3.95 ms, 4.11 ms] | 22.8 [22.0, 22.8] | 0.996 [0.950, 1.013] | same |
| explicit_tri3_ss | init | base-t1 | 3 | 1 | 37.62 ms [37.31 ms, 37.76 ms] | 37.62 ms [37.31 ms, 37.76 ms] | 209.0 [207.3, 209.8] | – | same |
| explicit_tri3_ss | init | base-t2 | 3 | 1 | 38.11 ms [37.55 ms, 38.25 ms] | 38.11 ms [37.55 ms, 38.25 ms] | 211.7 [208.6, 212.5] | 1.013 [0.998, 1.022] | same |
| explicit_tri3_ss | init | base-t4 | 3 | 1 | 37.99 ms [37.98 ms, 38.41 ms] | 37.99 ms [37.98 ms, 38.41 ms] | 211.1 [211.0, 213.4] | 1.017 [1.010, 1.018] | same |
| explicit_tri3_ss | init | head-t2 | 3 | 1 | 37.73 ms [37.55 ms, 38.12 ms] | 37.73 ms [37.55 ms, 38.12 ms] | 209.6 [208.6, 211.8] | 1.003 [0.994, 1.022] | same |
| explicit_tri3_ss | init | head-t4 | 3 | 1 | 38.21 ms [38.00 ms, 38.21 ms] | 38.21 ms [38.00 ms, 38.21 ms] | 212.3 [211.1, 212.3] | 1.012 [1.010, 1.024] | same |
| explicit_tri3_ss | stable_time_step | base-t1 | 3 | 5 | 23.16 ms [23.06 ms, 23.45 ms] | 4.63 ms [4.61 ms, 4.69 ms] | 25.7 [25.6, 26.1] | – | same |
| explicit_tri3_ss | stable_time_step | base-t2 | 3 | 5 | 23.20 ms [23.10 ms, 23.51 ms] | 4.64 ms [4.62 ms, 4.70 ms] | 25.8 [25.7, 26.1] | 1.002 [0.997, 1.006] | same |
| explicit_tri3_ss | stable_time_step | base-t4 | 3 | 5 | 23.14 ms [22.91 ms, 23.19 ms] | 4.63 ms [4.58 ms, 4.64 ms] | 25.7 [25.5, 25.8] | 0.989 [0.987, 1.006] | same |
| explicit_tri3_ss | stable_time_step | head-t2 | 3 | 5 | 22.66 ms [22.63 ms, 23.35 ms] | 4.53 ms [4.53 ms, 4.67 ms] | 25.2 [25.1, 25.9] | 0.977 [0.966, 1.013] | same |
| explicit_tri3_ss | stable_time_step | head-t4 | 3 | 5 | 23.40 ms [23.34 ms, 23.99 ms] | 4.68 ms [4.67 ms, 4.80 ms] | 26.0 [25.9, 26.7] | 1.015 [1.008, 1.023] | same |
| explicit_tri3_ss | solve_step | base-t1 | 3 | 300 | 1.522 s [1.509 s, 1.524 s] | 5.07 ms [5.03 ms, 5.08 ms] | 28.2 [27.9, 28.2] | – | same |
| explicit_tri3_ss | solve_step | base-t2 | 3 | 300 | 957.82 ms [950.67 ms, 959.45 ms] | 3.19 ms [3.17 ms, 3.20 ms] | 17.7 [17.6, 17.8] | 0.628 [0.624, 0.636] | same |
| explicit_tri3_ss | solve_step | base-t4 | 3 | 300 | 781.99 ms [781.36 ms, 782.71 ms] | 2.61 ms [2.60 ms, 2.61 ms] | 14.5 [14.5, 14.5] | 0.514 [0.513, 0.518] | same |
| explicit_tri3_ss | solve_step | head-t2 | 3 | 300 | 778.12 ms [776.58 ms, 784.20 ms] | 2.59 ms [2.59 ms, 2.61 ms] | 14.4 [14.4, 14.5] | 0.511 [0.510, 0.520] | same |
| explicit_tri3_ss | solve_step | head-t4 | 3 | 300 | 497.95 ms [493.56 ms, 498.95 ms] | 1.66 ms [1.65 ms, 1.66 ms] | 9.2 [9.1, 9.2] | 0.327 [0.327, 0.327] | same |
| explicit_tri3_ss | predictor | base-t1 | 3 | 300 | 32.0 µs [27.7 µs, 46.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | predictor | base-t2 | 3 | 300 | 30.3 µs [25.4 µs, 33.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.793 [0.730, 1.095] | same |
| explicit_tri3_ss | predictor | base-t4 | 3 | 300 | 33.7 µs [32.7 µs, 43.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.054 [0.709, 1.567] | same |
| explicit_tri3_ss | predictor | head-t2 | 3 | 300 | 14.2 µs [11.4 µs, 14.8 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | 0.443 [0.248, 0.533] | same |
| explicit_tri3_ss | predictor | head-t4 | 3 | 300 | 11.9 µs [9.3 µs, 15.7 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.292 [0.258, 0.568] | same |
| explicit_tri3_ss | residual | base-t1 | 3 | 300 | 1.260 s [1.247 s, 1.261 s] | 4.20 ms [4.16 ms, 4.20 ms] | 23.3 [23.1, 23.4] | – | same |
| explicit_tri3_ss | residual | base-t2 | 3 | 300 | 695.73 ms [689.31 ms, 697.07 ms] | 2.32 ms [2.30 ms, 2.32 ms] | 12.9 [12.8, 12.9] | 0.552 [0.547, 0.559] | same |
| explicit_tri3_ss | residual | base-t4 | 3 | 300 | 522.12 ms [522.01 ms, 522.50 ms] | 1.74 ms [1.74 ms, 1.74 ms] | 9.7 [9.7, 9.7] | 0.415 [0.414, 0.419] | same |
| explicit_tri3_ss | residual | head-t2 | 3 | 300 | 601.51 ms [598.61 ms, 606.21 ms] | 2.01 ms [2.00 ms, 2.02 ms] | 11.1 [11.1, 11.2] | 0.477 [0.475, 0.486] | same |
| explicit_tri3_ss | residual | head-t4 | 3 | 300 | 378.15 ms [376.04 ms, 378.23 ms] | 1.26 ms [1.25 ms, 1.26 ms] | 7.0 [7.0, 7.0] | 0.300 [0.300, 0.302] | same |
| explicit_tri3_ss | lumped_matrix | base-t1 | 3 | 300 | 62.5 µs [59.8 µs, 78.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | lumped_matrix | base-t2 | 3 | 300 | 71.1 µs [67.4 µs, 76.9 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.190 [0.860, 1.231] | same |
| explicit_tri3_ss | lumped_matrix | base-t4 | 3 | 300 | 76.2 µs [75.8 µs, 81.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.213 [1.036, 1.274] | same |
| explicit_tri3_ss | lumped_matrix | head-t2 | 3 | 300 | 72.6 µs [70.9 µs, 75.8 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.213 [0.904, 1.215] | same |
| explicit_tri3_ss | lumped_matrix | head-t4 | 3 | 300 | 58.5 µs [54.4 µs, 65.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.910 [0.746, 1.047] | same |
| explicit_tri3_ss | corrector | base-t1 | 3 | 300 | 34.6 µs [19.5 µs, 46.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | corrector | base-t2 | 3 | 300 | 31.2 µs [22.5 µs, 35.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.765 [0.651, 1.599] | same |
| explicit_tri3_ss | corrector | base-t4 | 3 | 300 | 22.9 µs [22.6 µs, 36.3 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.661 [0.488, 1.857] | same |
| explicit_tri3_ss | corrector | head-t2 | 3 | 300 | 22.0 µs [19.2 µs, 26.4 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.556 [0.475, 1.350] | same |
| explicit_tri3_ss | corrector | head-t4 | 3 | 300 | 16.1 µs [13.5 µs, 19.9 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.464 [0.292, 1.018] | same |
| explicit_tri3_ss | step_hooks | base-t1 | 3 | 600 | 882.8 µs [827.9 µs, 911.5 µs] | 1.5 µs [1.4 µs, 1.5 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_tri3_ss | step_hooks | base-t2 | 3 | 600 | 832.1 µs [822.2 µs, 869.7 µs] | 1.4 µs [1.4 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.985 [0.913, 0.993] | same |
| explicit_tri3_ss | step_hooks | base-t4 | 3 | 600 | 796.8 µs [770.3 µs, 836.0 µs] | 1.3 µs [1.3 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.874 [0.873, 1.010] | same |
| explicit_tri3_ss | step_hooks | head-t2 | 3 | 600 | 903.4 µs [814.3 µs, 946.3 µs] | 1.5 µs [1.4 µs, 1.6 µs] | 0.0 [0.0, 0.0] | 1.072 [0.893, 1.091] | same |
| explicit_tri3_ss | step_hooks | head-t4 | 3 | 600 | 859.2 µs [806.9 µs, 896.5 µs] | 1.4 µs [1.3 µs, 1.5 µs] | 0.0 [0.0, 0.0] | 0.975 [0.943, 1.016] | same |
| explicit_tri3_ss | step_rest | base-t1 | 3 | 300 | 261.22 ms [260.37 ms, 261.63 ms] | 870.7 µs [867.9 µs, 872.1 µs] | 4.8 [4.8, 4.8] | – | same |
| explicit_tri3_ss | step_rest | base-t2 | 3 | 300 | 261.00 ms [260.31 ms, 261.37 ms] | 870.0 µs [867.7 µs, 871.2 µs] | 4.8 [4.8, 4.8] | 0.998 [0.997, 1.004] | same |
| explicit_tri3_ss | step_rest | base-t4 | 3 | 300 | 258.87 ms [258.36 ms, 259.23 ms] | 862.9 µs [861.2 µs, 864.1 µs] | 4.8 [4.8, 4.8] | 0.992 [0.988, 0.994] | same |
| explicit_tri3_ss | step_rest | head-t2 | 3 | 300 | 176.81 ms [175.63 ms, 176.93 ms] | 589.4 µs [585.4 µs, 589.8 µs] | 3.3 [3.3, 3.3] | 0.676 [0.672, 0.680] | same |
| explicit_tri3_ss | step_rest | head-t4 | 3 | 300 | 118.83 ms [116.59 ms, 119.65 ms] | 396.1 µs [388.6 µs, 398.8 µs] | 2.2 [2.2, 2.2] | 0.455 [0.448, 0.457] | same |
| explicit_tri3_ss | internal_forces | base-t1 | 3 | 60 | 212.61 ms [212.28 ms, 290.76 ms] | 3.54 ms [3.54 ms, 4.85 ms] | 19.7 [19.7, 26.9] | – | same |
| explicit_tri3_ss | internal_forces | base-t2 | 3 | 60 | 103.26 ms [102.20 ms, 103.68 ms] | 1.72 ms [1.70 ms, 1.73 ms] | 9.6 [9.5, 9.6] | 0.481 [0.357, 0.486] | same |
| explicit_tri3_ss | internal_forces | base-t4 | 3 | 60 | 66.17 ms [65.36 ms, 66.35 ms] | 1.10 ms [1.09 ms, 1.11 ms] | 6.1 [6.1, 6.1] | 0.308 [0.228, 0.312] | same |
| explicit_tri3_ss | internal_forces | head-t2 | 3 | 60 | 92.72 ms [92.13 ms, 93.60 ms] | 1.55 ms [1.54 ms, 1.56 ms] | 8.6 [8.5, 8.7] | 0.433 [0.322, 0.437] | same |
| explicit_tri3_ss | internal_forces | head-t4 | 3 | 60 | 51.07 ms [50.58 ms, 51.17 ms] | 851.1 µs [843.1 µs, 852.8 µs] | 4.7 [4.7, 4.7] | 0.240 [0.174, 0.241] | same |
| explicit_tri3_ss | stresses | base-t1 | 3 | 60 | 74.02 ms [73.68 ms, 74.13 ms] | 1.23 ms [1.23 ms, 1.24 ms] | 6.9 [6.8, 6.9] | – | same |
| explicit_tri3_ss | stresses | base-t2 | 3 | 60 | 50.33 ms [50.30 ms, 51.18 ms] | 838.8 µs [838.4 µs, 853.0 µs] | 4.7 [4.7, 4.7] | 0.683 [0.680, 0.690] | same |
| explicit_tri3_ss | stresses | base-t4 | 3 | 60 | 38.06 ms [37.55 ms, 38.22 ms] | 634.3 µs [625.9 µs, 637.1 µs] | 3.5 [3.5, 3.5] | 0.516 [0.507, 0.517] | same |
| explicit_tri3_ss | stresses | head-t2 | 3 | 60 | 49.52 ms [48.96 ms, 49.90 ms] | 825.3 µs [816.0 µs, 831.7 µs] | 4.6 [4.5, 4.6] | 0.672 [0.660, 0.674] | same |
| explicit_tri3_ss | stresses | head-t4 | 3 | 60 | 37.18 ms [35.73 ms, 37.48 ms] | 619.6 µs [595.5 µs, 624.6 µs] | 3.4 [3.3, 3.5] | 0.501 [0.483, 0.509] | same |
| explicit_tri3_ss | energies | base-t1 | 3 | 60 | 139.49 ms [139.07 ms, 140.01 ms] | 2.32 ms [2.32 ms, 2.33 ms] | 12.9 [12.9, 13.0] | – | same |
| explicit_tri3_ss | energies | base-t2 | 3 | 60 | 139.89 ms [139.16 ms, 140.49 ms] | 2.33 ms [2.32 ms, 2.34 ms] | 13.0 [12.9, 13.0] | 0.999 [0.998, 1.010] | same |
| explicit_tri3_ss | energies | base-t4 | 3 | 60 | 139.71 ms [139.52 ms, 140.55 ms] | 2.33 ms [2.33 ms, 2.34 ms] | 12.9 [12.9, 13.0] | 1.003 [1.002, 1.004] | same |
| explicit_tri3_ss | energies | head-t2 | 3 | 60 | 141.31 ms [140.25 ms, 141.34 ms] | 2.36 ms [2.34 ms, 2.36 ms] | 13.1 [13.0, 13.1] | 1.009 [1.005, 1.016] | same |
| explicit_tri3_ss | energies | head-t4 | 3 | 60 | 140.65 ms [140.18 ms, 140.97 ms] | 2.34 ms [2.34 ms, 2.35 ms] | 13.0 [13.0, 13.1] | 1.008 [1.005, 1.011] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | 7365a08f4, f8babbe8e | 78b072c2f464d8f9 |
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | 7365a08f4, f8babbe8e | 37ce0eeb914dd994 |
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 7365a08f4, f8babbe8e | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 36137566d8b810bb |
| explicit_tet10_fd | 105456 | 148877 | 446631 | 1 | 7365a08f4, f8babbe8e | c9f013e1a9bbd7a1 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | 7365a08f4, f8babbe8e | a12864e94a9122c8 |
| explicit_tet4_fd | 162000 | 29791 | 89373 | 1 | 7365a08f4, f8babbe8e | 9d6cdbdad424d346 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 7365a08f4, f8babbe8e | fecb3fb4b46c8b8c |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | 7365a08f4, f8babbe8e | 27979d9fbcdea83d |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 7365a08f4, f8babbe8e | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 7365a08f4, f8babbe8e | b6b902f5ca71049e |
