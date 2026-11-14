// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (no direct upstream counterpart): law-specific
// exchanges of the solid model on distributed meshes (MPI-041
// `_clh_init_cl`, MPI-053 tag liveness / user extension tags) on the local
// world at 1, 2 and 4 ranks; the same scenarios run on real MPI in
// `mpi_smm.rs` (scripts/mpi/run-par-tests.sh).

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_laws;
mod par_smm;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_smm::Outcome;

/// Every test of this binary holds `par_laws::RNG_LOCK`: the rank threads
/// of concurrent tests share the process-global random generator, and a
/// draw (or a seed) of one test between the seeded draws of
/// `clh_marigo_matches_the_oracle` changes its `Yd` (observed: 2 failures
/// in 80 runs of the binary at 9 test threads, before this lock).
fn on_ranks(f: impl Fn(Communicator) -> Outcome + Sync) {
    let _rng = par_laws::RNG_LOCK.lock();
    for n in [1, 2, 4] {
        let out = run_local(n, Duration::from_secs(180), &f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn marigo_distributed_vs_serial() {
    on_ranks(par_laws::marigo_distributed_vs_serial);
}

#[test]
fn marigo_run_damages() {
    on_ranks(par_laws::marigo_run_damages);
}

#[test]
fn clh_init_cl_exchange() {
    on_ranks(par_laws::clh_init_cl_exchange);
}

/// One rank only in the local world: the rank threads share the process's
/// random generator (2 and 4 ranks run under real MPI, `mpi_smm`).
#[test]
fn clh_marigo_matches_the_oracle() {
    let _rng = par_laws::RNG_LOCK.lock();
    assert_eq!(
        par_common::oracle_rank_counts("clh_marigo_cube_n3"),
        [1, 2, 4],
        "oracle fixtures present"
    );
    for (r, o) in run_local(
        1,
        Duration::from_secs(180),
        par_laws::clh_marigo_matches_oracle,
    )
    .into_iter()
    .enumerate()
    {
        if let Err(e) = o {
            panic!("1 rank, rank {r}: {e}");
        }
    }
}

#[test]
fn extension_tag_dispatch() {
    on_ranks(par_laws::extension_tag_dispatch);
}

/// Law data for a tag without law data refused at init.
#[test]
fn law_data_for_a_generic_tag_is_refused() {
    on_ranks(par_laws::law_data_for_a_generic_tag_is_refused);
}

#[test]
fn phasefield_degrade_mass_lumped_vs_serial() {
    on_ranks(par_laws::phasefield_degrade_mass_lumped_vs_serial);
}

/// Failure injection: meaningful from two ranks.
fn on_ranks_2_4(f: impl Fn(Communicator) -> Outcome + Sync) {
    let _rng = par_laws::RNG_LOCK.lock();
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
fn clh_init_cl_failure_is_agreed_fatal() {
    on_ranks_2_4(par_laws::clh_init_cl_failure_is_agreed_fatal);
}

/// `_smm_density` carries `grad_u` too.
#[test]
fn smm_density_carries_damage_and_grad_u() {
    on_ranks(par_laws::smm_density_carries_damage_and_grad_u);
}

#[test]
fn smm_density_failure_is_agreed() {
    on_ranks_2_4(par_laws::smm_density_failure_is_agreed);
}
