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

`SparseOrdering::Auto` keeps COLAMD below 2,000 unknowns, and when a
column's diagonal entry is zero or smaller in magnitude than another entry
of its column; otherwise it computes both orderings, predicts their work
(`Σ c_j²` over the column counts `c_j` of the symmetric factor of the
permuted `A + Aᵀ`, `cholesky_counts`, `O(|L|)`) and takes nested dissection
when its work is at most 0.8 of COLAMD's and its predicted size not larger.

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

@VALIDATION@

## Threads over the elimination tree

Not implemented. With a dissection the elimination tree has independent
subtrees, but the left-looking supernodal factorisation of Eigen's
SparseLU cannot factorise them concurrently without a different
algorithm:

- **Pivoting couples the subtrees.** A column's pivot row is chosen among
  the rows not yet pivoted (`perm_r` is global). The rows of separator
  unknowns adjacent to two subtrees are candidates in columns of both: if
  one subtree pivots on such a row, the other may not. The result would
  depend on which subtree runs first, unless the candidate rows of a
  subtree are restricted to its own unknowns, which is another pivoting
  rule (other results, weaker stability), not the same algorithm on more
  threads.
- **The factor storage is appended in column order** (`xlusup`, `xlsub`,
  `xusub` are prefix offsets), and a subtree's storage is known only after
  its factorisation, because the structure follows the pivots. Concurrent
  subtrees need their own buffers stitched in column order afterwards, or
  a symbolic pre-pass that bounds each subtree's storage through the
  structure of `AᵀA`.
- **The work arrays are global**: the panel and column depth-first
  searches use `marker`, `repfnz`, `parent`, `xplore` and the panel
  accumulator over all `m` rows, and walk the pruned graph of every
  earlier supernode; panels (16 columns) and relaxed supernodes cross
  subtree roots.

The smallest restructuring that would give bit-identical results at every
thread count for a given ordering: (1) pivots restricted to the subtree's
own rows below a chosen tree level (a new pivoting rule with its own
tolerance and stability evidence); (2) per-subtree factor buffers and
work arrays, the buffers concatenated in column order after the subtrees;
(3) panels and relaxed supernodes cut at the subtree roots. That is a
rewrite of the factorisation driver (SuperLU_MT's design), not a change of
this module.

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
