Suite `models`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | mesh | base | 5 | 1 | 8.29 ms [8.13 ms, 8.55 ms] | 8.29 ms [8.13 ms, 8.55 ms] | 1184.0 [1162.0, 1221.4] | – | same |
| contact_implicit_static_quad4 | mesh | t1 | 5 | 1 | 8.15 ms [7.97 ms, 8.50 ms] | 8.15 ms [7.97 ms, 8.50 ms] | 1163.9 [1139.1, 1214.6] | 0.969 [0.953, 1.045] | same |
| contact_implicit_static_quad4 | mesh | t2 | 5 | 1 | 8.05 ms [7.92 ms, 8.43 ms] | 8.05 ms [7.92 ms, 8.43 ms] | 1150.1 [1131.9, 1204.7] | 0.983 [0.927, 1.036] | same |
| contact_implicit_static_quad4 | mesh | t4 | 5 | 1 | 8.38 ms [8.26 ms, 8.67 ms] | 8.38 ms [8.26 ms, 8.67 ms] | 1197.3 [1180.6, 1238.7] | 1.016 [0.967, 1.040] | same |
| contact_implicit_static_quad4 | mesh | t8 | 5 | 1 | 8.25 ms [8.09 ms, 8.78 ms] | 8.25 ms [8.09 ms, 8.78 ms] | 1179.0 [1156.3, 1254.3] | 1.011 [0.947, 1.053] | same |
| contact_implicit_static_quad4 | init | base | 5 | 1 | 8.74 ms [8.02 ms, 8.78 ms] | 8.74 ms [8.02 ms, 8.78 ms] | 1248.9 [1146.1, 1254.6] | – | same |
| contact_implicit_static_quad4 | init | t1 | 5 | 1 | 7.82 ms [7.74 ms, 8.01 ms] | 7.82 ms [7.74 ms, 8.01 ms] | 1117.6 [1105.4, 1143.6] | 0.910 [0.881, 0.975] | same |
| contact_implicit_static_quad4 | init | t2 | 5 | 1 | 7.93 ms [7.53 ms, 8.69 ms] | 7.93 ms [7.53 ms, 8.69 ms] | 1133.1 [1076.1, 1241.0] | 0.906 [0.862, 1.056] | same |
| contact_implicit_static_quad4 | init | t4 | 5 | 1 | 8.60 ms [8.18 ms, 8.81 ms] | 8.60 ms [8.18 ms, 8.81 ms] | 1228.0 [1169.3, 1258.0] | 0.979 [0.935, 1.077] | same |
| contact_implicit_static_quad4 | init | t8 | 5 | 1 | 8.24 ms [7.67 ms, 8.67 ms] | 8.24 ms [7.67 ms, 8.67 ms] | 1177.6 [1096.1, 1238.4] | 0.941 [0.878, 1.081] | same |
| contact_implicit_static_quad4 | solve_step | base | 5 | 5 | 741.65 ms [740.39 ms, 748.81 ms] | 148.33 ms [148.08 ms, 149.76 ms] | 21190.0 [21153.9, 21394.5] | – | same |
| contact_implicit_static_quad4 | solve_step | t1 | 5 | 5 | 741.27 ms [737.27 ms, 747.06 ms] | 148.25 ms [147.45 ms, 149.41 ms] | 21179.1 [21065.0, 21344.6] | 0.996 [0.993, 1.007] | same |
| contact_implicit_static_quad4 | solve_step | t2 | 5 | 5 | 702.91 ms [700.74 ms, 713.15 ms] | 140.58 ms [140.15 ms, 142.63 ms] | 20083.2 [20021.2, 20375.9] | 0.948 [0.945, 0.952] | same |
| contact_implicit_static_quad4 | solve_step | t4 | 5 | 5 | 694.79 ms [687.56 ms, 707.46 ms] | 138.96 ms [137.51 ms, 141.49 ms] | 19851.1 [19644.7, 20213.1] | 0.930 [0.928, 0.954] | same |
| contact_implicit_static_quad4 | solve_step | t8 | 5 | 5 | 685.39 ms [684.20 ms, 696.69 ms] | 137.08 ms [136.84 ms, 139.34 ms] | 19582.7 [19548.7, 19905.3] | 0.924 [0.920, 0.941] | same |
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 21.18 ms [20.65 ms, 21.60 ms] | 21.18 ms [20.65 ms, 21.60 ms] | 973.7 [949.4, 993.3] | – | same |
| embedded_rebar_tet4 | mesh | t1 | 5 | 1 | 20.50 ms [20.16 ms, 21.37 ms] | 20.50 ms [20.16 ms, 21.37 ms] | 942.8 [927.0, 982.4] | 0.978 [0.959, 0.990] | same |
| embedded_rebar_tet4 | mesh | t2 | 5 | 1 | 20.45 ms [20.36 ms, 21.05 ms] | 20.45 ms [20.36 ms, 21.05 ms] | 940.3 [935.9, 967.7] | 0.975 [0.946, 0.986] | same |
| embedded_rebar_tet4 | mesh | t4 | 5 | 1 | 20.45 ms [19.99 ms, 21.60 ms] | 20.45 ms [19.99 ms, 21.60 ms] | 940.4 [919.3, 992.9] | 0.966 [0.942, 1.004] | same |
| embedded_rebar_tet4 | mesh | t8 | 5 | 1 | 20.75 ms [20.04 ms, 21.48 ms] | 20.75 ms [20.04 ms, 21.48 ms] | 954.2 [921.4, 987.7] | 0.970 [0.961, 1.014] | same |
| embedded_rebar_tet4 | init | base | 5 | 1 | 36.53 ms [33.96 ms, 36.72 ms] | 36.53 ms [33.96 ms, 36.72 ms] | 1679.7 [1561.3, 1688.3] | – | same |
| embedded_rebar_tet4 | init | t1 | 5 | 1 | 34.34 ms [33.86 ms, 34.60 ms] | 34.34 ms [33.86 ms, 34.60 ms] | 1578.6 [1556.7, 1590.7] | 0.940 [0.926, 1.011] | same |
| embedded_rebar_tet4 | init | t2 | 5 | 1 | 34.65 ms [34.55 ms, 37.13 ms] | 34.65 ms [34.55 ms, 37.13 ms] | 1593.0 [1588.3, 1707.2] | 0.959 [0.945, 1.020] | same |
| embedded_rebar_tet4 | init | t4 | 5 | 1 | 36.75 ms [35.84 ms, 37.04 ms] | 36.75 ms [35.84 ms, 37.04 ms] | 1689.8 [1648.0, 1703.0] | 1.001 [0.980, 1.083] | same |
| embedded_rebar_tet4 | init | t8 | 5 | 1 | 36.75 ms [36.17 ms, 36.84 ms] | 36.75 ms [36.17 ms, 36.84 ms] | 1689.6 [1662.8, 1693.7] | 1.005 [0.990, 1.085] | same |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 100.88 ms [100.00 ms, 101.40 ms] | 20.18 ms [20.00 ms, 20.28 ms] | 927.6 [919.5, 932.4] | – | same |
| embedded_rebar_tet4 | intersect | t1 | 5 | 5 | 100.12 ms [99.68 ms, 101.20 ms] | 20.02 ms [19.94 ms, 20.24 ms] | 920.6 [916.6, 930.6] | 0.994 [0.987, 1.010] | same |
| embedded_rebar_tet4 | intersect | t2 | 5 | 5 | 100.85 ms [100.40 ms, 110.45 ms] | 20.17 ms [20.08 ms, 22.09 ms] | 927.3 [923.3, 1015.7] | 1.004 [0.995, 1.103] | same |
| embedded_rebar_tet4 | intersect | t4 | 5 | 5 | 100.74 ms [100.34 ms, 101.62 ms] | 20.15 ms [20.07 ms, 20.32 ms] | 926.3 [922.6, 934.5] | 1.002 [0.996, 1.013] | same |
| embedded_rebar_tet4 | intersect | t8 | 5 | 5 | 101.59 ms [100.73 ms, 104.17 ms] | 20.32 ms [20.15 ms, 20.83 ms] | 934.2 [926.2, 957.9] | 1.010 [1.001, 1.027] | same |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 1.071 s [1.061 s, 1.076 s] | 1.071 s [1.061 s, 1.076 s] | 49245.1 [48762.4, 49470.8] | – | same |
| embedded_rebar_tet4 | solve_step | t1 | 5 | 1 | 1.076 s [1.063 s, 1.079 s] | 1.076 s [1.063 s, 1.079 s] | 49490.6 [48877.5, 49608.6] | 1.005 [0.991, 1.015] | same |
| embedded_rebar_tet4 | solve_step | t2 | 5 | 1 | 1.069 s [1.062 s, 1.073 s] | 1.069 s [1.062 s, 1.073 s] | 49146.0 [48840.8, 49348.8] | 0.998 [0.990, 1.012] | same |
| embedded_rebar_tet4 | solve_step | t4 | 5 | 1 | 1.064 s [1.062 s, 1.067 s] | 1.064 s [1.062 s, 1.067 s] | 48934.4 [48819.6, 49047.6] | 0.994 [0.990, 1.003] | same |
| embedded_rebar_tet4 | solve_step | t8 | 5 | 1 | 1.066 s [1.057 s, 1.073 s] | 1.066 s [1.057 s, 1.073 s] | 48989.1 [48602.2, 49327.9] | 0.997 [0.989, 1.005] | same |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 30.95 ms [30.74 ms, 31.45 ms] | 1.55 ms [1.54 ms, 1.57 ms] | 71.1 [70.7, 72.3] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | t1 | 5 | 20 | 31.66 ms [31.54 ms, 36.26 ms] | 1.58 ms [1.58 ms, 1.81 ms] | 72.8 [72.5, 83.4] | 1.029 [1.007, 1.172] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t2 | 5 | 20 | 20.71 ms [20.35 ms, 20.88 ms] | 1.04 ms [1.02 ms, 1.04 ms] | 47.6 [46.8, 48.0] | 0.670 [0.647, 0.675] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t4 | 5 | 20 | 17.40 ms [17.05 ms, 17.50 ms] | 869.8 µs [852.5 µs, 875.1 µs] | 40.0 [39.2, 40.2] | 0.562 [0.542, 0.569] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t8 | 5 | 20 | 15.94 ms [15.30 ms, 16.11 ms] | 797.2 µs [764.9 µs, 805.6 µs] | 36.7 [35.2, 37.0] | 0.515 [0.498, 0.522] | same |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 99.00 ms [97.95 ms, 99.03 ms] | 19.80 ms [19.59 ms, 19.81 ms] | 910.4 [900.7, 910.6] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | t1 | 5 | 5 | 90.87 ms [90.24 ms, 92.77 ms] | 18.17 ms [18.05 ms, 18.55 ms] | 835.5 [829.8, 853.1] | 0.923 [0.913, 0.937] | same |
| embedded_rebar_tet4 | assemble_stiffness | t2 | 5 | 5 | 55.40 ms [54.04 ms, 57.41 ms] | 11.08 ms [10.81 ms, 11.48 ms] | 509.4 [496.9, 528.0] | 0.566 [0.550, 0.580] | same |
| embedded_rebar_tet4 | assemble_stiffness | t4 | 5 | 5 | 38.24 ms [37.66 ms, 38.48 ms] | 7.65 ms [7.53 ms, 7.70 ms] | 351.6 [346.3, 353.8] | 0.388 [0.380, 0.390] | same |
| embedded_rebar_tet4 | assemble_stiffness | t8 | 5 | 5 | 28.66 ms [28.26 ms, 28.92 ms] | 5.73 ms [5.65 ms, 5.78 ms] | 263.6 [259.9, 265.9] | 0.291 [0.285, 0.294] | same |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 19.47 ms [19.12 ms, 19.93 ms] | 19.47 ms [19.12 ms, 19.93 ms] | 772.5 [758.8, 790.7] | – | same |
| embedded_rebar_tri3 | mesh | t1 | 5 | 1 | 19.29 ms [18.99 ms, 20.11 ms] | 19.29 ms [18.99 ms, 20.11 ms] | 765.6 [753.6, 797.9] | 0.993 [0.968, 1.033] | same |
| embedded_rebar_tri3 | mesh | t2 | 5 | 1 | 18.97 ms [18.66 ms, 19.77 ms] | 18.97 ms [18.66 ms, 19.77 ms] | 752.8 [740.3, 784.5] | 0.992 [0.944, 1.026] | same |
| embedded_rebar_tri3 | mesh | t4 | 5 | 1 | 19.42 ms [19.27 ms, 20.35 ms] | 19.42 ms [19.27 ms, 20.35 ms] | 770.6 [764.8, 807.4] | 0.993 [0.969, 1.060] | same |
| embedded_rebar_tri3 | mesh | t8 | 5 | 1 | 19.83 ms [19.33 ms, 20.01 ms] | 19.83 ms [19.33 ms, 20.01 ms] | 786.8 [767.0, 794.1] | 1.022 [0.993, 1.033] | same |
| embedded_rebar_tri3 | init | base | 5 | 1 | 28.60 ms [28.25 ms, 28.96 ms] | 28.60 ms [28.25 ms, 28.96 ms] | 1134.9 [1121.2, 1149.4] | – | same |
| embedded_rebar_tri3 | init | t1 | 5 | 1 | 28.38 ms [27.88 ms, 28.65 ms] | 28.38 ms [27.88 ms, 28.65 ms] | 1126.2 [1106.3, 1137.0] | 0.993 [0.971, 1.007] | same |
| embedded_rebar_tri3 | init | t2 | 5 | 1 | 30.14 ms [29.55 ms, 31.11 ms] | 30.14 ms [29.55 ms, 31.11 ms] | 1195.9 [1172.6, 1234.6] | 1.067 [1.033, 1.089] | same |
| embedded_rebar_tri3 | init | t4 | 5 | 1 | 30.58 ms [30.13 ms, 31.75 ms] | 30.58 ms [30.13 ms, 31.75 ms] | 1213.5 [1195.7, 1259.8] | 1.078 [1.054, 1.101] | same |
| embedded_rebar_tri3 | init | t8 | 5 | 1 | 31.08 ms [29.83 ms, 31.72 ms] | 31.08 ms [29.83 ms, 31.72 ms] | 1233.3 [1183.7, 1258.8] | 1.088 [1.056, 1.100] | same |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 68.68 ms [68.16 ms, 69.43 ms] | 13.74 ms [13.63 ms, 13.89 ms] | 545.1 [541.0, 551.0] | – | same |
| embedded_rebar_tri3 | intersect | t1 | 5 | 5 | 68.95 ms [68.19 ms, 69.41 ms] | 13.79 ms [13.64 ms, 13.88 ms] | 547.2 [541.2, 550.9] | 1.004 [0.982, 1.014] | same |
| embedded_rebar_tri3 | intersect | t2 | 5 | 5 | 70.83 ms [70.64 ms, 72.13 ms] | 14.17 ms [14.13 ms, 14.43 ms] | 562.2 [560.7, 572.4] | 1.038 [1.028, 1.041] | same |
| embedded_rebar_tri3 | intersect | t4 | 5 | 5 | 71.76 ms [70.79 ms, 72.35 ms] | 14.35 ms [14.16 ms, 14.47 ms] | 569.5 [561.8, 574.2] | 1.042 [1.029, 1.053] | same |
| embedded_rebar_tri3 | intersect | t8 | 5 | 5 | 70.92 ms [70.64 ms, 72.37 ms] | 14.18 ms [14.13 ms, 14.47 ms] | 562.9 [560.6, 574.4] | 1.035 [1.029, 1.042] | same |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 239.79 ms [236.16 ms, 241.64 ms] | 239.79 ms [236.16 ms, 241.64 ms] | 9515.3 [9371.4, 9588.9] | – | same |
| embedded_rebar_tri3 | solve_step | t1 | 5 | 1 | 238.62 ms [234.61 ms, 242.19 ms] | 238.62 ms [234.61 ms, 242.19 ms] | 9469.0 [9309.9, 9610.5] | 0.993 [0.989, 1.002] | same |
| embedded_rebar_tri3 | solve_step | t2 | 5 | 1 | 235.96 ms [232.53 ms, 240.53 ms] | 235.96 ms [232.53 ms, 240.53 ms] | 9363.4 [9227.5, 9544.7] | 0.993 [0.975, 1.001] | same |
| embedded_rebar_tri3 | solve_step | t4 | 5 | 1 | 235.27 ms [231.55 ms, 241.51 ms] | 235.27 ms [231.55 ms, 241.51 ms] | 9336.1 [9188.7, 9583.7] | 0.984 [0.975, 1.007] | same |
| embedded_rebar_tri3 | solve_step | t8 | 5 | 1 | 235.63 ms [231.04 ms, 241.93 ms] | 235.63 ms [231.04 ms, 241.93 ms] | 9350.4 [9168.3, 9600.4] | 0.978 [0.976, 1.002] | same |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 42.90 ms [42.59 ms, 44.47 ms] | 2.15 ms [2.13 ms, 2.22 ms] | 85.1 [84.5, 88.2] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | t1 | 5 | 20 | 43.48 ms [43.01 ms, 43.90 ms] | 2.17 ms [2.15 ms, 2.19 ms] | 86.3 [85.3, 87.1] | 1.003 [0.975, 1.027] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t2 | 5 | 20 | 37.45 ms [37.19 ms, 38.49 ms] | 1.87 ms [1.86 ms, 1.92 ms] | 74.3 [73.8, 76.4] | 0.870 [0.843, 0.897] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t4 | 5 | 20 | 36.54 ms [36.35 ms, 37.32 ms] | 1.83 ms [1.82 ms, 1.87 ms] | 72.5 [72.1, 74.1] | 0.847 [0.822, 0.876] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t8 | 5 | 20 | 41.13 ms [39.94 ms, 41.68 ms] | 2.06 ms [2.00 ms, 2.08 ms] | 81.6 [79.2, 82.7] | 0.965 [0.921, 0.976] | same |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 42.26 ms [42.06 ms, 42.77 ms] | 8.45 ms [8.41 ms, 8.55 ms] | 335.4 [333.8, 339.5] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | t1 | 5 | 5 | 36.46 ms [36.39 ms, 36.84 ms] | 7.29 ms [7.28 ms, 7.37 ms] | 289.3 [288.8, 292.4] | 0.867 [0.852, 0.870] | same |
| embedded_rebar_tri3 | assemble_stiffness | t2 | 5 | 5 | 29.48 ms [29.30 ms, 30.14 ms] | 5.90 ms [5.86 ms, 6.03 ms] | 234.0 [232.5, 239.2] | 0.696 [0.689, 0.717] | same |
| embedded_rebar_tri3 | assemble_stiffness | t4 | 5 | 5 | 26.79 ms [26.38 ms, 27.20 ms] | 5.36 ms [5.28 ms, 5.44 ms] | 212.6 [209.4, 215.9] | 0.636 [0.617, 0.647] | same |
| embedded_rebar_tri3 | assemble_stiffness | t8 | 5 | 5 | 27.91 ms [27.24 ms, 28.11 ms] | 5.58 ms [5.45 ms, 5.62 ms] | 221.5 [216.2, 223.1] | 0.656 [0.645, 0.668] | same |
| heat_implicit_hex8_kvar | mesh | base | 5 | 1 | 3.62 ms [3.47 ms, 3.76 ms] | 3.62 ms [3.47 ms, 3.76 ms] | 2093.2 [2010.4, 2176.5] | – | same |
| heat_implicit_hex8_kvar | mesh | t1 | 5 | 1 | 3.73 ms [3.55 ms, 3.87 ms] | 3.73 ms [3.55 ms, 3.87 ms] | 2156.2 [2056.1, 2240.2] | 1.030 [0.946, 1.103] | same |
| heat_implicit_hex8_kvar | mesh | t2 | 5 | 1 | 3.71 ms [3.49 ms, 3.78 ms] | 3.71 ms [3.49 ms, 3.78 ms] | 2149.9 [2019.1, 2185.2] | 1.019 [0.945, 1.084] | same |
| heat_implicit_hex8_kvar | mesh | t4 | 5 | 1 | 3.70 ms [3.49 ms, 3.79 ms] | 3.70 ms [3.49 ms, 3.79 ms] | 2142.4 [2020.3, 2193.3] | 1.023 [0.928, 1.054] | same |
| heat_implicit_hex8_kvar | mesh | t8 | 5 | 1 | 3.53 ms [3.49 ms, 3.82 ms] | 3.53 ms [3.49 ms, 3.82 ms] | 2041.1 [2019.7, 2208.9] | 1.012 [0.928, 1.055] | same |
| heat_implicit_hex8_kvar | init | base | 5 | 1 | 782.0 µs [708.0 µs, 802.0 µs] | 782.0 µs [708.0 µs, 802.0 µs] | 452.5 [409.7, 464.1] | – | same |
| heat_implicit_hex8_kvar | init | t1 | 5 | 1 | 753.0 µs [716.0 µs, 796.0 µs] | 753.0 µs [716.0 µs, 796.0 µs] | 435.8 [414.4, 460.6] | 1.003 [0.893, 1.079] | same |
| heat_implicit_hex8_kvar | init | t2 | 5 | 1 | 741.0 µs [716.0 µs, 782.0 µs] | 741.0 µs [716.0 µs, 782.0 µs] | 428.8 [414.4, 452.5] | 0.970 [0.924, 1.105] | same |
| heat_implicit_hex8_kvar | init | t4 | 5 | 1 | 734.0 µs [690.0 µs, 857.0 µs] | 734.0 µs [690.0 µs, 857.0 µs] | 424.8 [399.3, 495.9] | 0.988 [0.860, 1.096] | same |
| heat_implicit_hex8_kvar | init | t8 | 5 | 1 | 755.0 µs [698.0 µs, 859.0 µs] | 755.0 µs [698.0 µs, 859.0 µs] | 436.9 [403.9, 497.1] | 1.023 [0.870, 1.085] | same |
| heat_implicit_hex8_kvar | solve_step | base | 5 | 5 | 586.82 ms [584.47 ms, 593.20 ms] | 117.36 ms [116.89 ms, 118.64 ms] | 67918.5 [67647.5, 68657.1] | – | same |
| heat_implicit_hex8_kvar | solve_step | t1 | 5 | 5 | 588.03 ms [586.41 ms, 590.86 ms] | 117.61 ms [117.28 ms, 118.17 ms] | 68058.7 [67871.3, 68386.8] | 1.000 [0.989, 1.008] | same |
| heat_implicit_hex8_kvar | solve_step | t2 | 5 | 5 | 564.87 ms [564.17 ms, 571.48 ms] | 112.97 ms [112.83 ms, 114.30 ms] | 65378.9 [65297.8, 66143.3] | 0.961 [0.954, 0.975] | same |
| heat_implicit_hex8_kvar | solve_step | t4 | 5 | 5 | 557.26 ms [556.15 ms, 561.64 ms] | 111.45 ms [111.23 ms, 112.33 ms] | 64497.7 [64369.6, 65005.1] | 0.950 [0.946, 0.958] | same |
| heat_implicit_hex8_kvar | solve_step | t8 | 5 | 5 | 555.55 ms [550.03 ms, 566.90 ms] | 111.11 ms [110.01 ms, 113.38 ms] | 64299.5 [63660.5, 65614.0] | 0.937 [0.935, 0.970] | same |
| heat_static_tri6 | mesh | base | 5 | 1 | 39.19 ms [38.37 ms, 39.38 ms] | 39.19 ms [38.37 ms, 39.38 ms] | 1619.5 [1585.6, 1627.3] | – | same |
| heat_static_tri6 | mesh | t1 | 5 | 1 | 38.11 ms [37.99 ms, 38.59 ms] | 38.11 ms [37.99 ms, 38.59 ms] | 1574.8 [1570.0, 1594.8] | 0.978 [0.966, 0.992] | same |
| heat_static_tri6 | mesh | t2 | 5 | 1 | 38.64 ms [38.21 ms, 39.08 ms] | 38.64 ms [38.21 ms, 39.08 ms] | 1596.5 [1579.1, 1615.0] | 0.997 [0.970, 1.006] | same |
| heat_static_tri6 | mesh | t4 | 5 | 1 | 38.68 ms [37.87 ms, 39.03 ms] | 38.68 ms [37.87 ms, 39.03 ms] | 1598.4 [1564.8, 1612.6] | 0.989 [0.973, 1.004] | same |
| heat_static_tri6 | mesh | t8 | 5 | 1 | 38.59 ms [38.38 ms, 40.64 ms] | 38.59 ms [38.38 ms, 40.64 ms] | 1594.6 [1585.9, 1679.3] | 1.005 [0.980, 1.037] | same |
| heat_static_tri6 | init | base | 5 | 1 | 4.85 ms [4.79 ms, 4.97 ms] | 4.85 ms [4.79 ms, 4.97 ms] | 200.5 [197.9, 205.4] | – | same |
| heat_static_tri6 | init | t1 | 5 | 1 | 4.59 ms [4.55 ms, 4.78 ms] | 4.59 ms [4.55 ms, 4.78 ms] | 189.5 [188.0, 197.4] | 0.954 [0.915, 0.984] | same |
| heat_static_tri6 | init | t2 | 5 | 1 | 4.62 ms [4.24 ms, 4.83 ms] | 4.62 ms [4.24 ms, 4.83 ms] | 190.9 [175.3, 199.7] | 0.945 [0.877, 0.996] | same |
| heat_static_tri6 | init | t4 | 5 | 1 | 4.77 ms [4.55 ms, 4.79 ms] | 4.77 ms [4.55 ms, 4.79 ms] | 197.3 [187.8, 197.9] | 0.979 [0.914, 1.000] | same |
| heat_static_tri6 | init | t8 | 5 | 1 | 4.79 ms [4.67 ms, 4.94 ms] | 4.79 ms [4.67 ms, 4.94 ms] | 197.9 [193.1, 204.3] | 0.978 [0.968, 0.995] | same |
| heat_static_tri6 | assemble_conductivity | base | 5 | 5 | 38.51 ms [38.39 ms, 39.90 ms] | 7.70 ms [7.68 ms, 7.98 ms] | 318.2 [317.3, 329.8] | – | same |
| heat_static_tri6 | assemble_conductivity | t1 | 5 | 5 | 32.00 ms [31.81 ms, 32.34 ms] | 6.40 ms [6.36 ms, 6.47 ms] | 264.4 [262.9, 267.3] | 0.831 [0.811, 0.834] | same |
| heat_static_tri6 | assemble_conductivity | t2 | 5 | 5 | 19.92 ms [19.79 ms, 20.12 ms] | 3.98 ms [3.96 ms, 4.02 ms] | 164.7 [163.5, 166.2] | 0.515 [0.498, 0.524] | same |
| heat_static_tri6 | assemble_conductivity | t4 | 5 | 5 | 11.65 ms [11.48 ms, 12.19 ms] | 2.33 ms [2.30 ms, 2.44 ms] | 96.3 [94.9, 100.7] | 0.303 [0.296, 0.305] | same |
| heat_static_tri6 | assemble_conductivity | t8 | 5 | 5 | 7.58 ms [7.27 ms, 7.64 ms] | 1.52 ms [1.45 ms, 1.53 ms] | 62.6 [60.1, 63.1] | 0.191 [0.189, 0.197] | same |
| heat_static_tri6 | solve_step | base | 5 | 1 | 1.974 s [1.938 s, 1.987 s] | 1.974 s [1.938 s, 1.987 s] | 81573.7 [80089.8, 82097.1] | – | same |
| heat_static_tri6 | solve_step | t1 | 5 | 1 | 1.945 s [1.943 s, 1.981 s] | 1.945 s [1.943 s, 1.981 s] | 80374.5 [80288.9, 81876.3] | 0.997 [0.984, 1.003] | same |
| heat_static_tri6 | solve_step | t2 | 5 | 1 | 1.954 s [1.940 s, 1.962 s] | 1.954 s [1.940 s, 1.962 s] | 80753.6 [80156.5, 81072.6] | 0.990 [0.983, 1.008] | same |
| heat_static_tri6 | solve_step | t4 | 5 | 1 | 1.947 s [1.927 s, 1.973 s] | 1.947 s [1.927 s, 1.973 s] | 80446.4 [79624.6, 81509.2] | 0.994 [0.982, 0.999] | same |
| heat_static_tri6 | solve_step | t8 | 5 | 1 | 1.940 s [1.926 s, 1.962 s] | 1.940 s [1.926 s, 1.962 s] | 80176.5 [79580.7, 81081.2] | 0.992 [0.977, 0.996] | same |
| phasefield_staggered_tet4 | mesh | base | 5 | 1 | 6.57 ms [6.29 ms, 6.64 ms] | 6.57 ms [6.29 ms, 6.64 ms] | 1095.7 [1048.5, 1106.2] | – | same |
| phasefield_staggered_tet4 | mesh | t1 | 5 | 1 | 6.06 ms [6.02 ms, 6.41 ms] | 6.06 ms [6.02 ms, 6.41 ms] | 1010.0 [1002.7, 1068.8] | 0.957 [0.922, 0.974] | same |
| phasefield_staggered_tet4 | mesh | t2 | 5 | 1 | 6.28 ms [6.02 ms, 6.55 ms] | 6.28 ms [6.02 ms, 6.55 ms] | 1045.8 [1003.8, 1091.5] | 0.994 [0.914, 0.997] | same |
| phasefield_staggered_tet4 | mesh | t4 | 5 | 1 | 6.35 ms [6.30 ms, 6.71 ms] | 6.35 ms [6.30 ms, 6.71 ms] | 1058.7 [1050.8, 1118.0] | 0.992 [0.960, 1.019] | same |
| phasefield_staggered_tet4 | mesh | t8 | 5 | 1 | 6.54 ms [6.23 ms, 6.60 ms] | 6.54 ms [6.23 ms, 6.60 ms] | 1089.2 [1038.0, 1099.7] | 0.994 [0.938, 1.039] | same |
| phasefield_staggered_tet4 | init | base | 5 | 1 | 2.38 ms [2.30 ms, 2.52 ms] | 2.38 ms [2.30 ms, 2.52 ms] | 397.5 [383.3, 419.8] | – | same |
| phasefield_staggered_tet4 | init | t1 | 5 | 1 | 2.10 ms [2.07 ms, 2.44 ms] | 2.10 ms [2.07 ms, 2.44 ms] | 350.8 [345.0, 407.2] | 0.892 [0.822, 1.062] | same |
| phasefield_staggered_tet4 | init | t2 | 5 | 1 | 2.12 ms [2.00 ms, 2.47 ms] | 2.12 ms [2.00 ms, 2.47 ms] | 354.2 [333.5, 412.3] | 0.877 [0.844, 1.035] | same |
| phasefield_staggered_tet4 | init | t4 | 5 | 1 | 2.34 ms [2.21 ms, 2.53 ms] | 2.34 ms [2.21 ms, 2.53 ms] | 390.8 [367.7, 422.3] | 0.988 [0.876, 1.062] | same |
| phasefield_staggered_tet4 | init | t8 | 5 | 1 | 2.33 ms [2.17 ms, 2.45 ms] | 2.33 ms [2.17 ms, 2.45 ms] | 388.2 [362.5, 409.0] | 0.961 [0.925, 1.026] | same |
| phasefield_staggered_tet4 | solid_solve | base | 5 | 4 | 585.59 ms [583.22 ms, 604.48 ms] | 146.40 ms [145.80 ms, 151.12 ms] | 24399.5 [24300.7, 25186.6] | – | same |
| phasefield_staggered_tet4 | solid_solve | t1 | 5 | 4 | 576.32 ms [571.94 ms, 586.43 ms] | 144.08 ms [142.98 ms, 146.61 ms] | 24013.3 [23830.8, 24434.7] | 0.981 [0.970, 0.988] | same |
| phasefield_staggered_tet4 | solid_solve | t2 | 5 | 4 | 575.00 ms [573.15 ms, 580.11 ms] | 143.75 ms [143.29 ms, 145.03 ms] | 23958.4 [23881.4, 24171.2] | 0.982 [0.960, 0.985] | same |
| phasefield_staggered_tet4 | solid_solve | t4 | 5 | 4 | 568.41 ms [567.43 ms, 570.54 ms] | 142.10 ms [141.86 ms, 142.64 ms] | 23683.9 [23642.8, 23772.6] | 0.972 [0.940, 0.973] | same |
| phasefield_staggered_tet4 | solid_solve | t8 | 5 | 4 | 569.18 ms [562.80 ms, 572.97 ms] | 142.30 ms [140.70 ms, 143.24 ms] | 23715.9 [23449.8, 23873.8] | 0.966 [0.931, 0.978] | same |
| phasefield_staggered_tet4 | strain_transfer | base | 5 | 4 | 762.0 µs [731.0 µs, 789.0 µs] | 190.5 µs [182.8 µs, 197.2 µs] | 31.7 [30.5, 32.9] | – | same |
| phasefield_staggered_tet4 | strain_transfer | t1 | 5 | 4 | 765.0 µs [714.0 µs, 806.0 µs] | 191.2 µs [178.5 µs, 201.5 µs] | 31.9 [29.8, 33.6] | 0.992 [0.933, 1.058] | same |
| phasefield_staggered_tet4 | strain_transfer | t2 | 5 | 4 | 757.0 µs [748.0 µs, 787.0 µs] | 189.2 µs [187.0 µs, 196.8 µs] | 31.5 [31.2, 32.8] | 1.005 [0.978, 1.026] | same |
| phasefield_staggered_tet4 | strain_transfer | t4 | 5 | 4 | 769.0 µs [732.0 µs, 793.0 µs] | 192.2 µs [183.0 µs, 198.2 µs] | 32.0 [30.5, 33.0] | 1.025 [0.957, 1.041] | same |
| phasefield_staggered_tet4 | strain_transfer | t8 | 5 | 4 | 803.0 µs [768.0 µs, 824.0 µs] | 200.8 µs [192.0 µs, 206.0 µs] | 33.5 [32.0, 34.3] | 1.071 [0.973, 1.105] | same |
| phasefield_staggered_tet4 | phase_solve | base | 5 | 4 | 70.54 ms [69.83 ms, 72.80 ms] | 17.63 ms [17.46 ms, 18.20 ms] | 2939.1 [2909.5, 3033.3] | – | same |
| phasefield_staggered_tet4 | phase_solve | t1 | 5 | 4 | 64.45 ms [64.01 ms, 65.50 ms] | 16.11 ms [16.00 ms, 16.37 ms] | 2685.6 [2667.1, 2729.1] | 0.910 [0.895, 0.923] | same |
| phasefield_staggered_tet4 | phase_solve | t2 | 5 | 4 | 64.72 ms [64.00 ms, 65.39 ms] | 16.18 ms [16.00 ms, 16.35 ms] | 2696.8 [2666.7, 2724.6] | 0.917 [0.885, 0.927] | same |
| phasefield_staggered_tet4 | phase_solve | t4 | 5 | 4 | 64.88 ms [64.04 ms, 66.37 ms] | 16.22 ms [16.01 ms, 16.59 ms] | 2703.2 [2668.5, 2765.6] | 0.912 [0.908, 0.922] | same |
| phasefield_staggered_tet4 | phase_solve | t8 | 5 | 4 | 65.42 ms [64.98 ms, 65.73 ms] | 16.36 ms [16.25 ms, 16.43 ms] | 2726.0 [2707.6, 2739.0] | 0.921 [0.903, 0.937] | same |
| phasefield_staggered_tet4 | damage_transfer | base | 5 | 4 | 1.41 ms [1.36 ms, 1.45 ms] | 352.5 µs [341.0 µs, 362.5 µs] | 58.8 [56.8, 60.4] | – | same |
| phasefield_staggered_tet4 | damage_transfer | t1 | 5 | 4 | 1.43 ms [1.37 ms, 1.49 ms] | 357.5 µs [343.2 µs, 371.5 µs] | 59.6 [57.2, 61.9] | 0.989 [0.981, 1.073] | same |
| phasefield_staggered_tet4 | damage_transfer | t2 | 5 | 4 | 1.43 ms [1.39 ms, 1.47 ms] | 357.8 µs [347.5 µs, 367.5 µs] | 59.6 [57.9, 61.2] | 1.006 [0.959, 1.054] | same |
| phasefield_staggered_tet4 | damage_transfer | t4 | 5 | 4 | 1.44 ms [1.42 ms, 1.49 ms] | 359.0 µs [355.2 µs, 371.5 µs] | 59.8 [59.2, 61.9] | 1.021 [0.992, 1.054] | same |
| phasefield_staggered_tet4 | damage_transfer | t8 | 5 | 4 | 1.45 ms [1.43 ms, 1.51 ms] | 361.5 µs [358.0 µs, 377.5 µs] | 60.2 [59.7, 62.9] | 1.041 [1.017, 1.050] | same |
| phasefield_staggered_tet4 | internal_forces | base | 5 | 4 | 1.68 ms [1.65 ms, 1.70 ms] | 418.8 µs [412.5 µs, 424.8 µs] | 69.8 [68.8, 70.8] | – | same |
| phasefield_staggered_tet4 | internal_forces | t1 | 5 | 4 | 1.67 ms [1.62 ms, 1.68 ms] | 417.0 µs [406.0 µs, 421.0 µs] | 69.5 [67.7, 70.2] | 0.996 [0.963, 1.011] | same |
| phasefield_staggered_tet4 | internal_forces | t2 | 5 | 4 | 1.19 ms [1.18 ms, 1.24 ms] | 297.8 µs [294.8 µs, 309.8 µs] | 49.6 [49.1, 51.6] | 0.716 [0.697, 0.740] | same |
| phasefield_staggered_tet4 | internal_forces | t4 | 5 | 4 | 1.20 ms [1.17 ms, 1.26 ms] | 300.5 µs [292.5 µs, 315.5 µs] | 50.1 [48.8, 52.6] | 0.719 [0.692, 0.753] | same |
| phasefield_staggered_tet4 | internal_forces | t8 | 5 | 4 | 1.19 ms [1.19 ms, 1.22 ms] | 296.8 µs [296.2 µs, 306.2 µs] | 49.5 [49.4, 51.0] | 0.719 [0.701, 0.721] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base | 5 | 5 | 24.91 ms [24.83 ms, 25.35 ms] | 4.98 ms [4.97 ms, 5.07 ms] | 830.2 [827.6, 844.9] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t1 | 5 | 5 | 23.31 ms [23.12 ms, 24.31 ms] | 4.66 ms [4.62 ms, 4.86 ms] | 777.0 [770.6, 810.5] | 0.939 [0.913, 0.964] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t2 | 5 | 5 | 12.47 ms [12.32 ms, 12.80 ms] | 2.49 ms [2.46 ms, 2.56 ms] | 415.5 [410.6, 426.6] | 0.495 [0.491, 0.515] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t4 | 5 | 5 | 8.40 ms [8.16 ms, 8.57 ms] | 1.68 ms [1.63 ms, 1.71 ms] | 280.1 [271.9, 285.7] | 0.335 [0.328, 0.345] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | t8 | 5 | 5 | 5.86 ms [5.51 ms, 5.93 ms] | 1.17 ms [1.10 ms, 1.19 ms] | 195.3 [183.6, 197.6] | 0.235 [0.221, 0.238] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base | 5 | 5 | 7.96 ms [7.92 ms, 8.31 ms] | 1.59 ms [1.58 ms, 1.66 ms] | 265.5 [264.0, 276.9] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t1 | 5 | 5 | 4.63 ms [4.58 ms, 4.90 ms] | 926.2 µs [915.0 µs, 979.0 µs] | 154.4 [152.5, 163.2] | 0.582 [0.558, 0.617] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t2 | 5 | 5 | 4.63 ms [4.62 ms, 4.87 ms] | 925.2 µs [923.0 µs, 974.6 µs] | 154.2 [153.8, 162.4] | 0.583 [0.567, 0.612] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t4 | 5 | 5 | 4.74 ms [4.72 ms, 5.00 ms] | 948.2 µs [944.4 µs, 1.00 ms] | 158.0 [157.4, 166.8] | 0.598 [0.591, 0.603] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | t8 | 5 | 5 | 4.85 ms [4.82 ms, 4.89 ms] | 970.4 µs [964.0 µs, 978.6 µs] | 161.7 [160.7, 163.1] | 0.610 [0.584, 0.615] | same |
| phasefield_staggered_tri3 | mesh | base | 5 | 1 | 39.66 ms [39.16 ms, 40.30 ms] | 39.66 ms [39.16 ms, 40.30 ms] | 774.5 [764.9, 787.1] | – | same |
| phasefield_staggered_tri3 | mesh | t1 | 5 | 1 | 39.81 ms [39.58 ms, 41.22 ms] | 39.81 ms [39.58 ms, 41.22 ms] | 777.6 [773.1, 805.1] | 1.015 [0.988, 1.026] | same |
| phasefield_staggered_tri3 | mesh | t2 | 5 | 1 | 38.92 ms [38.54 ms, 40.61 ms] | 38.92 ms [38.54 ms, 40.61 ms] | 760.2 [752.7, 793.1] | 0.984 [0.966, 1.024] | same |
| phasefield_staggered_tri3 | mesh | t4 | 5 | 1 | 40.34 ms [39.30 ms, 41.49 ms] | 40.34 ms [39.30 ms, 41.49 ms] | 787.9 [767.6, 810.4] | 1.003 [0.993, 1.059] | same |
| phasefield_staggered_tri3 | mesh | t8 | 5 | 1 | 40.03 ms [39.59 ms, 41.59 ms] | 40.03 ms [39.59 ms, 41.59 ms] | 781.9 [773.2, 812.2] | 1.017 [0.995, 1.035] | same |
| phasefield_staggered_tri3 | init | base | 5 | 1 | 16.59 ms [16.34 ms, 16.68 ms] | 16.59 ms [16.34 ms, 16.68 ms] | 323.9 [319.1, 325.9] | – | same |
| phasefield_staggered_tri3 | init | t1 | 5 | 1 | 16.55 ms [16.25 ms, 16.76 ms] | 16.55 ms [16.25 ms, 16.76 ms] | 323.3 [317.5, 327.3] | 1.006 [0.974, 1.024] | same |
| phasefield_staggered_tri3 | init | t2 | 5 | 1 | 16.16 ms [16.02 ms, 16.71 ms] | 16.16 ms [16.02 ms, 16.71 ms] | 315.6 [313.0, 326.3] | 0.974 [0.969, 1.023] | same |
| phasefield_staggered_tri3 | init | t4 | 5 | 1 | 16.29 ms [16.22 ms, 16.78 ms] | 16.29 ms [16.22 ms, 16.78 ms] | 318.2 [316.9, 327.7] | 0.997 [0.976, 1.010] | same |
| phasefield_staggered_tri3 | init | t8 | 5 | 1 | 16.24 ms [16.12 ms, 16.76 ms] | 16.24 ms [16.12 ms, 16.76 ms] | 317.3 [314.8, 327.3] | 0.987 [0.966, 1.010] | same |
| phasefield_staggered_tri3 | solid_solve | base | 5 | 4 | 3.218 s [3.212 s, 3.223 s] | 804.61 ms [802.89 ms, 805.70 ms] | 15715.0 [15681.5, 15736.4] | – | same |
| phasefield_staggered_tri3 | solid_solve | t1 | 5 | 4 | 3.271 s [3.257 s, 3.286 s] | 817.73 ms [814.25 ms, 821.53 ms] | 15971.4 [15903.3, 16045.5] | 1.016 [1.011, 1.022] | same |
| phasefield_staggered_tri3 | solid_solve | t2 | 5 | 4 | 3.163 s [3.150 s, 3.185 s] | 790.64 ms [787.48 ms, 796.14 ms] | 15442.2 [15380.5, 15549.5] | 0.985 [0.978, 0.990] | same |
| phasefield_staggered_tri3 | solid_solve | t4 | 5 | 4 | 3.140 s [3.128 s, 3.164 s] | 784.99 ms [781.98 ms, 791.07 ms] | 15331.8 [15273.0, 15450.6] | 0.974 [0.971, 0.984] | same |
| phasefield_staggered_tri3 | solid_solve | t8 | 5 | 4 | 3.139 s [3.130 s, 3.160 s] | 784.84 ms [782.57 ms, 789.96 ms] | 15328.8 [15284.5, 15428.9] | 0.978 [0.971, 0.982] | same |
| phasefield_staggered_tri3 | strain_transfer | base | 5 | 4 | 4.21 ms [4.20 ms, 4.31 ms] | 1.05 ms [1.05 ms, 1.08 ms] | 20.5 [20.5, 21.0] | – | same |
| phasefield_staggered_tri3 | strain_transfer | t1 | 5 | 4 | 4.48 ms [4.43 ms, 4.52 ms] | 1.12 ms [1.11 ms, 1.13 ms] | 21.9 [21.7, 22.1] | 1.061 [1.038, 1.075] | same |
| phasefield_staggered_tri3 | strain_transfer | t2 | 5 | 4 | 4.50 ms [4.44 ms, 4.64 ms] | 1.12 ms [1.11 ms, 1.16 ms] | 22.0 [21.7, 22.7] | 1.065 [1.030, 1.104] | same |
| phasefield_staggered_tri3 | strain_transfer | t4 | 5 | 4 | 4.51 ms [4.39 ms, 4.64 ms] | 1.13 ms [1.10 ms, 1.16 ms] | 22.0 [21.4, 22.6] | 1.058 [1.045, 1.103] | same |
| phasefield_staggered_tri3 | strain_transfer | t8 | 5 | 4 | 4.62 ms [4.55 ms, 4.89 ms] | 1.16 ms [1.14 ms, 1.22 ms] | 22.6 [22.2, 23.9] | 1.100 [1.073, 1.145] | same |
| phasefield_staggered_tri3 | phase_solve | base | 5 | 4 | 863.10 ms [860.97 ms, 867.01 ms] | 215.78 ms [215.24 ms, 216.75 ms] | 4214.4 [4204.0, 4233.4] | – | same |
| phasefield_staggered_tri3 | phase_solve | t1 | 5 | 4 | 845.11 ms [841.25 ms, 847.37 ms] | 211.28 ms [210.31 ms, 211.84 ms] | 4126.5 [4107.7, 4137.6] | 0.978 [0.970, 0.983] | same |
| phasefield_staggered_tri3 | phase_solve | t2 | 5 | 4 | 810.56 ms [807.54 ms, 814.95 ms] | 202.64 ms [201.89 ms, 203.74 ms] | 3957.8 [3943.1, 3979.2] | 0.939 [0.933, 0.943] | same |
| phasefield_staggered_tri3 | phase_solve | t4 | 5 | 4 | 795.53 ms [794.42 ms, 798.56 ms] | 198.88 ms [198.61 ms, 199.64 ms] | 3884.4 [3879.0, 3899.2] | 0.923 [0.917, 0.925] | same |
| phasefield_staggered_tri3 | phase_solve | t8 | 5 | 4 | 794.44 ms [791.58 ms, 795.67 ms] | 198.61 ms [197.89 ms, 198.92 ms] | 3879.1 [3865.1, 3885.1] | 0.918 [0.916, 0.924] | same |
| phasefield_staggered_tri3 | damage_transfer | base | 5 | 4 | 10.56 ms [10.50 ms, 10.65 ms] | 2.64 ms [2.63 ms, 2.66 ms] | 51.5 [51.3, 52.0] | – | same |
| phasefield_staggered_tri3 | damage_transfer | t1 | 5 | 4 | 10.55 ms [10.45 ms, 10.65 ms] | 2.64 ms [2.61 ms, 2.66 ms] | 51.5 [51.0, 52.0] | 0.994 [0.992, 1.014] | same |
| phasefield_staggered_tri3 | damage_transfer | t2 | 5 | 4 | 10.76 ms [10.52 ms, 10.88 ms] | 2.69 ms [2.63 ms, 2.72 ms] | 52.5 [51.4, 53.1] | 1.012 [0.996, 1.035] | same |
| phasefield_staggered_tri3 | damage_transfer | t4 | 5 | 4 | 10.69 ms [10.45 ms, 10.88 ms] | 2.67 ms [2.61 ms, 2.72 ms] | 52.2 [51.0, 53.1] | 1.015 [0.989, 1.021] | same |
| phasefield_staggered_tri3 | damage_transfer | t8 | 5 | 4 | 10.94 ms [10.90 ms, 11.15 ms] | 2.73 ms [2.73 ms, 2.79 ms] | 53.4 [53.2, 54.5] | 1.039 [1.033, 1.047] | same |
| phasefield_staggered_tri3 | internal_forces | base | 5 | 4 | 8.65 ms [8.37 ms, 8.75 ms] | 2.16 ms [2.09 ms, 2.19 ms] | 42.3 [40.9, 42.7] | – | same |
| phasefield_staggered_tri3 | internal_forces | t1 | 5 | 4 | 8.76 ms [8.46 ms, 8.83 ms] | 2.19 ms [2.12 ms, 2.21 ms] | 42.8 [41.3, 43.1] | 1.011 [1.006, 1.039] | same |
| phasefield_staggered_tri3 | internal_forces | t2 | 5 | 4 | 5.68 ms [5.62 ms, 5.86 ms] | 1.42 ms [1.41 ms, 1.47 ms] | 27.8 [27.5, 28.6] | 0.673 [0.646, 0.679] | same |
| phasefield_staggered_tri3 | internal_forces | t4 | 5 | 4 | 4.52 ms [4.48 ms, 4.59 ms] | 1.13 ms [1.12 ms, 1.15 ms] | 22.1 [21.9, 22.4] | 0.528 [0.515, 0.540] | same |
| phasefield_staggered_tri3 | internal_forces | t8 | 5 | 4 | 4.09 ms [4.07 ms, 4.21 ms] | 1.02 ms [1.02 ms, 1.05 ms] | 20.0 [19.9, 20.6] | 0.481 [0.468, 0.486] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base | 5 | 5 | 59.17 ms [58.95 ms, 59.28 ms] | 11.83 ms [11.79 ms, 11.86 ms] | 231.1 [230.3, 231.6] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t1 | 5 | 5 | 44.57 ms [44.46 ms, 44.81 ms] | 8.91 ms [8.89 ms, 8.96 ms] | 174.1 [173.7, 175.0] | 0.753 [0.753, 0.757] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t2 | 5 | 5 | 25.61 ms [25.38 ms, 25.68 ms] | 5.12 ms [5.08 ms, 5.14 ms] | 100.0 [99.1, 100.3] | 0.433 [0.428, 0.435] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t4 | 5 | 5 | 15.95 ms [15.68 ms, 16.17 ms] | 3.19 ms [3.14 ms, 3.23 ms] | 62.3 [61.2, 63.2] | 0.270 [0.265, 0.273] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | t8 | 5 | 5 | 11.81 ms [11.68 ms, 11.92 ms] | 2.36 ms [2.34 ms, 2.38 ms] | 46.1 [45.6, 46.5] | 0.199 [0.198, 0.201] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base | 5 | 5 | 57.14 ms [56.47 ms, 59.31 ms] | 11.43 ms [11.29 ms, 11.86 ms] | 223.2 [220.6, 231.7] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t1 | 5 | 5 | 32.59 ms [32.37 ms, 33.71 ms] | 6.52 ms [6.47 ms, 6.74 ms] | 127.3 [126.4, 131.7] | 0.568 [0.566, 0.575] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t2 | 5 | 5 | 21.01 ms [20.87 ms, 21.59 ms] | 4.20 ms [4.17 ms, 4.32 ms] | 82.1 [81.5, 84.3] | 0.370 [0.354, 0.375] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t4 | 5 | 5 | 13.59 ms [13.39 ms, 14.49 ms] | 2.72 ms [2.68 ms, 2.90 ms] | 53.1 [52.3, 56.6] | 0.238 [0.226, 0.257] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | t8 | 5 | 5 | 10.39 ms [9.99 ms, 10.70 ms] | 2.08 ms [2.00 ms, 2.14 ms] | 40.6 [39.0, 41.8] | 0.184 [0.168, 0.187] | same |
| structural_beam3_frame_dynamic | mesh | base | 5 | 1 | 280.0 µs [267.0 µs, 290.0 µs] | 280.0 µs [267.0 µs, 290.0 µs] | 46.0 [43.9, 47.7] | – | same |
| structural_beam3_frame_dynamic | mesh | t1 | 5 | 1 | 259.0 µs [234.0 µs, 277.0 µs] | 259.0 µs [234.0 µs, 277.0 µs] | 42.6 [38.5, 45.5] | 0.912 [0.807, 1.037] | same |
| structural_beam3_frame_dynamic | mesh | t2 | 5 | 1 | 275.0 µs [264.0 µs, 475.0 µs] | 275.0 µs [264.0 µs, 475.0 µs] | 45.2 [43.4, 78.1] | 1.026 [0.910, 1.673] | same |
| structural_beam3_frame_dynamic | mesh | t4 | 5 | 1 | 276.0 µs [262.0 µs, 279.0 µs] | 276.0 µs [262.0 µs, 279.0 µs] | 45.4 [43.1, 45.9] | 0.981 [0.962, 1.019] | same |
| structural_beam3_frame_dynamic | mesh | t8 | 5 | 1 | 282.0 µs [265.0 µs, 291.0 µs] | 282.0 µs [265.0 µs, 291.0 µs] | 46.4 [43.6, 47.8] | 1.003 [0.933, 1.056] | same |
| structural_beam3_frame_dynamic | init | base | 5 | 1 | 27.57 ms [26.49 ms, 28.15 ms] | 27.57 ms [26.49 ms, 28.15 ms] | 4532.4 [4354.4, 4627.5] | – | same |
| structural_beam3_frame_dynamic | init | t1 | 5 | 1 | 26.97 ms [26.45 ms, 27.82 ms] | 26.97 ms [26.45 ms, 27.82 ms] | 4432.3 [4347.6, 4572.5] | 0.966 [0.948, 1.050] | same |
| structural_beam3_frame_dynamic | init | t2 | 5 | 1 | 26.60 ms [26.43 ms, 27.85 ms] | 26.60 ms [26.43 ms, 27.85 ms] | 4371.5 [4345.0, 4577.4] | 0.965 [0.949, 1.010] | same |
| structural_beam3_frame_dynamic | init | t4 | 5 | 1 | 26.99 ms [26.71 ms, 27.29 ms] | 26.99 ms [26.71 ms, 27.29 ms] | 4436.2 [4390.9, 4486.4] | 0.970 [0.959, 1.016] | same |
| structural_beam3_frame_dynamic | init | t8 | 5 | 1 | 27.90 ms [26.70 ms, 28.11 ms] | 27.90 ms [26.70 ms, 28.11 ms] | 4585.8 [4387.9, 4620.6] | 1.008 [0.984, 1.019] | same |
| structural_beam3_frame_dynamic | solve_step | base | 5 | 60 | 2.870 s [2.861 s, 2.943 s] | 47.84 ms [47.69 ms, 49.05 ms] | 7863.3 [7838.7, 8062.2] | – | same |
| structural_beam3_frame_dynamic | solve_step | t1 | 5 | 60 | 2.815 s [2.796 s, 2.918 s] | 46.91 ms [46.60 ms, 48.63 ms] | 7711.1 [7660.2, 7993.4] | 0.984 [0.976, 0.991] | same |
| structural_beam3_frame_dynamic | solve_step | t2 | 5 | 60 | 2.794 s [2.788 s, 2.891 s] | 46.57 ms [46.47 ms, 48.19 ms] | 7654.0 [7637.6, 7920.8] | 0.976 [0.972, 0.982] | same |
| structural_beam3_frame_dynamic | solve_step | t4 | 5 | 60 | 2.793 s [2.783 s, 2.875 s] | 46.55 ms [46.39 ms, 47.91 ms] | 7651.1 [7624.2, 7875.2] | 0.976 [0.972, 0.983] | same |
| structural_beam3_frame_dynamic | solve_step | t8 | 5 | 60 | 2.826 s [2.815 s, 2.894 s] | 47.10 ms [46.92 ms, 48.23 ms] | 7742.2 [7712.2, 7926.6] | 0.984 [0.983, 0.995] | same |
| structural_beam3_frame_static | mesh | base | 5 | 1 | 277.0 µs [245.0 µs, 290.0 µs] | 277.0 µs [245.0 µs, 290.0 µs] | 45.5 [40.3, 47.7] | – | same |
| structural_beam3_frame_static | mesh | t1 | 5 | 1 | 265.0 µs [242.0 µs, 282.0 µs] | 265.0 µs [242.0 µs, 282.0 µs] | 43.6 [39.8, 46.4] | 0.957 [0.903, 1.086] | same |
| structural_beam3_frame_static | mesh | t2 | 5 | 1 | 260.0 µs [228.0 µs, 292.0 µs] | 260.0 µs [228.0 µs, 292.0 µs] | 42.7 [37.5, 48.0] | 0.992 [0.823, 1.061] | same |
| structural_beam3_frame_static | mesh | t4 | 5 | 1 | 281.0 µs [248.0 µs, 292.0 µs] | 281.0 µs [248.0 µs, 292.0 µs] | 46.2 [40.8, 48.0] | 1.000 [0.876, 1.147] | same |
| structural_beam3_frame_static | mesh | t8 | 5 | 1 | 262.0 µs [258.0 µs, 278.0 µs] | 262.0 µs [258.0 µs, 278.0 µs] | 43.1 [42.4, 45.7] | 0.982 [0.890, 1.069] | same |
| structural_beam3_frame_static | init | base | 5 | 1 | 27.63 ms [26.72 ms, 27.87 ms] | 27.63 ms [26.72 ms, 27.87 ms] | 4541.4 [4391.2, 4581.5] | – | same |
| structural_beam3_frame_static | init | t1 | 5 | 1 | 26.55 ms [25.76 ms, 27.09 ms] | 26.55 ms [25.76 ms, 27.09 ms] | 4364.6 [4233.9, 4452.0] | 0.961 [0.936, 1.014] | same |
| structural_beam3_frame_static | init | t2 | 5 | 1 | 26.51 ms [25.73 ms, 27.47 ms] | 26.51 ms [25.73 ms, 27.47 ms] | 4357.5 [4228.5, 4515.6] | 0.981 [0.923, 0.994] | same |
| structural_beam3_frame_static | init | t4 | 5 | 1 | 26.69 ms [26.51 ms, 27.31 ms] | 26.69 ms [26.51 ms, 27.31 ms] | 4387.1 [4356.5, 4489.3] | 0.979 [0.960, 0.999] | same |
| structural_beam3_frame_static | init | t8 | 5 | 1 | 27.24 ms [26.93 ms, 27.36 ms] | 27.24 ms [26.93 ms, 27.36 ms] | 4476.8 [4426.7, 4497.5] | 0.989 [0.975, 1.020] | same |
| structural_beam3_frame_static | assemble_stiffness | base | 5 | 5 | 35.23 ms [34.99 ms, 35.77 ms] | 7.05 ms [7.00 ms, 7.15 ms] | 1158.2 [1150.3, 1175.8] | – | same |
| structural_beam3_frame_static | assemble_stiffness | t1 | 5 | 5 | 28.59 ms [28.23 ms, 28.80 ms] | 5.72 ms [5.65 ms, 5.76 ms] | 939.7 [927.9, 946.8] | 0.812 [0.790, 0.823] | same |
| structural_beam3_frame_static | assemble_stiffness | t2 | 5 | 5 | 15.93 ms [15.88 ms, 16.28 ms] | 3.19 ms [3.18 ms, 3.26 ms] | 523.5 [522.0, 535.2] | 0.453 [0.446, 0.460] | same |
| structural_beam3_frame_static | assemble_stiffness | t4 | 5 | 5 | 9.66 ms [9.54 ms, 9.67 ms] | 1.93 ms [1.91 ms, 1.93 ms] | 317.6 [313.5, 317.9] | 0.273 [0.267, 0.276] | same |
| structural_beam3_frame_static | assemble_stiffness | t8 | 5 | 5 | 7.09 ms [6.94 ms, 7.18 ms] | 1.42 ms [1.39 ms, 1.44 ms] | 233.0 [228.0, 236.0] | 0.198 [0.194, 0.204] | same |
| structural_beam3_frame_static | solve_step | base | 5 | 1 | 1.200 s [1.192 s, 1.202 s] | 1.200 s [1.192 s, 1.202 s] | 197174.6 [195891.2, 197540.8] | – | same |
| structural_beam3_frame_static | solve_step | t1 | 5 | 1 | 1.197 s [1.193 s, 1.205 s] | 1.197 s [1.193 s, 1.205 s] | 196742.9 [196138.2, 198092.5] | 0.999 [0.996, 1.004] | same |
| structural_beam3_frame_static | solve_step | t2 | 5 | 1 | 1.201 s [1.200 s, 1.203 s] | 1.201 s [1.200 s, 1.203 s] | 197371.1 [197170.9, 197771.5] | 1.002 [0.998, 1.008] | same |
| structural_beam3_frame_static | solve_step | t4 | 5 | 1 | 1.197 s [1.196 s, 1.204 s] | 1.197 s [1.196 s, 1.204 s] | 196786.2 [196548.7, 197841.6] | 1.001 [0.996, 1.005] | same |
| structural_beam3_frame_static | solve_step | t8 | 5 | 1 | 1.197 s [1.196 s, 1.205 s] | 1.197 s [1.196 s, 1.205 s] | 196818.9 [196654.0, 198047.8] | 1.003 [0.998, 1.004] | same |
| structural_dkt18_plate_static | mesh | base | 5 | 1 | 230.0 µs [214.0 µs, 241.0 µs] | 230.0 µs [214.0 µs, 241.0 µs] | 31.9 [29.7, 33.5] | – | same |
| structural_dkt18_plate_static | mesh | t1 | 5 | 1 | 237.0 µs [227.0 µs, 260.0 µs] | 237.0 µs [227.0 µs, 260.0 µs] | 32.9 [31.5, 36.1] | 1.051 [1.017, 1.159] | same |
| structural_dkt18_plate_static | mesh | t2 | 5 | 1 | 246.0 µs [234.0 µs, 251.0 µs] | 246.0 µs [234.0 µs, 251.0 µs] | 34.2 [32.5, 34.9] | 1.065 [0.975, 1.168] | same |
| structural_dkt18_plate_static | mesh | t4 | 5 | 1 | 239.0 µs [237.0 µs, 249.0 µs] | 239.0 µs [237.0 µs, 249.0 µs] | 33.2 [32.9, 34.6] | 1.065 [1.030, 1.117] | same |
| structural_dkt18_plate_static | mesh | t8 | 5 | 1 | 253.0 µs [238.0 µs, 267.0 µs] | 253.0 µs [238.0 µs, 267.0 µs] | 35.1 [33.1, 37.1] | 1.139 [0.988, 1.236] | same |
| structural_dkt18_plate_static | init | base | 5 | 1 | 65.18 ms [64.26 ms, 65.34 ms] | 65.18 ms [64.26 ms, 65.34 ms] | 9052.4 [8925.3, 9074.4] | – | same |
| structural_dkt18_plate_static | init | t1 | 5 | 1 | 64.50 ms [63.39 ms, 65.41 ms] | 64.50 ms [63.39 ms, 65.41 ms] | 8958.3 [8804.7, 9084.3] | 0.998 [0.972, 1.004] | same |
| structural_dkt18_plate_static | init | t2 | 5 | 1 | 64.15 ms [63.26 ms, 64.71 ms] | 64.15 ms [63.26 ms, 64.71 ms] | 8909.2 [8785.7, 8987.1] | 0.985 [0.975, 0.994] | same |
| structural_dkt18_plate_static | init | t4 | 5 | 1 | 63.96 ms [63.53 ms, 64.40 ms] | 63.96 ms [63.53 ms, 64.40 ms] | 8883.2 [8823.8, 8944.4] | 0.986 [0.980, 0.995] | same |
| structural_dkt18_plate_static | init | t8 | 5 | 1 | 64.10 ms [63.47 ms, 65.02 ms] | 64.10 ms [63.47 ms, 65.02 ms] | 8903.2 [8814.7, 9030.4] | 0.988 [0.979, 0.997] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 5 | 5 | 217.37 ms [216.70 ms, 223.04 ms] | 43.47 ms [43.34 ms, 44.61 ms] | 6038.0 [6019.5, 6195.5] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | t1 | 5 | 5 | 125.41 ms [124.45 ms, 126.93 ms] | 25.08 ms [24.89 ms, 25.39 ms] | 3483.6 [3456.9, 3525.9] | 0.578 [0.559, 0.579] | same |
| structural_dkt18_plate_static | assemble_stiffness | t2 | 5 | 5 | 66.73 ms [66.52 ms, 69.29 ms] | 13.35 ms [13.30 ms, 13.86 ms] | 1853.5 [1847.6, 1924.8] | 0.308 [0.304, 0.313] | same |
| structural_dkt18_plate_static | assemble_stiffness | t4 | 5 | 5 | 38.37 ms [37.69 ms, 39.14 ms] | 7.67 ms [7.54 ms, 7.83 ms] | 1065.8 [1046.9, 1087.3] | 0.177 [0.173, 0.178] | same |
| structural_dkt18_plate_static | assemble_stiffness | t8 | 5 | 5 | 24.26 ms [24.14 ms, 24.82 ms] | 4.85 ms [4.83 ms, 4.96 ms] | 673.9 [670.6, 689.6] | 0.111 [0.109, 0.113] | same |
| structural_dkt18_plate_static | solve_step | base | 5 | 1 | 937.46 ms [930.42 ms, 958.76 ms] | 937.46 ms [930.42 ms, 958.76 ms] | 130202.9 [129225.7, 133160.6] | – | same |
| structural_dkt18_plate_static | solve_step | t1 | 5 | 1 | 943.49 ms [937.57 ms, 960.66 ms] | 943.49 ms [937.57 ms, 960.66 ms] | 131040.0 [130218.3, 133425.7] | 1.002 [0.996, 1.012] | same |
| structural_dkt18_plate_static | solve_step | t2 | 5 | 1 | 939.17 ms [937.37 ms, 950.08 ms] | 939.17 ms [937.37 ms, 950.08 ms] | 130439.9 [130190.8, 131955.7] | 1.000 [0.991, 1.012] | same |
| structural_dkt18_plate_static | solve_step | t4 | 5 | 1 | 944.44 ms [940.60 ms, 946.83 ms] | 944.44 ms [940.60 ms, 946.83 ms] | 131172.8 [130638.3, 131504.4] | 1.006 [0.988, 1.011] | same |
| structural_dkt18_plate_static | solve_step | t8 | 5 | 1 | 943.27 ms [937.39 ms, 947.95 ms] | 943.27 ms [937.39 ms, 947.95 ms] | 131010.0 [130193.8, 131659.3] | 1.006 [0.989, 1.007] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 0bd91ba55, d404f4c70 | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 0bd91ba55, d404f4c70 | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 0bd91ba55, d404f4c70 | 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 0bd91ba55, d404f4c70 | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 0bd91ba55, d404f4c70 | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 0bd91ba55, d404f4c70 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 0bd91ba55, d404f4c70 | a77809465b3e7c18 |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | 0bd91ba55, d404f4c70 | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 0bd91ba55, d404f4c70 | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 0bd91ba55, d404f4c70 | 1cfc7f010c52d6b3 |
