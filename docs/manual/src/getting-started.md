# Getting started

## Requirements

* The Rust toolchain pinned in `rust-toolchain.toml` (currently 1.99.1,
  edition 2024); `rustup` installs it automatically on the first `cargo`
  command in the repository.
* Nothing else for the default build: it is pure Rust and needs no native
  library, no Python and no MPI (upstream needs CMake, Boost and Eigen;
  its sequential Eigen sparse solver, added in 5.0.4, covers
  implicit solves on one process, and parallel implicit solves need MUMPS
  or PETSc: `AKANTU_IMPLICIT_SOLVER`, `packages/implicit.cmake`).
* Optional native back-ends, never part of the default build:

  | Cargo feature | Crates | Needs | Upstream equivalent |
  |---|---|---|---|
  | `mpi` | the facade `akantu`, `akantu-comm`, `akantu-mesh`, `akantu-solid`, `akantu-cohesive`, `akantu-diffusion`, `akantu-couplers`, `akantu-extra-materials`, `akantu-ntn-contact`, `akantu-python`, `akantu-bench` | an MPI implementation and `libclang` at build time | `AKANTU_PARALLEL` (MPI part) |
  | `scotch` | the facade `akantu`, `akantu-mesh`, `akantu-solid`, `akantu-cohesive`, `akantu-diffusion`, `akantu-couplers`, `akantu-extra-materials`, `akantu-ntn-contact`, `akantu-bench` | `libscotch` | `AKANTU_PARALLEL` (Scotch part) |
  | `mumps` | `akantu-linalg`, `akantu-solver`, `akantu-solid`, `akantu-bench` | MPI, a C compiler and the MUMPS development files | `AKANTU_IMPLICIT_SOLVER = Mumps` ([The MUMPS sparse solver](mumps.md)) |
  | `python` | `akantu-python`, the facade `akantu` (the example `python_user_defined_bc`) | Python 3.9 or later, NumPy and maturin | the `akantu` Python package ([Python interface](models/python.md)) |
  | `petsc` | `akantu-linalg`, `akantu-solver`, `akantu-solid`, `akantu-phase-field`, `akantu-couplers` | MPI, `pkg-config` and the PETSc development files (PETSc 3.17 or later, real scalars, 32-bit indices) | `AKANTU_IMPLICIT_SOLVER` with PETSc: the PETSc DOF manager of the models ([The PETSc back-end](petsc.md); a binary built with it against Debian's PETSc is GPL-3.0-or-later as a whole) |

  The C API is not part of this release ([Models not yet
  available](models/not-yet-available.md)).

## Building and testing

```sh
cargo build --workspace            # all crates, default features
cargo test --workspace             # fast test tier (unit, integration, doc-tests)
cargo test -p akantu-solid         # one crate
cargo test -p akantu --doc         # the examples of this manual
cargo test -p akantu --test manual # input snippets, generated tables, links
cargo run -p akantu --example static  # an upstream example, writes ./paraview/static_*
```

The examples ported from upstream's `examples/c++` are listed in
[Examples](examples.md).

The test tiers (fast, medium with golden fixtures and MPI ranks,
expensive) and the scripts that run the MPI tiers inside Docker are
described in [`docs/TESTING.md`](../../TESTING.md).

The manual itself is an [mdBook](https://rust-lang.github.io/mdBook/):
`mdbook build docs/manual` writes the HTML book to `docs/manual/book`.

## Optimised builds and the target CPU

`cargo build --release` uses the workspace release profile: `opt-level =
3`, no LTO, 16 codegen units, line-table debug information and the
x86-64 baseline instruction set (SSE2). Neither the profile nor any
code-generation flag enables fast-math or FMA contraction: Rust compiles
every floating-point operation as written, whatever the target CPU.

A build for a newer instruction-set level gives the same results, bit for
bit, as the default build on x86_64 Linux:

```sh
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release   # AVX2 (Haswell, Zen 1 and later)
RUSTFLAGS="-C target-cpu=native" cargo build --release      # the CPU of the build machine
```

What was checked, with the pinned toolchain (rustc 1.99.1) in the project
image on x86_64 Linux:

| `target-cpu` | fast tier (`cargo test --workspace`) and release tier | explicit-step and cohesive-driver results |
|---|---|---|
| `x86-64-v2` | pass | identical bits |
| `x86-64-v3` | pass | identical bits |
| `x86-64-v4` (AVX-512) | pass | identical bits |
| `znver4` (`native` on an AMD EPYC 9R14) | pass | identical bits |

The release-profile variants `codegen-units = 1`, `lto = "thin"`, `lto =
"fat"`, their combinations, `panic = "abort"` and profile-guided
optimisation also give identical results. `f64::min` and `f64::max`
return the same bits at every level (signed zeros included), and so do
the arithmetic operations. The only operation whose result can change
with the instruction set is the choice between two NaN operands: which
of the two an addition returns depends on the operand order the compiler
chooses. That choice also changes between debug and release builds, and
the comparisons of the test suite treat it as D-046 describes (two
default NaNs that differ only in their sign match). Data, scripts and
the timings: `docs/benchmarks/perf-campaign/build-profile/`.

On the measured AMD Zen 4 machine, `x86-64-v3` and `native` took 0.96 to
0.98 times the time of the default build for the explicit step, while
`x86-64-v4` was slower on hexahedral meshes. Prefer `x86-64-v3` or
`native` to `x86-64-v4`. A binary built for a level runs only on CPUs
that have it: the distributed Python wheels use the baseline. A build of
the Python extension for the local CPU:

```sh
cd python && RUSTFLAGS="-C target-cpu=native" maturin build --release
```

The statement holds for the pinned toolchain. After a toolchain update,
`scripts/perf/build/ci_matrix.sh` repeats the check (the CI tiers built
with each configuration). Other platforms (aarch64, macOS) were not
checked.

## The facade crate `akantu`

The workspace is split into crates (`akantu-core`, `akantu-parser`,
`akantu-mesh`, `akantu-fe`, `akantu-solid`, …; see
[`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md)). Applications depend on
the facade crate `akantu`, which re-exports every subsystem as a module and
selects the optional physics crates with Cargo features named after the
upstream CMake packages:

<!-- BEGIN GENERATED: availability-modules -->
| Module | Crate | Feature | Content |
|---|---|---|---|
| `akantu::core` | `akantu-core` | — | Core types: `Array`, small tensors, element types, enums, errors ([Basic types and iteration](basic-types.md)) |
| `akantu::parser` | `akantu-parser` | — | Input files, parameters, command-line options ([Input files](input-files.md)) |
| `akantu::comm` | `akantu-comm` | — | Communicators (serial, in-process, MPI) and synchronizers ([Parallel computations](parallel.md)) |
| `akantu::mesh` | `akantu-mesh` | — | Meshes, groups, mesh data, mesh changes, distribution ([Meshes](meshes.md)) |
| `akantu::fe` | `akantu-fe` | — | Element classes, quadrature, FE engine (with the structural element kernels) ([Appendix: elements](appendix/elements.md)) |
| `akantu::linalg` | `akantu-linalg` | — | Sparse matrices, sparse direct solvers (`eigen`, `faer`), dense algebra |
| `akantu::solver` | `akantu-solver` | — | DOF manager, non-linear and time-step solvers ([Solver configuration](solver-configuration.md)) |
| `akantu::io` | `akantu-io` | — | Mesh readers and writers, dumpers ([Output and dumpers](output.md)) |
| `akantu::model` | `akantu-model` | — | Model base, boundary conditions, constitutive-law framework |
| `akantu::geometry` | `akantu-geometry` | — | Geometric queries on meshes (segment intersector), spatial grid ([Geometry and the `cgal` feature](geometry.md)) |
| `akantu::solid` | `akantu-solid` | `solid` | The solid mechanics model `SolidMechanicsModel` and the solid materials ([Solid mechanics model](solid-mechanics.md)) |
| `akantu::cohesive` | `akantu-cohesive` | `cohesive` | The cohesive model `SolidMechanicsModelCohesive` (one process or a distributed mesh): cohesive laws, element inserter, fragment manager ([Cohesive elements and fragmentation](models/cohesive.md)) |
| `akantu::contact` | `akantu-contact` | `contact` | The contact mechanics model `ContactMechanicsModel` (one process): detection, penalty resolutions with Coulomb friction ([Contact mechanics](models/contact.md)) |
| `akantu::phase_field` | `akantu-phase-field` | `phase-field` | The phase-field fracture model `PhaseFieldModel` ([Phase-field fracture](models/phase-field.md)) |
| `akantu::structural` | `akantu-structural` | `structural` | The structural mechanics model `StructuralMechanicsModel` (beams and plates, one process) ([Structural mechanics model](models/structural.md)) |
| `akantu::ntn_contact` | `akantu-ntn-contact` | `ntn-contact` (off by default) | Split-node contact with rate-and-state friction (one process or a distributed mesh) ([Split-node contact and rate-and-state friction](models/ntn-contact.md)) |
| `akantu::geometry_cgal` | `akantu-geometry-cgal` | `cgal` (off by default) | The GPL-3.0-or-later geometry of CGAL's packages: AABB tree, sphere intersector ([Geometry and the `cgal` feature](geometry.md#the-cgal-feature-gpl-30-or-later)) |
| `akantu::diffusion` | `akantu-diffusion` | `diffusion` | The diffusion (heat transfer) model `DiffusionModel` (serial and distributed) ([Diffusion (heat transfer)](models/diffusion.md)) |
| `akantu::couplers` | `akantu-couplers` | `couplers` | The model couplers (solid–contact, solid–cohesive–contact, solid–phase-field) ([Model couplers](models/couplers.md)) |
| `akantu::embedded` | `akantu-embedded` | `embedded` | Embedded interfaces (reinforcements) ([Embedded interfaces](models/embedded.md)) |
| `akantu::igfem` | `akantu-igfem` | `igfem` (off by default) | IGFEM (interface-enriched elements, types `_igfem_*`; elastic and damage materials, moving interfaces on a live model, the `igfem elements` dumper; serial; the feature `igfem` implies the GPL-3.0-or-later feature `cgal`) ([IGFEM (interface-enriched finite elements)](models/igfem.md)) |
| `akantu::extra_materials` | `akantu-extra-materials` | `extra-materials` (off by default) | The extra materials (damage iterative, Vreepeerlings, FE², …) ([Extra materials](models/extra-materials.md)) |
<!-- END GENERATED: availability-modules -->

<!-- BEGIN GENERATED: availability-placeholders -->
<!-- END GENERATED: availability-placeholders -->

The structural element kernels are in `akantu::fe::structural`
([Structural mechanics model](models/structural.md)). The default features are `solid`,
`cohesive`, `contact`, `diffusion`, `phase-field`, `couplers`,
`structural` and `embedded`. Compared with upstream:

| Feature | Upstream package | Upstream default | Port default |
|---|---|---|---|
| `cohesive`, `contact`, `diffusion`, `phase-field` | `cohesive_element`, `contact_mechanics`, `diffusion`, `phase_field` | on | on |
| `couplers` | `model_couplers` | off, pulled in by `contact_mechanics` and `phase_field` | on |
| `structural` | `structural_mechanics` | off in the package, on in the `common` CMake preset | on |
| `embedded` | `embedded` | off (needs CGAL) | on (no native dependency) |
| `ntn-contact` | `traction_at_split_node_contact` (extra package) | off, on in the `common` preset | off |
| `extra-materials`, `igfem` | `extra_materials`, `IGFEM` (extra packages) | off | off |
| `cgal` | `CGAL` (external) | off | off; makes the build GPL-3.0-or-later ([Geometry](geometry.md#the-cgal-feature-gpl-30-or-later)) |
| `mumps` of `akantu-linalg` (no facade feature) | `AKANTU_IMPLICIT_SOLVER = Mumps` (the default of the `implicit` package) | on with `implicit` | off; pure-Rust `eigen` by default ([MUMPS](mumps.md)) |

```rust
// The facade exposes the port's version and the upstream version it tracks.
assert_eq!(akantu::UPSTREAM_VERSION, "v5.0.7-post1-554-g35a851780");
assert_eq!(akantu::VERSION, "0.1.0");
```

The crates are not published on crates.io (`publish = false`). Use the
facade from another Cargo project with a path or git dependency:

```toml
[dependencies]
akantu = { path = "../akantu-rs/crates/akantu" }
```

## A first simulation

The program below is the Rust counterpart of a minimal upstream `main`:
it reads the mesh [`square.msh`](meshes/square.msh) (a unit square of four
quadrangles with the physical groups `left`, `right`, `bottom`, `top` and
`body`) and the input file [`elastic.dat`](inputs/elastic.dat), stretches
the square by 1 % in a static analysis and checks the result against the
analytical solution.

```dat
{{#include inputs/elastic.dat}}
```

```rust
use std::path::Path;

use akantu::core::enums::{AnalysisMethod, MeshIoType, SpatialDirection};
use akantu::io::read_mesh;
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::model::boundary_condition::FixedValue;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;

# let manual = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manual/src");
# let mesh_file = manual.join("meshes/square.msh");
# let input_file = manual.join("inputs/elastic.dat");
// upstream: Mesh mesh(2); mesh.read("square.msh");
let mut mesh = Mesh::new(2, "mesh")?;
read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

// upstream: initialize("elastic.dat", argc, argv) fills the static parser
let parser = Parser::parse_file(&input_file)?;

// upstream: SolidMechanicsModel model(mesh); model.initFull(_analysis_method = _static);
let mesh = SharedMesh::new(mesh);
let mut model =
    SolidMechanicsModel::with_parser(mesh.clone(), None, "solid_mechanics_model", parser)?;
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;

// upstream: model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left"); …
let fixed = |value, axis| FixedValue { value, axis };
model.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::X), "left")?;
model.apply_dirichlet_bc(&fixed(0.0, SpatialDirection::Y), "bottom")?;
model.apply_dirichlet_bc(&fixed(0.01, SpatialDirection::X), "right")?;

model.solve_step(None)?;

// Plane strain, free lateral face: u_y(y = 1) = -ν/(1-ν) · 0.01.
{
    // a read guard on the displacement, released at the end of the block
    let u = model.displacement()?;
    let top_right = u.tuple(8); // node 9 of the file, at (1, 1)
    let expected = -0.3 / 0.7 * 0.01;
    assert!((top_right[0] - 0.01).abs() < 1e-14);
    assert!((top_right[1] - expected).abs() < 1e-14 * expected.abs().max(1.0));
}

// Elastic energy ½ σ:ε V with σ_xx = E/(1-ν²) ε_xx.
let energy = model.energy("potential")?;
let exact = 0.5 * 210e9 / (1.0 - 0.09) * 1e-4;
assert!((energy - exact).abs() <= 1e-12 * exact);
# Ok::<(), akantu::core::Error>(())
```

Bilinear quadrangles reproduce this linear displacement field exactly (a
patch test), so the only differences from the analytical values are
round-off errors of a 9-node system (`1e-14` relative is three orders of
magnitude above them).

Compared with the C++ program:

* errors are values (`Result<_, akantu::core::Error>`), not exceptions;
* the mesh is owned by a `SharedMesh` handle shared by the model and its
  dumpers instead of being referenced by pointers;
* the parsed input file is passed to the model explicitly. The
  process-global parser of upstream (`getStaticParser()`) exists as
  `akantu::runtime::global()` for programs that prefer it
  ([Command-line and environment options](command-line.md));
* node and element numbers start at 0, as in C++ (the Gmsh file numbers
  its nodes from 1).
