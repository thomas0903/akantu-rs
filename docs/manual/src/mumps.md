# The MUMPS sparse solver

Upstream solves the linear systems of implicit analyses with a sparse
direct solver chosen by `sparse_solver_type` ([Solver
configuration](solver-configuration.md)); with
`AKANTU_IMPLICIT_SOLVER = Mumps`, the default of its `implicit` package
(`packages/implicit.cmake`), that is the parallel MUMPS library
(`SparseSolverMumps`). The port's MUMPS back-end (parity rows
SOLV-002/003) is a binding to the same native library, Debian's parallel
MUMPS 5.7.3 as in the project's oracle image, linked dynamically; no
MUMPS source is copied or redistributed. The native libraries a build
with the feature links, as [`docs/DEPENDENCIES.md`](../../DEPENDENCIES.md)
(*MUMPS backend*) records them for the test image:

| Library | Version (Debian) | Licence | Used |
|---|---|---|---|
| MUMPS (`libdmumps`, `libmumps_common`) | 5.7.3 | CeCILL-C (its PORD ordering: public domain) | by the port's C shim |
| ScaLAPACK (Open MPI build) | 2.2.2 | BSD-3-Clause | through MUMPS |
| PT-Scotch / Scotch | 7.0.7 | CeCILL-C | through MUMPS orderings |
| METIS | 5.1.0 | Apache-2.0 | through MUMPS orderings |
| OpenBLAS | 0.3.29 | BSD-3-Clause | through MUMPS and ScaLAPACK |
| Open MPI | 5.0.7 | BSD-3-Clause | through `akantu-comm` (feature `mpi`) and MUMPS |

All are linked dynamically and none is part of the default build; a
binary built with the feature must ship MUMPS's CeCILL-C notice.

## Building with MUMPS

The back-end is compiled only with the Cargo feature `mumps` of
`akantu-linalg` (also exposed by `akantu-solver`), which needs MPI
(`akantu-comm/mpi`), a C compiler and the MUMPS development files. It is
never part of the default build, and the facade crate has no feature of
that name: enable it on the crate, Cargo's feature unification then
applies it to the whole build:

```toml
[dependencies]
akantu = { path = "…/akantu-rs/crates/akantu" }
akantu-linalg = { path = "…/akantu-rs/crates/akantu-linalg", features = ["mumps"] }
```

The project builds and tests it in the Docker image `akantu-rs-mumps:dev`
(`docker/mumps/Dockerfile`; [`docs/TESTING.md`](../../TESTING.md), *MUMPS
tier*). Without the feature the solver is not registered:

```rust
use akantu::core::enums::SparseSolverType;
use akantu::linalg::direct::SparseSolverFactory;

let factory = SparseSolverFactory::default();
// the model's rule for `auto` (see "Selecting it"): `resolve_available(type, mpi)`,
// `mpi` = the DOF manager communicates over an MPI communicator
let auto = SparseSolverType::Auto;
// serial or in-process communicator: `eigen`, with or without MUMPS compiled in
assert_eq!(factory.resolve_available(auto, false), SparseSolverType::Eigen);
if factory.is_registered("mumps") {
    // a build with the feature `mumps`, on an MPI communicator
    assert_eq!(factory.resolve_available(auto, true), SparseSolverType::Mumps);
} else {
    // the default build (the one this manual's tests run in)
    assert_eq!(factory.resolve_available(auto, true), SparseSolverType::Eigen);
    let err = factory.create(SparseSolverType::Mumps, 1).unwrap_err();
    assert!(err.to_string().contains("not available in this build"));
}
# Ok::<(), akantu::core::Error>(())
```

## Selecting it

From the input file, in a `time_step_solver` section:

```dat
model solid_mechanics_model [
    time_step_solver static [
        sparse_solver_type = mumps
    ]
]
```

or from code with `ModelSolverOptions::sparse_solver_type =
Some(SparseSolverType::Mumps)`. The default, `auto`, resolves:

* to `mumps` when the back-end is compiled in and the DOF manager
  communicates over an MPI communicator (any number of processes,
  including one: upstream's `_fully_distributed` mode);
* to `eigen` otherwise, in particular in a build with `mumps` that runs
  on the serial or in-process communicator, because the parallel MUMPS
  library needs MPI. The sequential MUMPS library (upstream's
  `_not_parallel` mode) is not ported. Upstream's `auto` picks MUMPS
  whenever it is compiled (`model_solver.cc`); this is the one difference
  in the selection rule;
* never to `faer` (D-040).

The model applies this rule with `SparseSolverFactory::resolve_available(type,
mpi)` (the example above); `SparseSolverFactory::resolve(type)` is
upstream's factory-level rule (MUMPS whenever it is registered). Tests:
the unit test `factory_names_and_auto` of `crates/akantu-linalg/src/direct.rs`
(`auto` on a non-MPI communicator gives `eigen`) and
`crates/akantu-linalg/tests/mumps_mpi.rs` (MPI communicators, MUMPS tier).

An explicit `mumps` in a build without the feature is an error when the
solver is created (test
`manual_claims_2.rs::doc_mumps_explicit_selection_without_the_feature`);
on a non-MPI communicator it is an error at the solve, on every process
(`crates/akantu-linalg/tests/mumps_mpi.rs`, MUMPS tier). Upstream's Eigen solver refuses to run on several
processes; the port's DOF manager instead gathers the system on rank 0,
solves it there with `eigen` or `faer` and scatters the solution
([Parallel computations](parallel.md)).

What MUMPS is given (upstream's controls): the distributed assembled
matrix (`ICNTL(18) = 3`, every process passes its own entries), a
right-hand side and solution centralised on process 0, `sym = 2` for a
symmetric matrix, automatic scaling (`ICNTL(8) = 77`) and the MUMPS
defaults otherwise. The analysis is redone when the matrix profile
changes, the factorisation when its values change, and every solve runs
the solve phase.

Setting MUMPS controls is a port extension. Upstream's
`SparseSolverMumps` keeps its control accessor private, so an upstream
program cannot change any `ICNTL` or `CNTL`; the port lets a program set
them through `MumpsSolver::options_mut()` (`MumpsOptions::icntl`,
`MumpsOptions::cntl`), applied after upstream's values before every
analysis. Values the backend cannot honour are refused before MUMPS runs,
with the same error on every process: indices outside `ICNTL(1–60)` /
`CNTL(1–15)`; a `CNTL` that is not finite; `ICNTL(5)`, `ICNTL(18)`,
`ICNTL(20)` and `ICNTL(21)` other than upstream's values (the assembled
distributed matrix and the centralised dense right-hand side above); the
values that make MUMPS read or write arrays the backend never provides,
namely `ICNTL(7) = 1` (user pivot order), `ICNTL(8) = -1` (user scaling),
`ICNTL(15) > 0` (block structure), `ICNTL(19) ≠ 0` (Schur complement),
`ICNTL(25) < 0` (null-space basis), `ICNTL(26) ≠ 0` (reduced right-hand
side), `ICNTL(30) ≠ 0` (sparse right-hand side), `ICNTL(32) ≠ 0`
(right-hand side during the factorisation); and a non-zero value of the
indices MUMPS 5.7.3 reserves (`ICNTL(40–47)`, `ICNTL(50–55)`, `ICNTL(57)`,
`ICNTL(59)`, `ICNTL(60)`). Every other value is passed to MUMPS as given
(its user's guide documents them).

Iterative refinement and error analysis (`ICNTL(10)`, `ICNTL(11)`) are
accepted. The backend clears its pointers to the distributed matrix entries
(`IRN_loc`, `JCN_loc`, `A_loc`) after each MUMPS job, and whether MUMPS
5.7.3 reads the distributed matrix during the solve phase when these
controls are set was not established. The tests that set `ICNTL(10) = 2`
or `ICNTL(11) = 2` (`crates/akantu-linalg/tests/mumps_mpi.rs`) check only
that the residual is below 1e-10, which holds whether or not MUMPS
refines.

## Rules a program must follow (D-054)

The MUMPS library keeps state shared by all processes of the communicator
it was created on, and its calls are blocking collectives. The port
therefore binds every MUMPS solver to that communicator and checks, before
each native call, that all processes agree; a violation is an ordinary
`Error` returned on every process before the library runs, instead of
the crash or hang upstream can produce. What this means for a program:

* One communicator. A solver is bound to the communicator of its
  creation. Using it with another communicator (a duplicate, a split, a
  reordered group, or a second import of the same `MPI_Comm`) is refused;
  to move it, destroy it (`destroy()` on the old communicator) and solve on
  the new one.
* Identical settings on every process. All control parameters
  (`ICNTL`, `CNTL`, the retry settings, the upstream-compatibility switch
  below) are compared across processes before the library runs;
  different values are an error on every process.
* Same calls on every process. Every native call carries its job
  identity, so processes that enter different jobs (one solving, another
  destroying) fail before the library instead of deadlocking.
* Drop on the main MPI thread. A solver is destroyed collectively;
  drop solvers at the same program point on every process, on MPI's main
  thread, with no other thread inside MPI. After a transport failure of
  the communicator the solver is abandoned without the collective
  destruction (its memory is leaked) and the job aborts.
* One native job at a time. With the MPI thread levels `Serialized`
  and `Multiple`, two MUMPS jobs never run at the same time in one
  process: a job started while another thread's is in flight is refused on
  every process (drive MUMPS solvers from one thread, or retry).

Regression tests pin these rules
(`crates/akantu-linalg/tests/review_v54_regressions.rs`,
`crates/akantu-comm/tests/review_v54_native.rs`,
`crates/akantu-linalg/tests/review_v66_regressions.rs`, run by the MUMPS tier
on 1, 2 and 4 processes and the three thread levels); the full contract is
decision D-054 in `docs/DECISIONS.md`.

## Errors and the workspace retry (UD-216)

After every phase the global MUMPS status (`INFOG(1)`) is checked: a
numerically or structurally singular matrix is `Error::LinearSolver`
("Solver encountered singular matrix", as upstream), other failures are
`Error::Backend` with the MUMPS codes, non-finite input or output is
`Error::Numerical`. When MUMPS reports a too small workspace
(`INFOG(1) = -9`), the workspace parameter `ICNTL(14)` is raised by 10 and
the analysis, factorisation and solve are redone, up to `ICNTL(14) = 90`.

Upstream's retry has a defect (UD-216, category B): after a retry during
the factorisation its outer solve uses the *solution* of the inner solve
as right-hand side, so after `k` retries it returns `A^-(k+1) b` without
any error (confirmed on the oracle: relative error 1.4e23 after 8
retries). The port starts every solve from the original right-hand side.
The switch `mumps_retry_with_solution_rhs` of the DOF manager's
`akantu::solver::UpstreamCompat` reproduces upstream; it applies to every
sparse solver of that manager, also after the non-linear solver
reallocates one (D-055), and overrides the per-solver option
`MumpsOptions::retry_rhs`:

```rust
use akantu::solver::UpstreamCompat;

let compat = UpstreamCompat::default(); // corrected behaviour
assert!(!compat.mumps_retry_with_solution_rhs);
assert!(!compat.sparse_solver().mumps_retry_with_solution_rhs);
let upstream = UpstreamCompat { mumps_retry_with_solution_rhs: true, ..UpstreamCompat::default() };
assert!(upstream.sparse_solver().mumps_retry_with_solution_rhs);
# Ok::<(), akantu::core::Error>(())
```

Tests: `crates/akantu-solver/tests/mumps_mpi_solver.rs::ud216_manager_compat`
(corrected: bitwise equal to the solve without retry; compat: upstream's
`A^-(k+1) b`) and
`review_v54_regressions.rs::mu06_manager_wide_ud216_switch_reaches_every_solver`.

## Bitwise reproducibility

On one process the solutions of the 13 comparison systems are equal bit
for bit to those of the oracle's MUMPS (`fixtures/solver/mumps`), on
x86_64 with OpenBLAS pinned to one thread and the Haswell kernels
(`OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1`, set by the test tier;
the aarch64 files are a separate platform variant). Results on several
processes are compared with tolerances, not bit for bit: a multi-process
factorisation is not bitwise reproducible from run to run (observed while
writing the MUMPS tier, `docs/TESTING.md`).
