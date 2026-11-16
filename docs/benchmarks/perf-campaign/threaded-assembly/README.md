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
| scatter of a repeated assembly (cached entry positions) | element by element | equations checked (and missing slots resolved) in parallel, then entry ranges: each thread adds, in element order, the contributions whose position lies in its range |
| first assembly of every element of a type (profile and values) | element by element, one lookup per pair | per part of elements, the pairs new to the matrix and to the part (threads); merge of the parts in order (serial, one insertion per listed pair); positions per element and the entry-range scatter (threads) |
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
- The profile is the order of first insertion. The merge processes the
  parts in element order and, inside a part, the pairs in the order the
  serial loop visits them, so the first occurrence of every key comes
  first and keeps its coordinates (orientation) and position.
- A row of the product receives, in the serial pass, the terms of the
  entries in storage order; the gathered index lists exactly those terms
  in that order.

## Tests

| test | what |
|---|---|
| `akantu-linalg` `sparse::perf_tests::threaded_batch_assembly_is_add_values` | the batch scatter (or the serial loop when it declines) against `add_values`, 5 matrix/block/policy combinations × 12 trials × 7 rounds × 1, 2, 3, 4, 8 threads: values, errors, releases (all slots, a subset, a repeated slot, changed equations, appended entries, rebuilt profile, missing entries, resize) |
| `…::threaded_batch_resolves_like_the_serial_loop` | stored positions and resolve counter equal to the serial loop's |
| `…::threaded_first_assembly_is_the_serial_fused_loop` | the threaded first assembly against `add_block_to_profile_and_values`: coordinates, lookup map, values, both releases |
| `…::threaded_matrix_operations_match_the_serial_ones` | products (both reductions, an entry appended between products), Dirichlet rows, sums, profile copy, scaling, zeroing |
| `akantu-fe` `tests/threaded_elemental_matrices.rs` | fused `∫ BᵀDB` = `compute_btdb` + `integrate` (every element type, order 2 and 4, with and without a filter, the errors), consistent mass, at 1 to 8 threads, finest and automatic grain; `engine::assembly::tests::gemv_integral_is_the_general_product` |
| `akantu-solver` `tests/threaded_assembly.rs` | DOF manager: two materials on one type and a second type, symmetric and unsymmetric storage, 6 rounds, appended couplings, `J = copy_profile(K) + c M + d K`, Dirichlet rows, products, the number of threaded assemblies |
| `akantu-solid` `tests/exec_threads_matrices.rs` | static Newton with an elastic law and `neohookean` (not opted in, tangent reassembled at every iteration) on tetrahedra and hexahedra with prescribed Dirichlet values; implicit dynamics with the consistent mass; repeated stiffness assemblies of two laws: `K`, `M`, `J`, nodal fields, laws' fields |
| `akantu-diffusion` `tests/exec_threads.rs` | backward Euler with a temperature-dependent conductivity, consistent capacity, triangles and quadrangles, Dirichlet conditions |
| `akantu-structural` `tests/exec_threads.rs` | a frame of beams: repeated stiffness assemblies, static solve, Newmark steps with the consistent mass |

Mutants checked to fail them: a range boundary off by one (an element's
last range skipped), the blocks of a range visited in reverse order
(entries scattered out of element order), the terms of a product row in
reverse order, the parts of the first assembly merged in reverse order,
the orientation of a first occurrence swapped.
