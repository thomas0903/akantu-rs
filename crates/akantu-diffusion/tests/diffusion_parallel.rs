// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests
// (DIFF-010): the heat-transfer model on distributed meshes on the local
// world at 1, 2 and 4 ranks; the same scenarios run on real MPI in
// `mpi_diffusion.rs` (scripts/mpi/run-par-tests.sh).

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_diffusion;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_diffusion::{Outcome, Run};

fn on_ranks(ranks: &[usize], f: impl Fn(Communicator) -> Outcome + Sync) {
    for &n in ranks {
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
    on_ranks(&[1, 2, 4], |c| {
        par_diffusion::distributed_vs_serial(c, Run::Explicit, 4, 30, 1e-13)
    });
}

#[test]
fn static_distributed_vs_serial() {
    on_ranks(&[1, 2, 4], |c| {
        par_diffusion::distributed_vs_serial(c, Run::Static, 3, 1, 1e-14)
    });
}

#[test]
fn implicit_distributed_vs_serial() {
    on_ranks(&[1, 2, 4], |c| {
        par_diffusion::distributed_vs_serial(c, Run::Implicit, 3, 5, 1e-12)
    });
}

#[test]
fn model_synchronize_tags() {
    on_ranks(&[1, 2, 4], par_diffusion::model_synchronize_tags);
}

#[test]
fn ud275_compat_reproduces_upstream_failure() {
    on_ranks(
        &[1, 2, 4],
        par_diffusion::ud275_compat_reproduces_upstream_failure,
    );
}

#[test]
fn rank_specific_selector_failure_is_agreed() {
    on_ranks(
        &[2, 4],
        par_diffusion::rank_specific_selector_failure_is_agreed,
    );
}

#[test]
fn rank_specific_init_failure_is_agreed_fatal() {
    on_ranks(
        &[2, 4],
        par_diffusion::rank_specific_init_failure_is_agreed_fatal,
    );
}

/// Local and master nodes count once.
#[test]
fn thermal_energy_by_node_counts_each_node_once() {
    on_ranks(
        &[1, 2, 4],
        par_diffusion::thermal_energy_by_node_counts_each_node_once,
    );
}

/// MPI-043: `_diffusion_gradient` against the oracle at 1, 2,
/// 4 and 8 ranks (fixtures/parallel/diffusion_gradient_cube_n3; np8 recorded
/// later).
#[test]
fn diffusion_gradient_matches_the_oracle() {
    assert_eq!(
        par_common::oracle_rank_counts("diffusion_gradient_cube_n3"),
        [1, 2, 4, 8],
        "oracle fixtures present"
    );
    on_ranks(
        &[1, 2, 4, 8],
        par_diffusion::diffusion_gradient_matches_oracle,
    );
}
