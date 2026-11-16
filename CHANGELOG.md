<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Changelog

All notable changes to akantu-rs are recorded in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html); before 1.0, a
change of default results or of a public API raises the minor version.

## Unreleased

### Added

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
- `akantu_core::eigen_order::{redux_tree_recursive, TREE_LEVELS_MAX}`;
  `akantu_core::eigen_gebp::route` is a `const fn`.

### Changed

- The cohesive insertion check is faster with bitwise identical results:
  the facet stresses are computed directly from the bulk stresses (the
  by-element buffer is no longer written per step; it is still allocated
  by the first check), the elemental interpolation selects its products'
  route once per block, the copy plan after an insertion is built in
  linear time, and the linear law's effective norm uses fixed-size loops
  (`docs/BENCHMARKS.md`, "The insertion check").
- Mesh distribution and the communication layer are faster with
  bitwise identical results (`docs/BENCHMARKS.md`, "Distribution and the
  communication layer"): the dual graph of the partitioners counts shared
  nodes in an array over linearised elements (same graph, same Scotch
  partition), the distribution's group tables are built once instead of
  per rank, checked collectives of the MPI back-end carry payloads of at
  most 64 bytes in their header (agreements and scalar reductions are one
  MPI collective), byte messages are sent and received without a copy,
  agreements on one rank skip the all-reduce, `Math::reduce` sorts with a
  radix sort, and the solid model posts its `_smm_stress` exchange before
  the not-ghost assembly and completes it before the ghost one (as
  upstream), keeps one assembly buffer set per ghost type and looks up the
  law blocks of an exchange once per run of elements.
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
