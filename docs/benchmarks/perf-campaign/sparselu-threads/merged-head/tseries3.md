Suite `models`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_implicit_hex8_kvar | mesh | base-t1 | 3 | 1 | 3.95 ms [3.94 ms, 25.11 ms] | 3.95 ms [3.94 ms, 25.11 ms] | 2285.9 [2278.4, 14531.8] | – | same |
| heat_implicit_hex8_kvar | mesh | head-t1 | 3 | 1 | 3.96 ms [3.92 ms, 27.41 ms] | 3.96 ms [3.92 ms, 27.41 ms] | 2292.2 [2266.2, 15865.2] | 1.003 [0.995, 1.092] | same |
| heat_implicit_hex8_kvar | mesh | head-t2 | 3 | 1 | 3.91 ms [3.87 ms, 3.98 ms] | 3.91 ms [3.87 ms, 3.98 ms] | 2261.6 [2237.3, 2305.0] | 0.979 [0.156, 1.012] | same |
| heat_implicit_hex8_kvar | mesh | head-t4 | 3 | 1 | 3.89 ms [3.85 ms, 3.91 ms] | 3.89 ms [3.85 ms, 3.91 ms] | 2252.3 [2229.2, 2261.0] | 0.975 [0.155, 0.992] | same |
| heat_implicit_hex8_kvar | mesh | head-t8 | 3 | 1 | 3.91 ms [3.84 ms, 3.94 ms] | 3.91 ms [3.84 ms, 3.94 ms] | 2261.6 [2220.5, 2280.7] | 0.975 [0.156, 0.998] | same |
| heat_implicit_hex8_kvar | init | base-t1 | 3 | 1 | 1.02 ms [1.01 ms, 7.97 ms] | 1.02 ms [1.01 ms, 7.97 ms] | 591.4 [582.2, 4614.6] | – | same |
| heat_implicit_hex8_kvar | init | head-t1 | 3 | 1 | 1.00 ms [985.0 µs, 7.99 ms] | 1.00 ms [985.0 µs, 7.99 ms] | 580.4 [570.0, 4621.5] | 0.981 [0.979, 1.002] | same |
| heat_implicit_hex8_kvar | init | head-t2 | 3 | 1 | 1.01 ms [959.0 µs, 1.01 ms] | 1.01 ms [959.0 µs, 1.01 ms] | 586.2 [555.0, 586.8] | 0.938 [0.127, 1.007] | same |
| heat_implicit_hex8_kvar | init | head-t4 | 3 | 1 | 993.0 µs [992.0 µs, 995.0 µs] | 993.0 µs [992.0 µs, 995.0 µs] | 574.7 [574.1, 575.8] | 0.971 [0.125, 0.987] | same |
| heat_implicit_hex8_kvar | init | head-t8 | 3 | 1 | 980.0 µs [978.0 µs, 1.01 ms] | 980.0 µs [978.0 µs, 1.01 ms] | 567.1 [566.0, 586.2] | 0.957 [0.127, 0.974] | same |
| heat_implicit_hex8_kvar | solve_step | base-t1 | 3 | 5 | 580.82 ms [579.57 ms, 599.22 ms] | 116.16 ms [115.91 ms, 119.84 ms] | 67224.3 [67079.4, 69353.7] | – | same |
| heat_implicit_hex8_kvar | solve_step | head-t1 | 3 | 5 | 583.95 ms [583.87 ms, 585.37 ms] | 116.79 ms [116.77 ms, 117.07 ms] | 67586.6 [67577.2, 67751.5] | 1.005 [0.977, 1.008] | same |
| heat_implicit_hex8_kvar | solve_step | head-t2 | 3 | 5 | 554.72 ms [553.57 ms, 554.96 ms] | 110.94 ms [110.71 ms, 110.99 ms] | 64203.9 [64070.1, 64231.4] | 0.955 [0.924, 0.957] | same |
| heat_implicit_hex8_kvar | solve_step | head-t4 | 3 | 5 | 546.86 ms [546.34 ms, 548.79 ms] | 109.37 ms [109.27 ms, 109.76 ms] | 63293.7 [63233.7, 63517.0] | 0.944 [0.912, 0.945] | same |
| heat_implicit_hex8_kvar | solve_step | head-t8 | 3 | 5 | 541.35 ms [540.02 ms, 545.54 ms] | 108.27 ms [108.00 ms, 109.11 ms] | 62656.6 [62502.1, 63141.8] | 0.932 [0.903, 0.939] | same |
| phasefield_staggered_tet4 | mesh | base-t1 | 3 | 1 | 6.71 ms [6.64 ms, 6.79 ms] | 6.71 ms [6.64 ms, 6.79 ms] | 1118.3 [1107.5, 1132.0] | – | same |
| phasefield_staggered_tet4 | mesh | head-t1 | 3 | 1 | 6.77 ms [6.76 ms, 6.83 ms] | 6.77 ms [6.76 ms, 6.83 ms] | 1128.3 [1126.3, 1138.8] | 1.007 [1.006, 1.019] | same |
| phasefield_staggered_tet4 | mesh | head-t2 | 3 | 1 | 6.78 ms [6.75 ms, 6.78 ms] | 6.78 ms [6.75 ms, 6.78 ms] | 1129.5 [1124.8, 1130.7] | 1.010 [0.999, 1.016] | same |
| phasefield_staggered_tet4 | mesh | head-t4 | 3 | 1 | 6.78 ms [6.67 ms, 6.80 ms] | 6.78 ms [6.67 ms, 6.80 ms] | 1129.3 [1110.8, 1132.8] | 1.010 [0.981, 1.023] | same |
| phasefield_staggered_tet4 | mesh | head-t8 | 3 | 1 | 6.79 ms [6.74 ms, 6.80 ms] | 6.79 ms [6.74 ms, 6.80 ms] | 1131.2 [1122.7, 1133.8] | 1.014 [0.992, 1.021] | same |
| phasefield_staggered_tet4 | init | base-t1 | 3 | 1 | 2.90 ms [2.86 ms, 4.07 ms] | 2.90 ms [2.86 ms, 4.07 ms] | 482.5 [476.3, 679.2] | – | same |
| phasefield_staggered_tet4 | init | head-t1 | 3 | 1 | 2.86 ms [2.80 ms, 4.23 ms] | 2.86 ms [2.80 ms, 4.23 ms] | 476.2 [466.5, 705.7] | 1.000 [0.967, 1.039] | same |
| phasefield_staggered_tet4 | init | head-t2 | 3 | 1 | 2.85 ms [2.84 ms, 2.85 ms] | 2.85 ms [2.84 ms, 2.85 ms] | 475.0 [474.2, 475.7] | 0.983 [0.699, 0.999] | same |
| phasefield_staggered_tet4 | init | head-t4 | 3 | 1 | 2.87 ms [2.81 ms, 2.88 ms] | 2.87 ms [2.81 ms, 2.88 ms] | 478.7 [468.3, 479.5] | 0.971 [0.706, 1.005] | same |
| phasefield_staggered_tet4 | init | head-t8 | 3 | 1 | 2.99 ms [2.90 ms, 3.04 ms] | 2.99 ms [2.90 ms, 3.04 ms] | 499.0 [482.5, 506.8] | 1.000 [0.746, 1.048] | same |
| phasefield_staggered_tet4 | solid_solve | base-t1 | 3 | 4 | 552.80 ms [549.87 ms, 555.41 ms] | 138.20 ms [137.47 ms, 138.85 ms] | 23033.4 [22911.2, 23142.1] | – | same |
| phasefield_staggered_tet4 | solid_solve | head-t1 | 3 | 4 | 559.63 ms [553.85 ms, 566.60 ms] | 139.91 ms [138.46 ms, 141.65 ms] | 23318.1 [23076.9, 23608.5] | 1.012 [1.007, 1.020] | same |
| phasefield_staggered_tet4 | solid_solve | head-t2 | 3 | 4 | 515.16 ms [513.74 ms, 517.90 ms] | 128.79 ms [128.44 ms, 129.48 ms] | 21464.9 [21405.8, 21579.3] | 0.932 [0.932, 0.934] | same |
| phasefield_staggered_tet4 | solid_solve | head-t4 | 3 | 4 | 496.21 ms [495.10 ms, 496.48 ms] | 124.05 ms [123.77 ms, 124.12 ms] | 20675.4 [20629.0, 20686.6] | 0.898 [0.891, 0.903] | same |
| phasefield_staggered_tet4 | solid_solve | head-t8 | 3 | 4 | 479.71 ms [476.12 ms, 484.82 ms] | 119.93 ms [119.03 ms, 121.21 ms] | 19987.8 [19838.3, 20201.0] | 0.868 [0.857, 0.882] | same |
| phasefield_staggered_tet4 | strain_transfer | base-t1 | 3 | 4 | 715.0 µs [693.0 µs, 717.0 µs] | 178.8 µs [173.2 µs, 179.2 µs] | 29.8 [28.9, 29.9] | – | same |
| phasefield_staggered_tet4 | strain_transfer | head-t1 | 3 | 4 | 706.0 µs [705.0 µs, 709.0 µs] | 176.5 µs [176.2 µs, 177.2 µs] | 29.4 [29.4, 29.5] | 0.989 [0.987, 1.017] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t2 | 3 | 4 | 704.0 µs [691.0 µs, 708.0 µs] | 176.0 µs [172.8 µs, 177.0 µs] | 29.3 [28.8, 29.5] | 0.982 [0.966, 1.022] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t4 | 3 | 4 | 702.0 µs [686.0 µs, 704.0 µs] | 175.5 µs [171.5 µs, 176.0 µs] | 29.2 [28.6, 29.3] | 0.982 [0.957, 1.016] | same |
| phasefield_staggered_tet4 | strain_transfer | head-t8 | 3 | 4 | 708.0 µs [701.0 µs, 709.0 µs] | 177.0 µs [175.2 µs, 177.2 µs] | 29.5 [29.2, 29.5] | 0.990 [0.989, 1.012] | same |
| phasefield_staggered_tet4 | phase_solve | base-t1 | 3 | 4 | 71.68 ms [71.67 ms, 71.96 ms] | 17.92 ms [17.92 ms, 17.99 ms] | 2986.8 [2986.3, 2998.5] | – | same |
| phasefield_staggered_tet4 | phase_solve | head-t1 | 3 | 4 | 71.80 ms [71.72 ms, 71.84 ms] | 17.95 ms [17.93 ms, 17.96 ms] | 2991.8 [2988.4, 2993.2] | 1.001 [0.998, 1.002] | same |
| phasefield_staggered_tet4 | phase_solve | head-t2 | 3 | 4 | 67.75 ms [67.31 ms, 67.94 ms] | 16.94 ms [16.83 ms, 16.98 ms] | 2822.8 [2804.8, 2830.6] | 0.945 [0.935, 0.948] | same |
| phasefield_staggered_tet4 | phase_solve | head-t4 | 3 | 4 | 68.05 ms [67.46 ms, 68.16 ms] | 17.01 ms [16.86 ms, 17.04 ms] | 2835.5 [2810.7, 2839.9] | 0.947 [0.941, 0.949] | same |
| phasefield_staggered_tet4 | phase_solve | head-t8 | 3 | 4 | 67.95 ms [67.87 ms, 67.96 ms] | 16.99 ms [16.97 ms, 16.99 ms] | 2831.2 [2827.8, 2831.8] | 0.947 [0.944, 0.948] | same |
| phasefield_staggered_tet4 | damage_transfer | base-t1 | 3 | 4 | 1.59 ms [1.58 ms, 1.59 ms] | 396.5 µs [396.0 µs, 398.8 µs] | 66.1 [66.0, 66.5] | – | same |
| phasefield_staggered_tet4 | damage_transfer | head-t1 | 3 | 4 | 1.61 ms [1.59 ms, 1.61 ms] | 401.5 µs [396.5 µs, 403.0 µs] | 66.9 [66.1, 67.2] | 1.007 [1.001, 1.016] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t2 | 3 | 4 | 1.59 ms [1.57 ms, 1.59 ms] | 396.5 µs [393.5 µs, 398.2 µs] | 66.1 [65.6, 66.4] | 1.001 [0.987, 1.004] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t4 | 3 | 4 | 1.60 ms [1.56 ms, 1.60 ms] | 399.0 µs [390.2 µs, 399.0 µs] | 66.5 [65.0, 66.5] | 1.006 [0.979, 1.008] | same |
| phasefield_staggered_tet4 | damage_transfer | head-t8 | 3 | 4 | 1.59 ms [1.58 ms, 1.60 ms] | 396.5 µs [396.2 µs, 401.0 µs] | 66.1 [66.0, 66.8] | 1.001 [0.999, 1.006] | same |
| phasefield_staggered_tet4 | internal_forces | base-t1 | 3 | 4 | 1.59 ms [1.57 ms, 1.62 ms] | 397.2 µs [392.0 µs, 405.2 µs] | 66.2 [65.3, 67.5] | – | same |
| phasefield_staggered_tet4 | internal_forces | head-t1 | 3 | 4 | 1.55 ms [1.54 ms, 1.56 ms] | 387.0 µs [385.5 µs, 389.8 µs] | 64.5 [64.2, 65.0] | 0.970 [0.962, 0.987] | same |
| phasefield_staggered_tet4 | internal_forces | head-t2 | 3 | 4 | 1.18 ms [1.17 ms, 1.21 ms] | 294.8 µs [291.8 µs, 302.5 µs] | 49.1 [48.6, 50.4] | 0.734 [0.727, 0.772] | same |
| phasefield_staggered_tet4 | internal_forces | head-t4 | 3 | 4 | 1.20 ms [1.19 ms, 1.21 ms] | 298.8 µs [297.2 µs, 301.8 µs] | 49.8 [49.5, 50.3] | 0.752 [0.745, 0.758] | same |
| phasefield_staggered_tet4 | internal_forces | head-t8 | 3 | 4 | 1.27 ms [1.25 ms, 1.30 ms] | 317.8 µs [312.5 µs, 324.8 µs] | 53.0 [52.1, 54.1] | 0.797 [0.784, 0.817] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base-t1 | 3 | 5 | 22.94 ms [22.92 ms, 23.04 ms] | 4.59 ms [4.58 ms, 4.61 ms] | 764.6 [764.1, 767.9] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t1 | 3 | 5 | 22.99 ms [22.96 ms, 23.06 ms] | 4.60 ms [4.59 ms, 4.61 ms] | 766.2 [765.3, 768.6] | 1.002 [0.997, 1.006] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t2 | 3 | 5 | 12.45 ms [12.43 ms, 12.50 ms] | 2.49 ms [2.49 ms, 2.50 ms] | 414.9 [414.2, 416.8] | 0.543 [0.542, 0.543] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t4 | 3 | 5 | 8.47 ms [8.37 ms, 8.48 ms] | 1.69 ms [1.67 ms, 1.70 ms] | 282.3 [279.1, 282.7] | 0.368 [0.365, 0.370] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | head-t8 | 3 | 5 | 5.55 ms [5.53 ms, 5.58 ms] | 1.11 ms [1.11 ms, 1.12 ms] | 185.0 [184.5, 185.9] | 0.242 [0.240, 0.243] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base-t1 | 3 | 5 | 8.49 ms [8.48 ms, 8.49 ms] | 1.70 ms [1.70 ms, 1.70 ms] | 282.9 [282.6, 283.1] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t1 | 3 | 5 | 8.46 ms [8.40 ms, 8.54 ms] | 1.69 ms [1.68 ms, 1.71 ms] | 282.1 [280.2, 284.7] | 0.998 [0.990, 1.006] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t2 | 3 | 5 | 6.37 ms [6.36 ms, 6.39 ms] | 1.27 ms [1.27 ms, 1.28 ms] | 212.4 [212.1, 213.1] | 0.751 [0.750, 0.753] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t4 | 3 | 5 | 6.39 ms [6.39 ms, 6.39 ms] | 1.28 ms [1.28 ms, 1.28 ms] | 213.0 [212.9, 213.2] | 0.753 [0.753, 0.754] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | head-t8 | 3 | 5 | 6.73 ms [6.71 ms, 6.74 ms] | 1.35 ms [1.34 ms, 1.35 ms] | 224.4 [223.6, 224.6] | 0.794 [0.790, 0.794] | same |
| phasefield_staggered_tri3 | mesh | base-t1 | 3 | 1 | 42.53 ms [41.97 ms, 44.11 ms] | 42.53 ms [41.97 ms, 44.11 ms] | 830.7 [819.7, 861.6] | – | same |
| phasefield_staggered_tri3 | mesh | head-t1 | 3 | 1 | 42.14 ms [42.02 ms, 44.49 ms] | 42.14 ms [42.02 ms, 44.49 ms] | 823.0 [820.6, 868.9] | 1.001 [0.991, 1.008] | same |
| phasefield_staggered_tri3 | mesh | head-t2 | 3 | 1 | 42.36 ms [42.01 ms, 42.62 ms] | 42.36 ms [42.01 ms, 42.62 ms] | 827.3 [820.5, 832.5] | 0.996 [0.952, 1.016] | same |
| phasefield_staggered_tri3 | mesh | head-t4 | 3 | 1 | 42.09 ms [41.92 ms, 42.11 ms] | 42.09 ms [41.92 ms, 42.11 ms] | 822.1 [818.8, 822.4] | 0.990 [0.955, 0.999] | same |
| phasefield_staggered_tri3 | mesh | head-t8 | 3 | 1 | 42.04 ms [42.02 ms, 42.16 ms] | 42.04 ms [42.02 ms, 42.16 ms] | 821.1 [820.8, 823.4] | 0.988 [0.953, 1.005] | same |
| phasefield_staggered_tri3 | init | base-t1 | 3 | 1 | 20.63 ms [20.54 ms, 28.86 ms] | 20.63 ms [20.54 ms, 28.86 ms] | 402.9 [401.2, 563.7] | – | same |
| phasefield_staggered_tri3 | init | head-t1 | 3 | 1 | 19.84 ms [19.70 ms, 28.90 ms] | 19.84 ms [19.70 ms, 28.90 ms] | 387.5 [384.7, 564.5] | 0.966 [0.955, 1.001] | same |
| phasefield_staggered_tri3 | init | head-t2 | 3 | 1 | 19.52 ms [19.45 ms, 19.74 ms] | 19.52 ms [19.45 ms, 19.74 ms] | 381.3 [379.9, 385.6] | 0.946 [0.674, 0.961] | same |
| phasefield_staggered_tri3 | init | head-t4 | 3 | 1 | 20.15 ms [19.92 ms, 20.50 ms] | 20.15 ms [19.92 ms, 20.50 ms] | 393.6 [389.1, 400.4] | 0.970 [0.698, 0.994] | same |
| phasefield_staggered_tri3 | init | head-t8 | 3 | 1 | 19.69 ms [19.58 ms, 20.00 ms] | 19.69 ms [19.58 ms, 20.00 ms] | 384.6 [382.4, 390.6] | 0.953 [0.693, 0.955] | same |
| phasefield_staggered_tri3 | solid_solve | base-t1 | 3 | 4 | 2.978 s [2.972 s, 3.005 s] | 744.40 ms [742.97 ms, 751.36 ms] | 14539.0 [14511.1, 14675.1] | – | same |
| phasefield_staggered_tri3 | solid_solve | head-t1 | 3 | 4 | 2.998 s [2.997 s, 3.002 s] | 749.54 ms [749.18 ms, 750.42 ms] | 14639.5 [14632.4, 14656.7] | 1.006 [0.998, 1.010] | same |
| phasefield_staggered_tri3 | solid_solve | head-t2 | 3 | 4 | 2.868 s [2.859 s, 2.868 s] | 716.92 ms [714.76 ms, 716.92 ms] | 14002.3 [13960.2, 14002.4] | 0.960 [0.954, 0.965] | same |
| phasefield_staggered_tri3 | solid_solve | head-t4 | 3 | 4 | 2.745 s [2.732 s, 2.770 s] | 686.25 ms [683.09 ms, 692.55 ms] | 13403.3 [13341.7, 13526.4] | 0.922 [0.919, 0.922] | same |
| phasefield_staggered_tri3 | solid_solve | head-t8 | 3 | 4 | 2.700 s [2.678 s, 2.702 s] | 674.91 ms [669.43 ms, 675.39 ms] | 13181.8 [13074.8, 13191.1] | 0.901 [0.899, 0.907] | same |
| phasefield_staggered_tri3 | strain_transfer | base-t1 | 3 | 4 | 4.04 ms [4.02 ms, 4.06 ms] | 1.01 ms [1.00 ms, 1.01 ms] | 19.7 [19.6, 19.8] | – | same |
| phasefield_staggered_tri3 | strain_transfer | head-t1 | 3 | 4 | 4.30 ms [4.27 ms, 4.33 ms] | 1.08 ms [1.07 ms, 1.08 ms] | 21.0 [20.9, 21.1] | 1.066 [1.059, 1.071] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t2 | 3 | 4 | 4.32 ms [4.30 ms, 4.32 ms] | 1.08 ms [1.08 ms, 1.08 ms] | 21.1 [21.0, 21.1] | 1.066 [1.065, 1.075] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t4 | 3 | 4 | 4.26 ms [4.26 ms, 4.30 ms] | 1.07 ms [1.06 ms, 1.07 ms] | 20.8 [20.8, 21.0] | 1.059 [1.056, 1.062] | same |
| phasefield_staggered_tri3 | strain_transfer | head-t8 | 3 | 4 | 4.33 ms [4.33 ms, 4.38 ms] | 1.08 ms [1.08 ms, 1.10 ms] | 21.1 [21.1, 21.4] | 1.078 [1.067, 1.086] | same |
| phasefield_staggered_tri3 | phase_solve | base-t1 | 3 | 4 | 813.25 ms [811.80 ms, 814.04 ms] | 203.31 ms [202.95 ms, 203.51 ms] | 3971.0 [3963.8, 3974.8] | – | same |
| phasefield_staggered_tri3 | phase_solve | head-t1 | 3 | 4 | 814.95 ms [814.15 ms, 816.21 ms] | 203.74 ms [203.54 ms, 204.05 ms] | 3979.2 [3975.3, 3985.4] | 1.003 [1.001, 1.004] | same |
| phasefield_staggered_tri3 | phase_solve | head-t2 | 3 | 4 | 770.15 ms [767.97 ms, 770.27 ms] | 192.54 ms [191.99 ms, 192.57 ms] | 3760.5 [3749.8, 3761.1] | 0.947 [0.943, 0.949] | same |
| phasefield_staggered_tri3 | phase_solve | head-t4 | 3 | 4 | 756.21 ms [755.48 ms, 762.61 ms] | 189.05 ms [188.87 ms, 190.65 ms] | 3692.4 [3688.9, 3723.7] | 0.929 [0.929, 0.939] | same |
| phasefield_staggered_tri3 | phase_solve | head-t8 | 3 | 4 | 753.40 ms [750.92 ms, 753.68 ms] | 188.35 ms [187.73 ms, 188.42 ms] | 3678.7 [3666.6, 3680.1] | 0.926 [0.925, 0.926] | same |
| phasefield_staggered_tri3 | damage_transfer | base-t1 | 3 | 4 | 12.29 ms [12.27 ms, 12.37 ms] | 3.07 ms [3.07 ms, 3.09 ms] | 60.0 [59.9, 60.4] | – | same |
| phasefield_staggered_tri3 | damage_transfer | head-t1 | 3 | 4 | 12.16 ms [12.16 ms, 12.24 ms] | 3.04 ms [3.04 ms, 3.06 ms] | 59.4 [59.4, 59.7] | 0.990 [0.983, 0.998] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t2 | 3 | 4 | 12.23 ms [12.17 ms, 12.28 ms] | 3.06 ms [3.04 ms, 3.07 ms] | 59.7 [59.4, 60.0] | 0.997 [0.984, 1.000] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t4 | 3 | 4 | 12.11 ms [12.09 ms, 12.21 ms] | 3.03 ms [3.02 ms, 3.05 ms] | 59.1 [59.0, 59.6] | 0.985 [0.978, 0.995] | same |
| phasefield_staggered_tri3 | damage_transfer | head-t8 | 3 | 4 | 12.19 ms [12.13 ms, 12.22 ms] | 3.05 ms [3.03 ms, 3.06 ms] | 59.5 [59.3, 59.7] | 0.988 [0.988, 0.994] | same |
| phasefield_staggered_tri3 | internal_forces | base-t1 | 3 | 4 | 7.22 ms [7.16 ms, 7.25 ms] | 1.81 ms [1.79 ms, 1.81 ms] | 35.3 [35.0, 35.4] | – | same |
| phasefield_staggered_tri3 | internal_forces | head-t1 | 3 | 4 | 7.14 ms [7.11 ms, 7.21 ms] | 1.78 ms [1.78 ms, 1.80 ms] | 34.8 [34.7, 35.2] | 0.985 [0.985, 1.007] | same |
| phasefield_staggered_tri3 | internal_forces | head-t2 | 3 | 4 | 4.39 ms [4.36 ms, 4.45 ms] | 1.10 ms [1.09 ms, 1.11 ms] | 21.4 [21.3, 21.7] | 0.609 [0.605, 0.617] | same |
| phasefield_staggered_tri3 | internal_forces | head-t4 | 3 | 4 | 3.40 ms [3.37 ms, 3.42 ms] | 849.0 µs [841.5 µs, 854.5 µs] | 16.6 [16.4, 16.7] | 0.470 [0.464, 0.477] | same |
| phasefield_staggered_tri3 | internal_forces | head-t8 | 3 | 4 | 3.26 ms [3.15 ms, 3.27 ms] | 815.2 µs [788.8 µs, 817.0 µs] | 15.9 [15.4, 16.0] | 0.450 [0.437, 0.456] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base-t1 | 3 | 5 | 61.84 ms [61.55 ms, 62.36 ms] | 12.37 ms [12.31 ms, 12.47 ms] | 241.6 [240.4, 243.6] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t1 | 3 | 5 | 61.83 ms [61.78 ms, 61.97 ms] | 12.37 ms [12.36 ms, 12.39 ms] | 241.5 [241.3, 242.1] | 0.999 [0.991, 1.007] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t2 | 3 | 5 | 32.09 ms [32.05 ms, 32.24 ms] | 6.42 ms [6.41 ms, 6.45 ms] | 125.3 [125.2, 125.9] | 0.519 [0.517, 0.521] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t4 | 3 | 5 | 20.36 ms [20.23 ms, 21.57 ms] | 4.07 ms [4.05 ms, 4.31 ms] | 79.5 [79.0, 84.3] | 0.327 [0.326, 0.350] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | head-t8 | 3 | 5 | 15.53 ms [15.47 ms, 15.79 ms] | 3.11 ms [3.09 ms, 3.16 ms] | 60.7 [60.4, 61.7] | 0.251 [0.251, 0.253] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base-t1 | 3 | 5 | 64.52 ms [64.25 ms, 64.58 ms] | 12.90 ms [12.85 ms, 12.92 ms] | 252.0 [251.0, 252.3] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t1 | 3 | 5 | 64.30 ms [64.05 ms, 64.62 ms] | 12.86 ms [12.81 ms, 12.92 ms] | 251.2 [250.2, 252.4] | 0.997 [0.996, 1.001] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t2 | 3 | 5 | 42.24 ms [42.24 ms, 42.35 ms] | 8.45 ms [8.45 ms, 8.47 ms] | 165.0 [165.0, 165.4] | 0.656 [0.654, 0.657] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t4 | 3 | 5 | 38.05 ms [37.99 ms, 38.10 ms] | 7.61 ms [7.60 ms, 7.62 ms] | 148.6 [148.4, 148.8] | 0.590 [0.589, 0.592] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | head-t8 | 3 | 5 | 37.34 ms [37.32 ms, 37.43 ms] | 7.47 ms [7.46 ms, 7.49 ms] | 145.8 [145.8, 146.2] | 0.580 [0.578, 0.581] | same |
| static_linear_eigen | mesh | base-t1 | 3 | 1 | 812.5 µs [807.9 µs, 6.58 ms] | 812.5 µs [807.9 µs, 6.58 ms] | 49.3 [49.1, 399.8] | – | same |
| static_linear_eigen | mesh | head-t1 | 3 | 1 | 820.6 µs [794.6 µs, 5.77 ms] | 820.6 µs [794.6 µs, 5.77 ms] | 49.8 [48.3, 350.4] | 0.978 [0.876, 1.016] | same |
| static_linear_eigen | mesh | head-t2 | 3 | 1 | 812.5 µs [793.7 µs, 827.8 µs] | 812.5 µs [793.7 µs, 827.8 µs] | 49.4 [48.2, 50.3] | 0.977 [0.126, 1.006] | same |
| static_linear_eigen | mesh | head-t4 | 3 | 1 | 804.5 µs [798.4 µs, 825.2 µs] | 804.5 µs [798.4 µs, 825.2 µs] | 48.9 [48.5, 50.1] | 0.988 [0.122, 1.016] | same |
| static_linear_eigen | mesh | head-t8 | 3 | 1 | 815.1 µs [815.1 µs, 818.6 µs] | 815.1 µs [815.1 µs, 818.6 µs] | 49.5 [49.5, 49.7] | 1.003 [0.124, 1.009] | same |
| static_linear_eigen | init | base-t1 | 3 | 1 | 9.35 ms [9.29 ms, 62.53 ms] | 9.35 ms [9.29 ms, 62.53 ms] | 567.9 [564.1, 3798.1] | – | same |
| static_linear_eigen | init | head-t1 | 3 | 1 | 9.35 ms [9.33 ms, 58.65 ms] | 9.35 ms [9.33 ms, 58.65 ms] | 567.9 [567.0, 3562.1] | 1.000 [0.938, 1.005] | same |
| static_linear_eigen | init | head-t2 | 3 | 1 | 9.57 ms [9.35 ms, 10.44 ms] | 9.57 ms [9.35 ms, 10.44 ms] | 581.3 [568.2, 634.0] | 1.001 [0.167, 1.030] | same |
| static_linear_eigen | init | head-t4 | 3 | 1 | 9.60 ms [9.52 ms, 9.64 ms] | 9.60 ms [9.52 ms, 9.64 ms] | 582.9 [578.1, 585.7] | 1.025 [0.153, 1.031] | same |
| static_linear_eigen | init | head-t8 | 3 | 1 | 9.90 ms [9.74 ms, 10.05 ms] | 9.90 ms [9.74 ms, 10.05 ms] | 601.1 [591.6, 610.5] | 1.042 [0.161, 1.066] | same |
| static_linear_eigen | solve_step | base-t1 | 3 | 1 | 719.33 ms [716.06 ms, 735.56 ms] | 719.33 ms [716.06 ms, 735.56 ms] | 43691.0 [43492.7, 44676.7] | – | same |
| static_linear_eigen | solve_step | head-t1 | 3 | 1 | 736.64 ms [717.39 ms, 745.11 ms] | 736.64 ms [717.39 ms, 745.11 ms] | 44742.6 [43573.0, 45256.7] | 1.013 [1.002, 1.024] | same |
| static_linear_eigen | solve_step | head-t2 | 3 | 1 | 651.13 ms [643.78 ms, 654.34 ms] | 651.13 ms [643.78 ms, 654.34 ms] | 39548.9 [39102.4, 39743.5] | 0.909 [0.875, 0.910] | same |
| static_linear_eigen | solve_step | head-t4 | 3 | 1 | 608.69 ms [607.15 ms, 608.79 ms] | 608.69 ms [607.15 ms, 608.79 ms] | 36971.1 [36877.3, 36976.9] | 0.844 [0.828, 0.850] | same |
| static_linear_eigen | solve_step | head-t8 | 3 | 1 | 582.02 ms [575.49 ms, 582.95 ms] | 582.02 ms [575.49 ms, 582.95 ms] | 35351.2 [34954.2, 35407.4] | 0.804 [0.791, 0.810] | same |
| static_linear_eigen | predictor | base-t1 | 3 | 1 | 0.1 µs [0.1 µs, 0.3 µs] | 0.1 µs [0.1 µs, 0.3 µs] | 0.0 [0.0, 0.0] | – | same |
| static_linear_eigen | predictor | head-t1 | 3 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.000 [0.516, 1.000] | same |
| static_linear_eigen | predictor | head-t2 | 3 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.000 [0.290, 1.111] | same |
| static_linear_eigen | predictor | head-t4 | 3 | 1 | 0.1 µs [0.1 µs, 0.1 µs] | 0.1 µs [0.1 µs, 0.1 µs] | 0.0 [0.0, 0.0] | 1.000 [0.290, 1.111] | same |
| static_linear_eigen | predictor | head-t8 | 3 | 1 | 0.1 µs [0.1 µs, 0.2 µs] | 0.1 µs [0.1 µs, 0.2 µs] | 0.0 [0.0, 0.0] | 1.000 [0.258, 2.111] | same |
| static_linear_eigen | matrix | base-t1 | 3 | 1 | 70.20 ms [68.80 ms, 76.44 ms] | 70.20 ms [68.80 ms, 76.44 ms] | 4263.7 [4178.9, 4643.2] | – | same |
| static_linear_eigen | matrix | head-t1 | 3 | 1 | 70.92 ms [69.22 ms, 77.74 ms] | 70.92 ms [69.22 ms, 77.74 ms] | 4307.7 [4204.3, 4722.1] | 1.010 [1.006, 1.017] | same |
| static_linear_eigen | matrix | head-t2 | 3 | 1 | 63.67 ms [63.14 ms, 63.82 ms] | 63.67 ms [63.14 ms, 63.82 ms] | 3867.3 [3835.3, 3876.1] | 0.907 [0.835, 0.918] | same |
| static_linear_eigen | matrix | head-t4 | 3 | 1 | 61.96 ms [61.78 ms, 62.68 ms] | 61.96 ms [61.78 ms, 62.68 ms] | 3763.2 [3752.2, 3807.3] | 0.883 [0.820, 0.898] | same |
| static_linear_eigen | matrix | head-t8 | 3 | 1 | 60.19 ms [60.16 ms, 61.74 ms] | 60.19 ms [60.16 ms, 61.74 ms] | 3655.9 [3654.2, 3750.3] | 0.874 [0.787, 0.880] | same |
| static_linear_eigen | corrector | base-t1 | 3 | 1 | 0.7 µs [0.6 µs, 0.9 µs] | 0.7 µs [0.6 µs, 0.9 µs] | 0.0 [0.0, 0.1] | – | same |
| static_linear_eigen | corrector | head-t1 | 3 | 1 | 0.8 µs [0.7 µs, 0.9 µs] | 0.8 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.250 [0.800, 1.333] | same |
| static_linear_eigen | corrector | head-t2 | 3 | 1 | 0.8 µs [0.6 µs, 1.0 µs] | 0.8 µs [0.6 µs, 1.0 µs] | 0.0 [0.0, 0.1] | 1.250 [0.711, 1.449] | same |
| static_linear_eigen | corrector | head-t4 | 3 | 1 | 0.7 µs [0.7 µs, 0.9 µs] | 0.7 µs [0.7 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.031 [0.822, 1.319] | same |
| static_linear_eigen | corrector | head-t8 | 3 | 1 | 0.8 µs [0.8 µs, 0.9 µs] | 0.8 µs [0.8 µs, 0.9 µs] | 0.0 [0.0, 0.1] | 1.188 [0.889, 1.333] | same |
| static_linear_eigen | step_hooks | base-t1 | 3 | 2 | 4.6 µs [4.2 µs, 10.9 µs] | 2.3 µs [2.1 µs, 5.5 µs] | 0.1 [0.1, 0.3] | – | same |
| static_linear_eigen | step_hooks | head-t1 | 3 | 2 | 4.5 µs [4.0 µs, 4.6 µs] | 2.3 µs [2.0 µs, 2.3 µs] | 0.1 [0.1, 0.1] | 0.882 [0.421, 1.071] | same |
| static_linear_eigen | step_hooks | head-t2 | 3 | 2 | 4.7 µs [4.3 µs, 17.3 µs] | 2.3 µs [2.1 µs, 8.7 µs] | 0.1 [0.1, 0.5] | 1.104 [0.393, 3.779] | same |
| static_linear_eigen | step_hooks | head-t4 | 3 | 2 | 4.9 µs [4.3 µs, 4.9 µs] | 2.5 µs [2.1 µs, 2.5 µs] | 0.1 [0.1, 0.2] | 0.932 [0.451, 1.165] | same |
| static_linear_eigen | step_hooks | head-t8 | 3 | 2 | 3.8 µs [3.5 µs, 4.2 µs] | 1.9 µs [1.8 µs, 2.1 µs] | 0.1 [0.1, 0.1] | 0.835 [0.343, 0.921] | same |
| static_linear_eigen | step_rest | base-t1 | 3 | 1 | 649.13 ms [647.26 ms, 659.10 ms] | 649.13 ms [647.26 ms, 659.10 ms] | 39427.0 [39313.5, 40032.8] | – | same |
| static_linear_eigen | step_rest | head-t1 | 3 | 1 | 665.72 ms [648.16 ms, 667.36 ms] | 665.72 ms [648.16 ms, 667.36 ms] | 40434.7 [39368.4, 40534.3] | 1.013 [1.001, 1.026] | same |
| static_linear_eigen | step_rest | head-t2 | 3 | 1 | 587.98 ms [579.96 ms, 590.65 ms] | 587.98 ms [579.96 ms, 590.65 ms] | 35713.2 [35226.0, 35875.2] | 0.908 [0.880, 0.910] | same |
| static_linear_eigen | step_rest | head-t4 | 3 | 1 | 546.10 ms [545.19 ms, 546.91 ms] | 546.10 ms [545.19 ms, 546.91 ms] | 33169.3 [33113.8, 33218.6] | 0.840 [0.829, 0.845] | same |
| static_linear_eigen | step_rest | head-t8 | 3 | 1 | 521.20 ms [515.32 ms, 521.83 ms] | 521.20 ms [515.32 ms, 521.83 ms] | 31656.8 [31299.7, 31695.0] | 0.796 [0.792, 0.803] | same |
| static_linear_eigen | stiffness_assembly | base-t1 | 3 | 3 | 40.11 ms [39.62 ms, 40.59 ms] | 13.37 ms [13.21 ms, 13.53 ms] | 812.1 [802.1, 821.8] | – | same |
| static_linear_eigen | stiffness_assembly | head-t1 | 3 | 3 | 39.73 ms [39.63 ms, 39.84 ms] | 13.24 ms [13.21 ms, 13.28 ms] | 804.5 [802.4, 806.6] | 0.993 [0.976, 1.003] | same |
| static_linear_eigen | stiffness_assembly | head-t2 | 3 | 3 | 21.81 ms [21.76 ms, 21.84 ms] | 7.27 ms [7.25 ms, 7.28 ms] | 441.7 [440.5, 442.2] | 0.545 [0.537, 0.549] | same |
| static_linear_eigen | stiffness_assembly | head-t4 | 3 | 3 | 13.17 ms [13.10 ms, 13.27 ms] | 4.39 ms [4.37 ms, 4.42 ms] | 266.6 [265.2, 268.6] | 0.328 [0.327, 0.331] | same |
| static_linear_eigen | stiffness_assembly | head-t8 | 3 | 3 | 8.48 ms [8.23 ms, 8.61 ms] | 2.83 ms [2.74 ms, 2.87 ms] | 171.8 [166.7, 174.3] | 0.209 [0.205, 0.217] | same |
| static_linear_eigen | analysis | base-t1 | 3 | 1 | 48.33 ms [48.27 ms, 48.57 ms] | 48.33 ms [48.27 ms, 48.57 ms] | 2935.7 [2931.6, 2950.3] | – | same |
| static_linear_eigen | analysis | head-t1 | 3 | 1 | 48.26 ms [48.22 ms, 48.55 ms] | 48.26 ms [48.22 ms, 48.55 ms] | 2931.2 [2928.9, 2949.0] | 1.000 [0.998, 1.000] | same |
| static_linear_eigen | analysis | head-t2 | 3 | 1 | 48.64 ms [48.26 ms, 48.86 ms] | 48.64 ms [48.26 ms, 48.86 ms] | 2954.6 [2931.3, 2967.5] | 1.006 [1.000, 1.006] | same |
| static_linear_eigen | analysis | head-t4 | 3 | 1 | 48.62 ms [48.62 ms, 48.87 ms] | 48.62 ms [48.62 ms, 48.87 ms] | 2953.4 [2953.0, 2968.5] | 1.006 [1.006, 1.007] | same |
| static_linear_eigen | analysis | head-t8 | 3 | 1 | 49.76 ms [49.73 ms, 49.86 ms] | 49.76 ms [49.73 ms, 49.86 ms] | 3022.5 [3020.6, 3028.1] | 1.029 [1.024, 1.033] | same |
| static_linear_eigen | factorize | base-t1 | 3 | 2 | 1.039 s [1.028 s, 1.041 s] | 519.48 ms [514.17 ms, 520.68 ms] | 31552.2 [31229.9, 31625.5] | – | same |
| static_linear_eigen | factorize | head-t1 | 3 | 2 | 1.060 s [1.057 s, 1.070 s] | 529.93 ms [528.31 ms, 535.17 ms] | 32187.0 [32089.0, 32505.7] | 1.020 [1.015, 1.041] | same |
| static_linear_eigen | factorize | head-t2 | 3 | 2 | 905.19 ms [903.99 ms, 916.91 ms] | 452.60 ms [452.00 ms, 458.46 ms] | 27490.1 [27453.7, 27845.9] | 0.871 [0.868, 0.892] | same |
| static_linear_eigen | factorize | head-t4 | 3 | 2 | 821.14 ms [814.88 ms, 822.32 ms] | 410.57 ms [407.44 ms, 411.16 ms] | 24937.3 [24747.3, 24973.3] | 0.790 [0.790, 0.792] | same |
| static_linear_eigen | factorize | head-t8 | 3 | 2 | 778.08 ms [776.28 ms, 778.25 ms] | 389.04 ms [388.14 ms, 389.12 ms] | 23629.7 [23575.0, 23634.9] | 0.749 [0.745, 0.757] | same |
| static_linear_eigen | back_substitution | base-t1 | 3 | 20 | 131.89 ms [129.98 ms, 132.50 ms] | 6.59 ms [6.50 ms, 6.63 ms] | 400.5 [394.7, 402.4] | – | same |
| static_linear_eigen | back_substitution | head-t1 | 3 | 20 | 130.96 ms [129.73 ms, 131.75 ms] | 6.55 ms [6.49 ms, 6.59 ms] | 397.7 [394.0, 400.1] | 0.994 [0.993, 0.998] | same |
| static_linear_eigen | back_substitution | head-t2 | 3 | 20 | 130.06 ms [128.38 ms, 131.57 ms] | 6.50 ms [6.42 ms, 6.58 ms] | 395.0 [389.9, 399.6] | 0.988 [0.986, 0.993] | same |
| static_linear_eigen | back_substitution | head-t4 | 3 | 20 | 129.67 ms [129.14 ms, 129.86 ms] | 6.48 ms [6.46 ms, 6.49 ms] | 393.8 [392.2, 394.4] | 0.983 [0.980, 0.994] | same |
| static_linear_eigen | back_substitution | head-t8 | 3 | 20 | 130.96 ms [129.18 ms, 132.03 ms] | 6.55 ms [6.46 ms, 6.60 ms] | 397.7 [392.3, 401.0] | 0.996 [0.979, 1.008] | same |
| structural_beam3_frame_static | mesh | base-t1 | 3 | 1 | 351.0 µs [348.0 µs, 352.0 µs] | 351.0 µs [348.0 µs, 352.0 µs] | 57.7 [57.2, 57.9] | – | same |
| structural_beam3_frame_static | mesh | head-t1 | 3 | 1 | 352.0 µs [335.0 µs, 353.0 µs] | 352.0 µs [335.0 µs, 353.0 µs] | 57.9 [55.1, 58.0] | 1.000 [0.954, 1.014] | same |
| structural_beam3_frame_static | mesh | head-t2 | 3 | 1 | 349.0 µs [342.0 µs, 368.0 µs] | 349.0 µs [342.0 µs, 368.0 µs] | 57.4 [56.2, 60.5] | 1.003 [0.972, 1.048] | same |
| structural_beam3_frame_static | mesh | head-t4 | 3 | 1 | 364.0 µs [349.0 µs, 370.0 µs] | 364.0 µs [349.0 µs, 370.0 µs] | 59.8 [57.4, 60.8] | 1.037 [0.991, 1.063] | same |
| structural_beam3_frame_static | mesh | head-t8 | 3 | 1 | 354.0 µs [349.0 µs, 362.0 µs] | 354.0 µs [349.0 µs, 362.0 µs] | 58.2 [57.4, 59.5] | 1.006 [0.994, 1.040] | same |
| structural_beam3_frame_static | init | base-t1 | 3 | 1 | 29.96 ms [29.95 ms, 31.93 ms] | 29.96 ms [29.95 ms, 31.93 ms] | 4925.0 [4922.4, 5248.7] | – | same |
| structural_beam3_frame_static | init | head-t1 | 3 | 1 | 30.08 ms [29.63 ms, 32.08 ms] | 30.08 ms [29.63 ms, 32.08 ms] | 4944.6 [4870.5, 5272.5] | 1.005 [0.989, 1.005] | same |
| structural_beam3_frame_static | init | head-t2 | 3 | 1 | 29.71 ms [29.43 ms, 29.92 ms] | 29.71 ms [29.43 ms, 29.92 ms] | 4884.1 [4837.4, 4917.7] | 0.983 [0.931, 0.998] | same |
| structural_beam3_frame_static | init | head-t4 | 3 | 1 | 30.14 ms [30.11 ms, 30.19 ms] | 30.14 ms [30.11 ms, 30.19 ms] | 4954.3 [4949.4, 4961.7] | 1.006 [0.943, 1.008] | same |
| structural_beam3_frame_static | init | head-t8 | 3 | 1 | 30.15 ms [30.12 ms, 30.29 ms] | 30.15 ms [30.12 ms, 30.29 ms] | 4955.0 [4950.9, 4979.5] | 1.006 [0.943, 1.012] | same |
| structural_beam3_frame_static | assemble_stiffness | base-t1 | 3 | 5 | 29.97 ms [29.95 ms, 30.05 ms] | 5.99 ms [5.99 ms, 6.01 ms] | 985.1 [984.5, 987.8] | – | same |
| structural_beam3_frame_static | assemble_stiffness | head-t1 | 3 | 5 | 29.84 ms [29.73 ms, 29.86 ms] | 5.97 ms [5.95 ms, 5.97 ms] | 981.0 [977.4, 981.5] | 0.996 [0.990, 0.996] | same |
| structural_beam3_frame_static | assemble_stiffness | head-t2 | 3 | 5 | 28.00 ms [27.91 ms, 28.07 ms] | 5.60 ms [5.58 ms, 5.62 ms] | 920.6 [917.6, 922.9] | 0.935 [0.929, 0.937] | same |
| structural_beam3_frame_static | assemble_stiffness | head-t4 | 3 | 5 | 27.58 ms [27.47 ms, 27.70 ms] | 5.52 ms [5.49 ms, 5.54 ms] | 906.6 [903.1, 910.5] | 0.918 [0.917, 0.925] | same |
| structural_beam3_frame_static | assemble_stiffness | head-t8 | 3 | 5 | 28.25 ms [28.18 ms, 28.30 ms] | 5.65 ms [5.64 ms, 5.66 ms] | 928.5 [926.2, 930.3] | 0.942 [0.940, 0.943] | same |
| structural_beam3_frame_static | solve_step | base-t1 | 3 | 1 | 1.345 s [1.344 s, 1.353 s] | 1.345 s [1.344 s, 1.353 s] | 221103.6 [220924.6, 222332.0] | – | same |
| structural_beam3_frame_static | solve_step | head-t1 | 3 | 1 | 1.360 s [1.353 s, 1.362 s] | 1.360 s [1.353 s, 1.362 s] | 223557.9 [222343.0, 223840.4] | 1.006 [1.006, 1.012] | same |
| structural_beam3_frame_static | solve_step | head-t2 | 3 | 1 | 1.082 s [1.069 s, 1.083 s] | 1.082 s [1.069 s, 1.083 s] | 177785.8 [175661.2, 178042.6] | 0.800 [0.795, 0.805] | same |
| structural_beam3_frame_static | solve_step | head-t4 | 3 | 1 | 924.86 ms [918.30 ms, 928.58 ms] | 924.86 ms [918.30 ms, 928.58 ms] | 152015.6 [150936.9, 152626.9] | 0.684 [0.683, 0.690] | same |
| structural_beam3_frame_static | solve_step | head-t8 | 3 | 1 | 826.40 ms [825.55 ms, 827.23 ms] | 826.40 ms [825.55 ms, 827.23 ms] | 135831.4 [135691.8, 135968.8] | 0.614 [0.611, 0.615] | same |
| structural_dkt18_plate_static | mesh | base-t1 | 3 | 1 | 307.0 µs [307.0 µs, 315.0 µs] | 307.0 µs [307.0 µs, 315.0 µs] | 42.6 [42.6, 43.8] | – | same |
| structural_dkt18_plate_static | mesh | head-t1 | 3 | 1 | 304.0 µs [296.0 µs, 341.0 µs] | 304.0 µs [296.0 µs, 341.0 µs] | 42.2 [41.1, 47.4] | 0.990 [0.940, 1.111] | same |
| structural_dkt18_plate_static | mesh | head-t2 | 3 | 1 | 296.0 µs [290.0 µs, 312.0 µs] | 296.0 µs [290.0 µs, 312.0 µs] | 41.1 [40.3, 43.3] | 0.964 [0.945, 0.990] | same |
| structural_dkt18_plate_static | mesh | head-t4 | 3 | 1 | 302.0 µs [298.0 µs, 310.0 µs] | 302.0 µs [298.0 µs, 310.0 µs] | 41.9 [41.4, 43.1] | 0.971 [0.959, 1.010] | same |
| structural_dkt18_plate_static | mesh | head-t8 | 3 | 1 | 304.0 µs [296.0 µs, 305.0 µs] | 304.0 µs [296.0 µs, 305.0 µs] | 42.2 [41.1, 42.4] | 0.990 [0.940, 0.993] | same |
| structural_dkt18_plate_static | init | base-t1 | 3 | 1 | 69.46 ms [69.25 ms, 70.03 ms] | 69.46 ms [69.25 ms, 70.03 ms] | 9647.6 [9618.3, 9726.1] | – | same |
| structural_dkt18_plate_static | init | head-t1 | 3 | 1 | 69.53 ms [69.31 ms, 69.60 ms] | 69.53 ms [69.31 ms, 69.60 ms] | 9656.7 [9627.1, 9666.2] | 1.001 [0.993, 1.002] | same |
| structural_dkt18_plate_static | init | head-t2 | 3 | 1 | 69.42 ms [69.41 ms, 69.47 ms] | 69.42 ms [69.41 ms, 69.47 ms] | 9641.0 [9640.3, 9649.0] | 0.999 [0.991, 1.003] | same |
| structural_dkt18_plate_static | init | head-t4 | 3 | 1 | 69.37 ms [69.35 ms, 69.58 ms] | 69.37 ms [69.35 ms, 69.58 ms] | 9634.2 [9632.2, 9663.8] | 1.002 [0.990, 1.002] | same |
| structural_dkt18_plate_static | init | head-t8 | 3 | 1 | 70.07 ms [69.65 ms, 70.47 ms] | 70.07 ms [69.65 ms, 70.47 ms] | 9732.1 [9673.8, 9787.1] | 1.006 [1.003, 1.012] | same |
| structural_dkt18_plate_static | assemble_stiffness | base-t1 | 3 | 5 | 264.99 ms [264.78 ms, 267.46 ms] | 53.00 ms [52.96 ms, 53.49 ms] | 7360.7 [7355.0, 7429.4] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t1 | 3 | 5 | 265.02 ms [264.79 ms, 266.44 ms] | 53.00 ms [52.96 ms, 53.29 ms] | 7361.6 [7355.3, 7401.1] | 0.999 [0.996, 1.001] | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t2 | 3 | 5 | 258.84 ms [258.06 ms, 261.09 ms] | 51.77 ms [51.61 ms, 52.22 ms] | 7190.0 [7168.3, 7252.6] | 0.976 [0.974, 0.978] | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t4 | 3 | 5 | 257.12 ms [255.31 ms, 257.43 ms] | 51.42 ms [51.06 ms, 51.49 ms] | 7142.2 [7091.9, 7150.8] | 0.964 [0.962, 0.970] | same |
| structural_dkt18_plate_static | assemble_stiffness | head-t8 | 3 | 5 | 253.84 ms [251.14 ms, 254.17 ms] | 50.77 ms [50.23 ms, 50.83 ms] | 7051.1 [6976.1, 7060.3] | 0.949 [0.948, 0.959] | same |
| structural_dkt18_plate_static | solve_step | base-t1 | 3 | 1 | 905.53 ms [902.76 ms, 908.70 ms] | 905.53 ms [902.76 ms, 908.70 ms] | 125767.6 [125383.8, 126208.8] | – | same |
| structural_dkt18_plate_static | solve_step | head-t1 | 3 | 1 | 927.45 ms [927.41 ms, 930.26 ms] | 927.45 ms [927.41 ms, 930.26 ms] | 128811.9 [128806.7, 129203.2] | 1.027 [1.021, 1.027] | same |
| structural_dkt18_plate_static | solve_step | head-t2 | 3 | 1 | 902.79 ms [895.64 ms, 914.59 ms] | 902.79 ms [895.64 ms, 914.59 ms] | 125387.6 [124394.7, 127026.0] | 0.997 [0.986, 1.013] | same |
| structural_dkt18_plate_static | solve_step | head-t4 | 3 | 1 | 896.38 ms [892.77 ms, 899.69 ms] | 896.38 ms [892.77 ms, 899.69 ms] | 124496.7 [123995.7, 124956.8] | 0.989 [0.986, 0.994] | same |
| structural_dkt18_plate_static | solve_step | head-t8 | 3 | 1 | 888.88 ms [886.55 ms, 890.65 ms] | 888.88 ms [886.55 ms, 890.65 ms] | 123456.0 [123131.5, 123701.4] | 0.980 [0.979, 0.985] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 87dd2d994, d404f4c70 | 55313c35e6ae3f79 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 87dd2d994, d404f4c70 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 87dd2d994, d404f4c70 | a77809465b3e7c18 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 87dd2d994, d404f4c70 | d5869f57a777d9a5 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 87dd2d994, d404f4c70 | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 87dd2d994, d404f4c70 | 1cfc7f010c52d6b3 |
