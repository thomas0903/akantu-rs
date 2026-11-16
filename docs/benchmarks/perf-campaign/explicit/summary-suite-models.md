Suite `models`, size `m`; variants: `after`, `base`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_explicit_quad4 | solve_step | after | 5 | 200 | 4.537 s [4.517 s, 4.579 s] | 22.68 ms [22.58 ms, 22.90 ms] | 180.0 [179.2, 181.7] | 0.948 [0.944, 0.951] | same |
| contact_explicit_quad4 | solve_step | base | 5 | 200 | 4.780 s [4.765 s, 4.834 s] | 23.90 ms [23.83 ms, 24.17 ms] | 189.7 [189.1, 191.8] | – | same |
| heat_explicit_tet4 | solve_step | after | 5 | 100 | 2.891 s [2.882 s, 2.911 s] | 28.91 ms [28.82 ms, 29.11 ms] | 103.3 [102.9, 104.0] | 0.926 [0.923, 0.941] | same |
| heat_explicit_tet4 | solve_step | base | 5 | 100 | 3.113 s [3.093 s, 3.135 s] | 31.13 ms [30.93 ms, 31.35 ms] | 111.2 [110.5, 112.0] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_explicit_quad4 | 126000 | 128222 | 256444 | 1 | 1fe1e551a, cac5d2cda | 73e756218136a9e6 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 1fe1e551a, cac5d2cda | b4798ad9264a3e5e |
