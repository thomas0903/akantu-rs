// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test support (no upstream counterpart): harness for the
// real-MPI acceptance binaries (harness = false, launched by mpirun; see
// docs/TESTING.md and scripts/mpi/run-tests.sh).

#![allow(dead_code)] // shared by several test binaries, each using a subset

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};

/// Per-operation progress deadline (`AKANTU_MPI_TEST_TIMEOUT_S`, default
/// 20 s): a protocol regression surfaces as a transport error, not a hang.
pub fn op_timeout() -> Duration {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&s| s > 0)
        .unwrap_or(20);
    Duration::from_secs(s)
}

/// Start the in-process watchdog: after `secs` (`AKANTU_MPI_WATCHDOG_S`
/// overrides) the process aborts, so mpirun tears the job down — the last
/// line of defence behind the per-operation deadlines and the runner's
/// `timeout`.
pub fn start_watchdog(default_secs: u64) {
    let secs = std::env::var("AKANTU_MPI_WATCHDOG_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_secs);
    std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            eprintln!("WATCHDOG: no completion after {secs} s, aborting");
            std::process::abort();
        })
        .expect("watchdog thread");
}

/// A fresh duplicate of `MPI_COMM_WORLD` (own scope registry and
/// transport-failure state).
pub fn world(level: ThreadLevel, timeout: Option<Duration>, mode: SendMode) -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: level,
        timeout,
        send_mode: mode,
        ..MpiOptions::default()
    })
    .expect("MPI world communicator")
}

/// A fresh duplicate of `MPI_COMM_WORLD` with the given reduction policy.
pub fn world_with_policy(
    level: ThreadLevel,
    timeout: Option<Duration>,
    policy: ReductionPolicy,
) -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: level,
        timeout,
        send_mode: SendMode::Eager,
        reduction: policy,
    })
    .expect("MPI world communicator")
}

/// Scenario outcome.
pub type Outcome = Result<(), String>;

/// Assert inside a scenario (returns `Err` with the location).
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $($fmt:tt)+) => {
        if !$cond {
            return Err(format!("{}:{}: {}", file!(), line!(), format!($($fmt)+)));
        }
    };
    ($cond:expr) => {
        if !$cond {
            return Err(format!("{}:{}: assertion failed: {}", file!(), line!(), stringify!($cond)));
        }
    };
}

/// Runs scenarios in a fixed order on every rank and agrees on the
/// outcome of each one through a control communicator.
pub struct Harness {
    pub control: Communicator,
    pub rank: i32,
    pub size: usize,
    pub failures: Vec<String>,
    pub names: Vec<String>,
}

impl Harness {
    pub fn new(control: Communicator) -> Self {
        let (rank, size) = (control.rank(), control.size());
        Self {
            control,
            rank,
            size,
            failures: Vec::new(),
            names: Vec::new(),
        }
    }

    /// Run one scenario; every rank learns whether it failed anywhere.
    pub fn run(&mut self, name: &str, f: impl FnOnce() -> Outcome) {
        let t0 = Instant::now();
        let r = f();
        let dt = t0.elapsed();
        let mut bad = [u8::from(r.is_err())];
        let agreed = self.control.all_reduce(&mut bad, ReduceOp::Max);
        let status = match (&r, &agreed) {
            (Ok(()), Ok(())) if bad[0] == 0 => "PASS",
            (Ok(()), Ok(())) => "FAIL(remote)",
            _ => "FAIL",
        };
        println!(
            "[rank {}/{}] {status} {name} ({:.1} ms){}",
            self.rank,
            self.size,
            dt.as_secs_f64() * 1e3,
            match &r {
                Err(e) => format!(": {e}"),
                Ok(()) => String::new(),
            }
        );
        if status != "PASS" {
            self.failures.push(name.to_owned());
        }
        if let Err(e) = agreed {
            eprintln!("[rank {}] control agreement failed: {e}", self.rank);
            std::process::abort();
        }
        self.names.push(name.to_owned());
    }

    /// Print the summary; returns the process exit code.
    pub fn finish(self) -> i32 {
        if self.rank == 0 {
            println!(
                "SUMMARY ranks={} scenarios={} failed={} {:?}",
                self.size,
                self.names.len(),
                self.failures.len(),
                self.failures
            );
        }
        i32::from(!self.failures.is_empty())
    }
}
