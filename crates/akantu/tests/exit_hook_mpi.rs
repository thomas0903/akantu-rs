// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): process-exit
// behaviour of `akantu::runtime::initialize` under MPI, against upstream
// src/common/aka_common.cc:62-125 (`initialize` + `std::atexit(finalize)`),
// src/synchronizer/communicator.cc:58-86 (the static communicator finalises
// MPI when it is destroyed at exit, and only when akantu initialised MPI)
// and src/common/aka_error.cc (an uncaught exception ends in
// `std::terminate`: no static destructors, no finalisation).

//! `exit_hook_mpi` (harness = false, feature `mpi`; decision D-067):
//! one scenario per run, selected by the first
//! argument, run under `mpirun -np N` by the MPI tier
//! (`scripts/mpi/run-mpi-tier.sh`) and the campaign script
//! (`scripts/mpi/run-exit-hook-campaign.sh`). Every rank prints
//! `EXIT_HOOK <scenario> rank=<r> reached-end` just before it leaves `main`
//! (or panics / exits).
//!
//! | Scenario | What the program does | Expected (glibc Linux) |
//! |---|---|---|
//! | `forget` | `initialize`, barrier, return from `main` without `finalize` | rc 0 (the exit hook finalises MPI) |
//! | `finalize` | `initialize`, barrier, `runtime::finalize`, return | rc 0 |
//! | `hostfirst` | raw `MPI_Init_thread(FUNNELED)`, `initialize` (imported environment, D-015), raw `MPI_Finalize`, return | rc 0 (no MPI call after the host's `MPI_Finalize`) |
//! | `hostfirst-universe` | the same with rsmpi: `mpi::initialize_with_threading`, `initialize`, the `Universe` dropped at the end of `main` (`MPI_Finalize`) | rc 0 |
//! | `panic` | `initialize`; rank 0 panics, the others wait in a barrier (no deadline) | rc 101, quickly (MPI not finalised after a panic) |
//! | `err-return` | `initialize`; rank 0 returns `Err` from `main` (exit status 1), the others wait in a barrier | rc 1, quickly |
//! | `exit-nonzero` | `initialize`; rank 0 calls `std::process::exit(3)`, the others wait in a barrier | rc 3, quickly |
//! | `serial-held` | a `Serialized` world, then `initialize`; a worker holds the `Serialized` MPI lock in a blocking call when `main` returns | no hang: the hook gives up (one emergency note per rank), MPI stays unfinalised, the launcher reports it (rc 1) |
//! | `worker-exit` | `initialize`; a worker thread calls `std::process::exit(0)` | rc 1 (documented: under `Funneled` MPI cannot be finalised off its main thread) |
//!
//! On targets without `on_exit` (macOS, musl) `err-return` and
//! `exit-nonzero` finalise MPI on the exiting rank (documented residual of
//! D-067); the MPI tier runs on glibc Linux.

use std::ffi::{c_char, c_int};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use akantu::comm::lifecycle::ThreadLevel;
use akantu::comm::{Communicator, MpiOptions, mpi_lifecycle};
use akantu::runtime::{finalize, global, initialize};

unsafe extern "C" {
    fn MPI_Init_thread(
        argc: *mut c_int,
        argv: *mut *mut *mut c_char,
        required: c_int,
        provided: *mut c_int,
    ) -> c_int;
    fn MPI_Finalize() -> c_int;
}

/// `initialize` on this process; returns (rank, size).
fn init_akantu(name: &str) -> Result<(i32, usize), String> {
    let mut argv = vec![name.to_owned()];
    let init = initialize("", &mut argv).map_err(|e| format!("initialize: {e}"))?;
    let rt = global().read().map_err(|e| e.to_string())?;
    if !rt.communicator.is_mpi() {
        return Err("the static communicator is not the MPI world".into());
    }
    Ok((init.rank, rt.communicator.size()))
}

/// The runtime's communicator (a clone, dropped by the caller).
fn world() -> Result<Communicator, String> {
    Ok(global()
        .read()
        .map_err(|e| e.to_string())?
        .communicator
        .clone())
}

fn reached(scenario: &str, rank: i32) {
    println!("EXIT_HOOK {scenario} rank={rank} reached-end");
}

/// Rank 0 ends abnormally through `end`; the other ranks wait in a barrier
/// of the runtime's communicator (default options: no deadline), which
/// only the launcher's teardown ends.
fn rank0_ends(
    scenario: &str,
    end: impl FnOnce() -> Result<ExitCode, String>,
) -> Result<ExitCode, String> {
    let (rank, _) = init_akantu(scenario)?;
    let c = world()?;
    reached(scenario, rank);
    if rank == 0 {
        drop(c);
        return end();
    }
    let r = c.barrier();
    Err(format!("rank {rank}: the barrier returned {r:?}"))
}

fn run(scenario: &str) -> Result<ExitCode, String> {
    match scenario {
        "forget" | "finalize" => {
            let (rank, _) = init_akantu(scenario)?;
            world()?.barrier().map_err(|e| format!("barrier: {e}"))?;
            if scenario == "finalize" {
                finalize().map_err(|e| format!("finalize: {e}"))?;
            }
            reached(scenario, rank);
            Ok(ExitCode::SUCCESS)
        }
        "hostfirst" => {
            let mut provided: c_int = -1;
            // SAFETY: the first MPI call of the process, on the main thread;
            // NULL argc/argv are allowed (MPI-4.1 §11.2.1); `provided` is a
            // valid out-pointer. 1 = MPI_THREAD_FUNNELED (Open MPI's value),
            // the level akantu's default options require.
            let rc = unsafe {
                MPI_Init_thread(
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    1,
                    &raw mut provided,
                )
            };
            if rc != 0 || provided < 1 {
                return Err(format!("MPI_Init_thread rc={rc} provided={provided}"));
            }
            let (rank, _) = init_akantu(scenario)?;
            world()?.barrier().map_err(|e| format!("barrier: {e}"))?;
            // the host finalises the MPI it initialised, then returns
            // SAFETY: MPI was initialised by this host on this (main) thread;
            // every akantu communication has completed (the barrier above).
            let rc = unsafe { MPI_Finalize() };
            if rc != 0 {
                return Err(format!("MPI_Finalize rc={rc}"));
            }
            reached(scenario, rank);
            Ok(ExitCode::SUCCESS)
        }
        "hostfirst-universe" => {
            let (universe, provided) = mpi::initialize_with_threading(mpi::Threading::Funneled)
                .ok_or("rsmpi: MPI already initialised")?;
            if provided == mpi::Threading::Single {
                return Err(format!("rsmpi provided {provided:?}"));
            }
            let (rank, _) = init_akantu(scenario)?;
            world()?.barrier().map_err(|e| format!("barrier: {e}"))?;
            reached(scenario, rank);
            // `universe` is dropped at the end of `main` (rsmpi finalises
            // MPI there, before the C library's exit handlers run)
            drop(universe);
            Ok(ExitCode::SUCCESS)
        }
        "panic" => rank0_ends(scenario, || {
            panic!("exit_hook_mpi: rank 0 fails after initialize");
        }),
        "err-return" => rank0_ends(scenario, || Err("rank 0 returns an error from main".into())),
        "exit-nonzero" => rank0_ends(scenario, || std::process::exit(3)),
        "serial-held" => {
            // the world at `Serialized` first (akantu owns MPI); leaked so
            // that its drop at the end of `main` (a blocking MPI_Comm_free
            // under the held lock) does not stop `main` before the exit
            let serialized = Communicator::mpi_world(MpiOptions {
                thread_level: ThreadLevel::Serialized,
                ..MpiOptions::default()
            })
            .map_err(|e| format!("Serialized world: {e}"))?;
            std::mem::forget(serialized);
            let (rank, _) = init_akantu(scenario)?;
            world()?.barrier().map_err(|e| format!("barrier: {e}"))?;
            let (held_tx, held_rx) = mpsc::channel::<()>();
            std::thread::spawn(move || {
                // a worker inside a blocking call under the process MPI
                // lock (as a stalled native reduction would be), holding a
                // lifecycle guard
                let env = match mpi_lifecycle().initialize(ThreadLevel::Serialized) {
                    Ok(env) => env,
                    Err(e) => {
                        println!("EXIT_HOOK serial-held worker: {e}");
                        return;
                    }
                };
                let _ = env.policy().call(|| {
                    let _ = held_tx.send(());
                    // far longer than the run: the process ends meanwhile
                    std::thread::sleep(Duration::from_secs(3600));
                });
            });
            held_rx
                .recv_timeout(Duration::from_secs(10))
                .map_err(|e| format!("worker did not take the lock: {e}"))?;
            reached(scenario, rank);
            Ok(ExitCode::SUCCESS)
        }
        "worker-exit" => {
            let (rank, _) = init_akantu(scenario)?;
            world()?.barrier().map_err(|e| format!("barrier: {e}"))?;
            reached(scenario, rank);
            std::thread::spawn(|| std::process::exit(0));
            std::thread::sleep(Duration::from_secs(30));
            Err("the worker's exit did not end the process".into())
        }
        other => Err(format!("unknown scenario {other:?}")),
    }
}

fn main() -> Result<ExitCode, String> {
    let scenario = std::env::args().nth(1).unwrap_or_else(|| "forget".into());
    run(&scenario)
}
