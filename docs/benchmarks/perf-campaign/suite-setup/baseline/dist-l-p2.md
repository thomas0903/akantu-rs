Suite `setup`, size `l`; variants: `p2`; reference `p2`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | mesh | p2 | 3 | 1 | 1.539 s [1.535 s, 1.558 s] | 1.539 s [1.535 s, 1.558 s] | 1187.3 [1184.6, 1202.5] | same |
| dist_mesh_tet4 | partition | p2 | 3 | 1 | 5.575 s [5.574 s, 5.590 s] | 5.575 s [5.574 s, 5.590 s] | 4301.6 [4301.3, 4312.9] | same |
| dist_mesh_tet4 | distribute | p2 | 3 | 1 | 7.323 s [7.321 s, 7.324 s] | 7.323 s [7.321 s, 7.324 s] | 5650.4 [5649.1, 5651.1] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_mesh_tet4 | 1296000 | 226981 | 0 | 2 | 6b12c7d8c | 19125b7baee33f01 |
