<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# A nested-dissection ordering for the Eigen SparseLU translation

The `eigen` sparse solver (`akantu_linalg::eigen_sparse_lu`) orders the
columns with Eigen's COLAMD. This directory measures the opt-in nested
dissection of D-125 (`SparseOrdering::NestedDissection`, and `Auto`,
which chooses between the two) against COLAMD.

Machine of every table: Google Cloud, AMD EPYC 9B14 (Zen 4, AVX-512),
8 cores, one thread per core, Ubuntu 24.04, nothing else running. "Base" =
`perf/wave2-base` 739e03eb4 (the branch point); "COLAMD", "ND" and "auto"
= this branch with `--ordering colamd | nested_dissection | auto`. Release
builds with the default flags, made on the machine (2026-11-17). `T`
threads: `--threads T` (the executor's policy), pinned to the cores
`0..T−1`.

## Suite, one thread (size m, 5 repetitions)

`series.sh` (`run_ab.sh`: the variants of a case back to back, their order
rotated per repetition; core 0). Ratio = variant / COLAMD per repetition,
median [min, max]. nnz(L+U): the factors of the case's first solve
(diagnostic build, `diag/`); peak RSS of the whole process
(`/usr/bin/time`). The branch point gives the digests of COLAMD on every
case (ratio base / COLAMD 0.993–1.014, `series/summary.md`); `auto` gives
nested dissection's digests where it chose it (on `phasefield_staggered_tet4`
it keeps COLAMD for the 1,331-unknown phase-field system).

| case | phase | COLAMD | ND / COLAMD | auto / COLAMD | nnz(L+U) COLAMD / ND | peak RSS COLAMD / ND / auto (MiB) |
|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 49.5 ms | 0.852 [0.850, 0.854] | 2.138 [2.137, 2.142] | 8478582 / 6048144 | 309 / 230 / 230 |
| static_linear_eigen | factorize | 1.006 s | 0.460 [0.448, 0.467] | 0.465 [0.452, 0.472] |  | 309 / 230 / 230 |
| static_linear_eigen | back_substitution | 127.3 ms | 0.618 [0.615, 0.627] | 0.619 [0.612, 0.623] |  | 309 / 230 / 230 |
| static_linear_eigen | solve_step | 704.3 ms | 0.565 [0.556, 0.570] | 0.655 [0.646, 0.660] |  | 309 / 230 / 230 |
| static_newton_plastic | solve_step | 1.671 s | 0.800 [0.766, 0.811] | 0.800 [0.765, 0.854] | 1976280 / 1528368 | 67 / 61 / 62 |
| implicit_dynamic | solve_step | 1.014 s | 0.718 [0.715, 0.720] | 0.718 [0.714, 0.722] | 8478582 / 6048144 | 211 / 176 / 176 |
| cohesive_intrinsic_implicit | solve_step | 2.999 s | 0.644 [0.642, 0.645] | 0.656 [0.654, 0.657] | 2857248 / 1745468 | 135 / 120 / 123 |
| heat_static_tri6 | solve_step | 1.752 s | 0.484 [0.482, 0.488] | 0.514 [0.509, 0.523] | 9139634 / 3959318 | 248 / 199 / 200 |
| heat_implicit_hex8_kvar | solve_step | 581.0 ms | 0.579 [0.576, 0.583] | 0.578 [0.577, 0.582] | 553756 / 409290 | 33 / 32 / 32 |
| structural_beam3_frame_static | solve_step | 1.341 s | 0.565 [0.562, 0.570] | 0.604 [0.599, 0.607] | 15017586 / 10701834 | 259 / 206 / 206 |
| structural_beam3_frame_dynamic | solve_step | 2.915 s | 1.006 [1.005, 1.009] | 0.999 [0.998, 1.004] | 17795292 / 21589212 | 313 / 338 / 313 |
| structural_dkt18_plate_static | solve_step | 893.5 ms | 3.086 [2.970, 3.090] | 0.993 [0.966, 1.004] | 14451516 / 30912234 | 365 / 524 / 365 |
| phasefield_staggered_tri3 | solid_solve | 3.040 s | 0.468 [0.467, 0.472] | 0.468 [0.467, 0.470] | 12453846 / 6875302 | 408 / 330 / 329 |
| phasefield_staggered_tri3 | phase_solve | 790.8 ms | 0.606 [0.602, 0.607] | 0.612 [0.607, 0.613] |  | 408 / 330 / 329 |
| phasefield_staggered_tet4 | solid_solve | 522.8 ms | 0.589 [0.585, 0.593] | 0.594 [0.587, 0.597] | 1981626 / 1522176 | 81 / 75 / 77 |
| phasefield_staggered_tet4 | phase_solve | 63.3 ms | 0.738 [0.732, 0.742] | 0.994 [0.992, 1.000] |  | 81 / 75 / 77 |
| contact_implicit_static_quad4 | solve_step | 757.8 ms | 0.898 [0.897, 0.906] | 1.003 [0.997, 1.010] | 1299312 / 1321048 | 100 / 100 / 102 |
| embedded_rebar_tri3 | solve_step | 237.2 ms | 0.794 [0.783, 0.799] | 0.915 [0.901, 0.927] | 3891030 / 2337150 | 134 / 118 / 121 |
| embedded_rebar_tet4 | solve_step | 1.066 s | 0.516 [0.512, 0.521] | 0.599 [0.592, 0.603] | 11428734 / 8125752 | 269 / 220 / 220 |

Where nested dissection wins: every 3-D case (factorisation 0.46 of
COLAMD's on `static_linear_eigen`, back-substitution 0.62), and the 2-D
heat, phase-field and beam cases (0.47–0.61), with 10–25 % less peak RSS.
Where it does not: `structural_dkt18_plate_static` (3.09 times COLAMD's
time, 1.4 times its peak RSS, twice its fill) and
`structural_beam3_frame_dynamic` (1.006, fill 1.21 times): the two matrices
whose factorisation pivots off the diagonal (below, "`Auto`"); and the
contact case gains only 0.90 with 1.7 % more fill. `auto` keeps COLAMD on
those three (ratios 0.993–1.003, within the spread) and costs, where it
chooses nested dissection, one more analysis (49 ms on
`static_linear_eigen`, the 0.565 → 0.655 of `solve_step`).

## Threads (size m)

The same series at 2, 4 and 8 threads; ratio to COLAMD at one thread
(median of the per-repetition ratios).

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

## `static_linear_eigen`, size l (73,167 unknowns)

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

At this size nested dissection needs 0.21 of COLAMD's factorisation time,
half its fill and 0.55 of its peak RSS; with 8 threads it factorises in
9.9 s, against 142 s for COLAMD on one thread and 41 s on 8. The analysis
(ordering, elimination tree) costs 0.35 s, less than COLAMD's 0.51 s.


## The ordering

`eigen_sparse_lu/nested_dissection.rs`, no external library:

1. The graph of `A + Aᵀ` without the diagonal; vertices with the same
   closed neighbourhood (the unknowns of one node) merged into weighted
   supervariables.
2. Multilevel bisection: heavy-edge matching (vertices by increasing
   degree, then index) down to 100 vertices; breadth-first growth to half
   the weight from four seeds (a pseudo-peripheral vertex and three spaced
   ones), each refined; Fiduccia–Mattheyses refinement of the edge cut at
   each level (4 passes, a pass stops after 50 moves without improvement;
   part weights within 0.5 ± 0.05 of the total, or one vertex more).
3. The separator: the minimum-weight vertex cover of the cut edges
   (König's theorem; Dinic's maximum flow on the bipartite graph of the
   two boundaries).
4. Recursion on each part (each connected component separately), the
   separator numbered last; parts of at most 64 supervariables are ordered
   by minimum degree.

Ties go to the smaller index everywhere; no hash-map order, no random
number, one thread: the same pattern gives the same permutation on every
machine and for every thread count
(`tests/sparse_lu_ordering.rs::the_permutation_depends_on_the_pattern_only`,
`::nested_dissection_factors_are_thread_invariant`). The elimination tree
and its postorder are computed from the permutation as for COLAMD
(`analyze_with_perm`), so the factorisation code is unchanged.

The fill matches a geometric dissection that knows the grid coordinates
(`examples/nested_dissection_fill.rs`, `fill.txt`; one thread, best of three factorisations):

| grid, unknowns per node | unknowns | nnz(L+U) / COLAMD's: geometric / graph ND | multiply-adds: geometric / graph | factorisation time, graph ND (COLAMD → ND) | analysis COLAMD / graph ND |
|---|---|---|---|---|---|
| 12³ cells, 1 | 2,197 | 0.730 / 0.739 | 0.508 / 0.524 | 0.516 (0.0218 s → 0.0112 s) | 0.0169 s / 0.0183 s |
| 30³ cells, 1 | 29,791 | 0.295 / 0.295 | 0.098 / 0.098 | 0.071 (12.1544 s → 0.8621 s) | 0.2990 s / 0.3112 s |
| 40³ cells, 1 | 68,921 | 0.272 / 0.287 | 0.089 / 0.099 | 0.088 (64.8159 s → 5.6831 s) | 0.7449 s / 0.7842 s |
| 14³ cells, 3 | 10,125 | 0.742 / 0.739 | 0.566 / 0.577 | 0.447 (0.5648 s → 0.2524 s) | 0.0914 s / 0.0469 s |
| 20³ cells, 3 | 27,783 | 0.631 / 0.638 | 0.513 / 0.512 | 0.348 (5.8702 s → 2.0455 s) | 0.5008 s / 0.1459 s |
| 24³ cells, 3 | 46,875 | 0.527 / 0.532 | 0.321 / 0.319 | 0.238 (25.5005 s → 6.0788 s) | 0.6251 s / 0.2504 s |


## `Auto`

`SparseOrdering::Auto` (fix round of the review):

1. COLAMD below 2,000 unknowns or 100,000 stored entries, and when a
   column's diagonal entry is zero or smaller in magnitude than another
   entry of its column (below);
2. otherwise COLAMD's predicted work (`Σ c_j²` over the column counts of
   the symmetric factor of the permuted `A + Aᵀ`, `cholesky_counts`: the
   algorithm of Gilbert, Ng and Peyton, almost linear in the nonzeros);
   the dissection is computed only when that work reaches 5·10⁷ and 3,000
   times the edges of the supervariable graph (the dissection's own cost
   grows with that graph, the factorisation's with the work: a scalar 2-D
   grid of 40,000 unknowns has the work but not the ratio, and nested
   dissection is 1.17 times slower there);
3. nested dissection when its predicted work is at most 0.8 of COLAMD's
   and its predicted size not larger;
4. after the first factorisation with nested dissection: nnz(L + U) above
   twice COLAMD's predicted size, or off-diagonal pivots above 1 % of the
   order, send the pattern back to COLAMD, which it keeps (an indefinite
   matrix with a unit diagonal passes step 1 and pivots off the diagonal
   throughout);
5. the analysis records the effective ordering; a factorisation after a
   switch of the precision mode analyses again, and the AIJ back-end
   factorises again when `factorization_stale` says so, so that a bitwise
   solve after a fast one is COLAMD's (`sparse_ordering_precision_switch.rs`).

Analysis and factorisation with a fresh analysis each time (the cost when
the pattern changes at every solve), one thread, best of 9
(`examples/auto_ordering_cost.rs`, `auto_cost.md`; the review's four small
systems first, then the indefinite grids, then a sweep):

| system | n | COLAMD predicted work | COLAMD | ND / COLAMD | auto / COLAMD | auto chose | fallback |
|---|---|---|---|---|---|---|---|
| strip 4 x 600, c 1 | 2400 | – | 0.0015 s | 4.413 | 1.003 | colamd | false |
| strip 6 x 400, c 2 | 4800 | – | 0.0062 s | 2.548 | 0.997 | colamd | false |
| square 30 x 30, c 3 | 2700 | – | 0.0138 s | 0.856 | 0.994 | colamd | false |
| square 50 x 50, c 1 | 2500 | – | 0.0091 s | 1.627 | 0.999 | colamd | false |
| square 50 x 50, c 3 | 7500 | 1.02e8 | 0.0688 s | 0.610 | 0.827 | nested_dissection | false |
| indefinite 16^3 | 4096 | – | 0.1164 s | 2.302 | 1.003 | colamd | false |
| indefinite 20^3 | 8000 | – | 0.4725 s | 2.340 | 3.419 | colamd | true |
| square 70 x 70, c 1 | 4900 | – | 0.0200 s | 1.400 | 1.003 | colamd | false |
| square 100 x 100, c 1 | 10000 | – | 0.0461 s | 1.435 | 0.999 | colamd | false |
| square 140 x 140, c 1 | 19600 | 1.33e8 | 0.0991 s | 1.345 | 1.042 | colamd | false |
| square 200 x 200, c 1 | 40000 | 4.22e8 | 0.2625 s | 1.168 | 1.000 | colamd | false |
| square 70 x 70, c 2 | 9800 | 1.54e8 | 0.0946 s | 0.527 | 0.699 | nested_dissection | false |
| square 100 x 100, c 2 | 20000 | 4.32e8 | 0.2358 s | 0.506 | 0.654 | nested_dissection | false |
| square 70 x 70, c 3 | 14700 | 3.22e8 | 0.1762 s | 0.510 | 0.678 | nested_dissection | false |
| square 100 x 100, c 3 | 30000 | 1.93e9 | 0.7462 s | 0.289 | 0.378 | nested_dissection | false |
| cube 12^3, c 1 | 1728 | – | 0.0265 s | 0.832 | 1.000 | colamd | false |
| cube 15^3, c 1 | 3375 | – | 0.0618 s | 0.787 | 1.001 | colamd | false |
| cube 18^3, c 1 | 5832 | 6.65e8 | 0.1740 s | 0.546 | 0.785 | nested_dissection | false |
| cube 22^3, c 1 | 10648 | 3.92e9 | 0.8520 s | 0.276 | 0.387 | nested_dissection | false |
| cube 26^3, c 1 | 17576 | 2.11e10 | 4.5780 s | 0.110 | 0.144 | nested_dissection | false |
| cube 8^3, c 3 | 1536 | – | 0.0354 s | 0.447 | 1.001 | colamd | false |
| cube 10^3, c 3 | 3000 | 3.23e8 | 0.1086 s | 0.398 | 0.575 | nested_dissection | false |
| cube 12^3, c 3 | 5184 | 1.20e9 | 0.2470 s | 0.425 | 0.561 | nested_dissection | false |
| cube 14^3, c 3 | 8232 | 1.74e9 | 0.4286 s | 0.544 | 0.684 | nested_dissection | false |

Where `auto` keeps COLAMD its checks cost at most 4.2 % (1.042 on the
140 × 140 scalar grid, whose dissection it computes and rejects by the
edge test; the review's four systems 0.994–1.003). The indefinite 20³
grid pays one nested-dissection factorisation before the fallback (3.4
times COLAMD's analysis and factorisation, once per pattern). On the suite
(5 repetitions, one thread, `series-fix/`; `--ordering auto` against
`--ordering colamd`, both with the bitwise kernels):

| case | auto / COLAMD | auto chose |
|---|---|---|
| static_linear_eigen solve_step | 0.631 [0.616, 0.641] | ND |
| static_linear_eigen analysis | 1.733 [1.729, 1.739] | ND (both orderings) |
| static_newton_plastic | 0.792 [0.786, 0.798] | ND |
| implicit_dynamic | 0.710 [0.709, 0.714] | ND |
| cohesive_intrinsic_implicit | 0.655 [0.643, 0.659] | ND |
| cohesive_extrinsic_insertion (explicit) | 1.000 [0.994, 1.003] | no solve |
| heat_static_tri6 | 0.507 [0.498, 0.509] | ND |
| heat_implicit_hex8_kvar (50,653 entries) | 1.005 [0.998, 1.009] | COLAMD |
| structural_beam3_frame_static | 0.578 [0.573, 0.579] | ND |
| structural_beam3_frame_dynamic | 0.996 [0.991, 1.000] | COLAMD |
| structural_dkt18_plate_static | 1.000 [0.987, 1.004] | COLAMD |
| phasefield_staggered_tri3 solid / phase | 0.469 / 0.610 | ND |
| phasefield_staggered_tet4 solid / phase | 0.592 / 0.998 | ND / COLAMD |
| contact_implicit_static_quad4 (pattern changes) | 0.993 [0.990, 1.000] | COLAMD |
| embedded_rebar_tri3 / tet4 | 0.879 / 0.566 | ND |

`heat_implicit_hex8_kvar` lost its gain (0.58 with nested dissection):
its 2,197 unknowns and 50,653 entries are below the nonzero threshold that
keeps the review's small systems within 5 %.

Why the diagonal test: the threshold partial pivoting
(`diagpivotthresh = 1`) takes the diagonal entry only when it is the
largest of its column after the updates. A matrix whose diagonal is not
the largest entry of its column before any update pivots off the
diagonal; COLAMD orders for the structure of `AᵀA`, which bounds the fill
of any row pivoting, while the dissection of `A + Aᵀ` assumes diagonal
pivots. The suite's two such matrices are the two where nested dissection
does not pay (diagnostic build, `diag/`):

| case | columns whose diagonal is not the largest entry | off-diagonal pivots COLAMD / ND | nnz(L+U) COLAMD / ND |
|---|---|---|---|
| `structural_dkt18_plate_static` | 6,962 of 22,326 | 10,139 / 7,109 | 14,451,516 / 30,912,234 |
| `structural_beam3_frame_dynamic` | 1,486 of 13,182 | 2,099 / 1,564 | 17,795,292 / 21,589,212 |

Every other solver matrix of the suite has none, and none of them pivots
off the diagonal except `cohesive_intrinsic_implicit` (110 of 14,884
columns with COLAMD, 68 with ND in its first solve, where ND still pays).

## Validation

Rule `fast_mode/linalg/sparse_lu_ordering`
(`fixtures/fast_mode/linalg/tolerances.json`,
`tests/sparse_lu_ordering.rs`): `|ND − COLAMD| ≤ 1e-12·max|x|` on three
matrices (3-D grids with 1 and 3 unknowns per node, a saddle-point matrix);
measured at most 2.1e-15 of max|x|. The other tests of that file: the
residuals `‖Ax − b‖/‖b‖` of both orderings (below 2.5e-15 on its
matrices), the off-diagonal pivots of both (the saddle-point matrix: 1,157
with COLAMD, 1,231 with ND; a diagonally dominant matrix: none with
either), the same kind of error for a structurally singular matrix (an
empty row), a column of stored zeros and a rank-one block, factors and
solutions identical at 1, 2, 3, 4 and 8 threads (both grains, every update
threshold), and a permutation that depends on the pattern only.

**Suite** (diagnostic build, `diag/summary.md`): per solver case, the
first solve's residual and 1-norm condition estimate, and the difference
between the two orderings' digested states (`--dump-state`, every value
the digest absorbs), relative to the largest value:

| case | condition estimate | residual COLAMD / ND | state difference |
|---|---|---|---|
| cohesive_intrinsic_implicit | 1.85e+05 | 3.157e-15 / 2.652e-15 | 2.58e-13 |
| contact_implicit_static_quad4 | 7.42e+03 | 9.677e-16 / 7.405e-16 | 1.39e-17 |
| embedded_rebar_tet4 | 2.54e+10 | 6.197e-14 / 4.550e-14 | 1.01e-13 |
| embedded_rebar_tri3 | 2.52e+11 | 4.892e-13 / 3.477e-13 | 3.49e-13 |
| heat_implicit_hex8_kvar | 1.80e+02 | 5.374e-16 / 4.685e-16 | 4.48e-16 |
| heat_static_tri6 | 3.07e+04 | 2.336e-15 / 1.590e-15 | 2.66e-14 |
| implicit_dynamic | 9.69e+01 | 1.668e-15 / 1.232e-15 | 1.64e-15 |
| phasefield_staggered_tet4 | 2.15e+03 | 1.940e-15 / 1.489e-15 | 1.88e-15 |
| phasefield_staggered_tri3 | 1.42e+05 | 4.328e-15 / 3.126e-15 | 2.62e-14 |
| static_linear_eigen | 6.85e+10 | 2.823e-15 / 2.052e-15 | 3.32e-14 |
| static_newton_plastic | 9.59e+10 | 1.734e-15 / 1.331e-15 | 4.29e-15 |
| structural_beam3_frame_dynamic | 9.74e+10 | 1.095e-15 / 1.418e-15 | 1.14e-13 |
| structural_beam3_frame_static | 8.65e+09 | 2.449e-12 / 1.613e-12 | 1.53e-13 |
| structural_dkt18_plate_static | 1.85e+10 | 6.647e-11 / 1.258e-10 | 4.84e-12 |

The differences grow with the condition number and stay far below the
worst-case `κ·u`; the residuals of the two orderings are of the same size
(nested dissection's mostly smaller: less fill, shorter sums). The plate
is the outlier: it pivots off the diagonal in 45 % of its columns, and its
residuals (6.6e-11, 1.3e-10) are the largest of the suite with either
ordering.

**Cohesive static solve** (diagnostic build, `diag/cohesive.txt`): the
displacements differ by 9.1e-14 of max|u| at h = 0.12 (condition estimate
2.4e11; residuals 1.2e-13 COLAMD, 8.5e-14 ND) and 9.7e-14 at h = 0.06
(1.3e11; 5.3e-13, 3.1e-13). After the 10 explicit steps of the series
(`c1/SUMMARY.md`) the energies differ by 6e-16 (potential) and 5e-13
(kinetic) relatively at h = 0.12, 3e-14 and 7e-13 at h = 0.06; COLAMD's
equal C++'s in every printed digit.

**Fragmentation** (`c1ins/`, h = 0.12, 1,200 steps of the full schedule,
one run each): C++, the port with COLAMD and the port with ND insert the
same 129 cohesive elements in the same 113 insertion steps, the first at
step 710 (identical `insertions.csv`). COLAMD's energy history equals
C++'s in every printed digit; ND's differs at most by 1.2e-9 relatively in
the dissipated energy (growing with the cracks, largest at the last
step), 4.7e-11 in the external work, 5.4e-13 in the kinetic, 1.3e-14 in
the potential and 7.2e-15 in the total energy. Over longer runs the
dissipated energy's difference keeps growing and a different insertion
sequence is to be expected, as between two solvers (the baseline's
MUMPS / SparseLU comparison, `../baseline/summary.md`).

## Against C++: the cohesive static solve, Eigen's SparseLU on both sides

`c1_nd.sh` (c1prep cases `cppeig` / `rusteig`, one rank, the oracle's
strict build on the C++ side; 3 repetitions at h = 0.12, 2 at h = 0.06;
the variants of a repetition in a rotated order). `static_solve` median
[min, max] and the paired ratios; peak RSS of the run; raw data `c1/`,
`c1/SUMMARY.md` (`c1_summary.py`).

| h | unknowns | C++ | COLAMD 1 thread | ND 1 | ND 2 | ND 4 | ND 8 | COLAMD 8 | peak RSS C++ / COLAMD / ND (MiB) |
|---|---|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.908 s | 2.062 s (0.708) | 0.869 s (0.299) | 0.769 s (0.265) | 0.697 s (0.239) | 0.670 s (0.230) | 1.270 s (0.437) | 526 / 418 / 348 |
| 0.06 | 97,551 | 245.6 s | 206.0 s (0.839) | 38.3 s (0.156) | 25.5 s (0.104) | 17.8 s (0.072) | 14.2 s (0.058) | 56.7 s (0.231) | 7,624 / 5,335 / 3,163 |

(ratio to C++ per repetition in parentheses). Fill: 16.4 M and 9.4 M
nonzeros in L + U at h = 0.12, 380 M and 156 M at h = 0.06.

## Threads over the elimination tree

Not implemented in this round. Eigen's SparseLU factorises along the
column elimination tree of `AᵀA`. Independent subtrees of that tree touch
disjoint rows: their pivot choices cannot interact, and every update a
column receives comes from its own subtree. A concurrent factorisation of
independent subtrees (private factor buffers and work arrays, the buffers
stitched in column order, then the columns above them serially) would
therefore give the same bits for a given ordering, with no new pivoting
rule. What it needs:

- **Storage and work arrays.** The factor storage is appended in column
  order (`xlusup`, `xlsub`, `xusub` are prefix offsets), and a subtree's
  storage is known only after its factorisation: per-subtree buffers,
  offsets shifted when they are stitched. The depth-first searches and the
  panel accumulator (`marker`, `repfnz`, `parent`, `xplore`, `dense`) span
  all `m` rows: per-thread copies. Panels (16 columns) and relaxed
  supernodes must stop at the subtree roots.
- **An ordering whose subtrees are independent in that tree.** The
  dissection of `A + Aᵀ` does not give them: the separator rows couple the
  two halves in the graph of `AᵀA` (two columns with a nonzero in the same
  separator row are adjacent there). Measured on the factors
  (`diag/tree.txt`: work per column = the updates it receives; the
  heaviest subtree split greedily, the subtrees assigned to threads by
  longest processing time, the split columns serial):

| matrix | ordering | share of the work in independent subtrees of the column tree | speed-up bound, 2 / 4 / 8 threads | the same on the tree of `A + Aᵀ` |
|---|---|---|---|---|
| `static_linear_eigen` l (73,167) | nested dissection | 0.000 | 1.00 / 1.00 / 1.00 | 1.79 / 1.85 / 1.85 |
| cohesive h = 0.06 (97,551) | nested dissection | 0.001 | 1.00 / 1.00 / 1.00 | 1.71 / 1.83 / 1.83 |
| cohesive h = 0.06 | COLAMD | 0.000 | 1.00 / 1.00 / 1.00 | 1.17 / 1.18 / 1.20 |

  With this ordering a subtree-parallel factorisation gains nothing: no
  split of the column tree does better than one thread. Separators
  widened to the graph of `AᵀA` (the dissection's separator plus the
  unknowns of the rows that couple the halves, about twice as wide) would
  give subtrees like those of the tree of `A + Aᵀ`, whose bound is 1.8 at
  4 and 8 threads (the columns above the subtrees, about 31 % of the work,
  stay serial there) before the extra fill of the wider separators, and where
  the panel threads already work on those top separators.

Estimate: the widened dissection (a vertex separator of the distance-2
graph, a variant of `nested_dissection.rs`), the subtree driver (buffers,
per-thread work arrays, stitching, panels cut at subtree roots; about the
size of `SparseLu::factorize` and `kernels::panel_dfs` together) and its
bitwise thread-invariance tests: a round of its own, for at most 1.8× on
the subtree part of large 3-D factorisations.

What the existing threads give under the new ordering (the panel updates
of at least 65,536 multiply-adds, `kernels::panel_bmod`): the thread
columns of the tables above. Nested dissection gives larger separator
supernodes near the root, so more of its updates pass the threshold: on
`static_linear_eigen` (size m) 8 threads take its factorisation from 0.460
to 0.344 of COLAMD's one-thread time (×0.75), COLAMD's from 1 to 0.757;
at size l, 30.2 s → 9.9 s (×0.33) against 142 s → 41 s (×0.29). The 2-D
cases gain little from threads with either ordering (`heat_static_tri6`
0.484 → 0.471): their supernodes stay small. The triangular solves are
not threaded (back-substitution flat in both).

## Files

- `series.sh`: the suite series (wrappers adding `--ordering` and recording
  the peak RSS of each run); `nd_summary.py`: the tables
  (`summary.md`); raw data in `series/` (one directory per suite and thread
  count, `rss/`).
- `c1_nd.sh`: the cohesive static solve against C++; raw data and
  summaries in `c1/`.
- `diagnostics.patch`: the diagnostic build (not part of the branch):
  per solve the relative residual, a 1-norm condition estimate (Hager's
  method with solves by `A` only, valid for the symmetric matrices), the
  largest asymmetry, the off-diagonal pivots and nnz(L+U), and the
  solution vectors appended to `ND_DUMP_X`; results in `diag/`.
- `compare_states.py`: value-by-value comparison of two state dumps
  (`--dump-state`) or solution files.
- `fill.txt`: `examples/nested_dissection_fill.rs` on 3-D grids.
