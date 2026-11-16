<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The Eigen SparseLU translation

Performance work on the default sparse direct solver
(`akantu_linalg::eigen_sparse_lu`, the translation of Eigen 3.4's
`SparseLU<SparseMatrix<double>, COLAMDOrdering<int>>`): as fast as possible
with the bits of the oracle's strict build (scalar Eigen, no FMA).

Machine of every table: Google Cloud, AMD EPYC 9B14 (Zen 4, AVX-512),
8 cores, Ubuntu 24.04; one process pinned to core 2, nothing else running.
"Before" = `perf/wave2-base` 24f73b94, "after" = this branch at f204c719
(the same suite version); both built on the machine with the release
profile and default flags (2026-11-16).

## What changed (all bitwise: same operations, same order per entry)

| Change | Where | Share addressed (profile before / at that step) |
|---|---|---|
| `sparselu_gemm` in register tiles (`MR` rows × 4 or 8 columns, `k` loop over the depth, `A` read column by column); per entry still `c ← c + a·b` for `k` in order, then the `rd` tail `c + b·a` | `dense.rs` `gemm_tiles` | 74 % of `static_linear_eigen` |
| The tile kernels compiled a second and third time with AVX and AVX-512F (no `fma` feature), chosen at run time; the portable instance (SSE2 / NEON) is the reference of the property tests | `simd.rs` | the same |
| `trsm_unit_lower`: the `gebp` updates run over tiles of rows, each lane with `gebp`'s even/odd accumulator sequence | `dense.rs` `gebp_minus` | 10 % (after the gemm) |
| Row-interleaved panel accumulator `dense[row·16 + c]`; the panel update scatters the L block row by row | `kernels.rs` | 19 % (`panel_bmod`) |
| Column-major GEMV (supernode solves, `column_bmod`, the triangular panels of `trsv`) over tiles of rows per column block, each lane with Eigen's per-row sequence; AVX / AVX-512F instances | `dense.rs` `gemv_tiles` | 31 % of `implicit_dynamic` (solves) |
| Work arrays and factor storage kept between factorisations (refilled; factor entries are written before they are read, as with Eigen's uninitialised `expand`); `SparseLu::solve_with` with a caller's scratch | `mod.rs`, `kernels.rs` | page faults 3–4 % |
| Factor arrays grow to the length in use (plus at most 4096 entries) instead of 1.5× zero-filled; `lsub` / `usub` as `i32` (Eigen's `int`) | `kernels.rs` | peak RSS |
| Sparse loops of the solves over index/value slices; `x[jcol]` loaded once (U rows lie above the supernode) | `mod.rs` | 15 % of `implicit_dynamic` |

Property tests (`dense::tests`): every instance this CPU runs against the
replaced kernels on random shapes and inputs with signed zeros,
infinities, NaNs and subnormals, bitwise (any NaN equal to any NaN: Rust
does not specify NaN payloads). `tests::reused_storage_gives_the_factors_of_a_fresh_factorisation`
checks that refactorising with kept storage (after a larger system and
after a failed factorisation) gives the factor arrays of a fresh run.

## Suite (5 repetitions, interleaved and rotated)

`series.sh`; raw data and full tables in `final-series/` (`solid.md`,
`models.md`). Ratio = after / before per repetition, median [min, max].
Every case has the same digest for both binaries and all repetitions.

| case | phase | before | after | ratio |
|---|---|---|---|---|
| static_linear_eigen | factorize (2 calls) | 5.934 s | 1.028 s | 0.174 [0.173, 0.179] |
| static_linear_eigen | back_substitution (20) | 205.7 ms | 130.3 ms | 0.638 [0.631, 0.644] |
| static_linear_eigen | analysis | 45.4 ms | 50.3 ms | 1.104 [1.087, 1.108] |
| static_linear_eigen | solve_step | 3.270 s | 0.872 s | 0.267 [0.264, 0.269] |
| static_newton_plastic | solve_step (12) | 3.094 s | 2.468 s | 0.799 [0.794, 0.803] |
| implicit_dynamic | solve_step (60) | 2.708 s | 2.315 s | 0.856 [0.848, 0.867] |
| cohesive_intrinsic_implicit | solve_step (30) | 10.164 s | 5.264 s | 0.518 [0.517, 0.524] |
| heat_implicit_hex8_kvar | solve_step (5) | 1.445 s | 0.818 s | 0.567 [0.566, 0.569] |
| heat_static_tri6 | solve_step | 4.745 s | 2.052 s | 0.432 [0.429, 0.438] |
| structural_beam3_frame_static | solve_step | 6.273 s | 1.406 s | 0.224 [0.223, 0.228] |
| structural_dkt18_plate_static | solve_step | 2.810 s | 1.022 s | 0.364 [0.360, 0.367] |
| phasefield_staggered_tri3 | solid_solve (4) | 12.097 s | 3.659 s | 0.303 [0.301, 0.303] |
| phasefield_staggered_tri3 | phase_solve (4) | 2.223 s | 1.067 s | 0.480 [0.480, 0.481] |
| phasefield_staggered_tet4 | solid_solve (4) | 2.392 s | 0.742 s | 0.310 [0.309, 0.311] |
| phasefield_staggered_tet4 | phase_solve (4) | 151.6 ms | 101.1 ms | 0.667 [0.660, 0.672] |
| contact_implicit_static_quad4 | solve_step (5) | 2.193 s | 1.331 s | 0.607 [0.606, 0.610] |
| embedded_rebar_tet4 | solve_step | 4.500 s | 1.289 s | 0.286 [0.282, 0.287] |

Two phases outside the solver are slower in `static_linear_eigen`, for
one reason: `analysis` (+4 ms) and `stiffness_assembly` (1.30 of before;
measured again after merging the sparse-assembly work: 44.5 → 96 ms over
3 calls). Neither runs solver code that changed: the assembly costs 96 ms
with both binaries in `static_linear_faer`, where no SparseLU runs, and
45 ms in every variant when glibc's thresholds are fixed
(`MALLOC_MMAP_THRESHOLD_=268435456 MALLOC_TRIM_THRESHOLD_=4294967296`:
base and branch, after Eigen or faer). The 51 ms are the assembly's own
arrays, allocated per call, mapped and page-faulted again each time. The
old SparseLU freed large blocks after each factorisation (its 1.5×
zero-filled factor arrays and its work arrays), which raises glibc's
dynamic mmap and trim thresholds, and the assembly that followed then
reused heap pages. The factorisation now keeps its storage for the next
one, so the thresholds stay at their defaults, as after a faer solve.
Dropping the kept storage (a probe build) does not bring it back either:
fewer, smaller blocks are freed. The fix belongs to the assembly (keep its
element arrays between calls, as the factorisation does); the phase costs
45 ms then, whatever solver ran before.

Peak RSS of the whole `static_linear_eigen` process (`/usr/bin/time`,
single runs during development): 436 MB with the 1.5× zero-filled growth,
339 MB with growth to the length in use, 283 MB with `int` subscripts.

## Against C++: the cohesive static solve with Eigen on both sides

`c1_series.sh` (cases `cppeig` and `rusteig` of `c1prep_campaign.sh`,
1 rank, 10 steps; C++ = the oracle's strict build); raw data `c1-eigen/`,
summary `c1-eigen/SUMMARY.txt` (`c1_summary.py`). `static_solve` is the
driver's phase (assembly, analysis, factorisation, solve). The energies of
the port and of C++ are bitwise identical in every pair ("bitwise 1").

| h | DOFs | C++ Eigen | port before | port after | after / before | after / C++ | peak RSS C++ / before / after |
|---|---|---|---|---|---|---|---|
| 0.12 (5 reps) | 14,472 | 2.92 s | 8.00 s | 2.36 s | 0.295 [0.294, 0.296] | 0.808 [0.804, 0.817] | 524 / 580 / 477 MiB |
| 0.06 (2 reps) | 97,551 | 248.5 s | 822 s | 209 s | 0.254 [0.253, 0.255] | 0.841 [0.839, 0.843] | 7.62 / 6.92 / 5.70 GiB |

At the branch head after merging the sparse-assembly work (6a8a69da, one
run, `c1-eigen-head/`): C++ 2.9 s, port 2.1 s (0.72), energies bitwise,
peak RSS 526 / 396 MiB.

h = 0.06 has two repetitions (a "before" run takes 14 minutes); the
spread of each variant is below 0.5 %.

## What is left (profile `profiles/static_linear_eigen.txt`, after)

- `gemm_avx512` 31 %: the dense update. The tile kernel alone reaches
  about 17–18 G multiply-adds/s on this CPU (60 % of the multiply/add
  peak without FMA; a 16 × 8 tile, two loads and eight broadcasts per
  sixteen multiplies and sixteen adds), about 12 G/s inside the
  factorisation (the `A` strips stream from L2/L3). The order per entry is
  fixed; the fast mode below uses FMA.
- `panel_bmod` 19 % (23 % in the 2-D `heat_static_tri6`): almost all of
  it is the scatter of the L block into the panel accumulator, whose rows
  are spread over `m × 16` doubles (1.3 MB at 10k DOFs, 6–10 MB in the
  2-D cases): cache misses. Eigen does the same scatter. A compact
  accumulator (row → slot map per panel) would keep it in L2; every
  kernel that touches the accumulator would change.
- `dfs_kernel` 7–12 %: the symbolic DFS, bound by the latency of random
  accesses to `marker`, `perm_r`, `repfnz`, `xsup`, `supno` (64-bit
  here, `int` in Eigen: converting them would halve their footprint).
- The rest is spread: `gebp_minus` 5 %, `copy_to_ucol` 2 %, the solves.

## Fast mode (not bitwise, off by default)

`SparseLu::set_fast_mode(Some(token))` (also on `EigenSparseLuSolver`; the token is `Exec::fast_token()` of an executor in precision `Fast`):
the panel products with fused multiply-adds (AVX-512F + FMA or AVX2 + FMA
instances, else the bitwise kernel). `static_linear_eigen`, factorisation
of two calls: 1.067 s → 0.982 s (×0.92, three alternated single runs, a
CI job running on the other four cores). Tolerance test
`tests::fma_mode_solution_within_tolerance`: relative 2-norm difference
of the solution ≤ 1e-12 on a diagonally dominant 3-D stencil (condition
number below 10). Larger fast-mode gains need another algorithm (a
supernodal BLAS-3 kernel with packing, another ordering).
