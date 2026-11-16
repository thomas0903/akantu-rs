Suite `setup`, size `l`; variants: `base`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| cohesive_insert_tet4 | mesh | base | 3 | 1 | 1.581 s [1.570 s, 1.606 s] | 1.581 s [1.570 s, 1.606 s] | 1220.2 [1211.3, 1239.4] | same |
| cohesive_insert_tet4 | init_mesh_facets | base | 3 | 1 | 21.969 s [21.895 s, 21.982 s] | 21.969 s [21.895 s, 21.982 s] | 16951.2 [16894.2, 16961.2] | same |
| cohesive_insert_tet4 | flag_facets | base | 3 | 1 | 153.58 ms [152.96 ms, 154.36 ms] | 153.58 ms [152.96 ms, 154.36 ms] | 118.5 [118.0, 119.1] | same |
| cohesive_insert_tet4 | insert | base | 3 | 1 | 33.395 s [33.205 s, 33.436 s] | 33.395 s [33.205 s, 33.436 s] | 25767.4 [25620.9, 25799.3] | same |
| cohesive_insert_tet4 | update_insertion_facets | base | 3 | 1 | 372.3 µs [364.2 µs, 389.4 µs] | 372.3 µs [364.2 µs, 389.4 µs] | 0.3 [0.3, 0.3] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_insert_tet4 | 1296000 | 226981 | 0 | 1 | e2bebff49 | 649fdb8539455608 |
