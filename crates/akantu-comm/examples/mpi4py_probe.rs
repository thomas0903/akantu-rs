// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/akantu/__init__.py (mpi4py is
// imported first when MPI is enabled) and src/synchronizer/
// mpi_communicator_data.hh (externally initialised MPI is never finalised
// by akantu). akantu-rs test probe (INT-009).

//! A `cdylib` loaded through `ctypes` by `scripts/mpi/mpi4py_first.py`
//! **after** `from mpi4py import MPI` initialised MPI: the real
//! mpi4py-first lifecycle (`docs/ARCHITECTURE.md`). The Python bindings proper
//! will build on the same `Communicator::mpi_world` /
//! `Communicator::from_mpi` entry points.
//!
//! `akantu_comm_mpi4py_probe(0)` (Python main thread): the environment
//! must be imported, the entering thread MPI's main thread; collectives,
//! an agreed rank-specific failure and a synchronizer exchange run on a
//! duplicate of `MPI_COMM_WORLD`; after the last handle akantu must not
//! have finalised MPI. `akantu_comm_mpi4py_probe(1)` (a Python worker
//! thread, mpi4py thread level FUNNELED): entering akantu must be refused.
//! Returns 0 on success.

use std::ffi::c_int;

use akantu_comm::lifecycle::{InitOwnership, ThreadLevel};
use akantu_comm::phase::{Severity, agree};
use akantu_comm::{
    Communicator, MpiOptions, ReduceOp, SendMode, SimpleIdxDataAccessor, Synchronizer,
    mpi_lifecycle,
};
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Error, Idx};

fn options() -> MpiOptions {
    MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(std::time::Duration::from_secs(20)),
        send_mode: SendMode::Eager,
        ..MpiOptions::default()
    }
}

fn main_thread_probe() -> Result<String, String> {
    let g = mpi_lifecycle()
        .initialize(ThreadLevel::Single)
        .map_err(|e| e.to_string())?;
    if g.ownership() != InitOwnership::Imported {
        return Err("mpi4py initialised MPI first: the environment must be imported".into());
    }
    if !g.policy().on_main_thread() {
        return Err("MPI_Is_thread_main must hold on Python's main thread".into());
    }
    let provided = g.policy().provided();
    drop(g);
    let c = Communicator::mpi_world(options()).map_err(|e| e.to_string())?;
    let (r, p) = (c.rank(), c.size());
    let mut v = [f64::from(r) + 0.5];
    c.all_reduce(&mut v, ReduceOp::Sum)
        .map_err(|e| e.to_string())?;
    let expected: f64 = (0..p).map(|k| k as f64 + 0.5).fold(0.0, |a, b| a + b);
    if v[0] != expected {
        return Err(format!("rank-ordered sum {} != {expected}", v[0]));
    }
    // A rank-specific failure is agreed on every rank.
    let local = if r == 0 {
        Err(Error::invalid("rank 0 fails"))
    } else {
        Ok(())
    };
    if agree(&c, "probe", local, Severity::Recoverable).is_ok() {
        return Err("agreement must fail on every rank".into());
    }
    // Ring synchronizer exchange.
    let pi = i32::try_from(p).map_err(|e| e.to_string())?;
    let mut s: Synchronizer<Idx> = Synchronizer::new("mpi4py-ring", c.clone());
    if p > 1 {
        s.communications.send.insert((r + 1) % pi, vec![0]);
        s.communications.recv.insert((r + pi - 1) % pi, vec![1]);
    }
    let mut data = [f64::from(r), f64::NAN];
    let mut acc = SimpleIdxDataAccessor::new(&mut data, 1, SynchronizationTag::Whatever);
    s.synchronize(&mut acc, SynchronizationTag::Whatever)
        .map_err(|e| e.to_string())?;
    if p > 1 && data[1] != f64::from((r + pi - 1) % pi) {
        return Err(format!("ring exchange delivered {}", data[1]));
    }
    drop(s);
    drop(c);
    if mpi::environment::is_finalized() || mpi_lifecycle().is_finalized() {
        return Err("akantu finalised an imported MPI environment".into());
    }
    let st = akantu_comm::mpi_backend::handle_stats();
    if st.comms_freed != st.comms_created {
        return Err(format!("akantu leaked communicators: {st:?}"));
    }
    Ok(format!(
        "imported provided={provided:?} comms={}",
        st.comms_created
    ))
}

fn worker_probe() -> Result<String, String> {
    match Communicator::mpi_world(options()) {
        Err(e) if e.to_string().contains("main thread") => Ok(format!("worker entry refused: {e}")),
        Err(e) => Err(format!("unexpected error: {e}")),
        Ok(_) => Err("a worker entry under FUNNELED must be refused".into()),
    }
}

/// Entry point called from Python (`ctypes`).
#[unsafe(no_mangle)]
pub extern "C" fn akantu_comm_mpi4py_probe(mode: c_int) -> c_int {
    let r = std::panic::catch_unwind(|| match mode {
        0 => main_thread_probe(),
        1 => worker_probe(),
        _ => Err(format!("unknown mode {mode}")),
    });
    match r {
        Ok(Ok(msg)) => {
            println!("MPI4PY-PROBE mode={mode} ok: {msg}");
            0
        }
        Ok(Err(e)) => {
            println!("MPI4PY-PROBE mode={mode} FAILED: {e}");
            1
        }
        Err(_) => {
            println!("MPI4PY-PROBE mode={mode} PANICKED");
            2
        }
    }
}
