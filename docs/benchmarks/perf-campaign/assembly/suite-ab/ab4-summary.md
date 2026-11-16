Suite `solid`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| setup_hex8 | mesh | after | 5 | 1 | 1.55 ms [1.53 ms, 1.63 ms] | 1.55 ms [1.53 ms, 1.63 ms] | 111.9 [110.6, 118.2] | 1.003 [0.956, 1.059] | same |
| setup_hex8 | mesh | before | 5 | 1 | 1.55 ms [1.53 ms, 1.62 ms] | 1.55 ms [1.53 ms, 1.62 ms] | 111.9 [110.9, 117.1] | – | same |
| setup_hex8 | init_explicit | after | 5 | 1 | 59.51 ms [59.09 ms, 59.93 ms] | 59.51 ms [59.09 ms, 59.93 ms] | 4304.5 [4274.5, 4335.0] | 0.957 [0.945, 0.987] | same |
| setup_hex8 | init_explicit | before | 5 | 1 | 62.59 ms [59.85 ms, 62.73 ms] | 62.59 ms [59.85 ms, 62.73 ms] | 4528.0 [4329.2, 4537.9] | – | same |
| setup_hex8 | shape_functions | after | 5 | 3 | 74.62 ms [72.30 ms, 80.14 ms] | 24.87 ms [24.10 ms, 26.71 ms] | 1799.4 [1743.2, 1932.4] | 0.994 [0.973, 1.123] | same |
| setup_hex8 | shape_functions | before | 5 | 3 | 74.31 ms [71.35 ms, 75.36 ms] | 24.77 ms [23.78 ms, 25.12 ms] | 1791.8 [1720.4, 1817.1] | – | same |
| setup_hex8 | mass_lumped | after | 5 | 3 | 16.91 ms [16.14 ms, 18.39 ms] | 5.64 ms [5.38 ms, 6.13 ms] | 407.7 [389.1, 443.4] | 0.848 [0.804, 0.941] | same |
| setup_hex8 | mass_lumped | before | 5 | 3 | 20.08 ms [19.54 ms, 20.41 ms] | 6.69 ms [6.51 ms, 6.80 ms] | 484.1 [471.3, 492.2] | – | same |
| setup_hex8 | init_implicit | after | 5 | 1 | 49.65 ms [46.80 ms, 50.19 ms] | 49.65 ms [46.80 ms, 50.19 ms] | 3591.8 [3385.2, 3630.9] | 0.948 [0.871, 0.989] | same |
| setup_hex8 | init_implicit | before | 5 | 1 | 50.83 ms [50.18 ms, 53.71 ms] | 50.83 ms [50.18 ms, 53.71 ms] | 3676.6 [3630.0, 3885.0] | – | same |
| setup_hex8 | mass_consistent | after | 5 | 3 | 326.14 ms [322.20 ms, 340.37 ms] | 108.71 ms [107.40 ms, 113.46 ms] | 7864.2 [7769.1, 8207.3] | 0.370 [0.363, 0.383] | same |
| setup_hex8 | mass_consistent | before | 5 | 3 | 881.77 ms [866.42 ms, 900.59 ms] | 293.92 ms [288.81 ms, 300.20 ms] | 21261.9 [20891.8, 21715.7] | – | same |
| setup_hex8 | stiffness_assembly | after | 5 | 3 | 1.486 s [1.483 s, 1.494 s] | 495.48 ms [494.38 ms, 498.09 ms] | 35841.9 [35762.5, 36030.5] | 0.707 [0.699, 0.708] | same |
| setup_hex8 | stiffness_assembly | before | 5 | 3 | 2.109 s [2.102 s, 2.121 s] | 703.02 ms [700.83 ms, 707.06 ms] | 50855.2 [50696.6, 51147.5] | – | same |
| setup_tet10 | mesh | after | 5 | 1 | 1.32 ms [1.30 ms, 1.37 ms] | 1.32 ms [1.30 ms, 1.37 ms] | 127.1 [125.0, 132.2] | 1.040 [1.022, 1.068] | same |
| setup_tet10 | mesh | before | 5 | 1 | 1.27 ms [1.25 ms, 1.29 ms] | 1.27 ms [1.25 ms, 1.29 ms] | 122.1 [120.5, 124.4] | – | same |
| setup_tet10 | init_explicit | after | 5 | 1 | 29.48 ms [29.30 ms, 29.91 ms] | 29.48 ms [29.30 ms, 29.91 ms] | 2843.4 [2825.8, 2884.6] | 0.940 [0.936, 0.947] | same |
| setup_tet10 | init_explicit | before | 5 | 1 | 31.42 ms [31.31 ms, 31.65 ms] | 31.42 ms [31.31 ms, 31.65 ms] | 3030.4 [3019.6, 3052.3] | – | same |
| setup_tet10 | shape_functions | after | 5 | 3 | 38.17 ms [36.53 ms, 39.36 ms] | 12.72 ms [12.18 ms, 13.12 ms] | 1227.2 [1174.4, 1265.4] | 0.989 [0.969, 1.040] | same |
| setup_tet10 | shape_functions | before | 5 | 3 | 38.55 ms [35.13 ms, 39.13 ms] | 12.85 ms [11.71 ms, 13.04 ms] | 1239.3 [1129.5, 1258.2] | – | same |
| setup_tet10 | mass_lumped | after | 5 | 3 | 4.11 ms [3.61 ms, 4.22 ms] | 1.37 ms [1.20 ms, 1.41 ms] | 132.1 [115.9, 135.6] | 0.930 [0.908, 0.974] | same |
| setup_tet10 | mass_lumped | before | 5 | 3 | 4.30 ms [3.91 ms, 4.53 ms] | 1.43 ms [1.30 ms, 1.51 ms] | 138.4 [125.7, 145.7] | – | same |
| setup_tet10 | init_implicit | after | 5 | 1 | 23.54 ms [22.55 ms, 24.41 ms] | 23.54 ms [22.55 ms, 24.41 ms] | 2270.1 [2174.7, 2354.3] | 0.917 [0.852, 0.928] | same |
| setup_tet10 | init_implicit | before | 5 | 1 | 26.40 ms [25.41 ms, 26.91 ms] | 26.40 ms [25.41 ms, 26.91 ms] | 2546.5 [2450.5, 2595.3] | – | same |
| setup_tet10 | mass_consistent | after | 5 | 3 | 259.69 ms [258.44 ms, 268.02 ms] | 86.56 ms [86.15 ms, 89.34 ms] | 8349.2 [8309.0, 8617.0] | 0.248 [0.245, 0.256] | same |
| setup_tet10 | mass_consistent | before | 5 | 3 | 1.048 s [1.041 s, 1.058 s] | 349.34 ms [346.94 ms, 352.82 ms] | 33694.0 [33462.6, 34030.2] | – | same |
| setup_tet10 | stiffness_assembly | after | 5 | 3 | 923.12 ms [920.20 ms, 931.04 ms] | 307.71 ms [306.73 ms, 310.35 ms] | 29678.5 [29584.5, 29933.2] | 0.533 [0.528, 0.536] | same |
| setup_tet10 | stiffness_assembly | before | 5 | 3 | 1.731 s [1.727 s, 1.743 s] | 576.89 ms [575.63 ms, 581.02 ms] | 55641.7 [55519.6, 56039.4] | – | same |
| setup_tet4 | mesh | after | 5 | 1 | 5.50 ms [5.45 ms, 5.59 ms] | 5.50 ms [5.45 ms, 5.59 ms] | 33.9 [33.6, 34.5] | 0.974 [0.963, 1.021] | same |
| setup_tet4 | mesh | before | 5 | 1 | 5.60 ms [5.38 ms, 5.81 ms] | 5.60 ms [5.38 ms, 5.81 ms] | 34.5 [33.2, 35.8] | – | same |
| setup_tet4 | init_explicit | after | 5 | 1 | 80.46 ms [79.94 ms, 81.15 ms] | 80.46 ms [79.94 ms, 81.15 ms] | 496.7 [493.5, 500.9] | 0.949 [0.920, 0.972] | same |
| setup_tet4 | init_explicit | before | 5 | 1 | 84.38 ms [83.08 ms, 86.90 ms] | 84.38 ms [83.08 ms, 86.90 ms] | 520.9 [512.8, 536.4] | – | same |
| setup_tet4 | shape_functions | after | 5 | 3 | 94.91 ms [93.94 ms, 95.93 ms] | 31.64 ms [31.31 ms, 31.98 ms] | 195.3 [193.3, 197.4] | 0.986 [0.958, 0.997] | same |
| setup_tet4 | shape_functions | before | 5 | 3 | 96.80 ms [96.21 ms, 98.04 ms] | 32.27 ms [32.07 ms, 32.68 ms] | 199.2 [198.0, 201.7] | – | same |
| setup_tet4 | mass_lumped | after | 5 | 3 | 63.51 ms [62.97 ms, 63.74 ms] | 21.17 ms [20.99 ms, 21.25 ms] | 130.7 [129.6, 131.2] | 0.982 [0.972, 1.001] | same |
| setup_tet4 | mass_lumped | before | 5 | 3 | 64.56 ms [63.26 ms, 65.56 ms] | 21.52 ms [21.09 ms, 21.85 ms] | 132.8 [130.2, 134.9] | – | same |
| setup_tet4 | init_implicit | after | 5 | 1 | 73.48 ms [72.97 ms, 73.95 ms] | 73.48 ms [72.97 ms, 73.95 ms] | 453.6 [450.4, 456.5] | 0.945 [0.938, 0.974] | same |
| setup_tet4 | init_implicit | before | 5 | 1 | 78.02 ms [74.94 ms, 78.58 ms] | 78.02 ms [74.94 ms, 78.58 ms] | 481.6 [462.6, 485.1] | – | same |
| setup_tet4 | mass_consistent | after | 5 | 3 | 622.12 ms [619.66 ms, 623.75 ms] | 207.37 ms [206.55 ms, 207.92 ms] | 1280.1 [1275.0, 1283.4] | 0.308 [0.305, 0.312] | same |
| setup_tet4 | mass_consistent | before | 5 | 3 | 2.018 s [1.997 s, 2.043 s] | 672.60 ms [665.50 ms, 680.90 ms] | 4151.9 [4108.1, 4203.1] | – | same |
| setup_tet4 | stiffness_assembly | after | 5 | 3 | 1.087 s [1.085 s, 1.090 s] | 362.33 ms [361.63 ms, 363.31 ms] | 2236.6 [2232.3, 2242.7] | 0.440 [0.436, 0.443] | same |
| setup_tet4 | stiffness_assembly | before | 5 | 3 | 2.473 s [2.461 s, 2.489 s] | 824.40 ms [820.37 ms, 829.58 ms] | 5088.9 [5064.0, 5120.9] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 0aa3207e9, 78a330615 | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 0aa3207e9, 78a330615 | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 0aa3207e9, 78a330615 | 562b4d2b6ad29d67 |
