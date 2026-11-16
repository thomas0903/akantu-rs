Suite `solid`, size `m`; variants: `base`, `base_env`, `head`, `head_env`; reference `base`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to base | digest |
|---|---|---|---|---|---|---|---|---|---|
| implicit_dynamic | mass_consistent | base | 5 | 1 | 56.42 ms [51.38 ms, 65.02 ms] | 56.42 ms [51.38 ms, 65.02 ms] | 3426.9 [3120.9, 3949.3] | – | same |
| implicit_dynamic | mass_consistent | base_env | 5 | 1 | 61.86 ms [60.89 ms, 63.95 ms] | 61.86 ms [60.89 ms, 63.95 ms] | 3757.5 [3698.5, 3884.1] | 1.133 [0.936, 1.215] | same |
| implicit_dynamic | mass_consistent | head | 5 | 1 | 51.64 ms [48.95 ms, 60.91 ms] | 51.64 ms [48.95 ms, 60.91 ms] | 3136.6 [2973.4, 3699.8] | 0.868 [0.791, 1.186] | same |
| implicit_dynamic | mass_consistent | head_env | 5 | 1 | 57.95 ms [56.43 ms, 59.16 ms] | 57.95 ms [56.43 ms, 59.16 ms] | 3520.0 [3427.4, 3593.2] | 1.049 [0.870, 1.128] | same |
| setup_hex8 | mass_consistent | base | 5 | 3 | 318.18 ms [307.50 ms, 320.50 ms] | 106.06 ms [102.50 ms, 106.83 ms] | 7672.2 [7414.7, 7728.2] | – | same |
| setup_hex8 | mass_consistent | base_env | 5 | 3 | 264.71 ms [258.66 ms, 270.75 ms] | 88.24 ms [86.22 ms, 90.25 ms] | 6382.9 [6237.0, 6528.6] | 0.850 [0.809, 0.856] | same |
| setup_hex8 | mass_consistent | head | 5 | 3 | 247.26 ms [238.93 ms, 249.66 ms] | 82.42 ms [79.64 ms, 83.22 ms] | 5962.1 [5761.2, 6020.0] | 0.779 [0.747, 0.805] | same |
| setup_hex8 | mass_consistent | head_env | 5 | 3 | 247.29 ms [244.13 ms, 249.54 ms] | 82.43 ms [81.38 ms, 83.18 ms] | 5962.7 [5886.6, 6017.2] | 0.780 [0.762, 0.797] | same |
| setup_hex8 | stiffness_assembly | base | 5 | 3 | 1.471 s [1.468 s, 1.479 s] | 490.17 ms [489.30 ms, 493.09 ms] | 35457.9 [35394.6, 35669.1] | – | same |
| setup_hex8 | stiffness_assembly | base_env | 5 | 3 | 1.417 s [1.410 s, 1.422 s] | 472.25 ms [469.98 ms, 474.03 ms] | 34161.7 [33997.6, 34290.1] | 0.964 [0.953, 0.967] | same |
| setup_hex8 | stiffness_assembly | head | 5 | 3 | 574.22 ms [569.39 ms, 578.04 ms] | 191.41 ms [189.80 ms, 192.68 ms] | 13846.0 [13729.6, 13938.1] | 0.390 [0.387, 0.393] | same |
| setup_hex8 | stiffness_assembly | head_env | 5 | 3 | 576.55 ms [573.12 ms, 581.74 ms] | 192.18 ms [191.04 ms, 193.91 ms] | 13902.1 [13819.4, 14027.4] | 0.392 [0.387, 0.396] | same |
| setup_tet10 | mass_consistent | base | 5 | 3 | 244.17 ms [239.02 ms, 251.04 ms] | 81.39 ms [79.67 ms, 83.68 ms] | 7850.2 [7684.6, 8071.1] | – | same |
| setup_tet10 | mass_consistent | base_env | 5 | 3 | 188.54 ms [185.93 ms, 191.40 ms] | 62.85 ms [61.98 ms, 63.80 ms] | 6061.6 [5977.7, 6153.4] | 0.772 [0.748, 0.789] | same |
| setup_tet10 | mass_consistent | head | 5 | 3 | 186.31 ms [179.57 ms, 194.78 ms] | 62.10 ms [59.86 ms, 64.93 ms] | 5990.0 [5773.1, 6262.3] | 0.776 [0.722, 0.793] | same |
| setup_tet10 | mass_consistent | head_env | 5 | 3 | 180.63 ms [176.83 ms, 181.45 ms] | 60.21 ms [58.94 ms, 60.48 ms] | 5807.2 [5685.2, 5833.5] | 0.743 [0.704, 0.756] | same |
| setup_tet10 | stiffness_assembly | base | 5 | 3 | 915.13 ms [906.78 ms, 916.29 ms] | 305.04 ms [302.26 ms, 305.43 ms] | 29421.7 [29153.1, 29458.8] | – | same |
| setup_tet10 | stiffness_assembly | base_env | 5 | 3 | 854.03 ms [846.72 ms, 860.04 ms] | 284.68 ms [282.24 ms, 286.68 ms] | 27457.2 [27222.3, 27650.4] | 0.934 [0.926, 0.940] | same |
| setup_tet10 | stiffness_assembly | head | 5 | 3 | 349.45 ms [345.54 ms, 351.89 ms] | 116.48 ms [115.18 ms, 117.30 ms] | 11235.0 [11109.2, 11313.4] | 0.383 [0.378, 0.388] | same |
| setup_tet10 | stiffness_assembly | head_env | 5 | 3 | 350.19 ms [343.39 ms, 354.33 ms] | 116.73 ms [114.46 ms, 118.11 ms] | 11258.7 [11040.1, 11391.6] | 0.382 [0.375, 0.389] | same |
| setup_tet4 | mass_consistent | base | 5 | 3 | 613.57 ms [608.73 ms, 621.22 ms] | 204.52 ms [202.91 ms, 207.07 ms] | 1262.5 [1252.5, 1278.2] | – | same |
| setup_tet4 | mass_consistent | base_env | 5 | 3 | 456.02 ms [453.23 ms, 474.36 ms] | 152.01 ms [151.08 ms, 158.12 ms] | 938.3 [932.6, 976.1] | 0.741 [0.732, 0.774] | same |
| setup_tet4 | mass_consistent | head | 5 | 3 | 372.77 ms [369.91 ms, 376.29 ms] | 124.26 ms [123.30 ms, 125.43 ms] | 767.0 [761.1, 774.2] | 0.608 [0.598, 0.618] | same |
| setup_tet4 | mass_consistent | head_env | 5 | 3 | 372.22 ms [369.99 ms, 373.19 ms] | 124.07 ms [123.33 ms, 124.40 ms] | 765.9 [761.3, 767.9] | 0.604 [0.600, 0.613] | same |
| setup_tet4 | stiffness_assembly | base | 5 | 3 | 1.075 s [1.061 s, 1.083 s] | 358.17 ms [353.62 ms, 361.16 ms] | 2210.9 [2182.8, 2229.4] | – | same |
| setup_tet4 | stiffness_assembly | base_env | 5 | 3 | 706.76 ms [702.42 ms, 711.82 ms] | 235.59 ms [234.14 ms, 237.27 ms] | 1454.2 [1445.3, 1464.6] | 0.658 [0.648, 0.666] | same |
| setup_tet4 | stiffness_assembly | head | 5 | 3 | 517.33 ms [514.02 ms, 524.85 ms] | 172.44 ms [171.34 ms, 174.95 ms] | 1064.5 [1057.6, 1079.9] | 0.481 [0.474, 0.491] | same |
| setup_tet4 | stiffness_assembly | head_env | 5 | 3 | 518.67 ms [513.99 ms, 524.41 ms] | 172.89 ms [171.33 ms, 174.80 ms] | 1067.2 [1057.6, 1079.0] | 0.484 [0.474, 0.489] | same |
| static_linear_eigen | stiffness_assembly | base | 5 | 3 | 78.13 ms [75.88 ms, 79.19 ms] | 26.04 ms [25.29 ms, 26.40 ms] | 1581.8 [1536.2, 1603.4] | – | same |
| static_linear_eigen | stiffness_assembly | base_env | 5 | 3 | 45.04 ms [43.86 ms, 48.62 ms] | 15.01 ms [14.62 ms, 16.21 ms] | 912.0 [888.0, 984.4] | 0.594 [0.561, 0.622] | same |
| static_linear_eigen | stiffness_assembly | head | 5 | 3 | 40.75 ms [39.15 ms, 42.14 ms] | 13.58 ms [13.05 ms, 14.05 ms] | 825.0 [792.6, 853.1] | 0.524 [0.515, 0.539] | same |
| static_linear_eigen | stiffness_assembly | head_env | 5 | 3 | 41.98 ms [39.48 ms, 42.48 ms] | 13.99 ms [13.16 ms, 14.16 ms] | 849.9 [799.3, 860.0] | 0.537 [0.513, 0.542] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| implicit_dynamic | 16464 | 3375 | 10125 | 1 | 105ea0eb1, 72337cb4e | 13939e4cdc8bb8b8 |
| setup_hex8 | 13824 | 15625 | 46875 | 1 | 105ea0eb1, 72337cb4e | 699ba53ab1309e89 |
| setup_tet10 | 10368 | 15625 | 46875 | 1 | 105ea0eb1, 72337cb4e | 2eaebacdfc237628 |
| setup_tet4 | 162000 | 29791 | 89373 | 1 | 105ea0eb1, 72337cb4e | 562b4d2b6ad29d67 |
| static_linear_eigen | 16464 | 3375 | 10125 | 1 | 105ea0eb1, 72337cb4e | d5869f57a777d9a5 |
