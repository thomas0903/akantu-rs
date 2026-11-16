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
`Tetrahedron4` mesh stores about 11 symmetric entries per element.

RESULTS_PLACEHOLDER
