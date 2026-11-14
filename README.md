<img src="assets/icon.svg" width="64" height="64" alt="">

# akantu-rs

akantu-rs is a Rust port of [Akantu](https://gitlab.com/akantu/akantu), the
finite-element library for solid mechanics, dynamic fracture (cohesive
elements) and contact developed at EPFL (LSMS). It follows upstream commit
`35a851780d0b9471d258ed53a003ae98aaa95ea0` (`v5.0.7-post1-554-g35a851780`)
and keeps upstream's input files, output formats and behaviour, with
floating-point results identical bit for bit wherever the computation allows
it. Like Akantu, it is free software under the LGPL-3.0-or-later.

## State

<!-- BEGIN GENERATED: parity-stats (scripts/parity/stats.py --update) -->
The feature parity matrix
[`docs/FEATURE_PARITY.md`](docs/FEATURE_PARITY.md) has 1,066 rows, 21 of
them superseded by finer rows. Of the other 1,045: 1,019 *Verified* and 26
approved exceptions (100.0 % together), 0 *Implemented, unverified*, 0 *In
progress*, 0 *Blocked* and 0 *Not started*.
<!-- END GENERATED: parity-stats -->

A row is *Verified* when differential tests compare the port with golden
fixtures recorded from the C++ library (bit for bit, or within a tolerance
justified in writing), independent mathematical checks apply where they can
(patch tests, closed forms, convergence rates), and a reviewer who did not
write the code has accepted the evidence. Every job of the CI, including
the macOS wheel and the documentation deployment, ran green on GitHub on
14 November 2026 ([`docs/TESTING.md`](docs/TESTING.md), "State at
release"). What is validated less (packages that do not compile upstream,
configurations refused by default, comparisons weaker than bit identity) is
listed in [`docs/VALIDATION_SUMMARY.md`](docs/VALIDATION_SUMMARY.md). The
manual's [differences](docs/manual/src/differences.md) page describes how
the port differs from the C++ library and the switches that restore
upstream's behaviour.

The port was written by AI coding agents directed by Thomas Molinari; each
feature was reviewed before its merge by an agent that had not written it.
The reports for the Akantu team in [`docs/reports/`](docs/reports/)
describe how: the final
report (`00-final-report`), the migration and its features
(`01-migration-and-features`), benchmarks against the C++ library
(`02-benchmarks-and-performance`), the defects found in the C++ library
(`03-upstream-defects`), possible improvements (`04-possible-improvements`)
and the AI setup (`05-ai-setup`), each as `.md` and `.pdf`.

## Quick start

The default build is pure Rust: it needs only the toolchain pinned in
`rust-toolchain.toml`, which `rustup` installs on the first `cargo` command.

```sh
cargo build --workspace
cargo test --workspace                       # fast test tier
cargo test -p akantu --doc                   # every Rust example of the manual
cargo run -p akantu --example static         # writes ./paraview/static_*
```

The `static` example is upstream's `examples/c++/solid_mechanics_model/static`
with the same input file and mesh; the 27 other examples are listed in the
manual's [examples](docs/manual/src/examples.md) page. A first program
written against the library, with each line next to its C++ counterpart, is
in [Getting started](docs/manual/src/getting-started.md#a-first-simulation).
To use the library from another Cargo project, depend on the facade crate
by path (the crates are not on crates.io):

```toml
[dependencies]
akantu = { path = "../akantu-rs/crates/akantu" }
```

The Python package keeps upstream's module names (`import akantu`). It is
not published on PyPI; build it from source with maturin (Python 3.9 or
later):

```sh
python3 -m venv .venv
.venv/bin/pip install maturin numpy scipy pytest greenlet matplotlib
cd python && ../.venv/bin/maturin develop --release
../.venv/bin/python -m pytest -q tests
```

The medium and expensive test tiers (golden fixtures, MPI at 1 to 8 ranks
in Docker) are described in [`docs/TESTING.md`](docs/TESTING.md).

## What is in the box

| Area | Content | Manual |
|---|---|---|
| Core | `Array` and small tensors, element types, the input-file parser and command-line options | [basic types](docs/manual/src/basic-types.md), [input files](docs/manual/src/input-files.md) |
| Meshes and I/O | Gmsh and Diana readers, Gmsh writer, groups, mesh data, periodicity; ParaView, text and LAMMPS dumpers | [meshes](docs/manual/src/meshes.md), [output](docs/manual/src/output.md) |
| Finite elements | regular, cohesive and structural element types, quadrature, the FE engine | [elements](docs/manual/src/appendix/elements.md) |
| Models | solid mechanics (static, implicit and explicit dynamics, finite deformation), cohesive elements and fragmentation, contact mechanics, phase-field fracture, diffusion, structural mechanics, embedded interfaces, the model couplers, split-node contact with rate-and-state friction, IGFEM | [solid mechanics](docs/manual/src/solid-mechanics.md) and the model pages |
| Materials | elastic, plastic, damage, viscoelastic, Neo-Hookean and phase-field laws, non-local laws, the extra materials, laws written in Rust or Python | [materials](docs/manual/src/materials.md), [new laws](docs/manual/src/new-constitutive-laws.md) |
| Solvers | DOF manager, Newton-Raphson and time-step solvers, the pure-Rust sparse solvers `eigen` (a translation of Eigen's `SparseLU`) and `faer`, MUMPS, PETSc (KSP, SNES, TAO) | [solver configuration](docs/manual/src/solver-configuration.md), [MUMPS](docs/manual/src/mumps.md), [PETSc](docs/manual/src/petsc.md) |
| Parallel | MPI communicator, mesh distribution with Scotch, ghost elements, distributed solid, cohesive, diffusion and phase-field models | [parallel computations](docs/manual/src/parallel.md) |
| Python | the `akantu` package: meshes, parser, FE engine, boundary conditions, solvers, every model and coupler, Python materials, NumPy views without copies | [Python interface](docs/manual/src/models/python.md) |

The default features of the facade crate `akantu` are `solid`, `cohesive`,
`contact`, `diffusion`, `phase-field`, `couplers`, `structural` and
`embedded`. Native back-ends (MPI, Scotch, MUMPS, PETSc, Python) and the
extra packages are opt-in:

<!-- BEGIN GENERATED: availability-readme -->
Behind a Cargo feature that is off by default: split-node contact (`ntn-contact`), CGAL geometry (`cgal`), MUMPS (`akantu-linalg/mumps`), MPI back-end (`akantu-comm/mpi`), MPI runtime (`mpi`), Scotch (`akantu-mesh/scotch`), Scotch partitioning of the examples (`scotch`), Python interface (`akantu-python/python`), IGFEM (`igfem`), extra materials (`extra-materials`), non-local extra materials (`extra-materials-non-local`), PETSc (`akantu-solver/petsc`), split-node contact on a distributed mesh (`ntn-contact`) and Python examples (`python`).

Not in this release, and none of them is being ported: contact mechanics on a distributed mesh, structural model on a distributed mesh and C API (the reason for each: [models not yet available](docs/manual/src/models/not-yet-available.md)).
<!-- END GENERATED: availability-readme -->

The module table with crates and features, and what each native back-end
needs at build time, are in
[Getting started](docs/manual/src/getting-started.md#the-facade-crate-akantu).

## Platforms

The reference platform for bitwise comparisons with the C++ library is
x86_64 Linux. macOS on Apple silicon is a supported development platform:
the system libm gives a listed set of one-ulp differences in the input-file
parser and may differ in the last bit at a few other call sites. Part of
the fixtures were first recorded on aarch64 Linux. The details are in
[`docs/PLATFORMS.md`](docs/PLATFORMS.md) and in the manual's section on
[bitwise reproducibility and platforms](docs/manual/src/differences.md#bitwise-reproducibility-and-platforms).

## Documentation

| Document | Content |
|---|---|
| [User manual](docs/manual/src/introduction.md) | mdBook (`mdbook build docs/manual`), organised like upstream's manual; every Rust example in it is a doc-test |
| [Differences from the C++ library](docs/manual/src/differences.md) | corrected defaults, compatibility switches, API conventions |
| [`docs/FEATURE_PARITY.md`](docs/FEATURE_PARITY.md) | the parity matrix: every upstream feature, its Rust counterpart, status and evidence |
| [`docs/VALIDATION_SUMMARY.md`](docs/VALIDATION_SUMMARY.md) | how the port was verified, per area, and what is validated less |
| [`docs/NUMERICAL_VALIDATION.md`](docs/NUMERICAL_VALIDATION.md), [`docs/TOLERANCE_LEDGER.md`](docs/TOLERANCE_LEDGER.md), [`docs/MPI_VALIDATION.md`](docs/MPI_VALIDATION.md) | the detail: fixtures, tolerances and their reasons, distributed runs |
| [`docs/UPSTREAM_DEFECTS.md`](docs/UPSTREAM_DEFECTS.md) | the register of defects found in the C++ library and what the port does with each |
| [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) | measured performance, Rust against C++ |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`docs/DECISIONS.md`](docs/DECISIONS.md) | crates, contracts between them, technical decisions |
| [`docs/TESTING.md`](docs/TESTING.md) | the test tiers and how to run them |
| [`CONTRIBUTING.md`](CONTRIBUTING.md), [`docs/MAINTAINING.md`](docs/MAINTAINING.md) | how to contribute and how to keep the evidence current |

The API reference is built with `cargo doc --workspace --no-deps --open`.
The manual and the Rust and Python API documentation of `main` are served
at <https://thomas0903.github.io/akantu-rs/>.

## Licence

LGPL-3.0-or-later: akantu-rs is a derivative work of Akantu (© EPFL, LSMS).
See [`COPYING`](COPYING), [`COPYING.lesser`](COPYING.lesser),
[`NOTICE.md`](NOTICE.md) and the third-party notices in
[`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md).

One exception, off by default: the crate `crates/akantu-geometry-cgal`
contains code derived from GPL-licensed CGAL packages and is licensed
GPL-3.0-or-later. It is compiled only when the facade feature `cgal` is
enabled (the feature `igfem` implies it). A build with `cgal` or `igfem`
enabled is distributed under GPL-3.0-or-later as a whole; the default build
contains no GPL code.

The same holds for a build with the PETSc back-end, also off by default
(feature `petsc` of `akantu-linalg` / `akantu-solver`): the port's code
stays LGPL-3.0-or-later, but Debian's PETSc (the project's images) links
GPL-2.0-or-later libraries (SuiteSparse's UMFPACK and SPQR and the
Supernodal, MatrixOps, Modify and GPU modules of CHOLMOD; FFTW; possibly
CombBLAS's `psort` code through SuperLU_DIST), so a binary built with
`petsc` against such a PETSc is distributed under GPL-3.0-or-later as a
whole (D-076; details in `NOTICE.md` and in
[`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md), section "`petsc`:
GPL-3.0-or-later for binaries linked against Debian's PETSc").

## How to cite

Please cite the upstream library (machine-readable metadata for the port:
[`CITATION.cff`](CITATION.cff)):

Richart et al., (2024). *Akantu: an HPC finite-element library for contact
and dynamic fracture simulations.* Journal of Open Source Software, 9(94),
5253, [10.21105/joss.05253](https://doi.org/10.21105/joss.05253)

