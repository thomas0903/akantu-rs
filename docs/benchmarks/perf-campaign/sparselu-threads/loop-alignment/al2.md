Suite `models`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| heat_static_tri6 | mesh | base | 7 | 1 | 40.42 ms [40.10 ms, 40.97 ms] | 40.42 ms [40.10 ms, 40.97 ms] | 1670.3 [1656.9, 1692.8] | – | same |
| heat_static_tri6 | mesh | head | 7 | 1 | 39.76 ms [39.69 ms, 40.59 ms] | 39.76 ms [39.69 ms, 40.59 ms] | 1643.1 [1640.0, 1677.1] | 0.988 [0.971, 1.008] | same |
| heat_static_tri6 | init | base | 7 | 1 | 5.50 ms [5.47 ms, 5.56 ms] | 5.50 ms [5.47 ms, 5.56 ms] | 227.3 [226.1, 229.8] | – | same |
| heat_static_tri6 | init | head | 7 | 1 | 5.49 ms [5.41 ms, 5.54 ms] | 5.49 ms [5.41 ms, 5.54 ms] | 226.9 [223.7, 228.8] | 0.997 [0.987, 1.012] | same |
| heat_static_tri6 | assemble_conductivity | base | 7 | 5 | 38.37 ms [38.27 ms, 39.47 ms] | 7.67 ms [7.65 ms, 7.89 ms] | 317.1 [316.3, 326.2] | – | same |
| heat_static_tri6 | assemble_conductivity | head | 7 | 5 | 38.34 ms [38.24 ms, 38.49 ms] | 7.67 ms [7.65 ms, 7.70 ms] | 316.9 [316.0, 318.1] | 0.999 [0.973, 1.004] | same |
| heat_static_tri6 | solve_step | base | 7 | 1 | 1.786 s [1.766 s, 1.814 s] | 1.786 s [1.766 s, 1.814 s] | 73814.6 [72988.7, 74962.0] | – | same |
| heat_static_tri6 | solve_step | head | 7 | 1 | 1.803 s [1.798 s, 1.832 s] | 1.803 s [1.798 s, 1.832 s] | 74499.0 [74290.5, 75720.5] | 1.017 [0.994, 1.025] | same |
| structural_beam3_frame_static | mesh | base | 7 | 1 | 351.0 µs [337.0 µs, 402.0 µs] | 351.0 µs [337.0 µs, 402.0 µs] | 57.7 [55.4, 66.1] | – | same |
| structural_beam3_frame_static | mesh | head | 7 | 1 | 350.0 µs [331.0 µs, 364.0 µs] | 350.0 µs [331.0 µs, 364.0 µs] | 57.5 [54.4, 59.8] | 0.966 [0.886, 1.044] | same |
| structural_beam3_frame_static | init | base | 7 | 1 | 30.02 ms [29.90 ms, 30.19 ms] | 30.02 ms [29.90 ms, 30.19 ms] | 4933.8 [4913.7, 4962.2] | – | same |
| structural_beam3_frame_static | init | head | 7 | 1 | 29.92 ms [29.76 ms, 30.46 ms] | 29.92 ms [29.76 ms, 30.46 ms] | 4918.1 [4891.4, 5006.1] | 0.997 [0.990, 1.014] | same |
| structural_beam3_frame_static | assemble_stiffness | base | 7 | 5 | 29.81 ms [29.76 ms, 30.13 ms] | 5.96 ms [5.95 ms, 6.03 ms] | 979.8 [978.2, 990.4] | – | same |
| structural_beam3_frame_static | assemble_stiffness | head | 7 | 5 | 30.00 ms [29.85 ms, 30.06 ms] | 6.00 ms [5.97 ms, 6.01 ms] | 986.2 [981.2, 988.2] | 1.004 [0.998, 1.009] | same |
| structural_beam3_frame_static | solve_step | base | 7 | 1 | 1.355 s [1.346 s, 1.368 s] | 1.355 s [1.346 s, 1.368 s] | 222775.1 [221154.7, 224840.1] | – | same |
| structural_beam3_frame_static | solve_step | head | 7 | 1 | 1.368 s [1.361 s, 1.380 s] | 1.368 s [1.361 s, 1.380 s] | 224921.1 [223754.4, 226848.3] | 1.009 [1.003, 1.023] | same |
| structural_dkt18_plate_static | mesh | base | 7 | 1 | 294.0 µs [291.0 µs, 307.0 µs] | 294.0 µs [291.0 µs, 307.0 µs] | 40.8 [40.4, 42.6] | – | same |
| structural_dkt18_plate_static | mesh | head | 7 | 1 | 296.0 µs [289.0 µs, 302.0 µs] | 296.0 µs [289.0 µs, 302.0 µs] | 41.1 [40.1, 41.9] | 1.000 [0.970, 1.031] | same |
| structural_dkt18_plate_static | init | base | 7 | 1 | 69.66 ms [69.61 ms, 70.67 ms] | 69.66 ms [69.61 ms, 70.67 ms] | 9674.4 [9668.6, 9814.7] | – | same |
| structural_dkt18_plate_static | init | head | 7 | 1 | 69.52 ms [69.46 ms, 70.64 ms] | 69.52 ms [69.46 ms, 70.64 ms] | 9655.0 [9646.8, 9811.7] | 0.998 [0.984, 1.011] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 7 | 5 | 267.02 ms [265.10 ms, 268.86 ms] | 53.40 ms [53.02 ms, 53.77 ms] | 7417.2 [7363.8, 7468.2] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | head | 7 | 5 | 266.99 ms [265.92 ms, 268.43 ms] | 53.40 ms [53.18 ms, 53.69 ms] | 7416.4 [7386.7, 7456.4] | 1.001 [0.990, 1.007] | same |
| structural_dkt18_plate_static | solve_step | base | 7 | 1 | 908.86 ms [904.14 ms, 918.57 ms] | 908.86 ms [904.14 ms, 918.57 ms] | 126230.7 [125575.6, 127579.4] | – | same |
| structural_dkt18_plate_static | solve_step | head | 7 | 1 | 925.59 ms [919.04 ms, 929.15 ms] | 925.59 ms [919.04 ms, 929.15 ms] | 128554.2 [127644.9, 129048.8] | 1.018 [1.008, 1.028] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | 4df3bb92e, 7a6ad8f42 | 463949e032392419 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 4df3bb92e, 7a6ad8f42 | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 4df3bb92e, 7a6ad8f42 | 1cfc7f010c52d6b3 |
