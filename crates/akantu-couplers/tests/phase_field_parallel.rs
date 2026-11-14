// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (ported intent: examples/c++/phase_field_model/
// phase_field_parallel.cc, PARALLEL): the solid–phase-field coupler on
// distributed meshes on the local world at 1, 2 and 4 ranks; the same
// scenarios run on real MPI in `mpi_phase_field.rs`.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_phase_field;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_phase_field::Outcome;

fn on_ranks(f: impl Fn(Communicator) -> Outcome + Sync) {
    for n in [1, 2, 4] {
        let out = run_local(n, Duration::from_secs(300), &f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn phase_field_coupler_distributed_vs_serial() {
    // D-082 regression guard: the port's own serial run
    on_ranks(|c| par_phase_field::distributed_vs_serial(c, 8, 4, 2, par_phase_field::ENVELOPE));
}

#[test]
fn phase_field_model_synchronize_tags() {
    on_ranks(par_phase_field::model_synchronize_tags);
}

/// Failure injection: meaningful from two ranks.
fn on_ranks_2_4(f: impl Fn(Communicator) -> Outcome + Sync) {
    for n in [2, 4] {
        let out = run_local(n, Duration::from_secs(120), &f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn pf_selector_failure_is_agreed() {
    on_ranks_2_4(par_phase_field::pf_selector_failure_is_agreed);
}

#[test]
fn pf_init_failures_are_agreed_fatal() {
    on_ranks_2_4(par_phase_field::pf_init_failures_are_agreed_fatal);
}

/// Distributed element events refused.
#[test]
fn pf_distributed_element_events_refused() {
    on_ranks(par_phase_field::pf_distributed_element_events_refused);
}

/// The UD-004 switch.
#[test]
fn ud004_ghost_gradient_switch() {
    on_ranks(par_phase_field::ud004_ghost_gradient_switch);
}

/// UD-905: one convergence answer on every rank.
#[test]
fn check_convergence_is_collective() {
    on_ranks_2_4(par_phase_field::check_convergence_is_collective);
}

#[test]
fn pfm_damage_failure_is_agreed() {
    on_ranks_2_4(par_phase_field::pfm_damage_failure_is_agreed);
}

/// The reduced `phase_field_parallel` against the oracle at one rank
/// (bitwise; upstream's 2- and 4-rank results are run-dependent: next
/// test).
#[test]
fn phase_field_square_matches_the_oracle() {
    assert_eq!(
        par_common::oracle_rank_counts("phase_field_square_n8"),
        [1, 2, 4],
        "oracle fixtures present"
    );
    on_ranks_1(par_phase_field::phase_field_square_matches_oracle);
}

/// D-082: at 2 and 4 ranks, the port against every
/// recorded distinct upstream result under the envelope rule.
#[test]
fn phase_field_square_matches_the_upstream_result_set() {
    for np in [2, 4] {
        assert!(
            !par_phase_field::result_set(np).is_empty(),
            "no recorded upstream result at {np} ranks"
        );
    }
    on_ranks_2_4(par_phase_field::phase_field_square_matches_result_set);
}

fn on_ranks_1(f: impl Fn(Communicator) -> Outcome + Sync) {
    for (r, o) in run_local(1, Duration::from_secs(300), &f)
        .into_iter()
        .enumerate()
    {
        if let Err(e) = o {
            panic!("1 rank, rank {r}: {e}");
        }
    }
}
