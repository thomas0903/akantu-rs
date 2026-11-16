Suite `setup`, size `m`; variants: `after1`, `after2`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| dump_paraview_ascii_tet4 | dump | after1 | 5 | 3 | 1.522 s [1.505 s, 1.545 s] | 507.49 ms [501.50 ms, 515.07 ms] | 3132.6 [3095.7, 3179.4] | 0.565 [0.558, 0.567] | same |
| dump_paraview_ascii_tet4 | dump | after2 | 5 | 3 | 1.531 s [1.517 s, 1.539 s] | 510.38 ms [505.78 ms, 512.88 ms] | 3150.5 [3122.1, 3165.9] | 0.568 [0.561, 0.571] | same |
| dump_paraview_ascii_tet4 | dump | before | 5 | 3 | 2.694 s [2.691 s, 2.730 s] | 898.11 ms [896.89 ms, 909.94 ms] | 5543.9 [5536.4, 5616.9] | – | same |
| dump_paraview_tet4 | dump | after1 | 5 | 15 | 2.061 s [2.035 s, 2.102 s] | 137.41 ms [135.70 ms, 140.12 ms] | 848.2 [837.6, 864.9] | 0.693 [0.687, 0.715] | same |
| dump_paraview_tet4 | dump | after2 | 5 | 15 | 2.045 s [2.019 s, 2.091 s] | 136.31 ms [134.62 ms, 139.38 ms] | 841.4 [831.0, 860.4] | 0.689 [0.687, 0.706] | same |
| dump_paraview_tet4 | dump | before | 5 | 15 | 2.967 s [2.938 s, 2.975 s] | 197.81 ms [195.88 ms, 198.33 ms] | 1221.0 [1209.1, 1224.3] | – | same |
| dump_text_tet4 | dump | after1 | 5 | 30 | 1.375 s [1.369 s, 1.382 s] | 45.83 ms [45.63 ms, 46.07 ms] | 282.9 [281.7, 284.4] | 0.623 [0.622, 0.627] | same |
| dump_text_tet4 | dump | after2 | 5 | 30 | 1.388 s [1.378 s, 1.400 s] | 46.27 ms [45.94 ms, 46.66 ms] | 285.6 [283.6, 288.0] | 0.630 [0.626, 0.633] | same |
| dump_text_tet4 | dump | before | 5 | 30 | 2.203 s [2.197 s, 2.211 s] | 73.44 ms [73.24 ms, 73.68 ms] | 453.4 [452.1, 454.8] | – | same |
| msh22_read_hex8 | read | after1 | 5 | 1 | 173.01 ms [172.80 ms, 183.10 ms] | 173.01 ms [172.80 ms, 183.10 ms] | 1384.1 [1382.4, 1464.8] | 0.999 [0.996, 1.013] | same |
| msh22_read_hex8 | read | after2 | 5 | 1 | 108.85 ms [108.06 ms, 110.27 ms] | 108.85 ms [108.06 ms, 110.27 ms] | 870.8 [864.5, 882.2] | 0.624 [0.603, 0.634] | same |
| msh22_read_hex8 | read | before | 5 | 1 | 173.24 ms [171.50 ms, 180.80 ms] | 173.24 ms [171.50 ms, 180.80 ms] | 1385.9 [1372.0, 1446.4] | – | same |
| msh22_read_tet4 | read | after1 | 5 | 1 | 144.31 ms [140.58 ms, 144.83 ms] | 144.31 ms [140.58 ms, 144.83 ms] | 890.8 [867.8, 894.0] | 1.015 [0.950, 1.031] | same |
| msh22_read_tet4 | read | after2 | 5 | 1 | 90.04 ms [89.73 ms, 90.49 ms] | 90.04 ms [89.73 ms, 90.49 ms] | 555.8 [553.9, 558.6] | 0.638 [0.593, 0.641] | same |
| msh22_read_tet4 | read | before | 5 | 1 | 140.78 ms [140.41 ms, 152.48 ms] | 140.78 ms [140.41 ms, 152.48 ms] | 869.0 [866.7, 941.2] | – | same |
| msh41_read_tet4 | read | after1 | 5 | 1 | 113.16 ms [112.83 ms, 120.23 ms] | 113.16 ms [112.83 ms, 120.23 ms] | 698.5 [696.5, 742.1] | 1.001 [0.985, 1.022] | same |
| msh41_read_tet4 | read | after2 | 5 | 1 | 77.18 ms [76.91 ms, 78.26 ms] | 77.18 ms [76.91 ms, 78.26 ms] | 476.4 [474.7, 483.1] | 0.677 [0.651, 0.684] | same |
| msh41_read_tet4 | read | before | 5 | 1 | 114.59 ms [112.74 ms, 120.05 ms] | 114.59 ms [112.74 ms, 120.05 ms] | 707.3 [695.9, 741.0] | – | same |
| msh_write_tet4 | write | after1 | 5 | 1 | 29.37 ms [29.11 ms, 29.48 ms] | 29.37 ms [29.11 ms, 29.48 ms] | 181.3 [179.7, 182.0] | 0.542 [0.524, 0.553] | same |
| msh_write_tet4 | write | after2 | 5 | 1 | 29.32 ms [29.18 ms, 29.84 ms] | 29.32 ms [29.18 ms, 29.84 ms] | 181.0 [180.1, 184.2] | 0.539 [0.525, 0.563] | same |
| msh_write_tet4 | write | before | 5 | 1 | 54.17 ms [53.02 ms, 56.27 ms] | 54.17 ms [53.02 ms, 56.27 ms] | 334.4 [327.3, 347.4] | – | same |
| parser_chain | parse | after1 | 5 | 1 | 24.2 µs [23.4 µs, 27.3 µs] | 24.2 µs [23.4 µs, 27.3 µs] | 1151.9 [1112.9, 1301.0] | 1.065 [0.989, 1.151] | same |
| parser_chain | parse | after2 | 5 | 1 | 23.6 µs [21.4 µs, 23.9 µs] | 23.6 µs [21.4 µs, 23.9 µs] | 1122.4 [1019.0, 1140.0] | 0.981 [0.935, 1.060] | same |
| parser_chain | parse | before | 5 | 1 | 22.9 µs [22.2 µs, 24.5 µs] | 22.9 µs [22.2 µs, 24.5 µs] | 1089.5 [1059.0, 1166.2] | – | same |
| parser_chain | evaluate_last | after1 | 5 | 5 | 41.8 µs [41.7 µs, 43.0 µs] | 8.4 µs [8.3 µs, 8.6 µs] | 398.3 [396.8, 409.5] | 0.000 [0.000, 0.000] | same |
| parser_chain | evaluate_last | after2 | 5 | 5 | 42.5 µs [41.5 µs, 42.6 µs] | 8.5 µs [8.3 µs, 8.5 µs] | 405.0 [395.1, 405.9] | 0.000 [0.000, 0.000] | same |
| parser_chain | evaluate_last | before | 5 | 5 | 1.985 s [1.968 s, 2.004 s] | 396.91 ms [393.54 ms, 400.74 ms] | 18900364.9 [18740157.1, 19082858.9] | – | same |
| parser_dat | parse | after1 | 5 | 200 | 1.286 s [1.266 s, 2.098 s] | 6.43 ms [6.33 ms, 10.49 ms] | 3215.8 [3165.9, 5245.4] | 0.966 [0.911, 0.979] | same |
| parser_dat | parse | after2 | 5 | 200 | 1.272 s [1.259 s, 1.328 s] | 6.36 ms [6.29 ms, 6.64 ms] | 3180.3 [3147.3, 3320.0] | 0.961 [0.596, 0.977] | same |
| parser_dat | parse | before | 5 | 200 | 1.323 s [1.311 s, 2.230 s] | 6.61 ms [6.55 ms, 11.15 ms] | 3306.3 [3276.6, 5574.4] | – | same |
| parser_dat | evaluate | after1 | 5 | 200 | 2.447 s [2.438 s, 2.595 s] | 12.23 ms [12.19 ms, 12.98 ms] | 6117.4 [6094.1, 6488.1] | 0.840 [0.772, 0.893] | same |
| parser_dat | evaluate | after2 | 5 | 200 | 2.550 s [2.544 s, 2.584 s] | 12.75 ms [12.72 ms, 12.92 ms] | 6374.9 [6360.4, 6459.1] | 0.875 [0.801, 0.889] | same |
| parser_dat | evaluate | before | 5 | 200 | 2.908 s [2.868 s, 3.225 s] | 14.54 ms [14.34 ms, 16.12 ms] | 7270.1 [7171.1, 8062.4] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dump_paraview_ascii_tet4 | 162000 | 29791 | 89373 | 1 | 9d5b044f4, f373ab7ce, f99711064 | 80dd05d0385d9b9b |
| dump_paraview_tet4 | 162000 | 29791 | 89373 | 1 | 9d5b044f4, f373ab7ce, f99711064 | e58603ca45e7a46d |
| dump_text_tet4 | 162000 | 29791 | 89373 | 1 | 9d5b044f4, f373ab7ce, f99711064 | 5c6fc3bc16ea68c7 |
| msh22_read_hex8 | 125000 | 132651 | 0 | 1 | 9d5b044f4, f373ab7ce, f99711064 | f53cd23380afee21 |
| msh22_read_tet4 | 162000 | 29791 | 0 | 1 | 9d5b044f4, f373ab7ce, f99711064 | 7591a6861fb64d1b |
| msh41_read_tet4 | 162000 | 29791 | 0 | 1 | 9d5b044f4, f373ab7ce, f99711064 | 7591a6861fb64d1b |
| msh_write_tet4 | 162000 | 29791 | 0 | 1 | 9d5b044f4, f373ab7ce, f99711064 | 2835a7aec9e209b8 |
| parser_chain | 21 | 0 | 0 | 1 | 9d5b044f4, f373ab7ce, f99711064 | a825173227900d22 |
| parser_dat | 2000 | 0 | 0 | 1 | 9d5b044f4, f373ab7ce, f99711064 | e1d3e4f25f338ed6 |
