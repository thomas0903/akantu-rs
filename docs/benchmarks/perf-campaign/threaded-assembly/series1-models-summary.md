Suite `models`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | mesh | base | 5 | 1 | 11.27 ms [9.79 ms, 11.39 ms] | 11.27 ms [9.79 ms, 11.39 ms] | 1609.9 [1398.6, 1627.7] | – | same |
| contact_implicit_static_quad4 | mesh | t1 | 5 | 1 | 11.09 ms [9.65 ms, 11.59 ms] | 11.09 ms [9.65 ms, 11.59 ms] | 1584.0 [1378.6, 1656.1] | 0.986 [0.869, 1.041] | same |
| contact_implicit_static_quad4 | mesh | t2 | 5 | 1 | 11.12 ms [10.99 ms, 11.61 ms] | 11.12 ms [10.99 ms, 11.61 ms] | 1589.0 [1570.6, 1658.9] | 0.996 [0.975, 1.186] | same |
| contact_implicit_static_quad4 | mesh | t4 | 5 | 1 | 11.07 ms [9.66 ms, 11.76 ms] | 11.07 ms [9.66 ms, 11.76 ms] | 1581.6 [1379.3, 1680.4] | 0.986 [0.968, 1.044] | same |
| contact_implicit_static_quad4 | mesh | t8 | 5 | 1 | 11.12 ms [11.06 ms, 11.71 ms] | 11.12 ms [11.06 ms, 11.71 ms] | 1589.3 [1580.1, 1672.3] | 1.021 [0.982, 1.130] | same |
| contact_implicit_static_quad4 | init | base | 5 | 1 | 8.10 ms [6.93 ms, 9.48 ms] | 8.10 ms [6.93 ms, 9.48 ms] | 1156.9 [989.9, 1354.9] | – | same |
| contact_implicit_static_quad4 | init | t1 | 5 | 1 | 8.07 ms [6.67 ms, 8.73 ms] | 8.07 ms [6.67 ms, 8.73 ms] | 1152.9 [952.6, 1246.7] | 0.957 [0.853, 1.078] | same |
| contact_implicit_static_quad4 | init | t2 | 5 | 1 | 8.43 ms [8.15 ms, 8.75 ms] | 8.43 ms [8.15 ms, 8.75 ms] | 1204.0 [1164.4, 1249.9] | 1.008 [0.883, 1.216] | same |
| contact_implicit_static_quad4 | init | t4 | 5 | 1 | 7.94 ms [6.92 ms, 8.84 ms] | 7.94 ms [6.92 ms, 8.84 ms] | 1134.1 [988.9, 1262.4] | 0.999 [0.826, 1.048] | same |
| contact_implicit_static_quad4 | init | t8 | 5 | 1 | 7.97 ms [7.93 ms, 8.85 ms] | 7.97 ms [7.93 ms, 8.85 ms] | 1138.6 [1133.3, 1264.6] | 1.064 [0.836, 1.146] | same |
| contact_implicit_static_quad4 | solve_step | base | 5 | 5 | 735.16 ms [671.92 ms, 746.11 ms] | 147.03 ms [134.38 ms, 149.22 ms] | 21004.6 [19197.8, 21317.4] | – | same |
| contact_implicit_static_quad4 | solve_step | t1 | 5 | 5 | 713.68 ms [666.72 ms, 727.61 ms] | 142.74 ms [133.34 ms, 145.52 ms] | 20390.9 [19049.1, 20789.0] | 0.987 [0.957, 1.017] | same |
| contact_implicit_static_quad4 | solve_step | t2 | 5 | 5 | 699.45 ms [605.34 ms, 705.98 ms] | 139.89 ms [121.07 ms, 141.20 ms] | 19984.2 [17295.3, 20171.0] | 0.949 [0.901, 1.013] | same |
| contact_implicit_static_quad4 | solve_step | t4 | 5 | 5 | 689.50 ms [610.78 ms, 730.74 ms] | 137.90 ms [122.16 ms, 146.15 ms] | 19700.1 [17451.0, 20878.2] | 0.935 [0.909, 1.049] | same |
| contact_implicit_static_quad4 | solve_step | t8 | 5 | 5 | 669.08 ms [618.54 ms, 677.29 ms] | 133.82 ms [123.71 ms, 135.46 ms] | 19116.7 [17672.5, 19351.3] | 0.913 [0.908, 0.934] | same |
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 28.33 ms [26.21 ms, 28.52 ms] | 28.33 ms [26.21 ms, 28.52 ms] | 1302.7 [1205.0, 1311.2] | – | same |
| embedded_rebar_tet4 | mesh | t1 | 5 | 1 | 29.16 ms [27.39 ms, 29.91 ms] | 29.16 ms [27.39 ms, 29.91 ms] | 1340.5 [1259.5, 1375.4] | 1.027 [1.027, 1.056] | same |
| embedded_rebar_tet4 | mesh | t2 | 5 | 1 | 29.06 ms [25.66 ms, 30.36 ms] | 29.06 ms [25.66 ms, 30.36 ms] | 1336.2 [1179.7, 1395.9] | 1.019 [0.979, 1.072] | same |
| embedded_rebar_tet4 | mesh | t4 | 5 | 1 | 28.58 ms [26.13 ms, 29.63 ms] | 28.58 ms [26.13 ms, 29.63 ms] | 1313.9 [1201.3, 1362.4] | 1.009 [0.964, 1.039] | same |
| embedded_rebar_tet4 | mesh | t8 | 5 | 1 | 28.77 ms [28.60 ms, 29.30 ms] | 28.77 ms [28.60 ms, 29.30 ms] | 1322.6 [1315.1, 1346.9] | 1.022 [1.014, 1.091] | same |
| embedded_rebar_tet4 | init | base | 5 | 1 | 1.435 s [1.263 s, 1.448 s] | 1.435 s [1.263 s, 1.448 s] | 65985.7 [58046.3, 66566.3] | – | same |
| embedded_rebar_tet4 | init | t1 | 5 | 1 | 1.412 s [1.323 s, 1.475 s] | 1.412 s [1.323 s, 1.475 s] | 64910.7 [60839.8, 67821.7] | 1.023 [0.933, 1.048] | same |
| embedded_rebar_tet4 | init | t2 | 5 | 1 | 1.390 s [1.311 s, 1.431 s] | 1.390 s [1.311 s, 1.431 s] | 63910.1 [60255.8, 65790.9] | 0.994 [0.933, 1.038] | same |
| embedded_rebar_tet4 | init | t4 | 5 | 1 | 1.379 s [1.307 s, 1.433 s] | 1.379 s [1.307 s, 1.433 s] | 63389.7 [60084.3, 65906.3] | 0.996 [0.951, 1.035] | same |
| embedded_rebar_tet4 | init | t8 | 5 | 1 | 1.402 s [1.324 s, 1.453 s] | 1.402 s [1.324 s, 1.453 s] | 64481.7 [60889.1, 66818.6] | 0.995 [0.975, 1.049] | same |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 6.912 s [6.218 s, 7.318 s] | 1.382 s [1.244 s, 1.464 s] | 63560.5 [57177.1, 67294.3] | – | same |
| embedded_rebar_tet4 | intersect | t1 | 5 | 5 | 6.651 s [6.260 s, 7.256 s] | 1.330 s [1.252 s, 1.451 s] | 61158.9 [57562.6, 66723.1] | 0.975 [0.953, 1.007] | same |
| embedded_rebar_tet4 | intersect | t2 | 5 | 5 | 6.617 s [6.499 s, 7.291 s] | 1.323 s [1.300 s, 1.458 s] | 60850.2 [59763.8, 67046.4] | 0.973 [0.956, 1.045] | same |
| embedded_rebar_tet4 | intersect | t4 | 5 | 5 | 6.972 s [6.862 s, 11.303 s] | 1.394 s [1.372 s, 2.261 s] | 64114.6 [63095.4, 103934.2] | 1.026 [0.964, 1.553] | same |
| embedded_rebar_tet4 | intersect | t8 | 5 | 5 | 6.996 s [6.433 s, 7.314 s] | 1.399 s [1.287 s, 1.463 s] | 64331.8 [59153.6, 67258.6] | 0.999 [0.976, 1.035] | same |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 1.074 s [1.025 s, 1.085 s] | 1.074 s [1.025 s, 1.085 s] | 49387.7 [47112.9, 49903.2] | – | same |
| embedded_rebar_tet4 | solve_step | t1 | 5 | 1 | 1.046 s [986.65 ms, 1.096 s] | 1.046 s [986.65 ms, 1.096 s] | 48072.9 [45363.0, 50377.1] | 0.986 [0.963, 1.009] | same |
| embedded_rebar_tet4 | solve_step | t2 | 5 | 1 | 1.048 s [1.013 s, 1.059 s] | 1.048 s [1.013 s, 1.059 s] | 48190.5 [46555.4, 48677.7] | 0.975 [0.955, 1.023] | same |
| embedded_rebar_tet4 | solve_step | t4 | 5 | 1 | 1.034 s [994.99 ms, 1.047 s] | 1.034 s [994.99 ms, 1.047 s] | 47523.2 [45746.8, 48118.8] | 0.964 [0.938, 0.981] | same |
| embedded_rebar_tet4 | solve_step | t8 | 5 | 1 | 1.035 s [948.89 ms, 1.038 s] | 1.035 s [948.89 ms, 1.038 s] | 47583.7 [43627.3, 47738.7] | 0.963 [0.926, 0.972] | same |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 35.68 ms [31.20 ms, 37.29 ms] | 1.78 ms [1.56 ms, 1.86 ms] | 82.0 [71.7, 85.7] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | t1 | 5 | 20 | 35.92 ms [31.16 ms, 36.70 ms] | 1.80 ms [1.56 ms, 1.84 ms] | 82.6 [71.6, 84.4] | 0.999 [0.948, 1.123] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t2 | 5 | 20 | 24.78 ms [23.57 ms, 25.64 ms] | 1.24 ms [1.18 ms, 1.28 ms] | 57.0 [54.2, 58.9] | 0.691 [0.660, 0.801] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t4 | 5 | 20 | 20.31 ms [20.23 ms, 20.93 ms] | 1.02 ms [1.01 ms, 1.05 ms] | 46.7 [46.5, 48.1] | 0.569 [0.543, 0.654] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t8 | 5 | 20 | 18.12 ms [17.79 ms, 18.28 ms] | 906.2 µs [889.4 µs, 914.1 µs] | 41.7 [40.9, 42.0] | 0.508 [0.486, 0.570] | same |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 163.72 ms [156.09 ms, 167.57 ms] | 32.74 ms [31.22 ms, 33.51 ms] | 1505.4 [1435.4, 1540.9] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | t1 | 5 | 5 | 97.19 ms [94.07 ms, 104.10 ms] | 19.44 ms [18.81 ms, 20.82 ms] | 893.7 [865.0, 957.2] | 0.603 [0.573, 0.641] | same |
| embedded_rebar_tet4 | assemble_stiffness | t2 | 5 | 5 | 62.44 ms [58.17 ms, 64.37 ms] | 12.49 ms [11.63 ms, 12.87 ms] | 574.2 [534.9, 591.9] | 0.388 [0.355, 0.400] | same |
| embedded_rebar_tet4 | assemble_stiffness | t4 | 5 | 5 | 46.44 ms [44.78 ms, 47.75 ms] | 9.29 ms [8.96 ms, 9.55 ms] | 427.0 [411.8, 439.0] | 0.287 [0.276, 0.291] | same |
| embedded_rebar_tet4 | assemble_stiffness | t8 | 5 | 5 | 37.73 ms [35.64 ms, 37.86 ms] | 7.55 ms [7.13 ms, 7.57 ms] | 346.9 [327.7, 348.2] | 0.228 [0.226, 0.233] | same |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 27.49 ms [26.98 ms, 27.94 ms] | 27.49 ms [26.98 ms, 27.94 ms] | 1090.8 [1070.7, 1108.8] | – | same |
| embedded_rebar_tri3 | mesh | t1 | 5 | 1 | 27.67 ms [27.46 ms, 28.25 ms] | 27.67 ms [27.46 ms, 28.25 ms] | 1097.9 [1089.5, 1121.2] | 1.001 [0.994, 1.031] | same |
| embedded_rebar_tri3 | mesh | t2 | 5 | 1 | 27.65 ms [27.33 ms, 28.23 ms] | 27.65 ms [27.33 ms, 28.23 ms] | 1097.4 [1084.6, 1120.2] | 0.998 [0.990, 1.025] | same |
| embedded_rebar_tri3 | mesh | t4 | 5 | 1 | 27.95 ms [27.25 ms, 28.41 ms] | 27.95 ms [27.25 ms, 28.41 ms] | 1109.3 [1081.5, 1127.3] | 1.008 [0.992, 1.037] | same |
| embedded_rebar_tri3 | mesh | t8 | 5 | 1 | 27.76 ms [27.63 ms, 28.06 ms] | 27.76 ms [27.63 ms, 28.06 ms] | 1101.7 [1096.5, 1113.7] | 1.005 [0.995, 1.040] | same |
| embedded_rebar_tri3 | init | base | 5 | 1 | 1.520 s [1.494 s, 2.249 s] | 1.520 s [1.494 s, 2.249 s] | 60320.8 [59297.3, 89246.2] | – | same |
| embedded_rebar_tri3 | init | t1 | 5 | 1 | 1.524 s [1.434 s, 1.571 s] | 1.524 s [1.434 s, 1.571 s] | 60490.3 [56906.7, 62338.8] | 0.992 [0.679, 1.025] | same |
| embedded_rebar_tri3 | init | t2 | 5 | 1 | 1.555 s [1.505 s, 1.578 s] | 1.555 s [1.505 s, 1.578 s] | 61716.3 [59733.5, 62633.6] | 1.010 [0.702, 1.030] | same |
| embedded_rebar_tri3 | init | t4 | 5 | 1 | 1.557 s [1.467 s, 1.603 s] | 1.557 s [1.467 s, 1.603 s] | 61771.9 [58220.2, 63627.9] | 0.997 [0.713, 1.047] | same |
| embedded_rebar_tri3 | init | t8 | 5 | 1 | 1.574 s [1.509 s, 1.601 s] | 1.574 s [1.509 s, 1.601 s] | 62452.9 [59882.6, 63516.0] | 1.042 [0.677, 1.060] | same |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 7.825 s [7.503 s, 7.878 s] | 1.565 s [1.501 s, 1.576 s] | 62105.8 [59551.2, 62521.4] | – | same |
| embedded_rebar_tri3 | intersect | t1 | 5 | 5 | 7.848 s [7.029 s, 7.890 s] | 1.570 s [1.406 s, 1.578 s] | 62282.3 [55788.3, 62616.5] | 1.005 [0.892, 1.051] | same |
| embedded_rebar_tri3 | intersect | t2 | 5 | 5 | 7.513 s [7.066 s, 7.856 s] | 1.503 s [1.413 s, 1.571 s] | 59624.2 [56080.6, 62345.8] | 0.983 [0.901, 1.009] | same |
| embedded_rebar_tri3 | intersect | t4 | 5 | 5 | 7.483 s [7.415 s, 7.846 s] | 1.497 s [1.483 s, 1.569 s] | 59389.1 [58846.4, 62266.4] | 0.997 [0.944, 1.001] | same |
| embedded_rebar_tri3 | intersect | t8 | 5 | 5 | 7.562 s [7.443 s, 7.907 s] | 1.512 s [1.489 s, 1.581 s] | 60012.3 [59067.9, 62752.0] | 0.966 [0.945, 1.048] | same |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 252.29 ms [247.42 ms, 255.03 ms] | 252.29 ms [247.42 ms, 255.03 ms] | 10011.5 [9818.3, 10120.2] | – | same |
| embedded_rebar_tri3 | solve_step | t1 | 5 | 1 | 247.68 ms [230.42 ms, 249.04 ms] | 247.68 ms [230.42 ms, 249.04 ms] | 9828.6 [9143.8, 9882.5] | 0.981 [0.931, 1.007] | same |
| embedded_rebar_tri3 | solve_step | t2 | 5 | 1 | 240.03 ms [236.19 ms, 242.74 ms] | 240.03 ms [236.19 ms, 242.74 ms] | 9524.8 [9372.5, 9632.4] | 0.948 [0.929, 0.981] | same |
| embedded_rebar_tri3 | solve_step | t4 | 5 | 1 | 238.80 ms [234.98 ms, 242.18 ms] | 238.80 ms [234.98 ms, 242.18 ms] | 9476.0 [9324.6, 9610.3] | 0.950 [0.935, 0.965] | same |
| embedded_rebar_tri3 | solve_step | t8 | 5 | 1 | 234.32 ms [230.66 ms, 239.58 ms] | 234.32 ms [230.66 ms, 239.58 ms] | 9298.5 [9153.3, 9507.0] | 0.939 [0.919, 0.950] | same |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 48.84 ms [47.05 ms, 49.29 ms] | 2.44 ms [2.35 ms, 2.46 ms] | 96.9 [93.4, 97.8] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | t1 | 5 | 20 | 50.26 ms [45.49 ms, 51.01 ms] | 2.51 ms [2.27 ms, 2.55 ms] | 99.7 [90.3, 101.2] | 1.027 [0.931, 1.079] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t2 | 5 | 20 | 41.39 ms [39.08 ms, 41.78 ms] | 2.07 ms [1.95 ms, 2.09 ms] | 82.1 [77.5, 82.9] | 0.854 [0.793, 0.882] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t4 | 5 | 20 | 39.97 ms [39.54 ms, 40.27 ms] | 2.00 ms [1.98 ms, 2.01 ms] | 79.3 [78.5, 79.9] | 0.825 [0.802, 0.850] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t8 | 5 | 20 | 38.99 ms [38.61 ms, 39.83 ms] | 1.95 ms [1.93 ms, 1.99 ms] | 77.4 [76.6, 79.0] | 0.806 [0.789, 0.846] | same |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 46.95 ms [46.04 ms, 48.00 ms] | 9.39 ms [9.21 ms, 9.60 ms] | 372.6 [365.4, 381.0] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | t1 | 5 | 5 | 48.97 ms [45.89 ms, 49.47 ms] | 9.79 ms [9.18 ms, 9.89 ms] | 388.6 [364.2, 392.6] | 1.033 [0.977, 1.067] | same |
| embedded_rebar_tri3 | assemble_stiffness | t2 | 5 | 5 | 39.91 ms [38.98 ms, 41.53 ms] | 7.98 ms [7.80 ms, 8.31 ms] | 316.7 [309.3, 329.6] | 0.852 [0.830, 0.902] | same |
| embedded_rebar_tri3 | assemble_stiffness | t4 | 5 | 5 | 36.70 ms [36.45 ms, 37.65 ms] | 7.34 ms [7.29 ms, 7.53 ms] | 291.3 [289.3, 298.8] | 0.786 [0.764, 0.802] | same |
| embedded_rebar_tri3 | assemble_stiffness | t8 | 5 | 5 | 35.06 ms [34.46 ms, 35.37 ms] | 7.01 ms [6.89 ms, 7.07 ms] | 278.2 [273.5, 280.7] | 0.743 [0.730, 0.768] | same |
| heat_implicit_hex8_kvar | mesh | base | 5 | 1 | 4.62 ms [3.98 ms, 4.83 ms] | 4.62 ms [3.98 ms, 4.83 ms] | 2675.3 [2302.1, 2795.7] | – | same |
| heat_implicit_hex8_kvar | mesh | t1 | 5 | 1 | 4.67 ms [4.08 ms, 4.86 ms] | 4.67 ms [4.08 ms, 4.86 ms] | 2700.2 [2361.1, 2814.8] | 1.013 [0.966, 1.067] | same |
| heat_implicit_hex8_kvar | mesh | t2 | 5 | 1 | 4.64 ms [3.94 ms, 5.13 ms] | 4.64 ms [3.94 ms, 5.13 ms] | 2686.3 [2280.7, 2967.6] | 0.991 [0.960, 1.109] | same |
| heat_implicit_hex8_kvar | mesh | t4 | 5 | 1 | 4.85 ms [4.33 ms, 5.10 ms] | 4.85 ms [4.33 ms, 5.10 ms] | 2809.0 [2506.9, 2950.2] | 1.055 [1.006, 1.111] | same |
| heat_implicit_hex8_kvar | mesh | t8 | 5 | 1 | 4.65 ms [4.25 ms, 4.98 ms] | 4.65 ms [4.25 ms, 4.98 ms] | 2692.7 [2458.3, 2883.1] | 1.020 [0.975, 1.068] | same |
| heat_implicit_hex8_kvar | init | base | 5 | 1 | 768.0 µs [728.0 µs, 936.0 µs] | 768.0 µs [728.0 µs, 936.0 µs] | 444.4 [421.3, 541.7] | – | same |
| heat_implicit_hex8_kvar | init | t1 | 5 | 1 | 737.0 µs [695.0 µs, 962.0 µs] | 737.0 µs [695.0 µs, 962.0 µs] | 426.5 [402.2, 556.7] | 0.955 [0.791, 1.309] | same |
| heat_implicit_hex8_kvar | init | t2 | 5 | 1 | 780.0 µs [759.0 µs, 1.00 ms] | 780.0 µs [759.0 µs, 1.00 ms] | 451.4 [439.2, 579.3] | 1.043 [0.832, 1.303] | same |
| heat_implicit_hex8_kvar | init | t4 | 5 | 1 | 751.0 µs [731.0 µs, 832.0 µs] | 751.0 µs [731.0 µs, 832.0 µs] | 434.6 [423.0, 481.5] | 0.968 [0.781, 1.143] | same |
| heat_implicit_hex8_kvar | init | t8 | 5 | 1 | 731.0 µs [716.0 µs, 1.00 ms] | 731.0 µs [716.0 µs, 1.00 ms] | 423.0 [414.4, 581.0] | 0.976 [0.787, 1.294] | same |
| heat_implicit_hex8_kvar | solve_step | base | 5 | 5 | 573.59 ms [505.13 ms, 577.47 ms] | 114.72 ms [101.03 ms, 115.49 ms] | 66387.6 [58463.8, 66837.2] | – | same |
| heat_implicit_hex8_kvar | solve_step | t1 | 5 | 5 | 567.93 ms [505.55 ms, 571.84 ms] | 113.59 ms [101.11 ms, 114.37 ms] | 65732.1 [58512.8, 66185.0] | 0.996 [0.983, 1.013] | same |
| heat_implicit_hex8_kvar | solve_step | t2 | 5 | 5 | 547.08 ms [487.21 ms, 550.53 ms] | 109.42 ms [97.44 ms, 110.11 ms] | 63319.3 [56389.7, 63719.3] | 0.960 [0.947, 0.965] | same |
| heat_implicit_hex8_kvar | solve_step | t4 | 5 | 5 | 541.71 ms [476.54 ms, 544.58 ms] | 108.34 ms [95.31 ms, 108.92 ms] | 62697.3 [55155.3, 63030.1] | 0.949 [0.933, 0.965] | same |
| heat_implicit_hex8_kvar | solve_step | t8 | 5 | 5 | 532.86 ms [491.82 ms, 534.73 ms] | 106.57 ms [98.36 ms, 106.95 ms] | 61673.0 [56923.4, 61889.6] | 0.929 [0.926, 0.974] | same |
| heat_static_tri6 | mesh | base | 5 | 1 | 50.48 ms [45.64 ms, 51.59 ms] | 50.48 ms [45.64 ms, 51.59 ms] | 2085.8 [1885.7, 2131.8] | – | same |
| heat_static_tri6 | mesh | t1 | 5 | 1 | 50.26 ms [48.29 ms, 51.87 ms] | 50.26 ms [48.29 ms, 51.87 ms] | 2076.9 [1995.2, 2143.3] | 1.005 [0.959, 1.073] | same |
| heat_static_tri6 | mesh | t2 | 5 | 1 | 50.73 ms [45.33 ms, 51.82 ms] | 50.73 ms [45.33 ms, 51.82 ms] | 2096.5 [1873.2, 2141.5] | 0.994 [0.983, 1.023] | same |
| heat_static_tri6 | mesh | t4 | 5 | 1 | 51.16 ms [48.27 ms, 52.52 ms] | 51.16 ms [48.27 ms, 52.52 ms] | 2114.3 [1994.7, 2170.4] | 1.016 [0.984, 1.058] | same |
| heat_static_tri6 | mesh | t8 | 5 | 1 | 50.46 ms [48.24 ms, 52.43 ms] | 50.46 ms [48.24 ms, 52.43 ms] | 2085.0 [1993.4, 2166.4] | 1.026 [0.978, 1.057] | same |
| heat_static_tri6 | init | base | 5 | 1 | 5.03 ms [4.52 ms, 5.08 ms] | 5.03 ms [4.52 ms, 5.08 ms] | 208.0 [186.8, 210.1] | – | same |
| heat_static_tri6 | init | t1 | 5 | 1 | 5.04 ms [4.99 ms, 5.07 ms] | 5.04 ms [4.99 ms, 5.07 ms] | 208.3 [206.2, 209.7] | 0.998 [0.994, 1.109] | same |
| heat_static_tri6 | init | t2 | 5 | 1 | 4.96 ms [4.36 ms, 5.13 ms] | 4.96 ms [4.36 ms, 5.13 ms] | 205.0 [180.1, 211.9] | 0.985 [0.933, 1.022] | same |
| heat_static_tri6 | init | t4 | 5 | 1 | 4.90 ms [4.68 ms, 5.06 ms] | 4.90 ms [4.68 ms, 5.06 ms] | 202.4 [193.5, 209.1] | 0.976 [0.968, 1.036] | same |
| heat_static_tri6 | init | t8 | 5 | 1 | 4.99 ms [4.57 ms, 5.20 ms] | 4.99 ms [4.57 ms, 5.20 ms] | 206.2 [188.7, 214.8] | 0.992 [0.910, 1.042] | same |
| heat_static_tri6 | assemble_conductivity | base | 5 | 5 | 44.03 ms [38.86 ms, 46.22 ms] | 8.81 ms [7.77 ms, 9.24 ms] | 363.9 [321.2, 382.0] | – | same |
| heat_static_tri6 | assemble_conductivity | t1 | 5 | 5 | 35.71 ms [34.52 ms, 38.95 ms] | 7.14 ms [6.90 ms, 7.79 ms] | 295.2 [285.3, 321.9] | 0.839 [0.747, 0.893] | same |
| heat_static_tri6 | assemble_conductivity | t2 | 5 | 5 | 20.81 ms [17.96 ms, 20.92 ms] | 4.16 ms [3.59 ms, 4.18 ms] | 172.0 [148.4, 172.9] | 0.457 [0.450, 0.492] | same |
| heat_static_tri6 | assemble_conductivity | t4 | 5 | 5 | 13.34 ms [12.32 ms, 13.50 ms] | 2.67 ms [2.46 ms, 2.70 ms] | 110.3 [101.8, 111.5] | 0.303 [0.289, 0.317] | same |
| heat_static_tri6 | assemble_conductivity | t8 | 5 | 5 | 9.38 ms [9.30 ms, 9.73 ms] | 1.88 ms [1.86 ms, 1.95 ms] | 77.5 [76.9, 80.4] | 0.213 [0.206, 0.239] | same |
| heat_static_tri6 | solve_step | base | 5 | 1 | 1.961 s [1.756 s, 1.978 s] | 1.961 s [1.756 s, 1.978 s] | 81044.2 [72553.6, 81717.7] | – | same |
| heat_static_tri6 | solve_step | t1 | 5 | 1 | 1.970 s [1.841 s, 1.982 s] | 1.970 s [1.841 s, 1.982 s] | 81405.9 [76071.6, 81921.0] | 1.004 [0.998, 1.048] | same |
| heat_static_tri6 | solve_step | t2 | 5 | 1 | 1.956 s [1.766 s, 1.979 s] | 1.956 s [1.766 s, 1.979 s] | 80811.5 [72979.3, 81775.2] | 1.001 [0.990, 1.006] | same |
| heat_static_tri6 | solve_step | t4 | 5 | 1 | 1.945 s [1.753 s, 1.972 s] | 1.945 s [1.753 s, 1.972 s] | 80356.6 [72419.4, 81473.4] | 0.994 [0.987, 1.005] | same |
| heat_static_tri6 | solve_step | t8 | 5 | 1 | 1.945 s [1.752 s, 1.979 s] | 1.945 s [1.752 s, 1.979 s] | 80371.1 [72388.8, 81779.3] | 0.998 [0.983, 1.001] | same |
| phasefield_staggered_tet4 | mesh | base | 5 | 1 | 8.88 ms [8.83 ms, 8.91 ms] | 8.88 ms [8.83 ms, 8.91 ms] | 1479.8 [1471.5, 1485.2] | – | same |
| phasefield_staggered_tet4 | mesh | t1 | 5 | 1 | 8.87 ms [8.82 ms, 9.06 ms] | 8.87 ms [8.82 ms, 9.06 ms] | 1477.5 [1469.8, 1509.5] | 1.001 [0.993, 1.016] | same |
| phasefield_staggered_tet4 | mesh | t2 | 5 | 1 | 8.74 ms [8.70 ms, 9.50 ms] | 8.74 ms [8.70 ms, 9.50 ms] | 1456.3 [1450.0, 1583.0] | 0.985 [0.976, 1.076] | same |
| phasefield_staggered_tet4 | mesh | t4 | 5 | 1 | 9.07 ms [8.94 ms, 9.29 ms] | 9.07 ms [8.94 ms, 9.29 ms] | 1512.3 [1490.3, 1549.2] | 1.028 [1.006, 1.047] | same |
| phasefield_staggered_tet4 | mesh | t8 | 5 | 1 | 9.11 ms [9.05 ms, 9.57 ms] | 9.11 ms [9.05 ms, 9.57 ms] | 1517.7 [1508.0, 1595.5] | 1.026 [1.015, 1.077] | same |
| phasefield_staggered_tet4 | init | base | 5 | 1 | 2.63 ms [2.59 ms, 2.67 ms] | 2.63 ms [2.59 ms, 2.67 ms] | 437.7 [431.2, 444.2] | – | same |
| phasefield_staggered_tet4 | init | t1 | 5 | 1 | 2.40 ms [2.29 ms, 2.75 ms] | 2.40 ms [2.29 ms, 2.75 ms] | 399.8 [381.8, 458.2] | 0.923 [0.860, 1.063] | same |
| phasefield_staggered_tet4 | init | t2 | 5 | 1 | 2.34 ms [2.26 ms, 2.68 ms] | 2.34 ms [2.26 ms, 2.68 ms] | 389.7 [376.8, 446.8] | 0.893 [0.848, 1.031] | same |
| phasefield_staggered_tet4 | init | t4 | 5 | 1 | 2.54 ms [2.51 ms, 2.75 ms] | 2.54 ms [2.51 ms, 2.75 ms] | 422.5 [417.7, 458.3] | 0.975 [0.956, 1.047] | same |
| phasefield_staggered_tet4 | init | t8 | 5 | 1 | 2.64 ms [2.61 ms, 2.71 ms] | 2.64 ms [2.61 ms, 2.71 ms] | 440.0 [434.3, 451.5] | 1.015 [0.998, 1.032] | same |
| phasefield_staggered_tet4 | solid_solve | base | 5 | 4 | 592.22 ms [590.15 ms, 616.61 ms] | 148.05 ms [147.54 ms, 154.15 ms] | 24675.7 [24589.8, 25692.1] | – | same |
| phasefield_staggered_tet4 | solid_solve | t1 | 5 | 4 | 580.09 ms [573.07 ms, 598.13 ms] | 145.02 ms [143.27 ms, 149.53 ms] | 24170.5 [23877.8, 24921.9] | 0.980 [0.964, 0.991] | same |
| phasefield_staggered_tet4 | solid_solve | t2 | 5 | 4 | 563.82 ms [555.26 ms, 582.16 ms] | 140.96 ms [138.82 ms, 145.54 ms] | 23492.6 [23135.9, 24256.8] | 0.949 [0.938, 0.967] | same |
| phasefield_staggered_tet4 | solid_solve | t4 | 5 | 4 | 557.25 ms [552.07 ms, 571.37 ms] | 139.31 ms [138.02 ms, 142.84 ms] | 23218.9 [23002.7, 23807.1] | 0.936 [0.927, 0.956] | same |
| phasefield_staggered_tet4 | solid_solve | t8 | 5 | 4 | 552.54 ms [547.52 ms, 556.17 ms] | 138.13 ms [136.88 ms, 139.04 ms] | 23022.5 [22813.3, 23173.9] | 0.929 [0.902, 0.941] | same |
| phasefield_staggered_tet4 | strain_transfer | base | 5 | 4 | 743.0 µs [698.0 µs, 1.02 ms] | 185.8 µs [174.5 µs, 254.0 µs] | 31.0 [29.1, 42.3] | – | same |
| phasefield_staggered_tet4 | strain_transfer | t1 | 5 | 4 | 769.0 µs [740.0 µs, 903.0 µs] | 192.2 µs [185.0 µs, 225.8 µs] | 32.0 [30.8, 37.6] | 1.003 [0.889, 1.094] | same |
| phasefield_staggered_tet4 | strain_transfer | t2 | 5 | 4 | 799.0 µs [771.0 µs, 1.02 ms] | 199.8 µs [192.8 µs, 254.5 µs] | 33.3 [32.1, 42.4] | 1.061 [1.002, 1.122] | same |
| phasefield_staggered_tet4 | strain_transfer | t4 | 5 | 4 | 783.0 µs [774.0 µs, 995.0 µs] | 195.8 µs [193.5 µs, 248.8 µs] | 32.6 [32.2, 41.5] | 1.042 [0.979, 1.122] | same |
| phasefield_staggered_tet4 | strain_transfer | t8 | 5 | 4 | 781.0 µs [760.0 µs, 809.0 µs] | 195.2 µs [190.0 µs, 202.2 µs] | 32.5 [31.7, 33.7] | 1.023 [0.796, 1.119] | same |
| phasefield_staggered_tet4 | phase_solve | base | 5 | 4 | 69.29 ms [64.55 ms, 70.80 ms] | 17.32 ms [16.14 ms, 17.70 ms] | 2887.2 [2689.6, 2949.9] | – | same |
| phasefield_staggered_tet4 | phase_solve | t1 | 5 | 4 | 68.63 ms [65.97 ms, 70.25 ms] | 17.16 ms [16.49 ms, 17.56 ms] | 2859.8 [2748.6, 2926.9] | 0.996 [0.990, 1.022] | same |
| phasefield_staggered_tet4 | phase_solve | t2 | 5 | 4 | 66.29 ms [65.60 ms, 67.61 ms] | 16.57 ms [16.40 ms, 16.90 ms] | 2762.0 [2733.4, 2817.1] | 0.957 [0.940, 1.031] | same |
| phasefield_staggered_tet4 | phase_solve | t4 | 5 | 4 | 66.43 ms [65.81 ms, 68.16 ms] | 16.61 ms [16.45 ms, 17.04 ms] | 2768.1 [2742.1, 2839.9] | 0.963 [0.943, 1.029] | same |
| phasefield_staggered_tet4 | phase_solve | t8 | 5 | 4 | 66.59 ms [65.73 ms, 66.89 ms] | 16.65 ms [16.43 ms, 16.72 ms] | 2774.5 [2738.6, 2787.2] | 0.952 [0.942, 1.032] | same |
| phasefield_staggered_tet4 | damage_transfer | base | 5 | 4 | 1.45 ms [1.39 ms, 1.49 ms] | 362.8 µs [348.2 µs, 372.5 µs] | 60.5 [58.0, 62.1] | – | same |
| phasefield_staggered_tet4 | damage_transfer | t1 | 5 | 4 | 1.36 ms [1.28 ms, 1.40 ms] | 341.2 µs [318.8 µs, 349.0 µs] | 56.9 [53.1, 58.2] | 0.917 [0.913, 0.982] | same |
| phasefield_staggered_tet4 | damage_transfer | t2 | 5 | 4 | 1.36 ms [1.33 ms, 1.37 ms] | 339.2 µs [331.5 µs, 341.5 µs] | 56.5 [55.2, 56.9] | 0.936 [0.890, 0.979] | same |
| phasefield_staggered_tet4 | damage_transfer | t4 | 5 | 4 | 1.37 ms [1.36 ms, 1.38 ms] | 343.2 µs [339.2 µs, 344.2 µs] | 57.2 [56.5, 57.4] | 0.946 [0.923, 0.989] | same |
| phasefield_staggered_tet4 | damage_transfer | t8 | 5 | 4 | 1.38 ms [1.36 ms, 1.40 ms] | 346.0 µs [339.5 µs, 351.0 µs] | 57.7 [56.6, 58.5] | 0.955 [0.932, 0.991] | same |
| phasefield_staggered_tet4 | internal_forces | base | 5 | 4 | 1.88 ms [1.81 ms, 2.30 ms] | 471.0 µs [452.0 µs, 573.8 µs] | 78.5 [75.3, 95.6] | – | same |
| phasefield_staggered_tet4 | internal_forces | t1 | 5 | 4 | 1.99 ms [1.85 ms, 2.03 ms] | 497.8 µs [463.2 µs, 508.2 µs] | 83.0 [77.2, 84.7] | 1.044 [0.886, 1.074] | same |
| phasefield_staggered_tet4 | internal_forces | t2 | 5 | 4 | 1.44 ms [1.42 ms, 1.60 ms] | 359.0 µs [354.5 µs, 399.0 µs] | 59.8 [59.1, 66.5] | 0.765 [0.695, 0.794] | same |
| phasefield_staggered_tet4 | internal_forces | t4 | 5 | 4 | 1.36 ms [1.34 ms, 1.50 ms] | 341.0 µs [336.2 µs, 374.5 µs] | 56.8 [56.0, 62.4] | 0.735 [0.653, 0.746] | same |
| phasefield_staggered_tet4 | internal_forces | t8 | 5 | 4 | 1.34 ms [1.31 ms, 1.35 ms] | 335.5 µs [327.5 µs, 336.5 µs] | 55.9 [54.6, 56.1] | 0.708 [0.585, 0.744] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base | 5 | 5 | 48.06 ms [46.79 ms, 48.50 ms] | 9.61 ms [9.36 ms, 9.70 ms] | 1601.9 [1559.8, 1616.5] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t1 | 5 | 5 | 24.61 ms [23.86 ms, 25.25 ms] | 4.92 ms [4.77 ms, 5.05 ms] | 820.5 [795.4, 841.8] | 0.510 [0.503, 0.540] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t2 | 5 | 5 | 14.12 ms [14.09 ms, 14.51 ms] | 2.82 ms [2.82 ms, 2.90 ms] | 470.7 [469.7, 483.7] | 0.297 [0.293, 0.302] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t4 | 5 | 5 | 9.52 ms [9.49 ms, 9.60 ms] | 1.90 ms [1.90 ms, 1.92 ms] | 317.3 [316.3, 320.1] | 0.200 [0.196, 0.204] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t8 | 5 | 5 | 6.27 ms [6.22 ms, 6.31 ms] | 1.25 ms [1.24 ms, 1.26 ms] | 209.0 [207.4, 210.3] | 0.131 [0.129, 0.134] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base | 5 | 5 | 8.98 ms [8.09 ms, 9.41 ms] | 1.80 ms [1.62 ms, 1.88 ms] | 299.2 [269.5, 313.6] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t1 | 5 | 5 | 8.22 ms [7.88 ms, 8.36 ms] | 1.64 ms [1.58 ms, 1.67 ms] | 273.9 [262.7, 278.5] | 0.915 [0.888, 0.975] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t2 | 5 | 5 | 6.42 ms [6.15 ms, 6.44 ms] | 1.28 ms [1.23 ms, 1.29 ms] | 214.0 [205.0, 214.6] | 0.710 [0.682, 0.796] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t4 | 5 | 5 | 6.00 ms [5.96 ms, 6.04 ms] | 1.20 ms [1.19 ms, 1.21 ms] | 199.8 [198.8, 201.3] | 0.664 [0.637, 0.747] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t8 | 5 | 5 | 5.79 ms [5.77 ms, 5.84 ms] | 1.16 ms [1.15 ms, 1.17 ms] | 192.9 [192.3, 194.8] | 0.645 [0.615, 0.714] | same |
| phasefield_staggered_tri3 | mesh | base | 5 | 1 | 56.31 ms [55.49 ms, 57.05 ms] | 56.31 ms [55.49 ms, 57.05 ms] | 1099.8 [1083.7, 1114.2] | – | same |
| phasefield_staggered_tri3 | mesh | t1 | 5 | 1 | 58.18 ms [57.00 ms, 58.86 ms] | 58.18 ms [57.00 ms, 58.86 ms] | 1136.3 [1113.2, 1149.6] | 1.021 [1.012, 1.056] | same |
| phasefield_staggered_tri3 | mesh | t2 | 5 | 1 | 58.17 ms [57.27 ms, 59.08 ms] | 58.17 ms [57.27 ms, 59.08 ms] | 1136.1 [1118.6, 1153.8] | 1.036 [1.017, 1.060] | same |
| phasefield_staggered_tri3 | mesh | t4 | 5 | 1 | 58.50 ms [57.49 ms, 58.87 ms] | 58.50 ms [57.49 ms, 58.87 ms] | 1142.6 [1122.9, 1149.8] | 1.027 [1.021, 1.057] | same |
| phasefield_staggered_tri3 | mesh | t8 | 5 | 1 | 57.80 ms [56.74 ms, 58.32 ms] | 57.80 ms [56.74 ms, 58.32 ms] | 1128.9 [1108.2, 1139.0] | 1.024 [1.000, 1.049] | same |
| phasefield_staggered_tri3 | init | base | 5 | 1 | 16.88 ms [15.55 ms, 16.96 ms] | 16.88 ms [15.55 ms, 16.96 ms] | 329.6 [303.7, 331.2] | – | same |
| phasefield_staggered_tri3 | init | t1 | 5 | 1 | 17.16 ms [15.91 ms, 17.32 ms] | 17.16 ms [15.91 ms, 17.32 ms] | 335.2 [310.8, 338.2] | 1.023 [0.989, 1.036] | same |
| phasefield_staggered_tri3 | init | t2 | 5 | 1 | 17.21 ms [16.03 ms, 17.64 ms] | 17.21 ms [16.03 ms, 17.64 ms] | 336.2 [313.1, 344.6] | 1.039 [0.949, 1.061] | same |
| phasefield_staggered_tri3 | init | t4 | 5 | 1 | 17.45 ms [17.03 ms, 17.70 ms] | 17.45 ms [17.03 ms, 17.70 ms] | 340.7 [332.7, 345.6] | 1.035 [1.028, 1.115] | same |
| phasefield_staggered_tri3 | init | t8 | 5 | 1 | 17.00 ms [16.71 ms, 17.95 ms] | 17.00 ms [16.71 ms, 17.95 ms] | 332.1 [326.4, 350.5] | 1.026 [1.003, 1.075] | same |
| phasefield_staggered_tri3 | solid_solve | base | 5 | 4 | 3.232 s [3.224 s, 3.245 s] | 808.07 ms [806.04 ms, 811.14 ms] | 15782.6 [15742.9, 15842.6] | – | same |
| phasefield_staggered_tri3 | solid_solve | t1 | 5 | 4 | 3.219 s [3.190 s, 3.259 s] | 804.79 ms [797.45 ms, 814.71 ms] | 15718.5 [15575.2, 15912.3] | 0.996 [0.989, 1.008] | same |
| phasefield_staggered_tri3 | solid_solve | t2 | 5 | 4 | 3.196 s [3.146 s, 3.264 s] | 799.02 ms [786.39 ms, 815.95 ms] | 15605.9 [15359.3, 15936.5] | 0.989 [0.974, 1.010] | same |
| phasefield_staggered_tri3 | solid_solve | t4 | 5 | 4 | 3.154 s [3.101 s, 3.166 s] | 788.58 ms [775.32 ms, 791.39 ms] | 15401.9 [15142.9, 15456.9] | 0.972 [0.962, 0.980] | same |
| phasefield_staggered_tri3 | solid_solve | t8 | 5 | 4 | 3.147 s [3.067 s, 3.158 s] | 786.74 ms [766.87 ms, 789.41 ms] | 15366.0 [14977.9, 15418.2] | 0.973 [0.949, 0.978] | same |
| phasefield_staggered_tri3 | strain_transfer | base | 5 | 4 | 4.32 ms [4.25 ms, 4.44 ms] | 1.08 ms [1.06 ms, 1.11 ms] | 21.1 [20.8, 21.7] | – | same |
| phasefield_staggered_tri3 | strain_transfer | t1 | 5 | 4 | 4.51 ms [4.44 ms, 4.73 ms] | 1.13 ms [1.11 ms, 1.18 ms] | 22.0 [21.7, 23.1] | 1.061 [1.016, 1.082] | same |
| phasefield_staggered_tri3 | strain_transfer | t2 | 5 | 4 | 4.69 ms [4.59 ms, 5.04 ms] | 1.17 ms [1.15 ms, 1.26 ms] | 22.9 [22.4, 24.6] | 1.085 [1.055, 1.185] | same |
| phasefield_staggered_tri3 | strain_transfer | t4 | 5 | 4 | 4.69 ms [4.64 ms, 4.72 ms] | 1.17 ms [1.16 ms, 1.18 ms] | 22.9 [22.6, 23.0] | 1.083 [1.045, 1.110] | same |
| phasefield_staggered_tri3 | strain_transfer | t8 | 5 | 4 | 4.71 ms [4.66 ms, 4.93 ms] | 1.18 ms [1.17 ms, 1.23 ms] | 23.0 [22.8, 24.1] | 1.095 [1.051, 1.141] | same |
| phasefield_staggered_tri3 | phase_solve | base | 5 | 4 | 852.59 ms [837.58 ms, 868.66 ms] | 213.15 ms [209.40 ms, 217.16 ms] | 4163.0 [4089.8, 4241.5] | – | same |
| phasefield_staggered_tri3 | phase_solve | t1 | 5 | 4 | 852.32 ms [843.41 ms, 859.89 ms] | 213.08 ms [210.85 ms, 214.97 ms] | 4161.7 [4118.2, 4198.7] | 0.997 [0.983, 1.018] | same |
| phasefield_staggered_tri3 | phase_solve | t2 | 5 | 4 | 827.68 ms [813.15 ms, 845.42 ms] | 206.92 ms [203.29 ms, 211.36 ms] | 4041.4 [3970.5, 4128.1] | 0.973 [0.948, 0.988] | same |
| phasefield_staggered_tri3 | phase_solve | t4 | 5 | 4 | 809.30 ms [807.81 ms, 815.10 ms] | 202.33 ms [201.95 ms, 203.78 ms] | 3951.7 [3944.4, 3980.0] | 0.950 [0.938, 0.964] | same |
| phasefield_staggered_tri3 | phase_solve | t8 | 5 | 4 | 807.94 ms [787.99 ms, 818.12 ms] | 201.99 ms [197.00 ms, 204.53 ms] | 3945.0 [3847.6, 3994.7] | 0.943 [0.907, 0.977] | same |
| phasefield_staggered_tri3 | damage_transfer | base | 5 | 4 | 9.96 ms [9.45 ms, 10.26 ms] | 2.49 ms [2.36 ms, 2.57 ms] | 48.6 [46.2, 50.1] | – | same |
| phasefield_staggered_tri3 | damage_transfer | t1 | 5 | 4 | 9.89 ms [9.65 ms, 10.38 ms] | 2.47 ms [2.41 ms, 2.60 ms] | 48.3 [47.1, 50.7] | 0.997 [0.976, 1.038] | same |
| phasefield_staggered_tri3 | damage_transfer | t2 | 5 | 4 | 10.35 ms [10.12 ms, 10.46 ms] | 2.59 ms [2.53 ms, 2.62 ms] | 50.5 [49.4, 51.1] | 1.033 [1.009, 1.085] | same |
| phasefield_staggered_tri3 | damage_transfer | t4 | 5 | 4 | 10.41 ms [10.26 ms, 10.55 ms] | 2.60 ms [2.56 ms, 2.64 ms] | 50.8 [50.1, 51.5] | 1.046 [1.028, 1.085] | same |
| phasefield_staggered_tri3 | damage_transfer | t8 | 5 | 4 | 10.39 ms [10.29 ms, 10.68 ms] | 2.60 ms [2.57 ms, 2.67 ms] | 50.7 [50.2, 52.1] | 1.036 [1.003, 1.111] | same |
| phasefield_staggered_tri3 | internal_forces | base | 5 | 4 | 13.33 ms [13.11 ms, 13.97 ms] | 3.33 ms [3.28 ms, 3.49 ms] | 65.1 [64.0, 68.2] | – | same |
| phasefield_staggered_tri3 | internal_forces | t1 | 5 | 4 | 13.34 ms [13.23 ms, 14.00 ms] | 3.34 ms [3.31 ms, 3.50 ms] | 65.1 [64.6, 68.4] | 1.007 [0.991, 1.050] | same |
| phasefield_staggered_tri3 | internal_forces | t2 | 5 | 4 | 8.87 ms [8.69 ms, 9.42 ms] | 2.22 ms [2.17 ms, 2.36 ms] | 43.3 [42.4, 46.0] | 0.665 [0.654, 0.676] | same |
| phasefield_staggered_tri3 | internal_forces | t4 | 5 | 4 | 6.72 ms [6.69 ms, 6.75 ms] | 1.68 ms [1.67 ms, 1.69 ms] | 32.8 [32.7, 33.0] | 0.505 [0.483, 0.513] | same |
| phasefield_staggered_tri3 | internal_forces | t8 | 5 | 4 | 5.58 ms [5.55 ms, 5.77 ms] | 1.39 ms [1.39 ms, 1.44 ms] | 27.2 [27.1, 28.2] | 0.423 [0.399, 0.440] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base | 5 | 5 | 72.13 ms [69.65 ms, 73.64 ms] | 14.43 ms [13.93 ms, 14.73 ms] | 281.8 [272.1, 287.7] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t1 | 5 | 5 | 58.47 ms [56.31 ms, 61.44 ms] | 11.69 ms [11.26 ms, 12.29 ms] | 228.4 [219.9, 240.0] | 0.824 [0.781, 0.839] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t2 | 5 | 5 | 33.32 ms [32.56 ms, 33.40 ms] | 6.66 ms [6.51 ms, 6.68 ms] | 130.2 [127.2, 130.5] | 0.463 [0.442, 0.478] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t4 | 5 | 5 | 22.02 ms [21.90 ms, 22.45 ms] | 4.40 ms [4.38 ms, 4.49 ms] | 86.0 [85.6, 87.7] | 0.305 [0.299, 0.316] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t8 | 5 | 5 | 17.67 ms [17.17 ms, 17.86 ms] | 3.53 ms [3.43 ms, 3.57 ms] | 69.0 [67.1, 69.8] | 0.247 [0.234, 0.252] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base | 5 | 5 | 61.05 ms [59.29 ms, 62.79 ms] | 12.21 ms [11.86 ms, 12.56 ms] | 238.5 [231.6, 245.3] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t1 | 5 | 5 | 56.95 ms [54.59 ms, 58.34 ms] | 11.39 ms [10.92 ms, 11.67 ms] | 222.5 [213.2, 227.9] | 0.938 [0.889, 0.961] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t2 | 5 | 5 | 40.67 ms [40.32 ms, 42.22 ms] | 8.13 ms [8.06 ms, 8.44 ms] | 158.9 [157.5, 164.9] | 0.661 [0.648, 0.703] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t4 | 5 | 5 | 37.60 ms [37.10 ms, 37.90 ms] | 7.52 ms [7.42 ms, 7.58 ms] | 146.9 [144.9, 148.0] | 0.614 [0.601, 0.634] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t8 | 5 | 5 | 36.51 ms [36.35 ms, 37.10 ms] | 7.30 ms [7.27 ms, 7.42 ms] | 142.6 [142.0, 144.9] | 0.600 [0.579, 0.616] | same |
| structural_beam3_frame_dynamic | mesh | base | 5 | 1 | 287.0 µs [282.0 µs, 290.0 µs] | 287.0 µs [282.0 µs, 290.0 µs] | 47.2 [46.4, 47.7] | – | same |
| structural_beam3_frame_dynamic | mesh | t1 | 5 | 1 | 287.0 µs [271.0 µs, 315.0 µs] | 287.0 µs [271.0 µs, 315.0 µs] | 47.2 [44.5, 51.8] | 0.990 [0.961, 1.098] | same |
| structural_beam3_frame_dynamic | mesh | t2 | 5 | 1 | 262.0 µs [245.0 µs, 289.0 µs] | 262.0 µs [245.0 µs, 289.0 µs] | 43.1 [40.3, 47.5] | 0.913 [0.845, 1.018] | same |
| structural_beam3_frame_dynamic | mesh | t4 | 5 | 1 | 283.0 µs [257.0 µs, 290.0 µs] | 283.0 µs [257.0 µs, 290.0 µs] | 46.5 [42.2, 47.7] | 0.993 [0.911, 1.003] | same |
| structural_beam3_frame_dynamic | mesh | t8 | 5 | 1 | 298.0 µs [288.0 µs, 304.0 µs] | 298.0 µs [288.0 µs, 304.0 µs] | 49.0 [47.3, 50.0] | 1.041 [1.014, 1.059] | same |
| structural_beam3_frame_dynamic | init | base | 5 | 1 | 27.25 ms [26.48 ms, 27.82 ms] | 27.25 ms [26.48 ms, 27.82 ms] | 4479.1 [4351.9, 4573.3] | – | same |
| structural_beam3_frame_dynamic | init | t1 | 5 | 1 | 26.74 ms [24.58 ms, 27.55 ms] | 26.74 ms [24.58 ms, 27.55 ms] | 4395.0 [4039.6, 4529.1] | 0.981 [0.928, 1.006] | same |
| structural_beam3_frame_dynamic | init | t2 | 5 | 1 | 27.10 ms [26.89 ms, 27.26 ms] | 27.10 ms [26.89 ms, 27.26 ms] | 4453.6 [4420.4, 4480.8] | 1.000 [0.967, 1.023] | same |
| structural_beam3_frame_dynamic | init | t4 | 5 | 1 | 26.97 ms [24.90 ms, 27.15 ms] | 26.97 ms [24.90 ms, 27.15 ms] | 4432.8 [4092.0, 4462.9] | 0.977 [0.940, 1.015] | same |
| structural_beam3_frame_dynamic | init | t8 | 5 | 1 | 26.91 ms [25.85 ms, 27.64 ms] | 26.91 ms [25.85 ms, 27.64 ms] | 4423.1 [4249.5, 4543.4] | 0.990 [0.969, 1.011] | same |
| structural_beam3_frame_dynamic | solve_step | base | 5 | 60 | 3.152 s [2.930 s, 3.154 s] | 52.54 ms [48.83 ms, 52.56 ms] | 8636.0 [8026.0, 8638.8] | – | same |
| structural_beam3_frame_dynamic | solve_step | t1 | 5 | 60 | 3.155 s [3.090 s, 3.194 s] | 52.58 ms [51.50 ms, 53.23 ms] | 8643.1 [8465.0, 8748.6] | 1.012 [1.000, 1.055] | same |
| structural_beam3_frame_dynamic | solve_step | t2 | 5 | 60 | 3.164 s [3.140 s, 3.190 s] | 52.74 ms [52.34 ms, 53.17 ms] | 8667.8 [8602.3, 8738.5] | 1.005 [0.999, 1.079] | same |
| structural_beam3_frame_dynamic | solve_step | t4 | 5 | 60 | 3.165 s [2.963 s, 3.175 s] | 52.74 ms [49.39 ms, 52.92 ms] | 8669.4 [8117.7, 8698.9] | 1.007 [1.000, 1.011] | same |
| structural_beam3_frame_dynamic | solve_step | t8 | 5 | 60 | 3.171 s [3.055 s, 3.197 s] | 52.85 ms [50.92 ms, 53.28 ms] | 8687.0 [8370.0, 8757.6] | 1.014 [1.004, 1.043] | same |
| structural_beam3_frame_static | mesh | base | 5 | 1 | 294.0 µs [234.0 µs, 321.0 µs] | 294.0 µs [234.0 µs, 321.0 µs] | 48.3 [38.5, 52.8] | – | same |
| structural_beam3_frame_static | mesh | t1 | 5 | 1 | 297.0 µs [244.0 µs, 304.0 µs] | 297.0 µs [244.0 µs, 304.0 µs] | 48.8 [40.1, 50.0] | 1.023 [0.760, 1.299] | same |
| structural_beam3_frame_static | mesh | t2 | 5 | 1 | 280.0 µs [250.0 µs, 313.0 µs] | 280.0 µs [250.0 µs, 313.0 µs] | 46.0 [41.1, 51.4] | 0.990 [0.814, 1.197] | same |
| structural_beam3_frame_static | mesh | t4 | 5 | 1 | 285.0 µs [280.0 µs, 315.0 µs] | 285.0 µs [280.0 µs, 315.0 µs] | 46.8 [46.0, 51.8] | 0.960 [0.928, 1.346] | same |
| structural_beam3_frame_static | mesh | t8 | 5 | 1 | 289.0 µs [250.0 µs, 305.0 µs] | 289.0 µs [250.0 µs, 305.0 µs] | 47.5 [41.1, 50.1] | 0.935 [0.850, 1.269] | same |
| structural_beam3_frame_static | init | base | 5 | 1 | 26.80 ms [25.90 ms, 27.04 ms] | 26.80 ms [25.90 ms, 27.04 ms] | 4405.7 [4257.6, 4443.8] | – | same |
| structural_beam3_frame_static | init | t1 | 5 | 1 | 26.77 ms [25.55 ms, 26.92 ms] | 26.77 ms [25.55 ms, 26.92 ms] | 4399.2 [4200.2, 4424.9] | 0.990 [0.945, 1.034] | same |
| structural_beam3_frame_static | init | t2 | 5 | 1 | 26.64 ms [24.22 ms, 26.91 ms] | 26.64 ms [24.22 ms, 26.91 ms] | 4379.2 [3980.3, 4423.1] | 0.994 [0.917, 1.033] | same |
| structural_beam3_frame_static | init | t4 | 5 | 1 | 26.81 ms [22.40 ms, 27.08 ms] | 26.81 ms [22.40 ms, 27.08 ms] | 4407.1 [3681.5, 4451.5] | 0.998 [0.848, 1.046] | same |
| structural_beam3_frame_static | init | t8 | 5 | 1 | 26.79 ms [25.81 ms, 27.48 ms] | 26.79 ms [25.81 ms, 27.48 ms] | 4402.9 [4242.4, 4516.4] | 0.997 [0.963, 1.035] | same |
| structural_beam3_frame_static | assemble_stiffness | base | 5 | 5 | 34.41 ms [32.41 ms, 36.54 ms] | 6.88 ms [6.48 ms, 7.31 ms] | 1131.3 [1065.3, 1201.1] | – | same |
| structural_beam3_frame_static | assemble_stiffness | t1 | 5 | 5 | 31.32 ms [29.87 ms, 33.09 ms] | 6.26 ms [5.97 ms, 6.62 ms] | 1029.7 [981.9, 1087.9] | 0.922 [0.851, 0.976] | same |
| structural_beam3_frame_static | assemble_stiffness | t2 | 5 | 5 | 31.21 ms [28.32 ms, 33.02 ms] | 6.24 ms [5.66 ms, 6.60 ms] | 1026.0 [931.1, 1085.6] | 0.907 [0.854, 0.965] | same |
| structural_beam3_frame_static | assemble_stiffness | t4 | 5 | 5 | 31.39 ms [28.70 ms, 32.18 ms] | 6.28 ms [5.74 ms, 6.44 ms] | 1032.0 [943.5, 1058.0] | 0.890 [0.881, 0.931] | same |
| structural_beam3_frame_static | assemble_stiffness | t8 | 5 | 5 | 30.89 ms [30.80 ms, 31.98 ms] | 6.18 ms [6.16 ms, 6.40 ms] | 1015.5 [1012.7, 1051.4] | 0.907 [0.844, 0.959] | same |
| structural_beam3_frame_static | solve_step | base | 5 | 1 | 1.173 s [1.063 s, 1.186 s] | 1.173 s [1.063 s, 1.186 s] | 192873.1 [174645.1, 195013.3] | – | same |
| structural_beam3_frame_static | solve_step | t1 | 5 | 1 | 1.181 s [1.083 s, 1.193 s] | 1.181 s [1.083 s, 1.193 s] | 194158.9 [178083.7, 196015.5] | 1.007 [1.005, 1.020] | same |
| structural_beam3_frame_static | solve_step | t2 | 5 | 1 | 1.169 s [1.073 s, 1.187 s] | 1.169 s [1.073 s, 1.187 s] | 192214.5 [176296.2, 195170.3] | 1.004 [0.997, 1.009] | same |
| structural_beam3_frame_static | solve_step | t4 | 5 | 1 | 1.182 s [1.056 s, 1.195 s] | 1.182 s [1.056 s, 1.195 s] | 194287.8 [173542.1, 196344.8] | 1.008 [0.991, 1.019] | same |
| structural_beam3_frame_static | solve_step | t8 | 5 | 1 | 1.166 s [1.067 s, 1.183 s] | 1.166 s [1.067 s, 1.183 s] | 191694.3 [175407.5, 194491.9] | 1.004 [0.993, 1.005] | same |
| structural_dkt18_plate_static | mesh | base | 5 | 1 | 237.0 µs [220.0 µs, 257.0 µs] | 237.0 µs [220.0 µs, 257.0 µs] | 32.9 [30.6, 35.7] | – | same |
| structural_dkt18_plate_static | mesh | t1 | 5 | 1 | 264.0 µs [261.0 µs, 276.0 µs] | 264.0 µs [261.0 µs, 276.0 µs] | 36.7 [36.2, 38.3] | 1.156 [1.027, 1.186] | same |
| structural_dkt18_plate_static | mesh | t2 | 5 | 1 | 258.0 µs [230.0 µs, 260.0 µs] | 258.0 µs [230.0 µs, 260.0 µs] | 35.8 [31.9, 36.1] | 1.067 [0.987, 1.182] | same |
| structural_dkt18_plate_static | mesh | t4 | 5 | 1 | 230.0 µs [224.0 µs, 274.0 µs] | 230.0 µs [224.0 µs, 274.0 µs] | 31.9 [31.1, 38.1] | 1.023 [0.895, 1.156] | same |
| structural_dkt18_plate_static | mesh | t8 | 5 | 1 | 241.0 µs [221.0 µs, 258.0 µs] | 241.0 µs [221.0 µs, 258.0 µs] | 33.5 [30.7, 35.8] | 0.981 [0.932, 1.107] | same |
| structural_dkt18_plate_static | init | base | 5 | 1 | 63.03 ms [62.14 ms, 64.25 ms] | 63.03 ms [62.14 ms, 64.25 ms] | 8754.7 [8630.7, 8923.1] | – | same |
| structural_dkt18_plate_static | init | t1 | 5 | 1 | 64.75 ms [64.10 ms, 65.12 ms] | 64.75 ms [64.10 ms, 65.12 ms] | 8993.7 [8902.8, 9044.0] | 1.023 [1.002, 1.048] | same |
| structural_dkt18_plate_static | init | t2 | 5 | 1 | 64.76 ms [62.02 ms, 65.85 ms] | 64.76 ms [62.02 ms, 65.85 ms] | 8994.3 [8614.2, 9145.6] | 1.020 [0.969, 1.060] | same |
| structural_dkt18_plate_static | init | t4 | 5 | 1 | 64.11 ms [62.69 ms, 65.01 ms] | 64.11 ms [62.69 ms, 65.01 ms] | 8904.0 [8706.7, 9028.9] | 1.012 [0.980, 1.032] | same |
| structural_dkt18_plate_static | init | t8 | 5 | 1 | 64.08 ms [61.98 ms, 64.31 ms] | 64.08 ms [61.98 ms, 64.31 ms] | 8899.7 [8608.9, 8931.2] | 1.005 [0.969, 1.035] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 5 | 5 | 221.72 ms [219.32 ms, 223.75 ms] | 44.34 ms [43.87 ms, 44.75 ms] | 6158.9 [6092.4, 6215.2] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | t1 | 5 | 5 | 212.71 ms [211.26 ms, 215.86 ms] | 42.54 ms [42.25 ms, 43.17 ms] | 5908.7 [5868.3, 5996.2] | 0.963 [0.949, 0.983] | same |
| structural_dkt18_plate_static | assemble_stiffness | t2 | 5 | 5 | 209.41 ms [207.78 ms, 210.92 ms] | 41.88 ms [41.56 ms, 42.18 ms] | 5816.9 [5771.7, 5858.8] | 0.944 [0.933, 0.955] | same |
| structural_dkt18_plate_static | assemble_stiffness | t4 | 5 | 5 | 210.15 ms [207.44 ms, 212.03 ms] | 42.03 ms [41.49 ms, 42.41 ms] | 5837.6 [5762.4, 5889.7] | 0.946 [0.944, 0.957] | same |
| structural_dkt18_plate_static | assemble_stiffness | t8 | 5 | 5 | 206.35 ms [205.43 ms, 212.95 ms] | 41.27 ms [41.09 ms, 42.59 ms] | 5732.0 [5706.4, 5915.4] | 0.939 [0.918, 0.962] | same |
| structural_dkt18_plate_static | solve_step | base | 5 | 1 | 920.75 ms [917.37 ms, 934.19 ms] | 920.75 ms [917.37 ms, 934.19 ms] | 127881.7 [127412.4, 129748.7] | – | same |
| structural_dkt18_plate_static | solve_step | t1 | 5 | 1 | 930.85 ms [925.25 ms, 940.06 ms] | 930.85 ms [925.25 ms, 940.06 ms] | 129284.9 [128507.2, 130563.9] | 1.013 [0.990, 1.015] | same |
| structural_dkt18_plate_static | solve_step | t2 | 5 | 1 | 935.82 ms [932.22 ms, 947.80 ms] | 935.82 ms [932.22 ms, 947.80 ms] | 129975.0 [129475.3, 131638.5] | 1.018 [1.002, 1.029] | same |
| structural_dkt18_plate_static | solve_step | t4 | 5 | 1 | 939.77 ms [931.78 ms, 946.81 ms] | 939.77 ms [931.78 ms, 946.81 ms] | 130523.5 [129414.3, 131500.8] | 1.016 [1.006, 1.028] | same |
| structural_dkt18_plate_static | solve_step | t8 | 5 | 1 | 920.30 ms [915.59 ms, 944.74 ms] | 920.30 ms [915.59 ms, 944.74 ms] | 127819.7 [127164.9, 131213.8] | 1.003 [0.985, 1.026] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 105ea0eb1, 72337cb4e | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 105ea0eb1, 72337cb4e | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 105ea0eb1, 72337cb4e | 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 105ea0eb1, 72337cb4e | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 105ea0eb1, 72337cb4e | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 105ea0eb1, 72337cb4e | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 105ea0eb1, 72337cb4e | a77809465b3e7c18 |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | 105ea0eb1, 72337cb4e | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 105ea0eb1, 72337cb4e | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 105ea0eb1, 72337cb4e | 1cfc7f010c52d6b3 |
