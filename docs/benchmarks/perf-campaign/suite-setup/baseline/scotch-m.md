Suite `setup`, size `m`; variants: `mpi1`; reference `mpi1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| partition_rcb_tet4 | mesh | mpi1 | 5 | 1 | 174.76 ms [173.47 ms, 177.37 ms] | 174.76 ms [173.47 ms, 177.37 ms] | 1078.8 [1070.8, 1094.9] | same |
| partition_rcb_tet4 | partition | mpi1 | 5 | 1 | 79.13 ms [78.96 ms, 79.36 ms] | 79.13 ms [78.96 ms, 79.36 ms] | 488.5 [487.4, 489.9] | same |
| partition_scotch_tet4 | mesh | mpi1 | 5 | 1 | 175.01 ms [174.09 ms, 176.38 ms] | 175.01 ms [174.09 ms, 176.38 ms] | 1080.3 [1074.6, 1088.8] | same |
| partition_scotch_tet4 | partition | mpi1 | 5 | 1 | 839.60 ms [831.71 ms, 844.64 ms] | 839.60 ms [831.71 ms, 844.64 ms] | 5182.7 [5134.0, 5213.8] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| partition_rcb_tet4 | 162000 | 29791 | 0 | 1 | 6b12c7d8c | 2ddb9a95d22fb5e6 |
| partition_scotch_tet4 | 162000 | 29791 | 0 | 1 | 6b12c7d8c | 8389d3c23e68ba2f |
