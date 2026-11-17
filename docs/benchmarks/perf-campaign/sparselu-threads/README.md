<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Threads and the remaining candidates of the Eigen SparseLU translation

Machine of every table: Google Cloud, AMD EPYC 9B14 (Zen 4, AVX-512),
8 cores, one thread per core, Ubuntu 24.04, nothing else running.
"Base" = `perf/wave2-base` 72337cb4 at one thread; "head" = this branch.
The thread count of every run is set with the environment variable
`AKANTU_NUM_THREADS` (the suites' `--threads` option is only recorded);
`T` threads run pinned to the cores `0..T−1`. A check with `ps -L`
during a run: 4 threads with `AKANTU_NUM_THREADS=4`, 1 with `1`.

## What changed

- **Threads (bitwise at every thread count).** The AIJ backend hands the
  DOF manager's executor (`LinearSystemBackend::set_exec`, the model's
  policy) to the sparse solver of every solve
  (`SparseDirectSolver::set_exec`); a DOF manager built without a model
  starts with the process default policy; `SparseLu::set_exec` for a
  solver outside a model. (Before the merge with the threaded assembly,
  this branch had its own `set_sparse_solver_exec` and made
  `new_dof_manager` set the process default: both removed.) A panel update of at least 2¹⁶ multiply-adds runs in
  two regions: the product over strips of whole 16-row tiles (each part
  writes its own contiguous chunk of `L`), then the scatter over blocks
  of accumulator rows (each part applies the `L` rows that land in its
  block). Every entry gets the operations of the serial code, so the
  factors do not depend on the split. Threshold: 2¹⁴ to 2¹⁸ within
  1.5 % of each other at 2, 4 and 8 threads, 2¹⁶ the best, 2²⁰ 6–14 %
  slower.
- **One SIMD dispatch.** The dense kernels take the executor's
  `SimdToken` (`Avx512` → the AVX-512F instances, `Avx2` → the AVX ones,
  `Scalar` / `Neon` → portable); the fast mode uses the `FastToken`'s
  level. The former `OnceLock` and `AKANTU_SPARSELU_ISA` are gone;
  `AKANTU_SIMD=scalar` selects the portable code (same digests).
  Nothing in `akantu-core/src/exec/simd.rs` was needed beyond what it has
  (`SimdLevel`, `SimdToken::level`, `FastToken::simd`).
- **Fast mode** (FAST MODE, precision `Fast` of the executor or an
  explicit token): fused multiply-adds also in the triangular panel solves
  and the GEMV kernels; rule `fast_mode/linalg/sparse_lu_stencil`
  (`fixtures/fast_mode/linalg/tolerances.json`).

## Thread invariance

- `tests/eigen_sparse_lu_oracle.rs`: every oracle fixture factorisation
  and solve also at 2, 3, 4 and 8 threads, automatic and finest grain,
  with every panel update threaded (threshold 0): bit for bit the
  oracle's arrays.
- `tests::factors_are_thread_invariant`: factor arrays, permutations and
  solutions at 1/2/3/4/8 threads × grains × thresholds on 2-D and 3-D
  stencils.
- Suites, size `m`, ten solver cases × 1/2/3/4/8 threads × automatic /
  finest grain, every digest the base's at one thread
  (`thread_invariance.sh`, `thread_invariance_digests.txt`).
- Mutants (each made by hand, the tests run, the code restored): the
  block range test `r >= hi` → `r > hi` (a boundary row applied twice),
  the strip decode `i >= bounds[p + 1]` → `i > bounds[p + 1]`, and the
  product's depth split in two halves added afterwards (an accumulation
  split across work units): all three fail the unit and the oracle
  tests. Moving a strip boundary by one row (still a valid split) passes,
  as it must.

## Suite: threads against the base at one thread

`series.sh`, 5 repetitions, the variants of a case back to back in a
rotated order; ratio = head / base per repetition, median [min, max];
full table `suite-series/summary.md`, raw data `suite-series/`.

| case | phase | base 1 thread | head 1 | head 2 | head 4 | head 8 |
|---|---|---|---|---|---|---|
| static_linear_eigen | factorize (2) | 1.027 s | 1.030 [1.022, 1.046] | 0.878 [0.872, 0.878] | 0.791 [0.788, 0.795] | 0.753 [0.747, 0.757] |
| static_linear_eigen | back_substitution (20) | 130.8 ms | 1.001 | 0.999 | 1.000 | 1.005 |
| static_linear_eigen | solve_step | 746 ms | 1.018 [0.993, 1.023] | 0.907 | 0.855 | 0.814 |
| heat_implicit_hex8_kvar | solve_step | 592 ms | 1.005 | 0.993 | 0.994 | 1.006 |
| structural_beam3_frame_static | solve_step | 1.356 s | 1.007 | 0.789 | 0.678 | 0.615 |
| structural_dkt18_plate_static | solve_step | 916 ms | 1.010 | 1.004 | 0.997 | 0.996 |
| phasefield_staggered_tri3 | solid_solve (4) | 3.059 s | 1.013 [0.990, 1.031] | 0.944 | 0.915 | 0.906 |
| phasefield_staggered_tri3 | phase_solve (4) | 824 ms | 1.000 | 0.994 | 0.985 | 0.987 |
| phasefield_staggered_tet4 | solid_solve (4) | 579 ms | 1.009 [0.994, 1.020] | 0.956 | 0.925 | 0.919 |
| phasefield_staggered_tet4 | phase_solve (4) | 69.8 ms | 0.996 | 1.000 | 1.002 | 1.011 |

What limits the scaling: Amdahl. At one thread the threaded part
(product and L scatter of the panel updates above the threshold) is
about 45 % of a 3-D factorisation; the symbolic DFS, the U-block solves,
the column updates, the pivoting and every small update stay serial
(a profile at 4 threads: 52 % of the samples are idle workers). The 2-D
problems and the heat cases have small supernodes (few updates above the
threshold): no gain. The triangular solves are not threaded (they read
the factors once: memory-bound; no gain measured worth a region per
supernode). Beyond 4 threads little remains.

One thread: within ±1 % on every case except `static_linear_eigen`
factorize, +3.0 % [2.2, 4.6]. Bisection: the threaded update and the SIMD
unification left it at the base's speed (f08639ce: 1.019 s against
1.029 s); the fast-mode commit added about 2.5 %, of which the restored
bitwise triangular solve, the bitwise product in `kernel_bmod` and the
out-of-line fast entry points recovered part. The code of the hot
kernels is identical (`gemm_avx512` disassembles the same), the slowdown
is spread over unchanged kernels, and two builds with a padding function
did not move it: not identified (open problem).

## Against C++: the cohesive static solve, Eigen on both sides

`c1_threads.sh` (cases `cppeig` / `rusteig` of `c1prep_campaign.sh`, one
rank; the port's threads with `RUST_THREADS` → `AKANTU_NUM_THREADS`, its
cpuset `np × threads` cores; C++ has no threads in this solver).
`static_solve` median [min, max]; energies byte-identical to C++ in every
run (all thread counts). Raw data `c1-eigen/`, summary
`c1_threads_summary.py`.

| h | C++ | base 1 thread | head 1 | head 2 | head 4 | head 8 |
|---|---|---|---|---|---|---|
| 0.12 (3 reps) | 2.94 s | 2.12 s (0.721 of C++) | 2.10 s (0.714) | 1.71 s (0.582) | 1.46 s (0.497) | 1.35 s (0.459) |
| 0.06 (1 rep) | 251 s | 209 s (0.833) | 210 s (0.837) | 133 s (0.530) | 82.1 s (0.327) | 58.1 s (0.231) |

Peak RSS: h = 0.12 C++ 524 MiB, port 383–397 MiB; h = 0.06 C++ 7.62 GiB,
port 5.16–5.18 GiB, at every thread count. At h = 0.06 the supernodes
are large (97,551 unknowns): 8 threads take 0.28 of the base's time.

## Candidates measured and dropped (one thread)

| candidate | result |
|---|---|
| compact panel accumulator (row → slot map per panel, assigned on first touch) | factorize +10 %, `heat_static_tri6` +17 %, `phasefield_staggered_tri3` +11 %: the slot lookup on every access costs more than the smaller accumulator saves |
| `i32` work arrays (`marker`, `repfnz`, `panel_lsub`) | no measurable change (±1 %); memory −140 bytes per row; not kept |
| packing the A strips of the AVX-512 gemm | +15 % / +11 % (a strip is read by at most two column tiles and stays in L1); a stack buffer zero-filled per strip +55 %; a thread-local buffer compiles its closure without AVX-512 |
| candidate 5 (`analysis` +4 ms) | not chased (the assembly task keeps its arrays) |

## Fast mode: numbers

- FMA in the panel products, the triangular panel solves and the GEMV
  (precision `Fast`): `static_linear_eigen` factorize 1.03 s → 0.966 s
  (−6 %), single runs.
- A packed BLAS-3 style supernodal kernel with FMA: the tile kernel
  reaches 17.7 G multiply-adds/s alone without FMA (about 60 % of the
  peak of separate multiplies and adds); with FMA the peak doubles, so the gemm's share (31 % of a 3-D factorisation) would at
  best halve (factorisation ×0.85), less in practice since the kernel
  reaches 12 G/s inside the factorisation (memory).
- **Another ordering** (`examples/nested_dissection_fill.rs`, a geometric
  nested dissection by recursive bisection against COLAMD, on 3-D grid
  matrices with hexahedral connectivity; `nested_dissection_fill.txt`):

| grid, unknowns per node | unknowns | nnz(L+U) ND / COLAMD | multiply-adds | factorisation time |
|---|---|---|---|---|
| 12³ cells, 1 | 2,197 | 0.730 | 0.508 | 0.527 |
| 30³, 1 | 29,791 | 0.295 | 0.098 | 0.079 (12.1 s → 0.96 s) |
| 40³, 1 | 68,921 | 0.272 | 0.089 | 0.084 (66.0 s → 5.5 s) |
| 14³, 3 | 10,125 | 0.742 | 0.566 | 0.466 |
| 20³, 3 | 27,783 | 0.631 | 0.513 | 0.340 |
| 24³, 3 | 46,875 | 0.527 | 0.321 | 0.242 (25.9 s → 6.3 s) |

  The gap grows with the problem: COLAMD's fill on 3-D meshes grows much
  faster than nested dissection's. The factors differ from Eigen's
  (another permutation), so it is a fast-mode or an explicit option; an
  implementation needs a graph partitioner (METIS-like, or a geometric
  one where coordinates are known) and is the largest gain left for big
  3-D implicit problems.
