Suite `solid`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 405.1 µs [392.5 µs, 415.8 µs] | 405.1 µs [392.5 µs, 415.8 µs] | 112.5 [109.0, 115.5] | – | same |
| cohesive_intrinsic_implicit | mesh | t1 | 5 | 1 | 415.2 µs [403.9 µs, 424.6 µs] | 415.2 µs [403.9 µs, 424.6 µs] | 115.3 [112.2, 117.9] | 1.043 [0.995, 1.058] | same |
| cohesive_intrinsic_implicit | mesh | t2 | 5 | 1 | 390.8 µs [370.1 µs, 408.7 µs] | 390.8 µs [370.1 µs, 408.7 µs] | 108.6 [102.8, 113.5] | 0.963 [0.913, 1.014] | same |
| cohesive_intrinsic_implicit | mesh | t4 | 5 | 1 | 403.3 µs [397.1 µs, 415.7 µs] | 403.3 µs [397.1 µs, 415.7 µs] | 112.0 [110.3, 115.5] | 1.000 [0.980, 1.028] | same |
| cohesive_intrinsic_implicit | mesh | t8 | 5 | 1 | 396.7 µs [388.1 µs, 420.3 µs] | 396.7 µs [388.1 µs, 420.3 µs] | 110.2 [107.8, 116.8] | 0.986 [0.954, 1.065] | same |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 10.08 ms [9.80 ms, 10.17 ms] | 10.08 ms [9.80 ms, 10.17 ms] | 2800.1 [2721.1, 2824.4] | – | same |
| cohesive_intrinsic_implicit | init | t1 | 5 | 1 | 10.05 ms [9.90 ms, 10.48 ms] | 10.05 ms [9.90 ms, 10.48 ms] | 2791.0 [2750.8, 2910.6] | 1.018 [0.974, 1.039] | same |
| cohesive_intrinsic_implicit | init | t2 | 5 | 1 | 10.09 ms [9.92 ms, 10.54 ms] | 10.09 ms [9.92 ms, 10.54 ms] | 2803.7 [2755.9, 2927.2] | 1.013 [1.000, 1.043] | same |
| cohesive_intrinsic_implicit | init | t4 | 5 | 1 | 10.24 ms [10.22 ms, 10.43 ms] | 10.24 ms [10.22 ms, 10.43 ms] | 2843.7 [2837.8, 2896.9] | 1.035 [1.005, 1.053] | same |
| cohesive_intrinsic_implicit | init | t8 | 5 | 1 | 10.33 ms [10.16 ms, 10.51 ms] | 10.33 ms [10.16 ms, 10.51 ms] | 2870.0 [2821.8, 2918.2] | 1.029 [1.014, 1.072] | same |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 3.216 s [3.202 s, 3.242 s] | 107.20 ms [106.73 ms, 108.08 ms] | 29777.5 [29647.6, 30022.3] | – | same |
| cohesive_intrinsic_implicit | solve_step | t1 | 5 | 30 | 3.197 s [3.166 s, 3.222 s] | 106.58 ms [105.53 ms, 107.41 ms] | 29604.3 [29312.6, 29834.7] | 0.992 [0.986, 1.001] | same |
| cohesive_intrinsic_implicit | solve_step | t2 | 5 | 30 | 3.141 s [3.091 s, 3.179 s] | 104.69 ms [103.05 ms, 105.95 ms] | 29079.8 [28624.7, 29431.2] | 0.969 [0.963, 0.990] | same |
| cohesive_intrinsic_implicit | solve_step | t4 | 5 | 30 | 3.063 s [3.032 s, 3.122 s] | 102.09 ms [101.05 ms, 104.07 ms] | 28358.2 [28069.6, 28909.7] | 0.956 [0.944, 0.963] | same |
| cohesive_intrinsic_implicit | solve_step | t8 | 5 | 30 | 3.059 s [3.002 s, 3.238 s] | 101.96 ms [100.07 ms, 107.93 ms] | 28321.6 [27797.9, 29980.1] | 0.944 [0.931, 1.011] | same |
| implicit_dynamic | mesh | base | 5 | 1 | 668.8 µs [652.1 µs, 726.1 µs] | 668.8 µs [652.1 µs, 726.1 µs] | 40.6 [39.6, 44.1] | – | same |
| implicit_dynamic | mesh | t1 | 5 | 1 | 649.6 µs [643.6 µs, 697.4 µs] | 649.6 µs [643.6 µs, 697.4 µs] | 39.5 [39.1, 42.4] | 0.972 [0.892, 1.069] | same |
| implicit_dynamic | mesh | t2 | 5 | 1 | 666.1 µs [656.9 µs, 674.5 µs] | 666.1 µs [656.9 µs, 674.5 µs] | 40.5 [39.9, 41.0] | 0.982 [0.929, 1.024] | same |
| implicit_dynamic | mesh | t4 | 5 | 1 | 692.7 µs [670.7 µs, 713.8 µs] | 692.7 µs [670.7 µs, 713.8 µs] | 42.1 [40.7, 43.4] | 1.021 [0.954, 1.067] | same |
| implicit_dynamic | mesh | t8 | 5 | 1 | 696.9 µs [685.7 µs, 726.0 µs] | 696.9 µs [685.7 µs, 726.0 µs] | 42.3 [41.6, 44.1] | 1.038 [0.960, 1.086] | same |
| implicit_dynamic | init | base | 5 | 1 | 8.65 ms [8.63 ms, 9.04 ms] | 8.65 ms [8.63 ms, 9.04 ms] | 525.7 [524.1, 549.2] | – | same |
| implicit_dynamic | init | t1 | 5 | 1 | 8.37 ms [8.33 ms, 8.66 ms] | 8.37 ms [8.33 ms, 8.66 ms] | 508.7 [505.7, 525.8] | 0.971 [0.921, 1.002] | same |
| implicit_dynamic | init | t2 | 5 | 1 | 8.67 ms [8.51 ms, 8.72 ms] | 8.67 ms [8.51 ms, 8.72 ms] | 526.6 [516.9, 529.5] | 1.001 [0.941, 1.009] | same |
| implicit_dynamic | init | t4 | 5 | 1 | 8.80 ms [8.54 ms, 8.86 ms] | 8.80 ms [8.54 ms, 8.86 ms] | 534.4 [518.7, 538.1] | 1.020 [0.970, 1.022] | same |
| implicit_dynamic | init | t8 | 5 | 1 | 9.05 ms [8.72 ms, 9.35 ms] | 9.05 ms [8.72 ms, 9.35 ms] | 549.9 [529.8, 567.6] | 1.037 [1.001, 1.081] | same |
| implicit_dynamic | mass_consistent | base | 5 | 1 | 66.89 ms [63.85 ms, 67.76 ms] | 66.89 ms [63.85 ms, 67.76 ms] | 4062.9 [3878.0, 4115.9] | – | same |
| implicit_dynamic | mass_consistent | t1 | 5 | 1 | 64.23 ms [63.14 ms, 65.91 ms] | 64.23 ms [63.14 ms, 65.91 ms] | 3901.5 [3835.1, 4003.4] | 0.983 [0.932, 1.030] | same |
| implicit_dynamic | mass_consistent | t2 | 5 | 1 | 59.73 ms [58.78 ms, 60.31 ms] | 59.73 ms [58.78 ms, 60.31 ms] | 3627.9 [3570.4, 3663.2] | 0.899 [0.885, 0.925] | same |
| implicit_dynamic | mass_consistent | t4 | 5 | 1 | 58.49 ms [56.67 ms, 58.74 ms] | 58.49 ms [56.67 ms, 58.74 ms] | 3552.7 [3442.3, 3567.9] | 0.878 [0.845, 0.916] | same |
| implicit_dynamic | mass_consistent | t8 | 5 | 1 | 57.26 ms [55.31 ms, 57.72 ms] | 57.26 ms [55.31 ms, 57.72 ms] | 3478.1 [3359.3, 3505.7] | 0.859 [0.825, 0.890] | same |
| implicit_dynamic | solve_step | base | 5 | 60 | 1.197 s [1.067 s, 1.316 s] | 19.94 ms [17.78 ms, 21.94 ms] | 1211.4 [1080.1, 1332.6] | – | same |
| implicit_dynamic | solve_step | t1 | 5 | 60 | 1.198 s [1.155 s, 1.271 s] | 19.96 ms [19.24 ms, 21.19 ms] | 1212.6 [1168.7, 1286.8] | 0.984 [0.931, 1.082] | same |
| implicit_dynamic | solve_step | t2 | 5 | 60 | 1.033 s [962.79 ms, 1.088 s] | 17.22 ms [16.05 ms, 18.13 ms] | 1045.7 [974.6, 1101.1] | 0.902 [0.754, 0.915] | same |
| implicit_dynamic | solve_step | t4 | 5 | 60 | 934.11 ms [842.66 ms, 960.39 ms] | 15.57 ms [14.04 ms, 16.01 ms] | 945.6 [853.0, 972.2] | 0.746 [0.710, 0.855] | same |
| implicit_dynamic | solve_step | t8 | 5 | 60 | 852.43 ms [832.07 ms, 981.21 ms] | 14.21 ms [13.87 ms, 16.35 ms] | 862.9 [842.3, 993.3] | 0.740 [0.690, 0.799] | same |
| implicit_dynamic | predictor | base | 5 | 60 | 10.2 µs [9.6 µs, 11.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | predictor | t1 | 5 | 60 | 9.9 µs [9.6 µs, 10.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.940 [0.848, 1.025] | same |
| implicit_dynamic | predictor | t2 | 5 | 60 | 9.7 µs [9.0 µs, 9.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.876 [0.838, 1.014] | same |
| implicit_dynamic | predictor | t4 | 5 | 60 | 9.7 µs [8.8 µs, 9.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.888 [0.796, 1.019] | same |
| implicit_dynamic | predictor | t8 | 5 | 60 | 9.8 µs [8.6 µs, 10.8 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.908 [0.808, 1.054] | same |
| implicit_dynamic | residual | base | 5 | 180 | 203.26 ms [201.77 ms, 204.08 ms] | 1.13 ms [1.12 ms, 1.13 ms] | 68.6 [68.1, 68.9] | – | same |
| implicit_dynamic | residual | t1 | 5 | 180 | 206.13 ms [204.60 ms, 207.68 ms] | 1.15 ms [1.14 ms, 1.15 ms] | 69.6 [69.0, 70.1] | 1.014 [1.004, 1.029] | same |
| implicit_dynamic | residual | t2 | 5 | 180 | 112.89 ms [112.04 ms, 113.73 ms] | 627.2 µs [622.4 µs, 631.8 µs] | 38.1 [37.8, 38.4] | 0.557 [0.553, 0.559] | same |
| implicit_dynamic | residual | t4 | 5 | 180 | 74.34 ms [73.09 ms, 75.17 ms] | 413.0 µs [406.1 µs, 417.6 µs] | 25.1 [24.7, 25.4] | 0.368 [0.362, 0.369] | same |
| implicit_dynamic | residual | t8 | 5 | 180 | 59.30 ms [58.70 ms, 62.39 ms] | 329.5 µs [326.1 µs, 346.6 µs] | 20.0 [19.8, 21.1] | 0.293 [0.290, 0.307] | same |
| implicit_dynamic | matrix | base | 5 | 420 | 163.8 µs [154.4 µs, 182.3 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | matrix | t1 | 5 | 420 | 159.6 µs [148.2 µs, 170.1 µs] | 0.4 µs [0.4 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.960 [0.928, 1.054] | same |
| implicit_dynamic | matrix | t2 | 5 | 420 | 154.1 µs [146.4 µs, 167.3 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.953 [0.803, 1.084] | same |
| implicit_dynamic | matrix | t4 | 5 | 420 | 141.9 µs [132.8 µs, 150.8 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.812 [0.797, 0.934] | same |
| implicit_dynamic | matrix | t8 | 5 | 420 | 135.4 µs [127.9 µs, 139.7 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.819 [0.727, 0.904] | same |
| implicit_dynamic | corrector | base | 5 | 120 | 25.6 µs [24.3 µs, 30.1 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | t1 | 5 | 120 | 20.1 µs [17.4 µs, 25.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.732 [0.669, 1.015] | same |
| implicit_dynamic | corrector | t2 | 5 | 120 | 20.7 µs [17.6 µs, 23.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.745 [0.688, 0.939] | same |
| implicit_dynamic | corrector | t4 | 5 | 120 | 20.7 µs [19.4 µs, 22.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.796 [0.682, 0.873] | same |
| implicit_dynamic | corrector | t8 | 5 | 120 | 19.7 µs [18.1 µs, 22.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.750 [0.641, 0.838] | same |
| implicit_dynamic | step_hooks | base | 5 | 120 | 14.78 ms [13.29 ms, 15.06 ms] | 123.2 µs [110.7 µs, 125.5 µs] | 7.5 [6.7, 7.6] | – | same |
| implicit_dynamic | step_hooks | t1 | 5 | 120 | 14.70 ms [14.42 ms, 15.00 ms] | 122.5 µs [120.2 µs, 125.0 µs] | 7.4 [7.3, 7.6] | 1.004 [0.976, 1.106] | same |
| implicit_dynamic | step_hooks | t2 | 5 | 120 | 14.63 ms [13.77 ms, 15.04 ms] | 121.9 µs [114.8 µs, 125.3 µs] | 7.4 [7.0, 7.6] | 1.018 [0.962, 1.036] | same |
| implicit_dynamic | step_hooks | t4 | 5 | 120 | 14.29 ms [12.53 ms, 14.98 ms] | 119.1 µs [104.4 µs, 124.9 µs] | 7.2 [6.3, 7.6] | 0.980 [0.886, 1.076] | same |
| implicit_dynamic | step_hooks | t8 | 5 | 120 | 13.36 ms [12.67 ms, 15.63 ms] | 111.3 µs [105.6 µs, 130.2 µs] | 6.8 [6.4, 7.9] | 0.969 [0.857, 1.037] | same |
| implicit_dynamic | step_rest | base | 5 | 60 | 977.83 ms [851.68 ms, 1.098 s] | 16.30 ms [14.19 ms, 18.30 ms] | 989.9 [862.2, 1111.4] | – | same |
| implicit_dynamic | step_rest | t1 | 5 | 60 | 976.88 ms [931.96 ms, 1.049 s] | 16.28 ms [15.53 ms, 17.48 ms] | 988.9 [943.4, 1061.8] | 0.978 [0.915, 1.094] | same |
| implicit_dynamic | step_rest | t2 | 5 | 60 | 905.29 ms [836.78 ms, 958.81 ms] | 15.09 ms [13.95 ms, 15.98 ms] | 916.4 [847.1, 970.6] | 0.981 [0.788, 0.992] | same |
| implicit_dynamic | step_rest | t4 | 5 | 60 | 845.06 ms [756.86 ms, 870.09 ms] | 14.08 ms [12.61 ms, 14.50 ms] | 855.5 [766.2, 880.8] | 0.829 [0.770, 0.967] | same |
| implicit_dynamic | step_rest | t8 | 5 | 60 | 779.61 ms [760.21 ms, 903.01 ms] | 12.99 ms [12.67 ms, 15.05 ms] | 789.2 [769.6, 914.1] | 0.823 [0.761, 0.915] | same |
| setup_hex8 | mesh | base | 5 | 1 | 1.57 ms [1.56 ms, 1.60 ms] | 1.57 ms [1.56 ms, 1.60 ms] | 113.8 [112.9, 115.7] | – | same |
| setup_hex8 | mesh | t1 | 5 | 1 | 1.61 ms [1.60 ms, 1.71 ms] | 1.61 ms [1.60 ms, 1.71 ms] | 116.8 [115.5, 123.8] | 1.022 [1.013, 1.087] | same |
| setup_hex8 | mesh | t2 | 5 | 1 | 1.62 ms [1.60 ms, 1.64 ms] | 1.62 ms [1.60 ms, 1.64 ms] | 116.9 [115.9, 118.8] | 1.032 [1.002, 1.044] | same |
| setup_hex8 | mesh | t4 | 5 | 1 | 1.62 ms [1.61 ms, 1.68 ms] | 1.62 ms [1.61 ms, 1.68 ms] | 117.0 [116.5, 121.5] | 1.025 [1.007, 1.075] | same |
| setup_hex8 | mesh | t8 | 5 | 1 | 1.61 ms [1.61 ms, 1.71 ms] | 1.61 ms [1.61 ms, 1.71 ms] | 116.5 [116.2, 123.9] | 1.022 [1.007, 1.096] | same |
| setup_hex8 | init_explicit | base | 5 | 1 | 60.96 ms [60.09 ms, 61.83 ms] | 60.96 ms [60.09 ms, 61.83 ms] | 4409.6 [4347.1, 4472.9] | – | same |
| setup_hex8 | init_explicit | t1 | 5 | 1 | 60.23 ms [60.00 ms, 61.27 ms] | 60.23 ms [60.00 ms, 61.27 ms] | 4356.9 [4340.4, 4432.5] | 0.997 [0.972, 1.011] | same |
| setup_hex8 | init_explicit | t2 | 5 | 1 | 61.46 ms [59.75 ms, 62.23 ms] | 61.46 ms [59.75 ms, 62.23 ms] | 4446.0 [4322.1, 4501.4] | 0.995 [0.992, 1.021] | same |
| setup_hex8 | init_explicit | t4 | 5 | 1 | 61.37 ms [61.22 ms, 62.24 ms] | 61.37 ms [61.22 ms, 62.24 ms] | 4439.2 [4428.4, 4502.7] | 1.007 [0.996, 1.036] | same |
| setup_hex8 | init_explicit | t8 | 5 | 1 | 61.49 ms [60.73 ms, 62.81 ms] | 61.49 ms [60.73 ms, 62.81 ms] | 4447.7 [4392.9, 4543.7] | 1.007 [0.994, 1.045] | same |
| setup_hex8 | shape_functions | base | 5 | 3 | 82.92 ms [80.93 ms, 84.00 ms] | 27.64 ms [26.98 ms, 28.00 ms] | 1999.5 [1951.4, 2025.4] | – | same |
| setup_hex8 | shape_functions | t1 | 5 | 3 | 84.92 ms [83.59 ms, 91.36 ms] | 28.31 ms [27.86 ms, 30.45 ms] | 2047.6 [2015.7, 2203.0] | 1.021 [1.013, 1.129] | same |
| setup_hex8 | shape_functions | t2 | 5 | 3 | 83.10 ms [82.29 ms, 85.42 ms] | 27.70 ms [27.43 ms, 28.47 ms] | 2003.7 [1984.2, 2059.8] | 1.015 [0.991, 1.054] | same |
| setup_hex8 | shape_functions | t4 | 5 | 3 | 85.01 ms [83.85 ms, 85.58 ms] | 28.34 ms [27.95 ms, 28.53 ms] | 2049.7 [2021.9, 2063.5] | 1.021 [1.010, 1.050] | same |
| setup_hex8 | shape_functions | t8 | 5 | 3 | 84.92 ms [83.22 ms, 89.47 ms] | 28.31 ms [27.74 ms, 29.82 ms] | 2047.6 [2006.6, 2157.3] | 1.030 [1.002, 1.065] | same |
| setup_hex8 | mass_lumped | base | 5 | 3 | 17.27 ms [16.47 ms, 17.52 ms] | 5.76 ms [5.49 ms, 5.84 ms] | 416.4 [397.1, 422.4] | – | same |
| setup_hex8 | mass_lumped | t1 | 5 | 3 | 16.59 ms [16.24 ms, 16.86 ms] | 5.53 ms [5.41 ms, 5.62 ms] | 400.1 [391.7, 406.4] | 0.962 [0.941, 1.007] | same |
| setup_hex8 | mass_lumped | t2 | 5 | 3 | 16.64 ms [15.14 ms, 16.74 ms] | 5.55 ms [5.05 ms, 5.58 ms] | 401.3 [365.1, 403.7] | 0.969 [0.877, 1.010] | same |
| setup_hex8 | mass_lumped | t4 | 5 | 3 | 17.40 ms [16.99 ms, 17.47 ms] | 5.80 ms [5.66 ms, 5.82 ms] | 419.5 [409.6, 421.3] | 1.011 [0.984, 1.056] | same |
| setup_hex8 | mass_lumped | t8 | 5 | 3 | 18.20 ms [17.58 ms, 19.01 ms] | 6.07 ms [5.86 ms, 6.34 ms] | 438.8 [423.8, 458.4] | 1.067 [1.054, 1.101] | same |
| setup_hex8 | init_implicit | base | 5 | 1 | 55.12 ms [54.09 ms, 56.64 ms] | 55.12 ms [54.09 ms, 56.64 ms] | 3987.4 [3912.6, 4097.2] | – | same |
| setup_hex8 | init_implicit | t1 | 5 | 1 | 55.95 ms [54.93 ms, 57.01 ms] | 55.95 ms [54.93 ms, 57.01 ms] | 4047.3 [3973.3, 4123.9] | 1.007 [0.970, 1.034] | same |
| setup_hex8 | init_implicit | t2 | 5 | 1 | 55.42 ms [52.53 ms, 56.16 ms] | 55.42 ms [52.53 ms, 56.16 ms] | 4009.3 [3799.8, 4062.8] | 0.992 [0.962, 1.025] | same |
| setup_hex8 | init_implicit | t4 | 5 | 1 | 56.64 ms [54.53 ms, 57.28 ms] | 56.64 ms [54.53 ms, 57.28 ms] | 4097.1 [3944.9, 4143.2] | 1.011 [0.998, 1.031] | same |
| setup_hex8 | init_implicit | t8 | 5 | 1 | 55.86 ms [53.36 ms, 57.77 ms] | 55.86 ms [53.36 ms, 57.77 ms] | 4040.9 [3859.9, 4178.8] | 0.986 [0.968, 1.058] | same |
| setup_hex8 | mass_consistent | base | 5 | 3 | 372.38 ms [367.29 ms, 381.44 ms] | 124.13 ms [122.43 ms, 127.15 ms] | 8979.0 [8856.4, 9197.5] | – | same |
| setup_hex8 | mass_consistent | t1 | 5 | 3 | 269.80 ms [268.35 ms, 276.99 ms] | 89.93 ms [89.45 ms, 92.33 ms] | 6505.5 [6470.5, 6678.9] | 0.726 [0.719, 0.743] | same |
| setup_hex8 | mass_consistent | t2 | 5 | 3 | 144.66 ms [141.62 ms, 148.62 ms] | 48.22 ms [47.21 ms, 49.54 ms] | 3488.2 [3414.9, 3583.6] | 0.388 [0.379, 0.403] | same |
| setup_hex8 | mass_consistent | t4 | 5 | 3 | 76.31 ms [75.67 ms, 78.71 ms] | 25.44 ms [25.22 ms, 26.24 ms] | 1839.9 [1824.6, 1898.0] | 0.206 [0.203, 0.214] | same |
| setup_hex8 | mass_consistent | t8 | 5 | 3 | 41.73 ms [40.92 ms, 43.54 ms] | 13.91 ms [13.64 ms, 14.51 ms] | 1006.2 [986.7, 1049.9] | 0.112 [0.111, 0.117] | same |
| setup_hex8 | stiffness_assembly | base | 5 | 3 | 1.529 s [1.519 s, 1.534 s] | 509.56 ms [506.44 ms, 511.21 ms] | 36860.4 [36634.7, 36980.0] | – | same |
| setup_hex8 | stiffness_assembly | t1 | 5 | 3 | 596.36 ms [588.26 ms, 599.30 ms] | 198.79 ms [196.09 ms, 199.77 ms] | 14379.9 [14184.5, 14450.6] | 0.390 [0.385, 0.393] | same |
| setup_hex8 | stiffness_assembly | t2 | 5 | 3 | 316.56 ms [315.33 ms, 322.44 ms] | 105.52 ms [105.11 ms, 107.48 ms] | 7633.0 [7603.5, 7774.9] | 0.208 [0.206, 0.212] | same |
| setup_hex8 | stiffness_assembly | t4 | 5 | 3 | 169.98 ms [168.16 ms, 171.71 ms] | 56.66 ms [56.05 ms, 57.24 ms] | 4098.7 [4054.7, 4140.4] | 0.111 [0.110, 0.113] | same |
| setup_hex8 | stiffness_assembly | t8 | 5 | 3 | 93.55 ms [91.17 ms, 94.88 ms] | 31.18 ms [30.39 ms, 31.63 ms] | 2255.8 [2198.4, 2287.9] | 0.061 [0.060, 0.062] | same |
| setup_tet10 | mesh | base | 5 | 1 | 1.33 ms [1.32 ms, 1.35 ms] | 1.33 ms [1.32 ms, 1.35 ms] | 128.4 [127.3, 130.3] | – | same |
| setup_tet10 | mesh | t1 | 5 | 1 | 1.34 ms [1.31 ms, 1.40 ms] | 1.34 ms [1.31 ms, 1.40 ms] | 129.1 [126.7, 135.0] | 0.995 [0.986, 1.061] | same |
| setup_tet10 | mesh | t2 | 5 | 1 | 1.33 ms [1.30 ms, 1.34 ms] | 1.33 ms [1.30 ms, 1.34 ms] | 128.4 [125.2, 129.4] | 0.996 [0.975, 1.017] | same |
| setup_tet10 | mesh | t4 | 5 | 1 | 1.35 ms [1.34 ms, 1.45 ms] | 1.35 ms [1.34 ms, 1.45 ms] | 130.0 [129.0, 140.1] | 1.013 [0.991, 1.101] | same |
| setup_tet10 | mesh | t8 | 5 | 1 | 1.36 ms [1.33 ms, 1.43 ms] | 1.36 ms [1.33 ms, 1.43 ms] | 130.9 [128.1, 138.4] | 1.020 [0.985, 1.087] | same |
| setup_tet10 | init_explicit | base | 5 | 1 | 30.24 ms [29.85 ms, 30.69 ms] | 30.24 ms [29.85 ms, 30.69 ms] | 2917.1 [2879.4, 2959.9] | – | same |
| setup_tet10 | init_explicit | t1 | 5 | 1 | 29.72 ms [29.44 ms, 31.25 ms] | 29.72 ms [29.44 ms, 31.25 ms] | 2866.4 [2839.7, 3014.5] | 0.986 [0.967, 1.033] | same |
| setup_tet10 | init_explicit | t2 | 5 | 1 | 30.56 ms [30.35 ms, 30.91 ms] | 30.56 ms [30.35 ms, 30.91 ms] | 2947.2 [2927.3, 2981.1] | 1.010 [0.994, 1.028] | same |
| setup_tet10 | init_explicit | t4 | 5 | 1 | 30.33 ms [29.62 ms, 31.94 ms] | 30.33 ms [29.62 ms, 31.94 ms] | 2925.2 [2857.1, 3080.4] | 1.008 [0.965, 1.070] | same |
| setup_tet10 | init_explicit | t8 | 5 | 1 | 30.93 ms [30.57 ms, 31.64 ms] | 30.93 ms [30.57 ms, 31.64 ms] | 2982.8 [2948.3, 3051.4] | 1.021 [1.005, 1.060] | same |
| setup_tet10 | shape_functions | base | 5 | 3 | 41.00 ms [40.02 ms, 41.62 ms] | 13.67 ms [13.34 ms, 13.87 ms] | 1318.2 [1286.8, 1338.2] | – | same |
| setup_tet10 | shape_functions | t1 | 5 | 3 | 39.94 ms [35.88 ms, 42.34 ms] | 13.31 ms [11.96 ms, 14.11 ms] | 1284.2 [1153.5, 1361.3] | 0.982 [0.862, 1.058] | same |
| setup_tet10 | shape_functions | t2 | 5 | 3 | 40.14 ms [37.40 ms, 41.68 ms] | 13.38 ms [12.47 ms, 13.89 ms] | 1290.5 [1202.5, 1340.0] | 0.964 [0.912, 1.041] | same |
| setup_tet10 | shape_functions | t4 | 5 | 3 | 40.19 ms [37.67 ms, 43.64 ms] | 13.40 ms [12.56 ms, 14.55 ms] | 1292.1 [1211.2, 1403.1] | 0.978 [0.912, 1.073] | same |
| setup_tet10 | shape_functions | t8 | 5 | 3 | 40.86 ms [38.34 ms, 41.86 ms] | 13.62 ms [12.78 ms, 13.95 ms] | 1313.5 [1232.6, 1345.7] | 0.982 [0.928, 1.033] | same |
| setup_tet10 | mass_lumped | base | 5 | 3 | 4.22 ms [4.02 ms, 4.23 ms] | 1.41 ms [1.34 ms, 1.41 ms] | 135.6 [129.1, 135.9] | – | same |
| setup_tet10 | mass_lumped | t1 | 5 | 3 | 4.30 ms [3.60 ms, 4.36 ms] | 1.43 ms [1.20 ms, 1.45 ms] | 138.1 [115.9, 140.1] | 1.027 [0.855, 1.070] | same |
| setup_tet10 | mass_lumped | t2 | 5 | 3 | 4.14 ms [4.07 ms, 4.15 ms] | 1.38 ms [1.36 ms, 1.38 ms] | 133.1 [130.8, 133.5] | 0.979 [0.965, 1.032] | same |
| setup_tet10 | mass_lumped | t4 | 5 | 3 | 3.92 ms [3.71 ms, 4.22 ms] | 1.31 ms [1.24 ms, 1.41 ms] | 126.2 [119.2, 135.7] | 0.928 [0.915, 1.002] | same |
| setup_tet10 | mass_lumped | t8 | 5 | 3 | 3.86 ms [3.83 ms, 3.89 ms] | 1.29 ms [1.28 ms, 1.30 ms] | 124.1 [123.0, 125.0] | 0.918 [0.908, 0.961] | same |
| setup_tet10 | init_implicit | base | 5 | 1 | 27.41 ms [25.54 ms, 27.97 ms] | 27.41 ms [25.54 ms, 27.97 ms] | 2643.2 [2463.8, 2698.2] | – | same |
| setup_tet10 | init_implicit | t1 | 5 | 1 | 26.03 ms [23.51 ms, 28.12 ms] | 26.03 ms [23.51 ms, 28.12 ms] | 2510.3 [2267.2, 2711.9] | 0.963 [0.858, 1.023] | same |
| setup_tet10 | init_implicit | t2 | 5 | 1 | 26.97 ms [26.13 ms, 27.82 ms] | 26.97 ms [26.13 ms, 27.82 ms] | 2601.0 [2520.6, 2683.0] | 1.012 [0.964, 1.024] | same |
| setup_tet10 | init_implicit | t4 | 5 | 1 | 27.61 ms [25.12 ms, 28.16 ms] | 27.61 ms [25.12 ms, 28.16 ms] | 2662.6 [2422.5, 2715.8] | 1.007 [0.973, 1.052] | same |
| setup_tet10 | init_implicit | t8 | 5 | 1 | 27.87 ms [24.43 ms, 28.06 ms] | 27.87 ms [24.43 ms, 28.06 ms] | 2687.7 [2356.2, 2706.0] | 1.002 [0.926, 1.080] | same |
| setup_tet10 | mass_consistent | base | 5 | 3 | 310.77 ms [308.18 ms, 315.92 ms] | 103.59 ms [102.73 ms, 105.31 ms] | 9991.4 [9908.0, 10157.0] | – | same |
| setup_tet10 | mass_consistent | t1 | 5 | 3 | 211.64 ms [208.05 ms, 221.04 ms] | 70.55 ms [69.35 ms, 73.68 ms] | 6804.1 [6689.0, 7106.6] | 0.678 [0.667, 0.716] | same |
| setup_tet10 | mass_consistent | t2 | 5 | 3 | 111.74 ms [107.96 ms, 114.52 ms] | 37.25 ms [35.99 ms, 38.17 ms] | 3592.5 [3471.0, 3681.7] | 0.362 [0.346, 0.368] | same |
| setup_tet10 | mass_consistent | t4 | 5 | 3 | 55.91 ms [54.94 ms, 58.68 ms] | 18.64 ms [18.31 ms, 19.56 ms] | 1797.6 [1766.3, 1886.7] | 0.179 [0.177, 0.188] | same |
| setup_tet10 | mass_consistent | t8 | 5 | 3 | 32.17 ms [30.72 ms, 32.63 ms] | 10.72 ms [10.24 ms, 10.88 ms] | 1034.3 [987.6, 1049.2] | 0.103 [0.099, 0.105] | same |
| setup_tet10 | stiffness_assembly | base | 5 | 3 | 970.57 ms [964.32 ms, 982.33 ms] | 323.52 ms [321.44 ms, 327.44 ms] | 31204.1 [31003.2, 31582.1] | – | same |
| setup_tet10 | stiffness_assembly | t1 | 5 | 3 | 370.98 ms [362.17 ms, 380.77 ms] | 123.66 ms [120.72 ms, 126.92 ms] | 11927.1 [11643.7, 12241.8] | 0.382 [0.369, 0.395] | same |
| setup_tet10 | stiffness_assembly | t2 | 5 | 3 | 199.77 ms [192.29 ms, 205.68 ms] | 66.59 ms [64.10 ms, 68.56 ms] | 6422.5 [6182.2, 6612.8] | 0.207 [0.199, 0.211] | same |
| setup_tet10 | stiffness_assembly | t4 | 5 | 3 | 104.79 ms [99.71 ms, 106.45 ms] | 34.93 ms [33.24 ms, 35.48 ms] | 3369.1 [3205.8, 3422.3] | 0.107 [0.103, 0.110] | same |
| setup_tet10 | stiffness_assembly | t8 | 5 | 3 | 55.49 ms [54.90 ms, 56.30 ms] | 18.50 ms [18.30 ms, 18.77 ms] | 1784.0 [1765.0, 1810.1] | 0.057 [0.056, 0.058] | same |
| setup_tet4 | mesh | base | 5 | 1 | 5.64 ms [5.63 ms, 5.77 ms] | 5.64 ms [5.63 ms, 5.77 ms] | 34.8 [34.7, 35.6] | – | same |
| setup_tet4 | mesh | t1 | 5 | 1 | 5.88 ms [5.81 ms, 6.03 ms] | 5.88 ms [5.81 ms, 6.03 ms] | 36.3 [35.8, 37.2] | 1.032 [1.007, 1.068] | same |
| setup_tet4 | mesh | t2 | 5 | 1 | 5.85 ms [5.74 ms, 5.86 ms] | 5.85 ms [5.74 ms, 5.86 ms] | 36.1 [35.4, 36.2] | 1.033 [0.998, 1.039] | same |
| setup_tet4 | mesh | t4 | 5 | 1 | 5.87 ms [5.83 ms, 5.90 ms] | 5.87 ms [5.83 ms, 5.90 ms] | 36.2 [36.0, 36.4] | 1.036 [1.021, 1.049] | same |
| setup_tet4 | mesh | t8 | 5 | 1 | 5.93 ms [5.81 ms, 6.00 ms] | 5.93 ms [5.81 ms, 6.00 ms] | 36.6 [35.8, 37.1] | 1.032 [1.008, 1.064] | same |
| setup_tet4 | init_explicit | base | 5 | 1 | 82.56 ms [82.13 ms, 85.60 ms] | 82.56 ms [82.13 ms, 85.60 ms] | 509.7 [507.0, 528.4] | – | same |
| setup_tet4 | init_explicit | t1 | 5 | 1 | 83.41 ms [82.09 ms, 84.24 ms] | 83.41 ms [82.09 ms, 84.24 ms] | 514.9 [506.7, 520.0] | 1.000 [0.984, 1.016] | same |
| setup_tet4 | init_explicit | t2 | 5 | 1 | 83.02 ms [81.40 ms, 83.34 ms] | 83.02 ms [81.40 ms, 83.34 ms] | 512.4 [502.5, 514.4] | 0.989 [0.967, 1.015] | same |
| setup_tet4 | init_explicit | t4 | 5 | 1 | 82.15 ms [81.83 ms, 85.36 ms] | 82.15 ms [81.83 ms, 85.36 ms] | 507.1 [505.1, 526.9] | 0.997 [0.958, 1.014] | same |
| setup_tet4 | init_explicit | t8 | 5 | 1 | 82.92 ms [82.49 ms, 84.34 ms] | 82.92 ms [82.49 ms, 84.34 ms] | 511.8 [509.2, 520.6] | 1.004 [0.983, 1.008] | same |
| setup_tet4 | shape_functions | base | 5 | 3 | 104.22 ms [102.05 ms, 104.80 ms] | 34.74 ms [34.02 ms, 34.93 ms] | 214.4 [210.0, 215.6] | – | same |
| setup_tet4 | shape_functions | t1 | 5 | 3 | 103.71 ms [99.03 ms, 107.89 ms] | 34.57 ms [33.01 ms, 35.96 ms] | 213.4 [203.8, 222.0] | 0.995 [0.970, 1.030] | same |
| setup_tet4 | shape_functions | t2 | 5 | 3 | 103.78 ms [103.25 ms, 105.55 ms] | 34.59 ms [34.42 ms, 35.18 ms] | 213.5 [212.4, 217.2] | 1.006 [0.985, 1.034] | same |
| setup_tet4 | shape_functions | t4 | 5 | 3 | 104.37 ms [100.52 ms, 108.62 ms] | 34.79 ms [33.51 ms, 36.21 ms] | 214.7 [206.8, 223.5] | 1.001 [0.962, 1.064] | same |
| setup_tet4 | shape_functions | t8 | 5 | 3 | 104.46 ms [102.56 ms, 104.75 ms] | 34.82 ms [34.19 ms, 34.92 ms] | 214.9 [211.0, 215.5] | 1.004 [0.979, 1.024] | same |
| setup_tet4 | mass_lumped | base | 5 | 3 | 65.75 ms [63.22 ms, 68.62 ms] | 21.92 ms [21.07 ms, 22.87 ms] | 135.3 [130.1, 141.2] | – | same |
| setup_tet4 | mass_lumped | t1 | 5 | 3 | 66.89 ms [64.45 ms, 67.23 ms] | 22.30 ms [21.48 ms, 22.41 ms] | 137.6 [132.6, 138.3] | 1.017 [0.947, 1.021] | same |
| setup_tet4 | mass_lumped | t2 | 5 | 3 | 66.62 ms [65.38 ms, 67.72 ms] | 22.21 ms [21.79 ms, 22.57 ms] | 137.1 [134.5, 139.3] | 1.021 [0.968, 1.051] | same |
| setup_tet4 | mass_lumped | t4 | 5 | 3 | 66.43 ms [64.49 ms, 67.66 ms] | 22.14 ms [21.50 ms, 22.55 ms] | 136.7 [132.7, 139.2] | 0.986 [0.981, 1.029] | same |
| setup_tet4 | mass_lumped | t8 | 5 | 3 | 66.58 ms [64.16 ms, 68.82 ms] | 22.19 ms [21.39 ms, 22.94 ms] | 137.0 [132.0, 141.6] | 1.004 [0.935, 1.053] | same |
| setup_tet4 | init_implicit | base | 5 | 1 | 78.78 ms [76.37 ms, 79.18 ms] | 78.78 ms [76.37 ms, 79.18 ms] | 486.3 [471.4, 488.7] | – | same |
| setup_tet4 | init_implicit | t1 | 5 | 1 | 77.54 ms [75.37 ms, 80.08 ms] | 77.54 ms [75.37 ms, 80.08 ms] | 478.7 [465.3, 494.3] | 1.004 [0.956, 1.027] | same |
| setup_tet4 | init_implicit | t2 | 5 | 1 | 79.20 ms [77.41 ms, 81.00 ms] | 79.20 ms [77.41 ms, 81.00 ms] | 488.9 [477.8, 500.0] | 1.017 [0.979, 1.039] | same |
| setup_tet4 | init_implicit | t4 | 5 | 1 | 77.60 ms [76.85 ms, 81.36 ms] | 77.60 ms [76.85 ms, 81.36 ms] | 479.0 [474.4, 502.2] | 0.986 [0.975, 1.033] | same |
| setup_tet4 | init_implicit | t8 | 5 | 1 | 80.02 ms [75.63 ms, 81.25 ms] | 80.02 ms [75.63 ms, 81.25 ms] | 493.9 [466.9, 501.6] | 1.014 [0.960, 1.064] | same |
| setup_tet4 | mass_consistent | base | 5 | 3 | 649.73 ms [649.23 ms, 662.01 ms] | 216.58 ms [216.41 ms, 220.67 ms] | 1336.9 [1335.9, 1362.2] | – | same |
| setup_tet4 | mass_consistent | t1 | 5 | 3 | 388.03 ms [383.04 ms, 388.77 ms] | 129.34 ms [127.68 ms, 129.59 ms] | 798.4 [788.1, 799.9] | 0.591 [0.587, 0.599] | same |
| setup_tet4 | mass_consistent | t2 | 5 | 3 | 225.59 ms [224.25 ms, 226.69 ms] | 75.20 ms [74.75 ms, 75.56 ms] | 464.2 [461.4, 466.4] | 0.345 [0.341, 0.349] | same |
| setup_tet4 | mass_consistent | t4 | 5 | 3 | 134.09 ms [131.19 ms, 136.89 ms] | 44.70 ms [43.73 ms, 45.63 ms] | 275.9 [269.9, 281.7] | 0.205 [0.202, 0.209] | same |
| setup_tet4 | mass_consistent | t8 | 5 | 3 | 91.79 ms [91.27 ms, 93.44 ms] | 30.60 ms [30.42 ms, 31.15 ms] | 188.9 [187.8, 192.3] | 0.141 [0.140, 0.144] | same |
| setup_tet4 | stiffness_assembly | base | 5 | 3 | 1.100 s [1.092 s, 1.106 s] | 366.67 ms [363.92 ms, 368.69 ms] | 2263.4 [2246.4, 2275.9] | – | same |
| setup_tet4 | stiffness_assembly | t1 | 5 | 3 | 540.83 ms [536.68 ms, 546.26 ms] | 180.28 ms [178.89 ms, 182.09 ms] | 1112.8 [1104.3, 1124.0] | 0.492 [0.485, 0.497] | same |
| setup_tet4 | stiffness_assembly | t2 | 5 | 3 | 307.21 ms [305.52 ms, 315.04 ms] | 102.40 ms [101.84 ms, 105.01 ms] | 632.1 [628.6, 648.2] | 0.279 [0.277, 0.289] | same |
| setup_tet4 | stiffness_assembly | t4 | 5 | 3 | 183.96 ms [182.47 ms, 184.37 ms] | 61.32 ms [60.82 ms, 61.46 ms] | 378.5 [375.5, 379.4] | 0.167 [0.166, 0.169] | same |
| setup_tet4 | stiffness_assembly | t8 | 5 | 3 | 120.86 ms [119.62 ms, 122.97 ms] | 40.29 ms [39.87 ms, 40.99 ms] | 248.7 [246.1, 253.0] | 0.109 [0.109, 0.113] | same |
| static_linear_eigen | mesh | base | 5 | 1 | 662.9 µs [662.1 µs, 676.3 µs] | 662.9 µs [662.1 µs, 676.3 µs] | 40.3 [40.2, 41.1] | – | same |
| static_linear_eigen | mesh | t1 | 5 | 1 | 685.0 µs [623.6 µs, 697.9 µs] | 685.0 µs [623.6 µs, 697.9 µs] | 41.6 [37.9, 42.4] | 1.028 [0.941, 1.053] | same |
| static_linear_eigen | mesh | t2 | 5 | 1 | 674.5 µs [622.3 µs, 698.4 µs] | 674.5 µs [622.3 µs, 698.4 µs] | 41.0 [37.8, 42.4] | 1.019 [0.939, 1.053] | same |
| static_linear_eigen | mesh | t4 | 5 | 1 | 679.0 µs [674.5 µs, 716.5 µs] | 679.0 µs [674.5 µs, 716.5 µs] | 41.2 [41.0, 43.5] | 1.023 [0.997, 1.082] | same |
| static_linear_eigen | mesh | t8 | 5 | 1 | 701.2 µs [669.2 µs, 726.7 µs] | 701.2 µs [669.2 µs, 726.7 µs] | 42.6 [40.6, 44.1] | 1.055 [1.009, 1.074] | same |
| static_linear_eigen | init | base | 5 | 1 | 8.66 ms [8.55 ms, 8.86 ms] | 8.66 ms [8.55 ms, 8.86 ms] | 526.2 [519.4, 538.4] | – | same |
| static_linear_eigen | init | t1 | 5 | 1 | 8.50 ms [7.99 ms, 8.58 ms] | 8.50 ms [7.99 ms, 8.58 ms] | 516.2 [485.1, 521.4] | 0.978 [0.934, 0.997] | same |
| static_linear_eigen | init | t2 | 5 | 1 | 8.59 ms [8.26 ms, 8.62 ms] | 8.59 ms [8.26 ms, 8.62 ms] | 521.6 [501.5, 523.8] | 0.993 [0.965, 0.998] | same |
| static_linear_eigen | init | t4 | 5 | 1 | 8.66 ms [8.54 ms, 8.82 ms] | 8.66 ms [8.54 ms, 8.82 ms] | 526.2 [518.9, 535.8] | 1.003 [0.964, 1.025] | same |
| static_linear_eigen | init | t8 | 5 | 1 | 9.02 ms [8.87 ms, 9.30 ms] | 9.02 ms [8.87 ms, 9.30 ms] | 547.8 [538.7, 565.1] | 1.044 [1.024, 1.049] | same |
| static_linear_eigen | solve_step | base | 5 | 1 | 787.38 ms [777.35 ms, 806.06 ms] | 787.38 ms [777.35 ms, 806.06 ms] | 47824.6 [47214.9, 48959.1] | – | same |
| static_linear_eigen | solve_step | t1 | 5 | 1 | 757.26 ms [754.00 ms, 772.63 ms] | 757.26 ms [754.00 ms, 772.63 ms] | 45995.0 [45797.1, 46928.4] | 0.965 [0.938, 0.981] | same |
| static_linear_eigen | solve_step | t2 | 5 | 1 | 750.82 ms [744.02 ms, 753.17 ms] | 750.82 ms [744.02 ms, 753.17 ms] | 45604.0 [45190.5, 45746.7] | 0.950 [0.934, 0.958] | same |
| static_linear_eigen | solve_step | t4 | 5 | 1 | 749.51 ms [739.08 ms, 755.85 ms] | 749.51 ms [739.08 ms, 755.85 ms] | 45524.4 [44891.0, 45909.4] | 0.953 [0.938, 0.959] | same |
| static_linear_eigen | solve_step | t8 | 5 | 1 | 752.87 ms [750.30 ms, 757.08 ms] | 752.87 ms [750.30 ms, 757.08 ms] | 45728.2 [45572.4, 45983.7] | 0.962 [0.931, 0.968] | same |
| static_linear_eigen | predictor | base | 5 | 1 | 0.3 µs [0.2 µs, 0.4 µs] | 0.3 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | t1 | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.816 [0.371, 0.990] | same |
| static_linear_eigen | predictor | t2 | 5 | 1 | 0.2 µs [0.1 µs, 0.4 µs] | 0.2 µs [0.1 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.839 [0.391, 1.298] | same |
| static_linear_eigen | predictor | t4 | 5 | 1 | 0.2 µs [0.1 µs, 0.3 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.751 [0.433, 1.004] | same |
| static_linear_eigen | predictor | t8 | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.833 [0.538, 1.008] | same |
| static_linear_eigen | matrix | base | 5 | 1 | 75.74 ms [72.97 ms, 77.28 ms] | 75.74 ms [72.97 ms, 77.28 ms] | 4600.3 [4432.3, 4693.9] | – | same |
| static_linear_eigen | matrix | t1 | 5 | 1 | 70.92 ms [67.33 ms, 71.60 ms] | 70.92 ms [67.33 ms, 71.60 ms] | 4307.5 [4089.5, 4349.1] | 0.927 [0.923, 0.938] | same |
| static_linear_eigen | matrix | t2 | 5 | 1 | 64.81 ms [64.50 ms, 65.28 ms] | 64.81 ms [64.50 ms, 65.28 ms] | 3936.6 [3917.7, 3964.8] | 0.855 [0.843, 0.884] | same |
| static_linear_eigen | matrix | t4 | 5 | 1 | 61.21 ms [60.43 ms, 62.19 ms] | 61.21 ms [60.43 ms, 62.19 ms] | 3717.8 [3670.7, 3777.5] | 0.821 [0.782, 0.836] | same |
| static_linear_eigen | matrix | t8 | 5 | 1 | 60.93 ms [60.20 ms, 61.77 ms] | 60.93 ms [60.20 ms, 61.77 ms] | 3700.6 [3656.6, 3751.6] | 0.810 [0.785, 0.835] | same |
| static_linear_eigen | corrector | base | 5 | 1 | 0.6 µs [0.5 µs, 0.6 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | corrector | t1 | 5 | 1 | 0.6 µs [0.4 µs, 0.9 µs] | 0.6 µs [0.4 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.060 [0.860, 1.886] | same |
| static_linear_eigen | corrector | t2 | 5 | 1 | 0.5 µs [0.5 µs, 0.7 µs] | 0.5 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.986 [0.759, 1.237] | same |
| static_linear_eigen | corrector | t4 | 5 | 1 | 0.6 µs [0.5 µs, 0.7 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 1.006 [0.869, 1.203] | same |
| static_linear_eigen | corrector | t8 | 5 | 1 | 0.6 µs [0.4 µs, 0.6 µs] | 0.6 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.004 [0.767, 1.397] | same |
| static_linear_eigen | step_hooks | base | 5 | 2 | 273.0 µs [261.7 µs, 276.6 µs] | 136.5 µs [130.8 µs, 138.3 µs] | 8.3 [7.9, 8.4] | – | same |
| static_linear_eigen | step_hooks | t1 | 5 | 2 | 272.4 µs [261.1 µs, 273.9 µs] | 136.2 µs [130.6 µs, 136.9 µs] | 8.3 [7.9, 8.3] | 0.989 [0.964, 1.007] | same |
| static_linear_eigen | step_hooks | t2 | 5 | 2 | 269.5 µs [266.9 µs, 284.6 µs] | 134.8 µs [133.4 µs, 142.3 µs] | 8.2 [8.1, 8.6] | 1.020 [0.971, 1.040] | same |
| static_linear_eigen | step_hooks | t4 | 5 | 2 | 261.8 µs [197.6 µs, 272.8 µs] | 130.9 µs [98.8 µs, 136.4 µs] | 8.0 [6.0, 8.3] | 0.959 [0.729, 0.995] | same |
| static_linear_eigen | step_hooks | t8 | 5 | 2 | 273.0 µs [263.7 µs, 280.9 µs] | 136.5 µs [131.8 µs, 140.4 µs] | 8.3 [8.0, 8.5] | 0.994 [0.966, 1.044] | same |
| static_linear_eigen | step_rest | base | 5 | 1 | 710.50 ms [704.10 ms, 728.93 ms] | 710.50 ms [704.10 ms, 728.93 ms] | 43154.9 [42766.1, 44273.9] | – | same |
| static_linear_eigen | step_rest | t1 | 5 | 1 | 687.69 ms [684.63 ms, 700.75 ms] | 687.69 ms [684.63 ms, 700.75 ms] | 41769.0 [41583.4, 42562.7] | 0.968 [0.939, 0.987] | same |
| static_linear_eigen | step_rest | t2 | 5 | 1 | 685.66 ms [679.23 ms, 688.14 ms] | 685.66 ms [679.23 ms, 688.14 ms] | 41646.2 [41255.6, 41796.6] | 0.959 [0.943, 0.969] | same |
| static_linear_eigen | step_rest | t4 | 5 | 1 | 687.06 ms [678.38 ms, 694.38 ms] | 687.06 ms [678.38 ms, 694.38 ms] | 41731.1 [41203.6, 42175.5] | 0.967 [0.953, 0.972] | same |
| static_linear_eigen | step_rest | t8 | 5 | 1 | 692.40 ms [688.26 ms, 696.15 ms] | 692.40 ms [688.26 ms, 696.15 ms] | 42055.6 [41804.1, 42283.1] | 0.978 [0.944, 0.981] | same |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 88.30 ms [83.04 ms, 90.44 ms] | 29.43 ms [27.68 ms, 30.15 ms] | 1787.7 [1681.2, 1831.1] | – | same |
| static_linear_eigen | stiffness_assembly | t1 | 5 | 3 | 44.63 ms [43.32 ms, 45.93 ms] | 14.88 ms [14.44 ms, 15.31 ms] | 903.6 [877.0, 929.9] | 0.520 [0.486, 0.540] | same |
| static_linear_eigen | stiffness_assembly | t2 | 5 | 3 | 24.54 ms [23.96 ms, 25.13 ms] | 8.18 ms [7.99 ms, 8.38 ms] | 496.9 [485.2, 508.8] | 0.275 [0.275, 0.303] | same |
| static_linear_eigen | stiffness_assembly | t4 | 5 | 3 | 13.86 ms [13.61 ms, 14.00 ms] | 4.62 ms [4.54 ms, 4.67 ms] | 280.5 [275.5, 283.5] | 0.159 [0.153, 0.167] | same |
| static_linear_eigen | stiffness_assembly | t8 | 5 | 3 | 9.14 ms [8.96 ms, 9.27 ms] | 3.05 ms [2.99 ms, 3.09 ms] | 185.1 [181.5, 187.7] | 0.105 [0.101, 0.112] | same |
| static_linear_eigen | analysis | base | 5 | 1 | 50.99 ms [47.59 ms, 52.25 ms] | 50.99 ms [47.59 ms, 52.25 ms] | 3097.3 [2890.3, 3173.4] | – | same |
| static_linear_eigen | analysis | t1 | 5 | 1 | 51.71 ms [47.34 ms, 52.00 ms] | 51.71 ms [47.34 ms, 52.00 ms] | 3140.7 [2875.3, 3158.3] | 0.994 [0.985, 1.087] | same |
| static_linear_eigen | analysis | t2 | 5 | 1 | 53.93 ms [50.55 ms, 54.03 ms] | 53.93 ms [50.55 ms, 54.03 ms] | 3275.9 [3070.2, 3281.5] | 1.058 [1.015, 1.123] | same |
| static_linear_eigen | analysis | t4 | 5 | 1 | 51.57 ms [50.87 ms, 52.32 ms] | 51.57 ms [50.87 ms, 52.32 ms] | 3132.4 [3089.8, 3178.1] | 1.021 [0.974, 1.089] | same |
| static_linear_eigen | analysis | t8 | 5 | 1 | 51.41 ms [50.02 ms, 52.28 ms] | 51.41 ms [50.02 ms, 52.28 ms] | 3122.7 [3038.1, 3175.7] | 1.007 [0.981, 1.099] | same |
| static_linear_eigen | factorize | base | 5 | 2 | 1.137 s [1.136 s, 1.146 s] | 568.46 ms [568.19 ms, 572.96 ms] | 34527.4 [34510.8, 34800.5] | – | same |
| static_linear_eigen | factorize | t1 | 5 | 2 | 1.122 s [1.119 s, 1.131 s] | 560.96 ms [559.56 ms, 565.41 ms] | 34072.1 [33986.8, 34342.3] | 0.986 [0.985, 0.987] | same |
| static_linear_eigen | factorize | t2 | 5 | 2 | 1.128 s [1.116 s, 1.130 s] | 564.11 ms [557.86 ms, 564.89 ms] | 34263.1 [33883.8, 34310.6] | 0.985 [0.980, 0.994] | same |
| static_linear_eigen | factorize | t4 | 5 | 2 | 1.125 s [1.111 s, 1.131 s] | 562.35 ms [555.34 ms, 565.49 ms] | 34156.6 [33730.4, 34347.0] | 0.987 [0.977, 0.995] | same |
| static_linear_eigen | factorize | t8 | 5 | 2 | 1.126 s [1.108 s, 1.132 s] | 562.94 ms [553.88 ms, 565.92 ms] | 34192.2 [33641.9, 34373.1] | 0.988 [0.975, 0.991] | same |
| static_linear_eigen | back_substitution | base | 5 | 20 | 114.28 ms [105.99 ms, 129.86 ms] | 5.71 ms [5.30 ms, 6.49 ms] | 347.1 [321.9, 394.4] | – | same |
| static_linear_eigen | back_substitution | t1 | 5 | 20 | 107.75 ms [101.41 ms, 124.28 ms] | 5.39 ms [5.07 ms, 6.21 ms] | 327.2 [308.0, 377.4] | 0.943 [0.895, 0.997] | same |
| static_linear_eigen | back_substitution | t2 | 5 | 20 | 105.35 ms [101.59 ms, 115.34 ms] | 5.27 ms [5.08 ms, 5.77 ms] | 320.0 [308.5, 350.3] | 0.922 [0.815, 0.995] | same |
| static_linear_eigen | back_substitution | t4 | 5 | 20 | 111.20 ms [103.65 ms, 115.25 ms] | 5.56 ms [5.18 ms, 5.76 ms] | 337.7 [314.8, 350.0] | 0.922 [0.856, 1.009] | same |
| static_linear_eigen | back_substitution | t8 | 5 | 20 | 111.06 ms [103.30 ms, 111.50 ms] | 5.55 ms [5.16 ms, 5.58 ms] | 337.3 [313.7, 338.6] | 0.911 [0.844, 1.050] | same |
| static_newton_plastic | mesh | base | 5 | 1 | 264.9 µs [262.8 µs, 277.1 µs] | 264.9 µs [262.8 µs, 277.1 µs] | 44.2 [43.8, 46.2] | – | same |
| static_newton_plastic | mesh | t1 | 5 | 1 | 287.5 µs [249.9 µs, 290.6 µs] | 287.5 µs [249.9 µs, 290.6 µs] | 47.9 [41.7, 48.4] | 1.054 [0.910, 1.102] | same |
| static_newton_plastic | mesh | t2 | 5 | 1 | 286.9 µs [271.6 µs, 351.6 µs] | 286.9 µs [271.6 µs, 351.6 µs] | 47.8 [45.3, 58.6] | 1.045 [0.983, 1.338] | same |
| static_newton_plastic | mesh | t4 | 5 | 1 | 283.2 µs [269.7 µs, 291.6 µs] | 283.2 µs [269.7 µs, 291.6 µs] | 47.2 [44.9, 48.6] | 1.052 [0.982, 1.074] | same |
| static_newton_plastic | mesh | t8 | 5 | 1 | 279.1 µs [269.6 µs, 292.8 µs] | 279.1 µs [269.6 µs, 292.8 µs] | 46.5 [44.9, 48.8] | 1.031 [1.009, 1.066] | same |
| static_newton_plastic | init | base | 5 | 1 | 3.83 ms [3.81 ms, 3.85 ms] | 3.83 ms [3.81 ms, 3.85 ms] | 637.6 [634.6, 641.5] | – | same |
| static_newton_plastic | init | t1 | 5 | 1 | 3.49 ms [3.12 ms, 3.82 ms] | 3.49 ms [3.12 ms, 3.82 ms] | 581.5 [520.6, 636.3] | 0.913 [0.811, 1.003] | same |
| static_newton_plastic | init | t2 | 5 | 1 | 3.85 ms [3.81 ms, 4.12 ms] | 3.85 ms [3.81 ms, 4.12 ms] | 642.1 [634.5, 686.0] | 1.012 [0.996, 1.071] | same |
| static_newton_plastic | init | t4 | 5 | 1 | 4.02 ms [3.97 ms, 4.15 ms] | 4.02 ms [3.97 ms, 4.15 ms] | 669.7 [661.4, 690.8] | 1.050 [1.031, 1.085] | same |
| static_newton_plastic | init | t8 | 5 | 1 | 4.15 ms [4.09 ms, 4.32 ms] | 4.15 ms [4.09 ms, 4.32 ms] | 691.7 [682.3, 719.9] | 1.080 [1.075, 1.129] | same |
| static_newton_plastic | solve_step | base | 5 | 12 | 1.970 s [1.961 s, 1.983 s] | 164.14 ms [163.38 ms, 165.24 ms] | 27356.5 [27230.3, 27540.1] | – | same |
| static_newton_plastic | solve_step | t1 | 5 | 12 | 1.938 s [1.925 s, 1.952 s] | 161.51 ms [160.43 ms, 162.63 ms] | 26917.7 [26737.8, 27104.3] | 0.984 [0.977, 0.985] | same |
| static_newton_plastic | solve_step | t2 | 5 | 12 | 1.841 s [1.823 s, 1.848 s] | 153.39 ms [151.93 ms, 154.02 ms] | 25564.3 [25321.0, 25670.2] | 0.930 [0.919, 0.943] | same |
| static_newton_plastic | solve_step | t4 | 5 | 12 | 1.841 s [1.829 s, 1.865 s] | 153.38 ms [152.38 ms, 155.40 ms] | 25563.9 [25397.1, 25900.0] | 0.930 [0.928, 0.951] | same |
| static_newton_plastic | solve_step | t8 | 5 | 12 | 1.830 s [1.820 s, 1.843 s] | 152.47 ms [151.67 ms, 153.60 ms] | 25411.1 [25278.7, 25600.2] | 0.929 [0.920, 0.931] | same |
| static_newton_plastic | predictor | base | 5 | 12 | 1.5 µs [1.2 µs, 1.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | predictor | t1 | 5 | 12 | 1.5 µs [1.4 µs, 1.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.992 [0.884, 1.255] | same |
| static_newton_plastic | predictor | t2 | 5 | 12 | 1.6 µs [1.4 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.028 [0.897, 1.299] | same |
| static_newton_plastic | predictor | t4 | 5 | 12 | 1.5 µs [1.4 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.092 [0.902, 1.216] | same |
| static_newton_plastic | predictor | t8 | 5 | 12 | 1.5 µs [1.3 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.076 [0.814, 1.298] | same |
| static_newton_plastic | residual | base | 5 | 961 | 792.71 ms [790.03 ms, 796.15 ms] | 824.9 µs [822.1 µs, 828.5 µs] | 137.5 [137.0, 138.1] | – | same |
| static_newton_plastic | residual | t1 | 5 | 961 | 792.06 ms [790.76 ms, 800.61 ms] | 824.2 µs [822.8 µs, 833.1 µs] | 137.4 [137.1, 138.9] | 0.999 [0.994, 1.008] | same |
| static_newton_plastic | residual | t2 | 5 | 961 | 685.31 ms [680.70 ms, 687.72 ms] | 713.1 µs [708.3 µs, 715.6 µs] | 118.9 [118.1, 119.3] | 0.864 [0.856, 0.869] | same |
| static_newton_plastic | residual | t4 | 5 | 961 | 677.69 ms [673.03 ms, 699.39 ms] | 705.2 µs [700.3 µs, 727.8 µs] | 117.5 [116.7, 121.3] | 0.852 [0.850, 0.882] | same |
| static_newton_plastic | residual | t8 | 5 | 961 | 659.30 ms [656.40 ms, 661.69 ms] | 686.1 µs [683.0 µs, 688.5 µs] | 114.3 [113.8, 114.8] | 0.831 [0.828, 0.834] | same |
| static_newton_plastic | matrix | base | 5 | 949 | 573.9 µs [521.5 µs, 656.6 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.1 [0.1, 0.1] | – | same |
| static_newton_plastic | matrix | t1 | 5 | 949 | 603.0 µs [573.3 µs, 621.1 µs] | 0.6 µs [0.6 µs, 0.7 µs] | 0.1 [0.1, 0.1] | 1.064 [0.919, 1.156] | same |
| static_newton_plastic | matrix | t2 | 5 | 949 | 555.6 µs [524.0 µs, 564.7 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.930 [0.860, 1.065] | same |
| static_newton_plastic | matrix | t4 | 5 | 949 | 559.8 µs [558.6 µs, 582.6 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.991 [0.860, 1.073] | same |
| static_newton_plastic | matrix | t8 | 5 | 949 | 534.8 µs [512.3 µs, 595.5 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.976 [0.780, 1.038] | same |
| static_newton_plastic | corrector | base | 5 | 949 | 150.9 µs [128.9 µs, 164.3 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | corrector | t1 | 5 | 949 | 143.6 µs [130.2 µs, 151.4 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.951 [0.857, 1.030] | same |
| static_newton_plastic | corrector | t2 | 5 | 949 | 143.7 µs [110.2 µs, 147.1 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.945 [0.749, 1.141] | same |
| static_newton_plastic | corrector | t4 | 5 | 949 | 148.7 µs [118.7 µs, 153.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.985 [0.723, 1.162] | same |
| static_newton_plastic | corrector | t8 | 5 | 949 | 154.5 µs [141.3 µs, 157.7 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.027 [0.874, 1.224] | same |
| static_newton_plastic | step_hooks | base | 5 | 24 | 1.61 ms [1.58 ms, 1.65 ms] | 66.9 µs [65.9 µs, 68.7 µs] | 11.2 [11.0, 11.4] | – | same |
| static_newton_plastic | step_hooks | t1 | 5 | 24 | 1.61 ms [1.59 ms, 1.63 ms] | 66.9 µs [66.1 µs, 67.8 µs] | 11.2 [11.0, 11.3] | 0.996 [0.974, 1.013] | same |
| static_newton_plastic | step_hooks | t2 | 5 | 24 | 1.59 ms [1.54 ms, 1.61 ms] | 66.3 µs [64.1 µs, 67.2 µs] | 11.0 [10.7, 11.2] | 0.971 [0.958, 1.020] | same |
| static_newton_plastic | step_hooks | t4 | 5 | 24 | 1.59 ms [1.56 ms, 1.61 ms] | 66.2 µs [65.2 µs, 67.2 µs] | 11.0 [10.9, 11.2] | 0.990 [0.978, 1.002] | same |
| static_newton_plastic | step_hooks | t8 | 5 | 24 | 1.57 ms [1.54 ms, 1.58 ms] | 65.4 µs [64.2 µs, 66.0 µs] | 10.9 [10.7, 11.0] | 0.977 [0.952, 1.002] | same |
| static_newton_plastic | step_rest | base | 5 | 12 | 1.176 s [1.166 s, 1.185 s] | 98.01 ms [97.13 ms, 98.77 ms] | 16334.7 [16188.0, 16461.4] | – | same |
| static_newton_plastic | step_rest | t1 | 5 | 12 | 1.139 s [1.132 s, 1.149 s] | 94.93 ms [94.33 ms, 95.71 ms] | 15820.8 [15722.3, 15951.7] | 0.969 [0.963, 0.975] | same |
| static_newton_plastic | step_rest | t2 | 5 | 12 | 1.152 s [1.139 s, 1.161 s] | 95.98 ms [94.88 ms, 96.72 ms] | 15996.1 [15814.0, 16119.5] | 0.975 [0.962, 0.996] | same |
| static_newton_plastic | step_rest | t4 | 5 | 12 | 1.161 s [1.153 s, 1.163 s] | 96.73 ms [96.10 ms, 96.92 ms] | 16122.0 [16017.1, 16154.1] | 0.983 [0.978, 0.998] | same |
| static_newton_plastic | step_rest | t8 | 5 | 12 | 1.168 s [1.161 s, 1.179 s] | 97.32 ms [96.78 ms, 98.27 ms] | 16219.3 [16129.4, 16378.6] | 0.995 [0.982, 0.998] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 72337cb4e, ea071de35 | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 72337cb4e, ea071de35 | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 72337cb4e, ea071de35 | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 72337cb4e, ea071de35 | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 72337cb4e, ea071de35 | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 72337cb4e, ea071de35 | d5869f57a777d9a5 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 72337cb4e, ea071de35 | 7bd5f114957d7d7b |
