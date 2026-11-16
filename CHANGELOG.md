<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Changelog

All notable changes to akantu-rs are recorded in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html); before 1.0, a
change of default results or of a public API raises the minor version.

## Unreleased

### Added

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
- Fast-mode harness `akantu_validation::fast_mode` with the rules of
  `fixtures/fast_mode/`, and its demonstrator (the gradient kernel with
  fused multiply-adds under `Precision::Fast`).
- `akantu_fe::engine::PointInterpolation`
  (`ElementalFieldInterpolation::point_interpolation`): the value of one
  interpolation point of one element of an elemental-field interpolation
  with one quadrature point per element, with the operations
  `interpolate_elemental_field` performs (`value`, or `coefficients` then
  `value_from_coefficients`).
- The cohesive benchmark driver (`akantu-bench`, `cohesive_extrinsic`)
  writes `chrono_rank<r>.csv` (the phase times of every rank) and, for the
  cohesive model, `insertion_stats_rank<r>.csv` (the split of the
  insertion check) on every rank.
- `akantu_core::eigen_kernels`: `SliceProduct` (a product of slice operands
  with its Eigen route, strides and kernel decided once; `eval` gives the
  bits of `eigen_gebp::product_into`), `product_slices`, and
  `fast_mode` (order-free dot and GEMM variants for the fast mode of
  D-122; not called by the library; hidden from the documentation and
  unstable: it will move behind the precision switch).
  `akantu_fe::eigen::prepare` (the
  `SliceProduct` of two `Dims`), `akantu_fe::kernels::{
  shape_derivatives_product, shape_derivatives_with}`.
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

- `DofManager::split_solution_per_dofs` returns `Error::OutOfRange` for a
  local equation outside the solution vector, where it used to panic.
- `MaterialElastic::compute_stress` returns `Error::DimensionMismatch` when
  `grad_u` or the stress block does not have `dim × dim` components per
  point (before, such a block was processed in chunks of the wrong size).
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
- Small dense products and Eigen-order reductions are faster with bitwise
  identical results: the halving-tree reduction is non-recursive and
  inlined, fixed-size tensor products select their route at compile time,
  the FE products of slices run fixed-order kernels (one per route and
  depth class), and the FE per-element loops decide each product once.
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
  - The plastic, Marigo, Mazars, standard linear solid and Maxwell laws
    compute their blocks into scratch storage kept between steps and
    commit them by exchange, instead of cloning their inputs and results
    at every step; the elastic potential energy, the Neo-Hookean and
    anisotropic kernels read their inputs in place.
  - A non-local neighbourhood whose weight function depends on the
    distance only keeps its weights while the radius, points, pairs,
    slots, integration weights and incoming volumes are unchanged, instead
    of recomputing them at every stress computation (`update_rate = 1`).
  - `compute_all_stresses` no longer copies the element filter per step.

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
  `main` to <https://thomas0903.github.io/akantu-rs/>.
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
