Suite `setup`, size `l`; variants: `p8`; reference `p8`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | mesh | p8 | 3 | 1 | 1.553 s [1.545 s, 1.557 s] | 1.553 s [1.545 s, 1.557 s] | 1198.4 [1192.4, 1201.0] | same |
| dist_mesh_tet4 | partition | p8 | 3 | 1 | 6.384 s [6.339 s, 6.387 s] | 6.384 s [6.339 s, 6.387 s] | 4926.0 [4891.6, 4927.9] | same |
| dist_mesh_tet4 | distribute | p8 | 3 | 1 | 10.650 s [10.588 s, 10.720 s] | 10.650 s [10.588 s, 10.720 s] | 8217.5 [8169.6, 8271.4] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dist_mesh_tet4 | 1296000 | 226981 | 0 | 8 | 6b12c7d8c | 0835d2ad334ba9f2 |
