<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Changelog

All notable changes to akantu-rs are recorded in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html); before 1.0, a
change of default results or of a public API raises the minor version.

## [Unreleased]

### Added

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

### Changed

- `DofManager::assemble_elemental_matrices_to_matrix` keeps the entry
  positions of each element (4 bytes per assembled entry and per element
  equation, e.g. 360 bytes per `Tetrahedron4` element of a 3-D solid)
  from the second assembly into an unchanged profile on;
  `DofManager::set_assembly_cache(false)` turns this off.

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
