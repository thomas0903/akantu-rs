// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): src/common/aka_common.cc
// (initialize, lines 62-125: setParallelContext, dblError before the parse,
// printBacktrace(--aka_print_backtrace) before readInputFile, the level from
// --aka_debug_level then AKA_DEBUG_LEVEL, AKANTU_DEBUG_IN / _INFO("Random
// seed set to ...") / _OUT) and src/common/aka_error.cc (one debugger per
// process).

//! `initialize_debug` (rows CORE-033 / CORE-044):
//! `runtime::initialize_with` configures the debugger of the
//! calling thread as upstream `initialize` does — and nothing else holds a
//! copy of the level — and each in-process rank of a local world gets its
//! own debugger. Own test binary: it changes the process-wide runtime and
//! the `log` logger.

use std::sync::Mutex;
use std::time::Duration;

use akantu::comm::{Communicator, run_local};
use akantu::core::debug::{self, DebugLevel, Debugger, DebuggerScope};
use akantu::parser::cli::ParseOutcome;
use akantu::runtime::{ProcessInputs, Runtime, global, initialize_with};

/// The runtime is process-global: one `initialize` at a time.
static RUNTIME: Mutex<()> = Mutex::new(());

/// A fresh runtime (upstream: a new process; the static argument parser
/// keeps what an earlier `initialize` parsed, `--help` included).
fn fresh_runtime() {
    *global()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Runtime::default();
}

fn argv(args: &[&str]) -> Vec<String> {
    std::iter::once("prog")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect()
}

fn inputs(env_level: Option<&str>) -> ProcessInputs {
    ProcessInputs {
        env_aka_seed: None,
        env_aka_debug_level: env_level.map(str::to_owned),
        time_now: 1_000,
    }
}

/// (prefix + message) of every line of `text`, locations and time stamps
/// dropped; the parallel context is checked separately.
fn messages(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| {
            let (_, rest) = l.split_once("} ").unwrap();
            rest.rsplit_once(" (").map_or(rest, |(m, _)| m).to_owned()
        })
        .collect()
}

#[test]
fn initialize_configures_the_calling_threads_debugger_as_upstream() {
    let _g = RUNTIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    fresh_runtime();
    let mut d = Debugger::new();
    let out = d.set_out_buffer();
    let _scope = DebuggerScope::new(d);
    let pid = std::process::id();

    // --aka_debug_level 5 and --aka_print_backtrace: dblInfo afterwards, the
    // flag set, the info line printed; dblIn (6) was not reached at entry
    let mut a = argv(&[
        "--aka_debug_level",
        "5",
        "--aka_print_backtrace",
        "--aka_seed",
        "3",
    ]);
    let init = initialize_with("", &mut a, &inputs(None), Communicator::serial()).unwrap();
    assert_eq!((init.debug_level, init.seed), (5, 3));
    assert_eq!(debug::debug_level(), DebugLevel::INFO);
    assert!(debug::print_backtrace_enabled());
    assert_eq!(global().read().unwrap().debug_level(), 5);
    assert!(global().read().unwrap().print_backtrace());
    assert_eq!(
        debug::current().parallel_context(),
        format!("<{pid}>[R0|S1] ")
    );
    let text = out.take();
    assert_eq!(messages(&text), ["--- Random seed set to 3"], "{text}");
    assert!(
        text.lines()
            .all(|l| l.starts_with(&format!("<{pid}>[R0|S1] {{")))
    );
    assert!(
        text.contains("(initialize_with(): crates/akantu/src/runtime.rs:"),
        "{text}"
    );

    // the `log` records of the readers and the parser reach the debugger
    log::warn!(target: "akantu_io::mesh_io", "reader warning");
    log::info!(target: "akantu_io::mesh_io", "reader info");
    log::debug!(target: "akantu_io::mesh_io", "hidden at dblInfo");
    assert_eq!(
        messages(&out.take()),
        ["/!\\ reader warning", "--- reader info"]
    );

    // AKA_DEBUG_LEVEL wins (glibc atoi: "  8xyz" → 8); no backtrace flag;
    // entry at dblInfo: no `==>`, exit at dblTrace: `<==`
    let mut a = argv(&["--aka_debug_level", "5"]);
    let init =
        initialize_with("", &mut a, &inputs(Some("  8xyz")), Communicator::serial()).unwrap();
    assert_eq!(init.debug_level, 8);
    assert_eq!(debug::debug_level(), DebugLevel::TRACE);
    assert!(!debug::print_backtrace_enabled());
    assert_eq!(
        messages(&out.take()),
        ["--- Random seed set to 3", "<== initialize_with()"]
    );

    // entry at dblTrace: `==>` first, with the context of the previous call
    let mut a = argv(&["--aka_debug_level", "6"]);
    initialize_with("", &mut a, &inputs(None), Communicator::serial()).unwrap();
    assert_eq!(
        messages(&out.take()),
        [
            "==> initialize_with()",
            "--- Random seed set to 3",
            "<== initialize_with()"
        ]
    );

    // --help: returns where upstream exits, leaving dblError and the context
    let mut a = argv(&["--help"]);
    let init = initialize_with("", &mut a, &inputs(Some("100")), Communicator::serial()).unwrap();
    assert_eq!(init.outcome, ParseOutcome::HelpRequested);
    assert_eq!(
        (init.debug_level, debug::debug_level()),
        (0, DebugLevel::ERROR)
    );
    assert_eq!(
        debug::current().parallel_context(),
        format!("<{pid}>[R0|S1] ")
    );
    let text = out.take();
    assert_eq!(messages(&text), ["==> initialize_with()"], "{text}");
}

/// With the wiring: two in-process ranks each run
/// `initialize_with` on their own communicator; each rank's debugger (the
/// scope `run_local` installs) gets its own parallel context and its own
/// `--aka_debug_level`, the caller's debugger is untouched, and the
/// messages carry the rank prefixes.
#[test]
fn local_world_ranks_initialize_their_own_debugger() {
    let _g = RUNTIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    fresh_runtime();
    let mut d = Debugger::new();
    d.set_debug_level(DebugLevel::ERROR);
    let out = d.set_out_buffer();
    let _scope = DebuggerScope::new(d);
    let pid = std::process::id();
    let per_rank = run_local(2, Duration::from_secs(20), |c| {
        let rank = c.rank();
        let level = if rank == 0 { "5" } else { "4" };
        let mut a = argv(&["--aka_debug_level", level, "--aka_seed", "7"]);
        let init = initialize_with("", &mut a, &inputs(None), c).unwrap();
        akantu::core::aka_debug_info!("rank {rank} ready");
        (
            init.seed,
            debug::debug_level(),
            debug::current().parallel_context().to_owned(),
        )
    });
    assert_eq!(
        per_rank,
        [
            (7, DebugLevel::INFO, format!("<{pid}>[R0|S2] ")),
            (14, DebugLevel::WARNING, format!("<{pid}>[R1|S2] ")),
        ]
    );
    // the caller's debugger is unchanged
    assert_eq!(debug::debug_level(), DebugLevel::ERROR);
    assert_eq!(debug::current().parallel_context(), "");
    // only rank 0 (dblInfo) printed, with its own context
    let text = out.take();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    for l in &lines {
        assert!(l.starts_with(&format!("<{pid}>[R0|S2] {{")), "{text}");
    }
    assert_eq!(
        messages(&text),
        ["--- Random seed set to 7", "--- rank 0 ready"]
    );
}
