Suite `models`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| structural_dkt18_plate_static | mesh | base | 7 | 1 | 290.0 µs [285.0 µs, 302.0 µs] | 290.0 µs [285.0 µs, 302.0 µs] | 40.3 [39.6, 41.9] | – | same |
| structural_dkt18_plate_static | mesh | head | 7 | 1 | 295.0 µs [291.0 µs, 315.0 µs] | 295.0 µs [291.0 µs, 315.0 µs] | 41.0 [40.4, 43.8] | 1.030 [0.977, 1.090] | same |
| structural_dkt18_plate_static | init | base | 7 | 1 | 69.66 ms [69.31 ms, 69.94 ms] | 69.66 ms [69.31 ms, 69.94 ms] | 9675.1 [9626.8, 9714.6] | – | same |
| structural_dkt18_plate_static | init | head | 7 | 1 | 69.60 ms [69.31 ms, 70.07 ms] | 69.60 ms [69.31 ms, 70.07 ms] | 9667.1 [9626.5, 9731.4] | 0.999 [0.995, 1.006] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 7 | 5 | 266.43 ms [264.26 ms, 271.18 ms] | 53.29 ms [52.85 ms, 54.24 ms] | 7400.9 [7340.6, 7532.9] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | head | 7 | 5 | 267.03 ms [265.20 ms, 273.41 ms] | 53.41 ms [53.04 ms, 54.68 ms] | 7417.4 [7366.6, 7594.7] | 1.003 [0.997, 1.012] | same |
| structural_dkt18_plate_static | solve_step | base | 7 | 1 | 907.47 ms [900.80 ms, 909.40 ms] | 907.47 ms [900.80 ms, 909.40 ms] | 126037.9 [125111.2, 126305.0] | – | same |
| structural_dkt18_plate_static | solve_step | head | 7 | 1 | 900.32 ms [899.25 ms, 912.12 ms] | 900.32 ms [899.25 ms, 912.12 ms] | 125044.6 [124895.8, 126683.3] | 0.998 [0.990, 1.013] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 7a6ad8f42, 988a8c227 | 1cfc7f010c52d6b3 |
