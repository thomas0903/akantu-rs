Suite `solid`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 398.6 µs [391.9 µs, 416.4 µs] | 398.6 µs [391.9 µs, 416.4 µs] | 110.7 [108.9, 115.7] | – | same |
| cohesive_intrinsic_implicit | mesh | t1 | 5 | 1 | 413.1 µs [407.0 µs, 417.3 µs] | 413.1 µs [407.0 µs, 417.3 µs] | 114.8 [113.1, 115.9] | 1.040 [0.989, 1.054] | same |
| cohesive_intrinsic_implicit | mesh | t2 | 5 | 1 | 396.9 µs [389.6 µs, 413.0 µs] | 396.9 µs [389.6 µs, 413.0 µs] | 110.2 [108.2, 114.7] | 1.003 [0.937, 1.021] | same |
| cohesive_intrinsic_implicit | mesh | t4 | 5 | 1 | 403.9 µs [392.2 µs, 414.8 µs] | 403.9 µs [392.2 µs, 414.8 µs] | 112.2 [108.9, 115.2] | 1.001 [0.964, 1.036] | same |
| cohesive_intrinsic_implicit | mesh | t8 | 5 | 1 | 393.9 µs [373.6 µs, 419.7 µs] | 393.9 µs [373.6 µs, 419.7 µs] | 109.4 [103.8, 116.6] | 0.988 [0.897, 1.071] | same |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 10.09 ms [9.99 ms, 10.38 ms] | 10.09 ms [9.99 ms, 10.38 ms] | 2803.3 [2775.3, 2884.6] | – | same |
| cohesive_intrinsic_implicit | init | t1 | 5 | 1 | 9.98 ms [9.92 ms, 10.15 ms] | 9.98 ms [9.92 ms, 10.15 ms] | 2771.6 [2756.2, 2820.4] | 0.990 [0.957, 1.016] | same |
| cohesive_intrinsic_implicit | init | t2 | 5 | 1 | 10.16 ms [10.07 ms, 10.41 ms] | 10.16 ms [10.07 ms, 10.41 ms] | 2821.9 [2797.5, 2891.9] | 1.010 [0.983, 1.020] | same |
| cohesive_intrinsic_implicit | init | t4 | 5 | 1 | 10.26 ms [10.10 ms, 10.30 ms] | 10.26 ms [10.10 ms, 10.30 ms] | 2850.7 [2806.2, 2861.7] | 1.006 [0.979, 1.031] | same |
| cohesive_intrinsic_implicit | init | t8 | 5 | 1 | 10.21 ms [10.06 ms, 10.84 ms] | 10.21 ms [10.06 ms, 10.84 ms] | 2835.1 [2793.8, 3011.7] | 1.020 [0.969, 1.085] | same |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 3.240 s [3.211 s, 3.288 s] | 107.99 ms [107.03 ms, 109.60 ms] | 29996.6 [29730.2, 30445.4] | – | same |
| cohesive_intrinsic_implicit | solve_step | t1 | 5 | 30 | 3.240 s [3.154 s, 3.248 s] | 107.99 ms [105.13 ms, 108.27 ms] | 29996.3 [29203.1, 30076.3] | 0.995 [0.982, 1.006] | same |
| cohesive_intrinsic_implicit | solve_step | t2 | 5 | 30 | 3.188 s [3.158 s, 3.340 s] | 106.26 ms [105.26 ms, 111.33 ms] | 29516.7 [29239.0, 30924.5] | 0.981 [0.979, 1.031] | same |
| cohesive_intrinsic_implicit | solve_step | t4 | 5 | 30 | 3.097 s [3.073 s, 3.299 s] | 103.22 ms [102.43 ms, 109.97 ms] | 28671.6 [28453.7, 30548.5] | 0.952 [0.949, 1.024] | same |
| cohesive_intrinsic_implicit | solve_step | t8 | 5 | 30 | 3.242 s [3.060 s, 3.248 s] | 108.08 ms [102.02 ms, 108.27 ms] | 30020.8 [28337.5, 30075.2] | 0.998 [0.937, 1.010] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 72337cb4e, f28c5a7bc | 88b89af4f4608998 |
