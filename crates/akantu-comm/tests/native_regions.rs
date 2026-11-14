// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs acceptance scenarios (no upstream counterpart): the native
// library entries of the communicator (`native_collective`,
// `native_ticket`/`native_region`) under real MPI —
// regressions of a region's own thread refusing its own native-policy
// reductions and of `mpi_world()` on a region thread waiting for its own
// registration, and the nested-entry rule under MPI_THREAD_MULTIPLE.

//! Real-MPI scenarios of the native entries, one per process (`harness =
//! false`), launched by `scripts/mpi/run-mpi-tier.sh` as
//!
//! ```text
//! mpirun -np N native_regions <scenario>
//! ```
//!
//! The suffix of a scenario names the MPI thread level. Each rank prints
//! `NATIVE PASS <scenario>` (or `NATIVE FAIL …`) and the job must exit 0.
//! Without an argument the scenarios are listed (exit 0, e.g. under
//! `cargo test`).

mod mpi_common;

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy};
use mpi_common::{start_watchdog, world_with_policy};

const SCENARIOS: &[&str] = &[
    "ticket_native_reduce_serialized",
    "ticket_native_reduce_multiple",
    "ticket_native_reduce_funneled",
    "region_world_serialized",
    "region_world_funneled",
    "nested_native_multiple",
    "nested_native_serialized",
];

fn level_of(name: &str) -> ThreadLevel {
    if name.ends_with("_serialized") {
        ThreadLevel::Serialized
    } else if name.ends_with("_multiple") {
        ThreadLevel::Multiple
    } else {
        ThreadLevel::Funneled
    }
}

fn es(e: akantu_core::Error) -> String {
    format!("{e} (fatal kind {:?})", e.fatal_kind())
}

/// With the `NativeMpi` reduction policy, the thread holding an
/// admitted native ticket agrees its verdict with a native-policy
/// all-reduce, and inside the region runs native-policy reductions and a
/// nested native collective. Under `Serialized` the ticket's split-class
/// registration excluded every other exclusive operation of the process,
/// but before the fix it also refused the ticket thread's own reductions
/// (`Error::Communication` "… 1 committed split(s)").
fn ticket_native_reduce(c: &Communicator) -> Result<(), String> {
    let ticket = c.native_ticket("regions:ticket");
    let mut verdict = [u8::from(ticket.verdict().is_err())];
    c.all_reduce(&mut verdict, ReduceOp::Max)
        .map_err(|e| format!("agreement of the ticket's verdict: {}", es(e)))?;
    if verdict[0] != 0 {
        return Err(format!("ticket refused: {:?}", ticket.verdict()));
    }
    let inner = c
        .native_region(ticket, "regions:region", |nc| {
            let mut v = [1.0_f64 + f64::from(nc.rank())];
            let reduce = c.all_reduce(&mut v, ReduceOp::Sum);
            let nested = c.native_collective("regions:nested", Ok(()), |h| h.size());
            (reduce.map(|()| v[0]), nested)
        })
        .map_err(es)?;
    let n = c.size() as f64;
    match inner {
        (Ok(sum), Ok(size)) if sum == n + n * (n - 1.0) / 2.0 && size == c.size() => Ok(()),
        (r, s) => Err(format!(
            "inside the region: reduction {:?}, nested native collective {:?}",
            r.map_err(es),
            s.map_err(es)
        )),
    }
}

/// `Communicator::mpi_world()` on the thread inside a native region.
/// Under `Serialized` the region's registration counts as a committed
/// split for the creation rule: the adoption is refused at once with an
/// ordinary error (before the fix it waited for its own registration until
/// its deadline, then failed `Fatal{Transport}`). Under `Funneled` there is
/// no rule: every rank adopts the world inside its region.
fn region_world(c: &Communicator, level: ThreadLevel) -> Result<(), String> {
    let ticket = c.native_ticket("regions:world");
    let mut verdict = [u8::from(ticket.verdict().is_err())];
    c.all_reduce(&mut verdict, ReduceOp::Max).map_err(es)?;
    if verdict[0] != 0 {
        return Err(format!("ticket refused: {:?}", ticket.verdict()));
    }
    let (r, dt) = c
        .native_region(ticket, "regions:world-region", |_| {
            let t = Instant::now();
            let r = Communicator::mpi_world(MpiOptions {
                thread_level: level,
                timeout: Some(Duration::from_secs(10)),
                ..MpiOptions::default()
            });
            (r.map(drop), t.elapsed())
        })
        .map_err(es)?;
    println!(
        "INFO region_world rank {}: mpi_world in the region = {:?} after {dt:?}",
        c.rank(),
        r.as_ref().map_err(|e| e.to_string())
    );
    match level {
        ThreadLevel::Serialized => match r {
            Ok(()) => Err("mpi_world admitted inside a native region".into()),
            Err(e) if e.fatal_kind().is_some() => Err(format!("fatal refusal: {}", es(e))),
            Err(_) if dt >= Duration::from_secs(2) => Err(format!(
                "the refusal took {dt:?} (waited for its own registration)"
            )),
            Err(_) => c.barrier().map_err(es),
        },
        _ => r.map_err(es).and_then(|()| c.barrier().map_err(es)),
    }
}

/// Nested native collectives of a region thread are covered by the region's
/// registration (no second admission), also under `Multiple`, where native
/// entries refuse each other process-wide.
fn nested_native(c: &Communicator) -> Result<(), String> {
    let ticket = c.native_ticket("regions:nested-ticket");
    let mut verdict = [u8::from(ticket.verdict().is_err())];
    c.all_reduce(&mut verdict, ReduceOp::Max).map_err(es)?;
    if verdict[0] != 0 {
        return Err(format!("ticket refused: {:?}", ticket.verdict()));
    }
    let r = c
        .native_region(ticket, "regions:nested-region", |_| {
            (0..3)
                .map(|k| {
                    c.native_collective(&format!("regions:nested-{k}"), Ok(()), |h| h.rank())
                        .map_err(es)
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(es)??;
    if r.iter().all(|&x| x == c.rank()) {
        Ok(())
    } else {
        Err(format!("nested ranks {r:?}"))
    }
}

fn main() {
    let arg = std::env::args().nth(1);
    let Some(name) = arg.filter(|a| SCENARIOS.contains(&a.as_str())) else {
        println!("native_regions scenarios:");
        for s in SCENARIOS {
            println!("  {s}");
        }
        return;
    };
    start_watchdog(120);
    let level = level_of(&name);
    let policy = if name.starts_with("ticket_native_reduce") {
        ReductionPolicy::NativeMpi
    } else {
        ReductionPolicy::RankOrdered
    };
    let c = world_with_policy(level, Some(Duration::from_secs(20)), policy);
    let r = c.barrier().map_err(es).and_then(|()| {
        if name.starts_with("ticket_native_reduce") {
            ticket_native_reduce(&c)
        } else if name.starts_with("region_world") {
            region_world(&c, level)
        } else {
            nested_native(&c)
        }
    });
    match &r {
        Ok(()) => println!("NATIVE PASS {name} rank={}", c.rank()),
        Err(e) => println!("NATIVE FAIL {name} rank={}: {e}", c.rank()),
    }
    let code = i32::from(r.is_err());
    drop(c);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(code);
}
