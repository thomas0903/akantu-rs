// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regressions (no upstream counterpart):
// the DOF-count consequence of UD-231 (orphan lower-dimensional elements,
// upstream src/mesh_utils/mesh_partition.cc:284-287) on the local world.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;

use std::time::Duration;

use akantu_comm::run_local;
use akantu_solver::DofManager;

/// A nodal family of 2 components on the
/// adversarial mesh (28 nodes, an orphan point on node 18) distributed on
/// 4 ranks has 56 global DOFs; with upstream's orphan placement (compat
/// switch, and before the fix by default) node 18 has two owners and the
/// system silently has 58.
#[test]
fn pa02_orphan_point_global_dof_count() {
    for (upstream, want) in [(false, 56), (true, 58)] {
        let out = run_local(4, Duration::from_secs(60), move |comm| {
            let m = par_common::adv2d_distributed(&comm, upstream)?;
            let mut dm = DofManager::new("dof", comm.clone());
            dm.register_nodal_dofs(&m, "u", 2)
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(dm.system_size())
        });
        for (r, o) in out.into_iter().enumerate() {
            let n = o.unwrap_or_else(|e| panic!("rank {r}: {e}"));
            assert_eq!(
                u64::try_from(n).unwrap(),
                want,
                "upstream = {upstream}, rank {r}"
            );
        }
    }
}
