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

### Changed

- The cohesive insertion check is faster with bitwise identical results:
  the facet stresses are computed directly from the bulk stresses (the
  by-element buffer is no longer written per step; it is still allocated
  by the first check), the elemental interpolation selects its products'
  route once per block, the copy plan after an insertion is built in
  linear time, and the linear law's effective norm uses fixed-size loops
  (`docs/BENCHMARKS.md`, "The insertion check").
- The input-file parser evaluates each variable once per evaluation (it
  parsed a variable's expression again at every reference, so a chain of
  k parameters that each refer twice to the previous one cost 2^k
  parses: 21 lines 400 ms, now 7 µs); values and error messages are
  unchanged.
- Dumps and the MSH writer format numbers into the output buffer instead
  of one `String` per number, with a faster exact digit generator for up
  to 17 significant digits; ParaView `.vtu` pieces are validated first and
  then written to the file through a buffer (no copy of the whole file in
  memory), base64 arrays encoded in one pass. The written bytes are
  unchanged. On 162,000 Tet4: a base64 ParaView dump 208 → 99 ms, ASCII
  913 → 296 ms, a text dump 74 → 17 ms, the MSH writer 54 → 26 ms
  (`docs/benchmarks/perf-campaign/io-python/`).
- The Gmsh reader keeps node and element tags in dense tables, allocates
  nothing per element, parses integers in one pass and sorts dense group
  node lists through a bitmap; the meshes read are unchanged (162,000
  Tet4: 153 → 85 ms in MSH 2.2).
- Python Dirichlet functors (`__call__` per node) enter Python once per
  group instead of once per node (40,401 nodes: 31.5 → 23.4 ms per
  `applyBC`; with `callNodes` 1.0 ms).

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
