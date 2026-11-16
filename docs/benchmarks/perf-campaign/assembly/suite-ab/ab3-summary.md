Suite `solid`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | after | 5 | 1 | 358.1 µs [332.3 µs, 413.0 µs] | 358.1 µs [332.3 µs, 413.0 µs] | 99.5 [92.3, 114.7] | 0.996 [0.854, 1.140] | same |
| cohesive_intrinsic_implicit | mesh | before | 5 | 1 | 376.0 µs [348.6 µs, 413.2 µs] | 376.0 µs [348.6 µs, 413.2 µs] | 104.4 [96.8, 114.8] | – | same |
| cohesive_intrinsic_implicit | init | after | 5 | 1 | 10.61 ms [9.87 ms, 11.68 ms] | 10.61 ms [9.87 ms, 11.68 ms] | 2948.3 [2740.7, 3243.2] | 0.853 [0.748, 1.037] | same |
| cohesive_intrinsic_implicit | init | before | 5 | 1 | 12.51 ms [11.12 ms, 13.20 ms] | 12.51 ms [11.12 ms, 13.20 ms] | 3475.8 [3089.7, 3667.7] | – | same |
| cohesive_intrinsic_implicit | solve_step | after | 5 | 30 | 6.100 s [5.963 s, 7.061 s] | 203.34 ms [198.78 ms, 235.37 ms] | 56483.0 [55217.0, 65381.8] | 0.727 [0.721, 0.764] | same |
| cohesive_intrinsic_implicit | solve_step | before | 5 | 30 | 8.334 s [8.255 s, 9.246 s] | 277.80 ms [275.17 ms, 308.21 ms] | 77166.4 [76435.3, 85614.5] | – | same |
| implicit_dynamic | mesh | after | 5 | 1 | 599.5 µs [591.6 µs, 689.2 µs] | 599.5 µs [591.6 µs, 689.2 µs] | 36.4 [35.9, 41.9] | 0.890 [0.871, 1.203] | same |
| implicit_dynamic | mesh | before | 5 | 1 | 675.1 µs [572.7 µs, 693.0 µs] | 675.1 µs [572.7 µs, 693.0 µs] | 41.0 [34.8, 42.1] | – | same |
| implicit_dynamic | init | after | 5 | 1 | 10.88 ms [10.41 ms, 11.55 ms] | 10.88 ms [10.41 ms, 11.55 ms] | 660.6 [632.3, 701.7] | 0.971 [0.953, 1.062] | same |
| implicit_dynamic | init | before | 5 | 1 | 11.03 ms [10.71 ms, 12.10 ms] | 11.03 ms [10.71 ms, 12.10 ms] | 669.8 [650.3, 735.2] | – | same |
| implicit_dynamic | mass_consistent | after | 5 | 1 | 125.74 ms [121.55 ms, 133.35 ms] | 125.74 ms [121.55 ms, 133.35 ms] | 7637.0 [7382.7, 8099.4] | 0.693 [0.659, 0.724] | same |
| implicit_dynamic | mass_consistent | before | 5 | 1 | 181.34 ms [175.62 ms, 187.23 ms] | 181.34 ms [175.62 ms, 187.23 ms] | 11014.3 [10667.0, 11372.4] | – | same |
| implicit_dynamic | solve_step | after | 5 | 60 | 1.184 s [1.177 s, 1.240 s] | 19.74 ms [19.61 ms, 20.67 ms] | 1199.0 [1191.2, 1255.3] | 0.618 [0.608, 0.637] | same |
| implicit_dynamic | solve_step | before | 5 | 60 | 1.909 s [1.901 s, 2.041 s] | 31.81 ms [31.69 ms, 34.01 ms] | 1932.2 [1924.7, 2065.7] | – | same |
| implicit_dynamic | predictor | after | 5 | 60 | 9.5 µs [8.7 µs, 10.6 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.821 [0.669, 1.111] | same |
| implicit_dynamic | predictor | before | 5 | 60 | 12.0 µs [7.9 µs, 15.8 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | residual | after | 5 | 180 | 182.98 ms [182.13 ms, 192.25 ms] | 1.02 ms [1.01 ms, 1.07 ms] | 61.7 [61.5, 64.9] | 0.993 [0.981, 1.011] | same |
| implicit_dynamic | residual | before | 5 | 180 | 184.20 ms [182.30 ms, 193.74 ms] | 1.02 ms [1.01 ms, 1.08 ms] | 62.2 [61.5, 65.4] | – | same |
| implicit_dynamic | matrix | after | 5 | 420 | 127.1 µs [121.6 µs, 129.1 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.857 [0.830, 0.866] | same |
| implicit_dynamic | matrix | before | 5 | 420 | 148.3 µs [141.6 µs, 154.5 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | after | 5 | 120 | 25.1 µs [19.3 µs, 32.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.988 [0.694, 1.186] | same |
| implicit_dynamic | corrector | before | 5 | 120 | 27.1 µs [24.7 µs, 28.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | step_hooks | after | 5 | 120 | 10.20 ms [10.01 ms, 10.60 ms] | 85.0 µs [83.5 µs, 88.3 µs] | 5.2 [5.1, 5.4] | 0.961 [0.921, 0.981] | same |
| implicit_dynamic | step_hooks | before | 5 | 120 | 10.59 ms [10.35 ms, 11.51 ms] | 88.2 µs [86.3 µs, 95.9 µs] | 5.4 [5.2, 5.8] | – | same |
| implicit_dynamic | step_rest | after | 5 | 60 | 991.67 ms [984.34 ms, 1.037 s] | 16.53 ms [16.41 ms, 17.28 ms] | 1003.9 [996.5, 1049.8] | 0.575 [0.565, 0.595] | same |
| implicit_dynamic | step_rest | before | 5 | 60 | 1.714 s [1.708 s, 1.835 s] | 28.57 ms [28.47 ms, 30.59 ms] | 1735.3 [1729.5, 1857.7] | – | same |
| setup_hex8 | mesh | after | 5 | 1 | 1.40 ms [1.36 ms, 1.59 ms] | 1.40 ms [1.36 ms, 1.59 ms] | 101.4 [98.2, 115.3] | 0.982 [0.862, 1.203] | same |
| setup_hex8 | mesh | before | 5 | 1 | 1.43 ms [1.32 ms, 1.57 ms] | 1.43 ms [1.32 ms, 1.57 ms] | 103.3 [95.8, 113.9] | – | same |
| setup_hex8 | init_explicit | after | 5 | 1 | 91.36 ms [86.68 ms, 95.41 ms] | 91.36 ms [86.68 ms, 95.41 ms] | 6608.7 [6270.4, 6901.5] | 1.004 [0.918, 1.041] | same |
| setup_hex8 | init_explicit | before | 5 | 1 | 93.23 ms [87.77 ms, 95.39 ms] | 93.23 ms [87.77 ms, 95.39 ms] | 6743.9 [6349.4, 6900.4] | – | same |
| setup_hex8 | shape_functions | after | 5 | 3 | 183.64 ms [173.26 ms, 196.75 ms] | 61.21 ms [57.75 ms, 65.58 ms] | 4428.2 [4177.9, 4744.1] | 1.076 [1.013, 1.114] | same |
| setup_hex8 | shape_functions | before | 5 | 3 | 172.52 ms [170.29 ms, 179.33 ms] | 57.51 ms [56.76 ms, 59.78 ms] | 4159.9 [4106.2, 4324.0] | – | same |
| setup_hex8 | mass_lumped | after | 5 | 3 | 25.73 ms [24.34 ms, 27.61 ms] | 8.58 ms [8.11 ms, 9.20 ms] | 620.3 [587.0, 665.8] | 1.029 [0.991, 1.065] | same |
| setup_hex8 | mass_lumped | before | 5 | 3 | 24.56 ms [24.43 ms, 25.95 ms] | 8.19 ms [8.14 ms, 8.65 ms] | 592.2 [589.1, 625.7] | – | same |
| setup_hex8 | init_implicit | after | 5 | 1 | 79.95 ms [76.83 ms, 87.00 ms] | 79.95 ms [76.83 ms, 87.00 ms] | 5783.1 [5557.8, 6293.4] | 0.973 [0.912, 1.049] | same |
| setup_hex8 | init_implicit | before | 5 | 1 | 84.22 ms [81.23 ms, 86.09 ms] | 84.22 ms [81.23 ms, 86.09 ms] | 6092.4 [5875.8, 6227.3] | – | same |
| setup_hex8 | mass_consistent | after | 5 | 3 | 5.667 s [5.630 s, 5.881 s] | 1.889 s [1.877 s, 1.960 s] | 136641.5 [135750.3, 141810.0] | 0.905 [0.866, 0.910] | same |
| setup_hex8 | mass_consistent | before | 5 | 3 | 6.259 s [6.206 s, 6.787 s] | 2.086 s [2.069 s, 2.262 s] | 150911.5 [149650.1, 163664.6] | – | same |
| setup_hex8 | stiffness_assembly | after | 5 | 3 | 2.130 s [2.090 s, 2.228 s] | 710.06 ms [696.67 ms, 742.58 ms] | 51364.0 [50395.6, 53716.7] | 0.777 [0.735, 0.783] | same |
| setup_hex8 | stiffness_assembly | before | 5 | 3 | 2.721 s [2.688 s, 3.029 s] | 907.08 ms [896.12 ms, 1.010 s] | 65616.0 [64823.4, 73040.6] | – | same |
| setup_tet10 | mesh | after | 5 | 1 | 1.18 ms [1.16 ms, 1.35 ms] | 1.18 ms [1.16 ms, 1.35 ms] | 113.5 [111.7, 130.5] | 1.051 [0.974, 1.079] | same |
| setup_tet10 | mesh | before | 5 | 1 | 1.19 ms [1.11 ms, 1.31 ms] | 1.19 ms [1.11 ms, 1.31 ms] | 114.7 [106.8, 126.7] | – | same |
| setup_tet10 | init_explicit | after | 5 | 1 | 43.02 ms [41.09 ms, 49.40 ms] | 43.02 ms [41.09 ms, 49.40 ms] | 4149.7 [3962.9, 4764.7] | 0.951 [0.856, 1.028] | same |
| setup_tet10 | init_explicit | before | 5 | 1 | 46.82 ms [42.65 ms, 50.42 ms] | 46.82 ms [42.65 ms, 50.42 ms] | 4515.4 [4113.3, 4862.8] | – | same |
| setup_tet10 | shape_functions | after | 5 | 3 | 82.97 ms [79.60 ms, 86.04 ms] | 27.66 ms [26.53 ms, 28.68 ms] | 2667.4 [2559.2, 2766.2] | 1.010 [0.995, 1.026] | same |
| setup_tet10 | shape_functions | before | 5 | 3 | 81.34 ms [79.53 ms, 85.39 ms] | 27.11 ms [26.51 ms, 28.46 ms] | 2615.0 [2556.9, 2745.4] | – | same |
| setup_tet10 | mass_lumped | after | 5 | 3 | 3.96 ms [3.50 ms, 4.26 ms] | 1.32 ms [1.17 ms, 1.42 ms] | 127.2 [112.7, 137.1] | 1.084 [0.991, 1.218] | same |
| setup_tet10 | mass_lumped | before | 5 | 3 | 3.57 ms [3.50 ms, 3.86 ms] | 1.19 ms [1.17 ms, 1.29 ms] | 114.8 [112.6, 124.1] | – | same |
| setup_tet10 | init_implicit | after | 5 | 1 | 37.26 ms [35.90 ms, 42.68 ms] | 37.26 ms [35.90 ms, 42.68 ms] | 3593.5 [3462.8, 4116.6] | 0.969 [0.896, 1.026] | same |
| setup_tet10 | init_implicit | before | 5 | 1 | 37.55 ms [37.38 ms, 44.59 ms] | 37.55 ms [37.38 ms, 44.59 ms] | 3622.0 [3605.4, 4300.6] | – | same |
| setup_tet10 | mass_consistent | after | 5 | 3 | 3.678 s [3.656 s, 3.887 s] | 1.226 s [1.219 s, 1.296 s] | 118263.5 [117540.3, 124953.7] | 0.826 [0.819, 0.831] | same |
| setup_tet10 | mass_consistent | before | 5 | 3 | 4.473 s [4.454 s, 4.676 s] | 1.491 s [1.485 s, 1.559 s] | 143801.6 [143197.0, 150333.2] | – | same |
| setup_tet10 | stiffness_assembly | after | 5 | 3 | 1.310 s [1.293 s, 1.402 s] | 436.65 ms [431.11 ms, 467.25 ms] | 42115.6 [41581.3, 45066.9] | 0.633 [0.621, 0.647] | same |
| setup_tet10 | stiffness_assembly | before | 5 | 3 | 2.094 s [2.069 s, 2.165 s] | 697.97 ms [689.75 ms, 721.79 ms] | 67319.9 [66526.8, 69617.5] | – | same |
| setup_tet4 | mesh | after | 5 | 1 | 5.11 ms [4.89 ms, 5.46 ms] | 5.11 ms [4.89 ms, 5.46 ms] | 31.5 [30.2, 33.7] | 0.992 [0.960, 1.066] | same |
| setup_tet4 | mesh | before | 5 | 1 | 5.12 ms [4.82 ms, 5.45 ms] | 5.12 ms [4.82 ms, 5.45 ms] | 31.6 [29.8, 33.6] | – | same |
| setup_tet4 | init_explicit | after | 5 | 1 | 99.66 ms [97.58 ms, 102.73 ms] | 99.66 ms [97.58 ms, 102.73 ms] | 615.2 [602.3, 634.2] | 0.924 [0.824, 0.980] | same |
| setup_tet4 | init_explicit | before | 5 | 1 | 107.42 ms [102.45 ms, 119.23 ms] | 107.42 ms [102.45 ms, 119.23 ms] | 663.1 [632.4, 736.0] | – | same |
| setup_tet4 | shape_functions | after | 5 | 3 | 173.77 ms [171.50 ms, 180.24 ms] | 57.92 ms [57.17 ms, 60.08 ms] | 357.6 [352.9, 370.9] | 1.012 [0.951, 1.017] | same |
| setup_tet4 | shape_functions | before | 5 | 3 | 174.51 ms [168.71 ms, 189.60 ms] | 58.17 ms [56.24 ms, 63.20 ms] | 359.1 [347.1, 390.1] | – | same |
| setup_tet4 | mass_lumped | after | 5 | 3 | 66.12 ms [64.85 ms, 70.56 ms] | 22.04 ms [21.62 ms, 23.52 ms] | 136.0 [133.4, 145.2] | 0.995 [0.895, 1.075] | same |
| setup_tet4 | mass_lumped | before | 5 | 3 | 66.42 ms [62.63 ms, 74.92 ms] | 22.14 ms [20.88 ms, 24.97 ms] | 136.7 [128.9, 154.2] | – | same |
| setup_tet4 | init_implicit | after | 5 | 1 | 96.74 ms [92.77 ms, 99.77 ms] | 96.74 ms [92.77 ms, 99.77 ms] | 597.1 [572.7, 615.9] | 0.951 [0.930, 1.012] | same |
| setup_tet4 | init_implicit | before | 5 | 1 | 99.41 ms [96.58 ms, 107.33 ms] | 99.41 ms [96.58 ms, 107.33 ms] | 613.7 [596.2, 662.6] | – | same |
| setup_tet4 | mass_consistent | after | 5 | 3 | 3.009 s [3.004 s, 3.136 s] | 1.003 s [1.001 s, 1.045 s] | 6192.1 [6181.1, 6453.1] | 0.702 [0.686, 0.705] | same |
| setup_tet4 | mass_consistent | before | 5 | 3 | 4.288 s [4.270 s, 4.498 s] | 1.429 s [1.423 s, 1.499 s] | 8823.9 [8786.0, 9256.0] | – | same |
| setup_tet4 | stiffness_assembly | after | 5 | 3 | 1.324 s [1.315 s, 1.359 s] | 441.26 ms [438.26 ms, 453.00 ms] | 2723.8 [2705.3, 2796.3] | 0.507 [0.497, 0.509] | same |
| setup_tet4 | stiffness_assembly | before | 5 | 3 | 2.613 s [2.582 s, 2.716 s] | 870.94 ms [860.60 ms, 905.36 ms] | 5376.2 [5312.3, 5588.6] | – | same |
| static_linear_eigen | mesh | after | 5 | 1 | 632.0 µs [581.0 µs, 676.6 µs] | 632.0 µs [581.0 µs, 676.6 µs] | 38.4 [35.3, 41.1] | 1.041 [0.825, 1.211] | same |
| static_linear_eigen | mesh | before | 5 | 1 | 602.8 µs [558.8 µs, 704.1 µs] | 602.8 µs [558.8 µs, 704.1 µs] | 36.6 [33.9, 42.8] | – | same |
| static_linear_eigen | init | after | 5 | 1 | 11.24 ms [10.57 ms, 11.46 ms] | 11.24 ms [10.57 ms, 11.46 ms] | 682.9 [641.9, 696.0] | 1.033 [0.943, 1.080] | same |
| static_linear_eigen | init | before | 5 | 1 | 10.73 ms [10.56 ms, 12.15 ms] | 10.73 ms [10.56 ms, 12.15 ms] | 651.5 [641.5, 738.0] | – | same |
| static_linear_eigen | solve_step | after | 5 | 1 | 2.214 s [2.201 s, 2.368 s] | 2.214 s [2.201 s, 2.368 s] | 134469.3 [133679.9, 143841.6] | 0.948 [0.938, 0.965] | same |
| static_linear_eigen | solve_step | before | 5 | 1 | 2.337 s [2.315 s, 2.480 s] | 2.337 s [2.315 s, 2.480 s] | 141946.7 [140596.5, 150630.0] | – | same |
| static_linear_eigen | predictor | after | 5 | 1 | 0.2 µs [0.2 µs, 0.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.051 [0.605, 1.191] | same |
| static_linear_eigen | predictor | before | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | matrix | after | 5 | 1 | 71.91 ms [67.27 ms, 80.97 ms] | 71.91 ms [67.27 ms, 80.97 ms] | 4367.4 [4085.7, 4917.7] | 0.561 [0.544, 0.603] | same |
| static_linear_eigen | matrix | before | 5 | 1 | 123.69 ms [122.99 ms, 134.29 ms] | 123.69 ms [122.99 ms, 134.29 ms] | 7513.0 [7470.2, 8156.4] | – | same |
| static_linear_eigen | corrector | after | 5 | 1 | 0.6 µs [0.4 µs, 0.7 µs] | 0.6 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.912 [0.433, 0.969] | same |
| static_linear_eigen | corrector | before | 5 | 1 | 0.8 µs [0.6 µs, 0.9 µs] | 0.8 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | step_hooks | after | 5 | 2 | 242.0 µs [160.7 µs, 256.3 µs] | 121.0 µs [80.3 µs, 128.2 µs] | 7.3 [4.9, 7.8] | 0.950 [0.646, 1.495] | same |
| static_linear_eigen | step_hooks | before | 5 | 2 | 248.6 µs [165.9 µs, 254.8 µs] | 124.3 µs [82.9 µs, 127.4 µs] | 7.6 [5.0, 7.7] | – | same |
| static_linear_eigen | step_rest | after | 5 | 1 | 2.146 s [2.132 s, 2.287 s] | 2.146 s [2.132 s, 2.287 s] | 130370.4 [129489.6, 138908.3] | 0.970 [0.960, 0.986] | same |
| static_linear_eigen | step_rest | before | 5 | 1 | 2.208 s [2.192 s, 2.345 s] | 2.208 s [2.192 s, 2.345 s] | 134126.9 [133111.1, 142459.4] | – | same |
| static_linear_eigen | stiffness_assembly | after | 5 | 3 | 74.94 ms [72.34 ms, 79.65 ms] | 24.98 ms [24.11 ms, 26.55 ms] | 1517.3 [1464.7, 1612.7] | 0.481 [0.471, 0.515] | same |
| static_linear_eigen | stiffness_assembly | before | 5 | 3 | 154.68 ms [149.80 ms, 167.16 ms] | 51.56 ms [49.93 ms, 55.72 ms] | 3131.8 [3032.9, 3384.4] | – | same |
| static_linear_eigen | analysis | after | 5 | 1 | 39.43 ms [38.50 ms, 43.72 ms] | 39.43 ms [38.50 ms, 43.72 ms] | 2395.1 [2338.3, 2655.2] | 1.003 [0.950, 1.062] | same |
| static_linear_eigen | analysis | before | 5 | 1 | 40.52 ms [39.09 ms, 41.15 ms] | 40.52 ms [39.09 ms, 41.15 ms] | 2461.4 [2374.2, 2499.4] | – | same |
| static_linear_eigen | factorize | after | 5 | 2 | 4.176 s [4.107 s, 4.315 s] | 2.088 s [2.054 s, 2.158 s] | 126816.7 [124734.0, 131056.3] | 0.993 [0.961, 0.997] | same |
| static_linear_eigen | factorize | before | 5 | 2 | 4.201 s [4.156 s, 4.489 s] | 2.101 s [2.078 s, 2.245 s] | 127585.0 [126205.0, 136334.8] | – | same |
| static_linear_eigen | back_substitution | after | 5 | 20 | 142.98 ms [140.73 ms, 158.42 ms] | 7.15 ms [7.04 ms, 7.92 ms] | 434.2 [427.4, 481.1] | 1.058 [0.924, 1.186] | same |
| static_linear_eigen | back_substitution | before | 5 | 20 | 133.61 ms [133.01 ms, 163.60 ms] | 6.68 ms [6.65 ms, 8.18 ms] | 405.8 [403.9, 496.8] | – | same |
| static_linear_faer | mesh | after | 5 | 1 | 602.6 µs [590.4 µs, 669.3 µs] | 602.6 µs [590.4 µs, 669.3 µs] | 36.6 [35.9, 40.7] | 0.949 [0.885, 0.982] | same |
| static_linear_faer | mesh | before | 5 | 1 | 667.8 µs [629.0 µs, 699.3 µs] | 667.8 µs [629.0 µs, 699.3 µs] | 40.6 [38.2, 42.5] | – | same |
| static_linear_faer | init | after | 5 | 1 | 10.54 ms [10.35 ms, 11.33 ms] | 10.54 ms [10.35 ms, 11.33 ms] | 640.3 [628.5, 688.2] | 0.858 [0.850, 1.042] | same |
| static_linear_faer | init | before | 5 | 1 | 12.29 ms [10.88 ms, 12.64 ms] | 12.29 ms [10.88 ms, 12.64 ms] | 746.4 [660.7, 767.8] | – | same |
| static_linear_faer | solve_step | after | 5 | 1 | 4.167 s [4.126 s, 4.234 s] | 4.167 s [4.126 s, 4.234 s] | 253072.1 [250625.3, 257194.1] | 0.979 [0.960, 0.996] | same |
| static_linear_faer | solve_step | before | 5 | 1 | 4.270 s [4.221 s, 4.302 s] | 4.270 s [4.221 s, 4.302 s] | 259357.0 [256388.6, 261298.4] | – | same |
| static_linear_faer | predictor | after | 5 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.814 [0.529, 0.928] | same |
| static_linear_faer | predictor | before | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_faer | matrix | after | 5 | 1 | 71.17 ms [70.02 ms, 72.60 ms] | 71.17 ms [70.02 ms, 72.60 ms] | 4323.0 [4252.7, 4409.8] | 0.548 [0.532, 0.564] | same |
| static_linear_faer | matrix | before | 5 | 1 | 129.06 ms [124.22 ms, 133.71 ms] | 129.06 ms [124.22 ms, 133.71 ms] | 7839.0 [7544.9, 8121.3] | – | same |
| static_linear_faer | corrector | after | 5 | 1 | 0.7 µs [0.6 µs, 0.8 µs] | 0.7 µs [0.6 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 0.701 [0.502, 0.988] | same |
| static_linear_faer | corrector | before | 5 | 1 | 0.9 µs [0.7 µs, 1.1 µs] | 0.9 µs [0.7 µs, 1.1 µs] | 0.1 [0.0, 0.1] | – | same |
| static_linear_faer | step_hooks | after | 5 | 2 | 254.1 µs [248.1 µs, 257.3 µs] | 127.1 µs [124.0 µs, 128.6 µs] | 7.7 [7.5, 7.8] | 1.002 [0.928, 1.025] | same |
| static_linear_faer | step_hooks | before | 5 | 2 | 253.9 µs [248.0 µs, 267.5 µs] | 127.0 µs [124.0 µs, 133.7 µs] | 7.7 [7.5, 8.1] | – | same |
| static_linear_faer | step_rest | after | 5 | 1 | 4.095 s [4.056 s, 4.163 s] | 4.095 s [4.056 s, 4.163 s] | 248700.1 [246357.5, 252855.5] | 0.993 [0.973, 1.011] | same |
| static_linear_faer | step_rest | before | 5 | 1 | 4.146 s [4.093 s, 4.173 s] | 4.146 s [4.093 s, 4.173 s] | 251795.7 [248577.0, 253443.7] | – | same |
| static_linear_faer | stiffness_assembly | after | 5 | 3 | 101.23 ms [96.65 ms, 103.72 ms] | 33.74 ms [32.22 ms, 34.57 ms] | 2049.5 [1956.7, 2100.0] | 0.570 [0.549, 0.608] | same |
| static_linear_faer | stiffness_assembly | before | 5 | 3 | 175.92 ms [170.70 ms, 177.79 ms] | 58.64 ms [56.90 ms, 59.26 ms] | 3561.8 [3456.0, 3599.6] | – | same |
| static_linear_faer | analysis | after | 5 | 1 | 48.19 ms [46.78 ms, 49.66 ms] | 48.19 ms [46.78 ms, 49.66 ms] | 2927.2 [2841.2, 3016.4] | 0.993 [0.925, 1.053] | same |
| static_linear_faer | analysis | before | 5 | 1 | 47.46 ms [47.12 ms, 52.13 ms] | 47.46 ms [47.12 ms, 52.13 ms] | 2882.4 [2862.3, 3166.2] | – | same |
| static_linear_faer | factorize | after | 5 | 2 | 8.055 s [8.002 s, 8.337 s] | 4.027 s [4.001 s, 4.169 s] | 244620.9 [243006.7, 253192.9] | 1.000 [0.988, 1.021] | same |
| static_linear_faer | factorize | before | 5 | 2 | 8.089 s [8.054 s, 8.278 s] | 4.044 s [4.027 s, 4.139 s] | 245654.8 [244597.8, 251399.3] | – | same |
| static_linear_faer | back_substitution | after | 5 | 20 | 148.99 ms [147.45 ms, 165.04 ms] | 7.45 ms [7.37 ms, 8.25 ms] | 452.5 [447.8, 501.2] | 0.982 [0.951, 1.017] | same |
| static_linear_faer | back_substitution | before | 5 | 20 | 156.45 ms [151.67 ms, 162.24 ms] | 7.82 ms [7.58 ms, 8.11 ms] | 475.1 [460.6, 492.7] | – | same |
| static_newton_plastic | mesh | after | 5 | 1 | 248.8 µs [236.5 µs, 259.8 µs] | 248.8 µs [236.5 µs, 259.8 µs] | 41.5 [39.4, 43.3] | 1.053 [0.964, 1.109] | same |
| static_newton_plastic | mesh | before | 5 | 1 | 232.9 µs [228.0 µs, 269.5 µs] | 232.9 µs [228.0 µs, 269.5 µs] | 38.8 [38.0, 44.9] | – | same |
| static_newton_plastic | init | after | 5 | 1 | 4.40 ms [4.22 ms, 4.71 ms] | 4.40 ms [4.22 ms, 4.71 ms] | 732.8 [704.0, 784.9] | 0.931 [0.848, 1.061] | same |
| static_newton_plastic | init | before | 5 | 1 | 4.59 ms [4.44 ms, 5.18 ms] | 4.59 ms [4.44 ms, 5.18 ms] | 765.7 [739.8, 863.9] | – | same |
| static_newton_plastic | solve_step | after | 5 | 12 | 2.607 s [2.593 s, 2.683 s] | 217.24 ms [216.08 ms, 223.57 ms] | 36206.5 [36013.9, 37261.8] | 1.024 [1.023, 1.044] | same |
| static_newton_plastic | solve_step | before | 5 | 12 | 2.545 s [2.534 s, 2.607 s] | 212.10 ms [211.16 ms, 217.29 ms] | 35350.3 [35193.2, 36214.3] | – | same |
| static_newton_plastic | predictor | after | 5 | 12 | 1.8 µs [1.7 µs, 1.9 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.907 [0.888, 1.131] | same |
| static_newton_plastic | predictor | before | 5 | 12 | 1.9 µs [1.6 µs, 2.0 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | residual | after | 5 | 961 | 1.010 s [1.005 s, 1.039 s] | 1.05 ms [1.05 ms, 1.08 ms] | 175.1 [174.3, 180.2] | 1.005 [0.999, 1.007] | same |
| static_newton_plastic | residual | before | 5 | 961 | 1.011 s [998.10 ms, 1.034 s] | 1.05 ms [1.04 ms, 1.08 ms] | 175.3 [173.1, 179.4] | – | same |
| static_newton_plastic | matrix | after | 5 | 949 | 577.7 µs [542.2 µs, 601.3 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 1.114 [1.013, 1.154] | same |
| static_newton_plastic | matrix | before | 5 | 949 | 520.6 µs [517.3 µs, 535.2 µs] | 0.5 µs [0.5 µs, 0.6 µs] | 0.1 [0.1, 0.1] | – | same |
| static_newton_plastic | corrector | after | 5 | 949 | 177.2 µs [173.9 µs, 197.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.905 [0.857, 1.042] | same |
| static_newton_plastic | corrector | before | 5 | 949 | 194.1 µs [188.6 µs, 206.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | step_hooks | after | 5 | 24 | 3.02 ms [2.99 ms, 3.11 ms] | 125.8 µs [124.8 µs, 129.7 µs] | 21.0 [20.8, 21.6] | 1.088 [1.033, 1.106] | same |
| static_newton_plastic | step_hooks | before | 5 | 24 | 2.82 ms [2.72 ms, 2.90 ms] | 117.4 µs [113.2 µs, 120.7 µs] | 19.6 [18.9, 20.1] | – | same |
| static_newton_plastic | step_rest | after | 5 | 12 | 1.594 s [1.584 s, 1.647 s] | 132.87 ms [132.04 ms, 137.22 ms] | 22145.1 [22006.5, 22869.3] | 1.041 [1.034, 1.069] | same |
| static_newton_plastic | step_rest | before | 5 | 12 | 1.533 s [1.531 s, 1.570 s] | 127.77 ms [127.57 ms, 130.81 ms] | 21294.5 [21261.0, 21801.7] | – | same |
| contact_implicit_static_quad4 | mesh | after | 5 | 1 | 10.33 ms [9.47 ms, 11.62 ms] | 10.33 ms [9.47 ms, 11.62 ms] | 1476.0 [1352.9, 1659.7] | 0.955 [0.864, 1.041] | same |
| contact_implicit_static_quad4 | mesh | before | 5 | 1 | 10.96 ms [10.82 ms, 11.16 ms] | 10.96 ms [10.82 ms, 11.16 ms] | 1565.0 [1545.1, 1594.0] | – | same |
| contact_implicit_static_quad4 | init | after | 5 | 1 | 10.78 ms [10.46 ms, 11.22 ms] | 10.78 ms [10.46 ms, 11.22 ms] | 1539.7 [1494.6, 1602.6] | 0.963 [0.884, 1.072] | same |
| contact_implicit_static_quad4 | init | before | 5 | 1 | 11.02 ms [10.46 ms, 12.19 ms] | 11.02 ms [10.46 ms, 12.19 ms] | 1574.3 [1494.4, 1741.3] | – | same |
| contact_implicit_static_quad4 | solve_step | after | 5 | 5 | 1.416 s [1.366 s, 1.477 s] | 283.12 ms [273.20 ms, 295.41 ms] | 40445.1 [39028.0, 42200.7] | 0.878 [0.838, 0.884] | same |
| contact_implicit_static_quad4 | solve_step | before | 5 | 5 | 1.636 s [1.545 s, 1.689 s] | 327.23 ms [309.07 ms, 337.85 ms] | 46747.6 [44153.4, 48264.4] | – | same |
| embedded_rebar_tet4 | mesh | after | 5 | 1 | 25.87 ms [24.19 ms, 27.11 ms] | 25.87 ms [24.19 ms, 27.11 ms] | 1189.4 [1112.1, 1246.6] | 0.955 [0.871, 1.078] | same |
| embedded_rebar_tet4 | mesh | before | 5 | 1 | 27.05 ms [24.07 ms, 27.79 ms] | 27.05 ms [24.07 ms, 27.79 ms] | 1243.8 [1106.5, 1277.5] | – | same |
| embedded_rebar_tet4 | init | after | 5 | 1 | 1.281 s [1.255 s, 1.325 s] | 1.281 s [1.255 s, 1.325 s] | 58918.8 [57704.2, 60927.0] | 1.002 [0.929, 1.053] | same |
| embedded_rebar_tet4 | init | before | 5 | 1 | 1.274 s [1.259 s, 1.379 s] | 1.274 s [1.259 s, 1.379 s] | 58567.1 [57881.0, 63388.4] | – | same |
| embedded_rebar_tet4 | intersect | after | 5 | 5 | 6.484 s [6.269 s, 6.697 s] | 1.297 s [1.254 s, 1.339 s] | 59626.4 [57645.2, 61586.0] | 0.996 [0.978, 1.047] | same |
| embedded_rebar_tet4 | intersect | before | 5 | 5 | 6.470 s [6.398 s, 6.512 s] | 1.294 s [1.280 s, 1.302 s] | 59496.0 [58834.2, 59882.3] | – | same |
| embedded_rebar_tet4 | solve_step | after | 5 | 1 | 3.312 s [3.222 s, 3.358 s] | 3.312 s [3.222 s, 3.358 s] | 152253.2 [148159.9, 154382.4] | 0.963 [0.952, 0.974] | same |
| embedded_rebar_tet4 | solve_step | before | 5 | 1 | 3.447 s [3.385 s, 3.477 s] | 3.447 s [3.385 s, 3.477 s] | 158484.1 [155618.4, 159873.4] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | after | 5 | 20 | 33.62 ms [32.95 ms, 37.03 ms] | 1.68 ms [1.65 ms, 1.85 ms] | 77.3 [75.7, 85.1] | 0.977 [0.932, 1.060] | same |
| embedded_rebar_tet4 | assemble_internal_forces | before | 5 | 20 | 35.26 ms [32.77 ms, 37.68 ms] | 1.76 ms [1.64 ms, 1.88 ms] | 81.1 [75.3, 86.6] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | after | 5 | 5 | 248.25 ms [238.93 ms, 249.05 ms] | 49.65 ms [47.79 ms, 49.81 ms] | 2282.7 [2197.1, 2290.1] | 0.565 [0.532, 0.584] | same |
| embedded_rebar_tet4 | assemble_stiffness | before | 5 | 5 | 433.09 ms [423.20 ms, 455.72 ms] | 86.62 ms [84.64 ms, 91.14 ms] | 3982.5 [3891.5, 4190.5] | – | same |
| embedded_rebar_tri3 | mesh | after | 5 | 1 | 25.83 ms [24.12 ms, 28.57 ms] | 25.83 ms [24.12 ms, 28.57 ms] | 1025.0 [957.0, 1133.9] | 1.065 [0.884, 1.260] | same |
| embedded_rebar_tri3 | mesh | before | 5 | 1 | 24.26 ms [22.68 ms, 28.28 ms] | 24.26 ms [22.68 ms, 28.28 ms] | 962.8 [900.2, 1122.3] | – | same |
| embedded_rebar_tri3 | init | after | 5 | 1 | 1.395 s [1.345 s, 1.467 s] | 1.395 s [1.345 s, 1.467 s] | 55357.1 [53385.2, 58195.2] | 0.996 [0.959, 1.020] | same |
| embedded_rebar_tri3 | init | before | 5 | 1 | 1.421 s [1.367 s, 1.443 s] | 1.421 s [1.367 s, 1.443 s] | 56398.3 [54259.7, 57243.6] | – | same |
| embedded_rebar_tri3 | intersect | after | 5 | 5 | 6.968 s [6.776 s, 7.203 s] | 1.394 s [1.355 s, 1.441 s] | 55298.9 [53778.7, 57168.5] | 1.001 [0.907, 1.044] | same |
| embedded_rebar_tri3 | intersect | before | 5 | 5 | 7.059 s [6.674 s, 7.537 s] | 1.412 s [1.335 s, 1.507 s] | 56024.9 [52969.5, 59814.7] | – | same |
| embedded_rebar_tri3 | solve_step | after | 5 | 1 | 422.29 ms [410.75 ms, 435.56 ms] | 422.29 ms [410.75 ms, 435.56 ms] | 16757.4 [16299.6, 17284.2] | 0.900 [0.890, 0.935] | same |
| embedded_rebar_tri3 | solve_step | before | 5 | 1 | 467.84 ms [451.72 ms, 478.25 ms] | 467.84 ms [451.72 ms, 478.25 ms] | 18565.2 [17925.5, 18978.4] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | after | 5 | 20 | 44.59 ms [44.06 ms, 45.22 ms] | 2.23 ms [2.20 ms, 2.26 ms] | 88.5 [87.4, 89.7] | 1.009 [0.967, 1.029] | same |
| embedded_rebar_tri3 | assemble_internal_forces | before | 5 | 20 | 44.18 ms [43.32 ms, 46.76 ms] | 2.21 ms [2.17 ms, 2.34 ms] | 87.7 [86.0, 92.8] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | after | 5 | 5 | 80.31 ms [74.64 ms, 81.66 ms] | 16.06 ms [14.93 ms, 16.33 ms] | 637.4 [592.4, 648.1] | 0.619 [0.583, 0.638] | same |
| embedded_rebar_tri3 | assemble_stiffness | before | 5 | 5 | 131.32 ms [125.82 ms, 134.46 ms] | 26.26 ms [25.16 ms, 26.89 ms] | 1042.2 [998.6, 1067.2] | – | same |
| heat_implicit_hex8_kvar | mesh | after | 5 | 1 | 4.61 ms [4.00 ms, 4.81 ms] | 4.61 ms [4.00 ms, 4.81 ms] | 2668.4 [2316.0, 2784.7] | 0.979 [0.864, 1.221] | same |
| heat_implicit_hex8_kvar | mesh | before | 5 | 1 | 4.45 ms [3.90 ms, 4.74 ms] | 4.45 ms [3.90 ms, 4.74 ms] | 2572.9 [2256.9, 2740.7] | – | same |
| heat_implicit_hex8_kvar | init | after | 5 | 1 | 745.0 µs [720.0 µs, 843.0 µs] | 745.0 µs [720.0 µs, 843.0 µs] | 431.1 [416.7, 487.8] | 0.782 [0.730, 1.206] | same |
| heat_implicit_hex8_kvar | init | before | 5 | 1 | 921.0 µs [699.0 µs, 1.02 ms] | 921.0 µs [699.0 µs, 1.02 ms] | 533.0 [404.5, 590.3] | – | same |
| heat_implicit_hex8_kvar | solve_step | after | 5 | 5 | 1.022 s [994.89 ms, 1.028 s] | 204.31 ms [198.98 ms, 205.59 ms] | 118233.8 [115149.2, 118975.9] | 0.909 [0.889, 0.927] | same |
| heat_implicit_hex8_kvar | solve_step | before | 5 | 5 | 1.111 s [1.089 s, 1.157 s] | 222.26 ms [217.90 ms, 231.34 ms] | 128619.9 [126096.8, 133877.0] | – | same |
| heat_static_tri6 | mesh | after | 5 | 1 | 49.25 ms [45.31 ms, 50.52 ms] | 49.25 ms [45.31 ms, 50.52 ms] | 2035.0 [1872.4, 2087.7] | 1.046 [0.943, 1.078] | same |
| heat_static_tri6 | mesh | before | 5 | 1 | 47.82 ms [45.68 ms, 48.98 ms] | 47.82 ms [45.68 ms, 48.98 ms] | 1975.9 [1887.6, 2024.1] | – | same |
| heat_static_tri6 | init | after | 5 | 1 | 4.56 ms [4.44 ms, 4.81 ms] | 4.56 ms [4.44 ms, 4.81 ms] | 188.6 [183.3, 199.0] | 0.744 [0.694, 0.784] | same |
| heat_static_tri6 | init | before | 5 | 1 | 6.32 ms [5.75 ms, 6.83 ms] | 6.32 ms [5.75 ms, 6.83 ms] | 261.1 [237.4, 282.1] | – | same |
| heat_static_tri6 | assemble_conductivity | after | 5 | 5 | 73.90 ms [72.46 ms, 77.35 ms] | 14.78 ms [14.49 ms, 15.47 ms] | 610.7 [598.9, 639.2] | 0.496 [0.484, 0.526] | same |
| heat_static_tri6 | assemble_conductivity | before | 5 | 5 | 147.38 ms [146.04 ms, 154.79 ms] | 29.48 ms [29.21 ms, 30.96 ms] | 1218.0 [1206.9, 1279.2] | – | same |
| heat_static_tri6 | solve_step | after | 5 | 1 | 4.001 s [3.878 s, 4.049 s] | 4.001 s [3.878 s, 4.049 s] | 165331.6 [160237.5, 167313.3] | 0.950 [0.946, 0.962] | same |
| heat_static_tri6 | solve_step | before | 5 | 1 | 4.223 s [4.092 s, 4.261 s] | 4.223 s [4.092 s, 4.261 s] | 174488.6 [169087.3, 176082.5] | – | same |
| phasefield_staggered_tet4 | mesh | after | 5 | 1 | 8.78 ms [7.77 ms, 8.85 ms] | 8.78 ms [7.77 ms, 8.85 ms] | 1462.7 [1294.7, 1474.7] | 0.996 [0.890, 1.202] | same |
| phasefield_staggered_tet4 | mesh | before | 5 | 1 | 8.73 ms [7.21 ms, 8.87 ms] | 8.73 ms [7.21 ms, 8.87 ms] | 1455.2 [1201.5, 1477.8] | – | same |
| phasefield_staggered_tet4 | init | after | 5 | 1 | 2.17 ms [1.84 ms, 2.71 ms] | 2.17 ms [1.84 ms, 2.71 ms] | 361.0 [306.7, 451.7] | 0.924 [0.810, 1.061] | same |
| phasefield_staggered_tet4 | init | before | 5 | 1 | 2.34 ms [2.17 ms, 2.61 ms] | 2.34 ms [2.17 ms, 2.61 ms] | 390.7 [362.0, 435.5] | – | same |
| phasefield_staggered_tet4 | solid_solve | after | 5 | 4 | 1.749 s [1.718 s, 1.773 s] | 437.19 ms [429.58 ms, 443.36 ms] | 72864.5 [71597.4, 73893.2] | 0.945 [0.936, 0.969] | same |
| phasefield_staggered_tet4 | solid_solve | before | 5 | 4 | 1.862 s [1.801 s, 1.896 s] | 465.62 ms [450.34 ms, 473.91 ms] | 77602.7 [75056.8, 78984.7] | – | same |
| phasefield_staggered_tet4 | strain_transfer | after | 5 | 4 | 809.0 µs [737.0 µs, 830.0 µs] | 202.2 µs [184.2 µs, 207.5 µs] | 33.7 [30.7, 34.6] | 0.904 [0.851, 1.029] | same |
| phasefield_staggered_tet4 | strain_transfer | before | 5 | 4 | 889.0 µs [806.0 µs, 951.0 µs] | 222.2 µs [201.5 µs, 237.8 µs] | 37.0 [33.6, 39.6] | – | same |
| phasefield_staggered_tet4 | phase_solve | after | 5 | 4 | 105.37 ms [99.55 ms, 106.94 ms] | 26.34 ms [24.89 ms, 26.73 ms] | 4390.5 [4148.0, 4455.7] | 0.853 [0.794, 0.888] | same |
| phasefield_staggered_tet4 | phase_solve | before | 5 | 4 | 121.47 ms [120.00 ms, 130.36 ms] | 30.37 ms [30.00 ms, 32.59 ms] | 5061.2 [5000.0, 5431.5] | – | same |
| phasefield_staggered_tet4 | damage_transfer | after | 5 | 4 | 1.75 ms [1.66 ms, 1.82 ms] | 437.5 µs [415.0 µs, 455.0 µs] | 72.9 [69.2, 75.8] | 1.009 [0.973, 1.030] | same |
| phasefield_staggered_tet4 | damage_transfer | before | 5 | 4 | 1.70 ms [1.65 ms, 1.87 ms] | 424.8 µs [411.2 µs, 467.8 µs] | 70.8 [68.5, 78.0] | – | same |
| phasefield_staggered_tet4 | internal_forces | after | 5 | 4 | 1.94 ms [1.80 ms, 2.00 ms] | 483.8 µs [449.2 µs, 500.0 µs] | 80.6 [74.9, 83.3] | 0.926 [0.907, 1.092] | same |
| phasefield_staggered_tet4 | internal_forces | before | 5 | 4 | 2.03 ms [1.80 ms, 2.20 ms] | 508.2 µs [450.2 µs, 550.0 µs] | 84.7 [75.0, 91.7] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | after | 5 | 5 | 45.57 ms [44.28 ms, 46.73 ms] | 9.12 ms [8.86 ms, 9.35 ms] | 1519.2 [1476.0, 1557.5] | 0.480 [0.457, 0.502] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | before | 5 | 5 | 94.96 ms [88.27 ms, 102.21 ms] | 18.99 ms [17.65 ms, 20.44 ms] | 3165.4 [2942.4, 3407.1] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | after | 5 | 5 | 10.49 ms [10.23 ms, 11.69 ms] | 2.10 ms [2.05 ms, 2.34 ms] | 349.7 [341.0, 389.8] | 0.524 [0.468, 0.587] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | before | 5 | 5 | 19.91 ms [19.52 ms, 21.88 ms] | 3.98 ms [3.90 ms, 4.38 ms] | 663.8 [650.7, 729.3] | – | same |
| phasefield_staggered_tri3 | mesh | after | 5 | 1 | 51.85 ms [49.83 ms, 53.54 ms] | 51.85 ms [49.83 ms, 53.54 ms] | 1012.8 [973.3, 1045.6] | 1.028 [0.996, 1.080] | same |
| phasefield_staggered_tri3 | mesh | before | 5 | 1 | 50.00 ms [48.01 ms, 52.05 ms] | 50.00 ms [48.01 ms, 52.05 ms] | 976.5 [937.7, 1016.7] | – | same |
| phasefield_staggered_tri3 | init | after | 5 | 1 | 15.09 ms [14.34 ms, 15.40 ms] | 15.09 ms [14.34 ms, 15.40 ms] | 294.7 [280.0, 300.8] | 0.889 [0.834, 0.909] | same |
| phasefield_staggered_tri3 | init | before | 5 | 1 | 16.94 ms [16.14 ms, 18.27 ms] | 16.94 ms [16.14 ms, 18.27 ms] | 330.9 [315.1, 356.8] | – | same |
| phasefield_staggered_tri3 | solid_solve | after | 5 | 4 | 8.984 s [8.736 s, 9.199 s] | 2.246 s [2.184 s, 2.300 s] | 43864.9 [42655.4, 44915.6] | 0.943 [0.909, 0.972] | same |
| phasefield_staggered_tri3 | solid_solve | before | 5 | 4 | 9.468 s [9.363 s, 9.605 s] | 2.367 s [2.341 s, 2.401 s] | 46230.1 [45716.5, 46901.8] | – | same |
| phasefield_staggered_tri3 | strain_transfer | after | 5 | 4 | 4.10 ms [3.91 ms, 4.22 ms] | 1.02 ms [976.5 µs, 1.06 ms] | 20.0 [19.1, 20.6] | 0.990 [0.866, 1.002] | same |
| phasefield_staggered_tri3 | strain_transfer | before | 5 | 4 | 4.21 ms [4.09 ms, 4.51 ms] | 1.05 ms [1.02 ms, 1.13 ms] | 20.6 [20.0, 22.0] | – | same |
| phasefield_staggered_tri3 | phase_solve | after | 5 | 4 | 1.628 s [1.583 s, 1.672 s] | 406.90 ms [395.82 ms, 418.11 ms] | 7947.3 [7730.9, 8166.2] | 0.898 [0.863, 0.920] | same |
| phasefield_staggered_tri3 | phase_solve | before | 5 | 4 | 1.817 s [1.781 s, 1.835 s] | 454.28 ms [445.35 ms, 458.79 ms] | 8872.6 [8698.2, 8960.7] | – | same |
| phasefield_staggered_tri3 | damage_transfer | after | 5 | 4 | 13.93 ms [13.14 ms, 15.65 ms] | 3.48 ms [3.29 ms, 3.91 ms] | 68.0 [64.2, 76.4] | 1.015 [0.899, 1.143] | same |
| phasefield_staggered_tri3 | damage_transfer | before | 5 | 4 | 13.81 ms [13.69 ms, 15.24 ms] | 3.45 ms [3.42 ms, 3.81 ms] | 67.4 [66.8, 74.4] | – | same |
| phasefield_staggered_tri3 | internal_forces | after | 5 | 4 | 13.84 ms [13.16 ms, 14.63 ms] | 3.46 ms [3.29 ms, 3.66 ms] | 67.6 [64.3, 71.4] | 0.995 [0.962, 1.048] | same |
| phasefield_staggered_tri3 | internal_forces | before | 5 | 4 | 13.79 ms [13.38 ms, 14.39 ms] | 3.45 ms [3.35 ms, 3.60 ms] | 67.3 [65.4, 70.3] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | after | 5 | 5 | 91.51 ms [88.39 ms, 93.42 ms] | 18.30 ms [17.68 ms, 18.68 ms] | 357.5 [345.3, 364.9] | 0.417 [0.401, 0.430] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | before | 5 | 5 | 216.37 ms [214.60 ms, 225.85 ms] | 43.27 ms [42.92 ms, 45.17 ms] | 845.2 [838.3, 882.2] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | after | 5 | 5 | 73.28 ms [71.41 ms, 78.15 ms] | 14.66 ms [14.28 ms, 15.63 ms] | 286.3 [279.0, 305.3] | 0.547 [0.524, 0.595] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | before | 5 | 5 | 134.09 ms [131.39 ms, 136.62 ms] | 26.82 ms [26.28 ms, 27.32 ms] | 523.8 [513.2, 533.7] | – | same |
| structural_beam3_frame_dynamic | mesh | after | 5 | 1 | 285.0 µs [248.0 µs, 302.0 µs] | 285.0 µs [248.0 µs, 302.0 µs] | 46.8 [40.8, 49.6] | 1.010 [0.886, 1.148] | same |
| structural_beam3_frame_dynamic | mesh | before | 5 | 1 | 263.0 µs [255.0 µs, 286.0 µs] | 263.0 µs [255.0 µs, 286.0 µs] | 43.2 [41.9, 47.0] | – | same |
| structural_beam3_frame_dynamic | init | after | 5 | 1 | 25.64 ms [23.96 ms, 27.43 ms] | 25.64 ms [23.96 ms, 27.43 ms] | 4214.3 [3939.0, 4508.7] | 0.975 [0.867, 1.056] | same |
| structural_beam3_frame_dynamic | init | before | 5 | 1 | 26.78 ms [25.28 ms, 28.21 ms] | 26.78 ms [25.28 ms, 28.21 ms] | 4401.5 [4155.3, 4637.4] | – | same |
| structural_beam3_frame_dynamic | solve_step | after | 5 | 60 | 4.289 s [4.115 s, 4.303 s] | 71.48 ms [68.59 ms, 71.72 ms] | 11748.5 [11273.5, 11788.4] | 0.825 [0.821, 0.833] | same |
| structural_beam3_frame_dynamic | solve_step | before | 5 | 60 | 5.163 s [4.992 s, 5.213 s] | 86.04 ms [83.20 ms, 86.89 ms] | 14142.6 [13675.4, 14282.0] | – | same |
| structural_beam3_frame_static | mesh | after | 5 | 1 | 255.0 µs [247.0 µs, 268.0 µs] | 255.0 µs [247.0 µs, 268.0 µs] | 41.9 [40.6, 44.0] | 0.931 [0.924, 1.085] | same |
| structural_beam3_frame_static | mesh | before | 5 | 1 | 272.0 µs [242.0 µs, 290.0 µs] | 272.0 µs [242.0 µs, 290.0 µs] | 44.7 [39.8, 47.7] | – | same |
| structural_beam3_frame_static | init | after | 5 | 1 | 24.69 ms [23.58 ms, 26.42 ms] | 24.69 ms [23.58 ms, 26.42 ms] | 4057.5 [3875.6, 4342.2] | 0.993 [0.862, 1.058] | same |
| structural_beam3_frame_static | init | before | 5 | 1 | 25.99 ms [23.33 ms, 27.35 ms] | 25.99 ms [23.33 ms, 27.35 ms] | 4272.5 [3834.6, 4494.9] | – | same |
| structural_beam3_frame_static | assemble_stiffness | after | 5 | 5 | 70.88 ms [67.04 ms, 71.86 ms] | 14.18 ms [13.41 ms, 14.37 ms] | 2330.1 [2203.8, 2362.3] | 0.516 [0.483, 0.527] | same |
| structural_beam3_frame_static | assemble_stiffness | before | 5 | 5 | 137.07 ms [135.57 ms, 138.92 ms] | 27.41 ms [27.11 ms, 27.78 ms] | 4506.0 [4456.5, 4566.9] | – | same |
| structural_beam3_frame_static | solve_step | after | 5 | 1 | 4.624 s [4.536 s, 4.672 s] | 4.624 s [4.536 s, 4.672 s] | 759979.0 [745501.5, 767978.6] | 0.989 [0.981, 1.014] | same |
| structural_beam3_frame_static | solve_step | before | 5 | 1 | 4.666 s [4.522 s, 4.755 s] | 4.666 s [4.522 s, 4.755 s] | 766870.3 [743230.8, 781555.1] | – | same |
| structural_dkt18_plate_static | mesh | after | 5 | 1 | 247.0 µs [234.0 µs, 251.0 µs] | 247.0 µs [234.0 µs, 251.0 µs] | 34.3 [32.5, 34.9] | 1.074 [0.944, 1.173] | same |
| structural_dkt18_plate_static | mesh | before | 5 | 1 | 230.0 µs [214.0 µs, 248.0 µs] | 230.0 µs [214.0 µs, 248.0 µs] | 31.9 [29.7, 34.4] | – | same |
| structural_dkt18_plate_static | init | after | 5 | 1 | 66.19 ms [62.89 ms, 66.74 ms] | 66.19 ms [62.89 ms, 66.74 ms] | 9193.6 [8734.6, 9269.2] | 1.008 [0.936, 1.037] | same |
| structural_dkt18_plate_static | init | before | 5 | 1 | 64.95 ms [64.05 ms, 67.21 ms] | 64.95 ms [64.05 ms, 67.21 ms] | 9020.4 [8896.0, 9335.1] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | after | 5 | 5 | 372.35 ms [357.19 ms, 383.56 ms] | 74.47 ms [71.44 ms, 76.71 ms] | 10343.0 [9922.0, 10654.3] | 0.623 [0.618, 0.642] | same |
| structural_dkt18_plate_static | assemble_stiffness | before | 5 | 5 | 597.07 ms [556.07 ms, 609.25 ms] | 119.41 ms [111.21 ms, 121.85 ms] | 16585.3 [15446.5, 16923.5] | – | same |
| structural_dkt18_plate_static | solve_step | after | 5 | 1 | 2.342 s [2.264 s, 2.396 s] | 2.342 s [2.264 s, 2.396 s] | 325254.7 [314499.3, 332756.4] | 0.967 [0.950, 0.972] | same |
| structural_dkt18_plate_static | solve_step | before | 5 | 1 | 2.410 s [2.377 s, 2.467 s] | 2.410 s [2.377 s, 2.467 s] | 334674.6 [330091.4, 342669.2] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 59e05aa3f, fc4a375ff | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 59e05aa3f, fc4a375ff | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 59e05aa3f, fc4a375ff | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 59e05aa3f, fc4a375ff | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 59e05aa3f, fc4a375ff | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 59e05aa3f, fc4a375ff | 35e60e6f09e111e7 |
| static_linear_faer | 16464 | 3375 | 10125 | 1 | 59e05aa3f, fc4a375ff | c9951bde736748ef |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 59e05aa3f, fc4a375ff | 7bd5f114957d7d7b |
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | fc4a375ff | 776a3c6490006b8e |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | fc4a375ff | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | fc4a375ff | 8f333faf172f0935 |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | fc4a375ff | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | fc4a375ff | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | fc4a375ff | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | fc4a375ff | a77809465b3e7c18 |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | fc4a375ff | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | fc4a375ff | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | fc4a375ff | 1cfc7f010c52d6b3 |
