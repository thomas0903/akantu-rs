// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: `SolverVector::norm_in` on a communicator split to one rank
// (D-126: nothing is deferred on a one-rank communicator and the norm's
// reduction is issued there too), under both collective-check modes, after a
// deferred outcome, on the in-process local world (the real-MPI binary
// mpi_cohesive runs the same check).

use std::time::Duration;

use akantu_comm::phase::{CollectiveChecks, Phases, Severity};
use akantu_comm::run_local;
use akantu_linalg::SolverVector;

#[test]
fn norm_in_on_a_communicator_split_to_one_rank() {
    let out = run_local(2, Duration::from_secs(10), |c| {
        let solo = c
            .split(Some(u32::try_from(c.rank()).unwrap()), 0)
            .unwrap()
            .unwrap();
        assert_eq!(solo.size(), 1);
        let mut norms = Vec::new();
        for mode in [CollectiveChecks::EveryPhase, CollectiveChecks::CommitPoints] {
            solo.set_collective_checks(mode);
            let mut v = SolverVector::new("v", false);
            v.resize(4);
            v.set(2.0);
            let mut ph = Phases::settling(&solo, "norm");
            ph.defer("local", Ok(()), Severity::Recoverable).unwrap();
            let n = v.norm_in(&mut ph, "norm", &[true, true, true, false]);
            norms.push(ph.settle(n).unwrap());
        }
        norms
    });
    for norms in out {
        for n in norms {
            assert_eq!(n.to_bits(), 12.0_f64.sqrt().to_bits());
        }
    }
}
