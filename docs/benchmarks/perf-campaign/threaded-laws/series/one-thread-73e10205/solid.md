Suite `solid`, size `m`; variants: `base-t1`, `head-t1`, `head2-t1`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_marigo | solve_step | base-t1 | 5 | 200 | 2.263 s [2.252 s, 2.271 s] | 11.32 ms [11.26 ms, 11.36 ms] | 107.3 [106.8, 107.7] | – | same |
| explicit_marigo | solve_step | head-t1 | 5 | 200 | 2.189 s [2.177 s, 2.196 s] | 10.95 ms [10.89 ms, 10.98 ms] | 103.8 [103.2, 104.1] | 0.966 [0.964, 0.970] | same |
| explicit_marigo | solve_step | head2-t1 | 5 | 200 | 2.201 s [2.198 s, 2.247 s] | 11.01 ms [10.99 ms, 11.23 ms] | 104.4 [104.2, 106.5] | 0.977 [0.969, 0.990] | same |
| explicit_maxwell | solve_step | base-t1 | 5 | 40 | 1.559 s [1.558 s, 1.588 s] | 38.99 ms [38.95 ms, 39.70 ms] | 369.7 [369.4, 376.5] | – | same |
| explicit_maxwell | solve_step | head-t1 | 5 | 40 | 1.620 s [1.610 s, 1.640 s] | 40.49 ms [40.25 ms, 41.00 ms] | 384.0 [381.7, 388.8] | 1.034 [1.025, 1.052] | same |
| explicit_maxwell | solve_step | head2-t1 | 5 | 40 | 1.541 s [1.539 s, 1.555 s] | 38.53 ms [38.48 ms, 38.88 ms] | 365.3 [364.9, 368.7] | 0.987 [0.979, 0.998] | same |
| explicit_plastic | solve_step | base-t1 | 5 | 120 | 2.747 s [2.716 s, 2.769 s] | 22.90 ms [22.64 ms, 23.07 ms] | 217.1 [214.6, 218.8] | – | same |
| explicit_plastic | solve_step | head-t1 | 5 | 120 | 2.640 s [2.607 s, 2.715 s] | 22.00 ms [21.72 ms, 22.62 ms] | 208.6 [206.0, 214.5] | 0.965 [0.945, 0.990] | same |
| explicit_plastic | solve_step | head2-t1 | 5 | 120 | 2.754 s [2.739 s, 2.780 s] | 22.95 ms [22.82 ms, 23.17 ms] | 217.6 [216.4, 219.7] | 1.004 [0.990, 1.012] | same |
| explicit_sls | solve_step | base-t1 | 5 | 120 | 2.457 s [2.418 s, 2.524 s] | 20.47 ms [20.15 ms, 21.03 ms] | 194.1 [191.1, 199.5] | – | same |
| explicit_sls | solve_step | head-t1 | 5 | 120 | 2.526 s [2.478 s, 2.542 s] | 21.05 ms [20.65 ms, 21.19 ms] | 199.6 [195.8, 200.9] | 1.021 [0.989, 1.046] | same |
| explicit_sls | solve_step | head2-t1 | 5 | 120 | 2.557 s [2.500 s, 2.575 s] | 21.31 ms [20.83 ms, 21.46 ms] | 202.1 [197.5, 203.5] | 1.034 [0.993, 1.059] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 73e102050, 896d0f46b, c9eef15b8 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 73e102050, 896d0f46b, c9eef15b8 | 67385e0a3d462ed8 |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 73e102050, 896d0f46b, c9eef15b8 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 73e102050, 896d0f46b, c9eef15b8 | 36137566d8b810bb |
