# Model suite: baseline at size `m`

- Machine: an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB, Ubuntu
  24.04); every run one process pinned to core 2 (`taskset -c 2`), inside
  the campaign's timing wrapper (cores 0–3 reserved for one timing job at a
  time, builds on cores 4–7).
- Binary: `suite_models` of commit `ee4140ce`, release profile, no extra
  flags, built in a development VM with the same Ubuntu and toolchain;
  SHA-256 in `meta.json`.
- Command: `taskset -c 2 suite_models --case CASE --size m --reps 1 --out
  base/CASE.rR.jsonl`; five repetitions, each a separate timing job with the
  case order rotated by one place per repetition.
- Raw data: `base/CASE.rR.jsonl` (one JSON line per phase), `meta.json`.
- Table: `summarize.py` of the solid suite (`scripts/perf/suite/`, branch
  `perf/suite-solid`). Times are median [min, max] over the five runs;
  "ns/(element·call)" divides the per-call time by the element count of the
  case (meaningful for loop phases). DOFs count every model's unknowns (phase field: solid and damage). The digest column says whether every
  run produced the same digest.
- One-shot phases (`mesh`, `init`, `solve_step` of the static cases) are
  single calls; loop phases are totals over `calls` after three untimed
  warm-up calls.

Date (UTC): 2026-11-16, 07:46–10:30 (five timing jobs).

## Headline: the loop and solve phases

Per call, median [min, max] over five runs; the last column divides by
the case's elements (loops over elements) or DOFs (solves).

| case | elements | DOFs | phase | per call, median [min, max] | per element or DOF and call |
|---|---|---|---|---|---|
| heat_explicit_tet4 | 279936 | 50653 | solve_step | 40.2 ms [40.2 ms, 43.4 ms] | 144 ns/element |
| heat_explicit_quad4 | 160000 | 160801 | solve_step | 39.8 ms [39.8 ms, 39.9 ms] | 249 ns/element |
| heat_implicit_hex8_kvar | 1728 | 2197 | solve_step | 271.6 ms [271.2 ms, 272.4 ms] | 123.6 µs/DOF |
| heat_static_tri6 | 24200 | 48841 | assemble_conductivity | 45.1 ms [42.6 ms, 47.2 ms] | 1863 ns/element |
| heat_static_tri6 | 24200 | 48841 | solve_step | 4.52 s [4.48 s, 4.54 s] | 92.6 µs/DOF |
| phasefield_staggered_tri3 | 51200 | 77763 | solid_solve | 2.85 s [2.85 s, 2.97 s] | 36.7 µs/DOF |
| phasefield_staggered_tri3 | 51200 | 77763 | phase_solve | 531.7 ms [530.1 ms, 564.8 ms] | 6.8 µs/DOF |
| phasefield_staggered_tri3 | 51200 | 77763 | solid_assemble_stiffness | 64.3 ms [64.1 ms, 76.8 ms] | 1256 ns/element |
| phasefield_staggered_tri3 | 51200 | 77763 | phase_assemble_stiffness | 32.6 ms [32.6 ms, 39.1 ms] | 638 ns/element |
| phasefield_staggered_tet4 | 6000 | 5324 | solid_solve | 556.1 ms [554.2 ms, 584.5 ms] | 104.5 µs/DOF |
| phasefield_staggered_tet4 | 6000 | 5324 | phase_solve | 34.9 ms [34.9 ms, 35.9 ms] | 6.6 µs/DOF |
| contact_search_quad4 | 140000 | 420012 | search | 90.0 ms [89.7 ms, 91.6 ms] | 643 ns/element |
| contact_search_tet4 | 480000 | 365058 | search | 170.4 ms [169.7 ms, 171.9 ms] | 355 ns/element |
| contact_explicit_quad4 | 126000 | 256444 | solve_step | 24.4 ms [24.3 ms, 25.1 ms] | 194 ns/element |
| contact_implicit_static_quad4 | 7000 | 14784 | solve_step | 432.6 ms [429.3 ms, 448.6 ms] | 29.3 µs/DOF |
| contact_cohesive_quad4 | 64000 | 131282 | solve_step | 29.8 ms [29.7 ms, 29.8 ms] | 465 ns/element |
| structural_beam3_frame_static | 6084 | 13182 | assemble_stiffness | 38.3 ms [37.5 ms, 41.2 ms] | 6303 ns/element |
| structural_beam3_frame_static | 6084 | 13182 | solve_step | 5.87 s [5.87 s, 5.88 s] | 445.3 µs/DOF |
| structural_beam3_frame_dynamic | 6084 | 13182 | solve_step | 87.3 ms [86.6 ms, 88.6 ms] | 6.6 µs/DOF |
| structural_dkt18_plate_static | 7200 | 22326 | assemble_stiffness | 154.6 ms [153.8 ms, 163.0 ms] | 21.5 µs/element |
| structural_dkt18_plate_static | 7200 | 22326 | solve_step | 2.65 s [2.63 s, 2.68 s] | 118.6 µs/DOF |
| embedded_rebar_tri3 | 25200 | 20402 | init | 1.52 s [1.51 s, 1.58 s] | 60.2 µs/element |
| embedded_rebar_tri3 | 25200 | 20402 | intersect | 1.56 s [1.50 s, 1.57 s] | 62.0 µs/element |
| embedded_rebar_tri3 | 25200 | 20402 | solve_step | 544.5 ms [543.6 ms, 556.8 ms] | 26.7 µs/DOF |
| embedded_rebar_tri3 | 25200 | 20402 | assemble_internal_forces | 2.4 ms [2.4 ms, 2.5 ms] | 97 ns/element |
| embedded_rebar_tri3 | 25200 | 20402 | assemble_stiffness | 31.8 ms [31.3 ms, 35.6 ms] | 1261 ns/element |
| embedded_rebar_tet4 | 21750 | 12288 | init | 1.45 s [1.42 s, 1.45 s] | 66.5 µs/element |
| embedded_rebar_tet4 | 21750 | 12288 | intersect | 1.40 s [1.40 s, 1.43 s] | 64.5 µs/element |
| embedded_rebar_tet4 | 21750 | 12288 | solve_step | 4.22 s [4.21 s, 4.32 s] | 343.4 µs/DOF |
| ntn_friction_quad4 | 200000 | 416104 | solve_step | 38.0 ms [37.9 ms, 38.3 ms] | 190 ns/element |

## Every phase

| case | phase | variant | reps | calls | seconds | per call | ns/(element·call) | digest |
|---|---|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | mesh | base | 5 | 1 | 98.96 ms [98.92 ms, 99.74 ms] | 98.96 ms [98.92 ms, 99.74 ms] | 1546.2 [1545.6, 1558.4] | same |
| contact_cohesive_quad4 | init | base | 5 | 1 | 216.81 ms [213.17 ms, 217.39 ms] | 216.81 ms [213.17 ms, 217.39 ms] | 3387.6 [3330.8, 3396.8] | same |
| contact_cohesive_quad4 | solve_step | base | 5 | 300 | 8.929 s [8.904 s, 8.934 s] | 29.76 ms [29.68 ms, 29.78 ms] | 465.0 [463.7, 465.3] | same |
| contact_explicit_quad4 | mesh | base | 5 | 1 | 200.81 ms [200.02 ms, 215.07 ms] | 200.81 ms [200.02 ms, 215.07 ms] | 1593.7 [1587.5, 1706.9] | same |
| contact_explicit_quad4 | init | base | 5 | 1 | 265.25 ms [263.77 ms, 272.99 ms] | 265.25 ms [263.77 ms, 272.99 ms] | 2105.2 [2093.4, 2166.6] | same |
| contact_explicit_quad4 | stable_time_step | base | 5 | 1 | 3.67 ms [3.66 ms, 3.76 ms] | 3.67 ms [3.66 ms, 3.76 ms] | 29.1 [29.0, 29.8] | same |
| contact_explicit_quad4 | solve_step | base | 5 | 200 | 4.884 s [4.862 s, 5.029 s] | 24.42 ms [24.31 ms, 25.15 ms] | 193.8 [192.9, 199.6] | same |
| contact_implicit_static_quad4 | mesh | base | 5 | 1 | 11.52 ms [11.40 ms, 11.65 ms] | 11.52 ms [11.40 ms, 11.65 ms] | 1646.0 [1628.1, 1663.9] | same |
| contact_implicit_static_quad4 | init | base | 5 | 1 | 14.79 ms [14.72 ms, 14.85 ms] | 14.79 ms [14.72 ms, 14.85 ms] | 2112.1 [2103.0, 2122.0] | same |
| contact_implicit_static_quad4 | solve_step | base | 5 | 5 | 2.163 s [2.147 s, 2.243 s] | 432.65 ms [429.34 ms, 448.64 ms] | 61806.6 [61334.4, 64091.6] | same |
| contact_search_quad4 | mesh | base | 5 | 1 | 389.78 ms [385.38 ms, 437.82 ms] | 389.78 ms [385.38 ms, 437.82 ms] | 2784.2 [2752.7, 3127.3] | same |
| contact_search_quad4 | init | base | 5 | 1 | 340.19 ms [334.72 ms, 340.33 ms] | 340.19 ms [334.72 ms, 340.33 ms] | 2429.9 [2390.9, 2430.9] | same |
| contact_search_quad4 | search | base | 5 | 60 | 5.399 s [5.383 s, 5.494 s] | 89.98 ms [89.72 ms, 91.56 ms] | 642.7 [640.9, 654.0] | same |
| contact_search_tet4 | mesh | base | 5 | 1 | 874.46 ms [870.87 ms, 950.68 ms] | 874.46 ms [870.87 ms, 950.68 ms] | 1821.8 [1814.3, 1980.6] | same |
| contact_search_tet4 | init | base | 5 | 1 | 486.62 ms [481.06 ms, 492.29 ms] | 486.62 ms [481.06 ms, 492.29 ms] | 1013.8 [1002.2, 1025.6] | same |
| contact_search_tet4 | search | base | 5 | 30 | 5.112 s [5.092 s, 5.156 s] | 170.40 ms [169.73 ms, 171.85 ms] | 355.0 [353.6, 358.0] | same |
| embedded_rebar_tet4 | mesh | base | 5 | 1 | 27.97 ms [27.86 ms, 28.13 ms] | 27.97 ms [27.86 ms, 28.13 ms] | 1285.9 [1280.8, 1293.3] | same |
| embedded_rebar_tet4 | init | base | 5 | 1 | 1.446 s [1.424 s, 1.453 s] | 1.446 s [1.424 s, 1.453 s] | 66503.2 [65478.9, 66810.6] | same |
| embedded_rebar_tet4 | intersect | base | 5 | 5 | 7.014 s [7.006 s, 7.167 s] | 1.403 s [1.401 s, 1.433 s] | 64493.0 [64423.0, 65905.5] | same |
| embedded_rebar_tet4 | solve_step | base | 5 | 1 | 4.220 s [4.215 s, 4.317 s] | 4.220 s [4.215 s, 4.317 s] | 194010.2 [193792.0, 198501.5] | same |
| embedded_rebar_tet4 | assemble_internal_forces | base | 5 | 20 | 34.50 ms [34.33 ms, 34.57 ms] | 1.72 ms [1.72 ms, 1.73 ms] | 79.3 [78.9, 79.5] | same |
| embedded_rebar_tet4 | assemble_stiffness | base | 5 | 5 | 597.92 ms [595.44 ms, 605.99 ms] | 119.58 ms [119.09 ms, 121.20 ms] | 5498.1 [5475.3, 5572.3] | same |
| embedded_rebar_tri3 | mesh | base | 5 | 1 | 27.20 ms [27.15 ms, 27.22 ms] | 27.20 ms [27.15 ms, 27.22 ms] | 1079.5 [1077.5, 1080.0] | same |
| embedded_rebar_tri3 | init | base | 5 | 1 | 1.517 s [1.511 s, 1.584 s] | 1.517 s [1.511 s, 1.584 s] | 60195.9 [59977.8, 62857.3] | same |
| embedded_rebar_tri3 | intersect | base | 5 | 5 | 7.813 s [7.503 s, 7.873 s] | 1.563 s [1.501 s, 1.575 s] | 62010.0 [59547.8, 62487.2] | same |
| embedded_rebar_tri3 | solve_step | base | 5 | 1 | 544.49 ms [543.64 ms, 556.80 ms] | 544.49 ms [543.64 ms, 556.80 ms] | 21606.5 [21573.1, 22095.2] | same |
| embedded_rebar_tri3 | assemble_internal_forces | base | 5 | 20 | 48.94 ms [48.88 ms, 49.16 ms] | 2.45 ms [2.44 ms, 2.46 ms] | 97.1 [97.0, 97.5] | same |
| embedded_rebar_tri3 | assemble_stiffness | base | 5 | 5 | 158.85 ms [156.35 ms, 178.08 ms] | 31.77 ms [31.27 ms, 35.62 ms] | 1260.7 [1240.9, 1413.3] | same |
| heat_explicit_quad4 | mesh | base | 5 | 1 | 231.54 ms [231.29 ms, 232.54 ms] | 231.54 ms [231.29 ms, 232.54 ms] | 1447.1 [1445.5, 1453.4] | same |
| heat_explicit_quad4 | init | base | 5 | 1 | 41.16 ms [40.72 ms, 41.32 ms] | 41.16 ms [40.72 ms, 41.32 ms] | 257.2 [254.5, 258.2] | same |
| heat_explicit_quad4 | stable_time_step | base | 5 | 1 | 3.33 ms [3.33 ms, 3.34 ms] | 3.33 ms [3.33 ms, 3.34 ms] | 20.8 [20.8, 20.9] | same |
| heat_explicit_quad4 | solve_step | base | 5 | 120 | 4.779 s [4.773 s, 4.788 s] | 39.83 ms [39.77 ms, 39.90 ms] | 248.9 [248.6, 249.4] | same |
| heat_explicit_tet4 | mesh | base | 5 | 1 | 342.43 ms [341.58 ms, 358.26 ms] | 342.43 ms [341.58 ms, 358.26 ms] | 1223.3 [1220.2, 1279.8] | same |
| heat_explicit_tet4 | init | base | 5 | 1 | 19.30 ms [19.22 ms, 20.12 ms] | 19.30 ms [19.22 ms, 20.12 ms] | 68.9 [68.7, 71.9] | same |
| heat_explicit_tet4 | stable_time_step | base | 5 | 1 | 14.02 ms [14.00 ms, 14.04 ms] | 14.02 ms [14.00 ms, 14.04 ms] | 50.1 [50.0, 50.2] | same |
| heat_explicit_tet4 | solve_step | base | 5 | 100 | 4.022 s [4.018 s, 4.345 s] | 40.22 ms [40.18 ms, 43.45 ms] | 143.7 [143.5, 155.2] | same |
| heat_implicit_hex8_kvar | mesh | base | 5 | 1 | 4.75 ms [4.73 ms, 4.77 ms] | 4.75 ms [4.73 ms, 4.77 ms] | 2751.7 [2737.3, 2758.7] | same |
| heat_implicit_hex8_kvar | init | base | 5 | 1 | 1.11 ms [1.09 ms, 1.12 ms] | 1.11 ms [1.09 ms, 1.12 ms] | 641.2 [629.6, 647.6] | same |
| heat_implicit_hex8_kvar | solve_step | base | 5 | 5 | 1.358 s [1.356 s, 1.362 s] | 271.64 ms [271.17 ms, 272.37 ms] | 157200.6 [156926.4, 157620.5] | same |
| heat_static_tri6 | mesh | base | 5 | 1 | 49.95 ms [49.85 ms, 50.70 ms] | 49.95 ms [49.85 ms, 50.70 ms] | 2064.0 [2059.9, 2094.9] | same |
| heat_static_tri6 | init | base | 5 | 1 | 7.15 ms [7.13 ms, 7.68 ms] | 7.15 ms [7.13 ms, 7.68 ms] | 295.5 [294.5, 317.4] | same |
| heat_static_tri6 | assemble_conductivity | base | 5 | 5 | 225.48 ms [213.09 ms, 235.92 ms] | 45.10 ms [42.62 ms, 47.18 ms] | 1863.5 [1761.1, 1949.8] | same |
| heat_static_tri6 | solve_step | base | 5 | 1 | 4.521 s [4.481 s, 4.542 s] | 4.521 s [4.481 s, 4.542 s] | 186817.1 [185172.3, 187668.8] | same |
| ntn_friction_quad4 | mesh | base | 5 | 1 | 320.86 ms [320.44 ms, 321.78 ms] | 320.86 ms [320.44 ms, 321.78 ms] | 1604.3 [1602.2, 1608.9] | same |
| ntn_friction_quad4 | init | base | 5 | 1 | 278.35 ms [277.74 ms, 279.15 ms] | 278.35 ms [277.74 ms, 279.15 ms] | 1391.7 [1388.7, 1395.8] | same |
| ntn_friction_quad4 | solve_step | base | 5 | 100 | 3.797 s [3.786 s, 3.830 s] | 37.97 ms [37.86 ms, 38.30 ms] | 189.8 [189.3, 191.5] | same |
| phasefield_staggered_tet4 | mesh | base | 5 | 1 | 8.80 ms [8.78 ms, 8.88 ms] | 8.80 ms [8.78 ms, 8.88 ms] | 1466.2 [1464.0, 1480.0] | same |
| phasefield_staggered_tet4 | init | base | 5 | 1 | 3.34 ms [3.32 ms, 3.35 ms] | 3.34 ms [3.32 ms, 3.35 ms] | 556.3 [552.7, 558.0] | same |
| phasefield_staggered_tet4 | solid_solve | base | 5 | 4 | 2.224 s [2.217 s, 2.338 s] | 556.12 ms [554.23 ms, 584.55 ms] | 92686.6 [92371.5, 97424.9] | same |
| phasefield_staggered_tet4 | strain_transfer | base | 5 | 4 | 693.0 µs [675.0 µs, 715.0 µs] | 173.2 µs [168.8 µs, 178.8 µs] | 28.9 [28.1, 29.8] | same |
| phasefield_staggered_tet4 | phase_solve | base | 5 | 4 | 139.63 ms [139.56 ms, 143.75 ms] | 34.91 ms [34.89 ms, 35.94 ms] | 5817.8 [5815.0, 5989.4] | same |
| phasefield_staggered_tet4 | damage_transfer | base | 5 | 4 | 2.16 ms [2.15 ms, 2.18 ms] | 539.5 µs [538.0 µs, 544.8 µs] | 89.9 [89.7, 90.8] | same |
| phasefield_staggered_tet4 | internal_forces | base | 5 | 4 | 1.88 ms [1.86 ms, 2.01 ms] | 470.2 µs [464.2 µs, 501.8 µs] | 78.4 [77.4, 83.6] | same |
| phasefield_staggered_tet4 | solid_assemble_stiffness | base | 5 | 5 | 101.98 ms [100.06 ms, 120.19 ms] | 20.40 ms [20.01 ms, 24.04 ms] | 3399.2 [3335.3, 4006.3] | same |
| phasefield_staggered_tet4 | phase_assemble_stiffness | base | 5 | 5 | 24.19 ms [24.18 ms, 24.68 ms] | 4.84 ms [4.84 ms, 4.94 ms] | 806.2 [806.0, 822.6] | same |
| phasefield_staggered_tri3 | mesh | base | 5 | 1 | 56.83 ms [56.35 ms, 59.67 ms] | 56.83 ms [56.35 ms, 59.67 ms] | 1109.9 [1100.6, 1165.4] | same |
| phasefield_staggered_tri3 | init | base | 5 | 1 | 22.22 ms [22.15 ms, 23.30 ms] | 22.22 ms [22.15 ms, 23.30 ms] | 433.9 [432.6, 455.1] | same |
| phasefield_staggered_tri3 | solid_solve | base | 5 | 4 | 11.419 s [11.408 s, 11.879 s] | 2.855 s [2.852 s, 2.970 s] | 55756.6 [55703.5, 58002.6] | same |
| phasefield_staggered_tri3 | strain_transfer | base | 5 | 4 | 4.26 ms [4.15 ms, 4.28 ms] | 1.06 ms [1.04 ms, 1.07 ms] | 20.8 [20.3, 20.9] | same |
| phasefield_staggered_tri3 | phase_solve | base | 5 | 4 | 2.127 s [2.120 s, 2.259 s] | 531.71 ms [530.07 ms, 564.82 ms] | 10384.9 [10353.0, 11031.6] | same |
| phasefield_staggered_tri3 | damage_transfer | base | 5 | 4 | 17.58 ms [17.52 ms, 17.72 ms] | 4.40 ms [4.38 ms, 4.43 ms] | 85.9 [85.5, 86.5] | same |
| phasefield_staggered_tri3 | internal_forces | base | 5 | 4 | 10.54 ms [10.51 ms, 10.96 ms] | 2.63 ms [2.63 ms, 2.74 ms] | 51.5 [51.3, 53.5] | same |
| phasefield_staggered_tri3 | solid_assemble_stiffness | base | 5 | 5 | 321.52 ms [320.59 ms, 384.15 ms] | 64.30 ms [64.12 ms, 76.83 ms] | 1255.9 [1252.3, 1500.6] | same |
| phasefield_staggered_tri3 | phase_assemble_stiffness | base | 5 | 5 | 163.23 ms [162.80 ms, 195.61 ms] | 32.65 ms [32.56 ms, 39.12 ms] | 637.6 [635.9, 764.1] | same |
| structural_beam3_frame_dynamic | mesh | base | 5 | 1 | 341.0 µs [335.0 µs, 346.0 µs] | 341.0 µs [335.0 µs, 346.0 µs] | 56.0 [55.1, 56.9] | same |
| structural_beam3_frame_dynamic | init | base | 5 | 1 | 29.16 ms [28.93 ms, 29.49 ms] | 29.16 ms [28.93 ms, 29.49 ms] | 4793.2 [4755.8, 4847.6] | same |
| structural_beam3_frame_dynamic | solve_step | base | 5 | 60 | 5.239 s [5.194 s, 5.316 s] | 87.32 ms [86.56 ms, 88.60 ms] | 14352.2 [14228.0, 14562.2] | same |
| structural_beam3_frame_static | mesh | base | 5 | 1 | 344.0 µs [336.0 µs, 359.0 µs] | 344.0 µs [336.0 µs, 359.0 µs] | 56.5 [55.2, 59.0] | same |
| structural_beam3_frame_static | init | base | 5 | 1 | 28.93 ms [28.91 ms, 29.01 ms] | 28.93 ms [28.91 ms, 29.01 ms] | 4754.8 [4751.0, 4768.6] | same |
| structural_beam3_frame_static | assemble_stiffness | base | 5 | 5 | 191.74 ms [187.70 ms, 205.85 ms] | 38.35 ms [37.54 ms, 41.17 ms] | 6303.1 [6170.3, 6767.0] | same |
| structural_beam3_frame_static | solve_step | base | 5 | 1 | 5.870 s [5.868 s, 5.885 s] | 5.870 s [5.868 s, 5.885 s] | 964815.4 [964486.4, 967222.9] | same |
| structural_dkt18_plate_static | mesh | base | 5 | 1 | 295.0 µs [287.0 µs, 300.0 µs] | 295.0 µs [287.0 µs, 300.0 µs] | 41.0 [39.9, 41.7] | same |
| structural_dkt18_plate_static | init | base | 5 | 1 | 69.28 ms [68.86 ms, 69.72 ms] | 69.28 ms [68.86 ms, 69.72 ms] | 9622.9 [9563.3, 9682.8] | same |
| structural_dkt18_plate_static | assemble_stiffness | base | 5 | 5 | 773.11 ms [768.90 ms, 815.06 ms] | 154.62 ms [153.78 ms, 163.01 ms] | 21475.4 [21358.3, 22640.6] | same |
| structural_dkt18_plate_static | solve_step | base | 5 | 1 | 2.647 s [2.634 s, 2.676 s] | 2.647 s [2.634 s, 2.676 s] | 367705.0 [365842.2, 371653.2] | same |

Problem sizes and digests:

| case | elements | nodes | DOFs | ranks | commit(s) | digest(s) |
|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | 64000 | 65641 | 131282 | 1 | ee4140cec | e12d9fbc77254935 |
| contact_explicit_quad4 | 126000 | 128222 | 256444 | 1 | ee4140cec | 73e756218136a9e6 |
| contact_implicit_static_quad4 | 7000 | 7392 | 14784 | 1 | ee4140cec | 776a3c6490006b8e |
| contact_search_quad4 | 140000 | 210006 | 420012 | 1 | ee4140cec | ebe8b1e3825f515d |
| contact_search_tet4 | 480000 | 121686 | 365058 | 1 | ee4140cec | d5b06d84e1dc501b |
| embedded_rebar_tet4 | 21750 | 4096 | 12288 | 1 | ee4140cec | 9e3b18c54880d2b3 |
| embedded_rebar_tri3 | 25200 | 10201 | 20402 | 1 | ee4140cec | 8f333faf172f0935 |
| heat_explicit_quad4 | 160000 | 160801 | 160801 | 1 | ee4140cec | 84ce5ab70abf5654 |
| heat_explicit_tet4 | 279936 | 50653 | 50653 | 1 | ee4140cec | b4798ad9264a3e5e |
| heat_implicit_hex8_kvar | 1728 | 2197 | 2197 | 1 | ee4140cec | 55313c35e6ae3f79 |
| heat_static_tri6 | 24200 | 48841 | 48841 | 1 | ee4140cec | 463949e032392419 |
| ntn_friction_quad4 | 200000 | 208052 | 416104 | 1 | ee4140cec | ce823a5b9b7c5433 |
| phasefield_staggered_tet4 | 6000 | 1331 | 5324 | 1 | ee4140cec | fd27c4e51950421b |
| phasefield_staggered_tri3 | 51200 | 25921 | 77763 | 1 | ee4140cec | a77809465b3e7c18 |
| structural_beam3_frame_dynamic | 6084 | 2197 | 13182 | 1 | ee4140cec | 7c476f93a2a83956 |
| structural_beam3_frame_static | 6084 | 2197 | 13182 | 1 | ee4140cec | ba3b7d6fb6b7bbbc |
| structural_dkt18_plate_static | 7200 | 3721 | 22326 | 1 | ee4140cec | 1cfc7f010c52d6b3 |
