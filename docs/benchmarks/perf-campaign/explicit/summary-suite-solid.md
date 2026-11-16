Suite `solid`, size `m`; variants: `after`, `base`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | solve_step | after | 5 | 100 | 1.004 s [999.80 ms, 1.013 s] | 10.04 ms [10.00 ms, 10.13 ms] | 121.1 [120.5, 122.1] | 0.780 [0.779, 0.786] | same |
| cohesive_extrinsic_check | solve_step | base | 5 | 100 | 1.287 s [1.278 s, 1.299 s] | 12.87 ms [12.78 ms, 12.99 ms] | 155.1 [154.1, 156.7] | – | same |
| explicit_hex8_fd | solve_step | after | 5 | 30 | 2.137 s [2.122 s, 2.160 s] | 71.24 ms [70.72 ms, 72.00 ms] | 644.2 [639.4, 651.1] | 0.574 [0.566, 0.579] | same |
| explicit_hex8_fd | solve_step | base | 5 | 30 | 3.723 s [3.713 s, 3.776 s] | 124.08 ms [123.76 ms, 125.87 ms] | 1122.0 [1119.1, 1138.1] | – | same |
| explicit_hex8_ss | solve_step | after | 5 | 50 | 2.469 s [2.464 s, 2.494 s] | 49.39 ms [49.27 ms, 49.89 ms] | 446.6 [445.5, 451.1] | 0.769 [0.767, 0.777] | same |
| explicit_hex8_ss | solve_step | base | 5 | 50 | 3.211 s [3.210 s, 3.229 s] | 64.23 ms [64.20 ms, 64.58 ms] | 580.8 [580.5, 583.9] | – | same |
| explicit_multi_material | solve_step | after | 5 | 120 | 4.843 s [4.805 s, 5.200 s] | 40.36 ms [40.04 ms, 43.33 ms] | 382.7 [379.7, 410.9] | 0.938 [0.922, 0.987] | same |
| explicit_multi_material | solve_step | base | 5 | 120 | 5.194 s [5.120 s, 5.266 s] | 43.29 ms [42.67 ms, 43.89 ms] | 410.5 [404.6, 416.2] | – | same |
| explicit_plastic | solve_step | after | 5 | 120 | 5.158 s [5.142 s, 5.171 s] | 42.98 ms [42.85 ms, 43.09 ms] | 407.6 [406.3, 408.6] | 0.964 [0.962, 0.971] | same |
| explicit_plastic | solve_step | base | 5 | 120 | 5.331 s [5.328 s, 5.370 s] | 44.43 ms [44.40 ms, 44.75 ms] | 421.3 [421.0, 424.3] | – | same |
| explicit_tet10_fd | solve_step | after | 5 | 40 | 1.823 s [1.819 s, 1.832 s] | 45.56 ms [45.47 ms, 45.79 ms] | 432.1 [431.2, 434.2] | 0.581 [0.579, 0.586] | same |
| explicit_tet10_fd | solve_step | base | 5 | 40 | 3.130 s [3.127 s, 3.148 s] | 78.26 ms [78.17 ms, 78.70 ms] | 742.1 [741.3, 746.3] | – | same |
| explicit_tet10_ss | solve_step | after | 5 | 50 | 1.616 s [1.612 s, 1.628 s] | 32.32 ms [32.23 ms, 32.57 ms] | 306.5 [305.7, 308.8] | 0.742 [0.740, 0.748] | same |
| explicit_tet10_ss | solve_step | base | 5 | 50 | 2.178 s [2.175 s, 2.193 s] | 43.55 ms [43.49 ms, 43.85 ms] | 413.0 [412.4, 415.8] | – | same |
| explicit_tet4_fd | solve_step | after | 5 | 150 | 2.231 s [2.224 s, 2.244 s] | 14.87 ms [14.83 ms, 14.96 ms] | 91.8 [91.5, 92.3] | 0.722 [0.717, 0.724] | same |
| explicit_tet4_fd | solve_step | base | 5 | 150 | 3.089 s [3.083 s, 3.130 s] | 20.59 ms [20.55 ms, 20.87 ms] | 127.1 [126.9, 128.8] | – | same |
| explicit_tet4_ss | solve_step | after | 5 | 200 | 2.248 s [2.223 s, 2.286 s] | 11.24 ms [11.11 ms, 11.43 ms] | 69.4 [68.6, 70.5] | 0.800 [0.788, 0.817] | same |
| explicit_tet4_ss | solve_step | base | 5 | 200 | 2.818 s [2.799 s, 2.825 s] | 14.09 ms [13.99 ms, 14.12 ms] | 87.0 [86.4, 87.2] | – | same |
| explicit_thermal | solve_step | after | 5 | 300 | 2.109 s [2.105 s, 2.138 s] | 7.03 ms [7.02 ms, 7.13 ms] | 66.7 [66.5, 67.6] | 0.778 [0.770, 0.791] | same |
| explicit_thermal | solve_step | base | 5 | 300 | 2.709 s [2.700 s, 2.741 s] | 9.03 ms [9.00 ms, 9.14 ms] | 85.6 [85.4, 86.6] | – | same |
| explicit_tri3_fd | solve_step | after | 5 | 200 | 2.377 s [2.368 s, 2.381 s] | 11.88 ms [11.84 ms, 11.91 ms] | 66.0 [65.8, 66.2] | 0.953 [0.949, 0.956] | same |
| explicit_tri3_fd | solve_step | base | 5 | 200 | 2.492 s [2.485 s, 2.498 s] | 12.46 ms [12.43 ms, 12.49 ms] | 69.2 [69.0, 69.4] | – | same |
| explicit_tri3_ss | solve_step | after | 5 | 300 | 2.559 s [2.544 s, 2.567 s] | 8.53 ms [8.48 ms, 8.56 ms] | 47.4 [47.1, 47.5] | 0.784 [0.776, 0.786] | same |
| explicit_tri3_ss | solve_step | base | 5 | 300 | 3.265 s [3.256 s, 3.278 s] | 10.88 ms [10.85 ms, 10.93 ms] | 60.5 [60.3, 60.7] | – | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | 82944 | 15625 | 46875 | 1 | 1fe1e551a, cac5d2cda | f11c449c593d38ae |
| explicit_hex8_fd | 110592 | 117649 | 352947 | 1 | 1fe1e551a, cac5d2cda | 78b072c2f464d8f9 |
| explicit_hex8_ss | 110592 | 117649 | 352947 | 1 | 1fe1e551a, cac5d2cda | 37ce0eeb914dd994 |
| explicit_multi_material | 105456 | 19683 | 59049 | 1 | 1fe1e551a, cac5d2cda | ce8154043761dbc7 |
| explicit_plastic | 105456 | 19683 | 59049 | 1 | 1fe1e551a, cac5d2cda | 5c844dbf1dc6d9c8 |
| explicit_tet10_fd | 105456 | 148877 | 446631 | 1 | 1fe1e551a, cac5d2cda | c9f013e1a9bbd7a1 |
| explicit_tet10_ss | 105456 | 148877 | 446631 | 1 | 1fe1e551a, cac5d2cda | a12864e94a9122c8 |
| explicit_tet4_fd | 162000 | 29791 | 89373 | 1 | 1fe1e551a, cac5d2cda | 9d6cdbdad424d346 |
| explicit_tet4_ss | 162000 | 29791 | 89373 | 1 | 1fe1e551a, cac5d2cda | fecb3fb4b46c8b8c |
| explicit_thermal | 105456 | 19683 | 59049 | 1 | 1fe1e551a, cac5d2cda | 27979d9fbcdea83d |
| explicit_tri3_fd | 180000 | 90601 | 181202 | 1 | 1fe1e551a, cac5d2cda | 284a1d26bc19eda4 |
| explicit_tri3_ss | 180000 | 90601 | 181202 | 1 | 1fe1e551a, cac5d2cda | b6b902f5ca71049e |
