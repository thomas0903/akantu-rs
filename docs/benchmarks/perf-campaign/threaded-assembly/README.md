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
