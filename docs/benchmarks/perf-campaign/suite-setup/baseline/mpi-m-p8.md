Suite `setup`, size `m`; variants: `p8`; reference `p8`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_facets_tet4 | mesh | p8 | 5 | 1 | 176.08 ms [174.93 ms, 177.64 ms] | 176.08 ms [174.93 ms, 177.64 ms] | 1086.9 [1079.8, 1096.6] | same |
| dist_facets_tet4 | partition | p8 | 5 | 1 | 836.72 ms [830.36 ms, 838.63 ms] | 836.72 ms [830.36 ms, 838.63 ms] | 5164.9 [5125.7, 5176.7] | same |
| dist_facets_tet4 | distribute | p8 | 5 | 1 | 1.084 s [1.079 s, 1.088 s] | 1.084 s [1.079 s, 1.088 s] | 6692.3 [6660.0, 6715.0] | same |
| dist_facets_tet4 | init_mesh_facets | p8 | 5 | 1 | 276.09 ms [274.96 ms, 277.11 ms] | 276.09 ms [274.96 ms, 277.11 ms] | 1704.3 [1697.3, 1710.6] | same |
| dist_mesh_tet4 | mesh | p8 | 5 | 1 | 176.34 ms [175.19 ms, 183.03 ms] | 176.34 ms [175.19 ms, 183.03 ms] | 1088.5 [1081.4, 1129.8] | same |
| dist_mesh_tet4 | partition | p8 | 5 | 1 | 833.68 ms [831.66 ms, 851.71 ms] | 833.68 ms [831.66 ms, 851.71 ms] | 5146.2 [5133.7, 5257.5] | same |
| dist_mesh_tet4 | distribute | p8 | 5 | 1 | 1.083 s [1.081 s, 1.101 s] | 1.083 s [1.081 s, 1.101 s] | 6684.0 [6672.4, 6794.2] | same |
| explicit_step_dist_tet4 | mesh | p8 | 5 | 1 | 177.08 ms [176.72 ms, 179.12 ms] | 177.08 ms [176.72 ms, 179.12 ms] | 1093.1 [1090.8, 1105.7] | same |
| explicit_step_dist_tet4 | partition | p8 | 5 | 1 | 852.28 ms [831.46 ms, 854.72 ms] | 852.28 ms [831.46 ms, 854.72 ms] | 5261.0 [5132.5, 5276.1] | same |
| explicit_step_dist_tet4 | distribute | p8 | 5 | 1 | 1.101 s [1.080 s, 1.126 s] | 1.101 s [1.080 s, 1.126 s] | 6796.0 [6668.3, 6951.4] | same |
| explicit_step_dist_tet4 | init | p8 | 5 | 1 | 34.70 ms [34.55 ms, 102.67 ms] | 34.70 ms [34.55 ms, 102.67 ms] | 214.2 [213.3, 633.8] | same |
| explicit_step_dist_tet4 | solve_step | p8 | 5 | 100 | 767.34 ms [761.14 ms, 796.07 ms] | 7.67 ms [7.61 ms, 7.96 ms] | 47.4 [47.0, 49.1] | same |
| explicit_step_dist_tet4 | energies | p8 | 5 | 200 | 478.75 ms [475.47 ms, 527.89 ms] | 2.39 ms [2.38 ms, 2.64 ms] | 14.8 [14.7, 16.3] | same |
| ghost_elements_tet4 | mesh | p8 | 5 | 1 | 175.57 ms [175.25 ms, 176.11 ms] | 175.57 ms [175.25 ms, 176.11 ms] | 1083.8 [1081.8, 1087.1] | same |
| ghost_elements_tet4 | partition | p8 | 5 | 1 | 849.68 ms [831.09 ms, 860.69 ms] | 849.68 ms [831.09 ms, 860.69 ms] | 5244.9 [5130.2, 5312.9] | same |
| ghost_elements_tet4 | distribute | p8 | 5 | 1 | 1.099 s [1.081 s, 1.110 s] | 1.099 s [1.081 s, 1.110 s] | 6783.8 [6673.5, 6853.1] | same |
| ghost_elements_tet4 | synchronize | p8 | 5 | 5000 | 822.75 ms [811.57 ms, 836.70 ms] | 164.5 µs [162.3 µs, 167.3 µs] | 1.0 [1.0, 1.0] | same |
| ghost_nodes_tet4 | mesh | p8 | 5 | 1 | 176.04 ms [175.55 ms, 176.33 ms] | 176.04 ms [175.55 ms, 176.33 ms] | 1086.7 [1083.7, 1088.4] | same |
| ghost_nodes_tet4 | partition | p8 | 5 | 1 | 837.50 ms [832.47 ms, 847.24 ms] | 837.50 ms [832.47 ms, 847.24 ms] | 5169.7 [5138.7, 5229.8] | same |
| ghost_nodes_tet4 | distribute | p8 | 5 | 1 | 1.085 s [1.081 s, 1.097 s] | 1.085 s [1.081 s, 1.097 s] | 6695.0 [6670.0, 6769.5] | same |
| ghost_nodes_tet4 | reduce_synchronize | p8 | 5 | 5000 | 344.18 ms [337.01 ms, 349.45 ms] | 68.8 µs [67.4 µs, 69.9 µs] | 0.4 [0.4, 0.4] | same |
| reduce_scalars | all_reduce_2 | p8 | 5 | 200000 | 932.19 ms [921.72 ms, 953.58 ms] | 4.7 µs [4.6 µs, 4.8 µs] | – | same |
| reduce_scalars | all_reduce_4096 | p8 | 5 | 20000 | 7.118 s [6.989 s, 7.502 s] | 355.9 µs [349.4 µs, 375.1 µs] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_facets_tet4 | 162000 | 29791 | 0 | 8 | 6b12c7d8c | 11351ae40c227eaf |
| dist_mesh_tet4 | 162000 | 29791 | 0 | 8 | 6b12c7d8c | a8f27190b07b5f2b |
| explicit_step_dist_tet4 | 162000 | 29791 | 89373 | 8 | 6b12c7d8c | f7aed0e9096c9f8d |
| ghost_elements_tet4 | 162000 | 29791 | 0 | 8 | 6b12c7d8c | 28d211b579ba41bf |
| ghost_nodes_tet4 | 162000 | 29791 | 0 | 8 | 6b12c7d8c | 6f09dfbe5d6e56a6 |
| reduce_scalars | 0 | 0 | 0 | 8 | 6b12c7d8c | ecdb5aadb974b097 |
