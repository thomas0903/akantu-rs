Suite `models`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | mesh | after | 5 | 1 | 12.11 ms [12.06 ms, 12.13 ms] | 12.11 ms [12.06 ms, 12.13 ms] | 1729.7 [1722.3, 1733.6] | 0.998 [0.994, 1.002] | same |
| contact_implicit_static_quad4 | mesh | before | 5 | 1 | 12.13 ms [12.08 ms, 12.15 ms] | 12.13 ms [12.08 ms, 12.15 ms] | 1732.6 [1725.7, 1736.4] | – | same |
| contact_implicit_static_quad4 | init | after | 5 | 1 | 15.12 ms [14.98 ms, 15.17 ms] | 15.12 ms [14.98 ms, 15.17 ms] | 2160.0 [2140.0, 2167.9] | 1.004 [0.994, 1.009] | same |
| contact_implicit_static_quad4 | init | before | 5 | 1 | 15.06 ms [15.04 ms, 15.09 ms] | 15.06 ms [15.04 ms, 15.09 ms] | 2150.7 [2148.7, 2155.6] | – | same |
| contact_implicit_static_quad4 | solve_step | after | 5 | 5 | 1.331 s [1.329 s, 1.340 s] | 266.16 ms [265.90 ms, 267.95 ms] | 38022.3 [37985.5, 38277.9] | 0.607 [0.606, 0.610] | same |
| contact_implicit_static_quad4 | solve_step | before | 5 | 5 | 2.193 s [2.189 s, 2.197 s] | 438.66 ms [437.84 ms, 439.31 ms] | 62665.9 [62548.1, 62758.7] | – | same |
| embedded_rebar_tet4 | mesh | after | 5 | 1 | 29.76 ms [29.50 ms, 29.81 ms] | 29.76 ms [29.50 ms, 29.81 ms] | 1368.5 [1356.4, 1370.4] | 0.995 [0.982, 1.006] | same |
| embedded_rebar_tet4 | mesh | before | 5 | 1 | 29.82 ms [29.59 ms, 30.34 ms] | 29.82 ms [29.59 ms, 30.34 ms] | 1370.9 [1360.6, 1394.9] | – | same |
| embedded_rebar_tet4 | init | after | 5 | 1 | 1.529 s [1.527 s, 1.573 s] | 1.529 s [1.527 s, 1.573 s] | 70311.7 [70214.0, 72309.2] | 0.998 [0.995, 1.024] | same |
| embedded_rebar_tet4 | init | before | 5 | 1 | 1.534 s [1.532 s, 1.536 s] | 1.534 s [1.532 s, 1.536 s] | 70542.4 [70433.8, 70631.2] | – | same |
| embedded_rebar_tet4 | intersect | after | 5 | 5 | 7.744 s [7.574 s, 7.753 s] | 1.549 s [1.515 s, 1.551 s] | 71205.9 [69642.2, 71293.5] | 1.003 [0.979, 1.006] | same |
| embedded_rebar_tet4 | intersect | before | 5 | 5 | 7.718 s [7.708 s, 7.749 s] | 1.544 s [1.542 s, 1.550 s] | 70972.7 [70877.6, 71256.6] | – | same |
| embedded_rebar_tet4 | solve_step | after | 5 | 1 | 1.289 s [1.268 s, 1.294 s] | 1.289 s [1.268 s, 1.294 s] | 59260.1 [58278.4, 59482.7] | 0.286 [0.282, 0.287] | same |
| embedded_rebar_tet4 | solve_step | before | 5 | 1 | 4.500 s [4.491 s, 4.508 s] | 4.500 s [4.491 s, 4.508 s] | 206878.3 [206459.8, 207263.1] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | after | 5 | 20 | 37.01 ms [36.90 ms, 37.36 ms] | 1.85 ms [1.85 ms, 1.87 ms] | 85.1 [84.8, 85.9] | 0.993 [0.990, 1.003] | same |
| embedded_rebar_tet4 | assemble_internal_forces | before | 5 | 20 | 37.23 ms [37.18 ms, 37.36 ms] | 1.86 ms [1.86 ms, 1.87 ms] | 85.6 [85.5, 85.9] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | after | 5 | 5 | 615.17 ms [612.95 ms, 616.00 ms] | 123.03 ms [122.59 ms, 123.20 ms] | 5656.7 [5636.3, 5664.4] | 0.998 [0.996, 1.006] | same |
| embedded_rebar_tet4 | assemble_stiffness | before | 5 | 5 | 615.62 ms [611.29 ms, 617.45 ms] | 123.12 ms [122.26 ms, 123.49 ms] | 5660.9 [5621.1, 5677.7] | – | same |
| heat_implicit_hex8_kvar | mesh | after | 5 | 1 | 5.01 ms [4.96 ms, 5.05 ms] | 5.01 ms [4.96 ms, 5.05 ms] | 2897.6 [2870.9, 2920.1] | 1.006 [0.995, 1.017] | same |
| heat_implicit_hex8_kvar | mesh | before | 5 | 1 | 4.96 ms [4.95 ms, 5.00 ms] | 4.96 ms [4.95 ms, 5.00 ms] | 2873.3 [2864.6, 2896.4] | – | same |
| heat_implicit_hex8_kvar | init | after | 5 | 1 | 1.08 ms [1.07 ms, 1.08 ms] | 1.08 ms [1.07 ms, 1.08 ms] | 623.3 [620.4, 627.3] | 0.995 [0.979, 1.021] | same |
| heat_implicit_hex8_kvar | init | before | 5 | 1 | 1.08 ms [1.06 ms, 1.11 ms] | 1.08 ms [1.06 ms, 1.11 ms] | 624.4 [611.1, 640.6] | – | same |
| heat_implicit_hex8_kvar | solve_step | after | 5 | 5 | 818.27 ms [817.27 ms, 818.81 ms] | 163.65 ms [163.45 ms, 163.76 ms] | 94706.7 [94591.8, 94769.3] | 0.567 [0.566, 0.569] | same |
| heat_implicit_hex8_kvar | solve_step | before | 5 | 5 | 1.445 s [1.437 s, 1.446 s] | 288.91 ms [287.47 ms, 289.21 ms] | 167195.7 [166360.2, 167366.1] | – | same |
| heat_static_tri6 | mesh | after | 5 | 1 | 53.32 ms [53.11 ms, 53.49 ms] | 53.32 ms [53.11 ms, 53.49 ms] | 2203.5 [2194.5, 2210.4] | 1.003 [0.999, 1.006] | same |
| heat_static_tri6 | mesh | before | 5 | 1 | 53.18 ms [53.03 ms, 53.37 ms] | 53.18 ms [53.03 ms, 53.37 ms] | 2197.6 [2191.4, 2205.5] | – | same |
| heat_static_tri6 | init | after | 5 | 1 | 7.30 ms [7.25 ms, 7.34 ms] | 7.30 ms [7.25 ms, 7.34 ms] | 301.6 [299.5, 303.4] | 0.987 [0.982, 1.006] | same |
| heat_static_tri6 | init | before | 5 | 1 | 7.36 ms [7.25 ms, 7.43 ms] | 7.36 ms [7.25 ms, 7.43 ms] | 304.2 [299.8, 307.0] | – | same |
| heat_static_tri6 | assemble_conductivity | after | 5 | 5 | 210.36 ms [209.75 ms, 211.84 ms] | 42.07 ms [41.95 ms, 42.37 ms] | 1738.5 [1733.5, 1750.8] | 1.005 [0.986, 1.009] | same |
| heat_static_tri6 | assemble_conductivity | before | 5 | 5 | 210.03 ms [208.56 ms, 212.95 ms] | 42.01 ms [41.71 ms, 42.59 ms] | 1735.8 [1723.6, 1759.9] | – | same |
| heat_static_tri6 | solve_step | after | 5 | 1 | 2.052 s [2.027 s, 2.077 s] | 2.052 s [2.027 s, 2.077 s] | 84794.0 [83767.3, 85818.4] | 0.432 [0.429, 0.438] | same |
| heat_static_tri6 | solve_step | before | 5 | 1 | 4.745 s [4.720 s, 4.783 s] | 4.745 s [4.720 s, 4.783 s] | 196069.9 [195058.1, 197652.0] | – | same |
| phasefield_staggered_tet4 | mesh | after | 5 | 1 | 9.29 ms [9.26 ms, 9.37 ms] | 9.29 ms [9.26 ms, 9.37 ms] | 1549.0 [1543.8, 1561.8] | 0.998 [0.990, 1.005] | same |
| phasefield_staggered_tet4 | mesh | before | 5 | 1 | 9.33 ms [9.29 ms, 9.38 ms] | 9.33 ms [9.29 ms, 9.38 ms] | 1555.7 [1548.3, 1563.0] | – | same |
| phasefield_staggered_tet4 | init | after | 5 | 1 | 3.26 ms [3.23 ms, 3.32 ms] | 3.26 ms [3.23 ms, 3.32 ms] | 543.7 [539.0, 553.8] | 1.005 [0.995, 1.016] | same |
| phasefield_staggered_tet4 | init | before | 5 | 1 | 3.25 ms [3.24 ms, 3.28 ms] | 3.25 ms [3.24 ms, 3.28 ms] | 541.5 [540.5, 547.0] | – | same |
| phasefield_staggered_tet4 | solid_solve | after | 5 | 4 | 742.05 ms [737.05 ms, 742.62 ms] | 185.51 ms [184.26 ms, 185.66 ms] | 30918.8 [30710.5, 30942.5] | 0.310 [0.309, 0.311] | same |
| phasefield_staggered_tet4 | solid_solve | before | 5 | 4 | 2.392 s [2.382 s, 2.399 s] | 597.95 ms [595.59 ms, 599.81 ms] | 99658.4 [99264.7, 99969.2] | – | same |
| phasefield_staggered_tet4 | strain_transfer | after | 5 | 4 | 703.0 µs [699.0 µs, 710.0 µs] | 175.8 µs [174.8 µs, 177.5 µs] | 29.3 [29.1, 29.6] | 1.032 [1.014, 1.043] | same |
| phasefield_staggered_tet4 | strain_transfer | before | 5 | 4 | 682.0 µs [677.0 µs, 691.0 µs] | 170.5 µs [169.2 µs, 172.8 µs] | 28.4 [28.2, 28.8] | – | same |
| phasefield_staggered_tet4 | phase_solve | after | 5 | 4 | 101.08 ms [100.39 ms, 101.73 ms] | 25.27 ms [25.10 ms, 25.43 ms] | 4211.8 [4183.0, 4238.7] | 0.667 [0.660, 0.672] | same |
| phasefield_staggered_tet4 | phase_solve | before | 5 | 4 | 151.57 ms [151.38 ms, 152.02 ms] | 37.89 ms [37.84 ms, 38.01 ms] | 6315.6 [6307.4, 6334.3] | – | same |
| phasefield_staggered_tet4 | damage_transfer | after | 5 | 4 | 2.31 ms [2.31 ms, 2.33 ms] | 577.5 µs [576.2 µs, 581.5 µs] | 96.2 [96.0, 96.9] | 1.000 [0.986, 1.009] | same |
| phasefield_staggered_tet4 | damage_transfer | before | 5 | 4 | 2.32 ms [2.29 ms, 2.35 ms] | 580.8 µs [572.2 µs, 587.8 µs] | 96.8 [95.4, 98.0] | – | same |
| phasefield_staggered_tet4 | internal_forces | after | 5 | 4 | 1.92 ms [1.91 ms, 1.93 ms] | 478.8 µs [478.2 µs, 481.5 µs] | 79.8 [79.7, 80.2] | 0.978 [0.972, 0.980] | same |
| phasefield_staggered_tet4 | internal_forces | before | 5 | 4 | 1.96 ms [1.95 ms, 1.98 ms] | 489.5 µs [488.2 µs, 495.3 µs] | 81.6 [81.4, 82.5] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | after | 5 | 5 | 139.20 ms [138.82 ms, 142.35 ms] | 27.84 ms [27.76 ms, 28.47 ms] | 4640.1 [4627.2, 4745.1] | 1.292 [1.283, 1.322] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | before | 5 | 5 | 107.70 ms [107.35 ms, 108.21 ms] | 21.54 ms [21.47 ms, 21.64 ms] | 3590.1 [3578.4, 3607.0] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | after | 5 | 5 | 26.38 ms [26.23 ms, 26.41 ms] | 5.28 ms [5.25 ms, 5.28 ms] | 879.4 [874.4, 880.2] | 0.998 [0.996, 1.005] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | before | 5 | 5 | 26.30 ms [26.28 ms, 26.52 ms] | 5.26 ms [5.26 ms, 5.30 ms] | 876.6 [876.0, 884.1] | – | same |
| phasefield_staggered_tri3 | mesh | after | 5 | 1 | 60.03 ms [59.97 ms, 60.22 ms] | 60.03 ms [59.97 ms, 60.22 ms] | 1172.6 [1171.3, 1176.2] | 1.000 [0.998, 1.002] | same |
| phasefield_staggered_tri3 | mesh | before | 5 | 1 | 60.06 ms [59.85 ms, 60.26 ms] | 60.06 ms [59.85 ms, 60.26 ms] | 1173.1 [1168.9, 1177.0] | – | same |
| phasefield_staggered_tri3 | init | after | 5 | 1 | 22.40 ms [22.20 ms, 22.71 ms] | 22.40 ms [22.20 ms, 22.71 ms] | 437.4 [433.6, 443.6] | 1.002 [0.992, 1.017] | same |
| phasefield_staggered_tri3 | init | before | 5 | 1 | 22.35 ms [22.09 ms, 22.45 ms] | 22.35 ms [22.09 ms, 22.45 ms] | 436.5 [431.5, 438.5] | – | same |
| phasefield_staggered_tri3 | solid_solve | after | 5 | 4 | 3.659 s [3.641 s, 3.661 s] | 914.75 ms [910.19 ms, 915.23 ms] | 17866.2 [17777.2, 17875.5] | 0.303 [0.301, 0.303] | same |
| phasefield_staggered_tri3 | solid_solve | before | 5 | 4 | 12.097 s [12.077 s, 12.126 s] | 3.024 s [3.019 s, 3.032 s] | 59069.3 [58967.8, 59210.8] | – | same |
| phasefield_staggered_tri3 | strain_transfer | after | 5 | 4 | 4.25 ms [4.24 ms, 4.30 ms] | 1.06 ms [1.06 ms, 1.08 ms] | 20.8 [20.7, 21.0] | 1.059 [1.051, 1.071] | same |
| phasefield_staggered_tri3 | strain_transfer | before | 5 | 4 | 4.03 ms [3.96 ms, 4.04 ms] | 1.01 ms [989.0 µs, 1.01 ms] | 19.7 [19.3, 19.7] | – | same |
| phasefield_staggered_tri3 | phase_solve | after | 5 | 4 | 1.067 s [1.066 s, 1.068 s] | 266.73 ms [266.61 ms, 267.12 ms] | 5209.6 [5207.2, 5217.2] | 0.480 [0.480, 0.481] | same |
| phasefield_staggered_tri3 | phase_solve | before | 5 | 4 | 2.223 s [2.220 s, 2.227 s] | 555.85 ms [554.90 ms, 556.65 ms] | 10856.4 [10837.9, 10872.1] | – | same |
| phasefield_staggered_tri3 | damage_transfer | after | 5 | 4 | 18.97 ms [18.92 ms, 19.05 ms] | 4.74 ms [4.73 ms, 4.76 ms] | 92.6 [92.4, 93.0] | 1.002 [0.997, 1.003] | same |
| phasefield_staggered_tri3 | damage_transfer | before | 5 | 4 | 18.97 ms [18.88 ms, 19.01 ms] | 4.74 ms [4.72 ms, 4.75 ms] | 92.6 [92.2, 92.8] | – | same |
| phasefield_staggered_tri3 | internal_forces | after | 5 | 4 | 10.45 ms [10.36 ms, 10.50 ms] | 2.61 ms [2.59 ms, 2.62 ms] | 51.0 [50.6, 51.2] | 0.994 [0.985, 1.000] | same |
| phasefield_staggered_tri3 | internal_forces | before | 5 | 4 | 10.51 ms [10.50 ms, 10.55 ms] | 2.63 ms [2.62 ms, 2.64 ms] | 51.3 [51.3, 51.5] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | after | 5 | 5 | 329.85 ms [318.27 ms, 330.33 ms] | 65.97 ms [63.65 ms, 66.07 ms] | 1288.5 [1243.2, 1290.3] | 0.986 [0.947, 0.994] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | before | 5 | 5 | 332.71 ms [330.81 ms, 344.24 ms] | 66.54 ms [66.16 ms, 68.85 ms] | 1299.7 [1292.2, 1344.7] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | after | 5 | 5 | 178.39 ms [177.21 ms, 194.24 ms] | 35.68 ms [35.44 ms, 38.85 ms] | 696.8 [692.2, 758.7] | 1.000 [0.993, 1.085] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | before | 5 | 5 | 178.97 ms [177.94 ms, 179.15 ms] | 35.79 ms [35.59 ms, 35.83 ms] | 699.1 [695.1, 699.8] | – | same |
| structural_beam3_frame_static | mesh | after | 5 | 1 | 355.0 µs [341.0 µs, 365.0 µs] | 355.0 µs [341.0 µs, 365.0 µs] | 58.3 [56.0, 60.0] | 1.017 [0.872, 1.061] | same |
| structural_beam3_frame_static | mesh | before | 5 | 1 | 349.0 µs [344.0 µs, 391.0 µs] | 349.0 µs [344.0 µs, 391.0 µs] | 57.4 [56.5, 64.3] | – | same |
| structural_beam3_frame_static | init | after | 5 | 1 | 31.19 ms [31.15 ms, 31.27 ms] | 31.19 ms [31.15 ms, 31.27 ms] | 5126.1 [5120.0, 5140.2] | 1.009 [0.993, 1.010] | same |
| structural_beam3_frame_static | init | before | 5 | 1 | 30.99 ms [30.87 ms, 31.38 ms] | 30.99 ms [30.87 ms, 31.38 ms] | 5094.3 [5074.5, 5157.1] | – | same |
| structural_beam3_frame_static | assemble_stiffness | after | 5 | 5 | 183.88 ms [183.06 ms, 190.71 ms] | 36.78 ms [36.61 ms, 38.14 ms] | 6044.8 [6017.6, 6269.3] | 0.985 [0.939, 0.990] | same |
| structural_beam3_frame_static | assemble_stiffness | before | 5 | 5 | 191.67 ms [185.24 ms, 195.78 ms] | 38.33 ms [37.05 ms, 39.16 ms] | 6300.7 [6089.3, 6435.9] | – | same |
| structural_beam3_frame_static | solve_step | after | 5 | 1 | 1.406 s [1.394 s, 1.431 s] | 1.406 s [1.394 s, 1.431 s] | 231175.2 [229119.8, 235193.5] | 0.224 [0.223, 0.228] | same |
| structural_beam3_frame_static | solve_step | before | 5 | 1 | 6.273 s [6.263 s, 6.281 s] | 6.273 s [6.263 s, 6.281 s] | 1031141.8 [1029378.7, 1032428.0] | – | same |
| structural_dkt18_plate_static | mesh | after | 5 | 1 | 301.0 µs [293.0 µs, 315.0 µs] | 301.0 µs [293.0 µs, 315.0 µs] | 41.8 [40.7, 43.8] | 1.000 [0.930, 1.057] | same |
| structural_dkt18_plate_static | mesh | before | 5 | 1 | 305.0 µs [294.0 µs, 315.0 µs] | 305.0 µs [294.0 µs, 315.0 µs] | 42.4 [40.8, 43.8] | – | same |
| structural_dkt18_plate_static | init | after | 5 | 1 | 74.61 ms [74.56 ms, 74.86 ms] | 74.61 ms [74.56 ms, 74.86 ms] | 10362.5 [10355.4, 10397.4] | 0.995 [0.993, 0.998] | same |
| structural_dkt18_plate_static | init | before | 5 | 1 | 75.00 ms [74.95 ms, 75.12 ms] | 75.00 ms [74.95 ms, 75.12 ms] | 10416.2 [10410.0, 10433.5] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | after | 5 | 5 | 771.88 ms [767.07 ms, 778.07 ms] | 154.38 ms [153.41 ms, 155.61 ms] | 21441.1 [21307.5, 21613.1] | 0.999 [0.985, 1.004] | same |
| structural_dkt18_plate_static | assemble_stiffness | before | 5 | 5 | 774.56 ms [769.18 ms, 780.22 ms] | 154.91 ms [153.84 ms, 156.04 ms] | 21515.5 [21366.1, 21672.9] | – | same |
| structural_dkt18_plate_static | solve_step | after | 5 | 1 | 1.022 s [1.017 s, 1.033 s] | 1.022 s [1.017 s, 1.033 s] | 141946.7 [141278.6, 143413.8] | 0.364 [0.360, 0.367] | same |
| structural_dkt18_plate_static | solve_step | before | 5 | 1 | 2.810 s [2.806 s, 2.826 s] | 2.810 s [2.806 s, 2.826 s] | 390316.9 [389728.9, 392469.2] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 24f73b94e, f204c7196 | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 24f73b94e, f204c7196 | 9e3b18c54880d2b3 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 24f73b94e, f204c7196 | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 24f73b94e, f204c7196 | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 24f73b94e, f204c7196 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 24f73b94e, f204c7196 | a77809465b3e7c18 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 24f73b94e, f204c7196 | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 24f73b94e, f204c7196 | 1cfc7f010c52d6b3 |
