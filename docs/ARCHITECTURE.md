# Architecture

This document describes how akantu-rs is built: the crates and their layers,
how the C++ design of Akantu maps to Rust, the types every crate shares, the
storage and ownership rules, the model and solver stack, mesh mutation, the
parallel layout, the optional native back-ends and the rules that keep the
results bitwise equal to the C++ library. It is written for a contributor who
knows the C++ library and has to find their way in the 27 crates.

The reference is Akantu at commit `35a851780d0b9471d258ed53a003ae98aaa95ea0`
(`docs/REFERENCE.md`). Upstream paths below are relative to the root of the
upstream repository. Decisions are cited by their id in `docs/DECISIONS.md`
(D-…), upstream defects by their id in `docs/UPSTREAM_DEFECTS.md` (UD-…).
The code is the normative form of every interface named here; rustdoc gives
the full signatures.

## Principles

1. Upstream is the specification. Behaviour, numbering, iteration order,
   input and output formats and names follow upstream unless a decision
   record says otherwise. Upstream defects are handled by D-003: category A
   (undefined behaviour given a defined result), B (a clear bug, corrected by
   default with a switch that reproduces upstream) or C (upstream behaviour
   kept and documented).
2. Results are reproducible bit for bit. Every loop that accumulates
   floating-point values has a defined order taken from upstream; there is no
   fused multiply-add, no reassociation and no unordered parallel reduction
   (see "Rules for numerical code"). Numerical code runs single-threaded; native
   back-ends run with fixed, recorded thread counts (D-017).
3. Components own their data. Where upstream objects hold references to each
   other (model, DOF manager and solvers; mesh and event handlers; materials
   and model; NumPy and arrays), a Rust component owns its data and receives
   the others as parameters, or as a context that lives for one operation.
   Topology changes are transactions over every registered consumer.
4. Serial and MPI runs share one code path. All code is written against
   `akantu_comm::Communicator`. The serial back-end is the default, an
   in-process multi-rank back-end runs the distributed paths in ordinary
   tests, and MPI is a Cargo feature. Collective operations agree on failure
   before they communicate.
5. The default build is pure Rust. `cargo build --workspace` and
   `cargo test --workspace` need no native library and no Python. MPI, MUMPS,
   PETSc, Scotch and Python are opt-in features.
6. Hot loops are monomorphised. Dynamic dispatch happens once per
   `(element type, ghost type)` block, never per element or quadrature point.

## Workspace and layers

All crates live in `crates/` (`members = ["crates/*"]`), edition 2024, with
`publish = false` and the workspace lints. The dependency graph below is
derived from `cargo metadata --no-deps` (normal dependencies between
workspace crates, transitive edges removed; `docs/reports/data/crates.csv`).
The layer of a crate is its longest dependency path from `akantu-core`.

```text
layer 0   akantu-core                      akantu-validation (dev-dependency only)
layer 1   akantu-comm   akantu-parser
layer 2   akantu-mesh   akantu-linalg
layer 3   akantu-fe   akantu-geometry   akantu-solver
layer 4   akantu-io   akantu-geometry-cgal (GPL, opt-in)
layer 5   akantu-model
layer 6   akantu-solid   akantu-diffusion   akantu-phase-field   akantu-contact   akantu-structural
layer 7   akantu-cohesive   akantu-embedded   akantu-extra-materials   akantu-igfem   akantu-ntn-contact
layer 8   akantu-couplers   akantu-bench
layer 9   akantu (facade)
layer 10  akantu-python   akantu-capi
```

Notable edges: solver depends on linalg, mesh and parser; io on fe and
linalg; model on io and solver; embedded on solid and geometry; igfem on
solid and, optionally, geometry-cgal; couplers on cohesive, contact and
phase-field. The facade depends on every library crate (not on `akantu-bench`,
`akantu-python`, `akantu-capi` or `akantu-validation`), the physics crates
as optional dependencies behind features. A crate depends only on crates of
a lower layer, and `akantu-validation` is never a normal dependency.

Every crate is LGPL-3.0-or-later except three cases: `akantu-core` and
`akantu-linalg` contain MPL-2.0 files translated from Eigen (package
licence `LGPL-3.0-or-later AND MPL-2.0`), `akantu-geometry-cgal` is
GPL-3.0-or-later, and `akantu-igfem` is LGPL code that becomes part of a
GPL build when its feature `cgal` is on (see `docs/DEPENDENCIES.md`).

| Crate | What it holds | Main public items | Upstream source |
|---|---|---|---|
| `akantu-core` | index types, `Error`, element-type enums and tables, arrays and shared arrays, element-type maps, small tensors with Eigen's evaluation order, Voigt helpers, `BBox`, `Csr`, `SpatialGrid`, math helpers, ported glibc libm, random engine and distributions, debug output | `Array`, `SharedArray`, `ArrayRef`, `ArrayMut`, `SliceRef`, `ElementTypeMapArray`, `Element`, `ElementType`, `Error`, `tensor::{Matrix, Vector, MatrixX}` | `src/common`, element tables, `src/mesh/element_type_map*` |
| `akantu-parser` | `.dat` parser, typed parameters and registry, command-line parser | `Parser`, `ParserSection`, `ParameterRegistry`, `Parsable`, `ByteString`, `cli::ArgumentParser` | `src/io/parser` |
| `akantu-comm` | communicator (serial, local world, MPI), owned requests, phase scopes, MPI lifecycle, generic synchronizer | `Communicator`, `run_local`, `phase::Phases`, `lifecycle::Lifecycle`, `Synchronizer`, `DataAccessor` | `src/synchronizer` (generic part) |
| `akantu-mesh` | `Mesh`, the shared mesh domain, staged mutation and events, groups, mesh data, utilities, periodicity, distribution, all mesh synchronizers | `Mesh`, `SharedMesh`, `MeshTransaction`, `MeshEvent`, `MeshEventHandler`, `DistributedHook`, `GroupManager`, `MeshPartitioner` | `src/mesh`, `src/mesh_utils` (except the cohesive inserter), mesh `src/synchronizer` |
| `akantu-linalg` | COO sparse matrix, equation layout, sparse direct solvers (Eigen `SparseLU` translation, faer, MUMPS, PETSc), dense eigen-decompositions | `SparseMatrixAij`, `EquationLayout`, `SparseDirectSolver`, `SparseSolverFactory`, `dense::{eig, eigh}` | `src/solver`, `eig`/`eigh` of `aka_types.hh` |
| `akantu-fe` | element classes, shape functions, Gauss quadrature, FE engine, structural FE engine | `ElementClass`, `LagrangeElement`, `with_element_class!`, `FeEngine`, `gauss_rule`, `structural::StructuralFeEngine` | `src/fe_engine` |
| `akantu-geometry` | ports of CGAL's LGPL kernel, intersections and exact number types, mesh geometry factory, segment intersector, clean-room models of two libstdc++ algorithms | `MeshGeomFactory`, `MeshSegmentIntersector`, `LinearScan`, `number::{MpFloat, Quotient}`, `cxx` | `src/geometry` (LGPL parts) |
| `akantu-geometry-cgal` | ports of CGAL's GPL `AABB_tree`, `Circular_kernel_3` and `Algebraic_kernel_for_spheres`, the mesh-sphere intersector | `AabbTree`, `spherical::*`, `MeshSphereIntersector` | `src/geometry` (CGAL GPL parts) |
| `akantu-solver` | DOF manager, linear-system back-ends, solver callbacks, non-linear and time-step solvers, integration schemes, DOF synchronizer | `DofManager`, `LinearSystemBackend`, `AijBackend`, `PetscBackend`, `SolverCallback`, `NonLinearSolver`, `TimeStepSolver`, `ModelSolver` | `src/model/common/{dof_manager,non_linear_solver,time_step_solvers,integration_scheme}`, `model_solver.*` |
| `akantu-io` | mesh readers and writers (Gmsh, Diana), the IOHelper port (ParaView, text, LAMMPS), dump descriptors, group dumpers | `read_mesh`, `Dumper`, `FieldDescriptor`, `FieldResolver`, `dumper::group::MeshDumpers` | `src/io/mesh_io*`, `src/io/dumper`, `third-party/iohelper` |
| `akantu-model` | `Model` trait, boundary conditions, constitutive-law framework, internal fields, runtime parameters, non-local toolbox, strain-energy splits, coupling, Neumann load records | `Model`, `ConstitutiveLaw`, `ModelServices`, `LawContext`, `ConstitutiveLawFactory`, `InternalField`, `non_local::NonLocalManager` | `src/model/model*`, `src/model/common/*`, `energy_split*` |
| `akantu-solid` | solid mechanics model, `Material` trait, all solid materials, weight functions, runtime (Python) materials | `SolidMechanicsModel`, `Material`, `MaterialBase`, `MaterialFactory`, `ForeignMaterial`, `SmmUpstreamCompat` | `src/model/solid_mechanics` (except cohesive and embedded) |
| `akantu-diffusion` | diffusion and heat transfer model and laws | `DiffusionModel`, `HeatTransferModel` | `src/model/diffusion_model` |
| `akantu-phase-field` | phase-field model and laws | `PhaseFieldModel`, `PhaseField` | `src/model/phase_field` |
| `akantu-contact` | contact mechanics model: detection, resolutions with friction, selectors | `ContactMechanicsModel`, `ContactDetector` | `src/model/contact_mechanics` |
| `akantu-structural` | structural mechanics model (Bernoulli beams, DKT shells) | `StructuralMechanicsModel`, `StructuralMaterial` | `src/model/structural_mechanics` |
| `akantu-cohesive` | cohesive solid model, cohesive laws, cohesive element inserter (serial and parallel), fragment manager | `SolidMechanicsModelCohesive`, `CohesiveElementInserter`, `MaterialCohesive`, `FragmentManager` | `src/model/solid_mechanics/solid_mechanics_model_cohesive`, `src/mesh_utils/cohesive_element_inserter*` |
| `akantu-embedded` | embedded interface model and reinforcement materials | `EmbeddedInterfaceModel`, `EmbeddedInterfaceIntersector`, `MaterialReinforcement` | `src/model/solid_mechanics/solid_mechanics_model_embedded_interface` |
| `akantu-extra-materials` | the `extra-materials` package, including FE² | `MaterialFE2`, `Fe2Config`, `ExtraLawOptions`, `SolidMechanicsModelRve` | `extra_packages/extra-materials` |
| `akantu-igfem` | IGFEM elements, FE engine, model, materials, growing-gel generator | `SolidMechanicsModelIgfem`, `IgfemFeEngine` | `extra_packages/igfem` |
| `akantu-ntn-contact` | traction-at-split-node contact and friction | `NtnBaseContact`, `NtnFriction`, `SyncJournal` | `extra_packages/traction-at-split-node-contact` |
| `akantu-couplers` | solid-contact, solid-cohesive-contact and solid-phase-field couplers | `CouplerSolidPhaseField`, the contact couplers | `src/model/model_couplers` |
| `akantu-bench` | the Rust port of the cohesive scalability benchmark driver (`cohesive_extrinsic`) | `driver::run`, `Config` | `performance-testing-cohesive` (upstream benchmark) |
| `akantu` | facade: re-exports every crate, Cargo features matching the CMake packages, the process-global runtime, examples | `runtime`, `prelude` | `aka_common.cc`, `aka_extern.cc`, `examples/` |
| `akantu-python` | the PyO3 extension module `py11_akantu` (feature `python`) and the foreign-view machinery | `foreign_view`, `scoped` | `python/` |
| `akantu-capi` | one exported C function, `akantu_version()`; the C interface planned by D-024 is not built (D-100) | `akantu_version` | none (upstream has no C API) |
| `akantu-validation` | golden-fixture format, loader, tolerance rules and differential comparison against the C++ oracle | `Fixture`, `Manifest`, `ToleranceSpec`, `compare_f64`, `DiffTest` | `oracle/` of this repository |

### Upstream packages and their Rust location

Upstream's CMake packages map to facade features of `crates/akantu`:
`solid`, `cohesive`, `contact`, `diffusion`, `phase-field`, `couplers`,
`structural` and `embedded` are on by default (upstream preset `common`);
`extra-materials`, `extra-materials-non-local`, `igfem`, `ntn-contact`,
`cgal`, `mpi`, `scotch` and `python` are off. `damage_non_local` is the
default feature `damage-non-local` of `akantu-solid`; `Mumps` and `PETSc`
are features of the crates that use them (see "Optional native back-ends
and Cargo features"). Upstream's `akantu_iterators` became standard
iterator adaptors (idiom table in
`crates/akantu-core/tests/test_iterators.rs`), Boost became enums, macros
and the parser crate, Eigen and IOHelper were ported. Upstream tests map
to per-crate `tests/`, upstream examples to `crates/akantu/examples/` and
`python/examples/`; `docs/FEATURE_PARITY.md` records every mapping.

### Breaking upstream's include cycles

Upstream's include graph is cyclic; the crate graph breaks each cycle.
Element tables live in `akantu-core`, so the mesh needs no FE code. Generic
synchronization lives in `akantu-comm` below the mesh, mesh-specific
schemes in `akantu-mesh`. Dumpers take the mesh as a parameter
(`Mesh : Dumpable` is gone). `akantu-linalg` knows neither mesh nor DOFs.
The FE engine returns elemental arrays and the model assembles them
through the DOF manager. Global statics became explicit parameters with
process-wide defaults in `akantu::runtime` (D-023).

## How the C++ design maps to Rust

Behaviour that upstream gets from virtual inheritance comes from traits,
closed sets of variants are enums, and back-references became parameters
and operation-scoped contexts. Classes whose Rust type has the same name
and role (`Mesh`, `MeshAccessor`, `Element`, `ElementType`,
`ElementTypeMapArray`, `ElementGroup`, `NodeGroup`, `GroupManager`,
`Parser`, `ParserSection`) are not listed.

| Upstream class | Rust counterpart | Notes |
|---|---|---|
| `Array<T>`, `ArrayProxy` | `Array<T>`, `ArrayView`, `ArrayViewMut`; `SharedArray<T>` when exported to NumPy | see "Storage" |
| `Vector`, `Matrix`, `Tensor3` (Eigen maps) | `tensor::{Vector<N>, Matrix<R, C>, MatrixX}`, `as_matrix` views | column-major, Eigen order (D-011) |
| mesh event handlers (`registerEventHandler`) | `SharedMesh` consumer registry and `MeshEventHandler` | events are transaction data (D-019) |
| `ElementClass<type>`, `tuple_dispatch` | one zero-sized type per element type, `with_element_class!`, `with_dim!` | |
| `FEEngineTemplate<IntegratorGauss, ShapeLagrange>` | `FeEngine` | |
| `FEEngineTemplate<…, ShapeStructural, _ek_structural>` | `structural::StructuralFeEngine` | separate engine (D-049) |
| `SparseMatrixAIJ`, `SolverVector` | `SparseMatrixAij`, `SolverVector` | global equation coordinates |
| `DOFManagerDefault`, `DOFManagerPETSc` | `DofManager` over a `Box<dyn LinearSystemBackend>` (`AijBackend`, `PetscBackend`) | |
| `SparseSolverEigen`, `SparseSolverMumps`, `SparseSolverPETSc` | `EigenSparseLuSolver`, `MumpsSolver`, PETSc KSP through `PetscBackend`; port extension `FaerLuSolver` | D-040 |
| `NonLinearSolver*`, `TimeStepSolver*`, `IntegrationScheme*` | `NonLinearSolver` trait objects, `TimeStepSolver`, `IntegrationScheme` trait | |
| `Model`, `ModelSolver`, `SolverCallback` | `Model` trait; each model is `{ physics, solver: ModelSolver, token }` with the physics implementing `SolverCallback` | D-021 |
| `ConstitutiveLaw`, `Material`, `Material<dim>` | `ConstitutiveLaw` (akantu-model), `Material` (akantu-solid), `MaterialX<const DIM>` behind `Box<dyn Material>` | D-016 |
| `ConstitutiveLaw::getModel()` | `LawContext { phase, services: &dyn ModelServices }` | operation-scoped |
| `Parameter`, `ParameterRegistry`, `Parsable` | `ParameterRegistry` (static tables), `DynamicParameters` (runtime laws), `Parsable` | |
| `InternalField<T>` | `InternalField<T>`, `DynamicInternals` | |
| `MaterialFactory`, `ConstitutiveLawSelector` | `ConstitutiveLawFactory`, `ConstitutiveLawSelector` | factories are values, not singletons |
| `NonLocalManager`, `NonLocalNeighborhood`, `BaseWeightFunction` | same roles in `akantu-model::non_local`, injected callbacks from the physics crates | |
| `Communicator`, `StaticCommunicator` | `Communicator` (cloneable handle) | serial, local world, MPI |
| `Synchronizer`, `SynchronizerImpl<E>`, `DataAccessor<E>` | `Synchronizer<E>`, `DataAccessor<E>` | |
| `ElementSynchronizer`, `NodeSynchronizer`, `FacetSynchronizer`, `GridSynchronizer`, `PeriodicNodeSynchronizer` | `akantu-mesh` synchronizer modules | all in the mesh crate |
| `CohesiveElementInserter`, `CohesiveMeshGlobalDataUpdater` | `CohesiveElementInserter`, `CohesiveGlobalDataUpdater` (a `DistributedHook`) | |
| `Dumpable`, IOHelper `Dumper*` | `Dumper` trait, `DumperParaview`, `DumperText`, `DumperLammps`, `FieldResolver` | |
| `RandomGenerator`, `std::default_random_engine` | `akantu_core::random` | clean-room (D-006, D-030) |

## Fundamental types

Every crate uses these types. A contributor never introduces another index,
element or array type.

### Scalars and indices (D-012)

| Type | Definition | Use |
|---|---|---|
| `Real` | `f64` | upstream `Real` |
| `Int`, `UInt` | `i32`, `u32` | payload integers |
| `Idx` (aliases `NodeIdx`, `ElemIdx`, `DofIdx`) | `i32` | stored local index: node, element within a `(type, ghost)` block, local DOF or equation; sentinel `INVALID_IDX = -1` |
| `GlobalIdx` | `i64` | global node and element ids and global equation numbers; sentinel `-1` |
| `Rank` | `i32` | MPI rank; `NO_RANK = -1` |
| counts and in-memory arithmetic | `usize` | |

The widths are fixed. `i32` local indices are upstream's default `Idx`, the
dtype of upstream's NumPy connectivity exports and carry upstream's `-1`
sentinel, so foreign buffers are shared without conversion. Cargo features
must be additive, which rules out a width switch like upstream's optional
64-bit `Idx`. Global ids and equations are `i64`, signed like the native
APIs they feed, which lifts upstream's 2³¹ global limit. Local counts above
2³¹ − 1 per rank and entity kind, and local DOFs above that bound, are
refused with `Error::CapacityExceeded`. Conversions go through checked
helpers in `akantu_core::types` (`try_idx`, `decode_idx`, `encode_idx`,
`checked_local_dofs`, `try_global_idx`, `to_one_based_i32` for MUMPS and
32-bit PETSc), never a bare `as` on untrusted values. Every size product
coming from users, files or foreign code goes through a fallible
constructor (`Array::try_filled`, `try_resize`, `try_reserve`, …) that
returns the error before any state changes.

### Element types and `Element`

`ElementType` has every upstream type in upstream order (regular, cohesive,
structural and IGFEM types plus `NotDefined`). All variants always exist;
features gate implementations, not variants. Static `const` tables give the
kind, dimensions, node and shape counts, geometry, interpolation, Gauss
family, facet types and facet connectivities (upstream's column-major
arrays verbatim), the cohesive type of a facet and the structural and IGFEM
data. Names (`_triangle_3`, `_ek_regular`, `_not_ghost`) match upstream,
and only names appear in persisted or exchanged formats (D-010). Parsing a
name is an exact lookup, as upstream's `operator>>` (`GhostType` also
accepts the name without its leading underscore, a port extension). Iteration
over element types is always in enum order, which is the order of
upstream's `std::map<ElementType, …>`.

`Element { ty, ghost_type, index }` has private fields. Every value is
either the canonical `Element::NULL` or valid, so the derived `Eq`, `Hash`
and `Ord` agree; the ordering is upstream's `operator<`. `NodeFlag` keeps
upstream's bit values and predicates.

### Errors (D-018)

`akantu_core::Error` is one `#[non_exhaustive]` enum with stable numeric
codes (`Error::code()`). Library code returns `Result<T>`; user, file and
remote input never cause a panic; `debug_assert!` checks internal
invariants. `NotImplemented` marks upstream's `AKANTU_TO_IMPLEMENT`,
`Unsupported` a path the port refuses. `Error::Fatal { kind: Transport |
Agreed }` is the signal that an owner is poisoned (see "Agreement across
ranks"); code keys on `Error::fatal_kind()`, never on message text.
`error::if_present` is the one way to skip an absent block: it matches
`NotFound` only.

## Storage

### Arrays

`Array<T>` stores `len` tuples of `nb_component` values, tuple-major
(NumPy C order `(len, nb_component)`); `nb_component == 0` is valid and
represents upstream's point-element specialisations. Kernels take
`ArrayView` and `ArrayViewMut` (or slices), so owned arrays, DOF-manager
storage, NumPy buffers and MPI buffers are used without copies.
`swap_remove_tuple` is upstream's erase (the last tuple moves into the
hole). Newly allocated `Real` storage is `0.0`.

`ElementTypeMap<T>` is a dense table `[[Option<T>; ElementType::COUNT]; 2]`
with enum-order iteration. `TypeFilter { spatial_dimension, ghost_type,
kind }` replaces upstream's named arguments. `ElementTypeMapArray<T>`
allocates blocks with upstream's ids (`"{id}:{type}"`, plus `":ghost"`);
per-quadrature-point data uses `nb_element × nb_quad` tuples,
element-major. A different `nb_component` for an existing block is refused
(upstream ignores it silently).

`akantu_core::tensor` gives `Vector<N> = [Real; N]`, `Matrix<R, C> =
[[Real; R]; C]` (column-major, `m[col][row]`, as Eigen) and the dynamic
`MatrixX`; `as_matrix` views an array tuple as a matrix without copying.
Upstream's `Tensor3` maps to consecutive column-major matrices.

### Shared storage and its aliasing rules (D-024, D-047, D-056, D-071)

Python receives zero-copy writable NumPy views of model, mesh and material
arrays. A plain `Array<T>` is never exported. Fields that foreign code may
view are stored as `SharedArray<T>` (`crates/akantu-core/src/array_shared.rs`),
whose values live in an export-capable allocation
(`array_storage.rs`, no `unsafe`). The rules:

- Native code reaches the values only through guards: `read()` gives an
  `ArrayRef` (derefs to `Array<T>`), `write()` an `ArrayMut` (forwards the
  mutating operations, never hands out `&mut Array<T>`), `map_slice`
  narrows to a `SliceRef`. On an array that is not exported a guard costs
  nothing.
- Foreign code holds an `ExportLease`: pointer, shape (tuple count and
  `nb_component`, so `(n, 0)` arrays keep their shape), generation and a
  keep-alive anchor. An exported allocation is never reallocated in place:
  growth beyond capacity, any decrease of the tuple count, `replace`,
  `into_inner` and drop move the values to a new allocation and retire the
  old block into the anchor. Value writes, equal-length copies (history
  restore) and growth within capacity keep it.
- A view is attached until its block is retired, then stale but safe:
  reads return the values at retirement, writes no longer reach the array,
  and binding methods that take an output buffer refuse it. A script that
  keeps a view across cohesive insertion sees the pre-insertion array
  (upstream: undefined behaviour; UD-085, UD-024).
- A `ForeignRuntime` orders accesses with two phases. Native guards on an
  exported array are admitted only inside a native section and counted;
  foreign access needs the runtime's `Interpreter` (the GIL stand-in,
  `!Send + !Sync`) and the foreign phase. A callback into foreign code runs
  only when no native guard is alive, so no guard may be held across a
  callback: code that calls foreign code copies what it needs first.
- The guards have an explicit `Drop`, so holding one across a call that
  needs the owner mutably is a compile error; guard-narrowing closures are
  higher-ranked and never see the guard's lifetime.
- Accessors that had no `Result` before their field became exportable
  (`Mesh::nodes`, `NodeGroup::nodes`, …) have `RefCell::borrow` semantics:
  they panic only when the code that exported the array breaks the
  protocol, and say so in a `# Panics` section. Inside a callback, a
  binding getter refuses a cached lease that no longer covers every live
  tuple.
- Per-element accessors never allocate (`Mesh::element_connectivity`
  returns a `SliceRef`); the copying one is named for it (`element_to_vec`).
- Not exportable: law element filters, law-local numberings, FE-engine
  precomputations. Python gets read-only copies.
- `SharedArray`'s `Debug` prints metadata only; state comparisons use
  `Mesh::state_fingerprint`.
- The test-only feature `export-all` of `akantu-core` makes every
  `SharedArray` behave as exported and panics at every native-to-foreign
  call site where a guard of the thread is alive; the test suite run with
  it checks the "no guard across a callback" rule.

Laws never take a guard of a model array themselves: the model takes the
guard once per operation and lends the values (`ModelServices::nodal_field`
for reads, an `ArrayViewMut` of the internal forces for the custom
assembly hook).

## Mesh

### Representation

`Mesh` holds the nodes (`dim` components), optional global ids, node flags
and owner ranks, the connectivities (`ElementTypeMapArray<Idx>`), ghost
counters, local and global bounding boxes, an optional facet mesh, the
group manager, the mesh data, the synchronizers, the communicator and a
revision counter. The facet mesh is owned by its parent and shares its
node numbering, coordinates, flags, global ids and owner ranks: node
operations are accepted on the parent only and applied to both. In the
facet mesh, blocks whose dimension equals the parent's spatial dimension
hold parent elements; the others are facets. `Mesh::check_consistency` and
`utils::check_facet_mesh_consistency` check the invariants.

`MeshAccessor` (upstream's) edits a mesh while it is built (readers,
distribution set-up, tests). Once consumers are registered, topology
changes go through the staged mutation below.

Mesh utilities produce output in type-enum, element and local-facet order,
which fixes facet numbering and hence cohesive insertion and DOF
numbering. Node groups are owned by the `GroupManager` and addressed by
`NodeGroupId`; an element group created on an existing node group shares
it. Every group has a process-unique `uid()`, kept by renames and new for
a replaced or copied group; state upstream attaches to the group object
(group dumpers) is keyed by it. `MeshData` keeps upstream's well-known
names (`tag_0`, `physical_names`, `element_to_subelement`,
`subelement_to_element`, `mesh_to_mesh_facet`).

### The mesh domain (D-019)

`SharedMesh = Arc<MeshDomain>` (`crates/akantu-mesh/src/shared.rs`). The
domain owns the mesh, its facet mesh and a consumer registry. Models, FE
engines, DOF managers, dumpers and the Python wrapper hold a handle and a
`ConsumerToken`, read the mesh per operation (`read()`) and never store
`&Mesh`. A model is one consumer that takes part at several priority
levels: FE engines 9, DOF manager 20, its own arrays and laws 94, the
non-local manager 100 (upstream's `EventPriority` values). Locks are never
waited for: a re-entrant mutation, a mutation while a reader is alive or a
read during a mutation fails at once with `Error::Mesh`, and a re-entrant
mutation is refused locally before any collective. The coordinator of a
mutation is whoever owns every consumer of the mesh (a model, a coupler or
the Python simulation object); a mutation that does not hand over every
registered consumer is refused before anything changes.
`SharedMesh::attachment::<T>()` gives one object of type `T` per domain;
`akantu-io` keeps its group-dumper registry there.

### Staged mutation (`crates/akantu-mesh/src/mutation.rs`)

`SharedMesh::mutate(handlers, change, distributed_hook)` runs one phase
scope. No collective starts before the agreement that makes it safe, and
every error before the commit restores the complete mesh state on every
rank.

| Stage | What happens | On failure |
|---|---|---|
| re-entry check | the domain is idle on this rank (local) | error, nothing changed |
| 0 preflight | not poisoned; the handlers are exactly the registered consumers; every consumer acknowledged the current revision; dispatch schedule computed; write lock taken; agreement | nothing changed |
| 1 topology | the caller's closure edits mesh and facet mesh through the journaled `MeshTransaction` and records events, each validated against the live state when recorded; model-provided staging (for example the law of each new cohesive element, D-053); local dependents (element-group node lists); agreement with a digest of the event schedule | full rollback |
| 2 distributed hook | `DistributedHook::prepare`, agreement, `apply` through the transaction and the phase scope, agreement; events the hook records are appended and their schedule agreed | full rollback, hook `abort` |
| 3 boxes and counts | bounding boxes reduced; global event counts: an event is dispatched on every rank if its global count is non-zero, so ranks without work join collective consumers | full rollback |
| 3b capability check | the event-composition rules below; then every consumer's `check_mesh_event` for every event of its part, in registration order; agreement (D-068) | full rollback, recoverable |
| 4 commit | journal dropped, revision incremented, hook `commit` | |
| 5 dispatch | events in recorded order (facet events, then elements before nodes, as upstream); per event every consumer of that part by ascending priority level, newest registration first among equal levels; one agreement per event, consumer and level | domain poisoned on every rank |
| 6 acknowledge | every consumer's revision set to the committed one; write lock released | |

The journal covers topology, node flags, global ids, owner ranks and
copy-on-write snapshots of groups, mesh data, ghost counters, synchronizer
schemes and bounding boxes. `check_mesh_event` has no default: every
consumer states before the commit which events it cannot take, and a
forwarding handler forwards the check. A failure after the commit poisons
the domain; the documented recovery is to rebuild the simulation.

Consumers that communicate while they react (DOF numbering, synchronizer
updates) override `on_mesh_event_phased` and communicate only through the
`Phases` they are handed. A consumer present on only some ranks is
registered as a no-op on the others, so every rank has the same schedule.

### Event rules

`MeshEvent` keeps upstream's payloads (`old_nodes` of new cohesive nodes,
the law of new elements, an `Any` extension for IGFEM). Upstream updates
groups and the bounding box inside the node event and sends facet events
during insertion; here every mesh-owned dependent is final before the
first event is dispatched.

The node events of one mutation form a complete stream of the shared node
set: a `NodesAdded` lists exactly the next nodes not yet announced, in
increasing order, each `old_nodes` entry announced before its new node;
`NodesRemoved` comes only from the removal path (`remove_nodes`, `purify`);
node events are recorded on the parent and the facet twin is added
automatically; a facet mesh is never created while facet consumers are
registered. On a distributed mesh every rank records the same event kinds
on the same parts in the same order, with empty lists where nothing
changed. Element events are not constrained: as upstream, an
`ElementsAdded` may precede the `NodesAdded` of its new nodes.

Consumers receive the events after the commit, with the final mesh, so an
event whose indices a later event of the same mutation renumbers would
make a consumer read the wrong entity. Such compositions are refused in
stage 3b (D-095, D-111):

1. Node events: a `NodesAdded` naming a node beyond the final count; a
   `NodesRemoved` preceded by an addition; with a group-supported DOF
   family, a removal preceded by another removal; on a distributed or
   periodic mesh, a `NodesRemoved` followed by any other node event.
   Record node removals before node additions and compose the removals of
   one mutation into one numbering; on a distributed mesh a removal is the
   only node event of its mutation (`DistributedRemovalHook` filters and
   renumbers the schemes).
2. Element events, for every part that has consumers: an event that
   renumbers a block (a non-identity `ElementsRemoved` or `ElementsChanged`
   numbering) is refused when an earlier event of the mutation names that
   block. Record all erasures of a block in one `erase_elements` call,
   before additions. An `ElementsChanged` is followed through its `old` and
   `new` lists; a consumer with per-element state accepts a renumbering of
   its elements only when the lists state the same move.
3. Upstream sequences that add then remove (IGFEM's interface move) are
   ported as separate mutations, one per phase the rules cannot compose.

Solver state named by local equations does not survive a renumbering: the
variable bounds of a `petsc_tao` solver are invalidated and must be set
again.

## FE engine

Each element type is a zero-sized type implementing `ElementClass`
(associated constants from the core tables) and a family kernel trait:
`LagrangeElement` (shapes, derivatives, `contains`, inradius, `inverse_map`
by Gauss-Newton from ξ = 0 with upstream's defaults of 100 iterations and
1e-10) or `StructuralElement` (rotation, `N`, `dN/dξ` and `B` as separate
primitives). IGFEM kernels live in `akantu-igfem`. Kernels reproduce the
operation order of upstream's `element_classes/*_inline_impl.hh`.

`with_element_class!(ty, E => body)` expands to a `match` with one arm per
type, so `body` is monomorphised per element class. Loops run over elements
and quadrature points with constant bounds; scratch buffers are allocated
at most once per `(type, ghost)` batch. Quadrature tables are transcribed
with their expression form and evaluated with the same operations, and
tensor-product and pentahedron rules keep upstream's point order.

`FeEngine` precomputes per `(type, ghost)` the shapes, shape derivatives
(zero components for point elements) and `det J · w`. Its methods take
`&Mesh`, read `ArrayView`s and write arrays they resize; the table of
upstream operations and their Rust names is in the rustdoc of
`crates/akantu-fe/src/engine/mod.rs`. The FE engine never assembles into a
global system: it returns elemental arrays, and the model assembles them
through the DOF manager. Structural elements use
`structural::StructuralFeEngine` (D-049), which stores rotations and the
`N·T` and `B·T` products; every operation that reads a cached block takes
the mesh and refuses a block whose mesh revision, element count or normal
rows have changed.

## Solver stack

### Ownership and coupling (D-021)

Upstream's model is a `SolverCallback` that owns a DOF manager owning
solvers that call back into the model. In Rust a model is
`{ physics, solver: ModelSolver, token }`: the physics implements
`SolverCallback`, `ModelSolver` owns the `DofManager` and the named
`TimeStepSolver`s, and `self.solver.solve_step(&mut self.physics, id)` is
a disjoint borrow. Every callback method receives `&mut DofManager`.

`akantu-model::coupling` offers both upstream coupling styles.
`Staggered<A, B>` holds two independently owned models, each with its own
solver; `solve` runs the first, transfers, runs the second and transfers
back (solid-phase-field). `Monolithic<P>` holds one `ModelSolver` and a
combined physics value whose callback calls its parts in upstream order
(solid-contact, solid-cohesive-contact). The contact model is such a part:
it owns no DOF manager (D-063).

An intercepting callback (Python's `solveStep(callback)`) runs through
`ModelSolver::solve_step_intercepted`: each user method receives a
`Delegate` to the model's own callback and the running DOF manager, valid
for that call only. `Delegate::split_as::<T>()` reaches the model state
for reads and refresh calls (D-057).

### DOF manager

`DofManager` (`crates/akantu-solver/src/dof_manager.rs`) owns per family
the DOFs, derivatives, increment, previous values, blocked flags, local
equation numbers (periodic slaves alias their master's equation) and the
group support; globally the `EquationLayout`, the residual, lumped
matrices, matrix relations (`new_derived_matrix("J", &["K", "M", "C"])`)
and the linear-system back-end.

Numbering follows `dof_manager.cc` (D-005): each registration and each
node addition numbers a block, nodes in local order and components
innermost; owned DOFs get consecutive global numbers from an exclusive
scan over the ranks; slave DOFs receive their owner's number over the node
synchronizer; pure-ghost DOFs have a local equation and no global one; on
node addition the families are updated in lexicographic id order (upstream
iterates a `std::map`). An executable worked example on three ranks is
`crates/akantu-solver/tests/dof_numbering_distributed.rs`. After any
numbering change every back-end matrix is reset (profile, values,
factorisation caches) and rebuilt by the next assembly; element events
reset the matrix profiles too (upstream clears only on node events and
would assemble into missing entries).

Every public collective method runs through `DofManager::collective`,
which opens the manager's phase scope or joins the running one, agrees
local validation before dependent communication and ends with an
agreement on its outcome, so callers on every rank may branch on the
result. `DofManager::step` runs one solver callback inside the scope; a
callback that communicates uses `DofManager::phases()`. Between operations
the scope is parked in the manager. A failure after an irreversible change
(a symmetry promotion, a reset after a committed numbering change) poisons
the manager on every rank.

Lumped matrices may carry symmetric 3×3 couplings over three consecutive
components of one family (`LumpedBlockCouplings`), used by the structural
model's exact rotary inertia (D-059). They are serial and non-periodic
only (a distributed manager or a family with periodic slaves is refused);
a lumped matrix without couplings runs upstream's operations exactly.

### Sparse matrices

`SparseMatrixAij` (upstream's COO "AIJ") stores global equation
coordinates and keeps three things apart, as upstream: the lookup key
(`(min, max)` for symmetric matrices), the stored coordinates (a new entry
keeps its first-inserted `(i, j)`, entry order is profile order, so MUMPS
receives upstream's arrays) and the value selection of `add_values`. An
unsymmetric block added to a symmetric matrix is handled by
`SymmetryPolicy`: `PromoteOnUnsymmetric` (default, UD-034 corrected: the
matrix becomes unsymmetric, agreed on every rank at `finalize_matrix` and
propagated to derived matrices) or `UpstreamUpperSelection` (upstream's
selection of the upper part). `add_values` checks every entry before it
changes anything.

Dirichlet conditions zero the rows and columns of blocked equations and
put `block_val` on the diagonal of equations owned by the rank. The
residual is not zeroed at blocked DOFs (reactions remain); Newton-Raphson
masks them in its norm. Serial products follow upstream's order. A
distributed product accumulates in storage order on a copy of `y` in which
shared rows start from zero, reduces it onto owners and synchronises back
(UD-003 corrected); rows the reduction does not touch keep upstream's
bits.

### Back-ends and direct solvers

The DOF manager dispatches over `Box<dyn LinearSystemBackend>`
(`crates/akantu-solver/src/backend.rs`). Collective back-end methods
receive a `BackendContext` and communicate only through its phase scope.
A native library enters its own collectives only after the scope agreed
that every rank enters them (`Communicator::native_collective`, which also
agrees the job identity); an error inside them is a transport-class
failure.

| Back-end | Matrix | Right-hand side and solution | Status |
|---|---|---|---|
| `AijBackend` + `EigenSparseLuSolver` (`eigen`) | centralised on rank 0 | centralised | default when MUMPS is absent; translation of Eigen 3.4.0 `SparseLU` + COLAMD, bitwise with the strict oracle (D-040) |
| `AijBackend` + `FaerLuSolver` (`faer`) | centralised on rank 0 | centralised | port extension, explicit selection only, qualified by residual and conditioning checks, no bitwise claim |
| `AijBackend` + `MumpsSolver` (`mumps`) | distributed assembled (`ICNTL(18)=3`) | centralised on rank 0, as upstream (a distributed right-hand side is not provided) | feature `mumps`; `auto` selects it on MPI communicators; bitwise identity with the oracle observed for a pinned configuration only (D-050, D-054) |
| `PetscBackend` (`petsc`) | PETSc-owned, distributed, one application ordering for rows, columns and vectors (UD-006 corrected) | distributed | feature `petsc`; KSP, SNES (`newtonls`), TAO (`gpcg`) |

`SparseSolverFactory::create` refuses the serial-only `eigen` and `faer`
with more than one process; multi-rank runs use `create_centralized`
(gather, rank-0 solve, scatter). Native solver objects are bound to the
communicator of their creation, agree a descriptor of every control
setting before each job, agree each job's global outcome, and are
destroyed collectively on MPI's main thread at the same program point on
every rank (D-054). Native non-linear solvers (SNES, TAO) receive a
`NativeSolveContext` whose trampolines return status codes that are equal
on every rank and store the Rust error, re-raised after the native solve.

### Non-linear solvers, time stepping and switches

`NonLinearSolver` implementations: `Linear`, `NewtonRaphson`,
`NewtonRaphsonModified`, `Lumped`, `NewtonRaphsonContact`, `PetscSnes`,
`PetscTao`. Each iteration, or the whole solve, is one collective
operation; every callback runs through `DofManager::step`; norms and
convergence decisions come from collective methods. `TimeStepSolver`
implements pseudo-time static, explicit and implicit default solvers with
lumped or consistent mass, and integration schemes of the generalized
trapezoidal and Newmark-β families. Schemes bind their DOF family at
construction; `J = c_M M + c_C C + c_K K` is a derived matrix (D-041).

Category-B defects have switches that reproduce upstream, corrected by
default, one switch set per owner: `UpstreamCompat` per DOF manager (it
covers every solver of that manager and the sparse solvers' switches,
D-055), `SmmUpstreamCompat` for the solid model, `LawOptions` per law,
`CohesiveUpstreamCompat`, `DiffusionUpstreamCompat`,
`PhaseFieldUpstreamCompat`, the couplers' compat sets and
`StructuralUpstreamCompat`. Oracle replays set only the switches their case
needs; `upstream()` constructors set all of them.

## Constitutive laws and models

### Laws (D-016)

`ConstitutiveLaw` (akantu-model) is the object-safe base: name, type,
element filter, parameters, internal fields, history save and restore,
energies. `Material` (akantu-solid) adds the solid hooks (stress, tangent,
energies, wave speeds, custom assembly hooks, declared reductions,
non-local entry point). Materials are generic over `const DIM` and stored
as `Box<dyn Material>`; the quadrature-point loop is inside the batch
methods, called once per `(type, ghost)`.

For each phase of a step the model splits itself into `(&mut laws,
&services)` and hands every law a `LawContext { phase, services: &dyn
ModelServices }`. A law reads the mesh, FE engines and nodal fields
through the services while it is itself mutably borrowed. Python
trampolines get the services as a handle that refuses every use once the
callback returns; the end of the callback waits for uses already in
flight, so a use must never wait for the lending thread or for a lock it
holds across the callback (the GIL included), and `ModelServices`
implementations only try such locks. A law never communicates during a phase: it implements
`Material::reduce_after_stresses` and issues its reductions through the
`LawReducer` the model binds to the running scope.

Built-in laws describe parameters in static `ParamDescriptor` tables,
runtime laws (Python subclasses, user Rust laws) own `DynamicParameters`;
both are `Parsable`, so `.dat` sections fill them byte for byte.
`InternalField<T>` covers the law's filtered elements with a default value
and optional history. A history save of a block that is not exported is
deferred until the block is written (and becomes an exchange of the two
allocations when the writer rewrites the whole block); an exported block is
copied in place, so foreign views stay attached. The cohesive laws' maximal openings
are committed at the end of a converged step, not at step entry (D-052).
An internal field with a random default (upstream's `RandomInternalField`)
draws through `akantu_core::random`, which reproduces libstdc++'s engine
and distributions clean-room (D-006); the parser's
random-parameter descriptors convert to it through
`akantu_parser::param::RandomParameter::to_core`, so the core has no parser
dependency.

`ConstitutiveLawFactory` maps names to constructors that receive the
construction-phase services. Built-ins are registered explicitly, users
and Python register at run time; `ForeignMaterial` is the runtime solid
law. `ConstitutiveLawSelector` implementations (default, mesh data,
element group, Python) form fallback chains.

Two exceptions to "laws see the model only through the services" exist,
each confined: the IGFEM laws share an `Arc<IgfemContext>` with their
model (engine and interface geometry behind `RwLock`s, written only while
the model dispatches mesh events, never exposed to Python laws), and
`MaterialFE2` owns its RVE models outright (one solid model per macro
point; `Dumper: Send` makes that possible). The FE² RVEs receive the macro
input through `Fe2Config` instead of a global parser, and the
extra-materials defect switches are set when a law is registered, before
its section is parsed.

### Non-local averaging

The non-local manager lives in `akantu-model` and talks to laws through
object-safe interfaces injected from the physics crates
(`NonLocalManagerCallback`, `WeightFunction`, `NonLocalGhostExchange`). A
solid law takes part only by implementing `MaterialNonLocal` and returning
it from `Material::as_non_local`. Each neighbourhood keeps its points in a
`SpatialGrid` (spacing 1.2 R) and builds half pair lists in upstream's
grid order. The default summation route `UpstreamHalfPairs` reproduces
upstream's pair order bitwise; `KeyOrder` is an opt-in, order-independent
route refused on distributed meshes (D-062, D-097). The cycle is
upstream's `computeAllNonLocalContribution`: flatten, start the ghost
exchange, update weights when due, average owned pairs, wait, average ghost
pairs, copy back, non-local stresses. An element event invalidates the
neighbourhoods and averaging is refused until they are rebuilt.

### Models

`Model: MeshEventHandler` (`crates/akantu-model/src/model.rs`) gives the
model type, the mesh handle and token, the `ModelSolver`, `init_full`,
`solve_step` (which starts with `mesh.check_current(token)`), the stable
time step and dumping. Every model has the `{ physics, solver, token }`
shape and routes mesh events by priority level. Upstream's convenience API is available as
snake_case methods and, in Python, under upstream's names.

- `SolidMechanicsModel`: explicit and implicit dynamics, statics, finite
  deformation, materials, energies, stable time step, Neumann load records
  that follow boundary elements when nodes are doubled (D-105).
- `SolidMechanicsModelCohesive`: intrinsic and extrinsic. An extrinsic
  insertion is one mesh mutation: the inserter doubles facets in the
  topology stage, stages and validates the law of each new element, the
  distributed hook updates flags, synchronizers and global ids, and the
  dispatch delivers facet events, `ElementsAdded`, then `NodesAdded` with
  `old_nodes` (DOF arrays grow at level 20, the model copies old-node
  values at 94, laws allocate new points).
- `ContactMechanicsModel`: a part of a monolithic coupler (D-063);
  detection on the core `SpatialGrid`; the generic contact model refuses a
  distributed mesh (CONT-019 is an approved exception, D-089).
- `DiffusionModel` (`HeatTransferModel` is a newtype over it),
  `PhaseFieldModel`, `StructuralMechanicsModel` and
  `EmbeddedInterfaceModel` follow the same pattern. The embedded model
  wraps a solid model; its reinforcement laws assemble on the background
  elements through the solid law hooks `assemble_internal_forces_custom`
  and `elemental_stiffness_custom`.
- The couplers are `Monolithic` (solid-contact, solid-cohesive-contact) or
  `Staggered` (solid-phase-field).
- The extra packages are separate crates behind facade features; their
  semantics are those of the code as it last built upstream (D-007).

## Parallel layout

### Communicator back-ends (D-015)

`akantu_comm::Communicator` is one cloneable handle with generic methods
over a sealed set of data types:

- serial (default): collectives are identities, self point-to-point goes
  through a mailbox;
- local world (`Communicator::local_world`, `run_local`): `n` ranks as
  threads in one process, collectives reduced in rank order,
  collective-mismatch detection and receive timeouts. Sends are eager or,
  with `SendMode::Rendezvous`, complete only when the receive consumed
  them. The fast test tier runs the distributed code paths on it;
- MPI (feature `mpi`, rsmpi): imported from `mpi4py` or a host
  application, or initialised by akantu.

Non-blocking operations own their buffers (dropping an incomplete send
completes it, dropping an unmatched receive cancels it). On MPI every
collective first agrees a descriptor (operation, root, reduction, type,
count, local validity), so contract violations fail on every rank instead
of hanging; contributions of at most 64 bytes (agreements, scalar
reductions) travel in that header, so such a collective is one MPI
collective. Self-messages never enter MPI, and every remote receive is
filled by `MPI_Improbe` and an exact-length `MPI_Imrecv`, which avoids an
Open MPI 5.0.7 defect (`docs/DEPENDENCIES.md`); a truncated receive
allocates the whole message (at most 2 GiB), and posted receives progress
only inside akantu waits and tests. Reductions fold in rank order; a
`NativeMpi` policy uses MPI's own reductions (D-039).

### MPI lifecycle

`crates/akantu-comm/src/lifecycle.rs`: an MPI environment that is already
initialised is imported and never finalised by akantu; otherwise akantu
calls `MPI_Init_thread` and finalises after the last `MpiEnvironment`
guard (held by communicators, requests and native back-ends) is dropped,
in the order models, native back-ends, requests, communicators,
`MPI_Finalize`. The thread level is enforced (`ThreadPolicy`), handle
destruction is deferred to MPI's main thread when the level requires it,
and `MPI_Finalize` runs only on the main thread after every queued
destruction. Under `MPI_THREAD_SERIALIZED`, communicator creations and
native-policy reductions are admitted one at a time: a split can be
refused repeatedly under contention (serialise creations or retry), and MPI
calls a host application makes outside akantu are not covered by this
rule. The facade registers an exit hook (`release_at_exit`) that
finalises MPI at a normal process end only, never after a panic or, on
glibc, a non-zero exit status, and never blocks on the port's own locks
(D-067). A rank that cannot call `MPI_Abort` asks the launcher to abort the
job through `PMIx_Abort` before `_exit(71)` (D-069).

### Agreement across ranks (`crates/akantu-comm/src/phase.rs`)

A distributed operation is a sequence of named phases: local preflight
(validation, sizes, capacity; no communication, no shared-state change),
an agreement (one bounded all-reduce of status, phase id and an optional
descriptor digest in which every rank takes part, including ranks without
work and ranks whose preflight failed), communication only if every rank
agreed, then an agreement on the local post-processing. Code inside a
larger operation receives a `Phases` scope (`run`, `run_collective`,
`collective`, `agree`, `agree_digest`) instead of a raw communicator.

After a failed agreement the scope is failed on every rank and later
phases return errors without communicating, so ranks that detected the
failure in different code paths never wait for each other. A rank's own
error comes first in its report; the others report a recoverable "failed
on another rank" error, so callers do not branch on the variant. There is
one owner scope per communicator: an independent scope or a raw operation
inside a running scope, on any thread, is refused locally and the refusal
is agreed. A live `Phases` is `!Send`; `Phases::park` and
`ParkedScope::resume` are the only transfer. An `Error::Fatal { kind:
Agreed }` is agreed as fatal and poisons every owner (DOF manager, mesh
domain); an `Error::Fatal { kind: Transport }` is never agreed: the
communicator is marked broken and the runtime aborts the job. Owners
convert their scope's state with `Phases::settle` before dropping it.
Ordinary input errors therefore surface as errors on every rank, never as
timeouts; the tests inject rank-specific failures on the local world.

### Synchronizers

`Synchronizer<E> { send/recv: BTreeMap<Rank, Vec<E>> }` with
`synchronize`, `slave_reduction`, their asynchronous split and `*_in`
variants that run inside an enclosing scope. `DataAccessor<E>` packs and
unpacks per `SynchronizationTag`. An exchange is one phase: prepack and
size every buffer, agree, post owned receives then sends, unpack in rank
order with a typed wire contract (received length equals the announced
size), agree. Message tags are `1024 + 64·tag + hash6(synchronizer id)`; two
synchronizers whose ids collide in the hash must not exchange the same tag
concurrently.

The element and node synchronizers are `Synchronizer<Element>` and
`Synchronizer<Idx>` in `MeshSynchronizers`; entity behaviour is in
functions (substitution and renumbering on element events, node-scheme
rebuild on node additions). The DOF synchronizer translates node schemes
into equation schemes. The facet synchronizer, the grid synchronizer and
the periodic node synchronizer live in `akantu-mesh` because upstream's
`Mesh` builds them (the facet synchronizer for every distributed facet
mesh).

### Distribution, ghosts and partitions (D-020)

`distribution::distribute(mesh, partition, root)` is centralised, as
upstream: the global mesh and the partition live on the root, which
computes each rank's local mesh with upstream's rules (types in enum
order, local then ghost elements in global order, node numbering by first
appearance, master is the lowest claiming rank, schemes sorted by global
id, groups, data, periodic partners as pure ghosts) and scatters one
package per rank. It is a set-up operation, run before any consumer
registers; non-root meshes must be empty, the root must have no facet mesh
and no ghosts, and on one rank the mesh is only marked distributed. A mesh
distributed after `make_periodic` needs `update_periodic_synchronizer`
first (UD-254). Partitioners implement `MeshPartitioner`: Scotch (feature
`scotch`, a C shim), prescribed partitions (oracle fixtures, D-009), mesh
data, and a pure-Rust recursive coordinate bisection that is never claimed
equal to Scotch. `PartitionMetadata` records what a Scotch parity claim
needs, including `SCOTCH_PTHREAD_NUMBER=1`.

`init_mesh_facets` on a distributed mesh is collective: facets are built
locally, the facet synchronizer matches ghost facets to their owners by
sorted global connectivity (upstream has no global facet numbering), ghost
facets are flipped to their owner's node order (`FacetFlipMode::UpstreamInverse`
reproduces upstream's inverse permutation, UD-122), and the facet mesh is
installed after every rank succeeded. Upstream's
`synchronizeGhostFacetsConnectivity` is commented out upstream and not
ported.

Parallel cohesive insertion keeps upstream's structure: the insertion
decision (`_ce_groups`, an exclusive scan, `_ce_insertion_order`) runs in
its own scope before the mutation; every rank records every event kind;
the hook `CohesiveGlobalDataUpdater` extends the cohesive element
synchronizer along the facet schemes without communication, gives doubled
nodes global ids and owners, rebuilds the node schemes and sends the
owners' staged laws of new elements to the ghost copies.

The grid synchronizer adds a ghost layer from a spatial grid.
`make_periodic` on a distributed mesh appends missing partners as pure
ghosts; periodic slaves are summed per master in reverse insertion order
(D-061), the cross-rank part in rank order.

The solid, diffusion and phase-field models accept distributed meshes:
ghost elements receive their law over `_constitutive_law_id` in rank order
(upstream's `MPI_Waitany` order is not reproducible from three ranks on),
owners' state reaches ghosts inside the residual callback through
`DofManager::phases`, and a model refuses a synchronization tag it does
not register (UD-553, with a compat switch). `_smm_density` and
`_smm_mass` are not exchanged for the laws that carry no data for them
(upstream packs nothing, or never unpacks). `all_gather_v` carries
bounded payloads only; root-only allocations are mesh input and
distribution and the centralised right-hand side of `eigen`, `faer` and
MUMPS.

## Input and output

Mesh readers (`read_mesh`: Gmsh `.msh` 1.x, 2.x, 4.0, 4.1, Gmsh-struct,
Diana) target an empty mesh in the build phase, validate every index, stage
everything and fill the mesh by moves. They never communicate (D-037), so
reading on the root before distribution works on any communicator. Abaqus
is `Error::Unsupported`, as upstream has no reader (D-027).

Dumpers store descriptors (`FieldDescriptor`), never data; at dump time
the owner of the data passes a `FieldResolver` that reads live storage, so
a dump after a topology change needs no refresh. A dump renders every file
in memory before the first write, writes `.vtu`, `.pvtu`, then replaces
`.pvd` atomically, and commits the dumper state only after that. Deterministic formats are
byte-identical with the oracle; corrected layouts (UD-110, UD-113, UD-114)
are the default and `DumperParaview::set_compat_layout(true)` reproduces
upstream. Deviations, all in the DUMP rows of `docs/FEATURE_PARITY.md`:
duplicate field or variable names are `Error::AlreadyExists` (one name
space), unregistering an unknown name is `Error::NotFound`, a field group
that does not cover the written nodes or elements and names XML cannot
represent are errors (upstream writes broken files), a failed dump leaves
the dumper unchanged, output directories are created recursively, and text
precision is at most 65535 digits. The dumpers upstream attaches to the mesh and its groups live in
one registry per mesh domain, keyed by group identity
(`akantu_io::dumper::group::MeshDumpers`). Dumped blocks are bounded
(D-070). `Dumper: Send`.

The parser keeps input files as byte strings, stores sections in file
order and gives upstream's `std::map` order on request; it is iterative,
so nesting depth is limited by memory only. On Linux with glibc 2.41 parsing is bitwise with
the oracle; on macOS it is bitwise except 18 enumerated last-bit libm
differences (`LIBM_EXCEPTIONS` in `crates/akantu-parser/tests/oracle.rs`);
other platforms are not measured. `cli::ArgumentParser` ports
`cppargparse` and returns errors and `--help` to the caller instead of
exiting.

## Optional native back-ends and Cargo features

The default build compiles no native code. Each native feature compiles
its C shim in `build.rs` only when it is enabled.

| Feature | Where | Native dependency | Licence consequence |
|---|---|---|---|
| `mpi` | `akantu-comm`, `akantu-mesh`, `akantu-solid`, `akantu-cohesive`, `akantu-diffusion`, `akantu-couplers`, `akantu-extra-materials`, `akantu-ntn-contact`, `akantu-python`, `akantu-bench`, facade | an MPI-3 implementation (`mpicc`), libclang at build time | none (Open MPI is BSD-3) |
| `scotch` | `akantu-mesh`, `akantu-solid`, `akantu-cohesive`, `akantu-diffusion`, `akantu-couplers`, `akantu-extra-materials`, `akantu-ntn-contact`, `akantu-bench`, facade | `libscotch` (CeCILL-C), dynamically linked | ship Scotch's notice with binaries |
| `mumps` | `akantu-linalg`, `akantu-solver`, `akantu-solid`, `akantu-bench` (implies `mpi`) | parallel MUMPS 5.7 (CeCILL-C) and its BLAS, ScaLAPACK, Scotch, METIS | ship the linked libraries' notices |
| `petsc` | `akantu-linalg`, `akantu-solver`, `akantu-solid`, `akantu-phase-field`, `akantu-couplers` (not in the facade) | PETSc 3.22 (BSD-2-Clause) through `pkg-config` | a binary linked against Debian's PETSc also links GPL-2.0-or-later libraries and is distributed under GPL-3.0-or-later as a whole (D-076) |
| `cgal` | facade, `akantu-igfem` | none (pure Rust GPL crate `akantu-geometry-cgal`) | the build is GPL-3.0-or-later as a whole (D-042) |
| `igfem` | facade (implies `cgal` and `akantu-igfem/cgal`) | none | GPL-3.0-or-later as a whole |
| `python` | `akantu-python`, facade (embedded CPython for one example) | CPython ≥ 3.9 | none |
| `export-all` | `akantu-core` (tests only) | none | none |

`cgal`, `igfem` and `akantu-igfem/cgal` are the only features that reach
the GPL crate. `crates/akantu-geometry/tests/license_separation.rs` replays
Cargo's feature resolution and fails if any default feature set of an LGPL
crate, or any other feature, reaches it. `embedded` never implies `cgal`:
it uses the LGPL `GridIndex` search structure (or `LinearScan`).

## Python extension and C interface

`akantu-python` builds the extension module `py11_akantu` (PyO3 0.30,
`abi3-py39`, rust-numpy 0.30), loaded by a copy of upstream's
`python/akantu/__init__.py`; the wheel is built with maturin from
`python/pyproject.toml`. Classes, methods, enums and constants keep
upstream's names; connectivities are `int32` with `-1` sentinels. Arrays
are zero-copy views under the shared-storage rules: one `ForeignRuntime`
per interpreter, whose interpreter lock is the GIL (`gil_used = true`);
every entry point that touches model state opens a native section and
never releases the GIL inside it; trampolines go through `callback()`.
Python-reachable objects sit behind a borrow flag (`SharedOwner`). Python
subclasses of laws, solver callbacks and boundary-condition functors are
supported; handles lent to a callback are admitted only from the lending
callback's execution context (D-086), and overridable methods follow
pybind11's `get_override` rule (D-104). The residual user obligation
(UD-085): a NumPy operation that releases the GIL on an akantu view in one
thread must not overlap an akantu call in another thread. An MPI build of
the module (feature `mpi` of `akantu-python`) covers the mesh level on
several ranks; the model level on several ranks from Python is not
provided (D-101).

`akantu-capi` exports `akantu_version()` only. The C interface with opaque
handles and borrowed views that D-024 planned is not built; C and C++
users go through the Rust crates or the Python module (D-100).

## Execution policy (D-123)

`akantu_core::exec` says how the kernels of a model run. Its parts:

| Item | What |
|---|---|
| `Precision` | `Bitwise` (default): every result bit as the serial reference path; `Fast`: kernels marked `FAST MODE` may round differently (D-122) |
| `ExecPolicy` | precision, thread count (default 1), `SimdToken`, `Grain`; a plain value. `ExecPolicy::process_default()` reads `AKANTU_NUM_THREADS`, `AKANTU_PRECISION`, `AKANTU_SIMD`, `AKANTU_EXEC_GRAIN` once, unless `set_process_default` was called first |
| `Exec` | a policy with its thread pool (`exec/spin.rs`, one per thread count per process; the caller takes part, each thread takes its own block of parts first, late workers skip a region, idle workers spin 5 ms then park); `for_each` / `try_for_each` / `for_each_bounds` run a closure on contiguous parts of one or several arrays (`Tuples`, `TuplesRef`, tuples of them); `sum_in_order` |
| `NodeScatter` | per node, the positions `(element, local node)` that contribute to it, in the serial order; `scatter_add` with threads owning disjoint node ranges; `matches` checks it against a connectivity |
| `SimdLevel`, `SimdToken`, `with_fma` | detected level, proof of support, the dispatch convention (`exec::simd`) |

Ownership: a model holds one `Exec`. `SolidMechanicsModel` stores it in its
FE-engine registry (`FeEngineRegistry::exec`, given to every engine,
including those registered later) and its DOF manager
(`DofManager::exec`); its services return it to the laws
(`ModelServices::exec`, default serial for services without a model).
`SolidMechanicsModel::{exec_policy, set_exec_policy}` read and change it
between steps; `HeatTransferModel` (`DiffusionModel`), `PhaseFieldModel`
and `StructuralMechanicsModel` have the same pair and the same ownership
(one policy set at creation, given to the FE engines and the DOF
manager); the heat model's services return the engines' executor to its
laws.

Rules for a threaded kernel:

- Only independent outputs are split: an output tuple is computed from
  inputs that no other part writes, by the same code whatever the split.
  The borrow checker enforces the disjoint writes (`Tuples` parts are
  `&mut` sub-slices).
- A sum into shared data keeps its serial order: element-to-node
  assemblies through `NodeScatter` (the solid model caches one per
  `(law, type, ghost)` in `AssemblyScratch::scatters`, reuses it while the
  mesh revision and the law's filter are unchanged and the connectivity is
  not exported, compares it row by row otherwise, and rebuilds it when it
  differs); reductions through `sum_in_order` or an equivalent serial fold
  of per-item values.
- The split is resolved once per batch from the engine's `Exec`
  (`nb_parts`: a minimum part size of tens of microseconds of work, at most
  four parts per thread); without a pool the batch runs as one part on the
  calling thread, i.e. the serial code.
- Errors: `try_for_each` returns the error of the first failing part in
  item order. Outputs after the failing item may already be written
  (unlike the serial loop). The FE kernels' callers treat the outputs of a
  failed kernel as undefined. A law's state after an error is the serial
  loop's at every thread count (current and previous values). The laws
  that compute into scratch blocks committed by swap (plastic, standard
  linear solid, Marigo, Maxwell) take their guards, size the scratch and
  load what the points read before they write (the plastic hardening, the
  Marigo damage) on the calling thread, before the parts; the parts write
  disjoint ranges of the scratch and call no `InternalField` method; the
  commits run once on the calling thread after every part succeeded,
  none when a part failed. The laws that work on copies of their blocks
  write them back only when every point succeeded (Mazars, Drucker–Prager,
  anisotropic damage);
  the neo-Hookean stress finds the first point with `J ≤ 0` with a
  read-only pass of the kernel's own predicate, runs the points before it
  on the threads and the rest serially; the plane-stress `C33` solve
  writes a copy and commits the points before the first failure; a law
  whose points fail only on sizes that are the same for every point (the
  anisotropic kernel) fails at the first point of every part, before
  writing.

Threaded today (all bitwise, `akantu-solid/tests/exec_threads.rs`,
`akantu-diffusion/tests/exec_threads_explicit.rs`,
`akantu-couplers/tests/exec_threads_phase_field.rs`): the FE
gradient, `BᵀD` and integration batch kernels (`akantu-fe`
`engine/fast.rs`); `grad_u −= eigen_grad_u` in `computeAllStresses`, on
the model's threads for every law, whatever the law declares (a
per-value subtraction, `material.rs`); the finite-deformation residual
(`akantu-solid` `model/assembly/finite_deformation/fast.rs`); the
internal-force scatter; the per-point loops of the laws that declare
independent points (list below: stress, the per-step hooks and energies
that run every step, the phase-field law's tangent); the
external-work increment's per-node terms (summed in node order,
`model/energy.rs`); in `akantu-solver`, the Newmark-β predictor and
corrector (`integration_scheme.rs`), the lumped solve (`r += M∘a`,
`x = α·(b / m)`), the residual scatter through the inverse equation map
and the solution gather (`dof_manager/assembly.rs`, `dof_manager.rs`); in
`akantu-linalg`, the large panel updates of the Eigen SparseLU
translation (at least 65,536 multiply-adds: the product over strips of
rows, the scatter over blocks of accumulator rows,
`eigen_sparse_lu/kernels.rs`; the AIJ backend hands the DOF manager's
executor to the solver of every solve, `SparseDirectSolver::set_exec`; a
DOF manager starts with the process default policy until its model sets
its own); in
the heat transfer model, the heat law's diffusivity and `D∇u` loops and
the internal-flow scatter (`akantu-diffusion` `model/assembly.rs`, a
cached `NodeScatter` per `(law, type, ghost)` as in the solid model),
with the FE kernels above; in the phase-field model, its FE kernels.
Also on threads: the generalized-trapezoidal predictor and corrector, the
increment update (`time_step.rs`), the schemes' store and restore copies,
the blocked-DOF scan (each part's blocked equations appended in part
order, then sorted as before); the non-local weighted average
(`akantu-model` `non_local/neighborhood.rs`, from 3 threads: at 2 the
per-point gather, which visits every pair twice, is slower than the
serial half-pair loop; per point, its contributions
in pair order from a per-point list built with the pair list, when the
slots increase with the points; the manager takes the executor from its
callback); the FE plans' and kernels' checks of the element ids and the
connectivity (an integer maximum, a logical or or a logical and per
part, then over the parts: `fast::max_unsigned_with`, `any_with`,
`consecutive_with`, `nodes_valid_with`, `batch_ids_with`; one element's
row takes the serial maximum) and the finite-deformation check of the
material filter (`Derivatives::check_filter`: the largest id per part,
the serial scan only to report the first element out of range); the
deferred history save of a block reached through
`InternalField::get_mut_with` (a copy in parts); Mazars' copies of its
blocks (with threads into buffers kept per element type and ghost type
from one step to the next: a fresh allocation of a block's size, once
glibc's mmap threshold has grown past it, comes from the heap and is
zeroed by `calloc` on the calling thread; the cost is a permanent second
copy of the blocks the points work on, `5 + 2·dim²` values per
quadrature point, `5 + 3·dim²` for `plastic_mazars`: 184 bytes per point
in 3-D, 104 in 2-D; a clone of the law starts without them). The
per-node and per-equation loops split into parts of at least
2,048 nodes or 4,096 equations: measured with the step's other kernels on
threads, smaller parts gave the explicit steps of 0.1 to 0.35 million
equations 0.70 to 0.91 of the time at 8 threads (also with idle workers
parked at once); an earlier screening, made while the kernels that write
these arrays ran serially, had found parts of 262,144 equations faster.
Serial still: the phase-field model's driving-force loop (its law
interface carries no executor) and its internal-force scatter, the sparse
direct solves, the external-work increment's sum (one fold in node order,
as the bitwise result needs), the non-local Marigo law's damage after the
average.

Matrix assembly and the matrix operations of implicit steps (bitwise at
every thread count; `akantu-solver/tests/threaded_assembly.rs`,
`akantu-solid/tests/exec_threads_matrices.rs`,
`akantu-fe/tests/threaded_elemental_matrices.rs`, the heat and
structural `tests/exec_threads.rs`):

- Elemental matrices on the FE engine's executor:
  `FeEngine::compute_btdb_integrated` (`∫ BᵀDB` per element, the products
  of `compute_btdb` and of `integrate`, without the per-point array; the
  solid stiffness and the heat conductivity) and the consistent mass
  (`field_matrix_elemental`: Jacobians, point matrices and integrals per
  part of elements). A law's tangent is called once per block as before
  (on threads only inside a law that declared `independent_points`). The
  solid model keeps its stiffness and mass arrays between assemblies
  (`StiffnessScratch`, boxed in the model: one elemental-matrix array
  per element type, shared by stiffness and mass, the tangent, the
  gradient and the density values; 187 + 46 MB for 162,000 four-node
  tetrahedra), so that repeated assemblies map and fault no fresh arrays;
  only arrays taken from its pool return to it (a law's own blocks and
  the finite-deformation blocks are dropped). `release_static_solver`
  frees them. The mass goes through
  `FeEngine::field_matrix_elemental_into` (caller-owned arrays).
- The scatter of a repeated element assembly
  (`DofManager::assemble_elemental_matrices_to_matrix` →
  `SparseMatrixAij::add_values_cached_batch`): when the round continues
  a `BlockPositionCache` of the current entry positions, the blocks'
  equations are checked against the stored slots (and missing slots
  resolved, without touching the matrix) in parallel; then the entries
  are split into contiguous ranges and every part visits, in element
  order, the elements whose smallest-to-largest stored position meets its
  range, adding only the contributions inside it. Every entry receives
  its contributions in element order. The first assembly of a matrix
  (its profile, in first-insertion order) and any batch the matrix
  declines (a promotion, an equation the cache does not hold, a missing
  entry, a repeated slot) run the serial loop.
- The scatter plan: the position cache keeps, per slot, the span and the
  count of stored positions, and one plan per batch of slots (one per
  element filter, i.e. per material of the type; at most 16): the entry
  bounds of the parts (one per thread, equal entry counts; with the
  finest grain as many as the executor splits a batch into) and, per
  part, the blocks whose span meets it in block order, flagged when they
  lie inside it. A plan is rebuilt when its slots, the profile size or the
  split differ, dropped with the stored positions (another epoch, another
  block shape or order) and when one of its slots is stored. The DOF
  manager vouches for the equations
  (`BlockPositionCache::vouch_equations`) with the mesh revision, the
  numbering generation and the connectivity's storage, never for an
  exported connectivity; with a matching key the batch skips the
  per-block check of the equations. The key is consulted only after the
  matrix order, the position epoch and the entry count have matched the
  plan's, so what it alone guards are in-place edits of the connectivity
  (the mesh revision and the connectivity's storage). Every operation that
  changes the numbering generation (family registration, node addition or
  removal, the rollback of a failed growth) also changes the order or
  resets the profile; the generation is kept in the key as a defence
  against a future renumbering that would keep both (one comparison). On one thread the batch takes only
  the planned path; a round without a plan runs the serial loop, after
  which the DOF manager builds the plan from the slots it has just
  checked (`plan_after_serial_round`).
- The chunked assembly (`DofManager::assemble_elemental_matrices_by_chunks`,
  one thread only): with a plan ready, the elemental matrices are
  produced per chunk of about 4 MB and scattered at once (the plan's part
  lists cut at the chunk's blocks), so they are still in the caches; the
  solid small-strain stiffness uses it (its whole-type array is then not
  kept). With threads a chunk of consecutive elements touches a narrow
  band of entries and leaves most parts idle; the whole-array batch is
  used.
- Whole-matrix operations of the AIJ backend on the DOF manager's
  executor (`LinearSystemBackend::set_exec`): zeroing, entry-wise sums of
  matrices with the same entries (`J += c·K`), the profile copy of `J`,
  Dirichlet rows, and the matrix-vector product on serial layouts, which
  gathers per row the terms of its entries and of the mirrors of
  symmetric storage in storage order (`RowGather`, built once per entry
  positions, 8 bytes per term) so that every row adds its terms in the
  order of the serial pass. Parts of at least 32,768 entries.
- `HeatTransferModel`, `PhaseFieldModel` and `StructuralMechanicsModel`
  take the process default policy and have `exec_policy` /
  `set_exec_policy`. The phase-field elemental matrices use
  `compute_btdb_integrated` (order 2) and `compute_ntbn_integrated` (the
  products of `compute_ntbn` and `integrate` per element, on threads);
  `StructuralFeEngine` has an executor (set by the structural model):
  `element_stiffness` and `consistent_mass` compute each element's point
  matrices and integral on threads (the products of `compute_btdb` /
  `ntbn_local` and of `integrate_rows`).

A law opts in with `Material::independent_points() -> true` and uses
`material::points_exec(self, ctx)` in its per-point loops (a law that
does not declare it gets the serial executor there, also for the shared
kernels it calls: the damage parents' stress kernel, the plastic and
damage energy updates, the thermal pre-step). Each declaration rests on a
reading of every piece of state the stress, the tangent and the per-step
hooks touch, and has a test of every internal field at 1 to 8 threads
(`exec_threads.rs`) that a mutant making a point depend on its
predecessor fails. Declared:

| Law | Threaded loops | Why independent |
|---|---|---|
| `elastic` | stress, thermal pre-step | a point's stress from its own gradient and thermal stress |
| `elastic_orthotropic`, `elastic_anisotropic` | stress | a point from its own gradient and the constant `C` |
| `neohookean` (incl. 2-D plane stress) | stress, `C33` solve | a point from its own gradient and stored `C33`; error path as above |
| `plastic_linear_isotropic_hardening` | J2 return, plastic work, thermal pre-step | a point from its own history (scratch blocks committed on success) |
| `plastic_drucker_prager` | projection, plastic work, thermal pre-step | the same |
| `mazars`, `plastic_mazars` | parent stress, damage loop, dissipated energy, thermal refresh | a point from its own values (copies); a failed eigen solve leaves the state unchanged |
| `marigo` | stress and damage loop, dissipated energy, thermal refresh | a point from its own values (scratch blocks, the damage loaded first) |
| `mazars_non_local`, `marigo_non_local` | the inner law's local loop (equivalent strain / `Y` per point) | the local loop is per point; the averaging, its own phase, stays serial |
| `anisotropic_damage`, experimental and corrected models | stress and damage | a point from its own gradient and committed damage (copies) |
| `viscoelastic_maxwell` | stress, branch update, potential and dissipated energy | a point from its own gradients and branch state; block constants set outside the loops; the stress and potential energy into scratch blocks, the branch state, work and dissipation in place |
| `sls_deviatoric` | stress and history | a point from its own gradient and history (scratch blocks) |
| `phasefield` | stress, tangent, thermal pre-step | a point from its own gradient, damage (written by the phase-field model) and thermal strain |
| `visco_plastic` (extra materials) | flow and return, plastic work | a point from its own values (copies); upstream mode (UD-583): also the first point's previous state, which the loop does not write |
| `damage_linear` (extra materials), per-point thresholds | stress and damage | a point from its own damage and threshold (copies) |
| `ve_stiffness_prop` (extra materials) | viscous stress | a point from its own velocity gradient |

Must stay serial: `anisotropic_damage` in its upstream model (a point
exceeding `Dc` rewrites the law's own `E` and `ν`, which later points
read); the non-local averages (the averaging couples points); the
iterative laws with a rank reduction after the stress phase
(`damage_iterative`, `iterative_stiffness_reduction`,
`orthotropic_damage_iterative`: the damage step picks the most stressed
point of the law); `damage_linear` in its upstream mode (UD-575: every
point reads and writes the first point's threshold); the FE² law
(`material_FE2`: one sub-model per point, with its own solver); laws
written in Python (the update calls into the interpreter); the cohesive
laws (insertion state; `akantu-cohesive`, not reviewed here). Update each
point from its own data and can opt in after the same review and test
(not done: no benchmark case uses them): `brittle` (its point loop hands
each point's state to a sequential sink used by the non-local variant),
`vreepeerlings` and `reinforcement` (embedded). The thermal pre-step of `thermal.rs` is the
elastic law's (`MaterialElastic::compute_thermal_with`).

SIMD convention (`exec::simd` module documentation): one portable body
performing per lane the scalar sequence, one `#[target_feature]` version per
level, one dispatch per batch on `SimdToken::level()`, a property test of
every supported level against the scalar bits. Fast-mode kernels: marked
`FAST MODE`, selected by `precision() == Fast` once per batch, validated by
`akantu_validation::fast_mode::check_fast_mode` against the bitwise run with
a rule of `fixtures/fast_mode/<area>/tolerances.json`.

Bitwise lane kernels (`exec::with_simd`, `exec::Lanes4`): a kernel body
written once over `Lanes4` (four `f64` lanes, each operation per lane, no
FMA) implements `SimdTask`; `with_simd(token, task)` compiles it inside an
`#[target_feature(enable = "avx2")]` entry point on `Avx2` and `Avx512`
tokens and for the baseline otherwise, so every level gives the same bits
and the crates that write such kernels (`akantu-fe`, `akantu-solid`) stay
free of `unsafe` (the one dispatch site is in `akantu-core`, `UNSAFE.md`
CO-19, CO-20). Lane layouts of the explicit step: the gradient has one
lane per row of `∇u`, the integrated `BᵀD` one lane per row of `D`, the
integrated finite-deformation residual one lane per node of a group of
four (component `k` fixed per vector; the `B_L0` selections then depend on
`k` only). Lanes are not a promise of packed instructions: the compiler
packs the integrated `BᵀD` and residual, about half of the plain gradient
and none of the gradient with the `eigen_grad_u` subtraction (scalar AVX
instructions; a packed variant was not faster,
`docs/benchmarks/perf-campaign/explicit/README.md`). No AVX-512 instance: three-component lanes do not fill 512-bit
vectors, and the compiler's AVX-512 code for the gradient was slower.

Element-block pass of the internal forces
(`MaterialElastic::stress_and_element_forces`, called by
`assemble_internal_forces`): for a model with one elastic law on one
element type of a mesh without ghosts and without non-local laws, one
threaded region runs, per block of elements of about 192 KiB of
per-element data, the gradient minus `eigen_grad_u`
(`FeEngine::gradient_plan`), the thermal pre-step, the stress (or second
Piola–Kirchhoff stress) and the integrated `BᵀD` (`btd_integrate_plan`)
or residual (`assembly::ResidualPlan`); the scatter follows. The plans run
the batch kernels on element ranges after checking the whole filter once,
and `grad_u`, the stress fields and the thermal fields receive the values
of the separate passes. Any other model, or a plan that cannot be made,
runs the separate passes.

Wait policy: idle workers spin for `ExecPolicy::spin` (`AKANTU_EXEC_SPIN_US`;
default 5 ms, 0 when the threads outnumber the available cores, with a
one-time warning), then park; the caller's wait for the workers inside a
region yields after 2048 polls. A pool is shared by every executor of its
thread count in the process: a region started while another runs (another
OS thread, or nested) runs serially. Pools live until the process ends;
after `fork` the child creates its own (D-123 lists the limits).

Threads and MPI: each rank has its own pool. The library does not bind
threads to cores; they inherit the rank's CPU mask. Run
`ranks × threads ≤ cores` with a binding that gives each rank its
`threads` cores (Open MPI `--map-by slot:PE=<threads>` or `--bind-to none`);
with `--bind-to core` all threads of a rank share one core. Spinning workers
keep `threads − 1` cores busy after each region, also during MPI waits and
output, and share SMT siblings' pipelines: with more ranks × threads than
physical cores set `AKANTU_EXEC_SPIN_US=0`.

## Where `unsafe` lives

Crates that need no `unsafe` declare `#![forbid(unsafe_code)]`. The others
declare `deny(unsafe_code)` (or `forbid` unless a native feature is on)
and allow it in named modules only. Every `unsafe` block carries a
`// SAFETY:` comment, and every crate with an `unsafe` site has an
`UNSAFE.md` table of its sites with their invariants. `python3
scripts/unsafe_inventory.py --check` verifies that the tables match the
code (fingerprints in `UNSAFE.fingerprints`); `--list` prints every site.
Sites by file:

| File | Sites | What |
|---|---|---|
| `crates/akantu-comm/src/mpi_backend.rs`, `communicator.rs` (feature `mpi`) | 69 | MPI FFI, raw handles, `Send`/`Sync` of handle wrappers, exit hook (`on_exit`/`atexit`), `PMIx_Abort` lookup |
| `crates/akantu-linalg/src/petsc/{objects,session,callbacks,ffi}.rs` (feature `petsc`) | 59 | PETSc shim calls and callback trampolines |
| `crates/akantu-linalg/src/mumps/mod.rs` (feature `mumps`) | 21 | MUMPS shim calls |
| `crates/akantu-python/src/{scoped,foreign_view}.rs` | 17 | phase-scoped model handles, NumPy arrays over export leases, frame checks |
| `crates/akantu-mesh/src/{scotch,scotch_reorder}.rs` (feature `scotch`) | 6 | Scotch shim calls |
| `crates/akantu-parser/src/cmath.rs` | 2 | libm FFI (`erf`, `erfc`, `tgamma`, `lgamma_r`, `asinh`, `acosh`, `atanh`) |
| `crates/akantu-capi/src/lib.rs` | 2 | the unmangled `akantu_version` symbol |
| `crates/akantu-core/src/exec/simd.rs`, `exec/spin.rs` | 18 | `#[target_feature]` versions of the SIMD example kernel and the FMA entry point of fast-mode kernels, called after a `SimdToken` check; the thread pool's job slot and the lifetime of a region's closure (D-123) |
| tests and examples (`akantu-comm`, `akantu-mesh`, `akantu-solid`, `akantu-linalg`, the facade) | 45 | MPI FFI in MPI tests and the `mpi4py` probe, counting allocators, raw-bit checks |

The counts are those of `--list` at the time of writing. `akantu-core`,
where the shared storage lives, has no `unsafe` outside `exec::simd` and
`exec::spin`. Changes to `unsafe` code
or to the shared-storage protocol are checked under Miri
(`cargo +nightly miri test -p akantu-python`, and `-p akantu-core` for the
storage tests, Stacked and Tree Borrows).

## Rules for numerical code

The port is bitwise equal to the C++ library in the oracle's `strict`
configuration (GCC 14.2, `-O2 -ffp-contract=off -DEIGEN_DONT_VECTORIZE`,
D-031) wherever the tolerance ledger says "exact". These rules keep it so:

- Keep the operation order of the upstream code you port, term by term,
  including the order of sums over elements, quadrature points, nodes and
  ranks (D-008). Element types iterate in enum order, groups in name order,
  parser maps in `std::map` order.
- No fused multiply-add unless the reference has one: never a crate
  kernel that fuses, and `mul_add` only where it reproduces a fused
  instruction of the oracle's build. The exception is code of the fast
  mode (D-122, D-123): marked `FAST MODE`, run only when the policy's
  precision is `Fast`, validated against the bitwise path. Rust does not contract `a*b + c` by
  itself. Three files hold every `mul_add` of the library code, each
  documented in place: `crates/akantu-core/src/glibc_libm/sincos.rs` and
  `crates/akantu-core/src/random/libm.rs` reproduce the fused instructions
  of the oracle's aarch64 glibc 2.41 binary (checked against glibc by
  `crates/akantu-core/tests/random_libm*.rs` and `glibc_libm_sincos.rs`), and the clean-room complex division
  (`crates/akantu-linalg/src/dense/complex.rs`) reproduces a libgcc
  helper.
- No reassociation, no fast-math, no approximate intrinsics, no SIMD that
  changes results, no `powi` where upstream calls `pow` (use the ported
  glibc `pow`; `math::pow_n` for integer powers multiplies left to right).
- Choose the Eigen operand family of the upstream call site. Eigen's
  result depends on whether an operand has a compile-time size: fixed
  `Matrix<R, C>` and `Vector<N>` reduce by a halving tree within Eigen's
  unrolling limit, `MatrixX` sequentially; products follow the ported
  `gebp` and GEMV orders (`akantu_core::eigen_order`, `eigen_gebp`);
  `det` and `inverse` use Eigen's closed forms for fixed `N ≤ 4` and LU
  otherwise, and dynamic maps (FE Jacobians) always use LU.
- Scalar integrals use `math::reduce` (upstream's in-place sort and
  pairwise sum).
- libm calls that reach results go through the ported glibc functions in
  `akantu_core::glibc_libm` and `random::libm`, not the platform's libm.
  Exceptions: the functions of `.dat` expressions call the platform libm on
  purpose, as upstream calls `<cmath>` (`crates/akantu-parser/src/cmath.rs`;
  see "Input and output"), and a few call sites still use the platform's
  `powf`, `atan2`, `atan` or `tan` (listed in `docs/NUMERICAL_VALIDATION.md`,
  "Platform effects"). On glibc these give the oracle's values; on macOS
  they can differ in the last bit.
- Reductions across ranks fold in rank order (D-039). Never use an
  unordered parallel reduction in the default mode. Threads go through
  `akantu_core::exec` (D-123): only independent outputs are split, sums
  into shared data keep their serial order (`NodeScatter`,
  `Exec::sum_in_order`), and every threaded kernel has a test comparing it
  bitwise with the serial run (D-017).
- Never loosen a tolerance to make a test pass. A new tolerance needs a
  written justification in `docs/NUMERICAL_VALIDATION.md` and an entry in
  the tolerance ledger. Under an exact rule `+0.0` and `-0.0` differ, and
  the only NaNs that match are the two default quiet NaNs that differ in
  the sign bit (D-046, D-051).
- A behaviour change to upstream's numbers needs a defect entry
  (`docs/UPSTREAM_DEFECTS.md`) and, for category B, a switch that restores
  upstream bitwise.

Results that are not bitwise by design: Eigen's blocked paths outside
upstream's call sites (LU for dynamic `n > 16`, eigenvectors for `n > 48`,
eigenvalue sorts with ties at `n ≥ 17`, D-065), the `faer` solver, MUMPS
outside the pinned configuration, and distributed results where upstream
itself is run-dependent (D-082). `docs/NUMERICAL_VALIDATION.md` gives the
criteria used there.

## Extension points

`docs/MAINTAINING.md` gives the step-by-step procedures. The architecture
view:

- A solid material: a type implementing `ConstitutiveLaw` and `Material`
  (usually generic over `const DIM`) in
  `crates/akantu-solid/src/materials/`, registered in
  `register_builtin_materials` (or in a user factory at run time). It
  reads the model only through `LawContext`, declares its internals as
  `InternalField`s and its parameters in a descriptor table, and
  implements `reduce_after_stresses` if it needs a collective. A law of
  another model implements that model's law trait (`DiffusionLaw`,
  `PhaseField`, the cohesive laws' trait) in its crate. Python-defined
  laws need no Rust code: `ForeignMaterial` hosts them.
- An element type: a variant of `ElementType` and rows of the static
  tables in `crates/akantu-core/src/element_type.rs`, a zero-sized type
  with its kernels in `crates/akantu-fe/src/element_class.rs` (or the
  structural or IGFEM module), an arm of `with_element_class!`, its Gauss
  rule in `crates/akantu-fe/src/quadrature.rs`, and the mesh-reader code
  for its Gmsh number in `crates/akantu-io/src/mesh_io/`.
- A model: a crate (or module) with a physics type implementing
  `SolverCallback`, a model type `{ physics, solver: ModelSolver, token }`
  implementing `Model` and `MeshEventHandler` (including
  `check_mesh_event`), its own law trait and factory if it has laws, a
  compat switch set if it corrects upstream defects, and a facade feature
  in `crates/akantu/Cargo.toml`. It registers its DOF families with the
  DOF manager and communicates only through the scopes it is handed.
- A mesh consumer: `MeshEventHandler` with a real `check_mesh_event`,
  registered at upstream's priority, part of the coordinator's handlers.
- A sparse direct solver: `SparseDirectSolver` with honest
  `capabilities()`, a `SparseSolverType` variant and a factory arm; `auto`
  never selects a solver that is not upstream's.
- A native back-end: a C shim compiled only under its feature, calls
  inside `Communicator::native_collective` or a native region, objects
  bound and destroyed by D-054, sites listed in `UNSAFE.md`.
