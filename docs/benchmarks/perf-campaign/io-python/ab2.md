Suite `setup`, size `m`; variants: `after2`, `after3`, `after4`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| dump_paraview_ascii_tet4 | dump | after2 | 5 | 3 | 1.559 s [1.545 s, 1.603 s] | 519.55 ms [514.98 ms, 534.43 ms] | 3207.1 [3178.9, 3299.0] | 0.573 [0.564, 0.584] | same |
| dump_paraview_ascii_tet4 | dump | after3 | 5 | 3 | 1.441 s [1.438 s, 1.477 s] | 480.29 ms [479.27 ms, 492.18 ms] | 2964.7 [2958.5, 3038.1] | 0.529 [0.525, 0.537] | same |
| dump_paraview_ascii_tet4 | dump | after4 | 5 | 3 | 959.97 ms [951.10 ms, 974.81 ms] | 319.99 ms [317.03 ms, 324.94 ms] | 1975.2 [1957.0, 2005.8] | 0.353 [0.348, 0.355] | same |
| dump_paraview_ascii_tet4 | dump | before | 5 | 3 | 2.725 s [2.720 s, 2.747 s] | 908.29 ms [906.52 ms, 915.71 ms] | 5606.7 [5595.8, 5652.5] | – | same |
| dump_paraview_tet4 | dump | after2 | 5 | 15 | 2.191 s [2.144 s, 2.379 s] | 146.09 ms [142.92 ms, 158.59 ms] | 901.8 [882.2, 979.0] | 0.704 [0.699, 0.724] | same |
| dump_paraview_tet4 | dump | after3 | 5 | 15 | 1.971 s [1.943 s, 2.102 s] | 131.41 ms [129.54 ms, 140.11 ms] | 811.2 [799.6, 864.9] | 0.638 [0.609, 0.663] | same |
| dump_paraview_tet4 | dump | after4 | 5 | 15 | 1.970 s [1.956 s, 2.084 s] | 131.32 ms [130.43 ms, 138.93 ms] | 810.6 [805.1, 857.6] | 0.641 [0.600, 0.679] | same |
| dump_paraview_tet4 | dump | before | 5 | 15 | 3.091 s [3.051 s, 3.284 s] | 206.04 ms [203.43 ms, 218.96 ms] | 1271.9 [1255.7, 1351.6] | – | same |
| dump_text_tet4 | dump | after2 | 5 | 30 | 1.389 s [1.381 s, 1.397 s] | 46.32 ms [46.04 ms, 46.58 ms] | 285.9 [284.2, 287.5] | 0.629 [0.618, 0.631] | same |
| dump_text_tet4 | dump | after3 | 5 | 30 | 1.386 s [1.381 s, 1.391 s] | 46.20 ms [46.03 ms, 46.36 ms] | 285.2 [284.1, 286.2] | 0.626 [0.618, 0.628] | same |
| dump_text_tet4 | dump | after4 | 5 | 30 | 511.97 ms [509.94 ms, 517.82 ms] | 17.07 ms [17.00 ms, 17.26 ms] | 105.3 [104.9, 106.5] | 0.231 [0.228, 0.233] | same |
| dump_text_tet4 | dump | before | 5 | 30 | 2.213 s [2.206 s, 2.233 s] | 73.78 ms [73.55 ms, 74.45 ms] | 455.4 [454.0, 459.6] | – | same |
| msh_write_tet4 | write | after2 | 5 | 1 | 29.40 ms [28.94 ms, 29.51 ms] | 29.40 ms [28.94 ms, 29.51 ms] | 181.5 [178.7, 182.2] | 0.535 [0.530, 0.539] | same |
| msh_write_tet4 | write | after3 | 5 | 1 | 29.79 ms [29.20 ms, 31.50 ms] | 29.79 ms [29.20 ms, 31.50 ms] | 183.9 [180.2, 194.4] | 0.538 [0.530, 0.574] | same |
| msh_write_tet4 | write | after4 | 5 | 1 | 25.44 ms [25.22 ms, 25.80 ms] | 25.44 ms [25.22 ms, 25.80 ms] | 157.0 [155.7, 159.3] | 0.463 [0.457, 0.473] | same |
| msh_write_tet4 | write | before | 5 | 1 | 54.83 ms [54.41 ms, 55.41 ms] | 54.83 ms [54.41 ms, 55.41 ms] | 338.5 [335.9, 342.0] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dump_paraview_ascii_tet4 | 162000 | 29791 | 89373 | 1 | 137ba0110, 9d5b044f4, accc48c7e, f373ab7ce | 80dd05d0385d9b9b |
| dump_paraview_tet4 | 162000 | 29791 | 89373 | 1 | 137ba0110, 9d5b044f4, accc48c7e, f373ab7ce | e58603ca45e7a46d |
| dump_text_tet4 | 162000 | 29791 | 89373 | 1 | 137ba0110, 9d5b044f4, accc48c7e, f373ab7ce | 5c6fc3bc16ea68c7 |
| msh_write_tet4 | 162000 | 29791 | 0 | 1 | 137ba0110, 9d5b044f4, accc48c7e, f373ab7ce | 2835a7aec9e209b8 |
