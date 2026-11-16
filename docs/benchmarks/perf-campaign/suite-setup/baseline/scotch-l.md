Suite `setup`, size `l`; variants: `mpi1`; reference `mpi1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| partition_rcb_tet4 | mesh | mpi1 | 3 | 1 | 1.555 s [1.544 s, 1.557 s] | 1.555 s [1.544 s, 1.557 s] | 1199.9 [1191.6, 1201.6] | same |
| partition_rcb_tet4 | partition | mpi1 | 3 | 1 | 880.60 ms [878.26 ms, 896.26 ms] | 880.60 ms [878.26 ms, 896.26 ms] | 679.5 [677.7, 691.6] | same |
| partition_scotch_tet4 | mesh | mpi1 | 3 | 1 | 1.551 s [1.534 s, 1.559 s] | 1.551 s [1.534 s, 1.559 s] | 1196.4 [1183.6, 1202.7] | same |
| partition_scotch_tet4 | partition | mpi1 | 3 | 1 | 6.428 s [6.421 s, 6.459 s] | 6.428 s [6.421 s, 6.459 s] | 4959.5 [4954.3, 4983.8] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| partition_rcb_tet4 | 1296000 | 226981 | 0 | 1 | 6b12c7d8c | 23ea53e63054b366 |
| partition_scotch_tet4 | 1296000 | 226981 | 0 | 1 | 6b12c7d8c | c9baef28ec4aa5f3 |
