Suite `setup`, size `m`; variants: `p4`; reference `p4`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_facets_tet4 | mesh | p4 | 5 | 1 | 175.48 ms [175.02 ms, 176.40 ms] | 175.48 ms [175.02 ms, 176.40 ms] | 1083.2 [1080.4, 1088.9] | same |
| dist_facets_tet4 | partition | p4 | 5 | 1 | 766.39 ms [748.77 ms, 769.99 ms] | 766.39 ms [748.77 ms, 769.99 ms] | 4730.8 [4622.0, 4753.0] | same |
| dist_facets_tet4 | distribute | p4 | 5 | 1 | 937.85 ms [918.24 ms, 940.64 ms] | 937.85 ms [918.24 ms, 940.64 ms] | 5789.2 [5668.1, 5806.4] | same |
| dist_facets_tet4 | init_mesh_facets | p4 | 5 | 1 | 517.96 ms [517.21 ms, 518.52 ms] | 517.96 ms [517.21 ms, 518.52 ms] | 3197.3 [3192.7, 3200.7] | same |
| dist_mesh_tet4 | mesh | p4 | 5 | 1 | 175.99 ms [175.54 ms, 176.28 ms] | 175.99 ms [175.54 ms, 176.28 ms] | 1086.4 [1083.6, 1088.1] | same |
| dist_mesh_tet4 | partition | p4 | 5 | 1 | 749.13 ms [748.27 ms, 771.97 ms] | 749.13 ms [748.27 ms, 771.97 ms] | 4624.3 [4618.9, 4765.2] | same |
| dist_mesh_tet4 | distribute | p4 | 5 | 1 | 920.31 ms [918.93 ms, 943.01 ms] | 920.31 ms [918.93 ms, 943.01 ms] | 5680.9 [5672.4, 5821.0] | same |
| explicit_step_dist_tet4 | mesh | p4 | 5 | 1 | 176.20 ms [175.09 ms, 177.22 ms] | 176.20 ms [175.09 ms, 177.22 ms] | 1087.7 [1080.8, 1093.9] | same |
| explicit_step_dist_tet4 | partition | p4 | 5 | 1 | 753.81 ms [749.28 ms, 800.37 ms] | 753.81 ms [749.28 ms, 800.37 ms] | 4653.1 [4625.2, 4940.6] | same |
| explicit_step_dist_tet4 | distribute | p4 | 5 | 1 | 924.81 ms [922.59 ms, 971.98 ms] | 924.81 ms [922.59 ms, 971.98 ms] | 5708.7 [5695.0, 5999.9] | same |
| explicit_step_dist_tet4 | init | p4 | 5 | 1 | 62.82 ms [62.33 ms, 64.77 ms] | 62.82 ms [62.33 ms, 64.77 ms] | 387.8 [384.8, 399.8] | same |
| explicit_step_dist_tet4 | solve_step | p4 | 5 | 100 | 795.23 ms [791.36 ms, 804.09 ms] | 7.95 ms [7.91 ms, 8.04 ms] | 49.1 [48.8, 49.6] | same |
| explicit_step_dist_tet4 | energies | p4 | 5 | 200 | 851.56 ms [850.28 ms, 853.18 ms] | 4.26 ms [4.25 ms, 4.27 ms] | 26.3 [26.2, 26.3] | same |
| ghost_elements_tet4 | mesh | p4 | 5 | 1 | 175.58 ms [175.18 ms, 176.39 ms] | 175.58 ms [175.18 ms, 176.39 ms] | 1083.8 [1081.4, 1088.8] | same |
| ghost_elements_tet4 | partition | p4 | 5 | 1 | 765.38 ms [751.35 ms, 769.42 ms] | 765.38 ms [751.35 ms, 769.42 ms] | 4724.5 [4638.0, 4749.5] | same |
| ghost_elements_tet4 | distribute | p4 | 5 | 1 | 936.06 ms [923.12 ms, 940.45 ms] | 936.06 ms [923.12 ms, 940.45 ms] | 5778.1 [5698.3, 5805.2] | same |
| ghost_elements_tet4 | synchronize | p4 | 5 | 5000 | 894.62 ms [876.34 ms, 905.47 ms] | 178.9 µs [175.3 µs, 181.1 µs] | 1.1 [1.1, 1.1] | same |
| ghost_nodes_tet4 | mesh | p4 | 5 | 1 | 175.70 ms [174.61 ms, 176.54 ms] | 175.70 ms [174.61 ms, 176.54 ms] | 1084.6 [1077.8, 1089.7] | same |
| ghost_nodes_tet4 | partition | p4 | 5 | 1 | 753.75 ms [747.71 ms, 769.11 ms] | 753.75 ms [747.71 ms, 769.11 ms] | 4652.8 [4615.5, 4747.6] | same |
| ghost_nodes_tet4 | distribute | p4 | 5 | 1 | 923.79 ms [919.95 ms, 940.46 ms] | 923.79 ms [919.95 ms, 940.46 ms] | 5702.4 [5678.7, 5805.3] | same |
| ghost_nodes_tet4 | reduce_synchronize | p4 | 5 | 5000 | 335.92 ms [323.71 ms, 338.79 ms] | 67.2 µs [64.7 µs, 67.8 µs] | 0.4 [0.4, 0.4] | same |
| reduce_scalars | all_reduce_2 | p4 | 5 | 200000 | 484.51 ms [480.42 ms, 544.93 ms] | 2.4 µs [2.4 µs, 2.7 µs] | – | same |
| reduce_scalars | all_reduce_4096 | p4 | 5 | 20000 | 2.867 s [1.161 s, 2.903 s] | 143.3 µs [58.0 µs, 145.1 µs] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_facets_tet4 | 162000 | 29791 | 0 | 4 | 6b12c7d8c | 44bb22d09431c091 |
| dist_mesh_tet4 | 162000 | 29791 | 0 | 4 | 6b12c7d8c | 51a6a851906ab0b7 |
| explicit_step_dist_tet4 | 162000 | 29791 | 89373 | 4 | 6b12c7d8c | 77efd8ff1b28a554 |
| ghost_elements_tet4 | 162000 | 29791 | 0 | 4 | 6b12c7d8c | 750dd0d7d2db2fb7 |
| ghost_nodes_tet4 | 162000 | 29791 | 0 | 4 | 6b12c7d8c | aef757c868e1fdbb |
| reduce_scalars | 0 | 0 | 0 | 4 | 6b12c7d8c | 5b2a904a560324d1 |
