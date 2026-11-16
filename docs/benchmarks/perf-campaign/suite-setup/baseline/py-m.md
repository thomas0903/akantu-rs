Suite `python`, size `m`; variants: `python`; reference `python`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| dirichlet_tri3 | mesh | python | 5 | 1 | 64.46 ms [64.16 ms, 74.58 ms] | 64.46 ms [64.16 ms, 74.58 ms] | 805.7 [802.0, 932.2] | same |
| dirichlet_tri3 | init | python | 5 | 1 | 43.29 ms [42.42 ms, 43.60 ms] | 43.29 ms [42.42 ms, 43.60 ms] | 541.2 [530.2, 545.0] | same |
| dirichlet_tri3 | python_functor | python | 5 | 20 | 569.79 ms [561.90 ms, 604.34 ms] | 28.49 ms [28.09 ms, 30.22 ms] | 356.1 [351.2, 377.7] | same |
| dirichlet_tri3 | builtin_functor | python | 5 | 200 | 58.45 ms [58.42 ms, 58.56 ms] | 292.3 µs [292.1 µs, 292.8 µs] | 3.7 [3.7, 3.7] | same |
| explicit_tri3 | mesh | python | 5 | 1 | 65.34 ms [64.06 ms, 100.04 ms] | 65.34 ms [64.06 ms, 100.04 ms] | 816.8 [800.7, 1250.5] | same |
| explicit_tri3 | init | python | 5 | 1 | 43.67 ms [43.58 ms, 94.60 ms] | 43.67 ms [43.58 ms, 94.60 ms] | 545.9 [544.7, 1182.5] | same |
| explicit_tri3 | solve_step | python | 5 | 400 | 1.891 s [1.876 s, 2.220 s] | 4.73 ms [4.69 ms, 5.55 ms] | 59.1 [58.6, 69.4] | same |
| explicit_tri3 | solve_step_tiny | python | 5 | 300000 | 2.875 s [2.833 s, 2.964 s] | 9.6 µs [9.4 µs, 9.9 µs] | 0.1 [0.1, 0.1] | same |
| fields_tri3 | mesh | python | 5 | 1 | 64.67 ms [64.20 ms, 72.57 ms] | 64.67 ms [64.20 ms, 72.57 ms] | 808.4 [802.5, 907.1] | same |
| fields_tri3 | init | python | 5 | 1 | 43.67 ms [43.28 ms, 45.42 ms] | 43.67 ms [43.28 ms, 45.42 ms] | 545.8 [541.0, 567.8] | same |
| fields_tri3 | get_displacement | python | 5 | 20000 | 4.98 ms [4.92 ms, 5.07 ms] | 0.2 µs [0.2 µs, 0.3 µs] | 0.0 [0.0, 0.0] | same |
| fields_tri3 | copy_displacement | python | 5 | 2000 | 30.81 ms [30.14 ms, 30.89 ms] | 15.4 µs [15.1 µs, 15.4 µs] | 0.2 [0.2, 0.2] | same |
| fields_tri3 | python_loop_set | python | 5 | 3 | 64.01 ms [63.47 ms, 64.73 ms] | 21.34 ms [21.16 ms, 21.58 ms] | 266.7 [264.5, 269.7] | same |
| fields_tri3 | numpy_set | python | 5 | 200 | 93.60 ms [93.38 ms, 106.62 ms] | 468.0 µs [466.9 µs, 533.1 µs] | 5.8 [5.8, 6.7] | same |
| fields_tri3 | get_stress | python | 5 | 20000 | 9.07 ms [9.03 ms, 9.34 ms] | 0.5 µs [0.5 µs, 0.5 µs] | 0.0 [0.0, 0.0] | same |
| import | python_startup | python | 5 | 10 | 88.66 ms [88.59 ms, 140.14 ms] | 8.87 ms [8.86 ms, 14.01 ms] | – | same |
| import | import_akantu | python | 5 | 10 | 1.671 s [1.668 s, 1.848 s] | 167.12 ms [166.79 ms, 184.85 ms] | – | same |
| import | import_numpy | python | 5 | 10 | 622.49 ms [621.29 ms, 670.69 ms] | 62.25 ms [62.13 ms, 67.07 ms] | – | same |
| material_tri3 | mesh | python | 5 | 1 | 64.61 ms [64.16 ms, 69.35 ms] | 64.61 ms [64.16 ms, 69.35 ms] | 807.7 [802.0, 866.9] | same |
| material_tri3 | init | python | 5 | 1 | 118.09 ms [116.83 ms, 121.93 ms] | 118.09 ms [116.83 ms, 121.93 ms] | 1476.1 [1460.4, 1524.1] | same |
| material_tri3 | solve_step_python_law | python | 5 | 400 | 4.236 s [4.108 s, 4.769 s] | 10.59 ms [10.27 ms, 11.92 ms] | 132.4 [128.4, 149.0] | same |
| material_tri3 | solve_step_builtin_law | python | 5 | 400 | 1.913 s [1.875 s, 2.040 s] | 4.78 ms [4.69 ms, 5.10 ms] | 59.8 [58.6, 63.8] | same |
| neumann_tri3 | mesh | python | 5 | 1 | 67.60 ms [64.60 ms, 74.51 ms] | 67.60 ms [64.60 ms, 74.51 ms] | 845.0 [807.5, 931.4] | same |
| neumann_tri3 | init | python | 5 | 1 | 43.23 ms [43.06 ms, 45.12 ms] | 43.23 ms [43.06 ms, 45.12 ms] | 540.4 [538.3, 564.0] | same |
| neumann_tri3 | python_functor | python | 5 | 200 | 250.31 ms [248.48 ms, 260.29 ms] | 1.25 ms [1.24 ms, 1.30 ms] | 15.6 [15.5, 16.3] | same |
| neumann_tri3 | builtin_functor | python | 5 | 2000 | 870.98 ms [868.81 ms, 893.00 ms] | 435.5 µs [434.4 µs, 446.5 µs] | 5.4 [5.4, 5.6] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| dirichlet_tri3 | 80000 | 40401 | 80802 | 1 | a1b454b0c | bd9af3b6c4a93e75 |
| explicit_tri3 | 80000 | 40401 | 80802 | 1 | a1b454b0c | 7c3f18493f05f971 |
| fields_tri3 | 80000 | 40401 | 80802 | 1 | a1b454b0c | dbc85a48f7874956 |
| import | 0 | 0 | 0 | 1 | a1b454b0c | 0000000000000000 |
| material_tri3 | 80000 | 40401 | 80802 | 1 | a1b454b0c | 7c3f18493f05f971 |
| neumann_tri3 | 80000 | 40401 | 80802 | 1 | a1b454b0c | 744d6ee2d8065795 |
