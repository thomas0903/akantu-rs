Suite `models`, size `m`; variants: `base`, `head`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| structural_dkt18_plate_static | mesh | base | 7 | 1 | 294.0 µs [290.0 µs, 310.0 µs] | 294.0 µs [290.0 µs, 310.0 µs] | 40.8 [40.3, 43.1] | – | same |
| structural_dkt18_plate_static | mesh | head | 7 | 1 | 295.0 µs [288.0 µs, 322.0 µs] | 295.0 µs [288.0 µs, 322.0 µs] | 41.0 [40.0, 44.7] | 1.000 [0.929, 1.099] | same |
| structural_dkt18_plate_static | init | base | 7 | 1 | 69.80 ms [69.67 ms, 70.05 ms] | 69.80 ms [69.67 ms, 70.05 ms] | 9694.0 [9675.8, 9728.7] | – | same |
| structural_dkt18_plate_static | init | head | 7 | 1 | 69.64 ms [69.58 ms, 69.90 ms] | 69.64 ms [69.58 ms, 69.90 ms] | 9671.5 [9663.8, 9708.3] | 0.998 [0.996, 1.000] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 7 | 5 | 266.65 ms [265.34 ms, 268.54 ms] | 53.33 ms [53.07 ms, 53.71 ms] | 7406.9 [7370.4, 7459.4] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | head | 7 | 5 | 266.41 ms [265.53 ms, 267.73 ms] | 53.28 ms [53.11 ms, 53.55 ms] | 7400.2 [7375.8, 7436.9] | 1.000 [0.989, 1.004] | same |
| structural_dkt18_plate_static | solve_step | base | 7 | 1 | 909.69 ms [905.57 ms, 913.22 ms] | 909.69 ms [905.57 ms, 913.22 ms] | 126346.0 [125774.0, 126836.2] | – | same |
| structural_dkt18_plate_static | solve_step | head | 7 | 1 | 914.00 ms [912.90 ms, 921.87 ms] | 914.00 ms [912.90 ms, 921.87 ms] | 126945.0 [126791.7, 128037.5] | 1.008 [1.000, 1.015] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 7a6ad8f42, 8c714206b | 1cfc7f010c52d6b3 |
