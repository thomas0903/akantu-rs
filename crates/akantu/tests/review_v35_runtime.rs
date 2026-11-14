// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for `akantu::runtime::initialize`: upstream
// src/common/aka_common.cc:62-125 (initialize) and
// src/io/parser/cppargparse/cppargparse.cc:111-205 (addArgument),
// :221-400 (parse, `success_parsed`, `_exit` on `--help`).

//! `review_v35_runtime` (row CORE-031): each test
//! asserts the upstream behaviour derived from the reference source.
//!
//! * `vg02_*`: `initialize` registers the `--aka_*` options in the **existing**
//!   global argument parser (upstream `static_argparser`), so options the
//!   application registered before are parsed by the same call; a second
//!   call re-declares the options and keeps the first call's `--aka_seed`
//!   (upstream `success_parsed` still points to the old declaration).
//! * `vg05_*`: `--help` returns [`ParseOutcome::HelpRequested`] (upstream
//!   prints the help and exits) **before** the input file is read, the
//!   global engine is seeded or the debug level is set from the options.

use std::sync::{Mutex, MutexGuard, PoisonError};

use akantu::comm::Communicator;
use akantu::core::random::{IdxSlot, MinstdRand0, RandomGenerator};
use akantu::parser::cli::{ArgumentSpec, ArgumentType, Nargs, ParseOutcome};
use akantu::runtime::{DEBUG_LEVEL_ERROR, ProcessInputs, Runtime, global, initialize_with};

/// The runtime and the engine are process-global: one test at a time, each
/// starting from a fresh runtime (upstream: a fresh process).
static GLOBAL: Mutex<()> = Mutex::new(());

fn fresh() -> MutexGuard<'static, ()> {
    let g = GLOBAL.lock().unwrap_or_else(PoisonError::into_inner);
    *global().write().unwrap_or_else(PoisonError::into_inner) = Runtime::default();
    g
}

fn inputs(seed: Option<&str>) -> ProcessInputs {
    ProcessInputs {
        env_aka_seed: seed.map(str::to_owned),
        env_aka_debug_level: None,
        time_now: 1_700_000_000,
    }
}

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

/// Upstream `parse` calls `_exit()` on `--help` before
/// `readInputFile`: the missing input file is never opened.
#[test]
fn vg05_help_with_missing_input_file_reports_help() {
    let _g = fresh();
    let mut a = argv(&[
        "p",
        "--help",
        "--aka_input_file",
        "/nonexistent/v35-cli.dat",
    ]);
    let r = initialize_with(
        "/nonexistent/v35-missing.dat",
        &mut a,
        &inputs(None),
        Communicator::serial(),
    );
    assert!(
        matches!(r, Ok(ref i) if i.outcome == ParseOutcome::HelpRequested),
        "{r:?}"
    );
    // the parsed options are removed as upstream (`--help` itself is kept)
    assert_eq!(a, argv(&["p", "--help"]));
}

/// Nothing after the parse runs on `--help`: the engine is not
/// re-seeded, no input file is read, and the debug level stays at
/// upstream's `dblError` set before the parse (not `--aka_debug_level`).
#[test]
fn vg05_help_stops_before_seed_input_file_and_debug_level() {
    let _g = fresh();
    RandomGenerator::<IdxSlot>::seed(31337);
    let before = format!("{:?}", RandomGenerator::<IdxSlot>::engine_snapshot());
    let dir = std::env::temp_dir();
    let file = dir.join(format!("akantu-rs-{}-v35-help.dat", std::process::id()));
    std::fs::write(&file, "seed = 9\nmarker = 1\n").unwrap();
    let mut a = argv(&["p", "-h", "--aka_debug_level", "12", "--aka_seed", "4"]);
    let r = initialize_with(
        file.to_str().unwrap(),
        &mut a,
        &inputs(Some("5")),
        Communicator::serial(),
    )
    .unwrap();
    std::fs::remove_file(&file).unwrap();
    assert_eq!(r.outcome, ParseOutcome::HelpRequested);
    let after = format!("{:?}", RandomGenerator::<IdxSlot>::engine_snapshot());
    assert_eq!(after, before, "engine re-seeded after --help");
    assert_eq!(RandomGenerator::<IdxSlot>::get_seed(), 31337);
    let rt = global().read().unwrap_or_else(PoisonError::into_inner);
    assert!(
        rt.parser.root.parameter("marker").is_none(),
        "input file read"
    );
    assert_eq!(rt.debug_level(), DEBUG_LEVEL_ERROR);
    assert_eq!(r.debug_level, DEBUG_LEVEL_ERROR);
    // the options were parsed (what the help printer may show)
    assert!(rt.arguments.has("help"));
    assert_eq!(rt.arguments.get_int("aka_debug_level").unwrap(), 12);
    assert_eq!(a, argv(&["p", "-h"]));
}

/// An option the application registered in the global parser
/// before `initialize` is parsed (and removed with `_remove_parsed`) by
/// the same call.
#[test]
fn vg02_pre_registered_application_option_survives_first_initialize() {
    let _g = fresh();
    global()
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .arguments
        .add_argument(
            ArgumentSpec::new("--app", "application option")
                .nargs(Nargs::Exactly(1))
                .arg_type(ArgumentType::String),
        )
        .unwrap();
    let mut a = argv(&["p", "--app", "hello", "--aka_seed", "6"]);
    let r = initialize_with("", &mut a, &inputs(None), Communicator::serial()).unwrap();
    assert_eq!(r.outcome, ParseOutcome::Parsed);
    assert_eq!(r.seed, 6);
    let rt = global().read().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(rt.arguments.get("app"), Some("hello"));
    assert_eq!(a, argv(&["p"]));
}

/// Repeated calls: upstream re-declares the `--aka_*` options in
/// the same parser; `--aka_seed` has no default, so `success_parsed` keeps
/// the first call's value (77) and the second call seeds with it. Options
/// with a default are reset to it (`--aka_debug_level` → 4).
#[test]
fn vg02_second_initialize_keeps_first_aka_seed_upstream() {
    let _g = fresh();
    let mut a = argv(&["p", "--aka_seed", "77", "--aka_debug_level", "9"]);
    let first = initialize_with("", &mut a, &inputs(None), Communicator::serial()).unwrap();
    assert_eq!((first.seed, first.debug_level), (77, 9));
    let mut a = argv(&["p"]);
    let second = initialize_with("", &mut a, &inputs(None), Communicator::serial()).unwrap();
    assert_eq!(second.seed, 77, "upstream keeps the first --aka_seed");
    assert_eq!(second.debug_level, 4, "re-declared default");
    // a new --aka_seed on the second call wins
    let mut a = argv(&["p", "--aka_seed", "3"]);
    let third = initialize_with("", &mut a, &inputs(None), Communicator::serial()).unwrap();
    assert_eq!(third.seed, 3);
}

/// Repeated calls: an application option parsed by the first call
/// stays parsed; giving it again is upstream's "already present" error.
#[test]
fn vg02_second_initialize_rejects_a_repeated_application_option() {
    let _g = fresh();
    global()
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .arguments
        .add_argument(ArgumentSpec::new("--app", "application option"))
        .unwrap();
    let mut a = argv(&["p", "--app", "x"]);
    initialize_with("", &mut a, &inputs(Some("1")), Communicator::serial()).unwrap();
    let mut a = argv(&["p"]);
    initialize_with("", &mut a, &inputs(Some("1")), Communicator::serial()).unwrap();
    assert_eq!(
        global()
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .arguments
            .get("app"),
        Some("x")
    );
    let mut a = argv(&["p", "--app", "y"]);
    let r = initialize_with("", &mut a, &inputs(Some("1")), Communicator::serial());
    assert!(
        r.as_ref()
            .is_err_and(|e| e.to_string().contains("already present")),
        "{r:?}"
    );
}

/// Consistency: `AKA_SEED=""` → glibc `atol` = 0 → seed 0;
/// libstdc++ `minstd_rand0::seed(0)` gives state 1.
#[test]
fn v35_empty_aka_seed() {
    let _g = fresh();
    let mut a = argv(&["p"]);
    let r = initialize_with("", &mut a, &inputs(Some("")), Communicator::serial()).unwrap();
    assert_eq!(r.seed, 0);
    let mut e = MinstdRand0::default();
    e.seed(1);
    let mut g = RandomGenerator::<IdxSlot>::new();
    let ours: Vec<u32> = (0..4).map(|_| g.next_u32()).collect();
    let expect: Vec<u32> = (0..4).map(|_| e.next_u32()).collect();
    assert_eq!(ours, expect);
}
