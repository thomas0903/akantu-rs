// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential test of
// `akantu::runtime::initialize` against fixtures/core/initialize (written by
// oracle/probes/cpp/init_probe.cc under mpirun from upstream
// `akantu::initialize`, src/common/aka_common.cc:62-125).

//! `core::initialize_matches_oracle` (CORE-031): for every
//! recorded scenario (AKA_SEED with glibc `atol` parsing, `--aka_seed`,
//! input-file `seed` with C++ stream conversion, `--aka_input_file`
//! precedence, root-scope-only lookup, time fallback, `long` overflow of the
//! rank factor, debug-level options) and every rank of 1- and 2-rank runs,
//! [`initialize_with`] gives the oracle's seed, the oracle's next raw values
//! of `RandomGenerator<Idx>` and `RandomGenerator<UInt>`, the oracle's
//! remaining `argv` and debug level — all exact. Every rank is replayed in
//! this process with the rank's communicator handle; the time-fallback
//! scenarios are replayed with the oracle's clock (a time inside the
//! recorded `[time_before, time_after]` window that reproduces the seed
//! must exist).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use akantu::comm::Communicator;
use akantu::core::random::{IdxSlot, RandomGenerator, UIntSlot};
use akantu::runtime::{ProcessInputs, Runtime, global, initialize_with};
use akantu_validation::Fixture;
use akantu_validation::fixture::fixtures_root;

/// Input files of oracle/probes/python/init_cases.py (checked against the
/// stored copy of the file given to `initialize`).
const INPUTS: [(&str, &str); 7] = [
    ("seed99.dat", "seed = 99\n"),
    ("seed123.dat", "seed = 123\n"),
    ("seed456.dat", "seed = 456\n"),
    ("seed_float.dat", "seed = 12.7\n"),
    ("seed_expr.dat", "seed = 2*3\n"),
    ("seed_scoped.dat", "user section [\n  seed = 5\n]\n"),
    ("seed_negative.dat", "seed = -40\n"),
];

/// The tests share the process-global runtime and random engines.
static GLOBAL: Mutex<()> = Mutex::new(());

/// Each oracle case is a fresh process: start every replayed run from a
/// fresh runtime (`initialize` keeps the global argument parser between
/// calls, as upstream's `static_argparser`).
fn fresh_runtime() {
    *global().write().unwrap_or_else(PoisonError::into_inner) = Runtime::default();
}

const SCENARIOS: [&str; 12] = [
    "env",
    "env_atol",
    "cli",
    "file",
    "file_negative",
    "cli_input_file",
    "file_float",
    "file_expr",
    "file_scoped",
    "time",
    "overflow",
    "debug_level",
];

fn strings(v: &serde_json::Value) -> Vec<String> {
    v.as_array()
        .expect("string array")
        .iter()
        .map(|s| s.as_str().expect("string").to_owned())
        .collect()
}

fn int(v: &serde_json::Value) -> i64 {
    v.as_i64().expect("integer")
}

struct Case {
    id: String,
    fixture: Fixture,
}

fn cases() -> Vec<Case> {
    let dir = fixtures_root().join("core/initialize");
    let mut ids: Vec<String> = std::fs::read_dir(&dir)
        .expect("fixtures/core/initialize")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && n != "tolerances.json")
        .collect();
    ids.sort();
    ids.into_iter()
        .map(|n| {
            let id = format!("core/initialize/{n}");
            let fixture = Fixture::open_by_id(&id).unwrap();
            fixture.verify_checksums().unwrap();
            Case { id, fixture }
        })
        .collect()
}

#[test]
fn initialize_matches_oracle() {
    let _guard = GLOBAL.lock().unwrap_or_else(PoisonError::into_inner);
    let work = std::env::temp_dir().join(format!("akantu-rs-init-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let inputs: BTreeMap<&str, &str> = INPUTS.into_iter().collect();
    for (name, text) in &inputs {
        std::fs::write(work.join(name), text).unwrap();
    }
    let abs = |a: &str| -> String {
        if inputs.contains_key(a) {
            work.join(a).to_string_lossy().into_owned()
        } else {
            a.to_owned()
        }
    };
    let rel = |a: &str| -> String {
        Path::new(a)
            .strip_prefix(&work)
            .map_or_else(|_| a.to_owned(), |p| p.to_string_lossy().into_owned())
    };
    let world2 = Communicator::local_world(2, Duration::from_secs(5));

    let cases = cases();
    let mut seen = BTreeMap::new();
    for case in &cases {
        let p = &case.fixture.manifest().parameters;
        let scenario = p["scenario"].as_str().unwrap().to_owned();
        let (rank, size) = (int(&p["rank"]), int(&p["size"]));
        *seen.entry((scenario.clone(), size)).or_insert(0) += 1;
        let input_file = p["input_file"].as_str().unwrap();
        if !input_file.is_empty() {
            let stored: PathBuf = case.fixture.input_path("input_file").unwrap();
            assert_eq!(
                std::fs::read_to_string(stored).unwrap(),
                inputs[input_file],
                "{}: input file differs from init_cases.py",
                case.id
            );
        }
        let communicator = match size {
            1 => Communicator::serial(),
            2 => world2[usize::try_from(rank).unwrap()].clone(),
            _ => panic!("{}: unexpected size {size}", case.id),
        };
        let env_seed = p
            .get("env_aka_seed")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let (t0, t1) = (int(&p["time_before"]), int(&p["time_after"]));
        let oracle_seed = case.fixture.load_i64("seed").unwrap().data[0];
        // replay with the oracle's clock: the first time of the recorded
        // window that reproduces the seed (every time works when the seed
        // does not come from the clock)
        let mut replayed = None;
        for t in t0..=t1 {
            let mut argv: Vec<String> = strings(&p["argv"]).iter().map(|a| abs(a)).collect();
            let inputs = ProcessInputs {
                env_aka_seed: env_seed.clone(),
                env_aka_debug_level: None,
                time_now: t,
            };
            fresh_runtime();
            let init = initialize_with(&abs(input_file), &mut argv, &inputs, communicator.clone())
                .unwrap_or_else(|e| panic!("{}: {e}", case.id));
            if init.seed == oracle_seed {
                replayed = Some((init, argv));
                break;
            }
        }
        let (init, argv) = replayed.unwrap_or_else(|| {
            panic!(
                "{}: no time in [{t0}, {t1}] reproduces the oracle seed {oracle_seed}",
                case.id
            )
        });
        assert_eq!(i64::from(init.rank), rank, "{}", case.id);
        assert_eq!(
            init.debug_level,
            int(&p["debug_level"]),
            "{}: debug level",
            case.id
        );
        let remaining: Vec<String> = argv.iter().map(|a| rel(a)).collect();
        assert_eq!(
            remaining,
            strings(&p["remaining_argv"]),
            "{}: argv",
            case.id
        );
        let mut g = RandomGenerator::<IdxSlot>::new();
        let draws: Vec<i64> = (0..6).map(|_| i64::from(g.next_u32())).collect();
        assert_eq!(
            draws,
            case.fixture.load_i64("draws/idx").unwrap().data,
            "{}: RandomGenerator<Idx> draws",
            case.id
        );
        // RandomGenerator<UInt> is never seeded by initialize; each oracle
        // case is a fresh process, so restore its static default first
        RandomGenerator::<UIntSlot>::seed(5489);
        let mut gu = RandomGenerator::<UIntSlot>::new();
        let draws_u: Vec<i64> = (0..2).map(|_| i64::from(gu.next_u32())).collect();
        assert_eq!(
            draws_u,
            case.fixture.load_i64("draws/uint").unwrap().data,
            "{}: RandomGenerator<UInt> draws",
            case.id
        );
        eprintln!(
            "{}: seed {} (rank {rank}/{size}) reproduced",
            case.id, init.seed
        );
    }
    for s in SCENARIOS {
        assert_eq!(
            seen.get(&(s.to_owned(), 1)),
            Some(&1),
            "scenario {s}, 1 rank"
        );
        assert_eq!(
            seen.get(&(s.to_owned(), 2)),
            Some(&2),
            "scenario {s}, 2 ranks"
        );
    }
    assert_eq!(cases.len(), 3 * SCENARIOS.len());
    std::fs::remove_dir_all(&work).unwrap();
}

/// `AKA_DEBUG_LEVEL` overrides `--aka_debug_level` with glibc `atoi`
/// (upstream reads it after the option, `aka_common.cc:113-117`).
#[test]
fn env_debug_level_overrides_the_option() {
    let _guard = GLOBAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut argv = vec![
        "prog".to_owned(),
        "--aka_debug_level".to_owned(),
        "7".to_owned(),
    ];
    let inputs = ProcessInputs {
        env_aka_seed: Some("3".to_owned()),
        env_aka_debug_level: Some(" 12abc".to_owned()),
        time_now: 0,
    };
    fresh_runtime();
    let init = initialize_with("", &mut argv, &inputs, Communicator::serial()).unwrap();
    assert_eq!((init.seed, init.debug_level), (3, 12));
    assert_eq!(argv, vec!["prog".to_owned()]);
    assert_eq!(akantu::runtime::global().read().unwrap().debug_level(), 12);
}
