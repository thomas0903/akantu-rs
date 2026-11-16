Suite `setup`, size `m`; variants: `base`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| cohesive_insert_tet4 | mesh | base | 5 | 1 | 179.79 ms [178.89 ms, 183.90 ms] | 179.79 ms [178.89 ms, 183.90 ms] | 1109.8 [1104.3, 1135.2] | same |
| cohesive_insert_tet4 | init_mesh_facets | base | 5 | 1 | 1.820 s [1.818 s, 1.836 s] | 1.820 s [1.818 s, 1.836 s] | 11236.1 [11224.9, 11335.2] | same |
| cohesive_insert_tet4 | flag_facets | base | 5 | 1 | 19.21 ms [19.04 ms, 19.23 ms] | 19.21 ms [19.04 ms, 19.23 ms] | 118.6 [117.5, 118.7] | same |
| cohesive_insert_tet4 | insert | base | 5 | 1 | 4.224 s [4.182 s, 4.270 s] | 4.224 s [4.182 s, 4.270 s] | 26072.6 [25812.1, 26355.0] | same |
| cohesive_insert_tet4 | update_insertion_facets | base | 5 | 1 | 423.8 µs [408.9 µs, 429.8 µs] | 423.8 µs [408.9 µs, 429.8 µs] | 2.6 [2.5, 2.7] | same |
| dump_paraview_ascii_tet4 | mesh | base | 5 | 1 | 181.31 ms [180.83 ms, 182.51 ms] | 181.31 ms [180.83 ms, 182.51 ms] | 1119.2 [1116.2, 1126.6] | same |
| dump_paraview_ascii_tet4 | init | base | 5 | 1 | 133.06 ms [131.87 ms, 133.72 ms] | 133.06 ms [131.87 ms, 133.72 ms] | 821.3 [814.0, 825.4] | same |
| dump_paraview_ascii_tet4 | dump | base | 5 | 3 | 2.780 s [2.754 s, 2.804 s] | 926.83 ms [917.98 ms, 934.50 ms] | 5721.2 [5666.6, 5768.5] | same |
| dump_paraview_tet4 | mesh | base | 5 | 1 | 178.47 ms [178.09 ms, 181.39 ms] | 178.47 ms [178.09 ms, 181.39 ms] | 1101.7 [1099.3, 1119.7] | same |
| dump_paraview_tet4 | init | base | 5 | 1 | 130.20 ms [128.78 ms, 130.45 ms] | 130.20 ms [128.78 ms, 130.45 ms] | 803.7 [794.9, 805.2] | same |
| dump_paraview_tet4 | dump | base | 5 | 15 | 2.995 s [2.994 s, 3.044 s] | 199.70 ms [199.58 ms, 202.94 ms] | 1232.7 [1232.0, 1252.7] | same |
| dump_text_tet4 | mesh | base | 5 | 1 | 180.74 ms [180.00 ms, 182.25 ms] | 180.74 ms [180.00 ms, 182.25 ms] | 1115.7 [1111.1, 1125.0] | same |
| dump_text_tet4 | init | base | 5 | 1 | 130.68 ms [130.20 ms, 131.66 ms] | 130.68 ms [130.20 ms, 131.66 ms] | 806.7 [803.7, 812.7] | same |
| dump_text_tet4 | dump | base | 5 | 30 | 2.207 s [2.198 s, 2.211 s] | 73.58 ms [73.27 ms, 73.71 ms] | 454.2 [452.3, 455.0] | same |
| erase_purify_tet4 | mesh | base | 5 | 1 | 180.98 ms [179.36 ms, 184.99 ms] | 180.98 ms [179.36 ms, 184.99 ms] | 1117.2 [1107.2, 1141.9] | same |
| erase_purify_tet4 | erase_elements | base | 5 | 1 | 10.60 ms [10.56 ms, 10.98 ms] | 10.60 ms [10.56 ms, 10.98 ms] | 65.4 [65.2, 67.8] | same |
| erase_purify_tet4 | purify_mesh | base | 5 | 1 | 1.78 ms [1.76 ms, 1.79 ms] | 1.78 ms [1.76 ms, 1.79 ms] | 11.0 [10.9, 11.1] | same |
| facets_hex8 | mesh | base | 5 | 1 | 240.20 ms [237.91 ms, 251.03 ms] | 240.20 ms [237.91 ms, 251.03 ms] | 1921.6 [1903.3, 2008.2] | same |
| facets_hex8 | node_to_elements | base | 5 | 1 | 4.87 ms [4.84 ms, 5.05 ms] | 4.87 ms [4.84 ms, 5.05 ms] | 38.9 [38.7, 40.4] | same |
| facets_hex8 | element_to_subelements | base | 5 | 1 | 9.49 ms [9.41 ms, 9.87 ms] | 9.49 ms [9.41 ms, 9.87 ms] | 75.9 [75.3, 78.9] | same |
| facets_hex8 | init_mesh_facets | base | 5 | 1 | 964.39 ms [957.13 ms, 978.72 ms] | 964.39 ms [957.13 ms, 978.72 ms] | 7715.1 [7657.0, 7829.8] | same |
| facets_tet4 | mesh | base | 5 | 1 | 179.92 ms [178.97 ms, 186.98 ms] | 179.92 ms [178.97 ms, 186.98 ms] | 1110.6 [1104.7, 1154.2] | same |
| facets_tet4 | node_to_elements | base | 5 | 1 | 2.90 ms [2.89 ms, 2.93 ms] | 2.90 ms [2.89 ms, 2.93 ms] | 17.9 [17.8, 18.1] | same |
| facets_tet4 | element_to_subelements | base | 5 | 1 | 11.00 ms [10.96 ms, 11.45 ms] | 11.00 ms [10.96 ms, 11.45 ms] | 67.9 [67.6, 70.7] | same |
| facets_tet4 | init_mesh_facets | base | 5 | 1 | 1.803 s [1.802 s, 1.815 s] | 1.803 s [1.802 s, 1.815 s] | 11131.2 [11125.8, 11206.4] | same |
| groups_tet4 | mesh | base | 5 | 1 | 181.47 ms [179.78 ms, 188.08 ms] | 181.47 ms [179.78 ms, 188.08 ms] | 1120.2 [1109.7, 1161.0] | same |
| groups_tet4 | filtered_node_group | base | 5 | 1 | 293.4 µs [291.2 µs, 315.5 µs] | 293.4 µs [291.2 µs, 315.5 µs] | 1.8 [1.8, 1.9] | same |
| groups_tet4 | element_group_from_node_group | base | 5 | 1 | 163.78 ms [162.02 ms, 164.24 ms] | 163.78 ms [162.02 ms, 164.24 ms] | 1011.0 [1000.1, 1013.8] | same |
| groups_tet4 | groups_from_mesh_data | base | 5 | 1 | 29.38 ms [29.28 ms, 29.49 ms] | 29.38 ms [29.28 ms, 29.49 ms] | 181.4 [180.7, 182.0] | same |
| msh22_read_hex8 | generate | base | 5 | 1 | 73.79 ms [72.14 ms, 75.65 ms] | 73.79 ms [72.14 ms, 75.65 ms] | 590.3 [577.1, 605.2] | same |
| msh22_read_hex8 | read | base | 5 | 1 | 173.42 ms [172.53 ms, 187.88 ms] | 173.42 ms [172.53 ms, 187.88 ms] | 1387.3 [1380.2, 1503.0] | same |
| msh22_read_tet4 | generate | base | 5 | 1 | 46.55 ms [44.74 ms, 47.64 ms] | 46.55 ms [44.74 ms, 47.64 ms] | 287.3 [276.2, 294.1] | same |
| msh22_read_tet4 | read | base | 5 | 1 | 144.20 ms [141.97 ms, 148.15 ms] | 144.20 ms [141.97 ms, 148.15 ms] | 890.1 [876.4, 914.5] | same |
| msh41_read_tet4 | generate | base | 5 | 1 | 36.27 ms [35.72 ms, 36.67 ms] | 36.27 ms [35.72 ms, 36.67 ms] | 223.9 [220.5, 226.3] | same |
| msh41_read_tet4 | read | base | 5 | 1 | 114.58 ms [112.18 ms, 120.90 ms] | 114.58 ms [112.18 ms, 120.90 ms] | 707.3 [692.5, 746.3] | same |
| msh_write_tet4 | mesh | base | 5 | 1 | 180.66 ms [179.13 ms, 188.40 ms] | 180.66 ms [179.13 ms, 188.40 ms] | 1115.2 [1105.7, 1162.9] | same |
| msh_write_tet4 | write | base | 5 | 1 | 53.96 ms [53.17 ms, 55.23 ms] | 53.96 ms [53.17 ms, 55.23 ms] | 333.1 [328.2, 340.9] | same |
| parser_dat | generate | base | 5 | 1 | 502.4 µs [498.7 µs, 504.2 µs] | 502.4 µs [498.7 µs, 504.2 µs] | 251.2 [249.4, 252.1] | same |
| parser_dat | parse | base | 5 | 200 | 1.319 s [1.312 s, 1.324 s] | 6.59 ms [6.56 ms, 6.62 ms] | 3297.4 [3279.7, 3310.3] | same |
| parser_dat | evaluate | base | 5 | 200 | 2.900 s [2.865 s, 2.911 s] | 14.50 ms [14.32 ms, 14.55 ms] | 7250.1 [7161.3, 7276.3] | same |
| partition_rcb_tet4 | mesh | base | 5 | 1 | 179.90 ms [179.63 ms, 181.36 ms] | 179.90 ms [179.63 ms, 181.36 ms] | 1110.5 [1108.8, 1119.5] | same |
| partition_rcb_tet4 | partition | base | 5 | 1 | 79.24 ms [78.90 ms, 79.33 ms] | 79.24 ms [78.90 ms, 79.33 ms] | 489.1 [487.0, 489.7] | same |
| periodic_tet4 | mesh | base | 5 | 1 | 180.15 ms [178.70 ms, 187.64 ms] | 180.15 ms [178.70 ms, 187.64 ms] | 1112.0 [1103.1, 1158.3] | same |
| periodic_tet4 | make_periodic | base | 5 | 1 | 6.29 ms [6.27 ms, 6.31 ms] | 6.29 ms [6.27 ms, 6.31 ms] | 38.8 [38.7, 39.0] | same |
| py_twin_explicit_tri3 | init | base | 5 | 1 | 44.60 ms [44.48 ms, 45.06 ms] | 44.60 ms [44.48 ms, 45.06 ms] | 557.5 [555.9, 563.3] | same |
| py_twin_explicit_tri3 | solve_step | base | 5 | 400 | 1.885 s [1.877 s, 2.027 s] | 4.71 ms [4.69 ms, 5.07 ms] | 58.9 [58.7, 63.4] | same |
| py_twin_explicit_tri3 | solve_step_tiny | base | 5 | 300000 | 2.416 s [2.403 s, 2.462 s] | 8.1 µs [8.0 µs, 8.2 µs] | 0.1 [0.1, 0.1] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_insert_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 729b912480738e87 |
| dump_paraview_ascii_tet4 | 162000 | 29791 | 89373 | 1 | e2bebff49 | 80dd05d0385d9b9b |
| dump_paraview_tet4 | 162000 | 29791 | 89373 | 1 | e2bebff49 | e58603ca45e7a46d |
| dump_text_tet4 | 162000 | 29791 | 89373 | 1 | e2bebff49 | cbf29ce484222325 |
| erase_purify_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 102cce9be0774685 |
| facets_hex8 | 125000 | 132651 | 0 | 1 | e2bebff49 | ba6eea7da98faed7 |
| facets_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 02af661b6dbce562 |
| groups_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | f17e342837f73adb |
| msh22_read_hex8 | 125000 | 132651 | 0 | 1 | e2bebff49 | f53cd23380afee21 |
| msh22_read_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 7591a6861fb64d1b |
| msh41_read_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 7591a6861fb64d1b |
| msh_write_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 2835a7aec9e209b8 |
| parser_dat | 2000 | 0 | 0 | 1 | e2bebff49 | e1d3e4f25f338ed6 |
| partition_rcb_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 2ddb9a95d22fb5e6 |
| periodic_tet4 | 162000 | 29791 | 0 | 1 | e2bebff49 | 7c5827d758bea30f |
| py_twin_explicit_tri3 | 80000 | 40401 | 80802 | 1 | e2bebff49 | 7c3f18493f05f971 |
