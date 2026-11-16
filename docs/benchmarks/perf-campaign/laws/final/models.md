Suite `models`, size `m`; variants: `before`, `final`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_quad4 | mesh | before | 5 | 1 | 153.97 ms [150.98 ms, 155.35 ms] | 153.97 ms [150.98 ms, 155.35 ms] | 962.3 [943.6, 971.0] | – | same |
| heat_explicit_quad4 | mesh | final | 5 | 1 | 154.94 ms [153.78 ms, 155.27 ms] | 154.94 ms [153.78 ms, 155.27 ms] | 968.4 [961.1, 970.4] | 1.005 [0.997, 1.028] | same |
| heat_explicit_quad4 | init | before | 5 | 1 | 20.95 ms [20.54 ms, 21.28 ms] | 20.95 ms [20.54 ms, 21.28 ms] | 130.9 [128.4, 133.0] | – | same |
| heat_explicit_quad4 | init | final | 5 | 1 | 21.02 ms [20.25 ms, 21.29 ms] | 21.02 ms [20.25 ms, 21.29 ms] | 131.4 [126.6, 133.1] | 1.001 [0.952, 1.028] | same |
| heat_explicit_quad4 | stable_time_step | before | 5 | 1 | 2.47 ms [2.47 ms, 2.48 ms] | 2.47 ms [2.47 ms, 2.48 ms] | 15.5 [15.4, 15.5] | – | same |
| heat_explicit_quad4 | stable_time_step | final | 5 | 1 | 2.48 ms [2.46 ms, 2.51 ms] | 2.48 ms [2.46 ms, 2.51 ms] | 15.5 [15.4, 15.7] | 1.004 [0.995, 1.017] | same |
| heat_explicit_quad4 | solve_step | before | 5 | 120 | 2.388 s [2.384 s, 2.401 s] | 19.90 ms [19.87 ms, 20.01 ms] | 124.4 [124.2, 125.1] | – | same |
| heat_explicit_quad4 | solve_step | final | 5 | 120 | 1.237 s [1.228 s, 1.292 s] | 10.31 ms [10.23 ms, 10.77 ms] | 64.4 [63.9, 67.3] | 0.518 [0.515, 0.542] | same |
| heat_explicit_tet4 | mesh | before | 5 | 1 | 228.09 ms [226.84 ms, 230.60 ms] | 228.09 ms [226.84 ms, 230.60 ms] | 814.8 [810.3, 823.7] | – | same |
| heat_explicit_tet4 | mesh | final | 5 | 1 | 228.98 ms [227.56 ms, 229.91 ms] | 228.98 ms [227.56 ms, 229.91 ms] | 818.0 [812.9, 821.3] | 1.008 [0.987, 1.011] | same |
| heat_explicit_tet4 | init | before | 5 | 1 | 11.30 ms [11.04 ms, 11.44 ms] | 11.30 ms [11.04 ms, 11.44 ms] | 40.4 [39.5, 40.9] | – | same |
| heat_explicit_tet4 | init | final | 5 | 1 | 11.36 ms [10.99 ms, 11.39 ms] | 11.36 ms [10.99 ms, 11.39 ms] | 40.6 [39.3, 40.7] | 0.995 [0.979, 1.013] | same |
| heat_explicit_tet4 | stable_time_step | before | 5 | 1 | 9.57 ms [9.55 ms, 9.66 ms] | 9.57 ms [9.55 ms, 9.66 ms] | 34.2 [34.1, 34.5] | – | same |
| heat_explicit_tet4 | stable_time_step | final | 5 | 1 | 9.58 ms [9.53 ms, 9.63 ms] | 9.58 ms [9.53 ms, 9.63 ms] | 34.2 [34.1, 34.4] | 0.999 [0.989, 1.006] | same |
| heat_explicit_tet4 | solve_step | before | 5 | 100 | 1.977 s [1.971 s, 1.996 s] | 19.77 ms [19.71 ms, 19.96 ms] | 70.6 [70.4, 71.3] | – | same |
| heat_explicit_tet4 | solve_step | final | 5 | 100 | 976.39 ms [974.01 ms, 995.43 ms] | 9.76 ms [9.74 ms, 9.95 ms] | 34.9 [34.8, 35.6] | 0.495 [0.488, 0.503] | same |
| heat_implicit_hex8_kvar | mesh | before | 5 | 1 | 3.20 ms [3.17 ms, 3.23 ms] | 3.20 ms [3.17 ms, 3.23 ms] | 1849.5 [1832.8, 1867.5] | – | same |
| heat_implicit_hex8_kvar | mesh | final | 5 | 1 | 3.22 ms [3.20 ms, 3.25 ms] | 3.22 ms [3.20 ms, 3.25 ms] | 1864.6 [1850.7, 1883.7] | 1.008 [0.991, 1.019] | same |
| heat_implicit_hex8_kvar | init | before | 5 | 1 | 545.0 µs [509.0 µs, 564.0 µs] | 545.0 µs [509.0 µs, 564.0 µs] | 315.4 [294.6, 326.4] | – | same |
| heat_implicit_hex8_kvar | init | final | 5 | 1 | 525.0 µs [513.0 µs, 538.0 µs] | 525.0 µs [513.0 µs, 538.0 µs] | 303.8 [296.9, 311.3] | 0.941 [0.931, 1.057] | same |
| heat_implicit_hex8_kvar | solve_step | before | 5 | 5 | 878.03 ms [872.97 ms, 880.95 ms] | 175.61 ms [174.59 ms, 176.19 ms] | 101623.8 [101037.8, 101961.7] | – | same |
| heat_implicit_hex8_kvar | solve_step | final | 5 | 5 | 870.38 ms [867.92 ms, 877.88 ms] | 174.08 ms [173.58 ms, 175.58 ms] | 100737.8 [100453.4, 101606.9] | 0.993 [0.989, 0.997] | same |
| heat_static_tri6 | mesh | before | 5 | 1 | 33.55 ms [33.25 ms, 33.95 ms] | 33.55 ms [33.25 ms, 33.95 ms] | 1386.5 [1374.1, 1402.8] | – | same |
| heat_static_tri6 | mesh | final | 5 | 1 | 34.22 ms [33.72 ms, 34.47 ms] | 34.22 ms [33.72 ms, 34.47 ms] | 1413.9 [1393.3, 1424.4] | 1.021 [0.993, 1.032] | same |
| heat_static_tri6 | init | before | 5 | 1 | 3.92 ms [3.86 ms, 3.93 ms] | 3.92 ms [3.86 ms, 3.93 ms] | 162.0 [159.4, 162.4] | – | same |
| heat_static_tri6 | init | final | 5 | 1 | 3.91 ms [3.89 ms, 3.92 ms] | 3.91 ms [3.89 ms, 3.92 ms] | 161.4 [160.7, 161.8] | 0.999 [0.990, 1.012] | same |
| heat_static_tri6 | assemble_conductivity | before | 5 | 5 | 122.82 ms [121.75 ms, 124.37 ms] | 24.56 ms [24.35 ms, 24.87 ms] | 1015.0 [1006.2, 1027.8] | – | same |
| heat_static_tri6 | assemble_conductivity | final | 5 | 5 | 126.27 ms [123.36 ms, 152.94 ms] | 25.25 ms [24.67 ms, 30.59 ms] | 1043.6 [1019.5, 1264.0] | 1.018 [1.007, 1.245] | same |
| heat_static_tri6 | solve_step | before | 5 | 1 | 2.988 s [2.964 s, 3.095 s] | 2.988 s [2.964 s, 3.095 s] | 123450.7 [122469.5, 127886.7] | – | same |
| heat_static_tri6 | solve_step | final | 5 | 1 | 2.961 s [2.957 s, 2.990 s] | 2.961 s [2.957 s, 2.990 s] | 122355.4 [122179.3, 123549.5] | 0.993 [0.966, 0.999] | same |
| phasefield_staggered_tet4 | mesh | before | 5 | 1 | 5.78 ms [5.75 ms, 5.80 ms] | 5.78 ms [5.75 ms, 5.80 ms] | 962.7 [957.8, 966.2] | – | same |
| phasefield_staggered_tet4 | mesh | final | 5 | 1 | 5.79 ms [5.76 ms, 5.83 ms] | 5.79 ms [5.76 ms, 5.83 ms] | 965.7 [959.8, 970.8] | 1.002 [0.998, 1.011] | same |
| phasefield_staggered_tet4 | init | before | 5 | 1 | 1.55 ms [1.54 ms, 1.56 ms] | 1.55 ms [1.54 ms, 1.56 ms] | 257.7 [257.0, 259.5] | – | same |
| phasefield_staggered_tet4 | init | final | 5 | 1 | 1.53 ms [1.51 ms, 1.55 ms] | 1.53 ms [1.51 ms, 1.55 ms] | 255.8 [251.8, 257.7] | 0.993 [0.970, 1.003] | same |
| phasefield_staggered_tet4 | solid_solve | before | 5 | 4 | 1.390 s [1.387 s, 1.395 s] | 347.43 ms [346.66 ms, 348.72 ms] | 57905.2 [57777.4, 58119.7] | – | same |
| phasefield_staggered_tet4 | solid_solve | final | 5 | 4 | 1.386 s [1.382 s, 1.405 s] | 346.62 ms [345.40 ms, 351.18 ms] | 57770.4 [57567.2, 58529.8] | 1.000 [0.994, 1.007] | same |
| phasefield_staggered_tet4 | strain_transfer | before | 5 | 4 | 427.0 µs [420.0 µs, 434.0 µs] | 106.8 µs [105.0 µs, 108.5 µs] | 17.8 [17.5, 18.1] | – | same |
| phasefield_staggered_tet4 | strain_transfer | final | 5 | 4 | 427.0 µs [422.0 µs, 453.0 µs] | 106.8 µs [105.5 µs, 113.2 µs] | 17.8 [17.6, 18.9] | 1.002 [0.984, 1.079] | same |
| phasefield_staggered_tet4 | phase_solve | before | 5 | 4 | 91.31 ms [91.01 ms, 91.65 ms] | 22.83 ms [22.75 ms, 22.91 ms] | 3804.6 [3792.0, 3818.9] | – | same |
| phasefield_staggered_tet4 | phase_solve | final | 5 | 4 | 91.20 ms [90.62 ms, 92.80 ms] | 22.80 ms [22.65 ms, 23.20 ms] | 3800.0 [3775.7, 3866.8] | 0.996 [0.992, 1.020] | same |
| phasefield_staggered_tet4 | damage_transfer | before | 5 | 4 | 1.07 ms [1.07 ms, 1.08 ms] | 268.8 µs [267.0 µs, 271.2 µs] | 44.8 [44.5, 45.2] | – | same |
| phasefield_staggered_tet4 | damage_transfer | final | 5 | 4 | 1.07 ms [1.06 ms, 1.07 ms] | 267.8 µs [265.2 µs, 268.5 µs] | 44.6 [44.2, 44.8] | 0.988 [0.983, 1.004] | same |
| phasefield_staggered_tet4 | internal_forces | before | 5 | 4 | 1.32 ms [1.31 ms, 1.33 ms] | 330.2 µs [328.8 µs, 332.2 µs] | 55.0 [54.8, 55.4] | – | same |
| phasefield_staggered_tet4 | internal_forces | final | 5 | 4 | 1.32 ms [1.31 ms, 1.35 ms] | 329.5 µs [326.8 µs, 338.0 µs] | 54.9 [54.5, 56.3] | 0.996 [0.985, 1.026] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | before | 5 | 5 | 59.51 ms [57.44 ms, 61.61 ms] | 11.90 ms [11.49 ms, 12.32 ms] | 1983.7 [1914.6, 2053.6] | – | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | final | 5 | 5 | 59.73 ms [57.00 ms, 61.30 ms] | 11.95 ms [11.40 ms, 12.26 ms] | 1990.9 [1900.0, 2043.3] | 0.989 [0.974, 1.030] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | before | 5 | 5 | 14.39 ms [13.93 ms, 14.62 ms] | 2.88 ms [2.79 ms, 2.92 ms] | 479.6 [464.3, 487.4] | – | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | final | 5 | 5 | 14.09 ms [13.98 ms, 14.51 ms] | 2.82 ms [2.80 ms, 2.90 ms] | 469.6 [466.0, 483.7] | 0.972 [0.957, 1.031] | same |
| phasefield_staggered_tri3 | mesh | before | 5 | 1 | 37.66 ms [37.05 ms, 37.98 ms] | 37.66 ms [37.05 ms, 37.98 ms] | 735.5 [723.6, 741.9] | – | same |
| phasefield_staggered_tri3 | mesh | final | 5 | 1 | 37.66 ms [37.33 ms, 38.64 ms] | 37.66 ms [37.33 ms, 38.64 ms] | 735.5 [729.0, 754.6] | 0.996 [0.984, 1.043] | same |
| phasefield_staggered_tri3 | init | before | 5 | 1 | 10.88 ms [10.82 ms, 10.93 ms] | 10.88 ms [10.82 ms, 10.93 ms] | 212.4 [211.4, 213.4] | – | same |
| phasefield_staggered_tri3 | init | final | 5 | 1 | 10.83 ms [10.64 ms, 10.95 ms] | 10.83 ms [10.64 ms, 10.95 ms] | 211.6 [207.9, 213.8] | 0.992 [0.983, 1.007] | same |
| phasefield_staggered_tri3 | solid_solve | before | 5 | 4 | 7.041 s [7.000 s, 7.075 s] | 1.760 s [1.750 s, 1.769 s] | 34377.5 [34177.3, 34544.0] | – | same |
| phasefield_staggered_tri3 | solid_solve | final | 5 | 4 | 7.028 s [7.022 s, 7.063 s] | 1.757 s [1.755 s, 1.766 s] | 34315.0 [34286.5, 34486.6] | 0.998 [0.995, 1.004] | same |
| phasefield_staggered_tri3 | strain_transfer | before | 5 | 4 | 2.40 ms [2.40 ms, 2.43 ms] | 601.0 µs [599.5 µs, 606.5 µs] | 11.7 [11.7, 11.8] | – | same |
| phasefield_staggered_tri3 | strain_transfer | final | 5 | 4 | 2.43 ms [2.41 ms, 2.51 ms] | 608.0 µs [602.2 µs, 626.5 µs] | 11.9 [11.8, 12.2] | 1.012 [1.002, 1.033] | same |
| phasefield_staggered_tri3 | phase_solve | before | 5 | 4 | 1.325 s [1.311 s, 1.328 s] | 331.33 ms [327.87 ms, 332.10 ms] | 6471.3 [6403.8, 6486.3] | – | same |
| phasefield_staggered_tri3 | phase_solve | final | 5 | 4 | 1.312 s [1.309 s, 1.323 s] | 327.89 ms [327.33 ms, 330.69 ms] | 6404.1 [6393.2, 6458.7] | 0.995 [0.986, 1.000] | same |
| phasefield_staggered_tri3 | damage_transfer | before | 5 | 4 | 8.10 ms [8.06 ms, 8.12 ms] | 2.02 ms [2.01 ms, 2.03 ms] | 39.5 [39.3, 39.6] | – | same |
| phasefield_staggered_tri3 | damage_transfer | final | 5 | 4 | 8.00 ms [7.99 ms, 8.52 ms] | 2.00 ms [2.00 ms, 2.13 ms] | 39.1 [39.0, 41.6] | 0.993 [0.985, 1.054] | same |
| phasefield_staggered_tri3 | internal_forces | before | 5 | 4 | 7.28 ms [7.26 ms, 7.36 ms] | 1.82 ms [1.81 ms, 1.84 ms] | 35.6 [35.4, 35.9] | – | same |
| phasefield_staggered_tri3 | internal_forces | final | 5 | 4 | 7.30 ms [7.27 ms, 7.32 ms] | 1.83 ms [1.82 ms, 1.83 ms] | 35.7 [35.5, 35.7] | 0.999 [0.994, 1.006] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | before | 5 | 5 | 219.48 ms [211.92 ms, 234.42 ms] | 43.90 ms [42.38 ms, 46.88 ms] | 857.3 [827.8, 915.7] | – | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | final | 5 | 5 | 219.59 ms [216.41 ms, 229.38 ms] | 43.92 ms [43.28 ms, 45.88 ms] | 857.8 [845.3, 896.0] | 0.986 [0.978, 1.030] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | before | 5 | 5 | 101.63 ms [100.68 ms, 106.25 ms] | 20.33 ms [20.14 ms, 21.25 ms] | 397.0 [393.3, 415.0] | – | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | final | 5 | 5 | 103.61 ms [101.56 ms, 104.22 ms] | 20.72 ms [20.31 ms, 20.84 ms] | 404.7 [396.7, 407.1] | 1.009 [0.970, 1.031] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | 78a330615, f25a10053 | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 78a330615, f25a10053 | b4798ad9264a3e5e |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | 78a330615, f25a10053 | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 78a330615, f25a10053 | 463949e032392419 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 78a330615, f25a10053 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 78a330615, f25a10053 | a77809465b3e7c18 |
