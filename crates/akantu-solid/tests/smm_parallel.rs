// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (ported intent: the PARALLEL registration of
// test_solid_mechanics_model, test_dof_mesh_distribute.cc): solid mechanics
// on distributed meshes on the local world at 1, 2 and 4 ranks; the same
// scenarios run on real MPI in `mpi_smm.rs`.

mod par_smm;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_smm::{Outcome, Part, Run};

fn on_ranks(f: impl Fn(Communicator) -> Outcome + Sync) {
    for n in [1, 2, 4] {
        let out = run_local(n, Duration::from_secs(120), &f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn explicit_distributed_vs_serial() {
    on_ranks(|c| par_smm::distributed_vs_serial(c, Run::Explicit, 4, 40, Part::Rcb, 1e-12));
}

#[test]
fn static_distributed_vs_serial() {
    on_ranks(|c| par_smm::distributed_vs_serial(c, Run::Static, 3, 1, Part::Rcb, 1e-10));
}

#[test]
fn implicit_dynamic_distributed_vs_serial() {
    on_ranks(|c| par_smm::distributed_vs_serial(c, Run::ImplicitDynamic, 3, 5, Part::Rcb, 1e-10));
}

#[test]
fn test_dof_mesh_distribute() {
    on_ranks(par_smm::dof_mesh_distribute);
}

#[test]
fn model_synchronize_tags() {
    on_ranks(par_smm::model_synchronize_tags);
}

/// Regression tests at 2 and 4 ranks (a 1-rank world has no
/// element synchronizer and no peer to wait).
fn on_ranks_2_4(f: impl Fn(Communicator) -> Outcome + Sync) {
    for n in [2, 4] {
        // a short transport timeout: before the fix the peers of the failing
        // rank timed out (or hung without a timeout)
        let out = run_local(n, Duration::from_secs(20), &f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn pa01_rank_specific_selector_failure_is_agreed() {
    on_ranks_2_4(par_smm::pa01_rank_specific_selector_failure_is_agreed);
}

#[test]
fn pa01_rank_specific_init_material_failure_is_agreed() {
    on_ranks_2_4(par_smm::pa01_rank_specific_init_material_failure_is_agreed);
}

#[test]
fn pa2_01_law_id_exchange_failure_is_agreed_fatal() {
    on_ranks_2_4(par_smm::pa2_01_law_id_exchange_failure_is_agreed_fatal);
}

#[test]
fn pa2_02_instantiation_failure_is_agreed_fatal() {
    on_ranks_2_4(par_smm::pa2_02_instantiation_failure_is_agreed_fatal);
}

/// CL-008 (strengthens MPI-040) and
/// (MPI-034): two laws (one finite-deformation) on a distributed mesh:
/// ghost law = owner's, `_smm_stress` gives the ghosts the owner's PK2,
/// `grad_u` and σ per law (see the scenario's docs).
#[test]
fn two_law_ghosts_take_owner_law_and_internals() {
    on_ranks(par_smm::two_law_ghosts_take_owner_law_and_internals);
}

/// SMM-016 / SMM-027 / MPI-034: an explicit run
/// with a finite-deformation law next to a small-strain one at 1 / 2 / 4
/// ranks against the serial run; the finite-deformation ghost elements'
/// internal forces use the exchanged PK2 and `grad_u` (upstream
/// `assembleInternalForces(_ghost)` after `_smm_stress`).
#[test]
fn explicit_two_laws_finite_deformation_distributed_vs_serial() {
    on_ranks(|c| {
        par_smm::distributed_vs_serial_laws(
            c,
            Run::Explicit,
            4,
            40,
            Part::Rcb,
            1e-12,
            par_smm::Laws::TwoWithFiniteDeformation,
        )
    });
}
