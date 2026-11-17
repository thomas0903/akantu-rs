Suite `models`, size `m`; variants: `auto`, `base`, `colamd`; reference `colamd`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to colamd | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | mesh | auto | 5 | 1 | 8.91 ms [8.84 ms, 8.92 ms] | 8.91 ms [8.84 ms, 8.92 ms] | 1272.6 [1263.4, 1273.9] | 0.996 [0.978, 1.005] | same |
| contact_implicit_static_quad4 | mesh | base | 5 | 1 | 8.78 ms [8.73 ms, 8.87 ms] | 8.78 ms [8.73 ms, 8.87 ms] | 1254.6 [1246.9, 1266.7] | 0.981 [0.963, 0.998] | same |
| contact_implicit_static_quad4 | mesh | colamd | 5 | 1 | 8.90 ms [8.86 ms, 9.12 ms] | 8.90 ms [8.86 ms, 9.12 ms] | 1271.4 [1266.1, 1303.1] | – | same |
| contact_implicit_static_quad4 | init | auto | 5 | 1 | 10.41 ms [10.37 ms, 10.52 ms] | 10.41 ms [10.37 ms, 10.52 ms] | 1487.1 [1481.6, 1502.6] | 0.998 [0.985, 1.006] | same |
| contact_implicit_static_quad4 | init | base | 5 | 1 | 10.49 ms [10.45 ms, 10.55 ms] | 10.49 ms [10.45 ms, 10.55 ms] | 1498.1 [1492.3, 1506.6] | 1.005 [0.988, 1.014] | same |
| contact_implicit_static_quad4 | init | colamd | 5 | 1 | 10.43 ms [10.40 ms, 10.57 ms] | 10.43 ms [10.40 ms, 10.57 ms] | 1490.3 [1486.4, 1510.1] | – | same |
| contact_implicit_static_quad4 | solve_step | auto | 5 | 5 | 759.60 ms [754.87 ms, 761.42 ms] | 151.92 ms [150.97 ms, 152.28 ms] | 21702.9 [21567.7, 21754.9] | 0.993 [0.990, 1.000] | same |
| contact_implicit_static_quad4 | solve_step | base | 5 | 5 | 752.47 ms [750.77 ms, 769.01 ms] | 150.49 ms [150.15 ms, 153.80 ms] | 21499.1 [21450.5, 21971.7] | 0.989 [0.976, 1.009] | same |
| contact_implicit_static_quad4 | solve_step | colamd | 5 | 5 | 761.97 ms [759.72 ms, 769.21 ms] | 152.39 ms [151.94 ms, 153.84 ms] | 21770.6 [21706.4, 21977.3] | – | same |
| embedded_rebar_tet4 | mesh | auto | 5 | 1 | 21.88 ms [21.82 ms, 21.96 ms] | 21.88 ms [21.82 ms, 21.96 ms] | 1006.1 [1003.1, 1009.7] | 1.008 [1.004, 1.008] | DIFFER (2) |
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 21.51 ms [21.46 ms, 21.57 ms] | 21.51 ms [21.46 ms, 21.57 ms] | 989.1 [986.7, 991.5] | 0.991 [0.986, 0.994] | DIFFER (2) |
| embedded_rebar_tet4 | mesh | colamd | 5 | 1 | 21.73 ms [21.64 ms, 21.78 ms] | 21.73 ms [21.64 ms, 21.78 ms] | 998.9 [994.9, 1001.5] | – | DIFFER (2) |
| embedded_rebar_tet4 | init | auto | 5 | 1 | 39.89 ms [39.78 ms, 40.19 ms] | 39.89 ms [39.78 ms, 40.19 ms] | 1834.0 [1829.1, 1847.6] | 1.001 [0.992, 1.010] | DIFFER (2) |
| embedded_rebar_tet4 | init | base | 5 | 1 | 40.09 ms [39.78 ms, 40.25 ms] | 40.09 ms [39.78 ms, 40.25 ms] | 1843.0 [1829.1, 1850.4] | 1.002 [0.999, 1.011] | DIFFER (2) |
| embedded_rebar_tet4 | init | colamd | 5 | 1 | 39.82 ms [39.80 ms, 40.11 ms] | 39.82 ms [39.80 ms, 40.11 ms] | 1830.7 [1829.7, 1844.3] | – | DIFFER (2) |
| embedded_rebar_tet4 | intersect | auto | 5 | 5 | 93.48 ms [93.21 ms, 93.63 ms] | 18.70 ms [18.64 ms, 18.73 ms] | 859.5 [857.1, 860.9] | 1.001 [0.998, 1.002] | DIFFER (2) |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 93.14 ms [92.84 ms, 93.31 ms] | 18.63 ms [18.57 ms, 18.66 ms] | 856.5 [853.7, 858.1] | 0.997 [0.994, 1.000] | DIFFER (2) |
| embedded_rebar_tet4 | intersect | colamd | 5 | 5 | 93.40 ms [93.33 ms, 93.61 ms] | 18.68 ms [18.67 ms, 18.72 ms] | 858.8 [858.2, 860.8] | – | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | auto | 5 | 1 | 610.41 ms [608.54 ms, 616.04 ms] | 610.41 ms [608.54 ms, 616.04 ms] | 28064.9 [27978.8, 28323.9] | 0.566 [0.559, 0.568] | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 1.087 s [1.086 s, 1.093 s] | 1.087 s [1.086 s, 1.093 s] | 49983.9 [49913.0, 50253.6] | 1.006 [0.994, 1.017] | DIFFER (2) |
| embedded_rebar_tet4 | solve_step | colamd | 5 | 1 | 1.081 s [1.074 s, 1.092 s] | 1.081 s [1.074 s, 1.092 s] | 49685.3 [49396.0, 50215.6] | – | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | auto | 5 | 20 | 28.17 ms [28.11 ms, 31.01 ms] | 1.41 ms [1.41 ms, 1.55 ms] | 64.8 [64.6, 71.3] | 1.001 [0.982, 1.107] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 28.60 ms [28.56 ms, 28.62 ms] | 1.43 ms [1.43 ms, 1.43 ms] | 65.8 [65.6, 65.8] | 1.013 [0.998, 1.020] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_internal_forces | colamd | 5 | 20 | 28.24 ms [28.00 ms, 28.63 ms] | 1.41 ms [1.40 ms, 1.43 ms] | 64.9 [64.4, 65.8] | – | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | auto | 5 | 5 | 78.28 ms [78.08 ms, 78.35 ms] | 15.66 ms [15.62 ms, 15.67 ms] | 719.8 [717.9, 720.5] | 1.007 [1.002, 1.012] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 78.01 ms [77.60 ms, 79.10 ms] | 15.60 ms [15.52 ms, 15.82 ms] | 717.4 [713.6, 727.3] | 1.007 [0.998, 1.014] | DIFFER (2) |
| embedded_rebar_tet4 | assemble_stiffness | colamd | 5 | 5 | 77.56 ms [77.43 ms, 78.23 ms] | 15.51 ms [15.49 ms, 15.65 ms] | 713.2 [712.0, 719.3] | – | DIFFER (2) |
| embedded_rebar_tri3 | mesh | auto | 5 | 1 | 20.60 ms [20.46 ms, 20.68 ms] | 20.60 ms [20.46 ms, 20.68 ms] | 817.6 [811.8, 820.8] | 0.994 [0.991, 1.000] | DIFFER (2) |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 20.36 ms [20.28 ms, 20.40 ms] | 20.36 ms [20.28 ms, 20.40 ms] | 807.9 [804.6, 809.4] | 0.985 [0.978, 0.991] | DIFFER (2) |
| embedded_rebar_tri3 | mesh | colamd | 5 | 1 | 20.67 ms [20.58 ms, 20.73 ms] | 20.67 ms [20.58 ms, 20.73 ms] | 820.4 [816.6, 822.7] | – | DIFFER (2) |
| embedded_rebar_tri3 | init | auto | 5 | 1 | 31.68 ms [31.53 ms, 31.86 ms] | 31.68 ms [31.53 ms, 31.86 ms] | 1257.3 [1251.2, 1264.2] | 0.996 [0.991, 1.012] | DIFFER (2) |
| embedded_rebar_tri3 | init | base | 5 | 1 | 31.56 ms [31.46 ms, 31.92 ms] | 31.56 ms [31.46 ms, 31.92 ms] | 1252.5 [1248.6, 1266.5] | 0.998 [0.990, 1.003] | DIFFER (2) |
| embedded_rebar_tri3 | init | colamd | 5 | 1 | 31.66 ms [31.47 ms, 31.99 ms] | 31.66 ms [31.47 ms, 31.99 ms] | 1256.2 [1248.8, 1269.3] | – | DIFFER (2) |
| embedded_rebar_tri3 | intersect | auto | 5 | 5 | 67.66 ms [67.49 ms, 67.78 ms] | 13.53 ms [13.50 ms, 13.56 ms] | 537.0 [535.6, 537.9] | 0.999 [0.990, 1.002] | DIFFER (2) |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 67.15 ms [66.95 ms, 69.89 ms] | 13.43 ms [13.39 ms, 13.98 ms] | 532.9 [531.3, 554.6] | 0.992 [0.981, 1.033] | DIFFER (2) |
| embedded_rebar_tri3 | intersect | colamd | 5 | 5 | 67.72 ms [67.47 ms, 68.47 ms] | 13.54 ms [13.49 ms, 13.69 ms] | 537.5 [535.5, 543.4] | – | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | auto | 5 | 1 | 210.43 ms [210.15 ms, 211.05 ms] | 210.43 ms [210.15 ms, 211.05 ms] | 8350.5 [8339.2, 8375.2] | 0.879 [0.876, 0.884] | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 240.96 ms [239.79 ms, 241.70 ms] | 240.96 ms [239.79 ms, 241.70 ms] | 9561.9 [9515.5, 9591.3] | 1.005 [0.999, 1.017] | DIFFER (2) |
| embedded_rebar_tri3 | solve_step | colamd | 5 | 1 | 240.08 ms [237.68 ms, 240.30 ms] | 240.08 ms [237.68 ms, 240.30 ms] | 9527.1 [9431.8, 9535.8] | – | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | auto | 5 | 20 | 44.13 ms [44.06 ms, 44.34 ms] | 2.21 ms [2.20 ms, 2.22 ms] | 87.6 [87.4, 88.0] | 1.001 [0.985, 1.005] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 44.16 ms [43.98 ms, 44.70 ms] | 2.21 ms [2.20 ms, 2.23 ms] | 87.6 [87.3, 88.7] | 0.999 [0.983, 1.015] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_internal_forces | colamd | 5 | 20 | 44.04 ms [43.94 ms, 45.01 ms] | 2.20 ms [2.20 ms, 2.25 ms] | 87.4 [87.2, 89.3] | – | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | auto | 5 | 5 | 36.04 ms [35.81 ms, 36.66 ms] | 7.21 ms [7.16 ms, 7.33 ms] | 286.1 [284.2, 290.9] | 1.000 [0.989, 1.023] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 35.90 ms [35.77 ms, 36.21 ms] | 7.18 ms [7.15 ms, 7.24 ms] | 284.9 [283.8, 287.4] | 0.996 [0.987, 1.011] | DIFFER (2) |
| embedded_rebar_tri3 | assemble_stiffness | colamd | 5 | 5 | 36.01 ms [35.84 ms, 36.35 ms] | 7.20 ms [7.17 ms, 7.27 ms] | 285.8 [284.4, 288.5] | – | DIFFER (2) |
| heat_implicit_hex8_kvar | mesh | auto | 5 | 1 | 3.91 ms [3.88 ms, 3.94 ms] | 3.91 ms [3.88 ms, 3.94 ms] | 2259.8 [2247.7, 2280.1] | 1.003 [0.989, 1.013] | same |
| heat_implicit_hex8_kvar | mesh | base | 5 | 1 | 3.90 ms [3.85 ms, 3.92 ms] | 3.90 ms [3.85 ms, 3.92 ms] | 2256.4 [2230.3, 2266.2] | 0.999 [0.977, 1.004] | same |
| heat_implicit_hex8_kvar | mesh | colamd | 5 | 1 | 3.92 ms [3.89 ms, 3.94 ms] | 3.92 ms [3.89 ms, 3.94 ms] | 2268.5 [2251.2, 2283.0] | – | same |
| heat_implicit_hex8_kvar | init | auto | 5 | 1 | 969.0 µs [958.0 µs, 974.0 µs] | 969.0 µs [958.0 µs, 974.0 µs] | 560.8 [554.4, 563.7] | 0.993 [0.929, 1.002] | same |
| heat_implicit_hex8_kvar | init | base | 5 | 1 | 983.0 µs [974.0 µs, 996.0 µs] | 983.0 µs [974.0 µs, 996.0 µs] | 568.9 [563.7, 576.4] | 1.002 [0.966, 1.017] | same |
| heat_implicit_hex8_kvar | init | colamd | 5 | 1 | 972.0 µs [967.0 µs, 1.03 ms] | 972.0 µs [967.0 µs, 1.03 ms] | 562.5 [559.6, 596.6] | – | same |
| heat_implicit_hex8_kvar | solve_step | auto | 5 | 5 | 582.85 ms [577.12 ms, 583.34 ms] | 116.57 ms [115.42 ms, 116.67 ms] | 67459.6 [66796.2, 67516.1] | 1.005 [0.998, 1.009] | same |
| heat_implicit_hex8_kvar | solve_step | base | 5 | 5 | 577.20 ms [576.05 ms, 584.00 ms] | 115.44 ms [115.21 ms, 116.80 ms] | 66805.8 [66672.3, 67592.0] | 0.997 [0.996, 1.010] | same |
| heat_implicit_hex8_kvar | solve_step | colamd | 5 | 5 | 578.27 ms [578.01 ms, 579.90 ms] | 115.65 ms [115.60 ms, 115.98 ms] | 66929.2 [66898.8, 67118.4] | – | same |
| heat_static_tri6 | mesh | auto | 5 | 1 | 40.28 ms [40.19 ms, 40.58 ms] | 40.28 ms [40.19 ms, 40.58 ms] | 1664.6 [1660.6, 1676.8] | 1.002 [0.997, 1.011] | DIFFER (2) |
| heat_static_tri6 | mesh | base | 5 | 1 | 40.17 ms [40.10 ms, 40.36 ms] | 40.17 ms [40.10 ms, 40.36 ms] | 1659.9 [1657.0, 1667.6] | 0.997 [0.993, 1.007] | DIFFER (2) |
| heat_static_tri6 | mesh | colamd | 5 | 1 | 40.30 ms [40.09 ms, 40.48 ms] | 40.30 ms [40.09 ms, 40.48 ms] | 1665.5 [1656.6, 1672.9] | – | DIFFER (2) |
| heat_static_tri6 | init | auto | 5 | 1 | 5.33 ms [5.32 ms, 5.40 ms] | 5.33 ms [5.32 ms, 5.40 ms] | 220.4 [220.0, 223.2] | 1.000 [0.993, 1.013] | DIFFER (2) |
| heat_static_tri6 | init | base | 5 | 1 | 5.44 ms [5.43 ms, 5.47 ms] | 5.44 ms [5.43 ms, 5.47 ms] | 224.9 [224.4, 225.9] | 1.018 [1.015, 1.021] | DIFFER (2) |
| heat_static_tri6 | init | colamd | 5 | 1 | 5.34 ms [5.32 ms, 5.37 ms] | 5.34 ms [5.32 ms, 5.37 ms] | 220.6 [219.8, 222.1] | – | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | auto | 5 | 5 | 29.65 ms [29.59 ms, 30.23 ms] | 5.93 ms [5.92 ms, 6.05 ms] | 245.0 [244.5, 249.9] | 1.003 [0.999, 1.018] | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | base | 5 | 5 | 29.22 ms [29.14 ms, 29.29 ms] | 5.84 ms [5.83 ms, 5.86 ms] | 241.5 [240.9, 242.1] | 0.989 [0.985, 0.990] | DIFFER (2) |
| heat_static_tri6 | assemble_conductivity | colamd | 5 | 5 | 29.60 ms [29.52 ms, 29.70 ms] | 5.92 ms [5.90 ms, 5.94 ms] | 244.6 [244.0, 245.5] | – | DIFFER (2) |
| heat_static_tri6 | solve_step | auto | 5 | 1 | 891.37 ms [890.09 ms, 896.49 ms] | 891.37 ms [890.09 ms, 896.49 ms] | 36833.6 [36780.4, 37045.2] | 0.507 [0.498, 0.509] | DIFFER (2) |
| heat_static_tri6 | solve_step | base | 5 | 1 | 1.770 s [1.754 s, 1.778 s] | 1.770 s [1.754 s, 1.778 s] | 73136.0 [72474.0, 73484.3] | 1.001 [0.984, 1.015] | DIFFER (2) |
| heat_static_tri6 | solve_step | colamd | 5 | 1 | 1.760 s [1.752 s, 1.788 s] | 1.760 s [1.752 s, 1.788 s] | 72739.2 [72412.9, 73863.8] | – | DIFFER (2) |
| phasefield_staggered_tet4 | mesh | auto | 5 | 1 | 6.77 ms [6.75 ms, 6.83 ms] | 6.77 ms [6.75 ms, 6.83 ms] | 1128.2 [1124.7, 1138.3] | 0.997 [0.989, 1.009] | DIFFER (2) |
| phasefield_staggered_tet4 | mesh | base | 5 | 1 | 6.69 ms [6.65 ms, 6.72 ms] | 6.69 ms [6.65 ms, 6.72 ms] | 1114.3 [1109.0, 1119.3] | 0.985 [0.976, 0.992] | DIFFER (2) |
| phasefield_staggered_tet4 | mesh | colamd | 5 | 1 | 6.79 ms [6.75 ms, 6.84 ms] | 6.79 ms [6.75 ms, 6.84 ms] | 1132.0 [1125.5, 1140.7] | – | DIFFER (2) |
| phasefield_staggered_tet4 | init | auto | 5 | 1 | 2.84 ms [2.82 ms, 2.84 ms] | 2.84 ms [2.82 ms, 2.84 ms] | 473.0 [470.2, 474.0] | 1.002 [0.995, 1.007] | DIFFER (2) |
| phasefield_staggered_tet4 | init | base | 5 | 1 | 2.82 ms [2.78 ms, 2.86 ms] | 2.82 ms [2.78 ms, 2.86 ms] | 469.3 [463.8, 477.0] | 0.994 [0.985, 1.012] | DIFFER (2) |
| phasefield_staggered_tet4 | init | colamd | 5 | 1 | 2.83 ms [2.80 ms, 2.86 ms] | 2.83 ms [2.80 ms, 2.86 ms] | 471.3 [466.8, 476.3] | – | DIFFER (2) |
| phasefield_staggered_tet4 | solid_solve | auto | 5 | 4 | 309.88 ms [309.10 ms, 313.87 ms] | 77.47 ms [77.28 ms, 78.47 ms] | 12911.5 [12879.2, 13077.9] | 0.592 [0.589, 0.601] | DIFFER (2) |
| phasefield_staggered_tet4 | solid_solve | base | 5 | 4 | 520.34 ms [517.96 ms, 530.33 ms] | 130.09 ms [129.49 ms, 132.58 ms] | 21680.9 [21581.8, 22097.2] | 0.995 [0.982, 1.016] | DIFFER (2) |
| phasefield_staggered_tet4 | solid_solve | colamd | 5 | 4 | 523.00 ms [522.00 ms, 527.97 ms] | 130.75 ms [130.50 ms, 131.99 ms] | 21791.7 [21750.0, 21998.9] | – | DIFFER (2) |
| phasefield_staggered_tet4 | strain_transfer | auto | 5 | 4 | 699.0 µs [685.0 µs, 711.0 µs] | 174.8 µs [171.2 µs, 177.8 µs] | 29.1 [28.5, 29.6] | 1.004 [0.965, 1.022] | DIFFER (2) |
| phasefield_staggered_tet4 | strain_transfer | base | 5 | 4 | 692.0 µs [680.0 µs, 709.0 µs] | 173.0 µs [170.0 µs, 177.2 µs] | 28.8 [28.3, 29.5] | 0.986 [0.975, 1.019] | DIFFER (2) |
| phasefield_staggered_tet4 | strain_transfer | colamd | 5 | 4 | 696.0 µs [690.0 µs, 710.0 µs] | 174.0 µs [172.5 µs, 177.5 µs] | 29.0 [28.8, 29.6] | – | DIFFER (2) |
| phasefield_staggered_tet4 | phase_solve | auto | 5 | 4 | 63.26 ms [63.06 ms, 63.32 ms] | 15.82 ms [15.77 ms, 15.83 ms] | 2635.9 [2627.7, 2638.2] | 0.998 [0.993, 1.002] | DIFFER (2) |
| phasefield_staggered_tet4 | phase_solve | base | 5 | 4 | 63.08 ms [63.02 ms, 63.35 ms] | 15.77 ms [15.76 ms, 15.84 ms] | 2628.4 [2625.9, 2639.6] | 0.997 [0.990, 1.002] | DIFFER (2) |
| phasefield_staggered_tet4 | phase_solve | colamd | 5 | 4 | 63.27 ms [63.17 ms, 63.65 ms] | 15.82 ms [15.79 ms, 15.91 ms] | 2636.0 [2631.9, 2652.1] | – | DIFFER (2) |
| phasefield_staggered_tet4 | damage_transfer | auto | 5 | 4 | 1.59 ms [1.57 ms, 1.60 ms] | 396.8 µs [391.5 µs, 401.0 µs] | 66.1 [65.2, 66.8] | 1.001 [0.981, 1.008] | DIFFER (2) |
| phasefield_staggered_tet4 | damage_transfer | base | 5 | 4 | 1.59 ms [1.57 ms, 1.60 ms] | 397.5 µs [393.2 µs, 401.0 µs] | 66.2 [65.5, 66.8] | 0.997 [0.991, 0.999] | DIFFER (2) |
| phasefield_staggered_tet4 | damage_transfer | colamd | 5 | 4 | 1.60 ms [1.58 ms, 1.61 ms] | 399.0 µs [395.5 µs, 401.5 µs] | 66.5 [65.9, 66.9] | – | DIFFER (2) |
| phasefield_staggered_tet4 | internal_forces | auto | 5 | 4 | 1.53 ms [1.52 ms, 1.56 ms] | 382.2 µs [381.0 µs, 390.8 µs] | 63.7 [63.5, 65.1] | 1.007 [0.995, 1.024] | DIFFER (2) |
| phasefield_staggered_tet4 | internal_forces | base | 5 | 4 | 1.52 ms [1.51 ms, 1.53 ms] | 379.8 µs [378.5 µs, 383.0 µs] | 63.3 [63.1, 63.8] | 0.992 [0.991, 1.008] | DIFFER (2) |
| phasefield_staggered_tet4 | internal_forces | colamd | 5 | 4 | 1.53 ms [1.52 ms, 1.53 ms] | 382.0 µs [379.5 µs, 382.8 µs] | 63.7 [63.2, 63.8] | – | DIFFER (2) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | auto | 5 | 5 | 19.63 ms [19.60 ms, 19.81 ms] | 3.93 ms [3.92 ms, 3.96 ms] | 654.3 [653.2, 660.5] | 0.992 [0.981, 1.004] | DIFFER (2) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base | 5 | 5 | 19.54 ms [19.47 ms, 19.58 ms] | 3.91 ms [3.90 ms, 3.92 ms] | 651.3 [649.2, 652.6] | 0.986 [0.978, 0.990] | DIFFER (2) |
| phasefield_staggered_tet4 | solid_assemble_stiffness | colamd | 5 | 5 | 19.79 ms [19.74 ms, 19.98 ms] | 3.96 ms [3.95 ms, 4.00 ms] | 659.8 [658.1, 666.0] | – | DIFFER (2) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | auto | 5 | 5 | 4.56 ms [4.55 ms, 4.57 ms] | 911.2 µs [909.8 µs, 913.4 µs] | 151.9 [151.6, 152.2] | 0.993 [0.989, 1.001] | DIFFER (2) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base | 5 | 5 | 4.59 ms [4.53 ms, 4.63 ms] | 918.8 µs [906.2 µs, 926.8 µs] | 153.1 [151.0, 154.5] | 0.998 [0.985, 1.011] | DIFFER (2) |
| phasefield_staggered_tet4 | phase_assemble_stiffness | colamd | 5 | 5 | 4.58 ms [4.56 ms, 4.61 ms] | 916.8 µs [911.6 µs, 921.2 µs] | 152.8 [151.9, 153.5] | – | DIFFER (2) |
| phasefield_staggered_tri3 | mesh | auto | 5 | 1 | 42.53 ms [42.41 ms, 43.05 ms] | 42.53 ms [42.41 ms, 43.05 ms] | 830.6 [828.4, 840.8] | 1.001 [0.998, 1.014] | DIFFER (2) |
| phasefield_staggered_tri3 | mesh | base | 5 | 1 | 41.91 ms [41.66 ms, 42.14 ms] | 41.91 ms [41.66 ms, 42.14 ms] | 818.7 [813.6, 823.0] | 0.988 [0.984, 0.993] | DIFFER (2) |
| phasefield_staggered_tri3 | mesh | colamd | 5 | 1 | 42.44 ms [42.35 ms, 42.62 ms] | 42.44 ms [42.35 ms, 42.62 ms] | 828.8 [827.2, 832.4] | – | DIFFER (2) |
| phasefield_staggered_tri3 | init | auto | 5 | 1 | 19.66 ms [19.55 ms, 19.78 ms] | 19.66 ms [19.55 ms, 19.78 ms] | 384.0 [381.8, 386.3] | 0.997 [0.989, 1.001] | DIFFER (2) |
| phasefield_staggered_tri3 | init | base | 5 | 1 | 19.80 ms [19.69 ms, 19.86 ms] | 19.80 ms [19.69 ms, 19.86 ms] | 386.7 [384.5, 387.9] | 1.002 [0.991, 1.011] | DIFFER (2) |
| phasefield_staggered_tri3 | init | colamd | 5 | 1 | 19.76 ms [19.64 ms, 19.87 ms] | 19.76 ms [19.64 ms, 19.87 ms] | 386.0 [383.6, 388.0] | – | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | auto | 5 | 4 | 1.430 s [1.427 s, 1.434 s] | 357.52 ms [356.85 ms, 358.61 ms] | 6982.7 [6969.8, 7004.1] | 0.469 [0.465, 0.471] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | base | 5 | 4 | 3.049 s [3.039 s, 3.054 s] | 762.15 ms [759.72 ms, 763.54 ms] | 14885.7 [14838.2, 14912.8] | 1.000 [0.991, 1.002] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_solve | colamd | 5 | 4 | 3.047 s [3.045 s, 3.075 s] | 761.82 ms [761.36 ms, 768.80 ms] | 14879.4 [14870.3, 15015.7] | – | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | auto | 5 | 4 | 4.34 ms [4.26 ms, 4.38 ms] | 1.08 ms [1.06 ms, 1.10 ms] | 21.2 [20.8, 21.4] | 1.014 [0.975, 1.034] | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | base | 5 | 4 | 4.09 ms [4.05 ms, 4.12 ms] | 1.02 ms [1.01 ms, 1.03 ms] | 20.0 [19.8, 20.1] | 0.962 [0.936, 0.972] | DIFFER (2) |
| phasefield_staggered_tri3 | strain_transfer | colamd | 5 | 4 | 4.24 ms [4.24 ms, 4.37 ms] | 1.06 ms [1.06 ms, 1.09 ms] | 20.7 [20.7, 21.3] | – | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | auto | 5 | 4 | 483.45 ms [482.58 ms, 486.08 ms] | 120.86 ms [120.65 ms, 121.52 ms] | 2360.6 [2356.4, 2373.4] | 0.610 [0.610, 0.616] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | base | 5 | 4 | 791.00 ms [788.12 ms, 793.21 ms] | 197.75 ms [197.03 ms, 198.30 ms] | 3862.3 [3848.2, 3873.1] | 1.001 [0.995, 1.003] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_solve | colamd | 5 | 4 | 791.44 ms [788.77 ms, 792.71 ms] | 197.86 ms [197.19 ms, 198.18 ms] | 3864.5 [3851.4, 3870.6] | – | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | auto | 5 | 4 | 12.13 ms [12.10 ms, 12.16 ms] | 3.03 ms [3.03 ms, 3.04 ms] | 59.2 [59.1, 59.4] | 0.998 [0.997, 1.005] | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | base | 5 | 4 | 12.16 ms [12.12 ms, 12.36 ms] | 3.04 ms [3.03 ms, 3.09 ms] | 59.4 [59.2, 60.3] | 1.003 [1.002, 1.016] | DIFFER (2) |
| phasefield_staggered_tri3 | damage_transfer | colamd | 5 | 4 | 12.14 ms [12.10 ms, 12.17 ms] | 3.03 ms [3.02 ms, 3.04 ms] | 59.3 [59.1, 59.4] | – | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | auto | 5 | 4 | 6.97 ms [6.96 ms, 6.98 ms] | 1.74 ms [1.74 ms, 1.74 ms] | 34.0 [34.0, 34.1] | 1.000 [0.998, 1.005] | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | base | 5 | 4 | 7.05 ms [7.03 ms, 7.07 ms] | 1.76 ms [1.76 ms, 1.77 ms] | 34.4 [34.3, 34.5] | 1.013 [1.008, 1.019] | DIFFER (2) |
| phasefield_staggered_tri3 | internal_forces | colamd | 5 | 4 | 6.96 ms [6.94 ms, 6.98 ms] | 1.74 ms [1.73 ms, 1.74 ms] | 34.0 [33.9, 34.1] | – | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | auto | 5 | 5 | 41.98 ms [41.97 ms, 42.10 ms] | 8.40 ms [8.39 ms, 8.42 ms] | 164.0 [163.9, 164.5] | 1.007 [1.000, 1.008] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base | 5 | 5 | 42.25 ms [41.99 ms, 42.66 ms] | 8.45 ms [8.40 ms, 8.53 ms] | 165.0 [164.0, 166.7] | 1.014 [0.998, 1.025] | DIFFER (2) |
| phasefield_staggered_tri3 | solid_assemble_stiffness | colamd | 5 | 5 | 41.69 ms [41.63 ms, 42.08 ms] | 8.34 ms [8.33 ms, 8.42 ms] | 162.9 [162.6, 164.4] | – | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | auto | 5 | 5 | 34.63 ms [34.60 ms, 34.68 ms] | 6.93 ms [6.92 ms, 6.94 ms] | 135.3 [135.2, 135.5] | 1.008 [1.003, 1.009] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base | 5 | 5 | 34.09 ms [33.89 ms, 34.15 ms] | 6.82 ms [6.78 ms, 6.83 ms] | 133.2 [132.4, 133.4] | 0.989 [0.988, 0.995] | DIFFER (2) |
| phasefield_staggered_tri3 | phase_assemble_stiffness | colamd | 5 | 5 | 34.40 ms [34.31 ms, 34.53 ms] | 6.88 ms [6.86 ms, 6.91 ms] | 134.4 [134.0, 134.9] | – | DIFFER (2) |
| structural_beam3_frame_dynamic | mesh | auto | 5 | 1 | 332.0 µs [330.0 µs, 332.0 µs] | 332.0 µs [330.0 µs, 332.0 µs] | 54.6 [54.2, 54.6] | 0.988 [0.946, 1.009] | same |
| structural_beam3_frame_dynamic | mesh | base | 5 | 1 | 346.0 µs [333.0 µs, 371.0 µs] | 346.0 µs [333.0 µs, 371.0 µs] | 56.9 [54.7, 61.0] | 1.027 [0.991, 1.085] | same |
| structural_beam3_frame_dynamic | mesh | colamd | 5 | 1 | 336.0 µs [329.0 µs, 349.0 µs] | 336.0 µs [329.0 µs, 349.0 µs] | 55.2 [54.1, 57.4] | – | same |
| structural_beam3_frame_dynamic | init | auto | 5 | 1 | 28.80 ms [28.68 ms, 29.11 ms] | 28.80 ms [28.68 ms, 29.11 ms] | 4734.4 [4713.8, 4784.2] | 1.005 [0.990, 1.016] | same |
| structural_beam3_frame_dynamic | init | base | 5 | 1 | 28.89 ms [28.72 ms, 28.94 ms] | 28.89 ms [28.72 ms, 28.94 ms] | 4748.5 [4720.7, 4756.4] | 1.004 [0.995, 1.010] | same |
| structural_beam3_frame_dynamic | init | colamd | 5 | 1 | 28.66 ms [28.59 ms, 29.07 ms] | 28.66 ms [28.59 ms, 29.07 ms] | 4710.7 [4699.9, 4777.8] | – | same |
| structural_beam3_frame_dynamic | solve_step | auto | 5 | 60 | 2.954 s [2.945 s, 2.963 s] | 49.23 ms [49.09 ms, 49.38 ms] | 8091.6 [8068.8, 8116.7] | 0.996 [0.991, 1.000] | same |
| structural_beam3_frame_dynamic | solve_step | base | 5 | 60 | 2.959 s [2.953 s, 2.968 s] | 49.32 ms [49.21 ms, 49.47 ms] | 8106.2 [8088.3, 8130.7] | 0.999 [0.993, 1.006] | same |
| structural_beam3_frame_dynamic | solve_step | colamd | 5 | 60 | 2.961 s [2.947 s, 2.989 s] | 49.35 ms [49.11 ms, 49.81 ms] | 8111.9 [8072.3, 8187.3] | – | same |
| structural_beam3_frame_static | mesh | auto | 5 | 1 | 336.0 µs [330.0 µs, 350.0 µs] | 336.0 µs [330.0 µs, 350.0 µs] | 55.2 [54.2, 57.5] | 1.018 [0.951, 1.061] | DIFFER (2) |
| structural_beam3_frame_static | mesh | base | 5 | 1 | 336.0 µs [332.0 µs, 365.0 µs] | 336.0 µs [332.0 µs, 365.0 µs] | 55.2 [54.6, 60.0] | 1.012 [0.994, 1.052] | DIFFER (2) |
| structural_beam3_frame_static | mesh | colamd | 5 | 1 | 334.0 µs [330.0 µs, 347.0 µs] | 334.0 µs [330.0 µs, 347.0 µs] | 54.9 [54.2, 57.0] | – | DIFFER (2) |
| structural_beam3_frame_static | init | auto | 5 | 1 | 28.65 ms [28.46 ms, 28.78 ms] | 28.65 ms [28.46 ms, 28.78 ms] | 4708.6 [4677.7, 4729.9] | 0.999 [0.994, 1.009] | DIFFER (2) |
| structural_beam3_frame_static | init | base | 5 | 1 | 28.67 ms [28.54 ms, 28.77 ms] | 28.67 ms [28.54 ms, 28.77 ms] | 4712.9 [4690.7, 4729.0] | 1.000 [0.990, 1.008] | DIFFER (2) |
| structural_beam3_frame_static | init | colamd | 5 | 1 | 28.63 ms [28.53 ms, 28.81 ms] | 28.63 ms [28.53 ms, 28.81 ms] | 4705.1 [4689.2, 4735.9] | – | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | auto | 5 | 5 | 23.55 ms [23.47 ms, 23.56 ms] | 4.71 ms [4.69 ms, 4.71 ms] | 774.0 [771.7, 774.4] | 1.001 [0.995, 1.004] | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | base | 5 | 5 | 23.54 ms [23.49 ms, 23.60 ms] | 4.71 ms [4.70 ms, 4.72 ms] | 773.9 [772.3, 775.9] | 1.001 [0.996, 1.003] | DIFFER (2) |
| structural_beam3_frame_static | assemble_stiffness | colamd | 5 | 5 | 23.52 ms [23.46 ms, 23.62 ms] | 4.70 ms [4.69 ms, 4.72 ms] | 773.3 [771.3, 776.3] | – | DIFFER (2) |
| structural_beam3_frame_static | solve_step | auto | 5 | 1 | 787.46 ms [780.56 ms, 789.21 ms] | 787.46 ms [780.56 ms, 789.21 ms] | 129432.1 [128297.0, 129719.3] | 0.578 [0.573, 0.579] | DIFFER (2) |
| structural_beam3_frame_static | solve_step | base | 5 | 1 | 1.358 s [1.352 s, 1.364 s] | 1.358 s [1.352 s, 1.364 s] | 223268.6 [222230.9, 224195.3] | 0.997 [0.993, 1.002] | DIFFER (2) |
| structural_beam3_frame_static | solve_step | colamd | 5 | 1 | 1.362 s [1.351 s, 1.370 s] | 1.362 s [1.351 s, 1.370 s] | 223887.4 [222078.7, 225139.1] | – | DIFFER (2) |
| structural_dkt18_plate_static | mesh | auto | 5 | 1 | 290.0 µs [289.0 µs, 311.0 µs] | 290.0 µs [289.0 µs, 311.0 µs] | 40.3 [40.1, 43.2] | 1.003 [0.964, 1.072] | same |
| structural_dkt18_plate_static | mesh | base | 5 | 1 | 320.0 µs [292.0 µs, 327.0 µs] | 320.0 µs [292.0 µs, 327.0 µs] | 44.4 [40.6, 45.4] | 1.046 [1.007, 1.135] | same |
| structural_dkt18_plate_static | mesh | colamd | 5 | 1 | 290.0 µs [287.0 µs, 306.0 µs] | 290.0 µs [287.0 µs, 306.0 µs] | 40.3 [39.9, 42.5] | – | same |
| structural_dkt18_plate_static | init | auto | 5 | 1 | 65.66 ms [65.44 ms, 66.40 ms] | 65.66 ms [65.44 ms, 66.40 ms] | 9119.7 [9089.3, 9222.2] | 1.001 [0.996, 1.012] | same |
| structural_dkt18_plate_static | init | base | 5 | 1 | 66.06 ms [65.76 ms, 66.17 ms] | 66.06 ms [65.76 ms, 66.17 ms] | 9174.6 [9133.5, 9190.7] | 1.006 [1.004, 1.007] | same |
| structural_dkt18_plate_static | init | colamd | 5 | 1 | 65.63 ms [65.51 ms, 65.78 ms] | 65.63 ms [65.51 ms, 65.78 ms] | 9115.3 [9098.8, 9136.1] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | auto | 5 | 5 | 114.96 ms [114.51 ms, 115.64 ms] | 22.99 ms [22.90 ms, 23.13 ms] | 3193.4 [3180.9, 3212.3] | 1.000 [0.996, 1.009] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 5 | 5 | 114.48 ms [114.01 ms, 114.86 ms] | 22.90 ms [22.80 ms, 22.97 ms] | 3180.1 [3167.0, 3190.5] | 0.997 [0.988, 1.003] | same |
| structural_dkt18_plate_static | assemble_stiffness | colamd | 5 | 5 | 114.87 ms [114.52 ms, 115.40 ms] | 22.97 ms [22.90 ms, 23.08 ms] | 3190.8 [3181.2, 3205.7] | – | same |
| structural_dkt18_plate_static | solve_step | auto | 5 | 1 | 898.67 ms [893.41 ms, 902.62 ms] | 898.67 ms [893.41 ms, 902.62 ms] | 124815.0 [124084.6, 125363.7] | 1.000 [0.987, 1.004] | same |
| structural_dkt18_plate_static | solve_step | base | 5 | 1 | 904.43 ms [899.31 ms, 906.25 ms] | 904.43 ms [899.31 ms, 906.25 ms] | 125614.7 [124904.3, 125868.5] | 1.004 [0.998, 1.011] | same |
| structural_dkt18_plate_static | solve_step | colamd | 5 | 1 | 899.39 ms [896.02 ms, 907.68 ms] | 899.39 ms [896.02 ms, 907.68 ms] | 124915.3 [124447.8, 126066.2] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 3b1764d2a, 53de1eeef | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 3b1764d2a, 53de1eeef | 1d4a7198d9949951, 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 3b1764d2a, 53de1eeef | 19b493e428f16a69, 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 3b1764d2a, 53de1eeef | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 3b1764d2a, 53de1eeef | 463949e032392419, 5a187045a928bbf6 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 3b1764d2a, 53de1eeef | fae4f8e620c2c34e, fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 3b1764d2a, 53de1eeef | a77809465b3e7c18, e25bd76ab4a487de |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | 3b1764d2a, 53de1eeef | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 3b1764d2a, 53de1eeef | 0cff4c3b39afeb0d, ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 3b1764d2a, 53de1eeef | 1cfc7f010c52d6b3 |
