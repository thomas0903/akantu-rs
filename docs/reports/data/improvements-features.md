# Possible improvements in functionality, API and usability

A scan of the repository made on 14 November 2026 from three
points of view: an Akantu user who writes C++ or Python simulations, a Rust
developer who wants a library, and an Akantu maintainer looking for what to
take back upstream. It lists 39 findings in seven groups, each ranked by
value for effort within its group, and ends with the ten to do first. Most
of the gaps against upstream are things upstream lacks as well (contact and
structural models on distributed meshes, a C API, model-level Python on
several ranks). Most of the usability findings come from the port's
parity-first design: string-typed options, positional constructors and 14
separately shaped compatibility switch sets. No source code was changed for
this scan.

The scan was revised for the public tree on 15 November 2026: file
references, counts and section names were updated to the current tree, and
the findings that the preparation of the public repository resolved say so
(1.4, 6.2, 7.2, 7.4).

Each finding gives what is there now (with file references), the proposal,
who benefits, the effort and the risks. Efforts are estimates for one
developer who knows the code base; none of them was measured. "Hours"
means less than a day, "days" up to two weeks, "weeks" more. D-numbers
refer to entries of `docs/DECISIONS.md`, UD-numbers to
`docs/UPSTREAM_DEFECTS.md`.

## 1. Gaps against upstream

### 1.1 Model-level Python on several MPI ranks

Now: the MPI build of the Python module covers the mesh level only:
`Mesh.distribute()`, the local and global numbering, compared with the
oracle's MPI module at 1, 2 and 4 ranks bit for bit. A `SolidMechanicsModel`
on a distributed mesh, its solves and dumpers are not supported from Python
(`docs/manual/src/models/python.md`, "The MPI build of the module";
`docs/MPI_VALIDATION.md`, "What is refused on several ranks, and why"). The reason given is the missing
reference: upstream never runs its module on several ranks and its default
solver refuses several ranks.

Proposal: enable the models on a distributed mesh in the bindings. The Rust
models already run distributed. The work is in the bindings (communicator
ownership next to mpi4py, collective error agreement when a Python callback
raises on one rank) and in tests that compare a Python run with the Rust run
of the same partition, bit for bit, since both call the same code.

Benefits: Python users with parallel jobs; it removes the largest exception
to the "drop-in replacement" description of `crates/akantu-python/Cargo.toml`.

Effort: 1 to 2 weeks. Risks: the module runs MPI at thread level
`funneled`, so callbacks from other Python threads must stay refused; an
uncaught exception on one rank inside a collective already hangs the job in
both codes (python.md), which needs a documented launch pattern
(`python -m mpi4py`).

### 1.2 Structural model on a distributed mesh

Now: refused at construction with `Error::Unsupported` because upstream's
distributed residual is wrong at every interface node (UD-902, category A:
no synchronizer, ghost forces skipped; `docs/MPI_VALIDATION.md`, "What is
refused on several ranks, and why"; test `st05_serial_only_refusals`).

Proposal: register a node synchronizer for the rotational and translational
DOFs, assemble only not-ghost elements as the solid model does, and check
the lumped rotary inertia blocks (UD-059, the exact 3×3 nodal block) across
rank boundaries. Validate as the other distributed models: the port at 2, 4
and 8 ranks against its own serial run, with a bound stated per test (the
other models use 1e-14 to 1e-10, `docs/VALIDATION_SUMMARY.md`, "Partition
independence").

Benefits: users of large frame and shell models; upstream gets a design for
its own UD-902.

Effort: 1 to 2 weeks. Risks: no oracle exists, so evidence rests on
serial-vs-distributed and analytical checks.

### 1.3 Contact mechanics on a distributed mesh

Now: the generic contact model refuses a distributed mesh, recoverably
(parity row CONT-019, approved exception; test
`doc012_contact_model_refuses_a_distributed_mesh` in
`crates/akantu/tests/manual_claims_2.rs`). Upstream's contact model and
couplers are serial only. The couplers that contain a contact model inherit
the restriction.

Proposal: a distributed detection that exchanges a ghost layer of master
surface elements around each rank's slave nodes. The port already has a
grid synchronizer (`akantu-mesh`, built on the single `SpatialGrid` of
`docs/ARCHITECTURE.md`, used by the non-local laws), which is the natural
starting point. Resolution stays per rank; forces go through the
distributed DOF manager.

Benefits: users of the Hertz, bloc-friction and cohesive-contact setups at
sizes that need MPI; upstream maintainers, since upstream has no such
protocol.

Effort: 3 to 6 weeks. Risks: no oracle; detection order decides which
master is chosen, so serial-vs-distributed identity needs a deterministic
tie rule; pairs on partition boundaries.

### 1.4 The C API

Now: `crates/akantu-capi` exports one function, `akantu_version`
(`crates/akantu-capi/src/lib.rs`, 40 lines), and its crate documentation
says so. (At the scan it described handles, a generated header and a C++
wrapper that did not exist; that text was corrected for the public tree.)
The C API is not built (decision D-100; manual page
`docs/manual/src/models/not-yet-available.md`). Upstream has no C API.

Proposal: if wanted, a small C ABI: read a mesh, build a model from an
input file, `init_full`, Dirichlet and Neumann conditions by group,
`solve_step`, borrowed array views and `dump`. The foreign-view contract
already written for Python (`docs/ARCHITECTURE.md`, "Shared storage and its aliasing rules") covers the views.

Benefits: C, C++ and Fortran programs that want to call the port; Julia and
other languages through `ccall`.

Effort: a minimal ABI 1 to 2 weeks; full coverage months. Risks: every
exported function is `unsafe` surface that needs review under the
project's rules (`AGENTS.md`, "Rules for `unsafe` and licensing").

### 1.5 Configurations refused by default

Now: where upstream's result is undefined or wrong and no reviewed
formulation exists, the port refuses the input with an error; a
compatibility switch reproduces upstream (`docs/VALIDATION_SUMMARY.md`,
"Refused by default"). The
list: finite-deformation J2 plasticity (UD-030) and damage (UD-142), the
anisotropic damage law (UD-031 / UD-058; a `Corrected` model exists as an
opt-in awaiting independent review), Maxwell viscoelasticity with thermal
loading (UD-327), intrinsic insertion with `cohesive_linear` (UD-224),
extrinsic `_cohesive_3d_8` / `_cohesive_3d_16` (UD-223), bounded
`petsc_tao` solves whose bounds exclude a zero increment (UD-1022), the
`residual_mass_wgh` criterion (UD-194) and finite deformation in the IGFEM
laws (UD-613). A user who runs such an upstream input gets an error.

Proposal: implement reviewed formulations, one at a time, starting with
those users ask for. Candidates: a multiplicative finite-strain J2 return
mapping, damage on a finite-strain stress measure, and the review of the
existing anisotropic `Corrected` model (the cheapest, since the code
exists).

Benefits: users of those inputs; upstream gets corrected formulations.

Effort: the anisotropic review takes days; each new formulation takes weeks.
Risks: new physics without an oracle needs analytical checks and a
scientific review per D-003.

### 1.6 IGFEM and the extra materials without an upstream reference

Now: upstream's IGFEM package does not compile at the reference commit; the
port is checked against the `.verified` outputs of its last working version
at 6 printed digits, upstream's test criteria and analytical solutions.
Eleven of the twelve translation units of the extra materials do not
compile; the evidence is upstream assertions, one `.verified` file and
closed forms (`docs/VALIDATION_SUMMARY.md`, "No reference from upstream as
it is"). The embedded model was in the same
position and is now compared with a patched oracle
(`oracle/patches/embedded/`, eight patches, D-060).

Proposal: apply the embedded approach to both. Either build the last
upstream commit where IGFEM compiled as a second oracle image, or write a
compile-restoring patch set that changes no formula, reviewed hunk by hunk,
and record full-precision fixtures.

Benefits: higher confidence for these features; upstream gets patches that
make its extra packages build again.

Effort: 1 to 3 weeks each, most of it oracle work on x86_64 sandboxes.
Risks: an old commit may need an older compiler or Eigen; a patched oracle
is not upstream as it is, which the reports must keep saying.

### 1.7 Smaller refusals on several ranks

Now (`docs/MPI_VALIDATION.md`, "What is refused on several ranks, and
why"): split-node contact refuses partitions
that separate an interface pair (UD-1105) and runs without
`init_parallel` (UD-904); phase-field element events are refused on a
distributed mesh; parallel IGFEM is not provided (D-085); the non-local
`KeyOrder` route is refused on a distributed mesh (D-097); a distributed 1-D
facet mesh is refused (UD-1141, as upstream).

Proposal, in order of value: a partition constraint that keeps each
split-node pair on one rank (Scotch fixed vertices or a post-pass on the
partition), so the refusal is never reached; distributed renumbering for
phase-field element events; partition-independent global ids for
`KeyOrder`. Parallel IGFEM has no known users and can wait.

Benefits: users of split-node contact and phase field at scale.

Effort: days each, except parallel IGFEM (weeks). Risks: a constrained
partition changes the load balance and the fixture partitions recorded from
upstream.

### 1.8 PETSc coverage

Now: the PETSc back-end serves the solid, phase-field, diffusion,
structural and cohesive models. The contact couplers refuse a
`dof_manager_type` other than `default`; PETSc must have real double
scalars and 32-bit indices (the C shim refuses other builds at compile
time); a serial mesh's PETSc manager needs a one-process job
(`docs/manual/src/petsc.md`). Upstream's parallel PETSc path is unfinished
(UD-429), so multi-rank values have no oracle.

Proposal: accept the PETSc DOF manager in the contact couplers; support
64-bit PETSc indices for systems above 2³¹ unknowns.

Benefits: HPC users who standardise on PETSc.

Effort: days each. Risks: a binary built against Debian's PETSc is
GPL-3.0-or-later as a whole (`README.md`, licence section), which limits
who can redistribute it.

## 2. API and usability

### 2.1 One switch for upstream compatibility

Now: the corrected defaults of category-B defects are reverted through 14
`*Compat` structs with three shapes. `CohesiveUpstreamCompat`
(`crates/akantu-cohesive/src/material.rs:250`), `SolidContactCompat`
(`crates/akantu-couplers/src/contact.rs:308`, also `all()`) and
`TasnUpstreamCompat` (`crates/akantu-ntn-contact/src/lib.rs:100`) have an
`upstream()` function; `StructuralUpstreamCompat`
(`crates/akantu-fe/src/structural/element.rs:203`) and
`StructuralModelUpstreamCompat` (`crates/akantu-structural/src/model.rs:160`)
have an `UPSTREAM` constant; the other nine (`SmmUpstreamCompat`,
`DiffusionUpstreamCompat`, `EmbeddedUpstreamCompat`,
`PhaseFieldUpstreamCompat`, `ContactUpstreamCompat`,
`CouplerSolidPhaseFieldCompat`, `SparseSolverCompat`, `PartitionCompat` and
the solver's `UpstreamCompat`) are set field by field. Other switches are
setters or option fields: `DumperParaview::set_compat_layout(true)`,
`NonLinearSolverNewtonRaphson::set_legacy_hooks(true)`,
`ReadOptions::fill_unnamed_physical_groups = false`,
`WriteOptions::upstream_precision()`, `LawOptions::upstream()`
(`docs/manual/src/differences.md`, "Corrected defaults with a compatibility
switch"). The examples share one `--upstream-compat` flag implemented in
their 520-line harness (`crates/akantu/examples/common/mod.rs`).

Proposal: one `upstream()` / `corrected()` pair on every switch struct
(a small trait), a model-level `CompatProfile::{Corrected, Upstream}` in
`ModelOptions` that sets every switch the model owns, and an input-file parameter
(for example `upstream_compat = true` in the model section) so an existing
`.dat` file can ask for upstream's numbers without code. Fine-grained
fields stay.

Benefits: C++ users who want identical numbers first and corrections
second; anyone writing differential tests.

Effort: about a week. Risks: some switches must be set before `init_full`
and some after; the profile has to apply them at the right moment and
refuse late changes.

### 2.2 Typed solver options, energies and solver ids

Now: solver parameters go through the parameter registry by name, as in
upstream's `set("max_iterations", 2)`:
`solver.set_param("max_iterations", ParamValue::Int(2))` and
`ParamValue::Enum("solution".into())`
(`crates/akantu/examples/static/main.rs:79-82`). Energies are strings
(`energy(&mut self, energy_id: &str)`,
`crates/akantu-solid/src/model/energy.rs:413`), solver ids are
`Option<&str>` (`solve_step(None)`, `time_step(Option<&str>)`), and the DOF
manager type is a string (`init_dof_manager("petsc")`). Misspellings fail
at run time.

Proposal: typed option structs next to the registry, such as
`NewtonRaphsonOptions { max_iterations, threshold, convergence:
ConvergenceType::Solution }`, an `Energy` enum, and a `SolverId` newtype
with a constant for the default. The input-file path keeps the names; the
typed structs can be generated from the registry so the two cannot drift.

Benefits: Rust users (compile-time checks, completion in editors).

Effort: 1 to 2 weeks across the models. Risks: two ways to set the same
value; the registry stays the single source and the typed layer writes
through it.

### 2.3 Builders for model construction

Now: every manual chapter and example builds a model in four steps with a
positional constructor that mirrors the C++ one:
`SolidMechanicsModel::with_parser(mesh, None, "solid_mechanics_model", parser)`
(`docs/manual/src/getting-started.md`, `output.md`, `solid-mechanics.md`;
`crates/akantu/examples/static/main.rs:48-53`). The `None` is the spatial
dimension and the string is upstream's model id.

Proposal: a builder (`SolidMechanicsModel::builder(mesh).parser(p).build()`)
with the id and dimension defaulted, and a helper that reads a mesh and an
input file in one call. The existing constructors stay.

Benefits: Rust users; shorter manual examples.

Effort: days. Risks: none beyond keeping both paths tested.

### 2.4 An idiomatic mode for the category-C quirks

Now: 176 entries of `docs/UPSTREAM_DEFECTS.md` have a category starting
with C (168 C, 8 C+; `python3 scripts/docs/validation_counts.py defects`)
and reproduce upstream's behaviour by default. Some of them are defaults
that a future major version could change:

| Entry | Kept behaviour | Possible new default |
|---|---|---|
| UD-079 | J2 plasticity returns the elastic tangent, so Newton converges linearly | consistent tangent, elastic one as an option |
| UD-1293 | the default `solution` criterion never tests the residual of the last contact set | `residual` for the contact couplers |
| UD-1294 | the `residual` criterion is relative to the first residual and can stall at the rounding floor | an absolute floor next to the relative target |
| UD-1296 | boundary FE data is not updated after a cohesive insertion rewrites a boundary row | recompute jacobians and normals of rewritten rows |
| UD-475 | pure-ghost nodal fields drift in distributed explicit runs and appear in dumps | synchronise or leave them out of the dumps |
| UD-202 | the increment of displacement-type solutions is the last Newton correction | the step increment `u − u_previous` |
| UD-009 | `init_full` replaces integration schemes chosen in the solver options | keep the user's choice |
| UD-201 | the default solver id is set before the options are validated | set it after success |
| UD-017 | Newmark `alpha·beta` is the textbook β and `beta` is γ | accept `beta` and `gamma` under their textbook names |
| UD-055 | two expression grammars (scalars and vector entries) | one grammar |

Proposal: collect these behind the `Corrected` profile of 2.1, each with
its own test and a note in `differences.md`, and switch defaults only at a
major version with a deprecation period.

Benefits: new users; the table also lists candidates for upstream changes.

Effort: hours to days per entry, 3 to 5 weeks for the set. Risks: every
change moves away from the oracle; the fixtures cover the upstream mode
only, so each change needs analytical evidence and review.

### 2.5 The `Model` trait does not cover every model

Now: `akantu_model::Model` (`crates/akantu-model/src/model.rs:88`) is
implemented by `SolidMechanicsModel`, `DiffusionModel`,
`HeatTransferModel`, `PhaseFieldModel` and `StructuralMechanicsModel`.
`SolidMechanicsModelCohesive`, `ContactMechanicsModel`,
`EmbeddedInterfaceModel`, `SolidMechanicsModelIgfem` and the couplers do
not implement it (none of their crates has an `impl Model for`).

Proposal: implement it, or a smaller trait for the time loop
(`solve_step`, `stable_time_step`, `dump`), for every model and coupler, so
generic drivers can be written once.

Benefits: Rust users writing reusable drivers; the Python layer could use
it too.

Effort: days. Risks: some methods (mesh token, model solver) do not fit
couplers; a smaller trait avoids forcing them.

### 2.6 Accessors that need `&mut self` and nested results

Now: `current_position(&mut self)` (`crates/akantu-solid/src/model.rs:1356`)
and `energy(&mut self, …)` mutate caches on read;
`instantiate_materials` returns `Result<Option<Result<()>>>`
(`model.rs:658`); nodal arrays come back as `Result<ArrayRef<'_, Real>>`,
lock guards that need `?` even for a read. The lock design follows the
zero-copy NumPy views (D-047).

Proposal: explicit `update_current_position()` plus `&self` getters (or
interior caching), a dedicated outcome enum instead of the nested result,
and infallible read accessors where the lock cannot fail in practice.

Benefits: Rust users; borrows that today force sequential code.

Effort: days. Risks: changes to the lock protocol touch the Python views and
need the same review as the original design.

### 2.7 Richer prelude and facade features

Now: `akantu::prelude` holds the core prelude, `Mesh`, `SharedMesh`,
`Model` and `ModelOptions` (`crates/akantu/src/lib.rs:69-74`); the first
program of the manual imports eight paths. The facade has features for
`mpi`, `scotch`, `cgal` and `python`, none for `mumps` or `petsc`
(`docs/manual/src/getting-started.md`, row "no facade feature";
`petsc.md`: "the facade crate has no feature of that name"). The facade
feature `igfem` implies the GPL feature `cgal`, although `akantu-igfem`
needs it only for the growing-gel mesh generation
(`crates/akantu-igfem/Cargo.toml`, feature `cgal`).

Proposal: add the common enums, `read_mesh`, `Parser`, `FixedValue` and the
model types (under their features) to the prelude; add facade features
`mumps` and `petsc`; split `igfem` into an LGPL `igfem` and an
`igfem-cgal` that adds mesh generation.

Benefits: Rust users; IGFEM users who must stay under the LGPL.

Effort: hours for the prelude and features; a day or two to check which
IGFEM tests need the generator. Risks: the IGFEM split changes the meaning
of an existing feature name.

### 2.8 Structured error context

Now: `akantu_core::Error` is `#[non_exhaustive]` with structured variants
for indices, missing entities and dimensions, but several common variants
carry free text only (`InvalidArgument(String)`, `Mesh(String)`,
`Unsupported(String)`, `User(String)`, `Numerical(String)`;
`crates/akantu-core/src/error.rs`). A caller cannot find out which
parameter or group was wrong without parsing the message.

Proposal: add context fields where they exist (parameter name and input
section, element type and group, rank), keep the messages, and map the
main variants to Python exception subclasses.

Benefits: Rust and Python users handling errors; better messages in
batch runs.

Effort: 1 to 2 weeks for the common paths. Risks: `Error::code` values used
by the C API design must stay stable.

### 2.9 `Send` and `Sync`, checked once

Now: a compile check written for this scan (outside the repository)
confirms that `Mesh`, `SharedMesh`, `Parser`, `Error` and the models
`SolidMechanicsModel`, `SolidMechanicsModelCohesive`,
`ContactMechanicsModel`, `DiffusionModel`, `PhaseFieldModel` and
`StructuralMechanicsModel` are `Send`, and that `Mesh`, `SharedMesh`,
`Parser` and `Error` are `Sync`. `SolidMechanicsModel` is not `Sync`
because its boxed trait objects (`Material`, `Dumper`, `NonLinearSolver`,
`IntegrationScheme`, `LinearSystemBackend`, `WeightFunction`, event
handlers) have no `Sync` bound. Models do not need `Sync`; moving one to a
worker thread works.

Proposal: keep the property with compile-time assertions in a test, so a
future `Rc` or `Cell` does not remove `Send` silently.

Benefits: Rust users running models in worker threads or async services.

Effort: hours. Risks: none.

## 3. Python

### 3.1 Distribution name, wheels and platforms

Now: CI builds abi3 wheels for manylinux_2_28 x86_64 and macOS arm64 and
tests them (`ci/github/packaging.yml`); nothing is published (D-114). The
distribution is called `akantu` (`python/pyproject.toml`), the name
upstream publishes its own wheels under on PyPI
(`docs/manual/src/upstream-changes.md`, row "wheels and PyPI packaging").
No workflow runs on Windows (every job runs on `ubuntu-24.04` except the
macOS wheel job on `macos-15`) and no wheel targets Linux aarch64 or macOS
x86_64.

Proposal: agree a distribution name with the Akantu maintainers (for
example a separate distribution that installs the import package
`akantu`, or a release under upstream's project), add Linux aarch64 wheels
(the project's Docker images already run aarch64) and try a Windows build of
the default, MPI-free module.

Benefits: Python users, who today must build with maturin.

Effort: days for naming and aarch64; the Windows effort is unknown, since
the port has never been built there. Risks: two distributions that install
the same import name conflict in one environment; publication is the
user's decision.

### 3.2 Type stubs

Now: no `.pyi` file and no `py.typed` marker in `python/` or
`crates/akantu-python`.

Proposal: generate stubs from the PyO3 classes and ship `py.typed`.

Benefits: completion and type checking in editors for Python users.

Effort: 2 to 4 days, then kept current by a CI check that regenerates and
compares. Risks: upstream's overloads (element-type-map variants) need
hand-written `@overload` entries.

### 3.3 Upstream examples that fail in both codes

Now: `cohesive/custom_material.py` raises `AttributeError` with upstream's
module and with the port's, because `MaterialCohesive` does not bind
`getNormals` (UD-980; python.md, "Upstream's Python examples").

Proposal: bind `getNormals` as a documented extension so the upstream
example runs to its end, and offer the binding upstream.

Benefits: Python users writing cohesive laws; upstream.

Effort: hours to a day, plus a test of the example's later part (no oracle
exists past the error). Risks: none.

### 3.4 Read-only topology views need a visible note

Now: once a model uses a mesh, connectivity views become read-only and
earlier views go stale (UD-532; python.md, "Topology views are read-only
once a model uses the mesh"). Upstream lets scripts write the topology
under the model. Scripts that write connectivities after creating a model
will now fail, although the port is right to refuse.

Proposal: raise a clear error text that names UD-532 and the event
protocol when a script writes to such a view, and list the case first in a
Python migration section (see 7.1).

Benefits: Python users porting scripts.

Effort: hours. Risks: none.

## 4. Input, output and interoperability

### 4.1 Compressed and appended VTU

Now: the ParaView dumper writes inline base64 or ASCII arrays
(`crates/akantu-io/src/dumper/paraview.rs`). No crate depends on a
compression library, and `akantu-io` has no compression path.

Proposal: zlib-compressed and appended raw-binary VTU as dumper options;
keep the byte-identical upstream layout as the default.

Benefits: users of large or long runs (file size, ParaView load time).

Effort: 2 to 3 days. Risks: none for parity, since the default does not
change; the new layouts are checked with VTK's reader instead of the
oracle.

### 4.2 One-file parallel output (VTKHDF or XDMF)

Now: distributed runs write one `.vtu` piece per rank plus `.pvtu` and
`.pvd` files (`docs/manual/src/output.md`). At 64 ranks and thousands of
steps that is many small files.

Proposal: a VTKHDF writer (one HDF5 file holding all ranks and time
steps), or XDMF with HDF5, behind an optional feature.

Benefits: HPC users; post-processing scripts that read one file.

Effort: 2 to 3 weeks. Risks: HDF5 is a native dependency, and parallel
HDF5 needs an MPI-enabled build; neither format exists upstream, so there
is no oracle.

### 4.3 Model checkpoint and restart

Now: neither code has a model checkpoint. The legacy IOHelper restart
writer and reader were never built upstream and are approved exceptions
(parity rows DUMP-019, DUMP-020); split-node contact has its own restart
arrays (UD-396, UD-402).

Proposal: a checkpoint of nodal arrays, law internals and histories,
solver state, inserted cohesive elements and mesh changes, with a test
that a run of N steps equals, byte for byte, a run of N/2 steps, a
restart, and N/2 more. The port's deterministic reductions (D-039) make
that test possible.

Benefits: long runs such as the full-size cohesive benchmark (5,000 steps,
about 40 minutes at 64 ranks; `docs/BENCHMARKS.md`, "The full-size run");
recovery after a job limit.

Effort: 3 to 6 weeks, most of it for cohesive mesh changes and distributed
meshes. Risks: state hidden in laws and solvers; a format that must be
versioned.

### 4.4 Binary Gmsh files and MSH 4.1 output

Now: ASCII MSH 1.0 to 4.1 are read; binary MSH files are refused with
`Error::Unsupported`; the writer produces ASCII MSH 2.2
(`docs/manual/src/meshes.md`, "Supported formats").

Proposal: read binary MSH 2.2 and 4.1; optionally write MSH 4.1.

Benefits: users of large meshes, which Gmsh users often save as binary.

Effort: 3 to 5 days with round-trip tests on frozen meshes. Risks: test
meshes must be frozen files, since Gmsh output differs between aarch64 and
x86_64.

### 4.5 Other mesh formats through meshio

Now: Abaqus is listed by upstream's manual but has no reader in either
code (UD-049, refused); Diana is read-only. The Python package already
converts Gmsh data in `convertGmshToAkantuMesh` (`python/akantu/__init__.py`).

Proposal: a Python helper `Mesh.from_meshio(m)` (and the reverse), which
covers Abaqus, Exodus, VTK and others at once; a native Abaqus `.inp`
reader only if users ask.

Benefits: Python users with meshes from other tools.

Effort: 2 to 3 days for the meshio bridge; 1 to 2 weeks for a native
reader. Risks: meshio's element and group conventions need a mapping table
and tests.

## 5. New capabilities

### 5.1 Fuzzing of the parser and mesh readers

Now: the error policy says malformed input never panics
(`crates/akantu-core/src/error.rs`, header). Property tests exist in
`akantu-core`, `akantu-parser`, `akantu-io` and `akantu-ntn-contact`
(`proptest` dev-dependencies) but there is no fuzz target. The scan of
upstream found several input defects in exactly these parts: stack
overflow on cyclic or deep expressions (UD-080, UD-081), reads of
uninitialised data (UD-082, UD-083), and three Gmsh reader defects
(UD-064, UD-090, UD-095).

Proposal: `cargo-fuzz` targets for `Parser::parse_str`, the expression
evaluator, `read_mesh_from_str` (Gmsh, Diana) and the restart readers of
split-node contact, with a corpus from `fixtures/` and a short CI run.

Benefits: backs the no-panic claim with evidence; every user who feeds
files from elsewhere.

Effort: 2 to 3 days. Risks: none.

### 5.2 Deterministic shared-memory threading

Now: decision D-017 allows threads only for kernels whose threaded result
is bit-identical, each with a serial-vs-threaded test. No crate depends on
`rayon` or uses threads in its numerical code.

Proposal: thread the kernels with disjoint outputs first (per-element
stress and tangent evaluation, shape-function precomputation, dump
rendering), then assembly with a fixed reduction order (colouring or
per-thread buffers summed in thread order).

Benefits: users on one workstation without MPI; Python users, whose
model-level runs are single-process (1.1). Measured hot spots are in the
performance report.

Effort: 2 to 4 weeks for the main kernels. Risks: the bitwise rule rules
out unordered reductions; laws written by users (including Python laws
under the GIL) must declare whether they are thread-safe.

### 5.3 A pure-Rust distributed iterative solver

Now: the pure-Rust solvers (`eigen`, `faer`) are direct LU solvers; on
several ranks they gather the system on rank 0
(`docs/manual/src/differences.md`, "Deliberate deviations"). Iterative
Krylov solvers exist only through PETSc.

Proposal: a distributed conjugate gradient and GMRES with block-Jacobi
preconditioning under `sparse_solver_type`, later algebraic multigrid.

Benefits: parallel implicit runs without MUMPS or PETSc, and without the
GPL consequence of Debian's PETSc.

Effort: 1 to 2 weeks for CG and GMRES with Jacobi; months for multigrid.
Risks: iterative results depend on the partition and the tolerance, so
they cannot be bitwise against the direct solvers; documentation must say
so.

### 5.4 Material plug-ins through MFront

Now: new laws are written with the `Material` trait and registered in each
model's factory (`docs/manual/src/new-constitutive-laws.md`), or as Python
subclasses. The reference clone has an upstream branch
`features/mfront-material` (last commit 2026-09-24).

Proposal: follow that branch with an optional MFront/MGIS bridge once
upstream merges it.

Benefits: users with existing MFront behaviours.

Effort: 2 to 4 weeks. Risks: native C++ dependency; the upstream design
may still change.

### 5.5 WebAssembly build for teaching

Now: the default build is pure Rust with no native library
(`docs/manual/src/getting-started.md`, "Requirements"), and meshes and
input files can be read from strings (`read_mesh_from_str`,
`Parser::parse_str`). A `wasm32` build has not been tried.

Proposal: check whether `akantu-core` to `akantu-solid` compile for
`wasm32-unknown-unknown`; if they do, a browser demo for teaching.

Benefits: courses and outreach.

Effort: a day to check, 1 to 2 weeks for a demo. Risks: unknown, since
nothing was tried; some code uses `std::fs`.

## 6. Distribution and upstream tracking

### 6.1 A procedure for following upstream

Now: the port tracks commit `35a85178`. The hash appears in 168 files
under `crates/`, `scripts/`, `oracle/`, `python/` and `ci/` (36 under
`crates/`; `git grep -l 35a85178`, 15 November 2026). The oracle build
(`docs/ORACLE_BUILD.md`), the fixture generators (`oracle/probes/`, 50
directories; `oracle/scripts/`) and `fixture-diff` exist.
`docs/MAINTAINING.md` ("Following a new upstream commit") describes the
move to a newer commit as a manual procedure: the copies of the pin that
act on something, mapping changed upstream files to parity rows by search,
a second oracle, `fixture-diff`, porting row by row. At the scan no
document described it. In the local reference clone, `master` had no commit after the pin at its
last fetch (not fetched for this scan), and several upstream feature
branches continue past it: `features/matrix_free` (210 commits ahead of
`master`, last commit 2026-11-05), `features/ufl-model`,
`features/modular_fe_engine`, `features/52-anisotropic-and-at1-phase-field-models`
and `features/mfront-material`.

Proposal: tooling for the procedure: (1) keep the pinned commit in one file
and generate the constant and the scripts' copies from it; (2) a script
that maps an upstream diff to parity rows through the matrix's source
column (file and line references) and lists the affected rows and
fixtures. Steps (3) rebuild, regenerate and classify with `fixture-diff`
and (4) port row by row are already written down.

Benefits: whoever maintains the port after the hand-over; upstream
maintainers, who can see which upstream changes affect which rows.

Effort: about a week for the tooling; each sync then depends on its size.
Risks: line numbers in the matrix go stale with every upstream change;
fixture regeneration needs x86_64 sandboxes.

### 6.2 Changelog, version policy and crate metadata

Now: every crate is at version 0.1.0 (`Cargo.toml`, `workspace.package`).
`CHANGELOG.md` exists since the preparation of the public repository (at
the scan it did not), and `docs/MAINTAINING.md` ("Cutting a release")
describes a release. The facade exports `UPSTREAM_VERSION`
(`crates/akantu/src/lib.rs:77`). The MSRV is declared
(`rust-version = "1.90"`) and checked by a CI job (`ci/github/fast.yml`,
job `msrv`), while development uses 1.99.1 (`rust-toolchain.toml`). The
workspace sets `keywords` and `categories`; only the facade sets `readme`,
and no crate sets `documentation`.

Proposal: a written semver policy (a change of a default from 2.4 is a
major change, a new switch a minor one); the upstream commit in the
version's build metadata (for example `0.1.0+akantu.35a85178`);
`cargo-semver-checks` in CI once crates are published.

Benefits: Rust users who pin versions; reviewers of future changes.

Effort: a day. Risks: none.

### 6.3 crates.io readiness

Now: every crate has `publish = false`. A package dry run during the port
(25 members at the time; the workspace has 27 crates now) reported for every
member: no licence text in the package, no readme; files above 1 MiB (test
data) in `akantu-core` and `akantu-linalg`; MPL-2.0 files without
`LICENSES/MPL-2.0.txt` in those two crates. The weekly `crates-dry-run`
job of `ci/github/packaging.yml` repeats the check. The
`ci/tools/package_report.py` check R8 also flags tests that read the
workspace `fixtures/`, which cannot run from a published crate. Publication
is an approved exception for now (D-114).

Proposal: if publication is wanted, decide which crates are public (the
facade depends on all member crates, so they must all be published or
merged), add a readme and the licence texts to each, exclude test data,
and publish the GPL crate `akantu-geometry-cgal` with its own licence
field (already set).

Benefits: Rust users (a normal `cargo add`); docs.rs hosting of the API
reference.

Effort: 2 to 4 days of preparation. Risks: whether the crate names are free
on crates.io was not checked; publication is outward-facing and the user's
decision.

### 6.4 Patches and probes to offer upstream

Now: `oracle/patches/` holds patches that restore the compilation of
upstream's embedded package (eight patches) and of the mesh sphere
intersector (two), and zero-initialisation variants for UD-001, UD-031 and
UD-814 (three). The registry counts 148 entries of category A or A+ and 242
of category B or B+. The 48 probe programs in `oracle/probes/` exercise
upstream paths its own tests do not reach.

Proposal: offer the compile patches and selected probes to upstream as
merge requests and regression tests. The defect list itself is the subject
of the defects report.

Benefits: upstream maintainers; future syncs (6.1), since every patch
upstream accepts is one fewer to carry.

Effort: days to prepare. Risks: contact with upstream is the user's
decision.

## 7. Documentation

### 7.1 A C++ and Python to Rust name table

Now: `differences.md` has a nine-row table of API conventions;
`upstream-changes.md` maps changelog entries. The parity matrix already
pairs each upstream symbol (its source column) with the Rust file and type
(its port column) for 1,066 rows.

Proposal: generate a searchable appendix from the matrix: upstream C++ name,
Python name, Rust path, status. A script like `scripts/parity/stats.py` can
produce it and a test can keep it current.

Benefits: C++ and Python users moving to the Rust API.

Effort: 2 to 3 days. Risks: the port column holds free text in places and
needs a parseable form.

### 7.2 A README for newcomers

Done. At the scan the README's status section was one long paragraph of
identifiers and validation caveats before any line of code. The README of
the public tree opens with what the library is, its state, a quick start
and links; the status detail is in `docs/VALIDATION_SUMMARY.md`.

### 7.3 Standalone tutorials

Now: the 28 examples are ports of upstream's programs, compiled with a
520-line shared harness (`crates/akantu/examples/common/mod.rs`) and
oracle-oriented options (`--upstream-compat`, `--solver eigen`,
`--steps`). The manual's examples are doc-tests with hidden setup lines.

Proposal: three to five tutorials of 50 to 100 lines with no harness:
static elasticity, explicit wave propagation, cohesive fragmentation, a
Python custom material. Each one a cargo example and a manual page.

Benefits: new users of both languages.

Effort: about a week. Risks: none.

### 7.4 Small documentation defects

Done. The three defects found at the scan are fixed: the generated block
`availability-placeholders` of `docs/manual/src/getting-started.md` no
longer prints a sentence with an empty list, the crate documentation of
`akantu-capi` states what it exports (1.4), and the workspace rustdoc is
deployed. At the scan `cargo doc -p akantu --no-deps` (built without a
warning in 38 s) documented only the facade, whose modules are re-exports,
and the full reference needs `--workspace`. Since the first runs on the
public repository (14 November 2026, parity row CI-024) `docs.yml` deploys
the workspace rustdoc next to the manual and the Python API, at
<https://thomas0903.github.io/akantu-rs/>.

## The ten to do first

| Rank | Finding | Section | Effort | Main beneficiary |
|---|---|---|---|---|
| 1 | Small documentation defects, the `akantu-capi` text and a README with a quick start (done for the public tree) | 7.4, 1.4, 7.2 | hours | every new reader |
| 2 | One switch for upstream compatibility, also from the input file | 2.1 | about a week | C++ users, testers |
| 3 | A procedure and tooling for following upstream | 6.1 | about a week | maintainers of the port |
| 4 | Version policy and crate metadata (the changelog exists) | 6.2 | a day | Rust users |
| 5 | Python distribution name and Linux aarch64 wheels | 3.1 | days | Python users |
| 6 | Builders, a richer prelude and facade features `mumps` / `petsc` | 2.3, 2.7 | days | Rust users |
| 7 | Typed solver options, energies and solver ids | 2.2 | 1 to 2 weeks | Rust users |
| 8 | Fuzzing of the parser and mesh readers | 5.1 | 2 to 3 days | all users |
| 9 | Compressed and appended VTU | 4.1 | 2 to 3 days | users of large runs |
| 10 | A C++ and Python to Rust name table | 7.1 | 2 to 3 days | C++ and Python users |

After these, the larger feature items in order: model-level Python on
several ranks (1.1), the structural model on a distributed mesh (1.2) and
model checkpoints (4.3).

## Unverified ideas

- Whether the names `akantu` and `akantu-*` are free on crates.io; not
  checked, since the scan made no outside contact.
- Whether upstream ships Python type stubs; not checked.
- `no_std` numerical kernels (element shape functions, small tensors):
  not examined beyond the observation that `akantu-core` and `akantu-fe`
  use `std::fs`, `std::time` or `std::process` in about ten places.
- A `wasm32` build (5.5) was not attempted.
- Demand for distributed contact (1.3) among Akantu users is not known;
  the Akantu team could say.
- Julia or R bindings on top of a C API.
- GPU kernels for explicit dynamics.
