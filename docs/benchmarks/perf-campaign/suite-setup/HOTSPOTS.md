# Set-up suite: where the time goes (ranked)

Area of `suite_setup` and `python/benchmarks/suite_python.py`: mesh
reading and writing, derived connectivities, groups, periodicity, cohesive
insertion, partitioning and distribution, the MPI layer, the parser, the
dumpers and the Python layer. Timings: `baseline/` (size `m`, 5
repetitions, an AWS `m7a.2xlarge`, AMD EPYC 9R14, one process per run
pinned to one core) and `scaling/` (sizes `s`, `m`, `l`). Profile shares:
`profiles/` (`perf record --call-graph dwarf` with `--loop-seconds`, size
`m` unless the file name says `l`; recorded on a 4-vCPU boat.dev sandbox,
AMD Ryzen 9 9950X, and for the two Python profiles on a 4-core AMD EPYC
Milan development machine; shares, not times). Distributed profiles: 4
ranks, rank 0 and rank 1 recorded separately (`-r0`, `-r1`); the Scotch
partitions of those profiles used Scotch's default thread count.

Each entry: the cases and phases it dominates, its share, the reading,
the expected gain, and whether a fix keeps every bit (bitwise-safe, the
default path of D-122) or needs the fast mode. Rank = share × gain × how
common the use is (every cohesive run builds the facet mesh; every
distributed run distributes; every run reads a mesh).

## Baseline at a glance (size `m`, AWS, medians of 5)

| case | phase | median | per unit |
|---|---|---|---|
| `facets_tet4` (162,000 Tet4) | `init_mesh_facets` | 1.80 s | 11.1 µs per element |
| `facets_hex8` (125,000 Hex8) | `init_mesh_facets` | 0.96 s | 7.7 µs per element |
| `cohesive_insert_tet4` | `init_mesh_facets` / `insert` | 1.82 s / 4.22 s | 26 µs per cohesive element |
| `msh22_read_tet4` / `msh41_read_tet4` / `msh22_read_hex8` | `read` | 144 / 115 / 173 ms | 0.7–1.4 µs per element |
| `msh_write_tet4` | `write` | 54 ms | 0.33 µs per element |
| `groups_tet4` | `element_group_from_node_group` | 164 ms | quadratic (b = 1.87) |
| `partition_rcb_tet4` / Scotch | `partition` | 79 ms / 840 ms | |
| `dump_paraview_tet4` / ASCII / text | `dump` | 200 / 927 / 74 ms | per dump |
| `parser_dat` | `parse` / `evaluate` | 6.6 / 14.5 ms | 0.9 µs per parameter |
| `py_twin_explicit_tri3` (80,000 Tri3) | `solve_step` / tiny | 4.71 ms / 8.1 µs | 59 ns per element |
| `dist_mesh_tet4`, 4 ranks | `partition` / `distribute` | 749 / 920 ms | |

Full tables: `baseline/*.md` (`summarize.py`); growth exponents:
`scaling/exponents.md`.

## Ranked list

### 1. Facet mesh construction (`init_mesh_facets`): pairwise row scans (bitwise-safe)

- Cases: `facets_tet4` (init_mesh_facets 1.80 s at `m`, 12× the time to
  read the mesh), `facets_hex8` (0.96 s), `cohesive_insert_tet4` (30 %),
  `dist_facets_tet4` (28 % at 4 ranks). Every cohesive model (extrinsic and
  intrinsic) and the contact detection build it.
- Profile: `akantu_mesh::utils::plan_facets` 64 % self of `facets_tet4`,
  41 % at `l`. For every facet of every element it scans the element row
  of the facet's first node against the rows of the other facet nodes,
  `count()` with multiplicity (`crates/akantu-mesh/src/utils.rs:376-384`):
  about valence² ≈ 1,150 comparisons per facet of the Kuhn Tet4 mesh, and
  every interior facet is evaluated from both of its elements (kept by the
  smaller one). Linear in the mesh, large constant; upstream's algorithm.
- Fix: intersect sorted CSR rows (or mark the rows of the first node in a
  per-element stamp array), and decide the owner before collecting the
  connected list. The created facets, their order (element order, facet
  order, smallest element first) and the connected-element order (the
  first node's row order) can be kept exactly: integer work, no floating
  point. Expected gain on `plan_facets`: 5–10×, on `init_mesh_facets`
  3–5× at `m`.

### 2. Physical-name transfer to the facet mesh: superlinear on planar boundaries (bitwise-safe)

- Cases: the same as 1 when the mesh has `physical_names` (every Gmsh mesh
  with physical groups). `transfer_physical_names` is 7 % of
  `facets_tet4` at `m` and 35 % at `l`; the whole `init_mesh_facets`
  grows with b = 1.18 (Tet4) and 1.38 (Hex8) between `m` and `l`.
- Reading: `first_facet_within` (`crates/akantu-mesh/src/utils.rs:700-716`)
  sorts the facet barycentres on **x only** and scans the window
  `[x − 2 tol, x + 2 tol]`. All boundary facets of a plane `x = const`
  (the `xmin`, `xmax` sides of every box-shaped mesh, and every structured
  mesh) share the same x, so each query scans O(n²) facets: O(N^{4/3}) in
  the element count instead of O(N log N). This is a defect of the port's
  acceleration structure, not of the algorithm.
- Fix: a cell grid (or a sort on all coordinates) of the barycentres, the
  same tolerance test, the same "smallest facet index within tolerance"
  rule: identical results. Alternatively match facets by their sorted node
  set (exact for conforming meshes; would need care to keep upstream's
  barycentre semantics for non-conforming input). Expected gain: the phase
  becomes linear; 5× on `init_mesh_facets` at `l`, more on larger meshes.

### 3. Cohesive insertion topology (bitwise-safe)

- Case: `cohesive_insert_tet4` insert 4.22 s at `m` (160,200 facets
  doubled in one call: 26 µs per new cohesive element). Extrinsic
  runs insert every few steps (smaller batches, same per-facet cost).
- Profile (71 % of the case): `InserterHelper::find_subfacet_to_double`
  26 % inclusive, `find_elements_around_subfacet` 17 %, `double_facets`
  13 %, dropping the `MeshTransaction` undo log 12 %
  (`core::ptr::drop_glue::<Vec<akantu_mesh::mutation::Undo>>`: one heap
  record per change), `malloc`/`free` about 10 %, `MeshData::element_map`
  and `set_element_list_row` lookups.
- Fix: adjacency of sub-facets built once per insertion (no repeated
  searches around each doubled node), undo records in a compact arena (or
  a cheaper commit path when no consumer can veto), fewer small `Vec`s.
  Integer topology only. Expected gain 2–4× on `insert`.
- Scaling and memory (`baseline/coh-l`, 1,296,000 Tet4, 3 runs): `insert`
  33.4 s (linear, b = 1.00 from `m`), `init_mesh_facets` 22.0 s
  (b = 1.20); peak resident memory 16.5 GB (`/usr/bin/time -v`), about
  12.7 kB per tetrahedron for the mesh, its facet mesh and the inserted
  elements: such a run does not fit a 7 GiB machine. The undo log of the
  mutation and the per-change allocations above are candidates for the
  memory as well.

### 4. Distribution from rank 0: partition and packaging (bitwise-safe)

- Cases: `dist_mesh_tet4` (`baseline/dist-l-p*`, `baseline/mpi-m-p*`):

  | size | ranks | partition (Scotch, rank 0) | distribute |
  |---|---|---|---|
  | `m` | 2 | 674 ms [669, 684] | 809 ms [803, 818] |
  | `m` | 4 | 749 ms [748, 772] | 920 ms [919, 943] |
  | `m` | 8 | 834 ms [832, 852] | 1.08 s [1.08, 1.10] |
  | `l` | 2 | 5.58 s [5.57, 5.59] | 7.32 s [7.32, 7.32] |
  | `l` | 4 | 5.93 s [5.91, 5.95] | 8.46 s [8.46, 8.50] |
  | `l` | 8 | 6.38 s [6.34, 6.39] | 10.65 s [10.59, 10.72] |

  From 2 to 8 ranks the distribution of the `l` mesh grows by 45 %
  (7.32 → 10.65 s) and the partition by 14 %.
  The serial Scotch partition alone (`baseline/scotch-*`, 8 parts): 840 ms
  at `m`, 6.43 s at `l` (RCB: 79 ms and 881 ms). The distribution grows
  with the rank count although each rank's package shrinks. Rank 0
  works alone: the other ranks wait in `MpiInFlight::finish` (nanosleep
  and `sched_yield` of the progress loop: 27 % of rank 1's samples).
- Profile of rank 0 (4 ranks, `l`): partition 30 % of the wall time,
  `build_dual_graph` 31 % of rank 0's samples, half of it in
  `BTreeMap<Element, usize>` entries and `Element::cmp`
  (`crates/akantu-mesh/src/partition.rs:366`); `Plan::package` 11 %.
- Defect, growth with the rank count: `Plan::package(p)` rebuilds
  `element_to_groups(mesh, ty)` (a map over every grouped element of the
  global mesh, `crates/akantu-mesh/src/distribution.rs:615`,
  `:771-780`) for every rank and type, and for every node group copies
  and scans its whole node list (`node_list()` at
  `distribution.rs:738`) for every rank: O(P·N) on rank 0 where the
  package itself is O(N/P). This matches the growth of `dist_mesh` with
  the rank count in `docs/BENCHMARKS.md` ("Set-up costs").
- Fix: dense element linear indices instead of a `BTreeMap` in the dual
  graph (the same CSR arrays reach Scotch, so the same partition);
  `element_to_groups` once per type; node groups mapped through one
  global → (rank, local) table. Expected gain: partition 2–3×, packaging
  independent of P.

### 5. Groups from node groups: quadratic (bitwise-safe)

- Case: `groups_tet4` element_group_from_node_group 164 ms at `m`,
  4.7 s per call at `l` on the sandbox profile (74 % self in
  `GroupManager::fill_from_node_group`); b = 1.87 between `m` and `l`
  (7.9 s at `l` on the development machine).
- Defect: `fill_from_node_group` tests each node of each candidate element
  with `nodes.contains(c)` on the node group's `Vec`
  (`crates/akantu-mesh/src/group.rs:1278`): O(group nodes × candidate
  elements). Fast at `m` only because the scan is vectorised.
- Fix: a boolean mask over the mesh nodes. Identical result. Expected gain:
  linear; 50× at `l`.

### 6. Per-step overhead of the serial communicator's collective bookkeeping (bitwise-safe)

- Cases: `py_twin_explicit_tri3` solve_step_tiny 8.1 µs per step on a
  4 × 4 square (32 triangles); the Python loop 9.64 µs (item 11).
- Profile: `Phases::agree_digest` 7 %, `Communicator::check_raw` 4 %,
  `ParkedScope::resume`, `Communicator::broken`, `std::thread::current`,
  `DofManager::settle` (together 15–20 % of the samples):
  the collective-agreement machinery of `akantu-comm` runs on every DOF
  manager step even with the serial communicator.
- Fix: a serial fast path (no agreement, no scope parking) when the
  communicator has one rank. Expected gain: 15–20 % of a small model's
  step; matters for small models, parameter sweeps and Python loops.

### 7. Per-step energies (`Math::reduce` sorts) (bitwise-safe; solid area)

- Case: `explicit_step_dist_tet4` at 4 ranks, `m`: the two energies of a step
  cost 4.26 ms per step against 7.95 ms for `solve_step`; on one rank
  15.9 ms against 13.4 ms: the energies cost more than the step
  (`baseline/mpi-m-p*`; profile: 44 % of the case at 4 ranks;
  `akantu_core::math::reduce` sorts every quadrature value per call with
  `sort_unstable_by(f64::total_cmp)`, `crates/akantu-core/src/math.rs:200`,
  plus the eigen-order `redux_tree` per point and an `ArrayStorage`
  resize).
- Reading: upstream's deterministic sum sorts too; the sorted order is
  unique (equal keys are equal bits, `total_cmp` orders ±0), so any
  correct sort, e.g. a radix sort on the `f64` bit patterns, or caching
  the potential-energy buffer, keeps every bit. Expected gain 2–3× on
  the energy phase. Belongs to the solid-model task; listed because every
  explicit example prints energies every step.

### 8. ParaView and text dumps: base64 and number formatting (bitwise-safe for the bytes)

- Cases: `dump_paraview_tet4` 200 ms per dump (162,000 Tet4, 5
  fields), `dump_paraview_ascii_tet4` 927 ms, `dump_text_tet4`
  73.6 ms (3 nodal fields).
- Profile: base64 mode: `Base64Writer::dump_to_buffer` 20 %,
  `Vtu::push_f64` 12 %, kernel page faults and copies. Text modes:
  `core::fmt` exact float formatting (`grisu::format_exact_opt`,
  `dragon::format_exact`, `Big32x40::mul_pow2`), `cfmt::format_e`, and a
  `format!` allocation per number (`format_inner`, `realloc`).
- Fix: write numbers into one reused buffer (no `String` per value), a
  table-driven base64 encoder over whole blocks, a direct `%e` formatter
  with the same bytes as C `printf` (proven against the current output on
  random and special values). Expected gain 2–3× per dump. The output
  bytes are fixed by the oracle comparisons, so a change is accepted only
  with byte-identical files.

### 9. Mesh reading: per-element string formatting and hashing (bitwise-safe)

- Cases: `msh22_read_tet4` read 144 ms, `msh41_read_tet4`
  115 ms, `msh22_read_hex8` 173 ms at `m`: 0.7–1.4 µs per
  element, about a tenth of the facet construction.
- Profile: the lexer's `Line::token` 9 %, `format!("tag_{j}")` for every
  tag of every element (`crates/akantu-io/src/mesh_io/msh.rs:450`) and the
  `String`-keyed lookup in `MeshBuild::set_int`
  (`mesh_io/staging.rs:165`), SipHash of `u64` node tags
  (`node_index`, `RandomState::hash_one` 4 %), sorting.
- Fix: per-block tag vectors indexed by tag number, an integer hasher for
  node tags (or a dense table when tags are contiguous). Expected gain
  1.5–2× on `read`.

### 10. Distributed exchanges and reductions (MPI layer)

- Cases: `ghost_nodes_tet4` (slave reduction + synchronize of 3 reals per
  shared node), `ghost_elements_tet4` (9 reals per ghost element),
  `reduce_scalars`, per call, size `m` (`baseline/mpi-m-p*`; at one rank
  the mesh stays serial and the exchanges have no peer):

  | operation | 1 rank | 2 ranks | 4 ranks | 8 ranks |
  |---|---|---|---|---|
  | node residual exchange (`ghost_nodes_tet4`) | – | 57 µs [54, 58] | 67 µs [65, 68] | 69 µs [67, 70] |
  | element exchange, 9 reals (`ghost_elements_tet4`) | – | 157 µs [155, 158] | 179 µs [175, 181] | 165 µs [162, 167] |
  | `all_reduce` of 2 reals | 0.55 µs | 1.36 µs | 2.42 µs | 4.7 µs |
  | `all_reduce` of 4,096 reals | 7.6 µs | 24.7 µs | 143 µs [58, 145] | 356 µs [349, 375] |
  | explicit `solve_step` | 13.4 ms | 9.70 ms | 7.95 ms | 7.67 ms |
  | two energies | 15.9 ms | 7.95 ms | 4.26 ms | 2.39 ms |

- Profile (4 ranks, `m`): `ReduceDataAccessor::unpack_data`, per-entity
  `accessor::record` bounds checks, `<f64 as Sealed>::reduce` (rank-order
  fold, D-039), `u64::pack_all`, `MpiInFlight::finish` and libc copies;
  `fold_in_rank_order` and the gather-based all-reduce in
  `reduce_scalars`.
- Readings and fixes: the per-entity packing goes through a bounds-checked
  record lookup and a generic `Packable` loop; packing contiguous ranges
  with one copy and reusing receive buffers (`docs/BENCHMARKS.md`, "MPI
  layer": one packing copy per send, a zeroed buffer and an unpacking copy
  per receive) is bitwise-safe. The rank-order fold of the all-reduce is
  the price of reproducible reductions (D-039): the all-reduce of 4,096
  reals costs 7.6 / 24.7 / 143 / 356 µs at 1 / 2 / 4 / 8 ranks (gathered
  on every rank and folded in rank order); `MPI_Allreduce` would be
  fast-mode. A reduce-to-root in rank order followed by a broadcast keeps
  the bits and moves P times less data (bitwise-safe).

### 10b. The distributed explicit step stops scaling after 4 ranks (open)

- Case: `explicit_step_dist_tet4`, size `m` (162,000 Tet4): `solve_step`
  13.4 / 9.70 / 7.95 / 7.67 ms at 1 / 2 / 4 / 8 ranks
  (`baseline/mpi-m-p*`), while the exchanges it contains cost well under
  0.1 ms per call (table of item 10). About 6.5 ms per step do not
  shrink with the rank count.
- Profile (4 ranks, rank 0): 26 % of the samples in one libc routine of
  the container (the symbol is misnamed, `xdr_int16_t`, and has no call
  chain: the image's libc has no unwind information), besides the
  element kernels. Whether this is copying of whole arrays per step, the
  progress loop's waiting, or load imbalance is not settled.
- Next step: profile at 8 ranks with the image's libc debug symbols (or
  frame pointers) and count the collectives per step
  (`Phases::agree_digest` runs on every DOF-manager step). A fix of a
  per-step O(N) copy or of collective bookkeeping is bitwise-safe.

### 11. Python layer

- `import akantu` 167 ms (`import numpy` 62 ms, an empty
  interpreter 8.9 ms).
- `solveStep` from Python: identical bits to the Rust loop (digest
  `7c3f18493f05f971` at `m` in both) and no measurable difference at `m`:
  4.721 ms [4.691, 5.371] per step against 4.705 ms [4.698, 4.838] in
  Rust, paired ratio 1.001 [0.996, 1.142] (interleaved A/B,
  `baseline/twin-ab`, 5 pairs). On the 4 × 4 mesh the call overhead is
  measurable: 9.64 µs [9.52, 10.30] against 8.02 µs [8.01, 8.06], ratio
  1.20 [1.19, 1.29], about 1.6 µs per call. Profile: the binding adds
  about 6 % of the samples, `ModelHandle::refresh` 4.7 %: after every
  call it re-registers every nodal view and re-exports internal and
  residual exports (`crates/akantu-python/src/py/handle.rs:551-592`). Fix:
  refresh only exports whose storage changed (revision counters).
  Bitwise-safe; matters only for very small models.
- Callbacks: a Python `DirichletFunctor` costs 28.5 ms per `applyBC`
  on 40,401 nodes (0.71 µs per node) against 0.29 ms for
  the built-in `FixedValue`; a Python `NeumannFunctor` 1.25 ms against
  0.44 ms on the 800 boundary segments; the custom-material
  example's Python elastic law 10.6 ms per step against 4.78 ms
  (same bits: the NumPy law gives the built-in law's digest at `m`).
  These are Python costs (one call per node or point); a vectorised
  functor API (one call per group with arrays) would remove most of them.
- Field access: `getDisplacement()` 0.25 µs per call (a view),
  `np.array(...)` copy 15.4 µs; a Python loop over nodes 21.3 ms
  against 0.47 ms vectorised. Already tight.

### Not hot

`erase_elements` (10.6 ms), `purify_mesh` (1.8 ms),
`make_periodic` (6.3 ms for x, y, z), `partition_rcb` (79 ms),
the parser (6.6 ms to parse 2,000 sections, 14.5 ms to evaluate all
16,003 parameters: about 0.9 µs per expression), the MSH writer
(54 ms). Models evaluate input expressions only at set-up, never per
step or per point.

## Fast-mode candidates

- `MPI_Allreduce` (native reductions) instead of the rank-order fold
  (`ReductionPolicy::NativeMpi` exists): `reduce_scalars` and every energy
  or residual-norm reduction.
- Unsorted (pairwise or Kahan) energy sums instead of `Math::reduce`'s
  sort.
- Threaded facet planning and threaded packaging of the distribution are
  bitwise-safe (independent outputs, ordered merge), not fast-mode.

## Upstream twins (for C++ timings by a later task)

Paths under `examples/` of the reference: `explicit_step_dist_tet4` ≈
`c++/solid_mechanics_model/explicit` (explicit lumped mass on Tet4);
`cohesive_insert_tet4` and the facet cases ≈ the set-up of
`c++/solid_mechanics_cohesive_model/cohesive_extrinsic`; `dist_mesh_tet4`
≈ `c++/solid_mechanics_model/parallel/parallel_2d.cc` and the `dist_mesh`
phase of the JOSS cohesive benchmark; `dump_paraview_tet4` ≈
`c++/solid_mechanics_model/io/dumper/dumper_low_level.cc`; `parser_dat` ≈
`c++/solid_mechanics_model/io/parser/example_parser.cc`;
`py_twin_explicit_tri3` and `explicit_tri3` ≈
`python/solid_mechanics_model/dynamics`; `material_tri3` ≈
`python/solid_mechanics_model/custom-material`.
