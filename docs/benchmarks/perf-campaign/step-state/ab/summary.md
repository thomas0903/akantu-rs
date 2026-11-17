Suite `solid`, size `m`; variants: `after`, `base`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_bilinear | init | after | 5 | 1 | 1.202 s [1.184 s, 1.216 s] | 1.202 s [1.184 s, 1.216 s] | 25034.1 [24667.5, 25329.5] | 0.988 [0.982, 1.012] | same |
| cohesive_intrinsic_bilinear | init | base | 5 | 1 | 1.206 s [1.202 s, 1.221 s] | 1.206 s [1.202 s, 1.221 s] | 25130.9 [25034.6, 25446.8] | – | same |
| cohesive_intrinsic_bilinear | solve_step | after | 5 | 300 | 5.160 s [5.126 s, 5.183 s] | 17.20 ms [17.09 ms, 17.28 ms] | 358.3 [355.9, 359.9] | 0.962 [0.953, 0.966] | same |
| cohesive_intrinsic_bilinear | solve_step | base | 5 | 300 | 5.365 s [5.327 s, 5.440 s] | 17.88 ms [17.76 ms, 18.13 ms] | 372.6 [369.9, 377.8] | – | same |
| cohesive_intrinsic_bilinear | energies | after | 5 | 60 | 206.26 ms [206.19 ms, 206.86 ms] | 3.44 ms [3.44 ms, 3.45 ms] | 71.6 [71.6, 71.8] | 0.918 [0.906, 0.918] | same |
| cohesive_intrinsic_bilinear | energies | base | 5 | 60 | 225.24 ms [224.57 ms, 227.48 ms] | 3.75 ms [3.74 ms, 3.79 ms] | 78.2 [78.0, 79.0] | – | same |
| cohesive_intrinsic_exponential | init | after | 5 | 1 | 1.181 s [1.176 s, 1.219 s] | 1.181 s [1.176 s, 1.219 s] | 24613.1 [24510.0, 25401.1] | 0.989 [0.970, 1.003] | same |
| cohesive_intrinsic_exponential | init | base | 5 | 1 | 1.201 s [1.190 s, 1.218 s] | 1.201 s [1.190 s, 1.218 s] | 25015.5 [24786.4, 25384.6] | – | same |
| cohesive_intrinsic_exponential | solve_step | after | 5 | 300 | 7.271 s [7.206 s, 7.328 s] | 24.24 ms [24.02 ms, 24.43 ms] | 504.9 [500.4, 508.9] | 0.973 [0.967, 0.985] | same |
| cohesive_intrinsic_exponential | solve_step | base | 5 | 300 | 7.458 s [7.420 s, 7.520 s] | 24.86 ms [24.73 ms, 25.07 ms] | 517.9 [515.3, 522.2] | – | same |
| cohesive_intrinsic_exponential | energies | after | 5 | 60 | 206.54 ms [205.60 ms, 209.26 ms] | 3.44 ms [3.43 ms, 3.49 ms] | 71.7 [71.4, 72.7] | 0.913 [0.912, 0.922] | same |
| cohesive_intrinsic_exponential | energies | base | 5 | 60 | 226.32 ms [225.18 ms, 227.03 ms] | 3.77 ms [3.75 ms, 3.78 ms] | 78.6 [78.2, 78.8] | – | same |
| explicit_hex8_fd | init | after | 5 | 1 | 720.20 ms [715.44 ms, 730.27 ms] | 720.20 ms [715.44 ms, 730.27 ms] | 6512.2 [6469.1, 6603.3] | 1.001 [0.981, 1.015] | same |
| explicit_hex8_fd | init | base | 5 | 1 | 720.34 ms [718.49 ms, 732.37 ms] | 720.34 ms [718.49 ms, 732.37 ms] | 6513.5 [6496.8, 6622.2] | – | same |
| explicit_hex8_fd | solve_step | after | 5 | 30 | 3.795 s [3.779 s, 3.824 s] | 126.51 ms [125.98 ms, 127.46 ms] | 1143.9 [1139.1, 1152.6] | 0.895 [0.871, 0.902] | same |
| explicit_hex8_fd | solve_step | base | 5 | 30 | 4.243 s [4.202 s, 4.338 s] | 141.44 ms [140.07 ms, 144.60 ms] | 1279.0 [1266.5, 1307.5] | – | same |
| explicit_hex8_fd | residual | after | 5 | 30 | 3.677 s [3.659 s, 3.703 s] | 122.56 ms [121.96 ms, 123.45 ms] | 1108.2 [1102.8, 1116.2] | 0.994 [0.984, 1.003] | same |
| explicit_hex8_fd | residual | base | 5 | 30 | 3.700 s [3.661 s, 3.723 s] | 123.34 ms [122.05 ms, 124.10 ms] | 1115.3 [1103.6, 1122.1] | – | same |
| explicit_hex8_fd | step_hooks | after | 5 | 60 | 184.3 µs [173.9 µs, 204.8 µs] | 3.1 µs [2.9 µs, 3.4 µs] | 0.0 [0.0, 0.0] | 0.000 [0.000, 0.000] | same |
| explicit_hex8_fd | step_hooks | base | 5 | 60 | 413.24 ms [411.03 ms, 484.85 ms] | 6.89 ms [6.85 ms, 8.08 ms] | 62.3 [61.9, 73.1] | – | same |
| explicit_hex8_fd | energies | after | 5 | 6 | 104.28 ms [103.43 ms, 104.57 ms] | 17.38 ms [17.24 ms, 17.43 ms] | 157.1 [155.9, 157.6] | 0.170 [0.160, 0.171] | same |
| explicit_hex8_fd | energies | base | 5 | 6 | 615.14 ms [612.55 ms, 645.54 ms] | 102.52 ms [102.09 ms, 107.59 ms] | 927.0 [923.1, 972.9] | – | same |
| explicit_hex8_ss | init | after | 5 | 1 | 612.46 ms [610.73 ms, 632.77 ms] | 612.46 ms [610.73 ms, 632.77 ms] | 5538.0 [5522.4, 5721.7] | 0.995 [0.967, 1.021] | same |
| explicit_hex8_ss | init | base | 5 | 1 | 621.91 ms [615.69 ms, 632.48 ms] | 621.91 ms [615.69 ms, 632.48 ms] | 5623.5 [5567.3, 5719.0] | – | same |
| explicit_hex8_ss | solve_step | after | 5 | 50 | 3.415 s [3.340 s, 3.511 s] | 68.30 ms [66.79 ms, 70.22 ms] | 617.6 [604.0, 635.0] | 0.883 [0.868, 0.887] | same |
| explicit_hex8_ss | solve_step | base | 5 | 50 | 3.888 s [3.833 s, 3.961 s] | 77.76 ms [76.67 ms, 79.22 ms] | 703.1 [693.3, 716.3] | – | same |
| explicit_hex8_ss | residual | after | 5 | 50 | 3.210 s [3.148 s, 3.303 s] | 64.19 ms [62.96 ms, 66.06 ms] | 580.5 [569.3, 597.3] | 1.009 [1.003, 1.014] | same |
| explicit_hex8_ss | residual | base | 5 | 50 | 3.190 s [3.133 s, 3.272 s] | 63.81 ms [62.67 ms, 65.45 ms] | 577.0 [566.7, 591.8] | – | same |
| explicit_hex8_ss | step_hooks | after | 5 | 100 | 258.6 µs [234.3 µs, 321.6 µs] | 2.6 µs [2.3 µs, 3.2 µs] | 0.0 [0.0, 0.0] | 0.001 [0.001, 0.001] | same |
| explicit_hex8_ss | step_hooks | base | 5 | 100 | 476.00 ms [467.02 ms, 507.72 ms] | 4.76 ms [4.67 ms, 5.08 ms] | 43.0 [42.2, 45.9] | – | same |
| explicit_hex8_ss | energies | after | 5 | 10 | 108.74 ms [107.62 ms, 112.74 ms] | 10.87 ms [10.76 ms, 11.27 ms] | 98.3 [97.3, 101.9] | 0.111 [0.109, 0.115] | same |
| explicit_hex8_ss | energies | base | 5 | 10 | 982.96 ms [981.69 ms, 988.19 ms] | 98.30 ms [98.17 ms, 98.82 ms] | 888.8 [887.7, 893.5] | – | same |
| explicit_marigo | init | after | 5 | 1 | 77.26 ms [75.71 ms, 79.86 ms] | 77.26 ms [75.71 ms, 79.86 ms] | 732.7 [717.9, 757.3] | 1.003 [0.989, 1.023] | same |
| explicit_marigo | init | base | 5 | 1 | 76.93 ms [76.54 ms, 78.10 ms] | 76.93 ms [76.54 ms, 78.10 ms] | 729.5 [725.8, 740.6] | – | same |
| explicit_marigo | solve_step | after | 5 | 200 | 3.120 s [3.102 s, 3.168 s] | 15.60 ms [15.51 ms, 15.84 ms] | 147.9 [147.1, 150.2] | 0.811 [0.807, 0.818] | same |
| explicit_marigo | solve_step | base | 5 | 200 | 3.859 s [3.802 s, 3.906 s] | 19.30 ms [19.01 ms, 19.53 ms] | 183.0 [180.3, 185.2] | – | same |
| explicit_marigo | residual | after | 5 | 200 | 2.222 s [2.214 s, 2.262 s] | 11.11 ms [11.07 ms, 11.31 ms] | 105.4 [105.0, 107.2] | 0.842 [0.837, 0.843] | same |
| explicit_marigo | residual | base | 5 | 200 | 2.648 s [2.630 s, 2.702 s] | 13.24 ms [13.15 ms, 13.51 ms] | 125.5 [124.7, 128.1] | – | same |
| explicit_marigo | step_hooks | after | 5 | 400 | 743.32 ms [740.49 ms, 755.18 ms] | 1.86 ms [1.85 ms, 1.89 ms] | 17.6 [17.6, 17.9] | 0.724 [0.703, 0.741] | same |
| explicit_marigo | step_hooks | base | 5 | 400 | 1.032 s [1.003 s, 1.056 s] | 2.58 ms [2.51 ms, 2.64 ms] | 24.5 [23.8, 25.0] | – | same |
| explicit_marigo | energies | after | 5 | 40 | 112.98 ms [112.32 ms, 113.71 ms] | 2.82 ms [2.81 ms, 2.84 ms] | 26.8 [26.6, 27.0] | 0.992 [0.951, 1.004] | same |
| explicit_marigo | energies | base | 5 | 40 | 114.09 ms [113.24 ms, 118.74 ms] | 2.85 ms [2.83 ms, 2.97 ms] | 27.0 [26.8, 28.1] | – | same |
| explicit_maxwell | init | after | 5 | 1 | 89.96 ms [89.75 ms, 90.62 ms] | 89.96 ms [89.75 ms, 90.62 ms] | 853.0 [851.1, 859.3] | 0.998 [0.994, 1.005] | same |
| explicit_maxwell | init | base | 5 | 1 | 90.30 ms [89.54 ms, 90.61 ms] | 90.30 ms [89.54 ms, 90.61 ms] | 856.3 [849.1, 859.2] | – | same |
| explicit_maxwell | solve_step | after | 5 | 40 | 4.177 s [4.162 s, 4.263 s] | 104.43 ms [104.05 ms, 106.58 ms] | 990.2 [986.7, 1010.7] | 0.615 [0.612, 0.624] | same |
| explicit_maxwell | solve_step | base | 5 | 40 | 6.809 s [6.793 s, 6.861 s] | 170.21 ms [169.83 ms, 171.53 ms] | 1614.1 [1610.4, 1626.6] | – | same |
| explicit_maxwell | residual | after | 5 | 40 | 1.479 s [1.459 s, 1.515 s] | 36.98 ms [36.46 ms, 37.87 ms] | 350.6 [345.8, 359.1] | 0.910 [0.899, 0.925] | same |
| explicit_maxwell | residual | base | 5 | 40 | 1.628 s [1.622 s, 1.659 s] | 40.69 ms [40.55 ms, 41.48 ms] | 385.9 [384.5, 393.4] | – | same |
| explicit_maxwell | step_hooks | after | 5 | 80 | 2.670 s [2.665 s, 2.717 s] | 33.38 ms [33.31 ms, 33.97 ms] | 316.5 [315.9, 322.1] | 0.519 [0.519, 0.528] | same |
| explicit_maxwell | step_hooks | base | 5 | 80 | 5.142 s [5.134 s, 5.188 s] | 64.27 ms [64.17 ms, 64.85 ms] | 609.5 [608.5, 615.0] | – | same |
| explicit_maxwell | energies | after | 5 | 8 | 29.97 ms [29.95 ms, 30.10 ms] | 3.75 ms [3.74 ms, 3.76 ms] | 35.5 [35.5, 35.7] | 1.005 [1.001, 1.019] | same |
| explicit_maxwell | energies | base | 5 | 8 | 29.82 ms [29.55 ms, 29.95 ms] | 3.73 ms [3.69 ms, 3.74 ms] | 35.3 [35.0, 35.5] | – | same |
| explicit_mazars | init | after | 5 | 1 | 77.92 ms [76.78 ms, 80.30 ms] | 77.92 ms [76.78 ms, 80.30 ms] | 738.9 [728.0, 761.4] | 0.993 [0.980, 1.045] | same |
| explicit_mazars | init | base | 5 | 1 | 77.50 ms [76.84 ms, 78.50 ms] | 77.50 ms [76.84 ms, 78.50 ms] | 734.9 [728.6, 744.4] | – | same |
| explicit_mazars | solve_step | after | 5 | 80 | 7.789 s [7.729 s, 7.907 s] | 97.36 ms [96.61 ms, 98.83 ms] | 923.2 [916.1, 937.2] | 1.016 [0.911, 1.025] | same |
| explicit_mazars | solve_step | base | 5 | 80 | 7.711 s [7.608 s, 8.552 s] | 96.38 ms [95.10 ms, 106.91 ms] | 914.0 [901.8, 1013.7] | – | same |
| explicit_mazars | residual | after | 5 | 80 | 7.420 s [7.360 s, 7.532 s] | 92.75 ms [92.00 ms, 94.15 ms] | 879.5 [872.4, 892.8] | 1.030 [0.919, 1.041] | same |
| explicit_mazars | residual | base | 5 | 80 | 7.233 s [7.145 s, 8.075 s] | 90.41 ms [89.31 ms, 100.94 ms] | 857.3 [846.9, 957.2] | – | same |
| explicit_mazars | step_hooks | after | 5 | 160 | 302.95 ms [301.52 ms, 307.82 ms] | 1.89 ms [1.88 ms, 1.92 ms] | 18.0 [17.9, 18.2] | 0.746 [0.744, 0.763] | same |
| explicit_mazars | step_hooks | base | 5 | 160 | 406.09 ms [395.20 ms, 408.71 ms] | 2.54 ms [2.47 ms, 2.55 ms] | 24.1 [23.4, 24.2] | – | same |
| explicit_mazars | energies | after | 5 | 16 | 33.35 ms [32.68 ms, 34.67 ms] | 2.08 ms [2.04 ms, 2.17 ms] | 19.8 [19.4, 20.5] | 0.989 [0.967, 1.035] | same |
| explicit_mazars | energies | base | 5 | 16 | 33.63 ms [33.51 ms, 34.42 ms] | 2.10 ms [2.09 ms, 2.15 ms] | 19.9 [19.9, 20.4] | – | same |
| explicit_mazars_non_local | init | after | 5 | 1 | 1.106 s [1.084 s, 1.135 s] | 1.106 s [1.084 s, 1.135 s] | 23050.1 [22582.8, 23640.2] | 1.005 [0.943, 1.038] | same |
| explicit_mazars_non_local | init | base | 5 | 1 | 1.099 s [1.093 s, 1.152 s] | 1.099 s [1.093 s, 1.152 s] | 22896.9 [22775.6, 24004.2] | – | same |
| explicit_mazars_non_local | solve_step | after | 5 | 40 | 4.063 s [4.016 s, 4.090 s] | 101.57 ms [100.40 ms, 102.25 ms] | 2116.1 [2091.7, 2130.1] | 0.448 [0.442, 0.456] | same |
| explicit_mazars_non_local | solve_step | base | 5 | 40 | 9.047 s [8.944 s, 9.085 s] | 226.18 ms [223.60 ms, 227.13 ms] | 4712.1 [4658.4, 4731.9] | – | same |
| explicit_mazars_non_local | residual | after | 5 | 40 | 3.982 s [3.935 s, 4.008 s] | 99.55 ms [98.37 ms, 100.21 ms] | 2074.0 [2049.5, 2087.8] | 0.443 [0.438, 0.452] | same |
| explicit_mazars_non_local | residual | base | 5 | 40 | 8.950 s [8.848 s, 8.988 s] | 223.74 ms [221.20 ms, 224.70 ms] | 4661.3 [4608.3, 4681.2] | – | same |
| explicit_mazars_non_local | step_hooks | after | 5 | 80 | 67.85 ms [67.34 ms, 68.39 ms] | 848.1 µs [841.8 µs, 854.9 µs] | 17.7 [17.5, 17.8] | 0.815 [0.806, 0.832] | same |
| explicit_mazars_non_local | step_hooks | base | 5 | 80 | 83.05 ms [82.24 ms, 83.73 ms] | 1.04 ms [1.03 ms, 1.05 ms] | 21.6 [21.4, 21.8] | – | same |
| explicit_mazars_non_local | energies | after | 5 | 8 | 16.82 ms [16.71 ms, 16.90 ms] | 2.10 ms [2.09 ms, 2.11 ms] | 43.8 [43.5, 44.0] | 1.003 [0.962, 1.012] | same |
| explicit_mazars_non_local | energies | base | 5 | 8 | 16.77 ms [16.64 ms, 17.52 ms] | 2.10 ms [2.08 ms, 2.19 ms] | 43.7 [43.3, 45.6] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | after | 5 | 2 | 2.067 s [2.054 s, 2.077 s] | 1.033 s [1.027 s, 1.038 s] | 21527.7 [21400.3, 21634.5] | 0.994 [0.990, 1.010] | same |
| explicit_mazars_non_local | nonlocal_rebuild | base | 5 | 2 | 2.070 s [2.056 s, 2.088 s] | 1.035 s [1.028 s, 1.044 s] | 21562.9 [21413.4, 21751.9] | – | same |
| explicit_multi_material | init | after | 5 | 1 | 79.95 ms [78.69 ms, 88.78 ms] | 79.95 ms [78.69 ms, 88.78 ms] | 758.1 [746.2, 841.8] | 0.998 [0.974, 1.085] | same |
| explicit_multi_material | init | base | 5 | 1 | 80.12 ms [79.93 ms, 81.84 ms] | 80.12 ms [79.93 ms, 81.84 ms] | 759.7 [758.0, 776.0] | – | same |
| explicit_multi_material | solve_step | after | 5 | 120 | 5.254 s [5.224 s, 5.360 s] | 43.78 ms [43.53 ms, 44.67 ms] | 415.2 [412.8, 423.6] | 0.927 [0.890, 0.945] | same |
| explicit_multi_material | solve_step | base | 5 | 120 | 5.671 s [5.642 s, 5.904 s] | 47.25 ms [47.01 ms, 49.20 ms] | 448.1 [445.8, 466.5] | – | same |
| explicit_multi_material | residual | after | 5 | 120 | 4.955 s [4.930 s, 5.061 s] | 41.29 ms [41.08 ms, 42.18 ms] | 391.5 [389.5, 399.9] | 0.963 [0.922, 0.984] | same |
| explicit_multi_material | residual | base | 5 | 120 | 5.144 s [5.112 s, 5.376 s] | 42.87 ms [42.60 ms, 44.80 ms] | 406.5 [403.9, 424.8] | – | same |
| explicit_multi_material | step_hooks | after | 5 | 240 | 203.23 ms [201.04 ms, 203.79 ms] | 846.8 µs [837.7 µs, 849.1 µs] | 8.0 [7.9, 8.1] | 0.476 [0.468, 0.477] | same |
| explicit_multi_material | step_hooks | base | 5 | 240 | 426.98 ms [426.79 ms, 429.92 ms] | 1.78 ms [1.78 ms, 1.79 ms] | 16.9 [16.9, 17.0] | – | same |
| explicit_multi_material | energies | after | 5 | 24 | 94.34 ms [94.17 ms, 95.65 ms] | 3.93 ms [3.92 ms, 3.99 ms] | 37.3 [37.2, 37.8] | 0.940 [0.930, 0.953] | same |
| explicit_multi_material | energies | base | 5 | 24 | 100.40 ms [99.40 ms, 101.50 ms] | 4.18 ms [4.14 ms, 4.23 ms] | 39.7 [39.3, 40.1] | – | same |
| explicit_neohookean | init | after | 5 | 1 | 83.02 ms [82.39 ms, 83.98 ms] | 83.02 ms [82.39 ms, 83.98 ms] | 787.3 [781.3, 796.3] | 0.989 [0.970, 0.998] | same |
| explicit_neohookean | init | base | 5 | 1 | 84.15 ms [83.30 ms, 85.20 ms] | 84.15 ms [83.30 ms, 85.20 ms] | 798.0 [789.9, 807.9] | – | same |
| explicit_neohookean | solve_step | after | 5 | 100 | 2.172 s [2.135 s, 2.321 s] | 21.72 ms [21.35 ms, 23.21 ms] | 206.0 [202.4, 220.1] | 0.885 [0.878, 0.963] | same |
| explicit_neohookean | solve_step | base | 5 | 100 | 2.413 s [2.407 s, 2.473 s] | 24.13 ms [24.07 ms, 24.73 ms] | 228.8 [228.3, 234.5] | – | same |
| explicit_neohookean | residual | after | 5 | 100 | 2.098 s [2.061 s, 2.245 s] | 20.98 ms [20.61 ms, 22.45 ms] | 198.9 [195.4, 212.9] | 0.974 [0.965, 1.060] | same |
| explicit_neohookean | residual | base | 5 | 100 | 2.118 s [2.113 s, 2.174 s] | 21.18 ms [21.13 ms, 21.74 ms] | 200.8 [200.4, 206.1] | – | same |
| explicit_neohookean | step_hooks | after | 5 | 200 | 386.0 µs [300.6 µs, 432.0 µs] | 1.9 µs [1.5 µs, 2.2 µs] | 0.0 [0.0, 0.0] | 0.002 [0.001, 0.002] | same |
| explicit_neohookean | step_hooks | base | 5 | 200 | 219.80 ms [216.87 ms, 225.94 ms] | 1.10 ms [1.08 ms, 1.13 ms] | 10.4 [10.3, 10.7] | – | same |
| explicit_neohookean | energies | after | 5 | 20 | 266.23 ms [263.86 ms, 269.65 ms] | 13.31 ms [13.19 ms, 13.48 ms] | 126.2 [125.1, 127.9] | 1.003 [0.984, 1.014] | same |
| explicit_neohookean | energies | base | 5 | 20 | 266.99 ms [263.16 ms, 269.75 ms] | 13.35 ms [13.16 ms, 13.49 ms] | 126.6 [124.8, 127.9] | – | same |
| explicit_orthotropic | init | after | 5 | 1 | 70.50 ms [69.91 ms, 70.71 ms] | 70.50 ms [69.91 ms, 70.71 ms] | 668.5 [662.9, 670.5] | 0.983 [0.975, 0.988] | same |
| explicit_orthotropic | init | base | 5 | 1 | 71.69 ms [71.11 ms, 72.06 ms] | 71.69 ms [71.11 ms, 72.06 ms] | 679.8 [674.3, 683.3] | – | same |
| explicit_orthotropic | solve_step | after | 5 | 200 | 2.802 s [2.785 s, 2.809 s] | 14.01 ms [13.93 ms, 14.04 ms] | 132.9 [132.1, 133.2] | 0.874 [0.874, 0.878] | same |
| explicit_orthotropic | solve_step | base | 5 | 200 | 3.200 s [3.171 s, 3.213 s] | 16.00 ms [15.86 ms, 16.07 ms] | 151.7 [150.4, 152.4] | – | same |
| explicit_orthotropic | residual | after | 5 | 200 | 2.656 s [2.641 s, 2.664 s] | 13.28 ms [13.21 ms, 13.32 ms] | 125.9 [125.2, 126.3] | 0.961 [0.960, 0.965] | same |
| explicit_orthotropic | residual | base | 5 | 200 | 2.757 s [2.741 s, 2.771 s] | 13.79 ms [13.70 ms, 13.85 ms] | 130.7 [129.9, 131.4] | – | same |
| explicit_orthotropic | step_hooks | after | 5 | 400 | 459.1 µs [412.7 µs, 540.6 µs] | 1.1 µs [1.0 µs, 1.4 µs] | 0.0 [0.0, 0.0] | 0.002 [0.001, 0.002] | same |
| explicit_orthotropic | step_hooks | base | 5 | 400 | 286.79 ms [279.90 ms, 289.75 ms] | 717.0 µs [699.7 µs, 724.4 µs] | 6.8 [6.6, 6.9] | – | same |
| explicit_orthotropic | energies | after | 5 | 40 | 163.22 ms [161.88 ms, 165.66 ms] | 4.08 ms [4.05 ms, 4.14 ms] | 38.7 [38.4, 39.3] | 0.849 [0.843, 0.868] | same |
| explicit_orthotropic | energies | base | 5 | 40 | 191.28 ms [190.82 ms, 193.57 ms] | 4.78 ms [4.77 ms, 4.84 ms] | 45.3 [45.2, 45.9] | – | same |
| explicit_plastic | init | after | 5 | 1 | 82.43 ms [81.73 ms, 82.84 ms] | 82.43 ms [81.73 ms, 82.84 ms] | 781.6 [775.1, 785.6] | 0.994 [0.987, 0.999] | same |
| explicit_plastic | init | base | 5 | 1 | 82.93 ms [82.48 ms, 83.28 ms] | 82.93 ms [82.48 ms, 83.28 ms] | 786.4 [782.1, 789.7] | – | same |
| explicit_plastic | solve_step | after | 5 | 120 | 2.157 s [2.145 s, 2.163 s] | 17.97 ms [17.87 ms, 18.02 ms] | 170.4 [169.5, 170.9] | 0.349 [0.347, 0.353] | same |
| explicit_plastic | solve_step | base | 5 | 120 | 6.181 s [6.104 s, 6.194 s] | 51.51 ms [50.87 ms, 51.62 ms] | 488.4 [482.4, 489.5] | – | same |
| explicit_plastic | residual | after | 5 | 120 | 1.934 s [1.922 s, 1.937 s] | 16.12 ms [16.02 ms, 16.14 ms] | 152.8 [151.9, 153.0] | 0.343 [0.341, 0.346] | same |
| explicit_plastic | residual | base | 5 | 120 | 5.640 s [5.575 s, 5.653 s] | 47.00 ms [46.46 ms, 47.11 ms] | 445.7 [440.5, 446.7] | – | same |
| explicit_plastic | step_hooks | after | 5 | 240 | 135.98 ms [133.80 ms, 138.43 ms] | 566.6 µs [557.5 µs, 576.8 µs] | 5.4 [5.3, 5.5] | 0.311 [0.304, 0.318] | same |
| explicit_plastic | step_hooks | base | 5 | 240 | 437.03 ms [429.21 ms, 445.85 ms] | 1.82 ms [1.79 ms, 1.86 ms] | 17.3 [17.0, 17.6] | – | same |
| explicit_plastic | energies | after | 5 | 24 | 129.54 ms [128.95 ms, 130.13 ms] | 5.40 ms [5.37 ms, 5.42 ms] | 51.2 [50.9, 51.4] | 1.005 [1.000, 1.014] | same |
| explicit_plastic | energies | base | 5 | 24 | 128.67 ms [128.32 ms, 129.48 ms] | 5.36 ms [5.35 ms, 5.39 ms] | 50.8 [50.7, 51.2] | – | same |
| explicit_sls | init | after | 5 | 1 | 90.18 ms [89.68 ms, 91.57 ms] | 90.18 ms [89.68 ms, 91.57 ms] | 855.2 [850.4, 868.4] | 0.982 [0.965, 1.024] | same |
| explicit_sls | init | base | 5 | 1 | 91.41 ms [88.93 ms, 94.89 ms] | 91.41 ms [88.93 ms, 94.89 ms] | 866.8 [843.3, 899.9] | – | same |
| explicit_sls | solve_step | after | 5 | 120 | 1.693 s [1.672 s, 1.704 s] | 14.11 ms [13.93 ms, 14.20 ms] | 133.8 [132.1, 134.6] | 0.297 [0.294, 0.297] | same |
| explicit_sls | solve_step | base | 5 | 120 | 5.705 s [5.651 s, 5.729 s] | 47.54 ms [47.09 ms, 47.74 ms] | 450.8 [446.5, 452.7] | – | same |
| explicit_sls | residual | after | 5 | 120 | 1.604 s [1.585 s, 1.612 s] | 13.37 ms [13.21 ms, 13.43 ms] | 126.8 [125.2, 127.4] | 0.310 [0.308, 0.312] | same |
| explicit_sls | residual | base | 5 | 120 | 5.151 s [5.119 s, 5.183 s] | 42.92 ms [42.66 ms, 43.19 ms] | 407.0 [404.5, 409.6] | – | same |
| explicit_sls | step_hooks | after | 5 | 240 | 529.3 µs [479.3 µs, 576.5 µs] | 2.2 µs [2.0 µs, 2.4 µs] | 0.0 [0.0, 0.0] | 0.001 [0.001, 0.001] | same |
| explicit_sls | step_hooks | base | 5 | 240 | 443.61 ms [428.31 ms, 454.88 ms] | 1.85 ms [1.78 ms, 1.90 ms] | 17.5 [16.9, 18.0] | – | same |
| explicit_sls | energies | after | 5 | 24 | 152.23 ms [152.16 ms, 153.56 ms] | 6.34 ms [6.34 ms, 6.40 ms] | 60.1 [60.1, 60.7] | 0.994 [0.976, 1.008] | same |
| explicit_sls | energies | base | 5 | 24 | 153.10 ms [152.35 ms, 155.87 ms] | 6.38 ms [6.35 ms, 6.49 ms] | 60.5 [60.2, 61.6] | – | same |
| explicit_tet10_fd | init | after | 5 | 1 | 428.83 ms [422.79 ms, 433.45 ms] | 428.83 ms [422.79 ms, 433.45 ms] | 4066.5 [4009.2, 4110.3] | 0.984 [0.971, 1.006] | same |
| explicit_tet10_fd | init | base | 5 | 1 | 431.96 ms [426.74 ms, 440.41 ms] | 431.96 ms [426.74 ms, 440.41 ms] | 4096.1 [4046.6, 4176.3] | – | same |
| explicit_tet10_fd | solve_step | after | 5 | 40 | 3.278 s [3.244 s, 3.341 s] | 81.94 ms [81.11 ms, 83.52 ms] | 777.1 [769.1, 792.0] | 0.918 [0.914, 0.941] | same |
| explicit_tet10_fd | solve_step | base | 5 | 40 | 3.548 s [3.538 s, 3.588 s] | 88.71 ms [88.44 ms, 89.70 ms] | 841.2 [838.6, 850.6] | – | same |
| explicit_tet10_fd | residual | after | 5 | 40 | 3.062 s [3.032 s, 3.125 s] | 76.56 ms [75.80 ms, 78.13 ms] | 726.0 [718.8, 740.9] | 0.995 [0.990, 1.019] | same |
| explicit_tet10_fd | residual | base | 5 | 40 | 3.067 s [3.042 s, 3.102 s] | 76.66 ms [76.06 ms, 77.55 ms] | 727.0 [721.2, 735.4] | – | same |
| explicit_tet10_fd | step_hooks | after | 5 | 80 | 286.0 µs [218.8 µs, 303.7 µs] | 3.6 µs [2.7 µs, 3.8 µs] | 0.0 [0.0, 0.0] | 0.001 [0.001, 0.001] | same |
| explicit_tet10_fd | step_hooks | base | 5 | 80 | 269.20 ms [264.92 ms, 274.52 ms] | 3.37 ms [3.31 ms, 3.43 ms] | 31.9 [31.4, 32.5] | – | same |
| explicit_tet10_fd | energies | after | 5 | 8 | 83.94 ms [83.70 ms, 84.23 ms] | 10.49 ms [10.46 ms, 10.53 ms] | 99.5 [99.2, 99.8] | 0.180 [0.177, 0.183] | same |
| explicit_tet10_fd | energies | base | 5 | 8 | 467.38 ms [459.59 ms, 473.45 ms] | 58.42 ms [57.45 ms, 59.18 ms] | 554.0 [544.8, 561.2] | – | same |
| explicit_tet10_ss | init | after | 5 | 1 | 383.40 ms [372.84 ms, 405.29 ms] | 383.40 ms [372.84 ms, 405.29 ms] | 3635.6 [3535.5, 3843.2] | 0.975 [0.914, 1.054] | same |
| explicit_tet10_ss | init | base | 5 | 1 | 389.61 ms [382.08 ms, 407.78 ms] | 389.61 ms [382.08 ms, 407.78 ms] | 3694.5 [3623.1, 3866.9] | – | same |
| explicit_tet10_ss | solve_step | after | 5 | 50 | 2.343 s [2.283 s, 2.390 s] | 46.86 ms [45.66 ms, 47.80 ms] | 444.4 [433.0, 453.3] | 0.901 [0.889, 0.912] | same |
| explicit_tet10_ss | solve_step | base | 5 | 50 | 2.620 s [2.515 s, 2.635 s] | 52.40 ms [50.30 ms, 52.71 ms] | 496.9 [476.9, 499.8] | – | same |
| explicit_tet10_ss | residual | after | 5 | 50 | 2.080 s [2.019 s, 2.122 s] | 41.60 ms [40.37 ms, 42.43 ms] | 394.5 [382.8, 402.4] | 1.004 [0.979, 1.010] | same |
| explicit_tet10_ss | residual | base | 5 | 50 | 2.101 s [2.000 s, 2.135 s] | 42.02 ms [39.99 ms, 42.69 ms] | 398.5 [379.3, 404.8] | – | same |
| explicit_tet10_ss | step_hooks | after | 5 | 100 | 300.6 µs [229.8 µs, 317.8 µs] | 3.0 µs [2.3 µs, 3.2 µs] | 0.0 [0.0, 0.0] | 0.001 [0.001, 0.001] | same |
| explicit_tet10_ss | step_hooks | base | 5 | 100 | 247.68 ms [230.44 ms, 252.48 ms] | 2.48 ms [2.30 ms, 2.52 ms] | 23.5 [21.9, 23.9] | – | same |
| explicit_tet10_ss | energies | after | 5 | 10 | 72.72 ms [72.28 ms, 74.92 ms] | 7.27 ms [7.23 ms, 7.49 ms] | 69.0 [68.5, 71.0] | 0.130 [0.127, 0.134] | same |
| explicit_tet10_ss | energies | base | 5 | 10 | 560.35 ms [551.92 ms, 570.80 ms] | 56.03 ms [55.19 ms, 57.08 ms] | 531.4 [523.4, 541.3] | – | same |
| explicit_tet4_fd | init | after | 5 | 1 | 130.10 ms [129.11 ms, 132.31 ms] | 130.10 ms [129.11 ms, 132.31 ms] | 803.1 [797.0, 816.7] | 0.980 [0.934, 0.990] | same |
| explicit_tet4_fd | init | base | 5 | 1 | 133.04 ms [130.69 ms, 139.33 ms] | 133.04 ms [130.69 ms, 139.33 ms] | 821.2 [806.8, 860.1] | – | same |
| explicit_tet4_fd | solve_step | after | 5 | 150 | 2.935 s [2.918 s, 2.997 s] | 19.57 ms [19.46 ms, 19.98 ms] | 120.8 [120.1, 123.3] | 0.848 [0.844, 0.849] | same |
| explicit_tet4_fd | solve_step | base | 5 | 150 | 3.475 s [3.440 s, 3.532 s] | 23.17 ms [22.93 ms, 23.55 ms] | 143.0 [141.6, 145.4] | – | same |
| explicit_tet4_fd | residual | after | 5 | 150 | 2.799 s [2.781 s, 2.855 s] | 18.66 ms [18.54 ms, 19.03 ms] | 115.2 [114.5, 117.5] | 1.017 [1.011, 1.020] | same |
| explicit_tet4_fd | residual | base | 5 | 150 | 2.764 s [2.730 s, 2.798 s] | 18.43 ms [18.20 ms, 18.65 ms] | 113.8 [112.4, 115.2] | – | same |
| explicit_tet4_fd | step_hooks | after | 5 | 300 | 784.8 µs [646.5 µs, 843.1 µs] | 2.6 µs [2.2 µs, 2.8 µs] | 0.0 [0.0, 0.0] | 0.001 [0.001, 0.002] | same |
| explicit_tet4_fd | step_hooks | base | 5 | 300 | 566.13 ms [560.84 ms, 584.75 ms] | 1.89 ms [1.87 ms, 1.95 ms] | 11.6 [11.5, 12.0] | – | same |
| explicit_tet4_fd | energies | after | 5 | 30 | 244.27 ms [239.85 ms, 250.28 ms] | 8.14 ms [7.99 ms, 8.34 ms] | 50.3 [49.4, 51.5] | 0.854 [0.821, 0.878] | same |
| explicit_tet4_fd | energies | base | 5 | 30 | 285.09 ms [283.73 ms, 292.03 ms] | 9.50 ms [9.46 ms, 9.73 ms] | 58.7 [58.4, 60.1] | – | same |
| explicit_tet4_ss | init | after | 5 | 1 | 111.46 ms [109.72 ms, 114.19 ms] | 111.46 ms [109.72 ms, 114.19 ms] | 688.0 [677.3, 704.9] | 0.979 [0.934, 1.011] | same |
| explicit_tet4_ss | init | base | 5 | 1 | 112.92 ms [112.03 ms, 119.96 ms] | 112.92 ms [112.03 ms, 119.96 ms] | 697.0 [691.5, 740.5] | – | same |
| explicit_tet4_ss | solve_step | after | 5 | 200 | 2.806 s [2.767 s, 2.840 s] | 14.03 ms [13.84 ms, 14.20 ms] | 86.6 [85.4, 87.7] | 0.874 [0.853, 0.878] | same |
| explicit_tet4_ss | solve_step | base | 5 | 200 | 3.212 s [3.183 s, 3.329 s] | 16.06 ms [15.91 ms, 16.65 ms] | 99.1 [98.2, 102.8] | – | same |
| explicit_tet4_ss | residual | after | 5 | 200 | 2.623 s [2.585 s, 2.657 s] | 13.12 ms [12.93 ms, 13.28 ms] | 81.0 [79.8, 82.0] | 1.038 [1.016, 1.045] | same |
| explicit_tet4_ss | residual | base | 5 | 200 | 2.520 s [2.495 s, 2.616 s] | 12.60 ms [12.48 ms, 13.08 ms] | 77.8 [77.0, 80.7] | – | same |
| explicit_tet4_ss | step_hooks | after | 5 | 400 | 952.2 µs [705.4 µs, 1.00 ms] | 2.4 µs [1.8 µs, 2.5 µs] | 0.0 [0.0, 0.0] | 0.002 [0.001, 0.002] | same |
| explicit_tet4_ss | step_hooks | base | 5 | 400 | 498.34 ms [496.60 ms, 515.88 ms] | 1.25 ms [1.24 ms, 1.29 ms] | 7.7 [7.7, 8.0] | – | same |
| explicit_tet4_ss | energies | after | 5 | 40 | 270.28 ms [268.08 ms, 276.22 ms] | 6.76 ms [6.70 ms, 6.91 ms] | 41.7 [41.4, 42.6] | 0.795 [0.781, 0.803] | same |
| explicit_tet4_ss | energies | base | 5 | 40 | 344.34 ms [335.41 ms, 347.69 ms] | 8.61 ms [8.39 ms, 8.69 ms] | 53.1 [51.8, 53.7] | – | same |
| explicit_thermal | init | after | 5 | 1 | 72.48 ms [72.00 ms, 72.61 ms] | 72.48 ms [72.00 ms, 72.61 ms] | 687.3 [682.7, 688.5] | 0.985 [0.970, 0.990] | same |
| explicit_thermal | init | base | 5 | 1 | 73.43 ms [73.31 ms, 74.74 ms] | 73.43 ms [73.31 ms, 74.74 ms] | 696.3 [695.2, 708.7] | – | same |
| explicit_thermal | solve_step | after | 5 | 300 | 2.762 s [2.749 s, 2.810 s] | 9.21 ms [9.16 ms, 9.37 ms] | 87.3 [86.9, 88.8] | 0.890 [0.887, 0.893] | same |
| explicit_thermal | solve_step | base | 5 | 300 | 3.098 s [3.097 s, 3.153 s] | 10.33 ms [10.32 ms, 10.51 ms] | 97.9 [97.9, 99.7] | – | same |
| explicit_thermal | residual | after | 5 | 300 | 2.529 s [2.526 s, 2.581 s] | 8.43 ms [8.42 ms, 8.60 ms] | 80.0 [79.8, 81.6] | 1.048 [1.047, 1.051] | same |
| explicit_thermal | residual | base | 5 | 300 | 2.412 s [2.408 s, 2.465 s] | 8.04 ms [8.03 ms, 8.22 ms] | 76.2 [76.1, 77.9] | – | same |
| explicit_thermal | step_hooks | after | 5 | 600 | 1.08 ms [972.4 µs, 1.36 ms] | 1.8 µs [1.6 µs, 2.3 µs] | 0.0 [0.0, 0.0] | 0.002 [0.002, 0.003] | same |
| explicit_thermal | step_hooks | base | 5 | 600 | 459.08 ms [458.05 ms, 461.88 ms] | 765.1 µs [763.4 µs, 769.8 µs] | 7.3 [7.2, 7.3] | – | same |
| explicit_thermal | energies | after | 5 | 60 | 263.80 ms [263.25 ms, 268.69 ms] | 4.40 ms [4.39 ms, 4.48 ms] | 41.7 [41.6, 42.5] | 0.833 [0.825, 0.841] | same |
| explicit_thermal | energies | base | 5 | 60 | 318.04 ms [313.56 ms, 322.53 ms] | 5.30 ms [5.23 ms, 5.38 ms] | 50.3 [49.6, 51.0] | – | same |
| explicit_tri3_fd | init | after | 5 | 1 | 93.58 ms [93.02 ms, 103.04 ms] | 93.58 ms [93.02 ms, 103.04 ms] | 519.9 [516.8, 572.4] | 0.998 [0.990, 1.084] | same |
| explicit_tri3_fd | init | base | 5 | 1 | 94.17 ms [93.76 ms, 95.01 ms] | 94.17 ms [93.76 ms, 95.01 ms] | 523.2 [520.9, 527.8] | – | same |
| explicit_tri3_fd | solve_step | after | 5 | 200 | 2.409 s [2.401 s, 2.460 s] | 12.05 ms [12.00 ms, 12.30 ms] | 66.9 [66.7, 68.3] | 0.860 [0.857, 0.873] | same |
| explicit_tri3_fd | solve_step | base | 5 | 200 | 2.807 s [2.792 s, 2.816 s] | 14.03 ms [13.96 ms, 14.08 ms] | 78.0 [77.6, 78.2] | – | same |
| explicit_tri3_fd | residual | after | 5 | 200 | 2.042 s [2.034 s, 2.085 s] | 10.21 ms [10.17 ms, 10.42 ms] | 56.7 [56.5, 57.9] | 1.009 [1.001, 1.022] | same |
| explicit_tri3_fd | residual | base | 5 | 200 | 2.029 s [2.022 s, 2.040 s] | 10.14 ms [10.11 ms, 10.20 ms] | 56.4 [56.2, 56.7] | – | same |
| explicit_tri3_fd | step_hooks | after | 5 | 400 | 923.8 µs [868.9 µs, 1.28 ms] | 2.3 µs [2.2 µs, 3.2 µs] | 0.0 [0.0, 0.0] | 0.002 [0.002, 0.003] | same |
| explicit_tri3_fd | step_hooks | base | 5 | 400 | 392.72 ms [390.01 ms, 395.56 ms] | 981.8 µs [975.0 µs, 988.9 µs] | 5.5 [5.4, 5.5] | – | same |
| explicit_tri3_fd | energies | after | 5 | 40 | 300.11 ms [299.54 ms, 303.57 ms] | 7.50 ms [7.49 ms, 7.59 ms] | 41.7 [41.6, 42.2] | 0.896 [0.888, 0.906] | same |
| explicit_tri3_fd | energies | base | 5 | 40 | 334.86 ms [333.90 ms, 337.99 ms] | 8.37 ms [8.35 ms, 8.45 ms] | 46.5 [46.4, 46.9] | – | same |
| explicit_tri3_ss | init | after | 5 | 1 | 85.13 ms [83.55 ms, 87.34 ms] | 85.13 ms [83.55 ms, 87.34 ms] | 473.0 [464.2, 485.2] | 1.002 [0.957, 1.017] | same |
| explicit_tri3_ss | init | base | 5 | 1 | 85.73 ms [84.23 ms, 87.44 ms] | 85.73 ms [84.23 ms, 87.44 ms] | 476.3 [467.9, 485.8] | – | same |
| explicit_tri3_ss | solve_step | after | 5 | 300 | 3.331 s [3.315 s, 3.366 s] | 11.10 ms [11.05 ms, 11.22 ms] | 61.7 [61.4, 62.3] | 0.896 [0.885, 0.899] | same |
| explicit_tri3_ss | solve_step | base | 5 | 300 | 3.733 s [3.703 s, 3.746 s] | 12.44 ms [12.34 ms, 12.49 ms] | 69.1 [68.6, 69.4] | – | same |
| explicit_tri3_ss | residual | after | 5 | 300 | 2.771 s [2.762 s, 2.801 s] | 9.24 ms [9.21 ms, 9.34 ms] | 51.3 [51.1, 51.9] | 1.010 [1.001, 1.013] | same |
| explicit_tri3_ss | residual | base | 5 | 300 | 2.750 s [2.736 s, 2.765 s] | 9.17 ms [9.12 ms, 9.22 ms] | 50.9 [50.7, 51.2] | – | same |
| explicit_tri3_ss | step_hooks | after | 5 | 600 | 1.25 ms [1.14 ms, 1.38 ms] | 2.1 µs [1.9 µs, 2.3 µs] | 0.0 [0.0, 0.0] | 0.003 [0.003, 0.003] | same |
| explicit_tri3_ss | step_hooks | base | 5 | 600 | 407.35 ms [400.19 ms, 410.16 ms] | 678.9 µs [667.0 µs, 683.6 µs] | 3.8 [3.7, 3.8] | – | same |
| explicit_tri3_ss | energies | after | 5 | 60 | 437.09 ms [436.64 ms, 441.32 ms] | 7.28 ms [7.28 ms, 7.36 ms] | 40.5 [40.4, 40.9] | 0.908 [0.903, 0.912] | same |
| explicit_tri3_ss | energies | base | 5 | 60 | 483.76 ms [480.00 ms, 483.87 ms] | 8.06 ms [8.00 ms, 8.06 ms] | 44.8 [44.4, 44.8] | – | same |
| setup_hex8 | init_explicit | after | 5 | 1 | 77.95 ms [77.56 ms, 79.31 ms] | 77.95 ms [77.56 ms, 79.31 ms] | 5638.4 [5610.5, 5737.4] | 0.988 [0.961, 1.003] | same |
| setup_hex8 | init_explicit | base | 5 | 1 | 78.55 ms [78.26 ms, 80.91 ms] | 78.55 ms [78.26 ms, 80.91 ms] | 5682.4 [5661.4, 5852.9] | – | same |
| setup_hex8 | shape_functions | after | 5 | 3 | 98.60 ms [98.00 ms, 102.63 ms] | 32.87 ms [32.67 ms, 34.21 ms] | 2377.5 [2363.1, 2474.7] | 0.970 [0.963, 1.023] | same |
| setup_hex8 | shape_functions | base | 5 | 3 | 101.03 ms [100.33 ms, 103.13 ms] | 33.68 ms [33.44 ms, 34.38 ms] | 2436.2 [2419.2, 2486.7] | – | same |
| setup_tet4 | init_explicit | after | 5 | 1 | 112.04 ms [109.87 ms, 112.18 ms] | 112.04 ms [109.87 ms, 112.18 ms] | 691.6 [678.2, 692.5] | 1.004 [0.968, 1.012] | same |
| setup_tet4 | init_explicit | base | 5 | 1 | 111.27 ms [110.87 ms, 113.56 ms] | 111.27 ms [110.87 ms, 113.56 ms] | 686.8 [684.4, 701.0] | – | same |
| setup_tet4 | shape_functions | after | 5 | 3 | 145.57 ms [144.91 ms, 149.64 ms] | 48.52 ms [48.30 ms, 49.88 ms] | 299.5 [298.2, 307.9] | 0.978 [0.974, 1.010] | same |
| setup_tet4 | shape_functions | base | 5 | 3 | 148.76 ms [148.20 ms, 149.38 ms] | 49.59 ms [49.40 ms, 49.79 ms] | 306.1 [304.9, 307.4] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_bilinear | 48000 | 46338 | 139014 | 1 | 84310238f, b3f6c78aa | b6a0645b6cf17997 |
| cohesive_intrinsic_exponential | 48000 | 46338 | 139014 | 1 | 84310238f, b3f6c78aa | 2e07a3f9b642f09a |
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | 84310238f, b3f6c78aa | 78b072c2f464d8f9 |
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | 84310238f, b3f6c78aa | 37ce0eeb914dd994 |
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 84310238f, b3f6c78aa | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 36137566d8b810bb |
| explicit_tet10_fd | 105456 | 148877 | 446631 | 1 | 84310238f, b3f6c78aa | c9f013e1a9bbd7a1 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | 84310238f, b3f6c78aa | a12864e94a9122c8 |
| explicit_tet4_fd | 162000 | 29791 | 89373 | 1 | 84310238f, b3f6c78aa | 9d6cdbdad424d346 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 84310238f, b3f6c78aa | fecb3fb4b46c8b8c |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | 84310238f, b3f6c78aa | 27979d9fbcdea83d |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 84310238f, b3f6c78aa | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 84310238f, b3f6c78aa | b6b902f5ca71049e |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 84310238f, b3f6c78aa | 699ba53ab1309e89 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 84310238f, b3f6c78aa | 562b4d2b6ad29d67 |
