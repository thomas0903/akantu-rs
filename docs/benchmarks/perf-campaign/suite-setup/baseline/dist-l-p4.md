Suite `setup`, size `l`; variants: `p4`; reference `p4`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | mesh | p4 | 3 | 1 | 1.538 s [1.532 s, 1.555 s] | 1.538 s [1.532 s, 1.555 s] | 1186.7 [1182.0, 1199.8] | same |
| dist_mesh_tet4 | partition | p4 | 3 | 1 | 5.934 s [5.906 s, 5.950 s] | 5.934 s [5.906 s, 5.950 s] | 4578.5 [4556.8, 4590.9] | same |
| dist_mesh_tet4 | distribute | p4 | 3 | 1 | 8.462 s [8.462 s, 8.501 s] | 8.462 s [8.462 s, 8.501 s] | 6529.6 [6529.3, 6559.5] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_mesh_tet4 | 1296000 | 226981 | 0 | 4 | 6b12c7d8c | 01d363536c9314d9 |
