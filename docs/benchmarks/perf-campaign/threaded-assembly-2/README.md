<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Matrix assembly on threads, second round

Branch `perf/threaded-assembly-2` from `perf/wave2-base` (d404f4c70). Every
result keeps every bit at every thread count (D-122, D-123): every stored
entry receives its contributions in element order.

Machine of every table: Intel Xeon Platinum 8581C (Emerald Rapids), 8
cores, one thread per core, 30 GiB, Ubuntu 24.04 (a development machine
used alone). One process per run. A run with `N` threads uses the suites'
`--threads N` (the policy in force is recorded in every JSON line) and is
pinned to cores `0..N-1`; the base runs with `--threads 1` on core 0.

## What changed

| Where | Before | After |
|---|---|---|
| repeated (cached) scatter | per call: every block's equations recomputed and checked against its slot, spans computed, the range boundaries searched (serial), parts sized by elements (up to four per thread) | the position cache keeps per slot the span and the count of stored positions, and one plan per element filter: entry bounds (one part per thread, equal entry counts) and per part the exact list of blocks, flagged when they lie inside the part; with the equations vouched for (mesh revision, numbering generation, connectivity storage; never an exported connectivity) the call scatters at once |
| one thread | `add_values_cached` per element (key check per block) | the planned scatter (one part); a round without a plan runs the serial loop, and the plan is built from the slots it has just checked |
| one thread, solid small-strain stiffness | all elemental matrices of a type, then the scatter | produced and scattered per 4 MB chunk (still in the caches); the whole-type array is not kept |
| two materials on one type | each material's assembly replaced the other's plan | one plan per filter (at most 16 per cache); a store drops only the plans that list the slot |
| phase-field elemental matrices | `compute_btdb` and `compute_ntbn` (serial), then `integrate` | `compute_btdb_integrated` (order 2) and `compute_ntbn_integrated`, per element on threads |
| structural elemental matrices | `compute_btdb` + `integrate`, mass point matrices then integral, serial | per element on the structural engine's executor (`StructuralFeEngine::set_exec`, set by the model) |

Memory: the position cache holds 12 more bytes per stored slot (span and
count; 372 instead of 360 bytes per four-node tetrahedron of a 3-D solid)
and per plan about 4 bytes per block visit (1.0–1.3 visits per block at 8
threads), 8 bytes per slot of the filter and one byte per slot of the type.
The solid model's kept arrays are as documented, and on one thread the
whole-type elemental array is no longer kept once the assemblies are
chunked.

## Tests and mutants

| test | what |
|---|---|
| `akantu-linalg` `sparse::perf_tests::threaded_batch_plan_follows_the_cache` | vouched keys, a key change with changed equations, profile growth, a reordered profile, another filter: every round bit for bit `add_values`, 1 to 8 threads, finest and automatic grain |
| `akantu-solver` `tests/threaded_assembly.rs` | against the uncached loop at 1, 2, 3, 4, 8 threads: two materials and a second type, symmetric and unsymmetric storage, couplings appended (the next round's batch follows the growth), a mesh event (node order of two elements permuted through the mesh accessor), `J` operations, products; the chunked assembly with chunks of three elements |
| `akantu-fe` `tests/threaded_elemental_matrices.rs` | `compute_ntbn_integrated` = `compute_ntbn` + `integrate` (every type, filters, errors) |
| `akantu-structural`, `akantu-phase-field` `tests/exec_threads.rs` | a DKT18 plate (repeated assemblies, static solve); a phase-field static solve and repeated stiffness assemblies |
| `akantu-solid` `tests/exec_threads_matrices.rs` | the kept arrays stay bounded (the chunked steady state keeps less) |

Mutants that fail: the equations key without the mesh revision; the plan
reused after the profile grew; a part boundary off by one; the inside flag
set on a straddling block; the plan reused whatever the key; the last
block of each chunk dropped. One that survives: not dropping a plan when
one of its slots is stored (a second guard: a batch that stores slots
rebuilds its plan, and changed equations change the key).

## The fused (chunked) pass

Instead of one array with the elemental matrices of the whole type,
the elemental matrices of a chunk of about 4 MB are produced and scattered
at once, while they are still in the caches; chunks in order and blocks in
order inside each part keep every entry's contributions in element order.
`setup_tet4` `stiffness_assembly` (3 calls), seconds per run, this machine
(raw runs `chunked/`):

| size | threads | without chunks (752e3e15b) | chunked, first version (fa6dc829f) | chunked (0bd91ba55) |
|---|---|---|---|---|
| `m`, 162,000 tetrahedra | 1 | 0.431 | 0.366 | 0.352 |
| `l`, 1,296,000 tetrahedra | 1 | 3.658, 3.636 | 3.792, 3.793 | 3.029, 3.025 |
| `m` | 8 | 0.080 | 0.122 | (whole array: 0.080) |
| `l` | 8 | 0.717, 0.716 | 0.732, 0.735 | (whole array: 0.713, 0.722) |

- On one thread the chunked pass is 17–18 % faster than the whole-array
  pass at both sizes. Its first version was slower at size `l`: every
  chunk looked its plan up by comparing the whole slot list (10 MB, 356
  chunks); the plan is now looked up once per assembly.
- With threads it is slower (0.122 against 0.080 s at 8 threads, size
  `m`): a chunk of consecutive elements touches a narrow band of entries,
  so only one or two of the entry-range parts have work per chunk. It is
  used on one thread only; with threads the whole-array batch keeps every
  part busy.
- The memory-bound regime (750,000 tetrahedra and more) is where the
  threaded scatter itself now scales best (5.4× at 8 threads, table below);
  what remains at size `l` with 8 threads is the elemental pass, the
  serial first assembly and the per-call set-up.

## The first assembly (profile construction)

It stays serial. A bitwise parallel form exists: per part of the elements,
the pairs new to the matrix and to the part in order, merged part by part
in order (the first occurrence of every key keeps its position and
orientation). It was implemented, property-tested and measured in the
previous round (`../threaded-assembly/README.md`, "Tried, did not pay"):
it does three hash operations per pair against one in the serial fused
loop, and was equal at 4 and 8 threads on `setup_tet4` and
`static_linear_eigen` and slower at 2 threads where a model profiles again
at every iteration (`cohesive_intrinsic_implicit`). Per-thread sorted
column lists merged in a fixed order would give a row-sorted profile,
which is another entry order than the oracle's first-insertion order: a
different input order for the direct solvers and for the matrix files
(not bitwise). What would pay is fewer hash operations in the serial
merge (a row-wise index of the profile), not more threads.

## The repeated scatter alone, against the prototype

`crates/akantu-solver/examples/assembly_scaling.rs` (the DOF manager's
cached assembly, symmetric 12 × 12 blocks, the mesh of
`crates/akantu-linalg/examples/assembly_threads.rs`) and that prototype,
median of 9 (7) rounds, three runs each, 0bd91ba55 (raw output `probe/`;
the prototype's own numbers in `probe/probe-head.txt`):

| tetrahedra | | 1 thread | 2 | 4 | 8 |
|---|---|---|---|---|---|
| 162,000 | d404f4c70 (library, before) | 35.6 ms | 20.4 (1.75×) | 13.5 (2.63×) | 10.9 (3.27×) |
| 162,000 | this branch | 25.0–25.4 ms | 13.7 (1.84×) | 7.7 (3.28×) | 4.5–4.6 (5.5×; 7.8× of the former one thread) |
| 162,000 | prototype (serial loop 24.7 ms) | 26.8 | 14.5 (1.70×) | 7.4 (3.35×) | 4.1–4.2 (5.9–6.0×) |
| 750,000 | this branch | 122.4–122.7 ms | 65.6 (1.87×) | 37.1 (3.30×) | 22.3 (5.5×) |
| 750,000 | prototype (serial loop 148 ms) | 167.5 | 113.4 (1.31×) | 70.0 (2.11×) | 43.5 (3.40×) |

At 162,000 tetrahedra the library is within 10 % of the prototype at 8
threads (it also keeps the cache, the release counter and the plan
checks); at 750,000, where the scatter is bound by memory, it is about
twice as fast as the prototype: one part per thread with exact block
lists, and blocks inside a part added without a range test per pair.

## Threads: the cases of the assembly work

Series: `scripts/perf/threaded-assembly/series.sh BIN_DIR OUT 5` (size
`m`, 5 repetitions, the five variants rotated per repetition), base =
d404f4c70 with `--threads 1` on core 0, head = 0bd91ba55 with
`--threads N` on cores `0..N-1` (2026-11-17, raw data `series/`, full
tables `series-*-summary.md`). Ratio = head / base per repetition,
median [min, max]; base = median seconds. Every digest equal across the
five variants and the five repetitions (the `digest` column reads `same`
in every row).

| case | phase (calls) | base | 1 thread | 2 threads | 4 threads | 8 threads |
|---|---|---|---|---|---|---|
| `setup_tet4` | `stiffness_assembly` (3) | 470.7 ms | 0.744 [0.741, 0.748] | 0.483 [0.481, 0.491] | 0.276 [0.273, 0.278] | 0.169 [0.167, 0.170] |
| `setup_tet4` | `mass_consistent` (3) | 343.6 ms | 0.919 [0.903, 0.925] | 0.496 [0.492, 0.504] | 0.285 [0.284, 0.291] | 0.185 [0.183, 0.187] |
| `setup_tet10` | `stiffness_assembly` (3) | 345.0 ms | 0.894 [0.882, 0.907] | 0.489 [0.474, 0.494] | 0.251 [0.248, 0.255] | 0.135 [0.132, 0.137] |
| `setup_tet10` | `mass_consistent` (3) | 179.2 ms | 0.982 [0.976, 1.015] | 0.497 [0.495, 0.504] | 0.264 [0.262, 0.269] | 0.148 [0.146, 0.150] |
| `setup_hex8` | `stiffness_assembly` (3) | 599.2 ms | 0.877 [0.864, 0.912] | 0.487 [0.478, 0.488] | 0.249 [0.246, 0.250] | 0.138 [0.138, 0.140] |
| `setup_hex8` | `mass_consistent` (3) | 243.2 ms | 0.985 [0.972, 1.030] | 0.533 [0.524, 0.542] | 0.283 [0.280, 0.288] | 0.162 [0.155, 0.165] |
| `static_linear_eigen` | `stiffness_assembly` (3) | 41.3 ms | 0.846 [0.838, 0.848] | 0.473 [0.450, 0.477] | 0.289 [0.270, 0.292] | 0.174 [0.167, 0.176] |
| `static_linear_eigen` | `solve_step` (1) | 754.5 ms | 0.986 [0.979, 0.994] | 0.982 [0.973, 0.990] | 0.978 [0.974, 0.991] | 0.977 [0.970, 0.982] |
| `static_newton_plastic` | `solve_step` (12) | 1.762 s | 0.997 [0.995, 1.001] | 0.939 [0.939, 0.941] | 0.942 [0.939, 0.950] | 0.957 [0.952, 0.974] |
| `implicit_dynamic` | `residual` (180) | 149.4 ms | 0.993 [0.991, 1.001] | 0.525 [0.523, 0.530] | 0.318 [0.316, 0.320] | 0.222 [0.220, 0.223] |
| `implicit_dynamic` | `solve_step` (60) | 822.7 ms | 1.001 [0.990, 1.007] | 0.891 [0.884, 0.898] | 0.863 [0.855, 0.865] | 0.853 [0.843, 0.858] |
| `cohesive_intrinsic_implicit` | `solve_step` (30) | 3.052 s | 0.978 [0.975, 1.011] | 0.922 [0.915, 0.924] | 0.912 [0.906, 0.913] | 0.906 [0.903, 0.910] |
| `heat_static_tri6` | `assemble_conductivity` (5) | 38.5 ms | 0.831 [0.811, 0.834] | 0.515 [0.498, 0.524] | 0.303 [0.296, 0.305] | 0.191 [0.189, 0.197] |
| `heat_static_tri6` | `solve_step` (1) | 1.974 s | 0.997 [0.984, 1.003] | 0.990 [0.983, 1.008] | 0.994 [0.982, 0.999] | 0.992 [0.977, 0.996] |
| `heat_implicit_hex8_kvar` | `solve_step` (5) | 586.8 ms | 1.000 [0.989, 1.008] | 0.961 [0.954, 0.975] | 0.950 [0.946, 0.958] | 0.937 [0.935, 0.970] |
| `phasefield_staggered_tri3` | `solid_assemble_stiffness` (5) | 59.2 ms | 0.753 [0.753, 0.757] | 0.433 [0.428, 0.435] | 0.270 [0.265, 0.273] | 0.199 [0.198, 0.201] |
| `phasefield_staggered_tri3` | `phase_assemble_stiffness` (5) | 57.1 ms | 0.568 [0.566, 0.575] | 0.370 [0.354, 0.375] | 0.238 [0.226, 0.257] | 0.184 [0.168, 0.187] |
| `phasefield_staggered_tri3` | `solid_solve` (4) | 3.218 s | 1.016 [1.011, 1.022] | 0.985 [0.978, 0.990] | 0.974 [0.971, 0.984] | 0.978 [0.971, 0.982] |
| `phasefield_staggered_tri3` | `phase_solve` (4) | 863.1 ms | 0.978 [0.970, 0.983] | 0.939 [0.933, 0.943] | 0.923 [0.917, 0.925] | 0.918 [0.916, 0.924] |
| `phasefield_staggered_tet4` | `solid_assemble_stiffness` (5) | 24.9 ms | 0.939 [0.913, 0.964] | 0.495 [0.491, 0.515] | 0.335 [0.328, 0.345] | 0.235 [0.221, 0.238] |
| `phasefield_staggered_tet4` | `phase_assemble_stiffness` (5) | 8.0 ms | 0.582 [0.558, 0.617] | 0.583 [0.567, 0.612] | 0.598 [0.591, 0.603] | 0.610 [0.584, 0.615] |
| `embedded_rebar_tri3` | `assemble_stiffness` (5) | 42.3 ms | 0.867 [0.852, 0.870] | 0.696 [0.689, 0.717] | 0.636 [0.617, 0.647] | 0.656 [0.645, 0.668] |
| `embedded_rebar_tet4` | `assemble_stiffness` (5) | 99.0 ms | 0.923 [0.913, 0.937] | 0.566 [0.550, 0.580] | 0.388 [0.380, 0.390] | 0.291 [0.285, 0.294] |
| `contact_implicit_static_quad4` | `solve_step` (5) | 741.7 ms | 0.996 [0.993, 1.007] | 0.948 [0.945, 0.952] | 0.930 [0.928, 0.954] | 0.924 [0.920, 0.941] |
| `structural_beam3_frame_static` | `assemble_stiffness` (5) | 35.2 ms | 0.812 [0.790, 0.823] | 0.453 [0.446, 0.460] | 0.273 [0.267, 0.276] | 0.198 [0.194, 0.204] |
| `structural_beam3_frame_dynamic` | `solve_step` (60) | 2.870 s | 0.984 [0.976, 0.991] | 0.976 [0.972, 0.982] | 0.976 [0.972, 0.983] | 0.984 [0.983, 0.995] |
| `structural_dkt18_plate_static` | `assemble_stiffness` (5) | 217.4 ms | 0.578 [0.559, 0.579] | 0.308 [0.304, 0.313] | 0.177 [0.173, 0.178] | 0.111 [0.109, 0.113] |
| `structural_dkt18_plate_static` | `solve_step` (1) | 937.5 ms | 1.002 [0.996, 1.012] | 1.000 [0.991, 1.012] | 1.006 [0.988, 1.011] | 1.006 [0.989, 1.007] |

One thread: every assembly phase is faster than the base (0.57–0.98).
Two rows above 1:

- `implicit_dynamic` `mass_consistent` (a one-shot mass, one call): 1.095
  [1.025, 1.114] in the series, 0.989 in 9 interleaved pairs of that case
  alone (`checks/idm/`): the series value reflects its position in the
  rotation, not the code.
- `phasefield_staggered_tri3` `solid_solve`: 1.016 in the series, 1.014
  [1.007, 1.017] in 7 interleaved pairs alone (`checks/pft/`); its time is
  the SparseLU factorisation, whose kernels (`gemm_avx512`, `panel_bmod`,
  `dfs_kernel`, `gebp_minus`, `copy_to_ucol`) are instruction for
  instruction identical in both binaries but sit at other offsets within
  64-byte lines (e.g. `gemm_avx512` at 16 instead of 48). The same effect
  moved `step_rest` of `implicit_dynamic` and `static_newton_plastic` by
  +4 % in an intermediate build of this branch and back to 1.000 in the
  final one. It is code placement in the solver, not a change of this
  branch.

## What limits what remains

- **The sparse solver**: every implicit step is led by the SparseLU
  factorisation and solves (another branch threads them); the steps gain
  0–15 % at 8 threads while their assemblies run 5–9× faster.
- **Serial parts of an assembly**: the first assembly (profile
  construction, above), the law's tangent for laws that have not opted in
  (the phase-field solid law, the reinforcement law of the embedded
  model: `embedded_rebar_tri3` stops at 1.5× at 4 threads), the density
  functor of the mass, and the per-call set-up. `phase_assemble_stiffness`
  of `phasefield_staggered_tet4` (6,000 elements, 8 ms) is too small to
  split: it stays at its one-thread time (0.58 of the base) at every
  thread count.
- **Memory traffic**: the elemental pass and the scatter of 1-point
  tetrahedra move about 1.2 kB per element; `setup_tet4` stops at 5.9×
  (stiffness) and 5.4× (mass) at 8 threads where the higher-order
  elements reach 6.2–7.4×.
