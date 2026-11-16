Suite `setup`, size `m`; variants: `p2`; reference `p2`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_facets_tet4 | mesh | p2 | 5 | 1 | 175.80 ms [175.26 ms, 176.21 ms] | 175.80 ms [175.26 ms, 176.21 ms] | 1085.2 [1081.8, 1087.7] | same |
| dist_facets_tet4 | partition | p2 | 5 | 1 | 673.95 ms [669.26 ms, 678.04 ms] | 673.95 ms [669.26 ms, 678.04 ms] | 4160.2 [4131.2, 4185.4] | same |
| dist_facets_tet4 | distribute | p2 | 5 | 1 | 806.25 ms [801.71 ms, 810.51 ms] | 806.25 ms [801.71 ms, 810.51 ms] | 4976.8 [4948.8, 5003.2] | same |
| dist_facets_tet4 | init_mesh_facets | p2 | 5 | 1 | 976.95 ms [976.13 ms, 978.73 ms] | 976.95 ms [976.13 ms, 978.73 ms] | 6030.6 [6025.5, 6041.6] | same |
| dist_mesh_tet4 | mesh | p2 | 5 | 1 | 175.46 ms [174.90 ms, 176.52 ms] | 175.46 ms [174.90 ms, 176.52 ms] | 1083.1 [1079.6, 1089.7] | same |
| dist_mesh_tet4 | partition | p2 | 5 | 1 | 675.64 ms [669.38 ms, 684.12 ms] | 675.64 ms [669.38 ms, 684.12 ms] | 4170.6 [4132.0, 4222.9] | same |
| dist_mesh_tet4 | distribute | p2 | 5 | 1 | 808.72 ms [803.02 ms, 817.76 ms] | 808.72 ms [803.02 ms, 817.76 ms] | 4992.1 [4956.9, 5047.9] | same |
| explicit_step_dist_tet4 | mesh | p2 | 5 | 1 | 175.77 ms [175.09 ms, 177.57 ms] | 175.77 ms [175.09 ms, 177.57 ms] | 1085.0 [1080.8, 1096.1] | same |
| explicit_step_dist_tet4 | partition | p2 | 5 | 1 | 674.05 ms [667.52 ms, 711.48 ms] | 674.05 ms [667.52 ms, 711.48 ms] | 4160.8 [4120.5, 4391.9] | same |
| explicit_step_dist_tet4 | distribute | p2 | 5 | 1 | 805.99 ms [799.80 ms, 844.81 ms] | 805.99 ms [799.80 ms, 844.81 ms] | 4975.2 [4937.0, 5214.9] | same |
| explicit_step_dist_tet4 | init | p2 | 5 | 1 | 107.31 ms [107.25 ms, 108.01 ms] | 107.31 ms [107.25 ms, 108.01 ms] | 662.4 [662.1, 666.7] | same |
| explicit_step_dist_tet4 | solve_step | p2 | 5 | 100 | 970.25 ms [949.84 ms, 993.54 ms] | 9.70 ms [9.50 ms, 9.94 ms] | 59.9 [58.6, 61.3] | same |
| explicit_step_dist_tet4 | energies | p2 | 5 | 200 | 1.590 s [1.588 s, 1.593 s] | 7.95 ms [7.94 ms, 7.97 ms] | 49.1 [49.0, 49.2] | same |
| ghost_elements_tet4 | mesh | p2 | 5 | 1 | 175.88 ms [175.57 ms, 176.91 ms] | 175.88 ms [175.57 ms, 176.91 ms] | 1085.7 [1083.8, 1092.0] | same |
| ghost_elements_tet4 | partition | p2 | 5 | 1 | 675.53 ms [669.10 ms, 687.82 ms] | 675.53 ms [669.10 ms, 687.82 ms] | 4169.9 [4130.2, 4245.8] | same |
| ghost_elements_tet4 | distribute | p2 | 5 | 1 | 808.22 ms [801.55 ms, 820.42 ms] | 808.22 ms [801.55 ms, 820.42 ms] | 4989.0 [4947.9, 5064.3] | same |
| ghost_elements_tet4 | synchronize | p2 | 5 | 5000 | 782.50 ms [776.20 ms, 790.75 ms] | 156.5 µs [155.2 µs, 158.2 µs] | 1.0 [1.0, 1.0] | same |
| ghost_nodes_tet4 | mesh | p2 | 5 | 1 | 175.71 ms [174.89 ms, 176.98 ms] | 175.71 ms [174.89 ms, 176.98 ms] | 1084.6 [1079.6, 1092.5] | same |
| ghost_nodes_tet4 | partition | p2 | 5 | 1 | 671.85 ms [671.17 ms, 681.73 ms] | 671.85 ms [671.17 ms, 681.73 ms] | 4147.2 [4143.0, 4208.2] | same |
| ghost_nodes_tet4 | distribute | p2 | 5 | 1 | 804.86 ms [802.74 ms, 814.56 ms] | 804.86 ms [802.74 ms, 814.56 ms] | 4968.2 [4955.2, 5028.1] | same |
| ghost_nodes_tet4 | reduce_synchronize | p2 | 5 | 5000 | 287.29 ms [272.36 ms, 291.02 ms] | 57.5 µs [54.5 µs, 58.2 µs] | 0.4 [0.3, 0.4] | same |
| reduce_scalars | all_reduce_2 | p2 | 5 | 200000 | 271.99 ms [267.88 ms, 284.09 ms] | 1.4 µs [1.3 µs, 1.4 µs] | – | same |
| reduce_scalars | all_reduce_4096 | p2 | 5 | 20000 | 493.05 ms [490.64 ms, 495.10 ms] | 24.7 µs [24.5 µs, 24.8 µs] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_facets_tet4 | 162000 | 29791 | 0 | 2 | 6b12c7d8c | b737801c5ccde0d1 |
| dist_mesh_tet4 | 162000 | 29791 | 0 | 2 | 6b12c7d8c | 369f1715efdd42cc |
| explicit_step_dist_tet4 | 162000 | 29791 | 89373 | 2 | 6b12c7d8c | b638c8269913615e |
| ghost_elements_tet4 | 162000 | 29791 | 0 | 2 | 6b12c7d8c | c322b9e1fb8d94bc |
| ghost_nodes_tet4 | 162000 | 29791 | 0 | 2 | 6b12c7d8c | d348ddf3bb82fb83 |
| reduce_scalars | 0 | 0 | 0 | 2 | 6b12c7d8c | f4950cd8e945cbe0 |
