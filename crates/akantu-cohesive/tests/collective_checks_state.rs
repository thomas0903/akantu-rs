// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the persistent state the collective error protocol leaves
// after a failure (D-126) on the distributed extrinsic cohesive scenario of
// oracle/probes/parallel_cohesive, on the in-process local world (the
// real-MPI binary mpi_cohesive.rs runs the same check).
//
// When an operation returns an error, every rank's persistent state (the
// DOF arrays, the lumped mass, the laws' internal variables, the mesh) must
// be the state the every-phase mode leaves on that rank for the same
// injected failure: under the commit-points mode the ranks that learn of a
// peer's failure later may run further, but they may change transient data
// only (residual, forces, buffers). The explicit solve step is checked with
// a failure injected in each of its phases in turn (every agreement,
// deferred outcome and collective of the step, as recorded by a run without
// failure), at 2 and 4 ranks, on the first and on the last rank, at the
// step that inserts the first cohesive elements and two steps later.

mod par_coh;

use akantu_comm::run_local;
use par_coh::checks;

fn check(np: usize) {
    for offset in [0, 2] {
        let out = run_local(np, par_coh::local_timeout(), |comm| {
            let rank = usize::try_from(comm.rank()).unwrap();
            checks::check_states(
                rank,
                &[0, np - 1],
                &mut |mode, inject, failing| {
                    checks::state_run(&comm, offset, mode, inject, failing)
                },
                &format!("{np} ranks, step +{offset}"),
            )
        });
        for (rank, r) in out.into_iter().enumerate() {
            let n = r.unwrap_or_else(|e| panic!("{e}"));
            if rank == 0 {
                eprintln!("{np} ranks, step +{offset}: {n} injected failures compared");
            }
        }
    }
}

/// The explicit solve step at 2 ranks.
#[test]
fn solve_step_failure_leaves_the_every_phase_state_np2() {
    check(2);
}

/// The explicit solve step at 4 ranks.
#[test]
fn solve_step_failure_leaves_the_every_phase_state_np4() {
    check(4);
}
