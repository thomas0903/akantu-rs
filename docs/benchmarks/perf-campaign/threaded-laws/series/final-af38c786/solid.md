Suite `solid`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_marigo | solve_step | base-t1 | 5 | 200 | 2.296 s [2.255 s, 2.440 s] | 11.48 ms [11.27 ms, 12.20 ms] | 108.9 [106.9, 115.7] | – | same |
| explicit_marigo | solve_step | head-t1 | 5 | 200 | 2.255 s [2.177 s, 2.414 s] | 11.28 ms [10.89 ms, 12.07 ms] | 106.9 [103.2, 114.4] | 0.982 [0.966, 0.993] | same |
| explicit_marigo | solve_step | head-t2 | 5 | 200 | 2.341 s [2.294 s, 2.470 s] | 11.70 ms [11.47 ms, 12.35 ms] | 111.0 [108.8, 117.1] | 1.018 [0.967, 1.038] | same |
| explicit_marigo | solve_step | head-t4 | 5 | 200 | 2.069 s [2.023 s, 2.109 s] | 10.34 ms [10.12 ms, 10.55 ms] | 98.1 [95.9, 100.0] | 0.897 [0.843, 0.921] | same |
| explicit_marigo | solve_step | head-t8 | 5 | 200 | 2.035 s [1.925 s, 2.078 s] | 10.18 ms [9.63 ms, 10.39 ms] | 96.5 [91.3, 98.5] | 0.856 [0.834, 0.893] | same |
| explicit_maxwell | solve_step | base-t1 | 5 | 40 | 1.600 s [1.524 s, 1.627 s] | 40.01 ms [38.10 ms, 40.67 ms] | 379.4 [361.3, 385.7] | – | same |
| explicit_maxwell | solve_step | head-t1 | 5 | 40 | 1.572 s [1.554 s, 1.609 s] | 39.31 ms [38.85 ms, 40.22 ms] | 372.7 [368.4, 381.4] | 0.987 [0.972, 1.026] | same |
| explicit_maxwell | solve_step | head-t2 | 5 | 40 | 1.312 s [1.309 s, 1.338 s] | 32.81 ms [32.73 ms, 33.46 ms] | 311.1 [310.4, 317.3] | 0.823 [0.819, 0.861] | same |
| explicit_maxwell | solve_step | head-t4 | 5 | 40 | 1.192 s [1.187 s, 1.217 s] | 29.79 ms [29.66 ms, 30.43 ms] | 282.5 [281.3, 288.6] | 0.754 [0.732, 0.779] | same |
| explicit_maxwell | solve_step | head-t8 | 5 | 40 | 1.147 s [1.130 s, 1.200 s] | 28.69 ms [28.25 ms, 30.01 ms] | 272.0 [267.9, 284.6] | 0.720 [0.705, 0.750] | same |
| explicit_mazars | solve_step | base-t1 | 5 | 80 | 2.940 s [2.878 s, 2.990 s] | 36.75 ms [35.98 ms, 37.37 ms] | 348.5 [341.2, 354.4] | – | same |
| explicit_mazars | solve_step | head-t1 | 5 | 80 | 2.857 s [2.836 s, 2.947 s] | 35.71 ms [35.45 ms, 36.83 ms] | 338.7 [336.2, 349.3] | 0.988 [0.965, 1.001] | same |
| explicit_mazars | solve_step | head-t2 | 5 | 80 | 1.605 s [1.593 s, 1.650 s] | 20.06 ms [19.91 ms, 20.63 ms] | 190.2 [188.8, 195.6] | 0.552 [0.543, 0.555] | same |
| explicit_mazars | solve_step | head-t4 | 5 | 80 | 1.035 s [1.027 s, 1.085 s] | 12.94 ms [12.83 ms, 13.57 ms] | 122.7 [121.7, 128.7] | 0.355 [0.352, 0.363] | same |
| explicit_mazars | solve_step | head-t8 | 5 | 80 | 733.37 ms [724.09 ms, 777.38 ms] | 9.17 ms [9.05 ms, 9.72 ms] | 86.9 [85.8, 92.1] | 0.252 [0.249, 0.260] | same |
| explicit_mazars_non_local | solve_step | base-t1 | 5 | 40 | 4.171 s [4.149 s, 4.351 s] | 104.27 ms [103.73 ms, 108.77 ms] | 2172.2 [2161.0, 2266.0] | – | same |
| explicit_mazars_non_local | solve_step | head-t1 | 5 | 40 | 4.175 s [4.119 s, 4.317 s] | 104.38 ms [102.99 ms, 107.92 ms] | 2174.5 [2145.5, 2248.4] | 0.998 [0.992, 1.009] | same |
| explicit_mazars_non_local | solve_step | head-t2 | 5 | 40 | 3.974 s [3.952 s, 4.145 s] | 99.34 ms [98.79 ms, 103.62 ms] | 2069.6 [2058.1, 2158.7] | 0.953 [0.946, 0.958] | same |
| explicit_mazars_non_local | solve_step | head-t4 | 5 | 40 | 3.888 s [3.843 s, 4.070 s] | 97.19 ms [96.09 ms, 101.75 ms] | 2024.8 [2001.8, 2119.9] | 0.932 [0.926, 0.938] | same |
| explicit_mazars_non_local | solve_step | head-t8 | 5 | 40 | 3.830 s [3.806 s, 3.998 s] | 95.76 ms [95.15 ms, 99.94 ms] | 1994.9 [1982.3, 2082.2] | 0.919 [0.912, 0.929] | same |
| explicit_multi_material | solve_step | base-t1 | 5 | 120 | 2.590 s [2.537 s, 2.684 s] | 21.59 ms [21.14 ms, 22.37 ms] | 204.7 [200.5, 212.1] | – | same |
| explicit_multi_material | solve_step | head-t1 | 5 | 120 | 2.575 s [2.543 s, 2.698 s] | 21.46 ms [21.19 ms, 22.48 ms] | 203.5 [201.0, 213.2] | 0.994 [0.964, 1.005] | same |
| explicit_multi_material | solve_step | head-t2 | 5 | 120 | 1.513 s [1.472 s, 1.562 s] | 12.61 ms [12.26 ms, 13.02 ms] | 119.6 [116.3, 123.5] | 0.582 [0.568, 0.593] | same |
| explicit_multi_material | solve_step | head-t4 | 5 | 120 | 1.025 s [1.019 s, 1.037 s] | 8.54 ms [8.49 ms, 8.65 ms] | 81.0 [80.5, 82.0] | 0.395 [0.383, 0.402] | same |
| explicit_multi_material | solve_step | head-t8 | 5 | 120 | 794.42 ms [783.71 ms, 804.28 ms] | 6.62 ms [6.53 ms, 6.70 ms] | 62.8 [61.9, 63.6] | 0.303 [0.297, 0.309] | same |
| explicit_neohookean | solve_step | base-t1 | 5 | 100 | 1.413 s [1.382 s, 1.449 s] | 14.13 ms [13.82 ms, 14.49 ms] | 134.0 [131.0, 137.4] | – | same |
| explicit_neohookean | solve_step | head-t1 | 5 | 100 | 1.322 s [1.295 s, 1.373 s] | 13.22 ms [12.95 ms, 13.73 ms] | 125.3 [122.8, 130.2] | 0.945 [0.916, 0.963] | same |
| explicit_neohookean | solve_step | head-t2 | 5 | 100 | 839.85 ms [818.51 ms, 868.64 ms] | 8.40 ms [8.19 ms, 8.69 ms] | 79.6 [77.6, 82.4] | 0.597 [0.579, 0.609] | same |
| explicit_neohookean | solve_step | head-t4 | 5 | 100 | 550.67 ms [543.74 ms, 565.55 ms] | 5.51 ms [5.44 ms, 5.66 ms] | 52.2 [51.6, 53.6] | 0.394 [0.385, 0.397] | same |
| explicit_neohookean | solve_step | head-t8 | 5 | 100 | 440.22 ms [431.94 ms, 453.15 ms] | 4.40 ms [4.32 ms, 4.53 ms] | 41.7 [41.0, 43.0] | 0.317 [0.304, 0.318] | same |
| explicit_orthotropic | solve_step | base-t1 | 5 | 200 | 1.378 s [1.374 s, 1.446 s] | 6.89 ms [6.87 ms, 7.23 ms] | 65.3 [65.1, 68.5] | – | same |
| explicit_orthotropic | solve_step | head-t1 | 5 | 200 | 1.413 s [1.395 s, 1.470 s] | 7.07 ms [6.97 ms, 7.35 ms] | 67.0 [66.1, 69.7] | 1.017 [1.014, 1.026] | same |
| explicit_orthotropic | solve_step | head-t2 | 5 | 200 | 980.77 ms [970.56 ms, 1.029 s] | 4.90 ms [4.85 ms, 5.15 ms] | 46.5 [46.0, 48.8] | 0.710 [0.685, 0.714] | same |
| explicit_orthotropic | solve_step | head-t4 | 5 | 200 | 807.49 ms [766.83 ms, 827.29 ms] | 4.04 ms [3.83 ms, 4.14 ms] | 38.3 [36.4, 39.2] | 0.572 [0.557, 0.590] | same |
| explicit_orthotropic | solve_step | head-t8 | 5 | 200 | 728.98 ms [709.95 ms, 750.24 ms] | 3.64 ms [3.55 ms, 3.75 ms] | 34.6 [33.7, 35.6] | 0.519 [0.514, 0.531] | same |
| explicit_plastic | solve_step | base-t1 | 5 | 120 | 2.725 s [2.661 s, 2.912 s] | 22.71 ms [22.18 ms, 24.27 ms] | 215.3 [210.3, 230.1] | – | same |
| explicit_plastic | solve_step | head-t1 | 5 | 120 | 2.742 s [2.717 s, 2.927 s] | 22.85 ms [22.64 ms, 24.39 ms] | 216.7 [214.7, 231.3] | 1.006 [0.993, 1.021] | same |
| explicit_plastic | solve_step | head-t2 | 5 | 120 | 2.346 s [2.202 s, 2.456 s] | 19.55 ms [18.35 ms, 20.47 ms] | 185.4 [174.0, 194.1] | 0.862 [0.789, 0.865] | same |
| explicit_plastic | solve_step | head-t4 | 5 | 120 | 2.037 s [1.975 s, 2.282 s] | 16.97 ms [16.46 ms, 19.02 ms] | 161.0 [156.1, 180.3] | 0.765 [0.724, 0.792] | same |
| explicit_plastic | solve_step | head-t8 | 5 | 120 | 2.018 s [1.967 s, 2.176 s] | 16.82 ms [16.39 ms, 18.14 ms] | 159.5 [155.4, 172.0] | 0.744 [0.711, 0.762] | same |
| explicit_sls | solve_step | base-t1 | 5 | 120 | 2.580 s [2.464 s, 2.688 s] | 21.50 ms [20.53 ms, 22.40 ms] | 203.9 [194.7, 212.4] | – | same |
| explicit_sls | solve_step | head-t1 | 5 | 120 | 2.628 s [2.471 s, 2.711 s] | 21.90 ms [20.59 ms, 22.59 ms] | 207.6 [195.3, 214.3] | 1.018 [1.001, 1.067] | same |
| explicit_sls | solve_step | head-t2 | 5 | 120 | 2.051 s [2.010 s, 2.123 s] | 17.09 ms [16.75 ms, 17.69 ms] | 162.0 [158.8, 167.8] | 0.803 [0.776, 0.832] | same |
| explicit_sls | solve_step | head-t4 | 5 | 120 | 1.793 s [1.740 s, 1.838 s] | 14.94 ms [14.50 ms, 15.32 ms] | 141.7 [137.5, 145.3] | 0.712 [0.671, 0.728] | same |
| explicit_sls | solve_step | head-t8 | 5 | 120 | 1.686 s [1.653 s, 1.724 s] | 14.05 ms [13.78 ms, 14.36 ms] | 133.2 [130.6, 136.2] | 0.653 [0.639, 0.698] | same |
| explicit_thermal | solve_step | base-t1 | 5 | 300 | 1.841 s [1.809 s, 1.919 s] | 6.14 ms [6.03 ms, 6.40 ms] | 58.2 [57.2, 60.7] | – | same |
| explicit_thermal | solve_step | head-t1 | 5 | 300 | 1.856 s [1.817 s, 1.938 s] | 6.19 ms [6.06 ms, 6.46 ms] | 58.7 [57.4, 61.3] | 1.004 [0.988, 1.010] | same |
| explicit_thermal | solve_step | head-t2 | 5 | 300 | 1.303 s [1.285 s, 1.384 s] | 4.34 ms [4.28 ms, 4.61 ms] | 41.2 [40.6, 43.7] | 0.710 [0.694, 0.721] | same |
| explicit_thermal | solve_step | head-t4 | 5 | 300 | 1.061 s [996.48 ms, 1.089 s] | 3.54 ms [3.32 ms, 3.63 ms] | 33.5 [31.5, 34.4] | 0.565 [0.544, 0.576] | same |
| explicit_thermal | solve_step | head-t8 | 5 | 300 | 907.50 ms [892.28 ms, 913.14 ms] | 3.03 ms [2.97 ms, 3.04 ms] | 28.7 [28.2, 28.9] | 0.487 [0.473, 0.505] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_marigo | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 017c8c9ef7659aa0 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | af38c786d, c9eef15b8 | 849f4cd05e9ad628 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | aa8ed096eeb974f2 |
| explicit_orthotropic | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 545b629b46d364ad |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 36137566d8b810bb |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | af38c786d, c9eef15b8 | 27979d9fbcdea83d |
