Suite `models`, size `m`; variants: `base`, `t1`, `t2`, `t4`, `t8`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 28.77 ms [28.39 ms, 29.63 ms] | 28.77 ms [28.39 ms, 29.63 ms] | 1322.6 [1305.4, 1362.3] | – | same |
| embedded_rebar_tet4 | mesh | t1 | 5 | 1 | 28.58 ms [28.40 ms, 29.05 ms] | 28.58 ms [28.40 ms, 29.05 ms] | 1313.9 [1305.6, 1335.8] | 0.990 [0.964, 1.023] | same |
| embedded_rebar_tet4 | mesh | t2 | 5 | 1 | 28.32 ms [27.88 ms, 29.32 ms] | 28.32 ms [27.88 ms, 29.32 ms] | 1301.9 [1282.0, 1347.8] | 0.994 [0.969, 1.008] | same |
| embedded_rebar_tet4 | mesh | t4 | 5 | 1 | 28.42 ms [27.85 ms, 29.44 ms] | 28.42 ms [27.85 ms, 29.44 ms] | 1306.7 [1280.6, 1353.6] | 1.001 [0.940, 1.022] | same |
| embedded_rebar_tet4 | mesh | t8 | 5 | 1 | 28.53 ms [27.95 ms, 28.72 ms] | 28.53 ms [27.95 ms, 28.72 ms] | 1311.7 [1284.9, 1320.6] | 0.987 [0.969, 1.005] | same |
| embedded_rebar_tet4 | init | base | 5 | 1 | 1.396 s [1.373 s, 1.469 s] | 1.396 s [1.373 s, 1.469 s] | 64186.6 [63141.7, 67556.5] | – | same |
| embedded_rebar_tet4 | init | t1 | 5 | 1 | 1.414 s [1.401 s, 1.489 s] | 1.414 s [1.401 s, 1.489 s] | 65031.0 [64396.2, 68466.1] | 1.007 [0.963, 1.076] | same |
| embedded_rebar_tet4 | init | t2 | 5 | 1 | 1.403 s [1.330 s, 1.482 s] | 1.403 s [1.330 s, 1.482 s] | 64521.0 [61142.6, 68131.9] | 0.977 [0.954, 1.061] | same |
| embedded_rebar_tet4 | init | t4 | 5 | 1 | 1.467 s [1.397 s, 1.494 s] | 1.467 s [1.397 s, 1.494 s] | 67433.6 [64231.1, 68694.0] | 1.004 [0.974, 1.074] | same |
| embedded_rebar_tet4 | init | t8 | 5 | 1 | 1.419 s [1.397 s, 1.446 s] | 1.419 s [1.397 s, 1.446 s] | 65238.9 [64228.6, 66480.0] | 1.011 [0.966, 1.028] | same |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 6.905 s [6.891 s, 7.325 s] | 1.381 s [1.378 s, 1.465 s] | 63494.6 [63366.0, 67353.4] | – | same |
| embedded_rebar_tet4 | intersect | t1 | 5 | 5 | 6.996 s [6.885 s, 7.302 s] | 1.399 s [1.377 s, 1.460 s] | 64335.5 [63308.1, 67142.5] | 1.012 [0.955, 1.058] | same |
| embedded_rebar_tet4 | intersect | t2 | 5 | 5 | 6.951 s [6.903 s, 7.266 s] | 1.390 s [1.381 s, 1.453 s] | 63916.7 [63478.9, 66815.8] | 1.007 [0.942, 1.053] | same |
| embedded_rebar_tet4 | intersect | t4 | 5 | 5 | 6.949 s [6.892 s, 7.078 s] | 1.390 s [1.378 s, 1.416 s] | 63903.0 [63374.8, 65087.0] | 1.004 [0.966, 1.019] | same |
| embedded_rebar_tet4 | intersect | t8 | 5 | 5 | 7.285 s [6.927 s, 11.304 s] | 1.457 s [1.385 s, 2.261 s] | 66985.7 [63695.1, 103940.8] | 1.011 [1.000, 1.638] | same |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 1.087 s [1.082 s, 1.153 s] | 1.087 s [1.082 s, 1.153 s] | 49984.3 [49766.0, 53024.4] | – | same |
| embedded_rebar_tet4 | solve_step | t1 | 5 | 1 | 1.099 s [1.074 s, 1.149 s] | 1.099 s [1.074 s, 1.149 s] | 50507.1 [49371.0, 52832.8] | 0.996 [0.981, 1.053] | same |
| embedded_rebar_tet4 | solve_step | t2 | 5 | 1 | 1.075 s [1.073 s, 1.123 s] | 1.075 s [1.073 s, 1.123 s] | 49402.3 [49322.5, 51647.6] | 0.988 [0.974, 1.034] | same |
| embedded_rebar_tet4 | solve_step | t4 | 5 | 1 | 1.076 s [1.072 s, 1.123 s] | 1.076 s [1.072 s, 1.123 s] | 49480.6 [49272.7, 51632.7] | 0.988 [0.974, 1.026] | same |
| embedded_rebar_tet4 | solve_step | t8 | 5 | 1 | 1.077 s [1.068 s, 1.126 s] | 1.077 s [1.068 s, 1.126 s] | 49497.2 [49102.8, 51788.1] | 0.990 [0.975, 0.997] | same |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 36.55 ms [36.33 ms, 37.22 ms] | 1.83 ms [1.82 ms, 1.86 ms] | 84.0 [83.5, 85.6] | – | same |
| embedded_rebar_tet4 | assemble_internal_forces | t1 | 5 | 20 | 36.16 ms [34.28 ms, 37.00 ms] | 1.81 ms [1.71 ms, 1.85 ms] | 83.1 [78.8, 85.1] | 0.990 [0.938, 1.018] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t2 | 5 | 20 | 25.75 ms [25.64 ms, 26.37 ms] | 1.29 ms [1.28 ms, 1.32 ms] | 59.2 [58.9, 60.6] | 0.703 [0.692, 0.718] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t4 | 5 | 20 | 21.39 ms [21.30 ms, 22.10 ms] | 1.07 ms [1.07 ms, 1.11 ms] | 49.2 [49.0, 50.8] | 0.586 [0.572, 0.596] | same |
| embedded_rebar_tet4 | assemble_internal_forces | t8 | 5 | 20 | 19.11 ms [18.53 ms, 19.31 ms] | 955.7 µs [926.3 µs, 965.6 µs] | 43.9 [42.6, 44.4] | 0.516 [0.507, 0.529] | same |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 166.93 ms [164.38 ms, 202.44 ms] | 33.39 ms [32.88 ms, 40.49 ms] | 1535.0 [1511.5, 1861.6] | – | same |
| embedded_rebar_tet4 | assemble_stiffness | t1 | 5 | 5 | 96.90 ms [95.18 ms, 118.51 ms] | 19.38 ms [19.04 ms, 23.70 ms] | 891.1 [875.2, 1089.7] | 0.583 [0.570, 0.713] | same |
| embedded_rebar_tet4 | assemble_stiffness | t2 | 5 | 5 | 61.21 ms [59.65 ms, 63.49 ms] | 12.24 ms [11.93 ms, 12.70 ms] | 562.8 [548.5, 583.8] | 0.365 [0.314, 0.382] | same |
| embedded_rebar_tet4 | assemble_stiffness | t4 | 5 | 5 | 40.80 ms [39.87 ms, 41.28 ms] | 8.16 ms [7.97 ms, 8.26 ms] | 375.2 [366.6, 379.6] | 0.243 [0.204, 0.250] | same |
| embedded_rebar_tet4 | assemble_stiffness | t8 | 5 | 5 | 31.22 ms [30.72 ms, 31.92 ms] | 6.24 ms [6.14 ms, 6.38 ms] | 287.0 [282.5, 293.5] | 0.188 [0.154, 0.190] | same |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 27.70 ms [27.22 ms, 28.10 ms] | 27.70 ms [27.22 ms, 28.10 ms] | 1099.3 [1080.3, 1115.0] | – | same |
| embedded_rebar_tri3 | mesh | t1 | 5 | 1 | 26.64 ms [24.48 ms, 27.53 ms] | 26.64 ms [24.48 ms, 27.53 ms] | 1057.1 [971.2, 1092.4] | 0.976 [0.883, 0.984] | same |
| embedded_rebar_tri3 | mesh | t2 | 5 | 1 | 27.20 ms [26.71 ms, 27.69 ms] | 27.20 ms [26.71 ms, 27.69 ms] | 1079.3 [1059.9, 1098.7] | 0.981 [0.971, 0.999] | same |
| embedded_rebar_tri3 | mesh | t4 | 5 | 1 | 27.34 ms [27.12 ms, 30.06 ms] | 27.34 ms [27.12 ms, 30.06 ms] | 1085.0 [1076.0, 1193.0] | 0.993 [0.973, 1.075] | same |
| embedded_rebar_tri3 | mesh | t8 | 5 | 1 | 27.20 ms [26.56 ms, 27.77 ms] | 27.20 ms [26.56 ms, 27.77 ms] | 1079.5 [1054.1, 1102.1] | 0.983 [0.973, 0.999] | same |
| embedded_rebar_tri3 | init | base | 5 | 1 | 1.582 s [1.477 s, 1.601 s] | 1.582 s [1.477 s, 1.601 s] | 62758.1 [58616.5, 63539.6] | – | same |
| embedded_rebar_tri3 | init | t1 | 5 | 1 | 1.530 s [1.468 s, 1.594 s] | 1.530 s [1.468 s, 1.594 s] | 60719.4 [58273.3, 63259.4] | 0.983 [0.944, 1.011] | same |
| embedded_rebar_tri3 | init | t2 | 5 | 1 | 1.545 s [1.510 s, 1.574 s] | 1.545 s [1.510 s, 1.574 s] | 61307.2 [59910.8, 62468.5] | 0.983 [0.957, 1.046] | same |
| embedded_rebar_tri3 | init | t4 | 5 | 1 | 1.571 s [1.483 s, 1.604 s] | 1.571 s [1.483 s, 1.604 s] | 62353.0 [58839.0, 63634.9] | 0.993 [0.962, 1.014] | same |
| embedded_rebar_tri3 | init | t8 | 5 | 1 | 1.536 s [1.489 s, 1.590 s] | 1.536 s [1.489 s, 1.590 s] | 60954.4 [59090.5, 63093.1] | 0.983 [0.952, 1.008] | same |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 7.553 s [7.518 s, 7.773 s] | 1.511 s [1.504 s, 1.555 s] | 59945.1 [59667.7, 61692.0] | – | same |
| embedded_rebar_tri3 | intersect | t1 | 5 | 5 | 7.725 s [7.499 s, 7.950 s] | 1.545 s [1.500 s, 1.590 s] | 61310.3 [59517.7, 63092.8] | 1.003 [0.993, 1.048] | same |
| embedded_rebar_tri3 | intersect | t2 | 5 | 5 | 7.547 s [7.386 s, 15.720 s] | 1.509 s [1.477 s, 3.144 s] | 59898.9 [58617.5, 124758.7] | 1.004 [0.950, 2.071] | same |
| embedded_rebar_tri3 | intersect | t4 | 5 | 5 | 7.562 s [7.532 s, 7.786 s] | 1.512 s [1.506 s, 1.557 s] | 60012.4 [59779.1, 61795.2] | 1.004 [0.986, 1.031] | same |
| embedded_rebar_tri3 | intersect | t8 | 5 | 5 | 7.722 s [7.498 s, 7.990 s] | 1.544 s [1.500 s, 1.598 s] | 61282.4 [59504.2, 63414.9] | 1.001 [0.993, 1.053] | same |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 255.55 ms [248.79 ms, 265.32 ms] | 255.55 ms [248.79 ms, 265.32 ms] | 10140.9 [9872.7, 10528.8] | – | same |
| embedded_rebar_tri3 | solve_step | t1 | 5 | 1 | 255.42 ms [231.60 ms, 259.24 ms] | 255.42 ms [231.60 ms, 259.24 ms] | 10135.6 [9190.3, 10287.1] | 0.977 [0.931, 1.008] | same |
| embedded_rebar_tri3 | solve_step | t2 | 5 | 1 | 249.20 ms [246.14 ms, 258.68 ms] | 249.20 ms [246.14 ms, 258.68 ms] | 9889.0 [9767.4, 10265.2] | 0.975 [0.961, 0.997] | same |
| embedded_rebar_tri3 | solve_step | t4 | 5 | 1 | 250.72 ms [244.95 ms, 259.74 ms] | 250.72 ms [244.95 ms, 259.74 ms] | 9949.4 [9720.2, 10307.3] | 0.979 [0.966, 0.990] | same |
| embedded_rebar_tri3 | solve_step | t8 | 5 | 1 | 251.45 ms [246.58 ms, 258.60 ms] | 251.45 ms [246.58 ms, 258.60 ms] | 9978.4 [9784.9, 10261.7] | 0.984 [0.969, 0.991] | same |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 49.67 ms [48.65 ms, 51.02 ms] | 2.48 ms [2.43 ms, 2.55 ms] | 98.6 [96.5, 101.2] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | t1 | 5 | 20 | 48.92 ms [47.11 ms, 52.65 ms] | 2.45 ms [2.36 ms, 2.63 ms] | 97.1 [93.5, 104.5] | 0.968 [0.957, 1.065] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t2 | 5 | 20 | 42.48 ms [41.15 ms, 42.69 ms] | 2.12 ms [2.06 ms, 2.13 ms] | 84.3 [81.7, 84.7] | 0.856 [0.811, 0.878] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t4 | 5 | 20 | 39.97 ms [39.67 ms, 40.64 ms] | 2.00 ms [1.98 ms, 2.03 ms] | 79.3 [78.7, 80.6] | 0.807 [0.783, 0.818] | same |
| embedded_rebar_tri3 | assemble_internal_forces | t8 | 5 | 20 | 38.78 ms [38.70 ms, 38.96 ms] | 1.94 ms [1.94 ms, 1.95 ms] | 76.9 [76.8, 77.3] | 0.784 [0.760, 0.797] | same |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 47.48 ms [45.46 ms, 49.33 ms] | 9.50 ms [9.09 ms, 9.87 ms] | 376.8 [360.8, 391.5] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | t1 | 5 | 5 | 40.77 ms [37.27 ms, 43.11 ms] | 8.15 ms [7.45 ms, 8.62 ms] | 323.6 [295.8, 342.2] | 0.864 [0.785, 0.929] | same |
| embedded_rebar_tri3 | assemble_stiffness | t2 | 5 | 5 | 32.52 ms [31.32 ms, 33.08 ms] | 6.50 ms [6.26 ms, 6.62 ms] | 258.1 [248.6, 262.6] | 0.697 [0.644, 0.722] | same |
| embedded_rebar_tri3 | assemble_stiffness | t4 | 5 | 5 | 28.36 ms [28.09 ms, 28.82 ms] | 5.67 ms [5.62 ms, 5.76 ms] | 225.1 [223.0, 228.7] | 0.592 [0.575, 0.634] | same |
| embedded_rebar_tri3 | assemble_stiffness | t8 | 5 | 5 | 26.47 ms [26.37 ms, 26.62 ms] | 5.29 ms [5.27 ms, 5.32 ms] | 210.1 [209.3, 211.2] | 0.561 [0.537, 0.582] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | 72337cb4e, f28c5a7bc | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 72337cb4e, f28c5a7bc | 8f333faf172f0935 |
