Suite `setup`, size `m`; variants: `p1`; reference `p1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_facets_tet4 | mesh | p1 | 5 | 1 | 174.86 ms [174.34 ms, 176.79 ms] | 174.86 ms [174.34 ms, 176.79 ms] | 1079.4 [1076.2, 1091.3] | same |
| dist_facets_tet4 | partition | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| dist_facets_tet4 | distribute | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| dist_facets_tet4 | init_mesh_facets | p1 | 5 | 1 | 1.813 s [1.811 s, 1.826 s] | 1.813 s [1.811 s, 1.826 s] | 11189.5 [11180.6, 11273.0] | same |
| dist_mesh_tet4 | mesh | p1 | 5 | 1 | 175.42 ms [175.21 ms, 175.91 ms] | 175.42 ms [175.21 ms, 175.91 ms] | 1082.8 [1081.5, 1085.8] | same |
| dist_mesh_tet4 | partition | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| dist_mesh_tet4 | distribute | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_step_dist_tet4 | mesh | p1 | 5 | 1 | 175.17 ms [174.56 ms, 176.66 ms] | 175.17 ms [174.56 ms, 176.66 ms] | 1081.3 [1077.5, 1090.5] | same |
| explicit_step_dist_tet4 | partition | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_step_dist_tet4 | distribute | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| explicit_step_dist_tet4 | init | p1 | 5 | 1 | 133.96 ms [132.40 ms, 134.25 ms] | 133.96 ms [132.40 ms, 134.25 ms] | 826.9 [817.3, 828.7] | same |
| explicit_step_dist_tet4 | solve_step | p1 | 5 | 100 | 1.342 s [1.341 s, 1.345 s] | 13.42 ms [13.41 ms, 13.45 ms] | 82.8 [82.8, 83.1] | same |
| explicit_step_dist_tet4 | energies | p1 | 5 | 200 | 3.180 s [3.179 s, 3.188 s] | 15.90 ms [15.90 ms, 15.94 ms] | 98.1 [98.1, 98.4] | same |
| ghost_elements_tet4 | mesh | p1 | 5 | 1 | 174.73 ms [173.97 ms, 175.58 ms] | 174.73 ms [173.97 ms, 175.58 ms] | 1078.6 [1073.9, 1083.8] | same |
| ghost_elements_tet4 | partition | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| ghost_elements_tet4 | distribute | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| ghost_elements_tet4 | synchronize | p1 | 5 | 5000 | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| ghost_nodes_tet4 | mesh | p1 | 5 | 1 | 176.53 ms [175.06 ms, 177.32 ms] | 176.53 ms [175.06 ms, 177.32 ms] | 1089.7 [1080.6, 1094.5] | same |
| ghost_nodes_tet4 | partition | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| ghost_nodes_tet4 | distribute | p1 | 5 | 1 | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| ghost_nodes_tet4 | reduce_synchronize | p1 | 5 | 5000 | 0.0 µs [0.0 µs, 0.2 µs] | 0.0 µs [0.0 µs, 0.0 µs] | 0.0 [0.0, 0.0] | same |
| reduce_scalars | all_reduce_2 | p1 | 5 | 200000 | 109.52 ms [109.23 ms, 129.16 ms] | 0.5 µs [0.5 µs, 0.6 µs] | – | same |
| reduce_scalars | all_reduce_4096 | p1 | 5 | 20000 | 151.13 ms [150.66 ms, 152.44 ms] | 7.6 µs [7.5 µs, 7.6 µs] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_facets_tet4 | 162000 | 29791 | 0 | 1 | 6b12c7d8c | 5a818cc6d44b3c0e |
| dist_mesh_tet4 | 162000 | 29791 | 0 | 1 | 6b12c7d8c | 1e111e2a3160236b |
| explicit_step_dist_tet4 | 162000 | 29791 | 89373 | 1 | 6b12c7d8c | 91aef538ce1f3d2d |
| ghost_elements_tet4 | 162000 | 29791 | 0 | 1 | 6b12c7d8c | e167b824bc05cfc4 |
| ghost_nodes_tet4 | 162000 | 29791 | 0 | 1 | 6b12c7d8c | 9e153b12205f3d73 |
| reduce_scalars | 0 | 0 | 0 | 1 | 6b12c7d8c | 277584880ec5f062 |
