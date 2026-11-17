<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Threads and the remaining candidates of the Eigen SparseLU translation

Machine of every table: Google Cloud, AMD EPYC 9B14 (Zen 4, AVX-512),
8 cores, one thread per core, Ubuntu 24.04, nothing else running.
"Base" = `perf/wave2-base` at one thread, at the commit each section
names: 72337cb4 up to "Candidates measured and dropped", d404f4c70 in "On
the merged head", 7a6ad8f42 in the last two sections; "head" = this
branch. The thread count of every run is set with the environment variable
`AKANTU_NUM_THREADS`; the suites' `--threads` option, which only recorded
the count in the first sections, sets the executor's policy since the
merge with d404f4c70 (the runs of the later sections pass both).
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
did not move it. Identified later (last two sections): the placement of
the gemm tile's hot loop relative to the 64-byte fetch blocks, and in
`panel_bmod` a reload of the `L` block's base pointer from the stack in
the serial scatter loop; both fixed, one thread is now at the base's
speed.

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

## On the merged head (base d404f4c70: threaded assembly, explicit step, per-step state)

One executor story: the DOF manager's executor is the model's policy
(copied at creation, `set_exec_policy`), a DOF manager built without a
model starts with the process default, and the AIJ backend hands the DOF
manager's executor (`LinearSystemBackend::set_exec`) to the sparse solver
of every solve. Removed from this branch: `set_sparse_solver_exec` and its
call before each solve, and the process default set in `new_dof_manager`.

**One thread** (`--threads 1`, core 2, interleaved and rotated;
`one-thread/`): `static_linear_eigen` factorize 1.021 [1.018, 1.023]
(7 repetitions) and 1.015 [1.004, 1.044] (9); back-substitution 0.998;
`implicit_dynamic`, `cohesive_intrinsic_implicit` 0.997–1.002. Two
throwaway builds that differ from the head only by code that cannot run
in the measured phase (a suite line executed after it) give 1.005
[0.992, 1.022] for factorize and move `solve_step` by 1.6 %. On the host
cohesive driver (h = 0.12, static solve, `merged-head/hostab/`) the head
is 4.2 % slower than the base, and the profile puts the whole difference
in `gemm_avx512` (+8 % of its samples; same calls, same results, the
same code at the same offset of the function): the function starts at
16 mod 64 in the head and at 32 mod 64 in the base, so its hottest loop
spans the 64-byte fetch blocks differently. Built once with LLVM's loop
alignment forced to 64 bytes (`-C llvm-args=-align-loops=64`, diagnostic
only), base and head take 2.152 s and 2.164 s (0.6 % apart), between
the base's 2.11 s and the head's 2.19 s with the default placement. The
one-thread difference is therefore the placement of that loop, which a
change anywhere in the crate can move either way; stable Rust has no
function or loop alignment attribute, and unrolling the k loop by two
was slower (2.22 s). A per-crate loop-alignment flag would remove this
variance and is a build-flag decision for the owner.

**Scaling on the merged head** (short series, 3 repetitions for the suite,
`merged-head/tseries3.md`; cohesive 2 + 1 repetitions, `merged-head/c1m/`):

| case | head 1 | 2 | 4 | 8 threads (ratio to the base at one thread) |
|---|---|---|---|---|
| static_linear_eigen factorize | 1.020 | 0.871 | 0.790 | 0.749 |
| structural_beam3_frame_static solve_step | 1.006 | 0.800 | 0.684 | 0.614 |
| phasefield_staggered_tet4 solid_solve | 1.012 | 0.932 | 0.898 | 0.868 |
| heat_implicit_hex8_kvar solve_step | 1.005 | 0.955 | 0.944 | 0.932 |

| cohesive static solve | C++ | base | head 1 | 2 | 4 | 8 threads |
|---|---|---|---|---|---|---|
| h = 0.12 | 2.93 s | 2.11 s | 2.195 s | 1.74 s | 1.445 s | 1.31 s (0.45 of C++) |
| h = 0.06 | 249 s | 209 s | 227 s | 138 s | 84.8 s | 60.6 s (0.24 of C++) |

Energies byte-identical to C++ in all 18 runs. At h = 0.06 the one-thread
time is 8.6 % above the base: the gemm is a larger share there, so the
loop placement above weighs more.

## The gemm's hot loop on a 64-byte boundary (base 7a6ad8f42)

`simd::align_next_loop` (inline assembly `.p2align 6` and 16 NOP bytes,
x86-64, before the k loop of the AVX-512 gemm tile): the alignment
directive also raises the function's section to 64 bytes, and the NOP
bytes compensate the 0x70-byte preheader the compiler places after it.
Disassembly (`loop-alignment/`): `gemm_avx512` at 0x804740 in
`suite_solid` and at 0x884cc0 in `cohesive_extrinsic` (both 0 mod 64),
loop head at 0x804b80 and 0x885100 (both 0 mod 64); the base's
`cohesive_extrinsic` has it at 0x8c1810 (16 mod 64), loop head 0x8c1bf0
(48 mod 64). With the directive alone (no NOP bytes) the loop head sat at
48 mod 64 in every build and the static solve took the slow time (2.195 s
against 2.10 s).

One thread against the base 7a6ad8f42 (interleaved, rotated, core 2):

| case | head / base, median [min, max] |
|---|---|
| static_linear_eigen factorize (7 reps) | 1.006 [0.975, 1.020] |
| static_linear_eigen back_substitution | 0.996 [0.986, 1.014] |
| static_linear_eigen solve_step | 1.012 [0.991, 1.021] |
| heat_static_tri6 solve_step | 1.017 [0.994, 1.025] |
| structural_beam3_frame_static solve_step | 1.009 [1.003, 1.023] |
| structural_dkt18_plate_static solve_step | 1.018 [1.008, 1.028] (1.027 in the previous round, without the directive) |

Host cohesive static solve (`timing_ab.sh`, `loop-alignment/alc/`):

| h | base | head | head / base per repetition |
|---|---|---|---|
| 0.12 (5 reps) | 2.178 s | 2.112 s | 0.968 [0.961, 0.975] |
| 0.06 (2 reps) | 226.1 s | 214.5 s | 0.948 [0.947, 0.950] |

These cohesive ratios below 1 are not a gain of the threaded code (one
thread runs the serial path): the base binary of 7a6ad8f42 happens to have
the slow placement of the gemm loop (loop head at 48 mod 64), and the
directive gives the head the fast one. They measure that placement
effect; the head no longer depends on where the linker puts the function.

## Fix round: the serial scatter and the directive per loop (base 7a6ad8f42)

- The serial product and `L` scatter of `panel_bmod` moved to an
  out-of-line helper (`PanelUpdate::run_serial`): nothing of the caller is
  live in the scatter loop, whose inner loop now keeps every pointer in a
  register (it reloaded the `L` block's base from the stack before).
- That helper alone left `structural_dkt18_plate_static` at 1.008
  [1.000, 1.015] (`fix-round/t3m`): its inner loop sat at 48 mod 64 in
  `suite_models`, across two fetch blocks (0 mod 64 in `suite_solid`). The
  directive now takes a NOP count per loop (`align_next_loop::<NOPS>`):
  16 before the k loop of the bitwise AVX-512 16 × 8 gemm tile only (no
  longer in the 16 × 4 or the FMA instances), 48 before the inner scatter
  loop (its padding runs once per `L` row). Three-way series
  (`fix-round/t4m`, `t4s`): helper only 1.005, helper and aligned
  scatter 0.995.
- `loop_alignment_check.py BINARY ...` prints the two loop heads and
  their offset mod 64 and fails when one is not 0 (the base: gemm loop at
  32, no `run_serial`). **Run it on `suite_solid` and `suite_models` after
  every toolchain bump**: the NOP counts compensate this rustc's (1.99.1)
  preheaders.

One thread against the base 7a6ad8f42 (7 repetitions, interleaved and
rotated, core 2; head 988a8c22, `fix-round/t5m`, `t5s`; digests the same):

| case | phase | base | head | head / base, median [min, max] |
|---|---|---|---|---|
| structural_dkt18_plate_static | solve_step | 907.5 ms | 900.3 ms | 0.998 [0.990, 1.013] |
| static_linear_eigen | factorize (2) | 1.027 s | 1.016 s | 0.989 [0.966, 1.001] |
| static_linear_eigen | back_substitution (20) | 128.3 ms | 127.8 ms | 0.997 [0.986, 1.004] |
| static_linear_eigen | solve_step | 713.0 ms | 702.8 ms | 0.987 [0.944, 1.011] |
