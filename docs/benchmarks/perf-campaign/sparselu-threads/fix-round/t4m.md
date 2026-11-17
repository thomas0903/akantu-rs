Suite `models`, size `m`; variants: `al`, `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| structural_dkt18_plate_static | mesh | al | 7 | 1 | 298.0 µs [290.0 µs, 304.0 µs] | 298.0 µs [290.0 µs, 304.0 µs] | 41.4 [40.3, 42.2] | 0.997 [0.952, 1.031] | same |
| structural_dkt18_plate_static | mesh | base | 7 | 1 | 295.0 µs [290.0 µs, 315.0 µs] | 295.0 µs [290.0 µs, 315.0 µs] | 41.0 [40.3, 43.8] | – | same |
| structural_dkt18_plate_static | mesh | head | 7 | 1 | 296.0 µs [287.0 µs, 324.0 µs] | 296.0 µs [287.0 µs, 324.0 µs] | 41.1 [39.9, 45.0] | 1.003 [0.921, 1.095] | same |
| structural_dkt18_plate_static | init | al | 7 | 1 | 69.88 ms [69.84 ms, 69.96 ms] | 69.88 ms [69.84 ms, 69.96 ms] | 9705.1 [9699.7, 9716.1] | 1.001 [0.990, 1.005] | same |
| structural_dkt18_plate_static | init | base | 7 | 1 | 69.80 ms [69.64 ms, 70.64 ms] | 69.80 ms [69.64 ms, 70.64 ms] | 9694.3 [9671.5, 9811.0] | – | same |
| structural_dkt18_plate_static | init | head | 7 | 1 | 69.77 ms [69.62 ms, 69.93 ms] | 69.77 ms [69.62 ms, 69.93 ms] | 9689.7 [9669.7, 9712.4] | 0.999 [0.988, 1.002] | same |
| structural_dkt18_plate_static | assemble_stiffness | al | 7 | 5 | 266.58 ms [265.77 ms, 270.14 ms] | 53.32 ms [53.15 ms, 54.03 ms] | 7405.1 [7382.4, 7503.9] | 0.999 [0.995, 1.015] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 7 | 5 | 266.79 ms [265.04 ms, 267.51 ms] | 53.36 ms [53.01 ms, 53.50 ms] | 7410.9 [7362.1, 7430.9] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | head | 7 | 5 | 266.40 ms [266.16 ms, 267.83 ms] | 53.28 ms [53.23 ms, 53.57 ms] | 7400.1 [7393.4, 7439.9] | 1.000 [0.997, 1.004] | same |
| structural_dkt18_plate_static | solve_step | al | 7 | 1 | 902.01 ms [898.59 ms, 912.10 ms] | 902.01 ms [898.59 ms, 912.10 ms] | 125279.7 [124803.8, 126679.9] | 0.995 [0.988, 1.000] | same |
| structural_dkt18_plate_static | solve_step | base | 7 | 1 | 907.42 ms [902.85 ms, 915.77 ms] | 907.42 ms [902.85 ms, 915.77 ms] | 126030.4 [125396.3, 127189.6] | – | same |
| structural_dkt18_plate_static | solve_step | head | 7 | 1 | 912.59 ms [910.45 ms, 917.06 ms] | 912.59 ms [910.45 ms, 917.06 ms] | 126748.9 [126451.5, 127369.2] | 1.005 [0.995, 1.014] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 35250f369, 7a6ad8f42, 8c714206b | 1cfc7f010c52d6b3 |
