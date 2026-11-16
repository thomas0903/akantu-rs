Suite `setup`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| dump_paraview_ascii_tet4 | dump | after | 5 | 3 | 888.57 ms [856.13 ms, 933.59 ms] | 296.19 ms [285.38 ms, 311.20 ms] | 1828.3 [1761.6, 1921.0] | 0.327 [0.313, 0.332] | same |
| dump_paraview_ascii_tet4 | dump | before | 5 | 3 | 2.739 s [2.713 s, 2.815 s] | 912.84 ms [904.48 ms, 938.23 ms] | 5634.8 [5583.2, 5791.5] | – | same |
| dump_paraview_tet4 | dump | after | 5 | 15 | 1.488 s [1.432 s, 1.519 s] | 99.19 ms [95.48 ms, 101.27 ms] | 612.3 [589.4, 625.1] | 0.480 [0.462, 0.487] | same |
| dump_paraview_tet4 | dump | before | 5 | 15 | 3.126 s [2.981 s, 3.150 s] | 208.43 ms [198.76 ms, 210.00 ms] | 1286.6 [1226.9, 1296.3] | – | same |
| dump_text_tet4 | dump | after | 5 | 30 | 517.25 ms [506.66 ms, 517.77 ms] | 17.24 ms [16.89 ms, 17.26 ms] | 106.4 [104.3, 106.5] | 0.233 [0.230, 0.235] | same |
| dump_text_tet4 | dump | before | 5 | 30 | 2.213 s [2.202 s, 2.220 s] | 73.77 ms [73.40 ms, 74.01 ms] | 455.4 [453.1, 456.9] | – | same |
| msh22_read_hex8 | read | after | 5 | 1 | 101.02 ms [100.16 ms, 102.88 ms] | 101.02 ms [100.16 ms, 102.88 ms] | 808.2 [801.3, 823.0] | 0.514 [0.502, 0.524] | same |
| msh22_read_hex8 | read | before | 5 | 1 | 196.70 ms [192.75 ms, 200.54 ms] | 196.70 ms [192.75 ms, 200.54 ms] | 1573.6 [1542.0, 1604.3] | – | same |
| msh22_read_tet4 | read | after | 5 | 1 | 84.77 ms [84.56 ms, 85.85 ms] | 84.77 ms [84.56 ms, 85.85 ms] | 523.3 [522.0, 529.9] | 0.560 [0.546, 0.566] | same |
| msh22_read_tet4 | read | before | 5 | 1 | 153.05 ms [151.08 ms, 155.40 ms] | 153.05 ms [151.08 ms, 155.40 ms] | 944.8 [932.6, 959.3] | – | same |
| msh41_read_tet4 | read | after | 5 | 1 | 73.92 ms [73.27 ms, 74.43 ms] | 73.92 ms [73.27 ms, 74.43 ms] | 456.3 [452.3, 459.5] | 0.573 [0.563, 0.590] | same |
| msh41_read_tet4 | read | before | 5 | 1 | 128.55 ms [124.13 ms, 131.31 ms] | 128.55 ms [124.13 ms, 131.31 ms] | 793.5 [766.3, 810.6] | – | same |
| msh_write_tet4 | write | after | 5 | 1 | 25.76 ms [25.46 ms, 26.42 ms] | 25.76 ms [25.46 ms, 26.42 ms] | 159.0 [157.2, 163.1] | 0.481 [0.466, 0.489] | same |
| msh_write_tet4 | write | before | 5 | 1 | 53.89 ms [53.03 ms, 54.63 ms] | 53.89 ms [53.03 ms, 54.63 ms] | 332.7 [327.4, 337.2] | – | same |
| parser_chain | parse | after | 5 | 1 | 24.5 µs [23.5 µs, 30.7 µs] | 24.5 µs [23.5 µs, 30.7 µs] | 1165.7 [1117.6, 1461.5] | 0.997 [0.994, 1.301] | same |
| parser_chain | parse | before | 5 | 1 | 24.2 µs [23.5 µs, 24.6 µs] | 24.2 µs [23.5 µs, 24.6 µs] | 1152.9 [1120.5, 1172.9] | – | same |
| parser_chain | evaluate_last | after | 5 | 5 | 35.6 µs [35.1 µs, 50.2 µs] | 7.1 µs [7.0 µs, 10.0 µs] | 338.9 [334.0, 478.4] | 0.000 [0.000, 0.000] | same |
| parser_chain | evaluate_last | before | 5 | 5 | 2.001 s [1.970 s, 2.046 s] | 400.15 ms [394.03 ms, 409.28 ms] | 19054912.5 [18763437.8, 19489651.8] | – | same |
| parser_dat | parse | after | 5 | 200 | 1.755 s [1.659 s, 1.794 s] | 8.77 ms [8.30 ms, 8.97 ms] | 4386.7 [4148.2, 4485.0] | 1.006 [0.921, 1.023] | same |
| parser_dat | parse | before | 5 | 200 | 1.744 s [1.742 s, 1.801 s] | 8.72 ms [8.71 ms, 9.00 ms] | 4360.4 [4354.2, 4502.1] | – | same |
| parser_dat | evaluate | after | 5 | 200 | 2.389 s [2.364 s, 2.442 s] | 11.95 ms [11.82 ms, 12.21 ms] | 5973.5 [5908.8, 6104.0] | 0.769 [0.755, 0.777] | same |
| parser_dat | evaluate | before | 5 | 200 | 3.115 s [3.079 s, 3.166 s] | 15.57 ms [15.40 ms, 15.83 ms] | 7786.6 [7697.8, 7915.8] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dump_paraview_ascii_tet4 | 162000 | 29791 | 89373 | 1 | bc71120b5, f373ab7ce | 80dd05d0385d9b9b |
| dump_paraview_tet4 | 162000 | 29791 | 89373 | 1 | bc71120b5, f373ab7ce | e58603ca45e7a46d |
| dump_text_tet4 | 162000 | 29791 | 89373 | 1 | bc71120b5, f373ab7ce | 5c6fc3bc16ea68c7 |
| msh22_read_hex8 | 125000 | 132651 | 0 | 1 | bc71120b5, f373ab7ce | f53cd23380afee21 |
| msh22_read_tet4 | 162000 | 29791 | 0 | 1 | bc71120b5, f373ab7ce | 7591a6861fb64d1b |
| msh41_read_tet4 | 162000 | 29791 | 0 | 1 | bc71120b5, f373ab7ce | 7591a6861fb64d1b |
| msh_write_tet4 | 162000 | 29791 | 0 | 1 | bc71120b5, f373ab7ce | 2835a7aec9e209b8 |
| parser_chain | 21 | 0 | 0 | 1 | bc71120b5, f373ab7ce | a825173227900d22 |
| parser_dat | 2000 | 0 | 0 | 1 | bc71120b5, f373ab7ce | e1d3e4f25f338ed6 |
