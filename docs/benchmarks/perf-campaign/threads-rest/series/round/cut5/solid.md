Suite `solid`, size `m`; variants: `base-t1`, `gather-t5`, `serial-t5`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars_non_local | mesh | base-t1 | 5 | 1 | 1.03 ms [1.02 ms, 1.04 ms] | 1.03 ms [1.02 ms, 1.04 ms] | 21.4 [21.2, 21.6] | – | same |
| explicit_mazars_non_local | mesh | gather-t5 | 5 | 1 | 1.02 ms [995.4 µs, 1.02 ms] | 1.02 ms [995.4 µs, 1.02 ms] | 21.2 [20.7, 21.3] | 0.993 [0.961, 1.003] | same |
| explicit_mazars_non_local | mesh | serial-t5 | 5 | 1 | 1.01 ms [995.8 µs, 1.03 ms] | 1.01 ms [995.8 µs, 1.03 ms] | 21.0 [20.7, 21.5] | 0.984 [0.979, 1.008] | same |
| explicit_mazars_non_local | init | base-t1 | 5 | 1 | 606.41 ms [601.09 ms, 611.24 ms] | 606.41 ms [601.09 ms, 611.24 ms] | 12633.5 [12522.7, 12734.1] | – | same |
| explicit_mazars_non_local | init | gather-t5 | 5 | 1 | 607.42 ms [604.52 ms, 612.65 ms] | 607.42 ms [604.52 ms, 612.65 ms] | 12654.5 [12594.1, 12763.5] | 0.997 [0.994, 1.019] | same |
| explicit_mazars_non_local | init | serial-t5 | 5 | 1 | 607.67 ms [605.35 ms, 609.92 ms] | 607.67 ms [605.35 ms, 609.92 ms] | 12659.9 [12611.4, 12706.6] | 1.004 [0.990, 1.015] | same |
| explicit_mazars_non_local | stable_time_step | base-t1 | 5 | 5 | 9.63 ms [9.61 ms, 9.70 ms] | 1.93 ms [1.92 ms, 1.94 ms] | 40.1 [40.0, 40.4] | – | same |
| explicit_mazars_non_local | stable_time_step | gather-t5 | 5 | 5 | 9.92 ms [9.90 ms, 9.99 ms] | 1.98 ms [1.98 ms, 2.00 ms] | 41.3 [41.3, 41.6] | 1.029 [1.023, 1.037] | same |
| explicit_mazars_non_local | stable_time_step | serial-t5 | 5 | 5 | 9.97 ms [9.89 ms, 10.04 ms] | 1.99 ms [1.98 ms, 2.01 ms] | 41.6 [41.2, 41.8] | 1.034 [1.021, 1.043] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 1.669 s [1.668 s, 1.682 s] | 41.73 ms [41.69 ms, 42.04 ms] | 869.3 [868.6, 875.8] | – | same |
| explicit_mazars_non_local | solve_step | gather-t5 | 5 | 40 | 893.66 ms [881.44 ms, 895.12 ms] | 22.34 ms [22.04 ms, 22.38 ms] | 465.4 [459.1, 466.2] | 0.534 [0.527, 0.536] | same |
| explicit_mazars_non_local | solve_step | serial-t5 | 5 | 40 | 960.11 ms [946.47 ms, 963.35 ms] | 24.00 ms [23.66 ms, 24.08 ms] | 500.1 [493.0, 501.7] | 0.572 [0.567, 0.578] | same |
| explicit_mazars_non_local | predictor | base-t1 | 5 | 40 | 5.0 µs [2.6 µs, 5.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | gather-t5 | 5 | 40 | 2.5 µs [1.3 µs, 4.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.503 [0.361, 0.807] | same |
| explicit_mazars_non_local | predictor | serial-t5 | 5 | 40 | 2.4 µs [2.1 µs, 2.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.486 [0.358, 1.115] | same |
| explicit_mazars_non_local | residual | base-t1 | 5 | 40 | 1.632 s [1.630 s, 1.644 s] | 40.79 ms [40.75 ms, 41.11 ms] | 849.8 [849.0, 856.4] | – | same |
| explicit_mazars_non_local | residual | gather-t5 | 5 | 40 | 876.85 ms [864.88 ms, 878.50 ms] | 21.92 ms [21.62 ms, 21.96 ms] | 456.7 [450.5, 457.6] | 0.536 [0.528, 0.538] | same |
| explicit_mazars_non_local | residual | serial-t5 | 5 | 40 | 942.68 ms [930.10 ms, 946.90 ms] | 23.57 ms [23.25 ms, 23.67 ms] | 491.0 [484.4, 493.2] | 0.575 [0.570, 0.581] | same |
| explicit_mazars_non_local | lumped_matrix | base-t1 | 5 | 40 | 9.1 µs [8.2 µs, 16.9 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | gather-t5 | 5 | 40 | 11.1 µs [10.2 µs, 11.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.216 [0.666, 1.238] | same |
| explicit_mazars_non_local | lumped_matrix | serial-t5 | 5 | 40 | 11.1 µs [10.0 µs, 11.2 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.209 [0.663, 1.232] | same |
| explicit_mazars_non_local | corrector | base-t1 | 5 | 40 | 6.2 µs [5.4 µs, 8.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | gather-t5 | 5 | 40 | 6.3 µs [5.6 µs, 6.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.941 [0.761, 1.149] | same |
| explicit_mazars_non_local | corrector | serial-t5 | 5 | 40 | 6.1 µs [5.9 µs, 6.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.971 [0.713, 1.290] | same |
| explicit_mazars_non_local | step_hooks | base-t1 | 5 | 80 | 29.66 ms [29.58 ms, 29.82 ms] | 370.7 µs [369.8 µs, 372.8 µs] | 7.7 [7.7, 7.8] | – | same |
| explicit_mazars_non_local | step_hooks | gather-t5 | 5 | 80 | 11.38 ms [11.18 ms, 11.62 ms] | 142.3 µs [139.8 µs, 145.3 µs] | 3.0 [2.9, 3.0] | 0.384 [0.377, 0.390] | same |
| explicit_mazars_non_local | step_hooks | serial-t5 | 5 | 80 | 11.12 ms [11.03 ms, 11.18 ms] | 139.0 µs [137.9 µs, 139.7 µs] | 2.9 [2.9, 2.9] | 0.375 [0.370, 0.376] | same |
| explicit_mazars_non_local | step_rest | base-t1 | 5 | 40 | 7.54 ms [7.38 ms, 7.71 ms] | 188.5 µs [184.5 µs, 192.8 µs] | 3.9 [3.8, 4.0] | – | same |
| explicit_mazars_non_local | step_rest | gather-t5 | 5 | 40 | 5.13 ms [5.06 ms, 5.21 ms] | 128.2 µs [126.6 µs, 130.3 µs] | 2.7 [2.6, 2.7] | 0.680 [0.657, 0.704] | same |
| explicit_mazars_non_local | step_rest | serial-t5 | 5 | 40 | 5.21 ms [5.15 ms, 6.15 ms] | 130.3 µs [128.7 µs, 153.8 µs] | 2.7 [2.7, 3.2] | 0.698 [0.682, 0.798] | same |
| explicit_mazars_non_local | internal_forces | base-t1 | 5 | 8 | 336.80 ms [335.50 ms, 337.17 ms] | 42.10 ms [41.94 ms, 42.15 ms] | 877.1 [873.7, 878.0] | – | same |
| explicit_mazars_non_local | internal_forces | gather-t5 | 5 | 8 | 175.56 ms [174.85 ms, 175.83 ms] | 21.94 ms [21.86 ms, 21.98 ms] | 457.2 [455.3, 457.9] | 0.521 [0.521, 0.522] | same |
| explicit_mazars_non_local | internal_forces | serial-t5 | 5 | 8 | 189.27 ms [187.24 ms, 189.91 ms] | 23.66 ms [23.41 ms, 23.74 ms] | 492.9 [487.6, 494.6] | 0.563 [0.555, 0.566] | same |
| explicit_mazars_non_local | stresses | base-t1 | 5 | 8 | 75.19 ms [75.11 ms, 75.73 ms] | 9.40 ms [9.39 ms, 9.47 ms] | 195.8 [195.6, 197.2] | – | same |
| explicit_mazars_non_local | stresses | gather-t5 | 5 | 8 | 73.18 ms [72.76 ms, 73.40 ms] | 9.15 ms [9.09 ms, 9.18 ms] | 190.6 [189.5, 191.2] | 0.969 [0.966, 0.977] | same |
| explicit_mazars_non_local | stresses | serial-t5 | 5 | 8 | 73.31 ms [72.78 ms, 73.45 ms] | 9.16 ms [9.10 ms, 9.18 ms] | 190.9 [189.5, 191.3] | 0.970 [0.961, 0.977] | same |
| explicit_mazars_non_local | energies | base-t1 | 5 | 8 | 5.49 ms [5.47 ms, 5.50 ms] | 686.6 µs [684.1 µs, 687.8 µs] | 14.3 [14.3, 14.3] | – | same |
| explicit_mazars_non_local | energies | gather-t5 | 5 | 8 | 4.63 ms [4.58 ms, 4.65 ms] | 579.0 µs [571.9 µs, 581.8 µs] | 12.1 [11.9, 12.1] | 0.842 [0.833, 0.848] | same |
| explicit_mazars_non_local | energies | serial-t5 | 5 | 8 | 4.60 ms [4.59 ms, 4.72 ms] | 575.2 µs [574.0 µs, 590.6 µs] | 12.0 [12.0, 12.3] | 0.840 [0.835, 0.860] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t1 | 5 | 2 | 1.184 s [1.180 s, 1.187 s] | 591.86 ms [589.96 ms, 593.66 ms] | 12330.5 [12290.9, 12367.9] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | gather-t5 | 5 | 2 | 1.185 s [1.182 s, 1.188 s] | 592.41 ms [591.07 ms, 594.16 ms] | 12341.8 [12313.9, 12378.4] | 1.003 [0.998, 1.004] | same |
| explicit_mazars_non_local | nonlocal_rebuild | serial-t5 | 5 | 2 | 1.183 s [1.182 s, 1.195 s] | 591.37 ms [591.05 ms, 597.46 ms] | 12320.2 [12313.6, 12447.1] | 1.000 [0.996, 1.013] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 7365a08f4, e49de3d84 | 849f4cd05e9ad628 |
