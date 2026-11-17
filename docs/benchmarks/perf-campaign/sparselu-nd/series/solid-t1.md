Suite `solid`, size `m`; variants: `auto`, `base`, `colamd`, `nested_dissection`; reference `colamd`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to colamd | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | mesh | auto | 5 | 1 | 463.3 µs [446.8 µs, 484.0 µs] | 463.3 µs [446.8 µs, 484.0 µs] | 128.7 [124.1, 134.5] | 0.992 [0.966, 1.078] | DIFFER (2) |
| cohesive_intrinsic_implicit | mesh | base | 5 | 1 | 468.2 µs [447.0 µs, 482.0 µs] | 468.2 µs [447.0 µs, 482.0 µs] | 130.1 [124.2, 133.9] | 1.005 [0.992, 1.060] | DIFFER (2) |
| cohesive_intrinsic_implicit | mesh | colamd | 5 | 1 | 451.3 µs [449.1 µs, 479.6 µs] | 451.3 µs [449.1 µs, 479.6 µs] | 125.4 [124.7, 133.2] | – | DIFFER (2) |
| cohesive_intrinsic_implicit | mesh | nested_dissection | 5 | 1 | 449.3 µs [443.4 µs, 475.9 µs] | 449.3 µs [443.4 µs, 475.9 µs] | 124.8 [123.2, 132.2] | 0.992 [0.981, 0.998] | DIFFER (2) |
| cohesive_intrinsic_implicit | init | auto | 5 | 1 | 10.83 ms [10.74 ms, 10.95 ms] | 10.83 ms [10.74 ms, 10.95 ms] | 3008.9 [2983.6, 3042.0] | 1.000 [0.988, 1.011] | DIFFER (2) |
| cohesive_intrinsic_implicit | init | base | 5 | 1 | 10.84 ms [10.81 ms, 10.91 ms] | 10.84 ms [10.81 ms, 10.91 ms] | 3010.7 [3001.6, 3030.2] | 0.998 [0.989, 1.007] | DIFFER (2) |
| cohesive_intrinsic_implicit | init | colamd | 5 | 1 | 10.86 ms [10.74 ms, 10.93 ms] | 10.86 ms [10.74 ms, 10.93 ms] | 3016.2 [2982.3, 3035.0] | – | DIFFER (2) |
| cohesive_intrinsic_implicit | init | nested_dissection | 5 | 1 | 10.81 ms [10.71 ms, 10.85 ms] | 10.81 ms [10.71 ms, 10.85 ms] | 3002.4 [2975.9, 3012.5] | 0.998 [0.988, 0.999] | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | auto | 5 | 30 | 1.967 s [1.963 s, 1.969 s] | 65.56 ms [65.43 ms, 65.65 ms] | 18212.1 [18174.2, 18236.1] | 0.656 [0.654, 0.657] | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | base | 5 | 30 | 2.998 s [2.992 s, 3.005 s] | 99.94 ms [99.75 ms, 100.16 ms] | 27761.9 [27707.4, 27821.7] | 1.000 [0.996, 1.003] | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | colamd | 5 | 30 | 2.999 s [2.993 s, 3.007 s] | 99.96 ms [99.76 ms, 100.24 ms] | 27767.0 [27711.7, 27844.5] | – | DIFFER (2) |
| cohesive_intrinsic_implicit | solve_step | nested_dissection | 5 | 30 | 1.930 s [1.928 s, 1.931 s] | 64.34 ms [64.28 ms, 64.37 ms] | 17871.0 [17856.2, 17880.3] | 0.644 [0.642, 0.645] | DIFFER (2) |
| implicit_dynamic | mesh | auto | 5 | 1 | 803.3 µs [798.4 µs, 808.2 µs] | 803.3 µs [798.4 µs, 808.2 µs] | 48.8 [48.5, 49.1] | 1.001 [0.985, 1.013] | DIFFER (2) |
| implicit_dynamic | mesh | base | 5 | 1 | 805.0 µs [786.9 µs, 824.2 µs] | 805.0 µs [786.9 µs, 824.2 µs] | 48.9 [47.8, 50.1] | 1.009 [0.983, 1.030] | DIFFER (2) |
| implicit_dynamic | mesh | colamd | 5 | 1 | 800.2 µs [793.7 µs, 815.9 µs] | 800.2 µs [793.7 µs, 815.9 µs] | 48.6 [48.2, 49.6] | – | DIFFER (2) |
| implicit_dynamic | mesh | nested_dissection | 5 | 1 | 800.1 µs [780.7 µs, 805.0 µs] | 800.1 µs [780.7 µs, 805.0 µs] | 48.6 [47.4, 48.9] | 0.995 [0.976, 1.014] | DIFFER (2) |
| implicit_dynamic | init | auto | 5 | 1 | 9.29 ms [9.26 ms, 9.35 ms] | 9.29 ms [9.26 ms, 9.35 ms] | 564.5 [562.5, 568.2] | 0.992 [0.988, 1.004] | DIFFER (2) |
| implicit_dynamic | init | base | 5 | 1 | 9.30 ms [9.28 ms, 9.32 ms] | 9.30 ms [9.28 ms, 9.32 ms] | 564.9 [563.7, 565.9] | 0.997 [0.989, 1.000] | DIFFER (2) |
| implicit_dynamic | init | colamd | 5 | 1 | 9.35 ms [9.28 ms, 9.40 ms] | 9.35 ms [9.28 ms, 9.40 ms] | 567.6 [563.9, 571.2] | – | DIFFER (2) |
| implicit_dynamic | init | nested_dissection | 5 | 1 | 9.29 ms [9.29 ms, 9.33 ms] | 9.29 ms [9.29 ms, 9.33 ms] | 564.5 [564.0, 566.5] | 0.998 [0.987, 1.001] | DIFFER (2) |
| implicit_dynamic | mass_consistent | auto | 5 | 1 | 63.11 ms [62.87 ms, 63.41 ms] | 63.11 ms [62.87 ms, 63.41 ms] | 3833.0 [3818.6, 3851.5] | 0.996 [0.995, 1.007] | DIFFER (2) |
| implicit_dynamic | mass_consistent | base | 5 | 1 | 62.67 ms [62.46 ms, 63.87 ms] | 62.67 ms [62.46 ms, 63.87 ms] | 3806.2 [3794.0, 3879.5] | 0.991 [0.986, 1.008] | DIFFER (2) |
| implicit_dynamic | mass_consistent | colamd | 5 | 1 | 63.34 ms [62.99 ms, 63.45 ms] | 63.34 ms [62.99 ms, 63.45 ms] | 3847.0 [3826.2, 3853.6] | – | DIFFER (2) |
| implicit_dynamic | mass_consistent | nested_dissection | 5 | 1 | 63.04 ms [62.80 ms, 63.81 ms] | 63.04 ms [62.80 ms, 63.81 ms] | 3829.1 [3814.2, 3875.6] | 0.995 [0.990, 1.013] | DIFFER (2) |
| implicit_dynamic | solve_step | auto | 5 | 60 | 728.59 ms [724.88 ms, 729.97 ms] | 12.14 ms [12.08 ms, 12.17 ms] | 737.6 [733.8, 739.0] | 0.718 [0.714, 0.722] | DIFFER (2) |
| implicit_dynamic | solve_step | base | 5 | 60 | 1.017 s [1.015 s, 1.018 s] | 16.95 ms [16.91 ms, 16.97 ms] | 1029.6 [1027.2, 1030.6] | 1.002 [0.999, 1.006] | DIFFER (2) |
| implicit_dynamic | solve_step | colamd | 5 | 60 | 1.014 s [1.011 s, 1.015 s] | 16.90 ms [16.85 ms, 16.92 ms] | 1026.4 [1023.5, 1027.8] | – | DIFFER (2) |
| implicit_dynamic | solve_step | nested_dissection | 5 | 60 | 727.95 ms [724.61 ms, 729.16 ms] | 12.13 ms [12.08 ms, 12.15 ms] | 736.9 [733.5, 738.1] | 0.718 [0.715, 0.720] | DIFFER (2) |
| implicit_dynamic | predictor | auto | 5 | 60 | 4.5 µs [4.0 µs, 5.0 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.132 [0.948, 1.495] | DIFFER (2) |
| implicit_dynamic | predictor | base | 5 | 60 | 2.4 µs [2.4 µs, 3.4 µs] | 0.0 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.629 [0.469, 1.168] | DIFFER (2) |
| implicit_dynamic | predictor | colamd | 5 | 60 | 4.3 µs [2.9 µs, 5.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| implicit_dynamic | predictor | nested_dissection | 5 | 60 | 4.7 µs [3.9 µs, 6.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.385 [0.778, 1.935] | DIFFER (2) |
| implicit_dynamic | residual | auto | 5 | 180 | 143.43 ms [143.01 ms, 144.10 ms] | 796.9 µs [794.5 µs, 800.6 µs] | 48.4 [48.3, 48.6] | 0.997 [0.993, 1.006] | DIFFER (2) |
| implicit_dynamic | residual | base | 5 | 180 | 144.01 ms [143.74 ms, 144.77 ms] | 800.0 µs [798.5 µs, 804.3 µs] | 48.6 [48.5, 48.8] | 1.001 [0.997, 1.013] | DIFFER (2) |
| implicit_dynamic | residual | colamd | 5 | 180 | 144.03 ms [142.87 ms, 144.53 ms] | 800.2 µs [793.7 µs, 803.0 µs] | 48.6 [48.2, 48.8] | – | DIFFER (2) |
| implicit_dynamic | residual | nested_dissection | 5 | 180 | 143.26 ms [142.05 ms, 143.94 ms] | 795.9 µs [789.2 µs, 799.7 µs] | 48.3 [47.9, 48.6] | 0.993 [0.989, 1.003] | DIFFER (2) |
| implicit_dynamic | matrix | auto | 5 | 420 | 86.3 µs [78.6 µs, 91.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.953 [0.752, 0.993] | DIFFER (2) |
| implicit_dynamic | matrix | base | 5 | 420 | 94.1 µs [93.0 µs, 104.4 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.997 [0.952, 1.111] | DIFFER (2) |
| implicit_dynamic | matrix | colamd | 5 | 420 | 94.3 µs [86.9 µs, 105.5 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| implicit_dynamic | matrix | nested_dissection | 5 | 420 | 90.5 µs [83.7 µs, 97.2 µs] | 0.2 µs [0.2 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.960 [0.887, 1.026] | DIFFER (2) |
| implicit_dynamic | corrector | auto | 5 | 120 | 13.4 µs [10.7 µs, 18.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.903 [0.640, 1.635] | DIFFER (2) |
| implicit_dynamic | corrector | base | 5 | 120 | 15.4 µs [11.6 µs, 18.5 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.100 [0.599, 1.384] | DIFFER (2) |
| implicit_dynamic | corrector | colamd | 5 | 120 | 14.3 µs [11.1 µs, 19.3 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| implicit_dynamic | corrector | nested_dissection | 5 | 120 | 13.3 µs [11.9 µs, 16.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.999 [0.615, 1.337] | DIFFER (2) |
| implicit_dynamic | step_hooks | auto | 5 | 120 | 191.4 µs [182.8 µs, 197.9 µs] | 1.6 µs [1.5 µs, 1.6 µs] | 0.1 [0.1, 0.1] | 1.018 [0.955, 1.061] | DIFFER (2) |
| implicit_dynamic | step_hooks | base | 5 | 120 | 178.3 µs [160.9 µs, 197.7 µs] | 1.5 µs [1.3 µs, 1.6 µs] | 0.1 [0.1, 0.1] | 0.966 [0.852, 1.033] | DIFFER (2) |
| implicit_dynamic | step_hooks | colamd | 5 | 120 | 189.7 µs [180.3 µs, 191.4 µs] | 1.6 µs [1.5 µs, 1.6 µs] | 0.1 [0.1, 0.1] | – | DIFFER (2) |
| implicit_dynamic | step_hooks | nested_dissection | 5 | 120 | 184.1 µs [177.1 µs, 189.1 µs] | 1.5 µs [1.5 µs, 1.6 µs] | 0.1 [0.1, 0.1] | 0.982 [0.925, 1.021] | DIFFER (2) |
| implicit_dynamic | step_rest | auto | 5 | 60 | 584.52 ms [581.22 ms, 585.98 ms] | 9.74 ms [9.69 ms, 9.77 ms] | 591.7 [588.4, 593.2] | 0.672 [0.668, 0.675] | DIFFER (2) |
| implicit_dynamic | step_rest | base | 5 | 60 | 872.11 ms [870.37 ms, 874.09 ms] | 14.54 ms [14.51 ms, 14.57 ms] | 882.8 [881.1, 884.8] | 1.003 [1.000, 1.007] | DIFFER (2) |
| implicit_dynamic | step_rest | colamd | 5 | 60 | 869.72 ms [867.92 ms, 870.51 ms] | 14.50 ms [14.47 ms, 14.51 ms] | 880.4 [878.6, 881.2] | – | DIFFER (2) |
| implicit_dynamic | step_rest | nested_dissection | 5 | 60 | 584.56 ms [582.26 ms, 584.91 ms] | 9.74 ms [9.70 ms, 9.75 ms] | 591.8 [589.4, 592.1] | 0.672 [0.669, 0.674] | DIFFER (2) |
| static_linear_eigen | mesh | auto | 5 | 1 | 805.4 µs [791.2 µs, 813.9 µs] | 805.4 µs [791.2 µs, 813.9 µs] | 48.9 [48.1, 49.4] | 1.017 [1.007, 1.035] | DIFFER (2) |
| static_linear_eigen | mesh | base | 5 | 1 | 808.1 µs [789.1 µs, 814.0 µs] | 808.1 µs [789.1 µs, 814.0 µs] | 49.1 [47.9, 49.4] | 1.016 [0.987, 1.047] | DIFFER (2) |
| static_linear_eigen | mesh | colamd | 5 | 1 | 794.1 µs [773.1 µs, 799.8 µs] | 794.1 µs [773.1 µs, 799.8 µs] | 48.2 [47.0, 48.6] | – | DIFFER (2) |
| static_linear_eigen | mesh | nested_dissection | 5 | 1 | 798.5 µs [779.0 µs, 805.4 µs] | 798.5 µs [779.0 µs, 805.4 µs] | 48.5 [47.3, 48.9] | 1.001 [0.984, 1.035] | DIFFER (2) |
| static_linear_eigen | init | auto | 5 | 1 | 9.23 ms [9.19 ms, 9.56 ms] | 9.23 ms [9.19 ms, 9.56 ms] | 560.5 [557.9, 580.5] | 0.998 [0.993, 1.045] | DIFFER (2) |
| static_linear_eigen | init | base | 5 | 1 | 9.24 ms [9.12 ms, 9.54 ms] | 9.24 ms [9.12 ms, 9.54 ms] | 561.0 [553.9, 579.7] | 0.999 [0.986, 1.044] | DIFFER (2) |
| static_linear_eigen | init | colamd | 5 | 1 | 9.25 ms [9.14 ms, 9.26 ms] | 9.25 ms [9.14 ms, 9.26 ms] | 561.8 [555.2, 562.3] | – | DIFFER (2) |
| static_linear_eigen | init | nested_dissection | 5 | 1 | 9.24 ms [9.11 ms, 9.41 ms] | 9.24 ms [9.11 ms, 9.41 ms] | 561.2 [553.5, 571.3] | 1.001 [0.985, 1.029] | DIFFER (2) |
| static_linear_eigen | solve_step | auto | 5 | 1 | 461.04 ms [458.35 ms, 461.73 ms] | 461.04 ms [458.35 ms, 461.73 ms] | 28003.0 [27839.5, 28044.8] | 0.655 [0.646, 0.660] | DIFFER (2) |
| static_linear_eigen | solve_step | base | 5 | 1 | 702.87 ms [701.78 ms, 719.25 ms] | 702.87 ms [701.78 ms, 719.25 ms] | 42691.1 [42625.0, 43686.4] | 1.001 [0.991, 1.013] | DIFFER (2) |
| static_linear_eigen | solve_step | colamd | 5 | 1 | 704.25 ms [699.18 ms, 709.91 ms] | 704.25 ms [699.18 ms, 709.91 ms] | 42775.1 [42467.0, 43118.6] | – | DIFFER (2) |
| static_linear_eigen | solve_step | nested_dissection | 5 | 1 | 396.38 ms [394.74 ms, 400.73 ms] | 396.38 ms [394.74 ms, 400.73 ms] | 24075.7 [23976.1, 24339.9] | 0.565 [0.556, 0.570] | DIFFER (2) |
| static_linear_eigen | predictor | auto | 5 | 1 | 0.2 µs [0.1 µs, 0.2 µs] | 0.2 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.818 [0.005, 2.000] | DIFFER (2) |
| static_linear_eigen | predictor | base | 5 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 0.800 [0.006, 1.545] | DIFFER (2) |
| static_linear_eigen | predictor | colamd | 5 | 1 | 0.1 µs [0.1 µs, 38.0 µs] | 0.1 µs [0.1 µs, 38.0 µs] | 0.0 [0.0, 2.3] | – | DIFFER (2) |
| static_linear_eigen | predictor | nested_dissection | 5 | 1 | 0.2 µs [0.1 µs, 0.3 µs] | 0.2 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.000 [0.009, 2.000] | DIFFER (2) |
| static_linear_eigen | matrix | auto | 5 | 1 | 67.80 ms [67.57 ms, 68.66 ms] | 67.80 ms [67.57 ms, 68.66 ms] | 4118.1 [4104.1, 4170.2] | 0.999 [0.991, 1.007] | DIFFER (2) |
| static_linear_eigen | matrix | base | 5 | 1 | 67.74 ms [67.45 ms, 68.99 ms] | 67.74 ms [67.45 ms, 68.99 ms] | 4114.2 [4096.6, 4190.1] | 0.997 [0.991, 1.012] | DIFFER (2) |
| static_linear_eigen | matrix | colamd | 5 | 1 | 68.19 ms [67.65 ms, 68.33 ms] | 68.19 ms [67.65 ms, 68.33 ms] | 4141.8 [4108.8, 4150.5] | – | DIFFER (2) |
| static_linear_eigen | matrix | nested_dissection | 5 | 1 | 67.95 ms [67.68 ms, 68.47 ms] | 67.95 ms [67.68 ms, 68.47 ms] | 4127.1 [4110.9, 4158.6] | 0.996 [0.993, 1.012] | DIFFER (2) |
| static_linear_eigen | corrector | auto | 5 | 1 | 0.7 µs [0.6 µs, 0.9 µs] | 0.7 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 0.930 [0.772, 1.114] | DIFFER (2) |
| static_linear_eigen | corrector | base | 5 | 1 | 0.8 µs [0.6 µs, 1.0 µs] | 0.8 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | 0.887 [0.878, 1.408] | DIFFER (2) |
| static_linear_eigen | corrector | colamd | 5 | 1 | 0.8 µs [0.7 µs, 0.9 µs] | 0.8 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | DIFFER (2) |
| static_linear_eigen | corrector | nested_dissection | 5 | 1 | 0.6 µs [0.6 µs, 0.7 µs] | 0.6 µs [0.6 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.797 [0.700, 0.972] | DIFFER (2) |
| static_linear_eigen | step_hooks | auto | 5 | 2 | 5.0 µs [4.5 µs, 5.1 µs] | 2.5 µs [2.2 µs, 2.6 µs] | 0.2 [0.1, 0.2] | 0.955 [0.517, 1.152] | DIFFER (2) |
| static_linear_eigen | step_hooks | base | 5 | 2 | 4.2 µs [4.0 µs, 5.3 µs] | 2.1 µs [2.0 µs, 2.6 µs] | 0.1 [0.1, 0.2] | 0.923 [0.410, 0.998] | DIFFER (2) |
| static_linear_eigen | step_hooks | colamd | 5 | 2 | 4.8 µs [4.3 µs, 9.9 µs] | 2.4 µs [2.2 µs, 4.9 µs] | 0.1 [0.1, 0.3] | – | DIFFER (2) |
| static_linear_eigen | step_hooks | nested_dissection | 5 | 2 | 4.8 µs [4.1 µs, 5.1 µs] | 2.4 µs [2.1 µs, 2.5 µs] | 0.1 [0.1, 0.2] | 0.901 [0.514, 1.118] | DIFFER (2) |
| static_linear_eigen | step_rest | auto | 5 | 1 | 393.06 ms [390.48 ms, 393.81 ms] | 393.06 ms [390.48 ms, 393.81 ms] | 23874.2 [23717.4, 23919.2] | 0.619 [0.609, 0.624] | DIFFER (2) |
| static_linear_eigen | step_rest | base | 5 | 1 | 635.13 ms [634.32 ms, 650.26 ms] | 635.13 ms [634.32 ms, 650.26 ms] | 38576.7 [38528.0, 39496.1] | 1.002 [0.990, 1.013] | DIFFER (2) |
| static_linear_eigen | step_rest | colamd | 5 | 1 | 635.91 ms [631.31 ms, 641.70 ms] | 635.91 ms [631.31 ms, 641.70 ms] | 38624.3 [38344.9, 38976.2] | – | DIFFER (2) |
| static_linear_eigen | step_rest | nested_dissection | 5 | 1 | 327.91 ms [326.89 ms, 332.78 ms] | 327.91 ms [326.89 ms, 332.78 ms] | 19916.8 [19854.7, 20212.5] | 0.518 [0.510, 0.524] | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | auto | 5 | 3 | 31.78 ms [31.67 ms, 31.94 ms] | 10.59 ms [10.56 ms, 10.65 ms] | 643.4 [641.3, 646.7] | 0.972 [0.970, 0.977] | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 32.09 ms [31.98 ms, 32.25 ms] | 10.70 ms [10.66 ms, 10.75 ms] | 649.7 [647.4, 652.9] | 0.981 [0.979, 0.986] | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | colamd | 5 | 3 | 32.63 ms [32.62 ms, 32.94 ms] | 10.88 ms [10.87 ms, 10.98 ms] | 660.6 [660.4, 666.8] | – | DIFFER (2) |
| static_linear_eigen | stiffness_assembly | nested_dissection | 5 | 3 | 31.84 ms [31.73 ms, 32.12 ms] | 10.61 ms [10.58 ms, 10.71 ms] | 644.7 [642.5, 650.4] | 0.973 [0.973, 0.984] | DIFFER (2) |
| static_linear_eigen | analysis | auto | 5 | 1 | 105.82 ms [105.72 ms, 105.94 ms] | 105.82 ms [105.72 ms, 105.94 ms] | 6427.1 [6421.4, 6434.5] | 2.138 [2.137, 2.142] | DIFFER (2) |
| static_linear_eigen | analysis | base | 5 | 1 | 50.14 ms [49.93 ms, 50.67 ms] | 50.14 ms [49.93 ms, 50.67 ms] | 3045.3 [3032.9, 3077.3] | 1.014 [1.010, 1.023] | DIFFER (2) |
| static_linear_eigen | analysis | colamd | 5 | 1 | 49.47 ms [49.46 ms, 49.53 ms] | 49.47 ms [49.46 ms, 49.53 ms] | 3004.8 [3003.9, 3008.4] | – | DIFFER (2) |
| static_linear_eigen | analysis | nested_dissection | 5 | 1 | 42.13 ms [42.09 ms, 42.25 ms] | 42.13 ms [42.09 ms, 42.25 ms] | 2559.0 [2556.7, 2566.4] | 0.852 [0.850, 0.854] | DIFFER (2) |
| static_linear_eigen | factorize | auto | 5 | 2 | 467.37 ms [461.81 ms, 470.06 ms] | 233.68 ms [230.90 ms, 235.03 ms] | 14193.6 [14024.7, 14275.5] | 0.465 [0.452, 0.472] | DIFFER (2) |
| static_linear_eigen | factorize | base | 5 | 2 | 1.015 s [1.007 s, 1.017 s] | 507.42 ms [503.34 ms, 508.45 ms] | 30820.2 [30572.3, 30882.5] | 1.011 [0.996, 1.019] | DIFFER (2) |
| static_linear_eigen | factorize | colamd | 5 | 2 | 1.006 s [995.68 ms, 1.021 s] | 502.92 ms [497.84 ms, 510.46 ms] | 30546.7 [30238.2, 31004.8] | – | DIFFER (2) |
| static_linear_eigen | factorize | nested_dissection | 5 | 2 | 464.87 ms [455.94 ms, 465.73 ms] | 232.44 ms [227.97 ms, 232.87 ms] | 14117.9 [13846.5, 14143.9] | 0.460 [0.448, 0.467] | DIFFER (2) |
| static_linear_eigen | back_substitution | auto | 5 | 20 | 78.47 ms [77.92 ms, 79.31 ms] | 3.92 ms [3.90 ms, 3.97 ms] | 238.3 [236.6, 240.9] | 0.619 [0.612, 0.623] | DIFFER (2) |
| static_linear_eigen | back_substitution | base | 5 | 20 | 127.80 ms [127.36 ms, 128.82 ms] | 6.39 ms [6.37 ms, 6.44 ms] | 388.1 [386.8, 391.2] | 1.010 [0.995, 1.013] | DIFFER (2) |
| static_linear_eigen | back_substitution | colamd | 5 | 20 | 127.33 ms [126.00 ms, 128.02 ms] | 6.37 ms [6.30 ms, 6.40 ms] | 386.7 [382.7, 388.8] | – | DIFFER (2) |
| static_linear_eigen | back_substitution | nested_dissection | 5 | 20 | 78.86 ms [78.04 ms, 79.07 ms] | 3.94 ms [3.90 ms, 3.95 ms] | 239.5 [237.0, 240.1] | 0.618 [0.615, 0.627] | DIFFER (2) |
| static_newton_plastic | mesh | auto | 5 | 1 | 339.2 µs [327.9 µs, 349.2 µs] | 339.2 µs [327.9 µs, 349.2 µs] | 56.5 [54.6, 58.2] | 1.047 [0.946, 1.081] | DIFFER (2) |
| static_newton_plastic | mesh | base | 5 | 1 | 317.3 µs [309.5 µs, 338.7 µs] | 317.3 µs [309.5 µs, 338.7 µs] | 52.9 [51.6, 56.4] | 0.961 [0.915, 1.048] | DIFFER (2) |
| static_newton_plastic | mesh | colamd | 5 | 1 | 333.6 µs [305.0 µs, 346.8 µs] | 333.6 µs [305.0 µs, 346.8 µs] | 55.6 [50.8, 57.8] | – | DIFFER (2) |
| static_newton_plastic | mesh | nested_dissection | 5 | 1 | 317.5 µs [303.6 µs, 332.8 µs] | 317.5 µs [303.6 µs, 332.8 µs] | 52.9 [50.6, 55.5] | 0.971 [0.875, 1.041] | DIFFER (2) |
| static_newton_plastic | init | auto | 5 | 1 | 4.14 ms [4.12 ms, 4.15 ms] | 4.14 ms [4.12 ms, 4.15 ms] | 689.2 [686.0, 691.2] | 1.002 [0.982, 1.014] | DIFFER (2) |
| static_newton_plastic | init | base | 5 | 1 | 4.15 ms [4.12 ms, 4.23 ms] | 4.15 ms [4.12 ms, 4.23 ms] | 691.6 [687.0, 705.1] | 1.008 [0.986, 1.023] | DIFFER (2) |
| static_newton_plastic | init | colamd | 5 | 1 | 4.12 ms [4.09 ms, 4.21 ms] | 4.12 ms [4.09 ms, 4.21 ms] | 686.7 [681.5, 701.7] | – | DIFFER (2) |
| static_newton_plastic | init | nested_dissection | 5 | 1 | 4.13 ms [4.12 ms, 4.17 ms] | 4.13 ms [4.12 ms, 4.17 ms] | 689.1 [686.1, 695.3] | 1.004 [0.991, 1.007] | DIFFER (2) |
| static_newton_plastic | solve_step | auto | 5 | 12 | 1.337 s [1.327 s, 1.426 s] | 111.40 ms [110.54 ms, 118.85 ms] | 18567.0 [18424.2, 19808.8] | 0.800 [0.765, 0.854] | DIFFER (2) |
| static_newton_plastic | solve_step | base | 5 | 12 | 1.666 s [1.650 s, 1.677 s] | 138.86 ms [137.48 ms, 139.73 ms] | 23143.1 [22912.9, 23288.8] | 0.996 [0.956, 1.012] | DIFFER (2) |
| static_newton_plastic | solve_step | colamd | 5 | 12 | 1.671 s [1.652 s, 1.754 s] | 139.23 ms [137.65 ms, 146.15 ms] | 23204.7 [22941.6, 24357.5] | – | DIFFER (2) |
| static_newton_plastic | solve_step | nested_dissection | 5 | 12 | 1.340 s [1.335 s, 1.343 s] | 111.65 ms [111.22 ms, 111.95 ms] | 18608.3 [18537.1, 18659.0] | 0.800 [0.766, 0.811] | DIFFER (2) |
| static_newton_plastic | predictor | auto | 5 | 12 | 0.9 µs [0.8 µs, 1.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.571 [0.752, 1.690] | DIFFER (2) |
| static_newton_plastic | predictor | base | 5 | 12 | 0.9 µs [0.5 µs, 0.9 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.200 [0.848, 1.517] | DIFFER (2) |
| static_newton_plastic | predictor | colamd | 5 | 12 | 0.6 µs [0.6 µs, 1.0 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| static_newton_plastic | predictor | nested_dissection | 5 | 12 | 1.0 µs [0.4 µs, 1.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.655 [0.552, 1.743] | DIFFER (2) |
| static_newton_plastic | residual | auto | 5 | 961 | 584.56 ms [581.12 ms, 675.26 ms] | 608.3 µs [604.7 µs, 702.7 µs] | 101.4 [100.8, 117.1] | 0.996 [0.860, 1.155] | DIFFER (2) |
| static_newton_plastic | residual | base | 5 | 961 | 585.36 ms [583.01 ms, 585.72 ms] | 609.1 µs [606.7 µs, 609.5 µs] | 101.5 [101.1, 101.6] | 0.997 [0.862, 1.002] | DIFFER (2) |
| static_newton_plastic | residual | colamd | 5 | 961 | 584.58 ms [583.54 ms, 679.66 ms] | 608.3 µs [607.2 µs, 707.2 µs] | 101.4 [101.2, 117.9] | – | DIFFER (2) |
| static_newton_plastic | residual | nested_dissection | 5 | 961 | 582.49 ms [582.29 ms, 584.38 ms] | 606.1 µs [605.9 µs, 608.1 µs] | 101.0 [101.0, 101.3] | 0.996 [0.857, 1.001] | DIFFER (2) |
| static_newton_plastic | matrix | auto | 5 | 949 | 235.6 µs [212.9 µs, 245.0 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.758 [0.726, 1.177] | DIFFER (2) |
| static_newton_plastic | matrix | base | 5 | 949 | 236.5 µs [219.6 µs, 256.2 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.844 [0.681, 1.230] | DIFFER (2) |
| static_newton_plastic | matrix | colamd | 5 | 949 | 281.9 µs [208.2 µs, 322.5 µs] | 0.3 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.1] | – | DIFFER (2) |
| static_newton_plastic | matrix | nested_dissection | 5 | 949 | 224.5 µs [202.0 µs, 268.4 µs] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.750 [0.626, 1.091] | DIFFER (2) |
| static_newton_plastic | corrector | auto | 5 | 949 | 56.0 µs [47.6 µs, 67.9 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.875 [0.780, 1.082] | DIFFER (2) |
| static_newton_plastic | corrector | base | 5 | 949 | 55.5 µs [45.4 µs, 85.2 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.884 [0.785, 1.148] | DIFFER (2) |
| static_newton_plastic | corrector | colamd | 5 | 949 | 62.8 µs [57.8 µs, 74.2 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | – | DIFFER (2) |
| static_newton_plastic | corrector | nested_dissection | 5 | 949 | 53.8 µs [44.8 µs, 76.6 µs] | 0.1 µs [0.0 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.859 [0.725, 1.209] | DIFFER (2) |
| static_newton_plastic | step_hooks | auto | 5 | 24 | 635.6 µs [622.4 µs, 637.7 µs] | 26.5 µs [25.9 µs, 26.6 µs] | 4.4 [4.3, 4.4] | 0.981 [0.980, 1.018] | DIFFER (2) |
| static_newton_plastic | step_hooks | base | 5 | 24 | 639.9 µs [627.4 µs, 658.2 µs] | 26.7 µs [26.1 µs, 27.4 µs] | 4.4 [4.4, 4.6] | 1.001 [0.965, 1.051] | DIFFER (2) |
| static_newton_plastic | step_hooks | colamd | 5 | 24 | 642.4 µs [622.1 µs, 650.5 µs] | 26.8 µs [25.9 µs, 27.1 µs] | 4.5 [4.3, 4.5] | – | DIFFER (2) |
| static_newton_plastic | step_hooks | nested_dissection | 5 | 24 | 630.5 µs [618.5 µs, 656.2 µs] | 26.3 µs [25.8 µs, 27.3 µs] | 4.4 [4.3, 4.6] | 0.987 [0.969, 1.021] | DIFFER (2) |
| static_newton_plastic | step_rest | auto | 5 | 12 | 750.14 ms [744.48 ms, 755.60 ms] | 62.51 ms [62.04 ms, 62.97 ms] | 10418.6 [10340.0, 10494.5] | 0.697 [0.691, 0.704] | DIFFER (2) |
| static_newton_plastic | step_rest | base | 5 | 12 | 1.082 s [1.063 s, 1.090 s] | 90.19 ms [88.60 ms, 90.85 ms] | 15032.2 [14766.0, 15141.0] | 0.997 [0.988, 1.019] | DIFFER (2) |
| static_newton_plastic | step_rest | colamd | 5 | 12 | 1.076 s [1.067 s, 1.086 s] | 89.64 ms [88.94 ms, 90.51 ms] | 14939.5 [14822.7, 15085.6] | – | DIFFER (2) |
| static_newton_plastic | step_rest | nested_dissection | 5 | 12 | 754.97 ms [749.49 ms, 760.14 ms] | 62.91 ms [62.46 ms, 63.35 ms] | 10485.7 [10409.6, 10557.5] | 0.703 [0.690, 0.708] | DIFFER (2) |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_intrinsic_implicit | 3600 | 7442 | 14884 | 1 | 739e03eb4, dfa02e171 | 88b89af4f4608998, bd166b6884da9239 |
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 739e03eb4, dfa02e171 | 0bad43447a75a362, 13939e4cdc8bb8b8 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 739e03eb4, dfa02e171 | 83c1ac7e34efcd05, d5869f57a777d9a5 |
| static_newton_plastic | 6000 | 1331 | 3993 | 1 | 739e03eb4, dfa02e171 | 7bd5f114957d7d7b, e1b550d7684c485a |
