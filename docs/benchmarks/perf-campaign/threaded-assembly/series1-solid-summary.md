Suite `solid`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 399.4 µs [389.0 µs, 409.3 µs] | 399.4 µs [389.0 µs, 409.3 µs] | 110.9 [108.1, 113.7] | – | same |
| cohesive_intrinsic_implicit | mesh | t1 | 5 | 1 | 397.8 µs [364.8 µs, 411.3 µs] | 397.8 µs [364.8 µs, 411.3 µs] | 110.5 [101.3, 114.2] | 0.988 [0.921, 1.022] | same |
| cohesive_intrinsic_implicit | mesh | t2 | 5 | 1 | 381.6 µs [370.0 µs, 392.5 µs] | 381.6 µs [370.0 µs, 392.5 µs] | 106.0 [102.8, 109.0] | 0.963 [0.906, 0.985] | same |
| cohesive_intrinsic_implicit | mesh | t4 | 5 | 1 | 389.7 µs [385.5 µs, 407.6 µs] | 389.7 µs [385.5 µs, 407.6 µs] | 108.3 [107.1, 113.2] | 1.002 [0.942, 1.012] | same |
| cohesive_intrinsic_implicit | mesh | t8 | 5 | 1 | 392.3 µs [388.4 µs, 408.1 µs] | 392.3 µs [388.4 µs, 408.1 µs] | 109.0 [107.9, 113.4] | 0.982 [0.951, 1.030] | same |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 10.06 ms [10.02 ms, 10.29 ms] | 10.06 ms [10.02 ms, 10.29 ms] | 2793.1 [2782.5, 2858.6] | – | same |
| cohesive_intrinsic_implicit | init | t1 | 5 | 1 | 9.49 ms [9.27 ms, 9.74 ms] | 9.49 ms [9.27 ms, 9.74 ms] | 2635.3 [2574.2, 2706.7] | 0.940 [0.901, 0.973] | same |
| cohesive_intrinsic_implicit | init | t2 | 5 | 1 | 9.53 ms [9.46 ms, 10.12 ms] | 9.53 ms [9.46 ms, 10.12 ms] | 2646.8 [2628.1, 2812.5] | 0.951 [0.924, 0.992] | same |
| cohesive_intrinsic_implicit | init | t4 | 5 | 1 | 10.19 ms [10.15 ms, 10.23 ms] | 10.19 ms [10.15 ms, 10.23 ms] | 2831.7 [2818.4, 2841.3] | 1.013 [0.991, 1.017] | same |
| cohesive_intrinsic_implicit | init | t8 | 5 | 1 | 10.25 ms [10.22 ms, 10.46 ms] | 10.25 ms [10.22 ms, 10.46 ms] | 2846.9 [2838.9, 2905.3] | 1.023 [0.993, 1.037] | same |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 2.857 s [2.824 s, 2.885 s] | 95.25 ms [94.13 ms, 96.18 ms] | 26457.9 [26146.6, 26716.9] | – | same |
| cohesive_intrinsic_implicit | solve_step | t1 | 5 | 30 | 2.866 s [2.852 s, 2.873 s] | 95.53 ms [95.07 ms, 95.78 ms] | 26535.2 [26407.3, 26605.4] | 1.006 [0.988, 1.011] | same |
| cohesive_intrinsic_implicit | solve_step | t2 | 5 | 30 | 2.923 s [2.908 s, 2.975 s] | 97.42 ms [96.94 ms, 99.16 ms] | 27061.1 [26927.2, 27545.2] | 1.026 [1.008, 1.042] | same |
| cohesive_intrinsic_implicit | solve_step | t4 | 5 | 30 | 2.909 s [2.897 s, 2.920 s] | 96.97 ms [96.55 ms, 97.35 ms] | 26934.8 [26820.3, 27040.7] | 1.017 [1.011, 1.029] | same |
| cohesive_intrinsic_implicit | solve_step | t8 | 5 | 30 | 2.884 s [2.874 s, 2.933 s] | 96.14 ms [95.78 ms, 97.77 ms] | 26706.9 [26606.9, 27159.7] | 1.017 [0.998, 1.027] | same |
| implicit_dynamic | mesh | base | 5 | 1 | 664.5 µs [659.4 µs, 684.0 µs] | 664.5 µs [659.4 µs, 684.0 µs] | 40.4 [40.1, 41.5] | – | same |
| implicit_dynamic | mesh | t1 | 5 | 1 | 614.7 µs [599.3 µs, 698.3 µs] | 614.7 µs [599.3 µs, 698.3 µs] | 37.3 [36.4, 42.4] | 0.922 [0.876, 1.059] | same |
| implicit_dynamic | mesh | t2 | 5 | 1 | 640.7 µs [616.7 µs, 660.9 µs] | 640.7 µs [616.7 µs, 660.9 µs] | 38.9 [37.5, 40.1] | 0.964 [0.925, 0.994] | same |
| implicit_dynamic | mesh | t4 | 5 | 1 | 661.7 µs [641.4 µs, 686.5 µs] | 661.7 µs [641.4 µs, 686.5 µs] | 40.2 [39.0, 41.7] | 0.994 [0.962, 1.033] | same |
| implicit_dynamic | mesh | t8 | 5 | 1 | 664.2 µs [643.8 µs, 676.9 µs] | 664.2 µs [643.8 µs, 676.9 µs] | 40.3 [39.1, 41.1] | 1.006 [0.941, 1.026] | same |
| implicit_dynamic | init | base | 5 | 1 | 8.75 ms [8.67 ms, 8.86 ms] | 8.75 ms [8.67 ms, 8.86 ms] | 531.2 [526.9, 538.0] | – | same |
| implicit_dynamic | init | t1 | 5 | 1 | 8.20 ms [8.05 ms, 8.41 ms] | 8.20 ms [8.05 ms, 8.41 ms] | 498.2 [489.1, 511.1] | 0.942 [0.909, 0.968] | same |
| implicit_dynamic | init | t2 | 5 | 1 | 8.65 ms [8.54 ms, 8.71 ms] | 8.65 ms [8.54 ms, 8.71 ms] | 525.2 [518.8, 529.0] | 0.981 [0.977, 1.002] | same |
| implicit_dynamic | init | t4 | 5 | 1 | 8.80 ms [8.47 ms, 8.89 ms] | 8.80 ms [8.47 ms, 8.89 ms] | 534.3 [514.7, 540.0] | 1.000 [0.957, 1.017] | same |
| implicit_dynamic | init | t8 | 5 | 1 | 8.88 ms [8.81 ms, 8.98 ms] | 8.88 ms [8.81 ms, 8.98 ms] | 539.1 [534.9, 545.3] | 1.015 [1.000, 1.033] | same |
| implicit_dynamic | mass_consistent | base | 5 | 1 | 63.76 ms [59.65 ms, 66.39 ms] | 63.76 ms [59.65 ms, 66.39 ms] | 3872.5 [3623.3, 4032.6] | – | same |
| implicit_dynamic | mass_consistent | t1 | 5 | 1 | 61.68 ms [53.40 ms, 62.04 ms] | 61.68 ms [53.40 ms, 62.04 ms] | 3746.1 [3243.6, 3768.1] | 0.929 [0.838, 1.037] | same |
| implicit_dynamic | mass_consistent | t2 | 5 | 1 | 47.56 ms [45.06 ms, 47.62 ms] | 47.56 ms [45.06 ms, 47.62 ms] | 2888.6 [2737.2, 2892.2] | 0.729 [0.717, 0.797] | same |
| implicit_dynamic | mass_consistent | t4 | 5 | 1 | 37.22 ms [32.67 ms, 38.12 ms] | 37.22 ms [32.67 ms, 38.12 ms] | 2260.7 [1984.3, 2315.2] | 0.596 [0.500, 0.620] | same |
| implicit_dynamic | mass_consistent | t8 | 5 | 1 | 30.96 ms [28.23 ms, 32.49 ms] | 30.96 ms [28.23 ms, 32.49 ms] | 1880.6 [1714.7, 1973.6] | 0.494 [0.425, 0.521] | same |
| implicit_dynamic | solve_step | base | 5 | 60 | 890.46 ms [888.68 ms, 909.68 ms] | 14.84 ms [14.81 ms, 15.16 ms] | 901.4 [899.6, 920.9] | – | same |
| implicit_dynamic | solve_step | t1 | 5 | 60 | 878.44 ms [865.26 ms, 886.55 ms] | 14.64 ms [14.42 ms, 14.78 ms] | 889.2 [875.9, 897.5] | 0.979 [0.966, 0.998] | same |
| implicit_dynamic | solve_step | t2 | 5 | 60 | 779.24 ms [776.82 ms, 782.83 ms] | 12.99 ms [12.95 ms, 13.05 ms] | 788.8 [786.4, 792.5] | 0.874 [0.858, 0.876] | same |
| implicit_dynamic | solve_step | t4 | 5 | 60 | 737.31 ms [734.34 ms, 739.27 ms] | 12.29 ms [12.24 ms, 12.32 ms] | 746.4 [743.4, 748.4] | 0.828 [0.807, 0.830] | same |
| implicit_dynamic | solve_step | t8 | 5 | 60 | 725.38 ms [721.67 ms, 726.36 ms] | 12.09 ms [12.03 ms, 12.11 ms] | 734.3 [730.5, 735.3] | 0.812 [0.798, 0.816] | same |
| implicit_dynamic | predictor | base | 5 | 60 | 10.3 µs [9.6 µs, 10.7 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | predictor | t1 | 5 | 60 | 8.4 µs [8.1 µs, 10.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.848 [0.759, 1.075] | same |
| implicit_dynamic | predictor | t2 | 5 | 60 | 8.6 µs [5.9 µs, 10.1 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.807 [0.575, 0.975] | same |
| implicit_dynamic | predictor | t4 | 5 | 60 | 9.1 µs [7.5 µs, 17.8 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.904 [0.704, 1.855] | same |
| implicit_dynamic | predictor | t8 | 5 | 60 | 8.0 µs [6.8 µs, 10.4 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.775 [0.630, 1.082] | same |
| implicit_dynamic | residual | base | 5 | 180 | 193.80 ms [193.05 ms, 196.38 ms] | 1.08 ms [1.07 ms, 1.09 ms] | 65.4 [65.1, 66.3] | – | same |
| implicit_dynamic | residual | t1 | 5 | 180 | 196.09 ms [194.95 ms, 198.55 ms] | 1.09 ms [1.08 ms, 1.10 ms] | 66.2 [65.8, 67.0] | 1.008 [1.003, 1.024] | same |
| implicit_dynamic | residual | t2 | 5 | 180 | 113.55 ms [113.29 ms, 114.22 ms] | 630.8 µs [629.4 µs, 634.6 µs] | 38.3 [38.2, 38.5] | 0.586 [0.579, 0.588] | same |
| implicit_dynamic | residual | t4 | 5 | 180 | 71.18 ms [70.81 ms, 71.38 ms] | 395.4 µs [393.4 µs, 396.6 µs] | 24.0 [23.9, 24.1] | 0.367 [0.362, 0.370] | same |
| implicit_dynamic | residual | t8 | 5 | 180 | 57.16 ms [56.57 ms, 58.33 ms] | 317.5 µs [314.3 µs, 324.0 µs] | 19.3 [19.1, 19.7] | 0.295 [0.290, 0.302] | same |
| implicit_dynamic | matrix | base | 5 | 420 | 152.1 µs [144.4 µs, 158.8 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | matrix | t1 | 5 | 420 | 149.2 µs [143.5 µs, 151.6 µs] | 0.4 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.971 [0.916, 1.050] | same |
| implicit_dynamic | matrix | t2 | 5 | 420 | 137.8 µs [137.2 µs, 148.7 µs] | 0.3 µs [0.3 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 0.915 [0.881, 0.978] | same |
| implicit_dynamic | matrix | t4 | 5 | 420 | 133.3 µs [129.6 µs, 139.4 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.887 [0.853, 0.899] | same |
| implicit_dynamic | matrix | t8 | 5 | 420 | 136.2 µs [130.5 µs, 146.6 µs] | 0.3 µs [0.3 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.901 [0.874, 0.923] | same |
| implicit_dynamic | corrector | base | 5 | 120 | 18.6 µs [17.9 µs, 20.9 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| implicit_dynamic | corrector | t1 | 5 | 120 | 23.9 µs [22.9 µs, 25.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.242 [1.153, 1.449] | same |
| implicit_dynamic | corrector | t2 | 5 | 120 | 24.2 µs [22.4 µs, 26.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.204 [1.132, 1.475] | same |
| implicit_dynamic | corrector | t4 | 5 | 120 | 25.1 µs [23.2 µs, 26.0 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.348 [1.114, 1.390] | same |
| implicit_dynamic | corrector | t8 | 5 | 120 | 24.0 µs [20.4 µs, 24.8 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.189 [1.028, 1.355] | same |
| implicit_dynamic | step_hooks | base | 5 | 120 | 10.35 ms [10.32 ms, 10.40 ms] | 86.2 µs [86.0 µs, 86.7 µs] | 5.2 [5.2, 5.3] | – | same |
| implicit_dynamic | step_hooks | t1 | 5 | 120 | 10.38 ms [10.18 ms, 10.44 ms] | 86.5 µs [84.8 µs, 87.0 µs] | 5.3 [5.2, 5.3] | 0.998 [0.986, 1.010] | same |
| implicit_dynamic | step_hooks | t2 | 5 | 120 | 10.36 ms [10.27 ms, 10.38 ms] | 86.4 µs [85.6 µs, 86.5 µs] | 5.2 [5.2, 5.3] | 1.002 [0.989, 1.005] | same |
| implicit_dynamic | step_hooks | t4 | 5 | 120 | 10.41 ms [10.36 ms, 10.47 ms] | 86.8 µs [86.3 µs, 87.3 µs] | 5.3 [5.2, 5.3] | 1.004 [1.001, 1.012] | same |
| implicit_dynamic | step_hooks | t8 | 5 | 120 | 10.16 ms [10.05 ms, 10.19 ms] | 84.7 µs [83.7 µs, 84.9 µs] | 5.1 [5.1, 5.2] | 0.980 [0.973, 0.986] | same |
| implicit_dynamic | step_rest | base | 5 | 60 | 686.57 ms [685.06 ms, 702.77 ms] | 11.44 ms [11.42 ms, 11.71 ms] | 695.0 [693.5, 711.4] | – | same |
| implicit_dynamic | step_rest | t1 | 5 | 60 | 670.51 ms [659.95 ms, 679.84 ms] | 11.18 ms [11.00 ms, 11.33 ms] | 678.8 [668.1, 688.2] | 0.972 [0.954, 0.992] | same |
| implicit_dynamic | step_rest | t2 | 5 | 60 | 655.18 ms [652.94 ms, 658.07 ms] | 10.92 ms [10.88 ms, 10.97 ms] | 663.2 [661.0, 666.2] | 0.953 [0.934, 0.956] | same |
| implicit_dynamic | step_rest | t4 | 5 | 60 | 655.34 ms [652.66 ms, 657.81 ms] | 10.92 ms [10.88 ms, 10.96 ms] | 663.4 [660.7, 665.9] | 0.955 [0.929, 0.957] | same |
| implicit_dynamic | step_rest | t8 | 5 | 60 | 657.72 ms [654.78 ms, 658.37 ms] | 10.96 ms [10.91 ms, 10.97 ms] | 665.8 [662.8, 666.5] | 0.956 [0.936, 0.958] | same |
| setup_hex8 | mesh | base | 5 | 1 | 1.57 ms [1.56 ms, 1.58 ms] | 1.57 ms [1.56 ms, 1.58 ms] | 113.8 [112.5, 114.3] | – | same |
| setup_hex8 | mesh | t1 | 5 | 1 | 1.54 ms [1.52 ms, 1.58 ms] | 1.54 ms [1.52 ms, 1.58 ms] | 111.5 [110.2, 114.0] | 0.982 [0.964, 1.001] | same |
| setup_hex8 | mesh | t2 | 5 | 1 | 1.53 ms [1.53 ms, 1.57 ms] | 1.53 ms [1.53 ms, 1.57 ms] | 110.7 [110.6, 113.6] | 0.984 [0.971, 0.995] | same |
| setup_hex8 | mesh | t4 | 5 | 1 | 1.57 ms [1.55 ms, 1.60 ms] | 1.57 ms [1.55 ms, 1.60 ms] | 113.7 [112.3, 116.1] | 0.997 [0.987, 1.017] | same |
| setup_hex8 | mesh | t8 | 5 | 1 | 1.58 ms [1.56 ms, 1.66 ms] | 1.58 ms [1.56 ms, 1.66 ms] | 114.2 [113.0, 119.8] | 1.015 [0.989, 1.050] | same |
| setup_hex8 | init_explicit | base | 5 | 1 | 58.89 ms [58.54 ms, 59.86 ms] | 58.89 ms [58.54 ms, 59.86 ms] | 4260.3 [4234.4, 4329.9] | – | same |
| setup_hex8 | init_explicit | t1 | 5 | 1 | 59.67 ms [58.30 ms, 61.92 ms] | 59.67 ms [58.30 ms, 61.92 ms] | 4316.3 [4217.3, 4479.2] | 1.014 [0.990, 1.050] | same |
| setup_hex8 | init_explicit | t2 | 5 | 1 | 57.74 ms [56.96 ms, 59.16 ms] | 57.74 ms [56.96 ms, 59.16 ms] | 4177.0 [4120.0, 4279.9] | 0.986 [0.966, 0.988] | same |
| setup_hex8 | init_explicit | t4 | 5 | 1 | 59.55 ms [58.22 ms, 59.88 ms] | 59.55 ms [58.22 ms, 59.88 ms] | 4307.7 [4211.6, 4331.8] | 1.016 [0.989, 1.019] | same |
| setup_hex8 | init_explicit | t8 | 5 | 1 | 60.08 ms [59.85 ms, 62.13 ms] | 60.08 ms [59.85 ms, 62.13 ms] | 4345.8 [4329.7, 4494.2] | 1.022 [1.017, 1.050] | same |
| setup_hex8 | shape_functions | base | 5 | 3 | 72.45 ms [71.08 ms, 73.77 ms] | 24.15 ms [23.69 ms, 24.59 ms] | 1746.9 [1713.8, 1778.8] | – | same |
| setup_hex8 | shape_functions | t1 | 5 | 3 | 73.30 ms [70.99 ms, 75.92 ms] | 24.43 ms [23.66 ms, 25.31 ms] | 1767.4 [1711.7, 1830.5] | 1.022 [0.963, 1.048] | same |
| setup_hex8 | shape_functions | t2 | 5 | 3 | 74.52 ms [70.12 ms, 75.78 ms] | 24.84 ms [23.37 ms, 25.26 ms] | 1796.8 [1690.8, 1827.3] | 1.010 [0.968, 1.054] | same |
| setup_hex8 | shape_functions | t4 | 5 | 3 | 74.45 ms [71.30 ms, 76.64 ms] | 24.82 ms [23.77 ms, 25.55 ms] | 1795.3 [1719.2, 1848.1] | 1.038 [1.003, 1.039] | same |
| setup_hex8 | shape_functions | t8 | 5 | 3 | 74.13 ms [72.07 ms, 79.51 ms] | 24.71 ms [24.02 ms, 26.50 ms] | 1787.6 [1737.7, 1917.2] | 1.005 [0.993, 1.119] | same |
| setup_hex8 | mass_lumped | base | 5 | 3 | 16.86 ms [15.17 ms, 17.21 ms] | 5.62 ms [5.06 ms, 5.74 ms] | 406.5 [365.9, 414.9] | – | same |
| setup_hex8 | mass_lumped | t1 | 5 | 3 | 17.68 ms [17.03 ms, 17.80 ms] | 5.89 ms [5.68 ms, 5.93 ms] | 426.3 [410.7, 429.2] | 1.037 [1.010, 1.173] | same |
| setup_hex8 | mass_lumped | t2 | 5 | 3 | 16.92 ms [16.79 ms, 17.22 ms] | 5.64 ms [5.60 ms, 5.74 ms] | 408.0 [404.9, 415.2] | 1.012 [0.993, 1.107] | same |
| setup_hex8 | mass_lumped | t4 | 5 | 3 | 16.89 ms [16.58 ms, 17.06 ms] | 5.63 ms [5.53 ms, 5.69 ms] | 407.2 [399.8, 411.2] | 1.000 [0.977, 1.113] | same |
| setup_hex8 | mass_lumped | t8 | 5 | 3 | 17.14 ms [16.93 ms, 17.18 ms] | 5.71 ms [5.64 ms, 5.73 ms] | 413.4 [408.1, 414.2] | 1.019 [0.984, 1.131] | same |
| setup_hex8 | init_implicit | base | 5 | 1 | 49.54 ms [47.91 ms, 51.40 ms] | 49.54 ms [47.91 ms, 51.40 ms] | 3583.4 [3465.5, 3718.3] | – | same |
| setup_hex8 | init_implicit | t1 | 5 | 1 | 48.70 ms [47.77 ms, 50.52 ms] | 48.70 ms [47.77 ms, 50.52 ms] | 3522.5 [3455.6, 3654.2] | 0.990 [0.937, 1.054] | same |
| setup_hex8 | init_implicit | t2 | 5 | 1 | 48.70 ms [46.80 ms, 49.52 ms] | 48.70 ms [46.80 ms, 49.52 ms] | 3523.1 [3385.7, 3582.3] | 0.978 [0.944, 1.026] | same |
| setup_hex8 | init_implicit | t4 | 5 | 1 | 48.85 ms [47.40 ms, 49.73 ms] | 48.85 ms [47.40 ms, 49.73 ms] | 3533.8 [3428.6, 3597.4] | 0.982 [0.950, 1.035] | same |
| setup_hex8 | init_implicit | t8 | 5 | 1 | 49.27 ms [47.74 ms, 50.28 ms] | 49.27 ms [47.74 ms, 50.28 ms] | 3564.2 [3453.3, 3637.3] | 0.996 [0.946, 1.015] | same |
| setup_hex8 | mass_consistent | base | 5 | 3 | 314.52 ms [311.07 ms, 325.88 ms] | 104.84 ms [103.69 ms, 108.63 ms] | 7584.0 [7500.7, 7857.9] | – | same |
| setup_hex8 | mass_consistent | t1 | 5 | 3 | 246.79 ms [237.87 ms, 250.48 ms] | 82.26 ms [79.29 ms, 83.49 ms] | 5950.8 [5735.6, 6039.6] | 0.789 [0.750, 0.805] | same |
| setup_hex8 | mass_consistent | t2 | 5 | 3 | 129.28 ms [128.27 ms, 131.29 ms] | 43.09 ms [42.76 ms, 43.76 ms] | 3117.3 [3092.8, 3165.8] | 0.411 [0.403, 0.419] | same |
| setup_hex8 | mass_consistent | t4 | 5 | 3 | 68.77 ms [68.43 ms, 69.46 ms] | 22.92 ms [22.81 ms, 23.15 ms] | 1658.1 [1650.1, 1674.9] | 0.219 [0.211, 0.223] | same |
| setup_hex8 | mass_consistent | t8 | 5 | 3 | 38.79 ms [38.39 ms, 38.88 ms] | 12.93 ms [12.80 ms, 12.96 ms] | 935.4 [925.7, 937.4] | 0.123 [0.119, 0.124] | same |
| setup_hex8 | stiffness_assembly | base | 5 | 3 | 1.470 s [1.463 s, 1.472 s] | 490.11 ms [487.68 ms, 490.57 ms] | 35453.9 [35277.9, 35487.1] | – | same |
| setup_hex8 | stiffness_assembly | t1 | 5 | 3 | 577.77 ms [569.59 ms, 578.88 ms] | 192.59 ms [189.86 ms, 192.96 ms] | 13931.6 [13734.4, 13958.3] | 0.393 [0.387, 0.395] | same |
| setup_hex8 | stiffness_assembly | t2 | 5 | 3 | 299.12 ms [295.98 ms, 300.45 ms] | 99.71 ms [98.66 ms, 100.15 ms] | 7212.5 [7136.9, 7244.7] | 0.204 [0.201, 0.205] | same |
| setup_hex8 | stiffness_assembly | t4 | 5 | 3 | 160.67 ms [159.31 ms, 163.24 ms] | 53.56 ms [53.10 ms, 54.41 ms] | 3874.2 [3841.3, 3936.2] | 0.109 [0.108, 0.111] | same |
| setup_hex8 | stiffness_assembly | t8 | 5 | 3 | 89.46 ms [88.55 ms, 89.79 ms] | 29.82 ms [29.52 ms, 29.93 ms] | 2157.1 [2135.2, 2165.0] | 0.061 [0.061, 0.061] | same |
| setup_tet10 | mesh | base | 5 | 1 | 1.33 ms [1.30 ms, 1.33 ms] | 1.33 ms [1.30 ms, 1.33 ms] | 128.1 [125.1, 128.5] | – | same |
| setup_tet10 | mesh | t1 | 5 | 1 | 1.28 ms [1.22 ms, 1.34 ms] | 1.28 ms [1.22 ms, 1.34 ms] | 123.4 [118.1, 129.7] | 0.973 [0.922, 1.009] | same |
| setup_tet10 | mesh | t2 | 5 | 1 | 1.26 ms [1.25 ms, 1.30 ms] | 1.26 ms [1.25 ms, 1.30 ms] | 121.2 [120.1, 125.1] | 0.957 [0.937, 0.987] | same |
| setup_tet10 | mesh | t4 | 5 | 1 | 1.31 ms [1.30 ms, 1.35 ms] | 1.31 ms [1.30 ms, 1.35 ms] | 126.2 [125.4, 130.1] | 1.009 [0.976, 1.026] | same |
| setup_tet10 | mesh | t8 | 5 | 1 | 1.32 ms [1.29 ms, 1.39 ms] | 1.32 ms [1.29 ms, 1.39 ms] | 127.0 [124.4, 133.7] | 1.005 [0.967, 1.044] | same |
| setup_tet10 | init_explicit | base | 5 | 1 | 29.08 ms [28.59 ms, 29.46 ms] | 29.08 ms [28.59 ms, 29.46 ms] | 2804.7 [2757.1, 2841.1] | – | same |
| setup_tet10 | init_explicit | t1 | 5 | 1 | 29.77 ms [29.32 ms, 30.45 ms] | 29.77 ms [29.32 ms, 30.45 ms] | 2870.9 [2828.0, 2936.7] | 1.029 [1.000, 1.041] | same |
| setup_tet10 | init_explicit | t2 | 5 | 1 | 29.05 ms [28.32 ms, 30.41 ms] | 29.05 ms [28.32 ms, 30.41 ms] | 2802.2 [2731.6, 2933.0] | 0.989 [0.971, 1.064] | same |
| setup_tet10 | init_explicit | t4 | 5 | 1 | 28.24 ms [28.06 ms, 30.83 ms] | 28.24 ms [28.06 ms, 30.83 ms] | 2723.6 [2706.8, 2973.5] | 0.986 [0.953, 1.048] | same |
| setup_tet10 | init_explicit | t8 | 5 | 1 | 29.82 ms [29.48 ms, 30.30 ms] | 29.82 ms [29.48 ms, 30.30 ms] | 2875.9 [2843.0, 2922.3] | 1.031 [1.012, 1.042] | same |
| setup_tet10 | shape_functions | base | 5 | 3 | 37.55 ms [35.66 ms, 38.71 ms] | 12.52 ms [11.89 ms, 12.90 ms] | 1207.1 [1146.4, 1244.5] | – | same |
| setup_tet10 | shape_functions | t1 | 5 | 3 | 38.04 ms [35.21 ms, 38.34 ms] | 12.68 ms [11.74 ms, 12.78 ms] | 1222.8 [1131.9, 1232.5] | 1.013 [0.916, 1.021] | same |
| setup_tet10 | shape_functions | t2 | 5 | 3 | 38.64 ms [37.70 ms, 39.11 ms] | 12.88 ms [12.57 ms, 13.04 ms] | 1242.3 [1212.0, 1257.3] | 1.029 [0.980, 1.097] | same |
| setup_tet10 | shape_functions | t4 | 5 | 3 | 37.91 ms [35.81 ms, 38.47 ms] | 12.64 ms [11.94 ms, 12.82 ms] | 1218.7 [1151.4, 1236.9] | 0.990 [0.931, 1.079] | same |
| setup_tet10 | shape_functions | t8 | 5 | 3 | 38.90 ms [35.15 ms, 39.05 ms] | 12.97 ms [11.72 ms, 13.02 ms] | 1250.6 [1130.0, 1255.5] | 1.013 [0.933, 1.091] | same |
| setup_tet10 | mass_lumped | base | 5 | 3 | 4.23 ms [3.85 ms, 4.27 ms] | 1.41 ms [1.28 ms, 1.42 ms] | 136.1 [123.8, 137.3] | – | same |
| setup_tet10 | mass_lumped | t1 | 5 | 3 | 4.25 ms [3.94 ms, 4.30 ms] | 1.42 ms [1.31 ms, 1.43 ms] | 136.6 [126.6, 138.3] | 1.007 [1.004, 1.022] | same |
| setup_tet10 | mass_lumped | t2 | 5 | 3 | 4.02 ms [3.93 ms, 4.21 ms] | 1.34 ms [1.31 ms, 1.40 ms] | 129.3 [126.3, 135.4] | 0.954 [0.931, 1.020] | same |
| setup_tet10 | mass_lumped | t4 | 5 | 3 | 3.86 ms [3.69 ms, 3.90 ms] | 1.29 ms [1.23 ms, 1.30 ms] | 124.2 [118.5, 125.3] | 0.913 [0.863, 1.011] | same |
| setup_tet10 | mass_lumped | t8 | 5 | 3 | 3.83 ms [3.76 ms, 3.86 ms] | 1.28 ms [1.25 ms, 1.29 ms] | 123.2 [120.9, 124.1] | 0.901 [0.891, 1.002] | same |
| setup_tet10 | init_implicit | base | 5 | 1 | 24.26 ms [23.48 ms, 24.52 ms] | 24.26 ms [23.48 ms, 24.52 ms] | 2340.1 [2265.0, 2365.1] | – | same |
| setup_tet10 | init_implicit | t1 | 5 | 1 | 23.48 ms [23.05 ms, 24.34 ms] | 23.48 ms [23.05 ms, 24.34 ms] | 2265.0 [2223.3, 2347.2] | 0.982 [0.965, 1.003] | same |
| setup_tet10 | init_implicit | t2 | 5 | 1 | 24.40 ms [24.11 ms, 24.84 ms] | 24.40 ms [24.11 ms, 24.84 ms] | 2353.3 [2325.2, 2396.1] | 1.019 [0.983, 1.039] | same |
| setup_tet10 | init_implicit | t4 | 5 | 1 | 23.56 ms [23.35 ms, 23.84 ms] | 23.56 ms [23.35 ms, 23.84 ms] | 2272.4 [2252.3, 2299.5] | 0.972 [0.970, 0.994] | same |
| setup_tet10 | init_implicit | t8 | 5 | 1 | 24.18 ms [22.45 ms, 24.96 ms] | 24.18 ms [22.45 ms, 24.96 ms] | 2331.8 [2165.4, 2407.7] | 0.996 [0.935, 1.063] | same |
| setup_tet10 | mass_consistent | base | 5 | 3 | 247.26 ms [244.92 ms, 258.82 ms] | 82.42 ms [81.64 ms, 86.27 ms] | 7949.4 [7874.1, 8321.2] | – | same |
| setup_tet10 | mass_consistent | t1 | 5 | 3 | 184.89 ms [179.51 ms, 185.30 ms] | 61.63 ms [59.84 ms, 61.77 ms] | 5944.1 [5771.2, 5957.3] | 0.734 [0.697, 0.755] | same |
| setup_tet10 | mass_consistent | t2 | 5 | 3 | 92.62 ms [91.13 ms, 94.07 ms] | 30.87 ms [30.38 ms, 31.36 ms] | 2977.6 [2929.7, 3024.3] | 0.372 [0.363, 0.380] | same |
| setup_tet10 | mass_consistent | t4 | 5 | 3 | 50.84 ms [50.75 ms, 51.14 ms] | 16.95 ms [16.92 ms, 17.05 ms] | 1634.7 [1631.6, 1644.2] | 0.206 [0.197, 0.207] | same |
| setup_tet10 | mass_consistent | t8 | 5 | 3 | 28.42 ms [28.25 ms, 28.91 ms] | 9.47 ms [9.42 ms, 9.64 ms] | 913.7 [908.2, 929.6] | 0.115 [0.110, 0.118] | same |
| setup_tet10 | stiffness_assembly | base | 5 | 3 | 907.93 ms [904.66 ms, 938.17 ms] | 302.64 ms [301.55 ms, 312.72 ms] | 29190.0 [29084.9, 30162.3] | – | same |
| setup_tet10 | stiffness_assembly | t1 | 5 | 3 | 352.09 ms [349.53 ms, 359.95 ms] | 117.36 ms [116.51 ms, 119.98 ms] | 11319.9 [11237.6, 11572.6] | 0.388 [0.383, 0.391] | same |
| setup_tet10 | stiffness_assembly | t2 | 5 | 3 | 176.58 ms [175.86 ms, 178.16 ms] | 58.86 ms [58.62 ms, 59.39 ms] | 5677.2 [5654.0, 5727.8] | 0.194 [0.190, 0.195] | same |
| setup_tet10 | stiffness_assembly | t4 | 5 | 3 | 92.73 ms [91.67 ms, 93.38 ms] | 30.91 ms [30.56 ms, 31.13 ms] | 2981.3 [2947.2, 3002.1] | 0.102 [0.098, 0.103] | same |
| setup_tet10 | stiffness_assembly | t8 | 5 | 3 | 51.27 ms [50.87 ms, 51.82 ms] | 17.09 ms [16.96 ms, 17.27 ms] | 1648.4 [1635.5, 1666.1] | 0.056 [0.054, 0.057] | same |
| setup_tet4 | mesh | base | 5 | 1 | 5.58 ms [5.33 ms, 5.59 ms] | 5.58 ms [5.33 ms, 5.59 ms] | 34.4 [32.9, 34.5] | – | same |
| setup_tet4 | mesh | t1 | 5 | 1 | 5.63 ms [5.47 ms, 5.75 ms] | 5.63 ms [5.47 ms, 5.75 ms] | 34.8 [33.8, 35.5] | 1.024 [0.983, 1.031] | same |
| setup_tet4 | mesh | t2 | 5 | 1 | 5.60 ms [5.40 ms, 5.88 ms] | 5.60 ms [5.40 ms, 5.88 ms] | 34.6 [33.4, 36.3] | 1.003 [0.968, 1.104] | same |
| setup_tet4 | mesh | t4 | 5 | 1 | 5.62 ms [5.31 ms, 5.64 ms] | 5.62 ms [5.31 ms, 5.64 ms] | 34.7 [32.8, 34.8] | 1.008 [0.953, 1.054] | same |
| setup_tet4 | mesh | t8 | 5 | 1 | 5.60 ms [5.55 ms, 5.70 ms] | 5.60 ms [5.55 ms, 5.70 ms] | 34.6 [34.3, 35.2] | 1.007 [0.995, 1.052] | same |
| setup_tet4 | init_explicit | base | 5 | 1 | 80.04 ms [79.89 ms, 83.78 ms] | 80.04 ms [79.89 ms, 83.78 ms] | 494.1 [493.1, 517.2] | – | same |
| setup_tet4 | init_explicit | t1 | 5 | 1 | 78.38 ms [75.48 ms, 80.30 ms] | 78.38 ms [75.48 ms, 80.30 ms] | 483.8 [465.9, 495.7] | 0.955 [0.945, 1.004] | same |
| setup_tet4 | init_explicit | t2 | 5 | 1 | 78.68 ms [78.20 ms, 80.05 ms] | 78.68 ms [78.20 ms, 80.05 ms] | 485.7 [482.7, 494.2] | 0.977 [0.953, 1.002] | same |
| setup_tet4 | init_explicit | t4 | 5 | 1 | 78.66 ms [75.83 ms, 80.88 ms] | 78.66 ms [75.83 ms, 80.88 ms] | 485.6 [468.1, 499.2] | 0.978 [0.935, 0.993] | same |
| setup_tet4 | init_explicit | t8 | 5 | 1 | 80.35 ms [76.32 ms, 80.42 ms] | 80.35 ms [76.32 ms, 80.42 ms] | 496.0 [471.1, 496.4] | 0.974 [0.953, 1.007] | same |
| setup_tet4 | shape_functions | base | 5 | 3 | 96.33 ms [94.74 ms, 99.85 ms] | 32.11 ms [31.58 ms, 33.28 ms] | 198.2 [194.9, 205.5] | – | same |
| setup_tet4 | shape_functions | t1 | 5 | 3 | 95.87 ms [92.55 ms, 99.27 ms] | 31.96 ms [30.85 ms, 33.09 ms] | 197.3 [190.4, 204.3] | 0.979 [0.950, 1.048] | same |
| setup_tet4 | shape_functions | t2 | 5 | 3 | 95.41 ms [93.54 ms, 100.25 ms] | 31.80 ms [31.18 ms, 33.42 ms] | 196.3 [192.5, 206.3] | 0.990 [0.973, 1.008] | same |
| setup_tet4 | shape_functions | t4 | 5 | 3 | 96.04 ms [91.40 ms, 105.49 ms] | 32.01 ms [30.47 ms, 35.16 ms] | 197.6 [188.1, 217.1] | 1.007 [0.938, 1.100] | same |
| setup_tet4 | shape_functions | t8 | 5 | 3 | 95.90 ms [93.50 ms, 96.58 ms] | 31.97 ms [31.17 ms, 32.19 ms] | 197.3 [192.4, 198.7] | 0.971 [0.962, 1.012] | same |
| setup_tet4 | mass_lumped | base | 5 | 3 | 64.00 ms [60.49 ms, 65.22 ms] | 21.33 ms [20.16 ms, 21.74 ms] | 131.7 [124.5, 134.2] | – | same |
| setup_tet4 | mass_lumped | t1 | 5 | 3 | 64.39 ms [61.86 ms, 65.11 ms] | 21.46 ms [20.62 ms, 21.70 ms] | 132.5 [127.3, 134.0] | 1.000 [0.960, 1.064] | same |
| setup_tet4 | mass_lumped | t2 | 5 | 3 | 62.59 ms [62.14 ms, 66.01 ms] | 20.86 ms [20.71 ms, 22.00 ms] | 128.8 [127.9, 135.8] | 1.010 [0.960, 1.034] | same |
| setup_tet4 | mass_lumped | t4 | 5 | 3 | 62.71 ms [62.03 ms, 63.29 ms] | 20.90 ms [20.68 ms, 21.10 ms] | 129.0 [127.6, 130.2] | 0.975 [0.960, 1.046] | same |
| setup_tet4 | mass_lumped | t8 | 5 | 3 | 62.61 ms [61.69 ms, 64.76 ms] | 20.87 ms [20.56 ms, 21.59 ms] | 128.8 [126.9, 133.3] | 0.964 [0.955, 1.050] | same |
| setup_tet4 | init_implicit | base | 5 | 1 | 75.26 ms [71.95 ms, 75.96 ms] | 75.26 ms [71.95 ms, 75.96 ms] | 464.6 [444.1, 468.9] | – | same |
| setup_tet4 | init_implicit | t1 | 5 | 1 | 72.00 ms [71.39 ms, 73.77 ms] | 72.00 ms [71.39 ms, 73.77 ms] | 444.5 [440.7, 455.3] | 0.974 [0.946, 0.992] | same |
| setup_tet4 | init_implicit | t2 | 5 | 1 | 72.80 ms [71.27 ms, 77.01 ms] | 72.80 ms [71.27 ms, 77.01 ms] | 449.4 [439.9, 475.4] | 0.973 [0.946, 1.060] | same |
| setup_tet4 | init_implicit | t4 | 5 | 1 | 73.55 ms [72.35 ms, 75.65 ms] | 73.55 ms [72.35 ms, 75.65 ms] | 454.0 [446.6, 467.0] | 0.989 [0.961, 1.051] | same |
| setup_tet4 | init_implicit | t8 | 5 | 1 | 74.39 ms [71.06 ms, 74.63 ms] | 74.39 ms [71.06 ms, 74.63 ms] | 459.2 [438.6, 460.7] | 0.980 [0.972, 1.037] | same |
| setup_tet4 | mass_consistent | base | 5 | 3 | 615.89 ms [607.03 ms, 626.64 ms] | 205.30 ms [202.34 ms, 208.88 ms] | 1267.3 [1249.0, 1289.4] | – | same |
| setup_tet4 | mass_consistent | t1 | 5 | 3 | 371.16 ms [368.27 ms, 373.58 ms] | 123.72 ms [122.76 ms, 124.53 ms] | 763.7 [757.8, 768.7] | 0.602 [0.596, 0.607] | same |
| setup_tet4 | mass_consistent | t2 | 5 | 3 | 204.04 ms [202.32 ms, 204.73 ms] | 68.01 ms [67.44 ms, 68.24 ms] | 419.8 [416.3, 421.3] | 0.331 [0.324, 0.336] | same |
| setup_tet4 | mass_consistent | t4 | 5 | 3 | 123.50 ms [122.90 ms, 124.19 ms] | 41.17 ms [40.97 ms, 41.40 ms] | 254.1 [252.9, 255.5] | 0.200 [0.197, 0.205] | same |
| setup_tet4 | mass_consistent | t8 | 5 | 3 | 87.00 ms [86.57 ms, 87.48 ms] | 29.00 ms [28.86 ms, 29.16 ms] | 179.0 [178.1, 180.0] | 0.141 [0.139, 0.144] | same |
| setup_tet4 | stiffness_assembly | base | 5 | 3 | 1.068 s [1.064 s, 1.079 s] | 356.15 ms [354.63 ms, 359.80 ms] | 2198.5 [2189.1, 2221.0] | – | same |
| setup_tet4 | stiffness_assembly | t1 | 5 | 3 | 523.56 ms [513.17 ms, 526.52 ms] | 174.52 ms [171.06 ms, 175.51 ms] | 1077.3 [1055.9, 1083.4] | 0.486 [0.480, 0.494] | same |
| setup_tet4 | stiffness_assembly | t2 | 5 | 3 | 293.21 ms [289.57 ms, 294.95 ms] | 97.74 ms [96.52 ms, 98.32 ms] | 603.3 [595.8, 606.9] | 0.275 [0.270, 0.276] | same |
| setup_tet4 | stiffness_assembly | t4 | 5 | 3 | 175.55 ms [175.04 ms, 176.52 ms] | 58.52 ms [58.35 ms, 58.84 ms] | 361.2 [360.2, 363.2] | 0.165 [0.163, 0.165] | same |
| setup_tet4 | stiffness_assembly | t8 | 5 | 3 | 116.11 ms [115.61 ms, 117.24 ms] | 38.70 ms [38.54 ms, 39.08 ms] | 238.9 [237.9, 241.2] | 0.109 [0.108, 0.110] | same |
| static_linear_eigen | mesh | base | 5 | 1 | 667.9 µs [662.6 µs, 680.6 µs] | 667.9 µs [662.6 µs, 680.6 µs] | 40.6 [40.2, 41.3] | – | same |
| static_linear_eigen | mesh | t1 | 5 | 1 | 645.5 µs [626.9 µs, 657.0 µs] | 645.5 µs [626.9 µs, 657.0 µs] | 39.2 [38.1, 39.9] | 0.968 [0.939, 0.977] | same |
| static_linear_eigen | mesh | t2 | 5 | 1 | 663.2 µs [642.5 µs, 677.0 µs] | 663.2 µs [642.5 µs, 677.0 µs] | 40.3 [39.0, 41.1] | 0.988 [0.944, 1.022] | same |
| static_linear_eigen | mesh | t4 | 5 | 1 | 654.7 µs [638.1 µs, 663.5 µs] | 654.7 µs [638.1 µs, 663.5 µs] | 39.8 [38.8, 40.3] | 0.980 [0.938, 0.992] | same |
| static_linear_eigen | mesh | t8 | 5 | 1 | 663.4 µs [654.3 µs, 666.7 µs] | 663.4 µs [654.3 µs, 666.7 µs] | 40.3 [39.7, 40.5] | 0.993 [0.961, 1.001] | same |
| static_linear_eigen | init | base | 5 | 1 | 8.66 ms [8.55 ms, 8.77 ms] | 8.66 ms [8.55 ms, 8.77 ms] | 526.1 [519.3, 532.7] | – | same |
| static_linear_eigen | init | t1 | 5 | 1 | 8.46 ms [8.42 ms, 8.56 ms] | 8.46 ms [8.42 ms, 8.56 ms] | 513.6 [511.5, 519.9] | 0.976 [0.967, 0.997] | same |
| static_linear_eigen | init | t2 | 5 | 1 | 8.63 ms [8.07 ms, 8.80 ms] | 8.63 ms [8.07 ms, 8.80 ms] | 524.5 [490.1, 534.2] | 1.010 [0.920, 1.016] | same |
| static_linear_eigen | init | t4 | 5 | 1 | 8.52 ms [8.38 ms, 8.68 ms] | 8.52 ms [8.38 ms, 8.68 ms] | 517.5 [508.8, 527.0] | 0.980 [0.970, 1.002] | same |
| static_linear_eigen | init | t8 | 5 | 1 | 8.78 ms [8.72 ms, 9.00 ms] | 8.78 ms [8.72 ms, 9.00 ms] | 533.2 [529.6, 546.9] | 1.026 [0.994, 1.040] | same |
| static_linear_eigen | solve_step | base | 5 | 1 | 755.84 ms [749.74 ms, 767.93 ms] | 755.84 ms [749.74 ms, 767.93 ms] | 45908.4 [45538.1, 46643.2] | – | same |
| static_linear_eigen | solve_step | t1 | 5 | 1 | 745.01 ms [742.91 ms, 749.09 ms] | 745.01 ms [742.91 ms, 749.09 ms] | 45250.9 [45123.4, 45498.9] | 0.983 [0.975, 0.992] | same |
| static_linear_eigen | solve_step | t2 | 5 | 1 | 738.46 ms [731.84 ms, 739.35 ms] | 738.46 ms [731.84 ms, 739.35 ms] | 44853.2 [44450.7, 44907.1] | 0.978 [0.956, 0.986] | same |
| static_linear_eigen | solve_step | t4 | 5 | 1 | 723.47 ms [711.84 ms, 731.69 ms] | 723.47 ms [711.84 ms, 731.69 ms] | 43942.8 [43236.1, 44442.1] | 0.954 [0.927, 0.971] | same |
| static_linear_eigen | solve_step | t8 | 5 | 1 | 713.75 ms [704.40 ms, 722.61 ms] | 713.75 ms [704.40 ms, 722.61 ms] | 43352.3 [42784.5, 43890.4] | 0.941 [0.917, 0.964] | same |
| static_linear_eigen | predictor | base | 5 | 1 | 0.2 µs [0.1 µs, 0.3 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | t1 | 5 | 1 | 0.2 µs [0.2 µs, 0.4 µs] | 0.2 µs [0.2 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.049 [0.641, 2.711] | same |
| static_linear_eigen | predictor | t2 | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.023 [0.810, 1.336] | same |
| static_linear_eigen | predictor | t4 | 5 | 1 | 0.2 µs [0.2 µs, 0.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.771 [0.705, 1.101] | same |
| static_linear_eigen | predictor | t8 | 5 | 1 | 0.2 µs [0.2 µs, 0.3 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.067 [0.583, 1.410] | same |
| static_linear_eigen | matrix | base | 5 | 1 | 70.06 ms [68.83 ms, 71.30 ms] | 70.06 ms [68.83 ms, 71.30 ms] | 4255.2 [4180.4, 4330.4] | – | same |
| static_linear_eigen | matrix | t1 | 5 | 1 | 66.98 ms [66.63 ms, 68.36 ms] | 66.98 ms [66.63 ms, 68.36 ms] | 4068.5 [4047.3, 4152.2] | 0.966 [0.937, 0.976] | same |
| static_linear_eigen | matrix | t2 | 5 | 1 | 53.07 ms [51.94 ms, 53.42 ms] | 53.07 ms [51.94 ms, 53.42 ms] | 3223.2 [3154.8, 3244.5] | 0.761 [0.741, 0.771] | same |
| static_linear_eigen | matrix | t4 | 5 | 1 | 40.19 ms [39.99 ms, 41.21 ms] | 40.19 ms [39.99 ms, 41.21 ms] | 2441.1 [2428.8, 2503.0] | 0.574 [0.566, 0.599] | same |
| static_linear_eigen | matrix | t8 | 5 | 1 | 33.96 ms [33.63 ms, 34.55 ms] | 33.96 ms [33.63 ms, 34.55 ms] | 2062.5 [2042.4, 2098.7] | 0.486 [0.472, 0.495] | same |
| static_linear_eigen | corrector | base | 5 | 1 | 0.6 µs [0.5 µs, 0.6 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | corrector | t1 | 5 | 1 | 0.6 µs [0.4 µs, 0.8 µs] | 0.6 µs [0.4 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 0.998 [0.653, 1.385] | same |
| static_linear_eigen | corrector | t2 | 5 | 1 | 0.5 µs [0.4 µs, 0.7 µs] | 0.5 µs [0.4 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.888 [0.671, 1.193] | same |
| static_linear_eigen | corrector | t4 | 5 | 1 | 0.6 µs [0.5 µs, 0.7 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 1.002 [0.838, 1.237] | same |
| static_linear_eigen | corrector | t8 | 5 | 1 | 0.6 µs [0.4 µs, 0.6 µs] | 0.6 µs [0.4 µs, 0.6 µs] | 0.0 [0.0, 0.0] | 1.002 [0.740, 1.241] | same |
| static_linear_eigen | step_hooks | base | 5 | 2 | 262.3 µs [256.5 µs, 275.1 µs] | 131.1 µs [128.2 µs, 137.5 µs] | 8.0 [7.8, 8.4] | – | same |
| static_linear_eigen | step_hooks | t1 | 5 | 2 | 260.5 µs [257.2 µs, 277.9 µs] | 130.2 µs [128.6 µs, 138.9 µs] | 7.9 [7.8, 8.4] | 0.992 [0.935, 1.061] | same |
| static_linear_eigen | step_hooks | t2 | 5 | 2 | 267.9 µs [185.5 µs, 272.9 µs] | 134.0 µs [92.8 µs, 136.5 µs] | 8.1 [5.6, 8.3] | 1.022 [0.679, 1.064] | same |
| static_linear_eigen | step_hooks | t4 | 5 | 2 | 243.0 µs [240.9 µs, 266.7 µs] | 121.5 µs [120.5 µs, 133.4 µs] | 7.4 [7.3, 8.1] | 0.942 [0.889, 1.017] | same |
| static_linear_eigen | step_hooks | t8 | 5 | 2 | 271.4 µs [267.0 µs, 281.9 µs] | 135.7 µs [133.5 µs, 140.9 µs] | 8.2 [8.1, 8.6] | 1.032 [0.971, 1.059] | same |
| static_linear_eigen | step_rest | base | 5 | 1 | 684.88 ms [680.65 ms, 697.55 ms] | 684.88 ms [680.65 ms, 697.55 ms] | 41598.9 [41341.8, 42368.4] | – | same |
| static_linear_eigen | step_rest | t1 | 5 | 1 | 676.39 ms [675.81 ms, 682.08 ms] | 676.39 ms [675.81 ms, 682.08 ms] | 41082.8 [41047.6, 41428.5] | 0.988 [0.976, 0.996] | same |
| static_linear_eigen | step_rest | t2 | 5 | 1 | 684.78 ms [679.71 ms, 686.02 ms] | 684.78 ms [679.71 ms, 686.02 ms] | 41592.5 [41284.6, 41667.9] | 1.000 [0.975, 1.008] | same |
| static_linear_eigen | step_rest | t4 | 5 | 1 | 683.04 ms [671.58 ms, 691.29 ms] | 683.04 ms [671.58 ms, 691.29 ms] | 41486.9 [40791.1, 41987.7] | 0.993 [0.963, 1.009] | same |
| static_linear_eigen | step_rest | t8 | 5 | 1 | 678.92 ms [670.37 ms, 688.27 ms] | 678.92 ms [670.37 ms, 688.27 ms] | 41236.4 [40717.2, 41804.4] | 0.987 [0.961, 1.011] | same |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 77.74 ms [75.59 ms, 78.18 ms] | 25.91 ms [25.20 ms, 26.06 ms] | 1573.9 [1530.3, 1582.8] | – | same |
| static_linear_eigen | stiffness_assembly | t1 | 5 | 3 | 41.37 ms [39.84 ms, 42.64 ms] | 13.79 ms [13.28 ms, 14.21 ms] | 837.6 [806.7, 863.3] | 0.535 [0.510, 0.546] | same |
| static_linear_eigen | stiffness_assembly | t2 | 5 | 3 | 24.42 ms [22.06 ms, 24.63 ms] | 8.14 ms [7.35 ms, 8.21 ms] | 494.4 [446.7, 498.6] | 0.314 [0.283, 0.317] | same |
| static_linear_eigen | stiffness_assembly | t4 | 5 | 3 | 13.95 ms [13.73 ms, 14.39 ms] | 4.65 ms [4.58 ms, 4.80 ms] | 282.4 [277.9, 291.4] | 0.179 [0.176, 0.186] | same |
| static_linear_eigen | stiffness_assembly | t8 | 5 | 3 | 9.10 ms [9.05 ms, 9.21 ms] | 3.03 ms [3.02 ms, 3.07 ms] | 184.1 [183.3, 186.5] | 0.117 [0.117, 0.120] | same |
| static_linear_eigen | analysis | base | 5 | 1 | 49.66 ms [48.94 ms, 50.40 ms] | 49.66 ms [48.94 ms, 50.40 ms] | 3016.3 [2972.3, 3061.4] | – | same |
| static_linear_eigen | analysis | t1 | 5 | 1 | 48.92 ms [47.05 ms, 49.63 ms] | 48.92 ms [47.05 ms, 49.63 ms] | 2971.2 [2857.9, 3014.4] | 0.987 [0.947, 0.991] | same |
| static_linear_eigen | analysis | t2 | 5 | 1 | 48.74 ms [45.26 ms, 49.58 ms] | 48.74 ms [45.26 ms, 49.58 ms] | 2960.5 [2748.9, 3011.2] | 0.967 [0.917, 1.000] | same |
| static_linear_eigen | analysis | t4 | 5 | 1 | 51.06 ms [50.38 ms, 51.77 ms] | 51.06 ms [50.38 ms, 51.77 ms] | 3101.3 [3059.8, 3144.4] | 1.024 [1.013, 1.048] | same |
| static_linear_eigen | analysis | t8 | 5 | 1 | 51.29 ms [51.01 ms, 52.33 ms] | 51.29 ms [51.01 ms, 52.33 ms] | 3115.2 [3098.5, 3178.6] | 1.043 [1.018, 1.053] | same |
| static_linear_eigen | factorize | base | 5 | 2 | 1.084 s [1.078 s, 1.091 s] | 541.98 ms [538.84 ms, 545.73 ms] | 32919.0 [32728.4, 33146.9] | – | same |
| static_linear_eigen | factorize | t1 | 5 | 2 | 1.083 s [1.074 s, 1.091 s] | 541.26 ms [536.93 ms, 545.63 ms] | 32875.4 [32612.1, 33140.5] | 1.000 [0.991, 1.009] | same |
| static_linear_eigen | factorize | t2 | 5 | 2 | 1.081 s [1.080 s, 1.084 s] | 540.33 ms [539.89 ms, 542.03 ms] | 32818.9 [32792.0, 32922.4] | 0.996 [0.990, 1.004] | same |
| static_linear_eigen | factorize | t4 | 5 | 2 | 1.087 s [1.078 s, 1.100 s] | 543.34 ms [539.14 ms, 550.11 ms] | 33001.4 [32746.8, 33413.0] | 0.999 [0.997, 1.020] | same |
| static_linear_eigen | factorize | t8 | 5 | 2 | 1.084 s [1.073 s, 1.088 s] | 542.24 ms [536.74 ms, 544.19 ms] | 32935.1 [32600.7, 33053.5] | 0.995 [0.994, 1.009] | same |
| static_linear_eigen | back_substitution | base | 5 | 20 | 96.68 ms [94.50 ms, 100.56 ms] | 4.83 ms [4.72 ms, 5.03 ms] | 293.6 [287.0, 305.4] | – | same |
| static_linear_eigen | back_substitution | t1 | 5 | 20 | 95.87 ms [94.09 ms, 97.79 ms] | 4.79 ms [4.70 ms, 4.89 ms] | 291.1 [285.7, 297.0] | 0.973 [0.942, 1.016] | same |
| static_linear_eigen | back_substitution | t2 | 5 | 20 | 96.28 ms [93.36 ms, 97.71 ms] | 4.81 ms [4.67 ms, 4.89 ms] | 292.4 [283.5, 296.7] | 0.972 [0.963, 1.019] | same |
| static_linear_eigen | back_substitution | t4 | 5 | 20 | 94.22 ms [92.80 ms, 96.39 ms] | 4.71 ms [4.64 ms, 4.82 ms] | 286.1 [281.8, 292.7] | 0.960 [0.949, 0.996] | same |
| static_linear_eigen | back_substitution | t8 | 5 | 20 | 95.30 ms [93.28 ms, 97.87 ms] | 4.76 ms [4.66 ms, 4.89 ms] | 289.4 [283.3, 297.2] | 0.985 [0.939, 1.009] | same |
| static_newton_plastic | mesh | base | 5 | 1 | 273.2 µs [259.9 µs, 280.1 µs] | 273.2 µs [259.9 µs, 280.1 µs] | 45.5 [43.3, 46.7] | – | same |
| static_newton_plastic | mesh | t1 | 5 | 1 | 265.8 µs [262.0 µs, 283.2 µs] | 265.8 µs [262.0 µs, 283.2 µs] | 44.3 [43.7, 47.2] | 0.973 [0.937, 1.040] | same |
| static_newton_plastic | mesh | t2 | 5 | 1 | 262.6 µs [261.4 µs, 274.2 µs] | 262.6 µs [261.4 µs, 274.2 µs] | 43.8 [43.6, 45.7] | 0.958 [0.934, 1.015] | same |
| static_newton_plastic | mesh | t4 | 5 | 1 | 273.8 µs [261.0 µs, 279.9 µs] | 273.8 µs [261.0 µs, 279.9 µs] | 45.6 [43.5, 46.6] | 0.996 [0.932, 1.054] | same |
| static_newton_plastic | mesh | t8 | 5 | 1 | 262.9 µs [260.2 µs, 286.5 µs] | 262.9 µs [260.2 µs, 286.5 µs] | 43.8 [43.4, 47.8] | 0.968 [0.938, 1.049] | same |
| static_newton_plastic | init | base | 5 | 1 | 3.85 ms [3.75 ms, 3.88 ms] | 3.85 ms [3.75 ms, 3.88 ms] | 641.6 [624.9, 646.5] | – | same |
| static_newton_plastic | init | t1 | 5 | 1 | 3.48 ms [3.37 ms, 3.72 ms] | 3.48 ms [3.37 ms, 3.72 ms] | 580.5 [561.6, 619.7] | 0.905 [0.871, 0.987] | same |
| static_newton_plastic | init | t2 | 5 | 1 | 3.84 ms [3.79 ms, 4.12 ms] | 3.84 ms [3.79 ms, 4.12 ms] | 639.4 [632.5, 686.3] | 1.017 [0.978, 1.070] | same |
| static_newton_plastic | init | t4 | 5 | 1 | 3.95 ms [3.91 ms, 4.13 ms] | 3.95 ms [3.91 ms, 4.13 ms] | 658.6 [651.4, 688.5] | 1.029 [1.015, 1.102] | same |
| static_newton_plastic | init | t8 | 5 | 1 | 4.08 ms [4.01 ms, 4.24 ms] | 4.08 ms [4.01 ms, 4.24 ms] | 679.9 [669.1, 706.7] | 1.071 [1.060, 1.093] | same |
| static_newton_plastic | solve_step | base | 5 | 12 | 1.974 s [1.961 s, 1.979 s] | 164.51 ms [163.44 ms, 164.88 ms] | 27419.1 [27239.9, 27480.2] | – | same |
| static_newton_plastic | solve_step | t1 | 5 | 12 | 1.935 s [1.929 s, 1.952 s] | 161.26 ms [160.79 ms, 162.71 ms] | 26876.6 [26798.2, 27117.9] | 0.984 [0.976, 0.987] | same |
| static_newton_plastic | solve_step | t2 | 5 | 12 | 1.836 s [1.817 s, 1.838 s] | 152.97 ms [151.38 ms, 153.19 ms] | 25495.3 [25230.8, 25531.8] | 0.929 [0.923, 0.935] | same |
| static_newton_plastic | solve_step | t4 | 5 | 12 | 1.827 s [1.818 s, 1.828 s] | 152.26 ms [151.46 ms, 152.36 ms] | 25376.8 [25243.4, 25392.7] | 0.924 [0.923, 0.928] | same |
| static_newton_plastic | solve_step | t8 | 5 | 12 | 1.822 s [1.820 s, 1.825 s] | 151.85 ms [151.65 ms, 152.10 ms] | 25308.3 [25274.8, 25349.6] | 0.923 [0.920, 0.931] | same |
| static_newton_plastic | predictor | base | 5 | 12 | 1.6 µs [1.4 µs, 1.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | predictor | t1 | 5 | 12 | 1.6 µs [1.4 µs, 1.6 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.010 [0.842, 1.111] | same |
| static_newton_plastic | predictor | t2 | 5 | 12 | 1.6 µs [1.3 µs, 1.8 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.004 [0.815, 1.134] | same |
| static_newton_plastic | predictor | t4 | 5 | 12 | 1.5 µs [1.2 µs, 1.5 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.964 [0.723, 1.068] | same |
| static_newton_plastic | predictor | t8 | 5 | 12 | 1.6 µs [1.6 µs, 1.7 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.051 [0.977, 1.147] | same |
| static_newton_plastic | residual | base | 5 | 961 | 794.35 ms [790.43 ms, 796.83 ms] | 826.6 µs [822.5 µs, 829.2 µs] | 137.8 [137.1, 138.2] | – | same |
| static_newton_plastic | residual | t1 | 5 | 961 | 788.98 ms [785.12 ms, 795.45 ms] | 821.0 µs [817.0 µs, 827.7 µs] | 136.8 [136.2, 138.0] | 0.995 [0.988, 1.000] | same |
| static_newton_plastic | residual | t2 | 5 | 961 | 676.01 ms [667.17 ms, 678.14 ms] | 703.4 µs [694.2 µs, 705.7 µs] | 117.2 [115.7, 117.6] | 0.850 [0.844, 0.851] | same |
| static_newton_plastic | residual | t4 | 5 | 961 | 659.84 ms [659.13 ms, 660.91 ms] | 686.6 µs [685.9 µs, 687.7 µs] | 114.4 [114.3, 114.6] | 0.831 [0.828, 0.834] | same |
| static_newton_plastic | residual | t8 | 5 | 961 | 650.98 ms [649.05 ms, 651.45 ms] | 677.4 µs [675.4 µs, 677.9 µs] | 112.9 [112.6, 113.0] | 0.819 [0.816, 0.824] | same |
| static_newton_plastic | matrix | base | 5 | 949 | 565.8 µs [528.9 µs, 594.2 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | – | same |
| static_newton_plastic | matrix | t1 | 5 | 949 | 566.4 µs [560.7 µs, 609.6 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 1.026 [0.996, 1.062] | same |
| static_newton_plastic | matrix | t2 | 5 | 949 | 564.2 µs [554.9 µs, 574.5 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.997 [0.941, 1.080] | same |
| static_newton_plastic | matrix | t4 | 5 | 949 | 537.0 µs [487.3 µs, 571.9 µs] | 0.6 µs [0.5 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.962 [0.861, 1.015] | same |
| static_newton_plastic | matrix | t8 | 5 | 949 | 563.3 µs [522.8 µs, 577.9 µs] | 0.6 µs [0.6 µs, 0.6 µs] | 0.1 [0.1, 0.1] | 0.973 [0.924, 1.078] | same |
| static_newton_plastic | corrector | base | 5 | 949 | 141.9 µs [136.4 µs, 148.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_newton_plastic | corrector | t1 | 5 | 949 | 193.2 µs [173.2 µs, 201.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.370 [1.169, 1.480] | same |
| static_newton_plastic | corrector | t2 | 5 | 949 | 191.4 µs [179.3 µs, 195.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.320 [1.263, 1.432] | same |
| static_newton_plastic | corrector | t4 | 5 | 949 | 175.8 µs [166.4 µs, 196.3 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.289 [1.134, 1.395] | same |
| static_newton_plastic | corrector | t8 | 5 | 949 | 195.8 µs [181.0 µs, 206.9 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.333 [1.312, 1.471] | same |
| static_newton_plastic | step_hooks | base | 5 | 24 | 1.51 ms [1.49 ms, 1.53 ms] | 62.9 µs [62.0 µs, 63.7 µs] | 10.5 [10.3, 10.6] | – | same |
| static_newton_plastic | step_hooks | t1 | 5 | 24 | 1.52 ms [1.49 ms, 1.54 ms] | 63.2 µs [62.0 µs, 64.2 µs] | 10.5 [10.3, 10.7] | 1.005 [0.987, 1.036] | same |
| static_newton_plastic | step_hooks | t2 | 5 | 24 | 1.52 ms [1.50 ms, 1.54 ms] | 63.2 µs [62.5 µs, 64.3 µs] | 10.5 [10.4, 10.7] | 1.001 [0.984, 1.036] | same |
| static_newton_plastic | step_hooks | t4 | 5 | 24 | 1.50 ms [1.49 ms, 1.51 ms] | 62.6 µs [62.2 µs, 62.9 µs] | 10.4 [10.4, 10.5] | 1.000 [0.976, 1.014] | same |
| static_newton_plastic | step_hooks | t8 | 5 | 24 | 1.48 ms [1.47 ms, 1.49 ms] | 61.6 µs [61.2 µs, 62.0 µs] | 10.3 [10.2, 10.3] | 0.979 [0.962, 0.990] | same |
| static_newton_plastic | step_rest | base | 5 | 12 | 1.175 s [1.167 s, 1.181 s] | 97.95 ms [97.21 ms, 98.42 ms] | 16325.8 [16201.9, 16402.7] | – | same |
| static_newton_plastic | step_rest | t1 | 5 | 12 | 1.144 s [1.141 s, 1.155 s] | 95.36 ms [95.09 ms, 96.23 ms] | 15892.5 [15847.6, 16038.1] | 0.978 [0.967, 0.981] | same |
| static_newton_plastic | step_rest | t2 | 5 | 12 | 1.157 s [1.147 s, 1.160 s] | 96.42 ms [95.60 ms, 96.67 ms] | 16070.4 [15933.2, 16111.0] | 0.983 [0.976, 0.992] | same |
| static_newton_plastic | step_rest | t4 | 5 | 12 | 1.165 s [1.156 s, 1.165 s] | 97.09 ms [96.35 ms, 97.10 ms] | 16182.1 [16057.8, 16183.3] | 0.987 [0.984, 0.994] | same |
| static_newton_plastic | step_rest | t8 | 5 | 12 | 1.170 s [1.168 s, 1.171 s] | 97.46 ms [97.37 ms, 97.62 ms] | 16243.4 [16228.8, 16270.3] | 0.995 [0.989, 1.004] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 105ea0eb1, 72337cb4e | 88b89af4f4608998 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 105ea0eb1, 72337cb4e | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 105ea0eb1, 72337cb4e | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 105ea0eb1, 72337cb4e | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 105ea0eb1, 72337cb4e | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 105ea0eb1, 72337cb4e | d5869f57a777d9a5 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 105ea0eb1, 72337cb4e | 7bd5f114957d7d7b |
