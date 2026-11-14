// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common.cc (initialize,
// finalize, readInputFile), src/common/aka_extern.cc (static_parser,
// static_argparser), src/synchronizer/communicator.cc
// (getStaticCommunicator).

//! Process-global runtime (decision record D-023).
//!
//! Upstream keeps global singletons (static parser filled by
//! `initialize("material.dat", argc, argv)`/`parseInput`, static argument
//! parser, static communicator, factories). Library code in akantu-rs takes
//! these as explicit parameters; this module provides the global defaults
//! for API parity (C++-style programs and the Python module), behind a
//! `RwLock` so it is thread-safe.
//!
//! # `initialize` (row CORE-031)
//!
//! [`initialize`] is upstream `akantu::initialize(input_file, argc, argv)`
//! (`aka_common.cc:62-125`): it installs the static communicator
//! ([`static_communicator`]: the MPI world with feature `mpi`, as upstream's
//! `Communicator::getStaticCommunicator(argc, argv)` in a parallel build),
//! registers the `--aka_*` options in the existing global argument
//! parser ([`Runtime::arguments`], upstream `static_argparser`: options the
//! application registered before are parsed by the same call) and parses
//! `argv` removing what it consumed, reads the input file (`--aka_input_file`
//! wins over the `input_file` argument), seeds the process-global
//! `RandomGenerator<Idx>` engine with
//! [`initial_seed`] (`AKA_SEED`
//! (glibc `atol`) > `--aka_seed` > the input file's root-scope `seed`
//! parameter (C++ stream conversion to `long`) > `time(nullptr)`, times
//! `prank + 1`) and configures the debugger as upstream (rows CORE-033,
//! CORE-044): parallel context of the rank and level
//! `dblError` before the parse (so `--help` leaves both, and messages
//! during the input-file read are filtered at `dblError`), the backtrace
//! flag (`--aka_print_backtrace`) before the input file is read, and after
//! seeding the level `--aka_debug_level`, then `AKA_DEBUG_LEVEL` with glibc
//! `atoi` ([`akantu_core::debug::configure_initialize`]); the `log` records
//! of the readers and the parser reach the debugger
//! ([`akantu_core::debug::install_log_bridge`], installed by the first
//! call unless the host installed a logger). It acts on the calling
//! thread's debugger: the process default, or the thread's scope (an
//! in-process rank, [`akantu_core::debug::rank_scope`]). The runtime keeps
//! no copy of these settings ([`Runtime::debug_level`],
//! [`Runtime::print_backtrace`] read the debugger). [`initialize_with`] takes the
//! process inputs (environment variables, wall-clock time, communicator)
//! explicitly so that every rank of a run can be replayed in one process
//! (`tests/initialize_matches_oracle.rs`, fixtures `fixtures/core/initialize`).
//!
//! A second call behaves as upstream's: the options are declared again in
//! the same parser, so an option without default given to the first call
//! (`--aka_seed`) keeps its value unless given again (upstream
//! `success_parsed`), options with a default return to it, and an
//! application option parsed by the first call stays parsed (giving it
//! again is upstream's "already present" error).
//!
//! Adaptations (`docs/UPSTREAM_DEFECTS.md` UD-498, UD-499):
//! * `-h/--help` is reported as [`ParseOutcome::HelpRequested`] instead of
//!   printing the help and exiting the process (UD-498). It is returned
//!   right after the parse, where upstream exits: the input file is not
//!   read, the global engine is not seeded and the debug level stays at
//!   `dblError` ([`DEBUG_LEVEL_ERROR`], set before the parse as upstream).
//!   The runtime keeps the installed communicator and the parsed arguments
//!   (help flag included, `--aka_*` options removed from `argv`): the
//!   caller prints [`ArgumentParser::help_string`] on rank 0 and exits;
//!   a later `initialize` would report the help request again;
//! * `Tag::setMaxTag` has no counterpart: communication tags are per
//!   communicator (UD-499);
//! * MPI finalisation (UD-499, decision D-067).
//!   Upstream registers `std::atexit(finalize)` (an empty function) and
//!   finalises MPI when the static communicator is destroyed at exit, if
//!   akantu initialised MPI. Rust statics are never dropped, so the first
//!   [`initialize_with`] that installs an MPI communicator registers, once
//!   per process, the release of the runtime's communicator with the
//!   process-exit hook of `akantu-comm` (`akantu_comm::release_at_exit`),
//!   which then completes the lifecycle's finalisation on MPI's main
//!   thread (in the order of `docs/ARCHITECTURE.md`: models, native
//!   backends, requests, communicators, `MPI_Finalize`). A program that returns from `main`
//!   without calling [`finalize`] therefore ends normally, as upstream's.
//!   The hook finalises MPI only after a normal end (D-067): not
//!   after a panic (a panicking `main`, or a panic on any thread since
//!   `initialize`, caught or not) and, on glibc Linux, where it is
//!   registered with `on_exit`, not for a non-zero exit status (`Err`
//!   from `main`, `std::process::exit(3)`), as upstream's uncaught
//!   exception ends in `std::terminate` without finalising: the launcher
//!   sees the rank's abnormal end and tears the job down instead of
//!   waiting in `MPI_Finalize` for ranks that never come. Two deliberate
//!   differences from upstream (D-067): a
//!   panic that the program catches and handles still counts, so a
//!   run that then ends normally leaves MPI unfinalised and the launcher
//!   reports the rank (job status ≠ 0); such a program calls
//!   [`finalize`] explicitly, which does not consult the panic flag; and
//!   upstream's static destructor does finalise after `exit(n)` with
//!   n ≠ 0, which the port does not on glibc. It makes no
//!   MPI call once MPI is finalised (D-067): a host that imported
//!   akantu and finalised MPI itself before exiting gets nothing from the
//!   hook. It never blocks (D-067): if the runtime lock, the
//!   lifecycle lock, a destruction drain or the `Serialized` MPI lock is
//!   held by another thread, MPI is left unfinalised with one emergency
//!   note. MPI also stays unfinalised (the launcher reports an abnormal
//!   exit) if the process is terminated from another thread than MPI's
//!   main thread under `Single`/`Funneled`, or if objects holding a
//!   communicator are never dropped (leaked, owned by a static, or alive
//!   when `std::process::exit` is called). On targets without `on_exit`
//!   (macOS, musl) a non-zero `exit` without a panic still finalises
//!   (documented residual, as upstream's static destructor after
//!   `exit(1)`). The explicit [`finalize`] remains available and the hook
//!   is then a no-op. Unlike upstream's no-op `finalize()`, [`finalize`]
//!   finalises an akantu-owned MPI environment, so an [`initialize`] after
//!   it is refused with a clear error (MPI cannot be initialised twice in
//!   a process).

use std::path::Path;
#[cfg(feature = "mpi")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use akantu_comm::Communicator;
use akantu_core::debug::{self, DebugLevel};
use akantu_core::random::{IdxSlot, RandomGenerator, initial_seed};
use akantu_core::{Error, Rank, Result};
use akantu_parser::cli::{ArgumentParser, ParseFlags, ParseOutcome, register_akantu_options};
use akantu_parser::{Parser, SearchScope};

/// Upstream `dblError`, the debug level `initialize` sets before parsing.
pub const DEBUG_LEVEL_ERROR: i64 = 0;

/// Global state.
#[derive(Debug, Default)]
pub struct Runtime {
    /// Parsed input file (upstream `getStaticParser()`).
    pub parser: Parser,
    /// Command-line options (upstream `getStaticArgumentParser()`).
    pub arguments: ArgumentParser,
    /// World communicator (upstream `Communicator::getStaticCommunicator()`).
    pub communicator: Communicator,
}

impl Runtime {
    /// Upstream `debug::getDebugLevel()`: the level of the debugger in
    /// effect on the calling thread ([`akantu_core::debug`], the single
    /// source of truth; `dblWarning` = 4 by default).
    #[must_use]
    pub fn debug_level(&self) -> i64 {
        i64::from(debug::debug_level().0)
    }

    /// Upstream `debug::debugger.printBacktrace()` on the calling thread.
    #[must_use]
    pub fn print_backtrace(&self) -> bool {
        debug::print_backtrace_enabled()
    }
}

static RUNTIME: OnceLock<RwLock<Runtime>> = OnceLock::new();

/// Access the global runtime (created on first use with a serial
/// communicator and an empty parser).
pub fn global() -> &'static RwLock<Runtime> {
    RUNTIME.get_or_init(|| RwLock::new(Runtime::default()))
}

/// Read an input file into the global parser (upstream `readInputFile` /
/// Python `parseInput`): the member parse of the existing static parser, so
/// — as upstream — the tree is cleared before the file is opened, the
/// permissive flag carries over between files unless a file sets
/// `permissive_parser`, and on error the partially filled tree is kept.
pub fn parse_input(path: impl AsRef<Path>) -> Result<()> {
    let mut rt = global()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    rt.parser.parse(path)
}

/// The process inputs upstream `initialize` reads besides `argv`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessInputs {
    /// Value of the environment variable `AKA_SEED`, if set.
    pub env_aka_seed: Option<String>,
    /// Value of the environment variable `AKA_DEBUG_LEVEL`, if set.
    pub env_aka_debug_level: Option<String>,
    /// `time(nullptr)` (seconds since the Unix epoch) for the seed fallback.
    pub time_now: i64,
}

impl ProcessInputs {
    /// The inputs of this process: its environment and the current time.
    /// Non-UTF-8 environment values are read lossily, which does not change
    /// what glibc `atol`/`atoi` extract (leading ASCII white space, sign and
    /// digits).
    #[must_use]
    pub fn from_process() -> Self {
        let var = |name: &str| std::env::var_os(name).map(|v| v.to_string_lossy().into_owned());
        let time_now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
            Err(e) => i64::try_from(e.duration().as_secs()).map_or(i64::MIN, |s| -s),
        };
        Self {
            env_aka_seed: var("AKA_SEED"),
            env_aka_debug_level: var("AKA_DEBUG_LEVEL"),
            time_now,
        }
    }
}

/// What [`initialize`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Initialized {
    /// Seed given to `RandomGenerator<Idx>` (after the rank factor); with
    /// [`ParseOutcome::HelpRequested`] nothing was seeded and this is the
    /// engine's current seed (`RandomGenerator<Idx>::seed()`).
    pub seed: i64,
    /// Rank of the communicator.
    pub rank: Rank,
    /// Input file read (empty: none, or `--help` was given).
    pub input_file: String,
    /// Debug level set (upstream `debug::getDebugLevel()` afterwards: the
    /// calling thread's debugger, a 32-bit level as upstream's
    /// `DebugLevel(long)`).
    pub debug_level: i64,
    /// Whether `-h/--help` was given.
    pub outcome: ParseOutcome,
}

/// Upstream `Communicator::getStaticCommunicator(argc, argv)`: the
/// communicator [`initialize`] installs. With feature `mpi` it is the MPI
/// world — a duplicate of `MPI_COMM_WORLD`, MPI being initialised by akantu
/// unless the host did it first (D-015) — created on first use and reused
/// while the runtime holds it (a later `initialize` keeps the same
/// communicator, as upstream's static one); without `mpi` it is the serial
/// communicator (upstream's build without MPI).
///
/// # Errors
/// MPI initialisation or duplication errors (feature `mpi`).
pub fn static_communicator() -> Result<Communicator> {
    #[cfg(feature = "mpi")]
    {
        let current = global()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .communicator
            .clone();
        if current.is_mpi() {
            return Ok(current);
        }
        Communicator::mpi_world(akantu_comm::MpiOptions::default())
    }
    #[cfg(not(feature = "mpi"))]
    {
        Ok(Communicator::serial())
    }
}

/// Upstream `akantu::initialize(input_file, argc, argv)` for this process:
/// [`initialize_with`] with [`ProcessInputs::from_process`] and the
/// [`static_communicator`] (the MPI world with feature `mpi`, so every rank
/// gets its own seed factor `rank + 1`, as upstream). `argv[0]` is the
/// program name; the parsed `--aka_*` arguments are removed from `argv`.
///
/// # Errors
/// Argument errors (unknown types, missing values) and input-file errors,
/// as upstream throws them; MPI errors from [`static_communicator`].
pub fn initialize(input_file: &str, argv: &mut Vec<String>) -> Result<Initialized> {
    refuse_after_finalize()?;
    let communicator = static_communicator()?;
    initialize_with(
        input_file,
        argv,
        &ProcessInputs::from_process(),
        communicator,
    )
}

/// An `initialize` after [`finalize`] finalised the MPI environment akantu
/// owned is an error (feature `mpi`; MPI cannot be initialised twice).
fn refuse_after_finalize() -> Result<()> {
    #[cfg(feature = "mpi")]
    if akantu_comm::mpi_lifecycle().is_finalized() {
        return Err(akantu_core::Error::Communication(
            "initialize after finalize: runtime::finalize() finalised the MPI environment \
             akantu owned, and MPI cannot be initialised again in the same process \
             (upstream's finalize() is a no-op; UD-499)"
                .into(),
        ));
    }
    Ok(())
}

/// The communicator part of [`initialize`], for hosts that parse their own
/// options (the Python module's `__initialize`, which mirrors the rest of
/// upstream's `initialize(argc, argv)` itself, D-101): installs
/// the [`static_communicator`] in the runtime — with feature `mpi` the MPI
/// world, MPI being initialised by akantu unless the host (mpi4py) did it
/// first — registers its release with the process-exit hook once (as
/// [`initialize_with`] does, D-067) and sets the debugger's parallel
/// context. Repeated calls keep the installed communicator. Returns it.
///
/// # Errors
/// As [`initialize`]: MPI initialisation errors, and an MPI environment
/// akantu already finalised.
pub fn install_static_communicator() -> Result<Communicator> {
    refuse_after_finalize()?;
    let communicator = static_communicator()?;
    let mut rt = global()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    #[cfg(feature = "mpi")]
    if communicator.is_mpi() && !EXIT_RELEASE_REGISTERED.load(Ordering::Acquire) {
        akantu_comm::release_at_exit(release_static_communicator)?;
        EXIT_RELEASE_REGISTERED.store(true, Ordering::Release);
    }
    rt.communicator = communicator.clone();
    debug::set_parallel_context(communicator.rank(), communicator.size());
    Ok(communicator)
}

/// Upstream `akantu::finalize()` together with the destruction of the
/// static communicator at exit (UD-499): the runtime's communicator is
/// replaced by the serial one, releasing its handle (so is the process-wide
/// world of the PETSc DOF managers of serial meshes,
/// `akantu_solver::model_solver::release_petsc_world`), and — with feature
/// `mpi` — an akantu-owned MPI environment whose last handle this was is
/// finalised on MPI's main thread (`Lifecycle::shutdown`, D-023). Calling
/// it is optional after a normal end: the exit hook that [`initialize`]
/// registers does the same at process exit (see the module documentation;
/// not after a panic, nor — on glibc Linux — for a non-zero exit status,
/// D-067). **It is required after a caught panic:** the exit hook does
/// nothing once any panic happened since [`initialize`], while this
/// function ignores that flag (a program that handled a panic and ends
/// normally calls it to end with status 0). It is idempotent;
/// objects still holding a communicator keep MPI alive until they are
/// dropped. An [`initialize`] after it is an error once MPI was finalised.
///
/// # Errors
/// MPI finalisation errors (feature `mpi`; e.g. called off MPI's main
/// thread while a finalisation is pending); `Error::Communication` when
/// another thread is creating the process-wide world of the PETSc DOF
/// managers of serial meshes at that moment: that world could not be
/// released, so MPI is **not** finalised (the runtime's
/// communicator is released; call `finalize` again once that thread is
/// done).
pub fn finalize() -> Result<()> {
    let released = std::mem::replace(
        &mut global()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .communicator,
        Communicator::serial(),
    );
    drop(released);
    // The process-wide world of the PETSc DOF managers of serial meshes
    // (akantu-solver's serial-mesh rule): released here
    // as the runtime's communicator (models still holding it keep MPI
    // alive until they are dropped).
    if !akantu_solver::model_solver::release_petsc_world() {
        return Err(Error::Communication(
            "runtime::finalize: another thread is creating the process-wide MPI world of the \
             PETSc DOF managers of serial meshes; it was not released and MPI was not finalised \
             (call finalize again once that thread is done)"
                .into(),
        ));
    }
    #[cfg(feature = "mpi")]
    akantu_comm::mpi_lifecycle().shutdown()?;
    Ok(())
}

/// The runtime's release callback of the process-exit hook (registered by
/// [`initialize_with`] for an MPI communicator; upstream: the static
/// communicator's destructor finalises MPI at exit,
/// D-067): replaces the runtime's communicator by the serial one and drops
/// it. The `akantu-comm` hook calls it only after a normal end, while MPI
/// is not finalised, in exit mode, and then completes the lifecycle's
/// finalisation. It runs after the exiting thread's thread-local
/// destructors: it never blocks on the runtime lock (another thread holding
/// it means the runtime is in use: it returns `false` and the hook leaves
/// MPI unfinalised, D-067 (3)), never calls `std::thread::current()` and
/// prints nothing.
#[cfg(feature = "mpi")]
fn release_static_communicator() -> bool {
    let released = match global().try_write() {
        Ok(mut rt) => std::mem::replace(&mut rt.communicator, Communicator::serial()),
        Err(std::sync::TryLockError::Poisoned(p)) => {
            std::mem::replace(&mut p.into_inner().communicator, Communicator::serial())
        }
        Err(std::sync::TryLockError::WouldBlock) => return false,
    };
    drop(released);
    true
}

/// [`release_static_communicator`] is registered with the exit hook (set
/// under the runtime's write lock after a successful registration, so a
/// refused registration is tried again by the next [`initialize_with`]).
#[cfg(feature = "mpi")]
static EXIT_RELEASE_REGISTERED: AtomicBool = AtomicBool::new(false);

/// Upstream `akantu::initialize(input_file, argc, argv)` with explicit
/// process inputs and communicator (see the module documentation for the
/// sequence, the seed policy, repeated calls and `--help`).
///
/// # Errors
/// Argument errors and input-file errors; the runtime keeps whatever was
/// set before the failing step (upstream throws at the same points).
pub fn initialize_with(
    input_file: &str,
    argv: &mut Vec<String>,
    inputs: &ProcessInputs,
    communicator: Communicator,
) -> Result<Initialized> {
    akantu_core::aka_debug_in!();
    // the readers' and the parser's `log` records reach the debugger
    // (upstream: their AKANTU_DEBUG_WARNINGs); already installed by an
    // earlier call, or a logger of the host: kept as it is
    let _ = debug::install_log_bridge();
    let mut rt = global()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let rank = communicator.rank();
    let size = communicator.size();
    // upstream: the static communicator (destroyed at exit, finalising
    // MPI) and `std::atexit(finalize)`; registered before the parse, so the
    // `--help` path (the caller exits) is covered too. Once per process:
    // the write lock serialises concurrent calls.
    #[cfg(feature = "mpi")]
    if communicator.is_mpi() && !EXIT_RELEASE_REGISTERED.load(Ordering::Acquire) {
        akantu_comm::release_at_exit(release_static_communicator)?;
        EXIT_RELEASE_REGISTERED.store(true, Ordering::Release);
    }
    rt.communicator = communicator;
    // upstream: setParallelContext and dblError before the parse
    debug::set_parallel_context(rank, size);
    debug::set_debug_level(DebugLevel(i32::try_from(DEBUG_LEVEL_ERROR).unwrap_or(0)));

    // the existing static parser (upstream `static_argparser`): earlier
    // registrations survive, repeated calls re-declare the options
    rt.arguments
        .set_parallel_context(rank, i32::try_from(size).unwrap_or(i32::MAX));
    register_akantu_options(&mut rt.arguments)?;
    let outcome = rt
        .arguments
        .parse_argv(argv, ParseFlags::REMOVE_PARSED, true)?;
    if outcome == ParseOutcome::HelpRequested {
        // upstream `_exit()`s inside `parse`: nothing below runs (UD-498)
        return Ok(Initialized {
            seed: RandomGenerator::<IdxSlot>::get_seed(),
            rank,
            input_file: String::new(),
            debug_level: rt.debug_level(),
            outcome,
        });
    }

    let mut infile = rt.arguments.get_string("aka_input_file")?;
    if infile.is_empty() {
        infile = input_file.to_owned();
    }
    let print_backtrace = rt.arguments.get_bool("aka_print_backtrace")?;
    debug::set_print_backtrace(print_backtrace);
    if !infile.is_empty() {
        rt.parser.parse(&infile)?;
    }

    let cli_seed = if rt.arguments.has("aka_seed") {
        Some(rt.arguments.get_int("aka_seed")?)
    } else {
        None
    };
    // upstream: static_parser.getParameter("seed", time(nullptr),
    // _ppsc_current_scope) — only consulted when neither AKA_SEED nor
    // --aka_seed is given
    let file_seed = if inputs.env_aka_seed.is_none() && cli_seed.is_none() {
        Some(rt.parser.root.get_parameter_or::<i64>(
            "seed",
            inputs.time_now,
            SearchScope::Current,
        )?)
    } else {
        None
    };
    let seed = initial_seed(
        inputs.env_aka_seed.as_deref(),
        cli_seed,
        file_seed,
        inputs.time_now,
        rank,
    );
    RandomGenerator::<IdxSlot>::seed(seed);

    // `--aka_debug_level`, then `AKA_DEBUG_LEVEL` (glibc `atoi`); the
    // context, `dblError` and the backtrace flag are set again, unchanged
    let cli_level = rt.arguments.get_int("aka_debug_level")?;
    debug::configure_initialize(
        rank,
        size,
        cli_level,
        inputs.env_aka_debug_level.as_deref(),
        print_backtrace,
    );
    akantu_core::aka_debug_info!("Random seed set to {seed}");
    let initialized = Initialized {
        seed,
        rank,
        input_file: infile,
        debug_level: rt.debug_level(),
        outcome,
    };
    akantu_core::aka_debug_out!();
    Ok(initialized)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sequential `parse_input` calls keep the permissive
    /// flag like upstream's static parser (C++ prints `true,true`).
    #[test]
    fn parse_input_keeps_the_permissive_flag() {
        let dir = std::env::temp_dir();
        let first = dir.join(format!("akantu-rs-{}-rt-first.dat", std::process::id()));
        let second = dir.join(format!("akantu-rs-{}-rt-second.dat", std::process::id()));
        std::fs::write(&first, "permissive_parser = true\na = 1\n").unwrap();
        std::fs::write(&second, "a = 2\n").unwrap();
        parse_input(&first).unwrap();
        let after_first = global().read().unwrap().parser.is_permissive();
        parse_input(&second).unwrap();
        let rt = global().read().unwrap();
        assert_eq!((after_first, rt.parser.is_permissive()), (true, true));
        assert!(
            rt.parser
                .root
                .parameter("a")
                .is_some_and(|a| a.value == "2")
        );
        drop(rt);
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
    }

    #[test]
    fn global_defaults() {
        let rt = global().read().unwrap();
        assert_eq!(rt.communicator.size(), 1);
        assert_eq!(rt.parser.root.sub_sections.len(), 0);
    }
}
