Suite `models`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_implicit_hex8_kvar | mesh | base-t1 | 5 | 1 | 5.03 ms [5.01 ms, 5.05 ms] | 5.03 ms [5.01 ms, 5.05 ms] | 2909.1 [2897.6, 2923.6] | – | same |
| heat_implicit_hex8_kvar | mesh | head-t1 | 5 | 1 | 4.97 ms [4.96 ms, 5.07 ms] | 4.97 ms [4.96 ms, 5.07 ms] | 2877.3 [2867.5, 2931.7] | 0.990 [0.987, 1.003] | same |
| heat_implicit_hex8_kvar | mesh | head-t2 | 5 | 1 | 4.98 ms [4.96 ms, 5.24 ms] | 4.98 ms [4.96 ms, 5.24 ms] | 2884.8 [2867.5, 3033.6] | 0.992 [0.986, 1.038] | same |
| heat_implicit_hex8_kvar | mesh | head-t4 | 5 | 1 | 4.98 ms [4.97 ms, 5.12 ms] | 4.98 ms [4.97 ms, 5.12 ms] | 2882.5 [2874.4, 2964.7] | 0.990 [0.983, 1.023] | same |
| heat_implicit_hex8_kvar | mesh | head-t8 | 5 | 1 | 4.98 ms [4.97 ms, 5.03 ms] | 4.98 ms [4.97 ms, 5.03 ms] | 2883.7 [2875.0, 2909.1] | 0.989 [0.987, 1.004] | same |
| heat_implicit_hex8_kvar | init | base-t1 | 5 | 1 | 994.0 µs [985.0 µs, 995.0 µs] | 994.0 µs [985.0 µs, 995.0 µs] | 575.2 [570.0, 575.8] | – | same |
| heat_implicit_hex8_kvar | init | head-t1 | 5 | 1 | 960.0 µs [953.0 µs, 1.00 ms] | 960.0 µs [953.0 µs, 1.00 ms] | 555.6 [551.5, 578.7] | 0.972 [0.959, 1.005] | same |
| heat_implicit_hex8_kvar | init | head-t2 | 5 | 1 | 990.0 µs [967.0 µs, 1.00 ms] | 990.0 µs [967.0 µs, 1.00 ms] | 572.9 [559.6, 578.7] | 0.996 [0.973, 1.005] | same |
| heat_implicit_hex8_kvar | init | head-t4 | 5 | 1 | 976.0 µs [948.0 µs, 999.0 µs] | 976.0 µs [948.0 µs, 999.0 µs] | 564.8 [548.6, 578.1] | 0.981 [0.954, 1.004] | same |
| heat_implicit_hex8_kvar | init | head-t8 | 5 | 1 | 1.03 ms [986.0 µs, 1.05 ms] | 1.03 ms [986.0 µs, 1.05 ms] | 597.2 [570.6, 607.1] | 1.037 [0.991, 1.055] | same |
| heat_implicit_hex8_kvar | solve_step | base-t1 | 5 | 5 | 592.30 ms [590.63 ms, 594.18 ms] | 118.46 ms [118.13 ms, 118.84 ms] | 68553.6 [68360.3, 68771.4] | – | same |
| heat_implicit_hex8_kvar | solve_step | head-t1 | 5 | 5 | 595.49 ms [592.46 ms, 598.46 ms] | 119.10 ms [118.49 ms, 119.69 ms] | 68922.8 [68571.8, 69265.7] | 1.005 [1.001, 1.011] | same |
| heat_implicit_hex8_kvar | solve_step | head-t2 | 5 | 5 | 588.35 ms [584.97 ms, 589.51 ms] | 117.67 ms [116.99 ms, 117.90 ms] | 68096.4 [67705.2, 68230.6] | 0.993 [0.988, 0.996] | same |
| heat_implicit_hex8_kvar | solve_step | head-t4 | 5 | 5 | 588.95 ms [586.85 ms, 591.04 ms] | 117.79 ms [117.37 ms, 118.21 ms] | 68166.0 [67922.6, 68407.6] | 0.994 [0.993, 0.998] | same |
| heat_implicit_hex8_kvar | solve_step | head-t8 | 5 | 5 | 594.69 ms [590.80 ms, 597.78 ms] | 118.94 ms [118.16 ms, 119.56 ms] | 68830.1 [68380.0, 69187.8] | 1.006 [0.997, 1.009] | same |
| phasefield_staggered_tet4 | mesh | base-t1 | 5 | 1 | 9.28 ms [9.25 ms, 9.33 ms] | 9.28 ms [9.25 ms, 9.33 ms] | 1547.2 [1542.0, 1554.5] | – | same |
| phasefield_staggered_tet4 | mesh | head-t1 | 5 | 1 | 9.30 ms [9.23 ms, 9.42 ms] | 9.30 ms [9.23 ms, 9.42 ms] | 1549.5 [1538.7, 1569.3] | 1.002 [0.995, 1.010] | same |
| phasefield_staggered_tet4 | mesh | head-t2 | 5 | 1 | 9.29 ms [9.23 ms, 9.33 ms] | 9.29 ms [9.23 ms, 9.33 ms] | 1548.2 [1537.8, 1555.5] | 0.997 [0.990, 1.009] | same |
| phasefield_staggered_tet4 | mesh | head-t4 | 5 | 1 | 9.32 ms [9.28 ms, 9.35 ms] | 9.32 ms [9.28 ms, 9.35 ms] | 1552.7 [1547.2, 1558.3] | 1.002 [1.000, 1.007] | same |
| phasefield_staggered_tet4 | mesh | head-t8 | 5 | 1 | 9.31 ms [9.23 ms, 9.32 ms] | 9.31 ms [9.23 ms, 9.32 ms] | 1550.8 [1537.5, 1554.2] | 1.000 [0.994, 1.005] | same |
| phasefield_staggered_tet4 | init | base-t1 | 5 | 1 | 2.97 ms [2.96 ms, 2.99 ms] | 2.97 ms [2.96 ms, 2.99 ms] | 495.3 [494.0, 497.8] | – | same |
| phasefield_staggered_tet4 | init | head-t1 | 5 | 1 | 2.93 ms [2.93 ms, 3.00 ms] | 2.93 ms [2.93 ms, 3.00 ms] | 489.0 [489.0, 499.3] | 0.990 [0.987, 1.003] | same |
| phasefield_staggered_tet4 | init | head-t2 | 5 | 1 | 3.00 ms [2.95 ms, 3.05 ms] | 3.00 ms [2.95 ms, 3.05 ms] | 499.8 [491.3, 508.3] | 1.009 [0.993, 1.021] | same |
| phasefield_staggered_tet4 | init | head-t4 | 5 | 1 | 2.99 ms [2.98 ms, 3.03 ms] | 2.99 ms [2.98 ms, 3.03 ms] | 498.7 [497.0, 504.5] | 1.008 [1.002, 1.018] | same |
| phasefield_staggered_tet4 | init | head-t8 | 5 | 1 | 3.01 ms [2.97 ms, 3.12 ms] | 3.01 ms [2.97 ms, 3.12 ms] | 501.8 [495.5, 520.2] | 1.016 [0.995, 1.049] | same |
| phasefield_staggered_tet4 | solid_solve | base-t1 | 5 | 4 | 578.58 ms [575.86 ms, 584.59 ms] | 144.64 ms [143.96 ms, 146.15 ms] | 24107.5 [23994.0, 24358.0] | – | same |
| phasefield_staggered_tet4 | solid_solve | head-t1 | 5 | 4 | 584.46 ms [581.00 ms, 590.42 ms] | 146.12 ms [145.25 ms, 147.60 ms] | 24352.6 [24208.2, 24600.6] | 1.009 [0.994, 1.020] | same |
| phasefield_staggered_tet4 | solid_solve | head-t2 | 5 | 4 | 551.83 ms [548.26 ms, 566.03 ms] | 137.96 ms [137.07 ms, 141.51 ms] | 22993.0 [22844.3, 23584.7] | 0.956 [0.946, 0.968] | same |
| phasefield_staggered_tet4 | solid_solve | head-t4 | 5 | 4 | 533.05 ms [530.27 ms, 537.83 ms] | 133.26 ms [132.57 ms, 134.46 ms] | 22210.5 [22094.6, 22409.5] | 0.925 [0.909, 0.928] | same |
| phasefield_staggered_tet4 | solid_solve | head-t8 | 5 | 4 | 532.89 ms [525.94 ms, 556.01 ms] | 133.22 ms [131.49 ms, 139.00 ms] | 22203.8 [21914.3, 23167.1] | 0.919 [0.905, 0.961] | same |
| phasefield_staggered_tet4 | strain_transfer | base-t1 | 5 | 4 | 701.0 µs [686.0 µs, 710.0 µs] | 175.2 µs [171.5 µs, 177.5 µs] | 29.2 [28.6, 29.6] | – | same |
| phasefield_staggered_tet4 | strain_transfer | head-t1 | 5 | 4 | 701.0 µs [684.0 µs, 713.0 µs] | 175.2 µs [171.0 µs, 178.2 µs] | 29.2 [28.5, 29.7] | 0.993 [0.972, 1.022] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t2 | 5 | 4 | 691.0 µs [672.0 µs, 709.0 µs] | 172.8 µs [168.0 µs, 177.2 µs] | 28.8 [28.0, 29.5] | 0.994 [0.959, 1.007] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t4 | 5 | 4 | 692.0 µs [677.0 µs, 705.0 µs] | 173.0 µs [169.2 µs, 176.2 µs] | 28.8 [28.2, 29.4] | 0.993 [0.966, 1.001] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t8 | 5 | 4 | 692.0 µs [681.0 µs, 695.0 µs] | 173.0 µs [170.2 µs, 173.8 µs] | 28.8 [28.4, 29.0] | 0.984 [0.971, 1.009] | same |
| phasefield_staggered_tet4 | phase_solve | base-t1 | 5 | 4 | 69.80 ms [69.51 ms, 70.16 ms] | 17.45 ms [17.38 ms, 17.54 ms] | 2908.5 [2896.1, 2923.3] | – | same |
| phasefield_staggered_tet4 | phase_solve | head-t1 | 5 | 4 | 69.55 ms [69.35 ms, 70.12 ms] | 17.39 ms [17.34 ms, 17.53 ms] | 2897.9 [2889.6, 2921.7] | 0.996 [0.988, 1.007] | same |
| phasefield_staggered_tet4 | phase_solve | head-t2 | 5 | 4 | 69.93 ms [69.49 ms, 70.18 ms] | 17.48 ms [17.37 ms, 17.55 ms] | 2913.8 [2895.3, 2924.2] | 1.000 [0.992, 1.006] | same |
| phasefield_staggered_tet4 | phase_solve | head-t4 | 5 | 4 | 70.02 ms [69.89 ms, 70.58 ms] | 17.51 ms [17.47 ms, 17.64 ms] | 2917.7 [2911.9, 2940.8] | 1.002 [0.999, 1.011] | same |
| phasefield_staggered_tet4 | phase_solve | head-t8 | 5 | 4 | 70.91 ms [70.42 ms, 74.95 ms] | 17.73 ms [17.61 ms, 18.74 ms] | 2954.7 [2934.4, 3123.1] | 1.011 [1.009, 1.071] | same |
| phasefield_staggered_tet4 | damage_transfer | base-t1 | 5 | 4 | 1.57 ms [1.56 ms, 1.59 ms] | 393.2 µs [390.2 µs, 397.2 µs] | 65.5 [65.0, 66.2] | – | same |
| phasefield_staggered_tet4 | damage_transfer | head-t1 | 5 | 4 | 1.57 ms [1.56 ms, 1.60 ms] | 393.2 µs [389.5 µs, 399.2 µs] | 65.5 [64.9, 66.5] | 1.003 [0.982, 1.015] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t2 | 5 | 4 | 1.56 ms [1.54 ms, 1.59 ms] | 391.2 µs [385.8 µs, 396.8 µs] | 65.2 [64.3, 66.1] | 0.995 [0.982, 1.006] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t4 | 5 | 4 | 1.56 ms [1.55 ms, 1.57 ms] | 390.0 µs [387.5 µs, 393.5 µs] | 65.0 [64.6, 65.6] | 0.999 [0.975, 1.001] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t8 | 5 | 4 | 1.57 ms [1.55 ms, 1.58 ms] | 391.8 µs [387.2 µs, 395.2 µs] | 65.3 [64.5, 65.9] | 0.995 [0.985, 1.004] | same |
| phasefield_staggered_tet4 | internal_forces | base-t1 | 5 | 4 | 1.92 ms [1.89 ms, 2.16 ms] | 481.0 µs [471.2 µs, 540.2 µs] | 80.2 [78.5, 90.0] | – | same |
| phasefield_staggered_tet4 | internal_forces | head-t1 | 5 | 4 | 2.10 ms [2.06 ms, 2.10 ms] | 524.0 µs [514.0 µs, 526.2 µs] | 87.3 [85.7, 87.7] | 1.088 [0.974, 1.091] | same |
| phasefield_staggered_tet4 | internal_forces | head-t2 | 5 | 4 | 1.52 ms [1.49 ms, 1.56 ms] | 379.5 µs [372.0 µs, 389.2 µs] | 63.2 [62.0, 64.9] | 0.784 [0.720, 0.815] | same |
| phasefield_staggered_tet4 | internal_forces | head-t4 | 5 | 4 | 1.40 ms [1.36 ms, 1.44 ms] | 350.2 µs [341.2 µs, 360.2 µs] | 58.4 [56.9, 60.0] | 0.728 [0.648, 0.764] | same |
| phasefield_staggered_tet4 | internal_forces | head-t8 | 5 | 4 | 1.39 ms [1.36 ms, 1.39 ms] | 346.5 µs [339.8 µs, 348.0 µs] | 57.8 [56.6, 58.0] | 0.708 [0.644, 0.735] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t1 | 5 | 5 | 59.31 ms [59.08 ms, 59.47 ms] | 11.86 ms [11.82 ms, 11.89 ms] | 1977.1 [1969.3, 1982.4] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t1 | 5 | 5 | 59.30 ms [59.17 ms, 59.48 ms] | 11.86 ms [11.83 ms, 11.90 ms] | 1976.8 [1972.4, 1982.7] | 1.000 [0.997, 1.007] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t2 | 5 | 5 | 54.13 ms [54.03 ms, 54.76 ms] | 10.83 ms [10.81 ms, 10.95 ms] | 1804.4 [1800.9, 1825.4] | 0.911 [0.909, 0.927] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t4 | 5 | 5 | 54.02 ms [53.69 ms, 54.87 ms] | 10.80 ms [10.74 ms, 10.97 ms] | 1800.7 [1789.6, 1829.0] | 0.909 [0.903, 0.929] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t8 | 5 | 5 | 58.90 ms [58.71 ms, 60.03 ms] | 11.78 ms [11.74 ms, 12.01 ms] | 1963.3 [1957.0, 2001.0] | 0.995 [0.987, 1.013] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t1 | 5 | 5 | 9.39 ms [9.34 ms, 9.44 ms] | 1.88 ms [1.87 ms, 1.89 ms] | 313.1 [311.4, 314.7] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t1 | 5 | 5 | 9.40 ms [9.37 ms, 9.42 ms] | 1.88 ms [1.87 ms, 1.88 ms] | 313.3 [312.4, 313.9] | 0.999 [0.993, 1.006] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t2 | 5 | 5 | 9.40 ms [9.37 ms, 9.44 ms] | 1.88 ms [1.87 ms, 1.89 ms] | 313.3 [312.2, 314.8] | 0.997 [0.994, 1.006] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t4 | 5 | 5 | 9.39 ms [9.36 ms, 10.20 ms] | 1.88 ms [1.87 ms, 2.04 ms] | 313.0 [312.1, 340.0] | 1.000 [0.993, 1.092] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t8 | 5 | 5 | 9.45 ms [9.38 ms, 9.52 ms] | 1.89 ms [1.88 ms, 1.90 ms] | 314.9 [312.8, 317.4] | 1.006 [1.001, 1.009] | same |
| phasefield_staggered_tri3 | mesh | base-t1 | 5 | 1 | 59.81 ms [59.41 ms, 60.14 ms] | 59.81 ms [59.41 ms, 60.14 ms] | 1168.3 [1160.4, 1174.6] | – | same |
| phasefield_staggered_tri3 | mesh | head-t1 | 5 | 1 | 60.03 ms [59.88 ms, 60.43 ms] | 60.03 ms [59.88 ms, 60.43 ms] | 1172.5 [1169.5, 1180.4] | 1.007 [0.998, 1.010] | same |
| phasefield_staggered_tri3 | mesh | head-t2 | 5 | 1 | 59.91 ms [59.65 ms, 60.32 ms] | 59.91 ms [59.65 ms, 60.32 ms] | 1170.1 [1165.0, 1178.1] | 1.005 [0.992, 1.007] | same |
| phasefield_staggered_tri3 | mesh | head-t4 | 5 | 1 | 59.99 ms [59.85 ms, 60.17 ms] | 59.99 ms [59.85 ms, 60.17 ms] | 1171.6 [1168.9, 1175.2] | 1.003 [0.995, 1.013] | same |
| phasefield_staggered_tri3 | mesh | head-t8 | 5 | 1 | 59.77 ms [59.66 ms, 60.41 ms] | 59.77 ms [59.66 ms, 60.41 ms] | 1167.3 [1165.2, 1179.9] | 1.004 [0.992, 1.013] | same |
| phasefield_staggered_tri3 | init | base-t1 | 5 | 1 | 19.20 ms [18.93 ms, 20.28 ms] | 19.20 ms [18.93 ms, 20.28 ms] | 375.1 [369.8, 396.2] | – | same |
| phasefield_staggered_tri3 | init | head-t1 | 5 | 1 | 19.14 ms [19.10 ms, 19.26 ms] | 19.14 ms [19.10 ms, 19.26 ms] | 373.9 [373.1, 376.2] | 0.996 [0.947, 1.017] | same |
| phasefield_staggered_tri3 | init | head-t2 | 5 | 1 | 18.90 ms [18.81 ms, 19.13 ms] | 18.90 ms [18.81 ms, 19.13 ms] | 369.1 [367.4, 373.7] | 0.989 [0.928, 1.009] | same |
| phasefield_staggered_tri3 | init | head-t4 | 5 | 1 | 19.11 ms [19.08 ms, 19.46 ms] | 19.11 ms [19.08 ms, 19.46 ms] | 373.3 [372.7, 380.1] | 1.000 [0.942, 1.015] | same |
| phasefield_staggered_tri3 | init | head-t8 | 5 | 1 | 19.38 ms [18.97 ms, 19.94 ms] | 19.38 ms [18.97 ms, 19.94 ms] | 378.5 [370.6, 389.5] | 1.003 [0.939, 1.038] | same |
| phasefield_staggered_tri3 | solid_solve | base-t1 | 5 | 4 | 3.059 s [3.044 s, 3.077 s] | 764.78 ms [760.96 ms, 769.37 ms] | 14937.1 [14862.6, 15026.7] | – | same |
| phasefield_staggered_tri3 | solid_solve | head-t1 | 5 | 4 | 3.090 s [3.046 s, 3.153 s] | 772.60 ms [761.43 ms, 788.29 ms] | 15089.8 [14871.6, 15396.2] | 1.013 [0.990, 1.031] | same |
| phasefield_staggered_tri3 | solid_solve | head-t2 | 5 | 4 | 2.887 s [2.873 s, 2.905 s] | 721.66 ms [718.13 ms, 726.24 ms] | 14094.8 [14025.9, 14184.4] | 0.944 [0.938, 0.950] | same |
| phasefield_staggered_tri3 | solid_solve | head-t4 | 5 | 4 | 2.794 s [2.789 s, 2.822 s] | 698.43 ms [697.28 ms, 705.53 ms] | 13641.2 [13618.8, 13780.0] | 0.915 [0.909, 0.923] | same |
| phasefield_staggered_tri3 | solid_solve | head-t8 | 5 | 4 | 2.776 s [2.762 s, 2.785 s] | 693.90 ms [690.44 ms, 696.32 ms] | 13552.8 [13485.1, 13600.0] | 0.906 [0.902, 0.915] | same |
| phasefield_staggered_tri3 | strain_transfer | base-t1 | 5 | 4 | 4.14 ms [4.11 ms, 4.19 ms] | 1.03 ms [1.03 ms, 1.05 ms] | 20.2 [20.1, 20.5] | – | same |
| phasefield_staggered_tri3 | strain_transfer | head-t1 | 5 | 4 | 4.02 ms [3.98 ms, 4.11 ms] | 1.00 ms [994.8 µs, 1.03 ms] | 19.6 [19.4, 20.0] | 0.971 [0.956, 0.996] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t2 | 5 | 4 | 4.05 ms [4.03 ms, 4.13 ms] | 1.01 ms [1.01 ms, 1.03 ms] | 19.8 [19.7, 20.2] | 0.984 [0.966, 0.994] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t4 | 5 | 4 | 4.04 ms [4.02 ms, 4.05 ms] | 1.01 ms [1.01 ms, 1.01 ms] | 19.7 [19.6, 19.8] | 0.975 [0.967, 0.980] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t8 | 5 | 4 | 4.06 ms [4.02 ms, 4.09 ms] | 1.02 ms [1.01 ms, 1.02 ms] | 19.8 [19.6, 20.0] | 0.977 [0.965, 0.992] | same |
| phasefield_staggered_tri3 | phase_solve | base-t1 | 5 | 4 | 824.29 ms [823.01 ms, 826.05 ms] | 206.07 ms [205.75 ms, 206.51 ms] | 4024.9 [4018.6, 4033.4] | – | same |
| phasefield_staggered_tri3 | phase_solve | head-t1 | 5 | 4 | 823.71 ms [823.05 ms, 855.97 ms] | 205.93 ms [205.76 ms, 213.99 ms] | 4022.0 [4018.8, 4179.5] | 1.000 [0.998, 1.038] | same |
| phasefield_staggered_tri3 | phase_solve | head-t2 | 5 | 4 | 818.83 ms [817.09 ms, 820.72 ms] | 204.71 ms [204.27 ms, 205.18 ms] | 3998.2 [3989.7, 4007.4] | 0.994 [0.991, 0.996] | same |
| phasefield_staggered_tri3 | phase_solve | head-t4 | 5 | 4 | 812.06 ms [808.81 ms, 812.64 ms] | 203.02 ms [202.20 ms, 203.16 ms] | 3965.1 [3949.3, 3968.0] | 0.985 [0.981, 0.986] | same |
| phasefield_staggered_tri3 | phase_solve | head-t8 | 5 | 4 | 813.45 ms [810.47 ms, 818.35 ms] | 203.36 ms [202.62 ms, 204.59 ms] | 3971.9 [3957.4, 3995.9] | 0.987 [0.985, 0.993] | same |
| phasefield_staggered_tri3 | damage_transfer | base-t1 | 5 | 4 | 11.96 ms [11.92 ms, 12.21 ms] | 2.99 ms [2.98 ms, 3.05 ms] | 58.4 [58.2, 59.6] | – | same |
| phasefield_staggered_tri3 | damage_transfer | head-t1 | 5 | 4 | 11.96 ms [11.93 ms, 12.03 ms] | 2.99 ms [2.98 ms, 3.01 ms] | 58.4 [58.3, 58.7] | 1.000 [0.985, 1.008] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t2 | 5 | 4 | 12.02 ms [11.91 ms, 12.05 ms] | 3.01 ms [2.98 ms, 3.01 ms] | 58.7 [58.1, 58.8] | 1.000 [0.982, 1.010] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t4 | 5 | 4 | 11.92 ms [11.89 ms, 11.95 ms] | 2.98 ms [2.97 ms, 2.99 ms] | 58.2 [58.1, 58.3] | 0.997 [0.976, 0.999] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t8 | 5 | 4 | 11.95 ms [11.91 ms, 12.06 ms] | 2.99 ms [2.98 ms, 3.01 ms] | 58.3 [58.1, 58.9] | 0.997 [0.978, 1.010] | same |
| phasefield_staggered_tri3 | internal_forces | base-t1 | 5 | 4 | 10.32 ms [10.29 ms, 10.52 ms] | 2.58 ms [2.57 ms, 2.63 ms] | 50.4 [50.2, 51.4] | – | same |
| phasefield_staggered_tri3 | internal_forces | head-t1 | 5 | 4 | 10.41 ms [10.32 ms, 10.57 ms] | 2.60 ms [2.58 ms, 2.64 ms] | 50.8 [50.4, 51.6] | 1.004 [1.002, 1.012] | same |
| phasefield_staggered_tri3 | internal_forces | head-t2 | 5 | 4 | 6.41 ms [6.41 ms, 6.60 ms] | 1.60 ms [1.60 ms, 1.65 ms] | 31.3 [31.3, 32.2] | 0.623 [0.617, 0.630] | same |
| phasefield_staggered_tri3 | internal_forces | head-t4 | 5 | 4 | 5.05 ms [5.05 ms, 5.14 ms] | 1.26 ms [1.26 ms, 1.28 ms] | 24.7 [24.7, 25.1] | 0.489 [0.486, 0.499] | same |
| phasefield_staggered_tri3 | internal_forces | head-t8 | 5 | 4 | 4.84 ms [4.77 ms, 5.04 ms] | 1.21 ms [1.19 ms, 1.26 ms] | 23.6 [23.3, 24.6] | 0.470 [0.453, 0.488] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t1 | 5 | 5 | 72.62 ms [72.36 ms, 72.74 ms] | 14.53 ms [14.47 ms, 14.55 ms] | 283.7 [282.7, 284.2] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t1 | 5 | 5 | 72.58 ms [72.14 ms, 72.94 ms] | 14.52 ms [14.43 ms, 14.59 ms] | 283.5 [281.8, 284.9] | 0.999 [0.994, 1.003] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t2 | 5 | 5 | 70.20 ms [69.66 ms, 70.56 ms] | 14.04 ms [13.93 ms, 14.11 ms] | 274.2 [272.1, 275.6] | 0.968 [0.959, 0.973] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t4 | 5 | 5 | 69.12 ms [68.53 ms, 69.33 ms] | 13.82 ms [13.71 ms, 13.87 ms] | 270.0 [267.7, 270.8] | 0.953 [0.942, 0.955] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t8 | 5 | 5 | 69.09 ms [67.88 ms, 69.39 ms] | 13.82 ms [13.58 ms, 13.88 ms] | 269.9 [265.1, 271.1] | 0.950 [0.935, 0.955] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t1 | 5 | 5 | 69.27 ms [68.73 ms, 70.40 ms] | 13.85 ms [13.75 ms, 14.08 ms] | 270.6 [268.5, 275.0] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t1 | 5 | 5 | 69.39 ms [68.77 ms, 70.22 ms] | 13.88 ms [13.75 ms, 14.04 ms] | 271.1 [268.6, 274.3] | 1.002 [0.986, 1.014] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t2 | 5 | 5 | 78.98 ms [78.86 ms, 79.28 ms] | 15.80 ms [15.77 ms, 15.86 ms] | 308.5 [308.0, 309.7] | 1.142 [1.122, 1.149] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t4 | 5 | 5 | 68.83 ms [68.77 ms, 69.20 ms] | 13.77 ms [13.75 ms, 13.84 ms] | 268.9 [268.6, 270.3] | 0.994 [0.978, 1.003] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t8 | 5 | 5 | 69.16 ms [69.03 ms, 69.32 ms] | 13.83 ms [13.81 ms, 13.86 ms] | 270.2 [269.7, 270.8] | 1.001 [0.984, 1.004] | same |
| static_linear_eigen | mesh | base-t1 | 5 | 1 | 809.5 µs [787.3 µs, 814.9 µs] | 809.5 µs [787.3 µs, 814.9 µs] | 49.2 [47.8, 49.5] | – | same |
| static_linear_eigen | mesh | head-t1 | 5 | 1 | 816.4 µs [798.7 µs, 836.5 µs] | 816.4 µs [798.7 µs, 836.5 µs] | 49.6 [48.5, 50.8] | 1.011 [1.002, 1.052] | same |
| static_linear_eigen | mesh | head-t2 | 5 | 1 | 816.7 µs [787.3 µs, 823.7 µs] | 816.7 µs [787.3 µs, 823.7 µs] | 49.6 [47.8, 50.0] | 1.009 [0.990, 1.017] | same |
| static_linear_eigen | mesh | head-t4 | 5 | 1 | 816.2 µs [797.1 µs, 822.9 µs] | 816.2 µs [797.1 µs, 822.9 µs] | 49.6 [48.4, 50.0] | 1.008 [1.003, 1.044] | same |
| static_linear_eigen | mesh | head-t8 | 5 | 1 | 809.9 µs [792.4 µs, 821.1 µs] | 809.9 µs [792.4 µs, 821.1 µs] | 49.2 [48.1, 49.9] | 1.006 [0.979, 1.033] | same |
| static_linear_eigen | init | base-t1 | 5 | 1 | 9.19 ms [9.14 ms, 9.22 ms] | 9.19 ms [9.14 ms, 9.22 ms] | 558.2 [555.0, 560.1] | – | same |
| static_linear_eigen | init | head-t1 | 5 | 1 | 9.24 ms [9.19 ms, 9.29 ms] | 9.24 ms [9.19 ms, 9.29 ms] | 561.0 [558.2, 564.0] | 1.005 [1.001, 1.012] | same |
| static_linear_eigen | init | head-t2 | 5 | 1 | 9.48 ms [9.43 ms, 9.70 ms] | 9.48 ms [9.43 ms, 9.70 ms] | 575.7 [572.6, 589.1] | 1.037 [1.027, 1.052] | same |
| static_linear_eigen | init | head-t4 | 5 | 1 | 9.56 ms [9.49 ms, 9.67 ms] | 9.56 ms [9.49 ms, 9.67 ms] | 581.0 [576.3, 587.1] | 1.042 [1.029, 1.058] | same |
| static_linear_eigen | init | head-t8 | 5 | 1 | 9.79 ms [9.70 ms, 9.92 ms] | 9.79 ms [9.70 ms, 9.92 ms] | 594.5 [589.0, 602.3] | 1.066 [1.055, 1.085] | same |
| static_linear_eigen | solve_step | base-t1 | 5 | 1 | 746.05 ms [741.51 ms, 756.62 ms] | 746.05 ms [741.51 ms, 756.62 ms] | 45314.1 [45038.5, 45956.0] | – | same |
| static_linear_eigen | solve_step | head-t1 | 5 | 1 | 757.35 ms [751.17 ms, 762.61 ms] | 757.35 ms [751.17 ms, 762.61 ms] | 46000.6 [45625.0, 46319.6] | 1.018 [0.993, 1.023] | same |
| static_linear_eigen | solve_step | head-t2 | 5 | 1 | 676.75 ms [672.59 ms, 686.76 ms] | 676.75 ms [672.59 ms, 686.76 ms] | 41104.7 [40852.2, 41712.8] | 0.907 [0.901, 0.909] | same |
| static_linear_eigen | solve_step | head-t4 | 5 | 1 | 642.08 ms [633.91 ms, 645.70 ms] | 642.08 ms [633.91 ms, 645.70 ms] | 38999.2 [38502.9, 39219.0] | 0.855 [0.843, 0.871] | same |
| static_linear_eigen | solve_step | head-t8 | 5 | 1 | 612.26 ms [600.87 ms, 615.51 ms] | 612.26 ms [600.87 ms, 615.51 ms] | 37187.9 [36495.9, 37385.3] | 0.814 [0.805, 0.826] | same |
| static_linear_eigen | predictor | base-t1 | 5 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head-t1 | 5 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.000 [0.600, 1.750] | same |
| static_linear_eigen | predictor | head-t2 | 5 | 1 | 0.1 µs [0.1 µs, 0.3 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | 1.000 [0.550, 2.364] | same |
| static_linear_eigen | predictor | head-t4 | 5 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 0.833 [0.591, 1.091] | same |
| static_linear_eigen | predictor | head-t8 | 5 | 1 | 0.2 µs [0.1 µs, 0.4 µs] | 0.2 µs [0.1 µs, 0.4 µs] | 0.0 [0.0, 0.0] | 1.583 [0.650, 1.818] | same |
| static_linear_eigen | matrix | base-t1 | 5 | 1 | 79.81 ms [79.33 ms, 81.04 ms] | 79.81 ms [79.33 ms, 81.04 ms] | 4847.3 [4818.4, 4922.1] | – | same |
| static_linear_eigen | matrix | head-t1 | 5 | 1 | 80.01 ms [79.50 ms, 80.68 ms] | 80.01 ms [79.50 ms, 80.68 ms] | 4859.4 [4828.6, 4900.1] | 1.004 [0.984, 1.017] | same |
| static_linear_eigen | matrix | head-t2 | 5 | 1 | 79.27 ms [78.80 ms, 80.58 ms] | 79.27 ms [78.80 ms, 80.58 ms] | 4814.5 [4786.1, 4894.3] | 0.993 [0.977, 1.011] | same |
| static_linear_eigen | matrix | head-t4 | 5 | 1 | 80.20 ms [79.91 ms, 80.80 ms] | 80.20 ms [79.91 ms, 80.80 ms] | 4871.2 [4853.4, 4907.5] | 1.003 [0.987, 1.012] | same |
| static_linear_eigen | matrix | head-t8 | 5 | 1 | 80.96 ms [77.93 ms, 83.24 ms] | 80.96 ms [77.93 ms, 83.24 ms] | 4917.3 [4733.7, 5055.6] | 1.013 [0.977, 1.033] | same |
| static_linear_eigen | corrector | base-t1 | 5 | 1 | 0.8 µs [0.6 µs, 1.0 µs] | 0.8 µs [0.6 µs, 1.0 µs] | 0.1 [0.0, 0.1] | – | same |
| static_linear_eigen | corrector | head-t1 | 5 | 1 | 0.9 µs [0.6 µs, 0.9 µs] | 0.9 µs [0.6 µs, 0.9 µs] | 0.1 [0.0, 0.1] | 0.906 [0.655, 1.274] | same |
| static_linear_eigen | corrector | head-t2 | 5 | 1 | 0.8 µs [0.7 µs, 0.9 µs] | 0.8 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.016 [0.792, 1.119] | same |
| static_linear_eigen | corrector | head-t4 | 5 | 1 | 0.7 µs [0.6 µs, 0.8 µs] | 0.7 µs [0.6 µs, 0.8 µs] | 0.0 [0.0, 0.0] | 0.781 [0.729, 1.109] | same |
| static_linear_eigen | corrector | head-t8 | 5 | 1 | 0.8 µs [0.6 µs, 0.9 µs] | 0.8 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 0.906 [0.781, 1.188] | same |
| static_linear_eigen | step_hooks | base-t1 | 5 | 2 | 110.4 µs [80.3 µs, 118.4 µs] | 55.2 µs [40.2 µs, 59.2 µs] | 3.4 [2.4, 3.6] | – | same |
| static_linear_eigen | step_hooks | head-t1 | 5 | 2 | 110.2 µs [84.8 µs, 113.4 µs] | 55.1 µs [42.4 µs, 56.7 µs] | 3.3 [2.6, 3.4] | 0.931 [0.768, 1.411] | same |
| static_linear_eigen | step_hooks | head-t2 | 5 | 2 | 93.5 µs [82.0 µs, 111.5 µs] | 46.8 µs [41.0 µs, 55.8 µs] | 2.8 [2.5, 3.4] | 0.998 [0.695, 1.348] | same |
| static_linear_eigen | step_hooks | head-t4 | 5 | 2 | 83.9 µs [78.6 µs, 99.5 µs] | 41.9 µs [39.3 µs, 49.8 µs] | 2.5 [2.4, 3.0] | 0.902 [0.757, 0.978] | same |
| static_linear_eigen | step_hooks | head-t8 | 5 | 2 | 111.3 µs [77.2 µs, 119.3 µs] | 55.7 µs [38.6 µs, 59.6 µs] | 3.4 [2.3, 3.6] | 1.008 [0.751, 1.386] | same |
| static_linear_eigen | step_rest | base-t1 | 5 | 1 | 666.16 ms [660.83 ms, 675.47 ms] | 666.16 ms [660.83 ms, 675.47 ms] | 40461.7 [40138.0, 41027.1] | – | same |
| static_linear_eigen | step_rest | head-t1 | 5 | 1 | 677.23 ms [671.34 ms, 681.82 ms] | 677.23 ms [671.34 ms, 681.82 ms] | 41134.2 [40776.4, 41412.8] | 1.020 [0.994, 1.024] | same |
| static_linear_eigen | step_rest | head-t2 | 5 | 1 | 596.06 ms [592.55 ms, 607.49 ms] | 596.06 ms [592.55 ms, 607.49 ms] | 36203.8 [35990.6, 36897.9] | 0.897 [0.887, 0.899] | same |
| static_linear_eigen | step_rest | head-t4 | 5 | 1 | 561.20 ms [553.62 ms, 565.31 ms] | 561.20 ms [553.62 ms, 565.31 ms] | 34086.7 [33626.1, 34336.0] | 0.837 [0.825, 0.855] | same |
| static_linear_eigen | step_rest | head-t8 | 5 | 1 | 528.91 ms [522.85 ms, 534.47 ms] | 528.91 ms [522.85 ms, 534.47 ms] | 32125.4 [31757.5, 32462.8] | 0.791 [0.784, 0.800] | same |
| static_linear_eigen | stiffness_assembly | base-t1 | 5 | 3 | 103.35 ms [103.23 ms, 104.11 ms] | 34.45 ms [34.41 ms, 34.70 ms] | 2092.5 [2089.9, 2107.8] | – | same |
| static_linear_eigen | stiffness_assembly | head-t1 | 5 | 3 | 103.93 ms [103.44 ms, 104.31 ms] | 34.64 ms [34.48 ms, 34.77 ms] | 2104.1 [2094.3, 2111.9] | 1.002 [0.998, 1.007] | same |
| static_linear_eigen | stiffness_assembly | head-t2 | 5 | 3 | 103.63 ms [103.39 ms, 104.00 ms] | 34.54 ms [34.46 ms, 34.67 ms] | 2098.1 [2093.2, 2105.6] | 1.003 [0.993, 1.003] | same |
| static_linear_eigen | stiffness_assembly | head-t4 | 5 | 3 | 93.55 ms [92.61 ms, 94.20 ms] | 31.18 ms [30.87 ms, 31.40 ms] | 1894.0 [1875.0, 1907.2] | 0.901 [0.896, 0.912] | same |
| static_linear_eigen | stiffness_assembly | head-t8 | 5 | 3 | 102.89 ms [101.40 ms, 104.76 ms] | 34.30 ms [33.80 ms, 34.92 ms] | 2083.1 [2052.9, 2121.1] | 0.993 [0.981, 1.009] | same |
| static_linear_eigen | analysis | base-t1 | 5 | 1 | 54.34 ms [53.95 ms, 54.80 ms] | 54.34 ms [53.95 ms, 54.80 ms] | 3300.4 [3277.0, 3328.2] | – | same |
| static_linear_eigen | analysis | head-t1 | 5 | 1 | 54.13 ms [54.12 ms, 54.60 ms] | 54.13 ms [54.12 ms, 54.60 ms] | 3288.0 [3286.9, 3316.6] | 0.996 [0.988, 1.012] | same |
| static_linear_eigen | analysis | head-t2 | 5 | 1 | 53.92 ms [53.70 ms, 54.21 ms] | 53.92 ms [53.70 ms, 54.21 ms] | 3275.3 [3261.8, 3292.8] | 0.992 [0.988, 1.002] | same |
| static_linear_eigen | analysis | head-t4 | 5 | 1 | 50.52 ms [49.87 ms, 51.33 ms] | 50.52 ms [49.87 ms, 51.33 ms] | 3068.5 [3028.8, 3117.9] | 0.928 [0.917, 0.951] | same |
| static_linear_eigen | analysis | head-t8 | 5 | 1 | 53.97 ms [53.54 ms, 54.05 ms] | 53.97 ms [53.54 ms, 54.05 ms] | 3278.0 [3252.1, 3282.9] | 0.993 [0.977, 1.002] | same |
| static_linear_eigen | factorize | base-t1 | 5 | 2 | 1.027 s [1.024 s, 1.027 s] | 513.46 ms [512.14 ms, 513.66 ms] | 31187.0 [31107.0, 31199.1] | – | same |
| static_linear_eigen | factorize | head-t1 | 5 | 2 | 1.057 s [1.048 s, 1.075 s] | 528.56 ms [524.11 ms, 537.45 ms] | 32104.0 [31833.4, 32644.1] | 1.030 [1.022, 1.046] | same |
| static_linear_eigen | factorize | head-t2 | 5 | 2 | 899.68 ms [895.75 ms, 901.86 ms] | 449.84 ms [447.88 ms, 450.93 ms] | 27322.6 [27203.3, 27388.9] | 0.878 [0.872, 0.878] | same |
| static_linear_eigen | factorize | head-t4 | 5 | 2 | 812.01 ms [809.70 ms, 815.01 ms] | 406.01 ms [404.85 ms, 407.50 ms] | 24660.3 [24589.9, 24751.2] | 0.791 [0.788, 0.795] | same |
| static_linear_eigen | factorize | head-t8 | 5 | 2 | 772.40 ms [767.56 ms, 777.20 ms] | 386.20 ms [383.78 ms, 388.60 ms] | 23457.2 [23310.1, 23603.0] | 0.753 [0.747, 0.757] | same |
| static_linear_eigen | back_substitution | base-t1 | 5 | 20 | 130.76 ms [129.11 ms, 131.23 ms] | 6.54 ms [6.46 ms, 6.56 ms] | 397.1 [392.1, 398.5] | – | same |
| static_linear_eigen | back_substitution | head-t1 | 5 | 20 | 130.66 ms [128.76 ms, 131.37 ms] | 6.53 ms [6.44 ms, 6.57 ms] | 396.8 [391.0, 399.0] | 1.001 [0.985, 1.012] | same |
| static_linear_eigen | back_substitution | head-t2 | 5 | 20 | 130.32 ms [128.69 ms, 130.87 ms] | 6.52 ms [6.43 ms, 6.54 ms] | 395.8 [390.8, 397.5] | 0.999 [0.984, 1.009] | same |
| static_linear_eigen | back_substitution | head-t4 | 5 | 20 | 130.54 ms [128.70 ms, 131.95 ms] | 6.53 ms [6.44 ms, 6.60 ms] | 396.5 [390.9, 400.7] | 1.000 [0.991, 1.006] | same |
| static_linear_eigen | back_substitution | head-t8 | 5 | 20 | 131.32 ms [131.16 ms, 132.44 ms] | 6.57 ms [6.56 ms, 6.62 ms] | 398.8 [398.3, 402.2] | 1.005 [0.999, 1.017] | same |
| structural_beam3_frame_static | mesh | base-t1 | 5 | 1 | 361.0 µs [343.0 µs, 365.0 µs] | 361.0 µs [343.0 µs, 365.0 µs] | 59.3 [56.4, 60.0] | – | same |
| structural_beam3_frame_static | mesh | head-t1 | 5 | 1 | 357.0 µs [337.0 µs, 369.0 µs] | 357.0 µs [337.0 µs, 369.0 µs] | 58.7 [55.4, 60.7] | 0.978 [0.961, 1.076] | same |
| structural_beam3_frame_static | mesh | head-t2 | 5 | 1 | 352.0 µs [337.0 µs, 366.0 µs] | 352.0 µs [337.0 µs, 366.0 µs] | 57.9 [55.4, 60.2] | 0.970 [0.964, 1.026] | same |
| structural_beam3_frame_static | mesh | head-t4 | 5 | 1 | 345.0 µs [341.0 µs, 351.0 µs] | 345.0 µs [341.0 µs, 351.0 µs] | 56.7 [56.0, 57.7] | 0.959 [0.934, 1.023] | same |
| structural_beam3_frame_static | mesh | head-t8 | 5 | 1 | 344.0 µs [342.0 µs, 348.0 µs] | 344.0 µs [342.0 µs, 348.0 µs] | 56.5 [56.2, 57.2] | 0.964 [0.937, 1.006] | same |
| structural_beam3_frame_static | init | base-t1 | 5 | 1 | 29.99 ms [29.84 ms, 30.12 ms] | 29.99 ms [29.84 ms, 30.12 ms] | 4928.7 [4905.2, 4951.5] | – | same |
| structural_beam3_frame_static | init | head-t1 | 5 | 1 | 29.95 ms [29.83 ms, 30.34 ms] | 29.95 ms [29.83 ms, 30.34 ms] | 4922.3 [4903.4, 4986.2] | 0.999 [0.996, 1.013] | same |
| structural_beam3_frame_static | init | head-t2 | 5 | 1 | 29.93 ms [29.85 ms, 29.95 ms] | 29.93 ms [29.85 ms, 29.95 ms] | 4919.1 [4906.3, 4922.3] | 0.998 [0.991, 1.003] | same |
| structural_beam3_frame_static | init | head-t4 | 5 | 1 | 29.84 ms [29.79 ms, 36.69 ms] | 29.84 ms [29.79 ms, 36.69 ms] | 4903.8 [4896.1, 6031.1] | 0.998 [0.993, 1.218] | same |
| structural_beam3_frame_static | init | head-t8 | 5 | 1 | 29.71 ms [29.59 ms, 29.85 ms] | 29.71 ms [29.59 ms, 29.85 ms] | 4882.8 [4862.8, 4906.5] | 0.990 [0.986, 0.997] | same |
| structural_beam3_frame_static | assemble_stiffness | base-t1 | 5 | 5 | 31.14 ms [30.94 ms, 31.21 ms] | 6.23 ms [6.19 ms, 6.24 ms] | 1023.6 [1017.1, 1026.1] | – | same |
| structural_beam3_frame_static | assemble_stiffness | head-t1 | 5 | 5 | 31.19 ms [31.10 ms, 31.20 ms] | 6.24 ms [6.22 ms, 6.24 ms] | 1025.3 [1022.4, 1025.5] | 1.000 [0.997, 1.008] | same |
| structural_beam3_frame_static | assemble_stiffness | head-t2 | 5 | 5 | 30.94 ms [30.87 ms, 31.04 ms] | 6.19 ms [6.17 ms, 6.21 ms] | 1017.1 [1014.9, 1020.4] | 0.997 [0.990, 1.000] | same |
| structural_beam3_frame_static | assemble_stiffness | head-t4 | 5 | 5 | 30.86 ms [30.77 ms, 30.95 ms] | 6.17 ms [6.15 ms, 6.19 ms] | 1014.6 [1011.4, 1017.3] | 0.994 [0.986, 0.997] | same |
| structural_beam3_frame_static | assemble_stiffness | head-t8 | 5 | 5 | 30.93 ms [30.89 ms, 31.17 ms] | 6.19 ms [6.18 ms, 6.23 ms] | 1016.7 [1015.5, 1024.5] | 0.995 [0.990, 1.001] | same |
| structural_beam3_frame_static | solve_step | base-t1 | 5 | 1 | 1.356 s [1.348 s, 1.356 s] | 1.356 s [1.348 s, 1.356 s] | 222807.4 [221505.3, 222928.7] | – | same |
| structural_beam3_frame_static | solve_step | head-t1 | 5 | 1 | 1.363 s [1.362 s, 1.379 s] | 1.363 s [1.362 s, 1.379 s] | 224086.9 [223897.9, 226615.7] | 1.007 [1.005, 1.017] | same |
| structural_beam3_frame_static | solve_step | head-t2 | 5 | 1 | 1.068 s [1.062 s, 1.071 s] | 1.068 s [1.062 s, 1.071 s] | 175563.9 [174519.2, 175992.4] | 0.789 [0.784, 0.790] | same |
| structural_beam3_frame_static | solve_step | head-t4 | 5 | 1 | 918.78 ms [916.92 ms, 924.80 ms] | 918.78 ms [916.92 ms, 924.80 ms] | 151015.8 [150709.7, 152005.3] | 0.678 [0.676, 0.682] | same |
| structural_beam3_frame_static | solve_step | head-t8 | 5 | 1 | 833.37 ms [826.51 ms, 840.94 ms] | 833.37 ms [826.51 ms, 840.94 ms] | 136977.5 [135850.4, 138220.9] | 0.615 [0.609, 0.621] | same |
| structural_dkt18_plate_static | mesh | base-t1 | 5 | 1 | 306.0 µs [301.0 µs, 309.0 µs] | 306.0 µs [301.0 µs, 309.0 µs] | 42.5 [41.8, 42.9] | – | same |
| structural_dkt18_plate_static | mesh | head-t1 | 5 | 1 | 306.0 µs [293.0 µs, 314.0 µs] | 306.0 µs [293.0 µs, 314.0 µs] | 42.5 [40.7, 43.6] | 1.013 [0.958, 1.017] | same |
| structural_dkt18_plate_static | mesh | head-t2 | 5 | 1 | 297.0 µs [294.0 µs, 318.0 µs] | 297.0 µs [294.0 µs, 318.0 µs] | 41.2 [40.8, 44.2] | 0.977 [0.955, 1.039] | same |
| structural_dkt18_plate_static | mesh | head-t4 | 5 | 1 | 293.0 µs [290.0 µs, 317.0 µs] | 293.0 µs [290.0 µs, 317.0 µs] | 40.7 [40.3, 44.0] | 0.973 [0.948, 1.033] | same |
| structural_dkt18_plate_static | mesh | head-t8 | 5 | 1 | 306.0 µs [292.0 µs, 324.0 µs] | 306.0 µs [292.0 µs, 324.0 µs] | 42.5 [40.6, 45.0] | 0.997 [0.964, 1.076] | same |
| structural_dkt18_plate_static | init | base-t1 | 5 | 1 | 69.41 ms [69.31 ms, 69.67 ms] | 69.41 ms [69.31 ms, 69.67 ms] | 9640.1 [9626.2, 9676.8] | – | same |
| structural_dkt18_plate_static | init | head-t1 | 5 | 1 | 69.25 ms [69.22 ms, 69.28 ms] | 69.25 ms [69.22 ms, 69.28 ms] | 9618.5 [9613.8, 9622.4] | 0.998 [0.993, 0.999] | same |
| structural_dkt18_plate_static | init | head-t2 | 5 | 1 | 69.20 ms [69.06 ms, 69.51 ms] | 69.20 ms [69.06 ms, 69.51 ms] | 9611.2 [9591.8, 9654.4] | 0.997 [0.991, 1.002] | same |
| structural_dkt18_plate_static | init | head-t4 | 5 | 1 | 69.16 ms [69.09 ms, 69.31 ms] | 69.16 ms [69.09 ms, 69.31 ms] | 9605.4 [9596.1, 9625.8] | 0.996 [0.993, 1.000] | same |
| structural_dkt18_plate_static | init | head-t8 | 5 | 1 | 69.97 ms [69.08 ms, 70.08 ms] | 69.97 ms [69.08 ms, 70.08 ms] | 9717.8 [9594.3, 9732.6] | 1.006 [0.995, 1.010] | same |
| structural_dkt18_plate_static | assemble_stiffness | base-t1 | 5 | 5 | 264.91 ms [264.14 ms, 266.98 ms] | 52.98 ms [52.83 ms, 53.40 ms] | 7358.5 [7337.1, 7416.1] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t1 | 5 | 5 | 263.47 ms [261.30 ms, 264.14 ms] | 52.69 ms [52.26 ms, 52.83 ms] | 7318.7 [7258.2, 7337.3] | 0.995 [0.986, 0.998] | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t2 | 5 | 5 | 263.83 ms [262.50 ms, 265.55 ms] | 52.77 ms [52.50 ms, 53.11 ms] | 7328.7 [7291.7, 7376.3] | 0.994 [0.983, 1.003] | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t4 | 5 | 5 | 264.92 ms [263.15 ms, 265.71 ms] | 52.98 ms [52.63 ms, 53.14 ms] | 7358.9 [7309.6, 7380.8] | 1.000 [0.986, 1.006] | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t8 | 5 | 5 | 264.04 ms [261.19 ms, 265.51 ms] | 52.81 ms [52.24 ms, 53.10 ms] | 7334.4 [7255.3, 7375.2] | 0.994 [0.987, 0.997] | same |
| structural_dkt18_plate_static | solve_step | base-t1 | 5 | 1 | 916.43 ms [911.01 ms, 918.10 ms] | 916.43 ms [911.01 ms, 918.10 ms] | 127282.5 [126529.2, 127514.3] | – | same |
| structural_dkt18_plate_static | solve_step | head-t1 | 5 | 1 | 922.47 ms [917.73 ms, 934.89 ms] | 922.47 ms [917.73 ms, 934.89 ms] | 128121.2 [127462.5, 129845.6] | 1.010 [1.000, 1.022] | same |
| structural_dkt18_plate_static | solve_step | head-t2 | 5 | 1 | 919.11 ms [917.14 ms, 920.46 ms] | 919.11 ms [917.14 ms, 920.46 ms] | 127654.6 [127380.1, 127841.5] | 1.004 [1.001, 1.007] | same |
| structural_dkt18_plate_static | solve_step | head-t4 | 5 | 1 | 915.16 ms [906.95 ms, 925.43 ms] | 915.16 ms [906.95 ms, 925.43 ms] | 127104.9 [125965.7, 128532.5] | 0.997 [0.989, 1.010] | same |
| structural_dkt18_plate_static | solve_step | head-t8 | 5 | 1 | 909.83 ms [907.03 ms, 926.22 ms] | 909.83 ms [907.03 ms, 926.22 ms] | 126365.7 [125976.7, 128641.9] | 0.996 [0.988, 1.010] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 545b66021, 72337cb4e | 55313c35e6ae3f79 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 545b66021, 72337cb4e | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 545b66021, 72337cb4e | a77809465b3e7c18 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 545b66021, 72337cb4e | d5869f57a777d9a5 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 545b66021, 72337cb4e | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 545b66021, 72337cb4e | 1cfc7f010c52d6b3 |
