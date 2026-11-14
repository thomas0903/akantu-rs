// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential tests (no upstream counterpart): the
// distributed facet meshes, grid synchronizers and periodic synchronizers of
// the port equal the oracle's rank by rank — fixtures/parallel/{facets_*,
// grid_*, periodic_*}/np{1,2,4}/rank* (oracle/probes/parallel, probe version 2;
// grid_* and periodic_* regenerated with probe version 3: bounding boxes,
// multimap order, non-dyadic sums) and the adversarial scenarios
// fixtures/parallel/v101_*/np{1,2,4}/rank* (probe version 3);
// periodic_* and v101_periodic_* regenerated with probe version 4 (per-rank
// contributions and exact run-dependent mask of the non-dyadic `_sum`,
// D-039 amendment).

mod par2a_common;
mod par_common;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_common::Outcome;

/// `f` at every rank count the probe recorded for `scenario`, which must
/// be exactly `expected` (1, 2 and 4; 8 where recorded):
/// a missing or removed fixture fails here.
fn on_oracle_ranks(scenario: &str, expected: &[usize], f: fn(Communicator) -> Outcome) {
    let counts = par_common::oracle_rank_counts(scenario);
    assert_eq!(counts, expected, "oracle fixtures of {scenario}");
    on_counts(scenario, &counts, f);
}

fn on_counts(scenario: &str, counts: &[usize], f: fn(Communicator) -> Outcome) {
    for &np in counts {
        let out = run_local(np, Duration::from_secs(60), f);
        let errors: Vec<String> = out
            .into_iter()
            .enumerate()
            .filter_map(|(r, o)| o.err().map(|e| format!("rank {r}: {e}")))
            .collect();
        assert!(
            errors.is_empty(),
            "{scenario}: np {np}:\n{}",
            errors.join("\n")
        );
    }
}

#[test]
fn facets_cube_msh_matches_the_oracle() {
    on_oracle_ranks(
        "facets_cube_msh",
        &[1, 2, 4],
        par2a_common::facets_cube_msh_matches_oracle,
    );
}

#[test]
fn facets_cube_n4_matches_the_oracle() {
    on_oracle_ranks(
        "facets_cube_n4",
        &[1, 2, 4, 8],
        par2a_common::facets_cube_n4_matches_oracle,
    );
}

#[test]
fn facets_adv2d_matches_the_oracle() {
    on_oracle_ranks(
        "facets_adv2d",
        &[1, 2, 4],
        par2a_common::facets_adv2d_matches_oracle,
    );
}

#[test]
fn grid_tri_n8_matches_the_oracle() {
    on_oracle_ranks(
        "grid_tri_n8",
        &[1, 2, 4],
        par2a_common::grid_tri_n8_matches_oracle,
    );
}

#[test]
fn grid_cube_n4_matches_the_oracle() {
    on_oracle_ranks(
        "grid_cube_n4",
        &[1, 2, 4, 8],
        par2a_common::grid_cube_n4_matches_oracle,
    );
}

#[test]
fn periodic_matches_the_oracle() {
    // every case at 1, 2 and 4 ranks; periodic_3d_xyz also at 8,
    // where the scenario replays the cases recorded there
    let mut counts = vec![1, 2, 4];
    for (dim, pdim) in par2a_common::PERIODIC_TYPES {
        let name = par2a_common::periodic_scenario_name(dim, pdim);
        let expected: &[usize] = if name == "periodic_3d_xyz" {
            &[1, 2, 4, 8]
        } else {
            &[1, 2, 4]
        };
        let c = par_common::oracle_rank_counts(&name);
        assert_eq!(c, expected, "oracle fixtures of {name}");
        if c.contains(&8) && !counts.contains(&8) {
            counts.push(8);
        }
    }
    assert!(counts.contains(&8), "periodic_3d_xyz recorded at 8 ranks");
    on_counts("periodic_*", &counts, par2a_common::periodic_matches_oracle);
}

#[test]
fn v101_facets_tri_quadrants_matches_the_oracle() {
    on_oracle_ranks(
        "v101_facets_tri_quadrants",
        &[1, 2, 4],
        par2a_common::v101_facets_tri_quadrants_matches_oracle,
    );
}

#[test]
fn v101_facets_tet_quadrants_matches_the_oracle() {
    on_oracle_ranks(
        "v101_facets_tet_quadrants",
        &[1, 2, 4],
        par2a_common::v101_facets_tet_quadrants_matches_oracle,
    );
}

#[test]
fn v101_grid_wide_matches_the_oracle() {
    on_oracle_ranks(
        "v101_grid_wide",
        &[1, 2, 4],
        par2a_common::v101_grid_wide_matches_oracle,
    );
}

#[test]
fn v101_grid_adv2d_matches_the_oracle() {
    on_oracle_ranks(
        "v101_grid_adv2d",
        &[1, 2, 4],
        par2a_common::v101_grid_adv2d_matches_oracle,
    );
}

#[test]
fn v101_periodic_matches_the_oracle() {
    for (dim, ndir, _) in par2a_common::V101_PERIODIC {
        let name = format!("v101_periodic_{dim}d_{}", &"xyz"[..ndir]);
        assert_eq!(
            par_common::oracle_rank_counts(&name),
            vec![1, 2, 4],
            "{name} present (parity partitions of 1, 2 and 4 ranks)"
        );
    }
    on_oracle_ranks(
        "v101_periodic_2d_x",
        &[1, 2, 4],
        par2a_common::v101_periodic_matches_oracle,
    );
}
