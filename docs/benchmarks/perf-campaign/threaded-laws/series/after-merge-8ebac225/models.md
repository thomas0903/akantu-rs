Suite `models`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_tet4 | solve_step | base-t1 | 5 | 100 | 883.84 ms [882.29 ms, 890.85 ms] | 8.84 ms [8.82 ms, 8.91 ms] | 31.6 [31.5, 31.8] | – | same |
| heat_explicit_tet4 | solve_step | head-t1 | 5 | 100 | 881.97 ms [880.51 ms, 883.73 ms] | 8.82 ms [8.81 ms, 8.84 ms] | 31.5 [31.5, 31.6] | 0.996 [0.992, 0.999] | same |
| heat_explicit_tet4 | solve_step | head-t2 | 5 | 100 | 528.75 ms [512.64 ms, 529.70 ms] | 5.29 ms [5.13 ms, 5.30 ms] | 18.9 [18.3, 18.9] | 0.598 [0.575, 0.600] | same |
| heat_explicit_tet4 | solve_step | head-t4 | 5 | 100 | 359.78 ms [358.93 ms, 359.98 ms] | 3.60 ms [3.59 ms, 3.60 ms] | 12.9 [12.8, 12.9] | 0.407 [0.404, 0.408] | same |
| heat_explicit_tet4 | solve_step | head-t8 | 5 | 100 | 307.12 ms [306.73 ms, 307.42 ms] | 3.07 ms [3.07 ms, 3.07 ms] | 11.0 [11.0, 11.0] | 0.347 [0.345, 0.348] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 3443d417b, 8ebac2254 | b4798ad9264a3e5e |
