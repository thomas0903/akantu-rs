// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): real-MPI check of
// `akantu::runtime::initialize` against upstream src/common/aka_common.cc:62-125
// (`Communicator::getStaticCommunicator(argc, argv)`, then
// `seed *= (comm.whoAmI() + 1)`) and src/synchronizer/communicator.cc:65-86
// (one static communicator per process).

//! `initialize_mpi` (row CORE-031; harness = false,
//! feature `mpi`): run under `mpirun -np N` (docs/TESTING.md). Every rank calls [`akantu::runtime::initialize`]
//! with `--aka_seed 7` and checks, against upstream's derivation:
//!
//! * the installed communicator is the MPI world (`size` = N, rank = MPI
//!   rank);
//! * the seed is `base · (rank + 1)` with `base` = `atol($AKA_SEED)` if set
//!   (glibc `atol`, environment wins), else 7 — so ranks get distinct seeds;
//!   the global engine's next value is that of a fresh `minstd_rand0`
//!   seeded with it;
//! * the seeds gathered over the world are pairwise distinct for N > 1;
//! * a second `initialize` reuses the same static communicator and keeps
//!   `--aka_seed` (upstream `success_parsed`);
//! * [`akantu::runtime::finalize`] releases the communicator and akantu
//!   finalises the MPI environment it owns.
//!
//! The first argument selects the end of the run:
//! * `finalize` (default): the explicit [`akantu::runtime::finalize`] as
//!   above;
//! * `forget`: no `finalize` — the program returns from `main` holding the
//!   static MPI communicator, as an upstream program may; the exit hook
//!   registered by `initialize` must finalise MPI, so `mpirun` exits 0
//!   (without the hook it reports "exiting improperly", rc 1);
//! * `reinit`: `finalize`, then `initialize` again, which must be refused
//!   with the clear UD-499 error (MPI cannot be initialised twice), and
//!   the process still ends normally.
//!
//! Prints `INITIALIZE_MPI PASS mode=<m> rank=<r> size=<n> seed=<s>` per
//! rank and exits 0, or prints the failure and exits 1.

use std::process::ExitCode;

use akantu::core::random::{IdxSlot, MinstdRand0, RandomGenerator, atol};
use akantu::runtime::{finalize, global, initialize};

/// How the run ends (first argument).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Finalize,
    Forget,
    Reinit,
}

fn mode() -> Result<Mode, String> {
    match std::env::args().nth(1).as_deref() {
        None | Some("finalize") => Ok(Mode::Finalize),
        Some("forget") => Ok(Mode::Forget),
        Some("reinit") => Ok(Mode::Reinit),
        Some(other) => Err(format!("unknown mode {other:?}")),
    }
}

macro_rules! check {
    ($cond:expr, $($fmt:tt)+) => {
        if !$cond {
            return Err(format!("{}:{}: {}", file!(), line!(), format!($($fmt)+)));
        }
    };
}

fn run() -> Result<String, String> {
    let mode = mode()?;
    let base = std::env::var("AKA_SEED").map_or(7, |s| atol(&s));
    let mut argv = vec![
        "initialize_mpi".to_owned(),
        "--aka_seed".to_owned(),
        "7".to_owned(),
    ];
    let init = initialize("", &mut argv).map_err(|e| format!("initialize: {e}"))?;
    let (rank, size, is_mpi) = {
        let rt = global().read().map_err(|e| e.to_string())?;
        (
            rt.communicator.rank(),
            rt.communicator.size(),
            rt.communicator.is_mpi(),
        )
    };
    check!(is_mpi, "the static communicator is not the MPI world");
    if let Ok(n) = std::env::var("OMPI_COMM_WORLD_SIZE") {
        check!(n == size.to_string(), "size {size}, mpirun -np {n}");
    }
    check!(
        init.rank == rank,
        "Initialized.rank {} vs {rank}",
        init.rank
    );
    check!(argv == ["initialize_mpi"], "argv not consumed: {argv:?}");
    let factor = i64::from(rank) + 1;
    let expected = base * factor;
    check!(
        init.seed == expected,
        "rank {rank}: seed {} (upstream: {base} * ({rank} + 1) = {expected})",
        init.seed
    );
    let mut replica = MinstdRand0::default();
    replica.seed_long(expected);
    let next = RandomGenerator::<IdxSlot>::new().next_u32();
    check!(
        next == replica.next_u32(),
        "rank {rank}: the global engine was not seeded with {expected}"
    );
    let all = global()
        .read()
        .map_err(|e| e.to_string())?
        .communicator
        .all_gather(&[init.seed])
        .map_err(|e| format!("all_gather: {e}"))?;
    let mut distinct = all.clone();
    distinct.sort_unstable();
    distinct.dedup();
    check!(
        distinct.len() == size,
        "seeds over the ranks are not distinct: {all:?}"
    );

    // a second call: same static communicator, `--aka_seed` kept
    let mut argv = vec!["initialize_mpi".to_owned()];
    let again = initialize("", &mut argv).map_err(|e| format!("second initialize: {e}"))?;
    check!(
        again.seed == expected,
        "second call: seed {} (upstream keeps --aka_seed: {expected})",
        again.seed
    );
    {
        let rt = global().read().map_err(|e| e.to_string())?;
        check!(
            rt.communicator.is_mpi() && rt.communicator.size() == size,
            "second call changed the static communicator"
        );
    }

    check!(
        akantu::comm::exit_hook_registered(),
        "initialize did not register the MPI finalisation exit hook"
    );
    let pass = format!(
        "INITIALIZE_MPI PASS mode={mode:?} rank={rank} size={size} seed={}",
        init.seed
    );
    if mode == Mode::Forget {
        // return from `main` holding the static communicator: the exit hook
        // finalises MPI (checked by mpirun's exit status)
        return Ok(pass);
    }

    finalize().map_err(|e| format!("finalize: {e}"))?;
    {
        let rt = global().read().map_err(|e| e.to_string())?;
        check!(
            !rt.communicator.is_mpi(),
            "finalize kept the MPI communicator"
        );
    }
    check!(
        akantu::comm::mpi_lifecycle().is_finalized(),
        "akantu did not finalise the MPI environment it owns"
    );
    finalize().map_err(|e| format!("second finalize (idempotent): {e}"))?;
    if mode == Mode::Reinit {
        let mut argv = vec!["initialize_mpi".to_owned()];
        match initialize("", &mut argv) {
            Ok(_) => return Err("initialize after finalize succeeded".into()),
            Err(e) => {
                let msg = e.to_string();
                check!(
                    msg.contains("initialize after finalize") && msg.contains("UD-499"),
                    "initialize after finalize: unclear error {msg:?}"
                );
            }
        }
        let rt = global().read().map_err(|e| e.to_string())?;
        check!(
            !rt.communicator.is_mpi(),
            "the refused initialize installed a communicator"
        );
    }
    Ok(pass)
}

fn main() -> ExitCode {
    match run() {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("INITIALIZE_MPI FAIL {e}");
            ExitCode::FAILURE
        }
    }
}
