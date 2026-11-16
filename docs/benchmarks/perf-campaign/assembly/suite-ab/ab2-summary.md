Suite `solid`, size `m`; variants: `a2`, `a3`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | mesh | a2 | 5 | 1 | 575.0 µs [568.1 µs, 672.4 µs] | 575.0 µs [568.1 µs, 672.4 µs] | 34.9 [34.5, 40.8] | 0.969 [0.857, 1.118] | same |
| static_linear_eigen | mesh | a3 | 5 | 1 | 675.7 µs [588.9 µs, 687.6 µs] | 675.7 µs [588.9 µs, 687.6 µs] | 41.0 [35.8, 41.8] | 1.012 [0.917, 1.157] | same |
| static_linear_eigen | mesh | before | 5 | 1 | 600.0 µs [589.6 µs, 671.9 µs] | 600.0 µs [589.6 µs, 671.9 µs] | 36.4 [35.8, 40.8] | – | same |
| static_linear_eigen | init | a2 | 5 | 1 | 10.30 ms [10.23 ms, 11.71 ms] | 10.30 ms [10.23 ms, 11.71 ms] | 625.9 [621.3, 711.5] | 0.977 [0.964, 1.073] | same |
| static_linear_eigen | init | a3 | 5 | 1 | 10.78 ms [10.18 ms, 11.37 ms] | 10.78 ms [10.18 ms, 11.37 ms] | 654.9 [618.3, 690.4] | 1.010 [0.911, 1.072] | same |
| static_linear_eigen | init | before | 5 | 1 | 10.66 ms [10.54 ms, 11.17 ms] | 10.66 ms [10.54 ms, 11.17 ms] | 647.5 [640.4, 678.5] | – | same |
| static_linear_eigen | solve_step | a2 | 5 | 1 | 2.190 s [2.182 s, 2.223 s] | 2.190 s [2.182 s, 2.223 s] | 133037.1 [132536.9, 135010.4] | 0.951 [0.944, 0.955] | same |
| static_linear_eigen | solve_step | a3 | 5 | 1 | 2.196 s [2.160 s, 2.211 s] | 2.196 s [2.160 s, 2.211 s] | 133384.8 [131222.9, 134313.6] | 0.946 [0.937, 0.962] | same |
| static_linear_eigen | solve_step | before | 5 | 1 | 2.310 s [2.299 s, 2.327 s] | 2.310 s [2.299 s, 2.327 s] | 140326.4 [139659.7, 141366.0] | – | same |
| static_linear_eigen | predictor | a2 | 5 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.920 [0.490, 1.302] | same |
| static_linear_eigen | predictor | a3 | 5 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.807 [0.425, 1.055] | same |
| static_linear_eigen | predictor | before | 5 | 1 | 0.2 µs [0.2 µs, 0.4 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | matrix | a2 | 5 | 1 | 69.95 ms [67.36 ms, 72.77 ms] | 69.95 ms [67.36 ms, 72.77 ms] | 4248.8 [4091.5, 4420.2] | 0.571 [0.520, 0.599] | same |
| static_linear_eigen | matrix | a3 | 5 | 1 | 71.75 ms [68.20 ms, 72.71 ms] | 71.75 ms [68.20 ms, 72.71 ms] | 4358.0 [4142.4, 4416.1] | 0.576 [0.561, 0.586] | same |
| static_linear_eigen | matrix | before | 5 | 1 | 122.67 ms [121.49 ms, 129.50 ms] | 122.67 ms [121.49 ms, 129.50 ms] | 7451.0 [7379.2, 7865.4] | – | same |
| static_linear_eigen | corrector | a2 | 5 | 1 | 0.5 µs [0.4 µs, 0.7 µs] | 0.5 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.644 [0.486, 0.956] | same |
| static_linear_eigen | corrector | a3 | 5 | 1 | 0.6 µs [0.4 µs, 0.8 µs] | 0.6 µs [0.4 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 0.705 [0.510, 0.911] | same |
| static_linear_eigen | corrector | before | 5 | 1 | 0.8 µs [0.6 µs, 1.0 µs] | 0.8 µs [0.6 µs, 1.0 µs] | 0.1 [0.0, 0.1] | – | same |
| static_linear_eigen | step_hooks | a2 | 5 | 2 | 256.7 µs [248.8 µs, 261.9 µs] | 128.4 µs [124.4 µs, 130.9 µs] | 7.8 [7.6, 8.0] | 1.013 [1.009, 1.051] | same |
| static_linear_eigen | step_hooks | a3 | 5 | 2 | 248.8 µs [207.2 µs, 259.7 µs] | 124.4 µs [103.6 µs, 129.9 µs] | 7.6 [6.3, 7.9] | 1.012 [0.844, 1.025] | same |
| static_linear_eigen | step_hooks | before | 5 | 2 | 249.3 µs [245.5 µs, 255.1 µs] | 124.6 µs [122.7 µs, 127.6 µs] | 7.6 [7.5, 7.7] | – | same |
| static_linear_eigen | step_rest | a2 | 5 | 1 | 2.121 s [2.109 s, 2.155 s] | 2.121 s [2.109 s, 2.155 s] | 128800.2 [128101.5, 130902.9] | 0.972 [0.965, 0.981] | same |
| static_linear_eigen | step_rest | a3 | 5 | 1 | 2.124 s [2.092 s, 2.139 s] | 2.124 s [2.092 s, 2.139 s] | 129024.9 [127065.4, 129939.7] | 0.967 [0.958, 0.983] | same |
| static_linear_eigen | step_rest | before | 5 | 1 | 2.186 s [2.177 s, 2.198 s] | 2.186 s [2.177 s, 2.198 s] | 132799.1 [132204.1, 133485.4] | – | same |
| static_linear_eigen | stiffness_assembly | a2 | 5 | 3 | 72.47 ms [72.23 ms, 73.95 ms] | 24.16 ms [24.08 ms, 24.65 ms] | 1467.2 [1462.3, 1497.2] | 0.481 [0.464, 0.493] | same |
| static_linear_eigen | stiffness_assembly | a3 | 5 | 3 | 99.95 ms [98.30 ms, 106.04 ms] | 33.32 ms [32.77 ms, 35.35 ms] | 2023.5 [1990.2, 2146.8] | 0.661 [0.622, 0.707] | same |
| static_linear_eigen | stiffness_assembly | before | 5 | 3 | 150.31 ms [148.34 ms, 158.10 ms] | 50.10 ms [49.45 ms, 52.70 ms] | 3043.3 [3003.3, 3201.0] | – | same |
| static_linear_eigen | analysis | a2 | 5 | 1 | 39.01 ms [37.85 ms, 39.99 ms] | 39.01 ms [37.85 ms, 39.99 ms] | 2369.2 [2298.7, 2428.9] | 0.996 [0.893, 1.025] | same |
| static_linear_eigen | analysis | a3 | 5 | 1 | 42.20 ms [41.74 ms, 47.30 ms] | 42.20 ms [41.74 ms, 47.30 ms] | 2563.3 [2535.5, 2872.7] | 1.090 [0.993, 1.211] | same |
| static_linear_eigen | analysis | before | 5 | 1 | 39.17 ms [38.61 ms, 42.39 ms] | 39.17 ms [38.61 ms, 42.39 ms] | 2378.8 [2345.2, 2575.0] | – | same |
| static_linear_eigen | factorize | a2 | 5 | 2 | 4.122 s [4.095 s, 4.129 s] | 2.061 s [2.048 s, 2.065 s] | 125174.9 [124364.7, 125408.9] | 0.995 [0.981, 1.003] | same |
| static_linear_eigen | factorize | a3 | 5 | 2 | 4.138 s [4.117 s, 4.189 s] | 2.069 s [2.059 s, 2.094 s] | 125679.1 [125034.7, 127211.0] | 1.000 [0.996, 1.014] | same |
| static_linear_eigen | factorize | before | 5 | 2 | 4.144 s [4.119 s, 4.178 s] | 2.072 s [2.059 s, 2.089 s] | 125852.2 [125081.1, 126884.7] | – | same |
| static_linear_eigen | back_substitution | a2 | 5 | 20 | 141.30 ms [140.02 ms, 144.79 ms] | 7.06 ms [7.00 ms, 7.24 ms] | 429.1 [425.2, 439.7] | 1.058 [1.050, 1.112] | same |
| static_linear_eigen | back_substitution | a3 | 5 | 20 | 145.68 ms [137.62 ms, 148.51 ms] | 7.28 ms [6.88 ms, 7.43 ms] | 442.4 [417.9, 451.0] | 1.092 [1.053, 1.118] | same |
| static_linear_eigen | back_substitution | before | 5 | 20 | 132.68 ms [130.26 ms, 135.91 ms] | 6.63 ms [6.51 ms, 6.80 ms] | 402.9 [395.6, 412.8] | – | same |
| static_newton_plastic | mesh | a2 | 5 | 1 | 261.3 µs [225.0 µs, 289.4 µs] | 261.3 µs [225.0 µs, 289.4 µs] | 43.5 [37.5, 48.2] | 1.034 [0.804, 1.130] | same |
| static_newton_plastic | mesh | a3 | 5 | 1 | 247.2 µs [233.3 µs, 268.2 µs] | 247.2 µs [233.3 µs, 268.2 µs] | 41.2 [38.9, 44.7] | 0.964 [0.834, 1.160] | same |
| static_newton_plastic | mesh | before | 5 | 1 | 260.4 µs [231.0 µs, 289.4 µs] | 260.4 µs [231.0 µs, 289.4 µs] | 43.4 [38.5, 48.2] | – | same |
| static_newton_plastic | init | a2 | 5 | 1 | 4.89 ms [4.11 ms, 5.03 ms] | 4.89 ms [4.11 ms, 5.03 ms] | 815.1 [685.5, 837.6] | 0.963 [0.898, 1.171] | same |
| static_newton_plastic | init | a3 | 5 | 1 | 4.34 ms [4.21 ms, 4.92 ms] | 4.34 ms [4.21 ms, 4.92 ms] | 723.8 [700.9, 819.2] | 0.939 [0.788, 1.151] | same |
| static_newton_plastic | init | before | 5 | 1 | 4.62 ms [4.27 ms, 5.34 ms] | 4.62 ms [4.27 ms, 5.34 ms] | 770.6 [711.7, 889.2] | – | same |
| static_newton_plastic | solve_step | a2 | 5 | 12 | 2.613 s [2.591 s, 2.627 s] | 217.72 ms [215.92 ms, 218.96 ms] | 36285.9 [35986.3, 36492.9] | 1.027 [1.009, 1.037] | same |
| static_newton_plastic | solve_step | a3 | 5 | 12 | 2.609 s [2.590 s, 2.615 s] | 217.38 ms [215.82 ms, 217.95 ms] | 36229.3 [35970.5, 36325.4] | 1.024 [1.011, 1.033] | same |
| static_newton_plastic | solve_step | before | 5 | 12 | 2.539 s [2.528 s, 2.588 s] | 211.59 ms [210.71 ms, 215.67 ms] | 35265.6 [35117.8, 35945.3] | – | same |
| static_newton_plastic | predictor | a2 | 5 | 12 | 1.7 µs [1.7 µs, 1.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.909 [0.889, 1.066] | same |
| static_newton_plastic | predictor | a3 | 5 | 12 | 1.7 µs [1.6 µs, 1.8 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.952 [0.848, 1.061] | same |
| static_newton_plastic | predictor | before | 5 | 12 | 1.9 µs [1.7 µs, 1.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | residual | a2 | 5 | 961 | 1.006 s [996.00 ms, 1.011 s] | 1.05 ms [1.04 ms, 1.05 ms] | 174.4 [172.7, 175.4] | 0.996 [0.968, 1.005] | same |
| static_newton_plastic | residual | a3 | 5 | 961 | 1.006 s [999.94 ms, 1.010 s] | 1.05 ms [1.04 ms, 1.05 ms] | 174.5 [173.4, 175.2] | 1.001 [0.958, 1.008] | same |
| static_newton_plastic | residual | before | 5 | 961 | 1.006 s [1.003 s, 1.044 s] | 1.05 ms [1.04 ms, 1.09 ms] | 174.4 [173.9, 181.1] | – | same |
| static_newton_plastic | matrix | a2 | 5 | 949 | 565.9 µs [538.6 µs, 590.8 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 1.067 [1.046, 1.097] | same |
| static_newton_plastic | matrix | a3 | 5 | 949 | 533.7 µs [526.9 µs, 568.1 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 1.037 [0.993, 1.048] | same |
| static_newton_plastic | matrix | before | 5 | 949 | 527.3 µs [514.7 µs, 542.2 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.1 [0.1, 0.1] | – | same |
| static_newton_plastic | corrector | a2 | 5 | 949 | 176.4 µs [136.6 µs, 205.5 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.103 [0.690, 1.156] | same |
| static_newton_plastic | corrector | a3 | 5 | 949 | 140.8 µs [134.5 µs, 154.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.758 [0.680, 0.923] | same |
| static_newton_plastic | corrector | before | 5 | 949 | 186.3 µs [149.8 µs, 203.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | step_hooks | a2 | 5 | 24 | 3.08 ms [2.99 ms, 3.15 ms] | 128.4 µs [124.5 µs, 131.4 µs] | 21.4 [20.8, 21.9] | 1.108 [1.070, 1.128] | same |
| static_newton_plastic | step_hooks | a3 | 5 | 24 | 2.80 ms [2.72 ms, 2.85 ms] | 116.8 µs [113.4 µs, 118.9 µs] | 19.5 [18.9, 19.8] | 1.002 [0.973, 1.021] | same |
| static_newton_plastic | step_hooks | before | 5 | 24 | 2.79 ms [2.72 ms, 2.88 ms] | 116.4 µs [113.2 µs, 120.0 µs] | 19.4 [18.9, 20.0] | – | same |
| static_newton_plastic | step_rest | a2 | 5 | 12 | 1.598 s [1.586 s, 1.622 s] | 133.14 ms [132.16 ms, 135.16 ms] | 22190.4 [22027.4, 22526.7] | 1.047 [1.036, 1.057] | same |
| static_newton_plastic | step_rest | a3 | 5 | 12 | 1.598 s [1.581 s, 1.612 s] | 133.14 ms [131.76 ms, 134.33 ms] | 22190.6 [21959.3, 22388.7] | 1.046 [1.027, 1.050] | same |
| static_newton_plastic | step_rest | before | 5 | 12 | 1.531 s [1.522 s, 1.540 s] | 127.58 ms [126.86 ms, 128.37 ms] | 21262.6 [21142.6, 21395.4] | – | same |
| heat_static_tri6 | mesh | a2 | 5 | 1 | 44.54 ms [43.67 ms, 49.54 ms] | 44.54 ms [43.67 ms, 49.54 ms] | 1840.5 [1804.5, 2047.1] | 1.021 [0.976, 1.170] | same |
| heat_static_tri6 | mesh | a3 | 5 | 1 | 44.29 ms [43.96 ms, 45.31 ms] | 44.29 ms [43.96 ms, 45.31 ms] | 1830.0 [1816.4, 1872.1] | 1.008 [0.963, 1.059] | same |
| heat_static_tri6 | mesh | before | 5 | 1 | 44.02 ms [42.33 ms, 45.64 ms] | 44.02 ms [42.33 ms, 45.64 ms] | 1819.0 [1749.1, 1886.2] | – | same |
| heat_static_tri6 | init | a2 | 5 | 1 | 4.38 ms [4.37 ms, 4.51 ms] | 4.38 ms [4.37 ms, 4.51 ms] | 181.2 [180.7, 186.2] | 0.781 [0.757, 0.807] | same |
| heat_static_tri6 | init | a3 | 5 | 1 | 4.56 ms [4.40 ms, 5.06 ms] | 4.56 ms [4.40 ms, 5.06 ms] | 188.6 [181.9, 209.1] | 0.817 [0.761, 0.903] | same |
| heat_static_tri6 | init | before | 5 | 1 | 5.66 ms [5.59 ms, 5.79 ms] | 5.66 ms [5.59 ms, 5.79 ms] | 234.0 [230.9, 239.2] | – | same |
| heat_static_tri6 | assemble_conductivity | a2 | 5 | 5 | 70.36 ms [69.50 ms, 71.84 ms] | 14.07 ms [13.90 ms, 14.37 ms] | 581.5 [574.4, 593.7] | 0.492 [0.460, 0.507] | same |
| heat_static_tri6 | assemble_conductivity | a3 | 5 | 5 | 72.66 ms [70.91 ms, 75.87 ms] | 14.53 ms [14.18 ms, 15.17 ms] | 600.5 [586.0, 627.0] | 0.507 [0.471, 0.535] | same |
| heat_static_tri6 | assemble_conductivity | before | 5 | 5 | 141.74 ms [139.86 ms, 151.10 ms] | 28.35 ms [27.97 ms, 30.22 ms] | 1171.4 [1155.9, 1248.8] | – | same |
| heat_static_tri6 | solve_step | a2 | 5 | 1 | 3.842 s [3.795 s, 3.872 s] | 3.842 s [3.795 s, 3.872 s] | 158753.1 [156810.7, 159988.5] | 0.955 [0.952, 0.978] | same |
| heat_static_tri6 | solve_step | a3 | 5 | 1 | 4.021 s [4.002 s, 4.051 s] | 4.021 s [4.002 s, 4.051 s] | 166173.7 [165354.2, 167410.5] | 1.004 [0.992, 1.019] | same |
| heat_static_tri6 | solve_step | before | 5 | 1 | 3.995 s [3.960 s, 4.037 s] | 3.995 s [3.960 s, 4.037 s] | 165084.4 [163646.7, 166797.9] | – | same |
| structural_beam3_frame_static | mesh | a2 | 5 | 1 | 285.0 µs [245.0 µs, 292.0 µs] | 285.0 µs [245.0 µs, 292.0 µs] | 46.8 [40.3, 48.0] | 1.084 [0.984, 1.146] | same |
| structural_beam3_frame_static | mesh | a3 | 5 | 1 | 237.0 µs [220.0 µs, 276.0 µs] | 237.0 µs [220.0 µs, 276.0 µs] | 39.0 [36.2, 45.4] | 0.908 [0.837, 1.074] | same |
| structural_beam3_frame_static | mesh | before | 5 | 1 | 257.0 µs [249.0 µs, 263.0 µs] | 257.0 µs [249.0 µs, 263.0 µs] | 42.2 [40.9, 43.2] | – | same |
| structural_beam3_frame_static | init | a2 | 5 | 1 | 24.84 ms [23.40 ms, 26.29 ms] | 24.84 ms [23.40 ms, 26.29 ms] | 4083.7 [3846.3, 4320.8] | 1.007 [0.922, 1.116] | same |
| structural_beam3_frame_static | init | a3 | 5 | 1 | 23.50 ms [23.45 ms, 24.91 ms] | 23.50 ms [23.45 ms, 24.91 ms] | 3863.2 [3854.0, 4095.0] | 0.971 [0.898, 1.018] | same |
| structural_beam3_frame_static | init | before | 5 | 1 | 24.92 ms [23.22 ms, 26.12 ms] | 24.92 ms [23.22 ms, 26.12 ms] | 4096.6 [3816.9, 4293.1] | – | same |
| structural_beam3_frame_static | assemble_stiffness | a2 | 5 | 5 | 69.62 ms [68.38 ms, 71.69 ms] | 13.92 ms [13.68 ms, 14.34 ms] | 2288.6 [2247.9, 2356.8] | 0.519 [0.506, 0.538] | same |
| structural_beam3_frame_static | assemble_stiffness | a3 | 5 | 5 | 70.36 ms [67.77 ms, 71.92 ms] | 14.07 ms [13.55 ms, 14.38 ms] | 2313.1 [2227.6, 2364.2] | 0.528 [0.499, 0.540] | same |
| structural_beam3_frame_static | assemble_stiffness | before | 5 | 5 | 133.23 ms [131.68 ms, 137.89 ms] | 26.65 ms [26.34 ms, 27.58 ms] | 4379.7 [4328.6, 4532.8] | – | same |
| structural_beam3_frame_static | solve_step | a2 | 5 | 1 | 4.468 s [4.433 s, 4.483 s] | 4.468 s [4.433 s, 4.483 s] | 734423.4 [728613.6, 736875.6] | 0.989 [0.981, 0.999] | same |
| structural_beam3_frame_static | solve_step | a3 | 5 | 1 | 4.602 s [4.548 s, 4.623 s] | 4.602 s [4.548 s, 4.623 s] | 756403.2 [747460.7, 759832.7] | 1.022 [1.011, 1.025] | same |
| structural_beam3_frame_static | solve_step | before | 5 | 1 | 4.501 s [4.487 s, 4.543 s] | 4.501 s [4.487 s, 4.543 s] | 739792.7 [737443.5, 746738.2] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 0223e03ad, 59e05aa3f, 9d5f4476b | 35e60e6f09e111e7 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 0223e03ad, 59e05aa3f, 9d5f4476b | 7bd5f114957d7d7b |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 9d5f4476b | 463949e032392419 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 9d5f4476b | ba3b7d6fb6b7bbbc |
