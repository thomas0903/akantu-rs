// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// {test_solid_mechanics_model_reassign_material.cc, cube_two_materials.geo,
// two_materials.dat} (registered PARALLEL: 1, 2, 4, … ranks).
//! Port of upstream `test_solid_mechanics_model_reassign_material`
//! (TEST-105): the frozen Gmsh mesh of the upstream `add_mesh` target
//! (`cube_two_materials.msh`, D-009; rank 0 reads, the mesh is
//! distributed), two elastic materials (`two_materials.dat`), the
//! `StraightInterfaceMaterialSelector` (`mat_1` where the barycentre's
//! coordinate `horizontal` is below `pos_interface = 0`, else `mat_2`),
//! `initFull(_static)`; the selector's condition must hold for every
//! (not-ghost) element of the mesh dimension; then the interface is turned
//! (`horizontal = _y`) and `reassignMaterial()` must make the condition
//! hold again (`AKANTU_ERROR` + `EXIT_FAILURE` otherwise).
//!
//! Upstream's selector stores its barycentres through a copy, so they stay
//! uninitialised and every element selects the same material: the upstream
//! test is vacuous (UD-205, category A). The port uses the real barycentres
//! (the test's intent) and guards against vacuity: both materials own
//! elements before and after, and the reassignment moves elements. Ghost
//! elements (distributed runs) are checked as well (stronger than
//! upstream, whose condition loops over the not-ghost elements only;
//! upstream's `reassignConstitutiveLaw` visits both ghost types).
//!
//! Variants: serial (`buildFacets` as upstream, before the model: a
//! mesh-only operation that adds the facet elements, which neither the
//! selector nor the condition sees), and the `PARALLEL` registration on
//! the local world at 1, 2 and 4 ranks (RCB partition instead of upstream's
//! default Scotch: the condition is partition independent; the static
//! solver is the centralised `faer` backend as in `smm_parallel.rs`, no
//! solve happens). Real MPI: the scenario `reassign_scenario::scenario`
//! (shared module) also runs in `mpi_smm.rs` at 1 / 2 / 4 ranks
//! (`scripts/mpi/run-par-tests.sh`).

mod model_common;
mod reassign_scenario;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use reassign_scenario::scenario;

#[test]
fn test_solid_mechanics_model_reassign_material() {
    scenario(Communicator::serial(), true).unwrap();
}

/// The `PARALLEL` registration (upstream: N, N/2, …, 1 ranks) at 1, 2 and
/// 4 ranks of the local world (the same scenario runs on real MPI in
/// `mpi_smm.rs`).
#[test]
fn test_solid_mechanics_model_reassign_material_parallel() {
    for n in [1, 2, 4] {
        let out = run_local(n, Duration::from_secs(120), |c| scenario(c, false));
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}
