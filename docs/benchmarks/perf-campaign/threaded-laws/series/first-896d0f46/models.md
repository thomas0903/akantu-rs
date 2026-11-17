Suite `models`, size `m`; variants: `base-t1`, `head-t1`, `head-t2`, `head-t4`, `head-t8`; reference `base-t1`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base-t1 | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_quad4 | solve_step | base-t1 | 5 | 120 | 1.236 s [1.226 s, 1.243 s] | 10.30 ms [10.22 ms, 10.36 ms] | 64.4 [63.9, 64.7] | – | same |
| heat_explicit_quad4 | solve_step | head-t1 | 5 | 120 | 1.243 s [1.223 s, 1.253 s] | 10.36 ms [10.20 ms, 10.44 ms] | 64.7 [63.7, 65.3] | 1.001 [0.993, 1.010] | same |
| heat_explicit_quad4 | solve_step | head-t2 | 5 | 120 | 784.97 ms [776.60 ms, 797.49 ms] | 6.54 ms [6.47 ms, 6.65 ms] | 40.9 [40.4, 41.5] | 0.634 [0.630, 0.643] | same |
| heat_explicit_quad4 | solve_step | head-t4 | 5 | 120 | 694.06 ms [650.89 ms, 707.52 ms] | 5.78 ms [5.42 ms, 5.90 ms] | 36.1 [33.9, 36.9] | 0.566 [0.525, 0.570] | same |
| heat_explicit_quad4 | solve_step | head-t8 | 5 | 120 | 633.47 ms [626.19 ms, 641.43 ms] | 5.28 ms [5.22 ms, 5.35 ms] | 33.0 [32.6, 33.4] | 0.513 [0.506, 0.518] | same |
| heat_explicit_tet4 | solve_step | base-t1 | 5 | 100 | 999.95 ms [987.56 ms, 1.005 s] | 10.00 ms [9.88 ms, 10.05 ms] | 35.7 [35.3, 35.9] | – | same |
| heat_explicit_tet4 | solve_step | head-t1 | 5 | 100 | 997.04 ms [986.66 ms, 1.008 s] | 9.97 ms [9.87 ms, 10.08 ms] | 35.6 [35.2, 36.0] | 1.000 [0.997, 1.003] | same |
| heat_explicit_tet4 | solve_step | head-t2 | 5 | 100 | 583.88 ms [576.71 ms, 634.90 ms] | 5.84 ms [5.77 ms, 6.35 ms] | 20.9 [20.6, 22.7] | 0.585 [0.582, 0.631] | same |
| heat_explicit_tet4 | solve_step | head-t4 | 5 | 100 | 417.43 ms [408.15 ms, 426.91 ms] | 4.17 ms [4.08 ms, 4.27 ms] | 14.9 [14.6, 15.3] | 0.415 [0.412, 0.432] | same |
| heat_explicit_tet4 | solve_step | head-t8 | 5 | 100 | 362.54 ms [359.05 ms, 367.78 ms] | 3.63 ms [3.59 ms, 3.68 ms] | 13.0 [12.8, 13.1] | 0.365 [0.360, 0.367] | same |
| phasefield_staggered_tet4 | solid_solve | base-t1 | 5 | 4 | 1.293 s [1.289 s, 1.305 s] | 323.25 ms [322.26 ms, 326.31 ms] | 53874.9 [53710.5, 54385.2] | – | same |
| phasefield_staggered_tet4 | solid_solve | head-t1 | 5 | 4 | 1.299 s [1.287 s, 1.301 s] | 324.80 ms [321.78 ms, 325.29 ms] | 54133.0 [53630.3, 54215.1] | 0.998 [0.995, 1.009] | same |
| phasefield_staggered_tet4 | solid_solve | head-t2 | 5 | 4 | 1.293 s [1.289 s, 1.302 s] | 323.16 ms [322.29 ms, 325.50 ms] | 53860.8 [53714.4, 54250.4] | 1.000 [0.998, 1.001] | same |
| phasefield_staggered_tet4 | solid_solve | head-t4 | 5 | 4 | 1.296 s [1.293 s, 1.310 s] | 323.93 ms [323.18 ms, 327.44 ms] | 53988.2 [53863.9, 54573.7] | 1.003 [1.000, 1.005] | same |
| phasefield_staggered_tet4 | solid_solve | head-t8 | 5 | 4 | 1.302 s [1.301 s, 1.312 s] | 325.53 ms [325.14 ms, 328.10 ms] | 54255.2 [54189.5, 54683.7] | 1.007 [1.005, 1.009] | same |
| phasefield_staggered_tet4 | phase_solve | base-t1 | 5 | 4 | 75.66 ms [75.48 ms, 76.32 ms] | 18.92 ms [18.87 ms, 19.08 ms] | 3152.7 [3145.0, 3180.0] | – | same |
| phasefield_staggered_tet4 | phase_solve | head-t1 | 5 | 4 | 75.34 ms [75.22 ms, 75.86 ms] | 18.84 ms [18.81 ms, 18.97 ms] | 3139.2 [3134.3, 3160.9] | 0.996 [0.987, 1.003] | same |
| phasefield_staggered_tet4 | phase_solve | head-t2 | 5 | 4 | 74.97 ms [74.41 ms, 75.53 ms] | 18.74 ms [18.60 ms, 18.88 ms] | 3123.8 [3100.6, 3146.9] | 0.990 [0.985, 0.995] | same |
| phasefield_staggered_tet4 | phase_solve | head-t4 | 5 | 4 | 74.56 ms [74.14 ms, 75.13 ms] | 18.64 ms [18.53 ms, 18.78 ms] | 3106.5 [3089.0, 3130.4] | 0.984 [0.982, 0.986] | same |
| phasefield_staggered_tet4 | phase_solve | head-t8 | 5 | 4 | 74.67 ms [74.50 ms, 75.41 ms] | 18.67 ms [18.63 ms, 18.85 ms] | 3111.2 [3104.2, 3142.2] | 0.988 [0.979, 0.989] | same |
| phasefield_staggered_tet4 | internal_forces | base-t1 | 5 | 4 | 1.41 ms [1.39 ms, 1.41 ms] | 351.2 µs [346.8 µs, 353.2 µs] | 58.5 [57.8, 58.9] | – | same |
| phasefield_staggered_tet4 | internal_forces | head-t1 | 5 | 4 | 1.40 ms [1.39 ms, 1.41 ms] | 349.8 µs [346.2 µs, 351.2 µs] | 58.3 [57.7, 58.5] | 0.996 [0.980, 1.012] | same |
| phasefield_staggered_tet4 | internal_forces | head-t2 | 5 | 4 | 894.0 µs [866.0 µs, 899.0 µs] | 223.5 µs [216.5 µs, 224.8 µs] | 37.2 [36.1, 37.5] | 0.636 [0.624, 0.636] | same |
| phasefield_staggered_tet4 | internal_forces | head-t4 | 5 | 4 | 797.0 µs [788.0 µs, 808.0 µs] | 199.2 µs [197.0 µs, 202.0 µs] | 33.2 [32.8, 33.7] | 0.564 [0.561, 0.580] | same |
| phasefield_staggered_tet4 | internal_forces | head-t8 | 5 | 4 | 786.0 µs [778.0 µs, 792.0 µs] | 196.5 µs [194.5 µs, 198.0 µs] | 32.8 [32.4, 33.0] | 0.561 [0.551, 0.567] | same |
| phasefield_staggered_tri3 | solid_solve | base-t1 | 5 | 4 | 6.683 s [6.649 s, 6.784 s] | 1.671 s [1.662 s, 1.696 s] | 32631.2 [32467.1, 33123.7] | – | same |
| phasefield_staggered_tri3 | solid_solve | head-t1 | 5 | 4 | 6.685 s [6.675 s, 6.748 s] | 1.671 s [1.669 s, 1.687 s] | 32642.2 [32592.3, 32950.0] | 1.001 [0.995, 1.005] | same |
| phasefield_staggered_tri3 | solid_solve | head-t2 | 5 | 4 | 6.654 s [6.647 s, 6.734 s] | 1.664 s [1.662 s, 1.684 s] | 32492.5 [32456.7, 32881.7] | 0.996 [0.993, 1.000] | same |
| phasefield_staggered_tri3 | solid_solve | head-t4 | 5 | 4 | 6.667 s [6.644 s, 6.730 s] | 1.667 s [1.661 s, 1.682 s] | 32553.1 [32442.1, 32859.7] | 0.996 [0.992, 1.003] | same |
| phasefield_staggered_tri3 | solid_solve | head-t8 | 5 | 4 | 6.713 s [6.673 s, 6.738 s] | 1.678 s [1.668 s, 1.684 s] | 32776.5 [32583.5, 32898.5] | 1.004 [0.993, 1.009] | same |
| phasefield_staggered_tri3 | phase_solve | base-t1 | 5 | 4 | 1.207 s [1.204 s, 1.224 s] | 301.63 ms [301.06 ms, 306.01 ms] | 5891.3 [5880.1, 5976.8] | – | same |
| phasefield_staggered_tri3 | phase_solve | head-t1 | 5 | 4 | 1.208 s [1.204 s, 1.221 s] | 302.00 ms [301.11 ms, 305.31 ms] | 5898.3 [5881.1, 5963.0] | 1.002 [0.995, 1.003] | same |
| phasefield_staggered_tri3 | phase_solve | head-t2 | 5 | 4 | 1.181 s [1.177 s, 1.194 s] | 295.29 ms [294.13 ms, 298.62 ms] | 5767.5 [5744.7, 5832.4] | 0.976 [0.972, 0.982] | same |
| phasefield_staggered_tri3 | phase_solve | head-t4 | 5 | 4 | 1.219 s [1.210 s, 1.222 s] | 304.80 ms [302.53 ms, 305.54 ms] | 5953.1 [5908.9, 5967.5] | 1.008 [0.998, 1.011] | same |
| phasefield_staggered_tri3 | phase_solve | head-t8 | 5 | 4 | 1.199 s [1.193 s, 1.208 s] | 299.84 ms [298.24 ms, 301.89 ms] | 5856.2 [5825.0, 5896.2] | 0.994 [0.986, 0.997] | same |
| phasefield_staggered_tri3 | internal_forces | base-t1 | 5 | 4 | 7.35 ms [7.26 ms, 7.38 ms] | 1.84 ms [1.82 ms, 1.85 ms] | 35.9 [35.5, 36.0] | – | same |
| phasefield_staggered_tri3 | internal_forces | head-t1 | 5 | 4 | 7.79 ms [7.73 ms, 7.91 ms] | 1.95 ms [1.93 ms, 1.98 ms] | 38.0 [37.8, 38.6] | 1.068 [1.049, 1.077] | same |
| phasefield_staggered_tri3 | internal_forces | head-t2 | 5 | 4 | 4.34 ms [4.33 ms, 4.42 ms] | 1.09 ms [1.08 ms, 1.10 ms] | 21.2 [21.1, 21.6] | 0.598 [0.587, 0.601] | same |
| phasefield_staggered_tri3 | internal_forces | head-t4 | 5 | 4 | 3.21 ms [3.16 ms, 3.23 ms] | 802.2 µs [790.5 µs, 807.5 µs] | 15.7 [15.4, 15.8] | 0.437 [0.428, 0.444] | same |
| phasefield_staggered_tri3 | internal_forces | head-t8 | 5 | 4 | 3.30 ms [3.27 ms, 3.33 ms] | 825.8 µs [817.8 µs, 832.5 µs] | 16.1 [16.0, 16.3] | 0.450 [0.445, 0.453] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | 896d0f46b, c9eef15b8 | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 896d0f46b, c9eef15b8 | b4798ad9264a3e5e |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | 896d0f46b, c9eef15b8 | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | 896d0f46b, c9eef15b8 | a77809465b3e7c18 |
