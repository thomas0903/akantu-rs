Suite `solid`, size `m`; variants: `base-t1`, `gather-t6`, `gather-t8`, `serial-t6`, `serial-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars_non_local | mesh | base-t1 | 5 | 1 | 1.02 ms [921.0 µs, 1.05 ms] | 1.02 ms [921.0 µs, 1.05 ms] | 21.3 [19.2, 21.8] | – | same |
| explicit_mazars_non_local | mesh | gather-t6 | 5 | 1 | 1.02 ms [1.01 ms, 1.04 ms] | 1.02 ms [1.01 ms, 1.04 ms] | 21.3 [21.1, 21.6] | 1.003 [0.976, 1.100] | same |
| explicit_mazars_non_local | mesh | gather-t8 | 5 | 1 | 1.01 ms [993.3 µs, 1.01 ms] | 1.01 ms [993.3 µs, 1.01 ms] | 20.9 [20.7, 21.0] | 0.985 [0.948, 1.092] | same |
| explicit_mazars_non_local | mesh | serial-t6 | 5 | 1 | 1.01 ms [977.9 µs, 1.04 ms] | 1.01 ms [977.9 µs, 1.04 ms] | 21.1 [20.4, 21.6] | 0.991 [0.942, 1.095] | same |
| explicit_mazars_non_local | mesh | serial-t8 | 5 | 1 | 1.02 ms [1.01 ms, 1.02 ms] | 1.02 ms [1.01 ms, 1.02 ms] | 21.2 [21.1, 21.3] | 0.992 [0.975, 1.104] | same |
| explicit_mazars_non_local | init | base-t1 | 5 | 1 | 603.66 ms [603.15 ms, 605.23 ms] | 603.66 ms [603.15 ms, 605.23 ms] | 12576.3 [12565.6, 12608.9] | – | same |
| explicit_mazars_non_local | init | gather-t6 | 5 | 1 | 608.58 ms [606.60 ms, 614.37 ms] | 608.58 ms [606.60 ms, 614.37 ms] | 12678.7 [12637.4, 12799.4] | 1.008 [1.003, 1.015] | same |
| explicit_mazars_non_local | init | gather-t8 | 5 | 1 | 609.04 ms [607.35 ms, 615.86 ms] | 609.04 ms [607.35 ms, 615.86 ms] | 12688.4 [12653.2, 12830.4] | 1.010 [1.007, 1.018] | same |
| explicit_mazars_non_local | init | serial-t6 | 5 | 1 | 606.01 ms [604.60 ms, 609.44 ms] | 606.01 ms [604.60 ms, 609.44 ms] | 12625.2 [12595.9, 12696.6] | 1.004 [1.000, 1.009] | same |
| explicit_mazars_non_local | init | serial-t8 | 5 | 1 | 606.69 ms [605.80 ms, 614.99 ms] | 606.69 ms [605.80 ms, 614.99 ms] | 12639.3 [12620.8, 12812.3] | 1.005 [1.003, 1.017] | same |
| explicit_mazars_non_local | stable_time_step | base-t1 | 5 | 5 | 9.63 ms [9.63 ms, 9.71 ms] | 1.93 ms [1.93 ms, 1.94 ms] | 40.1 [40.1, 40.5] | – | same |
| explicit_mazars_non_local | stable_time_step | gather-t6 | 5 | 5 | 9.61 ms [9.59 ms, 9.76 ms] | 1.92 ms [1.92 ms, 1.95 ms] | 40.1 [40.0, 40.7] | 0.998 [0.988, 1.013] | same |
| explicit_mazars_non_local | stable_time_step | gather-t8 | 5 | 5 | 9.65 ms [9.60 ms, 10.16 ms] | 1.93 ms [1.92 ms, 2.03 ms] | 40.2 [40.0, 42.3] | 1.001 [0.992, 1.054] | same |
| explicit_mazars_non_local | stable_time_step | serial-t6 | 5 | 5 | 9.62 ms [9.59 ms, 9.63 ms] | 1.92 ms [1.92 ms, 1.93 ms] | 40.1 [39.9, 40.1] | 0.999 [0.988, 0.999] | same |
| explicit_mazars_non_local | stable_time_step | serial-t8 | 5 | 5 | 9.61 ms [9.57 ms, 9.62 ms] | 1.92 ms [1.91 ms, 1.92 ms] | 40.0 [39.9, 40.1] | 0.997 [0.989, 0.999] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 1.670 s [1.667 s, 1.672 s] | 41.76 ms [41.66 ms, 41.81 ms] | 870.0 [868.0, 870.9] | – | same |
| explicit_mazars_non_local | solve_step | gather-t6 | 5 | 40 | 771.64 ms [768.66 ms, 793.56 ms] | 19.29 ms [19.22 ms, 19.84 ms] | 401.9 [400.3, 413.3] | 0.463 [0.460, 0.475] | same |
| explicit_mazars_non_local | solve_step | gather-t8 | 5 | 40 | 612.35 ms [607.69 ms, 623.82 ms] | 15.31 ms [15.19 ms, 15.60 ms] | 318.9 [316.5, 324.9] | 0.366 [0.364, 0.373] | same |
| explicit_mazars_non_local | solve_step | serial-t6 | 5 | 40 | 926.83 ms [920.21 ms, 929.60 ms] | 23.17 ms [23.01 ms, 23.24 ms] | 482.7 [479.3, 484.2] | 0.555 [0.551, 0.557] | same |
| explicit_mazars_non_local | solve_step | serial-t8 | 5 | 40 | 892.48 ms [880.56 ms, 893.65 ms] | 22.31 ms [22.01 ms, 22.34 ms] | 464.8 [458.6, 465.4] | 0.534 [0.528, 0.536] | same |
| explicit_mazars_non_local | predictor | base-t1 | 5 | 40 | 6.4 µs [3.3 µs, 8.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | gather-t6 | 5 | 40 | 2.6 µs [1.0 µs, 3.8 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.419 [0.155, 1.154] | same |
| explicit_mazars_non_local | predictor | gather-t8 | 5 | 40 | 2.7 µs [1.1 µs, 3.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.404 [0.319, 0.711] | same |
| explicit_mazars_non_local | predictor | serial-t6 | 5 | 40 | 2.6 µs [1.9 µs, 4.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.443 [0.239, 1.251] | same |
| explicit_mazars_non_local | predictor | serial-t8 | 5 | 40 | 2.5 µs [1.4 µs, 2.7 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.398 [0.216, 0.668] | same |
| explicit_mazars_non_local | residual | base-t1 | 5 | 40 | 1.633 s [1.629 s, 1.635 s] | 40.83 ms [40.72 ms, 40.87 ms] | 850.6 [848.4, 851.6] | – | same |
| explicit_mazars_non_local | residual | gather-t6 | 5 | 40 | 756.23 ms [753.26 ms, 778.34 ms] | 18.91 ms [18.83 ms, 19.46 ms] | 393.9 [392.3, 405.4] | 0.464 [0.461, 0.476] | same |
| explicit_mazars_non_local | residual | gather-t8 | 5 | 40 | 597.17 ms [592.66 ms, 608.63 ms] | 14.93 ms [14.82 ms, 15.22 ms] | 311.0 [308.7, 317.0] | 0.365 [0.363, 0.373] | same |
| explicit_mazars_non_local | residual | serial-t6 | 5 | 40 | 912.09 ms [905.27 ms, 914.40 ms] | 22.80 ms [22.63 ms, 22.86 ms] | 475.0 [471.5, 476.2] | 0.559 [0.555, 0.560] | same |
| explicit_mazars_non_local | residual | serial-t8 | 5 | 40 | 877.77 ms [865.62 ms, 878.75 ms] | 21.94 ms [21.64 ms, 21.97 ms] | 457.2 [450.8, 457.7] | 0.537 [0.531, 0.539] | same |
| explicit_mazars_non_local | lumped_matrix | base-t1 | 5 | 40 | 9.4 µs [8.6 µs, 9.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | gather-t6 | 5 | 40 | 10.5 µs [10.3 µs, 11.0 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.143 [1.088, 1.202] | same |
| explicit_mazars_non_local | lumped_matrix | gather-t8 | 5 | 40 | 11.6 µs [10.7 µs, 20.6 µs] | 0.3 µs [0.3 µs, 0.5 µs] | 0.0 [0.0, 0.0] | 1.232 [1.103, 2.242] | same |
| explicit_mazars_non_local | lumped_matrix | serial-t6 | 5 | 40 | 10.6 µs [9.4 µs, 12.0 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.118 [1.104, 1.305] | same |
| explicit_mazars_non_local | lumped_matrix | serial-t8 | 5 | 40 | 10.9 µs [10.6 µs, 11.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.180 [1.094, 1.244] | same |
| explicit_mazars_non_local | corrector | base-t1 | 5 | 40 | 6.2 µs [4.3 µs, 6.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | gather-t6 | 5 | 40 | 5.8 µs [5.7 µs, 14.5 µs] | 0.1 µs [0.1 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.958 [0.843, 2.702] | same |
| explicit_mazars_non_local | corrector | gather-t8 | 5 | 40 | 6.3 µs [5.8 µs, 7.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.059 [0.967, 1.479] | same |
| explicit_mazars_non_local | corrector | serial-t6 | 5 | 40 | 5.7 µs [5.6 µs, 6.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.916 [0.820, 1.514] | same |
| explicit_mazars_non_local | corrector | serial-t8 | 5 | 40 | 6.1 µs [3.2 µs, 6.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.991 [0.594, 1.407] | same |
| explicit_mazars_non_local | step_hooks | base-t1 | 5 | 80 | 29.71 ms [29.55 ms, 29.93 ms] | 371.3 µs [369.4 µs, 374.1 µs] | 7.7 [7.7, 7.8] | – | same |
| explicit_mazars_non_local | step_hooks | gather-t6 | 5 | 80 | 10.47 ms [10.33 ms, 10.49 ms] | 130.9 µs [129.2 µs, 131.1 µs] | 2.7 [2.7, 2.7] | 0.353 [0.345, 0.355] | same |
| explicit_mazars_non_local | step_hooks | gather-t8 | 5 | 80 | 10.27 ms [10.18 ms, 10.31 ms] | 128.3 µs [127.2 µs, 128.9 µs] | 2.7 [2.7, 2.7] | 0.345 [0.343, 0.349] | same |
| explicit_mazars_non_local | step_hooks | serial-t6 | 5 | 80 | 10.13 ms [10.00 ms, 10.42 ms] | 126.7 µs [125.1 µs, 130.2 µs] | 2.6 [2.6, 2.7] | 0.341 [0.334, 0.353] | same |
| explicit_mazars_non_local | step_hooks | serial-t8 | 5 | 80 | 10.10 ms [9.95 ms, 10.17 ms] | 126.2 µs [124.4 µs, 127.1 µs] | 2.6 [2.6, 2.6] | 0.340 [0.332, 0.342] | same |
| explicit_mazars_non_local | step_rest | base-t1 | 5 | 40 | 7.52 ms [7.39 ms, 7.67 ms] | 188.0 µs [184.8 µs, 191.8 µs] | 3.9 [3.9, 4.0] | – | same |
| explicit_mazars_non_local | step_rest | gather-t6 | 5 | 40 | 4.65 ms [4.59 ms, 4.82 ms] | 116.3 µs [114.8 µs, 120.4 µs] | 2.4 [2.4, 2.5] | 0.624 [0.603, 0.652] | same |
| explicit_mazars_non_local | step_rest | gather-t8 | 5 | 40 | 4.77 ms [4.67 ms, 4.81 ms] | 119.3 µs [116.8 µs, 120.3 µs] | 2.5 [2.4, 2.5] | 0.627 [0.622, 0.650] | same |
| explicit_mazars_non_local | step_rest | serial-t6 | 5 | 40 | 4.64 ms [4.55 ms, 4.68 ms] | 116.1 µs [113.7 µs, 117.0 µs] | 2.4 [2.4, 2.4] | 0.613 [0.593, 0.632] | same |
| explicit_mazars_non_local | step_rest | serial-t8 | 5 | 40 | 4.67 ms [4.66 ms, 4.71 ms] | 116.9 µs [116.4 µs, 117.8 µs] | 2.4 [2.4, 2.5] | 0.619 [0.609, 0.636] | same |
| explicit_mazars_non_local | internal_forces | base-t1 | 5 | 8 | 335.47 ms [335.39 ms, 336.39 ms] | 41.93 ms [41.92 ms, 42.05 ms] | 873.6 [873.4, 876.0] | – | same |
| explicit_mazars_non_local | internal_forces | gather-t6 | 5 | 8 | 152.73 ms [149.49 ms, 155.28 ms] | 19.09 ms [18.69 ms, 19.41 ms] | 397.7 [389.3, 404.4] | 0.455 [0.446, 0.463] | same |
| explicit_mazars_non_local | internal_forces | gather-t8 | 5 | 8 | 119.89 ms [118.15 ms, 121.82 ms] | 14.99 ms [14.77 ms, 15.23 ms] | 312.2 [307.7, 317.2] | 0.357 [0.352, 0.363] | same |
| explicit_mazars_non_local | internal_forces | serial-t6 | 5 | 8 | 183.33 ms [181.44 ms, 183.74 ms] | 22.92 ms [22.68 ms, 22.97 ms] | 477.4 [472.5, 478.5] | 0.545 [0.541, 0.547] | same |
| explicit_mazars_non_local | internal_forces | serial-t8 | 5 | 8 | 175.49 ms [173.32 ms, 175.99 ms] | 21.94 ms [21.67 ms, 22.00 ms] | 457.0 [451.4, 458.3] | 0.522 [0.517, 0.525] | same |
| explicit_mazars_non_local | stresses | base-t1 | 5 | 8 | 75.52 ms [75.14 ms, 75.71 ms] | 9.44 ms [9.39 ms, 9.46 ms] | 196.7 [195.7, 197.2] | – | same |
| explicit_mazars_non_local | stresses | gather-t6 | 5 | 8 | 72.82 ms [72.55 ms, 73.23 ms] | 9.10 ms [9.07 ms, 9.15 ms] | 189.6 [188.9, 190.7] | 0.967 [0.961, 0.968] | same |
| explicit_mazars_non_local | stresses | gather-t8 | 5 | 8 | 72.68 ms [72.34 ms, 72.91 ms] | 9.08 ms [9.04 ms, 9.11 ms] | 189.3 [188.4, 189.9] | 0.963 [0.957, 0.969] | same |
| explicit_mazars_non_local | stresses | serial-t6 | 5 | 8 | 73.20 ms [72.65 ms, 73.92 ms] | 9.15 ms [9.08 ms, 9.24 ms] | 190.6 [189.2, 192.5] | 0.970 [0.966, 0.976] | same |
| explicit_mazars_non_local | stresses | serial-t8 | 5 | 8 | 72.58 ms [72.44 ms, 73.12 ms] | 9.07 ms [9.05 ms, 9.14 ms] | 189.0 [188.6, 190.4] | 0.965 [0.957, 0.968] | same |
| explicit_mazars_non_local | energies | base-t1 | 5 | 8 | 5.51 ms [5.48 ms, 5.66 ms] | 688.5 µs [684.7 µs, 707.6 µs] | 14.3 [14.3, 14.7] | – | same |
| explicit_mazars_non_local | energies | gather-t6 | 5 | 8 | 4.47 ms [4.45 ms, 4.55 ms] | 558.3 µs [555.7 µs, 568.8 µs] | 11.6 [11.6, 11.8] | 0.815 [0.785, 0.821] | same |
| explicit_mazars_non_local | energies | gather-t8 | 5 | 8 | 4.32 ms [4.31 ms, 4.36 ms] | 540.2 µs [539.0 µs, 545.1 µs] | 11.3 [11.2, 11.4] | 0.785 [0.770, 0.793] | same |
| explicit_mazars_non_local | energies | serial-t6 | 5 | 8 | 4.50 ms [4.45 ms, 4.54 ms] | 563.0 µs [556.4 µs, 568.0 µs] | 11.7 [11.6, 11.8] | 0.819 [0.795, 0.827] | same |
| explicit_mazars_non_local | energies | serial-t8 | 5 | 8 | 4.34 ms [4.31 ms, 4.39 ms] | 542.0 µs [538.7 µs, 548.3 µs] | 11.3 [11.2, 11.4] | 0.789 [0.764, 0.792] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t1 | 5 | 2 | 1.175 s [1.173 s, 1.183 s] | 587.25 ms [586.50 ms, 591.50 ms] | 12234.4 [12218.8, 12322.9] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | gather-t6 | 5 | 2 | 1.184 s [1.182 s, 1.204 s] | 592.08 ms [590.89 ms, 601.98 ms] | 12335.1 [12310.2, 12541.2] | 1.007 [1.001, 1.025] | same |
| explicit_mazars_non_local | nonlocal_rebuild | gather-t8 | 5 | 2 | 1.183 s [1.177 s, 1.188 s] | 591.25 ms [588.66 ms, 594.23 ms] | 12317.7 [12263.7, 12379.8] | 1.004 [0.999, 1.012] | same |
| explicit_mazars_non_local | nonlocal_rebuild | serial-t6 | 5 | 2 | 1.185 s [1.180 s, 1.190 s] | 592.42 ms [589.97 ms, 595.20 ms] | 12342.1 [12291.0, 12400.0] | 1.009 [1.002, 1.010] | same |
| explicit_mazars_non_local | nonlocal_rebuild | serial-t8 | 5 | 2 | 1.184 s [1.179 s, 1.200 s] | 592.07 ms [589.45 ms, 600.17 ms] | 12334.8 [12280.3, 12503.6] | 1.004 [1.000, 1.023] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 7365a08f4, e49de3d84 | 849f4cd05e9ad628 |
