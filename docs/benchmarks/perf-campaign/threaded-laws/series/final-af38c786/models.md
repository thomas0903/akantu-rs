Suite `models`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_quad4 | solve_step | base-t1 | 5 | 120 | 1.262 s [1.241 s, 1.306 s] | 10.52 ms [10.34 ms, 10.89 ms] | 65.7 [64.6, 68.0] | – | same |
| heat_explicit_quad4 | solve_step | head-t1 | 5 | 120 | 1.259 s [1.236 s, 1.291 s] | 10.49 ms [10.30 ms, 10.76 ms] | 65.6 [64.4, 67.2] | 0.988 [0.986, 1.015] | same |
| heat_explicit_quad4 | solve_step | head-t2 | 5 | 120 | 800.36 ms [794.44 ms, 809.75 ms] | 6.67 ms [6.62 ms, 6.75 ms] | 41.7 [41.4, 42.2] | 0.635 [0.613, 0.644] | same |
| heat_explicit_quad4 | solve_step | head-t4 | 5 | 120 | 700.12 ms [655.78 ms, 715.80 ms] | 5.83 ms [5.46 ms, 5.96 ms] | 36.5 [34.2, 37.3] | 0.539 [0.524, 0.564] | same |
| heat_explicit_quad4 | solve_step | head-t8 | 5 | 120 | 634.58 ms [633.01 ms, 649.73 ms] | 5.29 ms [5.28 ms, 5.41 ms] | 33.1 [33.0, 33.8] | 0.507 [0.485, 0.515] | same |
| heat_explicit_tet4 | solve_step | base-t1 | 5 | 100 | 997.39 ms [989.05 ms, 1.006 s] | 9.97 ms [9.89 ms, 10.06 ms] | 35.6 [35.3, 35.9] | – | same |
| heat_explicit_tet4 | solve_step | head-t1 | 5 | 100 | 1.007 s [994.35 ms, 1.012 s] | 10.07 ms [9.94 ms, 10.12 ms] | 36.0 [35.5, 36.1] | 1.005 [0.998, 1.017] | same |
| heat_explicit_tet4 | solve_step | head-t2 | 5 | 100 | 596.22 ms [585.11 ms, 624.99 ms] | 5.96 ms [5.85 ms, 6.25 ms] | 21.3 [20.9, 22.3] | 0.594 [0.588, 0.627] | same |
| heat_explicit_tet4 | solve_step | head-t4 | 5 | 100 | 411.41 ms [408.10 ms, 424.45 ms] | 4.11 ms [4.08 ms, 4.24 ms] | 14.7 [14.6, 15.2] | 0.414 [0.408, 0.422] | same |
| heat_explicit_tet4 | solve_step | head-t8 | 5 | 100 | 360.15 ms [358.82 ms, 369.91 ms] | 3.60 ms [3.59 ms, 3.70 ms] | 12.9 [12.8, 13.2] | 0.361 [0.358, 0.368] | same |
| phasefield_staggered_tet4 | solid_solve | base-t1 | 5 | 4 | 1.299 s [1.294 s, 1.313 s] | 324.65 ms [323.51 ms, 328.22 ms] | 54107.8 [53918.9, 54703.1] | – | same |
| phasefield_staggered_tet4 | solid_solve | head-t1 | 5 | 4 | 1.299 s [1.283 s, 1.305 s] | 324.73 ms [320.71 ms, 326.21 ms] | 54121.4 [53452.2, 54368.6] | 0.998 [0.991, 1.002] | same |
| phasefield_staggered_tet4 | solid_solve | head-t2 | 5 | 4 | 1.293 s [1.291 s, 1.317 s] | 323.26 ms [322.76 ms, 329.20 ms] | 53876.6 [53793.1, 54867.5] | 0.999 [0.996, 1.007] | same |
| phasefield_staggered_tet4 | solid_solve | head-t4 | 5 | 4 | 1.299 s [1.294 s, 1.308 s] | 324.76 ms [323.41 ms, 326.89 ms] | 54126.9 [53902.4, 54481.7] | 1.000 [0.989, 1.005] | same |
| phasefield_staggered_tet4 | solid_solve | head-t8 | 5 | 4 | 1.305 s [1.302 s, 1.331 s] | 326.27 ms [325.41 ms, 332.78 ms] | 54378.5 [54235.0, 55463.2] | 1.006 [0.996, 1.023] | same |
| phasefield_staggered_tet4 | phase_solve | base-t1 | 5 | 4 | 75.72 ms [75.30 ms, 76.83 ms] | 18.93 ms [18.82 ms, 19.21 ms] | 3155.2 [3137.3, 3201.0] | – | same |
| phasefield_staggered_tet4 | phase_solve | head-t1 | 5 | 4 | 76.26 ms [75.61 ms, 77.27 ms] | 19.06 ms [18.90 ms, 19.32 ms] | 3177.5 [3150.5, 3219.7] | 1.007 [0.998, 1.013] | same |
| phasefield_staggered_tet4 | phase_solve | head-t2 | 5 | 4 | 75.35 ms [74.98 ms, 76.81 ms] | 18.84 ms [18.74 ms, 19.20 ms] | 3139.7 [3124.0, 3200.5] | 1.000 [0.990, 1.009] | same |
| phasefield_staggered_tet4 | phase_solve | head-t4 | 5 | 4 | 74.91 ms [74.43 ms, 75.62 ms] | 18.73 ms [18.61 ms, 18.91 ms] | 3121.0 [3101.1, 3150.9] | 0.988 [0.975, 0.999] | same |
| phasefield_staggered_tet4 | phase_solve | head-t8 | 5 | 4 | 75.33 ms [74.83 ms, 77.02 ms] | 18.83 ms [18.71 ms, 19.25 ms] | 3138.6 [3118.0, 3209.0] | 0.994 [0.983, 1.018] | same |
| phasefield_staggered_tet4 | internal_forces | base-t1 | 5 | 4 | 1.42 ms [1.41 ms, 1.50 ms] | 354.0 µs [352.0 µs, 374.5 µs] | 59.0 [58.7, 62.4] | – | same |
| phasefield_staggered_tet4 | internal_forces | head-t1 | 5 | 4 | 1.41 ms [1.40 ms, 1.43 ms] | 352.8 µs [349.5 µs, 356.5 µs] | 58.8 [58.2, 59.4] | 0.993 [0.943, 1.001] | same |
| phasefield_staggered_tet4 | internal_forces | head-t2 | 5 | 4 | 901.0 µs [890.0 µs, 925.0 µs] | 225.2 µs [222.5 µs, 231.2 µs] | 37.5 [37.1, 38.5] | 0.632 [0.614, 0.649] | same |
| phasefield_staggered_tet4 | internal_forces | head-t4 | 5 | 4 | 798.0 µs [787.0 µs, 810.0 µs] | 199.5 µs [196.8 µs, 202.5 µs] | 33.2 [32.8, 33.8] | 0.567 [0.533, 0.572] | same |
| phasefield_staggered_tet4 | internal_forces | head-t8 | 5 | 4 | 787.0 µs [780.0 µs, 819.0 µs] | 196.8 µs [195.0 µs, 204.8 µs] | 32.8 [32.5, 34.1] | 0.554 [0.532, 0.574] | same |
| phasefield_staggered_tri3 | solid_solve | base-t1 | 5 | 4 | 6.733 s [6.723 s, 6.768 s] | 1.683 s [1.681 s, 1.692 s] | 32875.2 [32826.2, 33047.8] | – | same |
| phasefield_staggered_tri3 | solid_solve | head-t1 | 5 | 4 | 6.755 s [6.733 s, 6.878 s] | 1.689 s [1.683 s, 1.720 s] | 32983.5 [32876.2, 33585.5] | 1.004 [1.000, 1.016] | same |
| phasefield_staggered_tri3 | solid_solve | head-t2 | 5 | 4 | 6.685 s [6.680 s, 6.778 s] | 1.671 s [1.670 s, 1.695 s] | 32642.9 [32616.8, 33098.0] | 0.993 [0.992, 1.002] | same |
| phasefield_staggered_tri3 | solid_solve | head-t4 | 5 | 4 | 6.746 s [6.704 s, 6.823 s] | 1.687 s [1.676 s, 1.706 s] | 32939.6 [32735.4, 33313.1] | 1.002 [0.996, 1.008] | same |
| phasefield_staggered_tri3 | solid_solve | head-t8 | 5 | 4 | 6.775 s [6.718 s, 6.810 s] | 1.694 s [1.680 s, 1.703 s] | 33081.5 [32803.7, 33252.5] | 1.002 [0.997, 1.013] | same |
| phasefield_staggered_tri3 | phase_solve | base-t1 | 5 | 4 | 1.218 s [1.213 s, 1.227 s] | 304.51 ms [303.25 ms, 306.80 ms] | 5947.4 [5922.9, 5992.1] | – | same |
| phasefield_staggered_tri3 | phase_solve | head-t1 | 5 | 4 | 1.224 s [1.215 s, 1.251 s] | 306.00 ms [303.64 ms, 312.81 ms] | 5976.5 [5930.4, 6109.5] | 1.005 [1.001, 1.020] | same |
| phasefield_staggered_tri3 | phase_solve | head-t2 | 5 | 4 | 1.194 s [1.184 s, 1.223 s] | 298.57 ms [296.04 ms, 305.84 ms] | 5831.5 [5782.1, 5973.5] | 0.982 [0.976, 0.997] | same |
| phasefield_staggered_tri3 | phase_solve | head-t4 | 5 | 4 | 1.230 s [1.227 s, 1.241 s] | 307.48 ms [306.84 ms, 310.30 ms] | 6005.4 [5992.9, 6060.5] | 1.012 [1.008, 1.015] | same |
| phasefield_staggered_tri3 | phase_solve | head-t8 | 5 | 4 | 1.213 s [1.205 s, 1.236 s] | 303.29 ms [301.14 ms, 309.01 ms] | 5923.7 [5881.6, 6035.4] | 0.996 [0.987, 1.018] | same |
| phasefield_staggered_tri3 | internal_forces | base-t1 | 5 | 4 | 7.34 ms [7.33 ms, 7.39 ms] | 1.84 ms [1.83 ms, 1.85 ms] | 35.8 [35.8, 36.1] | – | same |
| phasefield_staggered_tri3 | internal_forces | head-t1 | 5 | 4 | 7.53 ms [7.37 ms, 7.72 ms] | 1.88 ms [1.84 ms, 1.93 ms] | 36.7 [36.0, 37.7] | 1.020 [1.004, 1.054] | same |
| phasefield_staggered_tri3 | internal_forces | head-t2 | 5 | 4 | 4.17 ms [4.10 ms, 4.39 ms] | 1.04 ms [1.02 ms, 1.10 ms] | 20.4 [20.0, 21.5] | 0.564 [0.555, 0.600] | same |
| phasefield_staggered_tri3 | internal_forces | head-t4 | 5 | 4 | 3.08 ms [3.07 ms, 3.13 ms] | 770.8 µs [767.8 µs, 782.5 µs] | 15.1 [15.0, 15.3] | 0.419 [0.417, 0.427] | same |
| phasefield_staggered_tri3 | internal_forces | head-t8 | 5 | 4 | 3.29 ms [3.25 ms, 3.36 ms] | 821.2 µs [813.0 µs, 839.0 µs] | 16.0 [15.9, 16.4] | 0.447 [0.443, 0.457] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | af38c786d, c9eef15b8 | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | af38c786d, c9eef15b8 | b4798ad9264a3e5e |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | af38c786d, c9eef15b8 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | af38c786d, c9eef15b8 | a77809465b3e7c18 |
