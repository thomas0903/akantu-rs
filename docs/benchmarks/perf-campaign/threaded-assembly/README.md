<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Matrix assembly and implicit matrix operations on threads

Branch `perf/threaded-assembly` from `perf/wave2-base` (b3f6c78aa). Every
result keeps every bit at every thread count (D-122, D-123): the same
binary64 operations in the same order into every output.

Machine of every table: Intel Xeon Platinum 8581C (Emerald Rapids), 8
cores, one thread per core, 30 GiB, Ubuntu 24.04 (a development machine
used alone). One process per run; a run with `N` threads is pinned to
cores `0..N-1` (`AKANTU_NUM_THREADS=N taskset -c 0-(N-1)`,
`scripts/perf/exec/wrappers.sh`), the base to core 0.

## What runs on threads

| Where | Before | After |
|---|---|---|
| elemental stiffness `∫ BᵀDB` (solid small strain, heat conductivity) | `compute_btdb` into a per-point array (`ne·nq·n²` reals: 187 MB for 162,000 four-node tetrahedra, 510 MB for 13,824 hexahedra), then `integrate`, both serial | `FeEngine::compute_btdb_integrated`: per element the same products and the same integration, on the FE engine's executor, without the per-point array |
| consistent mass (`field_matrix_elemental`) | Jacobians of all elements, then the elements in order | Jacobians, point matrices and integrals per part of elements on the executor; the integral by the fixed loop of the GEMV route when the general product takes it (same additions) |
| solid stiffness arrays | gradient, tangent and elemental matrices allocated at every assembly | kept between assemblies (`StiffnessScratch`; a fresh array of that size costs one page fault per 4 KiB at every assembly) |
| scatter of a repeated assembly (cached entry positions; the first assembly, which builds the profile, stays serial) | element by element | equations checked (and missing slots resolved) in parallel, then entry ranges: each thread adds, in element order, the contributions whose position lies in its range |
| `zero`, `J += c·K` (same entries), `copy_profile` of `J`, Dirichlet rows | serial loops | entry ranges on the DOF manager's executor (parts of at least 32,768 entries) |
| `mat_vec_mul` (serial layouts) | one pass over the entries, two scattered updates per off-diagonal entry | rows on threads, each row adding its terms in storage order from a row-gathered index built once per entry positions (`RowGather`, 8 bytes per term, about 16 bytes per stored entry of a symmetric matrix) |
| heat, phase-field, structural models | serial | the process default policy and `exec_policy` / `set_exec_policy` (their DOF manager; the FE engines of the heat and phase-field models) |

Why each is bitwise:

- An elemental matrix is an independent output: the threaded kernels
  compute each element with the code of the serial path (the products
  are `SliceProduct::eval` with the operand descriptions of
  `compute_btdb`, the integral that of `integrate`).
- The value of an entry is the sum of its contributions in element order.
  With entry ranges every entry is written by one thread, which visits the
  elements in order; the order of the contributions does not depend on the
  ranges or the thread count. Colouring or per-thread partial matrices
  would change that order.
- A row of the product receives, in the serial pass, the terms of the
  entries in storage order; the gathered index lists exactly those terms
  in that order.

## Tests

| test | what |
|---|---|
| `akantu-linalg` `sparse::perf_tests::threaded_batch_assembly_is_add_values` | the batch scatter (or the serial loop when it declines) against `add_values`, 5 matrix/block/policy combinations × 12 trials × 7 rounds × 1, 2, 3, 4, 8 threads: values, errors, releases (all slots, a subset, a repeated slot, changed equations, appended entries, rebuilt profile, missing entries, resize) |
| `…::threaded_batch_resolves_like_the_serial_loop` | stored positions and resolve counter equal to the serial loop's |
| `…::threaded_matrix_operations_match_the_serial_ones` | products (both reductions, an entry appended between products), Dirichlet rows, sums, profile copy, scaling, zeroing |
| `akantu-fe` `tests/threaded_elemental_matrices.rs` | fused `∫ BᵀDB` = `compute_btdb` + `integrate` (every element type, order 2 and 4, with and without a filter, the errors), consistent mass, at 1 to 8 threads, finest and automatic grain; `engine::assembly::tests::gemv_integral_is_the_general_product` |
| `akantu-solver` `tests/threaded_assembly.rs` | DOF manager: two materials on one type and a second type, symmetric and unsymmetric storage, 6 rounds, appended couplings, `J = copy_profile(K) + c M + d K`, Dirichlet rows, products, the number of threaded assemblies |
| `akantu-solid` `tests/exec_threads_matrices.rs` | static Newton with an elastic law and `neohookean` (not opted in, tangent reassembled at every iteration) on tetrahedra and hexahedra with prescribed Dirichlet values; implicit dynamics with the consistent mass; repeated stiffness assemblies of two laws: `K`, `M`, `J`, nodal fields, laws' fields |
| `akantu-diffusion` `tests/exec_threads.rs` | backward Euler with a temperature-dependent conductivity, consistent capacity, triangles and quadrangles, Dirichlet conditions |
| `akantu-structural` `tests/exec_threads.rs` | a frame of beams: repeated stiffness assemblies, static solve, Newmark steps with the consistent mass |

Mutants checked to fail them: a range boundary off by one (an element's
last range skipped), the blocks of a range visited in reverse order
(entries scattered out of element order), the terms of a product row in
reverse order.

## Per-call arrays and the allocator (one thread)

The assembly used to allocate its element arrays for the whole type at
every call (elemental matrices, per-point `BᵀDB`, tangent, gradient; for
the mass, the elemental matrices and the density values). Arrays above
glibc's mmap threshold are mapped afresh and page-faulted at every call;
when an earlier phase had freed large blocks, glibc's dynamic thresholds
were raised and the cost hid. The head keeps these arrays in the solid
model between assemblies (`StiffnessScratch`: one pool of elemental
arrays shared by the stiffness and the mass, the tangent, the gradient
and the density values; released by `release_static_solver`) and no
longer builds the per-point `BᵀDB` array.

Series: `scripts/perf/suite/run_ab.sh --size m --reps 5 --core 0`, four
variants rotated per repetition, all at one thread on core 0: `base`
(`perf/wave2-base` 72337cb4e) and `head` (105ea0eb1), each also with
`MALLOC_MMAP_THRESHOLD_=268435456 MALLOC_TRIM_THRESHOLD_=4294967296`
(`*_env`; wrapper: `#!/bin/sh` + `[env] AKANTU_NUM_THREADS=1 exec taskset
-c 0 BINARY "$@"`). 2026-11-16, raw data `alloc/`, full table
`alloc-summary.md`. Every digest equal across the four variants and the
five repetitions.

| case | phase (calls) | base | base_env | head | head_env | head / base |
|---|---|---|---|---|---|---|
| `static_linear_eigen` | `stiffness_assembly` (3) | 78.1 ms | 45.0 ms | 40.8 ms | 42.0 ms | 0.524 [0.515, 0.539] |
| `setup_tet4` | `stiffness_assembly` (3) | 1.075 s | 0.707 s | 0.517 s | 0.519 s | 0.481 [0.474, 0.491] |
| `setup_tet4` | `mass_consistent` (3) | 613.6 ms | 456.0 ms | 372.8 ms | 372.2 ms | 0.608 [0.598, 0.618] |
| `setup_tet10` | `stiffness_assembly` (3) | 915.1 ms | 854.0 ms | 349.5 ms | 350.2 ms | 0.383 [0.378, 0.388] |
| `setup_tet10` | `mass_consistent` (3) | 244.2 ms | 188.5 ms | 186.3 ms | 180.6 ms | 0.776 [0.722, 0.793] |
| `setup_hex8` | `stiffness_assembly` (3) | 1.471 s | 1.417 s | 0.574 s | 0.577 s | 0.390 [0.387, 0.393] |
| `setup_hex8` | `mass_consistent` (3) | 318.2 ms | 264.7 ms | 247.3 ms | 247.3 ms | 0.779 [0.747, 0.805] |
| `implicit_dynamic` | `mass_consistent` (1) | 56.4 ms | 61.9 ms | 51.6 ms | 58.0 ms | 0.868 [0.791, 1.186] |

Medians over 5 repetitions ([min, max] in `alloc-summary.md`); ratio =
head / base per repetition, median [min, max]. Without any environment
setting the head is at or below the base with the larger thresholds, and
the two settings no longer differ for the head (inside the spread). The
one-shot mass of `implicit_dynamic` (its arrays are new in every run)
does not gain from the reuse; its spread contains 1.

## Tried, did not pay

- **First assembly (profile construction) on threads.** Every part of the
  elements listed, in order, the pairs whose key was new to the matrix and
  to the part; the parts were merged in order (serial), which reproduces
  the serial profile entry for entry (property-tested, mutants failed);
  then positions and the entry-range scatter. It does three hash
  operations per pair against one in the serial fused loop
  (`add_block_to_profile_and_values`). Against the same branch without
  it (3 rounds, this machine): `setup_tet4` `stiffness_assembly` 122.5
  against 122.1 ms at 8 threads, 186.2 against 184.4 ms at 4;
  `static_linear_eigen` `stiffness_assembly` equal at 2, 4, 8 threads; the
  static solve of `cohesive_intrinsic_implicit`, whose profile is built
  again at every Newton iteration, 3.33 against 3.16 s at 2 threads and
  3.27 against 3.11 s at 4. It did pay for a one-shot assembly of a
  whole mesh: the consistent mass of `implicit_dynamic` (assembled once)
  took 31.0 ms at 8 threads and 37.2 ms at 4 with it (first series,
  commit 105ea0eb1) against 57.3 and 58.5 ms without it (final series).
  Removed; the first assembly stays serial. A form whose temporaries (the
  equations, positions and pair lists of the whole batch) are kept, or a
  row-wise profile index, may keep the gain without the loss.
- **Entry-wise matrix operations kept serial below 4 M entries** (to keep
  `J` in the cache of the core that factorises it): no change on
  `cohesive_intrinsic_implicit` and `structural_dkt18_plate_static` at 2
  threads (3.39 against 3.37 s, 0.983 against 0.984 s); kept threaded.
- **Workers parked at once** (`AKANTU_EXEC_SPIN_US=0`) against the default
  spin: no change on the same two cases (the slowdown measured there came
  from the threaded first assembly above).

## Threads: the implicit and set-up cases

Series: `scripts/perf/threaded-assembly/series.sh BIN_DIR OUT 5` (size
`m`, 5 repetitions, the five variants rotated per repetition): `base` =
`perf/wave2-base` 72337cb4e at one thread on core 0; `t1`…`t8` = the
head with `N` threads on cores `0..N-1`. **The thread count was set with
`AKANTU_NUM_THREADS=N` in the wrappers** (`scripts/perf/exec/wrappers.sh`);
the suites' `--threads` option did not set it before commit 2779476a0,
so the `threads` field of these JSON lines reads 1 for every variant.
Head: ea071de35 for `series2/` (2026-11-16, all cases); the three cases
whose laws assemble their own blocks (`cohesive_intrinsic_implicit`,
`embedded_rebar_*`) were measured again after the pool fix (head
f28c5a7bc, "Stiffness scratch:
only arrays taken from the pool return to it", `series3/`), and those
rows come from `series3/`. Ratio = head / base per repetition, median
[min, max]; `base` = median seconds. Every digest equal across the five
variants and the repetitions (column `digest` = `same` in every row of
`series2-*-summary.md` and `series3-*-summary.md`).

| case | phase (calls) | base | 1 thread | 2 threads | 4 threads | 8 threads |
|---|---|---|---|---|---|---|
| `setup_tet4` | `stiffness_assembly` (3) | 1.100 s | 0.492 [0.485, 0.497] | 0.279 [0.277, 0.289] | 0.167 [0.166, 0.169] | 0.109 [0.109, 0.113] |
| `setup_tet4` | `mass_consistent` (3) | 649.7 ms | 0.591 [0.587, 0.599] | 0.345 [0.341, 0.349] | 0.205 [0.202, 0.209] | 0.141 [0.140, 0.144] |
| `setup_tet10` | `stiffness_assembly` (3) | 970.6 ms | 0.382 [0.369, 0.395] | 0.207 [0.199, 0.211] | 0.107 [0.103, 0.110] | 0.057 [0.056, 0.058] |
| `setup_tet10` | `mass_consistent` (3) | 310.8 ms | 0.678 [0.667, 0.716] | 0.362 [0.346, 0.368] | 0.179 [0.177, 0.188] | 0.103 [0.099, 0.105] |
| `setup_hex8` | `stiffness_assembly` (3) | 1.529 s | 0.390 [0.385, 0.393] | 0.208 [0.206, 0.212] | 0.111 [0.110, 0.113] | 0.061 [0.060, 0.062] |
| `setup_hex8` | `mass_consistent` (3) | 372.4 ms | 0.726 [0.719, 0.743] | 0.388 [0.379, 0.403] | 0.206 [0.203, 0.214] | 0.112 [0.111, 0.117] |
| `static_linear_eigen` | `stiffness_assembly` (3) | 88.3 ms | 0.520 [0.486, 0.540] | 0.275 [0.275, 0.303] | 0.159 [0.153, 0.167] | 0.105 [0.101, 0.112] |
| `static_linear_eigen` | `solve_step` (1) | 787.4 ms | 0.965 [0.938, 0.981] | 0.950 [0.934, 0.958] | 0.953 [0.938, 0.959] | 0.962 [0.931, 0.968] |
| `static_newton_plastic` | `solve_step` (12) | 1.970 s | 0.984 [0.977, 0.985] | 0.930 [0.919, 0.943] | 0.930 [0.928, 0.951] | 0.929 [0.920, 0.931] |
| `implicit_dynamic` | `mass_consistent` (1) | 66.9 ms | 0.983 [0.932, 1.030] | 0.899 [0.885, 0.925] | 0.878 [0.845, 0.916] | 0.859 [0.825, 0.890] |
| `implicit_dynamic` | `residual` (180) | 203.3 ms | 1.014 [1.004, 1.029] | 0.557 [0.553, 0.559] | 0.368 [0.362, 0.369] | 0.293 [0.290, 0.307] |
| `implicit_dynamic` | `solve_step` (60) | 1.197 s | 0.984 [0.931, 1.082] | 0.902 [0.754, 0.915] | 0.746 [0.710, 0.855] | 0.740 [0.690, 0.799] |
| `cohesive_intrinsic_implicit` | `solve_step` (30) | 3.240 s | 0.995 [0.982, 1.006] | 0.981 [0.979, 1.031] | 0.952 [0.949, 1.024] | 0.998 [0.937, 1.010] |
| `heat_static_tri6` | `assemble_conductivity` (5) | 46.2 ms | 0.845 [0.768, 0.865] | 0.455 [0.437, 0.468] | 0.301 [0.295, 0.305] | 0.212 [0.207, 0.214] |
| `heat_static_tri6` | `solve_step` (1) | 2.066 s | 0.994 [0.989, 1.004] | 0.990 [0.977, 0.991] | 0.982 [0.975, 1.000] | 0.982 [0.976, 0.983] |
| `heat_implicit_hex8_kvar` | `solve_step` (5) | 583.4 ms | 0.982 [0.979, 1.008] | 0.957 [0.940, 0.972] | 0.945 [0.926, 0.985] | 0.935 [0.928, 0.948] |
| `phasefield_staggered_tri3` | `solid_assemble_stiffness` (5) | 74.0 ms | 0.836 [0.791, 0.926] | 0.463 [0.423, 0.485] | 0.304 [0.298, 0.312] | 0.238 [0.235, 0.252] |
| `phasefield_staggered_tri3` | `phase_assemble_stiffness` (5) | 62.3 ms | 0.947 [0.874, 0.981] | 0.680 [0.657, 0.711] | 0.622 [0.610, 0.644] | 0.600 [0.595, 0.628] |
| `phasefield_staggered_tri3` | `solid_solve` (4) | 3.503 s | 0.987 [0.970, 0.992] | 0.969 [0.954, 0.971] | 0.961 [0.942, 0.970] | 0.963 [0.959, 0.970] |
| `phasefield_staggered_tet4` | `solid_assemble_stiffness` (5) | 48.3 ms | 0.516 [0.502, 0.526] | 0.300 [0.288, 0.307] | 0.203 [0.197, 0.210] | 0.137 [0.131, 0.141] |
| `phasefield_staggered_tet4` | `solid_solve` (4) | 647.0 ms | 0.957 [0.950, 0.975] | 0.925 [0.904, 0.939] | 0.918 [0.913, 0.922] | 0.910 [0.905, 0.922] |
| `embedded_rebar_tri3` | `assemble_stiffness` (5) | 47.5 ms | 0.864 [0.785, 0.929] | 0.697 [0.644, 0.722] | 0.592 [0.575, 0.634] | 0.561 [0.537, 0.582] |
| `embedded_rebar_tet4` | `assemble_stiffness` (5) | 166.9 ms | 0.583 [0.570, 0.713] | 0.365 [0.314, 0.382] | 0.243 [0.204, 0.250] | 0.188 [0.154, 0.190] |
| `embedded_rebar_tet4` | `solve_step` (1) | 1.087 s | 0.996 [0.981, 1.053] | 0.988 [0.974, 1.034] | 0.988 [0.974, 1.026] | 0.990 [0.975, 0.997] |
| `contact_implicit_static_quad4` | `solve_step` (5) | 825.5 ms | 0.992 [0.950, 1.024] | 0.944 [0.914, 0.972] | 0.907 [0.889, 0.954] | 0.916 [0.879, 0.932] |
| `structural_beam3_frame_static` | `assemble_stiffness` (5) | 35.0 ms | 0.960 [0.905, 0.980] | 0.935 [0.893, 0.975] | 0.947 [0.896, 0.959] | 0.937 [0.919, 0.953] |
| `structural_beam3_frame_static` | `solve_step` (1) | 1.233 s | 1.001 [0.993, 1.011] | 1.007 [0.986, 1.022] | 1.006 [0.986, 1.013] | 1.003 [0.996, 1.014] |
| `structural_beam3_frame_dynamic` | `solve_step` (60) | 3.740 s | 0.992 [0.989, 1.006] | 0.990 [0.988, 0.998] | 0.988 [0.979, 0.998] | 0.989 [0.985, 0.996] |
| `structural_dkt18_plate_static` | `assemble_stiffness` (5) | 308.0 ms | 0.948 [0.937, 0.962] | 0.947 [0.921, 0.987] | 0.935 [0.902, 0.961] | 0.920 [0.910, 0.943] |
| `structural_dkt18_plate_static` | `solve_step` (1) | 974.6 ms | 1.008 [0.992, 1.022] | 1.002 [0.984, 1.005] | 1.006 [1.001, 1.008] | 1.003 [0.988, 1.009] |

The absolute times of `series2` are 5–25 % above those of the first
series and of the allocator series on the same machine for the same
binaries (`base` `setup_hex8` `mass_consistent` 372 against 314–318 ms):
the machine is a cloud VM whose speed varied between sessions; the
paired ratios of a series are taken within each repetition and agree
between sessions (`setup_*` `stiffness_assembly` at one thread: 0.39 /
0.38 / 0.49 in both).

One thread: every assembly phase is faster than the base (the fused
`∫ BᵀDB` without the per-point array, the kept arrays). No phase is
slower outside its spread except `implicit_dynamic` `residual` 1.014
[1.004, 1.029] (one row; 3 ms over 180 calls; the residual's product
`K·u` takes the same serial code at one thread) and
`structural_beam3_frame_static` `solve_step` in the first series
(1.007 [1.005, 1.020]; 1.001 [0.993, 1.011] in the final one): solver
phases whose code this branch does not change at one thread.

## What limits the scaling

- **The sparse solver.** Every implicit step is led by the SparseLU
  factorisation and solves (serial, another task): `static_linear_eigen`
  spends 1.08 s in `factorize` against 9 ms in the assembly at 8
  threads; the steps gain 2–26 %, the assemblies 2–18×.
- **Serial parts of an assembly**: the first assembly of a matrix
  (profile construction, serial; see below), the law's tangent for laws
  that did not opt in, the density functor of the mass (`fill_field`, a
  user closure), the element list and the validation of the equations
  in the DOF manager, and the per-call set-up of a region. `setup_tet4`
  (1-point tetrahedra, little work per element) stops at 9.2× for the
  stiffness and 7.1× for the mass; `setup_tet10` / `setup_hex8` (more
  work per element) reach 17–18× and 9–10×.
- **Elemental matrices that stay serial**: the structural engine
  (`StructuralFeEngine::compute_btdb` / `compute_ntbn`) and the
  phase-field elemental matrices (`compute_btdb` order 2 plus
  `compute_ntbn`, both in `ops.rs`): `structural_*` `assemble_stiffness`
  gains 5–8 % (the scatter only), `phase_assemble_stiffness` 1.7× at 8
  threads.
- **Memory traffic** of the scatter for large meshes: per entry a 4-byte
  position, the value and a read-modify-write of the entry, in element
  order (the prototype measured 3.6× at 8 threads on 750,000 tetrahedra).

## Raw data

| directory | what |
|---|---|
| `alloc/`, `alloc-summary.md` | the allocator series (one thread, with and without the glibc thresholds) |
| `series1/`, `series1-*-summary.md` | the first thread series, head 105ea0eb1 (with the threaded first assembly that was removed afterwards) |
| `series2/`, `series2-*-summary.md` | the final thread series, head ea071de35 |
| `series3/`, `series3-*-summary.md` | the three cases with laws that assemble their own blocks, head f28c5a7bc (pool fix) |
| `experiments/fa/` | threaded first assembly against the same branch without it (`head-*` / `exp2-*`, 3 rounds) |
| `experiments/spin*/`, `experiments/emb/` | the two-thread slowdown hunt (spin time, entry-wise operations, base at two threads, without the first assembly) and the pool-fix check of `embedded_rebar_tri3` |

Every JSON line of these series records `"threads":1`: the thread count
was set through `AKANTU_NUM_THREADS` by the wrappers, the suites'
`--threads` option was not applied before commit 2779476a0.
