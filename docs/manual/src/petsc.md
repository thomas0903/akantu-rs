# The PETSc back-end

Upstream can delegate the linear algebra of implicit analyses to PETSc
(`AKANTU_IMPLICIT_SOLVER` with PETSc, `packages/petsc.cmake`): the DOF
manager `DOFManagerPETSc` with its matrices (`SparseMatrixPETSc`), KSP
linear solves (`SparseSolverPETSc`) and the SNES and TAO non-linear
solvers (`NonLinearSolverPETSc`, `NonLinearSolverTAO`). The port's PETSc
back-end (parity rows DOF-015, SPM-008, SPM-012, SOLV-005, SOLV-007,
NLS-006, NLS-007, NLS-008) is a binding to the same native library,
Debian's PETSc 3.22.5 as in the project's oracle image, through a C shim
(`crates/akantu-linalg/src/petsc/shim.c`) linked dynamically; no PETSc
source is copied or redistributed.

What it provides, in the solver crate (`akantu_solver::petsc_backend`):

* `PetscBackend`, a linear-system back-end of the DOF manager: the
  matrices are assembled as in the default back-end and brought into a
  PETSc `Mat` at each collective use, with upstream's ordering
  (application ordering over the owned equations) and symmetry flags;
* KSP linear solves (`sparse_solver_type = petsc`) with upstream's
  defaults (GMRES, ILU(0) on one process, block Jacobi with ILU(0) on
  several, `rtol = 1e-5`) and a private options database per solver;
* the SNES (`newtonls`) and TAO (`gpcg`) non-linear solvers
  (`petsc_snes`, `petsc_tao`), whose model callbacks run inside the
  solve, and the forwarding of a solver section's parameters as PETSc
  options (`snes_*`, `ksp_*`, `pc_*`, `tao_*`).

Where the port differs from upstream's PETSc classes is listed in
`docs/UPSTREAM_DEFECTS.md` (UD-422 to UD-429); for instance a SNES solve
does not write upstream's debugging file `J_petsc` unless the
compatibility switch `UpstreamCompat::petsc_write_j_petsc` asks for it
(UD-426).

## Used by the models

The models create their DOF manager by name, as upstream's
`ModelSolver::initDOFManager`: from the model section's
`dof_manager_type` (default `default`) when the model is constructed, or
with `init_dof_manager(type)` right after the constructor and before
`init_full` (`SolidMechanicsModel`, `PhaseFieldModel`; refused once a
solver or a DOF family exists, UD-1020). With the `petsc` feature the
type `petsc` gives the PETSc DOF manager (id `<model>:dof_manager_petsc`);
its default non-linear solvers then use the KSP, and `petsc_snes`,
`petsc_tao` with `sparse_solver_type = petsc` select the native solvers.
Without the feature `petsc` is refused with upstream's message for an
unregistered type, and `petsc_snes`, `petsc_tao` or
`sparse_solver_type = petsc` on the default DOF manager are refused too
(test `manual_claims_2.rs::doc_solver_petsc_types_are_refused`).

The PETSc DOF manager runs on the mesh's MPI communicator. A mesh that is
not distributed (read on one process) has a serial communicator without an
MPI handle: its PETSc DOF manager runs on `MPI_COMM_WORLD`, as upstream's
on one process, and the job must then have exactly one process. On more
processes the request is refused with `Error::Unsupported` (every process
would own its whole mesh: distribute the mesh); the number of processes is
a local query made before anything is communicated, so the refusal is an
ordinary error, also when only some processes ask, and the program can go
on using MPI. If MPI is uninitialised at that point, this rule initialises it at
`MPI_THREAD_FUNNELED`, the level of the default `MpiOptions`; a later
`Communicator::mpi_world` of the same process that asks for `Serialized`
or `Multiple` then returns the error "MPI provides Funneled, … required".
An environment initialised earlier is used at its own level, so a program
that needs a higher thread level initialises MPI with it before its first
PETSc DOF manager of a serial mesh. The world of these DOF managers is one duplicate of
`MPI_COMM_WORLD` shared by every such manager of the process and kept until
`akantu::runtime::finalize()` or the end of the process, never released by
dropping a model, so models created one after another work as upstream's
(tests: the isolated cases `pt02_serial_models_in_sequence` and
`pt02_refusal_recoverable` of `petsc_models.rs`, PETSc tier).
PETSc options of PETSc's global database (upstream's
`PetscOptionsSetValue(NULL, …)`) are set with
`akantu_linalg::petsc::set_global_option`, identically on every process;
the command line (`akantu_linalg::petsc::set_command_line`) and
`PETSC_OPTIONS` enter the same database, and every solver applies it after
its own options (a global value wins; the KSP reads options only once one
of its own was set, UD-425). An option set before PETSc is initialised is
applied at the initialisation, before the command line and `PETSC_OPTIONS`
(upstream's order: its TEST-100 calls `PetscOptionsSetValue` before
`initDOFManager("petsc")`; test `petsc_mpi_cases::pt03_pending_global_option`).
PETSc does not check option values when they are set. An invalid value,
of a solver's own option (`set_param`, a solver section) or of a global one
(`set_global_option`, the command line, `PETSC_OPTIONS`), surfaces at the
next solve, inside a collective native call, as
`Error::Fatal { kind: Transport }`: the process's communicators are marked
broken and the runtime abort policy applies (the job ends with exit code
71), where upstream throws an ordinary exception (a PETSc error inside an
agreed native collective, see `docs/ARCHITECTURE.md`). PETSc
stays initialised while the communicator of its first user lives (a
re-initialised PETSc 3.22 cannot find its TAO solver types); for the models
of serial meshes that is the process-wide world above, so nothing has to be
kept alive, and a program that builds PETSc objects on communicators of its
own keeps the first of them alive while it uses PETSc.

On a distributed mesh the PETSc DOF manager keeps the DOF manager's
local-equation vectors: `assemble_to_global_array` and
`get_array_per_dofs` read a slave node's local value, where upstream's
ghosted PETSc vectors give the owner's value, and assembling into a slave
node's entry reaches its owner in the solves, where upstream's PETSc
manager drops it ([UD-1021](../../UPSTREAM_DEFECTS.md)). The phase-field
model's bounded TAO solve on a distributed mesh is compared with
upstream's one-process result (test
`petsc_models.rs::test_phase_field_optimal_linear_distributed`): bit for
bit on one process; on 2 and 4 processes PETSc's default preconditioner is
block Jacobi instead of ILU(0), so the result moves within the solver's
tolerances (upstream's own criterion holds); with a preconditioner that
does not depend on the partition (the solver's option `pc_type none`) the
distributed and one-process results agree within $10^{-14}$.

The phase-field model's bounded `petsc_tao` solve is described in
[The phase-field model](models/phase-field.md#bounded-tao-solve). The
contact couplers keep the default DOF manager (a `dof_manager_type` other
than `default` in their section is refused). A program can also use
the back-end directly: a `DofManager` built on a `PetscBackend`, and a
`ModelSolver` around it with the program's own solver callback, as the
project's tests `crates/akantu-solver/tests/petsc_mpi.rs` do (KSP, SNES
and TAO solves on 1, 2, 4 and 8 processes; the one-process cases are
compared with the oracle's record of upstream's PETSc tests); the models
on the PETSc DOF manager are tested by
`crates/akantu-couplers/tests/petsc_models.rs`.

## Building with PETSc

The back-end is compiled only with the Cargo feature `petsc` of
`akantu-solver` (it enables `petsc` of `akantu-linalg` and the MPI
communicator `akantu-comm/mpi`); the features `petsc` of `akantu-solid`,
`akantu-phase-field` and `akantu-couplers` enable it for those crates. It needs MPI and its C compiler wrapper
(`$MPICC`, default `mpicc`), `pkg-config` finding PETSc (`PETSc.pc` or
`petsc.pc`), and a PETSc 3.17 or later built with real double scalars and
32-bit indices (the shim refuses other builds at compile time). It is
never part of the default build, and the facade crate has no feature of
that name: enable it on the crate, Cargo's feature unification then
applies it to the whole build:

```toml
[dependencies]
akantu = { path = "…/akantu-rs/crates/akantu" }
akantu-solver = { path = "…/akantu-rs/crates/akantu-solver", features = ["petsc"] }
```

The project builds and tests it in the Docker image `akantu-rs-petsc:dev`
(`docker/petsc/Dockerfile`, Debian's `petsc-dev`; the `petsc` tier of
`docs/TESTING.md`, "The tiers").

## Threads and destruction

PETSc objects follow the contract of every native solver object of the
port (decision D-054, as MUMPS): PETSc is driven from one thread, and
its objects (DOF managers on a `PetscBackend`, their KSP, SNES and TAO
solvers) are dropped on MPI's main thread, at the same program point on
every process, with no other thread inside MPI or PETSc. Some PETSc
destructions are collective: a KSP whose preconditioner factorises with
an external package (`pc_factor_mat_solver_type`, e.g. the MUMPS or
SuperLU_DIST libraries) runs that package's collective clean-up when it
is destroyed, so dropping it on another thread while the main thread
solves can hang.

## Licence of a build with PETSc

PETSc itself is under the BSD-2-Clause licence, and the shim uses only
its public interface, so the port's source stays LGPL-3.0-or-later.
Debian's PETSc, the one of the project's images, is built with further
libraries and links them dynamically, among them code under the
GPL-2.0-or-later licence (Debian's copyright files):

| Linked library (through PETSc) | GPL-2.0-or-later part |
|---|---|
| SuiteSparse UMFPACK (`libumfpack`) | all of UMFPACK |
| SuiteSparse SPQR (`libspqr`) | all of SPQR |
| SuiteSparse CHOLMOD (`libcholmod`) | the modules Supernodal, MatrixOps, Modify and GPU (the rest is LGPL-2.1-or-later) |
| FFTW 3 (`libfftw3`, `libfftw3_mpi`) | all of FFTW |
| CombBLAS (`libCombBLAS`, through SuperLU_DIST) | the header-only `psort` sorting code; whether it is compiled into the library was not determined |

The port never calls these libraries (it selects no PETSc solver type that
uses them; upstream neither), but a program built with the `petsc` feature
against such a PETSc is linked with them. **Such a binary is distributed
under GPL-3.0-or-later as a whole** (the port's LGPL-3.0-or-later code and
the GPL-2.0-or-later libraries are both compatible with GPL-3.0-or-later),
as a build with the `cgal` feature ([Geometry](geometry.md)); this is the
project's decision D-076. The default build links no PETSc and no GPL
code. A PETSc configured without SuiteSparse, FFTW and SuperLU_DIST
would avoid the GPL components (everything the port uses remains); the
project does not provide one. Details: `NOTICE.md` and
`docs/DEPENDENCIES.md` (*PETSc licensing*).
