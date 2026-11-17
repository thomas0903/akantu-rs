## One thread, size m

Ratio = variant / COLAMD per repetition, median [min, max]; peak RSS of the process, median (MiB).

| case | phase | COLAMD | ND | auto | base | RSS COLAMD / ND / auto | digests |
|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 49.5 ms | 0.852 [0.850, 0.854] | 2.138 [2.137, 2.142] | 1.014 [1.010, 1.023] | 309 / 230 / 230 | base = COLAMD; auto = ND |
| static_linear_eigen | factorize | 1.006 s | 0.460 [0.448, 0.467] | 0.465 [0.452, 0.472] | 1.011 [0.996, 1.019] | 309 / 230 / 230 | base = COLAMD; auto = ND |
| static_linear_eigen | back_substitution | 127.3 ms | 0.618 [0.615, 0.627] | 0.619 [0.612, 0.623] | 1.010 [0.995, 1.013] | 309 / 230 / 230 | base = COLAMD; auto = ND |
| static_linear_eigen | solve_step | 704.3 ms | 0.565 [0.556, 0.570] | 0.655 [0.646, 0.660] | 1.001 [0.991, 1.013] | 309 / 230 / 230 | base = COLAMD; auto = ND |
| static_newton_plastic | solve_step | 1.671 s | 0.800 [0.766, 0.811] | 0.800 [0.765, 0.854] | 0.996 [0.956, 1.012] | 67 / 61 / 62 | base = COLAMD; auto = ND |
| implicit_dynamic | solve_step | 1.014 s | 0.718 [0.715, 0.720] | 0.718 [0.714, 0.722] | 1.002 [0.999, 1.006] | 211 / 176 / 176 | base = COLAMD; auto = ND |
| cohesive_intrinsic_implicit | solve_step | 2.999 s | 0.644 [0.642, 0.645] | 0.656 [0.654, 0.657] | 1.000 [0.996, 1.003] | 135 / 120 / 123 | base = COLAMD; auto = ND |
| heat_static_tri6 | solve_step | 1.752 s | 0.484 [0.482, 0.488] | 0.514 [0.509, 0.523] | 1.000 [0.996, 1.018] | 248 / 199 / 200 | base = COLAMD; auto = ND |
| heat_implicit_hex8_kvar | solve_step | 581.0 ms | 0.579 [0.576, 0.583] | 0.578 [0.577, 0.582] | 0.993 [0.987, 0.999] | 33 / 32 / 32 | base = COLAMD; auto = ND |
| structural_beam3_frame_static | solve_step | 1.341 s | 0.565 [0.562, 0.570] | 0.604 [0.599, 0.607] | 0.999 [0.992, 1.010] | 259 / 206 / 206 | base = COLAMD; auto = ND |
| structural_beam3_frame_dynamic | solve_step | 2.915 s | 1.006 [1.005, 1.009] | 0.999 [0.998, 1.004] | 1.007 [1.004, 1.010] | 313 / 338 / 313 | base = COLAMD; auto = COLAMD |
| structural_dkt18_plate_static | solve_step | 893.5 ms | 3.086 [2.970, 3.090] | 0.993 [0.966, 1.004] | 1.007 [0.967, 1.021] | 365 / 524 / 365 | base = COLAMD; auto = COLAMD |
| phasefield_staggered_tri3 | solid_solve | 3.040 s | 0.468 [0.467, 0.472] | 0.468 [0.467, 0.470] | 1.004 [0.997, 1.009] | 408 / 330 / 329 | base = COLAMD; auto = ND |
| phasefield_staggered_tri3 | phase_solve | 790.8 ms | 0.606 [0.602, 0.607] | 0.612 [0.607, 0.613] | 0.994 [0.990, 0.998] | 408 / 330 / 329 | base = COLAMD; auto = ND |
| phasefield_staggered_tet4 | solid_solve | 522.8 ms | 0.589 [0.585, 0.593] | 0.594 [0.587, 0.597] | 1.000 [0.988, 1.005] | 81 / 75 / 77 | base = COLAMD; auto mixed |
| phasefield_staggered_tet4 | phase_solve | 63.3 ms | 0.738 [0.732, 0.742] | 0.994 [0.992, 1.000] | 0.996 [0.990, 0.999] | 81 / 75 / 77 | base = COLAMD; auto mixed |
| contact_implicit_static_quad4 | solve_step | 757.8 ms | 0.898 [0.897, 0.906] | 1.003 [0.997, 1.010] | 0.992 [0.988, 1.001] | 100 / 100 / 102 | base = COLAMD; auto = COLAMD |
| embedded_rebar_tri3 | solve_step | 237.2 ms | 0.794 [0.783, 0.799] | 0.915 [0.901, 0.927] | 1.014 [0.993, 1.024] | 134 / 118 / 121 | base = COLAMD; auto = ND |
| embedded_rebar_tet4 | solve_step | 1.066 s | 0.516 [0.512, 0.521] | 0.599 [0.592, 0.603] | 1.006 [0.997, 1.016] | 269 / 220 / 220 | base = COLAMD; auto = ND |

## Threads, size m

Ratio to COLAMD at one thread, median of the per-repetition ratios.

| case | phase | COLAMD 1 | COLAMD 2 | 4 | 8 | ND 1 | ND 2 | 4 | 8 | auto 8 |
|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 49.5 ms | 1.003 | 0.970 | 0.998 | 0.852 | 0.776 | 0.777 | 0.780 | 2.066 |
| static_linear_eigen | factorize | 1.006 s | 0.876 | 0.788 | 0.757 | 0.460 | 0.418 | 0.364 | 0.344 | 0.343 |
| static_linear_eigen | back_substitution | 127.3 ms | 0.993 | 1.007 | 1.001 | 0.618 | 0.621 | 0.622 | 0.626 | 0.625 |
| static_linear_eigen | solve_step | 704.3 ms | 0.901 | 0.840 | 0.804 | 0.565 | 0.518 | 0.487 | 0.476 | 0.564 |
| static_newton_plastic | solve_step | 1.671 s | 0.843 | 0.812 | 0.808 | 0.800 | 0.642 | 0.618 | 0.614 | 0.613 |
| implicit_dynamic | solve_step | 1.014 s | 0.924 | 0.898 | 0.884 | 0.718 | 0.634 | 0.605 | 0.593 | 0.593 |
| cohesive_intrinsic_implicit | solve_step | 2.999 s | 0.994 | 0.904 | 0.900 | 0.644 | 0.633 | 0.626 | 0.609 | 0.608 |
| heat_static_tri6 | solve_step | 1.752 s | 1.002 | 0.999 | 1.005 | 0.484 | 0.486 | 0.471 | 0.471 | 0.509 |
| heat_implicit_hex8_kvar | solve_step | 581.0 ms | 0.956 | 0.944 | 0.928 | 0.579 | 0.543 | 0.533 | 0.520 | 0.523 |
| structural_beam3_frame_static | solve_step | 1.341 s | 0.795 | 0.688 | 0.618 | 0.565 | 0.514 | 0.471 | 0.441 | 0.485 |
| structural_beam3_frame_dynamic | solve_step | 2.915 s | 0.997 | 0.989 | 0.991 | 1.006 | 1.001 | 0.996 | 0.995 | 0.990 |
| structural_dkt18_plate_static | solve_step | 893.5 ms | 0.996 | 0.984 | 0.972 | 3.086 | 2.817 | 2.506 | 2.292 | 0.975 |
| phasefield_staggered_tri3 | solid_solve | 3.040 s | 0.919 | 0.893 | 0.898 | 0.468 | 0.417 | 0.440 | 0.403 | 0.434 |
| phasefield_staggered_tri3 | phase_solve | 790.8 ms | 0.939 | 0.918 | 0.918 | 0.606 | 0.563 | 0.548 | 0.539 | 0.548 |
| phasefield_staggered_tet4 | solid_solve | 522.8 ms | 0.974 | 0.936 | 0.897 | 0.589 | 0.584 | 0.568 | 0.546 | 0.549 |
| phasefield_staggered_tet4 | phase_solve | 63.3 ms | 1.005 | 0.987 | 0.992 | 0.738 | 0.749 | 0.762 | 0.753 | 1.006 |
| contact_implicit_static_quad4 | solve_step | 757.8 ms | 0.958 | 0.933 | 0.927 | 0.898 | 0.851 | 0.825 | 0.819 | 0.923 |
| embedded_rebar_tri3 | solve_step | 237.2 ms | 0.992 | 0.990 | 0.981 | 0.794 | 0.792 | 0.794 | 0.790 | 0.907 |
| embedded_rebar_tet4 | solve_step | 1.066 s | 0.871 | 0.800 | 0.757 | 0.516 | 0.463 | 0.429 | 0.412 | 0.495 |

## static_linear_eigen, size l

| phase | threads | COLAMD | ND | ND / COLAMD | RSS COLAMD / ND (MiB) | nnz(L+U) COLAMD / ND |
|---|---|---|---|---|---|---|
| analysis | 1 | 515.4 ms | 349.1 ms | 0.677 [0.676, 0.678] | 4992 / 2763 | 219826326 / 106501992 |
| factorize | 1 | 142.022 s | 30.179 s | 0.213 [0.211, 0.213] | 4992 / 2763 | 219826326 / 106501992 |
| back_substitution | 1 | 3.162 s | 1.511 s | 0.478 [0.476, 0.481] | 4992 / 2763 | 219826326 / 106501992 |
| solve_step | 1 | 73.697 s | 16.909 s | 0.230 [0.228, 0.231] | 4992 / 2763 | 219826326 / 106501992 |
| analysis | 2 | 514.7 ms | 351.0 ms | 0.682 [0.681, 0.687] | 5157 / 2921 | 219826326 / 106501992 |
| factorize | 2 | 90.202 s | 19.407 s | 0.215 [0.214, 0.218] | 5157 / 2921 | 219826326 / 106501992 |
| back_substitution | 2 | 3.166 s | 1.513 s | 0.478 [0.475, 0.479] | 5157 / 2921 | 219826326 / 106501992 |
| solve_step | 2 | 47.632 s | 11.384 s | 0.239 [0.236, 0.240] | 5157 / 2921 | 219826326 / 106501992 |
| analysis | 4 | 508.9 ms | 347.0 ms | 0.680 [0.677, 0.686] | 5157 / 2925 | 219826326 / 106501992 |
| factorize | 4 | 57.314 s | 13.068 s | 0.228 [0.227, 0.229] | 5157 / 2925 | 219826326 / 106501992 |
| back_substitution | 4 | 3.170 s | 1.519 s | 0.480 [0.477, 0.481] | 5157 / 2925 | 219826326 / 106501992 |
| solve_step | 4 | 31.113 s | 8.172 s | 0.263 [0.262, 0.263] | 5157 / 2925 | 219826326 / 106501992 |
| analysis | 8 | 508.8 ms | 352.1 ms | 0.693 [0.686, 0.706] | 5159 / 2931 | 219826326 / 106501992 |
| factorize | 8 | 41.035 s | 9.896 s | 0.241 [0.236, 0.242] | 5159 / 2931 | 219826326 / 106501992 |
| back_substitution | 8 | 3.174 s | 1.529 s | 0.480 [0.472, 0.490] | 5159 / 2931 | 219826326 / 106501992 |
| solve_step | 8 | 22.964 s | 6.568 s | 0.287 [0.285, 0.288] | 5159 / 2931 | 219826326 / 106501992 |
