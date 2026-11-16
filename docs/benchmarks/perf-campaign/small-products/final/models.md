Suite `models`, size `m`; variants: `before`, `final`; reference `before`. Times: median [min, max] over the repetitions; ratios: median [min, max] of the per-repetition ratios to the reference.

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | ratio to before | digest |
|---|---|---|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | mesh | before | 5 | 1 | 69.73 ms [66.99 ms, 89.95 ms] | 69.73 ms [66.99 ms, 89.95 ms] | 1089.6 [1046.7, 1405.5] | – | same |
| contact_cohesive_quad4 | mesh | final | 5 | 1 | 79.62 ms [70.39 ms, 88.67 ms] | 79.62 ms [70.39 ms, 88.67 ms] | 1244.1 [1099.9, 1385.4] | 1.030 [0.971, 1.165] | same |
| contact_cohesive_quad4 | init | before | 5 | 1 | 166.06 ms [144.39 ms, 175.78 ms] | 166.06 ms [144.39 ms, 175.78 ms] | 2594.7 [2256.1, 2746.6] | – | same |
| contact_cohesive_quad4 | init | final | 5 | 1 | 150.11 ms [129.88 ms, 157.84 ms] | 150.11 ms [129.88 ms, 157.84 ms] | 2345.4 [2029.3, 2466.2] | 0.886 [0.845, 1.040] | same |
| contact_cohesive_quad4 | solve_step | before | 5 | 300 | 7.352 s [7.272 s, 8.107 s] | 24.51 ms [24.24 ms, 27.02 ms] | 382.9 [378.8, 422.2] | – | same |
| contact_cohesive_quad4 | solve_step | final | 5 | 300 | 5.957 s [5.745 s, 6.500 s] | 19.86 ms [19.15 ms, 21.67 ms] | 310.2 [299.2, 338.6] | 0.790 [0.787, 0.841] | same |
| contact_search_quad4 | mesh | before | 5 | 1 | 322.07 ms [292.46 ms, 398.07 ms] | 322.07 ms [292.46 ms, 398.07 ms] | 2300.5 [2089.0, 2843.4] | – | same |
| contact_search_quad4 | mesh | final | 5 | 1 | 314.26 ms [287.25 ms, 369.67 ms] | 314.26 ms [287.25 ms, 369.67 ms] | 2244.7 [2051.8, 2640.5] | 0.982 [0.929, 1.020] | same |
| contact_search_quad4 | init | before | 5 | 1 | 230.35 ms [217.12 ms, 237.10 ms] | 230.35 ms [217.12 ms, 237.10 ms] | 1645.3 [1550.9, 1693.5] | – | same |
| contact_search_quad4 | init | final | 5 | 1 | 154.14 ms [135.88 ms, 157.12 ms] | 154.14 ms [135.88 ms, 157.12 ms] | 1101.0 [970.6, 1122.3] | 0.651 [0.590, 0.723] | same |
| contact_search_quad4 | search | before | 5 | 60 | 4.478 s [4.371 s, 4.621 s] | 74.63 ms [72.84 ms, 77.01 ms] | 533.1 [520.3, 550.1] | – | same |
| contact_search_quad4 | search | final | 5 | 60 | 4.151 s [3.991 s, 4.271 s] | 69.18 ms [66.51 ms, 71.18 ms] | 494.2 [475.1, 508.4] | 0.916 [0.898, 0.959] | same |
| contact_search_tet4 | mesh | before | 5 | 1 | 679.54 ms [618.41 ms, 775.79 ms] | 679.54 ms [618.41 ms, 775.79 ms] | 1415.7 [1288.4, 1616.2] | – | same |
| contact_search_tet4 | mesh | final | 5 | 1 | 673.54 ms [659.29 ms, 716.64 ms] | 673.54 ms [659.29 ms, 716.64 ms] | 1403.2 [1373.5, 1493.0] | 0.991 [0.903, 1.099] | same |
| contact_search_tet4 | init | before | 5 | 1 | 341.89 ms [305.06 ms, 360.47 ms] | 341.89 ms [305.06 ms, 360.47 ms] | 712.3 [635.5, 751.0] | – | same |
| contact_search_tet4 | init | final | 5 | 1 | 192.54 ms [184.41 ms, 202.33 ms] | 192.54 ms [184.41 ms, 202.33 ms] | 401.1 [384.2, 421.5] | 0.563 [0.551, 0.625] | same |
| contact_search_tet4 | search | before | 5 | 30 | 4.217 s [4.164 s, 4.339 s] | 140.58 ms [138.80 ms, 144.64 ms] | 292.9 [289.2, 301.3] | – | same |
| contact_search_tet4 | search | final | 5 | 30 | 3.343 s [3.274 s, 3.477 s] | 111.44 ms [109.12 ms, 115.92 ms] | 232.2 [227.3, 241.5] | 0.801 [0.776, 0.813] | same |
| embedded_rebar_tri3 | mesh | before | 5 | 1 | 19.78 ms [18.98 ms, 21.18 ms] | 19.78 ms [18.98 ms, 21.18 ms] | 785.0 [753.2, 840.5] | – | same |
| embedded_rebar_tri3 | mesh | final | 5 | 1 | 19.08 ms [18.88 ms, 23.09 ms] | 19.08 ms [18.88 ms, 23.09 ms] | 756.9 [749.1, 916.2] | 0.960 [0.901, 1.216] | same |
| embedded_rebar_tri3 | init | before | 5 | 1 | 1.339 s [1.297 s, 1.344 s] | 1.339 s [1.297 s, 1.344 s] | 53131.3 [51488.0, 53317.7] | – | same |
| embedded_rebar_tri3 | init | final | 5 | 1 | 1.321 s [1.289 s, 1.414 s] | 1.321 s [1.289 s, 1.414 s] | 52403.4 [51165.3, 56125.3] | 0.983 [0.961, 1.090] | same |
| embedded_rebar_tri3 | intersect | before | 5 | 5 | 6.806 s [6.574 s, 9.864 s] | 1.361 s [1.315 s, 1.973 s] | 54015.2 [52178.1, 78286.1] | – | same |
| embedded_rebar_tri3 | intersect | final | 5 | 5 | 6.474 s [6.384 s, 6.728 s] | 1.295 s [1.277 s, 1.346 s] | 51384.8 [50664.4, 53395.0] | 0.965 [0.656, 0.984] | same |
| embedded_rebar_tri3 | solve_step | before | 5 | 1 | 404.59 ms [357.47 ms, 476.41 ms] | 404.59 ms [357.47 ms, 476.41 ms] | 16055.0 [14185.3, 18905.3] | – | same |
| embedded_rebar_tri3 | solve_step | final | 5 | 1 | 433.62 ms [376.87 ms, 471.13 ms] | 433.62 ms [376.87 ms, 471.13 ms] | 17207.1 [14955.1, 18695.6] | 1.108 [0.791, 1.221] | same |
| embedded_rebar_tri3 | assemble_internal_forces | before | 5 | 20 | 42.04 ms [41.65 ms, 44.81 ms] | 2.10 ms [2.08 ms, 2.24 ms] | 83.4 [82.6, 88.9] | – | same |
| embedded_rebar_tri3 | assemble_internal_forces | final | 5 | 20 | 39.71 ms [38.49 ms, 47.64 ms] | 1.99 ms [1.92 ms, 2.38 ms] | 78.8 [76.4, 94.5] | 0.953 [0.861, 1.133] | same |
| embedded_rebar_tri3 | assemble_stiffness | before | 5 | 5 | 204.22 ms [159.21 ms, 230.06 ms] | 40.84 ms [31.84 ms, 46.01 ms] | 1620.8 [1263.6, 1825.9] | – | same |
| embedded_rebar_tri3 | assemble_stiffness | final | 5 | 5 | 198.45 ms [159.19 ms, 220.44 ms] | 39.69 ms [31.84 ms, 44.09 ms] | 1575.0 [1263.4, 1749.5] | 1.011 [0.780, 1.199] | same |
| heat_explicit_quad4 | mesh | before | 5 | 1 | 191.72 ms [168.68 ms, 203.31 ms] | 191.72 ms [168.68 ms, 203.31 ms] | 1198.3 [1054.2, 1270.7] | – | same |
| heat_explicit_quad4 | mesh | final | 5 | 1 | 186.37 ms [170.98 ms, 197.26 ms] | 186.37 ms [170.98 ms, 197.26 ms] | 1164.8 [1068.6, 1232.9] | 0.972 [0.903, 1.034] | same |
| heat_explicit_quad4 | init | before | 5 | 1 | 23.02 ms [21.66 ms, 24.82 ms] | 23.02 ms [21.66 ms, 24.82 ms] | 143.9 [135.4, 155.1] | – | same |
| heat_explicit_quad4 | init | final | 5 | 1 | 22.90 ms [20.95 ms, 25.12 ms] | 22.90 ms [20.95 ms, 25.12 ms] | 143.1 [130.9, 157.0] | 1.057 [0.844, 1.074] | same |
| heat_explicit_quad4 | stable_time_step | before | 5 | 1 | 2.72 ms [2.64 ms, 2.81 ms] | 2.72 ms [2.64 ms, 2.81 ms] | 17.0 [16.5, 17.5] | – | same |
| heat_explicit_quad4 | stable_time_step | final | 5 | 1 | 2.62 ms [2.60 ms, 2.80 ms] | 2.62 ms [2.60 ms, 2.80 ms] | 16.4 [16.3, 17.5] | 0.985 [0.927, 1.045] | same |
| heat_explicit_quad4 | solve_step | before | 5 | 120 | 3.645 s [3.464 s, 3.812 s] | 30.37 ms [28.86 ms, 31.77 ms] | 189.8 [180.4, 198.5] | – | same |
| heat_explicit_quad4 | solve_step | final | 5 | 120 | 2.713 s [2.691 s, 2.857 s] | 22.61 ms [22.42 ms, 23.81 ms] | 141.3 [140.1, 148.8] | 0.756 [0.715, 0.777] | same |
| heat_explicit_tet4 | mesh | before | 5 | 1 | 252.43 ms [240.25 ms, 294.20 ms] | 252.43 ms [240.25 ms, 294.20 ms] | 901.8 [858.2, 1050.9] | – | same |
| heat_explicit_tet4 | mesh | final | 5 | 1 | 276.09 ms [261.09 ms, 293.19 ms] | 276.09 ms [261.09 ms, 293.19 ms] | 986.2 [932.7, 1047.3] | 1.087 [0.947, 1.189] | same |
| heat_explicit_tet4 | init | before | 5 | 1 | 12.33 ms [11.90 ms, 13.95 ms] | 12.33 ms [11.90 ms, 13.95 ms] | 44.1 [42.5, 49.8] | – | same |
| heat_explicit_tet4 | init | final | 5 | 1 | 13.03 ms [11.36 ms, 13.73 ms] | 13.03 ms [11.36 ms, 13.73 ms] | 46.6 [40.6, 49.0] | 0.952 [0.922, 1.153] | same |
| heat_explicit_tet4 | stable_time_step | before | 5 | 1 | 11.48 ms [10.97 ms, 12.23 ms] | 11.48 ms [10.97 ms, 12.23 ms] | 41.0 [39.2, 43.7] | – | same |
| heat_explicit_tet4 | stable_time_step | final | 5 | 1 | 10.73 ms [9.62 ms, 11.25 ms] | 10.73 ms [9.62 ms, 11.25 ms] | 38.3 [34.4, 40.2] | 0.906 [0.838, 1.025] | same |
| heat_explicit_tet4 | solve_step | before | 5 | 100 | 3.077 s [2.902 s, 3.089 s] | 30.77 ms [29.02 ms, 30.89 ms] | 109.9 [103.7, 110.3] | – | same |
| heat_explicit_tet4 | solve_step | final | 5 | 100 | 2.279 s [2.171 s, 2.403 s] | 22.79 ms [21.71 ms, 24.03 ms] | 81.4 [77.6, 85.8] | 0.741 [0.720, 0.778] | same |
| structural_beam3_frame_static | mesh | before | 5 | 1 | 170.0 µs [166.0 µs, 182.0 µs] | 170.0 µs [166.0 µs, 182.0 µs] | 27.9 [27.3, 29.9] | – | same |
| structural_beam3_frame_static | mesh | final | 5 | 1 | 176.0 µs [165.0 µs, 198.0 µs] | 176.0 µs [165.0 µs, 198.0 µs] | 28.9 [27.1, 32.5] | 1.016 [0.965, 1.165] | same |
| structural_beam3_frame_static | init | before | 5 | 1 | 19.21 ms [18.32 ms, 19.60 ms] | 19.21 ms [18.32 ms, 19.60 ms] | 3157.6 [3010.8, 3221.7] | – | same |
| structural_beam3_frame_static | init | final | 5 | 1 | 18.89 ms [18.57 ms, 21.46 ms] | 18.89 ms [18.57 ms, 21.46 ms] | 3105.7 [3051.4, 3527.8] | 1.018 [0.948, 1.095] | same |
| structural_beam3_frame_static | assemble_stiffness | before | 5 | 5 | 208.73 ms [188.53 ms, 228.25 ms] | 41.75 ms [37.71 ms, 45.65 ms] | 6861.5 [6197.5, 7503.2] | – | same |
| structural_beam3_frame_static | assemble_stiffness | final | 5 | 5 | 152.52 ms [141.87 ms, 164.60 ms] | 30.50 ms [28.37 ms, 32.92 ms] | 5013.8 [4663.7, 5411.1] | 0.721 [0.662, 0.809] | same |
| structural_beam3_frame_static | solve_step | before | 5 | 1 | 4.451 s [4.284 s, 4.632 s] | 4.451 s [4.284 s, 4.632 s] | 731589.4 [704069.4, 761397.6] | – | same |
| structural_beam3_frame_static | solve_step | final | 5 | 1 | 4.395 s [4.342 s, 4.455 s] | 4.395 s [4.342 s, 4.455 s] | 722328.2 [713647.9, 732171.4] | 0.987 [0.955, 1.014] | same |
| structural_dkt18_plate_static | mesh | before | 5 | 1 | 159.0 µs [149.0 µs, 213.0 µs] | 159.0 µs [149.0 µs, 213.0 µs] | 22.1 [20.7, 29.6] | – | same |
| structural_dkt18_plate_static | mesh | final | 5 | 1 | 160.0 µs [146.0 µs, 161.0 µs] | 160.0 µs [146.0 µs, 161.0 µs] | 22.2 [20.3, 22.4] | 1.006 [0.685, 1.074] | same |
| structural_dkt18_plate_static | init | before | 5 | 1 | 44.82 ms [42.61 ms, 50.14 ms] | 44.82 ms [42.61 ms, 50.14 ms] | 6225.6 [5918.6, 6963.6] | – | same |
| structural_dkt18_plate_static | init | final | 5 | 1 | 42.04 ms [41.66 ms, 43.43 ms] | 42.04 ms [41.66 ms, 43.43 ms] | 5839.2 [5785.6, 6031.8] | 0.969 [0.831, 1.014] | same |
| structural_dkt18_plate_static | assemble_stiffness | before | 5 | 5 | 732.82 ms [693.13 ms, 773.78 ms] | 146.56 ms [138.63 ms, 154.76 ms] | 20356.2 [19253.5, 21494.0] | – | same |
| structural_dkt18_plate_static | assemble_stiffness | final | 5 | 5 | 478.98 ms [464.42 ms, 658.32 ms] | 95.80 ms [92.88 ms, 131.66 ms] | 13305.0 [12900.4, 18286.8] | 0.691 [0.614, 0.898] | same |
| structural_dkt18_plate_static | solve_step | before | 5 | 1 | 1.982 s [1.859 s, 2.045 s] | 1.982 s [1.859 s, 2.045 s] | 275329.2 [258191.5, 284067.6] | – | same |
| structural_dkt18_plate_static | solve_step | final | 5 | 1 | 1.937 s [1.899 s, 2.093 s] | 1.937 s [1.899 s, 2.093 s] | 269008.6 [263751.0, 290638.3] | 0.996 [0.937, 1.056] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | 64000 | 65641 | 131282 | 1 | 184dbb6e5 | e12d9fbc77254935 |
| contact_search_quad4 | 140000 | 210006 | 420012 | 1 | 184dbb6e5 | ebe8b1e3825f515d |
| contact_search_tet4 | 480000 | 121686 | 365058 | 1 | 184dbb6e5 | d5b06d84e1dc501b |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | 184dbb6e5 | 8f333faf172f0935 |
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | 184dbb6e5 | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | 184dbb6e5 | b4798ad9264a3e5e |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | 184dbb6e5 | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | 184dbb6e5 | 1cfc7f010c52d6b3 |
