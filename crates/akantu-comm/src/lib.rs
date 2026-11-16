// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/{communicator.hh,
// communicator_dummy_inline_impl.hh, communicator_mpi_inline_impl.hh,
// communication_buffer.hh, communication_tag.hh, communication_request.hh,
// communications.hh, communication_descriptor.hh, synchronizer.hh,
// synchronizer_impl.hh, synchronizer_registry.hh, data_accessor.hh}.

//! # akantu-comm
//!
//! The distributed-memory communication layer (decision D-015), ported from
//! upstream's `src/synchronizer`. Cargo feature: `mpi` (the MPI back-end;
//! it needs an MPI implementation and `libclang` at build time).
//!
//! * [`Communicator`]: one concrete, cheaply clonable handle with generic
//!   methods; the backend is the always-available serial backend, the
//!   in-process local world (tests), or MPI (rsmpi raw bindings, Cargo
//!   feature `mpi`: `Communicator::mpi_world`). All akantu code is
//!   written against this type only, so serial and parallel builds share
//!   every code path (upstream `Communicator` + `CommunicatorDummy`).
//! * [`buffer::CommunicationBuffer`]: byte buffer with typed pack/unpack.
//! * [`synchronizer`]: generic `Synchronizer<E>` (send/receive schemes per
//!   rank) and the [`synchronizer::DataAccessor`] trait implemented by
//!   models, materials, the FE engine, … to pack/unpack their data.
//!
//! Reductions (decision record D-039): by default every reduction and scan
//! is folded in rank order (`((v0 ∘ v1) ∘ v2)…`) in every backend (the
//! MPI backend gathers the contributions instead of calling
//! `MPI_Allreduce`, whose order MPI leaves to the implementation), so
//! results are bitwise identical between the serial backend, the local
//! world and MPI for a fixed rank count; the MPI backend also offers the
//! [`ReductionPolicy::NativeMpi`] compatibility policy (MPI's typed
//! reductions). Neither policy implies rank-count or partition independence
//! (`docs/NUMERICAL_VALIDATION.md`).
//!
//! `unsafe` is confined to the MPI FFI edge, feature `mpi`: the module
//! `mpi_backend` and the handle imports `Communicator::from_mpi` and
//! `Communicator::from_raw_mpi` (an `unsafe fn`). Without that feature the
//! crate forbids it.

#![cfg_attr(not(feature = "mpi"), forbid(unsafe_code))]
#![cfg_attr(feature = "mpi", deny(unsafe_code))]
#![warn(missing_docs)]

pub mod accessor;
pub mod buffer;
pub mod communicator;
pub mod lifecycle;
#[cfg(feature = "mpi")]
pub mod mpi_backend;
// Poll pacing of the MPI backend's waits and the self channel;
// compiled on every build so that it is unit tested.
#[cfg_attr(not(feature = "mpi"), allow(dead_code))]
mod pacing;
pub mod phase;
pub mod registry;
// Used by the MPI backend only; compiled on every build so that its
// semantics are unit tested without MPI.
#[cfg_attr(not(feature = "mpi"), allow(dead_code))]
mod self_channel;
pub mod synchronizer;

pub use accessor::{ReduceDataAccessor, SimpleIdxDataAccessor};
pub use buffer::{CommunicationBuffer, Packable};
pub use communicator::{
    BarrierRequest, CommData, Communicator, MAX_MESSAGE_BYTES, RecvRequest, ReduceOp,
    ReductionPolicy, SendMode, SendRequest, Status, TRANSPORT_ABORT_CODE, Tag, ValueLoc, run_local,
    run_local_with_mode,
};
#[cfg(feature = "mpi")]
pub use mpi_backend::{
    MpiOptions, NativeComm, NativeTicket, RsmpiRuntime, exit_hook_registered, mpi_lifecycle,
    release_at_exit, world_size,
};
pub use registry::SynchronizerRegistry;
pub use synchronizer::{Communications, DataAccessor, PostedExchange, Synchronizer};
