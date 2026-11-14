// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/petsc_wrapper.hh (SOLV-007),
// src/model/common/dof_manager/dof_manager_petsc.cc (PETScSingleton,
// petscErrorHandler, PETSc_call), src/solver/{sparse_matrix_petsc.*,
// solver_vector_petsc.*, sparse_solver_petsc.*}, src/io/parser/
// parsable_petsc.hh, src/model/common/non_linear_solver/
// {non_linear_solver_petsc.*, non_linear_solver_tao.*} (native parts).

//! PETSc bindings of the PETSc backend (Cargo feature `petsc`;
//! SOLV-005/007, SPM-008/012, NLS-006/007/008, DOF-015).
//!
//! **Licence.** The feature is opt-in and off by default. Debian's PETSc
//! (the project's image) links GPL-2.0-or-later libraries (SuiteSparse
//! UMFPACK / SPQR / CHOLMOD modules, FFTW), so a binary built with the
//! `petsc` feature is distributed under GPL-3.0-or-later as a whole (D-076
//! option A, `NOTICE.md` "PETSc"); the source stays LGPL-3.0-or-later and
//! the default build links no PETSc.
//!
//! A safe layer over a small C shim (`shim.c`, compiled by `build.rs`
//! against the system PETSc found with pkg-config): the Rust side never
//! sees PETSc's struct layouts or macros. The DOF-manager glue (matrices,
//! numbering, KSP solves, SNES/TAO drivers) is
//! `akantu_solver::petsc_backend`; this module owns every `unsafe` call.
//!
//! # Execution rules (`docs/ARCHITECTURE.md`)
//!
//! * **Initialisation** ([`Session::acquire`], upstream `PETScSingleton`):
//!   collective over the first communicator that uses PETSc in the process,
//!   with `-no_signal_handler`, the arguments given to [`set_command_line`]
//!   (upstream: akantu's global argc/argv; agreed identical on every rank
//!   first) and an error handler that records the first
//!   message instead of throwing. `PETSC_COMM_WORLD` is **akantu's own
//!   duplicate** of that communicator, kept alive until `PetscFinalize`
//!   returned. MPI is already initialised by akantu, so
//!   `PetscFinalize` never finalises MPI. As upstream (finalised once, at
//!   exit), PETSc stays initialised while its first user's communicator has
//!   an akantu handle: `PetscFinalize` is queued through the MPI lifecycle
//!   (before `MPI_Finalize`) only when that communicator **and** the last
//!   [`Session`] are gone. A later use on a new communicator initialises
//!   PETSc again (PETSc 3.22 then cannot find TAO types such as `gpcg`, so
//!   TAO needs one world per process).
//! * **Collective PETSc calls** (object creation and destruction, assembly,
//!   KSP/SNES/TAO solves, the application ordering) take a [`NativeComm`]:
//!   the capability handed out by `Communicator::native_collective` /
//!   `Communicator::native_region` after every rank agreed to enter, under
//!   the MPI thread policy (the `Serialized` lock; split-class admission).
//!   It is bound to that closure (it cannot be kept) and checked against
//!   the object's communicator. A PETSc error inside
//!   such a call is a transport-class failure for the caller (ranks may
//!   have diverged inside PETSc), except a failure that a callback reported
//!   (its status is agreed on every rank).
//! * **One thread at a time; lock order MPI → PETSc**
//!   ([`PetscLock`]). The Debian PETSc build is not thread-safe: every call
//!   holds a process-wide re-entrant PETSc lock. Native entries take it
//!   **inside** their agreed native closure ([`PetscLock::enter_native`],
//!   waiting: every rank is already committed), never across an agreement;
//!   destructions take it inside the lifecycle's destruction closure. Calls
//!   outside native entries (diagnostics, the session's initialisation
//!   query) only try it ([`PetscLock::try_enter`]: a local refusal).
//! * **Objects** ([`Matrix`], [`Vector`], [`Ksp`], [`Snes`], [`Tao`],
//!   [`Options`]) hold a clone of their communicator and of the session;
//!   dropping one queues its collective `*Destroy` through
//!   `Communicator::release_native` (SPMD order, before `MPI_Finalize`).
//!
//! # Unsafe sites
//!
//! Every `unsafe` block is in `session.rs`, `objects.rs` and
//! `callbacks.rs` and states its invariants; `ffi.rs` declares the shim.

mod callbacks;
pub mod ffi;
mod objects;
mod session;

pub use akantu_comm::NativeComm;
pub use callbacks::{NativeCallbacks, take_caught_panic};
pub use ffi::PetscInt;
pub use objects::{
    Ksp, KspOutcome, Matrix, Options, Snes, SnesOutcome, Tao, TaoOutcome, Vector, ao_map,
};
pub use session::{
    BlasInfo, PetscGuard, PetscLock, Session, WorldRelation, blas_info, set_command_line,
    set_global_option, version, world_relation,
};
