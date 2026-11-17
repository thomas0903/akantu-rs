<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Changelog

All notable changes to akantu-rs are recorded in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html); before 1.0, a
change of default results or of a public API raises the minor version.

## Unreleased

### Added

- The cohesive model runs on the execution policy's threads (D-123),
  bitwise at every thread count: the facet stresses and the insertion
  decision of the extrinsic check, the normals, openings and tractions of
  `cohesive_linear` and `cohesive_bilinear`, the cohesive residual
  (scattered in element order per node) and the energy update;
  `SolidMechanicsModelCohesive::{exec_policy, set_exec_policy}`. The
  insertion itself and the other cohesive laws stay serial
  (`docs/ARCHITECTURE.md`, "Execution policy").
- `akantu_geometry::GridIndex`: a uniform-grid search structure over the
  triangle primitives of the segment intersector that tests only the
  primitives near a query, in primitive order with the predicates of
  `LinearScan`. `EmbeddedInterfaceIntersector` uses it by default; its new
  field `exhaustive_search = true` selects `LinearScan`. The grid reports
  the scan's reports, in the scan's order, minus those of primitives
  farther from the query than its margin (a quarter cell). The scan
  reports such primitives only through rounding noise, when a query lies,
  up to rounding, in the plane of a distant face or on the line of a
  distant edge; the noise can be the query itself attached to the distant
  element. In the collected set (`remove_near_duplicates = false`) the grid
  can therefore keep another element for a duplicated piece: the one
  containing it, where the scan's deduplication keeps the lower, distant
  one. The grid's choice is upstream's: a probe of upstream's
  `MeshSegmentIntersector` gives the grid's pieces on such queries, and so
  does the port's CGAL-compatible AABB tree. With the default tiled
  assembly no difference was found.
- `akantu_contact::geometry::OutwardFrame::CurrentCached`,
  `OutwardCache` and `orthogonal_projection_in`: outward directions of the
  facets memoised for one set of positions; the contact detector uses them
  within a search (identical normals).
- `akantu_contact::geometry::outside_direction_with_nodes`: the reference
  outward direction with node arrays read once by the caller (the nodal
  areas of a search use it).
- `akantu_contact::GridSpacing` and `ContactDetector::grid_spacing`: the
  cell size of the detection grid. `Upstream` (default) keeps upstream's
  rule (√2 · the largest in-radius of every facet around a master node);
  `ContactSurface` takes the contact-surface facets only, for meshes whose
  side facets are much larger than the surface facets (the closest master
  node found can then differ); `ContactMechanicsModel::set_grid_spacing`.
- Execution policy (D-123): `akantu_core::exec` with `ExecPolicy`
  (precision mode `Bitwise`/`Fast`, thread count, SIMD level, split grain),
  `Exec` (a shared thread pool per thread count), `NodeScatter` and the
  SIMD dispatch convention. Environment variables `AKANTU_NUM_THREADS`
  (default 1), `AKANTU_PRECISION` (`bitwise`), `AKANTU_SIMD` (`auto`),
  `AKANTU_EXEC_GRAIN` (`auto`), `AKANTU_EXEC_SPIN_US` (5000; 0 when the
  threads outnumber the cores). Rust API:
  `SolidMechanicsModel::{exec_policy, set_exec_policy}`,
  `FeEngine::{exec, set_exec}`, `FeEngineRegistry::{exec, set_exec}`,
  `DofManager::{exec, set_exec}`, `ModelServices::exec` (default serial),
  `Material::independent_points` (default `false`) and
  `material::points_exec`. Python: `akantu.setNumThreads`,
  `getNumThreads`, `setPrecision`, `getPrecision`, `setSimdLevel`,
  `getSimdLevel` (the default of models created afterwards).
- Threads in the explicit step of `SolidMechanicsModel`, bitwise equal to
  one thread for every thread count: FE gradient, `BᵀD` and integration
  kernels, `grad_u −= eigen_grad_u` (for every law), elastic stresses and
  thermal strains, finite-deformation residual, internal-force scatter,
  external-work increment (per-node terms), Newmark-β predictor and
  corrector, lumped solve, residual scatter and solution gather (the
  per-node and per-equation loops only for parts of at least 131,072
  nodes / 262,144 equations).
- Matrix assembly, second round (bitwise at every thread count): the
  position cache keeps per-slot spans and one scatter plan per element
  filter (entry-sized parts, exact per-part block lists), and the DOF
  manager vouches for unchanged equations so a repeated assembly skips
  the per-block check (`BlockPositionCache::{vouch_equations,
  plan_ready, plan_after_serial_round}`, `akantu_linalg::sparse::EquationsKey`);
  on one thread the elemental matrices are produced and scattered chunk
  by chunk (`DofManager::assemble_elemental_matrices_by_chunks`,
  `akantu_solver::ElementalProducer`,
  `SparseMatrixAij::add_values_planned_chunk`). The cached repeated
  assembly of 162,000 four-node tetrahedra takes 25.5 ms on one thread
  (35.3 before) and 4.5 ms on 8 (Intel Xeon Platinum 8581C). Phase-field elemental matrices on
  threads (`FeEngine::compute_ntbn_integrated`), structural elemental
  stiffness and consistent mass on threads (`StructuralFeEngine::{exec,
  set_exec}`). Memory: 12 bytes more per stored slot (span and count),
  and per plan 4 bytes per block visit, 8 per slot and one per slot of
  the type; the solid model no longer keeps its whole-type elemental
  array once its assemblies are chunked (one thread).
  After a failed assembly the matrix is partly assembled (the chunks or
  element types before the failure are added), as it already was across
  laws.
- Threads in the per-point loops of more constitutive laws, bitwise equal
  to one thread for every thread count, also on the error path (the same
  error and the same law state as the serial loop):
  `elastic_orthotropic`, `elastic_anisotropic`, `neohookean` (stress and
  plane-stress `C33`), `plastic_linear_isotropic_hardening`,
  `plastic_drucker_prager`, `mazars`, `plastic_mazars`, `marigo`, the
  local loop of `mazars_non_local` / `marigo_non_local`,
  `anisotropic_damage` (experimental and corrected models),
  `viscoelastic_maxwell`, `sls_deviatoric`, `phasefield` (stress and
  tangent), and in the extra materials `visco_plastic`, `damage_linear`
  (per-point thresholds) and `ve_stiffness_prop`, with their per-step hooks (plastic work, dissipated energy,
  Maxwell branch update, thermal refresh). `docs/ARCHITECTURE.md`,
  "Execution policy", lists the laws and why the others stay serial.
- Threads in the explicit step of the heat transfer model (under the
  model's execution policy, see below): FE kernels, the heat law's
  diffusivity and `D∇u` loops, the internal-flow scatter in the serial
  order per node. The phase-field model's FE kernels run on its policy's
  threads too.
- Threads in matrix assembly and in the matrix operations of implicit
  steps, bitwise equal to one thread for every thread count: elemental
  stiffness (`FeEngine::compute_btdb_integrated`, `∫ BᵀDB` per element,
  bit for bit `compute_btdb` then `integrate`, used by the solid stiffness
  and the heat conductivity) and consistent mass on the FE engine's
  executor; the scatter of a repeated element assembly by entry ranges
  (every entry receives its contributions in element order:
  `SparseMatrixAij::add_values_cached_batch`,
  `LinearSystemBackend::add_values_cached_batch`,
  `BlockPositionCache::continues`, the diagnostic
  `DofManager::threaded_assemblies`); zeroing, scaling, entry-wise sums,
  profile copies, Dirichlet rows and the matrix-vector product
  (`SparseMatrixAij::{set_with, zero_with, mul_with, add_matrix_with,
  add_matrix_upstream_with, copy_profile_with, apply_boundary_on,
  mat_vec_mul_with}`, `LinearSystemBackend::set_exec`; the product
  gathers each row's terms in storage order, about 16 bytes per stored
  entry, built once per profile). `DofManager::set_exec` reaches the
  backend. `HeatTransferModel`, `PhaseFieldModel` and
  `StructuralMechanicsModel` take the process default policy and have
  `exec_policy` / `set_exec_policy` (the structural engine's elemental
  matrices stay serial). The first assembly of a matrix (its profile)
  stays serial. `FeEngine::field_matrix_elemental_into`: the consistent
  mass into caller-owned arrays (same checks, errors and values).
- `SolidMechanicsModel` keeps the arrays of its stiffness and
  consistent-mass assemblies between assemblies: one elemental-matrix
  array per element type (shared by the stiffness and the mass), the
  tangent, the gradient and the density values. For 162,000 four-node
  tetrahedra that is 187 MB of elemental matrices and 46 MB of tangent
  held after an assembly, which used to be allocated, page-faulted and
  freed at every assembly. The peak during an assembly is lower than
  before: the per-point `BᵀDB` array (`nb_element · nq · n²` values) is
  no longer built. `SolidMechanicsModel::release_static_solver` frees
  them; the next assembly allocates them again.
- Fast-mode harness `akantu_validation::fast_mode` with the rules of
  `fixtures/fast_mode/`, and its demonstrator (the gradient kernel with
  fused multiply-adds under `Precision::Fast`).
- `akantu_fe::engine::PointInterpolation`
  (`ElementalFieldInterpolation::point_interpolation`): the value of one
  interpolation point of one element of an elemental-field interpolation
  with one quadrature point per element, with the operations
  `interpolate_elemental_field` performs (`value`, or `coefficients` then
  `value_from_coefficients`).
- Python: a `DirichletFunctor` subclass may define
  `callNodes(nodes, flags, primal, coords)`; `applyBC` then calls it once
  per group with NumPy copies of the group's rows and writes `flags` and
  `primal` back after it returns, instead of calling `__call__` per node.
  Subclasses that define only `__call__` behave as before.
- `akantu_model::boundary_condition::DirichletFunctor::apply_shared_nodes`
  (default: `apply_shared` per node, `apply_shared_per_node`): the
  foreign-functor path of `apply_dirichlet` hands the whole node list to the
  functor.
- Benchmark suites: `suite_setup` case `parser_chain` (a chain of
  parameters that each refer twice to the previous one) and Python case
  `dirichlet_group_tri3`.
- The cohesive benchmark driver (`akantu-bench`, `cohesive_extrinsic`)
  writes `chrono_rank<r>.csv` (the phase times of every rank) and, for the
  cohesive model, `insertion_stats_rank<r>.csv` (the split of the
  insertion check) on every rank.
- `akantu_comm::Synchronizer::post_in` / `wait_in` and
  `akantu_comm::PostedExchange`: an owner → ghost exchange posted inside a
  running collective scope and completed later, with one agreement on the
  unpacking and the caller's work in between.
- `akantu_comm::phase::agreement_stats`: the number of agreements of the
  collective error protocol performed by the process and the time spent in
  them; `suite_setup`'s `explicit_step_dist_tet4` reports them per step.
- `akantu_core::math::sort_total`: an ascending sort by `f64::total_cmp`
  (the array of any correct sort; a radix sort for long slices).
- `akantu_core::eigen_kernels`: `SliceProduct` (a product of slice operands
  with its Eigen route, strides and kernel decided once; `eval` gives the
  bits of `eigen_gebp::product_into`), `product_slices`, and
  `fast_mode` (order-free dot and GEMM variants for the fast mode of
  D-122; not called by the library; hidden from the documentation and
  unstable: it will move behind the precision switch).
  `akantu_fe::eigen::prepare` (the
  `SliceProduct` of two `Dims`), `akantu_fe::kernels::{
  shape_derivatives_product, shape_derivatives_with}`.
- Bitwise SIMD kernels of the explicit step without `unsafe` outside
  `akantu-core`: `akantu_core::exec::{Lanes4, SimdTask, with_simd}` (four
  `f64` lanes with per-lane operations; a portable kernel body compiled
  for AVX2, without FMA, on `Avx2` and `Avx512` tokens) and
  `akantu_core::eigen_unrolled::tree_sum_of` (the halving tree of
  `tree_sum` over any per-lane type). `Lanes4::mul_add` for fast-mode
  kernels.
- `FeEngine::compute_btd_integrate` (`compute_btd` then `integrate` with
  the same checks and values, the point values integrated as they are
  computed), `FeEngine::{gradient_plan, btd_integrate_plan}` and
  `akantu_fe::engine::{GradientPlan, BtdIntegratePlan}` (the gradient and
  the integrated `BᵀD` checked once over a filter and run on element
  ranges, with the bits of the full calls).
- `akantu_core::eigen_order::{redux_tree_recursive, TREE_LEVELS_MAX}`;
  `akantu_core::eigen_gebp::route` is a `const fn`.
- Sparse assembly without per-entry hashing, bitwise equal to the former
  code: `akantu_linalg::sparse::BlockPositionCache` and
  `SparseMatrixAij::add_values_cached` (entry positions per element
  block, stored from the second assembly with unchanged positions),
  `SparseMatrixAij::{add_block_to_profile_and_values, would_promote}`
  (profile and first values in one pass), `EquationLayout::{try_local,
  is_identity}`, the module `akantu_linalg::int_hash` (a deterministic
  hasher for integer keys), `DofManager::{set_assembly_cache,
  assembly_cache_bytes}`, and the `LinearSystemBackend` methods
  `begin_cached_assembly`, `add_values_cached`,
  `can_fuse_profile_and_values` and `add_block_to_profile_and_values`
  (default implementations forward to the former methods).

- Release of the static solver's memory, off unless called (upstream
  keeps the factors): `SolidMechanicsModel::release_static_solver`
  (Python `SolidMechanicsModel.releaseStaticSolver`), built on
  `DofManager::{release_factorization, release_matrix,
  release_matrix_values}`, `LinearSystemBackend::{release_factorization,
  release_matrix_storage, release_matrix_values}` (PETSc: not supported),
  `SparseDirectSolver::release_factorization` (MUMPS: the collective
  `destroy`) and `SparseMatrixAij::{release_storage, release_values,
  is_released}`. Freed: the factors, `J`, `K`'s values and lookup map;
  kept: `K`'s coordinates and profile, so a later solve assembles,
  analyses and factorises again with the results of a run without the
  release, bit for bit. The cohesive
  benchmark driver uses it after the static preload with
  `rust_release_static_solver = true` (default false) and records
  `rss_after_release`. Manual: "Solver configuration", "Releasing the
  static solver's memory".
- `akantu_model::InternalField::prepare_overwrite` and
  `InternalField::commit_overwrite`: a caller that rewrites a whole block
  of an internal field announces it, or commits a block it computed
  elsewhere, so that the history save of the block becomes an exchange of
  allocations instead of a copy (see "Changed").
- `akantu_core::array_shared::SharedArray::as_unexported` and
  `SharedArray::try_swap_values` (the array of a block no foreign code can
  see, and an exchange of values with a plain array when it is not
  exported).
- `akantu_solid::Material::prepare_stress_overwrite` (default: nothing):
  `compute_all_stresses` calls it before `compute_stress`; a law announces
  there the blocks its `compute_stress` rewrites completely.
- `akantu_model::non_local::WeightFunction::weights_depend_only_on_distance`
  (default `false`; `true` for `base_wf`).

### Changed

- `akantu_solid::materials::damage::DamageParent::{stress_kernel,
  stress_kernel_from, parent_update_energies}`, `DamageFields::update_energies`
  and `PlasticFields::update_energies` take the executor of the calling law
  (`material::points_exec`) as their first argument.
- The laws listed above return `Error::DimensionMismatch` when a block of
  their per-point loops (gradient, stress, history) does not have the
  expected number of components per point (before, such a block was
  processed in chunks of the wrong size).
- `DofManager::split_solution_per_dofs` returns `Error::OutOfRange` for a
  local equation outside the solution vector, where it used to panic.
- `MaterialElastic::compute_stress` returns `Error::DimensionMismatch` when
  `grad_u` or the stress block does not have `dim × dim` components per
  point (before, such a block was processed in chunks of the wrong size).
- The explicit step of `SolidMechanicsModel` is faster with bitwise
  identical results at every thread count and SIMD level (0.52–0.83 of
  its time at one thread on an AMD EPYC 7B13, per mesh in
  `docs/benchmarks/perf-campaign/explicit/README.md`): the FE gradient,
  `BᵀD`, integration and finite-deformation residual kernels check their
  element ids and connectivity once per batch and run lane kernels,
  compiled for AVX2 when the CPU has it (the integrated `BᵀD` and the
  integrated residual run packed AVX2 arithmetic, the gradient partly, and
  the gradient with the `eigen_grad_u` subtraction of the block pass
  scalar AVX instructions); `BᵀD` and the finite-deformation residual are
  integrated as they are computed (no per-point array); for one elastic
  law on one element type of a mesh without ghosts, the gradient, the
  thermal pre-step, the stress and the integration run in one pass per
  block of elements that fits the L2 cache; the internal-force assembly
  reads the law filter without a copy; the element scatter indexes each
  node once.
- In `Precision::Fast` the integrated `BᵀD`, the integrated
  finite-deformation residual and the integration also use fused
  multiply-adds; the fast-mode case of the explicit step is
  `fast_mode/solid/explicit_fma_kernels` (was `explicit_fma_gradient`).
- `akantu-core` denies `unsafe` instead of forbidding it: the thread pool
  of `exec::spin` and the SIMD kernels of `exec::simd` (listed in its
  `UNSAFE.md`) are the only sites.
- The cohesive insertion check is faster with bitwise identical results:
  the facet stresses are computed directly from the bulk stresses (the
  by-element buffer is no longer written per step; it is still allocated
  by the first check), the elemental interpolation selects its products'
  route once per block, the copy plan after an insertion is built in
  linear time, and the linear law's effective norm uses fixed-size loops
  (`docs/BENCHMARKS.md`, "The insertion check").
- The cohesive insertion step and the mesh operations under it are faster
  with bitwise identical results, serial and distributed
  (`docs/BENCHMARKS.md`, "The insertion step"): in release builds a mesh
  transaction that starts from a state that passed the cross-part check
  checks the adjacency references only in the rows it replaced or appended
  (debug builds keep the full check; `SharedMesh::set_full_consistency_checks`
  selects it explicitly); element-group node lists and the node
  synchronizer are journaled instead of snapshotted; the stress copy plan
  follows an insertion instead of being rebuilt; the cohesive laws keep
  their residual and energy work arrays between steps;
  a distributed check without any flagged facet skips the insertion-order
  exchanges; the DOF manager's global-number exchange indexes the
  equations of each node once; `plan_facets`, `transfer_physical_names`
  and `fill_from_node_group` lost their quadratic searches.
- `SolidMechanicsModelCohesive::ext` is a `Box<CohesiveExtension>` (field
  access unchanged through `Deref`; a destructuring pattern needs
  `&mut *model.ext`): the model is 11.6 kB instead of 38 kB by value, and
  the benchmark suite's insertion case runs in a debug `mumps` build on a
  1.5 MiB stack instead of 2 MiB.
- `CohesiveSynchronizers::element_to_prank` is an
  `Arc<ElementTypeMapArray<Rank>>` (shared, never changed after
  construction) instead of an `ElementTypeMapArray<Rank>`.
- Mesh distribution and the communication layer are faster with
  bitwise identical results (`docs/BENCHMARKS.md`, "Distribution and the
  communication layer"): the dual graph of the partitioners counts shared
  nodes in an array over linearised elements (same graph, same Scotch
  partition), the distribution's group tables are built once instead of
  per rank, checked collectives of the MPI back-end carry payloads of at
  most 64 bytes in their header (agreements and scalar reductions are one
  MPI collective), byte messages are sent and received without a copy,
  agreements on a one-rank serial or in-process communicator skip the all-reduce, `Math::reduce` sorts with a
  radix sort, and the solid model posts its `_smm_stress` exchange before
  the not-ghost assembly and completes it before the ghost one (as
  upstream), keeps one assembly buffer set per ghost type and looks up the
  law blocks of an exchange once per run of elements.
- The default sparse direct solver (`akantu_linalg::eigen_sparse_lu`, the
  translation of Eigen's SparseLU) is about 3× faster on 3-D problems with
  bitwise identical results: dense kernels in register tiles with run-time
  AVX / AVX-512F instances in the same operation order, storage reused
  between factorisations, less memory (`int` row subscripts as in Eigen;
  factor arrays grow to the length in use).
- `akantu_linalg::eigen_sparse_lu::SparseLu::solve_with` (the caller's
  scratch buffer) and the opt-in fast mode `SparseLu::set_fast_mode` /
  `EigenSparseLuSolver::set_fast_mode`, enabled with an
  `akantu_core::exec::FastToken` (fused multiply-adds in the panel
  products; not bitwise, off by default).
- The sparse solver follows the DOF manager's executor (D-123): the AIJ
  backend hands it to the solver of every solve
  (`SparseDirectSolver::set_exec`; `SparseLu::set_exec` outside a model).
  With several threads the large panel updates of the Eigen SparseLU
  translation run on them, with the same factors and solutions at every
  thread count; in precision `Fast` its FAST MODE kernels run (fused
  multiply-adds also in the triangular panel solves and the GEMV kernels;
  rule `fixtures/fast_mode/linalg/tolerances.json`). Its dense kernels take
  the executor's SIMD level (`AKANTU_SIMD`); the environment variable
  `AKANTU_SPARSELU_ISA` is gone. A DOF manager built without a model
  (`DofManager::new`, `with_backend`) takes the process default policy
  instead of one thread, and falls back to one thread when the `AKANTU_*`
  variables are invalid (it does not fail); a model sets its own as before.
- Small dense products and Eigen-order reductions are faster with bitwise
  identical results: the halving-tree reduction is non-recursive and
  inlined, fixed-size tensor products select their route at compile time,
  the FE products of slices run fixed-order kernels (one per route and
  depth class), and the FE per-element loops decide each product once.
- Constitutive laws and the heat law are faster with bitwise identical
  results: the `EigenSolver` eigenvalues of fixed-size matrices up to
  4 × 4 (`akantu_linalg::dense::eig_fixed`, `eig3_fixed`,
  `eig_vectors_fixed`; Mazars' principal strains) run the Hessenberg
  reduction and the Francis iteration on stack arrays; the Maxwell and
  anisotropic laws use constant-size Voigt products and the Maxwell
  branch factors are computed once per block; the fixed-size matrix
  inverse no longer allocates; the heat law computes `∇u` and `D∇u` in
  place, skips the interpolation of the temperature when the
  conductivity cannot depend on it (`conductivity_variation = 0`, finite
  temperatures) and keeps its temporary arrays between steps
  (`docs/benchmarks/perf-campaign/laws/`). Two error paths of the heat law
  differ: `∇u` is written into its block, so an error part-way through
  the gradient leaves the entries of the elements before it written (as
  upstream's `computeGradU` does); the constant-conductivity path does not
  run the interpolation of `T` and its filter and length checks, so it
  sets the diffusivity and updates its release where an invalid filter
  used to return an error there (the same operation still fails right
  after, in the gradient or in `compute_btdb`).
- `akantu_fe::eigen::product_slices` checks the operands' inner
  dimension before the slice lengths: a call with both wrong now reports
  the inner-dimension mismatch (still `Error::DimensionMismatch`, with
  another context). Errors of `SliceProduct` carry their own context
  strings (`SliceProduct ...`).
- `FeEngine::field_matrix_elemental` (consistent mass) integrates each
  element's point matrices as they are computed instead of materialising
  all of them first (memory `nq·(nb_dof·nn)²` instead of
  `ne·nq·(nb_dof·nn)²` reals; 35 k `_tetrahedron_10` needed 4.4 GB).
- `DofManager::assemble_elemental_matrices_to_matrix` keeps the entry
  positions of each element (4 bytes per assembled entry and per element
  equation, e.g. 360 bytes per `Tetrahedron4` element of a 3-D solid)
  from the second assembly into an unchanged profile on;
  `DofManager::set_assembly_cache(false)` turns this off.
- The explicit step of the solid model repeats less work, with bitwise
  identical results:
  - `InternalField::save_current_values` defers the copy of a block whose
    current values are not exported: the previous values are the current
    ones until the block is written; the copy is skipped when nothing
    writes the block before the next save and becomes an exchange of
    allocations when the writer rewrites the whole block (the gradient and
    the stresses of most laws). A block with a live NumPy view is copied
    at once, as before, and never exchanged, so views keep their
    allocation and see the same values at the same times (D-047).
    `InternalField::previous` of a deferred block returns the current
    block, with its id.
  - The plastic, Marigo, standard linear solid and Maxwell laws compute
    their blocks into scratch storage kept between steps and commit them
    by exchange, instead of cloning their inputs and results at every
    step; the elastic potential energy does the same, and the Neo-Hookean
    and anisotropic kernels read their inputs in place.
  - A non-local neighbourhood whose weight function depends on the
    distance only keeps its weights while the radius, points, pairs,
    slots, integration weights and incoming volumes are unchanged, instead
    of recomputing them at every stress computation (`update_rate = 1`).
  - `compute_all_stresses` no longer copies the element filter per step.
- The input-file parser evaluates each variable once per evaluation (it
  parsed a variable's expression again at every reference, so a chain of
  k parameters that each refer twice to the previous one cost 2^k
  parses: 21 lines 400 ms, now 7 µs, measured on an AWS m7a.2xlarge (AMD
  EPYC 9R14)); values and error messages are unchanged.
- Dumps and the MSH writer format numbers into the output buffer instead
  of one `String` per number, with a faster exact digit generator for up
  to 17 significant digits; ParaView `.vtu` pieces are validated first and
  then written to the file through a buffer (no copy of the whole file in
  memory), base64 arrays encoded in one pass. The written bytes are
  unchanged. On 162,000 Tet4: a base64 ParaView dump 208 → 99 ms, ASCII
  913 → 296 ms, a text dump 74 → 17 ms, the MSH writer 54 → 26 ms
  (this entry's and the next two entries' timings: measured on an AWS
  m7a.2xlarge (AMD EPYC 9R14); `docs/benchmarks/perf-campaign/io-python/`).
- The Gmsh reader keeps node and element tags in dense tables, allocates
  nothing per element, parses integers in one pass and sorts dense group
  node lists through a bitmap; the meshes read are unchanged (162,000
  Tet4: 153 → 85 ms in MSH 2.2).
- Python Dirichlet functors (`__call__` per node) enter Python once per
  group instead of once per node (40,401 nodes: 31.5 → 23.4 ms per
  `applyBC`; with `callNodes` 1.0 ms).

### Added (insertion step)

- `SharedMesh::set_full_consistency_checks(bool)` and
  `MeshTransaction::replace_node_synchronizer`.

## [0.1.0] - Unreleased

The initial public state.

### Added

- A Rust port of the Akantu finite-element library at upstream commit
  `35a85178` (`v5.0.7-post1-554-g35a851780`), in 27 crates with the facade
  crate `akantu`.
- Of the 1,045 rows of the feature parity matrix, 1,019 are verified and 26
  are approved exceptions (`docs/FEATURE_PARITY.md`). On 14 November 2026
  every workflow of the CI ran once on the public repository and all 30
  jobs passed, among them the macOS arm64 wheel and the documentation
  deployment (`docs/TESTING.md`, "State at release"). An earlier run on the
  then private repository had failures whose causes were fixed before
  publication.
- The user manual and the Rust and Python API documentation, deployed from
  `main` to <https://thomas0903.github.io/akantu-rs/>. The manual's
  formulas are typeset (KaTeX, rendered when the book is built), and it
  shows the figures of the upstream manual next to images of results
  computed with akantu-rs. Building it needs mdBook 0.4.40 and
  mdbook-katex 0.9.4.
- The models of upstream: solid mechanics (static, explicit and implicit
  dynamics, finite deformation), cohesive elements, contact mechanics, heat
  diffusion, phase field, structural mechanics, the embedded interface, the
  couplers, and the extra packages (extra materials, IGFEM, split-node
  contact).
- Distributed-memory runs with MPI and Scotch partitioning; sparse direct
  solvers: `eigen` (pure Rust, a bitwise translation of Eigen's SparseLU;
  the default `auto` uses it unless MUMPS is built in), `faer` (pure Rust,
  opt-in), MUMPS and PETSc.
- Gmsh and Diana mesh readers, ParaView and text dumpers, the `.dat` input
  files of upstream.
- A Python package following the API of upstream's `akantu` module (abi3
  wheels for Linux and macOS, built from source).
- Differential tests against golden data from the C++ library, most of them
  bitwise; every other tolerance is justified in `docs/TOLERANCE_LEDGER.md`.
- A register of the defects and questionable behaviours found in the C++
  library, 596 entries, with what the port does about each
  (`docs/UPSTREAM_DEFECTS.md`); a corrected default keeps a switch that
  reproduces upstream's behaviour where that is practical.
- An opt-in GPL-3.0-or-later crate (`akantu-geometry-cgal`, feature `cgal`)
  with the parts ported from CGAL; the default build is LGPL-3.0-or-later.
