# Decision records

The technical decisions of akantu-rs: numerics, tolerances and platform,
licensing and provenance, API semantics, defect policy, storage and
architecture, and what the port refuses. Each record gives the decision,
the reason, the consequence for a user or contributor, and the documents or
code it governs. The ids are stable: source comments, the parity matrix
(`docs/FEATURE_PARITY.md`) and the defect register
(`docs/UPSTREAM_DEFECTS.md`) cite them. A later record that amends an
earlier one says so in both. Ids missing from the sequence below were
decisions about how the port was organised; they are listed at the end.

Dates are those of the decision. "Upstream" is Akantu at commit
`35a851780d0b9471d258ed53a003ae98aaa95ea0`; "the oracle" is that code built
as described in `docs/ORACLE_BUILD.md`.

## D-001: Licence of the port

2026-11-09. akantu-rs is distributed under LGPL-3.0-or-later, keeps
upstream's notices in `NOTICE.md` and has an SPDX header on every source
file.

Reason: a translation of Akantu's algorithms and structure is a derivative
work of LGPL-3.0-or-later code.

Consequence: dependencies must be compatible with LGPL-3.0-or-later (MIT,
BSD, Apache-2.0, MPL-2.0 at file level, LGPL). Exceptions with a GPL
consequence are D-042 (`cgal`) and D-076 (`petsc`). Governs `COPYING`,
`COPYING.lesser`, `NOTICE.md`, `docs/DEPENDENCIES.md`,
`ci/licence-policy.toml`.

## D-002: Reference build platform

2026-11-09. The C++ oracle is built in a Docker container derived from
upstream's CI Dockerfile (Debian trixie). The first images were aarch64;
x86_64 became the authoritative platform with D-043.

Reason: the oracle needs MPI, MUMPS, PETSc and Scotch, and upstream's CI
image defines a known environment.

Consequence: floating-point contraction can differ between architectures,
so every golden fixture records architecture, compiler and flags.
Governs `oracle/docker/`, `docs/ORACLE_BUILD.md`.

## D-003: Policy for upstream defects

2026-11-09. Every upstream defect is registered in
`docs/UPSTREAM_DEFECTS.md` with a category:

- A: undefined behaviour upstream. The port defines a correct behaviour and
  compares against a patched oracle variant (D-060) or analytical results.
- B: a clear bug. The port corrects it by default and, where useful, keeps
  a switch that reproduces upstream bitwise; a scientific review of the
  correction is required.
- C: a quirk or API semantics. The port keeps upstream's behaviour by
  default and documents it. Sub-labels (D-115): "C (documented difference)"
  for a deliberate deviation of the port's API or file format where
  upstream has no numerical behaviour to keep and no bug to correct, and
  "C (test defect)" where upstream's library is right and its test is not.

Reason: the port must keep compatibility without reproducing bugs blindly
or changing behaviour silently.

Consequence: every deviation is traceable; a differential test that uses a
patched oracle or a compat switch cites the defect id. Governs
`docs/UPSTREAM_DEFECTS.md` and every `*UpstreamCompat` switch set.

## D-004: Parallel contact

2026-11-09. The design of the contact code must not preclude distribution
(DOF-numbered assembly, synchronizer hooks, global ids, owner of a contact
pair = owner of the slave node). Distributed search and resolution of the
generic contact model (parity row CONT-019) is an approved exception
(D-089); split-node contact on distributed meshes is ported (D-083).

Reason: upstream's generic contact model and its couplers are serial only,
so there is no reference behaviour and no oracle for a distributed run.

Consequence: `ContactMechanicsModel` refuses a distributed mesh with a
recoverable error. Governs `crates/akantu-contact`, `crates/akantu-couplers`.

## D-005: Numbering at more than one rank

2026-11-09. The port reproduces upstream's first-seen local node
numbering, its rank-blocked global equation numbering and its triplet
insertion order. Differential parallel tests may feed oracle partitions to
remove partitioner differences.

Reason: identical numbering makes the oracle's distributed results
comparable at low cost.

Consequence: DOF numbering follows `dof_manager.cc` exactly (worked example
in `crates/akantu-solver/tests/dof_numbering_distributed.rs`). Solution
parity at more than one rank is bitwise where shown and otherwise
tolerance-based, as `docs/NUMERICAL_VALIDATION.md` states per case.

## D-006: Random numbers bit-compatible with libstdc++

2026-11-09. `akantu_core::random` implements a `minstd_rand0` engine
(`std::default_random_engine` of libstdc++) and libstdc++'s algorithm for
each distribution (uniform real, exponential, gamma, Weibull, extreme
value, normal with its cached value, lognormal, chi-squared, Cauchy,
Fisher F, Student t, `generate_canonical`), with Akantu's seeding and draw
order. The implementation is clean-room (D-030); every distribution is
reproduced bit for bit.

Reason: random material parameters (for example cohesive strengths) are
drawn through these algorithms, which the C++ standard leaves to the
implementation.

Consequence: seeded runs are bitwise equal to the oracle, including
fixtures with random fields. The reference semantics are tied to
libstdc++: an Akantu built with libc++ draws other numbers. Parsed random
parameters reach the core only through
`akantu_parser::param::RandomParameter::to_core`, so the core does not
depend on the parser. Provenance: `crates/akantu-core/src/random/CLEANROOM_LOG.md`.

## D-007: Upstream packages that no longer build

2026-11-09, extended 2026-11-11. `extra-materials`, `igfem` and the
`embedded` model, which do not compile at the reference commit (UD-025,
D-032), are in scope. Where no oracle can be built, the port follows the
source intent and is validated analytically and by upstream's own test
criteria; the parity matrix says "no oracle" (a patched oracle variant may
supply evidence, D-060).

For `extra-materials` the semantics are those of the code as it last
built: the package's statements are ported as written with the meaning of
v4.0.1 (the last release in which it built; the diff to the reference
commit is renames only); calls to core functions that still exist map to
the reference core; removed core functions are read with their Akantu 4
semantics, side effects included (for example the in-place overwrite of
`Math::matrix33_eigenvalues`, UD-576). Defects follow D-003 (UD-572 to
UD-586, UD-780 to UD-783), with switches in `ExtraLawOptions` chosen when a
law is registered, before its input section is parsed.

Reason: the mission covers the whole reference tree; finishing upstream's
unfinished development is out of scope.

Consequence: these crates have weaker evidence than the core; their rows
say so. Governs `crates/akantu-extra-materials`, `crates/akantu-igfem`,
`crates/akantu-embedded`.

## D-008: Reference arithmetic order in element-level kernels

2026-11-09. Element-level kernels replicate upstream's operation order
wherever it is deterministic: Eigen's closed-form `inverse()` and
`determinant()` for fixed sizes up to 4×4, Akantu's sorted pairwise sum for
scalar integrals, quadrature point order, and Eigen's product order
including its blocked GEMM kernel (D-036). Dense operations without a
deterministic order to replicate are validated with conditioning-based
criteria.

Reason: bitwise comparison of shape derivatives, Jacobians, stiffness
blocks and integrals needs the oracle's operation order.

Consequence: element-level fixtures compare bitwise. Conditioning-based
acceptance remains for Eigen's large-size paths that no upstream call site
reaches (blocked LU for dynamic `n > 16`, eigenvectors for `n > 48`,
eigenvalue sorts with ties, D-065; classes L1 to L3 of
`docs/NUMERICAL_VALIDATION.md`) and for iterative or native solvers.

## D-009: Oracle-generated inputs are frozen as files

2026-11-09, extended 2026-11-11. Meshes generated with Gmsh are committed
as `.msh` files (Gmsh version recorded) and oracle partitions are exported
as per-element partition ids; parallel differential tests feed them to the
Rust distribution. Every mesh used by a differential fixture or a timing
comparison is a file, committed or recorded by SHA-256 with its generating
platform, Gmsh version and command. Both sides of a comparison read the
same file, and a golden case never depends on regenerating a mesh on the
replaying platform.

Reason: Gmsh's output for the same `.geo` differs between Gmsh versions and
between x86_64 and aarch64 (for the cohesive benchmark cube at h = 0.12:
4,834 nodes and 23,183 tetrahedra on x86_64, 4,824 and 23,156 on aarch64);
Scotch partitions depend on version, strategy and seed.

Consequence: the Rust partitioner is validated separately (quality,
partition independence); build-time mesh generation is not ported (CI-008
is an approved exception, D-076). Governs `fixtures/meshes`,
`docs/BENCHMARKS.md`.

## D-010: Element type enumeration

2026-11-09. Rust has one fixed, complete `ElementType` enum (regular,
cohesive, structural, IGFEM); persisted and exchanged formats use the
string names (`_triangle_3`), never numeric values.

Reason: upstream's numeric values depend on the enabled packages.

Consequence: fixtures, files and Python use names; the MPI encoding of
`Element` uses the discriminant only between Rust ranks.

## D-011: Small dense algebra with Eigen's evaluation order

2026-11-09. `akantu-core::tensor` defines `Vector<N>`, column-major
`Matrix<R, C>` (`R` or `C` may be 0) and the dynamic `MatrixX`, with
zero-copy views of array tuples. Operations follow Eigen 3.4.0's
evaluation order without vectorisation, separately for compile-time-size
and run-time-size operands; `det` and `inverse` use Eigen's closed forms
for fixed `N ≤ 4` and partial-pivot LU otherwise and for every dynamic
size. `akantu-linalg::dense` reproduces upstream's `eig`/`eigh` overloads
(unsorted values, optional descending sort with paired permutation,
complex-eigenvector rejection) on a translation of Eigen's solvers kept in
MPL-2.0 files. `inverse()` returns `Error::Numerical` for an exactly
singular matrix (`inverse_unchecked()` reproduces upstream's `inf`/NaN),
and the eigen-solvers return an error on Eigen's `NoConvergence` where
upstream reads uninitialised values. `eig_vectors_complex` is an extension:
upstream's complex-vector overload does not compile (its sort comparator
is ill-formed for `std::complex`), and the port returns Eigen's
`eigenvectors()`.

Reason: upstream's kernels use Eigen fixed-size types and `Map`s; a
generic eigen-solver cannot reproduce Eigen's unsorted order or its
eigenvector signs, which are physically observed (UD-033).

Consequence: results are bitwise equal to the oracle's `strict` build
(vectorised builds differ from `strict` in about half of the probe
records, so they are not a target). A porter must choose the fixed or the
dynamic family to match the upstream call site. The one `mul_add` of the
workspace models the oracle's complex division (D-073). The MPL-2.0 files
are listed in `docs/DEPENDENCIES.md`.

## D-012: Scalar and index types

2026-11-09. `Real = f64`; stored local indices `Idx = i32` (aliases
`NodeIdx`, `ElemIdx`, `DofIdx`) with upstream's sentinel `-1`; global ids
and global equations `GlobalIdx = i64`; `Rank = i32`; payload `Int = i32`,
`UInt = u32`; counts and arithmetic `usize`. At most 2³¹ − 1 local entities
of one kind and 2³¹ − 1 local DOFs per rank; global counts up to 2⁶³ − 1.
Size paths from users, files and foreign code use fallible constructors
that return `Error::CapacityExceeded` before any state change.

Reason: `i32` is upstream's default `Idx` and the dtype of its NumPy
connectivity, so foreign buffers are shared without conversion; Cargo
features must be additive, so a width switch is not possible; MPI runs may
exceed 2³¹ global equations.

Consequence: NumPy and C connectivities are `int32` with `-1` sentinels as
upstream. Global systems beyond 2³¹ equations are representable (an
extension) and are checked collectively at native boundaries (32-bit
`MUMPS_INT`, PETSc and Scotch widths, MPI `int` counts). Upstream's
optional 64-bit `Idx` build is not reproduced. Mesh and DOF code must call
`checked_local_dofs` before resizing. Governs `akantu_core::types`.

## D-013: Element-generic code and spatial dimension

2026-11-09. Per-type constants live in static tables in `akantu-core`; each
element type is a zero-sized type implementing `ElementClass` and a family
kernel trait (`LagrangeElement`, `StructuralElement`, IGFEM in its crate);
`with_element_class!` dispatches once per `(type, ghost)` batch. The
spatial dimension is a runtime value at API level and a `const DIM` in
kernels; materials are `MaterialX<const DIM>` behind `dyn Material`.

Reason: upstream uses `ElementClass<type>` templates and `tuple_dispatch`;
hot loops must stay monomorphised and allocation-free.

Consequence: one dispatch per batch. `FeEngine` covers every inventoried
upstream FE operation, including `inverse_map` (100 iterations, 1e-10, as
upstream), arbitrary-point shapes, elemental-field interpolation,
configurable integration orders and cohesive interpolation; point elements
store zero derivative components.

## D-014: Sparse storage and linear-solver back-ends

2026-11-09, amended by D-040. `SparseMatrixAij` stores global equation
coordinates and reproduces upstream's lookup key, first-inserted
coordinates, profile order and value selection separately.
`SymmetryPolicy::PromoteOnUnsymmetric` is the default (UD-034 corrected:
an unsymmetric block makes the matrix unsymmetric, agreed on every rank
and propagated to derived matrices and back-ends) and
`UpstreamUpperSelection` reproduces upstream. `add_values` checks every
entry before changing anything. `EquationLayout` makes owned, shared and
pure-ghost equations explicit; Dirichlet diagonals go on owners only;
distributed products use the UD-003-corrected reduction. The DOF manager
dispatches over `Box<dyn LinearSystemBackend>` (`AijBackend`, and
`PetscBackend` with one ordering for rows, columns and vectors, UD-006).
Solver back-ends declare `BackendCapabilities`.

Reason: upstream's COO matrix drops half of an unsymmetric block added to
a symmetric matrix (UD-034); MUMPS takes distributed input and a
centralised right-hand side; PETSc needs its own objects; the default
build must be pure Rust.

Consequence: COO arrays equal upstream's (hand-derived contract tests in
`crates/akantu-linalg/tests/aij_upstream_semantics.rs`). Bitwise identity
with the oracle's MUMPS is an observed result for a pinned configuration,
never a promise (D-050). Implicit cohesive replays that depend on
upstream's selection set `UpstreamUpperSelection`.

## D-015: Communicator and MPI

2026-11-09. One cloneable `Communicator` over three back-ends: serial, an
in-process local world (threads as ranks, rank-ordered collectives,
mismatch detection, timeouts, optional rendezvous sends) and MPI through
rsmpi (feature `mpi`). Non-blocking operations own their buffers. The MPI
lifecycle distinguishes imported and owned initialisation, enforces the
thread level, finalises after the last environment guard on MPI's main
thread, refuses re-initialisation, duplicates imported communicators and
uses `MPI_ERRORS_RETURN`. Clones of a rank's communicator share a registry
of live phase scopes (D-018).

Reason: real MPI is required, the default build must work without it, and
distributed code must be testable in the fast test tier.

Consequence: identical code paths for serial, local-world and MPI runs;
distributed tests run without MPI; MPI builds need an MPI implementation
and libclang. MPI stays unfinalised if `shutdown` never runs on the main
thread, which the exit hook of D-067 handles. Governs `crates/akantu-comm`.

## D-016: Constitutive laws, parameters and factories

2026-11-09. Built-in laws keep static parameter descriptor tables and typed
internal fields; runtime laws (Python subclasses, user Rust laws) own
`DynamicParameters` (upstream's access and kind rules, no read before a
value exists, UD-022) and `DynamicInternals`. Laws reach the model only
through a phase-scoped `LawContext { phase, services: &dyn ModelServices }`;
factories receive the services; runtime solid laws are `ForeignMaterial`.
Parameter values from input files are converted by the parser's
`coerce_to_kind`, and upstream's `std::string` parameters keep their bytes
(`ParamKind::ByteString`). The non-local manager talks to laws through
object-safe interfaces defined in `akantu-model` and implemented in the
physics crates.

Reason: upstream registers parameters and internals at run time (also
from Python), gives laws a back-reference to the model and runs a
non-local callback cycle; Rust forbids self-references and aliased
mutable access.

Consequence: unchanged `.dat` files and Python custom materials;
`akantu-model` never depends on `akantu-solid`. The non-local summation
order is settled by D-062 and D-097.

## D-017: Shared-memory threads

2026-11-09. Execution is single-threaded by default. Optional `rayon` is
allowed only for kernels whose threaded result is bitwise equal to the
serial one (disjoint outputs, no unordered reductions, no concurrent
assembly), each with a serial-against-threaded test. Native back-ends run
with fixed, recorded thread counts.

Reason: upstream is MPI-only and the port is bitwise reproducible.

Consequence: threads add no nondeterminism.

## D-018: Errors and agreement on failure

2026-11-09. One `akantu_core::Error` (`#[non_exhaustive]`, stable
`code()`), `Result` everywhere, no panic on user, file or remote input.
Collective operations are sequences of named phases with agreements
(`akantu_comm::phase`: a bounded all-reduce of status, phase id and an
optional digest in which every rank takes part); communication happens
only after agreement; a failed scope stops communicating and keeps the
agreed severity. `Error::Fatal { kind: Transport | Agreed }` is the signal
of a poisoned owner (DOF manager, mesh domain): an agreed fatal failure
poisons the owner on every rank, a transport failure breaks the
communicator and leads to an abort. Code keys on `Error::fatal_kind()`,
never on message text. Communicating callbacks use the enclosing scope.

Reason: upstream throws exceptions; a rank failing between collective
phases leaves its peers waiting.

Consequence: injected rank-specific failures end with errors on every rank
instead of hangs; Python and C errors map from the variants. Governs
`crates/akantu-comm/src/phase.rs`, `crates/akantu-core/src/error.rs`.

Amended by D-126 (2026-11-18): by default an agreement is spent only at
commit points and before communication; the agreement after every phase
is the mode `every-phase`.

## D-019: Mesh ownership and mesh events

2026-11-09. `SharedMesh = Arc<MeshDomain>` owns the mesh, its facet mesh
and a consumer registry. Topology changes go only through
`SharedMesh::mutate`, given the handler of every registered consumer:
preflight, journaled topology edits, mesh-owned dependents, distributed
hook, global event counts, pre-commit capability check (D-068), commit,
dispatch in recorded order and in `(priority, registration)` order with
one agreement per event, consumer and level, acknowledgement. Locks are
never waited for; a post-commit failure poisons the domain on every rank;
poisoned locks are not recovered. Events are transaction data, and a model
is one consumer at several priority levels.

Reason: upstream calls registered handlers synchronously while it updates
mesh-owned dependents, global ids and synchronizers; a dispatch after an
immutable change could not reproduce that or keep consumers consistent.

Consequence: no half-mutated mesh, no missed or duplicated event, no
deadlock. A mutation needs a coordinator that owns every consumer (a
model, a coupler or the Python simulation object), a documented deviation
from upstream's implicit observer list. Governs
`crates/akantu-mesh/src/{shared,mutation,events}.rs`; the architecture
text describes the stages.

## D-020: Mesh partitioner

2026-11-09. `MeshPartitioner` has Scotch (feature `scotch`), prescribed
partitions (the default of differential MPI tests), mesh-data partitions
and a pure-Rust recursive coordinate bisection that is never equated with
Scotch. `PartitionMetadata` records partitioner kind and version,
strategy, seed and `SCOTCH_randomReset`, `SCOTCH_Num` width and a hash of
the ordered dual graph and weights. Distribution is centralised (global
mesh on the root), which is the capacity limit of that path.

Reason: partitions influence parallel results; Scotch results depend on
seed, graph order and widths.

Consequence: a partition is claimed equal to the oracle's only when all
metadata match and `SCOTCH_PTHREAD_NUMBER=1` (multi-threaded Scotch 7 is
not reproducible).

## D-021: Solver ownership and coupling

2026-11-09. Each model is `{ physics (SolverCallback), solver
(ModelSolver: DofManager and named TimeStepSolvers), token }`, so
`solver.solve_step(&mut physics)` is a disjoint borrow. Coupling is
`Staggered<A, B>` (independently owned components with their own solvers,
explicit transfers) or `Monolithic<P>` (one shared solver, a combined
physics value). Intercepting user callbacks run through `Intercept { user,
inner }` with a `Delegate` valid only during the solve. Native non-linear
solvers reach model and manager only through `NativeSolveContext`.

Reason: upstream's model is a solver callback that owns its DOF manager and
solvers; solid-contact shares one DOF manager while solid-phase-field
solves two models in sequence; Python intercepts callbacks.

Consequence: borrows are checked at compile time, the solver path has no
interior mutability, and every upstream coupling flow is representable.
Governs `crates/akantu-model/src/coupling.rs`, `crates/akantu-solver`.

## D-022: Geometry

2026-11-09, superseded in part by D-042. The geometry code follows CGAL's
formulas: the LGPL `Simple_cartesian<double>` kernel, intersections and
exact number types are ported into `akantu-geometry`, the GPL parts into
`akantu-geometry-cgal` (D-042). Snapping and duplicate suppression use
Eigen's `isApprox` (relative, 1e-10) as upstream; output is in primitive
order. The tangent-point test of `mesh_sphere_intersector_tmpl.hh:98` is a
registered defect with a switch between the intended and the upstream
behaviour.

Reason: upstream's geometry is CGAL; bitwise CGAL behaviour, including
AABB traversal order, needs CGAL's code.

Consequence: the differential tests against the CGAL-enabled oracle are
the evidence; the LGPL default build has the segment intersector with a
linear-scan search structure.

## D-023: Process-global runtime

2026-11-09. Library APIs take parser, arguments, communicator, factories,
tolerance and seed explicitly; `akantu::runtime` provides upstream's
global defaults behind a `RwLock` and owns the process MPI lifecycle.
Callbacks receive phase-scoped contexts, never a global lock held across
them; factories are values passed to models.

Reason: upstream relies on global singletons.

Consequence: a testable, thread-safe core with upstream's convenience at
the top; `runtime::initialize` and `finalize` are upstream's `initialize`
and `finalize`. Governs `crates/akantu/src/runtime.rs`.

## D-024: Python and C bindings

2026-11-09. The Python module `py11_akantu` uses PyO3 0.30 and rust-numpy
0.30 with upstream's names. Arrays are exported under the foreign-view
contract: only `SharedArray` can be exported; exported allocations are
never reallocated in place (growth beyond capacity, any shrink,
replacement or destruction retire the block, equal-length copies stay in
place); views are attached until retirement, then stale but safe (UD-085,
UD-024); native guards on exported arrays are admitted only inside a
native section of the array's `ForeignRuntime`, foreign accesses only in
its foreign phase with its `Interpreter` (the GIL stand-in); callbacks into
foreign code run only when no native guard is alive. Every binding entry
point that touches model state enters a native section and never releases
the GIL inside it. `getModel()` inside callbacks is a phase-scoped handle.
The C interface planned here (opaque handles, borrowed views) is not built
(D-100).

Reason: upstream hands raw `Array` pointers to NumPy that dangle after a
resize, restore or destruction, and nothing orders NumPy accesses against
native computation.

Consequence: no undefined behaviour from retained views or concurrent
native and foreign access in safe Rust (checked under Miri, Stacked and
Tree Borrows). A script that keeps a view across a topology change sees
the old values. Residual user obligation (UD-085): a NumPy operation that
releases the GIL on an akantu view in one thread must not overlap an
akantu call in another thread. Governs `crates/akantu-core/src/array_*`,
`crates/akantu-python/UNSAFE.md`.

## D-025: Pinned Rust toolchain

2026-11-09. `rust-toolchain.toml` pins Rust 1.99.1 with rustfmt and
clippy; a bump is a deliberate, separate commit after a clean clippy run.

Reason: reproducible lint and format results on every machine.

Consequence: Miri runs on nightly for changes to `unsafe` code or the
foreign-view contract.

## D-026: Workspace decomposition

2026-11-09. The crates form a directed acyclic graph (`docs/ARCHITECTURE.md`).
Cross-crate services are defined in the lower crate and implemented above
it: FE operations (D-013), linear-system back-ends (`akantu-solver`),
non-local callbacks and weight functions (`akantu-model`), dump field
resolution (`akantu-io`), model services for laws (`akantu-model`), random
descriptors bridged by the parser crate.

Reason: upstream's include graph is cyclic.

Consequence: no crate depends on a crate above it; Cargo enforces it.

## D-027: Abaqus mesh format

2026-11-09. Selecting Abaqus returns `Error::Unsupported`.

Reason: upstream has only the enum value `_miot_abaqus`;
`mesh_io_abaqus.cc` does not exist and selecting it dereferences a null
reader (UD-049).

Consequence: an Abaqus importer would be a labelled extension, not a
parity item.

## D-028: Upstream code that is never built or never called

2026-11-09, extended 2026-11-11 and by D-076. Code that no upstream target
compiles or that is never called is not ported; its behaviour stays
documented in `docs/inventory/` and in its parity row, marked
*Exception (approved)*: `TimeStepSolverDefaultExplicit` (declaration only,
TS-012); the legacy periodic-boundary map `mesh_utils_pbc.cc` (PART-014,
MUTIL-009: built but never called); the IOHelper restart writer and reader
and C facades (DUMP-019 to DUMP-022) and Base64 decoder (DUMP-023); the
explicit 6-, 8- and 21-point pentahedron Gauss tables (QUAD-018, UD-056);
`buildSegmentToNodeType`, removed upstream in 2018 (MUTIL-018); the tests
`generate_test` (TEST-213), `test_segment_nodetype` (TEST-214), the
stand-alone PETSc dynamic test (TEST-231) and the CI helper headers
(TEST-234). The compiled IOHelper LAMMPS writers are ported.

Reason: the scope requires every inventory item to be implemented or to
have an approved exception, and these items have no behaviour a program
can reach.

## D-029: Golden fixture format and differential harness

2026-11-09. One directory per case under `fixtures/<area>/<case>/` with a
`manifest.json` (provenance: reference commit and patches, build
configuration, compiler, flags, architecture, library versions, generator
command, content hashes of patched files, probe tree and libraries; inputs
with SHA-256; quantities with dtype, shape, axis vocabulary, layout,
element and ghost type, ordering keys and ragged structure; steps;
parallel layout; expected errors; status) and one little-endian C-order
`.npy` file per quantity. Tolerances live in reviewed `tolerances.json`
files (first match wins, mandatory justification, no implicit default).
Cases larger than 1 MiB are external (manifest committed, data
regenerated). Generation is staged and promoted only after every target and
a determinism re-run succeeded. Matrices are exported bit-exactly from the
AIJ arrays.

Floating-point values in JSON or CSV are compared as text (17 significant
digits identify a binary64 value) or after a correctly rounded parse;
the workspace's `serde_json` enables `float_roundtrip`, and tests carry no
parser of their own. An upstream golden file is evidence of the strength
its own driver gives it: if the driver compares text, the port compares
the same text byte for byte.

Reason: data must be exact, writable from C++ and Python without heavy
dependencies, and readable from Rust without `unsafe`. HDF5 (native
dependency, opaque in git), JSON numbers (lossy), `.npz` (hides per-array
checksums) and raw binary (not self-describing) were rejected.

Consequence: fixtures are inspectable with NumPy and verifiable by
checksum. Governs `crates/akantu-validation`,
`oracle/probes/include/akantu_probe/fixture_writer.hh`,
`oracle/probes/python/akantu_probe.py`, `docs/NUMERICAL_VALIDATION.md`.

## D-030: Source provenance

2026-11-09, amended 2026-11-09 and by D-042. Code under a licence that does
not allow a source translation to be distributed under LGPL-3.0-or-later
(GPL-only, GPL with exceptions such as libstdc++ and libgcc, proprietary)
is never read for translation, and its compiled code is not disassembled
or inspected either. Compatibility with such code is reproduced
clean-room, from public specifications and published algorithms and from
black-box input and output testing of the compiled binary, by someone who
has not read the original source. Code under LGPL-2.1-or-later (glibc),
LGPL-3, MPL-2.0 (Eigen, file level), MIT, BSD or Apache may be ported with
its notices; compiled code of such libraries (glibc's libm) may be
followed instruction by instruction. CGAL's GPL packages are the one
exemption (D-042).

Reason: the GCC Runtime Library Exception covers compiled target code, not
the relicensing of a source translation.

Consequence: the random engine and distributions (D-006), stream
conversions and number printing of the parser, the complex division
(D-073) and the libstdc++ algorithms used by the geometry are clean-room,
each with a log (`crates/akantu-core/src/random/CLEANROOM_LOG.md`,
`crates/akantu-parser/CLEANROOM_LOG.md`,
`crates/akantu-linalg/src/dense/COMPLEX_CLEANROOM_LOG.md`,
`crates/akantu-geometry/CLEANROOM_LOG.md`). Extending the CGAL exemption
to other GPL code needs a new decision.

## D-031: Floating-point flags of the `strict` oracle build

2026-11-09. The `strict` oracle adds `-ffp-contract=off
-DEIGEN_DONT_VECTORIZE` (`-ffp-contract=off` for Fortran) to upstream's
flags (RelWithDebInfo `-O2 -g`, `-frounding-math` from CGAL, LTO on
`libakantu`). The `default` build keeps upstream's flags and Eigen
vectorisation, to measure the variation between two legitimate builds.

Reason: Rust never contracts `a*b + c` and has no fast-math. GCC 14 on
aarch64 contracts by default, and Eigen 3.4.0's NEON path calls an FMA
intrinsic whatever `-ffp-contract` says. With both flags the Eigen kernels
compile to zero fused instructions.

Consequence: `strict` results are the primary golden data. The `strict`
`libakantu.so` has 14 fused instructions, all in libgcc's `__divdc3`.
Residual non-strictness: Debian-built MUMPS, PETSc, Scotch and OpenBLAS
inside implicit solves. Programs linked against the strict install must use
the same flags (`share/akantu-oracle/oracle-flags.cmake`). Multi-rank
outputs repeat only with a fixed `AKA_SEED` (upstream seeds Scotch from the
clock) and then only up to a permutation of parts among ranks;
`docs/ORACLE_BUILD.md` lists the outputs and conditions for which `strict`
reproducibility is claimed.

## D-032: Oracle coverage of packages that do not build

2026-11-09. One build configuration covers everything that compiles.
`IGFEM`, `extra_materials(_non_local)` and `embedded` are off in both
oracle builds: at the reference commit they include removed headers, use
the Akantu 4 API or contain unfinished work (`embedded` also has a macro
arity regression). `src/geometry` is written for CGAL 5.x, so the image
uses the pinned, checksum-verified header-only CGAL 5.6.2 instead of
Debian's CGAL 6, and `CGAL` is on. `traction_at_split_node_contact` is on.

Reason: fixing those packages would mean finishing upstream development.

Consequence: no unpatched oracle exists for IGFEM, extra materials and the
embedded model; their parity rests on code reading and independent
mathematics (D-007), or on a patched variant (D-060; the one-line
`embedded` fix is in `oracle/patches/unused/`).

## D-033: Rank counts of upstream's parallel tests

2026-11-09. The oracle build applies `oracle/patches/0001-…` (test
registration only) and sets `MPIEXEC_MAX_NUMPROCS=8`, so parallel tests run
on 8, 4, 2 and 1 ranks; tests with an explicit `PARALLEL_LEVEL` keep
upstream's values.

Reason: `cmake/AkantuTestsMacros.cmake` assigns the variable name instead
of its value, so upstream registers every such test with one rank only
(UD-015).

Consequence: multi-rank results of these tests were never exercised
upstream; their failures are classified in
`oracle/results/known-failures.json` (MUMPS gather test defect UD-075,
Eigen unsupported in parallel, unfinished PETSc path, DOF-manager contract
mismatch on unowned nodes UD-074, implicit dynamics UD-077, `ReceiveAny`
UD-076), and multi-rank output is qualified under D-035.

## D-034: Reproducible, network-free oracle configuration

2026-11-09. The oracle uses Debian's Eigen 3.4.0 and pybind11 2.13.6,
feeds upstream's googletest FetchContent path with Debian's googletest
sources, sets `FETCHCONTENT_FULLY_DISCONNECTED=ON`, configures with
`cmake --fresh` from `oracle/configs/`, pins the base image by digest,
records package versions in `oracle/docker/packages.tsv` and sets
`OPENBLAS_NUM_THREADS=OMP_NUM_THREADS=1`.

Reason: upstream otherwise downloads unpinned googletest `main`, Eigen
5.0.1 and pybind11.

Consequence: configure is network-free. Reproducibility is scoped to the
recorded image (`oracle/docker/image-id.txt` and its package manifest):
apt packages are not version-pinned, so a rebuilt image may differ, and
`build-image.sh` reports manifest drift and promotes nothing unless asked.
A dated `snapshot.debian.org` source with exact versions would make
rebuilds exact; it was not adopted because it could not be tested without
a full uncached build.

## D-035: Trust policy for multi-rank oracle results

2026-11-10. A multi-rank oracle case becomes golden data only if its
serial counterpart passes, it is repeatable (bitwise across at least three
runs of the recorded outputs, with a fixed recorded `AKA_SEED` or a
prescribed partition, compared per mesh part or global id rather than per
rank, at full precision), it is not in a known-failing family, and it
agrees with the serial result within the documented tolerance class where
the physics is partition-independent. Parallel implicit dynamics,
`ReceiveAny` and PETSc parallel paths are excluded until resolved.

Reason: upstream never ran its parallel tests on more than one rank
(UD-015) and seeds Scotch from the clock (UD-086).

Consequence: Rust parallel correctness also rests on serial equivalence,
partition independence and analytical checks.

## D-036: Eigen's blocked product order is ported

2026-11-10. Eigen's scalar `gebp` product order (strict build: even and odd
accumulator chains in 4-column panels for depth ≥ 8, Eigen 3.4.0's
balanced depth blocking `kc`, the `dst = acc·α + dst` write-back, the two
separately routed `computeBtDB` products) is ported under MPL-2.0 in
`crates/akantu-core/src/eigen_gebp.rs`.

Reason: element stiffness `BᵀDB` and other FE products go through Eigen's
GEMM route in the oracle; the order is deterministic and cheap to
reproduce (D-008).

Consequence: FE, material and structural products compare bitwise. The
GEMM tolerance class remains only for paths outside the bitwise claim.

## D-037: Mesh reading never communicates

2026-11-10. Mesh readers compute bounding boxes and groups locally while
staging and fill the mesh by moves; no read path has a collective. Global
bounding boxes come from distribution.

Reason: upstream's `Mesh::read` never communicates (a read target is never
distributed), and reading on rank 0 only is an upstream pattern.

Consequence: reading on the root before `distribute` works on any
communicator; `Mesh::compute_bounding_box` reduces across ranks only for
distributed meshes.

## D-038: Thermal energy of thermo-elastic materials

2026-11-10. The default energy is the stored elastic energy
`W = ½(ε − ε_th):C:(ε − ε_th)` (Green strain and PK2 for finite
deformation; plane strain uses the 3-D energy per unit thickness with
ε₃₃ = 0), verified by finite differences `∂W/∂∇u = σ` in 1-D, plane stress,
plane strain and 3-D. Upstream's `½ σ:ε` is a compat switch
(`LawOptions::upstream()`).

Reason: upstream's value is not a potential of the computed thermal stress
at fixed non-zero temperature (UD-109).

Consequence: without thermal stress both conventions are bitwise equal.

## D-039: Reductions of floating-point values across ranks

2026-11-10, amended 2026-11-11. Collective reductions and scans fold the
rank contributions in rank order on every back-end. A `NativeMpi` policy
selects MPI's predefined reductions consistently (all-reduce, reduce,
scans, min/max-loc) for oracle qualification and performance comparison;
the policy is part of the agreed collective descriptor and is inherited by
duplicates and splits. Synchronizer slave-to-owner reductions also fold
remote contributions in rank order.

Reason: MPI's association order depends on the implementation, rank count
and message size (a finite sum of `[1e16, 1, -1e16, 1]` matches Open MPI
5.0.7 at 1 and 2 ranks and differs at 4 and 8).

Consequence: rank-ordered results are deterministic for a fixed set of
contributions; native mode promises no bit identity across MPI
implementations. Rank folding costs O(P × L) memory and is limited to
bounded payloads. Upstream folds synchronizer contributions in arrival
order (`waitAny`), so an entry is run-dependent upstream when at least two
remote contributions are non-zero and their permutations differ; oracle
fixtures of such reductions record the per-rank contributions and mask
exactly the order-dependent entries, where the port must equal the
rank-order fold of the recorded contributions. Fixture tolerances are not
loosened because rank order differs. Where upstream is run-dependent
through its solver, D-082 applies.

## D-040: `eigen` is the bitwise SparseLU translation, `faer` an extension

2026-11-10, amends D-014. The default build has two pure-Rust sparse direct
solvers. `SparseSolverType::Eigen` (`eigen`) selects `EigenSparseLuSolver`,
a translation of Eigen 3.4.0 `SparseLU` and COLAMD qualified bitwise
against the pinned strict configuration only. `SparseSolverType::Faer`
(`faer`) selects `FaerLuSolver`, a port extension qualified by residual,
conditioning, singular and indefinite cases and non-linear histories,
without a bitwise claim. Neither substitutes for the other after a
failure. `auto` keeps upstream's resolution (MUMPS if registered and
available, else `eigen`); `faer` is selected explicitly only. The parser
key `sparse_solver_type` accepts upstream's values plus `faer`.

Reason: the Eigen translation keeps implicit fixtures generated with
`eigen` comparable bitwise; faer was the original replacement.

Consequence: Eigen and faer systems are centralised on rank 0 through the
enclosing agreed scope; direct use of a serial-only solver with several
processes is unsupported. Convergence thresholds are unchanged.

## D-041: Solver-stack contracts

2026-11-11. (1) `IntegrationScheme` binds its DOF family at construction
and provides `invalidate_jacobian()`. (2) `get_new_solver` and
`set_integration_scheme` are single agreed collectives; the model's
`init_solver` runs inside such a step. (3) Each time-step solver records
the Jacobian's release after its own assembly and makes its schemes
rebuild `J` when another solver changed it (UD-193); upstream's stale
shared `J` is a compat behaviour. (4) Category-B defects of the solver
stack are switched per DOF manager through `UpstreamCompat`, one value for
every solver of that manager (amended by D-055). (5) Solver kinds are
`SparseSolverType` values; multi-rank AIJ solves use `create_centralized`.
(6) `residual_mass_wgh` is refused by default (UD-194) and available in
compat mode only.

Reason: upstream's solver objects share state implicitly; the port makes
the sharing explicit and agreed.

Consequence: a shared `J` keeps the matrix type of the solver that created
it first (UD-203, a documented latent limitation that no model reaches).

## D-042: CGAL's GPL packages are ported in an isolated GPL crate

2026-11-11. The GPL CGAL packages that upstream's `src/geometry` uses may
be read and ported. GPL-derived code lives only in
`crates/akantu-geometry-cgal` (Cargo `license = "GPL-3.0-or-later"`; every
file has `// SPDX-License-Identifier: GPL-3.0-or-later`, names its CGAL
source files, keeps their copyright notices and marks the GPL-derived
parts). The crate is opt-in: facade feature `cgal`, off by default like
upstream's `CGAL` option. CGAL's LGPL parts may be ported into LGPL crates.

Reason: bitwise CGAL behaviour (AABB traversal order, the algebraic
number handling of the spherical kernel) needs CGAL's code; CGAL 5.6.2's
`AABB_tree`, `Circular_kernel_3` and `Algebraic_kernel_for_spheres` are
`GPL-3.0-or-later OR LicenseRef-Commercial`, its kernels and number types
`LGPL-3.0-or-later OR LicenseRef-Commercial`.

Consequence: a build with `cgal`, with `igfem` (which implies it) or with
`akantu-igfem/cgal` is GPL-3.0-or-later as a whole; the default build stays
LGPL-3.0-or-later. These three are the only declared GPL switches, guarded
by `crates/akantu-geometry/tests/license_separation.rs`; adding one is a
licensing decision. D-030's clean-room rule still applies to all other GPL
code. Governs `NOTICE.md`, `docs/DEPENDENCIES.md`, the crates' READMEs.

## D-043: x86_64 Linux is the authoritative platform

2026-11-11. x86_64 Linux (glibc, the oracle's Debian base) is the
authoritative platform for oracle fixtures and bitwise parity claims. The
oracle (`strict` and `default`) and the MPI, Scotch and MUMPS images are
built for `linux/amd64`, the fixture corpus is regenerated there and
diffed against the aarch64 corpus value by value, and every difference is
classified (libm, libgcc, fused or unfused, vectorisation, MPI order).
Where they differ, the x86_64 value is the golden value and the aarch64
value a recorded platform variant (D-073), never a loosened tolerance. The
Rust test suite must pass on x86_64 Linux; macOS arm64 is a supported
development platform with enumerated libm exceptions.

Reason: upstream's CI and its published cohesive benchmark run on x86_64.

Consequence: residual architecture differences come from glibc's libm
variants, libgcc helpers and code built without the strict flags (MUMPS,
Scotch, BLAS). Governs `docs/ORACLE_BUILD.md`,
`docs/NUMERICAL_VALIDATION.md`, `docs/PLATFORMS.md`.

## D-046: Default NaNs across platforms under exact rules

2026-11-11. Under an exact (bitwise) rule two `f64` values match if their
bit patterns are identical, or if both are default quiet NaNs that differ
only in the sign bit (`0x7ff8000000000000` and `0xfff8000000000000`).
NaNs with a payload, signalling NaNs, NaN against non-NaN, ±0, infinities
and finite values stay bitwise. The rule is implemented once, in the
`akantu-validation` comparator; digest helpers canonicalise a default NaN
to `0x7ff8000000000000` before hashing. A golden case that contains NaN
documents which quantities, from which step and why, in its manifest and
in `docs/NUMERICAL_VALIDATION.md`.

Reason: IEEE 754 leaves the sign of the NaN of an invalid operation to the
implementation: aarch64 produces `0x7ff8…`, x86_64 SSE `0xfff8…`, and both
the oracle and the port produce their platform's default.

Consequence: one golden corpus serves both platforms; this is not a
tolerance on computed values. An undocumented NaN in a fixture is a defect
of the fixture.

Addendum (2026-11-17, kernel tests in payload mode): the bitwise kernel
tests feed a single NaN with a payload among finite values. For an
algorithm that can combine that NaN with a negated copy of itself (an
iterative kernel that negates its inputs, such as the Householder updates
of the fixed-size `EigenSolver`), the sign of a NaN result is free and the
payload stays exact: when both operands are NaNs with the same payload and
opposite signs, which one IEEE 754 returns is unspecified and the compiler
may commute the operation. The rule is
`akantu_core::bitwise_testing::same_payload_sign_free`; kernels that never
meet their input NaN twice keep the exact comparison.

## D-047: Python arrays are zero-copy views

2026-11-11. Every array Python sees for model, mesh and material data is a
zero-copy `SharedArray` view under the D-024 protocol; there is no mirror
or synchronisation layer.

Reason: synchronised mirrors gave silently wrong results for legitimate
upstream-API programs: writes inside Python boundary-condition functors
were lost, the current position was stale after a Python write (a stable
time step twice too large), `Material.setReal` skipped the
synchronisation, and callbacks could not call model getters.

Consequence: exported fields are stored as `SharedArray` and reached
through guards (D-056, D-071 give the exceptions and rules).

## D-048: `SpatialGrid` lives in `akantu-core`

2026-11-11. The one spatial grid of the port is
`akantu_core::spatial_grid::SpatialGrid` (upstream keeps it in
`src/common`), used by the grid synchronizer, the non-local
neighbourhoods and the contact detector, and re-exported by
`akantu-geometry`. The per-direction cell-id function that upstream's
`GridSynchronizer` uses as a friend is public.

Reason: the grid needs nothing from geometry, and one implementation avoids
orphan-rule conflicts between the crates that implement traits for it.

## D-049: Structural element kernels and `StructuralFeEngine`

2026-11-11. Bernoulli beams (2-D, 3-D) and DKT18 have their own engine
`akantu_fe::structural::StructuralFeEngine` and the primitive trait
`StructuralElement` (rotation, `N`, `dN/dξ`, `B` as separate operations;
the products `N·T` and `B·T` in the engine); the regular `FeEngine`
refuses structural kinds. Every operation that reads a cached block takes
the mesh and checks freshness (mesh revision, element count, exact bits of
the `extra_normal` rows) before producing anything; a failed refresh drops
the affected blocks. Degenerate geometry is an error (UD-296, A). The 3-D
beam θy row is corrected by default (UD-293, B) and
`StructuralUpstreamCompat` reproduces upstream (oracle fixtures replay
bitwise in that mode). A 3-D beam normal that is not a unit vector
orthogonal to the axis (`|n·n − 1|` or `|n·t̂|` above 1e-10, input
validation) is refused by default; admissible normals are used raw; a
legacy switch disables the check (UD-343). DKT `N` is
`Error::NotImplemented` (UD-053, UD-295).

Reason: structural `N·T` and `B·T` widths are DOF and stress counts, not
shape counts, so the regular engine's storage does not fit.

Consequence: the engine never serves data it knows to be stale. Governs
`crates/akantu-fe/src/structural/`.

## D-050: Eigen's cache sizes and BLAS are pinned in the oracle

2026-11-11. The authoritative oracle runs with Eigen's cache sizes pinned
to Eigen's own defaults (L1 = 16384, L2 = L3 = 524288 bytes) in every
probe, generator and example run, applied through the oracle tooling and
recorded in each fixture's provenance; either a run-time
`setCpuCacheSizes` or `-DEIGEN_NO_CPUID` with these values is acceptable.
The port keeps its fixed 16 KiB blocking. Results that go through OpenBLAS
(MUMPS, PETSc direct solves) run with `OPENBLAS_CORETYPE=Haswell` and
`OPENBLAS_NUM_THREADS=1` in the oracle image and every project image that
runs BLAS-backed solvers. Comparisons against an unpinned upstream binary
are diagnostic, not golden.

Reason: Eigen sizes the blocking of `SparseLU` supernodal updates and
`gebp` from the L1 size, queried from the CPU on x86 and fixed at 16 KiB on
aarch64; any L1 of 24 KiB or more changes the bits of larger fronts (with
the pin all 468 quantities of `fixtures/solver` match bitwise on x86_64).
OpenBLAS selects kernels from the host CPU at run time (MUMPS results fall
into five groups over eight core types, up to 5.9e-13 relative).

Consequence: the aarch64 corpus, the pinned x86_64 oracle and the port's
blocking agree. With the same core type the port and the oracle agree
bitwise on 13 of 13 MUMPS systems at one rank; no x86 core type reproduces
the aarch64 MUMPS fixtures (platform variants, D-073); multi-rank MUMPS
solves are not run-to-run reproducible and stay diagnostic. A port option
that reproduces an unpinned x86 build (L1 above about 20.5 KiB) is not
provided.

## D-051: Exact rules distinguish the sign of zero

2026-11-11. Under an exact rule two `f64` values match only if their bits
are identical, with the single exception of D-046: `+0.0` and `-0.0` do not
match. Digest helpers canonicalise default NaNs only, never zeros.
`fixture-diff`'s value-equality mode reports signed zeros as value-equal
and bit-different, as a measurement and never as a pass condition. Rules
with a tolerance (`abs`, `rel`, `ulps` > 0) compare values, so ±0 are equal
there. The bounding box uses upstream's explicit selection
(`if v < l { l = v }`) and the distributed box is reduced as upstream
(one `Min` of `[lower, -upper]`).

Reason: the only signed-zero difference in the corpus came from a port
defect (`f64::min`/`max` in `BBox::add_point`, whose result for ±0 differs
between platforms and optimisation levels); upstream keeps the first
value seen.

Consequence: every exact rule became stricter; no rule became looser.

## D-052: Cohesive history maxima are committed at the end of a converged step

2026-11-11. The maximal openings a cohesive law reads as history
(`CohesiveLaw::committed_maxima`: `delta max` of `cohesive_exponential`
and, with the UD-241 default, of `cohesive_linear` and `cohesive_bilinear`;
`delta_n_max`, `delta_t_max` of `cohesive_linear_uncoupled`) are committed
at the end of each converged solve step (`after_solve_step(true)`, or the
return path of `solve_step` under the Newton legacy hooks), never at step
entry and never by `on_damage_iteration`. A non-converged step restores
them, and evaluations outside a step are trials against them. Every other
history keeps upstream's save at step entry, so for these maxima "previous"
means "committed at the end of the last converged step" and for every
other field of the same material "saved at the entry of the current step".

Reason: upstream commits a converged step's maxima only when the next step
starts, so a traction evaluated between two steps (the public
`assembleInternalForces`) heals the interface permanently (UD-245).

Consequence: both meanings coincide when nothing is evaluated between
steps, so every explicit oracle replay stays bitwise. Governs
`crates/akantu-cohesive`, `crates/akantu-model/src/internal_field.rs`.

## D-053: The law of new cohesive elements is chosen inside the mesh transaction

2026-11-11. During an insertion the law of every new not-ghost cohesive
element is selected inside the transaction's topology closure, exactly as
the dispatch would select it (the user's selector chain asked once per new
element, in type then event order, each decision written before the next),
and validated there (UD-224, UD-246, law index in range). A refusal or a
selector error is an ordinary recoverable closure error: the mesh is
rolled back, the law-index blocks are truncated and the insertion flags and
appended parameters are restored. The post-commit dispatch reuses the
staged choice and never asks the user's selector again. Ghost elements
receive their owner's staged law through the parallel insertion's
distributed hook and join their laws in rank order (UD-817).

Reason: refusals decided after the commit can no longer be rolled back;
upstream assigns the law in the post-insertion `onElementsAdded`.

Consequence: a custom selector cannot bypass the law checks; element-group
node lists are still pre-insertion while the laws are staged.

## D-054: Native solver objects (MUMPS, PETSc)

2026-11-11. Every native solver object is bound to the communicator of its
successful collective creation and holds a clone of it (so MPI is
finalised only after the object). Identity is the identity of the akantu
handle family (`Communicator::is_same`), not `MPI_Comm_compare`: a
duplicate, split, reordered or sub-group communicator is another one and
is refused. Before any native job every rank agrees the binding, an
instance token and a canonical descriptor of every setting that drives the
native control flow (MUMPS: all `ICNTL`, `CNTL`, the retry right-hand side
and ceiling, the persisted `ICNTL(14)`); every native call carries its job
identity. After every job its global outcome (MUMPS `INFOG(1)`,
`INFOG(2)`, `ICNTL(14)`) is gathered and checked identical; a disagreement
is an agreed fatal error and the object is abandoned. Host buffers are
derived together with the decision that uses them. Native objects are
destroyed collectively (`Communicator::release_native`) on MPI's main
thread, at the same program point on every rank, with no other thread
inside MPI or the library; an initialised object is never freed without
its collective destruction, and a broken communicator abandons it. PETSc
follows the same contract, with its lock strictly inside the process MPI
lock and `PETSC_COMM_WORLD` an akantu duplicate freed after
`PetscFinalize`.

Reason: concurrent or diverging native jobs abort or crash the libraries;
a destruction that waits for other ranks can hang if run on a worker
thread.

Consequence: ranks entering different native jobs fail before the library
runs. Governs `crates/akantu-linalg/src/{mumps,petsc}/`,
`crates/akantu-comm/src/communicator.rs` (`native_collective`,
`native_region`, `release_native`).

## D-055: Sparse-solver switches belong to the DOF manager's `UpstreamCompat`

2026-11-11, amends D-041 (4). The sparse solvers' compat switches
(`akantu_linalg::direct::SparseSolverCompat`, today
`mumps_retry_with_solution_rhs` for UD-216) are derived from the DOF
manager's `UpstreamCompat` and handed to the back-end before every solve,
whatever solver is installed. For a solver driven by a DOF manager the
manager's switch overrides the per-object option (`MumpsOptions::retry_rhs`
is for standalone use only). The setting is part of the solver's agreed
control-flow descriptor.

Reason: a per-object option could not be reached by a manager-driven
solve (factory `auto`, Newton reallocating its solver).

Consequence: one manager-wide value applies to every solver of that
manager; corrected by default.

## D-056: Exportable storage: element filters stay plain arrays

2026-11-11. Constitutive-law element filters
(`ConstitutiveLaw::element_filter(_mut)`, `MaterialBase::element_filter`)
are plain `ElementTypeMapArray`s, not exportable storage; Python's
`getElementFilter()` returns a read-only snapshot copy. Per-element mesh
accessors (`element_connectivity`, `elements_of`, `iter_elements`,
`barycenter_into`) do not allocate: they return guards that dereference to
slices. The built-in Dirichlet and Neumann functors take their guards once
per group.

Reason: upstream's returned NumPy view aliases an array upstream may
reallocate, and a write corrupts the law (the class of UD-536); making
filters exportable would change every material's trait signatures.

Consequence: a documented difference from upstream for
`getElementFilter()` (UD-536, UD-747). Governs
`crates/akantu-core/src/array_shared.rs`, `crates/akantu-model`.

## D-057: How an intercepting callback reaches the model

2026-11-11. `SolverCallback::as_any_mut` (default `None`) and
`Delegate::split_as::<T>()` / `split_as_ref::<T>()` give an intercepting
callback a safe downcast of the model callback together with the DOF
manager, for one callback invocation. Through it the callback may read the
model state and call the model's refresh entry points that upstream's
getters run implicitly (`refresh_current_position`); every solve step and
collective goes through the delegate.

Reason: upstream's `InterceptSolverCallback` reaches the model through
its stored reference; the port has no back-reference.

Consequence: no `unsafe`; read-only users take `split_as_ref`.

## D-058: Split-node contact keeps a change journal

2026-11-11. The traction-at-split-node contact owns a `SyncJournal`, an
append-only list of the structural changes of its node list
(`SyncRecord::Added`, `SyncRecord::Deleted`). It propagates its own
dependent arrays directly; a friction object remembers its journal
position and replays the records since then at every entry point that
reads the contact, so it never acts on a stale node list.

Reason: upstream registers dependent arrays by pointer.

Consequence: semantics equal upstream's registration (catch-up at
registration, default values for added records, the same swap-erasures in
the same order). Governs `crates/akantu-ntn-contact/src/sync_array.rs`.

## D-059: Lumped rotary inertia of 3-D beams

2026-11-11, amended 2026-11-12. The lumped rotary inertia of 3-D beams is
the exact nodal block `Σ_e R_e·diag(J_t, J_b, J_b)·R_eᵀ` on the three
rotations of each node (option `Block3x3`, default); `IsotropicMax` (a
diagonal, objective `max(J_t, J_b)`) is the alternative for users who need
a diagonal mass. There is no upstream option. The DOF manager supports
symmetric 3×3 lumped couplings for it (serial only).

Reason: upstream's `assembleLumpedMassMatrix` takes `node2 = conn(0)`
(`structural_mechanics_model_mass.cc:126`), so every element has length 0
and its lumped masses and rotary inertias are exactly 0 (an explicit step
divides by them, UD-514); a zero mass on a free DOF is refused (UD-518,
A), so upstream's values cannot be reproduced. An isotropic rule has
torsional frequencies 82 %, 65 % and 29 % too low at 3, 6 and 12 elements
when elements are longer than about 4.4 radii; `Block3x3` is within
−1.14 %, −0.29 % and −0.07 %.

Consequence: UD-059 is corrected without a compat switch (as UD-005).
`StructuralModelUpstreamCompat::UPSTREAM` keeps `Block3x3`; oracle replays
compare no lumped value and check upstream's zeros exactly. `Block3x3`'s
stable time step is never longer than `IsotropicMax`'s (equal on coarse
straight members, 1.39 and 2.41 times shorter at 24 and 48 elements of the
reference shaft).

## D-060: Patched oracle variants as labelled evidence

2026-11-11. A patched oracle variant is differential evidence when: (1)
the patch lives in `oracle/patches/<variant>/` (one defect per variant,
named after its UD id), is applied only to a build copy and is documented
in `docs/ORACLE_BUILD.md` hunk by hunk; (2) it preserves the numerics (API
adaptation, build fixes, restoring the behaviour of a released upstream
version, zero-initialising memory upstream reads uninitialised, each tied
to a registered UD), never adding or changing a formula, which an
independent reviewer confirms hunk by hunk; (3) upstream's own tests of
the feature pass on the variant; (4) fixtures from a variant record
`oracle_variant` and the patch's SHA-256, obey D-035 and D-050 and never
replace a golden of the unpatched oracle; (5) parity rows whose only
oracle evidence is a variant say so. Variant-only cases refuse to run
without the label and the patched files.

Reason: some packages do not compile or read uninitialised memory at the
reference commit (the embedded model; anisotropic damage; contact
accumulators, UD-001), so no plain oracle fixture can exist; a small
numerics-preserving patch makes them run (for the embedded model,
upstream's three tests then pass and its 2015 golden matrix is reproduced
byte for byte).

Consequence: such rows can be *Verified*. A variant is evidence for the
port, is not shipped and defines no new behaviour. Variant goldens are
listed in `docs/NUMERICAL_VALIDATION.md`.

## D-061: `Array::find`, periodic slave order, one event-handler list

2026-11-11. (1) `Array::find(&T) -> Result<Option<usize>>` returns the
index of the first match with upstream's comparison (`|a − v| ≤ ε` with
`ε = f64::EPSILON` for reals, so NaN and ±∞ never match; `==` otherwise)
and an error where upstream's iterator throws (a non-empty array with more
than one component); `find_upstream` returns upstream's raw value
(UD-042 corrects the returned index, B). (2) Periodic slaves are summed
per master in reverse insertion order, the iteration order of upstream's
`std::unordered_multimap` established black-box (UD-654, C):
`Mesh::periodic_slaves_upstream_order` gives that order and every port of
a loop over `getPeriodicSlaves` uses it. (3) Materials and user event
handlers of the solid model form one list dispatched newest first, as
upstream's `EventHandlerManager` (oracle probe: `c, C, b, B, a, A`).

Reason: (2) was established on 178 container patterns in four builds and
67 adversarial oracle cases (2,640 masters, 374 with 2 to 7 slaves) with no
mismatch, independent of bucket count and rehash history.

Consequence: periodic reductions are bitwise; fixtures
`fixtures/mesh_utils/periodic_order` pin them.

## D-062: Non-local averaging: summation route and two defects

2026-11-11, (1) amended by D-097. (1) Serial runs use
`SummationOrder::UpstreamHalfPairs` (upstream's pair order, bitwise to the
oracle) by default; `KeyOrder` is an opt-in, order-independent route
(measured difference at most 8.5e-16). (2) UD-558, category B: upstream
registers non-local variables in one map per manager, so two laws that
average different variables in one model average the same one (the oracle
shows an `Ehat` law that never damages at 8.6 × K0); by default variables
are keyed per neighbourhood and law, and a compat switch reproduces
upstream. (3) UD-557, category B: `damage_wf` converts a real that leaves
the integer range for damages near 1 (1 − D ≲ 3.6e-5), which is
platform-dependent upstream (x86_64: infinite volumes and NaN averages;
aarch64: saturation); by default the exponent is computed in `f64`, finite
over 0 ≤ D ≤ 1, and the compat mode reproduces the x86_64 oracle.

Reason: the defaults keep bitwise parity where upstream is correct and
correct it where it is not.

Consequence: `NonLocalOptions::upstream()` sets every manager-level switch
(UD-037, UD-557, UD-558, UD-561).

## D-063: The contact model is a part of a monolithic coupler

2026-11-11. `ContactMechanicsModel` owns no `ModelSolver` and no DOF
manager; it implements `SolverCallback` (residual = contact force on the
`displacement` family, `K` = contact stiffness) and exposes what the
couplers call (positions, `search`, assembly, before and after step,
`on_cohesive_elements_added`, `save_previous_state`,
`search_keeping_history`). The couplers keep upstream's call order
(explicit: positions, search, solid residual, contact forces; implicit:
search in the corrector, see D-109). In the corrected assembly forces are
accumulated (`+=`, UD-002) and a deformable master contributes with the
weight of `DeformableMasterWeighting` (`TwoPassHalf` = ½, default, or
`OnePassFull`) to force and stiffness alike, so `K` is the derivative of
the weighted residual; frictionless contact keeps `K` symmetric. Normal
and friction moduli are the derivatives of their forces (UD-368, UD-369).
The friction history is committed once per converged step by the coupler
(UD-374, B); upstream's per-iteration save is a compat switch.
`ContactUpstreamCompat` reproduces upstream's assembly.

Reason: upstream's contact model registers no DOFs; its residual and
stiffness are meaningful only through the couplers.

Consequence: standalone use (search, forces, dumps) remains possible.
Governs `crates/akantu-contact`, `crates/akantu-couplers`.

## D-064: `initSolver` and the current-position cache

2026-11-11. `init_solver` copies the reference coordinates into the
current-position cache only when it allocates it; a later call leaves the
cache and its release counter alone (UD-352, B; UD-632 points to it). The
compat switch `SmmUpstreamCompat::init_solver_resets_current_position`
reproduces upstream. The staleness of the position after direct writes to
the displacement is upstream API behaviour (a separate category-C entry;
18 oracle replays depend on it).

Reason: upstream copies the reference coordinates at every `initSolver`
without invalidating the cache, so after a solve, a read of the position
and a second `initNewSolver`, upstream computes on the undeformed mesh
(oracle probe: Δt 2.8832e-07 instead of 2.8970e-07). Of nine call
orderings measured against the oracle, this rule differs from upstream
only in the two the defect produces.

## D-065: Bitwise scope of the dense eigen-solvers

2026-11-11, corrected the same day. The bitwise claim of the dense
eigen-solver ports (parity row CORE-019) covers, for sorted results
(`sort = true`), `n ≤ 16`, or any `n ≤ 48` whose spectrum has no ties
(+0 and −0 count as a tie). Outside that scope the result is a correct
eigen-decomposition under the criteria of `docs/NUMERICAL_VALIDATION.md`,
not bitwise to Eigen; the row and the rustdoc of `dense.rs` say so.

Reason: every upstream `eig` call is on matrices of size at most 6. Above
48 Eigen switches to a blocked Householder accumulation, and above 16 the
tie order of the sort depends on libstdc++'s introsort (clean-room only,
D-030); 32 of 48 tied oracle cases at n ≥ 17 come out in another column
order.

Consequence: porting Eigen's blocked path and a clean-room tie model
remain possible extensions.

## D-066: One owner for the strain-energy split; UD-387; UD-058

2026-11-11. (1) The spectral and volumetric-deviatoric strain-energy splits
used by the solid `phasefield` material and the phase-field model live
only in `akantu_model::energy_split`; `akantu-phase-field` and the solid
material re-export it. (2) UD-387, category B: the corrected default
changes only the driving energy `φ`, so that `φ⁺` is the potential of the
existing `σ⁺`; a compat switch reproduces upstream. (3) The UD-058
specification of a corrected anisotropic damage law (scale the damage
increment by a closed-form factor, never clip the total tensor) is
accepted. It is implemented as `AnisotropicDamageModel::Corrected`, an
opt-in; the default stays `Unsupported` (UD-031), and the specification is
written in the module documentation of
`crates/akantu-solid/src/materials/anisotropic_damage.rs`.

Reason: (1) a one-ulp change of the split breaks both callers, so it must
be made once; the move changed no value. (2) the solid material never uses
`φ`.

Consequence: corrections of the split are made in one place.

## D-067: MPI finalisation at process exit

2026-11-11, with an addendum the same day. The facade's exit hook
(`release_at_exit`, registered by `runtime::initialize`) releases the
runtime's communicator and finalises MPI when a program did not call
`finalize`, under three rules. (1) No MPI call once MPI is finalised:
queued destructions are dropped when `MPI_Finalized` is true, and the hook
leaves an environment its host already finalised alone. (2) MPI is
finalised at exit only after a normal end: on glibc Linux the hook is
registered with `on_exit` and finalises for exit status 0 only; on every
target a panic since `initialize` (caught or not) disables it; on targets
without `on_exit` (macOS, musl) a non-zero `exit` without a panic still
finalises. (3) The hook never blocks on the port's own locks: it only
tries them and leaves MPI unfinalised with one note when one is held. A
bounded `MPI_Finalize` with a watchdog was not adopted, since finalisation
is collective and legitimately waits for slower ranks.

Reason: upstream finalises from a static destructor when it initialised
MPI itself (UD-499); finalising after a panic on one rank makes Open MPI 5
wait in a PMIx fence for peers that never come.

Consequence: a program that catches a panic and then ends normally calls
`runtime::finalize()` explicitly, or the launcher reports the rank (a
false failure, never a hang). These are deliberate differences from
upstream, which finalises after a caught exception and after `exit(n)`.
Layers with their own shutdown (the Python module with MPI) finalise there;
facade MPI test binaries call `finalize`. A shared object containing akantu
must not be unloaded after an MPI `initialize` (glibc skips the `on_exit`
handler of an unloaded object and `exit` crashes). "Never blocks" covers
the port's own locks only: a communicator finalize handler, a leaked
receive or peers that never finalise can still make `MPI_Finalize` wait.

## D-068: Pre-commit capability check of mesh mutations

2026-11-11. After the topology, hook, bounding-box and count stages and
before the commit, the mesh domain calls `MeshEventHandler::check_mesh_event`
once per consumer for every event of its part that will be dispatched, on
the staged final state, in registration order. The method has no default;
a forwarding handler forwards it. The check is local, changes no
observable state and must not depend on what the dispatch of earlier
events would change. Its outcome is agreed: a refusal rolls the mutation
back on every rank and leaves the domain usable; the refusing rank returns
the consumer's error, the others a recoverable `Error::Communication`. A
consumer may re-raise a typed fatal error from its own state (a poisoned
DOF manager), which poisons the domain before the commit.

Reason: a refusal reached only at dispatch cannot be rolled back and
poisons the domain.

Consequence: every consumer states its limits up front (for example the
DOF manager and a node addition followed by a renumbering, the IGFEM
handler and node removal). Callers must not branch on the error variant.

## D-069: Emergency abort through `PMIx_Abort`

2026-11-11. Where a rank cannot call `MPI_Abort` (a worker thread under
`FUNNELED` or `SINGLE`, the serialised lock not obtained within 2 s), the
runtime asks the launcher to abort the job through `PMIx_Abort`, waits at
most 2 s, then calls `_exit(71)`. `PMIx_Abort` is resolved with `dlsym`
once when MPI starts and called from a helper thread parked since then.
From `Communicator::abort_on_transport_fatal` on, the abort path does no
symbol lookup, no thread creation and no allocation, and its note is
static text plus integers written from a stack buffer. Without a PMIx
client library the fallback is the plain `_exit`.

Reason: PRRTE 3.0.8's `prterun` occasionally hangs in its teardown when
every rank exits non-zero at once without finalising; the `PMIx_Abort`
request takes the launcher's abort path instead (0 hangs in 5,000 runs,
against 9 in 2,000 before).

Consequence: the mitigation covers this fallback only; any other case in
which every rank exits non-zero at once (panics on all ranks) still meets
the defect (`docs/DEPENDENCIES.md`).

## D-070: Dumper bounds

2026-11-11. Every array a dump reads has `1 ≤ nb_component ≤ i32::MAX`,
checked on every block (empty blocks and meshes without nodes included)
before anything is written; an explicitly sized parallel context
(`ParallelContext::Explicit`) is refused above 2²⁰ ranks. A violation is
an ordinary error.

Reason: upstream would write unreadable files or truncate silently.

Consequence: valid input gives byte-identical output as before.

## D-071: Storage guards

2026-11-11, amends D-056. (1) `ArrayRef::map_slice` and `try_map_slice`
take a higher-ranked closure (`FnOnce(&Array<T>) -> &[T]`), as
`std::cell::Ref::map`; every guard-narrowing API follows this rule. (2)
`ArrayRef`, `ArrayMut` and `SliceRef` have an explicit `Drop`, so a guard's
borrow lasts as long as its run-time count and holding a guard across a
call that needs the owner mutably is a compile error. (3)
`Mesh::state_fingerprint` formats values, never export metadata;
`SharedArray`'s `Debug` stays metadata-only and no test compares states
through it. (4) The copying per-element accessor is named
`SharedElementTypeMapArray::element_to_vec`, so a change from plain to
shared storage can never turn a borrow into an allocation silently.

Reason: without (1) a narrowed slice could outlive its guard; without (2)
the error surfaced as a refused foreign callback at run time.

Consequence: compile-fail doc tests pin (1) and (2).

## D-073: The x86_64 oracle: platform variants, CPU class, complex division, Gmsh

2026-11-11. (1) A fixture that differs between platforms has a variant
keyed by its numerical environment (architecture, OpenBLAS core type,
library versions), stored in a `variants/` tree next to the case; only
differing cases get one (9 cases, 40 quantities), everything else is one
golden bitwise on both platforms, and no tolerance changes. (2) x86-64-v3
is the authoritative CPU class, with a host check in the oracle scripts
(the `Haswell` OpenBLAS pin needs it, and glibc selects other libm
variants without FMA and AVX2). (3) Complex division: the port models
both runtimes clean-room (`ComplexDivision::Fused`, Debian's aarch64
libgcc, and `Unfused`, its x86_64 libgcc); `Fused` is the default today,
and making `Unfused` the default on every host waits for 126 residual
cases near the 2^971 scaling threshold to be explained black-box. (4)
Build-time Gmsh meshes are not compared across platforms; frozen meshes
stay the inputs (D-009). Manifests record the Eigen cache sizes measured in
the binary, not those requested.

Reason: three full corpus regenerations on x86_64 were bit-identical (686
cases, 13,298 quantities), and every class of difference was confirmed by
an experiment that removes its cause.

Consequence: governs `fixtures/**/variants/`, `oracle/`,
`crates/akantu-linalg/src/dense/complex.rs`.

## D-074: The manual's availability table

2026-11-11. `docs/manual/availability.md` is the single source of what the
manual and `README.md` say the port contains (status, module, crate,
feature). The module table, the status lines of the model pages and the
README sentences are generated from it; every other sentence is checked
against it by `cargo test -p akantu --test manual`.

Reason: statements about availability drifted between documents.

Consequence: a change of a module's state, a feature or a crate updates
the table in the same commit and re-blesses the generated text
(`AKANTU_BLESS_MANUAL=1 cargo test -p akantu --test manual`, run alone,
then the plain test).

## D-076: Approved exceptions, PETSc licence, superseded rows

2026-11-12. Approved exceptions under D-028: TEST-211
(`test_interpolate_bernoulli_beam_2`: not built since 2017, removed API,
empty reference file), NL-009 (`NeighborhoodMaxCriterion` cannot be
instantiated upstream, UD-563), CI-007 (`akantu_diff.sh`, referenced
nowhere), CI-023 (`Jenkinsfile`, dead since upstream moved to GitLab CI),
CI-008 (build-time mesh generation with Gmsh; the port uses frozen
meshes, D-009). The 16 *Superseded* CORE and ITER rows stand (their scope
moved to child rows). PETSc licence: the `petsc` feature is off by
default, the facade has none, and no reduced PETSc build is made; a binary
built with `petsc` against Debian's PETSc is linked with GPL-2.0-or-later
libraries (SuiteSparse UMFPACK, SPQR and CHOLMOD modules, FFTW) and is
distributed under GPL-3.0-or-later as a whole, as for `cgal` (D-042).

Reason: the excepted items have no reachable behaviour or no counterpart
in a Rust project; a GPL-free PETSc would need a separate source build and
a separate oracle.

Consequence: the source and the default build stay LGPL-3.0-or-later.
Stated in `NOTICE.md`, `docs/DEPENDENCIES.md`, `README.md` and
`docs/manual/src/petsc.md`.

## D-077: Event-handler priorities and unregistration

2026-11-12. The solid model's `register_event_handler` takes a priority
with upstream's default and inserts before the handlers of equal priority
(upstream's `lower_bound`); a lower priority runs after every material.
`unregister_event_handler` removes the handler by the handle registration
returns. This extends D-061 (3) exactly as upstream
(`aka_event_handler_manager.hh:58-80`), with no port-only ordering rule.

Reason: parity row SMM-018 needs both operations.

Consequence: a test checks that a low-priority user handler runs after the
materials, against a committed oracle order fixture.

## D-078: Python bindings: what the refused calls became

2026-11-12. Accepted: Python-subclassable laws for cohesive, diffusion and
phase-field laws, and elastic laws that call their base computations;
writable zero-copy views of structural forces, element materials and
contact fields; contact-model and coupler dumpers; a port-only
`setUpstreamCompat(upstream=True)` with a getter on the contact model, the
phase-field model and the three couplers, documenting the covered UDs;
`reassignConstitutiveLaw` for the diffusion and phase-field handlers.
Rejected: a user-provided DOF manager in a model constructor (upstream's
own binding raises `RuntimeError` there; the port keeps the refusal with
upstream's error type). UD-830, UD-832 and UD-834 are category B (upstream
raises a deterministic error, the port corrects it); UD-831 and UD-837 are
C. PY-068 and PY-049 are exceptions (D-079).

Reason: each accepted item is upstream behaviour the bindings can provide
soundly under D-024.

## D-079: Two Python exceptions

2026-11-12. PY-068 (`register_constitutive_law_handler`: binds a template
upstream never instantiates) and PY-049 (upstream's `dumper_module` cannot
be imported from the oracle's module in any of three forms) are approved
exceptions.

Reason: neither has behaviour an upstream program can reach.

## D-080: Continuous integration

2026-11-12. The project's CI is GitHub Actions (workflow sources in
`ci/github/`, active copies in `.github/workflows/`), run through the
common entry point `ci/run-all.sh` on GitHub-hosted runners, with no
secrets and nothing published. Every
workflow stands on its own; no other service is needed.

Reason: the CI must run where the repository lives.

Consequence: a gate that exists only as a local script gets a workflow job
or a stated reason why not. Governs `ci/`.

## D-081: Upstream's Newton trace output is ported

2026-11-12. The `dblTrace` output of upstream's Newton-Raphson loop
(`non_linear_solver_newton_raphson.cc:101-108, 147-163`) is ported behind
the port's trace debug level, with an oracle probe of the text, as the
other debug texts.

Reason: a documented difference would leave row DOF-017 unverifiable; the
filtered-out macro costs nothing at run time.

## D-082: Comparison when upstream's distributed result varies through its solver

2026-11-12, extends D-039. Where a variation study shows that upstream
itself is run-dependent and the cause is a solver the port does not
reproduce (MUMPS's distributed factorisation in the distributed
phase-field runs: two distinct results in 12 runs at each of 2 and 4
ranks, all within 2.9e-16 of upstream's own one-rank result), the fixture
records every distinct upstream result with its frequency (at least 12
pinned runs per rank count), and the port is compared with each under an
envelope rule: a bound stated as a regression guard with its measured
margin (first use: 1e-14 relative to the maximum; measured 4.4e-16 on
damage, 1.45e-16 on displacement, 3.7e-16 on energies), written in
`docs/NUMERICAL_VALIDATION.md`. The one-rank case stays bitwise.

Reason: the port cannot equal any of upstream's varying results bit for
bit. A bound derived from an assumed condition number is not acceptable.

Consequence: everything that can be pinned (ghost filter order, arrival
order sums) is pinned first.

## D-083: Split-node contact on distributed meshes is ported

2026-11-12. The `_cf_nodal` and `_cf_incr` exchanges of
`NTNBaseContact::initParallel` are ported (MPI-051, MPI-052), with
reference data from an oracle probe that calls `initParallel()`; see D-097
for the pairing rule.

Reason: `initParallel()` is public and its scheme filter is collective
and consistent, so a user program gets the exchanges, although upstream's
own code and tests never call it (UD-904).

## D-085: Parallel IGFEM, GOLD-007 and CI-025 excepted; coverage tool and tutorials

2026-11-12. Parallel IGFEM is an exception: upstream's parallel IGFEM uses
removed APIs and the package does not compile at the reference commit
(UD-609); serial IGFEM stays in scope, and rows IGFEM-004, IGFEM-005,
TEST-192 (`mpirun -np 5`) and TEST-203 (`_parallel`) are judged on their
serial parts. GOLD-007 (a byte comparison of six 2017 outputs written
through removed classes) is an exception; TEST-242 is ported by intent.
CI-025 is a partial exception (Gitpod, the CDash drop site and gdb
pretty-printers are not ported; see D-116). The coverage tool
`cargo-llvm-cov` is approved for a CI job (CI-020). DOC-021 is a pointer to
upstream's external tutorial notebooks, as upstream has; no external
content is run. GOLD-003 was later excepted (D-087); a valgrind memcheck
job covers the FFI `unsafe` code.

## D-086: Handles lent to Python callbacks

2026-11-12. A handle that gives a Python callback access to the running
native call is admitted only when the lending Python frame (read through
the C API when the binding call starts to lend) is in the frame chain of
the running thread state, with a per-lend key retired when the lend ends.
Every other use raises `akantu.Exception`: another thread, another
greenlet, a generator or coroutine resumed outside the callback, a copied
`contextvars` context, a lend without a Python frame. This is a port-only
refusal where upstream lets the call through (UD-085, extended).

Threat model: sound against Python code that stays within the language's
object model (threads, generators, coroutines, copied contexts, trace and
profile hooks, finalizers, signal handlers, a replaced `sys._getframe`,
stack switchers that keep CPython's frame chain consistent). Not a defence
against code that rewrites interpreter internals (`ctypes`, raw-address
buffers, edits through `gc`, C extensions). The extension loads in a
legacy sub-interpreter under the PyO3 version in use, but `import akantu`
cannot reach one because NumPy and SciPy refuse first.

Reason: lent handles point into the lending call's stack; a stack switch
inside the callback reused that memory (the port crashed where upstream
ran).

Consequence: `getCurrentPosition()` inside callbacks follows UD-1001 (C).
Governs `crates/akantu-python/UNSAFE.md`.

## D-087: GOLD-003 is an approved exception

2026-11-12. The byte comparison with two legacy `.verified` files (PETSc
matrix-profile prints of an unregistered upstream test) is not ported.

Reason: the test's sources use removed APIs, and PETSc 3.22's `MatView`
text differs from the files (`Mat Object: 1 MPI process` and `(0, 2.)`
against `Matrix Object: 1 MPI processes` and `(0, 2)`).

Consequence: the content those files stood for is verified by SPM-008,
SPM-012 and TEST-228.

## D-088: Bounded TAO solves

2026-11-12. A bounded TAO solve whose bounds exclude a zero increment on
some owned row (`lower > 0` or `upper < 0`) is refused with a recoverable
error agreed over the ranks, naming UD-1022 (category A); a compat switch
reproduces upstream's result. No tolerance rule is added for the
phase-field TAO replays: they compare bitwise with UD-422's switch
`petsc_row_oriented_blocks` set.

Reason: upstream's TAO objective is right only when δ = 0 lies inside the
bounds: PETSc's GPCG evaluates it once at the median of (lower, 0, upper),
and upstream's gradient then solves the obstacle problem of a shifted
functional (the port predicts upstream's wrong result to 2.2e-16).
Upstream's only use, the phase-field model (0 ≤ δ ≤ 1 − d), always
contains 0. A corrected objective has no reference and no upstream user.

Consequence: the corrected default of UD-422 changes a bounded TAO solve
by up to 2.9e-11 on damage, recorded in UD-422's entry.

## D-089: CONT-019 is an approved exception

2026-11-12. Distributed search and resolution of the generic contact model
(CONT-019) is not ported; the contact model keeps its recoverable refusal
of a distributed mesh. This closes D-004's extension item.

Reason: upstream's generic contact model and its couplers are serial only;
there is no reference behaviour and no oracle.

## D-090: TEST-233 is superseded by the TEST rows

2026-11-12. TEST-233 (upstream's gtest main, its type-list helpers and the
test directory registrations) is *Superseded* and leaves the denominator
like the other superseded rows.

Reason: it has no behaviour of its own; the registration semantics
(`PARALLEL`, `PARALLEL_LEVEL`, the `.verified` driver) are reproduced per
test and recorded in each TEST row, and the rank counts follow D-033.

## D-091: The Python module on several MPI ranks

2026-11-12, scope fixed by D-101. An MPI-enabled build of the Python module
is ported (opt-in; the default wheel stays sequential like upstream's
published wheel).

Reason: upstream's CMake build with MPI produces an MPI-enabled module
(`has_mpi()` true; the example `plate-hole/plate.py` calls
`mesh.distribute()`).

## D-092: The PETSc DOF manager of a model on a serial mesh

2026-11-12. (1) `petsc` on a mesh whose communicator is MPI keeps that
communicator, as every other DOF manager type. (2) On a serial
communicator the PETSc DOF manager runs on the MPI world, and the job must
have exactly one process: MPI is initialised first if needed (at
`MPI_THREAD_FUNNELED`) or joined, the number of processes is a local query
(`akantu_comm::world_size`) made before any collective, and more than one
process is refused with a recoverable `Error::Unsupported`. (3) That world
is one process-wide duplicate of `MPI_COMM_WORLD`, shared by every such
manager and released by `akantu_solver::model_solver::release_petsc_world()`
(called by `akantu::runtime::finalize()`) or by the exit hook; dropping a
model never releases it, so PETSc is not finalised and re-initialised
between models. (4) An in-process world of several ranks is refused.

Reason: upstream builds every DOF manager on the mesh's communicator,
which in an MPI build is `MPI_COMM_WORLD` also for a mesh read on one
process; the port gives a serial mesh a serial communicator.

Consequence: serial upstream programs run unchanged on one process; a
multi-process job must distribute its mesh. A later request for
`Serialized` or `Multiple` in the same process fails once this rule has
started MPI at `FUNNELED`.

## D-093: Shared doubled nodes; references on inputs with undefined behaviour

2026-11-12. (1) UD-1043: upstream's facets-only insertion leaves a doubled
node shared between ranks without global id and owner, and asserts (the
oracle hangs at 2 and 4 ranks). The port refuses that case on every rank
before any state changes. A corrected numbering is not written: there is
no reference and no upstream program needs it. (2) When upstream's
behaviour depends on the input through undefined behaviour (UD-1060, the
cause of UD-655: a vector of `Array` relocating in
`MasterNodeInfoPerProc::synchronizePeriodicity`), a golden is recorded
only on inputs for which the probe computed that upstream is memory-safe;
the port recomputes that precondition and compares it; the relocation
pattern comes from observation with the oracle's compiler (D-030), with
valgrind evidence attached and re-observed if the compiler changes. No
value is ever taken from a run under a memory tool.

Consequence: on inputs where upstream is unsafe the port's behaviour is
defined and tested without an oracle.

## D-094: Coupler dumpers and Python insertion checks

2026-11-12. (1) UD-982: the port's models own their dumpers; a coupler's
model-level dump calls act on the dumper of the part that upstream's mesh
default belongs to, and part calls upstream sends to the mesh default
reach the coupler's series; the phase-field part of a
`CouplerSolidPhaseField` keeps its own series (documented difference); an
unknown field is refused, never omitted. (2) UD-984: the insertion flags
are a writable view only inside a Python `checkInsertion`; a view kept past
the check is detached; outside a check the accessor returns a read-only
copy. (3) UD-985: with Python cohesive laws the built-in checks run first,
then each Python law's `checkInsertion` with the model released, then the
insertion; a re-entrant `checkCohesiveStress()` or mesh-changing call from
inside a check is refused with a recoverable error, and the check refuses
to finish when the mesh changed between its phases. The mesh element group
`contact_surface` is upstream behaviour: the port writes it when a surface
selector is set.

## D-095: Composition of mesh events in one mutation

2026-11-12. Consumers receive every event of a mutation after the commit,
with the final mesh; an event names indices in the numbering current when
it was recorded. Compositions in which a later event renumbers what an
earlier one named are refused in the capability check (D-068), recoverably
and agreed on every rank. (1) Node events: a `NodesAdded` naming a node
beyond the final count; a `NodesRemoved` preceded by an addition (a pure
permutation included); with a group-supported family, a removal preceded
by another removal or whose group renumbering fails on the final mesh; on
a distributed or periodic mesh, a `NodesRemoved` followed by any other node
event (refused by the mesh domain itself). Record node removals before
node additions and compose the removals of one mutation into one
numbering; on a distributed mesh a removal is the only node event of its
mutation. (2) Element events, for every part with consumers: an event that
renumbers a block (a non-identity `ElementsRemoved` or `ElementsChanged`
numbering, including an `ElementsChanged` with empty lists that permutes)
is refused when an earlier event of the mutation names that block. Record
all erasures of a block in one `erase_elements` call, before additions.
(3) Upstream sequences that add then remove (IGFEM's interface move) are
ported as separate mutations, one per phase the rules cannot compose.

Reason: a consumer reading the final mesh at an index a later event
renumbered reads another entity.

Consequence: solver state named by local equations does not survive a
renumbering (the bounds of a `petsc_tao` solver are invalidated; the next
bounded solve is refused until they are set again). Every rule has a
recoverable-refusal test. Governs `crates/akantu-mesh/src/events.rs`,
`crates/akantu-solver/src/dof_manager/node_removal.rs`.

## D-096: Performance work keeps results bitwise

2026-11-12. Optimisations must keep the results bitwise equal to the
current ones; moving acceptance to tolerances for the sake of speed has
not been decided. Measuring is always allowed.

Reason: bitwise parity with the C++ library is the test target.

Consequence: an optimisation reruns the differential tests and keeps
every exact rule.

## D-097: Distributed non-local laws and split-node contact

2026-11-12, amends D-062 (1). (1) On a distributed mesh the non-local
manager uses upstream's pair order (`UpstreamHalfPairs`): bitwise to the
oracle at the oracle's partitions with the switches of UD-1101 and
UD-1102, and within the summation-order guard of the serial run with the
corrected defaults. `KeyOrder` is refused there (agreed
`Error::Unsupported` at `init_full`), because its key uses
`QuadPoint::global_element`, which a distributed mesh does not fill with
partition-independent ids. (2) Split-node contact pairs the nodes each rank
holds, as upstream, with no cross-rank pairing (UD-1105: C for the
per-rank pairing). On a mesh over several ranks
`NtnBaseContact::init_parallel` pairs the whole mesh with upstream's rule
and refuses, on every rank before anything is installed, a partition on
which a rank's pairing differs: a slave node owned by a rank that lacks a
pair of the whole mesh there, a rank that sees exactly one slave node and
a master node, or a rank forming a pair the whole mesh does not (A for
those cases); node pairs added after `init_parallel` through
`add_surface_pair` or `add_node_pairs` are refused the same way. Contact
nodes added after `init_parallel` through the low-level `add_split_node`
or the rigid-flat `add_surface` / `add_nodes` lie outside the filtered
synchronizer, as upstream, and are not refused. The check costs one
whole-mesh pairing per rank, a quadratic rule. (3) Distributed non-local
initialisation uses corrected defaults (UD-1100 to UD-1103: grid ghosts
receive their owner's law right after their synchronizer is built, the
cleanup erases only ghost elements the grid synchronizers added, every
weight-function internal is exchanged); the switches
`SmmUpstreamCompat::{non_local_ghost_laws_after_insertion,
non_local_cleanup_erases_distribution_ghosts}` and
`WeightExchange::Upstream` reproduce upstream, and where upstream aborts
(UD-1100) the compat path returns its assertion as an agreed error.

Consequence: supported split-node partitions keep both sides of every
interface node pair on the same ranks; the manual states the requirement.

## D-099: Cohesive tangent after failure; oracle probes with a defined behaviour; friction scale

2026-11-12, (3) amended 2026-11-13. (1) UD-1160, category B: the
linear-family cohesive tangent of a point that has failed has a zero
cohesive part (`tmax = σc·max(1 − δ_max/δc, 0)`) for the linear, bilinear,
fatigue and friction laws; a switch set by `upstream()` reproduces
upstream's negative secant stiffness (the compat replays of four implicit
fixtures reach that regime and stay bitwise). (2) An oracle probe may apply
a category-A defect's defined behaviour only when upstream aborts or has
undefined behaviour on that path, the probe applies exactly the registered
behaviour through upstream's public API without patching the library,
records it as a manifest parameter that the replay applies explicitly, and
the case is labelled a variant. (3) τ_max is the scale of rule R3-09 for
friction quantities while ‖Δt‖ ≲ 100·τ_max / k_f; beyond, the scale is
max(τ_max, k_f·‖Δt‖).

Reason: (1) upstream keeps a negative secant stiffness while the traction
is zero (`material_cohesive_linear_inline_impl.hh:228-231` has no clamp).
(3) the bound counts the cancellation in ‖Δt‖ − s with both operands and
the product rounded.

## D-100: The C interface is not built

2026-11-12. `akantu-capi` exports only `akantu_version()`. The C interface
planned by D-024 (opaque handles, borrowed views) is recorded as an
exception for now; C and C++ users go through the Rust crates or the
Python module.

Reason: upstream has no C API, so no parity row needs it.

Consequence: the availability table says "not planned for now".

## D-101: The Python module on several MPI ranks: the mesh level

2026-11-12, closes D-091. The MPI build of the Python module covers the
mesh level: an opt-in MPI build with `has_mpi()`, the import and exit
lifecycle, `Mesh.distribute()` and `isDistributed()` on the world
communicator, and an `mpirun` harness at 1, 2 and 4 ranks comparing
per-rank nodes, connectivities and groups with the oracle's module. The
model level on several ranks from Python is not provided.

Reason: no communicator hand-over is needed (mpi4py and the port share
`MPI_COMM_WORLD`); PY-053 is a mesh row; a model-level comparison has no
bitwise reference (upstream's Eigen solver asserts one rank, MUMPS varies
between runs), parallel dumpers never ran on real MPI upstream, and
upstream never runs its Python module on several ranks.

Consequence: the manual says what the MPI build covers.

## D-104: Overridable methods of Python law subclasses

2026-11-13. For `initMaterial`, `getPushWaveSpeed`, `getShearWaveSpeed`
and `hasStiffnessMatrixChanged` (and `hasMatrixChanged("K")` through it) of
a `Material` or `MaterialLinearElastic<d>D` subclass: when the calling
Python frame's code has the method's name, at least one positional
argument and the object as its first local, the base answers (the built-in
law's computation, or the `Material` base's for a plain runtime law);
otherwise the class's Python override answers if it has one. This holds
inside and outside the law's callbacks; the frame is read through the C
API with attribute access only. Access to lent state stays governed by
D-086.

Reason: upstream's trampolines use pybind11's `get_override`, which makes
exactly this distinction; without it a top-level
`hasStiffnessMatrixChanged()` answered the base and a top-level
`initMaterial()` super-call ran the override twice.

## D-105: The Neumann load of a doubled node follows its boundary elements

2026-11-13, amended by D-108. The solid mechanics model records, per
boundary element, the nodal forces its Neumann calls integrate; when
cohesive insertion doubles nodes, each recorded contribution moves to the
copy its element now uses (UD-1280, category B). Nodal forces of other
origin stay with the original copy. No communication is needed. The compat
switch `doubled_node_keeps_external_force` (set by
`CohesiveUpstreamCompat::upstream()`) reproduces upstream; the benchmark
driver has a separate setting to combine upstream's laws with the
corrected placement.

Reason: upstream gives a new copy of a loaded node zero external force and
leaves the accumulated load on the original copy; which copy is the
original follows the facet orientation, which on a distributed mesh
depends on the partition. The port reproduced this bit for bit, and it was
the deterministic cause of the rank-count dependence of the cohesive
benchmark.

Consequence: serial runs change where a crack reaches a loaded face. The
oracle replays set the switch and stay bitwise.

## D-106: Tests of timing properties and distributed scenarios

2026-11-13. (1) A test that protects a timing property asserts the
property itself: a count (no wait ended at a timeout:
`Communicator::local_timeouts()`), the end of the rank processes, or a
ratio against a calibration measured in the same job. Wall-time figures
are reported, not gated, and no limit is raised to make such a test pass;
a mutant that removes the property must still fail it. (2) In a test
scenario on a distributed mesh, a check whose result can differ between
ranks is agreed on every rank before the next collective call (shared
helper `par_common::agreed` / `agreed_value`); a scenario never returns
early on some ranks only, and expectations hold for any rank count or the
scenario states the rank counts it is valid for.

Reason: a scenario that returned early on two empty ranks left six others
waiting until the timeout at 8 ranks; wall-time bounds failed on stalled
machines without any defect.

## D-107: The agreement helper; sub-interpreters

2026-11-13. D-106 (2) is implemented with one shared helper. D-086's threat
model mentions sub-interpreters. Its decision on UD-1291 is superseded by
D-109.

## D-108: The Neumann store and loads written by other paths

2026-11-13, amends D-105. Before each recorded application and each move,
the store compares the external force with its own view of it. A node
whose value another path changed to zero is a reset: its records are
forgotten (the "zero, then apply again" pattern of upstream's examples). A
node whose value another path changed to anything else carries an added
force of other origin, which stays with the original copy while the
Neumann records still move. A move from a node another path has written is
counted (`moved_from_written_nodes`) and reported once at warning level.

Consequence: rescaling or replacing Neumann loads by writing the external
force array (a load ramp `f ← λ f`, hand-computed values) on a model that
may insert at loaded faces is not supported by the corrected default: zero
the array and apply the loads again, or set
`doubled_node_keeps_external_force`. Evidence for the cohesive benchmark
(h = 0.06, 2, 4 and 8 ranks, 2,500 steps): the serial run's elements are
inserted at the serial steps, and every energy differs from the serial
value by less than 1e-10 of the step's largest energy (measured at most
1.9e-11).
Bitwise equality across rank counts is not expected, nor agreement with
the C++ counts at more than one rank (upstream's distributed runs also
depart from the serial one through UD-890). `docs/BENCHMARKS.md` reports
the full-size runs against these criteria.

## D-109: UD-1291 is corrected by default

2026-11-13, supersedes D-107 on UD-1291. UD-1291 is category B: the static
and implicit solid-contact couplers search for contact before the first
residual test of a step, so a step that converges at iteration 0 no longer
keeps the previous contact state. The search is owed by the predictor and
runs at the first assembly of the step (the residual, or `K` for a solver
that assembles `K` first), because the integration scheme predicts the
positions after the coupler's own predictor callback. It keeps the
friction history (UD-374). A compat switch, set by the couplers'
`upstream()` and `all()` sets, reproduces upstream's order (search in the
corrector only), so the five static or implicit coupler fixtures and the
`contact_explicit_static` comparisons stay bitwise.

Reason: upstream searches for contact only in the corrector
(`coupler_solid_contact_tmpl.hh:67-79, 133-145`).

Consequence: convergence on a stale residual under the `solution`
criterion is a separate entry (UD-1293, D-113).

## D-111: A change of elements is followed through its lists

2026-11-13. An `ElementsChanged` is followed through its `old` and `new`
lists (upstream's `substituteElements`); the mesh does not apply the
event's `new_numbering`. A consumer that keeps per-element state (the
Neumann store, D-105) accepts a numbering that moves or removes one of its
elements only when the lists state the same move in the same block, and
never a substitution to an element with another number of nodes. Any
other such event is refused in the consumer's `check_mesh_event` with
`Error::Unsupported` before the commit, agreed on every rank.

Reason: neither the cohesive insertion nor any upstream caller emits such
an event (upstream's only emitter, IGFEM, names volume elements only), so
the rule refuses no run of upstream's or the port's examples, tests or
replays. The check also refuses elements whose rows are all zero, a case
no code path reaches.

## D-113: UD-1293, UD-1294 and UD-1295 keep upstream's behaviour

2026-11-13. UD-1293 (the `solution` criterion declares convergence on the
last increment and never tests the residual of the contact set the last
corrector's search found), UD-1294 (the `residual` criterion is relative
to the step's first residual, so a tiny first residual gives a target
below the rounding floor) and UD-1295 (with the linear solver the contact
couplers never compute a contact force, in both search orders and in both
codes) are category C, documented in the manual.

Reason: D-109 removes the large case of UD-1293; what remains is a property
of any increment-norm criterion, and changing it would touch the
non-linear solver every model shares. Correcting UD-1294 would change
iteration counts in every model and break the bitwise replays.

Consequence: a later correction would be a new category-B entry with its
own switch.

## D-114: CI rows: version, publication, documentation hosting

2026-11-13. CI-017: the port takes its version from `Cargo.toml` and
checks it against the release tag (upstream derives it from
`git describe`); approved exception. CI-022: nothing in the port publishes
to crates.io or PyPI (a dry run exists); approved exception, publication
can be added later. CI-024: the manual and the Rust and Python API
documentation are deployed to GitHub Pages by the CI from `main`.

## D-115: Register categories

2026-11-13. The category of a register entry says what the port does by
default today. UD-202 (the corrector's increment for displacement-type
solutions is the last solution) is C: the port reproduces upstream
bitwise, and a correction would make readers that accumulate the
increment over-count. UD-076 and UD-078 are C (test defect). UD-079 (the
plastic law returns the elastic tangent) is C: upstream does so on
purpose and the port's Newton iteration counts equal the oracle's. UD-023
(Python `getMaterial` returns a copy) is C as upstream's binding documents
it; the Rust API adds a live handle. UD-012 is B (A for order 0) and UD-890
B, both without a switch (the port solves the serial problem where
upstream's distributed insertion corrupts ghost boundary elements). UD-203
is C, a documented latent limitation. UD-790 (a map overload nothing uses)
is C and not ported. The label "C (documented difference)" is defined in
D-003 (UD-135, 235, 274, 342, 437, 440, 474, 683, 793, 1062, 1290) and
counts with C.

Consequence: no register entry is open; none of these settlements changed
the port's behaviour.

## D-116: Upstream's configure presets

2026-11-13. Every one of upstream's 13 CMake presets has a counterpart in
`ci/run-all.sh` or a stated reason (`ci/MAPPING.md`). `relwithdebinfo`
corresponds to tier `release` (the port has one optimised profile,
`[profile.release]` with `debug = 1`; debug assertions run in the
dev-profile tiers). The `devcontainer` presets have no counterpart (a
developer convenience, like Gitpod under D-085); what they switch on is
covered by the `examples`, `mumps`, `petsc` and `python` tiers and the
all-features check.

## D-117: UD-1296 keeps upstream's behaviour

2026-11-13. The model's boundary FE engine computes its Jacobians and
normals once and keeps them when cohesive insertion rewrites a boundary
row, so a Neumann load applied after the rewrite on a non-affine boundary
element with a multi-point rule uses the Jacobians of the old node order
(1.668e-3 of the largest nodal load on a distorted hexahedral test mesh).
Upstream does the same line for line. Category C, documented in the
register and the manual's cohesive chapter.

Reason: no test, example, benchmark or replay of either code is in the
affected class (one-point boundary elements are not affected) and the
replays' bitwise parity depends on the stale data. Recomputing the data on
the new rows reverses the normals of reflected rows; the right correction
(permute the stored per-point data of each rewritten row by matching
quadrature points by position, with a switch) would be new numerical code
in the FE engine.

Consequence: a mutation that renumbers nodes and then adds nodes is
refused while the Neumann store holds loads (no flow of the port or of
upstream does this; the refusal is recoverable).

## D-118: When the CI rows close

The parity rows CI-001 (every job green on GitHub), CI-014 (the macOS arm64
wheel job) and CI-024 (the hosting half of the documentation deployment)
stay *Implemented, unverified* until their jobs have run green on GitHub
runners. A replay of a job's commands on another machine does not close
them. In the public repository `fast.yml` and `docs.yml` run on every pull
request (the fast tier, the self-test, the MSRV check, rustdoc and the
manual), the heavier tiers run weekly and `expensive.yml` on manual
dispatch.

Reason: on 14 November 2026 the workflows ran once on GitHub, while the
repository was private. Of 29 jobs, 21 passed; six failed for three
reasons; the documentation deployment failed because GitHub Pages was off;
the macOS wheel job was cancelled at its time limit. None of the six
failures had shown in replays on machines that had built the project
before (`docs/TESTING.md`, "State at release").

The rows closed the same day: every workflow was dispatched once on `main`
of the public repository (commit `f0877f053`) and all 30 jobs passed,
including `packaging:wheel-macos` and `docs:deploy`. CI-001, CI-014 and
CI-024 are *Verified* on these runs.

## D-119: Fixes of the first runs shown off GitHub only

The causes of the first runs' failures were established and fixed off
GitHub, and the changed jobs passed when replayed. No job ran again on
GitHub before publication, so these fixes alone did not close CI-001,
CI-014 or CI-024 (D-118). The first runs on the public repository, the same
day, passed every job that had failed or been cancelled (`fast`, both
`exportall` jobs, `coverage`, `python`, `memcheck (mpi)`, `deploy` and
`wheel-macos`) with the fixes as they were replayed (Pages was enabled for
`deploy`).

## D-121: Oracle comparisons of BLAS-backed solvers by platform

2026-11-15. Results that go through OpenBLAS (MUMPS, PETSc's solvers) are
compared with an oracle record only in the numerical environment the
record was made in (the environment ids of D-073: architecture and
OpenBLAS core type). On x86_64 every comparison runs as before. On aarch64
Linux the nine MUMPS cases with an `aarch64-auto` variant (D-073) are
compared with that variant. The records made on x86_64 only, the
two-process MUMPS trace `solver_model/dbltrace_newton_raphson_np2` and the
PETSc oracle records under `fixtures/solver/petsc/oracle/`, are not
compared there. Those scenarios still run their other checks (the trace's
local nodes, iteration counts and file names; the TAO runs' convergence,
bounds, irreversibility and record integrity) and print one line
`ORACLE <scenario>: not compared: <reason>`. The MUMPS and PETSc tiers
print how many scenarios were compared and how many were not. The rule is
implemented once, in `akantu_validation::fixture::oracle_values_not_comparable`.
No tolerance changes, and checks that do not compare with a record run
unchanged on every platform.

Reason: OpenBLAS selects its kernels from the CPU, so BLAS-backed results
differ in their last bits between x86_64 (pinned to Haswell, D-050) and
aarch64, and x86_64 Linux is the authoritative platform (D-043). No
aarch64 PETSc oracle exists. The two-process trace case stores its
per-iteration matrices and solutions as inputs, which a variant shares
with the top level (D-073), so it cannot have an aarch64 variant. An
aarch64 oracle run of it (three identical runs) equals the port's aarch64
run bit for bit on all 28 displacements; that run is not committed.

Consequence: on aarch64 Linux one MUMPS scenario and five PETSc scenarios
report that they did not compare oracle values; a failing comparison on
x86_64 still fails.

## D-122: Performance work keeps the default bitwise; a fast mode may differ

Decision (project owner, 2026-11-16), for the performance work that follows
the port:

- The default build with its default run-time settings keeps every result
  bit. An optimisation of the default path must compute each result with the
  same operations in the same order, and every bitwise fixture must pass
  unchanged.
- A change of rounding is allowed only behind an explicit switch that is off
  by default (the fast mode): fused multiply-add, a sum taken in another
  order, a vectorised reduction, another ordering in a solver, or a threaded
  sum whose order depends on the thread count. Each such path is validated
  against the default path within a written tolerance
  (`docs/TOLERANCE_LEDGER.md`); runs that amplify rounding differences, such
  as fragmentation, are compared on energies and counts.
- Allowed kinds of change: threads inside a rank; hand-written SIMD with
  run-time dispatch (`unsafe`, every block reviewed); another default
  algorithm where the results stay bitwise, and under the fast mode where
  they do not; changes of the public Rust and Python API, recorded in the
  changelog.
- Every measurement compares the port before and after on the same machine
  in the same session, and the headline cases also with the C++ library
  (`docs/BENCHMARKS.md`, "Method").

Reason: the port's evidence is the bitwise comparison with the C++ library,
so the default path must keep it. Some of the remaining speed needs
operations that round differently, and a user who accepts differences in the
last digits should be able to have it.

Consequence: the rule "no fast-math of any kind" of `AGENTS.md` holds for
the default path. Code of the fast mode is marked as such and has its own
tests against the default path.

## D-123: Execution policy: precision mode, threads, SIMD level

2026-11-16. Extends D-017 and implements the switch of D-122.

- **One policy per model.** `akantu_core::exec::ExecPolicy` holds the
  precision mode (`Bitwise`, the default, or `Fast`), the thread count
  (default 1) and the SIMD level (detected once per process). A model
  copies `ExecPolicy::process_default()` when it is created and can change
  it between steps (`SolidMechanicsModel::set_exec_policy`); it hands an
  `Exec` (the policy with its thread pool) to its FE engines, its DOF
  manager and, through `ModelServices::exec`, to its laws. The process
  default comes from `AKANTU_NUM_THREADS`, `AKANTU_PRECISION`,
  `AKANTU_SIMD` (and `AKANTU_EXEC_GRAIN`), read on first use, or from
  `ExecPolicy::set_process_default` (Python: `akantu.setNumThreads`,
  `setPrecision`, `setSimdLevel`) before the models are created. An
  invalid variable makes model creation fail. A DOF manager built
  without a model (`DofManager::new`, `with_backend`) takes the process
  default policy; when the `AKANTU_*` variables are invalid it falls back
  to one thread (bitwise, detected SIMD level) instead of failing, and a
  model that creates its DOF manager replaces that executor by its own.
  Tests set the policy on their own model, never the process default, so
  tests running in parallel threads of one process do not race.
- **A kernel reads the policy once per batch**: the engine's or the
  model's `Exec` is passed to the batch, which picks its variant (fast
  mode, SIMD level) and splits its work once; nothing is read per
  element.
- **The invariant, tested.** In `Bitwise` every result is identical for
  every thread count and every SIMD level. Threads are therefore part of
  the default path wherever every sum keeps its serial order:
  independent outputs (per element, per quadrature point, per equation)
  are split into contiguous parts (`Exec::for_each`, any split gives the
  same bits); an element-to-node assembly uses a node-ordered table
  (`NodeScatter`: per node, its contributions `(element, local node)` in
  the serial order; threads own disjoint node ranges), cached per block
  and checked against the connectivity before every use (topology
  revision and filter unchanged, else row-by-row comparison, else
  rebuilt), so that cohesive insertion and every other topology change
  rebuild it; a global reduction computes its terms in parallel and sums
  them in the serial order (`Exec::sum_in_order`), in another order only
  under `Fast`. `AKANTU_EXEC_GRAIN=finest` splits every batch as finely as
  possible, for running test suites with many small parts.
- **Pool: a small pool on `std::thread`** (`exec/spin.rs`), one per
  thread count per process, shared by all models: the caller takes part in
  the region; participant `i` takes the parts of the `i`-th contiguous
  block first and then the leftovers of the others, so a thread keeps its
  elements from one region to the next; a worker joins a region only while
  it is open, and the caller, once no part is left, closes it and waits
  only for the workers inside (a worker that wakes late skips it); idle
  workers spin for 5 ms and then park (a region that has to wake parked
  workers cost its caller 18–31 µs of futex wake-up on the screening
  machine; the serial stretches of an explicit step last about a
  millisecond). Chosen over `rayon` by the measured cost of one
  region (`scripts/perf/exec/pool-overhead`; empty regions and regions of
  10–100 µs of work, overhead above the ideal `work / threads`, 2/4/8
  threads): on an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores) 0.18–3.7 µs
  at 2 and 4 threads (commit `3b6d0257`) against 4.9–13.8 µs for
  `rayon::ThreadPool::broadcast`; on an 8-vCPU EPYC Milan VM, all three
  thread counts at commit `427eb987`, 0.18–11.3 µs against 7.7–15.2 µs.
  An explicit step has about a dozen regions, so on a 20 k-element mesh
  `rayon` would add tens of percent. The
  price is `unsafe` code (a borrowed closure handed to the workers for one
  region; 7 sites in `crates/akantu-core/UNSAFE.md`) and workers that spin
  briefly after each region. A region started while the pool runs
  another (another thread, or a nested region) runs serially on its
  caller, which gives the same results: two models with the same thread
  count stepped from two OS threads share one pool, and whichever starts
  a region second runs it serially. Pools are never shut down: the
  process keeps one parked pool per thread count it ever used (a failed
  pool creation joins the workers it started). After `fork` the child
  starts new pools for the executors it creates (the table is keyed by
  process id); an executor created before the fork runs its regions
  serially in the child (no worker exists there), and a fork while
  another thread holds the pool table's lock or a worker holds the
  parking lock deadlocks the child at its next pool creation or region:
  Python `multiprocessing` should use `spawn` or `forkserver` with more
  than one thread. Each MPI rank has its own pool.
- **Wait policy.** Idle workers spin for the policy's spin time
  (`AKANTU_EXEC_SPIN_US`, `ExecPolicy::with_spin_us`; default 5 ms, or 0
  when the threads outnumber `std::thread::available_parallelism`, with
  a one-time warning in the debug output) and then park; 0 parks them as
  soon as a region ends. The caller's wait for the workers inside a
  region yields the core after 2048 polls. Spinning costs cores: with
  more than one thread, each MPI rank keeps up to `threads − 1` cores busy
  for the spin time after every region, also while it waits in MPI or
  writes output, and on SMT the spinning workers share their cores'
  pipelines with the siblings. Recommendation: `ranks × threads ≤ cores`
  (physical cores), otherwise `AKANTU_EXEC_SPIN_US=0`. Measured on an
  8-core cloud VM (`docs/benchmarks/perf-campaign/exec/README.md`,
  "Wait policy"; 4,000 back-to-back regions of about 5 µs of work): at 4
  threads on 4 cores the median region took 3.7 µs before these changes
  and 3.9 µs after (p99 4.5 and 4.4 µs); with 16 threads on 8 cores the
  old pool (5 ms of spinning) had a median of 8.6 µs and stalls of 6.0 ms
  (a worker descheduled inside a region), the default now (spin 0) a
  median of 20.1 µs, a p99 of 81 µs and a maximum of 0.2 ms; with 5 ms of
  spinning forced the stalls of 6 ms come back.
  The library does not pin threads: a rank's threads inherit the rank's
  CPU mask, so with `mpirun --bind-to core` all threads of a rank share
  one core; run `ranks × threads ≤ cores` with a binding that gives each
  rank `threads` cores (Open MPI `--map-by slot:PE=<threads>`, or
  `--bind-to none`). The default (1 thread) starts no pool and keeps every
  MPI run as before.
- **SIMD.** `SimdLevel` (`Scalar`, `Avx2` = AVX2 + FMA3, `Avx512`,
  `Neon`) is detected once; `SimdToken` proves the CPU supports a level
  and is the only way to call a kernel's `#[target_feature]` version
  (one dispatch point per batch, every `unsafe` call listed in the
  crate's `UNSAFE.md`). In `Bitwise` a vector kernel performs per lane
  the scalar sequence (no FMA, no reassociation); its property test runs
  every supported level against the scalar bits. `exec::simd::scaled_add`
  is the example. `AKANTU_SIMD=scalar` selects the portable code.
- **Fast mode.** Code of the fast mode is marked `FAST MODE` and runs only
  when `precision() == Fast`. Its test runs the case in both modes and
  calls `akantu_validation::fast_mode::check_fast_mode`, which compares
  with the bitwise run under the rule of
  `fixtures/fast_mode/<area>/tolerances.json` (listed in
  `docs/TOLERANCE_LEDGER.md`). Demonstrator: the gradient kernel with a
  chain of fused multiply-adds (`exec::with_fma` runs a fast-mode task with
  the FMA instructions of the token's level).
- **Laws** opt in to threads by `Material::independent_points()` (default
  `false`): the law's per-point loop then uses the model's executor
  (`material::points_exec`). The elastic law declares it. Laws with state
  shared between points, non-local terms or reductions keep the default;
  `docs/ARCHITECTURE.md`, "Execution policy", lists them.

Reason: several tasks will add threaded loops, SIMD kernels and fast-mode
kernels in many crates; one mechanism, read once per batch and tested by
one invariant, keeps the default path bitwise (D-122) while the threads
are on by choice.

Consequence: results do not depend on `AKANTU_NUM_THREADS` or
`AKANTU_SIMD` in the default mode (tests: `akantu-solid/tests/exec_threads.rs`,
the explicit-step digests at 1, 2, 4 and 8 threads); the core crate
denies `unsafe` instead of forbidding it, with its sites (the pool and the
SIMD kernels) listed in its `UNSAFE.md`; no new dependency.

## D-124: Formulas and figures of the manual

2026-11-17. The manual's formulas are written in LaTeX (`$...$` and
`$$...$$`) and rendered when the book is built, by the mdBook preprocessor
mdbook-katex 0.9.4 (KaTeX 0.16.4) next to mdBook 0.4.40. The KaTeX
stylesheet and fonts that the rendered formulas need are stored in
`docs/manual/src/katex/` (MIT), so the built book loads nothing from
another site. The `manual` and `site` jobs of the CI install both tools at
these versions and build through `ci/github/manual-build.sh`, which fails
when a formula does not render. An expression whose point is the order of
its floating-point operations stays a code block.

The manual has figures of two kinds. Figures of the upstream manual and
set-up drawings of upstream's examples are copied unchanged to
`docs/manual/src/figures/upstream/`, with a notice. Where the port's own
result shows the same thing as an upstream result image, the manual shows
the port's. The figures of
`docs/manual/src/figures/` outside that directory are made for akantu-rs:
drawings, and images of the port's results rendered by
`scripts/manual/render_figures.py`. A caption says where a figure comes
from, and for an image of a result, which library computed it.

Reason: the project's director asked for typeset formulas and for figures,
as in the upstream documentation, and chose both sources of figures. The
upstream figures carry no licence statement of their own; they are part of
the Akantu repository, which is LGPL-3.0-or-later like the port. This
replaces the earlier choice of copying no upstream figure
(`docs/DEPENDENCIES.md`, `docs/manual/NOTICE`).

The element appendix's formulas are generated, and its test
(`shape_formulas_match_the_kernels`) compares each displayed derivative
with the kernels within an absolute 1e-13, the value it already used for
the shape functions: the formula and the kernel evaluate the same
polynomial in different operation orders. This is a check of the
documentation, not a fixture tolerance; no `tolerances.json` changes. The
project's director agreed to it, to the added build tool and to the copied
figures on 17 November 2026.

Consequence: building the manual needs mdbook-katex besides mdBook. The
rendering script is a maintenance tool (pyvista in a container); neither
the build nor the CI depends on it.

Numbering: this record was written as D-122 on the main branch while the
performance work had already used D-122 and D-123 on its branch; it was
renumbered when the two were merged.

## D-125: An opt-in nested-dissection ordering for the default sparse solver

2026-11-17, the project owner's choice ("yes add it as opt in"). The Eigen
SparseLU translation (`eigen` solver) has a second column ordering next to
Eigen's COLAMD: a nested dissection of the graph of `A + Aᵀ`, written in
`akantu_linalg::eigen_sparse_lu::nested_dissection` (no partitioning
library). It is selected per solver (`SparseOrdering`,
`SparseLu::set_ordering`, the solver option `ordering`,
`ModelSolverOptions::sparse_solver_ordering`, the `time_step_solver`
parameter `sparse_solver_ordering`, the Python keyword of
`ModelSolverOptions`) or for a process (`AKANTU_SPARSE_ORDERING`). Values:
`colamd` (the default), `nested_dissection`, `auto`.

- COLAMD stays the default and gives the oracle's factors bit for bit.
  With neither the option nor `AKANTU_SPARSE_ORDERING` set, the ordering
  is COLAMD in the bitwise precision. A factorisation analyses the pattern
  again when the effective ordering changed since the analysis (a switch
  of the precision mode), and the solver stack factorises again when the
  factors no longer match the mode (`factorization_stale`), so a bitwise
  solve after a fast one is COLAMD's, bit for bit.
- Another ordering eliminates the unknowns in another order. The factors,
  the pivots and the last digits of the solutions differ from the default;
  by D-122 this is a fast-mode change. Rule
  `fast_mode/linalg/sparse_lu_ordering` of
  `fixtures/fast_mode/linalg/tolerances.json` (`docs/NUMERICAL_VALIDATION.md`,
  "SparseLU orderings").
- `auto` chooses per analysis, from the values at that moment: COLAMD
  below 2,000 unknowns or 100,000 stored entries, and whenever a column's
  diagonal entry is zero or smaller in magnitude than another entry of its
  column (the threshold partial pivoting would then pivot off the
  diagonal: COLAMD bounds that fill through the structure of `AᵀA`, the
  dissection of `A + Aᵀ` does not). Otherwise it predicts COLAMD's work
  (`Σ c_j²` over the column counts of the symmetric factor, computed in
  almost linear time) and computes the dissection only when that work
  reaches 5·10⁷ and 3,000 times the edges of the supervariable graph (the
  dissection's own cost grows with that graph); it takes nested dissection
  when its predicted work is at most 0.8 of COLAMD's and its predicted
  size not larger. After the first factorisation with nested dissection,
  nnz(L + U) above twice COLAMD's predicted size or off-diagonal pivots
  above 1 % of the order send the pattern back to COLAMD, which it keeps
  (an indefinite matrix can pass the diagonal test). With neither the
  option nor `AKANTU_SPARSE_ORDERING` set, the ordering is `auto` in the
  fast precision (D-123).
- The ordering is deterministic: the same pattern gives the same
  permutation on every machine and for every thread count. Ties go to the
  smaller index, no hash-map order and no random number are involved; it
  runs on one thread.
- The factorisation stays the left-looking supernodal code. Its panel
  updates use the executor's threads as before, with the same bits at
  every thread count for a given ordering. Independent subtrees of the
  elimination tree are not factorised concurrently (the reason is in the
  benchmark README of `docs/benchmarks/perf-campaign/sparselu-nd/`).
- A fixture writer refuses to run when `AKANTU_SPARSE_ORDERING` names
  another ordering than `colamd`; `non_default_ordering_used` tells a
  program whether one of its analyses used another ordering.

Reason: on the suite's 3-D and 2-D problems the dissection needs 0.46 of
COLAMD's factorisation time on the 3-D elasticity case and 0.47–0.61 of the
solve time on the heat, phase-field and beam cases (one thread, size m;
the README of the benchmark directory has the tables). A user who accepts
differences in the last digits gets that through one option. The
dissection loses where the threshold pivoting chooses many off-diagonal
pivots (the DKT plate of the suite: 3.1 times COLAMD's time); `auto`
detects that case and keeps COLAMD.

Consequence: where `auto` keeps COLAMD its checks cost at most 5 % of an
analysis and factorisation (none below 100,000 entries); where it computes
the dissection it pays the dissection and two predictions on top of
COLAMD, and chooses it only where the factorisation gains more. A pattern
that changes at every solve (contact, cohesive insertion) pays that at
every solve; `auto` can choose differently from one step to the next. The
fallback costs one factorisation with nested dissection, once per
pattern. A Scotch ordering was not added: the
crate's dissection already reaches the fill of a geometric dissection on
the grids measured, and Scotch would be a second, optional code path.

## D-126: Collective checks at commit points (amends D-018)

2026-11-18, the campaign lead's decision on a multi-host diagnosis
(`docs/benchmarks/perf-campaign/net/`). Principle of the new default: an
agreement is spent only where the ranks must decide something together or
are about to depend on each other; a purely local failure may stay local
until then.

Why: over a network every collective costs at least a round trip to the
farthest host and, with Open MPI's linear non-blocking all-gather that
carried every checked header, P − P/H network messages per rank. The
cohesive benchmark's step made 51.6 collectives per step and rank (41.8 of
them agreements: one after every phase), the C++ reference 8.4; across
four hosts the agreements alone cost the port half of its loop. The
results do not depend on them: the same runs without them gave
bitwise-identical energies, insertions and summaries.

Modes (`akantu_comm::phase::CollectiveChecks`, a property of a
communicator family; `Communicator::set_collective_checks`, the process
default `set_default_collective_checks` or `AKANTU_COLLECTIVE_CHECKS` =
`every-phase` | `commit-points`; it must be the same on every rank):

- `every-phase`: D-018 as written, one agreement after every phase
  (every `Phases::defer` agrees at once). The safety net.
- `commit-points` (default): the mechanisms below.

Mechanisms of `commit-points`:

1. **Deferred outcomes.** In a scope whose owner settles it on every path
   (`Phases::settling`: the DOF manager's operations, the mesh transaction,
   the standalone synchronizer exchanges, the cohesive insertion order),
   `Phases::defer` / `deferred` record a purely local phase's outcome
   (worst severity, folded phase ids and digests) without communicating;
   in any other scope (`Phases::new`) every outcome is agreed at once, as
   under every-phase checks; the failing rank gets its own error at once
   and skips the rest of the operation. Only on a communicator of more
   than one rank: on a one-rank communicator (a split, an MPI singleton)
   every outcome is agreed at once (no collective there could carry it).
   A settling scope dropped while it holds deferred outcomes (an owner
   that broke its promise, or a scope dropped after a caught panic)
   reports a failure in one last agreement, which its peers, waiting in an
   agreement of the scope, meet: they fail at once, recoverably, and the
   communicator stays usable; while the thread unwinds, or if that
   agreement cannot complete, the communicator is marked broken instead
   (the abort path). Deferred: the DOF manager's
   steps (solver callbacks) and joined operations, the post-processing
   (unpacking) of `Phases::run` and `Synchronizer::wait_in`, the mesh
   transaction's stages before its commit (preflight, topology with its
   schedule digest, hook prepare / apply / schedule) and its dispatch
   outcomes after it (with fatal severity).
2. **Agreement before communication.** Every agreement of a scope folds
   what is deferred; `Phases::collective`, `Phases::run` (its prepare
   agreement) and `Phases::settle` agree it first when anything is
   deferred. A peer that failed locally has left the operation through its
   error path, whose next communication is its scope's settle; the other
   ranks meet it at their next agreement, never in a point-to-point
   exchange. While a scope of a thread holds deferred outcomes (live or
   parked by that thread), every scope and raw collective of that thread
   on **another** communicator is refused locally, recoverably
   (`ScopeState::RefusedDeferred`, the raw operation's error): a peer that
   failed is in an agreement of the deferring scope's communicator and
   would never join the other one. The refusal is the code's local error;
   it is agreed when the deferring scope settles.
3. **The status in the header.** Every checked collective's header
   carries an optional protocol status (`HeaderStatus`: severity, phase id,
   digest, and the rank's collective-check mode), and every agreement
   carries its own. Ranks in different modes fail at their first header
   with one agreed-fatal error naming the setting; an invalid
   `AKANTU_COLLECTIVE_CHECKS` or `AKANTU_COLLECTIVE_HEADER` is an error of
   the world's creation (`mpi_world`, `from_mpi`, `from_raw_mpi`). A failure reported by any
   rank, or ranks at different phases, fails the collective on every rank
   before any payload moves, whatever the descriptors say (so a failed
   rank's settle agreement meets its peers' next reduction).
   `Phases::run_collective` therefore needs no agreement of its own: the
   deferred outcomes and the prepared payload's status ride in the header of
   its reduction; a rank whose preparation failed joins with a header-only
   collective. The operations that end with such a reduction close with its
   verdict (`DofManager::reduction`: the energies, the stable time step).
4. **Header exchange.** `MPI_Iallgather` below 64 ranks, `MPI_Igather` on
   rank 0 then `MPI_Ibcast` from 64 ranks on (2 (P − 1) messages instead of
   P (P − 1); 254 against 16,147 per agreement at 128 ranks on one host);
   `AKANTU_COLLECTIVE_HEADER` = `auto` | `allgather` | `gather-broadcast`
   (the same on every rank). Both modes, and both header forms, deliver
   every rank's header to every rank.

Commit points (an immediate agreement in both modes; `Phases::commit`,
`DofManager::collective_committed`):

- the mesh transaction's capability check, before the commit: nothing
  irreversible may happen while a peer can still roll back;
- the end of the transaction's dispatch: the domain is poisoned on every
  rank (agreed fatal) if any consumer failed after the commit;
- the close of a DOF manager's outermost operation (`DofManager::collective`):
  its result is common knowledge, callers branch on it;
- the native drivers' trampolines (`form_function`, `form_jacobian`,
  `finish`): their status codes steer the library on every rank;
- every agreement inside a native library's region (PETSc, MUMPS): the
  library communicates outside the protocol, so nothing is deferred there;
- every other existing `agree` / `agreed` (unchanged call sites: they stay
  immediate and also agree what is deferred);
- **before every persistent change** of a DOF manager's operation
  (`DofManager::commit_point`, which communicates only when something is
  deferred, never under every-phase checks). The rule: when an operation
  returns an error, every rank's persistent state — the DOF arrays (values,
  previous values, increments), the lumped mass, the laws' internal
  variables, the mesh — is the state the every-phase mode leaves on that
  rank for the same failure; the ranks that run further under commit
  points change transient data only (residual, forces, solution, matrices'
  content, buffers). Placed in the time-step adapter
  (`crates/akantu-solver/src/time_step.rs`), so that every solver goes
  through them: at the start of `before_solve_step` (the laws save their
  previous state, the schemes store the step's start), after the model's
  `predictor` hook and before the schemes' predictor, after the mass
  assembly and before the model's residual in `assemble_residual` and
  `assemble_residual_part` (internal variables), at the start of the
  corrector and of `after_solve_step`, and at the start of
  `assemble_matrix` (the schemes' Jacobian caches and release tracking);
  in the solid model's lumped mass after its density exchange and before
  the assembly into `M` (the cached mass and its flag); in the cohesive
  model's residual after the traction exchange, before the bulk stresses;
  in the Newton–Raphson solver before it clears its
  `force_linear_recompute` flag. A failure in the same step as a write is
  no concern: every-phase checks agree after the step, so the writes of
  that step happen in both modes. Each of the explicit step's commit points
  is necessary (removing any one makes the state test below fail). Not in
  the rule: the scheme's store for a restore, the solid model's
  `displacement_release` counter and the DOF manager's blocked-DOF index
  (recomputed at every step), and the Newton–Raphson Jacobian's content
  (reassembled whenever the solver's flag says so).

Fallback, agreement before every point-to-point exchange (the decision
asked for the sender's status in each message): every synchronizer
exchange keeps its agreement before it under both modes (it folds what is
deferred). A status in the message reaches only a rank that is at the
exchange. A rank that failed earlier has left the operation through its
error (`?`) and is then in an agreement — its scope's settle, or any
agreement of an enclosing operation — while its peers would wait in the
exchange for its message: a deadlock until the deadline. Under D-018 every
communication of a rank starts with a collective, so two ranks always meet
at one; only an agreement before the exchange keeps that invariant. A
prototype (statuses in the messages after an agreement with nothing
deferred, and as the first step of a scope) hung in four tests of the fast
tier (`collective_failures`: the Newton and distributed mat-vec callback
failures; `non_local_parallel`), a failing rank being in an agreement of
another scope. Letting every caller continue in a poisoned mode up to the
next exchange would touch every physics crate's step sequence.

Failure semantics:

- When a peer's purely local failure becomes visible: `every-phase` at the
  agreement that ends the failed phase; `commit-points` at the scope's next
  agreement (before its next communication, at a commit point, or when it
  settles). In between, the other ranks run the scope's further
  local phases (before a commit, all of it is rolled back; after a commit,
  the domain is poisoned anyway).
- Guaranteed in both: every rank ends a failed operation with an error;
  no rank waits for a rank that left (the polled waits and their deadlines
  stay the last line of defence: a silent peer fails the waiting agreement
  at the deadline); no rank commits what another rank rolled back; the
  agreed severity class is the same on every rank (recoverable or agreed
  fatal; transport failures as before). The failing rank keeps its own
  error; a peer's message names the remote failure ("failed on another
  rank") at the phase where it learned of it, which may be a later
  phase than under `every-phase` (the documented substitute).
- The agreed failure can be more severe than under `every-phase`: two
  ranks failing different deferred phases (one recoverable, a later one
  fatal) are agreed together, with the worst severity, where every-phase
  checks would have stopped at the first (recoverable) failure.
- Under `every-phase` the mesh's bounding-box and element-count
  reductions keep the D-018 form (a collective with its own agreements,
  no checked header), so that mode makes no more agreements than before.

Consequence: 19.4 collectives per step and rank for the benchmark's step
at 8 ranks (h = 0.06, 977 insertion steps in 2,000; was 51.6; the
reference 8.4). Those left, per step: the solve's operation close and its
two exchange agreements (ghost stresses, cohesive tractions); the four
energy reductions (the reference's four `MPI_Allreduce`); the facet-stress
exchange's agreement and its scope's settle, the insertion decision's
reduction and the driver's global count; and on insertion steps the
agreements before the insertion-order exchanges and of its uniqueness
check, the transaction's commit point and dispatch end, the consumers'
exchange agreements, and the global-id, count and bounding-box reductions
(the reference's scan and reductions of `updateGlobalIDs` / `updateData`).
Results are unchanged in both modes.
Tests: `crates/akantu-cohesive/tests/collective_checks_failures.rs` (one
rank failing the solve step, the insertion check, an energy, a mesh
transaction before and after its commit, at 2 and 4 ranks, both modes);
`crates/akantu-cohesive/tests/collective_checks_state.rs` (the persistent
state after a failure injected in each phase of the explicit solve step in
turn, on the first and the last rank, at 2 and 4 ranks, at an insertion
step and two steps later: the same bits in both modes on every rank); the
same two in the real-MPI binary `mpi_cohesive` at 2 and 4 ranks;
`crates/akantu-comm/tests/commit_point_failures.rs` (a scope on another
communicator while outcomes are deferred, an unsettled settling scope,
mixed modes, two failures of different severity, a failed preparation
against a payload collective) and the scenario `commit_points_scenarios`
of `mpi_acceptance` (the same under both header forms, the
gather-broadcast one forced, and checked reductions on a one-rank
communicator from a split); `crates/akantu-solver/tests/commit_point_steps.rs`
(where the peers of a failed step stop); the protocol tests of
`akantu-comm` (deferred post-processing, header verdicts, a silent peer
bounded by the deadline), the existing fault-injection tests of the
solver, mesh and solid crates in both modes.
Governs `crates/akantu-comm/src/{phase,communicator,mpi_backend,synchronizer}.rs`,
`crates/akantu-solver/src/{dof_manager,time_step,non_linear_newton_raphson}.rs`,
`crates/akantu-mesh/src/mutation.rs`, the commit points of
`crates/akantu-solid/src/model/assembly.rs` and
`crates/akantu-cohesive/src/model.rs`.

## Ids without a record

D-044, D-045, D-072, D-075, D-084, D-098, D-102, D-103, D-110, D-112 and
D-120 were decisions about how the port was carried out (compute
resources and their budgets, the models and number of agents, review and
merge procedure, publication). They set no rule for the code or its
evidence and have no record here.
