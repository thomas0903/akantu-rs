<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Sparse assembly, profile, matrix-vector product and solver hand-over (W2-ASSEMBLY)

Branch `perf/sparse-assembly` from `perf/wave2-base` (59e05aa3f). Every
change keeps the default results bit for bit: the same binary64 additions
into each matrix entry in the same order, the same profile order and the
same release counters (D-122).

Machine of every table: Intel Xeon Platinum 8581C (Emerald Rapids), 8
cores, one thread per core, 30 GiB, Ubuntu 24.04 (a development machine of
the campaign, not the AWS measurement machine). One process per run,
pinned (`taskset`; MPI ranks on cores 0..N-1).

## What changed

| Where | Before | After |
|---|---|---|
| profile map `(row, column) → entry` (`SparseMatrixAij`) | `HashMap` with SipHash-1-3 | the same map with `int_hash::IntHasher` (folded 64 × 64-bit multiply); never iterated, so no result depends on the hash |
| repeated element assembly (`DofManager::assemble_elemental_matrices_to_matrix`) | one hash lookup per elemental entry and assembly | entry positions per element cached (`BlockPositionCache`, `add_values_cached`) from the second assembly with unchanged positions; appended entries (contact) keep them |
| first assembly of every element of a type | profile all elements (one lookup per pair), then add the values (a second lookup per pair) | profile and values element by element, one lookup per pair (`add_block_to_profile_and_values`); not used when a block would promote the matrix (the order would differ) |
| `mat_vec_mul`, `apply_boundary` | a `Vec` of local index pairs rebuilt per call with two SipHash lookups per entry | coordinates checked once (O(1) on a serial layout: largest coordinate tracked), local equations from the identity layout or a dense global → local map; no allocation |
| `EquationLayout` global → local | `HashMap` (SipHash) | dense `Vec<u32>` while the largest global number present is below `4 · nb_local + 4096`, hashed (`IntHasher`) beyond |
| `J = copy_profile(K); J += K` (every Newton step, `integration_scheme.rs`) | `AijBackend::copy_profile` cloned `K`; `copy_profile` rebuilt `J`'s map; `add_matrix` one lookup per entry | no clone; the map is kept (same entries) or cloned (same key rule); `add_matrix` adds entry-wise when both matrices store the same coordinates with one key per entry |
| `CscMatrix::from_aij` (SparseLU / faer input) | triplet vector, then `setFromTriplets` | the same steps fed from the COO arrays, temporaries released early |

Property tests (`crates/akantu-linalg/src/sparse_perf_tests.rs`) compare
each path with the loop it replaces on random input with signed zeros,
infinities, NaN and subnormals: values bit for bit, errors, release
counters, the profile map. A mutant that forgets to invalidate the cached
positions on `clear_profile` fails them.

## Memory of the position cache

A stored element holds 4 bytes per entry the block adds and 4 bytes per
element equation; nothing is stored before the second assembly with
unchanged positions (a one-shot static solve stores nothing), and a
change of positions releases it. `DofManager::set_assembly_cache(false)`
turns it off; results are identical.

| element (3 DOFs per node unless noted) | equations | entries added (symmetric) | bytes per element | per 10⁶ elements |
|---|---|---|---|---|
| `Triangle3`, 2 DOFs | 6 | 21 | 108 | 108 MB |
| `Quadrangle4`, 2 DOFs | 8 | 36 | 176 | 176 MB |
| `Tetrahedron4` | 12 | 78 | 360 | 360 MB |
| `Hexahedron8` | 24 | 300 | 1,296 | 1.3 GB |
| `Tetrahedron10` | 30 | 465 | 1,980 | 2.0 GB |

An unsymmetric matrix stores `n²` entries per element instead of
`n (n + 1) / 2`. For comparison, the assembled matrix costs about 24 bytes
per stored entry plus 20–40 bytes per entry for its lookup map, and a
structured `Tetrahedron4` cube stores about 12 symmetric entries per
element (1,953,456 entries for 162,000 tetrahedra).

## Suite: before / after

`scripts/perf/suite/run_ab.sh`, size `m`, 5 interleaved repetitions,
order rotated, core 2; "before" = `perf/wave2-base` 59e05aa3f, "after" =
fc4a375ff (this branch before the merge of the reviewed suites; both
binaries of the same suite version). Ratio after / before per repetition,
median [min, max]. Every digest equal between the two binaries in every
repetition. Raw data: `suite-ab/ab3/`, full table
`suite-ab/ab3-summary.md` (`ab1`: an earlier build with the reverted
`from_aij` change; `ab2`: the check of that change).

| case | phase | after | after / before |
|---|---|---|---|
| `setup_tet4` (162,000 elements) | `stiffness_assembly` (3 calls) | 1.324 s | 0.507 [0.497, 0.509] |
| `setup_tet4` | `mass_consistent` (3 calls) | 3.009 s | 0.702 [0.686, 0.705] |
| `setup_tet10` | `stiffness_assembly` | 1.310 s | 0.633 [0.621, 0.647] |
| `setup_hex8` | `stiffness_assembly` | 2.130 s | 0.777 [0.735, 0.783] |
| `static_linear_eigen` | `stiffness_assembly` | 74.9 ms | 0.481 [0.471, 0.515] |
| `static_linear_eigen` | `solve_step` | 2.214 s | 0.948 [0.938, 0.965] |
| `implicit_dynamic` | `solve_step` (60 steps) | 1.184 s | 0.618 [0.608, 0.637] |
| `cohesive_intrinsic_implicit` | `solve_step` (30 steps) | 6.100 s | 0.727 [0.721, 0.764] |
| `static_newton_plastic` | `solve_step` (12 steps) | 2.607 s | 1.024 [1.023, 1.044] (see "Code-generation side effect") |
| `heat_static_tri6` | `assemble_conductivity` (5 calls) | 73.9 ms | 0.496 [0.484, 0.526] |
| `heat_static_tri6` | `solve_step` | 4.001 s | 0.950 [0.946, 0.962] |
| `heat_implicit_hex8_kvar` | `solve_step` | 1.022 s | 0.909 [0.889, 0.927] |
| `phasefield_staggered_tri3` | `solid_assemble_stiffness` | 91.5 ms | 0.417 [0.401, 0.430] |
| `phasefield_staggered_tri3` | `solid_solve` | 8.984 s | 0.943 [0.909, 0.972] |
| `phasefield_staggered_tet4` | `solid_assemble_stiffness` | 45.6 ms | 0.480 [0.457, 0.502] |
| `contact_implicit_static_quad4` | `solve_step` | 1.416 s | 0.878 [0.838, 0.884] |
| `structural_beam3_frame_static` | `assemble_stiffness` | 70.9 ms | 0.516 [0.483, 0.527] |
| `structural_beam3_frame_dynamic` | `solve_step` (60 steps) | 4.289 s | 0.825 [0.821, 0.833] |
| `structural_dkt18_plate_static` | `assemble_stiffness` | 372 ms | 0.623 [0.618, 0.642] |
| `embedded_rebar_tri3` | `assemble_stiffness` | 80.3 ms | 0.619 [0.583, 0.638] |
| `embedded_rebar_tet4` | `assemble_stiffness` | 248 ms | 0.565 [0.532, 0.584] |

The solve phases are led by the SparseLU factorisation, which this task
does not change; their gain is the assembly, `J = K` and the product
inside them. `mass_consistent` is led by the elemental mass products
(`eigen_gebp`, W2-GEBP).

## Static solve against C++, MUMPS on both sides

`crates/akantu-bench/scripts/c1prep_campaign.sh`, cohesive benchmark
input, 10 explicit steps after the static solve (only `static_solve` and
the memory are compared), C++ strict build against the port built with
`--features mpi,scotch,mumps` in the project image; at 4 ranks the C++
Scotch partition is replayed. Five repetitions, the three programs
interleaved, order alternated per repetition. "before" = `perf/wave2-base`,
"after" = d208746c0 (the first commit of this branch: hasher, identity
layouts, position cache, `copy_profile` / `add_matrix` fast paths; the
later commits do not touch this path except the fused first assembly).
Static solutions byte-identical (energies equal to 17 digits between all
three). Raw data and the script: `static-mumps/` (`parse.py`, `table.txt`).

| case | C++ [s] | before [s] | after [s] | before / C++ | after / C++ | after / before |
|---|---|---|---|---|---|---|
| h = 0.12, 1 rank (14,472 DOFs) | 0.587 [0.581, 0.640] | 0.732 [0.709, 0.846] | 0.540 [0.519, 0.621] | 1.220 [1.148, 1.321] | 0.920 [0.846, 0.970] | 0.734 [0.729, 0.754] |
| h = 0.06, 1 rank (97,551 DOFs) | 10.878 [10.741, 12.888] | 12.833 [12.229, 14.247] | 10.427 [10.228, 11.075] | 1.158 [1.105, 1.195] | 0.964 [0.859, 0.974] | 0.812 [0.777, 0.841] |
| h = 0.06, 4 ranks | 4.176 [4.071, 4.452] | 4.681 [4.474, 5.164] | 4.410 [4.242, 4.579] | 1.106 [1.070, 1.211] | 1.056 [0.953, 1.100] | 0.952 [0.887, 0.959] |

Peak resident memory, like for like (sampler, whole run, sum over the
ranks, MiB, medians): h = 0.12: C++ 408, before 277–366, after 336;
h = 0.06, 1 rank: C++ 3,175, before 2,853, after 2,902; 4 ranks: C++
3,879, before 3,327, after 3,391. The port stays below C++ (after / C++
0.82, 0.91, 0.88); after / before is 1.03 [0.94, 1.22], 1.02 [0.95,
1.03] and 1.01 [0.96, 1.16], inside the spread of the sampler. The 1.72×
of `baseline/summary.md` compared the port's SparseLU with C++'s MUMPS
(different solvers); with the same solver the port uses less memory. No
position cache is built in this one-shot solve.

What remains at 4 ranks (1.06×) was not profiled: the distributed MUMPS
input is handed over as before (`coo_one_based_i32` per analysis) and the
collective agreement phases of the DOF manager run around it.

## Releasing the static solver after the preload (report finding 1.1)

`SolidMechanicsModel::release_static_solver` (Python
`releaseStaticSolver`; manual "Solver configuration") destroys the
analysis and factorisation of `J`'s solver (MUMPS: `destroy()`, agreed
collective `JOB = -2`; SparseLU and faer: their factors) and the storage of
`J` and `K`, and makes the next `K` assembly happen. Not called by the
library. The PETSc back-end returns `Unsupported`. Tests: a later static
solve gives the bits of a run that kept everything
(`crates/akantu-solver/tests/release_factorization.rs`, SparseLU and faer;
`mumps_mpi_solver.rs` scenario `release_factorization_mumps`, PASS at 1, 2
and 4 ranks; `python/tests/test_solver_layer.py::
test_release_static_solver_then_resolve`; the manual example).

Driver: `rust_release_static_solver = true` (default false) calls it after
the static solve and after the solver statistics are read, and records
`rss_after_release`. `c1prep_campaign.sh`, port with MUMPS, one run per
configuration, raw data `release-static/` (per-run `summary.json`,
`insertions.csv`, `energies.md5`).

| mesh, ranks, steps | energies.csv, insertions.csv | after the static solve | after the release | loop peak (VmHWM after the reset) | end of the run |
|---|---|---|---|---|---|
| h = 0.12, 1 rank, 5,000 (3,545 insertion steps, 11,739 cohesive elements) | byte-identical | 234 MiB | 177 MiB | 361 → 279 MiB | 316 → 209 MiB |
| h = 0.12, 2 ranks, 5,000 (sum over ranks) | byte-identical | 312 MiB | 227 MiB | 435 → 329 MiB | 398 → 263 MiB |
| h = 0.06, 1 rank, 10 | byte-identical | 1,903 MiB | 750 MiB | 2,901 → 1,903 MiB | 2,268 → 1,037 MiB |
| h = 0.06, 4 ranks, 10 (sum over ranks) | see below | 2,625 MiB | 1,061 MiB | 3,550 → 2,622 MiB | 2,929 → 1,253 MiB |

At 4 ranks the static solution with distributed MUMPS is not
reproducible from run to run in either code: the five C++ runs, the five
runs of the base and the five of this branch of the earlier series all
have different `energies.csv` from the first line on (`energies.md5`), so
the 4-rank pair cannot be compared byte for byte; at 1 and 2 ranks it
can, and is identical. The loop peak with the release equals the static
peak because the driver resets the peak before the release; the loop's own
growth stays below it.

## Threaded assembly: design

The value of an entry is the sum of its elemental contributions in
element order. A threaded assembly is bitwise iff every entry still
receives its contributions in that order.

- Colouring (elements of one colour share no entry, colours one after
  another) changes the order of the contributions into an entry: not
  bitwise.
- Per-thread partial matrices summed afterwards: bitwise only if the
  partition and the final sums do not depend on the thread count, which
  makes the result depend on a fixed block count; fast mode otherwise.
- **Entry ranges** (bitwise for every thread count): thread `t` owns a
  contiguous range of entry positions and visits, in element order, the
  elements whose cached positions touch its range, adding only the pairs
  that fall inside it. Each entry is written by one thread, in element
  order. The positions are already cached ([`BlockPositionCache`]); per
  element the smallest and largest position decide whether a thread
  visits it. The profile is built in element order, so an element's
  entries lie close to each other and each thread visits about `E / P`
  elements plus the neighbours at its range boundaries.

Prototype: `crates/akantu-linalg/examples/assembly_threads.rs`
(`std::thread::scope`, no dependency on `perf/exec-policy`), structured
`Tetrahedron4` cube, symmetric 12 × 12 blocks, cached positions, values
checked bit for bit against `add_values` at every thread count. Median of
7 runs:

| tetrahedra | entries | serial | 2 threads | 4 threads | 8 threads | elements visited at 8 threads |
|---|---|---|---|---|---|---|
| 162,000 | 1.95 M | 29.1 ms | 16.6 ms (1.75×) | 8.3 ms (3.48×) | 4.7 ms (6.20×) | 1.24 E |
| 750,000 | 8.87 M | 170 ms | 123 ms (1.38×) | 76 ms (2.23×) | 47 ms (3.64×) | 1.14 E |

The larger case is bound by memory traffic (864 MB of elemental values).
The scatter is a small part of an assembly: at 162,000 tetrahedra the
whole `stiffness_assembly` of `setup_tet4` (elemental `BᵀDB` included)
takes 448 ms per call against 29 ms for the cached scatter, so the
threaded assembly pays when the elemental kernels (per element,
independent: any order) run on the same threads, each thread computing
the elements it visits. A later task on `akantu-core::exec`: compute the
elemental matrices in parallel into the `elementary` array, then the
entry-range scatter; the one-shot first assembly (profile construction)
stays serial, because the profile order is the element order.

## What remains, and why

After the changes, the profiles of the cases with an assembly phase
(`profiles/prof1/`, recorded before the fused first assembly) are led by
the SparseLU kernels (`sparselu_gemm`, `SparseLu::solve`, `panel_bmod`:
W2-SPARSELU) and the generic small products of the elemental matrices
(`eigen_gebp::product_into`, 12–35 %: W2-GEBP). What is left of this
task's code:

- **Repeated assembly** (`add_values_cached`, 2.6 % of `heat_static_tri6`):
  per entry one 4-byte position, one value and one read-modify-write of
  the entry, in element order. The order of the additions into an entry is
  fixed by the bitwise rule; the remaining cost is memory traffic. Faster
  only with threads (design above).
- **Element equations** (`DofManager::element_equations`, 2.7 % of
  `heat_static_tri6`, computed twice per element: once to validate before
  any backend change, once to assemble). Caching them per element would
  cost 4 bytes per equation for a gain of about 1 %.
- **First assembly** (profile construction, `add_to_profile`, 6 % of
  `setup_tet4`): one hash lookup and, for a new entry, one insertion per
  elemental pair; the profile order (first insertion, element order) is
  fixed. The hash table is about 30 bytes per entry and is accessed at
  random, so large meshes miss the caches on most lookups. A row-wise
  index (per row, the columns of its entries) would turn most of these
  misses into hits; it needs an index that grows as rows gain entries.
  Expected 1.5–2× on the profile construction of large meshes. Not done.
- **`mat_vec_mul`** (2.5 % of `implicit_dynamic` after the change, from
  32 %): one pass over the COO arrays, memory-bound. A row-gathered copy of
  the operator (per row, its direct and mirror contributions in storage
  order, which keeps each row's summation order) would allow threads by
  row; 2× the index memory.
- **Solver hand-over**: `CscMatrix::from_aij` is built at every analysis
  and factorisation (1.6–2.4 % of `contact_implicit_static_quad4`).
  Caching the triplet-to-CSC map per profile in the SparseLU solver would
  turn a refactorisation's conversion into a scatter of the values; it
  touches `eigen_sparse_lu/mod.rs` (W2-SPARSELU). A rewrite without the
  intermediate triplet vector was measured slower and reverted (`analysis`
  1.090 [0.993, 1.211] of the base against 0.996 without it).
- **Profile rebuild on node addition** (cohesive insertion, inserted
  nodes): every matrix's profile is cleared and rebuilt, as upstream's
  `resizeGlobalArrays` does. Keeping it is required for bitwise MUMPS and
  SparseLU input (entry order). Appended contact couplings no longer
  discard the cached positions (position epoch).

### Code-generation side effect in SparseLU (not this task's code)

In the binaries of this branch the compiler inlines
`eigen_sparse_lu::dense::gemv_product` and `trsv_upper` into
`SparseLu::solve`; in the base binary they are separate functions
(`profiles/prof-np/before.txt` against `a2.txt`, `static_newton_plastic`).
The triangular solves then take about 5 % longer: `static_newton_plastic`
`step_rest` 1.047 [1.036, 1.057], `back_substitution` of
`static_linear_eigen` 1.06 [1.05, 1.11] (`suite-ab/ab2-summary.md`). The
SparseLU code is unchanged; the edits to other files of `akantu-linalg`
change how the crate is split into codegen units (release profile:
16 units, no LTO). An `#[inline(never)]` / `#[inline]` decision in those
kernels (W2-SPARSELU) or `codegen-units = 1` for the release profile
would make the result independent of unrelated edits.

### Fast-mode candidates (not written; the switch of D-123 is not merged)

| candidate | where | expected gain |
|---|---|---|
| per-thread partial matrices, or colouring, summed in thread order | element assembly | up to the core count on the scatter; the entry-range scheme gets most of it bitwise |
| `mat_vec_mul` on a CSR copy with vectorised (reassociated) row sums and FMA | implicit dynamics | 1.5–2× of the product |
| another profile order (row-sorted, CSR from the start) | profile construction, solver input | 2× on profile construction; changes the MUMPS / SparseLU input order |


