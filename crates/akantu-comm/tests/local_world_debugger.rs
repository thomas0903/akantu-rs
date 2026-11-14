// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): src/common/aka_error.cc
// (Debugger::setParallelContext, one debugger per process) and
// src/common/aka_common.cc (initialize sets the parallel context of the rank).

//! In a local world every rank thread has its own
//! debugger (`run_local` installs `akantu_core::debug::rank_scope`), so
//! each rank's messages carry its own parallel context `<pid>[R<r>|S<n>]`
//! — upstream: one debugger per rank process — while sharing the caller's
//! level, module filter and output; changes made on a rank stay on it.

use std::time::Duration;

use akantu_comm::run_local;
use akantu_core::debug::{self, DebugLevel, Debugger, DebuggerScope};

#[test]
fn local_world_ranks_print_their_own_parallel_context() {
    let mut d = Debugger::new();
    d.set_debug_level(DebugLevel::INFO);
    let out = d.set_out_buffer();
    let _caller = DebuggerScope::new(d);
    let contexts = run_local(2, Duration::from_secs(20), |c| {
        let rank = c.rank();
        // the rank's debugger: the caller's level and output, its own context
        assert_eq!(debug::debug_level(), DebugLevel::INFO);
        akantu_core::aka_debug_info!("hello from rank {}", rank);
        // a change on one rank stays on that rank (its own debugger)
        if rank == 1 {
            debug::set_debug_level(DebugLevel::ERROR);
            akantu_core::aka_debug_info!("hidden on rank 1");
        }
        c.barrier().unwrap();
        akantu_core::aka_debug_warning!("after barrier on rank {}", rank);
        debug::current().parallel_context().to_owned()
    });
    let pid = std::process::id();
    assert_eq!(
        contexts,
        [format!("<{pid}>[R0|S2] "), format!("<{pid}>[R1|S2] ")]
    );
    // the caller's own debugger is unchanged
    assert_eq!(debug::current().parallel_context(), "");
    assert_eq!(debug::debug_level(), DebugLevel::INFO);
    let text = out.take();
    // (context, prefix + message) of every line, in a rank-independent order
    let mut seen: Vec<(String, String)> = text
        .lines()
        .map(|l| {
            let (ctx, rest) = l.split_once(" {").unwrap();
            let (_ts, msg) = rest.split_once("} ").unwrap();
            let msg = msg.rsplit_once(" (").unwrap().0;
            (ctx.to_owned(), msg.to_owned())
        })
        .collect();
    seen.sort_unstable();
    let r0 = format!("<{pid}>[R0|S2]");
    let r1 = format!("<{pid}>[R1|S2]");
    let expected: Vec<(String, String)> = [
        (&r0, "--- hello from rank 0"),
        (&r0, "/!\\ after barrier on rank 0"),
        (&r1, "--- hello from rank 1"),
    ]
    .into_iter()
    .map(|(c, m)| (c.clone(), m.to_owned()))
    .collect();
    assert_eq!(seen, expected, "{text}");
}
