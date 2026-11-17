Suite `models`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | mesh | base | 5 | 1 | 11.39 ms [11.32 ms, 11.63 ms] | 11.39 ms [11.32 ms, 11.63 ms] | 1627.3 [1617.0, 1661.0] | – | same |
| contact_implicit_static_quad4 | mesh | t1 | 5 | 1 | 11.55 ms [11.49 ms, 12.38 ms] | 11.55 ms [11.49 ms, 12.38 ms] | 1649.6 [1640.7, 1768.6] | 1.013 [0.988, 1.094] | same |
| contact_implicit_static_quad4 | mesh | t2 | 5 | 1 | 11.54 ms [11.36 ms, 11.75 ms] | 11.54 ms [11.36 ms, 11.75 ms] | 1648.9 [1622.7, 1679.3] | 1.004 [0.996, 1.021] | same |
| contact_implicit_static_quad4 | mesh | t4 | 5 | 1 | 11.54 ms [11.49 ms, 12.02 ms] | 11.54 ms [11.49 ms, 12.02 ms] | 1648.9 [1641.9, 1716.9] | 1.016 [0.988, 1.055] | same |
| contact_implicit_static_quad4 | mesh | t8 | 5 | 1 | 11.71 ms [11.55 ms, 12.13 ms] | 11.71 ms [11.55 ms, 12.13 ms] | 1673.1 [1649.9, 1732.3] | 1.028 [0.993, 1.071] | same |
| contact_implicit_static_quad4 | init | base | 5 | 1 | 9.36 ms [9.12 ms, 9.59 ms] | 9.36 ms [9.12 ms, 9.59 ms] | 1336.9 [1303.1, 1370.0] | – | same |
| contact_implicit_static_quad4 | init | t1 | 5 | 1 | 9.13 ms [8.79 ms, 9.39 ms] | 9.13 ms [8.79 ms, 9.39 ms] | 1303.7 [1256.3, 1341.6] | 0.975 [0.952, 0.988] | same |
| contact_implicit_static_quad4 | init | t2 | 5 | 1 | 9.31 ms [8.91 ms, 9.36 ms] | 9.31 ms [8.91 ms, 9.36 ms] | 1330.0 [1272.7, 1337.0] | 1.000 [0.929, 1.021] | same |
| contact_implicit_static_quad4 | init | t4 | 5 | 1 | 9.37 ms [8.95 ms, 9.71 ms] | 9.37 ms [8.95 ms, 9.71 ms] | 1338.9 [1279.1, 1387.7] | 1.002 [0.979, 1.013] | same |
| contact_implicit_static_quad4 | init | t8 | 5 | 1 | 9.39 ms [9.36 ms, 9.41 ms] | 9.39 ms [9.36 ms, 9.41 ms] | 1341.4 [1337.0, 1344.9] | 1.003 [0.981, 1.032] | same |
| contact_implicit_static_quad4 | solve_step | base | 5 | 5 | 825.54 ms [806.87 ms, 839.91 ms] | 165.11 ms [161.37 ms, 167.98 ms] | 23586.8 [23053.3, 23997.3] | – | same |
| contact_implicit_static_quad4 | solve_step | t1 | 5 | 5 | 826.20 ms [782.24 ms, 831.29 ms] | 165.24 ms [156.45 ms, 166.26 ms] | 23605.8 [22349.8, 23751.2] | 0.992 [0.950, 1.024] | same |
| contact_implicit_static_quad4 | solve_step | t2 | 5 | 5 | 773.14 ms [762.70 ms, 816.11 ms] | 154.63 ms [152.54 ms, 163.22 ms] | 22089.8 [21791.4, 23317.4] | 0.944 [0.914, 0.972] | same |
| contact_implicit_static_quad4 | solve_step | t4 | 5 | 5 | 754.99 ms [741.89 ms, 787.21 ms] | 151.00 ms [148.38 ms, 157.44 ms] | 21571.1 [21196.8, 22491.7] | 0.907 [0.889, 0.954] | same |
| contact_implicit_static_quad4 | solve_step | t8 | 5 | 5 | 752.07 ms [736.09 ms, 757.71 ms] | 150.41 ms [147.22 ms, 151.54 ms] | 21487.7 [21031.0, 21648.9] | 0.916 [0.879, 0.932] | same |
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 29.15 ms [28.68 ms, 30.48 ms] | 29.15 ms [28.68 ms, 30.48 ms] | 1340.0 [1318.5, 1401.2] | – | same |
| embedded_rebar_tet4 | mesh | t1 | 5 | 1 | 29.50 ms [29.27 ms, 29.73 ms] | 29.50 ms [29.27 ms, 29.73 ms] | 1356.4 [1345.7, 1367.0] | 1.018 [0.968, 1.034] | same |
| embedded_rebar_tet4 | mesh | t2 | 5 | 1 | 29.78 ms [29.20 ms, 30.13 ms] | 29.78 ms [29.20 ms, 30.13 ms] | 1369.1 [1342.7, 1385.2] | 1.020 [0.977, 1.036] | same |
| embedded_rebar_tet4 | mesh | t4 | 5 | 1 | 29.17 ms [29.04 ms, 29.96 ms] | 29.17 ms [29.04 ms, 29.96 ms] | 1341.0 [1335.2, 1377.4] | 1.008 [0.983, 1.015] | same |
| embedded_rebar_tet4 | mesh | t8 | 5 | 1 | 29.70 ms [29.08 ms, 29.83 ms] | 29.70 ms [29.08 ms, 29.83 ms] | 1365.7 [1336.8, 1371.4] | 1.023 [0.954, 1.036] | same |
| embedded_rebar_tet4 | init | base | 5 | 1 | 1.409 s [1.398 s, 1.479 s] | 1.409 s [1.398 s, 1.479 s] | 64797.2 [64274.6, 67983.4] | – | same |
| embedded_rebar_tet4 | init | t1 | 5 | 1 | 1.405 s [1.390 s, 1.436 s] | 1.405 s [1.390 s, 1.436 s] | 64580.4 [63891.5, 66038.5] | 0.994 [0.950, 1.019] | same |
| embedded_rebar_tet4 | init | t2 | 5 | 1 | 1.404 s [1.396 s, 1.442 s] | 1.404 s [1.396 s, 1.442 s] | 64544.2 [64182.4, 66311.1] | 0.995 [0.949, 1.032] | same |
| embedded_rebar_tet4 | init | t4 | 5 | 1 | 1.403 s [1.393 s, 1.426 s] | 1.403 s [1.393 s, 1.426 s] | 64511.7 [64026.8, 65568.2] | 0.996 [0.948, 1.017] | same |
| embedded_rebar_tet4 | init | t8 | 5 | 1 | 1.420 s [1.400 s, 1.451 s] | 1.420 s [1.400 s, 1.451 s] | 65295.9 [64361.3, 66709.9] | 1.009 [0.960, 1.030] | same |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 6.976 s [6.895 s, 7.266 s] | 1.395 s [1.379 s, 1.453 s] | 64146.3 [63399.0, 66813.6] | – | same |
| embedded_rebar_tet4 | intersect | t1 | 5 | 5 | 7.111 s [6.888 s, 7.370 s] | 1.422 s [1.378 s, 1.474 s] | 65390.9 [63336.3, 67773.4] | 1.007 [0.992, 1.057] | same |
| embedded_rebar_tet4 | intersect | t2 | 5 | 5 | 7.106 s [6.906 s, 7.327 s] | 1.421 s [1.381 s, 1.465 s] | 65347.1 [63507.3, 67376.2] | 0.994 [0.978, 1.063] | same |
| embedded_rebar_tet4 | intersect | t4 | 5 | 5 | 7.075 s [6.904 s, 7.273 s] | 1.415 s [1.381 s, 1.455 s] | 65056.5 [63486.7, 66874.3] | 1.013 [0.953, 1.047] | same |
| embedded_rebar_tet4 | intersect | t8 | 5 | 5 | 7.266 s [6.930 s, 7.351 s] | 1.453 s [1.386 s, 1.470 s] | 66811.5 [63723.8, 67595.4] | 1.027 [0.998, 1.054] | same |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 1.148 s [1.128 s, 1.153 s] | 1.148 s [1.128 s, 1.153 s] | 52759.7 [51858.3, 53022.7] | – | same |
| embedded_rebar_tet4 | solve_step | t1 | 5 | 1 | 1.125 s [1.114 s, 1.136 s] | 1.125 s [1.114 s, 1.136 s] | 51713.5 [51227.1, 52252.0] | 0.985 [0.975, 0.994] | same |
| embedded_rebar_tet4 | solve_step | t2 | 5 | 1 | 1.112 s [1.100 s, 1.121 s] | 1.112 s [1.100 s, 1.121 s] | 51127.6 [50577.2, 51540.6] | 0.975 [0.964, 0.978] | same |
| embedded_rebar_tet4 | solve_step | t4 | 5 | 1 | 1.099 s [1.090 s, 1.112 s] | 1.099 s [1.090 s, 1.112 s] | 50507.4 [50093.1, 51134.8] | 0.960 [0.953, 0.973] | same |
| embedded_rebar_tet4 | solve_step | t8 | 5 | 1 | 1.102 s [1.098 s, 1.110 s] | 1.102 s [1.098 s, 1.110 s] | 50687.8 [50501.2, 51053.3] | 0.963 [0.961, 0.974] | same |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 35.62 ms [34.85 ms, 36.10 ms] | 1.78 ms [1.74 ms, 1.80 ms] | 81.9 [80.1, 83.0] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | t1 | 5 | 20 | 36.42 ms [34.82 ms, 36.70 ms] | 1.82 ms [1.74 ms, 1.83 ms] | 83.7 [80.0, 84.4] | 1.023 [0.974, 1.038] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t2 | 5 | 20 | 25.67 ms [24.79 ms, 26.17 ms] | 1.28 ms [1.24 ms, 1.31 ms] | 59.0 [57.0, 60.2] | 0.721 [0.706, 0.726] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t4 | 5 | 20 | 21.18 ms [21.11 ms, 21.41 ms] | 1.06 ms [1.06 ms, 1.07 ms] | 48.7 [48.5, 49.2] | 0.599 [0.586, 0.606] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t8 | 5 | 20 | 18.40 ms [18.29 ms, 24.46 ms] | 920.1 µs [914.3 µs, 1.22 ms] | 42.3 [42.0, 56.2] | 0.525 [0.516, 0.677] | same |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 201.66 ms [190.89 ms, 212.56 ms] | 40.33 ms [38.18 ms, 42.51 ms] | 1854.3 [1755.3, 1954.5] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | t1 | 5 | 5 | 123.34 ms [122.01 ms, 125.40 ms] | 24.67 ms [24.40 ms, 25.08 ms] | 1134.2 [1121.9, 1153.1] | 0.619 [0.590, 0.645] | same |
| embedded_rebar_tet4 | assemble_stiffness | t2 | 5 | 5 | 65.86 ms [64.48 ms, 67.42 ms] | 13.17 ms [12.90 ms, 13.48 ms] | 605.6 [592.9, 619.9] | 0.327 [0.303, 0.341] | same |
| embedded_rebar_tet4 | assemble_stiffness | t4 | 5 | 5 | 44.07 ms [43.91 ms, 44.99 ms] | 8.81 ms [8.78 ms, 9.00 ms] | 405.3 [403.8, 413.7] | 0.219 [0.208, 0.230] | same |
| embedded_rebar_tet4 | assemble_stiffness | t8 | 5 | 5 | 34.54 ms [34.48 ms, 35.31 ms] | 6.91 ms [6.90 ms, 7.06 ms] | 317.6 [317.1, 324.7] | 0.171 [0.162, 0.181] | same |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 27.90 ms [27.65 ms, 28.21 ms] | 27.90 ms [27.65 ms, 28.21 ms] | 1107.2 [1097.4, 1119.5] | – | same |
| embedded_rebar_tri3 | mesh | t1 | 5 | 1 | 28.51 ms [28.16 ms, 29.30 ms] | 28.51 ms [28.16 ms, 29.30 ms] | 1131.4 [1117.3, 1162.9] | 1.025 [0.998, 1.042] | same |
| embedded_rebar_tri3 | mesh | t2 | 5 | 1 | 28.00 ms [26.30 ms, 28.25 ms] | 28.00 ms [26.30 ms, 28.25 ms] | 1111.1 [1043.6, 1121.2] | 1.001 [0.945, 1.011] | same |
| embedded_rebar_tri3 | mesh | t4 | 5 | 1 | 28.14 ms [27.54 ms, 28.32 ms] | 28.14 ms [27.54 ms, 28.32 ms] | 1116.7 [1092.9, 1123.7] | 1.011 [0.979, 1.023] | same |
| embedded_rebar_tri3 | mesh | t8 | 5 | 1 | 28.48 ms [28.14 ms, 28.82 ms] | 28.48 ms [28.14 ms, 28.82 ms] | 1130.3 [1116.6, 1143.7] | 1.022 [1.001, 1.028] | same |
| embedded_rebar_tri3 | init | base | 5 | 1 | 1.577 s [1.517 s, 1.603 s] | 1.577 s [1.517 s, 1.603 s] | 62594.6 [60213.1, 63598.6] | – | same |
| embedded_rebar_tri3 | init | t1 | 5 | 1 | 1.559 s [1.526 s, 1.581 s] | 1.559 s [1.526 s, 1.581 s] | 61874.2 [60571.5, 62749.6] | 0.998 [0.956, 1.028] | same |
| embedded_rebar_tri3 | init | t2 | 5 | 1 | 1.564 s [1.519 s, 1.600 s] | 1.564 s [1.519 s, 1.600 s] | 62062.1 [60259.9, 63496.4] | 0.995 [0.963, 1.031] | same |
| embedded_rebar_tri3 | init | t4 | 5 | 1 | 1.516 s [1.504 s, 1.531 s] | 1.516 s [1.504 s, 1.531 s] | 60172.7 [59672.2, 60766.9] | 0.968 [0.946, 0.991] | same |
| embedded_rebar_tri3 | init | t8 | 5 | 1 | 1.595 s [1.508 s, 1.621 s] | 1.595 s [1.508 s, 1.621 s] | 63276.1 [59836.9, 64328.4] | 1.008 [0.960, 1.051] | same |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 7.549 s [7.489 s, 7.839 s] | 1.510 s [1.498 s, 1.568 s] | 59911.9 [59435.6, 62214.4] | – | same |
| embedded_rebar_tri3 | intersect | t1 | 5 | 5 | 7.540 s [7.425 s, 7.823 s] | 1.508 s [1.485 s, 1.565 s] | 59838.7 [58926.0, 62085.8] | 1.007 [0.947, 1.036] | same |
| embedded_rebar_tri3 | intersect | t2 | 5 | 5 | 7.579 s [7.562 s, 7.901 s] | 1.516 s [1.512 s, 1.580 s] | 60150.5 [60019.0, 62706.7] | 1.012 [0.965, 1.047] | same |
| embedded_rebar_tri3 | intersect | t4 | 5 | 5 | 7.895 s [7.835 s, 7.903 s] | 1.579 s [1.567 s, 1.581 s] | 62661.0 [62180.7, 62722.8] | 1.046 [1.008, 1.047] | same |
| embedded_rebar_tri3 | intersect | t8 | 5 | 5 | 7.546 s [7.481 s, 7.880 s] | 1.509 s [1.496 s, 1.576 s] | 59885.4 [59375.1, 62537.5] | 1.000 [0.954, 1.052] | same |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 260.07 ms [255.86 ms, 266.47 ms] | 260.07 ms [255.86 ms, 266.47 ms] | 10320.3 [10153.2, 10574.2] | – | same |
| embedded_rebar_tri3 | solve_step | t1 | 5 | 1 | 260.14 ms [257.34 ms, 260.59 ms] | 260.14 ms [257.34 ms, 260.59 ms] | 10323.1 [10212.0, 10340.7] | 1.000 [0.966, 1.009] | same |
| embedded_rebar_tri3 | solve_step | t2 | 5 | 1 | 258.35 ms [255.09 ms, 258.45 ms] | 258.35 ms [255.09 ms, 258.45 ms] | 10251.9 [10122.7, 10255.9] | 0.991 [0.970, 1.010] | same |
| embedded_rebar_tri3 | solve_step | t4 | 5 | 1 | 258.01 ms [256.05 ms, 264.90 ms] | 258.01 ms [256.05 ms, 264.90 ms] | 10238.6 [10160.9, 10511.9] | 1.000 [0.968, 1.024] | same |
| embedded_rebar_tri3 | solve_step | t8 | 5 | 1 | 255.72 ms [252.74 ms, 265.91 ms] | 255.72 ms [252.74 ms, 265.91 ms] | 10147.6 [10029.4, 10551.9] | 0.986 [0.956, 1.014] | same |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 48.97 ms [46.62 ms, 51.23 ms] | 2.45 ms [2.33 ms, 2.56 ms] | 97.2 [92.5, 101.7] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | t1 | 5 | 20 | 49.84 ms [49.19 ms, 50.68 ms] | 2.49 ms [2.46 ms, 2.53 ms] | 98.9 [97.6, 100.6] | 1.022 [0.960, 1.080] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t2 | 5 | 20 | 42.44 ms [39.75 ms, 42.72 ms] | 2.12 ms [1.99 ms, 2.14 ms] | 84.2 [78.9, 84.8] | 0.831 [0.815, 0.916] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t4 | 5 | 20 | 40.35 ms [39.74 ms, 40.73 ms] | 2.02 ms [1.99 ms, 2.04 ms] | 80.1 [78.9, 80.8] | 0.819 [0.793, 0.852] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t8 | 5 | 20 | 39.24 ms [38.84 ms, 41.17 ms] | 1.96 ms [1.94 ms, 2.06 ms] | 77.8 [77.1, 81.7] | 0.793 [0.765, 0.849] | same |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 46.99 ms [46.52 ms, 47.52 ms] | 9.40 ms [9.30 ms, 9.50 ms] | 373.0 [369.2, 377.2] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | t1 | 5 | 5 | 50.60 ms [50.43 ms, 51.66 ms] | 10.12 ms [10.09 ms, 10.33 ms] | 401.6 [400.2, 410.0] | 1.084 [1.065, 1.103] | same |
| embedded_rebar_tri3 | assemble_stiffness | t2 | 5 | 5 | 41.16 ms [41.06 ms, 41.69 ms] | 8.23 ms [8.21 ms, 8.34 ms] | 326.7 [325.9, 330.9] | 0.878 [0.864, 0.887] | same |
| embedded_rebar_tri3 | assemble_stiffness | t4 | 5 | 5 | 37.07 ms [36.79 ms, 37.19 ms] | 7.42 ms [7.36 ms, 7.44 ms] | 294.2 [292.0, 295.1] | 0.791 [0.774, 0.799] | same |
| embedded_rebar_tri3 | assemble_stiffness | t8 | 5 | 5 | 35.76 ms [34.92 ms, 36.37 ms] | 7.15 ms [6.98 ms, 7.27 ms] | 283.8 [277.1, 288.7] | 0.761 [0.750, 0.768] | same |
| heat_implicit_hex8_kvar | mesh | base | 5 | 1 | 4.68 ms [4.64 ms, 4.86 ms] | 4.68 ms [4.64 ms, 4.86 ms] | 2706.6 [2682.9, 2812.5] | – | same |
| heat_implicit_hex8_kvar | mesh | t1 | 5 | 1 | 4.77 ms [4.73 ms, 4.88 ms] | 4.77 ms [4.73 ms, 4.88 ms] | 2761.0 [2739.6, 2821.8] | 1.020 [0.983, 1.043] | same |
| heat_implicit_hex8_kvar | mesh | t2 | 5 | 1 | 4.72 ms [4.71 ms, 4.84 ms] | 4.72 ms [4.71 ms, 4.84 ms] | 2730.3 [2726.3, 2803.2] | 1.014 [0.970, 1.045] | same |
| heat_implicit_hex8_kvar | mesh | t4 | 5 | 1 | 4.73 ms [4.71 ms, 4.85 ms] | 4.73 ms [4.71 ms, 4.85 ms] | 2736.1 [2725.1, 2803.8] | 1.018 [0.969, 1.021] | same |
| heat_implicit_hex8_kvar | mesh | t8 | 5 | 1 | 4.73 ms [4.69 ms, 4.87 ms] | 4.73 ms [4.69 ms, 4.87 ms] | 2736.7 [2714.7, 2817.1] | 1.012 [0.973, 1.048] | same |
| heat_implicit_hex8_kvar | init | base | 5 | 1 | 914.0 µs [873.0 µs, 1.21 ms] | 914.0 µs [873.0 µs, 1.21 ms] | 528.9 [505.2, 699.7] | – | same |
| heat_implicit_hex8_kvar | init | t1 | 5 | 1 | 845.0 µs [824.0 µs, 997.0 µs] | 845.0 µs [824.0 µs, 997.0 µs] | 489.0 [476.9, 577.0] | 0.906 [0.699, 1.091] | same |
| heat_implicit_hex8_kvar | init | t2 | 5 | 1 | 865.0 µs [821.0 µs, 979.0 µs] | 865.0 µs [821.0 µs, 979.0 µs] | 500.6 [475.1, 566.6] | 0.903 [0.694, 1.121] | same |
| heat_implicit_hex8_kvar | init | t4 | 5 | 1 | 905.0 µs [894.0 µs, 991.0 µs] | 905.0 µs [894.0 µs, 991.0 µs] | 523.7 [517.4, 573.5] | 1.016 [0.739, 1.090] | same |
| heat_implicit_hex8_kvar | init | t8 | 5 | 1 | 888.0 µs [834.0 µs, 1.03 ms] | 888.0 µs [834.0 µs, 1.03 ms] | 513.9 [482.6, 596.6] | 0.944 [0.853, 1.024] | same |
| heat_implicit_hex8_kvar | solve_step | base | 5 | 5 | 583.39 ms [574.66 ms, 590.08 ms] | 116.68 ms [114.93 ms, 118.02 ms] | 67521.5 [66512.2, 68296.4] | – | same |
| heat_implicit_hex8_kvar | solve_step | t1 | 5 | 5 | 577.71 ms [572.87 ms, 581.93 ms] | 115.54 ms [114.57 ms, 116.39 ms] | 66864.2 [66303.9, 67352.7] | 0.982 [0.979, 1.008] | same |
| heat_implicit_hex8_kvar | solve_step | t2 | 5 | 5 | 558.29 ms [554.44 ms, 562.59 ms] | 111.66 ms [110.89 ms, 112.52 ms] | 64616.6 [64171.9, 65114.2] | 0.957 [0.940, 0.972] | same |
| heat_implicit_hex8_kvar | solve_step | t4 | 5 | 5 | 550.03 ms [546.65 ms, 566.31 ms] | 110.01 ms [109.33 ms, 113.26 ms] | 63661.0 [63269.3, 65545.6] | 0.945 [0.926, 0.985] | same |
| heat_implicit_hex8_kvar | solve_step | t8 | 5 | 5 | 545.72 ms [537.66 ms, 550.66 ms] | 109.14 ms [107.53 ms, 110.13 ms] | 63162.3 [62229.5, 63733.4] | 0.935 [0.928, 0.948] | same |
| heat_static_tri6 | mesh | base | 5 | 1 | 51.28 ms [50.87 ms, 52.45 ms] | 51.28 ms [50.87 ms, 52.45 ms] | 2119.2 [2102.1, 2167.5] | – | same |
| heat_static_tri6 | mesh | t1 | 5 | 1 | 52.48 ms [51.60 ms, 52.69 ms] | 52.48 ms [51.60 ms, 52.69 ms] | 2168.5 [2132.4, 2177.1] | 1.023 [0.984, 1.030] | same |
| heat_static_tri6 | mesh | t2 | 5 | 1 | 52.16 ms [51.15 ms, 52.45 ms] | 52.16 ms [51.15 ms, 52.45 ms] | 2155.5 [2113.6, 2167.3] | 1.005 [0.994, 1.018] | same |
| heat_static_tri6 | mesh | t4 | 5 | 1 | 52.31 ms [51.38 ms, 52.98 ms] | 52.31 ms [51.38 ms, 52.98 ms] | 2161.5 [2123.3, 2189.5] | 1.010 [0.997, 1.035] | same |
| heat_static_tri6 | mesh | t8 | 5 | 1 | 52.45 ms [50.86 ms, 53.17 ms] | 52.45 ms [50.86 ms, 53.17 ms] | 2167.2 [2101.7, 2197.2] | 1.020 [0.994, 1.027] | same |
| heat_static_tri6 | init | base | 5 | 1 | 5.26 ms [5.17 ms, 5.28 ms] | 5.26 ms [5.17 ms, 5.28 ms] | 217.4 [213.5, 218.2] | – | same |
| heat_static_tri6 | init | t1 | 5 | 1 | 5.22 ms [4.75 ms, 5.50 ms] | 5.22 ms [4.75 ms, 5.50 ms] | 215.6 [196.2, 227.1] | 1.010 [0.902, 1.045] | same |
| heat_static_tri6 | init | t2 | 5 | 1 | 5.21 ms [5.18 ms, 5.30 ms] | 5.21 ms [5.18 ms, 5.30 ms] | 215.3 [214.0, 219.1] | 0.994 [0.987, 1.008] | same |
| heat_static_tri6 | init | t4 | 5 | 1 | 5.26 ms [5.24 ms, 5.48 ms] | 5.26 ms [5.24 ms, 5.48 ms] | 217.6 [216.6, 226.3] | 1.012 [0.993, 1.050] | same |
| heat_static_tri6 | init | t8 | 5 | 1 | 5.07 ms [5.04 ms, 5.09 ms] | 5.07 ms [5.04 ms, 5.09 ms] | 209.4 [208.1, 210.3] | 0.963 [0.960, 0.976] | same |
| heat_static_tri6 | assemble_conductivity | base | 5 | 5 | 46.20 ms [45.63 ms, 46.89 ms] | 9.24 ms [9.13 ms, 9.38 ms] | 381.8 [377.1, 387.5] | – | same |
| heat_static_tri6 | assemble_conductivity | t1 | 5 | 5 | 38.92 ms [36.01 ms, 39.47 ms] | 7.78 ms [7.20 ms, 7.89 ms] | 321.7 [297.6, 326.2] | 0.845 [0.768, 0.865] | same |
| heat_static_tri6 | assemble_conductivity | t2 | 5 | 5 | 21.16 ms [19.93 ms, 21.61 ms] | 4.23 ms [3.99 ms, 4.32 ms] | 174.9 [164.7, 178.6] | 0.455 [0.437, 0.468] | same |
| heat_static_tri6 | assemble_conductivity | t4 | 5 | 5 | 13.91 ms [13.84 ms, 14.03 ms] | 2.78 ms [2.77 ms, 2.81 ms] | 114.9 [114.3, 115.9] | 0.301 [0.295, 0.305] | same |
| heat_static_tri6 | assemble_conductivity | t8 | 5 | 5 | 9.78 ms [9.72 ms, 9.89 ms] | 1.96 ms [1.94 ms, 1.98 ms] | 80.9 [80.3, 81.7] | 0.212 [0.207, 0.214] | same |
| heat_static_tri6 | solve_step | base | 5 | 1 | 2.066 s [2.063 s, 2.072 s] | 2.066 s [2.063 s, 2.072 s] | 85360.8 [85227.4, 85608.6] | – | same |
| heat_static_tri6 | solve_step | t1 | 5 | 1 | 2.059 s [2.039 s, 2.077 s] | 2.059 s [2.039 s, 2.077 s] | 85091.8 [84263.7, 85836.9] | 0.994 [0.989, 1.004] | same |
| heat_static_tri6 | solve_step | t2 | 5 | 1 | 2.041 s [2.019 s, 2.048 s] | 2.041 s [2.019 s, 2.048 s] | 84332.5 [83431.2, 84623.8] | 0.990 [0.977, 0.991] | same |
| heat_static_tri6 | solve_step | t4 | 5 | 1 | 2.027 s [2.014 s, 2.068 s] | 2.027 s [2.014 s, 2.068 s] | 83744.8 [83237.3, 85470.9] | 0.982 [0.975, 1.000] | same |
| heat_static_tri6 | solve_step | t8 | 5 | 1 | 2.028 s [2.014 s, 2.037 s] | 2.028 s [2.014 s, 2.037 s] | 83782.0 [83238.4, 84193.7] | 0.982 [0.976, 0.983] | same |
| phasefield_staggered_tet4 | mesh | base | 5 | 1 | 8.87 ms [8.84 ms, 8.97 ms] | 8.87 ms [8.84 ms, 8.97 ms] | 1478.5 [1473.0, 1494.5] | – | same |
| phasefield_staggered_tet4 | mesh | t1 | 5 | 1 | 8.90 ms [8.90 ms, 9.59 ms] | 8.90 ms [8.90 ms, 9.59 ms] | 1483.8 [1482.8, 1598.7] | 1.006 [0.993, 1.081] | same |
| phasefield_staggered_tet4 | mesh | t2 | 5 | 1 | 8.80 ms [8.75 ms, 9.12 ms] | 8.80 ms [8.75 ms, 9.12 ms] | 1466.7 [1458.0, 1519.2] | 0.990 [0.981, 1.029] | same |
| phasefield_staggered_tet4 | mesh | t4 | 5 | 1 | 9.05 ms [9.05 ms, 9.13 ms] | 9.05 ms [9.05 ms, 9.13 ms] | 1509.2 [1507.8, 1521.7] | 1.021 [1.010, 1.024] | same |
| phasefield_staggered_tet4 | mesh | t8 | 5 | 1 | 9.07 ms [9.01 ms, 9.09 ms] | 9.07 ms [9.01 ms, 9.09 ms] | 1511.3 [1502.2, 1515.7] | 1.019 [1.007, 1.029] | same |
| phasefield_staggered_tet4 | init | base | 5 | 1 | 2.81 ms [2.76 ms, 2.85 ms] | 2.81 ms [2.76 ms, 2.85 ms] | 468.5 [460.5, 475.7] | – | same |
| phasefield_staggered_tet4 | init | t1 | 5 | 1 | 2.59 ms [2.57 ms, 2.88 ms] | 2.59 ms [2.57 ms, 2.88 ms] | 432.0 [428.3, 480.8] | 0.923 [0.904, 1.026] | same |
| phasefield_staggered_tet4 | init | t2 | 5 | 1 | 2.70 ms [2.62 ms, 2.82 ms] | 2.70 ms [2.62 ms, 2.82 ms] | 450.5 [436.0, 470.7] | 0.962 [0.947, 1.015] | same |
| phasefield_staggered_tet4 | init | t4 | 5 | 1 | 2.85 ms [2.83 ms, 3.06 ms] | 2.85 ms [2.83 ms, 3.06 ms] | 474.3 [471.8, 510.2] | 1.025 [0.992, 1.088] | same |
| phasefield_staggered_tet4 | init | t8 | 5 | 1 | 2.81 ms [2.80 ms, 2.86 ms] | 2.81 ms [2.80 ms, 2.86 ms] | 468.2 [467.3, 476.0] | 1.002 [0.999, 1.015] | same |
| phasefield_staggered_tet4 | solid_solve | base | 5 | 4 | 646.96 ms [644.60 ms, 656.50 ms] | 161.74 ms [161.15 ms, 164.12 ms] | 26956.8 [26858.2, 27354.2] | – | same |
| phasefield_staggered_tet4 | solid_solve | t1 | 5 | 4 | 621.41 ms [617.31 ms, 628.75 ms] | 155.35 ms [154.33 ms, 157.19 ms] | 25892.2 [25721.2, 26198.1] | 0.957 [0.950, 0.975] | same |
| phasefield_staggered_tet4 | solid_solve | t2 | 5 | 4 | 599.61 ms [584.73 ms, 608.06 ms] | 149.90 ms [146.18 ms, 152.02 ms] | 24983.6 [24363.8, 25335.9] | 0.925 [0.904, 0.939] | same |
| phasefield_staggered_tet4 | solid_solve | t4 | 5 | 4 | 593.12 ms [591.87 ms, 605.33 ms] | 148.28 ms [147.97 ms, 151.33 ms] | 24713.2 [24661.0, 25222.0] | 0.918 [0.913, 0.922] | same |
| phasefield_staggered_tet4 | solid_solve | t8 | 5 | 4 | 589.48 ms [583.43 ms, 597.49 ms] | 147.37 ms [145.86 ms, 149.37 ms] | 24561.6 [24309.7, 24895.3] | 0.910 [0.905, 0.922] | same |
| phasefield_staggered_tet4 | strain_transfer | base | 5 | 4 | 1.16 ms [1.11 ms, 1.21 ms] | 289.0 µs [277.8 µs, 302.2 µs] | 48.2 [46.3, 50.4] | – | same |
| phasefield_staggered_tet4 | strain_transfer | t1 | 5 | 4 | 1.16 ms [1.04 ms, 1.18 ms] | 291.0 µs [261.2 µs, 294.5 µs] | 48.5 [43.5, 49.1] | 0.975 [0.926, 1.048] | same |
| phasefield_staggered_tet4 | strain_transfer | t2 | 5 | 4 | 1.12 ms [1.08 ms, 1.18 ms] | 280.2 µs [270.5 µs, 294.2 µs] | 46.7 [45.1, 49.0] | 0.974 [0.933, 1.018] | same |
| phasefield_staggered_tet4 | strain_transfer | t4 | 5 | 4 | 1.14 ms [1.09 ms, 1.16 ms] | 285.2 µs [272.5 µs, 290.8 µs] | 47.5 [45.4, 48.5] | 0.971 [0.943, 1.041] | same |
| phasefield_staggered_tet4 | strain_transfer | t8 | 5 | 4 | 1.12 ms [1.10 ms, 1.13 ms] | 281.0 µs [275.0 µs, 282.5 µs] | 46.8 [45.8, 47.1] | 0.955 [0.918, 1.017] | same |
| phasefield_staggered_tet4 | phase_solve | base | 5 | 4 | 72.61 ms [70.04 ms, 73.88 ms] | 18.15 ms [17.51 ms, 18.47 ms] | 3025.2 [2918.4, 3078.2] | – | same |
| phasefield_staggered_tet4 | phase_solve | t1 | 5 | 4 | 73.71 ms [73.03 ms, 75.65 ms] | 18.43 ms [18.26 ms, 18.91 ms] | 3071.0 [3042.9, 3152.1] | 1.028 [0.998, 1.054] | same |
| phasefield_staggered_tet4 | phase_solve | t2 | 5 | 4 | 71.24 ms [70.28 ms, 71.62 ms] | 17.81 ms [17.57 ms, 17.91 ms] | 2968.4 [2928.2, 2984.3] | 0.969 [0.968, 1.019] | same |
| phasefield_staggered_tet4 | phase_solve | t4 | 5 | 4 | 71.26 ms [70.50 ms, 71.39 ms] | 17.82 ms [17.63 ms, 17.85 ms] | 2969.3 [2937.6, 2974.5] | 0.983 [0.965, 1.007] | same |
| phasefield_staggered_tet4 | phase_solve | t8 | 5 | 4 | 69.69 ms [69.02 ms, 70.61 ms] | 17.42 ms [17.25 ms, 17.65 ms] | 2903.9 [2875.8, 2942.2] | 0.960 [0.945, 1.008] | same |
| phasefield_staggered_tet4 | damage_transfer | base | 5 | 4 | 1.58 ms [1.38 ms, 1.59 ms] | 395.5 µs [344.8 µs, 398.5 µs] | 65.9 [57.5, 66.4] | – | same |
| phasefield_staggered_tet4 | damage_transfer | t1 | 5 | 4 | 1.35 ms [1.26 ms, 1.37 ms] | 337.2 µs [314.8 µs, 342.0 µs] | 56.2 [52.5, 57.0] | 0.854 [0.790, 0.992] | same |
| phasefield_staggered_tet4 | damage_transfer | t2 | 5 | 4 | 1.40 ms [1.34 ms, 1.42 ms] | 349.0 µs [334.0 µs, 354.0 µs] | 58.2 [55.7, 59.0] | 0.878 [0.838, 1.013] | same |
| phasefield_staggered_tet4 | damage_transfer | t4 | 5 | 4 | 1.40 ms [1.34 ms, 1.48 ms] | 349.5 µs [334.2 µs, 369.8 µs] | 58.2 [55.7, 61.6] | 0.889 [0.839, 1.014] | same |
| phasefield_staggered_tet4 | damage_transfer | t8 | 5 | 4 | 1.41 ms [1.40 ms, 1.67 ms] | 353.0 µs [349.0 µs, 416.5 µs] | 58.8 [58.2, 69.4] | 0.903 [0.876, 1.053] | same |
| phasefield_staggered_tet4 | internal_forces | base | 5 | 4 | 2.65 ms [2.61 ms, 2.70 ms] | 662.5 µs [653.2 µs, 674.2 µs] | 110.4 [108.9, 112.4] | – | same |
| phasefield_staggered_tet4 | internal_forces | t1 | 5 | 4 | 2.58 ms [2.51 ms, 2.62 ms] | 645.0 µs [627.0 µs, 655.5 µs] | 107.5 [104.5, 109.2] | 0.975 [0.951, 0.989] | same |
| phasefield_staggered_tet4 | internal_forces | t2 | 5 | 4 | 1.86 ms [1.77 ms, 1.88 ms] | 465.0 µs [443.5 µs, 469.2 µs] | 77.5 [73.9, 78.2] | 0.697 [0.672, 0.718] | same |
| phasefield_staggered_tet4 | internal_forces | t4 | 5 | 4 | 1.77 ms [1.74 ms, 1.78 ms] | 443.2 µs [435.5 µs, 445.8 µs] | 73.9 [72.6, 74.3] | 0.660 [0.658, 0.679] | same |
| phasefield_staggered_tet4 | internal_forces | t8 | 5 | 4 | 1.71 ms [1.66 ms, 1.75 ms] | 427.0 µs [414.5 µs, 436.8 µs] | 71.2 [69.1, 72.8] | 0.644 [0.629, 0.662] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base | 5 | 5 | 48.33 ms [46.54 ms, 48.75 ms] | 9.67 ms [9.31 ms, 9.75 ms] | 1611.0 [1551.4, 1624.9] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t1 | 5 | 5 | 24.28 ms [24.04 ms, 25.51 ms] | 4.86 ms [4.81 ms, 5.10 ms] | 809.4 [801.5, 850.3] | 0.516 [0.502, 0.526] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t2 | 5 | 5 | 14.13 ms [14.02 ms, 14.57 ms] | 2.83 ms [2.80 ms, 2.91 ms] | 471.0 [467.2, 485.5] | 0.300 [0.288, 0.307] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t4 | 5 | 5 | 9.77 ms [9.51 ms, 9.85 ms] | 1.95 ms [1.90 ms, 1.97 ms] | 325.7 [316.9, 328.3] | 0.203 [0.197, 0.210] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t8 | 5 | 5 | 6.47 ms [6.36 ms, 6.73 ms] | 1.29 ms [1.27 ms, 1.35 ms] | 215.5 [211.9, 224.4] | 0.137 [0.131, 0.141] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base | 5 | 5 | 8.94 ms [7.81 ms, 9.00 ms] | 1.79 ms [1.56 ms, 1.80 ms] | 298.1 [260.3, 299.9] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t1 | 5 | 5 | 8.29 ms [7.56 ms, 8.46 ms] | 1.66 ms [1.51 ms, 1.69 ms] | 276.4 [251.9, 281.9] | 0.940 [0.843, 1.062] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t2 | 5 | 5 | 6.53 ms [6.05 ms, 6.82 ms] | 1.31 ms [1.21 ms, 1.36 ms] | 217.8 [201.8, 227.3] | 0.738 [0.725, 0.775] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t4 | 5 | 5 | 6.06 ms [5.39 ms, 6.09 ms] | 1.21 ms [1.08 ms, 1.22 ms] | 202.0 [179.6, 203.0] | 0.679 [0.676, 0.720] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t8 | 5 | 5 | 5.81 ms [5.74 ms, 6.12 ms] | 1.16 ms [1.15 ms, 1.22 ms] | 193.8 [191.3, 204.1] | 0.649 [0.638, 0.784] | same |
| phasefield_staggered_tri3 | mesh | base | 5 | 1 | 58.06 ms [57.59 ms, 58.69 ms] | 58.06 ms [57.59 ms, 58.69 ms] | 1134.0 [1124.8, 1146.2] | – | same |
| phasefield_staggered_tri3 | mesh | t1 | 5 | 1 | 57.91 ms [57.04 ms, 59.48 ms] | 57.91 ms [57.04 ms, 59.48 ms] | 1131.0 [1114.1, 1161.7] | 0.999 [0.982, 1.029] | same |
| phasefield_staggered_tri3 | mesh | t2 | 5 | 1 | 58.96 ms [54.73 ms, 59.42 ms] | 58.96 ms [54.73 ms, 59.42 ms] | 1151.7 [1068.9, 1160.6] | 1.005 [0.947, 1.032] | same |
| phasefield_staggered_tri3 | mesh | t4 | 5 | 1 | 58.69 ms [57.69 ms, 59.06 ms] | 58.69 ms [57.69 ms, 59.06 ms] | 1146.2 [1126.8, 1153.6] | 1.004 [0.998, 1.026] | same |
| phasefield_staggered_tri3 | mesh | t8 | 5 | 1 | 59.07 ms [58.40 ms, 59.80 ms] | 59.07 ms [58.40 ms, 59.80 ms] | 1153.8 [1140.6, 1167.9] | 1.017 [1.004, 1.027] | same |
| phasefield_staggered_tri3 | init | base | 5 | 1 | 17.87 ms [17.27 ms, 17.98 ms] | 17.87 ms [17.27 ms, 17.98 ms] | 349.1 [337.2, 351.1] | – | same |
| phasefield_staggered_tri3 | init | t1 | 5 | 1 | 18.02 ms [16.70 ms, 18.16 ms] | 18.02 ms [16.70 ms, 18.16 ms] | 352.0 [326.1, 354.8] | 1.003 [0.930, 1.052] | same |
| phasefield_staggered_tri3 | init | t2 | 5 | 1 | 16.64 ms [16.11 ms, 18.01 ms] | 16.64 ms [16.11 ms, 18.01 ms] | 324.9 [314.6, 351.8] | 0.938 [0.901, 1.002] | same |
| phasefield_staggered_tri3 | init | t4 | 5 | 1 | 17.89 ms [17.77 ms, 18.14 ms] | 17.89 ms [17.77 ms, 18.14 ms] | 349.5 [347.2, 354.3] | 1.002 [0.995, 1.051] | same |
| phasefield_staggered_tri3 | init | t8 | 5 | 1 | 17.89 ms [17.81 ms, 17.95 ms] | 17.89 ms [17.81 ms, 17.95 ms] | 349.3 [347.9, 350.7] | 1.005 [0.991, 1.034] | same |
| phasefield_staggered_tri3 | solid_solve | base | 5 | 4 | 3.503 s [3.488 s, 3.521 s] | 875.66 ms [871.88 ms, 880.25 ms] | 17102.7 [17029.0, 17192.3] | – | same |
| phasefield_staggered_tri3 | solid_solve | t1 | 5 | 4 | 3.458 s [3.398 s, 3.478 s] | 864.47 ms [849.38 ms, 869.39 ms] | 16884.3 [16589.4, 16980.3] | 0.987 [0.970, 0.992] | same |
| phasefield_staggered_tri3 | solid_solve | t2 | 5 | 4 | 3.391 s [3.342 s, 3.416 s] | 847.64 ms [835.47 ms, 854.08 ms] | 16555.5 [16317.7, 16681.3] | 0.969 [0.954, 0.971] | same |
| phasefield_staggered_tri3 | solid_solve | t4 | 5 | 4 | 3.376 s [3.300 s, 3.405 s] | 844.12 ms [824.89 ms, 851.18 ms] | 16486.7 [16111.2, 16624.5] | 0.961 [0.942, 0.970] | same |
| phasefield_staggered_tri3 | solid_solve | t8 | 5 | 4 | 3.377 s [3.365 s, 3.383 s] | 844.18 ms [841.28 ms, 845.70 ms] | 16487.8 [16431.3, 16517.5] | 0.963 [0.959, 0.970] | same |
| phasefield_staggered_tri3 | strain_transfer | base | 5 | 4 | 4.44 ms [4.33 ms, 4.48 ms] | 1.11 ms [1.08 ms, 1.12 ms] | 21.7 [21.1, 21.9] | – | same |
| phasefield_staggered_tri3 | strain_transfer | t1 | 5 | 4 | 4.58 ms [4.48 ms, 4.76 ms] | 1.15 ms [1.12 ms, 1.19 ms] | 22.4 [21.9, 23.2] | 1.035 [1.015, 1.095] | same |
| phasefield_staggered_tri3 | strain_transfer | t2 | 5 | 4 | 4.82 ms [4.75 ms, 4.93 ms] | 1.21 ms [1.19 ms, 1.23 ms] | 23.6 [23.2, 24.1] | 1.103 [1.059, 1.117] | same |
| phasefield_staggered_tri3 | strain_transfer | t4 | 5 | 4 | 4.77 ms [4.75 ms, 4.86 ms] | 1.19 ms [1.19 ms, 1.21 ms] | 23.3 [23.2, 23.7] | 1.093 [1.063, 1.122] | same |
| phasefield_staggered_tri3 | strain_transfer | t8 | 5 | 4 | 4.78 ms [4.77 ms, 4.84 ms] | 1.20 ms [1.19 ms, 1.21 ms] | 23.4 [23.3, 23.6] | 1.078 [1.069, 1.119] | same |
| phasefield_staggered_tri3 | phase_solve | base | 5 | 4 | 912.66 ms [902.48 ms, 916.16 ms] | 228.17 ms [225.62 ms, 229.04 ms] | 4456.4 [4406.6, 4473.4] | – | same |
| phasefield_staggered_tri3 | phase_solve | t1 | 5 | 4 | 895.45 ms [872.99 ms, 904.12 ms] | 223.86 ms [218.25 ms, 226.03 ms] | 4372.3 [4262.6, 4414.7] | 0.983 [0.957, 1.002] | same |
| phasefield_staggered_tri3 | phase_solve | t2 | 5 | 4 | 867.98 ms [836.32 ms, 878.67 ms] | 216.99 ms [209.08 ms, 219.67 ms] | 4238.2 [4083.6, 4290.4] | 0.955 [0.916, 0.961] | same |
| phasefield_staggered_tri3 | phase_solve | t4 | 5 | 4 | 856.08 ms [843.79 ms, 867.87 ms] | 214.02 ms [210.95 ms, 216.97 ms] | 4180.1 [4120.1, 4237.7] | 0.941 [0.925, 0.960] | same |
| phasefield_staggered_tri3 | phase_solve | t8 | 5 | 4 | 858.14 ms [850.39 ms, 861.02 ms] | 214.53 ms [212.60 ms, 215.25 ms] | 4190.1 [4152.3, 4204.2] | 0.940 [0.940, 0.942] | same |
| phasefield_staggered_tri3 | damage_transfer | base | 5 | 4 | 10.53 ms [10.06 ms, 10.68 ms] | 2.63 ms [2.52 ms, 2.67 ms] | 51.4 [49.1, 52.1] | – | same |
| phasefield_staggered_tri3 | damage_transfer | t1 | 5 | 4 | 10.21 ms [10.05 ms, 10.55 ms] | 2.55 ms [2.51 ms, 2.64 ms] | 49.8 [49.1, 51.5] | 0.988 [0.951, 1.014] | same |
| phasefield_staggered_tri3 | damage_transfer | t2 | 5 | 4 | 10.38 ms [10.10 ms, 10.76 ms] | 2.59 ms [2.53 ms, 2.69 ms] | 50.7 [49.3, 52.5] | 1.004 [0.955, 1.031] | same |
| phasefield_staggered_tri3 | damage_transfer | t4 | 5 | 4 | 10.61 ms [10.21 ms, 10.83 ms] | 2.65 ms [2.55 ms, 2.71 ms] | 51.8 [49.9, 52.9] | 1.012 [0.965, 1.053] | same |
| phasefield_staggered_tri3 | damage_transfer | t8 | 5 | 4 | 10.76 ms [10.57 ms, 10.84 ms] | 2.69 ms [2.64 ms, 2.71 ms] | 52.5 [51.6, 52.9] | 1.028 [0.990, 1.069] | same |
| phasefield_staggered_tri3 | internal_forces | base | 5 | 4 | 14.95 ms [14.78 ms, 15.17 ms] | 3.74 ms [3.70 ms, 3.79 ms] | 73.0 [72.2, 74.1] | – | same |
| phasefield_staggered_tri3 | internal_forces | t1 | 5 | 4 | 14.97 ms [14.57 ms, 15.10 ms] | 3.74 ms [3.64 ms, 3.77 ms] | 73.1 [71.2, 73.7] | 1.009 [0.961, 1.010] | same |
| phasefield_staggered_tri3 | internal_forces | t2 | 5 | 4 | 9.76 ms [9.71 ms, 9.83 ms] | 2.44 ms [2.43 ms, 2.46 ms] | 47.7 [47.4, 48.0] | 0.654 [0.647, 0.660] | same |
| phasefield_staggered_tri3 | internal_forces | t4 | 5 | 4 | 7.47 ms [7.38 ms, 7.55 ms] | 1.87 ms [1.84 ms, 1.89 ms] | 36.5 [36.0, 36.9] | 0.498 [0.494, 0.505] | same |
| phasefield_staggered_tri3 | internal_forces | t8 | 5 | 4 | 6.30 ms [6.27 ms, 6.46 ms] | 1.58 ms [1.57 ms, 1.62 ms] | 30.8 [30.6, 31.5] | 0.422 [0.413, 0.437] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base | 5 | 5 | 74.02 ms [71.19 ms, 74.52 ms] | 14.80 ms [14.24 ms, 14.90 ms] | 289.1 [278.1, 291.1] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t1 | 5 | 5 | 61.84 ms [58.61 ms, 65.95 ms] | 12.37 ms [11.72 ms, 13.19 ms] | 241.6 [228.9, 257.6] | 0.836 [0.791, 0.926] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t2 | 5 | 5 | 33.88 ms [31.36 ms, 34.72 ms] | 6.78 ms [6.27 ms, 6.94 ms] | 132.3 [122.5, 135.6] | 0.463 [0.423, 0.485] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t4 | 5 | 5 | 22.18 ms [22.09 ms, 23.22 ms] | 4.44 ms [4.42 ms, 4.64 ms] | 86.6 [86.3, 90.7] | 0.304 [0.298, 0.312] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t8 | 5 | 5 | 17.60 ms [17.36 ms, 18.39 ms] | 3.52 ms [3.47 ms, 3.68 ms] | 68.7 [67.8, 71.8] | 0.238 [0.235, 0.252] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base | 5 | 5 | 62.33 ms [59.63 ms, 62.76 ms] | 12.47 ms [11.93 ms, 12.55 ms] | 243.5 [232.9, 245.2] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t1 | 5 | 5 | 57.99 ms [54.84 ms, 59.06 ms] | 11.60 ms [10.97 ms, 11.81 ms] | 226.5 [214.2, 230.7] | 0.947 [0.874, 0.981] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t2 | 5 | 5 | 42.37 ms [41.26 ms, 42.73 ms] | 8.47 ms [8.25 ms, 8.54 ms] | 165.5 [161.2, 166.9] | 0.680 [0.657, 0.711] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t4 | 5 | 5 | 38.79 ms [38.13 ms, 39.00 ms] | 7.76 ms [7.63 ms, 7.80 ms] | 151.5 [148.9, 152.4] | 0.622 [0.610, 0.644] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t8 | 5 | 5 | 37.39 ms [37.07 ms, 37.67 ms] | 7.48 ms [7.42 ms, 7.54 ms] | 146.1 [144.8, 147.2] | 0.600 [0.595, 0.628] | same |
| structural_beam3_frame_dynamic | mesh | base | 5 | 1 | 248.0 µs [242.0 µs, 303.0 µs] | 248.0 µs [242.0 µs, 303.0 µs] | 40.8 [39.8, 49.8] | – | same |
| structural_beam3_frame_dynamic | mesh | t1 | 5 | 1 | 290.0 µs [281.0 µs, 314.0 µs] | 290.0 µs [281.0 µs, 314.0 µs] | 47.7 [46.2, 51.6] | 1.142 [1.036, 1.260] | same |
| structural_beam3_frame_dynamic | mesh | t2 | 5 | 1 | 290.0 µs [268.0 µs, 311.0 µs] | 290.0 µs [268.0 µs, 311.0 µs] | 47.7 [44.0, 51.1] | 1.174 [0.884, 1.285] | same |
| structural_beam3_frame_dynamic | mesh | t4 | 5 | 1 | 291.0 µs [287.0 µs, 307.0 µs] | 291.0 µs [287.0 µs, 307.0 µs] | 47.8 [47.2, 50.5] | 1.170 [0.977, 1.269] | same |
| structural_beam3_frame_dynamic | mesh | t8 | 5 | 1 | 297.0 µs [289.0 µs, 326.0 µs] | 297.0 µs [289.0 µs, 326.0 µs] | 48.8 [47.5, 53.6] | 1.220 [0.967, 1.315] | same |
| structural_beam3_frame_dynamic | init | base | 5 | 1 | 28.00 ms [27.46 ms, 28.54 ms] | 28.00 ms [27.46 ms, 28.54 ms] | 4602.2 [4513.5, 4690.3] | – | same |
| structural_beam3_frame_dynamic | init | t1 | 5 | 1 | 27.37 ms [26.92 ms, 27.73 ms] | 27.37 ms [26.92 ms, 27.73 ms] | 4498.4 [4423.9, 4558.3] | 0.990 [0.953, 1.008] | same |
| structural_beam3_frame_dynamic | init | t2 | 5 | 1 | 27.77 ms [27.12 ms, 28.78 ms] | 27.77 ms [27.12 ms, 28.78 ms] | 4565.1 [4458.3, 4730.9] | 1.009 [0.961, 1.021] | same |
| structural_beam3_frame_dynamic | init | t4 | 5 | 1 | 27.73 ms [27.12 ms, 27.85 ms] | 27.73 ms [27.12 ms, 27.85 ms] | 4557.7 [4458.4, 4577.6] | 0.988 [0.962, 1.009] | same |
| structural_beam3_frame_dynamic | init | t8 | 5 | 1 | 27.62 ms [27.06 ms, 27.88 ms] | 27.62 ms [27.06 ms, 27.88 ms] | 4539.8 [4448.6, 4582.0] | 0.986 [0.948, 1.004] | same |
| structural_beam3_frame_dynamic | solve_step | base | 5 | 60 | 3.740 s [3.721 s, 3.765 s] | 62.34 ms [62.02 ms, 62.74 ms] | 10246.7 [10193.3, 10313.1] | – | same |
| structural_beam3_frame_dynamic | solve_step | t1 | 5 | 60 | 3.720 s [3.683 s, 3.753 s] | 62.00 ms [61.39 ms, 62.55 ms] | 10191.0 [10089.8, 10281.1] | 0.992 [0.989, 1.006] | same |
| structural_beam3_frame_dynamic | solve_step | t2 | 5 | 60 | 3.712 s [3.675 s, 3.725 s] | 61.86 ms [61.25 ms, 62.08 ms] | 10168.1 [10067.8, 10204.1] | 0.990 [0.988, 0.998] | same |
| structural_beam3_frame_dynamic | solve_step | t4 | 5 | 60 | 3.704 s [3.641 s, 3.725 s] | 61.73 ms [60.69 ms, 62.09 ms] | 10147.0 [9974.6, 10205.0] | 0.988 [0.979, 0.998] | same |
| structural_beam3_frame_dynamic | solve_step | t8 | 5 | 60 | 3.707 s [3.682 s, 3.715 s] | 61.79 ms [61.36 ms, 61.92 ms] | 10155.7 [10085.8, 10177.0] | 0.989 [0.985, 0.996] | same |
| structural_beam3_frame_static | mesh | base | 5 | 1 | 283.0 µs [262.0 µs, 290.0 µs] | 283.0 µs [262.0 µs, 290.0 µs] | 46.5 [43.1, 47.7] | – | same |
| structural_beam3_frame_static | mesh | t1 | 5 | 1 | 309.0 µs [290.0 µs, 310.0 µs] | 309.0 µs [290.0 µs, 310.0 µs] | 50.8 [47.7, 51.0] | 1.077 [1.000, 1.179] | same |
| structural_beam3_frame_static | mesh | t2 | 5 | 1 | 306.0 µs [281.0 µs, 321.0 µs] | 306.0 µs [281.0 µs, 321.0 µs] | 50.3 [46.2, 52.8] | 1.094 [0.969, 1.134] | same |
| structural_beam3_frame_static | mesh | t4 | 5 | 1 | 287.0 µs [250.0 µs, 291.0 µs] | 287.0 µs [250.0 µs, 291.0 µs] | 47.2 [41.1, 47.8] | 1.014 [0.883, 1.092] | same |
| structural_beam3_frame_static | mesh | t8 | 5 | 1 | 290.0 µs [276.0 µs, 306.0 µs] | 290.0 µs [276.0 µs, 306.0 µs] | 47.7 [45.4, 50.3] | 1.025 [0.979, 1.168] | same |
| structural_beam3_frame_static | init | base | 5 | 1 | 27.28 ms [26.70 ms, 27.91 ms] | 27.28 ms [26.70 ms, 27.91 ms] | 4483.2 [4388.9, 4587.4] | – | same |
| structural_beam3_frame_static | init | t1 | 5 | 1 | 27.20 ms [26.90 ms, 27.77 ms] | 27.20 ms [26.90 ms, 27.77 ms] | 4469.9 [4421.8, 4564.9] | 0.999 [0.966, 1.040] | same |
| structural_beam3_frame_static | init | t2 | 5 | 1 | 27.52 ms [26.66 ms, 27.89 ms] | 27.52 ms [26.66 ms, 27.89 ms] | 4523.0 [4381.2, 4583.5] | 1.002 [0.955, 1.031] | same |
| structural_beam3_frame_static | init | t4 | 5 | 1 | 27.40 ms [26.14 ms, 27.71 ms] | 27.40 ms [26.14 ms, 27.71 ms] | 4504.1 [4296.0, 4555.4] | 1.000 [0.938, 1.031] | same |
| structural_beam3_frame_static | init | t8 | 5 | 1 | 27.16 ms [26.36 ms, 27.29 ms] | 27.16 ms [26.36 ms, 27.29 ms] | 4463.7 [4333.5, 4484.9] | 0.973 [0.969, 1.022] | same |
| structural_beam3_frame_static | assemble_stiffness | base | 5 | 5 | 35.02 ms [34.23 ms, 35.88 ms] | 7.00 ms [6.85 ms, 7.18 ms] | 1151.2 [1125.3, 1179.7] | – | same |
| structural_beam3_frame_static | assemble_stiffness | t1 | 5 | 5 | 33.63 ms [32.01 ms, 34.01 ms] | 6.73 ms [6.40 ms, 6.80 ms] | 1105.6 [1052.3, 1117.9] | 0.960 [0.905, 0.980] | same |
| structural_beam3_frame_static | assemble_stiffness | t2 | 5 | 5 | 32.81 ms [32.04 ms, 33.39 ms] | 6.56 ms [6.41 ms, 6.68 ms] | 1078.6 [1053.4, 1097.7] | 0.935 [0.893, 0.975] | same |
| structural_beam3_frame_static | assemble_stiffness | t4 | 5 | 5 | 32.72 ms [32.16 ms, 33.34 ms] | 6.54 ms [6.43 ms, 6.67 ms] | 1075.6 [1057.0, 1095.9] | 0.947 [0.896, 0.959] | same |
| structural_beam3_frame_static | assemble_stiffness | t8 | 5 | 5 | 32.73 ms [32.53 ms, 33.22 ms] | 6.55 ms [6.51 ms, 6.64 ms] | 1075.9 [1069.4, 1092.0] | 0.937 [0.919, 0.953] | same |
| structural_beam3_frame_static | solve_step | base | 5 | 1 | 1.233 s [1.218 s, 1.235 s] | 1.233 s [1.218 s, 1.235 s] | 202631.3 [200139.1, 203057.7] | – | same |
| structural_beam3_frame_static | solve_step | t1 | 5 | 1 | 1.234 s [1.225 s, 1.238 s] | 1.234 s [1.225 s, 1.238 s] | 202799.3 [201304.6, 203508.5] | 1.001 [0.993, 1.011] | same |
| structural_beam3_frame_static | solve_step | t2 | 5 | 1 | 1.237 s [1.218 s, 1.252 s] | 1.237 s [1.218 s, 1.252 s] | 203251.0 [200278.1, 205704.6] | 1.007 [0.986, 1.022] | same |
| structural_beam3_frame_static | solve_step | t4 | 5 | 1 | 1.233 s [1.218 s, 1.240 s] | 1.233 s [1.218 s, 1.240 s] | 202663.5 [200138.9, 203883.1] | 1.006 [0.986, 1.013] | same |
| structural_beam3_frame_static | solve_step | t8 | 5 | 1 | 1.235 s [1.228 s, 1.239 s] | 1.235 s [1.228 s, 1.239 s] | 202949.2 [201890.2, 203653.2] | 1.003 [0.996, 1.014] | same |
| structural_dkt18_plate_static | mesh | base | 5 | 1 | 242.0 µs [229.0 µs, 250.0 µs] | 242.0 µs [229.0 µs, 250.0 µs] | 33.6 [31.8, 34.7] | – | same |
| structural_dkt18_plate_static | mesh | t1 | 5 | 1 | 263.0 µs [260.0 µs, 282.0 µs] | 263.0 µs [260.0 µs, 282.0 µs] | 36.5 [36.1, 39.2] | 1.124 [1.061, 1.165] | same |
| structural_dkt18_plate_static | mesh | t2 | 5 | 1 | 280.0 µs [259.0 µs, 289.0 µs] | 280.0 µs [259.0 µs, 289.0 µs] | 38.9 [36.0, 40.1] | 1.143 [1.107, 1.194] | same |
| structural_dkt18_plate_static | mesh | t4 | 5 | 1 | 238.0 µs [232.0 µs, 266.0 µs] | 238.0 µs [232.0 µs, 266.0 µs] | 33.1 [32.2, 36.9] | 1.034 [0.928, 1.099] | same |
| structural_dkt18_plate_static | mesh | t8 | 5 | 1 | 237.0 µs [233.0 µs, 264.0 µs] | 237.0 µs [233.0 µs, 264.0 µs] | 32.9 [32.4, 36.7] | 1.026 [0.979, 1.078] | same |
| structural_dkt18_plate_static | init | base | 5 | 1 | 65.72 ms [65.50 ms, 67.88 ms] | 65.72 ms [65.50 ms, 67.88 ms] | 9128.1 [9096.5, 9427.1] | – | same |
| structural_dkt18_plate_static | init | t1 | 5 | 1 | 67.26 ms [65.46 ms, 68.74 ms] | 67.26 ms [65.46 ms, 68.74 ms] | 9341.7 [9092.2, 9546.7] | 1.013 [0.991, 1.049] | same |
| structural_dkt18_plate_static | init | t2 | 5 | 1 | 66.84 ms [64.70 ms, 67.81 ms] | 66.84 ms [64.70 ms, 67.81 ms] | 9283.6 [8986.4, 9418.1] | 1.014 [0.953, 1.032] | same |
| structural_dkt18_plate_static | init | t4 | 5 | 1 | 66.91 ms [64.11 ms, 66.98 ms] | 66.91 ms [64.11 ms, 66.98 ms] | 9292.4 [8903.8, 9302.6] | 1.018 [0.944, 1.023] | same |
| structural_dkt18_plate_static | init | t8 | 5 | 1 | 66.75 ms [65.64 ms, 68.45 ms] | 66.75 ms [65.64 ms, 68.45 ms] | 9270.7 [9116.2, 9506.8] | 1.012 [0.983, 1.045] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 5 | 5 | 307.97 ms [290.47 ms, 315.32 ms] | 61.59 ms [58.09 ms, 63.06 ms] | 8554.6 [8068.7, 8759.0] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | t1 | 5 | 5 | 295.07 ms [273.12 ms, 298.97 ms] | 59.01 ms [54.62 ms, 59.79 ms] | 8196.3 [7586.7, 8304.7] | 0.948 [0.937, 0.962] | same |
| structural_dkt18_plate_static | assemble_stiffness | t2 | 5 | 5 | 291.47 ms [286.62 ms, 294.60 ms] | 58.29 ms [57.32 ms, 58.92 ms] | 8096.4 [7961.6, 8183.3] | 0.947 [0.921, 0.987] | same |
| structural_dkt18_plate_static | assemble_stiffness | t4 | 5 | 5 | 284.56 ms [279.16 ms, 293.58 ms] | 56.91 ms [55.83 ms, 58.72 ms] | 7904.4 [7754.5, 8155.1] | 0.935 [0.902, 0.961] | same |
| structural_dkt18_plate_static | assemble_stiffness | t8 | 5 | 5 | 286.07 ms [266.31 ms, 290.30 ms] | 57.21 ms [53.26 ms, 58.06 ms] | 7946.4 [7397.6, 8063.9] | 0.920 [0.910, 0.943] | same |
| structural_dkt18_plate_static | solve_step | base | 5 | 1 | 974.59 ms [960.80 ms, 981.33 ms] | 974.59 ms [960.80 ms, 981.33 ms] | 135360.0 [133444.2, 136295.1] | – | same |
| structural_dkt18_plate_static | solve_step | t1 | 5 | 1 | 979.62 ms [966.61 ms, 1.001 s] | 979.62 ms [966.61 ms, 1.001 s] | 136058.1 [134251.7, 139024.4] | 1.008 [0.992, 1.022] | same |
| structural_dkt18_plate_static | solve_step | t2 | 5 | 1 | 971.53 ms [959.18 ms, 982.01 ms] | 971.53 ms [959.18 ms, 982.01 ms] | 134935.1 [133219.0, 136390.6] | 1.002 [0.984, 1.005] | same |
| structural_dkt18_plate_static | solve_step | t4 | 5 | 1 | 980.14 ms [966.14 ms, 987.74 ms] | 980.14 ms [966.14 ms, 987.74 ms] | 136130.0 [134185.7, 137186.1] | 1.006 [1.001, 1.008] | same |
| structural_dkt18_plate_static | solve_step | t8 | 5 | 1 | 975.35 ms [962.91 ms, 982.65 ms] | 975.35 ms [962.91 ms, 982.65 ms] | 135465.3 [133738.2, 136479.6] | 1.003 [0.988, 1.009] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 72337cb4e, ea071de35 | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 72337cb4e, ea071de35 | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 72337cb4e, ea071de35 | 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 72337cb4e, ea071de35 | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 72337cb4e, ea071de35 | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 72337cb4e, ea071de35 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 72337cb4e, ea071de35 | a77809465b3e7c18 |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | 72337cb4e, ea071de35 | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 72337cb4e, ea071de35 | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 72337cb4e, ea071de35 | 1cfc7f010c52d6b3 |
