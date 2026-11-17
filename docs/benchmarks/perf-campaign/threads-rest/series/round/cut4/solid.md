Suite `solid`, size `m`; variants: `base-t1`, `gather-t4`, `serial-t4`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_mazars_non_local | mesh | base-t1 | 5 | 1 | 1.02 ms [1.02 ms, 1.04 ms] | 1.02 ms [1.02 ms, 1.04 ms] | 21.2 [21.2, 21.7] | – | same |
| explicit_mazars_non_local | mesh | gather-t4 | 5 | 1 | 1.02 ms [1.01 ms, 1.04 ms] | 1.02 ms [1.01 ms, 1.04 ms] | 21.3 [21.0, 21.6] | 1.001 [0.976, 1.010] | same |
| explicit_mazars_non_local | mesh | serial-t4 | 5 | 1 | 1.03 ms [1.02 ms, 1.04 ms] | 1.03 ms [1.02 ms, 1.04 ms] | 21.4 [21.2, 21.7] | 1.003 [0.984, 1.024] | same |
| explicit_mazars_non_local | init | base-t1 | 5 | 1 | 604.12 ms [601.49 ms, 611.05 ms] | 604.12 ms [601.49 ms, 611.05 ms] | 12585.9 [12531.0, 12730.3] | – | same |
| explicit_mazars_non_local | init | gather-t4 | 5 | 1 | 605.48 ms [604.95 ms, 608.77 ms] | 605.48 ms [604.95 ms, 608.77 ms] | 12614.2 [12603.2, 12682.8] | 1.003 [0.996, 1.007] | same |
| explicit_mazars_non_local | init | serial-t4 | 5 | 1 | 610.91 ms [602.26 ms, 615.06 ms] | 610.91 ms [602.26 ms, 615.06 ms] | 12727.3 [12547.1, 12813.7] | 1.011 [0.990, 1.023] | same |
| explicit_mazars_non_local | stable_time_step | base-t1 | 5 | 5 | 9.64 ms [9.62 ms, 9.76 ms] | 1.93 ms [1.92 ms, 1.95 ms] | 40.2 [40.1, 40.7] | – | same |
| explicit_mazars_non_local | stable_time_step | gather-t4 | 5 | 5 | 9.63 ms [9.60 ms, 9.64 ms] | 1.93 ms [1.92 ms, 1.93 ms] | 40.1 [40.0, 40.2] | 0.996 [0.987, 1.002] | same |
| explicit_mazars_non_local | stable_time_step | serial-t4 | 5 | 5 | 9.68 ms [9.61 ms, 9.71 ms] | 1.94 ms [1.92 ms, 1.94 ms] | 40.4 [40.0, 40.4] | 0.997 [0.990, 1.007] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 1.671 s [1.668 s, 1.676 s] | 41.78 ms [41.69 ms, 41.91 ms] | 870.4 [868.6, 873.1] | – | same |
| explicit_mazars_non_local | solve_step | gather-t4 | 5 | 40 | 1.058 s [1.055 s, 1.070 s] | 26.44 ms [26.37 ms, 26.75 ms] | 550.8 [549.5, 557.3] | 0.633 [0.629, 0.640] | same |
| explicit_mazars_non_local | solve_step | serial-t4 | 5 | 40 | 1.001 s [990.28 ms, 1.014 s] | 25.04 ms [24.76 ms, 25.34 ms] | 521.6 [515.8, 528.0] | 0.599 [0.591, 0.608] | same |
| explicit_mazars_non_local | predictor | base-t1 | 5 | 40 | 6.1 µs [3.7 µs, 7.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | predictor | gather-t4 | 5 | 40 | 1.4 µs [1.3 µs, 2.2 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.231 [0.176, 0.593] | same |
| explicit_mazars_non_local | predictor | serial-t4 | 5 | 40 | 2.8 µs [1.5 µs, 3.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.421 [0.292, 0.539] | same |
| explicit_mazars_non_local | residual | base-t1 | 5 | 40 | 1.634 s [1.630 s, 1.639 s] | 40.85 ms [40.76 ms, 40.97 ms] | 851.0 [849.1, 853.6] | – | same |
| explicit_mazars_non_local | residual | gather-t4 | 5 | 40 | 1.041 s [1.038 s, 1.053 s] | 26.02 ms [25.95 ms, 26.32 ms] | 542.0 [540.6, 548.4] | 0.637 [0.633, 0.644] | same |
| explicit_mazars_non_local | residual | serial-t4 | 5 | 40 | 984.78 ms [973.69 ms, 996.57 ms] | 24.62 ms [24.34 ms, 24.91 ms] | 512.9 [507.1, 519.0] | 0.603 [0.594, 0.611] | same |
| explicit_mazars_non_local | lumped_matrix | base-t1 | 5 | 40 | 8.5 µs [8.5 µs, 9.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | lumped_matrix | gather-t4 | 5 | 40 | 10.7 µs [9.9 µs, 11.8 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.198 [1.020, 1.390] | same |
| explicit_mazars_non_local | lumped_matrix | serial-t4 | 5 | 40 | 10.5 µs [10.4 µs, 11.3 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.232 [1.063, 1.333] | same |
| explicit_mazars_non_local | corrector | base-t1 | 5 | 40 | 6.8 µs [6.3 µs, 7.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | gather-t4 | 5 | 40 | 6.5 µs [6.2 µs, 6.6 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.973 [0.913, 0.990] | same |
| explicit_mazars_non_local | corrector | serial-t4 | 5 | 40 | 6.3 µs [6.0 µs, 6.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.968 [0.863, 1.068] | same |
| explicit_mazars_non_local | step_hooks | base-t1 | 5 | 80 | 29.68 ms [29.58 ms, 29.77 ms] | 371.1 µs [369.7 µs, 372.1 µs] | 7.7 [7.7, 7.8] | – | same |
| explicit_mazars_non_local | step_hooks | gather-t4 | 5 | 80 | 11.84 ms [11.81 ms, 12.17 ms] | 148.0 µs [147.7 µs, 152.1 µs] | 3.1 [3.1, 3.2] | 0.399 [0.398, 0.411] | same |
| explicit_mazars_non_local | step_hooks | serial-t4 | 5 | 80 | 11.51 ms [11.33 ms, 11.97 ms] | 143.8 µs [141.7 µs, 149.6 µs] | 3.0 [3.0, 3.1] | 0.386 [0.381, 0.405] | same |
| explicit_mazars_non_local | step_rest | base-t1 | 5 | 40 | 7.52 ms [7.42 ms, 7.63 ms] | 188.0 µs [185.5 µs, 190.7 µs] | 3.9 [3.9, 4.0] | – | same |
| explicit_mazars_non_local | step_rest | gather-t4 | 5 | 40 | 5.08 ms [5.03 ms, 5.11 ms] | 127.0 µs [125.7 µs, 127.7 µs] | 2.6 [2.6, 2.7] | 0.675 [0.666, 0.680] | same |
| explicit_mazars_non_local | step_rest | serial-t4 | 5 | 40 | 5.09 ms [4.95 ms, 5.09 ms] | 127.2 µs [123.7 µs, 127.4 µs] | 2.6 [2.6, 2.7] | 0.668 [0.658, 0.687] | same |
| explicit_mazars_non_local | internal_forces | base-t1 | 5 | 8 | 336.51 ms [335.88 ms, 339.80 ms] | 42.06 ms [41.99 ms, 42.47 ms] | 876.3 [874.7, 884.9] | – | same |
| explicit_mazars_non_local | internal_forces | gather-t4 | 5 | 8 | 209.11 ms [206.14 ms, 213.35 ms] | 26.14 ms [25.77 ms, 26.67 ms] | 544.5 [536.8, 555.6] | 0.623 [0.610, 0.634] | same |
| explicit_mazars_non_local | internal_forces | serial-t4 | 5 | 8 | 197.61 ms [196.03 ms, 199.41 ms] | 24.70 ms [24.50 ms, 24.93 ms] | 514.6 [510.5, 519.3] | 0.588 [0.577, 0.591] | same |
| explicit_mazars_non_local | stresses | base-t1 | 5 | 8 | 75.63 ms [75.10 ms, 75.87 ms] | 9.45 ms [9.39 ms, 9.48 ms] | 196.9 [195.6, 197.6] | – | same |
| explicit_mazars_non_local | stresses | gather-t4 | 5 | 8 | 72.99 ms [72.73 ms, 75.01 ms] | 9.12 ms [9.09 ms, 9.38 ms] | 190.1 [189.4, 195.3] | 0.965 [0.959, 0.999] | same |
| explicit_mazars_non_local | stresses | serial-t4 | 5 | 8 | 73.37 ms [73.21 ms, 73.43 ms] | 9.17 ms [9.15 ms, 9.18 ms] | 191.1 [190.6, 191.2] | 0.970 [0.965, 0.977] | same |
| explicit_mazars_non_local | energies | base-t1 | 5 | 8 | 5.51 ms [5.46 ms, 6.03 ms] | 689.4 µs [682.3 µs, 754.3 µs] | 14.4 [14.2, 15.7] | – | same |
| explicit_mazars_non_local | energies | gather-t4 | 5 | 8 | 4.49 ms [4.43 ms, 4.51 ms] | 560.8 µs [553.6 µs, 563.2 µs] | 11.7 [11.5, 11.7] | 0.808 [0.742, 0.822] | same |
| explicit_mazars_non_local | energies | serial-t4 | 5 | 8 | 4.52 ms [4.41 ms, 4.54 ms] | 564.4 µs [551.3 µs, 567.1 µs] | 11.8 [11.5, 11.8] | 0.822 [0.744, 0.830] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base-t1 | 5 | 2 | 1.176 s [1.174 s, 1.191 s] | 588.09 ms [586.76 ms, 595.53 ms] | 12251.9 [12224.2, 12407.0] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | gather-t4 | 5 | 2 | 1.187 s [1.186 s, 1.190 s] | 593.29 ms [592.86 ms, 595.18 ms] | 12360.3 [12351.2, 12399.6] | 1.008 [0.998, 1.014] | same |
| explicit_mazars_non_local | nonlocal_rebuild | serial-t4 | 5 | 2 | 1.183 s [1.176 s, 1.203 s] | 591.43 ms [588.19 ms, 601.55 ms] | 12321.4 [12253.9, 12532.3] | 1.006 [0.988, 1.025] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 7365a08f4, e49de3d84 | 849f4cd05e9ad628 |
