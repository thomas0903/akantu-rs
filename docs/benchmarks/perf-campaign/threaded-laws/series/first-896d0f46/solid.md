Suite `solid`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_marigo | solve_step | base-t1 | 5 | 200 | 2.285 s [2.282 s, 2.343 s] | 11.43 ms [11.41 ms, 11.71 ms] | 108.3 [108.2, 111.1] | – | same |
| explicit_marigo | solve_step | head-t1 | 5 | 200 | 2.210 s [2.192 s, 2.252 s] | 11.05 ms [10.96 ms, 11.26 ms] | 104.8 [103.9, 106.8] | 0.961 [0.958, 0.987] | same |
| explicit_marigo | solve_step | head-t2 | 5 | 200 | 2.334 s [2.309 s, 2.340 s] | 11.67 ms [11.55 ms, 11.70 ms] | 110.7 [109.5, 111.0] | 1.017 [0.999, 1.022] | same |
| explicit_marigo | solve_step | head-t4 | 5 | 200 | 2.089 s [2.065 s, 2.125 s] | 10.44 ms [10.32 ms, 10.63 ms] | 99.0 [97.9, 100.8] | 0.908 [0.904, 0.920] | same |
| explicit_marigo | solve_step | head-t8 | 5 | 200 | 1.961 s [1.934 s, 1.987 s] | 9.80 ms [9.67 ms, 9.94 ms] | 93.0 [91.7, 94.2] | 0.855 [0.837, 0.870] | same |
| explicit_maxwell | solve_step | base-t1 | 5 | 40 | 1.525 s [1.497 s, 1.559 s] | 38.13 ms [37.42 ms, 38.96 ms] | 361.6 [354.8, 369.5] | – | same |
| explicit_maxwell | solve_step | head-t1 | 5 | 40 | 1.587 s [1.564 s, 1.620 s] | 39.66 ms [39.10 ms, 40.50 ms] | 376.1 [370.8, 384.1] | 1.045 [1.035, 1.054] | same |
| explicit_maxwell | solve_step | head-t2 | 5 | 40 | 1.336 s [1.297 s, 1.353 s] | 33.40 ms [32.42 ms, 33.81 ms] | 316.7 [307.4, 320.6] | 0.870 [0.864, 0.898] | same |
| explicit_maxwell | solve_step | head-t4 | 5 | 40 | 1.188 s [1.174 s, 1.199 s] | 29.71 ms [29.36 ms, 29.97 ms] | 281.7 [278.4, 284.2] | 0.780 [0.769, 0.789] | same |
| explicit_maxwell | solve_step | head-t8 | 5 | 40 | 1.132 s [1.119 s, 1.166 s] | 28.31 ms [27.97 ms, 29.16 ms] | 268.4 [265.2, 276.5] | 0.750 [0.727, 0.773] | same |
| explicit_mazars | solve_step | base-t1 | 5 | 80 | 2.903 s [2.882 s, 2.935 s] | 36.28 ms [36.03 ms, 36.69 ms] | 344.0 [341.7, 347.9] | – | same |
| explicit_mazars | solve_step | head-t1 | 5 | 80 | 2.887 s [2.870 s, 2.921 s] | 36.09 ms [35.87 ms, 36.51 ms] | 342.3 [340.1, 346.2] | 0.995 [0.988, 1.006] | same |
| explicit_mazars | solve_step | head-t2 | 5 | 80 | 1.622 s [1.612 s, 1.631 s] | 20.27 ms [20.15 ms, 20.39 ms] | 192.2 [191.1, 193.3] | 0.559 [0.555, 0.560] | same |
| explicit_mazars | solve_step | head-t4 | 5 | 80 | 1.044 s [1.038 s, 1.053 s] | 13.05 ms [12.98 ms, 13.16 ms] | 123.7 [123.1, 124.8] | 0.359 [0.357, 0.363] | same |
| explicit_mazars | solve_step | head-t8 | 5 | 80 | 738.86 ms [722.67 ms, 745.90 ms] | 9.24 ms [9.03 ms, 9.32 ms] | 87.6 [85.7, 88.4] | 0.254 [0.251, 0.258] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 4.227 s [4.148 s, 4.298 s] | 105.67 ms [103.71 ms, 107.44 ms] | 2201.5 [2160.7, 2238.3] | – | same |
| explicit_mazars_non_local | solve_step | head-t1 | 5 | 40 | 4.190 s [4.153 s, 4.211 s] | 104.76 ms [103.82 ms, 105.29 ms] | 2182.4 [2163.0, 2193.5] | 0.991 [0.975, 1.010] | same |
| explicit_mazars_non_local | solve_step | head-t2 | 5 | 40 | 3.972 s [3.957 s, 4.052 s] | 99.30 ms [98.93 ms, 101.29 ms] | 2068.7 [2061.0, 2110.2] | 0.943 [0.936, 0.954] | same |
| explicit_mazars_non_local | solve_step | head-t4 | 5 | 40 | 3.917 s [3.873 s, 4.037 s] | 97.92 ms [96.82 ms, 100.92 ms] | 2040.1 [2017.2, 2102.5] | 0.934 [0.924, 0.951] | same |
| explicit_mazars_non_local | solve_step | head-t8 | 5 | 40 | 3.897 s [3.809 s, 3.955 s] | 97.41 ms [95.22 ms, 98.88 ms] | 2029.4 [1983.7, 2059.9] | 0.926 [0.901, 0.932] | same |
| explicit_multi_material | solve_step | base-t1 | 5 | 120 | 2.585 s [2.564 s, 2.603 s] | 21.54 ms [21.37 ms, 21.69 ms] | 204.3 [202.6, 205.7] | – | same |
| explicit_multi_material | solve_step | head-t1 | 5 | 120 | 2.568 s [2.535 s, 2.603 s] | 21.40 ms [21.12 ms, 21.69 ms] | 202.9 [200.3, 205.7] | 0.993 [0.987, 1.002] | same |
| explicit_multi_material | solve_step | head-t2 | 5 | 120 | 1.503 s [1.490 s, 1.512 s] | 12.53 ms [12.42 ms, 12.60 ms] | 118.8 [117.8, 119.5] | 0.581 [0.577, 0.587] | same |
| explicit_multi_material | solve_step | head-t4 | 5 | 120 | 1.025 s [1.003 s, 1.037 s] | 8.55 ms [8.36 ms, 8.64 ms] | 81.0 [79.3, 82.0] | 0.395 [0.391, 0.398] | same |
| explicit_multi_material | solve_step | head-t8 | 5 | 120 | 787.46 ms [783.61 ms, 799.61 ms] | 6.56 ms [6.53 ms, 6.66 ms] | 62.2 [61.9, 63.2] | 0.304 [0.301, 0.312] | same |
| explicit_neohookean | solve_step | base-t1 | 5 | 100 | 1.389 s [1.371 s, 1.445 s] | 13.89 ms [13.71 ms, 14.45 ms] | 131.7 [130.0, 137.0] | – | same |
| explicit_neohookean | solve_step | head-t1 | 5 | 100 | 1.173 s [1.156 s, 1.177 s] | 11.73 ms [11.56 ms, 11.77 ms] | 111.2 [109.6, 111.6] | 0.841 [0.815, 0.853] | same |
| explicit_neohookean | solve_step | head-t2 | 5 | 100 | 757.74 ms [753.32 ms, 764.38 ms] | 7.58 ms [7.53 ms, 7.64 ms] | 71.9 [71.4, 72.5] | 0.547 [0.529, 0.551] | same |
| explicit_neohookean | solve_step | head-t4 | 5 | 100 | 502.92 ms [499.30 ms, 509.17 ms] | 5.03 ms [4.99 ms, 5.09 ms] | 47.7 [47.3, 48.3] | 0.362 [0.346, 0.371] | same |
| explicit_neohookean | solve_step | head-t8 | 5 | 100 | 416.92 ms [412.28 ms, 427.50 ms] | 4.17 ms [4.12 ms, 4.27 ms] | 39.5 [39.1, 40.5] | 0.302 [0.285, 0.306] | same |
| explicit_orthotropic | solve_step | base-t1 | 5 | 200 | 1.396 s [1.378 s, 1.407 s] | 6.98 ms [6.89 ms, 7.03 ms] | 66.2 [65.3, 66.7] | – | same |
| explicit_orthotropic | solve_step | head-t1 | 5 | 200 | 1.369 s [1.353 s, 1.391 s] | 6.84 ms [6.76 ms, 6.95 ms] | 64.9 [64.1, 65.9] | 0.982 [0.973, 1.002] | same |
| explicit_orthotropic | solve_step | head-t2 | 5 | 200 | 972.52 ms [962.07 ms, 987.44 ms] | 4.86 ms [4.81 ms, 4.94 ms] | 46.1 [45.6, 46.8] | 0.701 [0.696, 0.704] | same |
| explicit_orthotropic | solve_step | head-t4 | 5 | 200 | 790.34 ms [779.14 ms, 813.47 ms] | 3.95 ms [3.90 ms, 4.07 ms] | 37.5 [36.9, 38.6] | 0.566 [0.558, 0.582] | same |
| explicit_orthotropic | solve_step | head-t8 | 5 | 200 | 725.95 ms [713.15 ms, 749.87 ms] | 3.63 ms [3.57 ms, 3.75 ms] | 34.4 [33.8, 35.6] | 0.524 [0.507, 0.537] | same |
| explicit_plastic | solve_step | base-t1 | 5 | 120 | 2.845 s [2.828 s, 2.869 s] | 23.71 ms [23.57 ms, 23.91 ms] | 224.8 [223.5, 226.7] | – | same |
| explicit_plastic | solve_step | head-t1 | 5 | 120 | 2.776 s [2.646 s, 2.825 s] | 23.13 ms [22.05 ms, 23.54 ms] | 219.3 [209.1, 223.2] | 0.967 [0.933, 0.986] | same |
| explicit_plastic | solve_step | head-t2 | 5 | 120 | 2.311 s [2.212 s, 2.346 s] | 19.26 ms [18.44 ms, 19.55 ms] | 182.6 [174.8, 185.4] | 0.814 [0.772, 0.818] | same |
| explicit_plastic | solve_step | head-t4 | 5 | 120 | 2.116 s [2.020 s, 2.128 s] | 17.63 ms [16.83 ms, 17.73 ms] | 167.2 [159.6, 168.2] | 0.744 [0.704, 0.752] | same |
| explicit_plastic | solve_step | head-t8 | 5 | 120 | 2.100 s [2.000 s, 2.116 s] | 17.50 ms [16.67 ms, 17.63 ms] | 166.0 [158.1, 167.2] | 0.737 [0.703, 0.748] | same |
| explicit_sls | solve_step | base-t1 | 5 | 120 | 2.561 s [2.449 s, 2.611 s] | 21.34 ms [20.41 ms, 21.76 ms] | 202.4 [193.5, 206.3] | – | same |
| explicit_sls | solve_step | head-t1 | 5 | 120 | 2.589 s [2.477 s, 2.664 s] | 21.58 ms [20.64 ms, 22.20 ms] | 204.6 [195.7, 210.5] | 1.011 [0.999, 1.029] | same |
| explicit_sls | solve_step | head-t2 | 5 | 120 | 2.040 s [2.029 s, 2.076 s] | 17.00 ms [16.91 ms, 17.30 ms] | 161.2 [160.3, 164.1] | 0.796 [0.781, 0.837] | same |
| explicit_sls | solve_step | head-t4 | 5 | 120 | 1.782 s [1.768 s, 1.829 s] | 14.85 ms [14.73 ms, 15.24 ms] | 140.8 [139.7, 144.5] | 0.714 [0.683, 0.723] | same |
| explicit_sls | solve_step | head-t8 | 5 | 120 | 1.698 s [1.637 s, 1.721 s] | 14.15 ms [13.64 ms, 14.35 ms] | 134.2 [129.3, 136.0] | 0.660 [0.656, 0.677] | same |
| explicit_thermal | solve_step | base-t1 | 5 | 300 | 1.819 s [1.808 s, 1.847 s] | 6.06 ms [6.03 ms, 6.16 ms] | 57.5 [57.2, 58.4] | – | same |
| explicit_thermal | solve_step | head-t1 | 5 | 300 | 1.787 s [1.777 s, 1.812 s] | 5.96 ms [5.92 ms, 6.04 ms] | 56.5 [56.2, 57.3] | 0.981 [0.976, 0.993] | same |
| explicit_thermal | solve_step | head-t2 | 5 | 300 | 1.358 s [1.334 s, 1.371 s] | 4.53 ms [4.45 ms, 4.57 ms] | 42.9 [42.2, 43.3] | 0.751 [0.722, 0.754] | same |
| explicit_thermal | solve_step | head-t4 | 5 | 300 | 1.058 s [1.034 s, 1.110 s] | 3.53 ms [3.45 ms, 3.70 ms] | 33.5 [32.7, 35.1] | 0.581 [0.565, 0.610] | same |
| explicit_thermal | solve_step | head-t8 | 5 | 300 | 899.28 ms [872.65 ms, 959.79 ms] | 3.00 ms [2.91 ms, 3.20 ms] | 28.4 [27.6, 30.3] | 0.490 [0.483, 0.528] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 896d0f46b, c9eef15b8 | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 36137566d8b810bb |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | 896d0f46b, c9eef15b8 | 27979d9fbcdea83d |
