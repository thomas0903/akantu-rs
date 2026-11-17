Suite `solid`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| explicit_marigo | solve_step | base-t1 | 5 | 200 | 1.581 s [1.576 s, 1.721 s] | 7.90 ms [7.88 ms, 8.61 ms] | 74.9 [74.7, 81.6] | – | same |
| explicit_marigo | solve_step | head-t1 | 5 | 200 | 1.513 s [1.509 s, 1.513 s] | 7.57 ms [7.54 ms, 7.57 ms] | 71.7 [71.5, 71.8] | 0.957 [0.877, 0.961] | same |
| explicit_marigo | solve_step | head-t2 | 5 | 200 | 955.69 ms [924.73 ms, 968.47 ms] | 4.78 ms [4.62 ms, 4.84 ms] | 45.3 [43.8, 45.9] | 0.588 [0.555, 0.615] | same |
| explicit_marigo | solve_step | head-t4 | 5 | 200 | 737.63 ms [732.58 ms, 750.15 ms] | 3.69 ms [3.66 ms, 3.75 ms] | 35.0 [34.7, 35.6] | 0.466 [0.436, 0.475] | same |
| explicit_marigo | solve_step | head-t8 | 5 | 200 | 632.03 ms [628.69 ms, 641.60 ms] | 3.16 ms [3.14 ms, 3.21 ms] | 30.0 [29.8, 30.4] | 0.401 [0.365, 0.406] | same |
| explicit_maxwell | solve_step | base-t1 | 5 | 40 | 533.86 ms [531.40 ms, 535.27 ms] | 13.35 ms [13.28 ms, 13.38 ms] | 126.6 [126.0, 126.9] | – | same |
| explicit_maxwell | solve_step | head-t1 | 5 | 40 | 486.50 ms [483.87 ms, 490.39 ms] | 12.16 ms [12.10 ms, 12.26 ms] | 115.3 [114.7, 116.3] | 0.912 [0.906, 0.916] | same |
| explicit_maxwell | solve_step | head-t2 | 5 | 40 | 279.34 ms [275.76 ms, 282.84 ms] | 6.98 ms [6.89 ms, 7.07 ms] | 66.2 [65.4, 67.1] | 0.523 [0.519, 0.531] | same |
| explicit_maxwell | solve_step | head-t4 | 5 | 40 | 200.07 ms [197.13 ms, 200.79 ms] | 5.00 ms [4.93 ms, 5.02 ms] | 47.4 [46.7, 47.6] | 0.376 [0.369, 0.376] | same |
| explicit_maxwell | solve_step | head-t8 | 5 | 40 | 180.77 ms [179.04 ms, 290.52 ms] | 4.52 ms [4.48 ms, 7.26 ms] | 42.9 [42.4, 68.9] | 0.339 [0.337, 0.544] | same |
| explicit_mazars | solve_step | base-t1 | 5 | 80 | 2.697 s [2.692 s, 2.704 s] | 33.72 ms [33.65 ms, 33.79 ms] | 319.7 [319.0, 320.5] | – | same |
| explicit_mazars | solve_step | head-t1 | 5 | 80 | 2.661 s [2.652 s, 2.676 s] | 33.26 ms [33.14 ms, 33.46 ms] | 315.4 [314.3, 317.2] | 0.987 [0.981, 0.990] | same |
| explicit_mazars | solve_step | head-t2 | 5 | 80 | 1.457 s [1.451 s, 1.469 s] | 18.21 ms [18.14 ms, 18.36 ms] | 172.7 [172.0, 174.1] | 0.539 [0.537, 0.544] | same |
| explicit_mazars | solve_step | head-t4 | 5 | 80 | 922.19 ms [920.10 ms, 923.62 ms] | 11.53 ms [11.50 ms, 11.55 ms] | 109.3 [109.1, 109.5] | 0.341 [0.340, 0.343] | same |
| explicit_mazars | solve_step | head-t8 | 5 | 80 | 639.06 ms [634.52 ms, 641.19 ms] | 7.99 ms [7.93 ms, 8.01 ms] | 75.7 [75.2, 76.0] | 0.237 [0.235, 0.237] | same |
| explicit_multi_material | solve_step | base-t1 | 5 | 120 | 2.000 s [1.995 s, 2.007 s] | 16.67 ms [16.62 ms, 16.73 ms] | 158.1 [157.6, 158.6] | – | same |
| explicit_multi_material | solve_step | head-t1 | 5 | 120 | 1.976 s [1.975 s, 2.027 s] | 16.46 ms [16.46 ms, 16.89 ms] | 156.1 [156.1, 160.1] | 0.987 [0.984, 1.016] | same |
| explicit_multi_material | solve_step | head-t2 | 5 | 120 | 1.106 s [1.102 s, 1.111 s] | 9.22 ms [9.18 ms, 9.26 ms] | 87.4 [87.1, 87.8] | 0.552 [0.549, 0.556] | same |
| explicit_multi_material | solve_step | head-t4 | 5 | 120 | 710.95 ms [697.33 ms, 713.43 ms] | 5.92 ms [5.81 ms, 5.95 ms] | 56.2 [55.1, 56.4] | 0.355 [0.349, 0.356] | same |
| explicit_multi_material | solve_step | head-t8 | 5 | 120 | 508.20 ms [506.56 ms, 702.09 ms] | 4.24 ms [4.22 ms, 5.85 ms] | 40.2 [40.0, 55.5] | 0.254 [0.253, 0.350] | same |
| explicit_neohookean | solve_step | base-t1 | 5 | 100 | 1.026 s [1.022 s, 1.032 s] | 10.26 ms [10.22 ms, 10.32 ms] | 97.3 [96.9, 97.9] | – | same |
| explicit_neohookean | solve_step | head-t1 | 5 | 100 | 1.045 s [1.020 s, 1.053 s] | 10.45 ms [10.20 ms, 10.53 ms] | 99.1 [96.8, 99.8] | 1.012 [0.997, 1.028] | same |
| explicit_neohookean | solve_step | head-t2 | 5 | 100 | 602.00 ms [597.41 ms, 628.26 ms] | 6.02 ms [5.97 ms, 6.28 ms] | 57.1 [56.7, 59.6] | 0.587 [0.581, 0.610] | same |
| explicit_neohookean | solve_step | head-t4 | 5 | 100 | 379.29 ms [373.01 ms, 381.03 ms] | 3.79 ms [3.73 ms, 3.81 ms] | 36.0 [35.4, 36.1] | 0.367 [0.362, 0.373] | same |
| explicit_neohookean | solve_step | head-t8 | 5 | 100 | 259.53 ms [256.11 ms, 276.39 ms] | 2.60 ms [2.56 ms, 2.76 ms] | 24.6 [24.3, 26.2] | 0.253 [0.250, 0.268] | same |
| explicit_plastic | solve_step | base-t1 | 5 | 120 | 990.59 ms [987.83 ms, 992.32 ms] | 8.25 ms [8.23 ms, 8.27 ms] | 78.3 [78.1, 78.4] | – | same |
| explicit_plastic | solve_step | head-t1 | 5 | 120 | 980.22 ms [974.17 ms, 1.112 s] | 8.17 ms [8.12 ms, 9.27 ms] | 77.5 [77.0, 87.9] | 0.989 [0.984, 1.126] | same |
| explicit_plastic | solve_step | head-t2 | 5 | 120 | 603.99 ms [601.89 ms, 604.75 ms] | 5.03 ms [5.02 ms, 5.04 ms] | 47.7 [47.6, 47.8] | 0.609 [0.608, 0.612] | same |
| explicit_plastic | solve_step | head-t4 | 5 | 120 | 437.15 ms [395.00 ms, 437.70 ms] | 3.64 ms [3.29 ms, 3.65 ms] | 34.5 [31.2, 34.6] | 0.441 [0.399, 0.443] | same |
| explicit_plastic | solve_step | head-t8 | 5 | 120 | 381.93 ms [380.53 ms, 382.73 ms] | 3.18 ms [3.17 ms, 3.19 ms] | 30.2 [30.1, 30.2] | 0.386 [0.383, 0.386] | same |
| explicit_sls | solve_step | base-t1 | 5 | 120 | 851.19 ms [825.87 ms, 856.47 ms] | 7.09 ms [6.88 ms, 7.14 ms] | 67.3 [65.3, 67.7] | – | same |
| explicit_sls | solve_step | head-t1 | 5 | 120 | 732.58 ms [726.66 ms, 860.55 ms] | 6.10 ms [6.06 ms, 7.17 ms] | 57.9 [57.4, 68.0] | 0.887 [0.849, 1.008] | same |
| explicit_sls | solve_step | head-t2 | 5 | 120 | 438.93 ms [431.70 ms, 439.69 ms] | 3.66 ms [3.60 ms, 3.66 ms] | 34.7 [34.1, 34.7] | 0.516 [0.504, 0.532] | same |
| explicit_sls | solve_step | head-t4 | 5 | 120 | 324.21 ms [303.38 ms, 332.86 ms] | 2.70 ms [2.53 ms, 2.77 ms] | 25.6 [24.0, 26.3] | 0.381 [0.354, 0.395] | same |
| explicit_sls | solve_step | head-t8 | 5 | 120 | 305.69 ms [301.46 ms, 308.93 ms] | 2.55 ms [2.51 ms, 2.57 ms] | 24.2 [23.8, 24.4] | 0.358 [0.355, 0.367] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| explicit_marigo | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | 99c2f9f68e3f3444 |
| explicit_maxwell | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | 67385e0a3d462ed8 |
| explicit_mazars | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | 017c8c9ef7659aa0 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | ce8154043761dbc7 |
| explicit_neohookean | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | aa8ed096eeb974f2 |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | 5c844dbf1dc6d9c8 |
| explicit_sls | 105456 | 19683 | 59049 | 1 | 3443d417b, 8ebac2254 | 36137566d8b810bb |
