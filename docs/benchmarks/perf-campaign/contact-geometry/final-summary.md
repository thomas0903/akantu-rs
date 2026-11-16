Suite `models`, size `m`; variants: `after`, `before`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | mesh | after | 5 | 1 | 98.97 ms [97.24 ms, 108.39 ms] | 98.97 ms [97.24 ms, 108.39 ms] | 1546.3 [1519.4, 1693.6] | 0.988 [0.980, 1.090] | same |
| contact_cohesive_quad4 | mesh | before | 5 | 1 | 99.46 ms [98.70 ms, 100.16 ms] | 99.46 ms [98.70 ms, 100.16 ms] | 1554.0 [1542.1, 1565.0] | – | same |
| contact_cohesive_quad4 | init | after | 5 | 1 | 216.55 ms [207.11 ms, 242.19 ms] | 216.55 ms [207.11 ms, 242.19 ms] | 3383.5 [3236.2, 3784.3] | 1.004 [0.987, 1.127] | same |
| contact_cohesive_quad4 | init | before | 5 | 1 | 214.86 ms [206.74 ms, 217.76 ms] | 214.86 ms [206.74 ms, 217.76 ms] | 3357.1 [3230.3, 3402.5] | – | same |
| contact_cohesive_quad4 | solve_step | after | 5 | 300 | 7.870 s [7.322 s, 9.369 s] | 26.23 ms [24.41 ms, 31.23 ms] | 409.9 [381.3, 488.0] | 0.972 [0.959, 1.121] | same |
| contact_cohesive_quad4 | solve_step | before | 5 | 300 | 8.093 s [7.533 s, 8.357 s] | 26.98 ms [25.11 ms, 27.86 ms] | 421.5 [392.3, 435.3] | – | same |
| contact_explicit_quad4 | mesh | after | 5 | 1 | 199.16 ms [194.91 ms, 217.55 ms] | 199.16 ms [194.91 ms, 217.55 ms] | 1580.6 [1546.9, 1726.6] | 0.976 [0.962, 1.045] | same |
| contact_explicit_quad4 | mesh | before | 5 | 1 | 202.62 ms [199.35 ms, 208.26 ms] | 202.62 ms [199.35 ms, 208.26 ms] | 1608.1 [1582.1, 1652.8] | – | same |
| contact_explicit_quad4 | init | after | 5 | 1 | 259.66 ms [251.10 ms, 264.88 ms] | 259.66 ms [251.10 ms, 264.88 ms] | 2060.8 [1992.8, 2102.2] | 1.003 [0.989, 1.022] | same |
| contact_explicit_quad4 | init | before | 5 | 1 | 259.23 ms [250.40 ms, 261.18 ms] | 259.23 ms [250.40 ms, 261.18 ms] | 2057.4 [1987.3, 2072.9] | – | same |
| contact_explicit_quad4 | stable_time_step | after | 5 | 1 | 3.77 ms [3.75 ms, 3.78 ms] | 3.77 ms [3.75 ms, 3.78 ms] | 29.9 [29.8, 30.0] | 1.027 [1.019, 1.028] | same |
| contact_explicit_quad4 | stable_time_step | before | 5 | 1 | 3.67 ms [3.65 ms, 3.68 ms] | 3.67 ms [3.65 ms, 3.68 ms] | 29.2 [29.0, 29.2] | – | same |
| contact_explicit_quad4 | solve_step | after | 5 | 200 | 4.413 s [4.102 s, 4.664 s] | 22.07 ms [20.51 ms, 23.32 ms] | 175.1 [162.8, 185.1] | 0.962 [0.938, 0.988] | same |
| contact_explicit_quad4 | solve_step | before | 5 | 200 | 4.642 s [4.187 s, 4.846 s] | 23.21 ms [20.93 ms, 24.23 ms] | 184.2 [166.1, 192.3] | – | same |
| contact_implicit_static_quad4 | mesh | after | 5 | 1 | 11.32 ms [11.22 ms, 11.45 ms] | 11.32 ms [11.22 ms, 11.45 ms] | 1617.9 [1603.0, 1635.3] | 0.984 [0.979, 0.990] | same |
| contact_implicit_static_quad4 | mesh | before | 5 | 1 | 11.46 ms [11.43 ms, 11.59 ms] | 11.46 ms [11.43 ms, 11.59 ms] | 1637.7 [1632.9, 1655.9] | – | same |
| contact_implicit_static_quad4 | init | after | 5 | 1 | 14.32 ms [14.03 ms, 14.65 ms] | 14.32 ms [14.03 ms, 14.65 ms] | 2046.3 [2004.6, 2092.4] | 0.995 [0.979, 1.008] | same |
| contact_implicit_static_quad4 | init | before | 5 | 1 | 14.35 ms [14.30 ms, 14.53 ms] | 14.35 ms [14.30 ms, 14.53 ms] | 2050.6 [2043.0, 2076.3] | – | same |
| contact_implicit_static_quad4 | solve_step | after | 5 | 5 | 2.100 s [2.071 s, 2.137 s] | 419.97 ms [414.15 ms, 427.44 ms] | 59995.7 [59164.6, 61062.6] | 0.993 [0.976, 1.000] | same |
| contact_implicit_static_quad4 | solve_step | before | 5 | 5 | 2.135 s [2.085 s, 2.147 s] | 426.96 ms [417.02 ms, 429.37 ms] | 60995.0 [59574.8, 61338.4] | – | same |
| contact_search_quad4 | mesh | after | 5 | 1 | 380.78 ms [376.57 ms, 436.75 ms] | 380.78 ms [376.57 ms, 436.75 ms] | 2719.9 [2689.8, 3119.6] | 0.978 [0.970, 1.036] | same |
| contact_search_quad4 | mesh | before | 5 | 1 | 384.99 ms [383.75 ms, 449.15 ms] | 384.99 ms [383.75 ms, 449.15 ms] | 2750.0 [2741.1, 3208.2] | – | same |
| contact_search_quad4 | init | after | 5 | 1 | 329.71 ms [324.00 ms, 339.45 ms] | 329.71 ms [324.00 ms, 339.45 ms] | 2355.0 [2314.3, 2424.7] | 1.004 [0.988, 1.012] | same |
| contact_search_quad4 | init | before | 5 | 1 | 328.73 ms [322.48 ms, 335.38 ms] | 328.73 ms [322.48 ms, 335.38 ms] | 2348.1 [2303.4, 2395.6] | – | same |
| contact_search_quad4 | search | after | 5 | 60 | 4.474 s [4.430 s, 4.498 s] | 74.57 ms [73.83 ms, 74.97 ms] | 532.7 [527.3, 535.5] | 0.832 [0.829, 0.835] | same |
| contact_search_quad4 | search | before | 5 | 60 | 5.378 s [5.345 s, 5.405 s] | 89.63 ms [89.08 ms, 90.08 ms] | 640.2 [636.3, 643.4] | – | same |
| contact_search_tet4 | mesh | after | 5 | 1 | 872.49 ms [859.64 ms, 924.32 ms] | 872.49 ms [859.64 ms, 924.32 ms] | 1817.7 [1790.9, 1925.7] | 0.984 [0.976, 1.021] | same |
| contact_search_tet4 | mesh | before | 5 | 1 | 885.27 ms [873.20 ms, 904.94 ms] | 885.27 ms [873.20 ms, 904.94 ms] | 1844.3 [1819.2, 1885.3] | – | same |
| contact_search_tet4 | init | after | 5 | 1 | 468.57 ms [459.88 ms, 476.06 ms] | 468.57 ms [459.88 ms, 476.06 ms] | 976.2 [958.1, 991.8] | 1.001 [0.996, 1.020] | same |
| contact_search_tet4 | init | before | 5 | 1 | 466.92 ms [455.86 ms, 473.97 ms] | 466.92 ms [455.86 ms, 473.97 ms] | 972.7 [949.7, 987.4] | – | same |
| contact_search_tet4 | search | after | 5 | 30 | 4.155 s [4.139 s, 4.188 s] | 138.51 ms [137.96 ms, 139.59 ms] | 288.6 [287.4, 290.8] | 0.817 [0.804, 0.819] | same |
| contact_search_tet4 | search | before | 5 | 30 | 5.098 s [5.068 s, 5.170 s] | 169.92 ms [168.94 ms, 172.33 ms] | 354.0 [352.0, 359.0] | – | same |
| embedded_rebar_tet4 | mesh | after | 5 | 1 | 28.01 ms [27.80 ms, 31.72 ms] | 28.01 ms [27.80 ms, 31.72 ms] | 1287.9 [1278.2, 1458.4] | 0.991 [0.980, 1.119] | same |
| embedded_rebar_tet4 | mesh | before | 5 | 1 | 28.35 ms [28.03 ms, 28.51 ms] | 28.35 ms [28.03 ms, 28.51 ms] | 1303.5 [1288.8, 1311.0] | – | same |
| embedded_rebar_tet4 | init | after | 5 | 1 | 39.98 ms [39.30 ms, 45.93 ms] | 39.98 ms [39.30 ms, 45.93 ms] | 1838.1 [1806.9, 2112.0] | 0.028 [0.027, 0.032] | same |
| embedded_rebar_tet4 | init | before | 5 | 1 | 1.421 s [1.417 s, 1.446 s] | 1.421 s [1.417 s, 1.446 s] | 65318.4 [65167.0, 66498.3] | – | same |
| embedded_rebar_tet4 | intersect | after | 5 | 5 | 121.17 ms [118.93 ms, 135.68 ms] | 24.23 ms [23.79 ms, 27.14 ms] | 1114.2 [1093.6, 1247.7] | 0.017 [0.017, 0.018] | same |
| embedded_rebar_tet4 | intersect | before | 5 | 5 | 7.175 s [6.992 s, 7.458 s] | 1.435 s [1.398 s, 1.492 s] | 65975.3 [64294.6, 68578.1] | – | same |
| embedded_rebar_tet4 | solve_step | after | 5 | 1 | 4.214 s [4.158 s, 5.120 s] | 4.214 s [4.158 s, 5.120 s] | 193729.8 [191182.5, 235402.5] | 1.000 [0.995, 1.063] | same |
| embedded_rebar_tet4 | solve_step | before | 5 | 1 | 4.232 s [4.175 s, 4.817 s] | 4.232 s [4.175 s, 4.817 s] | 194558.9 [191933.7, 221491.4] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | after | 5 | 20 | 34.41 ms [34.31 ms, 40.84 ms] | 1.72 ms [1.72 ms, 2.04 ms] | 79.1 [78.9, 93.9] | 0.998 [0.951, 1.052] | same |
| embedded_rebar_tet4 | assemble_internal_forces | before | 5 | 20 | 34.56 ms [34.49 ms, 38.81 ms] | 1.73 ms [1.72 ms, 1.94 ms] | 79.4 [79.3, 89.2] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | after | 5 | 5 | 589.11 ms [581.77 ms, 655.68 ms] | 117.82 ms [116.35 ms, 131.14 ms] | 5417.1 [5349.6, 6029.2] | 1.011 [0.983, 1.022] | same |
| embedded_rebar_tet4 | assemble_stiffness | before | 5 | 5 | 577.28 ms [575.34 ms, 641.78 ms] | 115.46 ms [115.07 ms, 128.36 ms] | 5308.3 [5290.5, 5901.4] | – | same |
| embedded_rebar_tri3 | mesh | after | 5 | 1 | 27.12 ms [26.85 ms, 28.08 ms] | 27.12 ms [26.85 ms, 28.08 ms] | 1076.2 [1065.6, 1114.4] | 0.988 [0.881, 0.996] | same |
| embedded_rebar_tri3 | mesh | before | 5 | 1 | 27.26 ms [27.15 ms, 31.86 ms] | 27.26 ms [27.15 ms, 31.86 ms] | 1081.9 [1077.6, 1264.2] | – | same |
| embedded_rebar_tri3 | init | after | 5 | 1 | 31.96 ms [31.25 ms, 33.68 ms] | 31.96 ms [31.25 ms, 33.68 ms] | 1268.1 [1239.9, 1336.4] | 0.021 [0.018, 0.021] | same |
| embedded_rebar_tri3 | init | before | 5 | 1 | 1.585 s [1.517 s, 1.831 s] | 1.585 s [1.517 s, 1.831 s] | 62892.1 [60199.4, 72653.5] | – | same |
| embedded_rebar_tri3 | intersect | after | 5 | 5 | 62.80 ms [62.08 ms, 63.18 ms] | 12.56 ms [12.42 ms, 12.64 ms] | 498.4 [492.7, 501.4] | 0.008 [0.007, 0.008] | same |
| embedded_rebar_tri3 | intersect | before | 5 | 5 | 7.827 s [7.489 s, 8.597 s] | 1.565 s [1.498 s, 1.719 s] | 62121.8 [59436.5, 68232.0] | – | same |
| embedded_rebar_tri3 | solve_step | after | 5 | 1 | 538.41 ms [534.40 ms, 540.99 ms] | 538.41 ms [534.40 ms, 540.99 ms] | 21365.3 [21206.5, 21467.8] | 0.984 [0.929, 0.988] | same |
| embedded_rebar_tri3 | solve_step | before | 5 | 1 | 547.80 ms [541.69 ms, 579.93 ms] | 547.80 ms [541.69 ms, 579.93 ms] | 21738.2 [21495.5, 23013.3] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | after | 5 | 20 | 48.88 ms [48.85 ms, 49.09 ms] | 2.44 ms [2.44 ms, 2.45 ms] | 97.0 [96.9, 97.4] | 0.999 [0.980, 1.005] | same |
| embedded_rebar_tri3 | assemble_internal_forces | before | 5 | 20 | 48.92 ms [48.80 ms, 50.09 ms] | 2.45 ms [2.44 ms, 2.50 ms] | 97.1 [96.8, 99.4] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | after | 5 | 5 | 158.20 ms [154.36 ms, 161.89 ms] | 31.64 ms [30.87 ms, 32.38 ms] | 1255.6 [1225.0, 1284.9] | 0.995 [0.971, 1.028] | same |
| embedded_rebar_tri3 | assemble_stiffness | before | 5 | 5 | 158.92 ms [156.86 ms, 160.78 ms] | 31.78 ms [31.37 ms, 32.16 ms] | 1261.3 [1244.9, 1276.0] | – | same |
| ntn_friction_quad4 | mesh | after | 5 | 1 | 316.94 ms [313.10 ms, 319.86 ms] | 316.94 ms [313.10 ms, 319.86 ms] | 1584.7 [1565.5, 1599.3] | 0.980 [0.876, 1.005] | same |
| ntn_friction_quad4 | mesh | before | 5 | 1 | 323.22 ms [318.20 ms, 361.81 ms] | 323.22 ms [318.20 ms, 361.81 ms] | 1616.1 [1591.0, 1809.1] | – | same |
| ntn_friction_quad4 | init | after | 5 | 1 | 267.75 ms [257.48 ms, 272.86 ms] | 267.75 ms [257.48 ms, 272.86 ms] | 1338.7 [1287.4, 1364.3] | 0.983 [0.886, 1.018] | same |
| ntn_friction_quad4 | init | before | 5 | 1 | 268.15 ms [267.84 ms, 302.06 ms] | 268.15 ms [267.84 ms, 302.06 ms] | 1340.7 [1339.2, 1510.3] | – | same |
| ntn_friction_quad4 | solve_step | after | 5 | 100 | 3.178 s [3.024 s, 3.390 s] | 31.78 ms [30.24 ms, 33.90 ms] | 158.9 [151.2, 169.5] | 0.944 [0.829, 0.955] | same |
| ntn_friction_quad4 | solve_step | before | 5 | 100 | 3.465 s [3.176 s, 3.836 s] | 34.65 ms [31.76 ms, 38.36 ms] | 173.2 [158.8, 191.8] | – | same |
| explicit_mazars_non_local | mesh | after | 5 | 1 | 2.07 ms [2.03 ms, 2.17 ms] | 2.07 ms [2.03 ms, 2.17 ms] | 43.1 [42.3, 45.1] | 0.996 [0.994, 1.010] | same |
| explicit_mazars_non_local | mesh | before | 5 | 1 | 2.08 ms [2.03 ms, 2.15 ms] | 2.08 ms [2.03 ms, 2.15 ms] | 43.4 [42.3, 44.7] | – | same |
| explicit_mazars_non_local | init | after | 5 | 1 | 864.41 ms [864.13 ms, 905.93 ms] | 864.41 ms [864.13 ms, 905.93 ms] | 18008.6 [18002.7, 18873.5] | 0.978 [0.978, 1.014] | same |
| explicit_mazars_non_local | init | before | 5 | 1 | 884.31 ms [883.72 ms, 895.49 ms] | 884.31 ms [883.72 ms, 895.49 ms] | 18423.2 [18410.7, 18656.0] | – | same |
| explicit_mazars_non_local | stable_time_step | after | 5 | 5 | 14.09 ms [14.07 ms, 14.91 ms] | 2.82 ms [2.81 ms, 2.98 ms] | 58.7 [58.6, 62.1] | 0.997 [0.992, 1.056] | same |
| explicit_mazars_non_local | stable_time_step | before | 5 | 5 | 14.14 ms [14.11 ms, 14.19 ms] | 2.83 ms [2.82 ms, 2.84 ms] | 58.9 [58.8, 59.1] | – | same |
| explicit_mazars_non_local | solve_step | after | 5 | 40 | 7.498 s [7.441 s, 7.608 s] | 187.44 ms [186.02 ms, 190.20 ms] | 3905.0 [3875.5, 3962.5] | 1.004 [0.960, 1.022] | same |
| explicit_mazars_non_local | solve_step | before | 5 | 40 | 7.454 s [7.416 s, 7.752 s] | 186.35 ms [185.39 ms, 193.79 ms] | 3882.4 [3862.3, 4037.3] | – | same |
| explicit_mazars_non_local | predictor | after | 5 | 40 | 5.6 µs [5.2 µs, 11.2 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 0.453 [0.230, 1.041] | same |
| explicit_mazars_non_local | predictor | before | 5 | 40 | 11.7 µs [8.6 µs, 24.5 µs] | 0.3 µs [0.2 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | residual | after | 5 | 40 | 7.315 s [7.262 s, 7.423 s] | 182.87 ms [181.54 ms, 185.58 ms] | 3809.8 [3782.2, 3866.3] | 1.004 [0.959, 1.023] | same |
| explicit_mazars_non_local | residual | before | 5 | 40 | 7.273 s [7.236 s, 7.569 s] | 181.81 ms [180.89 ms, 189.23 ms] | 3787.8 [3768.6, 3942.2] | – | same |
| explicit_mazars_non_local | lumped_matrix | after | 5 | 40 | 23.7 µs [18.7 µs, 27.5 µs] | 0.6 µs [0.5 µs, 0.7 µs] | 0.0 [0.0, 0.0] | 0.987 [0.708, 1.005] | same |
| explicit_mazars_non_local | lumped_matrix | before | 5 | 40 | 26.4 µs [23.5 µs, 29.4 µs] | 0.7 µs [0.6 µs, 0.7 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | corrector | after | 5 | 40 | 28.5 µs [22.5 µs, 32.7 µs] | 0.7 µs [0.6 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 1.341 [1.010, 1.467] | same |
| explicit_mazars_non_local | corrector | before | 5 | 40 | 20.8 µs [18.3 µs, 24.4 µs] | 0.5 µs [0.5 µs, 0.6 µs] | 0.0 [0.0, 0.0] | – | same |
| explicit_mazars_non_local | step_hooks | after | 5 | 80 | 167.99 ms [167.26 ms, 171.40 ms] | 2.10 ms [2.09 ms, 2.14 ms] | 43.7 [43.6, 44.6] | 1.002 [0.983, 1.016] | same |
| explicit_mazars_non_local | step_hooks | before | 5 | 80 | 168.61 ms [167.70 ms, 170.12 ms] | 2.11 ms [2.10 ms, 2.13 ms] | 43.9 [43.7, 44.3] | – | same |
| explicit_mazars_non_local | step_rest | after | 5 | 40 | 12.16 ms [11.60 ms, 13.02 ms] | 304.0 µs [289.9 µs, 325.6 µs] | 6.3 [6.0, 6.8] | 1.033 [0.954, 1.064] | same |
| explicit_mazars_non_local | step_rest | before | 5 | 40 | 12.16 ms [11.42 ms, 12.60 ms] | 304.0 µs [285.6 µs, 315.1 µs] | 6.3 [6.0, 6.6] | – | same |
| explicit_mazars_non_local | internal_forces | after | 5 | 8 | 1.494 s [1.479 s, 1.553 s] | 186.74 ms [184.90 ms, 194.10 ms] | 3890.3 [3852.2, 4043.8] | 1.015 [0.972, 1.051] | same |
| explicit_mazars_non_local | internal_forces | before | 5 | 8 | 1.482 s [1.472 s, 1.521 s] | 185.26 ms [184.05 ms, 190.16 ms] | 3859.5 [3834.3, 3961.7] | – | same |
| explicit_mazars_non_local | stresses | after | 5 | 8 | 241.00 ms [235.40 ms, 253.42 ms] | 30.12 ms [29.43 ms, 31.68 ms] | 627.6 [613.0, 660.0] | 1.020 [0.984, 1.082] | same |
| explicit_mazars_non_local | stresses | before | 5 | 8 | 236.06 ms [234.01 ms, 239.22 ms] | 29.51 ms [29.25 ms, 29.90 ms] | 614.7 [609.4, 623.0] | – | same |
| explicit_mazars_non_local | energies | after | 5 | 8 | 32.31 ms [32.28 ms, 33.95 ms] | 4.04 ms [4.04 ms, 4.24 ms] | 84.2 [84.1, 88.4] | 1.000 [0.978, 1.051] | same |
| explicit_mazars_non_local | energies | before | 5 | 8 | 32.32 ms [32.28 ms, 33.01 ms] | 4.04 ms [4.04 ms, 4.13 ms] | 84.2 [84.1, 86.0] | – | same |
| explicit_mazars_non_local | nonlocal_rebuild | after | 5 | 2 | 1.613 s [1.593 s, 1.677 s] | 806.46 ms [796.41 ms, 838.35 ms] | 16801.2 [16591.8, 17465.7] | 0.979 [0.967, 1.025] | same |
| explicit_mazars_non_local | nonlocal_rebuild | before | 5 | 2 | 1.647 s [1.636 s, 1.648 s] | 823.34 ms [818.00 ms, 824.14 ms] | 17152.8 [17041.6, 17169.7] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | 64000 | 65641 | 131282 | 1 | 24f73b94e, c534e539f | e12d9fbc77254935 |
| contact_explicit_quad4 | 126000 | 128222 | 256444 | 1 | 24f73b94e, c534e539f | 73e756218136a9e6 |
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | 24f73b94e, c534e539f | 776a3c6490006b8e |
| contact_search_quad4 | 140000 | 210006 | 420012 | 1 | 24f73b94e, c534e539f | ebe8b1e3825f515d |
| contact_search_tet4 | 480000 | 121686 | 365058 | 1 | 24f73b94e, c534e539f | d5b06d84e1dc501b |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 24f73b94e, c534e539f | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 24f73b94e, c534e539f | 8f333faf172f0935 |
| ntn_friction_quad4 | 200000 | 208052 | 416104 | 1 | 24f73b94e, c534e539f | ce823a5b9b7c5433 |
| explicit_mazars_non_local | 48000 | 9261 | 27783 | 1 | 24f73b94e, c534e539f | 849f4cd05e9ad628 |
