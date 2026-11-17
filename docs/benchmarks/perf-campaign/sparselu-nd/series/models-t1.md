Suite `models`, size `m`; variants: `auto`, `base`, `colamd`, `nested_dissection`; reference `colamd`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to colamd | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | mesh | auto | 5 | 1 | 8.84 ms [8.82 ms, 8.91 ms] | 8.84 ms [8.82 ms, 8.91 ms] | 1262.7 [1260.7, 1273.6] | 1.001 [0.991, 1.004] | DIFFER (2) |
| contact_implicit_static_quad4 | mesh | base | 5 | 1 | 8.89 ms [8.82 ms, 8.90 ms] | 8.89 ms [8.82 ms, 8.90 ms] | 1269.7 [1260.7, 1271.7] | 0.998 [0.994, 1.007] | DIFFER (2) |
| contact_implicit_static_quad4 | mesh | colamd | 5 | 1 | 8.88 ms [8.83 ms, 8.91 ms] | 8.88 ms [8.83 ms, 8.91 ms] | 1268.1 [1261.0, 1272.3] | – | DIFFER (2) |
| contact_implicit_static_quad4 | mesh | nested_dissection | 5 | 1 | 8.82 ms [8.80 ms, 8.92 ms] | 8.82 ms [8.80 ms, 8.92 ms] | 1260.0 [1257.1, 1274.6] | 0.996 [0.990, 1.009] | DIFFER (2) |
| contact_implicit_static_quad4 | init | auto | 5 | 1 | 10.57 ms [10.53 ms, 10.63 ms] | 10.57 ms [10.53 ms, 10.63 ms] | 1510.4 [1504.1, 1518.0] | 1.000 [0.995, 1.008] | DIFFER (2) |
| contact_implicit_static_quad4 | init | base | 5 | 1 | 10.47 ms [10.07 ms, 10.61 ms] | 10.47 ms [10.07 ms, 10.61 ms] | 1496.4 [1438.0, 1515.0] | 0.991 [0.955, 1.002] | DIFFER (2) |
| contact_implicit_static_quad4 | init | colamd | 5 | 1 | 10.57 ms [10.54 ms, 10.58 ms] | 10.57 ms [10.54 ms, 10.58 ms] | 1510.6 [1505.3, 1511.7] | – | DIFFER (2) |
| contact_implicit_static_quad4 | init | nested_dissection | 5 | 1 | 10.54 ms [10.48 ms, 10.59 ms] | 10.54 ms [10.48 ms, 10.59 ms] | 1505.4 [1497.7, 1513.4] | 0.997 [0.993, 1.005] | DIFFER (2) |
| contact_implicit_static_quad4 | solve_step | auto | 5 | 5 | 758.42 ms [755.17 ms, 765.21 ms] | 151.68 ms [151.03 ms, 153.04 ms] | 21669.3 [21576.2, 21863.1] | 1.003 [0.997, 1.010] | DIFFER (2) |
| contact_implicit_static_quad4 | solve_step | base | 5 | 5 | 750.15 ms [746.83 ms, 758.61 ms] | 150.03 ms [149.37 ms, 151.72 ms] | 21432.8 [21338.1, 21674.6] | 0.992 [0.988, 1.001] | DIFFER (2) |
| contact_implicit_static_quad4 | solve_step | colamd | 5 | 5 | 757.84 ms [755.77 ms, 758.43 ms] | 151.57 ms [151.15 ms, 151.69 ms] | 21652.7 [21593.5, 21669.4] | – | DIFFER (2) |
| contact_implicit_static_quad4 | solve_step | nested_dissection | 5 | 5 | 681.00 ms [678.95 ms, 685.18 ms] | 136.20 ms [135.79 ms, 137.04 ms] | 19457.1 [19398.5, 19576.5] | 0.898 [0.897, 0.906] | DIFFER (2) |
| embedded_rebar_tet4 | mesh | auto | 5 | 1 | 21.66 ms [21.48 ms, 21.85 ms] | 21.66 ms [21.48 ms, 21.85 ms] | 995.6 [987.5, 1004.5] | 1.000 [0.996, 1.007] | DIFFER (2) |
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 21.68 ms [21.62 ms, 21.75 ms] | 21.68 ms [21.62 ms, 21.75 ms] | 996.8 [993.9, 1000.2] | 1.001 [0.994, 1.011] | DIFFER (2) |
| embedded_rebar_tet4 | mesh | colamd | 5 | 1 | 21.61 ms [21.51 ms, 21.75 ms] | 21.61 ms [21.51 ms, 21.75 ms] | 993.7 [988.9, 1000.2] | – | DIFFER (2) |
| embedded_rebar_tet4 | mesh | nested_dissection | 5 | 1 | 21.67 ms [21.61 ms, 21.69 ms] | 21.67 ms [21.61 ms, 21.69 ms] | 996.3 [993.5, 997.4] | 1.003 [0.996, 1.005] | DIFFER (2) |
| embedded_rebar_tet4 | init | auto | 5 | 1 | 39.88 ms [39.50 ms, 40.40 ms] | 39.88 ms [39.50 ms, 40.40 ms] | 1833.7 [1816.2, 1857.6] | 1.005 [0.971, 1.014] | DIFFER (2) |
| embedded_rebar_tet4 | init | base | 5 | 1 | 39.92 ms [39.54 ms, 40.23 ms] | 39.92 ms [39.54 ms, 40.23 ms] | 1835.2 [1818.0, 1849.8] | 1.000 [0.983, 1.016] | DIFFER (2) |
| embedded_rebar_tet4 | init | colamd | 5 | 1 | 39.82 ms [39.59 ms, 40.69 ms] | 39.82 ms [39.59 ms, 40.69 ms] | 1830.6 [1820.3, 1870.9] | – | DIFFER (2) |
| embedded_rebar_tet4 | init | nested_dissection | 5 | 1 | 40.06 ms [39.76 ms, 40.49 ms] | 40.06 ms [39.76 ms, 40.49 ms] | 1841.7 [1828.0, 1861.5] | 1.002 [0.984, 1.016] | DIFFER (2) |
| embedded_rebar_tet4 | intersect | auto | 5 | 5 | 93.27 ms [93.13 ms, 93.42 ms] | 18.65 ms [18.63 ms, 18.68 ms] | 857.7 [856.4, 859.0] | 1.000 [0.996, 1.001] | DIFFER (2) |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 92.73 ms [92.61 ms, 92.79 ms] | 18.55 ms [18.52 ms, 18.56 ms] | 852.7 [851.6, 853.2] | 0.994 [0.992, 0.995] | DIFFER (2) |
| embedded_rebar_tet4 | intersect | colamd | 5 | 5 | 93.35 ms [93.11 ms, 93.46 ms] | 18.67 ms [18.62 ms, 18.69 ms] | 858.4 [856.2, 859.4] | – | DIFFER (2) |
| embedded_rebar_tet4 | intersect | nested_dissection | 5 | 5 | 93.12 ms [92.92 ms, 93.50 ms] | 18.62 ms [18.58 ms, 18.70 ms] | 856.3 [854.5, 859.8] | 1.000 [0.995, 1.002] | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | auto | 5 | 1 | 635.73 ms [634.20 ms, 639.21 ms] | 635.73 ms [634.20 ms, 639.21 ms] | 29229.0 [29158.4, 29388.8] | 0.599 [0.592, 0.603] | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 1.073 s [1.063 s, 1.080 s] | 1.073 s [1.063 s, 1.080 s] | 49312.4 [48863.0, 49645.8] | 1.006 [0.997, 1.016] | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | colamd | 5 | 1 | 1.066 s [1.060 s, 1.073 s] | 1.066 s [1.060 s, 1.073 s] | 48993.2 [48713.4, 49344.8] | – | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | nested_dissection | 5 | 1 | 550.98 ms [547.62 ms, 551.69 ms] | 550.98 ms [547.62 ms, 551.69 ms] | 25332.2 [25177.7, 25365.0] | 0.516 [0.512, 0.521] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | auto | 5 | 20 | 28.04 ms [27.87 ms, 28.20 ms] | 1.40 ms [1.39 ms, 1.41 ms] | 64.5 [64.1, 64.8] | 1.000 [0.995, 1.007] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 28.57 ms [28.54 ms, 28.94 ms] | 1.43 ms [1.43 ms, 1.45 ms] | 65.7 [65.6, 66.5] | 1.018 [1.017, 1.040] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | colamd | 5 | 20 | 28.03 ms [27.83 ms, 28.21 ms] | 1.40 ms [1.39 ms, 1.41 ms] | 64.4 [64.0, 64.8] | – | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | nested_dissection | 5 | 20 | 27.94 ms [27.87 ms, 28.01 ms] | 1.40 ms [1.39 ms, 1.40 ms] | 64.2 [64.1, 64.4] | 0.999 [0.991, 1.002] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | auto | 5 | 5 | 77.27 ms [76.95 ms, 77.71 ms] | 15.45 ms [15.39 ms, 15.54 ms] | 710.5 [707.6, 714.6] | 0.996 [0.991, 1.007] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 77.59 ms [77.35 ms, 77.83 ms] | 15.52 ms [15.47 ms, 15.57 ms] | 713.5 [711.3, 715.7] | 1.001 [0.998, 1.008] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | colamd | 5 | 5 | 77.40 ms [77.18 ms, 77.66 ms] | 15.48 ms [15.44 ms, 15.53 ms] | 711.7 [709.7, 714.1] | – | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | nested_dissection | 5 | 5 | 76.92 ms [76.86 ms, 77.09 ms] | 15.38 ms [15.37 ms, 15.42 ms] | 707.3 [706.8, 708.9] | 0.996 [0.990, 0.997] | DIFFER (2) |
| embedded_rebar_tri3 | mesh | auto | 5 | 1 | 20.34 ms [20.25 ms, 20.54 ms] | 20.34 ms [20.25 ms, 20.54 ms] | 807.1 [803.6, 814.9] | 0.992 [0.984, 1.002] | DIFFER (2) |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 20.49 ms [20.41 ms, 20.60 ms] | 20.49 ms [20.41 ms, 20.60 ms] | 813.3 [809.9, 817.5] | 1.001 [0.992, 1.005] | DIFFER (2) |
| embedded_rebar_tri3 | mesh | colamd | 5 | 1 | 20.49 ms [20.47 ms, 20.58 ms] | 20.49 ms [20.47 ms, 20.58 ms] | 813.3 [812.2, 816.5] | – | DIFFER (2) |
| embedded_rebar_tri3 | mesh | nested_dissection | 5 | 1 | 20.42 ms [20.29 ms, 20.54 ms] | 20.42 ms [20.29 ms, 20.54 ms] | 810.2 [805.2, 815.2] | 0.995 [0.991, 1.002] | DIFFER (2) |
| embedded_rebar_tri3 | init | auto | 5 | 1 | 31.71 ms [31.52 ms, 32.14 ms] | 31.71 ms [31.52 ms, 32.14 ms] | 1258.3 [1250.8, 1275.5] | 1.008 [0.987, 1.027] | DIFFER (2) |
| embedded_rebar_tri3 | init | base | 5 | 1 | 31.50 ms [31.31 ms, 31.52 ms] | 31.50 ms [31.31 ms, 31.52 ms] | 1249.9 [1242.3, 1250.7] | 1.000 [0.986, 1.007] | DIFFER (2) |
| embedded_rebar_tri3 | init | colamd | 5 | 1 | 31.44 ms [31.31 ms, 31.94 ms] | 31.44 ms [31.31 ms, 31.94 ms] | 1247.5 [1242.3, 1267.5] | – | DIFFER (2) |
| embedded_rebar_tri3 | init | nested_dissection | 5 | 1 | 31.33 ms [31.28 ms, 31.66 ms] | 31.33 ms [31.28 ms, 31.66 ms] | 1243.4 [1241.4, 1256.3] | 1.000 [0.979, 1.011] | DIFFER (2) |
| embedded_rebar_tri3 | intersect | auto | 5 | 5 | 67.03 ms [66.81 ms, 67.34 ms] | 13.41 ms [13.36 ms, 13.47 ms] | 532.0 [530.2, 534.4] | 1.001 [0.999, 1.010] | DIFFER (2) |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 66.85 ms [66.68 ms, 67.04 ms] | 13.37 ms [13.34 ms, 13.41 ms] | 530.5 [529.2, 532.0] | 1.001 [0.996, 1.006] | DIFFER (2) |
| embedded_rebar_tri3 | intersect | colamd | 5 | 5 | 66.80 ms [66.67 ms, 67.12 ms] | 13.36 ms [13.33 ms, 13.42 ms] | 530.1 [529.1, 532.7] | – | DIFFER (2) |
| embedded_rebar_tri3 | intersect | nested_dissection | 5 | 5 | 67.08 ms [66.96 ms, 67.37 ms] | 13.42 ms [13.39 ms, 13.47 ms] | 532.4 [531.5, 534.7] | 1.004 [1.003, 1.006] | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | auto | 5 | 1 | 217.56 ms [217.00 ms, 218.51 ms] | 217.56 ms [217.00 ms, 218.51 ms] | 8633.2 [8611.2, 8671.2] | 0.915 [0.901, 0.927] | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 239.83 ms [239.08 ms, 243.01 ms] | 239.83 ms [239.08 ms, 243.01 ms] | 9516.9 [9487.5, 9643.1] | 1.014 [0.993, 1.024] | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | colamd | 5 | 1 | 237.20 ms [235.76 ms, 240.98 ms] | 237.20 ms [235.76 ms, 240.98 ms] | 9412.8 [9355.6, 9562.6] | – | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | nested_dissection | 5 | 1 | 188.57 ms [188.01 ms, 189.91 ms] | 188.57 ms [188.01 ms, 189.91 ms] | 7483.0 [7460.6, 7536.2] | 0.794 [0.783, 0.799] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | auto | 5 | 20 | 44.33 ms [44.09 ms, 44.57 ms] | 2.22 ms [2.20 ms, 2.23 ms] | 88.0 [87.5, 88.4] | 1.005 [0.997, 1.006] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 44.29 ms [44.16 ms, 44.55 ms] | 2.21 ms [2.21 ms, 2.23 ms] | 87.9 [87.6, 88.4] | 1.004 [0.997, 1.007] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | colamd | 5 | 20 | 44.22 ms [44.10 ms, 44.28 ms] | 2.21 ms [2.20 ms, 2.21 ms] | 87.7 [87.5, 87.9] | – | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | nested_dissection | 5 | 20 | 44.53 ms [44.28 ms, 49.31 ms] | 2.23 ms [2.21 ms, 2.47 ms] | 88.4 [87.9, 97.8] | 1.006 [1.001, 1.118] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | auto | 5 | 5 | 36.47 ms [36.30 ms, 36.52 ms] | 7.29 ms [7.26 ms, 7.30 ms] | 289.5 [288.1, 289.8] | 1.015 [1.008, 1.022] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 35.66 ms [35.56 ms, 35.78 ms] | 7.13 ms [7.11 ms, 7.16 ms] | 283.0 [282.3, 284.0] | 0.995 [0.984, 0.997] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | colamd | 5 | 5 | 35.77 ms [35.69 ms, 36.23 ms] | 7.15 ms [7.14 ms, 7.25 ms] | 283.9 [283.2, 287.6] | – | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | nested_dissection | 5 | 5 | 35.69 ms [35.46 ms, 37.15 ms] | 7.14 ms [7.09 ms, 7.43 ms] | 283.3 [281.4, 294.8] | 0.998 [0.992, 1.025] | DIFFER (2) |
| heat_implicit_hex8_kvar | mesh | auto | 5 | 1 | 3.90 ms [3.87 ms, 3.92 ms] | 3.90 ms [3.87 ms, 3.92 ms] | 2257.5 [2241.9, 2268.5] | 1.003 [0.996, 1.014] | DIFFER (2) |
| heat_implicit_hex8_kvar | mesh | base | 5 | 1 | 3.91 ms [3.89 ms, 3.96 ms] | 3.91 ms [3.89 ms, 3.96 ms] | 2261.0 [2253.5, 2291.1] | 1.008 [1.001, 1.025] | DIFFER (2) |
| heat_implicit_hex8_kvar | mesh | colamd | 5 | 1 | 3.89 ms [3.86 ms, 3.89 ms] | 3.89 ms [3.86 ms, 3.89 ms] | 2250.0 [2234.4, 2252.3] | – | DIFFER (2) |
| heat_implicit_hex8_kvar | mesh | nested_dissection | 5 | 1 | 3.87 ms [3.85 ms, 3.90 ms] | 3.87 ms [3.85 ms, 3.90 ms] | 2237.3 [2229.7, 2254.6] | 1.001 [0.991, 1.004] | DIFFER (2) |
| heat_implicit_hex8_kvar | init | auto | 5 | 1 | 952.0 µs [942.0 µs, 968.0 µs] | 952.0 µs [942.0 µs, 968.0 µs] | 550.9 [545.1, 560.2] | 0.997 [0.978, 1.010] | DIFFER (2) |
| heat_implicit_hex8_kvar | init | base | 5 | 1 | 963.0 µs [952.0 µs, 969.0 µs] | 963.0 µs [952.0 µs, 969.0 µs] | 557.3 [550.9, 560.8] | 1.001 [0.983, 1.028] | DIFFER (2) |
| heat_implicit_hex8_kvar | init | colamd | 5 | 1 | 957.0 µs [943.0 µs, 970.0 µs] | 957.0 µs [943.0 µs, 970.0 µs] | 553.8 [545.7, 561.3] | – | DIFFER (2) |
| heat_implicit_hex8_kvar | init | nested_dissection | 5 | 1 | 956.0 µs [933.0 µs, 959.0 µs] | 956.0 µs [933.0 µs, 959.0 µs] | 553.2 [539.9, 555.0] | 0.987 [0.962, 1.017] | DIFFER (2) |
| heat_implicit_hex8_kvar | solve_step | auto | 5 | 5 | 335.61 ms [333.85 ms, 337.26 ms] | 67.12 ms [66.77 ms, 67.45 ms] | 38843.6 [38640.5, 39034.8] | 0.578 [0.577, 0.582] | DIFFER (2) |
| heat_implicit_hex8_kvar | solve_step | base | 5 | 5 | 576.00 ms [575.47 ms, 576.88 ms] | 115.20 ms [115.09 ms, 115.38 ms] | 66666.4 [66605.1, 66767.9] | 0.993 [0.987, 0.999] | DIFFER (2) |
| heat_implicit_hex8_kvar | solve_step | colamd | 5 | 5 | 581.01 ms [576.67 ms, 583.62 ms] | 116.20 ms [115.33 ms, 116.72 ms] | 67246.3 [66744.4, 67548.7] | – | DIFFER (2) |
| heat_implicit_hex8_kvar | solve_step | nested_dissection | 5 | 5 | 336.22 ms [333.44 ms, 338.76 ms] | 67.24 ms [66.69 ms, 67.75 ms] | 38914.7 [38592.0, 39208.1] | 0.579 [0.576, 0.583] | DIFFER (2) |
| heat_static_tri6 | mesh | auto | 5 | 1 | 40.52 ms [40.28 ms, 40.57 ms] | 40.52 ms [40.28 ms, 40.57 ms] | 1674.2 [1664.5, 1676.4] | 1.003 [0.995, 1.008] | DIFFER (2) |
| heat_static_tri6 | mesh | base | 5 | 1 | 40.27 ms [40.03 ms, 40.70 ms] | 40.27 ms [40.03 ms, 40.70 ms] | 1664.0 [1654.1, 1682.0] | 0.997 [0.991, 1.005] | DIFFER (2) |
| heat_static_tri6 | mesh | colamd | 5 | 1 | 40.41 ms [40.25 ms, 40.48 ms] | 40.41 ms [40.25 ms, 40.48 ms] | 1669.7 [1663.3, 1672.9] | – | DIFFER (2) |
| heat_static_tri6 | mesh | nested_dissection | 5 | 1 | 40.37 ms [40.05 ms, 40.65 ms] | 40.37 ms [40.05 ms, 40.65 ms] | 1668.0 [1655.0, 1679.8] | 0.999 [0.994, 1.005] | DIFFER (2) |
| heat_static_tri6 | init | auto | 5 | 1 | 5.32 ms [5.29 ms, 5.36 ms] | 5.32 ms [5.29 ms, 5.36 ms] | 219.8 [218.4, 221.3] | 0.999 [0.994, 1.011] | DIFFER (2) |
| heat_static_tri6 | init | base | 5 | 1 | 5.33 ms [5.29 ms, 5.37 ms] | 5.33 ms [5.29 ms, 5.37 ms] | 220.1 [218.6, 222.0] | 1.002 [1.000, 1.005] | DIFFER (2) |
| heat_static_tri6 | init | colamd | 5 | 1 | 5.31 ms [5.29 ms, 5.36 ms] | 5.31 ms [5.29 ms, 5.36 ms] | 219.5 [218.6, 221.4] | – | DIFFER (2) |
| heat_static_tri6 | init | nested_dissection | 5 | 1 | 5.32 ms [5.30 ms, 5.33 ms] | 5.32 ms [5.30 ms, 5.33 ms] | 219.9 [219.2, 220.4] | 0.999 [0.993, 1.007] | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | auto | 5 | 5 | 29.56 ms [29.53 ms, 29.65 ms] | 5.91 ms [5.91 ms, 5.93 ms] | 244.3 [244.1, 245.0] | 0.998 [0.924, 0.999] | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | base | 5 | 5 | 29.23 ms [29.10 ms, 29.60 ms] | 5.85 ms [5.82 ms, 5.92 ms] | 241.6 [240.5, 244.7] | 0.985 [0.911, 1.001] | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | colamd | 5 | 5 | 29.65 ms [29.57 ms, 32.04 ms] | 5.93 ms [5.91 ms, 6.41 ms] | 245.0 [244.4, 264.8] | – | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | nested_dissection | 5 | 5 | 29.36 ms [29.33 ms, 29.48 ms] | 5.87 ms [5.87 ms, 5.90 ms] | 242.6 [242.4, 243.6] | 0.992 [0.916, 0.993] | DIFFER (2) |
| heat_static_tri6 | solve_step | auto | 5 | 1 | 901.35 ms [900.41 ms, 913.14 ms] | 901.35 ms [900.41 ms, 913.14 ms] | 37245.8 [37207.0, 37733.1] | 0.514 [0.509, 0.523] | DIFFER (2) |
| heat_static_tri6 | solve_step | base | 5 | 1 | 1.762 s [1.745 s, 1.794 s] | 1.762 s [1.745 s, 1.794 s] | 72821.6 [72104.3, 74136.7] | 1.000 [0.996, 1.018] | DIFFER (2) |
| heat_static_tri6 | solve_step | colamd | 5 | 1 | 1.752 s [1.744 s, 1.770 s] | 1.752 s [1.744 s, 1.770 s] | 72378.0 [72080.9, 73139.2] | – | DIFFER (2) |
| heat_static_tri6 | solve_step | nested_dissection | 5 | 1 | 851.03 ms [847.36 ms, 852.32 ms] | 851.03 ms [847.36 ms, 852.32 ms] | 35166.7 [35014.9, 35219.7] | 0.484 [0.482, 0.488] | DIFFER (2) |
| phasefield_staggered_tet4 | mesh | auto | 5 | 1 | 6.68 ms [6.66 ms, 6.79 ms] | 6.68 ms [6.66 ms, 6.79 ms] | 1113.2 [1109.3, 1131.2] | 0.993 [0.990, 1.009] | DIFFER (3) |
| phasefield_staggered_tet4 | mesh | base | 5 | 1 | 6.76 ms [6.75 ms, 6.82 ms] | 6.76 ms [6.75 ms, 6.82 ms] | 1126.7 [1124.2, 1136.8] | 1.004 [0.999, 1.014] | DIFFER (3) |
| phasefield_staggered_tet4 | mesh | colamd | 5 | 1 | 6.72 ms [6.68 ms, 6.80 ms] | 6.72 ms [6.68 ms, 6.80 ms] | 1120.7 [1112.5, 1132.8] | – | DIFFER (3) |
| phasefield_staggered_tet4 | mesh | nested_dissection | 5 | 1 | 6.73 ms [6.71 ms, 6.77 ms] | 6.73 ms [6.71 ms, 6.77 ms] | 1122.5 [1117.8, 1129.2] | 1.001 [0.991, 1.008] | DIFFER (3) |
| phasefield_staggered_tet4 | init | auto | 5 | 1 | 2.77 ms [2.75 ms, 2.78 ms] | 2.77 ms [2.75 ms, 2.78 ms] | 462.5 [458.8, 463.7] | 0.996 [0.991, 1.006] | DIFFER (3) |
| phasefield_staggered_tet4 | init | base | 5 | 1 | 2.79 ms [2.77 ms, 2.83 ms] | 2.79 ms [2.77 ms, 2.83 ms] | 464.2 [461.8, 472.2] | 1.007 [0.997, 1.014] | DIFFER (3) |
| phasefield_staggered_tet4 | init | colamd | 5 | 1 | 2.78 ms [2.77 ms, 2.80 ms] | 2.78 ms [2.77 ms, 2.80 ms] | 463.3 [460.8, 466.3] | – | DIFFER (3) |
| phasefield_staggered_tet4 | init | nested_dissection | 5 | 1 | 2.77 ms [2.75 ms, 2.79 ms] | 2.77 ms [2.75 ms, 2.79 ms] | 461.5 [458.8, 465.0] | 1.001 [0.984, 1.008] | DIFFER (3) |
| phasefield_staggered_tet4 | solid_solve | auto | 5 | 4 | 309.15 ms [308.05 ms, 311.80 ms] | 77.29 ms [77.01 ms, 77.95 ms] | 12881.3 [12835.3, 12991.5] | 0.594 [0.587, 0.597] | DIFFER (3) |
| phasefield_staggered_tet4 | solid_solve | base | 5 | 4 | 523.43 ms [519.46 ms, 524.87 ms] | 130.86 ms [129.86 ms, 131.22 ms] | 21809.5 [21644.1, 21869.8] | 1.000 [0.988, 1.005] | DIFFER (3) |
| phasefield_staggered_tet4 | solid_solve | colamd | 5 | 4 | 522.81 ms [520.74 ms, 525.65 ms] | 130.70 ms [130.19 ms, 131.41 ms] | 21783.7 [21697.5, 21902.3] | – | DIFFER (3) |
| phasefield_staggered_tet4 | solid_solve | nested_dissection | 5 | 4 | 308.70 ms [307.57 ms, 309.23 ms] | 77.17 ms [76.89 ms, 77.31 ms] | 12862.3 [12815.4, 12884.8] | 0.589 [0.585, 0.593] | DIFFER (3) |
| phasefield_staggered_tet4 | strain_transfer | auto | 5 | 4 | 677.0 µs [673.0 µs, 689.0 µs] | 169.2 µs [168.2 µs, 172.2 µs] | 28.2 [28.0, 28.7] | 0.980 [0.959, 0.997] | DIFFER (3) |
| phasefield_staggered_tet4 | strain_transfer | base | 5 | 4 | 700.0 µs [675.0 µs, 706.0 µs] | 175.0 µs [168.8 µs, 176.5 µs] | 29.2 [28.1, 29.4] | 1.003 [0.960, 1.038] | DIFFER (3) |
| phasefield_staggered_tet4 | strain_transfer | colamd | 5 | 4 | 691.0 µs [680.0 µs, 703.0 µs] | 172.8 µs [170.0 µs, 175.8 µs] | 28.8 [28.3, 29.3] | – | DIFFER (3) |
| phasefield_staggered_tet4 | strain_transfer | nested_dissection | 5 | 4 | 679.0 µs [674.0 µs, 686.0 µs] | 169.8 µs [168.5 µs, 171.5 µs] | 28.3 [28.1, 28.6] | 0.975 [0.959, 1.007] | DIFFER (3) |
| phasefield_staggered_tet4 | phase_solve | auto | 5 | 4 | 63.14 ms [62.88 ms, 63.33 ms] | 15.79 ms [15.72 ms, 15.83 ms] | 2631.0 [2620.0, 2638.5] | 0.994 [0.992, 1.000] | DIFFER (3) |
| phasefield_staggered_tet4 | phase_solve | base | 5 | 4 | 63.16 ms [62.87 ms, 63.25 ms] | 15.79 ms [15.72 ms, 15.81 ms] | 2631.8 [2619.4, 2635.4] | 0.996 [0.990, 0.999] | DIFFER (3) |
| phasefield_staggered_tet4 | phase_solve | colamd | 5 | 4 | 63.30 ms [63.21 ms, 63.65 ms] | 15.82 ms [15.80 ms, 15.91 ms] | 2637.5 [2633.9, 2652.2] | – | DIFFER (3) |
| phasefield_staggered_tet4 | phase_solve | nested_dissection | 5 | 4 | 46.82 ms [46.61 ms, 46.98 ms] | 11.70 ms [11.65 ms, 11.74 ms] | 1950.8 [1942.2, 1957.4] | 0.738 [0.732, 0.742] | DIFFER (3) |
| phasefield_staggered_tet4 | damage_transfer | auto | 5 | 4 | 1.60 ms [1.59 ms, 1.60 ms] | 399.8 µs [397.8 µs, 400.8 µs] | 66.6 [66.3, 66.8] | 0.990 [0.986, 1.021] | DIFFER (3) |
| phasefield_staggered_tet4 | damage_transfer | base | 5 | 4 | 1.60 ms [1.58 ms, 1.62 ms] | 400.5 µs [394.0 µs, 406.0 µs] | 66.8 [65.7, 67.7] | 1.001 [0.985, 1.012] | DIFFER (3) |
| phasefield_staggered_tet4 | damage_transfer | colamd | 5 | 4 | 1.61 ms [1.57 ms, 1.62 ms] | 402.0 µs [392.5 µs, 404.5 µs] | 67.0 [65.4, 67.4] | – | DIFFER (3) |
| phasefield_staggered_tet4 | damage_transfer | nested_dissection | 5 | 4 | 1.59 ms [1.57 ms, 1.67 ms] | 398.5 µs [392.0 µs, 416.5 µs] | 66.4 [65.3, 69.4] | 0.986 [0.975, 1.031] | DIFFER (3) |
| phasefield_staggered_tet4 | internal_forces | auto | 5 | 4 | 1.51 ms [1.50 ms, 1.54 ms] | 376.5 µs [374.2 µs, 384.2 µs] | 62.8 [62.4, 64.0] | 0.993 [0.981, 1.035] | DIFFER (3) |
| phasefield_staggered_tet4 | internal_forces | base | 5 | 4 | 1.53 ms [1.51 ms, 1.58 ms] | 382.5 µs [378.2 µs, 394.0 µs] | 63.8 [63.0, 65.7] | 1.024 [1.000, 1.028] | DIFFER (3) |
| phasefield_staggered_tet4 | internal_forces | colamd | 5 | 4 | 1.51 ms [1.49 ms, 1.54 ms] | 377.0 µs [371.2 µs, 384.8 µs] | 62.8 [61.9, 64.1] | – | DIFFER (3) |
| phasefield_staggered_tet4 | internal_forces | nested_dissection | 5 | 4 | 1.51 ms [1.50 ms, 1.52 ms] | 376.2 µs [375.8 µs, 379.8 µs] | 62.7 [62.6, 63.3] | 0.999 [0.987, 1.012] | DIFFER (3) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | auto | 5 | 5 | 19.77 ms [19.73 ms, 20.01 ms] | 3.95 ms [3.95 ms, 4.00 ms] | 658.8 [657.5, 666.9] | 0.999 [0.997, 1.008] | DIFFER (3) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base | 5 | 5 | 19.44 ms [19.42 ms, 19.50 ms] | 3.89 ms [3.88 ms, 3.90 ms] | 648.1 [647.2, 650.0] | 0.984 [0.978, 0.986] | DIFFER (3) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | colamd | 5 | 5 | 19.83 ms [19.72 ms, 19.88 ms] | 3.97 ms [3.94 ms, 3.98 ms] | 660.9 [657.4, 662.6] | – | DIFFER (3) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | nested_dissection | 5 | 5 | 19.42 ms [19.41 ms, 19.65 ms] | 3.88 ms [3.88 ms, 3.93 ms] | 647.3 [647.1, 655.1] | 0.983 [0.977, 0.990] | DIFFER (3) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | auto | 5 | 5 | 4.59 ms [4.58 ms, 4.64 ms] | 918.8 µs [915.0 µs, 928.2 µs] | 153.1 [152.5, 154.7] | 0.999 [0.983, 1.001] | DIFFER (3) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base | 5 | 5 | 4.56 ms [4.54 ms, 4.57 ms] | 911.6 µs [907.4 µs, 914.6 µs] | 151.9 [151.2, 152.4] | 0.987 [0.981, 0.990] | DIFFER (3) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | colamd | 5 | 5 | 4.62 ms [4.58 ms, 4.66 ms] | 924.2 µs [917.0 µs, 932.0 µs] | 154.0 [152.8, 155.3] | – | DIFFER (3) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | nested_dissection | 5 | 5 | 5.16 ms [5.15 ms, 5.18 ms] | 1.03 ms [1.03 ms, 1.04 ms] | 172.0 [171.6, 172.6] | 1.116 [1.109, 1.123] | DIFFER (3) |
| phasefield_staggered_tri3 | mesh | auto | 5 | 1 | 42.13 ms [41.96 ms, 42.30 ms] | 42.13 ms [41.96 ms, 42.30 ms] | 822.9 [819.6, 826.2] | 0.998 [0.995, 1.002] | DIFFER (2) |
| phasefield_staggered_tri3 | mesh | base | 5 | 1 | 42.64 ms [42.33 ms, 43.16 ms] | 42.64 ms [42.33 ms, 43.16 ms] | 832.7 [826.8, 843.0] | 1.010 [1.004, 1.025] | DIFFER (2) |
| phasefield_staggered_tri3 | mesh | colamd | 5 | 1 | 42.17 ms [42.04 ms, 42.40 ms] | 42.17 ms [42.04 ms, 42.40 ms] | 823.7 [821.2, 828.2] | – | DIFFER (2) |
| phasefield_staggered_tri3 | mesh | nested_dissection | 5 | 1 | 42.07 ms [41.88 ms, 42.43 ms] | 42.07 ms [41.88 ms, 42.43 ms] | 821.7 [818.0, 828.8] | 0.996 [0.992, 1.008] | DIFFER (2) |
| phasefield_staggered_tri3 | init | auto | 5 | 1 | 19.41 ms [19.38 ms, 19.53 ms] | 19.41 ms [19.38 ms, 19.53 ms] | 379.2 [378.5, 381.4] | 0.997 [0.996, 1.000] | DIFFER (2) |
| phasefield_staggered_tri3 | init | base | 5 | 1 | 19.49 ms [19.16 ms, 19.50 ms] | 19.49 ms [19.16 ms, 19.50 ms] | 380.7 [374.2, 380.9] | 1.001 [0.986, 1.004] | DIFFER (2) |
| phasefield_staggered_tri3 | init | colamd | 5 | 1 | 19.45 ms [19.41 ms, 19.55 ms] | 19.45 ms [19.41 ms, 19.55 ms] | 380.0 [379.0, 381.9] | – | DIFFER (2) |
| phasefield_staggered_tri3 | init | nested_dissection | 5 | 1 | 19.38 ms [19.30 ms, 19.53 ms] | 19.38 ms [19.30 ms, 19.53 ms] | 378.5 [376.9, 381.4] | 0.994 [0.991, 1.006] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | auto | 5 | 4 | 1.422 s [1.420 s, 1.428 s] | 355.38 ms [355.10 ms, 356.99 ms] | 6941.1 [6935.6, 6972.4] | 0.468 [0.467, 0.470] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | base | 5 | 4 | 3.052 s [3.030 s, 3.056 s] | 763.07 ms [757.60 ms, 764.10 ms] | 14903.7 [14796.8, 14923.8] | 1.004 [0.997, 1.009] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | colamd | 5 | 4 | 3.040 s [3.029 s, 3.045 s] | 760.07 ms [757.26 ms, 761.24 ms] | 14845.0 [14790.3, 14868.0] | – | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | nested_dissection | 5 | 4 | 1.424 s [1.422 s, 1.431 s] | 356.08 ms [355.40 ms, 357.65 ms] | 6954.6 [6941.4, 6985.3] | 0.468 [0.467, 0.472] | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | auto | 5 | 4 | 4.10 ms [4.08 ms, 4.14 ms] | 1.02 ms [1.02 ms, 1.04 ms] | 20.0 [19.9, 20.2] | 0.999 [0.995, 1.027] | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | base | 5 | 4 | 4.25 ms [4.21 ms, 4.29 ms] | 1.06 ms [1.05 ms, 1.07 ms] | 20.8 [20.6, 21.0] | 1.042 [1.027, 1.063] | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | colamd | 5 | 4 | 4.10 ms [4.04 ms, 4.12 ms] | 1.02 ms [1.01 ms, 1.03 ms] | 20.0 [19.7, 20.1] | – | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | nested_dissection | 5 | 4 | 4.06 ms [4.04 ms, 4.09 ms] | 1.02 ms [1.01 ms, 1.02 ms] | 19.8 [19.8, 20.0] | 0.992 [0.983, 1.007] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | auto | 5 | 4 | 482.45 ms [482.30 ms, 484.70 ms] | 120.61 ms [120.58 ms, 121.18 ms] | 2355.7 [2355.0, 2366.7] | 0.612 [0.607, 0.613] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | base | 5 | 4 | 786.04 ms [784.73 ms, 787.48 ms] | 196.51 ms [196.18 ms, 196.87 ms] | 3838.1 [3831.7, 3845.1] | 0.994 [0.990, 0.998] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | colamd | 5 | 4 | 790.79 ms [787.99 ms, 795.16 ms] | 197.70 ms [197.00 ms, 198.79 ms] | 3861.3 [3847.6, 3882.6] | – | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | nested_dissection | 5 | 4 | 478.50 ms [478.25 ms, 479.21 ms] | 119.63 ms [119.56 ms, 119.80 ms] | 2336.4 [2335.2, 2339.9] | 0.606 [0.602, 0.607] | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | auto | 5 | 4 | 12.15 ms [12.13 ms, 12.20 ms] | 3.04 ms [3.03 ms, 3.05 ms] | 59.3 [59.2, 59.6] | 0.996 [0.991, 1.007] | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | base | 5 | 4 | 12.12 ms [12.08 ms, 12.25 ms] | 3.03 ms [3.02 ms, 3.06 ms] | 59.2 [59.0, 59.8] | 0.997 [0.988, 1.008] | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | colamd | 5 | 4 | 12.24 ms [12.11 ms, 12.26 ms] | 3.06 ms [3.03 ms, 3.06 ms] | 59.8 [59.2, 59.9] | – | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | nested_dissection | 5 | 4 | 12.12 ms [12.11 ms, 12.13 ms] | 3.03 ms [3.03 ms, 3.03 ms] | 59.2 [59.1, 59.2] | 0.990 [0.988, 1.001] | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | auto | 5 | 4 | 7.05 ms [7.00 ms, 7.10 ms] | 1.76 ms [1.75 ms, 1.77 ms] | 34.4 [34.2, 34.7] | 0.991 [0.989, 1.011] | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | base | 5 | 4 | 7.05 ms [6.98 ms, 7.14 ms] | 1.76 ms [1.74 ms, 1.79 ms] | 34.4 [34.1, 34.9] | 0.991 [0.986, 1.020] | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | colamd | 5 | 4 | 7.07 ms [7.01 ms, 7.12 ms] | 1.77 ms [1.75 ms, 1.78 ms] | 34.5 [34.2, 34.8] | – | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | nested_dissection | 5 | 4 | 7.06 ms [7.01 ms, 7.11 ms] | 1.76 ms [1.75 ms, 1.78 ms] | 34.5 [34.2, 34.7] | 1.005 [0.985, 1.008] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | auto | 5 | 5 | 41.50 ms [41.40 ms, 41.62 ms] | 8.30 ms [8.28 ms, 8.32 ms] | 162.1 [161.7, 162.6] | 0.997 [0.984, 1.001] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base | 5 | 5 | 41.95 ms [41.87 ms, 42.57 ms] | 8.39 ms [8.37 ms, 8.51 ms] | 163.9 [163.5, 166.3] | 1.010 [0.990, 1.026] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | colamd | 5 | 5 | 41.56 ms [41.49 ms, 42.28 ms] | 8.31 ms [8.30 ms, 8.46 ms] | 162.3 [162.1, 165.2] | – | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | nested_dissection | 5 | 5 | 43.91 ms [43.74 ms, 45.23 ms] | 8.78 ms [8.75 ms, 9.05 ms] | 171.5 [170.9, 176.7] | 1.057 [1.035, 1.088] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | auto | 5 | 5 | 35.21 ms [35.16 ms, 35.41 ms] | 7.04 ms [7.03 ms, 7.08 ms] | 137.5 [137.3, 138.3] | 1.036 [1.035, 1.040] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base | 5 | 5 | 34.12 ms [34.04 ms, 34.20 ms] | 6.82 ms [6.81 ms, 6.84 ms] | 133.3 [133.0, 133.6] | 1.004 [1.001, 1.008] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | colamd | 5 | 5 | 33.98 ms [33.93 ms, 34.05 ms] | 6.80 ms [6.79 ms, 6.81 ms] | 132.7 [132.5, 133.0] | – | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | nested_dissection | 5 | 5 | 34.25 ms [34.22 ms, 34.34 ms] | 6.85 ms [6.84 ms, 6.87 ms] | 133.8 [133.7, 134.2] | 1.008 [1.008, 1.010] | DIFFER (2) |
| structural_beam3_frame_dynamic | mesh | auto | 5 | 1 | 331.0 µs [319.0 µs, 358.0 µs] | 331.0 µs [319.0 µs, 358.0 µs] | 54.4 [52.4, 58.8] | 0.988 [0.917, 1.077] | DIFFER (2) |
| structural_beam3_frame_dynamic | mesh | base | 5 | 1 | 326.0 µs [321.0 µs, 369.0 µs] | 326.0 µs [321.0 µs, 369.0 µs] | 53.6 [52.8, 60.7] | 0.991 [0.903, 1.128] | DIFFER (2) |
| structural_beam3_frame_dynamic | mesh | colamd | 5 | 1 | 331.0 µs [324.0 µs, 361.0 µs] | 331.0 µs [324.0 µs, 361.0 µs] | 54.4 [53.3, 59.3] | – | DIFFER (2) |
| structural_beam3_frame_dynamic | mesh | nested_dissection | 5 | 1 | 326.0 µs [324.0 µs, 330.0 µs] | 326.0 µs [324.0 µs, 330.0 µs] | 53.6 [53.3, 54.2] | 0.985 [0.903, 1.019] | DIFFER (2) |
| structural_beam3_frame_dynamic | init | auto | 5 | 1 | 29.95 ms [29.82 ms, 30.22 ms] | 29.95 ms [29.82 ms, 30.22 ms] | 4922.1 [4900.6, 4966.5] | 1.002 [0.997, 1.015] | DIFFER (2) |
| structural_beam3_frame_dynamic | init | base | 5 | 1 | 30.04 ms [29.97 ms, 30.10 ms] | 30.04 ms [29.97 ms, 30.10 ms] | 4937.9 [4926.7, 4948.1] | 1.005 [1.002, 1.010] | DIFFER (2) |
| structural_beam3_frame_dynamic | init | colamd | 5 | 1 | 29.82 ms [29.77 ms, 30.04 ms] | 29.82 ms [29.77 ms, 30.04 ms] | 4901.1 [4893.7, 4937.9] | – | DIFFER (2) |
| structural_beam3_frame_dynamic | init | nested_dissection | 5 | 1 | 29.85 ms [29.78 ms, 29.89 ms] | 29.85 ms [29.78 ms, 29.89 ms] | 4906.5 [4894.1, 4913.1] | 0.999 [0.995, 1.003] | DIFFER (2) |
| structural_beam3_frame_dynamic | solve_step | auto | 5 | 60 | 2.913 s [2.908 s, 2.935 s] | 48.55 ms [48.47 ms, 48.92 ms] | 7979.8 [7966.9, 8040.1] | 0.999 [0.998, 1.004] | DIFFER (2) |
| structural_beam3_frame_dynamic | solve_step | base | 5 | 60 | 2.936 s [2.926 s, 2.944 s] | 48.93 ms [48.76 ms, 49.07 ms] | 8042.8 [8014.6, 8065.6] | 1.007 [1.004, 1.010] | DIFFER (2) |
| structural_beam3_frame_dynamic | solve_step | colamd | 5 | 60 | 2.915 s [2.913 s, 2.922 s] | 48.58 ms [48.55 ms, 48.70 ms] | 7984.3 [7979.3, 8005.1] | – | DIFFER (2) |
| structural_beam3_frame_dynamic | solve_step | nested_dissection | 5 | 60 | 2.937 s [2.931 s, 2.940 s] | 48.95 ms [48.84 ms, 49.00 ms] | 8046.5 [8028.2, 8054.0] | 1.006 [1.005, 1.009] | DIFFER (2) |
| structural_beam3_frame_static | mesh | auto | 5 | 1 | 338.0 µs [322.0 µs, 366.0 µs] | 338.0 µs [322.0 µs, 366.0 µs] | 55.6 [52.9, 60.2] | 1.011 [0.852, 1.053] | DIFFER (2) |
| structural_beam3_frame_static | mesh | base | 5 | 1 | 323.0 µs [317.0 µs, 333.0 µs] | 323.0 µs [317.0 µs, 333.0 µs] | 53.1 [52.1, 54.7] | 0.892 [0.881, 1.012] | DIFFER (2) |
| structural_beam3_frame_static | mesh | colamd | 5 | 1 | 362.0 µs [321.0 µs, 378.0 µs] | 362.0 µs [321.0 µs, 378.0 µs] | 59.5 [52.8, 62.1] | – | DIFFER (2) |
| structural_beam3_frame_static | mesh | nested_dissection | 5 | 1 | 328.0 µs [320.0 µs, 362.0 µs] | 328.0 µs [320.0 µs, 362.0 µs] | 53.9 [52.6, 59.5] | 0.925 [0.868, 1.025] | DIFFER (2) |
| structural_beam3_frame_static | init | auto | 5 | 1 | 29.75 ms [29.48 ms, 29.84 ms] | 29.75 ms [29.48 ms, 29.84 ms] | 4890.4 [4846.3, 4904.8] | 1.000 [0.993, 1.008] | DIFFER (2) |
| structural_beam3_frame_static | init | base | 5 | 1 | 29.94 ms [29.68 ms, 30.29 ms] | 29.94 ms [29.68 ms, 30.29 ms] | 4920.9 [4877.7, 4978.6] | 1.009 [0.997, 1.015] | DIFFER (2) |
| structural_beam3_frame_static | init | colamd | 5 | 1 | 29.75 ms [29.59 ms, 29.83 ms] | 29.75 ms [29.59 ms, 29.83 ms] | 4890.7 [4863.6, 4903.7] | – | DIFFER (2) |
| structural_beam3_frame_static | init | nested_dissection | 5 | 1 | 29.71 ms [29.55 ms, 29.87 ms] | 29.71 ms [29.55 ms, 29.87 ms] | 4883.1 [4856.3, 4909.4] | 1.001 [0.990, 1.007] | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | auto | 5 | 5 | 23.48 ms [23.40 ms, 23.54 ms] | 4.70 ms [4.68 ms, 4.71 ms] | 772.0 [769.4, 773.9] | 0.999 [0.995, 1.002] | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | base | 5 | 5 | 23.46 ms [23.40 ms, 23.61 ms] | 4.69 ms [4.68 ms, 4.72 ms] | 771.1 [769.2, 776.2] | 0.998 [0.996, 1.003] | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | colamd | 5 | 5 | 23.53 ms [23.43 ms, 23.53 ms] | 4.71 ms [4.69 ms, 4.71 ms] | 773.4 [770.2, 773.6] | – | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | nested_dissection | 5 | 5 | 23.46 ms [23.44 ms, 23.59 ms] | 4.69 ms [4.69 ms, 4.72 ms] | 771.3 [770.5, 775.6] | 1.000 [0.996, 1.003] | DIFFER (2) |
| structural_beam3_frame_static | solve_step | auto | 5 | 1 | 810.58 ms [802.62 ms, 812.11 ms] | 810.58 ms [802.62 ms, 812.11 ms] | 133230.6 [131923.2, 133482.4] | 0.604 [0.599, 0.607] | DIFFER (2) |
| structural_beam3_frame_static | solve_step | base | 5 | 1 | 1.340 s [1.333 s, 1.351 s] | 1.340 s [1.333 s, 1.351 s] | 220240.1 [219112.9, 222025.8] | 0.999 [0.992, 1.010] | DIFFER (2) |
| structural_beam3_frame_static | solve_step | colamd | 5 | 1 | 1.341 s [1.335 s, 1.347 s] | 1.341 s [1.335 s, 1.347 s] | 220392.5 [219506.9, 221341.9] | – | DIFFER (2) |
| structural_beam3_frame_static | solve_step | nested_dissection | 5 | 1 | 758.92 ms [754.69 ms, 761.68 ms] | 758.92 ms [754.69 ms, 761.68 ms] | 124740.3 [124044.2, 125193.6] | 0.565 [0.562, 0.570] | DIFFER (2) |
| structural_dkt18_plate_static | mesh | auto | 5 | 1 | 287.0 µs [286.0 µs, 307.0 µs] | 287.0 µs [286.0 µs, 307.0 µs] | 39.9 [39.7, 42.6] | 0.983 [0.972, 1.000] | DIFFER (2) |
| structural_dkt18_plate_static | mesh | base | 5 | 1 | 304.0 µs [286.0 µs, 323.0 µs] | 304.0 µs [286.0 µs, 323.0 µs] | 42.2 [39.7, 44.9] | 1.052 [0.905, 1.125] | DIFFER (2) |
| structural_dkt18_plate_static | mesh | colamd | 5 | 1 | 291.0 µs [287.0 µs, 316.0 µs] | 291.0 µs [287.0 µs, 316.0 µs] | 40.4 [39.9, 43.9] | – | DIFFER (2) |
| structural_dkt18_plate_static | mesh | nested_dissection | 5 | 1 | 287.0 µs [283.0 µs, 289.0 µs] | 287.0 µs [283.0 µs, 289.0 µs] | 39.9 [39.3, 40.1] | 0.986 [0.915, 0.990] | DIFFER (2) |
| structural_dkt18_plate_static | init | auto | 5 | 1 | 69.24 ms [69.00 ms, 69.29 ms] | 69.24 ms [69.00 ms, 69.29 ms] | 9616.3 [9583.3, 9623.5] | 1.001 [0.998, 1.002] | DIFFER (2) |
| structural_dkt18_plate_static | init | base | 5 | 1 | 69.28 ms [69.23 ms, 69.41 ms] | 69.28 ms [69.23 ms, 69.41 ms] | 9622.4 [9615.6, 9640.6] | 1.002 [1.000, 1.004] | DIFFER (2) |
| structural_dkt18_plate_static | init | colamd | 5 | 1 | 69.15 ms [68.96 ms, 69.35 ms] | 69.15 ms [68.96 ms, 69.35 ms] | 9603.8 [9578.2, 9631.9] | – | DIFFER (2) |
| structural_dkt18_plate_static | init | nested_dissection | 5 | 1 | 69.05 ms [68.95 ms, 69.29 ms] | 69.05 ms [68.95 ms, 69.29 ms] | 9590.0 [9576.1, 9623.5] | 0.999 [0.998, 1.001] | DIFFER (2) |
| structural_dkt18_plate_static | assemble_stiffness | auto | 5 | 5 | 114.49 ms [114.18 ms, 114.63 ms] | 22.90 ms [22.84 ms, 22.93 ms] | 3180.2 [3171.8, 3184.2] | 0.997 [0.990, 1.002] | DIFFER (2) |
| structural_dkt18_plate_static | assemble_stiffness | base | 5 | 5 | 114.22 ms [114.05 ms, 114.59 ms] | 22.84 ms [22.81 ms, 22.92 ms] | 3172.7 [3168.0, 3183.1] | 0.997 [0.986, 0.999] | DIFFER (2) |
| structural_dkt18_plate_static | assemble_stiffness | colamd | 5 | 5 | 114.53 ms [114.29 ms, 115.76 ms] | 22.91 ms [22.86 ms, 23.15 ms] | 3181.3 [3174.7, 3215.6] | – | DIFFER (2) |
| structural_dkt18_plate_static | assemble_stiffness | nested_dissection | 5 | 5 | 114.46 ms [114.24 ms, 114.91 ms] | 22.89 ms [22.85 ms, 22.98 ms] | 3179.4 [3173.4, 3191.9] | 1.000 [0.989, 1.003] | DIFFER (2) |
| structural_dkt18_plate_static | solve_step | auto | 5 | 1 | 890.80 ms [886.59 ms, 898.48 ms] | 890.80 ms [886.59 ms, 898.48 ms] | 123721.7 [123137.5, 124788.8] | 0.993 [0.966, 1.004] | DIFFER (2) |
| structural_dkt18_plate_static | solve_step | base | 5 | 1 | 898.19 ms [891.00 ms, 912.41 ms] | 898.19 ms [891.00 ms, 912.41 ms] | 124749.2 [123749.3, 126723.5] | 1.007 [0.967, 1.021] | DIFFER (2) |
| structural_dkt18_plate_static | solve_step | colamd | 5 | 1 | 893.50 ms [888.67 ms, 930.05 ms] | 893.50 ms [888.67 ms, 930.05 ms] | 124096.9 [123426.2, 129173.1] | – | DIFFER (2) |
| structural_dkt18_plate_static | solve_step | nested_dissection | 5 | 1 | 2.754 s [2.746 s, 2.769 s] | 2.754 s [2.746 s, 2.769 s] | 382492.5 [381357.4, 384605.0] | 3.086 [2.970, 3.090] | DIFFER (2) |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 739e03eb4, dfa02e171 | 776a3c6490006b8e, abbaf5af3032e219 |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 739e03eb4, dfa02e171 | 1d4a7198d9949951, 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 739e03eb4, dfa02e171 | 19b493e428f16a69, 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 739e03eb4, dfa02e171 | 55313c35e6ae3f79, 8f59188dbfc2c739 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 739e03eb4, dfa02e171 | 463949e032392419, 5a187045a928bbf6 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 739e03eb4, dfa02e171 | 4696ed1631b4f86a, fae4f8e620c2c34e, fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 739e03eb4, dfa02e171 | a77809465b3e7c18, e25bd76ab4a487de |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | 739e03eb4, dfa02e171 | 7c476f93a2a83956, d90db802f84089fb |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 739e03eb4, dfa02e171 | 0cff4c3b39afeb0d, ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 739e03eb4, dfa02e171 | 1cfc7f010c52d6b3, eb8f138ea394bf69 |
